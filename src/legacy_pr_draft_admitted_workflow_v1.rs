//! SOURCE ONLY combined14 with ONE capture ledger and original Window. No
//! native epoch/witness-save integration, publisher/retry/ACK or I/O in draw.
use crate::{
    forms::{FormPurpose, PrIntent},
    legacy_pr_draft_admitted_v1::{LegacyAdmittedDraftFailure, LegacyPrDraftAdmitted},
    legacy_pr_draft_binding_v1::LegacyPrDraftWitness,
    legacy_pr_draft_open_v1::{open_legacy_pr_draft_workflow_v1, LegacyPrDraftFailure},
    providers::{
        gh::{
            GithubErrorKind, GithubProvider, LegacyPrDraftWorkflowBudget,
            LegacyPrWorkflowAccounting, Repository,
        },
        runner::CancellationToken,
    },
};
use std::{fmt, path::Path};

pub struct LegacyPrWholeDraftReady {
    ready: LegacyPrDraftAdmitted,
    accounting: LegacyPrWorkflowAccounting,
}
impl fmt::Debug for LegacyPrWholeDraftReady {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "LegacyPrWholeDraftReady(<private; bounded captures not write/access authority>)",
        )
    }
}
impl LegacyPrWholeDraftReady {
    pub fn accounting(&self) -> LegacyPrWorkflowAccounting {
        self.accounting
    }
    pub fn into_admitted(self) -> LegacyPrDraftAdmitted {
        self.ready
    }
}
pub fn open_admitted_legacy_pr_draft_workflow_v1(
    mut intent: PrIntent,
    original_repository: Option<&Repository>,
    provider: &GithubProvider,
    database: &Path,
    workflow: &mut LegacyPrDraftWorkflowBudget,
    cancel: &CancellationToken,
) -> Result<LegacyPrWholeDraftReady, LegacyAdmittedDraftFailure> {
    if original_repository.is_some_and(|repo| {
        repo.id == 0
            || repo.id > i64::MAX as u64
            || repo.host != intent.identity.host
            || !repo
                .full_name
                .eq_ignore_ascii_case(&intent.target.pr.repository)
    }) {
        return Err(LegacyAdmittedDraftFailure::Access(
            GithubErrorKind::InvalidTarget,
        ));
    }
    let initial = provider
        .legacy_pr_draft_access_budgeted_v1(&intent.identity, &intent.target, workflow, cancel)
        .map_err(|failure| LegacyAdmittedDraftFailure::Access(failure.kind))?;
    if original_repository.is_some_and(|repo| repo.id != initial.repository().id) {
        return Err(LegacyAdmittedDraftFailure::Access(
            GithubErrorKind::IdentityChanged,
        ));
    }
    initial
        .validate_for(
            provider,
            &intent.identity,
            &intent.target,
            workflow.window(),
            cancel,
        )
        .map_err(|failure| LegacyAdmittedDraftFailure::Access(failure.kind))?;
    intent.identity = initial.identity().clone();
    let original = intent.target.clone();
    let witness = LegacyPrDraftWitness::issue(
        &initial,
        provider,
        &intent.identity,
        &original,
        intent.kind,
        workflow.window(),
        cancel,
    )
    .map_err(LegacyAdmittedDraftFailure::Local)?;
    let local = open_legacy_pr_draft_workflow_v1(
        intent,
        provider,
        database,
        cancel,
        workflow,
        Some(&witness),
    );
    let expected = local
        .as_ref()
        .map_or(initial.identity(), |ready| ready.identity());
    let final_access = provider
        .revalidate_legacy_pr_draft_access_budgeted_v1(&initial, expected, workflow, cancel)
        .map_err(|failure| LegacyAdmittedDraftFailure::Access(failure.kind))?;
    final_access
        .validate_for(
            provider,
            final_access.identity(),
            &original,
            workflow.window(),
            cancel,
        )
        .map_err(|failure| LegacyAdmittedDraftFailure::Access(failure.kind))?;
    let (mut form, _) = local
        .map_err(LegacyAdmittedDraftFailure::Local)?
        .into_parts();
    let identity = final_access.identity().clone();
    let FormPurpose::Pr(intent) = &mut form.purpose else {
        return Err(LegacyAdmittedDraftFailure::Local(
            LegacyPrDraftFailure::ConstructorContractChanged,
        ));
    };
    intent.identity = identity.clone();
    let accounting = workflow
        .verify_complete(provider, &identity, cancel)
        .map_err(|failure| LegacyAdmittedDraftFailure::Access(failure.kind))?;
    // This file is a child of the admitted module: no new public raw constructor
    // or wider field visibility is added at activation.
    let ready = LegacyPrDraftAdmitted {
        form,
        identity,
        admission: final_access,
        initial_accounting: *initial.accounting(),
    };
    Ok(LegacyPrWholeDraftReady { ready, accounting })
}
