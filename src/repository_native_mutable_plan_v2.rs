//! Closed read-only Native selected-plan preparation. Explicit confirmation is
//! a different purpose; this module never obtains a write from a read window.
use crate::{
    git::{MutableDiffKindV2, MutableDiffSourceV2},
    local_actions::LocalPlan,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_native_diff_v1::{NativeCurrentRangeV2, NativeDiffStateV1},
    workspace::Workspace,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, OnceLock,
};
type State = NativeDiffStateV1<MutableDiffSourceV2>;

pub struct NativeMutablePreviewRequestV2 {
    original: Arc<NativeCurrentRangeV2<MutableDiffSourceV2>>,
}
impl std::fmt::Debug for NativeMutablePreviewRequestV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NativeMutablePreviewRequestV2(<original source/frame/range; read only>)")
    }
}
impl NativeMutablePreviewRequestV2 {
    pub fn capture(state: &State, workspace: &Workspace) -> Result<Arc<Self>, String> {
        if !state.owns(workspace) {
            return Err("Mutable selection no longer owns the loaded repository".into());
        }
        let original = state.capture_current_range_v2()?;
        if original.source().intent().kind() == MutableDiffKindV2::Uncommitted {
            return Err("Preview selected writes from explicit Working or Staged scope; all-tracked edits have no inferred index direction".into());
        }
        Ok(Arc::new(Self { original }))
    }
    pub fn source(&self) -> &Arc<MutableDiffSourceV2> {
        self.original.source()
    }
    pub fn range(
        &self,
    ) -> &Arc<crate::repository_native_diff_projection_v1::NativeDiffRangeV1<MutableDiffSourceV2>>
    {
        self.original.range()
    }
    pub fn owns(&self, workspace: &Workspace) -> bool {
        self.source().intent().owns(workspace)
    }
    pub fn accepts(&self, state: &State) -> bool {
        self.original.matches(state)
    }
    pub fn prepare(
        self: &Arc<Self>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Arc<PreparedNativeMutablePlanV2>, String> {
        operation.check()?;
        let source = self.source();
        source.validate_current(operation)?;
        let reverse = source.intent().kind() == MutableDiffKindV2::Staged;
        let plan =
            source
                .intent()
                .origin()
                .with_saved_navigation_read_v1(operation, |repository| {
                    LocalPlan::prepare_native_selection_v2(
                        repository,
                        source.result(),
                        self.range().file(),
                        self.range().changed(),
                        reverse,
                    )
                })?;
        source.validate_current(operation)?;
        let prepared = Arc::new(PreparedNativeMutablePlanV2 {
            request: self.clone(),
            plan: Arc::new(plan),
        });
        operation.check()?;
        Ok(prepared)
    }
}
pub struct PreparedNativeMutablePlanV2 {
    request: Arc<NativeMutablePreviewRequestV2>,
    plan: Arc<LocalPlan>,
}
impl PreparedNativeMutablePlanV2 {
    pub fn request(&self) -> &Arc<NativeMutablePreviewRequestV2> {
        &self.request
    }
    pub fn plan(&self) -> &Arc<LocalPlan> {
        &self.plan
    }
    pub fn matches(
        &self,
        request: &Arc<NativeMutablePreviewRequestV2>,
        state: &State,
        workspace: &Workspace,
    ) -> bool {
        Arc::ptr_eq(request, &self.request)
            && self.request.owns(workspace)
            && self.request.accepts(state)
    }
}

/// Created only by a separate explicit Confirm of the still-current exact
/// prepared preview. Read-window possession cannot construct this purpose.
pub struct NativeMutableConfirmRequestV2 {
    prepared: Arc<PreparedNativeMutablePlanV2>,
    attempted: AtomicBool,
    retired: AtomicBool,
    original_cancellation: OnceLock<crate::providers::runner::CancellationToken>,
    original_operation:
        OnceLock<crate::providers::remote_git_read_window_v1::RemoteGitReadWindowV1>,
}
impl std::fmt::Debug for NativeMutableConfirmRequestV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "NativeMutableConfirmRequestV2(<distinct confirmed selected ApplyPatch purpose>)",
        )
    }
}
impl NativeMutableConfirmRequestV2 {
    pub(crate) fn capture(
        prepared: &Arc<PreparedNativeMutablePlanV2>,
        state: &State,
        workspace: &Workspace,
    ) -> Result<Arc<Self>, String> {
        if !prepared.matches(prepared.request(), state, workspace)
            || prepared.plan().mutation.observed
                != prepared
                    .request()
                    .source()
                    .result()
                    .observed
                    .ok_or("Original mutable observation unavailable")?
        {
            return Err(
                "Selected preview/source/frame/range retired before explicit confirmation".into(),
            );
        }
        Ok(Arc::new(Self {
            prepared: prepared.clone(),
            attempted: AtomicBool::new(false),
            retired: AtomicBool::new(false),
            original_cancellation: OnceLock::new(),
            original_operation: OnceLock::new(),
        }))
    }
    pub fn source(&self) -> &Arc<MutableDiffSourceV2> {
        self.prepared.request().source()
    }
    pub fn plan(&self) -> &Arc<LocalPlan> {
        self.prepared.plan()
    }
    pub fn owns(&self, workspace: &Workspace) -> bool {
        self.prepared.request().owns(workspace)
    }
    pub fn accepts(&self, state: &State) -> bool {
        self.prepared.request().accepts(state)
    }
    /// B's genuine owner calls this before its first queue submission. The
    /// retained window carries bounds only; explicit Confirm grants the attempt.
    pub(crate) fn bind_original_operation_v2(
        &self,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<(), String> {
        if self.retired.load(Ordering::Acquire) {
            return Err("Original explicit confirmation was retired before queue/write".into());
        }
        let original = self.original_operation.get_or_init(|| operation.window());
        if !original.same_operation_v2(&operation.window()) {
            return Err(
                "Original confirmation operation cannot be renewed, replaced or replayed".into(),
            );
        }
        let token = self
            .original_cancellation
            .get_or_init(|| operation.cancellation().clone());
        if self.retired.load(Ordering::Acquire) {
            token.cancel();
            return Err("Original explicit confirmation retired during queue binding".into());
        }
        Ok(())
    }
    pub(crate) fn retire(&self) {
        self.retired.store(true, Ordering::Release);
        if let Some(token) = self.original_cancellation.get() {
            token.cancel();
        }
    }
    pub(crate) fn reserve_attempt(&self) -> Result<(), String> {
        if self.retired.load(Ordering::Acquire) {
            return Err("Original explicit confirmation retired before attempt".into());
        }
        self.attempted.compare_exchange(false,true,Ordering::AcqRel,Ordering::Acquire).map(|_|()).map_err(|_|"Selected confirmation was already attempted; inspect current index state instead of retrying".into())
    }
    pub fn prepare(
        self: &Arc<Self>,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<PreparedNativeSelectedOutcomeV2, String> {
        self.bind_original_operation_v2(operation)?;
        operation.check()?;
        let output=self.source().intent().origin().with_saved_navigation_read_v1(operation,|repository|repository.execute_native_selected_v2(self,operation).map_err(|error|format!("{error}\nThe confirmed attempt may already have changed the index. Inspect current state before another write; no automatic retry.")))?;
        let note=format!("{} completed • index changed only by the exact confirmed selected patch\n{}\nReload and inspect the comparison before another write.",self.plan().command.label(),output);
        if note.len() > 3 * 1024 * 1024 {
            return Err("Confirmed selected operation completed but its outcome display exceeded bounds; inspect index state before another write".into());
        }
        let note = NativePlanNoteV2::prepare(Arc::from(note))?;
        let result = PreparedNativeSelectedOutcomeV2 {
            request: self.clone(),
            note,
        };
        operation.check()?;
        Ok(result)
    }
}
pub struct PreparedNativeSelectedOutcomeV2 {
    request: Arc<NativeMutableConfirmRequestV2>,
    note: Arc<NativePlanNoteV2>,
}
/// Worker-indexed borrowed outcome lines; drawing never scans the full body.
pub struct NativePlanNoteV2 {
    text: Arc<str>,
    lines: Arc<[std::ops::Range<usize>]>,
}
impl NativePlanNoteV2 {
    pub(crate) fn prepare(text: Arc<str>) -> Result<Arc<Self>, String> {
        if text.len() > 3 * 1024 * 1024 {
            return Err("Selected outcome exceeds bounded display; inspect current index".into());
        }
        let mut start = 0;
        let mut lines = Vec::new();
        for part in text.split_inclusive('\n') {
            if lines.len() == 50_000 {
                return Err(
                    "Selected outcome exceeds50000 display lines; inspect current index".into(),
                );
            }
            let end = start + part.len();
            let visible = end - usize::from(part.ends_with('\n'));
            lines.push(start..visible);
            start = end;
        }
        Ok(Arc::new(Self {
            text,
            lines: lines.into(),
        }))
    }
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }
    pub fn line(&self, index: usize) -> Option<&str> {
        self.lines.get(index).map(|range| &self.text[range.clone()])
    }
}
impl PreparedNativeSelectedOutcomeV2 {
    pub(crate) fn into_note(self) -> Arc<NativePlanNoteV2> {
        self.note
    }
    pub fn request(&self) -> &Arc<NativeMutableConfirmRequestV2> {
        &self.request
    }
    pub fn note(&self) -> &str {
        self.note.text()
    }
    pub fn matches(&self, request: &Arc<NativeMutableConfirmRequestV2>) -> bool {
        Arc::ptr_eq(&self.request, request)
    }
}

#[cfg(test)]
#[path = "repository_native_mutable_plan_v2_tests.rs"]
mod tests;
