//! Closed READ/ACK history, never a saved RunTarget or publication permit.
//! Current parent IDs bind history membership. Its H/B are NOT substituted for
//! old saved run/PR heads, which remain explicitly historical after pushes.
use super::*;
use crate::providers::gh::{
    CurrentCatalogueRepository, PrResourceParentV1, ScopedHistoricalRunReadV5,
};
use std::os::unix::fs::MetadataExt;
#[path = "history_row_fence_v6.rs"]
pub(super) mod row_fence_v6;
#[path = "run_observed_ack_v8.rs"]
mod run_ack_v8;
pub(crate) use run_ack_v8::ActionsRunHistoryAckV8;
#[path = "dispatch_history_v7.rs"]
mod dispatch_v7;
#[path = "workflow_remote_history_v9.rs"]
mod workflow_remote_v9;
#[path = "workflow_history_v6.rs"]
mod workflow_v6;
pub(crate) use dispatch_v7::{
    dispatch_acceptance_available_v7, ActionsDispatchAckV7, ActionsHistoricalDispatchKeyV7,
};
pub(crate) use workflow_remote_v9::{
    workflow_remote_ack_available_v9, ActionsHistoricalWorkflowRemoteKeyV9,
    ActionsWorkflowRemoteAckV9,
};
pub(crate) use workflow_v6::{
    workflow_ack_available_v6, ActionsHistoricalWorkflowKeyV6, ActionsWorkflowAckV6,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct BindingV5 {
    scope: Scope,
    repository_id: u64,
    repository_node: String,
    pr: Option<(u64, String, u64)>,
}
impl BindingV5 {
    fn actual(
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        parent: Option<&PrResourceParentV1>,
    ) -> Result<Self> {
        let scope = Scope::github(&identity.host, identity.account_id)?;
        let row = current.repository();
        let node = row
            .node_id
            .as_ref()
            .filter(|node| !node.is_empty() && node.len() <= 256)
            .context("Actual repository node unavailable")?;
        if row.id == 0 || row.permissions.pull != Some(true) {
            bail!("Actual original repository unavailable");
        }
        if parent.is_some_and(|p| {
            p.repository_id() != row.id
                || p.repository_node() != node
                || p.repository() != row.full_name
        }) {
            bail!("Actual current PR and repository differ");
        }
        Ok(Self {
            scope,
            repository_id: row.id,
            repository_node: node.clone(),
            pr: parent.map(|p| (p.database_id(), p.node_id().to_owned(), p.number())),
        })
    }
    fn matches(&self, marker: &ActionsWriteMarker) -> bool {
        marker.target.scope == self.scope
            && marker
                .context
                .original_repository_v3
                .as_ref()
                .is_some_and(|repository| {
                    repository.repository_id == self.repository_id
                        && repository.repository_node == self.repository_node
                })
            && match (&self.pr, &marker.context.original_pr_v3) {
                (None, None) => true,
                (Some((id, node, number)), Some(old)) => {
                    *id == old.id && node == &old.node && *number == old.number
                }
                _ => false,
            }
    }
}
/// Descriptive private selection read from a validated original marker. It
/// cannot enter any existing live RunTarget or command factory.
pub(crate) struct ActionsHistoricalRunKeyV5 {
    reference: ActionsPublicationReference,
    repository_id: u64,
    repository_node: String,
    run_id: u64,
    node: String,
    workflow_id: u64,
    suite: Option<u64>,
}
impl ActionsHistoricalRunKeyV5 {
    pub(crate) fn reference(&self) -> &ActionsPublicationReference {
        &self.reference
    }
    pub(crate) fn repository_id(&self) -> u64 {
        self.repository_id
    }
    pub(crate) fn repository_node(&self) -> &str {
        &self.repository_node
    }
    pub(crate) fn run_id(&self) -> u64 {
        self.run_id
    }
    pub(crate) fn node(&self) -> &str {
        &self.node
    }
    pub(crate) fn workflow_id(&self) -> u64 {
        self.workflow_id
    }
    pub(crate) fn suite(&self) -> Option<u64> {
        self.suite
    }
}
pub(crate) struct ActionsHistorySnapshotV5 {
    path: PathBuf,
    file: (u64, u64),
    binding: BindingV5,
    references: Vec<ActionsPublicationReference>,
    rows: Vec<crate::actions_workspace::ActionsPublicationRow>,
    total: u64,
    complete: bool,
    selected: Option<ActionsWriteMarker>,
    generations: Option<Vec<i64>>,
    owned: usize,
    recorded_workflow_v9: bool,
}
pub(crate) struct ActionsHistoryAckV5 {
    path: PathBuf,
    file: (u64, u64),
    marker: ActionsWriteMarker,
    generations: Vec<i64>,
    key: ActionsHistoricalRunKeyV5,
    observed: ScopedHistoricalRunReadV5,
    row_fence_v6: row_fence_v6::HistoryRowFenceV6,
}
impl ActionsHistoryAckV5 {
    pub(crate) fn retained_row_fence_bytes_v6(&self) -> usize {
        self.row_fence_v6.retained_bytes()
    }
}
fn file(path: &Path) -> Result<(u64, u64)> {
    let metadata = std::fs::symlink_metadata(path)?;
    // SAFETY: Linux/macOS effective UID query does not mutate state.
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
    {
        bail!("Original history database is no longer its private original file");
    }
    Ok((metadata.dev(), metadata.ino()))
}
fn selected_key(marker: &ActionsWriteMarker) -> Result<ActionsHistoricalRunKeyV5> {
    let repository = marker
        .context
        .original_repository_v3
        .as_ref()
        .context("Original repository unavailable")?;
    let run = marker
        .context
        .original_run_v3
        .as_ref()
        .context("Original run metadata unavailable")?;
    Ok(ActionsHistoricalRunKeyV5 {
        reference: marker.publication_reference(),
        repository_id: repository.repository_id,
        repository_node: repository.repository_node.clone(),
        run_id: marker.context.run_id.context("Original run ID missing")?,
        node: run.run_node.clone(),
        workflow_id: run.workflow_id,
        suite: run.check_suite_id,
    })
}
/// The bounded reader has already validated the complete saved marker. Keep
/// legacy unresolved rows unchanged, while closing the SAME W9 terminal rows
/// that the count/select predicate includes. This is READ membership only.
fn listable_state_v9(marker: &ActionsWriteMarker) -> bool {
    matches!(
        marker.state,
        ActionsWriteState::Pending
            | ActionsWriteState::Uncertain
            | ActionsWriteState::AcceptedPending
    ) || (matches!(
        marker.state,
        ActionsWriteState::Confirmed | ActionsWriteState::Acknowledged
    ) && matches!(
        (
            marker.kind,
            marker
                .context
                .workflow_edit_v9
                .as_ref()
                .map(|edit| edit.phase)
        ),
        (
            ActionsWriteKind::PublishWorkflowBranch,
            Some(WorkflowRemotePhaseV9::Branch)
        ) | (
            ActionsWriteKind::CreateWorkflowPr,
            Some(WorkflowRemotePhaseV9::Pr)
        )
    ))
}
impl ActionsHistorySnapshotV5 {
    pub(crate) fn recorded_workflow_v9(&self) -> bool {
        self.recorded_workflow_v9
    }
    pub(crate) fn reference_for(&self, id: i64) -> Option<ActionsPublicationReference> {
        self.references
            .iter()
            .find(|reference| reference.attempt_id == id)
            .cloned()
    }
    pub(crate) fn take_rows(&mut self) -> Vec<crate::actions_workspace::ActionsPublicationRow> {
        std::mem::take(&mut self.rows)
    }
    pub(crate) fn total(&self) -> u64 {
        self.total
    }
    pub(crate) fn complete(&self) -> bool {
        self.complete
    }
    pub(crate) fn selected(&self) -> Option<&ActionsWriteMarker> {
        self.selected.as_ref()
    }
    pub(crate) fn key(&self) -> Result<ActionsHistoricalRunKeyV5> {
        selected_key(
            self.selected
                .as_ref()
                .context("Choose an actual original publication row")?,
        )
    }
    pub(crate) fn owned_bytes(&self) -> usize {
        self.owned
    }
}
impl ActionsWriteJournal {
    pub(crate) fn scoped_history_v5(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        parent: Option<&PrResourceParentV1>,
        selected: Option<&ActionsPublicationReference>,
    ) -> Result<ActionsHistorySnapshotV5> {
        if provider.host() != identity.host {
            bail!("History host/principal differ");
        }
        let binding = BindingV5::actual(identity, current, parent)?;
        let transaction = self.connection.unchecked_transaction()?;
        // Preflight the SAME SQL snapshot BEFORE collecting any JSON. Known
        // explicit reference reads do not require an all-history catalogue.
        const HISTORY_WHERE_V9:&str="host=?1 AND principal=?2 AND (state IN('pending','uncertain','accepted_pending') OR (kind_json IN('\"publish_workflow_branch\"','\"create_workflow_pr\"') AND state IN('confirmed','acknowledged')))";
        let (total,bytes):(u64,i64)=transaction.query_row(&format!("SELECT COUNT(*),COALESCE(SUM(length(CAST(target_json AS BLOB))+length(CAST(resources_json AS BLOB))+length(CAST(kind_json AS BLOB))+length(CAST(context_json AS BLOB))+length(CAST(options_json AS BLOB))+length(CAST(fingerprint_sha256 AS BLOB))+COALESCE(length(CAST(receipt_json AS BLOB)),0)),0) FROM actions_write_attempts WHERE {HISTORY_WHERE_V9}"),
            params![binding.scope.host,binding.scope.login],|row|Ok((row.get(0)?,row.get(1)?)))?;
        if selected.is_none() && !(0..=8 * 1024 * 1024).contains(&bytes) {
            bail!("History exceeds8MiB closed metadata preflight; incomplete is not absence");
        }
        let ids = if let Some(reference) = selected {
            vec![reference.attempt_id]
        } else {
            let mut statement=transaction.prepare(&format!("SELECT attempt_id FROM actions_write_attempts WHERE {HISTORY_WHERE_V9} ORDER BY attempt_id DESC LIMIT 256"))?;
            let values = statement
                .query_map(params![binding.scope.host, binding.scope.login], |row| {
                    row.get::<_, i64>(0)
                })?;
            values.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut references = Vec::new();
        let mut rows = Vec::new();
        let mut selected_marker = None;
        let mut recorded_workflow_v9 = false;
        let mut owned =
            std::mem::size_of::<ActionsHistorySnapshotV5>() + self.path.capacity() + 4096;
        for id in ids {
            // SAME existing parser and this transaction's coherent snapshot.
            // bounded_original owns a standalone transaction, so nesting that
            // helper here would reject every nonempty actual history page.
            row_fence_v6::preflight(&transaction, &binding.scope, id)?;
            let marker = read_attempt(&transaction, &binding.scope, id)?
                .context("Original history reference unavailable")?;
            availability::validate_saved_marker(provider, &marker)?;
            if !binding.matches(&marker) {
                if selected.is_some() {
                    bail!("Saved history belongs to another actual numeric repository/PR context");
                }
                continue;
            }
            if selected.is_some_and(|reference| marker.publication_reference() != *reference) {
                bail!("Original history reference changed");
            }
            let reference = marker.publication_reference();
            let details = if let Some(edit) = &marker.context.workflow_edit_v9 {
                vec![
                format!("Historical {:?} / exact ref {} / D {}",edit.phase,edit.head_ref,edit.committed_head),
                format!("Saved C {} / base {} / B {}",edit.original_commit,edit.base_ref,edit.base_commit),
                "Recorded remote facts / explicit local ACK are not workflow execution or causal proof; no restore/retry/resend.".into(),
            ]
            } else {
                vec![format!("Historical run {:?} / attempt {:?} / H {}",marker.context.run_id,marker.context.attempt,
                marker.context.head_sha.as_deref().unwrap_or("unavailable")),
                "Saved H/PR H/B are historical, not current source/permission. No restore/resend/approval.".into()]
            };
            let mut heading = format!(
                "Original #{} {:?} {:?}",
                marker.attempt_id, marker.state, marker.kind
            );
            let role = if marker.context.workflow_edit_v9.is_some() {
                match marker.state {
                    ActionsWriteState::Confirmed => {
                        recorded_workflow_v9 = true;
                        heading = format!("[REMOTE OBSERVED; execution UNVERIFIED] {heading}");
                        crate::theme::SemanticToken::Reference
                    }
                    ActionsWriteState::Acknowledged => {
                        recorded_workflow_v9 = true;
                        heading = format!("[LOCAL ACK; effect UNVERIFIED] {heading}");
                        crate::theme::SemanticToken::Muted
                    }
                    _ => crate::theme::SemanticToken::Pending,
                }
            } else {
                crate::theme::SemanticToken::Pending
            };
            owned = owned
                .checked_add(
                    reference.fingerprint_sha256.capacity()
                        + heading.capacity()
                        + details.capacity() * std::mem::size_of::<String>()
                        + details.iter().map(String::capacity).sum::<usize>()
                        + std::mem::size_of::<crate::actions_workspace::ActionsPublicationRow>()
                        + std::mem::size_of::<ActionsPublicationReference>(),
                )
                .context("History retained accounting overflow")?;
            if owned > 480 * 1024 {
                bail!("Closed history references/presentation exceed512KiB; no truncated completeness claim");
            }
            references.push(reference);
            rows.push(crate::actions_workspace::ActionsPublicationRow {
                attempt_id: marker.attempt_id,
                heading,
                details,
                role,
            });
            if selected.is_some() {
                selected_marker = Some(marker);
            }
        }
        // Account vector spare capacity before accepting. Selected marker has
        // its separately preflighted128KiB metadata plus256KiB parser headroom.
        owned = owned
            .saturating_add(
                references.capacity() * std::mem::size_of::<ActionsPublicationReference>(),
            )
            .saturating_add(
                rows.capacity()
                    * std::mem::size_of::<crate::actions_workspace::ActionsPublicationRow>(),
            );
        if owned > 512 * 1024 {
            bail!("Closed history retained reference capacity exceeds512KiB");
        }
        let generations = selected_marker
            .as_ref()
            .map(|marker| generations(&transaction, &marker.target))
            .transpose()?;
        transaction.commit()?;
        let selected_bytes = if selected_marker.is_some() {
            512 * 1024
        } else {
            0
        };
        Ok(ActionsHistorySnapshotV5 {
            path: self.path.clone(),
            file: file(&self.path)?,
            binding,
            references,
            rows,
            total,
            complete: selected.is_none() && total <= 256,
            selected: selected_marker,
            generations,
            owned: owned + selected_bytes,
            recorded_workflow_v9,
        })
    }
    pub(crate) fn validate_history_v5(&self, snapshot: &ActionsHistorySnapshotV5) -> Result<()> {
        if self.path != snapshot.path || file(&self.path)? != snapshot.file {
            bail!("Original history file changed");
        }
        if let Some(marker) = &snapshot.selected {
            self.validate_scoped_marker_saved_v3(marker)?;
            if snapshot.generations.as_ref()
                != Some(&generations(&self.connection, &marker.target)?)
            {
                bail!("Original history generation changed");
            }
        } else {
            for reference in &snapshot.references {
                let marker = bounded_original::read_checked_attempt(
                    &self.connection,
                    &snapshot.binding.scope,
                    reference.attempt_id,
                )?
                .context("Original history changed")?;
                if !snapshot.binding.matches(&marker)
                    || marker.publication_reference() != *reference
                    || !listable_state_v9(&marker)
                {
                    bail!("Original history row changed; prepare its actual current page again");
                }
            }
        }
        Ok(())
    }
    pub(crate) fn prepare_history_acceptance_v5(
        &self,
        snapshot: &ActionsHistorySnapshotV5,
        observed: ScopedHistoricalRunReadV5,
        provider: &GithubProvider,
        cancel: &CancellationToken,
    ) -> Result<ActionsHistoryAckV5> {
        self.validate_history_v5(snapshot)?;
        let marker = snapshot
            .selected
            .as_ref()
            .context("Actual original row required")?;
        if marker.state != ActionsWriteState::AcceptedPending
            || !matches!(
                marker.kind,
                ActionsWriteKind::DeleteRun | ActionsWriteKind::DeleteLogs
            )
            || marker
                .receipt
                .as_ref()
                .is_none_or(|receipt| match marker.kind {
                    ActionsWriteKind::DeleteRun => {
                        receipt.kind != ActionsReceiptKind::RunDeletionAccepted
                    }
                    ActionsWriteKind::DeleteLogs => receipt.kind != ActionsReceiptKind::LogsDeleted,
                    _ => true,
                })
            || !observed.matches(provider, &selected_key(marker)?, cancel)
        {
            bail!("Only an actual fresh same-source inspected deletion/log acceptance can be acknowledged; unknown writes stay protected");
        }
        let key = selected_key(marker)?;
        let row_fence_v6 = row_fence_v6::HistoryRowFenceV6::capture(&self.connection, marker)?;
        Ok(ActionsHistoryAckV5 {
            path: self.path.clone(),
            file: snapshot.file,
            marker: marker.clone(),
            key,
            generations: snapshot
                .generations
                .clone()
                .context("Original generations unavailable")?,
            observed,
            row_fence_v6,
        })
    }
    pub(crate) fn validate_history_ack_v5(&self, ack: &ActionsHistoryAckV5) -> Result<()> {
        if self.path != ack.path
            || file(&self.path)? != ack.file
            || generations(&self.connection, &ack.marker.target)? != ack.generations
        {
            bail!("Original history acceptance changed");
        }
        self.validate_scoped_marker_saved_v3(&ack.marker)
    }
    pub(crate) fn acknowledge_history_acceptance_v5(
        &mut self,
        provider: &GithubProvider,
        ack: ActionsHistoryAckV5,
        identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> Result<()> {
        // Full bounded metadata validation/preparation was BEFORE final UID.
        // After it, only SAME Immediate local release CAS/advance and fixed
        // scope/ref/generation/token checks occur, not a new body/JSON parse.
        if self.path != ack.path
            || file(&self.path)? != ack.file
            || Scope::github(&identity.host, identity.account_id)? != ack.marker.target.scope
            || !ack.observed.matches(provider, &ack.key, cancel)
        {
            bail!("Original final history scope/read changed");
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if !ack.observed.matches(provider, &ack.key, cancel)
            || generations(&transaction, &ack.marker.target)? != ack.generations
        {
            bail!("Original history token/generation changed while waiting for the ACK lock");
        }
        ack.row_fence_v6.acknowledge(&transaction, &ack.marker)?;
        advance(&transaction, &ack.marker.target)?;
        if !ack.observed.matches(provider, &ack.key, cancel) {
            bail!("Original historical observation changed before local acceptance commit");
        }
        transaction.commit()?;
        Ok(())
    }
}
#[cfg(test)]
mod list_states_v9 {
    use super::*;
    #[test]
    fn task54_workflow_edit_v9_list_terminal_state_exception_is_only_validated_branch_or_pr_kind_phase(
    ) {
        use std::os::unix::fs::PermissionsExt;
        for phase in [WorkflowRemotePhaseV9::Branch, WorkflowRemotePhaseV9::Pr] {
            let directory = tempfile::Builder::new()
                .permissions(std::fs::Permissions::from_mode(0o700))
                .tempdir()
                .unwrap();
            let mut journal =
                ActionsWriteJournal::open(&directory.path().join("cache.sqlite3")).unwrap();
            let context = ActionsImmutableContext {
                workflow_commit: Some("a".repeat(40)),
                original_repository_v3: Some(ActionsOriginalRepositoryContextV3 {
                    repository_id: 10,
                    repository_node: "BASE10".into(),
                    original_name: "old/base".into(),
                }),
                original_workflow_v6: Some(ActionsOriginalWorkflowContextV6 {
                    workflow_id: 7,
                    node: "WF7".into(),
                    path: ".github/workflows/ci.yml".into(),
                }),
                workflow_edit_v9: Some(ActionsOriginalWorkflowEditContextV9 {
                    phase,
                    head_repository_id: 20,
                    head_repository_node: "HEAD20".into(),
                    head_original_name: "old/head".into(),
                    head_current_name: "new/head".into(),
                    head_ref: "workflow/edit".into(),
                    base_ref: "main".into(),
                    original_commit: "a".repeat(40),
                    committed_head: "d".repeat(40),
                    base_commit: "b".repeat(40),
                }),
                ..Default::default()
            };
            let target = ActionsWriteTarget::from_original_context_v3(
                &Scope::github("github.invalid", Some(42)).unwrap(),
                &ActionsTarget::Workflow {
                    repository: "new/base".into(),
                    id: 7,
                },
                &context,
            )
            .unwrap();
            let intent = ActionsWriteIntent::new(
                if phase == WorkflowRemotePhaseV9::Branch {
                    ActionsWriteKind::PublishWorkflowBranch
                } else {
                    ActionsWriteKind::CreateWorkflowPr
                },
                context,
                Default::default(),
                b"not authority",
            )
            .unwrap();
            let permit = journal.begin(&target, &intent).unwrap();
            journal.claim(&permit, &target, &intent).unwrap();
            let receipt = ActionsReceipt {
                kind: if phase == WorkflowRemotePhaseV9::Branch {
                    ActionsReceiptKind::WorkflowBranchObserved
                } else {
                    ActionsReceiptKind::WorkflowPrObserved
                },
                resource_id: Some(if phase == WorkflowRemotePhaseV9::Branch {
                    20
                } else {
                    91
                }),
                observed_attempt: None,
                observed_run_id: None,
                original_dispatch_run_v7: None,
                workflow_remote_v9: Some(if phase == WorkflowRemotePhaseV9::Branch {
                    WorkflowRemoteReceiptV9::Branch {
                        head_repository_id: 20,
                        head_repository_node: "HEAD20".into(),
                        head_ref: "workflow/edit".into(),
                        committed_head: "d".repeat(40),
                    }
                } else {
                    WorkflowRemoteReceiptV9::Pr {
                        base_repository_id: 10,
                        base_repository_node: "BASE10".into(),
                        pr_id: 9901,
                        pr_node: "PR9901".into(),
                        number: 91,
                        base_ref: "main".into(),
                        base_commit: "b".repeat(40),
                        head_repository_id: 20,
                        head_repository_node: "HEAD20".into(),
                        head_ref: "workflow/edit".into(),
                        committed_head: "d".repeat(40),
                    }
                }),
            };
            journal.mark_confirmed(&permit, &receipt).unwrap();
            let original = bounded_original::read_checked_attempt(
                &journal.connection,
                target.scope(),
                permit.marker().attempt_id,
            )
            .unwrap()
            .unwrap();
            for state in [
                ActionsWriteState::Pending,
                ActionsWriteState::Uncertain,
                ActionsWriteState::AcceptedPending,
                ActionsWriteState::Confirmed,
                ActionsWriteState::Acknowledged,
                ActionsWriteState::TransitionObserved,
            ] {
                let mut marker = original.clone();
                marker.state = state;
                assert_eq!(
                    listable_state_v9(&marker),
                    state != ActionsWriteState::TransitionObserved
                );
                for kind in [
                    ActionsWriteKind::RerunAll,
                    ActionsWriteKind::RerunFailed,
                    ActionsWriteKind::RerunJob,
                    ActionsWriteKind::Cancel,
                    ActionsWriteKind::ForceCancel,
                    ActionsWriteKind::DeleteRun,
                    ActionsWriteKind::DeleteLogs,
                    ActionsWriteKind::ApproveFork,
                    ActionsWriteKind::EnableWorkflow,
                    ActionsWriteKind::DisableWorkflow,
                    ActionsWriteKind::DispatchWorkflow,
                    ActionsWriteKind::DeleteArtifact,
                    ActionsWriteKind::ApproveDeployment,
                    ActionsWriteKind::RejectDeployment,
                    ActionsWriteKind::DeleteCache,
                    ActionsWriteKind::SetSecret,
                    ActionsWriteKind::DeleteSecret,
                    ActionsWriteKind::SetVariable,
                    ActionsWriteKind::DeleteVariable,
                    ActionsWriteKind::EditPolicy,
                    ActionsWriteKind::EditEnvironment,
                    ActionsWriteKind::RunnerLabels,
                    ActionsWriteKind::RemoveRunner,
                    ActionsWriteKind::EditRunnerGroup,
                    ActionsWriteKind::GroupRunners,
                    ActionsWriteKind::GroupRepositories,
                    ActionsWriteKind::CreateRunnerGroup,
                    ActionsWriteKind::DeleteRunnerGroup,
                ] {
                    marker.kind = kind;
                    assert_eq!(
                        listable_state_v9(&marker),
                        matches!(
                            state,
                            ActionsWriteState::Pending
                                | ActionsWriteState::Uncertain
                                | ActionsWriteState::AcceptedPending
                        ),
                        "Unrelated kind {kind:?} cannot borrow W9 terminal exception"
                    );
                }
            }
            for state in [
                ActionsWriteState::Confirmed,
                ActionsWriteState::Acknowledged,
            ] {
                let mut marker = original.clone();
                marker.state = state;
                marker.context.workflow_edit_v9.as_mut().unwrap().phase =
                    if phase == WorkflowRemotePhaseV9::Branch {
                        WorkflowRemotePhaseV9::Pr
                    } else {
                        WorkflowRemotePhaseV9::Branch
                    };
                assert!(!listable_state_v9(&marker));
                marker.context.workflow_edit_v9 = None;
                assert!(!listable_state_v9(&marker));
            }
            let after = bounded_original::read_checked_attempt(
                &journal.connection,
                target.scope(),
                original.attempt_id,
            )
            .unwrap()
            .unwrap();
            assert_eq!(
                (
                    &after.attempt_id,
                    (
                        &after.target.scope,
                        serde_json::to_vec(&after.target.target).unwrap(),
                        &after.target.resources
                    ),
                    &after.kind,
                    &after.context,
                    &after.options,
                    &after.fingerprint_sha256,
                    &after.state,
                    &after.receipt,
                    &after.created_at
                ),
                (
                    &original.attempt_id,
                    (
                        &original.target.scope,
                        serde_json::to_vec(&original.target.target).unwrap(),
                        &original.target.resources
                    ),
                    &original.kind,
                    &original.context,
                    &original.options,
                    &original.fingerprint_sha256,
                    &original.state,
                    &original.receipt,
                    &original.created_at
                ),
                "Membership tests never mutate original journal"
            );
        }
    }
}
