//! State-command DATA model. Native eligibility requires the purpose-closed
//! genuine original producer; this model never grants current access.
//! Local state-intent previews and closed recorded metadata shapes only. No
//! transport, SQLite, mutation method, acknowledgement, or authority constructor.
//! A legacy PrSummary is not an opaque current access/capability observation.
use crate::{
    identity::ActorIdentity,
    pr_write_journal::{PrWriteIntent, PrWriteTarget},
    providers::{
        gh::{Actor, HostIdentity, MergeMethod, PrSummary},
        runner::sanitize_terminal,
    },
    search::model::valid_sha,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, fmt};

const MAX_INPUT_BYTES: usize = 128 * 1024;
const MAX_BODY_BYTES: usize = 65_536;
const MAX_ID_BYTES: usize = 1024;
const MAX_ROWS: usize = 4096;
const MAX_PREPARED_BYTES: usize = 512 * 1024;
const MAX_RECORD_BYTES: usize = 8192;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrStateCommand {
    Draft,
    Ready,
    Close,
    Reopen,
    Lock,
    Unlock,
    Merge,
    Squash,
    Rebase,
    EnableAutoMerge,
    EnableAutoSquash,
    EnableAutoRebase,
    DisableAutoMerge,
    Enqueue,
    Dequeue,
    UpdateMerge,
    UpdateRebase,
    Revert,
    DeleteRemoteHead,
}
impl PrStateCommand {
    pub const ALL: [Self; 19] = [
        Self::Draft,
        Self::Ready,
        Self::Close,
        Self::Reopen,
        Self::Lock,
        Self::Unlock,
        Self::Merge,
        Self::Squash,
        Self::Rebase,
        Self::EnableAutoMerge,
        Self::EnableAutoSquash,
        Self::EnableAutoRebase,
        Self::DisableAutoMerge,
        Self::Enqueue,
        Self::Dequeue,
        Self::UpdateMerge,
        Self::UpdateRebase,
        Self::Revert,
        Self::DeleteRemoteHead,
    ];
    pub fn key(self) -> &'static str {
        match self {
            Self::Draft => "state-draft",
            Self::Ready => "state-ready",
            Self::Close => "state-close",
            Self::Reopen => "state-reopen",
            Self::Lock => "state-lock",
            Self::Unlock => "state-unlock",
            Self::Merge => "state-merge",
            Self::Squash => "state-squash",
            Self::Rebase => "state-rebase",
            Self::EnableAutoMerge => "state-enable-auto-merge",
            Self::EnableAutoSquash => "state-enable-auto-squash",
            Self::EnableAutoRebase => "state-enable-auto-rebase",
            Self::DisableAutoMerge => "state-disable-auto-merge",
            Self::Enqueue => "state-enqueue",
            Self::Dequeue => "state-dequeue",
            Self::UpdateMerge => "state-update-merge",
            Self::UpdateRebase => "state-update-rebase",
            Self::Revert => "state-revert",
            Self::DeleteRemoteHead => "state-delete-remote-head",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Draft => "Convert to draft",
            Self::Ready => "Mark ready for review",
            Self::Close => "Close without merging",
            Self::Reopen => "Reopen unmerged PR",
            Self::Lock => "Lock conversation",
            Self::Unlock => "Unlock conversation",
            Self::Merge => "Merge with a merge commit",
            Self::Squash => "Squash and merge",
            Self::Rebase => "Rebase and merge",
            Self::EnableAutoMerge => "Enable auto-merge with a merge commit",
            Self::EnableAutoSquash => "Enable auto-merge with squash",
            Self::EnableAutoRebase => "Enable auto-merge with rebase",
            Self::DisableAutoMerge => "Disable auto-merge",
            Self::Enqueue => "Join merge queue",
            Self::Dequeue => "Leave merge queue",
            Self::UpdateMerge => "Update PR branch by merging base",
            Self::UpdateRebase => "Update PR branch by rebasing onto base",
            Self::Revert => "Create a reverting PR",
            Self::DeleteRemoteHead => "Delete only the remote head branch",
        }
    }
    /// The existing provider enum is reused; this is an intended method, not a
    /// proof the repository/caller/rules allow it or the server used it.
    pub fn requested_merge_method(self) -> Option<MergeMethod> {
        match self {
            Self::Merge | Self::EnableAutoMerge => Some(MergeMethod::Merge),
            Self::Squash | Self::EnableAutoSquash => Some(MergeMethod::Squash),
            Self::Rebase | Self::EnableAutoRebase => Some(MergeMethod::Rebase),
            _ => None,
        }
    }
    fn method_name(self) -> Option<&'static str> {
        self.requested_merge_method().map(|method| match method {
            MergeMethod::Merge => "merge",
            MergeMethod::Squash => "squash",
            MergeMethod::Rebase => "rebase",
        })
    }
    fn merges(self) -> bool {
        matches!(self, Self::Merge | Self::Squash | Self::Rebase)
    }
    fn enables_auto(self) -> bool {
        matches!(
            self,
            Self::EnableAutoMerge | Self::EnableAutoSquash | Self::EnableAutoRebase
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PrStateLockReason {
    OffTopic,
    TooHeated,
    Resolved,
    Spam,
}
impl PrStateLockReason {
    pub fn label(self) -> &'static str {
        match self {
            Self::OffTopic => "off-topic",
            Self::TooHeated => "too heated",
            Self::Resolved => "resolved",
            Self::Spam => "spam",
        }
    }
}

/// Typed local field data, never an eligible catalogue/current-state proof.
#[derive(Clone)]
pub enum PrStateFields {
    None,
    Lock {
        reason: Option<PrStateLockReason>,
    },
    Revert {
        title: String,
        body: String,
        draft: bool,
    },
}
impl fmt::Debug for PrStateFields {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PrStateFields(<private local fields>)")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrStateSourceRequirement {
    FreshPrincipalAndCanonicalAccess,
    ExactOriginalComparisonAndLifecycle,
    NullableCallerCapability,
    RepositoryMergeSettings,
    RequiredChecksAndRulesCoverage,
    CurrentQueueEntry,
    CurrentAutoMergeRequest,
    ExactIssueLinkAndLockState,
    ExactHeadRepositoryAndRef,
    SupportedUpdateMethod,
    SupportedTraceableRevert,
    RemoteOnlyDeletionTransportDecision,
    DurableStateFilterReadRepair,
}

/// No Serialize/Deserialize: restoring fields never restores this preview or
/// approval. There is intentionally no execute/is_authorized/can_confirm method.
#[derive(Clone)]
pub struct PrStatePlan {
    command: PrStateCommand,
    target: PrWriteTarget,
    intent: PrWriteIntent,
    digest: String,
    preview: Vec<String>,
    actor: ActorIdentity,
    disabled_reason: Option<String>,
    requirements: Vec<PrStateSourceRequirement>,
}
impl fmt::Debug for PrStatePlan {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrStatePlan")
            .field("command", &self.command)
            .field("private_fields", &"[REDACTED]")
            .field("authority", &"none: local snapshot")
            .finish()
    }
}
impl PrStatePlan {
    pub fn prepare(
        command: PrStateCommand,
        identity: &HostIdentity,
        original: &PrSummary,
        fields: &PrStateFields,
    ) -> Result<Self, String> {
        let (target, intent) = checked_intent(command, identity, original, fields)?;
        let digest = intent.digest().map_err(|e| e.to_string())?;
        let actor = ActorIdentity::github(
            &identity.host,
            &Actor {
                login: identity.login.clone(),
                ..Actor::default()
            },
        );
        let disabled_reason = snapshot_disabled(command, original);
        let requirements = source_requirements(command);
        let mut rows = Rows::default();
        rows.push(&format!("PR state intent • {}", command.label()))?;
        if let Some(reason) = &disabled_reason {
            rows.push(reason)?;
        }
        rows.push("Local snapshot only • not authorization or a publication plan.")?;
        rows.push(&format!(
            "Target: {}#{} • {}",
            target.repository(),
            target.number(),
            target.node_id()
        ))?;
        rows.push(&format!(
            "Caller: {} • account #{} • {}",
            actor.label(),
            identity.account_id.expect("positive scope checked"),
            target.scope().host
        ))?;
        rows.push(&format!("Original head: {}", original.head_sha))?;
        rows.push(&format!("Original base: {}", original.base_sha))?;
        rows.push(&format!(
            "Original head ref (escaped): {}",
            escaped(&original.head_ref)
        ))?;
        rows.push(&format!(
            "Original base ref (escaped): {}",
            escaped(&original.base_ref)
        ))?;
        rows.push(&format!(
            "Loaded lifecycle: {} • draft {} • merged {}",
            original.pr.state, original.pr.draft, original.merged
        ))?;
        if let Some(method) = command.method_name() {
            rows.push(&format!("Requested merge method: {method}"))?;
        }
        rows.push(effect_notice(command))?;
        match fields {
            PrStateFields::Lock { reason } => rows.push(&format!(
                "Requested lock reason: {}",
                reason.map_or("none (explicitly omitted)", PrStateLockReason::label)
            ))?,
            PrStateFields::Revert { title, body, draft } => {
                rows.push(&format!("Requested reverting PR draft: {draft}"))?;
                rows.push("Requested reverting PR title:")?;
                rows.push(&escaped(title))?;
                rows.push("Requested reverting PR description (escaped):")?;
                rows.push(&escaped(body))?;
            }
            PrStateFields::None => {}
        }
        rows.push("Caller capability, repository method settings, queue identity and required rules: Source unknown in this legacy snapshot.")?;
        rows.push("Fresh opaque original-bound source and state-filter read repair are required before any durable journal claim or request.")?;
        rows.push("Local checkout, files, index and commits are not part of this request.")?;
        rows.push("Draft/restart/edit never restores approval. Accepted, queued, auto-enabled and merged are different outcomes; nothing is resent automatically.")?;
        Ok(Self {
            command,
            target,
            intent,
            digest,
            preview: rows.rows,
            actor,
            disabled_reason,
            requirements,
        })
    }
    /// This only checks local immutable input; success is NOT write authority.
    pub fn validate_local(
        &self,
        identity: &HostIdentity,
        original: &PrSummary,
        fields: &PrStateFields,
    ) -> Result<(), String> {
        let (target, intent) = checked_intent(self.command, identity, original, fields)?;
        if target != self.target || intent.digest().map_err(|e| e.to_string())? != self.digest {
            return Err("Original state/target or fields changed after local Preview; prepare a new snapshot, never reuse approval".into());
        }
        if let Some(reason) = &self.disabled_reason {
            return Err(reason.clone());
        }
        Ok(())
    }
    pub fn command(&self) -> PrStateCommand {
        self.command
    }
    pub fn target(&self) -> &PrWriteTarget {
        &self.target
    }
    pub fn intent(&self) -> &PrWriteIntent {
        &self.intent
    }
    pub fn digest(&self) -> &str {
        &self.digest
    }
    pub fn preview(&self) -> &[String] {
        &self.preview
    }
    pub fn actor(&self) -> &ActorIdentity {
        &self.actor
    }
    pub fn disabled_reason(&self) -> Option<&str> {
        self.disabled_reason.as_deref()
    }
    pub fn source_requirements(&self) -> &[PrStateSourceRequirement] {
        &self.requirements
    }
}

fn checked_intent(
    command: PrStateCommand,
    identity: &HostIdentity,
    original: &PrSummary,
    fields: &PrStateFields,
) -> Result<(PrWriteTarget, PrWriteIntent), String> {
    let mut bytes = 0usize;
    for value in [
        identity.host.as_str(),
        identity.login.as_str(),
        original.pr.host.as_str(),
        original.pr.repository.as_str(),
        original.pr.id.as_str(),
        original.head_sha.as_str(),
        original.base_sha.as_str(),
        original.head_ref.as_str(),
        original.base_ref.as_str(),
        original.pr.state.as_str(),
    ] {
        if value.len() > MAX_ID_BYTES {
            return Err("Original identity/ref exceeds the 1 KiB per-field budget".into());
        }
        bytes = bytes
            .checked_add(value.len())
            .ok_or("Original metadata byte budget overflow")?;
    }
    if !valid_sha(&original.head_sha)
        || !valid_sha(&original.base_sha)
        || original.head_ref.is_empty()
        || original.base_ref.is_empty()
        || original.head_ref.chars().any(char::is_control)
        || original.base_ref.chars().any(char::is_control)
        || identity.login.trim().is_empty()
        || identity.login.chars().any(char::is_control)
        || identity.account_id.is_none_or(|id| !positive(id))
        || !matches!(original.pr.state.as_str(), "open" | "closed" | "merged")
        || original.merged != (original.pr.state == "merged")
        || (original.merged && original.pr.draft)
    {
        return Err("Original principal/ref/full comparison/lifecycle is missing or inconsistent; no state inferred".into());
    }
    match (command, fields) {
        (PrStateCommand::Lock, PrStateFields::Lock { .. }) => {}
        (
            PrStateCommand::Revert,
            PrStateFields::Revert {
                title,
                body,
                draft: _,
            },
        ) => {
            if title.trim().is_empty() || title.len() > 4096 || body.len() > MAX_BODY_BYTES {
                return Err("Reverting PR needs a nonempty title <=4 KiB and description <=65536 UTF8 bytes".into());
            }
            bytes = bytes
                .checked_add(title.len())
                .and_then(|n| n.checked_add(body.len()))
                .ok_or("Local field byte budget overflow")?;
        }
        (PrStateCommand::Lock | PrStateCommand::Revert, _) => {
            return Err("This state operation needs its exact typed fields".into())
        }
        (_, PrStateFields::None) => {}
        _ => return Err("Unselected fields are not allowed for this state operation".into()),
    }
    if bytes > MAX_INPUT_BYTES {
        return Err("Local state-intent metadata exceeds 128 KiB before cloning".into());
    }
    let target = PrWriteTarget::new(identity, original).map_err(|e| e.to_string())?;
    let mut values = BTreeMap::from([
        ("scope-host".into(), target.scope().host.clone()),
        ("principal".into(), target.scope().login.clone()),
        ("repository".into(), target.repository().to_owned()),
        ("pr-node".into(), target.node_id().to_owned()),
        ("pr-number".into(), target.number().to_string()),
        ("base-sha".into(), original.base_sha.clone()),
        ("head-ref".into(), original.head_ref.clone()),
        ("base-ref".into(), original.base_ref.clone()),
        ("state".into(), original.pr.state.clone()),
        ("draft".into(), original.pr.draft.to_string()),
        ("merged".into(), original.merged.to_string()),
    ]);
    match fields {
        PrStateFields::Lock { reason } => {
            values.insert(
                "lock-reason".into(),
                reason.map_or("none", PrStateLockReason::label).into(),
            );
        }
        PrStateFields::Revert { title, body, draft } => {
            values.insert("title".into(), title.clone());
            values.insert("body".into(), body.clone());
            values.insert("revert-draft".into(), draft.to_string());
        }
        PrStateFields::None => {}
    }
    Ok((
        target,
        PrWriteIntent {
            command: command.key().into(),
            selected_resource: "pr".into(),
            observed_head: original.head_sha.clone(),
            values,
        },
    ))
}
fn snapshot_disabled(command: PrStateCommand, original: &PrSummary) -> Option<String> {
    use PrStateCommand::*;
    let reason = match command {
        DeleteRemoteHead => Some("Remote-only deletion transport is not admitted; no CAS or local branch deletion is assumed"),
        Draft | Ready if original.pr.state != "open" => Some("Draft/readiness changes need an open original PR"),
        Draft if original.pr.draft => Some("No change • loaded snapshot already says draft"),
        Ready if !original.pr.draft => Some("No change • loaded snapshot already says ready"),
        Close if original.pr.state != "open" => Some("Only an open PR can be closed without merging"),
        Reopen if original.pr.state != "closed" || original.merged => Some("Only a closed, unmerged PR can be reopened"),
        Revert if !original.merged => Some("Revert needs a merged original PR"),
        Draft | Ready | Enqueue | UpdateMerge | UpdateRebase
            if original.pr.state != "open" => Some("This operation needs an open original PR"),
        _ if (command.merges() || command.enables_auto() || command == Enqueue)
            && (original.pr.state != "open" || original.pr.draft) => Some("Merge/auto/queue admission needs an open, ready original PR"),
        _ => None,
    };
    reason.map(str::to_owned)
}
fn effect_notice(command: PrStateCommand) -> &'static str {
    use PrStateCommand::*;
    match command {
        Draft => "Effect: convert only this open PR to draft; no branch update or publication of review comments.",
        Ready => "Effect: mark only this draft PR ready for review; readiness is not approval or merge.",
        Close => "Effect: close this PR without merging or deleting any branch.",
        Reopen => "Effect: reopen this closed, unmerged PR; do not restore a merged PR as open.",
        Lock | Unlock => "Effect: change only the conversation lock; comments/body/state are not replaced.",
        Merge | Squash | Rebase => "Effect: request this merge method at the exact original head. A queue-required target may enter a queue instead; queued is not merged. No admin bypass, queue priority or branch deletion is requested.",
        EnableAutoMerge | EnableAutoSquash | EnableAutoRebase => "Effect: request auto-merge configuration at the original head; enabled is not merged. Queue method overrides must be explained from genuine source.",
        DisableAutoMerge => "Effect: disable the exact current auto-merge request; no merge or queue cancellation is implied.",
        Enqueue => "Effect: request normal-priority queue admission at the original head; no jump/admin bypass. Queue-generated head may differ; admitted is not merged.",
        Dequeue => "Effect: remove only the exact observed current queue entry; no branch deletion or merge is implied.",
        UpdateMerge | UpdateRebase => "Effect: update the remote PR head with the selected base method. Acceptance is not completion; a later changed head is an observation, not proof this request caused it.",
        Revert => "Effect: create a new traceable reverting PR; the original remains merged and the new PR is not automatically merged.",
        DeleteRemoteHead => "Effect decision open: remote branch only. Never use gh merge --delete-branch, which also changes a local branch. API read checks are not atomic compare-and-delete.",
    }
}
fn source_requirements(command: PrStateCommand) -> Vec<PrStateSourceRequirement> {
    use PrStateSourceRequirement::*;
    let mut out = vec![
        FreshPrincipalAndCanonicalAccess,
        ExactOriginalComparisonAndLifecycle,
        NullableCallerCapability,
        DurableStateFilterReadRepair,
    ];
    if command.merges() || command.enables_auto() || command == PrStateCommand::Enqueue {
        out.extend([RepositoryMergeSettings, RequiredChecksAndRulesCoverage]);
    }
    match command {
        PrStateCommand::Dequeue => out.push(CurrentQueueEntry),
        PrStateCommand::DisableAutoMerge => out.push(CurrentAutoMergeRequest),
        PrStateCommand::Lock | PrStateCommand::Unlock => out.push(ExactIssueLinkAndLockState),
        PrStateCommand::UpdateMerge | PrStateCommand::UpdateRebase => {
            out.extend([ExactHeadRepositoryAndRef, SupportedUpdateMethod]);
        }
        PrStateCommand::Revert => out.push(SupportedTraceableRevert),
        PrStateCommand::DeleteRemoteHead => out.extend([
            ExactHeadRepositoryAndRef,
            RemoteOnlyDeletionTransportDecision,
        ]),
        _ => {}
    }
    out
}
fn escaped(value: &str) -> String {
    value.chars().flat_map(char::escape_default).collect()
}
#[derive(Default)]
struct Rows {
    rows: Vec<String>,
    bytes: usize,
}
impl Rows {
    fn push(&mut self, value: &str) -> Result<(), String> {
        if value.len() > MAX_PREPARED_BYTES.saturating_sub(self.bytes) {
            return Err("Prepared state-intent UTF8 byte budget exceeded".into());
        }
        for line in crate::ui::notice_lines(&sanitize_terminal(value), 68) {
            if self.rows.len() == MAX_ROWS {
                return Err("Prepared state-intent row budget exceeded".into());
            }
            let mut text = String::new();
            for span in line.spans {
                text.push_str(&span.content);
            }
            self.bytes = self
                .bytes
                .checked_add(text.len())
                .filter(|n| *n <= MAX_PREPARED_BYTES)
                .ok_or("Prepared state-intent UTF8 byte budget exceeded")?;
            self.rows.push(text);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrStateRecordedStatus {
    ConfirmedEffect,
    AcceptedPending,
    UnknownOutcome,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RecordEnvelope {
    version: u8,
    command: PrStateCommand,
    principal: u64,
    host: String,
    repository: String,
    pr_node: String,
    pr_number: u64,
    original_head: String,
    original_base: String,
    intent_sha256: String,
    disposition: RecordedDisposition,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum RecordedDisposition {
    Confirmed { effect: RecordedEffect },
    AcceptedPending { acceptance: RecordedAcceptance },
    UnknownOutcome,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum RecordedEffect {
    DraftSet {
        draft: bool,
    },
    StateSet {
        state: String,
    },
    ConversationLock {
        issue_id: u64,
        issue_node: String,
        locked: bool,
        reason: RequiredNullable<PrStateLockReason>,
    },
    Merged {
        commit_sha: String,
    },
    AutoMergeConfigured {
        method: String,
    },
    AutoMergeDisabled,
    QueueEntryAdmitted {
        entry_id: String,
    },
    QueueEntryRemoved {
        entry_id: String,
    },
    RevertingPrCreated {
        pr_id: u64,
        pr_node: String,
        number: u64,
        url: String,
        draft: bool,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
enum RecordedAcceptance {
    BranchUpdate {
        method: String,
    },
    MergeRequest {
        request_id: RequiredNullable<String>,
    },
    QueueAdmission,
    AutoMergeEnable,
}
// The decoder also compares the exact input shape with its canonical typed
// shape, requiring nullable keys to be present rather than treating omission as
// an observed null/default. This wrapper keeps that policy explicit in types.
#[derive(Serialize, Deserialize)]
#[serde(transparent)]
struct RequiredNullable<T>(Option<T>);

/// Decoded recorded metadata, NOT a provider confirmation, access proof, journal
/// permit or ACK token. A real writer must derive receipts from opaque producer
/// observations; a JSON decode cannot create that authority.
pub struct PrStateRecordedDisposition {
    data: RecordEnvelope,
    status: PrStateRecordedStatus,
    label: &'static str,
}
impl fmt::Debug for PrStateRecordedDisposition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PrStateRecordedDisposition")
            .field("status", &self.status)
            .field("authority", &"none: recorded metadata")
            .finish()
    }
}
impl PrStateRecordedDisposition {
    /// Exact closed-shape/intent validation only. The caller must read this from
    /// the exact bounded durable original and verify its producer/current scope.
    pub fn decode_recorded(encoded: &str, plan: &PrStatePlan) -> Result<Self, String> {
        if encoded.len() > MAX_RECORD_BYTES {
            return Err("Recorded state disposition exceeds 8 KiB UTF8 before JSON decode".into());
        }
        let data: RecordEnvelope = serde_json::from_str(encoded)
            .map_err(|_| "Invalid closed recorded state disposition".to_owned())?;
        let input_shape: serde_json::Value = serde_json::from_str(encoded)
            .map_err(|_| "Invalid recorded state metadata shape".to_owned())?;
        let typed_shape = serde_json::to_value(&data)
            .map_err(|_| "Invalid typed recorded state metadata shape".to_owned())?;
        if input_shape != typed_shape {
            return Err("Recorded state metadata has missing nullable or extra fields; no defaults or unknown state inferred".into());
        }
        let principal = plan
            .target
            .scope()
            .login
            .strip_prefix("user-id:")
            .and_then(|id| id.parse::<u64>().ok())
            .ok_or("Local state plan has no positive principal")?;
        if data.version != 1
            || data.command != plan.command
            || data.principal != principal
            || data.host != plan.target.scope().host
            || data.repository != plan.target.repository()
            || data.pr_node != plan.target.node_id()
            || data.pr_number != plan.target.number()
            || data.original_head != plan.intent.observed_head
            || data.original_base != plan.intent.values["base-sha"]
            || data.intent_sha256 != plan.digest
        {
            return Err("Recorded disposition differs from the exact original operation/target/principal/comparison/fingerprint".into());
        }
        let (status, label) = validate_disposition(&data, plan)?;
        Ok(Self {
            data,
            status,
            label,
        })
    }
    pub fn status(&self) -> PrStateRecordedStatus {
        self.status
    }
    pub fn label(&self) -> &'static str {
        self.label
    }
    /// Accepted/unknown remain blocking. This does not clear a Confirmed marker
    /// either; only the real shared journal owns any state transition.
    pub fn is_unresolved(&self) -> bool {
        matches!(
            self.status,
            PrStateRecordedStatus::AcceptedPending | PrStateRecordedStatus::UnknownOutcome
        )
    }
    /// Configuration/admission outcomes never mean merged. Stored data alone
    /// still cannot establish a current live effect or permission.
    pub fn records_merge_effect(&self) -> bool {
        matches!(
            &self.data.disposition,
            RecordedDisposition::Confirmed {
                effect: RecordedEffect::Merged { .. }
            }
        )
    }
}
fn validate_disposition(
    data: &RecordEnvelope,
    plan: &PrStatePlan,
) -> Result<(PrStateRecordedStatus, &'static str), String> {
    use PrStateCommand::*;
    if plan.disabled_reason.is_some()
        && !matches!(data.disposition, RecordedDisposition::UnknownOutcome)
    {
        return Err(
            "Recorded effect/acceptance contradicts the disabled original local intent".into(),
        );
    }
    match &data.disposition {
        RecordedDisposition::UnknownOutcome => Ok((
            PrStateRecordedStatus::UnknownOutcome,
            "Recorded unknown outcome • original protected • no resend",
        )),
        RecordedDisposition::AcceptedPending { acceptance } => {
            let compatible = match acceptance {
                RecordedAcceptance::BranchUpdate { method } => {
                    (data.command == UpdateMerge && method == "merge")
                        || (data.command == UpdateRebase && method == "rebase")
                }
                RecordedAcceptance::MergeRequest { request_id } => {
                    data.command.merges() && request_id.0.as_deref().is_none_or(opaque_identifier)
                }
                RecordedAcceptance::QueueAdmission => {
                    data.command == Enqueue || data.command.merges()
                }
                RecordedAcceptance::AutoMergeEnable => {
                    data.command.enables_auto() || data.command.merges()
                }
            };
            if !compatible {
                return Err("Accepted metadata does not match the original operation".into());
            }
            Ok((PrStateRecordedStatus::AcceptedPending,
                "Recorded request accepted • effect pending/unverified • original protected • no resend"))
        }
        RecordedDisposition::Confirmed { effect } => {
            let (valid, label) = match effect {
                RecordedEffect::DraftSet { draft } => (
                    (data.command == Draft && *draft) || (data.command == Ready && !draft),
                    "Recorded draft/readiness effect • not merged",
                ),
                RecordedEffect::StateSet { state } => (
                    (data.command == Close && state == "closed")
                        || (data.command == Reopen && state == "open"),
                    "Recorded open/closed effect • not a merge receipt",
                ),
                RecordedEffect::ConversationLock {
                    issue_id,
                    issue_node,
                    locked,
                    reason,
                } => {
                    let requested = plan.intent.values.get("lock-reason").map(String::as_str);
                    let exact = if data.command == Lock {
                        *locked
                            && requested == Some(reason.0.map_or("none", PrStateLockReason::label))
                    } else {
                        data.command == Unlock && !locked && reason.0.is_none()
                    };
                    (
                        exact
                            && positive(*issue_id)
                            && opaque_identifier(issue_node)
                            && issue_node != &data.pr_node,
                        "Recorded conversation lock effect • not merged",
                    )
                }
                RecordedEffect::Merged { commit_sha } => (
                    data.command.merges() && valid_sha(commit_sha),
                    "Recorded merged effect • local/remote branch deletion not requested",
                ),
                RecordedEffect::AutoMergeConfigured { method } => (
                    (data.command.enables_auto() || data.command.merges())
                        && data.command.method_name() == Some(method.as_str()),
                    "Recorded auto-merge enabled • not merged",
                ),
                RecordedEffect::AutoMergeDisabled => (
                    data.command == DisableAutoMerge,
                    "Recorded auto-merge disabled • no merge implied",
                ),
                RecordedEffect::QueueEntryAdmitted { entry_id } => (
                    (data.command == Enqueue || data.command.merges())
                        && opaque_identifier(entry_id),
                    "Recorded queue admission • not merged",
                ),
                RecordedEffect::QueueEntryRemoved { entry_id } => (
                    data.command == Dequeue && opaque_identifier(entry_id),
                    "Recorded queue removal • no merge implied",
                ),
                RecordedEffect::RevertingPrCreated {
                    pr_id,
                    pr_node,
                    number,
                    url,
                    draft,
                } => {
                    let canonical_suffix = format!("/pull/{number}");
                    let original_url_same_scope = url
                        .strip_prefix(&format!("https://{}/", data.host))
                        .and_then(|path| path.strip_suffix(&canonical_suffix))
                        .is_some_and(|repository| {
                            repository.eq_ignore_ascii_case(&data.repository)
                        });
                    (data.command == Revert && positive(*pr_id) && positive(*number)
                        && *number != data.pr_number && opaque_identifier(pr_node)
                        && pr_node != &data.pr_node && original_url_same_scope
                        && plan.intent.values.get("revert-draft").is_some_and(|v|v==&draft.to_string()),
                     "Recorded reverting PR created • original remains merged • new PR not merged")
                }
            };
            if !valid {
                return Err("Confirmed metadata does not match the exact requested effect/identity; acceptance/head change is not confirmation".into());
            }
            Ok((PrStateRecordedStatus::ConfirmedEffect, label))
        }
    }
}
fn positive(value: u64) -> bool {
    value > 0 && value <= i64::MAX as u64
}
fn opaque_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/:_-.=+".contains(&b))
}

#[cfg(test)]
#[path = "pr_state_model_tests.rs"]
mod tests;
