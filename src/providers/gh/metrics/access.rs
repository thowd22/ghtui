//! Undeclared lightweight current-identity reads for Dashboard guards/drills.
//! Root owns fresh principal/journal/access brackets. These do not confer history.
use super::*;

fn canonical_repository(
    provider: &GithubProvider,
    identity: &HostIdentity,
    expected: &MetricsRepository,
) -> GithubResult<()> {
    scope(provider, identity, &expected.full_name)?;
    if expected.host != provider.host
        || expected.full_name != expected.full_name.to_ascii_lowercase()
        || expected.id == 0
        || !valid_opaque(&expected.node_id)
    {
        return Err(error(
            GithubErrorKind::InvalidTarget,
            "Current metrics access needs the exact positive canonical repository identity",
        ));
    }
    Ok(())
}
impl GithubProvider {
    /// Bounded fresh read access for cached discussion admission. This does not
    /// authorize mutation, renew cached text or infer permissions from a prior
    /// SQL binding. The caller brackets the batch with numeric identity checks.
    pub fn cached_discussion_repository_access(
        &self,
        identity: &HostIdentity,
        expected: &[(String, u64)],
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        if expected.len() > 256 {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Cached-source access repository budget exceeded",
            ));
        }
        let limits = MetricsReadLimits {
            max_document_bytes: 128 * 1024,
            max_response_bytes: 16 * 1024 * 1024,
            ..Default::default()
        };
        let mut budget = MetricsBudget::new(&limits)?;
        let mut seen = std::collections::BTreeSet::new();
        for (name, id) in expected {
            scope(self, identity, name)?;
            if *id == 0
                || *id > i64::MAX as u64
                || name != &name.to_ascii_lowercase()
                || !seen.insert(name)
            {
                return Err(error(
                    GithubErrorKind::InvalidTarget,
                    "Cached-source access requires unique exact positive repository identities",
                ));
            }
            let raw = budget.read_document(self, &format!("repos/{name}"), cancel)?;
            // Discussion bindings retain a genuine numeric repository ID, not
            // a repository node. Do not invent a missing node or require an
            // unrecorded alias: fresh exact name/positive ID/access suffices.
            let current_name = text(&raw, "full_name");
            validate_repo(&current_name)?;
            if !current_name.eq_ignore_ascii_case(name) || positive(&raw, "id")? != *id {
                return Err(error(GithubErrorKind::IdentityChanged,"Cached discussion repository identity changed; previous private source unavailable"));
            }
            if raw["permissions"]["pull"].as_bool() != Some(true) {
                return Err(error(GithubErrorKind::Permission,"Current repository read access is unverified; previous cached discussion unavailable"));
            }
        }
        Ok(())
    }
    /// Exactly one bounded repository GET on success. No discovery or hidden
    /// identity request; principal/access/error classification belongs to Root.
    pub fn metrics_repository_identity(
        &self,
        identity: &HostIdentity,
        expected: &MetricsRepository,
        limits: &MetricsReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<MetricsRepository> {
        canonical_repository(self, identity, expected)?;
        limits.validate()?;
        let mut budget = MetricsBudget::new(limits)?;
        let endpoint = format!("repos/{}", expected.full_name);
        let current = repository(
            &self.host,
            &expected.full_name,
            &budget.read_document(self, &endpoint, cancel)?,
        )?;
        require_same_repo(expected, &current)?;
        Ok(current)
    }
    /// Exactly repository/PR/repository GETs on success, sharing one aggregate
    /// budget. No queue, detail/page load or lifetime/histogram authority.
    pub fn metrics_pr_identity(
        &self,
        identity: &HostIdentity,
        original: &MetricsPrObservation,
        limits: &MetricsReadLimits,
        cancel: &CancellationToken,
    ) -> GithubResult<MetricsPrObservation> {
        canonical_repository(self, identity, &original.repository)?;
        limits.validate()?;
        if original.id == 0
            || original.number == 0
            || !valid_opaque(&original.node_id)
            || original
                .head_sha
                .as_ref()
                .is_none_or(|head| !crate::search::model::valid_sha(head))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Current PR drill needs its positive database/node/number and original full head",
            ));
        }
        let mut budget = MetricsBudget::new(limits)?;
        let repo_endpoint = format!("repos/{}", original.repository.full_name);
        let repo = repository(
            &self.host,
            &original.repository.full_name,
            &budget.read_document(self, &repo_endpoint, cancel)?,
        )?;
        require_same_repo(&original.repository, &repo)?;
        let parent = parse_pr(
            &repo,
            &budget.read_document(
                self,
                &format!("{repo_endpoint}/pulls/{}", original.number),
                cancel,
            )?,
        )?;
        require_same_parent(original, &parent)?;
        let final_repo = repository(
            &self.host,
            &original.repository.full_name,
            &budget.read_document(self, &repo_endpoint, cancel)?,
        )?;
        require_same_repo(&repo, &final_repo)?;
        Ok(parent)
    }
}
