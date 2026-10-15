//! Exact observed actor image matching. Public bytes and numeric cache partitions
//! are display metadata, NEVER authentication, permission or GitHub association.
//! Main syncs outside draw; widgets borrow by their actual prepared identity.
use crate::{
    app::App,
    identity::{ActorIdentity, AvatarCache, AvatarImage, AvatarState, CurlAvatarFetcher},
    providers::gh::{Actor, HostIdentity},
    storage::Scope,
};
use std::{collections::HashMap, path::Path, sync::Arc};

pub const MAX_ACTOR_AVATAR_ROWS: usize = 128;
const MAX_FIELD_BYTES: usize = 4096;
const MAX_BATCH_BYTES: usize = 64 * 1024;
const MAX_CONTEXT_BYTES: usize = 16 * 1024;

/// Keys include actual host/login/kind/name/email/URL/Git-role/association. No
/// public insert-by-login, serialized approval or display-name association.
#[derive(Default)]
pub struct ActorAvatarStatesV1 {
    states: HashMap<ActorIdentity, AvatarState>,
    scope: Option<String>,
    projection_v7: Option<Arc<()>>,
}
impl std::fmt::Debug for ActorAvatarStatesV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ActorAvatarStatesV1")
            .field("prepared_actor_count", &self.states.len())
            .finish_non_exhaustive()
    }
}
impl ActorAvatarStatesV1 {
    pub fn clear(&mut self) {
        self.states.clear();
        self.scope = None;
        self.projection_v7 = None;
    }
    /// Display allocation only: no source/access/permission or historical data.
    pub(crate) fn projection_v7(&self) -> Option<std::sync::Weak<()>> {
        self.projection_v7.as_ref().map(Arc::downgrade)
    }
    pub(crate) fn matches_projection_v7(&self, original: &std::sync::Weak<()>) -> bool {
        self.projection_v7
            .as_ref()
            .is_some_and(|current| std::sync::Weak::ptr_eq(original, &Arc::downgrade(current)))
    }
    pub fn len(&self) -> usize {
        self.states.len()
    }
    pub fn is_empty(&self) -> bool {
        self.states.is_empty()
    }
    /// Borrowed exact-key lookup; no row/key clone, I/O or source fallback.
    pub fn get(&self, actor: &ActorIdentity) -> Option<&AvatarState> {
        self.states.get(actor)
    }
    pub fn ready(&self, actor: &ActorIdentity) -> Option<&AvatarImage> {
        match self.get(actor) {
            Some(AvatarState::Ready(image)) => Some(image.as_ref()),
            _ => None,
        }
    }
    /// Retirement runs outside draw, keeping the existing Main-owned cache
    /// responsible for bounded worker joining at identity replacement/exit.
    pub fn retire(&mut self, cache: Option<&AvatarCache>) {
        self.clear();
        if let Some(cache) = cache {
            cache.cancel_pending();
        }
    }
    /// Actual unavailable/scope retirement drops the canceled cache epoch too.
    /// A later genuine source may create a fresh cache; real Failed assets are
    /// never retried merely by polling a still-current surface.
    pub fn retire_owned_cache(&mut self, cache: &mut Option<(String, AvatarCache)>) {
        self.clear();
        if let Some((_, previous)) = cache.take() {
            previous.shutdown();
        }
    }
    /// Exact already-prepared Git-role display keys. Display/cache metadata
    /// only, never a fresh remote receipt or permission/association constructor.
    pub(crate) fn sync_prepared(
        &mut self,
        requester: &HostIdentity,
        cache: &AvatarCache,
        actors: &[&ActorIdentity],
    ) -> bool {
        let Some(scope) = partition(requester) else {
            self.retire(Some(cache));
            return false;
        };
        if cache.scope_key() != scope
            || !crate::repository_commit_actor_avatars_v1::prepared_avatar_preflight(actors)
        {
            self.retire(Some(cache));
            return false;
        }
        if self.scope.as_deref().is_some_and(|old| old != scope) {
            cache.cancel_pending();
            self.clear();
        }
        // Role/filter/resize are not a cache epoch. Failed stays Failed; same
        // genuine actor/URL remains deduplicated by the one existing cache.
        let mut next = HashMap::with_capacity(actors.len());
        for identity in actors {
            next.insert((*identity).clone(), cache.request(identity));
        }
        self.states = next;
        self.scope = Some(scope);
        self.projection_v7 = Some(Arc::new(()));
        true
    }
    /// Synchronize ONLY the active admitted surface's real actor observations.
    /// This tests partition metadata, NOT whether that surface remains allowed.
    /// The application must still retire private views on source/access loss.
    /// Rows after128 remain ordinary initials; no whole-inventory cloning here.
    pub fn sync_from_cache(
        &mut self,
        requester: &HostIdentity,
        context: &str,
        cache: &AvatarCache,
        actors: &[Actor],
    ) -> bool {
        let Some(scope) = partition(requester) else {
            self.retire(Some(cache));
            return false;
        };
        if cache.scope_key() != scope || !preflight(context, actors) {
            self.retire(Some(cache));
            return false;
        }
        if self.scope.as_deref().is_some_and(|old| old != scope) {
            cache.cancel_pending();
            self.clear();
        }
        // Context/filter/tab labels are NOT a source epoch. Do not cancel a
        // valid same-actor request or convert it into permanent Failed state.
        // Raw fields were bounded before ActorIdentity::github copies/sanitizes.
        // Failed/Loading remain truthful states; cache owns dedup/no auto-retry.
        let mut next = HashMap::with_capacity(actors.len().min(MAX_ACTOR_AVATAR_ROWS));
        for actor in actors.iter().take(MAX_ACTOR_AVATAR_ROWS) {
            let identity = ActorIdentity::github(&requester.host, actor);
            let state = cache.request(&identity);
            next.insert(identity, state);
        }
        self.states = next;
        self.scope = Some(scope);
        self.projection_v7 = Some(Arc::new(()));
        true
    }
}

/// Positive caller cache partition, not a remote account/read receipt. Caller
/// login renames preserve the same partition; missing/zero UID cannot use it.
pub fn partition(identity: &HostIdentity) -> Option<String> {
    identity.account_id.filter(|id| *id > 0)?;
    let scope = Scope::github(&identity.host, identity.account_id).ok()?;
    Some(format!("{}/{}", scope.host, scope.login))
}

/// Main boundary hook. The caller first gives the catalogue its exclusive
/// painted source, and calls this ONLY if the catalogue does not own the view.
/// No hidden model fallback, no render/SQL/network-auth work, one existing cache.
pub fn sync_active_surface(
    app: &mut App,
    cache: &mut Option<(String, AvatarCache)>,
    directory: &Path,
    enabled: bool,
) {
    if !enabled || app.theme().mode != crate::theme::ColorMode::TrueColor {
        app.avatar_states.retire_owned_cache(cache);
        return;
    }
    let Some((identity, context, actors)) = app.avatar_source() else {
        app.avatar_states.retire_owned_cache(cache);
        return;
    };
    let Some(scope) = partition(&identity) else {
        app.avatar_states.retire_owned_cache(cache);
        return;
    };
    if !preflight(&context, &actors) {
        app.avatar_states.retire_owned_cache(cache);
        return;
    }
    if cache.as_ref().is_none_or(|(current, _)| *current != scope) {
        app.avatar_states.retire_owned_cache(cache);
        if let Some(fetcher) = CurlAvatarFetcher::detect(Default::default()) {
            *cache = Some((
                scope.clone(),
                AvatarCache::new(directory.to_owned(), scope, Arc::new(fetcher)),
            ));
        }
    }
    if let Some((_, current)) = cache {
        if !app
            .avatar_states
            .sync_from_cache(&identity, &context, current, &actors)
        {
            app.avatar_states.retire_owned_cache(cache);
        }
    } else {
        app.avatar_states.clear();
    }
}
fn preflight(context: &str, actors: &[Actor]) -> bool {
    if context.len() > MAX_CONTEXT_BYTES || context.contains('\0') {
        return false;
    }
    let mut bytes = 0usize;
    for actor in actors.iter().take(MAX_ACTOR_AVATAR_ROWS) {
        for field in [
            Some(actor.login.as_str()),
            Some(actor.actor_type.as_str()),
            actor.display_name.as_deref(),
            actor.avatar_url.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if field.len() > MAX_FIELD_BYTES || field.len() > MAX_BATCH_BYTES.saturating_sub(bytes)
            {
                return false;
            }
            bytes += field.len();
        }
    }
    true
}

#[cfg(test)]
#[path = "actor_avatar_states_v1_tests.rs"]
mod tests;
