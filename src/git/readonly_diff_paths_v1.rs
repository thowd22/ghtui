//! Bounded Git numstat -z path protocol, not another canonical patch parser.
use super::{Diff, DiffPaths, GitError, GitResult, MAX_OUTPUT};
use std::collections::BTreeSet;
const MAX_FILES: usize = 50_000;
const MAX_PATH: usize = 4096;

pub(super) fn paths(diff: &Diff, bytes: &[u8], cap: usize) -> GitResult<DiffPaths> {
    if cap == 0
        || cap > MAX_OUTPUT
        || bytes.len() > cap
        || diff.files.len() > MAX_FILES
        || (!bytes.is_empty() && bytes.last() != Some(&0))
    {
        return Err(error("capture, member or final-NUL bound"));
    }
    if bytes.is_empty() {
        return if diff.files.is_empty() {
            Ok(Vec::new())
        } else {
            Err(error("canonical file count disagreement"))
        };
    }
    let mut records = bytes[..bytes.len() - 1].split(|b| *b == 0);
    let mut prepared = Vec::new();
    let mut unique = BTreeSet::new();
    let mut path_bytes = 0usize;
    while let Some(record) = records.next() {
        let file = diff
            .files
            .get(prepared.len())
            .ok_or_else(|| error("extra path record"))?;
        let parsed = numstat_record(record, &mut records)?;
        if parsed.counts.is_none() != file.binary {
            return Err(error("binary metadata disagreement"));
        }
        if let Some((added, removed)) = parsed.counts {
            let actual_added = file
                .hunks
                .iter()
                .flat_map(|h| &h.lines)
                .filter(|l| l.kind == crate::diff::LineKind::Added)
                .count();
            let actual_removed = file
                .hunks
                .iter()
                .flat_map(|h| &h.lines)
                .filter(|l| l.kind == crate::diff::LineKind::Removed)
                .count();
            if added != actual_added || removed != actual_removed {
                return Err(error("canonical changed-line count disagreement"));
            }
        }
        let (paired, old, new) = (parsed.paired, parsed.old, parsed.new);
        for path in [old, new] {
            if path.is_empty() || path.len() > MAX_PATH {
                return Err(error("empty or oversized path"));
            }
            path_bytes = path_bytes
                .checked_add(path.len())
                .ok_or_else(|| error("path aggregate overflow"))?;
            if path_bytes > cap {
                return Err(error("path aggregate bound"));
            }
        }
        if !unique.insert((old, new)) {
            return Err(error("duplicate path pair"));
        }
        let created = mode(file, b"new file mode ")?;
        let deleted = mode(file, b"deleted file mode ")?;
        if created && deleted {
            return Err(error("conflicting side metadata"));
        }
        let old_mode = mode(file, b"old mode ")?;
        let new_mode = mode(file, b"new mode ")?;
        if old_mode != new_mode || (created || deleted) && (old_mode || new_mode) {
            return Err(error("incomplete or conflicting mode metadata"));
        }
        let rename = metadata_pair(file, b"rename from ", b"rename to ")?;
        let copy = metadata_pair(file, b"copy from ", b"copy to ")?;
        if rename && copy
            || paired != (rename || copy)
            || paired && (created || deleted || old == new)
        {
            return Err(error("rename/copy sentinel disagreement"));
        }
        if paired && (file.old_path.is_none() || file.new_path.is_none()) {
            return Err(error("canonical rename/copy paths unavailable"));
        }
        if !paired
            && file.old_path.is_none()
            && file.new_path.is_none()
            && !created
            && !deleted
            && !file.binary
            && !(old_mode && new_mode)
        {
            return Err(error("canonical side metadata unavailable"));
        }
        if created && file.old_path.is_some() || deleted && file.new_path.is_some() {
            return Err(error("canonical side presence disagreement"));
        }
        for (parsed, raw) in [
            (file.old_path.as_deref(), old),
            (file.new_path.as_deref(), new),
        ] {
            if parsed.is_some_and(|p| p != raw) {
                return Err(error("canonical path/order disagreement"));
            }
        }
        prepared.push(((!created).then_some(old), (!deleted).then_some(new)));
    }
    if prepared.len() != diff.files.len() {
        return Err(error("canonical file count disagreement"));
    }
    // All variable material is bounded and crosschecked before copying paths.
    Ok(prepared
        .into_iter()
        .map(|(a, b)| (a.map(<[u8]>::to_vec), b.map(<[u8]>::to_vec)))
        .collect())
}
/// One `--numstat -z` record: the line counts (None for a binary file) and its paths. A
/// rename or copy (`added\tremoved\t` with an empty path) takes its old and new paths from the
/// next two NUL-separated fields.
pub(super) struct NumstatRecordV1<'a> {
    pub counts: Option<(usize, usize)>,
    pub old: &'a [u8],
    pub new: &'a [u8],
    pub paired: bool,
}

/// Reads one numstat record from `record` and, for a rename or copy, the two path fields that
/// follow it in `records`.
pub(super) fn numstat_record<'a>(
    record: &'a [u8],
    records: &mut impl Iterator<Item = &'a [u8]>,
) -> GitResult<NumstatRecordV1<'a>> {
    let mut fields = record.splitn(3, |b| *b == b'\t');
    let added = fields.next().ok_or_else(|| error("missing additions"))?;
    let removed = fields.next().ok_or_else(|| error("missing deletions"))?;
    let path = fields.next().ok_or_else(|| error("missing path field"))?;
    let counts = if added == b"-" && removed == b"-" {
        None
    } else {
        Some((count(added)?, count(removed)?))
    };
    let paired = path.is_empty();
    let (old, new) = if paired {
        (
            records.next().ok_or_else(|| error("missing old path"))?,
            records.next().ok_or_else(|| error("missing new path"))?,
        )
    } else {
        (path, path)
    };
    Ok(NumstatRecordV1 {
        counts,
        old,
        new,
        paired,
    })
}
fn count(bytes: &[u8]) -> GitResult<usize> {
    if bytes.is_empty() || bytes.len() > 20 || !bytes.iter().all(u8::is_ascii_digit) {
        return Err(error("invalid decimal count"));
    }
    bytes.iter().try_fold(0usize, |n, b| {
        n.checked_mul(10)
            .and_then(|n| n.checked_add((b - b'0') as usize))
            .ok_or_else(|| error("decimal count overflow"))
    })
}
fn mode(file: &crate::diff::FileDiff, prefix: &[u8]) -> GitResult<bool> {
    let mut found = false;
    for metadata in &file.metadata {
        if metadata.starts_with(&prefix[..prefix.len() - 1]) && !metadata.starts_with(prefix) {
            return Err(error("malformed side mode separator"));
        }
        if let Some(mode) = metadata.strip_prefix(prefix) {
            if found || mode.len() != 6 || !mode.iter().all(|b| matches!(b, b'0'..=b'7')) {
                return Err(error("duplicate or malformed side mode"));
            }
            found = true;
        }
    }
    Ok(found)
}
fn metadata_pair(file: &crate::diff::FileDiff, from: &[u8], to: &[u8]) -> GitResult<bool> {
    let a = file.metadata.iter().filter(|m| m.starts_with(from)).count();
    let b = file.metadata.iter().filter(|m| m.starts_with(to)).count();
    if a > 1 || b > 1 || a != b {
        return Err(error("incomplete or duplicate rename/copy metadata"));
    }
    Ok(a == 1)
}
fn error(reason: &str) -> GitError {
    let guidance = if matches!(
        reason,
        "extra path record" | "canonical file count disagreement"
    ) {
        "; conflict or combined diffs need the conflict viewer"
    } else {
        ""
    };
    GitError(format!(
        "Read-only diff path protocol refused: {reason}{guidance}"
    ))
}

#[cfg(test)]
#[path = "readonly_diff_paths_v1_tests.rs"]
mod tests;
