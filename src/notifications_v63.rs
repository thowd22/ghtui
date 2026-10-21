//! Notification presentation and requests. Observation is never PR/run source
//! authority, and every explicit phase captures its clock BEFORE queueing.
use crate::{
    app::{App, Overlay, View},
    github_workspace::PrKey,
    prepared_form_text_v1::PreparedFormTextV1,
    providers::{
        gh::{
            notifications_v63::NotificationQueryV63, ClosedNotificationReadV63, GithubProvider,
            HostIdentity,
        },
        runner::{sanitize_terminal, CancellationToken},
    },
    storage::{Cache, CatalogueIntentSource, Scope},
};
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

/// Display ownership only. Constructors are restricted to the actual Inbox
/// action; neither this ticket nor its cached principal authorizes transport.
pub struct NotificationPresentationV63 {
    identity: HostIdentity,
    scope: Scope,
    generation: u64,
    command_epoch: u64,
    selected: Option<PrKey>,
    source: Option<Arc<CatalogueIntentSource>>,
    active: Option<Arc<crate::active_remote_scope_v1::ActiveRemoteScopeV1>>,
}
fn same_optional_arc<T>(a: Option<&Arc<T>>, b: Option<&Arc<T>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(a), Some(b)) => Arc::ptr_eq(a, b),
        _ => false,
    }
}
impl NotificationPresentationV63 {
    pub(crate) fn capture(app: &App) -> Result<Arc<Self>, &'static str> {
        if app.view != View::Inbox
            || app.overlay.is_some()
            || app.form.is_some()
            || app.loading.iter().any(|loading| *loading)
            || app.notifications_owner_v63.is_some()
            || app.notifications_cancel_v63
            || app.personal_pr_source_v13.is_some()
            || app.notification_pr_source_v63.is_some()
            || app.command_context_exhausted
            || crate::scoped_pr_collaboration_native_v17::retired_presentation(app)
            || crate::notifications_native_v63::newer_intent(app)
        {
            return Err("Open Notifications from an idle Inbox; finish the current action first");
        }
        let github = app
            .github
            .as_ref()
            .filter(|github| {
                github.pr.is_none()
                    && github.scoped_pr_navigation.is_none()
                    && github.scoped_pr_resource.is_none()
            })
            .ok_or("Return to the Inbox before opening Notifications")?;
        if github.identity.login.len() > 4096 {
            return Err("Current account label exceeds its display bound");
        }
        let scope = github
            .scope()
            .map_err(|_| "Refresh Inbox to observe a current numeric account first")?;
        if github
            .identity
            .account_id
            .is_none_or(|id| id == 0 || id > i64::MAX as u64)
        {
            return Err("Refresh Inbox to observe a current numeric account first");
        }
        Ok(Arc::new(Self {
            identity: github.identity.clone(),
            scope,
            generation: github.generation,
            command_epoch: app.command_source_epoch,
            selected: github.selected.clone(),
            source: app.remote_scope_source.clone(),
            active: app.remote_scope.clone(),
        }))
    }
    /// Exact original page mechanics, deliberately excluding account equality
    /// so authentic account loss can hide ONLY this old private presentation.
    pub(crate) fn same_presentation(&self, app: &App) -> bool {
        app.view == View::Inbox
            && app.command_source_epoch == self.command_epoch
            && !app.command_context_exhausted
            && app.personal_pr_source_v13.is_none()
            && app.notification_pr_source_v63.is_none()
            && same_optional_arc(app.remote_scope.as_ref(), self.active.as_ref())
            && same_optional_arc(app.remote_scope_source.as_ref(), self.source.as_ref())
            && app.github.as_ref().is_some_and(|github| {
                github.generation == self.generation
                    && github.selected == self.selected
                    && github.pr.is_none()
                    && github.scoped_pr_navigation.is_none()
                    && github.scoped_pr_resource.is_none()
            })
    }
    pub(crate) fn owns(&self, app: &App) -> bool {
        self.same_presentation(app)
            && app.form.is_none()
            && app.overlay == Some(Overlay::NotificationsV63)
            && app
                .github
                .as_ref()
                .is_some_and(|github| github.scope().ok().as_ref() == Some(&self.scope))
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.scope
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationReadPhaseV63 {
    Load,
    Refresh,
    Details,
    History,
    PreviewMarkRead,
    PreviewIgnore,
    PreviewUnignore,
    Publish,
    Inspect,
    Acknowledge,
    FollowPr,
}

pub struct NotificationWorkV63 {
    #[cfg(test)]
    actual_worker_fault_v63: std::sync::atomic::AtomicU8,
    original: Arc<NotificationPresentationV63>,
    phase: NotificationReadPhaseV63,
    query: NotificationQueryV63,
    previous: Option<Arc<ClosedNotificationReadV63>>,
    selected: Option<crate::providers::gh::NotificationSelectionV63>,
    approval: Option<Arc<crate::providers::gh::NotificationActionPreviewV63>>,
    attempt: Option<Arc<crate::storage::NotificationWriteMarkerV63>>,
    inspection: Option<Arc<crate::providers::gh::NotificationInspectionV63>>,
    revision: u64,
    cancellation: CancellationToken,
    deadline: Instant,
}
#[derive(Default)]
pub(crate) struct NotificationSelectedInputV63 {
    pub(crate) selected: Option<crate::providers::gh::NotificationSelectionV63>,
    pub(crate) approval: Option<Arc<crate::providers::gh::NotificationActionPreviewV63>>,
    pub(crate) attempt: Option<Arc<crate::storage::NotificationWriteMarkerV63>>,
    pub(crate) inspection: Option<Arc<crate::providers::gh::NotificationInspectionV63>>,
}
impl NotificationWorkV63 {
    pub(crate) fn capture(
        original: Arc<NotificationPresentationV63>,
        phase: NotificationReadPhaseV63,
        query: NotificationQueryV63,
        previous: Option<Arc<ClosedNotificationReadV63>>,
        selected: NotificationSelectedInputV63,
        revision: u64,
    ) -> Arc<Self> {
        Arc::new(Self {
            #[cfg(test)]
            actual_worker_fault_v63: std::sync::atomic::AtomicU8::new(0),
            original,
            phase,
            query,
            previous,
            selected: selected.selected,
            approval: selected.approval,
            attempt: selected.attempt,
            inspection: selected.inspection,
            revision,
            cancellation: CancellationToken::default(),
            deadline: Instant::now() + Duration::from_secs(30),
        })
    }
    pub fn host(&self) -> &str {
        &self.original.scope.host
    }
    #[cfg(test)]
    pub(crate) fn actual_worker_fault_v63(&self) -> u8 {
        self.actual_worker_fault_v63
            .load(std::sync::atomic::Ordering::Acquire)
    }
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }
    pub(crate) fn original(&self) -> &Arc<NotificationPresentationV63> {
        &self.original
    }
    pub(crate) fn read_phase_v63(&self) -> NotificationReadPhaseV63 {
        self.phase
    }
    pub(crate) fn expired(&self) -> bool {
        self.cancellation.is_cancelled() || Instant::now() >= self.deadline
    }
    pub(crate) fn matches(&self, app: &App) -> bool {
        self.original.owns(app)
            && app.notifications_v63.as_ref().is_some_and(|view| {
                Arc::ptr_eq(&view.original, &self.original)
                    && view.revision == self.revision
                    && view.query == self.query
                    && (self.phase != NotificationReadPhaseV63::FollowPr
                        || view.prepared.as_ref().is_some_and(|prepared| {
                            self.previous
                                .as_ref()
                                .is_some_and(|source| Arc::ptr_eq(source, &prepared.source))
                                && self.selected.as_ref().is_some_and(|selected| {
                                    prepared
                                        .source
                                        .rows()
                                        .get(view.selected)
                                        .is_some_and(|row| row.id() == selected.id())
                                })
                        }))
                    && self.approval.as_ref().is_none_or(|approval| {
                        view.action_preview
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(approval, current))
                    })
                    && self.inspection.as_ref().is_none_or(|inspection| {
                        view.inspection
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(inspection, current))
                    })
            })
    }
}

pub struct PreparedNotificationRowV63 {
    pub(crate) label: Arc<str>,
    pub(crate) unread: bool,
}
pub struct PreparedNotificationsV63 {
    pub(crate) source: Arc<ClosedNotificationReadV63>,
    pub(crate) rows: Vec<PreparedNotificationRowV63>,
    pub(crate) detail: Option<(String, Arc<PreparedFormTextV1>)>,
    pub(crate) coverage: Arc<str>,
    pub(crate) attempts: Vec<Arc<crate::storage::NotificationWriteMarkerV63>>,
}
pub struct PreparedNotificationResultV63 {
    original: Arc<NotificationWorkV63>,
    result: Result<PreparedNotificationWorkV63, &'static str>,
    hide_original_private: bool,
}
pub(crate) enum PreparedNotificationWorkV63 {
    Read(PreparedNotificationsV63),
    Preview(Arc<crate::providers::gh::NotificationActionPreviewV63>),
    Publication(crate::providers::gh::NotificationPublicationV63),
    Inspection(Arc<crate::providers::gh::NotificationInspectionV63>),
    Acknowledged(bool),
    PrFollow(Box<crate::notification_pr_v63::PreparedNotificationPrV63>),
}
impl PreparedNotificationResultV63 {
    pub(crate) fn hide_original_private(&self) -> bool {
        self.hide_original_private
    }
    pub(crate) fn matches(&self, work: &Arc<NotificationWorkV63>) -> bool {
        if !Arc::ptr_eq(&self.original, work) {
            return false;
        }
        match &self.result {
            Err(_) => true,
            Ok(PreparedNotificationWorkV63::Read(prepared)) => match work.phase {
                NotificationReadPhaseV63::Load | NotificationReadPhaseV63::Refresh => {
                    prepared.detail.is_none() && prepared.source.query() == &work.query
                }
                NotificationReadPhaseV63::Details | NotificationReadPhaseV63::History => {
                    prepared.source.query() == &work.query
                        && prepared.detail.as_ref().is_some_and(|(id, _)| {
                            work.selected
                                .as_ref()
                                .is_some_and(|selected| selected.id() == id)
                        })
                }
                _ => false,
            },
            Ok(PreparedNotificationWorkV63::Preview(preview)) => {
                let expected = match work.phase {
                    NotificationReadPhaseV63::PreviewMarkRead => {
                        Some(crate::storage::NotificationRequestedV63::MarkRead)
                    }
                    NotificationReadPhaseV63::PreviewIgnore => {
                        Some(crate::storage::NotificationRequestedV63::Ignore)
                    }
                    NotificationReadPhaseV63::PreviewUnignore => {
                        Some(crate::storage::NotificationRequestedV63::Unignore)
                    }
                    _ => None,
                };
                expected == Some(preview.requested())
                    && work
                        .previous
                        .as_ref()
                        .is_some_and(|source| Arc::ptr_eq(source, preview.source()))
            }
            Ok(PreparedNotificationWorkV63::Publication(_)) => {
                work.phase == NotificationReadPhaseV63::Publish
            }
            Ok(PreparedNotificationWorkV63::Inspection(inspection)) => {
                work.phase == NotificationReadPhaseV63::Inspect
                    && work
                        .previous
                        .as_ref()
                        .is_some_and(|source| Arc::ptr_eq(source, inspection.source()))
                    && work
                        .attempt
                        .as_ref()
                        .is_some_and(|attempt| Arc::ptr_eq(attempt, inspection.marker()))
            }
            Ok(PreparedNotificationWorkV63::Acknowledged(_)) => {
                work.phase == NotificationReadPhaseV63::Acknowledge
            }
            Ok(PreparedNotificationWorkV63::PrFollow(prepared)) => {
                work.phase == NotificationReadPhaseV63::FollowPr
                    && work
                        .previous
                        .as_ref()
                        .is_some_and(|source| prepared.belongs_to(source))
            }
        }
    }
    pub(crate) fn take(self) -> Result<PreparedNotificationWorkV63, &'static str> {
        self.result
    }
}
/// A readable label for GitHub's notification `reason`. Unknown reasons keep
/// their observed (sanitized) text so nothing is silently re-labelled.
pub(crate) fn reason_label_v63(reason: &str) -> String {
    match reason {
        "approval_requested" => "Approval requested".into(),
        "assign" => "Assigned".into(),
        "author" => "Author".into(),
        "ci_activity" => "CI activity".into(),
        "comment" => "Comment".into(),
        "invitation" => "Invitation".into(),
        "manual" => "Subscribed manually".into(),
        "member_feature_requested" => "Feature requested".into(),
        "mention" => "Mentioned".into(),
        "review_requested" => "Review requested".into(),
        "security_advisory_credit" => "Advisory credit".into(),
        "security_alert" => "Security alert".into(),
        "state_change" => "State changed".into(),
        "subscribed" => "Watching".into(),
        "team_mention" => "Team mentioned".into(),
        other => sanitize_terminal(other).chars().take(40).collect(),
    }
}
/// One list row: state, freshness, reason and resource (`PR #7`) first, so
/// they survive an 80-column cut, then the title and repository. Freshness is relative to `now` (seconds) captured once per read;
/// the exact UTC time stays in Details.
pub(crate) fn row_label_v63(
    unread: bool,
    updated_at: &str,
    now: u64,
    reason: &str,
    title: &str,
    repository: &str,
    subject: &str,
) -> String {
    let age = crate::ui_table::age_at(updated_at, now).unwrap_or_else(|| "—".into());
    format!(
        "{:<6} {:>4} {} • {} • {} • {}",
        if unread { "Unread" } else { "Read" },
        age,
        reason_label_v63(reason),
        subject,
        title,
        sanitize_terminal(repository),
    )
}
/// The Details actor line: the shared initials chip and `@login` first, then
/// what the login actually is. An unknown actor says why, never guesses.
pub(crate) fn latest_actor_line_v63(
    host: &str,
    observed: &crate::providers::gh::NotificationLatestActorV63,
    fallback: &str,
) -> String {
    use crate::providers::gh::NotificationLatestActorV63 as Latest;
    match observed {
        Latest::Observed { actor, is_subject } => {
            let identity = crate::identity::ActorIdentity::github(host, actor);
            format!(
                "Latest activity: [{}] {} • {}",
                identity.initials(),
                identity.label(),
                if *is_subject {
                    "subject author (no newer comment)"
                } else {
                    "latest comment"
                }
            )
        }
        Latest::Unavailable(reason) => format!("Latest activity: {fallback} • {reason}"),
    }
}
fn alive(work: &NotificationWorkV63) -> Result<(), &'static str> {
    if work.expired() {
        Err("Original notification read canceled or expired; refresh explicitly")
    } else {
        Ok(())
    }
}
pub(crate) fn execute(
    work: Arc<NotificationWorkV63>,
    provider: &GithubProvider,
    database: &Path,
) -> PreparedNotificationResultV63 {
    let mut hide_original_private = false;
    let result = (|| {
        alive(&work)?;
        if work.selected.as_ref().is_some_and(|selected| {
            work.previous
                .as_ref()
                .is_none_or(|source| !selected.belongs_to(source))
        }) {
            return Err("Original selected notification allocation changed before any child");
        }
        let provider = provider.with_notification_read_window_v63(work.deadline);
        let mut cache = Cache::open(database)
            .map_err(|_| "Notification cache unavailable; private rows hidden")?;
        if work.phase == NotificationReadPhaseV63::FollowPr {
            let selected = work
                .selected
                .as_ref()
                .ok_or("Select a current actual notification PR target")?;
            let prepared=crate::notification_pr_v63::read(&provider,selected,&cache,database,&work.cancellation,work.deadline)
                .map_err(|error| {
                    hide_original_private=matches!(error.kind, crate::providers::gh::GithubErrorKind::IdentityChanged|crate::providers::gh::GithubErrorKind::Authentication|crate::providers::gh::GithubErrorKind::Permission|crate::providers::gh::GithubErrorKind::NotFound);
                    "Original notification PR/source could not be verified; no viewer or mutation admitted"
                })?;
            alive(&work)?;
            return Ok(PreparedNotificationWorkV63::PrFollow(Box::new(prepared)));
        }
        if matches!(
            work.phase,
            NotificationReadPhaseV63::Inspect | NotificationReadPhaseV63::Acknowledge
        ) {
            let selected = work
                .selected
                .as_ref()
                .ok_or("Select actual original notification history before Inspect")?;
            let marker = work
                .attempt
                .as_ref()
                .ok_or("Exact protected original notification marker unavailable")?;
            let inspection = provider
                .inspect_notification_attempt_v63(
                    selected,
                    marker.clone(),
                    &cache,
                    &work.cancellation,
                    work.deadline,
                )
                .map_err(|error| {
                    hide_original_private = matches!(
                        error.kind,
                        crate::providers::gh::GithubErrorKind::IdentityChanged
                            | crate::providers::gh::GithubErrorKind::Authentication
                            | crate::providers::gh::GithubErrorKind::Permission
                            | crate::providers::gh::GithubErrorKind::NotFound
                    );
                    "Fresh original notification inspection refused; protected history retained"
                })?;
            if work.phase == NotificationReadPhaseV63::Acknowledge {
                let completed = cache
                    .acknowledge_notification_inspection_v63(
                        &provider,
                        &inspection,
                        &work.cancellation,
                        work.deadline,
                    )
                    .is_ok();
                return Ok(PreparedNotificationWorkV63::Acknowledged(completed));
            }
            return Ok(PreparedNotificationWorkV63::Inspection(inspection));
        }
        if work.phase == NotificationReadPhaseV63::Publish {
            let approval = work
                .approval
                .as_ref()
                .filter(|preview| {
                    work.previous
                        .as_ref()
                        .is_some_and(|source| Arc::ptr_eq(source, preview.source()))
                })
                .ok_or("Exact original Preview changed; no publication started")?;
            let result = provider.publish_notification_action_v63(
                approval,
                &mut cache,
                &work.cancellation,
                work.deadline,
            );
            hide_original_private = result.hide_private();
            return Ok(PreparedNotificationWorkV63::Publication(result));
        }
        let requested = match work.phase {
            NotificationReadPhaseV63::PreviewMarkRead => {
                Some(crate::storage::NotificationRequestedV63::MarkRead)
            }
            NotificationReadPhaseV63::PreviewIgnore => {
                Some(crate::storage::NotificationRequestedV63::Ignore)
            }
            NotificationReadPhaseV63::PreviewUnignore => {
                Some(crate::storage::NotificationRequestedV63::Unignore)
            }
            _ => None,
        };
        if let Some(requested) = requested {
            let selected = work
                .selected
                .as_ref()
                .ok_or("Select an actual observed notification before Preview")?;
            let preview = provider
                .preview_notification_action_v63(
                    selected,
                    requested,
                    &cache,
                    &work.cancellation,
                    work.deadline,
                )
                .map_err(|error| {
                    hide_original_private = matches!(
                        error.kind,
                        crate::providers::gh::GithubErrorKind::IdentityChanged
                            | crate::providers::gh::GithubErrorKind::Authentication
                            | crate::providers::gh::GithubErrorKind::Permission
                            | crate::providers::gh::GithubErrorKind::NotFound
                    );
                    "Current original notification/state could not be verified; no write started"
                })?;
            return Ok(PreparedNotificationWorkV63::Preview(preview));
        }
        let access=provider.begin_notification_read_v63(&work.query,work.original.identity(),&mut cache,
            work.previous.as_ref(),&work.cancellation,work.deadline)
            .map_err(|error|{
                hide_original_private=matches!(error.kind,crate::providers::gh::GithubErrorKind::IdentityChanged|crate::providers::gh::GithubErrorKind::Authentication|crate::providers::gh::GithubErrorKind::Permission|crate::providers::gh::GithubErrorKind::NotFound);
                "Current notification account/repository/source could not be verified; private rows hidden"
            })?;
        let mut rows = Vec::with_capacity(access.rows().len());
        let mut retained = 0usize;
        let now = crate::storage::now();
        for row in access.rows() {
            alive(&work)?;
            // This is bounded display preparation in the original worker, not
            // draw-time formatting or a claim about an absent acting user.
            let title = sanitize_terminal(row.title())
                .chars()
                .take(240)
                .collect::<String>();
            let label = row_label_v63(
                row.unread(),
                row.updated_at(),
                now,
                row.reason(),
                &title,
                row.repository_label(),
                &row.subject_short_v63(),
            );
            retained = retained
                .checked_add(
                    label.capacity()
                        + row.id().len()
                        + std::mem::size_of::<PreparedNotificationRowV63>(),
                )
                .ok_or("Notification display bound exceeded; private rows hidden")?;
            if retained > 512 * 1024 {
                return Err("Notification display bound exceeded; private rows hidden");
            }
            rows.push(PreparedNotificationRowV63 {
                label: Arc::from(label),
                unread: row.unread(),
            });
        }
        let mut history_attempts = Vec::new();
        let detail = if let Some(selected) = &work.selected {
            let row = access
                .rows()
                .iter()
                .find(|row| row.id() == selected.id())
                .ok_or(
                    "Selected notification is no longer in this actual query; nothing inferred",
                )?;
            // History stays a pure journal view: only Details reads the latest
            // commenter, so History adds no child to its original phase.
            let actor = if work.phase == NotificationReadPhaseV63::Details {
                let index = access
                    .rows()
                    .iter()
                    .position(|row| row.id() == selected.id())
                    .ok_or("Original selected notification disappeared; details hidden")?;
                let observed = access.latest_actor_v63(&provider, index);
                alive(&work)?;
                latest_actor_line_v63(&work.original.scope.host, &observed, row.actor_label())
            } else {
                format!(
                    "Latest activity: open Details to read it • {}",
                    row.actor_label()
                )
            };
            let mut lines = vec![
                format!(
                    "Notification #{} • observed {}",
                    row.id(),
                    if row.unread() { "Unread" } else { "Read" }
                ),
                actor,
                format!("Repository: {}", row.repository_label()),
                format!("Subject: {}", row.subject_label()),
                format!("Title: {}", row.title()),
                format!(
                    "Reason: {} ({})",
                    reason_label_v63(row.reason()),
                    sanitize_terminal(row.reason())
                ),
                format!(
                    "Updated: {} • {} ago",
                    row.updated_at(),
                    crate::ui_table::age_at(row.updated_at(), now)
                        .unwrap_or_else(|| "age unknown".into())
                ),
                format!("Last read: {}", row.last_read_at().unwrap_or("Unavailable")),
                format!(
                    "Signed-in observer: @{} • numeric account {}",
                    sanitize_terminal(&access.identity().login),
                    access
                        .identity()
                        .account_id
                        .ok_or("Actual numeric observer unavailable")?
                ),
                "GitHub notifications name no acting user; Latest activity is the latest comment's author.".into(),
                "Observed notification metadata grants no PR/run access or publication permission."
                    .into(),
            ];
            if work.phase == NotificationReadPhaseV63::History {
                let index = access
                    .rows()
                    .iter()
                    .position(|row| row.id() == selected.id())
                    .ok_or("Original selected notification disappeared; history hidden")?;
                let origin = access
                    .write_origin_v63(index)
                    .map_err(|_| "Original notification history target could not be produced")?;
                let attempts = cache
                    .notification_attempts_v63(
                        &origin,
                        access.local_witness_v63(),
                        &work.cancellation,
                        work.deadline,
                    )
                    .map_err(|_| {
                        "Original notification journal/account/database changed; history hidden"
                    })?;
                lines.push("Publication history • inspect only • never restore/retry/acknowledge from this view".into());
                if attempts.is_empty() {
                    lines.push(
                        "No protected attempt recorded for this exact observed notification."
                            .into(),
                    );
                }
                for marker in attempts {
                    let state = match marker.state() {
                        crate::storage::NotificationWriteStateV63::Pending => {
                            "Pending • outcome not yet established"
                        }
                        crate::storage::NotificationWriteStateV63::Uncertain => {
                            "Uncertain • inspect fresh server state; never auto resend"
                        }
                        crate::storage::NotificationWriteStateV63::Confirmed => {
                            "Confirmed observation • not proof of causality"
                        }
                        crate::storage::NotificationWriteStateV63::Acknowledged => {
                            "Explicitly acknowledged historical observation"
                        }
                    };
                    lines.push(format!(
                        "Attempt #{} • {} • {}",
                        marker.reference().attempt_id(),
                        marker.command(),
                        state
                    ));
                    history_attempts.push(Arc::new(marker));
                }
            }
            let doc = PreparedFormTextV1::prepare(&lines, &work.cancellation, work.deadline)
                .map_err(|_| {
                    "Notification detail cannot fit its bounded display; nothing published"
                })?;
            if doc.retained_bytes() > 1024 * 1024 || doc.preparation_peak_bound() > 2 * 1024 * 1024
            {
                return Err("Notification detail exceeds its retained display bound");
            }
            Some((selected.id().into(), doc))
        } else {
            None
        };
        let coverage = access.coverage();
        let label = format!(
            "{} | {} | {} observed | {} | {} raw, {} duplicates, {} unverified repo",
            if work.query.include_read {
                "Including read"
            } else {
                "Unread only"
            },
            if work.query.participating {
                "Participating only"
            } else {
                "All reasons"
            },
            rows.len(),
            if coverage.complete {
                "Complete query"
            } else {
                "Partial query; absence unknown"
            },
            coverage.raw_rows,
            coverage.duplicates,
            coverage.unverified_repository_rows
        );
        let coverage: Arc<str> = Arc::from(label);
        // ALL bounded private preparation precedes closing repository/SQL and
        // final current numeric principal. After close: pure O(1) moves only.
        let source = access.finish(&cache, &work.cancellation).map_err(|error| {
            hide_original_private = matches!(
                error.kind,
                crate::providers::gh::GithubErrorKind::IdentityChanged
                    | crate::providers::gh::GithubErrorKind::Authentication
                    | crate::providers::gh::GithubErrorKind::Permission
                    | crate::providers::gh::GithubErrorKind::NotFound
            );
            "Notification source changed before completion; private rows hidden"
        })?;
        alive(&work)?;
        Ok(PreparedNotificationWorkV63::Read(
            PreparedNotificationsV63 {
                source,
                rows,
                detail,
                coverage,
                attempts: history_attempts,
            },
        ))
    })();
    PreparedNotificationResultV63 {
        original: work,
        result,
        hide_original_private,
    }
}
#[cfg(test)]
#[path = "notifications_v63_tests.rs"]
pub(crate) mod tests;
