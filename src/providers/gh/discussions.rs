//! Opt-in discussion hydration through gh, with category-specific enumeration evidence.
//! Native adapters still require current account/scope reservations and atomic publication.
use super::*;
use sha2::{Digest, Sha256};
use std::io::{self, Write};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HydrationLimits {
    pub max_documents: usize,
    pub max_document_bytes: usize,
    pub max_total_bytes: usize,
    pub max_pages: usize,
    pub page_size: usize,
    pub thread_comment_page_size: usize,
}
impl Default for HydrationLimits {
    fn default() -> Self {
        Self {
            max_documents: 5_000,
            max_document_bytes: 256 * 1024,
            max_total_bytes: 32 * 1024 * 1024,
            max_pages: 100,
            page_size: 50,
            thread_comment_page_size: 20,
        }
    }
}
impl HydrationLimits {
    fn validate(&self) -> GithubResult<()> {
        if self.max_documents == 0
            || self.max_documents > 10_000
            || self.max_document_bytes == 0
            || self.max_document_bytes > 4 * 1024 * 1024
            || self.max_total_bytes == 0
            || self.max_total_bytes > 32 * 1024 * 1024
            || self.max_pages == 0
            || self.max_pages > 100
            || !(1..=100).contains(&self.page_size)
            || !(1..=100).contains(&self.thread_comment_page_size)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Discussion hydration limits must be positive and within the provider document/page/32MiB budgets",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DiscussionCoverage {
    /// Only this flag can establish absence for the particular enumerated category.
    pub enumeration_complete: bool,
    /// False if any nested comments or bounded content remain unavailable.
    pub complete: bool,
    pub loaded: usize,
    pub pages: usize,
    /// Serialized source JSON bytes admitted, not a claimed network byte count.
    pub bytes: usize,
    pub total: Option<u64>,
    pub next_cursor: Option<String>,
    pub next_page: Option<u64>,
    /// Observed canonical nodes excluded by a cap must not authorize old cached text.
    pub excluded_nodes: Vec<String>,
    pub reasons: Vec<String>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscussionParentKind {
    PullRequest,
    Issue,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiscussionRecordKind {
    ConversationComment,
    Review,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscussionParent {
    pub host: String,
    pub repository_id: u64,
    pub repository: String,
    pub node_id: String,
    pub numeric_id: Option<u64>,
    pub number: u64,
    pub kind: DiscussionParentKind,
    pub title: String,
    pub body: String,
    pub body_complete: bool,
    pub state: String,
    pub author: Option<Actor>,
    /// None means the host omitted label state, not an observed empty list.
    pub labels: Option<Vec<String>>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub url: String,
    pub head_sha: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscussionRecord {
    pub node_id: String,
    pub numeric_id: Option<u64>,
    pub kind: DiscussionRecordKind,
    pub body: String,
    pub author: Option<Actor>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub submitted_at: Option<String>,
    pub url: String,
    pub state: Option<String>,
    /// Review provenance comes from actual commit_id, never the current PR head.
    pub commit_sha: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscussionSnapshot {
    pub identity: HostIdentity,
    pub parent: DiscussionParent,
    pub records: Vec<DiscussionRecord>,
    pub coverage: DiscussionCoverage,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DiscussionThreadsSnapshot {
    pub identity: HostIdentity,
    pub repository: Repository,
    pub parent: DiscussionParent,
    pub target: PrSummary,
    pub threads: Vec<ReviewThread>,
    pub coverage: DiscussionCoverage,
    /// Every group is keyed by its actual opaque thread node, not a row or path.
    pub comment_coverage: BTreeMap<String, DiscussionCoverage>,
}

struct SizeCounter(usize);
impl Write for SizeCounter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0 = self.0.saturating_add(bytes.len());
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn retained_size(value: &impl Serialize) -> GithubResult<usize> {
    let mut counter = SizeCounter(0);
    serde_json::to_writer(&mut counter, value)
        .map_err(|error| super::error(GithubErrorKind::Json, error.to_string()))?;
    Ok(counter.0)
}
fn required_string(value: &Value, field: &str) -> GithubResult<String> {
    value[field]
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                format!("Discussion response omitted required {field}"),
            )
        })
}
fn optional_string(value: &Value, field: &str) -> Option<String> {
    value[field]
        .as_str()
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}
fn discussion_body(value: &Value) -> GithubResult<String> {
    match value.get("body") {
        Some(Value::String(body)) => Ok(body.clone()),
        Some(Value::Null) => Ok(String::new()),
        _ => Err(error(
            GithubErrorKind::Json,
            "Discussion response omitted its body or returned an unknown body representation",
        )),
    }
}
fn optional_actor(value: &Value) -> Option<Actor> {
    (!value.is_null()).then(|| Actor::from_json(value))
}
fn positive_id(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| {
            value
                .as_str()
                .filter(|text| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit()))
                .and_then(|text| text.parse().ok())
        })
        .filter(|id| *id > 0)
}
/// No host-suffix matching, userinfo, alternate scheme or foreign parent is accepted.
pub(super) fn exact_url_path(url: &str, host: &str, expected: &str, api: bool) -> bool {
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
    let expected_host = if api && host.eq_ignore_ascii_case("github.com") {
        "api.github.com".to_owned()
    } else if api && residency_cloud {
        format!("api.{host}")
    } else {
        host.to_owned()
    };
    if !authority.eq_ignore_ascii_case(&expected_host) || path.contains('?') {
        return false;
    }
    let path = path.split('#').next().unwrap_or_default();
    let path = if api && !host.eq_ignore_ascii_case("github.com") && !residency_cloud {
        path.strip_prefix("api/v3/").unwrap_or(path)
    } else {
        path
    };
    path.eq_ignore_ascii_case(expected.trim_start_matches('/'))
}

fn check_source_url(
    value: &Value,
    field: &str,
    host: &str,
    path: &str,
    api: bool,
) -> GithubResult<String> {
    let url = required_string(value, field)?;
    if !exact_url_path(&url, host, path, api) {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            format!("Discussion {field} identifies another host or canonical parent"),
        ));
    }
    Ok(url)
}
struct FingerprintWriter(Sha256);
impl Write for FingerprintWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.update(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn parent_fingerprint(parent: &DiscussionParent) -> GithubResult<[u8; 32]> {
    let mut writer = FingerprintWriter(Sha256::new());
    serde_json::to_writer(&mut writer, parent)
        .map_err(|error| super::error(GithubErrorKind::Json, error.to_string()))?;
    Ok(writer.0.finalize().into())
}
fn bounded_parent(
    mut parent: DiscussionParent,
    limits: &HydrationLimits,
) -> GithubResult<DiscussionParent> {
    let size = retained_size(&parent)?;
    if size > limits.max_document_bytes || size > limits.max_total_bytes {
        parent.body = String::new();
        parent.body_complete = false;
        let size = retained_size(&parent)?;
        if size > limits.max_document_bytes || size > limits.max_total_bytes {
            return Err(error(
                GithubErrorKind::Transport,
                "Even discussion identity metadata exceeds the selected hydration budget",
            ));
        }
    }
    Ok(parent)
}
fn required_node(value: &Value, field: &str) -> GithubResult<String> {
    value[field]
        .as_str()
        .filter(|node| {
            !node.is_empty()
                && node.len() <= 512
                && !node
                    .chars()
                    .any(|character| character.is_control() || character.is_whitespace())
        })
        .map(str::to_owned)
        .ok_or_else(|| {
            error(
                GithubErrorKind::Json,
                "Discussion response omitted a bounded opaque node identity",
            )
        })
}
fn record_identity(
    value: &Value,
    parent: &DiscussionParent,
    kind: DiscussionRecordKind,
) -> GithubResult<(String, u64, String)> {
    let node = required_node(value, "node_id")?;
    if node == parent.node_id {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "A discussion record cannot reuse its canonical parent node",
        ));
    }
    let id = positive_id(&value["id"]).ok_or_else(|| {
        error(
            GithubErrorKind::Json,
            "Discussion record has no positive numeric ID",
        )
    })?;
    let (field, api_kind) = if kind == DiscussionRecordKind::Review {
        ("pull_request_url", "pulls")
    } else {
        ("issue_url", "issues")
    };
    check_source_url(
        value,
        field,
        &parent.host,
        &format!("repos/{}/{api_kind}/{}", parent.repository, parent.number),
        true,
    )?;
    let html_kind = if parent.kind == DiscussionParentKind::PullRequest {
        "pull"
    } else {
        "issues"
    };
    let url = check_source_url(
        value,
        "html_url",
        &parent.host,
        &format!("{}/{html_kind}/{}", parent.repository, parent.number),
        false,
    )?;
    Ok((node, id, url))
}
fn thread_identity(
    node: &Value,
    target: &PrSummary,
    repository: &Repository,
) -> GithubResult<String> {
    let id = required_node(node, "id")?;
    if id == target.pr.id {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "A review thread cannot reuse its parent PR node",
        ));
    }
    if node["pullRequest"]["id"] != target.pr.id
        || !node["repository"]["nameWithOwner"]
            .as_str()
            .is_some_and(|name| name.eq_ignore_ascii_case(&repository.full_name))
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "A thread identifies a foreign canonical PR/repository",
        ));
    }
    Ok(id)
}

/// Rich comment provenance is separate from mutation APIs and their compatibility queries.
pub const DISCUSSION_THREADS_QUERY: &str = "query($id:ID!,$cursor:String,$first:Int!,$commentsFirst:Int!){node(id:$id){__typename ... on PullRequest{id number headRefOid baseRefOid updatedAt repository{nameWithOwner} reviewThreads(first:$first,after:$cursor){totalCount nodes{id path diffSide line startLine isResolved isOutdated viewerCanResolve viewerCanUnresolve pullRequest{id} repository{nameWithOwner} comments(first:$commentsFirst){totalCount nodes{id fullDatabaseId body createdAt updatedAt url author{__typename login avatarUrl ... on User{name}} commit{oid} pullRequest{id}} pageInfo{hasNextPage endCursor}}} pageInfo{hasNextPage endCursor}}}}}";

fn required_bool(value: &Value, field: &str) -> GithubResult<bool> {
    value[field].as_bool().ok_or_else(|| {
        error(
            GithubErrorKind::Unsupported,
            format!("Discussion host omitted required {field} coverage/state"),
        )
    })
}
fn nullable_line(value: &Value, field: &str) -> GithubResult<Option<u64>> {
    match value.get(field) {
        Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .filter(|line| *line > 0)
            .map(Some)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Thread line is not a positive number or explicit null",
                )
            }),
        None => Err(error(
            GithubErrorKind::Unsupported,
            format!("Host omitted thread {field}"),
        )),
    }
}
fn connection_coverage(value: &Value, loaded: usize) -> GithubResult<DiscussionCoverage> {
    let has_next = required_bool(&value["pageInfo"], "hasNextPage")?;
    let total = value["totalCount"].as_u64().ok_or_else(|| {
        error(
            GithubErrorKind::Unsupported,
            "Discussion host omitted a valid connection totalCount",
        )
    })?;
    if total < loaded as u64 || !has_next && total != loaded as u64 {
        return Err(error(
            GithubErrorKind::Json,
            "Discussion connection count contradicts its reported page coverage",
        ));
    }
    let cursor = optional_string(&value["pageInfo"], "endCursor");
    if has_next && (cursor.is_none() || total <= loaded as u64) {
        return Err(error(
            GithubErrorKind::Json,
            "Discussion connection claims further records without advancing cursor/count evidence",
        ));
    }
    Ok(DiscussionCoverage {
        enumeration_complete: !has_next,
        complete: !has_next,
        loaded,
        pages: 1,
        total: Some(total),
        next_cursor: if has_next { cursor } else { None },
        reasons: if has_next {
            vec!["Only the bounded first comment page of this exact thread is loaded; unseen inner records are not absent.".into()]
        } else {
            vec![]
        },
        ..DiscussionCoverage::default()
    })
}

impl GithubProvider {
    fn hydration_scope(
        &self,
        identity: &HostIdentity,
        repository: &Repository,
        limits: &HydrationLimits,
    ) -> GithubResult<()> {
        limits.validate()?;
        validate_repo(&repository.full_name)?;
        if identity.host != self.host
            || identity.login.is_empty()
            || identity.account_id.is_none_or(|id| id == 0)
            || repository.host != self.host
            || repository.id == 0
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Hydration requires a verified positive account and canonical positive repository identity on this host",
            ));
        }
        Ok(())
    }
    fn hydration_identity(
        &self,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let fresh = self.identity(cancel)?;
        if !identity.same_account(&fresh) || fresh.account_id.is_none_or(|id| id == 0) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "GitHub account changed while hydrating private discussion sources; do not publish this snapshot",
            ));
        }
        Ok(())
    }
    fn hydration_repository(
        &self,
        expected: &Repository,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let fresh = self.repository(&expected.full_name, cancel)?;
        if fresh.id != expected.id || !fresh.full_name.eq_ignore_ascii_case(&expected.full_name) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Repository was renamed or recreated while hydrating discussions; reload its canonical identity",
            ));
        }
        Ok(())
    }
    fn discussion_parent(
        &self,
        repository: &Repository,
        number: u64,
        kind: DiscussionParentKind,
        cancel: &CancellationToken,
    ) -> GithubResult<(DiscussionParent, Value)> {
        validate_pr(&repository.full_name, number)?;
        let endpoint_kind = if kind == DiscussionParentKind::PullRequest {
            "pulls"
        } else {
            "issues"
        };
        let path = format!("repos/{}/{endpoint_kind}/{number}", repository.full_name);
        let value = self.api(&path, "GET", None, None, false, cancel)?;
        if positive_id(&value["number"]) != Some(number) || positive_id(&value["id"]).is_none() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Discussion parent does not have the requested positive number/identity",
            ));
        }
        if kind == DiscussionParentKind::Issue
            && value
                .get("pull_request")
                .is_some_and(|value| !value.is_null())
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Issues API returned a pull request; select its explicit PR category instead",
            ));
        }
        if kind == DiscussionParentKind::PullRequest {
            if positive_id(&value["base"]["repo"]["id"]) != Some(repository.id)
                || !value["base"]["repo"]["full_name"]
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&repository.full_name))
            {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "PR base repository no longer matches the selected canonical repository ID/name",
                ));
            }
            validate_sha(&required_string(&value["head"], "sha")?)?;
            validate_sha(&required_string(&value["base"], "sha")?)?;
        } else {
            check_source_url(
                &value,
                "repository_url",
                &self.host,
                &format!("repos/{}", repository.full_name),
                true,
            )?;
        }
        check_source_url(&value, "url", &self.host, &path, true)?;
        let html_kind = if kind == DiscussionParentKind::PullRequest {
            "pull"
        } else {
            "issues"
        };
        let url = check_source_url(
            &value,
            "html_url",
            &self.host,
            &format!("{}/{html_kind}/{number}", repository.full_name),
            false,
        )?;
        let parent = DiscussionParent {
            host: self.host.clone(),
            repository_id: repository.id,
            repository: repository.full_name.clone(),
            node_id: required_node(&value, "node_id")?,
            numeric_id: positive_id(&value["id"]),
            number,
            kind,
            title: required_string(&value, "title")?,
            body: discussion_body(&value)?,
            body_complete: true,
            state: required_string(&value, "state")?,
            author: optional_actor(&value["user"]),
            labels: value
                .get("labels")
                .filter(|value| !value.is_null())
                .map(|labels| {
                    labels
                        .as_array()
                        .ok_or_else(|| {
                            error(
                                GithubErrorKind::Json,
                                "Discussion parent labels are not an array",
                            )
                        })?
                        .iter()
                        .map(|label| {
                            label
                                .as_str()
                                .or_else(|| label["name"].as_str())
                                .map(str::to_owned)
                                .ok_or_else(|| {
                                    error(
                                        GithubErrorKind::Json,
                                        "Discussion label omitted its actual name",
                                    )
                                })
                        })
                        .collect::<GithubResult<Vec<_>>>()
                })
                .transpose()?,
            created_at: optional_string(&value, "created_at"),
            updated_at: optional_string(&value, "updated_at"),
            url,
            head_sha: (kind == DiscussionParentKind::PullRequest)
                .then(|| text(&value["head"], "sha")),
        };
        Ok((parent, value))
    }
    fn exact_pr_parent(
        &self,
        repository: &Repository,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> GithubResult<DiscussionParent> {
        if target.pr.host != self.host
            || !target
                .pr
                .repository
                .eq_ignore_ascii_case(&repository.full_name)
            || target.pr.id.is_empty()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Select an exact PR node on this host/repository before hydration",
            ));
        }
        let (parent, value) = self.discussion_parent(
            repository,
            target.pr.number,
            DiscussionParentKind::PullRequest,
            cancel,
        )?;
        if parent.node_id != target.pr.id
            || parent.head_sha.as_ref() != Some(&target.head_sha)
            || value["base"]["sha"] != target.base_sha
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "PR identity/base/head changed before hydration; refresh the selected target",
            ));
        }
        Ok(parent)
    }

    fn discussion_records(
        &self,
        parent: &DiscussionParent,
        kind: DiscussionRecordKind,
        limits: &HydrationLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<(Vec<DiscussionRecord>, DiscussionCoverage)> {
        let parent_bytes = retained_size(parent)?;
        if parent_bytes > limits.max_document_bytes || parent_bytes > limits.max_total_bytes {
            return Err(error(
                GithubErrorKind::Transport,
                "The selected discussion body exceeds the hydration document/total budget; no truncated body is presented as complete",
            ));
        }
        let endpoint = match kind {
            DiscussionRecordKind::ConversationComment => format!(
                "repos/{}/issues/{}/comments",
                parent.repository, parent.number
            ),
            DiscussionRecordKind::Review => format!(
                "repos/{}/pulls/{}/reviews",
                parent.repository, parent.number
            ),
        };
        let mut coverage = DiscussionCoverage {
            bytes: parent_bytes,
            next_page: Some(1),
            ..DiscussionCoverage::default()
        };
        if !parent.body_complete {
            coverage.excluded_nodes.push(parent.node_id.clone());
            coverage.reasons.push("The observed parent body exceeded the document budget and is not admitted; its old cached text must not authorize matches.".into());
        }
        let mut records = Vec::new();
        let mut nodes = BTreeSet::new();
        let mut numbers = BTreeSet::new();
        for page in 1..=limits.max_pages {
            let values = self.api(
                &format!("{endpoint}?per_page={}&page={page}", limits.page_size),
                "GET",
                None,
                None,
                false,
                cancel,
            )?;
            coverage.pages += 1;
            let values = values
                .as_array()
                .ok_or_else(|| error(GithubErrorKind::Json, "Discussion page is not an array"))?;
            if values.len() > limits.page_size {
                return Err(error(
                    GithubErrorKind::Json,
                    "Discussion page exceeded the explicitly requested page size",
                ));
            }
            for (index, value) in values.iter().enumerate() {
                let (node_id, numeric_id, url) = record_identity(value, parent, kind)?;
                let bytes = retained_size(value)?;
                if records.len() >= limits.max_documents
                    || bytes > limits.max_document_bytes
                    || coverage.bytes.saturating_add(bytes) > limits.max_total_bytes
                {
                    for excluded in &values[index..] {
                        let (node, id, _) = record_identity(excluded, parent, kind)?;
                        if !nodes.insert(node.clone()) || !numbers.insert(id) {
                            return Err(error(
                                GithubErrorKind::Json,
                                "Excluded discussion nodes repeat an admitted or excluded identity",
                            ));
                        }
                        coverage.excluded_nodes.push(node);
                    }
                    coverage.next_page = Some(page as u64);
                    coverage.reasons.push("Document or JSON-byte budget reached inside a page; this category is not authoritatively enumerated. Resume must reread that page, not skip its unseen records.".into());
                    return Ok((records, coverage));
                }
                if !nodes.insert(node_id.clone()) || !numbers.insert(numeric_id) {
                    return Err(error(
                        GithubErrorKind::Json,
                        "Discussion paging repeated a node/numeric identity; no authoritative enumeration is inferred",
                    ));
                }
                let commit_sha = if kind == DiscussionRecordKind::Review {
                    optional_string(value, "commit_id")
                } else {
                    None
                };
                if let Some(commit) = &commit_sha {
                    validate_sha(commit)?;
                }
                records.push(DiscussionRecord {
                    node_id,
                    numeric_id: Some(numeric_id),
                    kind,
                    body: discussion_body(value)?,
                    author: optional_actor(&value["user"]),
                    created_at: optional_string(value, "created_at"),
                    updated_at: optional_string(value, "updated_at"),
                    submitted_at: optional_string(value, "submitted_at"),
                    url,
                    state: optional_string(value, "state"),
                    commit_sha,
                });
                coverage.loaded = records.len();
                coverage.bytes = coverage.bytes.saturating_add(bytes);
            }
            if values.len() < limits.page_size {
                coverage.enumeration_complete = true;
                coverage.complete = parent.body_complete;
                coverage.next_page = None;
                coverage.total = Some(records.len() as u64);
                return Ok((records, coverage));
            }
            coverage.next_page = Some(page as u64 + 1);
        }
        coverage.reasons.push("Explicit REST page budget reached. A full last page does not prove the absence of further discussion records.".into());
        Ok((records, coverage))
    }

    pub fn pr_discussion_comments(
        &self,
        identity: &HostIdentity,
        repository: &Repository,
        target: &PrSummary,
        limits: &HydrationLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<DiscussionSnapshot> {
        self.pr_discussion_records(
            identity,
            repository,
            target,
            DiscussionRecordKind::ConversationComment,
            limits,
            cancel,
        )
    }

    pub fn pr_discussion_reviews(
        &self,
        identity: &HostIdentity,
        repository: &Repository,
        target: &PrSummary,
        limits: &HydrationLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<DiscussionSnapshot> {
        self.pr_discussion_records(
            identity,
            repository,
            target,
            DiscussionRecordKind::Review,
            limits,
            cancel,
        )
    }

    fn pr_discussion_records(
        &self,
        identity: &HostIdentity,
        repository: &Repository,
        target: &PrSummary,
        kind: DiscussionRecordKind,
        limits: &HydrationLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<DiscussionSnapshot> {
        self.hydration_scope(identity, repository, limits)?;
        self.hydration_identity(identity, cancel)?;
        let parent = self.exact_pr_parent(repository, target, cancel)?;
        let fingerprint = parent_fingerprint(&parent)?;
        let parent = bounded_parent(parent, limits)?;
        let (records, coverage) = self.discussion_records(&parent, kind, limits, cancel)?;
        let fresh = self.exact_pr_parent(repository, target, cancel)?;
        if fingerprint != parent_fingerprint(&fresh)? {
            return Err(error(
                GithubErrorKind::StaleHead,
                "PR parent metadata changed while hydrating its discussion category; do not publish mixed generations",
            ));
        }
        self.hydration_identity(identity, cancel)?;
        Ok(DiscussionSnapshot {
            identity: identity.clone(),
            parent,
            records,
            coverage,
        })
    }

    pub fn issue_details(
        &self,
        identity: &HostIdentity,
        repository: &Repository,
        expected: &DiscussionParent,
        limits: &HydrationLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<DiscussionSnapshot> {
        self.hydration_scope(identity, repository, limits)?;
        let number = expected.number;
        validate_pr(&repository.full_name, number)?;
        if expected.kind != DiscussionParentKind::Issue
            || expected.host != self.host
            || expected.repository_id != repository.id
            || !expected
                .repository
                .eq_ignore_ascii_case(&repository.full_name)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Issue hydration requires the preflight canonical issue parent used for the reservation",
            ));
        }
        self.hydration_identity(identity, cancel)?;
        self.hydration_repository(repository, cancel)?;
        let (parent, _) =
            self.discussion_parent(repository, number, DiscussionParentKind::Issue, cancel)?;
        if parent.node_id != expected.node_id || parent.numeric_id != expected.numeric_id {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Issue was replaced after hydration reservation; do not publish into the old parent source",
            ));
        }
        let fingerprint = parent_fingerprint(&parent)?;
        let parent = bounded_parent(parent, limits)?;
        let (records, coverage) = self.discussion_records(
            &parent,
            DiscussionRecordKind::ConversationComment,
            limits,
            cancel,
        )?;
        let (fresh, _) =
            self.discussion_parent(repository, number, DiscussionParentKind::Issue, cancel)?;
        if fingerprint != parent_fingerprint(&fresh)? {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Issue parent changed while hydrating comments; do not publish mixed or reassigned identities",
            ));
        }
        self.hydration_repository(repository, cancel)?;
        self.hydration_identity(identity, cancel)?;
        Ok(DiscussionSnapshot {
            identity: identity.clone(),
            parent,
            records,
            coverage,
        })
    }

    pub fn issue_parent(
        &self,
        identity: &HostIdentity,
        repository: &Repository,
        number: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<DiscussionParent> {
        let limits = HydrationLimits::default();
        self.hydration_scope(identity, repository, &limits)?;
        validate_pr(&repository.full_name, number)?;
        self.hydration_identity(identity, cancel)?;
        self.hydration_repository(repository, cancel)?;
        let (parent, _) =
            self.discussion_parent(repository, number, DiscussionParentKind::Issue, cancel)?;
        self.hydration_repository(repository, cancel)?;
        self.hydration_identity(identity, cancel)?;
        bounded_parent(parent, &limits)
    }

    pub fn discussion_threads(
        &self,
        identity: &HostIdentity,
        repository: &Repository,
        target: &PrSummary,
        limits: &HydrationLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<DiscussionThreadsSnapshot> {
        self.hydration_scope(identity, repository, limits)?;
        self.hydration_identity(identity, cancel)?;
        let parent = self.exact_pr_parent(repository, target, cancel)?;
        let fingerprint = parent_fingerprint(&parent)?;
        let parent = bounded_parent(parent, limits)?;
        let parent_bytes = retained_size(&parent)?;
        if parent_bytes > limits.max_document_bytes || parent_bytes > limits.max_total_bytes {
            return Err(error(
                GithubErrorKind::Transport,
                "PR discussion body exceeds the bounded hydration budget",
            ));
        }
        let mut coverage = DiscussionCoverage {
            bytes: parent_bytes,
            ..DiscussionCoverage::default()
        };
        if !parent.body_complete {
            coverage.excluded_nodes.push(parent.node_id.clone());
            coverage.reasons.push("The current parent body exceeds its budget; old parent text is excluded from this snapshot.".into());
        }
        let mut threads = Vec::new();
        let mut comment_coverage = BTreeMap::new();
        let mut thread_ids = BTreeSet::new();
        let mut comment_ids = BTreeSet::new();
        let mut cursor: Option<String> = None;
        let mut seen_cursors = BTreeSet::new();
        let mut documents = 0usize;
        'pages: for _ in 0..limits.max_pages {
            let value = self.api("graphql", "POST", Some(json!({"query":DISCUSSION_THREADS_QUERY,"variables":{"id":target.pr.id,"cursor":cursor,"first":limits.page_size,"commentsFirst":limits.thread_comment_page_size}})), None, false, cancel)?;
            coverage.pages += 1;
            let node = &value["data"]["node"];
            if node["__typename"] != "PullRequest"
                || node["id"] != target.pr.id
                || node["number"].as_u64() != Some(target.pr.number)
                || node["headRefOid"] != target.head_sha
                || node["baseRefOid"] != target.base_sha
                || !node["repository"]["nameWithOwner"]
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&repository.full_name))
                || parent
                    .updated_at
                    .as_ref()
                    .is_some_and(|timestamp| node["updatedAt"].as_str() != Some(timestamp.as_str()))
            {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    "GraphQL thread page belongs to another PR/repository/head/base or changed parent generation",
                ));
            }
            let collection = &node["reviewThreads"];
            let nodes = collection["nodes"].as_array().ok_or_else(|| {
                error(
                    GithubErrorKind::Unsupported,
                    "Host does not expose bounded review-thread nodes",
                )
            })?;
            if nodes.len() > limits.page_size {
                return Err(error(
                    GithubErrorKind::Json,
                    "Thread page exceeded the requested page size",
                ));
            }
            let total = collection["totalCount"].as_u64().ok_or_else(|| {
                error(
                    GithubErrorKind::Unsupported,
                    "Host omitted review-thread totalCount",
                )
            })?;
            if coverage.total.is_some_and(|previous| previous != total) {
                return Err(error(
                    GithubErrorKind::StaleHead,
                    "Thread inventory count changed across pages; do not publish mixed enumeration",
                ));
            }
            coverage.total = Some(total);
            for (index, node) in nodes.iter().enumerate() {
                let id = thread_identity(node, target, repository)?;
                let path = required_string(node, "path")?;
                if path.starts_with('/')
                    || path.contains('\0')
                    || path.split('/').any(|part| part == "..")
                {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Thread has no safe canonical repository-relative path",
                    ));
                }
                let side = required_string(node, "diffSide")?;
                if !matches!(side.as_str(), "LEFT" | "RIGHT") {
                    return Err(error(
                        GithubErrorKind::Unsupported,
                        "Host returned an unknown thread diff-side enum",
                    ));
                }
                let comments = node["comments"]["nodes"].as_array().ok_or_else(|| {
                    error(
                        GithubErrorKind::Unsupported,
                        "Host omitted bounded thread comment nodes",
                    )
                })?;
                if comments.len() > limits.thread_comment_page_size {
                    return Err(error(
                        GithubErrorKind::Json,
                        "Inner thread page exceeded the requested comment cap",
                    ));
                }
                let source_bytes = retained_size(node)?;
                let metadata_bytes = source_bytes.saturating_sub(retained_size(comments)?);
                let comment_bytes = comments
                    .iter()
                    .map(retained_size)
                    .collect::<GithubResult<Vec<_>>>()?;
                if documents.saturating_add(1 + comments.len()) > limits.max_documents
                    || metadata_bytes > limits.max_document_bytes
                    || coverage.bytes.saturating_add(source_bytes) > limits.max_total_bytes
                    || comment_bytes
                        .iter()
                        .any(|bytes| *bytes > limits.max_document_bytes)
                {
                    for excluded in &nodes[index..] {
                        let node = thread_identity(excluded, target, repository)?;
                        if !thread_ids.insert(node.clone()) {
                            return Err(error(
                                GithubErrorKind::Json,
                                "Excluded thread repeats an admitted or excluded node",
                            ));
                        }
                        coverage.excluded_nodes.push(node);
                    }
                    coverage.next_cursor = cursor.clone();
                    coverage.reasons.push("Thread/document/JSON budget reached inside an outer page. Unseen threads and inner comments are not absent; this page must be reread rather than skipped.".into());
                    break 'pages;
                }
                if !thread_ids.insert(id.clone()) {
                    return Err(error(
                        GithubErrorKind::Json,
                        "Thread node repeated across pages",
                    ));
                }
                for comment in comments {
                    let comment_id = required_node(comment, "id")?;
                    if !comment_ids.insert(comment_id)
                        || comment["pullRequest"]["id"] != target.pr.id
                    {
                        return Err(error(
                            GithubErrorKind::InvalidTarget,
                            "Inline comment is duplicated or identifies another PR node",
                        ));
                    }
                    let url = required_string(comment, "url")?;
                    if !exact_url_path(
                        &url,
                        &self.host,
                        &format!("{}/pull/{}", repository.full_name, target.pr.number),
                        false,
                    ) {
                        return Err(error(
                            GithubErrorKind::InvalidTarget,
                            "Inline comment URL belongs to another host/PR",
                        ));
                    }
                    discussion_body(comment)?;
                    // BigInt may exceed u64; an opaque node remains sufficient for read-only search.
                    // Do not manufacture a truncated REST numeric identity from fullDatabaseId.
                    if let Some(commit) = comment["commit"]["oid"].as_str() {
                        validate_sha(commit)?;
                    }
                }
                let mut inner = connection_coverage(&node["comments"], comments.len())?;
                inner.bytes = retained_size(comments)?;
                let line = nullable_line(node, "line")?;
                let start_line = nullable_line(node, "startLine")?;
                let thread = ReviewThread {
                    id: id.clone(),
                    path,
                    side,
                    line,
                    start_line,
                    resolved: required_bool(node, "isResolved")?,
                    outdated: required_bool(node, "isOutdated")?,
                    viewer_can_resolve: required_bool(node, "viewerCanResolve")?,
                    viewer_can_unresolve: required_bool(node, "viewerCanUnresolve")?,
                    comments: comments.clone(),
                    comments_complete: inner.complete,
                };
                documents += 1 + comments.len();
                coverage.bytes += source_bytes;
                threads.push(thread);
                comment_coverage.insert(id, inner);
                coverage.loaded = threads.len();
            }
            let has_next = required_bool(&collection["pageInfo"], "hasNextPage")?;
            if total < threads.len() as u64 || !has_next && total != threads.len() as u64 {
                return Err(error(
                    GithubErrorKind::Json,
                    "Outer thread count contradicts reported page coverage",
                ));
            }
            if !has_next {
                coverage.enumeration_complete = true;
                coverage.complete =
                    parent.body_complete && comment_coverage.values().all(|inner| inner.complete);
                coverage.next_cursor = None;
                if !coverage.complete {
                    coverage.reasons.push("All outer thread nodes are observed, but one or more bounded inner comment pages remain incomplete.".into());
                }
                break;
            }
            let next = required_string(&collection["pageInfo"], "endCursor")?;
            if Some(&next) == cursor.as_ref()
                || !seen_cursors.insert(next.clone())
                || nodes.is_empty()
                || total <= threads.len() as u64
            {
                return Err(error(
                    GithubErrorKind::Json,
                    "Outer thread pagination cursor/count did not advance",
                ));
            }
            cursor = Some(next);
            coverage.next_cursor = cursor.clone();
        }
        if !coverage.enumeration_complete && coverage.reasons.is_empty() {
            coverage.reasons.push(
                "Explicit GraphQL outer page budget reached; unseen threads are not absent.".into(),
            );
        }
        let fresh = self.exact_pr_parent(repository, target, cancel)?;
        if fingerprint != parent_fingerprint(&fresh)? {
            return Err(error(
                GithubErrorKind::StaleHead,
                "PR parent changed while hydrating thread pages; no mixed snapshot is published",
            ));
        }
        self.hydration_identity(identity, cancel)?;
        Ok(DiscussionThreadsSnapshot {
            identity: identity.clone(),
            repository: repository.clone(),
            parent,
            target: target.clone(),
            threads,
            coverage,
            comment_coverage,
        })
    }
}

#[cfg(test)]
mod url_contracts {
    use super::*;
    #[test]
    fn collaboration_index_provider_urls_use_exact_official_cloud_and_server_origins() {
        for (host, url) in [
            (
                "github.com",
                "https://api.github.com/repos/example/repo/issues/8",
            ),
            (
                "octocorp.ghe.com",
                "https://api.octocorp.ghe.com/repos/example/repo/issues/8",
            ),
            (
                "git.company.example",
                "https://git.company.example/api/v3/repos/example/repo/issues/8",
            ),
        ] {
            assert!(exact_url_path(
                url,
                host,
                "repos/example/repo/issues/8",
                true
            ));
        }
        for (host, url) in [
            (
                "github.com",
                "https://api.github.com/api/v3/repos/example/repo/issues/8",
            ),
            (
                "octocorp.ghe.com",
                "https://api.octocorp.ghe.com/api/v3/repos/example/repo/issues/8",
            ),
            (
                "octocorp.ghe.com",
                "https://api.other.ghe.com/repos/example/repo/issues/8",
            ),
            (
                "github.com",
                "https://api.github.com.evil.example/repos/example/repo/issues/8",
            ),
            (
                "github.com",
                "https://api.github.com@evil.example/repos/example/repo/issues/8",
            ),
            (
                "github.com",
                "https://api.github.com/repos/example/repo/issues/8?redirect=other",
            ),
        ] {
            assert!(!exact_url_path(
                url,
                host,
                "repos/example/repo/issues/8",
                true
            ));
        }
    }
}
