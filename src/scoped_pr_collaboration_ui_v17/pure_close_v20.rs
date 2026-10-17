//! ONE moved inert presentation, not a saved source, permission or replay.
//! Acceptance by DraftWriter is NOT durable success. Its real ordered ledger
//! must settle before a new genuine Open reads the local draft.
use super::*;
use crate::{pr_actions::PrCommand, pr_collaboration_origin_v17::PrCollaborationOpenIntentV17};

pub(crate) struct ClosedDraftTicketV20 {
    original: Arc<PrCollaborationOpenIntentV17>,
    presentation: Arc<()>,
    epoch: u64,
    purpose: ClosedPurposeV25,
    source: Arc<crate::scoped_pr_collaboration_source_v17::PrCollaborationSourceV17>,
}
enum ClosedPurposeV25 {
    StateV68 {
        command: crate::pr_state_model::PrStateCommand,
        ticket: Arc<()>,
    },
    Conversation {
        command: PrCommand,
        ticket: Arc<TicketV17>,
    },
    Metadata {
        field: crate::providers::gh::PrMetadataTextFieldV25,
        ticket: Arc<()>,
    },
    BaseV32 {
        ticket: Arc<()>,
    },
    MembersV48 {
        command: crate::pr_metadata_actions::PrMetadataCommand,
        ticket: Arc<()>,
    },
    IssueLinksV102 {
        command: crate::pr_issue_links_v87::IssueLinkCommandV87,
        ticket: Arc<()>,
    },
}
fn purpose(
    form: &Form,
) -> Option<(
    ClosedPurposeV25,
    Arc<crate::scoped_pr_collaboration_source_v17::PrCollaborationSourceV17>,
)> {
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return None;
    };
    if let Some(draft) = state
        .draft()
        .filter(|d| !d.intent.needs_new_v18 && d.preview.is_none())
    {
        Some((
            ClosedPurposeV25::Conversation {
                command: draft.intent.command,
                ticket: draft.ticket.clone(),
            },
            draft.intent.source.clone(),
        ))
    } else if let Some(draft) = issue_link_v87::state(form).filter(|d| d.preview.is_none()) {
        Some((
            ClosedPurposeV25::IssueLinksV102 {
                command: draft.intent.command,
                ticket: Arc::clone(&draft.ticket),
            },
            Arc::clone(&draft.intent.source),
        ))
    } else if let Some(draft) =
        state_v68::state(form).filter(|d| d.preview.is_none() && !d.needs_new)
    {
        Some((
            ClosedPurposeV25::StateV68 {
                command: draft.intent.command,
                ticket: Arc::clone(&draft.ticket),
            },
            Arc::clone(&draft.intent.source),
        ))
    } else if let Some(draft) =
        metadata_members_v48::state(form).filter(|d| d.preview.is_none() && !d.needs_new_v52)
    {
        Some((
            ClosedPurposeV25::MembersV48 {
                command: draft.intent.command,
                ticket: Arc::clone(&draft.ticket),
            },
            Arc::clone(&draft.intent.source),
        ))
    } else if let Some(draft) = metadata_base_v32::state(form).filter(|d| d.preview.is_none()) {
        Some((
            ClosedPurposeV25::BaseV32 {
                ticket: draft.ticket.clone(),
            },
            draft.intent.source.clone(),
        ))
    } else {
        let draft = metadata_v25::state(form).filter(|d| d.preview.is_none())?;
        Some((
            ClosedPurposeV25::Metadata {
                field: draft.intent.field,
                ticket: draft.ticket.clone(),
            },
            draft.intent.source.clone(),
        ))
    }
}
pub(crate) struct ClosedConversationDraftV20 {
    form: Form,
    ticket: Arc<ClosedDraftTicketV20>,
}
pub(crate) fn can_close(app: &App) -> bool {
    if app.overlay != Some(Overlay::Form) || app.form_dismissed {
        return false;
    }
    let Some(form) = app.form.as_ref() else {
        return false;
    };
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return false;
    };
    (state
        .draft()
        .is_some_and(|draft| !draft.intent.needs_new_v18)
        || metadata_v25::state(form).is_some()
        || metadata_members_v48::state(form).is_some_and(|state| !state.needs_new_v52)
        || metadata_base_v32::state(form).is_some()
        || state_v68::state(form).is_some_and(|state| !state.needs_new)
        || issue_link_v87::state(form).is_some_and(|state| !state.needs_new()))
        && state.original().owns_presentation(app)
        && !form.dirty
        && !form.submitting
        && !form.submitted
        && !form.preview_requested
        && !form.submit_requested
        && !form.editor_requested
        && form.publication.is_none()
        && form.actions_publication.is_none()
        && form.picker.is_none()
        && form.picker_requested.is_none()
        && form.metadata_picker.is_none()
        && metadata_base_v32::picker(form).is_none()
        && issue_link_v87::picker(form).is_none()
        && app.closed_conversation_draft_v20.is_none()
        && app.view == View::Inbox
        && !app.loading[View::Inbox.index()]
        && !crate::scoped_pr_collaboration_native_v17::newer_intent(app)
        && app.scoped_pr_collaboration_work_v17.is_none()
        && app.scoped_pr_collaboration_owner_v17.is_none()
        && !app.scoped_pr_collaboration_cancel_v17
        && app.scoped_inline_owner_v1.is_none()
        && app.scoped_threads_owner_v1.is_none()
        && app.scoped_reviews_owner_v2.is_none()
        && app.pr_native_diff_owner_v1.is_none()
        && app.personal_pr_entry_owner_v13.is_none()
        && app.scoped_actions_owner_v1.is_none()
}
fn idle(app: &App) -> bool {
    app.view == View::Inbox
        && app.overlay.is_none()
        && !app.loading[View::Inbox.index()]
        && !crate::scoped_pr_collaboration_native_v17::newer_intent_without_pr_v20(app)
        && app.scoped_pr_collaboration_work_v17.is_none()
        && app.scoped_pr_collaboration_owner_v17.is_none()
        && !app.scoped_pr_collaboration_cancel_v17
        && app.scoped_inline_owner_v1.is_none()
        && app.scoped_threads_owner_v1.is_none()
        && app.scoped_reviews_owner_v2.is_none()
        && app.pr_native_diff_owner_v1.is_none()
        && app.personal_pr_entry_owner_v13.is_none()
        && app.scoped_actions_owner_v1.is_none()
}
pub(crate) fn form_closed(app: &mut App) {
    if !idle(app)
        || app.pr_action.is_some()
        || !app.form_dismissed
        || app.closed_conversation_draft_v20.is_some()
    {
        return;
    }
    let Some(form) = app.form.as_ref() else {
        return;
    };
    let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
        return;
    };
    let Some((purpose, source)) = purpose(form) else {
        return;
    };
    if !state.original().owns_presentation(app)
        || form.dirty
        || form.submitting
        || form.submitted
        || form.preview_requested
        || form.submit_requested
        || form.editor_requested
        || form.publication.is_some()
        || form.actions_publication.is_some()
        || form.picker.is_some()
        || form.picker_requested.is_some()
        || form.metadata_picker.is_some()
        || metadata_base_v32::picker(form).is_some()
        || issue_link_v87::picker(form).is_some()
    {
        return;
    }
    let (presentation, Some(epoch)) = form.presentation_v4() else {
        return;
    };
    let ticket = Arc::new(ClosedDraftTicketV20 {
        original: state.original().clone(),
        presentation: presentation.clone(),
        epoch,
        purpose,
        source,
    });
    let form = app.form.take().expect("same borrowed actual Form");
    app.closed_conversation_draft_v20 = Some(ClosedConversationDraftV20 { form, ticket });
    app.hits.clear();
}
pub(crate) fn retire_invalid(app: &mut App) {
    if app
        .closed_conversation_draft_v20
        .as_ref()
        .is_some_and(|closed| {
            app.view != View::Inbox
                || !closed.ticket.original.owns_presentation(app)
                || app.form.is_some()
                || app.github_action == Some(crate::github_workspace::GithubAction::ClearCache)
        })
    {
        app.closed_conversation_draft_v20 = None;
    }
}
pub(crate) fn capture_ticket(app: &App, command: PrCommand) -> Option<Arc<ClosedDraftTicketV20>> {
    let closed = app.closed_conversation_draft_v20.as_ref()?;
    (idle(app)
        && app.form.is_none()
        && matches!(&closed.ticket.purpose, ClosedPurposeV25::Conversation { command: original, .. } if *original == command)
        && app
            .pr_action
            .as_ref()
            .is_none_or(|action| *action == crate::app::PrUiAction::Command(command))
        && closed.ticket.original.owns_presentation(app))
    .then(|| closed.ticket.clone())
}
pub(crate) fn capture_metadata_ticket_v25(
    app: &App,
    field: crate::providers::gh::PrMetadataTextFieldV25,
) -> Option<Arc<ClosedDraftTicketV20>> {
    let closed = app.closed_conversation_draft_v20.as_ref()?;
    (idle(app) && app.form.is_none()
        && matches!(&closed.ticket.purpose, ClosedPurposeV25::Metadata { field: original, .. } if *original == field)
        && app.pr_action.as_ref().is_none_or(|action| *action == crate::app::PrUiAction::MetadataTextV25(field))
        && closed.ticket.original.owns_presentation(app)).then(|| closed.ticket.clone())
}
fn same_slot(app: &App, ticket: &Arc<ClosedDraftTicketV20>) -> bool {
    app.closed_conversation_draft_v20.as_ref().is_some_and(|closed|
        Arc::ptr_eq(&closed.ticket, ticket)
            && Arc::ptr_eq(closed.form.presentation_v4().0, &ticket.presentation)
            && closed.form.presentation_v4().1 == Some(ticket.epoch)
            && match &ticket.purpose {
                ClosedPurposeV25::StateV68 {command,ticket}=>state_v68::state(&closed.form).is_some_and(|s|s.intent.command==*command&&Arc::ptr_eq(&s.ticket,ticket)),
                ClosedPurposeV25::Conversation { command, ticket } => matches!(&closed.form.purpose, FormPurpose::PrCollaborationV17(state)
                    if state.draft().is_some_and(|draft| draft.intent.command == *command && Arc::ptr_eq(&draft.ticket, ticket))),
                ClosedPurposeV25::Metadata { field, ticket } => metadata_v25::state(&closed.form)
                    .is_some_and(|draft| draft.intent.field == *field && Arc::ptr_eq(&draft.ticket, ticket)),
                ClosedPurposeV25::BaseV32 {ticket} => metadata_base_v32::state(&closed.form).is_some_and(|draft|Arc::ptr_eq(&draft.ticket,ticket)),
                ClosedPurposeV25::MembersV48 {command,ticket}=>metadata_members_v48::state(&closed.form).is_some_and(|draft|draft.intent.command==*command&&Arc::ptr_eq(&draft.ticket,ticket)),
                ClosedPurposeV25::IssueLinksV102 {command,ticket}=>issue_link_v87::state(&closed.form).is_some_and(|draft|draft.intent.command==*command&&Arc::ptr_eq(&draft.ticket,ticket)),
            })
}
pub(crate) fn capture_base_ticket_v32(app: &App) -> Option<Arc<ClosedDraftTicketV20>> {
    let closed = app.closed_conversation_draft_v20.as_ref()?;
    (idle(app)
        && app.form.is_none()
        && matches!(&closed.ticket.purpose, ClosedPurposeV25::BaseV32 { .. })
        && app
            .pr_action
            .as_ref()
            .is_none_or(|a| *a == crate::app::PrUiAction::MetadataBaseV32)
        && closed.ticket.original.owns_presentation(app))
    .then(|| closed.ticket.clone())
}
pub(crate) fn capture_state_ticket_v75(
    app: &App,
    command: crate::pr_state_model::PrStateCommand,
) -> Option<Arc<ClosedDraftTicketV20>> {
    let closed = app.closed_conversation_draft_v20.as_ref()?;
    (idle(app)&&app.form.is_none()
        &&matches!(&closed.ticket.purpose,ClosedPurposeV25::StateV68 {command:actual,..} if *actual==command)
        &&app.pr_action.as_ref().is_none_or(|action|*action==crate::app::PrUiAction::StateV68(command))
        &&closed.ticket.original.owns_presentation(app)).then(||Arc::clone(&closed.ticket))
}
pub(crate) fn opened(app: &mut App, ticket: Option<&Arc<ClosedDraftTicketV20>>) {
    if ticket.is_some_and(|t| same_slot(app, t)) {
        app.closed_conversation_draft_v20 = None;
    }
}
pub(crate) fn capture_members_ticket_v48(
    app: &App,
    command: crate::pr_metadata_actions::PrMetadataCommand,
) -> Option<Arc<ClosedDraftTicketV20>> {
    let closed = app.closed_conversation_draft_v20.as_ref()?;
    (idle(app)&&app.form.is_none()&&matches!(&closed.ticket.purpose,ClosedPurposeV25::MembersV48{command:actual,..} if *actual==command)&&app.pr_action.as_ref().is_none_or(|action|*action==crate::app::PrUiAction::MetadataMembersV48(command))&&closed.ticket.original.owns_presentation(app)).then(||Arc::clone(&closed.ticket))
}
pub(crate) fn capture_issue_ticket_v102(
    app: &App,
    command: crate::pr_issue_links_v87::IssueLinkCommandV87,
) -> Option<Arc<ClosedDraftTicketV20>> {
    let closed = app.closed_conversation_draft_v20.as_ref()?;
    (idle(app)
        && app.form.is_none()
        && matches!(&closed.ticket.purpose, ClosedPurposeV25::IssueLinksV102 {command:actual,..} if *actual==command)
        && app.pr_action.as_ref().is_none_or(|action| *action==crate::app::PrUiAction::IssueLinksV87(command))
        && closed.ticket.original.owns_presentation(app))
        .then(|| Arc::clone(&closed.ticket))
}
pub(crate) fn validate_local_recovery(
    ticket: &ClosedDraftTicketV20,
    database: &std::path::Path,
    cancel: &crate::providers::runner::CancellationToken,
    deadline: std::time::Instant,
) -> crate::providers::gh::GithubResult<()> {
    ticket
        .source
        .validate_local_draft_recovery_v20(database, cancel, deadline)
}
/// Called ONLY for the closed local SaveFailed variant, after the SAME Inbox
/// owner/generation/source/result admission. Provider/privacy/clock failures
/// never use this path. No autosave or SQL is replayed.
pub(crate) fn restore_failed_save(
    app: &mut App,
    ticket: Option<&Arc<ClosedDraftTicketV20>>,
) -> bool {
    let Some(ticket) = ticket else {
        return false;
    };
    if !same_slot(app, ticket)
        || app.view != View::Inbox
        || app.form.is_some()
        || app.overlay.is_some()
        || !ticket.original.owns_presentation(app)
        || crate::scoped_pr_collaboration_native_v17::newer_intent(app)
        || app.scoped_pr_collaboration_work_v17.is_some()
        || app.scoped_pr_collaboration_cancel_v17
    {
        return false;
    }
    let mut closed = app
        .closed_conversation_draft_v20
        .take()
        .expect("exact slot above");
    closed.form.preview = false;
    closed.form.preview_requested = false;
    closed.form.submit_requested = false;
    closed.form.submitting = false;
    // false means accepted, not durable. An explicit later edit can save; this
    // failure path must not automatically replay the failed Save.
    closed.form.dirty = false;
    closed.form.error = "Local draft save failed. Your original editable body is retained here; edit explicitly to retry saving. Nothing sent to GitHub.".into();
    app.form = Some(closed.form);
    app.form_dismissed = false;
    app.overlay = Some(Overlay::Form);
    app.hits.clear();
    true
}
