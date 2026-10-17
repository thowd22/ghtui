//! Purpose-closed raw parent for the same original Core member phase. The raw
//! response never leaves the provider family and is never a source constructor.
use super::*;
use std::sync::Arc;

pub(in crate::providers::gh) struct MetadataMembersParentV48 {
    parent: Arc<PrResourceParentV1>,
    raw: Value,
}
impl MetadataMembersParentV48 {
    pub(in crate::providers::gh) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(in crate::providers::gh) fn raw(&self) -> &Value {
        &self.raw
    }
}
impl GithubProvider {
    pub(in crate::providers::gh) fn metadata_members_bounded_parent_v62(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        heap: &mut super::metadata_context_v51::MetadataMembersHeapV62,
        cancel: &CancellationToken,
    ) -> GithubResult<MetadataMembersParentV48> {
        if current.repository().permissions.pull != Some(true)
            || intent.identity().host != self.host
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let mut spec = self.spec(vec![
            "api".into(),
            format!(
                "repos/{}/pulls/{}",
                current.repository().full_name,
                intent.number()
            ),
            "--hostname".into(),
            self.host.clone(),
            "--method".into(),
            "GET".into(),
        ]);
        spec.output_limit = spec.output_limit.min(128 * 1024);
        let bytes = self
            .runner
            .run(spec, cancel)
            .map_err(|failure| safe(GithubError::from(failure).kind))?
            .stdout;
        let raw = heap.object(&bytes)?;
        let parent = parent_expected_v13(
            self,
            current,
            ExpectedPrParentV13::PersonalIntent(intent),
            &raw,
        )?;
        Ok(MetadataMembersParentV48 {
            parent: Arc::new(parent),
            raw,
        })
    }
    pub(in crate::providers::gh) fn metadata_members_post_parent_v48(
        &self,
        current: &CurrentCatalogueRepository,
        original: &PrResourceParentV1,
        raw: &Value,
    ) -> GithubResult<PrResourceParentV1> {
        let fresh = parent(self, current, original.summary(), raw)?;
        if !same_parent(original, &fresh) {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        Ok(fresh)
    }
    pub(in crate::providers::gh) fn metadata_members_parent_v48(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<MetadataMembersParentV48> {
        let (parent, raw) = self.original_detail_parent_raw_v19(current, intent, cancel)?;
        Ok(MetadataMembersParentV48 {
            parent: Arc::new(parent),
            raw,
        })
    }
}
