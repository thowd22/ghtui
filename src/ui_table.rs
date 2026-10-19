//! Shared list table (TASK-91): aligned columns under a header row, a `▌` selection bar on the
//! Selection background (never reversed text), ellipsis truncation, right-aligned numbers and
//! ages, and columns that leave by priority when the pane is narrow. Views emit cells; this
//! module owns layout, selection painting and one click target per visible row.
//!
//! Row anatomy: `▌` bar slot, status glyph slot (when the table has glyphs), one blank cell,
//! then the columns separated by [`COLUMN_GAP`] blanks. Prose rows (details, logs) start where
//! the first column starts so a mixed list stays aligned.
use crate::{
    editor::display_width,
    identity::{ActorIdentity, InitialsChip},
    providers::runner::sanitize_terminal,
    theme::{ColorMode, Role, SemanticToken, Theme},
    ui_chrome::fit,
};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Modifier, Style},
    widgets::Widget,
    Frame,
};

/// The selection bar painted in the first cell of the selected row.
pub const SELECTION_BAR: &str = "▌";
/// Blank cells between two columns.
pub const COLUMN_GAP: u16 = 2;
/// Shown in a cell whose value is not known yet (a running job's duration).
pub const NONE: &str = "—";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Right,
}

/// One column: `min` cells it never shrinks below, `flex` share of spare width (0 keeps its
/// natural width), `priority` (lower stays longer; the highest leaves first when the row is
/// narrow) and alignment. An `actor` column paints the row's initials chip and login.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Column {
    pub title: &'static str,
    pub min: u16,
    pub flex: u16,
    pub priority: u8,
    pub align: Align,
    pub actor: bool,
}
impl Column {
    pub const fn new(title: &'static str, min: u16) -> Self {
        Self {
            title,
            min,
            flex: 0,
            priority: 0,
            align: Align::Left,
            actor: false,
        }
    }
    pub const fn flex(mut self, flex: u16) -> Self {
        self.flex = flex;
        self
    }
    pub const fn priority(mut self, priority: u8) -> Self {
        self.priority = priority;
        self
    }
    pub const fn right(mut self) -> Self {
        self.align = Align::Right;
        self
    }
    pub const fn actor(mut self) -> Self {
        self.actor = true;
        self
    }
}

/// A cell's text and an optional role; `None` inherits the row's tone.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub role: Option<Role>,
}
impl Cell {
    pub fn styled(text: impl Into<String>, role: Role) -> Self {
        Self {
            text: text.into(),
            role: Some(role),
        }
    }
}
impl From<String> for Cell {
    fn from(text: String) -> Self {
        Self { text, role: None }
    }
}
impl From<&str> for Cell {
    fn from(text: &str) -> Self {
        Self::from(text.to_owned())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowKind {
    /// One cell per column.
    Cells,
    /// The column titles, inline in the list (a mixed detail list such as a run's jobs).
    Header,
    /// Free text from the first column to the right edge: details, prose and log lines.
    Text,
}

#[derive(Clone, Debug)]
pub struct TableRow<A> {
    pub kind: RowKind,
    pub cells: Vec<Cell>,
    /// The row's base tone: cell text without its own role uses it.
    pub tone: SemanticToken,
    /// The status glyph in the glyph slot, colored by its own role.
    pub glyph: Option<SemanticToken>,
    /// Painted as initials chip + one blank + login in the `actor` column, or before the text
    /// of a `Text` row.
    pub actor: Option<ActorIdentity>,
    pub action: Option<A>,
}
impl<A> TableRow<A> {
    pub fn cells(cells: Vec<Cell>) -> Self {
        Self {
            kind: RowKind::Cells,
            cells,
            tone: SemanticToken::Normal,
            glyph: None,
            actor: None,
            action: None,
        }
    }
    pub fn header() -> Self {
        Self {
            kind: RowKind::Header,
            ..Self::cells(vec![])
        }
    }
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            kind: RowKind::Text,
            ..Self::cells(vec![Cell::from(text.into())])
        }
    }
    pub fn with_tone(mut self, tone: SemanticToken) -> Self {
        self.tone = tone;
        self
    }
    pub fn with_glyph(mut self, glyph: Option<SemanticToken>) -> Self {
        self.glyph = glyph;
        self
    }
    pub fn with_actor(mut self, actor: Option<ActorIdentity>) -> Self {
        self.actor = actor;
        self
    }
    pub fn with_action(mut self, action: Option<A>) -> Self {
        self.action = action;
        self
    }
}

/// Selection and scroll offset of one list.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListState {
    pub selected: usize,
    pub top: usize,
}
impl ListState {
    /// Scrolls the least distance that keeps `selected` within `visible` rows.
    pub fn ensure_visible(&mut self, visible: usize) {
        if self.selected < self.top {
            self.top = self.selected;
        }
        if visible > 0 && self.selected >= self.top.saturating_add(visible) {
            self.top = self.selected + 1 - visible;
        }
    }
}

/// How a table is drawn; rows come separately.
#[derive(Clone, Copy, Debug)]
pub struct Table<'a> {
    pub columns: &'a [Column],
    /// A sticky header row above the scrolled rows.
    pub header: bool,
    /// The list has keyboard focus: the bar takes the accent instead of the dim role.
    pub focused: bool,
    /// Reserve the status glyph slot.
    pub glyphs: bool,
    /// Muted text in the first row when there are no rows.
    pub empty: Option<&'a str>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableHit<A> {
    pub rect: Rect,
    pub index: usize,
    pub action: Option<A>,
}
#[derive(Clone, Debug)]
pub struct TableRender<A> {
    pub hits: Vec<TableHit<A>>,
    /// Rows the body can show (the sticky header excluded).
    pub visible_rows: usize,
    /// Where the first body row is painted.
    pub body: Rect,
}

/// A column placed in the row: its index in the spec, offset from the first column and width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub column: usize,
    pub x: u16,
    pub width: u16,
}

/// Cells before the first column: the bar, the glyph slot when present, one blank.
pub fn lead(glyphs: bool) -> u16 {
    if glyphs {
        3
    } else {
        2
    }
}

/// The login an actor cell shows after its chip: `@login [bot]` without the `@`.
pub fn actor_text(identity: &ActorIdentity) -> String {
    let label = identity.label();
    label.strip_prefix('@').map(str::to_owned).unwrap_or(label)
}

fn chip_width(identity: &ActorIdentity, mode: ColorMode) -> u16 {
    display_width(&InitialsChip::text(identity, mode)) as u16
}

fn cell_width<A>(row: &TableRow<A>, columns: &[Column], index: usize, mode: ColorMode) -> u16 {
    if columns[index].actor {
        return row.actor.as_ref().map_or(0, |identity| {
            chip_width(identity, mode) + 1 + display_width(&actor_text(identity)) as u16
        });
    }
    row.cells.get(index).map_or(0, |cell| {
        display_width(&sanitize_terminal(&cell.text)) as u16
    })
}

/// The widest title or cell per column over `rows` (only `Cells` rows count).
pub fn natural_widths<'r, A: 'r>(
    columns: &[Column],
    rows: impl IntoIterator<Item = &'r TableRow<A>>,
    mode: ColorMode,
) -> Vec<u16> {
    let mut widths = columns
        .iter()
        .map(|column| display_width(column.title) as u16)
        .collect::<Vec<_>>();
    for row in rows {
        if row.kind != RowKind::Cells {
            continue;
        }
        for (index, width) in widths.iter_mut().enumerate() {
            *width = (*width).max(cell_width(row, columns, index, mode));
        }
    }
    widths
}

/// Places columns in `width` cells. Fixed columns (flex 0) take their natural width, flexible
/// ones at least `min`; while that does not fit, the least important column (highest
/// `priority`, the rightmost on a tie) leaves. Fixed columns less important than every flexible
/// column also leave while the flexible ones cannot reach their natural width. Spare width
/// grows flexible columns up to their natural width by `flex` share; what is left stays blank
/// at the right.
pub fn layout(columns: &[Column], natural: &[u16], width: u16) -> Vec<Placed> {
    let desired = |index: usize| {
        columns[index]
            .min
            .max(natural.get(index).copied().unwrap_or(0))
    };
    let floor = |index: usize| {
        if columns[index].flex == 0 {
            desired(index)
        } else {
            columns[index].min
        }
    };
    let mut shown = (0..columns.len()).collect::<Vec<_>>();
    let need = |shown: &[usize]| {
        shown
            .iter()
            .map(|index| u32::from(floor(*index)))
            .sum::<u32>()
            + u32::from(COLUMN_GAP) * shown.len().saturating_sub(1) as u32
    };
    let least = |shown: &[usize]| {
        (0..shown.len())
            .max_by_key(|position| (columns[shown[*position]].priority, *position))
            .unwrap()
    };
    while shown.len() > 1 && need(&shown) > u32::from(width) {
        shown.remove(least(&shown));
    }
    // A fixed column less important than every flexible one leaves before a flexible column
    // is cut below its natural width (the run id goes before a workflow name is shortened).
    let flexible = shown
        .iter()
        .filter(|index| columns[**index].flex > 0)
        .map(|index| columns[*index].priority)
        .max();
    let full = |shown: &[usize]| {
        shown
            .iter()
            .map(|index| u32::from(desired(*index)))
            .sum::<u32>()
            + u32::from(COLUMN_GAP) * shown.len().saturating_sub(1) as u32
    };
    while shown.len() > 1 && full(&shown) > u32::from(width) {
        let position = least(&shown);
        let column = columns[shown[position]];
        if column.flex > 0 || flexible.is_some_and(|priority| column.priority <= priority) {
            break;
        }
        shown.remove(position);
    }
    let mut widths = shown.iter().map(|index| floor(*index)).collect::<Vec<_>>();
    let mut spare = u32::from(width).saturating_sub(need(&shown));
    loop {
        let growing = (0..shown.len())
            .filter(|position| {
                columns[shown[*position]].flex > 0 && widths[*position] < desired(shown[*position])
            })
            .collect::<Vec<_>>();
        if spare == 0 || growing.is_empty() {
            break;
        }
        let shares = growing
            .iter()
            .map(|position| u32::from(columns[shown[*position]].flex))
            .sum::<u32>();
        let mut given = 0;
        for position in &growing {
            let room = u32::from(desired(shown[*position]) - widths[*position]);
            let share = (spare * u32::from(columns[shown[*position]].flex) / shares)
                .max(1)
                .min(room)
                .min(spare - given);
            widths[*position] += share as u16;
            given += share;
        }
        if given == 0 {
            break;
        }
        spare -= given;
    }
    // One oversized column left alone is cut to the row.
    if shown.len() == 1 {
        widths[0] = widths[0].min(width);
    }
    let mut x = 0u16;
    shown
        .iter()
        .zip(widths)
        .map(|(column, width)| {
            let placed = Placed {
                column: *column,
                x,
                width,
            };
            x = x.saturating_add(width).saturating_add(COLUMN_GAP);
            placed
        })
        .collect()
}

/// The status glyph for the glyph slot; neutral rows keep a blank, skipped/muted rows a ring.
pub fn glyph_symbol(token: SemanticToken) -> &'static str {
    match token {
        SemanticToken::Normal => " ",
        SemanticToken::Muted => "○",
        token => token.symbol(),
    }
}

/// Text cut to `width` with an ellipsis and padded on the left when right-aligned.
pub fn aligned(text: &str, width: u16, align: Align) -> String {
    let text = fit(text, usize::from(width));
    match align {
        Align::Left => text,
        Align::Right => {
            let pad = usize::from(width).saturating_sub(display_width(&text));
            format!("{}{text}", " ".repeat(pad))
        }
    }
}

/// The Selection background (or reverse video without colors) laid over a row; it keeps every
/// cell's foreground so roles stay legible.
pub fn selection_style(theme: Theme) -> Style {
    let selection = theme.role(Role::Selection);
    Style {
        bg: selection.bg,
        add_modifier: selection.add_modifier,
        ..Style::default()
    }
}

fn put(buffer: &mut Buffer, x: u16, y: u16, right: u16, text: &str, style: Style) -> u16 {
    if x >= right {
        return x;
    }
    let (end, _) = buffer.set_stringn(x, y, text, usize::from(right - x), style);
    end
}

fn draw_actor(
    buffer: &mut Buffer,
    theme: Theme,
    identity: &ActorIdentity,
    area: Rect,
    style: Style,
) -> u16 {
    if area.is_empty() {
        return area.x;
    }
    let chip = chip_width(identity, theme.mode).min(area.width);
    InitialsChip { identity, theme }.render(Rect::new(area.x, area.y, chip, 1), buffer);
    let x = area.x + chip + 1;
    if x >= area.right() {
        return area.right();
    }
    let login = fit(
        &sanitize_terminal(&actor_text(identity)),
        usize::from(area.right() - x),
    );
    put(buffer, x, area.y, area.right(), &login, style)
}

/// Draws a header and the rows `[state.top..]` into `area` and returns one hit per painted
/// body row except inline header rows (index and the row's action). `state.top` is moved so the selection stays visible.
pub fn draw_table<A: Clone>(
    frame: &mut Frame,
    area: Rect,
    table: &Table<'_>,
    rows: &[TableRow<A>],
    state: &mut ListState,
    theme: Theme,
) -> TableRender<A> {
    let natural = natural_widths(table.columns, rows, theme.mode);
    draw_table_with(
        frame,
        area,
        table,
        &natural,
        rows.len(),
        |index| rows[index].clone(),
        state,
        theme,
    )
}

/// [`draw_table`] for long lists that build only the painted rows: `natural` comes from
/// [`natural_widths`] over the `Cells` rows, `row(index)` is called for visible indexes only.
#[allow(clippy::too_many_arguments)]
pub fn draw_table_with<A>(
    frame: &mut Frame,
    area: Rect,
    table: &Table<'_>,
    natural: &[u16],
    len: usize,
    row: impl Fn(usize) -> TableRow<A>,
    state: &mut ListState,
    theme: Theme,
) -> TableRender<A> {
    let mut output = TableRender {
        hits: vec![],
        visible_rows: 0,
        body: area,
    };
    if area.is_empty() {
        return output;
    }
    let lead = lead(table.glyphs).min(area.width);
    let columns_x = area.x + lead;
    let placed = layout(table.columns, natural, area.width - lead);
    let buffer = frame.buffer_mut();
    let header_style = theme.role(Role::TextMuted).add_modifier(Modifier::BOLD);
    let draw_header = |buffer: &mut Buffer, y: u16| {
        for column in &placed {
            let spec = table.columns[column.column];
            put(
                buffer,
                columns_x + column.x,
                y,
                area.right(),
                &aligned(spec.title, column.width, spec.align),
                header_style,
            );
        }
    };
    let mut body = area;
    if table.header {
        draw_header(buffer, area.y);
        body = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
    }
    output.body = body;
    output.visible_rows = usize::from(body.height);
    if len == 0 {
        if let Some(empty) = table.empty.filter(|_| !body.is_empty()) {
            put(
                buffer,
                columns_x,
                body.y,
                body.right(),
                &fit(empty, usize::from(body.right().saturating_sub(columns_x))),
                theme.role(Role::TextMuted),
            );
        }
        return output;
    }
    state.selected = state.selected.min(len - 1);
    state.ensure_visible(output.visible_rows);
    for (offset, index) in (state.top..len).take(output.visible_rows).enumerate() {
        let item = row(index);
        let y = body.y + offset as u16;
        let rect = Rect::new(body.x, y, body.width, 1);
        let tone = theme.style(item.tone);
        if let Some(glyph) = item.glyph.filter(|_| table.glyphs && lead > 1) {
            put(
                buffer,
                area.x + 1,
                y,
                area.right(),
                glyph_symbol(glyph),
                theme.style(glyph),
            );
        }
        match item.kind {
            RowKind::Header => draw_header(buffer, y),
            RowKind::Text => {
                let mut x = columns_x;
                if let Some(identity) = &item.actor {
                    x = draw_actor(
                        buffer,
                        theme,
                        identity,
                        Rect::new(x, y, area.right().saturating_sub(x), 1),
                        theme.role(Role::Text),
                    );
                    x = x.saturating_add(COLUMN_GAP).min(area.right());
                }
                if let Some(cell) = item.cells.first() {
                    let text = fit(
                        &sanitize_terminal(&cell.text),
                        usize::from(area.right().saturating_sub(x)),
                    );
                    put(
                        buffer,
                        x,
                        y,
                        area.right(),
                        &text,
                        cell.role.map_or(tone, |role| theme.role(role)),
                    );
                }
            }
            RowKind::Cells => {
                for column in &placed {
                    let spec = table.columns[column.column];
                    let x = columns_x + column.x;
                    let right = (x + column.width).min(area.right());
                    if spec.actor {
                        if let Some(identity) = &item.actor {
                            draw_actor(
                                buffer,
                                theme,
                                identity,
                                Rect::new(x, y, right.saturating_sub(x), 1),
                                theme.role(Role::Text),
                            );
                        }
                        continue;
                    }
                    let Some(cell) = item.cells.get(column.column) else {
                        continue;
                    };
                    put(
                        buffer,
                        x,
                        y,
                        right,
                        &aligned(&sanitize_terminal(&cell.text), column.width, spec.align),
                        cell.role.map_or(tone, |role| theme.role(role)),
                    );
                }
            }
        }
        if index == state.selected {
            buffer.set_style(rect, selection_style(theme));
            let bar = theme
                .role(if table.focused {
                    Role::Accent
                } else {
                    Role::TextDim
                })
                .patch(selection_style(theme));
            put(buffer, area.x, y, area.right(), SELECTION_BAR, bar);
        }
        // Column titles are not a target; every other painted row is one.
        if item.kind != RowKind::Header {
            output.hits.push(TableHit {
                rect,
                index,
                action: item.action,
            });
        }
    }
    output
}

/// A whole-age label in the largest fitting unit: `45s`, `12m`, `3h`, `9d`.
pub fn age_from_seconds(age: u64) -> String {
    match age {
        0..60 => format!("{age}s"),
        60..3600 => format!("{}m", age / 60),
        3600..86400 => format!("{}h", age / 3600),
        _ => format!("{}d", age / 86400),
    }
}
/// The age of a GitHub UTC timestamp at `now` (seconds since the epoch), `None` when the
/// timestamp does not parse.
pub fn age_at(timestamp: &str, now: u64) -> Option<String> {
    crate::github_workspace::timestamp_seconds(timestamp)
        .map(|created| age_from_seconds(now.saturating_sub(created)))
}
/// The one relative-age helper for list cells: `12m`, `3h`, `9d`, or `—` when unknown.
pub fn relative_age(timestamp: &str) -> String {
    age_at(timestamp, crate::storage::now()).unwrap_or_else(|| NONE.into())
}
/// Elapsed time between two GitHub UTC timestamps: `45s`, `8m 29s`, `1h 05m`.
pub fn duration(started: &str, completed: &str) -> Option<String> {
    use crate::github_workspace::timestamp_seconds;
    let (start, end) = (timestamp_seconds(started)?, timestamp_seconds(completed)?);
    let seconds = end.checked_sub(start)?;
    Some(match seconds {
        0..60 => format!("{seconds}s"),
        60..3600 => format!("{}m {:02}s", seconds / 60, seconds % 60),
        _ => format!("{}h {:02}m", seconds / 3600, seconds % 3600 / 60),
    })
}
/// The 7-character list form of a commit id; anything that is not a hex id stays as is.
pub fn short_sha(sha: &str) -> &str {
    if sha.len() > 7 && sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        &sha[..7]
    } else {
        sha
    }
}

#[cfg(test)]
#[path = "ui_table_tests.rs"]
mod design_table_v91_tests;
