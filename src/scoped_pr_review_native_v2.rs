//! Native scheduling and closed results on the SAME original Inbox queue.
//! No read-only range, restored metadata, Form ticket or Preview grants writes.
use super::*;
use crate::{
    app::{Action, Overlay},
    drafts::{DraftClient, DraftPublicationFenceV2},
    providers::gh::GithubProvider,
};
use browser_v2::{ReviewBrowserActionV2, ReviewBrowserV2};
use crossterm::event::{Event, KeyEventKind};
use operations_v3::{
    history_v3::{self, PreparedReviewOperationHistoryV3, ReviewOperationHistoryIntentV3},
    preview_v3::ReviewOperationPreviewV3,
    ui_v3 as operation_ui_v3, ScopedReviewOperationIntentV3, ScopedReviewOperationSourceV3,
};
use std::{path::Path, sync::Mutex, time::Duration};
#[path = "scoped_pr_review_work_origin_v3.rs"]
mod work_origin_v3;
pub use work_origin_v3::ReviewWorkOriginV3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedReviewsPhaseV2 {
    Open,
    Read,
    Compose,
    Preview,
    Confirm,
}
enum Kind {
    NewActionV12 {
        notice: Arc<action_lineage_v12::NeedsNewActionV12>,
        ticket: Arc<()>,
    },
    #[cfg(test)]
    ActualWorkerPanic(Arc<ScopedReviewsWorkV2>),
    Open {
        intent: Arc<ScopedReviewIntentV2>,
        anchor: Option<Arc<ScopedReviewAnchorIntentV2>>,
    },
    Read {
        draft: Arc<ScopedReviewDraftV2>,
        row: usize,
        ticket: Arc<()>,
    },
    Compose {
        draft: Arc<ScopedReviewDraftV2>,
        selection: Option<Arc<ScopedReviewSelectionV2>>,
        action: ReviewRequestedActionV2,
        ticket: Arc<()>,
    },
    Preview(Arc<ui_v2::ReviewComposerRequestV2>),
    Confirm(Arc<ui_v2::ReviewComposerRequestV2>),
    OperationOpen {
        intent: Arc<ScopedReviewOperationIntentV3>,
        ticket: Arc<()>,
    },
    OperationAdd {
        selection: Arc<ScopedReviewSelectionV2>,
        anchor: Arc<ScopedReviewAnchorIntentV2>,
        ticket: Arc<()>,
    },
    OperationPreview(Arc<operation_ui_v3::ReviewOperationComposerRequestV3>),
    OperationConfirm(Arc<operation_ui_v3::ReviewOperationComposerRequestV3>),
    HistoryList {
        intent: Arc<ReviewOperationHistoryIntentV3>,
        ticket: Option<Arc<()>>,
    },
    HistoryOpen {
        listed: Arc<PreparedReviewOperationHistoryV3>,
        row: usize,
        ticket: Arc<()>,
    },
    RecoveryInspectV11 {
        request: Arc<history_v3::recovery_v11::ReviewRecoveryRequestV11>,
        ticket: Arc<()>,
    },
    RecoveryAcknowledgeV11 {
        prepared: Arc<history_v3::recovery_v11::PreparedReviewRecoveryV11>,
        ticket: Arc<()>,
    },
    TeamsV3 {
        draft: Arc<ScopedReviewDraftV2>,
        ticket: Arc<()>,
    },
}
pub struct ScopedReviewsWorkV2 {
    kind: Kind,
    cancel: CancellationToken,
    deadline: Instant,
}
impl std::fmt::Debug for ScopedReviewsWorkV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedReviewsWorkV2(<original queued intent>)")
    }
}
impl ScopedReviewsWorkV2 {
    #[cfg(test)]
    pub(crate) fn cancellation_v15(&self) -> &CancellationToken {
        &self.cancel
    }
    #[cfg(test)]
    pub(crate) fn original_deadline_v15(&self) -> Instant {
        self.deadline
    }
    fn new(kind: Kind) -> Result<Arc<Self>, String> {
        let initial_summary = match &kind {
            Kind::HistoryList {
                intent,
                ticket: None,
            } => intent.personal_summary_v15(),
            _ => None,
        };
        let deadline = Instant::now()
            .checked_add(Duration::from_secs(30))
            .ok_or("Original review clock unavailable")?;
        let deadline = match &kind {
            Kind::NewActionV12 { notice, .. } => deadline.min(notice.deadline),
            Kind::RecoveryAcknowledgeV11 { prepared, .. } => deadline.min(prepared.deadline()),
            _ => initial_summary.map_or(deadline, |original| deadline.min(original.deadline())),
        };
        let cancel = initial_summary.map_or_else(CancellationToken::default, |original| {
            original.cancellation().clone()
        });
        Ok(Arc::new(Self {
            kind,
            cancel,
            deadline,
        }))
    }
    pub fn expired(&self) -> bool {
        self.cancel.is_cancelled() || Instant::now() >= self.deadline
    }
    pub fn phase(&self) -> ScopedReviewsPhaseV2 {
        match self.kind {
            Kind::NewActionV12 { .. } => ScopedReviewsPhaseV2::Compose,
            #[cfg(test)]
            Kind::ActualWorkerPanic(ref original) => original.phase(),
            Kind::Open { .. } => ScopedReviewsPhaseV2::Open,
            Kind::Read { .. } => ScopedReviewsPhaseV2::Read,
            Kind::Compose { .. } | Kind::OperationOpen { .. } | Kind::OperationAdd { .. } => {
                ScopedReviewsPhaseV2::Compose
            }
            Kind::Preview(_) | Kind::OperationPreview(_) => ScopedReviewsPhaseV2::Preview,
            Kind::Confirm(_) | Kind::OperationConfirm(_) => ScopedReviewsPhaseV2::Confirm,
            Kind::HistoryList { .. } => ScopedReviewsPhaseV2::Open,
            Kind::HistoryOpen { .. }
            | Kind::TeamsV3 { .. }
            | Kind::RecoveryInspectV11 { .. }
            | Kind::RecoveryAcknowledgeV11 { .. } => ScopedReviewsPhaseV2::Read,
        }
    }
    pub fn origin(&self) -> ReviewWorkOriginV3<'_> {
        match &self.kind {
            Kind::NewActionV12 { notice, .. } => ReviewWorkOriginV3::Files(notice.intent()),
            #[cfg(test)]
            Kind::ActualWorkerPanic(original) => original.origin(),
            Kind::Open { intent, .. } => ReviewWorkOriginV3::Files(intent),
            Kind::Read { draft, .. } | Kind::Compose { draft, .. } => {
                ReviewWorkOriginV3::Files(draft.intent())
            }
            Kind::Preview(request) | Kind::Confirm(request) => {
                ReviewWorkOriginV3::Files(request.request().intent())
            }
            Kind::OperationOpen { intent, .. } => ReviewWorkOriginV3::Files(intent.original()),
            Kind::OperationAdd { selection, .. } => ReviewWorkOriginV3::Files(selection.intent()),
            Kind::OperationPreview(request) | Kind::OperationConfirm(request) => {
                ReviewWorkOriginV3::Files(request.request().intent().original())
            }
            Kind::HistoryList { intent, .. } => ReviewWorkOriginV3::Summary(intent),
            Kind::HistoryOpen { listed, .. } => ReviewWorkOriginV3::Summary(listed.intent()),
            Kind::RecoveryInspectV11 { request, .. } => {
                ReviewWorkOriginV3::Summary(request.intent())
            }
            Kind::RecoveryAcknowledgeV11 { prepared, .. } => {
                ReviewWorkOriginV3::Summary(prepared.request().intent())
            }
            Kind::TeamsV3 { draft, .. } => ReviewWorkOriginV3::Files(draft.intent()),
        }
    }
    pub fn scope(&self) -> &Scope {
        self.origin().scope()
    }
    pub fn operation_request_v3(
        &self,
    ) -> Option<&Arc<operation_ui_v3::ReviewOperationComposerRequestV3>> {
        match &self.kind {
            Kind::OperationPreview(request) | Kind::OperationConfirm(request) => Some(request),
            _ => None,
        }
    }
    pub(crate) fn retire_form_busy_v3(&self, form: &mut crate::forms::Form) {
        if let Some(request) = self.request() {
            request.retire_busy(form);
        }
        if let Some(request) = self.operation_request_v3() {
            request.retire_busy(form);
        }
    }
    pub fn request(&self) -> Option<&Arc<ui_v2::ReviewComposerRequestV2>> {
        match &self.kind {
            #[cfg(test)]
            Kind::ActualWorkerPanic(original) => original.request(),
            Kind::Preview(request) | Kind::Confirm(request) => Some(request),
            _ => None,
        }
    }
    pub fn matches(&self, app: &App) -> bool {
        if !self.origin().owns(app) {
            return false;
        }
        if app.scoped_review_history_v3.is_some() && app.scoped_reviews_v2.is_some() {
            return false;
        }
        if app.scoped_review_history_v3.is_some()
            && !matches!(
                self.kind,
                Kind::HistoryList { .. }
                    | Kind::HistoryOpen { .. }
                    | Kind::RecoveryInspectV11 { .. }
                    | Kind::RecoveryAcknowledgeV11 { .. }
            )
        {
            return false;
        }
        match &self.kind {
            #[cfg(test)]
            Kind::ActualWorkerPanic(original) => original.matches(app),
            Kind::NewActionV12 { notice, ticket } => {
                app.overlay == Some(Overlay::ScopedReviewsV2)
                    && app.scoped_reviews_v2.as_ref().is_some_and(|view| {
                        Arc::ptr_eq(&view.ticket, ticket)
                            && view
                                .new_action_v12
                                .as_ref()
                                .is_some_and(|current| Arc::ptr_eq(current, notice))
                    })
            }
            Kind::Open { anchor, .. } => match anchor {
                Some(anchor) => {
                    app.overlay == Some(Overlay::PrFileImages) && anchor.matches_current_range(app)
                }
                None => app.overlay.is_none(),
            },
            Kind::Read { draft, row, ticket } => {
                app.overlay == Some(Overlay::ScopedReviewsV2)
                    && app.scoped_reviews_v2.as_ref().is_some_and(|view| {
                        Arc::ptr_eq(draft, &view.draft)
                            && *row == view.selected()
                            && Arc::ptr_eq(ticket, &view.ticket)
                    })
            }
            Kind::Compose {
                draft,
                selection,
                ticket,
                ..
            } => {
                app.overlay == Some(Overlay::ScopedReviewsV2)
                    && app.scoped_reviews_v2.as_ref().is_some_and(|view| {
                        Arc::ptr_eq(draft, &view.draft)
                            && Arc::ptr_eq(ticket, &view.ticket)
                            && match (selection, &view.selection) {
                                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                                (None, _) => true,
                                _ => false,
                            }
                    })
            }
            Kind::Preview(request) | Kind::Confirm(request) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app
                        .form
                        .as_ref()
                        .is_some_and(|form| request.matches_form(form))
                    && app.scoped_review_history_v3.is_none()
            }
            Kind::OperationOpen { intent, ticket } => {
                app.scoped_review_history_v3.is_none()
                    && app.overlay == Some(Overlay::ScopedReviewsV2)
                    && app.scoped_reviews_v2.as_ref().is_some_and(|view| {
                        Arc::ptr_eq(ticket, &view.ticket)
                            && Arc::ptr_eq(intent.original(), view.intent())
                            && intent.matches_browser_v3(view)
                    })
            }
            Kind::OperationAdd {
                selection,
                anchor,
                ticket,
            } => {
                app.scoped_review_history_v3.is_none()
                    && app.overlay == Some(Overlay::ScopedReviewsV2)
                    && app.scoped_reviews_v2.as_ref().is_some_and(|view| {
                        Arc::ptr_eq(ticket, &view.ticket)
                            && view
                                .selection
                                .as_ref()
                                .is_some_and(|current| Arc::ptr_eq(current, selection))
                            && view
                                .pending_anchor_v3
                                .as_ref()
                                .is_some_and(|current| Arc::ptr_eq(current, anchor))
                    })
                    && anchor.owns(app)
            }
            Kind::OperationPreview(request) | Kind::OperationConfirm(request) => {
                app.scoped_review_history_v3.is_none()
                    && !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app
                        .form
                        .as_ref()
                        .is_some_and(|form| request.matches_form(form))
            }
            Kind::HistoryList { intent, ticket } => {
                app.scoped_reviews_v2.is_none()
                    && match ticket {
                        None => app.overlay.is_none() && app.scoped_review_history_v3.is_none(),
                        Some(ticket) => {
                            app.overlay == Some(Overlay::ScopedReviewsV2)
                                && app.scoped_review_history_v3.as_ref().is_some_and(|view| {
                                    Arc::ptr_eq(view.intent(), intent)
                                        && Arc::ptr_eq(&view.ticket, ticket)
                                })
                        }
                    }
            }
            Kind::HistoryOpen {
                listed,
                row,
                ticket,
            } => {
                app.scoped_reviews_v2.is_none()
                    && app.overlay == Some(Overlay::ScopedReviewsV2)
                    && app.scoped_review_history_v3.as_ref().is_some_and(|view| {
                        Arc::ptr_eq(&view.listed, listed)
                            && view.selected == *row
                            && Arc::ptr_eq(&view.ticket, ticket)
                    })
            }
            Kind::RecoveryInspectV11 { request, ticket } => {
                app.scoped_reviews_v2.is_none()
                    && app.overlay == Some(Overlay::ScopedReviewsV2)
                    && app.scoped_review_history_v3.as_ref().is_some_and(|view| {
                        view.selected == request.row()
                            && view
                                .opened
                                .as_ref()
                                .is_some_and(|opened| Arc::ptr_eq(opened, request.history()))
                            && Arc::ptr_eq(&view.ticket, ticket)
                    })
            }
            Kind::RecoveryAcknowledgeV11 { prepared, ticket } => {
                app.scoped_reviews_v2.is_none()
                    && app.overlay == Some(Overlay::ScopedReviewsV2)
                    && app.scoped_review_history_v3.as_ref().is_some_and(|view| {
                        view.selected == prepared.request().row()
                            && view.opened.as_ref().is_some_and(|opened| {
                                Arc::ptr_eq(opened, prepared.request().history())
                            })
                            && view
                                .recovery_v11
                                .as_ref()
                                .is_some_and(|current| Arc::ptr_eq(current, prepared))
                            && Arc::ptr_eq(&view.ticket, ticket)
                    })
            }
            Kind::TeamsV3 { draft, ticket } => {
                app.scoped_review_history_v3.is_none()
                    && app.overlay == Some(Overlay::ScopedReviewsV2)
                    && app.scoped_reviews_v2.as_ref().is_some_and(|view| {
                        Arc::ptr_eq(&view.draft, draft) && Arc::ptr_eq(&view.ticket, ticket)
                    })
            }
        }
    }
}
#[cfg(test)]
pub(crate) fn actual_worker_panic_for_test(
    original: Arc<ScopedReviewsWorkV2>,
) -> Arc<ScopedReviewsWorkV2> {
    Arc::new(ScopedReviewsWorkV2 {
        cancel: original.cancel.clone(),
        deadline: original.deadline,
        kind: Kind::ActualWorkerPanic(original),
    })
}
#[cfg(test)]
pub(crate) fn expire_original_work_for_test(work: &mut Arc<ScopedReviewsWorkV2>, cancel: bool) {
    if cancel {
        work.cancel.cancel();
    } else {
        Arc::get_mut(work)
            .expect("sole genuine captured work")
            .deadline = Instant::now() - Duration::from_millis(1);
    }
}
pub struct ScopedReviewsOwnerV2 {
    work: Arc<ScopedReviewsWorkV2>,
    publication: Mutex<Option<DraftPublicationFenceV2>>,
}
impl std::fmt::Debug for ScopedReviewsOwnerV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedReviewsOwnerV2(<original queued operation>)")
    }
}
impl ScopedReviewsOwnerV2 {
    pub(crate) fn new(
        work: Arc<ScopedReviewsWorkV2>,
        database: &Path,
        drafts: &DraftClient,
    ) -> Result<Arc<Self>, String> {
        let publication_key = match &work.kind {
            Kind::Confirm(request) => Some(request.draft_key()),
            Kind::OperationConfirm(request) => Some(request.draft_key()),
            _ => None,
        };
        let publication = if let Some(key) = publication_key {
            Some(
                drafts
                    .capture_publication_v2(
                        database,
                        work.scope(),
                        key,
                        &work.cancel,
                        work.deadline,
                    )
                    .map_err(|error| error.to_string())?,
            )
        } else {
            None
        };
        Ok(Arc::new(Self {
            work,
            publication: Mutex::new(publication),
        }))
    }
    pub fn work(&self) -> &Arc<ScopedReviewsWorkV2> {
        &self.work
    }
    pub fn cancellation(&self) -> &CancellationToken {
        &self.work.cancel
    }
    pub fn deadline(&self) -> Instant {
        self.work.deadline
    }
    pub fn expired(&self) -> bool {
        self.cancellation().is_cancelled() || Instant::now() >= self.deadline()
    }
    pub fn owns(self: &Arc<Self>, app: &App) -> bool {
        app.scoped_reviews_owner_v2
            .as_ref()
            .is_some_and(|owner| Arc::ptr_eq(owner, self))
            && self.work.matches(app)
            && !newer_intent(app)
    }
    pub(crate) fn original_private_source_is_current(&self, app: &App) -> bool {
        self.work.origin().owns(app)
    }
    pub(crate) fn remote_owners(
        &self,
    ) -> Option<(
        Arc<ActiveRemoteScopeV1>,
        crate::pr_read_origin_v13::PrReadOriginV13,
        Arc<ScopedPrNavigationV1>,
    )> {
        let original = self.work.origin();
        Some((
            original.active()?.clone(),
            original.source()?,
            original.navigation()?.clone(),
        ))
    }
}
pub fn newer_intent(app: &App) -> bool {
    if app.notifications_work_v63.is_some()
        || app.notifications_cancel_v63
        || app.scoped_threads_work_v1.is_some()
        || app.scoped_threads_cancel_v1
        || app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
    {
        return true;
    }
    app.scoped_reviews_work_v2.is_some()
        || app.scoped_reviews_cancel_v2
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
        || app
            .form
            .as_ref()
            .is_some_and(|form| form.preview_requested || form.submit_requested)
}
fn queue(app: &mut App, kind: Kind) {
    if app.scoped_reviews_owner_v2.is_some()
        || app.scoped_reviews_work_v2.is_some()
        || app.loading[View::Inbox.index()]
    {
        app.status = "Review operation busy • Esc cancels exact original operation".into();
        return;
    }
    match ScopedReviewsWorkV2::new(kind) {
        Ok(work) if work.matches(app) => app.scoped_reviews_work_v2 = Some(work),
        Ok(_) => app.status = "Original review presentation unavailable; nothing queued".into(),
        Err(reason) => app.status = reason,
    }
}
/// Files toolbar entry, or genuine selected Images range for an anchored batch.
pub fn open(app: &mut App, anchored: bool) {
    let intent = match ScopedReviewIntentV2::capture(app) {
        Ok(intent) => intent,
        Err(reason) => {
            app.status = reason;
            return;
        }
    };
    let anchor = if anchored {
        match app
            .pr_file_images
            .as_ref()
            .and_then(|view| view.canonical_range())
            .ok_or("Choose an actual canonical Range before Batch")
            .and_then(|range| {
                ScopedReviewAnchorIntentV2::capture(range, app)
                    .map_err(|_| "Original batch range unavailable")
            }) {
            Ok(anchor) => Some(anchor),
            Err(reason) => {
                app.status = reason.into();
                return;
            }
        }
    } else {
        None
    };
    queue(app, Kind::Open { intent, anchor });
}
pub fn capture_form(app: &mut App) {
    if app.scoped_reviews_work_v2.is_some() || app.scoped_reviews_owner_v2.is_some() {
        return;
    }
    if app.form.as_ref().is_some_and(|form| {
        matches!(
            form.purpose,
            crate::forms::FormPurpose::PrReviewOperationV3(_)
        )
    }) {
        let Some(form) = app.form.as_mut() else {
            return;
        };
        if form.submitting || (!form.preview_requested && !form.submit_requested) {
            return;
        }
        let confirm = form.submit_requested;
        form.submit_requested = false;
        form.preview_requested = false;
        let request = if confirm {
            operation_ui_v3::confirmed_preview(form).and_then(|_| operation_ui_v3::request(form))
        } else {
            operation_ui_v3::request(form)
        };
        match request {
            Ok(request) => queue(
                app,
                if confirm {
                    Kind::OperationConfirm(request)
                } else {
                    Kind::OperationPreview(request)
                },
            ),
            Err(reason) => {
                if let Some(form) = &mut app.form {
                    form.error = reason;
                    form.invalidate_plan();
                }
            }
        }
        return;
    }
    let Some(form) = app
        .form
        .as_mut()
        .filter(|form| matches!(form.purpose, crate::forms::FormPurpose::PrReviewV2(_)))
    else {
        return;
    };
    if form.submitting || (!form.preview_requested && !form.submit_requested) {
        return;
    }
    let confirm = form.submit_requested;
    form.submit_requested = false;
    form.preview_requested = false;
    let request = if confirm {
        ui_v2::confirmed_preview(form).and_then(|_| ui_v2::request(form))
    } else {
        ui_v2::request(form)
    };
    match request {
        Ok(request) => queue(
            app,
            if confirm {
                Kind::Confirm(request)
            } else {
                Kind::Preview(request)
            },
        ),
        Err(reason) => {
            form.error = reason;
            form.invalidate_plan();
        }
    }
}
enum ResultKind {
    NeedsNewV12(Arc<action_lineage_v12::NeedsNewActionV12>),
    Opened(Box<ReviewBrowserV2>),
    Selected(
        Arc<ScopedReviewSelectionV2>,
        browser_v2::PreparedReviewDetailsV7,
    ),
    Form(Box<crate::forms::Form>),
    Preview(Arc<ScopedReviewPreviewV2>),
    Published(ScopedReviewPublicationV2),
    OperationPreview(Arc<ReviewOperationPreviewV3>),
    OperationPublished(operations_v3::execute_v3::ReviewOperationPublicationV3),
    History(Arc<PreparedReviewOperationHistoryV3>),
    RecoveryInspectionV11(Arc<history_v3::recovery_v11::PreparedReviewRecoveryV11>),
    RecoveryAcknowledgedV11(history_v3::recovery_v11::ReviewRecoveryAcknowledgedV11),
    TeamsV3(operations_v3::teams_v3::PreparedReviewRequestTeamsV3),
    Failed {
        hide_private: bool,
        message: String,
    },
}
pub struct PreparedScopedReviewsV2 {
    owner: Arc<ScopedReviewsOwnerV2>,
    kind: ResultKind,
}
impl PreparedScopedReviewsV2 {
    pub fn matches(&self, owner: &Arc<ScopedReviewsOwnerV2>) -> bool {
        Arc::ptr_eq(owner, &self.owner)
            && match (&owner.work.kind, &self.kind) {
                (Kind::NewActionV12 { notice, .. }, ResultKind::Form(form)) => {
                    match &form.purpose {
                        crate::forms::FormPurpose::PrReviewV2(state) => {
                            Arc::ptr_eq(state.intent(), notice.intent())
                                && state.draft_key() == notice.revision.key()
                        }
                        crate::forms::FormPurpose::PrReviewOperationV3(state) => {
                            Arc::ptr_eq(state.intent(), notice.intent())
                                && state.draft_key() == notice.revision.key()
                        }
                        _ => false,
                    }
                }
                (
                    Kind::Compose {
                        draft,
                        selection,
                        action,
                        ..
                    },
                    ResultKind::NeedsNewV12(notice),
                ) => {
                    matches!(&notice.context,action_lineage_v12::ActionContextV12::Review{draft:fresh,selection:actual,action:chosen}
                        if Arc::ptr_eq(draft.intent(),fresh.intent())&&chosen==action&&match(selection,actual){(None,None)=>true,(Some(a),Some(b))=>Arc::ptr_eq(a,b),_=>false})
                }
                (Kind::OperationOpen { intent, .. }, ResultKind::NeedsNewV12(notice)) => {
                    matches!(&notice.context,action_lineage_v12::ActionContextV12::Operation(actual)if Arc::ptr_eq(intent,actual.intent()))
                }
                (
                    Kind::OperationAdd {
                        selection, anchor, ..
                    },
                    ResultKind::NeedsNewV12(notice),
                ) => {
                    matches!(&notice.context,action_lineage_v12::ActionContextV12::Operation(actual)if actual.intent().matches_add_work_v3(selection,anchor))
                }
                (Kind::Open { intent, anchor }, ResultKind::Opened(view)) => {
                    Arc::ptr_eq(intent, view.intent())
                        && match (anchor, &view.pending_anchor_v3) {
                            (None, None) => true,
                            (Some(original), Some(retained)) => Arc::ptr_eq(original, retained),
                            _ => false,
                        }
                }
                (Kind::Read { draft, row, .. }, ResultKind::Selected(selection, _)) => {
                    Arc::ptr_eq(draft.intent(), selection.intent())
                        && draft.catalogue().rows().get(*row).is_some_and(|actual| {
                            actual.id() == selection.snapshot().review().id()
                                && actual.node() == selection.snapshot().review().node()
                        })
                }
                (Kind::Compose { draft, .. }, ResultKind::Form(form)) => {
                    matches!(&form.purpose,crate::forms::FormPurpose::PrReviewV2(state) if Arc::ptr_eq(state.intent(),draft.intent()))
                }
                (
                    Kind::Open {
                        intent,
                        anchor: Some(_),
                    },
                    ResultKind::Form(form),
                ) => {
                    matches!(&form.purpose,crate::forms::FormPurpose::PrReviewV2(state) if Arc::ptr_eq(state.intent(),intent))
                }
                (Kind::Preview(request), ResultKind::Preview(preview)) => {
                    Arc::ptr_eq(request.request(), preview.request())
                }
                (Kind::OperationOpen { intent, .. }, ResultKind::Form(form)) => {
                    matches!(&form.purpose,
                    crate::forms::FormPurpose::PrReviewOperationV3(state) if Arc::ptr_eq(state.operation_intent(),intent))
                }
                (
                    Kind::OperationAdd {
                        selection, anchor, ..
                    },
                    ResultKind::Form(form),
                ) => matches!(&form.purpose,
                    crate::forms::FormPurpose::PrReviewOperationV3(state) if Arc::ptr_eq(state.intent(),selection.intent())
                        &&state.operation_intent().matches_add_work_v3(selection,anchor)),
                (Kind::OperationPreview(request), ResultKind::OperationPreview(preview)) => {
                    Arc::ptr_eq(request.request(), preview.request())
                }
                (Kind::OperationConfirm(_), ResultKind::OperationPublished(_)) => true,
                (Kind::HistoryList { intent, .. }, ResultKind::History(data)) => {
                    Arc::ptr_eq(intent, data.intent()) && data.opened.is_none()
                }
                (Kind::HistoryOpen { listed, row, .. }, ResultKind::History(data)) => {
                    Arc::ptr_eq(listed.intent(), data.intent()) && data.opened == Some(*row)
                }
                (
                    Kind::RecoveryInspectV11 { request, .. },
                    ResultKind::RecoveryInspectionV11(prepared),
                ) => Arc::ptr_eq(request, prepared.request()),
                (
                    Kind::RecoveryAcknowledgeV11 { prepared, .. },
                    ResultKind::RecoveryAcknowledgedV11(outcome),
                ) => prepared.reference() == &outcome.reference && outcome.changed == 1,
                (Kind::TeamsV3 { draft, .. }, ResultKind::TeamsV3(data)) => {
                    Arc::ptr_eq(draft.intent(), &data.original)
                }
                (Kind::Confirm(_), ResultKind::Published(_)) | (_, ResultKind::Failed { .. }) => {
                    true
                }
                _ => false,
            }
    }
}
fn failure(error: ScopedReviewFailureV2) -> ResultKind {
    ResultKind::Failed {
        hide_private: error.hide_private,
        message: error.to_string(),
    }
}
pub fn execute(
    owner: Arc<ScopedReviewsOwnerV2>,
    provider: &GithubProvider,
    database: &Path,
    drafts: &DraftClient,
) -> PreparedScopedReviewsV2 {
    let cancel = owner.cancellation();
    let deadline = owner.deadline();
    let kind = match &owner.work.kind {
        Kind::NewActionV12 { notice, .. } => {
            new_action_form_v12(notice, provider, database, cancel, deadline)
        }
        #[cfg(test)]
        Kind::ActualWorkerPanic(_) => panic!("ACTUAL_REVIEW_WORKER_PANIC_PRIVATE_POISON"),
        Kind::Open { intent, anchor } => match ScopedReviewDraftV2::open(
            intent.clone(),
            provider,
            database,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            cancel,
            deadline,
        ) {
            Ok(draft) => {
                if let Some(anchor) = anchor {
                    if draft
                        .catalogue()
                        .rows()
                        .iter()
                        .any(|review| review.is_observed_own_pending())
                    {
                        match ReviewBrowserV2::prepare(draft, cancel, deadline) {
                            Ok(mut view) => {
                                view.pending_anchor_v3 = Some(anchor.clone());
                                ResultKind::Opened(Box::new(view))
                            }
                            Err(message) => ResultKind::Failed {
                                hide_private: false,
                                message,
                            },
                        }
                    } else {
                        match ui_v2::form_from_draft(
                            draft,
                            None,
                            ReviewRequestedActionV2::CreatePending,
                            Some(anchor.clone()),
                            cancel,
                            deadline,
                        ) {
                            Ok(form) => ResultKind::Form(Box::new(form)),
                            Err(message) => ResultKind::Failed {
                                hide_private: false,
                                message,
                            },
                        }
                    }
                } else {
                    match ReviewBrowserV2::prepare(draft, cancel, deadline) {
                        Ok(view) => ResultKind::Opened(Box::new(view)),
                        Err(message) => ResultKind::Failed {
                            hide_private: false,
                            message,
                        },
                    }
                }
            }
            Err(error) => failure(error),
        },
        Kind::Read { draft, row, .. } => match ScopedReviewSelectionV2::read(
            draft,
            *row,
            provider,
            database,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            cancel,
            deadline,
        ) {
            Ok(selection) => match ReviewBrowserV2::selected_rows(&selection, cancel, deadline) {
                Ok(rows) => ResultKind::Selected(selection, rows),
                Err(message) => ResultKind::Failed {
                    hide_private: false,
                    message,
                },
            },
            Err(error) => failure(error),
        },
        Kind::Compose {
            draft,
            selection,
            action,
            ..
        } => {
            let fresh = if let Some(selection) = selection {
                ScopedReviewDraftV2::open_selected(
                    selection,
                    *action,
                    provider,
                    database,
                    &Default::default(),
                    &Default::default(),
                    &Default::default(),
                    cancel,
                    deadline,
                )
            } else {
                ScopedReviewDraftV2::open(
                    draft.intent().clone(),
                    provider,
                    database,
                    &Default::default(),
                    &Default::default(),
                    &Default::default(),
                    cancel,
                    deadline,
                )
            };
            match fresh {
                Ok(draft) if draft.new_action_revision_v12().is_some() => {
                    match action_lineage_v12::NeedsNewActionV12::prepare(
                        action_lineage_v12::ActionContextV12::Review {
                            draft: draft.clone(),
                            selection: selection.clone(),
                            action: *action,
                        },
                        draft
                            .new_action_revision_v12()
                            .expect("checked original revision")
                            .clone(),
                        draft
                            .new_notice_v12
                            .as_ref()
                            .expect("producer prepared notice before final close")
                            .clone(),
                        cancel,
                        deadline,
                    ) {
                        Ok(notice) => ResultKind::NeedsNewV12(notice),
                        Err(message) => ResultKind::Failed {
                            hide_private: false,
                            message,
                        },
                    }
                }
                Ok(draft) => match ui_v2::form_from_draft(
                    draft,
                    selection.clone(),
                    *action,
                    None,
                    cancel,
                    deadline,
                ) {
                    Ok(form) => ResultKind::Form(Box::new(form)),
                    Err(message) => ResultKind::Failed {
                        hide_private: false,
                        message,
                    },
                },
                Err(error) => failure(error),
            }
        }
        Kind::Preview(request) => match ScopedReviewPreviewV2::prepare(
            request.request().clone(),
            provider,
            database,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            cancel,
            deadline,
        ) {
            Ok(preview) => ResultKind::Preview(preview),
            Err(error) => failure(error),
        },
        Kind::Confirm(request) => {
            let publication = owner
                .publication
                .lock()
                .ok()
                .and_then(|mut value| value.take());
            match (request.preview(), publication) {
                (Some(preview), Some(publication)) => match preview.execute(
                    provider,
                    database,
                    drafts,
                    publication,
                    true,
                    None,
                    &Default::default(),
                    &Default::default(),
                    &Default::default(),
                    &Default::default(),
                    cancel,
                    deadline,
                ) {
                    Ok(result) => ResultKind::Published(result),
                    Err(error) => failure(error),
                },
                _ => ResultKind::Failed {
                    hide_private: true,
                    message: "Original one-use draft ACK/Preview unavailable; nothing resent"
                        .into(),
                },
            }
        }
        Kind::OperationOpen { intent, .. } => {
            operation_form_v3(intent.clone(), provider, database, cancel, deadline)
        }
        Kind::OperationAdd {
            selection, anchor, ..
        } => {
            match ScopedReviewOperationIntentV3::add(selection.clone(), anchor, cancel, deadline) {
                Ok(intent) => operation_form_v3(intent, provider, database, cancel, deadline),
                Err(message) => ResultKind::Failed {
                    hide_private: false,
                    message,
                },
            }
        }
        Kind::OperationPreview(request) => match ReviewOperationPreviewV3::prepare(
            request.request().clone(),
            provider,
            database,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            cancel,
            deadline,
        ) {
            Ok(preview) => ResultKind::OperationPreview(preview),
            Err(error) => failure(error),
        },
        Kind::OperationConfirm(request) => {
            let publication = owner
                .publication
                .lock()
                .ok()
                .and_then(|mut value| value.take());
            match (request.preview(), publication) {
                (Some(preview), Some(publication)) => match preview.execute(
                    provider,
                    database,
                    drafts,
                    publication,
                    true,
                    None,
                    &Default::default(),
                    &Default::default(),
                    &Default::default(),
                    &Default::default(),
                    cancel,
                    deadline,
                ) {
                    Ok(result) => ResultKind::OperationPublished(result),
                    Err(error) => failure(error),
                },
                _ => ResultKind::Failed {
                    hide_private: true,
                    message: "Original operation Preview/one-use ACK unavailable; nothing resent"
                        .into(),
                },
            }
        }
        Kind::HistoryList { intent, .. } => {
            match history_v3::load(intent.clone(), None, provider, database, cancel, deadline) {
                Ok(data) => ResultKind::History(data),
                Err(error) => failure(error),
            }
        }
        Kind::HistoryOpen { listed, row, .. } => match history_v3::load(
            listed.intent().clone(),
            Some((listed, *row)),
            provider,
            database,
            cancel,
            deadline,
        ) {
            Ok(data) => ResultKind::History(data),
            Err(error) => failure(error),
        },
        Kind::RecoveryInspectV11 { request, .. } => match history_v3::recovery_v11::inspect(
            request.clone(),
            provider,
            database,
            &Default::default(),
            cancel,
            deadline,
        ) {
            Ok(prepared) => ResultKind::RecoveryInspectionV11(prepared),
            Err(error) => failure(error),
        },
        Kind::RecoveryAcknowledgeV11 { prepared, .. } => {
            match history_v3::recovery_v11::acknowledge(
                prepared, provider, database, cancel, deadline,
            ) {
                Ok(outcome) => ResultKind::RecoveryAcknowledgedV11(outcome),
                Err(error) => failure(error),
            }
        }
        Kind::TeamsV3 { draft, .. } => match operations_v3::teams_v3::read(
            draft.intent().clone(),
            provider,
            database,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            cancel,
            deadline,
        ) {
            Ok(data) => ResultKind::TeamsV3(data),
            Err(error) => failure(error),
        },
    };
    PreparedScopedReviewsV2 { owner, kind }
}
fn operation_form_v3(
    intent: Arc<ScopedReviewOperationIntentV3>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> ResultKind {
    match ScopedReviewOperationSourceV3::open(
        intent,
        provider,
        database,
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        cancel,
        deadline,
    ) {
        Ok(source) if source.new_action_revision_v12().is_some() => {
            match action_lineage_v12::NeedsNewActionV12::prepare(
                action_lineage_v12::ActionContextV12::Operation(source.clone()),
                source
                    .new_action_revision_v12()
                    .expect("checked exact terminal revision")
                    .clone(),
                source
                    .new_notice_v12
                    .as_ref()
                    .expect("producer prepared notice before final close")
                    .clone(),
                cancel,
                deadline,
            ) {
                Ok(notice) => ResultKind::NeedsNewV12(notice),
                Err(message) => ResultKind::Failed {
                    hide_private: false,
                    message,
                },
            }
        }
        Ok(source) => match operation_ui_v3::form_from_source(source, cancel, deadline) {
            Ok(form) => ResultKind::Form(Box::new(form)),
            Err(message) => ResultKind::Failed {
                hide_private: false,
                message,
            },
        },
        Err(error) => failure(error),
    }
}
fn new_action_form_v12(
    notice: &Arc<action_lineage_v12::NeedsNewActionV12>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> ResultKind {
    if cancel.is_cancelled()
        || Instant::now() >= deadline
        || Instant::now() >= notice.deadline
        || notice
            .opened
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_err()
    {
        return ResultKind::Failed {
            hide_private: false,
            message: "Original New action ended/already opened; inspect again explicitly".into(),
        };
    }
    let form = match &notice.context {
        action_lineage_v12::ActionContextV12::Review {
            draft,
            selection,
            action,
        } => {
            match ScopedReviewDraftV2::open_new_v12(
                draft.intent().clone(),
                selection.as_ref(),
                *action,
                &notice.revision,
                provider,
                database,
                &Default::default(),
                &Default::default(),
                &Default::default(),
                cancel,
                deadline,
            ) {
                Ok(fresh) => ui_v2::form_from_draft(
                    fresh,
                    selection.clone(),
                    *action,
                    None,
                    cancel,
                    deadline,
                ),
                Err(error) => return failure(error),
            }
        }
        action_lineage_v12::ActionContextV12::Operation(source) => {
            match ScopedReviewOperationSourceV3::read_lineage_v12(
                source.intent().clone(),
                None,
                &notice.revision,
                provider,
                database,
                &Default::default(),
                &Default::default(),
                &Default::default(),
                &Default::default(),
                cancel,
                deadline,
            ) {
                Ok(source) => operation_ui_v3::form_from_source(source, cancel, deadline),
                Err(error) => return failure(error),
            }
        }
    };
    match form {
        Ok(form) => ResultKind::Form(Box::new(form)),
        Err(message) => ResultKind::Failed {
            hide_private: false,
            message,
        },
    }
}
pub fn retire_private(app: &mut App, background: &mut crate::runtime::Background) {
    use crate::active_remote_scope_v1::{RemoteScopeFenceV1, RemoteScopeStopV1};
    // Same original owner/source admission as Inline: do not leave a retained
    // private Native frame alive until a later draw after terminal retirement.
    app.pr_native_diff_v1 = None;
    app.pr_native_diff_layout_v1 = Default::default();
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
    app.scoped_reviews_v2 = None;
    app.scoped_review_history_v3 = None;
    app.scoped_reviews_owner_v2 = None;
    app.scoped_reviews_work_v2 = None;
    app.scoped_reviews_cancel_v2 = false;
    // The private browser model and its event owner must retire together.
    // Otherwise the next genuine Quit is consumed by an empty modal. Keep
    // independently opened overlays (for example Help) untouched.
    if app.overlay == Some(Overlay::ScopedReviewsV2) {
        app.overlay = None;
    }
}
/// An orphaned Confirm no longer owns any Background slot. Retire ONLY its
/// actual presentation; never cancel/retag a replacement request, manufacture a
/// source fence or mutate a successor's loading/status. The genuine identity,
/// Source/Scope/Nav allocations remain so a fresh original-nav read can finish.
pub(crate) fn retire_orphaned_confirm_presentation(app: &mut App, owner: &ScopedReviewsOwnerV2) {
    if owner.work.phase() != ScopedReviewsPhaseV2::Confirm {
        return;
    }
    let Some(intent) = owner.work.origin().files() else {
        return;
    };
    let old_form = owner.work.request().is_some_and(|request| {
        app.form
            .as_ref()
            .is_some_and(|form| request.owns_presentation(form))
    }) || owner.work.operation_request_v3().is_some_and(|request| {
        app.form
            .as_ref()
            .is_some_and(|form| request.owns_presentation(form))
    });
    let old_browser = app
        .scoped_reviews_v2
        .as_ref()
        .is_some_and(|view| Arc::ptr_eq(view.intent(), intent));
    // SAME source alone is insufficient: another genuine chosen PR/navigation
    // or a newer original Files read must never be erased by an old Confirm.
    let active_old_form = old_form && !app.form_dismissed && app.overlay == Some(Overlay::Form);
    let exact_original = intent.owns(app)
        && (active_old_form || old_browser && app.overlay == Some(Overlay::ScopedReviewsV2));
    let can_retire = exact_original
        && (!intent.source.is_personal()
            || crate::personal_pr_entry_v13::retain_orphan_source_skeleton_v14(
                app,
                &intent.source,
                &intent.navigation,
            ));
    if can_retire {
        let old_images = app
            .pr_file_images
            .as_ref()
            .is_some_and(|images| images.owns(app));
        let old_checks = app
            .pr_checks_origin
            .as_ref()
            .and_then(|origin| origin.scoped_owner_v1())
            .is_some_and(|(source, scope, nav)| {
                intent.source.same_catalogue_source(source)
                    && Arc::ptr_eq(scope, &intent.active)
                    && Arc::ptr_eq(nav, &intent.navigation)
            });
        let parent = intent.parent();
        let old_reactions = app.pr_reactions.as_ref().is_some_and(|page| {
            Scope::github(&page.identity.host, page.identity.account_id)
                .ok()
                .as_ref()
                == Some(intent.scope())
                && page.target.pr.id == parent.node_id()
                && page
                    .target
                    .pr
                    .repository
                    .eq_ignore_ascii_case(parent.repository())
                && page.target.pr.number == parent.number()
        });
        let old_inspection = app.pr_inspection.as_ref().is_some_and(|inspection| {
            inspection.try_lock().is_ok_and(|inspection| {
                inspection.as_ref().is_some_and(|inspection| {
                    let target = inspection.target();
                    target.scope() == intent.scope()
                        && target.node_id() == parent.node_id()
                        && target
                            .repository()
                            .eq_ignore_ascii_case(parent.repository())
                        && target.number() == parent.number()
                })
            })
        });
        // Drop private data, not the Workspace/Nav producer. ScopedRemoteOwner
        // can still admit its genuine replacement original-nav completion.
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
            // The ordinary retire helper also queues cancellation; an orphan
            // has no such authority. Drop bound private presentation only.
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
    if old_browser {
        app.scoped_reviews_v2 = None;
        if app.overlay == Some(Overlay::ScopedReviewsV2) {
            app.overlay = None;
        }
        app.hits.clear();
    }
}
pub fn fail(app: &mut App, background: &mut crate::runtime::Background) {
    retire_private(app, background);
    app.status="Original review result unavailable • draft/protection retained • nothing resent; refresh explicitly".into();
}
pub fn publish(
    app: &mut App,
    background: &mut crate::runtime::Background,
    owner: &Arc<ScopedReviewsOwnerV2>,
    result: PreparedScopedReviewsV2,
) {
    match result.kind {
        ResultKind::NeedsNewV12(notice) => {
            let Some(view) = app.scoped_reviews_v2.as_mut() else {
                fail(app, background);
                return;
            };
            if !view.install_new_action_v12(notice) {
                fail(app, background);
                return;
            }
            app.status="Original terminal action protected • explicitly choose New action or Back • nothing sent".into();
        }
        ResultKind::Opened(view) => {
            app.scoped_review_history_v3 = None;
            app.scoped_reviews_v2 = Some(*view);
            app.overlay = Some(Overlay::ScopedReviewsV2);
            app.status = "Actual reviews loaded • numeric ownership observed • no mutation".into();
        }
        ResultKind::Selected(selection, rows) => {
            if app
                .scoped_reviews_v2
                .as_mut()
                .is_none_or(|view| !view.install_selection(selection, rows))
            {
                fail(app, background);
                return;
            }
            app.status = "Fresh actual review/comments resumed • no approval restored".into();
        }
        ResultKind::Form(form) => {
            app.scoped_review_history_v3 = None;
            app.form = Some(*form);
            app.form_dismissed = false;
            app.overlay = Some(Overlay::Form);
            app.status =
                "Original review intent opened • fresh Preview required • nothing submitted".into();
        }
        ResultKind::Preview(preview) => {
            if let (Some(form), Some(request)) = (&mut app.form, owner.work.request()) {
                if !ui_v2::accept_preview(form, request, preview) {
                    form.invalidate_plan();
                    form.error = "Exact review Preview presentation refused; draft retained".into();
                }
                form.submitting = false;
            }
            app.status = "Fresh review Preview ready • explicit Confirm required".into();
        }
        ResultKind::OperationPreview(preview) => {
            if let (Some(form), Some(request)) = (&mut app.form, owner.work.operation_request_v3())
            {
                if !operation_ui_v3::accept_preview(form, request, preview) {
                    form.invalidate_plan();
                    form.error = "Exact original operation Preview refused; draft retained".into();
                }
                form.submitting = false;
            }
            app.status = "Fresh pending operation Preview ready • explicit Confirm required".into();
        }
        ResultKind::OperationPublished(result) => {
            let state = result.state();
            let safe = result.safe_effect();
            retire_private(app, background);
            use operations_v3::execute_v3::ReviewOperationSafeEffectV3;
            app.status = match state {
                ScopedReviewPublicationStateV2::ConfirmedSnapshotRetired => match safe {
                    Some(ReviewOperationSafeEffectV3::PendingComment {
                        id,
                        review_and_comment_absent: true,
                        ..
                    }) => format!("Comment #{id} and review absence observed • refresh source"),
                    Some(ReviewOperationSafeEffectV3::PendingComment { id, .. }) => {
                        format!("Pending comment #{id} confirmed • refresh source")
                    }
                    Some(ReviewOperationSafeEffectV3::ReviewerRequested { kind, id }) => {
                        format!("{kind:?} #{id} requested • delivery unknown • refresh source")
                    }
                    None => "Effect recorded; inspect protected history; no resend".into(),
                },
                ScopedReviewPublicationStateV2::NotStarted => {
                    "Not sent • draft protected • inspect history; no resend".into()
                }
                ScopedReviewPublicationStateV2::Uncertain => {
                    "Outcome unknown • inspect protected history • nothing resent".into()
                }
                ScopedReviewPublicationStateV2::ConfirmedButJournalFailed => {
                    "Effect observed • journal unavailable • protected history retained".into()
                }
                ScopedReviewPublicationStateV2::ConfirmedButSourceUnavailable => {
                    "Effect confirmed • source unavailable • inspect protected history".into()
                }
            };
        }
        ResultKind::History(data) => {
            app.scoped_reviews_v2 = None;
            if data.opened.is_some() {
                let installed = app
                    .scoped_review_history_v3
                    .as_mut()
                    .is_some_and(|view| view.install_opened(data));
                if !installed {
                    fail(app, background);
                    return;
                }
            } else {
                app.scoped_review_history_v3 =
                    Some(history_v3::ui::ReviewOperationHistoryViewV3::new(data));
            }
            app.overlay = Some(Overlay::ScopedReviewsV2);
            app.status =
                "Original review operation history • inspect only • no approval restored".into();
        }
        ResultKind::TeamsV3(data) => {
            if !app
                .scoped_reviews_v2
                .as_mut()
                .is_some_and(|view| view.install_teams_v3(data))
            {
                fail(app, background);
                return;
            }
            app.status = "Actual repository teams • re-request eligibility UNKNOWN".into();
        }
        ResultKind::RecoveryInspectionV11(prepared) => {
            if !app
                .scoped_review_history_v3
                .as_mut()
                .is_some_and(|view| view.install_recovery_v11(prepared))
            {
                fail(app, background);
                return;
            }
            app.status = "Fresh current outcome • explicit local ACK only • nothing resent".into();
        }
        ResultKind::RecoveryAcknowledgedV11(outcome) => {
            let attempt = outcome.reference.attempt_id;
            retire_private(app, background);
            app.status = format!(
                "Attempt #{attempt} locally acknowledged • nothing resent • refresh source"
            );
        }
        ResultKind::Failed {
            hide_private,
            message,
        } => {
            if hide_private || owner.work.phase() == ScopedReviewsPhaseV2::Confirm {
                retire_private(app, background);
                app.status="Original review source unavailable • private content hidden • draft/protection retained; no resend".into();
            } else if let Some(form) = &mut app.form {
                form.submitting = false;
                form.invalidate_plan();
                form.error = message;
            } else if let Some(view) = &mut app.scoped_reviews_v2 {
                view.notice(&message);
            } else if let Some(view) = &mut app.scoped_review_history_v3 {
                // Static safe message only; do not turn a source failure into
                // retained private history (hide_private handled above).
                view.message = "Original history read refused; no source/approval restored";
            } else {
                app.status = message;
            }
        }
        ResultKind::Published(result) => {
            let id = result.confirmed_review_id();
            let state = result.state();
            retire_private(app, background);
            app.status = publication_status(state, id);
        }
    }
    app.loading[View::Inbox.index()] = false;
    app.hits.clear();
    if app
        .scoped_reviews_owner_v2
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, owner))
    {
        app.scoped_reviews_owner_v2 = None;
    }
}

/// Presentation only, never a receipt or publication proof. The entire outcome
/// and recovery instruction must fit the ordinary 80-column status row.
fn publication_status(state: ScopedReviewPublicationStateV2, id: Option<u64>) -> String {
    match state {
        ScopedReviewPublicationStateV2::ConfirmedSnapshotRetired => format!(
            "Review #{} effect confirmed; snapshot retired; refresh",
            id.unwrap_or(0)
        ),
        ScopedReviewPublicationStateV2::NotStarted => {
            "Review publication not started • draft/protection kept; no resend".into()
        }
        ScopedReviewPublicationStateV2::Uncertain => {
            "Review outcome unknown • inspect explicitly; no resend • draft/protection kept".into()
        }
        ScopedReviewPublicationStateV2::ConfirmedButJournalFailed => {
            "Review effect confirmed • journal failed; protection kept; refresh explicitly".into()
        }
        ScopedReviewPublicationStateV2::ConfirmedButSourceUnavailable => {
            "Review effect confirmed • source unavailable; hidden; refresh explicitly".into()
        }
    }
}

pub fn browser_action(app: &mut App, action: ReviewBrowserActionV2) {
    if retire_invalid(app) {
        return;
    }
    if app.scoped_reviews_owner_v2.is_some() {
        match action {
            ReviewBrowserActionV2::Close => app.scoped_reviews_cancel_v2 = true,
            ReviewBrowserActionV2::Quit => app.dispatch(Action::Quit),
            _ => app.status = "Original review operation pending; Esc cancels".into(),
        }
        return;
    }
    let Some(view) = &mut app.scoped_reviews_v2 else {
        return;
    };
    let Some(action) = view.apply(action) else {
        return;
    };
    if !view.interactive()
        && !matches!(
            action,
            ReviewBrowserActionV2::Close | ReviewBrowserActionV2::Quit
        )
    {
        view.notice("Resize to inspect the current review target and controls; actions disabled until useful current paint");
        return;
    }
    let draft = view.draft.clone();
    let ticket = view.ticket.clone();
    let selection = view.selection.clone();
    let row = view.selected();
    if view.new_action_v12.is_some() {
        match action {
            ReviewBrowserActionV2::NewActionV12 => {
                let notice = view
                    .new_action_v12
                    .as_ref()
                    .expect("current notice")
                    .clone();
                queue(app, Kind::NewActionV12 { notice, ticket });
            }
            ReviewBrowserActionV2::Close => {
                view.new_action_v12 = None;
                view.scroll = 0;
                view.ticket = Arc::new(());
                view.clear_layout();
            }
            ReviewBrowserActionV2::Quit => app.dispatch(Action::Quit),
            _ => {}
        }
        return;
    }
    if action == ReviewBrowserActionV2::Resume && view.teams_v3.is_some() {
        view.notice("The selected row is a team, not a review; choose explicit Re-request team or close/reopen Reviews");
        return;
    }
    match action {
        ReviewBrowserActionV2::NewActionV12 => {
            view.notice("Choose an exact protected action first; no arbitrary history revision")
        }
        ReviewBrowserActionV2::NextPendingCommentV3 => {
            if view.teams_v3.is_some() {
                view.notice("Choose an actual pending review, not a team");
                return;
            }
            let Some(selection) = selection else {
                view.notice("Resume an actual server PENDING review first");
                return;
            };
            let comments = selection.snapshot().comments();
            if comments.is_empty() {
                view.notice("Actual pending review has no loaded comments");
                return;
            }
            view.pending_comment_v3 = (view.pending_comment_v3 + 1) % comments.len();
            view.ticket = Arc::new(());
            view.notice(&format!("Selected actual PENDING comment #{} • F3 edit / F4 delete freshly reads capabilities",comments[view.pending_comment_v3].id()));
        }
        ReviewBrowserActionV2::EditPendingCommentV3
        | ReviewBrowserActionV2::DeletePendingCommentV3 => {
            if view.teams_v3.is_some() {
                view.notice("A team is not a pending comment");
                return;
            }
            let Some(selection) = selection else {
                view.notice("Resume an actual server PENDING review first");
                return;
            };
            match ScopedReviewOperationIntentV3::edit(
                selection,
                view.pending_comment_v3,
                action == ReviewBrowserActionV2::DeletePendingCommentV3,
            ) {
                Ok(intent) => queue(app, Kind::OperationOpen { intent, ticket }),
                Err(reason) => view.notice(&reason),
            }
        }
        ReviewBrowserActionV2::AddPendingRangeV3 => {
            if view.teams_v3.is_some() {
                view.notice("A team is not an own PENDING review");
                return;
            }
            let Some(selection) = selection else {
                view.notice("Resume actual own PENDING review, then choose G on its original captured range");
                return;
            };
            let Some(anchor) = view.pending_anchor_v3.clone() else {
                view.notice("Open Images, choose actual canonical Range and V Batch first; no decoded/native range");
                return;
            };
            queue(
                app,
                Kind::OperationAdd {
                    selection,
                    anchor,
                    ticket,
                },
            );
        }
        ReviewBrowserActionV2::RequestReviewerV3 => {
            if view.teams_v3.is_some() {
                view.notice("Choose the actual submitted reviewer or explicit selected team");
                return;
            }
            let Some(selection) = selection else {
                view.notice("Resume an actual submitted review first");
                return;
            };
            match ScopedReviewOperationIntentV3::reviewer(selection) {
                Ok(intent) => queue(app, Kind::OperationOpen { intent, ticket }),
                Err(reason) => view.notice(&reason),
            }
        }
        ReviewBrowserActionV2::TeamsV3 => queue(app, Kind::TeamsV3 { draft, ticket }),
        ReviewBrowserActionV2::RequestTeamV3 => {
            let Some(roster) = view.teams_v3.clone() else {
                view.notice("Load actual repository Teams before choosing an exact team");
                return;
            };
            match ScopedReviewOperationIntentV3::team(
                draft.intent().clone(),
                roster,
                view.selected_team_v3,
            ) {
                Ok(intent) => queue(app, Kind::OperationOpen { intent, ticket }),
                Err(reason) => view.notice(&reason),
            }
        }
        ReviewBrowserActionV2::Resume => queue(app, Kind::Read { draft, row, ticket }),
        ReviewBrowserActionV2::NewBatch => {
            if let Some(reason) = view.new_batch_disabled_v12() {
                view.notice(reason);
            } else {
                queue(
                    app,
                    Kind::Compose {
                        draft,
                        selection: None,
                        action: ReviewRequestedActionV2::CreatePending,
                        ticket,
                    },
                );
            }
        }
        ReviewBrowserActionV2::Effect(action) => {
            if let Some(reason) = view.effect_disabled(action) {
                view.notice(reason);
            } else {
                queue(
                    app,
                    Kind::Compose {
                        draft,
                        selection,
                        action,
                        ticket,
                    },
                )
            }
        }
        ReviewBrowserActionV2::Close => {
            app.scoped_reviews_v2 = None;
            app.overlay = None;
            app.hits.clear();
        }
        ReviewBrowserActionV2::Quit => app.dispatch(Action::Quit),
        _ => {}
    }
}
pub fn open_history_v3(app: &mut App) {
    if app.scoped_review_history_v3.is_some() || app.scoped_reviews_v2.is_some() {
        app.status =
            "Close the current actual review presentation before opening Summary history".into();
        return;
    }
    match ReviewOperationHistoryIntentV3::capture(app) {
        Ok(intent) => queue(
            app,
            Kind::HistoryList {
                intent,
                ticket: None,
            },
        ),
        Err(reason) => app.status = reason,
    }
}
/// SAME actual F9 action forwarded once to Main's known database boundary.
/// Only cached bounded capture happens here; no source/Files/SQL grant.
pub fn capture_requested_history_v15(app: &mut App, database: &Path) -> bool {
    if app.github_action != Some(crate::github_workspace::GithubAction::ReviewOperationHistoryV3) {
        return false;
    }
    app.github_action = None;
    if app.scoped_review_history_v3.is_some() || app.scoped_reviews_v2.is_some() {
        app.status = "Close the current review presentation before opening Summary history".into();
        return true;
    }
    match ReviewOperationHistoryIntentV3::capture_personal_v15(app, database) {
        Ok(intent) => queue(
            app,
            Kind::HistoryList {
                intent,
                ticket: None,
            },
        ),
        Err(reason) => app.status = reason,
    }
    true
}
fn history_action_v3(app: &mut App, action: history_v3::ui::HistoryActionV3) {
    use history_v3::ui::HistoryActionV3 as A;
    if app.scoped_reviews_v2.is_some() {
        return;
    } // mixed presentation never chooses hidden underlay
    if app.scoped_reviews_owner_v2.is_some() {
        match action {
            A::Back => app.scoped_reviews_cancel_v2 = true,
            A::Quit => {
                app.scoped_reviews_cancel_v2 = true;
                app.quit = true;
            }
            _ => {}
        }
        return;
    }
    let Some(view) = &mut app.scoped_review_history_v3 else {
        return;
    };
    let Some(action) = view.apply(action) else {
        return;
    };
    match action {
        A::Open => {
            let listed = view.listed.clone();
            let row = view.selected;
            let ticket = view.ticket.clone();
            queue(
                app,
                Kind::HistoryOpen {
                    listed,
                    row,
                    ticket,
                },
            );
        }
        A::Refresh => {
            let intent = view.intent().clone();
            let ticket = Some(view.ticket.clone());
            queue(app, Kind::HistoryList { intent, ticket });
        }
        A::InspectCurrentV11 => {
            let Some(opened) = view.opened.clone() else {
                return;
            };
            let row = view.selected;
            let ticket = view.ticket.clone();
            match history_v3::recovery_v11::ReviewRecoveryRequestV11::capture(opened,row){
                Ok(request)=>queue(app,Kind::RecoveryInspectV11{request,ticket}),
                Err(_)=>view.message="Choose an exact unresolved original Review attempt; no acknowledgement prepared",
            }
        }
        A::AcknowledgeV11 => {
            let Some(prepared) = view
                .recovery_v11
                .clone()
                .filter(|prepared| prepared.available())
            else {
                return;
            };
            let ticket = view.ticket.clone();
            queue(app, Kind::RecoveryAcknowledgeV11 { prepared, ticket });
        }
        A::Back => {
            app.scoped_review_history_v3 = None;
            if app.overlay == Some(Overlay::ScopedReviewsV2) {
                app.overlay = None;
            }
            app.hits.clear();
        }
        A::Quit => {
            app.scoped_review_history_v3 = None;
            app.overlay = None;
            app.quit = true;
            app.hits.clear();
        }
        _ => {}
    }
}
pub fn retire_invalid(app: &mut App) -> bool {
    let summary = app
        .scoped_review_history_v3
        .as_ref()
        .map(|view| view.intent().clone())
        .or_else(|| {
            app.scoped_reviews_work_v2
                .as_ref()
                .and_then(|work| work.origin().summary().cloned())
        })
        .or_else(|| {
            app.scoped_reviews_owner_v2
                .as_ref()
                .and_then(|owner| owner.work.origin().summary().cloned())
        });
    if let Some(original) = summary {
        if original.retire_account_mismatch_v15(app) {
            if app.scoped_reviews_work_v2.as_ref().is_some_and(|work| {
                work.origin()
                    .summary()
                    .is_some_and(|intent| Arc::ptr_eq(intent, &original))
            }) {
                if let Some(work) = app.scoped_reviews_work_v2.take() {
                    work.cancel.cancel();
                }
            }
            if let Some(owner) = app.scoped_reviews_owner_v2.as_ref().filter(|owner| {
                owner
                    .work
                    .origin()
                    .summary()
                    .is_some_and(|intent| Arc::ptr_eq(intent, &original))
            }) {
                owner.cancellation().cancel();
                app.scoped_reviews_cancel_v2 = true;
            }
            if app
                .scoped_review_history_v3
                .as_ref()
                .is_some_and(|view| Arc::ptr_eq(view.intent(), &original))
            {
                app.scoped_review_history_v3 = None;
            }
            if app.overlay == Some(Overlay::ScopedReviewsV2)
                && app.scoped_reviews_v2.is_none()
                && app.scoped_review_history_v3.is_none()
            {
                app.overlay = None;
                app.hits.clear();
            }
            return true;
        }
        if app
            .scoped_review_history_v3
            .as_ref()
            .is_some_and(|view| !view.owns(app))
        {
            app.scoped_review_history_v3 = None;
            // A valid independently newer browser/Form is not old history's
            // cleanup target, even if it shares the account/cache allocation.
            if app.overlay == Some(Overlay::ScopedReviewsV2) && app.scoped_reviews_v2.is_none() {
                app.overlay = None;
                app.hits.clear();
            }
            // Retirement of a different old Summary display must not swallow
            // the event belonging to a genuinely newer valid browser/Form.
            let newer_browser = app
                .scoped_reviews_v2
                .as_ref()
                .is_some_and(|view| view.owns(app));
            let newer_form = app.form.as_ref().is_some_and(|form| match &form.purpose {
                crate::forms::FormPurpose::PrReviewV2(state) => state.intent().owns(app),
                crate::forms::FormPurpose::PrReviewOperationV3(state) => state.intent().owns(app),
                _ => false,
            });
            return !(newer_browser || newer_form);
        }
    }
    // Explicit current-Main prerequisite: the shared pure retirement helper is
    // newer than the private96b baseline. Do not duplicate its privacy policy
    // or manufacture a Threads module in this isolated source snapshot.
    let original = app
        .scoped_reviews_v2
        .as_ref()
        .map(|view| view.intent().clone())
        .or_else(|| {
            app.form.as_ref().and_then(|form| match &form.purpose {
                crate::forms::FormPurpose::PrReviewV2(state) => Some(state.intent().clone()),
                crate::forms::FormPurpose::PrReviewOperationV3(state) => {
                    Some(state.intent().clone())
                }
                _ => None,
            })
        })
        .or_else(|| {
            app.scoped_reviews_work_v2
                .as_ref()
                .and_then(|work| work.origin().files().cloned())
        })
        .or_else(|| {
            app.scoped_reviews_owner_v2
                .as_ref()
                .and_then(|owner| owner.work.origin().files().cloned())
        });
    if let Some(original) = original {
        let original_form = app.form.as_ref().is_some_and(|form| {
            let intent = match &form.purpose {
                crate::forms::FormPurpose::PrReviewV2(state) => state.intent(),
                crate::forms::FormPurpose::PrReviewOperationV3(state) => state.intent(),
                _ => return false,
            };
            intent.source.same_allocation(&original.source)
                && Arc::ptr_eq(&intent.active, &original.active)
                && Arc::ptr_eq(&intent.navigation, &original.navigation)
        });
        if original
            .source
            .retire_account_mismatch_v14(app, &original.active, &original.navigation)
        {
            if original_form {
                app.form = None;
                app.form_dismissed = true;
            }
            let same_original = |intent: &Arc<ScopedReviewIntentV2>| {
                intent.source.same_allocation(&original.source)
                    && Arc::ptr_eq(&intent.active, &original.active)
            };
            if app
                .scoped_reviews_work_v2
                .as_ref()
                .is_some_and(|work| work.origin().files().is_some_and(same_original))
            {
                if let Some(work) = app.scoped_reviews_work_v2.take() {
                    work.cancel.cancel();
                }
            }
            if let Some(owner) = app
                .scoped_reviews_owner_v2
                .as_ref()
                .filter(|owner| owner.work.origin().files().is_some_and(same_original))
            {
                owner.cancellation().cancel();
                app.scoped_reviews_cancel_v2 = true;
            }
            if app
                .scoped_reviews_v2
                .as_ref()
                .is_some_and(|view| same_original(view.intent()))
            {
                app.scoped_reviews_v2 = None;
            }
            if matches!(app.overlay, Some(Overlay::ScopedReviewsV2 | Overlay::Form)) {
                app.overlay = None;
            }
            app.hits.clear();
            return true; // consume stale key/click before any action
        }
    }
    if app
        .scoped_reviews_v2
        .as_ref()
        .is_some_and(|view| !view.owns(app))
    {
        app.scoped_reviews_v2 = None;
        if app.overlay == Some(Overlay::ScopedReviewsV2) {
            app.overlay = None;
        }
        app.hits.clear();
        return true;
    }
    false
}
pub fn event(app: &mut App, event: Event) {
    if retire_invalid(app) {
        return;
    }
    if app.scoped_review_history_v3.is_some() {
        if app.scoped_reviews_v2.is_some() {
            return;
        }
        let input = app
            .scoped_review_history_v3
            .as_mut()
            .and_then(|view| history_v3::ui::input(view, event));
        if let Some(input) = input {
            history_action_v3(app, input);
        }
        return;
    }
    let Some(view) = app.scoped_reviews_v2.as_mut() else {
        app.overlay = None;
        return;
    };
    let action = match event {
        Event::Key(key) if key.kind != KeyEventKind::Release => browser_v2::key(key),
        Event::Mouse(mouse) => browser_v2::mouse(view, mouse),
        Event::Resize(_, _) => {
            view.clear_layout();
            None
        }
        _ => None,
    };
    if let Some(action) = action {
        browser_action(app, action);
    }
}
pub fn draw(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) {
    if retire_invalid(app) {
        return;
    }
    let busy = app.scoped_reviews_owner_v2.is_some();
    let theme = app.theme();
    if app.scoped_review_history_v3.is_some() {
        if app.scoped_reviews_v2.is_some() {
            return;
        }
        if let Some(view) = &mut app.scoped_review_history_v3 {
            history_v3::ui::draw(frame, area, view, theme, &app.avatar_states, busy);
        }
        return;
    }
    if let Some(view) = &mut app.scoped_reviews_v2 {
        browser_v2::draw(frame, area, view, theme, busy, &app.avatar_states);
    }
}

#[cfg(test)]
mod publication_status_tests {
    use super::*;

    #[test]
    fn pending_reviews_v2_publication_outcomes_render_complete_at_80_columns_with_max_actual_id() {
        use ScopedReviewPublicationStateV2::*;
        let cases = [
            (
                ConfirmedSnapshotRetired,
                "Review #18446744073709551615 effect confirmed; snapshot retired; refresh",
            ),
            (
                NotStarted,
                "Review publication not started • draft/protection kept; no resend",
            ),
            (
                Uncertain,
                "Review outcome unknown • inspect explicitly; no resend • draft/protection kept",
            ),
            (
                ConfirmedButJournalFailed,
                "Review effect confirmed • journal failed; protection kept; refresh explicitly",
            ),
            (
                ConfirmedButSourceUnavailable,
                "Review effect confirmed • source unavailable; hidden; refresh explicitly",
            ),
        ];
        for (state, expected) in cases {
            let status = publication_status(state, Some(u64::MAX));
            assert_eq!(status, expected);
            assert!(crate::editor::display_width(&status) <= 80);
            let mut app = App {
                status,
                ..Default::default()
            };
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let rows: Vec<String> = (0..24)
                .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect())
                .collect();
            assert!(
                rows.iter().any(|row| row.trim_end() == expected),
                "complete outcome/recovery instruction missing: {rows:?}"
            );
        }
    }
}
