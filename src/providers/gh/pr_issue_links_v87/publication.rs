//! One fixed mutation plus TWO independent readbacks. A usable mutation body
//! alone never constructs a typed receipt, permission, Source or new phase.
use super::*;

pub(crate) enum IssueLinkPublicationFailureV87 {
    NotStarted(GithubError),
    WriteMayHaveStarted(GithubError),
}
pub(crate) struct IssueLinkReceiptV87 {
    actual: Weak<OriginalIssueLinksV87>,
    plan_digest: String,
    identity: HostIdentity,
    parent: Arc<PrResourceParentV1>,
    effect: crate::pr_write_journal::PrConfirmedEffect,
    after: IssueLinkPairV87,
    generations: [u64; 2],
    cancellation: CancellationToken,
    deadline: Instant,
    retained: usize,
}
impl std::fmt::Debug for IssueLinkReceiptV87 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IssueLinkReceiptV87(<actual mutation+two independent readbacks; no Source>)")
    }
}
pub(crate) struct IssueLinkWriteV87<'a> {
    pub(crate) measured: &'a GithubProvider,
    pub(crate) actual: &'a Arc<OriginalIssueLinksV87>,
    pub(crate) plan: &'a IssueLinkPlanV87,
    pub(crate) captures: &'a mut PostWriteCaptureAllowanceV2,
    pub(crate) cancel: &'a CancellationToken,
    pub(crate) deadline: Instant,
}
impl GithubProvider {
    pub(crate) fn publish_original_issue_link_v87(
        &self,
        write: IssueLinkWriteV87<'_>,
        before_write: impl FnOnce() -> GithubResult<HostIdentity>,
    ) -> Result<IssueLinkReceiptV87, IssueLinkPublicationFailureV87> {
        let actual = write.actual;
        let (mut request, identity) = (|| {
            actual.binding.validate_live(write.measured, write.cancel)?;
            actual.validate_plan(write.plan)?;
            write
                .captures
                .validate_prewrite(write.measured, write.cancel)?;
            if write.captures.consumed_requests() != 0 || write.deadline > actual.deadline() {
                return Err(refused(GithubErrorKind::IdentityChanged));
            }
            let request = PreparedIssueLinkRequestV87::prepare(
                actual,
                write.plan,
                write.measured,
                write.cancel,
            )?;
            // All request/Value/env/Doc/origin/SQL setup is above final Core UID.
            // Following close: only scalar current ACK checks, fixed paid work.
            let identity = before_write()?;
            if !identity.same_account(actual.identity()) {
                return Err(refused(GithubErrorKind::IdentityChanged));
            }
            alive(write.cancel, write.deadline.min(actual.deadline()))?;
            Ok((request, identity))
        })()
        .map_err(IssueLinkPublicationFailureV87::NotStarted)?;
        // This typed result preserves the runner's actual pre-spawn refusal vs
        // invoked/unknown distinction; merely spending a slot is NOT a write.
        let run = write
            .captures
            .issue_link_mutation_v87(self, &mut request, write.cancel)?;
        let result = (|| {
            let old_capture = run
                .output()
                .stdout
                .capacity()
                .checked_add(run.output().stderr.capacity())
                .and_then(|n| {
                    n.checked_add(std::mem::size_of::<crate::providers::runner::CommandOutput>())
                })
                .ok_or_else(|| refused(GithubErrorKind::Transport))?;
            let mut heap = IssueLinkHeapV87::new(
                request
                    .retained
                    .checked_add(old_capture)
                    .ok_or_else(|| refused(GithubErrorKind::Transport))?,
            )?;
            heap.charge(2 * std::mem::size_of::<IssueLinkReceiptV87>() + 16 * 1024)?;
            let observed = (|| {
                let pr_doc = write.captures.issue_link_readback_v87(
                    self,
                    &request,
                    IssueLinkReadbackRoleV87::Pr,
                    &mut heap,
                    write.cancel,
                )?;
                let pr = protocol::IssueLinkPrSetsV87::parse(
                    pr_doc.value(),
                    &actual.opening,
                    &self.host,
                    &mut heap,
                )?;
                heap.release_document(pr_doc)?;
                let issue_doc = write.captures.issue_link_readback_v87(
                    self,
                    &request,
                    IssueLinkReadbackRoleV87::Issue,
                    &mut heap,
                    write.cancel,
                )?;
                let issue = protocol::IssueLinkReverseSetsV87::parse(
                    issue_doc.value(),
                    &actual.pair.issue,
                    &self.host,
                    &mut heap,
                )?;
                heap.release_document(issue_doc)?;
                let after =
                    IssueLinkPairV87::from_independent_readbacks(pr, issue, &actual.opening)?;
                actual
                    .pair
                    .confirms(&after, actual.parent(), actual.command)?;
                if write.captures.consumed_requests() != 3 {
                    return Err(refused(GithubErrorKind::InvalidTarget));
                }
                self.runner
                    .validate_issue_link_generations_v87(
                        &actual.resources,
                        run.successors(),
                        write.cancel,
                        write.deadline.min(request.deadline),
                    )
                    .map_err(GithubError::from)?;
                Ok(after)
            })();
            let after = match observed {
                Ok(after) => after,
                Err(error) => {
                    let _ = self
                        .runner
                        .mark_issue_link_pair_unusable_v87(&request, write.cancel);
                    return Err(error);
                }
            };
            let retained = actual
                .retained
                .checked_add(after.retained())
                .and_then(|n| n.checked_add(2 * std::mem::size_of::<IssueLinkReceiptV87>() + 4096))
                .filter(|n| *n <= MODEL_CAP_V87)
                .ok_or_else(|| refused(GithubErrorKind::Transport))?;
            if retained > heap.retained()? {
                return Err(refused(GithubErrorKind::Transport));
            }
            request.accept_readback_receipt_v87(&after)?;
            self.runner
                .complete_issue_link_receipt_v87(&request, write.cancel)
                .map_err(GithubError::from)?;
            Ok(IssueLinkReceiptV87 {
                actual: Arc::downgrade(actual),
                plan_digest: write.plan.digest.clone(),
                identity,
                parent: Arc::clone(actual.parent()),
                effect: crate::pr_write_journal::PrConfirmedEffect {
                    kind: crate::pr_write_journal::PrEffectKind::Metadata,
                    resource_id: actual.pair.issue.id.to_string(),
                    node_id: Some(actual.pair.issue.node.clone()),
                },
                after,
                generations: run.successors(),
                cancellation: write.cancel.clone(),
                deadline: write.deadline.min(request.deadline),
                retained,
            })
        })();
        result.map_err(IssueLinkPublicationFailureV87::WriteMayHaveStarted)
    }
}
impl IssueLinkReceiptV87 {
    pub(super) fn validate_closing_pair_v87(
        &self,
        actual: &Arc<OriginalIssueLinksV87>,
        observed: &IssueLinkPairV87,
    ) -> GithubResult<()> {
        if !self.actual.ptr_eq(&Arc::downgrade(actual)) || !observed.same_observation(&self.after) {
            return Err(refused(GithubErrorKind::StaleHead));
        }
        actual
            .pair
            .confirms(observed, actual.parent(), actual.command)
    }
    pub(in crate::providers::gh) fn validates_paid_pool_v87(
        &self,
        provider: &GithubProvider,
        actual: &Arc<OriginalIssueLinksV87>,
        pool: &Weak<ScopedReadBudgetV1>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.validate_epoch_v87(provider, actual, cancel)?;
        if !pool.ptr_eq(&Arc::downgrade(&actual.binding.measured)) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    #[cfg(test)]
    pub(super) fn actual_readback_counts_v87(&self) -> ([u64; 2], usize, usize) {
        let (forward, reverse) = self.after.actual_manual_counts_v87();
        (self.generations, forward, reverse)
    }
    pub(crate) fn effect(&self) -> &crate::pr_write_journal::PrConfirmedEffect {
        &self.effect
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn retained_bound_v87(&self) -> usize {
        self.retained
    }
    pub(in crate::providers::gh) fn validate_epoch_v87(
        &self,
        provider: &GithubProvider,
        actual: &Arc<OriginalIssueLinksV87>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        actual
            .binding
            .validate_lineage(provider, cancel, self.deadline)?;
        if !self.actual.ptr_eq(&Arc::downgrade(actual))
            || !self.cancellation.same_instance(cancel)
            || self.identity.host != actual.identity.host
            || self.identity.account_id != actual.identity.account_id
            || !GithubProvider::same_thread_history_parent_v1(&self.parent, actual.parent())
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        provider
            .runner
            .validate_issue_link_generations_v87(
                &actual.resources,
                self.generations,
                cancel,
                self.deadline,
            )
            .map_err(GithubError::from)
    }
    pub(crate) fn validate_plan_v87(
        &self,
        provider: &GithubProvider,
        actual: &Arc<OriginalIssueLinksV87>,
        plan: &IssueLinkPlanV87,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.validate_epoch_v87(provider, actual, cancel)?;
        if !plan.actual.ptr_eq(&Arc::downgrade(actual)) || plan.digest != self.plan_digest {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
}
