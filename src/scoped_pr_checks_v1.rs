//! Source-only child of pr_checks_native. SAME run/log factories, publisher,
//! original catalogue source/session; no default Actions scope or write permit.
use super::*;
use crate::{
    active_remote_scope_v1::{ActiveRemoteScopeV1, RemoteScopeConsumerV1},
    github_workspace::{PrKey, PrPage},
    providers::gh::{
        ObservedPrCheckKindV1, PrCheckLogReadPhase, PrResourceKindV1,
        RepositoryCatalogueAccessLimits,
    },
    scoped_pr_observed_tabs_v1::{
        matches_page,
        resources::{ResourceLocalGuard, ScopedPrResourceViewV1},
        ScopedPrNavigationV1,
    },
    storage::{CatalogueIntentSource, Scope},
};

#[path = "scoped_pr_actions_origin_v2.rs"]
pub mod actions_origin_v2;
#[path = "scoped_pr_check_choices_v1.rs"]
pub mod choices;

/// Navigation ownership only. No serde, raw DTO constructor or current access.
pub(super) struct ScopedPrCheckOwnerV1 {
    source: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    navigation: Arc<ScopedPrNavigationV1>,
    resource: Arc<ScopedPrResourceViewV1>,
    cached_selected: PrKey,
    check_index: usize,
}
impl ScopedPrCheckOwnerV1 {
    fn matches_workspace(&self, github: &GithubWorkspace) -> bool {
        let nav = &self.navigation;
        let RemoteScopeConsumerV1::Explicit(rows) = self.active.consumer(&github.identity) else {
            return false;
        };
        github.scope().ok().as_ref() == Some(self.source.scope())
            && nav.scope() == self.source.scope()
            && github
                .scoped_pr_navigation
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, nav))
            && github
                .scoped_pr_resource
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &self.resource))
            && github.selected.as_ref() == Some(&self.cached_selected)
            && github
                .pr
                .as_ref()
                .is_some_and(|page| page.tab == PrTab::Checks && matches_page(nav, page))
            && rows.references().iter().any(|row| {
                row.original() == nav.original()
                    && row.observed().is_some_and(|observed| {
                        observed
                            .last_name
                            .eq_ignore_ascii_case(&self.cached_selected.repository)
                    })
            })
            && github.inbox.pull_requests.iter().any(|row| {
                github.filter.accepts(row)
                    && !github.exclusions.contains(&row.repository)
                    && PrKey::from(row) == self.cached_selected
            })
    }
    pub(super) fn owns(&self, app: &App) -> bool {
        app.remote_scope
            .as_ref()
            .is_some_and(|value| Arc::ptr_eq(value, &self.active))
            && app
                .remote_scope_source
                .as_ref()
                .is_some_and(|value| Arc::ptr_eq(value, &self.source))
            && app
                .github
                .as_ref()
                .is_some_and(|github| self.matches_workspace(github))
    }
}
impl PrChecksOrigin {
    pub(super) fn capture_scoped_v1(app: &App) -> Result<Self, String> {
        let github = app.github.as_ref().ok_or("No original observed PR")?;
        let source = app
            .remote_scope_source
            .clone()
            .ok_or("Original source unavailable")?;
        let active = app
            .remote_scope
            .clone()
            .ok_or("Original explicit scope unavailable")?;
        let navigation = github
            .scoped_pr_navigation
            .clone()
            .ok_or("Original PR navigation unavailable")?;
        let resource = github
            .scoped_pr_resource
            .clone()
            .ok_or("Original Checks resource unavailable")?;
        let Some(crate::scoped_pr_observed_tabs_v1::resources::PrResourceRowV1::Check(
            false,
            index,
        )) = resource.row(app.selected)
        else {
            return Err("Select an actual observed CheckRun. Commit status contexts do not identify an Actions run.".into());
        };
        let observed = resource.observed();
        let check = observed
            .checks()
            .get(index)
            .ok_or("Selected genuine CheckRun unavailable")?;
        if observed.kind() != PrResourceKindV1::Checks
            || check.kind != ObservedPrCheckKindV1::CheckRun
            || check.id == 0
            || check.head_sha != observed.parent().head_sha()
        {
            return Err("Exact original CheckRun/full head unavailable".into());
        }
        let summary = observed.parent().summary().clone();
        let check = Check {
            id: check.id,
            head_sha: check.head_sha.clone(),
            kind: "check".into(),
            name: String::new(),
            status: String::new(),
            conclusion: String::new(),
            url: String::new(),
        };
        let scoped = ScopedPrCheckOwnerV1 {
            source,
            active,
            navigation,
            resource,
            cached_selected: github
                .selected
                .clone()
                .ok_or("Original selected PR unavailable")?,
            check_index: index,
        };
        if !scoped.matches_workspace(github) || github.pr.is_none() {
            return Err("Exact original observed Checks navigation changed".into());
        }
        Ok(Self {
            identity: github.identity.clone(),
            summary,
            check,
            row: app.selected,
            top: app.scroll,
            scoped: Some(scoped),
        })
    }
    pub(super) fn matches_scoped_v1(&self, github: &GithubWorkspace) -> Option<bool> {
        self.scoped.as_ref().map(|owner| {
            same_principal(&self.identity, &github.identity) && owner.matches_workspace(github)
        })
    }
    pub fn scoped_v1(&self) -> bool {
        self.scoped.is_some()
    }
    pub fn scoped_owner_v1(
        &self,
    ) -> Option<(
        &Arc<CatalogueIntentSource>,
        &Arc<ActiveRemoteScopeV1>,
        &Arc<ScopedPrNavigationV1>,
    )> {
        self.scoped
            .as_ref()
            .map(|owner| (&owner.source, &owner.active, &owner.navigation))
    }
}
pub fn scoped_open_action(app: &App) -> Action {
    if app.view != View::Inbox
        || app.loading[View::Inbox.index()]
        || app.github_action.is_some()
        || app.pr_action.is_some()
    {
        return Action::Explain(
            "Wait for the original observed Checks read before opening its run".into(),
        );
    }
    match PrChecksOrigin::capture_scoped_v1(app) {
        Ok(_) => Action::PrChecks(PrChecksUiAction::Open),
        Err(reason) => Action::Explain(reason),
    }
}
pub struct ScopedPrCheckInputV1 {
    origin: Arc<PrChecksOrigin>,
}
impl ScopedPrCheckInputV1 {
    pub fn origin(&self) -> &Arc<PrChecksOrigin> {
        &self.origin
    }
}
impl PrChecksRoute {
    pub fn scoped_v1(&self) -> bool {
        self.origin.scoped_v1()
    }
    pub fn scoped_origin_v1(&self) -> &Arc<PrChecksOrigin> {
        &self.origin
    }
}
#[derive(Default)]
pub struct ScopedPrCheckReadLimitsV1 {
    pub bridge: PrCheckRunReadLimits,
    pub logs: PrCheckLogReadLimits,
    pub access: RepositoryCatalogueAccessLimits,
}
pub fn take_actions_v1(
    app: &mut App,
) -> Option<(Arc<PrChecksRoute>, ActionsWorkspace, ActionsAction)> {
    let route = app
        .pr_checks_route
        .clone()
        .filter(|route| route.scoped_v1())?;
    let action = app.actions_action.take()?;
    if !owns_origin(app, &route.origin) {
        retire(app, true);
        crate::repository_catalogue_native::privacy(app);
        return None;
    }
    if let Some(reason) = scoped_unavailable_v1(&route, &action) {
        app.dispatch(Action::Explain(reason));
        return None;
    }
    // Select/paging/Back are handled purely by App; only genuine worker actions
    // may enter this function. Unknown queued actions never hit legacy apply.
    if !matches!(action, ActionsAction::OpenJob(_) | ActionsAction::Logs(_)) {
        return None;
    }
    let workspace = app.actions.take()?;
    app.actions_load_requested = false;
    app.refresh_requested = false;
    app.hits.clear();
    Some((route, workspace, action))
}
pub fn draw_pending_v1(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) {
    use ratatui::{
        style::Modifier,
        widgets::{Clear, Paragraph},
    };
    if app.view != View::Actions
        || app.actions.is_some()
        || !app
            .pr_checks_route
            .as_ref()
            .is_some_and(|route| route.scoped_v1())
        || area.width == 0
        || area.height == 0
    {
        return;
    }
    frame.render_widget(Clear, area);
    let rect = ratatui::layout::Rect::new(area.x, area.y, area.width.min(22), 1);
    let mut style = app
        .theme()
        .style(crate::theme::SemanticToken::Focus)
        .remove_modifier(Modifier::UNDERLINED);
    if !app.actions_content_focused && app.focus == app.hits.len() {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    frame.render_widget(Paragraph::new("[b Back to PR checks]").style(style), rect);
    app.hits.push(crate::app::HitRegion {
        rect,
        action: Action::Actions(ActionsAction::Back),
    });
    if area.height > 1 {
        frame.render_widget(
            Paragraph::new("Preparing exact original check read • previous private run/log hidden"),
            ratatui::layout::Rect::new(area.x, area.y + 1, area.width, area.height - 1),
        );
    }
}
pub fn take_open_v1(app: &mut App) -> Result<ScopedPrCheckInputV1, String> {
    let origin = app
        .pr_checks_origin
        .clone()
        .filter(|origin| origin.scoped_v1())
        .ok_or("No scoped original check request")?;
    if !app.pr_checks_request || !owns_origin(app, &origin) {
        return Err("Original scoped check request changed".into());
    }
    // The actual private page and cached Inbox stay on App throughout queueing
    // and the external read. Only the small opaque original owner is sent.
    app.pr_checks_request = false;
    app.refresh_requested = false;
    Ok(ScopedPrCheckInputV1 { origin })
}
pub struct ScopedPrCheckCompletionV1 {
    expected_origin: Arc<PrChecksOrigin>,
    origin: Arc<PrChecksOrigin>,
    page: Option<PrPage>,
    actions: Option<ActionsWorkspace>,
    route: Option<Arc<PrChecksRoute>>,
    identity: HostIdentity,
    unavailable: Option<&'static str>,
}
impl std::fmt::Debug for ScopedPrCheckCompletionV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedPrCheckCompletionV1(<original-owned read; no write permission>)")
    }
}
pub fn install_v1(app: &mut App, mut value: ScopedPrCheckCompletionV1) -> Result<(), String> {
    if !owns_origin(app, &value.expected_origin)
        || !value.origin.scoped.as_ref().is_some_and(|owner| {
            let prior = value.expected_origin.scoped.as_ref().unwrap();
            Arc::ptr_eq(&owner.source, &prior.source)
                && Arc::ptr_eq(&owner.active, &prior.active)
                && Arc::ptr_eq(&owner.resource, &prior.resource)
                && owner.cached_selected == prior.cached_selected
                && owner.check_index == prior.check_index
        })
    {
        return Err("Original scoped check owner changed; no private result installed".into());
    }
    if value.page.is_none() {
        let old = &value.expected_origin.scoped.as_ref().unwrap().navigation;
        if app
            .github
            .as_ref()
            .and_then(|github| github.pr.as_ref())
            .is_none_or(|page| !matches_page(old, page))
        {
            return Err("Original Checks page changed before selected/read completion".into());
        }
    }
    // All checks above precede ANY identity/page/status/private effects. SQL is
    // already closed and never recaptured by this pure foreground installer.
    let github = app
        .github
        .as_mut()
        .ok_or("Original PR context unavailable")?;
    let retained_page = value.page.is_none();
    if let Some(page) = value.page.take() {
        let navigation = &value.origin.scoped.as_ref().unwrap().navigation;
        if !matches_page(navigation, &page) {
            return Err("Original scoped check page changed; no result installed".into());
        }
        github.pr = Some(page);
        github.scoped_pr_navigation = Some(navigation.clone());
    }
    // No moved/cloned private page for an initial open or a choice. After all original-owner and
    // old-page checks, rebind only the genuinely closed canonical navigation.
    if retained_page {
        github.pr.as_mut().unwrap().details.summary.pr.repository =
            value.origin.summary.pr.repository.clone();
        github.scoped_pr_navigation =
            Some(value.origin.scoped.as_ref().unwrap().navigation.clone());
    }
    app.pr_check_log_matches_v2 = None;
    app.pr_check_log_editor_v2 = None;
    app.pr_check_annotations_detail_v2 = None;
    github.identity = value.identity;
    app.hits.clear();
    app.avatar_states.clear();
    if let Some(mut actions) = value.actions {
        if let Some(route) = value.route.as_ref() {
            preserve_controls_v1(route, &mut actions);
        }
        app.context = actions.context();
        app.actions = Some(actions);
        app.pr_checks_origin = Some(value.origin);
        app.pr_checks_route = value.route;
        app.view = View::Actions;
        app.selected = 0;
        app.scroll = 0;
        app.actions_content_focused = true;
        app.actions_load_requested = false;
        app.actions_action = None;
        app.status = "Exact scoped PR check/run/attempt/job • read only; no broader scope".into();
        if let Some(reason) = value.unavailable {
            app.dispatch(Action::Explain(reason.into()));
        }
    } else {
        app.actions = None;
        app.actions_action = None;
        app.actions_load_requested = false;
        app.view = View::Inbox;
        app.selected = value.origin.row;
        app.scroll = value.origin.top;
        app.github_content_focused = true;
        app.context = github.context();
        app.pr_checks_origin = value.route.as_ref().map(|_| value.origin);
        app.pr_checks_route = value.route;
        app.status = "Original Checks retained • no unique complete run/job correlation".into();
        if let Some(reason) = value.unavailable {
            app.dispatch(Action::Explain(reason.into()));
        }
    }
    Ok(())
}
fn safe_failure(kind: GithubErrorKind) -> GithubError {
    GithubError {
        kind,
        message: "Original scoped PR check read unavailable; private results hidden".into(),
    }
}
fn safe_sql(reason: String) -> GithubError {
    local_error(reason)
}
fn copied_origin_with_current_v1(
    origin: &Arc<PrChecksOrigin>,
    identity: HostIdentity,
    canonical: &str,
) -> Result<Arc<PrChecksOrigin>, String> {
    let old = origin
        .scoped
        .as_ref()
        .ok_or("Original scoped owner unavailable")?;
    let mut summary = origin.summary.clone();
    summary.pr.repository = canonical.into();
    let navigation = Arc::new(
        ScopedPrNavigationV1::bind(
            old.source.scope(),
            old.navigation.original(),
            &PrKey::from(&summary.pr),
            &summary,
        )
        .map_err(|e| e.to_string())?,
    );
    Ok(Arc::new(PrChecksOrigin {
        identity,
        summary,
        check: origin.check.clone(),
        row: origin.row,
        top: origin.top,
        scoped: Some(ScopedPrCheckOwnerV1 {
            source: old.source.clone(),
            active: old.active.clone(),
            navigation,
            resource: old.resource.clone(),
            cached_selected: old.cached_selected.clone(),
            check_index: old.check_index,
        }),
    }))
}
/// Whole original set and queue clock remain intact. Own6 + shared12 happy.
pub fn load_v1(
    input: ScopedPrCheckInputV1,
    provider: &GithubProvider,
    database: &Path,
    limits: &ScopedPrCheckReadLimitsV1,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let expected = input.origin.clone();
    let result: Result<ScopedPrCheckCompletionV1, GithubError> = (|| {
        let owner = expected
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?;
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
        let mut selected = limits.bridge.clone();
        selected.max_duration = selected.max_duration.min(session.remaining(cancel)?);
        let children = 10usize
            .checked_add(selected.max_run_pages)
            .and_then(|n| n.checked_add(selected.max_job_pages))
            .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?;
        let window = session.reserve_measured_external_reads(
            children,
            selected.max_document_bytes,
            selected.max_response_bytes,
            2,
            cancel,
        )?;
        let bounded = provider.clone().with_measured_read_window(window)?;
        let current = session.verify(owner.navigation.original(), cancel)?;
        let mut local = None;
        let mut actions_guard = None;
        let observation = Arc::new(bounded.pr_check_run_from_resource_with_guard_v1(
            &identity,
            current,
            owner.resource.observed(),
            owner.check_index,
            &selected,
            cancel,
            |phase| match phase {
                PrCheckReadPhase::BeforeRunCollection => {
                    if local.is_some() || actions_guard.is_some() {
                        return Err(safe_failure(GithubErrorKind::IdentityChanged));
                    }
                    local = Some(ResourceLocalGuard::capture(
                        database,
                        &owner.source,
                        &identity,
                        &expected.summary,
                    )?);
                    actions_guard = Some(Arc::new(
                        PrChecksSqlGuard::capture(database, &identity, &expected.summary)
                            .map_err(safe_sql)?,
                    ));
                    Ok(())
                }
                PrCheckReadPhase::BeforeFinalProof => {
                    local
                        .as_ref()
                        .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
                        .validate(database, &owner.source)?;
                    actions_guard
                        .as_ref()
                        .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
                        .validate(database)
                        .map_err(safe_sql)
                }
            },
        )?);
        let closing = session.verify(owner.navigation.original(), cancel)?;
        if closing.repository().full_name != observation.repository().full_name
            || closing.repository().permissions.pull != Some(true)
        {
            return Err(safe_failure(GithubErrorKind::IdentityChanged));
        }
        let deadline = session.deadline();
        let closed = session.close_with_guard(cancel, || {
            local
                .as_ref()
                .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
                .validate(database, &owner.source)?;
            actions_guard
                .as_ref()
                .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
                .validate(database)
                .map_err(safe_sql)
        })?;
        if !observation.matches_resource_v1(
            closed.identity(),
            owner.resource.observed(),
            owner.check_index,
        ) {
            return Err(safe_failure(GithubErrorKind::IdentityChanged));
        }
        let installed = copied_origin_with_current_v1(
            &expected,
            closed.identity().clone(),
            &observation.repository().full_name,
        )
        .map_err(safe_sql)?;
        let unavailable = observation.unavailable().map(|reason| reason.label());
        let choice_rows = choices::prepare_rows(&observation);
        let guard = actions_guard.ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        let scoped_local =
            Some(Arc::new(local.ok_or_else(|| {
                safe_failure(GithubErrorKind::IdentityChanged)
            })?));
        let retained = Arc::new(PrChecksRoute {
            origin: installed.clone(),
            observation: observation.clone(),
            guard: guard.clone(),
            scoped_local,
            choice_rows,
            loaded_capture_v2: None,
            annotations_v2: None,
        });
        let (actions, route) = if observation.link().is_some() {
            let mut actions = PreparedPrCheckActions::new(observation.clone())?.into_workspace();
            actions.identity = closed.identity().clone();
            decorate_scoped_v1(&mut actions, &guard, &observation, owner).map_err(safe_sql)?;
            (Some(actions), Some(retained))
        } else {
            let retained = retained.choice_rows.is_some().then_some(retained);
            (None, retained)
        };
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err(safe_failure(GithubErrorKind::Transport));
        }
        Ok(ScopedPrCheckCompletionV1 {
            expected_origin: expected.clone(),
            origin: installed,
            page: None,
            actions,
            route,
            identity: closed.identity().clone(),
            unavailable,
        })
    })();
    match result {
        Ok(value) => Ok(PreparedView {
            scoped_pr_checks: Some(Box::new(value)),
            ..Default::default()
        }),
        Err(_) => Ok(hidden(expected)),
    }
}
pub fn scoped_unavailable_v1(route: &PrChecksRoute, action: &ActionsAction) -> Option<String> {
    if !route.origin.scoped_v1() {
        return unavailable_action(action);
    }
    let allowed = matches!(action, ActionsAction::Refresh | ActionsAction::Watch)
        && route.observation.selected_run_v1().is_some()
        || matches!(action, ActionsAction::Annotations(id) if choices::other_job_allowed(route,Some(*id)))
        || matches!(action, ActionsAction::Compose(crate::actions_workspace::ActionsCommand::SearchLogs) if route.loaded_capture_v2.is_some())
        || match action {
            ActionsAction::Back
            | ActionsAction::Select(_)
            | ActionsAction::ControlsPage(_)
            | ActionsAction::Unavailable(_) => true,
            ActionsAction::OpenJob(job) => {
                choices::other_job_allowed(route, Some(*job))
                    || route
                        .observation
                        .link()
                        .is_some_and(|link| link.attempt().jobs.iter().any(|item| item.id == *job))
            }
            ActionsAction::Logs(job) => {
                choices::other_job_allowed(route, *job)
                    || route
                        .observation
                        .link()
                        .is_some_and(|link| *job == Some(link.selected_job()))
            }
            _ => false,
        };
    (!allowed).then(|| "This scoped PR-check route admits positively observed run/job navigation and freshly original-guarded matched/other-member logs. Other catalogues, drafts, recovery and publication require their own original scoped admission; nothing sent.".into())
}
fn decorate_scoped_v1(
    workspace: &mut ActionsWorkspace,
    guard: &PrChecksSqlGuard,
    observation: &PrCheckRunObservation,
    owner: &ScopedPrCheckOwnerV1,
) -> Result<(), String> {
    guard.decorate_scoped_read_v1(
        workspace,
        owner.resource.observed(),
        owner.check_index,
        observation,
    )?;
    workspace.watching = false;
    preserve_scoped_controls_v1(workspace, observation);
    Ok(())
}
/// Selection refreshes ordinary Actions form availability. Restore only this
/// route's prepared control policy afterward, without SQL or body preparation.
pub(crate) fn preserve_controls_v1(route: &PrChecksRoute, workspace: &mut ActionsWorkspace) {
    if route.scoped_v1() {
        preserve_scoped_controls_v1(workspace, &route.observation);
        // SAME single scoped policy survives generic selection. Do not enable
        // another Compose/draft/publication route as a side effect.
        let admitted: Vec<bool> = workspace
            .controls
            .iter()
            .map(|c| choices::continuations_v2::admitted_action(route, workspace, &c.action))
            .collect();
        for (control, allowed) in workspace.controls.iter_mut().zip(admitted) {
            if allowed {
                control.disabled_reason = None;
            }
        }
        if !workspace
            .controls
            .iter()
            .any(|c| c.action == ActionsAction::Watch)
            && route.observation.selected_run_v1().is_some()
        {
            workspace
                .controls
                .push(crate::actions_workspace::ActionsControl {
                    label: "[w Watch / Stop]".into(),
                    action: ActionsAction::Watch,
                    disabled_reason: None,
                });
        }
        for control in &mut workspace.controls {
            if control.action == ActionsAction::Watch {
                control.label = "[w Watch / Stop]".into();
            }
        }
    }
}
fn preserve_scoped_controls_v1(
    workspace: &mut ActionsWorkspace,
    observation: &PrCheckRunObservation,
) {
    let matched = observation.link().map(|link| link.selected_job());
    // Enable ONLY the original read-only Open/Preview intents whose exact
    // current run/member is already carried by this real PR observation.
    let run_entries: Vec<bool> = workspace
        .controls
        .iter()
        .map(|control| {
            actions_origin_v2::run_command_visible_v4(workspace, observation, &control.action)
        })
        .collect();
    for (control, run_entry) in workspace.controls.iter_mut().zip(run_entries) {
        let allowed = matches!(
            control.action,
            ActionsAction::Back
                | ActionsAction::Select(_)
                | ActionsAction::ControlsPage(_)
                | ActionsAction::OpenJob(_)
        ) || matches!(control.action, ActionsAction::Logs(job) if job == matched && matched.is_some());
        if control.action == ActionsAction::Back {
            control.label = "[b Back to PR checks]".into();
        }
        let allowed = allowed
            || run_entry
            || actions_origin_v2::resource_entry_action(&control.action)
            || matches!(control.action, ActionsAction::Logs(job) if job.is_some_and(|id| observation.selected_run_v1().is_some_and(|run| run.members().iter().any(|member| member.id()==id))));
        if run_entry {
            control.disabled_reason = None;
        } else if !allowed {
            control.disabled_reason = Some("Additional scoped PR-check action needs its own original access/draft/publication admission; nothing sent.".into());
        }
    }
}
/// Worker-only page-free subsequent read. Actions moves; original Checks stays
/// on App. Own6 + shared9 logs; pure OpenJob has no provider/SQL calls.
pub fn apply_v1(
    route: Arc<PrChecksRoute>,
    mut workspace: ActionsWorkspace,
    action: ActionsAction,
    provider: &GithubProvider,
    database: &Path,
    limits: &ScopedPrCheckReadLimitsV1,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let expected = route.origin.clone();
    if scoped_unavailable_v1(&route, &action).is_some() {
        return Err("Scoped action unavailable; no private draft read or fallback".into());
    }
    let mut captured = None;
    let result: Result<HostIdentity, GithubError> = (|| match action {
        ActionsAction::OpenJob(job) => {
            workspace.apply(provider, &ActionsAction::OpenJob(job), cancel)?;
            Ok(workspace.identity.clone())
        }
        ActionsAction::Logs(job) => {
            let owner = expected
                .scoped
                .as_ref()
                .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?;
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
            let mut selected = limits.logs.clone();
            selected.max_duration = selected.max_duration.min(session.remaining(cancel)?);
            let window = session.reserve_measured_external_reads(
                9,
                selected.max_document_bytes.max(selected.max_log_bytes),
                selected.max_response_bytes,
                2,
                cancel,
            )?;
            let bounded = provider.clone().with_measured_read_window(window)?;
            let current = session.verify(owner.navigation.original(), cancel)?;
            let local = route
                .scoped_local
                .as_ref()
                .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
            let logs = bounded.pr_check_logs_from_resource_with_guard_v1(
                route.observation.clone(),
                current,
                owner.resource.observed(),
                owner.check_index,
                &selected,
                cancel,
                |phase| match phase {
                    PrCheckLogReadPhase::BeforeCapture | PrCheckLogReadPhase::BeforeFinalProof => {
                        local.validate(database, &owner.source)?;
                        route.guard.validate(database).map_err(safe_sql)
                    }
                },
            )?;
            let closing = session.verify(owner.navigation.original(), cancel)?;
            if closing.repository().full_name != route.observation.repository().full_name
                || closing.repository().permissions.pull != Some(true)
            {
                return Err(safe_failure(GithubErrorKind::IdentityChanged));
            }
            let deadline = session.deadline();
            let closed = session.close_with_guard(cancel, || {
                local.validate(database, &owner.source)?;
                route.guard.validate(database).map_err(safe_sql)
            })?;
            if cancel.is_cancelled() || Instant::now() >= deadline {
                return Err(safe_failure(GithubErrorKind::Transport));
            }
            let page = workspace
                .run
                .as_mut()
                .ok_or_else(|| safe_failure(GithubErrorKind::InvalidTarget))?;
            page.logs = Some(logs.logs().to_owned());
            page.selected_job = job;
            captured = Some(Arc::new(logs));
            workspace.view = ActionsView::Logs;
            workspace.selected = 0;
            workspace.top = 0;
            Ok(closed.identity().clone())
        }
        _ => Err(safe_failure(GithubErrorKind::InvalidTarget)),
    })();
    match result {
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
                    .ok_or("Original scoped read owner unavailable")?,
            )?;
            let next = if let Some(captured) = captured {
                Arc::new(PrChecksRoute {
                    origin: route.origin.clone(),
                    observation: route.observation.clone(),
                    guard: route.guard.clone(),
                    scoped_local: route.scoped_local.clone(),
                    choice_rows: route.choice_rows.clone(),
                    loaded_capture_v2: Some(captured),
                    annotations_v2: None,
                })
            } else {
                route
            };
            preserve_controls_v1(&next, &mut workspace);
            Ok(PreparedView {
                scoped_pr_checks: Some(Box::new(ScopedPrCheckCompletionV1 {
                    expected_origin: expected.clone(),
                    origin: expected,
                    page: None,
                    actions: Some(workspace),
                    route: Some(next),
                    identity,
                    unavailable: None,
                })),
                ..Default::default()
            })
        }
        Err(_) => Ok(hidden(expected)),
    }
}
