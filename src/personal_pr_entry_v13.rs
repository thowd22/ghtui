//! Personal PR entry keeps the ACTUAL original page/details/body on App.
//! Work owns bounded expectations only; no error/Drop can restore stale text.
use crate::{
    active_remote_scope_v1::{ActiveRemoteScopeV1, RemoteScopeConsumerV1},
    app::{App, View},
    github_workspace::{PrKey, PrRow, PrTab},
    providers::{
        gh::{
            GithubProvider, PrParentIntentV13, PrResourceReadLimitsV1,
            RepositoryCatalogueAccessLimits,
        },
        runner::CancellationToken,
    },
    scoped_pr_observed_tabs_v1::{resources::ScopedPrResourceViewV1, ScopedPrNavigationV1},
    storage::{CatalogueIntentSource, PersonalPrSourceV13, Scope},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// Actual factory presentation epoch only, NEVER a source or auth constructor.
#[derive(Debug)]
pub struct PersonalPrPageTicketV13 {
    _private: (),
}
#[path = "personal_pr_collaboration_presentation_v17.rs"]
pub(crate) mod collaboration_v17;
#[path = "personal_pr_summary_history_phase_v75.rs"]
pub(crate) mod summary_history_v75;
#[path = "personal_pr_summary_presentation_v14.rs"]
pub(crate) mod summary_v14;
impl PersonalPrPageTicketV13 {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self { _private: () })
    }
}
struct OriginalPersonalPresentationV13 {
    numeric_scope: Scope,
    page: Arc<PersonalPrPageTicketV13>,
    parent: Arc<PrParentIntentV13>,
    selected: PrKey,
    scope: Option<Arc<ActiveRemoteScopeV1>>,
    catalogue: Option<Arc<CatalogueIntentSource>>,
    selected_row: usize,
    scroll: usize,
    focus: usize,
    content_focus: bool,
    tab: PrTab,
}
impl OriginalPersonalPresentationV13 {
    fn current_pointers(&self, app: &App) -> bool {
        app.github
            .as_ref()
            .is_some_and(|github| self.retained_pointers_v19(app, github))
    }
    /// Actual retained Workspace only, called by the closed terminal-return
    /// state. Same real page ticket/tuple allocations, never a temporary App
    /// or a permission/source minted from its details.
    fn retained_pointers_v19(
        &self,
        app: &App,
        github: &crate::github_workspace::GithubWorkspace,
    ) -> bool {
        let scope = match (&self.scope, &app.remote_scope) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        let catalogue = match (&self.catalogue, &app.remote_scope_source) {
            (None, None) => true,
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            _ => false,
        };
        app.view == View::Inbox
            && scope
            && catalogue
            && app.personal_pr_source_v13.is_none()
            && github
                .personal_pr_page_ticket_v13
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(v, &self.page))
            && github.selected.as_ref() == Some(&self.selected)
            && github.scoped_pr_navigation.is_none()
            && github.scoped_pr_resource.is_none()
            && github.pr.as_ref().is_some_and(|page| {
                page.tab == self.tab && self.parent.matches_observed_page_v13(&page.details.summary)
            })
    }
    fn matches(&self, app: &App) -> bool {
        self.current_pointers(app)
            && app.overlay.is_none()
            && app.form.is_none()
            && app.github.as_ref().is_some_and(|github| {
                github.identity.host == self.parent.identity().host
                    && github.identity.account_id == self.parent.identity().account_id
            })
            && app.selected == self.selected_row
            && app.scroll == self.scroll
            && app.focus == self.focus
            && app.github_content_focused == self.content_focus
    }
}
pub struct PersonalPrEntryWorkV13 {
    original: OriginalPersonalPresentationV13,
    chosen_default: Arc<ActiveRemoteScopeV1>,
    database: PathBuf,
    cancel: CancellationToken,
    deadline: Instant,
}
impl std::fmt::Debug for PersonalPrEntryWorkV13 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalPrEntryWorkV13(<bounded original page intent; no body or permission>)")
    }
}
impl PersonalPrEntryWorkV13 {
    /// ALL fields are cached and bounded. Do not canonicalize/open the database
    /// or scan/cl​​one the Inbox/body here; its real boundary is worker-owned.
    pub fn capture(app: &App, database: &Path) -> Result<Arc<Self>, String> {
        if crate::scoped_pr_collaboration_native_v17::retired_presentation(app) {
            return Err(
                "Hidden original Conversation page cannot issue new Files/read authority".into(),
            );
        }
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(30))
            .ok_or("Original personal deadline unavailable")?;
        let cancel = CancellationToken::default();
        if app.view != View::Inbox
            || app.overlay.is_some()
            || app.loading[View::Inbox.index()]
            || app.personal_pr_entry_owner_v13.is_some()
            || app.personal_pr_entry_work_v13.is_some()
            || app.personal_pr_source_v13.is_some()
            || app.form.is_some()
        {
            return Err("Open an idle personal PR before loading its verified Files".into());
        }
        let github = app
            .github
            .as_ref()
            .ok_or("Original personal Inbox unavailable")?;
        let page = github
            .pr
            .as_ref()
            .ok_or("Open the actual personal PR first")?;
        let selected = github
            .selected
            .as_ref()
            .ok_or("Original personal selection unavailable")?;
        if github.scoped_pr_navigation.is_some()
            || github.scoped_pr_resource.is_some()
            || PrKey::from(&page.details.summary.pr) != *selected
            || database.as_os_str().len() > 16 * 1024
        {
            return Err("Original personal page/target unavailable; no scoped fallback".into());
        }
        let ticket = github
            .personal_pr_page_ticket_v13
            .as_ref()
            .ok_or("Actual personal page-install ticket unavailable; reopen explicitly")?;
        let chosen_default = match &app.remote_scope {
            Some(scope)
                if matches!(
                    scope.consumer(&github.identity),
                    RemoteScopeConsumerV1::DefaultPersonal
                ) =>
            {
                scope.clone()
            }
            Some(_) => {
                return Err(
                    "Explicit/unresolved/fenced scope cannot fall back to personal PR entry".into(),
                )
            }
            None => Arc::new(
                ActiveRemoteScopeV1::default_personal(&github.identity)
                    .map_err(|e| e.to_string())?,
            ),
        };
        let parent =
            PrParentIntentV13::from_observed_page_v13(&github.identity, &page.details.summary)
                .map_err(|e| e.to_string())?;
        let numeric_scope = Scope::github(&github.identity.host, github.identity.account_id)
            .map_err(|_| "Original personal numeric account unavailable")?;
        let work = Arc::new(Self {
            original: OriginalPersonalPresentationV13 {
                numeric_scope,
                page: ticket.clone(),
                parent,
                selected: selected.clone(),
                scope: app.remote_scope.clone(),
                catalogue: app.remote_scope_source.clone(),
                selected_row: app.selected,
                scroll: app.scroll,
                focus: app.focus,
                content_focus: app.github_content_focused,
                tab: page.tab,
            },
            chosen_default,
            database: database.to_owned(),
            cancel,
            deadline,
        });
        if work.expired() || !work.original.matches(app) {
            return Err(
                "Original personal entry expired/changed before capture; page untouched".into(),
            );
        }
        Ok(work)
    }
    pub(crate) fn expired(&self) -> bool {
        self.cancel.is_cancelled() || Instant::now() >= self.deadline
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn original_deadline(&self) -> Instant {
        self.deadline
    }
    pub fn host(&self) -> &str {
        &self.original.parent.identity().host
    }
    pub(crate) fn matches_capture(&self, app: &App) -> bool {
        !self.expired()
            && self.original.matches(app)
            && app
                .personal_pr_entry_work_v13
                .as_ref()
                .is_none_or(|v| std::ptr::eq(v.as_ref(), self))
    }
    pub(crate) fn parent(&self) -> &Arc<PrParentIntentV13> {
        &self.original.parent
    }
}
pub struct PersonalPrEntryOwnerV13 {
    work: Arc<PersonalPrEntryWorkV13>,
}
impl std::fmt::Debug for PersonalPrEntryOwnerV13 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalPrEntryOwnerV13(<exact original pending read>)")
    }
}
impl PersonalPrEntryOwnerV13 {
    pub(crate) fn new(app: &App, work: Arc<PersonalPrEntryWorkV13>) -> Result<Arc<Self>, String> {
        if !work.matches_capture(app) || app.personal_pr_entry_owner_v13.is_some() {
            return Err("Original personal entry no longer current; no slot cancelled".into());
        }
        Ok(Arc::new(Self { work }))
    }
    pub(crate) fn work(&self) -> &Arc<PersonalPrEntryWorkV13> {
        &self.work
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        self.work.cancellation()
    }
    pub(crate) fn expired(&self) -> bool {
        self.work.expired()
    }
    pub(crate) fn owns(&self, app: &App) -> bool {
        self.work.original.matches(app)
            && app
                .personal_pr_entry_work_v13
                .as_ref()
                .is_none_or(|v| Arc::ptr_eq(v, &self.work))
            && app
                .personal_pr_entry_owner_v13
                .as_ref()
                .is_some_and(|v| std::ptr::eq(v.as_ref(), self))
    }
    pub(crate) fn same_original_page(&self, app: &App) -> bool {
        self.work.original.current_pointers(app)
    }
    pub(crate) fn original_scope(&self) -> &Scope {
        &self.work.original.numeric_scope
    }
    /// Account-loss privacy is independent of this old read's physical slot.
    /// Exact source/page/Nav identity still precedes every private effect.
    pub(crate) fn retire_account_mismatch(&self, app: &mut App) -> bool {
        if !self.same_original_page(app)
            || app
                .github
                .as_ref()
                .is_none_or(|g| g.scope().ok().as_ref() == Some(self.original_scope()))
        {
            return false;
        }
        retire_original_presentation(app);
        true
    }
}
fn retire_original_presentation(app: &mut App) {
    const HIDDEN: &str =
        "Original Personal PR private content hidden • refresh the current account explicitly";
    app.context = HIDDEN.into();
    app.content[View::Inbox.index()] = vec![HIDDEN.into()];
    if app.overlay == Some(crate::app::Overlay::Notice) {
        app.overlay = None;
        app.notice.clear();
        app.notice_scroll = 0;
    }
    app.github = None;
    app.github_action = None;
    app.personal_pr_source_v13 = None;
    app.pr_file_images = None;
    app.pr_native_diff_v1 = None;
    app.pr_native_diff_layout_v1 = Default::default();
    app.pr_reactions = None;
    app.pr_reaction_display.clear();
    app.pr_inspection = None;
    app.pr_pending_message.clear();
    app.pr_action = None;
    // No shared cache shutdown, Background generation/slot/token, loading or
    // successor status effect. These public assets are no longer painted.
    app.hits.clear();
}
pub(crate) fn terminal_failure(
    app: &mut App,
    owner: &Arc<PersonalPrEntryOwnerV13>,
    failure: PersonalPrEntryFailureV13,
) {
    if !owner.owns(app) {
        return;
    }
    if failure.hide_private {
        retire_original_presentation(app);
    }
    app.personal_pr_entry_owner_v13 = None;
    app.personal_pr_entry_work_v13 = None;
    app.loading[View::Inbox.index()] = false;
    app.status = failure.message.into();
}
pub(crate) fn expired_failure() -> PersonalPrEntryFailureV13 {
    bounded_failure()
}
/// Original presentation only: moved once after accepted success, never body.
struct PersonalPrReturnPresentationV13 {
    rows: Vec<PrRow>,
    tab: PrTab,
    diff: Option<crate::providers::gh::PrDiff>,
    checks: Option<Vec<crate::providers::gh::Check>>,
    context: Option<crate::providers::gh::PrContext>,
    progress: Option<crate::pr_progress::ReviewProgress>,
    selected: usize,
    scroll: usize,
    focus: usize,
    content_focus: bool,
    old_repository: String,
    old_scope: Option<Arc<ActiveRemoteScopeV1>>,
    old_catalogue: Option<Arc<CatalogueIntentSource>>,
}
pub struct PersonalPrEntryStateV13 {
    source: Arc<PersonalPrSourceV13>,
    page: Arc<PersonalPrPageTicketV13>,
    active: Arc<ActiveRemoteScopeV1>,
    navigation: Arc<ScopedPrNavigationV1>,
    selected: PrKey,
    previous: Option<PersonalPrReturnPresentationV13>,
    // Exact orphaned Confirm display retirement only. Never changes owns() or
    // permits capture/read/publication from a missing genuine PR page.
    private_retired_v14: bool,
}
impl std::fmt::Debug for PersonalPrEntryStateV13 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PersonalPrEntryStateV13(<current actual source; original body remains App>)")
    }
}
impl PersonalPrEntryStateV13 {
    pub(crate) fn source(&self) -> &Arc<PersonalPrSourceV13> {
        &self.source
    }
    pub(crate) fn active(&self) -> &Arc<ActiveRemoteScopeV1> {
        &self.active
    }
    pub(crate) fn navigation(&self) -> &Arc<ScopedPrNavigationV1> {
        &self.navigation
    }
    /// Caller has already matched the closed image result's original
    /// source/page/old-nav before this O(1) genuine navigation move.
    pub(crate) fn install_image_navigation(&mut self, navigation: Arc<ScopedPrNavigationV1>) {
        self.navigation = navigation;
        self.private_retired_v14 = false;
    }
    fn retained_skeleton_v14(&self, app: &App) -> bool {
        self.private_retired_v14
            && self.same_page(app)
            && app.github.as_ref().is_some_and(|github| {
                github.scope().ok().as_ref() == Some(self.source.scope())
                    && github
                        .scoped_pr_navigation
                        .as_ref()
                        .is_some_and(|nav| Arc::ptr_eq(nav, &self.navigation))
                    && github.pr.is_none()
                    && github.scoped_pr_resource.is_none()
            })
    }
    pub(crate) fn same_page(&self, app: &App) -> bool {
        app.github.as_ref().is_some_and(|github| {
            github
                .personal_pr_page_ticket_v13
                .as_ref()
                .is_some_and(|v| Arc::ptr_eq(v, &self.page))
                && github.selected.as_ref() == Some(&self.selected)
        }) && app
            .remote_scope
            .as_ref()
            .is_some_and(|v| Arc::ptr_eq(v, &self.active))
    }
    pub(crate) fn owns(&self, app: &App) -> bool {
        self.same_page(app)
            && app.github.as_ref().is_some_and(|github| {
                github.scope().ok().as_ref() == Some(self.source.scope())
                    && github
                        .scoped_pr_navigation
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, &self.navigation))
                    && github.pr.as_ref().is_some_and(|page| {
                        crate::scoped_pr_observed_tabs_v1::matches_page(&self.navigation, page)
                    })
            })
    }
}
pub(crate) struct PreparedPersonalPrEntryV13 {
    owner: Arc<PersonalPrEntryOwnerV13>,
    source: Arc<PersonalPrSourceV13>,
    navigation: Arc<ScopedPrNavigationV1>,
    view: Arc<ScopedPrResourceViewV1>,
    rows: Vec<PrRow>,
}
pub struct PersonalPrEntryResultV13 {
    pub(crate) owner: Arc<PersonalPrEntryOwnerV13>,
    pub(crate) result: Result<PreparedPersonalPrEntryV13, PersonalPrEntryFailureV13>,
}
impl PersonalPrEntryResultV13 {
    pub(crate) fn matches(&self, owner: &Arc<PersonalPrEntryOwnerV13>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
            && match &self.result {
                Ok(prepared) => prepared.matches(owner),
                Err(_) => true,
            }
    }
}
impl PreparedPersonalPrEntryV13 {
    pub(crate) fn matches(&self, owner: &Arc<PersonalPrEntryOwnerV13>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
    }
}
#[derive(Debug)]
pub(crate) struct PersonalPrEntryFailureV13 {
    pub(crate) hide_private: bool,
    pub(crate) message: &'static str,
}
fn failed(error: crate::providers::gh::GithubError) -> PersonalPrEntryFailureV13 {
    PersonalPrEntryFailureV13 {
        hide_private: matches!(
            error.kind,
            crate::providers::gh::GithubErrorKind::Authentication
                | crate::providers::gh::GithubErrorKind::IdentityChanged
        ),
        message:
            "Original personal PR verification unavailable; no source installed or mutation sent",
    }
}
fn bounded_failure() -> PersonalPrEntryFailureV13 {
    PersonalPrEntryFailureV13 {hide_private:false,message:"Personal Files preparation exceeds original bound or was cancelled; original page retained"}
}
fn check(work: &PersonalPrEntryWorkV13) -> Result<(), PersonalPrEntryFailureV13> {
    if work.expired() {
        Err(bounded_failure())
    } else {
        Ok(())
    }
}
pub(crate) fn execute(
    owner: Arc<PersonalPrEntryOwnerV13>,
    provider: &GithubProvider,
) -> Result<PreparedPersonalPrEntryV13, PersonalPrEntryFailureV13> {
    use crate::scoped_pr_observed_tabs_v1::resources::{
        self, PrLocalSourceBoundaryV13, ResourceLocalGuard,
    };
    use crate::storage::Cache;
    let work = &owner.work;
    check(work)?;
    let provider = provider.clone().with_catalogue_read_deadline(work.deadline);
    let mut session = provider
        .begin_personal_pr_access_v13(
            work.parent(),
            &RepositoryCatalogueAccessLimits::default(),
            &work.cancel,
        )
        .map_err(failed)?;
    let original = session.original().clone();
    let identity = session.identity().clone();
    let deadline = session.deadline().min(work.deadline);
    let mut reservation = None;
    let mut local = None;
    let read = session
        .read_files_v13(&PrResourceReadLimitsV1::default(), &work.cancel, |parent| {
            let mut cache = Cache::open(&work.database).map_err(|_| resources::safe_local())?;
            let source = cache
                .reserve_personal_pr_source_v13(&original, parent, &work.cancel, deadline)
                .map_err(|_| resources::safe_local())?;
            local = Some(ResourceLocalGuard::capture_boundary_v13(
                &work.database,
                PrLocalSourceBoundaryV13::PersonalReservation(&source),
                &identity,
                parent.summary(),
            )?);
            reservation = Some(source);
            Ok(())
        })
        .map_err(failed)?;
    check(work)?;
    let read = Arc::new(read);
    let (view, rows) = resources::prepare_resource_arc_v13(read.clone(), &work.cancel)
        .map_err(|_| bounded_failure())?;
    let (view, rows) = view.zip(rows).ok_or_else(bounded_failure)?;
    let prepared = reservation
        .ok_or_else(bounded_failure)?
        .prepare(read.clone())
        .map_err(|_| bounded_failure())?;
    let navigation = Arc::new(
        ScopedPrNavigationV1::bind(
            prepared.reservation().scope(),
            original.reference(),
            &PrKey::from(&read.parent().summary().pr),
            read.parent().summary(),
        )
        .map_err(|_| bounded_failure())?,
    );
    check(work)?;
    let closed = session
        .close_with_guard(&work.cancel, || {
            local
                .as_ref()
                .ok_or_else(resources::safe_local)?
                .validate_boundary_v13(
                    &work.database,
                    PrLocalSourceBoundaryV13::PersonalReservation(prepared.reservation()),
                )
        })
        .map_err(failed)?;
    let source = Arc::new(prepared.bind(closed).map_err(|_| bounded_failure())?);
    let result = PreparedPersonalPrEntryV13 {
        owner,
        source,
        navigation,
        view: Arc::new(view),
        rows,
    };
    check(result.owner.work())?;
    Ok(result)
}

/// Called by Main before the legacy/global resource path. It captures only
/// bounded cached intent; explicit/tagged sources are never personal fallback.
pub fn capture_requested_files(app: &mut App, database: &Path) -> bool {
    if !matches!(
        app.github_action,
        Some(crate::github_workspace::GithubAction::PrTab(PrTab::Files))
    ) {
        return false;
    }
    let Some(github) = app.github.as_ref() else {
        return false;
    };
    let personal = match &app.remote_scope {
        None => true,
        Some(active) => matches!(
            active.consumer(&github.identity),
            RemoteScopeConsumerV1::DefaultPersonal
        ),
    };
    if !personal {
        return false;
    }
    app.github_action = None;
    app.refresh_requested = false;
    if app.personal_pr_source_v13.is_some() {
        app.status =
            "Verified personal Files already open • Back returns the original cached page".into();
        return true;
    }
    match PersonalPrEntryWorkV13::capture(app, database) {
        Ok(work) => {
            app.personal_pr_entry_work_v13 = Some(work);
        }
        Err(reason) => app.status = reason,
    }
    true
}
pub fn take_work(app: &mut App) -> Option<Arc<PersonalPrEntryWorkV13>> {
    app.personal_pr_entry_work_v13.take()
}
/// Display/source retirement only; a genuinely newer source/navigation is
/// never cleared merely because an old retained return presentation exists.
pub fn retire_invalid(app: &mut App) {
    let Some(state) = app.personal_pr_source_v13.as_ref() else {
        return;
    };
    let lost_account = state.same_page(app)
        && app
            .github
            .as_ref()
            .and_then(|g| g.scoped_pr_navigation.as_ref())
            .is_some_and(|nav| Arc::ptr_eq(nav, state.navigation()))
        && app
            .github
            .as_ref()
            .is_some_and(|github| github.scope().ok().as_ref() != Some(state.source.scope()));
    if lost_account {
        retire_original_presentation(app);
    } else if !state.owns(app) && !state.retained_skeleton_v14(app) {
        app.personal_pr_source_v13 = None;
    }
}
/// Caller has already established exact current old Confirm Form/input epoch
/// and source/page/navigation. Keeps existing producer allocations only, never
/// invents an empty PR/Files page or permits a missing-page input factory.
pub(crate) fn retain_orphan_source_skeleton_v14(
    app: &mut App,
    source: &crate::pr_read_origin_v13::PrReadOriginV13,
    navigation: &Arc<ScopedPrNavigationV1>,
) -> bool {
    if !source.is_personal()
        || !source.matches_source(app)
        || app
            .personal_pr_source_v13
            .as_ref()
            .is_none_or(|state| !state.owns(app) || !Arc::ptr_eq(state.navigation(), navigation))
    {
        return false;
    }
    app.personal_pr_source_v13
        .as_mut()
        .unwrap()
        .private_retired_v14 = true;
    true
}
pub(crate) fn retire_source(
    app: &mut App,
    source: &crate::pr_read_origin_v13::PrReadOriginV13,
    navigation: &Arc<ScopedPrNavigationV1>,
) -> bool {
    if !source.matches_source(app)
        || app
            .github
            .as_ref()
            .and_then(|g| g.scoped_pr_navigation.as_ref())
            .is_none_or(|v| !Arc::ptr_eq(v, navigation))
    {
        return false;
    }
    retire_original_presentation(app);
    true
}
pub(crate) fn newer_intent(app: &App) -> bool {
    app.personal_pr_entry_work_v13.is_some()
        || app.personal_pr_entry_cancel_v13
        || app.scoped_threads_work_v1.is_some()
        || app.scoped_threads_cancel_v1
        || app.scoped_reviews_work_v2.is_some()
        || app.scoped_reviews_cancel_v2
        || app.scoped_inline_work_v1.is_some()
        || app.scoped_inline_cancel_v1
        || app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app.github_action.is_some()
        || app.refresh_requested
}
/// Nonblocking, pure installation. Every fallible predicate precedes moving
/// even one old presentation field. The actual details/body stay on App.
pub(crate) fn publish(
    app: &mut App,
    owner: &Arc<PersonalPrEntryOwnerV13>,
    prepared: PreparedPersonalPrEntryV13,
) -> bool {
    if !owner.owns(app)
        || owner.expired()
        || !prepared.matches(owner)
        || newer_intent(app)
        || app.github.as_ref().is_none_or(|g| g.pr.is_none())
    {
        return false;
    }
    let page = app.github.as_mut().unwrap().pr.as_mut().unwrap();
    let previous = PersonalPrReturnPresentationV13 {
        rows: std::mem::replace(&mut page.rows, prepared.rows),
        tab: std::mem::replace(&mut page.tab, PrTab::Files),
        diff: page.diff.take(),
        checks: page.checks.take(),
        context: page.context.take(),
        progress: page.review_progress.take(),
        selected: owner.work.original.selected_row,
        scroll: owner.work.original.scroll,
        focus: owner.work.original.focus,
        content_focus: owner.work.original.content_focus,
        old_repository: std::mem::replace(
            &mut page.details.summary.pr.repository,
            prepared.source.read().parent().repository().to_owned(),
        ),
        old_scope: owner.work.original.scope.clone(),
        old_catalogue: owner.work.original.catalogue.clone(),
    };
    let state = PersonalPrEntryStateV13 {
        source: prepared.source,
        page: owner.work.original.page.clone(),
        active: owner.work.chosen_default.clone(),
        navigation: prepared.navigation,
        selected: owner.work.original.selected.clone(),
        previous: Some(previous),
        private_retired_v14: false,
    };
    let github = app.github.as_mut().unwrap();
    github.identity = state.source.identity().clone();
    github.scoped_pr_navigation = Some(state.navigation.clone());
    github.scoped_pr_resource = Some(prepared.view);
    app.remote_scope = Some(state.active.clone());
    app.personal_pr_source_v13 = Some(state);
    app.personal_pr_entry_owner_v13 = None;
    app.personal_pr_entry_work_v13 = None;
    app.loading[View::Inbox.index()] = false;
    app.selected = 0;
    app.scroll = 0;
    app.focus = 0;
    app.github_content_focused = true;
    app.hits.clear();
    app.status = "Personal PR Files • current ID-bound read • cached Inbox retained".into();
    app.context = app.github.as_ref().unwrap().context();
    true
}
/// Exact cached return only. No SQL, API, reconstructed page, permission,
/// source resumption or fallback is granted by this display operation.
pub fn back_to_original(app: &mut App) -> bool {
    let Some(state) = app.personal_pr_source_v13.as_ref() else {
        return false;
    };
    if !state.owns(app) || state.previous.is_none() {
        return false;
    }
    let mut state = app.personal_pr_source_v13.take().unwrap();
    let Some(previous) = state.previous.take() else {
        return false;
    };
    let page = app.github.as_mut().unwrap().pr.as_mut().unwrap();
    page.rows = previous.rows;
    page.tab = previous.tab;
    page.diff = previous.diff;
    page.checks = previous.checks;
    page.context = previous.context;
    page.review_progress = previous.progress;
    page.details.summary.pr.repository = previous.old_repository;
    let github = app.github.as_mut().unwrap();
    github.scoped_pr_navigation = None;
    github.scoped_pr_resource = None;
    app.remote_scope = previous.old_scope;
    app.remote_scope_source = previous.old_catalogue;
    app.selected = previous.selected;
    app.scroll = previous.scroll;
    app.focus = previous.focus;
    app.github_content_focused = previous.content_focus;
    app.pr_file_images = None;
    app.pr_native_diff_v1 = None;
    app.pr_native_diff_layout_v1 = Default::default();
    app.overlay = None;
    app.hits.clear();
    app.status = "Original cached personal PR page • no new read or write".into();
    app.context = app.github.as_ref().unwrap().context();
    true
}
/// Same action for keyboard and mouse. A pending read never took the page.
pub fn intercept_action(app: &mut App, action: &crate::app::Action) -> bool {
    use crate::{app::Action, github_workspace::GithubAction};
    if app.personal_pr_source_v13.is_some()
        && (app.pr_native_diff_owner_v1.is_some() || app.pr_native_diff_work_v1.is_some())
        && matches!(action, Action::Cancel | Action::Github(GithubAction::Back))
    {
        crate::scoped_pr_native_diff_native_v1::close(app);
        return true;
    }
    if app.personal_pr_entry_owner_v13.is_some() || app.personal_pr_entry_work_v13.is_some() {
        if matches!(action, Action::Cancel | Action::Github(GithubAction::Back)) {
            if let Some(work) = app.personal_pr_entry_work_v13.take() {
                work.cancel.cancel();
            }
            if let Some(owner) = &app.personal_pr_entry_owner_v13 {
                owner.cancellation().cancel();
            }
            app.personal_pr_entry_cancel_v13 = true;
            app.status = "Cancelling original personal read • original page retained".into();
            return true;
        }
        if matches!(action, Action::Pr(_) | Action::Github(_)) {
            app.status =
                "Original personal Files read pending • Cancel before another remote action".into();
            return true;
        }
    }
    if matches!(action, Action::Github(GithubAction::Back)) && back_to_original(app) {
        return true;
    }
    // The genuine common-origin Threads adapter owns these closed actions.
    // No legacy Pr/Compose/review path or fabricated Catalogue is admitted.
    if matches!(
        action,
        Action::Github(
            GithubAction::ThreadHistoryV1
                | GithubAction::FileCommentV75
                | GithubAction::Resource(
                    crate::scoped_pr_observed_tabs_v1::resources::PrResourceUiActionV1::Threads
                )
        )
    ) {
        return false;
    }
    if app.personal_pr_source_v13.is_some()
        && matches!(action,Action::Pr(_)|Action::Github(GithubAction::Compose(_)|GithubAction::Actions
            |GithubAction::ThreadHistoryV1|GithubAction::ReviewOperationHistoryV3
            |GithubAction::Resource(crate::scoped_pr_observed_tabs_v1::resources::PrResourceUiActionV1::Threads))) {
        app.status="Personal review/write route is not enabled yet • no legacy fallback or mutation".into();
        return true;
    }
    if app.personal_pr_source_v13.is_some()
        && matches!(action,
        Action::Github(GithubAction::PrTab(tab)) if *tab!=PrTab::Files)
    {
        app.status =
            "Use Back for the original cached page • no personal global-tab fallback".into();
        return true;
    }
    if app.personal_pr_source_v13.is_some()
        && matches!(action, Action::Github(GithubAction::Resource(_)))
    {
        match action {
            Action::Github(GithubAction::Resource(
                crate::scoped_pr_observed_tabs_v1::resources::PrResourceUiActionV1::NativeDiff,
            )) => {
                crate::scoped_pr_native_diff_native_v1::open_files(app);
                return true;
            }
            Action::Github(GithubAction::Resource(
                crate::scoped_pr_observed_tabs_v1::resources::PrResourceUiActionV1::OldImage,
            )) => {
                crate::scoped_pr_native_diff_native_v1::open_personal_images(app, false);
                return true;
            }
            Action::Github(GithubAction::Resource(action)) => {
                crate::scoped_pr_native_diff_native_v1::prepare_personal_loaded(app, *action);
                return true;
            }
            _ => {}
        }
        return true;
    }
    false
}

#[cfg(test)]
#[path = "personal_pr_entry_v13_tests.rs"]
pub(crate) mod tests;
