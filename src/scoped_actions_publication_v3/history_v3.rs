//! Explicit fresh original-source inspection/ACK. Never a retry or an issuer
//! reconstructed from saved labels. SAME Actions observation and journal commit.
use super::*;
use crate::actions_write_journal::{
    ActionsInspectionDisposition, ActionsJournalInspection, ActionsPublicationReference,
    ActionsWriteJournal, ActionsWriteMarker, ActionsWriteState,
};

pub(crate) struct ScopedActionsHistoryV3 {
    marker: ActionsWriteMarker,
    inspection: Option<ActionsJournalInspection>,
    lines: Vec<String>,
    display: Arc<PreparedFormTextV1>,
    owned: usize,
    owned_history_v8: Option<Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>>,
    snapshot_v8: Option<crate::actions_write_journal::ActionsHistorySnapshotV5>,
    read_v8: Option<ScopedHistoricalRunReadV5>,
    acceptance_v8: bool,
}
impl std::fmt::Debug for ScopedActionsHistoryV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedActionsHistoryV3(<fresh observed original; not resend authority>)")
    }
}
impl ScopedActionsHistoryV3 {
    pub(crate) fn owned_history_v8(
        &self,
    ) -> Option<&Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>> {
        self.owned_history_v8.as_ref()
    }
    /// Saved kind is descriptive history, never a new command/approval.
    pub(crate) fn command_label_v4(&self) -> &'static str {
        use crate::actions_write_journal::ActionsWriteKind as K;
        match self.marker.kind {
            K::RerunAll => "Rerun all jobs",
            K::RerunFailed => "Rerun failed jobs",
            K::RerunJob => "Rerun selected job",
            K::ForceCancel => "Force-cancel run",
            K::DeleteRun => "Delete run",
            K::DeleteLogs => "Delete run logs",
            _ => "Cancel",
        }
    }
    pub(crate) fn reference(&self) -> ActionsPublicationReference {
        self.marker.publication_reference()
    }
    pub(crate) fn lines(&self) -> &[String] {
        &self.lines
    }
    pub(crate) fn display(&self) -> &Arc<PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn owned_bytes(&self) -> usize {
        self.owned
    }
    pub(crate) fn acknowledge_available(&self) -> bool {
        self.inspection
            .as_ref()
            .and_then(|inspection| inspection.disposition().ok())
            .is_some_and(|state| state.can_acknowledge())
    }
    pub(crate) fn acceptance_available_v5(&self) -> bool {
        self.acceptance_v8
            || self
                .inspection
                .as_ref()
                .and_then(|inspection| inspection.disposition().ok())
                .is_some_and(|state| state.can_acknowledge_inspected_acceptance())
    }
    pub(crate) fn validate(&self, database: &Path) -> Result<(), String> {
        let journal =
            ActionsWriteJournal::open(database).map_err(|_| "Original journal unavailable")?;
        if let Some(proof) = &self.owned_history_v8 {
            journal
                .validate_owned_run_history_v8(proof)
                .map_err(|_| "Original own Run row changed")?;
        }
        if let Some(snapshot) = &self.snapshot_v8 {
            journal
                .validate_history_v5(snapshot)
                .map_err(|_| "Original Run snapshot changed")?;
        }
        if let Some(inspection) = &self.inspection {
            journal
                .validate_scoped_inspection_v3(inspection)
                .map_err(|_| "Original inspection changed")?;
        } else {
            journal
                .validate_scoped_marker_saved_v3(&self.marker)
                .map_err(|_| "Original protected result changed")?;
        }
        Ok(())
    }
}
#[allow(clippy::too_many_arguments)] // distinct original source/clock/producer proof arguments
fn prepared(
    journal: &ActionsWriteJournal,
    provider: &GithubProvider,
    identity: &HostIdentity,
    target: &Arc<ScopedActionsRunTargetV1>,
    reference: &ActionsPublicationReference,
    cancel: &CancellationToken,
    deadline: Instant,
    source: &ScopedActionsSourceV1,
) -> GithubResult<ScopedActionsHistoryV3> {
    let marker = journal
        .scoped_marker_v3(provider, identity, reference, target)
        .map_err(|_| refuse("Saved original does not match this actual numeric run/source"))?;
    if marker.context.original_pr_v3
        != source
            .pr_origin_v2()
            .map(|origin| origin.durable_pr_context_v3())
    {
        return Err(refuse("Original PR-bound publication requires its SAME actual original PR/head source; repository labels cannot adopt it"));
    }
    let inspection = if matches!(
        marker.state,
        ActionsWriteState::Pending
            | ActionsWriteState::Uncertain
            | ActionsWriteState::AcceptedPending
    ) {
        Some(
            journal
                .prepare_scoped_inspection_v3(provider, identity, reference, target, cancel)
                .map_err(|_| refuse("Original fresh observation unavailable; nothing unlocked"))?,
        )
    } else {
        None
    };
    let mut lines = crate::actions_routes::original_lines(&marker);
    lines.pop(); // scoped history has no generic F9 new-writer escape.
    if matches!(
        marker.kind,
        crate::actions_write_journal::ActionsWriteKind::RerunAll
            | crate::actions_write_journal::ActionsWriteKind::RerunFailed
            | crate::actions_write_journal::ActionsWriteKind::RerunJob
    ) {
        lines.push(format!(
            "Original debug logging: {} • saved descriptive option, not new approval",
            marker.options.debug_logging
        ));
    }
    lines.extend([
        format!("Current numeric repository {} • actual current name {}",target.repository_id(),target.repository()),
        format!("Fresh actual run {} / attempt {} / head {}",target.run().id,target.run().attempt,target.run().head_sha),
        format!("Observed state: {} / {}",target.run().status,target.run().conclusion),
        "This later observation cannot prove who caused an effect. No request is repeated by inspection or acknowledgement.".into(),
    ]);
    if let Some(inspection) = &inspection {
        let disposition = inspection
            .disposition()
            .map_err(|_| refuse("Original observation classification unavailable"))?;
        lines.push(match disposition {
            ActionsInspectionDisposition::Pending{..}=>"Unknown/Pending original result • explicit F4 acknowledges only this displayed fresh observation; it does NOT claim cancellation occurred.",
            ActionsInspectionDisposition::TransitionObserved=>"Later run transition observed • explicit F4 can release the original lease; no causal claim or resend.",
            ActionsInspectionDisposition::AcceptedStillPending=>"Accepted original run transition still pending • F4 unavailable; no repeated request or completed-effect claim.",
            ActionsInspectionDisposition::AcceptedEffectUnverified=>"EFFECT UNVERIFIED • F4 does not confirm deletion. Separate F6 acknowledges only inspected acceptance, never execution/causality/retry.",
        }.into());
    } else {
        lines.push("Original result is already closed • no acknowledgement or new confirmation is available.".into());
    }
    let display = PreparedFormTextV1::prepare(&lines, cancel, deadline)
        .map_err(|_| refuse("Original history presentation exceeds shared bounds"))?;
    // Marker/observation JSON is preflighted by SAME journal <=128KiB, run
    // by tiny_workspace <=128KiB. Conservative bounded owned parser headroom
    // (not measured RSS) plus actual prepared document/line capacities.
    let owned = std::mem::size_of::<ScopedActionsHistoryV3>()
        + owned_lines(&lines)
        + display.retained_bytes()
        + 1024 * 1024;
    if owned > 8 * 1024 * 1024
        || owned.saturating_add(display.preparation_peak_bound()) > 16 * 1024 * 1024
    {
        return Err(refuse(
            "Original history preparation exceeds bounded capacities",
        ));
    }
    clock(cancel, deadline)?;
    Ok(ScopedActionsHistoryV3 {
        marker,
        inspection,
        lines,
        display,
        owned,
        owned_history_v8: None,
        snapshot_v8: None,
        read_v8: None,
        acceptance_v8: false,
    })
}
#[allow(clippy::too_many_arguments)] // same bounded original open phase, not a new context issuer
pub(super) fn restore(
    intent: &Arc<ScopedActionsIntentV3>,
    local: &LocalV3,
    provider: &GithubProvider,
    identity: &HostIdentity,
    target: &Arc<ScopedActionsRunTargetV1>,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Option<Arc<ScopedActionsHistoryV3>>> {
    local.validate(database, &intent.source)?;
    let body = Cache::open(database)
        .map_err(|_| refuse("Original draft cache unavailable"))?
        .bounded_draft(intent.source.original().scope(), intent.key(), 16 * 1024)
        .map_err(|_| refuse("Original draft reference unavailable"))?;
    let Some(body) = body else {
        let journal = ActionsWriteJournal::open(database)
            .map_err(|_| refuse("Original journal unavailable"))?;
        let reference = journal
            .scoped_reference_for_run_v3(provider, identity, target)
            .map_err(|_| {
                refuse("Original protected inventory incomplete/ambiguous; no new writer opened")
            })?;
        let Some(reference) = reference else {
            return Ok(None);
        };
        let history = prepared(
            &journal,
            provider,
            identity,
            target,
            &reference,
            cancel,
            deadline,
            &intent.source,
        )?;
        local.validate(database, &intent.source)?;
        return Ok(Some(Arc::new(history)));
    };
    let value: serde_json::Value = serde_json::from_str(&body)
        .map_err(|_| refuse("Original draft reference is malformed; no writer opened"))?;
    if value.get("version").and_then(serde_json::Value::as_u64) != Some(3)
        || value.get("purpose").and_then(serde_json::Value::as_str) != Some("scoped-actions-v3")
    {
        return Err(refuse(
            "Saved reference has another purpose; no writer opened",
        ));
    }
    let Some(reference) = value
        .get("actions_publication")
        .filter(|value| !value.is_null())
    else {
        return Ok(None);
    };
    let reference: ActionsPublicationReference = serde_json::from_value(reference.clone())
        .map_err(|_| refuse("Protected reference malformed; no writer opened"))?;
    let journal =
        ActionsWriteJournal::open(database).map_err(|_| refuse("Original journal unavailable"))?;
    let result = prepared(
        &journal,
        provider,
        identity,
        target,
        &reference,
        cancel,
        deadline,
        &intent.source,
    )?;
    local.validate(database, &intent.source)?;
    Ok(Some(Arc::new(result)))
}
#[allow(clippy::too_many_arguments)]
fn inspect_owned_v8(
    opened: &Arc<OpenedActionsComposerV3>,
    reference: &ActionsPublicationReference,
    proof: &Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>,
    displayed: Option<&Arc<ScopedActionsHistoryV3>>,
    acknowledge: bool,
    acceptance_only: bool,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<Arc<ScopedActionsHistoryV3>> {
    clock(cancel, deadline)?;
    let intent = opened.intent();
    let source = intent.source();
    if proof.reference() != *reference || !proof.original().same_watch_pin_v2(&intent.original) {
        return Err(refuse("Actual own Run history origin differs"));
    }
    let mut journal = ActionsWriteJournal::open(database)
        .map_err(|_| refuse("Original Run journal unavailable"))?;
    opened
        .local
        .validate_run_history_v8(database, source, &journal, proof)?;
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase.begin_repository_catalogue_access(
        source.original().source(),
        &access_limits(),
        cancel,
    )?;
    scope(source, session.identity())?;
    let window = session.reserve_measured_external_reads(6, 512 * 1024, 1024 * 1024, 2, cancel)?;
    let measured = phase.clone().with_measured_read_window(window)?;
    session.verify(source.reference(), cancel)?;
    let current = session
        .observed_original_v1(source.reference().id)
        .ok_or_else(|| refuse("Original Run repository unavailable"))?;
    let parent = source
        .pr_origin_v2()
        .map(|p| p.fresh_history_parent_v5(&measured, current, cancel))
        .transpose()?;
    let snapshot = journal
        .scoped_history_v5(
            &measured,
            session.identity(),
            current,
            parent.as_ref(),
            Some(reference),
        )
        .map_err(|_| refuse("Actual own Run history row unavailable"))?;
    let key = snapshot
        .key()
        .map_err(|_| refuse("Selected history is not an original Run"))?;
    let read =
        measured.scoped_actions_retained_run_history_v8(current, &key, proof.original(), cancel)?;
    let mut marker = snapshot
        .selected()
        .ok_or_else(|| refuse("Original Run row missing"))?
        .clone();
    let mut inspection = if !matches!(
        marker.kind,
        crate::actions_write_journal::ActionsWriteKind::DeleteRun
    ) && matches!(
        marker.state,
        ActionsWriteState::Pending
            | ActionsWriteState::Uncertain
            | ActionsWriteState::AcceptedPending
    ) && read.run().is_some()
    {
        Some(
            journal
                .prepare_run_history_inspection_v8(
                    &snapshot,
                    &read,
                    &measured,
                    session.identity(),
                    current,
                    cancel,
                )
                .map_err(|_| refuse("Actual Run observation unavailable"))?,
        )
    } else {
        None
    };
    let deletion_acceptance = marker.state == ActionsWriteState::AcceptedPending
        && marker.receipt.as_ref().is_some_and(|receipt| {
            matches!(
                (marker.kind, receipt.kind),
                (
                    crate::actions_write_journal::ActionsWriteKind::DeleteRun,
                    crate::actions_write_journal::ActionsReceiptKind::RunDeletionAccepted
                ) | (
                    crate::actions_write_journal::ActionsWriteKind::DeleteLogs,
                    crate::actions_write_journal::ActionsReceiptKind::LogsDeleted
                )
            )
        });
    if acknowledge {
        let old = displayed
            .ok_or_else(|| refuse("Explicit current displayed Run inspection required"))?;
        if old.reference() != *reference
            || old
                .owned_history_v8()
                .is_none_or(|p| !Arc::ptr_eq(p, proof))
            || !old
                .read_v8
                .as_ref()
                .is_some_and(|old| old.same_observation(&read))
        {
            return Err(refuse(
                "Displayed actual Run observation changed; inspect again",
            ));
        }
        journal
            .validate_history_v5(
                old.snapshot_v8
                    .as_ref()
                    .ok_or_else(|| refuse("Displayed Run snapshot missing"))?,
            )
            .map_err(|_| refuse("Displayed original Run row changed"))?;
        if acceptance_only {
            if !old.acceptance_available_v5() || !deletion_acceptance {
                return Err(refuse("No typed accepted deletion to acknowledge"));
            }
        } else if !old.acknowledge_available()
            || !inspection
                .as_ref()
                .and_then(|i| i.disposition().ok())
                .is_some_and(|d| d.can_acknowledge())
        {
            return Err(refuse(
                "Original transition still pending or unobservable; protection retained",
            ));
        }
    }
    let next = if acknowledge {
        Some(
            if !acceptance_only && marker.state == ActionsWriteState::AcceptedPending {
                ActionsWriteState::TransitionObserved
            } else {
                ActionsWriteState::Acknowledged
            },
        )
    } else {
        None
    };
    let mut lines = Vec::new();
    if let Some(parent) = &parent {
        lines.push(format!("CURRENT genuine PR #{} / numeric {} / H {} / B {} • original saved PR H/B remain historical",
        parent.number(),parent.database_id(),parent.head_sha(),parent.base_tip_sha()));
    }
    let mut original_lines = crate::actions_routes::original_lines(&marker);
    original_lines.pop();
    lines.extend(original_lines);
    lines.push(read.availability_label().into());
    if let Some(run) = read.run() {
        lines.push(format!("Actual observed run {} / A {} / H {} / {} / {} • saved original A/H above remain historical",
        run.id,run.attempt,run.head_sha,run.status,run.conclusion));
    }
    if deletion_acceptance {
        lines.push("EFFECT UNVERIFIED • F4 does not confirm deletion; separate F6 acknowledges only inspected typed acceptance. HTTP404 is unavailable, not deletion or causality proof.".into());
    } else if let Some(i) = &inspection {
        lines.push(match i.disposition().map_err(|_|refuse("Run disposition unavailable"))?{
        ActionsInspectionDisposition::Pending{..}=>"Pending/unknown original • explicit F4 acknowledges only this current observation, not an effect or retry.",
        ActionsInspectionDisposition::TransitionObserved=>"Later original run transition observed • F4 releases only this exact lease; no causality/workflow success claim.",
        ActionsInspectionDisposition::AcceptedStillPending=>"Accepted original transition still pending • F4 unavailable; no repeat request.",
        ActionsInspectionDisposition::AcceptedEffectUnverified=>"EFFECT UNVERIFIED • separate F6 inspected acceptance only.",
    }.into());
    } else {
        lines.push("Original result already closed or current observation unavailable • no inferred completed effect or new approval.".into());
    }
    if acknowledge {
        lines.push(if acceptance_only{"Explicit inspected acceptance acknowledged locally • effect UNVERIFIED • original receipt retained • no remote mutation/retry"}
        else{"Explicit original observation acknowledged locally • no causal claim • no remote mutation/retry"}.into());
    }
    let display = PreparedFormTextV1::prepare(&lines, cancel, deadline)
        .map_err(|_| refuse("Run history presentation exceeds shared bounds"))?;
    let closing =
        measured.scoped_actions_retained_run_history_v8(current, &key, proof.original(), cancel)?;
    if !read.same_observation(&closing) {
        return Err(refuse(
            "Actual original Run observation changed during inspection",
        ));
    }
    let closing_parent = source
        .pr_origin_v2()
        .map(|p| p.fresh_history_parent_v5(&measured, current, cancel))
        .transpose()?;
    if match (&parent, &closing_parent) {
        (None, None) => false,
        (Some(a), Some(b)) => !GithubProvider::same_thread_history_parent_v1(a, b),
        _ => true,
    } {
        return Err(refuse(
            "Current original PR parent changed during Run inspection",
        ));
    }
    let mut read = Some(read);
    let pending = if acknowledge {
        Some(
            journal
                .prepare_run_history_ack_v8(
                    &snapshot,
                    inspection.take(),
                    read.take()
                        .ok_or_else(|| refuse("Original Run read missing"))?,
                    &measured,
                    cancel,
                    acceptance_only,
                    Some(proof),
                )
                .map_err(|_| {
                    refuse("Original observed Run ACK unavailable; protection retained")
                })?,
        )
    } else {
        None
    };
    let owned = std::mem::size_of::<ScopedActionsHistoryV3>()
        + owned_lines(&lines)
        + display.retained_bytes()
        + snapshot.owned_bytes()
        + proof.retained_bytes()
        + 1024 * 1024
        + pending.as_ref().map_or(0, |p| p.retained_bytes());
    if owned > 8 * 1024 * 1024
        || owned.saturating_add(display.preparation_peak_bound()) > 16 * 1024 * 1024
    {
        return Err(refuse("Run history retained/preparation capacity exceeded"));
    }
    session.verify(source.reference(), cancel)?;
    let closed = session.close_with_guard(cancel, || {
        opened
            .local
            .validate_run_history_v8(database, source, &journal, proof)?;
        journal
            .validate_history_v5(&snapshot)
            .map_err(|_| refuse("Original Run row changed before final UID"))?;
        if let Some(pending) = &pending {
            journal
                .validate_run_history_ack_v8(pending)
                .map_err(|_| refuse("Original Run ACK changed before final UID"))?;
        }
        clock(cancel, deadline)
    })?;
    scope(source, closed.identity())?;
    let owned_history = if let Some(pending) = pending {
        journal
            .acknowledge_run_history_v8(&measured, pending, closed.identity(), cancel)
            .map_err(|_| refuse("Original exact Run ACK failed; durable protection retained"))?
            .ok_or_else(|| refuse("Actual committed Run successor missing"))?
    } else {
        proof.clone()
    };
    if let Some(next) = next {
        marker.state = next;
    }
    let value = Arc::new(ScopedActionsHistoryV3 {
        marker,
        inspection,
        lines,
        display,
        owned,
        owned_history_v8: Some(owned_history),
        snapshot_v8: (!acknowledge).then_some(snapshot),
        read_v8: read,
        acceptance_v8: deletion_acceptance && !acknowledge,
    });
    clock(cancel, deadline)?;
    Ok(value)
}
#[allow(clippy::too_many_arguments)]
pub(crate) fn inspect_with_acceptance_v5(
    opened: &Arc<OpenedActionsComposerV3>,
    reference: &ActionsPublicationReference,
    displayed: Option<&Arc<ScopedActionsHistoryV3>>,
    acknowledge: bool,
    acceptance_only: bool,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    owned_history_v8: Option<&Arc<crate::actions_write_journal::ActionsOwnedRunHistoryV8>>,
) -> GithubResult<Arc<ScopedActionsHistoryV3>> {
    if let Some(proof) = owned_history_v8 {
        return inspect_owned_v8(
            opened,
            reference,
            proof,
            displayed,
            acknowledge,
            acceptance_only,
            provider,
            database,
            cancel,
            deadline,
        );
    }
    clock(cancel, deadline)?;
    opened.validate_local(database)?;
    let intent = opened.intent();
    let phase = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = phase.begin_repository_catalogue_access(
        intent.source.original().source(),
        &access_limits(),
        cancel,
    )?;
    scope(&intent.source, session.identity())?;
    let window = session.reserve_measured_external_reads(15, 512 * 1024, 1024 * 1024, 2, cancel)?;
    let measured = phase.clone().with_measured_read_window(window)?;
    session.verify(intent.source.reference(), cancel)?;
    let current = session
        .observed_original_v1(intent.source.reference().id)
        .ok_or_else(|| refuse("Original repository unavailable"))?;
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(&measured, session.identity(), current, cancel)?;
    }
    let target = measured.scoped_actions_run_snapshot_v3(current, &intent.original, cancel)?;
    tiny_workspace(session.identity(), &target)?; // same preclone metadata bound
    let mut journal =
        ActionsWriteJournal::open(database).map_err(|_| refuse("Original journal unavailable"))?;
    let mut history = prepared(
        &journal,
        &measured,
        session.identity(),
        &target,
        reference,
        cancel,
        deadline,
        &intent.source,
    )?;
    if acknowledge {
        let old = displayed
            .and_then(|history| history.inspection.as_ref())
            .ok_or_else(|| refuse("Explicit fresh displayed inspection required before ACK"))?;
        let fresh = history
            .inspection
            .as_ref()
            .ok_or_else(|| refuse("Original result no longer awaits ACK"))?;
        journal
            .same_scoped_inspection_v3(old, fresh)
            .map_err(|_| refuse("Displayed original generation changed; inspect again"))?;
        if if acceptance_only {
            !history.acceptance_available_v5()
        } else {
            !history.acknowledge_available()
        } {
            return Err(refuse(
                "Accepted Cancel still pending; no lease was released",
            ));
        }
        // Prepare honest anticipated presentation BEFORE final source/UID close.
        let next = if !acceptance_only && history.marker.state == ActionsWriteState::AcceptedPending
        {
            ActionsWriteState::TransitionObserved
        } else {
            ActionsWriteState::Acknowledged
        };
        let mut marker = history.marker.clone();
        marker.state = next;
        history.lines = crate::actions_routes::original_lines(&marker);
        history.lines.pop();
        history.lines.push("Explicit original observation acknowledged • no remote mutation/retry • Close and refresh actual run before a new intent".into());
        history.display = PreparedFormTextV1::prepare(&history.lines, cancel, deadline)
            .map_err(|_| refuse("ACK display exceeds shared presentation bounds"))?;
        history.owned = std::mem::size_of::<ScopedActionsHistoryV3>()
            + owned_lines(&history.lines)
            + history.display.retained_bytes()
            + 1024 * 1024;
        if history
            .owned
            .saturating_add(history.display.preparation_peak_bound())
            > 16 * 1024 * 1024
        {
            return Err(refuse("ACK presentation capacity exceeded"));
        }
    }
    let pending_ack = if acknowledge {
        let inspection = history
            .inspection
            .take()
            .ok_or_else(|| refuse("Original inspection missing"))?;
        Some(
            journal
                .prepare_scoped_ack_mode_v5(inspection, acceptance_only)
                .map_err(|_| refuse("Original ACK observation unavailable"))?,
        )
    } else {
        None
    };
    let closing = measured.scoped_actions_run_snapshot_v3(
        session
            .observed_original_v1(intent.source.reference().id)
            .ok_or_else(|| refuse("Original repository unavailable"))?,
        &target,
        cancel,
    )?;
    if !same_run(&target, &closing) {
        return Err(refuse("Observed run changed during original inspection"));
    }
    drop(closing);
    if let Some(pr) = intent.source.pr_origin_v2() {
        pr.fresh_parent(
            &measured,
            session.identity(),
            session
                .observed_original_v1(intent.source.reference().id)
                .ok_or_else(|| refuse("Original repository unavailable"))?,
            cancel,
        )?;
    }
    session.verify(intent.source.reference(), cancel)?;
    let closed = session.close_with_guard(cancel, || {
        opened.validate_local(database)?;
        if let Some(inspection) = &history.inspection {
            journal
                .validate_scoped_inspection_v3(inspection)
                .map_err(|_| refuse("Original journal generation changed before final UID"))?;
        }
        if let Some(pending) = &pending_ack {
            journal
                .validate_scoped_ack_v3(pending)
                .map_err(|_| refuse("Original ACK journal generation changed before final UID"))?;
        }
        clock(cancel, deadline)
    })?;
    scope(&intent.source, closed.identity())?;
    if let Some(pending) = pending_ack {
        let next = if !acceptance_only && history.marker.state == ActionsWriteState::AcceptedPending
        {
            ActionsWriteState::TransitionObserved
        } else {
            ActionsWriteState::Acknowledged
        };
        journal
            .acknowledge_scoped_inspection_v3(&measured, pending, closed.identity())
            .map_err(|_| refuse("Original ACK commit unavailable; protection retained"))?;
        history.marker.state = next;
    }
    let result = Arc::new(history);
    clock(cancel, deadline)?;
    Ok(result)
}
