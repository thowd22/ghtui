//! Closed PR-check origin for SAME Actions worker, source and measured ledger.
//! This retains an actual route, not an invented Catalogue/Files/DTO capability.
use super::*;
use crate::providers::gh::{
    CurrentCatalogueRepository, PrResourceParentV1, ThreadHistoryParentObservationV1,
};
use crate::providers::gh::{GithubResult, WorkflowJsonLedgerV9};
pub struct ScopedPrActionsOriginV2 {
    route: Arc<PrChecksRoute>,
}
pub(crate) fn resource_entry_action(action: &ActionsAction) -> bool {
    matches!(
        action,
        ActionsAction::Artifacts
            | ActionsAction::View(
                ActionsView::Workflows
                    | ActionsView::Secrets
                    | ActionsView::Variables
                    | ActionsView::Policies
                    | ActionsView::Environments
                    | ActionsView::Runners
                    | ActionsView::Caches
            )
    )
}
/// Only opens a read-only intent. This is not a publication/permission gate.
pub(crate) fn run_command_entry_v4(action: &ActionsAction) -> bool {
    matches!(
        action,
        ActionsAction::Compose(
            crate::actions_workspace::ActionsCommand::Cancel
                | crate::actions_workspace::ActionsCommand::ForceCancel
                | crate::actions_workspace::ActionsCommand::DeleteRun
                | crate::actions_workspace::ActionsCommand::DeleteLogs
                | crate::actions_workspace::ActionsCommand::RerunAll
                | crate::actions_workspace::ActionsCommand::RerunFailed
                | crate::actions_workspace::ActionsCommand::RerunJob
        )
    )
}
pub(super) fn run_command_visible_v4(
    workspace: &ActionsWorkspace,
    observed: &PrCheckRunObservation,
    action: &ActionsAction,
) -> bool {
    use crate::actions_workspace::ActionsCommand as C;
    let Some(selected) = observed.selected_run_v1() else {
        return false;
    };
    let run = &selected.attempt().run;
    if observed.repository().id == 0
        || observed.repository().node_id.is_empty()
        || selected.node_v2().is_empty()
        || run.id == 0
        || run.attempt == 0
        || !workspace.run.as_ref().is_some_and(|page| {
            page.attempt.run.id == run.id
                && page.attempt.run.attempt == run.attempt
                && page.attempt.run.head_sha == run.head_sha
        })
    {
        return false;
    }
    match action {
        ActionsAction::Compose(C::Cancel | C::ForceCancel) => run.status != "completed",
        ActionsAction::Compose(C::DeleteRun | C::DeleteLogs) => true,
        ActionsAction::Compose(C::RerunAll | C::RerunFailed) => run.status == "completed",
        ActionsAction::Compose(C::RerunJob) => {
            run.status == "completed"
                && workspace
                    .run
                    .as_ref()
                    .and_then(|page| page.selected_job)
                    .is_some_and(|id| selected.members().iter().any(|member| member.id() == id))
        }
        _ => false,
    }
}
impl ScopedPrActionsOriginV2 {
    pub(crate) fn prepare_cleanup_pr_v9(
        &self,
        tx: &rusqlite::Transaction<'_>,
        rows: &mut Vec<crate::storage::workflow_cleanup_v9::WorkflowRawRowsV9>,
        heap: &WorkflowJsonLedgerV9,
    ) -> anyhow::Result<()> {
        self.route.guard.prepare_workflow_cleanup_v9(tx, rows, heap)
    }
    /// Borrow only every actual retained owning path. Arc sharing is
    /// conservatively charged again, never guessed free from display DTOs.
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        use crate::providers::gh::workflow_capacity_v9 as b;
        let route = &self.route;
        let origin = &route.origin;
        let mut n = b::sum([
            b::owner::<Self>(),
            b::owner::<PrChecksRoute>(),
            b::owner::<PrChecksOrigin>(),
            b::identity(&origin.identity)?,
            b::summary(&origin.summary)?,
            b::check(&origin.check)?,
            route.observation.workflow_owned_bytes_v9()?,
            route.guard.workflow_owned_bytes_v9()?,
        ])?;
        if let Some(owner) = &origin.scoped {
            n = b::sum([
                n,
                b::owner::<ScopedPrCheckOwnerV1>(),
                owner.source.workflow_owned_bytes_v9()?,
                owner.active.workflow_owned_bytes_v9()?,
                owner.navigation.workflow_owned_bytes_v9()?,
                owner.resource.workflow_owned_bytes_v9()?,
                b::strings([
                    &owner.cached_selected.host,
                    &owner.cached_selected.repository,
                ])?,
            ])?;
        }
        if let Some(local) = &route.scoped_local {
            n = n.checked_add(local.workflow_owned_bytes_v9()?)?;
        }
        if let Some(rows) = &route.choice_rows {
            n = n.checked_add(rows.workflow_owned_bytes_v9()?)?;
        }
        if let Some(logs) = &route.loaded_capture_v2 {
            n = n.checked_add(logs.workflow_owned_bytes_v9()?)?;
        }
        if let Some(annotations) = &route.annotations_v2 {
            n = n.checked_add(annotations.workflow_owned_bytes_v9()?)?;
        }
        Some(n)
    }
    pub(crate) fn fresh_workflow_history_parent_v9(
        &self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        heap: &WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        let scoped = self
            .route
            .origin
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        let expected = ThreadHistoryParentObservationV1::capture(
            scoped.resource.observed().parent().summary(),
        )?;
        provider.pr_thread_workflow_history_parent_v9(current, &expected, heap, cancel)
    }
    pub(crate) fn fresh_history_parent_v5(
        &self,
        provider: &GithubProvider,
        current: &CurrentCatalogueRepository,
        cancel: &CancellationToken,
    ) -> GithubResult<PrResourceParentV1> {
        let scoped = self
            .route
            .origin
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        let expected = ThreadHistoryParentObservationV1::capture(
            scoped.resource.observed().parent().summary(),
        )?;
        provider.pr_thread_history_parent_v1(current, &expected, cancel)
    }
    pub(crate) fn capture(app: &App) -> Result<Arc<Self>, String> {
        let route = app
            .pr_checks_route
            .clone()
            .filter(|route| route.scoped_v1())
            .ok_or("Actual scoped PR-check route required")?;
        if !owns_origin(app, &route.origin)
            || route.observation.selected_run_v1().is_none()
            || app.pr_check_continuation_owner_v2.is_some()
            || app.pr_check_continuation_request_v2.is_some()
            || app.pr_check_watch_v2.is_some()
        {
            return Err(
                "Wait/Stop the original PR-check read/watch before opening its Actions resources"
                    .into(),
            );
        }
        Ok(Arc::new(Self { route }))
    }
    pub(crate) fn current(&self, app: &App) -> bool {
        owns_origin(app, &self.route.origin)
            && app
                .pr_checks_route
                .as_ref()
                .is_some_and(|route| Arc::ptr_eq(route, &self.route))
    }
    pub(crate) fn source(&self) -> &Arc<CatalogueIntentSource> {
        &self
            .route
            .origin
            .scoped
            .as_ref()
            .expect("closed scoped origin")
            .source
    }
    pub(crate) fn scope(&self) -> &Arc<ActiveRemoteScopeV1> {
        &self
            .route
            .origin
            .scoped
            .as_ref()
            .expect("closed scoped origin")
            .active
    }
    pub(crate) fn reference(&self) -> &crate::repository_catalogue_intent_v1::IntentReference {
        self.route
            .origin
            .scoped
            .as_ref()
            .expect("closed scoped origin")
            .navigation
            .original()
    }
    pub(crate) fn observation(&self) -> &Arc<PrCheckRunObservation> {
        &self.route.observation
    }
    /// Descriptive durable identities from the SAME genuine original parent;
    /// this is not a current source or an authorization/inspection permit.
    pub(crate) fn durable_pr_context_v3(
        &self,
    ) -> crate::actions_write_journal::ActionsOriginalPrContextV3 {
        let parent = self
            .route
            .origin
            .scoped
            .as_ref()
            .expect("closed scoped origin")
            .resource
            .observed()
            .parent();
        crate::actions_write_journal::ActionsOriginalPrContextV3 {
            id: parent.database_id(),
            node: parent.node_id().into(),
            number: parent.number(),
            head_sha: parent.head_sha().into(),
            base_sha: parent.base_tip_sha().into(),
        }
    }
    pub(crate) fn validate_local(&self, database: &Path) -> GithubResult<()> {
        let original = self
            .route
            .origin
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        self.route
            .scoped_local
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
            .validate(database, &original.source)?;
        self.route.guard.validate(database).map_err(safe_sql)
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
        let original = self
            .route
            .origin
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        self.route
            .scoped_local
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
            .validate(database, &original.source)?;
        self.route
            .guard
            .validate_owned_publication_v4(database, journal, claim, result)
            .map_err(safe_sql)
    }
    pub(crate) fn fresh_parent(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        current: &crate::providers::gh::CurrentCatalogueRepository,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let original = self
            .route
            .origin
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        provider.scoped_actions_pr_parent_v2(
            identity,
            current,
            original.resource.observed(),
            original.check_index,
            &self.route.observation,
            cancel,
        )
    }
    pub(crate) fn fresh_workflow_edit_parent_v9(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        current: &CurrentCatalogueRepository,
        heap: &crate::providers::gh::WorkflowJsonLedgerV9,
        cancel: &CancellationToken,
    ) -> GithubResult<()> {
        let original = self
            .route
            .origin
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        provider.scoped_actions_pr_parent_heap_v9(
            (identity, current),
            original.resource.observed(),
            original.check_index,
            &self.route.observation,
            heap,
            cancel,
        )
    }
    pub(crate) fn validate_owned_run_history_v8(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedRunHistoryV8,
    ) -> GithubResult<()> {
        let original = self
            .route
            .origin
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        self.route
            .scoped_local
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
            .validate(database, &original.source)?;
        self.route
            .guard
            .validate_owned_run_history_v8(database, journal, proof)
            .map_err(safe_sql)
    }
    pub(crate) fn validate_owned_dispatch_history_v7(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedDispatchHistoryV7,
    ) -> GithubResult<()> {
        let original = self
            .route
            .origin
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        self.route
            .scoped_local
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
            .validate(database, &original.source)?;
        self.route
            .guard
            .validate_owned_dispatch_history_v7(database, journal, proof)
            .map_err(safe_sql)
    }
    pub(crate) fn validate_owned_workflow_history_v6(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedWorkflowHistoryV6,
    ) -> GithubResult<()> {
        let original = self
            .route
            .origin
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        self.route
            .scoped_local
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
            .validate(database, &original.source)?;
        self.route
            .guard
            .validate_owned_workflow_history_v6(database, journal, proof)
            .map_err(safe_sql)
    }
    pub(crate) fn validate_owned_workflow_remote_history_v9(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        proof: &crate::actions_write_journal::ActionsOwnedWorkflowRemoteHistoryV9,
    ) -> GithubResult<()> {
        let original = self
            .route
            .origin
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        self.route
            .scoped_local
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
            .validate(database, &original.source)?;
        self.route
            .guard
            .validate_owned_workflow_remote_history_v9(database, journal, proof)
            .map_err(safe_sql)
    }
    pub(crate) fn validate_workflow_branch_transition_v9(
        &self,
        database: &Path,
        journal: &crate::actions_write_journal::ActionsWriteJournal,
        branch: &crate::scoped_actions_workflow_edit_v9::WorkflowPublishedBranchV9,
    ) -> GithubResult<()> {
        let original = self
            .route
            .origin
            .scoped
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?;
        self.route
            .scoped_local
            .as_ref()
            .ok_or_else(|| safe_failure(GithubErrorKind::IdentityChanged))?
            .validate(database, &original.source)?;
        self.route
            .guard
            .validate_workflow_branch_transition_v9(database, journal, branch)
            .map_err(safe_sql)
    }
}
