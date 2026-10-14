//! Explicit positive choice through the SAME bridge/attempt kernel. No query
//! widening, first-row selection, serialized authority or new auth stack.
use super::*;

impl GithubProvider {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pr_check_selected_from_resource_with_guard_v1(
        &self,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        resource: &PrResourceReadV1,
        index: usize,
        expected: &Arc<PrCheckRunObservation>,
        choice: &PrCheckChoiceV1,
        limits: &PrCheckRunReadLimits,
        cancel: &CancellationToken,
        guard: impl FnMut(PrCheckSelectedReadPhaseV1) -> GithubResult<()>,
    ) -> GithubResult<PrCheckRunObservation> {
        match self.pr_check_selection_impl_v2(
            identity,
            current,
            resource,
            index,
            expected,
            choice,
            limits,
            cancel,
            guard,
            super::super::advance_v2::SelectionPolicyV2::Pinned,
        )? {
            PrCheckRefreshReadV2::Current(value) => Ok(*value),
            PrCheckRefreshReadV2::AttemptAdvanced(_) => Err(error(
                GithubErrorKind::StaleHead,
                "Pinned choice never adopts another attempt",
            )),
        }
    }
}
