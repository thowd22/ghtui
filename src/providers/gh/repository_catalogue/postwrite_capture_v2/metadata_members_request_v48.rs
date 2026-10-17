//! Purpose-closed preencoded request transport. Only a genuine live Members
//! observation/Plan can issue these bytes BEFORE the final Core principal.
use super::*;
use crate::providers::gh::PreparedMembersRequestV48;

impl PostWriteCaptureAllowanceV2 {
    pub(in crate::providers::gh) fn metadata_members_request_v48(
        &mut self,
        provider: &GithubProvider,
        request: PreparedMembersRequestV48,
        metadata_heap_v62: Option<&mut crate::providers::gh::pr_resources_v1::metadata_context_v51::MetadataMembersHeapV62>,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        request.validate_transport_v48(provider, cancel)?;
        if request.metadata_profile_v62() != metadata_heap_v62.is_some() {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Exact Members wire/heap profile differs",
            ));
        }
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let (deadline, cap) = bounded.scoped_read_window.ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Actual member prepaid window missing",
            )
        })?;
        let bounded = bounded.with_scoped_read_window(deadline.min(request.deadline_v48()), cap)?;
        let (bytes, resource) = request.into_transport_v48();
        let resource = resource.map(|value| bounded.canonical_resource(&value));
        let args = vec![
            "api".into(),
            "graphql".into(),
            "--hostname".into(),
            bounded.host.clone(),
            "--method".into(),
            "POST".into(),
            "--input".into(),
            "-".into(),
        ];
        let mut spec = bounded.spec(args);
        spec.stdin = bytes;
        if let Some(resource) = &resource {
            spec = spec.write(resource.clone());
        }
        let data = bounded
            .runner
            .run(spec, &self.cancellation)
            .map(|output| output.stdout)
            .map_err(GithubError::from);
        self.check_issuer(provider, cancel)?;
        match metadata_heap_v62 {
            Some(heap) => heap.object(&data?),
            None => bounded.parse_api_response_v48(data?, resource),
        }
    }
}
