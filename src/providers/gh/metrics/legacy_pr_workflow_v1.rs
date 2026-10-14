//! SOURCE ONLY: one owned scheduling Window/capture ledger, not current access,
//! authentication, historical original, permission, approval or a write permit.
use super::legacy_pr_draft_access_v1::{LegacyPrDraftAccessLimits, LegacyPrDraftReadWindow};
use super::*;
use crate::storage::Scope;
use std::{ffi::OsString, fmt, time::Instant};

pub const LEGACY_IDENTITY_VERSION_CAPTURE: usize = 4096;
pub const LEGACY_IDENTITY_USER_CAPTURE: usize = 128 * 1024;
pub const LEGACY_WORKFLOW_CAPTURE_MAX: usize = 2 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyPrIdentityPhase {
    BeforePrivateRead,
    BeforePresentation,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LegacyPrWorkflowAccounting {
    requests: usize,
    stdout: usize,
    diagnostics: usize,
    failed: usize,
}
impl LegacyPrWorkflowAccounting {
    pub fn requests(&self) -> usize {
        self.requests
    }
    pub fn successful_stdout_bytes(&self) -> usize {
        self.stdout
    }
    pub fn successful_diagnostic_bytes(&self) -> usize {
        self.diagnostics
    }
    pub fn failed_reserved_bytes(&self) -> usize {
        self.failed
    }
    pub fn charged_bytes(&self) -> usize {
        self.stdout
            .saturating_add(self.diagnostics)
            .saturating_add(self.failed)
    }
    pub(super) fn snapshot(budget: &MetricsBudget) -> Self {
        Self {
            requests: budget.requests,
            stdout: budget.bytes,
            diagnostics: budget.successful_diagnostic_bytes,
            failed: budget.failed_reserved_bytes,
        }
    }
}
pub struct LegacyPrDraftWorkflowBudget {
    window: LegacyPrDraftReadWindow,
    budget: MetricsBudget,
    scope: Scope,
    producer: CommandRunner,
    program: OsString,
    environment: BTreeMap<OsString, OsString>,
    access_limits: LegacyPrDraftAccessLimits,
    entered: [bool; 4],
    succeeded: [bool; 4],
    last: Option<usize>,
}
impl fmt::Debug for LegacyPrDraftWorkflowBudget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LegacyPrDraftWorkflowBudget(<one owned clock/capture ledger; not authority>)")
    }
}
impl LegacyPrDraftWorkflowBudget {
    pub fn new(
        provider: &GithubProvider,
        identity: &HostIdentity,
        limits: LegacyPrDraftAccessLimits,
        window: LegacyPrDraftReadWindow,
        max_capture_bytes: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        let selected = limits.selected(&window, cancel)?;
        let scope = Scope::github(&identity.host, identity.account_id).map_err(|_| {
            error(
                GithubErrorKind::InvalidTarget,
                "Whole legacy workflow requires positive numeric original scope",
            )
        })?;
        let worst = limits
            .max_document_bytes
            .checked_mul(10)
            .and_then(|bytes| {
                bytes.checked_add(
                    2 * LEGACY_IDENTITY_VERSION_CAPTURE + 2 * LEGACY_IDENTITY_USER_CAPTURE,
                )
            })
            .ok_or_else(|| {
                error(
                    GithubErrorKind::InvalidTarget,
                    "Whole legacy capture allocations overflow",
                )
            })?;
        if provider.host != scope.host
            || identity.host != scope.host
            || max_capture_bytes > LEGACY_WORKFLOW_CAPTURE_MAX
            || max_capture_bytes < worst
        {
            return Err(error(GithubErrorKind::InvalidTarget,"Whole legacy capture budget must fit all fourteen allocations and stay within two MiB"));
        }
        let mut selected = selected;
        selected.max_response_bytes = max_capture_bytes;
        selected.max_duration = window.remaining(cancel)?;
        let spec = provider.spec(vec![]);
        Ok(Self {
            window,
            budget: MetricsBudget::new(&selected)?,
            scope,
            producer: provider.runner.clone(),
            program: spec.program,
            environment: spec.env,
            access_limits: limits,
            entered: [false; 4],
            succeeded: [false; 4],
            last: None,
        })
    }
    pub fn window(&self) -> &LegacyPrDraftReadWindow {
        &self.window
    }
    pub fn deadline(&self) -> Instant {
        self.window.deadline()
    }
    pub fn accounting(&self) -> LegacyPrWorkflowAccounting {
        LegacyPrWorkflowAccounting::snapshot(&self.budget)
    }
    pub(super) fn limits(&self) -> LegacyPrDraftAccessLimits {
        self.access_limits
    }
    pub(super) fn assert_producer(
        &self,
        provider: &GithubProvider,
        scope: &Scope,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.window.remaining(cancel)?;
        let spec = provider.spec(vec![]);
        if scope != &self.scope
            || provider.host != self.scope.host
            || !provider.runner.same_instance(&self.producer)
            || spec.program != self.program
            || spec.env != self.environment
        {
            return Err(error(GithubErrorKind::IdentityChanged,"Whole legacy ledger cannot move to another scope/host/producer/program/environment"));
        }
        Ok(())
    }
    pub(super) fn enter(
        &mut self,
        phase: usize,
        provider: &GithubProvider,
        scope: &Scope,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.assert_producer(provider, scope, cancel)?;
        if phase >= 4
            || self.entered[phase]
            || self.last.is_some_and(|last| phase <= last)
            || phase > 0 && !self.succeeded[0]
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Whole legacy phase cannot repeat/reorder or bypass initial current access",
            ));
        }
        self.entered[phase] = true;
        self.last = Some(phase);
        Ok(())
    }
    pub(super) fn finish(&mut self, phase: usize, succeeded: bool) {
        self.succeeded[phase] = succeeded;
    }
    pub(super) fn parts(&mut self) -> (&LegacyPrDraftReadWindow, &mut MetricsBudget) {
        (&self.window, &mut self.budget)
    }
    pub fn verify_complete(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<LegacyPrWorkflowAccounting> {
        let scope = Scope::github(&identity.host, identity.account_id).map_err(|_| {
            error(
                GithubErrorKind::IdentityChanged,
                "Final whole workflow numeric scope unavailable",
            )
        })?;
        self.assert_producer(provider, &scope, cancel)?;
        let accounting = self.accounting();
        if !self.succeeded.iter().all(|success| *success)
            || accounting.requests != 14
            || accounting.failed != 0
        {
            return Err(error(
                GithubErrorKind::Transport,
                "Whole legacy workflow is incomplete; capture ledger does not authorize a form",
            ));
        }
        Ok(accounting)
    }
}
impl GithubProvider {
    pub fn legacy_pr_draft_identity_budgeted_v1(
        &self,
        scope: &Scope,
        workflow: &mut LegacyPrDraftWorkflowBudget,
        phase: LegacyPrIdentityPhase,
        cancel: &CancellationToken,
    ) -> GithubResult<HostIdentity> {
        let slot = match phase {
            LegacyPrIdentityPhase::BeforePrivateRead => 1,
            LegacyPrIdentityPhase::BeforePresentation => 2,
        };
        workflow.enter(slot, self, scope, cancel)?;
        let result = (|| {
            let (window, budget) = workflow.parts();
            let provider = self
                .clone()
                .with_timeout(self.timeout.min(window.remaining(cancel)? / 2));
            let output = budget.capture_gh_read_v1(
                &provider,
                vec!["--version".into()],
                LEGACY_IDENTITY_VERSION_CAPTURE,
                cancel,
            )?;
            window.remaining(cancel)?;
            // Intentionally identical legacy parsing/compatibility. Strict CLI
            // grammar/floors are a separate handoff, not admitted silently here.
            let version = String::from_utf8_lossy(&output.stdout)
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned();
            if !version.starts_with("gh version 2.") {
                return Err(error(
                    GithubErrorKind::Unsupported,
                    "Unsupported current gh CLI version; private composer hidden",
                ));
            }
            let provider = self
                .clone()
                .with_timeout(self.timeout.min(window.remaining(cancel)? / 2));
            let output = budget.capture_gh_read_v1(
                &provider,
                vec![
                    "api".into(),
                    "user".into(),
                    "--hostname".into(),
                    self.host.clone(),
                    "--method".into(),
                    "GET".into(),
                ],
                LEGACY_IDENTITY_USER_CAPTURE,
                cancel,
            )?;
            window.remaining(cancel)?;
            let user: Value = if output.stdout.is_empty() {
                // Existing api() treats an empty successful body as Null.
                // Keep its missing-login Authentication classification here.
                Value::Null
            } else {
                serde_json::from_slice(&output.stdout).map_err(|_| {
                    error(
                        GithubErrorKind::Json,
                        "Current user JSON unavailable; private composer hidden",
                    )
                })?
            };
            if user
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|errors| !errors.is_empty())
            {
                return Err(error(
                    GithubErrorKind::Unsupported,
                    "Current host user response reports API errors",
                ));
            }
            let login = text(&user, "login").to_ascii_lowercase();
            if login.is_empty() {
                return Err(error(
                    GithubErrorKind::Authentication,
                    "Current gh account unavailable; authenticate with gh for this host",
                ));
            }
            let identity = HostIdentity {
                host: self.host.clone(),
                login,
                account_id: user.get("id").and_then(Value::as_u64).filter(|id| *id > 0),
                gh_version: version,
            };
            let actual = Scope::github(&identity.host, identity.account_id).map_err(|_| {
                error(
                    GithubErrorKind::IdentityChanged,
                    "Current positive numeric principal unavailable",
                )
            })?;
            if &actual != scope {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Current positive numeric principal differs; private composer hidden",
                ));
            }
            Ok(identity)
        })();
        workflow.finish(slot, result.is_ok());
        result
    }
}

#[cfg(test)]
#[path = "legacy_pr_workflow_v1_tests.rs"]
mod tests;
