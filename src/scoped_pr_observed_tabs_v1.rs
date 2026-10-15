//! Pure observed tabs. No transport, SQL, freshness or capability.
use crate::{
    active_remote_scope_v1::{ActiveRemoteScopeV1, RemoteScopeConsumerV1},
    github_workspace::{GithubWorkspace, PrKey, PrTab},
    providers::{gh::PrSummary, runner::CancellationToken},
    repository_catalogue_intent_v1::IntentReference,
    storage::{CatalogueIntentSource, Scope},
};
use serde_json::Value;
use std::sync::Arc;
#[path = "scoped_pr_resources_v1.rs"]
pub mod resources;

pub const MAX_PREPARED_ROWS: usize = 65_536;
pub const MAX_PREPARED_TEXT: usize = 8 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservedPrTabV1 {
    Summary,
    Conversation,
    Commits,
    Timeline,
}
impl ObservedPrTabV1 {
    pub fn from_tab(tab: PrTab) -> Option<Self> {
        match tab {
            PrTab::Summary => Some(Self::Summary),
            PrTab::Conversation => Some(Self::Conversation),
            PrTab::Commits => Some(Self::Commits),
            PrTab::Timeline => Some(Self::Timeline),
            _ => None,
        }
    }
    pub fn tab(self) -> PrTab {
        match self {
            Self::Summary => PrTab::Summary,
            Self::Conversation => PrTab::Conversation,
            Self::Commits => PrTab::Commits,
            Self::Timeline => PrTab::Timeline,
        }
    }
}
/// Immutable NAVIGATION tuple installed only by actual initial PR13 success.
/// No serde/public raw constructor/authentication/current-permission authority.
#[derive(Clone)]
pub struct ScopedPrNavigationV1 {
    scope: Scope,
    original: IntentReference,
    canonical: PrKey,
    node: String,
    head: String,
    base: String,
}
impl std::fmt::Debug for ScopedPrNavigationV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedPrNavigationV1(<captured navigation only; not access>)")
    }
}
impl ScopedPrNavigationV1 {
    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }
    pub(crate) fn original(&self) -> &IntentReference {
        &self.original
    }
    /// Called inside actual admitted initial read, after both genuine brackets.
    pub(crate) fn bind(
        scope: &Scope,
        original: &IntentReference,
        canonical: &PrKey,
        summary: &PrSummary,
    ) -> Result<Self, ObservedPrTabFailureV1> {
        if original.id == 0
            || original.last_name.len() > 256
            || original
                .node_id
                .as_ref()
                .is_some_and(|node| !valid_node(node))
            || canonical.repository.len() > 256
            || PrKey::from(&summary.pr) != *canonical
            || canonical.host != scope.host
            || !valid_node(&summary.pr.id)
            || !full_sha(&summary.head_sha)
            || !full_sha(&summary.base_sha)
        {
            return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
        }
        Ok(Self {
            scope: scope.clone(),
            original: original.clone(),
            canonical: canonical.clone(),
            node: summary.pr.id.clone(),
            head: summary.head_sha.clone(),
            base: summary.base_sha.clone(),
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObservedPrTabFailureV1 {
    Cancelled,
    OriginalScopeUnavailable,
    SelectedPageUnavailable,
    PreparationUnavailable,
}
impl std::fmt::Display for ObservedPrTabFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "Observed PR section cancelled; no private result admitted",
            Self::OriginalScopeUnavailable => {
                "Original observed PR scope unavailable; no section retargeted"
            }
            Self::SelectedPageUnavailable => {
                "Exact captured PR/selected original member unavailable; no section substituted"
            }
            Self::PreparationUnavailable => {
                "Observed PR section preparation unavailable; no new access inferred"
            }
        })
    }
}
impl std::error::Error for ObservedPrTabFailureV1 {}
pub struct ObservedPrTabResultV1 {
    pub page: crate::github_workspace::PrPage,
    pub navigation: Arc<ScopedPrNavigationV1>,
    pub cached_selected: PrKey,
    pub original_source: Arc<CatalogueIntentSource>,
    pub original_active: Arc<ActiveRemoteScopeV1>,
    pub display_limited: bool,
}
/// Page-only move; cached Inbox/selection stays on App for pure Back. No serde,
/// model clone or caller-provided raw navigation constructor.
pub struct ObservedPrTabInputV1 {
    page: crate::github_workspace::PrPage,
    navigation: Arc<ScopedPrNavigationV1>,
    cached_selected: PrKey,
    identity: crate::providers::gh::HostIdentity,
    tab: ObservedPrTabV1,
}
pub fn take_input(
    workspace: &mut GithubWorkspace,
    tab: ObservedPrTabV1,
) -> Result<ObservedPrTabInputV1, ObservedPrTabFailureV1> {
    let navigation = workspace
        .scoped_pr_navigation
        .as_ref()
        .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?;
    let selected = workspace
        .selected
        .as_ref()
        .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?;
    let page = workspace
        .pr
        .as_ref()
        .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?;
    if !workspace.inbox.pull_requests.iter().any(|row| {
        workspace.filter.accepts(row)
            && !workspace.exclusions.contains(&row.repository)
            && row.host == selected.host
            && row.repository == selected.repository
            && row.number == selected.number
    }) || !matches_page(navigation, page)
    {
        return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
    }
    Ok(ObservedPrTabInputV1 {
        navigation: navigation.clone(),
        cached_selected: selected.clone(),
        identity: workspace.identity.clone(),
        page: workspace.pr.take().unwrap(),
        tab,
    })
}
impl std::fmt::Debug for ObservedPrTabResultV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ObservedPrTabResultV1")
            .field("display_limited", &self.display_limited)
            .finish_non_exhaustive()
    }
}

/// Consume the admitted page in a worker; never clone a large workspace in input
/// or draw, call load_tab, invent a new principal or perform I/O for this switch.
pub fn prepare(
    original: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    mut input: ObservedPrTabInputV1,
    cancel: &CancellationToken,
) -> Result<ObservedPrTabResultV1, ObservedPrTabFailureV1> {
    if cancel.is_cancelled() {
        return Err(ObservedPrTabFailureV1::Cancelled);
    }
    let RemoteScopeConsumerV1::Explicit(references) = active.consumer(&input.identity) else {
        return Err(ObservedPrTabFailureV1::OriginalScopeUnavailable);
    };
    let navigation = &input.navigation;
    let cached = &input.cached_selected;
    if Scope::github(&input.identity.host, input.identity.account_id)
        .ok()
        .as_ref()
        != Some(original.scope())
        || &navigation.scope != original.scope()
        || cached.host != navigation.scope.host
        || cached.number != navigation.canonical.number
        || !references.references().iter().any(|reference| {
            reference.original() == &navigation.original
                && reference.observed().is_some_and(|observed| {
                    observed.last_name.eq_ignore_ascii_case(&cached.repository)
                })
        })
        || !matches_page(navigation, &input.page)
    {
        return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
    }
    let limited = !presentation_fits(&input.page, input.tab, cancel)?;
    if !limited {
        input.page.tab = input.tab.tab();
        input
            .page
            .prepare_rows()
            .map_err(|_| ObservedPrTabFailureV1::PreparationUnavailable)?;
    }
    if cancel.is_cancelled() {
        return Err(ObservedPrTabFailureV1::Cancelled);
    }
    Ok(ObservedPrTabResultV1 {
        page: input.page,
        navigation: input.navigation,
        cached_selected: input.cached_selected,
        original_source: original,
        original_active: active,
        display_limited: limited,
    })
}
/// Runtime calls only AFTER matching original owner+generation, before status,
/// privacy or page effects. Back clears metadata, so a late page cannot reopen.
pub fn install_if_current(
    workspace: &mut GithubWorkspace,
    result: ObservedPrTabResultV1,
) -> Result<(), ObservedPrTabFailureV1> {
    if workspace.pr.is_some()
        || workspace
            .scoped_pr_navigation
            .as_ref()
            .is_none_or(|navigation| !Arc::ptr_eq(navigation, &result.navigation))
        || workspace.selected.as_ref() != Some(&result.cached_selected)
        || !matches_page(&result.navigation, &result.page)
    {
        return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
    }
    if !result.display_limited {
        workspace.scoped_pr_resource = None;
    }
    workspace.pr = Some(result.page);
    Ok(())
}
pub(crate) fn matches_page(
    navigation: &ScopedPrNavigationV1,
    page: &crate::github_workspace::PrPage,
) -> bool {
    PrKey::from(&page.details.summary.pr) == navigation.canonical
        && page.details.summary.pr.id == navigation.node
        && page.details.summary.head_sha == navigation.head
        && page.details.summary.base_sha == navigation.base
        && full_sha(&navigation.head)
        && full_sha(&navigation.base)
        && valid_node(&navigation.node)
}
fn full_sha(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn valid_node(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
/// Same borrowed preflight on initial PR13, AFTER its real closing proof and
/// BEFORE initial row allocation. Refusal is a display bound, never revoked
/// access, source deletion or a truncated supposedly complete observation.
pub(crate) fn initial_summary_fits(
    page: &crate::github_workspace::PrPage,
    cancel: &CancellationToken,
) -> Result<bool, ObservedPrTabFailureV1> {
    presentation_fits(page, ObservedPrTabV1::Summary, cancel)
}
struct PresentationBudget {
    rows: usize,
    text: usize,
}
impl PresentationBudget {
    fn add(&mut self, rows: usize, text: usize) -> bool {
        let Some(next_rows) = self.rows.checked_add(rows) else {
            return false;
        };
        let Some(next_text) = self.text.checked_add(text).and_then(|size| {
            rows.checked_mul(128)
                .and_then(|overhead| size.checked_add(overhead))
        }) else {
            return false;
        };
        if next_rows > MAX_PREPARED_ROWS || next_text > MAX_PREPARED_TEXT {
            return false;
        }
        self.rows = next_rows;
        self.text = next_text;
        true
    }
}
/// Conservative borrowed traversal before allocations, never truncation or
/// enumeration authority.128 bytes/row bounds fixed labels/glyph bookkeeping;
/// every variable raw string/JSON structure is additionally charged below.
fn presentation_fits(
    page: &crate::github_workspace::PrPage,
    tab: ObservedPrTabV1,
    cancel: &CancellationToken,
) -> Result<bool, ObservedPrTabFailureV1> {
    let mut budget = PresentationBudget { rows: 0, text: 0 };
    if cancel.is_cancelled() {
        return Err(ObservedPrTabFailureV1::Cancelled);
    }
    let summary = &page.details.summary;
    let header_bytes = [
        &summary.pr.url,
        &summary.head_ref,
        &summary.base_ref,
        &summary.pr.created_at,
        &summary.mergeable_state,
        &summary.head_sha,
        &summary.base_sha,
        &summary.pr.id,
        &summary.pr.title,
        &summary.pr.repository,
        &summary.pr.state,
    ]
    .into_iter()
    .map(String::len)
    .try_fold(0usize, usize::checked_add);
    let header_bytes =
        header_bytes.and_then(|bytes| bytes.checked_add(actor_bytes(&summary.pr.author)?));
    if header_bytes.is_none_or(|bytes| !budget.add(16, bytes)) {
        return Ok(false);
    }
    match tab {
        ObservedPrTabV1::Summary => {
            return Ok(budget.add(summary.pr.body.lines().count(), summary.pr.body.len()))
        }
        ObservedPrTabV1::Conversation => {
            if !budget.add(summary.pr.body.lines().count() + 1, summary.pr.body.len()) {
                return Ok(false);
            }
            for comment in &page.details.comments {
                if cancel.is_cancelled() {
                    return Err(ObservedPrTabFailureV1::Cancelled);
                }
                // Body appears once in visible row labels and once in the ONE
                // shared selection. Actor appears in header + shared payload.
                let bytes = comment
                    .body
                    .len()
                    .checked_mul(2)
                    .and_then(|bytes| bytes.checked_add(comment.updated_at.len().checked_mul(2)?))
                    .and_then(|bytes| bytes.checked_add(comment.created_at.len()))
                    .and_then(|bytes| bytes.checked_add(comment.node_id.len()))
                    .and_then(|bytes| bytes.checked_add(comment.url.len()))
                    .and_then(|bytes| {
                        bytes.checked_add(actor_bytes(&comment.author)?.checked_mul(2)?)
                    });
                if bytes.is_none_or(|bytes| {
                    !budget.add(comment.body.lines().count().saturating_add(2), bytes)
                }) {
                    return Ok(false);
                }
            }
            for review in &page.details.reviews {
                if cancel.is_cancelled() {
                    return Err(ObservedPrTabFailureV1::Cancelled);
                }
                let Some(bytes) = json_upper_bytes(review, 0) else {
                    return Ok(false);
                };
                if !budget.add(
                    review["body"]
                        .as_str()
                        .map_or(0, |body| body.lines().count())
                        .saturating_add(2),
                    bytes,
                ) {
                    return Ok(false);
                }
            }
        }
        ObservedPrTabV1::Commits => {
            for commit in &page.details.commits {
                if cancel.is_cancelled() {
                    return Err(ObservedPrTabFailureV1::Cancelled);
                }
                let bytes = [
                    &commit.sha,
                    &commit.message,
                    &commit.author_name,
                    &commit.committer_name,
                    &commit.authored_at,
                ]
                .into_iter()
                .map(|value| value.len())
                .chain(commit.parents.iter().map(String::len))
                .try_fold(0usize, usize::checked_add)
                .and_then(|bytes| bytes.checked_add(actor_bytes(&commit.author)?))
                .and_then(|bytes| bytes.checked_add(actor_bytes(&commit.committer)?))
                .and_then(|bytes| bytes.checked_add(commit.parents.len().checked_mul(2)?));
                if bytes.is_none_or(|bytes| !budget.add(4, bytes)) {
                    return Ok(false);
                }
            }
        }
        ObservedPrTabV1::Timeline => {
            for event in &page.details.timeline {
                if cancel.is_cancelled() {
                    return Err(ObservedPrTabFailureV1::Cancelled);
                }
                if !crate::github_workspace::timeline_projection_v1::actor_fields_fit(event) {
                    return Ok(false);
                }
                let Some(bytes) = json_upper_bytes(event, 0) else {
                    return Ok(false);
                };
                let rows = crate::github_workspace::timeline_projection_v1::row_upper(event);
                if !budget.add(rows, bytes) {
                    return Ok(false);
                }
            }
        }
    }
    Ok(true)
}
fn actor_bytes(actor: &crate::providers::gh::Actor) -> Option<usize> {
    [
        Some(actor.login.as_str()),
        Some(actor.actor_type.as_str()),
        actor.display_name.as_deref(),
        actor.avatar_url.as_deref(),
    ]
    .into_iter()
    .flatten()
    .map(str::len)
    .try_fold(0usize, usize::checked_add)
}
pub(crate) fn json_upper_bytes(value: &Value, depth: usize) -> Option<usize> {
    if depth > 64 {
        return None;
    }
    match value {
        Value::String(text) => text.len().checked_mul(6)?.checked_add(2),
        Value::Array(values) => values.iter().try_fold(2usize, |total, value| {
            total
                .checked_add(json_upper_bytes(value, depth + 1)?)?
                .checked_add(1)
        }),
        Value::Object(values) => values.iter().try_fold(2usize, |total, (key, value)| {
            total
                .checked_add(key.len().checked_mul(6)?)?
                .checked_add(json_upper_bytes(value, depth + 1)?)?
                .checked_add(4)
        }),
        _ => Some(32),
    }
}

#[cfg(test)]
mod budget_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn scoped_pr_observed_tabs_bounds_checked_overflow_is_atomic_and_never_allocates_rows() {
        let mut budget = PresentationBudget { rows: 0, text: 0 };
        assert!(!budget.add(usize::MAX, usize::MAX));
        assert!(!budget.add(1, MAX_PREPARED_TEXT));
        assert!(!budget.add(MAX_PREPARED_ROWS + 1, 0));
        assert_eq!((budget.rows, budget.text), (0, 0));
        assert!(budget.add(MAX_PREPARED_ROWS, 0));
        assert_eq!(
            (budget.rows, budget.text),
            (MAX_PREPARED_ROWS, MAX_PREPARED_ROWS * 128)
        );
        assert!(!budget.add(1, 0));
        assert_eq!(
            (budget.rows, budget.text),
            (MAX_PREPARED_ROWS, MAX_PREPARED_ROWS * 128)
        );
        let deep = Value::Array(vec![Value::Null]);
        assert_eq!(json_upper_bytes(&deep, 65), None);
        assert_eq!(json_upper_bytes(&json!("🦀"), 0), Some(26));
    }
}
