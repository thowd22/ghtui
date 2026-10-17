//! Original review postwrite visibility, using the SAME actual parent parser
//! and already-charged ordinary capture cursor, never another access session.
use super::*;
use crate::providers::gh::repository_catalogue::PostWriteCaptureAllowanceV2;
use std::time::Instant;

#[derive(Clone, Copy)]
enum PostwriteParentV32<'a> {
    Existing(&'a PrResourceParentV1),
    Base(&'a BaseTransitionV32),
}
impl PostwriteParentV32<'_> {
    fn parent(&self) -> &PrResourceParentV1 {
        match self {
            Self::Existing(parent) => parent,
            Self::Base(transition) => transition.parent(),
        }
    }
    fn parse(
        &self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        raw: &Value,
    ) -> GithubResult<PrResourceParentV1> {
        let actual = match self {
            Self::Existing(original) => parent(provider, current, original.summary(), raw)?,
            Self::Base(transition) => {
                provider.parse_confirmed_base_parent_v32(current, transition, raw)?
            }
        };
        if !immutable_images::original_parent(self.parent(), &actual) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        Ok(actual)
    }
}
pub(in crate::providers::gh) struct MetadataMembersPostOpeningV48 {
    parent: PrResourceParentV1,
    repository: Value,
    raw: Value,
}
pub(in crate::providers::gh) struct MetadataMembersPostDataV48 {
    repository: Value,
    raw: Value,
}
impl MetadataMembersPostOpeningV48 {
    pub(in crate::providers::gh) fn into_member_parts_v48(
        self,
    ) -> (PrResourceParentV1, MetadataMembersPostDataV48) {
        (
            self.parent,
            MetadataMembersPostDataV48 {
                repository: self.repository,
                raw: self.raw,
            },
        )
    }
}
impl MetadataMembersPostDataV48 {
    pub(in crate::providers::gh) fn repository_raw(&self) -> &Value {
        &self.repository
    }
    pub(in crate::providers::gh) fn raw(&self) -> &Value {
        &self.raw
    }
}
impl GithubProvider {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn close_review_postwrite_visibility_v2(
        &self,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        original: &PrResourceParentV1,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
        local: impl FnOnce() -> GithubResult<()>,
    ) -> GithubResult<()> {
        self.open_review_postwrite_visibility_v19(
            identity, current, original, captures, cancel, deadline,
        )?;
        postwrite_clock_v19(captures, cancel, deadline)?;
        local()?;
        self.finish_review_postwrite_visibility_v19(identity, captures, cancel, deadline)
    }
    // Private shared mechanics only. No transport getter, source/lease mint or
    // added child: the old Review ABI still performs exactly UID/repo/PR/UID.
    // Conversation's typed V19 producer inserts its paid detail phase between
    // this opening bracket and the SAME closing UID, never after final proof.
    pub(in crate::providers::gh) fn open_review_postwrite_visibility_v19(
        &self,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        original: &PrResourceParentV1,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<PrResourceParentV1> {
        self.open_postwrite_parent_raw_v48(
            identity,
            current,
            PostwriteParentV32::Existing(original),
            captures,
            cancel,
            deadline,
        )
        .map(|opening| opening.parent)
    }
    pub(in crate::providers::gh) fn open_base_postwrite_visibility_v32(
        &self,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        transition: &BaseTransitionV32,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<PrResourceParentV1> {
        self.open_postwrite_parent_raw_v48(
            identity,
            current,
            PostwriteParentV32::Base(transition),
            captures,
            cancel,
            deadline,
        )
        .map(|opening| opening.parent)
    }
    pub(in crate::providers::gh) fn open_members_postwrite_visibility_v48(
        &self,
        receipt: &PrMetadataMembersReceiptV48,
        current: &CurrentCatalogueRepository,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<MetadataMembersPostOpeningV48> {
        self.open_postwrite_parent_raw_v48(
            receipt.identity(),
            current,
            PostwriteParentV32::Existing(receipt.parent()),
            captures,
            cancel,
            deadline.min(receipt.deadline()),
        )
    }
    pub(in crate::providers::gh) fn open_members_metadata_visibility_v62(
        &self,
        request: MembersVisibilityV48<'_>,
        current: &CurrentCatalogueRepository,
        captures: &mut PostWriteCaptureAllowanceV2,
        heap: &mut metadata_context_v51::MetadataMembersHeapV62,
    ) -> GithubResult<MetadataMembersPostOpeningV48> {
        let MembersVisibilityV48 {
            actual,
            plan,
            receipt,
            cancel,
            deadline,
        } = request;
        postwrite_clock_v19(captures, cancel, deadline.min(receipt.deadline()))?;
        if self.scoped_read_budget.is_some()
            || !actual.metadata_tab_v62()
            || !receipt.validates_refresh(actual, plan, cancel)
            || receipt.identity().host != self.host
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let user = captures.members_metadata_role_v62(
            self,
            actual,
            MembersMetadataRoleV62::VisibilityUser,
            heap,
            cancel,
        )?;
        if user["id"].as_u64() != receipt.identity().account_id {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let original = receipt.parent();
        let repository = captures.members_metadata_role_v62(
            self,
            actual,
            MembersMetadataRoleV62::VisibilityRepository,
            heap,
            cancel,
        )?;
        if current.repository().id != original.repository_id()
            || current.repository().node_id.as_deref() != Some(original.repository_node())
            || repository["id"].as_u64() != Some(original.repository_id())
            || repository["node_id"].as_str() != Some(original.repository_node())
            || repository["full_name"]
                .as_str()
                .is_none_or(|name| !name.eq_ignore_ascii_case(original.repository()))
            || repository["permissions"]["pull"].as_bool() != Some(true)
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let raw = captures.members_metadata_role_v62(
            self,
            actual,
            MembersMetadataRoleV62::VisibilityParent,
            heap,
            cancel,
        )?;
        let parent = PostwriteParentV32::Existing(original).parse(self, current, &raw)?;
        postwrite_clock_v19(captures, cancel, deadline.min(receipt.deadline()))?;
        Ok(MetadataMembersPostOpeningV48 {
            parent,
            repository,
            raw,
        })
    }
    fn open_postwrite_parent_raw_v48(
        &self,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        expected: PostwriteParentV32<'_>,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<MetadataMembersPostOpeningV48> {
        postwrite_clock_v19(captures, cancel, deadline)?;
        if self.scoped_read_budget.is_some()
            || identity.host != self.host
            || identity
                .account_id
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        // Uniform prepaid slots preserve one cursor even when an earlier
        // comments page proved completion before its maximum page count.
        let opening = captures.api(self, "user", "GET", None, None, cancel)?;
        if opening["id"].as_u64() != identity.account_id {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let original = expected.parent();
        let name = original.repository();
        let raw = captures.api(self, &format!("repos/{name}"), "GET", None, None, cancel)?;
        if current.repository().id != original.repository_id()
            || current.repository().node_id.as_deref() != Some(original.repository_node())
            || raw["id"].as_u64() != Some(original.repository_id())
            || raw["node_id"].as_str() != Some(original.repository_node())
            || raw["full_name"]
                .as_str()
                .is_none_or(|actual| !actual.eq_ignore_ascii_case(name))
            || raw["permissions"]["pull"].as_bool() != Some(true)
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let repository_raw = raw;
        let raw = captures.api(
            self,
            &format!("repos/{name}/pulls/{}", original.number()),
            "GET",
            None,
            None,
            cancel,
        )?;
        let actual = expected.parse(self, current, &raw)?;
        postwrite_clock_v19(captures, cancel, deadline)?;
        Ok(MetadataMembersPostOpeningV48 {
            parent: actual,
            repository: repository_raw,
            raw,
        })
    }
    pub(in crate::providers::gh) fn finish_review_postwrite_visibility_v19(
        &self,
        identity: &HostIdentity,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        postwrite_clock_v19(captures, cancel, deadline)?;
        if self.scoped_read_budget.is_some()
            || identity.host != self.host
            || identity
                .account_id
                .is_none_or(|id| id == 0 || id > i64::MAX as u64)
        {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        let closing = captures.api(self, "user", "GET", None, None, cancel)?;
        if closing["id"].as_u64() != identity.account_id {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        postwrite_clock_v19(captures, cancel, deadline)?;
        Ok(())
    }
    pub(in crate::providers::gh) fn conversation_closing_summary_v19(
        &self,
        current: &CurrentCatalogueRepository,
        original: &PrResourceParentV1,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        self.closing_postwrite_summary_v32(
            current,
            PostwriteParentV32::Existing(original),
            captures,
            cancel,
        )
    }
    pub(in crate::providers::gh) fn base_closing_summary_v32(
        &self,
        current: &CurrentCatalogueRepository,
        transition: &BaseTransitionV32,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        self.closing_postwrite_summary_v32(
            current,
            PostwriteParentV32::Base(transition),
            captures,
            cancel,
        )
    }
    fn closing_postwrite_summary_v32(
        &self,
        current: &CurrentCatalogueRepository,
        expected: PostwriteParentV32<'_>,
        captures: &mut PostWriteCaptureAllowanceV2,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        // Narrow THIS raw request even for the uniform reaction cursor. The
        // whole reserved slot remains charged; no cursor skip/reset/refund.
        let bounded = self
            .clone()
            .with_scoped_read_window(captures.deadline(), 128 * 1024)?;
        let original = expected.parent();
        let raw = captures.api(
            &bounded,
            &format!(
                "repos/{}/pulls/{}",
                original.repository(),
                original.number()
            ),
            "GET",
            None,
            None,
            cancel,
        )?;
        let actual = expected.parse(self, current, &raw)?;
        ConversationRawSummaryV19::from_actual_raw_v19(self, actual, &raw)
    }
}
fn postwrite_clock_v19(
    captures: &PostWriteCaptureAllowanceV2,
    cancel: &CancellationToken,
    deadline: Instant,
) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline.min(captures.deadline()) {
        Err(safe(GithubErrorKind::Transport))
    } else {
        Ok(())
    }
}
