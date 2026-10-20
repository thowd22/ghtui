//! Exact immutable old/new file projection; no foreground model/body copies.
use super::*;
use crate::{
    git::{
        original_comparison_sides_v1::{append_wrapped, bounded_commit, bounded_label},
        OriginalComparisonSideV1 as Side, OriginalComparisonSourceV1,
    },
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_diff_sides_v1::OriginalDiffSideTicketV1,
};

pub struct PreparedOriginalDiffSideV1 {
    source: Arc<OriginalComparisonSourceV1>,
    index: usize,
    side: Side,
    sha: String,
    entry: TreeEntry,
    commit: Arc<Commit>,
    preview: Arc<FilePreview>,
    rows: Arc<[DisplayRow]>,
    focus: usize,
}
impl std::fmt::Debug for PreparedOriginalDiffSideV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedOriginalDiffSideV1(<exact original content/member/side>)")
    }
}
impl PreparedOriginalDiffSideV1 {
    pub fn open(
        ticket: &OriginalDiffSideTicketV1,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        if let Some(reason) = ticket.absent_reason() {
            return Err(reason);
        }
        let source = ticket.source().clone();
        let sha = source
            .endpoint(ticket.side())
            .commit()
            .ok_or("Original root has no old file; no commit/HEAD substitute")?;
        let path = ticket
            .member()
            .path(ticket.side())
            .ok_or("Original side path unavailable")?;
        source.intent().origin().with_saved_navigation_read_v1(operation,|repo|{
            let entry=repo.tree_entry(sha,path).map_err(|error|error.to_string())?;
            if entry.path!=*path||entry.id.len()!=sha.len()||!crate::git::original_actor_saved_sha(&entry.id){return Err("Original side raw path/object hash format changed".into());}
            let submodule=match(entry.mode.as_str(),entry.kind.as_str()){
                ("100644"|"100755"|"120000","blob")=>false,
                ("160000","commit")=>true,
                _=>return Err("Original changed-file side is not a supported genuine blob/symlink/submodule type".into()),
            };
            let content=if submodule{vec![]}else{
                let size=repo.object_sizes(std::slice::from_ref(&entry.id)).map_err(|error|error.to_string())?.into_iter().next().ok_or("Original side object size unavailable")?;
                if size>2*1024*1024{return Err("Original side file exceeds2MiB; no blob read, worktree fallback, external renderer or fetch".into());}
                let bytes=repo.original_side_blob_capture_v1(&entry.id).map_err(|error|error.to_string())?;
                if bytes.len()!=size{return Err("Original side blob size contradicts observed type/size".into());}bytes
            };
            let commit=Arc::new(bounded_commit(repo,sha)?);
            preflight_preview(&content)?;
            let preview=Arc::new(file_preview(sha.into(),&entry,content,10_000));
            // Check worst-case escaped row allocation BEFORE prepared display.
            let estimate=preview.rows.iter().try_fold(0usize,|total,row|total.checked_add(row.label.len().saturating_mul(4)+32)).ok_or("Original side display size overflow")?;
            if estimate>8*1024*1024{return Err("Original side display exceeds8MiB; no frontend body clone".into());}
            let mut detail=vec![];
            append_wrapped(&format!("Original {} saved file • read-only comparison side",ticket.side().label().to_lowercase()),&mut detail)?;
            detail.push(format!("Actual {} saved version:",ticket.side().label()));detail.push(sha.into());
            detail.push("Requested base:".into());detail.push(source.intent().requested_base().unwrap_or("unavailable").into());
            detail.push("Pinned Old content:".into());detail.push(source.intent().content_base().into());
            detail.push("Pinned New content:".into());detail.push(source.intent().head().into());
            append_wrapped(&format!("Exact {} path: {}",ticket.side().label(),crate::git::display_path_bytes(&path.0)),&mut detail)?;
            append_wrapped(&format!("Saved object: {} • mode {} • {:?}",entry.id,entry.mode,preview.kind),&mut detail)?;
            append_wrapped(&format!("Author of {sha}: {}",crate::git_actor_labels_v1::actor_text(&commit.author)),&mut detail)?;
            append_wrapped(&format!("Committer of {sha}: {}",crate::git_actor_labels_v1::actor_text(&commit.committer)),&mut detail)?;
            let mut rows=detail.into_iter().map(|label|DisplayRow{label,role:SemanticRole::Reference,action:None, cells: None}).collect::<Vec<_>>();
            let focus=0;
            for row in &preview.rows {
                rows.push(DisplayRow{label:bounded_label(&format!("{:>5} {}",row.line.map_or(String::new(),|line|line.to_string()),row.label)),role:SemanticRole::Normal,action:if preview.kind==crate::repository_view::BlobPreviewKind::Text{row.line.map(|line|RowAction::FileProvenance(FileAction::BlameAt(line)))}else{None}, cells: None});
            }
            if preview.truncated{rows.push(DisplayRow{label:"Partial original file preview •10k line limit; selected raw identity unchanged".into(),role:SemanticRole::Boundary,action:None, cells: None});}
            operation.check()?;
            Ok(Arc::new(Self{source:source.clone(),index:ticket.index(),side:ticket.side(),sha:sha.into(),entry,commit,preview,rows:rows.into(),focus}))
        })
    }
    pub fn source(&self) -> &Arc<OriginalComparisonSourceV1> {
        &self.source
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn side(&self) -> Side {
        self.side
    }
    pub fn sha(&self) -> &str {
        &self.sha
    }
    pub(crate) fn commit_v1(&self) -> &Commit {
        &self.commit
    }
    pub fn entry(&self) -> &TreeEntry {
        &self.entry
    }
    pub fn preview(&self) -> &FilePreview {
        &self.preview
    }
    pub fn rows(&self) -> &[DisplayRow] {
        &self.rows
    }
    pub fn focus(&self) -> usize {
        self.focus
    }
    pub fn return_ticket(self: &Arc<Self>) -> OriginalDiffReturnTicketV1 {
        OriginalDiffReturnTicketV1 { side: self.clone() }
    }
    pub fn owns(&self, workspace: &Workspace) -> bool {
        workspace
            .actor_origin
            .as_ref()
            .is_some_and(|origin| Arc::ptr_eq(origin, self.source.intent().origin()))
            && workspace.directory == self.source.intent().origin().source().root()
            && workspace
                .file_provenance
                .as_ref()
                .is_some_and(|state| state.revision == self.sha && state.path == self.entry.path)
            && workspace
                .original_diff_side_v1
                .as_ref()
                .is_some_and(|side| std::ptr::eq(self, side.as_ref()))
    }
    pub fn visible_for(&self, workspace: &Workspace) -> bool {
        self.owns(workspace)
            && workspace.mode == RepositoryMode::Tree
            && workspace.actor_profile.is_none()
            && workspace
                .file_provenance
                .as_ref()
                .is_some_and(|state| state.view == ProvenanceView::File)
            && workspace.revision == self.sha
            && workspace
                .selected_commit
                .as_ref()
                .is_some_and(|commit| commit.id == self.sha)
    }
    /// Runs only on the current owned worker; setter moves Arc and clears old
    /// legacy bodies. Actual side roles, SHA/path/line remain independent of HEAD.
    pub(crate) fn prepared_commit(&self) -> Commit {
        self.commit.as_ref().clone()
    }
    pub fn install(
        self: Arc<Self>,
        workspace: &mut Workspace,
        prepared_commit: Commit,
    ) -> Result<usize, String> {
        if !self.source.intent().owns(workspace) {
            return Err("Original comparison retired before side publication".into());
        }
        if prepared_commit.id != self.commit.id {
            return Err("Original prepared side roles/version changed".into());
        }
        workspace.rows.clear();
        workspace.tree = None;
        workspace.preview = None;
        workspace.comparison = None;
        workspace.tree_navigation_v1 = None;
        workspace.tree_breadcrumbs_v2 = None;
        workspace.graph_state_v1 = None;
        workspace.graph_display_v1 = None;
        workspace.revision = self.sha.clone();
        workspace.mode = RepositoryMode::Tree;
        workspace.actor_profile = None;
        workspace.selected_commit = Some(prepared_commit);
        workspace.file_provenance = Some(FileProvenance::new(
            self.sha.clone(),
            self.entry.path.clone(),
        ));
        workspace.original_comparison_v1 = Some(self.source.clone());
        let focus = self.focus;
        workspace.original_diff_side_v1 = Some(self);
        Ok(focus)
    }
    pub fn prepare_back(
        self: &Arc<Self>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<PreparedOriginalDiffBackV1, String> {
        operation.check()?;
        self.source
            .intent()
            .origin()
            .with_saved_navigation_read_v1(operation, |_| {
                let mut focus=0;
                for (index,row) in self.source.rows().iter().enumerate() {
                    if index%64==0 {operation.check()?;}
                    if matches!(&row.action,Some(RowAction::DiffLine(anchor)) if anchor.file_index==self.index) {focus=index;break;}
                }
                operation.check()?;
                Ok(PreparedOriginalDiffBackV1 {
                    original: self.clone(),
                    commit: self.source.head_commit().clone(),
                    head: self.source.intent().head().into(),
                    focus,
                })
            })
    }
}
#[derive(Clone)]
pub struct OriginalDiffReturnTicketV1 {
    side: Arc<PreparedOriginalDiffSideV1>,
}
impl std::fmt::Debug for OriginalDiffReturnTicketV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalDiffReturnTicketV1(<original side/source>)")
    }
}
impl PartialEq for OriginalDiffReturnTicketV1 {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.side, &other.side)
    }
}
impl Eq for OriginalDiffReturnTicketV1 {}
impl OriginalDiffReturnTicketV1 {
    pub fn side(&self) -> &Arc<PreparedOriginalDiffSideV1> {
        &self.side
    }
}
fn preflight_preview(content: &[u8]) -> Result<(), String> {
    let bytes = match std::str::from_utf8(content) {
        Ok(value) => value
            .chars()
            .try_fold(0usize, |sum, ch| {
                let width = match ch {
                    '\n' | '\r' | '\t' => 2,
                    ch if ch.is_control()
                        || matches!(ch,'\u{202a}'..='\u{202e}'|'\u{2066}'..='\u{2069}') =>
                    {
                        4 + ((32 - (ch as u32).leading_zeros()).max(1) as usize).div_ceil(4)
                    }
                    ch => ch.len_utf8(),
                };
                sum.checked_add(width)
            })
            .ok_or("Original file prepared bytes overflow")?,
        Err(_) => content.len().saturating_mul(3),
    };
    if bytes > 8 * 1024 * 1024 {
        return Err("Original file display exceeds8MiB before text preparation".into());
    }
    Ok(())
}
pub struct PreparedOriginalDiffBackV1 {
    original: Arc<PreparedOriginalDiffSideV1>,
    commit: Commit,
    head: String,
    focus: usize,
}
impl PreparedOriginalDiffBackV1 {
    pub fn install(self, workspace: &mut Workspace) -> Result<usize, String> {
        if !self.original.owns(workspace) {
            return Err("Original comparison-side route retired; no rebound Back".into());
        }
        let source = &self.original.source;
        workspace.revision = self.head;
        workspace.mode = source.intent().mode();
        workspace.selected_commit = Some(self.commit);
        workspace.actor_profile = None;
        workspace.file_provenance = None;
        workspace.rows.clear();
        workspace.tree = None;
        workspace.preview = None;
        workspace.comparison = None;
        workspace.tree_navigation_v1 = None;
        workspace.tree_breadcrumbs_v2 = None;
        workspace.graph_state_v1 = None;
        workspace.graph_display_v1 = None;
        workspace.original_diff_side_v1 = None;
        workspace.original_comparison_v1 = Some(source.clone());
        Ok(self.focus)
    }
}
impl OriginalComparisonSourceV1 {
    pub fn visible_for(&self, workspace: &Workspace) -> bool {
        self.intent().same_loaded_source_v1(workspace)
            && workspace.original_diff_side_v1.is_none()
            && workspace
                .original_comparison_v1
                .as_ref()
                .is_some_and(|source| std::ptr::eq(self, source.as_ref()))
    }
    pub fn install(self: &Arc<Self>, workspace: &mut Workspace) -> Result<(), String> {
        if !self.intent().owns(workspace) {
            return Err("Original comparison changed before Arc admission".into());
        }
        workspace.rows.clear();
        workspace.comparison = None;
        workspace.tree = None;
        workspace.preview = None;
        workspace.tree_navigation_v1 = None;
        workspace.tree_breadcrumbs_v2 = None;
        workspace.graph_state_v1 = None;
        workspace.graph_display_v1 = None;
        workspace.original_diff_side_v1 = None;
        workspace.original_comparison_v1 = Some(self.clone());
        Ok(())
    }
}
impl Workspace {
    /// Worker-only preservation through an exact freshly prepared nested file
    /// destination. The previous Arc is a return link, NOT a new target/actor.
    pub(crate) fn carry_original_diff_side_v1(
        &mut self,
        side: Arc<PreparedOriginalDiffSideV1>,
    ) -> Result<(), String> {
        if self
            .actor_origin
            .as_ref()
            .is_none_or(|origin| !Arc::ptr_eq(origin, side.source.intent().origin()))
            || self
                .file_provenance
                .as_ref()
                .is_none_or(|state| state.revision != side.sha || state.path != side.entry.path)
        {
            return Err(
                "Nested file destination changed the original comparison side source".into(),
            );
        }
        if self
            .file_provenance
            .as_ref()
            .is_some_and(|state| state.view == ProvenanceView::File)
        {
            let preview = self
                .preview
                .as_ref()
                .ok_or("Fresh original side return has no actual file proof")?;
            if preview.revision != side.sha
                || preview.path != side.entry.path
                || preview.object_id != side.entry.id
            {
                return Err("Fresh original side return object/path changed".into());
            }
            self.preview = None;
            self.rows.clear();
            self.tree = None;
        }
        self.original_comparison_v1 = Some(side.source.clone());
        self.original_diff_side_v1 = Some(side);
        Ok(())
    }
    /// ONE comparison semantics for draw/selection/mutation availability. An
    /// old retained comparison never masks the active side/history/Graph frame.
    pub fn comparison_v1(&self) -> Option<&ComparisonResult> {
        self.comparison.as_ref().or_else(|| {
            self.original_comparison_v1
                .as_ref()
                .filter(|source| source.visible_for(self))
                .map(|source| source.result())
        })
    }
}

#[cfg(test)]
#[path = "original_diff_sides_v1_tests.rs"]
mod tests;
