//! SAME existing run mutation body/disposition, with a closed optional original
//! source boundary. Transport reservations are not source or write authority.
use super::scoped_read_v1::{bind_scoped_actions_run_raw_v3, ScopedActionsRunRawPhaseV3};
use super::*;
use crate::providers::gh::PostWriteCaptureAllowanceV2;
use std::{sync::Arc, time::Instant};

/// Durable caller must distinguish a refusal BEFORE invocation from a write
/// that may have started. Neither case is an automatic retry permit.
#[derive(Debug)]
pub(crate) enum ActionsInvocationFailureV3 {
    NotStarted(GithubError),
    WriteMayHaveStarted(GithubError),
}
/// Constructed only by the SAME guarded mutation body after its actual fixed
/// HTTP202/204 capture. No DTO/serialized result can mint this acceptance.
pub(crate) struct ScopedRunMutationOutcomeV5 {
    outcome: RunMutationOutcome,
    http_ack: Option<u16>,
    deletion: Option<ScopedDeletedRunAvailabilityV5>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedDeletedRunAvailabilityV5 {
    Present,
    Unavailable,
}
impl ScopedRunMutationOutcomeV5 {
    pub(crate) fn outcome(&self) -> &RunMutationOutcome {
        &self.outcome
    }
    pub(crate) fn into_outcome(self) -> RunMutationOutcome {
        self.outcome
    }
    pub(crate) fn deletion_accepted(&self) -> bool {
        self.outcome.action == RunMutation::DeleteRun
            && self.http_ack == Some(204)
            && self.outcome.accepted
            && !self.outcome.completed
            && self.outcome.run.is_none()
    }
    pub(crate) fn deletion_availability(&self) -> Option<ScopedDeletedRunAvailabilityV5> {
        self.deletion
    }
}

/// Borrowed ONLY at the existing fixed invocation boundary. No constructor,
/// Clone, serde, DTO conversion or reusable write permit.
pub(crate) struct ScopedRunInvocationRefV3<'a> {
    original: &'a Arc<ScopedActionsRunTargetV1>,
    selected_job: Option<&'a Arc<ScopedActionsJobTargetV1>>,
    action: RunMutation,
    fresh: &'a WorkflowRun,
    repository: &'a str,
    endpoint: &'a str,
    method: &'a str,
    input: Option<&'a Value>,
    resource: &'a str,
}
impl ScopedRunInvocationRefV3<'_> {
    pub(crate) fn action_v4(&self) -> RunMutation {
        self.action
    }
    pub(crate) fn selected_job_v4(&self) -> Option<&Arc<ScopedActionsJobTargetV1>> {
        self.selected_job
    }
    pub(crate) fn original(&self) -> &Arc<ScopedActionsRunTargetV1> {
        self.original
    }
    pub(crate) fn fresh(&self) -> &WorkflowRun {
        self.fresh
    }
    pub(crate) fn repository(&self) -> &str {
        self.repository
    }
    pub(crate) fn endpoint(&self) -> &str {
        self.endpoint
    }
    pub(crate) fn method(&self) -> &str {
        self.method
    }
    pub(crate) fn input(&self) -> Option<&Value> {
        self.input
    }
    pub(crate) fn resource(&self) -> &str {
        self.resource
    }
}

struct ScopedRunBoundaryV3<'a, F> {
    measured: &'a GithubProvider,
    captures: &'a mut PostWriteCaptureAllowanceV2,
    original: &'a Arc<ScopedActionsRunTargetV1>,
    selected_job: Option<&'a Arc<ScopedActionsJobTargetV1>>,
    action: RunMutation,
    repository: &'a str,
    deadline: Instant,
    before_write: Option<F>,
    invoked: bool,
    http_ack: Option<u16>,
    original_catalogue_v5: Option<&'a RepositoryCatalogueRead>,
    deletion: Option<ScopedDeletedRunAvailabilityV5>,
}
enum RunMutationBoundaryV3<'a, F> {
    Legacy,
    Scoped(ScopedRunBoundaryV3<'a, F>),
}
fn phase(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(error(
            GithubErrorKind::Transport,
            "Original explicit Confirm cancelled or its queue-inclusive deadline expired",
        ));
    }
    Ok(())
}
impl<F> RunMutationBoundaryV3<'_, F>
where
    F: FnOnce(ScopedRunInvocationRefV3<'_>) -> GithubResult<()>,
{
    fn guard(
        &mut self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        repo: &str,
        target: &WorkflowRun,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowRun> {
        let Self::Scoped(scoped) = self else {
            return provider.guard_run(identity, repo, target, cancel);
        };
        phase(cancel, scoped.deadline)?;
        scoped.captures.validate_prewrite(scoped.measured, cancel)?;
        scoped.measured.actions_identity(identity, cancel)?;
        let (fresh, _, _) = scoped.measured.scoped_actions_run_raw_phase_v3(
            repo,
            scoped.original,
            ScopedActionsRunRawPhaseV3::UnchangedPrewrite,
            cancel,
        )?;
        phase(cancel, scoped.deadline)?;
        Ok(fresh)
    }
    fn resource(&self, provider: &GithubProvider, repo: &str, id: u64) -> String {
        match self {
            Self::Legacy => provider.actions_resource(repo, id),
            Self::Scoped(scoped) => provider.canonical_resource(&format!(
                "{}/repository-id/{}/actions/run/{id}",
                provider.host,
                scoped.original.repository_id(),
            )),
        }
    }
    fn guard_job(
        &mut self,
        provider: &GithubProvider,
        repo: &str,
        fresh: &WorkflowRun,
        job_id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if let Self::Scoped(scoped) = self {
            phase(cancel, scoped.deadline)?;
            let selected = scoped.selected_job.ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Actual original selected job required",
                )
            })?;
            if selected.id() != job_id {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Selected job ID changed",
                ));
            }
            return super::scoped_rerun_job_v4::verify(
                scoped.measured,
                repo,
                scoped.original,
                selected,
                cancel,
                scoped.deadline,
            );
        }
        // Legacy None keeps the exact old direct GET and required explicit
        // attempt field. The new optional-field proof is Some-only.
        let job = provider.api(
            &format!("repos/{repo}/actions/jobs/{job_id}"),
            "GET",
            None,
            None,
            false,
            cancel,
        )?;
        if number(&job, "id") != job_id
            || number(&job, "run_id") != fresh.id
            || number(&job, "run_attempt") != fresh.attempt
            || text(&job, "head_sha") != fresh.head_sha
        {
            return Err(error(
                GithubErrorKind::StaleHead,
                "The job does not belong to the selected current run attempt",
            ));
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn invoke(
        &mut self,
        provider: &GithubProvider,
        fresh: &WorkflowRun,
        endpoint: &str,
        method: &str,
        input: Option<Value>,
        resource: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let Self::Scoped(scoped) = self else {
            return provider.api(
                endpoint,
                method,
                input,
                Some(resource.into()),
                false,
                cancel,
            );
        };
        phase(cancel, scoped.deadline)?;
        scoped.captures.validate_prewrite(scoped.measured, cancel)?;
        // The borrowed reference is produced by THIS fixed shared body, never
        // selected labels/DTOs. The callback closes original repo/PR/SQL,
        // journal-generation/Draft ACK and final numeric principal.
        let before = scoped.before_write.take().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Original run publication boundary already consumed",
            )
        })?;
        before(ScopedRunInvocationRefV3 {
            original: scoped.original,
            selected_job: scoped.selected_job,
            action: scoped.action,
            fresh,
            repository: scoped.repository,
            endpoint,
            method,
            input: input.as_ref(),
            resource,
        })?;
        phase(cancel, scoped.deadline)?;
        let writer = provider
            .clone()
            .with_catalogue_read_deadline(scoped.deadline);
        // Set BEFORE the fixed prepaid slot/invocation attempt. A late error
        // remains uncertain rather than being mistaken for a safe retry.
        scoped.invoked = true;
        if matches!(
            scoped.action,
            RunMutation::ForceCancel | RunMutation::DeleteRun | RunMutation::DeleteLogs
        ) {
            if input.is_some() {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Closed empty-ack action has no input",
                ));
            }
            let ack = scoped.captures.scoped_run_empty_ack_v5(
                &writer,
                scoped.original,
                scoped.action,
                cancel,
            )?;
            if !Arc::ptr_eq(ack.original(), scoped.original) || ack.action() != scoped.action {
                return Err(error(
                    GithubErrorKind::AmbiguousWrite,
                    "Original HTTP receipt does not match its actual invocation",
                ));
            }
            scoped.http_ack = Some(ack.status());
            return Ok(Value::Null);
        }
        scoped.captures.api(
            &writer,
            endpoint,
            method,
            input,
            Some(resource.into()),
            cancel,
        )
    }
    fn observe(
        &mut self,
        provider: &GithubProvider,
        repo: &str,
        target: &WorkflowRun,
        cancel: &CancellationToken,
    ) -> GithubResult<WorkflowRun> {
        let Self::Scoped(scoped) = self else {
            return provider.run(repo, target.id, cancel);
        };
        phase(cancel, scoped.deadline)?;
        let writer = provider
            .clone()
            .with_catalogue_read_deadline(scoped.deadline);
        let raw = scoped.captures.api(
            &writer,
            &format!("repos/{repo}/actions/runs/{}", target.id),
            "GET",
            None,
            None,
            cancel,
        )?;
        // Same prepaid Value, same existing parser, all original repository/
        // run/node/workflow/event/suite/attempt/head facts BEFORE disposition.
        let (observed, _, _) = bind_scoped_actions_run_raw_v3(
            &raw,
            repo,
            scoped.original,
            if matches!(
                scoped.action,
                RunMutation::RerunAll { .. }
                    | RunMutation::RerunFailed { .. }
                    | RunMutation::RerunJob { .. }
            ) {
                ScopedActionsRunRawPhaseV3::RerunReceipt
            } else {
                ScopedActionsRunRawPhaseV3::CancelReceipt
            },
        )?;
        phase(cancel, scoped.deadline)?;
        Ok(observed)
    }
}
impl GithubProvider {
    pub(crate) fn actions_postwrite_identity_v3(
        &self,
        captures: &mut PostWriteCaptureAllowanceV2,
        expected: &HostIdentity,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<HostIdentity> {
        phase(cancel, deadline.min(captures.deadline()))?;
        let user = captures.api(self, "user", "GET", None, None, cancel)?;
        // Genuine already observed CLI version, never a new child/fallback.
        let current = self.identity_from_user_v3(&user, expected.gh_version.clone())?;
        if current
            .account_id
            .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || !current.same_account(expected)
            || current.login.len() > 256
            || current.login.chars().any(char::is_control)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Final original Actions principal changed/unavailable",
            ));
        }
        phase(cancel, deadline.min(captures.deadline()))?;
        Ok(current)
    }
    pub fn mutate_run(
        &self,
        identity: &HostIdentity,
        repo: &str,
        target: &WorkflowRun,
        action: RunMutation,
        cancel: &CancellationToken,
    ) -> GithubResult<RunMutationOutcome> {
        let mut boundary: RunMutationBoundaryV3<
            '_,
            fn(ScopedRunInvocationRefV3<'_>) -> GithubResult<()>,
        > = RunMutationBoundaryV3::Legacy;
        self.mutate_run_with_boundary_v3(identity, repo, target, action, cancel, &mut boundary)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mutate_run_guarded_v3<F>(
        &self,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        original: &Arc<ScopedActionsRunTargetV1>,
        canonical_repository: &str,
        action: RunMutation,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: F,
    ) -> Result<RunMutationOutcome, ActionsInvocationFailureV3>
    where
        F: FnOnce(ScopedRunInvocationRefV3<'_>) -> GithubResult<()>,
    {
        if action != RunMutation::Cancel {
            return Err(ActionsInvocationFailureV3::NotStarted(error(
                GithubErrorKind::InvalidTarget,
                "Original Cancel adapter cannot select another operation",
            )));
        }
        self.mutate_run_guarded_v4(
            measured,
            captures,
            identity,
            original,
            None,
            canonical_repository,
            action,
            cancel,
            deadline,
            before_write,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mutate_run_guarded_v4<F>(
        &self,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        original: &Arc<ScopedActionsRunTargetV1>,
        selected_job: Option<&Arc<ScopedActionsJobTargetV1>>,
        canonical_repository: &str,
        action: RunMutation,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: F,
    ) -> Result<RunMutationOutcome, ActionsInvocationFailureV3>
    where
        F: FnOnce(ScopedRunInvocationRefV3<'_>) -> GithubResult<()>,
    {
        self.mutate_run_guarded_inner_v5(
            measured,
            captures,
            identity,
            original,
            selected_job,
            canonical_repository,
            action,
            cancel,
            deadline,
            None,
            before_write,
        )
        .map(|closed| closed.into_outcome())
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn mutate_run_guarded_v5<F>(
        &self,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        original: &Arc<ScopedActionsRunTargetV1>,
        canonical_repository: &str,
        action: RunMutation,
        catalogue: &RepositoryCatalogueRead,
        cancel: &CancellationToken,
        deadline: Instant,
        before_write: F,
    ) -> Result<ScopedRunMutationOutcomeV5, ActionsInvocationFailureV3>
    where
        F: FnOnce(ScopedRunInvocationRefV3<'_>) -> GithubResult<()>,
    {
        self.mutate_run_guarded_inner_v5(
            measured,
            captures,
            identity,
            original,
            None,
            canonical_repository,
            action,
            cancel,
            deadline,
            Some(catalogue),
            before_write,
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn mutate_run_guarded_inner_v5<F>(
        &self,
        measured: &GithubProvider,
        captures: &mut PostWriteCaptureAllowanceV2,
        identity: &HostIdentity,
        original: &Arc<ScopedActionsRunTargetV1>,
        selected_job: Option<&Arc<ScopedActionsJobTargetV1>>,
        canonical_repository: &str,
        action: RunMutation,
        cancel: &CancellationToken,
        deadline: Instant,
        catalogue_v5: Option<&RepositoryCatalogueRead>,
        before_write: F,
    ) -> Result<ScopedRunMutationOutcomeV5, ActionsInvocationFailureV3>
    where
        F: FnOnce(ScopedRunInvocationRefV3<'_>) -> GithubResult<()>,
    {
        let check = (|| {
            phase(cancel, deadline)?;
            captures.validate_prewrite(measured, cancel)?;
            let supported = match action {
                RunMutation::ForceCancel | RunMutation::DeleteRun | RunMutation::DeleteLogs => {
                    catalogue_v5.is_some() && selected_job.is_none()
                }
                RunMutation::Cancel
                | RunMutation::RerunAll { .. }
                | RunMutation::RerunFailed { .. } => selected_job.is_none(),
                RunMutation::RerunJob { job_id, .. } => selected_job.is_some_and(|job| {
                    job.id() == job_id
                        && job.run().original_writer_v3(self)
                        && job.run().same_watch_pin_v2(original)
                }),
                _ => false,
            };
            if !supported
                || self.scoped_read_budget.is_some()
                || identity.host != self.host
                || identity
                    .account_id
                    .is_none_or(|id| id == 0 || id > i64::MAX as u64)
                || self.host != measured.host
                || self.program != measured.program
                || self.env != measured.env
                || !self.runner.same_instance(&measured.runner)
                || !original.original_writer_v3(self)
                || !original.original_writer_v3(measured)
                || validate_repo(canonical_repository).is_err()
                || captures
                    .reserved_requests()
                    .saturating_sub(captures.consumed_requests())
                    < 2
            {
                return Err(error(GithubErrorKind::InvalidTarget,
                    "Cancel needs its actual original issuer/numeric run and one original measured pool; no DTO or other operation adapter"));
            }
            Ok(())
        })();
        check.map_err(ActionsInvocationFailureV3::NotStarted)?;
        let deadline = deadline.min(captures.deadline());
        let mut boundary = RunMutationBoundaryV3::Scoped(ScopedRunBoundaryV3 {
            measured,
            captures,
            original,
            selected_job,
            action,
            repository: canonical_repository,
            deadline,
            before_write: Some(before_write),
            invoked: false,
            http_ack: None,
            original_catalogue_v5: catalogue_v5,
            deletion: None,
        });
        let result = self.mutate_run_with_boundary_v3(
            identity,
            canonical_repository,
            original.run(),
            action,
            cancel,
            &mut boundary,
        );
        let invoked = matches!(&boundary, RunMutationBoundaryV3::Scoped(scoped) if scoped.invoked);
        let http_ack = match &boundary {
            RunMutationBoundaryV3::Scoped(scoped) => scoped.http_ack,
            _ => None,
        };
        let deletion = match &boundary {
            RunMutationBoundaryV3::Scoped(scoped) => scoped.deletion,
            _ => None,
        };
        result
            .map(|outcome| ScopedRunMutationOutcomeV5 {
                outcome,
                http_ack,
                deletion,
            })
            .map_err(|error| {
                if invoked {
                    ActionsInvocationFailureV3::WriteMayHaveStarted(error)
                } else {
                    ActionsInvocationFailureV3::NotStarted(error)
                }
            })
    }
    #[allow(clippy::too_many_arguments)]
    fn mutate_run_with_boundary_v3<F>(
        &self,
        identity: &HostIdentity,
        repo: &str,
        target: &WorkflowRun,
        action: RunMutation,
        cancel: &CancellationToken,
        boundary: &mut RunMutationBoundaryV3<'_, F>,
    ) -> GithubResult<RunMutationOutcome>
    where
        F: FnOnce(ScopedRunInvocationRefV3<'_>) -> GithubResult<()>,
    {
        let fresh = boundary.guard(self, identity, repo, target, cancel)?;
        let resource = boundary.resource(self, repo, target.id);
        let base = format!("repos/{repo}/actions/runs/{}", target.id);
        let mut input = None;
        let (endpoint, method) = match action {
            RunMutation::RerunAll { debug } | RunMutation::RerunFailed { debug } => {
                if fresh.status != "completed" {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Only a completed run can be rerun",
                    ));
                }
                input = Some(json!({"enable_debug_logging":debug}));
                (
                    format!(
                        "{base}/{}",
                        if matches!(action, RunMutation::RerunFailed { .. }) {
                            "rerun-failed-jobs"
                        } else {
                            "rerun"
                        }
                    ),
                    "POST",
                )
            }
            RunMutation::RerunJob { job_id, debug } => {
                if fresh.status != "completed" || job_id == 0 {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "Select a job in a completed run",
                    ));
                }
                boundary.guard_job(self, repo, &fresh, job_id, cancel)?;
                input = Some(json!({"enable_debug_logging":debug}));
                (format!("repos/{repo}/actions/jobs/{job_id}/rerun"), "POST")
            }
            RunMutation::Cancel | RunMutation::ForceCancel => {
                if fresh.status == "completed" {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "The run already completed",
                    ));
                }
                (
                    format!(
                        "{base}/{}",
                        if matches!(action, RunMutation::ForceCancel) {
                            "force-cancel"
                        } else {
                            "cancel"
                        }
                    ),
                    "POST",
                )
            }
            RunMutation::DeleteRun => (base.clone(), "DELETE"),
            RunMutation::DeleteLogs => (format!("{base}/logs"), "DELETE"),
            RunMutation::ApproveFork => {
                if fresh.event != "pull_request"
                    || fresh.status != "action_required" && fresh.conclusion != "action_required"
                {
                    return Err(error(
                        GithubErrorKind::InvalidTarget,
                        "The selected fork run is not awaiting approval",
                    ));
                }
                (format!("{base}/approve"), "POST")
            }
        };
        boundary.invoke(self, &fresh, &endpoint, method, input, &resource, cancel)?;
        if matches!(action, RunMutation::DeleteRun) {
            if matches!(boundary, RunMutationBoundaryV3::Scoped(scoped) if scoped.http_ack==Some(204))
            {
                // The exact numeric run GET and closing repository/UID are
                // captured separately. A204 is acceptance, never catalogue
                // absence, complete deletion or a causal assertion.
                let RunMutationBoundaryV3::Scoped(scoped) = boundary else {
                    unreachable!("matched actual scoped boundary")
                };
                let writer = self.clone().with_catalogue_read_deadline(scoped.deadline);
                let availability = match scoped
                    .captures
                    .api(&writer, &base, "GET", None, None, cancel)
                {
                    Ok(raw) => {
                        bind_scoped_actions_run_raw_v3(
                            &raw,
                            repo,
                            scoped.original,
                            ScopedActionsRunRawPhaseV3::CancelReceipt,
                        )?;
                        ScopedDeletedRunAvailabilityV5::Present
                    }
                    Err(failure) if matches!(failure.kind, GithubErrorKind::NotFound) => {
                        ScopedDeletedRunAvailabilityV5::Unavailable
                    }
                    Err(failure) => return Err(failure),
                };
                scoped.captures.scoped_deleted_run_repository_v5(
                    &writer,
                    scoped.original,
                    scoped.original_catalogue_v5.ok_or_else(|| {
                        error(
                            GithubErrorKind::InvalidTarget,
                            "Actual original catalogue required",
                        )
                    })?,
                    cancel,
                )?;
                phase(cancel, scoped.deadline)?;
                scoped.deletion = Some(availability);
                return Ok(RunMutationOutcome { action,run:None,accepted:true,completed:false,
                    explanation:"GitHub accepted permanent whole-run deletion (HTTP204). The run may be unavailable; acceptance is not proof of deletion or who caused it. Inspect/acknowledge acceptance explicitly; never resend automatically.".into() });
            }
            let runs = self.after_write(&resource, self.runs(repo, None, cancel))?;
            if runs.iter().any(|r| r.id == target.id) {
                return Err(self.actions_ambiguous(
                    &resource,
                    "Run is still visible after deletion; reconcile without repeating DELETE",
                ));
            }
            return Ok(RunMutationOutcome {
                action,
                run: None,
                accepted: true,
                completed: true,
                explanation: "The selected run is absent from the refreshed catalog".into(),
            });
        }
        let observed = self.after_write(&resource, boundary.observe(self, repo, target, cancel))?;
        let rerun = matches!(
            action,
            RunMutation::RerunAll { .. }
                | RunMutation::RerunFailed { .. }
                | RunMutation::RerunJob { .. }
        );
        if observed.head_sha != target.head_sha
            || observed.workflow_id != target.workflow_id
            || observed.attempt < target.attempt
            || !rerun && observed.attempt != target.attempt
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Post-write run identity no longer matches the selected workflow/commit/original attempt; reconcile",
            ));
        }
        if !matches!(
            observed.status.as_str(),
            "queued"
                | "requested"
                | "pending"
                | "waiting"
                | "in_progress"
                | "completed"
                | "action_required"
        ) || observed.status == "completed" && observed.conclusion.is_empty()
        {
            return Err(self.actions_ambiguous(
                &resource,
                "Post-write run state is absent or unrecognized; no completed transition is inferred",
            ));
        }
        let completed = match action {
            RunMutation::RerunAll { .. }
            | RunMutation::RerunFailed { .. }
            | RunMutation::RerunJob { .. } => observed.attempt > target.attempt,
            RunMutation::Cancel | RunMutation::ForceCancel => {
                observed.status == "completed" && observed.conclusion == "cancelled"
            }
            RunMutation::ApproveFork => {
                observed.status != "action_required" && observed.conclusion != "action_required"
            }
            _ => false,
        };
        Ok(RunMutationOutcome {
            action,
            run: Some(observed),
            accepted: true,
            completed,
            explanation: if matches!(action, RunMutation::DeleteLogs) {
                "Log deletion accepted; the API does not expose a log-deleted field. A later log request may report unavailable; do not retry deletion automatically.".into()
            } else if completed {
                "The requested state transition was observed".into()
            } else {
                "GitHub accepted the request; the expected state transition has not yet been observed. Refresh or watch this exact run; do not issue the mutation again.".into()
            },
        })
    }
}

#[cfg(test)]
#[path = "scoped_run_write_v3_tests.rs"]
mod tests;
