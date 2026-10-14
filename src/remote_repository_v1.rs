//! SOURCE-ONLY worker-prepared remote Repository state. No fake local directory,
//! checkout status, JSON permission, persisted approval or implicit clone.
use crate::{
    editor::TextBuffer,
    git::{display_path_bytes, RawPath},
    identity::ActorIdentity,
    providers::{
        gh::{
            HostIdentity, RemoteDirectoryV1, RemoteEntryKindV1, RemoteFileV1,
            RemoteReferenceCatalogueV1, RemoteReferenceKindV1, RemoteRepositoryObservationV1,
            RemoteRepositoryPageV1, RemoteRepositoryRequestV1, RepositoryCatalogueRead,
        },
        runner::sanitize_terminal,
    },
    repository_catalogue_intent_v1::IntentReference,
    repository_view::{file_preview, FilePreview},
    theme::SemanticToken,
};
use std::sync::Arc;
mod controller;
mod local_handoff;
mod ui;
pub use controller::*;
pub use local_handoff::*;
pub use ui::REMOTE_REPOSITORY_TITLE_V1;

pub struct RemoteRepositoryOwnerV1 {
    pub(crate) original: Arc<RepositoryCatalogueRead>,
    pub(crate) reference: IntentReference,
}
impl std::fmt::Debug for RemoteRepositoryOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RemoteRepositoryOwnerV1(<original navigation owner; not auth>)")
    }
}
/// Capture only a real source row, never a raw name/ID/model JSON constructor.
impl RemoteRepositoryOwnerV1 {
    pub(crate) fn from_row(
        original: Arc<RepositoryCatalogueRead>,
        index: usize,
    ) -> Result<Arc<Self>, String> {
        let row = original
            .repositories()
            .get(index)
            .ok_or("Original catalogue row unavailable")?;
        if row.id == 0
            || row.full_name.len() > 256
            || row.node_id.as_ref().is_some_and(|n| n.len() > 1024)
        {
            return Err("Original remote row exceeds identity bounds".into());
        }
        let reference = IntentReference {
            id: row.id,
            node_id: row.node_id.clone(),
            last_name: row.full_name.clone(),
        };
        Ok(Arc::new(Self {
            original,
            reference,
        }))
    }
    pub fn original(&self) -> &Arc<RepositoryCatalogueRead> {
        &self.original
    }
    pub fn reference(&self) -> &IntentReference {
        &self.reference
    }
}
#[derive(Clone, Debug)]
pub struct RemoteBrowseRowV1 {
    pub label: String,
    pub index: usize,
    pub role: SemanticToken,
    pub unavailable: Option<&'static str>,
}
pub enum RemoteBrowsePageV1 {
    Directory(Arc<RemoteDirectoryV1>),
    References(Arc<RemoteReferenceCatalogueV1>),
    File {
        original: Arc<RemoteFileV1>,
        preview: Arc<FilePreview>,
    },
}
pub struct RemoteRepositoryWorkspaceV1 {
    pub owner: Arc<RemoteRepositoryOwnerV1>,
    pub identity: HostIdentity,
    pub canonical_name: String,
    pub rows: Arc<[RemoteBrowseRowV1]>,
    pub header: Arc<[String]>,
    pub actor: Option<ActorIdentity>,
    pub owner_actor: Option<crate::providers::gh::Actor>,
    pub page: RemoteBrowsePageV1,
    pub selected: usize,
    pub scroll: usize,
    pub generation: u64,
    pub commit_editor: TextBuffer,
    pub focus: usize,
    pub toolbar: usize,
    pub busy: bool,
    pub status: String,
    pub(crate) hits: Vec<(ratatui::layout::Rect, RemoteBrowseActionV1)>,
    pub(crate) list_area: ratatui::layout::Rect,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RemoteBrowseActionV1 {
    Row(usize),
    Move(i32),
    Page(i32),
    Open,
    Back,
    Up,
    Refresh,
    Branches,
    Tags,
    Commit,
    CommitInsert(String),
    CommitClear,
    CommitSelectAll,
    CommitBackspace,
    CommitLeft,
    CommitRight,
    CommitFocus,
    CommitClick(usize),
    FocusNext(bool),
    Toolbar(usize),
    Explain(&'static str),
    MappedLocal,
    Details,
}
impl RemoteRepositoryWorkspaceV1 {
    /// Worker only. Bounds BEFORE label/content copies; draw borrows these rows.
    pub fn prepare(
        owner: Arc<RemoteRepositoryOwnerV1>,
        observation: RemoteRepositoryObservationV1,
        generation: u64,
    ) -> Result<Self, String> {
        if observation.identity().host != owner.original.identity().host
            || observation.identity().account_id != owner.original.identity().account_id
            || observation.identity().account_id.is_none_or(|n| n == 0)
            || observation.repository().id != owner.reference.id
            || owner
                .reference
                .node_id
                .as_ref()
                .is_some_and(|n| observation.repository().node_id.as_ref() != Some(n))
        {
            return Err("Original remote model scope/resource differs; no publication".into());
        }
        let identity = observation.identity().clone();
        let canonical_name = observation.repository().full_name.clone();
        let actor = observation
            .repository()
            .owner
            .as_ref()
            .map(|o| ActorIdentity::github(&identity.host, &o.actor));
        let owner_actor = observation
            .repository()
            .owner
            .as_ref()
            .map(|owner| owner.actor.clone());
        let mut header = vec![
            format!(
                "Remote repository • {} • account #{} • @{}",
                canonical_name,
                identity.account_id.ok_or("Unverified account")?,
                sanitize_terminal(&identity.login)
            ),
            "Read-only • no clone/checkout • active repository scope unchanged".into(),
            "Repository owner is not the file author • authors not loaded".into(),
        ];
        let mut rows = Vec::new();
        let mut prepared_bytes = 0usize;
        let page = match observation.into_page() {
            RemoteRepositoryPageV1::Directory(directory) => {
                header.push(format!(
                    "Saved version {} • {}",
                    directory.commit(),
                    if directory.path().is_empty() {
                        "/".into()
                    } else {
                        display_path_bytes(directory.path().as_bytes())
                    }
                ));
                header.push(if directory.complete(){"Complete immediate directory • Git tree digest verified".into()}
                    else{"Partial reported directory • exact file membership unavailable • Refresh directory".into()});
                if directory.entries().len() > 20000 {
                    return Err("Remote prepared row cap exceeded".into());
                }
                for (index, row) in directory.entries().iter().enumerate() {
                    prepared_bytes = prepared_bytes
                        .checked_add(row.name().len().saturating_mul(4) + 256)
                        .filter(|n| *n <= 8 * 1024 * 1024)
                        .ok_or("Remote prepared labels exceed bounds")?;
                    rows.push(RemoteBrowseRowV1{label:format!("{} {} • {}",match row.kind(){RemoteEntryKindV1::Directory=>"▸",RemoteEntryKindV1::Symlink=>"↗",RemoteEntryKindV1::Submodule=>"◇",_=>"·"},
                        display_path_bytes(row.name().as_bytes()),row.kind().label()),index,role:SemanticToken::Reference,
                        unavailable:if !directory.complete(){Some("Partial reported tree cannot prove exact membership; Refresh this directory")}
                        else if row.kind()==RemoteEntryKindV1::Submodule{Some("Submodule commit only; never automatically fetched")}
                        else if row.size().is_some_and(|n|n>1024*1024){Some("Blob exceeds1MiB viewer bound; no content downloaded")}
                        else{None}});
                }
                RemoteBrowsePageV1::Directory(directory)
            }
            RemoteRepositoryPageV1::References(catalogue) => {
                header.push(format!(
                    "{}s • {} actual pages • {} observed choices • {}",
                    catalogue.kind().label(),
                    catalogue.pages(),
                    catalogue.entries().len(),
                    if catalogue.complete() {
                        "complete traversal, not atomic ref snapshot"
                    } else {
                        "partial; unseen refs unknown"
                    }
                ));
                for (index, row) in catalogue.entries().iter().enumerate() {
                    prepared_bytes = prepared_bytes
                        .checked_add(row.name().len().saturating_mul(4) + 128)
                        .filter(|n| *n <= 8 * 1024 * 1024)
                        .ok_or("Remote ref labels exceed bounds")?;
                    rows.push(RemoteBrowseRowV1 {
                        label: format!(
                            "{}: {} • {}",
                            catalogue.kind().label(),
                            display_path_bytes(row.name().as_bytes()),
                            row.commit()
                        ),
                        index,
                        role: SemanticToken::Reference,
                        unavailable: None,
                    });
                }
                RemoteBrowsePageV1::References(catalogue)
            }
            RemoteRepositoryPageV1::File(file) => {
                let entry = file.entry();
                let tree_entry = crate::git::TreeEntry {
                    path: RawPath::new(file.path().as_bytes().to_vec()),
                    id: entry.oid().into(),
                    kind: "blob".into(),
                    mode: match entry.kind() {
                        RemoteEntryKindV1::Symlink => "120000",
                        RemoteEntryKindV1::Executable => "100755",
                        _ => "100644",
                    }
                    .into(),
                };
                header.push(format!(
                    "Saved version {} • {} • verified blob {}",
                    file.directory().commit(),
                    display_path_bytes(file.path().as_bytes()),
                    entry.oid()
                ));
                let preview = Arc::new(bounded_remote_preview_v1(file_preview(
                    file.directory().commit().into(),
                    &tree_entry,
                    file.bytes().to_vec(),
                    512,
                )));
                header.push(format!(
                    "{} • literal bytes only • no file author guessed",
                    match &preview.kind {
                        crate::repository_view::BlobPreviewKind::Text => "Text",
                        crate::repository_view::BlobPreviewKind::Binary =>
                            "Binary bytes (not executed)",
                        crate::repository_view::BlobPreviewKind::Symlink =>
                            "Symlink target (not followed)",
                        crate::repository_view::BlobPreviewKind::Submodule =>
                            "Submodule reference (not fetched)",
                        crate::repository_view::BlobPreviewKind::LfsPointer { .. } =>
                            "LFS pointer only (object not downloaded)",
                    }
                ));
                for (index, line) in preview.rows.iter().enumerate() {
                    rows.push(RemoteBrowseRowV1 {
                        label: line.label.clone(),
                        index,
                        role: SemanticToken::Normal,
                        unavailable: None,
                    });
                }
                if preview.truncated {
                    header.push("Preview512lines only • full bounded blob hash verified".into());
                }
                RemoteBrowsePageV1::File {
                    original: file,
                    preview,
                }
            }
        };
        Ok(Self {
            owner,
            identity,
            canonical_name,
            rows: rows.into(),
            header: header.into(),
            actor,
            owner_actor,
            page,
            selected: 0,
            scroll: 0,
            generation,
            commit_editor: TextBuffer::default(),
            focus: 0,
            toolbar: 0,
            busy: false,
            status: "Fresh remote snapshot • no access lease".into(),
            hits: vec![],
            list_area: Default::default(),
        })
    }
    pub fn request(
        &self,
        action: &RemoteBrowseActionV1,
    ) -> Result<Option<RemoteRepositoryRequestV1>, String> {
        if self.busy {
            return Err("Remote read pending; no duplicate request".into());
        }
        Ok(Some(match action {
            RemoteBrowseActionV1::Branches => {
                RemoteRepositoryRequestV1::References(RemoteReferenceKindV1::Branch)
            }
            RemoteBrowseActionV1::Tags => {
                RemoteRepositoryRequestV1::References(RemoteReferenceKindV1::Tag)
            }
            RemoteBrowseActionV1::Commit => {
                RemoteRepositoryRequestV1::Commit(self.commit_editor.text.clone())
            }
            RemoteBrowseActionV1::Refresh => match &self.page {
                RemoteBrowsePageV1::Directory(original) => {
                    RemoteRepositoryRequestV1::ReloadDirectory {
                        original: original.clone(),
                    }
                }
                RemoteBrowsePageV1::References(c) => {
                    RemoteRepositoryRequestV1::References(c.kind())
                }
                RemoteBrowsePageV1::File { original, .. } => RemoteRepositoryRequestV1::File {
                    original: original.directory().clone(),
                    index: original.index(),
                },
            },
            RemoteBrowseActionV1::Open => {
                let row = self
                    .rows
                    .get(self.selected)
                    .ok_or("No observed remote choice")?;
                if let Some(reason) = row.unavailable {
                    return Err(reason.into());
                }
                match &self.page {
                    RemoteBrowsePageV1::References(catalogue) => {
                        RemoteRepositoryRequestV1::Reference {
                            catalogue: catalogue.clone(),
                            index: row.index,
                        }
                    }
                    RemoteBrowsePageV1::Directory(original) => {
                        if original.entries()[row.index].kind() == RemoteEntryKindV1::Directory {
                            RemoteRepositoryRequestV1::Directory {
                                original: original.clone(),
                                index: row.index,
                            }
                        } else {
                            RemoteRepositoryRequestV1::File {
                                original: original.clone(),
                                index: row.index,
                            }
                        }
                    }
                    RemoteBrowsePageV1::File { .. } => return Ok(None),
                }
            }
            _ => return Ok(None),
        }))
    }
    pub fn local(&mut self, action: &RemoteBrowseActionV1) -> Result<(), String> {
        match action {
            RemoteBrowseActionV1::Row(index) if *index < self.rows.len() => self.selected = *index,
            RemoteBrowseActionV1::Move(n) | RemoteBrowseActionV1::Page(n) => {
                self.selected = self
                    .selected
                    .saturating_add_signed(*n as isize)
                    .min(self.rows.len().saturating_sub(1))
            }
            RemoteBrowseActionV1::FocusNext(back) => {
                if self.focus != 2 {
                    self.focus = 2;
                    self.toolbar = if *back { 8 } else { 0 };
                } else if (*back && self.toolbar == 0) || (!*back && self.toolbar == 8) {
                    self.focus = 0;
                } else {
                    self.toolbar = self
                        .toolbar
                        .saturating_add_signed(if *back { -1 } else { 1 });
                }
            }
            RemoteBrowseActionV1::Toolbar(index) if *index < 9 => {
                self.focus = 2;
                self.toolbar = *index;
            }
            RemoteBrowseActionV1::CommitFocus => self.focus = 1,
            RemoteBrowseActionV1::CommitInsert(text) => {
                let selected = self
                    .commit_editor
                    .selection()
                    .map_or(0, |range| range.len());
                if text.len() > 64
                    || self.commit_editor.text.len() - selected + text.len() > 64
                    || !text.bytes().all(|b| b.is_ascii_hexdigit())
                {
                    return Err(
                        "Exact commit input requires at most64 hex bytes; no request sent".into(),
                    );
                }
                self.commit_editor.insert(text)?;
            }
            RemoteBrowseActionV1::CommitClear => self.commit_editor.clear(),
            RemoteBrowseActionV1::CommitSelectAll => self.commit_editor.select_all(),
            RemoteBrowseActionV1::CommitBackspace => self.commit_editor.delete_backward(),
            RemoteBrowseActionV1::CommitLeft => self.commit_editor.horizontal(false, false),
            RemoteBrowseActionV1::CommitRight => self.commit_editor.horizontal(true, false),
            _ => {}
        }
        self.clear_layout();
        Ok(())
    }
    pub fn clear_layout(&mut self) {
        self.hits.clear();
        self.list_area = Default::default();
    }
}

/// Display cap only. Original verified blob bytes are never truncated or
/// reused as a different canonical object; huge single lines stay labelled.
pub(crate) fn bounded_remote_preview_v1(mut preview: FilePreview) -> FilePreview {
    let mut bytes = 0usize;
    let mut rows = Vec::new();
    for mut row in std::mem::take(&mut preview.rows) {
        if row.label.len() > 4096 {
            let mut end = 4096;
            while !row.label.is_char_boundary(end) {
                end -= 1;
            }
            row.label.truncate(end);
            row.label.push_str(" [line preview clipped]");
            preview.truncated = true;
        }
        if bytes.saturating_add(row.label.len()) > 256 * 1024 || rows.len() >= 512 {
            preview.truncated = true;
            break;
        }
        bytes += row.label.len();
        rows.push(row);
    }
    preview.rows = rows;
    preview
}

#[cfg(test)]
#[path = "remote_repository_v1_tests.rs"]
mod tests;
