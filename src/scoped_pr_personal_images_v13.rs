//! The existing immutable producer, shared common session, shared image proof
//! and viewer; only the retained Personal App-page input differs. No page/body
//! is moved/cloned and no Catalogue source is synthesized.
use super::*;
use crate::{
    app::App, personal_pr_entry_v13::PersonalPrPageTicketV13, pr_read_origin_v13::PrReadOriginV13,
    scoped_pr_observed_tabs_v1::resources::native as loaded_native,
};
use std::time::{Duration, Instant};

pub(crate) struct PersonalImageInputV13 {
    source: PrReadOriginV13,
    active: Arc<ActiveRemoteScopeV1>,
    navigation: Arc<ScopedPrNavigationV1>,
    page: CommonImagePageV63,
    view: Arc<ScopedPrResourceViewV1>,
    selected: PrKey,
    file: Option<usize>,
    cancel: CancellationToken,
    deadline: Instant,
    native_width: Option<u16>,
}
enum CommonImagePageV63 {
    Personal(Arc<PersonalPrPageTicketV13>),
    Notifications(Arc<crate::notification_pr_v63::NotificationPrPageTicketV63>),
}
impl PersonalImageInputV13 {
    pub(crate) fn capture(
        app: &App,
        row: usize,
        native_width: Option<u16>,
    ) -> Result<Arc<Self>, String> {
        let mut result = Self::capture_view(app, native_width)?;
        if result.source.is_notification_v63() && result.view.observed.raw_patch().is_none() {
            return Err("Observed notification Files patch is unavailable; refresh explicitly before native verification".into());
        }
        result.file = Some(
            selected_file_index_v13(&result.view, row)
                .map_err(|_| "Choose an actual canonical file member")?,
        );
        result.check()?;
        Ok(Arc::new(result))
    }
    fn capture_view(app: &App, native_width: Option<u16>) -> Result<Self, String> {
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(30))
            .ok_or("Original Personal image deadline unavailable")?;
        let cancel = CancellationToken::default();
        let source = PrReadOriginV13::capture(app)?;
        let (active, navigation, observed, ticket) = if source.is_personal() {
            let state = app
                .personal_pr_source_v13
                .as_ref()
                .filter(|s| s.owns(app))
                .ok_or("Genuine original Personal Files source unavailable")?;
            let ticket = app
                .github
                .as_ref()
                .and_then(|g| g.personal_pr_page_ticket_v13.clone())
                .ok_or("Actual Personal page ticket unavailable")?;
            (
                state.active().clone(),
                state.navigation().clone(),
                state.source().read(),
                CommonImagePageV63::Personal(ticket),
            )
        } else if source.is_notification_v63() {
            let state = app
                .notification_pr_source_v63
                .as_ref()
                .filter(|s| s.owns(app))
                .ok_or("Genuine original Notifications PR Files source unavailable")?;
            (
                state.active().clone(),
                state.navigation().clone(),
                state.source().read(),
                CommonImagePageV63::Notifications(state.ticket().clone()),
            )
        } else {
            return Err("This same viewer path requires an actual Personal or Notifications PR source; no fallback".into());
        };
        let github = app.github.as_ref().ok_or("Original PR unavailable")?;
        let page = github
            .pr
            .as_ref()
            .filter(|p| p.tab == PrTab::Files)
            .ok_or("Open actual verified Files first")?;
        let view = github
            .scoped_pr_resource
            .as_ref()
            .filter(|v| {
                v.observed.kind() == PrResourceKindV1::Files && Arc::ptr_eq(&v.observed, observed)
            })
            .ok_or("Actual original Files allocation changed")?;
        if page.rows.len() != view.targets.len()
            || app.overlay.is_some()
            || app.form.is_some()
            || app.loading[crate::app::View::Inbox.index()]
        {
            return Err("Wait/cancel pending work and choose a current actual file row".into());
        }
        let result = Self {
            source,
            active,
            navigation,
            page: ticket,
            view: view.clone(),
            selected: github
                .selected
                .clone()
                .ok_or("Original selection unavailable")?,
            file: None,
            cancel,
            deadline,
            native_width,
        };
        result.check()?;
        Ok(result)
    }
    pub(crate) fn check(&self) -> Result<(), String> {
        if self.cancel.is_cancelled() || Instant::now() >= self.deadline {
            Err("Original Personal image operation cancelled/expired".into())
        } else {
            Ok(())
        }
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn matches(&self, app: &App) -> bool {
        self.source.matches_source(app)
            && match &self.page {
                CommonImagePageV63::Personal(ticket) => {
                    app.personal_pr_source_v13.as_ref().is_some_and(|s| {
                        s.owns(app)
                            && Arc::ptr_eq(s.active(), &self.active)
                            && Arc::ptr_eq(s.navigation(), &self.navigation)
                            && app
                                .github
                                .as_ref()
                                .and_then(|g| g.personal_pr_page_ticket_v13.as_ref())
                                .is_some_and(|p| Arc::ptr_eq(p, ticket))
                    })
                }
                CommonImagePageV63::Notifications(ticket) => {
                    app.notification_pr_source_v63.as_ref().is_some_and(|s| {
                        s.owns(app)
                            && Arc::ptr_eq(s.active(), &self.active)
                            && Arc::ptr_eq(s.navigation(), &self.navigation)
                            && Arc::ptr_eq(s.ticket(), ticket)
                    })
                }
            }
            && app.github.as_ref().is_some_and(|g| {
                g.selected.as_ref() == Some(&self.selected)
                    && g.scoped_pr_resource
                        .as_ref()
                        .is_some_and(|v| Arc::ptr_eq(v, &self.view))
            })
    }
}
pub(crate) struct PersonalImageResultV13 {
    pub(crate) input: Arc<PersonalImageInputV13>,
    pub(crate) proof: Arc<ClosedPrFileImagesV1>,
    viewer: Option<ui::PrImageViewerV1>,
    navigation: Arc<ScopedPrNavigationV1>,
    identity: crate::providers::gh::HostIdentity,
    canonical: String,
    pub(crate) native: Option<Box<crate::scoped_pr_native_diff_native_v1::PrNativeStateV1>>,
    pub(crate) accounting: crate::providers::runner::ScopedReadAccountingV1,
}
pub(crate) fn read(
    input: Arc<PersonalImageInputV13>,
    provider: &GithubProvider,
    database: &Path,
    limits: &PrImmutableFileLimitsV1,
    access: &RepositoryCatalogueAccessLimits,
    cancel: &CancellationToken,
) -> Result<PersonalImageResultV13, PrResourceFailureV1> {
    if !input.cancellation().same_instance(cancel) {
        return Err(refused(GithubErrorKind::InvalidTarget));
    }
    input
        .check()
        .map_err(|_| refused(GithubErrorKind::Transport))?;
    limits.validate().map_err(|e| refused(e.kind))?;
    let original = PrImmutableFileOriginalV1::from_observed(
        input.source.scope(),
        input.view.observed.clone(),
        input
            .file
            .ok_or_else(|| refused(GithubErrorKind::InvalidTarget))?,
        limits,
    )
    .map_err(|e| refused(e.kind))?;
    let nav = &input.navigation;
    if !input
        .source
        .admits_navigation(&input.active, input.source.identity(), nav, &input.selected)
        || original.original_parent().node_id() != nav.node
        || original.original_parent().repository_id() != nav.original().id
        || original.original_parent().head_sha() != nav.head
        || original.original_parent().base_tip_sha() != nav.base
    {
        return Err(refused(GithubErrorKind::InvalidTarget));
    }
    let provider = provider
        .clone()
        .with_original_pr_read_bounds_v13(input.deadline, 1024 * 1024)
        .map_err(|e| refused(e.kind))?;
    let mut session = input
        .source
        .begin_access(&provider, access, cancel)
        .map_err(|e| refused(e.kind))?;
    let identity = session.identity().clone();
    let window = session
        .reserve_measured_external_reads(
            original.max_external_requests(),
            1024 * 1024,
            limits.max_pool_bytes,
            2,
            cancel,
        )
        .map_err(|e| refused(e.kind))?;
    let bounded = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(|e| refused(e.kind))?;
    let current = session
        .verify(nav.original(), cancel)
        .map_err(|e| refused(e.kind))?;
    let mut local = None;
    let observed = bounded
        .pr_immutable_file_images_v1(current, &identity, &original, limits, cancel, |parent| {
            local = Some(ResourceLocalGuard::capture_boundary_v13(
                database,
                input.source.local_boundary(),
                &identity,
                parent.summary(),
            )?);
            Ok(())
        })
        .map_err(|e| refused(e.kind))?;
    // All bounded byte/line/view preparation stays on the SAME original clock
    // before final repository -> original SQL -> final principal closure.
    let canonical = observed.parent().repository().to_owned();
    let navigation = Arc::new(
        ScopedPrNavigationV1::bind(
            input.source.scope(),
            nav.original(),
            &PrKey {
                host: input.source.scope().host.clone(),
                repository: canonical.clone(),
                number: observed.parent().number(),
            },
            observed.parent().summary(),
        )
        .map_err(|_| refused(GithubErrorKind::InvalidTarget))?,
    );
    let old_lines = line_offsets(observed.image(PrImageSideV1::Old).bytes())
        .map_err(|_| refused(GithubErrorKind::Unsupported))?;
    let new_lines = line_offsets(observed.image(PrImageSideV1::New).bytes())
        .map_err(|_| refused(GithubErrorKind::Unsupported))?;
    let proof = Arc::new(ClosedPrFileImagesV1 {
        read: Arc::new(observed),
        original_view: input.view.clone(),
        source: input.source.clone(),
        active: input.active.clone(),
        navigation: navigation.clone(),
        old_lines,
        new_lines,
    });
    let viewer = ui::PrImageViewerV1::prepare(proof.clone()).ok();
    let deadline = session.deadline().min(input.deadline);
    let native = if let Some(width) = input.native_width {
        // SAME original token, only narrowed absolute clock; a CPU check is
        // neither a second transport pool nor a renewed read permission.
        let operation = crate::scoped_pr_native_diff_source_v1::PrNativePreparationV1::new(
            deadline,
            cancel.clone(),
        )
        .map_err(|_| refused(GithubErrorKind::Transport))?;
        let state =
            crate::scoped_pr_native_diff_native_v1::prepare_open(proof.clone(), width, &operation)
                .map_err(|_| refused(GithubErrorKind::Unsupported))?;
        operation
            .seal()
            .map_err(|_| refused(GithubErrorKind::Transport))?;
        Some(Box::new(state))
    } else {
        None
    };
    input
        .check()
        .map_err(|_| refused(GithubErrorKind::Transport))?;
    let closing = session
        .verify(nav.original(), cancel)
        .map_err(|e| refused(e.kind))?;
    if closing.repository().full_name != canonical
        || closing.repository().permissions.pull != Some(true)
    {
        return Err(refused(GithubErrorKind::IdentityChanged));
    }
    let closed = session
        .close_with_guard(cancel, || {
            local
                .as_ref()
                .ok_or_else(safe_local)?
                .validate_boundary_v13(database, input.source.local_boundary())
        })
        .map_err(|e| refused(e.kind))?;
    if closed.verified().len() != 1
        || closed.verified()[0].repository().id != proof.observed().parent().repository_id()
        || closed.verified()[0].repository().node_id.as_deref()
            != Some(proof.observed().parent().repository_node())
    {
        return Err(refused(GithubErrorKind::IdentityChanged));
    }
    let accounting = *closed
        .external_read_accounting()
        .ok_or_else(|| refused(GithubErrorKind::Transport))?;
    let result = PersonalImageResultV13 {
        input,
        proof,
        viewer,
        navigation,
        identity: closed.identity().clone(),
        canonical,
        native,
        accounting,
    };
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(refused(GithubErrorKind::Transport));
    }
    Ok(result)
}
pub(crate) fn install(
    app: &mut App,
    result: PersonalImageResultV13,
) -> Result<Option<ui::PrImageViewerV1>, ObservedPrTabFailureV1> {
    if !result.input.matches(app) || app.github.as_ref().is_none_or(|g| g.pr.is_none()) {
        return Err(ObservedPrTabFailureV1::SelectedPageUnavailable);
    }
    // No reconstructed page. Actual body/details/rows remain the SAME App
    // allocations; only bounded identity/canonical-name and actual Nav move.
    match &result.input.page {
        CommonImagePageV63::Personal(_) => app
            .personal_pr_source_v13
            .as_mut()
            .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?
            .install_image_navigation(result.navigation.clone()),
        CommonImagePageV63::Notifications(_) => app
            .notification_pr_source_v63
            .as_mut()
            .ok_or(ObservedPrTabFailureV1::SelectedPageUnavailable)?
            .install_image_navigation(result.navigation.clone()),
    }
    let github = app.github.as_mut().unwrap();
    github.identity = result.identity;
    github.scoped_pr_navigation = Some(result.navigation);
    github.pr.as_mut().unwrap().details.summary.pr.repository = result.canonical;
    Ok(result.viewer)
}

pub(crate) struct PersonalLoadedInputV13 {
    original: Arc<PersonalImageInputV13>,
    action: PrResourceUiActionV1,
    selected: usize,
    anchor: Option<crate::repository_view::DiffAnchor>,
}
impl PersonalLoadedInputV13 {
    pub(crate) fn capture(app: &App, action: PrResourceUiActionV1) -> Result<Arc<Self>, String> {
        if !matches!(
            action,
            PrResourceUiActionV1::NextFile
                | PrResourceUiActionV1::NextHunk
                | PrResourceUiActionV1::Layout
                | PrResourceUiActionV1::PatchContext
        ) {
            return Err("Choose an actual loaded patch control".into());
        }
        let original = Arc::new(PersonalImageInputV13::capture_view(app, None)?);
        let result = Arc::new(Self {
            original,
            action,
            selected: app.selected,
            anchor: app
                .github
                .as_ref()
                .and_then(|g| g.pr.as_ref())
                .and_then(|p| p.rows.get(app.selected))
                .and_then(|r| r.anchor.clone()),
        });
        result.original.check()?;
        Ok(result)
    }
    pub(crate) fn matches(&self, app: &App) -> bool {
        self.original.matches(app) && self.selected == app.selected
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        self.original.cancellation()
    }
    pub(crate) fn deadline(&self) -> Instant {
        self.original.deadline()
    }
    pub(crate) fn check(&self) -> Result<(), String> {
        self.original.check()
    }
}
pub(crate) struct PersonalLoadedResultV13 {
    input: Arc<PersonalLoadedInputV13>,
    prepared: loaded_native::PreparedLoadedViewV13,
}
impl PersonalLoadedResultV13 {
    pub(crate) fn matches(&self, input: &Arc<PersonalLoadedInputV13>) -> bool {
        Arc::ptr_eq(&self.input, input)
    }
}
pub(crate) fn prepare_loaded(
    input: Arc<PersonalLoadedInputV13>,
    cancel: &CancellationToken,
) -> Result<PersonalLoadedResultV13, String> {
    if !input.cancellation().same_instance(cancel) {
        return Err("Original loaded preparation token changed".into());
    }
    input.check()?;
    let prepared = loaded_native::prepare_loaded_view_v13(
        &input.original.view,
        input.action,
        input.selected,
        input.anchor.as_ref(),
        cancel,
    )
    .map_err(|_| "Original bounded loaded preparation refused")?;
    let result = PersonalLoadedResultV13 { input, prepared };
    result.input.check()?;
    Ok(result)
}
pub(crate) fn install_loaded(app: &mut App, result: PersonalLoadedResultV13) -> bool {
    if !result.input.matches(app) {
        return false;
    }
    // Admission has already checked SAME original source/page/view/selection.
    // Only rows/targets are replaced; actual details/body stay allocated on App.
    if let Some((view, rows)) = result.prepared.rebuilt {
        let github = app.github.as_mut().expect("same original page");
        github.pr.as_mut().expect("same original page").rows = rows;
        github.scoped_pr_resource = Some(view);
        app.pr_file_images = None;
    }
    if let Some(selected) = result.prepared.selected {
        app.selected = selected;
        app.scroll = selected.saturating_sub(app.github_visible_rows.saturating_sub(1));
    }
    if result.prepared.content_focus {
        app.github_content_focused = true;
    }
    app.status = if result.prepared.display_limited {
        "Loaded display bound • original rows retained • no new reads"
    } else {
        "Loaded patch navigation • no new API/SQL/Git reads"
    }
    .into();
    if let Some(notice) = result.prepared.notice {
        app.notice = notice;
        app.notice_scroll = 0;
        app.overlay = Some(crate::app::Overlay::Notice);
    }
    app.hits.clear();
    true
}
