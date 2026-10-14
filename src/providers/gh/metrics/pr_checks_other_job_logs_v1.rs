//! Fresh positive exact-attempt membership for another observed job. SAME log
//! capture/parent/budget/runner; a public job DTO/URL cannot mint admission.
use super::*;

pub struct PrCheckOtherJobLogsV1 {
    read: Option<PrCheckLogsObservation>,
    identity: HostIdentity,
    unavailable: Option<PrCheckChoiceUnavailableV1>,
    observed_job: Option<u64>,
}
impl fmt::Debug for PrCheckOtherJobLogsV1 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrCheckOtherJobLogsV1(<closed original read; private logs redacted>)")
    }
}
impl PrCheckOtherJobLogsV1 {
    pub(crate) fn into_read_v2(self) -> Option<PrCheckLogsObservation> {
        self.read
    }
    pub(in super::super) fn from_closed_v2(
        read: Option<PrCheckLogsObservation>,
        identity: HostIdentity,
        unavailable: Option<PrCheckChoiceUnavailableV1>,
        observed_job: Option<u64>,
    ) -> Self {
        Self {
            read,
            identity,
            unavailable,
            observed_job,
        }
    }
    pub fn read(&self) -> Option<&PrCheckLogsObservation> {
        self.read.as_ref()
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn unavailable(&self) -> Option<PrCheckChoiceUnavailableV1> {
        self.unavailable
    }
    pub fn observed_job(&self) -> Option<u64> {
        self.observed_job
    }
}
impl GithubProvider {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn pr_check_job_logs_from_resource_with_guard_v1(
        &self,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        resource: &PrResourceReadV1,
        index: usize,
        origin: Arc<PrCheckRunObservation>,
        choice: &PrCheckChoiceV1,
        limits: &PrCheckLogReadLimits,
        run_limits: &PrCheckRunReadLimits,
        cancel: &CancellationToken,
        guard: impl FnMut(PrCheckLogReadPhase) -> GithubResult<()>,
    ) -> GithubResult<PrCheckOtherJobLogsV1> {
        super::super::member_v2::other_logs_v2(
            self, identity, current, resource, index, origin, choice, limits, run_limits, cancel,
            guard,
        )
    }
}
