//! Closed fresh FILE draft/Preview/publication. The existing ordered writer,
//! PR journal and thread Inbox own the lifecycle; no retry or extra transport.
use super::*;
use crate::{
    drafts::{DraftClient, DraftPublicationFenceV2},
    pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrPublicationReference, PrWriteIntent, PrWriteJournal,
        PrWriteMarker, PrWriteState, PrWriteTarget,
    },
    providers::gh::{
        FileCommentBodyV75, HostIdentity, OriginalPrFileCommentV75,
        RepositoryCatalogueAccessLimits, ThreadPublicationFailureV1,
        FILE_COMMENT_POSTWRITE_CAPS_V75,
    },
    storage::Cache,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicBool, Ordering},
};
pub(crate) const BODY_CAP: usize = 65_536;
pub(crate) const DRAFT_CAP: usize = 128 * 1024;
pub(crate) struct FileCommentFailureV75 {
    pub hide_private: bool,
    message: &'static str,
}
impl std::fmt::Debug for FileCommentFailureV75 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}
impl FileCommentFailureV75 {
    pub(crate) fn message(&self) -> &'static str {
        self.message
    }
}
fn source<T>(_: T) -> FileCommentFailureV75 {
    FileCommentFailureV75{hide_private:true,message:"Original FILE source/account/storage unavailable • private views hidden • reload explicitly"}
}
fn refused(message: &'static str) -> FileCommentFailureV75 {
    FileCommentFailureV75 {
        hide_private: false,
        message,
    }
}
fn phase(cancel: &CancellationToken, deadline: Instant) -> Result<(), FileCommentFailureV75> {
    original_check(cancel, deadline)
        .map_err(|_| refused("Original FILE operation canceled/expired • no automatic retry"))
}
fn access_limits() -> RepositoryCatalogueAccessLimits {
    let mut limits = RepositoryCatalogueAccessLimits::default();
    limits.transport.max_response_bytes = limits.transport.max_response_bytes.min(8 * 1024 * 1024);
    limits.transport.max_document_bytes = limits.transport.max_document_bytes.min(128 * 1024);
    limits
}
struct FileLocalV75 {
    path: PathBuf,
    file: (u64, u64),
    target: PrWriteTarget,
    generation: i64,
}
impl FileLocalV75 {
    fn capture(
        path: &Path,
        intent: &FileCommentIntentV75,
        identity: &HostIdentity,
        actual: &OriginalPrFileCommentV75,
    ) -> Result<Self, GithubError> {
        intent
            .original
            .source
            .validate_cache(&Cache::open(path).map_err(|_| safe_local())?)
            .map_err(|_| safe_local())?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        let target =
            PrWriteTarget::new(identity, actual.parent().summary()).map_err(|_| safe_local())?;
        let generation = journal
            .source_generation(&target)
            .map_err(|_| safe_local())?;
        if generation < 0
            || journal
                .source_blocking(&target)
                .map_err(|_| safe_local())?
                .is_some()
        {
            return Err(safe_local());
        }
        Ok(Self {
            path: journal.database_path().into(),
            file: file_identity(path)?,
            target,
            generation,
        })
    }
    fn file(&self, path: &Path, intent: &FileCommentIntentV75) -> Result<(), GithubError> {
        if std::fs::canonicalize(path).map_err(|_| safe_local())? != self.path
            || file_identity(path)? != self.file
        {
            return Err(safe_local());
        }
        intent
            .original
            .source
            .validate_cache(&Cache::open(path).map_err(|_| safe_local())?)
            .map_err(|_| safe_local())
    }
    fn validate(&self, path: &Path, intent: &FileCommentIntentV75) -> Result<(), GithubError> {
        self.file(path, intent)?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        if journal
            .source_generation(&self.target)
            .map_err(|_| safe_local())?
            != self.generation
            || journal
                .source_blocking(&self.target)
                .map_err(|_| safe_local())?
                .is_some()
        {
            return Err(safe_local());
        }
        Ok(())
    }
    fn owned(
        &self,
        path: &Path,
        intent: &FileCommentIntentV75,
        marker: &PrWriteMarker,
        generation: i64,
        state: PrWriteState,
    ) -> Result<(), GithubError> {
        self.file(path, intent)?;
        let journal = PrWriteJournal::open(path).map_err(|_| safe_local())?;
        let current = journal
            .original_publication(&self.target, &marker.publication_reference())
            .map_err(|_| safe_local())?
            .ok_or_else(safe_local)?;
        if journal
            .source_generation(&self.target)
            .map_err(|_| safe_local())?
            != generation
            || current.state != state
            || current.command != marker.command
            || current.selected_resource != marker.selected_resource
            || current.payload_sha256 != marker.payload_sha256
            || current.target != marker.target
        {
            return Err(safe_local());
        }
        let blocking = journal
            .source_blocking(&self.target)
            .map_err(|_| safe_local())?;
        match state {
            PrWriteState::Pending
                if blocking.as_ref().is_some_and(|m| {
                    m.publication_reference() == marker.publication_reference() && m.state == state
                }) =>
            {
                Ok(())
            }
            PrWriteState::Confirmed if blocking.is_none() => Ok(()),
            _ => Err(safe_local()),
        }
    }
}
struct FreshV75 {
    actual: Arc<OriginalPrFileCommentV75>,
    identity: HostIdentity,
    local: FileLocalV75,
    encoded: Option<String>,
}
fn fresh<T>(
    intent: &FileCommentIntentV75,
    provider: &GithubProvider,
    database: &Path,
    cancel: &CancellationToken,
    deadline: Instant,
    read_draft: bool,
    prepare: impl FnOnce(
        &OriginalPrFileCommentV75,
        &HostIdentity,
        &FileCommentOriginSeedV75,
        Option<&str>,
    ) -> Result<T, FileCommentFailureV75>,
) -> Result<(FreshV75, T), FileCommentFailureV75> {
    phase(cancel, deadline)?;
    let provider = provider.clone().with_catalogue_read_deadline(deadline);
    let old = &intent.original;
    let mut session = old
        .source
        .begin_access(&provider, &access_limits(), cancel)
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
        .reserve_measured_external_reads(6, 128 * 1024, 8 * 1024 * 1024, 2, cancel)
        .map_err(source)?;
    let measured = provider
        .clone()
        .with_measured_read_window(window)
        .map_err(source)?;
    let current = session
        .verify(old.navigation.original(), cancel)
        .map_err(source)?;
    let (view, index) = intent.observed();
    let actual = Arc::new(
        measured
            .pr_file_comment_original_v75(
                current,
                &identity,
                &view.observed,
                index,
                &Default::default(),
                cancel,
                deadline,
                |_| {
                    // Capture only after genuine fresh parent. No body, draft or receipt
                    // enters this bracket until the original source is checked.
                    old.source
                        .validate_cache(&Cache::open(database).map_err(|_| safe_local())?)
                        .map_err(|_| safe_local())
                },
            )
            .map_err(source)?,
    );
    let local = FileLocalV75::capture(database, intent, &identity, &actual).map_err(source)?;
    let seed =
        FileCommentOriginSeedV75::from_producer(database, &identity, &actual).map_err(source)?;
    if seed.draft_key().map_err(source)? != intent.draft_key() {
        return Err(source(()));
    }
    let encoded = if read_draft {
        Cache::open(database)
            .map_err(source)?
            .bounded_draft(intent.scope(), intent.draft_key(), DRAFT_CAP)
            .map_err(source)?
    } else {
        None
    };
    let prepared = prepare(&actual, &identity, &seed, encoded.as_deref())?;
    phase(cancel, deadline)?;
    measured
        .file_comment_last_parent_v75(&actual, &identity, cancel, deadline)
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
                    .bounded_draft(intent.scope(), intent.draft_key(), DRAFT_CAP)
                    .map_err(|_| safe_local())?
                    != encoded
            {
                return Err(safe_local());
            }
            original_check(cancel, deadline)
        })
        .map_err(source)?;
    phase(cancel, deadline)?;
    Ok((
        FreshV75 {
            actual,
            identity: closed.identity().clone(),
            local,
            encoded,
        },
        prepared,
    ))
}
pub(crate) struct FileCommentDraftV75 {
    intent: Arc<FileCommentIntentV75>,
    body: Arc<str>,
    identity: HostIdentity,
    restored: bool,
}
impl FileCommentDraftV75 {
    pub(crate) fn intent(&self) -> &Arc<FileCommentIntentV75> {
        &self.intent
    }
    pub(crate) fn body(&self) -> &str {
        &self.body
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn restored(&self) -> bool {
        self.restored
    }
    pub(crate) fn open(
        intent: Arc<FileCommentIntentV75>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Self, FileCommentFailureV75> {
        let (fresh, body) = fresh(
            &intent,
            provider,
            database,
            cancel,
            deadline,
            true,
            |_, _, _, encoded| {
                if let Some(encoded) = encoded {
                    let (body, reference) = decode_draft(encoded)?;
                    if reference.is_some() {
                        return Err(refused("Protected FILE publication • inspect Summary FILE history • no writer restore/resend"));
                    }
                    Ok(Arc::<str>::from(body))
                } else {
                    Ok(Arc::<str>::from(""))
                }
            },
        )?;
        Ok(Self {
            intent,
            body,
            identity: fresh.identity,
            restored: fresh.encoded.is_some(),
        })
    }
}
pub(crate) struct FileCommentPreviewV75 {
    intent: Arc<FileCommentIntentV75>,
    body: Arc<str>,
    identity: HostIdentity,
    actual: Arc<OriginalPrFileCommentV75>,
    local: FileLocalV75,
    write: PrWriteIntent,
    payload_sha256: Arc<str>,
    lines: Arc<[String]>,
    display: Arc<crate::prepared_form_text_v1::PreparedFormTextV1>,
    started: AtomicBool,
}
impl FileCommentPreviewV75 {
    pub(crate) fn intent(&self) -> &Arc<FileCommentIntentV75> {
        &self.intent
    }
    pub(crate) fn body(&self) -> &str {
        &self.body
    }
    pub(crate) fn payload_sha256(&self) -> &str {
        &self.payload_sha256
    }
    pub(crate) fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    pub(crate) fn lines(&self) -> &Arc<[String]> {
        &self.lines
    }
    pub(crate) fn display_v1(&self) -> &Arc<crate::prepared_form_text_v1::PreparedFormTextV1> {
        &self.display
    }
    pub(crate) fn prepare(
        intent: Arc<FileCommentIntentV75>,
        body: Arc<str>,
        provider: &GithubProvider,
        database: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<Arc<Self>, FileCommentFailureV75> {
        FileCommentBodyV75::new(body.clone())
            .map_err(|_| refused("FILE comment needs nonempty text<=64KiB"))?;
        let (view, _) = intent.observed();
        let sentinel = PrPublicationReference {
            attempt_id: i64::MAX,
            pr_node_id: view.observed.parent().node_id().into(),
            observed_head: view.observed.parent().head_sha().into(),
            payload_sha256: "f".repeat(64),
        };
        preflight_draft(&body, Some(&sentinel))?;
        let (fresh, (lines, display, write, payload_sha256)) = fresh(
            &intent,
            provider,
            database,
            cancel,
            deadline,
            false,
            |actual, identity, seed, _| {
                FileCommentBodyV75::new(body.clone())
                    .map_err(source)?
                    .payload(actual)
                    .map_err(source)?;
                let p = actual.parent();
                let mut lines=vec![format!("FILE comment as {} • numeric account {:?}",identity.login,identity.account_id),format!("Original {} PR #{} • repository ID {} • PR ID {}",p.repository(),p.number(),p.repository_id(),p.database_id()),format!("Whole changed file {} • subject FILE • no line/range/thread inferred",actual.file().path),format!("Independent current PR H {} • base {}",p.head_sha(),p.base_tip_sha()),"Fresh Confirm rechecks current account, original source/head/file membership and durable exact draft, then sends ONCE. Server permission is authoritative; observed pull/push/login is not comment eligibility.".into(),"Requires actual HTTP201 + exact numeric GET + independent opaque FILE-node receipt. Readback is observed state, not proof of causality. No automatic retry.".into(),"Exact body (terminal controls escaped):".into()];
                lines.extend(body.split('\n').map(str::to_owned));
                let lines: Vec<String> = lines
                    .iter()
                    .map(|line| crate::git::display_bytes(line.as_bytes()))
                    .collect();
                if lines.len() > 65_536 || lines.iter().map(String::len).sum::<usize>() > 512 * 1024
                {
                    return Err(refused("FILE Preview display exceeds bounds"));
                }
                let display = crate::prepared_form_text_v1::PreparedFormTextV1::prepare(
                    &lines, cancel, deadline,
                )
                .map_err(source)?;
                let write = seed.write(&body).map_err(source)?;
                let payload_sha256 = Arc::<str>::from(write.digest().map_err(source)?);
                Ok((Arc::<[String]>::from(lines), display, write, payload_sha256))
            },
        )?;
        phase(cancel, deadline)?;
        Ok(Arc::new(Self {
            intent,
            body,
            identity: fresh.identity,
            actual: fresh.actual,
            local: fresh.local,
            write,
            payload_sha256,
            lines,
            display,
            started: AtomicBool::new(false),
        }))
    }
}
fn serialize_draft(
    body: &str,
    publication: Option<&PrPublicationReference>,
    writer: impl std::io::Write,
) -> Result<(), FileCommentFailureV75> {
    if body.len() > BODY_CAP {
        return Err(refused("FILE body exceeds64KiB before materialization"));
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
            version: 75,
            values: Values { body },
            publication,
        },
    )
    .map_err(|_| refused("Encoded FILE draft exceeds128KiB"))
}
pub(crate) fn preflight_draft(
    body: &str,
    publication: Option<&PrPublicationReference>,
) -> Result<usize, FileCommentFailureV75> {
    struct Bound(usize);
    impl std::io::Write for Bound {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(b.len())
                .filter(|n| *n <= DRAFT_CAP)
                .ok_or_else(|| std::io::Error::other("FILE draft exceeds128KiB"))?;
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Bound(0);
    serialize_draft(body, publication, &mut count)?;
    Ok(count.0)
}
pub(crate) fn encode_draft(
    body: &str,
    publication: Option<&PrPublicationReference>,
) -> Result<String, FileCommentFailureV75> {
    let n = preflight_draft(body, publication)?;
    let mut b = Vec::with_capacity(n);
    serialize_draft(body, publication, &mut b)?;
    String::from_utf8(b).map_err(source)
}
pub(crate) fn decode_draft(
    encoded: &str,
) -> Result<(String, Option<PrPublicationReference>), FileCommentFailureV75> {
    if encoded.len() > DRAFT_CAP {
        return Err(refused("FILE draft exceeds128KiB"));
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Values<'a> {
        #[serde(borrow)]
        body: std::borrow::Cow<'a, str>,
    }
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Envelope<'a> {
        version: u8,
        #[serde(borrow)]
        values: Values<'a>,
        publication: serde_json::Value,
    }
    let value: Envelope<'_> = serde_json::from_str(encoded)
        .map_err(|_| refused("Malformed FILE draft; nothing restored"))?;
    if value.version != 75 || value.values.body.len() > BODY_CAP {
        return Err(refused("FILE draft version/body exceeds bounds"));
    }
    if !value.publication.is_null()
        && value.publication.as_object().is_none_or(|reference| {
            reference.len() != 4
                || ![
                    "attempt_id",
                    "pr_node_id",
                    "observed_head",
                    "payload_sha256",
                ]
                .iter()
                .all(|key| reference.contains_key(*key))
        })
    {
        return Err(refused("Closed exact FILE publication reference required"));
    }
    let reference: Option<PrPublicationReference> = serde_json::from_value(value.publication)
        .map_err(|_| refused("Malformed FILE publication reference"))?;
    preflight_draft(&value.values.body, reference.as_ref())?;
    Ok((value.values.body.into_owned(), reference))
}

pub(crate) use crate::scoped_pr_observed_tabs_v1::resources::threads_v1::publication_v1::ThreadPublicationStateV1 as FileCommentPublicationStateV75;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileDraftRetentionV75 {
    Retained,
    Removed,
    Unknown,
}
pub(crate) struct FileCommentPublicationV75 {
    state: FileCommentPublicationStateV75,
    reference: PrPublicationReference,
    confirmed_id: Option<u64>,
    source_retired: bool,
    draft: FileDraftRetentionV75,
}
impl FileCommentPublicationV75 {
    pub(crate) fn state(&self) -> FileCommentPublicationStateV75 {
        self.state
    }
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        &self.reference
    }
    pub(crate) fn confirmed_comment_id(&self) -> Option<u64> {
        self.confirmed_id
    }
    pub(crate) fn source_retired(&self) -> bool {
        self.source_retired
    }
    pub(crate) fn draft_retention(&self) -> FileDraftRetentionV75 {
        self.draft
    }
    pub(crate) fn hide_private(&self) -> bool {
        true
    }
}
fn pending(
    reference: PrPublicationReference,
    state: FileCommentPublicationStateV75,
) -> FileCommentPublicationV75 {
    FileCommentPublicationV75 {
        state,
        reference,
        confirmed_id: None,
        source_retired: false,
        draft: FileDraftRetentionV75::Retained,
    }
}
fn exact_draft(
    database: &Path,
    intent: &FileCommentIntentV75,
    encoded: &str,
    cancel: &CancellationToken,
    deadline: Instant,
) -> Result<(), GithubError> {
    original_check(cancel, deadline)?;
    if encoded.len() > DRAFT_CAP
        || Cache::open(database)
            .map_err(|_| safe_local())?
            .bounded_draft(intent.scope(), intent.draft_key(), DRAFT_CAP)
            .map_err(|_| safe_local())?
            .as_deref()
            != Some(encoded)
    {
        return Err(safe_local());
    }
    original_check(cancel, deadline)
}
impl FileCommentPreviewV75 {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn execute(
        &self,
        provider: &GithubProvider,
        database: &Path,
        drafts: &DraftClient,
        publication: DraftPublicationFenceV2,
        confirmed: bool,
        restored: Option<&PrPublicationReference>,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<FileCommentPublicationV75, FileCommentFailureV75> {
        let deadline = deadline.min(publication.deadline());
        phase(cancel, deadline)?;
        if !confirmed || restored.is_some() {
            return Err(refused("Fresh explicit FILE Preview/Confirm required • protected history never restores approval"));
        }
        if self
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(refused(
                "This FILE Preview already attempted • inspect without resend",
            ));
        }
        self.local
            .validate(database, &self.intent)
            .map_err(source)?;
        let provider = provider.clone().with_catalogue_read_deadline(deadline);
        let old = &self.intent.original;
        let mut session = old
            .source
            .begin_access(&provider, &access_limits(), cancel)
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
            .reserve_postwrite_capture_v2(&FILE_COMMENT_POSTWRITE_CAPS_V75, 2, cancel)
            .map_err(source)?;
        let window = session
            .reserve_measured_external_reads(6, 128 * 1024, 8 * 1024 * 1024, 2, cancel)
            .map_err(source)?;
        let measured = provider
            .clone()
            .with_measured_read_window(window)
            .map_err(source)?;
        captures
            .validate_prewrite(&measured, cancel)
            .map_err(source)?;
        let current = session
            .verify(old.navigation.original(), cancel)
            .map_err(source)?;
        let (view, index) = self.intent.observed();
        let actual = measured
            .pr_file_comment_original_v75(
                current,
                &identity,
                &view.observed,
                index,
                &Default::default(),
                cancel,
                deadline,
                |_| self.local.validate(database, &self.intent),
            )
            .map_err(source)?;
        if !self.actual.same_observation(&actual) {
            return Err(refused(
                "Original FILE parent/member changed after Preview • reload explicitly",
            ));
        }
        let seed = FileCommentOriginSeedV75::from_producer(database, &identity, &actual)
            .map_err(source)?;
        let write = seed.write(&self.body).map_err(source)?;
        if seed.draft_key().map_err(source)? != self.intent.draft_key()
            || write.digest().map_err(source)? != self.write.digest().map_err(source)?
        {
            return Err(source(()));
        }
        FileCommentBodyV75::new(self.body.clone())
            .map_err(source)?
            .payload(&actual)
            .map_err(source)?;
        self.local
            .validate(database, &self.intent)
            .map_err(source)?;
        let target = PrWriteTarget::new(&identity, actual.parent().summary()).map_err(source)?;
        if target != self.local.target {
            return Err(source(()));
        }
        let mut journal = PrWriteJournal::open(database).map_err(source)?;
        let permit = journal
            .begin_file_comment_v75(&target, &write, &seed)
            .map_err(source)?;
        let marker = permit.marker();
        let reference = marker.publication_reference();
        let encoded = match encode_draft(&self.body, Some(&reference)) {
            Ok(value) => value,
            Err(_) => {
                return Ok(pending(
                    reference,
                    FileCommentPublicationStateV75::NotStarted,
                ))
            }
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
                    let mut p = pending(reference, FileCommentPublicationStateV75::NotStarted);
                    p.draft = FileDraftRetentionV75::Unknown;
                    return Ok(p);
                }
            };
        if self
            .local
            .owned(database, &self.intent, marker, begun, PrWriteState::Pending)
            .is_err()
            || exact_draft(database, &self.intent, &encoded, cancel, deadline).is_err()
            || phase(cancel, deadline).is_err()
            || ack.as_ref().is_none_or(|a| {
                a.check_current(
                    drafts,
                    database,
                    self.intent.scope(),
                    self.intent.draft_key(),
                    cancel,
                )
                .is_err()
            })
            || journal.claim(&permit, &target, &write).is_err()
        {
            return Ok(pending(
                reference,
                FileCommentPublicationStateV75::NotStarted,
            ));
        }
        let claimed = begun.checked_add(1).ok_or_else(|| source(()))?;
        let mut closed = None;
        let before_write = || {
            original_check(cancel, deadline)?;
            ack.as_ref()
                .ok_or_else(safe_local)?
                .check_current(
                    drafts,
                    database,
                    self.intent.scope(),
                    self.intent.draft_key(),
                    cancel,
                )
                .map_err(|_| safe_local())?;
            session.verify(old.navigation.original(), cancel)?;
            let proof = session.close_with_guard(cancel, || {
                original_check(cancel, deadline)?;
                self.local.owned(
                    database,
                    &self.intent,
                    marker,
                    claimed,
                    PrWriteState::Pending,
                )?;
                exact_draft(database, &self.intent, &encoded, cancel, deadline)?;
                ack.as_ref()
                    .ok_or_else(safe_local)?
                    .check_current(
                        drafts,
                        database,
                        self.intent.scope(),
                        self.intent.draft_key(),
                        cancel,
                    )
                    .map_err(|_| safe_local())?;
                original_check(cancel, deadline)
            })?;
            if proof.identity().host != identity.host
                || proof.identity().account_id != identity.account_id
            {
                return Err(safe_local());
            }
            closed = Some(proof);
            ack.as_ref()
                .ok_or_else(safe_local)?
                .check_current(
                    drafts,
                    database,
                    self.intent.scope(),
                    self.intent.draft_key(),
                    cancel,
                )
                .map_err(|_| safe_local())?;
            original_check(cancel, deadline)
        };
        let result = provider.create_file_comment_guarded_v75(
            &measured,
            &mut captures,
            &self.actual,
            &actual,
            &identity,
            &FileCommentBodyV75::new(self.body.clone()).map_err(source)?,
            cancel,
            deadline,
            before_write,
        );
        let receipt = match result {
            Err(ThreadPublicationFailureV1::NotStarted(_)) => {
                return Ok(pending(
                    reference,
                    FileCommentPublicationStateV75::NotStarted,
                ))
            }
            Ok(receipt)
                if receipt.principal() == actual.principal()
                    && receipt.path() == actual.file().path
                    && receipt.head() == actual.parent().head_sha() =>
            {
                receipt
            }
            Err(ThreadPublicationFailureV1::WriteMayHaveStarted(_)) | Ok(_) => {
                let _ = journal.mark_uncertain(&permit);
                // Terminal retirement; no private read/cleanup may follow.
                let retired = matches!(
                    crate::pr_projection::invalidate_unresolved_discussions(
                        database,
                        &identity,
                        actual.parent().summary(),
                        &reference
                    ),
                    Ok(Some(_))
                );
                let mut p = pending(reference, FileCommentPublicationStateV75::Uncertain);
                p.source_retired = retired;
                return Ok(p);
            }
        };
        let id = receipt.comment().id;
        let effect = PrConfirmedEffect {
            kind: PrEffectKind::Comment,
            resource_id: id.to_string(),
            node_id: Some(receipt.comment().node_id.clone()),
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
            return Ok(FileCommentPublicationV75 {
                state: FileCommentPublicationStateV75::ConfirmedButJournalFailed,
                reference,
                confirmed_id: Some(id),
                source_retired,
                draft: FileDraftRetentionV75::Retained,
            });
        }
        let confirmed_generation = claimed.checked_add(1).ok_or_else(|| source(()))?;
        let mut retention = FileDraftRetentionV75::Retained;
        let visible = closed
            .as_ref()
            .and_then(|c| {
                c.verified()
                    .iter()
                    .find(|row| row.repository().id == actual.parent().repository_id())
            })
            .is_some_and(|current| {
                provider
                    .close_file_comment_visibility_v75(
                        &actual,
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
                            exact_draft(database, &self.intent, &encoded, cancel, deadline)?;
                            let ack = ack.take().ok_or_else(safe_local)?;
                            ack.check_current(
                                drafts,
                                database,
                                self.intent.scope(),
                                self.intent.draft_key(),
                                cancel,
                            )
                            .map_err(|_| safe_local())?;
                            // Same moved envelope checked by one conditional SQL DELETE;
                            // unknown ACK cannot be represented as proof of absence.
                            retention = FileDraftRetentionV75::Unknown;
                            drafts
                                .remove_publication_and_wait_v2(ack, cancel)
                                .map_err(|_| safe_local())?;
                            retention = FileDraftRetentionV75::Removed;
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
            && captures.consumed_requests() == FILE_COMMENT_POSTWRITE_CAPS_V75.len();
        // FINAL operation. Do not reload draft/body/SQL/private source after.
        let source_retired = matches!(
            crate::pr_projection::invalidate_confirmed_discussions(
                database,
                &identity,
                actual.parent().summary(),
                &reference
            ),
            Ok(Some(_))
        );
        let state = if visible && source_retired && phase(cancel, deadline).is_ok() {
            FileCommentPublicationStateV75::ConfirmedSnapshotRetired
        } else {
            FileCommentPublicationStateV75::ConfirmedButSourceUnavailable
        };
        Ok(FileCommentPublicationV75 {
            state,
            reference,
            confirmed_id: Some(id),
            source_retired,
            draft: retention,
        })
    }
}
