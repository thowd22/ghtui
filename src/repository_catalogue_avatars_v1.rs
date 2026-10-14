//! Source-bound public avatar presentation. Requests run only at the Main
//! event boundary; drawing borrows an already decoded image. This is NOT
//! current authentication, repository access, or a mutation approval.
use crate::{
    app::App,
    identity::{ActorIdentity, AvatarCache, AvatarImage, AvatarState, CurlAvatarFetcher},
    providers::gh::{RepositoryCatalogueActor, RepositoryCatalogueRead},
    repository_catalogue_workspace_v1::{CatalogueFrame, CatalogueRow, CatalogueWorkspace},
    storage::Scope,
};
use std::{collections::HashMap, path::Path, sync::Arc};

const MAX_ROWS: usize = 64;

/// Retains genuine immutable source/frame handles only, never a reconstructed
/// provider observation or a serialized successful account receipt.
pub struct CatalogueAvatarRequest {
    source: Arc<RepositoryCatalogueRead>,
    frame: Option<Arc<CatalogueFrame>>,
    indices: Vec<usize>,
    scope_key: String,
}
impl std::fmt::Debug for CatalogueAvatarRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CatalogueAvatarRequest(<admitted original source rows; public assets only>)")
    }
}
impl CatalogueAvatarRequest {
    pub fn cache_scope(&self) -> &str {
        &self.scope_key
    }
    pub fn requested_rows(&self) -> usize {
        self.indices.len()
    }

    fn from_workspace(workspace: &CatalogueWorkspace, hidden: bool) -> Option<Self> {
        let source = workspace.source()?.clone();
        let scope = Scope::github(&source.identity().host, source.identity().account_id).ok()?;
        let frame = workspace.frame().cloned();
        let mut indices = Vec::new();
        if !hidden && !workspace.list_area.is_empty() {
            if let Some(model) = &frame {
                if model.rows.len() > MAX_ROWS {
                    return None;
                }
                let selected = model.selected_original();
                let selected_row = model
                    .rows
                    .iter()
                    .position(|row| Some(row.original_index) == selected)
                    .unwrap_or(0);
                let count = workspace.page_size.min(MAX_ROWS);
                let start = selected_row.saturating_sub(count.saturating_sub(1));
                for (offset, row) in model.rows.iter().enumerate().skip(start).take(count) {
                    let original = source.repositories().get(row.original_index)?;
                    if original.id != row.id {
                        return None;
                    }
                    let actual = original
                        .owner
                        .as_ref()
                        .map(|owner| ActorIdentity::github(&source.identity().host, &owner.actor));
                    if actual.as_ref() != row.actor.as_ref() {
                        return None;
                    }
                    indices.push(offset);
                }
            }
        }
        Some(Self {
            source,
            frame,
            indices,
            scope_key: format!("{}/{}", scope.host, scope.login),
        })
    }
}

/// The caller must branch on app.catalogue presence. None here MUST NOT cause
/// an underlying hidden GH/Actions/Search/Metrics actor-source fallback.
pub fn request(app: &App) -> Option<CatalogueAvatarRequest> {
    let modal = app.catalogue.as_ref()?;
    if modal.busy || modal.fenced {
        return None;
    }
    let workspace = modal.workspace.as_ref()?;
    let original = modal.guard.as_ref()?;
    if !Arc::ptr_eq(workspace.source()?, original.source()) {
        return None;
    }
    if modal.owner.host() != original.source().identity().host {
        return None;
    }
    CatalogueAvatarRequest::from_workspace(
        workspace,
        modal.dialog.is_some()
            || modal.source_transition.active()
            || modal.discovery.active()
            || modal.discovery.warning_active(),
    )
}

/// Main event-boundary hook, NEVER draw. True means the catalogue captures this
/// surface even when unavailable, so the caller must skip hidden-view prefetch.
/// Main retains the ONE cache and drains it at exit. No new pool/auth client.
pub fn poll_catalogue(
    app: &mut App,
    cache: &mut Option<(String, AvatarCache)>,
    directory: &Path,
    enabled: bool,
    was_catalogue: &mut bool,
) -> bool {
    if app.catalogue.is_none() {
        if *was_catalogue {
            if let Some((_, cache)) = cache.as_ref() {
                cache.cancel_pending();
            }
            *was_catalogue = false;
            app.avatar_states.clear();
        }
        return false;
    }
    let input = if enabled && app.theme().mode == crate::theme::ColorMode::TrueColor {
        request(app)
    } else {
        None
    };
    if let Some(input) = input {
        let scope = input.cache_scope().to_owned();
        if cache
            .as_ref()
            .is_none_or(|(previous, _)| *previous != scope)
        {
            if let Some((_, previous)) = cache.take() {
                previous.shutdown();
            }
            app.avatar_states.clear();
            if let Some(fetcher) = CurlAvatarFetcher::detect(Default::default()) {
                *cache = Some((
                    scope.clone(),
                    AvatarCache::new(directory.to_owned(), scope, Arc::new(fetcher)),
                ));
            }
        }
        if let Some((_, cache)) = cache.as_ref() {
            app.catalogue
                .as_mut()
                .unwrap()
                .avatars
                .prefetch(&input, cache);
        }
        *was_catalogue = true;
    } else {
        if let Some((_, cache)) = cache.as_ref() {
            cache.cancel_pending();
        }
        app.catalogue.as_mut().unwrap().avatars.clear();
        *was_catalogue = false;
    }
    true
}

struct Entry {
    repository_id: u64,
    owner: RepositoryCatalogueActor,
    identity: ActorIdentity,
    state: AvatarState,
}

/// Bounded to painted rows. Original source Arc and positive UID pair every
/// entry; raw owner IDs/node/type/login/URL are retained exactly, not login keys.
#[derive(Default)]
pub struct CatalogueAvatarAssets {
    source: Option<Arc<RepositoryCatalogueRead>>,
    entries: HashMap<usize, Entry>,
}
impl std::fmt::Debug for CatalogueAvatarAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CatalogueAvatarAssets(<original-row bound public-image state>)")
    }
}
impl CatalogueAvatarAssets {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.source = None;
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Outside draw ONLY. Existing AvatarCache owns capped/cancellable public
    /// workers and is retained by Main until shutdown; this map owns no thread.
    pub fn prefetch(&mut self, request: &CatalogueAvatarRequest, cache: &AvatarCache) {
        if cache.scope_key() != request.cache_scope() {
            self.clear();
            return;
        }
        if self
            .source
            .as_ref()
            .is_some_and(|source| !Arc::ptr_eq(source, &request.source))
        {
            cache.cancel_pending();
            self.clear();
        }
        self.source = Some(request.source.clone());
        let Some(frame) = &request.frame else {
            self.entries.clear();
            return;
        };
        if request.indices.is_empty() {
            // Geometry/dialog/query transitions do not cancel valid in-flight
            // images. No hidden rows are requested or displayed by this branch.
            self.entries.clear();
            return;
        }
        self.entries.retain(|index, _| {
            request
                .indices
                .iter()
                .any(|offset| frame.rows[*offset].original_index == *index)
        });
        for offset in &request.indices {
            let row = &frame.rows[*offset];
            let original = &request.source.repositories()[row.original_index];
            let (Some(owner), Some(identity)) = (original.owner.as_ref(), row.actor.as_ref())
            else {
                continue;
            };
            let state = cache.request(identity);
            if let Some(entry) = self.entries.get_mut(&row.original_index) {
                if entry.repository_id == original.id
                    && entry.owner == *owner
                    && entry.identity == *identity
                {
                    entry.state = state;
                    continue;
                }
            }
            self.entries.insert(
                row.original_index,
                Entry {
                    repository_id: original.id,
                    owner: owner.clone(),
                    identity: identity.clone(),
                    state,
                },
            );
        }
    }

    /// Pure bounded matching, no cache polling/request/decoding/filesystem work.
    pub fn image<'a>(
        &'a self,
        source: &Arc<RepositoryCatalogueRead>,
        row: &CatalogueRow,
    ) -> Option<&'a AvatarImage> {
        if !self
            .source
            .as_ref()
            .is_some_and(|original| Arc::ptr_eq(original, source))
        {
            return None;
        }
        let original = source.repositories().get(row.original_index)?;
        let entry = self.entries.get(&row.original_index)?;
        if original.id != row.id
            || entry.repository_id != row.id
            || original.owner.as_ref() != Some(&entry.owner)
            || row.actor.as_ref() != Some(&entry.identity)
        {
            return None;
        }
        match &entry.state {
            AvatarState::Ready(image) => Some(image.as_ref()),
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "repository_catalogue_avatars_v1_tests.rs"]
mod tests;
