//! Prepared role rendering and layout-derived read-only controls. No source
//! parsing, SQL, provider work, identity remapping or image lookup by login.
use crate::{
    actor_avatar_states_v1::ActorAvatarStatesV1,
    identity::{ActorIdentity, AvatarWidget, GitRole},
    providers::gh::LocalCommitRoleReport,
    repository_commit_actor_native_v1::{
        LocalActorActionV1, LocalCommitActorNativeV1, PreparedLocalActorReportV1,
    },
    theme::{SemanticToken, Theme},
};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalActorHitV1 {
    pub rect: Rect,
    pub action: LocalActorActionV1,
}
#[derive(Default)]
pub struct LocalActorLayoutV1 {
    hits: Vec<LocalActorHitV1>,
    detail: Rect,
}
impl LocalActorLayoutV1 {
    pub fn clear(&mut self) {
        self.hits.clear();
        self.detail = Rect::default();
    }
    pub fn hits(&self) -> &[LocalActorHitV1] {
        &self.hits
    }
    pub fn focused_action(&self, selected: usize) -> Option<LocalActorActionV1> {
        self.hits.get(selected).map(|hit| hit.action.clone())
    }
    pub fn mouse(&self, kind: MouseEventKind, column: u16, row: u16) -> Option<LocalActorActionV1> {
        let p = Position::new(column, row);
        match kind {
            MouseEventKind::Down(MouseButton::Left) => self
                .hits
                .iter()
                .find(|hit| hit.rect.contains(p))
                .map(|hit| hit.action.clone()),
            MouseEventKind::ScrollUp if self.detail.contains(p) => {
                Some(LocalActorActionV1::Scroll(-3))
            }
            MouseEventKind::ScrollDown if self.detail.contains(p) => {
                Some(LocalActorActionV1::Scroll(3))
            }
            _ => None,
        }
    }
    /// Root replaces ONLY an actual profile toolbar row, not graph/diff offsets.
    pub fn controls(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        state: &LocalCommitActorNativeV1,
        theme: Theme,
    ) {
        self.clear();
        if area.is_empty() || state.profile().is_none() {
            return;
        }
        let controls = [
            (
                "[R Choose repository]",
                LocalActorActionV1::ChooseRepository,
                false,
            ),
            (
                "[V Verify host report]",
                LocalActorActionV1::Verify,
                state.choice().is_none(),
            ),
            ("[X Clear host report]", LocalActorActionV1::Clear, false),
            (
                "[F7 Report details]",
                LocalActorActionV1::Details,
                state.report().is_none(),
            ),
        ];
        let mut x = area.x;
        let mut y = area.y;
        for (label, action, disabled) in controls {
            let width = (label.len() as u16).min(area.width);
            if x > area.x && x.saturating_add(width) > area.right() {
                x = area.x;
                y += 1;
            }
            if y >= area.bottom() {
                break;
            }
            let rect = Rect::new(x, y, width, 1);
            frame.render_widget(
                Paragraph::new(label).style(theme.style(if disabled {
                    SemanticToken::Muted
                } else {
                    SemanticToken::Reference
                })),
                rect,
            );
            self.hits.push(LocalActorHitV1 { rect, action });
            x += width + 1;
        }
    }
    /// Whole overlay capture belongs Root even if no model/hit remains. Reserve
    /// Back in tiny layouts, clear geometry before every resize/source change.
    pub fn details(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        state: &LocalCommitActorNativeV1,
        theme: Theme,
    ) {
        self.clear();
        if area.is_empty() {
            return;
        }
        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .title("Saved commit host report • read-only")
            .style(theme.base());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let available = if inner.is_empty() { area } else { inner };
        let back = Rect::new(
            available.right().saturating_sub(10).max(available.x),
            available.bottom().saturating_sub(1),
            available.width.min(10),
            1,
        );
        frame.render_widget(
            Paragraph::new("[Esc Back]").style(if state.detail_focus == 0 {
                theme
                    .style(SemanticToken::Focus)
                    .add_modifier(ratatui::style::Modifier::UNDERLINED)
            } else {
                theme.style(SemanticToken::Reference)
            }),
            back,
        );
        self.hits.push(LocalActorHitV1 {
            rect: back,
            action: LocalActorActionV1::Back,
        });
        self.detail = Rect::new(
            available.x,
            available.y,
            available.width,
            back.y.saturating_sub(available.y),
        );
        if let Some(report) = state.report() {
            for (offset, line) in report
                .detail()
                .iter()
                .skip(state.detail_scroll)
                .take(self.detail.height as usize)
                .enumerate()
            {
                frame.render_widget(
                    Paragraph::new(line.as_str()).style(theme.style(SemanticToken::Normal)),
                    Rect::new(
                        self.detail.x,
                        self.detail.y + offset as u16,
                        self.detail.width,
                        1,
                    ),
                );
            }
        } else {
            frame.render_widget(
                Paragraph::new("Original host report unavailable • local Git remains")
                    .style(theme.style(SemanticToken::Muted)),
                self.detail,
            );
        }
        let mut x = available.x;
        for (label, action) in [
            ("[↑ Earlier]", LocalActorActionV1::Scroll(-3)),
            ("[↓ Later]", LocalActorActionV1::Scroll(3)),
        ] {
            let width = crate::editor::display_width(label) as u16;
            if x + width <= back.x {
                let rect = Rect::new(x, back.y, width, 1);
                frame.render_widget(
                    Paragraph::new(label).style(if state.detail_focus == self.hits.len() {
                        theme
                            .style(SemanticToken::Focus)
                            .add_modifier(ratatui::style::Modifier::UNDERLINED)
                    } else {
                        theme.style(SemanticToken::Reference)
                    }),
                    rect,
                );
                self.hits.push(LocalActorHitV1 { rect, action });
                x += width + 1;
            }
        }
    }
}
pub fn profile_key(code: KeyCode, modifiers: KeyModifiers) -> Option<LocalActorActionV1> {
    if modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) {
        return None;
    }
    match code {
        KeyCode::Char('R') => Some(LocalActorActionV1::ChooseRepository),
        KeyCode::Char('V') => Some(LocalActorActionV1::Verify),
        KeyCode::Char('X') => Some(LocalActorActionV1::Clear),
        KeyCode::F(7) => Some(LocalActorActionV1::Details),
        _ => None,
    }
}
pub fn detail_key(code: KeyCode) -> Option<LocalActorActionV1> {
    match code {
        KeyCode::Esc => Some(LocalActorActionV1::Back),
        KeyCode::Tab => Some(LocalActorActionV1::Focus(1)),
        KeyCode::BackTab => Some(LocalActorActionV1::Focus(-1)),
        KeyCode::Up | KeyCode::Char('k') => Some(LocalActorActionV1::Scroll(-3)),
        KeyCode::Down | KeyCode::Char('j') => Some(LocalActorActionV1::Scroll(3)),
        KeyCode::PageUp => Some(LocalActorActionV1::Scroll(-12)),
        KeyCode::PageDown => Some(LocalActorActionV1::Scroll(12)),
        KeyCode::Home => Some(LocalActorActionV1::Scroll(i32::MIN)),
        KeyCode::End => Some(LocalActorActionV1::Scroll(i32::MAX)),
        _ => None,
    }
}
/// Borrow exact role identity/label. Generic Actor DTOs and image-by-login are
/// deliberately absent: GitRole/raw local metadata survive an accepted report.
pub fn draw_role(
    frame: &mut Frame,
    area: Rect,
    role: GitRole,
    local: &ActorIdentity,
    report: Option<&PreparedLocalActorReportV1>,
    images: &ActorAvatarStatesV1,
    theme: Theme,
) {
    if area.is_empty() {
        return;
    }
    let reported = report.and_then(|r| match r.role(role) {
        LocalCommitRoleReport::Reported(actor) => Some(actor.as_ref()),
        _ => None,
    });
    let identity = reported.map_or(local, |actor| actor.display());
    let image = reported.and_then(|actor| images.ready(actor.display()));
    let avatar_width = area.width.min(4);
    frame.render_widget(
        AvatarWidget {
            identity,
            image,
            theme,
        },
        Rect::new(area.x, area.y, avatar_width, area.height.min(2)),
    );
    if area.width <= avatar_width {
        return;
    }
    let text = Rect::new(
        area.x + avatar_width + 1,
        area.y,
        area.width.saturating_sub(avatar_width + 1),
        area.height,
    );
    if let Some(actor) = reported {
        frame.render_widget(
            Paragraph::new(actor.label()).style(theme.style(SemanticToken::Actor)),
            text,
        );
    } else {
        frame.render_widget(
            Paragraph::new(local.label()).style(theme.style(SemanticToken::Actor)),
            text,
        );
    }
}

#[cfg(test)]
#[path = "repository_commit_actor_ui_v1_tests.rs"]
mod tests;
