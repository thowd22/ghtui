//! Manual development links, issued only by the SAME original PR Core.
//! None of the private wire/domain types below is a Source or a permission.
use super::*;
use crate::pr_issue_links_v87::IssueLinkCommandV87;
use crate::providers::runner::ScopedReadBudgetV1;
use sha2::{Digest, Sha256};
use std::{
    sync::{Arc, Weak},
    time::Instant,
};

pub(in crate::providers::gh) mod bounded;
mod manual_v94;
mod protocol;
mod publication;
mod recovery;
pub(crate) use manual_v94::IssueLinkManualReadV94;
pub(crate) use recovery::IssueLinkRecoveryCurrentV87;
pub(in crate::providers::gh) mod refresh;
pub(crate) use publication::{
    IssueLinkPublicationFailureV87, IssueLinkReceiptV87, IssueLinkWriteV87,
};
pub(crate) mod read;
use super::pr_resources_v1::IssueLinkParentV87;
#[cfg(test)]
mod tests;
use bounded::IssueLinkHeapV87;
use protocol::{
    IssueLinkCapabilityV87, IssueLinkPairV87, IssueLinkRepositoryV87, IssueLinkTargetV87,
};

const KIB: usize = 1024;
const MODEL_CAP_V87: usize = 4 * 1024 * 1024;
const PAIR_CAP_V87: usize = 512 * KIB;
const SCALAR_CAP_V87: usize = 128 * KIB;
#[cfg(test)]
pub(crate) const ISSUE_LINK_PRE_CAPS_V87: [usize; 14] = [
    16 * KIB,
    64 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    512 * KIB,
    128 * KIB,
    128 * KIB,
    512 * KIB,
    128 * KIB,
    128 * KIB,
    16 * KIB,
    64 * KIB,
];
pub(crate) const ISSUE_LINK_METADATA_POST_CAPS_V87: [usize; 19] = [
    128 * KIB,
    512 * KIB,
    512 * KIB,
    64 * KIB,
    128 * KIB,
    128 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    512 * KIB,
    128 * KIB,
    64 * KIB,
];
pub(crate) const ISSUE_LINK_POST_CAPS_V87: [usize; 15] = [
    128 * KIB,
    512 * KIB,
    512 * KIB,
    64 * KIB,
    128 * KIB,
    128 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    128 * KIB,
    512 * KIB,
    128 * KIB,
    64 * KIB,
];
pub(crate) const ISSUE_LINK_METADATA_REFRESH_CAPS_V87: [usize; 23] = [
    16 * KIB,
    64 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    512 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    16 * KIB,
    64 * KIB,
];
pub(crate) const ISSUE_LINK_REFRESH_CAPS_V87: [usize; 19] = [
    16 * KIB,
    64 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    512 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    128 * KIB,
    16 * KIB,
    64 * KIB,
];

fn refused(kind: GithubErrorKind) -> GithubError {
    error(
        kind,
        "Original manual Issue-link observation unavailable; no substituted target or resend",
    )
}

fn alive(cancel: &CancellationToken, deadline: Instant) -> GithubResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(refused(GithubErrorKind::Transport));
    }
    Ok(())
}

/// Actual measured lineage, not an exported read-window or provider accessor.
struct IssueLinkBindingV87 {
    issuer: ThreadPhaseIssuerV16,
    measured: Arc<ScopedReadBudgetV1>,
    cancellation: CancellationToken,
    deadline: Instant,
    retained: usize,
}
fn issuer_layout_v87(provider: &GithubProvider) -> GithubResult<usize> {
    use std::os::unix::ffi::OsStrExt;
    let program = provider.program.as_bytes().len();
    let bytes = provider
        .env
        .iter()
        .try_fold(0usize, |sum, (key, value)| {
            sum.checked_add(key.as_bytes().len())?
                .checked_add(value.as_bytes().len())
        })
        .ok_or_else(|| refused(GithubErrorKind::Transport))?;
    if program > 4096 || provider.host.len() > 256 || provider.env.len() > 128 || bytes > 64 * 1024
    {
        return Err(refused(GithubErrorKind::Transport));
    }
    // Map nodes + key/value OsString headers and two byte copies/relocation;
    // explicit program/host copies, scalar issuer layout and Arc handles. This
    // is a requested-layout/lifetime bound, never an allocator/RSS assertion.
    provider
        .env
        .len()
        .checked_mul(512 + 2 * std::mem::size_of::<std::ffi::OsString>())
        .and_then(|n| n.checked_add(2 * bytes))
        .and_then(|n| {
            n.checked_add(
                2 * program
                    + 2 * provider.host.len()
                    + std::mem::size_of::<ThreadPhaseIssuerV16>() * 2
                    + 1024,
            )
        })
        .ok_or_else(|| refused(GithubErrorKind::Transport))
}
fn identity_bound_v87(identity: &HostIdentity) -> GithubResult<()> {
    if identity.host.len() > 256
        || identity.login.len() > 100
        || identity.gh_version.len() > 256
        || [&identity.host, &identity.login, &identity.gh_version]
            .into_iter()
            .any(|s| s.chars().any(char::is_control))
        || identity
            .account_id
            .is_none_or(|n| n == 0 || n > i64::MAX as u64)
    {
        return Err(refused(GithubErrorKind::IdentityChanged));
    }
    Ok(())
}
impl IssueLinkBindingV87 {
    fn capture(
        provider: &GithubProvider,
        cancel: &CancellationToken,
        heap: &mut IssueLinkHeapV87,
    ) -> GithubResult<Self> {
        let pool = provider
            .scoped_read_budget
            .as_ref()
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?;
        let deadline = provider
            .scoped_read_window
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?
            .0
            .min(pool.deadline());
        alive(cancel, deadline)?;
        if !pool.original_token_matches_v63(cancel)
            || pool.accounting().map_err(GithubError::from)?.sealed()
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        pool.check_clock(cancel).map_err(GithubError::from)?;
        let retained = issuer_layout_v87(provider)?
            .checked_add(2 * std::mem::size_of::<Self>())
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        heap.charge(retained)?;
        Ok(Self {
            issuer: ThreadPhaseIssuerV16::capture(provider)?,
            measured: Arc::clone(pool),
            cancellation: cancel.clone(),
            deadline,
            retained,
        })
    }
    fn validate_lineage(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        alive(
            cancel,
            deadline.min(self.deadline).min(self.measured.deadline()),
        )?;
        if !self.issuer.matches(provider)
            || !self.cancellation.same_instance(cancel)
            || !self.measured.original_token_matches_v63(cancel)
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    fn validate_live(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.validate_lineage(provider, cancel, self.deadline)?;
        if provider
            .scoped_read_budget
            .as_ref()
            .is_none_or(|p| !Arc::ptr_eq(p, &self.measured))
            || self
                .measured
                .accounting()
                .map_err(GithubError::from)?
                .sealed()
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        self.measured.check_clock(cancel).map_err(GithubError::from)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum IssueLinkReadPurposeV87 {
    Eligible,
    Inspection,
    MetadataEligibleV87,
    MetadataInspectionV87,
}
// Non-Clone opaque markers. Only the exact provider predicate/schema branch
// below can construct them; neither is a source, receipt or permission.
pub(crate) struct IssueNoChangeV102 {
    _private: (),
}
pub(crate) struct IssueHostUnsupportedV102 {
    _private: (),
}
pub(crate) enum IssueRequestedV102 {
    Plan(IssueLinkPlanV87),
    NoChange(IssueNoChangeV102),
}
pub(crate) enum IssueCatalogueResultV102 {
    Ready(Arc<IssueLinkCatalogueV87>),
    Unsupported(IssueHostUnsupportedV102),
}
impl IssueLinkReadPurposeV87 {
    fn eligible(self) -> bool {
        matches!(self, Self::Eligible | Self::MetadataEligibleV87)
    }
    fn metadata(self) -> bool {
        matches!(
            self,
            Self::MetadataEligibleV87 | Self::MetadataInspectionV87
        )
    }
}

/// Constructible only by read::observe_selected on the actual measured Core.
pub(crate) struct OriginalIssueLinksV87 {
    opening: IssueLinkParentV87,
    parent: Arc<PrResourceParentV1>,
    identity: HostIdentity,
    command: IssueLinkCommandV87,
    pair: IssueLinkPairV87,
    capability: IssueLinkCapabilityV87,
    binding: IssueLinkBindingV87,
    resources: [String; 2],
    generations: [u64; 2],
    purpose: IssueLinkReadPurposeV87,
    retained: usize,
}
impl std::fmt::Debug for OriginalIssueLinksV87 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("OriginalIssueLinksV87(<actual closed measured pair; no approval>)")
    }
}

pub(crate) struct IssueLinkPlanV87 {
    actual: Weak<OriginalIssueLinksV87>,
    command: IssueLinkCommandV87,
    digest: String,
}
/// Borrowed, current producer facts for the closed journal adapter. This is
/// neither a Source nor an approval; the only constructor revalidates the
/// actual live pool and exact plan before yielding these references.
pub(crate) struct IssueLinkOriginFactsV87<'a> {
    pub(crate) identity: &'a HostIdentity,
    pub(crate) parent: &'a Arc<PrResourceParentV1>,
    pub(crate) command: IssueLinkCommandV87,
    pub(crate) issue_id: u64,
    pub(crate) issue_node: &'a str,
    pub(crate) issue_number: u64,
    pub(crate) issue_repository_id: u64,
    pub(crate) issue_repository_node: &'a str,
    pub(crate) issue_repository: &'a str,
    pub(crate) plan_digest: &'a str,
    pub(crate) body_null: bool,
    pub(crate) body_digest: &'a str,
}
impl std::fmt::Debug for IssueLinkPlanV87 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IssueLinkPlanV87(<exact actual pair and one command>)")
    }
}
impl OriginalIssueLinksV87 {
    pub(crate) fn prepare_preview_lines_v87(
        self: &Arc<Self>,
        plan: &IssueLinkPlanV87,
    ) -> GithubResult<Arc<[String]>> {
        self.validate_plan(plan)?;
        self.retained
            .checked_add(128 * KIB)
            .filter(|n| *n <= MODEL_CAP_V87)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        let issue = &self.pair.issue;
        Ok(vec![
            format!("{} • ONE explicit two-sided development-link mutation",self.command.label()),
            format!("Original PR: {}#{} • node {} / numeric {}",self.parent.repository(),self.parent.number(),self.parent.node_id(),self.parent.database_id()),
            format!("Selected Issue: {}#{} • node {} / numeric {}",issue.repository.name,issue.number,issue.node,issue.id),
            format!("Issue title: {}",crate::app::safe_text(&issue.title)),
            format!("Issue source: {}",issue.url),
            format!("PR head: {}",self.parent.head_sha()),
            format!("PR base: {}",self.parent.base_tip_sha()),
            format!("Exact requested plan: {}",plan.digest()),
            "Manual link only • not a body keyword edit, closing promise or Projects item".into(),
            "All six complete sets are independently checked; manual/nonmanual membership may overlap".into(),
            "Both PR and Issue generations are serialized and checked; existing unrelated links remain exact".into(),
            "One mutation + independent PR and Issue receipts • observations do not prove causation".into(),
            "No local Git mutation • unresolved outcome protects original reference; inspect, never resend".into(),
        ].into())
    }
    pub(crate) fn validate_publication_clone_v87(
        &self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        self.binding.validate_live(provider, cancel)?;
        self.retained
            .checked_add(issuer_layout_v87(provider)?)
            .filter(|bytes| *bytes <= MODEL_CAP_V87)
            .ok_or_else(|| refused(GithubErrorKind::Transport))?;
        Ok(())
    }
    pub(crate) fn validated_origin_facts_v87<'a>(
        self: &'a Arc<Self>,
        plan: &'a IssueLinkPlanV87,
    ) -> GithubResult<IssueLinkOriginFactsV87<'a>> {
        self.binding
            .measured
            .check_clock(&self.binding.cancellation)
            .map_err(GithubError::from)?;
        if self
            .binding
            .measured
            .accounting()
            .map_err(GithubError::from)?
            .sealed()
            || !self.purpose.eligible()
            || !self.pair.writable()
            || !self.capability.supported()
        {
            return Err(refused(GithubErrorKind::Permission));
        }
        self.validate_plan(plan)?;
        Ok(IssueLinkOriginFactsV87 {
            identity: &self.identity,
            parent: &self.parent,
            command: self.command,
            issue_id: self.pair.issue.id,
            issue_node: &self.pair.issue.node,
            issue_number: self.pair.issue.number,
            issue_repository_id: self.pair.issue.repository.id,
            issue_repository_node: &self.pair.issue.repository.node,
            issue_repository: &self.pair.issue.repository.name,
            plan_digest: &plan.digest,
            body_null: self.opening.body_null(),
            body_digest: self.opening.body_digest(),
        })
    }
    pub(crate) fn rebind_requested_v87(
        self: &Arc<Self>,
        accepted: &IssueLinkPlanV87,
    ) -> GithubResult<IssueLinkPlanV87> {
        let prior = accepted
            .actual
            .upgrade()
            .ok_or_else(|| refused(GithubErrorKind::IdentityChanged))?;
        if prior.command != self.command
            || !GithubProvider::same_thread_history_parent_v1(prior.parent(), self.parent())
            || !prior.pair.same_observation(&self.pair)
            || prior.opening.body_null() != self.opening.body_null()
            || prior.opening.body_digest() != self.opening.body_digest()
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let plan = self.prepare_requested()?;
        if plan.digest != accepted.digest {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok(plan)
    }
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(in crate::providers::gh) fn metadata_tab_v87(&self) -> bool {
        self.purpose.metadata()
    }
    pub(in crate::providers::gh) fn matches_raw_body_v87(&self, raw: &Value) -> bool {
        self.opening.matches_body(raw)
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.binding.deadline.min(self.binding.measured.deadline())
    }
    pub(crate) fn retained_bound_v87(&self) -> usize {
        self.retained
    }
    pub(crate) fn prepare_requested(self: &Arc<Self>) -> GithubResult<IssueLinkPlanV87> {
        self.validate_requested_live_v102()?;
        self.plan_for_change_v102()
    }
    fn validate_requested_live_v102(&self) -> GithubResult<()> {
        alive(&self.binding.cancellation, self.deadline())?;
        if !self.purpose.eligible()
            || !self.capability.supported()
            || !self.pair.writable()
            || self
                .binding
                .measured
                .accounting()
                .map_err(GithubError::from)?
                .sealed()
        {
            return Err(refused(GithubErrorKind::Permission));
        }
        self.binding
            .measured
            .check_clock(&self.binding.cancellation)
            .map_err(GithubError::from)?;
        Ok(())
    }
    fn plan_for_change_v102(self: &Arc<Self>) -> GithubResult<IssueLinkPlanV87> {
        self.pair.require_change(self.command)?;
        let digest = self
            .pair
            .plan_digest(self.command, &self.identity, &self.opening)?;
        Ok(IssueLinkPlanV87 {
            actual: Arc::downgrade(self),
            command: self.command,
            digest,
        })
    }
    pub(crate) fn prepare_requested_native_v102(
        self: &Arc<Self>,
    ) -> GithubResult<IssueRequestedV102> {
        // Permission and the SAME live measured lineage precede NoChange.
        self.validate_requested_live_v102()?;
        if self.pair.no_change_v102(self.command) {
            return Ok(IssueRequestedV102::NoChange(IssueNoChangeV102 {
                _private: (),
            }));
        }
        self.plan_for_change_v102().map(IssueRequestedV102::Plan)
    }
    fn validate_plan(self: &Arc<Self>, plan: &IssueLinkPlanV87) -> GithubResult<()> {
        if !plan.actual.ptr_eq(&Arc::downgrade(self))
            || plan.command != self.command
            || self
                .pair
                .plan_digest(self.command, &self.identity, &self.opening)?
                != plan.digest
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        self.pair.require_change(self.command)
    }
}
impl IssueLinkPlanV87 {
    pub(crate) fn command(&self) -> IssueLinkCommandV87 {
        self.command
    }
    pub(crate) fn digest(&self) -> &str {
        &self.digest
    }
}

/// UI rows are discovery only. This allocation's typed choose is the sole
/// constructor; a number, node, display row or saved value cannot issue one.
pub(crate) struct IssueLinkCatalogueV87 {
    opening: IssueLinkParentV87,
    parent: Arc<PrResourceParentV1>,
    identity: HostIdentity,
    repository: IssueLinkRepositoryV87,
    rows: Vec<IssueLinkTargetV87>,
    complete: bool,
    binding: IssueLinkBindingV87,
    retained: usize,
    command: IssueLinkCommandV87,
    capability: IssueLinkCapabilityV87,
    pr_sets: Option<protocol::IssueLinkPrSetsV87>,
    runner_generation: u64,
}
pub(crate) struct IssueLinkRepositoryChoiceV87 {
    catalogue: Weak<RepositoryCatalogueRead>,
    index: usize,
}
pub(crate) struct IssueLinkChoiceV87 {
    catalogue: Weak<IssueLinkCatalogueV87>,
    index: usize,
}
impl IssueLinkCatalogueV87 {
    pub(crate) fn parent(&self) -> &Arc<PrResourceParentV1> {
        &self.parent
    }
    pub(crate) fn choose(self: &Arc<Self>, index: usize) -> GithubResult<IssueLinkChoiceV87> {
        alive(&self.binding.cancellation, self.binding.deadline)?;
        let row = self
            .rows
            .get(index)
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?;
        if !self.repository.same_identity(&row.repository)
            || !self.capability.supported()
            || !self
                .pr_sets
                .as_ref()
                .is_some_and(|s| s.selectable(row, self.command))
        {
            return Err(refused(GithubErrorKind::Permission));
        }
        Ok(IssueLinkChoiceV87 {
            catalogue: Arc::downgrade(self),
            index,
        })
    }
    pub(crate) fn complete(&self) -> bool {
        self.complete
    }
    pub(crate) fn len(&self) -> usize {
        self.rows.len()
    }
    pub(crate) fn retained_bound_v87(&self) -> usize {
        self.retained
    }
    pub(crate) fn supported(&self) -> bool {
        self.capability.supported()
    }
    pub(crate) fn row_v87(&self, index: usize) -> Option<IssueLinkRowV87<'_>> {
        let row = self.rows.get(index)?;
        Some(IssueLinkRowV87 {
            id: row.id,
            node: &row.node,
            number: row.number,
            repository: &row.repository.name,
            repository_id: row.repository.id,
            url: &row.url,
            title: &row.title,
            state: &row.state,
            selected_observations: self.pr_sets.as_ref().map(|s| s.flags(row)),
            enabled: self.capability.supported()
                && self
                    .pr_sets
                    .as_ref()
                    .is_some_and(|s| s.selectable(row, self.command)),
        })
    }
}
/// Borrowed presentation facts only. No choice/plan can be reconstructed here.
pub(crate) struct IssueLinkRowV87<'a> {
    pub(crate) id: u64,
    pub(crate) node: &'a str,
    pub(crate) number: u64,
    pub(crate) repository: &'a str,
    pub(crate) repository_id: u64,
    pub(crate) url: &'a str,
    pub(crate) title: &'a str,
    pub(crate) state: &'a str,
    pub(crate) selected_observations: Option<[bool; 3]>,
    pub(crate) enabled: bool,
}

/// Only this provider can prepare/bind the fixed operation. The runner receives
/// a borrowed immutable request, never an arbitrary spec/resources callback.
pub(in crate::providers) struct PreparedIssueLinkRequestV87 {
    actual: Weak<OriginalIssueLinksV87>,
    runner: CommandRunner,
    issuer: ThreadPhaseIssuerV16,
    cancellation: CancellationToken,
    deadline: Instant,
    resources: [String; 2],
    generations: [u64; 2],
    bytes: Vec<u8>,
    paid: Option<CommandSpec>,
    retained: usize,
    wire: IssueLinkWireV87,
    forward_bytes: Vec<u8>,
    reverse_bytes: Vec<u8>,
    ticket: Arc<()>,
    successors: Option<[u64; 2]>,
    readbacks_validated: bool,
}
#[derive(Clone, Copy)]
pub(in crate::providers::gh) enum IssueLinkReadbackRoleV87 {
    Pr,
    Issue,
}
struct IssueLinkWireV87 {
    program: std::ffi::OsString,
    args: Vec<std::ffi::OsString>,
    env: std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    output_limit: usize,
    timeout: std::time::Duration,
}
type IssueLinkRunnerPartsV87<'a> = (&'a CommandSpec, &'a [String; 2], [u64; 2], Instant);
impl PreparedIssueLinkRequestV87 {
    pub(in crate::providers::gh) fn prepare(
        actual: &Arc<OriginalIssueLinksV87>,
        plan: &IssueLinkPlanV87,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<Self> {
        actual.binding.validate_live(provider, cancel)?;
        actual.validate_plan(plan)?;
        if !actual.purpose.eligible() || !actual.pair.writable() || !actual.capability.supported() {
            return Err(refused(GithubErrorKind::Permission));
        }
        let mut heap = IssueLinkHeapV87::new(actual.retained)?;
        // Actual binding issuer + prepared issuer + ordinary paid spec env,
        // bytes Vec, paid stdin copy, Value and encoding relocation overlap.
        // Existing issuer validation bounds each env at64KiB/128 entries.
        let issuer_debit = issuer_layout_v87(provider)?;
        // Five actual new config lifetimes: captured issuer, immutable wire
        // binding, retained mutation spec, one paid readback spec and child
        // CString env/argv setup. Raw2x/Map nodes are included
        // by issuer_layout; fixed args/input and their layouts are EXTRA.
        heap.charge(
            issuer_debit
                .checked_mul(5)
                .and_then(|n| n.checked_add(12 * 16 * 1024 + 2 * std::mem::size_of::<Self>() + 256))
                // The closed native adapter keeps this receipt behind one
                // Box. Charge its fixed allocation before any request grows
                // and before the final principal check or mutation begins.
                .and_then(|n| n.checked_add(std::mem::size_of::<IssueLinkReceiptV87>()))
                .ok_or_else(|| refused(GithubErrorKind::Transport))?,
        )?;
        let input = json!({"query":protocol::mutation_query(plan.command),"variables":{"input":{"issueId":actual.pair.issue.node,"pullRequestIds":[actual.parent.node_id()]}}});
        let bytes = serde_json::to_vec(&input).map_err(|_| refused(GithubErrorKind::Json))?;
        if bytes.len() > 16 * 1024 || bytes.capacity() > 32 * 1024 {
            return Err(refused(GithubErrorKind::Transport));
        }
        let wire_spec = provider.spec(vec![
            "api".into(),
            "graphql".into(),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "POST".into(),
            "--input".into(),
            "-".into(),
        ]);
        let wire = IssueLinkWireV87 {
            program: wire_spec.program,
            args: wire_spec.args,
            env: wire_spec.env,
            output_limit: wire_spec.output_limit.min(SCALAR_CAP_V87),
            timeout: wire_spec.timeout,
        };
        let forward_bytes = serde_json::to_vec(
            &json!({"query":protocol::pr_links_query(),"variables":{"pr":actual.parent.node_id()}}),
        )
        .map_err(|_| refused(GithubErrorKind::Json))?;
        let reverse_bytes=serde_json::to_vec(&json!({"query":protocol::issue_links_query(),"variables":{"issue":actual.pair.issue.node}})).map_err(|_|refused(GithubErrorKind::Json))?;
        if [&forward_bytes, &reverse_bytes]
            .into_iter()
            .any(|bytes| bytes.len() > 16 * 1024 || bytes.capacity() > 32 * 1024)
        {
            return Err(refused(GithubErrorKind::Transport));
        }
        let retained = heap.retained()?;
        // Reject a known-impossible capture before write. This is a PEAK check,
        // not persistent scratch carried into later phases. The actual capture
        // and every dynamically decoded node are debited again by the decoder;
        // completed parser scratch dies before the next readback begins.
        heap.charge(3 * SCALAR_CAP_V87)?;
        Ok(Self {
            actual: Arc::downgrade(actual),
            runner: provider.runner.clone(),
            issuer: ThreadPhaseIssuerV16::capture(provider)?,
            cancellation: cancel.clone(),
            deadline: actual.deadline(),
            resources: actual.resources.clone(),
            generations: actual.generations,
            bytes,
            paid: None,
            retained,
            wire,
            forward_bytes,
            reverse_bytes,
            ticket: Arc::new(()),
            successors: None,
            readbacks_validated: false,
        })
    }
    pub(in crate::providers::gh) fn matches_paid_pool_v87(
        &self,
        provider: &GithubProvider,
        pool: &Weak<ScopedReadBudgetV1>,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let actual = self
            .actual
            .upgrade()
            .ok_or_else(|| refused(GithubErrorKind::IdentityChanged))?;
        actual
            .binding
            .validate_lineage(provider, cancel, self.deadline)?;
        if !pool.ptr_eq(&Arc::downgrade(&actual.binding.measured))
            || !self.issuer.matches(provider)
            || !self.cancellation.same_instance(cancel)
            || self.paid.is_some()
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        Ok(())
    }
    pub(in crate::providers::gh) fn metadata_profile_v87(&self) -> GithubResult<bool> {
        self.actual
            .upgrade()
            .map(|a| a.purpose.metadata())
            .ok_or_else(|| refused(GithubErrorKind::IdentityChanged))
    }
    pub(in crate::providers::gh) fn bind_paid_v87(
        &mut self,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        alive(cancel, self.deadline)?;
        if self.paid.is_some()
            || !self.issuer.matches(provider)
            || !self.cancellation.same_instance(cancel)
            || provider.scoped_read_budget.is_some()
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        let (deadline, cap) = provider
            .scoped_read_window
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?;
        self.deadline = self.deadline.min(deadline);
        let mut spec = provider.spec(vec![
            "api".into(),
            "graphql".into(),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "POST".into(),
            "--input".into(),
            "-".into(),
        ]);
        spec.output_limit = spec.output_limit.min(cap).min(self.wire.output_limit);
        spec.absolute_deadline_v1 = Some(self.deadline);
        spec.timeout = spec
            .timeout
            .min(self.wire.timeout)
            .min(self.deadline.saturating_duration_since(Instant::now()));
        spec.stdin = self.bytes.clone();
        spec = spec.write(self.resources[0].clone());
        self.paid = Some(spec);
        Ok(())
    }
    pub(in crate::providers) fn runner_parts_v87<'a>(
        &'a self,
        runner: &CommandRunner,
        cancel: &CancellationToken,
    ) -> Result<IssueLinkRunnerPartsV87<'a>, CommandFailure> {
        let refused = || CommandFailure {
            kind: FailureKind::ReconciliationRequired,
            message: "Actual prepared Issue-link request changed/expired; no invocation".into(),
            exit_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            outcome_unknown: false,
        };
        let actual = self.actual.upgrade().ok_or_else(refused)?;
        let spec = self.paid.as_ref().ok_or_else(refused)?;
        if !self.runner.same_instance(runner)
            || !self.cancellation.same_instance(cancel)
            || cancel.is_cancelled()
            || Instant::now() >= self.deadline.min(actual.deadline())
            || self.resources != actual.resources
            || self.generations != actual.generations
            || spec.program != self.wire.program
            || spec.args != self.wire.args
            || spec.env != self.wire.env
            || spec.cwd.is_some()
            || spec.clear_env
            || !spec.sensitive_values.is_empty()
            || spec.original_read_cancellation.is_some()
            || spec.timeout > self.wire.timeout
            || spec.scoped_read_budget.is_some()
            || spec.coalesce_read
            || spec.output_limit > self.wire.output_limit
            || spec.absolute_deadline_v1 != Some(self.deadline)
            || !matches!(&spec.kind,crate::providers::runner::CommandKind::Write{resource} if resource==&self.resources[0])
            || spec.stdin != self.bytes
        {
            return Err(refused());
        }
        Ok((spec, &self.resources, self.generations, self.deadline))
    }
    pub(in crate::providers) fn runner_ticket_v87(&self) -> Weak<()> {
        Arc::downgrade(&self.ticket)
    }
    pub(in crate::providers::gh) fn bind_run_v87(
        &mut self,
        run: &crate::providers::runner::IssueLinkMutationRunV87,
    ) -> GithubResult<()> {
        let successors = run.successors();
        if self.successors.is_some()
            || !run.matches_ticket_v87(&self.ticket)
            || self
                .generations
                .iter()
                .zip(successors)
                .any(|(old, next)| old.checked_add(1) != Some(next))
        {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        self.successors = Some(successors);
        Ok(())
    }
    pub(in crate::providers) fn runner_postwrite_v87<'a>(
        &'a self,
        runner: &CommandRunner,
        cancel: &CancellationToken,
    ) -> Result<(&'a [String; 2], [u64; 2], Instant), CommandFailure> {
        self.runner_parts_v87(runner, cancel)?;
        let successors = self.successors.ok_or_else(|| CommandFailure {
            kind: FailureKind::ReconciliationRequired,
            message: "Actual Issue-link mutation epoch missing".into(),
            exit_code: None,
            stdout: Vec::new(),
            stderr: Vec::new(),
            outcome_unknown: true,
        })?;
        Ok((&self.resources, successors, self.deadline))
    }
    // Parent-private: ONLY the fixed publication child, after its two actual
    // bounded readbacks and consumed cursor3, can set this completion marker.
    fn accept_readback_receipt_v87(&mut self, after: &IssueLinkPairV87) -> GithubResult<()> {
        let actual = self
            .actual
            .upgrade()
            .ok_or_else(|| refused(GithubErrorKind::IdentityChanged))?;
        if self.successors.is_none() || self.readbacks_validated {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        actual
            .pair
            .confirms(after, actual.parent(), actual.command)?;
        self.readbacks_validated = true;
        Ok(())
    }
    pub(in crate::providers) fn runner_confirmed_parts_v87<'a>(
        &'a self,
        runner: &CommandRunner,
        cancel: &CancellationToken,
    ) -> Result<(&'a [String; 2], [u64; 2], Instant), CommandFailure> {
        if !self.readbacks_validated {
            return Err(CommandFailure {
                kind: FailureKind::ReconciliationRequired,
                message: "Actual two independent Issue-link readbacks have not closed".into(),
                exit_code: None,
                stdout: Vec::new(),
                stderr: Vec::new(),
                outcome_unknown: true,
            });
        }
        self.runner_postwrite_v87(runner, cancel)
    }
    pub(in crate::providers::gh) fn paid_readback_v87(
        &self,
        provider: &GithubProvider,
        role: IssueLinkReadbackRoleV87,
        heap: &mut IssueLinkHeapV87,
        cancel: &CancellationToken,
    ) -> GithubResult<bounded::IssueDocumentV87> {
        let (_, successors, deadline) = self
            .runner_postwrite_v87(&provider.runner, cancel)
            .map_err(GithubError::from)?;
        if !self.issuer.matches(provider) || provider.scoped_read_budget.is_some() {
            return Err(refused(GithubErrorKind::IdentityChanged));
        }
        provider
            .runner
            .validate_issue_link_generations_v87(&self.resources, successors, cancel, deadline)
            .map_err(GithubError::from)?;
        let reservation = heap.reserve_document(PAIR_CAP_V87)?;
        let bytes = match role {
            IssueLinkReadbackRoleV87::Pr => &self.forward_bytes,
            IssueLinkReadbackRoleV87::Issue => &self.reverse_bytes,
        };
        let mut spec = provider.spec(vec![
            "api".into(),
            "graphql".into(),
            "--hostname".into(),
            provider.host.clone(),
            "--method".into(),
            "POST".into(),
            "--input".into(),
            "-".into(),
        ]);
        spec.output_limit = spec.output_limit.min(PAIR_CAP_V87);
        spec.stdin = bytes.clone();
        let output = provider
            .runner
            .run(spec, cancel)
            .map_err(|failure| refused(GithubError::from(failure).kind))?;
        provider
            .runner
            .validate_issue_link_generations_v87(&self.resources, successors, cancel, deadline)
            .map_err(GithubError::from)?;
        heap.decode(reservation, &output.stdout)
    }
    pub(in crate::providers) fn accepts_mutation_output_v87(
        &self,
        output: &super::super::runner::CommandOutput,
    ) -> bool {
        let Some(actual) = self.actual.upgrade() else {
            return false;
        };
        if output.exit_code != 0 || output.capture.total_bytes() > SCALAR_CAP_V87 {
            return false;
        }
        let result = (|| {
            let mut heap = IssueLinkHeapV87::new(self.retained)?;
            let reservation = heap.reserve_document(SCALAR_CAP_V87)?;
            let document = heap.decode(reservation, &output.stdout)?;
            protocol::mutation_matches(document.value(), &actual.pair.issue)?;
            heap.release_document(document)
        })();
        result.is_ok()
    }
}
