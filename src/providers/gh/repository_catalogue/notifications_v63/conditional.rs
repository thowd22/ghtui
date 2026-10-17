//! Closed conditional GET request. No arbitrary argv/spec/URL/write constructor.
use super::poll::NotificationValidatorsV63;
use super::*;
use crate::providers::runner::{CommandKind, FailureKind};

pub(crate) struct NotificationConditionalRequestV63 {
    runner: CommandRunner,
    original: CancellationToken,
    spec: CommandSpec,
}
impl std::fmt::Debug for NotificationConditionalRequestV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(
            "NotificationConditionalRequestV63(<closed original GET; argv/validators redacted>)",
        )
    }
}
impl NotificationConditionalRequestV63 {
    pub(in crate::providers::gh::repository_catalogue) fn page(
        provider: &GithubProvider,
        query: &NotificationQueryV63,
        page: usize,
        validators: &NotificationValidatorsV63,
        cancel: &CancellationToken,
        original_deadline: Instant,
        capture_cap: usize,
    ) -> GithubResult<Self> {
        query.validate()?;
        if cancel.is_cancelled()
            || Instant::now() >= original_deadline
            || !(1..=PAGE_CAP_V63).contains(&capture_cap)
        {
            return Err(bad_v63(GithubErrorKind::Transport));
        }
        // Revalidate borrowed cached metadata BEFORE materializing any header.
        let poll = current_access_v1::notifications_http_v63::NotificationPollHeadersV63 {
            last_modified: validators.last_modified.as_deref(),
            etag: validators.etag.as_deref(),
            link: None,
            poll_interval: None,
            retry_after: None,
        };
        poll::metadata(&poll, 0)?;
        let mut args = vec![
            "api".into(),
            query.endpoint(page)?,
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "GET".into(),
            "--include".into(),
        ];
        if let Some(value) = &validators.last_modified {
            args.extend(["--header".into(), format!("If-Modified-Since: {value}")]);
        }
        if let Some(value) = &validators.etag {
            args.extend(["--header".into(), format!("If-None-Match: {value}")]);
        }
        let mut spec = provider.spec(args);
        let deadline = spec
            .absolute_deadline_v1
            .map_or(original_deadline, |d| d.min(original_deadline));
        spec.absolute_deadline_v1 = Some(deadline);
        spec.output_limit = spec.output_limit.min(capture_cap);
        spec.timeout = spec
            .timeout
            .min(deadline.saturating_duration_since(Instant::now()));
        spec.original_read_cancellation = Some(cancel.clone());
        if spec
            .scoped_read_budget
            .as_ref()
            .is_some_and(|b| !b.original_token_matches_v63(cancel))
        {
            return Err(bad_v63(GithubErrorKind::InvalidTarget));
        }
        if spec.timeout.is_zero() || spec.output_limit == 0 {
            return Err(bad_v63(GithubErrorKind::Transport));
        }
        Ok(Self {
            runner: provider.runner.clone(),
            original: cancel.clone(),
            spec,
        })
    }
    /// Restricted to the provider boundary; never exported to App/UI or a
    /// generic raw-command endpoint. Request cannot be cloned or retargeted.
    pub(in crate::providers) fn runner_spec_v63<'a>(
        &'a self,
        runner: &CommandRunner,
        cancel: &CancellationToken,
    ) -> Result<&'a CommandSpec, CommandFailure> {
        if !self.runner.same_instance(runner)
            || !self.original.same_instance(cancel)
            || self
                .spec
                .original_read_cancellation
                .as_ref()
                .is_none_or(|t| !t.same_instance(cancel))
            || self
                .spec
                .scoped_read_budget
                .as_ref()
                .is_some_and(|b| !b.original_token_matches_v63(cancel))
            || !matches!(self.spec.kind, CommandKind::Read)
            || self.spec.coalesce_read
            || !self.spec.stdin.is_empty()
        {
            return Err(refused(
                FailureKind::ReconciliationRequired,
                "Conditional read original issuer/token/closed GET was not retained",
            ));
        }
        if cancel.is_cancelled() {
            return Err(refused(
                FailureKind::Cancelled,
                "Original conditional read cancelled",
            ));
        }
        if self
            .spec
            .absolute_deadline_v1
            .is_none_or(|d| Instant::now() >= d)
            || self.spec.timeout.is_zero()
        {
            return Err(refused(
                FailureKind::Timeout,
                "Original conditional read window expired",
            ));
        }
        Ok(&self.spec)
    }
}
fn refused(kind: FailureKind, message: &str) -> CommandFailure {
    CommandFailure {
        kind,
        message: message.into(),
        exit_code: None,
        stdout: Vec::new(),
        stderr: Vec::new(),
        outcome_unknown: false,
    }
}
