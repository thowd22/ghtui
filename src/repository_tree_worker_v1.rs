//! Typed original-tree work on the existing Background queue. No App/SQL/GH
//! mutation or body copies: parent hooks remain unapplied until admission.
use crate::{
    navigation_local_owner_v1::OriginalLocalNavigationOwnerV1,
    providers::runner::CancellationToken,
    repository_tree_jump_v1::{
        OriginalTreeFrameV1, OriginalTreeJumpV1, OriginalTreeRequestV1, OriginalTreeTicketV1,
    },
    workspace::{OriginalTreeNavigationActionV1, PreparedOriginalTreeNavigationV1, Workspace},
};
use std::sync::Arc;

#[derive(Clone)]
pub enum OriginalTreeWorkV1 {
    Query(Arc<OriginalTreeRequestV1>),
    Jump(OriginalTreeTicketV1),
    Navigate {
        projection: Arc<PreparedOriginalTreeNavigationV1>,
        action: OriginalTreeNavigationActionV1,
    },
    MemberDetail(Arc<crate::repository_tree_viewport_v2::TreeDetailRequestV2>),
    FolderDetail {
        intent: Arc<crate::repository_tree_viewport_v2::TreeFolderIntentV2>,
        width: u16,
        field: crate::repository_tree_viewport_v2::TreeDetailFieldV2,
    },
}
impl std::fmt::Debug for OriginalTreeWorkV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalTreeWorkV1(<original source/member request>)")
    }
}
impl OriginalTreeWorkV1 {
    pub fn metadata_v2(&self) -> bool {
        matches!(self, Self::MemberDetail(_) | Self::FolderDetail { .. })
    }
    pub fn owns(&self, workspace: &Workspace, jump: Option<&OriginalTreeJumpV1>) -> bool {
        match self {
            Self::MemberDetail(request) => {
                request.ticket().inventory().intent().owns(workspace)
                    && jump.is_some_and(|jump| jump.owns(workspace) && request.is_current(jump))
            }
            Self::FolderDetail { intent, .. } => intent.is_current(workspace),
            Self::Query(request) => {
                request.intent().owns(workspace)
                    && jump
                        .is_some_and(|jump| jump.owns(workspace) && jump.accepts_request(request))
            }
            Self::Jump(ticket) => {
                ticket.inventory().intent().owns(workspace)
                    && jump.is_some_and(|jump| {
                        jump.owns(workspace)
                            && jump.pending().is_none()
                            && jump.frame().is_some_and(|frame| ticket.belongs_to(frame))
                    })
            }
            Self::Navigate { projection, action } => {
                projection.visible_for(workspace)
                    && workspace
                        .tree_navigation_v1
                        .as_ref()
                        .is_some_and(|current| Arc::ptr_eq(current, projection))
                    && match action {
                        OriginalTreeNavigationActionV1::Toggle(ticket)
                        | OriginalTreeNavigationActionV1::Directory(ticket) => {
                            projection.owns_ticket(ticket)
                        }
                    }
            }
        }
    }
}

/// Captures the actual token/30s queue clock BEFORE queueing, reusing the
/// original actual runner/read pool. Private construction prevents rebasing.
pub struct OriginalTreeWorkerOwnerV1 {
    original: Arc<OriginalLocalNavigationOwnerV1>,
    work: Arc<OriginalTreeWorkV1>,
}
impl std::fmt::Debug for OriginalTreeWorkerOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalTreeWorkerOwnerV1(<same queued source/request/token>)")
    }
}
impl OriginalTreeWorkerOwnerV1 {
    pub fn begin(
        workspace: &Workspace,
        jump: Option<&OriginalTreeJumpV1>,
        work: Arc<OriginalTreeWorkV1>,
        actual_token: CancellationToken,
    ) -> Result<Arc<Self>, String> {
        if !work.owns(workspace, jump) {
            return Err(
                "Original tree request no longer belongs to this source/query/member".into(),
            );
        }
        let original = OriginalLocalNavigationOwnerV1::begin(workspace, actual_token)?;
        Ok(Arc::new(Self { original, work }))
    }
    pub fn can_admit(
        &self,
        workspace: &Workspace,
        jump: Option<&OriginalTreeJumpV1>,
        success: bool,
    ) -> bool {
        self.original.can_admit(workspace, success) && self.work.owns(workspace, jump)
    }
    pub fn work(&self) -> &Arc<OriginalTreeWorkV1> {
        &self.work
    }
    /// Runs on the SAME existing Background worker, seals success AND errors.
    /// Worker panics remain owned by the retained Completion owner in parent.
    pub fn prepare(self: &Arc<Self>) -> Result<PreparedOriginalTreeWorkV1, String> {
        let operation = self.original.operation();
        operation.check()?;
        let result = match self.work.as_ref() {
            OriginalTreeWorkV1::Query(request) => {
                OriginalTreeFrameV1::prepare(request.clone(), operation).map(TreeResultV1::Query)
            }
            OriginalTreeWorkV1::Jump(ticket) => {
                PreparedOriginalTreeNavigationV1::jump(ticket, operation)
                    .and_then(|projection| prepare_navigation_v2(projection, operation))
            }
            OriginalTreeWorkV1::Navigate { projection, action } => projection
                .navigate(action, operation)
                .and_then(|projection| prepare_navigation_v2(projection, operation)),
            OriginalTreeWorkV1::MemberDetail(request) => {
                crate::repository_tree_viewport_v2::PreparedTreeDetailV2::prepare(
                    request.clone(),
                    operation,
                )
                .map(TreeResultV1::MemberDetail)
            }
            OriginalTreeWorkV1::FolderDetail {
                intent,
                width,
                field,
            } => intent
                .prepare(*width, *field, operation)
                .map(TreeResultV1::FolderDetail),
        };
        let sealed = operation.seal();
        match (result, sealed) {
            (Ok(result), Ok(_)) => Ok(PreparedOriginalTreeWorkV1 {
                owner: self.clone(),
                result,
            }),
            (Err(error), _) | (_, Err(error)) => Err(error),
        }
    }
    pub fn failed(&self, jump: &mut OriginalTreeJumpV1, reason: &str) -> bool {
        match self.work.as_ref() {
            OriginalTreeWorkV1::Query(request) => jump.fail(request, reason),
            _ => false,
        }
    }
    pub fn cancellation(&self) -> &CancellationToken {
        self.original.operation().cancellation()
    }
    pub fn accounting(&self) -> Result<crate::providers::runner::ScopedReadAccountingV1, String> {
        self.original.operation().accounting()
    }
}
enum TreeResultV1 {
    Query(Arc<OriginalTreeFrameV1>),
    Navigation {
        projection: Arc<PreparedOriginalTreeNavigationV1>,
        strip: Arc<crate::repository_tree_viewport_v2::TreeBreadcrumbStripV2>,
    },
    MemberDetail(Arc<crate::repository_tree_viewport_v2::PreparedTreeDetailV2>),
    FolderDetail(Arc<crate::repository_tree_viewport_v2::PreparedTreeFolderDetailV2>),
}
fn prepare_navigation_v2(
    projection: Arc<PreparedOriginalTreeNavigationV1>,
    operation: &crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
) -> Result<TreeResultV1, String> {
    let strip = crate::repository_tree_viewport_v2::TreeBreadcrumbStripV2::prepare(
        projection.clone(),
        operation,
    )?;
    Ok(TreeResultV1::Navigation { projection, strip })
}
pub(crate) enum TreePublishEffectV2 {
    Query,
    Navigation(usize),
    MemberDetail(Arc<crate::repository_tree_viewport_v2::PreparedTreeDetailV2>),
    FolderDetail(Arc<crate::repository_tree_viewport_v2::PreparedTreeFolderDetailV2>),
}
pub struct PreparedOriginalTreeWorkV1 {
    owner: Arc<OriginalTreeWorkerOwnerV1>,
    result: TreeResultV1,
}
impl PreparedOriginalTreeWorkV1 {
    /// Typed result is exact captured request, not a generic Workspace/private
    /// page. Called before loading/status/identity/privacy/hits change.
    pub fn matches(&self, owner: &Arc<OriginalTreeWorkerOwnerV1>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
            && owner
                .accounting()
                .is_ok_and(|a| a.sealed() && a.in_flight_requests() == 0)
    }
    pub fn publish(
        self,
        workspace: &mut Workspace,
        jump: &mut Option<OriginalTreeJumpV1>,
    ) -> Result<Option<usize>, String> {
        match self.publish_v2(workspace, jump)? {
            TreePublishEffectV2::Query => Ok(None),
            TreePublishEffectV2::Navigation(focus) => Ok(Some(focus)),
            _ => {
                Err("Original metadata result requires its exact current purpose publisher".into())
            }
        }
    }
    pub(crate) fn publish_v2(
        self,
        workspace: &mut Workspace,
        jump: &mut Option<OriginalTreeJumpV1>,
    ) -> Result<TreePublishEffectV2, String> {
        if !self.owner.can_admit(workspace, jump.as_ref(), true) {
            return Err("Original tree source/request retired; no prepared result admitted".into());
        }
        match self.result {
            TreeResultV1::Query(frame) => {
                if !jump.as_mut().is_some_and(|jump| jump.accept(frame)) {
                    return Err("Original query changed before publication".into());
                }
                Ok(TreePublishEffectV2::Query)
            }
            TreeResultV1::Navigation { projection, strip } => {
                let focus = projection.focus();
                projection.install(workspace)?;
                workspace.tree_breadcrumbs_v2 = Some(strip);
                *jump = None;
                Ok(TreePublishEffectV2::Navigation(focus))
            }
            TreeResultV1::MemberDetail(detail) => Ok(TreePublishEffectV2::MemberDetail(detail)),
            TreeResultV1::FolderDetail(detail) => Ok(TreePublishEffectV2::FolderDetail(detail)),
        }
    }
}

#[cfg(test)]
#[path = "repository_tree_worker_v1_tests.rs"]
mod tests;
