//! Actual postwrite documents under one prepaid allowance. The genuine
//! prewrite row supplies original expectations, never current access alone.
use super::*;
impl GithubProvider {
    pub(crate) fn pr_thread_postwrite_parent_v2(
        &self,
        current: &CurrentCatalogueRepository,
        original: &PrResourceParentV1,
        capture: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let name = original.repository();
        let raw = capture.api(self, &format!("repos/{name}"), "GET", None, None, cancel)?;
        if current.repository().id != original.repository_id()
            || current.repository().node_id.as_deref() != Some(original.repository_node())
            || raw["id"].as_u64() != Some(original.repository_id())
            || raw["node_id"].as_str() != Some(original.repository_node())
            || raw["full_name"]
                .as_str()
                .is_none_or(|value| !value.eq_ignore_ascii_case(name))
            || raw["permissions"]["pull"].as_bool() != Some(true)
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let raw = capture.api(
            self,
            &format!("repos/{name}/pulls/{}", original.number()),
            "GET",
            None,
            None,
            cancel,
        )?;
        // SAME genuine raw parent parser: URLs, lifecycle, base/head refs,
        // original fork repo IDs/nodes and all immutable comparisons retained.
        let actual = parent(self, current, original.summary(), &raw)?;
        if !immutable_images::original_parent(original, &actual) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        Ok(())
    }
}
