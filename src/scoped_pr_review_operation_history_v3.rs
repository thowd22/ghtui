//! Protected operation history from a GENUINE fresh Summary. No Files source,
//! current deleted-comment authority, draft approval, reconciliation or resend.
use super::*;
use crate::{
    active_remote_scope_v1::RemoteScopeConsumerV1,
    app::{App, View},
    pr_write_journal::{
        review_operation_history_v3::ReviewOperationHistorySnapshotV3, PrWriteState, PrWriteTarget,
    },
    providers::gh::{Actor, ThreadHistoryParentObservationV1},
};
#[path = "scoped_pr_review_operation_history_ui_v3.rs"]
pub mod ui;

/// Closed Summary-vs-Files boundary: this does not manufacture a review intent
/// to bypass the original Files/pending-journal gate.
pub struct ReviewOperationHistoryIntentV3 {
    pub(super) source: Arc<CatalogueIntentSource>,
    pub(super) active: Arc<ActiveRemoteScopeV1>,
    pub(super) navigation: Arc<ScopedPrNavigationV1>,
    expected: ThreadHistoryParentObservationV1,
    selected: usize,
    scroll: usize,
}
impl std::fmt::Debug for ReviewOperationHistoryIntentV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ReviewOperationHistoryIntentV3(<actual Summary; inspection only>)")
    }
}
impl ReviewOperationHistoryIntentV3 {
    pub fn capture(app: &App) -> Result<Arc<Self>, String> {
        let github = app
            .github
            .as_ref()
            .ok_or("Open a genuine fresh scoped PR Summary")?;
        let page = github
            .pr
            .as_ref()
            .filter(|page| page.tab == crate::github_workspace::PrTab::Summary)
            .ok_or("Protected review operation history opens from Summary, not blocked Files")?;
        let original = Arc::new(Self {
            source: app
                .remote_scope_source
                .as_ref()
                .ok_or("Original Summary source unavailable")?
                .clone(),
            active: app
                .remote_scope
                .as_ref()
                .ok_or("Original active Summary scope unavailable")?
                .clone(),
            navigation: github
                .scoped_pr_navigation
                .as_ref()
                .ok_or("Original Summary navigation unavailable")?
                .clone(),
            expected: ThreadHistoryParentObservationV1::capture(&page.details.summary)
                .map_err(|_| "Actual bounded Summary observation unavailable")?,
            selected: app.selected,
            scroll: app.scroll,
        });
        if app.overlay.is_some() || !original.owns(app) {
            return Err("Choose the exact fresh scoped Summary presentation".into());
        }
        Ok(original)
    }
    pub fn scope(&self) -> &Scope {
        self.source.scope()
    }
    pub(crate) fn source(&self) -> &Arc<CatalogueIntentSource> {
        &self.source
    }
    pub(crate) fn active(&self) -> &Arc<ActiveRemoteScopeV1> {
        &self.active
    }
    pub(crate) fn navigation(&self) -> &Arc<ScopedPrNavigationV1> {
        &self.navigation
    }
    pub fn owns(&self, app: &App) -> bool {
        app.view == View::Inbox
            && app.selected == self.selected
            && app.scroll == self.scroll
            && app
                .remote_scope_source
                .as_ref()
                .is_some_and(|source| Arc::ptr_eq(source, &self.source))
            && app
                .remote_scope
                .as_ref()
                .is_some_and(|active| Arc::ptr_eq(active, &self.active))
            && app.github.as_ref().is_some_and(|github| {
                github.scope().ok().as_ref() == Some(self.scope())
                    && github
                        .scoped_pr_navigation
                        .as_ref()
                        .is_some_and(|navigation| Arc::ptr_eq(navigation, &self.navigation))
                    && github.pr.as_ref().is_some_and(|page| {
                        page.tab == crate::github_workspace::PrTab::Summary
                            && self.expected.matches(&page.details.summary)
                            && super::super::super::matches_page(&self.navigation, page)
                    })
            })
    }
}

pub struct PreparedReviewOperationHistoryV3 {
    pub(crate) intent: Arc<ReviewOperationHistoryIntentV3>,
    pub(crate) identity: HostIdentity,
    pub(crate) labels: Arc<[String]>,
    pub(crate) opened: Option<usize>,
    pub(crate) display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    pub(crate) actor: crate::identity::ActorIdentity,
    pub(crate) inspector_label: Arc<str>,
    snapshot: ReviewOperationHistorySnapshotV3,
}
impl std::fmt::Debug for PreparedReviewOperationHistoryV3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedReviewOperationHistoryV3(<closed historical text; no action permit>)")
    }
}
impl PreparedReviewOperationHistoryV3 {
    pub fn intent(&self) -> &Arc<ReviewOperationHistoryIntentV3> {
        &self.intent
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn rows(&self) -> usize {
        self.labels.len()
    }
}

pub fn load(
    intent: Arc<ReviewOperationHistoryIntentV3>,
    previous: Option<(&Arc<PreparedReviewOperationHistoryV3>, usize)>,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<Arc<PreparedReviewOperationHistoryV3>, ScopedReviewFailureV2> {
    clock(cancel, deadline)?;
    if previous.is_some_and(|(old, row)| !Arc::ptr_eq(old.intent(), &intent) || row >= old.rows()) {
        return Err(refused(
            "Choose an actual original review-operation history row",
        ));
    }
    let issuer = provider.clone().with_catalogue_read_deadline(deadline);
    let mut session = issuer
        .begin_repository_catalogue_access(intent.source.source(), &Default::default(), cancel)
        .map_err(source_failure)?;
    let identity = session.identity().clone();
    let RemoteScopeConsumerV1::Explicit(members) = intent.active.consumer(&identity) else {
        return Err(source_failure(safe_local()));
    };
    if Scope::github(&identity.host, identity.account_id)
        .ok()
        .as_ref()
        != Some(intent.scope())
        || !members
            .references()
            .iter()
            .any(|member| member.original() == intent.navigation.original())
    {
        return Err(source_failure(safe_local()));
    }
    // Exactly the opening/closing raw parents share ONE original measured
    // allocation; repo/principal closing capacity stays with the same session.
    let window = session
        .reserve_measured_external_reads(2, 128 * 1024, 1024 * 1024, 2, cancel)
        .map_err(source_failure)?;
    let measured = issuer
        .clone()
        .with_measured_read_window(window)
        .map_err(source_failure)?;
    let current = session
        .verify(intent.navigation.original(), cancel)
        .map_err(source_failure)?;
    let parent = measured
        .pr_thread_history_parent_v1(current, &intent.expected, cancel)
        .map_err(source_failure)?;
    Cache::open(database)
        .map_err(|_| source_failure(safe_local()))?
        .validate_repository_catalogue_intent_source(&intent.source)
        .map_err(|_| source_failure(safe_local()))?;
    let journal = PrWriteJournal::open(database).map_err(|_| source_failure(safe_local()))?;
    let target = PrWriteTarget::new(&identity, parent.summary())
        .map_err(|_| source_failure(safe_local()))?;
    if let Some((old, _)) = previous {
        journal
            .validate_review_operation_history_v3(
                &target,
                parent.repository_id(),
                parent.repository_node(),
                parent.database_id(),
                &old.snapshot,
            )
            .map_err(|_| source_failure(safe_local()))?;
    }
    let snapshot = journal
        .review_operation_history_v3(
            &target,
            parent.repository_id(),
            parent.repository_node(),
            parent.database_id(),
        )
        .map_err(|_| source_failure(safe_local()))?;
    let opened = previous.map(|(_, row)| row);
    let mut lines=vec![format!("FRESH current {}#{} • repo #{} • PR #{}",parent.repository(),parent.number(),parent.repository_id(),parent.database_id()),
        format!("CURRENT H {} • base {}",parent.head_sha(),parent.base_tip_sha()),
        format!("{} original review-operation attempts • {} • current inspector UID {:?}",snapshot.rows.len(),
            snapshot.total.map_or_else(||"further rows UNKNOWN".into(),|count|format!("complete bounded enumeration {count}")),identity.account_id),
        "INSPECT ONLY. No restore, retry, discard, ACK, re-request or approval. Current deleted-comment/review existence and server causality remain UNKNOWN.".into()];
    let labels = snapshot
        .rows
        .iter()
        .map(|row| {
            crate::git::display_bytes(
                format!(
                    "#{} {:?} {} • {}",
                    row.marker.attempt_id,
                    row.marker.state,
                    row.marker.command,
                    if row.origin.is_some() {
                        "original metadata recorded"
                    } else {
                        "protected legacy — original target unavailable"
                    }
                )
                .as_bytes(),
            )
        })
        .collect::<Vec<_>>();
    let mut body_witness = None;
    if let Some(index) = opened {
        let row = snapshot
            .rows
            .get(index)
            .ok_or_else(|| source_failure(safe_local()))?;
        let marker = &row.marker;
        lines.extend([
            format!(
                "Recorded attempt #{} • {:?} • {}",
                marker.attempt_id, marker.state, marker.command
            ),
            format!(
                "Local journal UNIX {} seconds — NOT a server event time",
                marker.created_at
            ),
            format!(
                "Historical source H {} — NOT current H",
                marker.observed_head
            ),
            format!("Exact original payload SHA256 {}", marker.payload_sha256),
        ]);
        if let Some(origin) = &row.origin {
            let metadata = origin.seed.metadata();
            lines.extend(metadata.history_lines());
            if let Some(effect) = &marker.confirmed {
                lines.push(format!("Recorded observed receipt {:?} #{} / {} — NOT a current source or causality proof",effect.kind,
                    effect.resource_id,effect.node_id.as_deref().unwrap_or("unavailable")));
            }
            let encoded = Cache::open(database)
                .map_err(|_| source_failure(safe_local()))?
                .bounded_draft(intent.scope(), metadata.draft_key(), REVIEW_DRAFT_BYTES_V2)
                .map_err(|_| source_failure(safe_local()))?;
            if let Some(encoded) = &encoded {
                let (body, reference) =
                    draft_v3::decode(encoded).map_err(|_| source_failure(safe_local()))?;
                if reference.as_ref() != Some(&origin.reference)
                    || metadata
                        .write(&body)
                        .and_then(|write| write.digest().map_err(|error| error.to_string()))
                        .map_err(|_| source_failure(safe_local()))?
                        != marker.payload_sha256
                {
                    return Err(source_failure(safe_local()));
                }
                lines.push(
                    "Exact protected LOCAL requested text; NOT a current server body or new draft:"
                        .into(),
                );
                lines.extend(body.split('\n').map(str::to_owned));
            } else {
                lines.push(if marker.state==PrWriteState::Confirmed{
                    "Local requested body unavailable; confirmation may have cleaned it. The actual recorded receipt is inspectable."
                }else{
                    "Local body unavailable; persistence/ACK is UNKNOWN, never an empty body or resend permission."
                }.into());
            }
            body_witness = Some((metadata.draft_key().to_owned(), encoded));
        } else {
            lines.push("Protected legacy record has no original observation. Do not guess digest targets or open a possibly unrelated body.".into());
        }
    }
    clock(cancel, deadline)?;
    let raw = lines
        .iter()
        .chain(&labels)
        .try_fold(0usize, |sum, line| sum.checked_add(line.len()))
        .filter(|bytes| *bytes <= 1024 * 1024)
        .ok_or_else(|| refused("Bounded original history presentation exceeds1MiB"))?;
    let _ = raw;
    let lines = lines
        .into_iter()
        .map(|line| crate::git::display_bytes(line.as_bytes()))
        .collect::<Vec<_>>();
    let display =
        crate::prepared_form_text_v1::PreparedFormTextV1::prepare(&lines, cancel, deadline)
            .map_err(|_| {
                refused("Original history display cannot be prepared within its original phase")
            })?;
    let current = session
        .observation_v1(parent.repository_id())
        .ok_or_else(|| source_failure(safe_local()))?;
    let last = measured
        .pr_thread_history_parent_v1(current, &intent.expected, cancel)
        .map_err(source_failure)?;
    if !GithubProvider::same_thread_history_parent_v1(&parent, &last) {
        return Err(source_failure(safe_local()));
    }
    session
        .verify(intent.navigation.original(), cancel)
        .map_err(source_failure)?;
    let closed = session
        .close_with_guard(cancel, || {
            private_clock(cancel, deadline)?;
            let cache = Cache::open(database).map_err(|_| safe_local())?;
            cache
                .validate_repository_catalogue_intent_source(&intent.source)
                .map_err(|_| safe_local())?;
            PrWriteJournal::open(database)
                .map_err(|_| safe_local())?
                .validate_review_operation_history_v3(
                    &target,
                    parent.repository_id(),
                    parent.repository_node(),
                    parent.database_id(),
                    &snapshot,
                )
                .map_err(|_| safe_local())?;
            if let Some((key, expected)) = &body_witness {
                if cache
                    .bounded_draft(intent.scope(), key, REVIEW_DRAFT_BYTES_V2)
                    .map_err(|_| safe_local())?
                    .as_ref()
                    != expected.as_ref()
                {
                    return Err(safe_local());
                }
            }
            private_clock(cancel, deadline)
        })
        .map_err(source_failure)?;
    if closed.identity().login.len() > 4096 {
        return Err(refused(
            "Actual closing history requester metadata exceeds bounded display",
        ));
    }
    let identity = closed.identity().clone();
    let actor = crate::identity::ActorIdentity::github(
        &identity.host,
        &Actor {
            login: identity.login.clone(),
            ..Default::default()
        },
    );
    let inspector_label = Arc::from(format!(
        "Current inspector {} • numeric UID {:?} • original history only",
        actor.label(),
        identity.account_id
    ));
    clock(cancel, deadline)?;
    let result = Arc::new(PreparedReviewOperationHistoryV3 {
        intent,
        identity,
        labels: labels.into(),
        opened,
        display,
        actor,
        inspector_label,
        snapshot,
    });
    clock(cancel, deadline)?;
    Ok(result)
}
