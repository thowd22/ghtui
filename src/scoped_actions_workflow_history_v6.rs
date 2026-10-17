//! Fresh closed workflow observation and exact-row local ACK; never resend.
use super::*;
use crate::actions_write_journal::{
    workflow_ack_available_v6, ActionsHistorySnapshotV5, ActionsPublicationReference,
};
pub(crate) struct WorkflowHistoryV6 {
    reference: ActionsPublicationReference,
    snapshot: ActionsHistorySnapshotV5,
    observed: Option<ScopedHistoricalWorkflowReadV6>,
    lines: Vec<String>,
    display: Arc<PreparedFormTextV1>,
    available: bool,
    owned_history: Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>,
}
impl WorkflowHistoryV6 {
    pub(crate) fn owned_history(
        &self,
    ) -> Option<&Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>> {
        Some(&self.owned_history)
    }
    pub(crate) fn reference(&self) -> &ActionsPublicationReference {
        &self.reference
    }
    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn acknowledge_available(&self) -> bool {
        self.available
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn inspect(
    opened: &Arc<OpenedWorkflowV6>,
    reference: &ActionsPublicationReference,
    owned_history: &Arc<crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6>,
    previous: Option<&Arc<WorkflowHistoryV6>>,
    acknowledge: bool,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Arc<WorkflowHistoryV6>> {
    shared::clock(cancel, deadline)?;
    let mut journal = ActionsWriteJournal::open(database)
        .map_err(|_| error("Original workflow journal unavailable"))?;
    if owned_history.reference() != *reference {
        return Err(error("Actual own workflow reference differs"));
    }
    opened.local.validate_workflow_history_v6(
        database,
        opened.intent().source(),
        &journal,
        owned_history,
    )?;
    let intent = opened.intent();
    let source = intent.source();
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase.begin_repository_catalogue_access(
        source.original().source(),
        &shared::access_limits(),
        cancel,
    )?;
    shared::scope(source, session.identity())?;
    let window = session.reserve_measured_external_reads(6, 512 * 1024, 1024 * 1024, 2, cancel)?;
    let measured = phase.clone().with_measured_read_window(window)?;
    session.verify(source.reference(), cancel)?;
    let current = session
        .observed_original_v1(source.reference().id)
        .ok_or_else(|| error("Original workflow repository unavailable"))?;
    let parent = if let Some(pr) = source.pr_origin_v2() {
        Some(pr.fresh_history_parent_v5(&measured, current, cancel)?)
    } else {
        None
    };
    let snapshot = journal
        .scoped_history_v5(
            &measured,
            session.identity(),
            current,
            parent.as_ref(),
            Some(reference),
        )
        .map_err(|_| error("Original workflow history unavailable"))?;
    let key = snapshot
        .workflow_key_v6()
        .map_err(|_| error("Original record is not exact workflow history"))?;
    if key.workflow_id() != opened.target().model().id
        || key.node() != opened.target().node_v6().unwrap_or("")
        || key.path() != opened.target().model().path
    {
        return Err(error(
            "Original selected workflow differs from protected history",
        ));
    }
    let observed = measured.scoped_actions_historical_workflow_v6(current, &key, cancel)?;
    let marker = snapshot
        .selected()
        .ok_or_else(|| error("Original workflow marker missing"))?;
    if acknowledge {
        let prior = previous
            .filter(|prior| prior.available && prior.reference == *reference)
            .ok_or_else(|| error("Explicitly inspect original workflow before ACK"))?;
        journal
            .validate_history_v5(&prior.snapshot)
            .map_err(|_| error("Displayed workflow history changed"))?;
        if !prior
            .observed
            .as_ref()
            .is_some_and(|old| old.same_observation(&observed))
        {
            return Err(error(
                "Displayed workflow observation changed; inspect again, no release",
            ));
        }
    }
    let available = workflow_ack_available_v6(marker, &observed);
    let mut lines = crate::actions_routes::original_lines(marker);
    lines.pop();
    lines.push(observed.availability_label().into());
    if let Some(workflow) = observed.workflow() {
        lines.push(format!(
            "Actual current workflow {} • path {} • state {} • observed state, not causation",
            workflow.id, workflow.path, workflow.state
        ));
    }
    if let Some(parent) = &parent {
        lines.push(format!("CURRENT genuine PR #{} / numeric {} / H {} / B {} • saved publication H/B remain historical",parent.number(),parent.database_id(),parent.head_sha(),parent.base_tip_sha()));
    }
    if acknowledge {
        lines.push("Explicit original workflow state acknowledged locally • exact receipt retained • no PUT/resend".into());
    }
    let display = display_v6(&lines, cancel, deadline)?;
    let mut observed = Some(observed);
    let pending = if acknowledge {
        Some(
            journal
                .prepare_workflow_ack_v6(
                    &snapshot,
                    observed
                        .take()
                        .ok_or_else(|| error("Original workflow observation missing"))?,
                    &measured,
                    cancel,
                    Some(owned_history),
                )
                .map_err(|_| error("Workflow transition unavailable; protection retained"))?,
        )
    } else {
        None
    };
    let bytes = opened.retained()
        + snapshot.owned_bytes()
        + lines_bytes(&lines)
        + display.retained_bytes()
        + 512 * 1024
        + owned_history.retained_bytes()
        + pending
            .as_ref()
            .map_or(0, |pending| pending.retained_bytes());
    if bytes > 8 * 1024 * 1024
        || bytes.saturating_add(display.preparation_peak_bound()) > 16 * 1024 * 1024
    {
        return Err(error("Workflow history capacity exceeded"));
    }
    let closing = if let Some(pr) = source.pr_origin_v2() {
        Some(pr.fresh_history_parent_v5(&measured, current, cancel)?)
    } else {
        None
    };
    if match (&parent, &closing) {
        (None, None) => false,
        (Some(a), Some(b)) => !GithubProvider::same_thread_history_parent_v1(a, b),
        _ => true,
    } {
        return Err(error("Current PR parent changed during workflow history"));
    }
    session.verify(source.reference(), cancel)?;
    let closed = session.close_with_guard(cancel, || {
        opened
            .local
            .validate_workflow_history_v6(database, source, &journal, owned_history)?;
        shared::clock(cancel, deadline)?;
        journal
            .validate_history_v5(&snapshot)
            .map_err(|_| error("Original workflow metadata changed"))?;
        if let Some(ack) = &pending {
            journal
                .validate_workflow_ack_v6(ack)
                .map_err(|_| error("Original workflow ACK changed"))?;
        }
        Ok(())
    })?;
    shared::scope(source, closed.identity())?;
    // SAME Immediate raw row CAS is the only local effect after final UID.
    let owned_history = if let Some(ack) = pending {
        journal
            .acknowledge_workflow_v6(&measured, ack, closed.identity(), cancel)
            .map_err(|_| error("Exact workflow ACK metadata drift; protection retained"))?
            .ok_or_else(|| error("Actual committed workflow successor missing"))?
    } else {
        owned_history.clone()
    };
    let value = Arc::new(WorkflowHistoryV6 {
        reference: reference.clone(),
        snapshot,
        observed,
        lines,
        display,
        available: available && !acknowledge,
        owned_history,
    });
    shared::clock(cancel, deadline)?;
    Ok(value)
}
