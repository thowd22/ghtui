//! Retained exact own Dispatch READ/ACK. No saved source or writer is minted.
use crate::{
    actions_write_journal::{
        dispatch_acceptance_available_v7, ActionsHistorySnapshotV5, ActionsOwnedDispatchHistoryV7,
        ActionsPublicationReference, ActionsWriteJournal,
    },
    prepared_form_text_v1::PreparedFormTextV1,
    providers::{gh::*, runner::CancellationToken},
    scoped_actions_dispatch_publication_v7::OpenedDispatchV7,
    scoped_actions_publication_v3 as shared,
};
use std::{path::Path, sync::Arc, time::Instant};
fn safe() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message:
            "Original Dispatch history/source/receipt changed • protection retained • no retry"
                .into(),
    }
}
pub(crate) struct DispatchHistoryV7 {
    reference: ActionsPublicationReference,
    snapshot: ActionsHistorySnapshotV5,
    observed: Option<ScopedHistoricalDispatchReadV7>,
    lines: Vec<String>,
    display: Arc<PreparedFormTextV1>,
    available: bool,
    owned: Arc<ActionsOwnedDispatchHistoryV7>,
}
impl DispatchHistoryV7 {
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
    pub(crate) fn owned_history(&self) -> Option<&Arc<ActionsOwnedDispatchHistoryV7>> {
        Some(&self.owned)
    }
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn inspect(
    opened: &Arc<OpenedDispatchV7>,
    reference: &ActionsPublicationReference,
    owned: &Arc<ActionsOwnedDispatchHistoryV7>,
    previous: Option<&Arc<DispatchHistoryV7>>,
    acknowledge: bool,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Arc<DispatchHistoryV7>> {
    shared::clock(cancel, deadline)?;
    let mut journal = ActionsWriteJournal::open(database).map_err(|_| safe())?;
    if owned.reference() != *reference {
        return Err(safe());
    }
    opened.validate_history(database, &journal, owned)?;
    let source = opened.intent().source();
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
        .ok_or_else(safe)?;
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
        .map_err(|_| safe())?;
    let key = snapshot.dispatch_key_v7().map_err(|_| safe())?;
    if key.workflow_id() != opened.intent().original().model().id
        || key.node() != opened.intent().original().node_v6().unwrap_or("")
        || key.path() != opened.intent().original().model().path
    {
        return Err(safe());
    }
    let observed = measured.scoped_actions_historical_dispatch_v7(current, &key, cancel)?;
    let marker = snapshot.selected().ok_or_else(safe)?;
    if acknowledge {
        let prior = previous
            .filter(|prior| prior.available && prior.reference == *reference)
            .ok_or_else(safe)?;
        journal
            .validate_history_v5(&prior.snapshot)
            .map_err(|_| safe())?;
        if !prior
            .observed
            .as_ref()
            .is_some_and(|old| old.same_observation(&observed))
        {
            return Err(safe());
        }
    }
    let available = dispatch_acceptance_available_v7(marker);
    let mut lines = crate::actions_routes::original_lines(marker);
    lines.pop();
    lines.push(format!(
        "HISTORICAL Dispatch {} {} • tip {} • definition {} • API {}",
        key.context().namespace,
        key.context().raw_name,
        key.context().tip,
        key.context().commit,
        key.context().protocol
    ));
    lines.push(observed.availability_label().into());
    if let Some(run) = observed.run() {
        lines.push(format!(
            "Actual returned run {} / A {} / H {} / {} / {} • not workflow completion/causation",
            run.id, run.attempt, run.head_sha, run.status, run.conclusion
        ));
    }
    if let Some(parent) = &parent {
        lines.push(format!("CURRENT genuine PR #{} / numeric{} / H {} / B {} • saved dispatch PR heads remain historical",parent.number(),parent.database_id(),parent.head_sha(),parent.base_tip_sha()));
    }
    if acknowledge {
        lines.push("Explicit inspected Dispatch acceptance ACK • LOCAL ONLY • original receipt retained • no POST/retry".into());
    }
    let display =
        crate::scoped_actions_workflow_publication_v6::display_v6(&lines, cancel, deadline)?;
    let mut observed = Some(observed);
    let pending = if acknowledge {
        Some(
            journal
                .prepare_dispatch_ack_v7(
                    &snapshot,
                    observed.take().ok_or_else(safe)?,
                    &measured,
                    cancel,
                    Some(owned),
                )
                .map_err(|_| safe())?,
        )
    } else {
        None
    };
    let retained = opened.retained()
        + snapshot.owned_bytes()
        + lines.capacity() * std::mem::size_of::<String>()
        + lines.iter().map(String::capacity).sum::<usize>()
        + display.retained_bytes()
        + owned.retained_bytes()
        + 512 * 1024
        + pending
            .as_ref()
            .map_or(0, |pending| pending.retained_bytes());
    if retained > 8 * 1024 * 1024
        || retained.saturating_add(display.preparation_peak_bound()) > 16 * 1024 * 1024
    {
        return Err(safe());
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
        return Err(safe());
    }
    session.verify(source.reference(), cancel)?;
    let closed = session.close_with_guard(cancel, || {
        opened.validate_history(database, &journal, owned)?;
        journal.validate_history_v5(&snapshot).map_err(|_| safe())?;
        if let Some(pending) = &pending {
            journal
                .validate_dispatch_ack_v7(pending)
                .map_err(|_| safe())?;
        }
        shared::clock(cancel, deadline)
    })?;
    shared::scope(source, closed.identity())?;
    let owned = if let Some(pending) = pending {
        journal
            .acknowledge_dispatch_v7(&measured, pending, closed.identity(), cancel)
            .map_err(|_| safe())?
            .ok_or_else(safe)?
    } else {
        owned.clone()
    };
    let result = Arc::new(DispatchHistoryV7 {
        reference: reference.clone(),
        snapshot,
        observed,
        lines,
        display,
        available: available && !acknowledge,
        owned,
    });
    shared::clock(cancel, deadline)?;
    Ok(result)
}
