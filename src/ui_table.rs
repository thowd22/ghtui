//! Shared list table (TASK-91): aligned columns under a header row, a `▌` selection bar on the
//! Selection background (never reversed text), ellipsis truncation, right-aligned numbers and
//! ages, and columns that leave by priority when the pane is narrow. Views emit cells; this
//! module owns layout, selection painting and one click target per visible row.
//!
//! Row anatomy: `▌` bar slot, status glyph slot (when the table has glyphs), one blank cell,
//! then the columns separated by [`column_gap`] blanks (density, TASK-96). Prose rows (details, logs) start where
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
/// Blank cells between two columns at Comfortable density.
pub const COLUMN_GAP: u16 = 2;
/// Blank cells between two columns at Compact density.
pub const COMPACT_COLUMN_GAP: u16 = 1;

/// Blank cells between two columns at the frame's density (TASK-96).
pub fn column_gap() -> u16 {
    if crate::ui_chrome::density().is_compact() {
        COMPACT_COLUMN_GAP
    } else {
        COLUMN_GAP
    }
}
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

/// A cell's text and an optional role; `None` inherits the row's tone. A cell built from
/// [`Cell::spans`] paints its own styled runs (History graph lanes and ref chips); `text` is
/// then their concatenation, used for the natural width.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cell {
    pub text: String,
    pub role: Option<Role>,
    pub spans: Vec<(String, Style)>,
}
impl Cell {
    pub fn styled(text: impl Into<String>, role: Role) -> Self {
        Self {
            text: text.into(),
            role: Some(role),
            spans: Vec::new(),
        }
    }
    /// Styled runs painted left to right and cut with `…` at the column's edge.
    pub fn spans(spans: Vec<(String, Style)>) -> Self {
        Self {
            text: spans.iter().map(|(text, _)| text.as_str()).collect(),
            role: None,
            spans,
        }
    }
}
impl From<String> for Cell {
    fn from(text: String) -> Self {
        Self {
            text,
            role: None,
            spans: Vec::new(),
        }
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
    // A local Git actor's cell is its name; the role and email stay in the detail rows.
    if identity.kind == crate::identity::ActorKind::Local
        && identity.login.is_none()
        && !identity.name.trim().is_empty()
    {
        return identity.name.clone();
    }
    let label = identity.label();
    label.strip_prefix('@').map(str::to_owned).unwrap_or(label)
}

fn chip_width(identity: &ActorIdentity, mode: ColorMode) -> u16 {
    display_width(&InitialsChip::text(identity, mode)) as u16
}

fn cell_width<A>(row: &TableRow<A>, columns: &[Column], index: usize, mode: ColorMode) -> u16 {
    // An actor column without an identity shows its cell text (a name with no username).
    if let Some(identity) = row.actor.as_ref().filter(|_| columns[index].actor) {
        return chip_width(identity, mode) + 1 + display_width(&actor_text(identity)) as u16;
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
    let gap = column_gap();
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
            + u32::from(gap) * shown.len().saturating_sub(1) as u32
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
            + u32::from(gap) * shown.len().saturating_sub(1) as u32
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
            x = x.saturating_add(width).saturating_add(gap);
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

/// Styled runs from `x` up to `right`; a cut run ends in `…` in its own style.
fn put_spans(buffer: &mut Buffer, x: u16, y: u16, right: u16, spans: &[(String, Style)]) {
    let width = usize::from(right.saturating_sub(x));
    let total: usize = spans
        .iter()
        .map(|(text, _)| display_width(&sanitize_terminal(text)))
        .sum();
    let mut x = x;
    let mut room = width;
    for (text, style) in spans {
        if room == 0 {
            break;
        }
        let text = sanitize_terminal(text);
        let cells = display_width(&text);
        // Only the run that reaches the edge of a row that does not fit is cut, so the
        // ellipsis sits in the last cell.
        let shown = if total > width && cells >= room {
            fit(&format!("{text}…"), room)
        } else {
            text
        };
        x = put(buffer, x, y, right, &shown, *style);
        room = usize::from(right.saturating_sub(x));
    }
}

#[cfg(test)]
pub(crate) fn put_spans_for_tests(
    buffer: &mut Buffer,
    x: u16,
    y: u16,
    right: u16,
    spans: &[(String, Style)],
) {
    put_spans(buffer, x, y, right, spans)
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
    if len > 0 && !area.is_empty() {
        state.selected = state.selected.min(len - 1);
        state.ensure_visible(usize::from(
            area.height.saturating_sub(u16::from(table.header)),
        ));
    }
    draw_table_at(
        frame,
        area,
        table,
        natural,
        len,
        row,
        Some(state.selected),
        state.top,
        theme,
    )
}

/// [`draw_table_with`] for a list whose viewport scrolls apart from its selection (a mouse
/// wheel that moves only the view): rows are painted from `top` as given, and the bar only
/// when `selected` is among them.
#[allow(clippy::too_many_arguments)]
pub fn draw_table_at<A>(
    frame: &mut Frame,
    area: Rect,
    table: &Table<'_>,
    natural: &[u16],
    len: usize,
    row: impl Fn(usize) -> TableRow<A>,
    selected: Option<usize>,
    top: usize,
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
    let painter = TablePainter::new(table, natural, area, theme);
    let buffer = frame.buffer_mut();
    let mut body = area;
    if table.header {
        painter.header(buffer, area.y);
        body = Rect::new(area.x, area.y + 1, area.width, area.height - 1);
    }
    output.body = body;
    output.visible_rows = usize::from(body.height);
    if len == 0 {
        if let Some(empty) = table.empty.filter(|_| !body.is_empty()) {
            painter.empty(buffer, body.y, empty);
        }
        return output;
    }
    for (offset, index) in (top..len).take(output.visible_rows).enumerate() {
        let item = row(index);
        let y = body.y + offset as u16;
        painter.row(buffer, y, &item);
        if Some(index) == selected {
            painter.select(buffer, y);
        }
        // Column titles are not a target; every other painted row is one.
        if item.kind != RowKind::Header {
            output.hits.push(TableHit {
                rect: painter.line(y),
                index,
                action: item.action,
            });
        }
    }
    output
}

/// Paints single table lines into a fixed area for a list that keeps its own scroll and
/// selection (a mixed detail list whose cursor may rest off screen, or rows of more than one
/// kind). [`draw_table_with`] is built on it, so both paint the same columns and bar.
pub struct TablePainter<'t> {
    table: &'t Table<'t>,
    placed: Vec<Placed>,
    area: Rect,
    lead: u16,
    theme: Theme,
}
impl<'t> TablePainter<'t> {
    /// Places the columns for `area`'s width (rows are painted at any `y` the caller picks).
    pub fn new(table: &'t Table<'t>, natural: &[u16], area: Rect, theme: Theme) -> Self {
        let lead = lead(table.glyphs).min(area.width);
        Self {
            table,
            placed: layout(table.columns, natural, area.width - lead),
            area,
            lead,
            theme,
        }
    }
    fn columns_x(&self) -> u16 {
        self.area.x + self.lead
    }
    /// The full-width line at `y`: one row's click target.
    pub fn line(&self, y: u16) -> Rect {
        Rect::new(self.area.x, y, self.area.width, 1)
    }
    /// The column titles at `y`.
    pub fn header(&self, buffer: &mut Buffer, y: u16) {
        let style = self
            .theme
            .role(Role::TextMuted)
            .add_modifier(Modifier::BOLD);
        for column in &self.placed {
            let spec = self.table.columns[column.column];
            put(
                buffer,
                self.columns_x() + column.x,
                y,
                self.area.right(),
                &aligned(spec.title, column.width, spec.align),
                style,
            );
        }
    }
    /// Muted text where the first row would start.
    pub fn empty(&self, buffer: &mut Buffer, y: u16, text: &str) {
        let x = self.columns_x();
        put(
            buffer,
            x,
            y,
            self.area.right(),
            &fit(text, usize::from(self.area.right().saturating_sub(x))),
            self.theme.role(Role::TextMuted),
        );
    }
    /// One row at `y`, without the selection.
    pub fn row<A>(&self, buffer: &mut Buffer, y: u16, item: &TableRow<A>) {
        let (table, theme, area, columns_x) = (self.table, self.theme, self.area, self.columns_x());
        let tone = theme.style(item.tone);
        if let Some(glyph) = item.glyph.filter(|_| table.glyphs && self.lead > 1) {
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
            RowKind::Header => self.header(buffer, y),
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
                    x = x.saturating_add(column_gap()).min(area.right());
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
                for column in &self.placed {
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
                            continue;
                        }
                    }
                    let Some(cell) = item.cells.get(column.column) else {
                        continue;
                    };
                    if !cell.spans.is_empty() {
                        put_spans(buffer, x, y, right, &cell.spans);
                        continue;
                    }
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
    }
    /// The Selection background and the `▌` bar over the line at `y` (accent when focused).
    pub fn select(&self, buffer: &mut Buffer, y: u16) {
        select_line(buffer, self.line(y), self.table.focused, self.theme);
    }
}

/// The selection over one painted line: Selection background on every cell (foregrounds
/// kept) and the `▌` bar in its first cell, accent when the list is focused, dim otherwise.
pub fn select_line(buffer: &mut Buffer, line: Rect, focused: bool, theme: Theme) {
    if line.is_empty() {
        return;
    }
    // Initials and ref chips keep their fill on the selected row, so a person or a branch
    // reads the same in every row and the chip ink stays legible over the Selection
    // background. Without color, reverse-video chips turn back to normal video inside the
    // reversed row so they still stand apart.
    let hues = theme.chip_fills();
    let chips: Vec<(u16, ratatui::style::Color)> = if theme.mode == ColorMode::NoColor {
        Vec::new()
    } else {
        (line.x..line.right())
            .filter_map(|x| {
                let bg = buffer.cell((x, line.y))?.bg;
                hues.contains(&bg).then_some((x, bg))
            })
            .collect()
    };
    let reversed: Vec<u16> = if theme.mode == ColorMode::NoColor {
        (line.x..line.right())
            .filter(|x| {
                buffer
                    .cell((*x, line.y))
                    .is_some_and(|cell| cell.modifier.contains(Modifier::REVERSED))
            })
            .collect()
    } else {
        Vec::new()
    };
    buffer.set_style(line, selection_style(theme));
    for (x, bg) in chips {
        if let Some(cell) = buffer.cell_mut((x, line.y)) {
            cell.set_bg(bg);
        }
    }
    for x in reversed {
        if let Some(cell) = buffer.cell_mut((x, line.y)) {
            cell.modifier.remove(Modifier::REVERSED);
        }
    }
    let bar = theme
        .role(if focused { Role::Accent } else { Role::TextDim })
        .patch(selection_style(theme));
    put(buffer, line.x, line.y, line.right(), SELECTION_BAR, bar);
}

/// A whole-age label in the largest fitting unit: `45s`, `12m`, `3h`, `9d`.
pub fn age_from_seconds(age: u64) -> String {
    match age {
        0..60 => format!("{age}s"),
        60..3600 => format!("{}m", age / 60),
        3600..86400 => format!("{}h", age / 3600),
        // Days up to a month, then months and years, so an old commit never widens AGE.
        86400..2_592_000 => format!("{}d", age / 86400),
        2_592_000..31_536_000 => format!("{}mo", age / 2_592_000),
        _ => format!("{}y", age / 31_536_000),
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
