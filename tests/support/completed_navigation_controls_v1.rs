//! Test-only observations of actual completed cursorless frames. Geometry and
//! labels are input hints, not Git/resource identity or permission authority.
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
        .find(|x| frame.cell(top, *x).is_some_and(|c| c.contents() == "┌"))?;
    let right =
        (label_x..width).find(|x| frame.cell(top, *x).is_some_and(|c| c.contents() == "┐"))?;
    let bottom = (top + 1..height).find(|y| {
        frame.cell(*y, left).is_some_and(|c| c.contents() == "└")
            && frame.cell(*y, right).is_some_and(|c| c.contents() == "┘")
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
        || text.contains("[Esc Cancel]")
        || text.contains("Reading original local source")
        || text.contains("Restoring original repository source")
    {
        return None;
    }
    let bounds = owned_box(frame, "Repository • focused view")?;
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
    let (graph_x, graph_y) = unique_position(frame, "[g Graph]", body.clone(), columns.clone())?;
    if !frame.cell(graph_y, graph_x)?.inverse() {
        return None;
    }
    let (x, y) = unique_position(frame, subject, body, columns)?;
    frame.cell(y, x)?.inverse().then_some((x, y))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActionsPage {
    pub current: usize,
    pub total: usize,
    pub previous: (u16, u16),
    pub next: (u16, u16),
    controls_top: u16,
    controls_bottom: u16,
    bounds: OwnedBox,
}

pub fn actions_page(completed: &CursorlessFrames, after: usize) -> Option<ActionsPage> {
    if completed.end <= after {
        return None;
    }
    let frame = completed.last.as_ref()?;
    if frame.contents().contains("Action unavailable") {
        return None;
    }
    let bounds = owned_box(frame, "Actions • focused view")?;
    let ys = bounds.top + 1..bounds.bottom;
    let xs = bounds.left + 1..bounds.right;
    let (_, page_y) = unique_position(frame, " • [] change control page", ys.clone(), xs.clone())?;
    let (page_row, _) = row(frame, page_y);
    let marker = page_row.find(" • [] change control page")?;
    let token = page_row[..marker].split_whitespace().last()?;
    let (current, total) = token.split_once('/')?;
    let current: usize = current.parse().ok()?;
    let total: usize = total.parse().ok()?;
    if current == 0 || current > total || total > 30 {
        return None;
    }
    Some(ActionsPage {
        current,
        total,
        previous: unique_position(frame, "[← Controls]", ys.clone(), xs.clone())?,
        next: unique_position(frame, "[Controls →]", ys, xs)?,
        controls_top: page_y + 1,
        // Existing shared Actions layout paints at most four control rows.
        controls_bottom: (page_y + 5).min(bounds.bottom),
        bounds,
    })
}

pub fn actions_control_position(
    completed: &CursorlessFrames,
    page: &ActionsPage,
    label: &str,
) -> Option<(u16, u16)> {
    if actions_page(completed, 0)? != *page {
        return None;
    }
    unique_position(
        completed.last.as_ref()?,
        label,
        page.controls_top..page.controls_bottom,
        page.bounds.left + 1..page.bounds.right,
    )
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
        || text.contains("[b Cancel read]")
    {
        return None;
    }
    unique_position(
        frame,
        job,
        page.controls_bottom..page.bounds.bottom,
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
    progress_selection_in(frame, "Inbox • focused view", label)
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
            if frame.cell(y, x).is_none_or(|cell| cell.contents() != "┌") {
                continue;
            }
            if let Some(right) = (x + 1..owner.right).find(|right| {
                frame
                    .cell(y, *right)
                    .is_some_and(|cell| cell.contents() == "┐")
            }) {
                if (y + 1..owner.bottom).any(|bottom| {
                    frame
                        .cell(bottom, x)
                        .is_some_and(|cell| cell.contents() == "└")
                        && frame
                            .cell(bottom, right)
                            .is_some_and(|cell| cell.contents() == "┘")
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
