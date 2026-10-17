//! Read-only historical attempts from a FRESH Summary source. A durable marker
//! is not permission, reconciliation, a restored draft approval or a retry.
use super::*;
use crate::{
    pr_write_journal::{
        thread_history_v1::ThreadHistorySnapshotV1, PrWriteJournal, PrWriteState, PrWriteTarget,
    },
    providers::gh::{HostIdentity, ThreadHistoryParentObservationV1},
};
#[path = "scoped_pr_thread_comment_history_v2.rs"]
pub mod comments_v2;
#[path = "scoped_pr_file_comment_history_v75.rs"]
pub mod files_v75;
#[path = "scoped_pr_history_phase_v75.rs"]
pub(crate) mod phase_v75;
#[path = "scoped_pr_thread_reaction_history_v3.rs"]
pub mod reactions_v3;
#[path = "scoped_pr_thread_history_ui_v1.rs"]
pub mod ui;

/// Genuine closed presentation alternatives. Personal Summary does not mint a
/// Files/source/navigation capability merely to inspect protected local rows.
pub(crate) enum HistoryOriginV75 {
    Read {
        source: crate::pr_read_origin_v13::PrReadOriginV13,
        active: Arc<ActiveRemoteScopeV1>,
        navigation: Arc<ScopedPrNavigationV1>,
        expected: Box<ThreadHistoryParentObservationV1>,
    },
    PersonalSummary(
        Arc<crate::personal_pr_entry_v13::summary_v14::PersonalPrSummaryPresentationV14>,
    ),
}

pub struct ThreadHistoryIntentV1 {
    pub(crate) origin: HistoryOriginV75,
    selected: usize,
    scroll: usize,
}
impl std::fmt::Debug for ThreadHistoryIntentV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ThreadHistoryIntentV1(<actual Summary navigation; no permission>)")
    }
}
impl ThreadHistoryIntentV1 {
    pub fn capture(app: &App) -> Result<Arc<Self>, String> {
        let github = app
            .github
            .as_ref()
            .ok_or("Open a fresh scoped PR Summary first")?;
        let page = github
            .pr
            .as_ref()
            .filter(|page| page.tab == crate::github_workspace::PrTab::Summary)
            .ok_or("Thread history opens from genuine scoped Summary, not blocked Files")?;
        let intent = Arc::new(Self {
            origin: HistoryOriginV75::Read {
                source: crate::pr_read_origin_v13::PrReadOriginV13::capture(app)?,
                active: app
                    .remote_scope
                    .as_ref()
                    .ok_or("Original active scope unavailable")?
                    .clone(),
                navigation: github
                    .scoped_pr_navigation
                    .as_ref()
                    .ok_or("Original Summary navigation unavailable")?
                    .clone(),
                expected: Box::new(
                    ThreadHistoryParentObservationV1::capture(&page.details.summary)
                        .map_err(|_| "Bounded actual Summary observation unavailable")?,
                ),
            },
            selected: app.selected,
            scroll: app.scroll,
        });
        if app.overlay.is_some() || !intent.owns(app) {
            return Err("Exact original Summary presentation required".into());
        }
        Ok(intent)
    }
    pub fn scope(&self) -> &Scope {
        match &self.origin {
            HistoryOriginV75::Read { source, .. } => source.scope(),
            HistoryOriginV75::PersonalSummary(original) => original.scope(),
        }
    }
    pub(crate) fn capture_personal(app: &App, database: &Path) -> Result<Arc<Self>, String> {
        let original =
            crate::personal_pr_entry_v13::summary_v14::PersonalPrSummaryPresentationV14::capture(
                app, database,
            )?;
        if !original.matches_capture(app) {
            return Err("Exact idle original Personal Summary required".into());
        }
        Ok(Arc::new(Self {
            origin: HistoryOriginV75::PersonalSummary(original),
            selected: app.selected,
            scroll: app.scroll,
        }))
    }
    pub(crate) fn personal_summary(
        &self,
    ) -> Option<&Arc<crate::personal_pr_entry_v13::summary_v14::PersonalPrSummaryPresentationV14>>
    {
        match &self.origin {
            HistoryOriginV75::PersonalSummary(original) => Some(original),
            _ => None,
        }
    }
    pub(crate) fn read_origin(
        &self,
    ) -> Option<(
        &crate::pr_read_origin_v13::PrReadOriginV13,
        &Arc<ActiveRemoteScopeV1>,
        &Arc<ScopedPrNavigationV1>,
    )> {
        match &self.origin {
            HistoryOriginV75::Read {
                source,
                active,
                navigation,
                ..
            } => Some((source, active, navigation)),
            _ => None,
        }
    }
    pub(crate) fn retire_account_mismatch(&self, app: &mut App) -> bool {
        match &self.origin {
            HistoryOriginV75::Read {
                source,
                active,
                navigation,
                ..
            } => retire_original_account_v75(app, source, active, navigation),
            HistoryOriginV75::PersonalSummary(original) => original.retire_account_mismatch(app),
        }
    }
    pub fn owns(&self, app: &App) -> bool {
        app.view == View::Inbox
            && app.selected == self.selected
            && app.scroll == self.scroll
            && match &self.origin {
                HistoryOriginV75::PersonalSummary(original) => original.owns_presentation(app),
                HistoryOriginV75::Read {
                    source,
                    active,
                    navigation,
                    expected,
                } => {
                    source.matches_source(app)
                        && app
                            .remote_scope
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(current, active))
                        && app.github.as_ref().is_some_and(|github| {
                            github.scope().ok().as_ref() == Some(self.scope())
                                && github
                                    .scoped_pr_navigation
                                    .as_ref()
                                    .is_some_and(|nav| Arc::ptr_eq(nav, navigation))
                                && github.pr.as_ref().is_some_and(|page| {
                                    page.tab == crate::github_workspace::PrTab::Summary
                                        && expected.matches(&page.details.summary)
                                        && matches_page(navigation, page)
                                })
                        })
                }
            }
    }
}
pub struct PreparedThreadHistoryV1 {
    pub intent: Arc<ThreadHistoryIntentV1>,
    pub identity: HostIdentity,
    pub header: Vec<String>,
    pub labels: Vec<String>,
    pub roles: Vec<SemanticToken>,
    pub actor_observation: crate::providers::gh::Actor,
    pub actor: ActorIdentity,
    snapshot: ThreadHistorySnapshotV1,
    pub opened: Option<usize>,
    pub detail: Vec<String>,
}
fn source_error(_: impl std::fmt::Debug) -> ThreadBrowseFailureV1 {
    ThreadBrowseFailureV1::Source
}
fn escape(value: &str) -> String {
    crate::git::display_bytes(value.as_bytes())
}

/// Opening always repeats the fresh source/account/repository/raw-PR bracket.
/// Original historical H/base are shown separately and NEVER used as current
/// expected head. Same current local generation/marker snapshot must persist
/// through this ONE read; an old catalogue generation need not still be current.
#[allow(clippy::too_many_arguments)]
pub fn load_history(
    intent: Arc<ThreadHistoryIntentV1>,
    previous: Option<(&Arc<PreparedThreadHistoryV1>, usize)>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Arc<PreparedThreadHistoryV1>, ThreadBrowseFailureV1> {
    original_check(cancel, deadline).map_err(source_error)?;
    if previous.is_some_and(|(old, index)| {
        !Arc::ptr_eq(&old.intent, &intent) || index >= old.snapshot.rows.len()
    }) {
        return Err(ThreadBrowseFailureV1::Source);
    }
    let provider = provider.clone().with_catalogue_read_deadline(deadline);
    let phase =
        phase_v75::HistoryReadPhaseV75::begin(&intent, &provider, database, cancel, deadline)?;
    let identity = phase.identity().clone();
    let parent = phase.parent().clone();
    // No ResourceLocalGuard capture/weakening. This read deliberately INSPECTS
    // protected history; current original Source/UID/actual IDs come first.
    original_check(cancel, deadline).map_err(source_error)?;
    let journal = PrWriteJournal::open(database).map_err(source_error)?;
    let target = PrWriteTarget::new(&identity, parent.summary()).map_err(source_error)?;
    if let Some((old, _)) = previous {
        journal
            .validate_thread_history_v1(
                &target,
                parent.repository_id(),
                parent.repository_node(),
                parent.database_id(),
                &old.snapshot,
            )
            .map_err(source_error)?;
    }
    let snapshot = journal
        .thread_history_v1(
            &target,
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
        )
        .map_err(source_error)?;
    let mut detail = vec![];
    let mut body_witness = None;
    let opened = previous.map(|(_, index)| index);
    if let Some(index) = opened {
        let row = snapshot
            .rows
            .get(index)
            .ok_or(ThreadBrowseFailureV1::Source)?;
        let marker = &row.marker;
        detail.push(format!(
            "Recorded attempt #{} • {:?} • {}",
            marker.attempt_id, marker.state, marker.command
        ));
        detail.push(format!(
            "Local journal created at UNIX {} seconds (not a server event timestamp)",
            marker.created_at
        ));
        detail.push(format!(
            "Historical H {} (NOT current H)",
            marker.observed_head
        ));
        detail.push(format!(
            "Durable payload fingerprint {}",
            marker.payload_sha256
        ));
        detail.push("Inspection grants no restore, approval, retry, discard or acknowledgement. Current thread existence and placement are unknown.".into());
        if let Some(origin) = &row.origin {
            detail.push(format!(
                "Observed original numeric account {} • login {}",
                origin.seed.scope.login,
                origin
                    .seed
                    .observed_login
                    .as_deref()
                    .unwrap_or("unavailable")
            ));
            detail.push(format!(
                "Original repository #{} {} • PR #{} {}",
                origin.seed.repository_id,
                origin.seed.repository_node,
                origin.seed.pr_id,
                origin.seed.pr_node
            ));
            detail.push(format!(
                "Historical base {} • opaque thread {}",
                origin.seed.base, origin.seed.thread_node
            ));
            if let Some(root) = &origin.seed.root {
                detail.push(format!(
                    "Actual historical root REST #{} {} • observed commit {} • path {}",
                    root.rest_id, root.node, root.commit, root.path
                ));
            }
            if let Some(effect) = &marker.confirmed {
                detail.push(format!(
                    "Recorded observed receipt {:?} {} • node {} (not proof of causality)",
                    effect.kind,
                    effect.resource_id,
                    effect.node_id.as_deref().unwrap_or("unavailable")
                ));
            }
            original_check(cancel, deadline).map_err(source_error)?;
            let encoded = Cache::open(database)
                .map_err(source_error)?
                .bounded_draft(intent.scope(), &origin.seed.draft_key, 128 * 1024)
                .map_err(source_error)?;
            if let Some(encoded) = &encoded {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Values {
                    body: String,
                }
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Envelope {
                    version: u8,
                    values: Values,
                    publication: Option<crate::pr_write_journal::PrPublicationReference>,
                }
                let value: serde_json::Value =
                    serde_json::from_str(encoded).map_err(source_error)?;
                let reference = value
                    .get("publication")
                    .and_then(serde_json::Value::as_object)
                    .ok_or(ThreadBrowseFailureV1::Source)?;
                if reference.len() != 4
                    || ![
                        "attempt_id",
                        "pr_node_id",
                        "observed_head",
                        "payload_sha256",
                    ]
                    .iter()
                    .all(|field| reference.contains_key(*field))
                {
                    return Err(ThreadBrowseFailureV1::Source);
                }
                let draft: Envelope = serde_json::from_value(value).map_err(source_error)?;
                if draft.version != 2
                    || draft.publication.as_ref() != Some(&origin.reference)
                    || draft.values.body.len() > 65_536
                    || origin
                        .seed
                        .write(&draft.values.body)
                        .and_then(|write| write.digest())
                        .map_err(source_error)?
                        != marker.payload_sha256
                {
                    return Err(ThreadBrowseFailureV1::Source);
                }
                detail.push("Exact protected LOCAL body follows; this is not a new server read or approval:".into());
                detail.extend(draft.values.body.split('\n').map(str::to_owned));
            } else {
                detail.push(if marker.state==PrWriteState::Confirmed{"Local body unavailable (may have been removed after confirmation); recorded receipt remains inspectable"}else{"Local body unavailable; ACK/persistence outcome unknown, not an empty comment or resend grant"}.into());
            }
            body_witness = Some((origin.seed.draft_key.clone(), encoded));
        } else {
            detail.push("Protected legacy attempt: exact original thread metadata unavailable. Body is NOT opened; no target guessed, migrated or discarded.".into());
        }
    }
    let labels: Vec<String> = snapshot
        .rows
        .iter()
        .map(|row| {
            format!(
                "#{} {:?} {} • {}",
                row.marker.attempt_id,
                row.marker.state,
                row.marker.command,
                if row.origin.is_some() {
                    "exact origin recorded"
                } else {
                    "protected legacy • target unavailable"
                }
            )
        })
        .collect();
    let roles = snapshot
        .rows
        .iter()
        .map(|row| match row.marker.state {
            PrWriteState::Pending => SemanticToken::Pending,
            PrWriteState::Uncertain => SemanticToken::Conflict,
            PrWriteState::Confirmed => SemanticToken::Success,
            PrWriteState::Acknowledged => SemanticToken::Muted,
        })
        .collect();
    let login = opened
        .and_then(|index| snapshot.rows[index].origin.as_ref())
        .and_then(|record| record.seed.observed_login.as_deref())
        .or(if opened.is_none() {
            Some(identity.login.as_str())
        } else {
            None
        });
    if login.is_some_and(|login| login.len() > 4096) {
        return Err(ThreadBrowseFailureV1::Display);
    }
    let mut actor_observation = crate::providers::gh::Actor {
        login: login.unwrap_or_default().to_owned(),
        actor_type: "User".into(),
        ..Default::default()
    };
    let mut actor = ActorIdentity::github(&identity.host, &actor_observation);
    let header=vec![
        format!("Fresh current {} PR #{} • repo #{} / PR #{}",parent.repository(),parent.number(),parent.repository_id(),parent.database_id()),
        format!("Current H {} • current base {}",parent.head_sha(),parent.base_tip_sha()),
        format!("Latest {} local attempts • {} • current numeric account {:?}",snapshot.rows.len(),snapshot.total.map_or_else(||"further rows not enumerated; total unknown".into(),|total|format!("bounded enumeration complete: {total}")),identity.account_id),
        "Read-only local history; observed targets are historical, not current thread locations or write permissions".into(),
    ];
    let header: Vec<_> = header.into_iter().map(|line| escape(&line)).collect();
    let labels: Vec<_> = labels.into_iter().map(|line| escape(&line)).collect();
    let detail: Vec<_> = detail.into_iter().map(|line| escape(&line)).collect();
    if header
        .iter()
        .chain(&labels)
        .chain(&detail)
        .map(String::len)
        .sum::<usize>()
        > 1024 * 1024
        || detail.len() > 65_536
    {
        return Err(ThreadBrowseFailureV1::Display);
    }
    // Re-read same current raw parent/repository, then private SQL inside the
    // original close and finally the positive numeric principal. NO later IO.
    let identity = phase.finish(|| {
        original_check(cancel, deadline)?;
        let cache = Cache::open(database).map_err(|_| safe_local())?;
        let current = PrWriteJournal::open(database).map_err(|_| safe_local())?;
        current
            .validate_thread_history_v1(
                &target,
                parent.repository_id(),
                parent.repository_node(),
                parent.database_id(),
                &snapshot,
            )
            .map_err(|_| safe_local())?;
        if let Some((key, expected)) = &body_witness {
            if cache
                .bounded_draft(intent.scope(), key, 128 * 1024)
                .map_err(|_| safe_local())?
                .as_ref()
                != expected.as_ref()
            {
                return Err(safe_local());
            }
        }
        Ok(())
    })?;
    if opened.is_none() {
        if identity.login.len() > 4096 {
            return Err(ThreadBrowseFailureV1::Display);
        }
        actor_observation.login = identity.login.clone();
        actor = ActorIdentity::github(&identity.host, &actor_observation);
    }
    Ok(Arc::new(PreparedThreadHistoryV1 {
        intent,
        identity,
        header,
        labels,
        roles,
        actor_observation,
        actor,
        snapshot,
        opened,
        detail,
    }))
}
