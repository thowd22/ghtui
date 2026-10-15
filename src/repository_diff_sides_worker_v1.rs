//! Typed comparison-side work on the ONE existing Background queue/token.
use crate::{
    navigation_local_owner_v1::OriginalLocalNavigationOwnerV1,
    providers::runner::CancellationToken,
    repository_diff_sides_v1::{
        OriginalDiffFrameV1, OriginalDiffPickerV1, OriginalDiffQueryV1, OriginalDiffSideTicketV1,
    },
    workspace::{PreparedOriginalDiffBackV1, PreparedOriginalDiffSideV1, Workspace},
};
use std::sync::Arc;

#[derive(Clone)]
pub enum OriginalDiffWorkV1 {
    Native(Arc<crate::repository_native_diff_v1::NativeDiffRequestV1>),
    Query(Arc<OriginalDiffQueryV1>),
    Side(OriginalDiffSideTicketV1),
    Back(Arc<PreparedOriginalDiffSideV1>),
}
impl std::fmt::Debug for OriginalDiffWorkV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalDiffWorkV1(<same original query/member/side/Back>)")
    }
}
impl OriginalDiffWorkV1 {
    pub fn owns(&self, workspace: &Workspace, picker: Option<&OriginalDiffPickerV1>) -> bool {
        match self {
            Self::Native(request) => request.owns(workspace),
            Self::Query(request) => {
                request.intent().owns(workspace)
                    && picker
                        .is_some_and(|picker| picker.owns(workspace) && picker.accepts(request))
            }
            Self::Side(ticket) => {
                ticket.source().intent().owns(workspace)
                    && picker.is_some_and(|picker| {
                        picker.owns(workspace) && picker.accepts_ticket(ticket)
                    })
                    && ticket.absent_reason().is_none()
            }
            Self::Back(side) => {
                side.owns(workspace)
                    && workspace
                        .original_diff_side_v1
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, side))
            }
        }
    }
}
pub struct OriginalDiffWorkerOwnerV1 {
    original: Arc<OriginalLocalNavigationOwnerV1>,
    work: Arc<OriginalDiffWorkV1>,
    checkout_labels: crate::git::RepoStatus,
}
impl std::fmt::Debug for OriginalDiffWorkerOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalDiffWorkerOwnerV1(<prequeue original clock/token/source>)")
    }
}
impl OriginalDiffWorkerOwnerV1 {
    pub fn begin(
        workspace: &Workspace,
        picker: Option<&OriginalDiffPickerV1>,
        work: Arc<OriginalDiffWorkV1>,
        actual_token: CancellationToken,
    ) -> Result<Arc<Self>, String> {
        if !work.owns(workspace, picker) {
            return Err(
                "Original comparison request no longer owns exact source/query/member/side".into(),
            );
        }
        let original = OriginalLocalNavigationOwnerV1::begin(workspace, actual_token)?;
        let checkout_labels =
            crate::repository_source_identity::original_diff_checkout_labels_v1(workspace)?;
        Ok(Arc::new(Self {
            original,
            work,
            checkout_labels,
        }))
    }
    pub fn work(&self) -> &Arc<OriginalDiffWorkV1> {
        &self.work
    }
    pub fn cancellation(&self) -> &CancellationToken {
        self.original.operation().cancellation()
    }
    pub fn accounting(&self) -> Result<crate::providers::runner::ScopedReadAccountingV1, String> {
        self.original.operation().accounting()
    }
    pub fn can_admit(
        &self,
        workspace: &Workspace,
        picker: Option<&OriginalDiffPickerV1>,
        success: bool,
    ) -> bool {
        self.original.can_admit(workspace, success) && self.work.owns(workspace, picker)
    }
    pub fn prepare(self: &Arc<Self>) -> Result<PreparedOriginalDiffWorkV1, String> {
        let operation = self.original.operation();
        operation.check()?;
        let result = match self.work.as_ref() {
            OriginalDiffWorkV1::Native(request) => {
                request.prepare(operation).map(DiffResultV1::Native)
            }
            OriginalDiffWorkV1::Query(request) => {
                OriginalDiffFrameV1::prepare(request.clone(), operation).map(DiffResultV1::Query)
            }
            OriginalDiffWorkV1::Side(ticket) => PreparedOriginalDiffSideV1::open(ticket, operation)
                .map(|side| {
                    let commit = side.prepared_commit();
                    DiffResultV1::Side { side, commit }
                }),
            OriginalDiffWorkV1::Back(side) => side.prepare_back(operation).map(DiffResultV1::Back),
        };
        let result=result.and_then(|result| {
            operation.check()?;
            let (source,side)=match &result {
                DiffResultV1::Native(prepared)=>(prepared.source().as_ref(),None),
                DiffResultV1::Query(frame)=>(frame.source().as_ref(),None),
                DiffResultV1::Side {side,..}=>(side.source().as_ref(),Some(side.as_ref())),
                DiffResultV1::Back(_)=>match self.work.as_ref(){OriginalDiffWorkV1::Back(side)=>(side.source().as_ref(),None),_=>return Err("Original comparison Back source unavailable".into())},
            };
            let identity=crate::repository_source_identity::PreparedRepositorySourceIdentity::prepare_original_diff_v1(&self.checkout_labels,source,side)?;
            operation.check()?;
            Ok((result,Arc::new(identity)))
        });
        let sealed = operation.seal();
        match (result, sealed) {
            (Ok((result, source_identity)), Ok(_)) => Ok(PreparedOriginalDiffWorkV1 {
                owner: self.clone(),
                result,
                source_identity,
            }),
            (Err(error), _) | (_, Err(error)) => Err(error),
        }
    }
    pub fn failed(&self, picker: &mut OriginalDiffPickerV1, reason: &str) -> bool {
        match self.work.as_ref() {
            OriginalDiffWorkV1::Query(request) => picker.fail(request, reason),
            OriginalDiffWorkV1::Side(ticket) => picker.fail_side(ticket, reason),
            _ => false,
        }
    }
}
enum DiffResultV1 {
    Native(crate::repository_native_diff_v1::PreparedNativeDiffV1),
    Query(Arc<OriginalDiffFrameV1>),
    Side {
        side: Arc<PreparedOriginalDiffSideV1>,
        commit: crate::git::Commit,
    },
    Back(PreparedOriginalDiffBackV1),
}
pub struct PreparedOriginalDiffWorkV1 {
    owner: Arc<OriginalDiffWorkerOwnerV1>,
    result: DiffResultV1,
    source_identity: Arc<crate::repository_source_identity::PreparedRepositorySourceIdentity>,
}
impl PreparedOriginalDiffWorkV1 {
    pub fn source_identity_v1(
        &self,
    ) -> &Arc<crate::repository_source_identity::PreparedRepositorySourceIdentity> {
        &self.source_identity
    }
    pub fn matches(&self, owner: &Arc<OriginalDiffWorkerOwnerV1>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
            && match (&self.result, owner.work().as_ref()) {
                (DiffResultV1::Native(prepared), OriginalDiffWorkV1::Native(request)) => {
                    Arc::ptr_eq(prepared.request(), request)
                }
                (DiffResultV1::Native(_), _) | (_, OriginalDiffWorkV1::Native(_)) => false,
                _ => true,
            }
            && owner
                .accounting()
                .is_ok_and(|accounting| accounting.sealed() && accounting.in_flight_requests() == 0)
    }
    /// Called only after exact specialized result/owner checks, BEFORE generic
    /// loading/status/privacy/model/hit effects. No body clone on publication.
    pub fn publish(
        self,
        workspace: &mut Workspace,
        picker: &mut Option<OriginalDiffPickerV1>,
    ) -> Result<Option<usize>, String> {
        if !self.owner.can_admit(workspace, picker.as_ref(), true) {
            return Err("Original comparison request retired; no prepared result admitted".into());
        }
        match self.result {
            DiffResultV1::Native(_) => {
                Err("Native projection needs its exact capturing controller".into())
            }
            DiffResultV1::Query(frame) => {
                frame.source().install(workspace)?;
                if !picker.as_mut().is_some_and(|picker| picker.accept(frame)) {
                    return Err("Original comparison query changed before publication".into());
                }
                Ok(None)
            }
            DiffResultV1::Side { side, commit } => {
                let focus = side.install(workspace, commit)?;
                *picker = None;
                Ok(Some(focus))
            }
            DiffResultV1::Back(back) => {
                let focus = back.install(workspace)?;
                *picker = None;
                Ok(Some(focus))
            }
        }
    }
    pub fn publish_native_v1(
        self,
        workspace: &Workspace,
        state: &mut crate::repository_native_diff_v1::NativeDiffStateV1,
    ) -> Result<(), String> {
        let OriginalDiffWorkV1::Native(request) = self.owner.work().as_ref() else {
            return Err("This is not an original native projection request".into());
        };
        if !self.owner.can_admit(workspace, None, true)
            || !self.matches(&self.owner)
            || !state.owns(workspace)
            || !state.accepts(request)
        {
            return Err("Original native source/query/options/width request retired".into());
        }
        let DiffResultV1::Native(prepared) = self.result else {
            return Err("Native request returned another prepared kind".into());
        };
        if !state.accept(prepared) {
            return Err("Original native controller changed before publication".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "repository_diff_sides_worker_v1_tests.rs"]
mod tests;
