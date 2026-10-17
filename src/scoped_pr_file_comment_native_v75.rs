//! Closed FILE work inside the existing Thread operation/Inbox result.
use super::*;
use file_comments_v75::{
    self as files, form as composer_v75, FileCommentDraftV75, FileCommentIntentV75,
    FileCommentPreviewV75, FileCommentPublicationV75,
};
pub(super) enum FileWorkV75 {
    Open(Arc<FileCommentIntentV75>),
    Preview(Arc<composer_v75::FileCommentRequestV75>),
    Confirm(Arc<composer_v75::FileCommentRequestV75>),
}
pub(super) enum PreparedFileWorkV75 {
    Preview(Arc<FileCommentPreviewV75>),
    Published(FileCommentPublicationV75),
}
impl FileWorkV75 {
    pub(super) fn phase(&self) -> ScopedThreadPhaseV1 {
        match self {
            Self::Open(_) => ScopedThreadPhaseV1::Open,
            Self::Preview(_) => ScopedThreadPhaseV1::Preview,
            Self::Confirm(_) => ScopedThreadPhaseV1::Confirm,
        }
    }
    pub(super) fn request(&self) -> Option<&Arc<composer_v75::FileCommentRequestV75>> {
        match self {
            Self::Open(_) => None,
            Self::Preview(r) | Self::Confirm(r) => Some(r),
        }
    }
    fn intent(&self) -> &Arc<FileCommentIntentV75> {
        match self {
            Self::Open(i) => i,
            Self::Preview(r) | Self::Confirm(r) => r.intent(),
        }
    }
    pub(super) fn presentation_matches(&self, app: &App) -> bool {
        self.intent().owns(app)
            && match self {
                Self::Open(intent) => {
                    app.overlay.is_none() && intent.opening_presentation_owns(app)
                }
                Self::Preview(r) | Self::Confirm(r) => {
                    app.overlay == Some(Overlay::Form)
                        && app.form.as_ref().is_some_and(|f| r.matches_form(f))
                }
            }
    }
    pub(super) fn accepts(&self, result: &ResultV1) -> bool {
        match (self, result) {
            (Self::Open(intent), ResultV1::Opened(form)) => {
                matches!(&form.purpose,crate::forms::FormPurpose::PrFileCommentV75(s) if Arc::ptr_eq(s.intent(),intent))
            }
            (Self::Preview(r), ResultV1::FileCommentV75(PreparedFileWorkV75::Preview(p))) => {
                Arc::ptr_eq(r.intent(), p.intent()) && r.body().as_ref() == p.body()
            }
            (Self::Confirm(r), ResultV1::FileCommentV75(PreparedFileWorkV75::Published(p))) => {
                let (view, _) = r.intent().observed();
                p.reference().attempt_id > 0
                    && p.reference().pr_node_id == view.observed.parent().node_id()
                    && p.reference().observed_head == view.observed.parent().head_sha()
                    && r.preview().is_some_and(|preview| {
                        p.reference().payload_sha256 == preview.payload_sha256()
                    })
            }
            (_, ResultV1::Failed { .. }) => true,
            _ => false,
        }
    }
    pub(super) fn execute(
        &self,
        owner: &Arc<ScopedThreadsOwnerV1>,
        provider: &GithubProvider,
        database: &Path,
        drafts: Option<&crate::drafts::DraftClient>,
    ) -> ResultV1 {
        let failed = |e: files::FileCommentFailureV75| ResultV1::Failed {
            hide_private: e.hide_private,
            message: e.message().into(),
        };
        match self {
            Self::Open(intent)=>match FileCommentDraftV75::open(intent.clone(),provider,database,&owner.cancel,owner.deadline){
                Ok(draft)=>match composer_v75::form_from_draft(draft){Ok(form)=>ResultV1::Opened(Box::new(form)),Err(message)=>ResultV1::Failed{hide_private:false,message}},Err(e)=>failed(e),
            },
            Self::Preview(r)=>match FileCommentPreviewV75::prepare(r.intent().clone(),r.body().clone(),provider,database,&owner.cancel,owner.deadline){
                Ok(p)=>ResultV1::FileCommentV75(PreparedFileWorkV75::Preview(p)),Err(e)=>failed(e),
            },
            Self::Confirm(r)=>match(r.preview(),drafts,owner.publication.lock().ok().and_then(|mut p|p.take())){
                (Some(p),Some(drafts),Some(publication))=>match p.execute(provider,database,drafts,publication,true,None,&owner.cancel,owner.deadline){
                    Ok(p)=>ResultV1::FileCommentV75(PreparedFileWorkV75::Published(p)),Err(e)=>failed(e),
                },_=>ResultV1::Failed{hide_private:false,message:"Fresh FILE Preview/original bounded draft writer required • nothing invoked".into()},
            },
        }
    }
}
pub(crate) fn open(app: &mut App) {
    if app.loading[View::Inbox.index()]
        || app.scoped_threads_owner_v1.is_some()
        || app.scoped_threads_work_v1.is_some()
        || newer_intent(app)
    {
        return;
    }
    match FileCommentIntentV75::capture(app).and_then(|i| {
        ScopedThreadsOwnerV1::new(
            i.original.clone(),
            WorkV1::FileCommentV75(FileWorkV75::Open(i)),
        )
    }) {
        Ok(owner) if owner.presentation_matches(app) => {
            app.scoped_threads_work_v1 = Some(owner);
            app.status="Opening original FILE comment draft • current source/account/member will be verified".into();
        }
        Ok(_) => {}
        Err(e) => app.status = e,
    }
}
pub(super) fn capture_form(
    app: &mut App,
    original_drafts: Option<(&Path, &crate::drafts::DraftClient)>,
) {
    if app.scoped_threads_work_v1.is_some() || app.scoped_threads_owner_v1.is_some() {
        return;
    }
    let Some(form) = app
        .form
        .as_ref()
        .filter(|f| matches!(f.purpose, crate::forms::FormPurpose::PrFileCommentV75(_)))
    else {
        return;
    };
    if form.submitting || (!form.preview_requested && !form.submit_requested) {
        return;
    }
    if crate::scoped_pr_observed_tabs_v1::resources::immutable_images::inline_v1::native_v1::newer_intent_for_original_file_form_v82(app, form) {
        return;
    }
    let confirm = form.submit_requested;
    let captured = if confirm {
        composer_v75::confirmed_request_v82(form)
    } else {
        composer_v75::request(form)
    };
    let form = app
        .form
        .as_mut()
        .expect("Exact actual FILE Form checked before any effect");
    form.preview_requested = false;
    form.submit_requested = false;
    let request = match captured {
        Ok(r) => r,
        Err(e) => {
            form.error = e;
            return;
        }
    };
    form.submitting = true;
    let work = if confirm {
        FileWorkV75::Confirm(request.clone())
    } else {
        FileWorkV75::Preview(request.clone())
    };
    let owner = match ScopedThreadsOwnerV1::new(
        request.intent().original.clone(),
        WorkV1::FileCommentV75(work),
    ) {
        Ok(o) => o,
        Err(e) => {
            form.submitting = false;
            form.error = e;
            return;
        }
    };
    if confirm {
        let Some((database, drafts)) = original_drafts else {
            form.submitting = false;
            form.error = "Original bounded draft writer unavailable • nothing queued".into();
            return;
        };
        let fence = match drafts.capture_publication_v2(
            database,
            request.intent().scope(),
            request.draft_key(),
            &owner.cancel,
            owner.deadline,
        ) {
            Ok(f) => f,
            Err(_) => {
                form.submitting = false;
                form.error =
                    "Original FILE draft writer fence unavailable • no write queued".into();
                return;
            }
        };
        let Ok(mut slot) = owner.publication.lock() else {
            form.submitting = false;
            form.error = "FILE publication ownership unavailable".into();
            return;
        };
        *slot = Some(fence);
    }
    if owner.presentation_matches(app) && !newer_intent(app) {
        app.scoped_threads_work_v1 = Some(owner);
    } else if let Some(form) = &mut app.form {
        request.retire_busy_presentation(form);
    }
}
impl PreparedFileWorkV75 {
    pub(super) fn publish(
        self,
        app: &mut App,
        background: &mut crate::runtime::Background,
        owner: &Arc<ScopedThreadsOwnerV1>,
    ) {
        match self {
            Self::Preview(preview) => {
                if let (Some(request), Some(form)) = (owner.file_request_v75(), &mut app.form) {
                    if !composer_v75::accept_preview(form, request, preview) {
                        form.invalidate_plan();
                        form.error = "FILE Preview presentation refused • draft retained".into();
                    }
                    form.submitting = false;
                }
                app.status="Fresh original whole FILE Preview ready • explicit Confirm required • nothing invoked".into();
            }
            Self::Published(p) => {
                let state = p.state();
                let id = p.confirmed_comment_id();
                let retention = p.draft_retention();
                let retired = p.source_retired();
                let hide = p.hide_private();
                if hide {
                    fail(app, background);
                }
                app.status=match state {
                    files::FileCommentPublicationStateV75::ConfirmedSnapshotRetired=>format!("FILE comment posted #{}; original snapshot retired—refresh explicitly",id.map_or_else(||"unknown".into(),|n|n.to_string())),
                    files::FileCommentPublicationStateV75::NotStarted=>"FILE write not started • protected history retained • draft ACK may have persisted • no resend".into(),
                    files::FileCommentPublicationStateV75::Uncertain=>"FILE outcome unknown • protected history retained • nothing resent; refresh explicitly".into(),
                    files::FileCommentPublicationStateV75::ConfirmedButJournalFailed=>"FILE comment observed • durable confirmation unavailable • protection retained; no resend".into(),
                    files::FileCommentPublicationStateV75::ConfirmedButSourceUnavailable=>"FILE comment observed • current source unavailable • private views hidden; refresh explicitly".into(),
                };
                // Safe status only; never expose receipt body/actor after fence.
                if retention == files::FileDraftRetentionV75::Unknown {
                    app.status.push_str(" • draft storage result unknown");
                }
                if !retired {
                    app.status.push_str(" • freshness retirement unavailable");
                }
            }
        }
    }
}
