//! Original review postwrite visibility, using the SAME actual parent parser
//! and already-charged ordinary capture cursor, never another access session.
use super::*;
use crate::providers::gh::repository_catalogue::PostWriteCaptureAllowanceV2;
use std::time::Instant;

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
        let check = || {
            if cancel.is_cancelled() || Instant::now() >= deadline.min(captures.deadline()) {
                Err(safe(GithubErrorKind::Transport))
            } else {
                Ok(())
            }
        };
        check()?;
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
        let raw = captures.api(
            self,
            &format!("repos/{name}/pulls/{}", original.number()),
            "GET",
            None,
            None,
            cancel,
        )?;
        let actual = parent(self, current, original.summary(), &raw)?;
        if !immutable_images::original_parent(original, &actual) {
            return Err(safe(GithubErrorKind::StaleHead));
        }
        if cancel.is_cancelled() || Instant::now() >= deadline.min(captures.deadline()) {
            return Err(safe(GithubErrorKind::Transport));
        }
        local()?;
        if cancel.is_cancelled() || Instant::now() >= deadline.min(captures.deadline()) {
            return Err(safe(GithubErrorKind::Transport));
        }
        let closing = captures.api(self, "user", "GET", None, None, cancel)?;
        if closing["id"].as_u64() != identity.account_id {
            return Err(safe(GithubErrorKind::IdentityChanged));
        }
        if cancel.is_cancelled() || Instant::now() >= deadline.min(captures.deadline()) {
            return Err(safe(GithubErrorKind::Transport));
        }
        Ok(())
    }
}
