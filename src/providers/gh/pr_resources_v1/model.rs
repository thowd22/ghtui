use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrResourceKindV1 {
    Files,
    Checks,
}
#[derive(Clone, Debug)]
pub struct PrResourceReadLimitsV1 {
    pub max_pages_per_source: usize,
    pub max_rows_per_source: usize,
    pub max_document_bytes: usize,
    pub max_patch_bytes: usize,
    pub max_pool_bytes: usize,
}
impl Default for PrResourceReadLimitsV1 {
    fn default() -> Self {
        Self {
            max_pages_per_source: 2,
            max_rows_per_source: 3000,
            // One 100-file page carries per-file patches; ordinary PRs exceed
            // 128 KiB, which previously left the canonical file list unloaded.
            max_document_bytes: 1024 * 1024,
            max_patch_bytes: 1024 * 1024,
            max_pool_bytes: 8 * 1024 * 1024,
        }
    }
}
impl PrResourceReadLimitsV1 {
    pub fn validate(&self) -> GithubResult<()> {
        if !(1..=30).contains(&self.max_pages_per_source)
            || !(100..=3000).contains(&self.max_rows_per_source)
            || !(1..=1024 * 1024).contains(&self.max_document_bytes)
            || !(1..=1024 * 1024).contains(&self.max_patch_bytes)
            || !(1..=32 * 1024 * 1024).contains(&self.max_pool_bytes)
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        Ok(())
    }
    pub fn max_external_requests(&self, kind: PrResourceKindV1) -> usize {
        match kind {
            PrResourceKindV1::Files => 3 + self.max_pages_per_source,
            PrResourceKindV1::Checks => 2 + self.max_pages_per_source * 2,
        }
    }
}
/// Coverage metadata is not a public access/history/required-check constructor.
#[derive(Clone, Debug, Default)]
pub struct PrResourceCoverageV1 {
    pub requested_pages: usize,
    pub fetched_pages: usize,
    pub raw_rows: usize,
    pub admitted_rows: usize,
    pub reported_total: Option<u64>,
    pub enumeration_exhausted: bool,
    pub semantic_complete: bool,
    pub failure: Option<GithubErrorKind>,
    pub reasons: Vec<String>,
}
impl PrResourceCoverageV1 {
    pub fn complete_observed(&self) -> bool {
        self.enumeration_exhausted
            && self.semantic_complete
            && self.failure.is_none()
            && self.reasons.is_empty()
            && self.reported_total == Some(self.admitted_rows as u64)
    }
    pub fn complete_empty(&self) -> bool {
        self.complete_observed() && self.raw_rows == 0
    }
    pub(super) fn reason(&mut self, reason: &'static str) {
        self.semantic_complete = false;
        if self.reasons.len() < 32 && !self.reasons.iter().any(|old| old == reason) {
            self.reasons.push(reason.into());
        }
    }
}
#[derive(Clone, Debug)]
pub struct ObservedPrFileV1 {
    pub path: String,
    pub previous_path: Option<String>,
    pub status: Option<String>,
    pub additions: Option<u64>,
    pub deletions: Option<u64>,
    pub blob_sha: Option<String>,
    pub patch_supplied: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservedPrCheckKindV1 {
    CheckRun,
    CommitStatus,
}
#[derive(Clone, Debug)]
pub struct ObservedPrCheckV1 {
    pub id: u64,
    pub node_id: Option<String>,
    pub kind: ObservedPrCheckKindV1,
    pub head_sha: String,
    pub name: Option<String>,
    pub current_context_unique: bool,
    pub status: Option<String>,
    pub conclusion: Option<String>,
    pub actor: Option<Actor>,
    pub actor_id: Option<u64>,
    pub app_id: Option<u64>,
    pub suite_id: Option<u64>,
    pub created_at: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}
/// Genuine raw current parent; constructor exists only in the transport parser.
/// It is not a write permit and does not certify the final account/local fences.
pub struct PrResourceParentV1 {
    pub(super) summary: PrSummary,
    pub(super) database_id: u64,
    pub(super) author_account_id: Option<u64>,
    pub(super) repository_id: u64,
    pub(super) repository_node: String,
    pub(super) head_repository: Option<(u64, String, String)>,
    pub(super) changed_files: Option<u64>,
}
impl std::fmt::Debug for PrResourceParentV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrResourceParentV1(<actual raw source; no write authority>)")
    }
}
impl PrResourceParentV1 {
    /// Heap owned by this observation only, not another retained source graph.
    pub(crate) fn retained_heap_bytes_v2(&self) -> usize {
        let s = &self.summary;
        let p = &s.pr;
        [&p.host,&p.id,&p.repository,&p.title,&p.body,&p.url,&p.state,&p.created_at,&p.updated_at,
            &s.head_sha,&s.base_sha,&s.head_ref,&s.base_ref,&s.mergeable_state,&self.repository_node,
            &p.author.login,&p.author.actor_type]
            .into_iter().map(String::capacity).sum::<usize>()
            + [&s.merge_commit_sha,&p.author.display_name,&p.author.avatar_url]
                .into_iter().map(|value|value.as_ref().map_or(0,String::capacity)).sum::<usize>()
            + self.head_repository.as_ref().map_or(0,|(_,node,name)|node.capacity()+name.capacity())
            // At most the three typed personal-queue memberships. Charge a
            // conservative full node per member; do not assume packed leaves.
            + p.queues.len()*512
    }
    /// Actual optional raw REST user.id. Unknown/deleted authors stay unknown;
    /// a login, opaque node or Actor presentation never manufactures this ID.
    pub fn author_account_id(&self) -> Option<u64> {
        self.author_account_id
    }
    /// Crate-local journal/identity interoperability only. Non-bound legacy
    /// merge/count fields are not observed permission/rule/capability truth.
    pub(crate) fn summary(&self) -> &PrSummary {
        &self.summary
    }
    pub fn repository(&self) -> &str {
        &self.summary.pr.repository
    }
    pub fn node_id(&self) -> &str {
        &self.summary.pr.id
    }
    pub fn number(&self) -> u64 {
        self.summary.pr.number
    }
    pub fn head_sha(&self) -> &str {
        &self.summary.head_sha
    }
    pub fn base_tip_sha(&self) -> &str {
        &self.summary.base_sha
    }
    pub fn database_id(&self) -> u64 {
        self.database_id
    }
    pub fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub fn repository_node(&self) -> &str {
        &self.repository_node
    }
    pub fn changed_files(&self) -> Option<u64> {
        self.changed_files
    }
}
/// Intermediate actual provider data. Only the owning original session's closing
/// repo + local guard + final principal admit a private prepared page.
pub struct PrResourceReadV1 {
    pub(super) kind: PrResourceKindV1,
    pub(super) parent: PrResourceParentV1,
    pub(super) files: Vec<ObservedPrFileV1>,
    pub(super) checks: Vec<ObservedPrCheckV1>,
    pub(super) statuses: Vec<ObservedPrCheckV1>,
    pub(super) file_coverage: PrResourceCoverageV1,
    pub(super) check_coverage: PrResourceCoverageV1,
    pub(super) status_coverage: PrResourceCoverageV1,
    pub(super) patch: Option<Vec<u8>>,
    pub(super) patch_failure: Option<GithubErrorKind>,
}
impl std::fmt::Debug for PrResourceReadV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrResourceReadV1")
            .field("kind", &self.kind)
            .field("observed_files", &self.files.len())
            .field("observed_checks", &self.checks.len())
            .finish_non_exhaustive()
    }
}
impl PrResourceReadV1 {
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        use crate::providers::gh::workflow_capacity_v9 as b;
        let mut n = b::sum([
            b::owner::<Self>(),
            b::summary(&self.parent.summary)?,
            self.parent.repository_node.capacity(),
            match &self.parent.head_repository {
                Some((_, node, name)) => b::strings([node, name])?,
                None => 0,
            },
            b::vector(&self.files)?,
            b::vector(&self.checks)?,
            b::vector(&self.statuses)?,
            self.patch.as_ref().map_or(0, Vec::capacity),
        ])?;
        for file in &self.files {
            n = n.checked_add(b::strings(
                [
                    Some(&file.path),
                    file.previous_path.as_ref(),
                    file.status.as_ref(),
                    file.blob_sha.as_ref(),
                ]
                .into_iter()
                .flatten(),
            )?)?;
        }
        for check in self.checks.iter().chain(&self.statuses) {
            n = n.checked_add(b::strings(
                [
                    Some(&check.head_sha),
                    check.node_id.as_ref(),
                    check.name.as_ref(),
                    check.status.as_ref(),
                    check.conclusion.as_ref(),
                    check.created_at.as_ref(),
                    check.started_at.as_ref(),
                    check.completed_at.as_ref(),
                ]
                .into_iter()
                .flatten(),
            )?)?;
            if let Some(actor) = &check.actor {
                n = n.checked_add(b::actor(actor)?)?;
            }
        }
        for coverage in [
            &self.file_coverage,
            &self.check_coverage,
            &self.status_coverage,
        ] {
            n = n.checked_add(b::lines(&coverage.reasons)?)?;
        }
        Some(n)
    }
    pub(crate) fn files_capacity_bytes_v63(&self) -> usize {
        self.files.capacity() * std::mem::size_of::<ObservedPrFileV1>()
    }
    pub(crate) fn raw_patch_capacity_bytes_v63(&self) -> usize {
        self.patch.as_ref().map_or(0, Vec::capacity)
    }
    pub fn kind(&self) -> PrResourceKindV1 {
        self.kind
    }
    pub fn parent(&self) -> &PrResourceParentV1 {
        &self.parent
    }
    pub fn files(&self) -> &[ObservedPrFileV1] {
        &self.files
    }
    pub fn checks(&self) -> &[ObservedPrCheckV1] {
        &self.checks
    }
    pub fn statuses(&self) -> &[ObservedPrCheckV1] {
        &self.statuses
    }
    pub fn file_coverage(&self) -> &PrResourceCoverageV1 {
        &self.file_coverage
    }
    pub fn check_coverage(&self) -> &PrResourceCoverageV1 {
        &self.check_coverage
    }
    pub fn status_coverage(&self) -> &PrResourceCoverageV1 {
        &self.status_coverage
    }
    pub fn raw_patch(&self) -> Option<&[u8]> {
        self.patch.as_deref()
    }
    pub fn patch_failure(&self) -> Option<&GithubErrorKind> {
        self.patch_failure.as_ref()
    }
    pub fn required_rules_notice(&self) -> &'static str {
        "Required rules unavailable; observed results are not certified merge blockers"
    }
    pub fn source_limitations(&self) -> &'static str {
        "API-visible latest results; non-atomic, fork association and 1,000-suite limits; not all history"
    }
}
