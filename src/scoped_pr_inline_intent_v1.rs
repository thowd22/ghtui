//! Original immutable range as navigation intent, NEVER publication authority.
//! The scoped worker independently verifies current source/storage/principal.
use super::{ClosedPrFileImagesV1, ReadOnlyCanonicalRangeV1};
use crate::{
    app::App,
    providers::gh::{DiffSide, PrImageSideV1},
    storage::Scope,
};
use sha2::{Digest, Sha256};
use std::sync::Arc;
#[path = "scoped_pr_native_inline_v1.rs"]
pub mod native_entry_v1;
#[path = "scoped_pr_inline_native_v1.rs"]
pub mod native_v1;
#[path = "scoped_pr_inline_plan_v1.rs"]
mod plan;
#[path = "scoped_pr_inline_ui_v1.rs"]
pub mod ui_v1;
pub use plan::{
    InlineFailureV1, ScopedInlineDraftV1, ScopedInlinePreviewV1, ScopedInlinePublicationStateV1,
    ScopedInlinePublicationV1,
};

const PATH_CAP: usize = 4096;

/// No JSON/restored constructor. Foreground capture retains only an opaque
/// original allocation and scalar coordinates; patch/body work stays workers.
pub struct ScopedInlineIntentV1 {
    proof: Arc<ClosedPrFileImagesV1>,
    side: PrImageSideV1,
    first: u32,
    last: u32,
}
impl std::fmt::Debug for ScopedInlineIntentV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedInlineIntentV1(<original read-only range; no publication permission>)")
    }
}
impl ScopedInlineIntentV1 {
    pub fn capture(range: &ReadOnlyCanonicalRangeV1) -> Result<Arc<Self>, String> {
        let proof = &range.proof;
        let file = proof
            .original_view
            .observed
            .files()
            .get(proof.file_index())
            .ok_or("Original inline file member unavailable")?;
        if range.first == 0
            || range.first > range.last
            || range.last - range.first > 9999
            || file.path.is_empty()
            || file.path.len() > PATH_CAP
            || range.path().is_empty()
            || range.path().len() > PATH_CAP
        {
            return Err(
                "Original inline range/path exceeds its bounded navigation contract".into(),
            );
        }
        Ok(Arc::new(Self {
            proof: proof.clone(),
            side: range.side,
            first: range.first,
            last: range.last,
        }))
    }
    pub fn owns(&self, app: &App) -> bool {
        self.proof.owns(app)
    }
    pub fn scope(&self) -> &Scope {
        self.proof.source_scope()
    }
    pub fn side(&self) -> DiffSide {
        match self.side {
            PrImageSideV1::Old => DiffSide::Left,
            PrImageSideV1::New => DiffSide::Right,
        }
    }
    pub fn start(&self) -> u32 {
        self.first
    }
    pub fn end(&self) -> u32 {
        self.last
    }
    pub fn review_head(&self) -> &str {
        self.proof.observed().parent().head_sha()
    }
    pub fn merge_base(&self) -> &str {
        self.proof.observed().merge_base()
    }
    /// Server path is the genuine PR-files filename, NOT the selected Old
    /// image lookup name. These differ for a renamed LEFT-side comment.
    pub fn server_path(&self) -> &str {
        &self.proof.original_view.observed.files()[self.proof.file_index()].path
    }
    pub fn selected_image_path(&self) -> &[u8] {
        self.proof.observed().image(self.side).path()
    }
    pub fn original_repository_id(&self) -> u64 {
        self.proof.observed().parent().repository_id()
    }
    pub fn original_pr_id(&self) -> u64 {
        self.proof.observed().parent().database_id()
    }
    pub fn original_pr_node(&self) -> &str {
        self.proof.observed().parent().node_id()
    }

    /// Worker-only stable metadata key: body edits do not move the draft.
    /// The durable publication intent independently fingerprints actual body.
    pub fn draft_key(&self) -> Result<String, String> {
        let parent = self.proof.observed().parent();
        let bytes = serde_json::to_vec(&(
            self.scope(),
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
            parent.node_id(),
            parent.number(),
            self.review_head(),
            parent.base_tip_sha(),
            self.merge_base(),
            self.server_path(),
            self.selected_image_path(),
            self.side(),
            self.first,
            self.last,
            self.proof.observed().patch_digest(),
        ))
        .map_err(|_| "Original inline anchor metadata cannot be encoded")?;
        if bytes.len() > 32 * 1024 {
            return Err("Original inline anchor metadata exceeds32KiB".into());
        }
        Ok(format!("inline/{:x}", Sha256::digest(bytes)))
    }

    /// Worker-only semantic preflight on the SAME existing canonical parser.
    /// Read-only context on Old remains browsable, but publication on LEFT
    /// ends on an actual removed line. Nothing silently changes side/line.
    fn validate_endpoint(&self) -> Result<(), String> {
        if !self
            .proof
            .observed()
            .range_admitted(self.side, self.first, self.last)
        {
            return Err(
                "Original range is outside one actual canonical hunk/side; select again".into(),
            );
        }
        let diff = self
            .proof
            .original_view
            .diff
            .as_ref()
            .ok_or("Original canonical patch unavailable")?;
        let mut matching = diff.files.iter().filter(|file| {
            file.new_path.as_deref().or(file.old_path.as_deref())
                == Some(self.server_path().as_bytes())
        });
        let file = matching
            .next()
            .filter(|_| matching.next().is_none())
            .ok_or("Original canonical file member is absent or ambiguous")?;
        if file.binary {
            return Err("Binary content cannot create a line comment".into());
        }
        let valid = file.hunks.iter().any(|hunk| {
            hunk.lines.iter().any(|line| match self.side {
                PrImageSideV1::Old => {
                    line.old_line == Some(self.last) && line.kind == crate::diff::LineKind::Removed
                }
                PrImageSideV1::New => {
                    line.new_line == Some(self.last)
                        && matches!(
                            line.kind,
                            crate::diff::LineKind::Added | crate::diff::LineKind::Context
                        )
                }
            })
        });
        if !valid {
            return Err("LEFT comments must end on a deleted line; unchanged context uses its actual RIGHT coordinate. No coordinate was converted.".into());
        }
        Ok(())
    }
}
