//! Original scoped PR scheduling for the SHARED native renderer/controller.
//! One Inbox operation; initial verification+projection publish atomically.
use crate::{
    active_remote_scope_v1::ActiveRemoteScopeV1,
    app::{App, Overlay, View},
    native_diff_operation_v3::NativeDiffCheckV3,
    providers::{
        gh::{GithubProvider, PrImmutableFileLimitsV1, RepositoryCatalogueAccessLimits},
        runner::CancellationToken,
    },
    repository_native_diff_content_v2::NativeDiffPurposeV2,
    repository_native_diff_v1::{NativeDiffRequestV1, NativeDiffStateV1, PreparedNativeDiffV1},
    scoped_pr_native_diff_source_v1::{PrNativePreparationV1, ScopedPrNativeSourceV1},
    scoped_pr_observed_tabs_v1::{
        self,
        resources::{
            immutable_images::{
                self, ClosedPrFileImagesV1, PrImmutableImageInputV1, PrImmutableImageResultV1,
            },
            native::PrResourceCompletionV1,
        },
    },
    storage::CatalogueIntentSource,
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
pub type PrNativeStateV1 = NativeDiffStateV1<ScopedPrNativeSourceV1>;
pub type PrNativeLayoutV1 =
    crate::repository_native_diff_ui_v1::NativeDiffLayoutV1<ScopedPrNativeSourceV1>;

enum WorkKind {
    #[cfg(test)]
    ActualWorkerPanic(Arc<PrNativeWorkV1>),
    Images(Box<Mutex<Option<PrImmutableImageInputV1>>>),
    Retained(Arc<ClosedPrFileImagesV1>),
    Projection(Arc<NativeDiffRequestV1<ScopedPrNativeSourceV1>>),
}
pub struct PrNativeWorkV1 {
    kind: WorkKind,
    source: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    navigation: Arc<scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>,
    selected_pr: Option<crate::github_workspace::PrKey>,
    selected: usize,
    width: u16,
}
impl std::fmt::Debug for PrNativeWorkV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrNativeWorkV1(<original scope/navigation/presentation>)")
    }
}
pub fn width(app: &App) -> u16 {
    app.original_tree_viewport_v2
        .width
        .saturating_sub(2)
        .min(512)
}
fn owners(
    app: &App,
) -> Option<(
    Arc<CatalogueIntentSource>,
    Arc<ActiveRemoteScopeV1>,
    Arc<scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>,
)> {
    Some((
        app.remote_scope_source.clone()?,
        app.remote_scope.clone()?,
        app.github.as_ref()?.scoped_pr_navigation.clone()?,
    ))
}
fn capture(app: &mut App, kind: WorkKind) {
    let Some((source, active, navigation)) = owners(app) else {
        return;
    };
    let selected_pr = app
        .github
        .as_ref()
        .and_then(|github| github.selected.clone());
    app.pr_native_diff_work_v1 = Some(Arc::new(PrNativeWorkV1 {
        kind,
        source,
        active,
        navigation,
        selected_pr,
        selected: app.selected,
        width: width(app),
    }));
    app.pr_native_diff_layout_v1.clear();
    app.hits.clear();
}
pub fn open_files(app: &mut App) {
    if app.view != View::Inbox
        || app.overlay.is_some()
        || app.loading[View::Inbox.index()]
        || owners(app).is_none()
    {
        return;
    }
    let selected = app.selected;
    let input = app
        .github
        .as_mut()
        .ok_or(scoped_pr_observed_tabs_v1::ObservedPrTabFailureV1::SelectedPageUnavailable)
        .and_then(|github| immutable_images::take_image_input(github, selected));
    match input {
        Ok(input) => capture(app, WorkKind::Images(Box::new(Mutex::new(Some(input))))),
        Err(_) => {
            app.status =
                "Choose one loaded PR file/patch row for exact native review; no request sent"
                    .into()
        }
    }
}
pub fn open_images(app: &mut App) {
    let Some(proof) = app
        .pr_file_images
        .as_ref()
        .map(|view| view.original_proof_v1().clone())
        .filter(|proof| proof.owns(app))
    else {
        return;
    };
    if app.scoped_inline_owner_v1.is_some() || app.loading[View::Inbox.index()] {
        return;
    }
    app.overlay = None;
    capture(app, WorkKind::Retained(proof));
}
pub(crate) fn queue_projection(
    app: &mut App,
    request: Arc<NativeDiffRequestV1<ScopedPrNativeSourceV1>>,
) {
    capture(app, WorkKind::Projection(request));
}
pub(crate) fn close(app: &mut App) {
    app.pr_native_diff_cancel_v1 = true;
    if let Some(work) = app.pr_native_diff_work_v1.take() {
        work.restore_not_submitted(app);
    }
    app.pr_native_diff_v1 = None;
    app.pr_native_diff_layout_v1 = Default::default();
    app.overlay = app
        .pr_file_images
        .as_ref()
        .filter(|view| view.owns(app))
        .map(|_| Overlay::PrFileImages);
    app.hits.clear();
}
pub fn newer_intent(app: &App) -> bool {
    app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || immutable_images::inline_v1::native_v1::newer_intent(app)
}
/// The provider could not be created, so the original input has NOT been
/// consumed by a worker. Restore only its exact current page/presentation.
pub fn submission_failed(app: &mut App, work: &Arc<PrNativeWorkV1>) {
    if !work.original_pair(app) {
        return;
    }
    work.restore_not_submitted(app);
    if matches!(&work.kind, WorkKind::Retained(_)) {
        app.overlay = app
            .pr_file_images
            .as_ref()
            .filter(|view| view.owns(app))
            .map(|_| Overlay::PrFileImages);
    }
    app.status =
        "Original PR native provider unavailable; original page retained, nothing submitted".into();
    app.hits.clear();
}
impl PrNativeWorkV1 {
    fn original_pair(&self, app: &App) -> bool {
        app.view == View::Inbox
            && app
                .remote_scope_source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &self.source))
            && app
                .remote_scope
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &self.active))
            && app.github.as_ref().is_some_and(|github| {
                github.selected == self.selected_pr
                    && github.scope().ok().as_ref() == Some(self.source.scope())
                    && github
                        .scoped_pr_navigation
                        .as_ref()
                        .is_some_and(|nav| Arc::ptr_eq(nav, &self.navigation))
            })
    }
    pub(crate) fn matches(&self, app: &App) -> bool {
        if !self.original_pair(app)
            || self.selected != app.selected
            || self.width != width(app)
            || app.overlay.is_some()
        {
            return false;
        }
        match &self.kind {
            #[cfg(test)]
            WorkKind::ActualWorkerPanic(original) => original.matches(app),
            WorkKind::Images(_) => {
                app.pr_native_diff_v1.is_none()
                    && app
                        .github
                        .as_ref()
                        .is_some_and(|github| github.pr.is_none())
            }
            WorkKind::Retained(proof) => {
                proof.owns(app)
                    && app
                        .pr_file_images
                        .as_ref()
                        .is_some_and(|viewer| Arc::ptr_eq(viewer.original_proof_v1(), proof))
            }
            WorkKind::Projection(request) => app
                .pr_native_diff_v1
                .as_ref()
                .is_some_and(|state| state.owns(app) && state.accepts(request)),
        }
    }
    pub(crate) fn restore_not_submitted(&self, app: &mut App) {
        if !self.original_pair(app) {
            return;
        }
        if let WorkKind::Images(slot) = &self.kind {
            if let Some(input) = slot.lock().ok().and_then(|mut input| input.take()) {
                input.restore_failed_submission(app);
            }
        }
        if let WorkKind::Projection(request) = &self.kind {
            if let Some(state) = app.pr_native_diff_v1.as_mut() {
                state.fail(request, "Original native request was not submitted; no read/write started. Retry explicitly.");
            }
        }
    }
}
pub struct PrNativeOwnerV1 {
    work: Arc<PrNativeWorkV1>,
    cancel: CancellationToken,
    deadline: Instant,
}
impl std::fmt::Debug for PrNativeOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PrNativeOwnerV1(<original Inbox request>)")
    }
}
impl PrNativeOwnerV1 {
    pub(crate) fn new(work: Arc<PrNativeWorkV1>) -> Result<Arc<Self>, String> {
        Ok(Arc::new(Self {
            work,
            cancel: Default::default(),
            deadline: Instant::now()
                .checked_add(Duration::from_secs(30))
                .ok_or("Original PR native deadline unavailable")?,
        }))
    }
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    pub fn expired(&self) -> bool {
        self.cancel.is_cancelled() || Instant::now() >= self.deadline
    }
    pub(crate) fn owns(self: &Arc<Self>, app: &App) -> bool {
        app.pr_native_diff_owner_v1
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(owner, self))
            && self.work.matches(app)
            && !newer_intent(app)
    }
    pub(crate) fn remote_owners(
        &self,
    ) -> (
        Arc<ActiveRemoteScopeV1>,
        Arc<CatalogueIntentSource>,
        Arc<scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>,
    ) {
        (
            self.work.active.clone(),
            self.work.source.clone(),
            self.work.navigation.clone(),
        )
    }
}
enum PreparedKind {
    Opened {
        images: Option<Box<PrImmutableImageResultV1>>,
        state: Box<PrNativeStateV1>,
    },
    Projected(Box<PreparedNativeDiffV1<ScopedPrNativeSourceV1>>),
}
pub struct PreparedPrNativeV1 {
    owner: Arc<PrNativeOwnerV1>,
    kind: PreparedKind,
}
impl std::fmt::Debug for PreparedPrNativeV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedPrNativeV1(<exact closed native outcome>)")
    }
}
impl PreparedPrNativeV1 {
    pub(crate) fn matches(&self, owner: &Arc<PrNativeOwnerV1>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
            && match (&owner.work.kind, &self.kind) {
                (
                    WorkKind::Images(_),
                    PreparedKind::Opened {
                        images: Some(images),
                        state,
                    },
                ) => {
                    state.width == owner.work.width
                        && state
                            .source()
                            .is_some_and(|source| Arc::ptr_eq(source.proof(), &images.proof))
                        && Arc::ptr_eq(&images.requested_navigation, &owner.work.navigation)
                }
                (
                    WorkKind::Retained(proof),
                    PreparedKind::Opened {
                        images: None,
                        state,
                    },
                ) => {
                    state.width == owner.work.width
                        && state
                            .source()
                            .is_some_and(|source| Arc::ptr_eq(source.proof(), proof))
                }
                (WorkKind::Projection(request), PreparedKind::Projected(prepared)) => {
                    Arc::ptr_eq(request, prepared.request())
                }
                _ => false,
            }
    }
}
pub(crate) fn execute(
    owner: Arc<PrNativeOwnerV1>,
    provider: &GithubProvider,
    database: &Path,
) -> Result<PreparedPrNativeV1, String> {
    let operation = PrNativePreparationV1::new(owner.deadline, owner.cancel.clone())?;
    let kind = match &owner.work.kind {
        #[cfg(test)]
        WorkKind::ActualWorkerPanic(_) => panic!("Actual original PR native worker panic control"),
        WorkKind::Projection(request) => {
            PreparedKind::Projected(Box::new(request.prepare(&operation)?))
        }
        WorkKind::Images(slot) => {
            let input = slot
                .lock()
                .map_err(|_| "Original PR native input unavailable")?
                .take()
                .ok_or("Original image input already consumed")?;
            let bounded = provider
                .clone()
                .with_catalogue_read_deadline(owner.deadline)
                .with_scoped_read_window(owner.deadline, 1024 * 1024)
                .map_err(|_| "Original PR native read bounds unavailable")?;
            let mut access = RepositoryCatalogueAccessLimits::default();
            access.transport.max_duration =
                owner.deadline.saturating_duration_since(Instant::now());
            let images = immutable_images::read_images(
                owner.work.source.clone(),
                owner.work.active.clone(),
                input,
                &bounded,
                database,
                &PrImmutableFileLimitsV1::default(),
                &access,
                &owner.cancel,
            )
            .map_err(|_| {
                "Original PR file verification refused; private source must be refreshed"
            })?;
            let state = prepare_open(images.proof.clone(), owner.work.width, &operation)?;
            PreparedKind::Opened {
                images: Some(Box::new(images)),
                state: Box::new(state),
            }
        }
        WorkKind::Retained(proof) => PreparedKind::Opened {
            images: None,
            state: Box::new(prepare_open(proof.clone(), owner.work.width, &operation)?),
        },
    };
    operation.check()?;
    let result = PreparedPrNativeV1 { owner, kind };
    operation.seal()?;
    Ok(result)
}
fn prepare_open(
    proof: Arc<ClosedPrFileImagesV1>,
    width: u16,
    operation: &PrNativePreparationV1,
) -> Result<PrNativeStateV1, String> {
    let source = ScopedPrNativeSourceV1::from_proof(proof, operation)?;
    let mut state = PrNativeStateV1::from_captured_v3(source.intent().clone(), Some(source), width);
    let request = state.request()?;
    let prepared = request.prepare(operation)?;
    if !state.accept(prepared) {
        return Err("Original native open presentation changed before seal".into());
    }
    operation.check()?;
    Ok(state)
}
pub(crate) fn publish(app: &mut App, result: PreparedPrNativeV1) -> bool {
    match result.kind {
        PreparedKind::Opened { images, state } => {
            if let Some(images) = images {
                let Ok((viewer, _limited)) = immutable_images::install_images(app, *images) else {
                    return false;
                };
                app.pr_file_images = viewer;
            }
            if !state.owns(app) {
                return false;
            }
            app.pr_native_diff_v1 = Some(*state);
            app.pr_native_diff_layout_v1 = Default::default();
            app.overlay = None;
            app.status = ScopedPrNativeSourceV1::ready_status().into();
        }
        PreparedKind::Projected(prepared) => {
            if !app
                .pr_native_diff_v1
                .as_mut()
                .is_some_and(|state| state.accept(*prepared))
            {
                return false;
            }
            app.status = ScopedPrNativeSourceV1::ready_status().into();
        }
    }
    app.hits.clear();
    true
}
pub(crate) fn failure(
    app: &mut App,
    background: &mut crate::runtime::Background,
    owner: &Arc<PrNativeOwnerV1>,
) {
    match &owner.work.kind {
        #[cfg(test)]
        WorkKind::ActualWorkerPanic(_) => {}
        WorkKind::Projection(request) => {
            if let Some(state) = app.pr_native_diff_v1.as_mut() {
                state.fail(request, "Native PR preparation refused/cancelled; previous preview is NOT CURRENT. No mutation or fallback.");
            }
        }
        WorkKind::Retained(_) => {
            app.overlay = app
                .pr_file_images
                .as_ref()
                .filter(|view| view.owns(app))
                .map(|_| Overlay::PrFileImages);
        }
        WorkKind::Images(_) => {
            immutable_images::inline_v1::native_v1::retire_private(app, background);
        }
    }
    app.status = if matches!(&owner.work.kind, WorkKind::Images(_)) {
        "Original repository scope is fenced • native verification refused • private pages hidden • explicitly refresh original source"
    } else {
        "Original PR native result unavailable • no mutation • explicitly reopen/refresh original source"
    }.into();
    app.hits.clear();
}
/// Pending initial open has no rows. Do not let resize/Close invent a new clock
/// or allow an old-width result to publish. Back remains the original Inbox path.
pub(crate) fn pending_event(app: &mut App, event: &crossterm::event::Event) -> bool {
    if app.pr_native_diff_v1.is_some()
        || app.pr_native_diff_owner_v1.is_none() && app.pr_native_diff_work_v1.is_none()
    {
        return false;
    }
    use crossterm::event::{Event, KeyCode, KeyEventKind};
    match event {
        Event::Resize(_, _) => {
            app.pr_native_diff_cancel_v1 = true;
            app.status = "Resize retired original native opening; Back then explicitly reopen at the current width".into();
            true
        }
        Event::Key(key) if key.kind != KeyEventKind::Release && key.code == KeyCode::Esc => {
            close(app);
            true
        }
        _ => false,
    }
}
pub(crate) fn resource_is_native(result: &PrResourceCompletionV1) -> bool {
    matches!(result, PrResourceCompletionV1::Native(_))
}
#[cfg(test)]
pub(crate) fn actual_panic_work(original: Arc<PrNativeWorkV1>) -> Arc<PrNativeWorkV1> {
    Arc::new(PrNativeWorkV1 {
        source: original.source.clone(),
        active: original.active.clone(),
        navigation: original.navigation.clone(),
        selected_pr: original.selected_pr.clone(),
        selected: original.selected,
        width: original.width,
        kind: WorkKind::ActualWorkerPanic(original),
    })
}

impl crate::repository_native_diff_native_v1::NativeAppV2 for ScopedPrNativeSourceV1 {
    fn view() -> View {
        View::Inbox
    }
    fn context(app: &App) -> Option<&Self::Context> {
        Some(app)
    }
    fn preview(app: &mut App) {
        if let Some(state) = app.pr_native_diff_v1.as_mut() {
            state.status = Self::stage_unavailable().into();
        }
    }
    fn state(app: &App) -> Option<&PrNativeStateV1> {
        app.pr_native_diff_v1.as_ref()
    }
    fn parts(app: &mut App) -> (&mut Option<PrNativeStateV1>, &mut PrNativeLayoutV1) {
        (
            &mut app.pr_native_diff_v1,
            &mut app.pr_native_diff_layout_v1,
        )
    }
    fn queue_request(app: &mut App, request: Arc<NativeDiffRequestV1<Self>>) {
        queue_projection(app, request);
    }
    fn cancel(app: &mut App) {
        app.pr_native_diff_cancel_v1 = true;
        app.pr_native_diff_work_v1 = None;
        if app.overlay == Some(Overlay::Notice)
            && app
                .notice
                .starts_with("Native PR diff • ONE verified selected file\n")
        {
            app.overlay = None;
            app.notice.clear();
            app.notice_scroll = 0;
        }
    }
    fn closed(app: &mut App) {
        app.overlay = app
            .pr_file_images
            .as_ref()
            .filter(|view| view.owns(app))
            .map(|_| Overlay::PrFileImages);
    }
    fn before_open(_app: &mut App) {}
    fn show_source(app: &mut App) {
        if app
            .pr_native_diff_v1
            .as_ref()
            .is_some_and(|state| state.pending().is_some())
        {
            app.status = "Original native presentation is preparing; wait for it or Close before opening Source".into();
            return;
        }
        if let Some(source) = app
            .pr_native_diff_v1
            .as_ref()
            .and_then(|state| state.source())
        {
            let details = source.source_details();
            app.dispatch(crate::app::Action::Explain(details));
        }
    }
}
