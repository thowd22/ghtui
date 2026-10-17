//! Closed fixed workflow state invocation on the existing prepaid cursor.
use super::*;
use crate::providers::gh::ScopedActionsWorkflowV2;
impl PostWriteCaptureAllowanceV2 {
    pub(crate) fn scoped_workflow_empty_ack_v6(
        &mut self,
        provider: &GithubProvider,
        original: &Arc<ScopedActionsWorkflowV2>,
        enabled: bool,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if !original.original_writer_v6(provider) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual original workflow issuer required",
            ));
        }
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let (deadline, cap) = bounded
            .scoped_read_window
            .expect("same original prepaid slot");
        let bounded = bounded.with_scoped_read_window(deadline, cap.min(16 * 1024))?;
        let resource = format!(
            "{}/repository-id/{}/actions/workflow/{}",
            provider.host,
            original.repository().id(),
            original.model().id
        );
        let result = (|| {
            let bytes = bounded.execute(
                vec![
                    "api".into(),
                    format!(
                        "repos/{}/actions/workflows/{}/{}",
                        original.repository().name(),
                        original.model().id,
                        if enabled { "enable" } else { "disable" }
                    ),
                    "--hostname".into(),
                    provider.host.clone(),
                    "--method".into(),
                    "PUT".into(),
                    "--include".into(),
                ],
                None,
                Some(resource.clone()),
                &self.cancellation,
            )?;
            self.check_issuer(provider, cancel)?;
            super::empty_http_ack_v5::validate(&bytes, "204")?;
            self.check_issuer(provider, cancel)
        })();
        if result.is_err() {
            provider.runner.mark_ambiguous(&resource);
        }
        result
    }
    pub(crate) fn scoped_workflow_observation_v6(
        &mut self,
        provider: &GithubProvider,
        original: &Arc<ScopedActionsWorkflowV2>,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<ScopedActionsWorkflowV2>> {
        let raw = self.api(
            provider,
            &format!(
                "repos/{}/actions/workflows/{}",
                original.repository().name(),
                original.model().id
            ),
            "GET",
            None,
            None,
            cancel,
        )?;
        let result = original.bind_raw_v6(provider, &raw, false)?;
        self.check_issuer(provider, cancel)?;
        Ok(result)
    }
}
