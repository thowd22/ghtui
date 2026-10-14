//! SOURCE ONLY child of the legacy OPEN. Both standalone and budgeted paths
//! must forward to this one restoration policy at activation.
use super::*;
use crate::providers::gh::{LegacyPrDraftWorkflowBudget, LegacyPrIdentityPhase};

pub(super) struct OpenReadArguments<'a> {
    pub intent: PrIntent,
    pub provider: &'a GithubProvider,
    pub database: &'a Path,
    pub cancel: &'a CancellationToken,
    pub deadline: Instant,
    pub witness: Option<&'a LegacyPrDraftWitness>,
}
pub(super) fn open_with_identity_reader_v1(
    args: OpenReadArguments<'_>,
    mut identity_read: impl FnMut(
        &Scope,
        LegacyDraftPhase,
    ) -> Result<HostIdentity, LegacyPrDraftFailure>,
) -> Result<LegacyPrDraftReady, LegacyPrDraftFailure> {
    let OpenReadArguments {
        mut intent,
        provider,
        database,
        cancel,
        deadline,
        witness,
    } = args;
    if deadline.saturating_duration_since(Instant::now()) > MAX_OPEN_DURATION {
        return Err(LegacyPrDraftFailure::InvalidOriginal);
    }
    let target = PrWriteTarget::new(&intent.identity, &intent.target)
        .map_err(|_| LegacyPrDraftFailure::InvalidOriginal)?;
    let scope = target.scope().clone();
    if provider.host() != scope.host || scope.host != intent.identity.host.to_ascii_lowercase() {
        return Err(LegacyPrDraftFailure::InvalidOriginal);
    }
    intent.identity = identity_read(&scope, LegacyDraftPhase::BeforePrivateRead)?;
    let read = (|| {
        if cancel.is_cancelled() {
            return Err(LegacyPrDraftFailure::Cancelled(
                LegacyDraftPhase::BeforePrivateRead,
            ));
        }
        if Instant::now() >= deadline {
            return Err(LegacyPrDraftFailure::Deadline(
                LegacyDraftPhase::BeforePrivateRead,
            ));
        }
        let mut form = Form::pr(intent).map_err(|_| LegacyPrDraftFailure::InvalidPrivateDraft)?;
        if form.fields.len() != LEGACY_PR_DRAFT_FIELDS
            || form
                .fields
                .iter()
                .any(|field| field.secret || field.key != "body")
        {
            return Err(LegacyPrDraftFailure::ConstructorContractChanged);
        }
        let snapshot = Cache::legacy_pr_draft_snapshot_file_before_v1(
            database,
            &scope,
            &form.resource(),
            &target,
            LEGACY_PR_DRAFT_SERIALIZED_BYTES,
            deadline,
        )
        .map_err(|_| LegacyPrDraftFailure::PrivateStorage)?;
        if snapshot.unresolved {
            return Err(LegacyPrDraftFailure::UnresolvedOriginal);
        }
        if let Some(body) = snapshot.body {
            let header = draft_header(&body)?;
            if snapshot.original_invalid {
                return Err(LegacyPrDraftFailure::InvalidOriginalPublication);
            }
            let FormPurpose::Pr(original) = &form.purpose else {
                return Err(LegacyPrDraftFailure::ConstructorContractChanged);
            };
            header.validate(original.kind, witness, snapshot.original.as_ref())?;
            restore_bound_draft(&mut form, &body)?;
        }
        form.invalidate_plan();
        form.preview = false;
        form.preview_requested = false;
        form.submit_requested = false;
        form.submitted = false;
        form.submitting = false;
        form.editor_requested = false;
        Ok(form)
    })();
    // Closing numeric observation is mandatory even after local failure. Final
    // account/auth/deadline refusal suppresses both form and private diagnostic.
    let identity = identity_read(&scope, LegacyDraftPhase::BeforePresentation)?;
    let mut form = read?;
    let FormPurpose::Pr(original) = &mut form.purpose else {
        return Err(LegacyPrDraftFailure::ConstructorContractChanged);
    };
    original.identity = identity.clone();
    Ok(LegacyPrDraftReady { form, identity })
}
pub(crate) fn open_legacy_pr_draft_workflow_v1(
    intent: PrIntent,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    workflow: &mut LegacyPrDraftWorkflowBudget,
    witness: Option<&LegacyPrDraftWitness>,
) -> Result<LegacyPrDraftReady, LegacyPrDraftFailure> {
    let deadline = workflow.deadline();
    open_with_identity_reader_v1(
        OpenReadArguments {
            intent,
            provider,
            database,
            cancel,
            deadline,
            witness,
        },
        |scope, phase| {
            if cancel.is_cancelled() {
                return Err(LegacyPrDraftFailure::Cancelled(phase));
            }
            if Instant::now() >= deadline {
                return Err(LegacyPrDraftFailure::Deadline(phase));
            }
            let slot = match phase {
                LegacyDraftPhase::BeforePrivateRead => LegacyPrIdentityPhase::BeforePrivateRead,
                LegacyDraftPhase::BeforePresentation => LegacyPrIdentityPhase::BeforePresentation,
            };
            let result =
                provider.legacy_pr_draft_identity_budgeted_v1(scope, workflow, slot, cancel);
            if cancel.is_cancelled() {
                return Err(LegacyPrDraftFailure::Cancelled(phase));
            }
            if Instant::now() >= deadline {
                return Err(LegacyPrDraftFailure::Deadline(phase));
            }
            result.map_err(|failure| LegacyPrDraftFailure::Identity {
                phase,
                problem: LegacyDraftIdentityProblem::Provider(failure.kind),
            })
        },
    )
}
