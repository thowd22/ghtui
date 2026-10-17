//! Exact15-child manual member refresh: SAME Core6 + raw/issue/GQL + five
//! bounded detail lists + closing raw. No eligible catalogue or write grant.
use super::*;
pub(crate) struct MembersManualReadV48 {
    actual: Arc<OriginalMetadataMembersV48>,
    summary: Option<ConversationRawSummaryV19>,
    witness: ConversationSummaryWitnessV19,
    context_v61: Option<ActualMembersPageContextV61>,
    metadata_v62: bool,
    closing_v62: Option<Arc<PrResourceParentV1>>,
}
impl MembersManualReadV48 {
    pub(crate) fn metadata_tab_v62(&self) -> bool {
        self.metadata_v62
    }
    pub(crate) fn take_summary_v48(&mut self) -> GithubResult<ConversationRawSummaryV19> {
        self.summary.take().ok_or_else(|| {
            refused(
                GithubErrorKind::InvalidTarget,
                "Actual member refresh summary already consumed",
            )
        })
    }
}
impl GithubProvider {
    pub(crate) fn collect_members_metadata_manual_details_v62(
        &self,
        read: &MembersManualReadV48,
        summary: ConversationRawSummaryV19,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationDetailSnapshotV19> {
        if !read.metadata_v62 {
            return Err(refused(
                GithubErrorKind::InvalidTarget,
                "Actual Metadata member manual purpose differs",
            ));
        }
        self.collect_members_metadata_details_measured_v62(summary, &read.actual, cancel)
    }
    pub(crate) fn begin_members_manual_refresh_v48(
        &self,
        current: &CurrentCatalogueRepository,
        own: &Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<MembersManualReadV48> {
        let command = own.terminal().members_command_v48().ok_or_else(|| {
            refused(
                GithubErrorKind::InvalidTarget,
                "Exact member terminal required for member refresh",
            )
        })?;
        let limits = PrMetadataReadLimits::default();
        let bounded = self.clone().with_original_pr_read_bounds_v13(
            self.original_pr_read_deadline_v13().ok_or_else(|| {
                refused(
                    GithubErrorKind::InvalidTarget,
                    "Original member refresh window missing",
                )
            })?,
            128 * 1024,
        )?;
        let actual = Arc::new(bounded.observe_original_metadata_members_v48(
            MetadataMembersObservationV48 {
                current,
                intent: own.source().opening().parent(),
                identity,
                command,
                limits: &limits,
                selected_plan: None,
                purpose: if own.source().opening().metadata_members_tab_v61(command) {
                    MetadataMembersReadPurposeV48::MetadataInspectionV62
                } else {
                    MetadataMembersReadPurposeV48::Inspection
                },
                retained_v62: None,
            },
            cancel,
        )?);
        if !GithubProvider::same_thread_history_parent_v1(own.terminal().parent(), actual.parent())
        {
            // Full comparison uses the existing genuine provider parent logic;
            // no saved IDs or expected DTO can construct a fresh boundary.
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Exact terminal member parent changed",
            ));
        }
        let summary = ConversationRawSummaryV19::from_members_actual_v48(self, &actual)?;
        let witness = summary.witness_v19();
        let retained =
            crate::pr_metadata_choices::members_context_capacity_v48(actual.read().context())
                .ok_or_else(|| {
                    refused(
                        GithubErrorKind::Transport,
                        "Actual Members manual context exceeds retained cap",
                    )
                })?;
        if retained > 2 * 1024 * 1024 {
            return Err(refused(
                GithubErrorKind::Transport,
                "Actual Members context cap exceeded",
            ));
        }
        let metadata_v62 = actual.metadata_tab_v62();
        let context_v61 = if metadata_v62 {
            None
        } else {
            Some(ActualMembersPageContextV61::from_validated_v61(
                &actual,
                &summary,
                actual.read().context().clone(),
            )?)
        };
        Ok(MembersManualReadV48 {
            actual,
            summary: Some(summary),
            witness,
            context_v61,
            metadata_v62,
            closing_v62: None,
        })
    }
    pub(crate) fn finish_members_metadata_manual_v62(
        &self,
        current: &CurrentCatalogueRepository,
        read: &mut MembersManualReadV48,
        intent: &PrParentIntentV13,
        snapshot: ConversationDetailSnapshotV19,
        cancel: &CancellationToken,
    ) -> GithubResult<(ConversationDetailSnapshotV19, ConversationPageSupplementV61)> {
        if !read.metadata_v62 || read.closing_v62.is_some() || read.context_v61.is_some() {
            return Err(refused(
                GithubErrorKind::InvalidTarget,
                "Actual Metadata member fresh phase differs",
            ));
        }
        read.actual
            .validate_lineage(self, cancel, read.actual.deadline())?;
        let retained = snapshot
            .metadata_model_bytes_v51()?
            .checked_add(read.actual.retained_model_v62()?)
            .and_then(|bytes| bytes.checked_add(64 * 1024))
            .filter(|bytes| *bytes <= 4 * 1024 * 1024)
            .ok_or_else(|| {
                refused(
                    GithubErrorKind::Transport,
                    "Actual member fresh snapshot debit exceeded",
                )
            })?;
        let mut heap = MetadataMembersHeapV62::new(retained)?;
        let closing =
            self.metadata_members_bounded_parent_v62(current, intent, &mut heap, cancel)?;
        if !GithubProvider::same_thread_history_parent_v1(read.actual.parent(), closing.parent()) {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual Metadata member fresh parent changed",
            ));
        }
        require_same_context(
            read.actual.read().parent_raw(),
            closing.raw(),
            &read.actual.read().context().issue_metadata,
        )?;
        let summary = ConversationRawSummaryV19::from_members_closing_v48(self, &closing)?;
        if !read.witness.matches_v19(&summary) {
            return Err(refused(
                GithubErrorKind::StaleHead,
                "Actual Metadata member fresh summary changed",
            ));
        }
        let context = self.pr_members_metadata_refresh_context_v62(
            current,
            &read.actual,
            &snapshot,
            retained,
            cancel,
        )?;
        read.closing_v62 = Some(Arc::clone(closing.parent()));
        let supplement = ConversationPageSupplementV61::metadata_v79(&snapshot, context)?;
        Ok((snapshot, supplement))
    }
    pub(crate) fn attach_members_manual_context_v48(
        &self,
        snapshot: ConversationDetailSnapshotV19,
        read: &mut MembersManualReadV48,
        cancel: &CancellationToken,
    ) -> GithubResult<(ConversationDetailSnapshotV19, ConversationPageSupplementV61)> {
        read.actual
            .validate_lineage(self, cancel, read.actual.deadline())?;
        let context = read.context_v61.take().ok_or_else(|| {
            refused(
                GithubErrorKind::InvalidTarget,
                "Actual Members context already consumed",
            )
        })?;
        context.validate_snapshot_v61(&snapshot)?;
        let supplement = ConversationPageSupplementV61::members_v79(&snapshot, context)?;
        Ok((snapshot, supplement))
    }
    pub(crate) fn close_members_manual_refresh_v48(
        &self,
        current: &CurrentCatalogueRepository,
        read: &MembersManualReadV48,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<PrResourceParentV1>> {
        read.actual
            .validate_lineage(self, cancel, read.actual.deadline())?;
        if read.metadata_v62 {
            return read.closing_v62.as_ref().map(Arc::clone).ok_or_else(|| {
                refused(
                    GithubErrorKind::InvalidTarget,
                    "Actual Metadata member fresh closure unavailable",
                )
            });
        }
        let bounded = self
            .clone()
            .with_original_pr_read_bounds_v13(read.actual.deadline(), 128 * 1024)?;
        let closing = bounded.metadata_members_parent_v48(current, intent, cancel)?;
        if !GithubProvider::same_thread_history_parent_v1(read.actual.parent(), closing.parent()) {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Full member refresh parent changed",
            ));
        }
        require_same_context(
            read.actual.read().parent_raw(),
            closing.raw(),
            &read.actual.read().context().issue_metadata,
        )?;
        // Compare the complete emitted closing raw summary, not only body or
        // the intentionally stripped parent expectation.
        let closing_summary =
            ConversationRawSummaryV19::from_members_closing_v48(&bounded, &closing)?;
        if !read.witness.matches_v19(&closing_summary) {
            return Err(refused(
                GithubErrorKind::StaleHead,
                "Full member refresh emitted summary changed",
            ));
        }
        Ok(Arc::clone(closing.parent()))
    }
}
