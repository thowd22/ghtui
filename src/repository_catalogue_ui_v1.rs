//! Inventory presentation. Borrow immutable prepared rows, shared
//! editor/theme/actor fallback; no provider, SQL, fuzzy scoring or source cloning.
use crate::{
    identity::ListAvatar,
    repository_catalogue_workspace_v1::{
        CatalogueDestination, CatalogueFocus, CatalogueHit, CatalogueInput, CatalogueWorkspace,
    },
    theme::{SemanticToken, Theme},
};
use crossterm::event::{MouseButton, MouseEventKind};
use ratatui::{
    layout::{Position, Rect},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

impl CatalogueWorkspace {
    /// Root captures the entire modal even when this is None; no fall-through.
    pub fn mouse(&self, kind: MouseEventKind, column: u16, row: u16) -> Option<CatalogueInput> {
        let position = Position::new(column, row);
        match kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let delta = if kind == MouseEventKind::ScrollUp {
                    -3
                } else {
                    3
                };
                if self.list_area.contains(position) {
                    Some(CatalogueInput::Move(delta))
                } else if self.detail_area.contains(position) {
                    Some(CatalogueInput::DetailScroll(delta))
                } else {
                    None
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if self.query_area.contains(position) {
                    Some(CatalogueInput::QueryClick(usize::from(
                        column - self.query_area.x,
                    )))
                } else if self.detail_area.contains(position) {
                    Some(CatalogueInput::DetailsFocus)
                } else {
                    self.hits
                        .iter()
                        .find(|hit| {
                            hit.layout_epoch == self.layout_epoch && hit.rect.contains(position)
                        })
                        .map(|hit| hit.input.clone())
                }
            }
            _ => None,
        }
    }
    /// Geometry belongs ONLY to this draw. Root must clear_layout immediately on
    /// Resize/source change, before processing input from an old screen.
    pub fn draw(&mut self, frame: &mut Frame, area: Rect, theme: Theme) {
        self.draw_with_avatars(frame, area, theme, None);
    }
    pub fn draw_with_avatars(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: Theme,
        avatars: Option<&crate::repository_catalogue_avatars_v1::CatalogueAvatarAssets>,
    ) {
        self.draw_for_local_actor(frame, area, theme, avatars, false);
    }
    /// Presentation-purpose flag only; native dispatch independently refuses
    /// escape inputs. Genuine PR/Source/recovery wrappers are unchanged.
    pub(crate) fn draw_for_local_actor(
        &mut self,
        frame: &mut Frame,
        area: Rect,
        theme: Theme,
        avatars: Option<&crate::repository_catalogue_avatars_v1::CatalogueAvatarAssets>,
        local_actor: bool,
    ) {
        self.clear_layout();
        if area.is_empty() {
            return;
        }
        frame.render_widget(Clear, area);
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .title("Repositories • explicit discovery")
            .style(theme.base());
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.is_empty() {
            return;
        }
        let close = Rect::new(
            inner.right().saturating_sub(11).max(inner.x),
            inner.bottom().saturating_sub(1),
            11.min(inner.width),
            1,
        );
        self.button(frame, close, "[Esc Close]", CatalogueInput::Close, theme);
        let bottom = close.y;
        let mut y = inner.y;
        if local_actor && y < bottom {
            frame.render_widget(
                Paragraph::new("Choose repository for saved commit • navigation intent only")
                    .style(theme.style(SemanticToken::Reference)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
        }
        if let Some(model) = &self.frame {
            for line in model.header.iter() {
                if y >= bottom.saturating_sub(8) {
                    break;
                }
                frame.render_widget(
                    Paragraph::new(line.text.as_str()).style(theme.style(line.role)),
                    Rect::new(inner.x, y, inner.width, 1),
                );
                y += 1;
            }
        } else if y < bottom {
            let label = if self.source.is_some() {
                "Preparing explicit inventory • old rows/counts/hits hidden"
            } else {
                "Original source unavailable • private repositories hidden"
            };
            frame.render_widget(
                Paragraph::new(label).style(theme.style(SemanticToken::Pending)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
        }
        if y < bottom {
            let caption = if self.focus == CatalogueFocus::Query {
                "Filter focused: "
            } else {
                "Filter: "
            };
            let width = (caption.len() as u16).min(inner.width);
            frame.render_widget(
                Paragraph::new(caption).style(theme.style(SemanticToken::Focus)),
                Rect::new(inner.x, y, width, 1),
            );
            self.query_area = Rect::new(inner.x + width, y, inner.width - width, 1);
            self.query
                .ensure_visible(usize::from(self.query_area.width), 1);
            // One bounded512-byte editor field, not inventory/description work.
            frame.render_widget(
                Paragraph::new(crate::editor::display_line(
                    &self.query.text,
                    self.query.left_column,
                    usize::from(self.query_area.width),
                ))
                .style(theme.style(SemanticToken::Normal)),
                self.query_area,
            );
            if self.focus == CatalogueFocus::Query && self.query_area.width > 0 {
                let column = self
                    .query
                    .position()
                    .1
                    .saturating_sub(self.query.left_column)
                    .min(usize::from(self.query_area.width - 1));
                frame.set_cursor_position(Position::new(self.query_area.x + column as u16, y));
            }
            self.hits.push(CatalogueHit {
                rect: Rect::new(inner.x, y, inner.width, 1),
                input: CatalogueInput::QueryFocus,
                layout_epoch: self.layout_epoch,
            });
            y += 1;
        }
        let controls = [
            ("[s Source]", CatalogueInput::Source),
            ("[l Limits]", CatalogueInput::Limits),
            ("[m Load more]", CatalogueInput::LoadMore),
            ("[? Unresolved]", CatalogueInput::ExplainChoices),
            ("[x Discard unresolved]", CatalogueInput::DiscardChoices),
            ("[/ Filter]", CatalogueInput::QueryFocus),
            ("[Space Select]", CatalogueInput::Select),
            ("[p Pin]", CatalogueInput::Pin),
            ("[w Save set]", CatalogueInput::SaveSet),
            ("[a Activate set]", CatalogueInput::ActivateSet),
            ("[r Retry]", CatalogueInput::Retry),
            (
                "[Open My PRs]",
                CatalogueInput::Open(CatalogueDestination::MyPrs),
            ),
            (
                "[Open Actions]",
                CatalogueInput::Open(CatalogueDestination::Actions),
            ),
            (
                "[B Remote files]",
                CatalogueInput::Open(CatalogueDestination::RemoteRepository),
            ),
            ("[Ctrl-U Clear]", CatalogueInput::Clear),
            ("[PgUp]", CatalogueInput::Page(-1)),
            ("[PgDn]", CatalogueInput::Page(1)),
        ];
        let mut x = inner.x;
        let actor_controls = [
            ("[s Source]", CatalogueInput::Source),
            ("[/ Filter]", CatalogueInput::QueryFocus),
            ("[l Limits]", CatalogueInput::Limits),
            ("[m Load more]", CatalogueInput::LoadMore),
            ("[? Unresolved]", CatalogueInput::ExplainChoices),
            ("[x Discard unresolved]", CatalogueInput::DiscardChoices),
            ("[r Retry]", CatalogueInput::Retry),
            ("[Enter Use repository]", CatalogueInput::UseLocalActor),
            ("[Ctrl-U Clear]", CatalogueInput::Clear),
            ("[PgUp]", CatalogueInput::Page(-1)),
            ("[PgDn]", CatalogueInput::Page(1)),
        ];
        let visible: &[_] = if local_actor {
            &actor_controls
        } else {
            &controls
        };
        for (label, input) in visible {
            let width = (label.len() as u16).min(inner.width);
            if x > inner.x && x.saturating_add(width) > inner.right() {
                x = inner.x;
                y += 1;
            }
            if y >= bottom.saturating_sub(5) {
                break;
            }
            self.button(
                frame,
                Rect::new(x, y, width, 1),
                label,
                input.clone(),
                theme,
            );
            x += width + 1;
        }
        y += 1;
        let Some(model) = self.frame.clone() else {
            return;
        }; // O1 Arc only.
        if let Some(target) = &model.selected_target {
            if y < bottom {
                let selected = if self.selected.contains(&target.id()) {
                    "Selected: yes"
                } else {
                    "Selected: no"
                };
                let pinned = if self.pins.contains(&target.id()) {
                    "Pinned: yes"
                } else {
                    "Pinned: no"
                };
                let line = ratatui::text::Line::from(vec![
                    ratatui::text::Span::raw(selected),
                    ratatui::text::Span::raw(" • "),
                    ratatui::text::Span::raw(pinned),
                ]);
                frame.render_widget(
                    Paragraph::new(line).style(theme.style(SemanticToken::Changed)),
                    Rect::new(inner.x, y, inner.width, 1),
                );
                y += 1;
            }
        }
        let available = bottom.saturating_sub(y);
        if available == 0 {
            return;
        }
        let detail_height = (available / 2).max(1);
        let list_height = available
            .saturating_sub(detail_height + 1)
            .max(1)
            .min(available);
        self.page_size = usize::from(list_height);
        self.list_area = Rect::new(inner.x, y, inner.width, list_height);
        let selected_original = model.selected_original();
        let selected_row = model
            .rows
            .iter()
            .position(|row| Some(row.original_index) == selected_original)
            .unwrap_or(0);
        let start = selected_row.saturating_sub(self.page_size.saturating_sub(1));
        for (offset, row) in model
            .rows
            .iter()
            .skip(start)
            .take(self.page_size)
            .enumerate()
        {
            let rect = Rect::new(inner.x, y + offset as u16, inner.width, 1);
            frame.render_widget(
                Paragraph::new(row.label.as_str()).style(
                    if Some(row.original_index) == selected_original {
                        theme.selected(row.role)
                    } else {
                        theme.style(row.role)
                    },
                ),
                rect,
            );
            // Actor avatar fallback is prepared/shared; never new image/network IO.
            if let Some(actor) = &row.actor {
                if inner.width >= 50 {
                    frame.render_widget(
                        ListAvatar {
                            identity: actor,
                            image: avatars.and_then(|assets| {
                                self.source
                                    .as_ref()
                                    .and_then(|source| assets.image(source, row))
                            }),
                            theme,
                        },
                        Rect::new(inner.right() - 4, rect.y, 4, 1),
                    );
                }
            }
            self.hits.push(CatalogueHit {
                rect,
                input: CatalogueInput::Row(row.original_key.clone()),
                layout_epoch: self.layout_epoch,
            });
        }
        if model.matching_count() == 0 {
            frame.render_widget(
                Paragraph::new("No matching repositories • scope not broadened"),
                self.list_area,
            );
        }
        y += list_height;
        if y < bottom {
            let labels = [
                (
                    "[Ctrl-PgUp Detail]",
                    CatalogueInput::DetailScroll(-(detail_height as i32)),
                ),
                (
                    "[Ctrl-PgDn Detail]",
                    CatalogueInput::DetailScroll(detail_height as i32),
                ),
            ];
            let mut x = inner.x;
            for (label, input) in labels {
                let width = label.len() as u16;
                if x + width > inner.right() {
                    break;
                }
                self.button(frame, Rect::new(x, y, width, 1), label, input, theme);
                x += width + 1;
            }
            y += 1;
        }
        self.detail_area = Rect::new(inner.x, y, inner.width, bottom.saturating_sub(y));
        self.detail_size = usize::from(self.detail_area.height).max(1);
        self.detail_scroll = self
            .detail_scroll
            .min(model.detail.len().saturating_sub(self.detail_size));
        for (offset, line) in model
            .detail
            .iter()
            .skip(self.detail_scroll)
            .take(self.detail_size)
            .enumerate()
        {
            frame.render_widget(
                Paragraph::new(line.text.as_str()).style(theme.style(line.role)),
                Rect::new(inner.x, y + offset as u16, inner.width, 1),
            );
        }
    }
    fn button(
        &mut self,
        frame: &mut Frame,
        rect: Rect,
        label: &str,
        input: CatalogueInput,
        theme: Theme,
    ) {
        frame.render_widget(
            Paragraph::new(label).style(theme.style(SemanticToken::Focus)),
            rect,
        );
        self.hits.push(CatalogueHit {
            rect,
            input,
            layout_epoch: self.layout_epoch,
        });
    }
}
