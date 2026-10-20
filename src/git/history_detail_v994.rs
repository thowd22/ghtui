//! The History commit detail's changed files (TASK-99.4): one bounded, read-only
//! `git diff-tree --raw --numstat -z -r` of a commit against its first parent (or, for a root
//! commit, `--root` against the empty tree). The numstat records are read with the same
//! `readonly_diff_paths_v1` record reader the diff path protocol uses, and every record is
//! crosschecked against the raw status record of the same file. Nothing touches the working
//! tree, the index or refs.
use super::*;

/// Files kept for display; the rest are counted (the pane shows `+N more files`).
pub const HISTORY_DETAIL_FILES_V994: usize = 200;
/// Changed files read at most; a larger commit is refused rather than cut silently.
const MAX_FILES: usize = 50_000;
/// Raw plus numstat output read at most.
const MAX_RAW: usize = 8 * 1024 * 1024;
const MAX_PATH: usize = 4096;

/// One changed file of a commit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryChangedFileV994 {
    /// Git's status letter: `A`, `M`, `D`, `T`, `R` (renamed), `C` (copied), `U` or `X`.
    pub status: char,
    /// The path in the commit (the deleted path for `D`).
    pub path: RawPath,
    /// The source path of a rename or copy.
    pub from: Option<RawPath>,
    /// Added and removed lines; `None` for a binary file.
    pub counts: Option<(usize, usize)>,
}

/// The changed files of `commit` against `parent` (its first parent, or none for a root).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryChangedFilesV994 {
    pub commit: String,
    pub parent: Option<String>,
    /// The first [`HISTORY_DETAIL_FILES_V994`] files in Git's order.
    pub files: Vec<HistoryChangedFileV994>,
    /// Every changed file, shown or not.
    pub total: usize,
    /// Line totals over every text file.
    pub added: usize,
    pub removed: usize,
    /// Binary files among all changed files.
    pub binary: usize,
}

impl GitRepository {
    /// Reads the changed files of `commit` against `parent` (None only for a root commit).
    /// Rename detection is on (`-M`), so a moved file is one `R` row.
    pub fn history_changed_files_v994(
        &self,
        commit: &str,
        parent: Option<&str>,
    ) -> GitResult<HistoryChangedFilesV994> {
        validate_object_id(commit)?;
        if let Some(parent) = parent {
            validate_object_id(parent)?;
        }
        let mut args: Vec<&str> = vec![
            "diff-tree",
            "-r",
            "-z",
            "-M",
            "--no-commit-id",
            "--no-ext-diff",
            "--no-textconv",
            "--raw",
            "--numstat",
        ];
        match parent {
            Some(parent) => args.extend([parent, commit]),
            None => args.extend(["--root", commit]),
        }
        let raw = self.original_bytes_with_capture_limit_v1(args.iter(), None, MAX_RAW)?;
        let (files, total, added, removed, binary) =
            parse_changed_files_v994(&raw, HISTORY_DETAIL_FILES_V994)?;
        Ok(HistoryChangedFilesV994 {
            commit: commit.into(),
            parent: parent.map(str::to_owned),
            files,
            total,
            added,
            removed,
            binary,
        })
    }
}

fn refused(reason: &str) -> GitError {
    GitError(format!("Changed files read refused: {reason}"))
}

fn checked_path(path: &[u8]) -> GitResult<RawPath> {
    if path.is_empty() || path.len() > MAX_PATH {
        return Err(refused("empty or oversized path"));
    }
    Ok(RawPath(path.to_vec()))
}

type ParsedV994 = (Vec<HistoryChangedFileV994>, usize, usize, usize, usize);
/// One raw record: its status letter, a rename or copy source, and its path.
type RawRecordV994<'a> = (char, Option<&'a [u8]>, &'a [u8]);

/// Parses `--raw --numstat -z` output: every raw record (`:modes oids STATUS\0path\0`, or two
/// paths for a rename or copy) first, then one numstat record per file in the same order.
/// Keeps the first `keep` files and totals the rest.
pub(crate) fn parse_changed_files_v994(raw: &[u8], keep: usize) -> GitResult<ParsedV994> {
    if raw.is_empty() {
        return Ok((Vec::new(), 0, 0, 0, 0));
    }
    if raw.last() != Some(&0) {
        return Err(refused("output does not end in NUL"));
    }
    let mut fields = raw[..raw.len() - 1].split(|byte| *byte == 0).peekable();

    let mut statuses: Vec<RawRecordV994> = Vec::new();
    while let Some(record) = fields.next_if(|field| field.first() == Some(&b':')) {
        if statuses.len() >= MAX_FILES {
            return Err(refused("more than 50,000 changed files"));
        }
        let status = record
            .rsplit(|byte| *byte == b' ')
            .next()
            .and_then(|status| status.first())
            .copied()
            .filter(|letter| b"AMDTRCUX".contains(letter))
            .ok_or_else(|| refused("unknown raw status"))?;
        let first = fields
            .next()
            .ok_or_else(|| refused("raw record lacks a path"))?;
        if matches!(status, b'R' | b'C') {
            let second = fields
                .next()
                .ok_or_else(|| refused("rename record lacks its new path"))?;
            statuses.push((char::from(status), Some(first), second));
        } else {
            statuses.push((char::from(status), None, first));
        }
    }
    let mut files = Vec::new();
    let (mut added, mut removed, mut binary) = (0usize, 0usize, 0usize);
    for (index, (status, from, path)) in statuses.iter().enumerate() {
        let record = fields
            .next()
            .ok_or_else(|| refused("numstat record missing"))?;
        let parsed = super::readonly_diff_paths_v1::numstat_record(record, &mut fields)?;
        let (old, new) = (from.unwrap_or(path), *path);
        if parsed.paired != from.is_some() || parsed.old != old || parsed.new != new {
            return Err(refused("raw and numstat records disagree"));
        }
        match parsed.counts {
            Some((plus, minus)) => {
                added = added.saturating_add(plus);
                removed = removed.saturating_add(minus);
            }
            None => binary += 1,
        }
        if index < keep {
            files.push(HistoryChangedFileV994 {
                status: *status,
                path: checked_path(path)?,
                from: from.map(checked_path).transpose()?,
                counts: parsed.counts,
            });
        }
    }
    if fields.next().is_some() {
        return Err(refused("extra numstat record"));
    }
    Ok((files, statuses.len(), added, removed, binary))
}

#[cfg(test)]
#[path = "history_detail_v994_tests.rs"]
mod history_detail_v994_tests;
