//! Actual scoped-inline scheduling intent and closed worker result. Same
//! Background Inbox queue/runner; original read range never grants permission.
use super::{
    ui_v1, InlineFailureV1, ScopedInlineDraftV1, ScopedInlineIntentV1, ScopedInlinePreviewV1,
    ScopedInlinePublicationStateV1, ScopedInlinePublicationV1,
};
use crate::{
    app::{App, Overlay, View},
    providers::{gh::GithubProvider, runner::CancellationToken},
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedInlinePhaseV1 {
    Open,
    Preview,
    Confirm,
}
enum Kind {
    #[cfg(test)]
    ActualWorkerPanic(Arc<ScopedInlineWorkV1>),
    Open(Arc<ScopedInlineIntentV1>),
    Preview(Arc<ui_v1::InlineComposerRequestV1>),
    Confirm(Arc<ui_v1::InlineComposerRequestV1>),
}
pub struct ScopedInlineWorkV1 {
    kind: Kind,
}
impl std::fmt::Debug for ScopedInlineWorkV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedInlineWorkV1(<original intent; no permission>)")
    }
}
impl ScopedInlineWorkV1 {
    pub fn phase(&self) -> ScopedInlinePhaseV1 {
        match self.kind {
            #[cfg(test)]
            Kind::ActualWorkerPanic(ref original) => original.phase(),
            Kind::Open(_) => ScopedInlinePhaseV1::Open,
            Kind::Preview(_) => ScopedInlinePhaseV1::Preview,
            Kind::Confirm(_) => ScopedInlinePhaseV1::Confirm,
        }
    }
    pub fn intent(&self) -> &Arc<ScopedInlineIntentV1> {
        match &self.kind {
            #[cfg(test)]
            Kind::ActualWorkerPanic(original) => original.intent(),
            Kind::Open(intent) => intent,
            Kind::Preview(request) | Kind::Confirm(request) => request.intent(),
        }
    }
    pub fn request(&self) -> Option<&Arc<ui_v1::InlineComposerRequestV1>> {
        match &self.kind {
            #[cfg(test)]
            Kind::ActualWorkerPanic(original) => original.request(),
            Kind::Open(_) => None,
            Kind::Preview(request) | Kind::Confirm(request) => Some(request),
        }
    }
    pub fn matches(&self, app: &App) -> bool {
        if app.view != View::Inbox || !self.intent().owns(app) {
            return false;
        }
        match &self.kind {
            #[cfg(test)]
            Kind::ActualWorkerPanic(original) => original.matches(app),
            Kind::Open(intent) => {
                app.overlay == Some(Overlay::PrFileImages)
                    && app
                        .pr_file_images
                        .as_ref()
                        .and_then(|view| view.canonical_range())
                        .is_some_and(|range| {
                            Arc::ptr_eq(&intent.proof, &range.proof)
                                && intent.side == range.side
                                && intent.first == range.first
                                && intent.last == range.last
                        })
            }
            Kind::Preview(request) | Kind::Confirm(request) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app
                        .form
                        .as_ref()
                        .is_some_and(|form| request.matches_form(form))
            }
        }
    }
}
#[cfg(test)]
pub(crate) fn actual_worker_panic_for_test(
    original: Arc<ScopedInlineWorkV1>,
) -> Arc<ScopedInlineWorkV1> {
    Arc::new(ScopedInlineWorkV1 {
        kind: Kind::ActualWorkerPanic(original),
    })
}
pub struct ScopedInlineOwnerV1 {
    work: Arc<ScopedInlineWorkV1>,
    cancel: CancellationToken,
    deadline: Instant,
}
impl std::fmt::Debug for ScopedInlineOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedInlineOwnerV1(<original queued operation>)")
    }
}
impl ScopedInlineOwnerV1 {
    pub(crate) fn new(work: Arc<ScopedInlineWorkV1>) -> Result<Arc<Self>, String> {
        Ok(Arc::new(Self {
            work,
            cancel: CancellationToken::default(),
            deadline: Instant::now()
                .checked_add(Duration::from_secs(30))
                .ok_or("Inline deadline unavailable")?,
        }))
    }
    pub fn work(&self) -> &Arc<ScopedInlineWorkV1> {
        &self.work
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
    /// Presentation dismissal cannot make a possibly invoked write safe to
    /// display. This is ONLY the exact original observed source/scope, never a
    /// newer chosen source and never permission to read, reconcile or resend.
    pub(crate) fn original_private_source_is_current(&self, app: &App) -> bool {
        let proof = &self.work.intent().proof;
        app.remote_scope_source
            .as_ref()
            .is_some_and(|source| Arc::ptr_eq(source, &proof.source))
            && app
                .remote_scope
                .as_ref()
                .is_some_and(|scope| Arc::ptr_eq(scope, &proof.active))
    }
    pub fn owns(self: &Arc<Self>, app: &App) -> bool {
        app.scoped_inline_owner_v1
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(owner, self))
            && self.work.matches(app)
            && !newer_intent(app)
    }
    pub(crate) fn remote_owners(
        &self,
    ) -> (
        Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>,
        Arc<crate::storage::CatalogueIntentSource>,
        Arc<crate::scoped_pr_observed_tabs_v1::ScopedPrNavigationV1>,
    ) {
        let proof = &self.work.intent().proof;
        (
            proof.active.clone(),
            proof.source.clone(),
            proof.navigation.clone(),
        )
    }
}
pub fn newer_intent(app: &App) -> bool {
    if app.scoped_threads_work_v1.is_some() || app.scoped_threads_cancel_v1 {
        return true;
    }
    if app
        .form
        .as_ref()
        .is_some_and(|form| form.preview_requested || form.submit_requested)
    {
        return true;
    }
    app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app.scoped_inline_work_v1.is_some()
        || app.scoped_inline_cancel_v1
        || app.cancel_requested
        || app.refresh_requested
        || app.scoped_pr_tab_cancel_requested
        || app.repository_load_requested
        || app.repository_action.is_some()
        || app.graph_reread_request_v3.is_some()
        || app.original_tree_work_v1.is_some()
        || app.original_tree_cancel_v1
        || app.original_diff_work_v1.is_some()
        || app.original_diff_cancel_v1
        || app.original_stash_recovery_request_v1.is_some()
        || app.original_stash_recovery_cancel_requested_v1
        || app.related_pr_requested_v1
        || app.related_pr_cancel_v1
        || app.github_action.is_some()
        || app.pr_action.is_some()
        || app.actions_action.is_some()
        || app.actions_load_requested
        || app.search_action.is_some()
        || app.search_load_requested
        || app.source_action.is_some()
        || app.corpus_requested
        || app.metrics_action.is_some()
        || app.metrics_load_requested
        || app.local_command.is_some()
        || app.navigation_restore.is_some()
        || app.navigation_restore_inflight
}
pub fn open(app: &mut App) {
    if app.scoped_inline_owner_v1.is_some()
        || app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app.scoped_inline_work_v1.is_some()
        || app.loading[View::Inbox.index()]
    {
        app.status = "Inline operation busy • Esc cancels original operation".into();
        return;
    }
    let result = app
        .pr_file_images
        .as_ref()
        .and_then(|view| view.canonical_range())
        .ok_or("Choose R Range on the original image first; no line or side was guessed")
        .and_then(|range| {
            ScopedInlineIntentV1::capture(range)
                .map_err(|_| "Original range cannot create a bounded inline intent")
        });
    match result {
        Ok(intent) => {
            let work = Arc::new(ScopedInlineWorkV1 {
                kind: Kind::Open(intent),
            });
            if work.matches(app) {
                app.scoped_inline_work_v1 = Some(work);
            }
        }
        Err(message) => notice(app, message),
    }
}
fn notice(app: &mut App, message: &str) {
    app.status = message.into();
    if let Some(view) = &mut app.pr_file_images {
        view.inline_notice(message);
    }
}
pub fn capture_form(app: &mut App) {
    if app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app.scoped_inline_work_v1.is_some()
        || app.scoped_inline_owner_v1.is_some()
    {
        return;
    }
    let Some(form) = app
        .form
        .as_mut()
        .filter(|form| matches!(form.purpose, crate::forms::FormPurpose::PrInline(_)))
    else {
        return;
    };
    if form.submitting || (!form.preview_requested && !form.submit_requested) {
        return;
    }
    let confirm = form.submit_requested;
    form.preview_requested = false;
    form.submit_requested = false;
    let result = if confirm {
        ui_v1::confirmed_preview(form)
            .map(|_| ())
            .and_then(|_| ui_v1::request(form))
    } else {
        ui_v1::request(form)
    };
    match result {
        Ok(request) => {
            let work = Arc::new(ScopedInlineWorkV1 {
                kind: if confirm {
                    Kind::Confirm(request)
                } else {
                    Kind::Preview(request)
                },
            });
            if work.matches(app) {
                app.scoped_inline_work_v1 = Some(work)
            }
        }
        Err(reason) => {
            form.error = reason;
            form.invalidate_plan();
        }
    }
}
enum ResultKind {
    Opened(Box<crate::forms::Form>),
    Preview(Arc<ScopedInlinePreviewV1>),
    Published(ScopedInlinePublicationV1),
    Failed { hide_private: bool, message: String },
}
pub struct PreparedScopedInlineV1 {
    owner: Arc<ScopedInlineOwnerV1>,
    kind: ResultKind,
}
impl PreparedScopedInlineV1 {
    pub fn matches(&self, owner: &Arc<ScopedInlineOwnerV1>) -> bool {
        Arc::ptr_eq(owner, &self.owner)
            && match (&owner.work.kind, &self.kind) {
                (Kind::Open(intent), ResultKind::Opened(form)) => {
                    matches!(&form.purpose,crate::forms::FormPurpose::PrInline(state)if Arc::ptr_eq(state.intent(),intent))
                }
                (Kind::Preview(request), ResultKind::Preview(preview)) => {
                    Arc::ptr_eq(request.intent(), preview.intent())
                        && request.body().as_ref() == preview.body()
                }
                (Kind::Confirm(_), ResultKind::Published(_)) | (_, ResultKind::Failed { .. }) => {
                    true
                }
                _ => false,
            }
    }
}
fn failure(error: InlineFailureV1) -> ResultKind {
    ResultKind::Failed {
        hide_private: error.hide_private,
        message: error.to_string(),
    }
}
pub fn execute(
    owner: Arc<ScopedInlineOwnerV1>,
    provider: &GithubProvider,
    database: &Path,
    drafts: &crate::drafts::DraftClient,
) -> PreparedScopedInlineV1 {
    let kind = match &owner.work.kind {
        #[cfg(test)]
        Kind::ActualWorkerPanic(_) => panic!("ACTUAL_INLINE_WORKER_PANIC_PRIVATE_POISON"),
        Kind::Open(intent) => match ScopedInlineDraftV1::open(
            intent.clone(),
            provider,
            database,
            &Default::default(),
            &Default::default(),
            &owner.cancel,
            owner.deadline,
        ) {
            Ok(draft) => match ui_v1::form_from_draft(draft) {
                Ok(form) => ResultKind::Opened(Box::new(form)),
                Err(message) => ResultKind::Failed {
                    hide_private: true,
                    message,
                },
            },
            Err(error) => failure(error),
        },
        Kind::Preview(request) => match ScopedInlinePreviewV1::prepare(
            request.intent().clone(),
            request.body().clone(),
            provider,
            database,
            &Default::default(),
            &Default::default(),
            &owner.cancel,
            owner.deadline,
        ) {
            Ok(preview) => ResultKind::Preview(preview),
            Err(error) => failure(error),
        },
        Kind::Confirm(request) => match request.preview() {
            Some(preview) => match preview.execute(
                provider,
                database,
                &Default::default(),
                &Default::default(),
                drafts,
                true,
                None,
                &owner.cancel,
                owner.deadline,
            ) {
                Ok(result) => ResultKind::Published(result),
                Err(error) => failure(error),
            },
            None => ResultKind::Failed {
                hide_private: false,
                message: "Fresh exact Preview required; nothing invoked".into(),
            },
        },
    };
    PreparedScopedInlineV1 { owner, kind }
}
pub fn retire_private(app: &mut App, background: &mut crate::runtime::Background) {
    use crate::active_remote_scope_v1::{RemoteScopeFenceV1, RemoteScopeStopV1};
    if let Some(scope) = &mut app.remote_scope {
        Arc::make_mut(scope).fence(RemoteScopeFenceV1::SourceUnavailable)
    }
    crate::remote_scope_gate_v1::retire_remote(
        app,
        background,
        crate::remote_scope_gate_v1::ScopeEntryStopV1::Intent(RemoteScopeStopV1::Fenced(
            RemoteScopeFenceV1::SourceUnavailable,
        )),
    );
    app.scoped_inline_owner_v1 = None;
    app.scoped_inline_work_v1 = None;
    app.scoped_inline_cancel_v1 = false;
}
pub fn fail(app: &mut App, background: &mut crate::runtime::Background) {
    retire_private(app, background);
    app.status="Original inline result unavailable • draft/journal preserved • nothing resent; refresh explicitly".into();
}
pub fn publish(
    app: &mut App,
    background: &mut crate::runtime::Background,
    owner: &Arc<ScopedInlineOwnerV1>,
    result: PreparedScopedInlineV1,
) {
    match result.kind {
        ResultKind::Opened(form) => {
            app.form = Some(*form);
            app.form_dismissed = false;
            app.overlay = Some(Overlay::Form);
            app.status =
                "Original inline draft opened • fresh Preview required • nothing published".into();
        }
        ResultKind::Preview(preview) => {
            if let (Some(form), Some(request)) = (&mut app.form, owner.work.request()) {
                if !ui_v1::accept_preview(form, request, preview) {
                    form.invalidate_plan();
                    form.error =
                        "Inline preview presentation refused • original draft retained".into();
                }
                form.submitting = false;
            }
            app.status = "Fresh original inline preview ready • explicit Confirm required".into();
        }
        ResultKind::Failed {
            hide_private,
            message,
        } => {
            if hide_private || owner.work.phase() == ScopedInlinePhaseV1::Confirm {
                retire_private(app, background);
                app.status="Original inline source unavailable • private views hidden • draft/journal preserved • nothing resent".into();
            } else if let Some(form) = &mut app.form {
                form.submitting = false;
                form.invalidate_plan();
                form.error = message;
                app.status =
                    "Inline preview refused • draft retained • explicitly re-anchor if needed"
                        .into();
            } else {
                let message = if owner.work.intent().side() == crate::providers::gh::DiffSide::Left
                    && owner.work.intent().validate_endpoint().is_err()
                {
                    "LEFT endpoint must be deleted; choose RIGHT for context. No side converted."
                        .into()
                } else {
                    message
                };
                notice(app, &message);
            }
        }
        ResultKind::Published(result) => {
            let id = result.confirmed_comment_id();
            let state = result.state();
            if result.hide_private() || result.source_retired() {
                retire_private(app, background);
            }
            app.status=match state {
                ScopedInlinePublicationStateV1::ConfirmedSnapshotRetired=>id.filter(|id|*id>0).map_or_else(||"Comment posted; confirmed ID unavailable • refresh explicitly".into(),|id|format!("Comment posted #{id}; original snapshot retired—refresh explicitly")),
                ScopedInlinePublicationStateV1::NotStarted=>"Inline publication not started • original pending record/draft retained • nothing resent; refresh explicitly".into(),
                ScopedInlinePublicationStateV1::Uncertain=>"Inline outcome unknown • original draft/protection retained • nothing resent; refresh explicitly".into(),
                ScopedInlinePublicationStateV1::ConfirmedButJournalFailed=>"Comment posted; journal confirmation unavailable • protection retained • refresh explicitly".into(),
                ScopedInlinePublicationStateV1::ConfirmedButSourceUnavailable=>"Comment posted; current source unavailable • private views hidden • refresh explicitly".into(),
            };
        }
    }
    app.loading[View::Inbox.index()] = false;
    app.hits.clear();
    if app
        .scoped_inline_owner_v1
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, owner))
    {
        app.scoped_inline_owner_v1 = None;
    }
}
