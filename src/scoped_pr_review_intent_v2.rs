//! Original closed Files navigation, never review publication permission.
//! Serialized draft metadata is explicitly untrusted intent and cannot recreate
//! this source, an image, a principal, a preview or a write permit.
use super::*;
use crate::providers::runner::CancellationToken;
use crate::{
    app::{App, View},
    providers::gh::{DiffSide, InlineComment},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::time::Instant;
#[path = "scoped_pr_review_avatars_v7.rs"]
pub mod avatars_v7;
#[path = "scoped_pr_review_browser_v2.rs"]
pub mod browser_v2;
#[path = "scoped_pr_review_native_v2.rs"]
pub mod native_v2;
#[path = "scoped_pr_review_plan_v2.rs"]
mod plan;
#[path = "scoped_pr_review_ui_v2.rs"]
pub mod ui_v2;
pub use plan::{
    ReviewRequestedActionV2, ScopedReviewDraftV2, ScopedReviewFailureV2, ScopedReviewPreviewV2,
    ScopedReviewPublicationStateV2, ScopedReviewPublicationV2, ScopedReviewRequestV2,
    ScopedReviewSelectionV2,
};

pub const REVIEW_DRAFT_BYTES_V2: usize = 128 * 1024;
pub use plan::operations_v3;
pub const REVIEW_COMMENTS_V2: usize = 32;
pub const REVIEW_FILES_V2: usize = 8;

/// O(1) retained allocations from actual selected resource publication. This
/// remains navigation intent: every worker freshly proves the original source.
pub struct ScopedReviewIntentV2 {
    pub(super) source: Arc<CatalogueIntentSource>,
    pub(super) active: Arc<ActiveRemoteScopeV1>,
    pub(super) navigation: Arc<ScopedPrNavigationV1>,
    pub(super) files: Arc<ScopedPrResourceViewV1>,
}
impl std::fmt::Debug for ScopedReviewIntentV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedReviewIntentV2(<original closed Files; no approval>)")
    }
}
impl ScopedReviewIntentV2 {
    pub fn capture(app: &App) -> Result<Arc<Self>, String> {
        if app.view != View::Inbox {
            return Err("Open an actual scoped PR Files source first".into());
        }
        let source = app
            .remote_scope_source
            .as_ref()
            .ok_or("Original catalogue source unavailable")?;
        let active = app
            .remote_scope
            .as_ref()
            .ok_or("Original selected scope unavailable")?;
        let github = app
            .github
            .as_ref()
            .ok_or("Original scoped PR workspace unavailable")?;
        let navigation = github
            .scoped_pr_navigation
            .as_ref()
            .ok_or("Original scoped PR navigation unavailable")?;
        let files = github
            .scoped_pr_resource
            .as_ref()
            .filter(|files| files.observed.kind() == PrResourceKindV1::Files)
            .ok_or("Load the actual scoped Files section before preparing a review batch")?;
        if github.scope().ok().as_ref() != Some(source.scope())
            || navigation.scope() != source.scope()
            || github.pr.as_ref().is_none_or(|page| {
                page.tab != crate::github_workspace::PrTab::Files || !matches_page(navigation, page)
            })
            || files.observed.parent().repository_id() != navigation.original().id
        {
            return Err(
                "Original selected PR/source allocation is unavailable; no personal fallback"
                    .into(),
            );
        }
        Ok(Arc::new(Self {
            source: source.clone(),
            active: active.clone(),
            navigation: navigation.clone(),
            files: files.clone(),
        }))
    }
    pub fn owns(&self, app: &App) -> bool {
        app.view == View::Inbox
            && app
                .remote_scope_source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &self.source))
            && app
                .remote_scope
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &self.active))
            && app.github.as_ref().is_some_and(|github| {
                github.scope().ok().as_ref() == Some(self.scope())
                    && github
                        .scoped_pr_navigation
                        .as_ref()
                        .is_some_and(|nav| Arc::ptr_eq(nav, &self.navigation))
                    && github
                        .scoped_pr_resource
                        .as_ref()
                        .is_some_and(|files| Arc::ptr_eq(files, &self.files))
                    && github.pr.as_ref().is_some_and(|page| {
                        page.tab == crate::github_workspace::PrTab::Files
                            && matches_page(&self.navigation, page)
                    })
            })
    }
    pub fn scope(&self) -> &Scope {
        self.source.scope()
    }
    pub fn parent(&self) -> &crate::providers::gh::PrResourceParentV1 {
        self.files.observed.parent()
    }
    /// Worker-only bounded metadata digest, never foreground body serialization.
    /// Head-independent on purpose: an old-head draft can be restored honestly
    /// and explicitly re-anchored, never silently submitted on a different H.
    pub fn draft_key(&self) -> Result<String, String> {
        let parent = self.parent();
        if parent.node_id().len() > 1024
            || parent.repository_node().len() > 1024
            || self.scope().host.len() > 256
        {
            return Err("Original review target metadata exceeds its bounded key contract".into());
        }
        let bytes = serde_json::to_vec(&(
            self.scope(),
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
            parent.node_id(),
            parent.number(),
        ))
        .map_err(|_| "Review draft target metadata cannot be encoded")?;
        if bytes.len() > 8192 {
            return Err("Original review target metadata exceeds its bounded key contract".into());
        }
        Ok(format!("review-draft/{:x}", Sha256::digest(bytes)))
    }
}

/// Captured genuine canonical range. It is converted to scalar draft intent in
/// the worker, so key handling never scans a patch or clones retained bodies.
pub struct ScopedReviewAnchorIntentV2 {
    proof: Arc<ClosedPrFileImagesV1>,
    side: PrImageSideV1,
    first: u32,
    last: u32,
}
impl std::fmt::Debug for ScopedReviewAnchorIntentV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedReviewAnchorIntentV2(<actual canonical range; no write permit>)")
    }
}
impl ScopedReviewAnchorIntentV2 {
    pub fn capture(range: &ReadOnlyCanonicalRangeV1, app: &App) -> Result<Arc<Self>, String> {
        if app.view != View::Inbox
            || app
                .github
                .as_ref()
                .and_then(|github| github.pr.as_ref())
                .is_none_or(|page| page.tab != crate::github_workspace::PrTab::Files)
            || !range.proof.owns(app)
            || range.first == 0
            || range.first > range.last
            || range.last - range.first > 9999
        {
            return Err("Select an exact current canonical PR range; no native/decoded range can authorize a review".into());
        }
        Ok(Arc::new(Self {
            proof: range.proof.clone(),
            side: range.side,
            first: range.first,
            last: range.last,
        }))
    }
    pub fn owns(&self, app: &App) -> bool {
        app.view == View::Inbox
            && app
                .github
                .as_ref()
                .and_then(|github| github.pr.as_ref())
                .is_some_and(|page| page.tab == crate::github_workspace::PrTab::Files)
            && self.proof.owns(app)
    }
    pub fn matches_current_range(&self, app: &App) -> bool {
        self.owns(app)
            && app
                .pr_file_images
                .as_ref()
                .and_then(|view| view.canonical_range())
                .is_some_and(|range| {
                    Arc::ptr_eq(&self.proof, &range.proof)
                        && self.side == range.side
                        && self.first == range.first
                        && self.last == range.last
                })
    }
    pub fn prepare_metadata(
        &self,
        intent: &ScopedReviewIntentV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<ReviewAnchorMetadataV2, String> {
        let check = || {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                Err("Original review anchor operation ended".to_owned())
            } else {
                Ok(())
            }
        };
        check()?;
        let parent = self.proof.observed().parent();
        let source = intent.parent();
        if !Arc::ptr_eq(&self.proof.source, &intent.source)
            || !Arc::ptr_eq(&self.proof.active, &intent.active)
            || parent.repository_id() != source.repository_id()
            || parent.database_id() != source.database_id()
            || parent.node_id() != source.node_id()
            || parent.head_sha() != source.head_sha()
            || parent.base_tip_sha() != source.base_tip_sha()
            || self.proof.source_scope() != intent.scope()
        {
            return Err("Canonical range belongs to another original PR/source/head; retain text and re-anchor explicitly".into());
        }
        let file = self
            .proof
            .original_view
            .observed
            .files()
            .get(self.proof.file_index())
            .ok_or("Original review file member unavailable")?;
        if file.path.len() > 4096
            || self.proof.observed().image(self.side).path().len() > 4096
            || parent.head_sha().len() > 64
            || self.proof.observed().merge_base().len() > 64
            || self.proof.observed().patch_digest().len() > 64
        {
            return Err("Actual original anchor metadata exceeds bounds before copy".into());
        }
        let metadata = ReviewAnchorMetadataV2 {
            server_path: file.path.clone(),
            image_path: self.proof.observed().image(self.side).path().to_vec(),
            side: if self.side == PrImageSideV1::Old {
                DiffSide::Left
            } else {
                DiffSide::Right
            },
            first: self.first,
            last: self.last,
            head: parent.head_sha().into(),
            merge_base: self.proof.observed().merge_base().into(),
            patch_digest: self.proof.observed().patch_digest().into(),
        };
        metadata.validate()?;
        // SAME provider endpoint policy: LEFT must end Removed; RIGHT must end
        // Added/Context. This uses the retained canonical patch, not new parser.
        let diff = self
            .proof
            .original_view
            .canonical_diff()
            .ok_or("Original canonical patch unavailable")?;
        let mut matching = None;
        for file in &diff.files {
            check()?;
            if file.new_path.as_deref().or(file.old_path.as_deref())
                == Some(metadata.server_path.as_bytes())
                && matching.replace(file).is_some()
            {
                return Err("Original canonical file is ambiguous".into());
            }
        }
        let file = matching.ok_or("Original canonical file is absent")?;
        let mut endpoint = false;
        for hunk in &file.hunks {
            check()?;
            for line in &hunk.lines {
                check()?;
                endpoint |= match metadata.side {
                    DiffSide::Left => {
                        line.old_line == Some(self.last)
                            && line.kind == crate::diff::LineKind::Removed
                    }
                    DiffSide::Right => {
                        line.new_line == Some(self.last)
                            && matches!(
                                line.kind,
                                crate::diff::LineKind::Added | crate::diff::LineKind::Context
                            )
                    }
                };
            }
        }
        if file.binary
            || !self
                .proof
                .observed()
                .range_admitted(self.side, self.first, self.last)
            || !endpoint
        {
            return Err("Choose a deleted LEFT or added/context RIGHT endpoint".into());
        }
        check()?;
        Ok(metadata)
    }
}

/// UNTRUSTED saved navigation metadata only. No public constructor upgrades it
/// to genuine source/preview authority; every use rehydrates actual H/M images.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewAnchorMetadataV2 {
    pub(super) server_path: String,
    pub(super) image_path: Vec<u8>,
    pub(super) side: DiffSide,
    pub(super) first: u32,
    pub(super) last: u32,
    pub(super) head: String,
    pub(super) merge_base: String,
    pub(super) patch_digest: String,
}
impl ReviewAnchorMetadataV2 {
    pub fn server_path(&self) -> &str {
        &self.server_path
    }
    pub fn side(&self) -> DiffSide {
        self.side
    }
    pub fn first(&self) -> u32 {
        self.first
    }
    pub fn last(&self) -> u32 {
        self.last
    }
    pub fn head(&self) -> &str {
        &self.head
    }
    pub fn validate(&self) -> Result<(), String> {
        let sha = |text: &str, sizes: &[usize]| {
            sizes.contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_hexdigit())
        };
        if self.server_path.is_empty()
            || self.server_path.len() > 4096
            || self.server_path.starts_with('/')
            || self.server_path.contains('\0')
            || self
                .server_path
                .split('/')
                .any(|part| matches!(part, "" | "." | ".."))
            || self.image_path.is_empty()
            || self.image_path.len() > 4096
            || self.image_path.contains(&0)
            || self.first == 0
            || self.first > self.last
            || self.last - self.first > 9999
            || !sha(&self.head, &[40, 64])
            || !sha(&self.merge_base, &[40, 64])
            || !sha(&self.patch_digest, &[64])
        {
            return Err("Saved review anchor metadata is malformed/bounded; no coordinates were restored as permission".into());
        }
        Ok(())
    }
    pub(super) fn comment(&self, body: &str) -> Result<InlineComment, String> {
        self.validate()?;
        Ok(InlineComment {
            path: self.server_path.clone(),
            side: self.side,
            line: u64::from(self.last),
            start_line: (self.first != self.last).then_some(u64::from(self.first)),
            start_side: (self.first != self.last).then_some(self.side),
            body: body.into(),
        })
    }
}

#[derive(Clone, Debug)]
pub struct ReviewDraftCommentV2 {
    pub(super) anchor: Arc<ReviewAnchorMetadataV2>,
    pub(super) body: Arc<str>,
}
impl ReviewDraftCommentV2 {
    pub fn anchor(&self) -> &ReviewAnchorMetadataV2 {
        &self.anchor
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn prepare(
        anchor: &ScopedReviewAnchorIntentV2,
        intent: &ScopedReviewIntentV2,
        body: Arc<str>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self, String> {
        if body.trim().is_empty() || body.len() > 65_536 {
            return Err("Review comment needs nonempty text within65536 UTF8 bytes".into());
        }
        Ok(Self {
            anchor: Arc::new(anchor.prepare_metadata(intent, cancel, deadline)?),
            body,
        })
    }
}

#[derive(Clone, Debug)]
pub struct ReviewDraftValuesV2 {
    pub(super) summary: Arc<str>,
    pub(super) comments: Arc<[ReviewDraftCommentV2]>,
}
impl ReviewDraftValuesV2 {
    pub fn empty() -> Self {
        Self {
            summary: Arc::from(""),
            comments: Arc::from([]),
        }
    }
    pub fn summary(&self) -> &str {
        &self.summary
    }
    pub fn comments(&self) -> &[ReviewDraftCommentV2] {
        &self.comments
    }
    pub fn replace(
        summary: Arc<str>,
        comments: Arc<[ReviewDraftCommentV2]>,
    ) -> Result<Self, String> {
        let values = Self { summary, comments };
        values.validate()?;
        Ok(values)
    }
    pub fn validate(&self) -> Result<(), String> {
        Self::validate_borrowed_v10(
            &self.summary,
            self.comments
                .iter()
                .map(|comment| (comment.anchor.as_ref(), comment.body.as_ref())),
        )
    }
    /// Same exact size/anchor rules for LOCAL edited values, before allocating
    /// replacement body Arcs. This does not issue canonical range authority.
    pub(super) fn validate_borrowed_v10<'a>(
        summary: &str,
        comments: impl IntoIterator<Item = (&'a ReviewAnchorMetadataV2, &'a str)>,
    ) -> Result<(), String> {
        if summary.len() > 65_536 {
            return Err("Review draft exceeds65536summary bytes or32comments".into());
        }
        let mut files = BTreeSet::new();
        let mut bytes = summary.len();
        fn quoted(text: &str) -> Option<usize> {
            text.bytes().try_fold(2usize, |bytes, byte| {
                bytes.checked_add(match byte {
                    b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
                    0..=31 => 6,
                    _ => 1,
                })
            })
        }
        // Checked BEFORE JSON objects, array/body clones or encoding. The fixed
        // envelope covers keys/punctuation/range numbers and final reference.
        let mut encoded = quoted(summary)
            .and_then(|bytes| bytes.checked_add(1024))
            .ok_or("Review encoded summary length overflow")?;
        let mut count = 0;
        for (anchor, body) in comments {
            count += 1;
            if count > REVIEW_COMMENTS_V2 {
                return Err("Review draft exceeds32comments; nothing truncated".into());
            }
            anchor.validate()?;
            files.insert(anchor.server_path.as_str());
            bytes = bytes
                .checked_add(body.len())
                .ok_or("Review body length overflow")?;
            let image_bytes = anchor.image_path.iter().try_fold(2usize, |bytes, byte| {
                bytes.checked_add(if *byte >= 100 {
                    4
                } else if *byte >= 10 {
                    3
                } else {
                    2
                })
            });
            let anchor_bytes = quoted(&anchor.server_path)
                .and_then(|bytes| bytes.checked_add(quoted(&anchor.head)?))
                .and_then(|bytes| bytes.checked_add(quoted(&anchor.merge_base)?))
                .and_then(|bytes| bytes.checked_add(quoted(&anchor.patch_digest)?))
                .and_then(|bytes| bytes.checked_add(image_bytes?))
                .and_then(|bytes| bytes.checked_add(quoted(body)?))
                .and_then(|bytes| bytes.checked_add(512));
            encoded = encoded
                .checked_add(anchor_bytes.ok_or("Review encoded comment length overflow")?)
                .ok_or("Review encoded draft length overflow")?;
            if body.trim().is_empty()
                || body.len() > 65_536
                || files.len() > REVIEW_FILES_V2
                || bytes > REVIEW_DRAFT_BYTES_V2
                || encoded > REVIEW_DRAFT_BYTES_V2
            {
                return Err(
                    "Review draft exceeds nonempty bodies/8files/128KiB; nothing was truncated"
                        .into(),
                );
            }
        }
        if encoded > REVIEW_DRAFT_BYTES_V2 {
            return Err("Review summary exceeds128KiB after bounded JSON preflight".into());
        }
        Ok(())
    }
    /// Worker-only real encoded-size check. Call validate BEFORE serialization;
    /// final complete envelope size is checked again before saving or POST.
    pub fn encoded_values(&self) -> Result<String, String> {
        self.validate()?;
        let comments = self.comments.iter().map(|comment|serde_json::json!({"anchor":comment.anchor.as_ref(),"body":comment.body.as_ref()})).collect::<Vec<_>>();
        let encoded =
            serde_json::json!({"summary":self.summary.as_ref(),"comments":comments}).to_string();
        if encoded.len() > REVIEW_DRAFT_BYTES_V2 {
            return Err("Review draft exceeds128KiB after actual JSON encoding".into());
        }
        Ok(encoded)
    }
    pub fn decode_values(encoded: &str) -> Result<Self, String> {
        if encoded.len() > REVIEW_DRAFT_BYTES_V2 {
            return Err("Saved review draft exceeds128KiB before parsing".into());
        }
        let value: serde_json::Value = serde_json::from_str(encoded)
            .map_err(|_| "Malformed saved review text; no proof restored")?;
        let object = value
            .as_object()
            .filter(|object| {
                object.len() == 2
                    && object.contains_key("summary")
                    && object.contains_key("comments")
            })
            .ok_or("Saved review values contain unknown or missing fields")?;
        let summary = object["summary"]
            .as_str()
            .filter(|text| text.len() <= 65_536)
            .ok_or("Saved review summary exceeds bound")?;
        let comments = object["comments"]
            .as_array()
            .filter(|comments| comments.len() <= REVIEW_COMMENTS_V2)
            .ok_or("Saved review contains too many comments; no partial restore")?;
        let mut restored = Vec::new();
        for comment in comments {
            let fields = comment
                .as_object()
                .filter(|fields| {
                    fields.len() == 2
                        && fields.contains_key("anchor")
                        && fields.contains_key("body")
                })
                .ok_or("Saved review comment contains unknown or missing fields")?;
            let body = fields["body"]
                .as_str()
                .filter(|body| !body.trim().is_empty() && body.len() <= 65_536)
                .ok_or("Saved review comment body exceeds bound")?;
            let metadata = fields["anchor"]
                .as_object()
                .filter(|metadata| metadata.len() == 8)
                .ok_or("Saved anchor has unknown or missing fields")?;
            for (field, cap) in [
                ("server_path", 4096),
                ("head", 64),
                ("merge_base", 64),
                ("patch_digest", 64),
            ] {
                metadata
                    .get(field)
                    .and_then(serde_json::Value::as_str)
                    .filter(|text| text.len() <= cap)
                    .ok_or("Saved anchor metadata exceeds bound before clone")?;
            }
            metadata
                .get("image_path")
                .and_then(serde_json::Value::as_array)
                .filter(|path| {
                    path.len() <= 4096
                        && path
                            .iter()
                            .all(|byte| byte.as_u64().is_some_and(|byte| byte <= 255))
                })
                .ok_or("Saved raw image path exceeds bound before clone")?;
            metadata
                .get("side")
                .and_then(serde_json::Value::as_str)
                .filter(|side| matches!(*side, "LEFT" | "RIGHT"))
                .ok_or("Saved side is unknown before clone")?;
            for field in ["first", "last"] {
                metadata
                    .get(field)
                    .and_then(serde_json::Value::as_u64)
                    .filter(|line| *line > 0 && *line <= u64::from(u32::MAX))
                    .ok_or("Saved line coordinate exceeds bound before clone")?;
            }
            let anchor: ReviewAnchorMetadataV2 =
                serde_json::from_value(fields["anchor"].clone())
                    .map_err(|_| "Malformed saved anchor metadata; no approval restored")?;
            anchor.validate()?;
            restored.push(ReviewDraftCommentV2 {
                anchor: Arc::new(anchor),
                body: Arc::from(body),
            });
        }
        Self::replace(Arc::from(summary), restored.into())
    }
}
