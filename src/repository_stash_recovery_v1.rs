//! Explicit, read-only observation after a genuine failed/refused stash intent.
//! Retained observations are not reconciliation, retry, drop or abort approval.
use crate::{
    app::{Action, App, HitRegion, View},
    git::FailedStashIntentV1,
    providers::{
        local_git_navigation_read_v1::LocalGitNavigationReadV1, runner::CancellationToken,
    },
    theme::SemanticToken,
};
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StashRecoveryActionV1 {
    Inspect,
    Conflict,
    Back,
    Scroll(i32),
}
pub struct OriginalStashRecoveryIntentV1 {
    failed: Arc<FailedStashIntentV1>,
    working: bool,
}
pub struct OriginalStashRecoveryOwnerV1 {
    intent: Arc<OriginalStashRecoveryIntentV1>,
    operation: Arc<LocalGitNavigationReadV1>,
}
pub struct PreparedStashRecoveryV1 {
    owner: Arc<OriginalStashRecoveryOwnerV1>,
    lines: Vec<String>,
    workspace: Option<crate::workspace::Workspace>,
}
pub struct StashRecoveryStateV1 {
    failed: Arc<FailedStashIntentV1>,
    pending: Option<Arc<OriginalStashRecoveryOwnerV1>>,
    lines: Vec<String>,
    scroll: usize,
    scroll_limit: usize,
}
fn actual_failed(app: &App) -> Option<&Arc<FailedStashIntentV1>> {
    let crate::forms::FormPurpose::Local(intent) = &app.form.as_ref()?.purpose else {
        return None;
    };
    let failed = intent.failed_stash_v1.as_ref()?;
    (intent.plan.is_none()
        && !app.form.as_ref()?.preview
        && intent
            .original_stash_source_v1
            .as_ref()
            .is_some_and(|source| Arc::ptr_eq(source, failed.origin()))
        && app
            .workspace
            .as_ref()?
            .actor_origin
            .as_ref()
            .is_some_and(|source| Arc::ptr_eq(source, failed.origin())))
    .then_some(failed)
}
impl OriginalStashRecoveryIntentV1 {
    pub fn owns(&self, app: &App) -> bool {
        actual_failed(app).is_some_and(|failed| Arc::ptr_eq(failed, &self.failed))
            && app
                .original_stash_recovery_v1
                .as_ref()
                .is_some_and(|state| Arc::ptr_eq(&state.failed, &self.failed))
    }
}
impl OriginalStashRecoveryOwnerV1 {
    pub fn begin(
        app: &mut App,
        intent: Arc<OriginalStashRecoveryIntentV1>,
        cancellation: CancellationToken,
    ) -> Result<Arc<Self>, String> {
        if !intent.owns(app) {
            return Err("Stash inspection intent/source changed".into());
        }
        let operation = Arc::new(LocalGitNavigationReadV1::begin(
            intent.failed.origin(),
            cancellation,
        )?);
        let owner = Arc::new(Self { intent, operation });
        let state = app
            .original_stash_recovery_v1
            .as_mut()
            .ok_or("Inspection closed")?;
        state.lines.clear(); // Old observations disappear BEFORE any new observer/effect.
        state.pending = Some(owner.clone());
        state.scroll = 0;
        Ok(owner)
    }
    pub fn cancellation(&self) -> &CancellationToken {
        self.operation.cancellation()
    }
    pub fn owns(&self, app: &App) -> bool {
        self.intent.owns(app)
            && app.view == View::Repository
            && app.original_stash_recovery_request_v1.is_none()
            && !app.original_stash_recovery_cancel_requested_v1
            && app.scoped_inline_work_v1.is_none()
            && app.pr_native_diff_work_v1.is_none()
            && !app.pr_native_diff_cancel_v1
            && app.scoped_reviews_work_v2.is_none()
            && !app.scoped_reviews_cancel_v2
            && !app.scoped_inline_cancel_v1
            && !app.repository_load_requested
            && app.repository_action.is_none()
            && app.original_tree_work_v1.is_none()
            && !app.original_tree_cancel_v1
            && app.original_diff_work_v1.is_none()
            && !app.original_diff_cancel_v1
            && app.original_diff_picker_v1.is_none()
            && app.graph_reread_request_v3.is_none()
            && !app.related_pr_requested_v1
            && !app.related_pr_cancel_v1
            && app.navigation_restore.is_none()
            && !app.navigation_restore_inflight
            && app.local_command.is_none()
            && !app.cancel_requested
            && !app.refresh_requested
            && !app.local_actor_requested
            && !app.local_actor_cancel_requested
            && !app.compare_requested
            && app
                .original_stash_recovery_v1
                .as_ref()
                .and_then(|s| s.pending.as_ref())
                .is_some_and(|p| std::ptr::eq(p.as_ref(), self))
    }
    pub fn prepare(self: &Arc<Self>) -> Result<PreparedStashRecoveryV1, String> {
        let origin = self.intent.failed.origin();
        let result = origin.inspect_stash_v1(&self.intent.failed, &self.operation).and_then(|(status, stages, present)| {
            let source = origin.source();
            use std::os::unix::ffi::OsStrExt;
            let root = crate::git::RawPath(source.root().as_os_str().as_bytes().to_vec()).display();
            let mut lines = vec!["Observed result • read-only; not retry/drop/abort approval".into(),
                format!("Original checkout: {root}"),
                format!("Original {} intent: {}", if self.intent.failed.pop(){"pop"}else{"apply"}, self.intent.failed.sha()),
                if self.intent.failed.command_succeeded(){"Git reported success; view refresh failed".into()}else{"Failed/refused command intent; no-effects not proven".into()},
                format!("Exact stash now observed: {}", if present{"retained in stash list"}else{"absent from stash list; absence does not prove why"}),
                format!("Current HEAD: {}", status.head.as_deref().unwrap_or("unborn")),
                format!("Unmerged stage rows: {}", stages.len()),
                format!("Sequencer observed: {}", status.operation.as_ref().map(|s|format!("{s:?}")).unwrap_or_else(||"none (stash conflicts do not imply merge-abort)".into())),
                "Manual recovery: inspect Working changes, resolve files, then explicitly stage/commit. A retained stash is not automatically dropped.".into()];
            let first_path=stages.first().map(|stage|stage.path.clone());
            for stage in stages { lines.push(format!("Stage {} • {} • {} • {}",stage.stage, stage.mode, stage.id, stage.path.display())); }
            if lines.iter().map(String::len).sum::<usize>() > 64*1024 { return Err("Stash report exceeds 64KiB display bound".into()); }
            let workspace = if self.intent.working {
                origin.with_saved_navigation_read_v1(&self.operation, |repo| {
                    let path=first_path.ok_or("No current unmerged file; inspect original result without a conflict handoff")?;
                    let before=repo.fingerprint().map_err(|e|e.to_string())?;
                    let current:Vec<_>=repo.conflict_stages().map_err(|e|e.to_string())?.into_iter().filter(|stage|stage.path==path).collect();
                    if current.is_empty() {return Err("Original conflict file changed before read-only opening".into());}
                    if current.len()>3 || repo.object_sizes(&current.iter().map(|stage|stage.id.clone()).collect::<Vec<_>>()).map_err(|e|e.to_string())?.iter().any(|size|*size>64*1024) {
                        return Err("Conflict body exceeds bounded existing viewer; original stage objects remain inspectable without opening bodies".into());
                    }
                    let mut workspace=crate::workspace::Workspace::load_from_repository_v1(repo, crate::workspace::RepositoryMode::Overview, "HEAD", None, Some(origin.clone()))?
                        .ok_or("Original conflict checkout unavailable")?;
                    workspace.open_from_repository_v1(crate::workspace::RowAction::Conflict(path),self.cancellation().clone(),Some(repo))?;
                    for row in &mut workspace.rows {
                        if row.label=="Theirs — incoming branch content" {row.label="Theirs — current index stage 3 (origin not proven)".into();}
                    }
                    workspace.rows.insert(0,crate::workspace::DisplayRow{label:"Observed index conflicts • not proof of stash/merge provenance; no merge-abort grant".into(),role:crate::repository_view::SemanticRole::Reference,action:None});
                    if repo.fingerprint().map_err(|e|e.to_string())?!=before {return Err("Original conflict index changed during inspection".into());}
                    Ok(workspace)
                }).map(Some)?
            } else { None };
            Ok(PreparedStashRecoveryV1 { owner: self.clone(), lines, workspace })
        });
        self.operation.seal()?;
        result
    }
}
impl PreparedStashRecoveryV1 {
    pub fn matches(&self, owner: &Arc<OriginalStashRecoveryOwnerV1>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }
}
pub fn fail(app: &mut App, owner: &Arc<OriginalStashRecoveryOwnerV1>) -> bool {
    if !owner.owns(app) {
        return false;
    }
    let state = app
        .original_stash_recovery_v1
        .as_mut()
        .expect("owned state");
    state.pending = None;
    state.lines.clear();
    state.scroll = 0;
    app.status = "Original stash inspection unavailable • observations hidden • nothing retried/reconciled/dropped".into();
    true
}
pub fn accept(
    app: &mut App,
    owner: &Arc<OriginalStashRecoveryOwnerV1>,
    prepared: PreparedStashRecoveryV1,
) -> bool {
    if !owner.owns(app) || !prepared.matches(owner) {
        return false;
    }
    if owner.cancellation().is_cancelled()
        || std::time::Instant::now() >= owner.operation.deadline()
    {
        return fail(app, owner);
    }
    if let Some(workspace) = prepared.workspace {
        // A conflict handoff changes the purpose of the local view. Retire all
        // sealed native diff rows/selection before replacing its workspace.
        crate::repository_native_diff_native_v1::retire(app);
        app.workspace = Some(workspace);
        app.selected = 0;
        app.scroll = 0;
        app.hits.clear();
        app.repository_mode = crate::workspace::RepositoryMode::Overview;
        app.original_stash_recovery_v1 = None;
        app.overlay = None;
        app.form = None;
        app.form_dismissed = true;
        app.status =
            "Original conflict viewer loaded • read-only • no reconciliation/retry/drop/abort"
                .into();
    } else {
        let state = app
            .original_stash_recovery_v1
            .as_mut()
            .expect("owned state");
        state.pending = None;
        state.lines = prepared.lines;
        state.scroll = 0;
        app.status = "Original stash result observed • no retry/drop/reconciliation".into();
    }
    true
}
pub fn dispatch(app: &mut App, action: StashRecoveryActionV1) {
    match action {
        StashRecoveryActionV1::Back => {
            app.original_stash_recovery_v1 = None;
            app.original_stash_recovery_request_v1 = None;
            app.original_stash_recovery_cancel_requested_v1 = true;
            app.hits.clear();
        }
        StashRecoveryActionV1::Scroll(delta) => {
            if let Some(state) = app.original_stash_recovery_v1.as_mut() {
                state.scroll = state
                    .scroll
                    .saturating_add_signed(delta as isize)
                    .min(state.scroll_limit);
            }
        }
        StashRecoveryActionV1::Inspect | StashRecoveryActionV1::Conflict => {
            if app.loading[View::Repository.index()] {
                app.status = "Stash inspection busy • Back cancels original read".into();
                return;
            }
            let Some(failed) = actual_failed(app).cloned() else {
                app.status = "No genuine failed stash intent/source to inspect".into();
                return;
            };
            if app.original_stash_recovery_v1.is_none() {
                app.original_stash_recovery_v1 = Some(StashRecoveryStateV1 {
                    failed: failed.clone(),
                    pending: None,
                    lines: vec![],
                    scroll: 0,
                    scroll_limit: 0,
                });
            }
            app.original_stash_recovery_request_v1 =
                Some(Arc::new(OriginalStashRecoveryIntentV1 {
                    failed,
                    working: action == StashRecoveryActionV1::Conflict,
                }));
        }
    }
}
pub fn event(app: &mut App, event: Event) -> bool {
    if app.original_stash_recovery_v1.is_none() {
        return false;
    }
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
            KeyCode::Esc => Some(StashRecoveryActionV1::Back),
            KeyCode::Char('r') => Some(StashRecoveryActionV1::Inspect),
            KeyCode::Char('c') => Some(StashRecoveryActionV1::Conflict),
            KeyCode::Up => Some(StashRecoveryActionV1::Scroll(-1)),
            KeyCode::Down => Some(StashRecoveryActionV1::Scroll(1)),
            KeyCode::PageUp => Some(StashRecoveryActionV1::Scroll(-10)),
            KeyCode::PageDown => Some(StashRecoveryActionV1::Scroll(10)),
            _ => None,
        },
        Event::Mouse(mouse) => match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => app
                .hits
                .iter()
                .find(|hit| hit.rect.contains(Position::new(mouse.column, mouse.row)))
                .and_then(|hit| {
                    if let Action::StashRecoveryV1(action) = &hit.action {
                        Some(action.clone())
                    } else {
                        None
                    }
                }),
            MouseEventKind::ScrollUp => Some(StashRecoveryActionV1::Scroll(-3)),
            MouseEventKind::ScrollDown => Some(StashRecoveryActionV1::Scroll(3)),
            _ => None,
        },
        _ => None,
    };
    if let Some(action) = action {
        dispatch(app, action);
    }
    true
}
pub fn draw(frame: &mut Frame, app: &mut App) -> bool {
    let Some(state) = app.original_stash_recovery_v1.as_ref() else {
        return false;
    };
    let area = frame.area();
    let theme = app.theme();
    frame.render_widget(Clear, area);
    let inner = Block::default()
        .borders(Borders::ALL)
        .title(" Original stash result ")
        .inner(area);
    frame.render_widget(
        Block::default()
            .borders(Borders::ALL)
            .title(" Original stash result "),
        area,
    );
    let lines = if state.pending.is_some() {
        vec!["Reading original stash result… • Esc Back cancels read".into()]
    } else if state.lines.is_empty() {
        vec!["Unavailable • old observations hidden; R Inspect explicitly".into()]
    } else {
        state.lines.clone()
    };
    let wrapped = crate::ui::notice_lines(&lines.join("\n"), inner.width);
    let scroll_limit = wrapped
        .len()
        .saturating_sub(usize::from(inner.height.saturating_sub(4)));
    let scroll = state.scroll.min(scroll_limit);
    frame.render_widget(
        Paragraph::new(wrapped)
            .style(theme.style(SemanticToken::Normal))
            .scroll((scroll.min(u16::MAX as usize) as u16, 0)),
        Rect::new(
            inner.x,
            inner.y,
            inner.width,
            inner.height.saturating_sub(4),
        ),
    );
    frame.render_widget(
        Paragraph::new(crate::app::safe_text(&app.status)).style(theme.style(SemanticToken::Muted)),
        Rect::new(inner.x, inner.bottom().saturating_sub(4), inner.width, 1),
    );
    app.hits.clear();
    if let Some(state) = app.original_stash_recovery_v1.as_mut() {
        state.scroll_limit = scroll_limit;
        state.scroll = scroll;
    }
    for (row, label, action) in [
        (0, "[R Inspect]", StashRecoveryActionV1::Inspect),
        (1, "[C First conflict]", StashRecoveryActionV1::Conflict),
        (2, "[Esc Back]", StashRecoveryActionV1::Back),
    ] {
        let offset = if inner.width >= 44 {
            match row {
                0 => 0,
                1 => 12,
                _ => 32,
            }
        } else {
            0
        };
        let y = if inner.width >= 44 {
            inner.bottom().saturating_sub(1)
        } else {
            inner.bottom().saturating_sub(3) + row
        };
        let rect = Rect::new(
            inner.x + offset,
            y,
            inner.width.saturating_sub(offset).min(label.len() as u16),
            1,
        );
        frame.render_widget(
            Paragraph::new(label).style(theme.style(SemanticToken::Focus)),
            rect,
        );
        app.hits.push(HitRegion {
            rect,
            action: Action::StashRecoveryV1(action),
        });
    }
    true
}

#[cfg(test)]
#[path = "repository_stash_recovery_v1_tests.rs"]
pub(crate) mod tests;
