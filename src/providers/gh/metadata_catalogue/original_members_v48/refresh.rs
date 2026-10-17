//! The two paid member-context children inserted into the existing visibility
//! engine. Complete field proof and full raw context are prepared before UID.
use super::*;
#[cfg(test)]
#[path = "page_context_v62_tests.rs"]
mod page_context_v62_tests;

pub(in crate::providers::gh) struct MembersVisibilityV48<'a> {
    pub(in crate::providers::gh) actual: &'a Arc<OriginalMetadataMembersV48>,
    pub(in crate::providers::gh) plan: &'a PrMetadataPlan,
    pub(in crate::providers::gh) receipt: &'a PrMetadataMembersReceiptV48,
    pub(in crate::providers::gh) cancel: &'a CancellationToken,
    pub(in crate::providers::gh) deadline: Instant,
}
pub(in crate::providers::gh) struct MetadataMembersContextV48 {
    actual_v61: Arc<OriginalMetadataMembersV48>,
    opening: MetadataMembersPostDataV48,
    expected: Arc<PrResourceParentV1>,
    issue: Value,
    context: PrContext,
    retained_bound_v62: usize,
    postwrite_v78: Option<super::publication::MembersPostwriteLineageV78>,
}
impl MetadataMembersContextV48 {
    pub(in crate::providers::gh) fn matches_actual_v62(
        &self,
        actual: &Arc<OriginalMetadataMembersV48>,
    ) -> bool {
        Arc::ptr_eq(&self.actual_v61, actual)
    }
    pub(in crate::providers::gh) fn retained_bound_v62(&self) -> GithubResult<usize> {
        if !self.actual_v61.metadata_tab_v62()
            || self.retained_bound_v62 == 0
            || self.retained_bound_v62 > 4 * 1024 * 1024
        {
            return Err(refused(
                GithubErrorKind::Transport,
                "Actual Metadata member retained context debit unavailable",
            ));
        }
        Ok(self.retained_bound_v62)
    }
    pub(in crate::providers::gh) fn validates_closing_v62(&self, raw: &Value) -> GithubResult<()> {
        require_same_context(self.opening.raw(), raw, &self.issue)
    }
    pub(in crate::providers::gh) fn finish_closing_v62(
        &self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        raw: &Value,
    ) -> GithubResult<ConversationRawSummaryV19> {
        self.validates_closing_v62(raw)?;
        let parent = provider.metadata_members_post_parent_v48(current, &self.expected, raw)?;
        ConversationRawSummaryV19::from_actual_raw_v19(provider, parent, raw)
    }
    pub(in crate::providers::gh) fn validate_metadata_pool_v62(
        &self,
        provider: &GithubProvider,
        pool: &Weak<ScopedReadBudgetV1>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        if !self.actual_v61.metadata_tab_v62()
            || !pool.ptr_eq(&Arc::downgrade(&self.actual_v61.pool))
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual Metadata member context pool differs",
            ));
        }
        self.postwrite_v78
            .as_ref()
            .ok_or_else(|| {
                refused(
                    GithubErrorKind::InvalidTarget,
                    "Typed Metadata member continuation missing",
                )
            })?
            .validate(provider, &self.actual_v61, pool, cancel, deadline)
    }
}
/// Issued only by this actual paid Members factory or its same-family fresh
/// read. It carries display context, never account/source/publication authority.
pub(crate) struct ActualMembersPageContextV61 {
    parent: Arc<PrResourceParentV1>,
    witness: ConversationSummaryWitnessV19,
    body_null: bool,
    context: PrContext,
    retained: usize,
    identity: HostIdentity,
    command: PrMetadataCommand,
}
impl ActualMembersPageContextV61 {
    pub(super) fn from_validated_v61(
        actual: &OriginalMetadataMembersV48,
        summary: &ConversationRawSummaryV19,
        context: PrContext,
    ) -> GithubResult<Self> {
        if !GithubProvider::same_thread_history_parent_v1(actual.parent(), summary.parent())
            || context.head_sha != summary.parent().head_sha()
            || context.base_sha != summary.parent().base_tip_sha()
            || context.repository.id != summary.parent().repository_id()
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual Members page context parent changed",
            ));
        }
        let retained = crate::pr_metadata_choices::members_context_capacity_v48(&context)
            .ok_or_else(|| {
                refused(
                    GithubErrorKind::Transport,
                    "Actual Members context capacity unavailable",
                )
            })?;
        let retained = retained
            .checked_add(std::mem::size_of::<Self>())
            .and_then(|n| n.checked_add(actual.parent().retained_heap_bytes_v2()))
            .and_then(|n| n.checked_add(actual.identity().host.capacity()))
            .and_then(|n| n.checked_add(actual.identity().login.capacity()))
            .and_then(|n| n.checked_add(actual.identity().gh_version.capacity()))
            .ok_or_else(|| {
                refused(
                    GithubErrorKind::Transport,
                    "Actual Members context retained accounting overflow",
                )
            })?;
        Ok(Self {
            parent: Arc::clone(summary.parent()),
            witness: summary.witness_v19(),
            body_null: summary.body_null_v51(),
            context,
            retained,
            identity: actual.identity().clone(),
            command: actual.command,
        })
    }
    pub(crate) fn retained_bytes_v61(&self) -> usize {
        self.retained
    }
    pub(in crate::providers::gh) fn reserve_box_move_v79(
        &mut self,
        snapshot: &ConversationDetailSnapshotV19,
    ) -> GithubResult<()> {
        self.validate_snapshot_v61(snapshot)?;
        // BEFORE Box::new: retain BOTH requested padded payload layouts (old
        // stack payload plus heap move overlap) and its pointer. Nested dynamic
        // buffers move unchanged. Keep this debit after unboxing; later storage
        // preflight sees it, with no discount/reset or allocator/RSS claim.
        let relocation = std::alloc::Layout::new::<Self>()
            .pad_to_align()
            .size()
            .checked_mul(2)
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Box<Self>>()))
            .ok_or_else(|| refused(GithubErrorKind::Transport, "Members box layout overflow"))?;
        let retained = self
            .retained
            .checked_add(relocation)
            .ok_or_else(|| refused(GithubErrorKind::Transport, "Members box retained overflow"))?;
        if snapshot
            .metadata_model_bytes_v51()?
            .checked_add(retained)
            .is_none_or(|bytes| bytes > 4 * 1024 * 1024)
        {
            return Err(refused(
                GithubErrorKind::Transport,
                "Members box move exceeds4MiB",
            ));
        }
        self.retained = retained;
        Ok(())
    }
    pub(crate) fn validates_command_v62(&self, command: PrMetadataCommand) -> bool {
        self.command == command
    }
    pub(crate) fn validate_snapshot_v61(
        &self,
        snapshot: &ConversationDetailSnapshotV19,
    ) -> GithubResult<()> {
        if !Arc::ptr_eq(&self.parent, snapshot.parent())
            || !GithubProvider::same_thread_history_parent_v1(&self.parent, snapshot.parent())
            || !self.witness.matches_snapshot_v61(snapshot)?
            || !snapshot.matches_body_null_v61(self.body_null)
            || snapshot
                .metadata_model_bytes_v51()?
                .checked_add(self.retained)
                .is_none_or(|n| n > 4 * 1024 * 1024)
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual Members context/snapshot or retained peak differs",
            ));
        }
        Ok(())
    }
    pub(crate) fn into_display_v61(self) -> PrContext {
        self.context
    }
    /// Count the actual bounded serialization without constructing a second
    /// Value/Vec. Used by the shared storage preclone debit, not a data getter.
    pub(crate) fn encoded_bytes_v62(&self) -> GithubResult<usize> {
        struct Count(usize);
        impl std::io::Write for Count {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.0 = self
                    .0
                    .checked_add(bytes.len())
                    .filter(|n| *n <= 512 * 1024)
                    .ok_or_else(|| std::io::Error::other("Actual Members context encoded bound"))?;
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let mut count = Count(0);
        serde_json::to_writer(&mut count, &self.context).map_err(|_| {
            refused(
                GithubErrorKind::Transport,
                "Actual Members encoded context unavailable",
            )
        })?;
        Ok(count.0)
    }
    pub(crate) fn prepare_fixed_record_v62(
        &self,
        snapshot: &ConversationDetailSnapshotV19,
        target: &crate::pr_write_journal::PrWriteTarget,
    ) -> GithubResult<crate::storage::Record> {
        self.validate_snapshot_v61(snapshot)?;
        if crate::storage::Scope::github(&self.identity.host, self.identity.account_id).map_err(
            |_| {
                refused(
                    GithubErrorKind::IdentityChanged,
                    "Actual Members record principal unavailable",
                )
            },
        )? != *target.scope()
            || target.node_id() != self.parent.node_id()
            || target.number() != self.parent.number()
            || !target
                .repository()
                .eq_ignore_ascii_case(self.parent.repository())
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual Members fixed record target differs",
            ));
        }
        self.encoded_bytes_v62()?;
        let mut record = crate::storage::Record::new(
            "pr-metadata-context-v48",
            target.node_id(),
            target.repository(),
            serde_json::to_value(&self.context).map_err(|_| {
                refused(
                    GithubErrorKind::Json,
                    "Actual Members context serialization unavailable",
                )
            })?,
        );
        record.complete = true;
        record.source_updated_at = Some(snapshot.details().summary.pr.updated_at.clone());
        Ok(record)
    }
}
impl GithubProvider {
    pub(in crate::providers::gh) fn validate_members_fresh_context_v62(
        &self,
        actual: &OriginalMetadataMembersV48,
        parent: &Value,
        issue: &Value,
    ) -> GithubResult<()> {
        if actual.purpose != MetadataMembersReadPurposeV48::MetadataInspectionV62
            || issue["id"] != actual.read().context().issue_metadata["id"]
            || issue["node_id"] != actual.read().context().issue_metadata["node_id"]
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual fresh Metadata member issue/purpose differs",
            ));
        }
        issue_identity(self, actual.parent().summary(), issue)?;
        require_same_context(actual.read().parent_raw(), parent, issue)?;
        let field = actual.command().field();
        if !matches!(field, "reviewers" | "teams")
            && issue.get(field) != actual.read().context().issue_metadata.get(field)
        {
            return Err(refused(
                GithubErrorKind::StaleHead,
                "Actual fresh Metadata selected membership changed",
            ));
        }
        Ok(())
    }
    pub(in crate::providers::gh) fn validate_members_context_after_v62(
        &self,
        actual: &OriginalMetadataMembersV48,
        plan: &PrMetadataPlan,
        parent: &Value,
        issue: &Value,
    ) -> GithubResult<()> {
        if !actual.metadata_tab_v62()
            || plan.command() != actual.command()
            || plan.pr_id() != actual.parent().database_id()
            || plan.repository_id() != actual.parent().repository_id()
            || issue["id"].as_u64() != Some(plan.issue_id())
            || issue["node_id"].as_str() != Some(plan.issue_node())
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual Metadata member context purpose/target differs",
            ));
        }
        issue_identity(self, actual.parent().summary(), issue)?;
        members_validation_v48::validate_rest_result_v48(plan, parent, issue)
    }
    pub(in crate::providers::gh) fn prepare_members_context_v48(
        &self,
        request: MembersVisibilityV48<'_>,
        opening: MetadataMembersPostDataV48,
        captures: &mut PostWriteCaptureAllowanceV2,
        mut metadata_heap_v62: Option<&mut MetadataMembersHeapV62>,
    ) -> GithubResult<MetadataMembersContextV48> {
        let MembersVisibilityV48 {
            actual,
            plan,
            receipt,
            cancel,
            deadline,
        } = request;
        if captures.consumed_requests() != 9
            || !receipt.validates_refresh(actual, plan, cancel)
            || !GithubProvider::same_thread_history_parent_v1(actual.parent(), receipt.parent())
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Exact member visibility opening/receipt differs",
            ));
        }
        alive(
            cancel,
            deadline.min(receipt.deadline()).min(captures.deadline()),
        )?;
        let parent = receipt.parent();
        let issue = if let Some(heap) = metadata_heap_v62.as_deref_mut() {
            captures.members_metadata_role_v62(
                self,
                actual,
                MembersMetadataRoleV62::VisibilityIssue,
                heap,
                cancel,
            )?
        } else {
            captures.api(
                self,
                &format!("repos/{}/issues/{}", parent.repository(), parent.number()),
                "GET",
                None,
                None,
                cancel,
            )?
        };
        issue_identity(self, parent.summary(), &issue)?;
        if issue["id"].as_u64() != Some(plan.issue_id())
            || issue["node_id"].as_str() != Some(plan.issue_node())
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual member visibility issue identity changed",
            ));
        }
        require_same_context(opening.raw(), opening.raw(), &issue)?;
        members_validation_v48::validate_rest_result_v48(plan, opening.raw(), &issue)?;
        let mut query = members_query_v48(plan.command(), plan.selected())?;
        query["variables"]["id"] = json!(parent.node_id());
        let raw = if let Some(heap) = metadata_heap_v62.as_deref_mut() {
            captures.members_metadata_current_v62(self, actual, plan, heap, cancel)?
        } else {
            captures.api(self, "graphql", "POST", Some(query), None, cancel)?
        };
        error_free(&raw)?;
        members_validation_v48::validate_result_v48(plan, actual.read(), plan.after(), &raw)?;
        let (_, context, reasons) = observed_context(
            self,
            parent.summary(),
            opening.repository_raw(),
            opening.raw(),
            issue.clone(),
        )?;
        if !reasons.is_empty() || captures.consumed_requests() != 11 {
            return Err(refused(
                GithubErrorKind::Unsupported,
                "Fresh member structured context unavailable; no default metadata publication",
            ));
        }
        crate::pr_metadata_choices::members_context_capacity_v48(&context).ok_or_else(|| {
            refused(
                GithubErrorKind::InvalidTarget,
                "Fresh member context retained capacity exceeded",
            )
        })?;
        alive(
            cancel,
            deadline.min(receipt.deadline()).min(captures.deadline()),
        )?;
        Ok(MetadataMembersContextV48 {
            postwrite_v78: if actual.metadata_tab_v62() {
                Some(receipt.postwrite_context_lineage_v78(self, actual, plan, cancel)?)
            } else {
                None
            },
            actual_v61: Arc::clone(actual),
            opening,
            expected: Arc::clone(parent),
            issue,
            context,
            retained_bound_v62: metadata_heap_v62
                .as_ref()
                .map_or(0, |heap| heap.retained_bound()),
        })
    }
    pub(in crate::providers::gh) fn finish_members_metadata_summary_v62(
        &self,
        current: &CurrentCatalogueRepository,
        context: MetadataMembersContextV48,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        debit:crate::providers::gh::pr_resources_v1::metadata_context_v51::MetadataRetainedDebitV51,
    ) -> GithubResult<(ConversationRawSummaryV19, usize)> {
        let retained = context.retained_bound_v62()?;
        let summary = captures.members_metadata_closing_summary_v62(
            self,
            MembersMetadataClosingInputV62 {
                actual: &context.actual_v61,
                current,
                context: &context,
                debit,
            },
            cancel,
        )?;
        Ok((summary, retained))
    }
    pub(in crate::providers::gh) fn finish_members_summary_v48(
        &self,
        current: &CurrentCatalogueRepository,
        context: MetadataMembersContextV48,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<(ConversationRawSummaryV19, ActualMembersPageContextV61)> {
        if captures.consumed_requests() != 16 {
            return Err(refused(
                GithubErrorKind::InvalidTarget,
                "Exact member detail cursor differs before closing parent",
            ));
        }
        let parent = &context.expected;
        let bounded = self
            .clone()
            .with_scoped_read_window(captures.deadline(), 128 * 1024)?;
        let raw = captures.api(
            &bounded,
            &format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
            "GET",
            None,
            None,
            cancel,
        )?;
        let closing = self.metadata_members_post_parent_v48(current, parent, &raw)?;
        require_same_context(context.opening.raw(), &raw, &context.issue)?;
        let summary = ConversationRawSummaryV19::from_actual_raw_v19(self, closing, &raw)?;
        let context = ActualMembersPageContextV61::from_validated_v61(
            // The original typed actual was validated at opening/cursor9 and
            // retained in this closed context until the fixed closing read.
            &context.actual_v61,
            &summary,
            context.context,
        )?;
        Ok((summary, context))
    }
}
