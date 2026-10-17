use super::state_source_heap_v80::StateSourceCaptureV80;
use super::*;
use crate::providers::gh::actions::{
    WorkflowJsonBoxedValueV9, WorkflowJsonCaptureV9, WorkflowJsonRawV9, WorkflowJsonRoleV9,
};
enum CoreCaptureV80<'a> {
    Workflow(&'a WorkflowJsonCaptureV9),
    StateSource(&'a StateSourceCaptureV80),
}

pub(super) enum CoreRawV9 {
    Standard(Vec<u8>),
    Workflow(WorkflowJsonRawV9),
}
impl std::ops::Deref for CoreRawV9 {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Standard(raw) => raw,
            Self::Workflow(raw) => raw,
        }
    }
}
pub(super) enum CoreValueV9 {
    Standard(Value),
    Workflow(WorkflowJsonBoxedValueV9),
}
impl std::ops::Deref for CoreValueV9 {
    type Target = Value;
    fn deref(&self) -> &Value {
        match self {
            Self::Standard(value) => value,
            Self::Workflow(value) => value.value(),
        }
    }
}

pub(super) struct CatalogueBudget {
    limits: RepositoryCatalogueLimits,
    started: Instant,
    /// None for discovery; current-access sessions bind their ORIGINAL token.
    pub(super) original_cancellation: Option<CancellationToken>,
    pub observed_from: u64,
    pub accounting: RepositoryCatalogueAccounting,
    pub(super) workflow_v9: Option<Arc<WorkflowCoreJsonV9>>,
}
impl CatalogueBudget {
    pub fn new(limits: &RepositoryCatalogueLimits) -> GithubResult<Self> {
        Ok(Self {
            limits: limits.clone(),
            started: Instant::now(),
            original_cancellation: None,
            observed_from: utc_now()?,
            accounting: RepositoryCatalogueAccounting::default(),
            workflow_v9: None,
        })
    }
    pub fn remaining_bytes(&self) -> usize {
        self.limits
            .max_response_bytes
            .saturating_sub(self.accounting.charged_bytes())
    }
    pub fn deadline(&self) -> Instant {
        let ordinary = self
            .started
            .checked_add(self.limits.max_duration)
            .expect("validated bounded catalogue duration");
        self.workflow_v9
            .as_ref()
            .map_or(ordinary, |policy| ordinary.min(policy.deadline()))
    }
    pub fn check(&self, cancel: &CancellationToken) -> GithubResult<()> {
        if let Some(policy) = &self.workflow_v9 {
            policy.check(cancel)?;
        }
        if cancel.is_cancelled()
            || self
                .original_cancellation
                .as_ref()
                .is_some_and(CancellationToken::is_cancelled)
            || self.started.elapsed() >= self.limits.max_duration
        {
            Err(error(GithubErrorKind::Transport,"Original catalogue deadline/cancellation expired; no lease, reset or automatic resend"))
        } else {
            Ok(())
        }
    }
    pub fn command(
        &mut self,
        provider: &GithubProvider,
        args: Vec<String>,
        response_cap: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<Vec<u8>> {
        self.command_inner_v9(provider, args, response_cap, cancel, None)
    }
    fn command_inner_v9(
        &mut self,
        provider: &GithubProvider,
        args: Vec<String>,
        response_cap: usize,
        cancel: &CancellationToken,
        capture: Option<CoreCaptureV80<'_>>,
    ) -> GithubResult<Vec<u8>> {
        self.check(cancel)?;
        let cap = self.remaining_bytes().min(response_cap);
        if cap == 0 {
            return Err(error(
                GithubErrorKind::Transport,
                "Original catalogue whole capture budget exhausted before another gh command",
            ));
        }
        let mut spec = provider.spec(args);
        spec.original_read_cancellation = self.original_cancellation.clone();
        let cap = cap.min(spec.output_limit);
        spec.output_limit = cap;
        spec.timeout = spec.timeout.min(provider.timeout).min(
            self.limits
                .max_duration
                .saturating_sub(self.started.elapsed()),
        );
        if cap == 0 || spec.timeout.is_zero() {
            return Err(error(
                GithubErrorKind::Transport,
                "Original read window exhausted before another catalogue child",
            ));
        }
        if let Some(capture) = capture {
            match capture {
                CoreCaptureV80::Workflow(capture) => capture.check_spec_v9(&mut spec)?,
                CoreCaptureV80::StateSource(capture) => {
                    capture.check_provider(provider)?;
                    capture.check_spec(&mut spec)?;
                }
            }
        }
        self.accounting.requests += 1;
        let output = match provider.runner.run(spec, cancel) {
            Ok(output) => output,
            Err(failure) => {
                self.accounting.failed_reserved_bytes = self
                    .accounting
                    .failed_reserved_bytes
                    .checked_add(cap)
                    .ok_or_else(|| {
                        error(
                            GithubErrorKind::Transport,
                            "Catalogue failed capture reservation overflow",
                        )
                    })?;
                return Err(failure.into());
            }
        };
        self.accounting.stdout_bytes = self
            .accounting
            .stdout_bytes
            .checked_add(output.capture.stdout_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Catalogue stdout accounting overflow",
                )
            })?;
        self.accounting.diagnostic_bytes = self
            .accounting
            .diagnostic_bytes
            .checked_add(output.capture.stderr_bytes())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::Transport,
                    "Catalogue diagnostic accounting overflow",
                )
            })?;
        if output.capture.stdout_bytes() != output.stdout.len()
            || output.capture.total_bytes() > cap
            || self.accounting.charged_bytes() > self.limits.max_response_bytes
        {
            return Err(error(
                GithubErrorKind::Transport,
                "Catalogue raw shared capture counters disagree; no invented byte coverage",
            ));
        }
        self.check(cancel)?;
        Ok(output.stdout)
    }
    pub(super) fn principal_version_v9(
        &mut self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<CoreRawV9> {
        let Some(policy) = &self.workflow_v9 else {
            return self
                .command(provider, vec!["--version".into()], 4096, cancel)
                .map(CoreRawV9::Standard);
        };
        let capture = policy.capture(provider, WorkflowJsonRoleV9::CoreVersion, cancel)?;
        let raw = self.command_inner_v9(
            provider,
            vec!["--version".into()],
            4096,
            cancel,
            Some(CoreCaptureV80::Workflow(&capture)),
        )?;
        capture.own_raw_v9(raw).map(CoreRawV9::Workflow)
    }
    pub(super) fn principal_user_v9(
        &mut self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<CoreValueV9> {
        self.core_json_v9(
            provider,
            "user",
            64 * 1024,
            WorkflowJsonRoleV9::CoreUser,
            cancel,
        )
    }
    pub(super) fn original_repository_v9(
        &mut self,
        provider: &GithubProvider,
        reference: &crate::repository_catalogue_intent_v1::IntentReference,
        cap: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<CoreValueV9> {
        self.core_json_v9(
            provider,
            &format!("repos/{}", reference.last_name),
            cap,
            WorkflowJsonRoleV9::CoreRepository,
            cancel,
        )
    }
    fn core_json_v9(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        cap: usize,
        role: WorkflowJsonRoleV9,
        cancel: &CancellationToken,
    ) -> GithubResult<CoreValueV9> {
        let Some(policy) = &self.workflow_v9 else {
            return self
                .json(provider, endpoint, cap, cancel)
                .map(CoreValueV9::Standard);
        };
        let capture = policy.capture(provider, role, cancel)?;
        let boxed = capture.precharge_box_v9()?;
        let raw = self.command_inner_v9(
            provider,
            vec![
                "api".into(),
                endpoint.into(),
                "--hostname".into(),
                provider.host.clone(),
                "--method".into(),
                "GET".into(),
            ],
            cap.min(capture.cap_v9()),
            cancel,
            Some(CoreCaptureV80::Workflow(&capture)),
        )?;
        let length = raw.len();
        capture
            .decode_owned_v9(raw, 0..length)
            .and_then(|value| boxed.wrap_v9(value))
            .map(CoreValueV9::Workflow)
    }
    pub(super) fn state_source_command_v80(
        &mut self,
        provider: &GithubProvider,
        args: Vec<String>,
        cancel: &CancellationToken,
        capture: &StateSourceCaptureV80,
    ) -> GithubResult<Vec<u8>> {
        self.command_inner_v9(
            provider,
            args,
            capture.cap(),
            cancel,
            Some(CoreCaptureV80::StateSource(capture)),
        )
    }
    pub fn json(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        cap: usize,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let raw = self.command(
            provider,
            vec![
                "api".into(),
                endpoint.into(),
                "--hostname".into(),
                provider.host.clone(),
                "--method".into(),
                "GET".into(),
            ],
            cap,
            cancel,
        )?;
        serde_json::from_slice(&raw).map_err(|_| {
            error(
                GithubErrorKind::Json,
                "Catalogue endpoint did not return bounded valid JSON",
            )
        })
    }
}

#[cfg(test)]
#[path = "read_limits_deadline_v1_tests.rs"]
mod read_limits_deadline_v1_tests;
