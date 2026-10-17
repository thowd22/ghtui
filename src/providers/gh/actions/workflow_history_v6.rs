//! Exact workflow historical observation, never a live workflow capability.
use super::*;
use crate::actions_write_journal::{ActionsHistoricalWorkflowKeyV6, ActionsPublicationReference};
use std::time::Instant;
pub(crate) struct ScopedHistoricalWorkflowReadV6 {
    producer: CommandRunner,
    host: String,
    repository_id: u64,
    repository_node: String,
    reference: ActionsPublicationReference,
    workflow_id: u64,
    workflow: Option<Workflow>,
    cancellation: CancellationToken,
    deadline: Instant,
}
impl ScopedHistoricalWorkflowReadV6 {
    pub(crate) fn workflow(&self) -> Option<&Workflow> {
        self.workflow.as_ref()
    }
    pub(crate) fn matches(
        &self,
        provider: &GithubProvider,
        key: &ActionsHistoricalWorkflowKeyV6,
        cancel: &CancellationToken,
    ) -> bool {
        self.producer.same_instance(&provider.runner)
            && self.host == provider.host
            && self.repository_id == key.repository_id()
            && self.repository_node == key.repository_node()
            && self.reference == *key.reference()
            && self.workflow_id == key.workflow_id()
            && self.cancellation.same_instance(cancel)
            && !cancel.is_cancelled()
            && Instant::now() < self.deadline
    }
    pub(crate) fn same_observation(&self, other: &Self) -> bool {
        self.producer.same_instance(&other.producer)
            && self.host == other.host
            && self.repository_id == other.repository_id
            && self.repository_node == other.repository_node
            && self.reference == other.reference
            && self.workflow_id == other.workflow_id
            && match (&self.workflow, &other.workflow) {
                (None, None) => true,
                (Some(a), Some(b)) => a.id == b.id && a.path == b.path && a.state == b.state,
                _ => false,
            }
    }
    pub(crate) fn availability_label(&self) -> &'static str {
        if self.workflow.is_some() {
            "Exact original numeric workflow/node/path currently readable • historical only, not new approval"
        } else {
            "Exact workflow endpoint unavailable (HTTP404) • NOT deletion/permission/causal proof • no workflow ACK"
        }
    }
}
impl GithubProvider {
    pub(crate) fn scoped_actions_historical_workflow_v6(
        &self,
        current: &CurrentCatalogueRepository,
        key: &ActionsHistoricalWorkflowKeyV6,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedHistoricalWorkflowReadV6> {
        let (deadline, _) = self.scoped_read_window.ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Original bounded historical operation required",
            )
        })?;
        let row = current.repository();
        if cancel.is_cancelled()
            || Instant::now() >= deadline
            || row.id != key.repository_id()
            || row.node_id.as_deref() != Some(key.repository_node())
            || row.permissions.pull != Some(true)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Actual original historical workflow repository unavailable",
            ));
        }
        let raw = self.scoped_workflow_raw_v6(&row.full_name, key.workflow_id(), cancel);
        let workflow = match raw {
            Ok(raw) => {
                let model = workflows::workflow_from_json_v2(&raw);
                workflows::validate_workflow_v2(&model)?;
                if raw["id"].as_u64() != Some(key.workflow_id())
                    || raw["node_id"].as_str() != Some(key.node())
                    || model.path != key.path()
                    || model.name.len() > 4096
                    || model.state.len() > 128
                    || model.state.chars().any(char::is_control)
                {
                    return Err(error(
                        GithubErrorKind::IdentityChanged,
                        "Original historical workflow ID/node/path changed",
                    ));
                }
                Some(model)
            }
            Err(failure) if matches!(failure.kind, GithubErrorKind::NotFound) => None,
            Err(failure) => return Err(failure),
        };
        if cancel.is_cancelled() || Instant::now() >= deadline {
            return Err(error(
                GithubErrorKind::Transport,
                "Original workflow history ended",
            ));
        }
        Ok(ScopedHistoricalWorkflowReadV6 {
            producer: self.runner.clone(),
            host: self.host.clone(),
            repository_id: row.id,
            repository_node: key.repository_node().into(),
            reference: key.reference().clone(),
            workflow_id: key.workflow_id(),
            workflow,
            cancellation: cancel.clone(),
            deadline,
        })
    }
}
