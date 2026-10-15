//! Bounded background preparation. The terminal thread only consumes completed state.
use crate::{
    app::{App, View},
    providers::runner::CancellationToken,
};
use std::{
    collections::HashMap,
    sync::{
        mpsc::{self, Receiver, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread,
};

#[derive(Default)]
pub struct PreparedView {
    pub scoped_inline_v1: Option<crate::scoped_pr_observed_tabs_v1::resources::immutable_images::inline_v1::native_v1::PreparedScopedInlineV1>,
    pub original_stash_recovery_v1: Option<crate::repository_stash_recovery_v1::PreparedStashRecoveryV1>,
    pub original_diff_result_v1:
        Option<crate::repository_diff_sides_worker_v1::PreparedOriginalDiffWorkV1>,
    pub related_pr_v1: Option<crate::repository_related_pr_v1::PreparedRelatedPrV1>,
    pub lines: Vec<String>,
    pub context: Option<String>,
    pub status: String,
    pub remote_repository: Option<crate::remote_repository_native_v1::RemoteRepositoryCompletionV1>,
    pub workspace: Option<crate::workspace::Workspace>,
    pub graph_reread_retention_v3:
        Option<crate::repository_graph_reread_native_v3::GraphRereadRetentionV3>,
    pub original_tree_result_v1:
        Option<crate::repository_tree_worker_v1::PreparedOriginalTreeWorkV1>,
    pub repository_source_identity:
        Option<Arc<crate::repository_source_identity::PreparedRepositorySourceIdentity>>,
    pub github: Option<crate::github_workspace::GithubWorkspace>,
    pub scoped_pr_tab: Option<crate::scoped_pr_observed_tabs_v1::ObservedPrTabResultV1>,
    pub scoped_pr_resource:
        Option<crate::scoped_pr_observed_tabs_v1::resources::native::PrResourceCompletionV1>,
    pub form: Option<crate::forms::Form>,
    pub search: Option<crate::search::workspace::SearchWorkspace>,
    pub navigate_to: Option<View>,
    pub restore_search_input: bool,
    pub actions: Option<crate::actions_workspace::ActionsWorkspace>,
    pub pr_checks_origin: Option<Arc<crate::pr_checks_native::PrChecksOrigin>>,
    pub pr_checks_route: Option<Arc<crate::pr_checks_native::PrChecksRoute>>,
    pub scoped_pr_checks: Option<Box<crate::pr_checks_native::scoped::ScopedPrCheckCompletionV1>>,
    pub pr_check_continuation_v2: Option<Box<crate::pr_checks_native::scoped::choices::continuations_v2::PrCheckContinuationCompletionV2>>,
    pub actions_inspection: Option<crate::actions_recovery::SharedActionsInspection>,
    pub clear_actions_inspection: bool,
    pub clear_private: bool,
    pub activate_if_untouched: bool,
    pub pr_inspection: Option<crate::app::SharedPrInspection>,
    pub clear_pr_inspection: bool,
    pub pr_pending_message: Option<String>,
    pub verified_identity: Option<crate::providers::gh::HostIdentity>,
    pub pr_reactions: Option<crate::pr_actions::PrReactionPage>,
    pub pr_reaction_display: Option<Vec<crate::pr_ui::PrReactionDisplayRow>>,
    pub readonly_notice: Option<String>,
    pub source_message: Option<String>,
    pub source_context: Option<crate::search_sources::context::SourceContext>,
    pub metrics: Option<crate::metrics_workspace::MetricsWorkspace>,
    pub clear_metrics: bool,
    pub github_origin: Option<View>,
    pub invalidate_metrics: bool,
    /// A known invocation/effect makes old structured private projections
    /// unavailable independently of whether optional fresh view loading worked.
    pub invalidate_pr_metadata: bool,
    pub navigation_restore: Option<crate::navigation_state::NavigationRestore>,
    pub catalogue: Option<crate::repository_catalogue_native::CataloguePrepared>,
    pub local_actor_result: Option<
        Result<
            Arc<crate::repository_commit_actor_native_v1::PreparedLocalActorReportV1>,
            crate::repository_commit_actor_native_v1::LocalActorVerificationErrorV1,
        >,
    >,
    /// Closed navigation observations, not renewed authentication or permission.
    pub remote_scope: Option<Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>>,
    pub remote_scope_source: Option<Arc<crate::storage::CatalogueIntentSource>>,
}

#[path = "remote_repository_background_v1.rs"]
mod remote_repository_background_v1;

#[cfg(test)]
#[path = "runtime/graph_refresh_v3_tests.rs"]
mod graph_refresh_v3_tests;

#[path = "runtime/local_saved_navigation_v1.rs"]
mod local_saved_navigation_v1;
#[path = "runtime/original_diff_sides_background_v1.rs"]
mod original_diff_sides_background_v1;
#[path = "runtime/original_stash_recovery_background_v1.rs"]
mod original_stash_recovery_background_v1;
#[path = "runtime/original_tree_background_v1.rs"]
mod original_tree_background_v1;
#[cfg(test)]
#[path = "runtime/pr_check_continuation_corrective_v3_test_support.rs"]
pub(crate) mod pr_check_continuation_corrective_v3_test_support;
#[path = "runtime/related_pr_background_v1.rs"]
mod related_pr_background_v1;
#[path = "runtime/scoped_inline_background_v1.rs"]
mod scoped_inline_background_v1;
#[cfg(test)]
#[path = "runtime/scoped_inline_test_support_v1.rs"]
pub(crate) mod scoped_inline_test_support_v1;

type Work = Box<dyn FnOnce(CancellationToken) -> Result<PreparedView, String> + Send>;

struct ScopedRemoteOwner {
    inline_v1: Option<Arc<crate::scoped_pr_observed_tabs_v1::resources::immutable_images::inline_v1::native_v1::ScopedInlineOwnerV1>>,
    scope: Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>,
    source: Arc<crate::storage::CatalogueIntentSource>,
    navigation: Option<Arc<crate::scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>>,
    check_origin: Option<Arc<crate::pr_checks_native::PrChecksOrigin>>,
    continuation_v2: Option<
        Arc<crate::pr_checks_native::scoped::choices::continuations_v2::PrCheckContinuationOwnerV2>,
    >,
}
impl ScopedRemoteOwner {
    fn owns(&self, app: &App) -> bool {
        self.inline_v1.as_ref().is_none_or(|owner| owner.owns(app))
            && app
                .remote_scope
                .as_ref()
                .is_some_and(|scope| Arc::ptr_eq(scope, &self.scope))
            && app
                .remote_scope_source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &self.source))
            && self
                .check_origin
                .as_ref()
                .is_none_or(|origin| crate::pr_checks_native::owns_origin(app, origin))
            && self
                .continuation_v2
                .as_ref()
                .is_none_or(|owner| owner.owns(app))
            && self.navigation.as_ref().is_none_or(|navigation| {
                app.github
                    .as_ref()
                    .and_then(|github| github.scoped_pr_navigation.as_ref())
                    .is_some_and(|current| Arc::ptr_eq(current, navigation))
            })
    }
}
struct Request {
    original_stash_recovery_owner_v1:
        Option<Arc<crate::repository_stash_recovery_v1::OriginalStashRecoveryOwnerV1>>,
    view: View,
    generation: u64,
    cancellation: CancellationToken,
    catalogue_owner: Option<Arc<crate::repository_catalogue_native::CatalogueOwner>>,
    catalogue_recovery: Option<Arc<crate::repository_catalogue_native::CatalogueReReadRecoveryV1>>,
    remote_scope_owner: Option<ScopedRemoteOwner>,
    local_actor_owner:
        Option<Arc<crate::repository_commit_actor_native_v1::LocalActorRequestOwnerV1>>,
    remote_repository_owner:
        Option<Arc<crate::remote_repository_native_v1::RemoteRepositoryRequestOwnerV1>>,
    local_navigation_owner:
        Option<Arc<crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1>>,
    original_tree_owner_v1:
        Option<Arc<crate::repository_tree_worker_v1::OriginalTreeWorkerOwnerV1>>,
    original_diff_owner_v1:
        Option<Arc<crate::repository_diff_sides_worker_v1::OriginalDiffWorkerOwnerV1>>,
    work: Work,
}
struct Completion {
    original_stash_recovery_owner_v1:
        Option<Arc<crate::repository_stash_recovery_v1::OriginalStashRecoveryOwnerV1>>,
    view: View,
    generation: u64,
    catalogue_owner: Option<Arc<crate::repository_catalogue_native::CatalogueOwner>>,
    catalogue_recovery: Option<Arc<crate::repository_catalogue_native::CatalogueReReadRecoveryV1>>,
    remote_scope_owner: Option<ScopedRemoteOwner>,
    local_actor_owner:
        Option<Arc<crate::repository_commit_actor_native_v1::LocalActorRequestOwnerV1>>,
    remote_repository_owner:
        Option<Arc<crate::remote_repository_native_v1::RemoteRepositoryRequestOwnerV1>>,
    local_navigation_owner:
        Option<Arc<crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1>>,
    original_tree_owner_v1:
        Option<Arc<crate::repository_tree_worker_v1::OriginalTreeWorkerOwnerV1>>,
    original_diff_owner_v1:
        Option<Arc<crate::repository_diff_sides_worker_v1::OriginalDiffWorkerOwnerV1>>,
    result: Result<PreparedView, String>,
}

pub struct Background {
    #[cfg(test)]
    inline_completion_sender_v1: SyncSender<Completion>,
    current_inline_owner_v1: Option<Arc<crate::scoped_pr_observed_tabs_v1::resources::immutable_images::inline_v1::native_v1::ScopedInlineOwnerV1>>,
    current_inline_generation_v1: Option<u64>,
    current_stash_recovery_v1: Option<(
        u64,
        Arc<crate::repository_stash_recovery_v1::OriginalStashRecoveryOwnerV1>,
    )>,
    current_related_pr_v1: Option<Arc<crate::repository_related_pr_v1::RelatedPrOwnerV1>>,
    current_related_pr_generation_v1: Option<u64>,
    requests: Option<SyncSender<Request>>,
    completions: Receiver<Completion>,
    current: HashMap<usize, (u64, CancellationToken)>,
    generation: u64,
    workers: Vec<thread::JoinHandle<()>>,
    current_original_local_owner:
        Option<Arc<crate::navigation_local_owner_v1::OriginalLocalNavigationOwnerV1>>,
    current_original_tree_owner_v1:
        Option<Arc<crate::repository_tree_worker_v1::OriginalTreeWorkerOwnerV1>>,
    current_original_tree_generation_v2: Option<u64>,
    current_original_diff_owner_v1:
        Option<Arc<crate::repository_diff_sides_worker_v1::OriginalDiffWorkerOwnerV1>>,
    current_original_diff_generation_v1: Option<u64>,
}

impl Background {
    pub fn new() -> Self {
        let (requests, receiver) = mpsc::sync_channel::<Request>(4);
        let (sender, completions) = mpsc::sync_channel(16);
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let receiver = receiver.clone();
            let sender = sender.clone();
            workers.push(thread::spawn(move || loop {
                let request = receiver.lock().unwrap_or_else(|p| p.into_inner()).recv();
                let Ok(request) = request else {
                    break;
                };
                if request.cancellation.is_cancelled() {
                    continue;
                }
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    (request.work)(request.cancellation.clone())
                }))
                .unwrap_or_else(|_| {
                    Err("Background task failed unexpectedly; refresh to retry".into())
                }).map(|mut prepared| {
                    if let Some(workspace) = &prepared.workspace {
                        // Bounded source text is prepared in the worker, not
                        // draw/poll. A failed producer cannot retain old labels.
                        prepared.repository_source_identity = crate::repository_source_identity::PreparedRepositorySourceIdentity::prepare(workspace).ok().map(Arc::new);
                    }
                    prepared
                });
                if sender
                    .send(Completion {
                        original_stash_recovery_owner_v1: request.original_stash_recovery_owner_v1,
                        view: request.view,
                        generation: request.generation,
                        catalogue_owner: request.catalogue_owner,
                        catalogue_recovery: request.catalogue_recovery,
                        remote_scope_owner: request.remote_scope_owner,
                        local_actor_owner: request.local_actor_owner,
                        local_navigation_owner: request.local_navigation_owner,
                        original_tree_owner_v1: request.original_tree_owner_v1,
                        original_diff_owner_v1: request.original_diff_owner_v1,
                        remote_repository_owner: request.remote_repository_owner,
                        result,
                    })
                    .is_err()
                {
                    break;
                }
            }));
        }
        Self {
            #[cfg(test)]
            inline_completion_sender_v1: sender,
            current_inline_owner_v1: None,
            current_inline_generation_v1: None,
            current_stash_recovery_v1: None,
            current_related_pr_v1: None,
            current_related_pr_generation_v1: None,
            requests: Some(requests),
            completions,
            current: HashMap::new(),
            generation: 0,
            workers,
            current_original_local_owner: None,
            current_original_tree_owner_v1: None,
            current_original_tree_generation_v2: None,
            current_original_diff_owner_v1: None,
            current_original_diff_generation_v1: None,
        }
    }

    pub fn submit(
        &mut self,
        app: &mut App,
        view: View,
        work: impl FnOnce(CancellationToken) -> Result<PreparedView, String> + Send + 'static,
    ) -> bool {
        self.submit_with_owner(app, view, None, work)
    }

    /// Bind successes, errors and caught panics to BOTH original navigation
    /// owners. Equal display text or a saved name never admits a late result.
    pub fn submit_scoped_remote(
        &mut self,
        app: &mut App,
        view: View,
        scope: Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>,
        source: Arc<crate::storage::CatalogueIntentSource>,
        work: impl FnOnce(CancellationToken) -> Result<PreparedView, String> + Send + 'static,
    ) -> bool {
        if crate::remote_scope_gate_v1::stop_before_fallback(Some(scope.as_ref())).is_some() {
            return false;
        }
        let owner = ScopedRemoteOwner {
            scope,
            source,
            navigation: None,
            check_origin: None,
            continuation_v2: None,
            inline_v1: None,
        };
        if !owner.owns(app) {
            return false;
        }
        self.submit_with_owner(app, view, Some(owner), work)
    }

    /// Page-only observed preparation binds the exact original tuple as well
    /// as both source owners. No new access/auth is minted.
    pub fn submit_scoped_pr_tab(
        &mut self,
        app: &mut App,
        scope: Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>,
        source: Arc<crate::storage::CatalogueIntentSource>,
        navigation: Arc<crate::scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>,
        work: impl FnOnce(CancellationToken) -> Result<PreparedView, String> + Send + 'static,
    ) -> bool {
        if crate::remote_scope_gate_v1::stop_before_fallback(Some(scope.as_ref())).is_some() {
            return false;
        }
        let owner = ScopedRemoteOwner {
            scope,
            source,
            navigation: Some(navigation),
            check_origin: None,
            continuation_v2: None,
            inline_v1: None,
        };
        if !owner.owns(app) {
            return false;
        }
        self.submit_with_owner(app, View::Inbox, Some(owner), work)
    }

    /// Scoped PR-check reads share the SAME publisher, but later Actions/log
    /// work owns the Actions slot rather than cancelling/occupying Inbox.
    pub fn submit_scoped_pr_check_v1(
        &mut self,
        app: &mut App,
        view: View,
        origin: Arc<crate::pr_checks_native::PrChecksOrigin>,
        work: impl FnOnce(CancellationToken) -> Result<PreparedView, String> + Send + 'static,
    ) -> bool {
        let Some((source, scope, navigation)) = origin.scoped_owner_v1() else {
            return false;
        };
        if !matches!(view, View::Inbox | View::Actions)
            || crate::remote_scope_gate_v1::stop_before_fallback(Some(scope.as_ref())).is_some()
        {
            return false;
        }
        let owner = ScopedRemoteOwner {
            scope: scope.clone(),
            source: source.clone(),
            navigation: Some(navigation.clone()),
            check_origin: Some(origin),
            continuation_v2: None,
            inline_v1: None,
        };
        if !owner.owns(app) {
            return false;
        }
        self.submit_with_owner(app, view, Some(owner), work)
    }

    /// Same queue/generation/token/publisher with actual current route/capture/
    /// watch facets. Ownership is checked BEFORE success, Err and panic effects.
    pub fn submit_pr_check_continuation_v2(
        &mut self,
        app: &mut App,
        continuation: Arc<
            crate::pr_checks_native::scoped::choices::continuations_v2::PrCheckContinuationOwnerV2,
        >,
        work: impl FnOnce(CancellationToken) -> Result<PreparedView, String> + Send + 'static,
    ) -> bool {
        if !continuation.owns(app) {
            return false;
        }
        let origin = continuation.origin().clone();
        let Some((source, scope, navigation)) = origin.scoped_owner_v1() else {
            return false;
        };
        if crate::remote_scope_gate_v1::stop_before_fallback(Some(scope.as_ref())).is_some() {
            return false;
        }
        let owner = ScopedRemoteOwner {
            scope: scope.clone(),
            source: source.clone(),
            navigation: Some(navigation.clone()),
            check_origin: Some(origin),
            continuation_v2: Some(continuation),
            inline_v1: None,
        };
        if !owner.owns(app) {
            return false;
        }
        self.submit_with_owner(app, View::Actions, Some(owner), work)
    }
    fn submit_with_owner(
        &mut self,
        app: &mut App,
        view: View,
        remote_scope_owner: Option<ScopedRemoteOwner>,
        work: impl FnOnce(CancellationToken) -> Result<PreparedView, String> + Send + 'static,
    ) -> bool {
        app.retire_command_context();
        self.cancel(view);
        self.generation += 1;
        let generation = self.generation;
        let cancellation = CancellationToken::default();
        let request = Request {
            original_stash_recovery_owner_v1: None,
            view,
            generation,
            cancellation: cancellation.clone(),
            catalogue_owner: None,
            catalogue_recovery: None,
            remote_scope_owner,
            local_actor_owner: None,
            remote_repository_owner: None,
            local_navigation_owner: None,
            original_tree_owner_v1: None,
            original_diff_owner_v1: None,
            work: Box::new(work),
        };
        match self
            .requests
            .as_ref()
            .expect("active worker queue")
            .try_send(request)
        {
            Ok(()) => {
                self.current
                    .insert(view.index(), (generation, cancellation));
                app.loading[view.index()] = true;
                if app.view == view {
                    app.status = format!("Loading {}… • Esc Cancel", view.label());
                }
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                app.loading[view.index()] = false;
                app.status = "Background queue is full • retry with Refresh".into();
                false
            }
        }
    }

    /// Create ONE token before the original profile/row owner. Same token is in
    /// Request/current/Work/raw Git/gh/closing original source callback.
    pub fn submit_local_actor(
        &mut self,
        app: &mut App,
        work: impl FnOnce(
                Arc<crate::repository_commit_actor_native_v1::LocalActorRequestOwnerV1>,
            ) -> Result<PreparedView, String>
            + Send
            + 'static,
    ) -> bool {
        let view = View::Repository;
        self.cancel(view);
        let cancellation = CancellationToken::default();
        let owner = match app.local_commit_actors.begin(cancellation.clone()) {
            Ok(owner) => owner,
            Err(reason) => {
                app.status = reason;
                return false;
            }
        };
        let Some(generation) = self.generation.checked_add(1) else {
            app.local_commit_actors.clear_host_report();
            app.status = "Background epoch exhausted; restart before verification".into();
            return false;
        };
        self.generation = generation;
        let work_owner = owner.clone();
        let request = Request {
            original_stash_recovery_owner_v1: None,
            view,
            generation,
            cancellation: cancellation.clone(),
            catalogue_owner: None,
            catalogue_recovery: None,
            remote_scope_owner: None,
            local_actor_owner: Some(owner.clone()),
            local_navigation_owner: None,
            original_tree_owner_v1: None,
            original_diff_owner_v1: None,
            remote_repository_owner: None,
            work: Box::new(move |_actual_same_token| work(work_owner)),
        };
        match self
            .requests
            .as_ref()
            .expect("active worker queue")
            .try_send(request)
        {
            Ok(()) => {
                self.current
                    .insert(view.index(), (generation, cancellation));
                app.loading[view.index()] = true;
                app.status = "Verifying original saved commit • Esc Cancel".into();
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                app.local_commit_actors.clear_host_report();
                app.loading[view.index()] = false;
                app.status = "Background queue full • nothing sent; verify explicitly".into();
                false
            }
        }
    }
    pub fn cancel(&mut self, view: View) {
        if view == View::Inbox {
            self.current_inline_owner_v1 = None;
            self.current_inline_generation_v1 = None;
        }
        if view == View::Repository {
            self.current_stash_recovery_v1 = None;
            self.current_related_pr_v1 = None;
            self.current_related_pr_generation_v1 = None;
            self.current_original_local_owner = None;
            self.current_original_tree_owner_v1 = None;
            self.current_original_tree_generation_v2 = None;
            self.current_original_diff_owner_v1 = None;
            self.current_original_diff_generation_v1 = None;
        }
        if let Some((_, cancellation)) = self.current.remove(&view.index()) {
            cancellation.cancel();
        }
    }

    /// Inventory preparation is independent of the five product-view jobs. Its
    /// immutable live owner binds failures as well as successes before privacy
    /// effects, so a superseded query cannot clear a newer source.
    pub fn submit_catalogue(
        &mut self,
        app: &mut App,
        owner: Arc<crate::repository_catalogue_native::CatalogueOwner>,
        work: impl FnOnce(CancellationToken) -> Result<PreparedView, String> + Send + 'static,
    ) -> bool {
        self.submit_catalogue_owned(app, owner, None, work)
    }
    /// Both owner and exact hidden slot bind success, failure and caught panic.
    pub fn submit_catalogue_recovery(
        &mut self,
        app: &mut App,
        owner: Arc<crate::repository_catalogue_native::CatalogueOwner>,
        recovery: Arc<crate::repository_catalogue_native::CatalogueReReadRecoveryV1>,
        work: impl FnOnce(CancellationToken) -> Result<PreparedView, String> + Send + 'static,
    ) -> bool {
        self.submit_catalogue_owned(app, owner, Some(recovery), work)
    }
    fn submit_catalogue_owned(
        &mut self,
        app: &mut App,
        owner: Arc<crate::repository_catalogue_native::CatalogueOwner>,
        recovery: Option<Arc<crate::repository_catalogue_native::CatalogueReReadRecoveryV1>>,
        work: impl FnOnce(CancellationToken) -> Result<PreparedView, String> + Send + 'static,
    ) -> bool {
        if !crate::repository_catalogue_native::owns(app, &owner)
            || recovery.as_ref().is_some_and(|slot| {
                !crate::repository_catalogue_native::owns_recovery(app, &owner, slot)
            })
        {
            return false;
        }
        self.cancel_catalogue();
        self.generation += 1;
        let generation = self.generation;
        let cancellation = CancellationToken::default();
        let request = Request {
            original_stash_recovery_owner_v1: None,
            view: app.view,
            generation,
            cancellation: cancellation.clone(),
            catalogue_owner: Some(owner),
            catalogue_recovery: recovery,
            remote_scope_owner: None,
            local_actor_owner: None,
            remote_repository_owner: None,
            local_navigation_owner: None,
            original_tree_owner_v1: None,
            original_diff_owner_v1: None,
            work: Box::new(work),
        };
        match self
            .requests
            .as_ref()
            .expect("active worker queue")
            .try_send(request)
        {
            Ok(()) => {
                self.current.insert(5, (generation, cancellation));
                true
            }
            Err(TrySendError::Full(_) | TrySendError::Disconnected(_)) => {
                if let Some(modal) = &mut app.catalogue {
                    modal.busy = false;
                    modal.status =
                        "Discovery worker queue full • no command sent; explicitly Retry or Close"
                            .into();
                }
                false
            }
        }
    }
    pub fn cancel_catalogue(&mut self) {
        if let Some((_, cancellation)) = self.current.remove(&5) {
            cancellation.cancel();
        }
    }
    pub fn catalogue_pending(&self) -> bool {
        self.current.contains_key(&5)
    }

    /// Mutation cancellation must still deliver the actual effect/draft/recovery
    /// result. Removing its generation would discard that safety information.
    pub fn request_cancel(&mut self, view: View) -> bool {
        if let Some((_, cancellation)) = self.current.get(&view.index()) {
            cancellation.cancel();
            true
        } else {
            false
        }
    }

    fn clear_remote_search(
        &mut self,
        app: &mut App,
        verified: Option<&crate::providers::gh::HostIdentity>,
    ) {
        if app.metrics.as_ref().is_some_and(|metrics| {
            verified.is_none_or(|identity| {
                metrics.snapshot.scope.host() != identity.host
                    || identity.account_id != Some(metrics.snapshot.scope.account_id())
            })
        }) {
            self.cancel(View::Dashboard);
            app.loading[View::Dashboard.index()] = false;
            app.metrics = None;
            app.metrics_action = None;
            app.metrics_load_requested = false;
            app.metrics_content = Default::default();
            app.content[View::Dashboard.index()] =
                vec!["Dashboard account changed or unavailable • previous values hidden".into()];
            app.hits.clear();
            app.avatar_states.clear();
            if app.view == View::Dashboard {
                app.context =
                    "Dashboard • previous values hidden • original storage preserved".into();
            }
        }
        let hidden = app.search.as_ref().is_some_and(|search| {
            (search.mode == crate::search::SearchMode::GitHub
                || search.remote_identity.is_some()
                || search.scope.host != "local")
                && verified.is_none_or(|identity| {
                    if search.scope.host != "local" {
                        crate::storage::Scope::github(&identity.host, identity.account_id)
                            .map_or(true, |scope| scope != search.scope)
                            || search
                                .remote_identity
                                .as_ref()
                                .is_some_and(|previous| !previous.same_account(identity))
                    } else {
                        search
                            .remote_identity
                            .as_ref()
                            .is_none_or(|previous| !previous.same_account(identity))
                    }
                })
        });
        if !hidden {
            return;
        }
        self.cancel(View::Search);
        clear_context_notice(app);
        app.loading[View::Search.index()] = false;
        app.search = None;
        app.search_action = None;
        app.search_load_requested = false;
        app.search_controls = Default::default();
        app.hits.clear();
        app.avatar_states.clear();
        if app
            .form
            .as_ref()
            .is_some_and(|form| form.view() != View::Repository)
        {
            app.form = None;
            if app.overlay == Some(crate::app::Overlay::Form) {
                app.overlay = None;
            }
            app.form_dismissed = true;
        }
        app.content[View::Search.index()] = vec![
            "GitHub identity changed or unavailable • private search hidden; storage preserved"
                .into(),
            "Choose an explicit repository scope and search again after identity verification."
                .into(),
        ];
        if app.view == View::Search {
            app.context = "GitHub identity unavailable • private search hidden".into();
            app.status = "Private search cleared • verify identity and search again".into();
            app.scroll = 0;
            app.selected = 0;
        }
    }

    fn accept_remote_principal(
        &mut self,
        app: &mut App,
        identity: &crate::providers::gh::HostIdentity,
    ) {
        {
            let actor_was_pending = app.local_commit_actors.is_pending();
            if app.local_commit_actors.observe_principal(identity) {
                app.local_actor_cancel_requested |= actor_was_pending;
                if actor_was_pending {
                    app.loading[View::Repository.index()] = false;
                }
                app.avatar_states.clear();
                app.local_actor_layout.clear();
            }
        }
        if crate::repository_catalogue_native::principal_changed(app, identity) {
            crate::repository_catalogue_native::privacy(app);
            self.cancel_catalogue();
        }
        let scope = identity.account_id.and_then(|id| {
            crate::navigation_trail::NavigationScope::github(&identity.host, id).ok()
        });
        let _ = app.navigation_trail.remote_identity(scope);
        let old_github = app
            .github
            .as_ref()
            .is_some_and(|github| !github.identity.same_account(identity));
        let old_actions = app
            .actions
            .as_ref()
            .is_some_and(|actions| !actions.identity.same_account(identity));
        if old_github {
            app.pr_reactions = None;
            app.pr_reaction_display.clear();
            app.pr_inspection = None;
            app.pr_pending_message.clear();
            app.pr_action = None;
            self.cancel(View::Inbox);
            app.loading[View::Inbox.index()] = false;
            app.github = None;
            app.github_action = None;
            app.content[View::Inbox.index()] = vec![
                "Account changed • previous private GitHub pages hidden; refresh to verify this account".into(),
            ];
        }
        if old_actions {
            app.actions_inspection = None;
            self.cancel(View::Actions);
            app.loading[View::Actions.index()] = false;
            app.actions = None;
            app.actions_action = None;
            app.content[View::Actions.index()] = vec![
                "Account changed • previous private Actions pages hidden; refresh to verify this account".into(),
            ];
        }
        if old_github || old_actions {
            crate::pr_checks_native::retire(app, true);
            clear_context_notice(app);
            app.avatar_states.clear();
            app.hits.clear();
            if app
                .form
                .as_ref()
                .is_some_and(|form| form.view() != View::Repository)
            {
                app.form = None;
                if app.overlay == Some(crate::app::Overlay::Form) {
                    app.overlay = None;
                }
                app.form_dismissed = true;
            }
        }
    }

    pub fn poll(&mut self, app: &mut App) -> bool {
        let mut changed = false;
        while let Ok(mut completion) = self.completions.try_recv() {
            let slot = if completion.remote_repository_owner.is_some() {
                6
            } else if completion.catalogue_owner.is_some() {
                5
            } else {
                completion.view.index()
            };
            if self.current.get(&slot).map(|(generation, _)| *generation)
                != Some(completion.generation)
            {
                continue;
            }
            if completion.original_stash_recovery_owner_v1.is_none()
                && (completion.view == View::Repository
                    || completion
                        .remote_scope_owner
                        .as_ref()
                        .is_some_and(|owner| owner.continuation_v2.is_some()))
                && (app.original_stash_recovery_request_v1.is_some()
                    || app.original_stash_recovery_cancel_requested_v1)
            {
                continue; // New original stash intent precedes every old success/error/panic effect.
            }
            if slot == View::Inbox.index()
                && self.current_inline_owner_v1.is_some()
                && completion
                    .remote_scope_owner
                    .as_ref()
                    .is_none_or(|remote| remote.inline_v1.is_none())
            {
                continue;
            }
            if let Some(owner) = completion
                .remote_scope_owner
                .as_ref()
                .and_then(|remote| remote.inline_v1.as_ref())
                .cloned()
            {
                let remote = completion.remote_scope_owner.as_ref().expect("same owner");
                if slot != View::Inbox.index()
                    || completion.view != View::Inbox
                    || completion.catalogue_owner.is_some()
                    || completion.catalogue_recovery.is_some()
                    || completion.local_actor_owner.is_some()
                    || completion.remote_repository_owner.is_some()
                    || completion.local_navigation_owner.is_some()
                    || completion.original_tree_owner_v1.is_some()
                    || completion.original_diff_owner_v1.is_some()
                    || remote.check_origin.is_some()
                    || completion.original_stash_recovery_owner_v1.is_some()
                    || remote.continuation_v2.is_some()
                    || self.current_inline_generation_v1 != Some(completion.generation)
                    || self
                        .current_inline_owner_v1
                        .as_ref()
                        .is_none_or(|current| !Arc::ptr_eq(current, &owner))
                    || !remote.owns(app)
                    || completion.result.as_ref().is_ok_and(|prepared| {
                        !scoped_inline_background_v1::only_inline(prepared)
                            || prepared
                                .scoped_inline_v1
                                .as_ref()
                                .is_none_or(|result| !result.matches(&owner))
                    })
                {
                    continue;
                }
                // ALL original/mixed/newer-intent gates precede slot removal
                // and success, error, panic, status, principal/privacy effects.
                self.current.remove(&slot);
                self.current_inline_owner_v1 = None;
                self.current_inline_generation_v1 = None;
                if owner.expired() {
                    crate::scoped_pr_observed_tabs_v1::resources::immutable_images::inline_v1::native_v1::fail(app,self);
                } else {
                    match completion.result {Ok(mut prepared)=>crate::scoped_pr_observed_tabs_v1::resources::immutable_images::inline_v1::native_v1::publish(app,self,&owner,prepared.scoped_inline_v1.take().expect("checked closed facet")),Err(_)=>crate::scoped_pr_observed_tabs_v1::resources::immutable_images::inline_v1::native_v1::fail(app,self)}
                }
                changed = true;
                continue;
            }
            if completion
                .result
                .as_ref()
                .is_ok_and(|prepared| prepared.scoped_inline_v1.is_some())
            {
                continue;
            }
            if let Some(owner) = &completion.original_tree_owner_v1 {
                if completion.original_stash_recovery_owner_v1.is_some() {
                    continue;
                }
                // The physical slot alone is not the retained logical owner.
                // Reject a foreign/mixed completion before retiring any slot
                // or allowing a specialized publisher's visible effects.
                if completion.view != View::Repository
                    || self.current_original_tree_generation_v2 != Some(completion.generation)
                    || self
                        .current_original_tree_owner_v1
                        .as_ref()
                        .is_none_or(|current| !Arc::ptr_eq(current, owner))
                    || completion.catalogue_owner.is_some()
                    || completion.catalogue_recovery.is_some()
                    || completion.remote_scope_owner.is_some()
                    || completion.local_actor_owner.is_some()
                    || completion.remote_repository_owner.is_some()
                    || completion.local_navigation_owner.is_some()
                    || completion.original_diff_owner_v1.is_some()
                {
                    continue;
                }
            }
            // The physical slot is shared with other repository workflows.
            // A stale logical Diff owner or mixed result cannot consume it.
            if let Some(owner) = &completion.original_diff_owner_v1 {
                if completion.original_stash_recovery_owner_v1.is_some() {
                    continue;
                }
                if completion.view != View::Repository
                    || completion.catalogue_owner.is_some()
                    || completion.catalogue_recovery.is_some()
                    || completion.remote_scope_owner.is_some()
                    || completion.local_actor_owner.is_some()
                    || completion.remote_repository_owner.is_some()
                    || completion.local_navigation_owner.is_some()
                    || completion.original_tree_owner_v1.is_some()
                    || self.current_related_pr_v1.is_some()
                    || self.current_original_diff_generation_v1 != Some(completion.generation)
                    || self
                        .current_original_diff_owner_v1
                        .as_ref()
                        .is_none_or(|current| !Arc::ptr_eq(current, owner))
                    || completion.result.as_ref().is_ok_and(|prepared| {
                        !original_diff_sides_background_v1::original_diff_prepared_only_v1(prepared)
                            || prepared
                                .original_diff_result_v1
                                .as_ref()
                                .is_none_or(|result| !result.matches(owner))
                    })
                {
                    continue;
                }
            } else if completion
                .result
                .as_ref()
                .is_ok_and(|prepared| prepared.original_diff_result_v1.is_some())
            {
                continue;
            }
            // Gate original owner before every success/error/panic effect.
            if let Some(admitted) = self.publish_stash_recovery_v1(app, &mut completion) {
                changed |= admitted;
                continue;
            }
            if completion
                .result
                .as_ref()
                .is_ok_and(|p| p.related_pr_v1.is_some())
                && (slot != View::Repository.index() || self.current_related_pr_v1.is_none())
            {
                continue;
            }
            if slot == View::Repository.index() {
                if let Some(owner) = self.current_related_pr_v1.clone() {
                    if self.current_related_pr_generation_v1 != Some(completion.generation)
                        || completion.local_navigation_owner.is_some()
                        || completion.local_actor_owner.is_some()
                        || completion.original_tree_owner_v1.is_some()
                        || completion.original_diff_owner_v1.is_some()
                        || completion.remote_scope_owner.is_some()
                        || !owner.owns(app)
                        || completion.result.as_ref().is_ok_and(|prepared| {
                            !related_pr_background_v1::only_related(prepared)
                                || prepared
                                    .related_pr_v1
                                    .as_ref()
                                    .is_none_or(|p| !p.matches_owner(&owner))
                        })
                    {
                        continue;
                    }
                    // Rejected completed work keeps its logical exact owner
                    // and slot until Close, never consuming a newer intent.
                    self.current.remove(&slot);
                    self.current_related_pr_v1 = None;
                    self.current_related_pr_generation_v1 = None;
                    if owner.expired() {
                        self.fail_current_related_pr_v1(app, &owner);
                        changed = true;
                        continue;
                    }
                    match completion.result {
                        Ok(mut prepared)
                            if related_pr_background_v1::only_related(&prepared)
                                && prepared
                                    .related_pr_v1
                                    .as_ref()
                                    .is_some_and(|p| p.matches_owner(&owner)) =>
                        {
                            if !crate::repository_related_pr_v1::accept(
                                app,
                                &owner,
                                prepared.related_pr_v1.take().expect("checked facet"),
                            ) {
                                if owner.owns(app) && owner.expired() {
                                    self.fail_current_related_pr_v1(app, &owner);
                                } else {
                                    continue;
                                }
                            }
                        }
                        Err(_) => {
                            self.fail_current_related_pr_v1(app, &owner);
                        }
                        _ => continue,
                    }
                    app.loading[View::Repository.index()] = false;
                    changed = true;
                    continue;
                }
            }
            self.current.remove(&slot);
            // Before EVERY specialized publisher/privacy/principal/loading
            // effect: this facet belongs ONLY to the original-local lane.
            if completion
                .result
                .as_ref()
                .is_ok_and(|prepared| prepared.graph_reread_retention_v3.is_some())
                && completion.local_navigation_owner.is_none()
            {
                continue;
            }
            if completion
                .catalogue_owner
                .as_ref()
                .is_some_and(|owner| !crate::repository_catalogue_native::owns(app, owner))
            {
                continue;
            }
            if completion.catalogue_recovery.as_ref().is_some_and(|slot| {
                completion.catalogue_owner.as_ref().is_none_or(|owner| {
                    !crate::repository_catalogue_native::owns_recovery(app, owner, slot)
                })
            }) {
                continue;
            }
            if completion
                .remote_scope_owner
                .as_ref()
                .is_some_and(|owner| !owner.owns(app))
            {
                // Reject before loading/status, principal, privacy, hits or
                // models change. This also covers Err and caught worker panic.
                continue;
            }
            // Continuations have ONE closed prepared-kind publisher. Its
            // original generation and Scope/Source/Nav/capture/watch ticket
            // were checked above; no generic effect may precede its result gate.
            if let Some(owner) = completion
                .remote_scope_owner
                .as_ref()
                .and_then(|owner| owner.continuation_v2.as_ref())
            {
                if completion.view != View::Actions
                    || completion.catalogue_owner.is_some()
                    || completion.catalogue_recovery.is_some()
                    || completion.local_actor_owner.is_some()
                    || completion.remote_repository_owner.is_some()
                    || completion.local_navigation_owner.is_some()
                    || completion.original_tree_owner_v1.is_some()
                    || completion.original_diff_owner_v1.is_some()
                {
                    continue;
                }
                changed |= crate::pr_checks_native::scoped::choices::continuations_v2::corrective_v3::publish(
                    app, owner, completion.result,
                );
                continue;
            }
            if completion
                .result
                .as_ref()
                .is_ok_and(|prepared| prepared.pr_check_continuation_v2.is_some())
            {
                continue; // A generic/local owner cannot consume this facet.
            }
            if completion
                .remote_repository_owner
                .as_ref()
                .is_some_and(|owner| !owner.owns(app))
            {
                continue;
            }
            if completion
                .remote_repository_owner
                .as_ref()
                .is_some_and(|owner| owner.expired())
            {
                crate::remote_repository_native_v1::worker_failed(app);
                changed = true;
                continue;
            }
            if let Some(owner) = &completion.original_diff_owner_v1 {
                let admitted = original_diff_sides_background_v1::publish_original_diff_sides_v1(
                    app,
                    owner,
                    completion.result,
                );
                if admitted {
                    self.current_original_diff_owner_v1 = None;
                    self.current_original_diff_generation_v1 = None;
                    changed = true;
                }
                continue;
            }
            if let Some(owner) = &completion.original_tree_owner_v1 {
                let admitted = original_tree_background_v1::publish_original_tree_v1(
                    app,
                    owner,
                    completion.result,
                );
                if admitted {
                    self.current_original_tree_owner_v1 = None;
                    self.current_original_tree_generation_v2 = None;
                }
                changed |= admitted;
                continue;
            }
            if completion
                .result
                .as_ref()
                .is_ok_and(|prepared| prepared.original_tree_result_v1.is_some())
            {
                continue;
            }
            if let Some(owner) = &completion.local_navigation_owner {
                // Main consumes an operation's own intent before queueing it.
                // A remaining flag/action therefore belongs to newer input;
                // the old result must not reset its desired mode or status.
                if app.repository_load_requested
                    || app.scoped_inline_work_v1.is_some()
                    || app.scoped_inline_cancel_v1
                    || app.original_stash_recovery_request_v1.is_some()
                    || app.original_stash_recovery_cancel_requested_v1
                    || app.repository_action.is_some()
                    || app.graph_reread_request_v3.is_some()
                    || app.related_pr_requested_v1
                    || app.related_pr_cancel_v1
                    || app.original_diff_work_v1.is_some()
                    || app.original_diff_cancel_v1
                {
                    continue;
                }
                if app
                    .workspace
                    .as_ref()
                    .is_none_or(|workspace| !owner.can_admit(workspace, completion.result.is_ok()))
                {
                    continue;
                }
                if let Ok(prepared) = &mut completion.result {
                    if !local_saved_navigation_v1::original_local_prepared_only_v1(prepared)
                        || prepared
                            .workspace
                            .as_ref()
                            .is_none_or(|workspace| !owner.admits_workspace(workspace))
                    {
                        continue;
                    }
                    if prepared
                        .graph_reread_retention_v3
                        .as_ref()
                        .is_some_and(|retention| !retention.matches_operation_v3(owner.operation()))
                    {
                        continue;
                    }
                    if let Some(retention) = prepared.graph_reread_retention_v3.take() {
                        if let Err(reason) =
                            retention.accept(app, prepared.workspace.as_ref().unwrap())
                        {
                            self.current_original_local_owner = None;
                            app.loading[View::Repository.index()] = false;
                            app.status = reason;
                            changed = true;
                            continue; // Old view/model/position remains unchanged.
                        }
                    }
                }
            } // Before success/error/panic loading/status/private/principal/hits.
            if completion
                .local_navigation_owner
                .as_ref()
                .is_some_and(|owner| {
                    self.current_original_local_owner
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, owner))
                })
            {
                self.current_original_local_owner = None;
            }
            if let Some(owner) = &completion.local_actor_owner {
                if app.remote_repository.is_some()
                    || app.view != View::Repository
                    || !app.local_commit_actors.owns(owner)
                {
                    continue;
                }
                if let Ok(prepared) = &completion.result {
                    match prepared.local_actor_result.as_ref() {
                        Some(Ok(report)) if !report.matches_request(owner) => continue,
                        None => continue, // Specialized workers never use the generic publisher.
                        _ => {}
                    }
                }
            } // BEFORE success, Err, panic, loading/status/private/identity/hits.
            app.retire_command_context();
            if slot != 5 && slot != 6 {
                app.loading[completion.view.index()] = false;
            }
            match completion.result {
                Ok(mut prepared) => {
                    if completion.remote_repository_owner.is_some() {
                        if let Some(result) = prepared.remote_repository.take() {
                            crate::remote_repository_native_v1::accept(app, result);
                        } else {
                            crate::remote_repository_native_v1::worker_failed(app);
                        }
                        changed = true;
                        continue;
                    }
                    if let Some(result) = prepared.local_actor_result.take() {
                        let Some(owner) = completion.local_actor_owner.as_ref() else {
                            continue;
                        };
                        match result {
                            Ok(report) => {
                                if !app.local_commit_actors.accept(owner, report) {
                                    continue;
                                }
                            }
                            Err(error) => {
                                if !app.local_commit_actors.fail(owner) {
                                    continue;
                                }
                                if matches!(&error,
                                    crate::repository_commit_actor_native_v1::LocalActorVerificationErrorV1::Host(
                                        crate::providers::gh::GithubError { kind:
                                            crate::providers::gh::GithubErrorKind::IdentityChanged
                                            | crate::providers::gh::GithubErrorKind::Authentication
                                            | crate::providers::gh::GithubErrorKind::MissingGh
                                            | crate::providers::gh::GithubErrorKind::Permission
                                            | crate::providers::gh::GithubErrorKind::NotFound, .. }))
                                {
                                    // IdentityChanged also covers local/resource drift;
                                    // no false assertion that the account always changed.
                                    crate::repository_catalogue_native::privacy(app);
                                }
                            }
                        }
                        app.local_actor_layout.clear();
                        app.avatar_states.clear();
                        app.hits.clear();
                        app.status = app
                            .local_commit_actors
                            .status
                            .unwrap_or("Host report unavailable; local Git remains")
                            .into();
                        changed = true;
                        continue;
                    }
                    if let Some(check) = prepared.scoped_pr_checks.take() {
                        if crate::pr_checks_native::scoped::install_v1(app, *check).is_err() {
                            app.status = "Original scoped check read changed or superseded • no private effect".into();
                        }
                        changed = true;
                        continue;
                    }
                    if let Some(resource) = prepared.scoped_pr_resource.take() {
                        match crate::scoped_pr_observed_tabs_v1::resources::native::install_completion(app,resource) {
                            Ok(effects) => crate::scoped_pr_observed_tabs_v1::resources::native::apply_effects(app,effects),
                            Err(_) => {
                                app.status = "Original resource page changed or superseded • no retarget or private effect".into();
                                changed = true;
                                continue;
                            }
                        }
                    }
                    if let Some(page) = prepared.scoped_pr_tab.take() {
                        let preserve_scoped_view = page.display_limited;
                        let same_pair =
                            app.remote_scope
                                .as_ref()
                                .is_some_and(|scope| Arc::ptr_eq(scope, &page.original_active))
                                && app.remote_scope_source.as_ref().is_some_and(|source| {
                                    Arc::ptr_eq(source, &page.original_source)
                                });
                        if !same_pair
                            || app.github.as_mut().is_none_or(|github| {
                                crate::scoped_pr_observed_tabs_v1::install_if_current(github, page)
                                    .is_err()
                            })
                        {
                            app.status = "Original observed page changed or was superseded • no page retargeted".into();
                            changed = true;
                            continue;
                        }
                        if app.view == View::Inbox && !preserve_scoped_view {
                            app.selected = 0;
                            app.scroll = 0;
                            app.github_content_focused = true;
                        }
                        app.hits.clear();
                    }
                    if prepared
                        .pr_checks_origin
                        .as_ref()
                        .is_some_and(|origin| !crate::pr_checks_native::owns_origin(app, origin))
                    {
                        app.status="Original PR check source changed or was superseded • no private route retargeted".into();
                        changed = true;
                        continue;
                    }
                    if let Some(restore) = &prepared.navigation_restore {
                        if !crate::navigation_state::validate_repository_completion(
                            app,
                            restore,
                            prepared.workspace.as_ref(),
                        ) {
                            app.abandon_navigation_restore();
                            app.status = "Original navigation source changed or was superseded • nothing retargeted".into();
                            changed = true;
                            continue;
                        }
                    }
                    if let Some(identity) = &prepared.verified_identity {
                        // The newly prepared form is verified separately; preserve
                        // an actual user dismissal while clearing old private views.
                        let dismissed = app.form_dismissed;
                        self.clear_remote_search(app, Some(identity));
                        self.accept_remote_principal(app, identity);
                        app.form_dismissed = dismissed;
                    }
                    if prepared.clear_private {
                        app.pr_file_images = None;
                        if app.overlay == Some(crate::app::Overlay::PrFileImages) {
                            app.overlay = None;
                        }
                        crate::repository_catalogue_native::privacy(app);
                        self.cancel_catalogue();
                        crate::pr_checks_native::retire(app, true);
                        let _ = app.navigation_trail.remote_identity(None);
                        app.github_origin = None;
                        app.actions_inspection = None;
                        app.pr_reactions = None;
                        app.pr_reaction_display.clear();
                        app.pr_inspection = None;
                        app.pr_pending_message.clear();
                        app.pr_action = None;
                        clear_context_notice(app);
                        self.clear_remote_search(app, None);
                        self.cancel(View::Inbox);
                        self.cancel(View::Actions);
                        app.github = None;
                        app.actions = None;
                        app.github_action = None;
                        app.actions_action = None;
                        app.avatar_states.clear();
                        app.content[View::Inbox.index()] = vec!["Identity unavailable • private pages hidden; Refresh to verify identity again".into()];
                        app.content[View::Actions.index()] = vec!["Identity unavailable • private Actions data hidden; account-scoped storage preserved".into()];
                        if app.view != View::Repository {
                            app.context =
                                "GitHub private pages hidden • original storage preserved".into();
                        }
                        if app
                            .form
                            .as_ref()
                            .is_some_and(|form| form.view() != View::Repository)
                        {
                            app.form = None;
                            if app.overlay == Some(crate::app::Overlay::Form) {
                                app.overlay = None;
                            }
                            app.form_dismissed = true;
                        }
                    }
                    if let Some(catalogue) = prepared.catalogue {
                        if matches!(catalogue.kind,crate::repository_catalogue_native::CataloguePreparedKind::Activated {..} | crate::repository_catalogue_native::CataloguePreparedKind::Opened {..}) {
                            for view in [View::Inbox,View::Actions,View::Search,View::Dashboard] {self.cancel(view);}
                        }
                        if !crate::repository_catalogue_native::accept(app, catalogue) {
                            changed = true;
                            continue;
                        }
                    }
                    if let Some(scope) = prepared.remote_scope {
                        app.remote_scope = Some(scope);
                    }
                    if let Some(source) = prepared.remote_scope_source {
                        app.remote_scope_source = Some(source);
                    }
                    if prepared.clear_metrics {
                        app.metrics = None;
                        app.metrics_action = None;
                        app.metrics_load_requested = false;
                        app.metrics_content = Default::default();
                        app.hits.clear();
                        app.avatar_states.clear();
                    }
                    if prepared.invalidate_pr_metadata {
                        crate::pr_checks_native::retire(app, true);
                        self.cancel(View::Search);
                        app.loading[View::Search.index()] = false;
                        // Metadata invalidation is not an account-change claim.
                        // Retain explicit scope/source/query intent but drop old
                        // frames, and never dismiss the just-published original.
                        if let Some(search) = &mut app.search {
                            search.page = None;
                            search.preview = None;
                            search.remote_items.clear();
                            search.blend_evidence = None;
                        }
                        app.search_action = None;
                        app.content[View::Search.index()]=vec!["PR metadata changed • previous private matches hidden; same scoped Search/Load verifies current source visibility.".into()];
                        app.source_context = None;
                        app.hits.clear();
                        app.pr_inspection = None;
                        app.pr_reactions = None;
                        app.github = None;
                        app.github_origin = None;
                        app.content[View::Inbox.index()]=vec!["PR metadata changed • previous private pages hidden; Refresh verifies current metadata. Original receipt/barrier is retained; do not resend.".into()];
                        if app.view == View::Inbox {
                            app.context =
                                "PR metadata changed • original protection retained".into();
                        }
                    }
                    if prepared.invalidate_metrics && prepared.metrics.is_none() {
                        self.cancel(View::Dashboard);
                        app.loading[View::Dashboard.index()] = false;
                        app.metrics = None;
                        app.metrics_action = None;
                        app.metrics_content = Default::default();
                        app.content[View::Dashboard.index()]=vec!["Original PR publication changed • previous Dashboard values hidden; Refresh verifies its sources.".into()];
                        if app.view == View::Dashboard {
                            app.context =
                                "Dashboard source changed • previous values hidden".into();
                            app.status =
                                "Dashboard source changed • Refresh before inspecting values"
                                    .into();
                        }
                    }
                    if let Some(search) = prepared.search {
                        if let Some(identity) = &search.remote_identity {
                            self.accept_remote_principal(app, identity);
                        }
                        if prepared.restore_search_input {
                            app.search_controls.query =
                                crate::editor::TextBuffer::new(&search.input).unwrap_or_default();
                        }
                        app.search = Some(search);
                    }
                    if let Some(workspace) = prepared.workspace {
                        if completion.local_navigation_owner.is_none()
                            && self
                                .current_original_local_owner
                                .as_ref()
                                .is_some_and(|owner| !owner.matches_original(&workspace))
                        {
                            self.cancel(View::Repository);
                            app.loading[View::Repository.index()] = false;
                        } // Genuine new source retires the prior owner before model installation.
                        crate::repository_source_ui::retire(app);
                        app.repository_source_identity = prepared.repository_source_identity;
                        app.repository_selection = None;
                        app.repository_mode = workspace.mode;
                        if let Some(index) = workspace
                            .provenance_focus_row()
                            .or_else(|| workspace.graph_focus_row_v1())
                        {
                            app.selected =
                                index.min(workspace.display_rows_v1().len().saturating_sub(1));
                            let visible = app.repository_visible_rows.max(1);
                            if app.selected < app.scroll {
                                app.scroll = app.selected;
                            } else if app.selected >= app.scroll + visible {
                                app.scroll = app.selected + 1 - visible;
                            }
                        }
                        let _ = app.local_commit_actors.install(&workspace);
                        app.local_actor_layout.clear();
                        app.workspace = Some(workspace);
                    }
                    if let Some(github) = prepared.github {
                        {
                            let actor_was_pending = app.local_commit_actors.is_pending();
                            if app.local_commit_actors.observe_principal(&github.identity) {
                                app.local_actor_cancel_requested |= actor_was_pending;
                                if actor_was_pending {
                                    app.loading[View::Repository.index()] = false;
                                }
                                app.avatar_states.clear();
                                app.local_actor_layout.clear();
                            }
                        }
                        self.clear_remote_search(app, Some(&github.identity));
                        let same_target = app.github.as_ref().is_some_and(|previous| {
                            previous.identity.same_account(&github.identity)
                                && previous.pr.as_ref().zip(github.pr.as_ref()).is_some_and(
                                    |(old, new)| {
                                        old.details.summary.pr.id == new.details.summary.pr.id
                                    },
                                )
                        });
                        let changed_pr_tab = same_target
                            && app.github.as_ref().is_some_and(|previous| {
                                previous
                                    .pr
                                    .as_ref()
                                    .zip(github.pr.as_ref())
                                    .is_some_and(|(old, new)| old.tab != new.tab)
                            });
                        if !same_target {
                            app.github_origin = None;
                        }
                        // A new PR observation/publication may invalidate an old
                        // Dashboard source. Preserve navigation intent, not old
                        // values; a canonical metric Open supplies its own newly
                        // checked snapshot in the same completion.
                        if prepared.invalidate_metrics && prepared.metrics.is_none() {
                            self.cancel(View::Dashboard);
                            app.loading[View::Dashboard.index()] = false;
                            app.metrics = None;
                            app.metrics_action = None;
                            app.content[View::Dashboard.index()] = vec!["Original PR publication changed • previous Dashboard values hidden; Refresh verifies its sources.".into()];
                            if app.view == View::Dashboard {
                                app.context =
                                    "Dashboard source changed • previous values hidden".into();
                                app.status =
                                    "Dashboard source changed • Refresh before inspecting values"
                                        .into();
                            }
                        }
                        let selected_comment = if same_target {
                            app.github
                                .as_ref()
                                .and_then(|previous| previous.pr.as_ref())
                                .and_then(|page| page.rows.get(app.selected))
                                .and_then(|row| row.comment.as_ref())
                                .cloned()
                        } else {
                            None
                        };
                        let selected_file = if same_target {
                            app.github
                                .as_ref()
                                .and_then(|previous| previous.pr.as_ref())
                                .and_then(|page| page.selected_progress_file(app.selected))
                                .map(|file| file.path)
                        } else {
                            None
                        };
                        app.selected = selected_comment
                            .and_then(|selected| {
                                github.pr.as_ref().and_then(|page| {
                                    page.rows.iter().position(|row| {
                                        row.comment.as_ref().is_some_and(|current| {
                                            current.kind == selected.kind
                                                && current.observed.id == selected.observed.id
                                                && current.observed.node_id
                                                    == selected.observed.node_id
                                        })
                                    })
                                })
                            })
                            .or_else(|| {
                                github.pr.as_ref().and_then(|page| {
                                    selected_file
                                        .as_deref()
                                        .and_then(|path| page.progress_selection(path))
                                })
                            })
                            .or_else(|| {
                                github.pr.as_ref().and_then(|page| {
                                    page.review_progress
                                        .as_ref()
                                        .and_then(|progress| progress.resume_path.as_deref())
                                        .and_then(|path| page.progress_selection(path))
                                })
                            })
                            .unwrap_or_else(|| github.selected_index());
                        if !same_target {
                            app.pr_reactions = None;
                            app.pr_reaction_display.clear();
                            app.pr_inspection = None;
                            app.pr_pending_message.clear();
                        }
                        if app.github.as_ref().is_some_and(|previous| {
                            previous.identity.host != github.identity.host
                                || previous.identity.login != github.identity.login
                                || previous.identity.account_id != github.identity.account_id
                        }) || app.actions.as_ref().is_some_and(|previous| {
                            !previous.identity.same_account(&github.identity)
                        }) {
                            clear_context_notice(app);
                            app.avatar_states.clear();
                            self.cancel(View::Actions);
                            app.actions_inspection = None;
                            app.actions = None;
                            app.actions_action = None;
                            if app
                                .form
                                .as_ref()
                                .is_some_and(|form| form.view() != View::Repository)
                            {
                                app.form = None;
                                app.overlay = None;
                            }
                        }
                        crate::pr_checks_native::retire(app, true);
                        // Input while a new section loads belongs to the old
                        // rows. Never apply its viewport to a different tab;
                        // preserve intentional same-tab refresh scrolling.
                        if changed_pr_tab && app.view == View::Inbox {
                            app.scroll = 0;
                        }
                        app.github = Some(github);
                        if let Some(origin) = prepared.github_origin {
                            app.github_origin = Some(origin);
                        }
                    }
                    if let Some(metrics) = prepared.metrics {
                        if !prepared.clear_private && !prepared.clear_metrics {
                            app.metrics_selected_repository =
                                if metrics.snapshot.scope.repositories().len() == 1
                                    && metrics.declared_repositories.len() > 1
                                {
                                    metrics.snapshot.scope.repositories().first().cloned()
                                } else {
                                    None
                                };
                            app.metrics_days = ((metrics.snapshot.window.end
                                - metrics.snapshot.window.start)
                                / 86_400)
                                .max(1);
                            app.metrics = Some(metrics);
                            app.metrics_content_focused = true;
                        }
                    }
                    if let Some(actions) = prepared.actions {
                        {
                            let actor_was_pending = app.local_commit_actors.is_pending();
                            if app.local_commit_actors.observe_principal(&actions.identity) {
                                app.local_actor_cancel_requested |= actor_was_pending;
                                if actor_was_pending {
                                    app.loading[View::Repository.index()] = false;
                                }
                                app.avatar_states.clear();
                                app.local_actor_layout.clear();
                            }
                        }
                        if prepared.pr_checks_origin.is_some() {
                            // A genuine closing numeric principal may rename its
                            // login without changing this original account/source.
                            if let Some(github) = &mut app.github {
                                github.identity = actions.identity.clone();
                            }
                        } else {
                            crate::pr_checks_native::retire(app, false);
                        }
                        app.actions_inspection = None;
                        self.clear_remote_search(app, Some(&actions.identity));
                        if app
                            .github
                            .as_ref()
                            .is_some_and(|github| !github.identity.same_account(&actions.identity))
                        {
                            clear_context_notice(app);
                            self.cancel(View::Inbox);
                            app.github = None;
                            app.pr_reactions = None;
                            app.pr_reaction_display.clear();
                            app.pr_inspection = None;
                            app.pr_pending_message.clear();
                            app.github_action = None;
                            app.avatar_states.clear();
                        }
                        if app.actions.as_ref().is_some_and(|previous| {
                            !previous.identity.same_account(&actions.identity)
                        }) {
                            clear_context_notice(app);
                            app.avatar_states.clear();
                        }
                        app.actions = Some(actions);
                    }
                    if let Some(route) = prepared.pr_checks_route {
                        app.pr_checks_route = Some(route);
                        app.actions_content_focused = true;
                        app.github_content_focused = false;
                        app.focus = 0;
                    }
                    if prepared.clear_actions_inspection {
                        app.actions_inspection = None;
                    }
                    if let Some(inspection) = prepared.actions_inspection {
                        let matches_current = app.actions.as_ref().is_some_and(|actions| {
                            let guard = inspection
                                .lock()
                                .unwrap_or_else(|poison| poison.into_inner());
                            guard.as_ref().is_some_and(|proof| {
                                actions.identity.account_id.is_some_and(|id| id > 0)
                                    && actions
                                        .identity
                                        .same_account(proof.observation().identity())
                                    && crate::storage::Scope::github(
                                        &actions.identity.host,
                                        actions.identity.account_id,
                                    )
                                    .is_ok_and(|scope| &scope == proof.marker().target.scope())
                            })
                        });
                        if !prepared.clear_private && matches_current {
                            app.actions_inspection = Some(inspection);
                        }
                    }
                    if prepared.clear_pr_inspection {
                        app.pr_inspection = None;
                    }
                    if let Some(inspection) = prepared.pr_inspection {
                        app.pr_inspection = Some(inspection);
                    }
                    if let Some(message) = prepared.pr_pending_message {
                        app.pr_pending_message = message;
                    }
                    if let Some(message) = prepared.source_message {
                        if app.view == completion.view
                            && app.overlay == Some(crate::app::Overlay::SearchSources)
                        {
                            app.source_controls.message = message;
                        }
                    }
                    if let Some(context) = prepared.source_context {
                        if !prepared.clear_private
                            && app.view == completion.view
                            && app.overlay.is_none()
                            && app
                                .search
                                .as_ref()
                                .is_some_and(|search| search.scope == context.scope)
                        {
                            app.source_context = Some(context);
                            app.source_context_scroll = 0;
                            app.focus = 0;
                            app.overlay = Some(crate::app::Overlay::SearchContext);
                        }
                    }
                    if let Some(message) = prepared.readonly_notice {
                        if !prepared.clear_private
                            && app.view == completion.view
                            && !app.form_dismissed
                            && app.overlay.is_none()
                        {
                            app.dispatch(crate::app::Action::Explain(message));
                        }
                    }
                    if let Some(page) = prepared.pr_reactions {
                        let exact_pr = app.github.as_ref().is_some_and(|github| {
                            github.identity.same_account(&page.identity)
                                && github.pr.as_ref().is_some_and(|current| {
                                    let target = &current.details.summary;
                                    target.pr.id == page.target.pr.id
                                        && target.pr.number == page.target.pr.number
                                        && target
                                            .pr
                                            .repository
                                            .eq_ignore_ascii_case(&page.target.pr.repository)
                                        && target.head_sha == page.target.head_sha
                                        && target.base_sha == page.target.base_sha
                                })
                        });
                        if !prepared.clear_private
                            && exact_pr
                            && app.view == completion.view
                            && !app.form_dismissed
                            && app.overlay.is_none()
                        {
                            app.pr_reactions = Some(page);
                            app.pr_reaction_display =
                                prepared.pr_reaction_display.unwrap_or_default();
                            app.pr_reaction_scroll = 0;
                            app.focus = 0;
                            app.overlay = Some(crate::app::Overlay::PrReactions);
                        }
                    }
                    if let Some(form) = prepared.form {
                        if form.submitted {
                            if app
                                .form
                                .as_ref()
                                .is_some_and(|current| current.resource() == form.resource())
                            {
                                app.form = Some(form);
                                if app.overlay == Some(crate::app::Overlay::Form) {
                                    app.overlay = None;
                                }
                            }
                        } else if app.view == completion.view
                            && !app.form_dismissed
                            && app.overlay != Some(crate::app::Overlay::Settings)
                        {
                            app.overlay = Some(crate::app::Overlay::Form);
                            app.form = Some(form);
                        }
                    }
                    if app.view == completion.view {
                        if let Some(destination) = prepared.navigate_to {
                            app.view = destination;
                            app.repository_content_focused = destination == View::Repository;
                            app.repository_mode =
                                app.workspace.as_ref().map_or(
                                    crate::workspace::RepositoryMode::Overview,
                                    |workspace| workspace.mode,
                                );
                            app.scroll = 0;
                            app.selected = 0;
                        }
                    }
                    if prepared.activate_if_untouched && !app.user_navigated {
                        app.view = completion.view;
                    }
                    // An empty result must not erase the safe privacy/recovery
                    // fallback installed above or leave a blank authenticated view.
                    if !prepared.clear_private || !prepared.lines.is_empty() {
                        app.content[completion.view.index()] = prepared.lines;
                    }
                    if app.view == completion.view || prepared.navigate_to == Some(app.view) {
                        if let Some(context) = prepared.context {
                            app.context = context;
                        }
                        app.status = prepared.status;
                    }
                    if let Some(restore) = prepared.navigation_restore {
                        app.finish_navigation(restore);
                    }
                }
                Err(error) => {
                    if completion.remote_repository_owner.is_some() {
                        crate::remote_repository_native_v1::worker_failed(app);
                        changed = true;
                        continue;
                    }
                    if let Some(owner) = &completion.local_actor_owner {
                        if !app.local_commit_actors.fail(owner) {
                            continue;
                        }
                        app.local_actor_layout.clear();
                        app.avatar_states.clear();
                        app.hits.clear();
                        app.status = "Host report unavailable; original local Git remains • verify explicitly".into();
                        changed = true;
                        continue;
                    }
                    if slot == 5 {
                        crate::repository_catalogue_native::privacy(app);
                        crate::repository_catalogue_native::retire_remote_views(app);
                        self.cancel_catalogue();
                        for view in [View::Inbox, View::Actions, View::Search, View::Dashboard] {
                            self.cancel(view);
                        }
                        changed = true;
                        continue;
                    }
                    if completion.view == View::Repository && app.navigation_restore_inflight {
                        app.abandon_navigation_restore();
                    }
                    if let Some(form) = &mut app.form {
                        if form.submitting && app.view == completion.view {
                            form.error = error.clone();
                            form.error_scroll = 0;
                            form.submitting = false;
                        }
                    }
                    if app.view == completion.view {
                        app.status = format!("Error: {error} • r Refresh to retry");
                    }
                }
            }
            changed = true;
        }
        changed
    }
}

fn clear_context_notice(app: &mut App) {
    crate::remote_repository_native_v1::retire(app);
    app.retire_command_context();
    app.clear_capability_inspector();
    app.source_controls = Default::default();
    app.source_action = None;
    app.source_context = None;
    app.source_context_scroll = 0;
    app.notice.clear();
    app.notice_scroll = 0;
    app.pr_reactions = None;
    app.pr_reaction_display.clear();
    app.pr_reaction_scroll = 0;
    if matches!(
        app.overlay,
        Some(
            crate::app::Overlay::Notice
                | crate::app::Overlay::PrReactions
                | crate::app::Overlay::SearchSources
                | crate::app::Overlay::SearchContext
        )
    ) {
        app.overlay = None;
    }
}

impl Default for Background {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Background {
    fn drop(&mut self) {
        for (_, cancellation) in self.current.values() {
            cancellation.cancel();
        }
        self.requests.take();
        let (_, replacement) = mpsc::sync_channel(1);
        self.completions = replacement;
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
#[path = "runtime/pr_metadata_viewport_v9_tests.rs"]
mod pr_metadata_viewport_v9_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn identity(account_id: u64) -> crate::providers::gh::HostIdentity {
        crate::providers::gh::HostIdentity {
            host: "github.com".into(),
            login: "same-login".into(),
            account_id: Some(account_id),
            gh_version: "2.99.0".into(),
        }
    }

    fn github(account_id: u64) -> crate::github_workspace::GithubWorkspace {
        crate::github_workspace::GithubWorkspace::empty(
            identity(account_id),
            vec!["org/private".into()],
            Default::default(),
        )
    }

    #[test]
    fn gh_provider_inspector_mouse_keyboard_resize_runtime_reused_login_new_uid_drops_overlay_hits()
    {
        let mut app = App {
            github: Some(github(42)),
            ..App::default()
        };
        app.dispatch(crate::app::Action::Capabilities);
        assert!(app.capability_inspector.is_some());
        let mut background = Background::new();
        background.submit(&mut app, View::Inbox, |_| {
            Ok(PreparedView {
                github: Some(github(43)),
                ..Default::default()
            })
        });
        poll_to_completion(&mut background, &mut app, View::Inbox);
        assert!(app.capability_inspector.is_none());
        assert!(app.capability_hits.is_empty());
        assert_eq!(app.overlay, None);
        assert!(app.hits.is_empty());
        assert_eq!(app.github.as_ref().unwrap().identity.account_id, Some(43));
        assert_eq!(app.github.as_ref().unwrap().identity.login, "same-login");
    }

    #[test]
    fn gh_provider_inspector_mouse_keyboard_resize_runtime_private_clear_without_local_retargeting()
    {
        let mut app = App {
            view: View::Repository,
            context: "Local preserved saved version".into(),
            github: Some(github(42)),
            ..App::default()
        };
        app.dispatch(crate::app::Action::Capabilities);
        let mut background = Background::new();
        background.submit(&mut app, View::Inbox, |_| {
            Ok(PreparedView {
                clear_private: true,
                ..Default::default()
            })
        });
        poll_to_completion(&mut background, &mut app, View::Inbox);
        assert!(app.capability_inspector.is_none());
        assert!(app.capability_hits.is_empty());
        assert_eq!(app.overlay, None);
        assert_eq!(app.view, View::Repository);
        assert_eq!(app.context, "Local preserved saved version");
        assert!(app.github.is_none());
    }

    fn remote_search(account_id: u64) -> crate::search::workspace::SearchWorkspace {
        let mut search = crate::search::workspace::SearchWorkspace::new(
            crate::storage::Scope::github("github.com", Some(account_id)).unwrap(),
            vec![],
        );
        search.mode = crate::search::SearchMode::GitHub;
        search.remote_identity = Some(identity(account_id));
        search.remote_repositories = vec!["org/private".into()];
        search.input = "PRIVATE_QUERY".into();
        search
    }

    fn poll_to_completion(background: &mut Background, app: &mut App, view: View) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.loading[view.index()] && Instant::now() < deadline {
            background.poll(app);
            thread::yield_now();
        }
        assert!(!app.loading[view.index()], "worker must complete in test");
    }

    fn metrics(account_id: u64) -> crate::metrics_workspace::MetricsWorkspace {
        let evidence = std::sync::Arc::new(
            crate::metrics_repository::repository_evidence(
                &identity(account_id),
                &["org/private".into()],
                &[],
            )
            .unwrap(),
        );
        let snapshot = std::sync::Arc::new(
            crate::metrics::dashboard(
                &evidence,
                100,
                crate::metrics::UtcWindow::new(1, 100).unwrap(),
            )
            .unwrap(),
        );
        crate::metrics_workspace::MetricsWorkspace::prepare(
            evidence,
            snapshot,
            vec!["org/private".into(), "org/other".into()],
            std::sync::Arc::from(vec![]),
            std::sync::Arc::from(vec![]),
        )
        .unwrap()
    }

    #[test]
    fn metrics_runtime_all_principal_channels_hide_original_dashboard_and_preserve_local_context() {
        for channel in 0..4 {
            for view in [View::Dashboard, View::Repository] {
                let mut background = Background::new();
                let mut app = App {
                    view,
                    metrics: Some(metrics(42)),
                    context: if view == View::Repository {
                        "Local checkout / dirty bytes".into()
                    } else {
                        "github.com / @same-login / org/private / PrivateDashboardCanary".into()
                    },
                    ..Default::default()
                };
                assert!(
                    background.submit(&mut app, View::Inbox, move |_| Ok(PreparedView {
                        verified_identity: (channel == 0 || channel == 3)
                            .then(|| identity(if channel == 3 { 42 } else { 43 })),
                        github: (channel == 1).then(|| github(43)),
                        clear_private: channel == 2,
                        ..Default::default()
                    }))
                );
                poll_to_completion(&mut background, &mut app, View::Inbox);
                assert_eq!(app.metrics.is_some(),channel==3,"same numeric account retains its immutable snapshot; all new/unavailable principals hide it");
                if view == View::Repository {
                    assert_eq!(app.context, "Local checkout / dirty bytes");
                } else if channel != 3 {
                    assert!(!app.context.contains("org/private"));
                    assert!(!app.context.contains("PrivateDashboardCanary"));
                    assert!(app.context.contains("hidden"));
                }
            }
        }
    }

    #[test]
    fn metrics_runtime_publication_hides_old_values_without_erasing_successful_scope_window_intent()
    {
        let mut background = Background::new();
        let mut app = App {
            view: View::Dashboard,
            metrics: Some(metrics(42)),
            metrics_selected_repository: Some("org/private".into()),
            metrics_days: 30,
            ..Default::default()
        };
        assert!(
            background.submit(&mut app, View::Inbox, |_| Ok(PreparedView {
                github: Some(github(42)),
                invalidate_metrics: true,
                ..Default::default()
            }))
        );
        poll_to_completion(&mut background, &mut app, View::Inbox);
        assert!(app.metrics.is_none());
        assert_eq!(
            app.metrics_selected_repository.as_deref(),
            Some("org/private")
        );
        assert_eq!(app.metrics_days, 30);
        assert!(app.content[View::Dashboard.index()][0].contains("publication changed"));
        assert!(app.context.contains("previous values hidden"));
        assert!(app.status.contains("Refresh"));
    }

    #[test]
    fn pr_metadata_actions_runtime_failed_refresh_hides_dashboard_without_new_github_frame_and_preserves_navigation_intent(
    ) {
        let mut background = Background::new();
        let mut app = App {
            view: View::Dashboard,
            metrics: Some(metrics(42)),
            metrics_selected_repository: Some("org/private".into()),
            metrics_days: 30,
            context: "org/private / PrivateOldMetricCanary".into(),
            ..Default::default()
        };
        background.submit(&mut app, View::Inbox, |_| {
            Ok(PreparedView {
                invalidate_metrics: true,
                invalidate_pr_metadata: true,
                status: "Published • private pages hidden • do not resend".into(),
                ..Default::default()
            })
        });
        poll_to_completion(&mut background, &mut app, View::Inbox);
        assert!(app.metrics.is_none() && app.github.is_none());
        assert_eq!(
            app.metrics_selected_repository.as_deref(),
            Some("org/private")
        );
        assert_eq!(app.metrics_days, 30);
        assert!(app.context.contains("previous values hidden"));
        assert!(!app.context.contains("PrivateOldMetricCanary"));
        assert!(
            app.content[View::Dashboard.index()][0].contains("previous Dashboard values hidden")
        );
    }

    #[test]
    fn collaboration_index_blended_privacy_safe_recovery_survives_empty_result_and_hides_breadcrumb(
    ) {
        for view in [View::Inbox, View::Actions, View::Repository] {
            for explicit_recovery in [false, true] {
                let mut background = Background::new();
                let mut app = App {
                    view,
                    context: if view == View::Repository {
                        "Local checkout / working files".into()
                    } else {
                        "github.com / org/private#7 / PrivateHeadCanary".into()
                    },
                    github: Some(github(42)),
                    actions: Some(crate::actions_workspace::ActionsWorkspace::new(
                        identity(42),
                        vec!["org/private".into()],
                    )),
                    ..Default::default()
                };
                assert!(background.submit(&mut app, View::Inbox, move |_| {
                    Ok(PreparedView {
                        clear_private: true,
                        lines: if explicit_recovery {
                            vec![
                                "Publication is confirmed. Do not resend this original request."
                                    .into(),
                            ]
                        } else {
                            vec![]
                        },
                        status: "Published • private pages hidden • do not resend".into(),
                        ..Default::default()
                    })
                }));
                poll_to_completion(&mut background, &mut app, View::Inbox);
                assert!(app.github.is_none() && app.actions.is_none());
                assert!(!app.content[View::Inbox.index()].is_empty());
                if explicit_recovery {
                    assert!(app.content[View::Inbox.index()][0].contains("Do not resend"));
                } else {
                    assert!(app.content[View::Inbox.index()][0].contains("private pages hidden"));
                }
                if view == View::Repository {
                    assert_eq!(app.context, "Local checkout / working files");
                } else {
                    assert!(!app.context.contains("org/private"));
                    assert!(!app.context.contains("PrivateHeadCanary"));
                    assert!(app.context.contains("private pages hidden"));
                }
            }
        }
    }

    #[test]
    fn selected_corpus_verified_form_hides_old_account_local_results_and_respects_user_dismissal() {
        for dismissed in [false, true] {
            let old_scope = crate::storage::Scope::github("github.com", Some(1)).unwrap();
            let mut old_search = crate::search::workspace::SearchWorkspace::new(old_scope, vec![]);
            old_search.input = "PRIVATE_OLD_ACCOUNT_QUERY".into();
            assert!(old_search.remote_identity.is_none());
            let mut app = App {
                view: View::Search,
                github: Some(github(1)),
                actions: Some(crate::actions_workspace::ActionsWorkspace::new(
                    identity(1),
                    vec!["org/private".into()],
                )),
                search: Some(old_search),
                form_dismissed: dismissed,
                ..Default::default()
            };
            let mut background = Background::new();
            background.submit(&mut app, View::Search, move |_| {
                let intent = crate::corpus_ui::CorpusIntent {
                    root: "/prepared/corpus".into(),
                    identity: identity(2),
                    allowed_repositories: vec!["org/private".into()],
                    current_selections: vec![],
                    catalog: vec![],
                    recoveries: vec![],
                    orphans: vec![],
                    choices: Default::default(),
                    plan: None,
                    progress: Default::default(),
                };
                Ok(PreparedView {
                    form: Some(crate::forms::Form::corpus(intent).unwrap()),
                    verified_identity: Some(identity(2)),
                    status: "Verified new account".into(),
                    ..Default::default()
                })
            });
            poll_to_completion(&mut background, &mut app, View::Search);
            assert!(app.github.is_none());
            assert!(app.actions.is_none());
            assert!(
                app.search.is_none(),
                "Account-scoped local results are private even without remote_identity"
            );
            assert_eq!(app.form.is_none(),dismissed,"An actual dismissal must remain honored; internal old-data clearing cannot dismiss a newly verified form");
            if let Some(form) = &app.form {
                assert_eq!(
                    form.scope(),
                    crate::storage::Scope::github("github.com", Some(2)).unwrap()
                );
                assert_eq!(app.overlay, Some(crate::app::Overlay::Form));
            }
        }
    }

    #[test]
    fn selected_corpus_same_verified_principal_preserves_local_scoped_state_until_replacement() {
        let scope = crate::storage::Scope::github("github.com", Some(1)).unwrap();
        let mut search = crate::search::workspace::SearchWorkspace::new(scope, vec![]);
        search.input = "same account working query".into();
        let mut app = App {
            view: View::Search,
            search: Some(search),
            ..Default::default()
        };
        let mut background = Background::new();
        background.submit(&mut app, View::Search, |_| {
            Ok(PreparedView {
                verified_identity: Some(identity(1)),
                status: "Same verified account".into(),
                ..Default::default()
            })
        });
        poll_to_completion(&mut background, &mut app, View::Search);
        assert_eq!(app.search.unwrap().input, "same account working query");
    }

    #[test]
    fn cache_storage_async_events_private_identity_failure_dismisses_notice_not_settings() {
        for overlay in [crate::app::Overlay::Notice, crate::app::Overlay::Settings] {
            let mut background = Background::new();
            let mut app = App {
                github: Some(github(42)),
                actions: Some(crate::actions_workspace::ActionsWorkspace::new(
                    identity(42),
                    vec!["org/private".into()],
                )),
                notice: "Private environment permission explanation".into(),
                notice_scroll: 12,
                overlay: Some(overlay),
                ..App::default()
            };
            assert!(background.submit(&mut app, View::Inbox, |_| {
                Ok(PreparedView {
                    clear_private: true,
                    ..PreparedView::default()
                })
            }));
            poll_to_completion(&mut background, &mut app, View::Inbox);
            assert!(app.notice.is_empty());
            assert_eq!(app.notice_scroll, 0);
            assert!(app.github.is_none() && app.actions.is_none());
            assert_eq!(
                app.overlay,
                (overlay == crate::app::Overlay::Settings).then_some(overlay)
            );
        }
    }

    #[test]
    fn cache_storage_async_events_same_login_new_principal_clears_context_notice() {
        for (view, previous_github, previous_actions) in [
            (View::Inbox, true, false),
            (View::Inbox, false, true),
            (View::Actions, true, false),
            (View::Actions, false, true),
        ] {
            let mut background = Background::new();
            let mut app = App {
                github: previous_github.then(|| github(42)),
                actions: previous_actions.then(|| {
                    crate::actions_workspace::ActionsWorkspace::new(
                        identity(42),
                        vec!["org/private".into()],
                    )
                }),
                notice: "Private repository permission explanation".into(),
                notice_scroll: 8,
                overlay: Some(crate::app::Overlay::Notice),
                ..App::default()
            };
            assert!(background.submit(&mut app, view, move |_| {
                Ok(PreparedView {
                    github: (view == View::Inbox).then(|| github(43)),
                    actions: (view == View::Actions).then(|| {
                        crate::actions_workspace::ActionsWorkspace::new(
                            identity(43),
                            vec!["org/private".into()],
                        )
                    }),
                    ..PreparedView::default()
                })
            }));
            poll_to_completion(&mut background, &mut app, view);
            assert!(app.notice.is_empty());
            assert_eq!(app.notice_scroll, 0);
            assert_eq!(app.overlay, None);
            assert_eq!(
                app.github
                    .as_ref()
                    .map(|workspace| workspace.identity.account_id)
                    .or_else(|| app
                        .actions
                        .as_ref()
                        .map(|workspace| workspace.identity.account_id)),
                Some(Some(43))
            );
        }
    }

    #[test]
    fn github_search_async_events_principal_change_clears_remote_query_and_ignores_late_result() {
        for view in [View::Inbox, View::Actions] {
            let mut background = Background::new();
            let mut app = App {
                view: View::Search,
                search: Some(remote_search(42)),
                notice: "Private search permission explanation".into(),
                overlay: Some(crate::app::Overlay::Notice),
                ..App::default()
            };
            app.search_controls.query = crate::editor::TextBuffer::new("PRIVATE_QUERY").unwrap();
            app.search_controls.name =
                crate::editor::TextBuffer::new("Private repo search").unwrap();
            app.search_controls.corpus = true;
            app.context = "org/private / same-login".into();
            app.hits.push(crate::app::HitRegion {
                rect: ratatui::layout::Rect::new(1, 1, 10, 1),
                action: crate::app::Action::Search(crate::search::workspace::SearchAction::Query),
            });
            let (release, waiting) = mpsc::channel();
            let (started, running) = mpsc::channel();
            assert!(background.submit(&mut app, View::Search, move |_| {
                started.send(()).unwrap();
                waiting.recv().unwrap();
                Ok(PreparedView {
                    search: Some(remote_search(42)),
                    lines: vec!["PRIVATE_LATE_RESULT".into()],
                    restore_search_input: true,
                    ..PreparedView::default()
                })
            }));
            running.recv_timeout(Duration::from_secs(2)).unwrap();
            assert!(background.submit(&mut app, view, move |_| {
                Ok(PreparedView {
                    github: (view == View::Inbox).then(|| github(43)),
                    actions: (view == View::Actions).then(|| {
                        crate::actions_workspace::ActionsWorkspace::new(identity(43), vec![])
                    }),
                    ..PreparedView::default()
                })
            }));
            poll_to_completion(&mut background, &mut app, view);
            assert!(app.search.is_none());
            assert!(!app.loading[View::Search.index()]);
            assert!(app.search_controls.query.text.is_empty());
            assert_eq!(app.search_controls.name.text, "My search");
            assert!(!app.search_controls.corpus);
            assert!(app.hits.is_empty());
            assert!(app.notice.is_empty());
            assert_eq!(app.overlay, None);
            assert!(!app.context.contains("org/private"));
            assert!(!app.content[View::Search.index()]
                .join(" ")
                .contains("PRIVATE_QUERY"));
            release.send(()).unwrap();
            // A subsequent request must finish behind the old result on its worker.
            assert!(
                background.submit(&mut app, View::Search, |_| Ok(PreparedView {
                    lines: vec!["safe current frame".into()],
                    ..PreparedView::default()
                }))
            );
            poll_to_completion(&mut background, &mut app, View::Search);
            assert!(app.search.is_none());
            assert_eq!(app.content[View::Search.index()], ["safe current frame"]);
            assert!(app.search_controls.query.text.is_empty());
        }
    }

    #[test]
    fn github_search_async_events_fresh_remote_principal_replaces_old_views_not_local_work() {
        for account_id in [42, 43] {
            let mut background = Background::new();
            let fixture = tempfile::tempdir().unwrap();
            let database = fixture.path().join("private/cache.sqlite3");
            let scope = crate::storage::Scope::github("github.com", Some(42)).unwrap();
            let cache = crate::storage::Cache::open(&database).unwrap();
            cache
                .put_draft(&scope, "private-draft", "preserved private draft")
                .unwrap();
            let mut app = App {
                view: View::Search,
                github: Some(github(42)),
                actions: Some(crate::actions_workspace::ActionsWorkspace::new(
                    identity(42),
                    vec![],
                )),
                notice: "Private old-account permission explanation".into(),
                overlay: Some(crate::app::Overlay::Notice),
                ..App::default()
            };
            assert!(
                background.submit(&mut app, View::Search, move |_| Ok(PreparedView {
                    search: Some(remote_search(account_id)),
                    restore_search_input: true,
                    ..PreparedView::default()
                }))
            );
            poll_to_completion(&mut background, &mut app, View::Search);
            assert_eq!(
                app.search
                    .as_ref()
                    .unwrap()
                    .remote_identity
                    .as_ref()
                    .unwrap()
                    .account_id,
                Some(account_id)
            );
            assert_eq!(app.search_controls.query.text, "PRIVATE_QUERY");
            if account_id == 43 {
                assert!(app.github.is_none() && app.actions.is_none());
                assert!(app.notice.is_empty());
                assert_eq!(app.overlay, None);
            } else {
                assert!(app.github.is_some() && app.actions.is_some());
            }
            assert_eq!(
                cache.draft(&scope, "private-draft").unwrap().as_deref(),
                Some("preserved private draft")
            );
        }
        let mut background = Background::new();
        let mut app = App::default();
        let mut local = crate::search::workspace::SearchWorkspace::new(
            crate::storage::Scope {
                host: "local".into(),
                login: "workspace".into(),
            },
            vec![],
        );
        local.input = "local query".into();
        app.search = Some(local);
        assert!(
            background.submit(&mut app, View::Inbox, |_| Ok(PreparedView {
                clear_private: true,
                ..PreparedView::default()
            }))
        );
        poll_to_completion(&mut background, &mut app, View::Inbox);
        assert_eq!(app.search.unwrap().input, "local query");
    }

    #[test]
    fn async_events_reordered_completions_preserve_latest_selection() {
        let mut background = Background::new();
        let mut app = App::default();
        let (release, waiting) = mpsc::channel();
        let (started, running) = mpsc::channel();
        background.submit(&mut app, View::Inbox, move |_| {
            started.send(()).unwrap();
            waiting.recv().unwrap();
            Ok(PreparedView {
                lines: vec!["old".into()],
                context: None,
                status: "old".into(),
                workspace: None,
                github: None,
                form: None,
                activate_if_untouched: false,
                ..PreparedView::default()
            })
        });
        running.recv_timeout(Duration::from_secs(2)).unwrap();
        background.submit(&mut app, View::Inbox, |_| {
            Ok(PreparedView {
                lines: vec!["new".into()],
                context: None,
                status: "new".into(),
                workspace: None,
                github: None,
                form: None,
                activate_if_untouched: false,
                ..PreparedView::default()
            })
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.content[0][0] != "new" && Instant::now() < deadline {
            background.poll(&mut app);
            thread::yield_now();
        }
        assert_eq!(app.content[0], ["new"]);
        release.send(()).unwrap();
        background.cancel(View::Inbox);
        assert_eq!(app.status, "new");
    }

    #[test]
    fn async_events_hidden_view_completion_does_not_change_active_status() {
        let mut background = Background::new();
        let mut app = App::default();
        background.submit(&mut app, View::Inbox, |_| {
            Ok(PreparedView {
                lines: vec!["loaded".into()],
                context: Some("old context".into()),
                status: "old status".into(),
                workspace: None,
                github: None,
                form: None,
                activate_if_untouched: false,
                ..PreparedView::default()
            })
        });
        app.dispatch(crate::app::Action::View(View::Search));
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.loading[0] && Instant::now() < deadline {
            background.poll(&mut app);
            thread::yield_now();
        }
        assert_eq!(app.content[0], ["loaded"]);
        assert_eq!(app.status, "Search • focused view");
        assert!(!app.context.contains("old context"));
    }

    #[test]
    fn async_events_bounded_queue_explains_saturation() {
        let mut background = Background::new();
        let mut app = App::default();
        let mut releases = Vec::new();
        for view in [View::Inbox, View::Repository] {
            let (release, waiting) = mpsc::channel();
            let (started, running) = mpsc::channel();
            releases.push(release);
            assert!(background.submit(&mut app, view, move |_| {
                started.send(()).unwrap();
                waiting.recv().unwrap();
                Err("deliberate worker failure".into())
            }));
            running.recv_timeout(Duration::from_secs(2)).unwrap();
        }
        for _ in 0..4 {
            assert!(background.submit(&mut app, View::Search, |_| Err("queued failure".into())));
        }
        assert!(!background.submit(&mut app, View::Dashboard, |_| Err("must not run".into())));
        assert!(app.status.contains("queue is full"));
        assert!(!app.loading[View::Dashboard.index()]);
        for release in releases {
            release.send(()).unwrap();
        }
    }

    #[test]
    fn async_events_worker_panic_becomes_visible_failure() {
        let mut background = Background::new();
        let mut app = App::default();
        background.submit(&mut app, View::Inbox, |_| {
            panic!("intentional worker fault")
        });
        let deadline = Instant::now() + Duration::from_secs(2);
        while app.loading[0] && Instant::now() < deadline {
            background.poll(&mut app);
            thread::yield_now();
        }
        assert!(app.status.contains("failed unexpectedly"));
        assert!(!app.loading[0]);
    }
}

#[cfg(test)]
#[path = "scoped_remote_runtime_v1_tests.rs"]
mod scoped_remote_runtime_v1_tests;

#[cfg(test)]
#[path = "repository_catalogue_recovery_runtime_v1_tests.rs"]
mod repository_catalogue_recovery_runtime_v1_tests;

#[cfg(test)]
#[path = "repository_commit_actor_runtime_v1_tests.rs"]
mod repository_commit_actor_runtime_v1_tests;
