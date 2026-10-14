//! SOURCE ONLY, undeclared worker-only private draft OPEN. No source loading,
//! publication, retry, acknowledgement or access inference. Bounded historical
//! original metadata may bind a saved reference, never grant current access.
use crate::{
    editor::MAX_TEXT_BYTES,
    forms::{Form, FormPurpose, PrIntent},
    legacy_pr_draft_binding_v1::{draft_header, restore_bound_draft, LegacyPrDraftWitness},
    pr_write_journal::PrWriteTarget,
    providers::{
        gh::{GithubErrorKind, GithubProvider, HostIdentity},
        runner::CancellationToken,
    },
    storage::{Cache, Scope},
};
use std::{
    fmt,
    path::Path,
    time::{Duration, Instant},
};

/// Current all-seven Form::pr constructors each have exactly one body editor.
/// Six bytes per input byte is a conservative JSON escape bound. Includes TWO
/// genuine repository facts (base/current and optional head source), explicit
/// host253/repo4096/node1024/ref4096 bounds, numeric IDs and the existing
/// original-reference node4096/fullhead64/SHA25664 envelope. No metadata cap is
/// loosened. If constructors/witness fields change, rederive before restoring.
pub const LEGACY_PR_DRAFT_FIELDS: usize = 1;
pub const LEGACY_PR_DRAFT_SERIALIZED_BYTES: usize = 6
    * (LEGACY_PR_DRAFT_FIELDS * MAX_TEXT_BYTES
        + 2 * (253 + 4096 + 1024)
        + 253
        + 28
        + 1024
        + 2 * 4096
        + 2 * 64
        + 32
        + 4096
        + 64
        + 64)
    + 4096;
const MAX_OPEN_DURATION: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyDraftPhase {
    BeforePrivateRead,
    BeforePresentation,
}
#[derive(Debug)]
pub enum LegacyDraftIdentityProblem {
    Provider(GithubErrorKind),
    MissingPositivePrincipal,
    DifferentPrincipal,
}
#[derive(Debug)]
pub enum LegacyPrDraftFailure {
    InvalidOriginal,
    Identity {
        phase: LegacyDraftPhase,
        problem: LegacyDraftIdentityProblem,
    },
    Cancelled(LegacyDraftPhase),
    Deadline(LegacyDraftPhase),
    PrivateStorage,
    InvalidPrivateDraft,
    ConstructorContractChanged,
    UnresolvedOriginal,
    UnboundOriginal,
    DifferentOriginalBinding,
    InvalidOriginalPublication,
}
impl fmt::Display for LegacyPrDraftFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // No provider/raw SQLite/serde diagnostic can echo a private draft.
        f.write_str(match self {
            Self::InvalidOriginal => "Original positive canonical PR scope is unavailable; no private draft read",
            Self::Identity { problem: LegacyDraftIdentityProblem::Provider(GithubErrorKind::MissingGh), .. } => "gh is unavailable; private composer hidden, local Git remains usable",
            Self::Identity { problem: LegacyDraftIdentityProblem::Provider(GithubErrorKind::Authentication), .. } => "Current gh authentication is unavailable; private composer hidden; use gh auth login for this host",
            Self::Identity { .. } => "Current positive numeric account could not be verified; private draft and diagnostics remain hidden",
            Self::Cancelled(_) => "Draft opening cancelled; no private prepared form or publication returned",
            Self::Deadline(_) => "Draft opening deadline exhausted; no private prepared form returned",
            Self::PrivateStorage => "Exact private draft storage is unavailable; no saved text or approval restored",
            Self::InvalidPrivateDraft => "Saved draft is corrupt or exceeds a supported field; no fresh-intent fallback or approval restored",
            Self::ConstructorContractChanged => "Legacy form layout changed; review its serialized draft budget before restoring",
            Self::UnresolvedOriginal => "Unresolved original PR publication; private draft retained, no composer or resend approval returned. Inspect and acknowledge explicitly before a new write.",
            Self::UnboundOriginal => "Saved draft has no verified original target binding; unchanged bytes quarantined. No form, adoption, fresh-intent fallback or resend returned.",
            Self::DifferentOriginalBinding => "Saved draft belongs to a different original canonical target or source; unchanged bytes retained, no form or automatic rebase returned.",
            Self::InvalidOriginalPublication => "Saved original reference does not resolve the exact command and canonical effect; unchanged bytes retained, no form or resend returned.",
        })
    }
}
impl std::error::Error for LegacyPrDraftFailure {}

/// Opaque worker completion, not a reusable authentication/permission lease.
/// Return only after final fresh numeric identity. This identity-only helper is
/// NOT repository/parent access authority. BEFORE calling it or any private SQL,
/// Root must prove current actual canonical repository/PR access independently,
/// retain that proof's freshness/generation guards and bracket it again before
/// admission. A cached summary/scope/original receipt is not that proof.
pub struct LegacyPrDraftReady {
    form: Form,
    identity: HostIdentity,
}
impl fmt::Debug for LegacyPrDraftReady {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LegacyPrDraftReady(<guarded private local draft>)")
    }
}
impl LegacyPrDraftReady {
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn into_parts(self) -> (Form, HostIdentity) {
        (self.form, self.identity)
    }
}
fn current(
    provider: &GithubProvider,
    expected: &Scope,
    cancel: &CancellationToken,
    deadline: Instant,
    phase: LegacyDraftPhase,
) -> Result<HostIdentity, LegacyPrDraftFailure> {
    if cancel.is_cancelled() {
        return Err(LegacyPrDraftFailure::Cancelled(phase));
    }
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(LegacyPrDraftFailure::Deadline(phase));
    }
    // identity has two commands; bound both within the remaining phase budget.
    let identity = provider
        .clone()
        .with_timeout(remaining / 2)
        .identity(cancel)
        .map_err(|error| LegacyPrDraftFailure::Identity {
            phase,
            problem: LegacyDraftIdentityProblem::Provider(error.kind),
        })?;
    if cancel.is_cancelled() {
        return Err(LegacyPrDraftFailure::Cancelled(phase));
    }
    if Instant::now() >= deadline {
        return Err(LegacyPrDraftFailure::Deadline(phase));
    }
    let scope = Scope::github(&identity.host, identity.account_id).map_err(|_| {
        LegacyPrDraftFailure::Identity {
            phase,
            problem: LegacyDraftIdentityProblem::MissingPositivePrincipal,
        }
    })?;
    if &scope != expected {
        return Err(LegacyPrDraftFailure::Identity {
            phase,
            problem: LegacyDraftIdentityProblem::DifferentPrincipal,
        });
    }
    Ok(identity)
}
pub fn open_legacy_pr_draft_v1(
    intent: PrIntent,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
) -> Result<LegacyPrDraftReady, LegacyPrDraftFailure> {
    open_legacy_pr_draft_before_v1(
        intent,
        provider,
        database,
        cancel,
        Instant::now() + MAX_OPEN_DURATION,
    )
}
/// Shared absolute deadline variant. No phase starts a fresh thirty-second
/// clock; bounded read-only SQLite and both identities use this same cutoff.
pub fn open_legacy_pr_draft_before_v1(
    intent: PrIntent,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<LegacyPrDraftReady, LegacyPrDraftFailure> {
    open_legacy_pr_draft_bound_before_v1(intent, provider, database, cancel, deadline, None)
}
/// The witness is issued by the combined caller from access5, never restored
/// from JSON. Isolated identity4 cannot authorize a persistent v4 target.
pub(crate) fn open_legacy_pr_draft_bound_before_v1(
    mut intent: PrIntent,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    witness: Option<&LegacyPrDraftWitness>,
) -> Result<LegacyPrDraftReady, LegacyPrDraftFailure> {
    if deadline.saturating_duration_since(Instant::now()) > MAX_OPEN_DURATION {
        return Err(LegacyPrDraftFailure::InvalidOriginal);
    }
    let target = PrWriteTarget::new(&intent.identity, &intent.target)
        .map_err(|_| LegacyPrDraftFailure::InvalidOriginal)?;
    let scope = target.scope().clone();
    if provider.host() != scope.host || scope.host != intent.identity.host.to_ascii_lowercase() {
        return Err(LegacyPrDraftFailure::InvalidOriginal);
    }
    // NO Cache open, private SQL, Form construction or restoration before this.
    intent.identity = current(
        provider,
        &scope,
        cancel,
        deadline,
        LegacyDraftPhase::BeforePrivateRead,
    )?;
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
    // Recheck even when local read/restore failed. Drift/auth failure suppresses
    // both prepared private text AND any private local diagnostic.
    let identity = current(
        provider,
        &scope,
        cancel,
        deadline,
        LegacyDraftPhase::BeforePresentation,
    )?;
    let mut form = read?;
    let FormPurpose::Pr(original) = &mut form.purpose else {
        return Err(LegacyPrDraftFailure::ConstructorContractChanged);
    };
    original.identity = identity.clone();
    Ok(LegacyPrDraftReady { form, identity })
}

#[cfg(test)]
#[path = "legacy_pr_draft_open_v1_tests.rs"]
mod tests;
