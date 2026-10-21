//! Navigation-intent stops, not repository access or permission.
//! A tagged live scope needs its own genuine current-ID route; this helper
//! never turns observed/saved names into an authorized legacy repository Vec.
use crate::{
    active_remote_scope_v1::{
        ActiveRemoteScopeV1, ExplicitRemoteRepositoriesV1, RemoteScopeConsumerV1, RemoteScopeStopV1,
    },
    app::{App, Overlay, View},
    providers::gh::HostIdentity,
    runtime::Background,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteConsumerV1 {
    MyPrs,
    Actions,
    Search,
    Dashboard,
    Corpus,
    Watch,
    SourceLoad,
    RemoteForm,
}
impl RemoteConsumerV1 {
    pub const ALL: [Self; 8] = [
        Self::MyPrs,
        Self::Actions,
        Self::Search,
        Self::Dashboard,
        Self::Corpus,
        Self::Watch,
        Self::SourceLoad,
        Self::RemoteForm,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::MyPrs => "My PRs",
            Self::Actions => "Actions",
            Self::Search => "Search",
            Self::Dashboard => "Dashboard",
            Self::Corpus => "Repository corpus",
            Self::Watch => "Actions Watch",
            Self::SourceLoad => "Search sources",
            Self::RemoteForm => "Remote form",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopeEntryStopV1 {
    Intent(RemoteScopeStopV1),
    /// No claim that observed refs were denied: a dedicated scoped worker must
    /// retain original source and perform current-ID/principal/SQL/window checks.
    ScopedRouteRequired,
}
impl ScopeEntryStopV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::Intent(RemoteScopeStopV1::ExplicitEmpty) =>
                "Explicit repository scope is empty • no fallback or query sent",
            Self::Intent(RemoteScopeStopV1::Unresolved) =>
                "Original repository scope is unresolved • no narrowing, fallback or query sent",
            Self::Intent(RemoteScopeStopV1::Fenced(_)) =>
                "Original repository scope is fenced • private pages hidden • no fallback or query sent",
            Self::ScopedRouteRequired =>
                "Current ID-bound repository access required • legacy fallback not used • no query sent",
        }
    }
}

/// STOP-only preflight works even after private identity is gone. None from
/// here is NOT admission; live tagged workers must inspect a fresh principal.
pub fn stop_before_fallback(active: Option<&ActiveRemoteScopeV1>) -> Option<ScopeEntryStopV1> {
    active
        .and_then(ActiveRemoteScopeV1::blocked_intent)
        .map(ScopeEntryStopV1::Intent)
}

/// Allows only unchanged pre-catalogue startup through legacy entry points.
/// The caller must use a dedicated original-bound route for ANY Some scope.
/// Deliberate DefaultPersonal still needs its original numeric-scope guard,
/// because legacy refresh can otherwise rebase after its inner identity read.
pub fn legacy_entry(active: Option<&ActiveRemoteScopeV1>) -> Result<(), ScopeEntryStopV1> {
    match active {
        None => Ok(()),
        Some(active) => {
            Err(stop_before_fallback(Some(active)).unwrap_or(ScopeEntryStopV1::ScopedRouteRequired))
        }
    }
}
/// Borrowed navigation decision only. An actual current provider/session
/// identity belongs to the worker; this does not grant API/SQL access or permit
/// a saved query to rewrite the active original refs.
pub enum CurrentNavigationV1<'a> {
    LegacyStartup,
    DefaultPersonalIntent,
    ExplicitRequiresCurrentAccess(ExplicitRemoteRepositoriesV1<'a>),
    Stopped(ScopeEntryStopV1),
}
pub fn after_current_identity<'a>(
    active: Option<&'a ActiveRemoteScopeV1>,
    current: &HostIdentity,
) -> CurrentNavigationV1<'a> {
    let Some(active) = active else {
        return CurrentNavigationV1::LegacyStartup;
    };
    match active.consumer(current) {
        RemoteScopeConsumerV1::DefaultPersonal => CurrentNavigationV1::DefaultPersonalIntent,
        RemoteScopeConsumerV1::NotificationPr(_) => {
            CurrentNavigationV1::Stopped(ScopeEntryStopV1::ScopedRouteRequired)
        }
        RemoteScopeConsumerV1::Explicit(repositories) => {
            CurrentNavigationV1::ExplicitRequiresCurrentAccess(repositories)
        }
        RemoteScopeConsumerV1::ExplicitEmpty => {
            CurrentNavigationV1::Stopped(ScopeEntryStopV1::Intent(RemoteScopeStopV1::ExplicitEmpty))
        }
        RemoteScopeConsumerV1::Unresolved(_) => {
            CurrentNavigationV1::Stopped(ScopeEntryStopV1::Intent(RemoteScopeStopV1::Unresolved))
        }
        RemoteScopeConsumerV1::Fenced(reason) => CurrentNavigationV1::Stopped(
            ScopeEntryStopV1::Intent(RemoteScopeStopV1::Fenced(reason)),
        ),
    }
}

/// Existing selected local paths/results only, never a request to rebuild from
/// Config or assign a repository-ID mapping. Metadata inspection reads no body.
pub fn independent_local_search(
    search: Option<&crate::search::workspace::SearchWorkspace>,
) -> bool {
    use crate::search::{model::SourceKind, SearchMode};
    search.is_some_and(|search| {
        search.scope.host == "local"
            && matches!(
                search.mode,
                SearchMode::Exact | SearchMode::Regex | SearchMode::Ranked | SearchMode::Fuzzy
            )
            && !search.blend_policy.enabled
            && search.remote_identity.is_none()
            && search.remote_items.is_empty()
            && search.blend_evidence.is_none()
            && search.metadata_sources.discussions.is_empty()
            && search.page.as_ref().is_none_or(|page| {
                page.hits.iter().all(|hit| {
                    hit.document.key.scope == search.scope
                        && matches!(
                            hit.document.source,
                            SourceKind::Worktree | SourceKind::SavedCommit
                        )
                })
            })
    })
}
fn local_search_action(action: Option<&crate::search::workspace::SearchAction>) -> bool {
    use crate::search::{workspace::SearchAction, SearchMode};
    matches!(
        action,
        None | Some(
            SearchAction::Query
                | SearchAction::Submit
                | SearchAction::Worktree
                | SearchAction::Revision
                | SearchAction::Preview(_)
                | SearchAction::Open(_)
                | SearchAction::PreviewScroll(_)
                | SearchAction::Mode(
                    SearchMode::Exact | SearchMode::Regex | SearchMode::Ranked | SearchMode::Fuzzy
                )
        )
    )
}

/// Retire private presentation only when the exact original source/scope pair
/// is still current but its numeric account is lost. No I/O or new authority;
/// independently newer sources and durable drafts/receipts remain untouched.
pub(crate) fn retire_original_account_mismatch(
    app: &mut App,
    source: &std::sync::Arc<crate::storage::CatalogueIntentSource>,
    active: &std::sync::Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>,
) -> bool {
    let original_account_lost = app
        .remote_scope_source
        .as_ref()
        .is_some_and(|current| std::sync::Arc::ptr_eq(current, source))
        && app
            .remote_scope
            .as_ref()
            .is_some_and(|current| std::sync::Arc::ptr_eq(current, active))
        && app
            .github
            .as_ref()
            .is_some_and(|github| github.scope().ok().as_ref() != Some(source.scope()));
    if !original_account_lost {
        return false;
    }
    crate::scoped_pr_observed_tabs_v1::resources::threads_v1::native_v1::retire_private(app);
    app.pr_file_images = None;
    crate::repository_catalogue_native::privacy(app);
    crate::repository_catalogue_native::retire_remote_views(app);
    let keep_local = app.view == View::Repository
        || app.view == View::Search && independent_local_search(app.search.as_ref());
    if !keep_local {
        let reason = ScopeEntryStopV1::Intent(RemoteScopeStopV1::Fenced(
            crate::active_remote_scope_v1::RemoteScopeFenceV1::PrivacyFence,
        ))
        .label();
        app.context = reason.into();
        app.status = reason.into();
        app.reset_lists_v92();
        app.focus = 0;
        app.github_content_focused = false;
    }
    app.hits.clear();
    true
}

/// UI/job retirement only: no provider, SQL, Git, refresh, reserve, claim/ACK or
/// intent persistence. Never fences/resets a merely empty/unresolved scope or
/// destroys the live catalogue source/owner. Durable drafts/receipts untouched.
pub fn retire_remote(app: &mut App, background: &mut Background, reason: ScopeEntryStopV1) {
    crate::scoped_pr_observed_tabs_v1::resources::threads_v1::native_v1::retire_private(app);
    let keep_local_search = independent_local_search(app.search.as_ref());
    let keep_local_request = keep_local_search
        && local_search_action(app.search_action.as_ref())
        && app.source_action.is_none()
        && !app.corpus_requested;
    for view in [View::Inbox, View::Actions, View::Search, View::Dashboard] {
        if view == View::Search && keep_local_request {
            continue;
        }
        background.cancel(view);
        app.loading[view.index()] = false;
        if view != View::Search || !keep_local_search {
            app.content[view.index()] = vec![reason.label().into()];
        }
    }
    crate::pr_checks_native::retire(app, true);
    let _ = app.navigation_trail.remote_identity(None);
    app.github = None;
    app.github_origin = None;
    app.github_action = None;
    app.pr_action = None;
    app.pr_inspection = None;
    app.pr_pending_message.clear();
    app.pr_reactions = None;
    app.pr_reaction_display.clear();
    app.pr_reaction_scroll = 0;
    app.pr_file_images = None;
    app.actions = None; // This also drops Watch; late producer was cancelled.
    app.actions_inspection = None;
    app.actions_action = None;
    app.actions_load_requested = false;
    if keep_local_search {
        app.search.as_mut().unwrap().remote_repositories.clear();
        if !keep_local_request {
            app.search_action = None;
            app.search_load_requested = false;
        }
    } else {
        app.search = None;
        app.search_action = None;
        app.search_load_requested = false;
        app.search_controls = Default::default();
    }
    app.source_action = None;
    app.source_controls = Default::default();
    app.source_context = None;
    app.source_context_scroll = 0;
    app.source_context_visible = 0;
    app.corpus_requested = false;
    app.metrics = None;
    app.metrics_action = None;
    app.metrics_load_requested = false;
    app.metrics_content = Default::default();
    app.metrics_visible_rows = 0;
    if matches!(
        reason,
        ScopeEntryStopV1::Intent(RemoteScopeStopV1::Fenced(_))
    ) {
        app.remote_scope_source = None;
    }
    if app
        .form
        .as_ref()
        .is_some_and(|form| form.view() != View::Repository)
    {
        app.form = None;
        app.form_dismissed = true;
        if app.overlay == Some(Overlay::Form) {
            app.overlay = None;
        }
    }
    if matches!(
        app.overlay,
        Some(
            Overlay::PrActions
                | Overlay::PrFileImages
                | Overlay::PrReactions
                | Overlay::SearchSources
                | Overlay::SearchContext
                | Overlay::SearchBlend
                | Overlay::SearchRankDetails
                | Overlay::Notice
        )
    ) {
        app.overlay = None;
    }
    app.notice.clear();
    app.notice_scroll = 0;
    app.clear_capability_inspector();
    app.retire_command_context();
    app.avatar_states.clear();
    app.hits.clear();
    // Retain local workspace/preview/form/loading/requests and breadcrumb. The
    // retired remote Search is not rebuilt from every Config mapping/current.
    if app.view != View::Repository && !(app.view == View::Search && keep_local_search) {
        // Old private-page scroll must not hide the replacement explanation.
        // Preserve the independent local Repository/Search presentation.
        app.reset_lists_v92();
        app.focus = 0;
        app.github_content_focused = false;
        app.refresh_requested = false;
        app.context = reason.label().into();
        app.status = reason.label().into();
    }
}
pub fn enforce_stopped_intent(app: &mut App, background: &mut Background) -> bool {
    let Some(reason) = stop_before_fallback(app.remote_scope.as_deref()) else {
        return false;
    };
    let local = independent_local_search(app.search.as_ref());
    let local_request = local && local_search_action(app.search_action.as_ref());
    let pending = app.github.is_some()
        || app.github_action.is_some()
        || app.pr_action.is_some()
        || app.pr_checks_origin.is_some()
        || app.pr_checks_route.is_some()
        || app.pr_checks_request
        || app.actions.is_some()
        || app.actions_action.is_some()
        || app.actions_load_requested
        || app.metrics.is_some()
        || app.metrics_action.is_some()
        || app.metrics_load_requested
        || (!local && app.search.is_some())
        || (!local_request && app.search_action.is_some())
        || (!local && app.search_load_requested)
        || app.source_action.is_some()
        || app.corpus_requested
        || app
            .form
            .as_ref()
            .is_some_and(|form| form.view() != View::Repository)
        || app.view != View::Repository
            && !(app.view == View::Search && local_request)
            && app.refresh_requested;
    // TASK-105: the explicit principal-first Personal re-admission read owns
    // the Inbox slot; it installs nothing unless the same account is verified.
    let readmitting = crate::personal_readmission_v105::in_flight(app);
    let stale_page = [View::Inbox, View::Actions, View::Search, View::Dashboard]
        .into_iter()
        .filter(|view| *view != View::Search || !local)
        .filter(|view| *view != View::Inbox || !readmitting)
        .any(|view| app.loading[view.index()] || app.content[view.index()] != [reason.label()]);
    // A quiescent stopped scope must not continuously dismiss a newly opened
    // read-only explanation or a preserved Local form/search each frame.
    if !pending && !stale_page {
        return false;
    }
    retire_remote(app, background, reason);
    true
}
/// Use only at a LEGACY consumer branch. An owned scoped current-access route
/// must be selected before this refusal, never after building fallback names.
pub fn refuse_legacy_request(
    app: &mut App,
    background: &mut Background,
    consumer: RemoteConsumerV1,
) -> bool {
    if consumer == RemoteConsumerV1::Search
        && independent_local_search(app.search.as_ref())
        && local_search_action(app.search_action.as_ref())
    {
        return false;
    }
    // Personal queues name no repositories, so they cannot narrow or redirect
    // repository-scoped views; those keep their own --repo/origin/set scope.
    if consumer != RemoteConsumerV1::MyPrs
        && app
            .remote_scope
            .as_deref()
            .is_some_and(ActiveRemoteScopeV1::is_default_personal)
    {
        return false;
    }
    let Err(reason) = legacy_entry(app.remote_scope.as_deref()) else {
        return false;
    };
    retire_remote(app, background, reason);
    app.status = format!("{} • {}", consumer.label(), reason.label());
    true
}

#[cfg(test)]
#[path = "remote_scope_gate_v1_tests.rs"]
mod tests;
