//! Genuine Notification target → SAME private raw-parent/Files parser. No
//! generic arbitrary repository/DTO constructor or synthetic Summary is exposed.
use super::*;
use parent_intent_v13::NotificationParentExpectedV63;

fn observed_parent(
    provider: &GithubProvider,
    current: &CurrentCatalogueRepository,
    target: &NotificationPrFollowIntentV63,
    raw: &Value,
) -> GithubResult<PrResourceParentV1> {
    if target.host() != provider.host
        || target.reference().id != current.repository().id
        || target.reference().node_id.as_deref() != current.repository().node_id.as_deref()
        || !target
            .reference()
            .last_name
            .eq_ignore_ascii_case(&current.repository().full_name)
        || current.repository().permissions.pull != Some(true)
        || current
            .repository()
            .node_id
            .as_deref()
            .is_none_or(|node| !opaque(node))
        || !raw.is_object()
        || raw.get("errors").is_some()
        || raw.get("message").is_some()
        || raw["node_id"].as_str().is_none_or(|node| !opaque(node))
        || raw["head"]["sha"].as_str().is_none_or(|sha| !full_sha(sha))
        || raw["base"]["sha"].as_str().is_none_or(|sha| !full_sha(sha))
    {
        return Err(safe(GithubErrorKind::IdentityChanged));
    }
    // Each required scalar is checked by the unchanged parser as well. The
    // borrowed expectation deliberately does not invent H/M, a node or author
    // from the notification URL; those come from this actual raw response.
    parent_expected_v13(
        provider,
        current,
        ExpectedPrParentV13::NotificationObservedV63(&NotificationParentExpectedV63 {
            host: target.host(),
            number: target.number(),
            raw,
        }),
        raw,
    )
}
impl GithubProvider {
    /// Initial actual parent, bounded Files and ONE existing raw-patch child.
    /// The caller declares P+5 measured children and prepares its actual rows
    /// BEFORE the separate closing-parent/thread/repo/SQL/final UID.
    pub(crate) fn notification_pr_files_v63(
        &self,
        current: &CurrentCatalogueRepository,
        identity: &HostIdentity,
        target: &NotificationPrFollowIntentV63,
        limits: &PrResourceReadLimitsV1,
        token: &CancellationToken,
        after_opening: impl FnOnce(&PrResourceParentV1) -> GithubResult<()>,
    ) -> GithubResult<PrResourceReadV1> {
        limits.validate()?;
        if limits.max_pages_per_source > 4
            || limits.max_pool_bytes > 8 * 1024 * 1024
            || identity.host != target.host()
            || identity.account_id != target.identity().account_id
        {
            return Err(safe(GithubErrorKind::InvalidTarget));
        }
        let raw = document(
            self,
            &format!(
                "repos/{}/pulls/{}",
                current.repository().full_name,
                target.number()
            ),
            limits.max_document_bytes,
            token,
            false,
        )?;
        let parent = observed_parent(self, current, target, &raw)?;
        after_opening(&parent)?;
        checked_finished(self, token)?;
        let (files, file_coverage) = collect_files(self, &parent, limits, token)?;
        checked_finished(self, token)?;
        let (patch, patch_failure) = match raw_patch(
            self,
            parent.summary(),
            limits.max_patch_bytes,
            limits.max_document_bytes,
            token,
        ) {
            Ok(bytes) => (Some(bytes), None),
            Err(failure) => (None, Some(failure.kind)),
        };
        checked_finished(self, token)?;
        Ok(PrResourceReadV1 {
            kind: PrResourceKindV1::Files,
            parent,
            files,
            file_coverage,
            checks: Vec::new(),
            statuses: Vec::new(),
            check_coverage: Default::default(),
            status_coverage: Default::default(),
            patch,
            patch_failure,
        })
    }
    pub(crate) fn notification_pr_closing_parent_v63(
        &self,
        current: &CurrentCatalogueRepository,
        target: &NotificationPrFollowIntentV63,
        read: &PrResourceReadV1,
        limits: &PrResourceReadLimitsV1,
        token: &CancellationToken,
    ) -> GithubResult<()> {
        limits.validate()?;
        let raw = document(
            self,
            &format!(
                "repos/{}/pulls/{}",
                current.repository().full_name,
                target.number()
            ),
            limits.max_document_bytes,
            token,
            true,
        )?;
        let last = observed_parent(self, current, target, &raw)?;
        if read.kind() != PrResourceKindV1::Files || !same_parent(read.parent(), &last) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        checked_finished(self, token)
    }
}
