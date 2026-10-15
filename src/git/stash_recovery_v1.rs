//! Genuine stash intent whose operation failed, including successful Git with
//! unavailable view refresh. This witness grants no retry/reconciliation.
use super::{local_commit_actor_v1::source_identity, *};

pub struct FailedStashIntentV1 {
    origin: Arc<OriginalLocalActorOriginV1>,
    sha: String,
    pop: bool,
    command_succeeded: bool,
}

#[cfg(test)]
#[path = "stash_recovery_v1_tests.rs"]
mod tests;
impl fmt::Debug for FailedStashIntentV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("FailedStashIntentV1(<actual stash intent/phase; no retry approval>)")
    }
}
impl FailedStashIntentV1 {
    pub fn origin(&self) -> &Arc<OriginalLocalActorOriginV1> {
        &self.origin
    }
    pub fn sha(&self) -> &str {
        &self.sha
    }
    pub fn pop(&self) -> bool {
        self.pop
    }
    pub fn command_succeeded(&self) -> bool {
        self.command_succeeded
    }
}

/// Returned only after the existing installed-Git execution reports success.
/// The exact pre-execution witness survives a later presentation refresh error.
#[derive(Debug)]
pub struct StashAttemptSuccessV2 {
    output: String,
    intent: Arc<FailedStashIntentV1>,
}
impl StashAttemptSuccessV2 {
    pub fn output(&self) -> &str {
        &self.output
    }
    pub(crate) fn into_parts(self) -> (String, Arc<FailedStashIntentV1>) {
        (self.output, self.intent)
    }
}

#[derive(Debug)]
pub struct StashAttemptFailureV1 {
    pub message: String,
    failed: Option<Arc<FailedStashIntentV1>>,
}
impl StashAttemptFailureV1 {
    pub fn failed_intent(&self) -> Option<&Arc<FailedStashIntentV1>> {
        self.failed.as_ref()
    }
    pub(crate) fn before(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            failed: None,
        }
    }
    pub(crate) fn after_success(
        message: impl Into<String>,
        intent: Arc<FailedStashIntentV1>,
    ) -> Self {
        Self { message:format!("Git reported the stash command succeeded, but repository view refresh failed: {}\nEffects may already be present. Inspect the original result; do not retry or drop automatically.",message.into()),failed:Some(intent) }
    }
}

impl OriginalLocalActorOriginV1 {
    pub(crate) fn inspect_stash_v1(
        &self,
        failed: &FailedStashIntentV1,
        operation: &crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    ) -> Result<(RepoStatus, Vec<ConflictStage>, bool), String> {
        if !std::ptr::eq(self, failed.origin().as_ref()) {
            return Err("Inspection belongs to another original source".into());
        }
        self.with_saved_navigation_read_v1(operation, |repo| {
            let before = repo.fingerprint().map_err(|e| e.to_string())?;
            let status = repo.status().map_err(|e| e.to_string())?;
            let stages = repo.conflict_stages().map_err(|e| e.to_string())?;
            if stages.len() > 1024 {
                return Err("Conflict observation exceeds 1024 stage rows".into());
            }
            if stages.iter().any(|stage| {
                stage.path.0.len() > 4096 || stage.id.len() > 64 || stage.mode.len() > 16
            }) || stages
                .iter()
                .map(|stage| stage.path.0.len() + stage.id.len() + stage.mode.len())
                .sum::<usize>()
                > 16 * 1024
            {
                return Err(
                    "Conflict observation exceeds bounded raw-path/metadata capacity".into(),
                );
            }
            let present = repo
                .stashes()
                .map_err(|e| e.to_string())?
                .iter()
                .any(|stash| stash.id == failed.sha());
            if repo.fingerprint().map_err(|e| e.to_string())? != before {
                return Err(
                    "Original workspace changed during stash inspection; inspect explicitly again"
                        .into(),
                );
            }
            Ok((status, stages, present))
        })
    }
}

impl GitRepository {
    /// Caller has the existing explicit confirmed MutationPlan. Only the
    /// source/typed-stash witness is added; execution, serialisation, stale
    /// fingerprint checks and ambiguity barriers stay in the existing runner.
    pub fn execute_stash_with_inspection_v1(
        &self,
        plan: &MutationPlan,
        confirmed: bool,
        origin: Arc<OriginalLocalActorOriginV1>,
    ) -> Result<StashAttemptSuccessV2, StashAttemptFailureV1> {
        if !confirmed {
            return Err(StashAttemptFailureV1::before("Operation was not confirmed"));
        }
        let (id, pop) = match &plan.operation {
            Operation::StashApply { id, pop } => (id, *pop),
            _ => {
                return Err(StashAttemptFailureV1::before(
                    "Stash inspection cannot execute another operation",
                ))
            }
        };
        if id.len() > 128 || plan.target.len() > 8192 || plan.effect.len() > 8192 {
            return Err(StashAttemptFailureV1::before(
                "Stash intent exceeds its capture bound",
            ));
        }
        // Metadata-only original-source checks precede ANY stash observer.
        origin
            .recheck(&self.cancellation)
            .map_err(|e| StashAttemptFailureV1::before(e.to_string()))?;
        if self.bare
            || self.navigation_read_window.is_some()
            || source_identity(self).map_err(|e| StashAttemptFailureV1::before(e.to_string()))?
                != *origin.source()
            || !self
                .actual_read_runner_v1()
                .same_instance(origin.original_repository_v1().actual_read_runner_v1())
        {
            return Err(StashAttemptFailureV1::before(
                "Stash intent does not belong to the original local checkout/producer",
            ));
        }
        let stash = self
            .resolve_stash(id)
            .map_err(|e| StashAttemptFailureV1::before(e.to_string()))?;
        if !super::local_commit_actor_origin_v1::original_saved_sha(&stash.id) {
            return Err(StashAttemptFailureV1::before(
                "Actual stash has no complete immutable identity",
            ));
        }
        origin
            .recheck(&self.cancellation)
            .map_err(|e| StashAttemptFailureV1::before(e.to_string()))?;
        let mut exact = plan.clone();
        exact.operation = Operation::StashApply {
            id: stash.id.clone(),
            pop,
        };
        let mut intent = FailedStashIntentV1 {
            origin,
            sha: stash.id,
            pop,
            command_succeeded: false,
        };
        match self.execute(&exact,true) {
            Ok(output)=>{intent.command_succeeded=true;Ok(StashAttemptSuccessV2{output,intent:Arc::new(intent)})},
            Err(error)=>Err(StashAttemptFailureV1 {
                message:format!("{error}\nThe failed/refused intent does not prove no effects. Inspect its original stash result; do not retry or drop automatically."),
                failed:Some(Arc::new(intent)),
            }),
        }
    }
}
