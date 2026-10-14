//! Immutable prepared tree/file projection. Native admission moves/installs it;
//! no foreground source/body clone, new directory discovery or host report.
use super::*;
use crate::{
    git::{OriginalTreeInventoryV1, OriginalTreeKindV1},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_tree_jump_v1::OriginalTreeTicketV1,
};
use std::collections::{BTreeMap, BTreeSet};

const RAW_SELECTION_CAP: usize = 16 * 1024;
const ROW_BYTES_CAP: usize = 8 * 1024 * 1024;

impl Workspace {
    pub fn original_tree_saved_sha_v1(&self) -> Option<&str> {
        self.tree_navigation_v1
            .as_ref()
            .filter(|projection| projection.visible_for(self))
            .map(|projection| projection.inventory().intent().sha())
            .or_else(|| self.tree.as_ref().map(|tree| tree.revision.as_str()))
            .or_else(|| {
                self.preview
                    .as_ref()
                    .filter(|preview| {
                        preview.revision == self.revision
                            && crate::git::original_actor_saved_sha(&preview.revision)
                    })
                    .map(|preview| preview.revision.as_str())
            })
    }
    pub fn file_preview_v1(&self) -> Option<&crate::repository_view::FilePreview> {
        self.preview.as_ref().or_else(|| {
            self.tree_navigation_v1
                .as_ref()
                .filter(|projection| projection.visible_for(self))
                .and_then(|projection| projection.preview())
        })
    }
}
pub(super) struct OriginalTreeRowSourceV1 {
    pub(super) inventory: Arc<OriginalTreeInventoryV1>,
    pub(super) expanded: Arc<[usize]>,
    pub(super) listed: Arc<[usize]>,
}
#[derive(Clone)]
pub struct OriginalTreeNodeTicketV1 {
    source: Arc<OriginalTreeRowSourceV1>,
    index: Option<usize>,
}
impl std::fmt::Debug for OriginalTreeNodeTicketV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalTreeNodeTicketV1(<genuine original row/source>)")
    }
}
impl PartialEq for OriginalTreeNodeTicketV1 {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.source, &other.source) && self.index == other.index
    }
}
impl Eq for OriginalTreeNodeTicketV1 {}
impl OriginalTreeNodeTicketV1 {
    /// Borrowed original fields for cursor hashing only, never a decoder.
    pub fn saved_sha(&self) -> &str {
        self.source.inventory.intent().sha()
    }
    pub fn member(&self) -> Option<&crate::git::OriginalTreeMemberV1> {
        self.index
            .and_then(|index| self.source.inventory.member(index))
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OriginalTreeNavigationActionV1 {
    Toggle(OriginalTreeNodeTicketV1),
    Directory(OriginalTreeNodeTicketV1),
}
pub struct OriginalTreeBreadcrumbV1 {
    label: String,
    width: usize,
    action: OriginalTreeNavigationActionV1,
}
impl OriginalTreeBreadcrumbV1 {
    pub fn label(&self) -> &str {
        &self.label
    }
    pub fn action(&self) -> &OriginalTreeNavigationActionV1 {
        &self.action
    }
    pub fn width(&self) -> usize {
        self.width
    }
}
pub struct PreparedOriginalTreeNavigationV1 {
    pub(super) source: Arc<OriginalTreeRowSourceV1>,
    rows: Arc<[DisplayRow]>,
    breadcrumbs: Arc<[OriginalTreeBreadcrumbV1]>,
    pub(super) preview: Option<Arc<crate::repository_view::FilePreview>>,
    pub(super) path: Option<RawPath>,
    pub(super) focus: usize,
    omitted: usize,
}
impl std::fmt::Debug for PreparedOriginalTreeNavigationV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedOriginalTreeNavigationV1(<bounded original local projection>)")
    }
}
impl PreparedOriginalTreeNavigationV1 {
    pub fn rows(&self) -> &[DisplayRow] {
        &self.rows
    }
    pub fn breadcrumbs(&self) -> &[OriginalTreeBreadcrumbV1] {
        &self.breadcrumbs
    }
    pub fn preview(&self) -> Option<&crate::repository_view::FilePreview> {
        self.preview.as_deref()
    }
    pub fn focus(&self) -> usize {
        self.focus
    }
    pub fn omitted_rows(&self) -> usize {
        self.omitted
    }
    pub fn inventory(&self) -> &Arc<OriginalTreeInventoryV1> {
        &self.source.inventory
    }
    pub fn visible_for(&self, workspace: &Workspace) -> bool {
        self.source.inventory.intent().owns(workspace)
            && workspace.mode == RepositoryMode::Tree
            && workspace.actor_profile.is_none()
            && match (&workspace.file_provenance, &self.path) {
                (None, None) => true,
                (Some(state), Some(path)) => {
                    state.view == ProvenanceView::File
                        && state.revision == workspace.revision
                        && state.path == *path
                }
                _ => false,
            }
    }
    /// A hidden projection is not a file source. Restore only its navigation
    /// context after the existing original-file reader has verified a fresh
    /// body at the same source, full SHA, raw path and immutable object.
    pub(super) fn matches_fresh_file_v1(&self, workspace: &Workspace) -> bool {
        self.visible_for(workspace)
            && self
                .preview()
                .zip(workspace.preview.as_ref())
                .is_some_and(|(prior, fresh)| {
                    prior.revision == fresh.revision
                        && prior.path == fresh.path
                        && prior.object_id == fresh.object_id
                        && prior.kind == fresh.kind
                })
    }
    /// Worker prepares initial immutable view from the newly admitted inventory.
    pub fn root(
        inventory: Arc<OriginalTreeInventoryV1>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        Self::tree(inventory, vec![], None, operation)
    }
    pub fn jump(
        ticket: &OriginalTreeTicketV1,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        let inventory = ticket.inventory().clone();
        let member = ticket.member();
        let expanded = ancestors(&inventory, &member.path().0)?;
        Self::open_member(inventory, ticket.index(), expanded, operation)
    }
    pub fn owns_ticket(&self, ticket: &OriginalTreeNodeTicketV1) -> bool {
        Arc::ptr_eq(&self.source, &ticket.source)
    }
    /// Source/node membership is checked BEFORE the first actual child.
    pub fn navigate(
        &self,
        action: &OriginalTreeNavigationActionV1,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        let ticket = match action {
            OriginalTreeNavigationActionV1::Toggle(ticket)
            | OriginalTreeNavigationActionV1::Directory(ticket) => ticket,
        };
        if !self.owns_ticket(ticket) {
            return Err("Original tree row/breadcrumb source changed; no target read".into());
        }
        operation.check()?;
        let inventory = self.source.inventory.clone();
        let Some(index) = ticket.index else {
            return Self::tree(inventory, vec![], None, operation);
        };
        let member = inventory
            .member(index)
            .ok_or("Original tree member unavailable")?;
        match action {
            OriginalTreeNavigationActionV1::Directory(_) => {
                if member.kind() != OriginalTreeKindV1::Directory {
                    return Err("Original breadcrumb is not a genuine directory".into());
                }
                let mut expanded = ancestors(&inventory, &member.path().0)?;
                expanded.push(index);
                Self::tree(inventory, expanded, Some(index), operation)
            }
            OriginalTreeNavigationActionV1::Toggle(_) => {
                if !self.source.listed.contains(&index) {
                    return Err("Original tree row is no longer visible".into());
                }
                let mut expanded = self.source.expanded.to_vec();
                if member.kind() == OriginalTreeKindV1::Directory && expanded.contains(&index) {
                    // Collapse is prepared-only: no Git/GH/SQL/source read.
                    let path = &member.path().0;
                    expanded.retain(|known| {
                        *known != index
                            && !inventory.members()[*known]
                                .path()
                                .0
                                .starts_with(&[path.as_slice(), b"/"].concat())
                    });
                    Self::tree(inventory, expanded, Some(index), operation)
                } else {
                    Self::open_member(inventory, index, expanded, operation)
                }
            }
        }
    }
    pub(super) fn open_member(
        inventory: Arc<OriginalTreeInventoryV1>,
        index: usize,
        mut expanded: Vec<usize>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        let member = inventory
            .member(index)
            .ok_or("Original tree member unavailable")?;
        inventory.intent().origin().with_saved_navigation_read_v1(operation,|repo|{
            let actual=repo.tree_entry(inventory.intent().sha(),member.path()).map_err(|error|error.to_string())?;
            if actual.path!=*member.path()||actual.id!=member.object_id()||actual.mode!=member.mode()||actual.kind!=member.entry().kind {return Err("Original raw path/mode/type/object changed; no source substitute".into());}
            if member.kind()==OriginalTreeKindV1::Directory {
                if !expanded.contains(&index){expanded.push(index);}return Self::tree(inventory.clone(),expanded.clone(),Some(index),operation);
            }
            let entry=member.entry().clone();
            let content=if member.kind()==OriginalTreeKindV1::Submodule {vec![]}else{
                let size=repo.object_sizes(std::slice::from_ref(&entry.id)).map_err(|error|error.to_string())?.into_iter().next().ok_or("Original saved object size unavailable")?;
                if size>2*1024*1024 {return Err("Original saved file exceeds2MiB preview cap; no blob read or working-file fallback".into());}
                repo.object(&entry.id).map_err(|error|error.to_string())?
            };
            let preview=Arc::new(file_preview(inventory.intent().sha().into(),&entry,content,10_000));
            let source=Arc::new(OriginalTreeRowSourceV1{inventory:inventory.clone(),expanded:expanded.clone().into(),listed:vec![index].into()});
            let mut rows=vec![DisplayRow{label:"Original immutable saved file • tree jump; no checkout or network".into(),role:SemanticRole::Reference,action:None},DisplayRow{label:bounded_label(&format!("Saved path: {}",member.display())),role:SemanticRole::Reference,action:None}];
            let mut bytes=0;let mut omitted=0;
            for row in &preview.rows {
                let label=bounded_label(&format!("{:>5} {}",row.line.map_or(String::new(),|line|line.to_string()),row.label));
                if bytes+label.len()>ROW_BYTES_CAP {omitted+=1;continue;}
                bytes+=label.len();rows.push(DisplayRow{label,role:SemanticRole::Normal,action:if preview.kind==crate::repository_view::BlobPreviewKind::Text {row.line.map(|line|RowAction::FileProvenance(FileAction::BlameAt(line)))}else{None}});
            }
            if preview.truncated||omitted>0 {rows.push(DisplayRow{label:"Original preview partial •10k-line/8MiB prepared limit; raw selected identity preserved".into(),role:SemanticRole::Boundary,action:None});}
            let breadcrumbs=breadcrumbs(source.clone(),Some(index))?;
            Ok(Arc::new(Self{source,rows:rows.into(),breadcrumbs:breadcrumbs.into(),preview:Some(preview),path:Some(entry.path),focus:2,omitted}))
        })
    }
    pub(super) fn tree(
        inventory: Arc<OriginalTreeInventoryV1>,
        expanded: Vec<usize>,
        focus: Option<usize>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        operation.check()?;
        if expanded.len() > 64 {
            return Err("Original expanded tree limited to64 directories".into());
        }
        let lookup: BTreeMap<&[u8], usize> = inventory
            .members()
            .iter()
            .enumerate()
            .map(|(index, row)| (row.path().0.as_slice(), index))
            .collect();
        let expanded_set: BTreeSet<usize> = expanded.iter().copied().collect();
        let mut metadata = 0;
        for index in &expanded {
            let row = inventory
                .member(*index)
                .ok_or("Original expanded member unavailable")?;
            if row.kind() != OriginalTreeKindV1::Directory {
                return Err("Expanded source member is not a directory".into());
            }
            metadata += row.path().0.len();
        }
        if metadata > RAW_SELECTION_CAP {
            return Err("Original expanded-path metadata exceeds16KiB".into());
        }
        let mut listed = vec![];
        let mut labels = vec![];
        let mut bytes = 0;
        let mut omitted = 0;
        for (index, row) in inventory.members().iter().enumerate() {
            operation.check()?;
            let visible = row
                .path()
                .0
                .iter()
                .enumerate()
                .filter(|(_, byte)| **byte == b'/')
                .all(|(end, _)| {
                    lookup
                        .get(&row.path().0[..end])
                        .is_some_and(|parent| expanded_set.contains(parent))
                });
            if !visible {
                continue;
            }
            let label = bounded_label(&format!(
                "{} {} · {}",
                match row.kind() {
                    OriginalTreeKindV1::Directory =>
                        if expanded_set.contains(&index) {
                            "▾"
                        } else {
                            "▸"
                        },
                    OriginalTreeKindV1::File => "·",
                    OriginalTreeKindV1::Symlink => "↗",
                    OriginalTreeKindV1::Submodule => "◇",
                },
                row.display(),
                row.kind().label()
            ));
            if bytes + label.len() > ROW_BYTES_CAP {
                omitted += 1;
                continue;
            }
            bytes += label.len();
            listed.push(index);
            labels.push(label);
        }
        let source = Arc::new(OriginalTreeRowSourceV1 {
            inventory,
            expanded: expanded.into(),
            listed: listed.into(),
        });
        let mut rows = vec![DisplayRow {
            label: "Saved file tree • exact original source; directory collapse is prepared-only"
                .into(),
            role: SemanticRole::Reference,
            action: None,
        }];
        let selected = focus
            .and_then(|index| source.listed.iter().position(|member| *member == index))
            .map_or(0, |index| index + 1);
        for (label, index) in labels.into_iter().zip(source.listed.iter().copied()) {
            rows.push(DisplayRow {
                label,
                role: SemanticRole::Normal,
                action: Some(RowAction::Tree(OriginalTreeNavigationActionV1::Toggle(
                    OriginalTreeNodeTicketV1 {
                        source: source.clone(),
                        index: Some(index),
                    },
                ))),
            });
        }
        if !source.inventory.complete() || omitted > 0 {
            rows.push(DisplayRow{label:"Partial saved tree • omitted/unseen paths unknown; Jump searches admitted original paths only".into(),role:SemanticRole::Boundary,action:None});
        }
        let crumbs = breadcrumbs(source.clone(), focus)?;
        operation.check()?;
        Ok(Arc::new(Self {
            source,
            rows: rows.into(),
            breadcrumbs: crumbs.into(),
            preview: None,
            path: None,
            focus: selected,
            omitted,
        }))
    }
    /// Typed publisher installs immutable fields without cloning body/source.
    pub fn install(self: Arc<Self>, workspace: &mut Workspace) -> Result<(), String> {
        if !self.source.inventory.intent().owns(workspace) {
            return Err("Original saved tree source changed before publication".into());
        }
        workspace.tree_breadcrumbs_v2 = None;
        workspace.rows.clear();
        workspace.tree = None;
        workspace.graph = CommitGraph::default();
        workspace.graph_state_v1 = None;
        workspace.graph_display_v1 = None;
        workspace.graph_view_v1 = Default::default();
        workspace.graph_horizontal_v1 = 0;
        workspace.graph_projection_start_v2 = 0;
        workspace.preview = None;
        workspace.comparison = None;
        workspace.actor_profile = None;
        workspace.file_provenance = self
            .path
            .as_ref()
            .map(|path| FileProvenance::new(workspace.revision.clone(), path.clone()));
        workspace.tree_navigation_v1 = Some(self);
        Ok(())
    }
}
fn ancestors(inventory: &OriginalTreeInventoryV1, path: &[u8]) -> Result<Vec<usize>, String> {
    let mut indices = vec![];
    let mut bytes = 0;
    for (end, _) in path.iter().enumerate().filter(|(_, byte)| **byte == b'/') {
        let index = inventory
            .members()
            .iter()
            .position(|row| {
                row.path().0 == path[..end] && row.kind() == OriginalTreeKindV1::Directory
            })
            .ok_or("Original directory prefix omitted/unavailable; no path guess")?;
        bytes += end;
        if indices.len() == 64 || bytes > RAW_SELECTION_CAP {
            return Err("Original directory breadcrumb bound exceeded".into());
        }
        indices.push(index);
    }
    Ok(indices)
}
fn breadcrumbs(
    source: Arc<OriginalTreeRowSourceV1>,
    focus: Option<usize>,
) -> Result<Vec<OriginalTreeBreadcrumbV1>, String> {
    let mut out = vec![OriginalTreeBreadcrumbV1 {
        label: "[Saved root]".into(),
        width: 12,
        action: OriginalTreeNavigationActionV1::Directory(OriginalTreeNodeTicketV1 {
            source: source.clone(),
            index: None,
        }),
    }];
    if let Some(index) = focus {
        let member = source
            .inventory
            .member(index)
            .ok_or("Original breadcrumb member unavailable")?;
        let mut parents = ancestors(&source.inventory, &member.path().0)?;
        if member.kind() == OriginalTreeKindV1::Directory {
            parents.push(index);
        }
        for index in parents {
            let row = &source.inventory.members()[index];
            let label = bounded_label(&format!("[Folder: {}]", row.display()));
            let width = crate::editor::display_width(&label);
            out.push(OriginalTreeBreadcrumbV1 {
                label,
                width,
                action: OriginalTreeNavigationActionV1::Directory(OriginalTreeNodeTicketV1 {
                    source: source.clone(),
                    index: Some(index),
                }),
            });
        }
    }
    Ok(out)
}
fn bounded_label(text: &str) -> String {
    if text.len() <= 4096 {
        text.into()
    } else {
        let mut end = 4092;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &text[..end])
    }
}

#[cfg(test)]
#[path = "tree_jump_v1_tests.rs"]
mod tests;
