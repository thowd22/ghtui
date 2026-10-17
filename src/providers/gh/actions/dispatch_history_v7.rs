//! Actual historical Dispatch READ proof. No saved metadata mints Run/Workflow.
use super::*;
use crate::actions_write_journal::{ActionsHistoricalDispatchKeyV7, ActionsPublicationReference};
use std::time::Instant;
pub(crate) struct ScopedHistoricalDispatchReadV7 {
    producer: CommandRunner,
    host: String,
    reference: ActionsPublicationReference,
    repository_id: u64,
    repository_node: String,
    workflow_id: u64,
    workflow: Option<Workflow>,
    run_id: Option<u64>,
    run: Option<WorkflowRun>,
    cancel: CancellationToken,
    deadline: Instant,
    resource: String,
    generation: u64,
    accepted: bool,
}
impl ScopedHistoricalDispatchReadV7 {
    pub(crate) fn run(&self) -> Option<&WorkflowRun> {
        self.run.as_ref()
    }
    pub(crate) fn availability_label(&self) -> &'static str {
        if !self.accepted {
            "No actual typed Dispatch acceptance receipt • outcome unknown • no ACK or resend authority"
        } else if self.run.is_some() {
            "Actual returned run is currently readable • acceptance is not workflow completion or causation"
        } else if self.run_id.is_some() {
            "Returned run endpoint HTTP404 unavailable • NOT deletion or permissions proof • receipt remains historical"
        } else {
            "HTTP204 acceptance only • no actual run ID, head, matching candidate or completion is invented"
        }
    }
    pub(crate) fn matches(
        &self,
        p: &GithubProvider,
        key: &ActionsHistoricalDispatchKeyV7,
        cancel: &CancellationToken,
    ) -> bool {
        self.producer.same_instance(&p.runner)
            && self.host == p.host
            && self.reference == *key.reference()
            && self.repository_id == key.repository_id()
            && self.repository_node == key.repository_node()
            && self.workflow_id == key.workflow_id()
            && self.run_id == key.run_id()
            && self.accepted == key.accepted()
            && self.cancel.same_instance(cancel)
            && !cancel.is_cancelled()
            && Instant::now() < self.deadline
            && p.runner.resource_generation(&self.resource) == self.generation
    }
    pub(crate) fn same_observation(&self, other: &Self) -> bool {
        self.producer.same_instance(&other.producer)
            && self.host == other.host
            && self.reference == other.reference
            && self.repository_id == other.repository_id
            && self.repository_node == other.repository_node
            && self.workflow_id == other.workflow_id
            && self.resource == other.resource
            && self.generation == other.generation
            && self.run_id == other.run_id
            && self.accepted == other.accepted
            && match (&self.workflow, &other.workflow) {
                (Some(a), Some(b)) => a.id == b.id && a.path == b.path && a.state == b.state,
                (None, None) => true,
                _ => false,
            }
            && match (&self.run, &other.run) {
                (Some(a), Some(b)) => {
                    a.id == b.id
                        && a.attempt == b.attempt
                        && a.workflow_id == b.workflow_id
                        && a.head_sha == b.head_sha
                        && a.event == b.event
                        && a.status == b.status
                        && a.conclusion == b.conclusion
                }
                (None, None) => true,
                _ => false,
            }
    }
}
fn bad() -> GithubError {
    error(GithubErrorKind::IdentityChanged,"Original Dispatch historical identity/observation unavailable; no saved target or private diagnostics")
}
impl GithubProvider {
    pub(crate) fn scoped_actions_historical_dispatch_v7(
        &self,
        current: &CurrentCatalogueRepository,
        key: &ActionsHistoricalDispatchKeyV7,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedHistoricalDispatchReadV7> {
        let (deadline, cap) = self.scoped_read_window.ok_or_else(bad)?;
        if self.scoped_read_budget.is_none()
            || cancel.is_cancelled()
            || Instant::now() >= deadline
            || current.repository().id != key.repository_id()
            || current.repository().node_id.as_deref() != Some(key.repository_node())
            || current.repository().permissions.pull != Some(true)
        {
            return Err(bad());
        }
        let resource = format!(
            "{}/repository-id/{}/actions/workflow/{}",
            self.host,
            key.repository_id(),
            key.workflow_id()
        );
        let generation = self.runner.resource_generation(&resource);
        let bounded = self
            .clone()
            .with_scoped_read_window(deadline, cap.min(64 * 1024))?;
        let repo = &current.repository().full_name;
        let read = |endpoint: String| -> GithubResult<Value> {
            let bytes = bounded.execute(
                vec![
                    "api".into(),
                    endpoint,
                    "--hostname".into(),
                    self.host.clone(),
                    "--method".into(),
                    "GET".into(),
                    "--header".into(),
                    format!("X-GitHub-Api-Version: {}", key.context().protocol),
                ],
                None,
                None,
                cancel,
            )?;
            serde_json::from_slice(&bytes).map_err(|_| bad())
        };
        let workflow = match read(format!(
            "repos/{repo}/actions/workflows/{}",
            key.workflow_id()
        )) {
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
                    return Err(bad());
                }
                Some(model)
            }
            Err(error) if matches!(error.kind, GithubErrorKind::NotFound) => None,
            Err(_) => return Err(bad()),
        };
        let run = if let Some(id) = key.run_id() {
            match read(format!("repos/{repo}/actions/runs/{id}")) {
                Ok(raw) => {
                    let model = run_from_json(&raw);
                    if raw["id"].as_u64() != Some(id)
                        || model.id != id
                        || model.workflow_id != key.workflow_id()
                        || model.event != "workflow_dispatch"
                        || model.head_sha != key.context().commit
                        || key.run_receipt().is_none_or(|known| {
                            raw["node_id"].as_str() != Some(known.run_node.as_str())
                                || raw["check_suite_id"].as_u64() != Some(known.check_suite_id)
                        })
                        || raw["run_attempt"]
                            .as_u64()
                            .filter(|a| *a > 0 && *a <= i64::MAX as u64)
                            != Some(model.attempt)
                        || key.attempt().is_some_and(|old| model.attempt < old)
                        || raw["node_id"].as_str().is_none_or(|node| {
                            node.is_empty()
                                || node.len() > 256
                                || node.chars().any(char::is_control)
                        })
                        || raw["check_suite_id"]
                            .as_u64()
                            .is_none_or(|id| id == 0 || id > i64::MAX as u64)
                        || raw["repository"]["id"].as_u64() != Some(key.repository_id())
                        || raw["repository"]["node_id"].as_str() != Some(key.repository_node())
                        || raw["repository"]["full_name"].as_str() != Some(repo.as_str())
                    {
                        return Err(bad());
                    }
                    Some(model)
                }
                Err(error) if matches!(error.kind, GithubErrorKind::NotFound) => None,
                Err(_) => return Err(bad()),
            }
        } else {
            None
        };
        if cancel.is_cancelled()
            || Instant::now() >= deadline
            || self.runner.resource_generation(&resource) != generation
        {
            return Err(bad());
        }
        Ok(ScopedHistoricalDispatchReadV7 {
            producer: self.runner.clone(),
            host: self.host.clone(),
            reference: key.reference().clone(),
            repository_id: key.repository_id(),
            repository_node: key.repository_node().into(),
            workflow_id: key.workflow_id(),
            workflow,
            run_id: key.run_id(),
            run,
            cancel: cancel.clone(),
            deadline,
            resource,
            generation,
            accepted: key.accepted(),
        })
    }
}
