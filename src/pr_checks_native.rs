//! Owned PR-check navigation and worker routing through the shared Actions view.
//! Local intents and snapshots are not authentication or publication authority.
use crate::{
    actions_workspace::{ActionsAction, ActionsView, ActionsWorkspace},
    app::{Action, App, View},
    github_workspace::{GithubWorkspace, PrTab},
    pr_checks_actions::PreparedPrCheckActions,
    pr_checks_sql_guard::PrChecksSqlGuard,
    providers::{
        gh::{
            Check, GithubError, GithubErrorKind, GithubProvider, HostIdentity,
            PrCheckLogReadLimits, PrCheckReadPhase, PrCheckRunObservation, PrCheckRunReadLimits,
            PrSummary,
        },
        runner::CancellationToken,
    },
    runtime::PreparedView,
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

#[path = "scoped_pr_checks_v1.rs"]
pub mod scoped;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrChecksUiAction {
    Open,
    Back,
}

/// Bounded semantic source and stable row, not a retained workspace or PR body.
pub struct PrChecksOrigin {
    identity: HostIdentity,
    summary: PrSummary,
    check: Check,
    row: usize,
    top: usize,
    scoped: Option<scoped::ScopedPrCheckOwnerV1>,
}
impl std::fmt::Debug for PrChecksOrigin {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrChecksOrigin(<local exact check intent>)")
    }
}
fn same_principal(a: &HostIdentity, b: &HostIdentity) -> bool {
    a.host == b.host && a.account_id.is_some_and(|id| id > 0) && a.account_id == b.account_id
}
fn same_source(a: &PrSummary, b: &PrSummary) -> bool {
    a.pr.host == b.pr.host
        && a.pr.id == b.pr.id
        && a.pr.repository.eq_ignore_ascii_case(&b.pr.repository)
        && a.pr.number == b.pr.number
        && a.pr.state == b.pr.state
        && a.pr.draft == b.pr.draft
        && a.merged == b.merged
        && a.head_sha == b.head_sha
        && a.base_sha == b.base_sha
        && a.head_ref == b.head_ref
        && a.base_ref == b.base_ref
}
/// Borrow-only local availability; it does not correlate a run or perform I/O.
fn selected_source(github: &GithubWorkspace, row: usize) -> Result<(&PrSummary, &Check), String> {
    let page = github
        .pr
        .as_ref()
        .filter(|p| p.tab == PrTab::Checks)
        .ok_or("Open the actual PR Checks tab before opening a run")?;
    let check = page
        .check_at(row)
        .ok_or("Select an actual check row first")?;
    let source = &page.details.summary;
    let fields = [
        &github.identity.host,
        &github.identity.login,
        &github.identity.gh_version,
        &source.pr.host,
        &source.pr.repository,
        &source.pr.id,
        &source.pr.state,
        &source.head_sha,
        &source.base_sha,
        &source.head_ref,
        &source.base_ref,
        &check.name,
        &check.status,
        &check.conclusion,
        &check.url,
        &check.head_sha,
        &check.kind,
    ];
    if fields.iter().any(|v| v.len() > 4096)
        || fields.iter().map(|v| v.len()).sum::<usize>() > 32 * 1024
        || !github
            .identity
            .account_id
            .is_some_and(|id| id > 0 && id <= i64::MAX as u64)
        || check.kind != "check"
        || check.id == 0
        || check.head_sha != source.head_sha
        || !crate::search::model::valid_sha(&source.head_sha)
        || !crate::search::model::valid_sha(&source.base_sha)
    {
        return Err("CheckRun unavailable: no bounded exact check/full PR source. Commit status contexts do not identify an Actions attempt.".into());
    }
    Ok((source, check))
}
pub fn open_action(app: &App) -> Action {
    if app.view != View::Inbox
        || app.loading[View::Inbox.index()]
        || app.github_action.is_some()
        || app.pr_action.is_some()
    {
        return Action::Explain("Wait for the exact PR/check read before opening its run".into());
    }
    match app
        .github
        .as_ref()
        .ok_or_else(|| "No exact PR Checks page is loaded".to_owned())
        .and_then(|github| selected_source(github, app.inbox_list.selected))
    {
        Ok(_) => Action::PrChecks(PrChecksUiAction::Open),
        Err(reason) => Action::Explain(reason),
    }
}
impl PrChecksOrigin {
    fn capture(github: &GithubWorkspace, row: usize, top: usize) -> Result<Self, String> {
        let (source, check) = selected_source(github, row)?;
        // Only facts consumed by canonical correlation are copied. In particular
        // an arbitrarily large cached title/body/actor is never retained here.
        let summary = PrSummary {
            pr: crate::providers::gh::PullRequest {
                host: source.pr.host.clone(),
                id: source.pr.id.clone(),
                repository: source.pr.repository.clone(),
                number: source.pr.number,
                state: source.pr.state.clone(),
                draft: source.pr.draft,
                title: String::new(),
                body: String::new(),
                url: String::new(),
                author: Default::default(),
                created_at: String::new(),
                updated_at: String::new(),
                queues: Default::default(),
            },
            head_sha: source.head_sha.clone(),
            base_sha: source.base_sha.clone(),
            head_ref: source.head_ref.clone(),
            base_ref: source.base_ref.clone(),
            merged: source.merged,
            auto_merge: source.auto_merge,
            merge_commit_sha: None,
            mergeable: source.mergeable,
            mergeable_state: String::new(),
            additions: 0,
            deletions: 0,
            changed_files: 0,
        };
        Ok(Self {
            identity: github.identity.clone(),
            summary,
            check: check.clone(),
            row,
            top,
            scoped: None,
        })
    }
    fn matches(&self, github: &GithubWorkspace) -> bool {
        if let Some(matches) = self.matches_scoped_v1(github) {
            return matches;
        }
        same_principal(&self.identity, &github.identity)
            && github.pr.as_ref().is_some_and(|page| {
                page.tab == PrTab::Checks
                    && same_source(&self.summary, &page.details.summary)
                    && page.check_at(self.row).is_some_and(|check| {
                        check.id == self.check.id
                            && check.kind == self.check.kind
                            && check.head_sha == self.check.head_sha
                    })
            })
    }
}
pub struct PrChecksRoute {
    origin: Arc<PrChecksOrigin>,
    observation: Arc<PrCheckRunObservation>,
    guard: Arc<PrChecksSqlGuard>,
    scoped_local: Option<Arc<crate::scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard>>,
    choice_rows: Option<Arc<scoped::choices::PreparedPrCheckChoiceRowsV1>>,
    loaded_capture_v2: Option<Arc<crate::providers::gh::PrCheckLogsObservation>>,
    annotations_v2: Option<Arc<crate::providers::gh::PrCheckAnnotationsReadV2>>,
}
impl PrChecksRoute {
    pub(crate) fn loaded_capture_v2(
        &self,
    ) -> Option<&Arc<crate::providers::gh::PrCheckLogsObservation>> {
        self.loaded_capture_v2.as_ref()
    }
    pub(crate) fn annotations_v2(
        &self,
    ) -> Option<&Arc<crate::providers::gh::PrCheckAnnotationsReadV2>> {
        self.annotations_v2.as_ref()
    }
    /// Borrowed opaque observed navigation metadata, not current access/write authority.
    pub fn observation(&self) -> &Arc<PrCheckRunObservation> {
        &self.observation
    }
}
impl std::fmt::Debug for PrChecksRoute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrChecksRoute(<owned read correlation; not permission>)")
    }
}
pub fn owns_origin(app: &App, origin: &Arc<PrChecksOrigin>) -> bool {
    app.pr_checks_origin
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, origin))
        && app
            .github
            .as_ref()
            .is_some_and(|github| origin.matches(github))
        && origin.scoped.as_ref().is_none_or(|owner| owner.owns(app))
        && matches!(app.view, View::Inbox | View::Actions)
}
fn decorate(workspace: &mut ActionsWorkspace, guard: &PrChecksSqlGuard) -> Result<(), String> {
    guard.decorate(workspace)?;
    for control in &mut workspace.controls {
        if control.action == ActionsAction::Back {
            control.label = "[b Back to PR checks]".into();
        }
        // Additional private reads need the same original PR bracket. The
        // dedicated Actions view remains available outside this owned route.
        if let Some(reason) = unavailable_action(&control.action) {
            control.disabled_reason = Some(reason);
        }
    }
    Ok(())
}
pub fn unavailable_action(action: &ActionsAction) -> Option<String> {
    if matches!(
        action,
        ActionsAction::Refresh
            | ActionsAction::Watch
            | ActionsAction::Annotations(_)
            | ActionsAction::Artifacts
            | ActionsAction::View(_)
            | ActionsAction::Repository(_)
            | ActionsAction::OpenRun { .. }
    ) {
        Some("This PR-check route retains an exact original run/job/log read. Additional catalogues need their own fresh source admission; no broader repository or cached permission is substituted.".into())
    } else {
        None
    }
}
pub fn dispatch(app: &mut App, action: PrChecksUiAction) {
    match action {
        PrChecksUiAction::Open => {
            if app.view != View::Inbox
                || app.loading[View::Inbox.index()]
                || app.github_action.is_some()
                || app.pr_action.is_some()
            {
                app.dispatch(Action::Explain(
                    "Wait for the exact PR/check read before opening its run".into(),
                ));
                return;
            }
            let captured = if app.remote_scope.is_some() {
                PrChecksOrigin::capture_scoped_v1(app)
            } else {
                app.github
                    .as_ref()
                    .ok_or_else(|| "No exact PR Checks page is loaded".to_owned())
                    .and_then(|github| {
                        PrChecksOrigin::capture(github, app.inbox_list.selected, app.inbox_list.top)
                    })
            };
            match captured {
                Ok(origin) => {
                    retire(app, true);
                    app.pr_checks_origin = Some(Arc::new(origin));
                    app.pr_checks_request = true;
                    app.hits.clear();
                    app.status =
                        "Preparing exact CheckRun / suite / run / attempt / job • no mutation"
                            .into();
                }
                Err(reason) => app.dispatch(Action::Explain(reason)),
            }
        }
        PrChecksUiAction::Back => {
            let Some(route) = app
                .pr_checks_route
                .clone()
                .filter(|route| owns_origin(app, &route.origin))
            else {
                retire(app, true);
                app.dispatch(Action::Explain("Original PR Checks source changed; no cached private route or broader inbox was restored".into()));
                return;
            };
            let row = route.origin.row;
            let top = route.origin.top;
            retire(app, true);
            app.pr_checks_cancel_actions = true;
            app.view = View::Inbox;
            app.inbox_list.selected = row;
            app.inbox_list.top = top;
            app.github_content_focused = true;
            app.actions_content_focused = false;
            app.context = app.github.as_ref().unwrap().context();
            app.status =
                "Returned to original prepared PR Checks • no refresh or write permission restored"
                    .into();
        }
    }
}
pub fn retire(app: &mut App, drop_run: bool) {
    scoped::choices::continuations_v2::retire(app);
    scoped::choices::retire(app);
    let owned = app.pr_checks_route.take().is_some() || app.pr_checks_origin.is_some();
    app.pr_checks_origin = None;
    app.pr_checks_request = false;
    if owned {
        app.hits.clear();
        if drop_run {
            app.actions = None;
            app.actions_action = None;
            app.actions_load_requested = false;
            app.content[View::Actions.index()] =
                vec!["Original PR check route retired • private run pages hidden".into()];
        } else if let Some(workspace) = &mut app.actions {
            for control in &mut workspace.controls {
                if control.action == ActionsAction::Back {
                    control.label = "[b Back]".into();
                }
            }
        }
    }
}
pub fn before_action(app: &mut App, action: &Action) {
    let retire = match action {
        Action::View(_) | Action::Github(_) | Action::Pr(_) | Action::Cancel => true,
        Action::Refresh => app.view == View::Inbox,
        Action::GithubRow(row) => app.pr_checks_origin.as_ref().is_some_and(|o| o.row != *row),
        _ => false,
    };
    if retire {
        self::retire(app, true);
    }
}
fn hidden(origin: Arc<PrChecksOrigin>) -> PreparedView {
    PreparedView {
        clear_private: true,
        pr_checks_origin: Some(origin),
        status: "PR check context changed • private pages hidden".into(),
        ..Default::default()
    }
}
fn local_error(_reason: String) -> GithubError {
    GithubError {
        kind: GithubErrorKind::Transport,
        message: "Original local check visibility/publication fence changed; private pages hidden"
            .into(),
    }
}
fn remaining(start: Instant, cancel: &CancellationToken) -> Result<Duration, String> {
    let remaining = Duration::from_secs(30).saturating_sub(start.elapsed());
    if remaining.is_zero() || cancel.is_cancelled() {
        return Err("Original check operation expired or cancelled".into());
    }
    Ok(remaining)
}
fn identity(
    provider: &GithubProvider,
    start: Instant,
    expected: &HostIdentity,
    cancel: &CancellationToken,
) -> Result<HostIdentity, String> {
    let identity = provider
        .identity_before(start + Duration::from_secs(30), cancel)
        .map_err(|e| e.to_string())?;
    if !same_principal(expected, &identity)
        || identity.login.len() > 256
        || identity.gh_version.len() > 4096
    {
        return Err("Original positive numeric check principal changed".into());
    }
    remaining(start, cancel)?;
    Ok(identity)
}
/// Worker only. Legacy identity4 captures each have a known4MiB cap, separate
/// from the source2MiB accounting; no claim that those raw counters are shared.
pub fn load(
    origin: Arc<PrChecksOrigin>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let start = Instant::now();
    let failure_origin = origin.clone();
    let read: Result<PreparedView, String> = (|| {
        let current = identity(provider, start, &origin.identity, cancel)?;
        let mut captured_guard = None;
        let limits = PrCheckRunReadLimits {
            max_duration: remaining(start, cancel)?,
            ..Default::default()
        };
        let observed = Arc::new(
            provider
                .pr_check_run_with_guard(
                    &current,
                    &origin.summary,
                    &origin.check,
                    &limits,
                    cancel,
                    |phase| {
                        remaining(start, cancel).map_err(local_error)?;
                        match phase {
                            PrCheckReadPhase::BeforeRunCollection => {
                                // The factory has now observed actual current
                                // user/repository/PR/check access. Do not read a
                                // private local ledger from identity alone.
                                if captured_guard.is_some() {
                                    return Err(local_error(
                                        "Original check fence cannot rebase".into(),
                                    ));
                                }
                                captured_guard = Some(Arc::new(
                                    PrChecksSqlGuard::capture(database, &current, &origin.summary)
                                        .map_err(local_error)?,
                                ));
                                Ok(())
                            }
                            PrCheckReadPhase::BeforeFinalProof => captured_guard
                                .as_ref()
                                .ok_or_else(|| local_error("Original check fence missing".into()))?
                                .validate(database)
                                .map_err(local_error),
                        }
                    },
                )
                .map_err(|e| e.to_string())?,
        );
        let guard = captured_guard.ok_or("Original check fence missing")?;
        let final_identity = identity(provider, start, &current, cancel)?;
        if !observed.matches_original(&final_identity, &origin.summary, &origin.check) {
            return Err("Original check source changed".into());
        }
        let prepared = PreparedPrCheckActions::new(observed.clone()).map_err(|e| e.to_string())?;
        let mut workspace = prepared.into_workspace();
        workspace.identity = final_identity;
        decorate(&mut workspace, &guard)?; // Pure captured ledger, not SQL after final proof.
        remaining(start, cancel)?;
        let route = Arc::new(PrChecksRoute {
            origin: origin.clone(),
            observation: observed,
            guard,
            scoped_local: None,
            choice_rows: None,
            loaded_capture_v2: None,
            annotations_v2: None,
        });
        Ok(PreparedView {
            context: Some(workspace.context()),
            status: "Exact PR check run/attempt/job loaded • original read; no mutation".into(),
            actions: Some(workspace),
            navigate_to: Some(View::Actions),
            pr_checks_origin: Some(origin),
            pr_checks_route: Some(route),
            ..Default::default()
        })
    })();
    match read {
        Ok(view) => Ok(view),
        Err(_) => Ok(hidden(failure_origin)),
    }
}
pub fn apply(
    route: Arc<PrChecksRoute>,
    mut workspace: ActionsWorkspace,
    action: ActionsAction,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let failure_origin = route.origin.clone();
    let result: Result<PreparedView, String> = (|| {
        match action {
            ActionsAction::OpenJob(job) => {
                workspace
                    .apply(provider, &ActionsAction::OpenJob(job), cancel)
                    .map_err(|e| e.to_string())?;
            }
            ActionsAction::Logs(job) => {
                let link = route
                    .observation
                    .link()
                    .ok_or("Exact original job unavailable")?;
                if job != Some(link.selected_job()) {
                    return Err("Another job cannot reuse this original CheckRun log route".into());
                }
                let logs = provider
                    .pr_check_logs_with_guard(
                        route.observation.clone(),
                        &PrCheckLogReadLimits::default(),
                        cancel,
                        |_| route.guard.validate(database).map_err(local_error),
                    )
                    .map_err(|e| e.to_string())?;
                let page = workspace
                    .run
                    .as_mut()
                    .ok_or("Original attempt unavailable")?;
                page.logs = Some(logs.logs().to_owned());
                page.logs_skipped_bytes = 0;
                page.selected_job = job;
                workspace.identity = logs.identity().clone();
                workspace.view = ActionsView::Logs;
                workspace.selected = 0;
                workspace.top = 0;
            }
            _ => {
                return Err("Additional PR-check read needs its own exact source admission".into())
            }
        }
        workspace.prepare_rows();
        decorate(&mut workspace, &route.guard)?;
        Ok(PreparedView {
            context: Some(workspace.context()),
            status: "Exact original PR check context • read only".into(),
            actions: Some(workspace),
            pr_checks_origin: Some(route.origin.clone()),
            pr_checks_route: Some(route),
            ..Default::default()
        })
    })();
    match result {
        Ok(view) => Ok(view),
        Err(_) => Ok(hidden(failure_origin)),
    }
}

#[cfg(test)]
#[path = "pr_checks_native_tests.rs"]
mod tests;
