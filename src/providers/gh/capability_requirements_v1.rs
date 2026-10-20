//! Closed PRODUCT requirements, never permission/support/publication evidence.
//! These map doc-3/doc-5 coverage without guessing an Enterprise version floor.
use super::{Capability, CliRequirement};

macro_rules! rows {
    ($($name:ident => ($key:literal, $label:literal, $doc:literal)),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
        pub enum CapabilityContractRowV1 { $($name),+ }
        impl CapabilityContractRowV1 {
            pub const ALL: &'static [Self] = &[$(Self::$name),+];
            pub fn key(self) -> &'static str { match self { $(Self::$name => $key),+ } }
            pub fn label(self) -> &'static str { match self { $(Self::$name => $label),+ } }
            pub fn document(self) -> &'static str { match self { $(Self::$name => $doc),+ } }
        }
    }
}
rows! {
    PrQueues => ("pr.queues", "Personal PR queues", "doc-3"),
    PrDetails => ("pr.details", "PR details, timeline, commits, files and reviews", "doc-3"),
    PrChecks => ("pr.checks", "Checks, jobs, logs and run controls", "doc-3"),
    PrCreate => ("pr.create", "Create a PR, choose forks/head/base and draft", "doc-3"),
    PrMetadata => ("pr.metadata", "Edit PR metadata and linked items", "doc-3"),
    PrLifecycle => ("pr.lifecycle", "PR lifecycle and branch controls", "doc-3"),
    PrConversation => ("pr.conversation", "Conversation comments, reactions and subscription", "doc-3"),
    PrInline => ("pr.inline", "Inline and multiline comments and replies", "doc-3"),
    PrPendingReview => ("pr.pending-review", "Pending review drafts, submit and discard", "doc-3"),
    PrDismissal => ("pr.dismissal", "Review dismissal and re-request", "doc-3"),
    PrThreadState => ("pr.thread-state", "Resolve threads and viewed files", "doc-3"),
    PrSuggestions => ("pr.suggestions", "Suggested changes and application", "doc-3"),
    PrConflicts => ("pr.conflicts", "Edit PR files and resolve conflicts", "doc-3"),
    PrMerge => ("pr.merge", "Merge methods, auto-merge and queues", "doc-3"),
    PrRevert => ("pr.revert", "Revert and restoration followups", "doc-3"),
    PrNavigation => ("pr.navigation", "Copy, open, export and navigation", "doc-3"),
    ActionsDiscovery => ("actions.discovery", "Actions indicators and discovery", "doc-5"),
    ActionsDetails => ("actions.details", "Run attempts, jobs, steps and progress", "doc-5"),
    ActionsLogs => ("actions.logs", "Logs, annotations, search and freshness", "doc-5"),
    ActionsDispatch => ("actions.dispatch", "Workflow dispatch and definitions", "doc-5"),
    ActionsControl => ("actions.control", "Watch, rerun, cancel and deletion", "doc-5"),
    ActionsArtifacts => ("actions.artifacts", "Artifact inventory, preview, export and deletion", "doc-5"),
    ActionsApprovals => ("actions.approvals", "Fork-run and deployment approvals", "doc-5"),
    ActionsCache => ("actions.cache", "Caches and quota context", "doc-5"),
    ActionsSettings => ("actions.settings", "Secrets and variables", "doc-5"),
    ActionsEnvironments => ("actions.environments", "Environments and Actions permissions", "doc-5"),
    ActionsRunners => ("actions.runners", "Runners, groups, labels and associations", "doc-5"),
    ActionsActivity => ("actions.activity", "Cross-repository Actions activity", "doc-5"),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityEffectV1 {
    ReadOnly,
    RemoteWrite,
    IsolatedGitWrite,
    LocalOutput,
    Mixed,
}
impl CapabilityEffectV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::ReadOnly => "Read-only inspection; no GitHub or Git mutation",
            Self::RemoteWrite => "May write remote state; reads do not authorize a write",
            Self::IsolatedGitWrite => "Isolated Git changes and explicit remote publication",
            Self::LocalOutput => "Local export or labelled external open; no GitHub mutation",
            Self::Mixed => "Mixed read/write family; the exact operation determines its effect",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Profile {
    Queue,
    Read,
    RunRead,
    Create,
    PrWrite,
    Discussion,
    Review,
    IsolatedGit,
    Run,
    Workflow,
    Approval,
    Settings,
    Artifact,
    Output,
    Checks,
}

/// Requirements only: no caller, target, permission grant, command or approval.
/// This descriptor cannot be submitted to any publication provider/controller.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CapabilityRequirementsV1 {
    pub rows: &'static [CapabilityContractRowV1],
    pub operations: &'static str,
    pub tasks: &'static [u16],
    pub cli: Option<(&'static str, CliRequirement)>,
    profile: Profile,
}
impl CapabilityRequirementsV1 {
    pub fn effect(self) -> CapabilityEffectV1 {
        use CapabilityEffectV1::*;
        match self.profile {
            Profile::Queue | Profile::Read | Profile::RunRead => ReadOnly,
            Profile::Create
            | Profile::PrWrite
            | Profile::Discussion
            | Profile::Review
            | Profile::Run
            | Profile::Approval
            | Profile::Settings => RemoteWrite,
            Profile::IsolatedGit => IsolatedGitWrite,
            Profile::Artifact | Profile::Checks | Profile::Workflow => Mixed,
            Profile::Output => LocalOutput,
        }
    }
    pub fn preview_requirement(self) -> &'static str {
        match self.profile {
            Profile::Queue | Profile::Read | Profile::RunRead => "Read-only navigation needs no publication approval; show stale or incomplete data explicitly.",
            Profile::Output => "Choose an exact export destination and overwrite policy; label browser handoffs as external, not native parity.",
            Profile::IsolatedGit => "Preview exact selected edits, resulting commit and push in an isolated worktree; never silently edit an unrelated checkout.",
            _ => "Prepare the exact effect read-only, then require explicit confirmation; a saved draft is not approval. Inspect uncertain results instead of resending.",
        }
    }
    pub fn target_requirement(self) -> &'static str {
        match self.profile {
            Profile::Queue => "Current host/account and selected repository scope; preserve distinct queue memberships and report partial search coverage.",
            Profile::Read => "Exact host/account/repository and original object/revision identity; a similar name or newer head is not the original source.",
            Profile::RunRead => "Exact run, attempt, job and commit SHA; never silently substitute the latest attempt. Label log bounds, polling and observation age.",
            Profile::Create => "Exact repository/fork, base/head refs and observed commits before creation; reconcile ambiguous creation and return the canonical new PR instead of duplicating it.",
            Profile::PrWrite => "Fresh principal, stable PR identity and expected head/resource state; selected permissions and readback are required. Preflight alone is not atomic.",
            Profile::Discussion => "Fresh principal, canonical PR/comment/reaction identity, selected-operation author/moderator permissions and observed body/state; never retarget by login or row number.",
            Profile::Review => "Original PR/head and the selected canonical file/side/range, thread or review identity; protect draft versions and explain outdated anchors before publishing.",
            Profile::IsolatedGit => "Original head, exact selected paths and isolated worktree/branch; preserve the user's checkout, index and unrelated work, and handle changed heads/conflicts.",
            Profile::Run | Profile::Checks => "Exact commit SHA, run, attempt and selected job; job IDs are not run IDs. Recheck current state and distinguish accepted requests from observed completion.",
            Profile::Workflow => "Exact workflow ID, immutable definition SHA, selected ref and typed inputs; verify allowed workflow state. Local YAML edits require their own explicit Git/commit/push workflow.",
            Profile::Approval => "Exact original run and environment/gate IDs with fresh pending state and caller permission; explain protection rules and never approve a similarly named replacement gate.",
            Profile::Settings => "Exact eligible repository/environment/organization scope and original stable resource identity/state; verify scoped permission and selected membership, preview scope and read back metadata.",
            Profile::Artifact => "Exact artifact, run and commit; bound untrusted archive paths/sizes, never execute content, and confirm exact deletion/export effects.",
            Profile::Output => "Keep the exact original source and navigation context; validate the explicit local destination without mutating Git or GitHub.",
        }
    }
    pub fn paging_requirement(self) -> &'static str {
        match self.profile {
            Profile::Queue => "Page all selected queues within limits; retain membership, deduplicate stable PR IDs, and label search caps/incomplete results.",
            Profile::IsolatedGit => "Bound original diffs and affected files; do not guess omitted edits or silently fall back to a different checkout.",
            Profile::Output => "Bound exports and preserve exact selected content; do not represent a truncated result as complete.",
            _ => "Bound pages/bytes/time and keep explicit incompleteness; selections and writes must not guess missing members, permissions or original IDs.",
        }
    }
    pub fn host_limit(self) -> &'static str {
        "Host/API and minimum Enterprise version unverified here; the exact workflow must check its public operation/schema, current target and permission. Unknown is not supported."
    }
    pub fn evidence_limit(self) -> &'static str {
        "Requirements are not delivery evidence. Task-specific code/provider and keyboard/mouse PTY gates plus opt-in disposable-repository evidence remain separate; this inspector performs no live probe or mutation."
    }
}
impl Capability {
    pub fn requirements_v1(&self) -> Option<CapabilityRequirementsV1> {
        capability_requirement_contract_v1(&self.id)
    }
}

/// Map category requirements, not the mutable/public legacy transport flag.
/// An unknown category has no guessed contract, host floor or permission.
pub fn capability_requirement_contract_v1(id: &str) -> Option<CapabilityRequirementsV1> {
    use CapabilityContractRowV1::*;
    use Profile::*;
    let (rows, profile, operations, tasks): (&'static [_], _, _, &'static [_]) = match id {
        "personal-queues" => (&[PrQueues], Queue, "Authored, assigned and review-requested queues across selected repositories; preserve membership and explicit incomplete search results.", &[12]),
        "pr-details" => (&[PrDetails], Read, "PR summary, event timeline, commits, canonical files and reviews; all pages with stable PR and exact base/head identities.", &[13,15,20]),
        "pr-checks" => (&[PrChecks], Checks, "Required checks/status, SHA-correlated jobs and logs; rerun/cancel uses the separately guarded exact Actions operation.", &[14,53,55]),
        "pr-create" => (&[PrCreate], Create, "Create a PR with fork/head/base/draft choices, exact diff preview and canonical new-PR readback.", &[26]),
        "pr-metadata" => (&[PrMetadata,PrDismissal], PrWrite, "Title/body/base, labels, assignees, reviewers and re-request; milestone and linked issues with selected-field readback; Projects (v2) placement add/remove with item readback.", &[18,20]),
        "pr-lifecycle" => (&[PrLifecycle], PrWrite, "Draft/ready and close/reopen transitions with explicit original head/state and outcome.", &[25]),
        "pr-lock-update-revert" => (&[PrLifecycle,PrRevert], PrWrite, "Lock/unlock, update branch, branch deletion and reverting PR followups; preserve exact scope and traceable outcomes.", &[25]),
        "pr-comments" => (&[PrConversation], Discussion, "Create conversation comments on the canonical PR; verify the original actor/permission and refresh the paginated discussion.", &[17]),
        "pr-comment-edit-delete-react" => (&[PrConversation], Discussion, "Edit/delete conversation comments, add/remove reactions and subscription; preserve canonical IDs and original-author permissions.", &[17]),
        "pr-inline" => (&[PrInline], Review, "Inline/multiline comments and replies, edit/delete; canonical file/side/line/range and expected head, with outdated-anchor handling.", &[19]),
        "pr-reviews" => (&[PrPendingReview], Review, "Submit approval, request changes or comment review on the observed head; keep local drafts distinct from publication.", &[20]),
        "pr-pending-reviews" => (&[PrPendingReview,PrDismissal], Review, "Add/edit/delete pending drafts, submit/discard and dismiss reviews; restart-safe drafts and maintainer/reviewer limits.", &[16,20]),
        "pr-threads" => (&[PrThreadState], Review, "Resolve/unresolve the exact review thread; refresh server state and invalidate stale file/head observations.", &[21]),
        "pr-viewed" => (&[PrThreadState], Review, "Mark exact files viewed/unviewed for the current viewer; invalidate progress when files change, without changing repository commits.", &[22]),
        "pr-suggestions" => (&[PrSuggestions], IsolatedGit, "Author/preview suggestions and explicitly apply selected edits; preserve credit where supported and handle changed-head/conflict outcomes.", &[23]),
        "pr-conflicts" => (&[PrConflicts], IsolatedGit, "Edit PR branch files and resolve conflicts in an explicit isolated-worktree flow; preview resulting commit and push.", &[27]),
        "pr-merge" => (&[PrMerge], PrWrite, "Merge/squash/rebase, allowed methods/rules/checks and auto-merge; expected head and distinct queued/merged outcomes.", &[24]),
        "pr-queue" => (&[PrMerge], PrWrite, "Merge-queue status, enqueue/dequeue and cancellation; current head/rules and separately observed queue versus merge effects.", &[24]),
        "pr-navigation" => (&[PrNavigation], Output, "Copy/open/export and native navigation; visibly label browser handoffs without counting them as implemented PR workflows.", &[10]),
        "actions-workflows" => (&[ActionsDiscovery], Read, "PR/commit/repository status indicators, workflow catalogues, selected repository/org discovery, filters and latest runs with exact SHA correlation.", &[14,52]),
        "actions-details" => (&[ActionsDetails], RunRead, "Run SHA/ref/event/actor/attempt, durations, matrix jobs, steps and progress; explain definition/dependency inference limits.", &[53]),
        "actions-logs" => (&[ActionsLogs], RunRead, "Bounded job/step logs, failed-step jump, exact/regex search, annotations and labelled polling freshness; do not invent a live log stream.", &[53]),
        "actions-dispatch" => (&[ActionsDispatch], Workflow, "Read YAML at a revision, typed workflow_dispatch inputs and ref selection, enable/disable, and explicit local workflow edits.", &[54]),
        "actions-control" => (&[ActionsControl], Run, "Watch and rerun all/failed/specific jobs, debug logging, cancel/force-cancel and delete runs/logs with exact operation effects.", &[55]),
        "actions-delete" => (&[ActionsControl], Run, "Explicit run/log deletion, selected-job/debug rerun and force cancellation where supported; distinguish accepted from completed requests.", &[55]),
        "actions-artifacts" => (&[ActionsArtifacts], Artifact, "Artifact inventory, bounded native previews, download/export, retention/expiry and explicit deletion; never execute untrusted archive content.", &[56]),
        "actions-approval" => (&[ActionsApprovals], Approval, "Fork-run approval and pending environment approval/rejection; explain protection rules and current host/caller limits.", &[57]),
        "actions-cache" => (&[ActionsCache], Settings, "List/filter cache sizes, refs and age, quota context and explicit individual/selected deletion using exact cache identity.", &[58]),
        "actions-secrets-variables" => (&[ActionsSettings], Settings, "Eligible repository/environment/organization metadata, variable CRUD and secret set/delete. Existing secret values cannot be read; entered values stay transient and masked.", &[59]),
        "actions-policy" => (&[ActionsEnvironments], Settings, "Supported Actions policies and workflow-token permissions with exact eligible scope, preview and current-plan/host/permission limits.", &[60]),
        "actions-environments" => (&[ActionsEnvironments], Settings, "Environment inventory and supported protection rules, branch/tag patterns and installed-App rules with exact original environment identity.", &[60]),
        "actions-runners" => (&[ActionsRunners], Settings, "Available/busy/offline runners and groups, labels, supported associations and removal; no runner service installation, compute provisioning or billing changes.", &[60]),
        "actions-activity" => (&[ActionsActivity], Read, "Saved cross-repository filters, actor usernames/avatar fallback, failure queues, elapsed states and dashboard evidence drilldowns.", &[61,62]),
        _ => return None,
    };
    let cli = match id {
        "pr-details" => Some((
            "PR detail collection via gh api --paginate --slurp",
            CliRequirement::ApiSlurp,
        )),
        "pr-checks" | "actions-logs" => Some((
            "Exact-attempt logs via gh run view --attempt --log",
            CliRequirement::RunAttempt,
        )),
        "pr-lifecycle" => Some((
            "CLI draft conversion via gh pr ready --undo",
            CliRequirement::ReadyUndo,
        )),
        "pr-merge" => Some((
            "CLI expected-head merge via gh pr merge --match-head-commit",
            CliRequirement::MergeExpectedHead,
        )),
        "actions-secrets-variables" => Some((
            "Empty selected organization secret scope via gh secret set --no-repos-selected",
            CliRequirement::SecretEmptySelection,
        )),
        _ => None,
    };
    Some(CapabilityRequirementsV1 {
        rows,
        profile,
        operations,
        tasks,
        cli,
    })
}

#[cfg(test)]
#[path = "capability_requirements_v1_tests.rs"]
mod tests;
