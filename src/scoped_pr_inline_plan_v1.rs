//! Worker-only fresh preview and durable single-submit inline publication.
use super::*;
use crate::{
    drafts::DraftClient,
    pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrPublicationReference, PrWriteIntent, PrWriteJournal,
        PrWriteMarker, PrWriteState, PrWriteTarget,
    },
    providers::{
        gh::{
            GithubError, GithubErrorKind, GithubProvider, HostIdentity, InlineComment,
            InlinePublicationFailureV1, PrImmutableFileLimitsV1, PrImmutableFileOriginalV1,
            PrResourceReadLimitsV1, RepositoryCatalogueAccessLimits,
        },
        runner::CancellationToken,
    },
    scoped_pr_observed_tabs_v1::resources::{file_identity, safe_local, ResourceLocalGuard},
    storage::Cache,
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

#[derive(Debug)]
pub struct InlineFailureV1 {
    pub kind: Option<GithubErrorKind>,
    pub hide_private: bool,
    message: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScopedInlinePublicationStateV1 {
    NotStarted,
    Uncertain,
    ConfirmedSnapshotRetired,
    ConfirmedButJournalFailed,
    ConfirmedButSourceUnavailable,
}
/// Safe publication metadata only. Mandatory discussion fencing retires the
/// original snapshot, so no stale receipt body/actor capability is exposed.
pub struct ScopedInlinePublicationV1 {
    state: ScopedInlinePublicationStateV1,
    reference: PrPublicationReference,
    confirmed_comment_id: Option<u64>,
    source_retired: bool,
    hide_private: bool,
    draft_retained: bool,
}
impl std::fmt::Debug for ScopedInlinePublicationV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ScopedInlinePublicationV1")
            .field("state", &self.state)
            .field("hide_private", &self.hide_private)
            .finish_non_exhaustive()
    }
}
impl ScopedInlinePublicationV1 {
    pub fn state(&self) -> ScopedInlinePublicationStateV1 {
        self.state
    }
    pub fn reference(&self) -> &PrPublicationReference {
        &self.reference
    }
    pub fn confirmed_comment_id(&self) -> Option<u64> {
        self.confirmed_comment_id
    }
    /// Actual completion of THIS attempt's discussion fence, not proof that
    /// any current source/account is available when false. Always honor hide_private.
    pub fn source_retired(&self) -> bool {
        self.source_retired
    }
    pub fn hide_private(&self) -> bool {
        self.hide_private
    }
    pub fn draft_retained(&self) -> bool {
        self.draft_retained
    }
}
fn pending(
    reference: PrPublicationReference,
    state: ScopedInlinePublicationStateV1,
) -> ScopedInlinePublicationV1 {
    ScopedInlinePublicationV1 {
        state,
        reference,
        confirmed_comment_id: None,
        source_retired: false,
        hide_private: true,
        draft_retained: true,
    }
}

/// Body-only draft restoration after a fresh original-source closure. The
/// reference is historical metadata, never an approval or repeat-write permit.
pub struct ScopedInlineDraftV1 {
    intent: Arc<ScopedInlineIntentV1>,
    body: Arc<str>,
    publication: Option<PrPublicationReference>,
    identity: HostIdentity,
    draft_key: String,
}
impl std::fmt::Debug for ScopedInlineDraftV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedInlineDraftV1(<closed private draft; no approval>)")
    }
}
impl ScopedInlineDraftV1 {
    pub fn intent(&self) -> &Arc<ScopedInlineIntentV1> {
        &self.intent
    }
    pub fn body(&self) -> &str {
        &self.body
    }
    pub fn publication(&self) -> Option<&PrPublicationReference> {
        self.publication.as_ref()
    }
    /// Honest observed composer account from the actual final closing proof;
    /// neither a restored username nor current write permission.
    pub fn identity(&self) -> &HostIdentity {
        &self.identity
    }
    /// Actual worker-prepared stable anchor key; foreground never hashes or
    /// serializes original source metadata during ordinary editing saves.
    pub fn draft_key(&self) -> &str {
        &self.draft_key
    }

    #[allow(clippy::too_many_arguments)]
    pub fn open(
        intent: Arc<ScopedInlineIntentV1>,
        provider: &GithubProvider,
        database: &Path,
        limits: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        original_deadline: Instant,
    ) -> Result<Self, InlineFailureV1> {
        check(cancel, original_deadline)?;
        intent
            .validate_endpoint()
            .map_err(|_| refused("Original range is not an admitted inline endpoint"))?;
        let provider = provider
            .clone()
            .with_catalogue_read_deadline(original_deadline);
        let mut session = provider
            .begin_repository_catalogue_access(intent.proof.source.source(), access, cancel)
            .map_err(source_failure)?;
        let identity = session.identity().clone();
        let window = session
            .reserve_measured_external_reads(
                2,
                limits.max_document_bytes,
                limits.max_pool_bytes,
                2,
                cancel,
            )
            .map_err(source_failure)?;
        let bounded = provider
            .clone()
            .with_measured_read_window(window)
            .map_err(source_failure)?;
        let current = session
            .verify(intent.proof.navigation.original(), cancel)
            .map_err(source_failure)?;
        let parent = bounded
            .pr_inline_original_parent_v1(
                current,
                &intent.proof.original_view.observed,
                limits,
                cancel,
            )
            .map_err(source_failure)?;
        private_check(cancel, original_deadline).map_err(source_failure)?;
        let local = ResourceLocalGuard::capture(
            database,
            &intent.proof.source,
            &identity,
            parent.summary(),
        )
        .map_err(source_failure)?;
        let key = intent
            .draft_key()
            .map_err(|_| refused("Inline anchor cannot be fingerprinted"))?;
        let encoded = Cache::open(database)
            .map_err(|_| source_failure(safe_local()))?
            .bounded_draft(intent.scope(), &key, 128 * 1024)
            .map_err(|_| source_failure(safe_local()))?;
        let mut body = Arc::<str>::from("");
        let mut publication = None;
        if let Some(encoded) = encoded {
            let value: serde_json::Value = serde_json::from_str(&encoded)
                .map_err(|_| refused("Malformed inline draft; no body or approval restored"))?;
            let object = value
                .as_object()
                .filter(|object| object.len() == 3)
                .ok_or_else(|| refused("Unknown inline draft fields; no values restored"))?;
            if object.get("version").and_then(serde_json::Value::as_u64) != Some(2) {
                return Err(refused(
                    "Unsupported inline draft version; no approval restored",
                ));
            }
            let values = object
                .get("values")
                .and_then(serde_json::Value::as_object)
                .filter(|values| values.len() == 1)
                .ok_or_else(|| refused("Inline draft must contain only body text"))?;
            let text = values
                .get("body")
                .and_then(serde_json::Value::as_str)
                .filter(|body| body.len() <= 65_536)
                .ok_or_else(|| refused("Inline body exceeds its bounded recovery contract"))?;
            publication = serde_json::from_value(
                object
                    .get("publication")
                    .cloned()
                    .ok_or_else(|| refused("Inline draft publication field absent"))?,
            )
            .map_err(|_| refused("Malformed original inline reference; no values restored"))?;
            if let Some(reference) = &publication {
                let marker = PrWriteJournal::open(database)
                    .map_err(|_| source_failure(safe_local()))?
                    .original_publication(&local.target, reference)
                    .map_err(|_| source_failure(safe_local()))?
                    .ok_or_else(|| source_failure(safe_local()))?;
                if marker.command != "inline-comment"
                    || marker.selected_resource != key
                    || marker.state != PrWriteState::Confirmed
                {
                    return Err(source_failure(safe_local()));
                }
            }
            body = Arc::from(text);
        }
        private_check(cancel, original_deadline).map_err(source_failure)?;
        bounded
            .pr_inline_original_parent_v1(
                current,
                &intent.proof.original_view.observed,
                limits,
                cancel,
            )
            .map_err(source_failure)?;
        session
            .verify(intent.proof.navigation.original(), cancel)
            .map_err(source_failure)?;
        let closed = session
            .close_with_guard(cancel, || {
                private_check(cancel, original_deadline)?;
                local.validate(database, &intent.proof.source)?;
                private_check(cancel, original_deadline)
            })
            .map_err(source_failure)?;
        check(cancel, original_deadline)?;
        let result = Self {
            intent,
            body,
            publication,
            identity: closed.identity().clone(),
            draft_key: key,
        };
        check(cancel, original_deadline)?;
        Ok(result)
    }
}
impl std::fmt::Display for InlineFailureV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}
fn refused(message: &'static str) -> InlineFailureV1 {
    InlineFailureV1 {
        kind: None,
        hide_private: false,
        message,
    }
}
fn source_failure(error: GithubError) -> InlineFailureV1 {
    InlineFailureV1 { kind: Some(error.kind), hide_private: true, message: "Original inline source or current account unavailable; private content hidden and no fallback selected" }
}
fn check(cancel: &CancellationToken, deadline: Instant) -> Result<(), InlineFailureV1> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(refused(
            "Original inline operation expired or canceled; no new phase started",
        ));
    }
    Ok(())
}
fn private_check(cancel: &CancellationToken, deadline: Instant) -> Result<(), GithubError> {
    check(cancel, deadline).map_err(|_| GithubError {
        kind: GithubErrorKind::Transport,
        message: "Original inline operation unavailable".into(),
    })
}

fn encoded_draft_fits(intent: &ScopedInlineIntentV1, body: &str) -> bool {
    fn string_bytes(text: &str) -> Option<usize> {
        text.bytes().try_fold(0usize, |bytes, byte| {
            bytes.checked_add(match byte {
                b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 8 | 12 => 2,
                0..=31 => 6,
                _ => 1,
            })
        })
    }
    // Real bounded node/head metadata; the fixed reserve covers JSON keys,
    // punctuation, <=19-digit actual attempt ID and the 64-hex payload digest.
    intent.original_pr_node().len() <= 4096
        && string_bytes(body)
            .and_then(|bytes| bytes.checked_add(string_bytes(intent.original_pr_node())?))
            .and_then(|bytes| bytes.checked_add(intent.review_head().len()))
            .and_then(|bytes| bytes.checked_add(512 + 64))
            .is_some_and(|bytes| bytes <= 128 * 1024)
}

/// Not a permission lease: Confirm has a NEW original queued deadline, fresh
/// original-source guards and a durable single-use producer permit.
pub struct ScopedInlinePreviewV1 {
    intent: Arc<ScopedInlineIntentV1>,
    body: Arc<str>,
    identity: HostIdentity,
    local: ResourceLocalGuard,
    write_intent: PrWriteIntent,
    lines: Arc<[String]>,
    started: AtomicBool,
}
impl std::fmt::Debug for ScopedInlinePreviewV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ScopedInlinePreviewV1(<fresh observed preview; no write permit>)")
    }
}
impl ScopedInlinePreviewV1 {
    #[allow(clippy::too_many_arguments)]
    pub fn prepare(
        intent: Arc<ScopedInlineIntentV1>,
        body: Arc<str>,
        provider: &GithubProvider,
        database: &Path,
        limits: &PrImmutableFileLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        cancel: &CancellationToken,
        original_deadline: Instant,
    ) -> Result<Arc<Self>, InlineFailureV1> {
        check(cancel, original_deadline)?;
        if body.len() > 65_536 || body.trim().is_empty() {
            return Err(refused(
                "Inline comment needs nonempty text within65536 UTF8 bytes",
            ));
        }
        if !encoded_draft_fits(&intent, &body) {
            return Err(refused("Original inline draft exceeds128KiB after JSON encoding; shorten the body. No text was truncated or publication started"));
        }
        intent.validate_endpoint().map_err(|_| refused("Choose an admitted range ending on a deleted LEFT line or added/context RIGHT line; no coordinates were converted"))?;
        let original = PrImmutableFileOriginalV1::from_observed(
            intent.scope(),
            intent.proof.original_view.observed.clone(),
            intent.proof.file_index(),
            limits,
        )
        .map_err(source_failure)?;
        let read_provider = provider
            .clone()
            .with_catalogue_read_deadline(original_deadline);
        let mut session = read_provider
            .begin_repository_catalogue_access(intent.proof.source.source(), access, cancel)
            .map_err(source_failure)?;
        let identity = session.identity().clone();
        let window = session
            .reserve_measured_external_reads(
                original.max_external_requests(),
                1024 * 1024,
                limits.max_pool_bytes,
                2,
                cancel,
            )
            .map_err(source_failure)?;
        let current = session
            .verify(intent.proof.navigation.original(), cancel)
            .map_err(source_failure)?;
        let bounded = read_provider
            .clone()
            .with_measured_read_window(window)
            .map_err(source_failure)?;
        let mut local = None;
        let observed = bounded
            .pr_immutable_file_images_v1(current, &identity, &original, limits, cancel, |parent| {
                private_check(cancel, original_deadline)?;
                local = Some(ResourceLocalGuard::capture(
                    database,
                    &intent.proof.source,
                    &identity,
                    parent.summary(),
                )?);
                private_check(cancel, original_deadline)
            })
            .map_err(source_failure)?;
        if observed.patch_digest() != intent.proof.observed().patch_digest()
            || observed.merge_base() != intent.merge_base()
            || !observed.range_admitted(intent.side, intent.first, intent.last)
            || [PrImageSideV1::Old, PrImageSideV1::New]
                .into_iter()
                .any(|side| {
                    let original = intent.proof.observed().image(side);
                    let current = observed.image(side);
                    current.path() != original.path()
                        || current.blob_sha() != original.blob_sha()
                        || current.mode() != original.mode()
                        || current.kind() != original.kind()
                        || current.absent() != original.absent()
                })
        {
            return Err(refused("Original immutable inline selection changed; retain the draft and explicitly re-anchor"));
        }
        let closing = session
            .verify(intent.proof.navigation.original(), cancel)
            .map_err(source_failure)?;
        if closing.repository().full_name != observed.parent().repository()
            || closing.repository().permissions.pull != Some(true)
        {
            return Err(source_failure(safe_local()));
        }
        let local = local.ok_or_else(|| source_failure(safe_local()))?;
        let closed = session
            .close_with_guard(cancel, || {
                private_check(cancel, original_deadline)?;
                local.validate(database, &intent.proof.source)?;
                private_check(cancel, original_deadline)
            })
            .map_err(source_failure)?;
        let identity = closed.identity().clone();
        let mut values = BTreeMap::new();
        values.insert("body".into(), body.to_string());
        values.insert(
            "repository-id".into(),
            intent.original_repository_id().to_string(),
        );
        values.insert("pr-id".into(), intent.original_pr_id().to_string());
        values.insert("base".into(), observed.parent().base_tip_sha().into());
        values.insert("merge-base".into(), intent.merge_base().into());
        values.insert("path".into(), intent.server_path().into());
        values.insert(
            "image-path-hex".into(),
            intent
                .selected_image_path()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        );
        values.insert(
            "side".into(),
            if intent.side == PrImageSideV1::Old {
                "LEFT"
            } else {
                "RIGHT"
            }
            .into(),
        );
        values.insert("start".into(), intent.first.to_string());
        values.insert("end".into(), intent.last.to_string());
        values.insert("patch".into(), observed.patch_digest().into());
        let write_intent = PrWriteIntent {
            command: "inline-comment".into(),
            selected_resource: intent
                .draft_key()
                .map_err(|_| refused("Inline anchor cannot be fingerprinted"))?,
            observed_head: intent.review_head().into(),
            values,
        };
        write_intent
            .digest()
            .map_err(|_| refused("Inline publication fingerprint exceeds its bounded contract"))?;
        let lines = vec![
            format!("Publish inline comment as @{} • numeric account {}", identity.login, identity.account_id.unwrap_or(0)),
            format!("{} PR #{} • repository ID {} / PR ID {}", observed.parent().repository(), observed.parent().number(), intent.original_repository_id(), intent.original_pr_id()),
            format!("Canonical API filename: {}", crate::git::display_bytes(intent.server_path().as_bytes())),
            format!("Selected image path: {}", crate::git::display_bytes(intent.selected_image_path())),
            format!("{} lines {}–{} • commit_id H {}", if intent.side == PrImageSideV1::Old { "LEFT" } else { "RIGHT" }, intent.first, intent.last, intent.review_head()),
            format!("Old content M {} • {} body bytes", intent.merge_base(), body.len()),
            "Fresh observed preview only. Explicit Confirm rechecks source/account/head/storage before exactly one publication; no automatic retry.".into(),
        ];
        check(cancel, original_deadline)?;
        let result = Arc::new(Self {
            intent,
            body,
            identity,
            local,
            write_intent,
            lines: lines.into(),
            started: AtomicBool::new(false),
        });
        check(cancel, original_deadline)?;
        Ok(result)
    }
    pub fn intent(&self) -> &Arc<ScopedInlineIntentV1> {
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
    pub fn draft_key(&self) -> &str {
        &self.write_intent.selected_resource
    }

    /// Only explicit Confirm can call this. Queueing captures a new absolute
    /// deadline; neither this preview nor a restored draft is a write lease.
    #[allow(clippy::too_many_arguments)]
    pub fn execute(
        &self,
        provider: &GithubProvider,
        database: &Path,
        limits: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        drafts: &DraftClient,
        confirmed: bool,
        restored: Option<&PrPublicationReference>,
        cancel: &CancellationToken,
        original_deadline: Instant,
    ) -> Result<ScopedInlinePublicationV1, InlineFailureV1> {
        check(cancel, original_deadline)?;
        if !confirmed || restored.is_some() {
            return Err(refused("Explicit fresh confirmation is required; restored publication metadata is history, never permission to resend"));
        }
        if self
            .started
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(refused("This inline preview already started an attempt; inspect its durable outcome instead of resending"));
        }
        limits.validate().map_err(source_failure)?;
        let phase_provider = provider
            .clone()
            .with_catalogue_read_deadline(original_deadline);
        let mut session = phase_provider
            .begin_repository_catalogue_access(self.intent.proof.source.source(), access, cancel)
            .map_err(source_failure)?;
        let identity = session.identity().clone();
        // Login is observed presentation, not the numeric account boundary.
        if identity.host != self.identity.host
            || identity.account_id.is_none_or(|id| id == 0)
            || identity.account_id != self.identity.account_id
        {
            return Err(source_failure(safe_local()));
        }
        let window = session
            .reserve_measured_external_reads(
                2,
                limits.max_document_bytes,
                limits.max_pool_bytes,
                2,
                cancel,
            )
            .map_err(source_failure)?;
        let bounded = phase_provider
            .clone()
            .with_measured_read_window(window)
            .map_err(source_failure)?;
        let current = session
            .verify(self.intent.proof.navigation.original(), cancel)
            .map_err(source_failure)?;
        let parent = bounded
            .pr_inline_original_parent_v1(
                current,
                &self.intent.proof.original_view.observed,
                limits,
                cancel,
            )
            .map_err(source_failure)?;
        private_check(cancel, original_deadline).map_err(source_failure)?;
        self.local
            .validate(database, &self.intent.proof.source)
            .map_err(source_failure)?;
        let target = PrWriteTarget::new(&identity, parent.summary())
            .map_err(|_| source_failure(safe_local()))?;
        if target != self.local.target {
            return Err(source_failure(safe_local()));
        }
        let mut journal =
            PrWriteJournal::open(database).map_err(|_| source_failure(safe_local()))?;
        private_check(cancel, original_deadline).map_err(source_failure)?;
        let permit = journal
            .begin(&target, &self.write_intent)
            .map_err(|_| source_failure(safe_local()))?;
        let marker = permit.marker();
        let reference = marker.publication_reference();
        // The same ordered writer used by typing persists the exact reference
        // before any invocation. A failed save leaves the pending blocker.
        let draft = serde_json::json!({"version":2,"values":{"body":self.body.as_ref()},"publication":reference}).to_string();
        let saved = draft.len() <= 128 * 1024
            && check(cancel, original_deadline).is_ok()
            && drafts
                .save_and_wait(self.intent.scope().clone(), self.draft_key().into(), draft)
                .is_ok();
        if !saved {
            let mut result = pending(reference, ScopedInlinePublicationStateV1::NotStarted);
            result.draft_retained = false;
            return Ok(result);
        }
        if check(cancel, original_deadline).is_err()
            || journal.claim(&permit, &target, &self.write_intent).is_err()
        {
            return Ok(pending(
                reference,
                ScopedInlinePublicationStateV1::NotStarted,
            ));
        }
        let claimed_generation = self
            .local
            .generation
            .checked_add(2)
            .ok_or_else(|| source_failure(safe_local()))?;
        let comment = InlineComment {
            path: self.intent.server_path().into(),
            side: self.intent.side(),
            line: u64::from(self.intent.end()),
            start_line: (self.intent.start() != self.intent.end())
                .then_some(u64::from(self.intent.start())),
            start_side: (self.intent.start() != self.intent.end()).then_some(self.intent.side()),
            body: self.body.to_string(),
        };
        // The provider's ordinary bounded WRITE transport cannot carry this
        // measured READ window. Close it fully in the final pre-POST callback.
        let result = phase_provider.create_inline_comment_guarded_v1(
            &identity,
            parent.summary(),
            &comment,
            cancel,
            original_deadline,
            || {
                private_check(cancel, original_deadline)?;
                let closing_current = session
                    .observation_v1(self.intent.original_repository_id())
                    .ok_or_else(safe_local)?;
                bounded.pr_inline_original_parent_v1(
                    closing_current,
                    &self.intent.proof.original_view.observed,
                    limits,
                    cancel,
                )?;
                session.verify(self.intent.proof.navigation.original(), cancel)?;
                session.close_with_guard(cancel, || {
                    private_check(cancel, original_deadline)?;
                    validate_owned(
                        &self.local,
                        database,
                        &self.intent,
                        marker,
                        claimed_generation,
                        PrWriteState::Pending,
                    )?;
                    private_check(cancel, original_deadline)
                })?;
                private_check(cancel, original_deadline)
            },
        );
        let receipt = match result {
            Err(InlinePublicationFailureV1::NotStarted(_)) => {
                return Ok(pending(
                    reference,
                    ScopedInlinePublicationStateV1::NotStarted,
                ));
            }
            Err(InlinePublicationFailureV1::WriteMayHaveStarted(_)) => {
                let _ = journal.mark_uncertain(&permit);
                let source_retired = matches!(
                    crate::pr_projection::invalidate_unresolved_discussions(
                        database,
                        &identity,
                        parent.summary(),
                        &reference,
                    ),
                    Ok(Some(_))
                );
                let mut result = pending(reference, ScopedInlinePublicationStateV1::Uncertain);
                result.source_retired = source_retired;
                return Ok(result);
            }
            Ok(receipt) => receipt,
        };
        let effect = PrConfirmedEffect {
            kind: PrEffectKind::Comment,
            resource_id: receipt.comment().id.to_string(),
            node_id: Some(receipt.comment().node_id.clone()),
        };
        if journal.mark_confirmed(&permit, &effect).is_err() {
            // Actual receipt exists, but no fabricated durable confirmation or
            // retry permission is returned when its journal persistence failed.
            let confirmed_comment_id = Some(receipt.comment().id);
            drop(receipt);
            let source_retired = matches!(
                crate::pr_projection::invalidate_unresolved_discussions(
                    database,
                    &identity,
                    parent.summary(),
                    &reference
                ),
                Ok(Some(_))
            );
            return Ok(ScopedInlinePublicationV1 {
                state: ScopedInlinePublicationStateV1::ConfirmedButJournalFailed,
                reference,
                confirmed_comment_id,
                source_retired,
                hide_private: true,
                draft_retained: true,
            });
        }
        let confirmed_generation = claimed_generation
            .checked_add(1)
            .ok_or_else(|| source_failure(safe_local()))?;
        let mut draft_retained = true;
        let visible = self
            .close_result_visibility(
                &phase_provider,
                database,
                limits,
                access,
                drafts,
                marker,
                confirmed_generation,
                cancel,
                original_deadline,
                &mut draft_retained,
            )
            .is_ok()
            && check(cancel, original_deadline).is_ok();
        let confirmed_comment_id = Some(receipt.comment().id);
        drop(receipt);
        // This is a terminal local retirement, NOT a rebased source epoch.
        // No private source reads or cleanup may follow the mandatory fence.
        let source_retired = matches!(
            crate::pr_projection::invalidate_confirmed_discussions(
                database,
                &identity,
                parent.summary(),
                &reference
            ),
            Ok(Some(_))
        );
        let state = if visible && source_retired && check(cancel, original_deadline).is_ok() {
            ScopedInlinePublicationStateV1::ConfirmedSnapshotRetired
        } else {
            ScopedInlinePublicationStateV1::ConfirmedButSourceUnavailable
        };
        Ok(ScopedInlinePublicationV1 {
            state,
            reference,
            confirmed_comment_id,
            source_retired,
            hide_private: true,
            draft_retained,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn close_result_visibility(
        &self,
        provider: &GithubProvider,
        database: &Path,
        limits: &PrResourceReadLimitsV1,
        access: &RepositoryCatalogueAccessLimits,
        drafts: &DraftClient,
        marker: &PrWriteMarker,
        generation: i64,
        cancel: &CancellationToken,
        deadline: Instant,
        draft_retained: &mut bool,
    ) -> Result<(), GithubError> {
        private_check(cancel, deadline)?;
        let mut session = provider.begin_repository_catalogue_access(
            self.intent.proof.source.source(),
            access,
            cancel,
        )?;
        let window = session.reserve_measured_external_reads(
            1,
            limits.max_document_bytes,
            limits.max_pool_bytes,
            2,
            cancel,
        )?;
        let bounded = provider.clone().with_measured_read_window(window)?;
        let current = session.verify(self.intent.proof.navigation.original(), cancel)?;
        bounded.pr_inline_original_parent_v1(
            current,
            &self.intent.proof.original_view.observed,
            limits,
            cancel,
        )?;
        session.verify(self.intent.proof.navigation.original(), cancel)?;
        session.close_with_guard(cancel, || {
            private_check(cancel, deadline)?;
            validate_owned(
                &self.local,
                database,
                &self.intent,
                marker,
                generation,
                PrWriteState::Confirmed,
            )?;
            *draft_retained = drafts
                .remove(self.intent.scope().clone(), self.draft_key().into())
                .is_err();
            private_check(cancel, deadline)?;
            validate_owned(
                &self.local,
                database,
                &self.intent,
                marker,
                generation,
                PrWriteState::Confirmed,
            )?;
            private_check(cancel, deadline)
        })?;
        private_check(cancel, deadline)?;
        Ok(())
    }
}

fn validate_owned(
    local: &ResourceLocalGuard,
    database: &Path,
    intent: &ScopedInlineIntentV1,
    marker: &PrWriteMarker,
    generation: i64,
    state: PrWriteState,
) -> Result<(), GithubError> {
    if std::fs::canonicalize(database).map_err(|_| safe_local())? != local.path
        || file_identity(database)? != local.file
    {
        return Err(safe_local());
    }
    Cache::open(database)
        .map_err(|_| safe_local())?
        .validate_repository_catalogue_intent_source(&intent.proof.source)
        .map_err(|_| safe_local())?;
    let journal = PrWriteJournal::open(database).map_err(|_| safe_local())?;
    if journal
        .source_generation(&local.target)
        .map_err(|_| safe_local())?
        != generation
    {
        return Err(safe_local());
    }
    let actual = journal
        .original_publication(&local.target, &marker.publication_reference())
        .map_err(|_| safe_local())?
        .ok_or_else(safe_local)?;
    if actual.state != state
        || actual.target != marker.target
        || actual.command != marker.command
        || actual.selected_resource != marker.selected_resource
        || actual.payload_sha256 != marker.payload_sha256
        || actual.observed_head != marker.observed_head
    {
        return Err(safe_local());
    }
    let blocking = journal
        .source_blocking(&local.target)
        .map_err(|_| safe_local())?;
    if match state {
        PrWriteState::Pending => blocking.is_none_or(|blocking| {
            blocking.publication_reference() != marker.publication_reference()
                || blocking.state != PrWriteState::Pending
        }),
        PrWriteState::Confirmed => blocking.is_some(),
        _ => true,
    } {
        return Err(safe_local());
    }
    Ok(())
}
