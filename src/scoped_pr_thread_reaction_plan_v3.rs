//! Purpose-specific reaction Preview/publication worker. Uses the same
//! original source, bounded ordered ACK writer, journal and transport allowance.
use super::*;
use crate::scoped_pr_observed_tabs_v1::resources::{file_identity, safe_local};
use crate::{
    drafts::{DraftClient, DraftPublicationFenceV2},
    pr_write_journal::{
        thread_reaction_history_v3::ThreadReactionOriginSeedV3, PrConfirmedEffect, PrEffectKind,
        PrPublicationReference, PrWriteIntent, PrWriteJournal, PrWriteMarker, PrWriteState,
        PrWriteTarget,
    },
    providers::gh::{
        GithubError, HostIdentity, ThreadPublicationFailureV1, ThreadReactionReceiptV3,
    },
    storage::Cache,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
pub const BODY_CAP: usize = 0;
pub const DRAFT_CAP: usize = 128 * 1024;
#[derive(Debug)]
pub struct ReactionPlanFailureV3 {
    pub hide_private: bool,
    message: &'static str,
}
impl ReactionPlanFailureV3 {
    pub fn message(&self) -> &'static str {
        self.message
    }
}
fn source<T>(_: T) -> ReactionPlanFailureV3 {
    ReactionPlanFailureV3{hide_private:true,message:"Original selected-comment source/account/storage unavailable • private views hidden • refresh explicitly"}
}
fn refused(message: &'static str) -> ReactionPlanFailureV3 {
    ReactionPlanFailureV3 {
        hide_private: false,
        message,
    }
}
fn phase(cancel: &CancellationToken, deadline: Instant) -> Result<(), ReactionPlanFailureV3> {
    original_check(cancel, deadline)
        .map_err(|_| refused("Original comment operation canceled/expired • no automatic retry"))
}
struct ReactionLocalV3 {
    path: PathBuf,
    file: (u64, u64),
    target: PrWriteTarget,
    generation: i64,
    blocking: Option<PrWriteMarker>,
}
impl ReactionLocalV3 {
    fn capture(
        path: &Path,
        intent: &ThreadReactionIntentV3,
        identity: &HostIdentity,
        summary: &crate::providers::gh::PrSummary,
    ) -> Result<Self, GithubError> {
        let cache = Cache::open(path).map_err(|_| safe_local())?;
        intent
            .original()
            .source
            .validate_cache(&cache)
            .map_err(|_| safe_local())?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        let target = PrWriteTarget::new(identity, summary).map_err(|_| safe_local())?;
        let generation = journal
            .source_generation(&target)
            .map_err(|_| safe_local())?;
        if generation < 0 {
            return Err(safe_local());
        }
        let blocking = journal.source_blocking(&target).map_err(|_| safe_local())?;
        Ok(Self {
            path: journal.database_path().into(),
            file: file_identity(path)?,
            target,
            generation,
            blocking,
        })
    }
    fn file(&self, path: &Path, intent: &ThreadReactionIntentV3) -> Result<(), GithubError> {
        if std::fs::canonicalize(path).map_err(|_| safe_local())? != self.path
            || file_identity(path)? != self.file
        {
            return Err(safe_local());
        }
        intent
            .original()
            .source
            .validate_cache(&Cache::open(path).map_err(|_| safe_local())?)
            .map_err(|_| safe_local())
    }
    fn validate(&self, path: &Path, intent: &ThreadReactionIntentV3) -> Result<(), GithubError> {
        self.file(path, intent)?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        let current = journal
            .source_blocking(&self.target)
            .map_err(|_| safe_local())?;
        if journal
            .source_generation(&self.target)
            .map_err(|_| safe_local())?
            != self.generation
            || !match (&self.blocking, &current) {
                (None, None) => true,
                (Some(a), Some(b)) => {
                    a.publication_reference() == b.publication_reference()
                        && a.state == b.state
                        && a.command == b.command
                        && a.selected_resource == b.selected_resource
                }
                _ => false,
            }
        {
            return Err(safe_local());
        }
        Ok(())
    }
    fn owned(
        &self,
        path: &Path,
        intent: &ThreadReactionIntentV3,
        marker: &PrWriteMarker,
        generation: i64,
        state: PrWriteState,
    ) -> Result<(), GithubError> {
        self.file(path, intent)?;
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
                if blocking.as_ref().is_some_and(|current| {
                    current.publication_reference() == marker.publication_reference()
                        && current.state == state
                }) =>
            {
                Ok(())
            }
            PrWriteState::Confirmed if blocking.is_none() => Ok(()),
            _ => Err(safe_local()),
        }
    }
}
struct FreshV3 {
    actual: Arc<OriginalThreadReactionsV3>,
    identity: HostIdentity,
    local: ReactionLocalV3,
    seed: ThreadReactionOriginSeedV3,
    encoded: Option<String>,
    lines: Arc<[String]>,
    display_v1: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
}
fn preview_lines(
    intent: &ThreadReactionIntentV3,
    actual: &OriginalThreadReactionsV3,
    seed: &ThreadReactionOriginSeedV3,
    identity: &HostIdentity,
) -> Result<Arc<[String]>, ReactionPlanFailureV3> {
    let author = actual
        .comment()
        .comment()
        .author
        .as_ref()
        .map(|author| ActorIdentity::github(&identity.host, &author.actor))
        .unwrap_or_else(|| {
            ActorIdentity::github(&identity.host, &crate::providers::gh::Actor::default())
        });
    let mut lines=vec![display(&format!("{} • observed opening viewer {} • numeric action UID {:?}",intent.action.label(),identity.login,identity.account_id)),
        display(&format!("Actual comment #{} / {} • observed author {} • numeric author UID {:?}",seed.comment_id,seed.comment_node,author.label(),seed.author_account_id)),
        display(&format!("Current PR H {} • base {} • independent observed comment C {} • raw path {}",seed.head,seed.base,seed.comment_commit,seed.observed_path)),
        display(&format!("Content {} • {} actual actor pages • {}",seed.content.as_str(),actual.pages(),if actual.complete(){"complete observation"}else{"PARTIAL • further actors/absence unknown"})),
        "Confirm rechecks exact original source/current numeric account/comment capability/time/body witness/storage, protects the exact durable reference, then invokes ONCE. No editable body, retry or restored approval.".into()];
    match intent.action{
        ReactionActionV3::Add(_)=>lines.push("HTTP200 observes an already-existing reaction;201 observes a created reaction. Actual numeric ID/node/content/viewer must match bounded paged readback. Observation is NOT proof of causality.".into()),
        ReactionActionV3::Remove(id)=>{
            let row=actual.selected(id).ok_or_else(||refused("Actual selected reaction no longer observed"))?;
            let actor=row.actor().map(|actor|ActorIdentity::github(&identity.host,actor)).unwrap_or_else(||ActorIdentity::github(&identity.host,&crate::providers::gh::Actor::default()));
            lines.push(display(&format!("REMOVE actual reaction #{} / {} • observed actor {} • numeric UID {:?} • created {}",id,row.node(),actor.label(),row.account_id(),row.created_at())));
            lines.push("BOTH exact documented HTTP204 and COMPLETE error-free paged absence of that actual ID/node are required. Partial/error/404/unknown is uncertainty, never permission to resend.".into());
        }
    }
    if lines.iter().map(String::len).sum::<usize>() > 64 * 1024 {
        return Err(refused("Reaction preview metadata exceeds64KiB"));
    }
    Ok(lines.into())
}
fn fresh(
    intent: &ThreadReactionIntentV3,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    read_draft: bool,
) -> Result<FreshV3, ReactionPlanFailureV3> {
    phase(cancel, deadline)?;
    let provider = provider.clone().with_catalogue_read_deadline(deadline);
    let old = intent.original();
    let mut session = old
        .source
        .begin_access(&provider, &Default::default(), cancel)
        .map_err(source)?;
    let identity = session.identity().clone();
    if Scope::github(&identity.host, identity.account_id)
        .ok()
        .as_ref()
        != Some(intent.scope())
        || !old.admits_identity_v75(&identity)
    {
        return Err(source(()));
    }
    let window = session
        .reserve_measured_external_reads(20, 1024 * 1024, 8 * 1024 * 1024, 2, cancel)
        .map_err(source)?;
    let bounded = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(source)?;
    let current = session
        .verify(old.navigation.original(), cancel)
        .map_err(source)?;
    let parent = old
        .read_parent_v4(&bounded, current, cancel)
        .map_err(source)?;
    let local =
        ReactionLocalV3::capture(database, intent, &identity, parent.summary()).map_err(source)?;
    if local.blocking.is_some() {
        return Err(refused("Protected publication exists • use Summary reaction history • no restored approval or resend"));
    }
    let thread = Arc::new(
        bounded
            .original_thread_publication_v1(
                parent,
                intent.listed.intent.thread_node(),
                cancel,
                deadline,
            )
            .map_err(source)?,
    );
    let comment = Arc::new(
        bounded
            .original_thread_comment_v2(
                &identity,
                thread,
                &intent.comment().node_id,
                intent.comment().database_id.ok_or_else(|| source(()))?,
                cancel,
                deadline,
            )
            .map_err(source)?,
    );
    let actual = Arc::new(
        bounded
            .original_thread_reactions_v3(&identity, comment, cancel, deadline)
            .map_err(source)?,
    );
    if !intent.matches_fresh(&actual) {
        return Err(refused(
            "Original comment changed; refresh explicitly before opening new intent",
        ));
    }
    let seed =
        ThreadReactionOriginSeedV3::from_producer(database, &identity, &actual, intent.action)
            .map_err(source)?;
    let lines = preview_lines(intent, &actual, &seed, &identity)?;
    // Same original worker clock/token: all bounded wrapping is complete
    // BEFORE original SQL/raw-parent/repository/final numeric-principal close.
    let display_v1 =
        crate::prepared_form_text_v1::PreparedFormTextV1::prepare(&lines, cancel, deadline)
            .map_err(source)?;
    let key = seed.draft_key().map_err(source)?;
    let encoded = if read_draft {
        Cache::open(database)
            .map_err(source)?
            .bounded_draft(intent.scope(), &key, DRAFT_CAP)
            .map_err(source)?
    } else {
        None
    };
    local.validate(database, intent).map_err(source)?;
    old.read_parent_v4(&bounded, current, cancel)
        .map_err(source)?;
    session
        .verify(old.navigation.original(), cancel)
        .map_err(source)?;
    let closed = session
        .close_with_guard(cancel, || {
            original_check(cancel, deadline)?;
            local.validate(database, intent)?;
            if read_draft
                && Cache::open(database)
                    .map_err(|_| safe_local())?
                    .bounded_draft(intent.scope(), &key, DRAFT_CAP)
                    .map_err(|_| safe_local())?
                    != encoded
            {
                return Err(safe_local());
            }
            original_check(cancel, deadline)
        })
        .map_err(source)?;
    phase(cancel, deadline)?;
    Ok(FreshV3 {
        actual,
        identity: closed.identity().clone(),
        local,
        seed,
        encoded,
        lines,
        display_v1,
    })
}
pub struct ReactionDraftV3 {
    intent: Arc<ThreadReactionIntentV3>,
    body: Arc<str>,
    identity: HostIdentity,
    key: String,
    restored: bool,
    lines: Arc<[String]>,
}
impl ReactionDraftV3 {
    pub fn intent(&self) -> &Arc<ThreadReactionIntentV3> {
        &self.intent
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn draft_key(&self) -> &str {
        &self.key
    }
    pub fn restored(&self) -> bool {
        self.restored
    }
    pub fn lines(&self) -> &Arc<[String]> {
        &self.lines
    }
    pub fn open(
        intent: Arc<ThreadReactionIntentV3>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self, ReactionPlanFailureV3> {
        let fresh = fresh(&intent, provider, database, cancel, deadline, true)?;
        if !allowed(&fresh.actual, intent.action) {
            return Err(refused("Actual current comment-specific permission is false/unknown • no push/login fallback"));
        }
        let restored = fresh.encoded.is_some();
        let body = if let Some(encoded) = fresh.encoded {
            let value: Value = serde_json::from_str(&encoded)
                .map_err(|_| refused("Malformed comment draft • nothing restored"))?;
            let (body, reference) = decode(&value)?;
            if reference.is_some() {
                return Err(refused("Protected publication is inspect only • open Summary Thread history • no body restored into a writer"));
            }
            Arc::<str>::from(body)
        } else {
            Arc::from("")
        };
        if !body.is_empty() {
            return Err(refused("Reactions have no editable/submitted text body"));
        }
        let key = fresh.seed.draft_key().map_err(source)?;
        phase(cancel, deadline)?;
        Ok(Self {
            intent,
            body,
            identity: fresh.identity,
            key,
            restored,
            lines: fresh.lines,
        })
    }
}
fn allowed(actual: &OriginalThreadReactionsV3, action: ReactionActionV3) -> bool {
    actual.comment().viewer_can_react() == Some(true)
        && match action {
            ReactionActionV3::Add(_) => true,
            ReactionActionV3::Remove(id) => actual
                .selected(id)
                .is_some_and(|row| row.account_id() == Some(actual.comment().principal())),
        }
}
pub(crate) fn decode(
    value: &Value,
) -> Result<(&str, Option<PrPublicationReference>), ReactionPlanFailureV3> {
    let object = value
        .as_object()
        .filter(|o| o.len() == 3)
        .ok_or_else(|| refused("Unknown comment draft fields"))?;
    let values = object
        .get("values")
        .and_then(Value::as_object)
        .filter(|o| o.len() == 1)
        .ok_or_else(|| refused("Exact comment body field unavailable"))?;
    if object.get("version").and_then(Value::as_u64) != Some(3) {
        return Err(refused("Unsupported reaction intent draft version"));
    }
    let body = values
        .get("body")
        .and_then(Value::as_str)
        .filter(|body| body.is_empty())
        .ok_or_else(|| refused("Reactions do not edit/send text"))?;
    let reference = serde_json::from_value(
        object
            .get("publication")
            .cloned()
            .ok_or_else(|| refused("Publication reference missing"))?,
    )
    .map_err(|_| refused("Malformed protected comment reference"))?;
    Ok((body, reference))
}
pub struct ReactionPreviewV3 {
    intent: Arc<ThreadReactionIntentV3>,
    body: Arc<str>,
    identity: HostIdentity,
    actual: Arc<OriginalThreadReactionsV3>,
    local: ReactionLocalV3,
    write: PrWriteIntent,
    key: String,
    lines: Arc<[String]>,
    started: AtomicBool,
    display_v1: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
}
impl ReactionPreviewV3 {
    pub fn intent(&self) -> &Arc<ThreadReactionIntentV3> {
        &self.intent
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub fn draft_key(&self) -> &str {
        &self.key
    }
    pub fn lines(&self) -> &Arc<[String]> {
        &self.lines
    }
    pub(crate) fn display_v1(&self) -> &Arc<crate::prepared_form_text_v1::PreparedFormTextV1> {
        &self.display_v1
    }
    pub fn prepare(
        intent: Arc<ThreadReactionIntentV3>,
        body: Arc<str>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, ReactionPlanFailureV3> {
        if !body.is_empty() {
            return Err(refused(
                "Reaction actions have no editable or submitted text body",
            ));
        }
        let reference = PrPublicationReference {
            attempt_id: i64::MAX,
            pr_node_id: intent.listed.actual.parent().node_id().into(),
            observed_head: intent.listed.actual.parent().head_sha().into(),
            payload_sha256: "f".repeat(64),
        };
        preflight(&body, Some(&reference))?;
        let fresh = fresh(&intent, provider, database, cancel, deadline, false)?;
        if !allowed(&fresh.actual, intent.action) {
            return Err(refused(
                "Actual fresh comment capability false/unknown • nothing queued for write",
            ));
        }
        let key = fresh.seed.draft_key().map_err(source)?;
        let write = fresh.seed.write().map_err(source)?;
        write.digest().map_err(source)?;
        phase(cancel, deadline)?;
        Ok(Arc::new(Self {
            intent,
            body,
            identity: fresh.identity,
            actual: fresh.actual,
            local: fresh.local,
            write,
            key,
            lines: fresh.lines,
            display_v1: fresh.display_v1,
            started: AtomicBool::new(false),
        }))
    }
}
fn serialize_draft(
    body: &str,
    publication: Option<&PrPublicationReference>,
    writer: impl std::io::Write,
) -> Result<(), ReactionPlanFailureV3> {
    if !body.is_empty() {
        return Err(refused("Reaction intent has no text body"));
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
            version: 3,
            values: Values { body },
            publication,
        },
    )
    .map_err(|_| refused("Encoded reaction intent exceeds128KiB"))
}
pub(crate) fn preflight(
    body: &str,
    publication: Option<&PrPublicationReference>,
) -> Result<usize, ReactionPlanFailureV3> {
    struct Bound(usize);
    impl std::io::Write for Bound {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= DRAFT_CAP)
                .ok_or_else(|| std::io::Error::other("Comment draft exceeds128KiB"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bound = Bound(0);
    serialize_draft(body, publication, &mut bound)?;
    Ok(bound.0)
}
pub fn encode_draft(
    body: &str,
    publication: Option<&PrPublicationReference>,
) -> Result<String, ReactionPlanFailureV3> {
    let bytes = preflight(body, publication)?;
    let mut value = Vec::with_capacity(bytes);
    serialize_draft(body, publication, &mut value)?;
    String::from_utf8(value).map_err(|_| refused("Comment draft cannot encode"))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactionPublicationStateV3 {
    NotStarted,
    Uncertain,
    ConfirmedObserved,
    ConfirmedButJournalFailed,
    ConfirmedButVisibilityUnavailable,
}
pub struct ReactionPublicationV3 {
    state: ReactionPublicationStateV3,
    reference: PrPublicationReference,
    confirmed_reaction_id: Option<u64>,
    already_existed: Option<bool>,
    draft_retained: bool,
}
impl ReactionPublicationV3 {
    pub fn state(&self) -> ReactionPublicationStateV3 {
        self.state
    }
    pub fn reference(&self) -> &PrPublicationReference {
        &self.reference
    }
    pub fn confirmed_reaction_id(&self) -> Option<u64> {
        self.confirmed_reaction_id
    }
    pub fn already_existed(&self) -> Option<bool> {
        self.already_existed
    }
    pub fn draft_retained(&self) -> bool {
        self.draft_retained
    }
    pub fn hide_private(&self) -> bool {
        true
    }
}
fn pending(
    reference: PrPublicationReference,
    state: ReactionPublicationStateV3,
) -> ReactionPublicationV3 {
    ReactionPublicationV3 {
        state,
        reference,
        confirmed_reaction_id: None,
        already_existed: None,
        draft_retained: true,
    }
}
fn exact_draft(
    database: &Path,
    scope: &Scope,
    key: &str,
    encoded: &str,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<(), GithubError> {
    original_check(cancel, deadline)?;
    if encoded.len() > DRAFT_CAP
        || Cache::open(database)
            .map_err(|_| safe_local())?
            .bounded_draft(scope, key, DRAFT_CAP)
            .map_err(|_| safe_local())?
            .as_deref()
            != Some(encoded)
    {
        return Err(safe_local());
    }
    original_check(cancel, deadline)
}
impl ReactionPreviewV3 {
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
    ) -> Result<ReactionPublicationV3, ReactionPlanFailureV3> {
        let deadline = deadline.min(publication.deadline());
        phase(cancel, deadline)?;
        if !confirmed || restored.is_some() {
            return Err(refused("Fresh matching explicit Confirm required • protected history never restores approval"));
        }
        if self
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(refused(
                "This original Preview already attempted • inspect without resend",
            ));
        }
        self.local
            .validate(database, &self.intent)
            .map_err(source)?;
        let provider = provider.clone().with_catalogue_read_deadline(deadline);
        let old = self.intent.original();
        let mut session = old
            .source
            .begin_access(&provider, &Default::default(), cancel)
            .map_err(source)?;
        let identity = session.identity().clone();
        if identity.host != self.identity.host
            || identity.account_id != self.identity.account_id
            || Scope::github(&identity.host, identity.account_id)
                .ok()
                .as_ref()
                != Some(self.intent.scope())
            || !old.admits_identity_v75(&identity)
        {
            return Err(source(()));
        }
        let mut captures = session
            .reserve_postwrite_capture_v2(
                &crate::providers::gh::THREAD_REACTIONS_POSTWRITE_CAPS_V3,
                2,
                cancel,
            )
            .map_err(source)?;
        // Parent1 + selected thread<=5 + comment5 + actors<=8 + final guards4
        // + closing parent1 =24, SAME original measured pool, no reset.
        let window = session
            .reserve_measured_external_reads(24, 1024 * 1024, 8 * 1024 * 1024, 2, cancel)
            .map_err(source)?;
        let bounded = provider
            .clone()
            .with_measured_read_window(window)
            .map_err(source)?;
        captures
            .validate_prewrite(&bounded, cancel)
            .map_err(source)?;
        let current = session
            .verify(old.navigation.original(), cancel)
            .map_err(source)?;
        let parent = old
            .read_parent_v4(&bounded, current, cancel)
            .map_err(source)?;
        original_check(cancel, deadline).map_err(source)?;
        self.local
            .validate(database, &self.intent)
            .map_err(source)?;
        let thread = Arc::new(
            bounded
                .original_thread_publication_v1(
                    parent,
                    self.intent.listed.intent.thread_node(),
                    cancel,
                    deadline,
                )
                .map_err(source)?,
        );
        let selected = Arc::new(
            bounded
                .original_thread_comment_v2(
                    &identity,
                    thread,
                    &self.intent.comment().node_id,
                    self.intent
                        .comment()
                        .database_id
                        .ok_or_else(|| source(()))?,
                    cancel,
                    deadline,
                )
                .map_err(source)?,
        );
        let actual = bounded
            .original_thread_reactions_v3(&identity, selected, cancel, deadline)
            .map_err(source)?;
        if !self.intent.matches_fresh(&actual)
            || !actual
                .comment()
                .same_observed_comment_v2(self.actual.comment())
            || !allowed(&actual, self.intent.action)
        {
            return Err(refused("Exact original comment/body/time/capability changed after Preview • nothing invoked"));
        }
        let seed = ThreadReactionOriginSeedV3::from_producer(
            database,
            &identity,
            &actual,
            self.intent.action,
        )
        .map_err(source)?;
        let write = seed.write().map_err(source)?;
        if seed.draft_key().map_err(source)? != self.key
            || write.digest().map_err(source)? != self.write.digest().map_err(source)?
        {
            return Err(source(()));
        }
        self.local
            .validate(database, &self.intent)
            .map_err(source)?;
        let target = PrWriteTarget::new(&identity, actual.comment().original().parent().summary())
            .map_err(source)?;
        if target != self.local.target {
            return Err(source(()));
        }
        let mut journal = PrWriteJournal::open(database).map_err(source)?;
        let permit = journal
            .begin_thread_reaction_v3(&target, &write, &seed)
            .map_err(source)?;
        let marker = permit.marker();
        let reference = marker.publication_reference();
        let encoded = match encode_draft(&self.body, Some(&reference)) {
            Ok(value) => value,
            Err(_) => return Ok(pending(reference, ReactionPublicationStateV3::NotStarted)),
        };
        let begun = self
            .local
            .generation
            .checked_add(1)
            .ok_or_else(|| source(()))?;
        let mut ack =
            match drafts.save_publication_and_wait_v2(publication, encoded.clone(), cancel) {
                Ok(ack) => Some(ack),
                Err(_) => {
                    let mut result = pending(reference, ReactionPublicationStateV3::NotStarted);
                    result.draft_retained = false;
                    return Ok(result);
                }
            };
        let saved = self
            .local
            .owned(database, &self.intent, marker, begun, PrWriteState::Pending)
            .is_ok()
            && exact_draft(
                database,
                self.intent.scope(),
                &self.key,
                &encoded,
                cancel,
                deadline,
            )
            .is_ok();
        if !saved {
            let mut result = pending(reference, ReactionPublicationStateV3::NotStarted);
            result.draft_retained = false;
            return Ok(result);
        }
        if phase(cancel, deadline).is_err()
            || ack.as_ref().is_none_or(|ack| {
                ack.check_current(drafts, database, self.intent.scope(), &self.key, cancel)
                    .is_err()
            })
            || journal.claim(&permit, &target, &write).is_err()
        {
            return Ok(pending(reference, ReactionPublicationStateV3::NotStarted));
        }
        let claimed = begun.checked_add(1).ok_or_else(|| source(()))?;
        let mut closed = None;
        let before_write = || {
            original_check(cancel, deadline)?;
            ack.as_ref()
                .ok_or_else(safe_local)?
                .check_current(drafts, database, self.intent.scope(), &self.key, cancel)
                .map_err(|_| safe_local())?;
            let current = session
                .observation_v1(actual.comment().original().parent().repository_id())
                .ok_or_else(safe_local)?;
            old.read_parent_v4(&bounded, current, cancel)?;
            session.verify(old.navigation.original(), cancel)?;
            let actual_closed = session.close_with_guard(cancel, || {
                original_check(cancel, deadline)?;
                ack.as_ref()
                    .ok_or_else(safe_local)?
                    .check_current(drafts, database, self.intent.scope(), &self.key, cancel)
                    .map_err(|_| safe_local())?;
                self.local.owned(
                    database,
                    &self.intent,
                    marker,
                    claimed,
                    PrWriteState::Pending,
                )?;
                exact_draft(
                    database,
                    self.intent.scope(),
                    &self.key,
                    &encoded,
                    cancel,
                    deadline,
                )?;
                original_check(cancel, deadline)
            })?;
            if actual_closed.identity().host != identity.host
                || actual_closed.identity().account_id != identity.account_id
            {
                return Err(safe_local());
            }
            closed = Some(actual_closed);
            ack.as_ref()
                .ok_or_else(safe_local)?
                .check_current(drafts, database, self.intent.scope(), &self.key, cancel)
                .map_err(|_| safe_local())?;
            original_check(cancel, deadline)
        };
        let mutation = self.intent.action.mutation();
        let receipt = provider.mutate_thread_reaction_guarded_v3(
            &identity,
            &self.actual,
            &actual,
            &bounded,
            &mut captures,
            mutation,
            cancel,
            deadline,
            before_write,
        );
        let (effect, id, readback_pages, already_existed) = match receipt {
            Err(ThreadPublicationFailureV1::NotStarted(_)) => {
                return Ok(pending(reference, ReactionPublicationStateV3::NotStarted))
            }
            Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_)) => {
                let _ = journal.mark_uncertain(&permit);
                let _ = crate::pr_projection::invalidate_unresolved_discussions(
                    database,
                    &identity,
                    actual.comment().original().parent().summary(),
                    &reference,
                );
                return Ok(pending(reference, ReactionPublicationStateV3::Uncertain));
            }
            Ok(ThreadReactionReceiptV3::Added(receipt)) => (
                PrConfirmedEffect {
                    kind: PrEffectKind::Reaction,
                    resource_id: receipt.reaction().id().to_string(),
                    node_id: Some(receipt.reaction().node().into()),
                },
                receipt.reaction().id(),
                receipt.readback_pages(),
                Some(receipt.already_existed()),
            ),
            Ok(ThreadReactionReceiptV3::Removed(receipt)) => (
                PrConfirmedEffect {
                    kind: PrEffectKind::ReactionRemoved,
                    resource_id: receipt.reaction_id().to_string(),
                    node_id: Some(receipt.reaction_node().into()),
                },
                receipt.reaction_id(),
                receipt.readback_pages(),
                None,
            ),
        };
        if journal.mark_confirmed(&permit, &effect).is_err() {
            let _ = crate::pr_projection::invalidate_unresolved_discussions(
                database,
                &identity,
                actual.comment().original().parent().summary(),
                &reference,
            );
            return Ok(ReactionPublicationV3 {
                state: ReactionPublicationStateV3::ConfirmedButJournalFailed,
                reference,
                confirmed_reaction_id: Some(id),
                already_existed,
                draft_retained: true,
            });
        }
        let confirmed_generation = claimed.checked_add(1).ok_or_else(|| source(()))?;
        let mut draft_retained = true;
        let accounted = captures.consumed_requests() == 1 + readback_pages;
        let visible = accounted
            && closed
                .as_ref()
                .and_then(|closed| {
                    closed.verified().iter().find(|row| {
                        row.repository().id == actual.comment().original().parent().repository_id()
                    })
                })
                .is_some_and(|current| {
                    provider
                        .close_thread_postwrite_visibility_v2(
                            &identity,
                            actual.comment().original(),
                            current,
                            &mut captures,
                            cancel,
                            deadline,
                            || {
                                original_check(cancel, deadline)?;
                                self.local.owned(
                                    database,
                                    &self.intent,
                                    marker,
                                    confirmed_generation,
                                    PrWriteState::Confirmed,
                                )?;
                                exact_draft(
                                    database,
                                    self.intent.scope(),
                                    &self.key,
                                    &encoded,
                                    cancel,
                                    deadline,
                                )?;
                                let ack = ack.take().ok_or_else(safe_local)?;
                                ack.check_current(
                                    drafts,
                                    database,
                                    self.intent.scope(),
                                    &self.key,
                                    cancel,
                                )
                                .map_err(|_| safe_local())?;
                                drafts
                                    .remove_publication_and_wait_v2(ack, cancel)
                                    .map_err(|_| safe_local())?;
                                draft_retained = false;
                                self.local.owned(
                                    database,
                                    &self.intent,
                                    marker,
                                    confirmed_generation,
                                    PrWriteState::Confirmed,
                                )?;
                                original_check(cancel, deadline)
                            },
                        )
                        .is_ok()
                })
            && phase(cancel, deadline).is_ok()
            && captures.consumed_requests() == 5 + readback_pages;
        // TERMINAL CLOSED empty text impact: no body binding is fenced, no
        // private SQL/body/cleanup follows. Parent retires only original UI.
        let empty_impact = matches!(
            crate::pr_projection::invalidate_confirmed_discussions(
                database,
                &identity,
                actual.comment().original().parent().summary(),
                &reference
            ),
            Ok(None)
        );
        let state = if visible && empty_impact && phase(cancel, deadline).is_ok() {
            ReactionPublicationStateV3::ConfirmedObserved
        } else {
            ReactionPublicationStateV3::ConfirmedButVisibilityUnavailable
        };
        Ok(ReactionPublicationV3 {
            state,
            reference,
            confirmed_reaction_id: Some(id),
            already_existed,
            draft_retained,
        })
    }
}
