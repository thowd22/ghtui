//! Remote repository observations are produced by fresh gh reads, not serde.
use super::*;
use std::{sync::Arc, time::Duration};

#[derive(Clone, Debug)]
pub struct RemoteRepositoryLimitsV1 {
    pub max_pages: usize,
    pub max_refs: usize,
    pub max_directory_rows: usize,
    pub max_metadata_bytes: usize,
    pub max_blob_bytes: usize,
    pub max_response_bytes: usize,
    pub max_duration: Duration,
}
impl Default for RemoteRepositoryLimitsV1 {
    fn default() -> Self {
        Self {
            max_pages: 10,
            max_refs: 1000,
            max_directory_rows: 10000,
            max_metadata_bytes: 4 * 1024 * 1024,
            max_blob_bytes: 1024 * 1024,
            max_response_bytes: 16 * 1024 * 1024,
            max_duration: Duration::from_secs(30),
        }
    }
}
impl RemoteRepositoryLimitsV1 {
    pub(crate) fn validate(&self) -> GithubResult<()> {
        if !(1..=100).contains(&self.max_pages)
            || !(100..=10000).contains(&self.max_refs)
            || !(1..=20000).contains(&self.max_directory_rows)
            || !(1..=8 * 1024 * 1024).contains(&self.max_metadata_bytes)
            || !(1..=1024 * 1024).contains(&self.max_blob_bytes)
            || !(1024 * 1024..=64 * 1024 * 1024).contains(&self.max_response_bytes)
            || self.max_duration.is_zero()
            || self.max_duration > Duration::from_secs(30)
        {
            return Err(remote_error(
                GithubErrorKind::InvalidTarget,
                "Remote browsing requires explicit bounded reads",
            ));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteReferenceKindV1 {
    Branch,
    Tag,
}
impl RemoteReferenceKindV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::Branch => "Branch",
            Self::Tag => "Tag",
        }
    }
    pub(super) fn namespace(self) -> &'static str {
        match self {
            Self::Branch => "heads",
            Self::Tag => "tags",
        }
    }
    pub(super) fn endpoint(self) -> &'static str {
        match self {
            Self::Branch => "branches",
            Self::Tag => "tags",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteReferenceV1 {
    pub(super) name: String,
    pub(super) commit: String,
}
impl RemoteReferenceV1 {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn commit(&self) -> &str {
        &self.commit
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemoteEntryKindV1 {
    Directory,
    File,
    Executable,
    Symlink,
    Submodule,
}
impl RemoteEntryKindV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::Directory => "directory",
            Self::File => "file",
            Self::Executable => "executable file (not executed)",
            Self::Symlink => "symlink (not followed)",
            Self::Submodule => "submodule (not fetched)",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteEntryV1 {
    pub(super) name: String,
    pub(super) oid: String,
    pub(super) mode: String,
    pub(super) kind: RemoteEntryKindV1,
    pub(super) size: Option<u64>,
}
impl RemoteEntryV1 {
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn oid(&self) -> &str {
        &self.oid
    }
    pub fn kind(&self) -> RemoteEntryKindV1 {
        self.kind
    }
    pub fn size(&self) -> Option<u64> {
        self.size
    }
}
/// Private source continuity, not a reusable authenticated access lease.
pub(super) struct RemoteOriginV1 {
    pub original: Arc<RepositoryCatalogueRead>,
    pub reference: crate::repository_catalogue_intent_v1::IntentReference,
}
pub struct RemoteDirectoryV1 {
    pub(super) origin: Arc<RemoteOriginV1>,
    pub(super) commit: String,
    pub(super) root_tree: String,
    pub(super) tree: String,
    pub(super) path: String,
    pub(super) parent: Option<Arc<RemoteDirectoryV1>>,
    pub(super) entries: Arc<[RemoteEntryV1]>,
    pub(super) complete: bool,
}
impl RemoteDirectoryV1 {
    pub fn commit(&self) -> &str {
        &self.commit
    }
    pub fn tree(&self) -> &str {
        &self.tree
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn entries(&self) -> &[RemoteEntryV1] {
        &self.entries
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn parent(&self) -> Option<&Arc<Self>> {
        self.parent.as_ref()
    }
    pub fn reference(&self) -> &crate::repository_catalogue_intent_v1::IntentReference {
        &self.origin.reference
    }
    pub fn original(&self) -> &Arc<RepositoryCatalogueRead> {
        &self.origin.original
    }
}
impl std::fmt::Debug for RemoteDirectoryV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteDirectoryV1")
            .field("complete", &self.complete)
            .field("entries", &self.entries.len())
            .finish_non_exhaustive()
    }
}
pub struct RemoteReferenceCatalogueV1 {
    pub(super) origin: Arc<RemoteOriginV1>,
    pub(super) kind: RemoteReferenceKindV1,
    pub(super) entries: Arc<[RemoteReferenceV1]>,
    pub(super) complete: bool,
    pub(super) pages: usize,
}
impl RemoteReferenceCatalogueV1 {
    pub fn kind(&self) -> RemoteReferenceKindV1 {
        self.kind
    }
    pub fn entries(&self) -> &[RemoteReferenceV1] {
        &self.entries
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn pages(&self) -> usize {
        self.pages
    }
    pub fn original(&self) -> &Arc<RepositoryCatalogueRead> {
        &self.origin.original
    }
    pub fn reference(&self) -> &crate::repository_catalogue_intent_v1::IntentReference {
        &self.origin.reference
    }
}
impl std::fmt::Debug for RemoteReferenceCatalogueV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RemoteReferenceCatalogueV1")
            .field("complete_traversal", &self.complete)
            .field("pages", &self.pages)
            .finish_non_exhaustive()
    }
}
pub struct RemoteFileV1 {
    pub(super) directory: Arc<RemoteDirectoryV1>,
    pub(super) index: usize,
    pub(super) bytes: Arc<[u8]>,
    pub(super) path: String,
}
impl RemoteFileV1 {
    pub fn directory(&self) -> &Arc<RemoteDirectoryV1> {
        &self.directory
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn entry(&self) -> &RemoteEntryV1 {
        &self.directory.entries[self.index]
    }
}
impl std::fmt::Debug for RemoteFileV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RemoteFileV1(<verified bounded blob bytes>)")
    }
}
pub enum RemoteRepositoryRequestV1 {
    DefaultTree,
    Commit(String),
    References(RemoteReferenceKindV1),
    Reference {
        catalogue: Arc<RemoteReferenceCatalogueV1>,
        index: usize,
    },
    Directory {
        original: Arc<RemoteDirectoryV1>,
        index: usize,
    },
    ReloadDirectory {
        original: Arc<RemoteDirectoryV1>,
    },
    File {
        original: Arc<RemoteDirectoryV1>,
        index: usize,
    },
}
pub enum RemoteRepositoryPageV1 {
    Directory(Arc<RemoteDirectoryV1>),
    References(Arc<RemoteReferenceCatalogueV1>),
    File(Arc<RemoteFileV1>),
}
pub struct RemoteRepositoryObservationV1 {
    pub(super) identity: HostIdentity,
    pub(super) repository: RepositoryCatalogueRepository,
    pub(super) page: RemoteRepositoryPageV1,
    pub(super) accounting: ClosedCatalogueAccess,
}
/// A payload failure followed by the SAME original repository/user closure.
/// This is an honest failure observation, never a grant to retain old content.
pub struct RemoteRepositoryUnavailableV1 {
    pub(super) identity: HostIdentity,
    pub(super) repository: RepositoryCatalogueRepository,
    pub(super) kind: GithubErrorKind,
    pub(super) accounting: ClosedCatalogueAccess,
}
impl RemoteRepositoryUnavailableV1 {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn repository(&self) -> &RepositoryCatalogueRepository {
        &self.repository
    }
    pub fn kind(&self) -> &GithubErrorKind {
        &self.kind
    }
    pub fn accounting(&self) -> &ClosedCatalogueAccess {
        &self.accounting
    }
}
impl std::fmt::Debug for RemoteRepositoryUnavailableV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RemoteRepositoryUnavailableV1(<closed failure; no retained content>)")
    }
}
pub enum RemoteRepositoryReadOutcomeV1 {
    Observed(RemoteRepositoryObservationV1),
    Unavailable(RemoteRepositoryUnavailableV1),
}
impl RemoteRepositoryObservationV1 {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn repository(&self) -> &RepositoryCatalogueRepository {
        &self.repository
    }
    pub fn page(&self) -> &RemoteRepositoryPageV1 {
        &self.page
    }
    pub fn into_page(self) -> RemoteRepositoryPageV1 {
        self.page
    }
    pub fn accounting(&self) -> &ClosedCatalogueAccess {
        &self.accounting
    }
}
impl std::fmt::Debug for RemoteRepositoryObservationV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RemoteRepositoryObservationV1(<fresh closed read; not future authority>)")
    }
}
