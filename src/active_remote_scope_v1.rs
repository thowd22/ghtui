//! SOURCE-ONLY navigation intent, NOT access, permission or a durable receipt.
//! Empty/unresolved/fenced selections must never become a default personal query.
use crate::{
    providers::gh::{HostIdentity, RepositoryCatalogueRead, RepositoryCatalogueRepository},
    repository_catalogue_intent_v1::{CatalogueIntentV1, IntentReference},
    storage::Scope,
};
use std::collections::{BTreeMap, BTreeSet};

pub const REMOTE_SCOPE_MAX_REFERENCES: usize = 4096;
/// Copied reference text, not a claim about Rust struct/map allocation overhead.
pub const REMOTE_SCOPE_MAX_TEXT_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteScopeFenceV1 {
    PrincipalUnavailable,
    PrincipalChanged,
    PrivacyFence,
    SourceUnavailable,
    AccessUnavailable,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteScopeErrorV1 {
    InvalidPrincipal,
    SelectionLimit,
    DuplicateSelection,
    SelectedOriginalUnavailable,
    SavedSetUnavailable,
    OriginalSourceChanged,
    CopiedTextLimit,
}
impl std::fmt::Display for RemoteScopeErrorV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidPrincipal => "Original remote scope needs a positive numeric principal",
            Self::SelectionLimit => "Explicit remote scope exceeds its reference limit",
            Self::DuplicateSelection => "Explicit remote scope repeats an original repository ID",
            Self::SelectedOriginalUnavailable => "Selected original repository was not observed",
            Self::SavedSetUnavailable => "Explicit saved set is unavailable; no default fallback",
            Self::OriginalSourceChanged => {
                "Original saved scope/source differs from this observation"
            }
            Self::CopiedTextLimit => {
                "Explicit remote scope exceeds its copied reference-text limit"
            }
        })
    }
}
impl std::error::Error for RemoteScopeErrorV1 {}

/// Reuses the catalogue's genuine DB/node/name reference type. Unresolved
/// originals remain original; name equality cannot manufacture an observation.
#[derive(Clone, PartialEq, Eq)]
pub struct RemoteRepositoryIntentV1 {
    original: IntentReference,
    observed: Option<IntentReference>,
}
impl RemoteRepositoryIntentV1 {
    pub fn original(&self) -> &IntentReference {
        &self.original
    }
    pub fn observed(&self) -> Option<&IntentReference> {
        self.observed.as_ref()
    }
}
impl std::fmt::Debug for RemoteRepositoryIntentV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteRepositoryIntentV1")
            .field("observed", &self.observed.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Clone)]
enum State {
    DefaultPersonal,
    NotificationPr(std::sync::Arc<crate::providers::gh::NotificationPrSourceV63>),
    Explicit(Vec<RemoteRepositoryIntentV1>),
    Fenced(RemoteScopeFenceV1),
}
/// No serde or raw/public-JSON constructor. Stored sets are NOT activated by
/// loading them; the explicit activation factory must be called by an action.
#[derive(Clone)]
pub struct ActiveRemoteScopeV1 {
    original_scope: Option<Scope>,
    state: State,
}
impl std::fmt::Debug for ActiveRemoteScopeV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match &self.state {
            State::DefaultPersonal => "ActiveRemoteScopeV1(DefaultPersonal)",
            State::NotificationPr(_) => "ActiveRemoteScopeV1(NotificationPr original allocation)",
            State::Explicit(_) => "ActiveRemoteScopeV1(Explicit original references)",
            State::Fenced(_) => "ActiveRemoteScopeV1(Fenced)",
        })
    }
}

/// A borrowed, nonempty set of observations, still navigation intent only.
/// Every consumer MUST perform its own fresh exact repository-access admission.
pub struct ExplicitRemoteRepositoriesV1<'a> {
    references: &'a [RemoteRepositoryIntentV1],
}
impl<'a> ExplicitRemoteRepositoriesV1<'a> {
    pub fn references(&self) -> &'a [RemoteRepositoryIntentV1] {
        self.references
    }
}
pub enum RemoteScopeConsumerV1<'a> {
    DefaultPersonal,
    NotificationPr(NotificationPrScopeV63<'a>),
    Explicit(ExplicitRemoteRepositoriesV1<'a>),
    ExplicitEmpty,
    /// Refuse the whole requested set rather than silently narrowing/broadening.
    Unresolved(&'a [RemoteRepositoryIntentV1]),
    Fenced(RemoteScopeFenceV1),
}
/// Opaque public borrowed view required by the existing public consumer ABI.
/// No constructor/serde/source capability escapes through the public surface.
pub struct NotificationPrScopeV63<'a> {
    source: &'a std::sync::Arc<crate::providers::gh::NotificationPrSourceV63>,
}
impl NotificationPrScopeV63<'_> {
    pub(crate) fn source(&self) -> &std::sync::Arc<crate::providers::gh::NotificationPrSourceV63> {
        self.source
    }
}
/// Stop-only intent inspection; this cannot authorize a live query.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteScopeStopV1 {
    ExplicitEmpty,
    Unresolved,
    Fenced(RemoteScopeFenceV1),
}

impl ActiveRemoteScopeV1 {
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        let mut n = std::mem::size_of::<Self>()
            .checked_add(2 * std::mem::size_of::<usize>())?
            .checked_add(
                self.original_scope
                    .as_ref()
                    .map_or(0, |s| s.host.capacity()),
            )?;
        n = n.checked_add(
            self.original_scope
                .as_ref()
                .map_or(0, |s| s.login.capacity()),
        )?;
        match &self.state {
            State::Explicit(rows) => {
                n = n.checked_add(
                    rows.capacity()
                        .checked_mul(std::mem::size_of::<RemoteRepositoryIntentV1>())?,
                )?;
                for row in rows {
                    for r in std::iter::once(&row.original).chain(row.observed.as_ref()) {
                        n = n
                            .checked_add(r.last_name.capacity())?
                            .checked_add(r.node_id.as_ref().map_or(0, String::capacity))?;
                    }
                }
            }
            // W9 has no Notification producer. Unknown retained graph refuses.
            State::NotificationPr(_) => return None,
            _ => {}
        }
        Some(n)
    }
    pub(crate) fn notification_pr_v63(
        source: std::sync::Arc<crate::providers::gh::NotificationPrSourceV63>,
    ) -> Self {
        Self {
            original_scope: Some(source.scope().clone()),
            state: State::NotificationPr(source),
        }
    }
    /// Authentication/setup may be unavailable before any original principal
    /// exists. This closed state cannot silently initialize a default query.
    pub fn unavailable(reason: RemoteScopeFenceV1) -> Self {
        Self {
            original_scope: None,
            state: State::Fenced(reason),
        }
    }
    /// Called only for deliberate default personal navigation, never as error
    /// recovery or as a result of reading a saved empty/unknown set.
    pub fn default_personal(original: &HostIdentity) -> Result<Self, RemoteScopeErrorV1> {
        Ok(Self {
            original_scope: Some(numeric_scope(original)?),
            state: State::DefaultPersonal,
        })
    }
    pub fn explicit_selection(
        read: &RepositoryCatalogueRead,
        ids: &[u64],
    ) -> Result<Self, RemoteScopeErrorV1> {
        if ids.len() > REMOTE_SCOPE_MAX_REFERENCES {
            return Err(RemoteScopeErrorV1::SelectionLimit);
        }
        let wanted = ids.iter().copied().collect::<BTreeSet<_>>();
        if wanted.len() != ids.len() {
            return Err(RemoteScopeErrorV1::DuplicateSelection);
        }
        let observed = selected_observations(read, &wanted);
        if wanted.contains(&0) || observed.len() != wanted.len() {
            return Err(RemoteScopeErrorV1::SelectedOriginalUnavailable);
        }
        let borrowed = observed.values().map(|repo| BorrowedReference {
            id: repo.id,
            node: repo.node_id.as_deref(),
            name: &repo.full_name,
            observed: Some(*repo),
        });
        Self::from_borrowed(numeric_scope(read.identity())?, borrowed)
    }
    pub fn explicitly_activate_saved_set(
        intent: &CatalogueIntentV1,
        read: &RepositoryCatalogueRead,
        exact_name: &str,
    ) -> Result<Self, RemoteScopeErrorV1> {
        intent
            .validate_original_read(read)
            .map_err(|_| RemoteScopeErrorV1::OriginalSourceChanged)?;
        let originals = intent
            .sets()
            .get(exact_name)
            .ok_or(RemoteScopeErrorV1::SavedSetUnavailable)?;
        if originals.len() > REMOTE_SCOPE_MAX_REFERENCES {
            return Err(RemoteScopeErrorV1::SelectionLimit);
        }
        let wanted = originals.keys().copied().collect::<BTreeSet<_>>();
        let observed = selected_observations(read, &wanted);
        let borrowed = originals.values().map(|original| BorrowedReference {
            id: original.id,
            node: original.node_id.as_deref(),
            name: &original.last_name,
            // Same policy as CatalogueIntentV1::resolve: genuine DB identity
            // and exact original node when present, NEVER a slug/node-prefix.
            observed: observed.get(&original.id).copied().filter(|current| {
                original
                    .node_id
                    .as_ref()
                    .is_none_or(|node| current.node_id.as_ref() == Some(node))
            }),
        });
        Self::from_borrowed(numeric_scope(read.identity())?, borrowed)
    }
    fn from_borrowed<'a>(
        scope: Scope,
        references: impl Iterator<Item = BorrowedReference<'a>>,
    ) -> Result<Self, RemoteScopeErrorV1> {
        // Bounded pointers/IDs first, owned raw text only after all preflights.
        let mut borrowed = Vec::new();
        let mut bytes = 0usize;
        for reference in references {
            if borrowed.len() == REMOTE_SCOPE_MAX_REFERENCES {
                return Err(RemoteScopeErrorV1::SelectionLimit);
            }
            let original_bytes = reference.name.len() + reference.node.map_or(0, str::len);
            let observed_bytes = reference.observed.map_or(0, |repo| {
                repo.full_name.len() + repo.node_id.as_deref().map_or(0, str::len)
            });
            bytes = bytes
                .checked_add(original_bytes)
                .and_then(|bytes| bytes.checked_add(observed_bytes))
                .filter(|bytes| *bytes <= REMOTE_SCOPE_MAX_TEXT_BYTES)
                .ok_or(RemoteScopeErrorV1::CopiedTextLimit)?;
            borrowed.push(reference);
        }
        let references = borrowed
            .into_iter()
            .map(|reference| RemoteRepositoryIntentV1 {
                original: IntentReference {
                    id: reference.id,
                    node_id: reference.node.map(str::to_owned),
                    last_name: reference.name.to_owned(),
                },
                observed: reference.observed.map(|repo| IntentReference {
                    id: repo.id,
                    node_id: repo.node_id.clone(),
                    last_name: repo.full_name.clone(),
                }),
            })
            .collect();
        Ok(Self {
            original_scope: Some(scope),
            state: State::Explicit(references),
        })
    }
    /// Clears retained private names/nodes; no privacy failure resets Default.
    pub fn fence(&mut self, reason: RemoteScopeFenceV1) {
        self.state = State::Fenced(reason);
    }
    /// Update navigation labels from a SEALED closing observation only. This
    /// never rewrites original saved references or authorizes subsequent reads.
    pub fn observe_closed_access(
        &mut self,
        original: &RepositoryCatalogueRead,
        closed: &crate::providers::gh::ClosedCatalogueAccess,
    ) -> Result<(), RemoteScopeErrorV1> {
        if self.original_scope.as_ref() != Some(&numeric_scope(original.identity())?)
            || self.original_scope.as_ref() != Some(&numeric_scope(closed.identity())?)
            || closed.original_source() != original.source()
            || closed.original_owner() != original.owner()
        {
            return Err(RemoteScopeErrorV1::OriginalSourceChanged);
        }
        let State::Explicit(references) = &mut self.state else {
            return Err(RemoteScopeErrorV1::OriginalSourceChanged);
        };
        let mut bytes = 0usize;
        for reference in references.iter() {
            let current = closed
                .observation(reference.original.id)
                .filter(|row| row.original() == &reference.original);
            bytes = bytes
                .checked_add(
                    reference.original.last_name.len()
                        + reference.original.node_id.as_deref().map_or(0, str::len),
                )
                .and_then(|bytes| {
                    bytes.checked_add(current.map_or(0, |row| {
                        row.repository().full_name.len()
                            + row.repository().node_id.as_deref().map_or(0, str::len)
                    }))
                })
                .filter(|bytes| *bytes <= REMOTE_SCOPE_MAX_TEXT_BYTES)
                .ok_or(RemoteScopeErrorV1::CopiedTextLimit)?;
        }
        for reference in references.iter_mut() {
            reference.observed = closed
                .observation(reference.original.id)
                .filter(|row| row.original() == &reference.original)
                .map(|row| IntentReference {
                    id: row.repository().id,
                    node_id: row.repository().node_id.clone(),
                    last_name: row.repository().full_name.clone(),
                });
        }
        Ok(())
    }
    /// The original host + numeric principal this intent was created for.
    pub(crate) fn original_scope(&self) -> Option<&Scope> {
        self.original_scope.as_ref()
    }
    /// Personal queues across all repositories: no repository narrowing intent.
    pub fn is_default_personal(&self) -> bool {
        matches!(self.state, State::DefaultPersonal)
    }
    pub fn blocked_intent(&self) -> Option<RemoteScopeStopV1> {
        match &self.state {
            State::Fenced(reason) => Some(RemoteScopeStopV1::Fenced(*reason)),
            State::Explicit(references) if references.is_empty() => {
                Some(RemoteScopeStopV1::ExplicitEmpty)
            }
            State::Explicit(references)
                if references.iter().any(|repo| repo.observed.is_none()) =>
            {
                Some(RemoteScopeStopV1::Unresolved)
            }
            State::DefaultPersonal | State::NotificationPr(_) | State::Explicit(_) => None,
        }
    }
    pub fn consumer(&self, current: &HostIdentity) -> RemoteScopeConsumerV1<'_> {
        if let State::Fenced(reason) = &self.state {
            return RemoteScopeConsumerV1::Fenced(*reason);
        }
        match numeric_scope(current) {
            Err(_) => {
                return RemoteScopeConsumerV1::Fenced(RemoteScopeFenceV1::PrincipalUnavailable)
            }
            Ok(scope) if self.original_scope.as_ref() != Some(&scope) => {
                return RemoteScopeConsumerV1::Fenced(RemoteScopeFenceV1::PrincipalChanged)
            }
            Ok(_) => {}
        }
        match &self.state {
            State::DefaultPersonal => RemoteScopeConsumerV1::DefaultPersonal,
            State::NotificationPr(source) => {
                RemoteScopeConsumerV1::NotificationPr(NotificationPrScopeV63 { source })
            }
            State::Explicit(references) if references.is_empty() => {
                RemoteScopeConsumerV1::ExplicitEmpty
            }
            State::Explicit(references)
                if references.iter().any(|repo| repo.observed.is_none()) =>
            {
                RemoteScopeConsumerV1::Unresolved(references)
            }
            State::Explicit(references) => {
                RemoteScopeConsumerV1::Explicit(ExplicitRemoteRepositoriesV1 { references })
            }
            State::Fenced(reason) => RemoteScopeConsumerV1::Fenced(*reason),
        }
    }
}
struct BorrowedReference<'a> {
    id: u64,
    node: Option<&'a str>,
    name: &'a str,
    observed: Option<&'a RepositoryCatalogueRepository>,
}
fn selected_observations<'a>(
    read: &'a RepositoryCatalogueRead,
    wanted: &BTreeSet<u64>,
) -> BTreeMap<u64, &'a RepositoryCatalogueRepository> {
    // Scan genuine bounded source once; never clone its entire inventory or do
    // O(requested members × inventory) resolution in a renderer/input handler.
    read.repositories()
        .iter()
        .filter(|repo| wanted.contains(&repo.id))
        .map(|repo| (repo.id, repo))
        .collect()
}
/// TASK-105 re-admission compares principals with the SAME normalization.
pub(crate) fn numeric_scope_v105(identity: &HostIdentity) -> Result<Scope, RemoteScopeErrorV1> {
    numeric_scope(identity)
}
fn numeric_scope(identity: &HostIdentity) -> Result<Scope, RemoteScopeErrorV1> {
    // Hostnames alone are ASCII-case-insensitive. Repository names are copied
    // exactly; opaque node bytes and set names are NEVER case-folded.
    if identity.host.is_empty()
        || identity.host.len() > 253
        || identity.host.starts_with('-')
        || identity
            .host
            .bytes()
            .any(|byte| !byte.is_ascii_alphanumeric() && !b".-".contains(&byte))
    {
        return Err(RemoteScopeErrorV1::InvalidPrincipal);
    }
    Scope::github(identity.host.to_ascii_lowercase(), identity.account_id)
        .map_err(|_| RemoteScopeErrorV1::InvalidPrincipal)
}

#[cfg(test)]
#[path = "active_remote_scope_v1_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "active_remote_scope_closed_v1_tests.rs"]
mod closed_tests;
