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
    sync::{Arc, Mutex},
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
    OpenNative {
        intent: Arc<ScopedInlineIntentV1>,
        origin: Arc<super::native_entry_v1::NativeInlineOriginV1>,
    },
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
            Kind::Open(_) | Kind::OpenNative { .. } => ScopedInlinePhaseV1::Open,
            Kind::Preview(_) => ScopedInlinePhaseV1::Preview,
            Kind::Confirm(_) => ScopedInlinePhaseV1::Confirm,
        }
    }
    pub fn intent(&self) -> &Arc<ScopedInlineIntentV1> {
        match &self.kind {
            #[cfg(test)]
            Kind::ActualWorkerPanic(original) => original.intent(),
            Kind::Open(intent) | Kind::OpenNative { intent, .. } => intent,
            Kind::Preview(request) | Kind::Confirm(request) => request.intent(),
        }
    }
    pub fn request(&self) -> Option<&Arc<ui_v1::InlineComposerRequestV1>> {
        match &self.kind {
            #[cfg(test)]
            Kind::ActualWorkerPanic(original) => original.request(),
            Kind::Open(_) | Kind::OpenNative { .. } => None,
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
            Kind::OpenNative { intent, origin } => {
                origin.matches_open(app) && origin.matches_intent(intent)
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
impl ScopedInlineWorkV1 {
    pub(crate) fn native_origin(
        &self,
    ) -> Option<&Arc<super::native_entry_v1::NativeInlineOriginV1>> {
        match &self.kind {
            Kind::OpenNative { origin, .. } => Some(origin),
            #[cfg(test)]
            Kind::ActualWorkerPanic(original) => original.native_origin(),
            _ => None,
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
    publication_v14: Mutex<Option<crate::drafts::DraftPublicationFenceV2>>,
}
impl std::fmt::Debug for ScopedInlineOwnerV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedInlineOwnerV1(<original queued operation>)")
    }
}
impl ScopedInlineOwnerV1 {
    pub(crate) fn new(
        work: Arc<ScopedInlineWorkV1>,
        provider: &GithubProvider,
    ) -> Result<Arc<Self>, String> {
        let captured = Instant::now()
            .checked_add(Duration::from_secs(30))
            .ok_or("Inline deadline unavailable")?;
        let deadline = provider
            .original_pr_read_deadline_v13()
            .map_or(captured, |original| original.min(captured));
        Ok(Arc::new(Self {
            work,
            cancel: CancellationToken::default(),
            publication_v14: Mutex::new(None),
            deadline,
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
    /// Tiny exact-client capture before the actual Inbox queue or slot changes.
    /// The already-prepared key is borrowed, never hashed/serialized here.
    pub(crate) fn capture_publication_v14(
        &self,
        database: &Path,
        drafts: &crate::drafts::DraftClient,
    ) -> Result<(), String> {
        if self.expired() {
            return Err("Original inline operation expired; nothing queued".into());
        }
        if self.work.phase() != ScopedInlinePhaseV1::Confirm {
            return Ok(());
        }
        let preview = self
            .work
            .request()
            .and_then(|request| request.preview())
            .ok_or("Exact prepared inline Preview required before Confirm")?;
        let fence = drafts
            .capture_publication_v2(
                database,
                self.work.intent().scope(),
                preview.draft_key(),
                &self.cancel,
                self.deadline,
            )
            .map_err(|_| "Original draft revision unavailable; nothing queued".to_owned())?;
        let mut publication = self
            .publication_v14
            .lock()
            .map_err(|_| "Original publication capture unavailable".to_owned())?;
        if publication.is_some() {
            return Err("Original inline publication capture already exists".into());
        }
        *publication = Some(fence);
        Ok(())
    }
    /// Presentation dismissal cannot make a possibly invoked write safe to
    /// display. This is ONLY the exact original observed source/scope, never a
    /// newer chosen source and never permission to read, reconcile or resend.
    pub(crate) fn original_private_source_is_current(&self, app: &App) -> bool {
        let proof = &self.work.intent().proof;
        proof.source.matches_source(app)
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
        crate::pr_read_origin_v13::PrReadOriginV13,
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
    newer_intent_allowing_work(app, None)
}
pub(crate) fn newer_intent_allowing_work(app: &App, own: Option<&Arc<ScopedInlineWorkV1>>) -> bool {
    app.form
        .as_ref()
        .is_some_and(|form| form.preview_requested || form.submit_requested)
        || newer_intent_without_form_requests_v82(app, own)
}
/// Only the actual currently installed FILE Form may exclude its own request
/// bits from the foreign-intent check. This is not a generic ignore flag or
/// permission to consume another Form/source/owner's pending input.
pub(crate) fn newer_intent_for_original_file_form_v82(
    app: &App,
    form: &crate::forms::Form,
) -> bool {
    let exact = app.view == View::Inbox
        && app.overlay == Some(Overlay::Form)
        && !app.form_dismissed
        && !app.loading[View::Inbox.index()]
        && !form.editor_requested
        && app
            .form
            .as_ref()
            .is_some_and(|current| std::ptr::eq(current, form))
        && matches!(&form.purpose, crate::forms::FormPurpose::PrFileCommentV75(state) if state.intent().owns(app));
    !exact
        || app.scoped_inline_owner_v1.is_some()
        || app.scoped_reviews_owner_v2.is_some()
        || app.scoped_threads_owner_v1.is_some()
        || app.pr_native_diff_owner_v1.is_some()
        || app.scoped_actions_work_v1.is_some()
        || app.scoped_actions_owner_v1.is_some()
        || app.scoped_actions_cancel_v1
        || app.pr_check_continuation_request_v2.is_some()
        || app.pr_check_continuation_owner_v2.is_some()
        || app.pr_check_continuation_cancel_v2
        || app.personal_pr_entry_work_v13.is_some()
        || app.personal_pr_entry_owner_v13.is_some()
        || app.personal_pr_entry_cancel_v13
        || newer_intent_without_form_requests_v82(app, None)
}
fn newer_intent_without_form_requests_v82(
    app: &App,
    own: Option<&Arc<ScopedInlineWorkV1>>,
) -> bool {
    if app.notifications_work_v63.is_some() || app.notifications_cancel_v63 {
        return true;
    }
    if app.scoped_reviews_work_v2.is_some() || app.scoped_reviews_cancel_v2 {
        return true;
    }
    if app.scoped_threads_work_v1.is_some() || app.scoped_threads_cancel_v1 {
        return true;
    }
    app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app
            .scoped_inline_work_v1
            .as_ref()
            .is_some_and(|queued| own.is_none_or(|own| !Arc::ptr_eq(queued, own)))
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
pub(crate) fn enqueue_native(
    app: &mut App,
    intent: Arc<ScopedInlineIntentV1>,
    origin: Arc<super::native_entry_v1::NativeInlineOriginV1>,
) {
    let work = Arc::new(ScopedInlineWorkV1 {
        kind: Kind::OpenNative { intent, origin },
    });
    if work.matches(app) && !newer_intent(app) {
        app.scoped_inline_work_v1 = Some(work);
        app.pr_native_diff_layout_v1.clear();
        app.hits.clear();
    }
}
pub fn open(app: &mut App) {
    if app.scoped_inline_owner_v1.is_some()
        || app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app.scoped_inline_work_v1.is_some()
        || app.scoped_reviews_work_v2.is_some()
        || app.scoped_reviews_cancel_v2
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
                (Kind::OpenNative { intent, origin }, ResultKind::Opened(form)) => {
                    matches!(&form.purpose,crate::forms::FormPurpose::PrInline(state)if Arc::ptr_eq(state.intent(),intent)&&state.native_origin_v1.as_ref().is_some_and(|actual|Arc::ptr_eq(actual,origin)))
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
        Kind::Open(intent) | Kind::OpenNative { intent, .. } => match ScopedInlineDraftV1::open(
            intent.clone(),
            provider,
            database,
            &Default::default(),
            &Default::default(),
            &owner.cancel,
            owner.deadline,
        ) {
            Ok(draft) => match ui_v1::form_from_draft(draft) {
                Ok(mut form) => {
                    if let Some(origin) = owner.work.native_origin() {
                        ui_v1::bind_native_origin(&mut form, origin.clone());
                    }
                    ResultKind::Opened(Box::new(form))
                }
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
        Kind::Confirm(request) => {
            let publication = owner
                .publication_v14
                .lock()
                .ok()
                .and_then(|mut slot| slot.take());
            match (request.preview(), publication) {
                (Some(preview), Some(publication)) => match preview.execute_with_publication_v14(
                    provider, database, &Default::default(), &Default::default(), drafts,
                    publication, true, None, &owner.cancel, owner.deadline,
                ) {
                    Ok(result) => ResultKind::Published(result),
                    Err(error) => failure(error),
                },
                _ => ResultKind::Failed {
                    hide_private: false,
                    message: "Fresh exact Preview and original prequeue draft capture required; nothing invoked".into(),
                },
            }
        }
    };
    PreparedScopedInlineV1 { owner, kind }
}
pub fn retire_private(app: &mut App, background: &mut crate::runtime::Background) {
    use crate::active_remote_scope_v1::{RemoteScopeFenceV1, RemoteScopeStopV1};
    // The caller has already admitted the exact original owner/source. A
    // terminal publication/source failure retires the retained Native frame
    // now, not on a later draw; it cannot become read-only Back authority.
    app.pr_native_diff_v1 = None;
    app.pr_native_diff_layout_v1 = Default::default();
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
/// A generic Inbox replacement may have retired the old physical owner after
/// invocation. Drop only exact old private presentation, NEVER the successor
/// slot/token/generation/loading/status or a newer source/navigation/Form.
pub(crate) fn retire_orphaned_confirm_presentation_v14(app: &mut App, owner: &ScopedInlineOwnerV1) {
    if owner.work.phase() != ScopedInlinePhaseV1::Confirm {
        return;
    }
    let Some(request) = owner.work.request() else {
        return;
    };
    let old_form = app
        .form
        .as_ref()
        .is_some_and(|form| request.owns_presentation_v14(form));
    let old_form_overlay = old_form && app.overlay == Some(Overlay::Form);
    let proof = &owner.work.intent().proof;
    // A newer Notice/Commands overlay does not transfer the original private
    // underlay. Snapshot every exact ownership predicate before any retirement
    // (including the Personal keepalive marker), preserving that newer overlay.
    let exact_private = old_form && !app.form_dismissed && proof.owns(app);
    let old_images = app
        .pr_file_images
        .as_ref()
        .is_some_and(|view| Arc::ptr_eq(view.original_proof_v1(), proof));
    let old_native = app
        .pr_native_diff_v1
        .as_ref()
        .and_then(|view| view.source())
        .is_some_and(|source| Arc::ptr_eq(source.proof(), proof));
    let old_checks = app
        .pr_checks_origin
        .as_ref()
        .and_then(|origin| origin.scoped_owner_v1())
        .is_some_and(|(source, active, nav)| {
            proof.source.same_catalogue_source(source)
                && Arc::ptr_eq(active, &proof.active)
                && Arc::ptr_eq(nav, &proof.navigation)
        });
    let parent = proof.observed().parent();
    let old_reactions = app.pr_reactions.as_ref().is_some_and(|page| {
        crate::storage::Scope::github(&page.identity.host, page.identity.account_id)
            .ok()
            .as_ref()
            == Some(proof.source_scope())
            && page.target.pr.id == parent.node_id()
            && page.target.pr.number == parent.number()
            && page
                .target
                .pr
                .repository
                .eq_ignore_ascii_case(parent.repository())
    });
    let can_retire = exact_private
        && (!proof.source.is_personal()
            || crate::personal_pr_entry_v13::retain_orphan_source_skeleton_v14(
                app,
                &proof.source,
                &proof.navigation,
            ));
    if can_retire {
        if let Some(github) = &mut app.github {
            github.pr = None;
            github.scoped_pr_resource = None;
        }
        if old_images {
            app.pr_file_images = None;
        }
        if old_native {
            app.pr_native_diff_v1 = None;
            app.pr_native_diff_layout_v1 = Default::default();
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
        app.pr_pending_message.clear();
        // No global avatar shutdown/map clear: a newer surface may share it.
    }
    if old_form {
        app.form = None;
        app.form_dismissed = true;
        if old_form_overlay {
            app.overlay = None;
            app.hits.clear();
        }
    }
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
            } else if owner.work.native_origin().is_some() {
                app.dispatch(crate::app::Action::Explain(message));
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
