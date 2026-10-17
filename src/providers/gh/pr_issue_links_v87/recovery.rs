//! Fresh two-version inspection, never permission from a saved Issue identity.
use super::*;
use crate::pr_write_journal::issue_link_origin_v87::history_v87::IssueLinkRecoveryWitnessV87;
use std::sync::atomic::{AtomicBool, Ordering};

pub(crate) struct IssueLinkRecoveryCurrentV87 {
    actual: Arc<OriginalIssueLinksV87>,
    runner: crate::providers::runner::CommandRunner,
    used: AtomicBool,
}
impl IssueLinkRecoveryCurrentV87 {
    pub(crate) fn prepare_inspection_lines_v87(
        &self,
        other_retained: usize,
    ) -> GithubResult<Vec<String>> {
        alive(self.cancellation(), self.deadline())?;
        let actual = &self.actual;
        let issue = &actual.pair.issue;
        let rows = actual
            .pair
            .display_row_count_v87()
            .checked_add(16)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        let mut bytes = 8192usize;
        for (_, set) in actual.pair.display_sets_v87() {
            for value in set {
                let (repository, _, url) = value.display_fact_v87();
                bytes = bytes
                    .checked_add(repository.len())
                    .and_then(|n| n.checked_add(url.len() + 256))
                    .ok_or_else(|| refused(GithubErrorKind::Transport))?;
            }
        }
        actual
            .retained
            .checked_add(other_retained)
            .and_then(|n| n.checked_add(bytes.checked_mul(64)?))
            .and_then(|n| n.checked_add(rows.checked_mul(512)?))
            .filter(|n| *n <= 8 * 1024 * 1024)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        let mut lines = vec![
            format!(
                "Fresh actual PR: {}#{} • node {} / numeric {}",
                actual.parent.repository(),
                actual.parent.number(),
                actual.parent.node_id(),
                actual.parent.database_id()
            ),
            format!(
                "Fresh selected Issue: {}#{} • node {} / numeric {}",
                issue.repository.name, issue.number, issue.node, issue.id
            ),
            format!("Issue source: {}", issue.url),
            format!(
                "Actual H {} • actual base {}",
                actual.parent.head_sha(),
                actual.parent.base_tip_sha()
            ),
        ];
        for (label, set) in actual.pair.display_sets_v87() {
            lines.push(format!("{label} • {} actual observed entries", set.len()));
            for value in set {
                let (repository, number, url) = value.display_fact_v87();
                lines.push(format!("{repository}#{number} • {url}"));
            }
        }
        lines.push("Six sets may overlap. Current observation is not receipt causation or permission to resend.".into());
        Ok(lines)
    }
    pub(crate) fn actual(&self) -> &Arc<OriginalIssueLinksV87> {
        &self.actual
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.actual.deadline()
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.actual.binding.cancellation
    }
    pub(crate) fn matches_witness_v87(&self, witness: &IssueLinkRecoveryWitnessV87) -> bool {
        let actual = &self.actual;
        let (name, repo, node, issue, id, number) = witness.selected_issue_v87();
        witness.matches_parent_v87(actual.parent(), actual.identity())
            && witness.command().is_ok_and(|v| v == actual.command)
            && actual.pair.issue.repository.name == name
            && actual.pair.issue.repository.id == repo
            && actual.pair.issue.repository.node == node
            && actual.pair.issue.node == issue
            && actual.pair.issue.id == id
            && actual.pair.issue.number == number
    }
    pub(in crate::providers) fn runner_parts_v87(
        &self,
        runner: &crate::providers::runner::CommandRunner,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<(&[String; 2], [u64; 2], Instant)> {
        let actual = &self.actual;
        let deadline = deadline.min(actual.deadline());
        alive(cancel, deadline)?;
        if !self.runner.same_instance(runner)
            || !cancel.same_instance(&actual.binding.cancellation)
            || self.used.swap(true, Ordering::AcqRel)
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok((&actual.resources, actual.generations, deadline))
    }
}
impl GithubProvider {
    pub(crate) fn inspect_original_issue_link_v87(
        &self,
        actual: Arc<OriginalIssueLinksV87>,
        cancel: &CancellationToken,
    ) -> GithubResult<IssueLinkRecoveryCurrentV87> {
        if !matches!(
            actual.purpose,
            IssueLinkReadPurposeV87::Inspection | IssueLinkReadPurposeV87::MetadataInspectionV87
        ) {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        actual.binding.validate_live(self, cancel)?;
        self.validate_original_issue_link_generations_v87(&actual, cancel, actual.deadline())?;
        actual
            .retained
            .checked_add(2 * std::mem::size_of::<IssueLinkRecoveryCurrentV87>())
            .filter(|n| *n <= MODEL_CAP_V87)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        Ok(IssueLinkRecoveryCurrentV87 {
            actual,
            runner: self.runner.clone(),
            used: AtomicBool::new(false),
        })
    }
    pub(crate) fn validate_issue_link_inspection_v87(
        &self,
        observed: &IssueLinkRecoveryCurrentV87,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        if observed.used.load(Ordering::Acquire)
            || !matches!(
                observed.actual.purpose,
                IssueLinkReadPurposeV87::Inspection
                    | IssueLinkReadPurposeV87::MetadataInspectionV87
            )
        {
            return Err(refused(GithubErrorKind::InvalidTarget));
        }
        // The caller's real Core alone closes its read pool. After that this
        // is lineage/token/min-I/two-version validation, NEVER a new read grant.
        self.validate_original_issue_link_generations_v87(
            &observed.actual,
            cancel,
            deadline.min(observed.deadline()),
        )
    }
    pub(crate) fn reconcile_issue_link_inspection_v87(
        &self,
        observed: IssueLinkRecoveryCurrentV87,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        self.validate_issue_link_inspection_v87(&observed, cancel, deadline)?;
        self.runner
            .reconcile_issue_link_inspection_v87(&observed, cancel, deadline)
            .map_err(GithubError::from)
    }
}
