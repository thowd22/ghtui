//! Worker-only original-thread draft/preview/single-attempt publication.
//! Pending history never restores approval, an account or a write permit.
use super::*;
use crate::{
    drafts::{DraftClient, DraftPublicationFenceV2},
    pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrPublicationReference, PrWriteIntent, PrWriteJournal,
        PrWriteMarker, PrWriteState, PrWriteTarget,
    },
    providers::gh::{GithubError, HostIdentity, ThreadPublicationFailureV1},
    scoped_pr_observed_tabs_v1::resources::{file_identity, safe_local},
    storage::Cache,
};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

pub const BODY_CAP: usize = 65_536;
pub const DRAFT_CAP: usize = 128 * 1024;
#[derive(Debug)]
pub struct ThreadPlanFailureV1 {
    pub hide_private: bool,
    message: &'static str,
}
impl ThreadPlanFailureV1 {
    pub fn message(&self) -> &'static str {
        self.message
    }
}
fn unavailable(_: impl std::fmt::Debug) -> ThreadPlanFailureV1 {
    ThreadPlanFailureV1 {hide_private:true, message:"Original thread source/account/storage unavailable; private views hidden; refresh explicitly"}
}
fn refused(message: &'static str) -> ThreadPlanFailureV1 {
    ThreadPlanFailureV1 {
        hide_private: false,
        message,
    }
}
fn check(cancel: &CancellationToken, deadline: Instant) -> Result<(), ThreadPlanFailureV1> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err(refused(
            "Original thread phase cancelled or expired; no automatic retry",
        ))
    } else {
        Ok(())
    }
}
fn private_check(cancel: &CancellationToken, deadline: Instant) -> Result<(), GithubError> {
    check(cancel, deadline).map_err(|_| safe_local())
}
/// Producer revision/ACK does not witness an external SAME-database writer.
/// Re-read bounded exact encoded body+reference under the original clock.
pub(crate) fn original_draft_matches_v1(
    database: &Path,
    scope: &Scope,
    key: &str,
    expected: &str,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<(), GithubError> {
    private_check(cancel, deadline)?;
    if expected.len() > DRAFT_CAP {
        return Err(safe_local());
    }
    let current = Cache::open(database)
        .map_err(|_| safe_local())?
        .bounded_draft(scope, key, DRAFT_CAP)
        .map_err(|_| safe_local())?;
    if current.as_deref() != Some(expected) {
        return Err(safe_local());
    }
    private_check(cancel, deadline)
}

struct ThreadLocalV1 {
    path: PathBuf,
    file: (u64, u64),
    target: PrWriteTarget,
    generation: i64,
    blocking: Option<PrWriteMarker>,
}
impl ThreadLocalV1 {
    fn capture(
        path: &Path,
        intent: &ScopedThreadPublicationIntentV1,
        identity: &HostIdentity,
        actual: &crate::providers::gh::PrResourceParentV1,
    ) -> Result<Self, GithubError> {
        Cache::open(path)
            .map_err(|_| safe_local())?
            .validate_repository_catalogue_intent_source(&intent.prepared.intent.source)
            .map_err(|_| safe_local())?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        let target = PrWriteTarget::new(identity, actual.summary()).map_err(|_| safe_local())?;
        let generation = journal
            .source_generation(&target)
            .map_err(|_| safe_local())?;
        if generation < 0 {
            return Err(safe_local());
        }
        let blocking = journal.source_blocking(&target).map_err(|_| safe_local())?;
        let path = journal.database_path().to_owned();
        Ok(Self {
            file: file_identity(&path)?,
            path,
            target,
            generation,
            blocking,
        })
    }
    fn validate_file(
        &self,
        path: &Path,
        intent: &ScopedThreadPublicationIntentV1,
    ) -> Result<(), GithubError> {
        if std::fs::canonicalize(path).map_err(|_| safe_local())? != self.path
            || file_identity(path)? != self.file
        {
            return Err(safe_local());
        }
        Cache::open(path)
            .map_err(|_| safe_local())?
            .validate_repository_catalogue_intent_source(&intent.prepared.intent.source)
            .map_err(|_| safe_local())?;
        Ok(())
    }
    fn validate(
        &self,
        path: &Path,
        intent: &ScopedThreadPublicationIntentV1,
    ) -> Result<(), GithubError> {
        self.validate_file(path, intent)?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        let current = journal
            .source_blocking(&self.target)
            .map_err(|_| safe_local())?;
        if journal
            .source_generation(&self.target)
            .map_err(|_| safe_local())?
            != self.generation
            || !match (&current, &self.blocking) {
                (Some(a), Some(b)) => {
                    a.target == b.target
                        && a.command == b.command
                        && a.selected_resource == b.selected_resource
                        && a.publication_reference() == b.publication_reference()
                        && a.state == b.state
                }
                (None, None) => true,
                _ => false,
            }
        {
            return Err(safe_local());
        }
        Ok(())
    }
    fn validate_owned(
        &self,
        path: &Path,
        intent: &ScopedThreadPublicationIntentV1,
        marker: &PrWriteMarker,
        generation: i64,
        state: PrWriteState,
    ) -> Result<(), GithubError> {
        self.validate_file(path, intent)?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        if journal
            .source_generation(&self.target)
            .map_err(|_| safe_local())?
            != generation
        {
            return Err(safe_local());
        }
        let actual = journal
            .original_publication(&self.target, &marker.publication_reference())
            .map_err(|_| safe_local())?
            .ok_or_else(safe_local)?;
        if actual.target != marker.target
            || actual.command != marker.command
            || actual.selected_resource != marker.selected_resource
            || actual.observed_head != marker.observed_head
            || actual.payload_sha256 != marker.payload_sha256
            || actual.state != state
        {
            return Err(safe_local());
        }
        let blocking = journal
            .source_blocking(&self.target)
            .map_err(|_| safe_local())?;
        match state {
            PrWriteState::Pending
                if blocking.as_ref().is_some_and(|actual| {
                    actual.publication_reference() == marker.publication_reference()
                        && actual.state == PrWriteState::Pending
                }) =>
            {
                Ok(())
            }
            PrWriteState::Confirmed if blocking.is_none() => Ok(()),
            _ => Err(safe_local()),
        }
    }
}
struct FreshThreadV1 {
    actual: OriginalThreadPublicationV1,
    identity: HostIdentity,
    local: ThreadLocalV1,
}
/// Fixed bounded read, never a retry: same original queued deadline/token,
/// <=7 external children (raw parent+two REST bridge+three GraphQL+closing
/// parent), same measured8MiB pool and closing original SQL before final UID.
fn fresh<T>(
    intent: &ScopedThreadPublicationIntentV1,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    read: impl FnOnce(
        &OriginalThreadPublicationV1,
        &HostIdentity,
        &ThreadLocalV1,
    ) -> Result<T, ThreadPlanFailureV1>,
) -> Result<(FreshThreadV1, T), ThreadPlanFailureV1> {
    check(cancel, deadline)?;
    let provider = provider.clone().with_catalogue_read_deadline(deadline);
    let source = &intent.prepared.intent.source;
    let mut session = provider
        .begin_repository_catalogue_access(source.source(), &Default::default(), cancel)
        .map_err(unavailable)?;
    let identity = session.identity().clone();
    let window = session
        .reserve_measured_external_reads(7, 1024 * 1024, 8 * 1024 * 1024, 2, cancel)
        .map_err(unavailable)?;
    let bounded = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(unavailable)?;
    let current = session
        .verify(intent.prepared.intent.navigation.original(), cancel)
        .map_err(unavailable)?;
    let parent = intent
        .prepared
        .intent
        .read_parent_v4(&bounded, current, cancel)
        .map_err(unavailable)?;
    private_check(cancel, deadline).map_err(unavailable)?;
    let local =
        ThreadLocalV1::capture(database, intent, &identity, &parent).map_err(unavailable)?;
    let actual = bounded
        .original_thread_publication_v1(parent, &intent.thread().node_id, cancel, deadline)
        .map_err(unavailable)?;
    intent.fresh_matches(&actual, false).map_err(|_| {
        refused("Original thread placement/root changed; no private draft or approval admitted")
    })?;
    private_check(cancel, deadline).map_err(unavailable)?;
    local.validate(database, intent).map_err(unavailable)?;
    let value = read(&actual, &identity, &local)?;
    private_check(cancel, deadline).map_err(unavailable)?;
    intent
        .prepared
        .intent
        .read_parent_v4(&bounded, current, cancel)
        .map_err(unavailable)?;
    session
        .verify(intent.prepared.intent.navigation.original(), cancel)
        .map_err(unavailable)?;
    let closed = session
        .close_with_guard(cancel, || {
            private_check(cancel, deadline)?;
            local.validate(database, intent)?;
            private_check(cancel, deadline)
        })
        .map_err(unavailable)?;
    check(cancel, deadline)?;
    Ok((
        FreshThreadV1 {
            actual,
            identity: closed.identity().clone(),
            local,
        },
        value,
    ))
}
pub struct ScopedThreadDraftV1 {
    intent: Arc<ScopedThreadPublicationIntentV1>,
    body: Arc<str>,
    publication: Option<PrPublicationReference>,
    identity: HostIdentity,
    key: String,
}
impl std::fmt::Debug for ScopedThreadDraftV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedThreadDraftV1(<fresh private history; no approval>)")
    }
}
impl ScopedThreadDraftV1 {
    pub fn intent(&self) -> &Arc<ScopedThreadPublicationIntentV1> {
        &self.intent
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn publication(&self) -> Option<&PrPublicationReference> {
        self.publication.as_ref()
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn draft_key(&self) -> &str {
        &self.key
    }
    pub fn open(
        intent: Arc<ScopedThreadPublicationIntentV1>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self, ThreadPlanFailureV1> {
        let key = intent
            .draft_key()
            .map_err(|_| refused("Original thread draft key unavailable"))?;
        let (closed, (body, publication)) = fresh(
            &intent,
            provider,
            database,
            cancel,
            deadline,
            |_, _, local| {
                // A pending marker is history, not access. Only this exact thread,
                // action and H can read this bounded draft after fresh source proof.
                if local.blocking.as_ref().is_some_and(|marker| {
                    marker.command != intent.action.command()
                        || marker.observed_head != intent.head()
                        || !intent.historical_selection_matches(&marker.selected_resource)
                }) {
                    return Err(refused("Another original publication blocks this PR; inspect it before any new write"));
                }
                let encoded = Cache::open(database)
                    .map_err(unavailable)?
                    .bounded_draft(intent.scope(), &key, DRAFT_CAP)
                    .map_err(unavailable)?;
                let Some(encoded) = encoded else {
                    if local.blocking.is_some() {
                        return Err(refused("Pending publication exists but its body is unavailable; inspect without resending"));
                    }
                    return Ok((Arc::<str>::from(""), None));
                };
                let value: Value = serde_json::from_str(&encoded)
                    .map_err(|_| refused("Malformed thread draft; nothing restored"))?;
                let object = value
                    .as_object()
                    .filter(|object| object.len() == 3)
                    .ok_or_else(|| refused("Unknown thread draft fields"))?;
                let values = object
                    .get("values")
                    .and_then(Value::as_object)
                    .filter(|values| values.len() == 1)
                    .ok_or_else(|| refused("Thread draft must contain only bounded body text"))?;
                let body = values
                    .get("body")
                    .and_then(Value::as_str)
                    .filter(|body| body.len() <= BODY_CAP)
                    .ok_or_else(|| refused("Thread draft body exceeds64KiB"))?;
                if object.get("version").and_then(Value::as_u64) != Some(2)
                    || (intent.action != ThreadPublicationActionV1::Reply && !body.is_empty())
                {
                    return Err(refused("Wrong thread draft version or state-action body"));
                }
                let publication: Option<PrPublicationReference> = serde_json::from_value(
                    object
                        .get("publication")
                        .cloned()
                        .ok_or_else(|| refused("Thread publication metadata missing"))?,
                )
                .map_err(|_| refused("Malformed thread publication history"))?;
                if let Some(reference) = &publication {
                    let journal = PrWriteJournal::open(database).map_err(unavailable)?;
                    let marker = journal
                        .original_publication(&local.target, reference)
                        .map_err(unavailable)?
                        .ok_or_else(|| unavailable(safe_local()))?;
                    if marker.command != intent.action.command()
                        || marker.observed_head != intent.head()
                        || !intent.historical_selection_matches(&marker.selected_resource)
                        || !matches!(
                            marker.state,
                            PrWriteState::Pending
                                | PrWriteState::Uncertain
                                | PrWriteState::Confirmed
                        )
                    {
                        return Err(unavailable(safe_local()));
                    }
                    if local
                        .blocking
                        .as_ref()
                        .is_some_and(|blocking| blocking.publication_reference() != *reference)
                    {
                        return Err(unavailable(safe_local()));
                    }
                } else if local.blocking.is_some() {
                    return Err(refused(
                        "Pending thread reference missing; no new approval or retry restored",
                    ));
                }
                Ok((Arc::from(body), publication))
            },
        )?;
        Ok(Self {
            intent,
            body,
            publication,
            identity: closed.identity,
            key,
        })
    }
}
pub struct ScopedThreadPreviewV1 {
    intent: Arc<ScopedThreadPublicationIntentV1>,
    body: Arc<str>,
    identity: HostIdentity,
    local: ThreadLocalV1,
    write: PrWriteIntent,
    draft_key: String,
    lines: Arc<[String]>,
    display_v1: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    started: AtomicBool,
}
impl std::fmt::Debug for ScopedThreadPreviewV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedThreadPreviewV1(<fresh bounded thread preview; no permit>)")
    }
}
impl ScopedThreadPreviewV1 {
    pub fn intent(&self) -> &Arc<ScopedThreadPublicationIntentV1> {
        &self.intent
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
    pub fn prepared_lines(&self) -> &Arc<[String]> {
        &self.lines
    }
    pub(crate) fn display_v1(&self) -> &Arc<crate::prepared_form_text_v1::PreparedFormTextV1> {
        &self.display_v1
    }
    pub fn draft_key(&self) -> &str {
        &self.draft_key
    }
    pub fn prepare(
        intent: Arc<ScopedThreadPublicationIntentV1>,
        body: Arc<str>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ThreadPlanFailureV1> {
        check(cancel, deadline)?;
        if body.len() > BODY_CAP
            || (intent.action == ThreadPublicationActionV1::Reply && body.trim().is_empty())
            || (intent.action != ThreadPublicationActionV1::Reply && !body.is_empty())
        {
            return Err(refused(
                "Reply requires nonempty <=64KiB body; resolution actions do not send comment text",
            ));
        }
        let maximum_reference = PrPublicationReference {
            attempt_id: i64::MAX,
            pr_node_id: intent.prepared.parent_v4.node_id().into(),
            observed_head: intent.head().into(),
            payload_sha256: "f".repeat(64),
        };
        if encode_draft(&body, Some(&maximum_reference)).is_err() {
            return Err(refused(
                "Thread draft exceeds128KiB after JSON encoding; no truncation",
            ));
        }
        let (closed, ()) = fresh(
            &intent,
            provider,
            database,
            cancel,
            deadline,
            |actual, _, local| {
                if local.blocking.is_some() {
                    return Err(refused(
                        "An unresolved publication blocks this PR; inspect without resending",
                    ));
                }
                intent.fresh_matches(actual, true).map_err(|_| {
                    refused(
                        "Original state/capability/root changed; refresh before another preview",
                    )
                })
            },
        )?;
        let selected = intent
            .selected_key(closed.actual.thread().resolved)
            .map_err(|_| refused("Original thread write key unavailable"))?;
        let draft_key = intent
            .draft_key()
            .map_err(|_| refused("Original thread draft key unavailable"))?;
        let mut values = BTreeMap::new();
        values.insert("body".into(), body.to_string());
        values.insert("thread-node".into(), intent.thread().node_id.clone());
        values.insert("action".into(), intent.action.command().into());
        values.insert("base".into(), closed.actual.parent().base_tip_sha().into());
        values.insert(
            "repo-id".into(),
            closed.actual.parent().repository_id().to_string(),
        );
        values.insert(
            "pr-id".into(),
            closed.actual.parent().database_id().to_string(),
        );
        if intent.action == ThreadPublicationActionV1::Reply {
            let root = closed.actual.original_reply_root().map_err(unavailable)?;
            values.insert("root-node".into(), root.node_id.clone());
            values.insert(
                "root-id".into(),
                root.database_id.expect("actual root checked").to_string(),
            );
            values.insert(
                "parent-comment-commit".into(),
                root.commit_sha
                    .as_ref()
                    .expect("actual root commit checked")
                    .clone(),
            );
        }
        let write = PrWriteIntent {
            command: intent.action.command().into(),
            selected_resource: selected,
            observed_head: intent.head().into(),
            values,
        };
        write
            .digest()
            .map_err(|_| refused("Thread publication fingerprint exceeds bounds"))?;
        let mut lines = vec![
            format!(
                "{} as {} • numeric account {:?}",
                intent.action.label(),
                ActorIdentity::github(
                    &closed.identity.host,
                    &crate::providers::gh::Actor {
                        login: closed.identity.login.clone(),
                        actor_type: "User".into(),
                        ..Default::default()
                    }
                )
                .label(),
                closed.identity.account_id
            ),
            format!(
                "Original {} PR #{} • repo #{} / PR #{}",
                closed.actual.parent().repository(),
                closed.actual.parent().number(),
                closed.actual.parent().repository_id(),
                closed.actual.parent().database_id()
            ),
            format!("Exact opaque thread {}", intent.thread().node_id),
            format!(
                "Independent current PR H {} • base {}",
                intent.head(),
                closed.actual.parent().base_tip_sha()
            ),
            format!(
                "Observed thread state {:?} • outdated {:?} • subject {:?}",
                closed.actual.thread().resolved,
                closed.actual.thread().outdated,
                closed.actual.thread().subject_type
            ),
        ];
        if intent.action == ThreadPublicationActionV1::Reply {
            let root = closed.actual.original_reply_root().map_err(unavailable)?;
            lines.push(format!(
                "Actual replyTo:null original comment #{} • {}",
                root.database_id.expect("actual ID checked"),
                root.node_id
            ));
            lines.push(format!(
                "Reply parent-comment commit {} (NOT replaced by PR H) • observed path {}",
                root.commit_sha.as_ref().expect("actual commit checked"),
                root.observed_path.as_deref().unwrap_or("unavailable")
            ));
        }
        lines.push("Explicit Confirm rechecks source/account/thread/storage, durably saves the original reference, then attempts once. Observed readback is not proof of causality; no automatic retry.".into());
        let mut lines: Vec<_> = lines
            .into_iter()
            .map(|line| crate::git::display_bytes(line.as_bytes()))
            .collect();
        if lines.iter().any(|line| line.len() > FIELD_CAP)
            || lines.iter().map(String::len).sum::<usize>() > 64 * 1024
        {
            return Err(refused("Thread preview display exceeds bounds"));
        }
        if intent.action == ThreadPublicationActionV1::Reply {
            lines.push("Exact reply body (terminal controls escaped):".into());
            lines.extend(
                body.split('\n')
                    .map(|line| crate::git::display_bytes(line.as_bytes())),
            );
        }
        if lines.len() > 65_536 || lines.iter().map(String::len).sum::<usize>() > 512 * 1024 {
            return Err(refused("Thread preview body presentation exceeds bounds"));
        }
        let display_v1 = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
            &lines, cancel, deadline,
        )
        .map_err(|_| {
            refused("Thread Preview display exceeds bounded native presentation; draft retained")
        })?;
        check(cancel, deadline)?;
        let result = Arc::new(Self {
            intent,
            body,
            identity: closed.identity,
            local: closed.local,
            write,
            draft_key,
            lines: lines.into(),
            display_v1,
            started: AtomicBool::new(false),
        });
        check(cancel, deadline)?;
        Ok(result)
    }
}
pub fn encode_draft(
    body: &str,
    publication: Option<&PrPublicationReference>,
) -> Result<String, ThreadPlanFailureV1> {
    let bytes = preflight_encoded_draft_v1(body, publication)?;
    let mut encoded = Vec::with_capacity(bytes);
    write_encoded_draft_v1(body, publication, &mut encoded)?;
    String::from_utf8(encoded).map_err(|_| refused("Thread draft cannot encode"))
}
fn write_encoded_draft_v1(
    body: &str,
    publication: Option<&PrPublicationReference>,
    writer: impl std::io::Write,
) -> Result<(), ThreadPlanFailureV1> {
    if body.len() > BODY_CAP {
        return Err(refused("Thread draft body exceeds64KiB"));
    }
    #[derive(serde::Serialize)]
    struct Values<'a> {
        body: &'a str,
    }
    #[derive(serde::Serialize)]
    struct Envelope<'a> {
        version: u8,
        values: Values<'a>,
        publication: Option<&'a PrPublicationReference>,
    }
    serde_json::to_writer(
        writer,
        &Envelope {
            version: 2,
            values: Values { body },
            publication,
        },
    )
    .map_err(|_| refused("Encoded thread draft exceeds128KiB"))
}
/// Counts borrowed exact JSON (including the real reference) before any
/// encoded-buffer allocation or body clone. UI requests use this same shape.
pub(super) fn preflight_encoded_draft_v1(
    body: &str,
    publication: Option<&PrPublicationReference>,
) -> Result<usize, ThreadPlanFailureV1> {
    struct Bound(usize);
    impl std::io::Write for Bound {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|bytes| *bytes <= DRAFT_CAP)
                .ok_or_else(|| std::io::Error::other("Thread draft exceeds128KiB"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bound = Bound(0);
    write_encoded_draft_v1(body, publication, &mut bound)?;
    Ok(bound.0)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadPublicationStateV1 {
    NotStarted,
    Uncertain,
    ConfirmedSnapshotRetired,
    ConfirmedButJournalFailed,
    ConfirmedButSourceUnavailable,
}
pub struct ScopedThreadPublicationV1 {
    state: ThreadPublicationStateV1,
    reference: PrPublicationReference,
    confirmed_reply_id: Option<u64>,
    source_retired: bool,
    draft_retained: bool,
}
impl ScopedThreadPublicationV1 {
    pub fn state(&self) -> ThreadPublicationStateV1 {
        self.state
    }
    pub fn reference(&self) -> &PrPublicationReference {
        &self.reference
    }
    pub fn confirmed_reply_id(&self) -> Option<u64> {
        self.confirmed_reply_id
    }
    pub fn source_retired(&self) -> bool {
        self.source_retired
    }
    pub fn hide_private(&self) -> bool {
        true
    }
    pub fn draft_retained(&self) -> bool {
        self.draft_retained
    }
}
fn pending(
    reference: PrPublicationReference,
    state: ThreadPublicationStateV1,
) -> ScopedThreadPublicationV1 {
    ScopedThreadPublicationV1 {
        state,
        reference,
        confirmed_reply_id: None,
        source_retired: false,
        draft_retained: true,
    }
}

impl ScopedThreadPreviewV1 {
    /// ONE original Confirm source session/pool and a prepaid six-child
    /// post-close allowance. Preview is not reused as current source proof.
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        provider: &GithubProvider,
        database: &Path,
        drafts: &DraftClient,
        publication: DraftPublicationFenceV2,
        confirmed: bool,
        restored: Option<&PrPublicationReference>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<ScopedThreadPublicationV1, ThreadPlanFailureV1> {
        let deadline = deadline.min(publication.deadline());
        check(cancel, deadline)?;
        if !confirmed || restored.is_some() {
            return Err(refused(
                "Fresh explicit Confirm required; restored history is never resend permission",
            ));
        }
        if self
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(refused(
                "This exact thread preview already attempted; inspect its durable outcome",
            ));
        }
        self.local
            .validate(database, &self.intent)
            .map_err(unavailable)?;
        let phase_provider = provider.clone().with_catalogue_read_deadline(deadline);
        let source = &self.intent.prepared.intent.source;
        let mut session = phase_provider
            .begin_repository_catalogue_access(source.source(), &Default::default(), cancel)
            .map_err(unavailable)?;
        let identity = session.identity().clone();
        if identity.host != self.identity.host
            || identity.account_id != self.identity.account_id
            || identity.account_id.is_none_or(|id| id == 0)
        {
            return Err(unavailable(safe_local()));
        }
        // Reserve conservative ordinary captures BEFORE the only measured pool.
        // Actual prewrite parent/thread/UID/closing children all share that Arc.
        let mut captures = session
            .reserve_postwrite_capture_v2(
                &crate::providers::gh::THREAD_POSTWRITE_CAPS_V2,
                2,
                cancel,
            )
            .map_err(unavailable)?;
        let window = session
            .reserve_measured_external_reads(10, 1024 * 1024, 8 * 1024 * 1024, 2, cancel)
            .map_err(unavailable)?;
        let bounded = phase_provider
            .clone()
            .with_measured_read_window(window)
            .map_err(unavailable)?;
        captures
            .validate_prewrite(&bounded, cancel)
            .map_err(unavailable)?;
        let current = session
            .verify(self.intent.prepared.intent.navigation.original(), cancel)
            .map_err(unavailable)?;
        let parent = self
            .intent
            .prepared
            .intent
            .read_parent_v4(&bounded, current, cancel)
            .map_err(unavailable)?;
        private_check(cancel, deadline).map_err(unavailable)?;
        self.local
            .validate(database, &self.intent)
            .map_err(unavailable)?;
        let target = PrWriteTarget::new(&identity, parent.summary()).map_err(unavailable)?;
        if target != self.local.target {
            return Err(unavailable(safe_local()));
        }
        let actual = bounded
            .original_thread_publication_v1(parent, &self.intent.thread().node_id, cancel, deadline)
            .map_err(unavailable)?;
        self.intent.fresh_matches(&actual, true).map_err(|_| {
            refused("Original thread state/capability/root changed after Preview; nothing sent")
        })?;
        captures
            .validate_prewrite(&bounded, cancel)
            .map_err(unavailable)?;
        private_check(cancel, deadline).map_err(unavailable)?;
        self.local
            .validate(database, &self.intent)
            .map_err(unavailable)?;
        let mut journal = PrWriteJournal::open(database).map_err(unavailable)?;
        let origin = crate::pr_write_journal::thread_history_v1::ThreadOriginSeedV1::from_producer(
            database,
            &identity,
            &actual,
            self.intent.action.command(),
            &self.draft_key,
        )
        .map_err(unavailable)?;
        let permit = journal
            .begin_thread_v1(&target, &self.write, &origin)
            .map_err(unavailable)?;
        let marker = permit.marker();
        let reference = marker.publication_reference();
        let encoded = match encode_draft(&self.body, Some(&reference)) {
            Ok(encoded) => encoded,
            Err(_) => {
                let mut result = pending(reference, ThreadPublicationStateV1::NotStarted);
                result.draft_retained = false;
                return Ok(result);
            }
        };
        let begun = self
            .local
            .generation
            .checked_add(1)
            .ok_or_else(|| unavailable(safe_local()))?;
        let mut acknowledgement =
            match drafts.save_publication_and_wait_v2(publication, encoded.clone(), cancel) {
                Ok(acknowledgement) => Some(acknowledgement),
                Err(_) => {
                    let mut result = pending(reference, ThreadPublicationStateV1::NotStarted);
                    // Unknown ACK is neither no-persistence nor POST evidence.
                    result.draft_retained = false;
                    return Ok(result);
                }
            };
        // ACK does not identify the DB issuer. Independently inspect the SAME
        // original cache before claim, not any global/foreign client's cache.
        let saved = self
            .local
            .validate_owned(database, &self.intent, marker, begun, PrWriteState::Pending)
            .is_ok()
            && Cache::open(database)
                .and_then(|cache| {
                    cache.bounded_draft(self.intent.scope(), &self.draft_key, DRAFT_CAP)
                })
                .ok()
                .flatten()
                .as_deref()
                == Some(encoded.as_str());
        if !saved {
            let mut result = pending(reference, ThreadPublicationStateV1::NotStarted);
            result.draft_retained = false;
            return Ok(result);
        }
        if check(cancel, deadline).is_err()
            || acknowledgement.as_ref().is_none_or(|ack| {
                ack.check_current(
                    drafts,
                    database,
                    self.intent.scope(),
                    &self.draft_key,
                    cancel,
                )
                .is_err()
            })
            || journal.claim(&permit, &target, &self.write).is_err()
        {
            return Ok(pending(reference, ThreadPublicationStateV1::NotStarted));
        }
        let claimed = begun
            .checked_add(1)
            .ok_or_else(|| unavailable(safe_local()))?;
        let mut original_closed = None;
        let before_post = || {
            private_check(cancel, deadline)?;
            acknowledgement
                .as_ref()
                .ok_or_else(safe_local)?
                .check_current(
                    drafts,
                    database,
                    self.intent.scope(),
                    &self.draft_key,
                    cancel,
                )
                .map_err(|_| safe_local())?;
            let current = session
                .observation_v1(actual.parent().repository_id())
                .ok_or_else(safe_local)?;
            self.intent
                .prepared
                .intent
                .read_parent_v4(&bounded, current, cancel)?;
            session.verify(self.intent.prepared.intent.navigation.original(), cancel)?;
            let closed = session.close_with_guard(cancel, || {
                private_check(cancel, deadline)?;
                acknowledgement
                    .as_ref()
                    .ok_or_else(safe_local)?
                    .check_current(
                        drafts,
                        database,
                        self.intent.scope(),
                        &self.draft_key,
                        cancel,
                    )
                    .map_err(|_| safe_local())?;
                self.local.validate_owned(
                    database,
                    &self.intent,
                    marker,
                    claimed,
                    PrWriteState::Pending,
                )?;
                original_draft_matches_v1(
                    database,
                    self.intent.scope(),
                    &self.draft_key,
                    &encoded,
                    cancel,
                    deadline,
                )?;
                private_check(cancel, deadline)
            })?;
            if closed.identity().host != identity.host
                || closed.identity().account_id != identity.account_id
            {
                return Err(safe_local());
            }
            original_closed = Some(closed); // actual producer/final-principal, not a report
            acknowledgement
                .as_ref()
                .ok_or_else(safe_local)?
                .check_current(
                    drafts,
                    database,
                    self.intent.scope(),
                    &self.draft_key,
                    cancel,
                )
                .map_err(|_| safe_local())?;
            private_check(cancel, deadline)
        };
        let receipt = match self.intent.action {
            ThreadPublicationActionV1::Reply => phase_provider
                .reply_thread_guarded_v1(
                    &identity,
                    &actual,
                    &bounded,
                    &mut captures,
                    &self.body,
                    cancel,
                    deadline,
                    before_post,
                )
                .map(|receipt| {
                    let id = receipt.comment().id;
                    (
                        PrConfirmedEffect {
                            kind: PrEffectKind::Comment,
                            resource_id: id.to_string(),
                            node_id: Some(receipt.comment().node_id.clone()),
                        },
                        Some(id),
                    )
                }),
            ThreadPublicationActionV1::Resolve | ThreadPublicationActionV1::Reopen => {
                phase_provider
                    .resolve_thread_guarded_v1(
                        &identity,
                        &actual,
                        &bounded,
                        &mut captures,
                        self.intent.action.desired().expect("state action"),
                        cancel,
                        deadline,
                        before_post,
                    )
                    .map(|receipt| {
                        (
                            PrConfirmedEffect {
                                kind: PrEffectKind::Thread,
                                resource_id: if receipt.resolved() {
                                    "resolved"
                                } else {
                                    "unresolved"
                                }
                                .into(),
                                node_id: Some(receipt.thread_node().into()),
                            },
                            None,
                        )
                    })
            }
        };
        let (effect, confirmed_reply_id) = match receipt {
            Err(ThreadPublicationFailureV1::NotStarted(_)) => {
                return Ok(pending(reference, ThreadPublicationStateV1::NotStarted))
            }
            Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_)) => {
                let _ = journal.mark_uncertain(&permit);
                // Terminal possible-state/content fence, no private reads after.
                let source_retired = matches!(
                    crate::pr_projection::invalidate_unresolved_discussions(
                        database,
                        &identity,
                        actual.parent().summary(),
                        &reference
                    ),
                    Ok(Some(_))
                );
                let mut result = pending(reference, ThreadPublicationStateV1::Uncertain);
                result.source_retired = source_retired;
                return Ok(result);
            }
            Ok(receipt) => receipt,
        };
        if journal.mark_confirmed(&permit, &effect).is_err() {
            let source_retired = matches!(
                crate::pr_projection::invalidate_unresolved_discussions(
                    database,
                    &identity,
                    actual.parent().summary(),
                    &reference
                ),
                Ok(Some(_))
            );
            return Ok(ScopedThreadPublicationV1 {
                state: ThreadPublicationStateV1::ConfirmedButJournalFailed,
                reference,
                confirmed_reply_id,
                source_retired,
                draft_retained: true,
            });
        }
        let confirmed_generation = claimed
            .checked_add(1)
            .ok_or_else(|| unavailable(safe_local()))?;
        let mut draft_retained = true;
        let visible = original_closed
            .as_ref()
            .and_then(|closed| closed.observation(actual.parent().repository_id()))
            .is_some_and(|current| {
                phase_provider
                    .close_thread_postwrite_visibility_v2(
                        &identity,
                        &actual,
                        current,
                        &mut captures,
                        cancel,
                        deadline,
                        || {
                            private_check(cancel, deadline)?;
                            self.local.validate_owned(
                                database,
                                &self.intent,
                                marker,
                                confirmed_generation,
                                PrWriteState::Confirmed,
                            )?;
                            let ack = acknowledgement.take().ok_or_else(safe_local)?;
                            ack.check_current(
                                drafts,
                                database,
                                self.intent.scope(),
                                &self.draft_key,
                                cancel,
                            )
                            .map_err(|_| safe_local())?;
                            original_draft_matches_v1(
                                database,
                                self.intent.scope(),
                                &self.draft_key,
                                &encoded,
                                cancel,
                                deadline,
                            )?;
                            drafts
                                .remove_publication_and_wait_v2(ack, cancel)
                                .map_err(|_| safe_local())?;
                            draft_retained = false;
                            self.local.validate_owned(
                                database,
                                &self.intent,
                                marker,
                                confirmed_generation,
                                PrWriteState::Confirmed,
                            )?;
                            private_check(cancel, deadline)
                        },
                    )
                    .is_ok()
            })
            && check(cancel, deadline).is_ok();
        // Exactly two receipt children + four original visibility children.
        let visible = visible
            && captures.consumed_requests() == crate::providers::gh::THREAD_POSTWRITE_CAPS_V2.len();
        // Mandatory terminal source fence: no subsequent private read/cleanup.
        let source_retired = matches!(
            crate::pr_projection::invalidate_confirmed_discussions(
                database,
                &identity,
                actual.parent().summary(),
                &reference
            ),
            Ok(Some(_))
        );
        let state = if visible && source_retired && check(cancel, deadline).is_ok() {
            ThreadPublicationStateV1::ConfirmedSnapshotRetired
        } else {
            ThreadPublicationStateV1::ConfirmedButSourceUnavailable
        };
        Ok(ScopedThreadPublicationV1 {
            state,
            reference,
            confirmed_reply_id,
            source_retired,
            draft_retained,
        })
    }
}
