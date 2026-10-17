//! Only genuine paid Branch or canonical created-PR observations enter these
//! metadata-only results. No fake Run/Definition or full YAML row preparation.
use super::*;
use crate::providers::gh::{
    HostIdentity, WorkflowBranchObservedV9, WorkflowPrCreatedV9, WorkflowRemoteRefsV9,
};
impl ActionsPublication {
    pub(crate) fn observed_workflow_branch_v9(
        identity: &HostIdentity,
        observed: &WorkflowBranchObservedV9,
        attempt: i64,
    ) -> GithubResult<Self> {
        Self::workflow_remote_result_v9(identity, observed.refs(), observed.receipt_v9(), attempt,
            "Exact new branch and committed D observed by Git and GitHub; creation causation not claimed".into())
    }
    pub(crate) fn observed_workflow_pr_v9(
        identity: &HostIdentity,
        refs: &Arc<WorkflowRemoteRefsV9>,
        observed: &WorkflowPrCreatedV9,
        attempt: i64,
    ) -> GithubResult<Self> {
        Self::workflow_remote_result_v9(
            identity,
            refs,
            observed.receipt_v9(refs)?,
            attempt,
            format!(
                "Actual canonical PR #{} observed after HTTP201; workflow completion not claimed",
                observed.number()
            ),
        )
    }
    fn workflow_remote_result_v9(
        identity: &HostIdentity,
        refs: &Arc<WorkflowRemoteRefsV9>,
        receipt: ActionsReceipt,
        attempt: i64,
        message: String,
    ) -> GithubResult<Self> {
        if attempt <= 0 {
            return Err(plain_error("Actual original workflow publication required"));
        }
        let mut workspace = ActionsWorkspace::new(
            identity.clone(),
            vec![refs.repositories().base().name().into()],
        );
        workspace.view = ActionsView::PublicationInspection;
        workspace.publication_inspection = Some(ActionsPublicationInspectionPage { attempt_id: attempt,
            heading: format!("Original workflow edit publication #{attempt} • observed receipt"),
            lines: vec![format!("Head repository ID {} • exact branch {}", refs.repositories().head().id(), refs.head_branch()),
                format!("Original C {}", refs.committed().staged().saved().opened().intent().definition().commit_sha),
                format!("Reviewed committed D {}", refs.committed().commit().head()),
                format!("Base {} at {}", refs.base_branch(), refs.base_sha()), message.clone(),
                "No remote request was repeated; exact durable receipt remains inspectable. Observations are not completion or causation.".into()],
            acknowledgement: None });
        Ok(finish_publication_v3(
            workspace,
            message,
            ActionsDisposition::VerifiedEffect { receipt },
            PostPublicationRefresh::None,
        ))
    }
}
