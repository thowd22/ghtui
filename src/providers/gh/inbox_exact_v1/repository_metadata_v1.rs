//! Optional metadata contradictions, NOT authentication/permission.
//! Entries come only from an ACTUAL opaque pending current-repository read.
use super::super::{
    error, validate_repo, CurrentCatalogueRepository, GithubErrorKind, GithubResult, HostIdentity,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const MAX_REPOSITORIES: usize = 64;
const MAX_TEXT_BYTES: usize = 128 * 1024;
struct ExpectedRepository {
    database_id: u64,
    node_id: Option<String>,
}
/// Small owned consistency constraints. No serde/raw-JSON/Repository DTO
/// constructor, no Clone of opaque producer rows and NO access/read authority.
pub(crate) struct ExactInboxRepositoryMetadataV1 {
    host: String,
    account_id: u64,
    repositories: BTreeMap<String, ExpectedRepository>,
    text_bytes: usize,
}
impl std::fmt::Debug for ExactInboxRepositoryMetadataV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExactInboxRepositoryMetadataV1")
            .field("observed_repositories", &self.repositories.len())
            .finish_non_exhaustive()
    }
}
impl ExactInboxRepositoryMetadataV1 {
    pub(crate) fn from_current(
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
    ) -> GithubResult<Self> {
        let account_id = identity.account_id.filter(|id| *id > 0).ok_or_else(|| {
            error(
                GithubErrorKind::IdentityChanged,
                "Repository metadata needs original positive principal context",
            )
        })?;
        if identity.host.is_empty()
            || identity.host.len() > 253
            || identity.host.starts_with('-')
            || identity
                .host
                .bytes()
                .any(|byte| !byte.is_ascii_alphanumeric() && !b".-".contains(&byte))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Repository metadata host context is unavailable",
            ));
        }
        let mut result = Self {
            host: identity.host.clone(),
            account_id,
            repositories: BTreeMap::new(),
            text_bytes: 0,
        };
        result.capture_current(current)?;
        Ok(result)
    }
    pub(crate) fn capture_current(
        &mut self,
        current: &CurrentCatalogueRepository,
    ) -> GithubResult<()> {
        let repository = current.repository();
        validate_repo(&repository.full_name)?;
        if repository.id == 0
            || repository.full_name.len() > 256
            || repository
                .node_id
                .as_ref()
                .is_some_and(|node| !valid_node(node))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Observed repository metadata fields are invalid/bounded",
            ));
        }
        let name = repository.full_name.to_ascii_lowercase();
        if let Some(previous) = self.repositories.get(&name) {
            if previous.database_id != repository.id || previous.node_id != repository.node_id {
                return Err(changed());
            }
            return Ok(());
        }
        if self.repositories.len() == MAX_REPOSITORIES
            || self.repositories.values().any(|row| {
                row.database_id == repository.id
                    || row
                        .node_id
                        .as_ref()
                        .is_some_and(|node| repository.node_id.as_ref() == Some(node))
            })
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Observed repository metadata repeats or exceeds the exact scope; no alias guess",
            ));
        }
        let bytes = name
            .len()
            .checked_add(repository.node_id.as_deref().map_or(0, str::len))
            .and_then(|added| self.text_bytes.checked_add(added))
            .filter(|bytes| *bytes <= MAX_TEXT_BYTES)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Observed repository metadata copy budget exceeded",
                )
            })?;
        self.repositories.insert(
            name,
            ExpectedRepository {
                database_id: repository.id,
                node_id: repository.node_id.clone(),
            },
        );
        self.text_bytes = bytes;
        Ok(())
    }
    /// Principal/set binding only; a successful comparison is NOT access.
    pub(crate) fn validate_scope(
        &self,
        identity: &HostIdentity,
        names: &[String],
    ) -> GithubResult<()> {
        if identity.host != self.host || identity.account_id != Some(self.account_id) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original repository metadata principal changed; no rebase",
            ));
        }
        if names.is_empty()
            || names.len() > MAX_REPOSITORIES
            || names.len() != self.repositories.len()
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Repository metadata must match the complete exact queried set; no narrowing",
            ));
        }
        let mut seen = BTreeSet::new();
        for name in names {
            validate_repo(name)?;
            if name.len() > 256
                || !self.repositories.contains_key(&name.to_ascii_lowercase())
                || !seen.insert(name.to_ascii_lowercase())
            {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Repository metadata canonical query set changed; no name-only fallback",
                ));
            }
        }
        Ok(())
    }
    /// Search Issue ID/node is NEVER compared here. Optional repository facts
    /// cannot add authority; missing fields are unknown, not observed absence.
    pub(crate) fn validate_optional_repository(&self, name: &str, raw: &Value) -> GithubResult<()> {
        let expected = self
            .repositories
            .get(&name.to_ascii_lowercase())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "No actual current repository metadata matches the exact query",
                )
            })?;
        let Some(repository) = raw.get("repository").filter(|value| !value.is_null()) else {
            return Ok(());
        };
        if !repository.is_object() {
            return Err(error(
                GithubErrorKind::Json,
                "Optional search repository metadata is malformed",
            ));
        }
        if let Some(value) = repository.get("id").filter(|value| !value.is_null()) {
            let database_id = value.as_u64().filter(|id| *id > 0).ok_or_else(|| {
                error(
                    GithubErrorKind::Json,
                    "Optional search repository ID is malformed",
                )
            })?;
            if database_id != expected.database_id {
                return Err(changed());
            }
        }
        if let Some(value) = repository.get("node_id").filter(|value| !value.is_null()) {
            let node = value
                .as_str()
                .filter(|node| valid_node(node))
                .ok_or_else(|| {
                    error(
                        GithubErrorKind::Json,
                        "Optional search repository node is malformed",
                    )
                })?;
            if expected
                .node_id
                .as_deref()
                .is_some_and(|original| original != node)
            {
                return Err(changed());
            }
            // Original unknown node remains unknown; never derive it from raw
            // Issue/search/repository fields or treat this as an access receipt.
        }
        Ok(())
    }
}
fn valid_node(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn changed() -> super::super::GithubError {
    error(GithubErrorKind::IdentityChanged,"Supplied search repository identity contradicts its actual current repository read; no private queue admitted")
}

#[cfg(test)]
#[path = "repository_metadata_v1_tests.rs"]
mod tests;
