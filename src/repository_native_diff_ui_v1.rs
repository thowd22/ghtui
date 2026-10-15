//! Screen geometry and borrowed, worker-prepared diff cells. No provider work.
use crate::{
    editor::{display_line, display_width},
    git::OriginalComparisonSourceV1,
    repository_native_diff_content_v2::NativeDiffPurposeV2,
    repository_native_diff_projection_v1::{
        NativeDiffCellV1, NativeDiffLineTicketV1, NativeDiffPaintRoleV1,
    },
    repository_native_diff_v1::{NativeDiffOptionChangeV1, NativeDiffStateV1},
    repository_view::{DiffLayout, DiffSide},
    theme::{SemanticToken, Theme},
};
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Margin, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame,
};

pub enum NativeDiffActionV1<S: NativeDiffPurposeV2 = OriginalComparisonSourceV1> {
    Open,
    Close,
    Quit,
    Controls,
    External,
    Comment,
    Threads,
    ThreadBackV4,
    CancelInline,
    FocusQuery,
    FocusBody,
    FocusNext(bool),
    Activate,
    QueryClick(usize),
    Insert(String),
    Backspace,
    Delete,
    SelectAll,
    ClearQuery,
    Cursor {
        right: bool,
        select: bool,
    },
    Edge {
        end: bool,
        select: bool,
    },
    Regex,
    Option(NativeDiffOptionChangeV1),
    File(i32),
    Match(i32),
    Hunk(i32),
    SelectHunk,
    Move {
        delta: i32,
        extend: bool,
    },
    First,
    Last,
    Horizontal(i32),
    Scroll(i32),
    Select,
    ClearRange,
    Source,
    Stage,
    FullSide(DiffSide),
    BackToPatch,
    FullPoint {
        ticket: crate::repository_native_full_side_v3::NativeFullSideTicketV3<S>,
        row: usize,
    },
    Point {
        ticket: NativeDiffLineTicketV1<S>,
        row: usize,
        right: bool,
    },
    Drag {
        first: NativeDiffLineTicketV1<S>,
        last: NativeDiffLineTicketV1<S>,
    },
}
impl<S: NativeDiffPurposeV2> Clone for NativeDiffActionV1<S> {
    fn clone(&self) -> Self {
        match self {
            Self::Open => Self::Open,
            Self::Close => Self::Close,
            Self::Quit => Self::Quit,
            Self::Controls => Self::Controls,
            Self::External => Self::External,
            Self::Comment => Self::Comment,
            Self::Threads => Self::Threads,
            Self::ThreadBackV4 => Self::ThreadBackV4,
            Self::CancelInline => Self::CancelInline,
            Self::FocusQuery => Self::FocusQuery,
            Self::FocusBody => Self::FocusBody,
            Self::FocusNext(value) => Self::FocusNext(*value),
            Self::Activate => Self::Activate,
            Self::QueryClick(value) => Self::QueryClick(*value),
            Self::Insert(value) => Self::Insert(value.clone()),
            Self::Backspace => Self::Backspace,
            Self::Delete => Self::Delete,
            Self::SelectAll => Self::SelectAll,
            Self::ClearQuery => Self::ClearQuery,
            Self::Cursor { right, select } => Self::Cursor {
                right: *right,
                select: *select,
            },
            Self::Edge { end, select } => Self::Edge {
                end: *end,
                select: *select,
            },
            Self::Regex => Self::Regex,
            Self::Option(value) => Self::Option(value.clone()),
            Self::File(value) => Self::File(*value),
            Self::Match(value) => Self::Match(*value),
            Self::Hunk(value) => Self::Hunk(*value),
            Self::SelectHunk => Self::SelectHunk,
            Self::Move { delta, extend } => Self::Move {
                delta: *delta,
                extend: *extend,
            },
            Self::First => Self::First,
            Self::Last => Self::Last,
            Self::Horizontal(value) => Self::Horizontal(*value),
            Self::Scroll(value) => Self::Scroll(*value),
            Self::Select => Self::Select,
            Self::ClearRange => Self::ClearRange,
            Self::Source => Self::Source,
            Self::Stage => Self::Stage,
            Self::FullSide(side) => Self::FullSide(*side),
            Self::BackToPatch => Self::BackToPatch,
            Self::FullPoint { ticket, row } => Self::FullPoint {
                ticket: ticket.clone(),
                row: *row,
            },
            Self::Point { ticket, row, right } => Self::Point {
                ticket: ticket.clone(),
                row: *row,
                right: *right,
            },
            Self::Drag { first, last } => Self::Drag {
                first: first.clone(),
                last: last.clone(),
            },
        }
    }
}
impl<S: NativeDiffPurposeV2> PartialEq for NativeDiffActionV1<S> {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Open, Self::Open) => true,
            (Self::Close, Self::Close) => true,
            (Self::Quit, Self::Quit) => true,
            (Self::Controls, Self::Controls) => true,
            (Self::External, Self::External) => true,
            (Self::Comment, Self::Comment) | (Self::CancelInline, Self::CancelInline) => true,
            (Self::Threads, Self::Threads) => true,
            (Self::ThreadBackV4, Self::ThreadBackV4) => true,
            (Self::FocusQuery, Self::FocusQuery) => true,
            (Self::FocusBody, Self::FocusBody) => true,
            (Self::FocusNext(a), Self::FocusNext(b)) => a == b,
            (Self::Activate, Self::Activate) => true,
            (Self::QueryClick(a), Self::QueryClick(b)) => a == b,
            (Self::Insert(a), Self::Insert(b)) => a == b,
            (Self::Backspace, Self::Backspace) => true,
            (Self::Delete, Self::Delete) => true,
            (Self::SelectAll, Self::SelectAll) => true,
            (Self::ClearQuery, Self::ClearQuery) => true,
            (
                Self::Cursor {
                    right: a_right,
                    select: a_select,
                },
                Self::Cursor {
                    right: b_right,
                    select: b_select,
                },
            ) => a_right == b_right && a_select == b_select,
            (
                Self::Edge {
                    end: a_end,
                    select: a_select,
                },
                Self::Edge {
                    end: b_end,
                    select: b_select,
                },
            ) => a_end == b_end && a_select == b_select,
            (Self::Regex, Self::Regex) => true,
            (Self::Option(a), Self::Option(b)) => a == b,
            (Self::File(a), Self::File(b)) => a == b,
            (Self::Match(a), Self::Match(b)) => a == b,
            (Self::Hunk(a), Self::Hunk(b)) => a == b,
            (Self::SelectHunk, Self::SelectHunk) => true,
            (
                Self::Move {
                    delta: a_delta,
                    extend: a_extend,
                },
                Self::Move {
                    delta: b_delta,
                    extend: b_extend,
                },
            ) => a_delta == b_delta && a_extend == b_extend,
            (Self::First, Self::First) => true,
            (Self::Last, Self::Last) => true,
            (Self::Horizontal(a), Self::Horizontal(b)) => a == b,
            (Self::Scroll(a), Self::Scroll(b)) => a == b,
            (Self::Select, Self::Select) => true,
            (Self::ClearRange, Self::ClearRange) => true,
            (Self::Source, Self::Source) => true,
            (Self::Stage, Self::Stage) => true,
            (Self::FullSide(a), Self::FullSide(b)) => a == b,
            (Self::BackToPatch, Self::BackToPatch) => true,
            (Self::FullPoint { ticket: a, row: ar }, Self::FullPoint { ticket: b, row: br }) => {
                a == b && ar == br
            }
            (
                Self::Point {
                    ticket: a_ticket,
                    row: a_row,
                    right: a_right,
                },
                Self::Point {
                    ticket: b_ticket,
                    row: b_row,
                    right: b_right,
                },
            ) => a_ticket == b_ticket && a_row == b_row && a_right == b_right,
            (
                Self::Drag {
                    first: a_first,
                    last: a_last,
                },
                Self::Drag {
                    first: b_first,
                    last: b_last,
                },
            ) => a_first == b_first && a_last == b_last,
            _ => false,
        }
    }
}
impl<S: NativeDiffPurposeV2> Eq for NativeDiffActionV1<S> {}
impl<S: NativeDiffPurposeV2> std::fmt::Debug for NativeDiffActionV1<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("NativeDiffActionV1")
            .field(&std::mem::discriminant(self))
            .finish()
    }
}

pub struct NativeDiffHitV1<S: NativeDiffPurposeV2 = OriginalComparisonSourceV1> {
    pub rect: Rect,
    pub action: NativeDiffActionV1<S>,
}
pub struct NativeDiffLayoutV1<S: NativeDiffPurposeV2 = OriginalComparisonSourceV1> {
    pub(crate) thread_return_v4: bool,
    pub(crate) area: Rect,
    pub(crate) hits: Vec<NativeDiffHitV1<S>>,
    pub(crate) body: Rect,
    pub(crate) query: Rect,
    pub query_focused: bool,
    pub controls: bool,
    pub focus: Option<usize>,
    drag: Option<(NativeDiffLineTicketV1<S>, NativeDiffLineTicketV1<S>)>,
}
impl<S: NativeDiffPurposeV2> Clone for NativeDiffHitV1<S> {
    fn clone(&self) -> Self {
        Self {
            rect: self.rect,
            action: self.action.clone(),
        }
    }
}
impl<S: NativeDiffPurposeV2> Default for NativeDiffLayoutV1<S> {
    fn default() -> Self {
        Self {
            thread_return_v4: false,
            area: Rect::default(),
            hits: Vec::new(),
            body: Rect::default(),
            query: Rect::default(),
            query_focused: false,
            controls: false,
            focus: None,
            drag: None,
        }
    }
}
impl<S: NativeDiffPurposeV2> NativeDiffLayoutV1<S> {
    pub fn clear(&mut self) {
        self.hits.clear();
        self.body = Rect::default();
        self.query = Rect::default();
        self.drag = None;
    }
    pub fn hits(&self) -> &[NativeDiffHitV1<S>] {
        &self.hits
    }
    pub fn body_height(&self) -> usize {
        usize::from(self.body.height.max(1))
    }
    pub fn cycle(&mut self, back: bool) {
        self.query_focused = false;
        let controls: Vec<_> = self
            .hits
            .iter()
            .enumerate()
            .filter(|(_, hit)| {
                !matches!(
                    hit.action,
                    NativeDiffActionV1::Point { .. } | NativeDiffActionV1::FullPoint { .. }
                )
            })
            .map(|(i, _)| i)
            .collect();
        if controls.is_empty() {
            self.focus = None;
            return;
        }
        let index = self
            .focus
            .and_then(|focus| controls.iter().position(|i| *i == focus));
        let next = match index {
            None => {
                if back {
                    controls.len() - 1
                } else {
                    0
                }
            }
            Some(i) => {
                if back {
                    (i + controls.len() - 1) % controls.len()
                } else {
                    (i + 1) % controls.len()
                }
            }
        };
        self.focus = Some(controls[next]);
    }
    pub fn activated(&self) -> Option<NativeDiffActionV1<S>> {
        self.focus
            .and_then(|i| self.hits.get(i))
            .map(|hit| hit.action.clone())
    }
    pub fn key(&self, code: KeyCode, mods: KeyModifiers) -> Option<NativeDiffActionV1<S>> {
        use NativeDiffActionV1 as A;
        let ctrl = mods.contains(KeyModifiers::CONTROL);
        let shift = mods.contains(KeyModifiers::SHIFT);
        if ctrl && code == KeyCode::Char('c') {
            return Some(A::Quit);
        }
        if code == KeyCode::Esc {
            return Some(if self.query_focused {
                A::FocusBody
            } else {
                A::Close
            });
        }
        if code == KeyCode::Tab || code == KeyCode::BackTab {
            return Some(A::FocusNext(code == KeyCode::BackTab));
        }
        if self.query_focused {
            return match code {
                KeyCode::Char('a') if ctrl => Some(A::SelectAll),
                KeyCode::Char('u') if ctrl => Some(A::ClearQuery),
                KeyCode::Char(value) if !ctrl && !mods.contains(KeyModifiers::ALT) => {
                    Some(A::Insert(value.to_string()))
                }
                KeyCode::Backspace => Some(A::Backspace),
                KeyCode::Delete => Some(A::Delete),
                KeyCode::Left | KeyCode::Right => Some(A::Cursor {
                    right: code == KeyCode::Right,
                    select: shift,
                }),
                KeyCode::Home | KeyCode::End => Some(A::Edge {
                    end: code == KeyCode::End,
                    select: shift,
                }),
                KeyCode::Enter | KeyCode::Down => Some(A::FocusBody),
                _ => None,
            };
        }
        if code == KeyCode::Enter && self.focus.is_some() {
            return Some(A::Activate);
        }
        if self.controls {
            match code {
                KeyCode::Up | KeyCode::Left | KeyCode::Char('k') => {
                    return Some(A::FocusNext(true))
                }
                KeyCode::Down | KeyCode::Right | KeyCode::Char('j') => {
                    return Some(A::FocusNext(false))
                }
                KeyCode::Char(' ') if self.focus.is_some() => return Some(A::Activate),
                KeyCode::Char(' ') => return Some(A::FocusNext(false)),
                _ => {}
            }
        }
        match code {
            KeyCode::Char('q') => Some(A::Quit),
            KeyCode::Char('o') => Some(A::Controls),
            KeyCode::Char('E') => Some(A::External),
            KeyCode::Char('P') if S::supports_inline_comment() => Some(A::Comment),
            KeyCode::Char('T') if S::supports_review_threads() => Some(A::Threads),
            KeyCode::Char('b') if self.thread_return_v4 && S::supports_review_threads() => {
                Some(A::ThreadBackV4)
            }
            KeyCode::Char('/') => Some(A::FocusQuery),
            KeyCode::Char('r') => Some(A::Regex),
            KeyCode::Char('l') => Some(A::Option(NativeDiffOptionChangeV1::Layout)),
            KeyCode::Char('O') => Some(A::Option(NativeDiffOptionChangeV1::Side(DiffSide::Old))),
            KeyCode::Char('N') => Some(A::Option(NativeDiffOptionChangeV1::Side(DiffSide::New))),
            KeyCode::Char('w') => Some(A::Option(NativeDiffOptionChangeV1::Wrap)),
            KeyCode::Char('c') => Some(A::Option(NativeDiffOptionChangeV1::Context)),
            KeyCode::Char('s') => Some(A::Option(NativeDiffOptionChangeV1::Whitespace)),
            KeyCode::Char('y') => Some(A::Option(NativeDiffOptionChangeV1::Syntax)),
            KeyCode::Char('i') => Some(A::Option(NativeDiffOptionChangeV1::WithinLine)),
            KeyCode::Char('G') => Some(A::Option(NativeDiffOptionChangeV1::CollapseGenerated)),
            KeyCode::Char('[') => Some(A::File(-1)),
            KeyCode::Char(']') => Some(A::File(1)),
            KeyCode::Char('n') => Some(A::Match(1)),
            KeyCode::Char('p') => Some(A::Match(-1)),
            KeyCode::Char('{') => Some(A::Hunk(-1)),
            KeyCode::Char('}') => Some(A::Hunk(1)),
            KeyCode::Char('h') => Some(A::SelectHunk),
            KeyCode::Char(' ') => Some(A::Select),
            KeyCode::Char('x') => Some(A::ClearRange),
            KeyCode::Char('I') => Some(A::Source),
            KeyCode::Char('a') => Some(A::Stage),
            KeyCode::Char('F') => Some(A::FullSide(DiffSide::Old)),
            KeyCode::Char('V') => Some(A::FullSide(DiffSide::New)),
            KeyCode::Char('b') => Some(A::BackToPatch),
            KeyCode::Up | KeyCode::Char('k') => Some(A::Move {
                delta: -1,
                extend: shift,
            }),
            KeyCode::Down | KeyCode::Char('j') => Some(A::Move {
                delta: 1,
                extend: shift,
            }),
            KeyCode::PageUp => Some(A::Move {
                delta: -(self.body_height() as i32),
                extend: shift,
            }),
            KeyCode::PageDown => Some(A::Move {
                delta: self.body_height() as i32,
                extend: shift,
            }),
            KeyCode::Home => Some(A::First),
            KeyCode::End => Some(A::Last),
            KeyCode::Left => Some(A::Horizontal(-8)),
            KeyCode::Right => Some(A::Horizontal(8)),
            _ => None,
        }
    }
    pub fn mouse(
        &mut self,
        kind: MouseEventKind,
        column: u16,
        row: u16,
    ) -> Option<NativeDiffActionV1<S>> {
        use NativeDiffActionV1 as A;
        let point = (column, row).into();
        let hit = self
            .hits
            .iter()
            .rev()
            .find(|hit| hit.rect.contains(point))
            .map(|hit| hit.action.clone());
        match kind {
            MouseEventKind::Down(MouseButton::Left) if self.query.contains(point) => {
                Some(A::QueryClick(usize::from(column - self.query.x)))
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(A::Point { ticket, .. }) = &hit {
                    self.drag = Some((ticket.clone(), ticket.clone()));
                }
                hit
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let (Some((_, last)), Some(A::Point { ticket, .. })) = (&mut self.drag, hit) {
                    *last = ticket;
                }
                None
            }
            MouseEventKind::Up(MouseButton::Left) => self
                .drag
                .take()
                .filter(|(first, last)| first != last)
                .map(|(first, last)| A::Drag { first, last }),
            MouseEventKind::ScrollDown | MouseEventKind::ScrollUp if self.body.contains(point) => {
                Some(A::Scroll(if kind == MouseEventKind::ScrollDown {
                    3
                } else {
                    -3
                }))
            }
            _ => None,
        }
    }
    pub(crate) fn button(
        &mut self,
        frame: &mut Frame,
        rect: Rect,
        label: &str,
        action: NativeDiffActionV1<S>,
        theme: Theme,
    ) {
        if rect.is_empty() {
            return;
        }
        let focused = self.focus == Some(self.hits.len());
        frame.render_widget(
            Paragraph::new(label).style(if focused {
                theme
                    .selected(SemanticToken::Focus)
                    .add_modifier(Modifier::UNDERLINED)
            } else {
                theme.style(
                    if (action == NativeDiffActionV1::Comment
                        || action == NativeDiffActionV1::Threads)
                        && label.contains("disabled")
                    {
                        SemanticToken::Muted
                    } else {
                        SemanticToken::Reference
                    },
                )
            }),
            rect,
        );
        self.hits.push(NativeDiffHitV1 { rect, action });
    }
    pub fn draw(
        &mut self,
        frame: &mut Frame,
        state: &mut NativeDiffStateV1<S>,
        area: Rect,
        theme: Theme,
    ) {
        // Do not retire an in-progress drag merely because another frame draws.
        self.area = area;
        self.hits.clear();
        self.body = Rect::default();
        self.query = Rect::default();
        frame.render_widget(Clear, area);
        frame.render_widget(
            Block::default()
                .borders(Borders::ALL)
                .title(if area.width > 514 {
                    S::pane_title(true)
                } else {
                    S::pane_title(false)
                })
                .style(theme.base()),
            area,
        );
        // Input and worker use the same bounded width. A wider terminal must
        // not paint cells at another geometry and silently remove mouse hits.
        let mut inner = area.inner(Margin::new(1, 1));
        inner.width = inner.width.min(512);
        if inner.is_empty() {
            return;
        }
        if inner.width < 16 || inner.height < 10 {
            self.button(
                frame,
                Rect::new(inner.x, inner.bottom() - 1, inner.width.min(11), 1),
                "[Esc Close]",
                NativeDiffActionV1::Close,
                theme,
            );
            if inner.height > 1 {
                frame.render_widget(
                    Paragraph::new("Resize to view native diff; no source was retargeted")
                        .style(theme.style(SemanticToken::Muted)),
                    Rect::new(inner.x, inner.y, inner.width, 1),
                );
            }
            return;
        }
        if self.controls {
            if state.is_full_side() {
                crate::repository_native_full_side_ui_v3::draw_controls(
                    self, frame, state, inner, theme,
                );
            } else {
                self.draw_controls(frame, state, inner, theme);
            }
            return;
        }
        if state.is_full_side() {
            crate::repository_native_full_side_ui_v3::draw(self, frame, state, inner, theme);
            return;
        }
        let mut y = inner.y;
        if let Some(source) = state.source() {
            if let Some(actors) = state.actors() {
                frame.render_widget(
                    Paragraph::new(actors.header_label.as_ref())
                        .style(theme.style(SemanticToken::Reference)),
                    Rect::new(inner.x, y, inner.width, 1),
                );
                y += 1;
            }
            if let Some(label) = source.member_label(state.file) {
                frame.render_widget(
                    Paragraph::new(format!(
                        "File {}/{}: {}",
                        state.file + 1,
                        source.member_count(),
                        label
                    ))
                    .style(theme.style(SemanticToken::Changed)),
                    Rect::new(inner.x, y, inner.width, 1),
                );
                y += 1;
            }
        }
        let projection = state.frame();
        if let Some(actors) = state.actors() {
            for (identity, label) in [
                (&actors.author, actors.author_label.as_ref()),
                (&actors.committer, actors.committer_label.as_ref()),
            ] {
                frame.render_widget(
                    crate::identity::AvatarWidget {
                        identity,
                        image: None,
                        theme,
                    },
                    Rect::new(inner.x, y, 4.min(inner.width), 1),
                );
                frame.render_widget(
                    Paragraph::new(label).style(theme.style(SemanticToken::Actor)),
                    Rect::new(inner.x + 5, y, inner.width.saturating_sub(5), 1),
                );
                y += 1;
            }
        }
        let current = state.current_frame().is_some();
        let summary = if !current && projection.is_some() {
            format!(
                "Range {} retained • previous preview (not current)",
                state.range().map_or(0, |range| range.changed().len())
            )
        } else {
            projection
                .map(|p| {
                    format!(
                        "Selected {} (hidden {}) • {} matches{} • hidden rows {}",
                        p.range().map_or(0, |r| r.changed().len()),
                        p.hidden_selected(),
                        p.matches().len(),
                        if p.matches_capped() { "+ capped" } else { "" },
                        p.hidden_rows()
                    )
                })
                .unwrap_or_else(|| "No prepared rows • no selection authority".into())
        };
        frame.render_widget(
            Paragraph::new(summary).style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, y, inner.width, 1),
        );
        y += 1;
        if inner.width < 60 {
            frame.render_widget(
                Paragraph::new(if !current {
                    "Selected-file results are not current".into()
                } else {
                    projection
                        .map(|p| {
                            format!(
                                "Matches {}{} • hidden rows {}",
                                p.matches().len(),
                                if p.matches_capped() { "+ capped" } else { "" },
                                p.hidden_rows()
                            )
                        })
                        .unwrap_or_else(|| "Selected-file patch scope only".into())
                })
                .style(theme.style(SemanticToken::Reference)),
                Rect::new(inner.x, y, inner.width, 1),
            );
            y += 1;
        }
        self.button(
            frame,
            Rect::new(inner.x, y, 7.min(inner.width), 1),
            if state.regex { "Regex:" } else { "Exact:" },
            NativeDiffActionV1::FocusQuery,
            theme,
        );
        self.query = Rect::new(
            inner.x + 7.min(inner.width),
            y,
            inner.width.saturating_sub(7),
            1,
        );
        state.query.ensure_visible(usize::from(self.query.width), 1);
        frame.render_widget(
            Paragraph::new(display_line(
                &state.query.text,
                state.query.left_column,
                usize::from(self.query.width),
            ))
            .style(theme.base()),
            self.query,
        );
        if self.query_focused && !self.query.is_empty() {
            let caret = display_width(&state.query.text[..state.query.cursor])
                .saturating_sub(state.query.left_column)
                .min(usize::from(self.query.width.saturating_sub(1)));
            frame.set_cursor_position((self.query.x + caret as u16, self.query.y));
        }
        y += 1;
        let controls = [
            ("[Esc Close]", NativeDiffActionV1::Close),
            ("[o Options]", NativeDiffActionV1::Controls),
            ("[h Hunk]", NativeDiffActionV1::SelectHunk),
            ("[x Clear]", NativeDiffActionV1::ClearRange),
            ("[I Source]", NativeDiffActionV1::Source),
            ("[E External]", NativeDiffActionV1::External),
            ("[[ File]", NativeDiffActionV1::File(-1)),
            ("[] File]", NativeDiffActionV1::File(1)),
            ("[n Match]", NativeDiffActionV1::Match(1)),
        ];
        let mut x = inner.x;
        for (label, action) in controls
            .into_iter()
            .chain(S::supports_inline_comment().then_some((
                if state.inline_selection_available_v1() {
                    "[P Comment]"
                } else {
                    "[P Comment disabled]"
                },
                NativeDiffActionV1::Comment,
            )))
            .chain(S::supports_review_threads().then_some((
                if state.current_frame().is_some() {
                    "[T Threads]"
                } else {
                    "[T Threads disabled]"
                },
                NativeDiffActionV1::Threads,
            )))
            .chain(
                self.thread_return_v4
                    .then_some(("[b Back thread]", NativeDiffActionV1::ThreadBackV4)),
            )
        {
            let width = (label.len() as u16).min(inner.width);
            if x + width > inner.right() {
                x = inner.x;
                y += 1;
            }
            if y >= inner.bottom().saturating_sub(3) {
                break;
            }
            self.button(frame, Rect::new(x, y, width, 1), label, action, theme);
            x += width + 1;
        }
        y += 1;
        let metadata_height = if inner.width < 60 { 3 } else { 1 };
        let footer = inner.bottom().saturating_sub(metadata_height + 3);
        self.body = Rect::new(inner.x, y, inner.width, footer.saturating_sub(y));
        frame.render_widget(
            Paragraph::new(state.status.as_str()).style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
        );
        if let Some(p) = state.frame() {
            frame.render_widget(
                Paragraph::new(p.syntax_label())
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .style(theme.style(SemanticToken::Muted)),
                Rect::new(inner.x, footer, inner.width, metadata_height),
            );
            frame.render_widget(
                Paragraph::new(format!(
                    "plain {} • linear fallback {} • patch only",
                    p.plain_fallback_lines(),
                    p.within_line_fallbacks()
                ))
                .style(theme.style(SemanticToken::Muted)),
                Rect::new(inner.x, footer + metadata_height, inner.width, 1),
            );
        }
        self.button(
            frame,
            Rect::new(inner.x, inner.bottom() - 1, 11.min(inner.width), 1),
            "[Esc Close]",
            NativeDiffActionV1::Close,
            theme,
        );
        self.button(
            frame,
            Rect::new(
                inner.x + 12,
                inner.bottom() - 1,
                11.min(inner.width.saturating_sub(12)),
                1,
            ),
            "[o Options]",
            NativeDiffActionV1::Controls,
            theme,
        );
        self.button(
            frame,
            Rect::new(
                inner.x + 24,
                inner.bottom() - 1,
                8.min(inner.width.saturating_sub(24)),
                1,
            ),
            "[q Quit]",
            NativeDiffActionV1::Quit,
            theme,
        );
        let notice_height = state.metadata_note().map_or(0, |note| {
            let height = self.body.height.min(4);
            frame.render_widget(
                Paragraph::new(note)
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .style(theme.style(SemanticToken::Muted)),
                Rect::new(self.body.x, self.body.y, self.body.width, height),
            );
            height
        });
        let Some(p) = state.frame() else {
            return;
        };
        let body = if !current {
            let label = p.source().member_label(p.file()).unwrap_or("unavailable");
            frame.render_widget(
                Paragraph::new(format!(
                    "Previous preview: {label} • NOT CURRENT • no row hits"
                ))
                .style(theme.style(SemanticToken::Muted)),
                Rect::new(
                    self.body.x,
                    self.body.y.saturating_add(notice_height),
                    self.body.width,
                    self.body.height.saturating_sub(notice_height).min(1),
                ),
            );
            Rect::new(
                self.body.x,
                self.body.y.saturating_add(notice_height + 1),
                self.body.width,
                self.body.height.saturating_sub(notice_height + 1),
            )
        } else {
            self.body
        };
        let interactive = state.current_frame().is_some() && p.width() == inner.width;
        for (index, row) in p
            .rows()
            .iter()
            .enumerate()
            .skip(state.scroll)
            .take(usize::from(body.height))
        {
            let y = body.y + (index - state.scroll) as u16;
            let split = p.options().layout == DiffLayout::Split && !p.split_focused();
            let left_width = if split {
                inner.width.saturating_sub(3) / 2
            } else {
                inner.width
            };
            let left = Rect::new(inner.x, y, left_width, 1);
            draw_cell(
                frame,
                row.left(),
                left,
                state.horizontal,
                index == state.selected && !state.right,
                theme,
            );
            if interactive {
                if let Some(ticket) = p.ticket(index, false) {
                    self.hits.push(NativeDiffHitV1 {
                        rect: left,
                        action: NativeDiffActionV1::Point {
                            ticket,
                            row: index,
                            right: false,
                        },
                    });
                }
            }
            if split {
                if let Some(cell) = row.right() {
                    let right = Rect::new(
                        left.right() + 3,
                        y,
                        inner.right().saturating_sub(left.right() + 3),
                        1,
                    );
                    frame.render_widget(
                        Paragraph::new(" │ ").style(theme.style(SemanticToken::Ancestor)),
                        Rect::new(left.right(), y, 3, 1),
                    );
                    draw_cell(
                        frame,
                        cell,
                        right,
                        state.horizontal,
                        index == state.selected && state.right,
                        theme,
                    );
                    if interactive {
                        if let Some(ticket) = p.ticket(index, true) {
                            self.hits.push(NativeDiffHitV1 {
                                rect: right,
                                action: NativeDiffActionV1::Point {
                                    ticket,
                                    row: index,
                                    right: true,
                                },
                            });
                        }
                    }
                }
            }
        }
    }
    fn draw_controls(
        &mut self,
        frame: &mut Frame,
        state: &NativeDiffStateV1<S>,
        inner: Rect,
        theme: Theme,
    ) {
        use NativeDiffActionV1 as A;
        use NativeDiffOptionChangeV1 as O;
        let entries = [
            ("[o Back to diff]", A::Controls),
            ("[E External]", A::External),
            ("[Esc Close]", A::Close),
            ("[F Full Old]", A::FullSide(DiffSide::Old)),
            ("[V Full New]", A::FullSide(DiffSide::New)),
            ("[b Back patch]", A::BackToPatch),
            ("[l Layout]", A::Option(O::Layout)),
            ("[O Old side]", A::Option(O::Side(DiffSide::Old))),
            ("[N New side]", A::Option(O::Side(DiffSide::New))),
            ("[w Wrap]", A::Option(O::Wrap)),
            ("[c Context]", A::Option(O::Context)),
            ("[s Whitespace]", A::Option(O::Whitespace)),
            ("[y Syntax]", A::Option(O::Syntax)),
            ("[i Within-line]", A::Option(O::WithinLine)),
            (
                "[g Mark generated]",
                A::Option(O::MarkGenerated(state.file)),
            ),
            ("[G Hide generated]", A::Option(O::CollapseGenerated)),
            ("[r Exact / regex]", A::Regex),
            ("[/ Search patch]", A::FocusQuery),
            ("[p Previous match]", A::Match(-1)),
            ("[n Matching line]", A::Match(1)),
            ("[{ Previous hunk]", A::Hunk(-1)),
            ("[} Next hunk]", A::Hunk(1)),
            ("[h Select hunk]", A::SelectHunk),
            ("[Space Select line]", A::Select),
            ("[x Clear range]", A::ClearRange),
            (S::stage_control(), A::Stage),
            ("[I Source]", A::Source),
            ("[q Quit]", A::Quit),
        ];
        let mut x = inner.x;
        let mut y = inner.y;
        for (label, action) in entries
            .into_iter()
            .chain(S::supports_inline_comment().then_some((
                if state.inline_selection_available_v1() {
                    "[P Comment]"
                } else {
                    "[P Comment disabled]"
                },
                A::Comment,
            )))
            .chain(S::supports_review_threads().then_some((
                if state.current_frame().is_some() {
                    "[T Threads]"
                } else {
                    "[T Threads disabled]"
                },
                A::Threads,
            )))
            .chain(
                self.thread_return_v4
                    .then_some(("[b Back thread]", A::ThreadBackV4)),
            )
        {
            let width = (label.len() as u16).min(inner.width);
            if x + width > inner.right() {
                x = inner.x;
                y += 1;
            }
            if y >= inner.bottom() {
                break;
            }
            self.button(frame, Rect::new(x, y, width, 1), label, action, theme);
            x += width + 1;
        }
        if y + 3 < inner.bottom() {
            frame.render_widget(
                Paragraph::new(S::selection_help())
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .style(theme.style(SemanticToken::Muted)),
                Rect::new(inner.x, y + 2, inner.width, inner.bottom() - y - 3),
            );
        }
        frame.render_widget(
            Paragraph::new(state.status.as_str()).style(theme.style(SemanticToken::Reference)),
            Rect::new(inner.x, inner.bottom() - 1, inner.width, 1),
        );
    }
}

#[cfg(test)]
#[path = "repository_native_diff_ui_v1_tests.rs"]
mod tests;
fn draw_cell<S: NativeDiffPurposeV2>(
    frame: &mut Frame,
    cell: &NativeDiffCellV1<S>,
    rect: Rect,
    horizontal: u16,
    focused: bool,
    theme: Theme,
) {
    let mut base = theme.style(cell.role().into());
    if cell.selected() {
        base = base.add_modifier(Modifier::REVERSED);
    }
    if focused {
        base = base.add_modifier(Modifier::UNDERLINED);
    }
    let number = cell
        .new_line()
        .or(cell.old_line())
        .map_or_else(|| "     ".into(), |n| format!("{n:>5}"));
    let mut spans = vec![Span::styled(
        format!(
            "{} {}{} ",
            if focused { ">" } else { " " },
            number,
            cell.prefix()
        ),
        base,
    )];
    let mut start = 0;
    for span in cell.spans() {
        if span.bytes.start > start {
            spans.push(Span::styled(&cell.text()[start..span.bytes.start], base));
        }
        let style = paint_style(span.role, base, theme);
        spans.push(Span::styled(&cell.text()[span.bytes.clone()], style));
        start = span.bytes.end;
    }
    if start < cell.text().len() {
        spans.push(Span::styled(&cell.text()[start..], base));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans))
            .scroll((0, horizontal))
            .style(base),
        rect,
    );
}
pub(crate) fn paint_style(role: NativeDiffPaintRoleV1, base: Style, theme: Theme) -> Style {
    match role {
        NativeDiffPaintRoleV1::Keyword => base.patch(theme.style(SemanticToken::Reference)),
        NativeDiffPaintRoleV1::String => base.patch(theme.style(SemanticToken::Actor)),
        NativeDiffPaintRoleV1::Comment => base.add_modifier(Modifier::ITALIC),
        NativeDiffPaintRoleV1::Number => base.patch(theme.style(SemanticToken::Changed)),
        NativeDiffPaintRoleV1::WithinLine => base.add_modifier(Modifier::BOLD),
        NativeDiffPaintRoleV1::Search => base.add_modifier(Modifier::BOLD | Modifier::REVERSED),
    }
}
