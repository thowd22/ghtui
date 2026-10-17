//! Shared Form adapter. Display tickets never grant source/write authority.
use crate::{
    app::{App, Overlay, View},
    forms::{Form, FormPurpose},
    providers::gh::HostIdentity,
    scoped_pr_collaboration_draft_v17::{
        encode_draft_v17, ConversationDraftIntentV17, OpenedConversationDraftV17,
    },
    scoped_pr_collaboration_plan_v17::ConversationPreviewV17,
};
use std::{collections::BTreeMap, sync::Arc};
pub(crate) mod history_close_v296;
#[path = "scoped_pr_issue_link_ui_v87.rs"]
pub(crate) mod issue_link_v87;
#[path = "scoped_pr_metadata_base_v32.rs"]
pub(crate) mod metadata_base_v32;
#[path = "scoped_pr_metadata_members_v48.rs"]
pub(crate) mod metadata_members_v48;
#[path = "scoped_pr_metadata_v25.rs"]
pub(crate) mod metadata_v25;
pub(crate) mod pure_close_v20;
#[path = "scoped_pr_state_v68.rs"]
pub(crate) mod state_v68;
struct TicketV17;
#[derive(Clone)]
struct DraftStateV17 {
    intent: Arc<ConversationDraftIntentV17>,
    identity: HostIdentity,
    ticket: Arc<TicketV17>,
    revision: Option<u64>,
    preview: Option<Arc<ConversationPreviewV17>>,
}
#[derive(Clone)]
enum ComposerModeV17 {
    Draft(DraftStateV17),
    StateV68(state_v68::StateComposerStateV68),
    StateHistoryV74(state_v68::history_v74::StateHistoryStateV74),
    MetadataV25(metadata_v25::MetadataStateV25),
    MetadataMembersV48(metadata_members_v48::MetadataMembersStateV48),
    IssueLinksV87(issue_link_v87::IssueLinkStateV87),
    IssueHistoryV87(issue_link_v87::history::IssueHistoryStateV87),
    MembersHistoryV48(metadata_members_v48::history::MembersHistoryStateV48),
    MetadataBaseV32(metadata_base_v32::BaseStateV32),
    BaseHistoryV32(metadata_base_v32::history::BaseHistoryStateV32),
    MetadataHistoryV25(metadata_v25::history::MetadataHistoryStateV25),
    TerminalReadV19(
        Arc<
            crate::scoped_pr_collaboration_refresh_v19::return_v19::ConversationReturnDescriptorV19,
        >,
    ),
    History {
        view: Arc<crate::scoped_pr_collaboration_history_v17::ConversationHistoryViewV17>,
        ticket: Arc<()>,
    },
}
#[derive(Clone)]
pub struct ConversationComposerStateV17 {
    mode: ComposerModeV17,
}
impl std::fmt::Debug for ConversationComposerStateV17 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConversationComposerStateV17(<presentation; no approval>)")
    }
}
impl ConversationComposerStateV17 {
    pub(crate) fn from_state_history_v74(
        state: state_v68::history_v74::StateHistoryStateV74,
    ) -> Self {
        Self {
            mode: ComposerModeV17::StateHistoryV74(state),
        }
    }
    pub(crate) fn from_state_v68(state: state_v68::StateComposerStateV68) -> Self {
        Self {
            mode: ComposerModeV17::StateV68(state),
        }
    }
    fn author(&self) -> Option<&crate::scoped_pr_collaboration_draft_v17::ConversationActorV17> {
        match &self.mode {
            ComposerModeV17::Draft(v) => v.intent.author.as_ref(),
            ComposerModeV17::StateV68(v) => Some(&v.intent.author),
            ComposerModeV17::StateHistoryV74(v) => Some(&v.view.author),
            ComposerModeV17::MetadataV25(v) => Some(&v.intent.author),
            ComposerModeV17::MetadataMembersV48(v) => Some(&v.intent.author),
            ComposerModeV17::IssueLinksV87(v) => Some(&v.intent.author),
            ComposerModeV17::IssueHistoryV87(v) => Some(&v.view.author),
            ComposerModeV17::MetadataBaseV32(v) => Some(&v.intent.author),
            ComposerModeV17::BaseHistoryV32(v) => Some(&v.view.author),
            ComposerModeV17::MetadataHistoryV25(v) => Some(&v.view.author),
            ComposerModeV17::MembersHistoryV48(v) => Some(&v.view.author),
            ComposerModeV17::History { .. } | ComposerModeV17::TerminalReadV19(_) => None,
        }
    }
    fn draft(&self) -> Option<&DraftStateV17> {
        match &self.mode {
            ComposerModeV17::Draft(v) => Some(v),
            _ => None,
        }
    }
    fn draft_mut(&mut self) -> Option<&mut DraftStateV17> {
        match &mut self.mode {
            ComposerModeV17::Draft(v) => Some(v),
            _ => None,
        }
    }
    pub(crate) fn original(
        &self,
    ) -> &Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17> {
        match &self.mode {
            ComposerModeV17::Draft(v) => v.intent.source.opening(),
            ComposerModeV17::StateV68(v) => v.intent.source.opening(),
            ComposerModeV17::StateHistoryV74(v) => &v.view.original,
            ComposerModeV17::MetadataV25(v) => v.intent.source.opening(),
            ComposerModeV17::MetadataMembersV48(v) => v.intent.source.opening(),
            ComposerModeV17::IssueLinksV87(v) => v.intent.source.opening(),
            ComposerModeV17::IssueHistoryV87(v) => &v.view.original,
            ComposerModeV17::MetadataBaseV32(v) => v.intent.source.opening(),
            ComposerModeV17::BaseHistoryV32(v) => &v.view.original,
            ComposerModeV17::MetadataHistoryV25(v) => &v.view.original,
            ComposerModeV17::MembersHistoryV48(v) => &v.view.original,
            ComposerModeV17::History { view, .. } => &view.original,
            ComposerModeV17::TerminalReadV19(v) => v.original(),
        }
    }
    pub(crate) fn is_history(&self) -> bool {
        matches!(
            self.mode,
            ComposerModeV17::History { .. }
                | ComposerModeV17::StateHistoryV74(_)
                | ComposerModeV17::IssueHistoryV87(_)
                | ComposerModeV17::MetadataHistoryV25(_)
                | ComposerModeV17::MembersHistoryV48(_)
                | ComposerModeV17::BaseHistoryV32(_)
        )
    }
    pub(crate) fn is_terminal_read_v19(&self) -> bool {
        matches!(self.mode, ComposerModeV17::TerminalReadV19(_))
    }
    pub(crate) fn needs_new_v18(&self) -> bool {
        self.draft().is_some_and(|v| v.intent.needs_new_v18)
            || matches!(&self.mode,ComposerModeV17::MetadataMembersV48(state) if state.needs_new_v52)
            || matches!(&self.mode,ComposerModeV17::StateV68(state) if state.needs_new)
            || matches!(&self.mode,ComposerModeV17::IssueLinksV87(state) if state.needs_new())
    }
    pub(crate) fn editable_body(&self) -> bool {
        matches!(self.mode, ComposerModeV17::MetadataV25(_))
            || self.draft().is_some_and(|v| {
                matches!(
                    v.intent.command,
                    crate::pr_actions::PrCommand::NewComment
                        | crate::pr_actions::PrCommand::EditComment
                )
            })
    }
    /// Borrow the actual worker-produced observation, never the requested
    /// choice or an inferred default. History/terminal reads have no draft.
    pub(crate) fn current_subscription(
        &self,
    ) -> Option<&crate::providers::gh::CurrentPrSubscription> {
        self.draft()?.intent.subscription.as_ref()
    }
    /// Desired values from the actual prepared Preview, never a substitute
    /// for the independent provider-observed current subscription.
    pub(crate) fn requested_subscription_preview(&self) -> Option<&str> {
        let draft = self.draft()?;
        if draft.intent.command != crate::pr_actions::PrCommand::Subscription {
            return None;
        }
        draft
            .preview
            .as_ref()?
            .values()
            .get("subscription")
            .map(String::as_str)
    }
    pub(crate) fn history(
        &self,
    ) -> Option<&Arc<crate::scoped_pr_collaboration_history_v17::ConversationHistoryViewV17>> {
        match &self.mode {
            ComposerModeV17::History { view, .. } => Some(view),
            _ => None,
        }
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        match &self.mode {
            ComposerModeV17::Draft(v) => &v.identity,
            ComposerModeV17::StateV68(v) => &v.identity,
            ComposerModeV17::StateHistoryV74(v) => &v.view.identity,
            ComposerModeV17::MetadataV25(v) => &v.identity,
            ComposerModeV17::MetadataMembersV48(v) => &v.identity,
            ComposerModeV17::IssueLinksV87(v) => &v.identity,
            ComposerModeV17::IssueHistoryV87(v) => &v.view.identity,
            ComposerModeV17::MetadataBaseV32(v) => &v.identity,
            ComposerModeV17::BaseHistoryV32(v) => &v.view.identity,
            ComposerModeV17::MetadataHistoryV25(v) => &v.view.identity,
            ComposerModeV17::MembersHistoryV48(v) => &v.view.identity,
            ComposerModeV17::History { view, .. } => &view.identity,
            ComposerModeV17::TerminalReadV19(v) => v.identity(),
        }
    }
    pub(crate) fn draft_key(&self) -> &str {
        match &self.mode {
            ComposerModeV17::Draft(v) => &v.intent.key,
            ComposerModeV17::StateV68(v) => &v.key,
            ComposerModeV17::StateHistoryV74(v) => &v.view.key,
            ComposerModeV17::MetadataV25(v) => &v.intent.key,
            ComposerModeV17::MetadataMembersV48(v) => v
                .key
                .as_deref()
                .unwrap_or("metadata-members-unselected-v48"),
            ComposerModeV17::IssueLinksV87(v) => {
                v.key.as_deref().unwrap_or("manual-issue-unselected-v87")
            }
            ComposerModeV17::MetadataBaseV32(v) => &v.intent.key,
            ComposerModeV17::BaseHistoryV32(v) => &v.view.key,
            ComposerModeV17::MetadataHistoryV25(v) => &v.view.key,
            ComposerModeV17::MembersHistoryV48(v) => &v.view.key,
            ComposerModeV17::IssueHistoryV87(v) => &v.view.key,
            ComposerModeV17::History { view, .. } => &view.key,
            ComposerModeV17::TerminalReadV19(_) => "conversation-terminal-readonly-v19",
        }
    }
    pub(crate) fn ready(&self) -> bool {
        match &self.mode {
            ComposerModeV17::Draft(v) => v.revision.is_some() && v.preview.is_some(),
            ComposerModeV17::StateV68(v) => v.ready(),
            ComposerModeV17::StateHistoryV74(v) => {
                v.view.current.as_ref().is_some_and(|v| v.ready())
            }
            ComposerModeV17::MetadataV25(v) => v.revision.is_some() && v.preview.is_some(),
            ComposerModeV17::MetadataMembersV48(v) => v.revision.is_some() && v.preview.is_some(),
            ComposerModeV17::IssueLinksV87(v) => v.revision.is_some() && v.preview.is_some(),
            ComposerModeV17::MetadataBaseV32(v) => v.revision.is_some() && v.preview.is_some(),
            ComposerModeV17::BaseHistoryV32(v) => {
                v.view.current.as_ref().is_some_and(|v| v.ready())
            }
            ComposerModeV17::MetadataHistoryV25(v) => {
                v.view.current.as_ref().is_some_and(|v| v.ready())
            }
            ComposerModeV17::MembersHistoryV48(v) => {
                v.view.current.as_ref().is_some_and(|v| v.ready())
            }
            ComposerModeV17::IssueHistoryV87(v) => {
                v.view.current.as_ref().is_some_and(|v| v.ready())
            }
            ComposerModeV17::History { view, .. } => {
                view.current.as_ref().is_some_and(|v| v.available())
            }
            ComposerModeV17::TerminalReadV19(_) => false,
        }
    }
    pub(crate) fn invalidate(&mut self) {
        if let ComposerModeV17::StateHistoryV74(state) = &self.mode {
            if let Some(current) = &state.view.current {
                current.painted(false);
            }
            return;
        }
        if let ComposerModeV17::IssueHistoryV87(state) = &self.mode {
            if let Some(current) = &state.view.current {
                current.painted(false);
            }
            return;
        }
        if let ComposerModeV17::StateV68(state) = &mut self.mode {
            state.invalidate();
            return;
        }
        if let ComposerModeV17::IssueLinksV87(state) = &mut self.mode {
            state.invalidate();
            return;
        }
        if let ComposerModeV17::MetadataMembersV48(state) = &mut self.mode {
            state.invalidate();
            return;
        }
        if let ComposerModeV17::MembersHistoryV48(state) = &self.mode {
            if let Some(current) = &state.view.current {
                current.painted(false);
            }
            return;
        }
        if let ComposerModeV17::BaseHistoryV32(state) = &self.mode {
            if let Some(current) = &state.view.current {
                current.painted(false);
            }
            return;
        }
        if let ComposerModeV17::MetadataBaseV32(state) = &mut self.mode {
            state.invalidate();
            return;
        }
        if let ComposerModeV17::MetadataHistoryV25(state) = &self.mode {
            if let Some(current) = &state.view.current {
                current.painted(false);
            }
            return;
        }
        if let ComposerModeV17::MetadataV25(state) = &mut self.mode {
            state.invalidate();
            return;
        }
        if let Some(state) = self.draft_mut() {
            state
                .intent
                .notice_painted_v18
                .store(false, std::sync::atomic::Ordering::Release);
            if let Some(preview) = state.preview.take() {
                preview.painted_v17(false);
            }
            state.revision = state.revision.and_then(|n| n.checked_add(1));
        } else if let Some(view) = self.history() {
            if let Some(current) = &view.current {
                current.painted(false);
            }
        } else if let ComposerModeV17::TerminalReadV19(v) = &self.mode {
            v.painted(false);
        }
    }
    pub(crate) fn plan_lines(&self) -> &[String] {
        match &self.mode {
            ComposerModeV17::Draft(v) => v
                .preview
                .as_ref()
                .map_or(&v.intent.original_lines, |p| p.lines()),
            ComposerModeV17::StateV68(v) => v.lines(),
            ComposerModeV17::StateHistoryV74(v) => &v.view.lines,
            ComposerModeV17::MetadataMembersV48(v) => v.lines(),
            ComposerModeV17::IssueLinksV87(v) => v.lines(),
            ComposerModeV17::IssueHistoryV87(v) => &v.view.lines,
            ComposerModeV17::MetadataV25(v) => {
                v.preview.as_ref().map_or(&v.intent.lines, |p| &p.lines)
            }
            ComposerModeV17::MetadataBaseV32(v) => {
                v.preview.as_ref().map_or(&v.intent.lines, |p| &p.lines)
            }
            ComposerModeV17::BaseHistoryV32(v) => &v.view.lines,
            ComposerModeV17::MetadataHistoryV25(v) => &v.view.lines,
            ComposerModeV17::MembersHistoryV48(v) => &v.view.lines,
            ComposerModeV17::History { view, .. } => &view.lines,
            ComposerModeV17::TerminalReadV19(v) => v.lines(),
        }
    }
    pub(crate) fn context(&self) -> String {
        if let ComposerModeV17::IssueLinksV87(state) = &self.mode {
            return format!(
                "Requester {} • account {} • {} • actual manual development link • no resend",
                crate::git::display_bytes(self.identity().login.as_bytes()),
                self.identity()
                    .account_id
                    .map_or_else(|| "unavailable".into(), |n| n.to_string()),
                state.intent.command.label()
            );
        }
        if matches!(
            self.mode,
            ComposerModeV17::StateV68(_) | ComposerModeV17::StateHistoryV74(_)
        ) {
            return format!(
                "Requester {} • account {} • original PR lifecycle • no resend",
                crate::git::display_bytes(self.identity().login.as_bytes()),
                self.identity()
                    .account_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "unavailable".into())
            );
        }
        if matches!(
            self.mode,
            ComposerModeV17::MetadataV25(_)
                | ComposerModeV17::IssueHistoryV87(_)
                | ComposerModeV17::MetadataMembersV48(_)
                | ComposerModeV17::MetadataHistoryV25(_)
                | ComposerModeV17::MembersHistoryV48(_)
                | ComposerModeV17::MetadataBaseV32(_)
                | ComposerModeV17::BaseHistoryV32(_)
        ) {
            return format!(
                "Requester {} • account {} • original PR metadata • no resend",
                crate::git::display_bytes(self.identity().login.as_bytes()),
                self.identity()
                    .account_id
                    .map(|id| id.to_string())
                    .unwrap_or_else(|| "unavailable".into())
            );
        }
        format!(
            "@{} • {} • account {} • original Conversation • no resend",
            crate::git::display_bytes(self.identity().login.as_bytes()),
            crate::git::display_bytes(self.identity().host.as_bytes()),
            crate::scoped_actions_workflow_ui_v6::account_caption_v25(self.identity().account_id)
        )
    }
}
pub(crate) struct ConversationComposerRequestV17 {
    presentation: Arc<()>,
    input_epoch: u64,
    ticket: Arc<TicketV17>,
    revision: u64,
    intent: Arc<ConversationDraftIntentV17>,
    values: Arc<BTreeMap<String, String>>,
    preview: Option<Arc<ConversationPreviewV17>>,
}
pub(crate) struct ConversationHistoryOpenRequestV17 {
    pub original: Arc<crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17>,
    presentation: Arc<()>,
    epoch: u64,
    ticket: Arc<TicketV17>,
}
pub(crate) struct ConversationNewActionRequestV18 {
    pub(crate) intent: Arc<ConversationDraftIntentV17>,
    presentation: Arc<()>,
    epoch: u64,
    ticket: Arc<TicketV17>,
    revision: u64,
}
impl ConversationNewActionRequestV18 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        matches!(&form.purpose,FormPurpose::PrCollaborationV17(state) if state.draft().is_some_and(|d|Arc::ptr_eq(&d.intent,&self.intent)&&Arc::ptr_eq(&d.ticket,&self.ticket)))
            && Arc::ptr_eq(form.presentation_v4().0, &self.presentation)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        self.owns(form)
            && form.presentation_v4().1 == Some(self.epoch)
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && self
                .intent
                .notice_painted_v18
                .load(std::sync::atomic::Ordering::Acquire)
            && form.publication.as_ref() == self.intent.lineage_v18.as_ref().map(|r| r.reference())
            && matches!(&form.purpose,FormPurpose::PrCollaborationV17(state) if state.draft().is_some_and(|d|d.revision==Some(self.revision)))
    }
}
pub(crate) fn new_action_request_v18(
    form: &Form,
) -> Result<Arc<ConversationNewActionRequestV18>, String> {
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return Err("Not Conversation New action".into());
    };
    let draft = state
        .draft()
        .filter(|d| d.intent.needs_new_v18 && d.intent.lineage_v18.is_some())
        .ok_or("Exact original terminal notice required")?;
    let request = Arc::new(ConversationNewActionRequestV18 {
        intent: draft.intent.clone(),
        presentation: form.presentation_v4().0.clone(),
        epoch: form
            .presentation_v4()
            .1
            .ok_or("Original notice input retired")?,
        ticket: draft.ticket.clone(),
        revision: draft.revision.ok_or("Original notice revision retired")?,
    });
    if !request.matches(form) {
        return Err("Paint the useful exact terminal notice before New action".into());
    }
    Ok(request)
}
pub(crate) fn new_action_ready_v18(form: &Form) -> bool {
    if state_v68::state(form).is_some() {
        return state_v68::new_request(form).is_ok();
    }
    if issue_link_v87::state(form).is_some() {
        return issue_link_v87::new_ready(form);
    }
    if metadata_members_v48::state(form).is_some() {
        return metadata_members_v48::new_request_v52(form).is_ok();
    }
    new_action_request_v18(form).is_ok()
}
impl ConversationHistoryOpenRequestV17 {
    pub(crate) fn owns(&self, form: &Form) -> bool {
        let (presentation, _) = form.presentation_v4();
        matches!(&form.purpose,FormPurpose::PrCollaborationV17(state) if state.draft().is_some_and(|draft|
            Arc::ptr_eq(&draft.ticket,&self.ticket)&&Arc::ptr_eq(draft.intent.source.opening(),&self.original)))
            && Arc::ptr_eq(presentation, &self.presentation)
    }
    pub(crate) fn matches(&self, form: &Form) -> bool {
        let (presentation, epoch) = form.presentation_v4();
        matches!(&form.purpose,FormPurpose::PrCollaborationV17(state) if state.draft().is_some_and(|draft|
            Arc::ptr_eq(&draft.ticket,&self.ticket)&&Arc::ptr_eq(draft.intent.source.opening(),&self.original)))
            && Arc::ptr_eq(presentation, &self.presentation)
            && epoch == Some(self.epoch)
    }
}
pub(crate) fn capture_history_open(
    form: &Form,
) -> Result<Arc<ConversationHistoryOpenRequestV17>, String> {
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return Err("Not original Conversation Form".into());
    };
    let draft = state
        .draft()
        .filter(|_| form.publication.is_some())
        .ok_or("Exact protected original reference required")?;
    let (presentation, epoch) = form.presentation_v4();
    Ok(Arc::new(ConversationHistoryOpenRequestV17 {
        original: draft.intent.source.opening().clone(),
        presentation: presentation.clone(),
        epoch: epoch.ok_or("Original history input epoch retired")?,
        ticket: draft.ticket.clone(),
    }))
}
impl ConversationComposerRequestV17 {
    pub(crate) fn intent(&self) -> &Arc<ConversationDraftIntentV17> {
        &self.intent
    }
    pub(crate) fn values(&self) -> &Arc<BTreeMap<String, String>> {
        &self.values
    }
    pub(crate) fn preview(&self) -> Option<&Arc<ConversationPreviewV17>> {
        self.preview.as_ref()
    }
    pub(crate) fn owns_presentation(&self, form: &Form) -> bool {
        matches!(&form.purpose, FormPurpose::PrCollaborationV17(s)
            if s.draft().is_some_and(|s|Arc::ptr_eq(&s.ticket, &self.ticket) && Arc::ptr_eq(&s.intent, &self.intent)))
    }
    pub(crate) fn matches_form(&self, form: &Form) -> bool {
        let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
            return false;
        };
        let Some(state) = state.draft() else {
            return false;
        };
        let (presentation, epoch) = form.presentation_v4();
        self.owns_presentation(form)
            && Arc::ptr_eq(&self.presentation, presentation)
            && epoch == Some(self.input_epoch)
            && state.revision == Some(self.revision)
            && form.publication.is_none()
            && match (&state.preview, &self.preview) {
                (Some(a), Some(b)) => Arc::ptr_eq(a, b),
                (None, None) => true,
                _ => false,
            }
            && form.fields.len() == self.values.len()
            && form.fields.iter().all(|f| {
                self.values.get(&f.key).map(String::as_str) == Some(f.editor.text.as_str())
            })
    }
    /// Restriction-only admission check for a captured Confirm. No request,
    /// body or source clone is made while checking the exact installed paint.
    pub(crate) fn matches_confirm(&self, form: &Form) -> bool {
        form.preview
            && !form.preview_display_v1.geometry_rejected
            && self.matches_form(form)
            && self.preview.as_ref().is_some_and(|expected| {
                expected.usefully_painted_v18()
                    && matches!(&form.purpose, FormPurpose::PrCollaborationV17(state)
                        if state.draft().and_then(|draft| draft.preview.as_ref())
                            .is_some_and(|actual| Arc::ptr_eq(actual, expected)))
            })
    }
    pub(crate) fn release_busy(&self, form: &mut Form) {
        if self.owns_presentation(form) {
            form.submitting = false;
            form.invalidate_plan();
        }
    }
}
pub(crate) fn form_from_draft(opened: OpenedConversationDraftV17) -> Form {
    let OpenedConversationDraftV17 { intent, mut form } = opened;
    let state = ConversationComposerStateV17 {
        mode: ComposerModeV17::Draft(DraftStateV17 {
            identity: intent.source.identity().clone(),
            intent,
            ticket: Arc::new(TicketV17),
            revision: Some(0),
            preview: None,
        }),
    };
    // All body/field/actor work completed BEFORE the original final UID.
    form.purpose = FormPurpose::PrCollaborationV17(Box::new(state));
    form
}
pub(crate) fn request(form: &Form) -> Result<Arc<ConversationComposerRequestV17>, String> {
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return Err("Not original Conversation Form".into());
    };
    let state = state
        .draft()
        .ok_or("History is never a publication request")?;
    if form.publication.is_some() {
        return Err("Original reference is protected history • inspect, never resend".into());
    }
    if form.fields.len() > 1 || form.fields.iter().any(|f| f.editor.text.len() > 65_536) {
        return Err("Conversation fields exceed preclone body limits".into());
    }
    let values = Arc::new(form.values());
    encode_draft_v17(&values, None)?;
    let (presentation, input_epoch) = form.presentation_v4();
    Ok(Arc::new(ConversationComposerRequestV17 {
        ticket: state.ticket.clone(),
        presentation: presentation.clone(),
        input_epoch: input_epoch.ok_or("Original Form input epoch exhausted")?,
        revision: state.revision.ok_or("Original input epoch retired")?,
        intent: state.intent.clone(),
        values,
        preview: state.preview.clone(),
    }))
}
pub(crate) fn accept_preview(
    form: &mut Form,
    request: &Arc<ConversationComposerRequestV17>,
    preview: Arc<ConversationPreviewV17>,
) -> bool {
    if !request.matches_form(form)
        || !Arc::ptr_eq(request.intent(), preview.intent())
        || !Arc::ptr_eq(request.values(), preview.values())
    {
        return false;
    }
    let FormPurpose::PrCollaborationV17(state) = &mut form.purpose else {
        return false;
    };
    let Some(state) = state.draft_mut() else {
        return false;
    };
    let display = preview.display().clone();
    state.identity = preview.identity().clone();
    state.preview = Some(preview);
    form.preview = true;
    form.preview_requested = false;
    form.submit_requested = false;
    form.preview_scroll = 0;
    form.focus = form.fields.len();
    form.error.clear();
    form.install_prepared_preview_display_v1(display);
    true
}
pub(crate) fn confirmed_preview(form: &Form) -> Result<Arc<ConversationPreviewV17>, String> {
    if !form.preview || form.preview_display_v1.geometry_rejected {
        return Err("Inspect actual useful original Preview before Confirm".into());
    }
    request(form)?
        .preview()
        .cloned()
        .ok_or("Fresh exact Conversation Preview unavailable".into())
}
pub fn encoded_draft(form: &Form) -> Result<String, String> {
    if !matches!(form.purpose, FormPurpose::PrCollaborationV17(_)) {
        return Err("Not original Conversation draft".into());
    }
    if matches!(&form.purpose,FormPurpose::PrCollaborationV17(state) if state.is_history()||state.is_terminal_read_v19())
    {
        return Err("Protected history never autosaves/restores/replays a body".into());
    }
    if metadata_members_v48::state(form).is_some() {
        let request = metadata_members_v48::request(form).map_err(|error| error.message)?;
        return encode_draft_v17(&request.values, form.publication.as_ref());
    }
    if issue_link_v87::state(form).is_some() {
        let request = issue_link_v87::request(form).map_err(|e| e.message)?;
        return encode_draft_v17(&request.values, form.publication.as_ref());
    }
    if metadata_base_v32::state(form).is_some() {
        let values = metadata_base_v32::bounded_values_v32(form)?;
        return crate::scoped_pr_collaboration_draft_v17::encode_base_draft_v32(
            &values,
            form.publication.as_ref(),
        );
    }
    if form.fields.len() > 1 || form.fields.iter().any(|f| f.editor.text.len() > 65_536) {
        return Err("Conversation draft exceeds bounds".into());
    }
    encode_draft_v17(&form.values(), form.publication.as_ref())
}
pub fn replace_editor_result(form: &mut Form, text: &str) -> Result<(), String> {
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return Err("Not original Conversation Form".into());
    };
    if form.preview
        || form.submitting
        || form.publication.is_some()
        || !state.editable_body()
        || text.len() > 65_536
        || form.fields.len() != 1
        || form.fields[0].key != "body"
    {
        return Err("Exact editable Conversation body required; original text retained".into());
    }
    let values = BTreeMap::from([("body".to_owned(), text.to_owned())]);
    encode_draft_v17(&values, None)?;
    let editor = crate::editor::TextBuffer::new(text)?;
    form.fields[0].editor = editor;
    form.dirty = true;
    form.invalidate_plan();
    Ok(())
}
pub(crate) fn painted_preview(form: &Form, useful: bool) {
    if let FormPurpose::PrCollaborationV17(state) = &form.purpose {
        if let Some(current) =
            state_v68::history_v74::state(form).and_then(|s| s.view.current.as_ref())
        {
            current.painted(form.preview && useful);
        }
        if let Some(preview) = state_v68::state(form).and_then(|s| s.preview.as_ref()) {
            preview.painted(form.preview && useful);
        }
        if let Some(current) =
            issue_link_v87::history::state(form).and_then(|s| s.view.current.as_ref())
        {
            current.painted(form.preview && useful);
        }
        if let Some(preview) = issue_link_v87::state(form).and_then(|s| s.preview.as_ref()) {
            preview.painted(form.preview && useful);
        }
        if let Some(preview) = metadata_base_v32::state(form).and_then(|s| s.preview.as_ref()) {
            preview.painted(form.preview && useful);
        }
        if let Some(current) =
            metadata_base_v32::history::state(form).and_then(|s| s.view.current.as_ref())
        {
            current.painted(form.preview && useful);
        }
        if let Some(current) =
            metadata_members_v48::history::state(form).and_then(|state| state.view.current.as_ref())
        {
            current.painted(form.preview && useful);
        }
        if let Some(preview) = metadata_members_v48::state(form).and_then(|s| s.preview.as_ref()) {
            preview.painted(form.preview && useful);
        }
        if let Some(preview) = metadata_v25::state(form).and_then(|s| s.preview.as_ref()) {
            preview.painted(form.preview && useful);
        }
        if let Some(current) =
            metadata_v25::history::state(form).and_then(|s| s.view.current.as_ref())
        {
            current.painted(form.preview && useful);
        }
        if let ComposerModeV17::TerminalReadV19(v) = &state.mode {
            v.painted(form.preview && useful);
        }
        if let Some(draft) = state.draft().filter(|v| v.intent.needs_new_v18) {
            draft
                .intent
                .notice_painted_v18
                .store(form.preview && useful, std::sync::atomic::Ordering::Release);
        }
        if let Some(preview) = state.draft().and_then(|s| s.preview.as_ref()) {
            preview.painted_v17(form.preview && useful);
        }
        if let Some(current) = state.history().and_then(|v| v.current.as_ref()) {
            current.painted(form.preview && useful);
        }
    }
}
pub fn validate_input(form: &Form, incoming: &str) -> Result<(), String> {
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return Ok(());
    };
    if form.submitting || form.preview || form.publication.is_some() || !state.editable_body() {
        if !form.submitting
            && !form.preview
            && form.publication.is_none()
            && matches!(state.mode, ComposerModeV17::MetadataBaseV32(_))
        {
            return Err(
                "Base fields are read only; use F2 Choose branch, then Preview or Close".into(),
            );
        }
        return Err(
            "This Conversation state is read only; use actual typed choices or Close".into(),
        );
    }
    if incoming.len() > 65_536
        || form
            .fields
            .first()
            .map_or(0, |f| f.editor.text.len())
            .checked_add(incoming.len())
            .is_none_or(|n| n > 65_536)
    {
        return Err("Conversation body exceeds64KiB; input not adopted".into());
    }
    Ok(())
}
pub(crate) fn retire_invalid_form(app: &mut App) -> bool {
    pure_close_v20::retire_invalid(app);
    if app
        .form
        .as_ref()
        .is_some_and(|f| return_descriptor_v19(f).is_some())
    {
        return crate::scoped_pr_collaboration_refresh_v19::return_v19::retire_stale(app);
    }
    let Some(original) = app.form.as_ref().and_then(|f| match &f.purpose {
        FormPurpose::PrCollaborationV17(s)
            if app.view != View::Inbox || !s.original().owns_presentation(app) =>
        {
            Some(s.original().clone())
        }
        _ => None,
    }) else {
        return false;
    };
    if original.retire_account_mismatch(app) {
        return true;
    }
    app.form = None;
    app.form_dismissed = true;
    if app.overlay == Some(Overlay::Form) {
        app.overlay = None;
        app.hits.clear();
    }
    true
}

pub(crate) fn form_from_history(
    opened: crate::scoped_pr_collaboration_history_v17::OpenedConversationHistoryV17,
) -> Form {
    let mut form = opened.form;
    form.install_prepared_preview_display_v1(opened.view.display.clone());
    form.purpose = FormPurpose::PrCollaborationV17(Box::new(ConversationComposerStateV17 {
        mode: ComposerModeV17::History {
            view: opened.view,
            ticket: Arc::new(()),
        },
    }));
    form
}
pub(crate) fn history_request(
    form: &Form,
) -> Result<Arc<crate::scoped_pr_collaboration_history_v17::ConversationHistoryRequestV17>, String>
{
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return Err("Not Conversation history".into());
    };
    let ComposerModeV17::History { view, ticket } = &state.mode else {
        return Err("Open original history first".into());
    };
    let mut blocked = view.snapshot.rows.iter().enumerate().filter(|(_, r)| {
        matches!(
            r.marker.state,
            crate::pr_write_journal::PrWriteState::Pending
                | crate::pr_write_journal::PrWriteState::Uncertain
        )
    });
    let row = blocked
        .next()
        .map(|(index, _)| index)
        .ok_or("No supported unresolved Conversation attempt; history remains read only")?;
    if blocked.next().is_some() || !form.fields.is_empty() {
        return Err("Exact single original Conversation blocker unavailable".into());
    }
    let (presentation, epoch) = form.presentation_v4();
    Ok(Arc::new(
        crate::scoped_pr_collaboration_history_v17::ConversationHistoryRequestV17 {
            view: view.clone(),
            row,
            ticket: ticket.clone(),
            presentation: presentation.clone(),
            input_epoch: epoch.ok_or("Original history epoch retired")?,
        },
    ))
}
pub(crate) fn history_matches(
    request: &crate::scoped_pr_collaboration_history_v17::ConversationHistoryRequestV17,
    form: &Form,
) -> bool {
    if !history_owns(request, form) {
        return false;
    }
    form.presentation_v4().1 == Some(request.input_epoch)
}
pub(crate) fn history_owns(
    request: &crate::scoped_pr_collaboration_history_v17::ConversationHistoryRequestV17,
    form: &Form,
) -> bool {
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return false;
    };
    let ComposerModeV17::History { view, ticket } = &state.mode else {
        return false;
    };
    let (presentation, _) = form.presentation_v4();
    Arc::ptr_eq(view, &request.view)
        && Arc::ptr_eq(ticket, &request.ticket)
        && Arc::ptr_eq(presentation, &request.presentation)
        && form.fields.is_empty()
        && view.snapshot.rows.get(request.row).is_some()
}
pub(crate) fn history_ack_ready(form: &Form) -> bool {
    if form.preview_display_v1.geometry_rejected || !form.preview {
        return false;
    }
    let Ok(request) = history_request(form) else {
        return false;
    };
    request.view.current.as_ref().is_some_and(|current| {
        current.available()
            && request.view.snapshot.rows[request.row]
                .marker
                .publication_reference()
                == current.reference
    })
}
pub(crate) fn footer(form: &Form) -> Option<Vec<(&'static str, crate::forms::FormAction)>> {
    use crate::forms::FormAction;
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return None;
    };
    if form.submitting {
        return Some(vec![("[Esc Cancel]", FormAction::Close)]);
    }
    if metadata_members_v48::history::state(form).is_some()
        || issue_link_v87::history::state(form).is_some()
        || metadata_v25::history::state(form).is_some()
        || metadata_base_v32::history::state(form).is_some()
    {
        Some(vec![
            ("[Inspect current]", FormAction::Preview),
            ("[Acknowledge local]", FormAction::Submit),
            ("[Esc Close]", FormAction::Close),
        ])
    } else if state.is_terminal_read_v19() {
        Some(vec![
            ("[r Refresh PR]", FormAction::RefreshConversationV19),
            ("[Esc Back to Inbox]", FormAction::Close),
        ])
    } else if state.is_history() {
        Some(vec![
            ("[Inspect current]", FormAction::Preview),
            ("[Acknowledge local]", FormAction::Submit),
            (
                if form.submitting {
                    "[Esc Cancel]"
                } else {
                    "[Esc Close]"
                },
                FormAction::Close,
            ),
        ])
    } else if state.needs_new_v18() {
        Some(vec![
            ("[n New action]", FormAction::NewIntent),
            ("[Inspect history]", FormAction::InspectConversationV17),
            ("[Esc Close]", FormAction::Close),
        ])
    } else if form.publication.is_some() {
        Some(vec![
            ("[Inspect history]", FormAction::InspectConversationV17),
            ("[Esc Close]", FormAction::Close),
        ])
    } else if issue_link_v87::state(form).is_some() {
        Some(if form.preview {
            vec![
                ("[Preview]", FormAction::Preview),
                ("[Confirm action]", FormAction::Submit),
                ("[Keep draft / Close]", FormAction::Close),
            ]
        } else {
            vec![
                ("[Choose Issue]", FormAction::OpenPicker(0)),
                ("[Preview]", FormAction::Preview),
                ("[Esc Close]", FormAction::Close),
            ]
        })
    } else if metadata_members_v48::state(form).is_some() && !form.preview {
        Some(
            if metadata_members_v48::state(form).is_some_and(|state| {
                state.intent.command
                    == crate::pr_metadata_actions::PrMetadataCommand::ClearMilestone
            }) {
                vec![
                    ("[Preview]", FormAction::Preview),
                    ("[Esc Close]", FormAction::Close),
                ]
            } else {
                vec![
                    ("[Choose member]", FormAction::OpenPicker(0)),
                    ("[Preview]", FormAction::Preview),
                    ("[Esc Close]", FormAction::Close),
                ]
            },
        )
    } else if state_v68::state(form).is_some() {
        Some(if form.preview {
            vec![
                ("[Preview]", FormAction::Preview),
                ("[Confirm action]", FormAction::Submit),
                ("[Esc Close]", FormAction::Close),
            ]
        } else {
            vec![
                ("[Preview]", FormAction::Preview),
                ("[Esc Close]", FormAction::Close),
            ]
        })
    } else if metadata_base_v32::state(form).is_some() && !form.preview {
        Some(vec![
            ("[F2 Choose branch]", FormAction::OpenPicker(0)),
            ("[Preview]", FormAction::Preview),
            ("[Esc Close]", FormAction::Close),
        ])
    } else {
        None
    }
}
pub(crate) fn dispatch(form: &mut Form, action: &crate::forms::FormAction) -> bool {
    use crate::forms::FormAction;
    if state_v68::history_v74::dispatch(form, action) {
        return true;
    }
    if state_v68::dispatch(form, action) {
        return true;
    }
    if issue_link_v87::history::state(form).is_some() {
        return match action {
            FormAction::Preview => {
                form.preview_requested = true;
                true
            }
            FormAction::Submit => {
                if issue_link_v87::history::ack_ready(form) {
                    form.submit_requested = true;
                } else {
                    form.error =
                        "Fresh useful exact PR+Issue inspection required before LOCAL ACK".into();
                }
                true
            }
            FormAction::Editor | FormAction::NewIntent => true,
            _ => false,
        };
    }
    if issue_link_v87::dispatch(form, action) {
        return true;
    }
    if metadata_members_v48::history::state(form).is_some() {
        return match action {
            FormAction::Preview => {
                form.preview_requested = true;
                true
            }
            FormAction::Submit => {
                if metadata_members_v48::history::ack_ready(form) {
                    form.submit_requested = true;
                } else {
                    form.error =
                        "Fresh useful exact member inspection required before LOCAL ACK".into();
                }
                true
            }
            FormAction::Editor | FormAction::NewIntent => true,
            _ => false,
        };
    }
    if metadata_members_v48::dispatch(form, action) {
        return true;
    }
    if metadata_base_v32::dispatch(form, action) {
        return true;
    }
    if metadata_base_v32::history::state(form).is_some() {
        return match action {
            FormAction::Preview => {
                form.preview_requested = true;
                true
            }
            FormAction::Submit => {
                if metadata_base_v32::history::ack_ready(form) {
                    form.submit_requested = true;
                } else {
                    form.error="Fresh useful exact Base inspection required before LOCAL ACK • never resend".into();
                }
                true
            }
            FormAction::Editor | FormAction::NewIntent | FormAction::InspectConversationV17 => true,
            _ => false,
        };
    }
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return false;
    };
    if metadata_v25::history::state(form).is_some() {
        return match action {
            FormAction::Preview => {
                form.preview_requested = true;
                true
            }
            FormAction::Submit => {
                if metadata_v25::history::ack_ready(form) {
                    form.submit_requested = true;
                } else {
                    form.error = "Fresh useful exact metadata inspection required before LOCAL ACK • never resend".into();
                }
                true
            }
            FormAction::Editor | FormAction::NewIntent | FormAction::InspectConversationV17 => true,
            _ => false,
        };
    }
    if metadata_v25::state(form).is_some() && *action == FormAction::Editor {
        if form.submitting
            || form.publication.is_some()
            || form.preview_requested
            || form.submit_requested
            || form.editor_requested
            || form.fields.len() != 1
            || form.fields[0].key != "body"
            || form.fields[0].editor.text.len() > 65_536
        {
            form.error =
                "Exact idle editable metadata field required; original text retained".into();
            return true;
        }
        // Retire the exact prepared approval BEFORE Main admits an external
        // editor. Cancellation/error keeps the same draft, never its approval.
        // This is the same invalidation used by an actual field edit; it grants
        // neither a new Preview nor source/write permission.
        form.invalidate_plan();
        form.editor_requested = true;
        return true;
    }
    if metadata_v25::state(form).is_some() && *action == FormAction::Submit {
        // Metadata carries its own exact prepared Preview, not a Conversation
        // Draft. Use that same current Form/paint proof before requesting work.
        if metadata_v25::request(form).is_ok_and(|request| request.matches_confirm(form)) {
            form.submit_requested = true;
        } else {
            form.error =
                "Fresh exact usefully painted metadata Preview required before Confirm".into();
        }
        return true;
    }
    match action {
        FormAction::RefreshConversationV19 if state.is_terminal_read_v19() => {
            if return_ready_v19(form) {
                form.preview_requested = true;
            } else {
                form.error = if return_descriptor_v19(form).is_some_and(|d| d.consumed_v19()) {
                    "Retained fresh-read attempt consumed • Back and reopen from a genuine current source • no read queued".into()
                } else {
                    "Resize/paint the exact useful read-only terminal document first • no read queued".into()
                };
            }
            true
        }
        FormAction::Preview | FormAction::Submit | FormAction::Editor | FormAction::NewIntent
            if state.is_terminal_read_v19() =>
        {
            true
        }
        FormAction::Preview if state.needs_new_v18() => true,
        FormAction::NewIntent if state.needs_new_v18() => {
            match new_action_request_v18(form) {
                Ok(_) => form.submit_requested = true,
                Err(reason) => form.error = reason,
            }
            true
        }
        FormAction::InspectConversationV17 if !state.is_history() && form.publication.is_some() => {
            form.preview_requested = true;
            true
        }
        FormAction::InspectConversationV17 => true,
        FormAction::Preview if state.is_history() => {
            form.preview_requested = true;
            true
        }
        FormAction::Submit if state.is_history() => {
            if history_ack_ready(form) {
                form.submit_requested = true;
            } else {
                form.error="Inspect this exact current protected attempt and useful observation first • local ACK never resends".into();
            }
            true
        }
        _ => false,
    }
}
pub(crate) fn draw_author(
    frame: &mut ratatui::Frame,
    area: ratatui::layout::Rect,
    form: &Form,
    states: &crate::actor_avatar_states_v1::ActorAvatarStatesV1,
    theme: crate::theme::Theme,
) -> u16 {
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return 0;
    };
    let Some(author) = state.author() else {
        return 0;
    };
    if area.height == 0 || area.width < 8 {
        return 0;
    }
    frame.render_widget(
        crate::identity::AvatarWidget {
            identity: &author.identity,
            image: states.ready(&author.identity),
            theme,
        },
        ratatui::layout::Rect::new(area.x, area.y, 4, 1),
    );
    frame.render_widget(
        ratatui::widgets::Paragraph::new(author.label.as_ref())
            .style(theme.style(crate::theme::SemanticToken::Muted)),
        ratatui::layout::Rect::new(area.x + 4, area.y, area.width - 4, 1),
    );
    1
}
/// Active surface first, never hidden Github fallback. Shared cache/client,
/// bounded actor preflight and observed URL/numeric partition remain unchanged.
pub(crate) fn avatar_surface(
    app: &App,
) -> Option<(HostIdentity, String, Vec<crate::providers::gh::Actor>)> {
    if app.view != View::Inbox || app.overlay != Some(Overlay::Form) || app.form_dismissed {
        return None;
    }
    let form = app.form.as_ref()?;
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return None;
    };
    if !state.original().owns_presentation(app)
        || crate::scoped_pr_collaboration_native_v17::retired_presentation(app)
    {
        return None;
    }
    let requester = crate::providers::gh::Actor {
        login: state.identity().login.clone(),
        ..Default::default()
    };
    let authors: Vec<&crate::providers::gh::Actor> = match &state.mode {
        ComposerModeV17::Draft(v) => v.intent.author.iter().map(|a| &a.observed).collect(),
        ComposerModeV17::StateV68(v) => vec![&v.intent.author.observed],
        ComposerModeV17::StateHistoryV74(v) => vec![&v.view.author.observed],
        ComposerModeV17::MetadataV25(v) => vec![&v.intent.author.observed],
        ComposerModeV17::MetadataMembersV48(v) => vec![&v.intent.author.observed],
        ComposerModeV17::IssueLinksV87(v) => vec![&v.intent.author.observed],
        ComposerModeV17::IssueHistoryV87(v) => vec![&v.view.author.observed],
        ComposerModeV17::MetadataBaseV32(v) => vec![&v.intent.author.observed],
        ComposerModeV17::BaseHistoryV32(v) => vec![&v.view.author.observed],
        ComposerModeV17::MetadataHistoryV25(v) => vec![&v.view.author.observed],
        ComposerModeV17::MembersHistoryV48(v) => vec![&v.view.author.observed],
        ComposerModeV17::History { view, .. } => view
            .snapshot
            .rows
            .iter()
            .filter_map(|r| r.origin.seed().selected.as_ref().map(|s| &s.author))
            .take(127)
            .collect(),
        ComposerModeV17::TerminalReadV19(_) => Vec::new(),
    };
    let actors =
        crate::actor_avatar_projection_v1::collect(std::iter::once(&requester).chain(authors))
            .unwrap_or_default();
    Some((
        state.identity().clone(),
        "Original Conversation display actors (not permissions)".into(),
        actors,
    ))
}
pub(crate) fn return_descriptor_v19(
    form: &Form,
) -> Option<
    &Arc<crate::scoped_pr_collaboration_refresh_v19::return_v19::ConversationReturnDescriptorV19>,
> {
    match &form.purpose {
        FormPurpose::PrCollaborationV17(state) => match &state.mode {
            ComposerModeV17::TerminalReadV19(v) => Some(v),
            _ => None,
        },
        _ => None,
    }
}
pub(crate) fn return_ready_v19(form: &Form) -> bool {
    return_descriptor_v19(form).is_some_and(|d| {
        d.useful()
            && form.preview
            && !form.preview_display_v1.geometry_rejected
            && form
                .preview_display_v1
                .document
                .as_ref()
                .is_some_and(|doc| Arc::ptr_eq(doc, d.display()))
            && form.fields.is_empty()
            && !form.dirty
            && form.publication.is_none()
    })
}
pub(crate) fn form_from_return_v19(
    descriptor: Arc<
        crate::scoped_pr_collaboration_refresh_v19::return_v19::ConversationReturnDescriptorV19,
    >,
) -> Result<Form, crate::providers::gh::GithubError> {
    let mut form = Form::pr(crate::forms::PrIntent {
        kind: crate::forms::PrFormKind::Comment,
        target: descriptor.source().parent().summary().clone(),
        identity: descriptor.identity().clone(),
    })
    .map_err(|_| crate::providers::gh::GithubError {
        kind: crate::providers::gh::GithubErrorKind::InvalidTarget,
        message: "Original terminal display unavailable".into(),
    })?;
    form.title = "Confirmed Conversation • read-only refresh".into();
    form.fields.clear();
    form.focus = 0;
    form.active_field = 0;
    form.preview = true;
    form.dirty = false;
    form.publication = None;
    form.install_prepared_preview_display_v1(Arc::clone(descriptor.display()));
    form.purpose = FormPurpose::PrCollaborationV17(Box::new(ConversationComposerStateV17 {
        mode: ComposerModeV17::TerminalReadV19(descriptor),
    }));
    Ok(form)
}
