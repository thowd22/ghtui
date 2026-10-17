//! Normally reaped raw output ONLY for an actual original Workflow branch GET.
//! Generic command failures/NotFound diagnostics cannot issue absence evidence.
use super::*;
use crate::providers::gh::{WorkflowExactBranchGetV9, WorkflowHistoricalGetV9};

pub(crate) struct WorkflowBranchOutputV9 {
    stdout: Vec<u8>,
    exit: i32,
}
impl std::fmt::Debug for WorkflowBranchOutputV9 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WorkflowBranchOutputV9(<private raw observation redacted>)")
    }
}
impl WorkflowBranchOutputV9 {
    pub(in crate::providers) fn into_raw_v9(self) -> (Vec<u8>, i32) {
        (self.stdout, self.exit)
    }
}
impl CommandRunner {
    pub(crate) fn run_workflow_branch_get_v9(
        &self,
        request: &WorkflowExactBranchGetV9,
        cancel: &CancellationToken,
    ) -> Result<WorkflowBranchOutputV9, CommandFailure> {
        let output = self.workflow_read_output_v9(request.runner_spec_v9(self, cancel)?, cancel)?;
        request.runner_spec_v9(self, cancel)?;
        Ok(output)
    }
    pub(crate) fn run_workflow_historical_get_v9(
        &self,
        request: &WorkflowHistoricalGetV9,
        cancel: &CancellationToken,
    ) -> Result<WorkflowBranchOutputV9, CommandFailure> {
        let output = self.workflow_read_output_v9(request.runner_spec_v9(self, cancel)?, cancel)?;
        request.runner_spec_v9(self, cancel)?;
        Ok(output)
    }
    // No caller outside this module can submit a raw spec through this policy;
    // both entrypoints require their distinct actual closed request type.
    fn workflow_read_output_v9(
        &self,
        spec: &CommandSpec,
        cancel: &CancellationToken,
    ) -> Result<WorkflowBranchOutputV9, CommandFailure> {
        let spec = spec.clone();
        let result = if let Some(pool) = spec.scoped_read_budget.clone() {
            pool.run_workflow_branch_v9(spec, cancel)
        } else {
            execute_mode_v63(&spec, cancel, None, ExecutePolicyV63::WorkflowBranchGetV9)
        }
        .map_err(|error| CommandFailure {
            kind: error.kind,
            exit_code: error.exit_code,
            outcome_unknown: false,
            message: "Original Workflow branch read failed; private protocol withheld".into(),
            stdout: Vec::new(),
            stderr: Vec::new(),
        })?;
        if !matches!(result.exit_code, 0 | 1) {
            return Err(failure(
                FailureKind::Exit,
                "Workflow branch read was not normally reaped",
                false,
            ));
        }
        Ok(WorkflowBranchOutputV9 {
            stdout: result.stdout,
            exit: result.exit_code,
        })
    }
}
