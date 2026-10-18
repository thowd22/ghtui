//! Original-owner-bound read-only images. Text is prepared once in the worker;
//! drawing and input never fetch, parse patches, copy bodies, or mint write access.
use super::*;
use crate::scoped_pr_observed_tabs_v1::MAX_PREPARED_TEXT;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    layout::Rect,
    style::Modifier,
    widgets::{Block, Borders, Paragraph},
    Frame,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageLayoutV1 {
    Unified,
    Split,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageVisibleSideV1 {
    Old,
    New,
    Both,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrImageUiActionV1 {
    NativeDiff,
    Old,
    New,
    Both,
    Layout,
    Context,
    Range,
    Comment,
    Batch,
    Close,
    Back,
    Quit,
    Move {
        delta: i32,
        extend: bool,
    },
    Point {
        side: PrImageSideV1,
        line: u32,
        extend: bool,
    },
    Scroll(i32),
    Horizontal(i32),
    Focus(i32),
    Activate,
}
#[derive(Clone, Copy)]
struct Hit {
    area: Rect,
    action: PrImageUiActionV1,
}
struct PreparedLine {
    number: u32,
    text: String,
}
pub struct PrImageViewerV1 {
    proof: Arc<ClosedPrFileImagesV1>,
    old: Vec<PreparedLine>,
    new: Vec<PreparedLine>,
    header: String,
    old_label: String,
    new_label: String,
    pub layout: ImageLayoutV1,
    pub visible: ImageVisibleSideV1,
    side: PrImageSideV1,
    cursor: u32,
    anchor: u32,
    scroll: usize,
    horizontal: u16,
    context: bool,
    focus: usize,
    hits: Vec<Hit>,
    geometry: Option<Rect>,
    range: Option<ReadOnlyCanonicalRangeV1>,
    notice: String,
}
impl std::fmt::Debug for PrImageViewerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrImageViewerV1(<private immutable read-only content>)")
    }
}
impl PrImageViewerV1 {
    pub(crate) fn original_proof_v1(&self) -> &Arc<ClosedPrFileImagesV1> {
        &self.proof
    }
    pub(crate) fn inline_notice(&mut self, message: &str) {
        self.notice = if message.len() <= 1024 {
            crate::app::safe_text(message)
        } else {
            "Original inline notice exceeds display bound".into()
        };
    }
    /// Worker-only bounded preparation after genuine closure, not a raw DTO.
    pub fn prepare(proof: Arc<ClosedPrFileImagesV1>) -> Result<Self, ObservedPrTabFailureV1> {
        let read = proof.observed();
        let actor_bytes =
            crate::actor_avatar_projection_v1::actor_bytes(&read.parent().summary().pr.author)
                .unwrap_or(128);
        let mut cost = actor_bytes
            .checked_add(read.image(PrImageSideV1::Old).path().len())
            .and_then(|size| size.checked_add(read.image(PrImageSideV1::New).path().len()))
            .and_then(|size| size.checked_add(1024))
            .and_then(|size| size.checked_mul(6))
            .ok_or(ObservedPrTabFailureV1::PreparationUnavailable)?;
        for side in [PrImageSideV1::Old, PrImageSideV1::New] {
            let image = proof.observed().image(side);
            // Six is the maximum display_bytes expansion. Checked BEFORE copying.
            cost = cost
                .checked_add(
                    image
                        .bytes()
                        .map_or(0, |bytes| bytes.len())
                        .checked_mul(6)
                        .ok_or(ObservedPrTabFailureV1::PreparationUnavailable)?,
                )
                .ok_or(ObservedPrTabFailureV1::PreparationUnavailable)?;
            cost = cost
                .checked_add(
                    proof
                        .line_count(side)
                        .checked_mul(32)
                        .ok_or(ObservedPrTabFailureV1::PreparationUnavailable)?,
                )
                .ok_or(ObservedPrTabFailureV1::PreparationUnavailable)?;
        }
        if cost > MAX_PREPARED_TEXT.saturating_sub(64 * 1024)
            || proof
                .old_lines
                .len()
                .checked_add(proof.new_lines.len())
                .is_none_or(|count| count > MAX_PREPARED_ROWS)
        {
            return Err(ObservedPrTabFailureV1::PreparationUnavailable);
        }
        let prepare = |side| {
            (1..=proof.line_count(side))
                .map(|number| {
                    let raw = proof
                        .line(side, number as u32)
                        .expect("actual prepared line offset");
                    let visible = raw.strip_suffix(b"\n").unwrap_or(raw);
                    let text = crate::git::display_bytes(visible);
                    if ratatui::text::Span::raw(text.as_str()).width() > usize::from(u16::MAX) {
                        return Err(ObservedPrTabFailureV1::PreparationUnavailable);
                    }
                    Ok(PreparedLine {
                        number: number as u32,
                        text,
                    })
                })
                .collect::<Result<Vec<_>, ObservedPrTabFailureV1>>()
        };
        let old = prepare(PrImageSideV1::Old)?;
        let new = prepare(PrImageSideV1::New)?;
        let label = |side| {
            let image = read.image(side);
            format!(
                "{} {} • {}",
                if side == PrImageSideV1::Old {
                    "OLD (merge-base M)"
                } else {
                    "NEW (review head H)"
                },
                crate::git::display_bytes(image.path()),
                if image.absent() {
                    "verified absent".into()
                } else {
                    match image.bytes() {
                        Some(bytes) => format!("{:?} • {} bytes", image.kind(), bytes.len()),
                        None => format!(
                            "{:?} • content unavailable; no implicit fetch",
                            image.kind()
                        ),
                    }
                }
            )
        };
        let actor = crate::identity::ActorIdentity::github(
            &read.parent().summary().pr.host,
            &read.parent().summary().pr.author,
        )
        .label();
        let header = format!(
            "M {}\nH {}\n{actor} • Image line numbers; row alignment is not a diff.\nRead-only ranges • LEFT publication would still target H; no write approval.\nBracketed API observations, not an atomic snapshot • < / >: long lines",
            read.merge_base(),
            read.parent().head_sha()
        );
        let old_label = label(PrImageSideV1::Old);
        let new_label = label(PrImageSideV1::New);
        let side = if old.is_empty() {
            PrImageSideV1::New
        } else {
            PrImageSideV1::Old
        };
        let cursor = u32::from(!old.is_empty() || !new.is_empty());
        Ok(Self {
            proof,
            old,
            new,
            header,
            old_label,
            new_label,
            layout: ImageLayoutV1::Split,
            visible: ImageVisibleSideV1::Both,
            side,
            cursor,
            anchor: cursor,
            scroll: 0,
            horizontal: 0,
            context: false,
            focus: CONTROLS.len(),
            hits: vec![],
            geometry: None,
            range: None,
            notice: "Digest-verified immutable objects • no write approval".into(),
        })
    }
    pub fn owns(&self, app: &crate::app::App) -> bool {
        self.proof.owns(app)
    }
    pub fn canonical_range(&self) -> Option<&ReadOnlyCanonicalRangeV1> {
        self.range.as_ref()
    }
    pub fn selected(&self) -> (PrImageSideV1, u32, u32) {
        (
            self.side,
            self.anchor.min(self.cursor),
            self.anchor.max(self.cursor),
        )
    }
    /// Diff "Comment line": show the chosen side, select that one line and
    /// verify it through the same canonical Range check as `R`.
    pub(crate) fn focus_line_v19(&mut self, side: PrImageSideV1, line: u32) -> bool {
        if line == 0 || line as usize > self.lines(side).len() {
            return false;
        }
        self.visible = if side == PrImageSideV1::Old {
            ImageVisibleSideV1::Old
        } else {
            ImageVisibleSideV1::New
        };
        self.point(side, line, false);
        self.scroll = (line as usize - 1).saturating_sub(5);
        self.clear_layout();
        self.apply(PrImageUiActionV1::Range);
        if self.range.is_some() {
            self.notice = format!(
                "P Comment on {} line {line} • Shift+↑/↓ or drag extends the range • {}",
                if side == PrImageSideV1::Old {
                    "LEFT"
                } else {
                    "RIGHT"
                },
                self.notice
            );
        }
        true
    }
    pub fn clear_layout(&mut self) {
        self.hits.clear();
        self.geometry = None;
    }
    fn lines(&self, side: PrImageSideV1) -> &[PreparedLine] {
        if side == PrImageSideV1::Old {
            &self.old
        } else {
            &self.new
        }
    }
    fn point(&mut self, side: PrImageSideV1, line: u32, extend: bool) {
        let count = self.lines(side).len();
        if (line == 0 && count != 0) || line as usize > count {
            return;
        }
        if !extend || side != self.side {
            self.anchor = line;
        }
        self.side = side;
        self.cursor = line;
        self.range = None;
        self.notice = if line == 0 {
            format!(
                "{} image has no selectable text lines",
                if side == PrImageSideV1::Old {
                    "OLD"
                } else {
                    "NEW"
                }
            )
        } else {
            format!(
                "Selection {} {}–{} • R: verify read-only range",
                if side == PrImageSideV1::Old {
                    "LEFT"
                } else {
                    "RIGHT"
                },
                self.anchor.min(self.cursor),
                self.anchor.max(self.cursor)
            )
        };
        self.focus = CONTROLS.len();
    }
    /// Root checks owns() BEFORE this function, including late queued inputs.
    /// Close/Back/Quit are returned to the same App semantics; no hidden backend.
    pub fn apply(&mut self, action: PrImageUiActionV1) -> Option<PrImageUiActionV1> {
        use PrImageUiActionV1::*;
        let before_scroll = self.scroll;
        let layout_changes = matches!(action, Old | New | Both | Layout | Context | Scroll(_));
        match action {
            // An unverified selection is checked first, so P alone comments.
            Comment if self.range.is_none() => {
                self.apply(Range);
                return self.range.is_some().then_some(Comment);
            }
            Close | Back | Quit | Comment | NativeDiff | Batch => return Some(action),
            Old | New | Both => {
                self.visible = match action {
                    Old => ImageVisibleSideV1::Old,
                    New => ImageVisibleSideV1::New,
                    _ => ImageVisibleSideV1::Both,
                };
                if action != Both {
                    let side = if action == Old {
                        PrImageSideV1::Old
                    } else {
                        PrImageSideV1::New
                    };
                    self.point(side, u32::from(!self.lines(side).is_empty()), false);
                }
                self.scroll = 0;
            }
            Layout => {
                self.layout = if self.layout == ImageLayoutV1::Split {
                    ImageLayoutV1::Unified
                } else {
                    ImageLayoutV1::Split
                };
                self.scroll = 0;
            }
            Context => {
                self.context = !self.context;
                self.scroll = 0;
            }
            Range => {
                let (side, first, last) = self.selected();
                match self.proof.canonical_range(side, first, last) {
                    Ok(range) => {
                        self.notice = format!(
                            "READ-ONLY {} {}–{} • review H {}",
                            if side == PrImageSideV1::Old {
                                "LEFT"
                            } else {
                                "RIGHT"
                            },
                            first,
                            last,
                            range.review_head()
                        );
                        self.range = Some(range);
                    }
                    Err(message) => {
                        self.notice = message;
                        self.range = None;
                    }
                }
            }
            Point { side, line, extend } => self.point(side, line, extend),
            Move { delta, extend } => {
                let count = self.lines(self.side).len();
                if count > 0 {
                    let next =
                        (i64::from(self.cursor) + i64::from(delta)).clamp(1, count as i64) as u32;
                    self.point(self.side, next, extend);
                    if !self.context {
                        let height = self.geometry.map_or(1, |area| {
                            let height = usize::from(area.height.saturating_sub(10)).max(1);
                            if self.visible == ImageVisibleSideV1::Both
                                && (self.layout == ImageLayoutV1::Unified || area.width < 102)
                            {
                                (height / 2).saturating_sub(1).max(1)
                            } else {
                                height.saturating_sub(1).max(1)
                            }
                        });
                        let index = self.cursor as usize - 1;
                        if index < self.scroll {
                            self.scroll = index;
                        } else if index >= self.scroll.saturating_add(height) {
                            self.scroll = index + 1 - height;
                        }
                    }
                }
            }
            Scroll(delta) => {
                let count = match self.visible {
                    ImageVisibleSideV1::Old => self.old.len(),
                    ImageVisibleSideV1::New => self.new.len(),
                    ImageVisibleSideV1::Both => self.old.len().max(self.new.len()),
                };
                self.scroll = self
                    .scroll
                    .saturating_add_signed(delta as isize)
                    .min(count.saturating_sub(1));
            }
            Horizontal(delta) => {
                self.horizontal =
                    (i32::from(self.horizontal) + delta).clamp(0, i32::from(u16::MAX)) as u16
            }
            Focus(delta) => {
                self.focus =
                    (self.focus as i32 + delta).rem_euclid(CONTROLS.len() as i32 + 1) as usize
            }
            Activate => {
                if let Some((action, _, _)) = CONTROLS.get(self.focus) {
                    return self.apply(*action);
                }
            }
        }
        // Selection/focus does not invalidate coordinates: a queued mouse drag
        // may contain Down+Drag before the next draw. Resize/layout/scroll does.
        if layout_changes || self.scroll != before_scroll {
            self.clear_layout();
        }
        None
    }
}
const CONTROLS: [(PrImageUiActionV1, &str, char); 14] = [
    (PrImageUiActionV1::NativeDiff, "Native diff", 'v'),
    (PrImageUiActionV1::Old, "Old", 'O'),
    (PrImageUiActionV1::New, "New", 'N'),
    (PrImageUiActionV1::Both, "Both", 'A'),
    (PrImageUiActionV1::Layout, "Layout", 'L'),
    (PrImageUiActionV1::Context, "Context", 'C'),
    (PrImageUiActionV1::Range, "Range", 'R'),
    (PrImageUiActionV1::Comment, "Comment", 'P'),
    (PrImageUiActionV1::Batch, "Batch", 'V'),
    (PrImageUiActionV1::Horizontal(-8), "Left", '<'),
    (PrImageUiActionV1::Horizontal(8), "Right", '>'),
    (PrImageUiActionV1::Close, "Close", '\u{1b}'),
    (PrImageUiActionV1::Back, "Back", 'b'),
    (PrImageUiActionV1::Quit, "Quit", 'q'),
];
pub fn key(view: &PrImageViewerV1, key: KeyEvent) -> Option<PrImageUiActionV1> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(PrImageUiActionV1::Quit);
    }
    let extend = key.modifiers.contains(KeyModifiers::SHIFT);
    match key.code {
        KeyCode::Esc => Some(PrImageUiActionV1::Close),
        KeyCode::Tab => Some(PrImageUiActionV1::Focus(1)),
        KeyCode::BackTab => Some(PrImageUiActionV1::Focus(-1)),
        KeyCode::Enter => Some(if view.focus == CONTROLS.len() {
            PrImageUiActionV1::Range
        } else {
            PrImageUiActionV1::Activate
        }),
        KeyCode::Up | KeyCode::Char('k') => Some(PrImageUiActionV1::Move { delta: -1, extend }),
        KeyCode::Down | KeyCode::Char('j') => Some(PrImageUiActionV1::Move { delta: 1, extend }),
        KeyCode::PageUp => Some(PrImageUiActionV1::Scroll(-8)),
        KeyCode::PageDown => Some(PrImageUiActionV1::Scroll(8)),
        KeyCode::Left => Some(PrImageUiActionV1::Horizontal(-8)),
        KeyCode::Right => Some(PrImageUiActionV1::Horizontal(8)),
        KeyCode::Char(value) => CONTROLS
            .iter()
            .find(|(_, _, key)| *key == value)
            .map(|(action, _, _)| *action),
        _ => None,
    }
}
pub fn mouse(view: &PrImageViewerV1, event: MouseEvent) -> Option<PrImageUiActionV1> {
    let area = view.geometry?;
    if event.column < area.x
        || event.column >= area.right()
        || event.row < area.y
        || event.row >= area.bottom()
    {
        return None;
    }
    match event.kind {
        MouseEventKind::ScrollUp => Some(PrImageUiActionV1::Scroll(-3)),
        MouseEventKind::ScrollDown => Some(PrImageUiActionV1::Scroll(3)),
        MouseEventKind::ScrollLeft => Some(PrImageUiActionV1::Horizontal(-8)),
        MouseEventKind::ScrollRight => Some(PrImageUiActionV1::Horizontal(8)),
        MouseEventKind::Down(MouseButton::Left) | MouseEventKind::Drag(MouseButton::Left) => {
            let hit = view.hits.iter().find(|hit| {
                event.column >= hit.area.x
                    && event.column < hit.area.right()
                    && event.row >= hit.area.y
                    && event.row < hit.area.bottom()
            })?;
            match hit.action {
                PrImageUiActionV1::Point { side, line, .. } => Some(PrImageUiActionV1::Point {
                    side,
                    line,
                    extend: matches!(event.kind, MouseEventKind::Drag(_))
                        || event.modifiers.contains(KeyModifiers::SHIFT),
                }),
                _ if matches!(event.kind, MouseEventKind::Drag(_)) => None,
                _ => Some(hit.action),
            }
        }
        _ => None,
    }
}
pub fn draw(frame: &mut Frame, area: Rect, view: &mut PrImageViewerV1, theme: crate::theme::Theme) {
    view.clear_layout();
    view.geometry = Some(area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(ratatui::widgets::BorderType::Rounded)
        .title(" Immutable file images • read-only canonical context ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.width < 12 || inner.height < 10 {
        if inner.width > 0 && inner.height > 0 {
            let quit = Rect::new(inner.x, inner.bottom() - 1, inner.width.min(8), 1);
            frame.render_widget(
                Paragraph::new(if inner.width >= 8 { "[q Quit]" } else { "q" })
                    .style(theme.style(crate::theme::SemanticToken::Focus)),
                quit,
            );
            view.hits.push(Hit {
                area: quit,
                action: PrImageUiActionV1::Quit,
            });
        }
        return;
    }
    frame.render_widget(
        Paragraph::new(view.header.as_str()),
        Rect::new(inner.x, inner.y, inner.width, 5),
    );
    let mut x = inner.x;
    let mut y = inner.y + 5;
    let mut body_top = y + 1;
    for (index, (action, label, key)) in CONTROLS.iter().enumerate() {
        if index == 8 {
            body_top = y + 1;
            x = inner.x;
            y = inner.bottom() - 1;
        }
        let text = if *key == '\u{1b}' {
            format!("[Esc {label}]")
        } else {
            format!("[{key} {label}]")
        };
        let width = text.len() as u16;
        if x + width > inner.right() {
            x = inner.x;
            y += 1;
        }
        if (index < 8 && y >= inner.bottom().saturating_sub(3)) || y >= inner.bottom() {
            break;
        }
        let region = Rect::new(x, y, width, 1);
        let style = if view.focus == index {
            theme
                .style(crate::theme::SemanticToken::Focus)
                .add_modifier(Modifier::UNDERLINED)
        } else {
            theme.style(crate::theme::SemanticToken::Reference)
        };
        frame.render_widget(Paragraph::new(text).style(style), region);
        view.hits.push(Hit {
            area: region,
            action: *action,
        });
        x += width + 1;
    }
    let body = Rect::new(
        inner.x,
        body_top,
        inner.width,
        inner.bottom().saturating_sub(body_top + 2),
    );
    let split = view.layout == ImageLayoutV1::Split
        && view.visible == ImageVisibleSideV1::Both
        && body.width >= 100;
    if split {
        let width = body.width / 2;
        draw_side(
            frame,
            Rect::new(body.x, body.y, width, body.height),
            view,
            PrImageSideV1::Old,
            theme,
        );
        draw_side(
            frame,
            Rect::new(body.x + width, body.y, body.width - width, body.height),
            view,
            PrImageSideV1::New,
            theme,
        );
    } else if view.visible == ImageVisibleSideV1::Both {
        let height = body.height / 2;
        draw_side(
            frame,
            Rect::new(body.x, body.y, body.width, height),
            view,
            PrImageSideV1::Old,
            theme,
        );
        draw_side(
            frame,
            Rect::new(body.x, body.y + height, body.width, body.height - height),
            view,
            PrImageSideV1::New,
            theme,
        );
    } else {
        draw_side(
            frame,
            body,
            view,
            if view.visible == ImageVisibleSideV1::Old {
                PrImageSideV1::Old
            } else {
                PrImageSideV1::New
            },
            theme,
        );
    }
    frame.render_widget(
        Paragraph::new(view.notice.as_str())
            .style(theme.style(crate::theme::SemanticToken::Changed)),
        Rect::new(inner.x, inner.bottom() - 2, inner.width, 1),
    );
}
fn draw_side(
    frame: &mut Frame,
    area: Rect,
    view: &mut PrImageViewerV1,
    side: PrImageSideV1,
    theme: crate::theme::Theme,
) {
    if area.height == 0 {
        return;
    }
    let color = if side == PrImageSideV1::Old {
        crate::theme::SemanticToken::Removed
    } else {
        crate::theme::SemanticToken::Added
    };
    let label = if side == PrImageSideV1::Old {
        &view.old_label
    } else {
        &view.new_label
    };
    frame.render_widget(
        Paragraph::new(label.as_str()).style(theme.style(color)),
        Rect::new(area.x, area.y, area.width, 1),
    );
    let start = if view.context && side == view.side {
        view.cursor.saturating_sub(21) as usize
    } else {
        view.scroll
    };
    for offset in 0..usize::from(area.height.saturating_sub(1)) {
        let Some(line) = view.lines(side).get(start + offset) else {
            break;
        };
        let number = line.number;
        let selected = side == view.side
            && number >= view.anchor.min(view.cursor)
            && number <= view.anchor.max(view.cursor);
        let style = if selected {
            theme
                .style(crate::theme::SemanticToken::Selection)
                .add_modifier(Modifier::UNDERLINED)
        } else {
            theme.style(color)
        };
        // Only a single visible line prefix is allocated, never a copied body.
        let prefix = format!(
            "{} {:>6} ",
            if side == PrImageSideV1::Old { "L" } else { "R" },
            number
        );
        let region = Rect::new(area.x, area.y + 1 + offset as u16, area.width, 1);
        let prefix_width = 9.min(region.width);
        frame.render_widget(
            Paragraph::new(prefix).style(style),
            Rect::new(region.x, region.y, prefix_width, 1),
        );
        frame.render_widget(
            Paragraph::new(line.text.as_str())
                .scroll((0, view.horizontal))
                .style(style),
            Rect::new(
                region.x + prefix_width,
                region.y,
                region.width - prefix_width,
                1,
            ),
        );
        view.hits.push(Hit {
            area: region,
            action: PrImageUiActionV1::Point {
                side,
                line: number,
                extend: false,
            },
        });
    }
}
