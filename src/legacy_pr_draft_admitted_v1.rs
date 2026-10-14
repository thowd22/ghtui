//! SOURCE ONLY combined legacy OPEN: access5 + isolated identity/local4 +
//! final access5, one captured monotonic deadline. No native registration.
use crate::{
    forms::{Form, FormPurpose, PrIntent},
    legacy_pr_draft_binding_v1::LegacyPrDraftWitness,
    legacy_pr_draft_open_v1::{open_legacy_pr_draft_bound_before_v1, LegacyPrDraftFailure},
    providers::{
        gh::{
            GithubErrorKind, GithubProvider, HostIdentity, LegacyPrDraftAccess,
            LegacyPrDraftAccessAccounting, LegacyPrDraftAccessLimits, LegacyPrDraftReadWindow,
            Repository,
        },
        runner::CancellationToken,
    },
};
use std::{fmt, path::Path};

#[derive(Debug)]
pub enum LegacyAdmittedDraftFailure {
    Access(GithubErrorKind),
    Local(LegacyPrDraftFailure),
}
impl fmt::Display for LegacyAdmittedDraftFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Access(_)=>f.write_str("Current original account/repository/PR access unavailable; private draft and local diagnostics hidden"),
            Self::Local(failure)=>write!(f,"{failure}"),
        }
    }
}
impl std::error::Error for LegacyAdmittedDraftFailure {}
/// Opaque worker-only completion; caller still rejects stale worker generation,
/// expired final admission or failed original journal/visibility guards. Not a
/// persisted form approval, receipt, reusable permission or SQL write permit.
pub struct LegacyPrDraftAdmitted {
    form: Form,
    identity: HostIdentity,
    admission: LegacyPrDraftAccess,
    initial_accounting: LegacyPrDraftAccessAccounting,
}
impl fmt::Debug for LegacyPrDraftAdmitted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LegacyPrDraftAdmitted(<private guarded local form>)")
    }
}
impl LegacyPrDraftAdmitted {
    pub fn initial_access_accounting(&self) -> &LegacyPrDraftAccessAccounting {
        &self.initial_accounting
    }
    pub fn final_admission(&self) -> &LegacyPrDraftAccess {
        &self.admission
    }
    pub fn into_parts(self) -> (Form, HostIdentity, LegacyPrDraftAccess) {
        (self.form, self.identity, self.admission)
    }
}
pub fn open_admitted_legacy_pr_draft_v1(
    mut intent: PrIntent,
    original_repository: Option<&Repository>,
    provider: &GithubProvider,
    database: &Path,
    limits: &LegacyPrDraftAccessLimits,
    window: &LegacyPrDraftReadWindow,
    cancel: &CancellationToken,
) -> Result<LegacyPrDraftAdmitted, LegacyAdmittedDraftFailure> {
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
        .legacy_pr_draft_access_v1(&intent.identity, &intent.target, limits, window, cancel)
        .map_err(|error| LegacyAdmittedDraftFailure::Access(error.kind))?;
    if original_repository.is_some_and(|repo| repo.id != initial.repository().id) {
        // Optional expectation is a genuine earlier provider repository record,
        // not inferred from PrSummary. Its cached permissions grant nothing.
        return Err(LegacyAdmittedDraftFailure::Access(
            GithubErrorKind::IdentityChanged,
        ));
    }
    initial
        .validate_for(provider, &intent.identity, &intent.target, window, cancel)
        .map_err(|error| LegacyAdmittedDraftFailure::Access(error.kind))?;
    intent.identity = initial.identity().clone();
    let original = intent.target.clone();
    let witness = LegacyPrDraftWitness::issue(
        &initial,
        provider,
        &intent.identity,
        &intent.target,
        intent.kind,
        window,
        cancel,
    )
    .map_err(LegacyAdmittedDraftFailure::Local)?;
    let local = open_legacy_pr_draft_bound_before_v1(
        intent,
        provider,
        database,
        cancel,
        window.deadline(),
        Some(&witness),
    );
    let expected = local
        .as_ref()
        .map_or(initial.identity(), |ready| ready.identity());
    // Mandatory AFTER local corruption/storage failures where the original
    // budget allows. No private form/error escapes a failed final access check.
    let final_access = provider
        .revalidate_legacy_pr_draft_access_v1(&initial, expected, window, cancel)
        .map_err(|error| LegacyAdmittedDraftFailure::Access(error.kind))?;
    final_access
        .validate_for(provider, final_access.identity(), &original, window, cancel)
        .map_err(|error| LegacyAdmittedDraftFailure::Access(error.kind))?;
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
    Ok(LegacyPrDraftAdmitted {
        form,
        identity,
        admission: final_access,
        initial_accounting: *initial.accounting(),
    })
}

#[cfg(test)]
#[path = "legacy_pr_draft_admitted_v1_tests.rs"]
mod tests;
