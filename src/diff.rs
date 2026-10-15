//! Canonical unified diff coordinates, independent of terminal presentation.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineKind {
    Context,
    Added,
    Removed,
    NoNewline,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: LineKind,
    pub old_line: Option<u32>,
    pub new_line: Option<u32>,
    /// Original bytes after the unified-diff prefix. Display sanitization belongs to the UI.
    pub text: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hunk {
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
    pub heading: Vec<u8>,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileDiff {
    pub old_path: Option<Vec<u8>>,
    pub new_path: Option<Vec<u8>>,
    pub hunks: Vec<Hunk>,
    pub binary: bool,
    pub metadata: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diff {
    pub base: Option<String>,
    pub head: Option<String>,
    pub files: Vec<FileDiff>,
}

impl Diff {
    /// Parses Git's canonical patch. Quoted byte paths retain their exact decoded bytes.
    pub fn parse(patch: &[u8]) -> Result<Self, String> {
        let mut diff = Self::default();
        // The next position may legitimately be u32::MAX+1 after the final
        // representable line. Never wrap it into another line's coordinates.
        let mut old_line = 0u64;
        let mut new_line = 0u64;
        for line in patch.split(|b| *b == b'\n') {
            if line.starts_with(b"diff --git ") {
                diff.files.push(FileDiff::default());
                continue;
            }
            let Some(file) = diff.files.last_mut() else {
                continue;
            };
            if line.starts_with(b"@@ ") {
                let hunk = parse_header(line)?;
                old_line = u64::from(hunk.old_start);
                new_line = u64::from(hunk.new_start);
                file.hunks.push(hunk);
            } else if let Some(hunk) = file.hunks.last_mut() {
                let (kind, old, new) = match line.first() {
                    Some(b' ') => {
                        let coords = (
                            LineKind::Context,
                            Some(
                                u32::try_from(old_line)
                                    .map_err(|_| "Old diff coordinate overflow")?,
                            ),
                            Some(
                                u32::try_from(new_line)
                                    .map_err(|_| "New diff coordinate overflow")?,
                            ),
                        );
                        old_line += 1;
                        new_line += 1;
                        coords
                    }
                    Some(b'+') => {
                        let coords = (
                            LineKind::Added,
                            None,
                            Some(
                                u32::try_from(new_line)
                                    .map_err(|_| "New diff coordinate overflow")?,
                            ),
                        );
                        new_line += 1;
                        coords
                    }
                    Some(b'-') => {
                        let coords = (
                            LineKind::Removed,
                            Some(
                                u32::try_from(old_line)
                                    .map_err(|_| "Old diff coordinate overflow")?,
                            ),
                            None,
                        );
                        old_line += 1;
                        coords
                    }
                    Some(b'\\') => (LineKind::NoNewline, None, None),
                    None => continue,
                    _ => return Err("Unexpected line in unified diff hunk".into()),
                };
                hunk.lines.push(DiffLine {
                    kind,
                    old_line: old,
                    new_line: new,
                    text: line.get(1..).unwrap_or_default().to_vec(),
                });
            } else if let Some(path) = line.strip_prefix(b"--- ") {
                file.old_path = decode_diff_path(path, b"a/")?;
            } else if let Some(path) = line.strip_prefix(b"+++ ") {
                file.new_path = decode_diff_path(path, b"b/")?;
            } else if line.starts_with(b"Binary files ") || line == b"GIT binary patch" {
                file.binary = true;
                file.metadata.push(line.to_vec());
            } else if let Some(path) = line
                .strip_prefix(b"rename from ")
                .or_else(|| line.strip_prefix(b"copy from "))
            {
                file.old_path = Some(decode_quoted_path(path)?);
                file.metadata.push(line.to_vec());
            } else if let Some(path) = line
                .strip_prefix(b"rename to ")
                .or_else(|| line.strip_prefix(b"copy to "))
            {
                file.new_path = Some(decode_quoted_path(path)?);
                file.metadata.push(line.to_vec());
            } else if !line.is_empty() {
                file.metadata.push(line.to_vec());
            }
        }
        for hunk in diff.files.iter().flat_map(|file| &file.hunks) {
            let old_count = hunk
                .lines
                .iter()
                .filter(|line| matches!(line.kind, LineKind::Context | LineKind::Removed))
                .count();
            let new_count = hunk
                .lines
                .iter()
                .filter(|line| matches!(line.kind, LineKind::Context | LineKind::Added))
                .count();
            if old_count != hunk.old_count as usize || new_count != hunk.new_count as usize {
                return Err(
                    "Incomplete unified diff: hunk counts do not match canonical rows".into(),
                );
            }
        }
        Ok(diff)
    }

    pub fn additions(&self) -> usize {
        self.files
            .iter()
            .flat_map(|f| &f.hunks)
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == LineKind::Added)
            .count()
    }

    pub fn deletions(&self) -> usize {
        self.files
            .iter()
            .flat_map(|f| &f.hunks)
            .flat_map(|h| &h.lines)
            .filter(|l| l.kind == LineKind::Removed)
            .count()
    }

    /// Build a patch from canonical changed-line selections for an existing text file.
    /// Unselected removals become context; unselected additions are omitted. Earlier unselected
    /// edits cannot shift selected coordinates. Feed this patch to guarded `git apply --cached`.
    pub fn selected_patch(
        &self,
        file_index: usize,
        selected: &[(usize, usize)],
    ) -> Result<Vec<u8>, String> {
        let file = self
            .files
            .get(file_index)
            .ok_or("Selected diff file is unavailable")?;
        let path = match (&file.old_path, &file.new_path) {
            (Some(old), Some(new)) if old == new && !file.binary => old,
            _ => {
                return Err(
                    "Stage the entire file for additions, deletions, renames or binary changes"
                        .into(),
                )
            }
        };
        if selected.is_empty() {
            return Err("Select changed lines to stage".into());
        }
        let selected = selected
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        for (hunk, line) in &selected {
            let line = file
                .hunks
                .get(*hunk)
                .and_then(|hunk| hunk.lines.get(*line))
                .ok_or("Selected diff coordinates are unavailable")?;
            if !matches!(line.kind, LineKind::Added | LineKind::Removed) {
                return Err("Only changed lines can be staged".into());
            }
        }
        let old_path = quote_patch_path(b"a/", path);
        let new_path = quote_patch_path(b"b/", path);
        let mut patch =
            format!("diff --git {old_path} {new_path}\n--- {old_path}\n+++ {new_path}\n")
                .into_bytes();
        let mut offset = 0i64;
        for (hunk_index, hunk) in file.hunks.iter().enumerate() {
            if !selected.iter().any(|(index, _)| *index == hunk_index) {
                continue;
            }
            let mut body = Vec::new();
            let mut new_count = 0u32;
            let mut last_emitted = false;
            for (line_index, line) in hunk.lines.iter().enumerate() {
                let chosen = selected.contains(&(hunk_index, line_index));
                let prefix = match (line.kind, chosen) {
                    (LineKind::Context, _) => {
                        new_count += 1;
                        Some(b' ')
                    }
                    (LineKind::Removed, false) => {
                        new_count += 1;
                        Some(b' ')
                    }
                    (LineKind::Removed, true) => Some(b'-'),
                    (LineKind::Added, true) => {
                        new_count += 1;
                        Some(b'+')
                    }
                    (LineKind::Added, false) => None,
                    (LineKind::NoNewline, _) => last_emitted.then_some(b'\\'),
                };
                if let Some(prefix) = prefix {
                    body.push(prefix);
                    body.extend_from_slice(&line.text);
                    body.push(b'\n');
                }
                last_emitted = prefix.is_some();
            }
            let new_start = i64::from(hunk.old_start)
                + offset
                + if hunk.old_count == 0 {
                    1
                } else if new_count == 0 {
                    -1
                } else {
                    0
                };
            if new_start < 0 {
                return Err("Selected patch coordinates are invalid".into());
            }
            patch.extend_from_slice(
                format!(
                    "@@ -{},{} +{},{} @@\n",
                    hunk.old_start, hunk.old_count, new_start, new_count
                )
                .as_bytes(),
            );
            patch.extend_from_slice(&body);
            offset += i64::from(new_count) - i64::from(hunk.old_count);
        }
        Ok(patch)
    }

    /// Build an index-to-HEAD patch for unstaging. Selecting against the reversed canonical
    /// file keeps unselected staged additions as context and leaves staged removals removed.
    /// Reversing a partial HEAD-to-index patch would lose that neighboring index context.
    pub fn selected_unstage_patch(
        &self,
        file_index: usize,
        selected: &[(usize, usize)],
    ) -> Result<Vec<u8>, String> {
        let mut file = self
            .files
            .get(file_index)
            .ok_or("Selected diff file is unavailable")?
            .clone();
        std::mem::swap(&mut file.old_path, &mut file.new_path);
        for hunk in &mut file.hunks {
            std::mem::swap(&mut hunk.old_start, &mut hunk.new_start);
            std::mem::swap(&mut hunk.old_count, &mut hunk.new_count);
            for line in &mut hunk.lines {
                std::mem::swap(&mut line.old_line, &mut line.new_line);
                line.kind = match line.kind {
                    LineKind::Added => LineKind::Removed,
                    LineKind::Removed => LineKind::Added,
                    kind => kind,
                };
            }
        }
        Self {
            base: self.head.clone(),
            head: self.base.clone(),
            files: vec![file],
        }
        .selected_patch(0, selected)
    }
}

fn quote_patch_path(prefix: &[u8], path: &[u8]) -> String {
    let mut result = String::from("\"");
    for byte in prefix.iter().chain(path) {
        match byte {
            b'"' => result.push_str("\\\""),
            b'\\' => result.push_str("\\\\"),
            b' '..=b'~' => result.push(*byte as char),
            _ => result.push_str(&format!("\\{byte:03o}")),
        }
    }
    result.push('"');
    result
}

fn parse_header(line: &[u8]) -> Result<Hunk, String> {
    let mut fields = line.splitn(5, |byte| *byte == b' ');
    if fields.next() != Some(b"@@".as_slice()) {
        return Err("Missing hunk delimiter".into());
    }
    let old_range = std::str::from_utf8(fields.next().unwrap_or_default())
        .map_err(|_| "Invalid old hunk coordinate")?;
    let new_range = std::str::from_utf8(fields.next().unwrap_or_default())
        .map_err(|_| "Invalid new hunk coordinate")?;
    let (old_start, old_count) = parse_range(old_range, '-')?;
    let (new_start, new_count) = parse_range(new_range, '+')?;
    if fields.next() != Some(b"@@".as_slice()) {
        return Err("Missing closing hunk delimiter".into());
    }
    Ok(Hunk {
        old_start,
        old_count,
        new_start,
        new_count,
        heading: fields.next().unwrap_or_default().to_vec(),
        lines: Vec::new(),
    })
}

fn parse_range(value: &str, prefix: char) -> Result<(u32, u32), String> {
    let value = value
        .strip_prefix(prefix)
        .ok_or("Invalid hunk coordinate prefix")?;
    let mut parts = value.split(',');
    let start: u32 = parts
        .next()
        .unwrap_or_default()
        .parse()
        .map_err(|_| "Invalid hunk start")?;
    let count: u32 = parts
        .next()
        .map(str::parse)
        .transpose()
        .map_err(|_| "Invalid hunk count")?
        .unwrap_or(1);
    if parts.next().is_some() {
        return Err("Too many hunk coordinates".into());
    }
    if count > 0 && (start == 0 || start.checked_add(count - 1).is_none()) {
        return Err("Hunk span has zero or overflowing canonical line coordinates".into());
    }
    Ok((start, count))
}

fn decode_diff_path(path: &[u8], prefix: &[u8]) -> Result<Option<Vec<u8>>, String> {
    // Unified file headers delimit their optional timestamp with a tab. Git also emits an
    // empty tab suffix for some quoted/space-bearing paths; it is not part of the filename.
    // Literal filename tabs are C-quoted and decoded below, never split inside the quotes.
    let path = if path.first() == Some(&b'"') {
        let mut escaped = false;
        let end = path
            .iter()
            .enumerate()
            .skip(1)
            .find_map(|(index, byte)| {
                if escaped {
                    escaped = false;
                    return None;
                }
                if *byte == b'\\' {
                    escaped = true;
                    return None;
                }
                (*byte == b'"').then_some(index)
            })
            .ok_or("Unterminated quoted Git path")?;
        if path.len() > end + 1 && path[end + 1] != b'\t' {
            return Err("Unexpected suffix after quoted Git file header".into());
        }
        &path[..=end]
    } else {
        path.split(|byte| *byte == b'\t').next().unwrap_or(path)
    };
    if path == b"/dev/null" {
        return Ok(None);
    }
    let decoded = decode_quoted_path(path)?;
    Ok(Some(
        decoded.strip_prefix(prefix).unwrap_or(&decoded).to_vec(),
    ))
}

pub(crate) fn decode_quoted_path(path: &[u8]) -> Result<Vec<u8>, String> {
    if path.first() != Some(&b'"') {
        return Ok(path.to_vec());
    }
    if path.last() != Some(&b'"') {
        return Err("Unterminated quoted Git path".into());
    }
    let path = &path[1..path.len() - 1];
    let mut result = Vec::new();
    let mut i = 0;
    while i < path.len() {
        if path[i] != b'\\' {
            result.push(path[i]);
            i += 1;
            continue;
        }
        i += 1;
        let value = *path.get(i).ok_or("Trailing Git path escape")?;
        result.push(match value {
            b'a' => 7,
            b'b' => 8,
            b't' => b'\t',
            b'n' => b'\n',
            b'v' => 11,
            b'f' => 12,
            b'r' => b'\r',
            b'\\' | b'"' => value,
            b'0'..=b'7' => {
                let mut octal = u16::from(value - b'0');
                for _ in 0..2 {
                    i += 1;
                    let next = *path.get(i).ok_or("Short Git path octal escape")?;
                    if !(b'0'..=b'7').contains(&next) {
                        return Err("Invalid Git path octal escape".into());
                    }
                    octal = octal * 8 + u16::from(next - b'0');
                }
                u8::try_from(octal).map_err(|_| "Git path octal escape overflow")?
            }
            _ => return Err("Unknown Git path escape".into()),
        });
        i += 1;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn staging_header_paths_split_timestamp_only_outside_quoted_exact_bytes() {
        assert_eq!(
            decode_diff_path(b"\"b/new \\346\\227\\245.txt\"\t", b"b/").unwrap(),
            Some("new 日.txt".as_bytes().to_vec())
        );
        assert_eq!(
            decode_diff_path("b/new 日本語.txt\t2026-10-13 12:00:00".as_bytes(), b"b/").unwrap(),
            Some("new 日本語.txt".as_bytes().to_vec())
        );
        assert_eq!(
            decode_diff_path(b"\"b/tab\\tname\\\".rs\"\tstamp", b"b/").unwrap(),
            Some(b"tab\tname\".rs".to_vec())
        );
        assert!(decode_diff_path(b"\"b/path\" arbitrary suffix", b"b/").is_err());
        assert!(decode_diff_path(b"\"b/path", b"b/").is_err());
    }

    #[test]
    fn diff_explorer_canonical_coordinates() {
        let diff = Diff::parse(b"diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -3,2 +3,3 @@ fn example\n same\n-old\n+new\n+extra\n\\ No newline at end of file\n").unwrap();
        let lines = &diff.files[0].hunks[0].lines;
        assert_eq!((lines[0].old_line, lines[0].new_line), (Some(3), Some(3)));
        assert_eq!((lines[1].old_line, lines[1].new_line), (Some(4), None));
        assert_eq!((lines[2].old_line, lines[2].new_line), (None, Some(4)));
        assert_eq!((lines[3].old_line, lines[3].new_line), (None, Some(5)));
        assert_eq!(diff.additions(), 2);
        assert_eq!(diff.deletions(), 1);
    }

    #[test]
    fn diff_explorer_decodes_raw_paths_and_root_additions() {
        let diff = Diff::parse(b"diff --git a/x b/x\n--- /dev/null\n+++ \"b/tab\\tline\\n\\377\"\n@@ -0,0 +1 @@\n+hello\n").unwrap();
        assert_eq!(diff.files[0].old_path, None);
        assert_eq!(diff.files[0].new_path, Some(b"tab\tline\n\xff".to_vec()));
        assert_eq!(diff.files[0].hunks[0].lines[0].new_line, Some(1));
    }

    #[test]
    fn native_diff_retains_non_utf8_headings_and_rejects_truncated_coordinates() {
        let diff = Diff::parse(
            b"diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1 +1 @@ function\xff\n-old\n+new\n",
        )
        .unwrap();
        assert_eq!(diff.files[0].hunks[0].heading, b"function\xff");
        assert!(Diff::parse(
            b"diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n-old\n+new\n"
        )
        .unwrap_err()
        .contains("Incomplete"));
    }
}
