//! Additive worker-only bounded discovery, staged until the shared activation gate.
//! Authentication stays in gh; the owning worker supplies fresh pre/post authority.
use super::{identity::*, *};
use std::time::Instant;

pub const REMOTE_SEARCH_MAX_RESPONSE_BYTES: usize = 16 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct RemoteSearchLimits {
    pub selected_kinds: Vec<RemoteSearchKind>,
    pub max_pages: usize,
    pub max_results: usize,
    /// Aggregate response stdout bytes. Each child is capped before launch;
    /// the runner additionally counts stderr against that child's output cap.
    pub max_response_bytes: usize,
    pub max_duration: Duration,
}
impl Default for RemoteSearchLimits {
    fn default() -> Self {
        Self {
            selected_kinds: vec![
                RemoteSearchKind::Code,
                RemoteSearchKind::PullRequests,
                RemoteSearchKind::Issues,
                RemoteSearchKind::Repositories,
            ],
            max_pages: 10,
            max_results: 1000,
            max_response_bytes: REMOTE_SEARCH_MAX_RESPONSE_BYTES,
            max_duration: Duration::from_secs(30),
        }
    }
}
impl RemoteSearchLimits {
    pub fn validate(&self, kind: RemoteSearchKind) -> GithubResult<()> {
        if !(1..=10).contains(&self.max_pages)
            || !(1..=1000).contains(&self.max_results)
            || !(1..=REMOTE_SEARCH_MAX_RESPONSE_BYTES).contains(&self.max_response_bytes)
            || self.max_duration.is_zero()
            || self.max_duration > Duration::from_secs(300)
            || self.selected_kinds.is_empty()
            || self.selected_kinds.len() > 4
            || self
                .selected_kinds
                .iter()
                .enumerate()
                .any(|(index, kind)| self.selected_kinds[..index].contains(kind))
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Choose 1..10 pages, 1..1000 results, 1..16 MiB response bytes, 1 ns..300 s and distinct explicit remote source kinds"));
        }
        if !self.selected_kinds.contains(&kind) {
            return Err(error(
                GithubErrorKind::Unsupported,
                "This remote source was not explicitly selected; no request was made",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub enum RemoteSearchStatus {
    Complete,
    Partial {
        failure: Option<GithubError>,
        cancelled: bool,
    },
    Unavailable {
        failure: GithubError,
        cancelled: bool,
    },
}
#[derive(Clone, Debug)]
pub struct BoundedRemoteSearchResult {
    pub result: RemoteSearchResult,
    /// Strictly position-aligned with result.items; never keyed by legacy id.
    pub provenance: Vec<Option<RemoteSearchProvenance>>,
    pub original_query: Query,
    pub requested_repositories: Vec<String>,
    pub limits: RemoteSearchLimits,
    pub requested_pages: usize,
    pub fetched_pages: usize,
    pub fetched_rows: usize,
    /// Bytes of fully received stdout responses. An interrupted/capped child
    /// is reported by status/requested_pages, never as a complete zero-byte page.
    pub response_bytes: usize,
    /// None is unknown, not a confirmed empty remote source.
    pub reported_total: Option<u64>,
    pub status: RemoteSearchStatus,
}

fn bounded_query(
    provider: &GithubProvider,
    identity: &HostIdentity,
    query: &Query,
    kind: RemoteSearchKind,
    repositories: &[String],
) -> GithubResult<(String, BTreeSet<String>, &'static str)> {
    if identity.host != provider.host || identity.account_id.is_none_or(|id| id == 0) {
        return Err(error(GithubErrorKind::IdentityChanged,"Bounded discovery requires the selected host and positive numeric current account; login alone is not authority"));
    }
    validate_name(&identity.login)?;
    if query.mode != SearchMode::GitHub {
        return Err(error(GithubErrorKind::Unsupported,"Remote discovery uses explicit GitHub mode; Ranked/Exact/Regex are not silently translated"));
    }
    if repositories.len() > 1000 {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Remote discovery scope exceeds1000 explicitly selected repositories",
        ));
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
            return Err(error(
                GithubErrorKind::Unsupported,
                "Explicit result types exclude this remote source; no weakened request was sent",
            ));
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
        .map_err(|failure| error(GithubErrorKind::Unsupported, failure.to_string()))?;
    if query.text.chars().count() > 256 {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "GitHub remote search permits at most256 non-qualifier characters",
        ));
    }
    if kind == RemoteSearchKind::Code && query.text.trim().is_empty() {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Legacy remote code search requires a search term",
        ));
    }
    if kind == RemoteSearchKind::Repositories && !repositories.is_empty() {
        return Err(error(GithubErrorKind::Unsupported,"Repository discovery is separately organization-scoped, not a selected-repo blend source"));
    }
    let scopes = repositories
        .iter()
        .map(|repo| {
            validate_repo(repo)?;
            Ok(repo.to_ascii_lowercase())
        })
        .collect::<GithubResult<BTreeSet<_>>>()?;
    if matches!(
        kind,
        RemoteSearchKind::Code | RemoteSearchKind::PullRequests | RemoteSearchKind::Issues
    ) && scopes.is_empty()
    {
        return Err(error(GithubErrorKind::InvalidTarget,"Choose explicit repositories before bounded code/collaboration discovery; no global scope is inferred"));
    }
    if !scopes.is_empty() {
        if query
            .filters
            .get("repo")
            .into_iter()
            .flatten()
            .any(|repo| !scopes.contains(&repo.to_ascii_lowercase()))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Query repository is outside the explicitly selected remote scope",
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
    Ok((q.trim().into(), scopes, endpoint))
}
/// Existing canonical origin policy factored for exact queue schema as well.
/// ASCII folding applies only to host/owner/repository and fixed route segments.
/// The Code suffix is never folded/rewritten into a blob, commit or filename ID.
pub(super) fn canonical_source_url_matches(
    host: &str,
    url: &str,
    expected_path: &str,
    api: bool,
    allow_suffix: bool,
) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let Some((authority, path)) = rest.split_once('/') else {
        return false;
    };
    let lower_host = host.to_ascii_lowercase();
    let residency_cloud = lower_host
        .strip_suffix(".ghe.com")
        .is_some_and(|tenant| !tenant.is_empty() && !tenant.contains('.'));
    let github_cloud = lower_host == "github.com";
    let api_host = if github_cloud {
        "api.github.com".into()
    } else if residency_cloud {
        format!("api.{host}")
    } else {
        host.into()
    };
    if !authority.eq_ignore_ascii_case(if api { &api_host } else { host })
        || path.contains('?')
        || path.contains('#')
    {
        return false;
    }
    let path = if api && !github_cloud && !residency_cloud {
        path.strip_prefix("api/v3/").unwrap_or(path)
    } else {
        path
    };
    if allow_suffix {
        path.get(..expected_path.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(expected_path))
    } else {
        path.eq_ignore_ascii_case(expected_path)
    }
}
fn validate_source_urls(host: &str, raw: &Value, item: &RemoteSearchItem) -> GithubResult<()> {
    if let Some(name) = raw
        .get("repository")
        .and_then(|repository| repository.get("full_name"))
        .filter(|value| !value.is_null())
    {
        if !name
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(&item.repository))
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Remote repository metadata contradicts its actual canonical repository name",
            ));
        }
    }
    for (field, api) in [("repository_url", true), ("html_url", false)] {
        let Some(url) = raw.get(field).filter(|value| !value.is_null()) else {
            continue;
        };
        let url = url.as_str().ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Remote canonical source URL is malformed",
            )
        })?;
        let (expected, allow_suffix) = if api {
            (format!("repos/{}", item.repository), false)
        } else {
            match item.kind {
                RemoteSearchKind::PullRequests => (
                    format!("{}/pull/{}", item.repository, item.number.unwrap_or(0)),
                    false,
                ),
                RemoteSearchKind::Issues => (
                    format!("{}/issues/{}", item.repository, item.number.unwrap_or(0)),
                    false,
                ),
                RemoteSearchKind::Repositories => (item.repository.clone(), false),
                RemoteSearchKind::Code => (format!("{}/blob/", item.repository), true),
            }
        };
        if !canonical_source_url_matches(host, url, &expected, api, allow_suffix) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Remote source URL has a different canonical host or ambiguous destination",
            ));
        }
    }
    Ok(())
}
fn conflicting_identity(a: &RemoteCanonicalIdentity, b: &RemoteCanonicalIdentity) -> bool {
    let conflicts = |a: Option<u64>, b: Option<u64>| a.zip(b).is_some_and(|(a, b)| a != b);
    match (a, b) {
        (
            RemoteCanonicalIdentity::OpaqueNode {
                node_id: a,
                database_id: ad,
                repository_id: ar,
            },
            RemoteCanonicalIdentity::OpaqueNode {
                node_id: b,
                database_id: bd,
                repository_id: br,
            },
        ) => a != b || conflicts(*ad, *bd) || conflicts(*ar, *br),
        (
            RemoteCanonicalIdentity::Numeric {
                database_id: a,
                repository_id: ar,
            },
            RemoteCanonicalIdentity::Numeric {
                database_id: b,
                repository_id: br,
            },
        ) => a != b || conflicts(*ar, *br),
        (
            RemoteCanonicalIdentity::CodeBlob {
                path: a,
                blob_sha: as_,
                repository_id: ar,
            },
            RemoteCanonicalIdentity::CodeBlob {
                path: b,
                blob_sha: bs,
                repository_id: br,
            },
        ) => a != b || as_ != bs || conflicts(*ar, *br),
        _ => true,
    }
}
impl GithubProvider {
    /// One selected source, independently bounded. Root must retain fresh
    /// pre/post principal and discovered-parent visibility/journal authority
    /// before admitting or opening private candidates; this adds no auth calls.
    pub fn remote_search_bounded(
        &self,
        identity: &HostIdentity,
        query: &Query,
        kind: RemoteSearchKind,
        repositories: &[String],
        limits: &RemoteSearchLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<BoundedRemoteSearchResult> {
        limits.validate(kind)?;
        let (q, scopes, endpoint) = bounded_query(self, identity, query, kind, repositories)?;
        let started = Instant::now();
        let page_size = limits.max_results.min(100); // Fixed width preserves REST page offsets.
        let mut items: Vec<RemoteSearchItem> = vec![];
        let mut provenance = vec![];
        let mut seen: BTreeMap<(String, String), usize> = BTreeMap::new();
        let mut canonical = vec![];
        let mut total = None;
        let mut reasons = vec![];
        let mut requested_pages = 0;
        let mut fetched_pages = 0;
        let mut fetched_rows = 0usize;
        let mut response_bytes = 0usize;
        let mut failure = None;
        let mut cancelled = false;
        for page in 1..=limits.max_pages {
            if cancel.is_cancelled() {
                cancelled = true;
                failure = Some(error(
                    GithubErrorKind::Transport,
                    "Selected remote discovery cancelled; no later page requested",
                ));
                break;
            }
            if fetched_rows.saturating_add(page_size) > limits.max_results {
                reasons.push(format!("Selected result budget {} cannot fit another fixed {page_size}-row page; stopped before requesting it",limits.max_results));
                break;
            }
            let remaining_bytes = limits.max_response_bytes.saturating_sub(response_bytes);
            let remaining_time = limits.max_duration.saturating_sub(started.elapsed());
            if remaining_bytes == 0 {
                reasons.push(
                    "Selected aggregate response byte budget reached before another request".into(),
                );
                break;
            }
            if remaining_time.is_zero() {
                reasons.push(
                    "Selected remote source time budget reached before another request".into(),
                );
                break;
            }
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
                format!("q={q}"),
                "-f".into(),
                format!("per_page={page_size}"),
                "-f".into(),
                format!("page={page}"),
            ];
            let mut spec = self.spec(args);
            spec.output_limit = remaining_bytes.min(4 * 1024 * 1024);
            spec.timeout = self.timeout.min(remaining_time);
            requested_pages += 1;
            let bytes = match self.runner.run(spec, cancel) {
                Ok(output) => output.stdout,
                Err(error_) => {
                    cancelled = error_.kind == FailureKind::Cancelled;
                    let error_ = if error_.kind == FailureKind::OutputLimit {
                        error(GithubErrorKind::Unsupported,"Selected response byte budget exceeded; partial output was not parsed or retained")
                    } else {
                        GithubError::from(error_)
                    };
                    if matches!(
                        error_.kind,
                        GithubErrorKind::Authentication
                            | GithubErrorKind::IdentityChanged
                            | GithubErrorKind::Permission
                            | GithubErrorKind::NotFound
                    ) {
                        // These failures cannot authorize already fetched private
                        // candidates. Do not retain a page across access loss or
                        // pretend that search proves which repository was revoked.
                        return Err(error_);
                    }
                    failure = Some(error_);
                    break;
                }
            };
            response_bytes = response_bytes
                .checked_add(bytes.len())
                .ok_or_else(|| error(GithubErrorKind::Json, "Remote byte count overflow"))?;
            if response_bytes > limits.max_response_bytes {
                return Err(error(
                    GithubErrorKind::Json,
                    "Runner response violated the pre-request aggregate byte budget",
                ));
            }
            let value: Value = serde_json::from_slice(&bytes)
                .map_err(|error_| error(GithubErrorKind::Json, error_.to_string()))?;
            let count = value["total_count"].as_u64().ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Remote response lacks its actual total_count",
                )
            })?;
            let incomplete = value["incomplete_results"].as_bool().ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Remote response lacks explicit incomplete_results coverage",
                )
            })?;
            let rows = value["items"].as_array().ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Remote response has no actual items array",
                )
            })?;
            if count < rows.len() as u64 {
                return Err(error(GithubErrorKind::Json,"Remote response count contradicts its actual page; complete empty coverage cannot be inferred"));
            }
            if rows.len() > page_size
                || rows.len() > limits.max_results.saturating_sub(fetched_rows)
            {
                return Err(error(
                    GithubErrorKind::Json,
                    "Remote response returned more rows than the requested page/result budget",
                ));
            }
            fetched_pages += 1;
            fetched_rows += rows.len();
            if total.is_none() {
                total = Some(count);
            } else if total != Some(count) {
                reasons.push(
                    "Matching count changed while paging; no stable complete snapshot is claimed"
                        .into(),
                );
            }
            if incomplete {
                reasons.push(
                    "GitHub returned incomplete_results; remote discovery is a subset".into(),
                );
            }
            for row in rows {
                let item = parse_search_item(&self.host, row, kind)?;
                if !scopes.is_empty() && !scopes.contains(&item.repository.to_ascii_lowercase()) {
                    return Err(error(
                        GithubErrorKind::IdentityChanged,
                        "Remote response contradicts the selected repository scope",
                    ));
                }
                validate_source_urls(&self.host, row, &item)?;
                let raw_identity = raw_canonical_identity(row, &item)?;
                let key = (
                    item.repository.to_ascii_lowercase(),
                    canonical_dedup_key(&raw_identity),
                );
                if let Some(index) = seen.get(&key) {
                    if conflicting_identity(&canonical[*index], &raw_identity)
                        || items[*index].number != item.number
                    {
                        return Err(error(GithubErrorKind::IdentityChanged,"Repeated remote canonical identity contradicts its actual database/repository ID or destination"));
                    }
                    if serde_json::to_vec(&items[*index]).ok() != serde_json::to_vec(&item).ok() {
                        reasons.push("Repeated canonical result changed while paging; retained first observation is not a complete snapshot".into());
                    }
                    continue;
                }
                let proof = search_provenance(identity, &item, raw_identity.clone())?;
                if proof.is_none() {
                    return Err(error(GithubErrorKind::IdentityChanged,"Positive remote provenance was unavailable; private candidate not admitted"));
                }
                seen.insert(key, items.len());
                canonical.push(raw_identity);
                provenance.push(proof);
                items.push(item);
            }
            if rows.len() < page_size || items.len() as u64 >= count.min(1000) {
                break;
            }
        }
        if total.is_some_and(|total| total > 1000) {
            reasons.push("GitHub remote search caps each query at 1,000 matches; no hidden repository completeness is inferred".into());
        }
        if let Some(total) = total {
            if (items.len() as u64) < total {
                reasons.push(format!(
                    "Loaded {} of {total} reported matches within selected {}-page/{}-row budgets",
                    items.len(),
                    limits.max_pages,
                    limits.max_results
                ));
            }
        }
        if let Some(error_) = &failure {
            reasons.push(error_.message.clone());
        }
        reasons.sort();
        reasons.dedup();
        let complete = failure.is_none() && total.is_some() && reasons.is_empty();
        let status = if complete {
            RemoteSearchStatus::Complete
        } else if fetched_pages > 0 {
            RemoteSearchStatus::Partial { failure, cancelled }
        } else {
            RemoteSearchStatus::Unavailable {
                failure: failure.unwrap_or_else(|| {
                    error(
                        GithubErrorKind::Transport,
                        "Selected remote source budget/cancellation prevented an observed response",
                    )
                }),
                cancelled,
            }
        };
        let provider_notes=vec![
            "Provider best-match order is not a local relevance score; inaccessible repositories are not enumerated by search".into(),
            "Text-match offsets belong to server fragments, not exact full-file lines or comment identity. Code SHA is a blob, not a commit.".into(),
            if kind==RemoteSearchKind::Code {
                "Legacy code discovery searches GitHub-indexed default-branch files below its size limit, not regex or every saved revision. Exact opening still requires immutable commit/blob verification.".into()
            } else {
                "PR/issue discovery may match comment text but identifies only the parent. Exact comment/thread destinations require independently current hydrated discussion data.".into()
            },
        ];
        Ok(BoundedRemoteSearchResult {
            result: RemoteSearchResult {
                identity: identity.clone(),
                query: q,
                coverage: Coverage {
                    loaded: items.len(),
                    total_matches: total.unwrap_or(0),
                    complete,
                    reasons,
                },
                items,
                provider_notes,
            },
            provenance,
            original_query: query.clone(),
            requested_repositories: repositories.to_vec(),
            limits: limits.clone(),
            requested_pages,
            fetched_pages,
            fetched_rows,
            response_bytes,
            reported_total: total,
            status,
        })
    }
}
