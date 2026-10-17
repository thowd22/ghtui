//! Closed fresh Review history source phase. Personal delegates the one shared
//! actual Summary producer; Catalogue retains its original public session ABI.
use super::*;
use crate::providers::gh::{PrResourceParentV1, RepositoryCatalogueAccessSession};

enum PhaseV15<'a> {
    Catalogue {
        session: RepositoryCatalogueAccessSession<'a>,
        measured: GithubProvider,
        parent: Arc<PrResourceParentV1>,
    },
    Personal(crate::personal_pr_entry_v13::summary_history_v75::PersonalSummaryHistoryPhaseV75<'a>),
}
pub(super) struct ReviewHistoryReadPhaseV15<'a> {
    intent: &'a ReviewOperationHistoryIntentV3,
    database: &'a Path,
    cancel: &'a CancellationToken,
    deadline: Instant,
    phase: PhaseV15<'a>,
}
impl<'a> ReviewHistoryReadPhaseV15<'a> {
    pub(super) fn begin(
        intent: &'a ReviewOperationHistoryIntentV3,
        provider: &'a GithubProvider,
        database: &'a Path,
        cancel: &'a CancellationToken,
        deadline: Instant,
        inspection: Option<&crate::providers::gh::ReviewReadLimitsV2>,
    ) -> Result<Self, ScopedReviewFailureV2> {
        clock(cancel, deadline)?;
        let requests = inspection.map_or(Ok(2), |limits| {
            limits.validate().map_err(source_failure)?;
            limits
                .max_pages
                .checked_mul(2)
                .and_then(|n| n.checked_add(5))
                .filter(|n| *n <= 32)
                .ok_or_else(|| {
                    refused("Bounded Review recovery requires at most32 original measured children")
                })
        })?;
        let child_cap = inspection.map_or(128 * 1024, |limits| {
            limits.max_document_bytes.max(128 * 1024)
        });
        let pool_cap = if inspection.is_some() {
            8 * 1024 * 1024
        } else {
            1024 * 1024
        };
        let phase = match &intent.origin {
            ReviewSummaryOriginV15::Catalogue {
                source,
                navigation,
                expected,
                ..
            } => {
                let mut session = provider
                    .begin_repository_catalogue_access(source.source(), &Default::default(), cancel)
                    .map_err(source_failure)?;
                intent.source_identity_v15(session.identity())?;
                let window = session
                    .reserve_measured_external_reads(requests, child_cap, pool_cap, 2, cancel)
                    .map_err(source_failure)?;
                let measured = provider
                    .clone()
                    .with_measured_read_window(window)
                    .map_err(source_failure)?;
                let current = session
                    .verify(navigation.original(), cancel)
                    .map_err(source_failure)?;
                let parent = Arc::new(
                    measured
                        .pr_thread_history_parent_v1(current, expected, cancel)
                        .map_err(source_failure)?,
                );
                intent
                    .validate_local_source_v15(database)
                    .map_err(source_failure)?;
                PhaseV15::Catalogue {
                    session,
                    measured,
                    parent,
                }
            }
            ReviewSummaryOriginV15::Personal(original) => {
                let phase = if let Some(limits) = inspection {
                    crate::personal_pr_entry_v13::summary_history_v75::PersonalSummaryHistoryPhaseV75::begin_review_inspection_v14(original, provider, database, cancel, deadline, limits)
                } else {
                    crate::personal_pr_entry_v13::summary_history_v75::PersonalSummaryHistoryPhaseV75::begin(original, provider, database, cancel, deadline)
                }.map_err(source_failure)?;
                PhaseV15::Personal(phase)
            }
        };
        Ok(Self {
            intent,
            database,
            cancel,
            deadline,
            phase,
        })
    }
    pub(super) fn parent(&self) -> &Arc<PrResourceParentV1> {
        match &self.phase {
            PhaseV15::Catalogue { parent, .. } => parent,
            PhaseV15::Personal(phase) => phase.parent(),
        }
    }
    pub(super) fn identity(&self) -> &HostIdentity {
        match &self.phase {
            PhaseV15::Catalogue { session, .. } => session.identity(),
            PhaseV15::Personal(phase) => phase.identity(),
        }
    }
    pub(super) fn inspect_review_outcome_v15(
        &self,
        selected: Option<(u64, Option<&str>)>,
        requested: bool,
        limits: &crate::providers::gh::ReviewReadLimitsV2,
    ) -> Result<crate::providers::gh::ReviewRecoveryCurrentV11, ScopedReviewFailureV2> {
        match &self.phase {
            PhaseV15::Catalogue {
                measured,
                parent,
                session,
            } => measured
                .inspect_review_outcome_v11(
                    session.identity(),
                    parent,
                    selected,
                    requested,
                    limits,
                    self.cancel,
                    self.deadline,
                )
                .map_err(source_failure),
            PhaseV15::Personal(phase) => phase
                .inspect_review_outcome_v14(selected, requested)
                .map_err(source_failure),
        }
    }
    pub(super) fn finish(
        self,
        guard: impl FnOnce() -> GithubResult<()>,
    ) -> Result<HostIdentity, ScopedReviewFailureV2> {
        let Self {
            intent,
            database,
            cancel,
            deadline,
            phase,
        } = self;
        clock(cancel, deadline)?;
        let identity = match phase {
            PhaseV15::Personal(phase) => phase.finish(guard).map_err(source_failure)?,
            PhaseV15::Catalogue {
                mut session,
                measured,
                parent,
            } => {
                let ReviewSummaryOriginV15::Catalogue {
                    navigation,
                    expected,
                    ..
                } = &intent.origin
                else {
                    return Err(source_failure(safe_local()));
                };
                let current = session
                    .observation_v1(parent.repository_id())
                    .ok_or_else(|| source_failure(safe_local()))?;
                let last = measured
                    .pr_thread_history_parent_v1(current, expected, cancel)
                    .map_err(source_failure)?;
                if !GithubProvider::same_thread_history_parent_v1(&parent, &last) {
                    return Err(source_failure(safe_local()));
                }
                session
                    .verify(navigation.original(), cancel)
                    .map_err(source_failure)?;
                let closed = session
                    .close_with_guard(cancel, || {
                        private_clock(cancel, deadline)?;
                        intent.validate_local_source_v15(database)?;
                        guard()?;
                        private_clock(cancel, deadline)
                    })
                    .map_err(source_failure)?;
                closed.identity().clone()
            }
        };
        intent.source_identity_v15(&identity)?;
        clock(cancel, deadline)?;
        Ok(identity)
    }
}
