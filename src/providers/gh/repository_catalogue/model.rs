use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RepositoryCatalogueSource {
    Organization(String),
    AuthenticatedAccessible,
    PublicUser(String),
}
impl RepositoryCatalogueSource {
    pub(crate) fn validate(&self) -> GithubResult<()> {
        if let Some(owner) = self.owner() {
            if owner.len() > 100 {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Explicit catalogue owner exceeds its raw-name cap",
                ));
            }
            validate_name(owner)?;
        }
        Ok(())
    }
    pub fn owner(&self) -> Option<&str> {
        match self {
            Self::Organization(owner) | Self::PublicUser(owner) => Some(owner),
            Self::AuthenticatedAccessible => None,
        }
    }
    pub(super) fn owner_endpoint(&self) -> Option<String> {
        match self {
            Self::Organization(owner) => Some(format!("orgs/{owner}")),
            Self::PublicUser(owner) => Some(format!("users/{owner}")),
            Self::AuthenticatedAccessible => None,
        }
    }
    pub(super) fn page_endpoint(&self, page: usize, size: usize) -> String {
        match self {
            Self::Organization(owner)=>format!("orgs/{owner}/repos?type=all&sort=full_name&direction=asc&per_page={size}&page={page}"),
            Self::AuthenticatedAccessible=>format!("user/repos?visibility=all&affiliation=owner,collaborator,organization_member&sort=full_name&direction=asc&per_page={size}&page={page}"),
            Self::PublicUser(owner)=>format!("users/{owner}/repos?type=owner&sort=full_name&direction=asc&per_page={size}&page={page}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RepositoryCatalogueLimits {
    pub max_pages: usize,
    /// Fixed for one original request. Official REST accepts1..100.
    pub page_size: usize,
    pub max_rows: usize,
    pub max_document_bytes: usize,
    pub max_metadata_bytes: usize,
    pub max_response_bytes: usize,
    pub max_duration: Duration,
}
impl Default for RepositoryCatalogueLimits {
    fn default() -> Self {
        Self {
            max_pages: 20,
            page_size: 100,
            max_rows: 2000,
            max_document_bytes: 128 * 1024,
            max_metadata_bytes: 8 * 1024 * 1024,
            max_response_bytes: 16 * 1024 * 1024,
            max_duration: Duration::from_secs(30),
        }
    }
}
impl RepositoryCatalogueLimits {
    pub(super) fn validate(&self) -> GithubResult<()> {
        if !(1..=1000).contains(&self.max_pages)
            || !(1..=100).contains(&self.page_size)
            || !(1..=100000).contains(&self.max_rows)
            || self.page_size > self.max_rows
            || !(1..=1024 * 1024).contains(&self.max_document_bytes)
            || !(1..=32 * 1024 * 1024).contains(&self.max_metadata_bytes)
            || !(1..=64 * 1024 * 1024).contains(&self.max_response_bytes)
            || self.max_duration.is_zero()
            || self.max_duration > Duration::from_secs(300)
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Repository discovery requires explicit bounded pages/fixed size/raw rows/documents/metadata/whole captures/deadline"));
        }
        Ok(())
    }
}

/// EACH missing/null flag is Unknown, not the legacy Permissions defaultfalse.
#[derive(Clone, Debug, PartialEq, Eq, Default, Serialize)]
pub struct RepositoryCataloguePermissions {
    pub pull: Option<bool>,
    pub push: Option<bool>,
    pub maintain: Option<bool>,
    pub admin: Option<bool>,
    pub triage: Option<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum RepositoryCatalogueOwnerKind {
    Organization,
    User,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RepositoryCatalogueOwner {
    pub id: u64,
    pub node_id: Option<String>,
    pub login: String,
    pub kind: RepositoryCatalogueOwnerKind,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RepositoryCatalogueActor {
    pub database_id: Option<u64>,
    pub node_id: Option<String>,
    pub actor: Actor,
}
/// Metadata observations may be persisted, but are not permission or live
/// access receipts. Only the private factory Read owns their observed membership.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RepositoryCatalogueRepository {
    pub id: u64,
    pub node_id: Option<String>,
    pub full_name: String,
    pub description: Option<String>,
    pub owner: Option<RepositoryCatalogueActor>,
    pub default_branch: Option<String>,
    pub private: Option<bool>,
    pub fork: Option<bool>,
    pub archived: Option<bool>,
    pub language: Option<String>,
    pub permissions: RepositoryCataloguePermissions,
    pub updated_at: Option<String>,
    pub metadata_limitations: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RepositoryCatalogueCoverage {
    /// Exhaustion of this current API-visible collection, not an atomic snapshot
    /// or proof every organization repository exists/is visible to this token.
    pub enumeration_exhausted: bool,
    pub membership_complete: bool,
    pub complete_empty: bool,
    pub requested_pages: usize,
    pub fetched_pages: usize,
    pub raw_rows: usize,
    pub admitted_rows: usize,
    pub duplicate_rows: usize,
    pub excluded_rows: usize,
    pub failure: Option<GithubErrorKind>,
    pub reasons: Vec<String>,
}
impl RepositoryCatalogueCoverage {
    pub(super) fn reason(&mut self, reason: &str) {
        if self.reasons.len() < 128 && !self.reasons.iter().any(|existing| existing == reason) {
            self.reasons.push(reason.into());
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct RepositoryCatalogueAccounting {
    pub(super) requests: usize,
    pub(super) stdout_bytes: usize,
    pub(super) diagnostic_bytes: usize,
    pub(super) failed_reserved_bytes: usize,
}
impl RepositoryCatalogueAccounting {
    pub fn requests(&self) -> usize {
        self.requests
    }
    pub fn successful_stdout_bytes(&self) -> usize {
        self.stdout_bytes
    }
    pub fn successful_diagnostic_bytes(&self) -> usize {
        self.diagnostic_bytes
    }
    pub fn failed_reserved_bytes(&self) -> usize {
        self.failed_reserved_bytes
    }
    pub fn charged_bytes(&self) -> usize {
        self.stdout_bytes
            .saturating_add(self.diagnostic_bytes)
            .saturating_add(self.failed_reserved_bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepositoryCataloguePhase {
    BeforeEnumeration,
    BeforeFinalProof,
    AfterFinalProof,
}
/// Constructor is private to the actual gh factory. Getters expose original
/// facts, not authority to serialize/reconstruct a successful scoped read.
pub struct RepositoryCatalogueContext {
    pub(super) identity: HostIdentity,
    pub(super) source: RepositoryCatalogueSource,
    pub(super) owner: Option<RepositoryCatalogueOwner>,
}
impl std::fmt::Debug for RepositoryCatalogueContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RepositoryCatalogueContext(<original factory facts>)")
    }
}
impl RepositoryCatalogueContext {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn source(&self) -> &RepositoryCatalogueSource {
        &self.source
    }
    pub fn owner(&self) -> Option<&RepositoryCatalogueOwner> {
        self.owner.as_ref()
    }
}
/// No public constructor, Clone or serde: cached/public metadata cannot mint
/// this current original-source read. Root still must enforce live SQL/epoch,
/// cancellation and principal/access invalidation before its native publication.
pub struct RepositoryCatalogueRead {
    pub(super) context: RepositoryCatalogueContext,
    pub(super) rows: Arc<[RepositoryCatalogueRepository]>,
    pub(super) coverage: RepositoryCatalogueCoverage,
    pub(super) limits: RepositoryCatalogueLimits,
    pub(super) accounting: RepositoryCatalogueAccounting,
    pub(super) observed_from: u64,
    pub(super) observed_through: u64,
}
impl std::fmt::Debug for RepositoryCatalogueRead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RepositoryCatalogueRead(<bounded original observed membership>)")
    }
}
impl RepositoryCatalogueRead {
    /// W9 allocation accounting only; borrowed traversal, no source/DTO copy.
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        let mut n = std::mem::size_of::<Self>()
            .checked_add(4 * std::mem::size_of::<usize>())?
            .checked_add(
                self.rows
                    .len()
                    .checked_mul(std::mem::size_of::<RepositoryCatalogueRepository>())?,
            )?;
        for s in [
            &self.context.identity.host,
            &self.context.identity.login,
            &self.context.identity.gh_version,
        ] {
            n = n.checked_add(s.capacity())?;
        }
        match &self.context.source {
            RepositoryCatalogueSource::Organization(s)
            | RepositoryCatalogueSource::PublicUser(s) => n = n.checked_add(s.capacity())?,
            _ => {}
        }
        if let Some(o) = &self.context.owner {
            n = n
                .checked_add(o.login.capacity())?
                .checked_add(o.node_id.as_ref().map_or(0, String::capacity))?;
        }
        n = n.checked_add(
            self.coverage
                .reasons
                .capacity()
                .checked_mul(std::mem::size_of::<String>())?,
        )?;
        for s in &self.coverage.reasons {
            n = n.checked_add(s.capacity())?;
        }
        for r in self.rows.iter() {
            n = n.checked_add(r.full_name.capacity())?;
            for s in [
                &r.node_id,
                &r.description,
                &r.default_branch,
                &r.language,
                &r.updated_at,
            ] {
                n = n.checked_add(s.as_ref().map_or(0, String::capacity))?;
            }
            n = n.checked_add(
                r.metadata_limitations
                    .capacity()
                    .checked_mul(std::mem::size_of::<String>())?,
            )?;
            for s in &r.metadata_limitations {
                n = n.checked_add(s.capacity())?;
            }
            if let Some(o) = &r.owner {
                n = n
                    .checked_add(o.node_id.as_ref().map_or(0, String::capacity))?
                    .checked_add(o.actor.login.capacity())?
                    .checked_add(o.actor.actor_type.capacity())?
                    .checked_add(o.actor.display_name.as_ref().map_or(0, String::capacity))?
                    .checked_add(o.actor.avatar_url.as_ref().map_or(0, String::capacity))?;
            }
        }
        Some(n)
    }
    pub fn context(&self) -> &RepositoryCatalogueContext {
        &self.context
    }
    pub fn identity(&self) -> &HostIdentity {
        self.context.identity()
    }
    pub fn source(&self) -> &RepositoryCatalogueSource {
        self.context.source()
    }
    pub fn owner(&self) -> Option<&RepositoryCatalogueOwner> {
        self.context.owner()
    }
    pub fn repositories(&self) -> &[RepositoryCatalogueRepository] {
        &self.rows
    }
    pub fn coverage(&self) -> &RepositoryCatalogueCoverage {
        &self.coverage
    }
    pub fn limits(&self) -> &RepositoryCatalogueLimits {
        &self.limits
    }
    pub fn accounting(&self) -> &RepositoryCatalogueAccounting {
        &self.accounting
    }
    pub fn observed_interval(&self) -> (u64, u64) {
        (self.observed_from, self.observed_through)
    }
    pub fn source_limitations(&self) -> &'static [&'static str] {
        &["Current API-visible repositories only; token/SSO policy may hide repositories", "Non-atomic paged traversal; new/deleted/renamed resources may race this read", "Inventory absence is not individual repository revocation; Open rechecks actual identity/access", "Nullable metadata/observed permissions are not current mutation authority"]
    }
}
