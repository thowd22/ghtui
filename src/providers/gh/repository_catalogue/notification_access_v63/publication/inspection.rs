//! Fresh server observation and explicit LOCAL acknowledgement. Historical
//! rows, body text and a restored reference never become publication authority.
use super::*;
use crate::storage::{
    NotificationSourceWitnessV63, NotificationWriteMarkerV63, NotificationWriteStateV63,
};
pub(crate) struct NotificationInspectionV63 {
    issuer: GithubProvider,
    source: Arc<ClosedNotificationReadV63>,
    index: usize,
    marker: Arc<NotificationWriteMarkerV63>,
    origin: notifications_v63::NotificationWriteOriginV63,
    generation: u64,
    identity: HostIdentity,
    token: CancellationToken,
    deadline: Instant,
    document: Arc<PreparedFormTextV1>,
}
impl std::fmt::Debug for NotificationInspectionV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationInspectionV63(<actual fresh observation; no resend authority>)")
    }
}
impl NotificationInspectionV63 {
    pub(crate) fn deadline_v63(&self) -> Instant {
        self.deadline
    }
    pub(crate) fn document(&self) -> &Arc<PreparedFormTextV1> {
        &self.document
    }
    pub(crate) fn marker(&self) -> &Arc<NotificationWriteMarkerV63> {
        &self.marker
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
    fn live(&self, token: &CancellationToken, deadline: Instant) -> bool {
        self.token.same_instance(token)
            && deadline <= self.deadline
            && !token.is_cancelled()
            && Instant::now() < deadline
            && self.origin.phase_matches(token, deadline)
    }
    pub(crate) fn runner_gate_v63<'a>(
        &'a self,
        runner: &crate::providers::runner::CommandRunner,
        token: &CancellationToken,
        deadline: Instant,
    ) -> Option<(&'a notifications_v63::NotificationWriteOriginV63, u64)> {
        (self.live(token, deadline)
            && self.issuer.runner.same_instance(runner)
            && self.origin.matches_runner_v63(runner))
        .then_some((&self.origin, self.generation))
    }
    pub(crate) fn local_gate_v63<'a>(
        &'a self,
        provider: &GithubProvider,
        token: &CancellationToken,
        deadline: Instant,
    ) -> Option<(
        &'a notifications_v63::NotificationWriteOriginV63,
        &'a NotificationWriteMarkerV63,
        &'a NotificationSourceWitnessV63,
    )> {
        (self.live(token, deadline)
            && self.issuer.runner.same_instance(&provider.runner)
            && self.issuer.host == provider.host
            && self.issuer.program == provider.program
            && self.issuer.env == provider.env
            && self.identity.account_id == self.source.identity().account_id
            && self.identity.host == self.source.identity().host)
            .then_some((&self.origin, &self.marker, &self.source.witness))
    }
}
impl GithubProvider {
    pub(crate) fn inspect_notification_attempt_v63(
        &self,
        selected: &NotificationSelectionV63,
        marker: Arc<NotificationWriteMarkerV63>,
        cache: &Cache,
        token: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<Arc<NotificationInspectionV63>> {
        let requested = NotificationRequestedV63::parse(marker.command()).ok_or_else(refused)?;
        let phase = self.begin_notification_selected_v63(
            selected,
            requested,
            cache,
            token,
            deadline,
            SelectedModeV63::Inspect,
        )?;
        let deadline = phase.deadline;
        cache
            .validate_notification_marker_v63(
                &marker,
                &phase.origin,
                &phase.source.witness,
                token,
                deadline,
            )
            .map_err(|_| refused())?;
        let state = match marker.state() {
            NotificationWriteStateV63::Pending => {
                "Pending; actual write outcome was not established"
            }
            NotificationWriteStateV63::Uncertain => {
                "Uncertain; actual write outcome was not established"
            }
            NotificationWriteStateV63::Confirmed => "Confirmed typed observation already recorded",
            NotificationWriteStateV63::Acknowledged => {
                "Historical local acknowledgement already recorded"
            }
        };
        let mut lines=vec![format!("Inspect local attempt #{} • {}",marker.reference().attempt_id(),requested.command()),state.into(),format!("Notification #{} • {}",phase.row.id(),phase.row.repository_label()),format!("Current observed state: {}",if phase.row.unread(){"Unread"}else{"Read"}),format!("Current numeric acting account: {}",phase.core.identity().account_id.ok_or_else(refused)?),"This is observed state, not proof the earlier attempt caused it.".into(),"Explicit LOCAL acknowledgement only releases this exact unresolved local barrier. It never resends, restores a draft, deletes history or claims a remote effect.".into()];
        if let Some(subscription) = phase.subscription {
            lines.push(format!(
                "Current thread ignored: {} • subscribed: {} (not PR subscription)",
                if subscription.ignored { "Yes" } else { "No" },
                if subscription.subscribed { "Yes" } else { "No" }
            ));
        }
        let document =
            PreparedFormTextV1::prepare(&lines, token, deadline).map_err(|_| refused())?;
        if document.retained_bytes() > 1024 * 1024
            || document.preparation_peak_bound() > 2 * 1024 * 1024
        {
            return Err(refused());
        }
        let origin = notifications_v63::NotificationWriteOriginV63::from_actual(
            &self.runner,
            phase.core.identity(),
            &phase.row,
            &phase.source.witness,
            token,
            deadline,
        )?;
        let source = phase.source.clone();
        let index = phase.index;
        let generation = phase.generation.ok_or_else(refused)?;
        let closed = phase.close(cache, None, Some(&marker))?;
        live(token, deadline)?;
        Ok(Arc::new(NotificationInspectionV63 {
            issuer: self.clone(),
            source,
            index,
            marker,
            origin,
            generation,
            identity: closed.identity,
            token: token.clone(),
            deadline,
            document,
        }))
    }
    pub(crate) fn reconcile_notification_inspection_v63(
        &self,
        inspection: &NotificationInspectionV63,
        token: &CancellationToken,
        deadline: Instant,
    ) -> GithubResult<()> {
        inspection
            .local_gate_v63(self, token, deadline)
            .ok_or_else(refused)?;
        self.runner
            .reconcile_notification_inspection_v63(inspection, token, deadline)
            .map_err(|_| refused())
    }
}
