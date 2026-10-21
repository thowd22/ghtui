//! SOURCE-ONLY child of scoped PR checks. Pure owned selector, same runtime
//! publisher and fresh original-session selected/member reads. No mutations.
use super::*;
use crate::providers::gh::{GithubResult, PrCheckChoiceV1, PrCheckSelectedReadPhaseV1};
#[path = "scoped_pr_check_continuations_v2.rs"]
pub mod continuations_v2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrCheckChoiceUiV1 {
    Open,
    Select(usize),
    Confirm,
    Scroll(i32),
    Close,
    Quit,
}
#[derive(Clone, Copy)]
enum Item {
    Run(usize),
    Job(usize),
}
struct Row {
    label: String,
    item: Item,
    actor: Option<crate::identity::ActorIdentity>,
}
pub(crate) struct PreparedPrCheckChoiceRowsV1 {
    rows: Vec<Row>,
    header: String,
}
impl PreparedPrCheckChoiceRowsV1 {
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        use crate::providers::gh::workflow_capacity_v9 as b;
        let mut n = b::sum([
            b::owner::<Self>(),
            b::vector(&self.rows)?,
            self.header.capacity(),
        ])?;
        for row in &self.rows {
            n = n.checked_add(row.label.capacity())?;
            if let Some(actor) = &row.actor {
                n = n.checked_add(b::actor_identity(actor)?)?;
            }
        }
        Some(n)
    }
}
pub(super) fn prepare_rows(
    observation: &Arc<PrCheckRunObservation>,
) -> Option<Arc<PreparedPrCheckChoiceRowsV1>> {
    let mut rows = vec![];
    let mut bytes = 0usize;
    for (i, candidate) in observation.run_candidates_v1().iter().enumerate() {
        let run = candidate.run();
        let label = crate::providers::runner::sanitize_terminal(&format!(
            "Run {} • observed attempt {} • {} • {} • {}",
            run.id,
            run.attempt,
            run.event,
            run.name,
            run.actor.label()
        ));
        let actor =
            crate::identity::ActorIdentity::github(&observation.identity().host, &run.actor);
        let actor_bytes = [
            actor.host.as_deref(),
            actor.login.as_deref(),
            Some(actor.name.as_str()),
            actor.email.as_deref(),
            actor.avatar_url.as_deref(),
        ]
        .into_iter()
        .flatten()
        .try_fold(0usize, |n, s| n.checked_add(s.len()))?;
        bytes = bytes
            .checked_add(label.len())
            .and_then(|n| n.checked_add(actor_bytes))
            .and_then(|n| n.checked_add(std::mem::size_of::<Row>()))
            .filter(|n| *n <= 2 * 1024 * 1024)?;
        rows.push(Row {
            label,
            item: Item::Run(i),
            actor: Some(actor),
        });
    }
    if let Some(run) = observation.selected_run_v1() {
        let jobs: std::collections::BTreeMap<_, _> =
            run.attempt().jobs.iter().map(|job| (job.id, job)).collect();
        for (i, member) in run.members().iter().enumerate() {
            let job = jobs.get(&member.id())?;
            let label = crate::providers::runner::sanitize_terminal(&format!(
                "Job {} • {} • attempt {} • {}",
                job.id,
                job.name,
                run.attempt().run.attempt,
                if member.maps_original_check() {
                    "maps selected CheckRun"
                } else {
                    "other exact-attempt member"
                }
            ));
            bytes = bytes
                .checked_add(label.len())
                .and_then(|n| n.checked_add(std::mem::size_of::<Row>()))
                .filter(|n| *n <= 2 * 1024 * 1024)?;
            rows.push(Row {
                label,
                item: Item::Job(i),
                actor: None,
            });
        }
    }
    if rows.is_empty() || rows.len() > 2000 {
        return None;
    }
    let coverage = observation.run_coverage();
    let reported = coverage
        .total_matches
        .map_or_else(|| "unknown".into(), |n| n.to_string());
    let header = format!("SEEN {} run candidates • reported {reported} • {}. Select a positive observed row; no unseen/unique/mutation authority.", observation.run_candidates_v1().len(), if coverage.enumeration_complete { "API list exhausted" } else { "PARTIAL" });
    Some(Arc::new(PreparedPrCheckChoiceRowsV1 { rows, header }))
}
pub struct PrCheckChoicePickerV1 {
    route: Arc<PrChecksRoute>,
    prepared: Arc<PreparedPrCheckChoiceRowsV1>,
    selected: usize,
    top: usize,
    focus: usize,
    pub(crate) hits: Vec<(ratatui::layout::Rect, PrCheckChoiceUiV1)>,
}
impl PrCheckChoicePickerV1 {
    fn owns(&self, app: &App) -> bool {
        owns_origin(app, &self.route.origin)
            && app
                .pr_checks_route
                .as_ref()
                .is_some_and(|route| Arc::ptr_eq(route, &self.route))
    }
}
pub struct PrCheckChoiceWorkV1 {
    kind: WorkKind,
}
enum WorkKind {
    Selected {
        route: Arc<PrChecksRoute>,
        choice: PrCheckChoiceV1,
    },
    OtherLogs {
        route: Arc<PrChecksRoute>,
        choice: PrCheckChoiceV1,
        workspace: Box<ActionsWorkspace>,
    },
}
impl PrCheckChoiceWorkV1 {
    pub fn origin(&self) -> &Arc<PrChecksOrigin> {
        match &self.kind {
            WorkKind::Selected { route, .. } | WorkKind::OtherLogs { route, .. } => &route.origin,
        }
    }
    pub fn view(&self) -> View {
        match &self.kind {
            WorkKind::Selected { .. } => View::Inbox,
            WorkKind::OtherLogs { .. } => View::Actions,
        }
    }
}
pub fn has_candidates(app: &App) -> bool {
    app.pr_checks_route.as_ref().is_some_and(|route| {
        route.origin.scoped_v1()
            && owns_origin(app, &route.origin)
            && route
                .choice_rows
                .as_ref()
                .is_some_and(|rows| !rows.rows.is_empty())
    })
}
pub fn pending(app: &App) -> bool {
    app.pr_checks_route
        .as_ref()
        .is_some_and(|route| route.scoped_v1())
        && (app.loading[View::Inbox.index()] || app.loading[View::Actions.index()])
}
pub fn retire(app: &mut App) {
    app.pr_check_choice_picker = None;
    app.pr_check_choice_request = None;
    if app.overlay == Some(crate::app::Overlay::PrCheckChoices) {
        app.overlay = None;
    }
}
pub fn dispatch(app: &mut App, action: PrCheckChoiceUiV1) {
    use PrCheckChoiceUiV1::*;
    if action == Quit {
        app.dispatch(Action::Quit);
        return;
    }
    if action == Open {
        if !has_candidates(app) || pending(app) {
            app.dispatch(Action::Explain(
                "No closed observed run/job candidates in the original scope; nothing queried"
                    .into(),
            ));
            return;
        }
        let route = app.pr_checks_route.clone().unwrap();
        let prepared = route.choice_rows.as_ref().unwrap().clone();
        app.pr_check_choice_picker = Some(PrCheckChoicePickerV1 {
            route,
            prepared,
            selected: 0,
            top: 0,
            focus: 0,
            hits: vec![],
        });
        app.overlay = Some(crate::app::Overlay::PrCheckChoices);
        app.hits.clear();
        return;
    }
    let Some(mut picker) = app.pr_check_choice_picker.take() else {
        return;
    };
    if !picker.owns(app) {
        retire(app);
        return;
    }
    match action {
        Close => {
            app.overlay = None;
            app.hits.clear();
            return;
        }
        Select(index) => {
            if index < picker.prepared.rows.len() {
                picker.selected = index;
                picker.focus = 0;
            }
        }
        Scroll(delta) => {
            picker.selected = picker
                .selected
                .saturating_add_signed(delta as isize)
                .min(picker.prepared.rows.len().saturating_sub(1));
        }
        Confirm => {
            let Some(row) = picker.prepared.rows.get(picker.selected) else {
                return;
            };
            let choice = match row.item {
                Item::Run(i) => picker.route.observation.choose_run_v1(i),
                Item::Job(i) => picker.route.observation.choose_job_v1(i),
            };
            let Ok(choice) = choice else {
                app.dispatch(Action::Explain(
                    "Actual original candidate unavailable; no raw index fallback".into(),
                ));
                return;
            };
            let Some(page) = app.github.as_ref().and_then(|github| github.pr.as_ref()) else {
                retire(app);
                return;
            };
            if !matches_page(
                &picker.route.origin.scoped.as_ref().unwrap().navigation,
                page,
            ) {
                retire(app);
                return;
            }
            // Keep the original Checks page on App for pure immediate Back.
            // Only tiny opaque observed navigation goes to the worker; neither
            // the page/body nor the cached Inbox is cloned into its request.
            app.pr_check_choice_request = Some(PrCheckChoiceWorkV1 {
                kind: WorkKind::Selected {
                    route: picker.route,
                    choice,
                },
            });
            app.overlay = None;
            app.hits.clear();
            app.status =
                "Preparing positively chosen run/member • original source guard; no mutation"
                    .into();
            return;
        }
        Open | Quit => {}
    }
    picker.hits.clear();
    app.pr_check_choice_picker = Some(picker);
}
pub fn other_job_allowed(route: &PrChecksRoute, job: Option<u64>) -> bool {
    job.is_some_and(|id| {
        route
            .observation
            .selected_run_v1()
            .is_some_and(|run| run.members().iter().any(|member| member.id() == id))
    })
}
pub fn queue_other_logs(app: &mut App) -> bool {
    let Some(route) = app
        .pr_checks_route
        .clone()
        .filter(|route| route.scoped_v1())
    else {
        return false;
    };
    let Some(ActionsAction::Logs(job)) = app.actions_action.as_ref() else {
        return false;
    };
    if route
        .observation
        .link()
        .is_some_and(|link| *job == Some(link.selected_job()))
    {
        return false;
    }
    if !owns_origin(app, &route.origin) || !other_job_allowed(&route, *job) {
        app.actions_action = None;
        app.dispatch(Action::Explain(
            "Other job is not an original observed exact-attempt member; no default log read"
                .into(),
        ));
        return true;
    }
    let index = route
        .observation
        .selected_run_v1()
        .unwrap()
        .members()
        .iter()
        .position(|member| Some(member.id()) == *job)
        .unwrap();
    let Ok(choice) = route.observation.choose_job_v1(index) else {
        return true;
    };
    let Some(workspace) = app.actions.take() else {
        app.actions_action = None;
        return true;
    };
    app.actions_action = None;
    app.actions_load_requested = false;
    app.refresh_requested = false;
    app.hits.clear();
    app.pr_check_choice_request = Some(PrCheckChoiceWorkV1 {
        kind: WorkKind::OtherLogs {
            route,
            choice,
            workspace: Box::new(workspace),
        },
    });
    true
}
pub fn take_work(app: &mut App) -> Option<PrCheckChoiceWorkV1> {
    let work = app.pr_check_choice_request.take()?;
    let route = match &work.kind {
        WorkKind::Selected { route, .. } | WorkKind::OtherLogs { route, .. } => route,
    };
    let choice = match &work.kind {
        WorkKind::Selected { choice, .. } | WorkKind::OtherLogs { choice, .. } => choice,
    };
    if !owns_origin(app, &route.origin) || !choice.belongs_to(&route.observation) {
        crate::repository_catalogue_native::privacy(app);
        return None;
    }
    Some(work)
}
pub fn execute(
    work: PrCheckChoiceWorkV1,
    provider: &GithubProvider,
    database: &Path,
    limits: &ScopedPrCheckReadLimitsV1,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    match work.kind {
        WorkKind::Selected { route, choice } => {
            selected(route, choice, provider, database, limits, cancel)
        }
        WorkKind::OtherLogs {
            route,
            choice,
            workspace,
        } => other_logs(
            route, choice, *workspace, provider, database, limits, cancel,
        ),
    }
}
fn validate_original(route: &PrChecksRoute, database: &Path) -> GithubResult<()> {
    let owner = route
        .origin
        .scoped
        .as_ref()
        .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?;
    route
        .scoped_local
        .as_ref()
        .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
        .validate(database, &owner.source)?;
    route.guard.validate(database).map_err(safe_sql)
}
fn selected(
    route: Arc<PrChecksRoute>,
    choice: PrCheckChoiceV1,
    provider: &GithubProvider,
    database: &Path,
    limits: &ScopedPrCheckReadLimitsV1,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let expected = route.origin.clone();
    let read: GithubResult<ScopedPrCheckCompletionV1> = (|| {
        let owner = expected
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?;
        if !choice.belongs_to(&route.observation) {
            return Err(safe_failure(GithubErrorKind::InvalidTarget));
        }
        let mut session = provider.begin_repository_catalogue_access(
            owner.source.source(),
            &limits.access,
            cancel,
        )?;
        if Scope::github(&session.identity().host, session.identity().account_id)
            .ok()
            .as_ref()
            != Some(owner.source.scope())
        {
            return Err(safe_failure(GithubErrorKind::IdentityChanged));
        }
        let mut selected = limits.bridge.clone();
        selected.max_duration = selected.max_duration.min(session.remaining(cancel)?);
        let window = session.reserve_measured_external_reads(
            11 + selected.max_job_pages,
            selected.max_document_bytes,
            selected.max_response_bytes,
            2,
            cancel,
        )?;
        let bounded = provider.clone().with_measured_read_window(window)?;
        let identity = session.identity().clone();
        let current = session.verify(owner.navigation.original(), cancel)?;
        let observation = Arc::new(bounded.pr_check_selected_from_resource_with_guard_v1(
            &identity,
            current,
            owner.resource.observed(),
            owner.check_index,
            &route.observation,
            &choice,
            &selected,
            cancel,
            |phase| match phase {
                PrCheckSelectedReadPhaseV1::BeforeSelectedProjection
                | PrCheckSelectedReadPhaseV1::BeforeFinalProof => {
                    validate_original(&route, database)
                }
            },
        )?);
        let current = session.verify(owner.navigation.original(), cancel)?;
        if current.repository().full_name != observation.repository().full_name
            || current.repository().permissions.pull != Some(true)
        {
            return Err(safe_failure(GithubErrorKind::IdentityChanged));
        }
        let deadline = session.deadline();
        let closed = session.close_with_guard(cancel, || validate_original(&route, database))?;
        if !observation.matches_resource_v1(
            closed.identity(),
            owner.resource.observed(),
            owner.check_index,
        ) {
            return Err(safe_failure(GithubErrorKind::IdentityChanged));
        }
        let installed = rebound_origin_v1(
            &expected,
            closed.identity().clone(),
            &observation.repository().full_name,
        )
        .map_err(safe_sql)?;
        let unavailable = observation
            .choice_unavailable_v1()
            .map(|reason| reason.label())
            .or_else(|| observation.unavailable().map(|reason| reason.label()));
        let actions = if observation.selected_run_v1().is_some() {
            let mut actions =
                PreparedPrCheckActions::new_selected_v1(observation.clone())?.into_workspace();
            actions.identity = closed.identity().clone();
            decorate_scoped_v1(&mut actions, &route.guard, &observation, owner)
                .map_err(safe_sql)?;
            Some(actions)
        } else {
            None
        };
        let choice_rows = prepare_rows(&observation);
        let next = Arc::new(PrChecksRoute {
            origin: installed.clone(),
            observation,
            guard: route.guard.clone(),
            scoped_local: route.scoped_local.clone(),
            choice_rows,
            loaded_capture_v2: None,
            annotations_v2: None,
        });
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err(safe_failure(GithubErrorKind::Transport));
        }
        Ok(ScopedPrCheckCompletionV1 {
            expected_origin: expected.clone(),
            origin: installed,
            page: None,
            actions,
            route: Some(next),
            identity: closed.identity().clone(),
            unavailable,
        })
    })();
    match read {
        Ok(value) => Ok(PreparedView {
            scoped_pr_checks: Some(Box::new(value)),
            ..Default::default()
        }),
        Err(_) => Ok(hidden(expected)),
    }
}
fn rebound_origin_v1(
    origin: &Arc<PrChecksOrigin>,
    identity: HostIdentity,
    canonical: &str,
) -> Result<Arc<PrChecksOrigin>, String> {
    // Initial and positively selected reads share the same tiny canonical
    // rebind. Neither prepares or clones the retained foreground PR page.
    copied_origin_with_current_v1(origin, identity, canonical)
}
fn other_logs(
    route: Arc<PrChecksRoute>,
    choice: PrCheckChoiceV1,
    mut workspace: ActionsWorkspace,
    provider: &GithubProvider,
    database: &Path,
    limits: &ScopedPrCheckReadLimitsV1,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let expected = route.origin.clone();
    let mut unavailable = None;
    let mut captured = None;
    let read: GithubResult<HostIdentity> = (|| {
        let owner = expected
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?;
        if !choice.belongs_to(&route.observation) {
            return Err(safe_failure(GithubErrorKind::InvalidTarget));
        }
        let mut session = provider.begin_repository_catalogue_access(
            owner.source.source(),
            &limits.access,
            cancel,
        )?;
        if Scope::github(&session.identity().host, session.identity().account_id)
            .ok()
            .as_ref()
            != Some(owner.source.scope())
        {
            return Err(safe_failure(GithubErrorKind::IdentityChanged));
        }
        let identity = session.identity().clone();
        let mut log_limits = limits.logs.clone();
        log_limits.max_duration = log_limits.max_duration.min(session.remaining(cancel)?);
        let window = session.reserve_measured_external_reads(
            11 + limits.bridge.max_job_pages,
            log_limits.max_document_bytes.max(log_limits.max_log_bytes),
            log_limits.max_response_bytes,
            2,
            cancel,
        )?;
        let bounded = provider.clone().with_measured_read_window(window)?;
        let current = session.verify(owner.navigation.original(), cancel)?;
        let observed = bounded.pr_check_job_logs_from_resource_with_guard_v1(
            &identity,
            current,
            owner.resource.observed(),
            owner.check_index,
            route.observation.clone(),
            &choice,
            &log_limits,
            &limits.bridge,
            cancel,
            |_| validate_original(&route, database),
        )?;
        unavailable = observed.unavailable().map(|reason| reason.label());
        let current = session.verify(owner.navigation.original(), cancel)?;
        if current.repository().full_name != route.observation.repository().full_name
            || current.repository().permissions.pull != Some(true)
        {
            return Err(safe_failure(GithubErrorKind::IdentityChanged));
        }
        let deadline = session.deadline();
        let closed = session.close_with_guard(cancel, || validate_original(&route, database))?;
        let page = workspace
            .run
            .as_mut()
            .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?;
        if let Some(log) = observed.into_read_v2() {
            page.logs = Some(log.logs().into());
            page.logs_skipped_bytes = 0;
            page.selected_job = Some(log.captured_target_v2().job_id());
            captured = Some(Arc::new(log));
            workspace.view = ActionsView::Logs;
        } else {
            page.logs = None;
            workspace.view = ActionsView::Jobs;
            workspace.warnings.push(
                unavailable
                    .unwrap_or("Current member unavailable; no empty log inferred")
                    .into(),
            );
        }
        workspace.selected = 0;
        workspace.top = 0;
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err(safe_failure(GithubErrorKind::Transport));
        }
        Ok(closed.identity().clone())
    })();
    match read {
        Ok(identity) => {
            workspace.identity = identity.clone();
            workspace.prepare_rows();
            decorate_scoped_v1(
                &mut workspace,
                &route.guard,
                &route.observation,
                expected
                    .scoped
                    .as_ref()
                    .ok_or("Original owner unavailable")?,
            )?;
            let next = Arc::new(PrChecksRoute {
                origin: route.origin.clone(),
                observation: route.observation.clone(),
                guard: route.guard.clone(),
                scoped_local: route.scoped_local.clone(),
                choice_rows: route.choice_rows.clone(),
                loaded_capture_v2: captured,
                annotations_v2: None,
            });
            super::preserve_controls_v1(&next, &mut workspace);
            Ok(PreparedView {
                scoped_pr_checks: Some(Box::new(ScopedPrCheckCompletionV1 {
                    expected_origin: expected.clone(),
                    origin: expected,
                    page: None,
                    actions: Some(workspace),
                    route: Some(next),
                    identity,
                    unavailable,
                })),
                ..Default::default()
            })
        }
        Err(_) => Ok(hidden(expected)),
    }
}
#[path = "scoped_pr_check_choices_ui_v1.rs"]
pub mod ui;
