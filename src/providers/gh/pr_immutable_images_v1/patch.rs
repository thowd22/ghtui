use super::*;
use sha2::Digest;

fn decoded(value: &[u8]) -> GithubResult<Vec<u8>> {
    crate::diff::decode_quoted_path(value).map_err(|_| safe(GithubErrorKind::Json))
}
fn path_valid(path: &[u8], limits: &PrImmutableFileLimitsV1) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && std::str::from_utf8(path).is_ok()
        && path.split(|byte| *byte == b'/').count() <= limits.max_path_depth
        && !path
            .split(|byte| *byte == b'/')
            .any(|part| part.is_empty() || part == b"." || part == b".." || part.contains(&0))
}
fn header_matches(raw: &[u8], old: &[u8], new: &[u8]) -> GithubResult<bool> {
    let old = [b"a/".as_slice(), old].concat();
    let new = [b"b/".as_slice(), new].concat();
    let tail = if raw.first() == Some(&b'"') {
        let mut escaped = false;
        let mut end = None;
        for (index, byte) in raw.iter().enumerate().skip(1) {
            if escaped {
                escaped = false;
                continue;
            }
            if *byte == b'\\' {
                escaped = true;
                continue;
            }
            if *byte == b'"' {
                end = Some(index);
                break;
            }
        }
        let end = end.ok_or_else(|| safe(GithubErrorKind::Json))?;
        if decoded(&raw[..=end])? != old {
            return Ok(false);
        }
        raw.get(end + 1..).and_then(|raw| raw.strip_prefix(b" "))
    } else {
        raw.strip_prefix(old.as_slice())
            .and_then(|raw| raw.strip_prefix(b" "))
    };
    Ok(tail.is_some_and(|tail| decoded(tail).is_ok_and(|path| path == new)))
}
pub(super) fn selected_patch(
    raw: &[u8],
    metadata: &ObservedPrFileV1,
    limits: &PrImmutableFileLimitsV1,
) -> GithubResult<SelectedPatchV1> {
    if raw.len() > 1024 * 1024 {
        return Err(safe(GithubErrorKind::Unsupported));
    }
    let status = metadata
        .status
        .as_deref()
        .ok_or_else(|| safe(GithubErrorKind::Unsupported))?;
    let new = metadata.path.as_bytes();
    let old = if matches!(status, "renamed" | "copied") {
        metadata
            .previous_path
            .as_deref()
            .ok_or_else(|| safe(GithubErrorKind::Unsupported))?
            .as_bytes()
    } else {
        new
    };
    if !path_valid(old, limits) || !path_valid(new, limits) {
        return Err(safe(GithubErrorKind::Unsupported));
    }
    if !matches!(
        status,
        "added" | "removed" | "modified" | "renamed" | "copied" | "changed"
    ) {
        return Err(safe(GithubErrorKind::Unsupported));
    }
    let mut starts = Vec::new();
    let mut position = 0;
    for line in raw.split_inclusive(|byte| *byte == b'\n') {
        if line.starts_with(b"diff --git ") {
            starts.push(position);
        }
        position += line.len();
    }
    if starts.len() > 65536 {
        return Err(safe(GithubErrorKind::Unsupported));
    }
    starts.push(raw.len());
    let mut selected = None;
    for pair in starts.windows(2) {
        let section = &raw[pair[0]..pair[1]];
        let header = section
            .split(|byte| *byte == b'\n')
            .next()
            .and_then(|line| line.strip_prefix(b"diff --git "))
            .ok_or_else(|| safe(GithubErrorKind::Json))?;
        if header.len() > 32 * 1024 {
            return Err(safe(GithubErrorKind::Unsupported));
        }
        if header_matches(header, old, new)? && selected.replace(section).is_some() {
            return Err(safe(GithubErrorKind::Json));
        }
    }
    let selected = selected.ok_or_else(|| safe(GithubErrorKind::StaleHead))?;
    if selected.split(|byte| *byte == b'\n').count() > 65536 {
        return Err(safe(GithubErrorKind::Unsupported));
    }
    for line in selected
        .split(|byte| *byte == b'\n')
        .filter(|line| line.starts_with(b"@@ "))
    {
        let text = std::str::from_utf8(line).map_err(|_| safe(GithubErrorKind::Json))?;
        let mut parts = text.split_whitespace();
        parts.next();
        for sign in ['-', '+'] {
            let part = parts
                .next()
                .and_then(|part| part.strip_prefix(sign))
                .ok_or_else(|| safe(GithubErrorKind::Json))?;
            let (start, count) = part.split_once(',').unwrap_or((part, "1"));
            let start = start
                .parse::<u32>()
                .map_err(|_| safe(GithubErrorKind::Json))?;
            let count = count
                .parse::<u32>()
                .map_err(|_| safe(GithubErrorKind::Json))?;
            // Actual image/prepared-line bounds also protect the legacy parser
            // from arithmetic overflow on lying count=0 hunk headers.
            if start > 65536 || count > 65536 {
                return Err(safe(GithubErrorKind::Unsupported));
            }
            start
                .checked_add(count)
                .ok_or_else(|| safe(GithubErrorKind::Json))?;
        }
    }
    let mut parsed = crate::diff::Diff::parse(selected).map_err(|_| safe(GithubErrorKind::Json))?;
    if parsed.files.len() != 1 {
        return Err(safe(GithubErrorKind::Json));
    }
    let mut file = parsed.files.pop().unwrap();
    let old_path = (status != "added").then(|| old.to_vec());
    let new_path = (status != "removed").then(|| new.to_vec());
    if file
        .old_path
        .as_ref()
        .is_some_and(|path| Some(path) != old_path.as_ref())
        || file
            .new_path
            .as_ref()
            .is_some_and(|path| Some(path) != new_path.as_ref())
    {
        return Err(safe(GithubErrorKind::StaleHead));
    }
    let mut saw_old = false;
    let mut saw_new = false;
    let mut added = false;
    let mut removed = false;
    for line in selected.split(|byte| *byte == b'\n') {
        for prefix in [b"rename from ".as_slice(), b"copy from ".as_slice()] {
            if let Some(value) = line.strip_prefix(prefix) {
                if decoded(value)? != old {
                    return Err(safe(GithubErrorKind::StaleHead));
                }
                saw_old = true;
            }
        }
        for prefix in [b"rename to ".as_slice(), b"copy to ".as_slice()] {
            if let Some(value) = line.strip_prefix(prefix) {
                if decoded(value)? != new {
                    return Err(safe(GithubErrorKind::StaleHead));
                }
                saw_new = true;
            }
        }
        added |= line.starts_with(b"new file mode ");
        removed |= line.starts_with(b"deleted file mode ");
    }
    if (status == "added" && !added)
        || (status == "removed" && !removed)
        || (added && status != "added")
        || (removed && status != "removed")
        || (matches!(status, "renamed" | "copied") && (!saw_old || !saw_new))
    {
        return Err(safe(GithubErrorKind::Json));
    }
    file.old_path = old_path.clone();
    file.new_path = new_path.clone();
    Ok(SelectedPatchV1 {
        old_path,
        new_path,
        old_lookup: old.to_vec(),
        new_lookup: new.to_vec(),
        file: Arc::new(file),
        digest: format!("{:x}", sha2::Sha256::digest(selected)),
    })
}
struct PatchLine {
    kind: crate::diff::LineKind,
    bytes: Vec<u8>,
    old: Option<u32>,
    new: Option<u32>,
}
fn patch_lines(hunk: &crate::diff::Hunk) -> GithubResult<Vec<PatchLine>> {
    use crate::diff::LineKind;
    let mut result: Vec<PatchLine> = Vec::with_capacity(hunk.lines.len());
    for line in &hunk.lines {
        if line.kind == LineKind::NoNewline {
            if line.text != b" No newline at end of file" {
                return Err(safe(GithubErrorKind::Unsupported));
            }
            let previous = result
                .last_mut()
                .filter(|line| line.bytes.ends_with(b"\n"))
                .ok_or_else(|| safe(GithubErrorKind::Json))?;
            previous.bytes.pop();
        } else {
            let mut bytes = line.text.clone();
            bytes.push(b'\n');
            result.push(PatchLine {
                kind: line.kind,
                bytes,
                old: line.old_line,
                new: line.new_line,
            });
        }
    }
    Ok(result)
}
fn apply(file: &crate::diff::FileDiff, old: &[u8], new: &[u8], max: usize) -> GithubResult<()> {
    use crate::diff::LineKind;
    let old_lines = old
        .split_inclusive(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    let new_lines = new
        .split_inclusive(|byte| *byte == b'\n')
        .collect::<Vec<_>>();
    if old_lines.len() > 65536 || new_lines.len() > 65536 {
        return Err(safe(GithubErrorKind::Unsupported));
    }
    let mut output = Vec::with_capacity(new.len());
    let mut cursor = 0;
    let mut produced_lines = 0usize;
    for hunk in &file.hunks {
        let actual_old = hunk
            .lines
            .iter()
            .filter(|line| matches!(line.kind, LineKind::Context | LineKind::Removed))
            .count();
        let actual_new = hunk
            .lines
            .iter()
            .filter(|line| matches!(line.kind, LineKind::Context | LineKind::Added))
            .count();
        if actual_old != hunk.old_count as usize || actual_new != hunk.new_count as usize {
            return Err(safe(GithubErrorKind::Json));
        }
        let start = if hunk.old_count == 0 {
            hunk.old_start as usize
        } else {
            hunk.old_start
                .checked_sub(1)
                .ok_or_else(|| safe(GithubErrorKind::Json))? as usize
        };
        if start < cursor || start > old_lines.len() {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        for line in &old_lines[cursor..start] {
            output.extend_from_slice(line);
            produced_lines += 1;
        }
        cursor = start;
        let new_start = if hunk.new_count == 0 {
            hunk.new_start as usize
        } else {
            hunk.new_start
                .checked_sub(1)
                .ok_or_else(|| safe(GithubErrorKind::Json))? as usize
        };
        if new_start != produced_lines {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        for line in patch_lines(hunk)? {
            if matches!(line.kind, LineKind::Context | LineKind::Removed) {
                if line.old.map(|line| line as usize) != Some(cursor + 1)
                    || old_lines.get(cursor).copied() != Some(line.bytes.as_slice())
                {
                    return Err(safe(GithubErrorKind::StaleHead));
                }
                cursor += 1;
            }
            if matches!(line.kind, LineKind::Context | LineKind::Added) {
                if line.new.map(|line| line as usize) != Some(produced_lines + 1)
                    || new_lines.get(produced_lines).copied() != Some(line.bytes.as_slice())
                {
                    return Err(safe(GithubErrorKind::StaleHead));
                }
                if output
                    .len()
                    .checked_add(line.bytes.len())
                    .is_none_or(|size| size > max)
                {
                    return Err(safe(GithubErrorKind::Unsupported));
                }
                output.extend_from_slice(&line.bytes);
                produced_lines += 1;
            }
        }
    }
    for line in &old_lines[cursor..] {
        if output
            .len()
            .checked_add(line.len())
            .is_none_or(|size| size > max)
        {
            return Err(safe(GithubErrorKind::Unsupported));
        }
        output.extend_from_slice(line);
    }
    if output != new {
        return Err(safe(GithubErrorKind::StaleHead));
    }
    Ok(())
}
pub(super) fn associate_patch(
    selected: &SelectedPatchV1,
    metadata: &ObservedPrFileV1,
    old: &PrImmutableImageV1,
    new: &PrImmutableImageV1,
    limits: &PrImmutableFileLimitsV1,
) -> GithubResult<bool> {
    if old.absent != (selected.old_path.is_none()) || new.absent != (selected.new_path.is_none()) {
        return Err(safe(GithubErrorKind::StaleHead));
    }
    if let Some(sha) = &metadata.blob_sha {
        if !old
            .blob
            .as_ref()
            .is_some_and(|blob| blob.eq_ignore_ascii_case(sha))
            && !new
                .blob
                .as_ref()
                .is_some_and(|blob| blob.eq_ignore_ascii_case(sha))
        {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        // This records consistency with a full proven object, NEVER guesses the
        // ambiguous REST SHA side or chooses a head blob from that metadata.
    }
    let additions = selected
        .file
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .filter(|line| line.kind == crate::diff::LineKind::Added)
        .count() as u64;
    let deletions = selected
        .file
        .hunks
        .iter()
        .flat_map(|hunk| &hunk.lines)
        .filter(|line| line.kind == crate::diff::LineKind::Removed)
        .count() as u64;
    if metadata.additions.is_some_and(|value| value != additions)
        || metadata.deletions.is_some_and(|value| value != deletions)
    {
        return Err(safe(GithubErrorKind::StaleHead));
    }
    for line in &selected.file.metadata {
        if let Some(index) = line.strip_prefix(b"index ") {
            let text = std::str::from_utf8(index).map_err(|_| safe(GithubErrorKind::Json))?;
            let mut fields = text.split_whitespace();
            let pair = fields
                .next()
                .and_then(|field| field.split_once(".."))
                .ok_or_else(|| safe(GithubErrorKind::Json))?;
            for (observed, image) in [(pair.0, old), (pair.1, new)] {
                if !(7..=64).contains(&observed.len())
                    || !observed.bytes().all(|byte| byte.is_ascii_hexdigit())
                {
                    return Err(safe(GithubErrorKind::Json));
                }
                // An abbreviation is consistency metadata ONLY. Objects were
                // located and fully hashed independently before this comparison.
                if image.absent {
                    if !observed.bytes().all(|byte| byte == b'0') {
                        return Err(safe(GithubErrorKind::StaleHead));
                    }
                } else if image.blob.as_deref().is_none_or(|sha| {
                    observed.len() > sha.len()
                        || !sha[..observed.len()].eq_ignore_ascii_case(observed)
                }) {
                    return Err(safe(GithubErrorKind::StaleHead));
                }
            }
            if let Some(mode) = fields.next() {
                if old.mode.as_deref() != Some(mode)
                    || new.mode.as_deref() != Some(mode)
                    || fields.next().is_some()
                {
                    return Err(safe(GithubErrorKind::StaleHead));
                }
            }
        }
        for (prefix, expected) in [
            (b"new file mode ".as_slice(), new.mode.as_deref()),
            (b"deleted file mode ".as_slice(), old.mode.as_deref()),
            (b"old mode ".as_slice(), old.mode.as_deref()),
            (b"new mode ".as_slice(), new.mode.as_deref()),
        ] {
            if let Some(mode) = line.strip_prefix(prefix) {
                if std::str::from_utf8(mode).ok() != expected {
                    return Err(safe(GithubErrorKind::StaleHead));
                }
            }
        }
    }
    if selected.file.binary
        || old.kind == Some(PrImmutableImageKindV1::Submodule)
        || new.kind == Some(PrImmutableImageKindV1::Submodule)
    {
        return Ok(false);
    }
    let old_bytes = old.bytes.as_deref().unwrap_or(&[]);
    let new_bytes = new.bytes.as_deref().unwrap_or(&[]);
    apply(&selected.file, old_bytes, new_bytes, limits.max_blob_bytes)?;
    Ok(!selected.file.hunks.is_empty()
        && [old.kind, new.kind]
            .into_iter()
            .all(|kind| matches!(kind, None | Some(PrImmutableImageKindV1::Text))))
}
