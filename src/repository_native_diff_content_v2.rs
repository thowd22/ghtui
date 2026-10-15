//! One immutable canonical-file contract for the existing native diff engine.
//! Implementations are sealed, genuine provider sources. Display keys retain
//! their concrete source type: unsaved content can never become saved authority.
use crate::{
    git::{
        MutableDiffIntentV2, MutableDiffKindV2, MutableDiffSourceV2, OriginalComparisonIntentV1,
        OriginalComparisonSideV1, OriginalComparisonSourceV1, RawPath,
    },
    identity::{ActorIdentity, ActorKind, GitRole},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_view::DiffSide,
    workspace::Workspace,
};
use std::sync::Arc;

pub(crate) mod sealed {
    pub trait Sealed {}
}

/// Read-only projection material, NOT a current-state check or write approval.
pub trait NativeDiffContentV2: sealed::Sealed + std::fmt::Debug + Send + Sync {
    fn canonical_file(&self, file: usize) -> Option<&crate::diff::FileDiff>;
    fn content_endpoints(&self) -> (Option<&str>, Option<&str>);
    fn member_count(&self) -> usize;
    fn member_index(&self, file: usize) -> Option<usize>;
    fn member_path(&self, file: usize, side: DiffSide) -> Option<&RawPath>;
}

impl sealed::Sealed for OriginalComparisonSourceV1 {}
impl NativeDiffContentV2 for OriginalComparisonSourceV1 {
    fn canonical_file(&self, file: usize) -> Option<&crate::diff::FileDiff> {
        self.result().diff.files.get(file)
    }
    fn content_endpoints(&self) -> (Option<&str>, Option<&str>) {
        (
            self.result().diff.base.as_deref(),
            self.result().diff.head.as_deref(),
        )
    }
    fn member_count(&self) -> usize {
        self.members().len()
    }
    fn member_index(&self, file: usize) -> Option<usize> {
        Some(self.members().get(file)?.index())
    }
    fn member_path(&self, file: usize, side: DiffSide) -> Option<&RawPath> {
        self.members().get(file)?.path(match side {
            DiffSide::Old => OriginalComparisonSideV1::Old,
            DiffSide::New => OriginalComparisonSideV1::New,
            DiffSide::Both => return None,
        })
    }
}

impl sealed::Sealed for MutableDiffSourceV2 {}
impl NativeDiffContentV2 for MutableDiffSourceV2 {
    fn canonical_file(&self, file: usize) -> Option<&crate::diff::FileDiff> {
        self.result().diff.files.get(file)
    }
    fn content_endpoints(&self) -> (Option<&str>, Option<&str>) {
        (
            self.result().diff.base.as_deref(),
            self.result().diff.head.as_deref(),
        )
    }
    fn member_count(&self) -> usize {
        self.members().len()
    }
    fn member_index(&self, file: usize) -> Option<usize> {
        Some(self.members().get(file)?.index())
    }
    fn member_path(&self, file: usize, side: DiffSide) -> Option<&RawPath> {
        self.members().get(file)?.path(side)
    }
}

/// Captured purpose and worker validation are source-specific. Sharing the
/// immutable engine does not share saved authorization with unsaved content.
pub type CapturedNativeDiffPurposeV2<S> = (Arc<<S as NativeDiffPurposeV2>::Intent>, Option<Arc<S>>);
pub trait NativeDiffPurposeV2: NativeDiffContentV2 + Sized {
    /// Borrowed original provider bytes, not a permission or reconstructed diff.
    #[doc(hidden)]
    fn external_patch_v4(&self) -> crate::repository_native_external_v4::NativeExternalPatchV4<'_>;
    fn supports_inline_comment() -> bool {
        false
    }
    fn supports_review_threads() -> bool {
        false
    }
    type Context;
    type Operation: crate::native_diff_operation_v3::NativeDiffCheckV3;
    fn validate_full_side_v3(
        &self,
        material: &crate::repository_native_full_side_v3::NativeFullSideMaterialV3,
        operation: &Self::Operation,
    ) -> Result<(), String>;
    #[doc(hidden)]
    fn prepare_full_side_v3(
        self: &Arc<Self>,
        file: usize,
        side: DiffSide,
        operation: &Self::Operation,
    ) -> Result<Arc<crate::repository_native_full_side_v3::NativeFullSideMaterialV3>, String>;
    fn closed_status() -> &'static str;
    fn pane_title(wide: bool) -> &'static str;
    fn stage_control() -> &'static str;
    fn stage_unavailable() -> &'static str;
    fn selection_help() -> &'static str;
    fn ready_status() -> &'static str;
    type Intent: std::fmt::Debug + Send + Sync;
    fn capture(workspace: &Self::Context) -> Result<CapturedNativeDiffPurposeV2<Self>, String>;
    fn intent(&self) -> &Arc<Self::Intent>;
    fn intent_owns(intent: &Self::Intent, workspace: &Self::Context) -> bool;
    fn source_owns(source: &Arc<Self>, workspace: &Self::Context) -> bool;
    fn read(intent: Arc<Self::Intent>, operation: &Self::Operation) -> Result<Arc<Self>, String>;
    fn validate_current(&self, operation: &Self::Operation) -> Result<(), String>;
    fn complete(&self) -> bool;
    fn observed_members(&self) -> usize;
    fn member_label(&self, file: usize) -> Option<&str>;
    fn pane_header(&self) -> String;
    fn actor_labels(&self) -> (ActorIdentity, ActorIdentity, String, String);
}

impl NativeDiffPurposeV2 for OriginalComparisonSourceV1 {
    fn external_patch_v4(&self) -> crate::repository_native_external_v4::NativeExternalPatchV4<'_> {
        crate::repository_native_external_v4::NativeExternalPatchV4::saved(self)
    }
    type Context = Workspace;
    type Operation = LocalGitNavigationReadV1;
    fn validate_full_side_v3(
        &self,
        material: &crate::repository_native_full_side_v3::NativeFullSideMaterialV3,
        operation: &Self::Operation,
    ) -> Result<(), String> {
        material.validate_local_v3(operation)
    }
    fn prepare_full_side_v3(
        self: &Arc<Self>,
        file: usize,
        side: DiffSide,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<crate::repository_native_full_side_v3::NativeFullSideMaterialV3>, String> {
        crate::repository_native_full_side_v3::NativeFullSideMaterialV3::saved(
            self.clone(),
            file,
            side,
            operation,
        )
    }
    fn closed_status() -> &'static str {
        "Native saved diff closed • no repository changes"
    }
    fn pane_title(wide: bool) -> &'static str {
        if wide {
            "Native saved diff • original source • bounded 512-column pane"
        } else {
            "Native saved diff • original source"
        }
    }
    fn stage_control() -> &'static str {
        "[a Stage disabled]"
    }
    fn stage_unavailable() -> &'static str {
        "Stage disabled: this is a saved comparison; no current index/worktree authority or write was created"
    }
    fn selection_help() -> &'static str {
        "Space selects original line; Shift+↑/↓ extends. Drag keeps exact original membership. Saved changes cannot be staged."
    }
    fn ready_status() -> &'static str {
        "Native saved diff ready • original canonical rows; read-only"
    }
    type Intent = OriginalComparisonIntentV1;
    fn capture(workspace: &Workspace) -> Result<(Arc<Self::Intent>, Option<Arc<Self>>), String> {
        let source = workspace
            .original_comparison_v1
            .as_ref()
            .filter(|source| source.intent().owns(workspace))
            .cloned();
        let intent = match &source {
            Some(source) => source.intent().clone(),
            None => OriginalComparisonIntentV1::capture(workspace)?,
        };
        Ok((intent, source))
    }
    fn intent(&self) -> &Arc<Self::Intent> {
        OriginalComparisonSourceV1::intent(self)
    }
    fn intent_owns(intent: &Self::Intent, workspace: &Workspace) -> bool {
        intent.owns(workspace)
    }
    fn source_owns(source: &Arc<Self>, workspace: &Workspace) -> bool {
        source.intent().owns(workspace)
            && workspace
                .original_comparison_v1
                .as_ref()
                .is_none_or(|current| Arc::ptr_eq(source, current))
    }
    fn read(
        intent: Arc<Self::Intent>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        OriginalComparisonSourceV1::read(intent, operation)
    }
    fn validate_current(&self, operation: &LocalGitNavigationReadV1) -> Result<(), String> {
        operation.check()
    }
    fn complete(&self) -> bool {
        OriginalComparisonSourceV1::complete(self)
    }
    fn observed_members(&self) -> usize {
        OriginalComparisonSourceV1::observed_members(self)
    }
    fn member_label(&self, file: usize) -> Option<&str> {
        Some(self.members().get(file)?.label())
    }
    fn pane_header(&self) -> String {
        let old = self.endpoint(OriginalComparisonSideV1::Old).object_id();
        let new = self.endpoint(OriginalComparisonSideV1::New).object_id();
        format!(
            "Old {} → New {} • read-only",
            &old[..old.len().min(12)],
            &new[..new.len().min(12)]
        )
    }
    fn actor_labels(&self) -> (ActorIdentity, ActorIdentity, String, String) {
        let commit = self.head_commit();
        let author = crate::git_actor_labels_v1::display_identity(&commit.author, GitRole::Author);
        let committer =
            crate::git_actor_labels_v1::display_identity(&commit.committer, GitRole::Committer);
        let author_label = format!("New commit: {}", author.label());
        let committer_label = format!("New commit: {}", committer.label());
        (author, committer, author_label, committer_label)
    }
}

impl NativeDiffPurposeV2 for MutableDiffSourceV2 {
    fn external_patch_v4(&self) -> crate::repository_native_external_v4::NativeExternalPatchV4<'_> {
        crate::repository_native_external_v4::NativeExternalPatchV4::mutable(self)
    }
    type Context = Workspace;
    type Operation = LocalGitNavigationReadV1;
    fn validate_full_side_v3(
        &self,
        material: &crate::repository_native_full_side_v3::NativeFullSideMaterialV3,
        operation: &Self::Operation,
    ) -> Result<(), String> {
        material.validate_local_v3(operation)
    }
    fn prepare_full_side_v3(
        self: &Arc<Self>,
        file: usize,
        side: DiffSide,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<crate::repository_native_full_side_v3::NativeFullSideMaterialV3>, String> {
        Ok(
            crate::repository_native_full_side_v3::NativeFullSideMaterialV3::mutable(
                crate::git::MutableFullSideV3::read(self.clone(), file, side, operation)?,
            ),
        )
    }
    fn closed_status() -> &'static str {
        "Native local diff closed • no repository changes"
    }
    fn pane_title(wide: bool) -> &'static str {
        if wide {
            "Native local diff • observed snapshot • bounded 512-column pane"
        } else {
            "Native local diff • observed snapshot"
        }
    }
    fn stage_control() -> &'static str {
        "[a Preview selected]"
    }
    fn stage_unavailable() -> &'static str {
        "Choose a current nonempty Working/Staged changed-line range, then preview its exact patch and explicitly confirm; no write occurs on selection"
    }
    fn selection_help() -> &'static str {
        "Space selects original line; Shift+↑/↓ extends. Drag keeps exact observed membership. A separate current-state preview and confirmation is required before any write."
    }
    fn ready_status() -> &'static str {
        "Native local diff ready • observed snapshot; preview before any write"
    }
    type Intent = MutableDiffIntentV2;
    fn capture(workspace: &Workspace) -> Result<(Arc<Self::Intent>, Option<Arc<Self>>), String> {
        Ok((MutableDiffIntentV2::capture(workspace)?, None))
    }
    fn intent(&self) -> &Arc<Self::Intent> {
        MutableDiffSourceV2::intent(self)
    }
    fn intent_owns(intent: &Self::Intent, workspace: &Workspace) -> bool {
        intent.owns(workspace)
    }
    fn source_owns(source: &Arc<Self>, workspace: &Workspace) -> bool {
        source.intent().owns(workspace)
    }
    fn read(
        intent: Arc<Self::Intent>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<Self>, String> {
        MutableDiffSourceV2::read(intent, operation)
    }
    fn validate_current(&self, operation: &LocalGitNavigationReadV1) -> Result<(), String> {
        MutableDiffSourceV2::validate_current(self, operation)
    }
    fn complete(&self) -> bool {
        MutableDiffSourceV2::complete(self)
    }
    fn observed_members(&self) -> usize {
        MutableDiffSourceV2::observed_members(self)
    }
    fn member_label(&self, file: usize) -> Option<&str> {
        Some(self.members().get(file)?.label())
    }
    fn pane_header(&self) -> String {
        match self.intent().kind(){
            MutableDiffKindV2::Working=>"Index → working files • tracked edits • observed snapshot".into(),
            MutableDiffKindV2::Staged=>"Saved baseline as Git interprets it → index • observed snapshot".into(),
            MutableDiffKindV2::Uncommitted=>"Saved baseline as Git interprets it → working files • all tracked edits • observed snapshot".into(),
        }
    }
    fn actor_labels(&self) -> (ActorIdentity, ActorIdentity, String, String) {
        // Unsaved files have no new-commit actor. Never infer who authored them
        // from config, the previous commit, names, or a cached hosted profile.
        let unknown = ActorIdentity {
            host: None,
            login: None,
            name: "Unknown unsaved-content actor".into(),
            email: None,
            avatar_url: None,
            kind: ActorKind::Unknown,
            git_role: None,
            host_associated: false,
        };
        (
            unknown.clone(),
            unknown,
            "Unsaved content: author unknown; no new commit exists".into(),
            "Unsaved content: committer unknown; no new commit exists".into(),
        )
    }
}
