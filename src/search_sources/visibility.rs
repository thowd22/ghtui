//! Bounded original-receipt recovery around private metadata reads.
use crate::{
    pr_projection::{guard_discussion_reads, validate_discussion_read_guard, DiscussionReadGuard},
    search::{collaboration::DiscussionTarget, workspace::SearchWorkspace},
    storage::Scope,
};
use std::path::Path;

/// Keep the safety outcome and current-access reason visible at 80 columns.
/// This formats an already-refused read; it does not classify or grant access.
pub fn hidden_read_status(error: &str) -> String {
    format!("Search failed • prior results hidden: {error}")
}

/// Fresh positive repo-ID/read access is separate from journal and SQL epochs.
/// No frame/count/Open can reuse a cached `Allowed` after an unavailable read.
pub fn access(
    provider: &crate::providers::gh::GithubProvider,
    identity: &crate::providers::gh::HostIdentity,
    scope: &Scope,
    targets: &[DiscussionTarget],
    cancel: &crate::providers::runner::CancellationToken,
) -> Result<(), String> {
    if Scope::github(identity.host.to_ascii_lowercase(), identity.account_id)
        .map_err(|e| e.to_string())?
        != *scope
    {
        return Err("Account identity differs from cached-source access scope".into());
    }
    let mut repos = std::collections::BTreeMap::new();
    for target in targets {
        target.validate().map_err(|e| e.to_string())?;
        if target.scope != *scope {
            return Err("Account identity differs from cached source".into());
        }
        if repos
            .insert(target.repository.to_ascii_lowercase(), target.repository_id)
            .is_some_and(|id| id != target.repository_id)
        {
            return Err("Cached-source repository IDs contradict one another".into());
        }
    }
    provider
        .cached_discussion_repository_access(
            identity,
            &repos.into_iter().collect::<Vec<_>>(),
            cancel,
        )
        .map_err(|e| {
            format!("Current repository access unavailable • prior private source hidden: {e}")
        })
}

pub fn before(
    database: &Path,
    search: &SearchWorkspace,
) -> Result<Vec<DiscussionReadGuard>, String> {
    before_targets(
        database,
        &search.scope,
        &search.metadata_sources.discussions,
    )
}
pub fn before_targets(
    database: &Path,
    scope: &Scope,
    targets: &[DiscussionTarget],
) -> Result<Vec<DiscussionReadGuard>, String> {
    let mut proofs = Vec::new();
    for batch in targets.chunks(256) {
        let guard = guard_discussion_reads(database, scope, batch)?;
        if !guard.blocked_sources().is_empty() {
            return Err(guard
                .blocked_sources()
                .values()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join(" • "));
        }
        proofs.push(guard);
    }
    Ok(proofs)
}
pub fn after(database: &Path, proofs: &[DiscussionReadGuard]) -> Result<(), String> {
    for guard in proofs {
        validate_discussion_read_guard(database, guard)?;
    }
    Ok(())
}

#[cfg(test)]
mod status_tests {
    #[test]
    fn pr_metadata_cached_access_failure_keeps_denial_and_hidden_outcome_visible_at_80() {
        let status = super::hidden_read_status(
            "Current repository access unavailable • prior private source hidden: Forbidden (HTTP 403)",
        );
        let visible: String = status.chars().take(79).collect();
        assert!(visible.contains("prior results hidden"));
        assert!(visible.contains("Current repository access unavailable"));
        assert!(!status.contains("0 results"));
    }
}
