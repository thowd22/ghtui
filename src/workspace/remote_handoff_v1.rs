//! Actual installed-Git snapshot prepared inside the original remote operation.
//! Matching bytes are NOT a persistent remote association or mutation grant.
use super::*;
use crate::{
    git::OriginalLocalActorOriginV1,
    providers::gh::{verify_blob_digest, RemoteFileV1},
};

impl Workspace {
    pub(crate) fn from_matching_remote_snapshot_v1(
        repository: GitRepository,
        original: &RemoteFileV1,
        cancel: &CancellationToken,
    ) -> Result<Self, String> {
        if cancel.is_cancelled() || !original.directory().complete() {
            return Err("Original handoff is cancelled or lacks canonical membership".into());
        }
        let commit = original.directory().commit();
        let resolved = repository
            .resolve_original(commit)
            .map_err(|_| "Exact local saved commit unavailable; no fetch/HEAD fallback")?;
        if resolved != commit {
            return Err("Exact local commit differs; no revision rebase".into());
        }
        let path = RawPath(original.path().as_bytes().to_vec());
        let entry = repository
            .tree_entry(commit, &path)
            .map_err(|_| "Exact local saved path unavailable")?;
        let expected = original.entry();
        let expected_mode = match expected.kind() {
            crate::providers::gh::RemoteEntryKindV1::File => "100644",
            crate::providers::gh::RemoteEntryKindV1::Executable => "100755",
            crate::providers::gh::RemoteEntryKindV1::Symlink => "120000",
            _ => return Err("Only an exact verified blob can hand off; no submodule fetch".into()),
        };
        if entry.kind != "blob"
            || entry.id != expected.oid()
            || entry.mode != expected_mode
            || entry.path != path
        {
            return Err("Local tree mode/path/blob differs from remote snapshot".into());
        }
        let sizes = repository
            .object_sizes(std::slice::from_ref(&entry.id))
            .map_err(|_| "Local blob size unavailable")?;
        if sizes.first().copied() != Some(original.bytes().len()) {
            return Err("Local blob size differs; no content read or partial fallback".into());
        }
        let bytes = repository
            .blob(commit, &path)
            .map_err(|_| "Local saved blob unavailable; no lazy fetch")?;
        verify_blob_digest(&bytes, &entry.id).map_err(|_| "Local saved blob hash differs")?;
        if bytes != original.bytes() {
            return Err("Local and remote saved bytes differ".into());
        }
        let actor_origin = OriginalLocalActorOriginV1::capture(&repository)
            .map_err(|_| "Local source identity unavailable")?;
        let status = repository
            .status()
            .map_err(|_| "Local status unavailable")?;
        let preview = crate::remote_repository_v1::bounded_remote_preview_v1(file_preview(
            commit.into(),
            &entry,
            bytes,
            512,
        ));
        let mut workspace = Self {
            directory: repository.root.clone(),
            revision: commit.into(),
            mode: RepositoryMode::Tree,
            status,
            graph: CommitGraph::default(),
            graph_state_v1: None,
            graph_display_v1: None,
            graph_view_v1: Default::default(),
            graph_horizontal_v1: 0,
            graph_projection_start_v2: 0,
            tree: None,
            tree_navigation_v1: None,
            tree_breadcrumbs_v2: None,
            comparison: None,
            original_comparison_v1: None,
            original_diff_side_v1: None,
            preview: Some(preview),
            file_provenance: Some(FileProvenance::new(commit.into(), path.clone())),
            selected_commit: None,
            actor_origin: Some(actor_origin.clone()),
            actor_profile: None,
            rows: vec![],
        };
        workspace.push(
            "Matching snapshot bytes • not a verified remote association",
            SemanticRole::Reference,
            None,
        );
        workspace.push(
            &format!("Saved commit: {commit}"),
            SemanticRole::Reference,
            None,
        );
        workspace.push(
            &format!("Path: {} • read-only • checkout unchanged", path.display()),
            SemanticRole::Reference,
            None,
        );
        let rows = workspace
            .preview
            .as_ref()
            .expect("actual preview")
            .rows
            .clone();
        for row in rows {
            workspace.push(&row.label, SemanticRole::Normal, None);
        }
        // Before closure: original loading token and original filesystem source.
        actor_origin
            .recheck_loading()
            .map_err(|_| "Original loaded local source changed")?;
        if cancel.is_cancelled() {
            return Err("Original local handoff cancelled".into());
        }
        Ok(workspace)
    }
}
