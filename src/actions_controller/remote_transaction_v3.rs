//! ONE existing durable Actions transaction and typed receipt ordering.
//! Private policies vary only the already-approved provider/source boundary.
use super::*;
use crate::{
    actions_workspace::{ActionsPublication, ActionsPublicationDescriptor, ActionsRefresh},
    actions_write_journal::ActionsWritePermit,
    providers::gh::ActionsInvocationFailureV3,
};

pub(super) trait RemoteTransactionPolicyV3 {
    fn begin(
        &mut self,
        journal: &mut ActionsWriteJournal,
        descriptor: &ActionsPublicationDescriptor,
    ) -> anyhow::Result<ActionsWritePermit> {
        journal.begin(&descriptor.target, &descriptor.intent)
    }
    fn publish(
        &mut self,
        journal: &ActionsWriteJournal,
        permit: &ActionsWritePermit,
    ) -> Result<ActionsPublication, ActionsInvocationFailureV3>;
    fn refresh(&mut self, publication: ActionsPublication) -> ActionsRefresh;
    fn final_identity(&mut self, journal: &ActionsWriteJournal) -> Result<HostIdentity, String>;
    fn private_on_local_refusal(&self) -> bool {
        true
    }
    fn begin_error(&self, failure: &anyhow::Error) -> String {
        failure.to_string()
    }
    fn claim_error(&self, failure: &anyhow::Error) -> String {
        format!("Publication permit was not authorized; no effect was sent: {failure}")
    }
    fn publication_error(
        &self,
        failure: &GithubError,
        kind: ActionsWriteKind,
        private_visible: bool,
    ) -> String;
    fn not_started_error(&self, _failure: &GithubError) -> String {
        "No Actions write was invoked; its original pending publication remains protected. Inspect the original attempt before another action; nothing was resent.".into()
    }
}

pub(super) struct LegacyTransactionV3<'a> {
    pub(super) provider: &'a GithubProvider,
    pub(super) plan: &'a ActionsPlan,
    pub(super) cancel: &'a CancellationToken,
}
impl RemoteTransactionPolicyV3 for LegacyTransactionV3<'_> {
    fn publish(
        &mut self,
        _journal: &ActionsWriteJournal,
        _permit: &ActionsWritePermit,
    ) -> Result<ActionsPublication, ActionsInvocationFailureV3> {
        // Legacy None intentionally keeps its exact historical conservative
        // uncertainty classification, transport order and messages.
        self.plan
            .publish(self.provider, true, self.cancel)
            .map_err(ActionsInvocationFailureV3::WriteMayHaveStarted)
    }
    fn refresh(&mut self, publication: ActionsPublication) -> ActionsRefresh {
        publication.refresh_checked(self.provider, self.cancel)
    }
    fn final_identity(&mut self, _journal: &ActionsWriteJournal) -> Result<HostIdentity, String> {
        fresh_identity(self.provider, &self.plan.identity, self.cancel)
    }
    fn publication_error(
        &self,
        failure: &GithubError,
        kind: ActionsWriteKind,
        private_visible: bool,
    ) -> String {
        publication_error(failure, kind, private_visible)
    }
}

pub(super) fn execute<M, P>(
    journal: &mut ActionsWriteJournal,
    descriptor: &ActionsPublicationDescriptor,
    ordered_metadata: M,
    policy: &mut P,
) -> ActionsExecution
where
    M: FnOnce(&ActionsWriteMarker) -> Result<(), String>,
    P: RemoteTransactionPolicyV3,
{
    let permit = match policy.begin(journal, descriptor) {
        Ok(permit) => permit,
        Err(error) => {
            return refused(
                policy.begin_error(&error),
                policy.private_on_local_refusal(),
            )
        }
    };
    let reference = permit.marker().publication_reference();
    if ordered_metadata(permit.marker()).is_err() {
        return refused("Original publication metadata could not be durably saved; no effect was sent and its pending marker remains blocked",policy.private_on_local_refusal());
    }
    if let Err(error) = journal.claim(&permit, &descriptor.target, &descriptor.intent) {
        return refused(
            policy.claim_error(&error),
            policy.private_on_local_refusal(),
        );
    }
    let publication = match policy.publish(journal, &permit) {
        Ok(value) => value,
        Err(ActionsInvocationFailureV3::NotStarted(error)) => {
            // No write invocation occurred. The original claimed PENDING
            // barrier/reference is retained, never silently retried/released.
            let private_visible = policy.final_identity(journal).is_ok();
            return refused(policy.not_started_error(&error), private_visible);
        }
        Err(ActionsInvocationFailureV3::WriteMayHaveStarted(error)) => {
            let persisted = journal.mark_uncertain(&permit);
            let private_visible = policy.final_identity(journal).is_ok();
            let mut message =
                policy.publication_error(&error, descriptor.intent.kind, private_visible);
            if persisted.is_err() {
                message
                    .push_str(" Pending marker retained because uncertainty persistence failed.");
            }
            return ActionsExecution::Unknown {
                publication: reference,
                error: message,
                private_visible,
            };
        }
    };
    let disposition = publication.disposition.clone();
    let (receipt, acceptance, persisted) = match &disposition {
        ActionsDisposition::VerifiedEffect { receipt } => (
            receipt.clone(),
            None,
            journal.mark_confirmed(&permit, receipt),
        ),
        ActionsDisposition::Accepted {
            receipt,
            observation,
        } => (
            receipt.clone(),
            Some(observation.clone()),
            journal.mark_accepted_pending(&permit, receipt),
        ),
        ActionsDisposition::LocalOnly => {
            let _ = journal.mark_uncertain(&permit);
            return ActionsExecution::Unknown {publication:reference,error:"Remote plan returned no typed effect/acceptance; original marker remains blocked and nothing may be resent".into(),private_visible:false};
        }
    };
    if persisted.is_err() {
        let private_visible = policy.final_identity(journal).is_ok();
        return ActionsExecution::ReceiptPersistenceFailed {publication:reference,disposition,error:"GitHub returned a typed effect/acceptance but its durable receipt failed; pending barrier remains. Do not resend; inspect the exact original result.".into(),private_visible};
    }
    // Optional I/O is strictly after the durable synchronous/accepted receipt.
    let mut refreshed = policy.refresh(publication);
    let current = match policy.final_identity(journal) {
        Ok(current) => current,
        Err(error) => return refresh_failure(reference, receipt, acceptance, None, error, false),
    };
    refreshed.outcome.workspace.identity = current;
    if refreshed.refresh_failed {
        return refresh_failure(reference,receipt,acceptance,Some(refreshed.outcome),"Original effect/acceptance is durably recorded; optional view refresh failed. Do not resend.".into(),true);
    }
    match acceptance {
        Some(observation) => ActionsExecution::AcceptedPending {
            publication: reference,
            receipt,
            observation,
            outcome: refreshed.outcome,
        },
        None => ActionsExecution::Confirmed {
            publication: reference,
            receipt,
            outcome: refreshed.outcome,
        },
    }
}
