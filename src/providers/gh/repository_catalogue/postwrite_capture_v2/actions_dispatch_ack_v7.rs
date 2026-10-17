//! Closed native Dispatch POST/receipt, SAME prepaid cursor and HTTP grammar.
use super::*;
use crate::providers::gh::{ScopedActionsRunTargetV1, ScopedDispatchSchemaV7};
impl PostWriteCaptureAllowanceV2 {
    pub(crate) fn scoped_dispatch_post_v7(
        &mut self,
        provider: &GithubProvider,
        schema: &Arc<ScopedDispatchSchemaV7>,
        payload: &Arc<[u8]>,
        cancel: &CancellationToken,
    ) -> GithubResult<Option<u64>> {
        if !schema.producing_provider_v7(provider) || payload.len() > 65_535 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual bounded original Dispatch payload required",
            ));
        }
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let (deadline, cap) = bounded.scoped_read_window.expect("original prepaid slot");
        let bounded = bounded.with_scoped_read_window(
            deadline,
            cap.min(if schema.protocol().detailed() {
                64 * 1024
            } else {
                16 * 1024
            }),
        )?;
        let resource = format!(
            "{}/repository-id/{}/actions/workflow/{}",
            provider.host,
            schema.original().repository().id(),
            schema.original().model().id
        );
        let mut spec = bounded.spec(vec![
            "api".into(),
            format!(
                "repos/{}/actions/workflows/{}/dispatches",
                schema.repository(),
                schema.original().model().id
            ),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "POST".into(),
            "--header".into(),
            format!("X-GitHub-Api-Version: {}", schema.protocol().version()),
            "--include".into(),
            "--input".into(),
            "-".into(),
        ]);
        // Fixed exact in-memory payload, not an argv/field interpolation or raw
        // arbitrary transport getter. Already bounded BEFORE this one copy.
        spec.stdin = payload.to_vec();
        spec = spec.write(bounded.canonical_resource(&resource));
        let result = (|| {
            let bytes = bounded
                .runner
                .run(spec, &self.cancellation)
                .map_err(|_| {
                    error(
                        GithubErrorKind::AmbiguousWrite,
                        "Dispatch request outcome unavailable; private diagnostics hidden",
                    )
                })?
                .stdout;
            self.check_issuer(provider, cancel)?;
            let id = if schema.protocol().detailed() {
                let response = pinned_http_response_v5::response(
                    &bytes,
                    pinned_http_response_v5::ProfileV5::ActionsDispatchCreated200,
                )?;
                let value: Value = serde_json::from_str(response.body).map_err(|_| {
                    error(
                        GithubErrorKind::AmbiguousWrite,
                        "Dispatch HTTP200 does not contain bounded valid JSON",
                    )
                })?;
                let id = value
                    .get("workflow_run_id")
                    .and_then(Value::as_u64)
                    .filter(|id| *id > 0 && *id <= i64::MAX as u64)
                    .ok_or_else(|| {
                        error(
                            GithubErrorKind::AmbiguousWrite,
                            "Dispatch HTTP200 omitted its actual positive workflow run ID",
                        )
                    })?;
                Some(id)
            } else {
                empty_http_ack_v5::validate(&bytes, "204")?;
                None
            };
            self.check_issuer(provider, cancel)?;
            Ok(id)
        })();
        if result.is_err() {
            provider.runner.mark_ambiguous(&resource);
        }
        result
    }
    pub(crate) fn scoped_dispatch_run_v7(
        &mut self,
        provider: &GithubProvider,
        schema: &Arc<ScopedDispatchSchemaV7>,
        id: u64,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<ScopedActionsRunTargetV1>> {
        if !schema.producing_provider_v7(provider) || !schema.protocol().detailed() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual detailed Dispatch run observation required",
            ));
        }
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let (deadline, cap) = bounded.scoped_read_window.expect("original prepaid slot");
        let bounded = bounded.with_scoped_read_window(deadline, cap.min(512 * 1024))?;
        let bytes = bounded
            .execute(
                vec![
                    "api".into(),
                    format!("repos/{}/actions/runs/{id}", schema.repository()),
                    "--hostname".into(),
                    provider.host.clone(),
                    "--method".into(),
                    "GET".into(),
                    "--header".into(),
                    format!("X-GitHub-Api-Version: {}", schema.protocol().version()),
                ],
                None,
                None,
                &self.cancellation,
            )
            .map_err(|_| {
                error(
                    GithubErrorKind::AmbiguousWrite,
                    "Actual Dispatch run receipt unavailable; no retry",
                )
            })?;
        self.check_issuer(provider, cancel)?;
        let raw: Value = serde_json::from_slice(&bytes).map_err(|_| {
            error(
                GithubErrorKind::AmbiguousWrite,
                "Actual Dispatch run receipt JSON unavailable",
            )
        })?;
        let result = ScopedActionsRunTargetV1::from_dispatch_raw_v7(provider, schema, id, &raw)?;
        self.check_issuer(provider, cancel)?;
        Ok(result)
    }
}
