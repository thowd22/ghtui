//! SOURCE-ONLY child of pr_resources_v1. Immutable selected-file data reuses
//! existing actual gh/auth/runner and original measured read window.
use super::*;
#[path = "pr_immutable_images_v1/model.rs"]
mod model;
#[path = "pr_immutable_images_v1/objects.rs"]
mod objects;
#[path = "pr_immutable_images_v1/patch.rs"]
mod patch;
pub use model::*;
use objects::*;
use patch::*;
use std::sync::Arc;
#[cfg(test)]
#[path = "pr_immutable_images_v1_tests.rs"]
mod tests;

impl PrImmutableFileOriginalV1 {
    /// Crate-local worker selection from an actual opaque resource observation.
    /// scope originates the SAME private navigation/source owner pair, NOT JSON.
    pub(crate) fn from_observed(
        scope: &crate::storage::Scope,
        observed: Arc<PrResourceReadV1>,
        file_index: usize,
        limits: &PrImmutableFileLimitsV1,
    ) -> GithubResult<Self> {
        limits.validate()?;
        let principal = scope
            .login
            .strip_prefix("user-id:")
            .and_then(|id| id.parse::<u64>().ok())
            .filter(|id| *id > 0 && *id <= i64::MAX as u64)
            .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?;
        if scope.login != format!("user-id:{principal}")
            || scope.host != observed.parent.summary.pr.host
            || observed.kind != PrResourceKindV1::Files
            || observed.parent.head_sha().len() != observed.parent.base_tip_sha().len()
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        let metadata = observed
            .files
            .get(file_index)
            .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?
            .clone();
        let raw = observed
            .raw_patch()
            .ok_or_else(|| safe(GithubErrorKind::Unsupported))?;
        let selected = selected_patch(raw, &metadata, limits)?;
        Ok(Self {
            scope: scope.clone(),
            observed,
            metadata,
            selected,
            file_index,
        })
    }
}

impl GithubProvider {
    /// Intermediate raw data only. The caller must use the SAME original
    /// current-access session and finish closing repo/local/principal guards.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pr_immutable_file_images_v1(
        &self,
        current: &CurrentCatalogueRepository,
        identity: &HostIdentity,
        original: &PrImmutableFileOriginalV1,
        limits: &PrImmutableFileLimitsV1,
        cancel: &CancellationToken,
        after_opening: impl FnOnce(&PrResourceParentV1) -> GithubResult<()>,
    ) -> GithubResult<PrImmutableFileReadV1> {
        limits.validate()?;
        if crate::storage::Scope::github(&identity.host, identity.account_id)
            .ok()
            .as_ref()
            != Some(&original.scope)
            || identity.host != self.host
            || current.repository().permissions.pull != Some(true)
            || current.repository().id != original.observed.parent.repository_id
            || current.repository().node_id.as_deref()
                != Some(original.observed.parent.repository_node.as_str())
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let before = &original.observed.parent;
        let endpoint = format!(
            "repos/{}/pulls/{}",
            current.repository().full_name,
            before.number()
        );
        let opening = parent(
            self,
            current,
            before.summary(),
            &document(self, &endpoint, limits.max_parent_bytes, cancel, false)?,
        )?;
        if !original_parent(before, &opening) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        after_opening(&opening).map_err(|failure| safe(failure.kind))?;
        checked_finished(self, cancel)?;
        let repository = opening.repository();
        let compare_endpoint = format!(
            "repos/{repository}/compare/{}...{}?per_page=1&page=1",
            before.base_tip_sha(),
            before.head_sha()
        );
        let comparison = document(
            self,
            &compare_endpoint,
            limits.max_document_bytes,
            cancel,
            false,
        )?;
        if comparison["base_commit"]["sha"].as_str() != Some(before.base_tip_sha()) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        let merge_base = comparison["merge_base_commit"]["sha"]
            .as_str()
            .filter(|sha| full_sha(sha) && sha.len() == before.head_sha().len())
            .ok_or_else(|| safe(GithubErrorKind::Json))?
            .to_owned();
        // No head_commit field, last-commit shortcut or300-files completeness.
        validate_optional_url(
            self,
            &comparison,
            "url",
            &format!(
                "repos/{repository}/compare/{}...{}",
                before.base_tip_sha(),
                before.head_sha()
            ),
        )?;
        let head_tree = commit_tree(self, repository, before.head_sha(), limits, cancel)?;
        let old_tree = commit_tree(self, repository, &merge_base, limits, cancel)?;
        let mut traversal = TreeReadState::default();
        let old = path_image(
            self,
            repository,
            &old_tree,
            (
                original.selected.old_lookup.as_slice(),
                original.selected.old_path.is_some(),
            ),
            limits,
            cancel,
            &mut traversal,
        )?;
        let new = path_image(
            self,
            repository,
            &head_tree,
            (
                original.selected.new_lookup.as_slice(),
                original.selected.new_path.is_some(),
            ),
            limits,
            cancel,
            &mut traversal,
        )?;
        let association =
            associate_patch(&original.selected, &original.metadata, &old, &new, limits)?;
        let ranges = |side: PrImageSideV1| {
            original
                .selected
                .file
                .hunks
                .iter()
                .filter_map(|hunk| {
                    let mut coordinates = hunk.lines.iter().filter_map(|line| {
                        if side == PrImageSideV1::Old {
                            line.old_line
                        } else {
                            line.new_line
                        }
                    });
                    let first = coordinates.next()?;
                    let last = coordinates.next_back().unwrap_or(first);
                    Some((first, last))
                })
                .collect::<Vec<_>>()
        };
        let old_ranges = ranges(PrImageSideV1::Old);
        let new_ranges = ranges(PrImageSideV1::New);
        let closing = parent(
            self,
            current,
            before.summary(),
            &document(self, &endpoint, limits.max_parent_bytes, cancel, true)?,
        )?;
        if !original_parent(&opening, &closing) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        checked_finished(self, cancel)?;
        Ok(PrImmutableFileReadV1 {
            canonical_file: original.selected.file.clone(),
            parent: closing,
            merge_base,
            head_tree,
            old_tree,
            old,
            new,
            association,
            file_index: original.file_index,
            patch_digest: original.selected.digest.clone(),
            raw_tree_rows: traversal.rows,
            tree_reads: traversal.reads,
            old_ranges,
            new_ranges,
        })
    }
}
pub(super) fn original_parent(first: &PrResourceParentV1, last: &PrResourceParentV1) -> bool {
    first.database_id == last.database_id
        && first.repository_id == last.repository_id
        && first.repository_node == last.repository_node
        && first.head_repository == last.head_repository
        && first.summary.pr.id == last.summary.pr.id
        && first.number() == last.number()
        && first.head_sha() == last.head_sha()
        && first.base_tip_sha() == last.base_tip_sha()
        && first.summary.head_ref == last.summary.head_ref
        && first.summary.base_ref == last.summary.base_ref
        && first.summary.pr.state == last.summary.pr.state
        && first.summary.pr.draft == last.summary.pr.draft
        && first.summary.merged == last.summary.merged
        && first.changed_files == last.changed_files
    // A genuine same-ID/owner rename updates only the current selected locator.
}
