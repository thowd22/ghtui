//! Close/Reopen facts on the SAME original Core/measured phase. No legacy
//! Metrics origin, DTO constructor, reusable transport or permission escape.
use super::*;
use crate::{pr_state_model::PrStateCommand, providers::runner::ScopedReadBudgetV1};
use std::{
    sync::{Arc, Weak},
    time::Instant,
};
mod publication;
pub(in crate::providers::gh) use publication::PreparedStateRequestV68;
pub(crate) use publication::StatePublishInputV68;
pub(super) use publication::{stable_state_parent_v68, StateReceiptExpectationV68};
pub(crate) use publication::{StatePublicationFailureV68, StateReceiptV68, StateTransitionV68};

pub(crate) const STATE_POST_CAPS_V68: [usize; 12] = [
    512 * 1024,
    512 * 1024,
    64 * 1024,
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

impl GithubProvider {
    /// NEW explicit Core/read pool only. The actual typed terminal supplies
    /// expected post-state; its OLD paid token/deadline is never revived.
    pub(crate) fn pr_state_refresh_summary_v68(
        &self,
        current: &CurrentCatalogueRepository,
        own: &Arc<crate::storage::conversation_refresh_v19::ConversationOwnFenceV19>,
        attempt: Option<
            &Arc<crate::storage::conversation_refresh_v19::PersonalConversationReadAttemptV20>,
        >,
        cancel: &CancellationToken,
    ) -> GithubResult<ConversationRawSummaryV19> {
        let (pool, deadline) = measured(self, cancel)?;
        let transition = own
            .terminal()
            .state_transition_v68()
            .ok_or_else(|| failure(GithubErrorKind::InvalidTarget))?;
        if !Arc::ptr_eq(own.terminal().parent(), transition.parent())
            || !parse::same_parent(own.source().parent(), transition.old())
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        match (attempt, own.is_personal_v20()) {
            (Some(attempt), true) if attempt.matches_own(own) => attempt
                .matches_phase(cancel, deadline)
                .map_err(|_| failure(GithubErrorKind::IdentityChanged))?,
            (None, false) => {}
            _ => return Err(failure(GithubErrorKind::IdentityChanged)),
        }
        own.terminal()
            .validate_file_v19()
            .map_err(|_| failure(GithubErrorKind::IdentityChanged))?;
        let expected = transition.parent();
        let raw = document(
            self,
            &format!(
                "repos/{}/pulls/{}",
                expected.repository(),
                expected.number()
            ),
            128 * 1024,
            cancel,
            true,
        )?;
        let parent = parse::parent_expected_v13(
            self,
            current,
            ExpectedPrParentV13::ConfirmedStateV68(transition),
            &raw,
        )?;
        if !parse::same_parent(expected, &parent) {
            return Err(failure(GithubErrorKind::StaleHead));
        }
        let summary = ConversationRawSummaryV19::from_actual_raw_v19(self, parent, &raw)?;
        pool.check_clock(cancel).map_err(GithubError::from)?;
        Ok(summary)
    }
}

fn failure(kind: GithubErrorKind) -> GithubError {
    error(
        kind,
        "Original PR lifecycle/source unavailable; no fallback or resend",
    )
}
fn supported(command: PrStateCommand) -> GithubResult<()> {
    if matches!(command, PrStateCommand::Close | PrStateCommand::Reopen) {
        Ok(())
    } else {
        Err(failure(GithubErrorKind::Unsupported))
    }
}
fn measured(
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> GithubResult<(Arc<ScopedReadBudgetV1>, Instant)> {
    let pool = provider
        .scoped_read_budget
        .as_ref()
        .ok_or_else(|| failure(GithubErrorKind::InvalidTarget))?;
    let deadline = provider
        .scoped_read_window
        .ok_or_else(|| failure(GithubErrorKind::InvalidTarget))?
        .0
        .min(pool.deadline());
    pool.check_clock(cancel).map_err(GithubError::from)?;
    if !pool.original_token_matches_v63(cancel)
        || pool.accounting().map_err(GithubError::from)?.sealed()
        || cancel.is_cancelled()
        || Instant::now() >= deadline
    {
        return Err(failure(GithubErrorKind::Transport));
    }
    Ok((Arc::clone(pool), deadline))
}

/// Missing and explicit null remain distinct display facts. Neither grants a
/// capability; only Observed(true) can prepare this exact command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StateCapabilityV68 {
    Observed(bool),
    Null,
    Missing,
}
impl StateCapabilityV68 {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Observed(true) => "Allowed by actual command capability",
            Self::Observed(false) => "Denied by actual command capability",
            Self::Null => "Command capability unavailable (null)",
            Self::Missing => "Command capability unavailable (not emitted)",
        }
    }
    fn read(raw: Option<&Value>) -> GithubResult<Self> {
        match raw {
            Some(Value::Bool(v)) => Ok(Self::Observed(*v)),
            Some(Value::Null) => Ok(Self::Null),
            None => Ok(Self::Missing),
            _ => Err(failure(GithubErrorKind::Json)),
        }
    }
}
struct StateReadBindingV68 {
    issuer: ThreadPhaseIssuerV16,
    pool: Weak<ScopedReadBudgetV1>,
    cancel: CancellationToken,
    deadline: Instant,
}
impl StateReadBindingV68 {
    fn validate_lineage(
        &self,
        provider: &GithubProvider,
        pool: &Arc<ScopedReadBudgetV1>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        if !self.issuer.matches(provider)
            || !self.pool.ptr_eq(&Arc::downgrade(pool))
            || !self.cancel.same_instance(cancel)
            || !pool.original_token_matches_v63(cancel)
            || cancel.is_cancelled()
            || Instant::now() >= self.deadline.min(pool.deadline()).min(deadline)
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    fn validate(
        &self,
        provider: &GithubProvider,
        pool: &Arc<ScopedReadBudgetV1>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        self.validate_lineage(provider, pool, cancel, deadline)?;
        pool.check_clock(cancel).map_err(GithubError::from)
    }
}
/// Only the raw PR + independent REST issue factory below constructs this.
/// In particular issue.id/node_id are NOT aliases of PR.id/node_id.
pub(crate) struct PrStateReadV68 {
    parent: Arc<PrResourceParentV1>,
    issue_id: u64,
    issue_node: String,
    command: PrStateCommand,
    generation: u64,
    binding: StateReadBindingV68,
}
impl std::fmt::Debug for PrStateReadV68 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrStateReadV68(<actual PR/independent issue; no approval>)")
    }
}
impl PrStateReadV68 {
    pub(in crate::providers::gh) fn validate_capability_source_v87(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let (pool, deadline) = measured(provider, cancel)?;
        self.binding.validate(provider, &pool, cancel, deadline)
    }
    pub(in crate::providers::gh) fn validate_capability_raw_v87(
        &self,
        raw: &Value,
    ) -> GithubResult<StateCapabilityV68> {
        capability_node(self.parent(), raw, self.command)
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn issue_id(&self) -> u64 {
        self.issue_id
    }
    pub(crate) fn issue_node(&self) -> &str {
        &self.issue_node
    }
    pub(crate) fn command(&self) -> PrStateCommand {
        self.command
    }
}
pub(crate) struct OriginalPrStateV68 {
    observed: Arc<PrStateReadV68>,
    identity: HostIdentity,
    capability: StateCapabilityV68,
    pool: Arc<ScopedReadBudgetV1>,
    cancel: CancellationToken,
    deadline: Instant,
}
/// Point-in-time READ observation on the SAME original phase. Capability and
/// historical state are facts only; this cannot invoke or issue a source.
pub(crate) struct StateRecoveryCurrentV68 {
    actual: Arc<OriginalPrStateV68>,
}
impl StateRecoveryCurrentV68 {
    pub(crate) fn actual(&self) -> &Arc<OriginalPrStateV68> {
        &self.actual
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.actual.publication_cutoff_v68()
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.actual.cancel
    }
    pub(crate) fn matches_target(&self, target: &crate::pr_write_journal::PrWriteTarget) -> bool {
        let parent = self.actual.observed.parent();
        crate::storage::Scope::github(&self.actual.identity.host, self.actual.identity.account_id)
            .ok()
            .as_ref()
            == Some(target.scope())
            && target.repository() == parent.repository()
            && target.node_id() == parent.node_id()
            && target.number() == parent.number()
    }
}
impl std::fmt::Debug for OriginalPrStateV68 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalPrStateV68(<same actual phase/capability; no approval>)")
    }
}
impl OriginalPrStateV68 {
    pub(crate) fn validate_repair_context_v74(
        &self,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        if !self.cancel.same_instance(cancel)
            || !self.observed.binding.cancel.same_instance(cancel)
            || !self.pool.original_token_matches_v63(cancel)
            || self
                .observed
                .binding
                .pool
                .upgrade()
                .is_none_or(|p| !Arc::ptr_eq(&p, &self.pool))
            || cancel.is_cancelled()
            || Instant::now() >= deadline.min(self.deadline).min(self.pool.deadline())
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    pub(crate) fn observed(&self) -> &Arc<PrStateReadV68> {
        &self.observed
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn capability(&self) -> StateCapabilityV68 {
        self.capability
    }
    pub(crate) fn publication_cutoff_v68(&self) -> Instant {
        self.deadline.min(self.pool.deadline())
    }
    pub(crate) fn validate_live_source_v68(&self) -> GithubResult<()> {
        if !self
            .observed
            .binding
            .pool
            .ptr_eq(&Arc::downgrade(&self.pool))
            || !self.observed.binding.cancel.same_instance(&self.cancel)
            || self.cancel.is_cancelled()
            || Instant::now() >= self.publication_cutoff_v68()
            || self.pool.accounting().map_err(GithubError::from)?.sealed()
        {
            return Err(failure(GithubErrorKind::Transport));
        }
        self.pool
            .check_clock(&self.cancel)
            .map_err(GithubError::from)
    }
    pub(crate) fn validate_with_v68(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.observed
            .binding
            .validate(provider, &self.pool, cancel, self.deadline)
    }
    pub(crate) fn require(&self) -> GithubResult<()> {
        supported(self.observed.command)?;
        if self.capability != StateCapabilityV68::Observed(true) {
            return Err(failure(GithubErrorKind::Permission));
        }
        let parent = self.observed.parent();
        if parent.summary().merged
            || match self.observed.command {
                PrStateCommand::Close => parent.summary().pr.state != "open",
                PrStateCommand::Reopen => parent.summary().pr.state != "closed",
                _ => true,
            }
        {
            return Err(failure(GithubErrorKind::InvalidTarget));
        }
        Ok(())
    }
}

fn error_free(raw: &Value) -> GithubResult<()> {
    if raw
        .get("errors")
        .is_some_and(|v| !v.is_null() && !v.as_array().is_some_and(Vec::is_empty))
    {
        Err(failure(GithubErrorKind::Json))
    } else {
        Ok(())
    }
}
fn capability_node(
    parent: &PrResourceParentV1,
    raw: &Value,
    command: PrStateCommand,
) -> GithubResult<StateCapabilityV68> {
    error_free(raw)?;
    let node = &raw["data"]["node"];
    if node["__typename"] != "PullRequest"
        || node["id"].as_str() != Some(parent.node_id())
        || crate::providers::gh::pr::canonical_database_id_v87(&node["fullDatabaseId"])
            != Some(parent.database_id())
        || node["number"].as_u64() != Some(parent.number())
        || node["repository"]["id"].as_str() != Some(parent.repository_node())
        || node["repository"]["nameWithOwner"]
            .as_str()
            .is_none_or(|s| !s.eq_ignore_ascii_case(parent.repository()))
        || node["headRefOid"].as_str() != Some(parent.head_sha())
        || node["baseRefOid"].as_str() != Some(parent.base_tip_sha())
        || node["headRefName"].as_str() != Some(parent.summary().head_ref.as_str())
        || node["baseRefName"].as_str() != Some(parent.summary().base_ref.as_str())
        || node["state"].as_str()
            != Some(if parent.summary().merged {
                "MERGED"
            } else if parent.summary().pr.state == "open" {
                "OPEN"
            } else {
                "CLOSED"
            })
        || node["isDraft"].as_bool() != Some(parent.summary().pr.draft)
        || node["merged"].as_bool() != Some(parent.summary().merged)
    {
        return Err(failure(GithubErrorKind::IdentityChanged));
    }
    StateCapabilityV68::read(node.get(match command {
        PrStateCommand::Close => "viewerCanClose",
        PrStateCommand::Reopen => "viewerCanReopen",
        _ => return Err(failure(GithubErrorKind::Unsupported)),
    }))
}

impl GithubProvider {
    /// Only an actual in-progress State read can issue the observation. The
    /// generation was captured BEFORE its first raw read, not after recovery.
    pub(crate) fn inspect_original_state_v68(
        &self,
        actual: Arc<OriginalPrStateV68>,
        cancel: &CancellationToken,
    ) -> GithubResult<StateRecoveryCurrentV68> {
        actual.validate_live_source_v68()?;
        actual.validate_with_v68(self, cancel)?;
        let current = StateRecoveryCurrentV68 { actual };
        self.validate_state_inspection_v68(&current, cancel, current.deadline())?;
        Ok(current)
    }
    pub(crate) fn validate_state_inspection_v68(
        &self,
        current: &StateRecoveryCurrentV68,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        self.validate_original_state_generation_v68(&current.actual, cancel, deadline)
    }
    pub(crate) fn reconcile_state_inspection_v68(
        &self,
        current: StateRecoveryCurrentV68,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        self.validate_state_inspection_v68(&current, cancel, deadline)?;
        let parent = current.actual.observed.parent();
        self.runner
            .reconcile_review_generation_v11(
                &self.resource(parent.repository(), parent.number()),
                current.actual.observed.generation,
                cancel,
                deadline.min(current.deadline()),
            )
            .map_err(GithubError::from)
    }
    /// Restriction-only SAME original issuer/token/resource generation. The
    /// successful Core close seals its pool; this does not reopen that pool.
    pub(crate) fn validate_original_state_generation_v68(
        &self,
        actual: &OriginalPrStateV68,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        if !actual.cancel.same_instance(cancel) {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        let deadline = deadline.min(actual.publication_cutoff_v68());
        actual
            .observed
            .binding
            .validate_lineage(self, &actual.pool, cancel, deadline)?;
        let parent = actual.observed.parent();
        let generation = self
            .runner
            .review_resource_generation_v11(
                &self.resource(parent.repository(), parent.number()),
                cancel,
                deadline,
            )
            .map_err(GithubError::from)?;
        if generation != actual.observed.generation {
            return Err(failure(GithubErrorKind::StaleHead));
        }
        Ok(())
    }
    /// EXACT first two of the four State measured roles. The caller's original
    /// Core/current repository is mandatory; the closing PR is a separate role.
    pub(crate) fn pr_original_state_read_v68(
        &self,
        current: &CurrentCatalogueRepository,
        intent: &PrParentIntentV13,
        command: PrStateCommand,
        cancel: &CancellationToken,
    ) -> GithubResult<Arc<PrStateReadV68>> {
        supported(command)?;
        let (pool, deadline) = measured(self, cancel)?;
        let issuer = ThreadPhaseIssuerV16::capture(self)?;
        let generation = self
            .runner
            .review_resource_generation_v11(
                &self.resource(&current.repository().full_name, intent.number()),
                cancel,
                deadline,
            )
            .map_err(GithubError::from)?;
        let (parent, raw) = self.original_detail_parent_raw_v19(current, intent, cancel)?;
        let issue_endpoint = format!("repos/{}/issues/{}", parent.repository(), parent.number());
        exact_source(self, &raw, "issue_url", &issue_endpoint, true, true)?;
        let issue = document(self, &issue_endpoint, 128 * 1024, cancel, false)?;
        let issue_id = positive(&issue, "id")?;
        let issue_node = actual_node(&issue, "node_id")?;
        if issue["number"].as_u64() != Some(parent.number())
            || issue["state"].as_str()
                != Some(if parent.summary().merged {
                    "closed"
                } else {
                    &parent.summary().pr.state
                })
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        exact_source(self, &issue, "url", &issue_endpoint, true, true)?;
        exact_source(
            self,
            &issue,
            "html_url",
            &format!("{}/pull/{}", parent.repository(), parent.number()),
            false,
            true,
        )?;
        exact_source(
            self,
            &issue,
            "repository_url",
            &format!("repos/{}", parent.repository()),
            true,
            true,
        )?;
        exact_source(
            self,
            &issue["pull_request"],
            "url",
            &format!("repos/{}/pulls/{}", parent.repository(), parent.number()),
            true,
            true,
        )?;
        measured(self, cancel)?;
        Ok(Arc::new(PrStateReadV68 {
            parent: Arc::new(parent),
            issue_id,
            issue_node,
            command,
            generation,
            binding: StateReadBindingV68 {
                issuer,
                pool: Arc::downgrade(&pool),
                cancel: cancel.clone(),
                deadline,
            },
        }))
    }
    /// ONE error-free fixed GraphQL role. A false/null/missing selected
    /// capability remains read data; require() alone makes the typed refusal.
    pub(crate) fn observe_original_state_v68(
        &self,
        observed: Arc<PrStateReadV68>,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> GithubResult<OriginalPrStateV68> {
        let (pool, deadline) = measured(self, cancel)?;
        observed.binding.validate(self, &pool, cancel, deadline)?;
        if identity.host != self.host
            || observed.parent.summary().pr.host != self.host
            || identity
                .account_id
                .is_none_or(|id| !(1..=i64::MAX as u64).contains(&id))
        {
            return Err(failure(GithubErrorKind::IdentityChanged));
        }
        checked(self, cancel, 128 * 1024, 128 * 1024)?;
        let capability = self.original_state_capability_v87(&observed, cancel)?;
        measured(self, cancel)?;
        Ok(OriginalPrStateV68 {
            observed,
            identity: identity.clone(),
            capability,
            pool,
            cancel: cancel.clone(),
            deadline,
        })
    }
}
