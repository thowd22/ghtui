//! SOURCE-ONLY retained-history worker. No module declaration/Cargo/native claim
//! until the coordinated domain/provider/capture/storage activation.
//! Opaque Pending deliberately exposes no evidence before final scope proof.
use crate::{
    metrics::{dashboard, DashboardEvidence, DashboardSnapshot, EventCategory, PrTarget},
    metrics_repository::MetricsReadGuard,
    metrics_workspace::MetricsWorkspace,
    providers::{
        gh::{
            GithubProvider, HostIdentity, MetricsBackfillRequestPair, MetricsPrObservation,
            MetricsScopeReceipt, MetricsScopeReceiptBatch,
        },
        runner::CancellationToken,
    },
    storage::{
        AvailableMetricCategoryBatch, Cache, MetricCategoryBatch, MetricEventTarget,
        MetricReadIntent, MetricsRetentionLimits, StoredMetricFrame,
    },
};
use anyhow::{bail, Context, Result};
use std::{collections::BTreeSet, path::PathBuf, sync::Arc, time::Duration};
mod authority;
mod budget;
use authority::{AuthorityContext, HistoryAuthority, SqlExpectations};
use budget::HistoryBudget;
pub use budget::{MetricsHistoryLimits, MetricsHistoryUsage};

pub struct MetricsHistoryRequest {
    pub workspace: MetricsWorkspace,
    pub database: PathBuf,
    /// Exact user-selected original canonical parents, not an implicit cohort.
    pub selected_parents: Vec<PrTarget>,
    pub limits: MetricsHistoryLimits,
}

/// Keeps the same immutable request, budget, journal and committed source proof
/// across Root's final principal read. No public evidence/snapshot accessor.
pub struct MetricsHistoryPending {
    request: MetricsHistoryRequest,
    identity: HostIdentity,
    guard: Arc<MetricsReadGuard>,
    selected: Vec<MetricsPrObservation>,
    frames: Vec<StoredMetricFrame>,
    expected: SqlExpectations,
    budget: HistoryBudget,
}
impl std::fmt::Debug for MetricsHistoryPending {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricsHistoryPending")
            .field("selected_parents", &self.selected.len())
            .field("source_frames", &self.frames.len())
            .finish_non_exhaustive()
    }
}
pub struct MetricsHistoryPrepared {
    pub evidence: Arc<DashboardEvidence>,
    pub snapshot: Arc<DashboardSnapshot>,
    pub frames: Arc<[StoredMetricFrame]>,
    pub read_guard: Arc<MetricsReadGuard>,
    pub verified_identity: HostIdentity,
    pub usage: MetricsHistoryUsage,
}

fn exact_target(target: &PrTarget, original: &MetricsPrObservation) -> bool {
    target.repository == original.repository.full_name
        && target.repository_id == Some(original.repository.id)
        && target.database_id == Some(original.id)
        && target.node_id.as_deref() == Some(original.node_id.as_str())
        && target.number == original.number
}
fn selection(
    request: &MetricsHistoryRequest,
    identity: &HostIdentity,
) -> Result<Vec<MetricsPrObservation>> {
    request.limits.validate()?;
    let workspace = &request.workspace;
    if identity.account_id != Some(workspace.snapshot.scope.account_id())
        || identity.host != workspace.snapshot.scope.host()
        || workspace.snapshot.scope != workspace.evidence.scope
        || request.selected_parents.is_empty()
        || request.selected_parents.len() > request.limits.max_parents
        || workspace.snapshot.window.start >= workspace.snapshot.window.end
        || workspace.snapshot.window.end > workspace.snapshot.as_of
    {
        bail!("History request differs from the original positive explicit scope/as-of/parent cap");
    }
    let mut selected = Vec::with_capacity(request.selected_parents.len());
    let mut seen = BTreeSet::new();
    for target in &request.selected_parents {
        target
            .validate(&workspace.snapshot.scope)
            .map_err(anyhow::Error::msg)?;
        if !target.has_canonical_identity()
            || !seen.insert((target.repository.clone(), target.number))
        {
            bail!("History selection needs distinct genuine canonical parents; numbers alone are not authority");
        }
        let original = workspace
            .observations
            .iter()
            .find(|original| exact_target(target, original))
            .context("Selected history parent is absent from the original canonical inventory")?;
        let created = original
            .created_at
            .as_ref()
            .context("Original created_at unavailable; no lifetime interval invented")?
            .unix_seconds();
        if created > workspace.snapshot.as_of
            || original
                .head_sha
                .as_ref()
                .is_none_or(|sha| !crate::search::model::valid_sha(sha))
            || !workspace
                .evidence
                .prs
                .iter()
                .any(|pr| exact_target(&pr.target, original))
            || !workspace
                .repositories
                .iter()
                .any(|repo| repo == &original.repository)
        {
            bail!("Original history creation/head/evidence/header is missing or contradictory");
        }
        selected.push(original.clone());
    }
    Ok(selected)
}

/// This function consumes genuine provider receipts, never a caller auth bool.
/// Root must keep old counts/actions hidden and run its final principal bracket
/// before invoking Pending.publish. All reads are bounded and never retried.
pub fn load_history(
    request: MetricsHistoryRequest,
    identity: &HostIdentity,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<MetricsHistoryPending> {
    let mut budget = HistoryBudget::new(
        &request.limits,
        request.selected_parents.len(),
        request.workspace.repositories.len(),
    )?;
    let selected = selection(&request, identity)?;
    let guard = request
        .workspace
        .read_guard
        .clone()
        .context("Original pre-fetch Dashboard journal guard absent")?;
    guard
        .validate(&request.database)
        .map_err(anyhow::Error::msg)?;
    guard
        .validate_parents(&request.workspace.observations)
        .map_err(anyhow::Error::msg)?;
    budget.check(cancel)?;
    let mut cache = Cache::open(&request.database)?;
    let mut expected =
        SqlExpectations::new(request.workspace.snapshot.as_of, &request.limits.retention);
    let mut frames = Vec::with_capacity(selected.len() * 2);
    for original in &selected {
        let limits = budget.scope_limits(5, cancel)?;
        let receipt = match provider.metric_scope_receipt(identity, original, &limits, cancel) {
            Ok(receipt) => receipt,
            Err(error) => {
                budget.failed_group(5, limits.max_response_bytes)?;
                return Err(error.into());
            }
        };
        budget.charge(receipt.accounting())?;
        let intent = MetricReadIntent {
            requested_from: Some(
                original
                    .created_at
                    .as_ref()
                    .context("Original creation boundary absent")?
                    .unix_seconds(),
            ),
            requested_through: request.workspace.snapshot.as_of,
        };
        let (reviews, timeline, pair) = {
            let mut authority = HistoryAuthority::single(
                AuthorityContext::new(&request, identity, &guard, &budget, cancel),
                original,
                &receipt,
                &mut expected,
            );
            let reviews = cache.reserve_metric_events(
                &MetricEventTarget::from_provider(identity, original, EventCategory::Reviews)?,
                &intent,
                &request.limits.retention,
                &mut authority,
            )?;
            let timeline = cache.reserve_metric_events(
                &MetricEventTarget::from_provider(identity, original, EventCategory::Timeline)?,
                &intent,
                &request.limits.retention,
                &mut authority,
            )?;
            let pair = MetricsBackfillRequestPair::new(
                reviews.backfill_request(&cache, identity, original, &mut authority)?,
                timeline.backfill_request(&cache, identity, original, &mut authority)?,
            )?;
            (reviews, timeline, pair)
        };
        let limits = budget.history_limits(cancel)?;
        let backfill = match provider.metric_history_backfill(&pair, &limits, cancel) {
            Ok(backfill) => backfill,
            Err(error) => {
                budget.failed_group(6 + 2 * limits.max_pages, limits.max_response_bytes)?;
                return Err(error.into());
            }
        };
        budget.charge(backfill.scope_receipt().accounting())?;
        let mut authority = HistoryAuthority::single(
            AuthorityContext::new(&request, identity, &guard, &budget, cancel),
            original,
            backfill.scope_receipt(),
            &mut expected,
        );
        for reservation in [&reviews, &timeline] {
            let frame = match AvailableMetricCategoryBatch::from_backfill(
                &cache,
                reservation,
                &backfill,
                &mut authority,
            )? {
                Some(available) => {
                    cache.commit_metric_history(reservation, available, &mut authority)?
                }
                None => cache.commit_metric_events(
                    reservation,
                    &MetricCategoryBatch::from_provider(
                        backfill.observations(),
                        reservation.target().category(),
                    )?,
                    &mut authority,
                )?,
            };
            frame.validate(&cache, &request.limits.retention, &mut authority)?;
            frames.push(frame);
        }
    }
    budget.check(cancel)?;
    Ok(MetricsHistoryPending {
        request,
        identity: identity.clone(),
        guard,
        selected,
        frames,
        expected,
        budget,
    })
}

impl MetricsHistoryPending {
    /// Called after Root's actual final identity read. Adds the final fresh
    /// whole-known-scope proof, then re-reads SQL/journal BEFORE exposing counts.
    pub fn publish(
        mut self,
        current_identity: &HostIdentity,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> Result<MetricsHistoryPrepared> {
        if !self.identity.same_account(current_identity) {
            bail!("History final positive principal changed; original private evidence remains hidden");
        }
        self.budget.check(cancel)?;
        self.guard
            .validate(&self.request.database)
            .map_err(anyhow::Error::msg)?;
        self.guard
            .validate_parents(&self.request.workspace.observations)
            .map_err(anyhow::Error::msg)?;
        // Reject a known local invalidation before sending more gh reads. This
        // metadata-only check is NOT a substitute for final remote authority or
        // the full guarded source recaptures below.
        for frame in &self.frames {
            let original = self
                .selected
                .iter()
                .find(|parent| {
                    frame
                        .target()
                        .validate_provider_parent(current_identity, parent)
                        .is_ok()
                })
                .context("Original pending frame parent absent")?;
            self.expected.check(
                &self.request.database,
                frame.target(),
                original,
                crate::storage::MetricAuthorityPhase::BeforeRead,
            )?;
        }
        let selected_repos = self
            .selected
            .iter()
            .map(|parent| parent.repository.full_name.as_str())
            .collect::<BTreeSet<_>>();
        let extras = self
            .request
            .workspace
            .repositories
            .iter()
            .filter(|repo| !selected_repos.contains(repo.full_name.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let reads = self.selected.len() * 3 + extras.len() + 2;
        let limits = self.budget.scope_limits(reads, cancel)?;
        let receipt = match provider.metric_scope_receipts_with_repositories(
            current_identity,
            &self.selected,
            &extras,
            &limits,
            cancel,
        ) {
            Ok(receipt) => receipt,
            Err(error) => {
                self.budget.failed_group(reads, limits.max_response_bytes)?;
                return Err(error.into());
            }
        };
        self.budget.charge(receipt.accounting())?;
        receipt.validate_repositories(
            current_identity,
            &self.request.workspace.repositories,
            self.request.limits.sql_lease,
            cancel,
        )?;
        let cache = Cache::open(&self.request.database)?;
        let mut authority = HistoryAuthority::batch(
            AuthorityContext::new(
                &self.request,
                current_identity,
                &self.guard,
                &self.budget,
                cancel,
            ),
            &receipt,
            &mut self.expected,
        );
        for frame in &self.frames {
            frame.validate(&cache, &self.request.limits.retention, &mut authority)?;
        }
        let mut evidence = (*self.request.workspace.evidence).clone();
        for frame in &self.frames {
            let parent = self
                .selected
                .iter()
                .find(|parent| {
                    frame
                        .target()
                        .validate_provider_parent(current_identity, parent)
                        .is_ok()
                })
                .context("Committed frame has no exact original selected parent")?;
            let collection = frame.validated_projection(&evidence.scope, parent)?;
            let pr = evidence
                .prs
                .iter_mut()
                .find(|pr| exact_target(&pr.target, parent))
                .context("Retained contributor parent disappeared from original cohort")?;
            match frame.target().category() {
                EventCategory::Reviews => pr.reviews = collection,
                EventCategory::Timeline => pr.timeline = collection,
            }
        }
        let snapshot = dashboard(
            &evidence,
            self.request.workspace.snapshot.as_of,
            self.request.workspace.snapshot.window,
        )
        .map_err(anyhow::Error::msg)?;
        // Formula preparation is bounded worker work, but may consume the lease.
        // Do not return an earlier SQL observation after preparation changed it.
        for frame in &self.frames {
            frame.validate(&cache, &self.request.limits.retention, &mut authority)?;
        }
        self.guard
            .validate_parents(&self.request.workspace.observations)
            .map_err(anyhow::Error::msg)?;
        self.budget.check(cancel)?;
        Ok(MetricsHistoryPrepared {
            evidence: Arc::new(evidence),
            snapshot: Arc::new(snapshot),
            frames: Arc::from(self.frames),
            read_guard: self.guard,
            verified_identity: current_identity.clone(),
            usage: self.budget.usage(),
        })
    }
}

#[cfg(test)]
#[path = "metrics_history/tests.rs"]
mod tests;
