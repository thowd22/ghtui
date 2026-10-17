//! Durable worker publication boundary. Native journeys are verified separately.
use crate::{
    actions_workspace::{
        ActionsAcceptedObservation, ActionsDisposition, ActionsOutcome, ActionsPlan,
    },
    actions_write_journal::{
        ActionsPublicationReference, ActionsReceipt, ActionsWriteJournal, ActionsWriteKind,
        ActionsWriteMarker,
    },
    providers::{
        gh::{GithubError, GithubErrorKind, GithubProvider, HostIdentity},
        runner::{sanitize_terminal, CancellationToken},
    },
};
mod local_only;
mod remote_transaction_v3;
mod scoped_dispatch_v7;
mod scoped_run_v3;
mod scoped_workflow_v6;
mod workflow_remote_v9;
pub(crate) use scoped_dispatch_v7::{execute as execute_scoped_dispatch_v7, DispatchExecutionV7};
pub(crate) use scoped_run_v3::execute as execute_scoped_run_v3;
pub(crate) use scoped_run_v3::RunExecutionV8;
pub(crate) use scoped_workflow_v6::{execute as execute_scoped_workflow_v6, WorkflowExecutionV6};
pub(crate) use workflow_remote_v9::{
    execute as execute_workflow_remote_v9, WorkflowRemoteExecutionV9,
};

#[derive(Debug)]
pub enum ActionsExecution {
    Refused {
        error: String,
        private_visible: bool,
    },
    /// Always present the ORIGINAL marker, not the newly edited form's request.
    RestoredOriginal {
        marker: ActionsWriteMarker,
    },
    NoRemoteEffect {
        outcome: ActionsOutcome,
    },
    Unknown {
        publication: ActionsPublicationReference,
        error: String,
        private_visible: bool,
    },
    Confirmed {
        publication: ActionsPublicationReference,
        receipt: ActionsReceipt,
        outcome: ActionsOutcome,
    },
    AcceptedPending {
        publication: ActionsPublicationReference,
        receipt: ActionsReceipt,
        observation: ActionsAcceptedObservation,
        outcome: ActionsOutcome,
    },
    /// A typed result exists remotely, but its local barrier remains pending.
    ReceiptPersistenceFailed {
        publication: ActionsPublicationReference,
        disposition: ActionsDisposition,
        error: String,
        private_visible: bool,
    },
    /// A synchronous effect is durable even though its optional view is unavailable.
    EffectConfirmedButRefreshFailed {
        publication: ActionsPublicationReference,
        receipt: ActionsReceipt,
        outcome: Option<ActionsOutcome>,
        error: String,
        private_visible: bool,
    },
    /// Never classify asynchronous acceptance as a confirmed requested effect.
    AcceptedButRefreshFailed {
        publication: ActionsPublicationReference,
        receipt: ActionsReceipt,
        observation: ActionsAcceptedObservation,
        outcome: Option<ActionsOutcome>,
        error: String,
        private_visible: bool,
    },
}
fn refused(error: impl Into<String>, private_visible: bool) -> ActionsExecution {
    ActionsExecution::Refused {
        error: sanitize_terminal(&error.into()),
        private_visible,
    }
}
fn refresh_failure(
    publication: ActionsPublicationReference,
    receipt: ActionsReceipt,
    acceptance: Option<ActionsAcceptedObservation>,
    outcome: Option<ActionsOutcome>,
    error: String,
    private_visible: bool,
) -> ActionsExecution {
    match acceptance {
        Some(observation) => ActionsExecution::AcceptedButRefreshFailed {
            publication,
            receipt,
            observation,
            outcome,
            error,
            private_visible,
        },
        None => ActionsExecution::EffectConfirmedButRefreshFailed {
            publication,
            receipt,
            outcome,
            error,
            private_visible,
        },
    }
}
fn fresh_identity(
    provider: &GithubProvider,
    expected: &HostIdentity,
    cancel: &CancellationToken,
) -> Result<HostIdentity, String> {
    if expected.account_id.is_none_or(|id| id == 0)
        || !expected.host.eq_ignore_ascii_case(provider.host())
    {
        return Err("Actions require a verified positive principal on the exact selected host; private details remain hidden".into());
    }
    let current=provider.identity(cancel).map_err(|_|"Actions identity unavailable; private details remain hidden and the original publication is retained".to_string())?;
    if !current.same_account(expected) {
        return Err("Actions account changed; private details remain hidden and the original publication is retained".into());
    }
    Ok(current)
}
fn publication_error(error: &GithubError, kind: ActionsWriteKind, private_visible: bool) -> String {
    if !private_visible {
        return "Actions account changed or is unavailable; private publication details are hidden. Inspect with the original account; nothing was resent.".into();
    }
    if kind == ActionsWriteKind::SetSecret {
        return format!("Secret publication outcome is unknown ({:?}); its original marker remains blocked. Inspect before another attempt; no secret value is displayed.",error.kind);
    }
    sanitize_terminal(&format!("{error}; original publication remains blocked. Inspect and explicitly acknowledge before another attempt; nothing is automatically resent."))
}

/// Call on restored form OPEN, before preparing new secret/body/head fields.
/// A marker is an original state, not necessarily a confirmed effect.
pub fn restore_original(
    provider: &GithubProvider,
    journal: &ActionsWriteJournal,
    expected: &HostIdentity,
    reference: &ActionsPublicationReference,
    cancel: &CancellationToken,
) -> ActionsExecution {
    if cancel.is_cancelled() {
        return refused(
            "Original publication inspection cancelled; nothing was sent",
            true,
        );
    }
    if let Err(error) = fresh_identity(provider, expected, cancel) {
        return refused(error, false);
    }
    let marker = match journal.original_publication(provider, expected, reference) {
        Ok(marker) => marker,
        Err(error) => {
            return refused(
                format!("Original publication cannot be verified; do not resend: {error}"),
                true,
            )
        }
    };
    if let Err(error) = fresh_identity(provider, expected, cancel) {
        return refused(error, false);
    }
    ActionsExecution::RestoredOriginal { marker }
}

/// The callback durably saves only the original marker reference/allowed fields.
/// It runs after begin and BEFORE claim or any invocation. Never save secret input.
#[allow(clippy::too_many_arguments)]
pub fn execute<F>(
    provider: &GithubProvider,
    journal: &mut ActionsWriteJournal,
    plan: &ActionsPlan,
    confirmed: bool,
    restored: Option<&ActionsPublicationReference>,
    ordered_metadata: F,
    cancel: &CancellationToken,
) -> ActionsExecution
where
    F: FnOnce(&ActionsWriteMarker) -> Result<(), String>,
{
    if !confirmed {
        return refused("Confirmation required; no Actions effect was sent", true);
    }
    if cancel.is_cancelled() {
        return refused(
            "Actions publication cancelled before invocation; no effect was sent",
            true,
        );
    }
    if let Some(reference) = restored {
        return restore_original(provider, journal, &plan.identity, reference, cancel);
    }
    // Derive intent from the private immutable operation, not public labels/fields.
    let descriptor = match plan.publication_descriptor(provider) {
        Ok(descriptor) => descriptor,
        Err(error) => {
            return refused(
                error.to_string(),
                !matches!(
                    error.kind,
                    GithubErrorKind::IdentityChanged | GithubErrorKind::Authentication
                ),
            )
        }
    };
    if let Err(error) = fresh_identity(provider, &plan.identity, cancel) {
        return refused(error, false);
    }
    let Some(descriptor) = descriptor else {
        let publication = match plan.publish(provider, true, cancel) {
            Ok(value) => value,
            Err(error) => {
                return local_only::failed_publication(provider, &plan.identity, &error, cancel)
            }
        };
        if !matches!(publication.disposition, ActionsDisposition::LocalOnly) {
            return refused("Local Actions operation unexpectedly produced a remote result; no result is guessed",false);
        }
        let mut refreshed = publication.refresh_checked(provider, cancel);
        let current = match fresh_identity(provider, &plan.identity, cancel) {
            Ok(current) => current,
            Err(error) => return refused(error, false),
        };
        refreshed.outcome.workspace.identity = current;
        return ActionsExecution::NoRemoteEffect {
            outcome: refreshed.outcome,
        };
    };
    let mut policy = remote_transaction_v3::LegacyTransactionV3 {
        provider,
        plan,
        cancel,
    };
    remote_transaction_v3::execute(journal, &descriptor, ordered_metadata, &mut policy)
}

#[cfg(test)]
#[path = "actions_controller/tests.rs"]
mod tests;
