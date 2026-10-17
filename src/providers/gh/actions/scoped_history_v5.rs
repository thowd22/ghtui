//! An actual bounded READ of a saved numeric run, never a live RunTarget or
//! publication/review source. Exact404 is unavailable, NOT deletion evidence.
use super::*;
use crate::actions_write_journal::ActionsHistoricalRunKeyV5;
use std::time::Instant;

pub(crate) struct ScopedHistoricalRunReadV5 {
    producer: CommandRunner,
    host: String,
    repository_id: u64,
    repository_node: String,
    reference: crate::actions_write_journal::ActionsPublicationReference,
    run_id: u64,
    run: Option<WorkflowRun>,
    cancel: CancellationToken,
    deadline: Instant,
    resource_v8: String,
    generation_v8: u64,
}
impl ScopedHistoricalRunReadV5 {
    pub(super) fn observed_for_v8<'a>(
        &'a self,
        provider: &GithubProvider,
        key: &ActionsHistoricalRunKeyV5,
        cancel: &CancellationToken,
    ) -> GithubResult<&'a WorkflowRun> {
        if !self.matches(provider, key, cancel) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original historical read expired/changed",
            ));
        }
        self.run.as_ref().ok_or_else(|| {
            error(
                GithubErrorKind::NotFound,
                "Original run unavailable; no current observation invented",
            )
        })
    }
    pub(crate) fn same_observation(&self, other: &Self) -> bool {
        self.host == other.host
            && self.repository_id == other.repository_id
            && self.repository_node == other.repository_node
            && self.reference == other.reference
            && self.run_id == other.run_id
            && self.producer.same_instance(&other.producer)
            && self.resource_v8 == other.resource_v8
            && self.generation_v8 == other.generation_v8
            && match (&self.run, &other.run) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    a.id == b.id
                        && a.workflow_id == b.workflow_id
                        && a.attempt == b.attempt
                        && a.head_sha == b.head_sha
                        && a.status == b.status
                        && a.conclusion == b.conclusion
                }
                _ => false,
            }
    }
    pub(crate) fn run(&self) -> Option<&WorkflowRun> {
        self.run.as_ref()
    }
    pub(crate) fn matches(
        &self,
        provider: &GithubProvider,
        key: &ActionsHistoricalRunKeyV5,
        cancel: &CancellationToken,
    ) -> bool {
        self.producer.same_instance(&provider.runner)
            && self.host == provider.host
            && self.repository_id == key.repository_id()
            && self.repository_node == key.repository_node()
            && self.reference == *key.reference()
            && self.run_id == key.run_id()
            && self.cancel.same_instance(cancel)
            && !cancel.is_cancelled()
            && Instant::now() < self.deadline
            && provider.runner.resource_generation(&self.resource_v8) == self.generation_v8
    }
    pub(crate) fn availability_label(&self) -> &'static str {
        if self.run.is_some() {
            "Actual exact numeric run is currently readable; historical receipt is NOT a new write target"
        } else {
            "Exact run endpoint currently unavailable (HTTP404); NOT proof of deletion, permissions, or causality"
        }
    }
}
impl GithubProvider {
    pub(crate) fn scoped_actions_historical_run_v5(
        &self,
        current: &CurrentCatalogueRepository,
        key: &ActionsHistoricalRunKeyV5,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedHistoricalRunReadV5> {
        self.read_historical_run_v8(current, key, None, cancel)
    }
    pub(crate) fn scoped_actions_retained_run_history_v8(
        &self,
        current: &CurrentCatalogueRepository,
        key: &ActionsHistoricalRunKeyV5,
        original: &std::sync::Arc<ScopedActionsRunTargetV1>,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedHistoricalRunReadV5> {
        if !original.current_resource_v2(self, current)
            || key.run_id() != original.run().id
            || key.repository_id() != original.repository_id()
            || Some(key.node()) != original.node_v2()
            || key.workflow_id() != original.run().workflow_id
            || key.suite() != original.suite_v3()
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Actual original Run differs from historical selection",
            ));
        }
        self.read_historical_run_v8(current, key, Some(original), cancel)
    }
    fn read_historical_run_v8(
        &self,
        current: &CurrentCatalogueRepository,
        key: &ActionsHistoricalRunKeyV5,
        original: Option<&ScopedActionsRunTargetV1>,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedHistoricalRunReadV5> {
        let (deadline, original_cap) = self.scoped_read_window.ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Original bounded history operation required",
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
                "Original historical repository unavailable",
            ));
        }
        let resource_v8 = format!(
            "{}/repository-id/{}/actions/run/{}",
            self.host,
            row.id,
            key.run_id()
        );
        let generation_v8 = self.runner.resource_generation(&resource_v8);
        let endpoint = format!("repos/{}/actions/runs/{}", row.full_name, key.run_id());
        let raw = if original.is_some() {
            // V8 performs two genuine raw metadata reads before closing its
            // PR/repository/principal. A failed child permanently consumes its
            // whole capture cap; two512KiB404s exhaust the unchanged1MiB pool.
            // Narrow THIS metadata child only, preserving the same immutable
            // pool, cancellation, original absolute deadline and lower issuer.
            // Ordinary V5/legacy reads keep their historical capture policy.
            self.clone()
                .with_scoped_read_window(deadline, original_cap.min(256 * 1024))?
                .api(&endpoint, "GET", None, None, false, cancel)
        } else {
            self.api(&endpoint, "GET", None, None, false, cancel)
        };
        let run = match raw {
            Ok(raw) => {
                if let Some(original) = original {
                    super::scoped_read_v1::bind_scoped_actions_run_raw_v3(
                        &raw,
                        &row.full_name,
                        original,
                        super::scoped_read_v1::ScopedActionsRunRawPhaseV3::HistoricalReadV8,
                    )?;
                }
                let repository =
                    raw.get("repository")
                        .filter(|v| v.is_object())
                        .ok_or_else(|| {
                            error(
                                GithubErrorKind::Json,
                                "Actual historical run repository unavailable",
                            )
                        })?;
                let node = raw.get("node_id").and_then(Value::as_str);
                let suite = match raw.get("check_suite_id") {
                    None | Some(Value::Null) => None,
                    Some(value) => Some(
                        value
                            .as_u64()
                            .filter(|id| *id > 0 && *id <= i64::MAX as u64)
                            .ok_or_else(|| {
                                error(
                                    GithubErrorKind::Json,
                                    "Actual historical check suite malformed",
                                )
                            })?,
                    ),
                };
                let run = run_from_json(&raw);
                if raw
                    .get("run_attempt")
                    .and_then(Value::as_u64)
                    .filter(|a| *a > 0)
                    != Some(run.attempt)
                    || run.id != key.run_id()
                    || run.workflow_id != key.workflow_id()
                    || node != Some(key.node())
                    || suite != key.suite()
                    || repository.get("id").and_then(Value::as_u64) != Some(key.repository_id())
                    || repository.get("node_id").and_then(Value::as_str)
                        != Some(key.repository_node())
                    || repository.get("full_name").and_then(Value::as_str)
                        != Some(row.full_name.as_str())
                    || validate_sha(&run.head_sha).is_err()
                    || crate::actions_workspace::scoped_read_v1::run_owned_bytes(&run) > 128 * 1024
                {
                    return Err(error(
                        GithubErrorKind::IdentityChanged,
                        "Actual historical numeric run/metadata changed; no saved target promoted",
                    ));
                }
                Some(run)
            }
            Err(failure) if matches!(failure.kind, GithubErrorKind::NotFound) => None,
            Err(failure) => return Err(failure),
        };
        if cancel.is_cancelled()
            || Instant::now() >= deadline
            || self.runner.resource_generation(&resource_v8) != generation_v8
        {
            return Err(error(
                GithubErrorKind::Transport,
                "Original historical operation ended",
            ));
        }
        Ok(ScopedHistoricalRunReadV5 {
            producer: self.runner.clone(),
            host: self.host.clone(),
            repository_id: row.id,
            repository_node: key.repository_node().into(),
            reference: key.reference().clone(),
            run_id: key.run_id(),
            run,
            cancel: cancel.clone(),
            deadline,
            resource_v8,
            generation_v8,
        })
    }
}
