//! Closed PR-check origin for SAME Actions worker, source and measured ledger.
//! This retains an actual route, not an invented Catalogue/Files/DTO capability.
use super::*;
use crate::providers::gh::GithubResult;
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
impl ScopedPrActionsOriginV2 {
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
}
