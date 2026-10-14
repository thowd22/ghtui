//! Exact local-commit host reports, issued only by actual bounded gh reads.
//! Commit presence does NOT establish a local-repository mapping, signatures,
//! current future access, or mutation permission.
use super::*;
use crate::{
    git::{OriginalGitActorHeader, OriginalLocalCommitActors},
    identity::{ActorIdentity, ActorKind, GitRole},
    repository_catalogue_intent_v1::IntentReference,
    storage::{Cache, CatalogueIntentSource},
};

const FIELD_BYTES: usize = 4096;
const COMMIT_CAPTURE: usize = 1024 * 1024;
const WHOLE_CAPTURE: usize = 4 * 1024 * 1024;

#[path = "related_pr_v1.rs"]
mod related_pr_v1;
pub use related_pr_v1::*;

#[derive(Clone, Debug)]
pub struct LocalCommitActorReadLimits {
    pub max_duration: Duration,
    pub max_response_bytes: usize,
}
impl Default for LocalCommitActorReadLimits {
    fn default() -> Self {
        Self {
            max_duration: Duration::from_secs(30),
            max_response_bytes: WHOLE_CAPTURE,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalCommitActorUnavailable {
    NoReportedAccount,
    MissingOrMalformedMetadata,
    DisplayLimit,
}
pub struct ReportedCommitActor {
    database_id: u64,
    node_id: Option<String>,
    actor: Actor,
    display: ActorIdentity,
    label: String,
}
impl std::fmt::Debug for ReportedCommitActor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReportedCommitActor(<original read-only role observation>)")
    }
}
impl ReportedCommitActor {
    pub fn database_id(&self) -> u64 {
        self.database_id
    }
    pub fn node_id(&self) -> Option<&str> {
        self.node_id.as_deref()
    }
    pub fn actor(&self) -> &Actor {
        &self.actor
    }
    pub fn display(&self) -> &ActorIdentity {
        &self.display
    }
    /// Bounded prepared text keeps missing local name/email honest even where
    /// the legacy generic associated-identity label assumed nonempty fields.
    pub fn label(&self) -> &str {
        &self.label
    }
}
pub enum LocalCommitRoleReport {
    Reported(Box<ReportedCommitActor>),
    Unavailable(LocalCommitActorUnavailable),
}
impl std::fmt::Debug for LocalCommitRoleReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Reported(_) => f.write_str("Reported(<role observation>)"),
            Self::Unavailable(reason) => f.debug_tuple("Unavailable").field(reason).finish(),
        }
    }
}
#[derive(Debug, PartialEq, Eq)]
pub struct ReportedCommitRepository {
    id: u64,
    node_id: String,
    full_name: String,
    owner_id: u64,
    owner_node: String,
    owner_kind: String,
}
impl ReportedCommitRepository {
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn node_id(&self) -> &str {
        &self.node_id
    }
    pub fn full_name(&self) -> &str {
        &self.full_name
    }
    pub fn owner_id(&self) -> u64 {
        self.owner_id
    }
    pub fn owner_node(&self) -> &str {
        &self.owner_node
    }
    pub fn owner_kind(&self) -> &str {
        &self.owner_kind
    }
}
/// Private constructor/no serde/Clone approval. A renderer borrows this already
/// admitted observation; it cannot construct one from a public metadata DTO.
pub struct ClosedLocalCommitActors {
    original: Arc<CatalogueIntentSource>,
    local: Arc<OriginalLocalCommitActors>,
    selected_index: usize,
    closed: ClosedCatalogueAccess,
    repository: ReportedCommitRepository,
    author: LocalCommitRoleReport,
    committer: LocalCommitRoleReport,
    observed_from: u64,
    observed_through: u64,
}
impl std::fmt::Debug for ClosedLocalCommitActors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ClosedLocalCommitActors(<closed selected repository report; not mapping or authority>)")
    }
}
impl ClosedLocalCommitActors {
    pub fn original_source(&self) -> &Arc<CatalogueIntentSource> {
        &self.original
    }
    pub fn original_local(&self) -> &Arc<OriginalLocalCommitActors> {
        &self.local
    }
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }
    pub fn identity(&self) -> &HostIdentity {
        self.closed.identity()
    }
    pub fn repository(&self) -> &ReportedCommitRepository {
        &self.repository
    }
    pub fn sha(&self) -> &str {
        self.local.sha()
    }
    pub fn role(&self, role: GitRole) -> &LocalCommitRoleReport {
        match role {
            GitRole::Author => &self.author,
            GitRole::Committer => &self.committer,
        }
    }
    pub fn observed_interval(&self) -> (u64, u64) {
        (self.observed_from, self.observed_through)
    }
    pub fn accounting(&self) -> &ClosedCatalogueAccess {
        &self.closed
    }
    pub fn label(&self) -> &'static str {
        "GitHub reports this commit in selected repository"
    }
    pub fn limitations(&self) -> &'static [&'static str] {
        &[
            "Observed read-only host account reports, not a verified local-repository mapping",
            "Not commit-signature verification or current future read/write permission",
            "Non-atomic principal/repository/commit observations; explicit verification only",
        ]
    }
}

/// Public request intent holds genuine opaque sources, not a permission bool.
/// Only the actual factory below can issue a closed role report.
pub struct LocalCommitActorReadTarget<'a> {
    pub original: &'a Arc<CatalogueIntentSource>,
    pub selected_index: usize,
    pub local: &'a Arc<OriginalLocalCommitActors>,
}

impl GithubProvider {
    /// Both genuine roles in ONE read. The SQL fence is enforced here, not a
    /// public caller-supplied authority bool; extra guard is source continuity.
    pub fn local_commit_actor_read(
        &self,
        target: LocalCommitActorReadTarget<'_>,
        cache: &Cache,
        limits: &LocalCommitActorReadLimits,
        cancel: &CancellationToken,
        source_guard: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<ClosedLocalCommitActors> {
        let LocalCommitActorReadTarget {
            original,
            selected_index,
            local,
        } = target;
        if limits.max_duration.is_zero()
            || limits.max_duration > Duration::from_secs(30)
            || limits.max_response_bytes == 0
            || limits.max_response_bytes > WHOLE_CAPTURE
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Local commit report requires bounded original duration/capture",
            ));
        }
        let initial_remaining = local.remaining(cancel).map_err(|_| {
            error(
                GithubErrorKind::Transport,
                "Original raw local actor deadline/cancellation unavailable",
            )
        })?;
        let actor_deadline = Instant::now()
            .checked_add(limits.max_duration.min(initial_remaining))
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Original actor deadline unavailable",
                )
            })?
            .min(local.deadline());
        let observed_from = utc_now()?;
        let source = original.source();
        let row = source.repositories().get(selected_index).ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Choose one genuine original catalogue row; no default repository",
            )
        })?;
        let node = row
            .node_id
            .as_deref()
            .filter(|node| opaque(node))
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Original repository node is unverified; discover explicitly",
                )
            })?;
        let original_owner = row
            .owner
            .as_ref()
            .filter(|owner| {
                owner.database_id.is_some_and(|id| id > 0)
                    && owner.node_id.as_deref().is_some_and(opaque)
                    && matches!(owner.actor.actor_type.as_str(), "User" | "Organization")
            })
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Original canonical owner identity is unverified",
                )
            })?;
        if row.full_name.len() > FIELD_BYTES
            || self.host() != original.scope().host
            || source.identity().account_id.is_none_or(|id| id == 0)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Original explicit host/repository/numeric scope is unavailable",
            ));
        }
        let reference = IntentReference {
            id: row.id,
            node_id: Some(node.into()),
            last_name: row.full_name.clone(),
        };
        // Local continuity is not account/access proof. Reject a retired source
        // before any actor transport, but do not read private SQL here.
        recheck_local(local, cancel)?;
        let remaining = local.remaining(cancel).map_err(|_| {
            error(
                GithubErrorKind::Transport,
                "Original raw local deadline/cancellation expired during source preparation",
            )
        })?;
        let actor_remaining = actor_deadline
            .checked_duration_since(Instant::now())
            .filter(|value| !value.is_zero())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Original actor deadline expired during source preparation",
                )
            })?;
        let bounded = self.clone().with_catalogue_read_deadline(actor_deadline);
        let policy = RepositoryCatalogueAccessLimits {
            transport: RepositoryCatalogueLimits {
                max_duration: actor_remaining.min(remaining),
                max_response_bytes: limits.max_response_bytes,
                ..Default::default()
            },
            max_references: 1,
            max_repository_reads: 2,
        };
        let mut session = bounded.begin_repository_catalogue_access(source, &policy, cancel)?;
        let first = session
            .verify(&reference, cancel)
            .and_then(|row| header(row.repository()));
        let first = match first {
            Ok(first) => first,
            Err(failure) => {
                close_failed(session, original, local, cache, cancel, source_guard)?;
                return Err(safe_actor_failure(failure));
            }
        };
        // The row may retain an actual owner node even when the independent
        // owner endpoint omitted it. Do not let the session's nullable broader
        // owner context weaken this genuine selected-row provenance.
        if Some(first.owner_id) != original_owner.database_id
            || Some(first.owner_node.as_str()) != original_owner.node_id.as_deref()
            || first.owner_kind != original_owner.actor.actor_type
        {
            close_failed(session, original, local, cache, cancel, source_guard)?;
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Genuine original selected-row owner database/node/kind changed",
            ));
        }
        // Fresh positive principal and exact original repository/owner facts
        // precede private source SQL. A stale/foreign issuer cannot preempt the
        // actual account check or authorize a commit read.
        cache
            .validate_repository_catalogue_intent_source(original)
            .map_err(|_| boundary())?;
        local.remaining(cancel).map_err(|_| {
            error(
                GithubErrorKind::Transport,
                "Original local actor window expired during current source validation",
            )
        })?;
        let window = session.reserve_measured_external_reads(
            1,
            COMMIT_CAPTURE,
            COMMIT_CAPTURE,
            1,
            cancel,
        )?;
        let client = bounded.clone().with_measured_read_window(window)?;
        let response = client.api(
            &format!(
                "repos/{}/commits/{}?per_page=1",
                first.full_name,
                local.sha()
            ),
            "GET",
            None,
            None,
            false,
            cancel,
        );
        // This repeat is deliberate source closure, never current-name rebase.
        // A cancelled operation cannot launch it; the session checks original
        // cancellation/deadline before every child.
        let last = session
            .verify(&reference, cancel)
            .and_then(|row| header(row.repository()));
        let observed_through = utc_now()?;
        let closed = close_failed(session, original, local, cache, cancel, source_guard)?;
        let last = last.map_err(safe_actor_failure)?;
        if first != last
            || closed.observation(reference.id).is_none()
            || !closed.failures().is_empty()
        {
            return Err(error(GithubErrorKind::IdentityChanged,"Original canonical repository moved/failed during report read; no private actor publication"));
        }
        let response = response.map_err(safe_actor_failure)?;
        if response.get("sha").and_then(Value::as_str) != Some(local.sha()) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Host commit response does not match exact original full SHA",
            ));
        }
        checked_url(
            &response,
            "url",
            &self.host,
            &format!("repos/{}/commits/{}", first.full_name, local.sha()),
            true,
        )?;
        checked_url(
            &response,
            "html_url",
            &self.host,
            &format!("{}/commit/{}", first.full_name, local.sha()),
            false,
        )?;
        checked_url(
            &response["commit"],
            "url",
            &self.host,
            &format!("repos/{}/git/commits/{}", first.full_name, local.sha()),
            true,
        )?;
        let author = role_report(&response, local.author(), &self.host, GitRole::Author)?;
        let committer = role_report(&response, local.committer(), &self.host, GitRole::Committer)?;
        for report in [&author, &committer] {
            if let LocalCommitRoleReport::Reported(actor) = report {
                if actor.node_id.as_deref() == Some(first.node_id.as_str())
                    || actor.node_id.as_deref().is_some_and(|node| {
                        node == first.owner_node
                            && (actor.database_id != first.owner_id
                                || actor.actor.actor_type != first.owner_kind)
                    })
                    || (actor.database_id == first.owner_id
                        && (actor.actor.actor_type != first.owner_kind
                            || actor
                                .node_id
                                .as_deref()
                                .is_some_and(|node| node != first.owner_node)))
                {
                    return Err(error(GithubErrorKind::IdentityChanged,"Actual opaque/account role identity contradicts canonical repository/owner namespace"));
                }
            }
        }
        if let (
            LocalCommitRoleReport::Reported(author),
            LocalCommitRoleReport::Reported(committer),
        ) = (&author, &committer)
        {
            if author
                .node_id
                .as_ref()
                .zip(committer.node_id.as_ref())
                .is_some_and(|(a, c)| a == c && author.database_id != committer.database_id)
                || (author.database_id == committer.database_id
                    && author
                        .node_id
                        .as_ref()
                        .zip(committer.node_id.as_ref())
                        .is_some_and(|(a, c)| a != c))
            {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Actual role actor database/node provenance contradicts itself",
                ));
            }
        }
        // Pure bounded construction only AFTER final principal proof. No SQL,
        // local stat/object resolution, auth refresh, asset fetch or new clock.
        local.remaining(cancel).map_err(|_| {
            error(
                GithubErrorKind::Transport,
                "Original raw actor operation expired/cancelled before publication",
            )
        })?;
        if Instant::now() >= actor_deadline {
            return Err(error(
                GithubErrorKind::Transport,
                "Original actor deadline expired before publication",
            ));
        }
        Ok(ClosedLocalCommitActors {
            original: original.clone(),
            local: local.clone(),
            selected_index,
            closed,
            repository: first,
            author,
            committer,
            observed_from,
            observed_through,
        })
    }
}
fn close_failed(
    session: RepositoryCatalogueAccessSession<'_>,
    original: &CatalogueIntentSource,
    local: &OriginalLocalCommitActors,
    cache: &Cache,
    cancel: &CancellationToken,
    source_guard: impl FnOnce() -> GithubResult<()>,
) -> GithubResult<ClosedCatalogueAccess> {
    session.close_with_guard(cancel, || {
        recheck_local(local, cancel)?;
        local.remaining(cancel).map_err(|_| {
            error(
                GithubErrorKind::Transport,
                "Original raw local actor operation retired",
            )
        })?;
        source_guard()?;
        local.remaining(cancel).map_err(|_| {
            error(
                GithubErrorKind::Transport,
                "Original raw local operation retired during source guard",
            )
        })?;
        recheck_local(local, cancel)?;
        cache
            .validate_repository_catalogue_intent_source(original)
            .map_err(|_| boundary())
    })
}
fn recheck_local(
    local: &OriginalLocalCommitActors,
    cancel: &CancellationToken,
) -> GithubResult<()> {
    local.remaining(cancel).map_err(|_| {
        error(
            GithubErrorKind::Transport,
            "Original local actor clock/cancellation retired",
        )
    })?;
    let checked = local.recheck_original_source(cancel);
    local.remaining(cancel).map_err(|_| {
        error(
            GithubErrorKind::Transport,
            "Original local actor clock/cancellation retired during source check",
        )
    })?;
    checked.map_err(|_| {
        error(
            GithubErrorKind::IdentityChanged,
            "Original local Git directory/locator source changed before closing proof",
        )
    })
}
fn boundary() -> GithubError {
    error(
        GithubErrorKind::IdentityChanged,
        "Original catalogue storage/source fence changed; no private actor publication",
    )
}
fn safe_actor_failure(failure: GithubError) -> GithubError {
    let message=match &failure.kind {
        GithubErrorKind::Unsupported=>"Current selected repository/transport is unverified or unsupported, not denied/granted; retain local Git actors",
        GithubErrorKind::Permission=>"Current selected repository read denied; retain local Git actors",
        _=>"Exact selected repository/commit read unavailable; retain local Git actors and verify explicitly",
    };
    error(failure.kind, message)
}
fn opaque(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn header(row: &RepositoryCatalogueRepository) -> GithubResult<ReportedCommitRepository> {
    match row.permissions.pull {
        Some(true) => {}
        None => {
            return Err(error(
                GithubErrorKind::Unsupported,
                "Current pull access is unverified, not denied or granted",
            ))
        }
        Some(false) => {
            return Err(error(
                GithubErrorKind::Permission,
                "Current selected repository does not report pull access",
            ))
        }
    }
    let owner = row.owner.as_ref().ok_or_else(|| {
        error(
            GithubErrorKind::IdentityChanged,
            "Current canonical owner unavailable",
        )
    })?;
    let owner_id = owner.database_id.filter(|id| *id > 0).ok_or_else(|| {
        error(
            GithubErrorKind::IdentityChanged,
            "Current owner positive database ID unavailable",
        )
    })?;
    let node = row
        .node_id
        .as_deref()
        .filter(|value| opaque(value))
        .ok_or_else(|| {
            error(
                GithubErrorKind::IdentityChanged,
                "Current repository node unavailable",
            )
        })?;
    let owner_node = owner
        .node_id
        .as_deref()
        .filter(|value| opaque(value))
        .ok_or_else(|| {
            error(
                GithubErrorKind::IdentityChanged,
                "Current owner node unavailable",
            )
        })?;
    if row.id == 0
        || row.full_name.len() > FIELD_BYTES
        || !matches!(owner.actor.actor_type.as_str(), "User" | "Organization")
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Current repository identity bounded provenance unavailable",
        ));
    }
    Ok(ReportedCommitRepository {
        id: row.id,
        node_id: node.into(),
        full_name: row.full_name.clone(),
        owner_id,
        owner_node: owner_node.into(),
        owner_kind: owner.actor.actor_type.clone(),
    })
}
fn checked_url(value: &Value, field: &str, host: &str, path: &str, api: bool) -> GithubResult<()> {
    if let Some(value) = value.get(field).filter(|value| !value.is_null()) {
        if !value
            .as_str()
            .filter(|value| value.len() <= FIELD_BYTES)
            .is_some_and(|url| super::super::discussions::exact_url_path(url, host, path, api))
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Supplied host report URL contradicts original host/repository/commit or actor",
            ));
        }
    }
    Ok(())
}
fn role_report(
    value: &Value,
    local: &OriginalGitActorHeader,
    host: &str,
    role: GitRole,
) -> GithubResult<LocalCommitRoleReport> {
    use LocalCommitActorUnavailable::*;
    let field = match role {
        GitRole::Author => "author",
        GitRole::Committer => "committer",
    };
    let raw = &value["commit"][field];
    let Some(name) = raw["name"].as_str() else {
        return Ok(LocalCommitRoleReport::Unavailable(
            MissingOrMalformedMetadata,
        ));
    };
    let Some(email) = raw["email"].as_str() else {
        return Ok(LocalCommitRoleReport::Unavailable(
            MissingOrMalformedMetadata,
        ));
    };
    let Some(seconds) = raw["date"]
        .as_str()
        .and_then(crate::github_workspace::timestamp_seconds)
        .and_then(|value| i64::try_from(value).ok())
    else {
        return Ok(LocalCommitRoleReport::Unavailable(
            MissingOrMalformedMetadata,
        ));
    };
    if name.as_bytes() != local.name_bytes()
        || email.as_bytes() != local.email_bytes()
        || seconds != local.seconds()
    {
        return Err(error(
            GithubErrorKind::IdentityChanged,
            "Reported raw Git name/email/seconds do not match original immutable object role",
        ));
    }
    let Some(remote) = value.get(field) else {
        return Ok(LocalCommitRoleReport::Unavailable(
            MissingOrMalformedMetadata,
        ));
    };
    if remote.is_null() {
        return Ok(LocalCommitRoleReport::Unavailable(NoReportedAccount));
    }
    let Some(id) = remote["id"].as_u64().filter(|id| *id > 0) else {
        return Ok(LocalCommitRoleReport::Unavailable(
            MissingOrMalformedMetadata,
        ));
    };
    let Some(login) = remote["login"].as_str().filter(|login| {
        !login.is_empty()
            && login.len() <= 100
            && !login
                .chars()
                .any(|ch| ch.is_control() || ch.is_whitespace() || matches!(ch, '/' | '\\' | '@'))
    }) else {
        return Ok(LocalCommitRoleReport::Unavailable(
            MissingOrMalformedMetadata,
        ));
    };
    let Some(kind) = remote["type"]
        .as_str()
        .filter(|kind| matches!(*kind, "User" | "Bot"))
    else {
        return Ok(LocalCommitRoleReport::Unavailable(
            MissingOrMalformedMetadata,
        ));
    };
    let checked_login = if kind == "Bot" {
        login.strip_suffix("[bot]").unwrap_or(login)
    } else {
        login
    };
    if validate_name(checked_login).is_err() {
        return Ok(LocalCommitRoleReport::Unavailable(
            MissingOrMalformedMetadata,
        ));
    }
    let node = match remote.get("node_id") {
        None | Some(Value::Null) => None,
        Some(value) => match value.as_str().filter(|value| opaque(value)) {
            Some(node) => Some(node.to_owned()),
            None => {
                return Ok(LocalCommitRoleReport::Unavailable(
                    MissingOrMalformedMetadata,
                ))
            }
        },
    };
    let optional = |field: &str| -> Result<Option<&str>, ()> {
        match remote.get(field) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_str()
                .filter(|value| value.len() <= FIELD_BYTES)
                .map(Some)
                .ok_or(()),
        }
    };
    let (display_name, avatar_url) = match (optional("name"), optional("avatar_url")) {
        (Ok(name), Ok(url)) => (name, url),
        _ => return Ok(LocalCommitRoleReport::Unavailable(DisplayLimit)),
    };
    checked_url(
        remote,
        "url",
        host,
        &format!("users/{}", url_component(login)),
        true,
    )?;
    let name_display = crate::git::display_bytes(local.name_bytes());
    let email_display = crate::git::display_bytes(local.email_bytes());
    if name_display.len() > FIELD_BYTES || email_display.len() > FIELD_BYTES {
        return Ok(LocalCommitRoleReport::Unavailable(DisplayLimit));
    }
    let actor = Actor {
        login: login.into(),
        display_name: display_name.map(str::to_owned),
        avatar_url: avatar_url.map(str::to_owned),
        actor_type: kind.into(),
    };
    let mut display = ActorIdentity::github(host, &actor);
    if display.kind == ActorKind::Unknown || display.login.is_none() {
        return Ok(LocalCommitRoleReport::Unavailable(DisplayLimit));
    }
    display.name = name_display;
    display.email = Some(email_display);
    display.git_role = Some(role);
    let local_label = crate::git_actor_labels_v1::local_label(
        &display.name,
        display.email.as_deref(),
        Some(role),
    );
    let label = format!(
        "{local_label} · @{login}{} [host report]",
        match display.kind {
            ActorKind::Bot => " [bot]",
            ActorKind::Deleted => " [deleted]",
            _ => "",
        }
    );
    Ok(LocalCommitRoleReport::Reported(Box::new(
        ReportedCommitActor {
            database_id: id,
            node_id: node,
            actor,
            display,
            label,
        },
    )))
}
