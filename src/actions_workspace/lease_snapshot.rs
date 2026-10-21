//! Immutable worker-prepared leases survive local selection and control rebuilds.
//! Presentation only: the durable controller remains the authority before sending.
use super::*;
use crate::{
    actions_write_journal::{
        ActionsUnresolvedPage, ActionsWriteMarker, ActionsWriteState, ActionsWriteTarget,
    },
    storage::Scope,
};
use anyhow::{bail, Result};

#[derive(Clone, Debug)]
pub struct ActionsPublicationLeaseSnapshot {
    scope: Scope,
    markers: Arc<[ActionsWriteMarker]>,
    total: u64,
    complete: bool,
    unavailable: bool,
}
impl ActionsPublicationLeaseSnapshot {
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        use crate::providers::gh::workflow_capacity_v9 as b;
        let mut n = b::sum([
            b::owner::<Self>(),
            self.scope.host.capacity(),
            self.scope.login.capacity(),
            self.markers
                .len()
                .checked_mul(std::mem::size_of::<ActionsWriteMarker>())?,
            2 * std::mem::size_of::<usize>(),
        ])?;
        for marker in self.markers.iter() {
            n = n.checked_add(marker.workflow_owned_bytes_v9()?)?;
        }
        Some(n)
    }
    /// Run in the worker after positive account verification and a bounded SQL read.
    /// Closed metadata is checked here once, never parsed or hashed by rendering.
    pub fn prepare(identity: &HostIdentity, page: &ActionsUnresolvedPage) -> Result<Self> {
        let host = canonical_actions_host(&identity.host)?;
        let scope = Scope::github(host, identity.account_id)?;
        if page.markers.len() > 256
            || page.total < page.markers.len() as u64
            || page.complete != (page.total == page.markers.len() as u64)
        {
            bail!("Publication lease snapshot has invalid bounded coverage; no release inferred");
        }
        let mut ids = std::collections::BTreeSet::new();
        for marker in &page.markers {
            if marker.attempt_id <= 0
                || !ids.insert(marker.attempt_id)
                || marker.target.scope() != &scope
                || !matches!(
                    marker.state,
                    ActionsWriteState::Pending
                        | ActionsWriteState::Uncertain
                        | ActionsWriteState::AcceptedPending
                )
            {
                bail!("Publication lease snapshot has another account or invalid original marker");
            }
            marker.validate_metadata()?;
        }
        Ok(Self {
            scope,
            markers: page.markers.clone().into(),
            total: page.total,
            complete: page.complete,
            unavailable: false,
        })
    }
    /// A failed metadata read is not an empty journal. Retain that distinction
    /// through local selection and control rebuilds until a worker publishes a
    /// newly verified snapshot. No private record or producer error is retained.
    pub fn unavailable(identity: &HostIdentity) -> Result<Self> {
        let host = canonical_actions_host(&identity.host)?;
        Ok(Self {
            scope: Scope::github(host, identity.account_id)?,
            markers: Arc::from([]),
            total: 0,
            complete: false,
            unavailable: true,
        })
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    /// Worker-only exact-target lookups may discover leases omitted by the global
    /// bounded page. Retain those proofs for later local selection; unseen leases
    /// remain the durable controller's responsibility, never an absence inference.
    pub fn with_exact_markers(mut self, exact: &[ActionsWriteMarker]) -> Result<Self> {
        if self.unavailable {
            bail!("Publication metadata is unavailable; exact markers cannot establish coverage");
        }
        if exact.len() > 256 {
            bail!("Exact publication lease snapshot exceeds its bounded budget");
        }
        let mut markers = self.markers.to_vec();
        for marker in exact {
            if marker.attempt_id <= 0
                || marker.target.scope() != &self.scope
                || !matches!(
                    marker.state,
                    ActionsWriteState::Pending
                        | ActionsWriteState::Uncertain
                        | ActionsWriteState::AcceptedPending
                )
            {
                bail!("Exact publication lease has another account or invalid original marker");
            }
            marker.validate_metadata()?;
            if let Some(existing) = markers
                .iter()
                .find(|row| row.attempt_id == marker.attempt_id)
            {
                if existing.fingerprint_sha256 != marker.fingerprint_sha256 {
                    bail!("Original publication context changed during its lease snapshot");
                }
                // The earlier valid unresolved state remains conservative. A
                // later refresh/ACK rebuilds the snapshot rather than mutating it.
                continue;
            }
            markers.push(marker.clone());
            self.complete = false;
        }
        self.markers = markers.into();
        Ok(self)
    }
    pub fn retained_markers(&self) -> &[ActionsWriteMarker] {
        &self.markers
    }
    pub fn total(&self) -> u64 {
        self.total
    }
    pub fn complete(&self) -> bool {
        self.complete
    }
    pub fn is_unavailable(&self) -> bool {
        self.unavailable
    }
    /// Pure, bounded typed comparisons; no runner/provider construction, SQL,
    /// subprocess, JSON parsing, message parsing or hashing on local selection.
    pub fn blocking_reason(
        &self,
        workspace: &ActionsWorkspace,
        command: ActionsCommand,
    ) -> Option<String> {
        let current = Scope::github(
            workspace.identity.host.to_ascii_lowercase(),
            workspace.identity.account_id,
        )
        .ok()?;
        if current != self.scope {
            return None;
        }
        let kind = command.publication_kind()?;
        if self.unavailable {
            return Some("Publication view unavailable; original records retained. Inspect the exact original before another effect; nothing is automatically resent.".into());
        }
        let target = workspace.current_publication_target(command)?;
        let canonical = match ActionsWriteTarget::from_scope(&self.scope, &target) {
            Ok(target) => target,
            Err(_) => {
                return Some("The current selection has no valid canonical Actions target. Refresh and inspect the exact target before composing; no effect sent.".into());
            }
        };
        self.markers
            .iter()
            .find(|marker| marker.blocks_target(&canonical, kind))
            .map(|marker| {
                format!("Original attempt #{} remains {:?}; open Publications, inspect that exact result and explicitly acknowledge an eligible transition. Refresh never unlocks it.",marker.attempt_id,marker.state)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions_write_journal::{
        ActionsImmutableContext, ActionsReceipt, ActionsReceiptKind, ActionsSafeOptions,
        ActionsWriteIntent, ActionsWriteJournal, ActionsWriteKind,
    };
    use std::os::unix::fs::PermissionsExt;
    fn identity(id: u64) -> HostIdentity {
        HostIdentity {
            host: "github.invalid".into(),
            login: "octocat".into(),
            account_id: Some(id),
            gh_version: "fixture".into(),
        }
    }
    fn journal() -> (tempfile::TempDir, ActionsWriteJournal) {
        let directory = tempfile::Builder::new()
            .permissions(std::fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let journal = ActionsWriteJournal::open(&directory.path().join("cache.sqlite3")).unwrap();
        (directory, journal)
    }
    fn cache(id: u64) -> ActionCache {
        ActionCache {
            id,
            key: format!("cache-{id}"),
            ref_name: "refs/heads/main".into(),
            version: "fixture".into(),
            size_in_bytes: 42,
            created_at: "2026-10-13T12:00:00Z".into(),
            last_accessed_at: "2026-10-13T12:01:00Z".into(),
        }
    }
    fn pending_cache(journal: &mut ActionsWriteJournal) -> ActionsUnresolvedPage {
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let target = ActionsWriteTarget::from_scope(
            &scope,
            &ActionsTarget::Cache {
                repository: "example/repo".into(),
                id: 71,
            },
        )
        .unwrap();
        let intent = ActionsWriteIntent::new(
            ActionsWriteKind::DeleteCache,
            ActionsImmutableContext::default(),
            ActionsSafeOptions::default(),
            b"",
        )
        .unwrap();
        journal.begin(&target, &intent).unwrap();
        journal.list_unresolved(&scope, 256).unwrap()
    }
    fn delete_disabled(workspace: &ActionsWorkspace) -> Option<&str> {
        workspace
            .controls
            .iter()
            .find(|control| control.action == ActionsAction::Compose(ActionsCommand::DeleteCache))
            .unwrap()
            .disabled_reason
            .as_deref()
    }
    #[test]
    fn actions_lease_snapshot_survives_select_and_prepare_rows_without_blocking_another_cache() {
        let (_directory, mut journal) = journal();
        let page = pending_cache(&mut journal);
        let mut workspace = ActionsWorkspace::new(identity(42), vec!["example/repo".into()]);
        workspace.view = ActionsView::Caches;
        workspace.caches = vec![cache(71), cache(72)];
        workspace.publication_leases = Some(Arc::new(
            ActionsPublicationLeaseSnapshot::prepare(&workspace.identity, &page).unwrap(),
        ));
        workspace.prepare_rows();
        assert!(delete_disabled(&workspace)
            .unwrap()
            .contains("Original attempt #1"));
        workspace.select(1);
        assert!(delete_disabled(&workspace).is_none());
        workspace.prepare_rows();
        assert!(delete_disabled(&workspace).is_none());
        workspace.select(0);
        assert!(delete_disabled(&workspace)
            .unwrap()
            .contains("Original attempt #1"));
        workspace.prepare_rows();
        assert!(delete_disabled(&workspace)
            .unwrap()
            .contains("Original attempt #1"));
        assert_eq!(
            journal
                .list_unresolved(page.markers[0].target.scope(), 256)
                .unwrap()
                .total,
            1
        );
    }
    #[test]
    fn actions_lease_snapshot_is_positive_account_host_repository_scoped_not_global() {
        let (_directory, mut journal) = journal();
        let page = pending_cache(&mut journal);
        let snapshot = ActionsPublicationLeaseSnapshot::prepare(&identity(42), &page).unwrap();
        for (id, host, repository) in [
            (43, "github.invalid", "example/repo"),
            (42, "other.invalid", "example/repo"),
            (42, "github.invalid", "another/repo"),
        ] {
            let mut identity = identity(id);
            identity.host = host.into();
            let mut workspace = ActionsWorkspace::new(identity, vec![repository.into()]);
            workspace.caches = vec![cache(71)];
            assert!(snapshot
                .blocking_reason(&workspace, ActionsCommand::DeleteCache)
                .is_none());
        }
        assert!(ActionsPublicationLeaseSnapshot::prepare(&identity(43), &page).is_err());
        let mut unavailable = identity(42);
        unavailable.account_id = None;
        assert!(ActionsPublicationLeaseSnapshot::prepare(&unavailable, &page).is_err());
        let workspace = ActionsWorkspace::new(identity(42), vec!["example/repo".into()]);
        assert!(snapshot
            .blocking_reason(&workspace, ActionsCommand::SetSecret)
            .is_none());
        assert!(snapshot
            .blocking_reason(&workspace, ActionsCommand::FilterCaches)
            .is_none());
    }
    #[test]
    fn actions_lease_snapshot_retains_exact_deployment_context_and_allows_unrelated_same_run_cancel(
    ) {
        let (_directory, mut journal) = journal();
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let target = ActionsWriteTarget::from_scope(
            &scope,
            &ActionsTarget::DeploymentReview {
                repository: "example/repo".into(),
                run_id: 123,
                attempt: 2,
                head_sha: "a".repeat(40),
                environment_ids: vec![9],
            },
        )
        .unwrap();
        let intent = ActionsWriteIntent::new(
            ActionsWriteKind::ApproveDeployment,
            ActionsImmutableContext {
                run_id: Some(123),
                attempt: Some(2),
                head_sha: Some("a".repeat(40)),
                ..Default::default()
            },
            ActionsSafeOptions {
                selected_ids: vec![9],
                ..Default::default()
            },
            b"explanation",
        )
        .unwrap();
        let permit = journal.begin(&target, &intent).unwrap();
        journal.claim(&permit, &target, &intent).unwrap();
        journal
            .mark_accepted_pending(
                &permit,
                &ActionsReceipt {
                    kind: ActionsReceiptKind::DeploymentAccepted,
                    resource_id: Some(123),
                    observed_attempt: Some(2),
                    observed_run_id: None,
                    original_dispatch_run_v7: None,
                    workflow_remote_v9: None,
                },
            )
            .unwrap();
        let page = journal.list_unresolved(&scope, 256).unwrap();
        let snapshot = ActionsPublicationLeaseSnapshot::prepare(&identity(42), &page).unwrap();
        let actor = Actor {
            login: "octocat".into(),
            actor_type: "User".into(),
            ..Default::default()
        };
        let run:WorkflowRun=serde_json::from_value(serde_json::json!({"id":123,"workflow_id":7,"name":"CI","title":"fixture","head_sha":"a".repeat(40),"head_branch":"feature","event":"pull_request","status":"waiting","conclusion":"","attempt":2,"actor":actor,"triggering_actor":actor,"created_at":"","updated_at":"","url":""})).unwrap();
        let mut workspace = ActionsWorkspace::new(identity(42), vec!["example/repo".into()]);
        workspace.run = Some(RunPage {
            repository: "example/repo".into(),
            attempt: RunAttempt {
                run,
                jobs: vec![],
                definition_note: String::new(),
            },
            selected_job: None,
            logs: None,
            logs_skipped_bytes: 0,
            annotations: vec![],
        });
        workspace.view = ActionsView::Approvals;
        workspace.deployments = [9, 10]
            .map(|id| PendingDeployment {
                environment_id: id,
                environment_name: format!("environment-{id}"),
                wait_timer_minutes: 0,
                wait_timer_started_at: String::new(),
                current_user_can_approve: true,
                reviewers: vec![],
            })
            .to_vec();
        workspace.publication_leases = Some(Arc::new(snapshot.clone()));
        workspace.prepare_rows();
        assert!(snapshot
            .blocking_reason(&workspace, ActionsCommand::RejectDeployment)
            .is_some());
        assert!(snapshot
            .blocking_reason(&workspace, ActionsCommand::Cancel)
            .is_none());
        let other = workspace
            .rows
            .iter()
            .position(|row| row.label.starts_with("Gate #10"))
            .unwrap();
        workspace.select(other);
        assert!(snapshot
            .blocking_reason(&workspace, ActionsCommand::ApproveDeployment)
            .is_none());
        workspace.select(0);
        workspace.run.as_mut().unwrap().attempt.run.attempt = 3;
        workspace.prepare_rows();
        assert!(snapshot
            .blocking_reason(&workspace, ActionsCommand::ApproveDeployment)
            .is_none());
        assert_eq!(
            journal.list_unresolved(&scope, 256).unwrap().markers[0].state,
            ActionsWriteState::AcceptedPending
        );
    }
    #[test]
    fn actions_lease_snapshot_partial_coverage_never_releases_or_validates_forged_markers() {
        let (_directory, mut journal) = journal();
        let mut page = pending_cache(&mut journal);
        page.total = 300;
        page.complete = false;
        let snapshot = ActionsPublicationLeaseSnapshot::prepare(&identity(42), &page).unwrap();
        assert_eq!(snapshot.total(), 300);
        assert!(!snapshot.complete());
        let mut workspace = ActionsWorkspace::new(identity(42), vec!["example/repo".into()]);
        workspace.caches = vec![cache(71)];
        assert!(snapshot
            .blocking_reason(&workspace, ActionsCommand::DeleteCache)
            .is_some());
        let omitted = ActionsUnresolvedPage {
            markers: vec![],
            total: 300,
            complete: false,
        };
        let retained = ActionsPublicationLeaseSnapshot::prepare(&identity(42), &omitted)
            .unwrap()
            .with_exact_markers(&page.markers)
            .unwrap();
        assert_eq!(retained.retained_markers().len(), 1);
        assert!(!retained.complete());
        assert_eq!(retained.total(), 300);
        assert!(retained
            .blocking_reason(&workspace, ActionsCommand::DeleteCache)
            .is_some());
        let mut forged = page.clone();
        forged.markers[0].state = ActionsWriteState::AcceptedPending;
        assert!(ActionsPublicationLeaseSnapshot::prepare(&identity(42), &forged).is_err());
        page.markers[0].context.attempt = Some(99);
        assert!(ActionsPublicationLeaseSnapshot::prepare(&identity(42), &page).is_err());
        assert_eq!(
            journal
                .list_unresolved(snapshot.scope(), 256)
                .unwrap()
                .total,
            1
        );
    }
    #[test]
    fn actions_lease_snapshot_unavailable_survives_selection_and_only_fresh_snapshot_clears_it() {
        let snapshot = ActionsPublicationLeaseSnapshot::unavailable(&identity(42)).unwrap();
        assert!(snapshot.is_unavailable());
        assert!(!snapshot.complete());
        assert!(snapshot.retained_markers().is_empty());
        assert!(snapshot.clone().with_exact_markers(&[]).is_err());
        let mut workspace = ActionsWorkspace::new(identity(42), vec!["example/repo".into()]);
        workspace.view = ActionsView::Caches;
        workspace.caches = vec![cache(71), cache(72)];
        workspace.publication_leases = Some(Arc::new(snapshot));
        workspace.prepare_rows();
        let reason = delete_disabled(&workspace).unwrap().to_owned();
        assert!(reason.contains("Publication view unavailable"));
        workspace.select(1);
        assert_eq!(delete_disabled(&workspace), Some(reason.as_str()));
        workspace.prepare_rows();
        assert_eq!(delete_disabled(&workspace), Some(reason.as_str()));
        // Even a new/editable remote target must not bypass the unavailable
        // original ledger merely because no current canonical target exists.
        assert!(workspace
            .disabled_reason(ActionsCommand::SetVariable)
            .unwrap()
            .contains("Publication view unavailable"));
        assert!(workspace
            .disabled_reason(ActionsCommand::FilterCaches)
            .is_none());
        assert!(workspace
            .disabled_reason(ActionsCommand::ChooseScope)
            .is_none());
        workspace.publication_leases = Some(Arc::new(
            ActionsPublicationLeaseSnapshot::prepare(
                &workspace.identity,
                &ActionsUnresolvedPage {
                    markers: vec![],
                    total: 0,
                    complete: true,
                },
            )
            .unwrap(),
        ));
        workspace.prepare_rows();
        assert!(delete_disabled(&workspace).is_none());
    }
    #[test]
    fn actions_lease_snapshot_unavailable_requires_positive_account_and_never_crosses_scope() {
        let mut unverified = identity(42);
        unverified.account_id = None;
        assert!(ActionsPublicationLeaseSnapshot::unavailable(&unverified).is_err());
        unverified.account_id = Some(0);
        assert!(ActionsPublicationLeaseSnapshot::unavailable(&unverified).is_err());
        let snapshot = ActionsPublicationLeaseSnapshot::unavailable(&identity(42)).unwrap();
        let mut workspace = ActionsWorkspace::new(identity(43), vec!["example/repo".into()]);
        assert!(snapshot
            .blocking_reason(&workspace, ActionsCommand::SetVariable)
            .is_none());
        workspace.identity = identity(42);
        workspace.identity.host = "other.invalid".into();
        assert!(snapshot
            .blocking_reason(&workspace, ActionsCommand::SetVariable)
            .is_none());
    }
}
