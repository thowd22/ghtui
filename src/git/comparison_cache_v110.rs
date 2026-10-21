//! Prepared saved-version comparisons, cached by their exact inputs (TASK-110).
//!
//! Reopening the same large commit diff used to run `git diff` twice and parse the whole patch
//! again. Only comparisons between saved objects are cached: their content is fixed by the
//! resolved object ids. Everything else that can change Git's output is part of the key or
//! checked on every hit:
//!
//! - the repository (common directory and worktree root), the exact `git diff` argv (resolved
//!   ids, rename detection and the other options), the capture limit and the description;
//! - the effective configuration (`git config --list -z`), which carries `diff.*` settings
//!   such as the algorithm, context and rename limits, diff drivers and the attributes file;
//! - `GIT_DIFF_OPTS` from the environment;
//! - the attributes of every changed path (`git check-attr -a`), compared again on each hit,
//!   so a new `.gitattributes` rule (binary, a diff driver) recomputes.
//!
//! Working-tree, index and staged comparisons are never cached. The cache is bounded by an
//! estimated memory budget and an entry count and evicts the least recently used entry first;
//! a comparison larger than the whole budget is not kept.

use super::ComparisonResult;
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

/// Estimated bytes all cached comparisons may hold together.
pub(crate) const COMPARISON_CACHE_BUDGET_V110: usize = 64 * 1024 * 1024;
/// At most this many comparisons are kept, however small.
pub(crate) const COMPARISON_CACHE_ENTRIES_V110: usize = 16;

/// Everything that decides a saved-version comparison's output, apart from the changed
/// paths' attributes (checked on each hit, see [`CachedComparisonV110::attributes`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ComparisonKeyV110 {
    pub common_dir: PathBuf,
    pub root: PathBuf,
    pub args: Vec<String>,
    pub capture_limit: usize,
    pub description: String,
    pub base: Option<String>,
    pub head: Option<String>,
    pub common_ancestor: Option<String>,
    pub config: Vec<u8>,
    pub diff_opts: Option<OsString>,
}

/// A prepared comparison and the attribute query that must still answer the same.
#[derive(Debug, Clone)]
pub(crate) struct CachedComparisonV110 {
    pub result: Arc<ComparisonResult>,
    /// NUL-separated changed paths given to `git check-attr -a -z --stdin`.
    pub attribute_paths: Vec<u8>,
    /// Its output when the comparison was prepared.
    pub attributes: Vec<u8>,
}

#[derive(Debug)]
struct Entry {
    cached: CachedComparisonV110,
    cost: usize,
    last_used: u64,
}

/// Bounded least-recently-used store of prepared comparisons.
#[derive(Debug)]
pub(crate) struct ComparisonCacheV110 {
    budget: usize,
    max_entries: usize,
    used: usize,
    clock: u64,
    entries: HashMap<ComparisonKeyV110, Entry>,
    /// Hits per repository (common directory), kept when entries are evicted.
    hits: HashMap<PathBuf, u64>,
}

impl ComparisonCacheV110 {
    pub(crate) fn new(budget: usize, max_entries: usize) -> Self {
        Self {
            budget,
            max_entries,
            used: 0,
            clock: 0,
            entries: HashMap::new(),
            hits: HashMap::new(),
        }
    }

    /// The cached comparison for `key`, without counting a hit: the caller still checks the
    /// attributes and then calls [`Self::hit`] or [`Self::remove`].
    pub(crate) fn candidate(&self, key: &ComparisonKeyV110) -> Option<CachedComparisonV110> {
        self.entries.get(key).map(|entry| entry.cached.clone())
    }

    /// Marks `key` as just used. Returns false when it is no longer cached.
    pub(crate) fn hit(&mut self, key: &ComparisonKeyV110) -> bool {
        self.clock += 1;
        let clock = self.clock;
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        entry.last_used = clock;
        *self.hits.entry(key.common_dir.clone()).or_default() += 1;
        true
    }

    pub(crate) fn remove(&mut self, key: &ComparisonKeyV110) {
        if let Some(entry) = self.entries.remove(key) {
            self.used -= entry.cost;
        }
    }

    /// Keeps `cached` under `key`, evicting the least recently used entries until it fits.
    /// Returns false (and keeps nothing) when it is larger than the whole budget.
    pub(crate) fn insert(&mut self, key: ComparisonKeyV110, cached: CachedComparisonV110) -> bool {
        self.remove(&key);
        let cost = estimated_cost_v110(&key, &cached);
        if cost > self.budget || self.max_entries == 0 {
            return false;
        }
        while self.used + cost > self.budget || self.entries.len() >= self.max_entries {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.remove(&oldest);
        }
        self.clock += 1;
        self.used += cost;
        self.entries.insert(
            key,
            Entry {
                cached,
                cost,
                last_used: self.clock,
            },
        );
        true
    }

    #[cfg(test)]
    pub(crate) fn contains(&self, key: &ComparisonKeyV110) -> bool {
        self.entries.contains_key(key)
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    #[cfg(test)]
    pub(crate) fn used(&self) -> usize {
        self.used
    }

    /// Hits counted for one repository so far.
    #[cfg(test)]
    pub(crate) fn hits_for(&self, common_dir: &std::path::Path) -> u64 {
        self.hits.get(common_dir).copied().unwrap_or(0)
    }
}

/// An upper estimate of the memory a cached comparison holds: the raw patch, each parsed
/// line's bytes and bookkeeping, the per-file metadata, and the key.
pub(crate) fn estimated_cost_v110(key: &ComparisonKeyV110, cached: &CachedComparisonV110) -> usize {
    let result = &cached.result;
    let mut cost = result.patch.len()
        + cached.attribute_paths.len()
        + cached.attributes.len()
        + key.config.len()
        + key.args.iter().map(String::len).sum::<usize>()
        + 1024;
    for file in &result.diff.files {
        cost += std::mem::size_of::<crate::diff::FileDiff>()
            + file.old_path.as_ref().map_or(0, Vec::len)
            + file.new_path.as_ref().map_or(0, Vec::len)
            + file
                .metadata
                .iter()
                .map(|line| line.len() + 24)
                .sum::<usize>();
        for hunk in &file.hunks {
            cost += std::mem::size_of::<crate::diff::Hunk>() + hunk.heading.len();
            cost += hunk
                .lines
                .iter()
                .map(|line| std::mem::size_of::<crate::diff::DiffLine>() + line.text.len())
                .sum::<usize>();
        }
    }
    cost
}

/// The process-wide cache shared by every repository handle.
pub(crate) fn shared_comparison_cache_v110() -> &'static Mutex<ComparisonCacheV110> {
    static CACHE: OnceLock<Mutex<ComparisonCacheV110>> = OnceLock::new();
    CACHE.get_or_init(|| {
        Mutex::new(ComparisonCacheV110::new(
            COMPARISON_CACHE_BUDGET_V110,
            COMPARISON_CACHE_ENTRIES_V110,
        ))
    })
}

/// NUL-separated, de-duplicated old and new paths of every changed file.
pub(crate) fn attribute_paths_v110(result: &ComparisonResult) -> Vec<u8> {
    let mut seen = std::collections::BTreeSet::new();
    for file in &result.diff.files {
        for path in [&file.old_path, &file.new_path].into_iter().flatten() {
            seen.insert(path.as_slice());
        }
    }
    let mut out = Vec::new();
    for path in seen {
        out.extend_from_slice(path);
        out.push(0);
    }
    out
}

#[cfg(test)]
#[path = "comparison_cache_v110_tests.rs"]
mod tests;
