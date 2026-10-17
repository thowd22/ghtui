//! Exact State-only prepared GraphQL bytes; SAME ordinary one-take cursor,
//! gh argv/environment/minI/error parser. No arbitrary raw request API.
use super::*;
use crate::providers::gh::pr_resources_v1::PreparedStateRequestV68;
impl PostWriteCaptureAllowanceV2 {
    pub(in crate::providers::gh) fn state_request_v68(
        &mut self,
        provider: &GithubProvider,
        request: PreparedStateRequestV68,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        request.validate_transport_v68(provider, cancel)?;
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let (deadline, cap) = bounded.scoped_read_window.ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Actual State prepaid window absent",
            )
        })?;
        let bounded = bounded.with_scoped_read_window(deadline.min(request.deadline()), cap)?;
        let (bytes, resource) = request.into_transport();
        let resource = bounded.canonical_resource(&resource);
        let mut spec = bounded.spec(vec![
            "api".into(),
            "graphql".into(),
            "--hostname".into(),
            bounded.host.clone(),
            "--method".into(),
            "POST".into(),
            "--input".into(),
            "-".into(),
        ]);
        spec.stdin = bytes;
        spec = spec.write(resource.clone());
        let result = bounded
            .runner
            .run(spec, &self.cancellation)
            .map(|o| o.stdout)
            .map_err(GithubError::from);
        self.check_issuer(provider, cancel)?;
        bounded.parse_api_response_v48(result?, Some(resource))
    }
}
