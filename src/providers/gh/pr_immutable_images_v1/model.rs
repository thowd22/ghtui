use super::*;

#[derive(Clone, Debug)]
pub struct PrImmutableFileLimitsV1 {
    pub max_parent_bytes: usize,
    pub max_document_bytes: usize,
    pub max_blob_bytes: usize,
    pub max_pool_bytes: usize,
    pub max_path_depth: usize,
    pub max_tree_rows: usize,
    pub max_aggregate_tree_rows: usize,
    pub max_tree_object_bytes: usize,
}
impl Default for PrImmutableFileLimitsV1 {
    fn default() -> Self {
        Self {
            max_parent_bytes: 128 * 1024,
            max_document_bytes: 1024 * 1024,
            max_blob_bytes: 512 * 1024,
            max_pool_bytes: 8 * 1024 * 1024,
            max_path_depth: 32,
            max_tree_rows: 4096,
            max_aggregate_tree_rows: 16384,
            max_tree_object_bytes: 2 * 1024 * 1024,
        }
    }
}
impl PrImmutableFileLimitsV1 {
    pub fn validate(&self) -> GithubResult<()> {
        if !(1..=1024 * 1024).contains(&self.max_parent_bytes)
            || !(1..=1024 * 1024).contains(&self.max_document_bytes)
            || !(1..=1024 * 1024).contains(&self.max_blob_bytes)
            || !(1..=32 * 1024 * 1024).contains(&self.max_pool_bytes)
            || !(1..=32).contains(&self.max_path_depth)
            || !(1..=4096).contains(&self.max_tree_rows)
            || !(1..=16384).contains(&self.max_aggregate_tree_rows)
            || !(1..=2 * 1024 * 1024).contains(&self.max_tree_object_bytes)
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        Ok(())
    }
}
/// Private original selection, not a raw DTO/current permission constructor.
pub struct PrImmutableFileOriginalV1 {
    pub(super) scope: crate::storage::Scope,
    pub(super) observed: Arc<PrResourceReadV1>,
    pub(super) metadata: ObservedPrFileV1,
    pub(super) selected: SelectedPatchV1,
    pub(super) file_index: usize,
}
impl std::fmt::Debug for PrImmutableFileOriginalV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrImmutableFileOriginalV1(<actual selected source; not permission>)")
    }
}
impl PrImmutableFileOriginalV1 {
    pub fn max_external_requests(&self) -> usize {
        7 + self.selected.old_lookup.split(|byte| *byte == b'/').count()
            + self.selected.new_lookup.split(|byte| *byte == b'/').count()
    }
    pub fn scope(&self) -> &crate::storage::Scope {
        &self.scope
    }
    pub fn original_parent(&self) -> &PrResourceParentV1 {
        self.observed.parent()
    }
    pub fn file_index(&self) -> usize {
        self.file_index
    }
    /// Borrowed SAME actually parsed selected patch; not a source constructor,
    /// image proof, mutation anchor or permission on its own.
    pub(crate) fn canonical_file_v14(&self) -> &crate::diff::FileDiff {
        &self.selected.file
    }
    pub(crate) fn patch_digest_v14(&self) -> &str {
        &self.selected.digest
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrImmutableImageKindV1 {
    Text,
    Binary,
    Symlink,
    Submodule,
    LfsPointer,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrImageSideV1 {
    Old,
    New,
}
pub struct PrImmutableImageV1 {
    pub(super) path: Vec<u8>,
    pub(super) blob: Option<String>,
    pub(super) mode: Option<String>,
    pub(super) bytes: Option<Vec<u8>>,
    pub(super) kind: Option<PrImmutableImageKindV1>,
    pub(super) absent: bool,
}
impl std::fmt::Debug for PrImmutableImageV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrImmutableImageV1")
            .field("kind", &self.kind)
            .field("absence_verified", &self.absent)
            .finish_non_exhaustive()
    }
}
impl PrImmutableImageV1 {
    pub fn path(&self) -> &[u8] {
        &self.path
    }
    pub fn blob_sha(&self) -> Option<&str> {
        self.blob.as_deref()
    }
    pub fn mode(&self) -> Option<&str> {
        self.mode.as_deref()
    }
    pub fn bytes(&self) -> Option<&[u8]> {
        self.bytes.as_deref()
    }
    pub fn kind(&self) -> Option<PrImmutableImageKindV1> {
        self.kind
    }
    pub fn absent(&self) -> bool {
        self.absent
    }
}
pub(super) struct SelectedPatchV1 {
    pub raw: Arc<[u8]>,
    pub old_path: Option<Vec<u8>>,
    pub new_path: Option<Vec<u8>>,
    pub old_lookup: Vec<u8>,
    pub new_lookup: Vec<u8>,
    pub file: Arc<crate::diff::FileDiff>,
    pub digest: String,
}
/// Produced ONLY from actual current API/tree/blob traversal; still intermediate
/// until original session closes repository, local epoch and principal guards.
pub struct PrImmutableFileReadV1 {
    pub(super) selected_raw_patch: Arc<[u8]>,
    pub(super) canonical_file: Arc<crate::diff::FileDiff>,
    pub(super) parent: PrResourceParentV1,
    pub(super) merge_base: String,
    pub(super) head_tree: String,
    pub(super) old_tree: String,
    pub(super) old: PrImmutableImageV1,
    pub(super) new: PrImmutableImageV1,
    pub(super) association: bool,
    pub(super) file_index: usize,
    pub(super) patch_digest: String,
    pub(super) raw_tree_rows: usize,
    pub(super) tree_reads: usize,
    pub(super) old_ranges: Vec<(u32, u32)>,
    pub(super) new_ranges: Vec<(u32, u32)>,
}
impl std::fmt::Debug for PrImmutableFileReadV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrImmutableFileReadV1(<server-linked trees/digest-verified bytes; no write authority>)")
    }
}
impl PrImmutableFileReadV1 {
    pub(crate) fn selected_raw_patch_v4(&self) -> &[u8] {
        &self.selected_raw_patch
    }
    /// The SAME parsed member whose raw patch was associated byte-for-byte
    /// with both verified images. No display reparse or synthetic coordinates.
    pub(crate) fn canonical_file_v1(&self) -> &crate::diff::FileDiff {
        &self.canonical_file
    }
    pub(crate) fn retained_content_bytes_v1(&self) -> usize {
        // Owned observation material only. Existing original Files/page/source
        // Arcs are not recursively included or cloned by native projection.
        let file = &self.canonical_file;
        let canonical = std::mem::size_of::<crate::diff::FileDiff>()
            + file.old_path.as_ref().map_or(0, Vec::capacity)
            + file.new_path.as_ref().map_or(0, Vec::capacity)
            + file.metadata.capacity() * std::mem::size_of::<Vec<u8>>()
            + file.metadata.iter().map(Vec::capacity).sum::<usize>()
            + file.hunks.capacity() * std::mem::size_of::<crate::diff::Hunk>()
            + file
                .hunks
                .iter()
                .map(|hunk| {
                    hunk.heading.capacity()
                        + hunk.lines.capacity() * std::mem::size_of::<crate::diff::DiffLine>()
                        + hunk
                            .lines
                            .iter()
                            .map(|line| line.text.capacity())
                            .sum::<usize>()
                })
                .sum::<usize>();
        [self.old.bytes.as_ref(), self.new.bytes.as_ref()]
            .into_iter()
            .flatten()
            .map(Vec::capacity)
            .sum::<usize>()
            + self.old.path.capacity()
            + self.new.path.capacity()
            + self.patch_digest.capacity()
            + self.selected_raw_patch.len()
            + 64
            + self.merge_base.capacity()
            + self.head_tree.capacity()
            + self.old_tree.capacity()
            + [
                &self.old.blob,
                &self.new.blob,
                &self.old.mode,
                &self.new.mode,
            ]
            .into_iter()
            .map(|value| value.as_ref().map_or(0, String::capacity))
            .sum::<usize>()
            + self.parent.retained_heap_bytes_v2()
            + self.old_ranges.capacity() * std::mem::size_of::<(u32, u32)>()
            + self.new_ranges.capacity() * std::mem::size_of::<(u32, u32)>()
            + canonical
            + std::mem::size_of::<Self>()
            + 64
    }
    pub fn parent(&self) -> &PrResourceParentV1 {
        &self.parent
    }
    pub fn merge_base(&self) -> &str {
        &self.merge_base
    }
    pub fn head_tree(&self) -> &str {
        &self.head_tree
    }
    pub fn old_tree(&self) -> &str {
        &self.old_tree
    }
    pub fn image(&self, side: PrImageSideV1) -> &PrImmutableImageV1 {
        if side == PrImageSideV1::Old {
            &self.old
        } else {
            &self.new
        }
    }
    pub fn patch_associated(&self) -> bool {
        self.association
    }
    pub fn file_index(&self) -> usize {
        self.file_index
    }
    pub fn patch_digest(&self) -> &str {
        &self.patch_digest
    }
    pub fn raw_tree_rows(&self) -> usize {
        self.raw_tree_rows
    }
    pub fn tree_reads(&self) -> usize {
        self.tree_reads
    }
    pub fn range_admitted(&self, side: PrImageSideV1, first: u32, last: u32) -> bool {
        self.association
            && first > 0
            && first <= last
            && (if side == PrImageSideV1::Old {
                &self.old_ranges
            } else {
                &self.new_ranges
            })
            .iter()
            .any(|(start, end)| first >= *start && last <= *end)
    }
}
