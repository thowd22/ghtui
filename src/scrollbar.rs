//! Bounded presentation geometry only: never a source, selection or permission.
//! Callers adapt requests to the same actions used by their keyboard handlers.
use crate::theme::{SemanticToken, Theme};
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::{layout::Rect, widgets::Paragraph, Frame};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScrollRequest {
    By(i32),
    To(usize),
}

#[derive(Default)]
pub(crate) struct Scrollbar {
    area: Rect,
    total: usize,
    visible: u16,
    position: usize,
    thumb_top: u16,
    thumb_height: u16,
    grab: Option<u16>,
}

impl Scrollbar {
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    pub(crate) fn cancel_drag(&mut self) {
        self.grab = None;
    }

    fn layout(&mut self, area: Rect, total: usize, visible: u16, position: usize) {
        if area.width != 1 || area.height < 3 || visible == 0 || total <= usize::from(visible) {
            self.clear();
            return;
        }
        if (self.area, self.total, self.visible) != (area, total, visible) {
            self.cancel_drag();
        }
        self.area = area;
        self.total = total;
        self.visible = visible;
        let max = total - usize::from(visible);
        self.position = position.min(max);
        let track = area.height - 2;
        // Overflow must leave room to move the thumb, even when only one row
        // is hidden. A single-cell track can still use its arrow controls.
        let maximum_thumb = track.saturating_sub(1).max(1);
        // u128 prevents usize overflow even for an extreme caller-provided
        // row count. All screen dimensions remain bounded by terminal geometry.
        self.thumb_height = (u128::from(track) * u128::from(visible))
            .div_ceil(total as u128)
            .clamp(1, u128::from(maximum_thumb)) as u16;
        self.thumb_top = area.y
            + 1
            + ((self.position as u128 * u128::from(track - self.thumb_height)) / max as u128)
                as u16;
    }

    pub(crate) fn draw(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        total: usize,
        visible: u16,
        position: usize,
        theme: Theme,
    ) {
        self.layout(area, total, visible, position);
        if self.area.is_empty() {
            return;
        }
        for y in self.area.y..self.area.bottom() {
            let (symbol, active) = if y == self.area.y {
                ("▲", self.position > 0)
            } else if y + 1 == self.area.bottom() {
                ("▼", self.position < self.total - usize::from(self.visible))
            } else if (self.thumb_top..self.thumb_top + self.thumb_height).contains(&y) {
                ("█", true)
            } else {
                ("│", false)
            };
            frame.render_widget(
                Paragraph::new(symbol).style(theme.style(if active {
                    SemanticToken::Focus
                } else {
                    SemanticToken::Muted
                })),
                Rect::new(self.area.x, y, 1, 1),
            );
        }
    }

    pub(crate) fn mouse(
        &mut self,
        kind: MouseEventKind,
        column: u16,
        row: u16,
    ) -> Option<ScrollRequest> {
        let inside = self.area.contains((column, row).into());
        match kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.cancel_drag();
                if !inside {
                    return None;
                }
                if row == self.area.y {
                    Some(ScrollRequest::By(-1))
                } else if row + 1 == self.area.bottom() {
                    Some(ScrollRequest::By(1))
                } else if row < self.thumb_top {
                    Some(ScrollRequest::By(-i32::from(self.visible)))
                } else if row >= self.thumb_top + self.thumb_height {
                    Some(ScrollRequest::By(i32::from(self.visible)))
                } else {
                    self.grab = Some(row - self.thumb_top);
                    None
                }
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                let grab = self.grab?;
                let travel = self.area.height - 2 - self.thumb_height;
                if travel == 0 {
                    return None;
                }
                // Continue an owned drag outside its column, but clamp at the
                // ends. A release outside also retires it; no sticky dragging.
                let top = row.saturating_sub(self.area.y + 1 + grab).min(travel);
                let max = self.total - usize::from(self.visible);
                Some(ScrollRequest::To(
                    ((u128::from(top) * max as u128 + u128::from(travel) / 2) / u128::from(travel))
                        as usize,
                ))
            }
            MouseEventKind::Up(MouseButton::Left) => {
                self.cancel_drag();
                None
            }
            MouseEventKind::ScrollDown if inside => Some(ScrollRequest::By(3)),
            MouseEventKind::ScrollUp if inside => Some(ScrollRequest::By(-3)),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "scrollbar_tests.rs"]
mod tests;
