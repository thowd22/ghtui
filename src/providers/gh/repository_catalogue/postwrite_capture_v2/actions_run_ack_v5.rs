//! Closed actual original-run transport. A receipt is NOT a source, owner,
//! write permit or proof of deletion/cancellation; current source closes first.
use super::*;
use crate::providers::gh::{RunMutation, ScopedActionsRunTargetV1};
pub(crate) struct ScopedRunHttpAckV5 {
    original: Arc<ScopedActionsRunTargetV1>,
    action: RunMutation,
    status: u16,
}
impl ScopedRunHttpAckV5 {
    pub(crate) fn original(&self) -> &Arc<ScopedActionsRunTargetV1> {
        &self.original
    }
    pub(crate) fn action(&self) -> RunMutation {
        self.action
    }
    pub(crate) fn status(&self) -> u16 {
        self.status
    }
}
impl PostWriteCaptureAllowanceV2 {
    /// One fixed post-delete repository GET on the SAME prepaid cursor. SAME
    /// canonical parser and actual original owner ID/node/kind; no DTO becomes
    /// a CurrentRepository/source or a deletion/permission receipt.
    pub(crate) fn scoped_deleted_run_repository_v5(
        &mut self,
        provider: &GithubProvider,
        original: &Arc<ScopedActionsRunTargetV1>,
        catalogue: &RepositoryCatalogueRead,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if !original.original_writer_v3(provider) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual original run issuer required",
            ));
        }
        let raw = self.api(
            provider,
            &format!("repos/{}", original.repository()),
            "GET",
            None,
            None,
            cancel,
        )?;
        let raw_owner = raw
            .get("owner")
            .filter(|value| value.is_object())
            .ok_or_else(|| {
                error(
                    GithubErrorKind::IdentityChanged,
                    "Current repository owner unavailable",
                )
            })?;
        let login = raw_owner
            .get("login")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty() && value.len() <= 100)
            .ok_or_else(|| {
                error(
                    GithubErrorKind::IdentityChanged,
                    "Current owner unavailable",
                )
            })?;
        let owner_source = match raw_owner.get("type").and_then(Value::as_str) {
            Some("Organization") => RepositoryCatalogueSource::Organization(login.into()),
            Some("User") => RepositoryCatalogueSource::PublicUser(login.into()),
            _ => {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Current owner kind unavailable",
                ))
            }
        };
        let owner = super::super::parse::owner(raw_owner, &owner_source, login)?;
        if catalogue.owner().is_some_and(|old| {
            old.id != owner.id
                || old
                    .node_id
                    .as_ref()
                    .is_some_and(|node| owner.node_id.as_ref() != Some(node))
                || old.kind != owner.kind
        }) {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original repository owner changed",
            ));
        }
        let context = super::super::parse::RepositoryParseContextV13::current_catalogue(
            catalogue.context(),
            &owner,
        );
        let (repository, _) =
            super::super::parse::repository_with_context_v13(&raw, &context, 64 * 1024, 256 * 1024)
                .map_err(|_| {
                    error(
                        GithubErrorKind::IdentityChanged,
                        "Current repository metadata unavailable/bounded",
                    )
                })?;
        if repository.id != original.repository_id()
            || repository.node_id.as_deref() != original.repository_node()
            || repository.full_name != original.repository()
            || repository.permissions.pull != Some(true)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Post-delete original repository identity/access changed",
            ));
        }
        self.check_issuer(provider, cancel)
    }
    /// SAME fixed original cursor and original deadline/token. Called only at
    /// the common source/controller guarded invocation; never arbitrary argv.
    pub(crate) fn scoped_run_empty_ack_v5(
        &mut self,
        provider: &GithubProvider,
        original: &Arc<ScopedActionsRunTargetV1>,
        action: RunMutation,
        cancel: &CancellationToken,
    ) -> GithubResult<ScopedRunHttpAckV5> {
        if !original.original_writer_v3(provider) {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Actual original run/runner required",
            ));
        }
        let base = format!(
            "repos/{}/actions/runs/{}",
            original.repository(),
            original.run().id
        );
        let (endpoint, method, status) = match action {
            RunMutation::ForceCancel => (format!("{base}/force-cancel"), "POST", "202"),
            RunMutation::DeleteRun => (base, "DELETE", "204"),
            RunMutation::DeleteLogs => (format!("{base}/logs"), "DELETE", "204"),
            _ => {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "No closed empty-ack adapter for this action",
                ))
            }
        };
        let bounded = self.spend_ordinary_slot_v2(provider, cancel)?;
        let (deadline, cap) = bounded
            .scoped_read_window
            .expect("actual fixed original slot");
        let bounded = bounded.with_scoped_read_window(deadline, cap.min(16 * 1024))?;
        let resource = format!(
            "{}/repository-id/{}/actions/run/{}",
            provider.host,
            original.repository_id(),
            original.run().id
        );
        let result = (|| {
            let bytes = bounded.execute(
                vec![
                    "api".into(),
                    endpoint,
                    "--hostname".into(),
                    provider.host.clone(),
                    "--method".into(),
                    method.into(),
                    "--include".into(),
                ],
                None,
                Some(resource.clone()),
                &self.cancellation,
            )?;
            self.check_issuer(provider, cancel)?;
            super::empty_http_ack_v5::validate(&bytes, status)?;
            self.check_issuer(provider, cancel)?;
            Ok(ScopedRunHttpAckV5 {
                original: original.clone(),
                action,
                status: if status == "202" { 202 } else { 204 },
            })
        })();
        if result.is_err() {
            provider.runner.mark_ambiguous(&resource);
        }
        result
    }
}
#[cfg(test)]
#[path = "actions_run_ack_v5_tests.rs"]
mod tests;
