//! Bounded real Summary/Timeline observation, distinct from a Files resource
//! or protected-history intent. Neither capture nor this raw parent read grants
//! current access; caller owns original session/SQL/current UID closure.
use super::*;
pub struct ThreadDetailParentObservationV4 {
    bounded: ThreadHistoryParentObservationV1,
}
impl ThreadDetailParentObservationV4 {
    pub(crate) fn capture(summary: &PrSummary) -> GithubResult<Self> {
        Ok(Self {
            bounded: ThreadHistoryParentObservationV1::capture(summary)?,
        })
    }
    pub(crate) fn matches(&self, summary: &PrSummary) -> bool {
        self.bounded.matches(summary)
    }
    pub(crate) fn head(&self) -> &str {
        self.bounded.head()
    }
    pub(crate) fn base(&self) -> &str {
        self.bounded.base()
    }
    pub(crate) fn key(&self) -> (&str, &str, u64) {
        self.bounded.key_v4()
    }
}
impl GithubProvider {
    pub(crate) fn pr_thread_detail_parent_v4(
        &self,
        current: &CurrentCatalogueRepository,
        expected: &ThreadDetailParentObservationV4,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        // Reuse only the SAME measured bounded raw-parent parser/child. No
        // protected-history SQL bypass, fresh source or auth fallback exists.
        self.pr_thread_history_parent_v1(current, &expected.bounded, cancel)
    }
}
