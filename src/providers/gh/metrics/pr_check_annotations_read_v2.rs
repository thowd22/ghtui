//! Shared actual member kernel + fresh canonical CheckRun before private SQL.
use super::super::member_v2::*;
use super::*;

impl GithubProvider {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pr_check_annotations_from_resource_with_guard_v2(
        &self,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        resource: &PrResourceReadV1,
        index: usize,
        origin: Arc<PrCheckRunObservation>,
        choice: &PrCheckChoiceV1,
        limits: &PrCheckAnnotationLimitsV2,
        cancel: &CancellationToken,
        mut guard: impl FnMut(PrCheckAnnotationPhaseV2) -> GithubResult<()>,
    ) -> GithubResult<PrCheckAnnotationsReadV2> {
        let selected = limits.selected()?;
        let mut budget = MetricsBudget::new(&selected)?;
        let (binding, opened) = open_member_v2(
            self,
            identity,
            current,
            resource,
            index,
            &origin,
            choice,
            &mut budget,
            &limits.transport,
            cancel,
        )?;
        let mut coverage = PrCheckAnnotationCoverageV2::default();
        let mut rows = vec![];
        let mut check_id = None;
        let mut check_node = None;
        let mut opening_check = None;
        let mut first_guard = Ok(());
        if let Some(job) = &opened.job {
            match check_selector(self, &opened.original.repository, &job.check_run_url) {
                Err(e) => {
                    coverage.failure = Some(e.kind);
                    coverage.reasons.push(
                        "Actual job has no supported canonical annotation source; nothing guessed"
                            .into(),
                    );
                }
                Ok(id) => {
                    check_id = Some(id);
                    let path = format!("repos/{}/check-runs/{id}", opened.original.repository);
                    match budget.read_document(self, &path, cancel) {
                        Err(e)
                            if matches!(
                                e.kind,
                                GithubErrorKind::Permission | GithubErrorKind::NotFound
                            ) =>
                        {
                            coverage.failure = Some(e.kind);
                            coverage.reasons.push(
                                "Check endpoint unavailable; no annotations permission inferred"
                                    .into(),
                            );
                        }
                        Err(e) => return Err(e),
                        Ok(raw) => {
                            let observed = annotation_check(self, &opened.original, id, &raw)?;
                            if observed.suite != opened.check.suite
                                || id == origin.original.check_id
                                    && observed.node != origin.check_node
                            {
                                return Err(error(GithubErrorKind::IdentityChanged,"Actual member check suite or genuine selected check node changed before annotation SQL"));
                            }
                            check_node = Some(observed.node.clone());
                            let count = raw["output"]["annotations_count"].as_u64();
                            opening_check = Some((id, observed, count));
                            first_guard = budget.pr_check_guard(cancel, || {
                                guard(PrCheckAnnotationPhaseV2::BeforeAnnotations)
                            });
                            if first_guard.is_ok() {
                                (rows, coverage) =
                                    read_pages(self, &mut budget, &path, count, limits, cancel)?;
                            }
                        }
                    }
                }
            }
        } else {
            coverage.reasons.push(if opened.coverage.enumeration_complete{"Chosen member not observed in exhausted API list; no historic deletion inferred"}else{"Chosen member unseen in partial jobs; membership Unknown"}.into());
            coverage.failure = Some(GithubErrorKind::NotFound);
        }
        if let Some((id, known, count)) = opening_check {
            let raw = match budget.read_document(
                self,
                &format!("repos/{}/check-runs/{id}", opened.original.repository),
                cancel,
            ) {
                Ok(raw) => Some(raw),
                Err(e)
                    if matches!(
                        e.kind,
                        GithubErrorKind::Permission | GithubErrorKind::NotFound
                    ) =>
                {
                    rows.clear();
                    coverage.complete = false;
                    coverage.failure = Some(e.kind);
                    coverage
                        .reasons
                        .push("Closing check unavailable; private annotations suppressed".into());
                    None
                }
                Err(e) => return Err(e),
            };
            if let Some(raw) = raw {
                let closing = annotation_check(self, &opened.original, id, &raw)?;
                if closing != known {
                    return Err(error(
                        GithubErrorKind::IdentityChanged,
                        "Original annotation check node/suite changed",
                    ));
                }
                if raw["output"]["annotations_count"].as_u64() != count {
                    coverage.complete = false;
                    coverage
                        .reasons
                        .push("Annotation advertised count moved; not an atomic list".into());
                }
            }
        }
        close_job_v2(self, &opened, &mut budget, &limits.transport, cancel, true)?;
        close_parent_v2(self, &opened, &binding, &mut budget, cancel)?;
        // Missing actual member/check is NO proof, not permission to inspect
        // private SQL or reuse an old annotations frame. Still close resource
        // and principal so a closing UID/auth failure takes precedence.
        let local = if check_node.is_some() {
            first_guard.and_then(|()| {
                budget.pr_check_guard(cancel, || guard(PrCheckAnnotationPhaseV2::BeforeFinalProof))
            })
        } else {
            Err(error(
                GithubErrorKind::Unsupported,
                "Actual member/CheckRun unavailable; no private guard called",
            ))
        };
        let who = close_account_v2(self, &opened, &binding, &mut budget, cancel)?;
        local.map_err(|e| {
            error(
                e.kind,
                "Original annotations guard unavailable; private diagnostic suppressed",
            )
        })?;
        let target = opened
            .job
            .is_some()
            .then(|| PrCheckCapturedTargetV2::member(choice))
            .transpose()?;
        let charged_bytes = budget
            .bytes
            .checked_add(budget.successful_diagnostic_bytes)
            .and_then(|n| n.checked_add(budget.failed_reserved_bytes))
            .ok_or_else(|| error(GithubErrorKind::Transport, "Annotation accounting overflow"))?;
        Ok(PrCheckAnnotationsReadV2 {
            origin,
            identity: who,
            target,
            check_id,
            check_node,
            rows,
            coverage,
            requests: budget.requests,
            charged_bytes,
            observed_through: budget.observed_through()?,
        })
    }
}
