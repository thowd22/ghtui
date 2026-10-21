//! Closed work/result on the existing Inbox queue. Display retirement does
//! not keep old Background ownership, source permission or cancel authority.
use crate::{
    app::{App, Overlay, PrUiAction, View},
    drafts::{
        ConversationOpenReadCutoffV20, DraftClient, DraftPublicationFenceV2, ReadCutoffFailureV20,
    },
    forms::{Form, FormPurpose},
    pr_actions::{PrCommand, PrPlan},
    pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    providers::{
        gh::{GithubErrorKind, GithubProvider},
        runner::CancellationToken,
    },
    scoped_pr_collaboration_draft_v17::OpenedConversationDraftV17,
    scoped_pr_collaboration_plan_v17::{
        ConversationPreviewV17, ConversationPublicationStateV17, ConversationPublicationV17,
    },
    scoped_pr_collaboration_ui_v17::{self as ui, ConversationComposerRequestV17},
};
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
#[path = "scoped_pr_state_native_v68.rs"]
mod state_v68;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversationPhaseKindV17 {
    StateSourceSyncV80,
    RefreshExactV19,
    MetadataSectionV51,
    Open,
    NewAction,
    Preview,
    Confirm,
    History,
    Inspect,
    Acknowledge,
}
enum KindV17 {
    StateSourceSyncV80(Arc<crate::state_inbox_sync_v76::StateSourceSyncV76>),
    StateV68(state_v68::StateWorkV68),
    IssueOpenV87 {
        original: Arc<PrCollaborationOpenIntentV17>,
        command: crate::pr_issue_links_v87::IssueLinkCommandV87,
        closed_draft_v20: Option<Arc<ui::pure_close_v20::ClosedDraftTicketV20>>,
    },
    IssuePreviewV87(Arc<ui::issue_link_v87::IssueLinkRequestV87>),
    IssueConfirmV87(Arc<ui::issue_link_v87::IssueLinkRequestV87>),
    IssueNewV87(Arc<ui::issue_link_v87::IssueLinkRequestV87>),
    IssueHistoryV87 {
        original: Arc<PrCollaborationOpenIntentV17>,
        command: crate::pr_issue_links_v87::IssueLinkCommandV87,
        from: Option<Arc<ui::issue_link_v87::history::IssueHistoryOpenRequestV87>>,
    },
    IssueInspectV87(Arc<ui::issue_link_v87::history::IssueHistoryRequestV87>),
    IssueAcknowledgeV87(Arc<ui::issue_link_v87::history::IssueHistoryRequestV87>),
    MetadataSectionV51(Arc<PrCollaborationOpenIntentV17>),
    RefreshExactV19(
        Arc<crate::scoped_pr_collaboration_refresh_v19::return_v19::ConversationRefreshRequestV19>,
    ),
    Open {
        original: Arc<PrCollaborationOpenIntentV17>,
        command: PrCommand,
        closed_draft_v20: Option<Arc<ui::pure_close_v20::ClosedDraftTicketV20>>,
    },
    MetadataOpenV25 {
        original: Arc<PrCollaborationOpenIntentV17>,
        field: crate::providers::gh::PrMetadataTextFieldV25,
        closed_draft_v20: Option<Arc<ui::pure_close_v20::ClosedDraftTicketV20>>,
    },
    MembersOpenV48 {
        original: Arc<PrCollaborationOpenIntentV17>,
        command: crate::pr_metadata_actions::PrMetadataCommand,
        closed_draft_v20: Option<Arc<ui::pure_close_v20::ClosedDraftTicketV20>>,
    },
    MembersPreviewV48(Arc<ui::metadata_members_v48::MetadataMembersRequestV48>),
    MembersConfirmV48(Arc<ui::metadata_members_v48::MetadataMembersRequestV48>),
    MembersNewV52(Arc<ui::metadata_members_v48::MetadataMembersNewRequestV52>),
    MembersHistoryV48 {
        original: Arc<PrCollaborationOpenIntentV17>,
        from: Option<Arc<ui::metadata_members_v48::history::MembersHistoryOpenRequestV48>>,
    },
    MembersInspectV48(Arc<ui::metadata_members_v48::history::MembersHistoryRequestV48>),
    MembersAcknowledgeV48(Arc<ui::metadata_members_v48::history::MembersHistoryRequestV48>),
    MetadataPreviewV25(Arc<ui::metadata_v25::MetadataRequestV25>),
    MetadataConfirmV25(Arc<ui::metadata_v25::MetadataRequestV25>),
    BaseOpenV32 {
        original: Arc<PrCollaborationOpenIntentV17>,
        closed_draft_v20: Option<Arc<ui::pure_close_v20::ClosedDraftTicketV20>>,
    },
    BasePreviewV32(Arc<ui::metadata_base_v32::BaseRequestV32>),
    BaseConfirmV32(Arc<ui::metadata_base_v32::BaseRequestV32>),
    BaseHistoryV32 {
        original: Arc<PrCollaborationOpenIntentV17>,
        from: Option<Arc<ui::metadata_base_v32::history::BaseHistoryOpenRequestV32>>,
    },
    BaseInspectV32(Arc<ui::metadata_base_v32::history::BaseHistoryRequestV32>),
    BaseAcknowledgeV32(Arc<ui::metadata_base_v32::history::BaseHistoryRequestV32>),
    MetadataHistoryV25 {
        original: Arc<PrCollaborationOpenIntentV17>,
        field: crate::providers::gh::PrMetadataTextFieldV25,
        from: Option<Arc<ui::metadata_v25::history::MetadataHistoryOpenRequestV25>>,
    },
    MetadataInspectV25(Arc<ui::metadata_v25::history::MetadataHistoryRequestV25>),
    MetadataAcknowledgeV25(Arc<ui::metadata_v25::history::MetadataHistoryRequestV25>),
    NewAction(Arc<ui::ConversationNewActionRequestV18>),
    Preview(Arc<ConversationComposerRequestV17>),
    Confirm(Arc<ConversationComposerRequestV17>),
    History {
        original: Arc<PrCollaborationOpenIntentV17>,
        from: Option<Arc<ui::ConversationHistoryOpenRequestV17>>,
    },
    Inspect(Arc<crate::scoped_pr_collaboration_history_v17::ConversationHistoryRequestV17>),
    Acknowledge(Arc<crate::scoped_pr_collaboration_history_v17::ConversationHistoryRequestV17>),
}
pub struct ScopedPrCollaborationWorkV17 {
    kind: KindV17,
    cancel: CancellationToken,
    deadline: Instant,
}
/// Closed physical-Inbox work origin. State source work has NO PR parent.
/// Borrowed data only; neither arm grants a read/write/ACK permission.
#[derive(Clone, Copy)]
pub(crate) enum CollaborationWorkOriginV78<'a> {
    PrPage(&'a Arc<PrCollaborationOpenIntentV17>),
    StateInbox(&'a Arc<crate::state_inbox_sync_v76::StateSourceSyncV76>),
}
impl<'a> CollaborationWorkOriginV78<'a> {
    pub(crate) fn scope(self) -> &'a crate::storage::Scope {
        match self {
            Self::PrPage(p) => p.scope(),
            Self::StateInbox(s) => s.original().scope(),
        }
    }
    pub(crate) fn database(self) -> &'a Path {
        match self {
            Self::PrPage(p) => p.database(),
            Self::StateInbox(s) => s.original().database(),
        }
    }
    #[cfg(test)]
    pub(crate) fn cancellation(self) -> &'a CancellationToken {
        match self {
            Self::PrPage(p) => p.cancellation(),
            Self::StateInbox(s) => s.original().cancellation(),
        }
    }
}
impl ScopedPrCollaborationWorkV17 {
    pub fn phase(&self) -> ConversationPhaseKindV17 {
        match self.kind {
            KindV17::StateSourceSyncV80(_) => ConversationPhaseKindV17::StateSourceSyncV80,
            KindV17::StateV68(ref state) => state.phase(),
            KindV17::IssueOpenV87 { .. } => ConversationPhaseKindV17::Open,
            KindV17::IssuePreviewV87(_) => ConversationPhaseKindV17::Preview,
            KindV17::IssueConfirmV87(_) => ConversationPhaseKindV17::Confirm,
            KindV17::IssueNewV87(_) => ConversationPhaseKindV17::NewAction,
            KindV17::IssueHistoryV87 { .. } => ConversationPhaseKindV17::History,
            KindV17::IssueInspectV87(_) => ConversationPhaseKindV17::Inspect,
            KindV17::IssueAcknowledgeV87(_) => ConversationPhaseKindV17::Acknowledge,
            KindV17::MetadataSectionV51(_) => ConversationPhaseKindV17::MetadataSectionV51,
            KindV17::RefreshExactV19(_) => ConversationPhaseKindV17::RefreshExactV19,
            KindV17::Open { .. }
            | KindV17::MetadataOpenV25 { .. }
            | KindV17::MembersOpenV48 { .. }
            | KindV17::BaseOpenV32 { .. } => ConversationPhaseKindV17::Open,
            KindV17::MetadataPreviewV25(_) => ConversationPhaseKindV17::Preview,
            KindV17::MembersPreviewV48(_) => ConversationPhaseKindV17::Preview,
            KindV17::MembersConfirmV48(_) => ConversationPhaseKindV17::Confirm,
            KindV17::MembersNewV52(_) => ConversationPhaseKindV17::NewAction,
            KindV17::MetadataConfirmV25(_) => ConversationPhaseKindV17::Confirm,
            KindV17::BasePreviewV32(_) => ConversationPhaseKindV17::Preview,
            KindV17::BaseConfirmV32(_) => ConversationPhaseKindV17::Confirm,
            KindV17::BaseHistoryV32 { .. } => ConversationPhaseKindV17::History,
            KindV17::BaseInspectV32(_) => ConversationPhaseKindV17::Inspect,
            KindV17::BaseAcknowledgeV32(_) => ConversationPhaseKindV17::Acknowledge,
            KindV17::MembersHistoryV48 { .. } => ConversationPhaseKindV17::History,
            KindV17::MembersInspectV48(_) => ConversationPhaseKindV17::Inspect,
            KindV17::MembersAcknowledgeV48(_) => ConversationPhaseKindV17::Acknowledge,
            KindV17::MetadataHistoryV25 { .. } => ConversationPhaseKindV17::History,
            KindV17::MetadataInspectV25(_) => ConversationPhaseKindV17::Inspect,
            KindV17::MetadataAcknowledgeV25(_) => ConversationPhaseKindV17::Acknowledge,
            KindV17::NewAction(_) => ConversationPhaseKindV17::NewAction,
            KindV17::Preview(_) => ConversationPhaseKindV17::Preview,
            KindV17::Confirm(_) => ConversationPhaseKindV17::Confirm,
            KindV17::History { .. } => ConversationPhaseKindV17::History,
            KindV17::Inspect(_) => ConversationPhaseKindV17::Inspect,
            KindV17::Acknowledge(_) => ConversationPhaseKindV17::Acknowledge,
        }
    }
    pub(crate) fn original(&self) -> CollaborationWorkOriginV78<'_> {
        let original = match &self.kind {
            KindV17::StateSourceSyncV80(source) => {
                return CollaborationWorkOriginV78::StateInbox(source)
            }
            KindV17::StateV68(state) => state.original(),
            KindV17::IssueOpenV87 { original, .. } => original,
            KindV17::IssueHistoryV87 { original, .. } => original,
            KindV17::IssueInspectV87(r) | KindV17::IssueAcknowledgeV87(r) => &r.view.original,
            KindV17::IssuePreviewV87(r) | KindV17::IssueConfirmV87(r) | KindV17::IssueNewV87(r) => {
                r.intent.source.opening()
            }
            KindV17::MetadataSectionV51(original) => original,
            KindV17::RefreshExactV19(r) => r.original(),
            KindV17::Open { original, .. } => original,
            KindV17::MetadataOpenV25 { original, .. } => original,
            KindV17::MembersOpenV48 { original, .. } => original,
            KindV17::MembersNewV52(r) => r.original_request.intent.source.opening(),
            KindV17::MembersPreviewV48(request) | KindV17::MembersConfirmV48(request) => {
                request.intent.source.opening()
            }
            KindV17::BaseOpenV32 { original, .. } => original,
            KindV17::BasePreviewV32(r) | KindV17::BaseConfirmV32(r) => r.intent.source.opening(),
            KindV17::BaseHistoryV32 { original, .. } => original,
            KindV17::BaseInspectV32(r) | KindV17::BaseAcknowledgeV32(r) => &r.view.original,
            KindV17::MembersHistoryV48 { original, .. } => original,
            KindV17::MembersInspectV48(r) | KindV17::MembersAcknowledgeV48(r) => &r.view.original,
            KindV17::MetadataHistoryV25 { original, .. } => original,
            KindV17::MetadataInspectV25(r) | KindV17::MetadataAcknowledgeV25(r) => &r.view.original,
            KindV17::MetadataPreviewV25(r) | KindV17::MetadataConfirmV25(r) => {
                r.intent.source.opening()
            }
            KindV17::NewAction(r) => r.intent.source.opening(),
            KindV17::Preview(r) | KindV17::Confirm(r) => r.intent().source.opening(),
            KindV17::History { original, .. } => original,
            KindV17::Inspect(r) | KindV17::Acknowledge(r) => &r.view.original,
        };
        CollaborationWorkOriginV78::PrPage(original)
    }
    pub(crate) fn request(&self) -> Option<&Arc<ConversationComposerRequestV17>> {
        match &self.kind {
            KindV17::Preview(r) | KindV17::Confirm(r) => Some(r),
            _ => None,
        }
    }
    pub(crate) fn refresh_request_v19(
        &self,
    ) -> Option<
        &Arc<crate::scoped_pr_collaboration_refresh_v19::return_v19::ConversationRefreshRequestV19>,
    > {
        match &self.kind {
            KindV17::RefreshExactV19(r) => Some(r),
            _ => None,
        }
    }
    pub(crate) fn owns_form(&self, form: &Form) -> bool {
        match &self.kind {
            KindV17::StateV68(state) => state.owns_form(form),
            KindV17::IssuePreviewV87(r) | KindV17::IssueConfirmV87(r) | KindV17::IssueNewV87(r) => {
                r.owns(form)
            }
            KindV17::RefreshExactV19(r) => r.owns_form(form),
            KindV17::Preview(r) | KindV17::Confirm(r) => r.owns_presentation(form),
            KindV17::MetadataPreviewV25(r) | KindV17::MetadataConfirmV25(r) => r.owns(form),
            KindV17::MembersPreviewV48(r) | KindV17::MembersConfirmV48(r) => r.owns(form),
            KindV17::MembersNewV52(r) => r.owns(form),
            KindV17::BasePreviewV32(r) | KindV17::BaseConfirmV32(r) => r.owns(form),
            KindV17::BaseHistoryV32 { from: Some(r), .. } => r.matches(form),
            KindV17::BaseInspectV32(r) | KindV17::BaseAcknowledgeV32(r) => r.owns(form),
            KindV17::MembersHistoryV48 { from: Some(r), .. } => r.matches(form),
            KindV17::IssueHistoryV87 { from: Some(r), .. } => r.matches(form),
            KindV17::IssueInspectV87(r) | KindV17::IssueAcknowledgeV87(r) => r.owns(form),
            KindV17::MembersInspectV48(r) | KindV17::MembersAcknowledgeV48(r) => r.owns(form),
            KindV17::MetadataHistoryV25 { from: Some(r), .. } => r.matches(form),
            KindV17::MetadataInspectV25(r) | KindV17::MetadataAcknowledgeV25(r) => r.owns(form),
            KindV17::History { from: Some(r), .. } => r.owns(form),
            KindV17::NewAction(r) => r.owns(form),
            KindV17::Inspect(r) | KindV17::Acknowledge(r) => ui::history_owns(r, form),
            _ => false,
        }
    }
    pub fn host(&self) -> &str {
        &self.original().scope().host
    }
    pub(crate) fn matches(&self, app: &App) -> bool {
        let original = match self.original() {
            CollaborationWorkOriginV78::StateInbox(source) => {
                return source.matches_capture_v80(app)
            }
            CollaborationWorkOriginV78::PrPage(original) => original,
        };
        if let KindV17::RefreshExactV19(r) = &self.kind {
            return r.matches_capture(app);
        }
        if let KindV17::MetadataSectionV51(original) = &self.kind {
            // ONLY this read-only section tolerates viewport movement. All
            // write/Form purposes below keep exact original row/scroll pins.
            return app.view == View::Inbox
                && original.owns_metadata_section_presentation_v266(app)
                && app.form.is_none()
                && app.overlay.is_none()
                && !app.scoped_pr_collaboration_cancel_v17
                && app
                    .scoped_pr_collaboration_work_v17
                    .as_ref()
                    .is_none_or(|pending| std::ptr::eq(pending.as_ref(), self))
                && !newer_intent(app);
        }
        if app.view != View::Inbox || !original.owns_presentation(app) {
            return false;
        }
        match &self.kind {
            KindV17::StateSourceSyncV80(_) => false,
            KindV17::StateV68(state) => state.matches(app),
            KindV17::IssueOpenV87 { .. } => app.form.is_none() && app.overlay.is_none(),
            KindV17::IssuePreviewV87(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
                    && app.form.as_ref().is_some_and(|f| f.publication.is_none())
            }
            KindV17::IssueNewV87(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app
                        .form
                        .as_ref()
                        .is_some_and(|f| r.matches(f) && ui::issue_link_v87::new_ready(f))
            }
            KindV17::IssueConfirmV87(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches_confirm(f))
            }
            KindV17::MetadataSectionV51(_) => app.form.is_none() && app.overlay.is_none(),
            KindV17::RefreshExactV19(_) => false,
            KindV17::Open { .. }
            | KindV17::MetadataOpenV25 { .. }
            | KindV17::MembersOpenV48 { .. }
            | KindV17::BaseOpenV32 { .. } => app.form.is_none() && app.overlay.is_none(),
            KindV17::MembersPreviewV48(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|form| r.matches(form))
            }
            KindV17::MembersNewV52(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|form| r.matches(form))
            }
            KindV17::MembersConfirmV48(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app
                        .form
                        .as_ref()
                        .is_some_and(|form| r.matches_confirm(form))
            }
            KindV17::MetadataPreviewV25(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::MetadataConfirmV25(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches_confirm(f))
            }
            KindV17::BasePreviewV32(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::BaseConfirmV32(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches_confirm(f))
            }
            KindV17::MembersHistoryV48 { from: None, .. } => {
                app.form.is_none() && app.overlay.is_none()
            }
            KindV17::IssueHistoryV87 { from: None, .. } => {
                app.form.is_none() && app.overlay.is_none()
            }
            KindV17::IssueHistoryV87 { from: Some(r), .. } => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::IssueInspectV87(r) | KindV17::IssueAcknowledgeV87(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::MembersHistoryV48 { from: Some(r), .. } => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::MembersInspectV48(r) | KindV17::MembersAcknowledgeV48(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::MetadataHistoryV25 { from: None, .. } => {
                app.form.is_none() && app.overlay.is_none()
            }
            KindV17::BaseHistoryV32 { from: None, .. } => {
                app.form.is_none() && app.overlay.is_none()
            }
            KindV17::BaseHistoryV32 { from: Some(r), .. } => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::BaseInspectV32(r) | KindV17::BaseAcknowledgeV32(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::MetadataHistoryV25 { from: Some(r), .. } => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::MetadataInspectV25(r) | KindV17::MetadataAcknowledgeV25(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::NewAction(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::History { from: None, .. } => app.form.is_none() && app.overlay.is_none(),
            KindV17::History { from: Some(r), .. } => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches(f))
            }
            KindV17::Inspect(r) | KindV17::Acknowledge(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| ui::history_matches(r, f))
            }
            KindV17::Preview(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches_form(f))
            }
            KindV17::Confirm(r) => {
                !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app.form.as_ref().is_some_and(|f| r.matches_confirm(f))
            }
        }
    }
}
pub struct ScopedPrCollaborationOwnerV17 {
    work: Arc<ScopedPrCollaborationWorkV17>,
    deadline: Instant,
    publication: Mutex<Option<DraftPublicationFenceV2>>,
    open_cutoff_v20: Mutex<Option<Arc<ConversationOpenReadCutoffV20>>>,
    // This may remain ONLY in App after generic replacement, never as active
    // Background ownership. Retains the original page bytes, not another copy.
    display_retired: AtomicBool,
}
impl ScopedPrCollaborationOwnerV17 {
    pub(crate) fn new(
        work: Arc<ScopedPrCollaborationWorkV17>,
        provider: &GithubProvider,
    ) -> Result<Arc<Self>, String> {
        let deadline = provider
            .original_pr_read_deadline_v13()
            .map_or(work.deadline, |d| d.min(work.deadline));
        if work.cancel.is_cancelled() || Instant::now() >= deadline {
            return Err("Original Conversation phase expired before enqueue".into());
        }
        Ok(Arc::new(Self {
            work,
            deadline,
            publication: Mutex::new(None),
            open_cutoff_v20: Mutex::new(None),
            display_retired: AtomicBool::new(false),
        }))
    }
    pub(crate) fn work(&self) -> &Arc<ScopedPrCollaborationWorkV17> {
        &self.work
    }
    pub(crate) fn original(&self) -> CollaborationWorkOriginV78<'_> {
        self.work.original()
    }
    pub(crate) fn cancellation(&self) -> &CancellationToken {
        &self.work.cancel
    }
    pub(crate) fn expired(&self) -> bool {
        self.cancellation().is_cancelled() || Instant::now() >= self.deadline
    }
    pub(crate) fn owns(&self, app: &App) -> bool {
        let current_members = match &self.work.kind {
            KindV17::StateSourceSyncV80(source) => {
                if app
                    .scoped_pr_collaboration_owner_v17
                    .as_ref()
                    .is_some_and(|current| std::ptr::eq(current.as_ref(), self))
                {
                    !self.expired() && source.owns(app)
                } else {
                    self.work.matches(app)
                }
            }
            KindV17::StateV68(state) => state
                .matches_owned_inflight_v74(self, app)
                .unwrap_or_else(|| self.work.matches(app)),
            KindV17::IssueConfirmV87(request)
                if app
                    .scoped_pr_collaboration_owner_v17
                    .as_ref()
                    .is_some_and(|current| std::ptr::eq(current.as_ref(), self)) =>
            {
                !self.cancellation().is_cancelled()
                    && app.view == View::Inbox
                    && matches!(self.original(), CollaborationWorkOriginV78::PrPage(original) if original.owns_presentation(app))
                    && !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app
                        .form
                        .as_ref()
                        .is_some_and(|form| request.matches_inflight(form))
            }
            KindV17::MembersConfirmV48(request)
                if app
                    .scoped_pr_collaboration_owner_v17
                    .as_ref()
                    .is_some_and(|current| std::ptr::eq(current.as_ref(), self)) =>
            {
                // This Arc is installed only AFTER accepted try_send. Keep
                // pre-owner/admission on Work.matches (!used), while this
                // exact current busy Form survives the worker's own consume.
                !self.cancellation().is_cancelled()
                    && app.view == View::Inbox
                    && matches!(self.original(),CollaborationWorkOriginV78::PrPage(original) if original.owns_presentation(app))
                    && !app.form_dismissed
                    && app.overlay == Some(Overlay::Form)
                    && app
                        .form
                        .as_ref()
                        .is_some_and(|form| request.matches_inflight_confirm_v81(form))
            }
            _ => self.work.matches(app),
        };
        !self.display_retired.load(Ordering::Acquire)
            && current_members
            && self.open_cutoff_v20.lock().is_ok_and(|slot| {
                slot.as_ref().is_none_or(|cutoff| {
                    cutoff.check_original() != Err(ReadCutoffFailureV20::Unavailable)
                })
            })
    }
    pub(crate) fn display_retired(&self) -> bool {
        self.display_retired.load(Ordering::Acquire)
    }
    pub(crate) fn waiting_open_drafts_v20(&self) -> bool {
        self.open_cutoff_v20.lock().is_ok_and(|slot| {
            slot.as_ref()
                .is_some_and(|cutoff| cutoff.pending_at_capture())
        })
    }
    pub(crate) fn capture_publication(
        &self,
        database: &Path,
        drafts: &DraftClient,
    ) -> Result<(), String> {
        if let CollaborationWorkOriginV78::StateInbox(source) = self.original() {
            return if database == source.original().database() && !self.expired() {
                Ok(()) // No DraftWriter/ACK/claim/SQL action in source recovery.
            } else {
                Err("Original State source database/clock changed".into())
            };
        }
        if self.work.phase() == ConversationPhaseKindV17::Open
            || matches!(
                &self.work.kind,
                KindV17::StateV68(state_v68::StateWorkV68::Preview(_))
            )
        {
            if database != self.original().database() || self.expired() {
                return Err("Original Conversation source/clock changed".into());
            }
            let mut captured = self
                .open_cutoff_v20
                .lock()
                .map_err(|_| "Conversation draft read cutoff unavailable")?;
            if captured.is_some() {
                return Err("Conversation draft read cutoff already captured".into());
            }
            *captured = Some(Arc::new(
                drafts
                    .capture_conversation_open_cutoff_v20(
                        database,
                        self.cancellation(),
                        self.deadline,
                    )
                    .map_err(|_| "Conversation draft read cutoff unavailable")?,
            ));
            return Ok(());
        }
        if self.work.phase() != ConversationPhaseKindV17::Confirm {
            return Ok(());
        }
        let key = match &self.work.kind {
            KindV17::StateV68(state) => state
                .confirm_key()
                .ok_or("Exact State publication request required")?,
            KindV17::IssueConfirmV87(request) => {
                &request
                    .preview
                    .as_ref()
                    .ok_or("Fresh exact Issue Preview missing")?
                    .key
            }
            KindV17::MetadataConfirmV25(request) => &request.intent.key,
            KindV17::MembersConfirmV48(request) => request
                .key
                .as_ref()
                .ok_or("Fresh exact member Preview key missing")?,
            KindV17::BaseConfirmV32(request) => &request.intent.key,
            _ => {
                &self
                    .work
                    .request()
                    .ok_or("Exact original publication request missing")?
                    .intent()
                    .key
            }
        };
        if database != self.original().database() || self.expired() {
            return Err("Original Conversation source/clock changed".into());
        }
        let fence = drafts
            .capture_publication_v2(
                database,
                self.original().scope(),
                key,
                self.cancellation(),
                self.deadline,
            )
            .map_err(|e| e.to_string())?;
        let mut captured = self
            .publication
            .lock()
            .map_err(|_| "Conversation ACK slot unavailable")?;
        if captured.is_some() {
            return Err("Conversation ACK cutoff already captured".into());
        }
        *captured = Some(fence);
        Ok(())
    }
}
enum ResultKindV17 {
    StateSourceV80(crate::storage::CommittedStateSourceV80),
    StateV68(state_v68::StateResultV68),
    IssuePreviewV87(Arc<ui::issue_link_v87::IssueLinkRequestV87>,Arc<crate::scoped_pr_issue_links_v87::IssueLinkPreviewV87>),
    IssueNoChangeV102(Arc<ui::issue_link_v87::IssueLinkRequestV87>,crate::providers::gh::IssueNoChangeV102),
    IssueUnsupportedV102(Arc<PrCollaborationOpenIntentV17>,crate::providers::gh::IssueHostUnsupportedV102),
    MetadataSectionV51(Box<crate::scoped_pr_collaboration_source_v17::metadata_context_v51::PreparedMetadataSectionV51>),
    MembersNewV52(
        Arc<ui::metadata_members_v48::MetadataMembersNewRequestV52>,
        Arc<ui::metadata_members_v48::MetadataMembersPreviewV48>,
    ),
    RefreshedV19(crate::scoped_pr_collaboration_refresh_v19::AdmittedConversationPageV19),
    Open(Box<Form>),
    LocalDraftReadFailureV20(ReadCutoffFailureV20),
    Preview(
        Arc<ConversationComposerRequestV17>,
        Arc<ConversationPreviewV17>,
    ),
    MembersPreviewV48(
        Arc<ui::metadata_members_v48::MetadataMembersRequestV48>,
        Arc<ui::metadata_members_v48::MetadataMembersPreviewV48>,
    ),
    MetadataPreviewV25(
        Arc<ui::metadata_v25::MetadataRequestV25>,
        Arc<ui::metadata_v25::MetadataPreviewV25>,
    ),
    MetadataNoChangeV65(Arc<ui::metadata_v25::MetadataRequestV25>),
    BasePreviewV32(
        Arc<ui::metadata_base_v32::BaseRequestV32>,
        Arc<ui::metadata_base_v32::BasePreviewV32>,
    ),
    BaseNoChangeV39(
        Arc<ui::metadata_base_v32::BaseRequestV32>,
        Arc<crate::scoped_pr_collaboration_source_v17::metadata_base_phase_v32::BaseNoChangeV39>,
    ),
    Publication(ConversationPublicationV17),
    History(Box<Form>),
    Acknowledged(crate::pr_write_journal::PrPublicationReference, usize),
    Failure {
        account: bool,
        permission: bool,
    },
}
pub struct PreparedPrCollaborationV17 {
    owner: Arc<ScopedPrCollaborationOwnerV17>,
    result: ResultKindV17,
}
impl PreparedPrCollaborationV17 {
    pub(crate) fn matches(&self, owner: &Arc<ScopedPrCollaborationOwnerV17>) -> bool {
        Arc::ptr_eq(&self.owner, owner)
            && match (&owner.work.kind, &self.result) {
                (KindV17::StateSourceSyncV80(source), ResultKindV17::StateSourceV80(result)) => {
                    result.matches_original(source)
                }
                (KindV17::StateSourceSyncV80(_), ResultKindV17::Failure { .. }) => true,
                (KindV17::StateSourceSyncV80(_), _) | (_, ResultKindV17::StateSourceV80(_)) => {
                    false
                }
                (KindV17::StateV68(state), result) => state.result_matches(result),
                (_, ResultKindV17::StateV68(_)) => false,
                (
                    KindV17::IssueOpenV87 { original, .. },
                    ResultKindV17::IssueUnsupportedV102(actual, _),
                ) => Arc::ptr_eq(original, actual),
                (_, ResultKindV17::IssueUnsupportedV102(..)) => false,
                (
                    KindV17::IssueOpenV87 {
                        original, command, ..
                    },
                    ResultKindV17::Open(form),
                ) => ui::issue_link_v87::state(form).is_some_and(|s| {
                    s.intent.command == *command && Arc::ptr_eq(s.intent.source.opening(), original)
                }),
                (_, ResultKindV17::Open(form)) if ui::issue_link_v87::state(form).is_some() => {
                    false
                }
                (
                    KindV17::IssuePreviewV87(expected) | KindV17::IssueNewV87(expected),
                    ResultKindV17::IssuePreviewV87(request, preview),
                ) => {
                    Arc::ptr_eq(expected, request)
                        && Arc::ptr_eq(&preview.intent, &request.selection)
                        && *preview.values == *request.values
                        && (!matches!(owner.work.kind, KindV17::IssueNewV87(_))
                            || (!preview.needs_new && preview.lineage.is_some()))
                }
                (_, ResultKindV17::IssuePreviewV87(..)) => false,
                (
                    KindV17::IssuePreviewV87(expected),
                    ResultKindV17::IssueNoChangeV102(actual, _),
                ) => Arc::ptr_eq(expected, actual),
                (_, ResultKindV17::IssueNoChangeV102(..)) => false,
                (
                    KindV17::IssuePreviewV87(_) | KindV17::IssueNewV87(_),
                    ResultKindV17::Failure { .. },
                ) => true,
                (KindV17::IssuePreviewV87(_) | KindV17::IssueNewV87(_), _) => false,
                (KindV17::IssueConfirmV87(request), ResultKindV17::Publication(outcome)) => {
                    request.preview.as_ref().is_some_and(|preview| {
                        preview
                            .seed
                            .write(&preview.values)
                            .and_then(|write| write.digest())
                            .is_ok_and(|digest| digest == outcome.reference.payload_sha256)
                    })
                }
                (KindV17::IssueConfirmV87(_), ResultKindV17::Failure { .. }) => true,
                (KindV17::IssueConfirmV87(_), _) => false,
                (
                    KindV17::MetadataSectionV51(original),
                    ResultKindV17::MetadataSectionV51(section),
                ) => section.matches_original(original),
                (_, ResultKindV17::MetadataSectionV51(_)) => false,
                (
                    KindV17::MembersNewV52(expected),
                    ResultKindV17::MembersNewV52(request, fresh),
                ) => {
                    Arc::ptr_eq(expected, request)
                        && Arc::ptr_eq(&fresh.intent, &request.original_request.intent)
                        && Arc::ptr_eq(&fresh.plan, &request.original_request.plan)
                        && Arc::ptr_eq(&fresh.values, &request.original_request.values)
                        && !fresh.notice_v52
                        && fresh.lineage_v52.is_some()
                }
                (KindV17::MembersNewV52(_), ResultKindV17::Failure { .. }) => true,
                (KindV17::MembersNewV52(_), _) | (_, ResultKindV17::MembersNewV52(..)) => false,
                (
                    KindV17::MetadataPreviewV25(expected),
                    ResultKindV17::MetadataNoChangeV65(request),
                ) => Arc::ptr_eq(expected, request),
                (_, ResultKindV17::MetadataNoChangeV65(_)) => false,
                (
                    KindV17::BasePreviewV32(expected),
                    ResultKindV17::BaseNoChangeV39(request, proof),
                ) => {
                    Arc::ptr_eq(expected, request)
                        && proof.matches(
                            &request.intent.source,
                            &request.selection,
                            owner.cancellation(),
                            owner.deadline,
                        )
                }
                (_, ResultKindV17::BaseNoChangeV39(..)) => false,
                (
                    KindV17::IssueHistoryV87 {
                        original, command, ..
                    },
                    ResultKindV17::History(form),
                ) => ui::issue_link_v87::history::state(form).is_some_and(|s| {
                    Arc::ptr_eq(&s.view.original, original) && s.view.command == *command
                }),
                (KindV17::IssueInspectV87(request), ResultKindV17::History(form)) => {
                    ui::issue_link_v87::history::state(form).is_some_and(|s| {
                        Arc::ptr_eq(&s.view.original, &request.view.original)
                            && Arc::ptr_eq(&s.view.snapshot, &request.view.snapshot)
                            && s.view.command == request.view.command
                    })
                }
                (
                    KindV17::IssueAcknowledgeV87(request),
                    ResultKindV17::Acknowledged(reference, changed),
                ) => {
                    *changed == 1
                        && request
                            .view
                            .current
                            .as_ref()
                            .is_some_and(|c| c.reference() == reference)
                }
                (_, ResultKindV17::History(form))
                    if ui::issue_link_v87::history::state(form).is_some() =>
                {
                    false
                }
                (KindV17::MembersHistoryV48 { original, .. }, ResultKindV17::History(form)) => {
                    ui::metadata_members_v48::history::state(form)
                        .is_some_and(|state| Arc::ptr_eq(&state.view.original, original))
                }
                (KindV17::MembersInspectV48(request), ResultKindV17::History(form)) => {
                    ui::metadata_members_v48::history::state(form).is_some_and(|state| {
                        Arc::ptr_eq(&state.view.original, &request.view.original)
                            && Arc::ptr_eq(&state.view.snapshot, &request.view.snapshot)
                    })
                }
                (
                    KindV17::MembersAcknowledgeV48(request),
                    ResultKindV17::Acknowledged(reference, changed),
                ) => {
                    *changed == 1
                        && request
                            .view
                            .current
                            .as_ref()
                            .is_some_and(|current| current.reference() == reference)
                }
                (_, ResultKindV17::History(form))
                    if ui::metadata_members_v48::history::state(form).is_some() =>
                {
                    false
                }
                (
                    KindV17::MembersOpenV48 {
                        original, command, ..
                    },
                    ResultKindV17::Open(form),
                ) => ui::metadata_members_v48::state(form).is_some_and(|state| {
                    state.intent.command == *command
                        && Arc::ptr_eq(state.intent.source.opening(), original)
                }),
                (_, ResultKindV17::Open(form))
                    if ui::metadata_members_v48::state(form).is_some() =>
                {
                    false
                }
                (
                    KindV17::MembersPreviewV48(expected),
                    ResultKindV17::MembersPreviewV48(request, preview),
                ) => {
                    Arc::ptr_eq(expected, request)
                        && Arc::ptr_eq(&preview.intent, &request.intent)
                        && Arc::ptr_eq(&preview.plan, &request.plan)
                        && Arc::ptr_eq(&preview.values, &request.values)
                }
                (KindV17::MembersPreviewV48(_), ResultKindV17::Failure { .. }) => true,
                (_, ResultKindV17::MembersPreviewV48(..)) | (KindV17::MembersPreviewV48(_), _) => {
                    false
                }
                (KindV17::MembersConfirmV48(request), ResultKindV17::Publication(outcome)) => {
                    request.preview.as_ref().is_some_and(|preview| {
                        preview
                            .seed
                            .write(&preview.values)
                            .and_then(|write| write.digest())
                            .is_ok_and(|digest| digest == outcome.reference.payload_sha256)
                    })
                }
                (
                    KindV17::MembersConfirmV48(_),
                    ResultKindV17::Open(_)
                    | ResultKindV17::History(_)
                    | ResultKindV17::Preview(..)
                    | ResultKindV17::MetadataPreviewV25(..)
                    | ResultKindV17::BasePreviewV32(..)
                    | ResultKindV17::Acknowledged(..)
                    | ResultKindV17::RefreshedV19(_),
                ) => false,
                (KindV17::Open { .. }, ResultKindV17::Open(form)) => {
                    ui::metadata_members_v48::state(form).is_none()
                        && ui::issue_link_v87::state(form).is_none()
                        && ui::metadata_base_v32::state(form).is_none()
                }
                (KindV17::BaseHistoryV32 { original, .. }, ResultKindV17::History(form)) => {
                    ui::metadata_base_v32::history::state(form)
                        .is_some_and(|s| Arc::ptr_eq(&s.view.original, original))
                }
                (KindV17::BaseInspectV32(request), ResultKindV17::History(form)) => {
                    ui::metadata_base_v32::history::state(form)
                        .is_some_and(|s| Arc::ptr_eq(&s.view.original, &request.view.original))
                }
                (KindV17::BaseOpenV32 { original, .. }, ResultKindV17::Open(form)) => {
                    ui::metadata_base_v32::state(form)
                        .is_some_and(|s| Arc::ptr_eq(s.intent.source.opening(), original))
                }
                (
                    KindV17::BasePreviewV32(expected),
                    ResultKindV17::BasePreviewV32(request, preview),
                ) => {
                    Arc::ptr_eq(expected, request)
                        && Arc::ptr_eq(&preview.intent, &request.intent)
                        && Arc::ptr_eq(&preview.values, &request.values)
                        && Arc::ptr_eq(&preview.selection, &request.selection)
                }
                (
                    KindV17::BasePreviewV32(_),
                    ResultKindV17::Preview(..) | ResultKindV17::MetadataPreviewV25(..),
                )
                | (
                    KindV17::Preview(_) | KindV17::MetadataPreviewV25(_),
                    ResultKindV17::BasePreviewV32(..),
                ) => false,
                (
                    KindV17::MetadataOpenV25 {
                        original, field, ..
                    },
                    ResultKindV17::Open(form),
                ) => ui::metadata_v25::state(form).is_some_and(|s| {
                    s.intent.field == *field && Arc::ptr_eq(s.intent.source.opening(), original)
                }),
                (
                    KindV17::MetadataPreviewV25(expected),
                    ResultKindV17::MetadataPreviewV25(request, preview),
                ) => {
                    Arc::ptr_eq(expected, request)
                        && Arc::ptr_eq(&preview.intent, &request.intent)
                        && Arc::ptr_eq(&preview.values, &request.values)
                }
                (KindV17::MetadataPreviewV25(_), ResultKindV17::Preview(..)) => false,
                (KindV17::Preview(_), ResultKindV17::MetadataPreviewV25(..)) => false,
                (
                    KindV17::MetadataHistoryV25 {
                        original, field, ..
                    },
                    ResultKindV17::History(form),
                ) => ui::metadata_v25::history::state(form).is_some_and(|s| {
                    s.view.field == *field && Arc::ptr_eq(&s.view.original, original)
                }),
                (KindV17::MetadataInspectV25(request), ResultKindV17::History(form)) => {
                    ui::metadata_v25::history::state(form).is_some_and(|s| {
                        s.view.field == request.view.field
                            && Arc::ptr_eq(&s.view.original, &request.view.original)
                    })
                }
                (KindV17::History { .. } | KindV17::Inspect(_), ResultKindV17::History(form)) => {
                    ui::metadata_v25::history::state(form).is_none()
                        && ui::metadata_base_v32::history::state(form).is_none()
                        && ui::metadata_members_v48::history::state(form).is_none()
                }
                _ => true,
            }
            && match &self.result {
                ResultKindV17::Open(_) if owner.work.phase() == ConversationPhaseKindV17::Open => {
                    owner.open_cutoff_v20.lock().is_ok_and(|slot| {
                        slot.as_ref()
                            .is_some_and(|cutoff| cutoff.check_original().is_ok())
                    })
                }
                ResultKindV17::LocalDraftReadFailureV20(failure) => {
                    owner.open_cutoff_v20.lock().is_ok_and(|slot| {
                        slot.as_ref()
                            .is_some_and(|cutoff| cutoff.check_original() == Err(*failure))
                    })
                }
                _ => true,
            }
            && matches!(
                (owner.work.phase(), &self.result),
                (_, ResultKindV17::Failure { .. })
                    | (
                        ConversationPhaseKindV17::StateSourceSyncV80,
                        ResultKindV17::StateSourceV80(_)
                    )
                    | (
                        ConversationPhaseKindV17::MetadataSectionV51,
                        ResultKindV17::MetadataSectionV51(_)
                    )
                    | (
                        ConversationPhaseKindV17::RefreshExactV19,
                        ResultKindV17::RefreshedV19(_)
                    )
                    | (ConversationPhaseKindV17::Open, ResultKindV17::Open(_))
                    | (
                        ConversationPhaseKindV17::Open,
                        ResultKindV17::IssueUnsupportedV102(..)
                    )
                    | (
                        ConversationPhaseKindV17::Open,
                        ResultKindV17::LocalDraftReadFailureV20(_)
                    )
                    | (ConversationPhaseKindV17::NewAction, ResultKindV17::Open(_))
                    | (
                        ConversationPhaseKindV17::NewAction,
                        ResultKindV17::MembersNewV52(..)
                    )
                    | (
                        ConversationPhaseKindV17::Preview | ConversationPhaseKindV17::NewAction,
                        ResultKindV17::IssuePreviewV87(..)
                    )
                    | (
                        ConversationPhaseKindV17::Preview,
                        ResultKindV17::IssueNoChangeV102(..)
                    )
                    | (
                        ConversationPhaseKindV17::Preview,
                        ResultKindV17::Preview(..)
                    )
                    | (
                        ConversationPhaseKindV17::Preview,
                        ResultKindV17::MetadataPreviewV25(..)
                    )
                    | (
                        ConversationPhaseKindV17::Preview,
                        ResultKindV17::MembersPreviewV48(..)
                    )
                    | (
                        ConversationPhaseKindV17::Preview,
                        ResultKindV17::MetadataNoChangeV65(_)
                    )
                    | (
                        ConversationPhaseKindV17::Preview,
                        ResultKindV17::BasePreviewV32(..)
                    )
                    | (
                        ConversationPhaseKindV17::Preview,
                        ResultKindV17::BaseNoChangeV39(..)
                    )
                    | (
                        ConversationPhaseKindV17::Confirm,
                        ResultKindV17::Publication(_)
                    )
                    | (
                        ConversationPhaseKindV17::History | ConversationPhaseKindV17::Inspect,
                        ResultKindV17::History(_)
                    )
                    | (
                        ConversationPhaseKindV17::Acknowledge,
                        ResultKindV17::Acknowledged(..)
                    )
                    | (
                        ConversationPhaseKindV17::Open
                            | ConversationPhaseKindV17::Preview
                            | ConversationPhaseKindV17::NewAction
                            | ConversationPhaseKindV17::History
                            | ConversationPhaseKindV17::Inspect
                            | ConversationPhaseKindV17::Acknowledge,
                        ResultKindV17::StateV68(_)
                    )
            )
    }
}
pub(crate) fn execute(
    owner: Arc<ScopedPrCollaborationOwnerV17>,
    provider: &GithubProvider,
    database: &Path,
    drafts: &DraftClient,
) -> PreparedPrCollaborationV17 {
    let run = || -> crate::providers::gh::GithubResult<ResultKindV17> {
        if let KindV17::StateSourceSyncV80(source) = &owner.work.kind {
            if database != source.original().database() || owner.expired() {
                return Err(crate::providers::gh::GithubError {
                    kind: GithubErrorKind::Unsupported,
                    message: "Original State source/clock changed before Core".into(),
                });
            }
            let read =
                crate::providers::gh::StateSourceReadV80::read(Arc::clone(source), provider)?;
            let prepared =
                crate::storage::PreparedStateSourceCommitV80::prepare(&read).map_err(|_| {
                    crate::providers::gh::GithubError {
                        kind: GithubErrorKind::Unsupported,
                        message: "State source preparation unavailable; no mutation or resend"
                            .into(),
                    }
                })?;
            let closed = read.close()?;
            return prepared
                .commit(closed)
                .map(ResultKindV17::StateSourceV80)
                .map_err(|_| crate::providers::gh::GithubError {
                    kind: GithubErrorKind::Unsupported,
                    message:
                        "State source/Clear predecessor changed; membership remains unavailable"
                            .into(),
                });
        }
        let provider = provider
            .clone()
            .with_original_pr_read_bounds_v13(owner.deadline, 1024 * 1024)?;
        let cancel = owner.cancellation();
        match &owner.work.kind {
            KindV17::StateSourceSyncV80(_) => Err(crate::providers::gh::GithubError {
                kind: GithubErrorKind::Unsupported,
                message: "State source work requires its original pre-Core allocation boundary"
                    .into(),
            }),
            KindV17::StateV68(state) => state.execute(&owner, &provider, database, drafts),
            KindV17::IssueOpenV87 {
                original, command, ..
            } => {
                let cutoff = owner
                    .open_cutoff_v20
                    .lock()
                    .ok()
                    .and_then(|slot| slot.clone())
                    .ok_or_else(crate::scoped_pr_issue_links_v87::failure)?;
                if let Err(failure) = drafts.wait_conversation_open_cutoff_v20(&cutoff, cancel) {
                    if failure == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = closed_draft_ticket_v20(&owner) {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket,
                                database,
                                cancel,
                                owner.deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(failure));
                }
                let opened = ui::issue_link_v87::open_native_v102(
                    Arc::clone(original),
                    *command,
                    &provider,
                    database,
                    cancel,
                    owner.deadline,
                )?;
                if let Err(failure) = cutoff.check(drafts, cancel) {
                    if failure == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = closed_draft_ticket_v20(&owner) {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket,
                                database,
                                cancel,
                                owner.deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(failure));
                }
                match opened {
                    crate::scoped_pr_collaboration_source_v17::initial_v17::IssueOpenCoreResultV102::Closed(form)=>Ok(ResultKindV17::Open(Box::new(form))),
                    crate::scoped_pr_collaboration_source_v17::initial_v17::IssueOpenCoreResultV102::Unsupported(proof)=>Ok(ResultKindV17::IssueUnsupportedV102(Arc::clone(original),proof)),
                }
            }
            KindV17::IssuePreviewV87(request) => request
                .prepare_native_v102(&provider, database, cancel, owner.deadline)
                .map(|result| match result {
                    crate::scoped_pr_issue_links_v87::IssuePreviewResultV102::Preview(preview) => {
                        ResultKindV17::IssuePreviewV87(Arc::clone(request), preview)
                    }
                    crate::scoped_pr_issue_links_v87::IssuePreviewResultV102::NoChange(proof) => {
                        ResultKindV17::IssueNoChangeV102(Arc::clone(request), proof)
                    }
                }),
            KindV17::IssueNewV87(request) => request
                .prepare(
                    &provider,
                    database,
                    cancel,
                    owner.deadline,
                    matches!(owner.work.kind, KindV17::IssueNewV87(_)),
                )
                .map(|preview| ResultKindV17::IssuePreviewV87(Arc::clone(request), preview)),
            KindV17::IssueConfirmV87(request) => {
                let preview = request
                    .preview
                    .as_ref()
                    .ok_or_else(crate::scoped_pr_issue_links_v87::failure)?;
                let fence = owner
                    .publication
                    .lock()
                    .ok()
                    .and_then(|mut slot| slot.take())
                    .ok_or_else(crate::scoped_pr_issue_links_v87::failure)?;
                PrPlan::execute_original_issue_links_v87(
                    preview,
                    &provider,
                    database,
                    drafts,
                    fence,
                    cancel,
                    owner.deadline,
                )
                .map(ResultKindV17::Publication)
            }
            KindV17::MetadataSectionV51(original) => {
                crate::scoped_pr_collaboration_source_v17::metadata_context_v51::read(
                    original.clone(),
                    &provider,
                    database,
                    owner.cancellation(),
                    owner.deadline,
                )
                .map(|section| ResultKindV17::MetadataSectionV51(Box::new(section)))
            }
            KindV17::BaseHistoryV32 { original, .. } => ui::metadata_base_v32::history::open(
                original.clone(),
                &provider,
                database,
                cancel,
                owner.deadline,
                None,
            )
            .map(Box::new)
            .map(ResultKindV17::History),
            KindV17::BaseInspectV32(r) => ui::metadata_base_v32::history::open(
                r.view.original.clone(),
                &provider,
                database,
                cancel,
                owner.deadline,
                Some(r),
            )
            .map(Box::new)
            .map(ResultKindV17::History),
            KindV17::BaseAcknowledgeV32(r) => ui::metadata_base_v32::history::acknowledge(
                r,
                &provider,
                database,
                cancel,
                owner.deadline,
            )
            .map(|(reference, changed)| ResultKindV17::Acknowledged(reference, changed)),
            KindV17::BaseOpenV32 { original, .. } => {
                let cutoff = owner
                    .open_cutoff_v20
                    .lock()
                    .ok()
                    .and_then(|v| v.clone())
                    .ok_or_else(ui::metadata_base_v32::failure)?;
                if let Err(failure) = drafts.wait_conversation_open_cutoff_v20(&cutoff, cancel) {
                    if failure == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = closed_draft_ticket_v20(&owner) {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket,
                                database,
                                cancel,
                                owner.deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(failure));
                }
                let form = ui::metadata_base_v32::open(
                    original.clone(),
                    &provider,
                    database,
                    cancel,
                    owner.deadline,
                )?;
                if let Err(failure) = cutoff.check(drafts, cancel) {
                    if failure == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = closed_draft_ticket_v20(&owner) {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket,
                                database,
                                cancel,
                                owner.deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(failure));
                }
                Ok(ResultKindV17::Open(Box::new(form)))
            }
            KindV17::BasePreviewV32(request) => ui::metadata_base_v32::BasePreviewV32::prepare(
                request,
                &provider,
                database,
                cancel,
                owner.deadline,
            )
            .map(|preview| match preview {
                ui::metadata_base_v32::BasePreviewResultV39::Prepared(preview) => {
                    ResultKindV17::BasePreviewV32(request.clone(), preview)
                }
                ui::metadata_base_v32::BasePreviewResultV39::NoChange(proof) => {
                    ResultKindV17::BaseNoChangeV39(request.clone(), proof)
                }
            }),
            KindV17::BaseConfirmV32(request) => {
                let preview = request
                    .preview
                    .as_ref()
                    .ok_or_else(ui::metadata_base_v32::failure)?;
                let fence = owner
                    .publication
                    .lock()
                    .ok()
                    .and_then(|mut f| f.take())
                    .ok_or_else(ui::metadata_base_v32::failure)?;
                PrPlan::execute_original_metadata_base_v32(
                    preview,
                    &provider,
                    database,
                    drafts,
                    fence,
                    cancel,
                    owner.deadline,
                )
                .map(ResultKindV17::Publication)
            }
            KindV17::MetadataHistoryV25 {
                original, field, ..
            } => ui::metadata_v25::history::open(
                original.clone(),
                *field,
                &provider,
                database,
                cancel,
                owner.deadline,
                None,
            )
            .map(Box::new)
            .map(ResultKindV17::History),
            KindV17::MetadataInspectV25(request) => ui::metadata_v25::history::open(
                request.view.original.clone(),
                request.view.field,
                &provider,
                database,
                cancel,
                owner.deadline,
                Some(request),
            )
            .map(Box::new)
            .map(ResultKindV17::History),
            KindV17::MetadataAcknowledgeV25(request) => ui::metadata_v25::history::acknowledge(
                request,
                &provider,
                database,
                cancel,
                owner.deadline,
            )
            .map(|(reference, changed)| ResultKindV17::Acknowledged(reference, changed)),
            KindV17::IssueHistoryV87 {
                original,
                command,
                from,
            } => ui::issue_link_v87::history::open(
                Arc::clone(original),
                *command,
                &provider,
                database,
                cancel,
                owner.deadline,
                from.as_ref().map_or(
                    ui::issue_link_v87::history::IssueHistoryAdmissionV87::Current,
                    |from| ui::issue_link_v87::history::IssueHistoryAdmissionV87::Form(from),
                ),
            )
            .map(Box::new)
            .map(ResultKindV17::History),
            KindV17::IssueInspectV87(r) => ui::issue_link_v87::history::open(
                Arc::clone(&r.view.original),
                r.view.command,
                &provider,
                database,
                cancel,
                owner.deadline,
                ui::issue_link_v87::history::IssueHistoryAdmissionV87::Inspection(r),
            )
            .map(Box::new)
            .map(ResultKindV17::History),
            KindV17::IssueAcknowledgeV87(r) => ui::issue_link_v87::history::acknowledge(
                r,
                &provider,
                database,
                cancel,
                owner.deadline,
            )
            .map(|(reference, changed)| ResultKindV17::Acknowledged(reference, changed)),
            KindV17::MembersHistoryV48 { original, .. } => ui::metadata_members_v48::history::open(
                original.clone(),
                &provider,
                database,
                cancel,
                owner.deadline,
                None,
            )
            .map(Box::new)
            .map(ResultKindV17::History),
            KindV17::MembersInspectV48(request) => ui::metadata_members_v48::history::open(
                request.view.original.clone(),
                &provider,
                database,
                cancel,
                owner.deadline,
                Some(request),
            )
            .map(Box::new)
            .map(ResultKindV17::History),
            KindV17::MembersAcknowledgeV48(request) => {
                ui::metadata_members_v48::history::acknowledge(
                    request,
                    &provider,
                    database,
                    cancel,
                    owner.deadline,
                )
                .map(|(reference, changed)| ResultKindV17::Acknowledged(reference, changed))
            }
            KindV17::MembersOpenV48 {
                original, command, ..
            } => {
                let cutoff = owner
                    .open_cutoff_v20
                    .lock()
                    .ok()
                    .and_then(|cutoff| cutoff.clone());
                let Some(cutoff) = cutoff else {
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(
                        ReadCutoffFailureV20::Unavailable,
                    ));
                };
                if let Err(failure) = drafts.wait_conversation_open_cutoff_v20(&cutoff, cancel) {
                    if failure == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = closed_draft_ticket_v20(&owner) {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket,
                                database,
                                cancel,
                                owner.deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(failure));
                }
                let form = ui::metadata_members_v48::open(
                    Arc::clone(original),
                    *command,
                    &provider,
                    database,
                    cancel,
                    owner.deadline,
                )?;
                if let Err(failure) = cutoff.check(drafts, cancel) {
                    if failure == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = closed_draft_ticket_v20(&owner) {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket,
                                database,
                                cancel,
                                owner.deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(failure));
                }
                Ok(ResultKindV17::Open(Box::new(form)))
            }
            KindV17::MembersPreviewV48(request) => {
                ui::metadata_members_v48::MetadataMembersPreviewV48::prepare(
                    request,
                    &provider,
                    database,
                    cancel,
                    owner.deadline,
                )
                .map(|preview| ResultKindV17::MembersPreviewV48(Arc::clone(request), preview))
            }
            KindV17::MembersNewV52(request) => request
                .prepare(&provider, database, cancel, owner.deadline)
                .map(|fresh| ResultKindV17::MembersNewV52(Arc::clone(request), fresh)),
            KindV17::MembersConfirmV48(request) => {
                let preview = request
                    .preview
                    .as_ref()
                    .ok_or_else(ui::metadata_members_v48::failure)?;
                let fence = owner
                    .publication
                    .lock()
                    .ok()
                    .and_then(|mut fence| fence.take())
                    .ok_or_else(ui::metadata_members_v48::failure)?;
                PrPlan::execute_original_metadata_members_v48(
                    preview,
                    &provider,
                    database,
                    drafts,
                    fence,
                    cancel,
                    owner.deadline,
                )
                .map(ResultKindV17::Publication)
            }
            KindV17::MetadataOpenV25 {
                original, field, ..
            } => {
                let cutoff = owner.open_cutoff_v20.lock().ok().and_then(|v| v.clone());
                let Some(cutoff) = cutoff else {
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(
                        ReadCutoffFailureV20::Unavailable,
                    ));
                };
                if let Err(failure) = drafts.wait_conversation_open_cutoff_v20(&cutoff, cancel) {
                    if failure == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = closed_draft_ticket_v20(&owner) {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket,
                                database,
                                cancel,
                                owner.deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(failure));
                }
                let form = ui::metadata_v25::open(
                    original.clone(),
                    *field,
                    &provider,
                    database,
                    cancel,
                    owner.deadline,
                )?;
                if let Err(failure) = cutoff.check(drafts, cancel) {
                    if failure == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = closed_draft_ticket_v20(&owner) {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket,
                                database,
                                cancel,
                                owner.deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(failure));
                }
                Ok(ResultKindV17::Open(Box::new(form)))
            }
            KindV17::MetadataPreviewV25(request) => {
                ui::metadata_v25::MetadataPreviewV25::prepare_native_v65(
                    request,
                    &provider,
                    database,
                    cancel,
                    owner.deadline,
                )
                .map(|result| match result {
                    ui::metadata_v25::MetadataPreviewDispositionV65::Preview(preview) => {
                        ResultKindV17::MetadataPreviewV25(request.clone(), preview)
                    }
                    ui::metadata_v25::MetadataPreviewDispositionV65::NoChange => {
                        ResultKindV17::MetadataNoChangeV65(request.clone())
                    }
                })
            }
            KindV17::MetadataConfirmV25(request) => {
                let preview = request
                    .preview
                    .as_ref()
                    .ok_or_else(ui::metadata_v25::failure)?;
                let fence = owner
                    .publication
                    .lock()
                    .ok()
                    .and_then(|mut f| f.take())
                    .ok_or_else(ui::metadata_v25::failure)?;
                PrPlan::execute_original_metadata_text_v25(
                    preview,
                    &provider,
                    database,
                    drafts,
                    fence,
                    cancel,
                    owner.deadline,
                )
                .map(ResultKindV17::Publication)
            }
            KindV17::RefreshExactV19(r) => r
                .execute(&provider, database, cancel, owner.deadline)
                .map(ResultKindV17::RefreshedV19),
            KindV17::History { original, .. } => crate::scoped_pr_collaboration_history_v17::open(
                original.clone(),
                &provider,
                database,
                cancel,
                owner.deadline,
            )
            .map(ui::form_from_history)
            .map(Box::new)
            .map(ResultKindV17::History),
            KindV17::Inspect(request) => crate::scoped_pr_collaboration_history_v17::inspect(
                request,
                &provider,
                database,
                cancel,
                owner.deadline,
            )
            .map(ui::form_from_history)
            .map(Box::new)
            .map(ResultKindV17::History),
            KindV17::Acknowledge(request) => {
                crate::scoped_pr_collaboration_history_v17::acknowledge(
                    request,
                    &provider,
                    database,
                    cancel,
                    owner.deadline,
                )
                .map(|(reference, changed)| ResultKindV17::Acknowledged(reference, changed))
            }
            KindV17::NewAction(request) => OpenedConversationDraftV17::open_phase_v18(
                request.intent.source.opening().clone(),
                request.intent.command,
                &provider,
                database,
                cancel,
                owner.deadline,
                request.intent.lineage_v18.as_ref(),
            )
            .map(ui::form_from_draft)
            .map(Box::new)
            .map(ResultKindV17::Open),
            KindV17::Open {
                original, command, ..
            } => {
                let cutoff = owner.open_cutoff_v20.lock().ok().and_then(|v| v.clone());
                let Some(cutoff) = cutoff else {
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(
                        ReadCutoffFailureV20::Unavailable,
                    ));
                };
                if let Err(failure) = drafts.wait_conversation_open_cutoff_v20(&cutoff, cancel) {
                    if failure == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = closed_draft_ticket_v20(&owner) {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket,
                                database,
                                cancel,
                                owner.deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(failure));
                }
                let opened = OpenedConversationDraftV17::open(
                    original.clone(),
                    *command,
                    &provider,
                    database,
                    cancel,
                    owner.deadline,
                )?;
                // Final check retains the same accepted revision/epoch through
                // actual Core and local draft preparation; no renewed clock.
                if let Err(failure) = cutoff.check(drafts, cancel) {
                    if failure == ReadCutoffFailureV20::SaveFailed {
                        if let Some(ticket) = closed_draft_ticket_v20(&owner) {
                            ui::pure_close_v20::validate_local_recovery(
                                ticket,
                                database,
                                cancel,
                                owner.deadline,
                            )?;
                        }
                    }
                    return Ok(ResultKindV17::LocalDraftReadFailureV20(failure));
                }
                Ok(ResultKindV17::Open(Box::new(ui::form_from_draft(opened))))
            }
            KindV17::Preview(request) => ConversationPreviewV17::prepare(
                request.intent().clone(),
                request.values().clone(),
                &provider,
                database,
                cancel,
                owner.deadline,
            )
            .map(|p| ResultKindV17::Preview(request.clone(), p)),
            KindV17::Confirm(request) => {
                let preview =
                    request
                        .preview()
                        .ok_or_else(|| crate::providers::gh::GithubError {
                            kind: GithubErrorKind::InvalidTarget,
                            message: "Fresh Conversation Preview unavailable".into(),
                        })?;
                let fence = owner
                    .publication
                    .lock()
                    .ok()
                    .and_then(|mut f| f.take())
                    .ok_or_else(|| crate::providers::gh::GithubError {
                        kind: GithubErrorKind::InvalidTarget,
                        message: "Original prequeue Conversation ACK cutoff unavailable".into(),
                    })?;
                PrPlan::execute_original_conversation_v17(
                    preview,
                    &provider,
                    database,
                    drafts,
                    fence,
                    cancel,
                    owner.deadline,
                )
                .map(ResultKindV17::Publication)
            }
        }
    };
    let result = run().unwrap_or_else(|e| ResultKindV17::Failure {
        account: matches!(
            e.kind,
            GithubErrorKind::Authentication | GithubErrorKind::IdentityChanged
        ),
        permission: matches!(e.kind, GithubErrorKind::Permission),
    });
    PreparedPrCollaborationV17 { owner, result }
}
/// Route these native actions through genuine original Summary/Conversation
/// producers BEFORE legacy context cloning or fallback. Other actual tabs or
/// incomplete legacy origins refuse honestly until their adapters are added;
/// only public legacy provider/PrPlan ABI is preserved, not every old UI route.
pub fn capture_action(app: &mut App, action: &PrUiAction, database: &Path) -> bool {
    if let PrUiAction::StateV68(command) | PrUiAction::StateHistoryV74(command) = action {
        match PrCollaborationOpenIntentV17::capture_state_v68(app, database, *command) {
            Ok(original) => {
                let state = if matches!(action, PrUiAction::StateHistoryV74(_)) {
                    state_v68::StateWorkV68::History {
                        original: Arc::clone(&original),
                        command: *command,
                        from: None,
                    }
                } else {
                    state_v68::StateWorkV68::Open {
                        original: Arc::clone(&original),
                        command: *command,
                        closed_draft_v20: ui::pure_close_v20::capture_state_ticket_v75(
                            app, *command,
                        ),
                    }
                };
                app.scoped_pr_collaboration_work_v17 =
                    Some(Arc::new(ScopedPrCollaborationWorkV17 {
                        cancel: original.cancellation().clone(),
                        deadline: original.deadline(),
                        kind: KindV17::StateV68(state),
                    }));
            }
            Err(reason) => app.status = reason,
        }
        return true;
    }
    if let PrUiAction::IssueLinksV87(command) = action {
        if let Err(reason) = capture_issue_links_v87(app, database, *command) {
            app.status = reason;
        }
        return true;
    }
    if let PrUiAction::IssueLinkHistoryV87(command) = action {
        if let Err(reason) = capture_issue_link_history_v87(app, database, *command) {
            app.status = reason;
        }
        return true;
    }
    if *action == PrUiAction::MetadataBaseHistoryV32 {
        match PrCollaborationOpenIntentV17::capture_metadata_base_v51(app, database) {
            Ok(original) => {
                app.scoped_pr_collaboration_work_v17 =
                    Some(Arc::new(ScopedPrCollaborationWorkV17 {
                        cancel: original.cancellation().clone(),
                        deadline: original.deadline(),
                        kind: KindV17::BaseHistoryV32 {
                            original,
                            from: None,
                        },
                    }))
            }
            Err(reason) => app.status = reason,
        }
        return true;
    }
    if *action == PrUiAction::MetadataMembersHistoryV48 {
        match PrCollaborationOpenIntentV17::capture_metadata_members_history_v61(app, database) {
            Ok(original) => {
                app.scoped_pr_collaboration_work_v17 =
                    Some(Arc::new(ScopedPrCollaborationWorkV17 {
                        cancel: original.cancellation().clone(),
                        deadline: original.deadline(),
                        kind: KindV17::MembersHistoryV48 {
                            original,
                            from: None,
                        },
                    }))
            }
            Err(reason) => app.status = reason,
        }
        return true;
    }
    if let PrUiAction::MetadataMembersV48(command) = action {
        match PrCollaborationOpenIntentV17::capture_metadata_members_v61(app, database, *command) {
            Ok(original) => {
                app.scoped_pr_collaboration_work_v17 =
                    Some(Arc::new(ScopedPrCollaborationWorkV17 {
                        cancel: original.cancellation().clone(),
                        deadline: original.deadline(),
                        kind: KindV17::MembersOpenV48 {
                            original,
                            command: *command,
                            closed_draft_v20: ui::pure_close_v20::capture_members_ticket_v48(
                                app, *command,
                            ),
                        },
                    }))
            }
            Err(reason) => app.status = reason,
        }
        return true;
    }
    if *action == PrUiAction::MetadataBaseV32 {
        match PrCollaborationOpenIntentV17::capture_metadata_base_v51(app, database) {
            Ok(original) => {
                app.scoped_pr_collaboration_work_v17 =
                    Some(Arc::new(ScopedPrCollaborationWorkV17 {
                        cancel: original.cancellation().clone(),
                        deadline: original.deadline(),
                        kind: KindV17::BaseOpenV32 {
                            original,
                            closed_draft_v20: ui::pure_close_v20::capture_base_ticket_v32(app),
                        },
                    }))
            }
            Err(reason) => app.status = reason,
        }
        return true;
    }
    if let PrUiAction::MetadataHistoryV25(field) = action {
        match PrCollaborationOpenIntentV17::capture_metadata_v51(app, database, *field) {
            Ok(original) => {
                app.scoped_pr_collaboration_work_v17 =
                    Some(Arc::new(ScopedPrCollaborationWorkV17 {
                        cancel: original.cancellation().clone(),
                        deadline: original.deadline(),
                        kind: KindV17::MetadataHistoryV25 {
                            original,
                            field: *field,
                            from: None,
                        },
                    }))
            }
            Err(reason) => app.status = reason,
        }
        return true;
    }
    if let PrUiAction::MetadataTextV25(field) = action {
        match PrCollaborationOpenIntentV17::capture_metadata_v51(app, database, *field) {
            Ok(original) => {
                app.scoped_pr_collaboration_work_v17 =
                    Some(Arc::new(ScopedPrCollaborationWorkV17 {
                        cancel: original.cancellation().clone(),
                        deadline: original.deadline(),
                        kind: KindV17::MetadataOpenV25 {
                            original,
                            field: *field,
                            closed_draft_v20: ui::pure_close_v20::capture_metadata_ticket_v25(
                                app, *field,
                            ),
                        },
                    }))
            }
            Err(reason) => app.status = reason,
        }
        return true;
    }
    if *action == PrUiAction::Inspect {
        match PrCollaborationOpenIntentV17::capture(app, database) {
            Ok(original) => {
                app.scoped_pr_collaboration_work_v17 =
                    Some(Arc::new(ScopedPrCollaborationWorkV17 {
                        cancel: original.cancellation().clone(),
                        deadline: original.deadline(),
                        kind: KindV17::History {
                            original,
                            from: None,
                        },
                    }));
            }
            Err(reason) => app.status = reason,
        }
        return true;
    }
    let PrUiAction::Command(command) = action else {
        return false;
    };
    if matches!(
        command,
        PrCommand::MarkViewed | PrCommand::MarkUnviewed | PrCommand::Merge
    ) || PrCommand::LIFECYCLE.contains(command)
        || PrCommand::GOVERNED.contains(command)
    {
        return false;
    }
    match PrCollaborationOpenIntentV17::capture(app, database) {
        Ok(original) => {
            let cancel = original.cancellation().clone();
            let deadline = original.deadline();
            app.scoped_pr_collaboration_work_v17 = Some(Arc::new(ScopedPrCollaborationWorkV17 {
                kind: KindV17::Open {
                    original,
                    command: *command,
                    closed_draft_v20: ui::pure_close_v20::capture_ticket(app, *command),
                },
                cancel,
                deadline,
            }));
        }
        Err(reason) => app.status = reason,
    }
    true
}
/// Closed actual-PR entry for the Issue command. The visible action catalogue
/// will call this only after the backend/current route gates are verified.
pub(crate) fn capture_issue_links_v87(
    app: &mut App,
    database: &Path,
    command: crate::pr_issue_links_v87::IssueLinkCommandV87,
) -> Result<(), String> {
    if newer_intent_without_pr_v20(app)
        || app
            .pr_action
            .as_ref()
            .is_some_and(|action| *action != PrUiAction::IssueLinksV87(command))
        || app.form.is_some()
        || app.overlay.is_some()
    {
        return Err("Finish the actual current intent before opening manual Issue links".into());
    }
    let original = PrCollaborationOpenIntentV17::capture_issue_links_v87(app, database, command)
        .map_err(|e| e.to_string())?;
    let work = Arc::new(ScopedPrCollaborationWorkV17 {
        cancel: original.cancellation().clone(),
        deadline: original.deadline(),
        kind: KindV17::IssueOpenV87 {
            original,
            command,
            closed_draft_v20: ui::pure_close_v20::capture_issue_ticket_v102(app, command),
        },
    });
    if !work.matches(app) {
        return Err("Actual original Issue presentation changed before admission".into());
    }
    app.scoped_pr_collaboration_work_v17 = Some(work);
    Ok(())
}
pub(crate) fn capture_issue_link_history_v87(
    app: &mut App,
    database: &Path,
    command: crate::pr_issue_links_v87::IssueLinkCommandV87,
) -> Result<(), String> {
    if newer_intent_without_pr_v20(app)
        || app
            .pr_action
            .as_ref()
            .is_some_and(|action| *action != PrUiAction::IssueLinkHistoryV87(command))
        || app.form.is_some()
        || app.overlay.is_some()
    {
        return Err("Finish the actual current intent before original Issue history".into());
    }
    let original = PrCollaborationOpenIntentV17::capture_issue_link_history_v87(app, database)?;
    let work = Arc::new(ScopedPrCollaborationWorkV17 {
        cancel: original.cancellation().clone(),
        deadline: original.deadline(),
        kind: KindV17::IssueHistoryV87 {
            original,
            command,
            from: None,
        },
    });
    if !work.matches(app) {
        return Err("Actual Issue history presentation changed before admission".into());
    }
    app.scoped_pr_collaboration_work_v17 = Some(work);
    Ok(())
}
/// Same real tab gesture/Inbox work slot for both source families. No legacy
/// load_tab, synthetic page, source, body move or clock is made on foreground.
pub fn capture_metadata_section_v51(app: &mut App, database: &Path) {
    if app
        .github
        .as_ref()
        .and_then(|g| g.pr.as_ref())
        .is_some_and(|p| p.tab == crate::github_workspace::PrTab::Metadata)
    {
        app.status =
            "Metadata is already displayed • edit starts a fresh field/capability read".into();
        return;
    }
    match PrCollaborationOpenIntentV17::capture(app, database) {
        Ok(original) => {
            app.scoped_pr_collaboration_work_v17 = Some(Arc::new(ScopedPrCollaborationWorkV17 {
                cancel: original.cancellation().clone(),
                deadline: original.deadline(),
                kind: KindV17::MetadataSectionV51(original),
            }))
        }
        Err(reason) => app.status = reason,
    }
}
/// Actual explicit Refresh interception, BEFORE either tagged or startup
/// legacy fallback. A refused capture consumes only this new input, never an
/// old queue/owner/slot or cached-private payload.
pub fn state_source_requires_explicit_refresh_v80(app: &App) -> bool {
    app.view == View::Inbox
        && app
            .github
            .as_ref()
            .is_some_and(|g| g.state_source_sync_required_v80())
}
pub fn prepare_state_source_sync_v80(app: &mut App, database: &Path) {
    if !app.refresh_requested
        || app.view != View::Inbox
        || !app
            .github
            .as_ref()
            .is_some_and(|g| g.state_source_sync_required_v80())
    {
        return;
    }
    let captured = crate::state_inbox_sync_v76::StateSourceSyncV76::capture(app, database);
    app.refresh_requested = false;
    match captured {
        Ok(source) => {
            app.scoped_pr_collaboration_work_v17 = Some(Arc::new(ScopedPrCollaborationWorkV17 {
                cancel: source.original().cancellation().clone(),
                deadline: source.original().deadline(),
                kind: KindV17::StateSourceSyncV80(source),
            }));
        }
        Err(reason) => {
            if app.scoped_pr_collaboration_owner_v17.is_none() && !app.loading[View::Inbox.index()]
            {
                app.status = reason;
            }
        }
    }
}
pub fn prepare_requested(app: &mut App) {
    if app
        .form
        .as_ref()
        .is_some_and(|f| ui::return_descriptor_v19(f).is_some())
    {
        if newer_intent(app)
            || app.view != View::Inbox
            || app
                .form
                .as_ref()
                .is_none_or(|f| f.submitting || !f.preview_requested)
        {
            return;
        }
        let request =
            match crate::scoped_pr_collaboration_refresh_v19::return_v19::capture_request(app) {
                Ok(r) => r,
                Err(reason) => {
                    app.form.as_mut().unwrap().error = reason;
                    return;
                }
            };
        let Some(deadline) = Instant::now().checked_add(Duration::from_secs(30)) else {
            return;
        };
        let work = Arc::new(ScopedPrCollaborationWorkV17 {
            kind: KindV17::RefreshExactV19(request),
            cancel: CancellationToken::default(),
            deadline,
        });
        if work.matches(app) {
            app.form.as_mut().unwrap().preview_requested = false;
            app.scoped_pr_collaboration_work_v17 = Some(work);
        }
        return;
    }
    if newer_intent(app) || retired_presentation(app) {
        return;
    }
    let Some(form) = &app.form else {
        return;
    };
    if !matches!(form.purpose, FormPurpose::PrCollaborationV17(_))
        || form.submitting
        || app.view != View::Inbox
        || app.form_dismissed
        || app.overlay != Some(Overlay::Form)
        || !matches!(&form.purpose, FormPurpose::PrCollaborationV17(state)
            if state.original().owns_presentation(app))
    {
        return;
    }
    let confirm = form.submit_requested;
    if !confirm && !form.preview_requested {
        return;
    }
    let Some(deadline) = Instant::now().checked_add(Duration::from_secs(30)) else {
        return;
    };
    let build = || -> Result<ScopedPrCollaborationWorkV17, String> {
        let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
            unreachable!("exact purpose above")
        };
        let (kind, cancel, deadline) = if ui::state_v68::state(form).is_some()
            || ui::state_v68::history_v74::state(form).is_some()
        {
            let (work, cancel, deadline) = state_v68::requested(form, confirm, deadline)?;
            (KindV17::StateV68(work), cancel, deadline)
        } else if ui::issue_link_v87::history::state(form).is_some() {
            let r = ui::issue_link_v87::history::request(form)?;
            if confirm {
                if !ui::issue_link_v87::history::ack_ready(form) {
                    return Err(
                        "Fresh useful exact PR+Issue inspection required before LOCAL ACK".into(),
                    );
                }
                let current = r
                    .view
                    .current
                    .as_ref()
                    .ok_or("Exact PR+Issue inspection unavailable")?;
                let cancel = current.cancel.clone();
                let deadline = current.deadline;
                (KindV17::IssueAcknowledgeV87(r), cancel, deadline)
            } else {
                (
                    KindV17::IssueInspectV87(r),
                    CancellationToken::default(),
                    deadline,
                )
            }
        } else if ui::issue_link_v87::state(form).is_some_and(|s| !s.needs_new())
            && form.publication.is_some()
        {
            let from = ui::issue_link_v87::history::capture_open(form)?;
            (
                KindV17::IssueHistoryV87 {
                    original: Arc::clone(&from.original),
                    command: from.command,
                    from: Some(from),
                },
                CancellationToken::default(),
                deadline,
            )
        } else if ui::metadata_members_v48::history::state(form).is_some() {
            let request = ui::metadata_members_v48::history::request(form)?;
            if confirm {
                if !ui::metadata_members_v48::history::ack_ready(form) {
                    return Err(
                        "Fresh useful exact member inspection required before LOCAL ACK".into(),
                    );
                }
                let current = request
                    .view
                    .current
                    .as_ref()
                    .ok_or("Exact member inspection unavailable")?;
                let cancel = current.cancel.clone();
                let deadline = current.deadline;
                (KindV17::MembersAcknowledgeV48(request), cancel, deadline)
            } else {
                (
                    KindV17::MembersInspectV48(request),
                    CancellationToken::default(),
                    deadline,
                )
            }
        } else if ui::metadata_base_v32::history::state(form).is_some() {
            let request = ui::metadata_base_v32::history::request(form)?;
            if confirm {
                if !ui::metadata_base_v32::history::ack_ready(form) {
                    return Err(
                        "Fresh useful exact Base inspection required before LOCAL ACK".into(),
                    );
                }
                let current = request
                    .view
                    .current
                    .as_ref()
                    .ok_or("Exact Base inspection unavailable")?;
                let cancel = current.cancel.clone();
                let deadline = current.deadline;
                (KindV17::BaseAcknowledgeV32(request), cancel, deadline)
            } else {
                (
                    KindV17::BaseInspectV32(request),
                    CancellationToken::default(),
                    deadline,
                )
            }
        } else if ui::metadata_v25::history::state(form).is_some() {
            let request = ui::metadata_v25::history::request(form)?;
            if confirm {
                if !ui::metadata_v25::history::ack_ready(form) {
                    return Err(
                        "Fresh useful exact metadata inspection required before LOCAL ACK".into(),
                    );
                }
                let current = request
                    .view
                    .current
                    .as_ref()
                    .ok_or("Exact metadata inspection unavailable")?;
                let cancel = current.cancel.clone();
                let deadline = current.deadline;
                (KindV17::MetadataAcknowledgeV25(request), cancel, deadline)
            } else {
                (
                    KindV17::MetadataInspectV25(request),
                    CancellationToken::default(),
                    deadline,
                )
            }
        } else if ui::issue_link_v87::state(form).is_some() {
            let request = ui::issue_link_v87::request(form).map_err(|e| e.message)?;
            let kind = if ui::issue_link_v87::state(form).is_some_and(|s| s.needs_new()) && confirm
            {
                if !ui::issue_link_v87::new_ready(form) {
                    return Err("Paint the exact original terminal notice before New action".into());
                }
                KindV17::IssueNewV87(request)
            } else {
                if form.publication.is_some() {
                    return Err(
                        "Exact manual Issue history inspection is required; no resend".into(),
                    );
                }
                if confirm && !request.matches_confirm(form) {
                    return Err("Paint the useful exact Issue Preview before Confirm".into());
                }
                if confirm {
                    KindV17::IssueConfirmV87(request)
                } else {
                    KindV17::IssuePreviewV87(request)
                }
            };
            (kind, CancellationToken::default(), deadline)
        } else if ui::metadata_members_v48::state(form).is_some() && form.publication.is_none() {
            let request = ui::metadata_members_v48::request(form).map_err(|error| error.message)?;
            if confirm && !request.matches_confirm(form) {
                return Err("Paint the useful exact member Preview before Confirm".into());
            }
            (
                if confirm {
                    KindV17::MembersConfirmV48(request)
                } else {
                    KindV17::MembersPreviewV48(request)
                },
                CancellationToken::default(),
                deadline,
            )
        } else if ui::metadata_base_v32::state(form).is_some() && form.publication.is_none() {
            let request = ui::metadata_base_v32::request(form)?;
            if confirm && !request.matches_confirm(form) {
                return Err("Paint the useful exact Base Preview before Confirm".into());
            }
            (
                if confirm {
                    KindV17::BaseConfirmV32(request)
                } else {
                    KindV17::BasePreviewV32(request)
                },
                CancellationToken::default(),
                deadline,
            )
        } else if state.is_history() {
            let request = ui::history_request(form)?;
            if confirm {
                if !ui::history_ack_ready(form) {
                    return Err(
                        "Fresh exact usefully painted inspection required before local ACK".into(),
                    );
                }
                let current = request
                    .view
                    .current
                    .as_ref()
                    .ok_or("Fresh exact inspection unavailable")?;
                let cancel = current.cancel.clone();
                let deadline = current.deadline;
                (KindV17::Acknowledge(request), cancel, deadline)
            } else {
                (
                    KindV17::Inspect(request),
                    CancellationToken::default(),
                    deadline,
                )
            }
        } else if state.needs_new_v18() && confirm {
            (
                if ui::metadata_members_v48::state(form).is_some() {
                    KindV17::MembersNewV52(
                        ui::metadata_members_v48::new_request_v52(form)
                            .map_err(|error| error.message)?,
                    )
                } else {
                    KindV17::NewAction(ui::new_action_request_v18(form)?)
                },
                CancellationToken::default(),
                deadline,
            )
        } else if form.publication.is_some() && ui::metadata_members_v48::state(form).is_some() {
            let from = ui::metadata_members_v48::history::capture_open(form)?;
            (
                KindV17::MembersHistoryV48 {
                    original: from.original.clone(),
                    from: Some(from),
                },
                CancellationToken::default(),
                deadline,
            )
        } else if form.publication.is_some() && ui::metadata_base_v32::state(form).is_some() {
            let from = ui::metadata_base_v32::history::capture_open(form)?;
            (
                KindV17::BaseHistoryV32 {
                    original: from.original.clone(),
                    from: Some(from),
                },
                CancellationToken::default(),
                deadline,
            )
        } else if form.publication.is_some() && ui::metadata_v25::state(form).is_some() {
            let from = ui::metadata_v25::history::capture_open(form)?;
            (
                KindV17::MetadataHistoryV25 {
                    original: from.original.clone(),
                    field: from.field,
                    from: Some(from),
                },
                CancellationToken::default(),
                deadline,
            )
        } else if form.publication.is_some() {
            let from = ui::capture_history_open(form)?;
            (
                KindV17::History {
                    original: from.original.clone(),
                    from: Some(from),
                },
                CancellationToken::default(),
                deadline,
            )
        } else if ui::metadata_v25::state(form).is_some() {
            let request = ui::metadata_v25::request(form)?;
            if confirm && !request.matches_confirm(form) {
                return Err("Paint the useful exact metadata Preview before Confirm".into());
            }
            (
                if confirm {
                    KindV17::MetadataConfirmV25(request)
                } else {
                    KindV17::MetadataPreviewV25(request)
                },
                CancellationToken::default(),
                deadline,
            )
        } else {
            if confirm {
                ui::confirmed_preview(form)?;
            }
            let request = ui::request(form)?;
            // LOCAL desired-value restrictions only, borrowing genuine intent
            // selections. The same parser defends the worker; fresh authority
            // still requires the unchanged Core/parent/capability/UID closure.
            crate::pr_actions::PrOperation::validate_values_v17(
                request.intent().command,
                request.intent().selected.as_ref(),
                &request.intent().reactions,
                request.intent().source.identity().account_id,
                request.values(),
            )?;
            (
                if confirm {
                    KindV17::Confirm(request)
                } else {
                    KindV17::Preview(request)
                },
                CancellationToken::default(),
                deadline,
            )
        };
        Ok(ScopedPrCollaborationWorkV17 {
            kind,
            cancel,
            deadline,
        })
    };
    let work = match build() {
        Ok(work) => Arc::new(work),
        Err(reason) => {
            app.form.as_mut().unwrap().error = reason;
            return;
        }
    };
    if !work.matches(app) || work.cancel.is_cancelled() || Instant::now() >= work.deadline {
        return;
    }
    let form = app.form.as_mut().unwrap();
    form.preview_requested = false;
    form.submit_requested = false;
    app.scoped_pr_collaboration_work_v17 = Some(work);
}
pub fn submission_failed(app: &mut App, work: &ScopedPrCollaborationWorkV17) {
    if !work.matches(app) {
        return;
    }
    if let Some(form) = &mut app.form {
        if matches!(form.purpose, FormPurpose::PrCollaborationV17(_)) {
            form.submitting = false;
            form.error =
                "Original Conversation queue/ACK unavailable • nothing sent • draft retained"
                    .into();
        }
    }
}
pub fn intercept_close(app: &mut App) -> bool {
    if app
        .form
        .as_ref()
        .is_some_and(|f| matches!(f.purpose, FormPurpose::PrCollaborationV17(_)) && f.submitting)
    {
        app.scoped_pr_collaboration_cancel_v17 = true;
        true
    } else {
        crate::scoped_pr_collaboration_refresh_v19::return_v19::back(app)
    }
}
/// An App-only canceled owner may retain this flag while a replacement queue
/// completes. It is NEVER active Background ownership or cancellation rights.
pub fn retired_presentation(app: &App) -> bool {
    app.scoped_pr_collaboration_owner_v17
        .as_ref()
        .is_some_and(|o| {
            o.display_retired.load(Ordering::Acquire)
                && match o.original() {
                    CollaborationWorkOriginV78::PrPage(original) => original.same_presentation(app),
                    CollaborationWorkOriginV78::StateInbox(_) => false,
                }
        })
}
pub(crate) fn retire_orphan(app: &mut App, owner: &Arc<ScopedPrCollaborationOwnerV17>) {
    let original = match owner.original() {
        CollaborationWorkOriginV78::PrPage(original) => original,
        CollaborationWorkOriginV78::StateInbox(_) => return,
    };
    if let Some(request) = owner.work.refresh_request_v19() {
        crate::scoped_pr_collaboration_refresh_v19::return_v19::retire_orphan_v19(app, request);
        return;
    }
    if owner.work.phase() != ConversationPhaseKindV17::Confirm {
        return;
    }
    let old_form = match &owner.work.kind {
        KindV17::StateV68(state) => app
            .form
            .as_ref()
            .is_some_and(|form| state.matches_retiring_confirm_v74(form)),
        KindV17::IssueConfirmV87(request) => {
            app.form.as_ref().is_some_and(|form| request.matches(form))
        }
        KindV17::MembersConfirmV48(request) => {
            app.form.as_ref().is_some_and(|form| request.matches(form))
        }
        _ => {
            let Some(request) = owner.work.request() else {
                return;
            };
            app.form
                .as_ref()
                .is_some_and(|form| request.matches_form(form))
        }
    };
    let old_overlay = old_form && app.overlay == Some(Overlay::Form);
    let underlay = old_form && original.owns_presentation(app);
    let account_lost = original.retire_account_mismatch(app);
    if old_form {
        app.form = None;
        app.form_dismissed = true;
    }
    if old_overlay {
        app.overlay = None;
        app.hits.clear();
    }
    if underlay && !account_lost {
        owner.display_retired.store(true, Ordering::Release);
        app.context =
            "Original Conversation hidden • refresh explicitly • draft/journal protected".into();
        app.content[View::Inbox.index()] = vec![app.context.clone()];
    }
}
pub(crate) fn retire_terminal(app: &mut App, owner: &Arc<ScopedPrCollaborationOwnerV17>) {
    let original = match owner.original() {
        CollaborationWorkOriginV78::PrPage(original) => original,
        CollaborationWorkOriginV78::StateInbox(_) => return,
    };
    if !original.owns_presentation(app) {
        return;
    }
    let old_form = app.form.as_ref().is_some_and(|f| owner.work.owns_form(f));
    let old_overlay = old_form && app.overlay == Some(Overlay::Form);
    if old_form {
        app.form = None;
        app.form_dismissed = true;
    }
    if old_overlay {
        app.overlay = None;
        app.hits.clear();
    }
    app.github = None;
    app.context = "Original Conversation retired • refresh explicitly • nothing resent".into();
    app.content[View::Inbox.index()] = vec![app.context.clone()];
    app.pr_file_images = None;
    app.pr_native_diff_v1 = None;
    app.pr_native_diff_layout_v1 = Default::default();
    app.pr_reactions = None;
    app.pr_reaction_display.clear();
    app.pr_inspection = None;
    app.avatar_states.clear();
}
pub(crate) fn publish(
    app: &mut App,
    owner: &Arc<ScopedPrCollaborationOwnerV17>,
    prepared: PreparedPrCollaborationV17,
) {
    // Exact ledger state was admitted before any slot/UI/privacy effect.
    // A settled lifecycle is not held open by independently retained old Arcs.
    if let Ok(mut cutoff) = owner.open_cutoff_v20.lock() {
        cutoff.take();
    }
    match prepared.result {
        ResultKindV17::StateSourceV80(result) => {
            if !owner.owns(app) {
                return;
            }
            let Ok((workspace, catalogue, context)) = result.into_parts() else {
                return;
            };
            app.github = Some(workspace);
            app.context = context;
            if let Some(catalogue) = catalogue {
                app.remote_scope_source = Some(catalogue);
            }
            app.inbox_list.selected = app.github.as_ref().map_or(0, |g| g.selected_index());
            app.inbox_list.top = 0;
            app.hits.clear();
            app.status = "Fresh complete State queue observation • read only • no resend".into();
        }
        ResultKindV17::StateV68(result) => {
            ui::pure_close_v20::opened(app, closed_draft_ticket_v20(owner));
            result.apply(app)
        }
        ResultKindV17::IssueNoChangeV102(request, _) => {
            if owner.expired()
                || !owner.owns(app)
                || newer_intent(app)
                || !app
                    .scoped_pr_collaboration_owner_v17
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, owner))
                || !app.form.as_ref().is_some_and(|form| request.matches(form))
            {
                return;
            }
            let form = app.form.as_mut().expect("same actual Issue Form above");
            form.invalidate_plan();
            form.submitting = false;
            let caption="Manual Issue relationship unchanged • nothing sent • fresh Preview required before Confirm";
            form.error = caption.into();
            app.status = caption.into();
        }
        ResultKindV17::IssueUnsupportedV102(original, _) => {
            if owner.expired()
                || !owner.owns(app)
                || newer_intent(app)
                || !app
                    .scoped_pr_collaboration_owner_v17
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, owner))
                || !matches!(owner.original(), CollaborationWorkOriginV78::PrPage(expected)
                    if Arc::ptr_eq(expected, &original))
                || app.form.is_some()
                || app.overlay.is_some()
            {
                return;
            }
            ui::pure_close_v20::opened(app, closed_draft_ticket_v20(owner));
            app.status =
                "Manual Issue-link schema unavailable • nothing sent • no new source or approval"
                    .into();
        }
        ResultKindV17::MetadataSectionV51(section) => {
            crate::scoped_pr_collaboration_source_v17::metadata_context_v51::install(
                *section, app, owner,
            );
        }
        ResultKindV17::MembersNewV52(request, fresh) => {
            if let Some(form) = &mut app.form {
                if ui::metadata_members_v48::accept_new_v52(form, &request, fresh) {
                    form.submitting = false;
                    app.status="New current Members action • fresh Preview required • old original retained inspect-only".into();
                }
            }
        }
        ResultKindV17::RefreshedV19(page) => {
            if let KindV17::RefreshExactV19(request) = &owner.work.kind {
                if page.install_return_v19(app, request) {
                    app.status =
                        "Fresh original PR/search observation • read only • nothing resent".into();
                }
            }
        }
        ResultKindV17::History(form) => {
            app.form = Some(*form);
            app.form_dismissed = false;
            app.overlay = Some(Overlay::Form);
            app.status =
                "Original history • inspect current outcome • local ACK never resends".into();
        }
        ResultKindV17::Acknowledged(reference, changed) => {
            retire_terminal(app, owner);
            app.status = format!(
                "Local ACK #{} • {} barrier released • draft retained • no resend",
                reference.attempt_id, changed
            );
        }
        ResultKindV17::Open(form) => {
            ui::pure_close_v20::opened(app, closed_draft_ticket_v20(owner));
            let metadata = ui::metadata_v25::state(&form).map(|s| s.intent.field.label());
            let members = ui::metadata_members_v48::state(&form).map(|s| s.intent.command.label());
            let issue = ui::issue_link_v87::state(&form).map(|s| s.intent.command.label());
            app.form = Some(*form);
            app.form_dismissed = false;
            app.overlay = Some(Overlay::Form);
            app.status = if let Some(command) = issue {
                format!("Original manual development links • {command} opened • choose actual Issue then fresh Preview")
            } else if let Some(command) = members {
                format!("{command} • choose a value, then Preview and Confirm • nothing changes before Confirm")
            } else if let Some(field) = metadata {
                format!("Original PR {field} opened • fresh Preview + explicit Confirm required")
            } else {
                "Original Conversation opened • fresh Preview + explicit Confirm required".into()
            };
        }
        ResultKindV17::LocalDraftReadFailureV20(failure) => {
            let restored = failure == ReadCutoffFailureV20::SaveFailed
                && ui::pure_close_v20::restore_failed_save(app, closed_draft_ticket_v20(owner));
            app.status = if restored {
                "Local save failed • original editable draft restored • nothing sent".into()
            } else {
                "Original draft read cutoff unavailable • retained draft not replayed • nothing sent".into()
            };
        }
        ResultKindV17::Preview(request, preview) => {
            if let Some(form) = &mut app.form {
                if ui::accept_preview(form, &request, preview) {
                    form.submitting = false;
                    app.status =
                        "Original Conversation Preview • inspect exact effect before Confirm"
                            .into();
                }
            }
        }
        ResultKindV17::IssuePreviewV87(request, preview) => {
            if let Some(form) = &mut app.form {
                if ui::issue_link_v87::accept_preview(form, &request, preview) {
                    form.submitting = false;
                    app.status="Original manual Issue Preview • inspect both targets and all six complete sets before Confirm".into();
                }
            }
        }
        ResultKindV17::MembersPreviewV48(request, preview) => {
            if let Some(form) = &mut app.form {
                if ui::metadata_members_v48::accept_preview(form, &request, preview) {
                    form.submitting = false;
                    app.status =
                        "Original metadata members Preview • exact one-field effect before Confirm"
                            .into();
                }
            }
        }
        ResultKindV17::MetadataPreviewV25(request, preview) => {
            if let Some(form) = &mut app.form {
                if ui::metadata_v25::accept_preview(form, &request, preview) {
                    form.submitting = false;
                    app.status =
                        "Original metadata Preview • inspect one-field effect before Confirm"
                            .into();
                }
            }
        }
        ResultKindV17::BasePreviewV32(request, preview) => {
            if let Some(form) = &mut app.form {
                if ui::metadata_base_v32::accept_preview(form, &request, preview) {
                    form.submitting = false;
                    app.status =
                        "Original Base Preview • inspect exact comparison effect before Confirm"
                            .into();
                }
            }
        }
        ResultKindV17::BaseNoChangeV39(..) => {
            release_busy(app, owner);
            let caption = "Selected Base branch is unchanged • no request sent";
            if let Some(form) = app.form.as_mut() {
                form.error = caption.into();
            }
            app.status = caption.into();
        }
        ResultKindV17::MetadataNoChangeV65(request) => {
            // Admission checked the exact original worker/request. Do not
            // display even this local refusal on a successor Form or epoch.
            if !app.form.as_ref().is_some_and(|form| request.matches(form)) {
                return;
            }
            release_busy(app, owner);
            if let Some(form) = app.form.as_mut() {
                form.error = ui::metadata_v25::METADATA_NO_CHANGE_V65.into();
            }
            app.status = ui::metadata_v25::METADATA_NO_CHANGE_V65.into();
        }
        ResultKindV17::Publication(mut outcome) => {
            let refreshed = outcome
                .refreshed_page_v19
                .take()
                .is_some_and(|page| page.install(app));
            let returned = !refreshed
                && outcome
                    .return_v19
                    .take()
                    .is_some_and(|retained| retained.install(app, owner));
            if !refreshed && !returned {
                retire_terminal(app, owner);
            }
            let label = match outcome.state {
                ConversationPublicationStateV17::Confirmed if refreshed => {
                    "Confirmed • fresh PR/search observation"
                }
                ConversationPublicationStateV17::Confirmed => {
                    "Confirmed; fresh page not admitted; no resend"
                }
                ConversationPublicationStateV17::ConfirmedVisibilityUnavailable => {
                    "Confirmed; visibility unavailable; no resend"
                }
                ConversationPublicationStateV17::ConfirmedJournalUnavailable => {
                    "Observed effect; journal unavailable; no resend"
                }
                ConversationPublicationStateV17::Uncertain => {
                    "Unknown outcome; inspect then local ACK; no resend"
                }
                ConversationPublicationStateV17::NotStartedProtected => {
                    "Not invoked; inspect original; no resend"
                }
            };
            // Safe actual IDs only; private receipt/body/actor never leaves
            // terminal retirement. Do not mislabel unavailable journal as durable.
            let actual = outcome
                .effect
                .as_ref()
                .map(|e| e.resource_id.as_str())
                .unwrap_or("unknown");
            app.status = format!("{label} • ref#{}", outcome.reference.attempt_id);
            app.content[View::Inbox.index()].extend([
                format!("Actual safe effect: {actual} • {}",outcome.observation),
                format!("Original attempt #{} • draft {} • refresh/Inspect explicitly; no automatic retry",outcome.reference.attempt_id,
                    outcome.draft_caption_v85()),
            ]);
        }
        ResultKindV17::Failure {
            account,
            permission,
        } => {
            if let CollaborationWorkOriginV78::StateInbox(source) = owner.original() {
                if !owner.owns(app) {
                    return;
                }
                if account && source.owns(app) {
                    // Only this exact old private queue. No newer page/source,
                    // unrelated owner or installed replacement is retired.
                    app.github = None;
                    app.content[View::Inbox.index()] =
                        vec!["Current account unavailable • old private State queue hidden".into()];
                    app.hits.clear();
                }
                app.status = if account {
                    "State queue account changed • private rows hidden • reopen with current account".into()
                } else {
                    "State source read unavailable • membership remains unavailable • nothing resent".into()
                };
                return;
            }
            // TASK-13: a failed read-only Metadata section read says so; the
            // shown tab stays, and u Metadata reads again.
            let metadata_section =
                owner.work.phase() == ConversationPhaseKindV17::MetadataSectionV51 && !account;
            let caption = if metadata_section && permission {
                METADATA_REFUSED_V13
            } else if metadata_section {
                METADATA_UNAVAILABLE_V13
            } else if permission {
                "Actual capability refused • nothing sent • original draft retained"
            } else {
                "Original Conversation unavailable • nothing resent • draft/journal retained"
            };
            ui::pure_close_v20::opened(app, closed_draft_ticket_v20(owner));
            if let Some(request) = owner.work.refresh_request_v19() {
                if account {
                    crate::scoped_pr_collaboration_refresh_v19::return_v19::retire_account_failure(
                        app, request,
                    );
                } else {
                    release_busy(app, owner);
                    crate::scoped_pr_collaboration_refresh_v19::return_v19::failed_guidance(
                        app, request,
                    );
                }
            } else if account || owner.work.phase() == ConversationPhaseKindV17::Confirm {
                retire_terminal(app, owner);
            } else {
                release_retained_failure_v314(app, owner, caption);
            }
            app.status = caption.into();
        }
    }
}
/// TASK-13: visible error states of the read-only Metadata section read.
pub(crate) const METADATA_UNAVAILABLE_V13: &str =
    "Metadata unavailable • GitHub read failed • u Metadata reads again";
pub(crate) const METADATA_REFUSED_V13: &str =
    "Metadata unavailable • GitHub refused the read • u Metadata reads again";
pub(crate) fn newer_intent(app: &App) -> bool {
    app.pr_action.is_some() || newer_intent_without_pr_v20(app)
}
pub(crate) fn newer_intent_without_pr_v20(app: &App) -> bool {
    app.notifications_work_v63.is_some()
        || app.notifications_owner_v63.is_some()
        || app.notifications_cancel_v63
        || app.github_action.is_some()
        || app.refresh_requested
        || app.personal_pr_entry_work_v13.is_some()
        || app.personal_pr_entry_cancel_v13
        || app.scoped_inline_work_v1.is_some()
        || app.scoped_inline_cancel_v1
        || app.scoped_threads_work_v1.is_some()
        || app.scoped_threads_cancel_v1
        || app.scoped_reviews_work_v2.is_some()
        || app.scoped_reviews_cancel_v2
        || app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app.scoped_actions_work_v1.is_some()
        || app.scoped_actions_cancel_v1
}
pub(crate) fn pending_intent(app: &App) -> bool {
    app.scoped_pr_collaboration_work_v17.is_some()
        || app.scoped_pr_collaboration_cancel_v17
        || app.form.as_ref().is_some_and(|f| {
            matches!(f.purpose, FormPurpose::PrCollaborationV17(_))
                && (f.preview_requested || f.submit_requested)
        })
}
pub(crate) fn drop_old_form(app: &mut App, owner: &ScopedPrCollaborationOwnerV17) {
    if app.form.as_ref().is_some_and(|f| owner.work.owns_form(f)) {
        app.form = None;
        app.form_dismissed = true;
        if app.overlay == Some(Overlay::Form) {
            app.overlay = None;
            app.hits.clear();
        }
    }
}
pub(crate) fn failed(app: &mut App, owner: &Arc<ScopedPrCollaborationOwnerV17>) {
    ui::pure_close_v20::opened(app, closed_draft_ticket_v20(owner));
    if owner.work.refresh_request_v19().is_some() {
        cancel_refresh_v19(app, owner);
    } else if owner.work.phase() == ConversationPhaseKindV17::Confirm {
        retire_terminal(app, owner);
    } else {
        release_retained_failure_v314(
            app,
            owner,
            "Conversation unavailable • draft/reference retained • nothing resent",
        );
    }
    app.status = "Conversation unavailable • draft/reference retained • nothing resent".into();
}
fn closed_draft_ticket_v20(
    owner: &ScopedPrCollaborationOwnerV17,
) -> Option<&Arc<ui::pure_close_v20::ClosedDraftTicketV20>> {
    match &owner.work.kind {
        KindV17::StateV68(state) => state.closed_draft_ticket_v75(),
        KindV17::Open {
            closed_draft_v20, ..
        }
        | KindV17::MetadataOpenV25 {
            closed_draft_v20, ..
        }
        | KindV17::BaseOpenV32 {
            closed_draft_v20, ..
        }
        | KindV17::MembersOpenV48 {
            closed_draft_v20, ..
        }
        | KindV17::IssueOpenV87 {
            closed_draft_v20, ..
        } => closed_draft_v20.as_ref(),
        _ => None,
    }
}
pub(crate) fn cancel_refresh_v19(app: &mut App, owner: &ScopedPrCollaborationOwnerV17) {
    let Some(request) = owner.work.refresh_request_v19() else {
        return;
    };
    if request.owns(app) || request.owns_cancel_v19(app) {
        crate::scoped_pr_collaboration_refresh_v19::return_v19::failed_guidance(app, request);
        release_busy(app, owner);
    }
}
fn release_busy(app: &mut App, owner: &ScopedPrCollaborationOwnerV17) {
    let Some(form) = app.form.as_mut().filter(|f| owner.work.owns_form(f)) else {
        return;
    };
    form.submitting = false;
    form.preview_requested = false;
    form.submit_requested = false;
    if let Some(request) = owner.work.request() {
        request.release_busy(form);
    }
    if let Some(state) = ui::metadata_v25::history::state(form) {
        if let Some(current) = &state.view.current {
            current.cancel.cancel();
            current.painted(false);
        }
    }
    if let Some(state) = ui::state_v68::history_v74::state(form) {
        if let Some(current) = &state.view.current {
            current.cancel.cancel();
            current.painted(false);
        }
    }
    if let FormPurpose::PrCollaborationV17(state) = &form.purpose {
        if let Some(current) = state.history().and_then(|v| v.current.as_ref()) {
            current.cancel.cancel();
            current.painted(false);
        }
    }
}
/// A later genuine autosave ACK may update status, but must not erase the only
/// retained worker-failure explanation. This is display-only and never restores
/// an account/Confirm/refresh Form or changes existing approval invalidation.
fn release_retained_failure_v314(
    app: &mut App,
    owner: &ScopedPrCollaborationOwnerV17,
    caption: &'static str,
) {
    let eligible = owner.work.phase() != ConversationPhaseKindV17::Confirm
        && owner.work.refresh_request_v19().is_none()
        && !app.form_dismissed
        && app.overlay == Some(Overlay::Form)
        && owner.work.matches(app)
        && app
            .form
            .as_ref()
            .is_some_and(|form| owner.work.owns_form(form));
    release_busy(app, owner); // Existing busy/approval/current-history release.
    if eligible && !app.form_dismissed && app.overlay == Some(Overlay::Form) {
        if let Some(form) = app.form.as_mut().filter(|form| owner.work.owns_form(form)) {
            form.error = caption.into();
            form.error_scroll = 0;
        }
    }
}
