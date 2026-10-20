//! History ref chips (TASK-99.2): the typed refs of one commit as the chips its row shows.
//!
//! Order: `HEAD` (attached or detached), the current branch, other local branches, remote
//! branches, tags. A local branch and its remote counterpart on the same commit become one
//! chip marked [`SYNCED`]: its configured upstream, or `origin/<same name>` when it has none.
//! Rows show every chip that fits and an explicit `+N` for the rest; nothing is cut silently.
use crate::{
    editor::display_width,
    git::{HistoryRefKindV1 as Kind, HistoryRefV1},
};

/// Marks a local branch whose remote counterpart points at the same commit.
pub const SYNCED: &str = "⇅";
/// The longest chip label in a row, in cells; longer names end in `…` (Details keeps them).
pub const MAX_CHIP_CELLS: usize = 28;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefChip {
    pub kind: Kind,
    /// `HEAD`, `main`, `origin/main`, `v1.0.0`, or `main ⇅` for a synced pair.
    pub label: String,
    /// The local branch HEAD is attached to (its `HEAD` chip comes first).
    pub current: bool,
    /// A local branch merged with its remote counterpart.
    pub synced: bool,
}

impl RefChip {
    fn of(chip: &HistoryRefV1) -> Self {
        Self {
            kind: chip.kind,
            label: chip.name.clone(),
            current: chip.current,
            synced: false,
        }
    }
    /// Cells the chip paints: one blank of padding on each side.
    pub fn width(&self) -> usize {
        display_width(&crate::ui_chrome::fit(&self.label, MAX_CHIP_CELLS)) + 2
    }
}

/// The remote refname a local branch pairs with: its upstream, else `origin/<name>`.
fn counterpart(local: &HistoryRefV1) -> Option<Vec<u8>> {
    if let Some(upstream) = &local.upstream {
        return upstream
            .starts_with(b"refs/remotes/")
            .then(|| upstream.clone());
    }
    let short = local.raw_name.strip_prefix(b"refs/heads/")?;
    Some([&b"refs/remotes/origin/"[..], short].concat())
}

/// The chips of one commit in display order.
pub fn chips(refs: &[HistoryRefV1]) -> Vec<RefChip> {
    let mut paired = vec![false; refs.len()];
    let mut synced = vec![false; refs.len()];
    for (index, local) in refs.iter().enumerate() {
        if local.kind != Kind::Local {
            continue;
        }
        let Some(remote) = counterpart(local) else {
            continue;
        };
        if let Some(found) = refs.iter().enumerate().position(|(other, chip)| {
            !paired[other]
                && chip.kind == Kind::Remote
                && chip.target == local.target
                && chip.raw_name == remote
        }) {
            paired[found] = true;
            synced[index] = true;
        }
    }
    let mut out = Vec::with_capacity(refs.len() + 1);
    // An attached HEAD reads as its own chip right before its branch.
    if refs
        .iter()
        .any(|chip| chip.kind == Kind::Local && chip.current)
    {
        out.push(RefChip {
            kind: Kind::Head,
            label: "HEAD".into(),
            current: false,
            synced: false,
        });
    }
    let mut ordered = refs.iter().enumerate().collect::<Vec<_>>();
    ordered.sort_by(|(_, a), (_, b)| {
        (a.kind, !a.current, &a.raw_name).cmp(&(b.kind, !b.current, &b.raw_name))
    });
    for (index, chip) in ordered {
        if paired[index] {
            continue;
        }
        let mut shown = RefChip::of(chip);
        if synced[index] {
            shown.synced = true;
            shown.label = format!("{} {SYNCED}", chip.name);
        }
        out.push(shown);
    }
    out
}

/// The chips that fit in `budget` cells (one blank between chips) and how many are left out.
/// When some are left out, room for the `+N` chip is kept; the first chip is always shown so a
/// named commit never reads as unnamed.
pub fn fit(chips: &[RefChip], budget: usize) -> (usize, usize) {
    let total = chips.len();
    let width = |count: usize| {
        chips[..count].iter().map(RefChip::width).sum::<usize>() + count.saturating_sub(1)
    };
    if total == 0 || width(total) <= budget {
        return (total, 0);
    }
    let mut shown = total - 1;
    while shown > 1 && width(shown) + 1 + overflow_width(total - shown) > budget {
        shown -= 1;
    }
    (shown.max(1), total - shown.max(1))
}

/// The `+N` overflow chip's label.
pub fn overflow_label(hidden: usize) -> String {
    format!("+{hidden}")
}

fn overflow_width(hidden: usize) -> usize {
    overflow_label(hidden).len() + 2
}

/// Compact text form for labels, plans and Details: `HEAD, main ⇅, v1.0.0`, every name up
/// to `max_bytes`, then an explicit `+N`.
pub fn summary(chips: &[RefChip], max_bytes: usize) -> String {
    let mut text = String::new();
    for (index, chip) in chips.iter().enumerate() {
        let label = match chip.kind {
            Kind::Tag => format!("tag {}", chip.label),
            _ => chip.label.clone(),
        };
        if !text.is_empty() && text.len() + label.len() + 2 > max_bytes {
            text.push_str(&format!(", +{}", chips.len() - index));
            return text;
        }
        if !text.is_empty() {
            text.push_str(", ");
        }
        text.push_str(&label);
    }
    text
}

#[cfg(test)]
#[path = "refs_tests.rs"]
mod history_refs_v992_tests;
