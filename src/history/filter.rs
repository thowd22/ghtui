//! History search, path filter and branch scope (TASK-99.3).
//!
//! - Plain text matches locally over the loaded commits: the whole message (what Git's
//!   `--grep` selects with `--regexp-ignore-case --fixed-strings`), the author name or email,
//!   or a SHA prefix of four or more hex digits.
//! - `author:` and `path:` chips are authoritative: Git selects their commits over the same
//!   walk (`GitRepository::original_graph_filter_ids_v993`, `--full-history`).
//! - Highlight mode keeps every commit and dims the ones that do not match; hide mode drops
//!   them and joins each lane through the hidden commits to the nearest shown ancestor.
//! - The branch scope chip re-reads the walk from one branch. Clearing every chip restores
//!   the all-branches walk and the commit that was selected before filtering.
use crate::{
    git::{GitActor, OriginalGraphCommitV1, RawPath},
    repository_graph_v1::{GraphPositionV2, OriginalGraphStateV1},
};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// Bytes kept of a typed query, author or path.
pub const MAX_QUERY: usize = 1024;
/// Graph parents one shown commit may gain through hidden commits in hide mode.
const MAX_THROUGH: usize = 64;

/// Which branches the walk starts from.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub enum HistoryScopeV993 {
    /// Every local branch, remote branch, tag and HEAD.
    #[default]
    All,
    /// HEAD alone (the checked-out branch, or a detached HEAD).
    Current,
    /// One branch, remote branch or tag, by its chip name.
    Branch(String),
}

/// A removable chip of the filter row, in the order chips were added.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub enum HistoryChipV993 {
    Path,
    Author,
    Scope,
}

/// What the filter row asks for. The worker applies it; text-only changes need no Git.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize)]
pub struct HistoryFilterSpecV993 {
    pub text: String,
    pub author: Option<String>,
    pub path: Option<RawPath>,
    pub scope: HistoryScopeV993,
    /// Hide non-matching commits instead of dimming them.
    pub hide: bool,
    /// Chips in the order they were added; Backspace on an empty query removes the last.
    pub order: Vec<HistoryChipV993>,
}

impl HistoryFilterSpecV993 {
    pub fn text(&self) -> Option<&str> {
        Some(self.text.trim()).filter(|text| !text.is_empty())
    }
    /// A text, author or path filter: rows are flagged as matches.
    pub fn matching(&self) -> bool {
        self.text().is_some() || self.author.is_some() || self.path.is_some()
    }
    /// Nothing filters and the walk covers every branch.
    pub fn is_empty(&self) -> bool {
        !self.matching() && self.scope == HistoryScopeV993::All
    }
    /// Git must select the chips' commits again.
    pub fn chips_differ(&self, other: &Self) -> bool {
        self.author != other.author || self.path != other.path
    }

    /// Applies a typed query to `self`: `author:NAME`, `path:PATH` and `branch:NAME`
    /// (`branch:all`, `branch:current`) tokens become chips, replacing a chip of the same
    /// kind; every other word is the text, which replaces the previous text.
    pub fn with_query(&self, input: &str) -> Self {
        let mut next = self.clone();
        let mut words = Vec::new();
        for word in input.split_whitespace() {
            let lower = word.to_ascii_lowercase();
            let value = |prefix: &str| word[prefix.len()..].to_owned();
            if lower.starts_with("author:") && word.len() > 7 {
                next.set_author(Some(value("author:")));
            } else if lower.starts_with("path:") && word.len() > 5 {
                next.set_path(Some(RawPath(value("path:").into_bytes())));
            } else if lower.starts_with("branch:") && word.len() > 7 {
                next.set_scope(match value("branch:").as_str() {
                    "all" | "*" => HistoryScopeV993::All,
                    "current" | "HEAD" | "@" => HistoryScopeV993::Current,
                    name => HistoryScopeV993::Branch(name.to_owned()),
                });
            } else {
                words.push(word);
            }
        }
        next.text = cut(&words.join(" "));
        next
    }
    pub fn set_author(&mut self, author: Option<String>) {
        self.author = author.map(|value| cut(&value));
        self.track(HistoryChipV993::Author, self.author.is_some());
    }
    pub fn set_path(&mut self, path: Option<RawPath>) {
        self.path = path.map(|mut path| {
            path.0.truncate(MAX_QUERY * 4);
            path
        });
        self.track(HistoryChipV993::Path, self.path.is_some());
    }
    pub fn set_scope(&mut self, scope: HistoryScopeV993) {
        let scoped = scope != HistoryScopeV993::All;
        self.scope = scope;
        self.track(HistoryChipV993::Scope, scoped);
    }
    pub fn remove(&mut self, chip: HistoryChipV993) {
        match chip {
            HistoryChipV993::Path => self.set_path(None),
            HistoryChipV993::Author => self.set_author(None),
            HistoryChipV993::Scope => self.set_scope(HistoryScopeV993::All),
        }
    }
    /// The chip Backspace on an empty query removes.
    pub fn last_chip(&self) -> Option<HistoryChipV993> {
        self.order.last().copied()
    }
    fn track(&mut self, chip: HistoryChipV993, present: bool) {
        self.order.retain(|old| *old != chip);
        if present {
            self.order.push(chip);
        }
    }
}

fn cut(value: &str) -> String {
    let mut end = value.len().min(MAX_QUERY);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].trim().to_owned()
}

/// Case-insensitive fixed-string containment; `needle` is already lowercase.
fn contains_folded(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    if needle.is_ascii() {
        let needle = needle.as_bytes();
        return haystack
            .as_bytes()
            .windows(needle.len())
            .any(|window| window.eq_ignore_ascii_case(needle));
    }
    haystack.to_lowercase().contains(needle)
}

/// True when the lowercase `text` occurs in the message, the author name or email, or is a
/// SHA prefix (four or more hex digits) of `id`.
pub fn text_matches(text: &str, message: &str, author: &GitActor, id: &str) -> bool {
    contains_folded(message, text)
        || contains_folded(&author.name, text)
        || contains_folded(&author.email, text)
        || (text.len() >= 4
            && text.bytes().all(|byte| byte.is_ascii_hexdigit())
            && id.starts_with(text))
}

/// Where the cached flags came from: one graph state, its chip ids and the text.
#[derive(Clone, Debug, PartialEq, Eq)]
struct CacheKey {
    state: usize,
    loaded: usize,
    ids: usize,
    text: String,
}

/// Unfiltered History kept while filtering: the walk, and the selection to return to.
#[derive(Clone)]
pub struct SavedHistoryV993 {
    pub state: Arc<OriginalGraphStateV1>,
    pub position: GraphPositionV2,
}

/// The applied filter of one History view, with its Git-selected chip commits and the match
/// flags of the loaded walk.
#[derive(Clone, Default)]
pub struct HistoryFilterV993 {
    pub spec: HistoryFilterSpecV993,
    chip_ids: Option<Arc<HashSet<String>>>,
    chip_capped: bool,
    chip_walk: Option<[u8; 32]>,
    pub saved: Option<SavedHistoryV993>,
    key: Option<CacheKey>,
    flags: Arc<[bool]>,
    count: usize,
}

impl std::fmt::Debug for HistoryFilterV993 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HistoryFilterV993")
            .field("spec", &self.spec)
            .field("chip_ids", &self.chip_ids.as_ref().map(|ids| ids.len()))
            .field("saved", &self.saved.as_ref().map(|saved| &saved.position))
            .field("count", &self.count)
            .finish()
    }
}

impl HistoryFilterV993 {
    /// Git's selection for the author/path chips over the walk with this fingerprint.
    pub fn set_chip_ids(&mut self, ids: Option<HashSet<String>>, capped: bool, walk: [u8; 32]) {
        self.chip_ids = ids.map(Arc::new);
        self.chip_capped = capped && self.chip_ids.is_some();
        self.chip_walk = Some(walk);
        self.key = None;
    }
    /// The chip ids still describe `state`'s walk.
    pub fn chips_current(&self, state: &OriginalGraphStateV1) -> bool {
        !(self.spec.author.is_some() || self.spec.path.is_some())
            || (self.chip_ids.is_some()
                && self.chip_walk == Some(state.last_page().walk_fingerprint()))
    }
    /// Git stopped the chip read at the 10,000-commit bound.
    pub fn chip_capped(&self) -> bool {
        self.chip_capped
    }
    fn key(&self, state: &Arc<OriginalGraphStateV1>) -> CacheKey {
        CacheKey {
            state: Arc::as_ptr(state) as usize,
            loaded: state.loaded(),
            ids: self
                .chip_ids
                .as_ref()
                .map_or(0, |ids| Arc::as_ptr(ids) as usize),
            text: self.spec.text().unwrap_or_default().to_lowercase(),
        }
    }
    /// Recomputes the match flags of every loaded commit when the walk, the chips or the text
    /// changed. Pure: no Git.
    pub fn sync(&mut self, state: &Arc<OriginalGraphStateV1>) {
        let key = self.key(state);
        if self.key.as_ref() == Some(&key) {
            return;
        }
        let mut flags = Vec::with_capacity(state.loaded());
        if self.spec.matching() && self.chips_current(state) {
            for index in 0..state.loaded() {
                flags.push(
                    state
                        .observed_at_v993(index)
                        .is_some_and(|observed| self.matches(&key.text, observed)),
                );
            }
        }
        self.count = flags.iter().filter(|flag| **flag).count();
        self.flags = Arc::from(flags);
        self.key = Some(key);
    }
    fn matches(&self, text: &str, observed: &OriginalGraphCommitV1) -> bool {
        let commit = observed.commit();
        if self
            .chip_ids
            .as_ref()
            .is_some_and(|ids| !ids.contains(&commit.id))
        {
            return false;
        }
        text.is_empty() || text_matches(text, observed.message(), &commit.author, &commit.id)
    }
    /// Per loaded commit (topology order): does it match? None while nothing filters or the
    /// flags belong to another walk.
    pub fn flags(&self, state: &Arc<OriginalGraphStateV1>) -> Option<&[bool]> {
        (self.spec.matching()
            && self.key.as_ref() == Some(&self.key(state))
            && self.flags.len() == state.loaded())
        .then_some(&self.flags[..])
    }
    /// Whether the loaded commit `id` matches; None when nothing filters.
    pub fn matched(&self, state: &Arc<OriginalGraphStateV1>, id: &str) -> Option<bool> {
        let flags = self.flags(state)?;
        let node = state.loaded_member_v2(id)?.node as usize;
        flags.get(node).copied()
    }
    /// Matching loaded commits.
    pub fn count(&self) -> usize {
        self.count
    }
    /// The next (or previous) matching loaded commit after `from`, wrapping around.
    pub fn step(
        &self,
        state: &Arc<OriginalGraphStateV1>,
        from: usize,
        forward: bool,
    ) -> Option<usize> {
        let flags = self.flags(state)?;
        let length = flags.len();
        (1..=length)
            .map(|offset| {
                if forward {
                    (from + offset) % length
                } else {
                    (from + length * 2 - offset) % length
                }
            })
            .find(|index| flags[*index])
    }
}

/// Graph parents with hidden commits dropped. `rows` are `(id, parents)` in display order
/// (children first); for every row the result lists its parents with each loaded hidden
/// parent replaced, in order, by that parent's own result, without duplicates. Parents that are
/// not loaded stay as they are, so their lanes continue below like the unfiltered graph.
pub fn hidden_parents<'a>(
    rows: &[(&'a str, Vec<&'a str>)],
    visible: &[bool],
) -> Result<Vec<Vec<&'a str>>, String> {
    let index: HashMap<&str, usize> = rows
        .iter()
        .enumerate()
        .map(|(at, (id, _))| (*id, at))
        .collect();
    let mut result: Vec<Vec<&'a str>> = vec![Vec::new(); rows.len()];
    for at in (0..rows.len()).rev() {
        let mut parents: Vec<&'a str> = Vec::new();
        for parent in &rows[at].1 {
            let through = match index.get(parent) {
                Some(&below) if below > at && !visible[below] => result[below].clone(),
                _ => vec![*parent],
            };
            for id in through {
                if !parents.contains(&id) {
                    parents.push(id);
                }
            }
        }
        if parents.len() > MAX_THROUGH {
            return Err(format!(
                "Hiding non-matches would join more than {MAX_THROUGH} paths into one commit • h shows them dimmed instead"
            ));
        }
        result[at] = parents;
    }
    Ok(result)
}

#[cfg(test)]
#[path = "filter_tests.rs"]
mod tests;
