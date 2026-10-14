//! Staged read-only component. Root owns editable source/set/mapping fields,
//! workers, storage and actual destination routing. No provider calls in drawing.
use super::{CatalogDestination, CatalogPreparedRow, CatalogState};
use crate::{
    app::safe_text,
    theme::{SemanticToken, Theme},
};
use crossterm::event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{layout::Rect, style::Modifier, widgets::Paragraph, Frame};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogAction {
    Source,
    Refresh,
    Filter,
    Select(u64),
    Pin(u64),
    SaveSet,
    ActivateSet,
    MapLocal(u64),
    Open(u64, CatalogDestination),
    Close,
}
#[derive(Debug, Clone)]
pub struct CatalogControl {
    pub label: String,
    pub shortcut: KeyCode,
    pub action: CatalogAction,
    pub enabled: bool,
}
#[derive(Debug, Clone)]
pub struct CatalogHit {
    pub rect: Rect,
    pub action: CatalogAction,
    pub enabled: bool,
    pub row_index: Option<usize>,
    pub control_index: Option<usize>,
}
#[derive(Debug, Clone, Default)]
pub struct CatalogControls {
    pub selected: usize,
    pub focused: usize,
    pub row_scroll: usize,
}
impl CatalogControls {
    pub fn controls(
        &self,
        state: &CatalogState,
        rows: &[CatalogPreparedRow],
        busy: bool,
    ) -> Vec<CatalogControl> {
        let row = rows.get(self.selected.min(rows.len().saturating_sub(1)));
        let available = row.filter(|row| row.current && !busy);
        let mut controls = vec![
            CatalogControl {
                label: "[s Source]".into(),
                shortcut: KeyCode::Char('s'),
                action: CatalogAction::Source,
                enabled: !busy,
            },
            CatalogControl {
                label: "[r Refresh/retry]".into(),
                shortcut: KeyCode::Char('r'),
                action: CatalogAction::Refresh,
                enabled: !busy,
            },
            CatalogControl {
                label: "[/ Filter]".into(),
                shortcut: KeyCode::Char('/'),
                action: CatalogAction::Filter,
                enabled: !busy,
            },
            CatalogControl {
                label: "[w Save selected set]".into(),
                shortcut: KeyCode::Char('w'),
                action: CatalogAction::SaveSet,
                enabled: !busy && !state.selected.is_empty(),
            },
            CatalogControl {
                label: "[a Activate saved set]".into(),
                shortcut: KeyCode::Char('a'),
                action: CatalogAction::ActivateSet,
                enabled: !busy && !state.intent.sets.is_empty(),
            },
        ];
        if let Some(row) = row {
            let id = row.id;
            controls.push(CatalogControl {
                label: format!(
                    "[Space {} repository]",
                    if state.selected.contains(&id) {
                        "Deselect"
                    } else {
                        "Select"
                    }
                ),
                shortcut: KeyCode::Char(' '),
                action: CatalogAction::Select(id),
                enabled: !busy && (available.is_some() || state.selected.contains(&id)),
            });
            controls.push(CatalogControl {
                label: format!(
                    "[p {} repository]",
                    if state.intent.pins.contains_key(&id) {
                        "Unpin"
                    } else {
                        "Pin"
                    }
                ),
                shortcut: KeyCode::Char('p'),
                action: CatalogAction::Pin(id),
                enabled: !busy && (available.is_some() || state.intent.pins.contains_key(&id)),
            });
            controls.push(CatalogControl {
                label: "[m Map existing local repo]".into(),
                shortcut: KeyCode::Char('m'),
                action: CatalogAction::MapLocal(id),
                enabled: available.is_some(),
            });
            for (key, label, destination) in [
                ('i', "PRs", CatalogDestination::PullRequests),
                ('c', "Actions", CatalogDestination::Actions),
            ] {
                controls.push(CatalogControl {
                    label: format!("[{key} Open {label}]"),
                    shortcut: KeyCode::Char(key),
                    action: CatalogAction::Open(id, destination),
                    enabled: available.is_some(),
                });
            }
            if let Some(branch) = &row.default_branch {
                controls.push(CatalogControl {
                    label: "[t Remote tree]".into(),
                    shortcut: KeyCode::Char('t'),
                    action: CatalogAction::Open(
                        id,
                        CatalogDestination::RemoteTree {
                            default_branch: branch.clone(),
                        },
                    ),
                    enabled: available.is_some(),
                });
            }
            if let Some(mapping) = state.intent.mappings.get(&id) {
                controls.push(CatalogControl {
                    label: "[o Local repository]".into(),
                    shortcut: KeyCode::Char('o'),
                    action: CatalogAction::Open(
                        id,
                        CatalogDestination::LocalRepository {
                            directory: mapping.directory.clone(),
                        },
                    ),
                    enabled: available.is_some(),
                });
            }
        }
        controls.push(CatalogControl {
            label: "[Esc Close]".into(),
            shortcut: KeyCode::Esc,
            action: CatalogAction::Close,
            enabled: true,
        });
        controls
    }
    /// Row movement/focus is local-only. All actual buttons share the descriptors
    /// used to generate click targets; Root dispatches the returned typed action.
    pub fn event_action(
        &mut self,
        event: &Event,
        row_count: usize,
        controls: &[CatalogControl],
        hits: &[CatalogHit],
    ) -> Option<CatalogAction> {
        self.selected = self.selected.min(row_count.saturating_sub(1));
        self.focused = self.focused.min(controls.len().saturating_sub(1));
        match event {
            Event::Key(key)
                if key.kind != KeyEventKind::Release
                    && !key
                        .modifiers
                        .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
            {
                match key.code {
                    KeyCode::Down | KeyCode::Char('j') => {
                        self.selected = (self.selected + 1).min(row_count.saturating_sub(1));
                        None
                    }
                    KeyCode::Up | KeyCode::Char('k') => {
                        self.selected = self.selected.saturating_sub(1);
                        None
                    }
                    KeyCode::Tab => {
                        self.focused = (self.focused + 1) % controls.len().max(1);
                        None
                    }
                    KeyCode::BackTab => {
                        self.focused = (self.focused + controls.len().saturating_sub(1))
                            % controls.len().max(1);
                        None
                    }
                    KeyCode::Enter => controls
                        .get(self.focused)
                        .filter(|control| control.enabled)
                        .map(|control| control.action.clone()),
                    _ => controls
                        .iter()
                        .position(|control| control.shortcut == key.code)
                        .and_then(|index| {
                            self.focused = index;
                            controls[index]
                                .enabled
                                .then(|| controls[index].action.clone())
                        }),
                }
            }
            Event::Mouse(mouse) if mouse.kind == MouseEventKind::Down(MouseButton::Left) => hits
                .iter()
                .find(|hit| hit.rect.contains((mouse.column, mouse.row).into()))
                .and_then(|hit| {
                    if let Some(index) = hit.row_index {
                        self.selected = index;
                    }
                    if let Some(index) = hit.control_index {
                        self.focused = index;
                    }
                    hit.enabled.then(|| hit.action.clone())
                }),
            Event::Mouse(mouse)
                if mouse.kind == MouseEventKind::ScrollDown
                    && hits.iter().any(|hit| {
                        hit.row_index.is_some()
                            && hit.rect.contains((mouse.column, mouse.row).into())
                    }) =>
            {
                self.selected = self
                    .selected
                    .saturating_add(3)
                    .min(row_count.saturating_sub(1));
                None
            }
            Event::Mouse(mouse)
                if mouse.kind == MouseEventKind::ScrollUp
                    && hits.iter().any(|hit| {
                        hit.row_index.is_some()
                            && hit.rect.contains((mouse.column, mouse.row).into())
                    }) =>
            {
                self.selected = self.selected.saturating_sub(3);
                None
            }
            _ => None,
        }
    }
}
pub fn draw_controls(
    frame: &mut Frame,
    area: Rect,
    controls: &[CatalogControl],
    focus: usize,
    theme: Theme,
) -> Vec<CatalogHit> {
    let first = focus.saturating_add(1).saturating_sub(area.height as usize);
    controls
        .iter()
        .enumerate()
        .skip(first)
        .take(area.height as usize)
        .map(|(index, control)| {
            let rect = Rect::new(area.x, area.y + (index - first) as u16, area.width, 1);
            let style = theme.style(if control.enabled {
                SemanticToken::Reference
            } else {
                SemanticToken::Muted
            });
            let style = if index == focus {
                style.add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                style
            };
            frame.render_widget(Paragraph::new(control.label.as_str()).style(style), rect);
            CatalogHit {
                rect,
                action: control.action.clone(),
                enabled: control.enabled,
                row_index: None,
                control_index: Some(index),
            }
        })
        .collect()
}
pub fn draw_rows(
    frame: &mut Frame,
    area: Rect,
    rows: &[CatalogPreparedRow],
    controls: &mut CatalogControls,
    busy: bool,
    theme: Theme,
) -> Vec<CatalogHit> {
    controls.selected = controls.selected.min(rows.len().saturating_sub(1));
    if controls.selected < controls.row_scroll {
        controls.row_scroll = controls.selected;
    }
    if controls.selected >= controls.row_scroll.saturating_add(area.height as usize) {
        controls.row_scroll = controls
            .selected
            .saturating_add(1)
            .saturating_sub(area.height as usize);
    }
    rows.iter()
        .enumerate()
        .skip(controls.row_scroll)
        .take(area.height as usize)
        .map(|(index, row)| {
            let id = row.id;
            let label = format!(
                "{} {} id:{id} {} {}",
                if row.selected { "[x]" } else { "[ ]" },
                row.name,
                if row.pinned { "PINNED" } else { "" },
                if row.current {
                    "current scoped row"
                } else {
                    "cached — not verified in partial refresh"
                }
            );
            let rect = Rect::new(
                area.x,
                area.y + (index - controls.row_scroll) as u16,
                area.width,
                1,
            );
            let style = theme.style(if row.current {
                SemanticToken::Normal
            } else {
                SemanticToken::Muted
            });
            let style = if index == controls.selected {
                style.add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                style
            };
            frame.render_widget(Paragraph::new(safe_text(&label)).style(style), rect);
            CatalogHit {
                rect,
                action: CatalogAction::Select(id),
                enabled: !busy && (row.current || row.selected),
                row_index: Some(index),
                control_index: None,
            }
        })
        .collect()
}
