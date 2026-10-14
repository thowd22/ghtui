//! Explicit positive choice through the SAME bridge/attempt kernel. No query
//! widening, first-row selection, serialized authority or new auth stack.
use super::advance_v2::{close_advance_v2, SelectionPolicyV2};
use super::choices_v1::{attempt_jobs_v1, retain_candidates_v1};
use super::*;

impl GithubProvider {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn pr_check_selection_impl_v2(
        &self,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        resource: &PrResourceReadV1,
        index: usize,
        expected: &Arc<PrCheckRunObservation>,
        choice: &PrCheckChoiceV1,
        limits: &PrCheckRunReadLimits,
        cancel: &CancellationToken,
        mut guard: impl FnMut(PrCheckSelectedReadPhaseV1) -> GithubResult<()>,
        policy: SelectionPolicyV2,
    ) -> GithubResult<PrCheckRefreshReadV2> {
        let selected = limits.selected()?;
        if !choice.belongs_to(expected)
            || !self.runner.same_instance(&expected.producer)
            || !expected.matches_resource_v1(identity, resource, index)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Original opaque choice/resource/producer differs; no selected read launched",
            ));
        }
        let wanted = choice.run()?;
        let wanted_job = choice.job()?;
        let binding = ScopedPrCheckBindingV1::new(current, resource, index)?;
        let (summary, check_dto) = binding.summary_and_check();
        let original = PrCheckOriginal::new(self, identity, &summary, &check_dto)?;
        let mut budget = MetricsBudget::new(&selected)?;
        let who = principal(self, &mut budget, identity, cancel)?;
        let repo_path = format!("repos/{}", original.repository);
        let parent_path = format!("{repo_path}/pulls/{}", original.number);
        let check_path = format!("{repo_path}/check-runs/{}", original.check_id);
        let raw = budget.read_document(self, &repo_path, cancel)?;
        let repo = current_repo(self, &original, &raw)?;
        binding.repository(&raw)?;
        // The genuine current catalogue row and original resource binding above
        // already constrain the owner and raw repository. A leaf rename across
        // observations changes presentation, not original numeric membership.
        if expected.repository.host != repo.host
            || expected.repository.id != repo.id
            || expected.repository.node_id != repo.node_id
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original repository host/database/node changed before selected read",
            ));
        }
        let raw = budget.read_document(self, &parent_path, cancel)?;
        let pr_id = current_parent(self, &original, &repo, &raw)?;
        binding.parent(self, &raw)?;
        let raw = budget.read_document(self, &check_path, cancel)?;
        let check = check_proof(self, &original, &raw)?;
        binding.check(&raw)?;
        if check.node != expected.check_node || check.suite != expected.suite_id {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Original check/suite choice changed; no retarget",
            ));
        }
        let raw = budget.read_document(
            self,
            &format!("{repo_path}/actions/runs/{}", wanted.run.id),
            cancel,
        )?;
        if matches!(policy, SelectionPolicyV2::RefreshExactCurrent)
            && raw["run_attempt"]
                .as_u64()
                .is_some_and(|attempt| attempt > wanted.run.attempt)
        {
            let advanced = close_advance_v2(
                self,
                &binding,
                &original,
                &repo,
                &check,
                pr_id,
                &wanted,
                &raw,
                expected,
                &mut budget,
                cancel,
                &mut guard,
            )?;
            return Ok(PrCheckRefreshReadV2::AttemptAdvanced(Box::new(advanced)));
        }
        let run = observed_run(self, &original, &repo, &check, &raw)?;
        // Explicitly observed attempt is immutable navigation intent. Advancing
        // it silently would be a different action; historical mode is separate.
        same_run(&wanted, &run)?;
        let mut rows = 0;
        let mut member_missing = false;
        let mut first_guard = Ok(());
        let read = attempt_jobs_v1(
            self,
            &original,
            &repo,
            &check,
            &run,
            &mut budget,
            limits,
            &mut rows,
            cancel,
            true,
            |members| {
                if let Some(wanted) = wanted_job {
                    match members.iter().find(|member| member.id() == wanted.id()) {
                        Some(found) if found.node() == wanted.node() => {}
                        Some(_) => {
                            return Err(error(
                                GithubErrorKind::IdentityChanged,
                                "Genuine chosen job node changed; no numeric alias",
                            ))
                        }
                        None => {
                            member_missing = true;
                            return Ok(());
                        }
                    }
                }
                // A stable-principal local failure must not expose diagnostics
                // or avoid the real closing source/account reads. Cancellation
                // and the original elapsed/capture budget still stop children.
                first_guard = guard(PrCheckSelectedReadPhaseV1::BeforeSelectedProjection);
                Ok(())
            },
        )?;
        let raw = budget.read_document(self, &check_path, cancel)?;
        let final_check = check_proof(self, &original, &raw)?;
        binding.check(&raw)?;
        if check != final_check {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Original check changed while choosing run",
            ));
        }
        let raw = budget.read_document(self, &parent_path, cancel)?;
        let final_pr = current_parent(self, &original, &repo, &raw)?;
        binding.parent(self, &raw)?;
        if final_pr != pr_id {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original PR database identity changed during choice",
            ));
        }
        let local = first_guard.and_then(|()| {
            budget.pr_check_guard(cancel, || {
                guard(PrCheckSelectedReadPhaseV1::BeforeFinalProof)
            })
        });
        let raw = budget.read_document(self, &repo_path, cancel)?;
        let final_repo = current_repo(self, &original, &raw)?;
        binding.repository(&raw)?;
        require_same_repo(&repo, &final_repo)?;
        let who = principal(self, &mut budget, &who, cancel)?;
        local.map_err(|failure| {
            error(
                failure.kind,
                "Original selected-read local guard unavailable; private results hidden",
            )
        })?;
        let selected_job = if member_missing {
            None
        } else {
            wanted_job.map(|job| job.id())
        };
        let selection = if wanted_job.is_some() {
            PrCheckSelectionOriginV1::ExplicitJob
        } else {
            PrCheckSelectionOriginV1::ExplicitRun
        };
        let selected_run = (!member_missing).then(|| {
            Arc::new(PrCheckSelectedRunV1 {
                attempt: read.attempt.clone(),
                run_node: read.run_node.clone(),
                members: read.members,
                selected_job,
                selection,
            })
        });
        let mapped = if let Some(id) = selected_job {
            read.matched.contains(&id).then_some(id)
        } else if read.coverage.enumeration_complete {
            match read.matched.as_slice() {
                [id] => Some(*id),
                _ => None,
            }
        } else {
            None
        };
        let link = mapped.map(|id| PrCheckRunLink {
            attempt: read.attempt.clone(),
            selected_job: id,
            sources: read.sources,
            run_node: read.run_node.clone(),
        });
        let unavailable = member_missing.then_some(if read.coverage.enumeration_complete {
            PrCheckBridgeUnavailable::NoCorrelatedJobObserved
        } else {
            PrCheckBridgeUnavailable::PartialJobCatalogue
        });
        let choice_unavailable = member_missing.then_some(if read.coverage.enumeration_complete {
            PrCheckChoiceUnavailableV1::SelectedMemberNotObserved
        } else {
            PrCheckChoiceUnavailableV1::PartialMemberCatalogue
        });
        let run_candidates = retain_candidates_v1(&BTreeMap::from([(run.run.id, run)]), limits)?;
        Ok(PrCheckRefreshReadV2::Current(Box::new(
            PrCheckRunObservation {
                identity: who,
                original,
                repository: final_repo,
                pr_database_id: final_pr,
                check_node: final_check.node,
                suite_id: final_check.suite,
                link,
                unavailable,
                run_coverage: PrCheckBridgePageCoverage {
                    admitted_rows: 1,
                    raw_rows: 1,
                    total_matches: Some(1),
                    enumeration_complete: false,
                    reasons: vec![
                        "One positively selected run only; suite catalogue not enumerated".into(),
                    ],
                    ..Default::default()
                },
                job_coverage: read.coverage,
                limits: limits.clone(),
                requests: budget.requests,
                stdout_bytes: budget.bytes,
                diagnostic_bytes: budget.successful_diagnostic_bytes,
                failed_reserved_bytes: budget.failed_reserved_bytes,
                observed_from: budget.observed_from,
                observed_through: budget.observed_through()?,
                producer: self.runner.clone(),
                run_candidates,
                selected_run,
                choice_unavailable,
            },
        )))
    }
}

impl GithubProvider {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pr_check_refresh_from_resource_with_guard_v2(
        &self,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        resource: &PrResourceReadV1,
        index: usize,
        expected: &Arc<PrCheckRunObservation>,
        choice: &PrCheckChoiceV1,
        limits: &PrCheckRunReadLimits,
        cancel: &CancellationToken,
        guard: impl FnMut(PrCheckSelectedReadPhaseV1) -> GithubResult<()>,
    ) -> GithubResult<PrCheckRefreshReadV2> {
        self.pr_check_selection_impl_v2(
            identity,
            current,
            resource,
            index,
            expected,
            choice,
            limits,
            cancel,
            guard,
            SelectionPolicyV2::RefreshExactCurrent,
        )
    }
}
