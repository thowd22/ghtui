//! Closed browser/draft/preview/confirm operations in the SAME Inbox slot.
use super::*;
use comments_v2::publication::{
    self as comment_publication_v2, ui as comment_composer_v2, CommentDraftV2, CommentPreviewV2,
    CommentPublicationV2, ThreadCommentIntentV2,
};
use publication_v1::{
    ui_v1 as composer, ScopedThreadDraftV1, ScopedThreadPreviewV1, ScopedThreadPublicationIntentV1,
    ScopedThreadPublicationV1, ThreadPublicationActionV1, ThreadPublicationStateV1,
};
use std::time::Duration;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedThreadPhaseV1 {
    Browse,
    Open,
    Preview,
    Confirm,
    HistoryList,
    HistoryOpen,
}
enum WorkV1 {
    Browse,
    CommentsV2(Arc<comments_v2::ThreadCommentsIntentV2>),
    CommentOpenV2(Arc<ThreadCommentIntentV2>),
    CommentPreviewV2(Arc<comment_composer_v2::CommentComposerRequestV2>),
    CommentConfirmV2(Arc<comment_composer_v2::CommentComposerRequestV2>),
    Open(Arc<ScopedThreadPublicationIntentV1>),
    Preview(Arc<composer::ThreadComposerRequestV1>),
    Confirm(Arc<composer::ThreadComposerRequestV1>),
    HistoryList,
    HistoryOpen(Arc<history_v1::PreparedThreadHistoryV1>, usize),
    CommentHistoryListV2,
    CommentHistoryOpenV2(
        Arc<history_v1::comments_v2::PreparedCommentHistoryV2>,
        usize,
    ),
}
/// Closed navigation origins, not interchangeable access capabilities. A
/// Summary history operation never manufactures a Files/browser intent.
pub enum ThreadOperationOriginV1 {
    Files(Arc<ThreadBrowseIntentV1>),
    Summary(Arc<history_v1::ThreadHistoryIntentV1>),
}
impl ThreadOperationOriginV1 {
    pub fn scope(&self) -> &Scope {
        match self {
            Self::Files(intent) => intent.scope(),
            Self::Summary(intent) => intent.scope(),
        }
    }
    pub(crate) fn source(&self) -> &Arc<CatalogueIntentSource> {
        match self {
            Self::Files(intent) => &intent.source,
            Self::Summary(intent) => &intent.source,
        }
    }
    pub(crate) fn active(&self) -> &Arc<ActiveRemoteScopeV1> {
        match self {
            Self::Files(intent) => &intent.active,
            Self::Summary(intent) => &intent.active,
        }
    }
    pub(crate) fn navigation(&self) -> &Arc<ScopedPrNavigationV1> {
        match self {
            Self::Files(intent) => &intent.navigation,
            Self::Summary(intent) => &intent.navigation,
        }
    }
    fn owns(&self, app: &App) -> bool {
        match self {
            Self::Files(intent) => intent.owns(app),
            Self::Summary(intent) => intent.owns(app),
        }
    }
}
pub struct ScopedThreadsOwnerV1 {
    pub intent: ThreadOperationOriginV1,
    work: WorkV1,
    cancel: CancellationToken,
    deadline: Instant,
    publication: std::sync::Mutex<Option<crate::drafts::DraftPublicationFenceV2>>,
    #[cfg(test)]
    actual_worker_panic: bool,
}
impl std::fmt::Debug for ScopedThreadsOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedThreadsOwnerV1(<original queued closed operation>)")
    }
}
impl ScopedThreadsOwnerV1 {
    fn new(intent: Arc<ThreadBrowseIntentV1>, work: WorkV1) -> Result<Arc<Self>, String> {
        Ok(Arc::new(Self {
            intent: ThreadOperationOriginV1::Files(intent),
            work,
            cancel: CancellationToken::default(),
            deadline: Instant::now()
                .checked_add(Duration::from_secs(30))
                .ok_or("Thread deadline unavailable")?,
            publication: std::sync::Mutex::new(None),
            #[cfg(test)]
            actual_worker_panic: false,
        }))
    }
    pub fn phase(&self) -> ScopedThreadPhaseV1 {
        match self.work {
            WorkV1::Browse => ScopedThreadPhaseV1::Browse,
            WorkV1::CommentsV2(_) => ScopedThreadPhaseV1::Browse,
            WorkV1::CommentOpenV2(_) => ScopedThreadPhaseV1::Open,
            WorkV1::CommentPreviewV2(_) => ScopedThreadPhaseV1::Preview,
            WorkV1::CommentConfirmV2(_) => ScopedThreadPhaseV1::Confirm,
            WorkV1::Open(_) => ScopedThreadPhaseV1::Open,
            WorkV1::Preview(_) => ScopedThreadPhaseV1::Preview,
            WorkV1::Confirm(_) => ScopedThreadPhaseV1::Confirm,
            WorkV1::HistoryList => ScopedThreadPhaseV1::HistoryList,
            WorkV1::HistoryOpen(..) => ScopedThreadPhaseV1::HistoryOpen,
            WorkV1::CommentHistoryListV2 => ScopedThreadPhaseV1::HistoryList,
            WorkV1::CommentHistoryOpenV2(..) => ScopedThreadPhaseV1::HistoryOpen,
        }
    }
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancel
    }
    pub fn deadline(&self) -> Instant {
        self.deadline
    }
    pub fn expired(&self) -> bool {
        self.cancel.is_cancelled() || Instant::now() >= self.deadline
    }
    fn request(&self) -> Option<&Arc<composer::ThreadComposerRequestV1>> {
        match &self.work {
            WorkV1::Preview(request) | WorkV1::Confirm(request) => Some(request),
            _ => None,
        }
    }
    fn comment_request_v2(&self) -> Option<&Arc<comment_composer_v2::CommentComposerRequestV2>> {
        match &self.work {
            WorkV1::CommentPreviewV2(request) | WorkV1::CommentConfirmV2(request) => Some(request),
            _ => None,
        }
    }
    fn comment_panel_v2(&self) -> Option<&Arc<comments_v2::ThreadCommentsIntentV2>> {
        match &self.work {
            WorkV1::CommentsV2(intent) => Some(intent),
            WorkV1::CommentOpenV2(intent) => Some(&intent.prepared().intent),
            WorkV1::CommentPreviewV2(request) | WorkV1::CommentConfirmV2(request) => {
                Some(&request.intent().prepared().intent)
            }
            _ => None,
        }
    }
    pub fn presentation_matches(&self, app: &App) -> bool {
        self.intent.owns(app)
            && match &self.work {
                WorkV1::Browse => {
                    app.overlay == Some(Overlay::PrThreadsV1)
                        && app.scoped_threads_v1.as_ref().is_some_and(|view| {
                            self.files_intent()
                                .is_some_and(|intent| Arc::ptr_eq(&view.intent, intent))
                                && view.prepared.is_none()
                        })
                }
                WorkV1::Open(intent) => {
                    app.overlay == Some(Overlay::PrThreadsV1) && intent.owns(app)
                }
                WorkV1::CommentsV2(intent) => {
                    app.overlay == Some(Overlay::PrThreadsV1)
                        && intent.owns(app)
                        && app
                            .scoped_threads_v1
                            .as_ref()
                            .and_then(|view| view.comments_v2.as_ref())
                            .is_some_and(|view| {
                                Arc::ptr_eq(&view.intent, intent) && view.prepared.is_none()
                            })
                }
                WorkV1::CommentOpenV2(intent) => {
                    app.overlay == Some(Overlay::PrThreadsV1) && intent.owns(app)
                }
                WorkV1::CommentPreviewV2(request) | WorkV1::CommentConfirmV2(request) => {
                    app.overlay == Some(Overlay::Form)
                        && request.intent().owns(app)
                        && app
                            .form
                            .as_ref()
                            .is_some_and(|form| request.matches_form(form))
                }
                WorkV1::Preview(request) | WorkV1::Confirm(request) => {
                    app.overlay == Some(Overlay::Form)
                        && request.intent().owns(app)
                        && app
                            .form
                            .as_ref()
                            .is_some_and(|form| request.matches_form(form))
                }
                WorkV1::HistoryList => {
                    app.overlay == Some(Overlay::PrThreadHistoryV1)
                        && app.scoped_thread_history_v1.as_ref().is_some_and(|view| {
                            self.history_intent()
                                .is_some_and(|intent| Arc::ptr_eq(&view.intent, intent))
                        })
                }
                WorkV1::HistoryOpen(list, index) => {
                    app.overlay == Some(Overlay::PrThreadHistoryV1)
                        && app.scoped_thread_history_v1.as_ref().is_some_and(|view| {
                            view.selected == *index
                                && view.opened.is_none()
                                && view
                                    .listed
                                    .as_ref()
                                    .is_some_and(|current| Arc::ptr_eq(current, list))
                        })
                }
                WorkV1::CommentHistoryListV2 => {
                    app.overlay == Some(Overlay::PrThreadHistoryV1)
                        && app
                            .scoped_thread_history_v1
                            .as_ref()
                            .and_then(|view| view.comment_history_v2.as_ref())
                            .is_some_and(|view| {
                                self.history_intent()
                                    .is_some_and(|intent| Arc::ptr_eq(intent, &view.intent))
                                    && view.listed.is_none()
                                    && view.opened.is_none()
                            })
                }
                WorkV1::CommentHistoryOpenV2(list, index) => {
                    app.overlay == Some(Overlay::PrThreadHistoryV1)
                        && app
                            .scoped_thread_history_v1
                            .as_ref()
                            .and_then(|view| view.comment_history_v2.as_ref())
                            .is_some_and(|view| {
                                view.selected == *index
                                    && view.opened.is_none()
                                    && view
                                        .listed
                                        .as_ref()
                                        .is_some_and(|current| Arc::ptr_eq(current, list))
                            })
                }
            }
    }
    pub(crate) fn files_intent(&self) -> Option<&Arc<ThreadBrowseIntentV1>> {
        match &self.intent {
            ThreadOperationOriginV1::Files(intent) => Some(intent),
            _ => None,
        }
    }
    pub(crate) fn history_intent(&self) -> Option<&Arc<history_v1::ThreadHistoryIntentV1>> {
        match &self.intent {
            ThreadOperationOriginV1::Summary(intent) => Some(intent),
            _ => None,
        }
    }
    fn new_history(
        intent: Arc<history_v1::ThreadHistoryIntentV1>,
        work: WorkV1,
    ) -> Result<Arc<Self>, String> {
        Ok(Arc::new(Self {
            intent: ThreadOperationOriginV1::Summary(intent),
            work,
            cancel: CancellationToken::default(),
            deadline: Instant::now()
                .checked_add(Duration::from_secs(30))
                .ok_or("History original deadline unavailable")?,
            publication: std::sync::Mutex::new(None),
            #[cfg(test)]
            actual_worker_panic: false,
        }))
    }
    pub fn owns(self: &Arc<Self>, app: &App) -> bool {
        app.scoped_threads_owner_v1
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(owner, self))
            && self.presentation_matches(app)
            && !newer_intent(app)
    }
    pub(crate) fn original_private_source_is_current(&self, app: &App) -> bool {
        self.intent.owns(app)
            && app.form.as_ref().is_none_or(|form| {
                self.request()
                    .is_some_and(|request| request.owns_presentation(form))
                    || self
                        .comment_request_v2()
                        .is_some_and(|request| request.owns_presentation(form))
            })
            && app.scoped_threads_v1.as_ref().is_none_or(|view| {
                self.files_intent()
                    .is_some_and(|intent| Arc::ptr_eq(&view.intent, intent))
            })
    }
    pub(crate) fn release_busy_presentation(&self, app: &mut App) {
        if let (Some(request), Some(form)) = (self.request(), &mut app.form) {
            request.retire_busy_presentation(form);
        }
        if let (Some(request), Some(form)) = (self.comment_request_v2(), &mut app.form) {
            request.retire_busy_presentation(form);
        }
    }
    pub(crate) fn rejected(&self, app: &mut App, message: &'static str) {
        self.release_busy_presentation(app);
        if self.intent.owns(app) {
            if let Some(view) = &mut app.scoped_threads_v1 {
                view.message = message;
            }
            if let Some(view) = &mut app.scoped_thread_history_v1 {
                view.message = message;
            }
        }
        app.status = message.into();
        app.hits.clear();
    }
}
/// Instrument only the genuine, uniquely owned operation before it enters the
/// SAME queue. Source, presentation, phase, token, clock and owner Arc remain
/// unchanged; the panic is raised inside the actual worker, not substituted
/// into an already completed result.
#[cfg(test)]
pub(crate) fn actual_worker_panic_for_test(
    mut original: Arc<ScopedThreadsOwnerV1>,
) -> Arc<ScopedThreadsOwnerV1> {
    Arc::get_mut(&mut original)
        .expect("Genuine thread operation must be uniquely owned and not queued")
        .actual_worker_panic = true;
    original
}
pub fn newer_intent(app: &App) -> bool {
    crate::scoped_pr_observed_tabs_v1::resources::immutable_images::inline_v1::native_v1::newer_intent(app)||app.scoped_threads_work_v1.is_some()||app.scoped_threads_cancel_v1
}
pub fn open(app: &mut App) {
    if app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_threads_work_v1.is_some()
    {
        app.status = "Thread operation busy • Esc cancels original operation".into();
        return;
    }
    match ThreadBrowseIntentV1::capture(app)
        .and_then(|intent| ScopedThreadsOwnerV1::new(intent, WorkV1::Browse))
    {
        Ok(owner) => {
            app.scoped_threads_v1 = Some(ui_v1::ThreadBrowserV1::pending(
                owner.files_intent().expect("Files browse owner").clone(),
            ));
            app.overlay = Some(Overlay::PrThreadsV1);
            app.scoped_threads_work_v1 = Some(owner);
            app.hits.clear();
        }
        Err(reason) => app.status = reason,
    }
}
pub fn open_publication(app: &mut App, action: ThreadPublicationActionV1) {
    if app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_threads_work_v1.is_some()
    {
        app.status = "Original thread operation busy • Esc cancels".into();
        return;
    }
    match ScopedThreadPublicationIntentV1::capture(app, action).and_then(|intent| {
        ScopedThreadsOwnerV1::new(intent.prepared.intent.clone(), WorkV1::Open(intent))
    }) {
        Ok(owner) => {
            app.scoped_threads_work_v1 = Some(owner);
            app.status="Opening original thread draft • current source/account rechecked • nothing published".into();
            app.hits.clear();
        }
        Err(reason) => app.status = reason,
    }
}
pub fn open_comments_v2(app: &mut App) {
    if app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_threads_work_v1.is_some()
    {
        return;
    }
    match comments_v2::ThreadCommentsIntentV2::capture(app).and_then(|intent| {
        ScopedThreadsOwnerV1::new(intent.original().clone(), WorkV1::CommentsV2(intent))
    }) {
        Ok(owner) => {
            let WorkV1::CommentsV2(intent) = &owner.work else {
                unreachable!("closed comment-list owner")
            };
            if let Some(view) = &mut app.scoped_threads_v1 {
                view.comments_v2 = Some(comments_v2::ui::ThreadCommentsBrowserV2::pending(
                    intent.clone(),
                ));
            }
            app.scoped_threads_work_v1 = Some(owner);
            app.hits.clear();
        }
        Err(reason) => app.status = reason,
    }
}
pub fn close_comments_v2(app: &mut App) {
    let Some(intent) = app
        .scoped_threads_v1
        .as_ref()
        .and_then(|view| view.comments_v2.as_ref())
        .map(|view| view.intent.clone())
    else {
        return;
    };
    if app.scoped_threads_work_v1.as_ref().is_some_and(|owner| {
        owner
            .comment_panel_v2()
            .is_some_and(|current| Arc::ptr_eq(current, &intent))
    }) {
        if let Some(owner) = app.scoped_threads_work_v1.take() {
            owner.cancel.cancel();
        }
    }
    if app.scoped_threads_owner_v1.as_ref().is_some_and(|owner| {
        owner
            .comment_panel_v2()
            .is_some_and(|current| Arc::ptr_eq(current, &intent))
    }) {
        app.scoped_threads_cancel_v1 = true;
    }
    if let Some(view) = &mut app.scoped_threads_v1 {
        view.comments_v2 = None;
        view.hits.clear();
    }
    app.hits.clear();
}
pub(crate) fn open_comment_publication_v2(
    app: &mut App,
    action: crate::pr_write_journal::thread_comment_history_v2::CommentActionV2,
) {
    if app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_threads_work_v1.is_some()
    {
        return;
    }
    match ThreadCommentIntentV2::capture(app, action).and_then(|intent| {
        ScopedThreadsOwnerV1::new(intent.original().clone(), WorkV1::CommentOpenV2(intent))
    }) {
        Ok(owner) => {
            app.scoped_threads_work_v1 = Some(owner);
            app.hits.clear();
            app.status="Opening exact submitted-comment intent • fresh capability/source required • nothing published".into();
        }
        Err(reason) => app.status = reason,
    }
}
pub fn open_history(app: &mut App) {
    if app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_threads_work_v1.is_some()
    {
        app.status = "Original thread operation busy • Esc cancels".into();
        return;
    }
    match history_v1::ThreadHistoryIntentV1::capture(app)
        .and_then(|intent| ScopedThreadsOwnerV1::new_history(intent, WorkV1::HistoryList))
    {
        Ok(owner) => {
            app.scoped_thread_history_v1 = Some(history_v1::ui::ThreadHistoryBrowserV1::pending(
                owner.history_intent().expect("Summary owner").clone(),
            ));
            app.overlay = Some(Overlay::PrThreadHistoryV1);
            app.scoped_threads_work_v1 = Some(owner);
            app.hits.clear();
        }
        Err(reason) => app.status = reason,
    }
}
pub fn refresh_history(app: &mut App) {
    if app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_threads_work_v1.is_some()
    {
        return;
    }
    let Some(view) = app
        .scoped_thread_history_v1
        .as_ref()
        .filter(|view| view.intent.owns(app) && app.overlay == Some(Overlay::PrThreadHistoryV1))
    else {
        return;
    };
    let intent = view.intent.clone();
    if let Ok(owner) = ScopedThreadsOwnerV1::new_history(intent.clone(), WorkV1::HistoryList) {
        app.scoped_thread_history_v1 =
            Some(history_v1::ui::ThreadHistoryBrowserV1::pending(intent));
        app.scoped_threads_work_v1 = Some(owner);
        app.hits.clear();
    }
}
pub fn open_history_record(app: &mut App) {
    if app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_threads_work_v1.is_some()
    {
        return;
    }
    let Some(view) = app.scoped_thread_history_v1.as_ref().filter(|view| {
        view.intent.owns(app)
            && view.opened.is_none()
            && app.overlay == Some(Overlay::PrThreadHistoryV1)
    }) else {
        return;
    };
    let Some(list) = view
        .listed
        .as_ref()
        .filter(|list| view.selected < list.labels.len())
    else {
        return;
    };
    if let Ok(owner) = ScopedThreadsOwnerV1::new_history(
        view.intent.clone(),
        WorkV1::HistoryOpen(list.clone(), view.selected),
    ) {
        app.scoped_threads_work_v1 = Some(owner);
        app.hits.clear();
    }
}
pub fn capture_form(app: &mut App) {
    capture_form_with_original_drafts(app, None);
}
pub fn open_comment_history_v2(app: &mut App) {
    if app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_threads_work_v1.is_some()
    {
        return;
    }
    let Some(intent) = app
        .scoped_thread_history_v1
        .as_ref()
        .filter(|view| view.intent.owns(app) && app.overlay == Some(Overlay::PrThreadHistoryV1))
        .map(|view| view.intent.clone())
    else {
        return;
    };
    if let Ok(owner) =
        ScopedThreadsOwnerV1::new_history(intent.clone(), WorkV1::CommentHistoryListV2)
    {
        if let Some(view) = &mut app.scoped_thread_history_v1 {
            view.comment_history_v2 =
                Some(history_v1::comments_v2::ui::CommentHistoryBrowserV2::pending(intent));
        }
        app.scoped_threads_work_v1 = Some(owner);
        app.hits.clear();
    }
}
pub fn refresh_comment_history_v2(app: &mut App) {
    open_comment_history_v2(app);
}
pub fn open_comment_history_record_v2(app: &mut App) {
    if app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_threads_work_v1.is_some()
    {
        return;
    }
    let Some(view) = app
        .scoped_thread_history_v1
        .as_ref()
        .and_then(|view| view.comment_history_v2.as_ref())
        .filter(|view| {
            view.intent.owns(app)
                && view.opened.is_none()
                && app.overlay == Some(Overlay::PrThreadHistoryV1)
        })
    else {
        return;
    };
    let Some(list) = view
        .listed
        .as_ref()
        .filter(|list| view.selected < list.labels.len())
    else {
        return;
    };
    if let Ok(owner) = ScopedThreadsOwnerV1::new_history(
        view.intent.clone(),
        WorkV1::CommentHistoryOpenV2(list.clone(), view.selected),
    ) {
        app.scoped_threads_work_v1 = Some(owner);
        app.hits.clear();
    }
}
pub fn close_comment_history_v2(app: &mut App) {
    let Some(intent) = app
        .scoped_thread_history_v1
        .as_ref()
        .and_then(|view| view.comment_history_v2.as_ref())
        .map(|view| view.intent.clone())
    else {
        return;
    };
    let matches = |owner: &Arc<ScopedThreadsOwnerV1>| {
        matches!(
            &owner.work,
            WorkV1::CommentHistoryListV2 | WorkV1::CommentHistoryOpenV2(..)
        ) && owner
            .history_intent()
            .is_some_and(|current| Arc::ptr_eq(current, &intent))
    };
    if app.scoped_threads_work_v1.as_ref().is_some_and(matches) {
        if let Some(owner) = app.scoped_threads_work_v1.take() {
            owner.cancel.cancel();
        }
    }
    if app.scoped_threads_owner_v1.as_ref().is_some_and(matches) {
        app.scoped_threads_cancel_v1 = true;
    }
    if let Some(view) = &mut app.scoped_thread_history_v1 {
        view.comment_history_v2 = None;
        view.hits.clear();
    }
    app.hits.clear();
}
/// Foreground metadata only. The same original client/database is forwarded
/// to the existing worker; no queue wait, SQL, body replay or new authority.
pub fn capture_form_with_drafts(
    app: &mut App,
    database: &Path,
    drafts: &crate::drafts::DraftClient,
) {
    capture_form_with_original_drafts(app, Some((database, drafts)));
}
fn capture_form_with_original_drafts(
    app: &mut App,
    original_drafts: Option<(&Path, &crate::drafts::DraftClient)>,
) {
    if app.form.as_ref().is_some_and(|form| {
        matches!(
            form.purpose,
            crate::forms::FormPurpose::PrThreadCommentV2(_)
        )
    }) {
        capture_comment_form_v2(app, original_drafts);
        return;
    }
    if app.scoped_threads_work_v1.is_some() || app.scoped_threads_owner_v1.is_some() {
        return;
    }
    let Some(form) = app
        .form
        .as_mut()
        .filter(|form| matches!(form.purpose, crate::forms::FormPurpose::PrThread(_)))
    else {
        return;
    };
    if form.submitting || (!form.preview_requested && !form.submit_requested) {
        return;
    }
    let confirm = form.submit_requested;
    form.preview_requested = false;
    form.submit_requested = false;
    let captured = if confirm {
        composer::confirmed_preview(form).and_then(|_| composer::request(form))
    } else {
        composer::request(form)
    };
    let request = match captured {
        Ok(request) => request,
        Err(reason) => {
            form.error = reason;
            form.invalidate_plan();
            return;
        }
    };
    let original = request.intent().prepared.intent.clone();
    let draft_key = request.draft_key();
    let draft_scope = request.intent().scope();
    let publication = if confirm {
        let Some((database, drafts)) = original_drafts else {
            form.error =
                "Exact original bounded draft writer required before Confirm; nothing queued"
                    .into();
            return;
        };
        Some((database, drafts, draft_scope.clone(), draft_key.to_owned()))
    } else {
        None
    };
    let owner = match ScopedThreadsOwnerV1::new(
        original,
        if confirm {
            WorkV1::Confirm(request)
        } else {
            WorkV1::Preview(request)
        },
    ) {
        Ok(owner) => owner,
        Err(reason) => {
            form.error = reason;
            return;
        }
    };
    if let Some((database, drafts, scope, draft_key)) = publication {
        let fence = match drafts.capture_publication_v2(
            database,
            &scope,
            &draft_key,
            owner.cancellation(),
            owner.deadline(),
        ) {
            Ok(fence) => fence,
            Err(_) => {
                form.error =
                    "Original draft revision unavailable/superseded; nothing queued or published"
                        .into();
                return;
            }
        };
        let Ok(mut publication) = owner.publication.lock() else {
            form.error = "Original publication capture unavailable; nothing queued".into();
            return;
        };
        *publication = Some(fence);
    }
    if owner.presentation_matches(app) && !newer_intent(app) {
        app.scoped_threads_work_v1 = Some(owner);
    }
}
fn capture_comment_form_v2(
    app: &mut App,
    original_drafts: Option<(&Path, &crate::drafts::DraftClient)>,
) {
    if app.scoped_threads_work_v1.is_some() || app.scoped_threads_owner_v1.is_some() {
        return;
    }
    let Some(form) = app.form.as_mut().filter(|form| {
        matches!(
            form.purpose,
            crate::forms::FormPurpose::PrThreadCommentV2(_)
        )
    }) else {
        return;
    };
    if form.submitting || (!form.preview_requested && !form.submit_requested) {
        return;
    }
    let confirm = form.submit_requested;
    form.preview_requested = false;
    form.submit_requested = false;
    let captured = if confirm {
        comment_composer_v2::confirmed_preview(form)
            .and_then(|_| comment_composer_v2::request(form))
    } else {
        comment_composer_v2::request(form)
    };
    let request = match captured {
        Ok(request) => request,
        Err(reason) => {
            form.error = reason;
            form.invalidate_plan();
            return;
        }
    };
    let key = request.draft_key().to_owned();
    let scope = request.intent().scope().clone();
    let original = request.intent().original().clone();
    let owner = match ScopedThreadsOwnerV1::new(
        original,
        if confirm {
            WorkV1::CommentConfirmV2(request)
        } else {
            WorkV1::CommentPreviewV2(request)
        },
    ) {
        Ok(owner) => owner,
        Err(reason) => {
            form.error = reason;
            return;
        }
    };
    if confirm {
        let Some((database, drafts)) = original_drafts else {
            form.error = "Exact original bounded database writer required • nothing queued".into();
            return;
        };
        let fence = match drafts.capture_publication_v2(
            database,
            &scope,
            &key,
            owner.cancellation(),
            owner.deadline(),
        ) {
            Ok(fence) => fence,
            Err(_) => {
                form.error =
                    "Original draft revision unavailable/superseded • nothing queued or invoked"
                        .into();
                return;
            }
        };
        let Ok(mut publication) = owner.publication.lock() else {
            form.error = "Original publication capture unavailable".into();
            return;
        };
        *publication = Some(fence);
    }
    if owner.presentation_matches(app) && !newer_intent(app) {
        app.scoped_threads_work_v1 = Some(owner);
    }
}
pub fn close(app: &mut App) {
    if let Some(work) = app.scoped_threads_work_v1.take() {
        work.cancel.cancel();
    }
    if app.scoped_threads_owner_v1.is_some() {
        app.scoped_threads_cancel_v1 = true;
    }
    app.scoped_threads_v1 = None;
    app.scoped_thread_history_v1 = None;
    if matches!(
        app.overlay,
        Some(Overlay::PrThreadsV1 | Overlay::PrThreadHistoryV1)
    ) {
        app.overlay = None;
    }
    app.hits.clear();
}
pub fn retire_private(app: &mut App) {
    if let Some(owner) = app.scoped_threads_work_v1.take() {
        owner.cancel.cancel();
    }
    if let Some(owner) = app.scoped_threads_owner_v1.take() {
        owner.cancel.cancel();
    }
    app.scoped_threads_v1 = None;
    app.scoped_thread_history_v1 = None;
    app.scoped_threads_cancel_v1 = false;
    if app.form.as_ref().is_some_and(|form| {
        matches!(
            form.purpose,
            crate::forms::FormPurpose::PrThread(_)
                | crate::forms::FormPurpose::PrThreadCommentV2(_)
        )
    }) {
        app.form = None;
        app.form_dismissed = true;
        if app.overlay == Some(Overlay::Form) {
            app.overlay = None;
        }
    }
    if matches!(
        app.overlay,
        Some(Overlay::PrThreadsV1 | Overlay::PrThreadHistoryV1)
    ) {
        app.overlay = None;
    }
}
enum ResultV1 {
    Browser(Result<Arc<PreparedThreadsV1>, ThreadBrowseFailureV1>),
    CommentsV2(Result<Arc<comments_v2::PreparedThreadCommentsV2>, ThreadBrowseFailureV1>),
    CommentPreviewV2(Arc<CommentPreviewV2>),
    CommentPublishedV2(CommentPublicationV2),
    Opened(Box<crate::forms::Form>),
    Preview(Arc<ScopedThreadPreviewV1>),
    Published(ScopedThreadPublicationV1),
    History(Result<Arc<history_v1::PreparedThreadHistoryV1>, ThreadBrowseFailureV1>),
    CommentHistoryV2(
        Result<Arc<history_v1::comments_v2::PreparedCommentHistoryV2>, ThreadBrowseFailureV1>,
    ),
    Failed {
        hide_private: bool,
        message: String,
    },
}
pub struct PreparedScopedThreadsV1 {
    owner: Arc<ScopedThreadsOwnerV1>,
    result: ResultV1,
}
impl PreparedScopedThreadsV1 {
    pub fn matches(&self, owner: &Arc<ScopedThreadsOwnerV1>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
            && match (&owner.work, &self.result) {
                (WorkV1::Browse, ResultV1::Browser(Ok(result))) => owner
                    .files_intent()
                    .is_some_and(|intent| Arc::ptr_eq(&result.intent, intent)),
                (WorkV1::Browse, ResultV1::Browser(Err(_))) => true,
                (WorkV1::CommentsV2(intent), ResultV1::CommentsV2(Ok(result))) => {
                    Arc::ptr_eq(intent, &result.intent)
                }
                (WorkV1::CommentsV2(_), ResultV1::CommentsV2(Err(_))) => true,
                (WorkV1::CommentOpenV2(intent), ResultV1::Opened(form)) => {
                    matches!(&form.purpose,crate::forms::FormPurpose::PrThreadCommentV2(state)if Arc::ptr_eq(state.intent(),intent))
                }
                (WorkV1::CommentPreviewV2(request), ResultV1::CommentPreviewV2(preview)) => {
                    Arc::ptr_eq(request.intent(), preview.intent())
                        && request.body().as_ref() == preview.body()
                        && request.draft_key() == preview.draft_key()
                }
                (WorkV1::CommentConfirmV2(_), ResultV1::CommentPublishedV2(_)) => true,
                (WorkV1::Open(intent), ResultV1::Opened(form)) => {
                    matches!(&form.purpose,crate::forms::FormPurpose::PrThread(state)if Arc::ptr_eq(state.intent(),intent))
                }
                (WorkV1::Preview(request), ResultV1::Preview(preview)) => {
                    Arc::ptr_eq(request.intent(), preview.intent())
                        && request.body().as_ref() == preview.body()
                }
                (WorkV1::Confirm(_), ResultV1::Published(_)) | (_, ResultV1::Failed { .. }) => true,
                (WorkV1::HistoryList, ResultV1::History(Ok(result))) => {
                    owner
                        .history_intent()
                        .is_some_and(|intent| Arc::ptr_eq(&result.intent, intent))
                        && result.opened.is_none()
                }
                (WorkV1::HistoryOpen(_, index), ResultV1::History(Ok(result))) => {
                    owner
                        .history_intent()
                        .is_some_and(|intent| Arc::ptr_eq(&result.intent, intent))
                        && result.opened == Some(*index)
                }
                (WorkV1::HistoryList | WorkV1::HistoryOpen(..), ResultV1::History(Err(_))) => true,
                (WorkV1::CommentHistoryListV2, ResultV1::CommentHistoryV2(Ok(result))) => {
                    owner
                        .history_intent()
                        .is_some_and(|intent| Arc::ptr_eq(intent, &result.intent))
                        && result.opened.is_none()
                }
                (
                    WorkV1::CommentHistoryOpenV2(_, index),
                    ResultV1::CommentHistoryV2(Ok(result)),
                ) => {
                    owner
                        .history_intent()
                        .is_some_and(|intent| Arc::ptr_eq(intent, &result.intent))
                        && result.opened == Some(*index)
                }
                (
                    WorkV1::CommentHistoryListV2 | WorkV1::CommentHistoryOpenV2(..),
                    ResultV1::CommentHistoryV2(Err(_)),
                ) => true,
                _ => false,
            }
    }
}
fn failed(error: publication_v1::ThreadPlanFailureV1) -> ResultV1 {
    ResultV1::Failed {
        hide_private: error.hide_private,
        message: error.message().into(),
    }
}
pub fn execute(
    owner: Arc<ScopedThreadsOwnerV1>,
    provider: &GithubProvider,
    database: &Path,
) -> PreparedScopedThreadsV1 {
    execute_with_drafts(owner, provider, database, None)
}
pub fn execute_with_drafts(
    owner: Arc<ScopedThreadsOwnerV1>,
    provider: &GithubProvider,
    database: &Path,
    drafts: Option<&crate::drafts::DraftClient>,
) -> PreparedScopedThreadsV1 {
    #[cfg(test)]
    if owner.actual_worker_panic {
        panic!("Intentional actual original thread worker panic");
    }
    let result=match &owner.work{
        WorkV1::Browse=>ResultV1::Browser(load(owner.files_intent().expect("Closed Files browse work").clone(),provider,database,&owner.cancel,owner.deadline)),
        WorkV1::CommentsV2(intent)=>ResultV1::CommentsV2(comments_v2::load(intent.clone(),provider,database,&owner.cancel,owner.deadline)),
        WorkV1::CommentOpenV2(intent)=>match CommentDraftV2::open(intent.clone(),provider,database,&owner.cancel,owner.deadline){
            Ok(draft)=>match comment_composer_v2::form_from_draft(draft){Ok(form)=>ResultV1::Opened(Box::new(form)),Err(message)=>ResultV1::Failed{hide_private:false,message}},
            Err(error)=>ResultV1::Failed{hide_private:error.hide_private,message:error.message().into()},
        },
        WorkV1::CommentPreviewV2(request)=>match CommentPreviewV2::prepare(request.intent().clone(),request.body().clone(),provider,database,&owner.cancel,owner.deadline){
            Ok(preview)=>ResultV1::CommentPreviewV2(preview),Err(error)=>ResultV1::Failed{hide_private:error.hide_private,message:error.message().into()},
        },
        WorkV1::CommentConfirmV2(request)=>match(request.preview(),drafts,owner.publication.lock().ok().and_then(|mut publication|publication.take())){
            (Some(preview),Some(drafts),Some(publication))=>match preview.execute(provider,database,drafts,publication,true,None,&owner.cancel,owner.deadline){
                Ok(result)=>ResultV1::CommentPublishedV2(result),Err(error)=>ResultV1::Failed{hide_private:error.hide_private,message:error.message().into()},
            },_=>ResultV1::Failed{hide_private:false,message:"Fresh matching comment Preview and original bounded writer required • nothing invoked".into()},
        },
        WorkV1::HistoryList=>ResultV1::History(history_v1::load_history(owner.history_intent().expect("Closed Summary history work").clone(),None,provider,database,&owner.cancel,owner.deadline)),
        WorkV1::HistoryOpen(old,index)=>ResultV1::History(history_v1::load_history(owner.history_intent().expect("Closed Summary history work").clone(),Some((old,*index)),provider,database,&owner.cancel,owner.deadline)),
        WorkV1::CommentHistoryListV2=>ResultV1::CommentHistoryV2(history_v1::comments_v2::load(owner.history_intent().expect("Closed Summary comment history work").clone(),None,provider,database,&owner.cancel,owner.deadline)),
        WorkV1::CommentHistoryOpenV2(old,index)=>ResultV1::CommentHistoryV2(history_v1::comments_v2::load(owner.history_intent().expect("Closed Summary comment history work").clone(),Some((old,*index)),provider,database,&owner.cancel,owner.deadline)),
        WorkV1::Open(intent)=>match ScopedThreadDraftV1::open(intent.clone(),provider,database,&owner.cancel,owner.deadline){
            Ok(draft)=>match composer::form_from_draft(draft){Ok(form)=>ResultV1::Opened(Box::new(form)),Err(message)=>ResultV1::Failed{hide_private:false,message}},
            Err(error)=>failed(error),
        },
        WorkV1::Preview(request)=>match ScopedThreadPreviewV1::prepare(request.intent().clone(),request.body().clone(),provider,database,&owner.cancel,owner.deadline){Ok(preview)=>ResultV1::Preview(preview),Err(error)=>failed(error)},
        WorkV1::Confirm(request)=>match (request.preview(),drafts,owner.publication.lock().ok().and_then(|mut publication|publication.take())){
            (Some(preview),Some(drafts),Some(publication))=>match preview.execute(provider,database,drafts,publication,true,None,&owner.cancel,owner.deadline){Ok(result)=>ResultV1::Published(result),Err(error)=>failed(error)},
            _=>ResultV1::Failed{hide_private:false,message:"Fresh matching Preview and original database draft writer required; nothing invoked".into()},
        },
    };
    PreparedScopedThreadsV1 { owner, result }
}
pub fn needs_private_retirement(result: &PreparedScopedThreadsV1) -> bool {
    matches!(
        result.result,
        ResultV1::Browser(Err(ThreadBrowseFailureV1::Source))
            | ResultV1::CommentsV2(Err(ThreadBrowseFailureV1::Source))
            | ResultV1::History(Err(ThreadBrowseFailureV1::Source))
            | ResultV1::CommentHistoryV2(Err(ThreadBrowseFailureV1::Source))
            | ResultV1::Failed {
                hide_private: true,
                ..
            }
    )
}
pub fn publish(
    app: &mut App,
    background: &mut crate::runtime::Background,
    owner: &Arc<ScopedThreadsOwnerV1>,
    result: PreparedScopedThreadsV1,
) {
    match result.result {
        ResultV1::CommentPreviewV2(preview) => {
            if let (Some(request), Some(form)) = (owner.comment_request_v2(), &mut app.form) {
                if !comment_composer_v2::accept_preview(form, request, preview) {
                    form.invalidate_plan();
                    form.error = "Comment Preview presentation refused • draft retained".into();
                }
                form.submitting = false;
            }
            app.status="Fresh exact selected-comment Preview ready • explicit Confirm required • nothing invoked".into();
        }
        ResultV1::CommentPublishedV2(result) => {
            let state = result.state();
            let id = result.confirmed_comment_id();
            fail(app, background);
            app.status=match state {
                comment_publication_v2::CommentPublicationStateV2::ConfirmedSnapshotRetired=>id.map_or_else(||"Comment effect observed; original snapshot retired—refresh explicitly".into(),|id|format!("Comment effect observed #{id}; original snapshot retired—refresh explicitly")),
                comment_publication_v2::CommentPublicationStateV2::NotStarted=>"Comment write not started • protected record retained • unknown draft ACK may have persisted • inspect without resend".into(),
                comment_publication_v2::CommentPublicationStateV2::Uncertain=>"Comment outcome unknown • protected draft/journal retained • nothing resent; refresh explicitly".into(),
                comment_publication_v2::CommentPublicationStateV2::ConfirmedButJournalFailed=>"Comment effect observed • durable confirmation unavailable • protection retained; refresh explicitly".into(),
                comment_publication_v2::CommentPublicationStateV2::ConfirmedButSourceUnavailable=>"Comment effect observed • current source unavailable • private views hidden; refresh explicitly".into(),
            };
        }
        ResultV1::CommentsV2(Ok(prepared)) => {
            if let Some(view) = app
                .scoped_threads_v1
                .as_mut()
                .and_then(|view| view.comments_v2.as_mut())
            {
                view.prepared = Some(prepared);
                view.message="Fresh exact selected-thread comments • explicit fresh Preview required for writes";
            }
            app.status="Selected thread comments ready • recorded author differs from action account • no write permission from rows".into();
        }
        ResultV1::CommentHistoryV2(Ok(prepared)) => {
            if let Some(view) = app
                .scoped_thread_history_v1
                .as_mut()
                .and_then(|view| view.comment_history_v2.as_mut())
            {
                if prepared.opened.is_some() {
                    view.opened = Some(prepared);
                } else {
                    view.listed = Some(prepared);
                    view.opened = None;
                    view.selected = 0;
                }
                view.scroll = 0;
                view.column = 0;
                view.message="Exact protected comment/reaction history ready • inspect only • nothing resent";
            }
            app.status="Comment history ready • historical target only • no current existence, restore or resend permission".into();
        }
        ResultV1::CommentHistoryV2(Err(ThreadBrowseFailureV1::Source)) => {
            unreachable!("Source refusal retires before publication")
        }
        ResultV1::CommentHistoryV2(Err(ThreadBrowseFailureV1::Display)) => {
            if let Some(view) = app
                .scoped_thread_history_v1
                .as_mut()
                .and_then(|view| view.comment_history_v2.as_mut())
            {
                view.message="Comment history display bound exceeded • no body truncated • Back preserves original history";
            }
            app.status = "Comment history display bound exceeded • no private body admitted".into();
        }
        ResultV1::CommentsV2(Err(ThreadBrowseFailureV1::Source)) => {
            unreachable!("Source refusal retires before publication")
        }
        ResultV1::CommentsV2(Err(ThreadBrowseFailureV1::Display)) => {
            if let Some(view) = app
                .scoped_threads_v1
                .as_mut()
                .and_then(|view| view.comments_v2.as_mut())
            {
                view.message="Comment presentation exceeds bounded display; no body truncated • Back preserves original thread";
            }
        }
        ResultV1::History(Ok(prepared)) => {
            if let Some(view) = &mut app.scoped_thread_history_v1 {
                if prepared.opened.is_some() {
                    view.opened = Some(prepared);
                } else {
                    view.listed = Some(prepared);
                    view.opened = None;
                    view.selected = 0;
                }
                view.scroll = 0;
                view.column = 0;
                view.message =
                    "Exact protected local history ready • inspect only • nothing resent";
            }
            app.status="Thread history ready • observed original target only • no resend/restore permission".into();
        }
        ResultV1::History(Err(ThreadBrowseFailureV1::Source)) => {
            unreachable!("Source failure retires before publication")
        }
        ResultV1::History(Err(ThreadBrowseFailureV1::Display)) => {
            if let Some(view) = &mut app.scoped_thread_history_v1 {
                view.message =
                    "History display bound exceeded • no text truncated • Back preserves Summary";
            }
            app.status = "History display bound exceeded • no private body admitted".into();
        }
        ResultV1::Browser(Ok(prepared)) => {
            if let Some(view) = &mut app.scoped_threads_v1 {
                view.prepared = Some(prepared);
            }
            app.status =
                "Original thread observation ready • read only • partial/outdated labels retained"
                    .into();
        }
        ResultV1::Browser(Err(ThreadBrowseFailureV1::Display)) => {
            if let Some(view) = &mut app.scoped_threads_v1 {
                view.message="Thread display bound exceeded • original Files preserved • Back returns without reads";
            }
            app.status = "Thread display bound exceeded • original Files preserved".into();
        }
        ResultV1::Browser(Err(ThreadBrowseFailureV1::Source)) => {
            unreachable!("Source failure retired before publication")
        }
        ResultV1::Opened(form) => {
            app.form = Some(*form);
            app.form_dismissed = false;
            app.overlay = Some(Overlay::Form);
            app.status =
                "Original thread draft opened • fresh Preview required • nothing published".into();
        }
        ResultV1::Preview(preview) => {
            if let (Some(request), Some(form)) = (owner.request(), &mut app.form) {
                if !composer::accept_preview(form, request, preview) {
                    form.invalidate_plan();
                    form.error = "Thread preview presentation refused • draft retained".into();
                }
                form.submitting = false;
            }
            app.status =
                "Fresh thread preview ready • explicit Confirm required • nothing published".into();
        }
        ResultV1::Failed {
            hide_private,
            message,
        } => {
            if hide_private || owner.phase() == ScopedThreadPhaseV1::Confirm {
                fail(app, background);
            } else if let Some(form) = &mut app.form {
                owner.release_busy_presentation_form(form);
                form.error = message;
                app.status = "Thread intent refused • draft retained • no automatic retry".into();
            } else {
                app.status = message;
            }
        }
        ResultV1::Published(result) => {
            let state = result.state();
            let id = result.confirmed_reply_id();
            fail(app, background);
            app.status=match state{
                ThreadPublicationStateV1::ConfirmedSnapshotRetired=>id.map_or_else(||"Thread state observed as requested; original snapshot retired—refresh explicitly".into(),|id|format!("Reply posted #{id}; original snapshot retired—refresh explicitly")),
                ThreadPublicationStateV1::NotStarted=>"Thread publication not started • original pending record retained • inspect without resend".into(),
                ThreadPublicationStateV1::Uncertain=>"Thread outcome unknown • draft/journal protection retained • nothing resent; refresh explicitly".into(),
                ThreadPublicationStateV1::ConfirmedButJournalFailed=>"Thread write observed; durable confirmation unavailable • protection retained; refresh explicitly".into(),
                ThreadPublicationStateV1::ConfirmedButSourceUnavailable=>"Thread write observed; current source unavailable • private views hidden; refresh explicitly".into(),
            };
        }
    }
    app.loading[View::Inbox.index()] = false;
    app.hits.clear();
    if app
        .scoped_threads_owner_v1
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, owner))
    {
        app.scoped_threads_owner_v1 = None;
    }
}
impl ScopedThreadsOwnerV1 {
    fn release_busy_presentation_form(&self, form: &mut crate::forms::Form) {
        if let Some(request) = self.request() {
            request.retire_busy_presentation(form);
        }
        if let Some(request) = self.comment_request_v2() {
            request.retire_busy_presentation(form);
        }
    }
}
pub fn fail(app: &mut App, background: &mut crate::runtime::Background) {
    use crate::active_remote_scope_v1::{RemoteScopeFenceV1, RemoteScopeStopV1};
    if let Some(scope) = &mut app.remote_scope {
        Arc::make_mut(scope).fence(RemoteScopeFenceV1::SourceUnavailable);
    }
    crate::remote_scope_gate_v1::retire_remote(
        app,
        background,
        crate::remote_scope_gate_v1::ScopeEntryStopV1::Intent(RemoteScopeStopV1::Fenced(
            RemoteScopeFenceV1::SourceUnavailable,
        )),
    );
    retire_private(app);
    app.loading[View::Inbox.index()] = false;
    app.status =
        "Original thread source unavailable • private views hidden • refresh explicitly".into();
    app.hits.clear();
}
/// An orphan has NO authority over Background or a replacement Inbox slot.
/// Hide only its still-current original private presentation, leaving genuine
/// source/scope/navigation allocations and successor status/loading intact.
pub(crate) fn retire_orphaned_confirm_presentation(app: &mut App, owner: &ScopedThreadsOwnerV1) {
    if owner.phase() != ScopedThreadPhaseV1::Confirm {
        return;
    }
    let old_form = app.form.as_ref().is_some_and(|form| {
        owner
            .request()
            .is_some_and(|request| request.owns_presentation(form))
            || owner
                .comment_request_v2()
                .is_some_and(|request| request.owns_presentation(form))
    });
    let old_browser = app.scoped_threads_v1.as_ref().is_some_and(|view| {
        owner
            .files_intent()
            .is_some_and(|intent| Arc::ptr_eq(&view.intent, intent))
    });
    let newer_original_form = !old_form
        && app.form.as_ref().is_some_and(|form| match &form.purpose {
            crate::forms::FormPurpose::PrThread(state) => state.intent().owns(app),
            crate::forms::FormPurpose::PrThreadCommentV2(state) => state.intent().owns(app),
            _ => false,
        });
    let exact_original = owner.intent.owns(app)
        && ((old_form && !app.form_dismissed && app.overlay == Some(Overlay::Form))
            || old_browser && app.overlay == Some(Overlay::PrThreadsV1));
    if exact_original {
        let old_images = app
            .pr_file_images
            .as_ref()
            .is_some_and(|images| images.owns(app));
        let old_checks = app
            .pr_checks_origin
            .as_ref()
            .and_then(|origin| origin.scoped_owner_v1())
            .is_some_and(|(source, scope, nav)| {
                Arc::ptr_eq(source, owner.intent.source())
                    && Arc::ptr_eq(scope, owner.intent.active())
                    && Arc::ptr_eq(nav, owner.intent.navigation())
            });
        let parent = owner
            .files_intent()
            .map(|intent| intent.original_view.observed.parent());
        let old_reactions = parent.is_some_and(|parent| {
            app.pr_reactions.as_ref().is_some_and(|page| {
                Scope::github(&page.identity.host, page.identity.account_id)
                    .ok()
                    .as_ref()
                    == Some(owner.intent.scope())
                    && page.target.pr.id == parent.node_id()
                    && page
                        .target
                        .pr
                        .repository
                        .eq_ignore_ascii_case(parent.repository())
                    && page.target.pr.number == parent.number()
            })
        });
        let old_inspection = parent.is_some_and(|parent| {
            app.pr_inspection.as_ref().is_some_and(|inspection| {
                inspection.try_lock().is_ok_and(|inspection| {
                    inspection.as_ref().is_some_and(|inspection| {
                        let target = inspection.target();
                        target.scope() == owner.intent.scope()
                            && target.node_id() == parent.node_id()
                            && target
                                .repository()
                                .eq_ignore_ascii_case(parent.repository())
                            && target.number() == parent.number()
                    })
                })
            })
        });
        if let Some(github) = &mut app.github {
            github.pr = None;
            github.scoped_pr_resource = None;
        }
        if old_images {
            app.pr_file_images = None;
            if app.overlay == Some(Overlay::PrFileImages) {
                app.overlay = None;
            }
        }
        if old_checks {
            app.pr_checks_origin = None;
            app.pr_checks_route = None;
            app.actions = None;
            app.actions_inspection = None;
            app.pr_check_choice_picker = None;
            app.pr_check_log_matches_v2 = None;
            app.pr_check_log_editor_v2 = None;
            app.pr_check_annotations_detail_v2 = None;
            app.pr_check_attempt_advance_v2 = None;
        }
        if old_reactions {
            app.pr_reactions = None;
            app.pr_reaction_display.clear();
            app.pr_reaction_scroll = 0;
        }
        if old_inspection {
            app.pr_inspection = None;
        }
        app.pr_pending_message.clear();
        app.avatar_states.clear();
        app.hits.clear();
    }
    if old_form {
        app.form = None;
        app.form_dismissed = true;
        if app.overlay == Some(Overlay::Form) {
            app.overlay = None;
        }
        app.hits.clear();
    }
    if old_browser && !newer_original_form {
        app.scoped_threads_v1 = None;
        if app.overlay == Some(Overlay::PrThreadsV1) {
            app.overlay = None;
        }
        app.hits.clear();
    }
}
