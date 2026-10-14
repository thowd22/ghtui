//! Source-only forwarding into the SAME legacy bridge/log implementations.
//! Genuine opaque resource/current row constrain raw documents before SQL.
use super::*;
use crate::providers::gh::{
    CurrentCatalogueRepository, ObservedPrCheckKindV1, PrResourceKindV1, PrResourceReadV1,
};

pub(super) struct ScopedPrCheckBindingV1<'a> {
    current: &'a CurrentCatalogueRepository,
    original: &'a PrResourceReadV1,
    index: usize,
}
impl<'a> ScopedPrCheckBindingV1<'a> {
    pub(super) fn new(
        current: &'a CurrentCatalogueRepository,
        original: &'a PrResourceReadV1,
        index: usize,
    ) -> GithubResult<Self> {
        let selected = original.checks().get(index).ok_or_else(|| {
            error(
                GithubErrorKind::InvalidTarget,
                "Actual selected CheckRun unavailable",
            )
        })?;
        if original.kind() != PrResourceKindV1::Checks
            || selected.kind != ObservedPrCheckKindV1::CheckRun
            || selected.id == 0
            || selected.head_sha != original.parent().head_sha()
            || current.repository().id != original.parent().repository_id()
            || current.repository().node_id.as_deref() != Some(original.parent().repository_node())
            || current.repository().permissions.pull != Some(true)
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Genuine original resource/current repository/selected CheckRun required",
            ));
        }
        Ok(Self {
            current,
            original,
            index,
        })
    }
    pub(super) fn repository(&self, raw: &Value) -> GithubResult<()> {
        let current = self.current.repository();
        if raw["id"].as_u64() != Some(current.id)
            || raw["node_id"].as_str() != current.node_id.as_deref()
            || !raw["full_name"]
                .as_str()
                .is_some_and(|name| name.eq_ignore_ascii_case(&current.full_name))
            || raw["permissions"]["pull"].as_bool() != Some(true)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original genuine repository identity changed before private bridge read",
            ));
        }
        if let Some(owner) = current.owner.as_ref() {
            if owner.database_id.is_some() && raw["owner"]["id"].as_u64() != owner.database_id
                || owner.node_id.is_some()
                    && raw["owner"]["node_id"].as_str() != owner.node_id.as_deref()
                || !owner.actor.actor_type.is_empty()
                    && raw["owner"]["type"].as_str() != Some(owner.actor.actor_type.as_str())
            {
                return Err(error(
                    GithubErrorKind::IdentityChanged,
                    "Original genuine repository owner changed before private bridge read",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn parent(&self, provider: &GithubProvider, raw: &Value) -> GithubResult<()> {
        self.original
            .validate_check_parent_v1(provider, self.current, raw)
    }
    pub(super) fn check(&self, raw: &Value) -> GithubResult<()> {
        let check = &self.original.checks()[self.index];
        if raw["id"].as_u64() != Some(check.id)
            || raw["head_sha"].as_str() != Some(check.head_sha.as_str())
            || check
                .node_id
                .as_ref()
                .is_some_and(|node| raw["node_id"].as_str() != Some(node.as_str()))
            || check.suite_id.is_some() && raw["check_suite"]["id"].as_u64() != check.suite_id
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original CheckRun node/suite/head changed before private bridge read",
            ));
        }
        Ok(())
    }
    pub(super) fn summary_and_check(&self) -> (PrSummary, Check) {
        // Resource summary is bounded and body-empty by its genuine raw parser.
        let mut summary = self.original.parent().summary().clone();
        summary.pr.repository = self.current.repository().full_name.clone();
        if summary.merged {
            summary.pr.state = "closed".into();
        }
        let observed = &self.original.checks()[self.index];
        let selected = Check {
            id: observed.id,
            head_sha: observed.head_sha.clone(),
            kind: "check".into(),
            name: String::new(),
            status: String::new(),
            conclusion: String::new(),
            url: String::new(),
        };
        (summary, selected)
    }
}
impl GithubProvider {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pr_check_run_from_resource_with_guard_v1(
        &self,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        original: &PrResourceReadV1,
        check_index: usize,
        limits: &PrCheckRunReadLimits,
        cancel: &CancellationToken,
        guard: impl FnMut(PrCheckReadPhase) -> GithubResult<()>,
    ) -> GithubResult<PrCheckRunObservation> {
        let binding = ScopedPrCheckBindingV1::new(current, original, check_index)?;
        let (summary, selected) = binding.summary_and_check();
        let mut observation = self.pr_check_run_impl_v1(
            identity,
            &summary,
            &selected,
            limits,
            cancel,
            guard,
            Some(&binding),
        )?;
        // A bounded partial catalogue cannot prove the unique correlated choice.
        if !observation.run_coverage.enumeration_complete {
            observation.link = None;
            observation.unavailable = Some(PrCheckBridgeUnavailable::PartialRunCatalogue);
        } else if observation.link.is_some() && !observation.job_coverage.enumeration_complete {
            observation.link = None;
            observation.unavailable = Some(PrCheckBridgeUnavailable::PartialJobCatalogue);
        }
        Ok(observation)
    }
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pr_check_logs_from_resource_with_guard_v1(
        &self,
        origin: Arc<PrCheckRunObservation>,
        current: &CurrentCatalogueRepository,
        original: &PrResourceReadV1,
        check_index: usize,
        limits: &PrCheckLogReadLimits,
        cancel: &CancellationToken,
        guard: impl FnMut(PrCheckLogReadPhase) -> GithubResult<()>,
    ) -> GithubResult<PrCheckLogsObservation> {
        let binding = ScopedPrCheckBindingV1::new(current, original, check_index)?;
        if !origin.matches_resource_v1(&origin.identity, original, check_index)
            || !origin
                .repository
                .full_name
                .eq_ignore_ascii_case(&current.repository().full_name)
        {
            return Err(error(
                GithubErrorKind::IdentityChanged,
                "Original scoped check/log source changed; no log route retargeted",
            ));
        }
        self.pr_check_logs_impl_v1(origin, limits, cancel, guard, Some(&binding))
    }
}
impl PrCheckRunObservation {
    /// Pure equality of genuine captured source facts; NOT later access proof.
    pub(crate) fn matches_resource_v1(
        &self,
        identity: &HostIdentity,
        original: &PrResourceReadV1,
        index: usize,
    ) -> bool {
        let Some(check) = original.checks().get(index) else {
            return false;
        };
        let parent = original.parent();
        let summary = parent.summary();
        self.identity.host == identity.host
            && self.original.host == summary.pr.host
            && self.identity.account_id.is_some_and(|id| id > 0)
            && self.identity.account_id == identity.account_id
            && self.repository.id == parent.repository_id()
            && self.repository.node_id == parent.repository_node()
            && self.pr_database_id == parent.database_id()
            && self.original.number == parent.number()
            && self.original.pr_node == parent.node_id()
            && self.original.head == parent.head_sha()
            && self.original.base == parent.base_tip_sha()
            && self.original.head_ref == summary.head_ref
            && self.original.base_ref == summary.base_ref
            && self.original.state
                == if summary.merged {
                    "closed"
                } else {
                    &summary.pr.state
                }
            && self.original.draft == summary.pr.draft
            && self.original.merged == summary.merged
            && check.kind == ObservedPrCheckKindV1::CheckRun
            && self.original.check_id == check.id
            && check.head_sha == self.original.head
            && check
                .node_id
                .as_ref()
                .is_none_or(|node| node == &self.check_node)
            && check.suite_id.is_none_or(|suite| suite == self.suite_id)
    }
}
