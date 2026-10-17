//! Closed member observation issued by the SAME original Core's measured
//! allocation. No caller can turn rows, a DTO or a saved origin into this type.
use super::*;
use crate::{
    pr_metadata_actions::{
        canonical_members, current_members, PrMetadataCommand, PrMetadataMember, PrMetadataPlan,
    },
    providers::{gh::ThreadPhaseIssuerV16, runner::ScopedReadBudgetV1},
};
use std::{
    sync::{Arc, Weak},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
#[path = "original_members_v48/bounded_read_v62.rs"]
mod bounded_read_v62;
pub(crate) use bounded_read_v62::MembersMetadataHistoryReadV62;
#[path = "original_members_v48/manual_refresh.rs"]
mod manual_refresh;
#[path = "original_members_v48/publication.rs"]
mod publication;
#[path = "original_members_v48/recovery.rs"]
mod recovery;
#[path = "original_members_v48/refresh.rs"]
mod refresh;
use crate::providers::gh::pr_resources_v1::metadata_context_v51::MetadataMembersHeapV62;
pub(in crate::providers::gh) use publication::PreparedMembersRequestV48;
pub(crate) use publication::{
    MembersWriteV48, PrMetadataMembersFailureV48, PrMetadataMembersReceiptV48,
};
pub(crate) use recovery::MetadataMembersRecoveryCurrentV48;
pub(crate) use refresh::ActualMembersPageContextV61;
pub(in crate::providers::gh) use refresh::{MembersVisibilityV48, MetadataMembersContextV48};

pub(crate) const METADATA_MEMBERS_POST_CAPS_V48: [usize; 18] = [
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    64 * 1024,
    64 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    128 * 1024,
    64 * 1024,
];
pub(crate) const METADATA_MEMBERS_TAB_POST_CAPS_V62: [usize; 22] = [
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    64 * 1024,
    64 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    64 * 1024,
];
pub(crate) const METADATA_MEMBERS_TAB_REFRESH_CAPS_V62: [usize; 13] = [
    128 * 1024,
    128 * 1024,
    128 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
    128 * 1024,
];
pub(crate) const METADATA_MEMBERS_REFRESH_CAPS_V48: [usize; 9] = [
    128 * 1024,
    128 * 1024,
    128 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    512 * 1024,
    128 * 1024,
];

/// These are retained observation facts, not a cache lease, approval or source.
pub(crate) struct OriginalMetadataMembersV48 {
    parent: Arc<PrResourceParentV1>,
    read: Arc<PrMetadataCatalogueRead>,
    command: PrMetadataCommand,
    issuer: ThreadPhaseIssuerV16,
    pool: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
    runner_generation: u64,
    caller_can_mutate: bool,
    limits: PrMetadataReadLimits,
    // Only namespaces corroborated by the SAME complete GraphQL response.
    // REST candidate rows may omit organization, so their Option spelling
    // must never create a different stable action base for the same Team.
    team_namespaces: std::collections::BTreeMap<String, String>,
    purpose: MetadataMembersReadPurposeV48,
    model_bound_v62: usize,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetadataMembersReadPurposeV48 {
    Eligible,
    Inspection,
    MetadataEligibleV62,
    MetadataInspectionV62,
}
pub(crate) struct MetadataMembersObservationV48<'a> {
    pub(crate) current: &'a CurrentCatalogueRepository,
    pub(crate) intent: &'a PrParentIntentV13,
    pub(crate) identity: &'a HostIdentity,
    pub(crate) command: PrMetadataCommand,
    pub(crate) limits: &'a PrMetadataReadLimits,
    pub(crate) selected_plan: Option<&'a PrMetadataPlan>,
    pub(crate) purpose: MetadataMembersReadPurposeV48,
    pub(crate) retained_v62: Option<&'a OriginalMetadataMembersV48>,
}
impl std::fmt::Debug for OriginalMetadataMembersV48 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalMetadataMembersV48(<actual measured member observation>)")
    }
}
fn refused(kind: GithubErrorKind, message: &'static str) -> GithubError {
    error(kind, message)
}
fn alive(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(refused(
            GithubErrorKind::Transport,
            "Original member phase cancelled/expired",
        ));
    }
    Ok(())
}
impl PrMetadataReadLimits {
    pub(crate) fn validate_original_members_v48(&self) -> GithubResult<()> {
        self.as_metrics()?;
        if !(1..=10).contains(&self.max_pages) {
            return Err(refused(
                GithubErrorKind::InvalidTarget,
                "Member catalogue requires1..10 explicit pages",
            ));
        }
        Ok(())
    }
}
fn measured(
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> GithubResult<(Arc<ScopedReadBudgetV1>, Instant)> {
    let pool = provider.scoped_read_budget.as_ref().ok_or_else(|| {
        refused(
            GithubErrorKind::InvalidTarget,
            "Member observation requires its actual measured original Core",
        )
    })?;
    let deadline = provider
        .scoped_read_window
        .ok_or_else(|| {
            refused(
                GithubErrorKind::InvalidTarget,
                "Original member window unavailable",
            )
        })?
        .0
        .min(pool.deadline());
    alive(cancel, deadline)?;
    pool.check_clock(cancel).map_err(GithubError::from)?;
    if !pool.original_token_matches_v63(cancel)
        || pool.accounting().map_err(GithubError::from)?.sealed()
    {
        return Err(refused(
            GithubErrorKind::InvalidTarget,
            "Member observation cannot adopt a sealed/foreign pool",
        ));
    }
    Ok((Arc::clone(pool), deadline))
}
fn error_free(raw: &Value) -> GithubResult<()> {
    if !raw.is_object()
        || raw
            .get("errors")
            .is_some_and(|errors| !matches!(errors, Value::Array(rows) if rows.is_empty()))
    {
        return Err(refused(
            GithubErrorKind::Json,
            "Partial/malformed GraphQL errors cannot prove metadata membership",
        ));
    }
    Ok(())
}
fn observed_utc() -> GithubResult<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_secs())
        .map_err(|_| {
            refused(
                GithubErrorKind::Transport,
                "Original metadata observation UTC unavailable",
            )
        })
}

/// Candidate-specific sub-budget on the attached original pool. Its failures
/// charge the entire effective cap, even when diagnostics were truncated.
pub(super) struct MemberPageCaptureV48<'a> {
    limits: &'a PrMetadataReadLimits,
    deadline: Instant,
    accounting: &'a mut PrMetadataReadAccounting,
    exhausted: bool,
    metadata_heap_v62: Option<&'a mut MetadataMembersHeapV62>,
}
impl MemberPageCaptureV48<'_> {
    pub(super) fn stdout_bytes(&self) -> usize {
        self.accounting.successful_stdout_bytes
    }
    pub(super) fn read(
        &mut self,
        provider: &GithubProvider,
        endpoint: &str,
        cancel: &CancellationToken,
    ) -> GithubResult<Value> {
        let (_, deadline) = measured(provider, cancel)?;
        alive(cancel, self.deadline.min(deadline))?;
        let left = self
            .limits
            .max_response_bytes
            .saturating_sub(self.accounting.charged_bytes());
        if self.exhausted || left == 0 {
            return Err(refused(
                GithubErrorKind::Transport,
                "Eligible member capture budget exhausted; unseen choices unknown",
            ));
        }
        let mut spec = provider.spec(vec![
            "api".into(),
            endpoint.into(),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "GET".into(),
        ]);
        spec.output_limit = spec
            .output_limit
            .min(left)
            .min(self.limits.max_document_bytes);
        spec.absolute_deadline_v1 = Some(
            spec.absolute_deadline_v1
                .map_or(self.deadline, |old| old.min(self.deadline)),
        );
        let cap = spec.output_limit;
        self.accounting.requests += 1;
        let output = match provider.runner.run(spec, cancel) {
            Ok(output) => output,
            Err(failure) => {
                self.accounting.failed_reserved_bytes = self
                    .accounting
                    .failed_reserved_bytes
                    .checked_add(cap)
                    .ok_or_else(|| {
                        refused(
                            GithubErrorKind::Transport,
                            "Eligible failed capture accounting overflow",
                        )
                    })?;
                self.exhausted = matches!(
                    failure.kind,
                    FailureKind::OutputLimit | FailureKind::Timeout | FailureKind::Cancelled
                );
                let mut error: GithubError = failure.into();
                if matches!(error.kind, GithubErrorKind::Permission) {
                    error.message =
                        "Member endpoint forbidden (HTTP 403); no empty catalogue inferred".into();
                }
                return Err(error);
            }
        };
        self.accounting.successful_stdout_bytes += output.capture.stdout_bytes();
        self.accounting.successful_diagnostic_bytes += output.capture.stderr_bytes();
        alive(cancel, self.deadline.min(deadline))?;
        match self.metadata_heap_v62.as_mut() {
            Some(heap) => heap.eligible_page(&output.stdout),
            None => serde_json::from_slice(&output.stdout)
                .map_err(|e| error(GithubErrorKind::Json, e.to_string())),
        }
    }
}

pub(super) fn members_query_v48(
    command: PrMetadataCommand,
    selected: Option<&PrMetadataMember>,
) -> GithubResult<Value> {
    // Reuse the existing field grammar; remove only the legacy optional Team
    // alias. A candidate alias is emitted solely for a genuine selected Team.
    let mut query = publication_actions::pr_metadata_members_query(command.field())?;
    let selected = if command == PrMetadataCommand::AddTeam {
        selected
    } else {
        None
    };
    let legacy_alias = " candidate:node(id:$candidate) @include(if:$includeCandidate){__typename ...on Team{id slug organization{login}}}";
    if command.field() == "teams" {
        if let Some(actual) = selected {
            query = query
                .replace(
                    ",$candidate:ID!,$includeCandidate:Boolean!",
                    ",$candidate:ID!",
                )
                .replace(" @include(if:$includeCandidate)", "");
            return Ok(json!({"query":query,"variables":{"candidate":actual.node()}}));
        }
        query = query
            .replace(",$candidate:ID!,$includeCandidate:Boolean!", "")
            .replace(legacy_alias, "");
    }
    Ok(json!({"query":query,"variables":{}}))
}
fn opposite_members(
    read: &PrMetadataCatalogueRead,
    command: PrMetadataCommand,
) -> GithubResult<Vec<PrMetadataMember>> {
    let opposite = match command.field() {
        "reviewers" => Some("teams"),
        "teams" => Some("reviewers"),
        _ => None,
    };
    let Some(field) = opposite else {
        return Ok(vec![]);
    };
    let key = if field == "teams" {
        "requested_teams"
    } else {
        "requested_reviewers"
    };
    let rows = read
        .parent_raw()
        .get(key)
        .and_then(Value::as_array)
        .ok_or_else(|| {
            refused(
                GithubErrorKind::Json,
                "Actual opposite review membership omitted; unknown is not empty",
            )
        })?;
    canonical_members(
        rows.iter()
            .map(|row| PrMetadataMember::from_rest(row, field, &read.target().pr.repository))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| error(GithubErrorKind::Json, e))?,
    )
    .map_err(|e| error(GithubErrorKind::Json, e))
}
impl OriginalMetadataMembersV48 {
    /// Local descriptive keys only. No file/SQL/provider I/O, current claim,
    /// approval or terminal lookup; a worker still proves all current authority.
    pub(crate) fn action_keys_v48(&self, plan: &PrMetadataPlan) -> GithubResult<(String, String)> {
        use sha2::{Digest, Sha256};
        plan.validate_original_members_v48(self)
            .map_err(|message| error(GithubErrorKind::InvalidTarget, message))?;
        let selected = plan.selected().ok_or_else(|| {
            refused(
                GithubErrorKind::InvalidTarget,
                "Actual selected member required",
            )
        })?;
        let scope =
            crate::storage::Scope::github(&self.identity().host, self.identity().account_id)
                .map_err(|_| {
                    refused(
                        GithubErrorKind::IdentityChanged,
                        "Actual numeric member scope unavailable",
                    )
                })?;
        let namespace_sha256 = self.selected_namespace_sha256_v48(plan)?;
        let parent = self.parent();
        let stable = serde_json::to_vec(&(
            "ghtui-metadata-members-base-v48",
            &scope,
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
            parent.node_id(),
            parent.number(),
            plan.issue_id(),
            plan.issue_node(),
            plan.command().key(),
            selected.id(),
            selected.node(),
            &namespace_sha256,
        ))
        .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?;
        if stable.len() > 8192 {
            return Err(refused(
                GithubErrorKind::InvalidTarget,
                "Member identity key bound exceeded",
            ));
        }
        let base = format!("metadata-members-base-v48/{:x}", Sha256::digest(stable));
        let draft = serde_json::to_vec(&(
            "ghtui-metadata-members-draft-v48",
            &base,
            parent.head_sha(),
            parent.base_tip_sha(),
        ))
        .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?;
        Ok((
            base,
            format!("metadata-members-v48/{:x}", Sha256::digest(draft)),
        ))
    }
    pub(crate) fn selected_namespace_sha256_v48(
        &self,
        plan: &PrMetadataPlan,
    ) -> GithubResult<String> {
        use sha2::{Digest, Sha256};
        let selected = plan.selected().ok_or_else(|| {
            refused(
                GithubErrorKind::InvalidTarget,
                "Actual selected member required",
            )
        })?;
        let namespace = if plan.command().field() == "teams" {
            Some(
                self.team_namespaces
                    .get(selected.node())
                    .ok_or_else(|| {
                        refused(
                    GithubErrorKind::InvalidTarget,
                    "Fresh actual selected Team organization proof required before action key",
                )
                    })?
                    .as_str(),
            )
        } else {
            None
        };
        let encoded = serde_json::to_vec(&namespace)
            .map_err(|e| error(GithubErrorKind::Json, e.to_string()))?;
        Ok(format!("{:x}", Sha256::digest(encoded)))
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn read(&self) -> &Arc<PrMetadataCatalogueRead> {
        &self.read
    }
    pub(crate) fn command(&self) -> PrMetadataCommand {
        self.command
    }
    pub(in crate::providers::gh) fn metadata_tab_v62(&self) -> bool {
        matches!(
            self.purpose,
            MetadataMembersReadPurposeV48::MetadataEligibleV62
                | MetadataMembersReadPurposeV48::MetadataInspectionV62
        )
    }
    pub(in crate::providers::gh) fn retained_model_v62(&self) -> GithubResult<usize> {
        if !self.metadata_tab_v62()
            || self.model_bound_v62 == 0
            || self.model_bound_v62 > 4 * 1024 * 1024
        {
            return Err(refused(
                GithubErrorKind::InvalidTarget,
                "Actual Metadata member model debit unavailable",
            ));
        }
        Ok(self.model_bound_v62)
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        self.read.identity()
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn limits(&self) -> &PrMetadataReadLimits {
        &self.limits
    }
    /// Display/selection availability from the actual Core observation, never
    /// a fresh capability, source or permission to publish from this snapshot.
    pub(crate) fn observed_mutation_available_v48(&self) -> bool {
        self.caller_can_mutate
    }
    pub(crate) fn require_mutation_v48(&self) -> GithubResult<()> {
        alive(&self.cancellation, self.deadline)?;
        if !matches!(
            self.purpose,
            MetadataMembersReadPurposeV48::Eligible
                | MetadataMembersReadPurposeV48::MetadataEligibleV62
        ) || !self.caller_can_mutate
        {
            return Err(refused(
                GithubErrorKind::Permission,
                "Current original requester metadata write capability unavailable",
            ));
        }
        Ok(())
    }
    pub(crate) fn require_selected_v48(&self, index: Option<usize>) -> GithubResult<()> {
        self.require_mutation_v48()?;
        if self.command == PrMetadataCommand::AddLabel {
            let row = index
                .and_then(|index| self.read.rows().get(index))
                .ok_or_else(|| {
                    refused(
                        GithubErrorKind::InvalidTarget,
                        "Select an actual eligible label",
                    )
                })?;
            if row.get("archived_at") != Some(&Value::Null)
                || row.get("archived_by") != Some(&Value::Null)
            {
                return Err(refused(
                    GithubErrorKind::Permission,
                    "Selected label archive evidence unavailable or archived; Add refused",
                ));
            }
        }
        Ok(())
    }
    pub(in crate::providers::gh) fn validate_metadata_measured_v62(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let (pool, deadline) = measured(provider, cancel)?;
        if !self.metadata_tab_v62() || !Arc::ptr_eq(&pool, &self.pool) {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual Metadata member measured phase differs",
            ));
        }
        self.validate_lineage(provider, cancel, deadline)
    }
    fn validate_lineage(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        alive(cancel, deadline.min(self.deadline))?;
        if !self.issuer.matches(provider)
            || provider
                .scoped_read_budget
                .as_ref()
                .is_some_and(|pool| !Arc::ptr_eq(pool, &self.pool))
            || !self.cancellation.same_instance(cancel)
            || !self.pool.original_token_matches_v63(cancel)
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual member producer/token lineage changed",
            ));
        }
        let generation = provider
            .runner
            .review_resource_generation_v11(
                &provider.resource(self.parent.repository(), self.parent.number()),
                cancel,
                deadline.min(self.deadline),
            )
            .map_err(GithubError::from)?;
        if generation != self.runner_generation {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual member runner generation changed",
            ));
        }
        Ok(())
    }
}
impl GithubProvider {
    /// Opening full PR + issue + actual P eligible pages + one complete current
    /// field query. The caller owns the closing raw PR/Core/SQL/final UID.
    pub(crate) fn observe_original_metadata_members_v48(
        &self,
        request: MetadataMembersObservationV48<'_>,
        cancel: &CancellationToken,
    ) -> GithubResult<OriginalMetadataMembersV48> {
        let MetadataMembersObservationV48 {
            current,
            intent,
            identity,
            command,
            limits,
            selected_plan,
            purpose,
            retained_v62,
        } = request;
        limits.validate_original_members_v48()?;
        let (pool, deadline) = measured(self, cancel)?;
        let metadata_v62 = matches!(
            purpose,
            MetadataMembersReadPurposeV48::MetadataEligibleV62
                | MetadataMembersReadPurposeV48::MetadataInspectionV62
        );
        let mut metadata_heap_v62 = if metadata_v62 {
            let existing = match (selected_plan, retained_v62) {
                (Some(plan), Some(prior))
                    if prior.command() == command
                        && prior.issuer.matches(self)
                        && prior.identity().host == identity.host
                        && prior.identity().account_id == identity.account_id
                        && intent.matches_observed_page_v13(prior.parent().summary()) =>
                {
                    prior
                        .retained_model_v62()?
                        .checked_add(plan.retained_plan_bytes_v62().ok_or_else(|| {
                            refused(
                                GithubErrorKind::Transport,
                                "Actual member plan debit overflow",
                            )
                        })?)
                        .ok_or_else(|| {
                            refused(
                                GithubErrorKind::Transport,
                                "Actual member inherited debit overflow",
                            )
                        })?
                }
                (None, None) => 0,
                _ => {
                    return Err(refused(
                        GithubErrorKind::InvalidTarget,
                        "Actual Metadata member retained producer differs",
                    ))
                }
            };
            Some(MetadataMembersHeapV62::new(existing)?)
        } else {
            if retained_v62.is_some() {
                return Err(refused(
                    GithubErrorKind::InvalidTarget,
                    "Ordinary Members cannot adopt Metadata retained proof",
                ));
            }
            None
        };
        if identity.host != self.host
            || identity.account_id != intent.identity().account_id
            || identity
                .account_id
                .is_none_or(|id| !(1..=i64::MAX as u64).contains(&id))
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Original positive numeric member actor changed",
            ));
        }
        let repository_observation = current.repository();
        let caller_can_mutate = repository_observation.archived == Some(false)
            && [
                repository_observation.permissions.push,
                repository_observation.permissions.maintain,
                repository_observation.permissions.admin,
            ]
            .contains(&Some(true));
        let from = observed_utc()?;
        let runner_generation = self
            .runner
            .review_resource_generation_v11(
                &self.resource(&repository_observation.full_name, intent.number()),
                cancel,
                deadline,
            )
            .map_err(GithubError::from)?;
        let opening = match metadata_heap_v62.as_mut() {
            Some(heap) => {
                self.metadata_members_bounded_parent_v62(current, intent, heap, cancel)?
            }
            None => self.metadata_members_parent_v48(current, intent, cancel)?,
        };
        let parent = Arc::clone(opening.parent());
        let repository_raw = &opening.raw()["base"]["repo"];
        let repository = repository(
            &self.host,
            &repository_observation.full_name,
            repository_raw,
        )?;
        if repository.id != repository_observation.id
            || repository_observation.node_id.as_deref() != Some(repository.node_id.as_str())
        {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Actual Core and raw PR repository identities disagree",
            ));
        }
        let issue = if let Some(heap) = metadata_heap_v62.as_mut() {
            bounded_read_v62::issue(self, &parent, heap, cancel)?
        } else {
            self.api(
                &format!("repos/{}/issues/{}", repository.full_name, intent.number()),
                "GET",
                None,
                None,
                false,
                cancel,
            )?
        };
        issue_identity(self, parent.summary(), &issue)?;
        let mut accounting = PrMetadataReadAccounting {
            requests: 0,
            successful_stdout_bytes: 0,
            successful_diagnostic_bytes: 0,
            failed_reserved_bytes: 0,
        };
        let (rows, mut coverage) = if matches!(
            purpose,
            MetadataMembersReadPurposeV48::Eligible
                | MetadataMembersReadPurposeV48::MetadataEligibleV62
        ) {
            collect_reader_v48(
                self,
                &mut CataloguePageReaderV48::Original(MemberPageCaptureV48 {
                    limits,
                    deadline,
                    accounting: &mut accounting,
                    exhausted: false,
                    metadata_heap_v62: metadata_heap_v62.as_mut(),
                }),
                &format!("repos/{}", repository.full_name),
                command.catalogue(),
                limits,
                cancel,
            )?
        } else {
            if selected_plan.is_some() {
                return Err(refused(
                    GithubErrorKind::InvalidTarget,
                    "Read-only member inspection cannot select a new candidate",
                ));
            }
            (Vec::new(), MetricsReadCoverage {
                enumeration_complete: false,
                metric_evidence_complete: true,
                complete_empty: false,
                requested_pages: 0,
                fetched_pages: 0,
                fetched_rows: 0,
                admitted_rows: 0,
                excluded_rows: 0,
                response_bytes: 0,
                reasons: vec!["Read-only current membership; no eligible catalogue requested or empty authority inferred".into()],
                status: MetricsReadStatus::Partial,
            })
        };
        let (target, mut context, reasons) =
            observed_context(self, parent.summary(), repository_raw, opening.raw(), issue)?;
        // The embedded REST base.repo may omit requester permissions. Preserve
        // its raw bytes unchanged; marshal the actual Core's typed observation
        // into the display/choice DTO instead of inventing raw JSON fields.
        context.repository.permissions.pull = repository_observation.permissions.pull == Some(true);
        context.repository.permissions.push = repository_observation.permissions.push == Some(true);
        context.repository.permissions.maintain =
            repository_observation.permissions.maintain == Some(true);
        context.repository.permissions.admin =
            repository_observation.permissions.admin == Some(true);
        context.repository.archived = repository_observation.archived != Some(false);
        crate::pr_metadata_choices::members_context_capacity_v48(&context).ok_or_else(|| {
            refused(
                GithubErrorKind::InvalidTarget,
                "Member context retained capacity exceeds existing bounded profile",
            )
        })?;
        coverage.reasons.extend(reasons);
        coverage.metric_evidence_complete &= coverage.reasons.is_empty();
        finish(&mut coverage);
        let through = observed_utc()?;
        if through < from {
            return Err(refused(
                GithubErrorKind::Transport,
                "Metadata observation UTC moved backwards",
            ));
        }
        let read = Arc::new(PrMetadataCatalogueRead {
            identity: identity.clone(),
            target,
            context,
            kind: command.catalogue(),
            rows,
            coverage,
            repository,
            parent_raw: opening.raw().clone(),
            repository_raw: repository_raw.clone(),
            observed_interval: (from, through),
            accounting,
        });
        let expected =
            current_members(&read, command).map_err(|e| error(GithubErrorKind::Json, e))?;
        let preserved = opposite_members(&read, command)?;
        let original_scope = crate::storage::Scope::github(&identity.host, identity.account_id)
            .map_err(|_| {
                refused(
                    GithubErrorKind::IdentityChanged,
                    "Original member scope unavailable",
                )
            })?;
        if selected_plan.is_some_and(|plan| {
            plan.command() != command
                || plan.pr_id() != parent.database_id()
                || plan.repository_id() != parent.repository_id()
                || plan.original().pr.id != parent.summary().pr.id
                || plan.target().scope() != &original_scope
        }) {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Selected member plan belongs to another original purpose/parent",
            ));
        }
        let selected = selected_plan.and_then(PrMetadataPlan::selected);
        let mut request = members_query_v48(command, selected)?;
        request["variables"]["id"] = json!(read.target().pr.id);
        let raw = if let Some(heap) = metadata_heap_v62.as_mut() {
            bounded_read_v62::current(self, &parent, command, selected, heap, cancel)?
        } else {
            self.api("graphql", "POST", Some(request), None, false, cancel)?
        };
        error_free(&raw)?;
        members_validation_v48::validate_complete_v48(
            &read, command, selected, &expected, &preserved, &raw,
        )?;
        // The validator above has already bound every Team node, slug and
        // organization to this actual repository. Retain bounded identity data
        // only; this is not a source/capability constructor or transport handle.
        let mut team_namespaces = std::collections::BTreeMap::new();
        if matches!(command.field(), "reviewers" | "teams") {
            for row in raw["data"]["node"]["reviewRequests"]["nodes"]
                .as_array()
                .ok_or_else(|| {
                    refused(
                        GithubErrorKind::Json,
                        "Complete actual review requests missing",
                    )
                })?
            {
                let team = &row["requestedReviewer"];
                if team["__typename"].as_str() == Some("Team") {
                    let node = team["id"].as_str().ok_or_else(|| {
                        refused(GithubErrorKind::Json, "Actual Team node missing")
                    })?;
                    let organization = team["organization"]["login"].as_str().ok_or_else(|| {
                        refused(GithubErrorKind::Json, "Actual Team organization missing")
                    })?;
                    if node.len() > 1024 || organization.len() > 256 {
                        return Err(refused(
                            GithubErrorKind::InvalidTarget,
                            "Actual Team identity bound exceeded",
                        ));
                    }
                    team_namespaces.insert(node.to_owned(), organization.to_ascii_lowercase());
                }
            }
            if command == PrMetadataCommand::AddTeam && selected.is_some() {
                let team = &raw["data"]["candidate"];
                let node = team["id"]
                    .as_str()
                    .ok_or_else(|| refused(GithubErrorKind::Json, "Selected Team node missing"))?;
                let organization = team["organization"]["login"].as_str().ok_or_else(|| {
                    refused(GithubErrorKind::Json, "Selected Team organization missing")
                })?;
                if node.len() > 1024 || organization.len() > 256 {
                    return Err(refused(
                        GithubErrorKind::InvalidTarget,
                        "Selected Team identity bound exceeded",
                    ));
                }
                team_namespaces.insert(node.to_owned(), organization.to_ascii_lowercase());
            }
        }
        measured(self, cancel)?;
        Ok(OriginalMetadataMembersV48 {
            parent,
            read,
            command,
            issuer: ThreadPhaseIssuerV16::capture(self)?,
            pool,
            cancellation: cancel.clone(),
            deadline,
            runner_generation,
            caller_can_mutate,
            limits: limits.clone(),
            team_namespaces,
            purpose,
            model_bound_v62: metadata_heap_v62
                .map(MetadataMembersHeapV62::finish_original_read_v80)
                .transpose()?
                .unwrap_or(0),
        })
    }
    pub(crate) fn close_original_metadata_members_v48(
        &self,
        actual: &OriginalMetadataMembersV48,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        actual.validate_lineage(self, cancel, actual.deadline)?;
        measured(self, cancel)?;
        let closing = if actual.metadata_tab_v62() {
            let mut heap = MetadataMembersHeapV62::new(actual.retained_model_v62()?)?;
            self.metadata_members_bounded_parent_v62(current, intent, &mut heap, cancel)?
        } else {
            self.metadata_members_parent_v48(current, intent, cancel)?
        };
        if !GithubProvider::same_thread_history_parent_v1(actual.parent(), closing.parent()) {
            return Err(refused(
                GithubErrorKind::IdentityChanged,
                "Full original member parent changed at close",
            ));
        }
        require_same_context(
            actual.read.parent_raw(),
            closing.raw(),
            &actual.read.context().issue_metadata,
        )
    }
    /// Post-close check only; a successfully closed pool is deliberately
    /// aborted and must not be revived to make this allocation check succeed.
    pub(crate) fn validate_original_members_generation_v48(
        &self,
        actual: &OriginalMetadataMembersV48,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        actual.validate_lineage(self, cancel, deadline)
    }
}
