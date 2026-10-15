//! Closed browser/draft/preview/confirm operations in the SAME Inbox slot.
use super::*;
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
}
enum WorkV1 {
    Browse,
    Open(Arc<ScopedThreadPublicationIntentV1>),
    Preview(Arc<composer::ThreadComposerRequestV1>),
    Confirm(Arc<composer::ThreadComposerRequestV1>),
}
pub struct ScopedThreadsOwnerV1 {
    pub intent: Arc<ThreadBrowseIntentV1>,
    work: WorkV1,
    cancel: CancellationToken,
    deadline: Instant,
    publication: std::sync::Mutex<Option<crate::drafts::DraftPublicationFenceV2>>,
}
impl std::fmt::Debug for ScopedThreadsOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedThreadsOwnerV1(<original queued closed operation>)")
    }
}
impl ScopedThreadsOwnerV1 {
    fn new(intent: Arc<ThreadBrowseIntentV1>, work: WorkV1) -> Result<Arc<Self>, String> {
        Ok(Arc::new(Self {
            intent,
            work,
            cancel: CancellationToken::default(),
            deadline: Instant::now()
                .checked_add(Duration::from_secs(30))
                .ok_or("Thread deadline unavailable")?,
            publication: std::sync::Mutex::new(None),
        }))
    }
    pub fn phase(&self) -> ScopedThreadPhaseV1 {
        match self.work {
            WorkV1::Browse => ScopedThreadPhaseV1::Browse,
            WorkV1::Open(_) => ScopedThreadPhaseV1::Open,
            WorkV1::Preview(_) => ScopedThreadPhaseV1::Preview,
            WorkV1::Confirm(_) => ScopedThreadPhaseV1::Confirm,
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
    pub fn presentation_matches(&self, app: &App) -> bool {
        self.intent.owns(app)
            && match &self.work {
                WorkV1::Browse => {
                    app.overlay == Some(Overlay::PrThreadsV1)
                        && app.scoped_threads_v1.as_ref().is_some_and(|view| {
                            Arc::ptr_eq(&view.intent, &self.intent) && view.prepared.is_none()
                        })
                }
                WorkV1::Open(intent) => {
                    app.overlay == Some(Overlay::PrThreadsV1) && intent.owns(app)
                }
                WorkV1::Preview(request) | WorkV1::Confirm(request) => {
                    app.overlay == Some(Overlay::Form)
                        && request.intent().owns(app)
                        && app
                            .form
                            .as_ref()
                            .is_some_and(|form| request.matches_form(form))
                }
            }
    }
    pub fn owns(self: &Arc<Self>, app: &App) -> bool {
        app.scoped_threads_owner_v1
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(owner, self))
            && self.presentation_matches(app)
            && !newer_intent(app)
    }
    pub(crate) fn original_private_source_is_current(&self, app: &App) -> bool {
        app.remote_scope_source
            .as_ref()
            .is_some_and(|source| Arc::ptr_eq(source, &self.intent.source))
            && app
                .remote_scope
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &self.intent.active))
    }
    pub(crate) fn release_busy_presentation(&self, app: &mut App) {
        if let (Some(request), Some(form)) = (self.request(), &mut app.form) {
            request.retire_busy_presentation(form);
        }
    }
    pub(crate) fn rejected(&self, app: &mut App, message: &'static str) {
        self.release_busy_presentation(app);
        if self.intent.owns(app) {
            if let Some(view) = &mut app.scoped_threads_v1 {
                view.message = message;
            }
        }
        app.status = message.into();
        app.hits.clear();
    }
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
            app.scoped_threads_v1 = Some(ui_v1::ThreadBrowserV1::pending(owner.intent.clone()));
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
pub fn capture_form(app: &mut App) {
    capture_form_with_original_drafts(app, None);
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
pub fn close(app: &mut App) {
    if let Some(work) = app.scoped_threads_work_v1.take() {
        work.cancel.cancel();
    }
    if app.scoped_threads_owner_v1.is_some() {
        app.scoped_threads_cancel_v1 = true;
    }
    app.scoped_threads_v1 = None;
    if app.overlay == Some(Overlay::PrThreadsV1) {
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
    app.scoped_threads_cancel_v1 = false;
    if app
        .form
        .as_ref()
        .is_some_and(|form| matches!(form.purpose, crate::forms::FormPurpose::PrThread(_)))
    {
        app.form = None;
        app.form_dismissed = true;
        if app.overlay == Some(Overlay::Form) {
            app.overlay = None;
        }
    }
    if app.overlay == Some(Overlay::PrThreadsV1) {
        app.overlay = None;
    }
}
enum ResultV1 {
    Browser(Result<Arc<PreparedThreadsV1>, ThreadBrowseFailureV1>),
    Opened(Box<crate::forms::Form>),
    Preview(Arc<ScopedThreadPreviewV1>),
    Published(ScopedThreadPublicationV1),
    Failed { hide_private: bool, message: String },
}
pub struct PreparedScopedThreadsV1 {
    owner: Arc<ScopedThreadsOwnerV1>,
    result: ResultV1,
}
impl PreparedScopedThreadsV1 {
    pub fn matches(&self, owner: &Arc<ScopedThreadsOwnerV1>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
            && match (&owner.work, &self.result) {
                (WorkV1::Browse, ResultV1::Browser(Ok(result))) => {
                    Arc::ptr_eq(&result.intent, &owner.intent)
                }
                (WorkV1::Browse, ResultV1::Browser(Err(_))) => true,
                (WorkV1::Open(intent), ResultV1::Opened(form)) => {
                    matches!(&form.purpose,crate::forms::FormPurpose::PrThread(state)if Arc::ptr_eq(state.intent(),intent))
                }
                (WorkV1::Preview(request), ResultV1::Preview(preview)) => {
                    Arc::ptr_eq(request.intent(), preview.intent())
                        && request.body().as_ref() == preview.body()
                }
                (WorkV1::Confirm(_), ResultV1::Published(_)) | (_, ResultV1::Failed { .. }) => true,
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
    let result=match &owner.work{
        WorkV1::Browse=>ResultV1::Browser(load(owner.intent.clone(),provider,database,&owner.cancel,owner.deadline)),
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
