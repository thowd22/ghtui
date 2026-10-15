//! One real bounded run page. Query intent is not source/access authority.
use super::*;
use std::sync::Arc;
pub const SCOPED_RUN_PAGE_SIZE_V2: usize = 100;
pub const SCOPED_RUN_MAX_PAGE_V2: u32 = 10_000;
#[derive(Clone, Debug)]
pub struct ScopedRunQueryV2 {
    filter: RunFilter,
    page: u32,
}
impl ScopedRunQueryV2 {
    pub fn new(filter: RunFilter, page: u32) -> GithubResult<Arc<Self>> {
        if page == 0 || page > SCOPED_RUN_MAX_PAGE_V2 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Run page must be within the explicit bounded page range",
            ));
        }
        let mut total = 0usize;
        for value in [
            &filter.actor,
            &filter.branch,
            &filter.event,
            &filter.status,
            &filter.head_sha,
            &filter.created,
        ]
        .into_iter()
        .flatten()
        {
            total = total.saturating_add(value.len());
            if value.is_empty() || value.len() > 1024 || value.chars().any(char::is_control) {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Run filter contains an empty, oversized or control-bearing value",
                ));
            }
        }
        if total > 4096 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Run filters exceed the bounded query input budget",
            ));
        }
        if let Some(status) = &filter.status {
            if !matches!(
                status.as_str(),
                "queued"
                    | "in_progress"
                    | "completed"
                    | "requested"
                    | "waiting"
                    | "pending"
                    | "action_required"
                    | "cancelled"
                    | "failure"
                    | "neutral"
                    | "skipped"
                    | "stale"
                    | "success"
                    | "timed_out"
            ) {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Choose an explicit supported run status or conclusion filter",
                ));
            }
        }
        if let Some(sha) = &filter.head_sha {
            validate_sha(sha)?;
        }
        if filter.workflow_id == Some(0) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Workflow filter needs a positive actual ID",
            ));
        }
        Ok(Arc::new(Self { filter, page }))
    }
    pub fn first(filter: RunFilter) -> GithubResult<Arc<Self>> {
        Self::new(filter, 1)
    }
    pub fn filter(&self) -> &RunFilter {
        &self.filter
    }
    pub fn page(&self) -> u32 {
        self.page
    }
    pub fn adjacent(&self, delta: i32) -> GithubResult<Arc<Self>> {
        let page = i64::from(self.page)
            .checked_add(i64::from(delta))
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "No run page in that direction",
                )
            })?;
        Self::new(self.filter.clone(), page)
    }
    pub(crate) fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + 2 * std::mem::size_of::<usize>()
            + [
                &self.filter.actor,
                &self.filter.branch,
                &self.filter.event,
                &self.filter.status,
                &self.filter.head_sha,
                &self.filter.created,
            ]
            .into_iter()
            .flatten()
            .map(String::capacity)
            .fold(0, usize::saturating_add)
    }
}
/// Actual page observation, not all-history absence or an atomic inventory.
pub struct ScopedRunWindowV2 {
    query: Arc<ScopedRunQueryV2>,
    reported: u64,
    loaded: usize,
}
impl ScopedRunWindowV2 {
    pub fn query(&self) -> &Arc<ScopedRunQueryV2> {
        &self.query
    }
    pub fn reported(&self) -> u64 {
        self.reported
    }
    pub fn loaded(&self) -> usize {
        self.loaded
    }
    pub fn next_query(&self) -> Option<Arc<ScopedRunQueryV2>> {
        (self.loaded > 0
            && u64::from(self.query.page).saturating_mul(SCOPED_RUN_PAGE_SIZE_V2 as u64)
                < self.reported)
            .then(|| self.query.adjacent(1).ok())
            .flatten()
    }
    pub fn previous_query(&self) -> Option<Arc<ScopedRunQueryV2>> {
        (self.query.page > 1)
            .then(|| self.query.adjacent(-1).ok())
            .flatten()
    }
    pub(crate) fn owned_bytes(&self) -> usize {
        std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>() + self.query.owned_bytes()
    }
}
pub(super) struct ScopedRunPageReadV2 {
    catalogue: RunCatalog,
    window: Arc<ScopedRunWindowV2>,
}
impl ScopedRunPageReadV2 {
    pub(super) fn into_parts(self) -> (RunCatalog, Arc<ScopedRunWindowV2>) {
        (self.catalogue, self.window)
    }
}
impl GithubProvider {
    pub(super) fn scoped_run_page_v2(
        &self,
        repo: &str,
        query: Arc<ScopedRunQueryV2>,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedRunPageReadV2> {
        let endpoint = format!(
            "{}&page={}",
            run_catalog_endpoint_v2(repo, query.filter())?,
            query.page()
        );
        let raw = self.api(&endpoint, "GET", None, None, false, cancel)?;
        let rows = raw["workflow_runs"].as_array().ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Actual run page omitted its member list",
            )
        })?;
        if rows.len() > SCOPED_RUN_PAGE_SIZE_V2 {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Run page exceeds100 actual members; no hidden truncation",
            ));
        }
        let mut catalogue = parse_run_catalogue_v2(std::slice::from_ref(&raw), query.filter())?;
        if catalogue.runs.len() != rows.len() {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Duplicate actual run IDs make this page ambiguous",
            ));
        }
        if let Some(status) = &query.filter().status {
            if catalogue
                .runs
                .iter()
                .any(|run| run.status != *status && run.conclusion != *status)
            {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    "Actual run page does not match the requested status/conclusion",
                ));
            }
        }
        let reported = catalogue.coverage.total_matches;
        let loaded = catalogue.runs.len();
        catalogue.coverage.complete = query.page() == 1 && loaded as u64 == reported;
        if !catalogue.coverage.complete {
            catalogue.coverage.reasons.push(format!("Observed page{} ({} rows) of {reported} reported matches; partial is not absence and pages are not one atomic snapshot",query.page(),loaded));
        }
        if cancel.is_cancelled() {
            return Err(error(
                GithubErrorKind::Transport,
                "Original run page canceled",
            ));
        }
        Ok(ScopedRunPageReadV2 {
            catalogue,
            window: Arc::new(ScopedRunWindowV2 {
                query,
                reported,
                loaded,
            }),
        })
    }
}
