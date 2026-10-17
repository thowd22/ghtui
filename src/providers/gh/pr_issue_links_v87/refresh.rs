//! Closed fixed paid Issue visibility roles. These are actual observations,
//! never a replacement Core/window, transport getter or receipt-to-Source API.
use super::*;
use crate::providers::gh::ConversationDetailListV19;
use crate::providers::gh::{
    ActualMetadataTabContextV51, ConversationDetailSnapshotV19, ConversationRawSummaryV19,
};

pub(in crate::providers::gh) struct IssueLinkPaidReadV87<'a> {
    actual: &'a Arc<OriginalIssueLinksV87>,
    receipt: &'a IssueLinkReceiptV87,
    plan: &'a IssueLinkPlanV87,
    heap: IssueLinkHeapV87,
}
pub(crate) struct ActualIssueLinkPageContextV87 {
    parent: Arc<PrResourceParentV1>,
    provenance: IssueLinkContextProvenanceV94,
    command: IssueLinkCommandV87,
    plan_digest: String,
    after: IssueLinkPairV87,
    metadata: Option<ActualMetadataTabContextV51>,
    retained: usize,
}
enum IssueLinkContextProvenanceV94 {
    Paid(Weak<OriginalIssueLinksV87>),
    Fresh(Arc<crate::pr_write_journal::conversation_refresh_v19::ConversationTerminalWitnessV19>),
}
/// Borrowed scalar comparison ONLY. Its public-crate fields cannot construct a
/// context, terminal, Source, read window, receipt or mutation permission.
pub(crate) struct IssueLinkContextBindingV87<'a> {
    pub(crate) terminal:
        &'a crate::pr_write_journal::conversation_refresh_v19::ConversationTerminalWitnessV19,
    pub(crate) actual: &'a Weak<OriginalIssueLinksV87>,
    pub(crate) command: IssueLinkCommandV87,
    pub(crate) plan_digest: &'a str,
    pub(crate) issue_node: &'a str,
    pub(crate) issue_id: u64,
    pub(crate) issue_repository_id: u64,
    pub(crate) metadata: bool,
}
#[derive(serde::Serialize)]
struct FixedIssueLinkRecordV87<'a> {
    version: u8,
    command: IssueLinkCommandV87,
    parent_pr_node: &'a str,
    parent_pr_id: u64,
    repository_id: u64,
    head: &'a str,
    base: &'a str,
    requested_plan_sha256: &'a str,
    observed: &'a IssueLinkPairV87,
}
struct RecordCountV87(usize);
impl std::io::Write for RecordCountV87 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= 512 * KIB)
            .ok_or_else(|| std::io::Error::other("Actual manual-Issue record exceeds512KiB"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl std::fmt::Debug for ActualIssueLinkPageContextV87 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ActualIssueLinkPageContextV87(<actual closed two-sided link observations; display only>)")
    }
}
impl ActualIssueLinkPageContextV87 {
    pub(super) fn from_fresh_v94(
        snapshot: &ConversationDetailSnapshotV19,
        descriptor: &crate::pr_write_journal::conversation_refresh_v19::IssueLinkManualDescriptorV94,
        after: IssueLinkPairV87,
        metadata: Option<ActualMetadataTabContextV51>,
        plan_digest: String,
        retained: usize,
    ) -> GithubResult<Self> {
        let terminal = descriptor.terminal();
        if !GithubProvider::same_thread_history_parent_v1(terminal.parent(), snapshot.parent())
            || terminal
                .source_v19()
                .opening()
                .issue_link_metadata_tab_v87()
                != metadata.is_some()
            || plan_digest != descriptor.plan_digest_v94()
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let context = Self {
            parent: Arc::clone(snapshot.parent()),
            provenance: IssueLinkContextProvenanceV94::Fresh(Arc::clone(terminal)),
            command: descriptor.command(),
            plan_digest,
            after,
            metadata,
            retained,
        };
        context.validate_snapshot_v87(snapshot)?;
        if !terminal.validates_issue_link_context_v87(&context) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok(context)
    }
    pub(crate) fn matches_terminal_binding_v87(&self, b: IssueLinkContextBindingV87<'_>) -> bool {
        let provenance = match &self.provenance {
            IssueLinkContextProvenanceV94::Paid(actual) => actual.ptr_eq(b.actual),
            IssueLinkContextProvenanceV94::Fresh(actual) => {
                std::ptr::eq(actual.as_ref(), b.terminal)
            }
        };
        provenance
            && self.command == b.command
            && self.plan_digest == b.plan_digest
            && self.after.issue.node == b.issue_node
            && self.after.issue.id == b.issue_id
            && self.after.issue.repository.id == b.issue_repository_id
            && self.metadata.is_some() == b.metadata
    }
    fn record(&self) -> FixedIssueLinkRecordV87<'_> {
        FixedIssueLinkRecordV87 {
            version: 87,
            command: self.command,
            parent_pr_node: self.parent.node_id(),
            parent_pr_id: self.parent.database_id(),
            repository_id: self.parent.repository_id(),
            head: self.parent.head_sha(),
            base: self.parent.base_tip_sha(),
            requested_plan_sha256: &self.plan_digest,
            observed: &self.after,
        }
    }
    pub(crate) fn fixed_record_copy_peak_v87(
        &self,
        snapshot: &ConversationDetailSnapshotV19,
        terminal:&crate::pr_write_journal::conversation_refresh_v19::ConversationTerminalWitnessV19,
    ) -> GithubResult<usize> {
        self.validate_snapshot_v87(snapshot)?;
        if !terminal.validates_issue_link_context_v87(self) {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let mut count = RecordCountV87(0);
        serde_json::to_writer(&mut count, &self.record())
            .map_err(|_| refused(GithubErrorKind::Transport))?;
        let metadata = "pr-manual-issue-links-v87"
            .len()
            .checked_add(terminal.target().node_id().len())
            .and_then(|n| n.checked_add(terminal.target().repository().len()))
            .and_then(|n| n.checked_add(snapshot.details().summary.pr.updated_at.len()))
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        // Original six-set trees remain live while the fixed Record Value,
        // encoded payload, escape scratch and vector/map spare copies coexist.
        // Twelve terms cover sixfold JSON escaping with2x requested growth;
        // 16KiB adds fixed Record/EncodedRecord, generated keys and set nodes.
        count
            .0
            .checked_add(self.after.retained())
            .and_then(|n| n.checked_add(metadata))
            .and_then(|n| n.checked_mul(12))
            .and_then(|n| n.checked_add(16 * KIB))
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or_else(|| refused(GithubErrorKind::Transport))
    }
    pub(crate) fn prepare_fixed_record_v87(
        &self,
        snapshot: &ConversationDetailSnapshotV19,
        terminal:&crate::pr_write_journal::conversation_refresh_v19::ConversationTerminalWitnessV19,
    ) -> GithubResult<crate::storage::Record> {
        self.fixed_record_copy_peak_v87(snapshot, terminal)?;
        let target = terminal.target();
        let mut record = crate::storage::Record::new(
            "pr-manual-issue-links-v87",
            target.node_id(),
            target.repository(),
            serde_json::to_value(self.record()).map_err(|_| refused(GithubErrorKind::Json))?,
        );
        record.source_updated_at = Some(snapshot.details().summary.pr.updated_at.clone());
        record.complete = snapshot.details().coverage.complete;
        Ok(record)
    }
    pub(crate) fn retained_bound_v87(&self) -> usize {
        self.retained
    }
    pub(crate) fn matches_source_v87(
        &self,
        source: &crate::scoped_pr_collaboration_source_v17::PrCollaborationSourceV17,
    ) -> bool {
        source.opening().admits_issue_links_v87(self.command)
            && source.opening().issue_link_metadata_tab_v87() == self.metadata.is_some()
            && GithubProvider::same_thread_history_parent_v1(source.parent(), &self.parent)
    }
    /// The only retained Metadata context is moved, not duplicated or
    /// reconstructed from this record. Ordinary Issue links retain None.
    pub(crate) fn into_metadata_v87(self) -> Option<ActualMetadataTabContextV51> {
        self.metadata
    }
    pub(crate) fn prepare_rows_v87(
        &self,
        snapshot: &ConversationDetailSnapshotV19,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<crate::github_workspace::PrRow>> {
        self.validate_snapshot_v87(snapshot)?;
        let rows = self
            .after
            .display_row_count_v87()
            .checked_add(9)
            .filter(|n| *n <= 65_536)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        let mut text = self
            .after
            .issue
            .url
            .len()
            .checked_add(self.after.issue.title.len())
            .and_then(|n| n.checked_add(self.after.issue.repository.name.len()))
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        for (label, values) in self.after.display_sets_v87() {
            text = text
                .checked_add(label.len())
                .ok_or_else(|| refused(GithubErrorKind::Transport))?;
            for value in values {
                let (repo, _, url) = value.display_fact_v87();
                text = text
                    .checked_add(repo.len())
                    .and_then(|n| n.checked_add(url.len()))
                    .ok_or_else(|| refused(GithubErrorKind::Transport))?;
            }
        }
        // The full actual context and snapshot remain alive during escaped
        // label/Vec construction. Sixfold escape and doubled String/Vec spare
        // capacity, per-row numeric/fixed captions and actual PrRow layouts
        // are charged BEFORE constructing even the first row.
        text.checked_mul(12)
            .and_then(|n| {
                n.checked_add(
                    rows.checked_mul(
                        512 + 2 * std::mem::size_of::<crate::github_workspace::PrRow>(),
                    )?,
                )
            })
            .and_then(|n| n.checked_add(self.retained))
            .and_then(|n| n.checked_add(snapshot.metadata_model_bytes_v51().ok()?))
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        if cancel.is_cancelled() {
            return Err(refused(GithubErrorKind::Transport));
        }
        let row = |label: String, role| crate::github_workspace::PrRow {
            label,
            role,
            actor: None,
            anchor: None,
            comment: None,
        };
        use crate::theme::SemanticToken;
        let issue = &self.after.issue;
        let mut result = Vec::with_capacity(rows);
        result.push(row(
            "Manual development links • independently observed on PR and Issue".into(),
            SemanticToken::Muted,
        ));
        result.push(row(
            format!(
                "Selected Issue: {}#{} • {}",
                issue.repository.name,
                issue.number,
                crate::app::safe_text(&issue.title)
            ),
            SemanticToken::Muted,
        ));
        result.push(row(
            format!("Issue source: {}", issue.url),
            SemanticToken::Muted,
        ));
        for (label, values) in self.after.display_sets_v87() {
            result.push(row(
                format!("{label}: {}", values.len()),
                SemanticToken::Muted,
            ));
            for value in values {
                let (repo, number, url) = value.display_fact_v87();
                result.push(row(
                    format!("{repo}#{number} • {url}"),
                    SemanticToken::Muted,
                ));
            }
        }
        Ok(result)
    }
    pub(crate) fn validate_snapshot_v87(
        &self,
        snapshot: &ConversationDetailSnapshotV19,
    ) -> GithubResult<()> {
        if !Arc::ptr_eq(&self.parent, snapshot.parent())
            || self.plan_digest.len() != 64
            || self
                .metadata
                .as_ref()
                .is_some_and(|ctx| !snapshot.metadata_context_matches_v51(ctx).unwrap_or(false))
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    pub(in crate::providers::gh) fn reserve_box_move_v87(
        &mut self,
        snapshot: &ConversationDetailSnapshotV19,
    ) -> GithubResult<()> {
        self.validate_snapshot_v87(snapshot)?;
        let movement = std::alloc::Layout::new::<Self>()
            .pad_to_align()
            .size()
            .checked_mul(2)
            .and_then(|n| n.checked_add(std::mem::size_of::<Box<Self>>()))
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        self.retained = self
            .retained
            .checked_add(movement)
            .filter(|n| *n <= MODEL_CAP_V87)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        Ok(())
    }
}
impl<'a> IssueLinkPaidReadV87<'a> {
    pub(in crate::providers::gh) fn new(
        provider: &GithubProvider,
        actual: &'a Arc<OriginalIssueLinksV87>,
        plan: &'a IssueLinkPlanV87,
        receipt: &'a IssueLinkReceiptV87,
        captures: &PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        receipt.validate_plan_v87(provider, actual, plan, cancel)?;
        if captures.consumed_requests() != 3
            || captures.reserved_requests() != (if actual.metadata_tab_v87() { 19 } else { 15 })
        {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        // Actual live issuer/env/spec/CString lifetimes for one paid child;
        // the previous request is dead, but Original+receipt+prepared Draft/SQL
        // remain live. Fixed64KiB is role input/SQL/row envelope, not a new cap.
        let retained = receipt
            .retained_bound_v87()
            .checked_add(
                issuer_layout_v87(provider)?
                    .checked_mul(4)
                    .ok_or_else(|| refused(GithubErrorKind::Transport))?,
            )
            .and_then(|n| {
                n.checked_add(64 * KIB + 2 * std::mem::size_of::<ActualIssueLinkPageContextV87>())
            })
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        Ok(Self {
            actual,
            receipt,
            plan,
            heap: IssueLinkHeapV87::new(retained)?,
        })
    }
    fn raw(
        &mut self,
        provider: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        role: IssueLinkPaidRoleV87,
        cancel: &CancellationToken,
    ) -> GithubResult<bounded::IssueDocumentV87> {
        let (_, cap) = role.slot(self.actual.metadata_tab_v87());
        let reservation = self.heap.reserve_document(if cap == 512 * KIB {
            PAIR_CAP_V87
        } else {
            SCALAR_CAP_V87
        })?;
        let output = captures.issue_link_fixed_role_v87(
            provider,
            self.actual,
            self.receipt,
            role,
            cancel,
        )?;
        let doc = self.heap.decode(reservation, &output.stdout)?;
        Ok(doc)
    }
    pub(in crate::providers::gh) fn opening(
        &mut self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        let user = self.raw(
            provider,
            captures,
            IssueLinkPaidRoleV87::OpeningUser,
            cancel,
        )?;
        if user.value()["id"].as_u64() != self.actual.identity.account_id
            || !user.value().is_object()
            || user.value().get("errors").is_some()
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        self.heap.release_document(user)?;
        let repo = self.raw(provider, captures, IssueLinkPaidRoleV87::Repository, cancel)?;
        self.actual.pair.pr_repository.matches_rest(repo.value())?;
        if current.repository().id != self.actual.parent.repository_id()
            || current.repository().node_id.as_deref() != Some(self.actual.parent.repository_node())
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        self.heap.release_document(repo)?;
        let raw = self.raw(
            provider,
            captures,
            IssueLinkPaidRoleV87::OpeningParent,
            cancel,
        )?;
        self.heap.charge(128 * KIB)?; // strict parent/actor/Arc DTO while raw lives
        let parent = provider.parse_paid_issue_parent_v87(
            current,
            self.actual,
            self.receipt,
            raw.value(),
            cancel,
        )?;
        self.heap.release_document(raw)?;
        Ok(parent)
    }
    pub(in crate::providers::gh) fn detail(
        &mut self,
        provider: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        list: ConversationDetailListV19,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<Value>> {
        let reservation = self.heap.reserve_document(PAIR_CAP_V87)?;
        let output = captures.issue_link_fixed_role_v87(
            provider,
            self.actual,
            self.receipt,
            IssueLinkPaidRoleV87::Detail(list),
            cancel,
        )?;
        self.heap.detail_pages_v87(reservation, output)
    }
    pub(in crate::providers::gh) fn closing_summary(
        &mut self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        let raw = self.raw(
            provider,
            captures,
            IssueLinkPaidRoleV87::ClosingParent,
            cancel,
        )?;
        self.heap.charge(128 * KIB)?; // nullable body/summary/actor conversion
        let parent = provider.parse_paid_issue_parent_v87(
            current,
            self.actual,
            self.receipt,
            raw.value(),
            cancel,
        )?;
        let summary =
            ConversationRawSummaryV19::from_actual_raw_v19(provider, parent, raw.value())?;
        self.heap.release_document(raw)?;
        Ok(summary)
    }
    pub(in crate::providers::gh) fn finish_context(
        &mut self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        snapshot: &ConversationDetailSnapshotV19,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<ActualIssueLinkPageContextV87> {
        let snapshot_bytes = snapshot.metadata_model_bytes_v51()?;
        // The actual five-list snapshot stays alive during four context
        // responses, closing pair/Issue decoding and finalUID. Debit it in
        // this same live heap BEFORE the next read, not only in the finished
        // context's descriptive retained scalar.
        self.heap.charge(snapshot_bytes)?;
        let metadata = if self.actual.metadata_tab_v87() {
            let retained = self.heap.retained()?;
            let context = captures.issue_link_metadata_context_v87(
                provider,
                self.actual,
                self.receipt,
                current,
                retained,
                cancel,
            )?;
            if !snapshot.metadata_context_matches_v51(&context)? {
                return Err(refused(GithubErrorKind::StaleHead));
            }
            self.heap.charge(context.retained_bytes())?;
            Some(context)
        } else {
            None
        };
        let pair_doc = self.raw(
            provider,
            captures,
            IssueLinkPaidRoleV87::ClosingPair,
            cancel,
        )?;
        let after = IssueLinkPairV87::parse(
            pair_doc.value(),
            &self.actual.opening,
            &self.actual.pair.issue,
            &provider.host,
            &mut self.heap,
        )?;
        self.receipt
            .validate_closing_pair_v87(self.actual, &after)?;
        self.heap.release_document(pair_doc)?;
        let repository_doc = self.raw(
            provider,
            captures,
            IssueLinkPaidRoleV87::ClosingIssueRepository,
            cancel,
        )?;
        after
            .issue
            .repository
            .matches_rest(repository_doc.value())?;
        self.heap.release_document(repository_doc)?;
        self.heap
            .charge(2 * std::mem::size_of::<ActualIssueLinkPageContextV87>() + 128)?;
        let retained = self.heap.retained()?;
        Ok(ActualIssueLinkPageContextV87 {
            parent: Arc::clone(snapshot.parent()),
            provenance: IssueLinkContextProvenanceV94::Paid(Arc::downgrade(self.actual)),
            command: self.actual.command,
            plan_digest: self.plan.digest.clone(),
            after,
            metadata,
            retained,
        })
    }
    pub(in crate::providers::gh) fn final_uid(
        &mut self,
        provider: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.receipt
            .validate_epoch_v87(provider, self.actual, cancel)?;
        let user = self.raw(provider, captures, IssueLinkPaidRoleV87::FinalUser, cancel)?;
        if user.value()["id"].as_u64() != self.actual.identity.account_id
            || !user.value().is_object()
            || user.value().get("errors").is_some()
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        self.heap.release_document(user)?;
        self.receipt
            .validate_epoch_v87(provider, self.actual, cancel)
    }
}

#[derive(Clone, Copy)]
pub(in crate::providers::gh) enum IssueLinkPaidRoleV87 {
    OpeningUser,
    Repository,
    OpeningParent,
    Detail(ConversationDetailListV19),
    ClosingParent,
    ClosingPair,
    ClosingIssueRepository,
    FinalUser,
}
impl IssueLinkPaidRoleV87 {
    pub(in crate::providers::gh) fn slot(self, metadata: bool) -> (usize, usize) {
        match self {
            Self::OpeningUser => (3, 64 * KIB),
            Self::Repository => (4, 128 * KIB),
            Self::OpeningParent => (5, 128 * KIB),
            Self::Detail(list) => (
                6 + match list {
                    ConversationDetailListV19::Comments => 0,
                    ConversationDetailListV19::Commits => 1,
                    ConversationDetailListV19::Files => 2,
                    ConversationDetailListV19::Reviews => 3,
                    ConversationDetailListV19::Timeline => 4,
                },
                512 * KIB,
            ),
            Self::ClosingParent => (11, 128 * KIB),
            Self::ClosingPair => (if metadata { 16 } else { 12 }, 512 * KIB),
            Self::ClosingIssueRepository => (if metadata { 17 } else { 13 }, 128 * KIB),
            Self::FinalUser => (if metadata { 18 } else { 14 }, 64 * KIB),
        }
    }
    pub(in crate::providers::gh) fn is_list(self) -> bool {
        matches!(self, Self::Detail(_))
    }
    pub(in crate::providers::gh) fn endpoint(
        self,
        provider: &GithubProvider,
        actual: &OriginalIssueLinksV87,
    ) -> GithubResult<String> {
        let parent = actual.parent();
        Ok(match self {
            Self::OpeningUser | Self::FinalUser => "user".into(),
            Self::Repository => format!("repos/{}", parent.repository()),
            Self::OpeningParent | Self::ClosingParent => {
                format!("repos/{}/pulls/{}", parent.repository(), parent.number())
            }
            Self::Detail(list) => list.endpoint_v19(provider, parent)?,
            Self::ClosingPair => "graphql".into(),
            Self::ClosingIssueRepository => format!("repos/{}", actual.pair.issue.repository.name),
        })
    }
    pub(in crate::providers::gh) fn input(
        self,
        actual: &OriginalIssueLinksV87,
    ) -> GithubResult<Option<Vec<u8>>> {
        let input = match self {
            Self::ClosingPair => Some(
                json!({"query":protocol::pair_query(),"variables":{"pr":actual.parent.node_id(),"issue":actual.pair.issue.node}}),
            ),
            _ => None,
        };
        // Fixed query + two canonical <=1024B IDs. The caller's fixed64KiB
        // role envelope is charged before reaching these Value/Vec copies.
        input
            .map(|input| {
                let bytes =
                    serde_json::to_vec(&input).map_err(|_| refused(GithubErrorKind::Json))?;
                if bytes.len() > 16 * KIB || bytes.capacity() > 32 * KIB {
                    return Err(refused(GithubErrorKind::Transport));
                }
                Ok(bytes)
            })
            .transpose()
    }
}
