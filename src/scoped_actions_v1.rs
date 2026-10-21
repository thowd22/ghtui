//! Genuine catalogue-source Actions reads, not legacy repository-name fallback.
use crate::pr_checks_native::scoped::actions_origin_v2::ScopedPrActionsOriginV2;
use crate::scoped_actions_native_v1::publication_v3::dispatch_v7::run_read_v7::DispatchRunReadRequestV7;
use crate::scoped_actions_read_v2::{
    ActionsLoadedMatchesV2, ActionsLoadedQueryV2, ActionsReadWatchV2, LogSearchModeV2,
};
use crate::scoped_actions_resources_v2::{ResourcePageV2, ResourceRequestV2};
use crate::{
    actions_workspace::{ActionsAction, ActionsView, ActionsWorkspace, RunPage, ScopedRun},
    active_remote_scope_v1::ActiveRemoteScopeV1,
    providers::{gh::*, runner::CancellationToken},
    repository_catalogue_intent_v1::IntentReference,
    storage::{Cache, CatalogueIntentSource, Scope},
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

pub const PREPARED_LIMIT_V1: usize = 8 * 1024 * 1024;
pub const BACK_LIMIT_V1: usize = 16 * 1024 * 1024;
pub struct ScopedActionsSourceV1 {
    original: Arc<CatalogueIntentSource>,
    scope: Arc<ActiveRemoteScopeV1>,
    reference: IntentReference,
    pr_v2: Option<Arc<ScopedPrActionsOriginV2>>,
    pub(crate) history_pr_v5:
        Option<Arc<crate::scoped_actions_summary_history_v5::ActionsSummaryHistoryOriginV5>>,
}
impl ScopedActionsSourceV1 {
    pub(crate) fn prepare_cleanup_pr_v9(
        &self,
        tx: &rusqlite::Transaction<'_>,
        rows: &mut Vec<crate::storage::workflow_cleanup_v9::WorkflowRawRowsV9>,
        heap: &WorkflowJsonLedgerV9,
    ) -> anyhow::Result<()> {
        if self
            .history_pr_v5
            .as_ref()
            .is_some_and(|history| !Arc::ptr_eq(history.source(), &self.original))
        {
            anyhow::bail!("Original Summary local source differs; no cleanup rebase");
        }
        if let Some(pr) = &self.pr_v2 {
            pr.prepare_cleanup_pr_v9(tx, rows, heap)?;
        }
        Ok(())
    }
    pub(crate) fn validate_owned_workflow_remote_history_v9(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedWorkflowRemoteHistoryV9,
    ) -> GithubResult<()> {
        original_guard(&self.original, database)?;
        journal
            .validate_owned_workflow_remote_history_v9(proof)
            .map_err(|_| GithubError {
                kind: GithubErrorKind::IdentityChanged,
                message: "Original W9 history row/aliases changed".into(),
            })?;
        if let Some(pr) = &self.pr_v2 {
            pr.validate_owned_workflow_remote_history_v9(database, journal, proof)?;
        }
        if let Some(pr) = &self.history_pr_v5 {
            pr.validate_local(database)?;
        }
        Ok(())
    }
    pub fn original(&self) -> &Arc<CatalogueIntentSource> {
        &self.original
    }
    pub fn scope(&self) -> &Arc<ActiveRemoteScopeV1> {
        &self.scope
    }
    pub fn reference(&self) -> &IntentReference {
        &self.reference
    }
    pub(crate) fn pr_origin_v2(&self) -> Option<&Arc<ScopedPrActionsOriginV2>> {
        self.pr_v2.as_ref()
    }
    pub(crate) fn prepare_policy(&self, workspace: &mut ActionsWorkspace, pending: bool) {
        crate::actions_workspace::scoped_read_v1::prepare_policy(workspace, pending);
        prepare_publications_label_v124(workspace);
        // Presentation eligibility only. The Workflow intent separately demands
        // the actual retained ResourcePage/Definition/member Arc before work.
        if let Some(definition) = workspace
            .definition
            .as_ref()
            .filter(|_| !pending && workspace.view == ActionsView::Workflow)
        {
            for control in &mut workspace.controls {
                if control.action == ActionsAction::EditWorkflowLocalV9 {
                    control.disabled_reason =
                        if crate::scoped_actions_workflow_edit_v9::safe_workflow_path_v9(
                            &definition.workflow.path,
                        ) && !definition.yaml.is_empty()
                            && definition.yaml.len() <= 256 * 1024
                            && crate::git::original_actor_saved_sha(&definition.commit_sha)
                        {
                            None
                        } else {
                            Some("Actual workflow path/revision/definition exceeds the supported local editor bound; no mapping/permission inferred".into())
                        };
                }
                if control.action
                    == ActionsAction::Compose(
                        crate::actions_workspace::ActionsCommand::DispatchWorkflow,
                    )
                {
                    control.disabled_reason = if definition.workflow.state == "active" {
                        None
                    } else {
                        Some("Dispatch needs an actual active workflow; inspect state first".into())
                    };
                }
                if let ActionsAction::Compose(
                    command @ (crate::actions_workspace::ActionsCommand::EnableWorkflow
                    | crate::actions_workspace::ActionsCommand::DisableWorkflow),
                ) = control.action
                {
                    control.disabled_reason = if crate::providers::gh::state_eligible_v6(
                        &definition.workflow,
                        command == crate::actions_workspace::ActionsCommand::EnableWorkflow,
                    ) {
                        None
                    } else {
                        Some("Workflow state is unknown, unsupported or already desired; refresh actual Definition".into())
                    };
                }
            }
        }
        if let Some(page) = workspace.run.as_ref().filter(|_| !pending) {
            for control in &mut workspace.controls {
                if control.action
                    == ActionsAction::Compose(crate::actions_workspace::ActionsCommand::Cancel)
                {
                    control.label = "[C Cancel / history]".into();
                    control.disabled_reason = None;
                }
                if let ActionsAction::Compose(
                    command @ (crate::actions_workspace::ActionsCommand::ForceCancel
                    | crate::actions_workspace::ActionsCommand::DeleteRun
                    | crate::actions_workspace::ActionsCommand::DeleteLogs),
                ) = &control.action
                {
                    control.disabled_reason = if *command
                        == crate::actions_workspace::ActionsCommand::ForceCancel
                        && workspace
                            .run
                            .as_ref()
                            .is_some_and(|page| page.attempt.run.status == "completed")
                    {
                        Some(
                            "The actual run is completed; force cancellation cannot target it"
                                .into(),
                        )
                    } else {
                        None
                    };
                }
                if let ActionsAction::Compose(
                    command @ (crate::actions_workspace::ActionsCommand::RerunAll
                    | crate::actions_workspace::ActionsCommand::RerunFailed
                    | crate::actions_workspace::ActionsCommand::RerunJob),
                ) = &control.action
                {
                    control.disabled_reason = if page.attempt.run.status != "completed" {
                        Some("Open a completed run to rerun; original protected result remains inspectable through Cancel / history".into())
                    } else if *command == crate::actions_workspace::ActionsCommand::RerunJob
                        && page.selected_job.is_none()
                    {
                        Some("Open an actual exact-attempt job before rerunning it; URL numbers are not targets".into())
                    } else {
                        None
                    };
                }
            }
        }
        self.prepare_publications_control_v124(workspace, pending);
        if self.pr_v2.is_some() {
            for control in &mut workspace.controls {
                if matches!(control.action, ActionsAction::View(ActionsView::Runs)) {
                    control.disabled_reason=Some("Original PR route stays pinned to its actual run/head; open repository Actions explicitly for other runs".into());
                }
            }
        }
    }
    // Only a genuine closed Source enables this existing reader. Initial seed
    // preparation already owns the fixed label before its final principal
    // proof; enabling it does not allocate or rebuild private presentation.
    fn prepare_publications_control_v124(&self, workspace: &mut ActionsWorkspace, pending: bool) {
        if !pending {
            for control in &mut workspace.controls {
                if control.action == ActionsAction::PublicationList {
                    debug_assert_eq!(control.label, "[P Publications]");
                    control.disabled_reason = None;
                }
            }
        }
    }
    pub(crate) fn validate_local(&self, database: &Path) -> GithubResult<()> {
        original_guard(&self.original, database)?;
        if let Some(pr) = &self.pr_v2 {
            pr.validate_local(database)?;
        }
        if let Some(pr) = &self.history_pr_v5 {
            pr.validate_local(database)?;
        }
        Ok(())
    }
    pub(crate) fn validate_owned_publication_local_v4(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        claim: &crate::actions_write_journal::ActionsClaimStampV3,
        result: Option<(
            crate::actions_write_journal::ActionsWriteState,
            &crate::actions_write_journal::ActionsReceipt,
        )>,
    ) -> GithubResult<()> {
        original_guard(&self.original, database)?;
        if let Some(pr) = &self.pr_v2 {
            pr.validate_owned_publication_local_v4(database, journal, claim, result)?;
        }
        Ok(())
    }
    pub(crate) fn validate_owned_run_history_v8(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedRunHistoryV8,
    ) -> GithubResult<()> {
        original_guard(&self.original, database)?;
        journal
            .validate_owned_run_history_v8(proof)
            .map_err(|_| GithubError {
                kind: GithubErrorKind::IdentityChanged,
                message: "Original Run history changed".into(),
            })?;
        if let Some(pr) = &self.pr_v2 {
            pr.validate_owned_run_history_v8(database, journal, proof)?;
        }
        if let Some(pr) = &self.history_pr_v5 {
            pr.validate_local(database)?;
        }
        Ok(())
    }
    pub(crate) fn validate_workflow_branch_transition_v9(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        branch: &crate::scoped_actions_workflow_edit_v9::WorkflowPublishedBranchV9,
    ) -> GithubResult<()> {
        original_guard(&self.original, database)?;
        if !branch.matches_source_v9(self) {
            return Err(GithubError {
                kind: GithubErrorKind::IdentityChanged,
                message: "Actual workflow branch belongs to another original source".into(),
            });
        }
        branch.validate_journal_v9(journal)?;
        if let Some(pr) = &self.pr_v2 {
            pr.validate_workflow_branch_transition_v9(database, journal, branch)?;
        }
        if self.history_pr_v5.is_some() {
            return Err(GithubError {
                kind: GithubErrorKind::IdentityChanged,
                message: "Historical source cannot continue workflow publication".into(),
            });
        }
        Ok(())
    }
    pub(crate) fn validate_owned_dispatch_history_v7(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedDispatchHistoryV7,
    ) -> GithubResult<()> {
        original_guard(&self.original, database)?;
        journal
            .validate_owned_dispatch_history_v7(proof)
            .map_err(|_| GithubError {
                kind: GithubErrorKind::IdentityChanged,
                message: "Actual owned Dispatch history changed".into(),
            })?;
        if let Some(pr) = &self.pr_v2 {
            pr.validate_owned_dispatch_history_v7(database, journal, proof)?;
        }
        if let Some(pr) = &self.history_pr_v5 {
            pr.validate_local(database)?;
        }
        Ok(())
    }
    pub(crate) fn validate_owned_workflow_history_v6(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6,
    ) -> GithubResult<()> {
        original_guard(&self.original, database)?;
        journal
            .validate_owned_workflow_history_v6(proof)
            .map_err(|_| GithubError {
                kind: GithubErrorKind::IdentityChanged,
                message: "Actual owned workflow history changed".into(),
            })?;
        if let Some(pr) = &self.pr_v2 {
            pr.validate_owned_workflow_history_v6(database, journal, proof)?;
        }
        if let Some(pr) = &self.history_pr_v5 {
            pr.validate_local(database)?;
        }
        Ok(())
    }
}
#[derive(Clone)]
pub enum ScopedActionsPageKindV1 {
    HistorySeedV5,
    Catalogue {
        runs: Arc<[Arc<ScopedActionsRunTargetV1>]>,
        window: Arc<ScopedRunWindowV2>,
    },
    Attempt {
        run: Arc<ScopedActionsRunTargetV1>,
        jobs: Arc<[Arc<ScopedActionsJobTargetV1>]>,
        model: Arc<RunAttempt>,
        loaded: Option<Arc<ScopedActionsLogSnapshotV2>>,
    },
    Resource(Arc<ResourcePageV2>),
}
pub struct ScopedActionsPageV1 {
    source: Arc<ScopedActionsSourceV1>,
    kind: ScopedActionsPageKindV1,
    view: ActionsView,
    selected_job: Option<u64>,
    identity: HostIdentity,
    fetched_at: Option<u64>,
}
impl ScopedActionsPageV1 {
    pub(crate) fn retained_bytes(&self) -> usize {
        let mut bytes = std::mem::size_of::<Self>() + 2 * std::mem::size_of::<usize>();
        bytes = bytes.saturating_add(
            self.identity.host.capacity()
                + self.identity.login.capacity()
                + self.identity.gh_version.capacity(),
        );
        bytes = bytes.saturating_add(
            std::mem::size_of::<ScopedActionsSourceV1>()
                + 2 * std::mem::size_of::<usize>()
                + self.source.reference.last_name.capacity()
                + self
                    .source
                    .reference
                    .node_id
                    .as_ref()
                    .map_or(0, String::capacity),
        );
        if self.source.history_pr_v5.is_some() {
            bytes = bytes.saturating_add(128 * 1024);
        }
        match &self.kind {
            ScopedActionsPageKindV1::HistorySeedV5 => {}
            ScopedActionsPageKindV1::Catalogue { runs, window } => {
                bytes = bytes.saturating_add(window.owned_bytes());
                bytes = bytes.saturating_add(
                    runs.len() * std::mem::size_of::<Arc<ScopedActionsRunTargetV1>>()
                        + 2 * std::mem::size_of::<usize>(),
                );
                for run in runs.iter() {
                    bytes = bytes.saturating_add(run.owned_bytes_v1());
                }
            }
            ScopedActionsPageKindV1::Attempt {
                run,
                jobs,
                model,
                loaded,
            } => {
                if let Some(snapshot) = loaded {
                    bytes = bytes.saturating_add(snapshot.owned_bytes());
                }
                bytes = bytes.saturating_add(run.owned_bytes_v1());
                bytes = bytes.saturating_add(
                    jobs.len() * std::mem::size_of::<Arc<ScopedActionsJobTargetV1>>()
                        + 2 * std::mem::size_of::<usize>(),
                );
                for job in jobs.iter() {
                    bytes = bytes.saturating_add(job.owned_bytes_v1());
                }
                bytes = bytes.saturating_add(
                    crate::actions_workspace::scoped_read_v1::attempt_owned_bytes(model)
                        + 2 * std::mem::size_of::<usize>(),
                );
            }
            ScopedActionsPageKindV1::Resource(page) => {
                bytes = bytes.saturating_add(page.owned_bytes())
            }
        }
        bytes
    }
    pub fn source(&self) -> &Arc<ScopedActionsSourceV1> {
        &self.source
    }
    pub fn view(&self) -> ActionsView {
        self.view
    }
    pub fn run_window_v2(&self) -> Option<&Arc<ScopedRunWindowV2>> {
        match &self.kind {
            ScopedActionsPageKindV1::Catalogue { window, .. } => Some(window),
            _ => None,
        }
    }
    pub fn run(&self) -> Option<&Arc<ScopedActionsRunTargetV1>> {
        match &self.kind {
            ScopedActionsPageKindV1::Attempt { run, .. } => Some(run),
            ScopedActionsPageKindV1::Resource(page) => page.run(),
            _ => None,
        }
    }
    pub(crate) fn loaded_log_snapshot_v2(&self) -> Option<&Arc<ScopedActionsLogSnapshotV2>> {
        match &self.kind {
            ScopedActionsPageKindV1::Attempt {
                run,
                jobs,
                loaded: Some(snapshot),
                ..
            } if self.view == ActionsView::Logs
                && Arc::ptr_eq(run, snapshot.run())
                && self.selected_job == Some(snapshot.job().id())
                && jobs.iter().any(|job| Arc::ptr_eq(job, snapshot.job())) =>
            {
                Some(snapshot)
            }
            _ => None,
        }
    }
    pub(crate) fn resource_v2(&self) -> Option<&Arc<ResourcePageV2>> {
        match &self.kind {
            ScopedActionsPageKindV1::Resource(page) => Some(page),
            _ => None,
        }
    }
    pub fn job(&self, id: u64) -> Option<Arc<ScopedActionsJobTargetV1>> {
        match &self.kind {
            ScopedActionsPageKindV1::Attempt { jobs, .. } => {
                jobs.iter().find(|job| job.id() == id).cloned()
            }
            _ => None,
        }
    }
    /// Actual current selected exact-attempt member, not a rendered row/URL ID.
    pub(crate) fn selected_job_for_publication_v4(&self) -> Option<Arc<ScopedActionsJobTargetV1>> {
        let ScopedActionsPageKindV1::Attempt {
            run, jobs, model, ..
        } = &self.kind
        else {
            return None;
        };
        let id = self.selected_job?;
        let target = jobs
            .iter()
            .find(|job| job.id() == id && Arc::ptr_eq(job.run(), run))?;
        let member = model.jobs.iter().find(|job| job.id == id)?;
        (member.run_id == run.run().id
            && member.attempt == run.run().attempt
            && member.head_sha == run.run().head_sha)
            .then(|| target.clone())
    }
    /// Borrowed original presentation only; not a target or permission.
    pub(crate) fn original_job_label_v4(
        &self,
        selected: &Arc<ScopedActionsJobTargetV1>,
    ) -> Option<&str> {
        let ScopedActionsPageKindV1::Attempt {
            run, jobs, model, ..
        } = &self.kind
        else {
            return None;
        };
        if !Arc::ptr_eq(run, selected.run()) || !jobs.iter().any(|job| Arc::ptr_eq(job, selected)) {
            return None;
        }
        model
            .jobs
            .iter()
            .find(|job| job.id == selected.id())
            .map(|job| job.name.as_str())
    }
    pub fn run_target(&self, id: u64, attempt: u64) -> Option<Arc<ScopedActionsRunTargetV1>> {
        match &self.kind {
            ScopedActionsPageKindV1::Catalogue { runs, .. } => runs
                .iter()
                .find(|run| run.run().id == id && run.run().attempt == attempt)
                .cloned(),
            ScopedActionsPageKindV1::Attempt { run, .. }
                if run.run().id == id && run.run().attempt == attempt =>
            {
                Some(run.clone())
            }
            ScopedActionsPageKindV1::Resource(page) => page
                .run()
                .filter(|run| run.run().id == id && run.run().attempt == attempt)
                .cloned(),
            _ => None,
        }
    }
}
pub struct InitialScopedActionsV1 {
    pub(crate) source: Arc<ScopedActionsSourceV1>,
    pub(crate) page: Arc<ScopedActionsPageV1>,
    pub(crate) workspace: ActionsWorkspace,
    pub(crate) retained_bytes: usize,
}
/// Pure actual Summary presentation seed. No run/catalogue read is invented;
/// the one captured history Work still must verify its fresh source/parent.
pub(crate) fn summary_history_seed_v5(
    app: &crate::app::App,
) -> Result<InitialScopedActionsV1, String> {
    let origin =
        crate::scoped_actions_summary_history_v5::ActionsSummaryHistoryOriginV5::capture(app)?;
    let identity = app
        .github
        .as_ref()
        .ok_or("Actual Summary unavailable")?
        .identity
        .clone();
    let source = Arc::new(ScopedActionsSourceV1 {
        original: origin.source().clone(),
        scope: origin.active().clone(),
        reference: origin.reference().clone(),
        pr_v2: None,
        history_pr_v5: Some(origin),
    });
    let mut workspace =
        ActionsWorkspace::new(identity.clone(), vec![source.reference.last_name.clone()]);
    workspace.view = ActionsView::Publications;
    workspace.prepare_rows();
    workspace.controls.retain(|control| {
        matches!(
            control.action,
            ActionsAction::Back | ActionsAction::PublicationList
        )
    });
    let page = Arc::new(ScopedActionsPageV1 {
        source: source.clone(),
        kind: ScopedActionsPageKindV1::HistorySeedV5,
        view: ActionsView::Publications,
        selected_job: None,
        identity,
        fetched_at: None,
    });
    let retained_bytes = charge(&workspace, &page)?;
    Ok(InitialScopedActionsV1 {
        source,
        page,
        workspace,
        retained_bytes,
    })
}
/// Worker-only presentation prepared from the actual opaque read BEFORE its
/// owning session's closing repository/SQL/principal proof. No new source or
/// publication authority is created here.
pub(crate) struct PreparedActionsSeedV1 {
    read: ScopedActionsCatalogueReadV1,
    workspace: ActionsWorkspace,
}
fn prepare_publications_label_v124(workspace: &mut ActionsWorkspace) {
    for control in &mut workspace.controls {
        if control.action == ActionsAction::PublicationList {
            control.label = "[P Publications]".into();
        }
    }
}
pub(crate) fn prepare_initial_read(
    read: ScopedActionsCatalogueReadV1,
    identity: &HostIdentity,
    cancel: &CancellationToken,
) -> Result<PreparedActionsSeedV1, String> {
    let mut workspace = ActionsWorkspace::new(identity.clone(), vec![read.repository().into()]);
    workspace.runs = read
        .catalogue()
        .runs
        .iter()
        .cloned()
        .map(|run| ScopedRun {
            repository: read.repository().into(),
            run,
        })
        .collect();
    workspace.coverage = read.catalogue().coverage.clone();
    workspace.fetched_at = Some(crate::storage::now());
    workspace.prepare_rows();
    crate::actions_workspace::scoped_read_v1::prepare_policy(&mut workspace, false);
    // Allocate only fixed public text while still inside the original read,
    // before closing repository/SQL/UID proof. It remains disabled until the
    // opaque read is admitted as a genuine Source in initial().
    prepare_publications_label_v124(&mut workspace);
    crate::actions_workspace::scoped_read_v1::prepare_catalogue_window_v2(
        &mut workspace,
        read.window(),
    );
    crate::actions_workspace::scoped_read_v1::retained_bytes(&workspace)?;
    if cancel.is_cancelled() {
        return Err("Original Actions seed cancelled".into());
    }
    Ok(PreparedActionsSeedV1 { read, workspace })
}
/// Only the actual provider-read/closed current-access path can call this with
/// genuine observations. No App/JSON/DTO overload exists.
pub(crate) fn initial(
    original: Arc<CatalogueIntentSource>,
    scope: Arc<ActiveRemoteScopeV1>,
    reference: IntentReference,
    seed: PreparedActionsSeedV1,
    closed: &ClosedCatalogueAccess,
    provider: &GithubProvider,
    cancel: &CancellationToken,
) -> Result<InitialScopedActionsV1, String> {
    let PreparedActionsSeedV1 {
        read,
        mut workspace,
    } = seed;
    let current = closed
        .observation(reference.id)
        .ok_or("Original closed Actions repository unavailable")?;
    let scope_matches = matches!(scope.consumer(closed.identity()),crate::active_remote_scope_v1::RemoteScopeConsumerV1::Explicit(refs)
        if refs.references().len()==1 && refs.references()[0].original()==&reference
        && refs.references()[0].observed().is_some_and(|observed|observed.id==read.repository_id()
            && observed.node_id.as_deref()==read.repository_node() && observed.last_name==read.repository()));
    if !read.same_producer(provider)
        || !scope_matches
        || read.repository_id() != reference.id
        || reference
            .node_id
            .as_deref()
            .is_some_and(|node| read.repository_node() != Some(node))
        || current.repository().id != read.repository_id()
        || current.repository().node_id.as_deref() != read.repository_node()
        || current.repository().full_name != read.repository()
        || current.repository().permissions.pull != Some(true)
        || Scope::github(&closed.identity().host, closed.identity().account_id)
            .ok()
            .as_ref()
            != Some(original.scope())
        || crate::remote_scope_gate_v1::stop_before_fallback(Some(&scope)).is_some()
    {
        return Err("Original closed Actions seed unavailable; no DTO promotion".into());
    }
    let (_, runs, window) = read.into_parts();
    let source = Arc::new(ScopedActionsSourceV1 {
        original,
        scope,
        reference,
        pr_v2: None,
        history_pr_v5: None,
    });
    source.prepare_publications_control_v124(&mut workspace, false);
    let page = Arc::new(ScopedActionsPageV1 {
        source: source.clone(),
        kind: ScopedActionsPageKindV1::Catalogue { runs, window },
        view: ActionsView::Runs,
        selected_job: None,
        identity: closed.identity().clone(),
        fetched_at: workspace.fetched_at,
    });
    workspace.identity = closed.identity().clone();
    let retained_bytes = charge(&workspace, &page)?;
    if cancel.is_cancelled() {
        return Err("Original Actions seed cancelled".into());
    }
    Ok(InitialScopedActionsV1 {
        source,
        page,
        workspace,
        retained_bytes,
    })
}
/// Re-express only the actual current PR run/route already visible on App.
/// Its distinct PR origin stays mandatory on every subsequent read. No DTO
/// can provide this closed input, and no fresh access is asserted here.
pub(crate) fn retained_pr_route_v2(
    origin: Arc<ScopedPrActionsOriginV2>,
    workspace: &ActionsWorkspace,
) -> Result<(Arc<ScopedActionsSourceV1>, Arc<ScopedActionsPageV1>, usize), String> {
    let observed = origin.observation();
    let selected = observed
        .selected_run_v1()
        .ok_or("Actual selected PR run unavailable")?;
    let run = ScopedActionsRunTargetV1::from_pr_observation_v2(observed)
        .map_err(|_| "Actual PR Actions target unavailable")?;
    let jobs = ScopedActionsJobTargetV1::from_pr_observation_v2(observed, &run)
        .map_err(|_| "Actual PR Actions member proof unavailable")?;
    if !workspace.run.as_ref().is_some_and(|page| {
        page.attempt.run.id == run.run().id
            && page.attempt.run.attempt == run.run().attempt
            && page.attempt.run.head_sha == run.run().head_sha
    }) || workspace.identity.host != observed.identity().host
        || workspace.identity.account_id != observed.identity().account_id
    {
        return Err("Visible Actions page differs from actual original PR route".into());
    }
    let source = Arc::new(ScopedActionsSourceV1 {
        original: origin.source().clone(),
        scope: origin.scope().clone(),
        reference: origin.reference().clone(),
        pr_v2: Some(origin.clone()),
        history_pr_v5: None,
    });
    let page = Arc::new(ScopedActionsPageV1 {
        source: source.clone(),
        kind: ScopedActionsPageKindV1::Attempt {
            run,
            jobs,
            model: selected.attempt().clone(),
            loaded: None,
        },
        view: workspace.view,
        selected_job: workspace.run.as_ref().and_then(|page| page.selected_job),
        identity: workspace.identity.clone(),
        fetched_at: workspace.fetched_at,
    });
    let bytes = charge(workspace, &page)?;
    Ok((source, page, bytes))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedActionsRequestKindV1 {
    Publication,
    Catalogue,
    Attempt,
    Logs,
    Annotations,
    PreparedView(ActionsView),
    Resource,
    Search,
}
pub struct ScopedActionsWorkV1 {
    pub(crate) journal_v5:
        Option<Arc<crate::scoped_actions_native_v1::journal_v5::JournalRequestV5>>,
    pub(crate) publication_v3:
        Option<Arc<crate::scoped_actions_native_v1::publication_v3::PublicationWorkV3>>,
    pub(crate) source: Arc<ScopedActionsSourceV1>,
    pub(crate) page: Arc<ScopedActionsPageV1>,
    pub(crate) run: Option<Arc<ScopedActionsRunTargetV1>>,
    pub(crate) job: Option<Arc<ScopedActionsJobTargetV1>>,
    pub(crate) kind: ScopedActionsRequestKindV1,
    pub(crate) query_v2: Option<Arc<ScopedRunQueryV2>>,
    pub(crate) resource_v2: Option<Arc<ResourceRequestV2>>,
    pub(crate) search_v2: Option<Arc<ActionsLoadedQueryV2>>,
    pub(crate) watch_v2: Option<Arc<ActionsReadWatchV2>>,
    pub(crate) position_v2: Option<(usize, usize)>,
    pub(crate) requested_view_v2: Option<ActionsView>,
    pub(crate) deadline: Instant,
    pub(crate) cancellation: CancellationToken,
    pub(crate) epoch: u64,
    #[cfg(test)]
    pub(crate) after_read_fault_v1: Option<fn()>,
}
impl ScopedActionsWorkV1 {
    pub(crate) fn capture_journal_v5(
        page: Arc<ScopedActionsPageV1>,
        epoch: u64,
        request: Arc<crate::scoped_actions_native_v1::journal_v5::JournalRequestV5>,
    ) -> Result<Arc<Self>, String> {
        if !request.matches_page(&page) {
            return Err("Actual original history page/source differs".into());
        }
        Ok(Arc::new(Self {
            journal_v5: Some(request),
            publication_v3: None,
            source: page.source.clone(),
            page,
            run: None,
            job: None,
            kind: ScopedActionsRequestKindV1::Publication,
            query_v2: None,
            resource_v2: None,
            search_v2: None,
            watch_v2: None,
            position_v2: None,
            requested_view_v2: None,
            deadline: Instant::now() + Duration::from_secs(30),
            cancellation: CancellationToken::default(),
            epoch,
            #[cfg(test)]
            after_read_fault_v1: None,
        }))
    }
    pub fn host(&self) -> &str {
        &self.source.original.scope().host
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }
    pub(crate) fn capture(
        page: Arc<ScopedActionsPageV1>,
        action: &ActionsAction,
        epoch: u64,
    ) -> Result<Arc<Self>, String> {
        if page.source.history_pr_v5.is_some() {
            return Err("Actual Summary history is READ/ACK only; it cannot construct run/resource/write work".into());
        }
        let mut query_v2 = page.run_window_v2().map(|window| window.query().clone());
        if page.source.pr_v2.is_some()
            && matches!(
                action,
                ActionsAction::View(ActionsView::Runs) | ActionsAction::ScopedReadV2(_)
            )
        {
            return Err("This original PR-check route is pinned to its actual run/head; open repository Actions explicitly for another run catalogue".into());
        }
        let resource_v2 = ResourceRequestV2::capture(&page, action)?;
        let (kind, run, job) = if resource_v2.is_some() {
            (
                ScopedActionsRequestKindV1::Resource,
                page.run().cloned(),
                None,
            )
        } else {
            match action {
                ActionsAction::ScopedReadV2(action) => {
                    use crate::scoped_actions_resources_v2::ScopedActionsReadActionV2 as Read;
                    let window = page
                        .run_window_v2()
                        .ok_or("Open an actual observed run page before paging/filtering")?;
                    query_v2=Some(match action{
                    Read::RunPage(1)=>window.next_query().ok_or("No further observed page within the bounded catalogue; partial is not absence")?,
                    Read::RunPage(-1)=>window.previous_query().ok_or("No previous run page")?,
                    Read::RunPage(_)=>return Err("Run paging moves one genuine bounded page at a time".into()),
                    filter=>{
                        let mut requested=window.query().filter().clone();
                        requested.status=match filter{Read::AllRunStatuses=>None,Read::FailedRuns=>Some("failure".into()),Read::ActiveRuns=>Some("in_progress".into()),_=>return Err("Presentation action has no API read protocol".into())};
                        ScopedRunQueryV2::first(requested).map_err(|_|"Bounded run filter unavailable")?
                    }
                });
                    (ScopedActionsRequestKindV1::Catalogue, None, None)
                }
                ActionsAction::OpenJob(id) => {
                    let job = page.job(*id).ok_or("Original selected job unavailable")?;
                    (
                        ScopedActionsRequestKindV1::PreparedView(ActionsView::Jobs),
                        Some(job.run().clone()),
                        Some(job),
                    )
                }
                ActionsAction::View(view @ (ActionsView::Run | ActionsView::Jobs)) => {
                    let run = page.run().ok_or("Original attempt unavailable")?.clone();
                    let job = page.selected_job.and_then(|id| page.job(id));
                    (
                        if page.resource_v2().is_some() {
                            ScopedActionsRequestKindV1::Attempt
                        } else {
                            ScopedActionsRequestKindV1::PreparedView(*view)
                        },
                        Some(run),
                        job,
                    )
                }
                ActionsAction::View(ActionsView::Runs) => {
                    (ScopedActionsRequestKindV1::Catalogue, None, None)
                }
                ActionsAction::Refresh if page.view == ActionsView::Runs => {
                    (ScopedActionsRequestKindV1::Catalogue, None, None)
                }
                ActionsAction::OpenRun {
                    repository,
                    id,
                    attempt,
                } => {
                    let run = page
                        .run_target(*id, *attempt)
                        .filter(|run| run.repository() == repository)
                        .ok_or("Selected run is not an original observed catalogue member")?;
                    (ScopedActionsRequestKindV1::Attempt, Some(run), None)
                }
                ActionsAction::Refresh => {
                    let run = page.run().ok_or("Original attempt unavailable")?.clone();
                    let job = page.selected_job.and_then(|id| page.job(id));
                    let kind = match page.view {
                        ActionsView::Logs => ScopedActionsRequestKindV1::Logs,
                        ActionsView::Annotations => ScopedActionsRequestKindV1::Annotations,
                        _ => ScopedActionsRequestKindV1::Attempt,
                    };
                    (kind, Some(run), job)
                }
                ActionsAction::Logs(Some(id)) | ActionsAction::Annotations(id) => {
                    let job = page
                        .job(*id)
                        .ok_or("Selected job is not an actual original attempt member")?;
                    let kind = if matches!(action, ActionsAction::Annotations(_)) {
                        ScopedActionsRequestKindV1::Annotations
                    } else {
                        ScopedActionsRequestKindV1::Logs
                    };
                    (kind, Some(job.run().clone()), Some(job))
                }
                _ => return Err("This scoped Actions action has no current read protocol".into()),
            }
        };
        Ok(Arc::new(Self {
            journal_v5: None,
            publication_v3: None,
            source: page.source.clone(),
            page,
            run,
            job,
            kind,
            query_v2,
            resource_v2,
            search_v2: None,
            watch_v2: None,
            position_v2: None,
            requested_view_v2: match action {
                ActionsAction::View(view @ (ActionsView::Run | ActionsView::Jobs)) => Some(*view),
                _ => None,
            },
            deadline: Instant::now() + Duration::from_secs(30),
            cancellation: CancellationToken::default(),
            epoch,
            #[cfg(test)]
            after_read_fault_v1: None,
        }))
    }
    pub(crate) fn check(&self) -> Result<Duration, String> {
        if self.cancellation.is_cancelled() {
            return Err("Original Actions read cancelled".into());
        }
        self.deadline
            .checked_duration_since(Instant::now())
            .filter(|left| !left.is_zero())
            .ok_or_else(|| "Original Actions read deadline expired".into())
    }
    pub(crate) fn capture_filter_v2(
        page: Arc<ScopedActionsPageV1>,
        query: Arc<ScopedRunQueryV2>,
        epoch: u64,
    ) -> Result<Arc<Self>, String> {
        if page.run_window_v2().is_none() || page.source.pr_v2.is_some() {
            return Err("Actual repository run catalogue required for editable filters".into());
        }
        let mut work = Self::capture(page, &ActionsAction::Refresh, epoch)?;
        let inner = Arc::get_mut(&mut work).ok_or("New filter work unexpectedly aliased")?;
        inner.kind = ScopedActionsRequestKindV1::Catalogue;
        inner.query_v2 = Some(query);
        Ok(work)
    }
    pub(crate) fn capture_search_v2(
        page: Arc<ScopedActionsPageV1>,
        query: String,
        mode: LogSearchModeV2,
        position: (usize, usize),
        epoch: u64,
    ) -> Result<Arc<Self>, String> {
        let requested = ActionsLoadedQueryV2::capture(&page, query, mode)?;
        let job = page
            .loaded_log_snapshot_v2()
            .ok_or("Actual snapshot unavailable")?
            .job()
            .id();
        let mut work = Self::capture(page, &ActionsAction::Logs(Some(job)), epoch)?;
        let inner = Arc::get_mut(&mut work).ok_or("New search work unexpectedly aliased")?;
        inner.kind = ScopedActionsRequestKindV1::Search;
        inner.search_v2 = Some(requested);
        inner.position_v2 = Some(position);
        Ok(work)
    }
    pub(crate) fn capture_watch_v2(
        page: Arc<ScopedActionsPageV1>,
        watch: Arc<ActionsReadWatchV2>,
        epoch: u64,
    ) -> Result<Arc<Self>, String> {
        if !watch.active() || !watch.matches(&page) {
            return Err("Original Watch source/run/attempt/head/member changed".into());
        }
        let action = if let Some(job) = watch.job() {
            ActionsAction::Logs(Some(job.id()))
        } else {
            let run = page.run().ok_or("Original watch run unavailable")?;
            ActionsAction::OpenRun {
                repository: run.repository().into(),
                id: run.run().id,
                attempt: run.run().attempt,
            }
        };
        let mut work = Self::capture(page, &action, epoch)?;
        let inner = Arc::get_mut(&mut work).ok_or("New watch work unexpectedly aliased")?;
        inner.deadline = watch.request_deadline();
        inner.cancellation = watch.cancellation().clone();
        inner.watch_v2 = Some(watch);
        Ok(work)
    }
}
pub struct PreparedScopedActionsV1 {
    pub(crate) work: Arc<ScopedActionsWorkV1>,
    pub(crate) page: Arc<ScopedActionsPageV1>,
    pub(crate) workspace: ActionsWorkspace,
    pub(crate) retained_bytes: usize,
    pub(crate) matches_v2: Option<Arc<ActionsLoadedMatchesV2>>,
    pub(crate) watch_progress_v2: Option<(usize, usize)>,
    pub(crate) transport_bytes_v2: usize,
}
impl PreparedScopedActionsV1 {
    pub(crate) fn matches(&self, work: &Arc<ScopedActionsWorkV1>) -> bool {
        Arc::ptr_eq(&self.work, work)
            && Arc::ptr_eq(&self.page.source, &work.source)
            && self
                .matches_v2
                .as_ref()
                .is_none_or(|matches| matches.matches_page(&self.page))
            && (work.kind != ScopedActionsRequestKindV1::Search || self.matches_v2.is_some())
            && work
                .requested_view_v2
                .is_none_or(|view| self.page.view == view && self.workspace.view == view)
            && work.watch_v2.as_ref().is_none_or(|watch| {
                watch.active() && watch.matches(&self.page) && self.watch_progress_v2.is_some()
            })
    }
}
fn original_guard(original: &CatalogueIntentSource, database: &Path) -> GithubResult<()> {
    let cache = Cache::open(database).map_err(|_| guard_error())?;
    cache
        .validate_repository_catalogue_intent_source(original)
        .map_err(|_| guard_error())
}
fn guard_error() -> GithubError {
    GithubError {
        kind: GithubErrorKind::IdentityChanged,
        message: "Original Actions intent changed; no source rebased".into(),
    }
}
fn charge(workspace: &ActionsWorkspace, page: &ScopedActionsPageV1) -> Result<usize, String> {
    let bytes = crate::actions_workspace::scoped_read_v1::retained_bytes(workspace)?
        .saturating_add(page.retained_bytes());
    if bytes > PREPARED_LIMIT_V1 {
        Err("Scoped Actions workspace/proof capacity exceeds8MiB; no partial complete page".into())
    } else {
        Ok(bytes)
    }
}
fn identity_capacity(identity: &HostIdentity) -> usize {
    identity
        .host
        .capacity()
        .saturating_add(identity.login.capacity())
        .saturating_add(identity.gh_version.capacity())
}
/// Final same-numeric-principal presentation only. Each independently cloned
/// identity is charged by its actual String capacities, never twice another
/// clone's capacities or predicted length. No row/body/SQL work occurs here.
fn replace_final_identity(
    workspace: &mut ActionsWorkspace,
    page: &mut Arc<ScopedActionsPageV1>,
    retained_bytes: &mut usize,
    identity: &HostIdentity,
) -> Result<(), String> {
    let page =
        Arc::get_mut(page).ok_or("Original prepared Actions page was unexpectedly aliased")?;
    let old =
        identity_capacity(&workspace.identity).saturating_add(identity_capacity(&page.identity));
    workspace.identity = identity.clone();
    page.identity = identity.clone();
    let new =
        identity_capacity(&workspace.identity).saturating_add(identity_capacity(&page.identity));
    *retained_bytes = retained_bytes.saturating_sub(old).saturating_add(new);
    if *retained_bytes > PREPARED_LIMIT_V1 {
        return Err("Final identity presentation exceeds original prepared limit".into());
    }
    Ok(())
}

/// Runs in SAME worker and SAME original deadline/token. Errors never expose
/// diagnostics; even a failing inner read closes own repo/SQL/final principal.
pub fn execute(
    work: Arc<ScopedActionsWorkV1>,
    provider: &GithubProvider,
    database: &Path,
) -> Result<PreparedScopedActionsV1, String> {
    work.check()?;
    if work.publication_v3.is_some() || work.kind == ScopedActionsRequestKindV1::Publication {
        return Err(
            "Original Actions publication requires its SAME typed Actions worker; no read fallback"
                .into(),
        );
    }
    let claim = work
        .watch_v2
        .as_ref()
        .map(|watch| watch.claim(&work.page))
        .transpose()?;
    let mut prepared = execute_inner_v2(work, provider, database, None)?;
    if let Some(claim) = claim {
        prepared.watch_progress_v2 = Some(claim.finish(prepared.transport_bytes_v2)?);
    }
    prepared.work.check()?;
    Ok(prepared)
}
/// Closed, actual returned-run purpose only. Ordinary execute continues to
/// refuse every publication window; no DTO or saved ID can select this branch.
pub(crate) fn execute_dispatch_read_v7(
    work: Arc<ScopedActionsWorkV1>,
    request: &Arc<DispatchRunReadRequestV7>,
    provider: &GithubProvider,
    database: &Path,
) -> Result<PreparedScopedActionsV1, String> {
    work.check()?;
    if !request.matches_work(&work)
        || !matches!(work.publication_v3.as_deref(),Some(crate::scoped_actions_native_v1::publication_v3::PublicationWorkV3::Dispatch(d))
            if matches!(&**d,crate::scoped_actions_native_v1::publication_v3::dispatch_v7::DispatchWorkV7::Read(r)if Arc::ptr_eq(r,request)))
    {
        return Err("Exact returned Dispatch read purpose/source differs".into());
    }
    let result = execute_inner_v2(work, provider, database, Some(request))?;
    result.work.check()?;
    if !result.matches(&result.work) {
        return Err(
            "Returned Dispatch read projection does not match its exact requested view".into(),
        );
    }
    Ok(result)
}
fn validate_read_source_v7(
    work: &ScopedActionsWorkV1,
    dispatch: Option<&Arc<DispatchRunReadRequestV7>>,
    database: &Path,
) -> GithubResult<()> {
    match dispatch {
        Some(request) if request.matches_work(work) => request.validate(database),
        Some(_) => Err(guard_error()),
        None => work.source.validate_local(database),
    }
}
fn execute_inner_v2(
    work: Arc<ScopedActionsWorkV1>,
    provider: &GithubProvider,
    database: &Path,
    dispatch: Option<&Arc<DispatchRunReadRequestV7>>,
) -> Result<PreparedScopedActionsV1, String> {
    let remaining = work.check()?;
    if work.kind == ScopedActionsRequestKindV1::Search {
        let query = work
            .search_v2
            .as_ref()
            .filter(|query| query.belongs_to(&work.page))
            .ok_or("Original query/snapshot changed")?;
        let matches = query.prepare(work.deadline, &work.cancellation)?;
        let ScopedActionsPageKindV1::Attempt { model, .. } = &work.page.kind else {
            return Err("Original captured attempt unavailable".into());
        };
        let snapshot = matches.snapshot();
        crate::actions_workspace::scoped_read_v1::preflight_projection(
            model,
            Some(&ScopedActionsReadBodyV1::Logs(snapshot.clone())),
        )?;
        let mut workspace = ActionsWorkspace::new(
            work.page.identity.clone(),
            vec![snapshot.run().repository().into()],
        );
        workspace.view = ActionsView::Logs;
        workspace.fetched_at = work.page.fetched_at;
        workspace.run = Some(RunPage {
            repository: workspace.repositories[0].clone(),
            attempt: model.as_ref().clone(),
            selected_job: Some(snapshot.job().id()),
            logs: Some(snapshot.text().into()),
            logs_skipped_bytes: 0,
            annotations: vec![],
        });
        workspace.warnings.push(format!("Loaded snapshot only • run {} attempt {} job {} • no reload or review/source coordinates",snapshot.run().run().id,snapshot.run().run().attempt,snapshot.job().id()));
        workspace.prepare_rows();
        for &index in matches.indices() {
            if let Some(row) = workspace.rows.get_mut(index) {
                row.role = crate::theme::SemanticToken::Reference;
            }
        }
        if let Some((selected, top)) = work.position_v2 {
            workspace.selected = selected.min(workspace.rows.len().saturating_sub(1));
            workspace.top = top.min(workspace.rows.len().saturating_sub(1));
        }
        work.source.prepare_policy(&mut workspace, false);
        let page = Arc::new(ScopedActionsPageV1 {
            source: work.source.clone(),
            kind: work.page.kind.clone(),
            view: ActionsView::Logs,
            selected_job: Some(snapshot.job().id()),
            identity: work.page.identity.clone(),
            fetched_at: work.page.fetched_at,
        });
        let retained_bytes = charge(&workspace, &page)?.saturating_add(matches.owned_bytes());
        if retained_bytes > PREPARED_LIMIT_V1 {
            return Err("Original log matches exceed prepared8MiB capacity".into());
        }
        work.check()?;
        return Ok(PreparedScopedActionsV1 {
            work,
            page,
            workspace,
            retained_bytes,
            matches_v2: Some(matches),
            watch_progress_v2: None,
            transport_bytes_v2: 0,
        });
    }
    if let Some(request) = work
        .resource_v2
        .as_ref()
        .filter(|request| request.is_retained())
    {
        let mut workspace = ActionsWorkspace::new(
            work.page.identity.clone(),
            vec![request
                .retained_repository()
                .ok_or("Original retained resource repository unavailable")?
                .into()],
        );
        let resource = request.prepare(&mut workspace, None, &work.cancellation)?;
        workspace.fetched_at = work.page.fetched_at;
        workspace.warnings.push("Retained observed archive • no fresh API access, extraction, export or publication permission".into());
        workspace.prepare_rows();
        resource.prepare_run_controls(&mut workspace);
        work.source.prepare_policy(&mut workspace, false);
        let page = Arc::new(ScopedActionsPageV1 {
            source: work.source.clone(),
            kind: ScopedActionsPageKindV1::Resource(resource),
            view: workspace.view,
            selected_job: None,
            identity: work.page.identity.clone(),
            fetched_at: work.page.fetched_at,
        });
        let retained_bytes = charge(&workspace, &page)?;
        work.check()?;
        return Ok(PreparedScopedActionsV1 {
            work,
            page,
            workspace,
            retained_bytes,
            matches_v2: None,
            watch_progress_v2: None,
            transport_bytes_v2: 0,
        });
    }
    if let ScopedActionsRequestKindV1::PreparedView(view) = work.kind {
        let ScopedActionsPageKindV1::Attempt { model, .. } = &work.page.kind else {
            return Err("Original retained attempt unavailable".into());
        };
        crate::actions_workspace::scoped_read_v1::preflight_projection(model, None)?;
        let mut workspace = ActionsWorkspace::new(
            work.page.identity.clone(),
            vec![work
                .run
                .as_ref()
                .ok_or("Original run unavailable")?
                .repository()
                .into()],
        );
        workspace.run = Some(RunPage {
            repository: workspace.repositories[0].clone(),
            attempt: model.as_ref().clone(),
            selected_job: work.job.as_ref().map(|job| job.id()),
            logs: None,
            logs_skipped_bytes: 0,
            annotations: vec![],
        });
        workspace.view = view;
        workspace.fetched_at = work.page.fetched_at;
        workspace.warnings.push("Retained observed attempt • this view adds no current API access or publication permission".into());
        workspace.prepare_rows();
        crate::actions_workspace::scoped_read_v1::focus_selected_job(&mut workspace);
        work.source.prepare_policy(&mut workspace, false);
        let page = Arc::new(ScopedActionsPageV1 {
            source: work.source.clone(),
            kind: work.page.kind.clone(),
            view,
            selected_job: work.job.as_ref().map(|job| job.id()),
            identity: work.page.identity.clone(),
            fetched_at: work.page.fetched_at,
        });
        let retained_bytes = charge(&workspace, &page)?;
        work.check()?;
        return Ok(PreparedScopedActionsV1 {
            work,
            page,
            workspace,
            retained_bytes,
            matches_v2: None,
            watch_progress_v2: None,
            transport_bytes_v2: 0,
        });
    }
    let archive_target = work
        .resource_v2
        .as_ref()
        .and_then(|request| request.archive_target());
    let original_bounded = if dispatch.is_some() {
        provider
            .clone()
            .with_catalogue_read_deadline(work.deadline)
            .with_timeout(remaining)
    } else {
        provider.clone().with_timeout(remaining)
    };
    let bounded = if archive_target.is_some() {
        original_bounded.with_actions_archive_read_deadline_v2(work.deadline)
    } else if dispatch.is_some() {
        original_bounded
    } else {
        original_bounded
            .with_scoped_read_window(work.deadline, 512 * 1024)
            .map_err(|_| "Original Actions window unavailable")?
    };
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_duration = remaining;
    limits.transport.max_response_bytes = if dispatch.is_some() {
        2 * 1024 * 1024
    } else if archive_target.is_some() {
        9 * 1024 * 1024
    } else {
        4 * 1024 * 1024
    };
    limits.transport.max_document_bytes = if dispatch.is_some() {
        64 * 1024
    } else {
        128 * 1024
    };
    limits.max_references = 1;
    limits.max_repository_reads = 2;
    let mut session = bounded
        .begin_repository_catalogue_access(
            work.source.original.source(),
            &limits,
            &work.cancellation,
        )
        .map_err(|_| "Original Actions principal unavailable")?;
    let window = if let Some(request) = dispatch {
        validate_read_source_v7(&work, dispatch, database)
            .map_err(|_| "Original returned-run SQL/source unavailable")?;
        session
            .verify(&work.source.reference, &work.cancellation)
            .map_err(|_| "Original Dispatch repository unavailable before read allocation")?;
        session.reserve_dispatch_reads_v7(request.intent().original(), 18, 1, &work.cancellation)
    } else if let Some(target) = archive_target {
        session
            .verify(&work.source.reference, &work.cancellation)
            .map_err(|_| "Original artifact repository unavailable before allocation")?;
        session.reserve_actions_archive_v2(target, 1, &work.cancellation)
    } else {
        session.reserve_measured_external_reads(
            24,
            512 * 1024,
            2 * 1024 * 1024,
            2,
            &work.cancellation,
        )
    }
    .map_err(|_| "Original Actions measured window unavailable")?;
    let scoped = bounded
        .clone()
        .with_measured_read_window(window)
        .map_err(|_| "Original Actions measured window cannot be replaced")?;
    let opening_identity = session.identity().clone();
    let current = if archive_target.is_some() || dispatch.is_some() {
        session
            .observed_original_v1(work.source.reference.id)
            .ok_or("Actual original artifact repository observation lost")?
    } else {
        session
            .verify(&work.source.reference, &work.cancellation)
            .map_err(|_| "Original Actions repository unavailable")?
    };
    if current.repository().permissions.pull != Some(true) {
        return Err("Original Actions pull access unavailable; no query fallback".into());
    }
    let canonical = current.repository().full_name.clone();
    if let Some(pr) = work.source.pr_origin_v2() {
        pr.fresh_parent(&scoped, &opening_identity, current, &work.cancellation)
            .map_err(|_| "Original PR/check/head unavailable before Actions resource")?;
        validate_read_source_v7(&work, dispatch, database)
            .map_err(|_| "Original PR Actions local source changed")?;
    }
    enum Inner {
        Catalogue(ScopedActionsCatalogueReadV1),
        Attempt(Box<ScopedActionsAttemptReadV1>),
        Resource(ScopedActionsResourceReadV2),
    }
    let read = match work.kind {
        ScopedActionsRequestKindV1::Resource => work
            .resource_v2
            .as_ref()
            .ok_or("Original resource request unavailable")?
            .read(&scoped, current, &work.cancellation, || {
                work.source.validate_local(database)
            })
            .map(Inner::Resource),
        ScopedActionsRequestKindV1::Catalogue => scoped
            .scoped_actions_catalogue_page_v2(
                current,
                match work.query_v2.clone() {
                    Some(query) => query,
                    None => ScopedRunQueryV2::first(RunFilter::default())
                        .map_err(|_| "Default original run query unavailable")?,
                },
                &work.cancellation,
            )
            .map(Inner::Catalogue),
        kind => {
            let run = work
                .run
                .as_ref()
                .ok_or("Actual original run target unavailable")?
                .clone();
            let read_kind = if let Some(request) = dispatch {
                request.read_kind()
            } else {
                match kind {
                    ScopedActionsRequestKindV1::Logs => ScopedActionsReadKindV1::Logs,
                    ScopedActionsRequestKindV1::Annotations => ScopedActionsReadKindV1::Annotations,
                    _ => ScopedActionsReadKindV1::Attempt,
                }
            };
            scoped
                .scoped_actions_attempt_v1(
                    current,
                    run,
                    work.job.clone(),
                    read_kind,
                    &work.cancellation,
                    || validate_read_source_v7(&work, dispatch, database),
                )
                .map(|read| Inner::Attempt(Box::new(read)))
        }
    };
    #[cfg(test)]
    if let Some(fault) = work.after_read_fault_v1 {
        fault();
    }
    let prepared = (|| -> Result<(ActionsWorkspace, Arc<ScopedActionsPageV1>, usize), String> {
        let read =
            read.map_err(|_| "Original Actions read unavailable; no private diagnostics retained")?;
        let mut workspace =
            ActionsWorkspace::new(opening_identity.clone(), vec![canonical.clone()]);
        let (kind, view, selected_job) = match read {
            Inner::Resource(read) => {
                let resource = work
                    .resource_v2
                    .as_ref()
                    .ok_or("Original resource request lost")?
                    .prepare(&mut workspace, Some(read), &work.cancellation)?;
                let view = workspace.view;
                (ScopedActionsPageKindV1::Resource(resource), view, None)
            }
            Inner::Catalogue(read) => {
                let (catalog, runs, window) = read.into_parts();
                workspace.runs = catalog
                    .runs
                    .into_iter()
                    .map(|run| ScopedRun {
                        repository: canonical.clone(),
                        run,
                    })
                    .collect();
                workspace.coverage = catalog.coverage;
                (
                    ScopedActionsPageKindV1::Catalogue { runs, window },
                    ActionsView::Runs,
                    None,
                )
            }
            Inner::Attempt(read) => {
                let read = *read;
                if read.requested().as_ref().run().id
                    != work.run.as_ref().ok_or("Original run lost")?.run().id
                    || !Arc::ptr_eq(
                        read.requested(),
                        work.run.as_ref().ok_or("Original run lost")?,
                    )
                    || read.selected().map(|job| job.id()) != work.job.as_ref().map(|job| job.id())
                {
                    return Err(
                        "Actual Actions prepared read does not belong to this request".into(),
                    );
                }
                let run = read.target().clone();
                let jobs = read.jobs().clone();
                let (attempt, body) = read.into_parts();
                crate::actions_workspace::scoped_read_v1::preflight_projection(
                    &attempt,
                    Some(&body),
                )?;
                if crate::actions_workspace::scoped_read_v1::attempt_owned_bytes(&attempt)
                    > PREPARED_LIMIT_V1 / 2
                {
                    return Err(
                        "Original Actions model clone would exceed prepared allocation budget"
                            .into(),
                    );
                }
                let model = Arc::new(attempt.clone());
                let selected_job = work.job.as_ref().map(|job| job.id());
                let mut page = RunPage {
                    repository: canonical.clone(),
                    attempt,
                    selected_job,
                    logs: None,
                    logs_skipped_bytes: 0,
                    annotations: vec![],
                };
                let mut loaded = None;
                let view = match body {
                    ScopedActionsReadBodyV1::Attempt => {
                        if let Some(view) = work.requested_view_v2 {
                            view
                        } else if work.page.view == ActionsView::Jobs {
                            ActionsView::Jobs
                        } else {
                            ActionsView::Run
                        }
                    }
                    ScopedActionsReadBodyV1::Logs(text) => {
                        page.logs = Some(text.text().into());
                        loaded = Some(text);
                        ActionsView::Logs
                    }
                    ScopedActionsReadBodyV1::Annotations(rows) => {
                        page.annotations = rows;
                        ActionsView::Annotations
                    }
                    ScopedActionsReadBodyV1::Unavailable(reason) => {
                        workspace.warnings.push(reason.into());
                        ActionsView::Jobs
                    }
                };
                workspace.run = Some(page);
                (
                    ScopedActionsPageKindV1::Attempt {
                        run,
                        jobs,
                        model,
                        loaded,
                    },
                    view,
                    selected_job,
                )
            }
        };
        workspace.view = view;
        workspace.fetched_at = Some(crate::storage::now());
        workspace.prepare_rows();
        crate::actions_workspace::scoped_read_v1::prepare_labels(&mut workspace);
        if let ScopedActionsPageKindV1::Resource(resource) = &kind {
            resource.prepare_run_controls(&mut workspace);
        }
        crate::actions_workspace::scoped_read_v1::focus_selected_job(&mut workspace);
        if dispatch.is_some() {
            crate::scoped_actions_native_v1::publication_v3::dispatch_v7::run_read_v7::prepare_policy(&mut workspace,false);
        } else {
            work.source.prepare_policy(&mut workspace, false);
        }
        let page = Arc::new(ScopedActionsPageV1 {
            source: work.source.clone(),
            kind,
            view,
            selected_job,
            identity: workspace.identity.clone(),
            fetched_at: workspace.fetched_at,
        });
        if let Some(window) = page.run_window_v2() {
            crate::actions_workspace::scoped_read_v1::prepare_catalogue_window_v2(
                &mut workspace,
                window,
            );
        }
        let retained_bytes = charge(&workspace, &page)?;
        Ok((workspace, page, retained_bytes))
    })();
    if let Some(pr) = work.source.pr_origin_v2() {
        pr.fresh_parent(&scoped, &opening_identity, current, &work.cancellation)
            .map_err(|_| "Closing original PR/check/head unavailable")?;
    }
    let closing = session
        .verify(&work.source.reference, &work.cancellation)
        .map_err(|_| "Closing original Actions repository unavailable")?;
    if closing.repository().full_name != canonical
        || closing.repository().permissions.pull != Some(true)
    {
        return Err("Original Actions access/name changed during read".into());
    }
    let closed = session
        .close_with_guard(&work.cancellation, || {
            validate_read_source_v7(&work, dispatch, database)
        })
        .map_err(|_| "Original Actions closing source/account proof unavailable")?;
    work.check()?;
    let (mut workspace, mut page, mut retained_bytes) = prepared?;
    // Tiny same-numeric-principal presentation continuity only; no body or
    // SQL after final principal and no foreground re-preparation.
    replace_final_identity(
        &mut workspace,
        &mut page,
        &mut retained_bytes,
        closed.identity(),
    )?;
    work.check()?;
    Ok(PreparedScopedActionsV1 {
        work,
        page,
        workspace,
        retained_bytes,
        matches_v2: None,
        watch_progress_v2: None,
        transport_bytes_v2: closed.charged_bytes(),
    })
}
#[cfg(test)]
#[path = "scoped_actions_v1_tests.rs"]
mod tests;
