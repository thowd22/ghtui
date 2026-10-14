//! Owned modal presentation/input only. Prepared rows are borrowed; no provider,
//! SQL, source parsing, inventory sorting or approval restoration occurs here.
use crate::{
    editor::{display_line, TextBuffer},
    repository_catalogue_native::{CatalogueDialog, CatalogueModal, CatalogueUiAction},
    repository_catalogue_workspace_v1::{CatalogueFocus, CatalogueInput, CATALOGUE_QUERY_BYTES},
    theme::SemanticToken,
};
use crossterm::event::{
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    layout::{Position, Rect},
    style::Modifier,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame,
};

const TITLE: &str = "Repositories • explicit discovery";
const OWNER_BYTES: usize = 100;
const SET_BYTES: usize = 128;

/// Root calls on Resize/source retirement BEFORE processing another mouse event.
pub fn clear_layout(modal: &mut CatalogueModal) {
    modal.hits.clear();
    modal.discovery.clear_layout();
    modal.source_transition.clear_layout();
    modal.editor_area = Rect::default();
    modal.dialog_area = Rect::default();
    if let Some(workspace) = &mut modal.workspace {
        workspace.clear_layout();
    }
}

pub fn draw(frame: &mut Frame, area: Rect, modal: &mut CatalogueModal) {
    clear_layout(modal);
    let focus = modal.shell_focus;
    if area.is_empty() {
        return;
    }
    if !modal.busy && !modal.fenced && modal.source_transition.active() {
        modal.source_transition.draw(frame, area, modal.theme);
        return;
    }
    if !modal.fenced
        && (modal.discovery.warning_active() || (!modal.busy && modal.discovery.active()))
    {
        modal.discovery.draw(frame, area, modal.theme);
        return;
    }
    if !modal.busy && !modal.fenced && modal.dialog.is_none() {
        if let Some(workspace) = &mut modal.workspace {
            workspace.draw_for_local_actor(
                frame,
                area,
                modal.theme,
                Some(&modal.avatars),
                modal.local_actor_purpose.is_some(),
            );
            workspace_footer(frame, area, modal);
            return;
        }
    }
    frame.render_widget(Clear, area);
    let shell = Block::default()
        .borders(Borders::ALL)
        .title(TITLE)
        .style(modal.theme.base());
    let inner = shell.inner(area);
    frame.render_widget(shell, area);
    if inner.is_empty() {
        return;
    }
    if modal.fenced || modal.busy {
        modal.shell_focus %= 2;
        let notice = if modal.fenced {
            "private repositories hidden • original saved intent/drafts retained"
        } else {
            "Original worker pending • private rows/counts hidden • Close to cancel"
        };
        frame.render_widget(
            Paragraph::new(notice)
                .wrap(Wrap { trim: false })
                .style(modal.theme.style(SemanticToken::Pending)),
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(1),
            ),
        );
        footer(
            frame,
            inner,
            modal,
            &[
                (
                    "[Esc Close]",
                    CatalogueUiAction::Close,
                    focus.is_multiple_of(2),
                ),
                ("[q Quit]", CatalogueUiAction::Quit, focus % 2 == 1),
            ],
        );
        return;
    }
    let Some(mut dialog) = modal.dialog.take() else {
        modal.shell_focus %= 3;
        let prompt = if modal.local_actor_purpose.is_some() {
            "Choose repository for saved commit • navigation intent only\nChoose an explicit source • no automatic discovery or clone"
        } else {
            "Choose an explicit source • no automatic discovery or clone"
        };
        frame.render_widget(
            Paragraph::new(prompt).wrap(Wrap { trim: false }),
            Rect::new(
                inner.x,
                inner.y,
                inner.width,
                inner.height.saturating_sub(2),
            ),
        );
        footer(
            frame,
            inner,
            modal,
            &[
                (
                    "[s Source]",
                    CatalogueUiAction::Source,
                    focus.is_multiple_of(3),
                ),
                ("[Esc Close]", CatalogueUiAction::Close, focus % 3 == 1),
                ("[q Quit]", CatalogueUiAction::Quit, focus % 3 == 2),
            ],
        );
        return;
    };
    let (title, desired_height) = match &dialog {
        CatalogueDialog::Source { .. } => ("Repository discovery source", 12),
        CatalogueDialog::Save { .. } => ("Save repository set", 8),
        CatalogueDialog::Activate { .. } => ("Activate repository set", 20),
        CatalogueDialog::Message { .. } => ("Action unavailable • explanation only", 16),
    };
    let width = inner.width.min(76);
    let height = inner.height.min(desired_height);
    modal.dialog_area = Rect::new(
        inner.x + (inner.width - width) / 2,
        inner.y + (inner.height - height) / 2,
        width,
        height,
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .style(modal.theme.base());
    let content = block.inner(modal.dialog_area);
    frame.render_widget(block, modal.dialog_area);
    if !content.is_empty() {
        match &mut dialog {
            CatalogueDialog::Source {
                kind,
                editor,
                editing,
            } => {
                let mut y = content.y;
                for (label, value) in [
                    ("[o Organization]", 0),
                    ("[u My accessible repositories]", 1),
                    ("[p Public user]", 2),
                ] {
                    let row = Rect::new(content.x, y, content.width, 1);
                    button(
                        frame,
                        row,
                        modal,
                        label,
                        CatalogueUiAction::Kind(value),
                        !*editing && focus == 0 && *kind == value,
                    );
                    y += 1;
                    if y >= content.bottom().saturating_sub(2) {
                        break;
                    }
                }
                if y < content.bottom().saturating_sub(1) {
                    if *kind == 1 {
                        frame.render_widget(
                            Paragraph::new("Owner not used for My accessible repositories")
                                .style(modal.theme.style(SemanticToken::Muted)),
                            Rect::new(content.x, y, content.width, 1),
                        );
                    } else {
                        editor_row(
                            frame,
                            Rect::new(content.x, y, content.width, 1),
                            modal,
                            "Owner: ",
                            editor,
                            *editing,
                            Some(CatalogueUiAction::Edit),
                        );
                    }
                }
                if modal.status.starts_with("Invalid source")
                    && y.saturating_add(1) < content.bottom().saturating_sub(2)
                {
                    frame.render_widget(
                        Paragraph::new(modal.status.as_str())
                            .wrap(Wrap { trim: false })
                            .style(modal.theme.style(SemanticToken::Failure)),
                        Rect::new(
                            content.x,
                            y + 1,
                            content.width,
                            content.bottom().saturating_sub(y + 3),
                        ),
                    );
                }
                if content.height > 2 {
                    button(
                        frame,
                        Rect::new(content.x, content.bottom() - 2, content.width.min(12), 1),
                        modal,
                        "[F6 Limits]",
                        CatalogueUiAction::Discovery(
                            crate::repository_catalogue_native::DiscoveryNativeActionV1::Limits,
                        ),
                        false,
                    );
                }
                footer(
                    frame,
                    content,
                    modal,
                    &[
                        ("[Enter Discover]", CatalogueUiAction::Submit, focus == 2),
                        ("[Esc Close]", CatalogueUiAction::DialogClose, focus == 3),
                        ("[q Quit]", CatalogueUiAction::Quit, focus == 4),
                    ],
                );
            }
            CatalogueDialog::Save { editor } => {
                frame.render_widget(
                    Paragraph::new("Save explicit observed originals; unseen members are kept"),
                    Rect::new(content.x, content.y, content.width, 1),
                );
                if content.height > 2 {
                    editor_row(
                        frame,
                        Rect::new(content.x, content.y + 1, content.width, 1),
                        modal,
                        "Set name: ",
                        editor,
                        focus == 0,
                        None,
                    );
                }
                footer(
                    frame,
                    content,
                    modal,
                    &[
                        ("[Enter Save]", CatalogueUiAction::Submit, focus == 1),
                        ("[Esc Close]", CatalogueUiAction::DialogClose, focus == 2),
                        ("[q Quit]", CatalogueUiAction::Quit, focus == 3),
                    ],
                );
            }
            CatalogueDialog::Activate {
                names,
                selected,
                top,
            } => {
                frame.render_widget(
                    Paragraph::new(
                        "Explicit activation verifies original IDs; never all repositories",
                    ),
                    Rect::new(content.x, content.y, content.width, 1),
                );
                let rows = content.height.saturating_sub(2).max(1);
                *top = (*top).min(names.len().saturating_sub(1));
                for (offset, name) in names.iter().enumerate().skip(*top).take(usize::from(rows)) {
                    let y = content.y + 1 + (offset - *top) as u16;
                    if y >= content.bottom().saturating_sub(1) {
                        break;
                    }
                    let rect = Rect::new(content.x, y, content.width, 1);
                    let style = if offset == *selected {
                        modal.theme.selected(SemanticToken::Reference)
                    } else {
                        modal.theme.style(SemanticToken::Normal)
                    };
                    frame.render_widget(
                        Paragraph::new(Line::from(vec![
                            Span::raw(if offset == *selected { "> " } else { "  " }),
                            Span::raw(name.as_str()),
                        ]))
                        .style(style),
                        rect,
                    );
                    modal
                        .hits
                        .push((rect, CatalogueUiAction::DialogItem(offset)));
                }
                let page = usize::from(rows);
                footer(
                    frame,
                    content,
                    modal,
                    &[
                        ("[Enter Activate]", CatalogueUiAction::Submit, focus == 1),
                        (
                            "[PgUp]",
                            CatalogueUiAction::DialogItem(selected.saturating_sub(page)),
                            false,
                        ),
                        (
                            "[PgDn]",
                            CatalogueUiAction::DialogItem(
                                selected
                                    .saturating_add(page)
                                    .min(names.len().saturating_sub(1)),
                            ),
                            false,
                        ),
                        ("[Esc Close]", CatalogueUiAction::DialogClose, focus == 2),
                        ("[q Quit]", CatalogueUiAction::Quit, focus == 3),
                    ],
                );
            }
            CatalogueDialog::Message { reason, scroll } => {
                let page = dialog_rows(modal);
                frame.render_widget(
                    Paragraph::new(reason.as_str())
                        .wrap(Wrap { trim: false })
                        .scroll(((*scroll).min(u16::MAX as usize) as u16, 0)),
                    Rect::new(
                        content.x,
                        content.y,
                        content.width,
                        content.height.saturating_sub(1),
                    ),
                );
                footer(
                    frame,
                    content,
                    modal,
                    &[
                        ("[PgUp]", CatalogueUiAction::DialogScroll(-page), false),
                        ("[PgDn]", CatalogueUiAction::DialogScroll(page), false),
                        (
                            "[Esc Close]",
                            CatalogueUiAction::DialogClose,
                            focus.is_multiple_of(2),
                        ),
                        ("[q Quit]", CatalogueUiAction::Quit, focus % 2 == 1),
                    ],
                );
            }
        }
    }
    modal.dialog = Some(dialog);
}

fn button(
    frame: &mut Frame,
    rect: Rect,
    modal: &mut CatalogueModal,
    label: &str,
    action: CatalogueUiAction,
    focused: bool,
) {
    let rect = Rect::new(
        rect.x,
        rect.y,
        rect.width.min(label.len() as u16),
        rect.height.min(1),
    );
    if rect.is_empty() {
        return;
    }
    // Focus's semantic palette may underline by default. Only the actual
    // keyboard target may carry that navigation marker on a control.
    let mut style = modal
        .theme
        .style(SemanticToken::Focus)
        .remove_modifier(Modifier::UNDERLINED);
    if focused {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    frame.render_widget(Paragraph::new(label).style(style), rect);
    modal.hits.push((rect, action));
}
fn footer(
    frame: &mut Frame,
    area: Rect,
    modal: &mut CatalogueModal,
    buttons: &[(&str, CatalogueUiAction, bool)],
) {
    let mut x = area.x;
    let y = area.bottom().saturating_sub(1);
    for (label, action, focused) in buttons {
        if x >= area.right() {
            break;
        }
        let width = (label.len() as u16).min(area.right() - x);
        button(
            frame,
            Rect::new(x, y, width, 1),
            modal,
            label,
            action.clone(),
            *focused,
        );
        x += width + 1;
    }
}
fn workspace_footer(frame: &mut Frame, area: Rect, modal: &mut CatalogueModal) {
    let inner = Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    if inner.is_empty() {
        return;
    }
    let close = 11.min(inner.width);
    let width = 8.min(inner.width.saturating_sub(close + 1));
    let x = inner.right().saturating_sub(close + 1 + width);
    let y = inner.bottom().saturating_sub(1);
    if x > inner.x {
        frame.render_widget(
            Paragraph::new(modal.status.as_str()).style(modal.theme.style(SemanticToken::Changed)),
            Rect::new(inner.x, y, x - inner.x - 1, 1),
        );
    }
    button(
        frame,
        Rect::new(x, y, width, 1),
        modal,
        "[q Quit]",
        CatalogueUiAction::Quit,
        false,
    );
}
fn editor_row(
    frame: &mut Frame,
    area: Rect,
    modal: &mut CatalogueModal,
    caption: &str,
    editor: &mut TextBuffer,
    focused: bool,
    caption_action: Option<CatalogueUiAction>,
) {
    let width = (caption.len() as u16).min(area.width);
    let caption_rect = Rect::new(area.x, area.y, width, 1);
    frame.render_widget(
        Paragraph::new(caption).style(modal.theme.style(SemanticToken::Focus)),
        caption_rect,
    );
    modal.hits.push((
        caption_rect,
        caption_action.unwrap_or(CatalogueUiAction::EditorClick {
            column: 0,
            extend: false,
        }),
    ));
    modal.editor_area = Rect::new(area.x + width, area.y, area.width - width, 1);
    editor.ensure_visible(usize::from(modal.editor_area.width), 1);
    frame.render_widget(
        Paragraph::new(display_line(
            &editor.text,
            editor.left_column,
            usize::from(modal.editor_area.width),
        ))
        .style(modal.theme.style(SemanticToken::Normal)),
        modal.editor_area,
    );
    if focused && modal.editor_area.width > 0 {
        let column = editor
            .position()
            .1
            .saturating_sub(editor.left_column)
            .min(usize::from(modal.editor_area.width - 1));
        frame.set_cursor_position(Position::new(modal.editor_area.x + column as u16, area.y));
    }
}
fn dialog_rows(modal: &CatalogueModal) -> i32 {
    i32::from(modal.dialog_area.height.saturating_sub(4).max(1))
}
fn input_action(input: CatalogueInput) -> CatalogueUiAction {
    if input == CatalogueInput::Close {
        CatalogueUiAction::Close
    } else if input == CatalogueInput::UseLocalActor {
        CatalogueUiAction::UseLocalActor
    } else {
        CatalogueUiAction::Input(input)
    }
}

pub fn mouse(modal: &CatalogueModal, event: MouseEvent) -> Option<CatalogueUiAction> {
    if !modal.busy && !modal.fenced && modal.source_transition.active() {
        return modal
            .source_transition
            .mouse(event)
            .map(CatalogueUiAction::SourceChange);
    }
    if !modal.fenced
        && (modal.discovery.warning_active() || (!modal.busy && modal.discovery.active()))
    {
        return modal
            .discovery
            .mouse(event)
            .map(CatalogueUiAction::Discovery);
    }
    let position = Position::new(event.column, event.row);
    if matches!(
        event.kind,
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left)
    ) {
        if !modal.busy
            && !modal.fenced
            && modal.dialog.is_some()
            && modal.editor_area.contains(position)
        {
            return Some(CatalogueUiAction::EditorClick {
                column: usize::from(event.column - modal.editor_area.x),
                extend: event.modifiers.contains(KeyModifiers::SHIFT)
                    || event.kind == MouseEventKind::Drag(MouseButton::Left),
            });
        }
        if event.kind != MouseEventKind::Down(MouseButton::Left) {
            return None; // Drag selection cannot activate a neighbouring button.
        }
        if let Some((_, action)) = modal.hits.iter().find(|(rect, _)| rect.contains(position)) {
            return Some(action.clone());
        }
    }
    if modal.busy || modal.fenced {
        return None;
    }
    if modal.dialog.is_some() {
        if modal.dialog_area.contains(position) {
            let delta = match event.kind {
                MouseEventKind::ScrollUp => -3,
                MouseEventKind::ScrollDown => 3,
                _ => return None,
            };
            if matches!(
                modal.dialog,
                Some(CatalogueDialog::Activate { .. } | CatalogueDialog::Message { .. })
            ) {
                return Some(CatalogueUiAction::DialogScroll(delta));
            }
        }
        return None; // NEVER delegate hidden workspace geometry through a dialog.
    }
    modal
        .workspace
        .as_ref()?
        .mouse(event.kind, event.column, event.row)
        .map(input_action)
}

pub fn key(modal: &mut CatalogueModal, event: KeyEvent) -> Option<CatalogueUiAction> {
    if event.kind == KeyEventKind::Release {
        return None;
    }
    if !modal.busy && !modal.fenced && modal.source_transition.active() {
        return modal
            .source_transition
            .key(event)
            .map(CatalogueUiAction::SourceChange);
    }
    if !modal.fenced
        && (modal.discovery.warning_active() || (!modal.busy && modal.discovery.active()))
    {
        return modal.discovery.key(event).map(CatalogueUiAction::Discovery);
    }
    if !modal.busy && !modal.fenced && event.code == KeyCode::F(6) {
        return Some(CatalogueUiAction::Discovery(
            crate::repository_catalogue_native::DiscoveryNativeActionV1::Limits,
        ));
    }
    let control = event.modifiers.contains(KeyModifiers::CONTROL);
    let shift = event.modifiers.contains(KeyModifiers::SHIFT);
    if control && matches!(event.code, KeyCode::Char('c' | 'q')) {
        return Some(CatalogueUiAction::Quit);
    }
    if event.code == KeyCode::Esc {
        return Some(if modal.dialog.is_some() && !modal.busy && !modal.fenced {
            CatalogueUiAction::DialogClose
        } else {
            CatalogueUiAction::Close
        });
    }
    if modal.busy || modal.fenced {
        return shell_key(modal, event, true);
    }
    let rows = dialog_rows(modal) as usize;
    if let Some(dialog) = &mut modal.dialog {
        match dialog {
            CatalogueDialog::Source {
                kind,
                editor,
                editing,
            } => {
                if matches!(event.code, KeyCode::Tab | KeyCode::BackTab) {
                    let backwards = shift || event.code == KeyCode::BackTab;
                    let current = if *editing {
                        1
                    } else {
                        modal.shell_focus.min(4)
                    };
                    modal.shell_focus = if backwards {
                        (current + 4) % 5
                    } else {
                        (current + 1) % 5
                    };
                    if *kind == 1 && modal.shell_focus == 1 {
                        modal.shell_focus = if backwards { 0 } else { 2 };
                    }
                    *editing = modal.shell_focus == 1;
                    let edit = *editing;
                    clear_layout(modal);
                    return edit.then_some(CatalogueUiAction::Edit);
                }
                if event.code == KeyCode::Enter {
                    return Some(match modal.shell_focus {
                        3 => CatalogueUiAction::DialogClose,
                        4 => CatalogueUiAction::Quit,
                        _ => CatalogueUiAction::Submit,
                    });
                }
                if *editing {
                    edit_key(editor, event, OWNER_BYTES, &mut modal.status);
                    clear_layout(modal);
                    return None;
                }
                return match event.code {
                    KeyCode::Char('o') => Some(CatalogueUiAction::Kind(0)),
                    KeyCode::Char('u') => Some(CatalogueUiAction::Kind(1)),
                    KeyCode::Char('p') => Some(CatalogueUiAction::Kind(2)),
                    KeyCode::Char('e') if *kind != 1 => Some(CatalogueUiAction::Edit),
                    KeyCode::Char('q') => Some(CatalogueUiAction::Quit),
                    _ => None,
                };
            }
            CatalogueDialog::Save { editor } => {
                if matches!(event.code, KeyCode::Tab | KeyCode::BackTab) {
                    next_focus(&mut modal.shell_focus, 4, event);
                } else if event.code == KeyCode::Enter {
                    return Some(match modal.shell_focus {
                        2 => CatalogueUiAction::DialogClose,
                        3 => CatalogueUiAction::Quit,
                        _ => CatalogueUiAction::Submit,
                    });
                } else if modal.shell_focus == 0 {
                    edit_key(editor, event, SET_BYTES, &mut modal.status);
                } else if event.code == KeyCode::Char('q') {
                    return Some(CatalogueUiAction::Quit);
                }
                clear_layout(modal);
                return None;
            }
            CatalogueDialog::Activate {
                names,
                selected,
                top,
            } => {
                if matches!(event.code, KeyCode::Tab | KeyCode::BackTab) {
                    next_focus(&mut modal.shell_focus, 4, event);
                    clear_layout(modal);
                    return None;
                }
                let index = match event.code {
                    KeyCode::Up => selected.saturating_sub(1),
                    KeyCode::Down => selected
                        .saturating_add(1)
                        .min(names.len().saturating_sub(1)),
                    KeyCode::PageUp => selected.saturating_sub(rows),
                    KeyCode::PageDown => selected
                        .saturating_add(rows)
                        .min(names.len().saturating_sub(1)),
                    KeyCode::Home => 0,
                    KeyCode::End => names.len().saturating_sub(1),
                    KeyCode::Enter => {
                        return Some(match modal.shell_focus {
                            2 => CatalogueUiAction::DialogClose,
                            3 => CatalogueUiAction::Quit,
                            _ => CatalogueUiAction::Submit,
                        })
                    }
                    KeyCode::Char('q') => return Some(CatalogueUiAction::Quit),
                    _ => return None,
                };
                if index < *top {
                    *top = index;
                } else if index >= top.saturating_add(rows) {
                    *top = index + 1 - rows;
                }
                return Some(CatalogueUiAction::DialogItem(index));
            }
            CatalogueDialog::Message { .. } => {
                return match event.code {
                    KeyCode::Tab | KeyCode::BackTab => {
                        next_focus(&mut modal.shell_focus, 2, event);
                        clear_layout(modal);
                        None
                    }
                    KeyCode::Up => Some(CatalogueUiAction::DialogScroll(-1)),
                    KeyCode::Down => Some(CatalogueUiAction::DialogScroll(1)),
                    KeyCode::PageUp => Some(CatalogueUiAction::DialogScroll(-(rows as i32))),
                    KeyCode::PageDown => Some(CatalogueUiAction::DialogScroll(rows as i32)),
                    KeyCode::Enter => Some(if modal.shell_focus.is_multiple_of(2) {
                        CatalogueUiAction::DialogClose
                    } else {
                        CatalogueUiAction::Quit
                    }),
                    KeyCode::Char('q') => Some(CatalogueUiAction::Quit),
                    _ => None,
                };
            }
        }
    }
    if let Some(workspace) = &modal.workspace {
        // Preserve prior owned Source/discovery/recovery/dialog capture.
        if modal.local_actor_purpose.is_some() && event.code == KeyCode::Enter {
            return Some(CatalogueUiAction::UseLocalActor);
        }
        if event.code == KeyCode::Char('q') && workspace.focus != CatalogueFocus::Query {
            return Some(CatalogueUiAction::Quit);
        }
        return workspace.key(event.code, event.modifiers).map(input_action);
    }
    shell_key(modal, event, false)
}
fn shell_key(
    modal: &mut CatalogueModal,
    event: KeyEvent,
    safe_only: bool,
) -> Option<CatalogueUiAction> {
    let count = if safe_only { 2 } else { 3 };
    match event.code {
        KeyCode::Char('q') => Some(CatalogueUiAction::Quit),
        KeyCode::Char('s') if !safe_only => Some(CatalogueUiAction::Source),
        KeyCode::Tab | KeyCode::BackTab => {
            next_focus(&mut modal.shell_focus, count, event);
            clear_layout(modal);
            None
        }
        KeyCode::Enter => Some(if safe_only {
            if modal.shell_focus.is_multiple_of(2) {
                CatalogueUiAction::Close
            } else {
                CatalogueUiAction::Quit
            }
        } else {
            match modal.shell_focus % 3 {
                0 => CatalogueUiAction::Source,
                1 => CatalogueUiAction::Close,
                _ => CatalogueUiAction::Quit,
            }
        }),
        _ => None,
    }
}
fn next_focus(focus: &mut usize, count: usize, event: KeyEvent) {
    *focus = if event.code == KeyCode::BackTab || event.modifiers.contains(KeyModifiers::SHIFT) {
        (*focus + count - 1) % count
    } else {
        (*focus + 1) % count
    };
}
fn insert(editor: &mut TextBuffer, value: &str, cap: usize) -> Result<(), &'static str> {
    if value.len() > cap
        || value.chars().any(char::is_control)
        || editor
            .text
            .len()
            .saturating_sub(editor.selection().map_or(0, |range| range.len()))
            .saturating_add(value.len())
            > cap
    {
        return Err("Single-line field limit reached; previous text retained");
    }
    editor
        .insert(value)
        .map_err(|_| "Single-line editor unavailable; previous text retained")
}
fn edit_key(editor: &mut TextBuffer, event: KeyEvent, cap: usize, status: &mut String) {
    let control = event.modifiers.contains(KeyModifiers::CONTROL);
    let shift = event.modifiers.contains(KeyModifiers::SHIFT);
    match event.code {
        KeyCode::Char('a') if control => editor.select_all(),
        KeyCode::Char('u') if control => editor.clear(),
        KeyCode::Left => editor.horizontal(false, shift),
        KeyCode::Right => editor.horizontal(true, shift),
        KeyCode::Home => editor.line_edge(false, true, shift),
        KeyCode::End => editor.line_edge(true, true, shift),
        KeyCode::Backspace => editor.delete_backward(),
        KeyCode::Delete => editor.delete_forward(),
        KeyCode::Char(value) if !control && !event.modifiers.contains(KeyModifiers::ALT) => {
            if let Err(reason) = insert(editor, &value.to_string(), cap) {
                *status = reason.into();
            }
        }
        _ => {}
    }
}
pub fn paste(modal: &mut CatalogueModal, value: &str) -> Option<CatalogueUiAction> {
    if modal.source_transition.active() {
        return None;
    }

    if !modal.fenced
        && (modal.discovery.warning_active() || (!modal.busy && modal.discovery.active()))
    {
        modal.discovery.paste(value);
        return None;
    }
    if modal.busy || modal.fenced {
        return None;
    }
    if let Some(dialog) = &mut modal.dialog {
        let field = match dialog {
            CatalogueDialog::Source {
                kind,
                editor,
                editing,
            } if *kind != 1 && *editing => Some((editor, OWNER_BYTES)),
            CatalogueDialog::Save { editor } if modal.shell_focus == 0 => Some((editor, SET_BYTES)),
            _ => None,
        };
        if let Some((editor, cap)) = field {
            if let Err(reason) = insert(editor, value, cap) {
                modal.status = reason.into();
            }
        }
        clear_layout(modal);
        return None;
    }
    if modal.workspace.is_some() {
        if value.len() > CATALOGUE_QUERY_BYTES
            || value.chars().take(129).count() > 128
            || value.chars().any(char::is_control)
        {
            modal.status = "Filter limit reached; previous query retained".into();
            return None;
        }
        return Some(CatalogueUiAction::Input(CatalogueInput::Paste(
            value.to_owned(),
        )));
    }
    None
}

#[cfg(test)]
#[path = "repository_catalogue_dialogs_tests.rs"]
mod tests;
