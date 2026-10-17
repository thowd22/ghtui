//! A closed lineage for the real pre-Catalogue startup queue. This is only
//! read intent; fresh Core/principal/query/closing proof still owns all access.
//! Empty(), cached(), saved State IDs and a missing Active never issue it.
use super::*;
use crate::storage::StateStartupReadV80;
use sha2::{Digest, Sha256};
use std::{sync::Arc, time::Instant};

pub(crate) struct LegacyStartupQueueV80 {
    digest: [u8; 32],
    startup: Option<StateStartupReadV80>,
}
/// Routing RESTRICTION only. No membership/access/ACK/write permission.
pub(super) struct StateSourceRouteHintV80 {
    digest: [u8; 32],
}
impl std::fmt::Debug for StateSourceRouteHintV80 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StateSourceRouteHintV80(restrict explicit read route)")
    }
}
impl std::fmt::Debug for LegacyStartupQueueV80 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LegacyStartupQueueV80(actual producer read intent)")
    }
}
fn bounded_source(g: &GithubWorkspace) -> Result<String> {
    if g.identity
        .account_id
        .is_none_or(|n| n == 0 || n > i64::MAX as u64)
        || g.identity.host.is_empty()
        || g.identity.host.len() > 253
        || g.identity.login.is_empty()
        || g.identity.login.len() > 100
        || g.identity.gh_version.len() > 256
        || !g.identity.same_account(&g.inbox.identity)
    {
        bail!("Actual startup numeric principal unavailable");
    }
    let fields = g
        .repositories
        .len()
        .checked_add(g.exclusions.len())
        .filter(|n| *n <= 512)
        .context("Startup source field limit")?;
    let text = g
        .repositories
        .iter()
        .chain(g.exclusions.iter())
        .try_fold(0usize, |n, s| n.checked_add(s.capacity()))
        .context("Startup source allocation overflow")?;
    text.checked_mul(24)
        .and_then(|n| n.checked_add(fields.checked_mul(1024)?))
        .and_then(|n| n.checked_add(4096))
        .filter(|n| *n <= 64 * 1024)
        .context("Startup source serialization exceeds existing bounded read intent")?;
    let source = g.source();
    if source.is_empty() || source.len() > 2048 || source.capacity() > 4096 {
        bail!("Startup source exceeds canonical source bound");
    }
    Ok(source)
}
fn digest(g: &GithubWorkspace) -> [u8; 32] {
    digest_generation(g, g.generation)
}
fn route_digest(g: &GithubWorkspace) -> [u8; 32] {
    // This is only a restriction. A live same-account login/generation change
    // must NOT erase the restriction and unlock an ordinary read fallback.
    let mut h = Sha256::new();
    h.update(b"state-source-read-intent-v80");
    h.update((g.identity.host.len() as u64).to_le_bytes());
    h.update(g.identity.host.as_bytes());
    h.update(g.identity.account_id.unwrap_or(0).to_le_bytes());
    h.update((g.repositories.len() as u64).to_le_bytes());
    for s in &g.repositories {
        h.update((s.len() as u64).to_le_bytes());
        h.update(s.as_bytes());
    }
    h.update((g.exclusions.len() as u64).to_le_bytes());
    for s in &g.exclusions {
        h.update((s.len() as u64).to_le_bytes());
        h.update(s.as_bytes());
    }
    h.finalize().into()
}
fn digest_generation(g: &GithubWorkspace, generation: u64) -> [u8; 32] {
    let mut h = Sha256::new();
    for bytes in [g.identity.host.as_bytes(), g.identity.login.as_bytes()] {
        h.update((bytes.len() as u64).to_le_bytes());
        h.update(bytes);
    }
    h.update(g.identity.account_id.unwrap_or(0).to_le_bytes());
    h.update(generation.to_le_bytes());
    h.update((g.repositories.len() as u64).to_le_bytes());
    for value in &g.repositories {
        h.update((value.len() as u64).to_le_bytes());
        h.update(value.as_bytes());
    }
    h.update((g.exclusions.len() as u64).to_le_bytes());
    for value in &g.exclusions {
        h.update((value.len() as u64).to_le_bytes());
        h.update(value.as_bytes());
    }
    h.finalize().into()
}
/// Prepared data only; the stamp cannot escape into a Workspace until the
/// SAME actual closed read and atomic transaction return it to the installer.
pub(crate) struct PreparedStateWorkspaceV80 {
    original: Arc<crate::state_inbox_sync_v76::StateSourceSyncV76>,
    expected: [u8; 32],
    generation: u64,
    fetched_at: u64,
    legacy: Option<Arc<LegacyStartupQueueV80>>,
    route: Arc<StateSourceRouteHintV80>,
    filter: InboxFilter,
    selected: Option<PrKey>,
}
impl PreparedStateWorkspaceV80 {
    pub(crate) fn prepare(
        read: &crate::providers::gh::StateSourceReadV80,
        generation: u64,
        fetched_at: u64,
    ) -> Result<Self> {
        let g = read.workspace();
        let actual = read.original().original();
        if generation == 0
            || generation > i64::MAX as u64
            || !g.inbox.coverage.complete
            || g.scope()? != *actual.scope()
        {
            bail!("Actual State workspace preparation differs");
        }
        bounded_source(g)?;
        let legacy = matches!(
            actual.origin(),
            crate::state_inbox_sync_v76::StateSourceOriginV80::LegacyStartup
        )
        .then(|| {
            Arc::new(LegacyStartupQueueV80 {
                digest: digest_generation(g, generation),
                startup: None,
            })
        });
        let (filter, selected) = actual.selection_v80();
        Ok(Self {
            original: Arc::clone(read.original()),
            expected: digest(g),
            generation,
            fetched_at,
            legacy,
            route: Arc::new(StateSourceRouteHintV80 {
                digest: route_digest(g),
            }),
            filter,
            selected: selected.clone(),
        })
    }
    pub(crate) fn install(
        self,
        original: &Arc<crate::state_inbox_sync_v76::StateSourceSyncV76>,
        g: &mut GithubWorkspace,
    ) -> Result<()> {
        if !Arc::ptr_eq(&self.original, original)
            || self.expected != digest(g)
            || original.original().cancellation().is_cancelled()
            || Instant::now() >= original.original().deadline()
        {
            bail!("Exact prepared State workspace/read changed");
        }
        g.generation = self.generation;
        g.freshness = Freshness::Fresh {
            fetched_at: self.fetched_at,
        };
        g.needs_refresh = false;
        g.state_membership_unavailable_v68 = false;
        g.filter = self.filter;
        g.selected = self.selected;
        if g.selected
            .as_ref()
            .is_some_and(|key| !g.inbox.pull_requests.iter().any(|p| PrKey::from(p) == *key))
        {
            g.selected = None;
        }
        g.legacy_startup_queue_v80 = self.legacy;
        g.state_source_route_v80 = Some(self.route);
        Ok(())
    }
}
impl GithubWorkspace {
    /// SAME real startup identity transport and ordinary healthy cached loader.
    /// A known State-fenced startup reads only bounded raw metadata, installs
    /// NO cached PR bodies, and waits for a separately explicit fresh read.
    pub fn legacy_startup_v80(
        provider: &GithubProvider,
        repositories: Vec<String>,
        exclusions: BTreeSet<String>,
        path: &Path,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        let identity = provider.identity(cancel)?;
        let mut g = Self::empty(identity, repositories, exclusions);
        if g.identity.account_id.is_none() {
            return g.cached(path);
        }
        let Ok(source) = bounded_source(&g) else {
            return g.cached(path);
        };
        let restriction =
            StateStartupReadV80::capture(&g.scope()?, &source, path, cancel, deadline)?;
        if let Some(restriction) = restriction {
            if restriction.cached_available() {
                restriction.validate(&g.scope()?, &source, path, cancel, deadline)?;
                g = g.cached(path)?;
            } else {
                g.generation = restriction.generation();
                g.retire_state_membership_v68();
                g.needs_refresh = false;
            }
            restriction.validate(&g.scope()?, &source, path, cancel, deadline)?;
            g.install_legacy_queue_v80(Some(restriction))?;
            g.state_source_route_v80 = Some(Arc::new(StateSourceRouteHintV80 {
                digest: route_digest(&g),
            }));
            if cancel.is_cancelled() || Instant::now() >= deadline {
                bail!("Actual startup queue expired/cancelled");
            }
        } else {
            g = g.cached(path)?;
            if g.install_legacy_queue_v80(None).is_err() {
                g.legacy_startup_queue_v80 = None;
            }
        }
        Ok(g)
    }
    pub(super) fn install_legacy_queue_v80(
        &mut self,
        startup: Option<StateStartupReadV80>,
    ) -> Result<()> {
        let _ = bounded_source(self)?;
        self.legacy_startup_queue_v80 = Some(Arc::new(LegacyStartupQueueV80 {
            digest: digest(self),
            startup,
        }));
        Ok(())
    }
    pub(crate) fn legacy_queue_v80(&self) -> Option<&Arc<LegacyStartupQueueV80>> {
        let stamp = self.legacy_startup_queue_v80.as_ref()?;
        (stamp.digest == digest(self)).then_some(stamp)
    }
    pub(crate) fn legacy_queue_retained_bytes_v80(&self) -> Option<usize> {
        self.legacy_startup_queue_v80
            .as_ref()
            .map_or(Some(0), |s| s.retained_bytes())
    }
    pub(crate) fn state_route_retained_bytes_v80(&self) -> usize {
        if self.state_source_route_v80.is_some() {
            std::mem::size_of::<StateSourceRouteHintV80>() + 2 * std::mem::size_of::<usize>()
        } else {
            0
        }
    }
    pub(crate) fn state_source_sync_required_v80(&self) -> bool {
        self.state_membership_unavailable_v68
            || self
                .state_source_route_v80
                .as_ref()
                .is_some_and(|r| r.digest == route_digest(self))
    }
}
impl LegacyStartupQueueV80 {
    pub(crate) fn retained_bytes(&self) -> Option<usize> {
        std::mem::size_of::<Self>()
            .checked_add(2 * std::mem::size_of::<usize>())?
            .checked_add(
                self.startup
                    .as_ref()
                    .map_or(Some(0), StateStartupReadV80::retained_bytes)?,
            )
    }
    pub(crate) fn validate(
        &self,
        g: &GithubWorkspace,
        path: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<()> {
        bounded_source(g)?;
        if self.digest != digest(g) {
            bail!("Original startup source/principal/generation changed");
        }
        if let Some(startup) = &self.startup {
            // Startup's raw witness only admitted its EMPTY unavailable view.
            // Each new explicit SourceSync captures a NEW exact raw State/Clear
            // predecessor before Core; an intervening genuine ACK is not a
            // permanently frozen old startup epoch or a freshness grant.
            startup.validate_file(path)?;
        }
        if cancel.is_cancelled() || Instant::now() >= deadline {
            bail!("Original startup read intent expired/cancelled");
        }
        Ok(())
    }
}
