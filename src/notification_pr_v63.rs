//! Genuine Notifications PR source installed into the existing Files/common
//! viewer. The original notification browser is MOVED for pure Back only.
use crate::{
    active_remote_scope_v1::ActiveRemoteScopeV1,
    app::{App, Overlay, View},
    github_workspace::{PrKey, PrPage, PrTab},
    notifications_ui_v63::NotificationBrowserV63,
    providers::{
        gh::{
            Coverage, GithubProvider, NotificationPrSourceV63, NotificationSelectionV63, PrDetails,
            PrResourceReadLimitsV1,
        },
        runner::CancellationToken,
    },
    scoped_pr_observed_tabs_v1::{
        resources::{self, ScopedPrResourceViewV1},
        ScopedPrNavigationV1,
    },
    storage::{Cache, CatalogueIntentSource},
};
use std::{path::Path, sync::Arc, time::Instant};

#[derive(Debug)]
pub(crate) struct NotificationPrPageTicketV63 {
    _private: (),
}
pub struct NotificationPrStateV63 {
    source: Arc<NotificationPrSourceV63>,
    active: Arc<ActiveRemoteScopeV1>,
    navigation: Arc<ScopedPrNavigationV1>,
    ticket: Arc<NotificationPrPageTicketV63>,
    selected: PrKey,
    previous: Option<NotificationBrowserV63>,
    previous_active: Option<Arc<ActiveRemoteScopeV1>>,
    previous_catalogue: Option<Arc<CatalogueIntentSource>>,
    previous_selected: Option<PrKey>,
    previous_row: usize,
    previous_scroll: usize,
    previous_focus: usize,
    previous_content_focus: bool,
    // Display allocation only, not source or account permission. A newer
    // projection may reuse the SAME public cache and must not be cleared.
    avatar_projection: Option<std::sync::Weak<()>>,
}
impl NotificationPrStateV63 {
    pub(crate) fn source(&self) -> &Arc<NotificationPrSourceV63> {
        &self.source
    }
    pub(crate) fn active(&self) -> &Arc<ActiveRemoteScopeV1> {
        &self.active
    }
    pub(crate) fn navigation(&self) -> &Arc<ScopedPrNavigationV1> {
        &self.navigation
    }
    pub(crate) fn ticket(&self) -> &Arc<NotificationPrPageTicketV63> {
        &self.ticket
    }
    pub(crate) fn install_image_navigation(&mut self, navigation: Arc<ScopedPrNavigationV1>) {
        self.navigation = navigation;
    }
    pub(crate) fn same_page(&self, app: &App) -> bool {
        app.remote_scope
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, &self.active))
            && app.github.as_ref().is_some_and(|github| {
                github.selected.as_ref() == Some(&self.selected)
                    && github
                        .notification_pr_page_ticket_v63
                        .as_ref()
                        .is_some_and(|ticket| Arc::ptr_eq(ticket, &self.ticket))
            })
    }
    pub(crate) fn owns(&self, app: &App) -> bool {
        self.same_page(app)
            && app.github.as_ref().is_some_and(|github| {
                github.scope().ok().as_ref() == Some(self.source.scope())
                    && github
                        .scoped_pr_navigation
                        .as_ref()
                        .is_some_and(|nav| Arc::ptr_eq(nav, &self.navigation))
                    && github.pr.as_ref().is_some_and(|page| {
                        crate::scoped_pr_observed_tabs_v1::matches_page(&self.navigation, page)
                    })
            })
    }
}
pub(crate) struct PreparedNotificationPrV63 {
    source: Arc<NotificationPrSourceV63>,
    active: Arc<ActiveRemoteScopeV1>,
    navigation: Arc<ScopedPrNavigationV1>,
    ticket: Arc<NotificationPrPageTicketV63>,
    selected: PrKey,
    page: PrPage,
    view: Arc<ScopedPrResourceViewV1>,
}
impl PreparedNotificationPrV63 {
    pub(crate) fn belongs_to(
        &self,
        original: &Arc<crate::providers::gh::ClosedNotificationReadV63>,
    ) -> bool {
        self.source.belongs_to_notifications_v63(original)
    }
}
/// Worker-only actual preparation BEFORE the SAME final rawPR/thread/repo/SQL
/// and numeric UID. No foreground full-workspace clone or fake Files member.
pub(crate) fn read(
    provider: &GithubProvider,
    selected: &NotificationSelectionV63,
    cache: &Cache,
    database: &Path,
    token: &CancellationToken,
    deadline: Instant,
) -> Result<PreparedNotificationPrV63, crate::providers::gh::GithubError> {
    let intent = selected.pr_follow_intent_v63()?;
    let pending = provider.begin_notification_pr_follow_v63(
        intent,
        cache,
        database,
        &PrResourceReadLimitsV1::default(),
        token,
        deadline,
    )?;
    let actual = pending.read();
    let (view, rows) = resources::prepare_resource_arc_v13(actual.clone(), token)
        .map_err(|_| resources::safe_local())?;
    let (view, rows) = view.zip(rows).ok_or_else(resources::safe_local)?;
    if actual.raw_patch().is_some()
        && !actual.files().is_empty()
        && view
            .canonical_diff()
            .is_none_or(|diff| diff.files.is_empty())
    {
        return Err(resources::safe_local());
    }
    let selected = PrKey::from(&actual.parent().summary().pr);
    let scope =
        crate::storage::Scope::github(&pending.identity().host, pending.identity().account_id)
            .map_err(|_| resources::safe_local())?;
    let navigation = Arc::new(
        ScopedPrNavigationV1::bind(
            &scope,
            pending.reference(),
            &selected,
            actual.parent().summary(),
        )
        .map_err(|_| resources::safe_local())?,
    );
    // A bounded actual raw-parent projection only. Empty collections below
    // are explicitly UNLOADED, never a claim that conversation/history is empty.
    let page = PrPage {
        details: PrDetails {
            summary: actual.parent().summary().clone(), comments:vec![],commits:vec![],files:vec![],reviews:vec![],timeline:vec![],
            coverage: Coverage { loaded:actual.files().len(), total_matches:actual.parent().changed_files().unwrap_or(0), complete:false,
                reasons:vec!["Notification follow observed current raw parent and bounded Files/patch; conversation/timeline/commits are not loaded; patch availability is labelled separately".into()] },
        },
        tab:PrTab::Files,diff:None,checks:None,rows,context:None,metadata_display_v51:None,review_progress:None,
    };
    let view = Arc::new(view);
    let ticket = Arc::new(NotificationPrPageTicketV63 { _private: () });
    if token.is_cancelled() || Instant::now() >= pending.deadline() {
        return Err(resources::safe_local());
    }
    let source = pending.finish(cache, token)?;
    // O(1) actual allocation moves only after final UID; no source SQL or
    // formatting/reconstruction can follow this terminal read closure.
    let active = Arc::new(ActiveRemoteScopeV1::notification_pr_v63(source.clone()));
    Ok(PreparedNotificationPrV63 {
        source,
        active,
        navigation,
        ticket,
        selected,
        page,
        view,
    })
}
pub(crate) fn install(app: &mut App, prepared: PreparedNotificationPrV63) -> bool {
    if app.notification_pr_source_v63.is_some()
        || app.personal_pr_source_v13.is_some()
        || app.overlay != Some(Overlay::NotificationsV63)
        || app.form.is_some()
        || crate::notifications_native_v63::newer_intent(app)
        || app.notifications_v63.as_ref().is_none_or(|view| {
            !view.original.owns(app)
                || view.prepared.as_ref().is_none_or(|original| {
                    !prepared
                        .source
                        .belongs_to_notifications_v63(&original.source)
                })
        })
        || app.github.as_ref().is_none_or(|github| {
            github.pr.is_some() || github.scope().ok().as_ref() != Some(prepared.source.scope())
        })
    {
        return false;
    }
    // Every refusal precedes the first move. The exact Work/result owner and
    // generation have already been checked by the existing publisher.
    let mut previous = app.notifications_v63.take();
    if let Some(view) = &mut previous {
        if let Some(original) = view.follow_return_v63.take() {
            view.detail = original.detail;
            view.scroll = original.scroll;
            view.viewport = original.viewport;
            view.message = original.message;
            view.focus = original.focus;
        }
        view.hits.clear();
    }
    let previous_active = app.remote_scope.take();
    let previous_catalogue = app.remote_scope_source.take();
    let github = app.github.as_mut().expect("checked actual current Inbox");
    let previous_selected = github.selected.replace(prepared.selected.clone());
    github.identity = prepared.source.identity().clone();
    github.pr = Some(prepared.page);
    github.notification_pr_page_ticket_v63 = Some(prepared.ticket.clone());
    github.scoped_pr_navigation = Some(prepared.navigation.clone());
    github.scoped_pr_resource = Some(prepared.view);
    app.notification_pr_source_v63 = Some(NotificationPrStateV63 {
        source: prepared.source,
        active: prepared.active.clone(),
        navigation: prepared.navigation,
        ticket: prepared.ticket,
        selected: prepared.selected,
        previous,
        previous_active,
        previous_catalogue,
        previous_selected,
        previous_row: app.selected,
        previous_scroll: app.scroll,
        previous_focus: app.focus,
        previous_content_focus: app.github_content_focused,
        avatar_projection: None,
    });
    app.remote_scope = Some(prepared.active);
    app.overlay = None;
    app.selected = 0;
    app.scroll = 0;
    app.focus = 0;
    app.github_content_focused = true;
    app.hits.clear();
    app.context = github.context();
    app.status="Notification PR Files • genuine current source • b Back returns original notifications without I/O".into();
    true
}
pub fn back(app: &mut App) -> bool {
    let Some(state) = app.notification_pr_source_v63.as_ref() else {
        return false;
    };
    if !state.owns(app)
        || state.previous.is_none()
        || app.overlay.is_some()
        || app.form.is_some()
        || app.pr_native_diff_v1.is_some()
        || app.pr_file_images.is_some()
        || app.loading.iter().any(|busy| *busy)
        || crate::notifications_native_v63::newer_intent(app)
    {
        return false;
    }
    let mut state = app
        .notification_pr_source_v63
        .take()
        .expect("exact current source");
    let github = app.github.as_mut().expect("checked actual page");
    github.pr = None;
    github.notification_pr_page_ticket_v63 = None;
    github.scoped_pr_navigation = None;
    github.scoped_pr_resource = None;
    github.selected = state.previous_selected;
    app.remote_scope = state.previous_active;
    app.remote_scope_source = state.previous_catalogue;
    app.notifications_v63 = state.previous.take();
    app.overlay = Some(Overlay::NotificationsV63);
    app.selected = state.previous_row;
    app.scroll = state.previous_scroll;
    app.focus = state.previous_focus;
    app.github_content_focused = state.previous_content_focus;
    app.hits.clear();
    app.status =
        "Original observed notifications restored • no reads, writes or renewed permission".into();
    app.context = github.context();
    true
}
/// Both input modes route the SAME existing read controls. A source kind is
/// never silently sent through an unsupported Catalogue/Personal legacy path.
pub fn intercept_action(app: &mut App, action: &crate::app::Action) -> bool {
    use crate::{app::Action, github_workspace::GithubAction};
    if app.notification_pr_source_v63.is_none() {
        return false;
    }
    retire_invalid(app);
    if app.notification_pr_source_v63.is_none() {
        return true;
    }
    if (app.pr_native_diff_owner_v1.is_some() || app.pr_native_diff_work_v1.is_some())
        && matches!(action, Action::Cancel | Action::Github(GithubAction::Back))
    {
        crate::scoped_pr_native_diff_native_v1::close(app);
        return true;
    }
    if *action == Action::Github(GithubAction::Back) && back(app) {
        return true;
    }
    if let Action::Github(GithubAction::Resource(resource)) = action {
        match resource {
            resources::PrResourceUiActionV1::NativeDiff => crate::scoped_pr_native_diff_native_v1::open_files(app),
            resources::PrResourceUiActionV1::OldImage => crate::scoped_pr_native_diff_native_v1::open_personal_images(app,false),
            resources::PrResourceUiActionV1::Threads => app.status="Notification source thread adapter awaits reviewed common-origin composition • no legacy fallback".into(),
            resource => crate::scoped_pr_native_diff_native_v1::prepare_personal_loaded(app,*resource),
        }
        return true;
    }
    if matches!(
        action,
        Action::Github(
            GithubAction::PrTab(_)
                | GithubAction::Compose(_)
                | GithubAction::Actions
                | GithubAction::ThreadHistoryV1
                | GithubAction::ReviewOperationHistoryV3
                | GithubAction::ReviewThreadsV4
        ) | Action::Pr(_)
    ) {
        app.status="This notification source currently exposes genuine Files/native reads only • no legacy query or write fallback".into();
        return true;
    }
    false
}
pub(crate) fn retire_source(
    app: &mut App,
    source: &crate::pr_read_origin_v13::PrReadOriginV13,
    navigation: &Arc<ScopedPrNavigationV1>,
) -> bool {
    if !source.is_notification_v63()
        || !source.matches_source(app)
        || app.notification_pr_source_v63.as_ref().is_none_or(|state| {
            !state.same_page(app) || !Arc::ptr_eq(state.navigation(), navigation)
        })
        || app
            .github
            .as_ref()
            .and_then(|g| g.scoped_pr_navigation.as_ref())
            .is_none_or(|nav| !Arc::ptr_eq(nav, navigation))
    {
        return false;
    }
    retire_exact_private(app);
    true
}
pub(crate) fn retire_account_mismatch(
    app: &mut App,
    source: &crate::pr_read_origin_v13::PrReadOriginV13,
    active: &Arc<ActiveRemoteScopeV1>,
    navigation: &Arc<ScopedPrNavigationV1>,
) -> bool {
    let lost = app
        .notification_pr_source_v63
        .as_ref()
        .is_some_and(|state| {
            state.same_page(app)
                && source.matches_source(app)
                && Arc::ptr_eq(state.active(), active)
                && Arc::ptr_eq(state.navigation(), navigation)
                && app.github.as_ref().is_some_and(|github| {
                    github
                        .scoped_pr_navigation
                        .as_ref()
                        .is_some_and(|nav| Arc::ptr_eq(nav, navigation))
                        && github.scope().ok().as_ref() != Some(state.source.scope())
                })
        });
    if !lost {
        return false;
    }
    retire_exact_private(app);
    true
}
fn retire_exact_private(app: &mut App) {
    // The page ticket does not own global resource/avatar stores. Inspect the
    // actual retained child proof before dropping the original page.
    let owns_proof = |proof: &resources::immutable_images::ClosedPrFileImagesV1| {
        app.notification_pr_source_v63
            .as_ref()
            .is_some_and(|state| {
                proof.source.matches_source(app)
                    && Arc::ptr_eq(&proof.active, state.active())
                    && Arc::ptr_eq(&proof.navigation, state.navigation())
            })
    };
    let retire_images = app
        .pr_file_images
        .as_ref()
        .is_some_and(|view| owns_proof(view.original_proof_v1()));
    let retire_native = app.pr_native_diff_v1.as_ref().is_some_and(|view| {
        view.source()
            .is_some_and(|source| owns_proof(source.proof()))
    });
    let retire_avatars = app
        .notification_pr_source_v63
        .as_ref()
        .is_some_and(|state| {
            state
                .avatar_projection
                .as_ref()
                .is_some_and(|projection| app.avatar_states.matches_projection_v7(projection))
        });
    // Context belongs to the currently focused view, while hit geometry may
    // belong to an independently opened topmost modal. Keep these separate.
    let independent_view_context = match app.view {
        View::Repository => app.workspace.is_some(),
        View::Actions => app.actions.is_some(),
        View::Search => app.search.is_some(),
        View::Dashboard => app.metrics.is_some(),
        View::Inbox => false,
    };
    let original_display_hits = app.view == View::Inbox
        && ((app.overlay.is_none() && app.form.is_none())
            || (app.overlay == Some(Overlay::PrFileImages) && retire_images));
    if app
        .notification_pr_source_v63
        .as_ref()
        .is_some_and(|state| {
            app.remote_scope
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, state.active()))
        })
    {
        if let Some(active) = &mut app.remote_scope {
            Arc::make_mut(active)
                .fence(crate::active_remote_scope_v1::RemoteScopeFenceV1::PrivacyFence);
        }
    }
    // This read-only Files/native source has NEVER issued a Form. Numeric
    // scope equality is not a presentation ticket and owns no foreign Form.
    app.notification_pr_source_v63 = None;
    if retire_images {
        app.pr_file_images = None;
    }
    if retire_native {
        app.pr_native_diff_v1 = None;
        app.pr_native_diff_layout_v1 = Default::default();
    }
    app.github = None;
    app.content[View::Inbox.index()].clear();
    if !independent_view_context {
        app.context.clear();
    }
    if retire_avatars {
        app.avatar_states.clear();
    }
    if original_display_hits {
        app.hits.clear();
        if app.overlay == Some(Overlay::PrFileImages) && retire_images {
            app.overlay = None;
        }
    }
    // NO physical slot/token/loading/status/ACK changes: a replacement read
    // must retain its exact owner and may still finish independently.
}
/// Called only after the existing shared cache installed a real successful
/// projection. No actor copies, IO, cache cancellation or source issuance.
pub(crate) fn record_avatar_projection(app: &mut App) {
    if app.view != View::Inbox || app.overlay.is_some() {
        return;
    }
    if app
        .notification_pr_source_v63
        .as_ref()
        .is_some_and(|state| state.owns(app))
    {
        let projection = app.avatar_states.projection_v7();
        if let Some(state) = app.notification_pr_source_v63.as_mut() {
            state.avatar_projection = projection;
        }
    }
}
pub fn retire_invalid(app: &mut App) {
    let Some(state) = app.notification_pr_source_v63.as_ref() else {
        return;
    };
    let same = state.same_page(app);
    let same_nav = app
        .github
        .as_ref()
        .and_then(|g| g.scoped_pr_navigation.as_ref())
        .is_some_and(|nav| Arc::ptr_eq(nav, state.navigation()));
    let lost = same
        && same_nav
        && app
            .github
            .as_ref()
            .is_some_and(|g| g.scope().ok().as_ref() != Some(state.source.scope()));
    if lost {
        retire_exact_private(app);
    } else if !state.owns(app) {
        // An independently newer source/page/navigation is never cleared.
        app.notification_pr_source_v63 = None;
    }
}
