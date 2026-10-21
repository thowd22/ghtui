//! Restart-safe Actions publication store. Worker-only; no automatic sends.
//!
//! Exported for coordinated source verification. Native controller/restart wiring
//! remains a separate gate; source availability alone does not complete a workflow.
use crate::{
    providers::{
        gh::{
            AcceptedDispatchTransition, AcceptedRunTransition, ActionsObservation, ActionsTarget,
            DeploymentTransition, GithubProvider, GroupAssociation, HostIdentity, RunMutation,
        },
        runner::CancellationToken,
    },
    storage::{now, Cache, Scope},
};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
pub(crate) use scoped_claim_v3::ActionsOwnedDispatchHistoryV7;
pub(crate) use scoped_claim_v3::ActionsOwnedWorkflowRemoteHistoryV9;
pub(crate) use scoped_claim_v3::{ActionsOwnedRunHistoryV8, ActionsOwnedWorkflowHistoryV6};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};
mod availability;
mod bounded_candidates;
mod bounded_original;
mod force_escalation_v5;
mod scoped_history_v5;
mod workflow_capacity_v9;
pub(crate) use scoped_history_v5::{
    dispatch_acceptance_available_v7, ActionsDispatchAckV7, ActionsHistoricalDispatchKeyV7,
};
pub(crate) use scoped_history_v5::{
    workflow_ack_available_v6, ActionsHistoricalWorkflowKeyV6, ActionsWorkflowAckV6,
};
pub(crate) use scoped_history_v5::{
    workflow_remote_ack_available_v9, ActionsHistoricalWorkflowRemoteKeyV9,
    ActionsWorkflowRemoteAckV9,
};
pub(crate) use scoped_history_v5::{
    ActionsHistoricalRunKeyV5, ActionsHistoryAckV5, ActionsHistorySnapshotV5,
    ActionsRunHistoryAckV8,
};
mod scope_target;
mod scoped_claim_v3;
mod scoped_context_v3;
mod workflow_edit_context_v9;
pub use workflow_edit_context_v9::{
    ActionsOriginalWorkflowEditContextV9, WorkflowRemotePhaseV9, WorkflowRemoteReceiptV9,
};
mod scoped_inspection_v3;
pub use availability::{ActionsInspectionDisposition, ActionsUnresolvedPage};
pub use force_escalation_v5::ActionsForceEscalationContextV5;
pub(crate) use force_escalation_v5::ScopedForceEscalationV5;
pub(crate) use scoped_claim_v3::ActionsClaimStampV3;
pub use scoped_context_v3::{
    ActionsOriginalDispatchContextV7, ActionsOriginalPrContextV3,
    ActionsOriginalRepositoryContextV3, ActionsOriginalRunContextV3,
    ActionsOriginalWorkflowContextV6,
};

/// Only remote effects belong here. Filtering, browsing and archive export do not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionsWriteKind {
    RerunAll,
    RerunFailed,
    RerunJob,
    Cancel,
    ForceCancel,
    DeleteRun,
    DeleteLogs,
    ApproveFork,
    EnableWorkflow,
    DisableWorkflow,
    DispatchWorkflow,
    PublishWorkflowBranch,
    CreateWorkflowPr,
    DeleteArtifact,
    ApproveDeployment,
    RejectDeployment,
    DeleteCache,
    SetSecret,
    DeleteSecret,
    SetVariable,
    DeleteVariable,
    EditPolicy,
    EditEnvironment,
    RunnerLabels,
    RemoveRunner,
    EditRunnerGroup,
    GroupRunners,
    GroupRepositories,
    CreateRunnerGroup,
    DeleteRunnerGroup,
}
impl ActionsWriteKind {
    fn deployment(self) -> bool {
        matches!(self, Self::ApproveDeployment | Self::RejectDeployment)
    }
}

/// Closed, non-content context. Names/values/comments and secret lengths are absent.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsImmutableContext {
    pub run_id: Option<u64>,
    pub attempt: Option<u64>,
    pub head_sha: Option<String>,
    pub workflow_commit: Option<String>,
    pub job_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_repository_v3: Option<ActionsOriginalRepositoryContextV3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_run_v3: Option<ActionsOriginalRunContextV3>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_pr_v3: Option<ActionsOriginalPrContextV3>,
    /// Descriptive original accepted Cancel; never a restored escalation permit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub force_escalation_v5: Option<ActionsForceEscalationContextV5>,
    /// Historical workflow identity, never a current API/write capability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_workflow_v6: Option<ActionsOriginalWorkflowContextV6>,
    /// Safe descriptive ref/protocol metadata only. Never input bytes/digests,
    /// a live schema, permission, source or restored publication capability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_dispatch_v7: Option<ActionsOriginalDispatchContextV7>,
    /// Safe observed repository/ref/C-D facts only, never edited private text
    /// or restored source, permission, request or publication capability.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_edit_v9: Option<ActionsOriginalWorkflowEditContextV9>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsSafeOptions {
    pub debug_logging: bool,
    pub enabled: Option<bool>,
    pub selected_ids: Vec<u64>,
}

#[derive(Clone, Debug)]
pub struct ActionsWriteTarget {
    scope: Scope,
    target: ActionsTarget,
    resources: Vec<String>,
}
impl ActionsWriteTarget {
    pub fn new(
        provider: &GithubProvider,
        identity: &HostIdentity,
        target: &ActionsTarget,
    ) -> Result<Self> {
        if !identity.host.eq_ignore_ascii_case(provider.host()) {
            bail!("Actions target and verified principal belong to different hosts");
        }
        let scope = Scope::github(identity.host.to_ascii_lowercase(), identity.account_id)?;
        Self::from_scope(&scope, target)
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn target(&self) -> &ActionsTarget {
        &self.target
    }
    pub fn resource_keys(&self) -> &[String] {
        &self.resources
    }
    fn same_resources(&self, other: &Self) -> bool {
        self.scope == other.scope && self.resources == other.resources
    }
}

#[derive(Clone)]
pub struct ActionsWriteIntent {
    pub kind: ActionsWriteKind,
    pub context: ActionsImmutableContext,
    pub options: ActionsSafeOptions,
    // This binds edited body/variable/secret bytes only while the process lives.
    // It is never serialized, persisted, copied into a draft, or exposed by Debug.
    transient_binding: [u8; 32],
}
impl fmt::Debug for ActionsWriteIntent {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output
            .debug_struct("ActionsWriteIntent")
            .field("kind", &self.kind)
            .field("context", &self.context)
            .field("options", &self.options)
            .field("transient_binding", &"[redacted memory-only binding]")
            .finish()
    }
}
impl ActionsWriteIntent {
    pub fn new(
        kind: ActionsWriteKind,
        context: ActionsImmutableContext,
        options: ActionsSafeOptions,
        transient_payload: &[u8],
    ) -> Result<Self> {
        if transient_payload.len() > 1024 * 1024 {
            bail!("Transient Actions publication input exceeds the 1 MiB binding budget");
        }
        validate_context(&context)?;
        if options.selected_ids.len() > 1000
            || options.selected_ids.contains(&0)
            || options.selected_ids.iter().collect::<BTreeSet<_>>().len()
                != options.selected_ids.len()
        {
            bail!("Actions options require bounded distinct positive canonical IDs");
        }
        Ok(Self {
            kind,
            context,
            options,
            transient_binding: Sha256::digest(transient_payload).into(),
        })
    }
    pub fn fingerprint(&self, target: &ActionsWriteTarget) -> Result<String> {
        validate_context(&self.context)?;
        if self.context.original_repository_v3.is_some() {
            let expected = ActionsWriteTarget::from_original_context_v3(
                &target.scope,
                &target.target,
                &self.context,
            )?;
            if !expected.same_resources(target) {
                bail!("Original numeric Actions metadata requires every exact original/current/numeric resource alias");
            }
        }
        validate_options(&self.options)?;
        validate_kind_target(self.kind, &target.target)?;
        if self.context.original_workflow_v6.is_some()
            && self.context.original_dispatch_v7.is_none()
            && self.context.workflow_edit_v9.is_none()
            && (!matches!(
                self.kind,
                ActionsWriteKind::EnableWorkflow | ActionsWriteKind::DisableWorkflow
            ) || self.options.enabled != Some(self.kind == ActionsWriteKind::EnableWorkflow)
                || self.options.debug_logging
                || !self.options.selected_ids.is_empty())
        {
            bail!("Original workflow metadata only describes its exact enable/disable choice");
        }
        if self.context.original_dispatch_v7.is_some()
            && (self.kind != ActionsWriteKind::DispatchWorkflow
                || self.options.enabled.is_some()
                || self.options.debug_logging
                || !self.options.selected_ids.is_empty())
        {
            bail!("Original Dispatch metadata only describes its exact dispatch choice");
        }
        if self.context.force_escalation_v5.is_some() && self.kind != ActionsWriteKind::ForceCancel
        {
            bail!("Only an explicit original Force cancellation may describe an escalation");
        }
        workflow_edit_context_v9::validate_intent(self)?;
        if let ActionsTarget::Run { id, .. } = &target.target {
            if self.context.run_id != Some(*id)
                || self.context.attempt.is_none()
                || self.context.head_sha.is_none()
            {
                bail!("Run effects require the original exact run ID, attempt and immutable head");
            }
            if self.kind == ActionsWriteKind::RerunJob && self.context.job_id.is_none() {
                bail!("Selected-job rerun requires its exact original canonical job ID");
            }
        }
        if self.kind == ActionsWriteKind::DispatchWorkflow && self.context.workflow_commit.is_none()
        {
            bail!("Workflow dispatch requires the inspected immutable workflow commit");
        }
        if self.kind.deployment() {
            let ActionsTarget::DeploymentReview {
                run_id,
                attempt,
                head_sha,
                ..
            } = &target.target
            else {
                bail!("Deployment decisions require an exact typed deployment target");
            };
            if self.context.run_id != Some(*run_id)
                || self.context.attempt != Some(*attempt)
                || self.context.head_sha.as_deref() != Some(head_sha)
            {
                bail!("Publication context differs from the original deployment attempt/head");
            }
        }
        // The schema is a whitelist, not a map of arbitrary form/body/secret values.
        let bytes = serde_json::to_vec(&(
            self.kind,
            &target.target,
            &target.resources,
            &self.context,
            &self.options,
        ))?;
        if bytes.len() > 64 * 1024 {
            bail!("Closed Actions publication descriptor exceeds its metadata budget");
        }
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionsWriteState {
    Pending,
    Uncertain,
    AcceptedPending,
    Confirmed,
    Acknowledged,
    TransitionObserved,
}
/// Static receipt facts only; never response JSON, body, value or diagnostic text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionsReceiptKind {
    RunTransition,
    RunDeleted,
    LogsDeleted,
    WorkflowState,
    Dispatch,
    ArtifactDeleted,
    DeploymentAccepted,
    CacheDeleted,
    SecretMetadata,
    VariableMetadata,
    SettingsMetadata,
    /// Documented HTTP204 acceptance, NOT observed deletion/causality.
    RunDeletionAccepted,
    WorkflowBranchObserved,
    WorkflowPrObserved,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsReceipt {
    pub kind: ActionsReceiptKind,
    pub resource_id: Option<u64>,
    pub observed_attempt: Option<u64>,
    /// Dispatch correlation is a canonical observed run, never workflow completion.
    #[serde(default)]
    pub observed_run_id: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_dispatch_run_v7: Option<ActionsDispatchRunReceiptV7>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_remote_v9: Option<WorkflowRemoteReceiptV9>,
}
/// Only descriptive actual POST200+prepaid raw Run facts. Never a live target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsDispatchRunReceiptV7 {
    pub repository_id: u64,
    pub repository_node: String,
    pub run_id: u64,
    pub run_node: String,
    pub workflow_id: u64,
    pub attempt: u64,
    pub head_sha: String,
    pub event: String,
    pub check_suite_id: u64,
}
#[derive(Clone, Debug)]
pub struct ActionsWriteMarker {
    pub attempt_id: i64,
    pub target: ActionsWriteTarget,
    pub kind: ActionsWriteKind,
    pub context: ActionsImmutableContext,
    pub options: ActionsSafeOptions,
    pub fingerprint_sha256: String,
    pub state: ActionsWriteState,
    pub receipt: Option<ActionsReceipt>,
    pub created_at: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionsPublicationReference {
    pub attempt_id: i64,
    pub fingerprint_sha256: String,
}
impl ActionsWriteMarker {
    pub fn publication_reference(&self) -> ActionsPublicationReference {
        ActionsPublicationReference {
            attempt_id: self.attempt_id,
            fingerprint_sha256: self.fingerprint_sha256.clone(),
        }
    }
    fn blocks(&self, target: &ActionsWriteTarget, kind: ActionsWriteKind) -> bool {
        if self.state != ActionsWriteState::AcceptedPending {
            return matches!(
                self.state,
                ActionsWriteState::Pending | ActionsWriteState::Uncertain
            );
        }
        if let ActionsTarget::DeploymentReview {
            run_id,
            attempt,
            head_sha,
            environment_ids,
            ..
        } = &self.target.target
        {
            let ActionsTarget::DeploymentReview {
                run_id: other_run,
                attempt: other_attempt,
                head_sha: other_head,
                environment_ids: other_ids,
                ..
            } = &target.target
            else {
                return false;
            };
            return kind.deployment()
                && run_id == other_run
                && attempt == other_attempt
                && head_sha == other_head
                && environment_ids.iter().any(|id| other_ids.contains(id));
        }
        // Accepted run/dispatch correlation is conservative until its own typed
        // transition proof is available; it is never called completed from text.
        true
    }
}
pub struct ActionsWritePermit {
    issuer: PathBuf,
    marker: ActionsWriteMarker,
    transient_binding: [u8; 32],
    used: AtomicBool,
    // Some-only original transaction generations. None performs no new SQL,
    // serializes nothing and keeps exact legacy fingerprints/claim policy.
    scoped_begin_generations_v3: Option<Vec<i64>>,
    force_escalation_v5: Option<std::sync::Arc<ScopedForceEscalationV5>>,
    workflow_pr_prior_branch_v9:
        Option<std::sync::Arc<scoped_claim_v3::WorkflowPrPriorBranchFenceV9>>,
}
impl fmt::Debug for ActionsWritePermit {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output
            .debug_struct("ActionsWritePermit")
            .field("marker", &self.marker)
            .finish_non_exhaustive()
    }
}
impl ActionsWritePermit {
    pub fn marker(&self) -> &ActionsWriteMarker {
        &self.marker
    }
}
/// Opaque, process-local proof retaining both durable and producer generations.
#[derive(Debug)]
pub struct ActionsJournalInspection {
    issuer: PathBuf,
    marker: ActionsWriteMarker,
    generations: Vec<i64>,
    observation: ActionsObservation,
}
impl ActionsJournalInspection {
    pub fn marker(&self) -> &ActionsWriteMarker {
        &self.marker
    }
    pub fn observation(&self) -> &ActionsObservation {
        &self.observation
    }
}
pub struct ActionsWriteJournal {
    connection: Connection,
    path: PathBuf,
}
impl ActionsWriteJournal {
    pub fn open(path: &Path) -> Result<Self> {
        // Cache owns secure filesystem opening/sidecar checks, not this store.
        drop(Cache::open(path)?);
        let path = std::fs::canonicalize(path)?;
        let mut connection = Connection::open(&path)?;
        crate::storage::busy_v1::install(&connection)?;
        connection.execute_batch("PRAGMA foreign_keys=ON; PRAGMA synchronous=FULL;")?;
        // TASK-102 follow-up: an open with nothing to create must not queue for the write
        // lock behind a concurrent draft save's slow commit (the confirmation of a local-only
        // intent failed with "database is locked" that way). The check itself only reads.
        if Self::schema_present(&connection)? {
            return Ok(Self { connection, path });
        }
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch("CREATE TABLE IF NOT EXISTS actions_write_attempts(
          attempt_id INTEGER PRIMARY KEY AUTOINCREMENT,host TEXT NOT NULL,principal TEXT NOT NULL,
          target_json TEXT NOT NULL,resources_json TEXT NOT NULL,kind_json TEXT NOT NULL,
          context_json TEXT NOT NULL,options_json TEXT NOT NULL,fingerprint_sha256 TEXT NOT NULL,
          state TEXT NOT NULL CHECK(state IN('pending','uncertain','accepted_pending','confirmed','acknowledged','transition_observed')),
          claimed INTEGER NOT NULL DEFAULT 0 CHECK(claimed IN(0,1)),receipt_json TEXT,created_at INTEGER NOT NULL);
          CREATE TABLE IF NOT EXISTS actions_write_resources(
          host TEXT NOT NULL,principal TEXT NOT NULL,resource_key TEXT NOT NULL,attempt_id INTEGER NOT NULL,
          PRIMARY KEY(host,principal,resource_key,attempt_id),
          FOREIGN KEY(attempt_id) REFERENCES actions_write_attempts(attempt_id));
          CREATE TABLE IF NOT EXISTS actions_write_generations(
          host TEXT NOT NULL,principal TEXT NOT NULL,resource_key TEXT NOT NULL,generation INTEGER NOT NULL,
          PRIMARY KEY(host,principal,resource_key));
          CREATE INDEX IF NOT EXISTS actions_write_unresolved_scope ON actions_write_attempts(host,principal,state,attempt_id);")?;
        transaction.commit()?;
        Ok(Self { connection, path })
    }
    /// Whether every table and index [`Self::open`] creates already exists, by name, as its
    /// `CREATE ... IF NOT EXISTS` statements would decide.
    fn schema_present(connection: &Connection) -> Result<bool> {
        const OBJECTS: [(&str, &str); 4] = [
            ("table", "actions_write_attempts"),
            ("table", "actions_write_resources"),
            ("table", "actions_write_generations"),
            ("index", "actions_write_unresolved_scope"),
        ];
        let present: i64 = connection.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE (type,name) IN
            (VALUES (?1,?2),(?3,?4),(?5,?6),(?7,?8))",
            params![
                OBJECTS[0].0,
                OBJECTS[0].1,
                OBJECTS[1].0,
                OBJECTS[1].1,
                OBJECTS[2].0,
                OBJECTS[2].1,
                OBJECTS[3].0,
                OBJECTS[3].1
            ],
            |row| row.get(0),
        )?;
        Ok(present == OBJECTS.len() as i64)
    }
    pub fn blocking(
        &self,
        target: &ActionsWriteTarget,
        kind: ActionsWriteKind,
    ) -> Result<Vec<ActionsWriteMarker>> {
        let transaction = self.connection.unchecked_transaction()?;
        let markers = blocking(&transaction, target, kind)?;
        transaction.commit()?;
        Ok(markers)
    }
    pub fn begin(
        &mut self,
        target: &ActionsWriteTarget,
        intent: &ActionsWriteIntent,
    ) -> Result<ActionsWritePermit> {
        self.begin_with_force_v5(target, intent, None)
    }
    fn begin_with_force_v5(
        &mut self,
        target: &ActionsWriteTarget,
        intent: &ActionsWriteIntent,
        escalation: Option<std::sync::Arc<ScopedForceEscalationV5>>,
    ) -> Result<ActionsWritePermit> {
        self.begin_closed_v9(target, intent, escalation, None)
    }
    pub(crate) fn begin_workflow_pr_after_branch_v9(
        &mut self,
        target: &ActionsWriteTarget,
        intent: &ActionsWriteIntent,
        branch: &crate::scoped_actions_workflow_edit_v9::WorkflowPublishedBranchV9,
    ) -> Result<ActionsWritePermit> {
        self.begin_closed_v9(target, intent, None, Some(branch))
    }
    fn begin_closed_v9(
        &mut self,
        target: &ActionsWriteTarget,
        intent: &ActionsWriteIntent,
        escalation: Option<std::sync::Arc<ScopedForceEscalationV5>>,
        branch: Option<&crate::scoped_actions_workflow_edit_v9::WorkflowPublishedBranchV9>,
    ) -> Result<ActionsWritePermit> {
        let fingerprint = intent.fingerprint(target)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        scoped_context_v3::before_begin(&transaction, target, intent)?;
        let workflow_pr_prior_branch_v9 = branch
            .map(|branch| {
                scoped_claim_v3::WorkflowPrPriorBranchFenceV9::prepare_before_begin_v9(
                    &transaction,
                    &self.path,
                    target,
                    intent,
                    branch,
                )
            })
            .transpose()?;
        let blocked = blocking(&transaction, target, intent.kind)?;
        if let Some(proof) = &escalation {
            force_escalation_v5::before_begin(
                &transaction,
                &self.path,
                target,
                intent,
                proof,
                &blocked,
            )?;
        } else if intent.context.force_escalation_v5.is_some() || !blocked.is_empty() {
            bail!("Actions publication unresolved: inspect and explicitly acknowledge its exact result before another overlapping effect");
        }
        advance(&transaction, target)?;
        let created_at = now();
        transaction.execute("INSERT INTO actions_write_attempts(host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,'pending',?9)",params![target.scope.host,target.scope.login,serde_json::to_string(&target.target)?,serde_json::to_string(&target.resources)?,serde_json::to_string(&intent.kind)?,serde_json::to_string(&intent.context)?,serde_json::to_string(&intent.options)?,fingerprint,created_at])?;
        let attempt_id = transaction.last_insert_rowid();
        for key in &target.resources {
            transaction.execute("INSERT INTO actions_write_resources(host,principal,resource_key,attempt_id) VALUES(?1,?2,?3,?4)",params![target.scope.host,target.scope.login,key,attempt_id])?;
        }
        let scoped_begin_generations_v3 = if intent.context.original_repository_v3.is_some() {
            Some(generations(&transaction, target)?)
        } else {
            None
        };
        transaction.commit()?;
        Ok(ActionsWritePermit {
            issuer: self.path.clone(),
            marker: ActionsWriteMarker {
                attempt_id,
                target: target.clone(),
                kind: intent.kind,
                context: intent.context.clone(),
                options: intent.options.clone(),
                fingerprint_sha256: fingerprint,
                state: ActionsWriteState::Pending,
                receipt: None,
                created_at,
            },
            transient_binding: intent.transient_binding,
            used: AtomicBool::new(false),
            scoped_begin_generations_v3,
            force_escalation_v5: escalation,
            workflow_pr_prior_branch_v9,
        })
    }
    pub fn claim(
        &mut self,
        permit: &ActionsWritePermit,
        target: &ActionsWriteTarget,
        intent: &ActionsWriteIntent,
    ) -> Result<()> {
        if permit.issuer != self.path
            || !permit.marker.target.same_resources(target)
            || permit.marker.fingerprint_sha256 != intent.fingerprint(target)?
            || permit.transient_binding != intent.transient_binding
        {
            bail!("Actions permit does not match this journal, account, target or exact in-memory payload");
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        scoped_claim_v3::before_claim(&transaction, permit)?;
        let changed = transaction.execute("UPDATE actions_write_attempts SET claimed=1 WHERE attempt_id=?1 AND host=?2 AND principal=?3 AND fingerprint_sha256=?4 AND state='pending' AND claimed=0",params![permit.marker.attempt_id,target.scope.host,target.scope.login,permit.marker.fingerprint_sha256])?;
        if changed != 1 {
            bail!("Actions permit is stale or already claimed; no effect is authorized");
        }
        if permit.used.swap(true, Ordering::AcqRel) {
            bail!("Actions publication permit was already used");
        }
        advance(&transaction, target)?;
        transaction.commit()?;
        Ok(())
    }
    pub fn mark_uncertain(&mut self, permit: &ActionsWritePermit) -> Result<()> {
        self.finish(permit, ActionsWriteState::Uncertain, None)
    }
    pub fn mark_confirmed(
        &mut self,
        permit: &ActionsWritePermit,
        receipt: &ActionsReceipt,
    ) -> Result<()> {
        self.finish(permit, ActionsWriteState::Confirmed, Some(receipt))
    }
    pub fn mark_accepted_pending(
        &mut self,
        permit: &ActionsWritePermit,
        receipt: &ActionsReceipt,
    ) -> Result<()> {
        self.finish(permit, ActionsWriteState::AcceptedPending, Some(receipt))
    }
    fn finish(
        &mut self,
        permit: &ActionsWritePermit,
        state: ActionsWriteState,
        receipt: Option<&ActionsReceipt>,
    ) -> Result<()> {
        if permit.issuer != self.path {
            bail!("Actions permit belongs to another journal");
        }
        if receipt.is_some() && !permit.used.load(Ordering::Acquire) {
            bail!("Cannot record an accepted or confirmed effect that was never authorized");
        }
        if let Some(receipt) = receipt {
            validate_receipt(receipt)?;
            validate_receipt_target(&permit.marker, receipt)?;
            if state == ActionsWriteState::AcceptedPending
                && permit.marker.kind.deployment()
                && receipt.kind != ActionsReceiptKind::DeploymentAccepted
            {
                bail!("A pending deployment requires a typed accepted-decision receipt");
            }
        }
        let target = &permit.marker.target;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        scoped_claim_v3::before_finish(&transaction, permit)?;
        let changed = transaction.execute("UPDATE actions_write_attempts SET state=?1,receipt_json=?2 WHERE attempt_id=?3 AND host=?4 AND principal=?5 AND fingerprint_sha256=?6 AND state IN('pending','uncertain') AND (claimed=1 OR ?7=0)",params![state_name(state),receipt.map(serde_json::to_string).transpose()?,permit.marker.attempt_id,target.scope.host,target.scope.login,permit.marker.fingerprint_sha256,receipt.is_some()])?;
        if changed != 1 {
            bail!("Actions result changed or was already completed; durable protection remains");
        }
        advance(&transaction, target)?;
        transaction.commit()?;
        Ok(())
    }
    /// Any restored reference resolves its ORIGINAL attempt, even after field edits.
    /// This helper is not used for deliberately fresh forms without a reference.
    pub fn restored_publication(
        &self,
        target: &ActionsWriteTarget,
        reference: &ActionsPublicationReference,
    ) -> Result<ActionsWriteMarker> {
        if reference.attempt_id <= 0 || reference.fingerprint_sha256.len() != 64 {
            bail!("Restored Actions publication reference is invalid");
        }
        let marker = bounded_original::read_checked_attempt(
            &self.connection,
            &target.scope,
            reference.attempt_id,
        )?
        .context("Original Actions publication is unavailable; do not resend a restored draft")?;
        if !marker.target.same_resources(target)
            || marker.fingerprint_sha256 != reference.fingerprint_sha256
        {
            bail!("Restored Actions publication belongs to another original target/account");
        }
        Ok(marker)
    }
    /// Restore the ORIGINAL verified resource before new form fields (including
    /// empty secret inputs or changed heads) are prepared or validated. The UI
    /// must present this marker's original action/target, never the edited form.
    pub fn original_publication(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        reference: &ActionsPublicationReference,
    ) -> Result<ActionsWriteMarker> {
        if reference.attempt_id <= 0
            || reference.fingerprint_sha256.len() != 64
            || !reference
                .fingerprint_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            bail!("Original Actions publication reference is invalid; nothing may be resent");
        }
        let scope = Scope::github(identity.host.to_ascii_lowercase(), identity.account_id)?;
        let marker=bounded_original::read_checked_attempt(&self.connection,&scope,reference.attempt_id)?.context("Original Actions publication is unavailable for this verified principal; nothing may be resent")?;
        if marker.fingerprint_sha256 != reference.fingerprint_sha256 {
            bail!("Original Actions publication fingerprint differs; nothing may be resent");
        }
        let legacy = ActionsWriteTarget::new(provider, identity, &marker.target.target)?;
        let canonical = ActionsWriteTarget::from_original_context_v3(
            legacy.scope(),
            legacy.target(),
            &marker.context,
        )?;
        let closed = ActionsWriteIntent::new(
            marker.kind,
            marker.context.clone(),
            marker.options.clone(),
            b"",
        )?;
        if !canonical.same_resources(&marker.target)
            || closed.fingerprint(&canonical)? != marker.fingerprint_sha256
        {
            bail!("Original Actions publication resource/context proof is invalid; nothing may be resent");
        }
        if let Some(receipt) = &marker.receipt {
            validate_receipt(receipt)?;
            validate_receipt_target(&marker, receipt)?;
        } else if matches!(
            marker.state,
            ActionsWriteState::Confirmed
                | ActionsWriteState::AcceptedPending
                | ActionsWriteState::TransitionObserved
        ) {
            bail!("Original Actions result has no typed receipt; nothing may be resent");
        }
        Ok(marker)
    }
    pub fn prepare_inspection(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        attempt_id: i64,
        cancel: &CancellationToken,
    ) -> Result<ActionsJournalInspection> {
        let scope = Scope::github(identity.host.to_ascii_lowercase(), identity.account_id)?;
        let marker = bounded_original::read_checked_attempt(&self.connection, &scope, attempt_id)?
            .context("No Actions publication exists for this verified principal")?;
        availability::validate_saved_marker(provider, &marker)?;
        if marker.context.original_repository_v3.is_some() {
            bail!("Original scoped Actions inspection requires its genuine current numeric source and raw run proof; the legacy name-only inspector cannot adopt it");
        }
        if !matches!(
            marker.state,
            ActionsWriteState::Pending
                | ActionsWriteState::Uncertain
                | ActionsWriteState::AcceptedPending
        ) {
            bail!("This Actions publication no longer awaits inspection");
        }
        let target = ActionsWriteTarget::new(provider, identity, &marker.target.target)?;
        if !target.same_resources(&marker.target) {
            bail!("Canonical Actions resources changed; no saved alias is acknowledged");
        }
        let before = generations(&self.connection, &target)?;
        let observation = provider
            .inspect_actions_target(identity, &target.target, cancel)
            .map_err(anyhow::Error::msg)?;
        let observed_target =
            ActionsWriteTarget::new(provider, observation.identity(), observation.target())?;
        let mut observed_resources = observation.resource_keys().to_vec();
        observed_resources.sort();
        if !observed_target.same_resources(&target)
            || observed_resources != target.resources
            || !observation.identity().same_account(identity)
            || generations(&self.connection, &target)? != before
        {
            bail!("Actions account/publication changed during inspection; nothing was unlocked");
        }
        Ok(ActionsJournalInspection {
            issuer: self.path.clone(),
            marker,
            generations: before,
            observation,
        })
    }
    pub fn acknowledge(
        &mut self,
        provider: &GithubProvider,
        inspection: ActionsJournalInspection,
        current_identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> Result<()> {
        self.acknowledge_inner(provider, inspection, current_identity, cancel, false)
    }
    /// DISTINCT explicit acknowledgement of an accepted but unverified request.
    /// Only uncorrelated dispatch or unobservable logs proofs qualify. This never
    /// claims an effect/completion, resends anything, or releases unknown writes.
    pub fn acknowledge_inspected_acceptance(
        &mut self,
        provider: &GithubProvider,
        inspection: ActionsJournalInspection,
        current_identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> Result<()> {
        self.acknowledge_inner(provider, inspection, current_identity, cancel, true)
    }
    fn acknowledge_inner(
        &mut self,
        provider: &GithubProvider,
        inspection: ActionsJournalInspection,
        current_identity: &HostIdentity,
        cancel: &CancellationToken,
        acceptance_only: bool,
    ) -> Result<()> {
        let target = &inspection.marker.target;
        if inspection.issuer != self.path
            || Scope::github(
                current_identity.host.to_ascii_lowercase(),
                current_identity.account_id,
            )? != target.scope
            || !current_identity.same_account(inspection.observation.identity())
            || generations(&self.connection, target)? != inspection.generations
        {
            bail!("Actions inspection is stale or belongs to another database/account; protection remains");
        }
        let next = if acceptance_only {
            require_unverified_acceptance(&inspection.marker, &inspection.observation)?;
            ActionsWriteState::Acknowledged
        } else if inspection.marker.state == ActionsWriteState::AcceptedPending {
            require_accepted_transition(&inspection.marker, &inspection.observation)?;
            ActionsWriteState::TransitionObserved
        } else {
            ActionsWriteState::Acknowledged
        };
        let fresh = provider.identity(cancel).map_err(anyhow::Error::msg)?;
        if !fresh.same_account(current_identity)
            || !fresh.same_account(inspection.observation.identity())
        {
            bail!("Account changed before Actions acknowledgement; durable protection remains");
        }
        provider
            .acknowledge_actions_observation(inspection.observation)
            .map_err(anyhow::Error::msg)?;
        self.acknowledge_closed_v3(inspection.marker, inspection.generations, next)
    }
    /// SAME durable acknowledgement commit. The specialized caller must have
    /// consumed a genuine provider observation after its original source close.
    fn acknowledge_closed_v3(
        &mut self,
        marker: ActionsWriteMarker,
        expected: Vec<i64>,
        next: ActionsWriteState,
    ) -> Result<()> {
        let target = &marker.target;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if generations(&transaction, target)? != expected {
            bail!("Actions publication changed during acknowledgement; durable protection remains");
        }
        let changed = transaction.execute("UPDATE actions_write_attempts SET state=?1 WHERE attempt_id=?2 AND host=?3 AND principal=?4 AND state=?5",params![state_name(next),marker.attempt_id,target.scope.host,target.scope.login,state_name(marker.state)])?;
        if changed != 1 {
            bail!("Actions result changed before acknowledgement; protection remains");
        }
        advance(&transaction, target)?;
        transaction.commit()?;
        Ok(())
    }
}

fn require_unverified_acceptance(
    marker: &ActionsWriteMarker,
    observation: &ActionsObservation,
) -> Result<()> {
    if marker.state != ActionsWriteState::AcceptedPending {
        bail!("Acknowledging inspected acceptance requires an original typed accepted receipt; pending/uncertain markers are not acceptance");
    }
    let receipt = marker
        .receipt
        .as_ref()
        .context("Original accepted request has no typed receipt")?;
    validate_receipt(receipt)?;
    validate_receipt_target(marker, receipt)?;
    match marker.kind {
        ActionsWriteKind::DeleteLogs => {
            let transition=observation.run_transition(marker.context.run_id.context("Accepted logs request has no original run ID")?,marker.context.attempt.context("Accepted logs request has no original attempt")?,marker.context.head_sha.as_deref().context("Accepted logs request has no original immutable head")?,RunMutation::DeleteLogs).map_err(anyhow::Error::msg)?;
            if transition!=AcceptedRunTransition::LogsEffectNotObservable {
                bail!("Only an exact fresh metadata proof that cannot observe log deletion permits this distinct acceptance acknowledgement");
            }
        },
        ActionsWriteKind::DispatchWorkflow => {
            let ActionsTarget::Workflow {id,..}=marker.target.target() else {bail!("Accepted dispatch has no original workflow ID");};
            let transition=observation.dispatch_transition(*id,marker.context.workflow_commit.as_deref().context("Accepted dispatch lacks its original workflow commit")?,receipt.observed_run_id).map_err(anyhow::Error::msg)?;
            if transition!=AcceptedDispatchTransition::UncorrelatedAcceptance {
                bail!("A known correlated dispatch cannot be relabelled uncorrelated to bypass its original launch lease");
            }
        },
        _=>bail!("This original accepted effect does not qualify for unverified-acceptance acknowledgement"),
    }
    Ok(())
}

/// This resolves an original lease through an opaque fresh observation, never a
/// message string or a claim that the requested write caused its later state.
fn require_accepted_transition(
    marker: &ActionsWriteMarker,
    observation: &ActionsObservation,
) -> Result<()> {
    if marker.kind.deployment() {
        match observation.deployment_transition().map_err(anyhow::Error::msg)? {
            DeploymentTransition::StillPending { .. } => bail!("Accepted decision still has pending requested gates; it cannot be repeated or acknowledged as completed"),
            DeploymentTransition::GatesNoLongerPending { .. }
            | DeploymentTransition::OriginalAttemptAdvanced { .. } => return Ok(()),
        }
    }
    if marker.kind == ActionsWriteKind::DispatchWorkflow {
        let ActionsTarget::Workflow { id, .. } = marker.target.target() else {
            bail!("Accepted dispatch has no original canonical workflow target");
        };
        let commit = marker
            .context
            .workflow_commit
            .as_deref()
            .context("Accepted dispatch lacks its original workflow commit")?;
        let receipt = marker
            .receipt
            .as_ref()
            .context("Accepted dispatch lacks its original acceptance receipt")?;
        match observation.dispatch_transition(*id, commit, receipt.observed_run_id).map_err(anyhow::Error::msg)? {
            AcceptedDispatchTransition::CorrelatedRunObserved { .. } => return Ok(()),
            AcceptedDispatchTransition::UncorrelatedAcceptance => bail!("Dispatch acceptance is uncorrelated; this inspection proves neither launch nor completion and cannot release its lease"),
            AcceptedDispatchTransition::CorrelationNotObserved { .. } => bail!("Original dispatch run was not found in the observed catalogue; bounded absence is not a completed or deleted run"),
        }
    }
    let action = match marker.kind {
        ActionsWriteKind::RerunAll => RunMutation::RerunAll { debug: marker.options.debug_logging },
        ActionsWriteKind::RerunFailed => RunMutation::RerunFailed { debug: marker.options.debug_logging },
        ActionsWriteKind::RerunJob => RunMutation::RerunJob { job_id: marker.context.job_id.context("Accepted selected-job rerun lacks its original positive job ID")?, debug: marker.options.debug_logging },
        ActionsWriteKind::Cancel => RunMutation::Cancel,
        ActionsWriteKind::ForceCancel => RunMutation::ForceCancel,
        ActionsWriteKind::ApproveFork => RunMutation::ApproveFork,
        ActionsWriteKind::DeleteLogs => RunMutation::DeleteLogs,
        _ => bail!("This accepted effect has no implemented typed transition policy; no completed state is guessed"),
    };
    match observation.run_transition(
        marker.context.run_id.context("Accepted run effect lacks its original canonical run ID")?,
        marker.context.attempt.context("Accepted run effect lacks its original attempt")?,
        marker.context.head_sha.as_deref().context("Accepted run effect lacks its original immutable head")?,
        action,
    ).map_err(anyhow::Error::msg)? {
        AcceptedRunTransition::OriginalAttemptAdvanced { .. }
        | AcceptedRunTransition::RequestedCancellationObserved { .. }
        | AcceptedRunTransition::DifferentCompletionObserved { .. }
        | AcceptedRunTransition::ForkApprovalGatePassed { .. } => Ok(()),
        AcceptedRunTransition::StillPending { .. } => bail!("Accepted run effect still awaits the original requested transition; it cannot be repeated or acknowledged as completed"),
        AcceptedRunTransition::RunNotFoundInObservedCatalogue => bail!("Original run is missing only from the observed catalogue; bounded absence cannot release its accepted lease"),
        AcceptedRunTransition::LogsEffectNotObservable => bail!("Run metadata cannot prove logs were deleted; its accepted effect is not completed by this inspection"),
    }
}

fn validate_context(context: &ActionsImmutableContext) -> Result<()> {
    if [context.run_id, context.attempt, context.job_id]
        .into_iter()
        .flatten()
        .any(|id| id == 0 || id > i64::MAX as u64)
    {
        bail!("Actions immutable context requires positive bounded identities");
    }
    for sha in [&context.head_sha, &context.workflow_commit]
        .into_iter()
        .flatten()
    {
        if !matches!(sha.len(), 40 | 64) || !sha.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            bail!("Actions comparison context requires immutable Git identities");
        }
    }
    scoped_context_v3::validate(context)?;
    Ok(())
}
fn validate_receipt(receipt: &ActionsReceipt) -> Result<()> {
    workflow_edit_context_v9::validate_receipt(receipt)?;
    if let Some(run) = &receipt.original_dispatch_run_v7 {
        if receipt.kind != ActionsReceiptKind::Dispatch
            || receipt.observed_run_id != Some(run.run_id)
            || receipt.observed_attempt != Some(run.attempt)
            || [
                run.repository_id,
                run.run_id,
                run.workflow_id,
                run.attempt,
                run.check_suite_id,
            ]
            .into_iter()
            .any(|id| id == 0 || id > i64::MAX as u64)
            || [run.repository_node.as_str(), run.run_node.as_str()]
                .into_iter()
                .any(|node| {
                    node.is_empty() || node.len() > 256 || node.chars().any(char::is_control)
                })
            || !matches!(run.head_sha.len(), 40 | 64)
            || !run.head_sha.bytes().all(|byte| byte.is_ascii_hexdigit())
            || run.event != "workflow_dispatch"
        {
            bail!("Actual Dispatch receipt facts are invalid");
        }
    }
    if [
        receipt.resource_id,
        receipt.observed_attempt,
        receipt.observed_run_id,
    ]
    .into_iter()
    .flatten()
    .any(|id| id == 0 || id > i64::MAX as u64)
    {
        bail!("Actions receipt facts require positive bounded canonical IDs");
    }
    if receipt.observed_run_id.is_some() && receipt.kind != ActionsReceiptKind::Dispatch {
        bail!("Observed launch correlation only belongs to a typed dispatch receipt");
    }
    Ok(())
}
fn validate_options(options: &ActionsSafeOptions) -> Result<()> {
    if options.selected_ids.len() > 1000
        || options.selected_ids.contains(&0)
        || options.selected_ids.iter().collect::<BTreeSet<_>>().len() != options.selected_ids.len()
    {
        bail!("Actions options require bounded distinct positive canonical IDs");
    }
    Ok(())
}
fn validate_kind_target(kind: ActionsWriteKind, target: &ActionsTarget) -> Result<()> {
    use ActionsWriteKind::*;
    let valid = match kind {
        RerunAll | RerunFailed | RerunJob | Cancel | ForceCancel | DeleteRun | DeleteLogs
        | ApproveFork => matches!(target, ActionsTarget::Run { .. }),
        EnableWorkflow
        | DisableWorkflow
        | DispatchWorkflow
        | PublishWorkflowBranch
        | CreateWorkflowPr => {
            matches!(target, ActionsTarget::Workflow { .. })
        }
        DeleteArtifact => matches!(target, ActionsTarget::Artifact { .. }),
        ApproveDeployment | RejectDeployment => {
            matches!(target, ActionsTarget::DeploymentReview { .. })
        }
        DeleteCache => matches!(target, ActionsTarget::Cache { .. }),
        SetSecret | DeleteSecret => matches!(target, ActionsTarget::Secret { .. }),
        SetVariable | DeleteVariable => matches!(target, ActionsTarget::Variable { .. }),
        EditPolicy => matches!(target, ActionsTarget::Policy { .. }),
        EditEnvironment => matches!(target, ActionsTarget::EnvironmentProtection { .. }),
        RunnerLabels | RemoveRunner => matches!(target, ActionsTarget::Runner { .. }),
        EditRunnerGroup | DeleteRunnerGroup => matches!(target, ActionsTarget::RunnerGroup { .. }),
        GroupRunners => matches!(
            target,
            ActionsTarget::GroupAssociations {
                kind: GroupAssociation::Runners,
                ..
            }
        ),
        GroupRepositories => matches!(
            target,
            ActionsTarget::GroupAssociations {
                kind: GroupAssociation::Repositories,
                ..
            }
        ),
        CreateRunnerGroup => matches!(target, ActionsTarget::RunnerGroupCreation { .. }),
    };
    if !valid {
        bail!("Actions publication command does not match its canonical typed target");
    }
    Ok(())
}
fn validate_receipt_target(marker: &ActionsWriteMarker, receipt: &ActionsReceipt) -> Result<()> {
    workflow_edit_context_v9::validate_receipt_target(marker, receipt)?;
    if let Some(dispatch) = &marker.context.original_dispatch_v7 {
        if receipt.kind != ActionsReceiptKind::Dispatch
            || match dispatch.protocol.as_str() {
                "2026-03-10" => {
                    receipt.observed_run_id.is_none()
                        || receipt.observed_attempt.is_none()
                        || receipt.original_dispatch_run_v7.is_none()
                }
                "2022-11-28" => {
                    receipt.observed_run_id.is_some()
                        || receipt.observed_attempt.is_some()
                        || receipt.original_dispatch_run_v7.is_some()
                }
                _ => true,
            }
        {
            bail!("Original Dispatch typed receipt differs from its documented protocol");
        }
        if let Some(run) = &receipt.original_dispatch_run_v7 {
            if marker
                .context
                .original_repository_v3
                .as_ref()
                .is_none_or(|repo| {
                    repo.repository_id != run.repository_id
                        || repo.repository_node != run.repository_node
                })
                || marker
                    .context
                    .original_workflow_v6
                    .as_ref()
                    .is_none_or(|workflow| workflow.workflow_id != run.workflow_id)
                || dispatch.commit != run.head_sha
            {
                bail!("Actual Dispatch run receipt differs from original repository/workflow/definition");
            }
        }
    } else if receipt.original_dispatch_run_v7.is_some() {
        bail!("Actual native Dispatch receipt requires its exact original Dispatch context");
    }
    use ActionsWriteKind::*;
    let expected_kind = match marker.kind {
        RerunAll | RerunFailed | RerunJob | Cancel | ForceCancel | ApproveFork => {
            ActionsReceiptKind::RunTransition
        }
        DeleteRun if marker.context.original_repository_v3.is_some() => {
            ActionsReceiptKind::RunDeletionAccepted
        }
        DeleteRun => ActionsReceiptKind::RunDeleted,
        DeleteLogs => ActionsReceiptKind::LogsDeleted,
        EnableWorkflow | DisableWorkflow => ActionsReceiptKind::WorkflowState,
        DispatchWorkflow => ActionsReceiptKind::Dispatch,
        PublishWorkflowBranch => ActionsReceiptKind::WorkflowBranchObserved,
        CreateWorkflowPr => ActionsReceiptKind::WorkflowPrObserved,
        DeleteArtifact => ActionsReceiptKind::ArtifactDeleted,
        ApproveDeployment | RejectDeployment => ActionsReceiptKind::DeploymentAccepted,
        DeleteCache => ActionsReceiptKind::CacheDeleted,
        SetSecret | DeleteSecret => ActionsReceiptKind::SecretMetadata,
        SetVariable | DeleteVariable => ActionsReceiptKind::VariableMetadata,
        _ => ActionsReceiptKind::SettingsMetadata,
    };
    if receipt.kind != expected_kind {
        bail!("Actions receipt does not describe the authorized effect");
    }
    let expected_id = if marker.context.workflow_edit_v9.is_some() {
        // The paired W9 context/receipt was fully validated above, followed by
        // the SAME generic kind check. Its exact effect is the actual head
        // repository (Branch) or actual PR number, not workflow container ID.
        Some(
            match receipt
                .workflow_remote_v9
                .as_ref()
                .context("Actual typed W9 receipt missing")?
            {
                WorkflowRemoteReceiptV9::Branch {
                    head_repository_id, ..
                } => *head_repository_id,
                WorkflowRemoteReceiptV9::Pr { number, .. } => *number,
            },
        )
    } else {
        match &marker.target.target {
            ActionsTarget::Run { id, .. }
            | ActionsTarget::Workflow { id, .. }
            | ActionsTarget::Artifact { id, .. }
            | ActionsTarget::Cache { id, .. }
            | ActionsTarget::Runner { id, .. }
            | ActionsTarget::RunnerGroup { id, .. }
            | ActionsTarget::GroupAssociations { id, .. } => Some(*id),
            ActionsTarget::DeploymentReview { run_id, .. } => Some(*run_id),
            ActionsTarget::EnvironmentProtection { environment_id, .. } => Some(*environment_id),
            _ => None,
        }
    };
    if expected_id.is_some() && receipt.resource_id != expected_id {
        bail!("Actions receipt belongs to another canonical effect target");
    }
    Ok(())
}
fn state_name(state: ActionsWriteState) -> &'static str {
    match state {
        ActionsWriteState::Pending => "pending",
        ActionsWriteState::Uncertain => "uncertain",
        ActionsWriteState::AcceptedPending => "accepted_pending",
        ActionsWriteState::Confirmed => "confirmed",
        ActionsWriteState::Acknowledged => "acknowledged",
        ActionsWriteState::TransitionObserved => "transition_observed",
    }
}
fn generations(connection: &Connection, target: &ActionsWriteTarget) -> Result<Vec<i64>> {
    target.resources.iter().map(|key| {
        Ok(connection.query_row("SELECT generation FROM actions_write_generations WHERE host=?1 AND principal=?2 AND resource_key=?3",params![target.scope.host,target.scope.login,key],|row|row.get(0)).optional()?.unwrap_or(0))
    }).collect()
}
fn advance(transaction: &Transaction, target: &ActionsWriteTarget) -> Result<()> {
    for key in &target.resources {
        transaction.execute("INSERT INTO actions_write_generations(host,principal,resource_key,generation) VALUES(?1,?2,?3,1) ON CONFLICT(host,principal,resource_key) DO UPDATE SET generation=generation+1",params![target.scope.host,target.scope.login,key])?;
    }
    Ok(())
}
fn read_attempt(
    connection: &Connection,
    scope: &Scope,
    id: i64,
) -> Result<Option<ActionsWriteMarker>> {
    type Saved = (
        String,
        String,
        String,
        String,
        String,
        String,
        String,
        Option<String>,
        u64,
    );
    let row:Option<Saved> = connection.query_row("SELECT target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,receipt_json,created_at FROM actions_write_attempts WHERE host=?1 AND principal=?2 AND attempt_id=?3",params![scope.host,scope.login,id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?))).optional()?;
    row.map(|row| {
        let state = match row.6.as_str() {
            "pending" => ActionsWriteState::Pending,
            "uncertain" => ActionsWriteState::Uncertain,
            "accepted_pending" => ActionsWriteState::AcceptedPending,
            "confirmed" => ActionsWriteState::Confirmed,
            "acknowledged" => ActionsWriteState::Acknowledged,
            "transition_observed" => ActionsWriteState::TransitionObserved,
            _ => bail!("Unknown persisted Actions publication state"),
        };
        Ok(ActionsWriteMarker {
            attempt_id: id,
            target: ActionsWriteTarget {
                scope: scope.clone(),
                target: serde_json::from_str(&row.0)?,
                resources: serde_json::from_str(&row.1)?,
            },
            kind: serde_json::from_str(&row.2)?,
            context: serde_json::from_str(&row.3)?,
            options: serde_json::from_str(&row.4)?,
            fingerprint_sha256: row.5,
            state,
            receipt: row
                .7
                .map(|value| serde_json::from_str(&value))
                .transpose()?,
            created_at: row.8,
        })
    })
    .transpose()
}
fn blocking(
    connection: &Connection,
    target: &ActionsWriteTarget,
    kind: ActionsWriteKind,
) -> Result<Vec<ActionsWriteMarker>> {
    bounded_candidates::blocking(connection, target, kind)
}

#[cfg(test)]
#[path = "actions_write_journal/tests.rs"]
mod tests;
