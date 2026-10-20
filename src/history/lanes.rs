//! Pure lane layout for the History graph (TASK-99.1).
//!
//! Commits arrive in display order (children before parents, as `git log
//! --topo-order` or `--date-order` emits them). Each commit occupies one lane
//! column; its first parent continues that lane, and later parents join an
//! existing lane or start a new one on the same row (`●─╮`, `├─●`). Lanes that
//! wait for the same commit converge on its row (`●─╯`). Freed columns are
//! reused, except the column a root or shallow boundary just vacated, which
//! stays blank for one row so an unrelated tip below never looks connected.
//!
//! The layout is incremental: pushing rows `0..N` and then `N..M` yields the
//! same rows as pushing `0..M`, so lane columns and color slots stay stable
//! while the view pages. Rows carry a color slot per cell; the renderer maps
//! slots onto the theme's lane palette.

use std::collections::HashSet;

/// Lane frontier bound; a wider history is an error, never a silent path loss.
pub const HISTORY_MAX_LANES: usize = 128;
/// Default number of distinct lane color slots.
pub const HISTORY_LANE_PALETTE: usize = 8;
/// A connector crossing a lane that continues below.
pub const CROSSING: char = '┿';

/// One commit in display order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LaneCommit {
    pub id: String,
    /// Parent ids in Git order; the first parent continues the commit's lane.
    pub parents: Vec<String>,
    /// Ref labels in display priority; the first names a lane that starts here.
    pub labels: Vec<String>,
    /// A shallow boundary: Git knows no parents beyond this commit.
    pub boundary: bool,
}

impl LaneCommit {
    pub fn new<P: Into<String>>(
        id: impl Into<String>,
        parents: impl IntoIterator<Item = P>,
    ) -> Self {
        Self {
            id: id.into(),
            parents: parents.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    pub fn with_labels<L: Into<String>>(mut self, labels: impl IntoIterator<Item = L>) -> Self {
        self.labels = labels.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_boundary(mut self, boundary: bool) -> Self {
        self.boundary = boundary;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LaneOptions {
    /// Number of color slots; slots are `0..palette`.
    pub palette: usize,
    /// Visible lane columns; wider rows end in an overflow marker `…`.
    pub max_columns: usize,
    /// Frontier bound; pushing past it is an error.
    pub max_lanes: usize,
}

impl Default for LaneOptions {
    fn default() -> Self {
        Self {
            palette: HISTORY_LANE_PALETTE,
            max_columns: HISTORY_MAX_LANES,
            max_lanes: HISTORY_MAX_LANES,
        }
    }
}

/// One terminal cell of the graph. `slot` is `None` for blanks and the overflow marker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphCell {
    pub glyph: char,
    pub slot: Option<usize>,
}

const BLANK: GraphCell = GraphCell {
    glyph: ' ',
    slot: None,
};

/// One laid-out row. `cells` alternates lane glyphs and the gaps between them,
/// so lane `k` is drawn at cell `2 * k`; the row is at most `2 * max_columns` wide.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphRow {
    pub cells: Vec<GraphCell>,
    /// Lane column of the commit dot.
    pub column: usize,
    /// Color slot of the commit's lane.
    pub slot: usize,
    /// Lane columns the row spans before width capping.
    pub width: usize,
    /// Columns beyond `max_columns` were replaced by the overflow marker.
    pub overflow: bool,
    /// No child above leads to this commit: a lane starts here.
    pub tip: bool,
    /// The commit's lane ends here (a root or shallow boundary).
    pub root: bool,
    /// Git reported a shallow boundary at this commit.
    pub boundary: bool,
    /// Ref label the commit's lane started from, if any.
    pub label: Option<String>,
}

impl GraphRow {
    pub fn text(&self) -> String {
        self.cells.iter().map(|cell| cell.glyph).collect()
    }

    /// Coalesced `(text, slot)` runs for styling.
    pub fn spans(&self) -> Vec<(String, Option<usize>)> {
        let mut spans: Vec<(String, Option<usize>)> = Vec::new();
        for cell in &self.cells {
            match spans.last_mut() {
                Some((text, slot)) if *slot == cell.slot => text.push(cell.glyph),
                _ => spans.push((cell.glyph.to_string(), cell.slot)),
            }
        }
        spans
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Lane {
    target: String,
    slot: usize,
    label: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Link {
    /// A lane waiting for this commit ends here.
    Converge,
    /// A later parent is already awaited by this lane.
    Join,
    /// A later parent starts this lane.
    Start,
}

impl Link {
    fn glyph(self, right: bool, continued: bool) -> char {
        match (self, right, continued) {
            (Link::Converge, _, true) => '┴',
            (Link::Join, _, true) => '┼',
            (Link::Start, _, true) => '┬',
            (Link::Converge, true, false) => '╯',
            (Link::Converge, false, false) => '╰',
            (Link::Join, true, false) => '┤',
            (Link::Join, false, false) => '├',
            (Link::Start, true, false) => '╮',
            (Link::Start, false, false) => '╭',
        }
    }
}

/// Incremental lane layout. An `Err` from `push` leaves the layout unusable.
#[derive(Clone, Debug)]
pub struct LaneLayout {
    options: LaneOptions,
    lanes: Vec<Option<Lane>>,
    cooling: Option<usize>,
    next_slot: usize,
    emitted: HashSet<String>,
}

impl Default for LaneLayout {
    fn default() -> Self {
        Self::new(LaneOptions::default())
    }
}

impl LaneLayout {
    pub fn new(options: LaneOptions) -> Self {
        Self {
            options,
            lanes: Vec::new(),
            cooling: None,
            next_slot: 0,
            emitted: HashSet::new(),
        }
    }

    pub fn options(&self) -> LaneOptions {
        self.options
    }

    /// Lanes still waiting for a commit below the last pushed row.
    pub fn open_lanes(&self) -> usize {
        self.lanes.iter().flatten().count()
    }

    pub fn rows(&self) -> usize {
        self.emitted.len()
    }

    pub fn extend<'a>(
        &mut self,
        commits: impl IntoIterator<Item = &'a LaneCommit>,
    ) -> Result<Vec<GraphRow>, String> {
        commits
            .into_iter()
            .map(|commit| self.push(commit))
            .collect()
    }

    pub fn push(&mut self, commit: &LaneCommit) -> Result<GraphRow, String> {
        if !self.emitted.insert(commit.id.clone()) {
            return Err(format!("History graph repeats commit {}", commit.id));
        }
        let mut parents: Vec<&str> = Vec::with_capacity(commit.parents.len());
        if !commit.boundary {
            for parent in &commit.parents {
                if self.emitted.contains(parent.as_str()) {
                    return Err(format!(
                        "History graph parent {parent} was shown above its child {}",
                        commit.id
                    ));
                }
                if !parents.contains(&parent.as_str()) {
                    parents.push(parent);
                }
            }
        }
        let before: Vec<Option<usize>> = self
            .lanes
            .iter()
            .map(|lane| lane.as_ref().map(|lane| lane.slot))
            .collect();
        let cooling = self.cooling.take();
        let mut free: Vec<usize> = (0..self.lanes.len())
            .filter(|k| self.lanes[*k].is_none() && Some(*k) != cooling)
            .collect();
        let waiting: Vec<usize> = (0..self.lanes.len())
            .filter(|k| matches!(&self.lanes[*k], Some(lane) if lane.target == commit.id))
            .collect();
        let tip = waiting.is_empty();
        let column = match waiting.first() {
            Some(column) => *column,
            None => {
                let column = self.allocate(&mut free, 0, cooling);
                let slot = self.new_slot();
                self.lanes[column] = Some(Lane {
                    target: commit.id.clone(),
                    slot,
                    label: commit.labels.first().cloned(),
                });
                column
            }
        };
        let mut links: Vec<(usize, Link, usize)> = Vec::new();
        for k in waiting.iter().skip(1) {
            let lane = self.lanes[*k].take().expect("waiting lane");
            links.push((*k, Link::Converge, lane.slot));
        }
        let lane = self.lanes[column].as_mut().expect("commit lane");
        if lane.label.is_none() {
            lane.label = commit.labels.first().cloned();
        }
        let (slot, label) = (lane.slot, lane.label.clone());
        let root = parents.is_empty();
        match parents.first() {
            Some(first) => lane.target = (*first).to_owned(),
            None => {
                self.lanes[column] = None;
                self.cooling = Some(column);
            }
        }
        for parent in parents.iter().skip(1) {
            let existing = (0..self.lanes.len()).find(|k| {
                *k != column && matches!(&self.lanes[*k], Some(lane) if lane.target == *parent)
            });
            match existing {
                Some(k) => links.push((k, Link::Join, self.lanes[k].as_ref().unwrap().slot)),
                None => {
                    let k = self.allocate(&mut free, column + 1, cooling);
                    let slot = self.new_slot();
                    self.lanes[k] = Some(Lane {
                        target: (*parent).to_owned(),
                        slot,
                        label: None,
                    });
                    links.push((k, Link::Start, slot));
                }
            }
        }
        while matches!(self.lanes.last(), Some(None)) {
            self.lanes.pop();
        }
        if self.lanes.len() > self.options.max_lanes {
            return Err(format!(
                "History graph needs more than {} lanes; no path is dropped",
                self.options.max_lanes
            ));
        }
        let mut row = self.render(&before, column, slot, &links);
        row.tip = tip;
        row.root = root;
        row.boundary = commit.boundary;
        row.label = label;
        Ok(row)
    }

    /// First reusable column at or after `min`, else a new column on the right.
    fn allocate(&mut self, free: &mut Vec<usize>, min: usize, cooling: Option<usize>) -> usize {
        if let Some(n) = free.iter().position(|k| *k >= min) {
            return free.remove(n);
        }
        let mut column = self.lanes.len().max(min);
        if Some(column) == cooling {
            column += 1;
        }
        if column >= self.lanes.len() {
            self.lanes.resize(column + 1, None);
        }
        column
    }

    /// Next palette slot in rotation, skipping slots held by open lanes while any are free.
    fn new_slot(&mut self) -> usize {
        let palette = self.options.palette.max(1);
        let used: Vec<usize> = self.lanes.iter().flatten().map(|lane| lane.slot).collect();
        let slot = (0..palette)
            .map(|step| (self.next_slot + step) % palette)
            .find(|slot| !used.contains(slot))
            .unwrap_or(self.next_slot % palette);
        self.next_slot = slot + 1;
        slot
    }

    fn render(
        &self,
        before: &[Option<usize>],
        column: usize,
        slot: usize,
        links: &[(usize, Link, usize)],
    ) -> GraphRow {
        let width = links
            .iter()
            .map(|(k, ..)| k + 1)
            .chain([before.len(), self.lanes.len(), column + 1])
            .max()
            .unwrap_or(1);
        let mut glyphs = vec![BLANK; width];
        let mut gaps = vec![BLANK; width];
        for (k, lane) in before.iter().enumerate() {
            if let Some(lane) = lane {
                glyphs[k] = GraphCell {
                    glyph: '│',
                    slot: Some(*lane),
                };
            }
        }
        glyphs[column] = GraphCell {
            glyph: '●',
            slot: Some(slot),
        };
        let link = |k: usize| links.iter().find(|(at, ..)| *at == k);
        let right = links.iter().map(|(k, ..)| *k).filter(|k| *k > column).max();
        let left = links.iter().map(|(k, ..)| *k).filter(|k| *k < column).min();
        if let Some(end) = right {
            let mut current = None;
            for k in (column + 1..=end).rev() {
                if let Some((_, kind, lane)) = link(k) {
                    current = Some(*lane);
                    glyphs[k] = GraphCell {
                        glyph: kind.glyph(true, k < end),
                        slot: current,
                    };
                } else {
                    glyphs[k] = crossing(glyphs[k], current);
                }
                gaps[k - 1] = GraphCell {
                    glyph: '─',
                    slot: current,
                };
            }
        }
        if let Some(start) = left {
            let mut current = None;
            for k in start..column {
                if let Some((_, kind, lane)) = link(k) {
                    current = Some(*lane);
                    glyphs[k] = GraphCell {
                        glyph: kind.glyph(false, k > start),
                        slot: current,
                    };
                } else {
                    glyphs[k] = crossing(glyphs[k], current);
                }
                gaps[k] = GraphCell {
                    glyph: '─',
                    slot: current,
                };
            }
        }
        let max = self.options.max_columns.max(1);
        let overflow = width > max;
        let shown = width.min(max);
        let mut cells = Vec::with_capacity(shown * 2);
        for k in 0..shown {
            cells.push(glyphs[k]);
            if k + 1 < shown {
                cells.push(gaps[k]);
            }
        }
        if overflow {
            let marker = GraphCell {
                glyph: '…',
                slot: None,
            };
            if column >= max - 1 {
                cells[2 * (max - 1)] = glyphs[column];
                cells.push(marker);
            } else {
                cells[2 * (max - 1)] = marker;
            }
        }
        GraphRow {
            cells,
            column,
            slot,
            width,
            overflow,
            tip: false,
            root: false,
            boundary: false,
            label: None,
        }
    }
}

/// A connector cell between the commit and its farthest link: `─` over a gap, and `┿` in the
/// connector's colour where it crosses a lane that keeps going, so the join reads as one
/// unbroken line while the crossed lane stays continuous (`┼` stays a real join).
fn crossing(cell: GraphCell, slot: Option<usize>) -> GraphCell {
    GraphCell {
        glyph: if cell.glyph == '│' { CROSSING } else { '─' },
        slot,
    }
}

/// Lays out a complete list of commits with fresh lanes.
pub fn layout(commits: &[LaneCommit], options: LaneOptions) -> Result<Vec<GraphRow>, String> {
    LaneLayout::new(options).extend(commits)
}

#[cfg(test)]
#[path = "lanes_tests.rs"]
mod tests;
