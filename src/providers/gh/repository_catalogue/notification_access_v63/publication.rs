//! One purpose-closed notification publisher. Preview is observation only;
//! explicit Confirm creates a fresh queue-inclusive phase on the SAME Core,
//! measured allocation and original fixed ordinary capture cursor.
use super::*;
use crate::{
    prepared_form_text_v1::PreparedFormTextV1,
    storage::{
        NotificationRequestedV63, NotificationWritePermitV63, NotificationWriteReferenceV63,
    },
};
use serde::Serialize;

const THREAD_CAP: usize = 128 * 1024;
#[path = "publication/inspection.rs"]
mod inspection;
pub(crate) use inspection::NotificationInspectionV63;
#[derive(Clone, Copy, PartialEq, Eq)]
enum SelectedModeV63 {
    Preview,
    Publish,
    Inspect,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct SubscriptionV63 {
    ignored: bool,
    subscribed: bool,
}
fn subscription(
    provider: &GithubProvider,
    thread: &str,
    value: &Value,
) -> GithubResult<SubscriptionV63> {
    if !value.is_object() || value.get("errors").is_some() || value.get("message").is_some() {
        return Err(refused());
    }
    for (key, path) in [
        (
            "url",
            format!("notifications/threads/{thread}/subscription"),
        ),
        ("thread_url", format!("notifications/threads/{thread}")),
    ] {
        if !value.get(key).and_then(Value::as_str).is_some_and(|url| {
            crate::providers::gh::search::canonical_source_url_matches(
                &provider.host,
                url,
                &path,
                true,
                false,
            )
        }) {
            return Err(refused());
        }
    }
    if value.get("reason").is_some_and(|v| {
        !v.is_null()
            && v.as_str()
                .is_none_or(|s| s.len() > 128 || s.chars().any(char::is_control))
    }) {
        return Err(refused());
    }
    if value.get("created_at").is_some_and(|v| {
        !v.is_null()
            && v.as_str()
                .is_none_or(|s| !notifications_v63::parse::timestamp(s))
    }) {
        return Err(refused());
    }
    Ok(SubscriptionV63 {
        ignored: value
            .get("ignored")
            .and_then(Value::as_bool)
            .ok_or_else(refused)?,
        subscribed: value
            .get("subscribed")
            .and_then(Value::as_bool)
            .ok_or_else(refused)?,
    })
}
fn get_thread(
    provider: &GithubProvider,
    id: &str,
    token: &CancellationToken,
) -> GithubResult<NotificationRowV63> {
    let value = provider.api(
        &format!("notifications/threads/{id}"),
        "GET",
        None,
        None,
        false,
        token,
    )?;
    let row = notifications_v63::parse::row(&provider.host, &value)?;
    if row.id() != id {
        return Err(refused());
    }
    Ok(row)
}
fn get_subscription(
    provider: &GithubProvider,
    id: &str,
    token: &CancellationToken,
) -> GithubResult<SubscriptionV63> {
    let value = provider.api(
        &format!("notifications/threads/{id}/subscription"),
        "GET",
        None,
        None,
        false,
        token,
    )?;
    subscription(provider, id, &value)
}

/// Genuine bounded observation. It is not permission or a live attempt clock.
pub(crate) struct NotificationActionPreviewV63 {
    source: Arc<ClosedNotificationReadV63>,
    index: usize,
    requested: NotificationRequestedV63,
    doc: Arc<PreparedFormTextV1>,
}
impl std::fmt::Debug for NotificationActionPreviewV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationActionPreviewV63(<observed target; no approval>)")
    }
}
impl NotificationActionPreviewV63 {
    pub(crate) fn document(&self) -> &Arc<PreparedFormTextV1> {
        &self.doc
    }
    pub(crate) fn requested(&self) -> NotificationRequestedV63 {
        self.requested
    }
    pub(crate) fn source(&self) -> &Arc<ClosedNotificationReadV63> {
        &self.source
    }
    pub(crate) fn selected(&self) -> NotificationSelectionV63 {
        NotificationSelectionV63 {
            source: self.source.clone(),
            index: self.index,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct NotificationWriteReceiptV63 {
    version: u8,
    host: String,
    account_id: u64,
    thread: String,
    repository_id: u64,
    repository_node: String,
    subject_kind: String,
    subject_key: String,
    requested: String,
    observed_unread: Option<bool>,
    observed_ignored: Option<bool>,
}
impl std::fmt::Debug for NotificationWriteReceiptV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationWriteReceiptV63(<actual typed HTTP/readback observation>)")
    }
}
impl NotificationWriteReceiptV63 {
    pub(crate) fn matches_origin(
        &self,
        origin: &notifications_v63::NotificationWriteOriginV63,
        command: &str,
    ) -> bool {
        self.version == 1
            && self.host == origin.scope().host
            && Some(self.account_id)
                == origin
                    .scope()
                    .login
                    .strip_prefix("user-id:")
                    .and_then(|id| id.parse().ok())
            && self.thread == origin.thread()
            && self.repository_id == origin.repository_id()
            && self.repository_node == origin.repository_node()
            && self.subject_kind == origin.subject_kind()
            && self.subject_key == origin.subject_key()
            && self.requested == command
            && match NotificationRequestedV63::parse(command) {
                Some(NotificationRequestedV63::MarkRead) => {
                    self.observed_unread == Some(false) && self.observed_ignored.is_none()
                }
                Some(NotificationRequestedV63::Ignore) => {
                    self.observed_ignored == Some(true) && self.observed_unread.is_none()
                }
                Some(NotificationRequestedV63::Unignore) => {
                    self.observed_ignored == Some(false) && self.observed_unread.is_none()
                }
                None => false,
            }
    }
    pub(crate) fn encoded(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotificationPublicationStateV63 {
    NotStarted,
    Unknown,
    Confirmed,
    ConfirmedSourceUnavailable,
}
pub(crate) struct NotificationPublicationV63 {
    state: NotificationPublicationStateV63,
    reference: Option<NotificationWriteReferenceV63>,
    hide_private: bool,
}
impl NotificationPublicationV63 {
    pub(crate) fn state(&self) -> NotificationPublicationStateV63 {
        self.state
    }
    pub(crate) fn reference(&self) -> Option<&NotificationWriteReferenceV63> {
        self.reference.as_ref()
    }
    #[cfg(test)]
    pub(crate) fn invoked(&self) -> bool {
        self.state != NotificationPublicationStateV63::NotStarted
    }
    pub(crate) fn hide_private(&self) -> bool {
        self.hide_private
    }
}

struct SelectedPhaseV63<'a> {
    core: OriginalRepositoryAccessCoreV13<'a>,
    reference: crate::repository_catalogue_intent_v1::IntentReference,
    measured: GithubProvider,
    captures: Option<PostWriteCaptureAllowanceV2>,
    source: Arc<ClosedNotificationReadV63>,
    index: usize,
    row: NotificationRowV63,
    origin: notifications_v63::NotificationWriteOriginV63,
    subscription: Option<SubscriptionV63>,
    requested: NotificationRequestedV63,
    token: CancellationToken,
    deadline: Instant,
    generation: Option<u64>,
}
impl GithubProvider {
    fn begin_notification_selected_v63<'a>(
        &'a self,
        selected: &NotificationSelectionV63,
        requested: NotificationRequestedV63,
        cache: &Cache,
        token: &CancellationToken,
        deadline: Instant,
        mode: SelectedModeV63,
    ) -> GithubResult<SelectedPhaseV63<'a>> {
        live(token, deadline)?;
        if self.scoped_read_budget.is_some()
            || self
                .scoped_read_window
                .is_none_or(|(bound, cap)| bound > deadline || cap > PAGE)
            || deadline.saturating_duration_since(Instant::now()) > Duration::from_secs(30)
            || !selected.source.original.matches_issuer(self)
        {
            return Err(refused());
        }
        let deadline = deadline.min(self.scoped_read_window.expect("required original window").0);
        live(token, deadline)?;
        let old = selected
            .source
            .rows
            .get(selected.index)
            .ok_or_else(refused)?;
        let mut limits = access_limits();
        limits.max_references = 1;
        limits.max_repository_reads = 2;
        let mut budget = CatalogueBudget::new(&limits.transport)?;
        budget.original_cancellation = Some(token.clone());
        let identity = principal(self, &mut budget, token).map_err(safe_failure)?;
        same_principal(selected.source.identity(), &identity)?;
        selected.source.validate_local(cache, token, deadline)?;
        let original = NotificationRepositoryOriginalV63::from_observed(
            self,
            &identity,
            selected.source.query(),
            std::slice::from_ref(old),
            true,
            METADATA,
        )?;
        let mut core = OriginalRepositoryAccessCoreV13::from_started_notifications_v63(
            self,
            original.clone(),
            identity,
            &limits,
            budget,
            token,
        )?;
        let reference = original.reference(0).ok_or_else(refused)?;
        if core
            .verify(&reference, token)?
            .repository()
            .permissions
            .pull
            != Some(true)
        {
            return Err(error(
                GithubErrorKind::Permission,
                "Current original notification repository read permission unavailable",
            ));
        }
        let captures = if mode == SelectedModeV63::Publish {
            // Fixed5: exact write HTTP, exact readback, opening UID, repo,
            // final UID. Same cumulative cursor; no GET/version extra pool.
            Some(core.reserve_postwrite_capture_v2(
                &[
                    if requested == NotificationRequestedV63::MarkRead {
                        16 * 1024
                    } else {
                        THREAD_CAP
                    },
                    THREAD_CAP,
                    64 * 1024,
                    REPO,
                    64 * 1024,
                ],
                1,
                token,
            )?)
        } else {
            None
        };
        let reads = if requested == NotificationRequestedV63::MarkRead {
            2
        } else {
            4
        };
        let window =
            core.reserve_measured_external_reads(reads, THREAD_CAP, reads * THREAD_CAP, 1, token)?;
        let measured = self.clone().with_measured_read_window(window)?;
        let initial_origin = notifications_v63::NotificationWriteOriginV63::from_actual(
            &self.runner,
            core.identity(),
            old,
            &selected.source.witness,
            token,
            deadline,
        )?;
        let generation = if mode == SelectedModeV63::Inspect {
            Some(
                self.runner
                    .notification_inspection_generation_v63(&initial_origin, token, deadline)
                    .map_err(|_| refused())?,
            )
        } else {
            None
        };
        let row = get_thread(&measured, old.id(), token)?;
        if !old.same_original_target_v63(&row) || old.updated_at() != row.updated_at() {
            return Err(refused());
        }
        let observed = if requested == NotificationRequestedV63::MarkRead {
            None
        } else {
            Some(get_subscription(&measured, row.id(), token)?)
        };
        if mode != SelectedModeV63::Inspect
            && (requested == NotificationRequestedV63::MarkRead && !row.unread()
                || observed
                    .is_some_and(|s| s.ignored == (requested == NotificationRequestedV63::Ignore)))
        {
            return Err(error(
                GithubErrorKind::InvalidTarget,
                "Requested notification state is already observed; no write started",
            ));
        }
        let origin = notifications_v63::NotificationWriteOriginV63::from_actual(
            &self.runner,
            core.identity(),
            &row,
            &selected.source.witness,
            token,
            deadline,
        )?;
        live(token, deadline)?;
        Ok(SelectedPhaseV63 {
            core,
            reference,
            measured,
            captures,
            source: selected.source.clone(),
            index: selected.index,
            row,
            origin,
            subscription: observed,
            requested,
            token: token.clone(),
            deadline,
            generation,
        })
    }
    pub(crate) fn preview_notification_action_v63(
        &self,
        selected: &NotificationSelectionV63,
        requested: NotificationRequestedV63,
        cache: &Cache,
        token: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<NotificationActionPreviewV63>> {
        let phase = self.begin_notification_selected_v63(
            selected,
            requested,
            cache,
            token,
            deadline,
            SelectedModeV63::Preview,
        )?;
        let deadline = phase.deadline;
        let effect = match requested {
            NotificationRequestedV63::MarkRead => "Mark this exact notification read",
            NotificationRequestedV63::Ignore => {
                "Ignore this exact notification thread (not the PR subscription)"
            }
            NotificationRequestedV63::Unignore => {
                "Stop ignoring this exact notification thread (does not claim PR subscription)"
            }
        };
        let lines = vec![
            effect.into(),
            format!("Repository: {}", phase.row.repository_label()),
            format!(
                "Notification #{} • {}",
                phase.row.id(),
                phase.row.subject_label()
            ),
            format!("Observed updated: {}", phase.row.updated_at()),
            "Current acting account: numeric account verified through installed gh".into(),
            "Explicit Confirm starts a new original phase; this Preview grants no permission."
                .into(),
            "Readback is observed state, not proof this action caused it. No automatic resend."
                .into(),
        ];
        let doc = PreparedFormTextV1::prepare(&lines, token, deadline).map_err(|_| refused())?;
        if doc.retained_bytes() > 1024 * 1024 || doc.preparation_peak_bound() > 2 * 1024 * 1024 {
            return Err(refused());
        }
        let source = phase.source.clone();
        let index = phase.index;
        phase.close(cache, None, None)?;
        Ok(Arc::new(NotificationActionPreviewV63 {
            source,
            index,
            requested,
            doc,
        }))
    }
    pub(crate) fn publish_notification_action_v63(
        &self,
        preview: &Arc<NotificationActionPreviewV63>,
        cache: &mut Cache,
        token: &CancellationToken,
        deadline: Instant,
    ) -> NotificationPublicationV63 {
        let mut reference = None;
        let mut invoked = false;
        let mut confirmed = false;
        let result = (|| {
            let selected = preview.selected();
            let phase = self.begin_notification_selected_v63(
                &selected,
                preview.requested,
                cache,
                token,
                deadline,
                SelectedModeV63::Publish,
            )?;
            let deadline = phase.deadline;
            let permit = cache
                .begin_notification_write_v63(
                    &phase.origin,
                    &phase.source.witness,
                    phase.requested,
                    token,
                    deadline,
                )
                .map_err(|_| refused())?;
            reference = Some(permit.marker().reference().clone());
            let origin = notifications_v63::NotificationWriteOriginV63::from_actual(
                &self.runner,
                phase.core.identity(),
                &phase.row,
                &phase.source.witness,
                token,
                deadline,
            )?;
            let original_row = phase.row.clone();
            let source = phase.source.clone();
            let requested = phase.requested;
            let mut captures = phase
                .close(cache, Some(&permit), None)?
                .captures
                .ok_or_else(refused)?;
            permit.claim_once().map_err(|_| refused())?;
            live(token, deadline)?;
            // Invocation boundary is conservative; ANY failure from here is
            // protected Unknown, never a source of automatic resend authority.
            invoked = true;
            let post = (|| {
                let ack = captures.notification_write_ack_v63(self, &origin, requested, token)?;
                let (observed_unread, observed_ignored) =
                    if requested == NotificationRequestedV63::MarkRead {
                        let value = captures.api(
                            self,
                            &format!("notifications/threads/{}", origin.thread()),
                            "GET",
                            None,
                            None,
                            token,
                        )?;
                        let row = notifications_v63::parse::row(&self.host, &value)?;
                        if !original_row.same_original_target_v63(&row) || row.unread() {
                            return Err(refused());
                        }
                        (Some(false), None)
                    } else {
                        let acknowledged =
                            subscription(self, origin.thread(), &ack.ok_or_else(refused)?)?;
                        let value = captures.api(
                            self,
                            &format!("notifications/threads/{}/subscription", origin.thread()),
                            "GET",
                            None,
                            None,
                            token,
                        )?;
                        let observed = subscription(self, origin.thread(), &value)?;
                        if acknowledged != observed
                            || observed.ignored != (requested == NotificationRequestedV63::Ignore)
                        {
                            return Err(refused());
                        }
                        (None, Some(observed.ignored))
                    };
                let opening = captures.api(self, "user", "GET", None, None, token)?;
                if opening["id"].as_u64() != source.identity().account_id {
                    return Err(refused());
                }
                let repo = captures.api(
                    self,
                    &format!("repos/{}", origin.repository_name()),
                    "GET",
                    None,
                    None,
                    token,
                )?;
                let actual = notifications_v63::parse::repository(&self.host, &repo)?;
                if &actual != original_row.repository()
                    || repo["permissions"]["pull"].as_bool() != Some(true)
                {
                    return Err(refused());
                }
                let receipt = NotificationWriteReceiptV63 {
                    version: 1,
                    host: self.host.clone(),
                    account_id: source.identity().account_id.ok_or_else(refused)?,
                    thread: origin.thread().into(),
                    repository_id: origin.repository_id(),
                    repository_node: origin.repository_node().into(),
                    subject_kind: origin.subject_kind().into(),
                    subject_key: origin.subject_key().into(),
                    requested: requested.command().into(),
                    observed_unread,
                    observed_ignored,
                };
                // Local exact original SQL precedes final numeric UID. The
                // receipt itself invalidates every old notification witness;
                // ordinary reads discover it even if the UI crashes now.
                cache
                    .confirm_notification_write_v63(
                        &permit,
                        &origin,
                        &source.witness,
                        &receipt,
                        token,
                        deadline,
                    )
                    .map_err(|_| refused())?;
                confirmed = true;
                let closing = captures.api(self, "user", "GET", None, None, token)?;
                if closing["id"].as_u64() != source.identity().account_id {
                    return Err(refused());
                }
                live(token, deadline)?;
                if captures.consumed_requests() != 5 {
                    return Err(refused());
                }
                Ok(())
            })();
            if post.is_err() && !confirmed {
                let _ = cache.uncertain_notification_write_v63(&permit, token, deadline);
            }
            post
        })();
        let state = if confirmed {
            if result.is_ok() {
                NotificationPublicationStateV63::Confirmed
            } else {
                NotificationPublicationStateV63::ConfirmedSourceUnavailable
            }
        } else if invoked {
            NotificationPublicationStateV63::Unknown
        } else {
            NotificationPublicationStateV63::NotStarted
        };
        let hide_private = invoked
            || result.as_ref().err().is_some_and(|error| {
                matches!(
                    error.kind,
                    GithubErrorKind::IdentityChanged
                        | GithubErrorKind::Authentication
                        | GithubErrorKind::Permission
                        | GithubErrorKind::NotFound
                )
            });
        NotificationPublicationV63 {
            state,
            reference,
            hide_private,
        }
    }
}
impl SelectedPhaseV63<'_> {
    fn close(
        mut self,
        cache: &Cache,
        permit: Option<&NotificationWritePermitV63>,
        marker: Option<&crate::storage::NotificationWriteMarkerV63>,
    ) -> GithubResult<ClosedSelectedPhaseV63> {
        live(&self.token, self.deadline)?;
        let closing = get_thread(&self.measured, self.row.id(), &self.token)?;
        if !self.row.same_original_target_v63(&closing)
            || closing.updated_at() != self.row.updated_at()
            || closing.unread() != self.row.unread()
        {
            return Err(refused());
        }
        if let Some(original) = self.subscription {
            if get_subscription(&self.measured, self.row.id(), &self.token)? != original {
                return Err(refused());
            }
        }
        self.core.verify(&self.reference, &self.token)?;
        if let Some(generation) = self.generation {
            if self
                .measured
                .runner
                .notification_inspection_generation_v63(&self.origin, &self.token, self.deadline)
                .map_err(|_| refused())?
                != generation
            {
                return Err(refused());
            }
        }
        if let Some(captures) = &self.captures {
            captures.validate_prewrite(&self.measured, &self.token)?;
        }
        let closed = self.core.close_with_guard(&self.token, || {
            if let Some(permit) = permit {
                cache
                    .validate_notification_permit_v63(
                        permit,
                        &self.origin,
                        &self.source.witness,
                        &self.token,
                        self.deadline,
                    )
                    .map_err(|_| refused())
            } else if let Some(marker) = marker {
                cache
                    .validate_notification_marker_v63(
                        marker,
                        &self.origin,
                        &self.source.witness,
                        &self.token,
                        self.deadline,
                    )
                    .map_err(|_| refused())
            } else {
                self.source
                    .validate_local(cache, &self.token, self.deadline)
            }
        })?;
        live(&self.token, self.deadline)?;
        Ok(ClosedSelectedPhaseV63 {
            captures: self.captures,
            identity: closed.identity,
        })
    }
}
struct ClosedSelectedPhaseV63 {
    captures: Option<PostWriteCaptureAllowanceV2>,
    identity: HostIdentity,
}
