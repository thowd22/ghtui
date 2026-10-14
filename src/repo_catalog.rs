//! SOURCE-ONLY catalogue projection; not exported or wired to App/providers yet.
//! Account/canonical-ID intent is separate from disposable inventory. The worker
//! caller owns fresh gh principal proofs, bounded transport and atomic storage.
use crate::{
    config::validate_repository,
    providers::gh::{Actor, Permissions},
    storage::Scope,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogSource {
    Organization(String),
    AuthenticatedAccessible,
    PublicUser(String),
}
impl CatalogSource {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Organization(owner) | Self::PublicUser(owner) => validate_owner(owner),
            Self::AuthenticatedAccessible => Ok(()),
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Organization(owner) => format!("Organization {owner}: accessible repositories"),
            Self::AuthenticatedAccessible => {
                "Current account: accessible owned/member/collaborator repositories".into()
            }
            Self::PublicUser(owner) => format!("User {owner}: public repositories only"),
        }
    }
}

/// Additive nullable catalogue metadata; no changes to shared Repository literals.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogRepository {
    pub id: u64,
    pub node_id: Option<String>,
    pub full_name: String,
    pub description: String,
    pub owner: Option<Actor>,
    pub default_branch: Option<String>,
    pub private: Option<bool>,
    pub fork: Option<bool>,
    pub archived: Option<bool>,
    pub language: Option<String>,
    pub permissions: Option<Permissions>,
    pub updated_at: Option<String>,
    pub fetched_at: u64,
}
impl CatalogRepository {
    pub fn validate(&self, source: &CatalogSource) -> Result<(), String> {
        validate_repository(&self.full_name).map_err(|error| error.to_string())?;
        if self.id == 0 || self.description.len() > 64 * 1024 {
            return Err(
                "Catalogue repository needs a positive canonical ID and bounded metadata".into(),
            );
        }
        if self.node_id.as_ref().is_some_and(|node| {
            node.is_empty() || node.len() > 1024 || node.chars().any(char::is_control)
        }) {
            return Err("Catalogue node identity is malformed".into());
        }
        let source_owner = match source {
            CatalogSource::Organization(owner) | CatalogSource::PublicUser(owner) => Some(owner),
            CatalogSource::AuthenticatedAccessible => None,
        };
        let owner = self.full_name.split('/').next().unwrap_or_default();
        if source_owner.is_some_and(|expected| !expected.eq_ignore_ascii_case(owner)) {
            return Err("Catalogue repository belongs to a different requested owner".into());
        }
        if matches!(source, CatalogSource::PublicUser(_)) && self.private == Some(true) {
            return Err("Public-user inventory cannot claim private repository coverage".into());
        }
        if self.owner.as_ref().is_some_and(|actor| {
            !actor.login.is_empty() && !actor.login.eq_ignore_ascii_case(owner)
        }) {
            return Err("Catalogue owner actor disagrees with canonical repository owner".into());
        }
        if self.default_branch.as_ref().is_some_and(|branch| {
            branch.is_empty() || branch.len() > 1024 || branch.chars().any(char::is_control)
        }) || self
            .language
            .as_ref()
            .is_some_and(|language| language.len() > 1024 || language.chars().any(char::is_control))
            || self
                .updated_at
                .as_ref()
                .is_some_and(|time| time.len() > 1024)
        {
            return Err(
                "Catalogue optional fields exceed their honest bounded representation".into(),
            );
        }
        Ok(())
    }
    pub fn labels(&self) -> String {
        let flag = |name: &str, value: Option<bool>| match value {
            Some(true) => format!("{name}:yes"),
            Some(false) => format!("{name}:no"),
            None => format!("{name}:unknown"),
        };
        format!(
            "{} • {} • {} • language:{} • branch:{} • read:{} • {}",
            flag("private", self.private),
            flag("fork", self.fork),
            flag("archived", self.archived),
            self.language.as_deref().unwrap_or("unknown"),
            self.default_branch.as_deref().unwrap_or("unknown"),
            self.permissions
                .as_ref()
                .map_or("unknown", |permission| if permission.pull {
                    "allowed at fetch"
                } else {
                    "denied at fetch"
                }),
            self.owner
                .as_ref()
                .map_or_else(|| "unknown owner".into(), Actor::label)
        )
    }
}
#[derive(Debug, Clone)]
pub struct CatalogSnapshot {
    pub scope: Scope,
    pub source: CatalogSource,
    /// The caller's actual reserved publication generation, never fetched_at.
    pub generation: u64,
    pub repositories: Vec<CatalogRepository>,
    pub loaded_pages: usize,
    pub max_pages: usize,
    pub max_repositories: usize,
    pub complete: bool,
    pub cancelled: bool,
    pub reasons: Vec<String>,
}
impl CatalogSnapshot {
    pub fn validate(&self) -> Result<(), String> {
        positive_scope(&self.scope)?;
        self.source.validate()?;
        if self.generation == 0
            || !(1..=100_000).contains(&self.max_repositories)
            || !(1..=1000).contains(&self.max_pages)
            || self.loaded_pages > self.max_pages
            || self.complete && self.loaded_pages == 0
            || self.repositories.len() > self.max_repositories
            || self.reasons.len() > 1000
            || self
                .reasons
                .iter()
                .any(|reason| reason.is_empty() || reason.len() > 16 * 1024)
            || self.complete && (self.cancelled || !self.reasons.is_empty())
            || !self.repositories.is_empty() && self.loaded_pages == 0
            || self.repositories.len() > self.loaded_pages.saturating_mul(100)
        {
            return Err("Catalogue snapshot needs a positive generation, bounded actual progress and honest completeness".into());
        }
        if !self.complete && self.reasons.is_empty() && !self.cancelled {
            return Err("Partial catalogue needs an explicit reason; absence is not zero".into());
        }
        for repository in &self.repositories {
            repository.validate(&self.source)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryReference {
    pub id: u64,
    pub last_name: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExplicitMapping {
    pub repository_id: u64,
    pub directory: PathBuf,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogIntent {
    pub version: u32,
    pub scope: Scope,
    pub pins: BTreeMap<u64, RepositoryReference>,
    pub sets: BTreeMap<String, BTreeMap<u64, RepositoryReference>>,
    pub mappings: BTreeMap<u64, ExplicitMapping>,
}
impl CatalogIntent {
    pub fn new(scope: Scope) -> Result<Self, String> {
        positive_scope(&scope)?;
        Ok(Self {
            version: 1,
            scope,
            pins: BTreeMap::new(),
            sets: BTreeMap::new(),
            mappings: BTreeMap::new(),
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        positive_scope(&self.scope)?;
        if self.version != 1
            || self.pins.len() > 100_000
            || self.sets.len() > 1000
            || self.mappings.len() > 100_000
        {
            return Err("Catalogue intent exceeds its supported schema/budget".into());
        }
        if self.sets.values().fold(
            self.pins.len().saturating_add(self.mappings.len()),
            |total, members| total.saturating_add(members.len()),
        ) > 100_000
        {
            return Err("Catalogue durable intent exceeds its combined reference budget".into());
        }
        for (name, members) in &self.sets {
            if name.is_empty()
                || name.len() > 128
                || name.chars().any(char::is_control)
                || members.len() > 100_000
            {
                return Err("Catalogue set needs a bounded explicit name/membership".into());
            }
        }
        for (id, reference) in self
            .pins
            .iter()
            .chain(self.sets.values().flat_map(|members| members.iter()))
        {
            if *id == 0 || reference.id != *id {
                return Err("Catalogue reference must retain its canonical repository ID".into());
            }
            validate_repository(&reference.last_name).map_err(|error| error.to_string())?;
        }
        for (id, mapping) in &self.mappings {
            if *id == 0 || mapping.repository_id != *id || !mapping.directory.is_absolute() {
                return Err(
                    "Local mapping must be explicit, absolute and canonical-ID-bound".into(),
                );
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone)]
pub struct CatalogRow {
    pub repository: CatalogRepository,
    pub current: bool,
}
/// Built outside draw after a snapshot/filter/intent change. Rows retain only
/// bounded presentation metadata; unseen saved references stay explicitly absent.
#[derive(Debug, Clone)]
pub struct CatalogPreparedRow {
    pub id: u64,
    pub name: String,
    pub labels: String,
    pub current: bool,
    pub selected: bool,
    pub pinned: bool,
    pub default_branch: Option<String>,
    pub mapping: Option<PathBuf>,
}
#[derive(Debug, Clone)]
pub struct CatalogState {
    pub scope: Scope,
    pub source: CatalogSource,
    pub generation: u64,
    pub rows: BTreeMap<u64, CatalogRow>,
    pub selected: BTreeSet<u64>,
    pub intent: CatalogIntent,
    pub complete: bool,
    pub loaded_pages: usize,
    pub reasons: Vec<String>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CatalogFilter {
    All,
    Archived,
    NotArchived,
    Forks,
    NotForks,
    Pinned,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogDestination {
    PullRequests,
    Actions,
    RemoteTree { default_branch: String },
    LocalRepository { directory: PathBuf },
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogRoute {
    pub scope: Scope,
    pub repository_id: u64,
    pub repository: String,
    pub generation: u64,
    pub destination: CatalogDestination,
}
impl CatalogState {
    pub fn new(
        scope: Scope,
        source: CatalogSource,
        intent: Option<CatalogIntent>,
    ) -> Result<Self, String> {
        positive_scope(&scope)?;
        source.validate()?;
        let intent = intent.unwrap_or(CatalogIntent::new(scope.clone())?);
        intent.validate()?;
        if intent.scope != scope {
            return Err("Catalogue intent belongs to another numeric account/host".into());
        }
        Ok(Self {
            scope,
            source,
            generation: 0,
            rows: BTreeMap::new(),
            selected: BTreeSet::new(),
            intent,
            complete: false,
            loaded_pages: 0,
            reasons: vec!["Inventory has not been loaded; no global coverage claimed".into()],
        })
    }
    pub fn publish(&mut self, snapshot: CatalogSnapshot) -> Result<(), String> {
        snapshot.validate()?;
        if snapshot.scope != self.scope
            || snapshot.source != self.source
            || snapshot.generation <= self.generation
        {
            return Err(
                "Catalogue completion belongs to another scope/source or stale generation".into(),
            );
        }
        if snapshot.cancelled {
            return Err("Cancelled catalogue completion cannot publish late".into());
        }
        let mut admitted = BTreeMap::new();
        for repository in snapshot.repositories {
            if let Some(previous) = admitted.insert(repository.id, repository.clone()) {
                if previous != repository {
                    return Err("Conflicting duplicate canonical repository observations; refresh the moving inventory".into());
                }
            }
        }
        let mut rows = if snapshot.complete {
            BTreeMap::new()
        } else {
            self.rows.clone()
        };
        for row in rows.values_mut() {
            row.current = false;
        }
        for (id, repository) in admitted {
            rows.insert(
                id,
                CatalogRow {
                    repository,
                    current: true,
                },
            );
        }
        if rows.len() > 100_000 {
            return Err("Catalogue partial retention reached its combined row budget; narrow the source before retry".into());
        }
        self.rows = rows;
        self.generation = snapshot.generation;
        self.complete = snapshot.complete;
        self.loaded_pages = snapshot.loaded_pages;
        self.reasons = snapshot.reasons;
        Ok(())
    }
    pub fn filtered(&self, filter: CatalogFilter) -> Vec<&CatalogRow> {
        let mut rows = self
            .rows
            .values()
            .filter(|row| match filter {
                CatalogFilter::All => true,
                CatalogFilter::Archived => row.repository.archived == Some(true),
                CatalogFilter::NotArchived => row.repository.archived == Some(false),
                CatalogFilter::Forks => row.repository.fork == Some(true),
                CatalogFilter::NotForks => row.repository.fork == Some(false),
                CatalogFilter::Pinned => self.intent.pins.contains_key(&row.repository.id),
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            (&left.repository.full_name, left.repository.id)
                .cmp(&(&right.repository.full_name, right.repository.id))
        });
        rows
    }
    pub fn toggle_selected(&mut self, id: u64) -> Result<(), String> {
        if self.selected.remove(&id) {
            return Ok(());
        }
        self.current(id)?;
        self.selected.insert(id);
        Ok(())
    }
    pub fn toggle_pin(&mut self, id: u64) -> Result<(), String> {
        if self.intent.pins.remove(&id).is_some() {
            return Ok(());
        }
        let repository = self.current(id)?.repository.clone();
        self.intent.pins.insert(
            id,
            RepositoryReference {
                id,
                last_name: repository.full_name,
            },
        );
        Ok(())
    }
    pub fn save_set(&mut self, name: &str) -> Result<(), String> {
        if self.selected.is_empty() {
            return Err(
                "Choose explicit current repositories; an empty set cannot mean all repositories"
                    .into(),
            );
        }
        let mut members = BTreeMap::new();
        for id in &self.selected {
            members.insert(
                *id,
                RepositoryReference {
                    id: *id,
                    last_name: self.current(*id)?.repository.full_name.clone(),
                },
            );
        }
        let mut next = self.intent.clone();
        next.sets.insert(name.into(), members);
        next.validate()?;
        self.intent = next;
        Ok(())
    }
    pub fn resolve_set(&self, name: &str) -> Result<Vec<&CatalogRepository>, String> {
        let set = self
            .intent
            .sets
            .get(name)
            .ok_or("Catalogue set does not exist")?;
        if set.is_empty() {
            return Err("Empty saved membership is not an all-repositories scope".into());
        }
        set.keys()
            .map(|id| self.current(*id).map(|row| &row.repository))
            .collect()
    }
    pub fn map_local(&mut self, id: u64, directory: PathBuf) -> Result<(), String> {
        self.current(id)?;
        let mut next = self.intent.clone();
        next.mappings.insert(
            id,
            ExplicitMapping {
                repository_id: id,
                directory,
            },
        );
        next.validate()?;
        self.intent = next;
        Ok(())
    }
    /// This is a typed navigation plan, NOT authorization. Root verifies same
    /// numeric account and actual repository ID before gh, and Git for local open.
    pub fn route(&self, id: u64, destination: CatalogDestination) -> Result<CatalogRoute, String> {
        let repository = &self.current(id)?.repository;
        match &destination {
            CatalogDestination::RemoteTree { default_branch }
                if repository.default_branch.as_ref() != Some(default_branch) =>
            {
                return Err("No guessed default branch or moved repository target".into())
            }
            CatalogDestination::LocalRepository { directory }
                if self
                    .intent
                    .mappings
                    .get(&id)
                    .is_none_or(|mapping| &mapping.directory != directory) =>
            {
                return Err("No implicit local mapping, clone or checkout".into())
            }
            _ => {}
        }
        Ok(CatalogRoute {
            scope: self.scope.clone(),
            repository_id: id,
            repository: repository.full_name.clone(),
            generation: self.generation,
            destination,
        })
    }
    pub fn validate_route(&self, route: &CatalogRoute) -> Result<(), String> {
        if route.scope != self.scope
            || route.generation != self.generation
            || self.route(route.repository_id, route.destination.clone())? != *route
        {
            return Err(
                "Catalogue navigation target became stale; choose the current canonical row".into(),
            );
        }
        Ok(())
    }
    pub fn prepare_rows(&self, filter: CatalogFilter) -> Vec<CatalogPreparedRow> {
        let mut prepared = self
            .filtered(filter)
            .into_iter()
            .map(|row| {
                let id = row.repository.id;
                CatalogPreparedRow {
                    id,
                    name: row.repository.full_name.clone(),
                    labels: row.repository.labels(),
                    current: row.current,
                    selected: self.selected.contains(&id),
                    pinned: self.intent.pins.contains_key(&id),
                    default_branch: row.repository.default_branch.clone(),
                    mapping: self
                        .intent
                        .mappings
                        .get(&id)
                        .map(|mapping| mapping.directory.clone()),
                }
            })
            .collect::<Vec<_>>();
        if matches!(filter, CatalogFilter::All | CatalogFilter::Pinned) {
            let mut absent = BTreeMap::new();
            for (id, reference) in self
                .intent
                .pins
                .iter()
                .chain(self.intent.sets.values().flat_map(|members| members.iter()))
            {
                if !self.rows.contains_key(id)
                    && (filter == CatalogFilter::All || self.intent.pins.contains_key(id))
                {
                    absent.entry(*id).or_insert(reference);
                }
            }
            for (id, reference) in absent {
                prepared.push(CatalogPreparedRow {id,name:reference.last_name.clone(),labels:"Saved canonical reference is absent in this current inventory; refresh original source, never retarget by name".into(),current:false,selected:self.selected.contains(&id),pinned:self.intent.pins.contains_key(&id),default_branch:None,mapping:self.intent.mappings.get(&id).map(|mapping|mapping.directory.clone())});
            }
        }
        prepared.sort_by(|left, right| (&left.name, left.id).cmp(&(&right.name, right.id)));
        prepared
    }
    fn current(&self, id: u64) -> Result<&CatalogRow, String> {
        self.rows.get(&id).filter(|row|row.current).ok_or_else(||"Repository is absent/stale in the current scoped inventory; refresh without retargeting by name".into())
    }
}
fn validate_owner(owner: &str) -> Result<(), String> {
    if owner.is_empty()
        || owner.len() > 39
        || owner.starts_with('-')
        || owner.ends_with('-')
        || !owner
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("Choose an explicit valid GitHub organization/user owner".into());
    }
    Ok(())
}
fn positive_scope(scope: &Scope) -> Result<(), String> {
    let id = scope
        .login
        .strip_prefix("user-id:")
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|id| *id > 0)
        .ok_or("Catalogue private intent requires a verified positive numeric account")?;
    if Scope::github(scope.host.to_ascii_lowercase(), Some(id))
        .map_err(|error| error.to_string())?
        != *scope
    {
        return Err("Catalogue scope must be canonical host/positive numeric account".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
pub mod ui;
