//! Public assets for the actual owned Related-PR surface, not access evidence.
//! Draw records weak source identity and scalar geometry only; projection and
//! image requests happen at the Main event boundary through the shared cache.
use super::{retire_invalid, RelatedPrStateV1};
use crate::{
    actor_avatar_states_v1::partition,
    app::App,
    github_workspace::PrTab,
    identity::{ActorIdentity, AvatarCache, CurlAvatarFetcher},
    providers::gh::{Actor, ClosedRelatedPrsV1},
    theme::ColorMode,
};
use std::{
    ops::Range,
    path::Path,
    sync::{Arc, Weak},
};

pub(super) struct ViewportV1 {
    source: Weak<ClosedRelatedPrsV1>,
    scroll: usize,
    selected: usize,
    rows: Range<usize>,
    tab: Option<PrTab>,
}
impl ViewportV1 {
    pub(super) fn candidates(state: &RelatedPrStateV1, visible: usize) -> Option<Self> {
        let source = state.snapshot.as_ref()?;
        let start = state.scroll.min(source.candidates().len());
        Some(Self {
            source: Arc::downgrade(source),
            scroll: state.scroll,
            selected: state.selected,
            rows: start..start.saturating_add(visible).min(source.candidates().len()),
            tab: None,
        })
    }
    pub(super) fn viewer(
        state: &RelatedPrStateV1,
        viewer: &crate::github_workspace::GithubWorkspace,
        visible: usize,
    ) -> Option<Self> {
        let source = state.snapshot.as_ref()?;
        let page = viewer.pr.as_ref()?;
        let start = state.scroll.min(page.rows.len());
        Some(Self {
            source: Arc::downgrade(source),
            scroll: state.scroll,
            selected: state.selected,
            rows: start..start.saturating_add(visible).min(page.rows.len()),
            tab: Some(page.tab),
        })
    }
    fn matches(&self, state: &RelatedPrStateV1, source: &Arc<ClosedRelatedPrsV1>) -> bool {
        Weak::ptr_eq(&self.source, &Arc::downgrade(source))
            && self.scroll == state.scroll
            && self.selected == state.selected
            && self.tab
                == state
                    .viewer
                    .as_ref()
                    .and_then(|v| v.pr.as_ref())
                    .map(|p| p.tab)
    }
}
fn actors(state: &RelatedPrStateV1) -> Result<Option<Vec<Actor>>, ()> {
    let source = state.snapshot.as_ref().ok_or(())?;
    if !Arc::ptr_eq(source.original_source(), state.choice.original_source())
        || source.selected_index() != state.choice.original_index()
        || source.sha() != state.profile.sha()
        || source.identity().account_id.is_none_or(|id| id == 0)
    {
        return Err(());
    }
    let Some(marker) = state.avatar_view_v1.as_ref() else {
        return Ok(None);
    };
    if !marker.matches(state, source) {
        return Err(());
    }
    if let Some(viewer) = &state.viewer {
        if viewer.identity != *source.identity() {
            return Err(());
        }
        let page = viewer.pr.as_ref().ok_or(())?;
        let rows = page.rows.get(marker.rows.clone()).ok_or(())?;
        crate::actor_avatar_projection_v1::collect(rows.iter().filter_map(|row| row.actor.as_ref()))
            .map(Some)
            .map_err(|_| ())
    } else {
        let rows = source.candidates().get(marker.rows.clone()).ok_or(())?;
        let observed =
            crate::actor_avatar_projection_v1::collect(rows.iter().map(|row| &row.pr().author))
                .map_err(|_| ())?;
        // Fallback labels do not license another actor or observed URL.
        if rows.iter().zip(&observed).any(|(row, actor)| {
            ActorIdentity::github(&source.identity().host, actor) != *row.display_actor()
        }) {
            return Err(());
        }
        Ok(Some(observed))
    }
}
/// Captures this modal even pending/failed/disabled; never hidden-view prefetch.
pub fn poll_related(
    app: &mut App,
    cache: &mut Option<(String, AvatarCache)>,
    directory: &Path,
    enabled: bool,
) -> bool {
    let was_related = app.related_pr_v1.is_some();
    retire_invalid(app);
    let mode = app.theme().mode;
    let Some(state) = app.related_pr_v1.as_mut() else {
        if was_related {
            app.avatar_states.retire_owned_cache(cache);
        }
        return false;
    };
    if !enabled
        || mode != ColorMode::TrueColor
        || state.pending.is_some()
        || state.boundary_failed
        || app.related_pr_requested_v1
        || app.related_pr_cancel_v1
        || app.repository_load_requested
        || app.repository_action.is_some()
        || app.navigation_restore.is_some()
        || app.navigation_restore_inflight
        || app.graph_reread_request_v3.is_some()
        || app.original_tree_work_v1.is_some()
        || app.graph_reread_dialog_v3.is_some()
        || app.original_tree_jump_v1.is_some()
        || app.original_diff_picker_v1.is_some()
        || app.original_diff_work_v1.is_some()
        || app.original_diff_cancel_v1
        || app.catalogue.is_some()
        || app.overlay.is_some()
        || app.form.is_some()
        || app.refresh_requested
        || app.cancel_requested
        || app.local_actor_requested
        || app.local_actor_cancel_requested
        || app.original_tree_cancel_v1
    {
        state.avatar_view_v1 = None;
        app.avatar_states.retire_owned_cache(cache);
        return true;
    }
    let observed = match actors(state) {
        Ok(Some(observed)) => observed,
        // Geometry-only invalidation is not a retry/cache epoch.
        Ok(None) => {
            app.avatar_states.clear();
            return true;
        }
        Err(()) => {
            state.avatar_view_v1 = None;
            app.avatar_states.retire_owned_cache(cache);
            return true;
        }
    };
    let source = state.snapshot.as_ref().expect("validated original source");
    if observed.is_empty() {
        app.avatar_states.clear();
        return true;
    }
    let Some(scope) = partition(source.identity()) else {
        app.avatar_states.retire_owned_cache(cache);
        return true;
    };
    if cache
        .as_ref()
        .is_none_or(|(previous, _)| *previous != scope)
    {
        app.avatar_states.retire_owned_cache(cache);
        if let Some(fetcher) = CurlAvatarFetcher::detect(Default::default()) {
            *cache = Some((
                scope.clone(),
                AvatarCache::new(directory.to_owned(), scope, Arc::new(fetcher)),
            ));
        }
    }
    if let Some((_, cache)) = cache.as_ref() {
        app.avatar_states.sync_from_cache(
            source.identity(),
            "original-related-visible-actors-v1",
            cache,
            &observed,
        );
    }
    true
}

#[cfg(test)]
#[path = "repository_related_pr_avatars_v1_tests.rs"]
mod tests;
