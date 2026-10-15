//! Undeclared child of scoped::choices. SAME original session and publisher;
//! bounded read-only continuations. None of these retained objects is access.
use super::*;
use crate::providers::gh::{
    PrCheckAnnotationLimitsV2, PrCheckAnnotationsReadV2, PrCheckAttemptAdvanceV2,
    PrCheckLogsObservation, PrCheckRefreshReadV2,
};
#[path = "scoped_pr_check_continuations_corrective_v3.rs"]
pub mod corrective_v3;
#[path = "pr_check_watch_quota_v2.rs"]
mod quota;
#[path = "pr_check_loaded_log_search_v2.rs"]
mod search;
#[path = "scoped_pr_check_continuations_ui_v2.rs"]
pub mod ui;
pub use quota::PrCheckWatchModeV2;
pub(crate) use quota::{PrCheckWatchClaimV2, PrCheckWatchQuotaV2};
pub(crate) use search::prepare_loaded_log_indices_v3;
pub use search::{LoadedLogMatchesV2, LoadedLogSearchModeV2};

/// A stable original run/attempt pin, not an access lease. Holds no capture
/// history/route chain: one bounded baseline observation, one finite quota.
pub struct PrCheckWatchV2 {
    origin: Arc<PrChecksOrigin>,
    observed: Arc<PrCheckRunObservation>,
    job: Option<u64>,
    mode: PrCheckWatchModeV2,
    quota: Arc<quota::PrCheckWatchQuotaV2>,
}
impl PrCheckWatchV2 {
    fn matches(&self, route: &PrChecksRoute) -> bool {
        let Some(old) = self.origin.scoped.as_ref() else {
            return false;
        };
        let Some(new) = route.origin.scoped.as_ref() else {
            return false;
        };
        let Some(a) = self.observed.selected_run_v1() else {
            return false;
        };
        let Some(b) = route.observation.selected_run_v1() else {
            return false;
        };
        Arc::ptr_eq(&old.source, &new.source)
            && Arc::ptr_eq(&old.active, &new.active)
            && Arc::ptr_eq(&old.resource, &new.resource)
            && old.navigation.original() == new.navigation.original()
            && same_source(&self.origin.summary, &route.origin.summary)
            && a.attempt().run.id == b.attempt().run.id
            && a.attempt().run.attempt == b.attempt().run.attempt
            && a.attempt().run.head_sha == b.attempt().run.head_sha
            && a.node_v2() == b.node_v2()
            && self
                .job
                .is_none_or(|id| b.members().iter().any(|j| j.id() == id))
    }
    pub fn active(&self) -> bool {
        self.quota.active()
    }
}

/// Exact pointer facets are checked by the SAME Background owner before any
/// success/error/panic/loading/status/private effects. A new ticket supersedes
/// the old one even when all displayed names are unchanged.
pub struct PrCheckContinuationOwnerV2 {
    route: Arc<PrChecksRoute>,
    capture: Option<Arc<PrCheckLogsObservation>>,
    watch: Option<Arc<PrCheckWatchV2>>,
    deadline: Instant,
}
impl std::fmt::Debug for PrCheckContinuationOwnerV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrCheckContinuationOwnerV2(<original volatile pointer facets>)")
    }
}
impl PrCheckContinuationOwnerV2 {
    pub fn origin(&self) -> &Arc<PrChecksOrigin> {
        &self.route.origin
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn owns(&self, app: &App) -> bool {
        app.view == View::Actions
            && owns_origin(app, &self.route.origin)
            && app
                .pr_checks_route
                .as_ref()
                .is_some_and(|r| Arc::ptr_eq(r, &self.route))
            && app
                .pr_check_continuation_owner_v2
                .as_ref()
                .is_some_and(|v| std::ptr::eq(v.as_ref(), self))
            && self.capture.as_ref().is_none_or(|c| {
                self.route
                    .loaded_capture_v2
                    .as_ref()
                    .is_some_and(|old| Arc::ptr_eq(old, c))
            })
            && self.watch.as_ref().is_none_or(|watch| {
                app.pr_check_watch_v2
                    .as_ref()
                    .is_some_and(|old| Arc::ptr_eq(old, watch))
                    && watch.matches(&self.route)
            })
        // Time/capture limits govern worker publication, NOT ownership.
        // An actual current failed proof must still hide private frames;
        // user Stop/Back removes the pointer and rejects every effect.
    }
}
pub struct PrCheckContinuationWorkV2 {
    owner: Arc<PrCheckContinuationOwnerV2>,
    kind: WorkKindV2,
}
enum WorkKindV2 {
    Annotations {
        choice: PrCheckChoiceV1,
    },
    Refresh {
        choice: PrCheckChoiceV1,
    },
    Logs {
        choice: PrCheckChoiceV1,
        matched: bool,
    },
    Search {
        capture: Arc<PrCheckLogsObservation>,
        query: String,
        mode: LoadedLogSearchModeV2,
    },
}
impl PrCheckContinuationWorkV2 {
    pub fn owner(&self) -> &Arc<PrCheckContinuationOwnerV2> {
        &self.owner
    }
}
pub struct PrCheckContinuationCompletionV2 {
    owner: Arc<PrCheckContinuationOwnerV2>,
    outcome: OutcomeV2,
    watch_progress: Option<(usize, usize)>,
}
enum OutcomeV2 {
    Remote(Box<ScopedPrCheckCompletionV1>),
    Search(LoadedLogMatchesV2),
    SearchUnavailable,
    Advanced(Arc<PrCheckAttemptAdvanceV2>),
    Hidden,
}

fn choice(route: &PrChecksRoute, job: Option<u64>) -> Result<PrCheckChoiceV1, String> {
    let selected = route
        .observation
        .selected_run_v1()
        .ok_or("No positive observed selected run")?;
    if let Some(id) = job {
        let index = selected
            .members()
            .iter()
            .position(|m| m.id() == id)
            .ok_or("No actual observed exact-attempt member; nothing queried")?;
        route
            .observation
            .choose_job_v1(index)
            .map_err(|_| "Original job choice unavailable".into())
    } else {
        route
            .observation
            .choose_selected_run_v2()
            .map_err(|_| "Original selected run choice unavailable".into())
    }
}
pub fn loaded_capture_allowed(route: &PrChecksRoute, workspace: &ActionsWorkspace) -> bool {
    route.loaded_capture_v2().is_some_and(|capture| {
        workspace.run.as_ref().is_some_and(|page| {
            let target = capture.captured_target_v2();
            target.matches_run(&page.attempt.run, page.selected_job)
                && route.observation.selected_run_v1().is_some_and(|run| {
                    run.attempt().run.id == target.run_id()
                        && run.attempt().run.attempt == target.attempt()
                        && run.attempt().run.head_sha == target.head()
                        && run.node_v2() == target.run_node()
                        && run.members().iter().any(|m| {
                            m.id() == target.job_id()
                                && target.job_node().is_none_or(|node| m.node() == node)
                        })
                })
                && capture.identity().host == workspace.identity.host
                && capture.identity().account_id == workspace.identity.account_id
        })
    })
}
pub fn admitted_action(
    route: &PrChecksRoute,
    workspace: &ActionsWorkspace,
    action: &ActionsAction,
) -> bool {
    match action {
        ActionsAction::Refresh | ActionsAction::Watch => {
            route.observation.selected_run_v1().is_some()
        }
        ActionsAction::Annotations(id) => other_job_allowed(route, Some(*id)),
        ActionsAction::Compose(crate::actions_workspace::ActionsCommand::SearchLogs) => {
            workspace.view == ActionsView::Logs && loaded_capture_allowed(route, workspace)
        }
        _ => false,
    }
}
fn queue(
    app: &mut App,
    route: Arc<PrChecksRoute>,
    kind: WorkKindV2,
    watch: Option<Arc<PrCheckWatchV2>>,
) {
    let capture = matches!(kind, WorkKindV2::Search { .. })
        .then(|| route.loaded_capture_v2.clone())
        .flatten();
    let deadline = watch
        .as_ref()
        .map_or(Instant::now() + Duration::from_secs(30), |w| {
            (Instant::now() + Duration::from_secs(30)).min(w.quota.deadline())
        });
    let owner = Arc::new(PrCheckContinuationOwnerV2 {
        route,
        capture,
        watch,
        deadline,
    });
    app.pr_check_continuation_owner_v2 = Some(owner.clone());
    app.pr_check_continuation_request_v2 = Some(PrCheckContinuationWorkV2 { owner, kind });
    app.actions_action = None;
    app.actions_load_requested = false;
    app.refresh_requested = false;
    app.hits.clear();
}
/// Intercept BEFORE legacy Compose/draft handling and BEFORE generic apply.
pub fn intercept(app: &mut App, action: &ActionsAction) -> bool {
    if app.scoped_actions_v1.is_some() {
        return false;
    }
    let Some(route) = app.pr_checks_route.clone().filter(|r| r.scoped_v1()) else {
        return false;
    };
    if !matches!(
        action,
        ActionsAction::Refresh
            | ActionsAction::Watch
            | ActionsAction::Annotations(_)
            | ActionsAction::Compose(crate::actions_workspace::ActionsCommand::SearchLogs)
    ) {
        return false;
    }
    if !owns_origin(app, &route.origin) {
        corrective_v3::hide_current(app);
        return true;
    }
    if *action == ActionsAction::Watch && app.pr_check_watch_v2.is_some() {
        stop(app);
        app.status = "Finite scoped watch stopped • no refresh, draft, ACK or publication".into();
        return true;
    }
    let Some(workspace) = app.actions.as_ref() else {
        return true;
    };
    if !admitted_action(&route, workspace, action) || pending(app) {
        app.dispatch(Action::Explain(
            "Original scoped continuation unavailable or busy; no default scope/draft read".into(),
        ));
        return true;
    }
    if matches!(action, ActionsAction::Compose(_)) {
        ui::open(app, route);
        return true;
    }
    let job = workspace.run.as_ref().and_then(|p| p.selected_job);
    if *action == ActionsAction::Watch {
        let mode = if workspace.view == ActionsView::Logs {
            PrCheckWatchModeV2::SelectedJobLogs
        } else {
            PrCheckWatchModeV2::Status
        };
        if mode == PrCheckWatchModeV2::SelectedJobLogs && job.is_none() {
            return true;
        }
        app.pr_check_watch_v2 = Some(Arc::new(PrCheckWatchV2 {
            origin: route.origin.clone(),
            observed: route.observation.clone(),
            job,
            mode,
            quota: quota::PrCheckWatchQuotaV2::new(),
        }));
        app.status =
            "Finite scoped watch armed • 12 fresh ticks / 120s / 32MiB • no overlap".into();
        return true;
    }
    let log_view = workspace.view == ActionsView::Logs;
    stop(app);
    let wanted_job = if let ActionsAction::Annotations(id) = action {
        Some(*id)
    } else {
        job
    };
    let Ok(choice) = choice(&route, wanted_job) else {
        return true;
    };
    let kind = if matches!(action, ActionsAction::Annotations(_)) {
        WorkKindV2::Annotations { choice }
    } else if log_view {
        WorkKindV2::Logs {
            choice,
            matched: route
                .observation
                .link()
                .is_some_and(|link| Some(link.selected_job()) == job),
        }
    } else {
        WorkKindV2::Refresh { choice }
    };
    queue(app, route, kind, None);
    true
}
pub fn queue_search(
    app: &mut App,
    route: Arc<PrChecksRoute>,
    query: String,
    mode: LoadedLogSearchModeV2,
) {
    let Some(capture) = route.loaded_capture_v2.clone() else {
        return;
    };
    if !owns_origin(app, &route.origin)
        || app
            .pr_checks_route
            .as_ref()
            .is_none_or(|r| !Arc::ptr_eq(r, &route))
    {
        return;
    }
    // Explicit search supersedes an in-flight watch before either can publish.
    stop(app);
    queue(
        app,
        route,
        WorkKindV2::Search {
            capture,
            query,
            mode,
        },
        None,
    );
}
pub fn stop(app: &mut App) {
    if let Some(watch) = app.pr_check_watch_v2.take() {
        watch.quota.stop();
    }
    app.pr_check_continuation_request_v2 = None;
    app.pr_check_continuation_owner_v2 = None;
    app.pr_check_continuation_cancel_v2 = true;
}
pub fn retire(app: &mut App) {
    stop(app);
    app.pr_check_log_matches_v2 = None;
    app.pr_check_log_editor_v2 = None;
    app.pr_check_annotations_detail_v2 = None;
    app.pr_check_attempt_advance_v2 = None;
}
/// One scheduler; legacy workspace.watching remains false. No capture history.
pub fn tick(app: &mut App) {
    if app.scoped_actions_v1.is_some() {
        return;
    }
    let Some(watch) = app.pr_check_watch_v2.clone() else {
        return;
    };
    let Some(route) = app.pr_checks_route.clone() else {
        stop(app);
        return;
    };
    if !watch.active() {
        let owned = watch.matches(&route) && owns_origin(app, &route.origin);
        stop(app);
        if owned {
            app.status="Finite scoped watch exhausted or expired • rearm explicitly; prior snapshot retained".into();
        }
        return;
    }
    if !watch.matches(&route) || !owns_origin(app, &route.origin) {
        stop(app);
        return;
    }
    if app.overlay.is_some()
        || app.view != View::Actions
        || pending(app)
        || app.pr_check_continuation_request_v2.is_some()
        || !watch.quota.due()
    {
        return;
    }
    let Ok(choice) = choice(&route, watch.job) else {
        stop(app);
        return;
    };
    let kind = match watch.mode {
        PrCheckWatchModeV2::Status => WorkKindV2::Refresh { choice },
        PrCheckWatchModeV2::SelectedJobLogs => {
            if app.actions.is_none() {
                return;
            }
            let matched = route
                .observation
                .link()
                .is_some_and(|l| Some(l.selected_job()) == watch.job);
            WorkKindV2::Logs { choice, matched }
        }
    };
    queue(app, route, kind, Some(watch));
}
pub fn take_work(app: &mut App) -> Option<PrCheckContinuationWorkV2> {
    corrective_v3::take_work(app)
}
fn prepared(owner: Arc<PrCheckContinuationOwnerV2>, outcome: OutcomeV2) -> PreparedView {
    PreparedView {
        pr_check_continuation_v2: Some(Box::new(PrCheckContinuationCompletionV2 {
            owner,
            outcome,
            watch_progress: None,
        })),
        ..Default::default()
    }
}
pub fn install(app: &mut App, value: PrCheckContinuationCompletionV2) -> Result<(), String> {
    if !value.owner.owns(app) {
        return Err("Original continuation superseded; no effects".into());
    }
    match value.outcome {
        OutcomeV2::Hidden => {
            corrective_v3::hide_current(app);
        }
        OutcomeV2::Remote(value) => {
            let view = value.actions.as_ref().map(|workspace| workspace.view);
            app.pr_check_log_matches_v2 = None;
            app.pr_check_annotations_detail_v2 = None;
            install_v1(app, *value)?;
            app.pr_check_continuation_owner_v2 = None;
            app.status = match view {
                Some(ActionsView::Annotations) => {
                    "Original scoped annotations loaded • reported locations only"
                }
                Some(ActionsView::Logs) => {
                    "Original scoped selected-job logs refreshed • original attempt retained"
                }
                _ => "Original scoped status refreshed • original attempt retained",
            }
            .into();
            if let Some(route) = app.pr_checks_route.as_ref() {
                if let Some(workspace) = app.actions.as_mut() {
                    super::super::preserve_controls_v1(route, workspace);
                }
            }
        }
        OutcomeV2::Search(matches) => {
            ui::clear_matches(app);
            if let Some(workspace) = app.actions.as_mut() {
                for &index in matches.indices() {
                    if let Some(row) = workspace.rows.get_mut(index) {
                        row.role = crate::theme::SemanticToken::Reference;
                    }
                }
            }
            app.status = format!(
                "{} matches in captured job {} attempt {} • loaded snapshot only",
                matches.indices().len(),
                matches.capture().captured_target_v2().job_id(),
                matches.capture().captured_target_v2().attempt()
            );
            app.pr_check_log_matches_v2 = Some(matches);
            app.pr_check_continuation_owner_v2 = None;
            ui::next_match(app, false);
        }
        OutcomeV2::SearchUnavailable => {
            app.pr_check_continuation_owner_v2 = None;
            app.status =
                "Loaded-log query unavailable • original captured snapshot retained".into();
            app.dispatch(Action::Explain(
                "Regex/query preparation unavailable; original captured logs unchanged".into(),
            ));
        }
        OutcomeV2::Advanced(advanced) => {
            let original_attempt = advanced
                .original()
                .selected_run_v1()
                .map(|run| run.attempt().run.attempt);
            let latest_attempt = advanced.latest().attempt;
            let latest_suite = advanced.latest_suite();
            stop(app);
            app.pr_check_attempt_advance_v2 = Some(advanced);
            app.status = "Original attempt retained • newer attempt observed; Watch stopped".into();
            app.dispatch(Action::Explain(format!("Original observed attempt is historical; latest attempt advanced. Original {original_attempt:?} / actual latest {latest_attempt} / suite {latest_suite}. Watch stopped; no latest attempt adopted or queried")));
        }
    }
    if let Some((ticks, bytes)) = value
        .watch_progress
        .filter(|_| app.pr_check_watch_v2.as_ref().is_some_and(|w| w.active()))
    {
        app.status=format!("Finite scoped watch • tick {ticks}/12 • {bytes}/33554432 charged bytes • original attempt retained");
    }
    Ok(())
}
pub fn execute(
    work: PrCheckContinuationWorkV2,
    provider: &GithubProvider,
    database: &Path,
    limits: &ScopedPrCheckReadLimitsV1,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    execute_after_preparation(work, provider, database, limits, cancel, || {})
}
fn execute_after_preparation(
    work: PrCheckContinuationWorkV2,
    provider: &GithubProvider,
    database: &Path,
    limits: &ScopedPrCheckReadLimitsV1,
    cancel: &CancellationToken,
    after_preparation: impl FnOnce(),
) -> Result<PreparedView, String> {
    let owner = work.owner;
    let kind = match work.kind {
        WorkKindV2::Search {
            capture,
            query,
            mode,
        } => {
            let outcome =
                match LoadedLogMatchesV2::prepare(capture, query, mode, owner.deadline, cancel) {
                    Ok(matches) => OutcomeV2::Search(matches),
                    Err(_) => OutcomeV2::SearchUnavailable,
                };
            after_preparation();
            original_window(&owner, cancel)
                .map_err(|_| "Original preparation expired".to_owned())?;
            return Ok(prepared(owner, outcome));
        }
        kind => kind,
    };
    let claim = owner.watch.as_ref().map(|w| w.quota.claim()).transpose()?;
    let result = remote(
        &owner,
        kind,
        provider,
        database,
        limits,
        cancel,
        after_preparation,
    );
    let mut progress = None;
    let outcome = match result {
        Ok((value, charged)) => {
            if let Some(claim) = claim {
                progress = Some(claim.finish(charged)?);
            }
            value
        }
        Err(_) => {
            drop(claim);
            OutcomeV2::Hidden
        }
    };
    let mut prepared = prepared(owner, outcome);
    prepared
        .pr_check_continuation_v2
        .as_mut()
        .unwrap()
        .watch_progress = progress;
    Ok(prepared)
}
fn remote(
    owner: &Arc<PrCheckContinuationOwnerV2>,
    kind: WorkKindV2,
    provider: &GithubProvider,
    database: &Path,
    limits: &ScopedPrCheckReadLimitsV1,
    cancel: &CancellationToken,
    after_preparation: impl FnOnce(),
) -> GithubResult<(OutcomeV2, usize)> {
    let route = &owner.route;
    let original = route
        .origin
        .scoped
        .as_ref()
        .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?;
    if Instant::now() >= owner.deadline || cancel.is_cancelled() {
        return Err(safe_failure(GithubErrorKind::Transport));
    }
    let mut access = limits.access.clone();
    if owner.watch.is_some() {
        // The finite ledger must cover WHOLE configured captures before any
        // dispatch, including source failures. Do not wait for an oversized
        // success to discover that callers supplied a larger per-tick budget.
        access.transport.max_document_bytes = access.transport.max_document_bytes.min(128 * 1024);
        access.transport.max_response_bytes = access
            .transport
            .max_response_bytes
            .min(quota::WATCH_GROUP_BYTES_V2);
        access.max_repository_reads = access.max_repository_reads.min(2);
        access.max_references = access.max_references.min(1);
    }
    access.transport.max_duration = access
        .transport
        .max_duration
        .min(owner.deadline.saturating_duration_since(Instant::now()));
    let mut session =
        provider.begin_repository_catalogue_access(original.source.source(), &access, cancel)?;
    if Scope::github(&session.identity().host, session.identity().account_id)
        .ok()
        .as_ref()
        != Some(original.source.scope())
    {
        return Err(safe_failure(GithubErrorKind::IdentityChanged));
    }
    let identity = session.identity().clone();
    let mut bridge = limits.bridge.clone();
    bridge.max_duration = bridge.max_duration.min(session.remaining(cancel)?);
    let mut logs = limits.logs.clone();
    logs.max_duration = logs.max_duration.min(session.remaining(cancel)?);
    let (children, child_cap, pool) = match &kind {
        WorkKindV2::Annotations { .. } => (
            12 + bridge.max_job_pages + 10,
            bridge.max_document_bytes,
            bridge.max_response_bytes,
        ),
        WorkKindV2::Refresh { .. } => (
            11 + bridge.max_job_pages,
            bridge.max_document_bytes,
            bridge.max_response_bytes,
        ),
        WorkKindV2::Logs { matched, .. } => (
            if *matched {
                9
            } else {
                11 + bridge.max_job_pages
            },
            logs.max_document_bytes.max(logs.max_log_bytes),
            logs.max_response_bytes,
        ),
        WorkKindV2::Search { .. } => return Err(safe_failure(GithubErrorKind::InvalidTarget)),
    };
    let window = session.reserve_measured_external_reads(children, child_cap, pool, 2, cancel)?;
    let bounded = provider.clone().with_measured_read_window(window)?;
    let current = session.verify(original.navigation.original(), cancel)?;
    let mut next_observation = route.observation.clone();
    let mut next_capture = route.loaded_capture_v2.clone();
    let mut annotations = None;
    let mut advance = None;
    let mut notice = None;
    let mut workspace = match kind {
        WorkKindV2::Annotations { choice } => {
            let mut workspace = actual_workspace(&route.observation)?;
            let selected = PrCheckAnnotationLimitsV2 {
                transport: bridge,
                ..Default::default()
            };
            let read = Arc::new(bounded.pr_check_annotations_from_resource_with_guard_v2(
                &identity,
                current,
                original.resource.observed(),
                original.check_index,
                route.observation.clone(),
                &choice,
                &selected,
                cancel,
                |_| validate_original(route, database),
            )?);
            if let Some(target) = read.target() {
                workspace
                    .run
                    .as_mut()
                    .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?
                    .selected_job = Some(target.job_id());
            }
            workspace.view = ActionsView::Annotations;
            annotations = Some(read);
            workspace
        }
        WorkKindV2::Refresh { choice } => {
            match bounded.pr_check_refresh_from_resource_with_guard_v2(
                &identity,
                current,
                original.resource.observed(),
                original.check_index,
                &route.observation,
                &choice,
                &bridge,
                cancel,
                |_| validate_original(route, database),
            )? {
                PrCheckRefreshReadV2::AttemptAdvanced(observed) => {
                    advance = Some(Arc::new(*observed));
                    // Nothing copied from the old private model; no fabricated
                    // empty current run. The completion retains original App.
                    ActionsWorkspace::new(identity.clone(), vec![])
                }
                PrCheckRefreshReadV2::Current(observed) => {
                    next_observation = Arc::new(*observed);
                    let mut workspace =
                        PreparedPrCheckActions::new_selected_v1(next_observation.clone())?
                            .into_workspace();
                    if let Some(capture) = next_capture.as_ref() {
                        if workspace.run.as_ref().is_some_and(|p| {
                            capture
                                .captured_target_v2()
                                .matches_run(&p.attempt.run, p.selected_job)
                        }) {
                            workspace.run.as_mut().unwrap().logs = Some(capture.logs().into());
                            workspace.warnings.push(format!("Previous loaded log snapshot • read as-of {} • status only refreshed", capture.observed_through()));
                        } else {
                            next_capture = None;
                        }
                    }
                    notice = next_observation.choice_unavailable_v1().map(|r| r.label());
                    workspace
                }
            }
        }
        WorkKindV2::Logs { choice, matched } => {
            let mut workspace = actual_workspace(&route.observation)?;
            let capture = if matched {
                Some(bounded.pr_check_logs_from_resource_with_guard_v1(
                    route.observation.clone(),
                    current,
                    original.resource.observed(),
                    original.check_index,
                    &logs,
                    cancel,
                    |_| validate_original(route, database),
                )?)
            } else {
                let observed = bounded.pr_check_job_logs_from_resource_with_guard_v1(
                    &identity,
                    current,
                    original.resource.observed(),
                    original.check_index,
                    route.observation.clone(),
                    &choice,
                    &logs,
                    &bridge,
                    cancel,
                    |_| validate_original(route, database),
                )?;
                notice = observed.unavailable().map(|r| r.label());
                observed.into_read_v2()
            };
            if let Some(capture) = capture {
                let capture = Arc::new(capture);
                let page = workspace
                    .run
                    .as_mut()
                    .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?;
                page.logs = Some(capture.logs().into());
                page.selected_job = Some(capture.captured_target_v2().job_id());
                workspace.view = ActionsView::Logs;
                next_capture = Some(capture);
            } else {
                next_capture = None;
                workspace
                    .run
                    .as_mut()
                    .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?
                    .logs = None;
                workspace.view = ActionsView::Jobs;
            }
            workspace
        }
        WorkKindV2::Search { .. } => return Err(safe_failure(GithubErrorKind::InvalidTarget)),
    };
    let current = session.verify(original.navigation.original(), cancel)?;
    if current.repository().full_name != next_observation.repository().full_name
        || current.repository().permissions.pull != Some(true)
    {
        return Err(safe_failure(GithubErrorKind::IdentityChanged));
    }
    // Genuine closing repository was read above. ALL original SQL is here,
    // then final version/user; absolutely no private SQL after sealed close.
    let closed = session.close_with_guard(cancel, || validate_original(route, database))?;
    if cancel.is_cancelled() || Instant::now() >= owner.deadline {
        return Err(safe_failure(GithubErrorKind::Transport));
    }
    let charged = closed.charged_bytes();
    if let Some(advanced) = advance {
        after_preparation();
        original_window(owner, cancel)?;
        return Ok((OutcomeV2::Advanced(advanced), charged));
    }
    let installed = rebound_origin_v1(
        &route.origin,
        closed.identity().clone(),
        &next_observation.repository().full_name,
    )
    .map_err(safe_sql)?;
    workspace.identity = closed.identity().clone();
    workspace.log_search = None; // No legacy regex compilation on selection/draw.
    if annotations.is_none() {
        workspace.prepare_rows();
    }
    decorate_scoped_v1(&mut workspace, &route.guard, &next_observation, original)
        .map_err(safe_sql)?;
    // Existing durable ledger decorator rebuilds generic rows. Attach genuine
    // prepared nullable annotations AFTER it, still in the worker/no SQL.
    if let Some(read) = annotations.as_ref() {
        prepare_annotations(&mut workspace, read);
    }
    let rows = prepare_rows(&next_observation);
    let next = Arc::new(PrChecksRoute {
        origin: installed.clone(),
        observation: next_observation,
        guard: route.guard.clone(),
        scoped_local: route.scoped_local.clone(),
        choice_rows: rows,
        loaded_capture_v2: next_capture,
        annotations_v2: annotations,
    });
    super::super::preserve_controls_v1(&next, &mut workspace);
    after_preparation();
    // Closing proof does not renew the original ticket. Pure bounded row and
    // control preparation is part of that SAME window; no SQL after this gate.
    original_window(owner, cancel)?;
    Ok((
        OutcomeV2::Remote(Box::new(ScopedPrCheckCompletionV1 {
            expected_origin: route.origin.clone(),
            origin: installed,
            page: None,
            actions: Some(workspace),
            route: Some(next),
            identity: closed.identity().clone(),
            unavailable: notice,
        })),
        charged,
    ))
}
fn original_window(
    owner: &PrCheckContinuationOwnerV2,
    cancel: &CancellationToken,
) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= owner.deadline {
        return Err(safe_failure(GithubErrorKind::Transport));
    }
    Ok(())
}
#[cfg(test)]
pub(crate) fn execute_after_preparation_for_test(
    work: PrCheckContinuationWorkV2,
    provider: &GithubProvider,
    database: &Path,
    limits: &ScopedPrCheckReadLimitsV1,
    cancel: &CancellationToken,
    after_preparation: impl FnOnce(),
) -> Result<PreparedView, String> {
    execute_after_preparation(work, provider, database, limits, cancel, after_preparation)
}
fn actual_workspace(observed: &Arc<PrCheckRunObservation>) -> GithubResult<ActionsWorkspace> {
    // AutoUnique is admitted ONLY by the existing closed matched-run factory;
    // explicit/other members still use the existing selected-read factory.
    let prepared = if observed.link().is_some() {
        PreparedPrCheckActions::new(observed.clone())?
    } else {
        PreparedPrCheckActions::new_selected_v1(observed.clone())?
    };
    Ok(prepared.into_workspace())
}
fn prepare_annotations(workspace: &mut ActionsWorkspace, read: &Arc<PrCheckAnnotationsReadV2>) {
    use crate::{actions_workspace::ActionsRow, theme::SemanticToken};
    workspace.view = ActionsView::Annotations;
    workspace.selected = 0;
    workspace.top = 0;
    workspace.prepare_rows();
    workspace.rows = vec![
        ActionsRow {
            label: "API-visible annotations • not a canonical file range".into(),
            role: SemanticToken::Muted,
            actor: None,
            action: None,
        },
        ActionsRow {
            label: format!(
                "{} observed • {} • job {}",
                read.rows().len(),
                if read.coverage().complete {
                    "observed source complete"
                } else {
                    "PARTIAL / unknown"
                },
                read.target()
                    .map_or_else(|| "unavailable".into(), |t| t.job_id().to_string())
            ),
            role: SemanticToken::Normal,
            actor: None,
            action: None,
        },
    ];
    for (index, row) in read.rows().iter().enumerate() {
        let location = row.lines().map_or_else(
            || "reported lines unavailable".into(),
            |(a, b)| format!("reported lines {a}–{b}"),
        );
        let label = crate::providers::runner::sanitize_terminal(&format!(
            "{} • {} • {} • {}",
            row.level().unwrap_or("level unknown"),
            row.path().unwrap_or("path unavailable"),
            location,
            row.title().unwrap_or("title unavailable")
        ));
        workspace.rows.push(ActionsRow {
            label,
            role: match row.level() {
                Some("failure") => SemanticToken::Failure,
                Some("warning") => SemanticToken::Changed,
                _ => SemanticToken::Normal,
            },
            actor: None,
            action: Some(ActionsAction::Select(index + ANNOTATION_ROW_BASE_V2)),
        });
    }
    workspace
        .warnings
        .extend(read.coverage().reasons.iter().cloned());
}
pub(super) const ANNOTATION_ROW_BASE_V2: usize = 2;
