//! Remote discovery explicitly uses legacy GitHub API semantics, not local ranking/regex.
use super::pr::url_component;
use super::*;
use crate::search::{Query, RemoteKind, SearchMode};
use base64::{engine::general_purpose::STANDARD, Engine};
use sha1::Sha1;
use sha2::{Digest, Sha256};
mod identity;
pub use identity::{RemoteCanonicalIdentity, RemoteSearchProvenance};
mod bounded;
/// Pure source-coordinate validation, not authentication or repository admission.
/// Callers pass only repository/fixed resource paths; never SHA/file/node aliases.
pub(super) fn canonical_source_url_matches(
    host: &str,
    url: &str,
    expected_path: &str,
    api: bool,
    allow_suffix: bool,
) -> bool {
    bounded::canonical_source_url_matches(host, url, expected_path, api, allow_suffix)
}
pub use bounded::{
    BoundedRemoteSearchResult, RemoteSearchLimits, RemoteSearchStatus,
    REMOTE_SEARCH_MAX_RESPONSE_BYTES,
};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum RemoteSearchKind {
    Code,
    PullRequests,
    Issues,
    Repositories,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteTextMatch {
    pub fragment: String,
    pub property: String,
    pub object_type: String,
    pub object_url: String,
    /// These offsets are relative to the server fragment, never a full-file line/column.
    pub server_match_indices: Vec<[u64; 2]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteSearchItem {
    pub host: String,
    pub repository: String,
    pub kind: RemoteSearchKind,
    pub id: String,
    pub number: Option<u64>,
    pub title: String,
    pub body: String,
    pub url: String,
    pub state: String,
    pub author: Option<Actor>,
    pub repository_owner: Option<Actor>,
    pub path: Option<String>,
    /// REST code search returns a blob SHA; it is not a commit SHA.
    pub blob_sha: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub text_matches: Vec<RemoteTextMatch>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RemoteSearchResult {
    pub identity: HostIdentity,
    pub query: String,
    pub items: Vec<RemoteSearchItem>,
    pub coverage: Coverage,
    pub provider_notes: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResolvedRemoteFile {
    pub host: String,
    pub repository: String,
    pub path: String,
    pub commit_sha: String,
    pub blob_sha: String,
}
/// Exact immutable file content, not a search-engine snippet or mutable default-ref body.
#[derive(Clone, Debug)]
pub struct ResolvedRemoteContent {
    pub file: ResolvedRemoteFile,
    pub bytes: Vec<u8>,
}
pub const REMOTE_CONTENT_MAX_BYTES: usize = 2 * 1024 * 1024;

/// Verify Git's object address over the canonical blob header and actual payload bytes.
/// SHA-1 is only used for legacy Git addressing, not for a new authentication protocol.
pub fn verify_blob_digest(bytes: &[u8], blob_sha: &str) -> GithubResult<()> {
    validate_sha(blob_sha)?;
    let header = format!("blob {}\0", bytes.len());
    let actual = if blob_sha.len() == 40 {
        let mut hasher = Sha1::new();
        hasher.update(header.as_bytes());
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    } else {
        let mut hasher = Sha256::new();
        hasher.update(header.as_bytes());
        hasher.update(bytes);
        format!("{:x}", hasher.finalize())
    };
    if !actual.eq_ignore_ascii_case(blob_sha) {
        return Err(error(GithubErrorKind::StaleHead,"Remote content bytes do not match the selected Git blob address; no unverified content is shown"));
    }
    Ok(())
}
fn repository_from_search(v: &Value, kind: RemoteSearchKind) -> GithubResult<String> {
    let repo = match kind {
        RemoteSearchKind::Code => text(&v["repository"], "full_name"),
        RemoteSearchKind::Repositories => text(v, "full_name"),
        _ => text(v, "repository_url")
            .split_once("/repos/")
            .map(|(_, repo)| repo.to_owned())
            .unwrap_or_default(),
    };
    validate_repo(&repo)?;
    Ok(repo)
}
fn parse_search_item(
    host: &str,
    v: &Value,
    kind: RemoteSearchKind,
) -> GithubResult<RemoteSearchItem> {
    let repository = repository_from_search(v, kind)?;
    let code = kind == RemoteSearchKind::Code;
    let item_number = if matches!(
        kind,
        RemoteSearchKind::PullRequests | RemoteSearchKind::Issues
    ) {
        let n = number(v, "number");
        if n == 0 {
            return Err(error(
                GithubErrorKind::Json,
                "Remote issue/PR result has no canonical number",
            ));
        }
        Some(n)
    } else {
        None
    };
    if matches!(kind, RemoteSearchKind::PullRequests) && v["pull_request"].is_null()
        || matches!(kind, RemoteSearchKind::Issues) && !v["pull_request"].is_null()
    {
        return Err(error(
            GithubErrorKind::Json,
            "Remote search returned the wrong issue/PR resource type",
        ));
    }
    let path = if code {
        let path = text(v, "path");
        if path.is_empty()
            || path.starts_with('/')
            || path.contains('\0')
            || path.split('/').any(|part| part == "..")
        {
            return Err(error(
                GithubErrorKind::Json,
                "Code result has no safe canonical path",
            ));
        }
        Some(path)
    } else {
        None
    };
    let blob_sha = if code {
        let sha = text(v, "sha");
        validate_sha(&sha)?;
        Some(sha)
    } else {
        None
    };
    let id = if code {
        format!(
            "{}:{}",
            path.as_deref().unwrap_or_default(),
            blob_sha.as_deref().unwrap_or_default()
        )
    } else if !text(v, "node_id").is_empty() {
        text(v, "node_id")
    } else {
        let id = number(v, "id");
        if id == 0 {
            return Err(error(
                GithubErrorKind::Json,
                "Remote result has no stable identity",
            ));
        }
        id.to_string()
    };
    let text_matches = v["text_matches"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|fragment| RemoteTextMatch {
            fragment: text(fragment, "fragment"),
            property: text(fragment, "property"),
            object_type: text(fragment, "object_type"),
            object_url: text(fragment, "object_url"),
            server_match_indices: fragment["matches"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| {
                    let indices = v["indices"].as_array()?;
                    if indices.len() != 2 {
                        return None;
                    }
                    Some([indices[0].as_u64()?, indices[1].as_u64()?])
                })
                .collect(),
        })
        .collect();
    Ok(RemoteSearchItem {
        host: host.into(),
        repository,
        kind,
        id,
        number: item_number,
        title: if code {
            text(v, "name")
        } else {
            let title = text(v, "title");
            if title.is_empty() {
                text(v, "full_name")
            } else {
                title
            }
        },
        body: if kind == RemoteSearchKind::Repositories {
            text(v, "description")
        } else {
            text(v, "body")
        },
        url: text(v, "html_url"),
        state: text(v, "state"),
        author: if matches!(
            kind,
            RemoteSearchKind::PullRequests | RemoteSearchKind::Issues
        ) {
            Some(Actor::from_json(&v["user"]))
        } else {
            None
        },
        repository_owner: if code {
            Some(Actor::from_json(&v["repository"]["owner"]))
        } else if kind == RemoteSearchKind::Repositories {
            Some(Actor::from_json(&v["owner"]))
        } else {
            None
        },
        path,
        blob_sha,
        created_at: text(v, "created_at"),
        updated_at: text(v, "updated_at"),
        text_matches,
    })
}
impl GithubProvider {
    pub fn remote_search(
        &self,
        identity: &HostIdentity,
        query: &Query,
        kind: RemoteSearchKind,
        repositories: &[String],
        cancel: &CancellationToken,
    ) -> GithubResult<RemoteSearchResult> {
        if identity.host != self.host {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Search identity belongs to a different host",
            ));
        }
        validate_name(&identity.login)?;
        if query.mode != SearchMode::GitHub {
            return Err(error(GithubErrorKind::Unsupported,"Remote discovery uses explicit GitHub mode; local Ranked/Exact/Regex semantics are not silently weakened"));
        }
        let mut translated = query.clone();
        if let Some(types) = translated.filters.remove("type") {
            let expected = match kind {
                RemoteSearchKind::Code => "file",
                RemoteSearchKind::PullRequests => "pr",
                RemoteSearchKind::Issues => "issue",
                RemoteSearchKind::Repositories => "repository",
            };
            if types.iter().any(|value| value != expected) {
                return Err(error(GithubErrorKind::Unsupported,"The result-type filter does not match this remote source; comment/commit/run hydration uses a different corpus"));
            }
        }
        let mapped = match kind {
            RemoteSearchKind::Code => RemoteKind::Code,
            RemoteSearchKind::PullRequests => RemoteKind::PullRequests,
            RemoteSearchKind::Issues => RemoteKind::Issues,
            RemoteSearchKind::Repositories => RemoteKind::Repositories,
        };
        let mut q = translated
            .github_query(mapped)
            .map_err(|e| error(GithubErrorKind::Unsupported, e.to_string()))?;
        if query.text.chars().count() > 256 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "GitHub remote search permits at most 256 non-qualifier characters",
            ));
        }
        if kind == RemoteSearchKind::Code && query.text.trim().is_empty() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Legacy remote code search requires a search term, not only qualifiers",
            ));
        }
        if kind == RemoteSearchKind::Repositories && !repositories.is_empty() {
            return Err(error(GithubErrorKind::Unsupported,"Repository discovery is scoped by organization; saved repository sets scope code and collaboration sources"));
        }
        let scopes = repositories
            .iter()
            .map(|repo| {
                validate_repo(repo)?;
                Ok(repo.to_lowercase())
            })
            .collect::<GithubResult<BTreeSet<_>>>()?;
        if !scopes.is_empty() {
            if query
                .filters
                .get("repo")
                .into_iter()
                .flatten()
                .any(|repo| !scopes.contains(&repo.to_lowercase()))
            {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Query repository is outside the selected repository set",
                ));
            }
            if !translated.filters.contains_key("repo") {
                for repo in &scopes {
                    q.push_str(&format!(" repo:{repo}"));
                }
            }
        }
        match kind {
            RemoteSearchKind::PullRequests => q.push_str(" is:pr"),
            RemoteSearchKind::Issues => q.push_str(" is:issue"),
            _ => {}
        }
        let endpoint = match kind {
            RemoteSearchKind::Code => "search/code",
            RemoteSearchKind::Repositories => "search/repositories",
            _ => "search/issues",
        };
        let mut items = Vec::new();
        let mut seen = BTreeSet::new();
        let mut total = None;
        let mut reasons = Vec::new();
        let mut budget = 0usize;
        for page in 1..=10 {
            let args = vec![
                "api".into(),
                endpoint.into(),
                "--hostname".into(),
                self.host.clone(),
                "--method".into(),
                "GET".into(),
                "--header".into(),
                "Accept: application/vnd.github.text-match+json".into(),
                "-f".into(),
                format!("q={}", q.trim()),
                "-f".into(),
                "per_page=100".into(),
                "-f".into(),
                format!("page={page}"),
            ];
            let bytes = self.execute(args, None, None, cancel)?;
            budget = budget.saturating_add(bytes.len());
            if budget > 16 * 1024 * 1024 {
                return Err(error(
                    GithubErrorKind::Unsupported,
                    "Remote search exceeded the total response budget; narrow the query",
                ));
            }
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?;
            let count = value["total_count"].as_u64().ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Remote search response has no total_count",
                )
            })?;
            if total.is_none() {
                total = Some(count);
            } else if total != Some(count) {
                reasons.push("Matching count changed while paging; refresh to reconcile moving remote results".into());
            }
            if boolean(&value, "incomplete_results") {
                reasons
                    .push("GitHub returned incomplete_results; this is a discovery subset".into());
            }
            let rows = value["items"].as_array().ok_or_else(|| {
                error(GithubErrorKind::Json, "Remote search response has no items")
            })?;
            for row in rows {
                let item = parse_search_item(&self.host, row, kind)?;
                if !scopes.is_empty() && !scopes.contains(&item.repository.to_lowercase()) {
                    return Err(error(
                        GithubErrorKind::Json,
                        "Remote search returned a repository outside the requested scope",
                    ));
                }
                if seen.insert((item.repository.to_lowercase(), item.id.clone())) {
                    items.push(item);
                }
            }
            if rows.len() < 100 || items.len() as u64 >= count.min(1000) {
                break;
            }
        }
        let total = total.unwrap_or(0);
        if total > 1000 {
            reasons.push("GitHub remote search caps each query at 1,000 matches; narrow its repository/date filters".into());
        }
        if (items.len() as u64) < total {
            reasons.push(format!(
                "Loaded {} of {total} reported matches",
                items.len()
            ));
        }
        reasons.sort();
        reasons.dedup();
        Ok(RemoteSearchResult{identity:identity.clone(),query:q.trim().into(),coverage:Coverage{loaded:items.len(),total_matches:total,complete:reasons.is_empty(),reasons},items,provider_notes:vec!["GitHub's provider-specific best-match order is not directly comparable to local ranked scores; invisible/inaccessible repositories are not enumerated by search".into(),if kind==RemoteSearchKind::Code{"Legacy REST code search covers indexed default-branch files smaller than 384 KiB, not regex or every historical revision. Blob matches require exact-commit verification before file navigation.".into()}else{"Remote issue/PR matching can include comment text, but result identity remains the parent item; exact comment destinations require hydrated discussion data.".into()}]})
    }
    pub fn resolve_remote_code(
        &self,
        item: &RemoteSearchItem,
        cancel: &CancellationToken,
    ) -> GithubResult<ResolvedRemoteFile> {
        self.resolve_remote_entry(item, cancel)
            .map(|(file, _)| file)
    }
    /// Performs the same three gh reads as resolution, reusing the verified Contents
    /// response. Authenticated content never uses the public-avatar image transport.
    pub fn resolve_remote_content(
        &self,
        item: &RemoteSearchItem,
        max_bytes: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<ResolvedRemoteContent> {
        if max_bytes == 0 || max_bytes > REMOTE_CONTENT_MAX_BYTES {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Choose a positive remote content budget no larger than 2 MiB",
            ));
        }
        let (file, content) = self.resolve_remote_entry(item, cancel)?;
        let size = content.get("size").and_then(Value::as_u64).ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Remote file did not report its exact byte size",
            )
        })?;
        if size > max_bytes as u64 {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Remote file exceeds the selected content byte budget",
            ));
        }
        if content["encoding"].as_str() != Some("base64") {
            return Err(error(GithubErrorKind::Unsupported,"This host did not return bounded base64 Contents data; no download URL or mutable body is substituted"));
        }
        let encoded = content["content"].as_str().ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Remote Contents response omitted encoded file bytes",
            )
        })?;
        let encoded_bytes = (size as usize).div_ceil(3) * 4;
        if encoded.len() > encoded_bytes.saturating_mul(2).saturating_add(2) {
            return Err(error(
                GithubErrorKind::Json,
                "Encoded Contents body exceeds its declared file-size budget",
            ));
        }
        if cancel.is_cancelled() {
            return Err(error(
                GithubErrorKind::Transport,
                "Remote content resolution cancelled before decoding",
            ));
        }
        // GitHub wraps base64 with line breaks. Ignore only CR/LF, never arbitrary
        // untrusted bytes; the encoded/decoded limits are checked before allocation.
        let encoded = encoded
            .bytes()
            .filter(|byte| !matches!(byte, b'\n' | b'\r'))
            .collect::<Vec<_>>();
        if encoded.len() != encoded_bytes {
            return Err(error(
                GithubErrorKind::Json,
                "Encoded Contents length does not match its declared file size",
            ));
        }
        let bytes = STANDARD.decode(encoded).map_err(|_| {
            error(
                GithubErrorKind::Json,
                "Remote Contents body is not canonical base64",
            )
        })?;
        if bytes.len() != size as usize || bytes.len() > max_bytes {
            return Err(error(
                GithubErrorKind::Json,
                "Decoded Contents bytes do not match their declared size/budget",
            ));
        }
        verify_blob_digest(&bytes, &file.blob_sha)?;
        if cancel.is_cancelled() {
            return Err(error(
                GithubErrorKind::Transport,
                "Remote content resolution cancelled before publication",
            ));
        }
        Ok(ResolvedRemoteContent { file, bytes })
    }
    fn resolve_remote_entry(
        &self,
        item: &RemoteSearchItem,
        cancel: &CancellationToken,
    ) -> GithubResult<(ResolvedRemoteFile, Value)> {
        if item.host != self.host || item.kind != RemoteSearchKind::Code {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select a code hit on the current host",
            ));
        }
        let path = item.path.as_ref().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Code hit has no canonical path",
            )
        })?;
        let blob = item.blob_sha.as_ref().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Code hit has no blob identity",
            )
        })?;
        if path.is_empty()
            || path.starts_with('/')
            || path.split('/').any(|part| matches!(part, "" | "." | ".."))
            || path.contains('\0')
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select an exact canonical remote file path",
            ));
        }
        validate_repo(&item.repository)?;
        validate_sha(blob)?;
        let repo = self.repository(&item.repository, cancel)?;
        if !repo.full_name.eq_ignore_ascii_case(&item.repository) {
            return Err(error(GithubErrorKind::StaleHead,"Repository identity changed since discovery; refresh search instead of following a rename or redirect silently"));
        }
        if repo.default_branch.is_empty() {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Repository has no default branch to resolve the indexed blob",
            ));
        }
        let commit = self.api(
            &format!(
                "repos/{}/commits/{}",
                item.repository,
                url_component(&repo.default_branch)
            ),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        let sha = text(&commit, "sha");
        validate_sha(&sha)?;
        let encoded = path
            .split('/')
            .map(url_component)
            .collect::<Vec<_>>()
            .join("/");
        let content = self.api(
            &format!("repos/{}/contents/{encoded}?ref={sha}", item.repository),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if text(&content, "type") != "file"
            || text(&content, "path") != *path
            || !text(&content, "sha").eq_ignore_ascii_case(blob)
        {
            return Err(error(GithubErrorKind::StaleHead,"Indexed code blob is not present at the current resolved default-branch commit; refresh the remote search rather than showing a different saved version"));
        }
        Ok((
            ResolvedRemoteFile {
                host: self.host.clone(),
                repository: item.repository.clone(),
                path: path.clone(),
                commit_sha: sha,
                blob_sha: blob.clone(),
            },
            content,
        ))
    }
}
