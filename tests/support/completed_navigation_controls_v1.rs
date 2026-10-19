//! Test-only observations of actual completed cursorless frames. Geometry and
//! labels are input hints, not Git/resource identity or permission authority.
use super::controls::{
    cell_is_corner, footer_row, locate_token_in_row, main_footer_visible, view_title, Control,
    Corner,
};
use super::cursorless_frames::CursorlessFrames;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OwnedBox {
    left: u16,
    right: u16,
    top: u16,
    bottom: u16,
}

fn row(frame: &vt100::Screen, y: u16) -> (String, Vec<(usize, u16)>) {
    let mut text = String::new();
    let mut columns = Vec::new();
    for x in 0..frame.size().1 {
        let cell = frame.cell(y, x).expect("Observed frame dimensions");
        if cell.is_wide_continuation() {
            continue;
        }
        columns.push((text.len(), x));
        let value = cell.contents();
        if value.is_empty() {
            text.push(' ');
        } else {
            text.push_str(&value);
        }
    }
    (text, columns)
}

fn unique_position(
    frame: &vt100::Screen,
    label: &str,
    ys: std::ops::Range<u16>,
    xs: std::ops::Range<u16>,
) -> Option<(u16, u16)> {
    checked_position(frame, label, ys, xs).ok().flatten()
}

fn checked_position(
    frame: &vt100::Screen,
    label: &str,
    ys: std::ops::Range<u16>,
    xs: std::ops::Range<u16>,
) -> Result<Option<(u16, u16)>, ()> {
    if label.is_empty() {
        return Err(());
    }
    let mut found = None;
    for y in ys {
        let (line, columns) = row(frame, y);
        for (byte, _) in line.match_indices(label) {
            let (_, x) = columns
                .iter()
                .rev()
                .find(|(start, _)| *start <= byte)
                .ok_or(())?;
            if xs.contains(x) {
                if found.is_some() {
                    return Err(());
                }
                found = Some((*x, y));
            }
        }
    }
    Ok(found)
}

fn owned_box(frame: &vt100::Screen, title: &str) -> Option<OwnedBox> {
    let (height, width) = frame.size();
    let (label_x, top) = unique_position(frame, title, 0..height, 0..width)?;
    let left = (0..label_x)
        .rev()
        .find(|x| cell_is_corner(frame, top, *x, Corner::TopLeft))?;
    let right = (label_x..width).find(|x| cell_is_corner(frame, top, *x, Corner::TopRight))?;
    let bottom = (top + 1..height).find(|y| {
        cell_is_corner(frame, *y, left, Corner::BottomLeft)
            && cell_is_corner(frame, *y, right, Corner::BottomRight)
    })?;
    Some(OwnedBox {
        left,
        right,
        top,
        bottom,
    })
}

pub fn saved_graph_position(
    completed: &CursorlessFrames,
    after: usize,
    sha: &str,
    subject: &str,
    status: &str,
) -> Option<(u16, u16)> {
    if completed.end <= after
        || !matches!(sha.len(), 40 | 64)
        || !sha.bytes().all(|byte| byte.is_ascii_hexdigit())
        || status.is_empty()
    {
        return None;
    }
    let frame = completed.last.as_ref()?;
    let text = frame.contents();
    if !text.contains(status)
        || super::controls::locate_control(frame, "[Esc Cancel]").is_some()
        || text.contains("Reading original local source")
        || text.contains("Restoring original repository source")
    {
        return None;
    }
    let bounds = owned_box(frame, &view_title("Repository"))?;
    let mut saved = text.lines().filter_map(|line| {
        line.trim_start_matches('│')
            .trim_start()
            .strip_prefix("Saved: ")
            .map(|tail| tail.split_whitespace().next().unwrap_or_default())
    });
    if saved.next() != Some(sha) || saved.next().is_some() {
        return None;
    }
    let body = bounds.top + 1..bounds.bottom;
    let columns = bounds.left + 1..bounds.right;
    // The selected Graph mode: a reversed bracketed button, or the selected tab of the
    // Repository tab strip (its label filled bold, or reversed without color).
    let graph_selected = match unique_position(frame, "[g Graph]", body.clone(), columns.clone()) {
        Some((graph_x, graph_y)) => frame.cell(graph_y, graph_x)?.inverse(),
        None => {
            let (graph_x, graph_y) =
                unique_position(frame, " g Graph ", body.clone(), columns.clone())?;
            frame
                .cell(graph_y, graph_x + 3)
                .is_some_and(|cell| cell.inverse() || cell.bold())
        }
    };
    if !graph_selected {
        return None;
    }
    let (x, y) = unique_position(frame, subject, body, columns)?;
    frame.cell(y, x)?.inverse().then_some((x, y))
}

/// One completed main Actions frame: its owned content box and the key bar in the last row.
/// There is no control pager: every enabled control is a key-bar token or, when it does not
/// fit, a command-palette entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionsPage {
    pub footer: u16,
    bounds: OwnedBox,
}

pub fn actions_page(completed: &CursorlessFrames, after: usize) -> Option<ActionsPage> {
    if completed.end <= after {
        return None;
    }
    let frame = completed.last.as_ref()?;
    // A loading frame (`Esc Cancel` in the bar) is not idle: input would race the read.
    if frame.contents().contains("Action unavailable")
        || !main_footer_visible(frame)
        || locate_token_in_row(frame, footer_row(frame), "Esc Cancel").is_some()
    {
        return None;
    }
    // The status line can repeat the title; the border title is followed by `─`.
    let bounds = owned_box(frame, &format!("{} ─", view_title("Actions")))
        .or_else(|| owned_box(frame, &view_title("Actions")))?;
    let footer = footer_row(frame);
    (footer > bounds.bottom).then_some(ActionsPage { footer, bounds })
}

/// The key-bar token for `label` in a completed page: `[l Logs]` is the token `l Logs`, and an
/// unbracketed fragment such as `Acknowledge` names the one token containing it. Ambiguous
/// fragments and labels painted only in the body never supply a coordinate.
pub fn actions_control_position(
    completed: &CursorlessFrames,
    page: &ActionsPage,
    label: &str,
) -> Option<(u16, u16)> {
    if actions_page(completed, 0)? != *page {
        return None;
    }
    let frame = completed.last.as_ref()?;
    let wanted = Control::parse(label).footer_form();
    if wanted.trim().is_empty() {
        return None;
    }
    // Tokens are separated by two spaces; an exact token wins over a fragment match, and two
    // matching tokens are ambiguous.
    let (line, columns) = row(frame, page.footer);
    let mut tokens = Vec::new();
    let mut start = 0;
    for token in line.split("  ") {
        let byte = start;
        start += token.len() + 2;
        let trimmed = token.trim();
        if !trimmed.is_empty() {
            tokens.push((byte + token.find(trimmed)?, trimmed));
        }
    }
    let exact = tokens
        .iter()
        .filter(|(_, token)| token.eq_ignore_ascii_case(&wanted))
        .collect::<Vec<_>>();
    let fragment = tokens
        .iter()
        .filter(|(_, token)| token.contains(wanted.as_str()))
        .collect::<Vec<_>>();
    // Below 100 columns a token can be the compact start of the caption (`Acknowledge`).
    // or the same key with a shorter caption of the same initial (`P Pubs`).
    let parsed = Control::parse(label);
    let keyed = |token: &str| {
        parsed.key.as_deref().is_some_and(|key| {
            token
                .strip_prefix(key)
                .and_then(|caption| caption.strip_prefix(' '))
                .is_some_and(|caption| {
                    caption.len() < parsed.text.len()
                        && caption.chars().next().map(|c| c.to_ascii_lowercase())
                            == parsed.text.chars().next().map(|c| c.to_ascii_lowercase())
                })
        })
    };
    let compact = tokens
        .iter()
        .filter(|(_, token)| token.len() >= 4 && wanted.starts_with(*token) || keyed(token))
        .collect::<Vec<_>>();
    let matches = if !exact.is_empty() {
        exact
    } else if !fragment.is_empty() {
        fragment
    } else {
        compact
    };
    let [(offset, _)] = matches.as_slice() else {
        return None;
    };
    columns
        .iter()
        .rev()
        .find(|(start, _)| start <= offset)
        .map(|(_, x)| (*x, page.footer))
}

/// A retained page's pending repaint still contains the old body and offers only
/// `b Cancel read` in the key bar. Neither can supply a new click target.
pub fn scoped_actions_ready_control_position_v4(
    completed: &CursorlessFrames,
    after: usize,
    label: &str,
) -> Option<(u16, u16)> {
    if label.len() <= 2
        || !label.starts_with('[')
        || !label.ends_with(']')
        || label[1..label.len() - 1]
            .chars()
            .any(|ch| ch.is_control() || matches!(ch, '[' | ']'))
    {
        return None;
    }
    let page = actions_page(completed, after)?;
    let frame = completed.last.as_ref()?;
    let text = frame.contents();
    if !text.contains("Original scoped Actions target verified")
        || text.contains("Original current read pending")
        || locate_token_in_row(frame, page.footer, "b Cancel read").is_some()
    {
        return None;
    }
    actions_control_position(completed, &page, label)
}

/// Run and Jobs retain the same job labels but paint them at different rows.
/// Only the completed Jobs layout may supply the next mouse coordinate.
pub fn scoped_actions_jobs_position_v2(
    completed: &CursorlessFrames,
    after: usize,
    job: &str,
    head: &str,
) -> Option<(u16, u16)> {
    let page = actions_page(completed, after)?;
    let frame = completed.last.as_ref()?;
    let text = frame.contents();
    if !matches!(head.len(), 40 | 64)
        || !head.bytes().all(|b| b.is_ascii_hexdigit())
        || !text.contains(head)
        || !text.contains("/ Jobs and steps /")
        || !text.contains("Retained observed attempt")
        || locate_token_in_row(frame, page.footer, "b Cancel read").is_some()
    {
        return None;
    }
    unique_position(
        frame,
        job,
        page.bounds.top + 1..page.bounds.bottom,
        page.bounds.left + 1..page.bounds.right,
    )
}

#[derive(Debug, PartialEq, Eq)]
pub struct ActionsControlFocusV2 {
    pub target_focused: bool,
    /// Observe the actual host-wide underline, including header/footer hits.
    /// Tab can legitimately leave the Actions box before returning to it.
    pub underlined: Vec<(u16, u16)>,
}

pub fn actions_control_focus_v2(
    completed: &CursorlessFrames,
    page: &ActionsPage,
    label: &str,
) -> Option<ActionsControlFocusV2> {
    let (x, y) = actions_control_position(completed, page, label)?;
    let frame = completed.last.as_ref()?;
    let mut underlined = Vec::new();
    for row in 0..frame.size().0 {
        for column in 0..frame.size().1 {
            if frame.cell(row, column)?.underline() {
                underlined.push((column, row));
            }
        }
    }
    Some(ActionsControlFocusV2 {
        target_focused: frame.cell(y, x)?.underline(),
        underlined,
    })
}

/// Coordinates are only input hints from one complete frame, never resource
/// identity. Reject a nested modal even when its parent's old footer survives.
pub fn owned_action_position(
    frame: &vt100::Screen,
    title: &str,
    label: &str,
    forbidden: &[&str],
) -> Option<(u16, u16)> {
    let text = frame.contents();
    if forbidden.iter().any(|title| text.contains(title)) {
        return None;
    }
    let bounds = owned_box(frame, title)?;
    unique_position(
        frame,
        label,
        bounds.top + 1..bounds.bottom,
        bounds.left + 1..bounds.right,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgressSelection {
    pub target: (u16, u16),
    pub selected_row: Option<u16>,
}
impl ProgressSelection {
    pub fn acknowledges_change_from(&self, previous: Option<u16>) -> bool {
        self.selected_row.is_some() && self.selected_row != previous
    }
}

/// Observe the actual selected row before deciding whether another arrow is
/// needed. A missing selection is allowed initially, ambiguity is never guessed.
pub fn progress_selection(frame: &vt100::Screen, label: &str) -> Option<ProgressSelection> {
    progress_selection_in(frame, &view_title("Inbox"), label)
}
/// One observer/parser, with an explicit owning view, never title guessing.
pub fn progress_selection_in(
    frame: &vt100::Screen,
    title: &str,
    label: &str,
) -> Option<ProgressSelection> {
    let bounds = owned_box(frame, title)?;
    let target = unique_position(
        frame,
        label,
        bounds.top + 1..bounds.bottom,
        bounds.left + 1..bounds.right,
    )?;
    let mut selected_row = None;
    for y in bounds.top + 1..bounds.bottom {
        let (line, _) = row(frame, y);
        let text = line.trim_start_matches('│').trim_start();
        if text.starts_with("> ") || text.starts_with("› ") {
            if selected_row.is_some() {
                return None;
            }
            selected_row = Some(y);
        }
    }
    Some(ProgressSelection {
        target,
        selected_row,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusAttributeV1 {
    Inverse,
    Underline,
}
impl FocusAttributeV1 {
    fn matches(self, frame: &vt100::Screen, point: (u16, u16)) -> bool {
        frame.cell(point.1, point.0).is_some_and(|cell| match self {
            Self::Inverse => cell.inverse(),
            Self::Underline => cell.underline(),
        })
    }
}
#[derive(Debug)]
pub struct OwnedFocusV1 {
    pub target: (u16, u16),
    pub target_focused: bool,
    owner: OwnedBox,
    title: String,
    identity: Option<(bool, usize, (u16, u16))>,
}
impl OwnedFocusV1 {
    pub fn acknowledges_change_from(&self, previous: &Self) -> bool {
        self.owner == previous.owner
            && self.title == previous.title
            && self.target == previous.target
            && self.identity.is_some()
            && self.identity != previous.identity
    }
}

/// Caller-declared UI anchors, never body/row styling, identify focus. Text
/// notices are used only by widgets that visibly name their Query/Rows focus.
pub fn owned_focus_v1(
    frame: &vt100::Screen,
    title: &str,
    target: &str,
    attribute: FocusAttributeV1,
    controls: &[&str],
    notices: &[&str],
) -> Option<OwnedFocusV1> {
    // The review browser has nineteen individually focusable controls. Keep
    // the observation bounded without excluding its complete declared toolbar.
    if !controls.contains(&target) || controls.len().saturating_add(notices.len()) > 32 {
        return None;
    }
    let owner = topmost_box_v1(frame, title)?;
    let target = unique_position(
        frame,
        target,
        owner.top + 1..owner.bottom,
        owner.left + 1..owner.right,
    )?;
    let mut identity = None;
    for (notice, labels) in [(false, controls), (true, notices)] {
        for (index, label) in labels.iter().enumerate() {
            let position = checked_position(
                frame,
                label,
                if notice { owner.top } else { owner.top + 1 }..owner.bottom,
                owner.left + 1..owner.right,
            )
            .ok()?;
            if let Some(point) = position.filter(|point| notice || attribute.matches(frame, *point))
            {
                if identity.replace((notice, index, point)).is_some() {
                    return None;
                }
            }
        }
    }
    Some(OwnedFocusV1 {
        target,
        target_focused: attribute.matches(frame, target),
        owner,
        title: title.into(),
        identity,
    })
}
fn topmost_box_v1(frame: &vt100::Screen, title: &str) -> Option<OwnedBox> {
    let owner = owned_box(frame, title)?;
    // A genuine nested complete overlay owns input instead of this underlay.
    for y in owner.top + 1..owner.bottom {
        for x in owner.left + 1..owner.right {
            if !cell_is_corner(frame, y, x, Corner::TopLeft) {
                continue;
            }
            if let Some(right) = (x + 1..owner.right)
                .find(|right| cell_is_corner(frame, y, *right, Corner::TopRight))
            {
                if (y + 1..owner.bottom).any(|bottom| {
                    cell_is_corner(frame, bottom, x, Corner::BottomLeft)
                        && cell_is_corner(frame, bottom, right, Corner::BottomRight)
                }) {
                    return None;
                }
            }
        }
    }
    Some(owner)
}

#[derive(Debug)]
pub struct OwnedDetailsV1 {
    pub point: (u16, u16),
    owner: OwnedBox,
    title: String,
    selected: String,
    selected_at: (u16, u16),
    rows: Vec<String>,
    first_field: Option<(usize, String)>,
}
impl OwnedDetailsV1 {
    pub fn contains(&self, label: &str) -> bool {
        self.rows.iter().any(|row| row.contains(label))
    }
    pub fn acknowledges_change_from(&self, previous: &Self) -> bool {
        self.owner == previous.owner
            && self.title == previous.title
            && self.selected == previous.selected
            && self.selected_at == previous.selected_at
            && self.point == previous.point
            && self.rows != previous.rows
            && (self.first_field.is_some()
                && previous.first_field.is_some()
                && self.first_field != previous.first_field
                || self.rows.iter().enumerate().any(|(index, line)| {
                    !line.is_empty()
                        && self.rows.iter().filter(|row| *row == line).count() == 1
                        && previous.rows.iter().filter(|row| *row == line).count() == 1
                        && previous
                            .rows
                            .iter()
                            .position(|old| old == line)
                            .is_some_and(|old_index| old_index != index)
                }))
    }
}
pub fn owned_details_v1(
    frame: &vt100::Screen,
    title: &str,
    selected: &str,
) -> Option<OwnedDetailsV1> {
    let owner = topmost_box_v1(frame, title)?;
    let up = unique_position(
        frame,
        "[Ctrl-PgUp Detail]",
        owner.top + 1..owner.bottom,
        owner.left + 1..owner.right,
    )?;
    let down = unique_position(
        frame,
        "[Ctrl-PgDn Detail]",
        owner.top + 1..owner.bottom,
        owner.left + 1..owner.right,
    )?;
    if up.1 != down.1 || up.1 + 1 >= owner.bottom.saturating_sub(1) {
        return None;
    }
    let selected_at = unique_position(
        frame,
        selected,
        owner.top + 1..up.1,
        owner.left + 1..owner.right,
    )?;
    let mut rows = Vec::new();
    for y in up.1 + 1..owner.bottom - 1 {
        let mut text = String::new();
        for x in owner.left + 1..owner.right {
            let cell = frame.cell(y, x)?;
            if cell.is_wide_continuation() {
                continue;
            }
            let value = cell.contents();
            if value.is_empty() {
                text.push(' ');
            } else {
                text.push_str(&value);
            }
        }
        rows.push(text.trim().to_owned());
    }
    let first_field = rows.iter().enumerate().find_map(|(index, line)| {
        line.split_once(':')
            .map(|(label, _)| (index, label.to_owned()))
    });
    Some(OwnedDetailsV1 {
        point: (owner.left + 1, up.1 + 1),
        owner,
        title: title.into(),
        selected: selected.into(),
        selected_at,
        rows,
        first_field,
    })
}
