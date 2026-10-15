//! SOURCE-ONLY exact selected-PR resources. Reuses existing gh runner and the
//! owning original catalogue measured window; no auth/client/write stack.
use super::*;
use crate::providers::gh::search::canonical_source_url_matches;
mod model;
mod paging;
mod parse;
#[path = "pr_resources_v1/pr_checks_binding_v1.rs"]
mod pr_checks_binding_v1;
pub use model::*;
#[path = "pr_immutable_images_v1.rs"]
mod immutable_images;
pub use immutable_images::*;
use paging::*;
use parse::*;

fn safe(kind: GithubErrorKind) -> GithubError {
    error(
        kind,
        "Selected PR resource source unavailable; private data not substituted",
    )
}
fn full_sha(sha: &str) -> bool {
    matches!(sha.len(), 40 | 64) && sha.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn opaque(node: &str) -> bool {
    !node.is_empty() && node.len() <= 1024 && !node.chars().any(char::is_control)
}
fn checked(
    provider: &GithubProvider,
    cancel: &CancellationToken,
    cap: usize,
    hold: usize,
) -> GithubResult<()> {
    let budget = provider
        .scoped_read_budget
        .as_ref()
        .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?;
    budget.check_clock(cancel).map_err(GithubError::from)?;
    let state = budget.accounting().map_err(GithubError::from)?;
    let required = cap
        .checked_add(hold)
        .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?;
    if state.sealed()
        || state.in_flight_requests() != 0
        || state
            .charged_bytes()
            .checked_add(required)
            .is_none_or(|needed| needed > state.capacity_bytes())
        || state
            .requests_claimed()
            .checked_add(if hold == 0 { 1 } else { 2 })
            .is_none_or(|needed| needed > state.max_requests())
    {
        return Err(safe(GithubErrorKind::Transport));
    }
    Ok(())
}
fn document(
    provider: &GithubProvider,
    endpoint: &str,
    cap: usize,
    cancel: &CancellationToken,
    closing: bool,
) -> GithubResult<Value> {
    checked(provider, cancel, cap, if closing { 0 } else { cap })?;
    // Same ordinary API GET argv as existing api(), but preflight raw UTF8 bytes
    // BEFORE JSON parse. Captures/failures are charged by the attached runner.
    let mut spec = provider.spec(vec![
        "api".into(),
        endpoint.into(),
        "--hostname".into(),
        provider.host.clone(),
        "--method".into(),
        "GET".into(),
    ]);
    spec.output_limit = spec.output_limit.min(cap);
    let bytes = provider
        .runner
        .run(spec, cancel)
        .map_err(|failure| safe(GithubError::from(failure).kind))?
        .stdout;
    if bytes.len() > cap {
        return Err(safe(GithubErrorKind::Json));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| safe(GithubErrorKind::Json))?;
    if !value.is_object() && !value.is_array() {
        return Err(safe(GithubErrorKind::Json));
    }
    Ok(value)
}
fn raw_patch(
    provider: &GithubProvider,
    original: &PrSummary,
    cap: usize,
    closing_cap: usize,
    cancel: &CancellationToken,
) -> GithubResult<Vec<u8>> {
    checked(provider, cancel, cap, closing_cap)?;
    // Exactly existing pr_diff argv, with no lossy String conversion, escape
    // flag retry, API fallback, hidden mutable ref or local checkout.
    let mut spec = provider.spec(vec![
        "pr".into(),
        "diff".into(),
        original.pr.number.to_string(),
        "--repo".into(),
        format!("{}/{}", provider.host, original.pr.repository),
        "--color".into(),
        "never".into(),
    ]);
    spec.output_limit = spec.output_limit.min(cap);
    provider
        .runner
        .run(spec, cancel)
        .map(|output| output.stdout)
        .map_err(|failure| safe(GithubError::from(failure).kind))
}
impl GithubProvider {
    /// Fresh raw parent in the caller's SAME measured original-source READ.
    /// This intermediate receipt never grants permission to publish a comment.
    pub(crate) fn pr_inline_original_parent_v1(
        &self,
        current: &CurrentCatalogueRepository,
        original: &PrResourceReadV1,
        limits: &PrResourceReadLimitsV1,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        limits.validate()?;
        if original.kind != PrResourceKindV1::Files {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        let endpoint = format!(
            "repos/{}/pulls/{}",
            current.repository().full_name,
            original.parent.number()
        );
        let value = document(self, &endpoint, limits.max_document_bytes, cancel, true)?;
        let fresh = parent(self, current, &original.parent.summary, &value)?;
        if !immutable_images::original_parent(&original.parent, &fresh) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        Ok(fresh)
    }
    /// Actual raw parent before/after, explicitly bounded resource pages. Fresh
    /// principal/repo admission is owned by the SAME original session, not this
    /// intermediate value; caller MUST close it with original local guards.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pr_resource_read_v1(
        &self,
        current: &CurrentCatalogueRepository,
        identity: &HostIdentity,
        original: &PrSummary,
        kind: PrResourceKindV1,
        limits: &PrResourceReadLimitsV1,
        cancel: &CancellationToken,
        after_opening: impl FnOnce(&PrResourceParentV1) -> GithubResult<()>,
    ) -> GithubResult<PrResourceReadV1> {
        limits.validate()?;
        if identity.host != self.host
            || identity
                .account_id
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
            || original.pr.host != self.host
            || original.pr.number == 0
            || !full_sha(&original.head_sha)
            || !full_sha(&original.base_sha)
            || !opaque(&original.pr.id)
            || current.repository().permissions.pull != Some(true)
            || current.repository().id == 0
            || current
                .repository()
                .node_id
                .as_deref()
                .is_none_or(|node| !opaque(node))
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        let endpoint = format!(
            "repos/{}/pulls/{}",
            current.repository().full_name,
            original.pr.number
        );
        let opening = parent(
            self,
            current,
            original,
            &document(self, &endpoint, limits.max_document_bytes, cancel, false)?,
        )?;
        checked(
            self,
            cancel,
            limits.max_document_bytes,
            limits.max_document_bytes,
        )?;
        after_opening(&opening).map_err(|failure| safe(failure.kind))?;
        checked(
            self,
            cancel,
            limits.max_document_bytes,
            limits.max_document_bytes,
        )?;
        let mut observed = PrResourceReadV1 {
            kind,
            parent: opening,
            files: vec![],
            checks: vec![],
            statuses: vec![],
            file_coverage: Default::default(),
            check_coverage: Default::default(),
            status_coverage: Default::default(),
            patch: None,
            patch_failure: None,
        };
        match kind {
            PrResourceKindV1::Files => {
                let (files, coverage) = collect_files(self, &observed.parent, limits, cancel)?;
                observed.files = files;
                observed.file_coverage = coverage;
                match raw_patch(
                    self,
                    observed.parent.summary(),
                    limits.max_patch_bytes,
                    limits.max_document_bytes,
                    cancel,
                ) {
                    Ok(bytes) => observed.patch = Some(bytes),
                    Err(failure) => observed.patch_failure = Some(failure.kind),
                }
            }
            PrResourceKindV1::Checks => {
                let (checks, coverage) =
                    collect_checks(self, &observed.parent, false, limits, cancel)?;
                observed.checks = checks;
                observed.check_coverage = coverage;
                let (statuses, coverage) =
                    collect_checks(self, &observed.parent, true, limits, cancel)?;
                observed.statuses = statuses;
                observed.status_coverage = coverage;
            }
        }
        let closing = match kind {
            PrResourceKindV1::Files => {
                self.pr_inline_original_parent_v1(current, &observed, limits, cancel)?
            }
            PrResourceKindV1::Checks => parent(
                self,
                current,
                original,
                &document(self, &endpoint, limits.max_document_bytes, cancel, true)?,
            )?,
        };
        if !same_parent(&observed.parent, &closing) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        observed.parent = closing;
        checked_finished(self, cancel)?;
        Ok(observed)
    }
}
fn checked_finished(provider: &GithubProvider, cancel: &CancellationToken) -> GithubResult<()> {
    provider
        .scoped_read_budget
        .as_ref()
        .ok_or_else(|| safe(GithubErrorKind::InvalidTarget))?
        .check_clock(cancel)
        .map_err(Into::into)
}
