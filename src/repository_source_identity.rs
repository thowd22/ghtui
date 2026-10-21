//! Bounded presentation of the exact prepared repository sources.
//! No object resolution, model/diff cloning, subprocess, SQL or filesystem IO.
//! These labels describe the original worker read, not current write authority.
use crate::{
    git::{display_path_bytes, Comparison},
    search::model::valid_sha,
    theme::SemanticToken,
    workspace::{RepositoryMode, Workspace},
};
use std::{os::unix::ffi::OsStrExt, sync::Arc};
use unicode_segmentation::UnicodeSegmentation;

struct MetadataViewV1<'a> {
    directory: &'a std::path::Path,
    revision: &'a str,
    status: &'a crate::git::RepoStatus,
    mode: RepositoryMode,
    selected_commit: Option<&'a crate::git::Commit>,
    preview: Option<&'a crate::repository_view::FilePreview>,
    comparison: Option<&'a crate::git::ComparisonResult>,
}
impl<'a> MetadataViewV1<'a> {
    fn workspace(workspace: &'a Workspace) -> Self {
        Self {
            directory: &workspace.directory,
            revision: &workspace.revision,
            status: &workspace.status,
            mode: workspace.mode,
            selected_commit: workspace.selected_commit.as_ref(),
            preview: workspace.file_preview_v1(),
            comparison: workspace.comparison_v1(),
        }
    }
    fn file_preview_v1(&self) -> Option<&'a crate::repository_view::FilePreview> {
        self.preview
    }
    fn comparison_v1(&self) -> Option<&'a crate::git::ComparisonResult> {
        self.comparison
    }
}

/// Tiny bounded original checkout labels only, not entries, a Workspace,
/// observed fingerprints, body copies, or fresh mutation authority.
pub(crate) fn original_diff_checkout_labels_v1(
    workspace: &Workspace,
) -> Result<crate::git::RepoStatus, String> {
    let mut budget = Rows::default();
    preflight(&MetadataViewV1::workspace(workspace), &mut budget)?;
    let status = &workspace.status;
    Ok(crate::git::RepoStatus {
        head: status.head.clone(),
        branch: status.branch.clone(),
        raw_branch: status.raw_branch.clone(),
        detached: status.detached,
        upstream: status.upstream.clone(),
        raw_upstream: status.raw_upstream.clone(),
        ahead: status.ahead,
        behind: status.behind,
        shallow: status.shallow,
        sparse: status.sparse,
        replacement_refs: status.replacement_refs,
        grafted: status.grafted,
        ..Default::default()
    })
}

const MAX_FIELD_BYTES: usize = 4096;
const MAX_RAW_BYTES: usize = 32 * 1024;
const MAX_PREPARED_BYTES: usize = 32 * 1024;
const MAX_ROWS: usize = 256;
const MAX_ROW_BYTES: usize = 1024;
const WRAP_CELLS: usize = 72;

#[derive(Clone, Debug)]
pub struct RepositorySourceRow {
    pub text: String,
    pub role: SemanticToken,
}
/// Cheap immutable model for native header/detail integration. No public
/// constructor, serialization, cached approval or path/ref-to-SHA inference.
#[derive(Clone, Debug)]
pub struct PreparedRepositorySourceIdentity {
    compact: Arc<[RepositorySourceRow]>,
    details: Arc<[RepositorySourceRow]>,
    detail_text: Arc<str>,
    checkout_head: Option<String>,
    viewed_revision: Option<String>,
    actor_commit: Option<String>,
    mutable: bool,
}
fn full_object(value: &str) -> bool {
    valid_sha(value) && value.bytes().any(|byte| byte != b'0')
}
#[derive(Default)]
struct Rows {
    raw_bytes: usize,
    prepared_bytes: usize,
    rows: Vec<RepositorySourceRow>,
}
impl Rows {
    fn raw(&mut self, value: &[u8]) -> Result<(), String> {
        self.raw_bytes = self.raw_bytes.saturating_add(value.len());
        if value.len() > MAX_FIELD_BYTES || self.raw_bytes > MAX_RAW_BYTES {
            return Err(
                "Repository source raw metadata budget exceeded before display copies".into(),
            );
        }
        Ok(())
    }
    fn row(&mut self, text: String, role: SemanticToken) -> Result<(), String> {
        self.prepared_bytes = self.prepared_bytes.saturating_add(text.len());
        if text.len() > MAX_ROW_BYTES
            || self.prepared_bytes > MAX_PREPARED_BYTES
            || self.rows.len() >= MAX_ROWS
        {
            return Err("Repository source prepared row/text budget exceeded".into());
        }
        self.rows.push(RepositorySourceRow { text, role });
        Ok(())
    }
    fn text(&mut self, text: &str, role: SemanticToken) -> Result<(), String> {
        if text.len() > MAX_RAW_BYTES {
            return Err("Repository source generated field exceeds its byte budget".into());
        }
        // Escape rather than silently collapse untrusted raw identifiers.
        let safe = display_path_bytes(text.as_bytes());
        self.safe_text(&safe, role)
    }
    fn raw_name(&mut self, label: &str, bytes: &[u8], role: SemanticToken) -> Result<(), String> {
        // Preflight checks the original bytes. Escape only once: a literal
        // backslash must remain distinct from a real control/invalid-byte escape.
        let name = display_path_bytes(bytes);
        self.safe_text(&format!("{label}: {name}"), role)
    }
    fn safe_text(&mut self, safe: &str, role: SemanticToken) -> Result<(), String> {
        if safe.len() > MAX_PREPARED_BYTES {
            return Err("Repository source escaped metadata byte budget exceeded".into());
        }
        let mut chunk = String::new();
        let mut cells = 0usize;
        let mut last_break = None;
        for grapheme in safe.graphemes(true) {
            if grapheme.len() > MAX_ROW_BYTES {
                return Err("Repository source grapheme byte budget exceeded".into());
            }
            let width = crate::editor::grapheme_width(grapheme);
            if width > WRAP_CELLS {
                return Err("Repository source grapheme display-width budget exceeded".into());
            }
            while !chunk.is_empty()
                && (cells.saturating_add(width) > WRAP_CELLS
                    || chunk.len().saturating_add(grapheme.len()) > MAX_ROW_BYTES)
            {
                if let Some((byte, break_cells)) = last_break.take() {
                    let tail = chunk.split_off(byte);
                    self.row(chunk.trim_end().into(), role)?;
                    chunk = tail;
                    cells = cells.saturating_sub(break_cells);
                } else {
                    self.row(std::mem::take(&mut chunk), role)?;
                    cells = 0;
                }
            }
            chunk.push_str(grapheme);
            cells = cells.saturating_add(width);
            if grapheme.chars().all(char::is_whitespace) {
                last_break = Some((chunk.len(), cells));
            }
        }
        self.row(chunk, role)
    }
    fn object(
        &mut self,
        label: &str,
        value: Option<&str>,
        role: SemanticToken,
    ) -> Result<(), String> {
        match value.filter(|value| full_object(value)) {
            Some(id) => self.text(&format!("{label}: {id}"), role),
            None => self.text(
                &format!("{label}: unavailable (no full object ID observed)"),
                SemanticToken::Muted,
            ),
        }
    }
}
/// Compact header label for the checked-out commit (Git's HEAD): the commit your
/// working files are on. Both labels stay at most 6 cells, so `label: <64-hex id>`
/// fits one WRAP_CELLS row and a SHA-256 ID is never split.
pub(crate) const CHECKED_OUT_LABEL: &str = "In use";
/// Compact header label for the saved version being viewed or selected.
pub(crate) const VIEWING_LABEL: &str = "Viewed";
fn preflight(workspace: &MetadataViewV1<'_>, rows: &mut Rows) -> Result<(), String> {
    rows.raw(workspace.directory.as_os_str().as_bytes())?;
    rows.raw(workspace.revision.as_bytes())?;
    for value in [
        &workspace.status.head,
        &workspace.status.branch,
        &workspace.status.upstream,
    ]
    .into_iter()
    .flatten()
    {
        rows.raw(value.as_bytes())?;
    }
    for value in [&workspace.status.raw_branch, &workspace.status.raw_upstream]
        .into_iter()
        .flatten()
    {
        rows.raw(value)?;
    }
    if let Some(commit) = &workspace.selected_commit {
        rows.raw(commit.id.as_bytes())?;
    }
    if let Some(preview) = workspace.file_preview_v1() {
        rows.raw(preview.revision.as_bytes())?;
    }
    if let Some(comparison) = workspace.comparison_v1() {
        for value in [
            &comparison.base,
            &comparison.head,
            &comparison.common_ancestor,
            &comparison.diff.base,
            &comparison.diff.head,
        ]
        .into_iter()
        .flatten()
        {
            rows.raw(value.as_bytes())?;
        }
        match &comparison.comparison {
            Comparison::Snapshots { base, head } | Comparison::BranchChanges { base, head } => {
                rows.raw(base.as_bytes())?;
                rows.raw(head.as_bytes())?;
            }
            Comparison::CommitChanges { commit, parent } => {
                rows.raw(commit.as_bytes())?;
                if let Some(parent) = workspace
                    .selected_commit
                    .as_ref()
                    .and_then(|selected| selected.parents.get(*parent))
                {
                    rows.raw(parent.as_bytes())?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}
impl PreparedRepositorySourceIdentity {
    pub(crate) fn prepare_native_mutable_v2(
        status: &crate::git::RepoStatus,
        source: &crate::git::MutableDiffSourceV2,
    ) -> Result<Self, String> {
        Self::prepare_metadata_v1(&MetadataViewV1 {
            directory: source.intent().origin().source().root(),
            revision: status.head.as_deref().unwrap_or("HEAD"),
            status,
            mode: match source.intent().kind() {
                crate::git::MutableDiffKindV2::Staged => RepositoryMode::Staged,
                crate::git::MutableDiffKindV2::Working => RepositoryMode::Working,
                crate::git::MutableDiffKindV2::Uncommitted => RepositoryMode::Uncommitted,
            },
            selected_commit: None,
            preview: None,
            comparison: Some(source.result()),
        })
    }
    pub fn prepare(workspace: &Workspace) -> Result<Self, String> {
        Self::prepare_metadata_v1(&MetadataViewV1::workspace(workspace))
    }
    pub(crate) fn prepare_original_diff_v1(
        status: &crate::git::RepoStatus,
        source: &crate::git::OriginalComparisonSourceV1,
        side: Option<&crate::workspace::PreparedOriginalDiffSideV1>,
    ) -> Result<Self, String> {
        Self::prepare_metadata_v1(&MetadataViewV1 {
            directory: source.intent().origin().source().root(),
            revision: side.map_or(source.intent().head(), |side| side.sha()),
            status,
            mode: side.map_or(source.intent().mode(), |_| RepositoryMode::Tree),
            selected_commit: Some(side.map_or(source.head_commit(), |side| side.commit_v1())),
            preview: side.map(|side| side.preview()),
            comparison: side.is_none().then_some(source.result()),
        })
    }
    fn prepare_metadata_v1(workspace: &MetadataViewV1<'_>) -> Result<Self, String> {
        let mut details = Rows::default();
        preflight(workspace, &mut details)?;
        let checkout_head = workspace
            .status
            .head
            .as_deref()
            .filter(|head| full_object(head));
        let actor_commit = workspace
            .selected_commit
            .as_ref()
            .map(|commit| commit.id.as_str())
            .filter(|id| full_object(id));
        let viewed_revision = workspace
            .file_preview_v1()
            .map_or(workspace.revision, |preview| preview.revision.as_str());
        let viewed_revision = full_object(viewed_revision).then_some(viewed_revision);
        let working_preview = workspace
            .file_preview_v1()
            .is_some_and(|preview| preview.revision == "WORKTREE");
        let mutable = working_preview
            || matches!(
                workspace.mode,
                RepositoryMode::Overview
                    | RepositoryMode::Working
                    | RepositoryMode::Staged
                    | RepositoryMode::Uncommitted
            )
            || workspace.comparison_v1().is_some_and(|comparison| {
                matches!(
                    comparison.comparison,
                    Comparison::Working | Comparison::Uncommitted | Comparison::Staged
                )
            });
        let mut compact = Rows::default();
        // Plain-language labels (TASK-67): beginners read "Viewed" (the
        // selected saved version) and "In use" (the commit the working files
        // are on), and the full IDs stay visible. The details pane still
        // names HEAD for people who know Git's word.
        compact.object(CHECKED_OUT_LABEL, checkout_head, SemanticToken::Head)?;
        // Preserve the original genuine selected-commit selector. It describes
        // the displayed saved actor basis, not ownership of mutable edits.
        compact.object(VIEWING_LABEL, actor_commit, SemanticToken::Reference)?;
        let content = if working_preview {
            "working file; mutable"
        } else if let Some(comparison) = workspace.comparison_v1() {
            match comparison.comparison {
                Comparison::Working => "working files vs index; mutable",
                Comparison::Uncommitted => "working files vs HEAD; mutable",
                Comparison::Staged => "index vs HEAD; mutable",
                Comparison::Snapshots { .. } => "saved snapshots A to B",
                Comparison::BranchChanges { .. } => "common starting point to target",
                Comparison::CommitChanges { .. } => "saved commit introduced changes",
            }
        } else {
            match workspace.mode {
                RepositoryMode::Overview => "checkout overview; local edits remain mutable",
                RepositoryMode::Graph => "saved commit relationships",
                RepositoryMode::Tree if workspace.file_preview_v1().is_some() => {
                    "saved file at chosen version"
                }
                RepositoryMode::Tree => "saved tree at chosen version",
                RepositoryMode::Working => "working source not loaded; mutable",
                RepositoryMode::Staged => "index source not loaded; mutable",
                RepositoryMode::Uncommitted => "all tracked edits source not loaded; mutable",
                RepositoryMode::Branch => "branch comparison unavailable",
                RepositoryMode::CommitChanges => "saved comparison unavailable",
            }
        };
        compact.text(&format!("Content: {content}"), SemanticToken::Normal)?;
        details.text(
            "Repository source identity • original prepared read",
            SemanticToken::Head,
        )?;
        details.text(&format!("Content: {content}"), SemanticToken::Normal)?;
        details.raw_name(
            "Local repository",
            workspace.directory.as_os_str().as_bytes(),
            SemanticToken::Reference,
        )?;
        details.text("HEAD is the checked-out saved version; browsing another version does not check it out.",SemanticToken::Normal)?;
        details.object("Checkout HEAD", checkout_head, SemanticToken::Head)?;
        if checkout_head.is_none() {
            details.text("No checkout commit was observed. An unborn branch has no saved version; missing data must not become an invented SHA.",SemanticToken::Muted)?;
        }
        details.text(
            if workspace.status.detached {
                "Checkout branch: detached HEAD (no branch name follows it)"
            } else {
                "Checkout branch: a name pointing to HEAD, not a separate copy of files"
            },
            SemanticToken::Normal,
        )?;
        if let Some(branch) = workspace.status.raw_branch.as_deref() {
            details.raw_name("Observed branch name", branch, SemanticToken::Reference)?;
        } else if let Some(branch) = &workspace.status.branch {
            details.text(
                &format!("Observed branch label: {branch}"),
                SemanticToken::Reference,
            )?;
        }
        details.object(
            "Viewed saved version",
            viewed_revision,
            SemanticToken::Reference,
        )?;
        if !full_object(workspace.revision) {
            details.text(
                &format!(
                    "Original unresolved/mutable revision label: {}",
                    workspace.revision
                ),
                SemanticToken::Muted,
            )?;
        }
        if working_preview {
            details.text("The displayed file is WORKTREE content read from disk; its saved HEAD actor is not the author of these uncommitted bytes.",SemanticToken::Changed)?;
        }
        if let Some(id) = actor_commit {
            details.text(
                &format!("Displayed Git author and committer belong to saved commit: {id}"),
                SemanticToken::Actor,
            )?;
        } else {
            details.text("Saved-commit author/committer metadata is unavailable; this does not prove the repository has no saved versions.",SemanticToken::Muted)?;
        }
        details.text("Author wrote the saved change; committer recorded it. Neither role identifies who made current working/index edits.",SemanticToken::Actor)?;
        if let Some(comparison) = workspace.comparison_v1() {
            match &comparison.comparison {
                Comparison::Working=>details.text("Working: index (next-commit contents) to current working files. Both are mutable; neither is a saved commit.",SemanticToken::Changed)?,
                Comparison::Uncommitted=>details.text("Uncommitted: all tracked edits since saved HEAD (staged + unstaged), observed baseline to working files. Untracked files are separate and not included. The target is mutable, not a saved snapshot.",SemanticToken::Changed)?,
                Comparison::Staged=>details.text("Staged: observed saved HEAD to index (next-commit contents). The index is mutable, not an immutable commit.",SemanticToken::Changed)?,
                Comparison::Snapshots {base,head}=>{
                    details.text("Saved snapshots: complete A-to-B content comparison, not a commit-list range or checkout.",SemanticToken::Reference)?;
                    details.text(&format!("Requested A: {base}"),SemanticToken::Muted)?;
                    details.text(&format!("Requested B: {head}"),SemanticToken::Muted)?;
                }
                Comparison::BranchChanges {base,head}=>{
                    details.text("Proposed branch changes: common starting point to target B. Direction matters; the requested base A is not necessarily the diff's starting snapshot.",SemanticToken::Reference)?;
                    details.text(&format!("Requested base A: {base}"),SemanticToken::Muted)?;
                    details.text(&format!("Requested target B: {head}"),SemanticToken::Muted)?;
                }
                Comparison::CommitChanges {commit,parent}=>{
                    details.text(&format!("Introduced changes for requested saved commit: {commit}"),SemanticToken::Reference)?;
                    let original=workspace.selected_commit.as_ref().filter(|selected|comparison.head.as_deref()==Some(selected.id.as_str()) && full_object(&selected.id));
                    let genuine_root=original.is_some_and(|selected|selected.parents.is_empty())
                        && *parent==0 && !workspace.status.shallow
                        && (!full_object(commit) || comparison.head.as_deref()==Some(commit.as_str()))
                        && comparison.base.as_deref().is_some_and(full_object)
                        && comparison.base.as_ref().zip(comparison.head.as_ref()).is_some_and(|(base,head)|base.len()==head.len())
                        && comparison.base!=comparison.head
                        && comparison.diff.base==comparison.base && comparison.diff.head==comparison.head;
                    if genuine_root {details.text("Root starting content: the genuine loaded original commit has no parents; the actual comparison base is its empty starting tree, not a parent commit.",SemanticToken::Reference)?;}
                    else if let Some(parent_id)=original.and_then(|selected|selected.parents.get(*parent)) {
                        details.object(&format!("Selected parent {}",parent.saturating_add(1)),Some(parent_id),SemanticToken::Reference)?;
                        if comparison.base.as_ref()!=Some(parent_id) {details.text("Comparison base differs from the loaded selected parent; source bindings need inspection, not a guessed root.",SemanticToken::Muted)?;}
                    } else {details.text("Original parent/root evidence unavailable or shallow-limited. A missing/null comparison endpoint is not proof of an empty root tree.",SemanticToken::Muted)?;}
                }
            }
            let mutable_start = matches!(comparison.comparison, Comparison::Working);
            let mutable_target = matches!(
                comparison.comparison,
                Comparison::Working | Comparison::Uncommitted | Comparison::Staged
            );
            if mutable_start {
                details.text("Observed comparison A: index (mutable next-commit contents; not a saved object)", SemanticToken::Changed)?;
            } else {
                details.object(
                    "Observed comparison A",
                    comparison.base.as_deref(),
                    SemanticToken::Reference,
                )?;
            }
            if mutable_target {
                details.text(if matches!(comparison.comparison, Comparison::Staged) {"Observed comparison B: index (mutable next-commit contents; not a saved object)"} else {"Observed comparison B: working files (mutable; not a saved object)"}, SemanticToken::Changed)?;
            } else {
                details.object(
                    "Observed comparison B",
                    comparison.head.as_deref(),
                    SemanticToken::Reference,
                )?;
            }
            if matches!(comparison.comparison, Comparison::BranchChanges { .. }) {
                details.object(
                    "Common starting point",
                    comparison.common_ancestor.as_deref(),
                    SemanticToken::Ancestor,
                )?;
            }
            if mutable_start {
                details.text(
                    "Actual diff starting content: mutable index; no saved object ID applies",
                    SemanticToken::Changed,
                )?;
            } else {
                details.object(
                    "Actual diff starting object",
                    comparison.diff.base.as_deref(),
                    SemanticToken::Reference,
                )?;
            }
            if mutable_target {
                details.text(if matches!(comparison.comparison, Comparison::Staged) {"Actual diff target content: mutable index; no saved object ID applies"} else {"Actual diff target content: mutable working files; no saved object ID applies"}, SemanticToken::Changed)?;
            } else {
                details.object(
                    "Actual diff target object",
                    comparison.diff.head.as_deref(),
                    SemanticToken::Reference,
                )?;
            }
            if matches!(
                comparison.comparison,
                Comparison::Staged | Comparison::Uncommitted
            ) && comparison.base.is_none()
            {
                details.text("No saved HEAD comparison base observed; an initial index may contain staged content, but null metadata cannot identify an empty tree.",SemanticToken::Muted)?;
            }
        } else if matches!(
            workspace.mode,
            RepositoryMode::Working
                | RepositoryMode::Staged
                | RepositoryMode::Uncommitted
                | RepositoryMode::Branch
                | RepositoryMode::CommitChanges
        ) {
            details.text("Comparison source is unavailable; no endpoint or root identity is inferred from the selected mode.",SemanticToken::Muted)?;
        }
        if workspace.status.shallow {
            details.text("Shallow history: some original parents/relationships may be outside the available objects; absence is not full ancestry or root proof.",SemanticToken::Muted)?;
        }
        if workspace.status.has_history_overrides() {
            details.text(&format!("Original snapshot limitation: replacement refs {} • legacy grafts {}. These overrides must not silently rewrite original saved relationships or authorize history writes.",workspace.status.replacement_refs,if workspace.status.grafted {"present"} else {"absent"}),SemanticToken::Changed)?;
        }
        details.text("Source IDs describe that prepared read only. Refresh/current-state checks are still required before writes; reading this explanation performs no Git, gh or SQL work.",SemanticToken::Muted)?;
        if details
            .prepared_bytes
            .saturating_add(compact.prepared_bytes)
            > MAX_PREPARED_BYTES
            || details.rows.len().saturating_add(compact.rows.len()) > MAX_ROWS
        {
            return Err("Repository source combined row/text budget exceeded".into());
        }
        let detail_text = details
            .rows
            .iter()
            .map(|row| row.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        if detail_text.len() > MAX_PREPARED_BYTES {
            return Err("Repository source detail text exceeds its aggregate budget".into());
        }
        Ok(Self {
            compact: Arc::from(compact.rows),
            details: Arc::from(details.rows),
            detail_text: Arc::from(detail_text),
            checkout_head: checkout_head.map(str::to_owned),
            viewed_revision: viewed_revision.map(str::to_owned),
            actor_commit: actor_commit.map(str::to_owned),
            mutable,
        })
    }
    pub fn compact(&self) -> &[RepositorySourceRow] {
        &self.compact
    }
    pub fn details(&self) -> &[RepositorySourceRow] {
        &self.details
    }
    pub fn detail_text(&self) -> &str {
        &self.detail_text
    }
    pub fn checkout_head(&self) -> Option<&str> {
        self.checkout_head.as_deref()
    }
    pub fn viewed_revision(&self) -> Option<&str> {
        self.viewed_revision.as_deref()
    }
    pub fn actor_commit(&self) -> Option<&str> {
        self.actor_commit.as_deref()
    }
    pub fn mutable(&self) -> bool {
        self.mutable
    }
}

#[cfg(test)]
#[path = "repository_source_identity_tests.rs"]
mod tests;
