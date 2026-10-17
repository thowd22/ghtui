//! The existing Inbox queue owns notification reads. No new worker/transport,
//! PR source, publication authority or background-scope fallback lives here.
use crate::{
    app::{App, Overlay, View},
    notifications_v63::*,
};
use std::sync::Arc;

pub(crate) fn newer_intent(app: &App) -> bool {
    crate::scoped_pr_collaboration_native_v17::pending_intent(app)
        || app.scoped_pr_collaboration_owner_v17.is_some()
        || app.refresh_requested
        || app.cancel_requested
        || app.navigation_restore.is_some()
        || app.navigation_restore_inflight
        || app.graph_reread_request_v3.is_some()
        || app.original_tree_work_v1.is_some()
        || app.original_tree_cancel_v1
        || app.original_diff_work_v1.is_some()
        || app.original_diff_cancel_v1
        || app.original_stash_recovery_request_v1.is_some()
        || app.original_stash_recovery_cancel_requested_v1
        || app.related_pr_requested_v1
        || app.related_pr_cancel_v1
        || app.scoped_actions_work_v1.is_some()
        || app.scoped_actions_cancel_v1
        || app.pr_native_diff_work_v1.is_some()
        || app.pr_native_diff_cancel_v1
        || app.scoped_inline_work_v1.is_some()
        || app.scoped_inline_cancel_v1
        || app.scoped_threads_work_v1.is_some()
        || app.scoped_threads_cancel_v1
        || app.scoped_reviews_work_v2.is_some()
        || app.scoped_reviews_cancel_v2
        || app.personal_pr_entry_work_v13.is_some()
        || app.personal_pr_entry_cancel_v13
        || app.repository_load_requested
        || app.repository_action.is_some()
        || app.github_action.is_some()
        || app.pr_checks_request
        || app.pr_check_choice_request.is_some()
        || app.pr_check_continuation_request_v2.is_some()
        || app.scoped_pr_tab_cancel_requested
        || app.local_actor_requested
        || app.local_actor_cancel_requested
        || app.search_load_requested
        || app.actions_load_requested
        || app.metrics_load_requested
}
pub(crate) fn open(app: &mut App) {
    match NotificationPresentationV63::capture(app) {
        Ok(original) => {
            app.overlay = Some(Overlay::NotificationsV63);
            app.notifications_v63 =
                Some(crate::notifications_ui_v63::NotificationBrowserV63::pending(original));
            request(app, NotificationReadPhaseV63::Load);
        }
        Err(message) => app.status = message.into(),
    }
}
pub(crate) fn watch(app: &mut App) {
    let Some(view) = app
        .notifications_v63
        .as_ref()
        .filter(|view| view.original.owns(app))
    else {
        return;
    };
    if view.watch.is_some() {
        let original = view.original.clone();
        if app.notifications_work_v63.as_ref().is_some_and(|work| {
            Arc::ptr_eq(work.original(), &original)
                && work.read_phase_v63() == NotificationReadPhaseV63::Refresh
        }) {
            if let Some(work) = app.notifications_work_v63.take() {
                work.cancellation().cancel();
            }
        }
        if app.notifications_owner_v63.as_ref().is_some_and(|work| {
            Arc::ptr_eq(work.original(), &original)
                && work.read_phase_v63() == NotificationReadPhaseV63::Refresh
        }) {
            app.notifications_cancel_v63 = true;
        }
        if let Some(view) = &mut app.notifications_v63 {
            if let Some(mut watch) = view.watch.take() {
                watch.stop();
            }
            view.message = "Finite notification watch stopped • no next poll queued";
            view.hits.clear();
        }
        return;
    }
    if app.notifications_work_v63.is_some()
        || app.notifications_owner_v63.is_some()
        || newer_intent(app)
    {
        return;
    }
    let Some(view) = app.notifications_v63.as_mut().filter(|view| !view.detail) else {
        return;
    };
    if let Some(prepared) = &view.prepared {
        match prepared.source.begin_watch_v63() {
            Ok(watch) => {
                app.status=format!("Read-only notification watch • at most {} further polls • server cooldown respected",watch.remaining());
                view.watch = Some(watch);
                view.message =
                    "Finite read-only watch • W Stop • server/configured cooldown applies";
            }
            Err(_) => view.message = "Watch unavailable for this observation; refresh explicitly",
        }
    }
}
pub(crate) fn tick(app: &mut App) {
    if newer_intent(app)
        || app.notifications_work_v63.is_some()
        || app.notifications_owner_v63.is_some()
        || app.loading.iter().any(|loading| *loading)
    {
        return;
    }
    if !app
        .notifications_v63
        .as_ref()
        .is_some_and(|view| view.original.owns(app))
    {
        return;
    }
    let due = app
        .notifications_v63
        .as_mut()
        .and_then(|view| view.watch.as_mut())
        .is_some_and(|watch| watch.claim_due(std::time::Instant::now()));
    if due {
        request(app, NotificationReadPhaseV63::Refresh);
    }
}
pub(crate) fn request(app: &mut App, phase: NotificationReadPhaseV63) {
    if app.notifications_work_v63.is_some()
        || app.notifications_owner_v63.is_some()
        || app.loading[View::Inbox.index()]
        || newer_intent(app)
    {
        return;
    }
    let Some(view) = app.notifications_v63.as_mut() else {
        return;
    };
    if phase == NotificationReadPhaseV63::Refresh
        && view
            .watch
            .as_ref()
            .is_some_and(|watch| !watch.in_flight_v63())
    {
        view.message = "Watch server cooldown still applies • W Stop before a manual refresh";
        return;
    }
    if phase != NotificationReadPhaseV63::Refresh {
        if let Some(mut watch) = view.watch.take() {
            watch.stop();
        }
    }
    let action_phase = matches!(
        phase,
        NotificationReadPhaseV63::PreviewMarkRead
            | NotificationReadPhaseV63::PreviewIgnore
            | NotificationReadPhaseV63::PreviewUnignore
    );
    let recovery = matches!(
        phase,
        NotificationReadPhaseV63::Inspect | NotificationReadPhaseV63::Acknowledge
    );
    let inspection = if phase == NotificationReadPhaseV63::Acknowledge {
        view.inspection
            .as_ref()
            .filter(|proof| {
                view.painted_inspection
                    .as_ref()
                    .is_some_and(|doc| Arc::ptr_eq(doc, proof.document()))
            })
            .cloned()
    } else {
        None
    };
    if phase == NotificationReadPhaseV63::Acknowledge
        && (inspection.is_none() || !view.acknowledge_rendered)
    {
        return;
    }
    let attempt = if let Some(inspection) = &inspection {
        Some(inspection.marker().clone())
    } else if recovery {
        view.prepared
            .as_ref()
            .and_then(|prepared| prepared.attempts.first())
            .cloned()
    } else {
        None
    };
    if recovery && attempt.is_none() {
        return;
    }
    let selected = if let Some(inspection) = &inspection {
        Some(inspection.selected())
    } else if matches!(
        phase,
        NotificationReadPhaseV63::Details
            | NotificationReadPhaseV63::History
            | NotificationReadPhaseV63::FollowPr
    ) || action_phase
        || recovery
    {
        view.prepared
            .as_ref()
            .and_then(|p| p.source.select_row_v63(view.selected))
    } else {
        None
    };
    if (matches!(
        phase,
        NotificationReadPhaseV63::Details
            | NotificationReadPhaseV63::History
            | NotificationReadPhaseV63::FollowPr
    ) || action_phase
        || recovery)
        && selected.is_none()
    {
        return;
    }
    let approval = if phase == NotificationReadPhaseV63::Publish {
        view.action_preview
            .as_ref()
            .filter(|preview| {
                view.painted_preview
                    .as_ref()
                    .is_some_and(|doc| Arc::ptr_eq(doc, preview.document()))
            })
            .cloned()
    } else {
        None
    };
    if phase == NotificationReadPhaseV63::Publish && (approval.is_none() || !view.confirm_rendered)
    {
        return;
    }
    let previous = if phase == NotificationReadPhaseV63::FollowPr {
        if let Some(mut watch) = view.watch.take() {
            watch.stop();
        }
        view.prepared
            .as_ref()
            .map(|prepared| prepared.source.clone())
    } else if let Some(inspection) = &inspection {
        Some(inspection.source().clone())
    } else if let Some(approval) = &approval {
        Some(approval.source().clone())
    } else if action_phase || recovery {
        let previous = view.prepared.take();
        let source = previous.as_ref().map(|p| p.source.clone());
        view.saved = previous;
        source
    } else {
        view.action_preview = None;
        view.inspection = None;
        view.saved = None;
        view.prepared.take().map(|p| p.source)
    };
    let Some(revision) = view.revision.checked_add(1) else {
        view.message = "Notification input epoch exhausted; close and reopen";
        return;
    };
    view.revision = revision;
    if phase == NotificationReadPhaseV63::FollowPr {
        view.follow_return_v63 = Some(crate::notifications_ui_v63::NotificationReturnV63 {
            detail: view.detail,
            scroll: view.scroll,
            viewport: std::mem::take(&mut view.viewport),
            message: view.message,
            focus: view.focus,
        });
    } else {
        view.follow_return_v63 = None;
    }
    view.hits.clear();
    view.painted_preview = None;
    view.painted_inspection = None;
    view.confirm_rendered = false;
    view.acknowledge_rendered = false;
    view.detail = false;
    view.scroll = 0;
    view.viewport = Default::default();
    view.message =
        "Verifying current account, notification query and repositories • Esc cancels exact read";
    let work = NotificationWorkV63::capture(
        view.original.clone(),
        phase,
        view.query.clone(),
        previous,
        NotificationSelectedInputV63 {
            selected,
            approval,
            attempt,
            inspection,
        },
        revision,
    );
    // The clock/token and exact immutable intent exist BEFORE Main queueing.
    app.notifications_work_v63 = Some(work);
}
pub(crate) fn close(app: &mut App) {
    let Some(original) = app
        .notifications_v63
        .as_ref()
        .map(|view| view.original.clone())
    else {
        return;
    };
    let closing_publish = original.owns(app)
        && app
            .notifications_owner_v63
            .as_ref()
            .or(app.notifications_work_v63.as_ref())
            .is_some_and(|work| {
                Arc::ptr_eq(work.original(), &original)
                    && work.read_phase_v63() == NotificationReadPhaseV63::Publish
            });
    if closing_publish {
        app.github = None;
        app.content[View::Inbox.index()].clear();
        app.context.clear();
        app.status =
            "Notification attempt closed • inspect protected history before a new action".into();
    }
    if app
        .notifications_work_v63
        .as_ref()
        .is_some_and(|work| Arc::ptr_eq(work.original(), &original))
    {
        if let Some(work) = app.notifications_work_v63.take() {
            work.cancellation().cancel();
        }
    }
    if app
        .notifications_owner_v63
        .as_ref()
        .is_some_and(|work| Arc::ptr_eq(work.original(), &original))
    {
        app.notifications_cancel_v63 = true;
    }
    app.notifications_v63 = None;
    if app.overlay == Some(Overlay::NotificationsV63) {
        app.overlay = None;
        app.hits.clear();
    }
}
/// No Background access: an obsolete owner cannot remove a replacement slot,
/// cancel its token, change loading/status, or erase a genuinely newer page.
pub(crate) fn retire_invalid(app: &mut App) -> bool {
    let Some(original) = app
        .notifications_v63
        .as_ref()
        .filter(|view| !view.original.owns(app))
        .map(|view| view.original.clone())
    else {
        return false;
    };
    let consumed = app.overlay == Some(Overlay::NotificationsV63);
    if original.same_presentation(app)
        && app
            .github
            .as_ref()
            .is_none_or(|github| github.scope().ok().as_ref() != Some(original.scope()))
    {
        // Exact old Inbox fallback/context must disappear on authentic numeric
        // account loss even if a different operation now owns the physical slot.
        app.github = None;
        app.content[View::Inbox.index()].clear();
        app.context.clear();
    }
    close(app);
    consumed
}
pub(crate) fn publish(
    app: &mut App,
    owner: &Arc<NotificationWorkV63>,
    prepared: PreparedNotificationResultV63,
) {
    if !owner.matches(app) || !prepared.matches(owner) {
        return;
    }
    let hide_private = prepared.hide_original_private();
    let outcome = prepared.take();
    if let Ok(PreparedNotificationWorkV63::Publication(publication)) = outcome {
        let state = publication.state();
        let attempt = publication.reference().map(|r| r.attempt_id());
        let message=match state {
            crate::providers::gh::NotificationPublicationStateV63::NotStarted=>"Notification write not started; inspect any protected claim before a new action",
            crate::providers::gh::NotificationPublicationStateV63::Unknown=>"Notification outcome uncertain; protected attempt retained • inspect, never auto resend",
            crate::providers::gh::NotificationPublicationStateV63::Confirmed=>"Notification state observed confirmed; old snapshot retired • refresh explicitly",
            crate::providers::gh::NotificationPublicationStateV63::ConfirmedSourceUnavailable=>"Notification state observed; current account/source unavailable • old snapshot retired",
        };
        if hide_private {
            app.github = None;
            app.content[View::Inbox.index()].clear();
            app.context.clear();
            close(app);
        } else if let Some(view) = &mut app.notifications_v63 {
            view.prepared = None;
            view.saved = None;
            view.action_preview = None;
            view.message = message;
            view.hits.clear();
        }
        if app
            .notifications_owner_v63
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, owner))
        {
            app.notifications_owner_v63 = None;
        }
        app.status = attempt.map_or_else(
            || message.into(),
            |id| format!("{message} • local attempt #{id}"),
        );
        return;
    }
    if hide_private {
        // Result belongs to this exact old presentation; do not relabel the
        // cached account or clear any independently newer page/slot/status.
        app.github = None;
        app.content[View::Inbox.index()].clear();
        app.context.clear();
        close(app);
        if app
            .notifications_owner_v63
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, owner))
        {
            app.notifications_owner_v63 = None;
        }
        app.status = "Private presentation retired • notification account/source changed".into();
        return;
    }
    if app
        .notifications_owner_v63
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, owner))
    {
        app.notifications_owner_v63 = None;
    }
    let outcome = match outcome {
        Ok(PreparedNotificationWorkV63::PrFollow(prepared)) => {
            if !crate::notification_pr_v63::install(app, *prepared) {
                if let Some(view) = &mut app.notifications_v63 {
                    view.message =
                        "Notification PR follow superseded • newer page/source preserved";
                }
            }
            return;
        }
        other => other,
    };
    let Some(view) = app.notifications_v63.as_mut() else {
        return;
    };
    match outcome {
        Ok(PreparedNotificationWorkV63::Read(prepared)) => {
            if let Some(watch) = view.watch.as_mut() {
                if prepared.source.complete_watch_v63(watch).is_err() || watch.remaining() == 0 {
                    watch.stop();
                    view.watch = None;
                }
            }
            view.detail = prepared.detail.is_some();
            view.selected = prepared
                .detail
                .as_ref()
                .and_then(|(id, _)| prepared.source.rows().iter().position(|row| row.id() == id))
                .unwrap_or_else(|| view.selected.min(prepared.rows.len().saturating_sub(1)));
            view.prepared = Some(prepared);
            view.scroll = 0;
            view.viewport = Default::default();
            view.message="Observed notifications • acting user is not supplied by GitHub • Enter inspects selected row";
            if view.watch.is_some() {
                view.message =
                    "Finite read-only watch • W Stop • server/configured cooldown applies";
            }
            app.status = "Notifications observed • no Git/PR/run changes".into();
        }
        Ok(PreparedNotificationWorkV63::Preview(preview)) => {
            view.prepared = None;
            view.action_preview = Some(preview);
            view.painted_preview = None;
            view.detail = true;
            view.scroll = 0;
            view.viewport = Default::default();
            view.message="Preview only • C Confirm rechecks fresh source/state • Back cancels without effects";
            app.status = "Notification Preview • no write started".into();
        }
        Ok(PreparedNotificationWorkV63::Inspection(inspection)) => {
            view.prepared = None;
            view.action_preview = None;
            view.inspection = Some(inspection);
            view.painted_inspection = None;
            view.detail = true;
            view.scroll = 0;
            view.viewport = Default::default();
            view.message =
                "Fresh Inspect only • L explicit LOCAL ACK rechecks again • never resend";
            app.status =
                "Current notification state observed • no remote request or local ACK performed"
                    .into();
        }
        Ok(PreparedNotificationWorkV63::Acknowledged(completed)) => {
            view.inspection = None;
            view.painted_inspection = None;
            view.action_preview = None;
            view.saved = None;
            view.prepared = None;
            view.detail = false;
            view.message = if completed {
                "Exact local notification barrier acknowledged • history retained • refresh explicitly"
            } else {
                "Local acknowledgement incomplete • protected record retained • inspect again; never resend"
            };
            app.status = view.message.into();
        }
        Ok(PreparedNotificationWorkV63::Publication(_)) => {
            unreachable!("handled closed publication above")
        }
        Ok(PreparedNotificationWorkV63::PrFollow(_)) => {
            unreachable!("handled exact PR follow above")
        }
        Err(message) => {
            view.prepared = None;
            view.saved = None;
            view.action_preview = None;
            if let Some(mut watch) = view.watch.take() {
                watch.stop();
            }
            view.message = message;
            app.status = message.into();
        }
    }
    view.hits.clear();
}
pub(crate) fn failed(app: &mut App, owner: &Arc<NotificationWorkV63>) {
    if !owner.matches(app) {
        return;
    }
    if app
        .notifications_owner_v63
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, owner))
    {
        app.notifications_owner_v63 = None;
    }
    if owner.read_phase_v63() == NotificationReadPhaseV63::Publish {
        app.github = None;
        app.content[View::Inbox.index()].clear();
        app.context.clear();
        close(app);
        app.status =
            "Notification worker failed • outcome unconfirmed; inspect protected history".into();
        return;
    }
    if let Some(view) = app.notifications_v63.as_mut() {
        view.prepared = None;
        if let Some(mut watch) = view.watch.take() {
            watch.stop();
        }
        view.saved = None;
        view.action_preview = None;
        view.inspection = None;
        view.painted_preview = None;
        view.painted_inspection = None;
        view.hits.clear();
        view.message = "Notification worker failed; private rows hidden • refresh explicitly";
    }
    app.status =
        "Notification worker failed • inspect protected history before a new action".into();
}
