//! Summary-origin historical inspection bridge. No Files capability, journal
//! bypass for publication, historical head retargeting or permission constructor.
use super::*;

pub struct ThreadHistoryParentObservationV1 {
    summary: PrSummary,
}
impl std::fmt::Debug for ThreadHistoryParentObservationV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThreadHistoryParentObservationV1(<bounded Summary observation only>)")
    }
}
impl ThreadHistoryParentObservationV1 {
    pub(crate) fn capture(summary: &PrSummary) -> GithubResult<Self> {
        let fields = [
            summary.pr.host.as_str(),
            &summary.pr.repository,
            &summary.pr.id,
            &summary.head_sha,
            &summary.base_sha,
            &summary.head_ref,
            &summary.base_ref,
            &summary.pr.state,
        ];
        if fields.iter().any(|field| field.len() > 1024)
            || !full_sha(&summary.head_sha)
            || !full_sha(&summary.base_sha)
            || !opaque(&summary.pr.id)
            || summary.pr.number == 0
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        // Only bounded scalar observations; NEVER clone the private PR body,
        // comment list, workspace or arbitrary author metadata on foreground.
        Ok(Self {
            summary: PrSummary {
                pr: PullRequest {
                    host: summary.pr.host.clone(),
                    id: summary.pr.id.clone(),
                    repository: summary.pr.repository.clone(),
                    number: summary.pr.number,
                    title: String::new(),
                    body: String::new(),
                    url: String::new(),
                    author: Actor::default(),
                    state: summary.pr.state.clone(),
                    draft: summary.pr.draft,
                    created_at: String::new(),
                    updated_at: String::new(),
                    queues: BTreeSet::new(),
                },
                head_sha: summary.head_sha.clone(),
                base_sha: summary.base_sha.clone(),
                head_ref: summary.head_ref.clone(),
                base_ref: summary.base_ref.clone(),
                merged: summary.merged,
                auto_merge: false,
                merge_commit_sha: None,
                mergeable: None,
                mergeable_state: String::new(),
                additions: 0,
                deletions: 0,
                changed_files: 0,
            },
        })
    }
    pub(crate) fn matches(&self, summary: &PrSummary) -> bool {
        self.summary.pr.host == summary.pr.host
            && self.summary.pr.repository == summary.pr.repository
            && self.summary.pr.id == summary.pr.id
            && self.summary.pr.number == summary.pr.number
            && self.summary.pr.state == summary.pr.state
            && self.summary.pr.draft == summary.pr.draft
            && self.summary.merged == summary.merged
            && self.summary.head_sha == summary.head_sha
            && self.summary.base_sha == summary.base_sha
            && self.summary.head_ref == summary.head_ref
            && self.summary.base_ref == summary.base_ref
    }
    pub(crate) fn head(&self) -> &str {
        &self.summary.head_sha
    }
    pub(crate) fn base(&self) -> &str {
        &self.summary.base_sha
    }
    pub(super) fn key_v4(&self) -> (&str, &str, u64) {
        (
            &self.summary.pr.id,
            &self.summary.pr.repository,
            self.summary.pr.number,
        )
    }
}
impl GithubProvider {
    pub(crate) fn pr_thread_history_parent_v1(
        &self,
        current: &CurrentCatalogueRepository,
        expected: &ThreadHistoryParentObservationV1,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        if current.repository().permissions.pull != Some(true)
            || current.repository().id == 0
            || current
                .repository()
                .node_id
                .as_deref()
                .is_none_or(|node| !opaque(node))
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        let value = document(
            self,
            &format!(
                "repos/{}/pulls/{}",
                current.repository().full_name,
                expected.summary.pr.number
            ),
            128 * 1024,
            cancel,
            true,
        )?;
        parent(self, current, &expected.summary, &value)
    }
    pub(crate) fn same_thread_history_parent_v1(
        first: &PrResourceParentV1,
        last: &PrResourceParentV1,
    ) -> bool {
        same_parent(first, last)
    }
}
