//! Purpose-specific comment draft/Preview/publication worker. Uses the same
//! original source, bounded ordered ACK writer, journal and transport allowance.
use super::*;
use crate::scoped_pr_observed_tabs_v1::resources::{file_identity, safe_local};
use crate::{
    drafts::{DraftClient, DraftPublicationFenceV2},
    pr_write_journal::{
        thread_comment_history_v2::ThreadCommentOriginSeedV2, PrConfirmedEffect, PrEffectKind,
        PrPublicationReference, PrWriteIntent, PrWriteJournal, PrWriteMarker, PrWriteState,
        PrWriteTarget,
    },
    providers::gh::{
        GithubError, HostIdentity, ThreadCommentMutationV2, ThreadCommentReceiptV2,
        ThreadPublicationFailureV1,
    },
    storage::Cache,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
pub const BODY_CAP: usize = 65_536;
pub const DRAFT_CAP: usize = 128 * 1024;
#[derive(Debug)]
pub struct CommentPlanFailureV2 {
    pub hide_private: bool,
    message: &'static str,
}
impl CommentPlanFailureV2 {
    pub fn message(&self) -> &'static str {
        self.message
    }
}
fn source<T>(_: T) -> CommentPlanFailureV2 {
    CommentPlanFailureV2{hide_private:true,message:"Original selected-comment source/account/storage unavailable • private views hidden • refresh explicitly"}
}
fn refused(message: &'static str) -> CommentPlanFailureV2 {
    CommentPlanFailureV2 {
        hide_private: false,
        message,
    }
}
fn phase(cancel: &CancellationToken, deadline: Instant) -> Result<(), CommentPlanFailureV2> {
    original_check(cancel, deadline)
        .map_err(|_| refused("Original comment operation canceled/expired • no automatic retry"))
}
struct CommentLocalV2 {
    path: PathBuf,
    file: (u64, u64),
    target: PrWriteTarget,
    generation: i64,
    blocking: Option<PrWriteMarker>,
}
impl CommentLocalV2 {
    fn capture(
        path: &Path,
        intent: &ThreadCommentIntentV2,
        identity: &HostIdentity,
        actual: &OriginalThreadCommentV2,
    ) -> Result<Self, GithubError> {
        let cache = Cache::open(path).map_err(|_| safe_local())?;
        cache
            .validate_repository_catalogue_intent_source(&intent.original().source)
            .map_err(|_| safe_local())?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        let target = PrWriteTarget::new(identity, actual.original().parent().summary())
            .map_err(|_| safe_local())?;
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
    fn file(&self, path: &Path, intent: &ThreadCommentIntentV2) -> Result<(), GithubError> {
        if std::fs::canonicalize(path).map_err(|_| safe_local())? != self.path
            || file_identity(path)? != self.file
        {
            return Err(safe_local());
        }
        Cache::open(path)
            .map_err(|_| safe_local())?
            .validate_repository_catalogue_intent_source(&intent.original().source)
            .map_err(|_| safe_local())
    }
    fn validate(&self, path: &Path, intent: &ThreadCommentIntentV2) -> Result<(), GithubError> {
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
        intent: &ThreadCommentIntentV2,
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
struct FreshV2 {
    actual: Arc<OriginalThreadCommentV2>,
    identity: HostIdentity,
    local: CommentLocalV2,
    seed: ThreadCommentOriginSeedV2,
    encoded: Option<String>,
}
fn fresh(
    intent: &ThreadCommentIntentV2,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    read_draft: bool,
) -> Result<FreshV2, CommentPlanFailureV2> {
    phase(cancel, deadline)?;
    let provider = provider.clone().with_catalogue_read_deadline(deadline);
    let old = intent.original();
    let mut session = provider
        .begin_repository_catalogue_access(old.source.source(), &Default::default(), cancel)
        .map_err(source)?;
    let identity = session.identity().clone();
    let RemoteScopeConsumerV1::Explicit(members) = old.active.consumer(&identity) else {
        return Err(source(()));
    };
    if Scope::github(&identity.host, identity.account_id)
        .ok()
        .as_ref()
        != Some(intent.scope())
        || !members
            .references()
            .iter()
            .any(|member| member.original() == old.navigation.original())
    {
        return Err(source(()));
    }
    let window = session
        .reserve_measured_external_reads(12, 1024 * 1024, 8 * 1024 * 1024, 2, cancel)
        .map_err(source)?;
    let bounded = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(source)?;
    let current = session
        .verify(old.navigation.original(), cancel)
        .map_err(source)?;
    let parent = bounded
        .pr_inline_original_parent_v1(
            current,
            &old.original_view.observed,
            &Default::default(),
            cancel,
        )
        .map_err(source)?;
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
    let actual = Arc::new(
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
    if !intent.matches_fresh(&actual) {
        return Err(refused(
            "Original comment changed; refresh explicitly before opening new intent",
        ));
    }
    let local = CommentLocalV2::capture(database, intent, &identity, &actual).map_err(source)?;
    if local.blocking.is_some() {
        return Err(refused("Protected publication exists • use Summary Thread history • no restored approval or resend"));
    }
    let seed =
        ThreadCommentOriginSeedV2::from_producer(database, &identity, &actual, intent.action)
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
    bounded
        .pr_inline_original_parent_v1(
            current,
            &old.original_view.observed,
            &Default::default(),
            cancel,
        )
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
    Ok(FreshV2 {
        actual,
        identity: closed.identity().clone(),
        local,
        seed,
        encoded,
    })
}
pub struct CommentDraftV2 {
    intent: Arc<ThreadCommentIntentV2>,
    body: Arc<str>,
    identity: HostIdentity,
    key: String,
    restored: bool,
}
impl CommentDraftV2 {
    pub fn intent(&self) -> &Arc<ThreadCommentIntentV2> {
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
    pub fn open(
        intent: Arc<ThreadCommentIntentV2>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self, CommentPlanFailureV2> {
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
        } else if intent.action == CommentActionV2::Edit {
            Arc::from(
                fresh
                    .actual
                    .comment()
                    .body
                    .as_deref()
                    .ok_or_else(|| source(()))?,
            )
        } else {
            Arc::from("")
        };
        if intent.action == CommentActionV2::Delete && !body.is_empty() {
            return Err(refused("Delete has no editable or submitted body"));
        }
        let key = fresh.seed.draft_key().map_err(source)?;
        phase(cancel, deadline)?;
        Ok(Self {
            intent,
            body,
            identity: fresh.identity,
            key,
            restored,
        })
    }
}
fn allowed(actual: &OriginalThreadCommentV2, action: CommentActionV2) -> bool {
    match action {
        CommentActionV2::Edit => actual.viewer_can_update() == Some(true),
        CommentActionV2::Delete => actual.viewer_can_delete() == Some(true),
    }
}
fn decode(value: &Value) -> Result<(&str, Option<PrPublicationReference>), CommentPlanFailureV2> {
    let object = value
        .as_object()
        .filter(|o| o.len() == 3)
        .ok_or_else(|| refused("Unknown comment draft fields"))?;
    let values = object
        .get("values")
        .and_then(Value::as_object)
        .filter(|o| o.len() == 1)
        .ok_or_else(|| refused("Exact comment body field unavailable"))?;
    if object.get("version").and_then(Value::as_u64) != Some(2) {
        return Err(refused("Unsupported comment draft version"));
    }
    let body = values
        .get("body")
        .and_then(Value::as_str)
        .filter(|body| body.len() <= BODY_CAP)
        .ok_or_else(|| refused("Comment body exceeds64KiB"))?;
    let reference = serde_json::from_value(
        object
            .get("publication")
            .cloned()
            .ok_or_else(|| refused("Publication reference missing"))?,
    )
    .map_err(|_| refused("Malformed protected comment reference"))?;
    Ok((body, reference))
}
pub struct CommentPreviewV2 {
    intent: Arc<ThreadCommentIntentV2>,
    body: Arc<str>,
    identity: HostIdentity,
    actual: Arc<OriginalThreadCommentV2>,
    local: CommentLocalV2,
    write: PrWriteIntent,
    key: String,
    lines: Arc<[String]>,
    display_v1: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    started: AtomicBool,
}
impl CommentPreviewV2 {
    pub fn intent(&self) -> &Arc<ThreadCommentIntentV2> {
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
        intent: Arc<ThreadCommentIntentV2>,
        body: Arc<str>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, CommentPlanFailureV2> {
        if body.len() > BODY_CAP
            || (intent.action == CommentActionV2::Edit && body.trim().is_empty())
            || (intent.action == CommentActionV2::Delete && !body.is_empty())
        {
            return Err(refused(
                "Edit needs nonempty64KiB text; Delete sends no body",
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
        let write = fresh.seed.write(&body).map_err(source)?;
        write.digest().map_err(source)?;
        let mut lines=vec![display(&format!("{} as {} • numeric action account {:?}",intent.action.label(),fresh.identity.login,fresh.identity.account_id)),
            display(&format!("Actual comment #{} / {} • recorded author numeric UID {:?}",fresh.seed.comment_id,fresh.seed.comment_node,fresh.seed.author_account_id)),
            display(&format!("Independent current PR H {} • base {} • observed parent-comment C {} • raw path {}",fresh.seed.head,fresh.seed.base,fresh.seed.comment_commit,fresh.seed.observed_path)),
            "Fresh explicit Confirm rechecks original source/current account/comment capability/body/time/storage, protects the exact draft reference, then invokes ONCE. Readback is an observation, not proof of causality. No automatic retry.".into()];
        let author = fresh
            .actual
            .comment()
            .author
            .as_ref()
            .map(|author| ActorIdentity::github(&fresh.identity.host, &author.actor))
            .unwrap_or_else(|| {
                ActorIdentity::github(
                    &fresh.identity.host,
                    &crate::providers::gh::Actor::default(),
                )
            });
        lines.insert(
            2,
            display(&format!(
                "Actual observed comment author {} • independent from acting account",
                author.label()
            )),
        );
        if intent.action == CommentActionV2::Edit {
            lines.push("Exact replacement body (terminal controls escaped):".into());
            lines.extend(body.split('\n').map(display));
        } else {
            lines.push("DELETE the exact submitted comment. BOTH documented HTTP204 acknowledgement and error-free exact-node null readback are required; neither alone/404/unknown is proof.".into());
        }
        if lines.len() > 65_536 || lines.iter().map(String::len).sum::<usize>() > 512 * 1024 {
            return Err(refused("Comment preview display exceeds bounds"));
        }
        let display_v1 = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
            &lines, cancel, deadline,
        )
        .map_err(|_| {
            refused("Comment Preview display exceeds bounded native presentation; draft retained")
        })?;
        phase(cancel, deadline)?;
        let result = Arc::new(Self {
            intent,
            body,
            identity: fresh.identity,
            actual: fresh.actual,
            local: fresh.local,
            write,
            key,
            lines: lines.into(),
            display_v1,
            started: AtomicBool::new(false),
        });
        phase(cancel, deadline)?;
        Ok(result)
    }
}
fn serialize_draft(
    body: &str,
    publication: Option<&PrPublicationReference>,
    writer: impl std::io::Write,
) -> Result<(), CommentPlanFailureV2> {
    if body.len() > BODY_CAP {
        return Err(refused("Comment body exceeds64KiB before materialization"));
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
    .map_err(|_| refused("Encoded comment draft exceeds128KiB"))
}
pub(crate) fn preflight(
    body: &str,
    publication: Option<&PrPublicationReference>,
) -> Result<usize, CommentPlanFailureV2> {
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
) -> Result<String, CommentPlanFailureV2> {
    let bytes = preflight(body, publication)?;
    let mut value = Vec::with_capacity(bytes);
    serialize_draft(body, publication, &mut value)?;
    String::from_utf8(value).map_err(|_| refused("Comment draft cannot encode"))
}
pub use super::super::super::publication_v1::ThreadPublicationStateV1 as CommentPublicationStateV2;
pub struct CommentPublicationV2 {
    state: CommentPublicationStateV2,
    reference: PrPublicationReference,
    confirmed_comment_id: Option<u64>,
    source_retired: bool,
    draft_retained: bool,
}
impl CommentPublicationV2 {
    pub fn state(&self) -> CommentPublicationStateV2 {
        self.state
    }
    pub fn reference(&self) -> &PrPublicationReference {
        &self.reference
    }
    pub fn confirmed_comment_id(&self) -> Option<u64> {
        self.confirmed_comment_id
    }
    pub fn source_retired(&self) -> bool {
        self.source_retired
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
    state: CommentPublicationStateV2,
) -> CommentPublicationV2 {
    CommentPublicationV2 {
        state,
        reference,
        confirmed_comment_id: None,
        source_retired: false,
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
impl CommentPreviewV2 {
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
    ) -> Result<CommentPublicationV2, CommentPlanFailureV2> {
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
        let mut session = provider
            .begin_repository_catalogue_access(old.source.source(), &Default::default(), cancel)
            .map_err(source)?;
        let identity = session.identity().clone();
        let RemoteScopeConsumerV1::Explicit(members) = old.active.consumer(&identity) else {
            return Err(source(()));
        };
        if identity.host != self.identity.host
            || identity.account_id != self.identity.account_id
            || Scope::github(&identity.host, identity.account_id)
                .ok()
                .as_ref()
                != Some(self.intent.scope())
            || !members
                .references()
                .iter()
                .any(|member| member.original() == old.navigation.original())
        {
            return Err(source(()));
        }
        let mut captures = session
            .reserve_postwrite_capture_v2(
                &crate::providers::gh::THREAD_COMMENT_POSTWRITE_CAPS_V2,
                2,
                cancel,
            )
            .map_err(source)?;
        // Parent1 + selected-thread bridge2/pages<=3 + selected-comment5 +
        // last numeric/parent guard3 + exact comment1 + closing parent1 =16.
        let window = session
            .reserve_measured_external_reads(16, 1024 * 1024, 8 * 1024 * 1024, 2, cancel)
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
        let parent = bounded
            .pr_inline_original_parent_v1(
                current,
                &old.original_view.observed,
                &Default::default(),
                cancel,
            )
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
        let actual = bounded
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
            .map_err(source)?;
        if !self.intent.matches_fresh(&actual)
            || !actual.same_observed_comment_v2(&self.actual)
            || !allowed(&actual, self.intent.action)
        {
            return Err(refused("Exact original comment/body/time/capability changed after Preview • nothing invoked"));
        }
        let seed = ThreadCommentOriginSeedV2::from_producer(
            database,
            &identity,
            &actual,
            self.intent.action,
        )
        .map_err(source)?;
        let write = seed.write(&self.body).map_err(source)?;
        if seed.draft_key().map_err(source)? != self.key
            || write.digest().map_err(source)? != self.write.digest().map_err(source)?
        {
            return Err(source(()));
        }
        self.local
            .validate(database, &self.intent)
            .map_err(source)?;
        let target =
            PrWriteTarget::new(&identity, actual.original().parent().summary()).map_err(source)?;
        if target != self.local.target {
            return Err(source(()));
        }
        let mut journal = PrWriteJournal::open(database).map_err(source)?;
        let permit = journal
            .begin_thread_comment_v2(&target, &write, &seed)
            .map_err(source)?;
        let marker = permit.marker();
        let reference = marker.publication_reference();
        let encoded = match encode_draft(&self.body, Some(&reference)) {
            Ok(value) => value,
            Err(_) => return Ok(pending(reference, CommentPublicationStateV2::NotStarted)),
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
                    let mut result = pending(reference, CommentPublicationStateV2::NotStarted);
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
            let mut result = pending(reference, CommentPublicationStateV2::NotStarted);
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
            return Ok(pending(reference, CommentPublicationStateV2::NotStarted));
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
                .observation_v1(actual.original().parent().repository_id())
                .ok_or_else(safe_local)?;
            bounded.pr_inline_original_parent_v1(
                current,
                &old.original_view.observed,
                &Default::default(),
                cancel,
            )?;
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
        let mutation = match self.intent.action {
            CommentActionV2::Edit => ThreadCommentMutationV2::Edit(&self.body),
            CommentActionV2::Delete => ThreadCommentMutationV2::Delete,
        };
        let receipt = provider.mutate_thread_comment_guarded_v2(
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
        let (effect, id) = match receipt {
            Err(ThreadPublicationFailureV1::NotStarted(_)) => {
                return Ok(pending(reference, CommentPublicationStateV2::NotStarted))
            }
            Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_)) => {
                let _ = journal.mark_uncertain(&permit);
                let retired = matches!(
                    crate::pr_projection::invalidate_unresolved_discussions(
                        database,
                        &identity,
                        actual.original().parent().summary(),
                        &reference
                    ),
                    Ok(Some(_))
                );
                let mut result = pending(reference, CommentPublicationStateV2::Uncertain);
                result.source_retired = retired;
                return Ok(result);
            }
            Ok(ThreadCommentReceiptV2::Edited(receipt)) => (
                PrConfirmedEffect {
                    kind: PrEffectKind::Comment,
                    resource_id: receipt.comment().id.to_string(),
                    node_id: Some(receipt.comment().node_id.clone()),
                },
                receipt.comment().id,
            ),
            Ok(ThreadCommentReceiptV2::Deleted(receipt)) => (
                PrConfirmedEffect {
                    kind: PrEffectKind::CommentDeleted,
                    resource_id: receipt.comment_id().to_string(),
                    node_id: Some(receipt.comment_node().into()),
                },
                receipt.comment_id(),
            ),
        };
        if journal.mark_confirmed(&permit, &effect).is_err() {
            let source_retired = matches!(
                crate::pr_projection::invalidate_unresolved_discussions(
                    database,
                    &identity,
                    actual.original().parent().summary(),
                    &reference
                ),
                Ok(Some(_))
            );
            return Ok(CommentPublicationV2 {
                state: CommentPublicationStateV2::ConfirmedButJournalFailed,
                reference,
                confirmed_comment_id: Some(id),
                source_retired,
                draft_retained: true,
            });
        }
        let confirmed_generation = claimed.checked_add(1).ok_or_else(|| source(()))?;
        let mut draft_retained = true;
        let visible = closed
            .as_ref()
            .and_then(|closed| closed.observation(actual.original().parent().repository_id()))
            .is_some_and(|current| {
                provider
                    .close_thread_postwrite_visibility_v2(
                        &identity,
                        actual.original(),
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
            && captures.consumed_requests()
                == crate::providers::gh::THREAD_COMMENT_POSTWRITE_CAPS_V2.len();
        // TERMINAL fence: no private SQL/body/cleanup follows this operation.
        let source_retired = matches!(
            crate::pr_projection::invalidate_confirmed_discussions(
                database,
                &identity,
                actual.original().parent().summary(),
                &reference
            ),
            Ok(Some(_))
        );
        let state = if visible && source_retired && phase(cancel, deadline).is_ok() {
            CommentPublicationStateV2::ConfirmedSnapshotRetired
        } else {
            CommentPublicationStateV2::ConfirmedButSourceUnavailable
        };
        Ok(CommentPublicationV2 {
            state,
            reference,
            confirmed_comment_id: Some(id),
            source_retired,
            draft_retained,
        })
    }
}
