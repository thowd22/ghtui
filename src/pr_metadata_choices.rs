//! Prepared metadata catalogue. This model performs no I/O and is
//! not publication authority. Its worker caller must bracket the actual eligible
//! endpoint/context reads with fresh positive-principal and original-PR checks.
//! A bare `metadata_choices()` Vec does not prove pagination or mutation rights.
use crate::{
    identity::ActorIdentity,
    pr_write_journal::PrWriteTarget,
    providers::{
        gh::{Actor, HostIdentity, MetadataChoices, PrContext, PrSummary},
        runner::sanitize_terminal,
    },
    search::model::valid_sha,
    theme::SemanticToken,
};
use serde_json::Value;
use std::{collections::BTreeSet, io};

#[derive(Clone, Copy, Debug)]
pub struct PrMetadataLimits {
    pub max_rows: usize,
    pub max_raw_bytes: usize,
    pub max_prepared_bytes: usize,
    pub max_field_bytes: usize,
}
impl Default for PrMetadataLimits {
    fn default() -> Self {
        Self {
            max_rows: 1000,
            max_raw_bytes: 1024 * 1024,
            max_prepared_bytes: 1024 * 1024,
            max_field_bytes: 4096,
        }
    }
}
impl PrMetadataLimits {
    pub(crate) fn validate(self) -> Result<(), String> {
        if !(1..=1000).contains(&self.max_rows)
            || !(1..=2 * 1024 * 1024).contains(&self.max_raw_bytes)
            || !(1..=2 * 1024 * 1024).contains(&self.max_prepared_bytes)
            || !(1..=4096).contains(&self.max_field_bytes)
        {
            return Err("Invalid bounded metadata preparation limits".into());
        }
        Ok(())
    }
}

/// Supplied by the read worker, never inferred from row count or an empty Vec.
#[derive(Clone, Copy, Debug)]
pub enum PrMetadataCoverage<'a> {
    Complete {
        fetched_pages: usize,
    },
    Partial {
        fetched_pages: usize,
        reason: &'a str,
    },
    Unavailable {
        reason: &'a str,
    },
}
#[derive(Clone, Copy, Debug)]
pub struct PrMetadataCatalogue<'a> {
    pub rows: &'a [Value],
    pub coverage: PrMetadataCoverage<'a>,
}
#[derive(Clone, Copy, Debug)]
pub struct PrMetadataOriginal<'a> {
    pub identity: &'a HostIdentity,
    pub target: &'a PrSummary,
    pub context: &'a PrContext,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrMetadataPermission {
    RepositoryWriteObserved,
    /// Missing/default false booleans cannot establish denial; triage/custom
    /// capabilities are not represented by the existing Permissions DTO.
    MutationPermissionUnverified,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrMetadataEligibility {
    Eligible,
    Unverified,
    Ineligible,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrMetadataValue {
    BaseBranch { name: String, commit_sha: String },
    Label { id: u64, name: String },
    Assignee { account_id: u64, login: String },
    Reviewer { account_id: u64, login: String },
    ReviewTeam { team_id: u64, slug: String },
    Milestone { number: u64 },
}
#[derive(Clone, Debug)]
pub struct PrMetadataChoice {
    label: String,
    value: Option<PrMetadataValue>,
    actor: Option<ActorIdentity>,
    disabled_reason: Option<String>,
    eligibility: PrMetadataEligibility,
}
impl PrMetadataChoice {
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn value(&self) -> Option<&PrMetadataValue> {
        self.value.as_ref()
    }
    pub fn actor(&self) -> Option<&ActorIdentity> {
        self.actor.as_ref()
    }
    pub fn disabled_reason(&self) -> Option<&str> {
        self.disabled_reason.as_deref()
    }
    pub fn eligibility(&self) -> PrMetadataEligibility {
        self.eligibility
    }
    pub fn role(&self) -> SemanticToken {
        if self.disabled_reason.is_some() {
            SemanticToken::Pending
        } else {
            SemanticToken::Reference
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct OriginalBinding {
    target: PrWriteTarget,
    repository_id: u64,
    head_sha: String,
    base_sha: String,
    author_account_id: Option<u64>,
}
#[derive(Clone, Debug)]
pub struct PrMetadataSelection {
    original: OriginalBinding,
    value: PrMetadataValue,
}
impl PrMetadataSelection {
    pub fn target(&self) -> &PrWriteTarget {
        &self.original.target
    }
    pub fn head_sha(&self) -> &str {
        &self.original.head_sha
    }
    pub fn value(&self) -> &PrMetadataValue {
        &self.value
    }
    pub fn validate_original(&self, current: PrMetadataOriginal<'_>) -> Result<(), String> {
        if binding(current, 4096)? != self.original {
            return Err("Metadata choice belongs to a different current account/PR/repository/head/base/author; reload choices".into());
        }
        let permissions = &current.context.repository.permissions;
        if current.context.repository.archived
            || !(permissions.push || permissions.maintain || permissions.admin)
        {
            return Err("Caller field mutation permission is now unverified; reload actual capability before publishing".into());
        }
        Ok(())
    }
    /// Pure compatibility check only. The worker still authenticates and guards
    /// this newly read catalogue; a prepared model is not a provider receipt.
    pub fn validate_catalogue(&self, current: &PrMetadataChoices) -> Result<(), String> {
        if self.original != current.original
            || current.unavailable_reason.is_some()
            || !current
                .choices
                .iter()
                .any(|row| row.disabled_reason.is_none() && row.value.as_ref() == Some(&self.value))
        {
            return Err("Original indexed choice is absent/changed/unavailable in the fresh exact catalogue; no replacement selected".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct PrMetadataChoices {
    original: OriginalBinding,
    kind: MetadataChoices,
    permission: PrMetadataPermission,
    choices: Vec<PrMetadataChoice>,
    complete: bool,
    coverage_notice: Option<String>,
    unavailable_reason: Option<String>,
    fetched_pages: usize,
    raw_bytes: usize,
}
impl PrMetadataChoices {
    pub(crate) fn prepare_original_members_v48(
        actual: &crate::providers::gh::OriginalMetadataMembersV48,
        limits: PrMetadataLimits,
    ) -> Result<Self, String> {
        let read = actual.read();
        let coverage = read.coverage();
        let reason = coverage.reasons.join("; ");
        let mapped = if matches!(
            coverage.status,
            crate::providers::gh::MetricsReadStatus::Unavailable(_)
        ) && read.rows().is_empty()
        {
            PrMetadataCoverage::Unavailable { reason: &reason }
        } else if coverage.enumeration_complete && coverage.excluded_rows == 0 {
            PrMetadataCoverage::Complete {
                fetched_pages: coverage.fetched_pages,
            }
        } else {
            PrMetadataCoverage::Partial {
                fetched_pages: coverage.fetched_pages,
                reason: &reason,
            }
        };
        let mut prepared = Self::prepare(
            PrMetadataOriginal {
                identity: read.identity(),
                target: read.target(),
                context: read.context(),
            },
            read.kind(),
            PrMetadataCatalogue {
                rows: read.rows(),
                coverage: mapped,
            },
            limits,
        )?;
        if actual.command() == crate::pr_metadata_actions::PrMetadataCommand::AddLabel {
            const ARCHIVE_UNKNOWN: &str =
                "Label archived or archive evidence unavailable; Add is disabled";
            let extra = read
                .rows()
                .iter()
                .filter(|row| {
                    row.get("archived_at") != Some(&Value::Null)
                        || row.get("archived_by") != Some(&Value::Null)
                })
                .count()
                .checked_mul(ARCHIVE_UNKNOWN.len())
                .ok_or("Prepared archive reason size overflow")?;
            // Generic preparation is bounded before this additive typed caption;
            // charge the unchanged existing cap, never enlarge it for the tag.
            let retained = prepared
                .choices
                .iter()
                .map(|choice| {
                    choice.label.capacity()
                        + choice.disabled_reason.as_ref().map_or(0, String::capacity)
                        + choice.value.as_ref().map_or(0, value_capacity_v48)
                        + choice.actor.as_ref().map_or(0, |actor| {
                            actor.name.capacity()
                                + actor.login.as_ref().map_or(0, String::capacity)
                                + actor.avatar_url.as_ref().map_or(0, String::capacity)
                                + actor.host.as_ref().map_or(0, String::capacity)
                        })
                })
                .sum::<usize>()
                + prepared
                    .coverage_notice
                    .as_ref()
                    .map_or(0, String::capacity)
                + prepared
                    .unavailable_reason
                    .as_ref()
                    .map_or(0, String::capacity);
            if retained
                .checked_add(extra)
                .is_none_or(|bytes| bytes > limits.max_prepared_bytes)
            {
                return Err(
                    "Prepared archive evidence captions exceed unchanged picker cap".into(),
                );
            }
            for (choice, row) in prepared.choices.iter_mut().zip(read.rows()) {
                if row.get("archived_at") != Some(&Value::Null)
                    || row.get("archived_by") != Some(&Value::Null)
                {
                    choice.disabled_reason = Some(ARCHIVE_UNKNOWN.into());
                    choice.eligibility = PrMetadataEligibility::Unverified;
                }
            }
        }
        Ok(prepared)
    }
    pub fn prepare(
        original: PrMetadataOriginal<'_>,
        kind: MetadataChoices,
        catalogue: PrMetadataCatalogue<'_>,
        limits: PrMetadataLimits,
    ) -> Result<Self, String> {
        limits.validate()?;
        let binding = binding(original, limits.max_field_bytes)?;
        let raw_bytes = preflight(catalogue.rows, limits)?;
        let (complete, fetched_pages, coverage_notice, coverage_reason) = match catalogue.coverage {
            PrMetadataCoverage::Complete { fetched_pages } if (1..=100).contains(&fetched_pages)&&catalogue.rows.len()<=fetched_pages*100 => (true, fetched_pages, None, None),
            PrMetadataCoverage::Partial { fetched_pages, reason } if fetched_pages<=100&&catalogue.rows.len()<=fetched_pages*100 => {
                bounded_text(reason, limits.max_field_bytes)?;
                (false, fetched_pages, Some(format!("Choice catalogue partial: {}. Only observed indexed values are available; unseen membership/absence/removal is unknown.",sanitize_terminal(reason))), None)
            }
            PrMetadataCoverage::Unavailable { reason } if catalogue.rows.is_empty() => {
                bounded_text(reason, limits.max_field_bytes)?;
                (false,0,None,Some(format!("Choice catalogue unavailable: {}",sanitize_terminal(reason))))
            }
            _ => return Err("Choice pagination/availability evidence contradicts its actual rows or bounded page count".into()),
        };
        let permissions = &original.context.repository.permissions;
        let permission = if permissions.push || permissions.maintain || permissions.admin {
            PrMetadataPermission::RepositoryWriteObserved
        } else {
            PrMetadataPermission::MutationPermissionUnverified
        };
        let unavailable_reason = coverage_reason.or_else(|| {
            if original.context.repository.archived { Some("Repository is archived; metadata mutation availability is unverified".into()) }
            else if permission==PrMetadataPermission::MutationPermissionUnverified { Some("Caller mutation permission is unverified (triage/custom permissions are not in this DTO); browse only, verify the actual field capability before publishing".into()) }
            else if matches!(kind,MetadataChoices::Branches)&&original.target.pr.state!="open" { Some("Changing base needs a currently open PR; no branch effect is inferred".into()) }
            else { None }
        });
        let mut choices = Vec::with_capacity(catalogue.rows.len());
        let mut identities = BTreeSet::new();
        let mut names = BTreeSet::new();
        let mut prepared_bytes = unavailable_reason
            .as_ref()
            .map_or(0, String::len)
            .saturating_add(coverage_notice.as_ref().map_or(0, String::len));
        if prepared_bytes > limits.max_prepared_bytes {
            return Err("Prepared metadata label/actor/reason byte budget exceeded".into());
        }
        for row in catalogue.rows {
            let choice = choice(original, kind, row, binding.author_account_id);
            if let Some(value) = &choice.value {
                let (id, name) = keys(value);
                prepared_bytes = prepared_bytes
                    .saturating_add(id.len())
                    .saturating_add(name.len())
                    .saturating_add(value_bytes(value));
                if !identities.insert(id) || !names.insert(name) {
                    return Err("Eligible catalogue has duplicate/conflicting canonical identities or names; no alias chosen".into());
                }
            }
            prepared_bytes = prepared_bytes
                .saturating_add(choice.label.len())
                .saturating_add(choice.disabled_reason.as_ref().map_or(0, String::len));
            if let Some(actor) = &choice.actor {
                prepared_bytes = prepared_bytes
                    .saturating_add(actor.name.len())
                    .saturating_add(actor.login.as_ref().map_or(0, String::len))
                    .saturating_add(actor.avatar_url.as_ref().map_or(0, String::len));
                prepared_bytes =
                    prepared_bytes.saturating_add(actor.host.as_ref().map_or(0, String::len));
            }
            if prepared_bytes > limits.max_prepared_bytes {
                return Err("Prepared metadata label/actor/reason byte budget exceeded".into());
            }
            choices.push(choice);
        }
        Ok(Self {
            original: binding,
            kind,
            permission,
            choices,
            complete,
            coverage_notice,
            unavailable_reason,
            fetched_pages,
            raw_bytes,
        })
    }
    pub fn kind(&self) -> MetadataChoices {
        self.kind
    }
    pub fn choices(&self) -> &[PrMetadataChoice] {
        &self.choices
    }
    pub fn permission(&self) -> PrMetadataPermission {
        self.permission
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn fetched_pages(&self) -> usize {
        self.fetched_pages
    }
    pub fn raw_bytes(&self) -> usize {
        self.raw_bytes
    }
    pub fn unavailable_reason(&self) -> Option<&str> {
        self.unavailable_reason.as_deref()
    }
    /// Partial traversal permits only an explicitly observed eligible member.
    /// It cannot authorize replacement/removal or infer unseen membership.
    pub fn coverage_notice(&self) -> Option<&str> {
        self.coverage_notice.as_deref()
    }
    pub fn choose(&self, index: usize) -> Result<PrMetadataSelection, String> {
        if let Some(reason) = &self.unavailable_reason {
            return Err(reason.clone());
        }
        let row = self
            .choices
            .get(index)
            .ok_or("Metadata option index is outside the prepared catalogue")?;
        if let Some(reason) = &row.disabled_reason {
            return Err(reason.clone());
        }
        Ok(PrMetadataSelection {
            original: self.original.clone(),
            value: row
                .value
                .clone()
                .ok_or("No genuine canonical value is available for this row")?,
        })
    }
}

fn bounded_text(text: &str, max: usize) -> Result<(), String> {
    if text.len() > max {
        Err("Raw metadata UTF-8 field budget exceeded before cloning".into())
    } else {
        Ok(())
    }
}
fn binding(original: PrMetadataOriginal<'_>, max: usize) -> Result<OriginalBinding, String> {
    let PrMetadataOriginal {
        identity,
        target,
        context,
    } = original;
    for value in [
        &identity.host,
        &identity.login,
        &target.pr.id,
        &target.pr.host,
        &target.pr.repository,
        &target.head_sha,
        &target.base_sha,
        &target.pr.author.login,
        &context.repository.host,
        &context.repository.full_name,
        &context.base_repository.host,
        &context.base_repository.full_name,
        &context.head_sha,
        &context.base_sha,
    ] {
        bounded_text(value, max)?;
    }
    let key = PrWriteTarget::new(identity, target).map_err(|error| error.to_string())?;
    let key = PrWriteTarget::for_source(
        key.scope(),
        &target.pr.repository.to_ascii_lowercase(),
        key.node_id(),
        key.number(),
    )
    .map_err(|error| error.to_string())?;
    if !valid_sha(&target.head_sha)
        || !valid_sha(&target.base_sha)
        || context.head_sha != target.head_sha
        || context.base_sha != target.base_sha
        || !context.repository.host.eq_ignore_ascii_case(&identity.host)
        || !context
            .repository
            .full_name
            .eq_ignore_ascii_case(&target.pr.repository)
        || context.repository.id == 0
        || context.repository.id != context.base_repository.id
        || !context
            .base_repository
            .host
            .eq_ignore_ascii_case(&identity.host)
        || !context
            .base_repository
            .full_name
            .eq_ignore_ascii_case(&target.pr.repository)
    {
        return Err(
            "Metadata original context lacks the exact canonical repository/current full head/base"
                .into(),
        );
    }
    // An actual positive author ID is usable only from this same observed issue
    // parent/user. Unknown is not guessed from the PR author or current login.
    let raw = &context.issue_metadata;
    if let Some(login) = raw["user"]["login"].as_str() {
        bounded_text(login, max)?;
    }
    if let Some(url) = raw["html_url"].as_str() {
        bounded_text(url, max)?;
    }
    let canonical_parent = format!(
        "https://{}/{}/pull/{}",
        identity.host, target.pr.repository, target.pr.number
    );
    let author_account_id = if raw["number"].as_u64() == Some(target.pr.number)
        && raw["html_url"]
            .as_str()
            .is_some_and(|url| url.eq_ignore_ascii_case(&canonical_parent))
        && raw["user"]["login"]
            .as_str()
            .is_some_and(|login| login.eq_ignore_ascii_case(&target.pr.author.login))
    {
        raw["user"]["id"].as_u64().filter(|id| *id > 0)
    } else {
        None
    };
    Ok(OriginalBinding {
        target: key,
        repository_id: context.repository.id,
        head_sha: target.head_sha.clone(),
        base_sha: target.base_sha.clone(),
        author_account_id,
    })
}
fn preflight(rows: &[Value], limits: PrMetadataLimits) -> Result<usize, String> {
    if rows.len() > limits.max_rows {
        return Err(
            "Raw metadata row cap exceeded; no silent truncation or completeness claim".into(),
        );
    }
    let mut pending = rows.iter().map(|value| (value, 0usize)).collect::<Vec<_>>();
    let mut nodes = 0usize;
    while let Some((value, depth)) = pending.pop() {
        nodes += 1;
        if depth > 16 || nodes > 100_000 {
            return Err("Raw metadata nesting/node budget exceeded before cloning".into());
        }
        match value {
            Value::String(text) => bounded_text(text, limits.max_field_bytes)?,
            Value::Array(values) => {
                if values.len() > 100_000usize.saturating_sub(nodes + pending.len()) {
                    return Err("Raw metadata node budget exceeded before traversal".into());
                }
                pending.extend(values.iter().map(|child| (child, depth + 1)));
            }
            Value::Object(map) => {
                if map.len() > 100_000usize.saturating_sub(nodes + pending.len()) {
                    return Err("Raw metadata node budget exceeded before traversal".into());
                }
                for (key, child) in map {
                    bounded_text(key, limits.max_field_bytes)?;
                    pending.push((child, depth + 1));
                }
            }
            _ => {}
        }
    }
    let mut writer = ByteBudget {
        bytes: 0,
        maximum: limits.max_raw_bytes,
    };
    serde_json::to_writer(&mut writer, rows).map_err(|_| {
        "Raw serialized metadata byte budget exceeded before cloning/formatting".to_owned()
    })?;
    Ok(writer.bytes)
}
struct ByteBudget {
    bytes: usize,
    maximum: usize,
}
impl io::Write for ByteBudget {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > self.maximum.saturating_sub(self.bytes) {
            return Err(io::Error::other("bounded raw metadata bytes"));
        }
        self.bytes += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn text<'a>(row: &'a Value, key: &str) -> Option<&'a str> {
    row.get(key)?
        .as_str()
        .filter(|value| !value.is_empty() && !value.chars().any(char::is_control))
}
fn positive(row: &Value, key: &str) -> Option<u64> {
    row.get(key)?.as_u64().filter(|id| *id > 0)
}
fn name(value: &str) -> bool {
    !value.is_empty()
        && value != "."
        && value != ".."
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
}
fn branch(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 1024
        && value != "@"
        && value != "HEAD"
        && !value.starts_with('-')
        && !value.starts_with('/')
        && !value.ends_with('/')
        && !value.ends_with('.')
        && !value.contains("..")
        && !value.contains("@{")
        && !value.chars().any(|character| {
            character.is_control() || character.is_whitespace() || "~^:?*[\\".contains(character)
        })
        && value
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.') && !part.ends_with(".lock"))
}
fn readable(row: &Value) -> Option<bool> {
    // Current product/roles documentation permits READ reviewers. This is the
    // candidate's access, not the requesting account's mutation permission:
    // https://docs.github.com/en/pull-requests/reference/pull-request-reviews
    let permissions = row.get("permissions");
    let pull = permissions.and_then(|permissions| permissions["pull"].as_bool());
    let stronger = permissions.is_some_and(|permissions| {
        ["push", "maintain", "admin", "triage"]
            .iter()
            .any(|key| permissions[key].as_bool() == Some(true))
    });
    let role = match row
        .get("permission")
        .or_else(|| row.get("role_name"))
        .and_then(Value::as_str)
    {
        Some("read" | "pull" | "triage" | "write" | "push" | "maintain" | "admin") => Some(true),
        Some("none") => Some(false),
        _ => None,
    };
    match (pull, role) {
        (Some(left), Some(right)) if left != right => None,
        (Some(false), _) if stronger => None,
        (Some(value), _) | (_, Some(value)) => Some(value),
        _ => stronger.then_some(true),
    }
}
fn choice(
    original: PrMetadataOriginal<'_>,
    kind: MetadataChoices,
    row: &Value,
    author_id: Option<u64>,
) -> PrMetadataChoice {
    let mut result = PrMetadataChoice {
        label: "Unavailable metadata option".into(),
        value: None,
        actor: None,
        disabled_reason: None,
        eligibility: PrMetadataEligibility::Unverified,
    };
    let unavailable = |reason: &str| Some(reason.to_owned());
    match kind {
        MetadataChoices::Branches => {
            let candidate = text(row, "name").filter(|candidate| branch(candidate));
            let sha = row["commit"]["sha"].as_str().filter(|sha| valid_sha(sha));
            result.label = candidate.map_or_else(
                || "Unavailable branch".into(),
                |value| format!("Branch: {value}"),
            );
            if let (Some(name), Some(sha)) = (candidate, sha) {
                result.value = Some(PrMetadataValue::BaseBranch {
                    name: name.into(),
                    commit_sha: sha.into(),
                });
            } else {
                result.disabled_reason = unavailable(
                    "Branch name or genuine full commit SHA is unavailable; no ref/commit guessed",
                );
            }
        }
        MetadataChoices::Labels => {
            let label = text(row, "name");
            result.label = label.map_or_else(
                || "Unavailable label".into(),
                |label| format!("Label: {label}"),
            );
            if let (Some(name), Some(id)) = (label, positive(row, "id")) {
                result.value = Some(PrMetadataValue::Label {
                    id,
                    name: name.into(),
                });
            } else {
                result.disabled_reason =
                    unavailable("Label name/positive source ID is unavailable");
            }
        }
        MetadataChoices::Assignees | MetadataChoices::Collaborators => {
            let observed_login = text(row, "login");
            let login = observed_login.filter(|login| name(login));
            let actor = Actor {
                login: observed_login.unwrap_or_default().into(),
                display_name: text(row, "name").map(str::to_owned),
                avatar_url: text(row, "avatar_url").map(str::to_owned),
                actor_type: text(row, "type").unwrap_or("Unknown").into(),
            };
            let identity = ActorIdentity::github(&original.identity.host, &actor);
            result.label = identity.label();
            result.actor = Some(identity);
            if let (Some(login), Some(id)) = (login, positive(row, "id")) {
                if matches!(kind, MetadataChoices::Assignees) {
                    // The assignees endpoint is itself the eligible assignment list.
                    result.value = Some(PrMetadataValue::Assignee {
                        account_id: id,
                        login: login.into(),
                    });
                } else if author_id == Some(id) {
                    result.eligibility = PrMetadataEligibility::Ineligible;
                    result.disabled_reason = unavailable(
                        "This is the original PR author; a self-review request is unavailable",
                    );
                } else if author_id.is_none() {
                    result.disabled_reason = unavailable(
                        "Original PR author numeric identity is unavailable; cannot verify a different reviewer from reusable login text",
                    );
                } else if readable(row) != Some(true) {
                    if readable(row) == Some(false) {
                        result.eligibility = PrMetadataEligibility::Ineligible;
                    }
                    result.disabled_reason = unavailable(
                        "Candidate repository read/collaborator eligibility is unverified; caller write permission is separate",
                    );
                } else {
                    result.value = Some(PrMetadataValue::Reviewer {
                        account_id: id,
                        login: login.into(),
                    });
                }
            } else {
                result.disabled_reason = unavailable(
                    "Actor source numeric ID/login is unavailable; display fallback is not a target",
                );
            }
        }
        MetadataChoices::Teams => {
            let slug = text(row, "slug").filter(|slug| name(slug));
            let label = text(row, "name");
            result.label = match (label, slug) {
                (Some(label), Some(slug)) => format!("Team: {label} ({slug})"),
                (None, Some(slug)) => format!("Team: {slug} • name unavailable"),
                _ => "Unavailable team".into(),
            };
            if let (Some(team_id), Some(slug)) = (positive(row, "id"), slug) {
                if readable(row) == Some(true) {
                    result.value = Some(PrMetadataValue::ReviewTeam {
                        team_id,
                        slug: slug.into(),
                    });
                } else {
                    if readable(row) == Some(false) {
                        result.eligibility = PrMetadataEligibility::Ineligible;
                    }
                    result.disabled_reason = unavailable(
                        "Team repository read eligibility is unverified; no write-role requirement inferred",
                    );
                }
            } else {
                result.disabled_reason =
                    unavailable("Team genuine positive ID/slug is unavailable");
            }
        }
        MetadataChoices::Milestones => {
            result.label = text(row, "title").map_or_else(
                || {
                    positive(row, "number").map_or_else(
                        || "Unavailable milestone".into(),
                        |number| format!("Milestone #{number} • title unavailable"),
                    )
                },
                |title| format!("Milestone: {title}"),
            );
            if let Some(number) = positive(row, "number") {
                if text(row, "state") == Some("open") {
                    result.value = Some(PrMetadataValue::Milestone { number });
                } else {
                    if text(row, "state") == Some("closed") {
                        result.eligibility = PrMetadataEligibility::Ineligible;
                    }
                    result.disabled_reason = unavailable(
                        "Milestone is closed or its open state is unavailable; no nullable state guessed",
                    );
                }
            } else {
                result.disabled_reason =
                    unavailable("Milestone positive source number is unavailable");
            }
        }
    }
    result.label = sanitize_terminal(&result.label);
    if result.value.is_some() && result.disabled_reason.is_none() {
        result.eligibility = PrMetadataEligibility::Eligible;
    }
    result
}
fn value_bytes(value: &PrMetadataValue) -> usize {
    match value {
        PrMetadataValue::BaseBranch { name, commit_sha } => name.len() + commit_sha.len(),
        PrMetadataValue::Label { name, .. } => name.len(),
        PrMetadataValue::Assignee { login, .. } | PrMetadataValue::Reviewer { login, .. } => {
            login.len()
        }
        PrMetadataValue::ReviewTeam { slug, .. } => slug.len(),
        PrMetadataValue::Milestone { .. } => 0,
    }
}
fn value_capacity_v48(value: &PrMetadataValue) -> usize {
    match value {
        PrMetadataValue::BaseBranch { name, commit_sha } => {
            name.capacity().saturating_add(commit_sha.capacity())
        }
        PrMetadataValue::Label { name, .. } => name.capacity(),
        PrMetadataValue::Assignee { login, .. } | PrMetadataValue::Reviewer { login, .. } => {
            login.capacity()
        }
        PrMetadataValue::ReviewTeam { slug, .. } => slug.capacity(),
        PrMetadataValue::Milestone { .. } => 0,
    }
}
/// Capacity-only borrowed traversal; accepts no source/permission and performs
/// no I/O/parse/clone. New member context must fit BEFORE further retained
/// copies/row encodings and is charged to the existing presentation cap.
pub(crate) fn members_context_capacity_v48(context: &PrContext) -> Option<usize> {
    struct Encoded(usize);
    impl io::Write for Encoded {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= 512 * 1024)
                .ok_or_else(|| io::Error::other("Member context encoded cap exceeded"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
    // No second JSON tree or body string is allocated by this preflight.
    serde_json::to_writer(&mut Encoded(0), context).ok()?;
    fn repo(repo: &crate::providers::gh::Repository) -> Option<usize> {
        [
            &repo.host,
            &repo.full_name,
            &repo.description,
            &repo.default_branch,
        ]
        .into_iter()
        .try_fold(std::mem::size_of_val(repo), |n, s| {
            n.checked_add(s.capacity())
        })
    }
    fn actor(actor: &Actor) -> Option<usize> {
        actor
            .login
            .capacity()
            .checked_add(actor.actor_type.capacity())?
            .checked_add(actor.display_name.as_ref().map_or(0, String::capacity))?
            .checked_add(actor.avatar_url.as_ref().map_or(0, String::capacity))
    }
    fn json(value: &Value, depth: usize) -> Option<usize> {
        if depth > 64 {
            return None;
        }
        let size = match value {
            Value::String(value) => value.capacity(),
            Value::Array(rows) => rows.iter().try_fold(
                rows.capacity().checked_mul(std::mem::size_of::<Value>())?,
                |n, value| n.checked_add(json(value, depth + 1)?),
            )?,
            // Conservative BTreeMap entry/node overhead; no hash-table capacity
            // or serialized-length shortcut substitutes for retained strings.
            Value::Object(map) => map.iter().try_fold(
                map.len().checked_mul(
                    2 * (std::mem::size_of::<String>()
                        + std::mem::size_of::<Value>()
                        + 6 * std::mem::size_of::<usize>()),
                )?,
                |n, (key, value)| {
                    n.checked_add(key.capacity())?
                        .checked_add(json(value, depth + 1)?)
                },
            )?,
            _ => 0,
        };
        (size <= 2 * 1024 * 1024).then_some(size)
    }
    let mut bytes = std::mem::size_of_val(context)
        .checked_add(repo(&context.repository)?)?
        .checked_add(repo(&context.base_repository)?)?
        .checked_add(context.head_repository.as_ref().map_or(Some(0), repo)?)?
        .checked_add(json(&context.issue_metadata, 0)?)?
        .checked_add(
            context
                .requested_reviewers
                .capacity()
                .checked_mul(std::mem::size_of::<Actor>())?,
        )?
        .checked_add(
            context
                .requested_teams
                .capacity()
                .checked_mul(std::mem::size_of::<crate::providers::gh::RequestedTeam>())?,
        )?
        .checked_add(
            context
                .source_urls
                .capacity()
                .checked_mul(std::mem::size_of::<String>())?,
        )?
        .checked_add(context.head_sha.capacity())?
        .checked_add(context.base_sha.capacity())?;
    for value in &context.requested_reviewers {
        bytes = bytes.checked_add(actor(value)?)?;
    }
    for value in &context.requested_teams {
        bytes = bytes
            .checked_add(value.slug.capacity())?
            .checked_add(value.name.capacity())?;
    }
    for value in &context.source_urls {
        bytes = bytes.checked_add(value.capacity())?;
    }
    (bytes <= 2 * 1024 * 1024).then_some(bytes)
}
fn keys(value: &PrMetadataValue) -> (String, String) {
    match value {
        PrMetadataValue::BaseBranch { name, .. } => (name.clone(), name.clone()),
        PrMetadataValue::Label { id, name } => (id.to_string(), name.clone()),
        PrMetadataValue::Assignee { account_id, login }
        | PrMetadataValue::Reviewer { account_id, login } => {
            (account_id.to_string(), login.to_ascii_lowercase())
        }
        PrMetadataValue::ReviewTeam { team_id, slug } => {
            (team_id.to_string(), slug.to_ascii_lowercase())
        }
        PrMetadataValue::Milestone { number } => (number.to_string(), number.to_string()),
    }
}

#[cfg(test)]
#[path = "pr_metadata_choices_tests.rs"]
mod tests;
