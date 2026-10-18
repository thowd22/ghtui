//! SOURCE-ONLY private child of repository_catalogue_native. A source name is
//! local navigation intent, not authentication/access or a saved-source rebase.
use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

pub const SOURCE_CHANGE_TITLE_V1: &str = "Change repository source";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogueSourceChangeActionV1 {
    Back,
    DiscardDiscover,
}
struct PendingSourceChangeV1 {
    owner: Arc<CatalogueOwner>,
    original: Arc<CatalogueIntentSource>,
    requested: RepositoryCatalogueSource,
    editor: TextBuffer,
    kind: u8,
    editing: bool,
    focus: usize,
}
/// Private pending editor and original Arcs only. No rows/permissions copied,
/// no activation, old source guard repair or recovery Verify implementation.
#[derive(Default)]
pub struct CatalogueSourceTransitionV1 {
    pending: Option<PendingSourceChangeV1>,
    hits: Vec<(Rect, CatalogueSourceChangeActionV1)>,
    focus: usize,
}
impl CatalogueSourceTransitionV1 {
    pub fn active(&self) -> bool {
        self.pending.is_some()
    }
    pub fn retire(&mut self) {
        self.pending = None;
        self.hits.clear();
    }
    pub fn clear_layout(&mut self) {
        self.hits.clear();
    }
    pub fn key(&mut self, key: KeyEvent) -> Option<CatalogueSourceChangeActionV1> {
        if key.kind == KeyEventKind::Release || !self.active() {
            return None;
        }
        Some(match key.code {
            KeyCode::Esc => CatalogueSourceChangeActionV1::Back,
            KeyCode::Char('x') => CatalogueSourceChangeActionV1::DiscardDiscover,
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = 1 - self.focus.min(1);
                return None;
            }
            KeyCode::Enter => {
                if self.focus == 0 {
                    CatalogueSourceChangeActionV1::Back
                } else {
                    CatalogueSourceChangeActionV1::DiscardDiscover
                }
            }
            _ => return None,
        })
    }
    pub fn mouse(&self, mouse: MouseEvent) -> Option<CatalogueSourceChangeActionV1> {
        if mouse.kind != MouseEventKind::Down(MouseButton::Left) {
            return None;
        }
        self.hits.iter().rev().find_map(|(rect, action)| {
            rect.contains(Position::new(mouse.column, mouse.row))
                .then_some(*action)
        })
    }
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: crate::theme::Theme) {
        self.hits.clear();
        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title(SOURCE_CHANGE_TITLE_V1)
            .style(theme.base());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() || !self.active() {
            return;
        }
        let footer_rows = 1;
        frame.render_widget(Paragraph::new(
            "Different source selected.\nBack keeps the original choices, filter and pending editor.\nDiscard & Discover drops ONLY local choices/filter.\nSaved sets, pins and drafts stay unchanged.\nThe new source is freshly read; active remote scope is fenced.\nNo activation, clone, saved-name permission or old-row merge.\nRe-activate explicitly after a successful read."
        ).wrap(Wrap { trim: false }), Rect::new(inner.x, inner.y, inner.width,
            inner.height.saturating_sub(footer_rows)));
        let mut footer_x = inner.x;
        for (index, (label, action)) in [
            ("[Esc Back]", CatalogueSourceChangeActionV1::Back),
            (
                "[x Discard & Discover]",
                CatalogueSourceChangeActionV1::DiscardDiscover,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            let rect = Rect::new(
                footer_x,
                inner.bottom().saturating_sub(footer_rows),
                (label.len() as u16).min(inner.right().saturating_sub(footer_x)),
                1,
            );
            if rect.is_empty() {
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
            footer_x = rect.right().saturating_add(1);
        }
    }
}

fn same_declaration(left: &RepositoryCatalogueSource, right: &RepositoryCatalogueSource) -> bool {
    // Chooses a workflow only. The real factory still authenticates original
    // UID/owner/node/kind and exact old SQL CAS BEFORE any new reservation.
    match (left, right) {
        (
            RepositoryCatalogueSource::Organization(a),
            RepositoryCatalogueSource::Organization(b),
        )
        | (RepositoryCatalogueSource::PublicUser(a), RepositoryCatalogueSource::PublicUser(b)) => {
            a.eq_ignore_ascii_case(b)
        }
        (
            RepositoryCatalogueSource::AuthenticatedAccessible,
            RepositoryCatalogueSource::AuthenticatedAccessible,
        ) => true,
        _ => false,
    }
}
fn original_live(modal: &CatalogueModal) -> Result<Arc<CatalogueIntentSource>, String> {
    if modal.busy || modal.fenced || modal.discovery.has_recovery() {
        return Err("Original live source unavailable; no hidden retry or recovery Verify".into());
    }
    let original = modal
        .guard
        .clone()
        .ok_or("Choose an explicit Source before Retry")?;
    let workspace = modal
        .workspace
        .as_ref()
        .ok_or("Original live choices unavailable")?;
    if original.scope().host != modal.owner.host
        || original
            .source()
            .identity()
            .account_id
            .is_none_or(|id| id == 0)
        || modal.owner.expected.as_ref().is_some_and(|identity| {
            identity.host != original.scope().host
                || identity.account_id != original.source().identity().account_id
                || identity.account_id.is_none_or(|uid| uid == 0)
        })
        || workspace
            .source()
            .is_none_or(|source| !Arc::ptr_eq(source, original.source()))
    {
        return Err(
            "Original live source/choice owner changed; no rows or intent retargeted".into(),
        );
    }
    Ok(original)
}
/// Existing shared Reload work, not another producer or write stack. Event
/// snapshots have the established4096/128KiB bounds; all row mapping stays worker-side.
pub(super) fn queue_original_reload(
    modal: &mut CatalogueModal,
    limits: RepositoryCatalogueLimits,
) -> Result<(), String> {
    let original = original_live(modal)?;
    let choices = modal
        .workspace
        .as_ref()
        .ok_or("Original choices unavailable")?
        .snapshot_transient_choices()?;
    if !Arc::ptr_eq(choices.original(), original.source()) {
        return Err("Original source/choice snapshot changed before retry".into());
    }
    modal.owner = Arc::new(CatalogueOwner {
        host: modal.owner.host.clone(),
        expected: Some(original.source().identity().clone()),
    });
    modal.request = Some(CatalogueWork::Reload {
        owner: modal.owner.clone(),
        original,
        choices,
        limits,
    });
    modal.dialog = None;
    modal.busy = true;
    modal.status = "Re-reading original source • choices retained • re-activation required".into();
    modal.hits.clear();
    Ok(())
}
pub(super) fn retry(modal: &mut CatalogueModal) -> Result<(), String> {
    let limits = modal.discovery.budget.limits();
    queue_original_reload(modal, limits)
}
/// Called with the existing bounded Source editor. Back returns that SAME
/// TextBuffer, including caret/selection/focus; no reconstruction from a label.
pub(super) fn requested_source(
    kind: u8,
    editor: &TextBuffer,
) -> Result<RepositoryCatalogueSource, String> {
    if editor.text.len() > 100 || editor.text.chars().any(char::is_control) {
        return Err("Source owner needs a bounded safe single line".into());
    }
    let requested = match kind {
        0 => RepositoryCatalogueSource::Organization(editor.text.clone()),
        1 => RepositoryCatalogueSource::AuthenticatedAccessible,
        2 => RepositoryCatalogueSource::PublicUser(editor.text.clone()),
        _ => return Err("Unknown source kind; no request sent".into()),
    };
    requested
        .validate()
        .map_err(|_| "Explicit source name invalid; no request/reservation sent")?;
    Ok(requested)
}

pub(super) fn source_submit(
    modal: &mut CatalogueModal,
    kind: u8,
    editor: TextBuffer,
    editing: bool,
) -> Result<(), String> {
    let requested = requested_source(kind, &editor)?;
    if modal.busy || modal.fenced || modal.discovery.has_recovery() {
        return Err(
            "Original source unavailable; explicitly Discard retained choices first".into(),
        );
    }
    if modal.guard.is_none() && modal.workspace.is_none() {
        // Unchanged first-discovery path; no existing transient intent to lose.
        load_request(modal, requested, String::new());
        return Ok(());
    }
    let original = original_live(modal)?;
    if same_declaration(original.source().source(), &requested) {
        return retry(modal);
    }
    let workspace = modal
        .workspace
        .as_ref()
        .ok_or("Original workspace unavailable")?;
    let has_intent = workspace.has_transient_choices()
        || !workspace.query().is_empty()
        || !workspace.selected_ids().is_empty();
    if has_intent {
        // O(1) state checks and bounded editor/Arc moves; never score/clone rows.
        modal.source_transition.pending = Some(PendingSourceChangeV1 {
            owner: modal.owner.clone(),
            original,
            requested,
            editor,
            kind,
            editing,
            focus: modal.shell_focus,
        });
        modal.source_transition.focus = 0;
        modal.source_transition.clear_layout();
        modal.dialog = None;
        modal.hits.clear();
    } else {
        queue_different_source(modal, original, requested);
    }
    Ok(())
}
fn queue_different_source(
    modal: &mut CatalogueModal,
    original: Arc<CatalogueIntentSource>,
    source: RepositoryCatalogueSource,
) {
    // Deliberately NEW source intent. No original choices cross kinds/owners,
    // no old saved namespace written, no old source guard repaired or activated.
    modal.source_transition.retire();
    modal.avatars.clear();
    modal.owner = Arc::new(CatalogueOwner {
        host: modal.owner.host.clone(),
        expected: Some(original.source().identity().clone()),
    });
    if let Some(workspace) = &mut modal.workspace {
        workspace.invalidate();
    }
    modal.workspace = None;
    modal.guard = None;
    modal.dialog = None;
    modal.source = Some(source.clone());
    modal.request = Some(CatalogueWork::ChangeSource {
        owner: modal.owner.clone(),
        original,
        source,
        limits: modal.discovery.budget.limits(),
    });
    modal.busy = true;
    modal.status =
        "Source changed deliberately • old local choices dropped • re-activation required".into();
    modal.hits.clear();
}
pub(super) fn dispatch(app: &mut App, action: CatalogueSourceChangeActionV1) {
    let Some(modal) = &mut app.catalogue else {
        return;
    };
    if modal.busy || modal.fenced {
        return;
    }
    let Some(pending) = modal.source_transition.pending.take() else {
        return;
    };
    if !Arc::ptr_eq(&pending.owner, &modal.owner)
        || modal
            .guard
            .as_ref()
            .is_none_or(|guard| !Arc::ptr_eq(guard, &pending.original))
        || modal
            .workspace
            .as_ref()
            .and_then(CatalogueWorkspace::source)
            .is_none_or(|source| !Arc::ptr_eq(source, pending.original.source()))
    {
        // Detached pending action can never clear/retarget another live context.
        modal.source_transition.clear_layout();
        return;
    }
    match action {
        CatalogueSourceChangeActionV1::Back => {
            modal.dialog = Some(CatalogueDialog::Source {
                kind: pending.kind,
                editor: pending.editor,
                editing: pending.editing,
            });
            modal.shell_focus = pending.focus;
            modal.source_transition.clear_layout();
            modal.hits.clear();
        }
        CatalogueSourceChangeActionV1::DiscardDiscover => {
            queue_different_source(modal, pending.original, pending.requested);
        }
    }
}

#[cfg(test)]
#[path = "repository_catalogue_source_transition_v1_tests.rs"]
mod tests;
