use super::*;
use sha2::Digest;

#[derive(Default)]
pub(super) struct TreeReadState {
    pub rows: usize,
    pub reads: usize,
}
pub(super) struct Entry {
    pub(super) path: Vec<u8>,
    pub(super) mode: String,
    pub(super) kind: String,
    pub(super) sha: String,
    pub(super) size: Option<u64>,
}
pub(super) fn validate_optional_url(
    provider: &GithubProvider,
    raw: &Value,
    field: &str,
    path: &str,
) -> GithubResult<()> {
    match raw.get(field) {
        None | Some(Value::Null) => Ok(()),
        Some(Value::String(url))
            if canonical_source_url_matches(&provider.host, url, path, true, false) =>
        {
            Ok(())
        }
        _ => Err(safe(GithubErrorKind::IdentityChanged)),
    }
}
pub(super) fn commit_tree(
    provider: &GithubProvider,
    repository: &str,
    sha: &str,
    limits: &PrImmutableFileLimitsV1,
    cancel: &CancellationToken,
) -> GithubResult<String> {
    let endpoint = format!("repos/{repository}/git/commits/{sha}");
    let raw = document(provider, &endpoint, limits.max_parent_bytes, cancel, false)?;
    if raw["sha"].as_str() != Some(sha) {
        return Err(safe(GithubErrorKind::StaleHead));
    }
    validate_optional_url(provider, &raw, "url", &endpoint)?;
    let tree = raw["tree"]["sha"]
        .as_str()
        .filter(|tree| full_sha(tree) && tree.len() == sha.len())
        .ok_or_else(|| safe(GithubErrorKind::Json))?;
    validate_optional_url(
        provider,
        &raw["tree"],
        "url",
        &format!("repos/{repository}/git/trees/{tree}"),
    )?;
    // Actual server-linked commit/tree only. No commit JSON hash/signature claim.
    Ok(tree.into())
}
fn hex_bytes(value: &str) -> GithubResult<Vec<u8>> {
    if !full_sha(value) {
        return Err(safe(GithubErrorKind::Json));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |b: u8| match b {
                b'0'..=b'9' => Some(b - b'0'),
                b'a'..=b'f' => Some(b - b'a' + 10),
                b'A'..=b'F' => Some(b - b'A' + 10),
                _ => None,
            };
            Ok(
                digit(pair[0]).ok_or_else(|| safe(GithubErrorKind::Json))? * 16
                    + digit(pair[1]).ok_or_else(|| safe(GithubErrorKind::Json))?,
            )
        })
        .collect()
}
pub(super) fn verify_tree(entries: &mut [Entry], sha: &str, max: usize) -> GithubResult<()> {
    let cost = entries.iter().try_fold(0usize, |sum, entry| {
        sum.checked_add(entry.path.len())?
            .checked_add(entry.mode.len())?
            .checked_add(2 + sha.len() / 2)
    });
    if cost.is_none_or(|cost| cost > max) {
        return Err(safe(GithubErrorKind::Unsupported));
    }
    // Git sorts a directory as if its name had a trailing slash. Never casefold
    // file bytes. Sort only this bounded worker object, never terminal rows.
    entries.sort_by(|a, b| {
        a.path
            .iter()
            .copied()
            .chain((a.kind == "tree").then_some(b'/'))
            .cmp(
                b.path
                    .iter()
                    .copied()
                    .chain((b.kind == "tree").then_some(b'/')),
            )
    });
    let mut body = Vec::with_capacity(cost.unwrap());
    for entry in entries {
        body.extend_from_slice(if entry.mode == "040000" {
            b"40000"
        } else {
            entry.mode.as_bytes()
        });
        body.push(b' ');
        body.extend_from_slice(&entry.path);
        body.push(0);
        body.extend(hex_bytes(&entry.sha)?);
    }
    let header = format!("tree {}\0", body.len());
    let actual = if sha.len() == 40 {
        let mut digest = sha1::Sha1::new();
        digest.update(header.as_bytes());
        digest.update(body);
        format!("{:x}", digest.finalize())
    } else {
        let mut digest = sha2::Sha256::new();
        digest.update(header.as_bytes());
        digest.update(body);
        format!("{:x}", digest.finalize())
    };
    if !actual.eq_ignore_ascii_case(sha) {
        return Err(safe(GithubErrorKind::StaleHead));
    }
    Ok(())
}
pub(super) fn tree(
    provider: &GithubProvider,
    repo: &str,
    sha: &str,
    limits: &PrImmutableFileLimitsV1,
    cancel: &CancellationToken,
    state: &mut TreeReadState,
) -> GithubResult<Vec<Entry>> {
    checked_finished(provider, cancel)?;
    let endpoint = format!("repos/{repo}/git/trees/{sha}"); // NEVER recursive=false.
    let raw = document(
        provider,
        &endpoint,
        limits.max_document_bytes,
        cancel,
        false,
    )?;
    state.reads = state
        .reads
        .checked_add(1)
        .ok_or_else(|| safe(GithubErrorKind::Json))?;
    if raw["sha"].as_str() != Some(sha) || raw["truncated"].as_bool() != Some(false) {
        return Err(safe(GithubErrorKind::Json));
    }
    if let Some(value) = raw.get("url").filter(|value| !value.is_null()) {
        let url = value.as_str().ok_or_else(|| safe(GithubErrorKind::Json))?;
        if ![
            format!("repos/{repo}/git/trees/{sha}"),
            format!("repos/{repo}/trees/{sha}"),
        ]
        .iter()
        .any(|path| canonical_source_url_matches(&provider.host, url, path, true, false))
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
    }
    let rows = raw["tree"]
        .as_array()
        .filter(|rows| rows.len() <= limits.max_tree_rows)
        .ok_or_else(|| safe(GithubErrorKind::Unsupported))?;
    state.rows = state
        .rows
        .checked_add(rows.len())
        .filter(|rows| *rows <= limits.max_aggregate_tree_rows)
        .ok_or_else(|| safe(GithubErrorKind::Unsupported))?;
    let mut entries = Vec::with_capacity(rows.len());
    let mut names = BTreeSet::new();
    for row in rows {
        let path = row["path"]
            .as_str()
            .filter(|name| {
                !name.is_empty()
                    && name.len() <= 4096
                    && !name.contains(['\0', '/'])
                    && !matches!(*name, "." | "..")
            })
            .ok_or_else(|| safe(GithubErrorKind::Json))?;
        if !names.insert(path) {
            return Err(safe(GithubErrorKind::Json));
        }
        let mode = row["mode"]
            .as_str()
            .ok_or_else(|| safe(GithubErrorKind::Json))?;
        let kind = row["type"]
            .as_str()
            .ok_or_else(|| safe(GithubErrorKind::Json))?;
        if !matches!(
            (mode, kind),
            ("040000", "tree") | ("100644" | "100755" | "120000", "blob") | ("160000", "commit")
        ) {
            return Err(safe(GithubErrorKind::Unsupported));
        }
        let object = row["sha"]
            .as_str()
            .filter(|object| full_sha(object) && object.len() == sha.len())
            .ok_or_else(|| safe(GithubErrorKind::Json))?;
        // URLs are consistency metadata only; documented tree examples contain
        // tree entries with blob-shaped URLs. NEVER follow/mint type from them.
        if let Some(url) = row.get("url").filter(|v| !v.is_null()) {
            let url = url.as_str().ok_or_else(|| safe(GithubErrorKind::Json))?;
            if !["blobs", "trees", "commits"].iter().any(|family| {
                canonical_source_url_matches(
                    &provider.host,
                    url,
                    &format!("repos/{repo}/git/{family}/{object}"),
                    true,
                    false,
                )
            }) {
                return Err(safe(GithubErrorKind::IdentityChanged));
            }
        }
        let size = match row.get("size") {
            None | Some(Value::Null) => None,
            Some(v) => Some(v.as_u64().ok_or_else(|| safe(GithubErrorKind::Json))?),
        };
        entries.push(Entry {
            path: path.as_bytes().to_vec(),
            mode: mode.into(),
            kind: kind.into(),
            sha: object.into(),
            size,
        });
    }
    verify_tree(&mut entries, sha, limits.max_tree_object_bytes)?;
    checked_finished(provider, cancel)?;
    Ok(entries)
}
pub(super) fn raw_blob(
    provider: &GithubProvider,
    repo: &str,
    entry: &Entry,
    limits: &PrImmutableFileLimitsV1,
    cancel: &CancellationToken,
) -> GithubResult<Vec<u8>> {
    if entry
        .size
        .is_some_and(|size| size > limits.max_blob_bytes as u64)
    {
        return Err(safe(GithubErrorKind::Unsupported));
    }
    checked(
        provider,
        cancel,
        limits.max_blob_bytes,
        limits.max_parent_bytes,
    )?;
    // Exact existing remote_blob argv/spec/reader, narrowed before capture.
    let mut spec = provider.spec(vec![
        "api".into(),
        format!("repos/{repo}/git/blobs/{}", entry.sha),
        "--hostname".into(),
        provider.host.clone(),
        "--method".into(),
        "GET".into(),
        "--header".into(),
        "Accept: application/vnd.github.raw+json".into(),
    ]);
    spec.output_limit = spec.output_limit.min(limits.max_blob_bytes);
    let bytes = provider
        .raw_spec(spec, cancel)
        .map_err(|failure| safe(failure.kind))?;
    if bytes.len() > limits.max_blob_bytes
        || entry.size.is_some_and(|size| size != bytes.len() as u64)
    {
        return Err(safe(GithubErrorKind::Json));
    }
    crate::providers::gh::verify_blob_digest(&bytes, &entry.sha)
        .map_err(|failure| safe(failure.kind))?;
    checked_finished(provider, cancel)?;
    Ok(bytes)
}
pub(super) fn path_image(
    provider: &GithubProvider,
    repo: &str,
    root: &str,
    selection: (&[u8], bool),
    limits: &PrImmutableFileLimitsV1,
    cancel: &CancellationToken,
    state: &mut TreeReadState,
) -> GithubResult<PrImmutableImageV1> {
    let (path, expected_present) = selection;
    let components = path.split(|byte| *byte == b'/').collect::<Vec<_>>();
    if path.len() > 4096
        || components.is_empty()
        || components.len() > limits.max_path_depth
        || components.iter().any(|part| {
            part.is_empty()
                || *part == b"."
                || *part == b".."
                || part.contains(&0)
                || std::str::from_utf8(part).is_err()
        })
    {
        return Err(safe(GithubErrorKind::Unsupported));
    }
    let mut object = root.to_owned();
    for (index, component) in components.iter().enumerate() {
        let entries = tree(provider, repo, &object, limits, cancel, state)?;
        let entry = entries.into_iter().find(|entry| entry.path == *component);
        let Some(entry) = entry else {
            if expected_present {
                return Err(safe(GithubErrorKind::StaleHead));
            }
            return Ok(PrImmutableImageV1 {
                path: path.to_vec(),
                blob: None,
                mode: None,
                bytes: None,
                kind: None,
                absent: true,
            });
        };
        if index + 1 < components.len() {
            if entry.kind != "tree" {
                if !expected_present {
                    return Ok(PrImmutableImageV1 {
                        path: path.to_vec(),
                        blob: None,
                        mode: None,
                        bytes: None,
                        kind: None,
                        absent: true,
                    });
                }
                return Err(safe(GithubErrorKind::StaleHead));
            }
            object = entry.sha;
            continue;
        }
        if !expected_present {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        if entry.kind == "commit" {
            return Ok(PrImmutableImageV1 {
                path: path.to_vec(),
                blob: Some(entry.sha),
                mode: Some(entry.mode),
                bytes: None,
                kind: Some(PrImmutableImageKindV1::Submodule),
                absent: false,
            });
        }
        if entry.kind != "blob" {
            return Err(safe(GithubErrorKind::Unsupported));
        }
        let bytes = raw_blob(provider, repo, &entry, limits, cancel)?;
        let kind = if entry.mode == "120000" {
            PrImmutableImageKindV1::Symlink
        } else if bytes.starts_with(b"version https://git-lfs.github.com/spec/v1\n") {
            PrImmutableImageKindV1::LfsPointer
        } else if bytes.contains(&0) || std::str::from_utf8(&bytes).is_err() {
            PrImmutableImageKindV1::Binary
        } else {
            PrImmutableImageKindV1::Text
        };
        return Ok(PrImmutableImageV1 {
            path: path.to_vec(),
            blob: Some(entry.sha),
            mode: Some(entry.mode),
            bytes: Some(bytes),
            kind: Some(kind),
            absent: false,
        });
    }
    Err(safe(GithubErrorKind::Json))
}
