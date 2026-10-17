//! SOURCE-ONLY strict schema for actual scoped personal-queue transport.
//! These observations are NOT repository access or canonical PR-node authority.
//! Root owns the unapplied shared-loop/URL-forwarder activation and verification.
use super::*;
mod repository_metadata_v1;
pub(crate) use repository_metadata_v1::ExactInboxRepositoryMetadataV1;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum InboxReadPolicy {
    Legacy,
    ExactScoped,
}

#[derive(PartialEq, Eq)]
struct SearchIdentity {
    database_id: u64,
    node_id: String,
}
type RecordKey = (String, u64);

pub(super) struct ExactInboxPage<'a> {
    pub total: u64,
    pub items: &'a [Value],
}
pub(super) struct ExactInboxState {
    repository: String,
    first_total: Option<u64>,
    queue_keys: BTreeSet<RecordKey>,
    identities: BTreeMap<RecordKey, SearchIdentity>,
    numeric_keys: BTreeMap<u64, RecordKey>,
    node_keys: BTreeMap<String, RecordKey>,
}
impl ExactInboxState {
    /// Parser data only, restricted to the actual State complete-source
    /// request. Empty repositories mean that producer's real personal query,
    /// never an implicit fallback from a failed explicit selection.
    pub(super) fn state_source_v80(
        actual: &crate::state_inbox_sync_v76::StateInboxOriginalV76,
    ) -> GithubResult<Self> {
        if actual.repositories().is_empty() {
            validate_name(&actual.identity().login)?;
            Ok(Self {
                repository: String::new(),
                first_total: None,
                queue_keys: BTreeSet::new(),
                identities: BTreeMap::new(),
                numeric_keys: BTreeMap::new(),
                node_keys: BTreeMap::new(),
            })
        } else {
            Self::new(
                &actual.identity().host,
                actual.identity(),
                actual.repositories(),
            )
        }
    }
    pub(super) fn state_item_v80(
        &mut self,
        provider: &GithubProvider,
        raw: &Value,
        coverage: &mut Coverage,
        repository: Option<&str>,
    ) -> GithubResult<(PullRequest, bool)> {
        if raw.get("state").and_then(Value::as_str) != Some("open")
            || raw.get("draft").is_some_and(|v| !v.is_boolean())
        {
            return Err(schema(
                "Complete State queue lacks the actual open state/draft type",
            ));
        }
        // Derive only a parser consistency key from the actual canonical URL.
        // This is not a repository access proof or a PR-parent constructor.
        let name = match repository {
            Some(name) => name,
            None => {
                let url = required_text(raw, "repository_url")?;
                let prefix = if provider.host == "github.com" {
                    "https://api.github.com/repos/".to_owned()
                } else {
                    format!("https://{}/api/v3/repos/", provider.host)
                };
                url.strip_prefix(&prefix).ok_or_else(|| {
                    changed("Personal State queue repository URL is not its actual API host")
                })?
            }
        };
        validate_repo(name)?;
        if name.len() > 256 {
            return Err(schema(
                "State queue repository name exceeds its actual bounded field",
            ));
        }
        self.repository = name.to_owned();
        self.item(provider, raw, coverage)
    }
    pub fn new(host: &str, identity: &HostIdentity, scope: &[String]) -> GithubResult<Self> {
        if identity.host != host || identity.account_id.is_none_or(|id| id == 0) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Exact queue source needs the original positive numeric principal",
            ));
        }
        if scope.is_empty() || scope.len() > 64 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Exact queues require 1..64 explicit repositories; never personal all-scope fallback",
            ));
        }
        let mut names = BTreeSet::new();
        for name in scope {
            if name.len() > 256 {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Exact queue repository intent exceeds its bounded canonical field",
                ));
            }
            validate_repo(name)?;
            if !names.insert(name.to_ascii_lowercase()) {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Exact queue intent repeats the same canonical repository",
                ));
            }
        }
        Ok(Self {
            repository: String::new(),
            first_total: None,
            queue_keys: BTreeSet::new(),
            identities: BTreeMap::new(),
            numeric_keys: BTreeMap::new(),
            node_keys: BTreeMap::new(),
        })
    }
    pub fn begin_query(&mut self, repository: &str) {
        self.repository = repository.into();
        self.first_total = None;
        self.queue_keys.clear();
    }
    pub fn page<'a>(
        &mut self,
        raw: &'a Value,
        coverage: &mut Coverage,
    ) -> GithubResult<ExactInboxPage<'a>> {
        let total = raw.get("total_count").and_then(Value::as_u64).ok_or_else(|| {
            schema("Exact queue response omitted an actual unsigned total_count; unknown is not zero")
        })?;
        let incomplete = raw
            .get("incomplete_results")
            .and_then(Value::as_bool)
            .ok_or_else(|| {
                schema("Exact queue response omitted actual boolean incomplete_results coverage")
            })?;
        let items = raw
            .get("items")
            .and_then(Value::as_array)
            .ok_or_else(|| schema("Exact queue response has no actual items array"))?;
        if items.len() > 100 {
            return Err(schema(
                "Exact queue response exceeds its requested 100-row page before admission",
            ));
        }
        match self.first_total {
            None => {
                coverage.total_matches =
                    coverage.total_matches.checked_add(total).ok_or_else(|| {
                        schema("Exact reported queue totals overflow; no zero or wrapped coverage")
                    })?;
                self.first_total = Some(total);
            }
            Some(first) if first != total => partial(
                coverage,
                "Reported queue total changed during non-atomic traversal; absence is unproven",
            ),
            Some(_) => {}
        }
        if incomplete {
            partial(
                coverage,
                "GitHub reported incomplete queue search; observed rows are a subset",
            );
        }
        Ok(ExactInboxPage { total, items })
    }
    pub fn item(
        &mut self,
        provider: &GithubProvider,
        raw: &Value,
        coverage: &mut Coverage,
    ) -> GithubResult<(PullRequest, bool)> {
        let database_id = positive(raw, "id")?;
        let number = positive(raw, "number")?;
        let node_id = opaque(raw, "node_id")?;
        let repo_api = format!("repos/{}", self.repository);
        require_url(
            &provider.host,
            required_text(raw, "repository_url")?,
            &repo_api,
            true,
        )?;
        require_url(
            &provider.host,
            required_text(raw, "html_url")?,
            &format!("{}/pull/{number}", self.repository),
            false,
        )?;
        let linkage = raw
            .get("pull_request")
            .filter(|value| value.is_object())
            .ok_or_else(|| schema("Exact is:pr result lacks actual pull_request linkage"))?;
        require_url(
            &provider.host,
            required_text(linkage, "url")?,
            &format!("{repo_api}/pulls/{number}"),
            true,
        )?;
        optional_url(
            &provider.host,
            linkage,
            "html_url",
            &format!("{}/pull/{number}", self.repository),
            false,
        )?;
        optional_url(
            &provider.host,
            raw,
            "url",
            &format!("{repo_api}/issues/{number}"),
            true,
        )?;
        if let Some(repository) = raw.get("repository").filter(|value| !value.is_null()) {
            if !repository.is_object() {
                return Err(schema("Exact search repository metadata is malformed"));
            }
            if let Some(name) = repository.get("full_name").filter(|value| !value.is_null()) {
                if !name
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&self.repository))
                {
                    return Err(changed(
                        "Exact search repository metadata contradicts its requested canonical name",
                    ));
                }
            }
            if repository.get("id").is_some_and(|value| !value.is_null()) {
                positive(repository, "id")?;
            }
            if repository
                .get("node_id")
                .is_some_and(|value| !value.is_null())
            {
                opaque(repository, "node_id")?;
            }
            optional_url(&provider.host, repository, "url", &repo_api, true)?;
            optional_url(
                &provider.host,
                repository,
                "html_url",
                &self.repository,
                false,
            )?;
            // Optional repository IDs remain observed metadata. Exact same-ID
            // access is established only by the outer genuine repository reads.
        }
        let key = (self.repository.to_ascii_lowercase(), number);
        let observed = SearchIdentity {
            database_id,
            node_id: node_id.into(),
        };
        if self
            .identities
            .get(&key)
            .is_some_and(|prior| prior != &observed)
            || self
                .numeric_keys
                .get(&database_id)
                .is_some_and(|prior| prior != &key)
            || self
                .node_keys
                .get(node_id)
                .is_some_and(|prior| prior != &key)
        {
            return Err(changed("Exact search identities contradict the same repository/number or global observed record"));
        }
        if !self.identities.contains_key(&key) {
            self.numeric_keys.insert(database_id, key.clone());
            self.node_keys.insert(node_id.into(), key.clone());
            self.identities.insert(key.clone(), observed);
        }
        let fresh_queue_member = self.queue_keys.insert(key);
        if !fresh_queue_member {
            partial(
                coverage,
                "Repeated queue record does not count twice; non-atomic enumeration is partial",
            );
        }
        // The already validated exact query name is copied, not a repository
        // parsed by splitting an unchecked foreign-authority string.
        let pr = provider.pr_from_json(raw, Some(&self.repository))?;
        Ok((pr, fresh_queue_member))
    }
    pub fn check_loaded(&self, loaded: u64, current_total: u64, coverage: &mut Coverage) {
        if loaded > current_total {
            partial(
                coverage,
                "Unique observed queue records exceed its reported total; zero/absence is unproven",
            );
        }
    }
}
fn required_text<'a>(raw: &'a Value, field: &str) -> GithubResult<&'a str> {
    raw.get(field)
        .and_then(Value::as_str)
        .filter(|value| {
            !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
        })
        .ok_or_else(|| schema("Exact queue canonical string is missing, malformed or unbounded"))
}
fn positive(raw: &Value, field: &str) -> GithubResult<u64> {
    raw.get(field)
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
        .ok_or_else(|| schema("Exact queue record omitted an actual positive numeric identity"))
}
fn opaque<'a>(raw: &'a Value, field: &str) -> GithubResult<&'a str> {
    let node = required_text(raw, field)?;
    if node.len() > 1024 {
        return Err(schema(
            "Exact queue opaque node exceeds its bounded raw field",
        ));
    }
    Ok(node)
}
fn require_url(host: &str, url: &str, expected: &str, api: bool) -> GithubResult<()> {
    if !super::search::canonical_source_url_matches(host, url, expected, api, false) {
        return Err(changed(
            "Exact queue source URL contradicts its canonical host/repository/kind/number",
        ));
    }
    Ok(())
}
fn optional_url(
    host: &str,
    raw: &Value,
    field: &str,
    expected: &str,
    api: bool,
) -> GithubResult<()> {
    if raw.get(field).is_some_and(|value| !value.is_null()) {
        require_url(host, required_text(raw, field)?, expected, api)?;
    }
    Ok(())
}
fn partial(coverage: &mut Coverage, reason: &str) {
    coverage.complete = false;
    if !coverage.reasons.iter().any(|prior| prior == reason) {
        coverage.reasons.push(reason.into());
    }
}
fn schema(message: &'static str) -> GithubError {
    error(GithubErrorKind::Json, message)
}
fn changed(message: &'static str) -> GithubError {
    error(GithubErrorKind::IdentityChanged, message)
}
