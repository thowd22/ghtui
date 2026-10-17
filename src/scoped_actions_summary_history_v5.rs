//! Actual Summary origin for READ/ACK only. No Files/Run/Review capability is
//! constructed; current H/B close the new parent but never retarget old rows.
use crate::{
    active_remote_scope_v1::{ActiveRemoteScopeV1, RemoteScopeConsumerV1},
    app::{App, View},
    github_workspace::PrTab,
    providers::{gh::*, runner::CancellationToken},
    scoped_pr_observed_tabs_v1::{self, ScopedPrNavigationV1},
    storage::{Cache, CatalogueIntentSource},
};
use std::{path::Path, sync::Arc};
pub struct ActionsSummaryHistoryOriginV5 {
    source: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    navigation: Arc<ScopedPrNavigationV1>,
    expected: ThreadHistoryParentObservationV1,
    selected: usize,
    scroll: usize,
}
impl ActionsSummaryHistoryOriginV5 {
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        use crate::providers::gh::workflow_capacity_v9 as b;
        b::sum([
            b::owner::<Self>(),
            self.source.workflow_owned_bytes_v9()?,
            self.active.workflow_owned_bytes_v9()?,
            self.navigation.workflow_owned_bytes_v9()?,
            self.expected.workflow_owned_bytes_v9()?,
        ])
    }
    pub(crate) fn capture(app: &App) -> Result<Arc<Self>, String> {
        let github = app.github.as_ref().ok_or("Open a genuine scoped Summary")?;
        let page = github
            .pr
            .as_ref()
            .filter(|page| page.tab == PrTab::Summary)
            .ok_or("Actions history opens from the actual Summary, no run is required")?;
        let value = Arc::new(Self {
            source: app
                .remote_scope_source
                .as_ref()
                .ok_or("Original Summary source unavailable")?
                .clone(),
            active: app
                .remote_scope
                .as_ref()
                .ok_or("Original Summary scope unavailable")?
                .clone(),
            navigation: github
                .scoped_pr_navigation
                .as_ref()
                .ok_or("Original Summary navigation unavailable")?
                .clone(),
            expected: ThreadHistoryParentObservationV1::capture(&page.details.summary)
                .map_err(|_| "Bounded actual Summary unavailable")?,
            selected: app.selected,
            scroll: app.scroll,
        });
        if app.view != View::Inbox || app.overlay.is_some() || !value.current(app) {
            return Err("Choose the actual current scoped Summary first".into());
        }
        Ok(value)
    }
    pub(crate) fn source(&self) -> &Arc<CatalogueIntentSource> {
        &self.source
    }
    pub(crate) fn active(&self) -> &Arc<ActiveRemoteScopeV1> {
        &self.active
    }
    pub(crate) fn reference(&self) -> &crate::repository_catalogue_intent_v1::IntentReference {
        self.navigation.original()
    }
    pub(crate) fn return_position(&self) -> (usize, usize) {
        (self.selected, self.scroll)
    }
    pub(crate) fn current(&self, app: &App) -> bool {
        matches!(app.view, View::Inbox | View::Actions)
            && (app.view == View::Actions
                || (app.selected == self.selected && app.scroll == self.scroll))
            && app
                .remote_scope_source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &self.source))
            && app
                .remote_scope
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &self.active))
            && app.github.as_ref().is_some_and(|github| {
                let RemoteScopeConsumerV1::Explicit(members) =
                    self.active.consumer(&github.identity)
                else {
                    return false;
                };
                github.scope().ok().as_ref() == Some(self.source.scope())
                    && members
                        .references()
                        .iter()
                        .any(|member| member.original() == self.navigation.original())
                    && github
                        .scoped_pr_navigation
                        .as_ref()
                        .is_some_and(|nav| Arc::ptr_eq(nav, &self.navigation))
                    && github.pr.as_ref().is_some_and(|page| {
                        page.tab == PrTab::Summary
                            && self.expected.matches(&page.details.summary)
                            && scoped_pr_observed_tabs_v1::matches_page(&self.navigation, page)
                    })
            })
    }
    pub(crate) fn validate_local(&self, database: &Path) -> GithubResult<()> {
        Cache::open(database)
            .and_then(|cache| {
                cache
                    .validate_repository_catalogue_intent_source(&self.source)
                    .map_err(anyhow::Error::from)
            })
            .map_err(|_| GithubError {
                kind: GithubErrorKind::IdentityChanged,
                message: "Original Summary history SQL changed".into(),
            })
    }
    pub(crate) fn fresh(
        &self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        provider.pr_thread_history_parent_v1(current, &self.expected, cancel)
    }
    pub(crate) fn fresh_workflow_edit_history_v9(
        &self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        provider.pr_thread_workflow_history_parent_v9(current, &self.expected, heap, cancel)
    }
}
