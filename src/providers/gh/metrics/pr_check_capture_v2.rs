//! Undeclared bridge child. Actual capture targeting, not current permission.
use super::*;

pub struct PrCheckCapturedTargetV2 {
    run_id: u64,
    run_node: String,
    attempt: u64,
    head: String,
    job_id: u64,
    job_node: Option<String>,
}
impl fmt::Debug for PrCheckCapturedTargetV2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrCheckCapturedTargetV2(<actual producer capture; not access>)")
    }
}
impl PrCheckCapturedTargetV2 {
    pub fn run_id(&self) -> u64 {
        self.run_id
    }
    pub fn run_node(&self) -> &str {
        &self.run_node
    }
    pub fn attempt(&self) -> u64 {
        self.attempt
    }
    pub fn head(&self) -> &str {
        &self.head
    }
    pub fn job_id(&self) -> u64 {
        self.job_id
    }
    pub fn job_node(&self) -> Option<&str> {
        self.job_node.as_deref()
    }
    /// Pure metadata equality. Does NOT supply any later authentication/access.
    pub fn matches_run(&self, run: &WorkflowRun, job: Option<u64>) -> bool {
        run.id == self.run_id
            && run.attempt == self.attempt
            && run.head_sha == self.head
            && job == Some(self.job_id)
    }
    pub(super) fn matched(origin: &PrCheckRunObservation) -> GithubResult<Self> {
        let link = origin.link.as_ref().ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "No actual matched capture target",
            )
        })?;
        let job_node = origin
            .selected_run_v1()
            .and_then(|run| run.members().iter().find(|m| m.id() == link.selected_job))
            .map(|m| m.node().to_owned());
        Ok(Self {
            run_id: link.attempt.run.id,
            run_node: link.run_node.clone(),
            attempt: link.attempt.run.attempt,
            head: link.attempt.run.head_sha.clone(),
            job_id: link.selected_job,
            job_node,
        })
    }
    pub(super) fn member(choice: &PrCheckChoiceV1) -> GithubResult<Self> {
        let run = choice.run()?;
        let member = choice.job()?.ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Actual member capture choice required",
            )
        })?;
        Ok(Self {
            run_id: run.run.id,
            run_node: run.raw_node,
            attempt: run.run.attempt,
            head: run.run.head_sha,
            job_id: member.id(),
            job_node: Some(member.node().to_owned()),
        })
    }
}
impl PrCheckLogsObservation {
    /// Issued inside the existing capture factory using its actual argv/member.
    pub fn captured_target_v2(&self) -> &PrCheckCapturedTargetV2 {
        &self.captured_target
    }
}
impl PrCheckRunObservation {
    /// Observed selected run only, never an index/default/URL reconstructed
    /// choice. The candidates and selected node came from the actual producer.
    pub(crate) fn choose_selected_run_v2(self: &Arc<Self>) -> GithubResult<PrCheckChoiceV1> {
        let selected = self
            .selected_run
            .as_ref()
            .ok_or_else(|| error(GithubErrorKind::InvalidTarget, "No genuinely selected run"))?;
        let index = self
            .run_candidates
            .iter()
            .position(|candidate| {
                candidate.run().id == selected.attempt.run.id
                    && candidate.node() == selected.run_node
            })
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Selected run not an original observed candidate",
                )
            })?;
        self.choose_run_v1(index)
    }
}
impl PrCheckSelectedRunV1 {
    pub fn node_v2(&self) -> &str {
        &self.run_node
    }
}
