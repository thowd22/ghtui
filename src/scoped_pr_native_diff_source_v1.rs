//! Genuine selected PR source for the ONE native engine. Remote observations
//! are never represented as a local Workspace/ComparisonResult or write lease.
use crate::{
    app::App,
    diff::FileDiff,
    git::RawPath,
    identity::{ActorIdentity, ActorKind},
    native_diff_operation_v3::{self, NativeDiffCheckV3},
    providers::{
        gh::{PrImageSideV1, PrImmutableImageKindV1},
        runner::CancellationToken,
    },
    repository_native_diff_content_v2::{
        self, CapturedNativeDiffPurposeV2, NativeDiffContentV2, NativeDiffPurposeV2,
    },
    repository_native_full_side_v3::NativeFullSideMaterialV3,
    repository_view::{BlobPreviewKind, DiffSide},
    scoped_pr_observed_tabs_v1::resources::immutable_images::ClosedPrFileImagesV1,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};

/// Original request CPU checks only. No provider, read window or mutation API.
pub struct PrNativePreparationV1 {
    cancel: CancellationToken,
    deadline: Instant,
    sealed: AtomicBool,
}
impl PrNativePreparationV1 {
    pub(crate) fn new(deadline: Instant, cancel: CancellationToken) -> Result<Self, String> {
        if deadline
            .checked_duration_since(Instant::now())
            .is_none_or(|left| left.is_zero() || left > Duration::from_secs(30))
        {
            return Err("PR native preparation needs its original <=30s deadline".into());
        }
        let result = Self {
            cancel,
            deadline,
            sealed: AtomicBool::new(false),
        };
        result.check()?;
        Ok(result)
    }
    pub(crate) fn seal(&self) -> Result<(), String> {
        self.check()?;
        self.sealed.store(true, Ordering::Release);
        Ok(())
    }
}
impl native_diff_operation_v3::sealed::Sealed for PrNativePreparationV1 {}
impl NativeDiffCheckV3 for PrNativePreparationV1 {
    fn check(&self) -> Result<(), String> {
        if self.cancel.is_cancelled()
            || self.sealed.load(Ordering::Acquire)
            || Instant::now() >= self.deadline
        {
            return Err(
                "Original PR native preparation cancelled, expired or sealed; no clock renewal"
                    .into(),
            );
        }
        Ok(())
    }
}

pub struct ScopedPrNativeIntentV1 {
    proof: Arc<ClosedPrFileImagesV1>,
}
impl std::fmt::Debug for ScopedPrNativeIntentV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedPrNativeIntentV1(<original verified PR file; not permission>)")
    }
}
pub struct ScopedPrNativeSourceV1 {
    intent: Arc<ScopedPrNativeIntentV1>,
    old_path: Option<RawPath>,
    new_path: Option<RawPath>,
    label: String,
}
impl std::fmt::Debug for ScopedPrNativeSourceV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedPrNativeSourceV1(<one genuine verified PR member>)")
    }
}
impl ScopedPrNativeSourceV1 {
    pub(crate) fn from_proof(
        proof: Arc<ClosedPrFileImagesV1>,
        operation: &PrNativePreparationV1,
    ) -> Result<Arc<Self>, String> {
        operation.check()?;
        let intent = Arc::new(ScopedPrNativeIntentV1 { proof });
        Self::read(intent, operation)
    }
    pub fn proof(&self) -> &Arc<ClosedPrFileImagesV1> {
        &self.intent.proof
    }
    pub(crate) fn source_details(&self) -> String {
        let read = self.proof().observed();
        let p = read.parent();
        let details=format!("Native PR diff • ONE verified selected file\nHost: {}\nNumeric account: {}\nRepository: {} • ID {} • node {}\nPR: #{} • ID {} • node {}\nRequested base tip: {}\nActual OLD common starting point M: {}\nReview HEAD H / NEW: {}\nPatch digest: {}\nOriginal Files member: {} • parent-reported changed-file count: {:?}\nNo local checkout, staging or publication authority. Return to original Files for other members.\nOLD raw path: {}\nNEW raw path: {}", self.proof().source_scope().host, self.proof().source_scope().login, p.repository(), p.repository_id(), p.repository_node(), p.number(), p.database_id(), p.node_id(), p.base_tip_sha(), read.merge_base(), p.head_sha(), read.patch_digest(), self.proof().file_index(), p.changed_files(), crate::git::display_path_bytes(read.image(PrImageSideV1::Old).path()), crate::git::display_path_bytes(read.image(PrImageSideV1::New).path()));
        if details.chars().count() > 15_000 {
            format!("{}\n[Source display bound reached; path suffix omitted, not an alternate path or approval]",details.chars().take(14_800).collect::<String>())
        } else {
            details
        }
    }
}
impl repository_native_diff_content_v2::sealed::Sealed for ScopedPrNativeSourceV1 {}
impl NativeDiffContentV2 for ScopedPrNativeSourceV1 {
    fn canonical_file(&self, file: usize) -> Option<&FileDiff> {
        (file == 0).then(|| self.proof().observed().canonical_file_v1())
    }
    fn content_endpoints(&self) -> (Option<&str>, Option<&str>) {
        (
            Some(self.proof().observed().merge_base()),
            Some(self.proof().observed().parent().head_sha()),
        )
    }
    fn member_count(&self) -> usize {
        1
    }
    fn member_index(&self, file: usize) -> Option<usize> {
        (file == 0).then_some(0)
    }
    fn member_path(&self, file: usize, side: DiffSide) -> Option<&RawPath> {
        if file != 0 {
            return None;
        }
        match side {
            DiffSide::Old => self.old_path.as_ref(),
            DiffSide::New => self.new_path.as_ref(),
            DiffSide::Both => None,
        }
    }
}
impl NativeDiffPurposeV2 for ScopedPrNativeSourceV1 {
    fn external_patch_v4(&self) -> crate::repository_native_external_v4::NativeExternalPatchV4<'_> {
        crate::repository_native_external_v4::NativeExternalPatchV4::pr(self)
    }
    fn supports_inline_comment() -> bool {
        true
    }
    fn supports_review_threads() -> bool {
        true
    }
    type Context = App;
    type Operation = PrNativePreparationV1;
    type Intent = ScopedPrNativeIntentV1;
    fn capture(app: &App) -> Result<CapturedNativeDiffPurposeV2<Self>, String> {
        let source = app
            .pr_native_diff_v1
            .as_ref()
            .and_then(|state| state.source())
            .filter(|source| source.proof().owns(app))
            .cloned()
            .ok_or("Open the selected PR file through the guarded image/native read")?;
        Ok((source.intent.clone(), Some(source)))
    }
    fn intent(&self) -> &Arc<Self::Intent> {
        &self.intent
    }
    fn intent_owns(intent: &Self::Intent, app: &App) -> bool {
        intent.proof.owns(app)
    }
    fn source_owns(source: &Arc<Self>, app: &App) -> bool {
        source.proof().owns(app)
    }
    fn read(intent: Arc<Self::Intent>, operation: &Self::Operation) -> Result<Arc<Self>, String> {
        operation.check()?;
        let file = intent.proof.observed().canonical_file_v1();
        if file.old_path.as_ref().is_some_and(|p| p.len() > 4096)
            || file.new_path.as_ref().is_some_and(|p| p.len() > 4096)
        {
            return Err("Verified PR member exceeds raw-path bound".into());
        }
        let old_path = file.old_path.clone().map(RawPath);
        let new_path = file.new_path.clone().map(RawPath);
        let label = crate::git::display_path_bytes(
            file.new_path
                .as_deref()
                .or(file.old_path.as_deref())
                .ok_or("Verified PR file has neither side")?,
        );
        let result = Arc::new(Self {
            intent,
            old_path,
            new_path,
            label,
        });
        operation.check()?;
        Ok(result)
    }
    fn validate_current(&self, operation: &Self::Operation) -> Result<(), String> {
        operation.check()
    }
    fn validate_full_side_v3(
        &self,
        material: &NativeFullSideMaterialV3,
        operation: &Self::Operation,
    ) -> Result<(), String> {
        operation.check()?;
        if !material.is_original_pr_v1(self.proof()) {
            return Err("Full-file material is not this exact original PR image proof".into());
        }
        Ok(())
    }
    fn prepare_full_side_v3(
        self: &Arc<Self>,
        file: usize,
        side: DiffSide,
        operation: &Self::Operation,
    ) -> Result<Arc<NativeFullSideMaterialV3>, String> {
        operation.check()?;
        if file != 0 {
            return Err("Only this verified selected PR member is available".into());
        }
        let side = match side {
            DiffSide::Old => PrImageSideV1::Old,
            DiffSide::New => PrImageSideV1::New,
            DiffSide::Both => return Err("Choose explicit OLD or NEW PR image".into()),
        };
        let image = self.proof().observed().image(side);
        let kind = if image.absent() {
            None
        } else {
            Some(crate::repository_view::file_content_kind_v3(
                image.mode().unwrap_or(""),
                image.kind() == Some(PrImmutableImageKindV1::Submodule),
                image.bytes().unwrap_or(&[]),
            ))
        };
        let material = Arc::new(ScopedPrNativeSideV1 {
            proof: self.proof().clone(),
            side,
            kind,
            label: format!(
                "Verified PR {} • {} • H {} • M {} • read-only, not current permission",
                if side == PrImageSideV1::Old {
                    "OLD"
                } else {
                    "NEW"
                },
                image.blob_sha().unwrap_or("ABSENT: no blob"),
                self.proof().observed().parent().head_sha(),
                self.proof().observed().merge_base()
            ),
        });
        operation.check()?;
        Ok(NativeFullSideMaterialV3::remote_pr_v1(material))
    }
    fn complete(&self) -> bool {
        true
    }
    fn observed_members(&self) -> usize {
        1
    }
    fn member_label(&self, file: usize) -> Option<&str> {
        (file == 0).then_some(self.label.as_str())
    }
    fn closed_status() -> &'static str {
        "Native PR diff closed • original verified images retained • no writes"
    }
    fn pane_title(wide: bool) -> &'static str {
        if wide {
            "Native PR diff • one verified file • bounded 512-column pane"
        } else {
            "Native PR diff • one verified selected file"
        }
    }
    fn stage_control() -> &'static str {
        "[a Stage disabled]"
    }
    fn stage_unavailable() -> &'static str {
        "Stage disabled: a remote PR image has no local index/worktree authority. No Git or GitHub mutation was queued."
    }
    fn selection_help() -> &'static str {
        "Explicit single-side canonical patch ranges can open P Comment for fresh verification; full-file/Both positions cannot. No publication approval is created by selection."
    }
    fn ready_status() -> &'static str {
        "Native PR diff ready • one verified file • original M/H • read-only"
    }
    fn pane_header(&self) -> String {
        format!(
            "{}#{} • M {} → H {} • one selected file; other files via Back",
            self.proof().observed().parent().repository(),
            self.proof().observed().parent().number(),
            self.proof().observed().merge_base(),
            self.proof().observed().parent().head_sha()
        )
    }
    fn actor_labels(&self) -> (ActorIdentity, ActorIdentity, String, String) {
        let actor = &self.proof().observed().parent().summary().pr.author;
        let author = ActorIdentity::github(&self.proof().source_scope().host, actor);
        let unknown = ActorIdentity {
            host: None,
            login: None,
            name: "Commit attribution not loaded".into(),
            email: None,
            avatar_url: None,
            kind: ActorKind::Unknown,
            git_role: None,
            host_associated: false,
        };
        (
            author,
            unknown,
            "PR author • NOT attribution of these file lines".into(),
            "Commit author/committer not observed in this PR image proof".into(),
        )
    }
}

#[derive(Debug)]
pub(crate) struct ScopedPrNativeSideV1 {
    pub(crate) proof: Arc<ClosedPrFileImagesV1>,
    pub(crate) side: PrImageSideV1,
    pub(crate) kind: Option<BlobPreviewKind>,
    pub(crate) label: String,
}
impl ScopedPrNativeSideV1 {
    pub(crate) fn content(&self) -> &[u8] {
        self.proof
            .observed()
            .image(self.side)
            .bytes()
            .unwrap_or(&[])
    }
    pub(crate) fn object(&self) -> &str {
        self.proof
            .observed()
            .image(self.side)
            .blob_sha()
            .unwrap_or("absent")
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        self.proof.retained_original_observation_bytes_v2()
            + self.label.capacity()
            + std::mem::size_of::<Self>()
            + 512
    }
}
