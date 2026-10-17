//! Optional original-workflow boundary in the SAME workflow state mutation.
//! Legacy None keeps its exact calls, argv, errors and expected-state policy.
use super::*;
use std::{sync::Arc, time::Instant};
pub(crate) struct ScopedWorkflowInvocationV6<'a> {
    original: &'a Arc<ScopedActionsWorkflowV2>,
    fresh: &'a Workflow,
    endpoint: &'a str,
    resource: &'a str,
    enabled: bool,
}
impl ScopedWorkflowInvocationV6<'_> {
    pub(crate) fn original(&self) -> &Arc<ScopedActionsWorkflowV2> {
        self.original
    }
    pub(crate) fn fresh(&self) -> &Workflow {
        self.fresh
    }
    pub(crate) fn endpoint(&self) -> &str {
        self.endpoint
    }
    pub(crate) fn resource(&self) -> &str {
        self.resource
    }
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }
}
pub(crate) struct ScopedWorkflowMutationV6 {
    original: Arc<ScopedActionsWorkflowV2>,
    observed: Arc<ScopedActionsWorkflowV2>,
    enabled: bool,
}
impl ScopedWorkflowMutationV6 {
    pub(crate) fn original(&self) -> &Arc<ScopedActionsWorkflowV2> {
        &self.original
    }
    pub(crate) fn observed(&self) -> &Arc<ScopedActionsWorkflowV2> {
        &self.observed
    }
    pub(crate) fn enabled(&self) -> bool {
        self.enabled
    }
    pub(crate) fn state_observed(&self) -> bool {
        self.observed.model().state == desired_state_v6(self.enabled)
    }
}
pub(crate) fn desired_state_v6(enabled: bool) -> &'static str {
    if enabled {
        "active"
    } else {
        "disabled_manually"
    }
}
pub(crate) fn state_eligible_v6(workflow: &Workflow, enabled: bool) -> bool {
    if enabled {
        matches!(
            workflow.state.as_str(),
            "disabled_manually" | "disabled_inactivity"
        )
    } else {
        workflow.state == "active"
    }
}
struct ScopedBoundaryV6<'a, F> {
    measured: &'a GithubProvider,
    captures: &'a mut PostWriteCaptureAllowanceV2,
    original: &'a Arc<ScopedActionsWorkflowV2>,
    deadline: Instant,
    before_write: Option<F>,
    invoked: bool,
    observed: Option<Arc<ScopedActionsWorkflowV2>>,
}
enum BoundaryV6<'a, F> {
    Legacy,
    Scoped(ScopedBoundaryV6<'a, F>),
}
fn clock(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(error(
            GithubErrorKind::Transport,
            "Original workflow attempt cancelled/expired",
        ))
    } else {
        Ok(())
    }
}
impl<F> BoundaryV6<'_, F>
where
    F: FnOnce(ScopedWorkflowInvocationV6<'_>) -> GithubResult<()>,
{
    fn guard(
        &mut self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        repo: &str,
        observed: &Workflow,
        cancel: &CancellationToken,
    ) -> GithubResult<Workflow> {
        let current = match self {
            Self::Legacy => {
                provider.actions_identity(identity, cancel)?;
                provider.workflow(repo, observed.id, cancel)?
            }
            Self::Scoped(scoped) => {
                clock(cancel, scoped.deadline)?;
                scoped.captures.validate_prewrite(scoped.measured, cancel)?;
                scoped.measured.actions_identity(identity, cancel)?;
                let raw = scoped
                    .measured
                    .scoped_workflow_raw_v6(repo, observed.id, cancel)?;
                let fresh = scoped.original.bind_raw_v6(scoped.measured, &raw, true)?;
                fresh.model().clone()
            }
        };
        if current.path != observed.path || current.state != observed.state {
            return Err(error(
                GithubErrorKind::StaleHead,
                "Workflow path/state changed; refresh before changing it",
            ));
        }
        Ok(current)
    }
    fn resource(&self, provider: &GithubProvider, repo: &str, id: u64) -> String {
        match self {
            Self::Legacy => format!("{}/{repo}/workflow/{id}", provider.host),
            Self::Scoped(s) => format!(
                "{}/repository-id/{}/actions/workflow/{id}",
                provider.host,
                s.original.repository().id()
            ),
        }
    }
    fn invoke(
        &mut self,
        provider: &GithubProvider,
        fresh: &Workflow,
        enabled: bool,
        endpoint: &str,
        resource: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        match self {
            Self::Legacy => {
                provider.api(endpoint, "PUT", None, Some(resource.into()), false, cancel)?;
                Ok(())
            }
            Self::Scoped(s) => {
                clock(cancel, s.deadline)?;
                let callback = s.before_write.take().ok_or_else(|| {
                    error(
                        GithubErrorKind::InvalidTarget,
                        "Original workflow callback already consumed",
                    )
                })?;
                callback(ScopedWorkflowInvocationV6 {
                    original: s.original,
                    fresh,
                    endpoint,
                    resource,
                    enabled,
                })?;
                clock(cancel, s.deadline)?;
                s.invoked = true;
                s.captures
                    .scoped_workflow_empty_ack_v6(provider, s.original, enabled, cancel)
            }
        }
    }
    fn receipt(
        &mut self,
        provider: &GithubProvider,
        repo: &str,
        observed: &Workflow,
        resource: &str,
        enabled: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<Workflow> {
        match self {
            Self::Legacy => {
                let fresh =
                    provider.after_write(resource, provider.workflow(repo, observed.id, cancel))?;
                if fresh.state != desired_state_v6(enabled) {
                    return Err(provider.actions_ambiguous(resource,
                        "Workflow state change was not confirmed; reconcile without repeating the request"));
                }
                Ok(fresh)
            }
            Self::Scoped(s) => {
                clock(cancel, s.deadline)?;
                let fresh = s
                    .captures
                    .scoped_workflow_observation_v6(provider, s.original, cancel)?;
                let model = fresh.model().clone();
                s.observed = Some(fresh);
                clock(cancel, s.deadline)?;
                Ok(model)
            }
        }
    }
}
impl GithubProvider {
    pub(super) fn scoped_workflow_raw_v6(
        &self,
        repo: &str,
        id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        validate_repo(repo)?;
        if id == 0 || id > i64::MAX as u64 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Bounded actual workflow ID required",
            ));
        }
        let (deadline, cap) = self.scoped_read_window.ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Original bounded workflow read required",
            )
        })?;
        clock(cancel, deadline)?;
        let bounded = self
            .clone()
            .with_scoped_read_window(deadline, cap.min(64 * 1024))?;
        bounded.api(
            &format!("repos/{repo}/actions/workflows/{id}"),
            "GET",
            None,
            None,
            false,
            cancel,
        )
    }
    fn set_workflow_enabled_inner_v6<F>(
        &self,
        identity: &HostIdentity,
        repo: &str,
        observed: &Workflow,
        enabled: bool,
        cancel: &CancellationToken,
        boundary: &mut BoundaryV6<'_, F>,
    ) -> GithubResult<Workflow>
    where
        F: FnOnce(ScopedWorkflowInvocationV6<'_>) -> GithubResult<()>,
    {
        let current = boundary.guard(self, identity, repo, observed, cancel)?;
        let resource = boundary.resource(self, repo, observed.id);
        let endpoint = format!(
            "repos/{repo}/actions/workflows/{}/{}",
            observed.id,
            if enabled { "enable" } else { "disable" }
        );
        boundary.invoke(self, &current, enabled, &endpoint, &resource, cancel)?;
        boundary.receipt(self, repo, observed, &resource, enabled, cancel)
    }
    pub(super) fn set_workflow_enabled_legacy_v6(
        &self,
        identity: &HostIdentity,
        repo: &str,
        observed: &Workflow,
        enabled: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<Workflow> {
        let mut boundary: BoundaryV6<'_, fn(ScopedWorkflowInvocationV6<'_>) -> GithubResult<()>> =
            BoundaryV6::Legacy;
        self.set_workflow_enabled_inner_v6(identity, repo, observed, enabled, cancel, &mut boundary)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mutate_workflow_enabled_guarded_v6<F>(
        &self,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        original: &Arc<ScopedActionsWorkflowV2>,
        enabled: bool,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: F,
    ) -> Result<ScopedWorkflowMutationV6, ActionsInvocationFailureV3>
    where
        F: FnOnce(ScopedWorkflowInvocationV6<'_>) -> GithubResult<()>,
    {
        let before = || {
            clock(cancel, deadline)?;
            if !original.original_writer_v6(self)
                || !original.original_writer_v6(measured)
                || self.host != measured.host
                || self.program != measured.program
                || self.env != measured.env
                || identity.host != self.host
                || identity
                    .account_id
                    .is_none_or(|id| id == 0 || id > i64::MAX as u64)
                || captures
                    .reserved_requests()
                    .saturating_sub(captures.consumed_requests())
                    < 2
                || !state_eligible_v6(original.model(), enabled)
            {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Actual supported workflow state and same producing runner required",
                ));
            }
            captures.validate_prewrite(measured, cancel)
        };
        before().map_err(ActionsInvocationFailureV3::NotStarted)?;
        let deadline = deadline.min(captures.deadline());
        let mut boundary = BoundaryV6::Scoped(ScopedBoundaryV6 {
            measured,
            captures,
            original,
            deadline,
            before_write: Some(before_write),
            invoked: false,
            observed: None,
        });
        let result = self.set_workflow_enabled_inner_v6(
            identity,
            original.repository().name(),
            original.model(),
            enabled,
            cancel,
            &mut boundary,
        );
        let BoundaryV6::Scoped(boundary) = boundary else {
            unreachable!("private actual boundary")
        };
        match result {
            Ok(_) => Ok(ScopedWorkflowMutationV6 {
                original: original.clone(),
                observed: boundary.observed.ok_or_else(|| {
                    ActionsInvocationFailureV3::WriteMayHaveStarted(error(
                        GithubErrorKind::Transport,
                        "Original workflow observation unavailable",
                    ))
                })?,
                enabled,
            }),
            Err(failure) => Err(if boundary.invoked {
                ActionsInvocationFailureV3::WriteMayHaveStarted(failure)
            } else {
                ActionsInvocationFailureV3::NotStarted(failure)
            }),
        }
    }
}
