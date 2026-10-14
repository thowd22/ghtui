//! Private child of repository_catalogue_native. One bounded local
//! dialog and original-scoped recovery slot; no draw/event transport or SQL.
use super::*;
use crate::{
    active_remote_scope_v1::RemoteScopeFenceV1,
    repository_catalogue_read_limits_dialog_v1::{
        CatalogueDiscoveryLimitsDialogV1, DiscoveryLimitsActionV1, DiscoveryLimitsPurposeV1,
    },
    repository_catalogue_read_limits_v1::CatalogueDiscoveryBudgetV1,
    repository_catalogue_reload_v1::reload_catalogue_intent_source_before,
    repository_catalogue_workspace_v1::CatalogueTransientChoiceSnapshotV1,
};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiscoveryNativeActionV1 {
    Recovery(CatalogueRecoveryActionV1),
    Limits,
    More,
    Limit(DiscoveryLimitsActionV1),
    ExplainChoices,
    DiscardChoices,
    CloseBack,
    DiscardAndClose,
    Close,
}
/// No public constructor, serde, old rows, permission or retry API. The OLD
/// source is intentionally stale after a failed deliberate reservation. A
/// future Verify-original-source protocol, not this slot, must recover it.
pub struct CatalogueReReadRecoveryV1 {
    pub(super) original: Arc<CatalogueIntentSource>,
    // Actual sealed failed-read reservation, never a reconstructed generation.
    // Retained only for a future explicit same-source recovery factory/CAS.
    pub(super) closed_current: Arc<CatalogueIntentSource>,
    pub(super) choices: Arc<CatalogueTransientChoiceSnapshotV1>,
}
impl std::fmt::Debug for CatalogueReReadRecoveryV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "CatalogueReReadRecoveryV1(<hidden original-scoped intent; not retry authority>)",
        )
    }
}
impl CatalogueReReadRecoveryV1 {
    pub(super) fn same_original(&self) -> bool {
        Arc::ptr_eq(self.original.source(), self.choices.original())
            && self.original.scope() == self.closed_current.scope()
            && self.original.intent().source() == self.closed_current.intent().source()
    }
}
#[derive(Default)]
pub struct CatalogueDiscoveryNativeV1 {
    pub budget: CatalogueDiscoveryBudgetV1,
    dialog: Option<CatalogueDiscoveryLimitsDialogV1>,
    pub(super) recovery: Option<Arc<CatalogueReReadRecoveryV1>>,
    pub(super) verification: Option<super::original_recovery_v1::RecoveryPreviewV1>,
    close_warning: bool,
    allow_close: bool,
    focus: usize,
    hits: Vec<(Rect, DiscoveryNativeActionV1)>,
}
impl CatalogueDiscoveryNativeV1 {
    pub fn active(&self) -> bool {
        self.dialog.is_some() || self.recovery.is_some() || self.close_warning
    }
    pub fn has_recovery(&self) -> bool {
        self.recovery.is_some()
    }
    pub fn warning_active(&self) -> bool {
        self.close_warning
    }
    pub(super) fn allow_close(&mut self) {
        self.allow_close = true;
    }
    pub fn clear_layout(&mut self) {
        self.hits.clear();
        if let Some(preview) = &mut self.verification {
            preview.clear_layout();
        }
        if let Some(dialog) = &mut self.dialog {
            dialog.clear_layout();
        }
    }
    pub fn retire(&mut self) {
        self.dialog = None;
        self.recovery = None;
        self.verification = None;
        self.close_warning = false;
        self.allow_close = false;
        self.clear_layout();
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<DiscoveryNativeActionV1> {
        if key.kind == KeyEventKind::Release {
            return None;
        }
        if let Some(dialog) = &mut self.dialog {
            return dialog.key(key).map(DiscoveryNativeActionV1::Limit);
        }
        if !self.close_warning {
            if let Some(preview) = &mut self.verification {
                return preview.key(key).map(DiscoveryNativeActionV1::Recovery);
            }
        }
        if !self.active() {
            return None;
        }
        Some(match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                let count = if self.close_warning { 2 } else { 3 };
                self.focus = if key.code == KeyCode::BackTab {
                    (self.focus + count - 1) % count
                } else {
                    (self.focus + 1) % count
                };
                return None;
            }
            KeyCode::Esc => {
                if self.close_warning {
                    DiscoveryNativeActionV1::CloseBack
                } else {
                    DiscoveryNativeActionV1::Close
                }
            }
            KeyCode::Char('v') if !self.close_warning => {
                DiscoveryNativeActionV1::Recovery(CatalogueRecoveryActionV1::Preview)
            }
            KeyCode::Char('x') => {
                if self.close_warning {
                    DiscoveryNativeActionV1::DiscardAndClose
                } else {
                    DiscoveryNativeActionV1::DiscardChoices
                }
            }
            KeyCode::Enter => {
                if self.close_warning {
                    if self.focus == 0 {
                        DiscoveryNativeActionV1::CloseBack
                    } else {
                        DiscoveryNativeActionV1::DiscardAndClose
                    }
                } else if self.focus == 0 {
                    DiscoveryNativeActionV1::Recovery(CatalogueRecoveryActionV1::Preview)
                } else if self.focus == 1 {
                    DiscoveryNativeActionV1::DiscardChoices
                } else {
                    DiscoveryNativeActionV1::Close
                }
            }
            _ => return None,
        })
    }
    pub fn mouse(&self, event: MouseEvent) -> Option<DiscoveryNativeActionV1> {
        if let Some(dialog) = &self.dialog {
            return dialog.mouse(event).map(DiscoveryNativeActionV1::Limit);
        }
        if !self.close_warning {
            if let Some(preview) = &self.verification {
                return preview.mouse(event).map(DiscoveryNativeActionV1::Recovery);
            }
        }
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return None;
        }
        self.hits.iter().rev().find_map(|(rect, action)| {
            rect.contains(Position::new(event.column, event.row))
                .then(|| action.clone())
        })
    }
    pub fn paste(&mut self, text: &str) {
        if let Some(dialog) = &mut self.dialog {
            dialog.paste(text);
        }
    }
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: crate::theme::Theme) {
        if let Some(dialog) = &mut self.dialog {
            dialog.draw(frame, area, theme);
            return;
        }
        if !self.close_warning {
            if let Some(preview) = &mut self.verification {
                preview.draw(frame, area, theme);
                return;
            }
        }
        self.hits.clear();
        frame.render_widget(Clear, area);
        let title = if self.close_warning {
            "Unsaved repository choices"
        } else {
            "Repository re-read unavailable"
        };
        let shell = Block::default()
            .borders(Borders::ALL)
            .title(title)
            .style(theme.base());
        let inner = shell.inner(area);
        frame.render_widget(shell, area);
        if inner.is_empty() {
            return;
        }
        let reason = if self.close_warning {
            "Local selections, pin edits or unresolved choices are retained.\nBack keeps them.\nDiscard & Close drops only local unsaved choices, never durable sets/pins/drafts.\nNothing will be activated or saved."
        } else {
            "Re-read endpoint unavailable after the same original account/owner closed. Previous rows, query and counts are hidden. One opaque original-scoped choice slot is retained; no retry, Save or Activate uses it. Discard explicitly before starting a new source. Use explicit Verify original source for a fresh read; it never releases old access."
        };
        let footer_rows = (if self.close_warning { 2 } else { 3 }).min(inner.height);
        frame.render_widget(
            Paragraph::new(reason).wrap(Wrap { trim: false }),
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(footer_rows),
            ),
        );
        let buttons = if self.close_warning {
            vec![
                ("[Esc Back]", DiscoveryNativeActionV1::CloseBack),
                (
                    "[x Discard & Close]",
                    DiscoveryNativeActionV1::DiscardAndClose,
                ),
            ]
        } else {
            vec![
                (
                    "[v Verify original source]",
                    DiscoveryNativeActionV1::Recovery(CatalogueRecoveryActionV1::Preview),
                ),
                (
                    "[x Discard retained choices]",
                    DiscoveryNativeActionV1::DiscardChoices,
                ),
                ("[Esc Close]", DiscoveryNativeActionV1::Close),
            ]
        };
        for (index, (label, action)) in buttons.into_iter().enumerate() {
            let rect = Rect::new(
                inner.x,
                inner.bottom().saturating_sub(footer_rows) + index as u16,
                inner.width,
                1,
            );
            if rect.y >= inner.bottom() {
                continue;
            }
            frame.render_widget(
                Paragraph::new(label).style(if index == self.focus {
                    theme.selected(crate::theme::SemanticToken::Reference)
                } else {
                    theme.base()
                }),
                rect,
            );
            self.hits.push((rect, action));
        }
    }
}

pub(super) fn prevent_close(modal: &mut CatalogueModal) -> bool {
    if modal.discovery.allow_close {
        modal.discovery.allow_close = false;
        return false;
    }
    if modal.discovery.recovery.is_some()
        || modal
            .workspace
            .as_ref()
            .is_some_and(CatalogueWorkspace::has_transient_choices)
    {
        modal.discovery.dialog = None;
        modal.discovery.close_warning = true;
        modal.discovery.focus = 0;
        modal.discovery.clear_layout();
        modal.hits.clear();
        return true;
    }
    false
}
pub(super) fn open_limits(modal: &mut CatalogueModal, larger: bool) -> Result<(), String> {
    if modal.discovery.has_recovery() {
        return Err(
            "Retained choices require explicit Discard; original source cannot retry".into(),
        );
    }
    let ready = modal.guard.is_some() && modal.workspace.is_some();
    if larger && !ready {
        return Err("Load more requires a genuine original source; choose Source first".into());
    }
    let budget = if larger {
        modal.discovery.budget.suggested_larger()
    } else {
        modal.discovery.budget
    };
    modal.discovery.dialog = Some(CatalogueDiscoveryLimitsDialogV1::new(
        budget,
        if ready {
            DiscoveryLimitsPurposeV1::ReRead
        } else {
            DiscoveryLimitsPurposeV1::ApplyOnly
        },
    ));
    Ok(())
}
pub(super) fn explain_choices(modal: &mut CatalogueModal) -> Result<(), String> {
    let reason = modal
        .workspace
        .as_ref()
        .ok_or("Original choices unavailable")?
        .unresolved_choices_notice();
    modal.dialog = Some(CatalogueDialog::Message { reason, scroll: 0 });
    Ok(())
}
pub(super) fn discard_choices(modal: &mut CatalogueModal) {
    modal.discovery.recovery = None;
    modal.discovery.verification = None;
    modal.owner = Arc::new(CatalogueOwner {
        host: modal.owner.host.clone(),
        expected: modal.owner.expected.clone(),
    });
    if let Some(workspace) = &mut modal.workspace {
        workspace.discard_unresolved_choices();
    }
    modal.discovery.close_warning = false;
    modal.status =
        "Unresolved local choices discarded explicitly • saved sets/pins unchanged".into();
}
pub(super) fn dispatch(app: &mut App, action: DiscoveryNativeActionV1) {
    if let DiscoveryNativeActionV1::Recovery(action) = action {
        super::original_recovery_v1::dispatch_recovery(app, action);
        return;
    }
    if action == DiscoveryNativeActionV1::Close {
        super::dispatch(app, CatalogueUiAction::Close);
        return;
    }
    if action == DiscoveryNativeActionV1::DiscardAndClose {
        if let Some(modal) = &mut app.catalogue {
            modal.discovery.retire();
            modal.discovery.allow_close = true;
        }
        super::dispatch(app, CatalogueUiAction::Close);
        return;
    }
    let Some(modal) = &mut app.catalogue else {
        return;
    };
    if modal.busy && action != DiscoveryNativeActionV1::CloseBack {
        return;
    }
    let result = (|| -> Result<(), String> {
        match action {
            DiscoveryNativeActionV1::Recovery(_) => unreachable!("handled before modal borrow"),
            DiscoveryNativeActionV1::Limits | DiscoveryNativeActionV1::More => {
                if modal.discovery.recovery.is_some() {
                    return Err("Retained choices need explicit Discard; stale original source cannot retry".into());
                }
                let can_reload = modal.guard.is_some() && modal.workspace.is_some();
                if action == DiscoveryNativeActionV1::More && !can_reload {
                    return Err(
                        "Load more requires a genuine original source; choose Source first".into(),
                    );
                }
                let budget = if action == DiscoveryNativeActionV1::More {
                    modal.discovery.budget.suggested_larger()
                } else {
                    modal.discovery.budget
                };
                modal.discovery.dialog = Some(CatalogueDiscoveryLimitsDialogV1::new(
                    budget,
                    if can_reload {
                        DiscoveryLimitsPurposeV1::ReRead
                    } else {
                        DiscoveryLimitsPurposeV1::ApplyOnly
                    },
                ));
            }
            DiscoveryNativeActionV1::Limit(action) => {
                let Some(dialog) = &mut modal.discovery.dialog else {
                    return Ok(());
                };
                match action {
                    DiscoveryLimitsActionV1::Apply => {
                        let Ok(budget) = dialog.budget() else {
                            return Ok(());
                        };
                        modal.discovery.budget = budget;
                        modal.discovery.dialog = None;
                        modal.status =
                            "Discovery limits applied locally • no request or save sent".into();
                    }
                    DiscoveryLimitsActionV1::Back => {
                        modal.discovery.dialog = None;
                    }
                    DiscoveryLimitsActionV1::ReRead => {
                        let Ok(budget) = dialog.budget() else {
                            return Ok(());
                        };
                        if dialog.purpose() != DiscoveryLimitsPurposeV1::ReRead {
                            return Ok(());
                        }
                        super::source_transition_v1::queue_original_reload(modal, budget.limits())?;
                        modal.discovery.budget = budget;
                        modal.discovery.dialog = None;
                    }
                    action => dialog.apply_local(&action),
                }
            }
            DiscoveryNativeActionV1::ExplainChoices => {
                let reason = modal
                    .workspace
                    .as_ref()
                    .ok_or("Original choices unavailable")?
                    .unresolved_choices_notice();
                modal.dialog = Some(CatalogueDialog::Message { reason, scroll: 0 });
            }
            DiscoveryNativeActionV1::DiscardChoices => {
                modal.discovery.recovery = None;
                modal.discovery.verification = None;
                modal.owner = Arc::new(CatalogueOwner {
                    host: modal.owner.host.clone(),
                    expected: modal.owner.expected.clone(),
                });
                if let Some(workspace) = &mut modal.workspace {
                    workspace.discard_unresolved_choices();
                }
                modal.discovery.close_warning = false;
                modal.status =
                    "Unresolved local choices discarded explicitly • saved sets/pins unchanged"
                        .into();
            }
            DiscoveryNativeActionV1::CloseBack => {
                modal.discovery.close_warning = false;
            }
            DiscoveryNativeActionV1::Close | DiscoveryNativeActionV1::DiscardAndClose => {
                unreachable!("handled before modal borrow")
            }
        }
        Ok(())
    })();
    if let Some(modal) = &mut app.catalogue {
        modal.hits.clear();
        if let Err(reason) = result {
            modal.dialog = Some(CatalogueDialog::Message { reason, scroll: 0 });
        }
    }
}

pub(super) fn execute_reload(
    owner: Arc<CatalogueOwner>,
    original: Arc<CatalogueIntentSource>,
    choices: Arc<CatalogueTransientChoiceSnapshotV1>,
    limits: RepositoryCatalogueLimits,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let deadline = std::time::Instant::now()
        .checked_add(limits.max_duration)
        .filter(|_| !limits.max_duration.is_zero())
        .ok_or("Original reread deadline invalid")?;
    if !Arc::ptr_eq(choices.original(), original.source()) {
        return Ok(hidden(owner));
    }
    // Mapping/bounds preflight is WORKER work and precedes Cache::open and gh.
    let choices_prepared = match choices.prepare(cancel) {
        Ok(value) => value,
        Err(_) => return Ok(prepared(owner, CataloguePreparedKind::Message(
            "Original choices preflight refused • intent retained • no request/reservation sent".into()))),
    };
    if cancel.is_cancelled() || std::time::Instant::now() >= deadline {
        return Ok(prepared(owner, CataloguePreparedKind::Message(
            "Original choices deadline/cancel refused • intent retained • no request/reservation sent".into(),
        )));
    }
    let mut cache = Cache::open(database).map_err(|_| "Original reread storage unavailable")?;
    let current = Arc::new(
        reload_catalogue_intent_source_before(
            provider, &mut cache, &original, &limits, deadline, cancel,
        )
        .map_err(|_| "Original reread proof unavailable; private rows must be hidden")?,
    );
    check_original_admission(deadline, cancel)?;
    if current.source().repositories().is_empty() && current.source().coverage().failure.is_some() {
        let mut view = prepared(
            owner,
            CataloguePreparedKind::ReloadUnavailable {
                recovery: Arc::new(CatalogueReReadRecoveryV1 {
                    original,
                    closed_current: current.clone(),
                    choices,
                }),
            },
        );
        view.verified_identity = Some(current.source().identity().clone());
        return Ok(view);
    }
    let restored = choices_prepared.restore(current.source().clone(), cancel)?;
    let query = restored.query().to_owned();
    let (mut workspace, mut job) = CatalogueWorkspace::new(current.source().clone())?;
    workspace.restore_observed_pins(current.intent())?;
    restored.install(&mut workspace)?;
    if !query.is_empty() {
        if let Some(CatalogueRequest::Prepare(next)) =
            workspace.apply(CatalogueInput::Paste(query))?
        {
            job = next;
        }
    }
    workspace.accept(job.prepare(cancel)?)?;
    // Frame/choice preparation is part of the ORIGINAL worker window too.
    // Pure admission only: no SQL, subprocess, or renewed proof after closure.
    check_original_admission(deadline, cancel)?;
    let mut view = prepared(
        owner,
        CataloguePreparedKind::Reloaded {
            guard: current.clone(),
            workspace: Box::new(workspace),
        },
    );
    view.verified_identity = Some(current.source().identity().clone());
    Ok(view)
}
fn check_original_admission(
    deadline: std::time::Instant,
    cancel: &CancellationToken,
) -> Result<(), String> {
    if cancel.is_cancelled() || std::time::Instant::now() >= deadline {
        Err(
            "Original reread cancelled/expired before prepared publication; private values hidden"
                .into(),
        )
    } else {
        Ok(())
    }
}
pub(super) fn accept(modal: &mut CatalogueModal, kind: CataloguePreparedKind) {
    match kind {
        CataloguePreparedKind::Reloaded { guard, workspace } => {
            modal.avatars.clear();
            modal.source_transition.retire();
            modal.owner = Arc::new(CatalogueOwner {
                host: modal.owner.host.clone(),
                expected: Some(guard.source().identity().clone()),
            });
            modal.guard = Some(guard);
            modal.workspace = Some(*workspace);
            modal.discovery.recovery = None;
            modal.discovery.verification = None;
            modal.status =
                "Fresh page-1 read • re-activation required • exact ID/node choices only".into();
        }
        CataloguePreparedKind::ReloadUnavailable { recovery } => {
            modal.avatars.clear();
            if let Some(workspace) = &mut modal.workspace {
                workspace.invalidate();
            }
            modal.workspace = None;
            modal.guard = None;
            modal.dialog = None;
            // The slot never repairs or rebases a mismatched original.
            modal.discovery.recovery = recovery.same_original().then_some(recovery);
            modal.discovery.verification = None;
            modal.discovery.close_warning = false;
            modal.status = "Re-read unavailable • previous values hidden • original choices retained, not retry authority".into();
        }
        _ => unreachable!("only dedicated reread outcomes"),
    }
}
/// MAIN pre-scheduling hook. Stops previous owners before the worker can reserve
/// deliberately; retains the tagged scope instead of default/personal fallback.
pub fn retire_for_work(app: &mut App, work: &CatalogueWork) -> Vec<View> {
    let original = match work {
        CatalogueWork::VerifyOriginal { recovery, .. } => Some(&recovery.original),
        CatalogueWork::Reload { original, .. } | CatalogueWork::ChangeSource { original, .. } => {
            Some(original)
        }
        CatalogueWork::Load { .. } if app.remote_scope.is_some() => None,
        _ => return vec![],
    };
    if !owns(app, work.owner()) {
        return vec![];
    }
    // Reload retires remote presentation even without an active scope. Local
    // repository and Working-only search remain independent.
    let local_search = crate::remote_scope_gate_v1::independent_local_search(app.search.as_ref());
    retire_remote_views(app);
    if app.view != View::Repository && !(app.view == View::Search && local_search) {
        app.context = "Original source unavailable • re-activate explicitly".into();
    }
    if app.remote_scope.is_none() {
        app.remote_scope = original
            .and_then(|original| {
                crate::active_remote_scope_v1::ActiveRemoteScopeV1::explicit_selection(
                    original.source(),
                    &[],
                )
                .ok()
            })
            .map(Arc::new);
    }
    if let Some(scope) = &mut app.remote_scope {
        Arc::make_mut(scope).fence(RemoteScopeFenceV1::SourceUnavailable);
    }
    // No old scope/source can silently survive a new reservation. Original
    // intent lives in the owned work/slot, never as a current access claim.
    app.remote_scope_source = None;
    [View::Inbox, View::Actions, View::Search, View::Dashboard]
        .into_iter()
        .filter(|view| *view != View::Search || !local_search)
        .collect()
}

#[cfg(test)]
#[path = "repository_catalogue_read_limits_native_v1_tests.rs"]
mod tests;
