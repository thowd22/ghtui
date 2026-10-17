//! Actual bounded refresh data, NOT a source/permission/projection admission.
//! Both measured explicit reads and ordinary prepaid postwrite children share
//! these exact fixed-list parsers and the SAME existing raw-parent factory.
use super::*;
use crate::providers::gh::pr_resources_v1::metadata_context_v51::PaidBaseMetadataInputV64;
use repository_catalogue::PostWriteCaptureAllowanceV2;
use std::collections::BTreeSet;
use std::sync::Weak;

const VISIBILITY_V19: [usize; 10] = [
    64 * 1024,
    128 * 1024,
    128 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    128 * 1024,
    64 * 1024,
];
const COMMENT_POST_V19: [usize; 12] = [
    1024 * 1024,
    1024 * 1024,
    VISIBILITY_V19[0],
    VISIBILITY_V19[1],
    VISIBILITY_V19[2],
    VISIBILITY_V19[3],
    VISIBILITY_V19[4],
    VISIBILITY_V19[5],
    VISIBILITY_V19[6],
    VISIBILITY_V19[7],
    VISIBILITY_V19[8],
    VISIBILITY_V19[9],
];
const DELETE_POST_V19: [usize; 12] = [
    64 * 1024,
    128 * 1024,
    VISIBILITY_V19[0],
    VISIBILITY_V19[1],
    VISIBILITY_V19[2],
    VISIBILITY_V19[3],
    VISIBILITY_V19[4],
    VISIBILITY_V19[5],
    VISIBILITY_V19[6],
    VISIBILITY_V19[7],
    VISIBILITY_V19[8],
    VISIBILITY_V19[9],
];
const SUBSCRIPTION_POST_V19: [usize; 12] = [
    128 * 1024,
    128 * 1024,
    VISIBILITY_V19[0],
    VISIBILITY_V19[1],
    VISIBILITY_V19[2],
    VISIBILITY_V19[3],
    VISIBILITY_V19[4],
    VISIBILITY_V19[5],
    VISIBILITY_V19[6],
    VISIBILITY_V19[7],
    VISIBILITY_V19[8],
    VISIBILITY_V19[9],
];
const REACTION_POST_V19: [usize; 15] = [256 * 1024; 15];

pub(crate) fn conversation_post_caps_v19(
    operation: &PrOperation,
) -> GithubResult<&'static [usize]> {
    let legacy = crate::providers::gh::conversation_post_caps_v17(operation)?;
    let caps: &'static [usize] = match operation {
        PrOperation::NewComment(_) | PrOperation::EditComment(_, _) => &COMMENT_POST_V19,
        PrOperation::DeleteComment(_) => &DELETE_POST_V19,
        PrOperation::Subscription(_) => &SUBSCRIPTION_POST_V19,
        PrOperation::AddReaction(_, _) | PrOperation::RemoveReaction(_, _) => &REACTION_POST_V19,
        PrOperation::ViewedFile(_, _) | PrOperation::Merge(_) => {
            return Err(bad(GithubErrorKind::InvalidTarget))
        }
    };
    if caps[..2] != legacy[..2] {
        return Err(bad(GithubErrorKind::InvalidTarget));
    }
    Ok(caps)
}

#[derive(Clone, Copy)]
pub(crate) enum ConversationDetailListV19 {
    Comments,
    Commits,
    Files,
    Reviews,
    Timeline,
}
impl ConversationDetailListV19 {
    const ALL: [Self; 5] = [
        Self::Comments,
        Self::Commits,
        Self::Files,
        Self::Reviews,
        Self::Timeline,
    ];
    pub(in crate::providers::gh) fn endpoint_v19(
        self,
        provider: &GithubProvider,
        parent: &PrResourceParentV1,
    ) -> GithubResult<String> {
        if parent.summary().pr.host != provider.host
            || parent.number() == 0
            || parent.repository_id() == 0
        {
            return Err(bad(GithubErrorKind::IdentityChanged));
        }
        let name = parent.repository();
        let n = parent.number();
        Ok(match self {
            Self::Comments => format!("repos/{name}/issues/{n}/comments?per_page=100"),
            Self::Commits => format!("repos/{name}/pulls/{n}/commits?per_page=100"),
            Self::Files => format!("repos/{name}/pulls/{n}/files?per_page=100"),
            Self::Reviews => format!("repos/{name}/pulls/{n}/reviews?per_page=100"),
            Self::Timeline => format!("repos/{name}/issues/{n}/timeline?per_page=100"),
        })
    }
}

/// Non-Clone/Deserialize; constructor is restricted to the provider family
/// after its actual SAME-response canonical parent parser succeeds.
pub(crate) struct ConversationRawSummaryV19 {
    parent: Arc<PrResourceParentV1>,
    summary: PrSummary,
    digest: [u8; 32],
    body_null_v51: bool,
}
impl ConversationRawSummaryV19 {
    pub(in crate::providers::gh) fn from_actual_raw_v19(
        provider: &GithubProvider,
        parent: PrResourceParentV1,
        raw: &Value,
    ) -> GithubResult<Self> {
        Self::from_actual_arc_data_v48(provider, Arc::new(parent), raw)
    }
    pub(in crate::providers::gh) fn from_members_actual_v48(
        provider: &GithubProvider,
        actual: &OriginalMetadataMembersV48,
    ) -> GithubResult<Self> {
        Self::from_actual_arc_data_v48(
            provider,
            Arc::clone(actual.parent()),
            actual.read().parent_raw(),
        )
    }
    pub(in crate::providers::gh) fn from_members_closing_v48(
        provider: &GithubProvider,
        closing: &crate::providers::gh::pr_resources_v1::MetadataMembersParentV48,
    ) -> GithubResult<Self> {
        Self::from_actual_arc_data_v48(provider, Arc::clone(closing.parent()), closing.raw())
    }
    fn from_actual_arc_data_v48(
        provider: &GithubProvider,
        parent: Arc<PrResourceParentV1>,
        raw: &Value,
    ) -> GithubResult<Self> {
        // No body/default-count inference from parent.summary(), which is an
        // intentionally stripped target expectation. Check borrowed raw fields
        // before the existing display DTO converter makes its bounded copies.
        match raw.get("body") {
            Some(Value::String(body)) if body.len() <= CONVERSATION_BODY_CAP_V17 => {}
            Some(Value::Null) => {}
            _ => return Err(bad(GithubErrorKind::Json)),
        }
        for key in ["title", "created_at", "updated_at", "html_url"] {
            if raw
                .get(key)
                .and_then(Value::as_str)
                .is_none_or(|s| s.len() > 4096)
            {
                return Err(bad(GithubErrorKind::Json));
            }
        }
        for key in ["additions", "deletions", "changed_files"] {
            if raw
                .get(key)
                .and_then(Value::as_u64)
                .is_none_or(|n| n > i64::MAX as u64)
            {
                return Err(error(
                    GithubErrorKind::Unsupported,
                    "Current summary counts unavailable; no zero/default fresh projection inferred",
                ));
            }
        }
        for key in ["login", "name", "avatar_url", "html_url", "type"] {
            if raw["user"]
                .get(key)
                .is_some_and(|v| !v.is_null() && v.as_str().is_none_or(|s| s.len() > 4096))
            {
                return Err(bad(GithubErrorKind::Json));
            }
        }
        for key in ["merge_commit_sha", "mergeable_state"] {
            if raw
                .get(key)
                .is_some_and(|v| !v.is_null() && v.as_str().is_none_or(|s| s.len() > 1024))
            {
                return Err(bad(GithubErrorKind::Json));
            }
        }
        if raw
            .get("mergeable")
            .is_some_and(|v| !v.is_null() && !v.is_boolean())
        {
            return Err(bad(GithubErrorKind::Json));
        }
        let summary =
            provider.pr_summary_from_raw_v19(parent.repository(), parent.number(), raw)?;
        if summary.pr.id != parent.node_id()
            || summary.pr.host != provider.host
            || summary.head_sha != parent.head_sha()
            || summary.base_sha != parent.base_tip_sha()
        {
            return Err(bad(GithubErrorKind::IdentityChanged));
        }
        let digest = summary_digest_v19(&summary)?;
        Ok(Self {
            parent,
            summary,
            digest,
            body_null_v51: raw.get("body") == Some(&Value::Null),
        })
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn body_null_v51(&self) -> bool {
        self.body_null_v51
    }
    /// Move actual bounded raw-derived data only. This cannot issue a source,
    /// Current, receipt or access proof; the original Core must still close.
    pub(in crate::providers::gh) fn into_metadata_parts_v51(
        self,
    ) -> (Arc<PrResourceParentV1>, PrSummary) {
        (self.parent, self.summary)
    }
    pub(crate) fn witness_v19(&self) -> ConversationSummaryWitnessV19 {
        ConversationSummaryWitnessV19 {
            parent: Arc::clone(&self.parent),
            digest: self.digest,
        }
    }
}
/// Comparison only, issued by actual bounded raw conversion. Neither a DTO
/// constructor nor a current parent/source/transport capability.
pub(crate) struct ConversationSummaryWitnessV19 {
    parent: Arc<PrResourceParentV1>,
    digest: [u8; 32],
}
impl ConversationSummaryWitnessV19 {
    pub(in crate::providers::gh) fn matches_snapshot_v61(
        &self,
        snapshot: &ConversationDetailSnapshotV19,
    ) -> GithubResult<bool> {
        Ok(
            self.digest == summary_digest_v19(&snapshot.details.summary)?
                && GithubProvider::same_thread_history_parent_v1(&self.parent, snapshot.parent()),
        )
    }
    pub(crate) fn matches_v19(&self, other: &ConversationRawSummaryV19) -> bool {
        self.digest == other.digest
            && GithubProvider::same_thread_history_parent_v1(&self.parent, &other.parent)
    }
}

// Counting/hash serialization never builds another full body buffer. This
// bound precedes all later clones/derived encodings; after UID no conversion
// or body materialization is permitted at all.
struct SummaryDigestV19 {
    bytes: usize,
    hash: Sha256,
}
impl std::io::Write for SummaryDigestV19 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .filter(|n| *n <= 128 * 1024)
            .ok_or_else(|| std::io::Error::other("Bounded actual summary exceeds128KiB"))?;
        self.hash.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn summary_digest_v19(summary: &PrSummary) -> GithubResult<[u8; 32]> {
    let mut writer = SummaryDigestV19 {
        bytes: 0,
        hash: Sha256::new(),
    };
    serde_json::to_writer(&mut writer, summary).map_err(|_| bad(GithubErrorKind::Json))?;
    Ok(writer.hash.finalize().into())
}

/// No public tuple/DTO constructor and no session/provider getter. This owns
/// only actual bounded detail data, not final local/UID or SQL admission.
pub(crate) struct ConversationDetailSnapshotV19 {
    parent: Arc<PrResourceParentV1>,
    details: PrDetails,
    body_null_v51: bool,
}
/// Genuine bounded same-cursor preparation, not a source or permission. The
/// unique weak marker binds the prepared transaction to THIS final-UID close.
pub(crate) struct ConversationRefreshPreparationV19 {
    parent: Arc<PrResourceParentV1>,
    identity: HostIdentity,
    ticket: Arc<()>,
    cutoff: Instant,
}
impl ConversationRefreshPreparationV19 {
    pub(crate) fn cutoff_v51(&self) -> Instant {
        self.cutoff
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn marker_v19(&self) -> Weak<()> {
        Arc::downgrade(&self.ticket)
    }
}
/// Issued ONLY after this same actual prepaid visibility bracket's final UID.
/// No constructor/Clone/Deserialize/getter for transport or renewed access.
pub(crate) struct ClosedConversationRefreshV19<T> {
    material: T,
    ticket: Arc<()>,
    cutoff: Instant,
}
pub(crate) struct ConversationRefreshCloseSealV19 {
    ticket: Arc<()>,
    cutoff: Instant,
}
impl<T> ClosedConversationRefreshV19<T> {
    pub(crate) fn into_material_v19(self) -> (T, ConversationRefreshCloseSealV19) {
        (
            self.material,
            ConversationRefreshCloseSealV19 {
                ticket: self.ticket,
                cutoff: self.cutoff,
            },
        )
    }
}
impl ConversationRefreshCloseSealV19 {
    pub(crate) fn cutoff_v51(&self) -> Instant {
        self.cutoff
    }
    pub(crate) fn matches_v19(&self, prepared: &Weak<()>) -> bool {
        prepared.ptr_eq(&Arc::downgrade(&self.ticket))
    }
}

/// Lists are prepared before the closing raw PR. No guessed/stripped summary
/// is filled in temporarily: only the actual closing raw response can finish
/// the postwrite snapshot. The explicit read instead supplies its opening
/// actual summary, then compares the complete emitted closing digest.
pub(in crate::providers::gh) struct ConversationDetailListsV19 {
    parent: Arc<PrResourceParentV1>,
    comments: Vec<ConversationComment>,
    commits: Vec<CommitSummary>,
    files: Vec<ChangedFile>,
    reviews: Vec<Value>,
    timeline: Vec<Value>,
}
impl ConversationDetailListsV19 {
    fn metadata_summary_debit_v51(
        &self,
    ) -> GithubResult<
        crate::providers::gh::pr_resources_v1::metadata_context_v51::MetadataRetainedDebitV51,
    > {
        let mut charge = MetadataModelChargeV51(128 * 1024);
        charge.add(std::mem::size_of::<Self>())?;
        charge.add(self.parent.retained_heap_bytes_v2())?;
        charge.lists(
            &self.comments,
            &self.commits,
            &self.files,
            &self.reviews,
            &self.timeline,
        )?;
        Ok(crate::providers::gh::pr_resources_v1::metadata_context_v51::MetadataRetainedDebitV51::from_actual_model_v51(charge.0))
    }
    pub(in crate::providers::gh) fn finish_v19(
        self,
        summary: ConversationRawSummaryV19,
    ) -> GithubResult<ConversationDetailSnapshotV19> {
        if !GithubProvider::same_thread_history_parent_v1(&self.parent, &summary.parent) {
            return Err(bad(GithubErrorKind::IdentityChanged));
        }
        let complete = self.files.len() as u64 >= summary.summary.changed_files;
        let coverage = Coverage {
            loaded: self.files.len(),
            total_matches: summary.summary.changed_files,
            complete,
            reasons: if complete {
                Vec::new()
            } else {
                vec!["GitHub changed-file API coverage is partial; loaded rows do not imply unlimited enumeration".into()]
            },
        };
        Ok(ConversationDetailSnapshotV19 {
            parent: summary.parent,
            body_null_v51: summary.body_null_v51,
            details: PrDetails {
                summary: summary.summary,
                comments: self.comments,
                commits: self.commits,
                files: self.files,
                reviews: self.reviews,
                timeline: self.timeline,
                coverage,
            },
        })
    }
}
impl ConversationDetailSnapshotV19 {
    pub(in crate::providers::gh) fn matches_body_null_v61(&self, expected: bool) -> bool {
        self.body_null_v51 == expected
    }
    pub(crate) fn metadata_summary_debit_v51(
        &self,
        context: Option<&ActualMetadataTabContextV51>,
    ) -> GithubResult<
        crate::providers::gh::pr_resources_v1::metadata_context_v51::MetadataRetainedDebitV51,
    > {
        let bytes = self
            .metadata_model_bytes_v51()?
            .checked_add(context.map_or(0, |value| value.retained_bytes()))
            .filter(|n| *n <= 4 * 1024 * 1024)
            .ok_or_else(|| bad(GithubErrorKind::Transport))?;
        Ok(crate::providers::gh::pr_resources_v1::metadata_context_v51::MetadataRetainedDebitV51::from_actual_model_v51(bytes))
    }
    pub(crate) fn metadata_context_matches_v51(
        &self,
        context: &ActualMetadataTabContextV51,
    ) -> GithubResult<bool> {
        Ok(
            GithubProvider::same_thread_history_parent_v1(&self.parent, context.parent())
                && self.body_null_v51 == context.body_null_v51()
                && summary_digest_v19(&self.details.summary)?
                    == summary_digest_v19(context.summary())?,
        )
    }
    /// Actual retained allocation debit for the new Metadata-only context
    /// profile. Old Summary/Conversation model/transport behavior is unchanged.
    pub(crate) fn metadata_model_bytes_v51(&self) -> GithubResult<usize> {
        let mut charge = MetadataModelChargeV51(128 * 1024); // bounded terminal/SQL preparation scratch
        charge.add(std::mem::size_of::<Self>())?;
        charge.add(self.parent.retained_heap_bytes_v2())?;
        let d = &self.details;
        let s = &d.summary;
        let p = &s.pr;
        for value in [
            &p.host,
            &p.id,
            &p.repository,
            &p.title,
            &p.body,
            &p.url,
            &p.state,
            &p.created_at,
            &p.updated_at,
            &s.head_sha,
            &s.base_sha,
            &s.head_ref,
            &s.base_ref,
            &s.mergeable_state,
        ] {
            charge.string(value)?;
        }
        charge.actor(&p.author)?;
        if let Some(value) = &s.merge_commit_sha {
            charge.string(value)?;
        }
        charge.add(
            p.queues
                .len()
                .checked_mul(512)
                .ok_or_else(|| bad(GithubErrorKind::Transport))?,
        )?;
        charge.add(
            d.comments
                .capacity()
                .checked_mul(std::mem::size_of::<ConversationComment>())
                .ok_or_else(|| bad(GithubErrorKind::Transport))?,
        )?;
        for c in &d.comments {
            for value in [&c.node_id, &c.body, &c.created_at, &c.updated_at, &c.url] {
                charge.string(value)?;
            }
            charge.actor(&c.author)?;
        }
        charge.add(
            d.commits
                .capacity()
                .checked_mul(std::mem::size_of::<CommitSummary>())
                .ok_or_else(|| bad(GithubErrorKind::Transport))?,
        )?;
        for c in &d.commits {
            for value in [
                &c.sha,
                &c.message,
                &c.author_name,
                &c.committer_name,
                &c.authored_at,
            ] {
                charge.string(value)?;
            }
            charge.actor(&c.author)?;
            charge.actor(&c.committer)?;
            charge.add(
                c.parents
                    .capacity()
                    .checked_mul(std::mem::size_of::<String>())
                    .ok_or_else(|| bad(GithubErrorKind::Transport))?,
            )?;
            for parent in &c.parents {
                charge.string(parent)?;
            }
        }
        charge.add(
            d.files
                .capacity()
                .checked_mul(std::mem::size_of::<ChangedFile>())
                .ok_or_else(|| bad(GithubErrorKind::Transport))?,
        )?;
        for file in &d.files {
            charge.string(&file.path)?;
            charge.string(&file.status)?;
            if let Some(value) = &file.previous_path {
                charge.string(value)?;
            }
            if let Some(value) = &file.patch {
                charge.string(value)?;
            }
        }
        for values in [&d.reviews, &d.timeline] {
            charge.add(
                values
                    .capacity()
                    .checked_mul(std::mem::size_of::<Value>())
                    .ok_or_else(|| bad(GithubErrorKind::Transport))?,
            )?;
            for value in values {
                charge.value(value, 0)?;
            }
        }
        charge.add(
            d.coverage
                .reasons
                .capacity()
                .checked_mul(std::mem::size_of::<String>())
                .ok_or_else(|| bad(GithubErrorKind::Transport))?,
        )?;
        for reason in &d.coverage.reasons {
            charge.string(reason)?;
        }
        Ok(charge.0)
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn details(&self) -> &PrDetails {
        &self.details
    }
    pub(crate) fn into_parts_v19(self) -> (Arc<PrResourceParentV1>, PrDetails) {
        (self.parent, self.details)
    }
}
struct MetadataModelChargeV51(usize);
impl MetadataModelChargeV51 {
    fn lists(
        &mut self,
        comments: &Vec<ConversationComment>,
        commits: &Vec<CommitSummary>,
        files: &Vec<ChangedFile>,
        reviews: &Vec<Value>,
        timeline: &Vec<Value>,
    ) -> GithubResult<()> {
        self.add(
            comments
                .capacity()
                .checked_mul(std::mem::size_of::<ConversationComment>())
                .ok_or_else(|| bad(GithubErrorKind::Transport))?,
        )?;
        for comment in comments {
            for value in [
                &comment.node_id,
                &comment.body,
                &comment.created_at,
                &comment.updated_at,
                &comment.url,
            ] {
                self.string(value)?;
            }
            self.actor(&comment.author)?;
        }
        self.add(
            commits
                .capacity()
                .checked_mul(std::mem::size_of::<CommitSummary>())
                .ok_or_else(|| bad(GithubErrorKind::Transport))?,
        )?;
        for commit in commits {
            for value in [
                &commit.sha,
                &commit.message,
                &commit.author_name,
                &commit.committer_name,
                &commit.authored_at,
            ] {
                self.string(value)?;
            }
            self.actor(&commit.author)?;
            self.actor(&commit.committer)?;
            self.add(
                commit
                    .parents
                    .capacity()
                    .checked_mul(std::mem::size_of::<String>())
                    .ok_or_else(|| bad(GithubErrorKind::Transport))?,
            )?;
            for parent in &commit.parents {
                self.string(parent)?;
            }
        }
        self.add(
            files
                .capacity()
                .checked_mul(std::mem::size_of::<ChangedFile>())
                .ok_or_else(|| bad(GithubErrorKind::Transport))?,
        )?;
        for file in files {
            self.string(&file.path)?;
            self.string(&file.status)?;
            for value in [&file.previous_path, &file.patch].into_iter().flatten() {
                self.string(value)?;
            }
        }
        for values in [reviews, timeline] {
            self.add(
                values
                    .capacity()
                    .checked_mul(std::mem::size_of::<Value>())
                    .ok_or_else(|| bad(GithubErrorKind::Transport))?,
            )?;
            for value in values {
                self.value(value, 0)?;
            }
        }
        Ok(())
    }
    fn add(&mut self, bytes: usize) -> GithubResult<()> {
        self.0 = self
            .0
            .checked_add(bytes)
            .filter(|n| *n <= 4 * 1024 * 1024)
            .ok_or_else(|| bad(GithubErrorKind::Transport))?;
        Ok(())
    }
    fn string(&mut self, value: &String) -> GithubResult<()> {
        self.add(value.capacity())
    }
    fn actor(&mut self, actor: &Actor) -> GithubResult<()> {
        self.string(&actor.login)?;
        self.string(&actor.actor_type)?;
        if let Some(value) = &actor.display_name {
            self.string(value)?;
        }
        if let Some(value) = &actor.avatar_url {
            self.string(value)?;
        }
        Ok(())
    }
    fn value(&mut self, value: &Value, depth: usize) -> GithubResult<()> {
        if depth > 64 {
            return Err(bad(GithubErrorKind::Transport));
        }
        match value {
            Value::String(value) => self.string(value),
            Value::Array(values) => {
                self.add(
                    values
                        .capacity()
                        .checked_mul(std::mem::size_of::<Value>())
                        .ok_or_else(|| bad(GithubErrorKind::Transport))?,
                )?;
                for value in values {
                    self.value(value, depth + 1)?;
                }
                Ok(())
            }
            Value::Object(values) => {
                // Conservative complete BTreeMap node/edge/entry allocation
                // per key plus initial leaf, before any new context allocation.
                self.add(
                    values
                        .len()
                        .checked_mul(1024)
                        .and_then(|n| n.checked_add(1024))
                        .ok_or_else(|| bad(GithubErrorKind::Transport))?,
                )?;
                for (key, value) in values {
                    self.string(key)?;
                    self.value(value, depth + 1)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

#[derive(Default)]
struct SnapshotBudgetV19 {
    items: usize,
    bytes: usize,
}
struct CountV19(usize);
impl std::io::Write for CountV19 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= 4 * 1024 * 1024)
            .ok_or_else(|| std::io::Error::other("Actual detail model exceeds4MiB"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl SnapshotBudgetV19 {
    fn charge(&mut self, values: &[Value]) -> GithubResult<()> {
        self.items = self
            .items
            .checked_add(values.len())
            .filter(|n| *n <= 10_000)
            .ok_or_else(|| bad(GithubErrorKind::Transport))?;
        let mut writer = CountV19(self.bytes);
        for value in values {
            serde_json::to_writer(&mut writer, value)
                .map_err(|_| bad(GithubErrorKind::Transport))?;
        }
        self.bytes = writer.0;
        Ok(())
    }
}
fn bounded_text_v19(raw: &Value, key: &str, cap: usize, required: bool) -> GithubResult<()> {
    match raw.get(key) {
        Some(Value::String(s)) if s.len() <= cap => Ok(()),
        None | Some(Value::Null) if !required => Ok(()),
        _ => Err(bad(GithubErrorKind::Json)),
    }
}
fn actor_v19(raw: &Value) -> GithubResult<()> {
    if raw.is_null() {
        return Ok(());
    }
    if !raw.is_object() {
        return Err(bad(GithubErrorKind::Json));
    }
    for key in ["login", "name", "avatar_url", "html_url", "type"] {
        bounded_text_v19(raw, key, 4096, false)?;
    }
    if raw
        .get("id")
        .is_some_and(|id| !id.is_null() && positive(id).is_none())
    {
        return Err(bad(GithubErrorKind::Json));
    }
    Ok(())
}
fn sha_v19(raw: &Value) -> bool {
    raw.as_str()
        .is_some_and(|s| matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit()))
}
fn list_identities_v19(list: ConversationDetailListV19, values: &[Value]) -> GithubResult<()> {
    let mut numbers = BTreeSet::new();
    let mut nodes = BTreeSet::new();
    let mut paths = BTreeSet::new();
    for value in values {
        if !value.is_object() {
            return Err(bad(GithubErrorKind::Json));
        }
        match list {
            ConversationDetailListV19::Comments | ConversationDetailListV19::Reviews => {
                let id = positive(&value["id"]).ok_or_else(|| bad(GithubErrorKind::Json))?;
                let node = value["node_id"]
                    .as_str()
                    .filter(|s| opaque(s))
                    .ok_or_else(|| bad(GithubErrorKind::Json))?;
                if !numbers.insert(id) || !nodes.insert(node) {
                    return Err(bad(GithubErrorKind::Json));
                }
                match value.get("body") {
                    Some(Value::String(s)) if s.len() <= CONVERSATION_BODY_CAP_V17 => {}
                    Some(Value::Null) => {}
                    _ => return Err(bad(GithubErrorKind::Json)),
                }
                actor_v19(&value["user"])?;
                for key in [
                    "created_at",
                    "updated_at",
                    "submitted_at",
                    "html_url",
                    "state",
                    "commit_id",
                ] {
                    bounded_text_v19(value, key, 4096, false)?;
                }
            }
            ConversationDetailListV19::Commits => {
                let sha = value["sha"]
                    .as_str()
                    .filter(|s| {
                        matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit())
                    })
                    .ok_or_else(|| bad(GithubErrorKind::Json))?;
                if !nodes.insert(sha) {
                    return Err(bad(GithubErrorKind::Json));
                }
                bounded_text_v19(&value["commit"], "message", CONVERSATION_BODY_CAP_V17, true)?;
                actor_v19(&value["author"])?;
                actor_v19(&value["committer"])?;
                for who in ["author", "committer"] {
                    for key in ["name", "date"] {
                        bounded_text_v19(&value["commit"][who], key, 4096, false)?;
                    }
                }
                let parents = value
                    .get("parents")
                    .and_then(Value::as_array)
                    .filter(|p| p.len() <= 128)
                    .ok_or_else(|| bad(GithubErrorKind::Json))?;
                if parents.iter().any(|p| !sha_v19(&p["sha"])) {
                    return Err(bad(GithubErrorKind::Json));
                }
            }
            ConversationDetailListV19::Files => {
                let path = value["filename"]
                    .as_str()
                    .filter(|s| !s.is_empty() && s.len() <= 4096 && !s.contains('\0'))
                    .ok_or_else(|| bad(GithubErrorKind::Json))?;
                if !paths.insert(path) {
                    return Err(bad(GithubErrorKind::Json));
                }
                bounded_text_v19(value, "previous_filename", 4096, false)?;
                bounded_text_v19(value, "patch", 1024 * 1024, false)?;
                bounded_text_v19(value, "status", 128, true)?;
                for key in ["additions", "deletions"] {
                    if value
                        .get(key)
                        .and_then(Value::as_u64)
                        .is_none_or(|n| n > i64::MAX as u64)
                    {
                        return Err(bad(GithubErrorKind::Json));
                    }
                }
            }
            ConversationDetailListV19::Timeline => {
                if let Some(id) = positive(&value["id"]) {
                    if !numbers.insert(id) {
                        return Err(bad(GithubErrorKind::Json));
                    }
                }
                if let Some(node) = value.get("node_id").and_then(Value::as_str) {
                    if !opaque(node) || !nodes.insert(node) {
                        return Err(bad(GithubErrorKind::Json));
                    }
                }
                actor_v19(&value["actor"])?;
                bounded_text_v19(value, "event", 256, false)?;
                bounded_text_v19(value, "created_at", 4096, false)?;
            }
        }
    }
    Ok(())
}
fn collect_v19(
    parent: Arc<PrResourceParentV1>,
    mut read: impl FnMut(ConversationDetailListV19, &PrResourceParentV1) -> GithubResult<Vec<Value>>,
) -> GithubResult<ConversationDetailListsV19> {
    // The summary already passed the stricter128KiB bounded DTO serializer.
    // Charge that full conservative maximum rather than materializing a copy.
    let mut budget = SnapshotBudgetV19 {
        bytes: 128 * 1024,
        ..Default::default()
    };
    let mut details = ConversationDetailListsV19 {
        parent,
        comments: Vec::new(),
        commits: Vec::new(),
        files: Vec::new(),
        reviews: Vec::new(),
        timeline: Vec::new(),
    };
    for list in ConversationDetailListV19::ALL {
        let values = read(list, &details.parent)?;
        budget.charge(&values)?;
        list_identities_v19(list, &values)?;
        match list {
            ConversationDetailListV19::Comments => {
                details.comments = values.iter().map(comment_from_json).collect()
            }
            ConversationDetailListV19::Commits => {
                details.commits = values.iter().map(commit_from_json).collect()
            }
            ConversationDetailListV19::Files => {
                if values.len() > 3000 {
                    return Err(bad(GithubErrorKind::Transport));
                }
                details.files = values.iter().map(file_from_json).collect();
            }
            ConversationDetailListV19::Reviews => details.reviews = values,
            ConversationDetailListV19::Timeline => details.timeline = values,
        }
    }
    Ok(details)
}

#[derive(Clone, Copy)]
enum ConfirmedRefreshTargetV25<'a> {
    Conversation(&'a FreshConversationPublicationV17),
    StateV68(&'a OriginalPrStateV68, &'a StateReceiptV68),
    IssueLinksV87(
        &'a Arc<OriginalIssueLinksV87>,
        &'a IssueLinkPlanV87,
        &'a IssueLinkReceiptV87,
    ),
    Metadata(
        &'a crate::providers::gh::OriginalMetadataTextV25,
        &'a crate::providers::gh::PrMetadataTextReceiptV25,
    ),
    MetadataTabV51(&'a OriginalMetadataTextV25, &'a PrMetadataTextReceiptV25),
    MembersV48(
        &'a Arc<OriginalMetadataMembersV48>,
        &'a crate::pr_metadata_actions::PrMetadataPlan,
        &'a PrMetadataMembersReceiptV48,
    ),
    MembersMetadataV62(
        &'a Arc<OriginalMetadataMembersV48>,
        &'a crate::pr_metadata_actions::PrMetadataPlan,
        &'a PrMetadataMembersReceiptV48,
    ),
    Base(
        &'a OriginalMetadataBaseV32,
        &'a BaseSelectionV32,
        &'a PrMetadataBaseReceiptV32,
    ),
    BaseMetadataTabV51(
        &'a OriginalMetadataBaseV32,
        &'a BaseSelectionV32,
        &'a PrMetadataBaseReceiptV32,
    ),
}
impl ConfirmedRefreshTargetV25<'_> {
    fn publication_cutoff_v51(&self) -> Instant {
        match self {
            Self::Conversation(actual) => actual.publication_cutoff_v51(),
            Self::StateV68(actual, receipt) => actual
                .publication_cutoff_v68()
                .min(receipt.publication_cutoff_v68()),
            Self::IssueLinksV87(actual, _, receipt) => actual.deadline().min(receipt.deadline()),
            Self::Metadata(actual, receipt) | Self::MetadataTabV51(actual, receipt) => actual
                .publication_cutoff_v51()
                .min(receipt.publication_cutoff_v51()),
            Self::MembersV48(actual, _, receipt) | Self::MembersMetadataV62(actual, _, receipt) => {
                actual.deadline().min(receipt.deadline())
            }
            Self::Base(actual, _, receipt) | Self::BaseMetadataTabV51(actual, _, receipt) => {
                actual.deadline().min(receipt.publication_cutoff_v51())
            }
        }
    }
    fn identity(&self) -> &HostIdentity {
        match self {
            Self::Conversation(v) => v.identity(),
            Self::StateV68(v, _) => v.identity(),
            Self::IssueLinksV87(_, _, receipt) => receipt.identity(),
            Self::Metadata(v, _) | Self::MetadataTabV51(v, _) => v.identity(),
            Self::Base(v, _, _) | Self::BaseMetadataTabV51(v, _, _) => v.identity(),
            Self::MembersV48(_, _, receipt) | Self::MembersMetadataV62(_, _, receipt) => {
                receipt.identity()
            }
        }
    }
    fn parent(&self) -> &Arc<PrResourceParentV1> {
        match self {
            Self::Conversation(v) => v.parent(),
            Self::StateV68(_, receipt) => receipt.parent(),
            Self::IssueLinksV87(_, _, receipt) => receipt.parent(),
            Self::Metadata(v, _) | Self::MetadataTabV51(v, _) => v.observed().parent(),
            Self::Base(_, _, receipt) | Self::BaseMetadataTabV51(_, _, receipt) => receipt.parent(),
            Self::MembersV48(_, _, receipt) | Self::MembersMetadataV62(_, _, receipt) => {
                receipt.parent()
            }
        }
    }
    fn validate_v25(
        &self,
        captures: &PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if let Self::StateV68(actual, receipt) = self {
            if captures.consumed_requests() != 2
                || captures.reserved_requests() != STATE_POST_CAPS_V68.len()
                || captures.reserved_bytes() != STATE_POST_CAPS_V68.iter().sum::<usize>()
                || !receipt.validates_refresh(actual, cancel)
            {
                return Err(bad(GithubErrorKind::InvalidTarget));
            }
        }
        if let Self::IssueLinksV87(actual, _, _) = self {
            if captures.consumed_requests() != 3
                || captures.reserved_requests() != if actual.metadata_tab_v87() { 19 } else { 15 }
            {
                return Err(bad(GithubErrorKind::InvalidTarget));
            }
        }
        if let Self::MembersV48(actual, plan, receipt)
        | Self::MembersMetadataV62(actual, plan, receipt) = self
        {
            if captures.consumed_requests() != 6 || !receipt.validates_refresh(actual, plan, cancel)
            {
                return Err(bad(GithubErrorKind::InvalidTarget));
            }
        }
        if let Self::MembersMetadataV62(actual, _, _) = self {
            if !actual.metadata_tab_v62() || captures.reserved_requests() != 22 {
                return Err(bad(GithubErrorKind::InvalidTarget));
            }
        }
        if let Self::Metadata(actual, receipt) | Self::MetadataTabV51(actual, receipt) = self {
            if captures.consumed_requests() != 2 || !receipt.validates_refresh(actual, cancel) {
                return Err(bad(GithubErrorKind::InvalidTarget));
            }
        }
        if matches!(
            self,
            Self::MetadataTabV51(..) | Self::BaseMetadataTabV51(..)
        ) && captures.reserved_requests() != 16
        {
            return Err(bad(GithubErrorKind::InvalidTarget));
        }
        if let Self::Base(actual, selected, receipt)
        | Self::BaseMetadataTabV51(actual, selected, receipt) = self
        {
            if captures.consumed_requests() != 2
                || !receipt.validates_refresh(actual, selected, cancel)
            {
                return Err(bad(GithubErrorKind::InvalidTarget));
            }
        }
        Ok(())
    }
}
// V64 stack carrier: one common snapshot and the already approved closed
// None|Members|Metadata supplement. No new context, allocation or purpose.
struct ConfirmedRefreshDataV51 {
    snapshot: ConversationDetailSnapshotV19,
    supplement: ConversationPageSupplementV61,
}
/// Display-only closed supplements. No generic DTO, Source/Current, transport,
/// serialized capability, or independently selectable purpose is represented.
pub(crate) enum ConversationPageSupplementV61 {
    None,
    Members(Box<ActualMembersPageContextV61>),
    Metadata(Box<ActualMetadataTabContextV51>),
    IssueLinksV87(Box<ActualIssueLinkPageContextV87>),
}
impl ConversationPageSupplementV61 {
    pub(crate) fn issue_links_v87(
        snapshot: &ConversationDetailSnapshotV19,
        mut context: ActualIssueLinkPageContextV87,
    ) -> GithubResult<Self> {
        context.reserve_box_move_v87(snapshot)?;
        Ok(Self::IssueLinksV87(Box::new(context)))
    }
    pub(crate) fn members_v79(
        snapshot: &ConversationDetailSnapshotV19,
        mut context: ActualMembersPageContextV61,
    ) -> GithubResult<Self> {
        context.reserve_box_move_v79(snapshot)?;
        Ok(Self::Members(Box::new(context)))
    }
    pub(crate) fn metadata_v79(
        snapshot: &ConversationDetailSnapshotV19,
        mut context: ActualMetadataTabContextV51,
    ) -> GithubResult<Self> {
        context.reserve_box_move_v79(snapshot)?;
        Ok(Self::Metadata(Box::new(context)))
    }
}
pub(crate) struct MembersRefreshV48<'a> {
    pub(crate) actual: &'a Arc<OriginalMetadataMembersV48>,
    pub(crate) plan: &'a crate::pr_metadata_actions::PrMetadataPlan,
    pub(crate) receipt: &'a PrMetadataMembersReceiptV48,
    pub(crate) current: &'a CurrentCatalogueRepository,
    pub(crate) captures: &'a mut PostWriteCaptureAllowanceV2,
    pub(crate) cancel: &'a CancellationToken,
    pub(crate) deadline: Instant,
}
pub(crate) struct StateRefreshV68<'a> {
    pub(crate) actual: &'a OriginalPrStateV68,
    pub(crate) receipt: &'a StateReceiptV68,
    pub(crate) current: &'a CurrentCatalogueRepository,
    pub(crate) captures: &'a mut PostWriteCaptureAllowanceV2,
    pub(crate) cancel: &'a CancellationToken,
    pub(crate) deadline: Instant,
}
pub(crate) struct IssueLinkRefreshV87<'a> {
    pub(crate) actual: &'a Arc<OriginalIssueLinksV87>,
    pub(crate) plan: &'a IssueLinkPlanV87,
    pub(crate) receipt: &'a IssueLinkReceiptV87,
    pub(crate) current: &'a CurrentCatalogueRepository,
    pub(crate) captures: &'a mut PostWriteCaptureAllowanceV2,
    pub(crate) cancel: &'a CancellationToken,
    pub(crate) deadline: Instant,
}

impl GithubProvider {
    pub(crate) fn prepare_confirmed_issue_link_refresh_v87<T>(
        &self,
        request: IssueLinkRefreshV87<'_>,
        prepare: impl FnOnce(
            ConversationDetailSnapshotV19,
            ConversationPageSupplementV61,
            &ConversationRefreshPreparationV19,
        ) -> GithubResult<T>,
    ) -> GithubResult<ClosedConversationRefreshV19<T>> {
        let IssueLinkRefreshV87 {
            actual,
            plan,
            receipt,
            current,
            captures,
            cancel,
            deadline,
        } = request;
        self.prepare_confirmed_refresh_target_v25(
            ConfirmedRefreshTargetV25::IssueLinksV87(actual, plan, receipt),
            current,
            captures,
            cancel,
            deadline,
            |data, preparation| match data.supplement {
                supplement @ ConversationPageSupplementV61::IssueLinksV87(_) => {
                    prepare(data.snapshot, supplement, preparation)
                }
                _ => Err(bad(GithubErrorKind::InvalidTarget)),
            },
        )
    }

    pub(crate) fn prepare_confirmed_state_refresh_v68<T>(
        &self,
        request: StateRefreshV68<'_>,
        prepare: impl FnOnce(
            ConversationDetailSnapshotV19,
            &ConversationRefreshPreparationV19,
        ) -> GithubResult<T>,
    ) -> GithubResult<ClosedConversationRefreshV19<T>> {
        let StateRefreshV68 {
            actual,
            receipt,
            current,
            captures,
            cancel,
            deadline,
        } = request;
        self.prepare_confirmed_refresh_target_v25(
            ConfirmedRefreshTargetV25::StateV68(actual, receipt),
            current,
            captures,
            cancel,
            deadline,
            |data, preparation| match data {
                ConfirmedRefreshDataV51 {
                    snapshot,
                    supplement: ConversationPageSupplementV61::None,
                } => prepare(snapshot, preparation),
                _ => Err(bad(GithubErrorKind::InvalidTarget)),
            },
        )
    }
    pub(crate) fn prepare_confirmed_members_metadata_refresh_v62<T>(
        &self,
        request: MembersRefreshV48<'_>,
        prepare: impl FnOnce(
            ConversationDetailSnapshotV19,
            ConversationPageSupplementV61,
            &ConversationRefreshPreparationV19,
        ) -> GithubResult<T>,
    ) -> GithubResult<ClosedConversationRefreshV19<T>> {
        let MembersRefreshV48 {
            actual,
            plan,
            receipt,
            current,
            captures,
            cancel,
            deadline,
        } = request;
        self.prepare_confirmed_refresh_target_v25(
            ConfirmedRefreshTargetV25::MembersMetadataV62(actual, plan, receipt),
            current,
            captures,
            cancel,
            deadline.min(actual.deadline()).min(receipt.deadline()),
            |data, preparation| match data {
                ConfirmedRefreshDataV51 {
                    snapshot,
                    supplement: ConversationPageSupplementV61::Metadata(context),
                } if context.members_command_v62() == Some(plan.command()) => prepare(
                    snapshot,
                    ConversationPageSupplementV61::Metadata(context),
                    preparation,
                ),
                _ => Err(bad(GithubErrorKind::InvalidTarget)),
            },
        )
    }
    pub(crate) fn prepare_confirmed_metadata_members_refresh_v48<T>(
        &self,
        request: MembersRefreshV48<'_>,
        prepare: impl FnOnce(
            ConversationDetailSnapshotV19,
            ConversationPageSupplementV61,
            &ConversationRefreshPreparationV19,
        ) -> GithubResult<T>,
    ) -> GithubResult<ClosedConversationRefreshV19<T>> {
        let MembersRefreshV48 {
            actual,
            plan,
            receipt,
            current,
            captures,
            cancel,
            deadline,
        } = request;
        self.prepare_confirmed_refresh_target_v25(
            ConfirmedRefreshTargetV25::MembersV48(actual, plan, receipt),
            current,
            captures,
            cancel,
            deadline.min(receipt.deadline()).min(actual.deadline()),
            |data, preparation| match data {
                ConfirmedRefreshDataV51 {
                    snapshot,
                    supplement: ConversationPageSupplementV61::Members(context),
                } => prepare(
                    snapshot,
                    ConversationPageSupplementV61::Members(context),
                    preparation,
                ),
                _ => Err(bad(GithubErrorKind::InvalidTarget)),
            },
        )
    }
    /// SAME ordinary prepaid cursor, AFTER the caller's actual typed receipt
    /// and durable Confirmed marker. All preparation/local witnesses/cleanup
    /// execute before this bracket's final original UID. No new access/Core,
    /// allowance reset, generic API getter or after-UID body conversion.
    pub(crate) fn prepare_confirmed_refresh_v19<T>(
        &self,
        actual: &FreshConversationPublicationV17,
        current: &CurrentCatalogueRepository,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
        prepare: impl FnOnce(
            ConversationDetailSnapshotV19,
            &ConversationRefreshPreparationV19,
        ) -> GithubResult<T>,
    ) -> GithubResult<ClosedConversationRefreshV19<T>> {
        self.prepare_confirmed_refresh_target_v25(
            ConfirmedRefreshTargetV25::Conversation(actual),
            current,
            captures,
            cancel,
            deadline,
            |data, preparation| match data {
                ConfirmedRefreshDataV51 {
                    snapshot,
                    supplement: ConversationPageSupplementV61::None,
                } => prepare(snapshot, preparation),
                _ => Err(bad(GithubErrorKind::InvalidTarget)),
            },
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Closed actual metadata receipt and original visibility cursor stay separate; no target or provider constructor"
    )]
    pub(crate) fn prepare_confirmed_metadata_refresh_v25<T>(
        &self,
        actual: &crate::providers::gh::OriginalMetadataTextV25,
        receipt: &crate::providers::gh::PrMetadataTextReceiptV25,
        current: &CurrentCatalogueRepository,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
        prepare: impl FnOnce(
            ConversationDetailSnapshotV19,
            &ConversationRefreshPreparationV19,
        ) -> GithubResult<T>,
    ) -> GithubResult<ClosedConversationRefreshV19<T>> {
        self.prepare_confirmed_refresh_target_v25(
            ConfirmedRefreshTargetV25::Metadata(actual, receipt),
            current,
            captures,
            cancel,
            deadline,
            |data, preparation| match data {
                ConfirmedRefreshDataV51 {
                    snapshot,
                    supplement: ConversationPageSupplementV61::None,
                } => prepare(snapshot, preparation),
                _ => Err(bad(GithubErrorKind::InvalidTarget)),
            },
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Closed actual Base selection/receipt and same prepaid visibility cursor remain explicit"
    )]
    pub(crate) fn prepare_confirmed_base_refresh_v32<T>(
        &self,
        actual: &OriginalMetadataBaseV32,
        selected: &BaseSelectionV32,
        receipt: &PrMetadataBaseReceiptV32,
        current: &CurrentCatalogueRepository,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
        prepare: impl FnOnce(
            ConversationDetailSnapshotV19,
            &ConversationRefreshPreparationV19,
        ) -> GithubResult<T>,
    ) -> GithubResult<ClosedConversationRefreshV19<T>> {
        self.prepare_confirmed_refresh_target_v25(
            ConfirmedRefreshTargetV25::Base(actual, selected, receipt),
            current,
            captures,
            cancel,
            deadline,
            |data, preparation| match data {
                ConfirmedRefreshDataV51 {
                    snapshot,
                    supplement: ConversationPageSupplementV61::None,
                } => prepare(snapshot, preparation),
                _ => Err(bad(GithubErrorKind::InvalidTarget)),
            },
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Purpose-closed actual text receipt and same fixed16 cursor; no provider/source constructor"
    )]
    pub(crate) fn prepare_confirmed_metadata_tab_refresh_v51<T>(
        &self,
        actual: &OriginalMetadataTextV25,
        receipt: &PrMetadataTextReceiptV25,
        current: &CurrentCatalogueRepository,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
        prepare: impl FnOnce(
            ConversationDetailSnapshotV19,
            ActualMetadataTabContextV51,
            &ConversationRefreshPreparationV19,
        ) -> GithubResult<T>,
    ) -> GithubResult<ClosedConversationRefreshV19<T>> {
        self.prepare_confirmed_refresh_target_v25(
            ConfirmedRefreshTargetV25::MetadataTabV51(actual, receipt),
            current,
            captures,
            cancel,
            deadline,
            |data, preparation| match data {
                ConfirmedRefreshDataV51 {
                    snapshot,
                    supplement: ConversationPageSupplementV61::Metadata(context),
                } => prepare(snapshot, *context, preparation),
                _ => Err(bad(GithubErrorKind::InvalidTarget)),
            },
        )
    }
    #[expect(
        clippy::too_many_arguments,
        reason = "Closed sealed Base receipt and its actual selection on the original16 prepaid cursor"
    )]
    pub(crate) fn prepare_confirmed_base_metadata_refresh_v51<T>(
        &self,
        actual: &OriginalMetadataBaseV32,
        selected: &BaseSelectionV32,
        receipt: &PrMetadataBaseReceiptV32,
        current: &CurrentCatalogueRepository,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
        prepare: impl FnOnce(
            ConversationDetailSnapshotV19,
            ActualMetadataTabContextV51,
            &ConversationRefreshPreparationV19,
        ) -> GithubResult<T>,
    ) -> GithubResult<ClosedConversationRefreshV19<T>> {
        self.prepare_confirmed_refresh_target_v25(
            ConfirmedRefreshTargetV25::BaseMetadataTabV51(actual, selected, receipt),
            current,
            captures,
            cancel,
            deadline,
            |data, preparation| match data {
                ConfirmedRefreshDataV51 {
                    snapshot,
                    supplement: ConversationPageSupplementV61::Metadata(context),
                } => prepare(snapshot, *context, preparation),
                _ => Err(bad(GithubErrorKind::InvalidTarget)),
            },
        )
    }
    fn prepare_confirmed_refresh_target_v25<T>(
        &self,
        actual: ConfirmedRefreshTargetV25<'_>,
        current: &CurrentCatalogueRepository,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
        prepare: impl FnOnce(
            ConfirmedRefreshDataV51,
            &ConversationRefreshPreparationV19,
        ) -> GithubResult<T>,
    ) -> GithubResult<ClosedConversationRefreshV19<T>> {
        actual.validate_v25(captures, cancel)?;
        let deadline = deadline
            .min(captures.deadline())
            .min(actual.publication_cutoff_v51())
            .min(
                self.scoped_read_window
                    .map_or(captures.deadline(), |(cutoff, _)| cutoff),
            );
        live(cancel, deadline.min(captures.deadline()))?;
        // Narrow only this new Conversation opening bracket. The shared
        // legacy four-child Review caller retains its original request caps.
        let opening_provider = self.clone().with_scoped_read_window(
            self.scoped_read_window
                .map_or(captures.deadline(), |(clock, _)| {
                    clock.min(captures.deadline())
                }),
            self.scoped_read_window
                .map_or(128 * 1024, |(_, cap)| cap.min(128 * 1024)),
        )?;
        let mut issue_read_v87 =
            if let ConfirmedRefreshTargetV25::IssueLinksV87(actual, plan, receipt) = actual {
                Some(
                    crate::providers::gh::pr_issue_links_v87::refresh::IssueLinkPaidReadV87::new(
                        self, actual, plan, receipt, captures, cancel,
                    )?,
                )
            } else {
                None
            };
        let mut members_context: Option<MetadataMembersContextV48> = None;
        let mut members_heap_v62 = if let ConfirmedRefreshTargetV25::MembersMetadataV62(
            actual,
            _,
            _,
        ) = actual
        {
            Some(crate::providers::gh::pr_resources_v1::metadata_context_v51::MetadataMembersHeapV62::new(
                actual.retained_model_v62()?.checked_add(64*1024).ok_or_else(||bad(GithubErrorKind::Transport))?)?)
        } else {
            None
        };
        let opening = match actual {
            ConfirmedRefreshTargetV25::IssueLinksV87(_, _, _) => issue_read_v87
                .as_mut()
                .ok_or_else(|| bad(GithubErrorKind::InvalidTarget))?
                .opening(self, current, captures, cancel)?,
            ConfirmedRefreshTargetV25::MembersV48(observation, plan, receipt)
            | ConfirmedRefreshTargetV25::MembersMetadataV62(observation, plan, receipt) => {
                let opening = if let Some(heap) = members_heap_v62.as_mut() {
                    opening_provider.open_members_metadata_visibility_v62(
                        MembersVisibilityV48 {
                            actual: observation,
                            plan,
                            receipt,
                            cancel,
                            deadline,
                        },
                        current,
                        captures,
                        heap,
                    )?
                } else {
                    opening_provider.open_members_postwrite_visibility_v48(
                        receipt, current, captures, cancel, deadline,
                    )?
                };
                let (parent, data) = opening.into_member_parts_v48();
                members_context = Some(self.prepare_members_context_v48(
                    MembersVisibilityV48 {
                        actual: observation,
                        plan,
                        receipt,
                        cancel,
                        deadline,
                    },
                    data,
                    captures,
                    members_heap_v62.as_mut(),
                )?);
                parent
            }
            ConfirmedRefreshTargetV25::Base(_, _, receipt)
            | ConfirmedRefreshTargetV25::BaseMetadataTabV51(_, _, receipt) => opening_provider
                .open_base_postwrite_visibility_v32(
                    actual.identity(),
                    current,
                    receipt.transition(),
                    captures,
                    cancel,
                    deadline,
                )?,
            _ => opening_provider.open_review_postwrite_visibility_v19(
                actual.identity(),
                current,
                actual.parent(),
                captures,
                cancel,
                deadline,
            )?,
        };
        let lists = if let Some(read) = issue_read_v87.as_mut() {
            collect_v19(Arc::new(opening), |list, _| {
                read.detail(self, captures, list, cancel)
            })?
        } else if let (
            ConfirmedRefreshTargetV25::MembersMetadataV62(observed, _, _),
            Some(heap),
            Some(context),
        ) = (actual, members_heap_v62.as_mut(), members_context.as_ref())
        {
            collect_v19(Arc::new(opening), |list, _| {
                captures
                    .members_metadata_detail_list_v62(self, observed, context, list, heap, cancel)
            })?
        } else {
            self.collect_conversation_details_post_v19(Arc::new(opening), captures, cancel)?
        };
        let mut metadata_context_v48 = None;
        let mut members_retained_v62 = None;
        let summary = match actual {
            ConfirmedRefreshTargetV25::IssueLinksV87(_, _, _) => issue_read_v87
                .as_mut()
                .ok_or_else(|| bad(GithubErrorKind::InvalidTarget))?
                .closing_summary(self, current, captures, cancel)?,
            ConfirmedRefreshTargetV25::MetadataTabV51(actual, receipt) => captures
                .metadata_text_closing_summary_v51(
                    self,
                    actual,
                    receipt,
                    current,
                    lists.metadata_summary_debit_v51()?,
                    cancel,
                )?,
            ConfirmedRefreshTargetV25::BaseMetadataTabV51(actual, selected, receipt) => captures
                .base_metadata_closing_summary_v51(
                    self,
                    PaidBaseMetadataInputV64 {
                        actual,
                        selection: selected,
                        receipt,
                    },
                    current,
                    lists.metadata_summary_debit_v51()?,
                    cancel,
                )?,
            ConfirmedRefreshTargetV25::MembersV48(_, _, _) => {
                let (summary, context) = self.finish_members_summary_v48(
                    current,
                    members_context
                        .take()
                        .ok_or_else(|| bad(GithubErrorKind::InvalidTarget))?,
                    captures,
                    cancel,
                )?;
                metadata_context_v48 = Some(context);
                summary
            }
            ConfirmedRefreshTargetV25::MembersMetadataV62(_, _, _) => {
                let (summary, retained) = self.finish_members_metadata_summary_v62(
                    current,
                    members_context
                        .take()
                        .ok_or_else(|| bad(GithubErrorKind::InvalidTarget))?,
                    captures,
                    cancel,
                    lists.metadata_summary_debit_v51()?,
                )?;
                members_retained_v62 = Some(retained);
                summary
            }
            ConfirmedRefreshTargetV25::Base(_, _, receipt) => {
                self.base_closing_summary_v32(current, receipt.transition(), captures, cancel)?
            }
            _ => {
                self.conversation_closing_summary_v19(current, actual.parent(), captures, cancel)?
            }
        };
        if let ConfirmedRefreshTargetV25::Metadata(_, receipt)
        | ConfirmedRefreshTargetV25::MetadataTabV51(_, receipt) = actual
        {
            let value = match receipt.field() {
                crate::providers::gh::PrMetadataTextFieldV25::Title => {
                    summary.summary.pr.title.as_str()
                }
                crate::providers::gh::PrMetadataTextFieldV25::Description => {
                    summary.summary.pr.body.as_str()
                }
            };
            if !receipt.matches_value(value) {
                return Err(bad(GithubErrorKind::AmbiguousWrite));
            }
        }
        let preparation = ConversationRefreshPreparationV19 {
            parent: Arc::clone(summary.parent()),
            identity: actual.identity().clone(),
            ticket: Arc::new(()),
            cutoff: deadline,
        };
        let data = if let Some(read) = issue_read_v87.as_mut() {
            let snapshot = lists.finish_v19(summary)?;
            let context = read.finish_context(self, current, &snapshot, captures, cancel)?;
            let supplement = ConversationPageSupplementV61::issue_links_v87(&snapshot, context)?;
            ConfirmedRefreshDataV51 {
                snapshot,
                supplement,
            }
        } else if let ConfirmedRefreshTargetV25::MetadataTabV51(actual, receipt) = actual {
            let expected_digest = summary.digest;
            let expected_null = summary.body_null_v51;
            let snapshot = lists.finish_v19(summary)?;
            let retained = snapshot.metadata_model_bytes_v51()?;
            let context = captures
                .metadata_tab_context_v51(self, actual, receipt, current, retained, cancel)?;
            if summary_digest_v19(context.summary())? != expected_digest
                || context.body_null_v51() != expected_null
            {
                return Err(bad(GithubErrorKind::StaleHead));
            }
            let supplement = ConversationPageSupplementV61::metadata_v79(&snapshot, context)?;
            ConfirmedRefreshDataV51 {
                snapshot,
                supplement,
            }
        } else if let ConfirmedRefreshTargetV25::BaseMetadataTabV51(actual, selected, receipt) =
            actual
        {
            let expected_digest = summary.digest;
            let expected_null = summary.body_null_v51;
            let snapshot = lists.finish_v19(summary)?;
            let retained = snapshot.metadata_model_bytes_v51()?;
            let context = captures.base_metadata_context_v51(
                self,
                PaidBaseMetadataInputV64 {
                    actual,
                    selection: selected,
                    receipt,
                },
                current,
                retained,
                cancel,
            )?;
            if summary_digest_v19(context.summary())? != expected_digest
                || context.body_null_v51() != expected_null
            {
                return Err(bad(GithubErrorKind::StaleHead));
            }
            let supplement = ConversationPageSupplementV61::metadata_v79(&snapshot, context)?;
            ConfirmedRefreshDataV51 {
                snapshot,
                supplement,
            }
        } else if let ConfirmedRefreshTargetV25::MembersMetadataV62(actual, plan, receipt) = actual
        {
            let snapshot = lists.finish_v19(summary)?;
            let retained = snapshot
                .metadata_model_bytes_v51()?
                .checked_add(members_retained_v62.ok_or_else(|| bad(GithubErrorKind::Transport))?)
                .filter(|bytes| *bytes <= 4 * 1024 * 1024)
                .ok_or_else(|| bad(GithubErrorKind::Transport))?;
            let context = captures.members_metadata_context_v62(
                self,
                MembersMetadataContextInputV62 {
                    actual,
                    plan,
                    receipt,
                    current,
                    snapshot: &snapshot,
                    retained,
                },
                cancel,
            )?;
            let supplement = ConversationPageSupplementV61::metadata_v79(&snapshot, context)?;
            ConfirmedRefreshDataV51 {
                snapshot,
                supplement,
            }
        } else if let Some(context) = metadata_context_v48 {
            let snapshot = lists.finish_v19(summary)?;
            context.validate_snapshot_v61(&snapshot)?;
            let supplement = ConversationPageSupplementV61::members_v79(&snapshot, context)?;
            ConfirmedRefreshDataV51 {
                snapshot,
                supplement,
            }
        } else {
            ConfirmedRefreshDataV51 {
                snapshot: lists.finish_v19(summary)?,
                supplement: ConversationPageSupplementV61::None,
            }
        };
        let mut metadata_final_heap_v62 = match (&data, actual) {
            (
                ConfirmedRefreshDataV51 {
                    snapshot,
                    supplement: ConversationPageSupplementV61::Metadata(context),
                },
                ConfirmedRefreshTargetV25::MembersMetadataV62(observed, _, _),
            ) => {
                let retained = snapshot
                    .metadata_model_bytes_v51()?
                    .checked_add(context.retained_bytes())
                    .and_then(|bytes| {
                        bytes.checked_add(
                            context
                                .members_producer_overlap_v62(observed.command())
                                .ok()?,
                        )
                    })
                    .filter(|bytes| *bytes <= 4 * 1024 * 1024)
                    .ok_or_else(|| bad(GithubErrorKind::Transport))?;
                Some(crate::providers::gh::pr_resources_v1::metadata_context_v51::MetadataMembersHeapV62::new(retained)?)
            }
            _ => None,
        };
        let prepared = prepare(data, &preparation)?;
        live(cancel, deadline.min(captures.deadline()))?;
        if let ConfirmedRefreshTargetV25::MembersMetadataV62(observed, _, receipt) = actual {
            receipt.validate_postwrite_epoch_v78(self, observed, cancel)?;
        }
        if let Some(read) = issue_read_v87.as_mut() {
            read.final_uid(self, captures, cancel)?;
        } else if let (Some(heap), ConfirmedRefreshTargetV25::MembersMetadataV62(observed, _, _)) =
            (metadata_final_heap_v62.as_mut(), actual)
        {
            let user = captures.members_metadata_role_v62(
                self,
                observed,
                MembersMetadataRoleV62::FinalUser,
                heap,
                cancel,
            )?;
            if user["id"].as_u64() != actual.identity().account_id {
                return Err(bad(GithubErrorKind::IdentityChanged));
            }
            live(cancel, deadline.min(captures.deadline()))?;
        } else {
            self.finish_review_postwrite_visibility_v19(
                actual.identity(),
                captures,
                cancel,
                deadline,
            )?;
        }
        if let ConfirmedRefreshTargetV25::MembersMetadataV62(observed, _, receipt) = actual {
            receipt.validate_postwrite_epoch_v78(self, observed, cancel)?;
        }
        Ok(ClosedConversationRefreshV19 {
            material: prepared,
            ticket: preparation.ticket,
            cutoff: preparation.cutoff,
        })
    }
    pub(in crate::providers::gh) fn collect_conversation_details_post_v19(
        &self,
        parent: Arc<PrResourceParentV1>,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationDetailListsV19> {
        collect_v19(parent, |list, parent| {
            captures.conversation_detail_list_v19(self, parent, list, cancel)
        })
    }
    pub(crate) fn collect_conversation_details_measured_v19(
        &self,
        summary: ConversationRawSummaryV19,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationDetailSnapshotV19> {
        let (_, deadline) = measured(self, cancel)?;
        let lists = collect_v19(Arc::clone(summary.parent()), |list, parent| {
            live(cancel, deadline)?;
            let raw = self.api(
                &list.endpoint_v19(self, parent)?,
                "GET",
                None,
                None,
                true,
                cancel,
            )?;
            if raw.as_array().is_none_or(Vec::is_empty) {
                return Err(bad(GithubErrorKind::Json));
            }
            Self::api_list_pages_v19(raw, None)
        })?;
        lists.finish_v19(summary)
    }
    pub(crate) fn collect_issue_link_manual_details_v94(
        &self,
        summary: ConversationRawSummaryV19,
        read: &mut crate::providers::gh::IssueLinkManualReadV94,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationDetailSnapshotV19> {
        read.validate_live_v94(self, cancel)?;
        let lists = collect_v19(Arc::clone(summary.parent()), |list, _| {
            read.detail_v94(self, list, cancel)
        })?;
        lists.finish_v19(summary)
    }
    pub(in crate::providers::gh) fn collect_members_metadata_details_measured_v62(
        &self,
        summary: ConversationRawSummaryV19,
        actual: &OriginalMetadataMembersV48,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationDetailSnapshotV19> {
        if !actual.metadata_tab_v62()
            || !GithubProvider::same_thread_history_parent_v1(actual.parent(), summary.parent())
        {
            return Err(bad(GithubErrorKind::IdentityChanged));
        }
        actual.validate_metadata_measured_v62(self, cancel)?;
        let mut heap=crate::providers::gh::pr_resources_v1::metadata_context_v51::MetadataMembersHeapV62::new(
            actual.retained_model_v62()?.checked_add(128*1024+64*1024).ok_or_else(||bad(GithubErrorKind::Transport))?)?;
        let lists = collect_v19(Arc::clone(summary.parent()), |list, parent| {
            actual.validate_metadata_measured_v62(self, cancel)?;
            let endpoint = list.endpoint_v19(self, parent)?;
            let mut spec = self.spec(vec![
                "api".into(),
                endpoint,
                "--hostname".into(),
                self.host.clone(),
                "--method".into(),
                "GET".into(),
                "--paginate".into(),
                "--slurp".into(),
            ]);
            spec.output_limit = spec.output_limit.min(512 * 1024);
            let output = self
                .runner
                .run(spec, cancel)
                .map_err(|failure| bad(GithubError::from(failure).kind))?;
            actual.validate_metadata_measured_v62(self, cancel)?;
            heap.detail_pages(&output.stdout)
        })?;
        lists.finish_v19(summary)
    }
}
