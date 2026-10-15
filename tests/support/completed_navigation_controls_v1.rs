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
    if label.is_empty() {
        return None;
    }
    let mut found = None;
    for y in ys {
        let (line, columns) = row(frame, y);
        for (byte, _) in line.match_indices(label) {
            let (_, x) = columns.iter().rev().find(|(start, _)| *start <= byte)?;
            if xs.contains(x) {
                if found.is_some() {
                    return None;
                }
                found = Some((*x, y));
            }
        }
    }
    found
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

/// Observe the actual selected row before deciding whether another arrow is
/// needed. A missing selection is allowed initially, ambiguity is never guessed.
pub fn progress_selection(frame: &vt100::Screen, label: &str) -> Option<ProgressSelection> {
    let bounds = owned_box(frame, "Inbox • focused view")?;
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
