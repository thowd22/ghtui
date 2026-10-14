use super::*;
use sha2::Digest;

pub(super) fn oid(value: &str) -> GithubResult<()> {
    if !matches!(value.len(), 40 | 64)
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || value.bytes().all(|b| b == b'0')
    {
        return Err(remote_error(
            GithubErrorKind::Json,
            "Remote object identity is missing or unverified",
        ));
    }
    Ok(())
}
pub(super) fn field<'a>(value: &'a Value, key: &str, max: usize) -> GithubResult<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() <= max)
        .ok_or_else(|| {
            remote_error(
                GithubErrorKind::Json,
                "Remote field is missing/malformed or exceeds bound",
            )
        })
}
pub(super) fn reference_name(name: &str) -> GithubResult<()> {
    if name.is_empty()
        || name.len() > 1024
        || name.chars().any(|c| c.is_control() || c.is_whitespace())
        || name.starts_with('/')
        || name.ends_with('/')
        || name.ends_with('.')
        || name.contains("//")
        || name.contains("..")
        || name.contains("@{")
        || name.bytes().any(|b| b"~^:?*[\\".contains(&b))
        || name
            .split('/')
            .any(|s| s.starts_with('.') || s.ends_with(".lock"))
    {
        return Err(remote_error(
            GithubErrorKind::InvalidTarget,
            "Remote ref requires a bounded exact literal name",
        ));
    }
    Ok(())
}
pub(super) fn encoded(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}
pub(super) fn joined(parent: &str, name: &str) -> GithubResult<String> {
    if name.is_empty()
        || name.len() > 4096
        || name.contains('/')
        || name.contains('\0')
        || matches!(name, "." | "..")
        || parent.len().saturating_add(name.len()).saturating_add(1) > 4096
        || parent.split('/').count() > 64
    {
        return Err(remote_error(
            GithubErrorKind::InvalidTarget,
            "Remote path exceeds exact segment/depth bounds",
        ));
    }
    Ok(if parent.is_empty() {
        name.to_owned()
    } else {
        format!("{parent}/{name}")
    })
}
pub(super) fn directory(
    value: Value,
    expected: &str,
    limits: &RemoteRepositoryLimitsV1,
) -> GithubResult<(Arc<[RemoteEntryV1]>, bool)> {
    if field(&value, "sha", 64)? != expected {
        return Err(remote_error(
            GithubErrorKind::IdentityChanged,
            "Remote tree object changed",
        ));
    }
    let truncated = value
        .get("truncated")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            remote_error(
                GithubErrorKind::Json,
                "Remote tree completeness was not reported",
            )
        })?;
    let rows = value.get("tree").and_then(Value::as_array).ok_or_else(|| {
        remote_error(
            GithubErrorKind::Json,
            "Remote tree is not an explicit array",
        )
    })?;
    if rows.len() > limits.max_directory_rows {
        return Err(remote_error(
            GithubErrorKind::Unsupported,
            "Remote directory row cap reached; no complete frame claimed",
        ));
    }
    // Bound raw decoded fields BEFORE clone, and exact row namespaces.
    let mut bytes = 0usize;
    let mut seen = BTreeSet::new();
    for row in rows {
        let name = field(row, "path", 4096)?;
        joined("", name)?;
        let id = field(row, "sha", 64)?;
        oid(id)?;
        let mode = field(row, "mode", 6)?;
        let kind = field(row, "type", 16)?;
        if id.len() != expected.len()
            || !matches!(
                (mode, kind),
                ("040000", "tree")
                    | ("100644" | "100755" | "120000", "blob")
                    | ("160000", "commit")
            )
            || !seen.insert(name)
        {
            return Err(remote_error(
                GithubErrorKind::Json,
                "Remote tree modes/types/path/object identities contradict",
            ));
        }
        if row
            .get("size")
            .is_some_and(|v| !v.is_null() && v.as_u64().is_none())
        {
            return Err(remote_error(
                GithubErrorKind::Json,
                "Remote object size is malformed; never zero guessed",
            ));
        }
        bytes = bytes
            .checked_add(name.len() + id.len() + 128)
            .filter(|n| *n <= limits.max_metadata_bytes)
            .ok_or_else(|| {
                remote_error(
                    GithubErrorKind::Unsupported,
                    "Remote metadata cap reached before copies",
                )
            })?;
    }
    let entries = rows
        .iter()
        .map(|row| {
            let mode = row["mode"].as_str().expect("preflight");
            RemoteEntryV1 {
                name: row["path"].as_str().expect("preflight").into(),
                oid: row["sha"].as_str().expect("preflight").into(),
                mode: mode.into(),
                kind: match mode {
                    "040000" => RemoteEntryKindV1::Directory,
                    "100755" => RemoteEntryKindV1::Executable,
                    "120000" => RemoteEntryKindV1::Symlink,
                    "160000" => RemoteEntryKindV1::Submodule,
                    _ => RemoteEntryKindV1::File,
                },
                size: row.get("size").and_then(Value::as_u64),
            }
        })
        .collect::<Vec<_>>();
    if !truncated {
        verify_tree(&entries, expected, limits.max_metadata_bytes)?;
    }
    Ok((entries.into(), !truncated))
}
fn verify_tree(entries: &[RemoteEntryV1], expected: &str, cap: usize) -> GithubResult<()> {
    let mut sorted = entries.iter().collect::<Vec<_>>();
    sorted.sort_by(|a, b| {
        let a = a
            .name
            .as_bytes()
            .iter()
            .copied()
            .chain((a.kind == RemoteEntryKindV1::Directory).then_some(b'/'));
        let b = b
            .name
            .as_bytes()
            .iter()
            .copied()
            .chain((b.kind == RemoteEntryKindV1::Directory).then_some(b'/'));
        a.cmp(b)
    });
    let size = sorted
        .iter()
        .try_fold(0usize, |n, e| {
            n.checked_add(
                e.mode.trim_start_matches('0').len() + 1 + e.name.len() + 1 + expected.len() / 2,
            )
        })
        .filter(|n| *n <= cap)
        .ok_or_else(|| {
            remote_error(
                GithubErrorKind::Unsupported,
                "Git tree reconstruction exceeds bound",
            )
        })?;
    let mut raw = Vec::with_capacity(size);
    for entry in sorted {
        raw.extend(entry.mode.trim_start_matches('0').as_bytes());
        raw.push(b' ');
        raw.extend(entry.name.as_bytes());
        raw.push(0);
        for pair in entry.oid.as_bytes().chunks_exact(2) {
            let digit = |b: u8| {
                if b.is_ascii_digit() {
                    b - b'0'
                } else {
                    b - b'a' + 10
                }
            };
            raw.push(digit(pair[0]) * 16 + digit(pair[1]));
        }
    }
    let header = format!("tree {}\0", raw.len());
    let digest = if expected.len() == 40 {
        let mut h = sha1::Sha1::new();
        h.update(header);
        h.update(raw);
        format!("{:x}", h.finalize())
    } else {
        let mut h = sha2::Sha256::new();
        h.update(header);
        h.update(raw);
        format!("{:x}", h.finalize())
    };
    if digest != expected {
        return Err(remote_error(
            GithubErrorKind::IdentityChanged,
            "Complete remote tree digest does not match its immutable object",
        ));
    }
    Ok(())
}
pub(super) fn commit(value: &Value, expected: &str) -> GithubResult<String> {
    if field(value, "sha", 64)? != expected {
        return Err(remote_error(
            GithubErrorKind::IdentityChanged,
            "Remote saved commit differs",
        ));
    }
    let tree = field(
        value
            .get("tree")
            .ok_or_else(|| remote_error(GithubErrorKind::Json, "Remote commit lacks root tree"))?,
        "sha",
        64,
    )?;
    oid(tree)?;
    if tree.len() != expected.len() {
        return Err(remote_error(
            GithubErrorKind::Json,
            "Remote object hash namespaces differ",
        ));
    }
    Ok(tree.into())
}
