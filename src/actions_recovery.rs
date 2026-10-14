//! Durable Actions recovery is a worker workflow, never a render-time SQL read.
use crate::{
    actions_workspace::{
        ActionsAction, ActionsPublicationAcknowledgement, ActionsPublicationInspectionPage,
        ActionsPublicationRow, ActionsView, ActionsWorkspace,
    },
    actions_write_journal::{
        ActionsInspectionDisposition, ActionsJournalInspection, ActionsWriteJournal,
        ActionsWriteState, ActionsWriteTarget,
    },
    providers::{
        gh::GithubProvider,
        runner::{sanitize_terminal, CancellationToken},
    },
    runtime::PreparedView,
    storage::Scope,
    theme::SemanticToken,
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

pub type SharedActionsInspection = Arc<Mutex<Option<ActionsJournalInspection>>>;

#[derive(Clone, Copy, Debug)]
pub enum RecoveryAcknowledgement {
    ObservedState(i64),
    InspectedAcceptance(i64),
}

fn display_lines(lines: impl IntoIterator<Item = String>) -> Vec<String> {
    lines
        .into_iter()
        .flat_map(|line| {
            crate::ui::notice_lines(&sanitize_terminal(&line), 68)
                .into_iter()
                .map(|line| {
                    line.spans
                        .iter()
                        .map(|span| span.content.as_ref())
                        .collect::<String>()
                })
        })
        .collect()
}

/// A bounded list does not establish absence. Exact known command targets use
/// the authoritative resource lookup, including accepted deployment exceptions.
pub fn decorate(
    workspace: &mut ActionsWorkspace,
    provider: &GithubProvider,
    journal: &ActionsWriteJournal,
) -> Result<(), String> {
    if !workspace
        .identity
        .host
        .eq_ignore_ascii_case(provider.host())
    {
        return Err("Actions publication host and verified workspace differ; private records are not loaded".into());
    }
    let scope = Scope::github(provider.host(), workspace.identity.account_id)
        .map_err(|error| error.to_string())?;
    let page = journal
        .list_unresolved(&scope, 256)
        .map_err(|error| error.to_string())?;
    let leases = crate::actions_workspace::ActionsPublicationLeaseSnapshot::prepare(
        &workspace.identity,
        &page,
    )
    .map_err(|error| error.to_string())?;
    workspace.publication_rows = page
        .markers
        .iter()
        .map(|marker| ActionsPublicationRow {
            attempt_id: marker.attempt_id,
            heading: format!(
                "ORIGINAL ACTIONS ATTEMPT #{} • {:?} • {:?}",
                marker.attempt_id, marker.kind, marker.state
            ),
            details: display_lines(crate::actions_routes::original_lines(marker)),
            role: match marker.state {
                ActionsWriteState::Pending | ActionsWriteState::AcceptedPending => {
                    SemanticToken::Pending
                }
                ActionsWriteState::Uncertain => SemanticToken::Failure,
                _ => SemanticToken::Muted,
            },
        })
        .collect();
    workspace.publication_total = Some(page.total);
    workspace.publication_complete = page.complete;
    workspace.publication_leases = Some(Arc::new(leases.clone()));
    workspace.prepare_rows();
    let candidates = workspace
        .controls
        .iter()
        .filter_map(|control| {
            let ActionsAction::Compose(command) = control.action else {
                return None;
            };
            Some((
                command,
                command.publication_kind()?,
                workspace.current_publication_target(command)?,
            ))
        })
        .collect::<Vec<_>>();
    let mut exact = std::collections::BTreeMap::new();
    for (command, kind, target) in candidates {
        let target = ActionsWriteTarget::new(provider, &workspace.identity, &target)
            .map_err(|error| error.to_string())?;
        let blocked = journal
            .blocking(&target, kind)
            .map_err(|error| error.to_string())?;
        for marker in &blocked {
            exact
                .entry(marker.attempt_id)
                .or_insert_with(|| marker.clone());
        }
        if let Some(marker) = blocked.first() {
            let reason=format!("Original attempt #{} remains {:?}; open Publications, inspect that exact result and explicitly acknowledge an eligible transition. Refresh never unlocks it.",marker.attempt_id,marker.state);
            for control in &mut workspace.controls {
                if control.action == ActionsAction::Compose(command) {
                    control.disabled_reason = Some(reason.clone());
                }
            }
        }
    }
    let final_leases = leases
        .with_exact_markers(&exact.into_values().collect::<Vec<_>>())
        .map_err(|error| error.to_string())?;
    workspace.publication_complete = final_leases.complete();
    workspace.publication_leases = Some(Arc::new(final_leases));
    Ok(())
}

/// Failed view preparation cannot discard a recorded original and enable retry.
pub fn decorate_or_unavailable(
    workspace: &mut ActionsWorkspace,
    provider: &GithubProvider,
    journal: &ActionsWriteJournal,
) -> Option<String> {
    match decorate(workspace, provider, journal) {
        Ok(()) => None,
        Err(error) => {
            workspace.publication_rows.clear();
            workspace.publication_total = None;
            workspace.publication_complete = false;
            workspace.publication_leases =
                crate::actions_workspace::ActionsPublicationLeaseSnapshot::unavailable(
                    &workspace.identity,
                )
                .ok()
                .map(Arc::new);
            workspace.prepare_rows();
            for control in &mut workspace.controls {
                if let ActionsAction::Compose(command) = control.action {
                    if command.publication_kind().is_some() {
                        control.disabled_reason=Some("Publication view unavailable; original records retained. Inspect the exact original before another effect; nothing is automatically resent.".into());
                    }
                }
            }
            Some(format!("publication view unavailable: {}; original record/reference retained, do not resend",sanitize_terminal(&error)))
        }
    }
}

fn hidden(status: &str) -> PreparedView {
    PreparedView {
        clear_private: true,
        clear_actions_inspection: true,
        status: status.into(),
        ..Default::default()
    }
}
fn failure(
    mut workspace: ActionsWorkspace,
    provider: &GithubProvider,
    error: String,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    if !provider.identity(cancel).is_ok_and(|identity| {
        identity.account_id.is_some_and(|id| id > 0) && identity.same_account(&workspace.identity)
    }) {
        return Ok(hidden("Actions account changed or unavailable • private inspection hidden; original protection retained; nothing resent"));
    }
    workspace.publication_inspection = None;
    workspace.prepare_rows();
    Ok(PreparedView {
        context: Some(workspace.context()),
        status: format!(
            "Actions recovery refused • {}; inspect the exact original again. Nothing resent.",
            sanitize_terminal(&error)
        ),
        actions: Some(workspace),
        clear_actions_inspection: true,
        ..Default::default()
    })
}

pub fn list(
    mut workspace: ActionsWorkspace,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let current = provider.identity(cancel);
    let Ok(current) = current else {
        return Ok(hidden(
            "Actions account unavailable • private publications hidden; nothing sent",
        ));
    };
    if current.account_id.is_none_or(|id| id == 0) || !current.same_account(&workspace.identity) {
        return Ok(hidden(
            "Actions account changed • private publications hidden; nothing sent",
        ));
    }
    workspace.identity = current;
    workspace.view = ActionsView::Publications;
    workspace.publication_inspection = None;
    workspace.selected = 0;
    workspace.top = 0;
    let journal = ActionsWriteJournal::open(database).map_err(|error| error.to_string())?;
    decorate(&mut workspace, provider, &journal)?;
    if !provider.identity(cancel).is_ok_and(|identity| {
        identity.account_id.is_some_and(|id| id > 0) && identity.same_account(&workspace.identity)
    }) {
        return Ok(hidden("Actions account changed before displaying publications • private details hidden; nothing sent"));
    }
    Ok(PreparedView {context:Some(workspace.context()),status:"Original unresolved Actions requests • all repositories for this verified account; no mutation or release".into(),actions:Some(workspace),clear_actions_inspection:true,..Default::default()})
}

pub fn inspect(
    mut workspace: ActionsWorkspace,
    attempt_id: i64,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let journal = ActionsWriteJournal::open(database).map_err(|error| error.to_string())?;
    let inspection =
        match journal.prepare_inspection(provider, &workspace.identity, attempt_id, cancel) {
            Ok(inspection) => inspection,
            Err(error) => return failure(workspace, provider, error.to_string(), cancel),
        };
    let disposition = inspection
        .disposition()
        .map_err(|error| error.to_string())?;
    let acknowledgement = if disposition.can_acknowledge() {
        Some(ActionsPublicationAcknowledgement::ObservedState)
    } else if disposition.can_acknowledge_inspected_acceptance() {
        Some(ActionsPublicationAcknowledgement::InspectedAcceptance)
    } else {
        None
    };
    let mut lines = crate::actions_routes::original_lines(inspection.marker());
    lines.push(match disposition {
        ActionsInspectionDisposition::Pending{..}=>"FRESH OBSERVED STATE • outcome/cause of the original invocation is not inferred".into(),
        ActionsInspectionDisposition::TransitionObserved=>"FRESH LATER TRANSITION • observed, not attributed to the original request".into(),
        ActionsInspectionDisposition::AcceptedStillPending=>"ACCEPTED REQUEST STILL WAITING • an overlapping request remains blocked".into(),
        ActionsInspectionDisposition::AcceptedEffectUnverified=>"FRESH INSPECTED ACCEPTANCE • effect/completion remains unverified; acknowledgement is distinct".into(),
    });
    let observed = serde_json::to_string_pretty(inspection.observation().state())
        .map_err(|error| error.to_string())?;
    if observed.len() <= 128 * 1024 {
        lines.extend(observed.lines().map(str::to_owned));
    } else {
        lines.push("Observed details exceed the bounded display budget • original typed proof retained; no effect is guessed".into());
    }
    workspace.identity = inspection.observation().identity().clone();
    workspace.view = ActionsView::PublicationInspection;
    workspace.publication_inspection = Some(ActionsPublicationInspectionPage {
        attempt_id,
        heading: format!("Inspect ORIGINAL Actions attempt #{attempt_id}"),
        lines: display_lines(lines),
        acknowledgement,
    });
    workspace.selected = 0;
    workspace.top = 0;
    decorate(&mut workspace, provider, &journal)?;
    Ok(PreparedView {context:Some(workspace.context()),status:"Original Actions result freshly inspected • explicit eligible acknowledgement still required; nothing resent".into(),actions:Some(workspace),actions_inspection:Some(Arc::new(Mutex::new(Some(inspection)))),clear_actions_inspection:true,..Default::default()})
}

pub fn acknowledge(
    mut workspace: ActionsWorkspace,
    acknowledgement: RecoveryAcknowledgement,
    slot: Option<SharedActionsInspection>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<PreparedView, String> {
    let (attempt_id, acceptance_only) = match acknowledgement {
        RecoveryAcknowledgement::ObservedState(id) => (id, false),
        RecoveryAcknowledgement::InspectedAcceptance(id) => (id, true),
    };
    let inspection = slot.and_then(|slot| {
        slot.lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .take()
    });
    let Some(inspection) = inspection else {
        return failure(workspace,provider,"Inspect this exact original attempt afresh; no reusable acknowledgement proof is available".into(),cancel);
    };
    if inspection.marker().attempt_id != attempt_id {
        return failure(
            workspace,
            provider,
            "The action refers to a different original attempt; protection remains".into(),
            cancel,
        );
    }
    let mut journal = ActionsWriteJournal::open(database).map_err(|error| error.to_string())?;
    let result = if acceptance_only {
        journal.acknowledge_inspected_acceptance(provider, inspection, &workspace.identity, cancel)
    } else {
        journal.acknowledge(provider, inspection, &workspace.identity, cancel)
    };
    if let Err(error) = result {
        return failure(workspace, provider, error.to_string(), cancel);
    }
    workspace.observation = None;
    workspace.reconciliation_target = None;
    workspace.publication_inspection = None;
    // An optional catalogue failure cannot undo the durable local acknowledgement.
    let refresh_error = workspace.load_catalog(provider, cancel).err();
    if !provider.identity(cancel).is_ok_and(|identity| {
        identity.account_id.is_some_and(|id| id > 0) && identity.same_account(&workspace.identity)
    }) {
        return Ok(hidden("Original Actions inspection durably acknowledged • account unavailable, private pages hidden; nothing resent"));
    }
    workspace.view = ActionsView::Publications;
    workspace.selected = 0;
    workspace.top = 0;
    let publication_error = decorate_or_unavailable(&mut workspace, provider, &journal);
    let message = if acceptance_only {
        "Inspected original acceptance acknowledged • effect/completion remains unverified; nothing resent"
    } else {
        "Original affected state acknowledged • no cause/completion inferred; nothing resent"
    };
    Ok(PreparedView {
        context: Some(workspace.context()),
        status: format!(
            "{message}{}{}",
            refresh_error.map_or(String::new(), |_| {
                " • optional catalogue refresh unavailable; acknowledgement retained".into()
            }),
            publication_error.map_or(String::new(), |error| format!(" • {error}"))
        ),
        actions: Some(workspace),
        clear_actions_inspection: true,
        ..Default::default()
    })
}

#[cfg(test)]
#[path = "actions_recovery_tests.rs"]
mod tests;
