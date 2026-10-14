//! Exact already-prepared local-role public images. One existing Main cache,
//! outside draw; no Actor DTO remapping, account lookup or hidden view fallback.
use crate::{
    actor_avatar_states_v1::{partition, ActorAvatarStatesV1},
    identity::{ActorIdentity, AvatarCache, CurlAvatarFetcher, GitRole},
    providers::gh::LocalCommitRoleReport,
    repository_commit_actor_native_v1::{LocalCommitActorNativeV1, PreparedLocalActorReportV1},
    theme::ColorMode,
};
use std::{path::Path, sync::Arc};

/// Returns only genuine reported roles, in Author/Committer order. Deleted or
/// null roles stay absent; local names/emails never manufacture a URL/account.
pub fn prepared_roles(report: &PreparedLocalActorReportV1) -> Vec<&ActorIdentity> {
    [GitRole::Author, GitRole::Committer]
        .into_iter()
        .filter_map(|role| match report.role(role) {
            LocalCommitRoleReport::Reported(actor) => Some(actor.display()),
            _ => None,
        })
        .collect()
}

/// Raw bounded display metadata preflight, never authority. Done before map
/// key copies/public image requests. No truncation-as-identity or normalization.
pub fn prepared_avatar_preflight(actors: &[&ActorIdentity]) -> bool {
    if actors.len() > 2 {
        return false;
    }
    let mut total = 0usize;
    for actor in actors {
        if actor.git_role.is_none() || !actor.host_associated {
            return false;
        }
        for field in [
            actor.host.as_deref(),
            actor.login.as_deref(),
            Some(actor.name.as_str()),
            actor.email.as_deref(),
            actor.avatar_url.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            if field.len() > 4096
                || field.len() > (64 * 1024usize).saturating_sub(total)
                || field.contains('\0')
            {
                return false;
            }
            total += field.len();
        }
    }
    true
}

/// Call AFTER catalogue-exclusive polling, BEFORE the generic active-surface
/// path. true captures Repository even with no report/noColor; do not consult
/// any hidden GH/Actions/Search/Dashboard actor source in that case.
pub fn poll_repository(
    state: &LocalCommitActorNativeV1,
    repository_visible: bool,
    images: &mut ActorAvatarStatesV1,
    cache: &mut Option<(String, AvatarCache)>,
    directory: &Path,
    enabled: bool,
    mode: ColorMode,
) -> bool {
    if !repository_visible {
        return false;
    }
    // Pure report details hide the pixel surface. Do not start a NEW request,
    // but do not poison valid same-source pending/cache entries on presentation
    // changes either. Genuine private retirement removes report and drains it.
    if state.details_open && state.report().is_some() {
        return true;
    }
    let report = state
        .report()
        .filter(|r| state.profile().is_some_and(|p| r.matches_profile(p)));
    let Some(report) = report.filter(|_| enabled && mode == ColorMode::TrueColor) else {
        images.retire_owned_cache(cache);
        return true;
    };
    let Some(scope) = partition(report.source().identity()) else {
        images.retire_owned_cache(cache);
        return true;
    };
    let roles = prepared_roles(report);
    if !prepared_avatar_preflight(&roles) {
        images.retire_owned_cache(cache);
        return true;
    }
    if cache.as_ref().is_none_or(|(old, _)| *old != scope) {
        images.retire_owned_cache(cache);
        if let Some(fetcher) = CurlAvatarFetcher::detect(Default::default()) {
            *cache = Some((
                scope.clone(),
                AvatarCache::new(directory.to_owned(), scope, Arc::new(fetcher)),
            ));
        }
    }
    if let Some((_, current)) = cache {
        if !images.sync_prepared(report.source().identity(), current, &roles) {
            images.retire_owned_cache(cache);
        }
    } else {
        images.clear();
    }
    true
}

#[cfg(test)]
#[path = "repository_commit_actor_avatars_v1_tests.rs"]
mod tests;
