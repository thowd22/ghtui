//! Worker boundary for deliberate repository-set intent. The
//! reservation is a local invalidation fence, never GitHub access authority.
use super::{Cache, Scope};
use crate::{
    providers::{
        gh::{RepositoryCatalogueContext, RepositoryCatalogueRead},
        runner::CancellationToken,
    },
    repository_catalogue_intent_v1::{CatalogueIntentV1, IntentSource, INTENT_MAX_BYTES},
};
use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::{fmt, os::unix::fs::MetadataExt, path::PathBuf, sync::Arc, time::Instant};
#[path = "repository_catalogue_intents_v1/conversation_continuation_v19.rs"]
pub(super) mod conversation_continuation_v19;

#[path = "repository_catalogue_intents_v1/local_clear_v5.rs"]
mod local_clear_v5;
#[path = "repository_catalogue_intents_v1/state_source_v80.rs"]
pub(super) mod state_source_v80;

/// Safe, closed diagnostics. SQL/JSON/provider payloads are never interpolated.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CatalogueIntentBoundaryError {
    Database,
    OriginalSource,
    SavedIntentChanged,
    SavedIntentInvalid,
    CandidateInvalid,
    CancelledOrExpired,
}
impl fmt::Display for CatalogueIntentBoundaryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Database => "Repository intent database boundary unavailable; no payload disclosed",
            Self::OriginalSource => "Original catalogue scope/source reservation changed; refresh explicitly, do not rebase",
            Self::SavedIntentChanged => "Original saved intent changed; no concurrent user edit overwritten",
            Self::SavedIntentInvalid => "Original saved intent is invalid or exceeds its 128KiB bound; no default restored",
            Self::CandidateInvalid => "Saved selection must retain bounded genuine original repository IDs and nodes",
            Self::CancelledOrExpired => "Original intent cancellation/deadline reached; no automatic resend",
        })
    }
}
impl std::error::Error for CatalogueIntentBoundaryError {}
type BoundaryResult<T> = std::result::Result<T, CatalogueIntentBoundaryError>;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Issuer {
    path: PathBuf,
    device: u64,
    inode: u64,
}
impl Issuer {
    pub(super) fn retained_bytes_v80(&self) -> Option<usize> {
        std::mem::size_of::<Self>().checked_add(self.path.capacity())
    }
    pub(super) fn fingerprint_v63(&self) -> String {
        use std::os::unix::ffi::OsStrExt;
        let mut digest = Sha256::new();
        let path = self.path.as_os_str().as_bytes();
        digest.update((path.len() as u64).to_be_bytes());
        digest.update(path);
        digest.update(self.device.to_be_bytes());
        digest.update(self.inode.to_be_bytes());
        format!("{:x}", digest.finalize())
    }
    /// Reuse exact existing path/device/inode/regular-single-link policy while
    /// another cache child holds the connection transaction borrow.
    pub(super) fn file_still_matches_v63(&self) -> bool {
        file_issuer(&self.path).is_ok_and(|current| current == *self)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct PayloadEpoch {
    bytes: usize,
    hash: [u8; 32],
}
type Checkpoint = Option<(u64, u64, bool)>;

/// Constructed only by a real cache transaction during the genuine factory's
/// BeforeEnumeration callback. No public constructor, Clone, or serde.
pub struct CatalogueIntentReservation {
    issuer: Issuer,
    scope: Scope,
    original: IntentSource,
    sync_source: String,
    view_key: String,
    generation: u64,
    checkpoint: Checkpoint,
    expected_payload: Option<PayloadEpoch>,
    intent: CatalogueIntentV1,
}
impl fmt::Debug for CatalogueIntentReservation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CatalogueIntentReservation(<original local fence; not permission>)")
    }
}
impl CatalogueIntentReservation {
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn sync_source(&self) -> &str {
        &self.sync_source
    }
    pub fn saved_view_key(&self) -> &str {
        &self.view_key
    }
    pub(crate) fn matches_context(
        &self,
        context: &RepositoryCatalogueContext,
    ) -> BoundaryResult<()> {
        let intent = CatalogueIntentV1::new(context)
            .map_err(|_| CatalogueIntentBoundaryError::OriginalSource)?;
        if intent.scope() != &self.scope || intent.source() != &self.original {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        Ok(())
    }
    /// PURE bind after the actual factory's final account proof. No SQL, no
    /// new generation and no renewed remote freshness are introduced here.
    pub(crate) fn bind(
        self,
        source: Arc<RepositoryCatalogueRead>,
    ) -> BoundaryResult<CatalogueIntentSource> {
        self.matches_context(source.context())?;
        self.intent
            .validate_original_read(&source)
            .map_err(|_| CatalogueIntentBoundaryError::OriginalSource)?;
        Ok(CatalogueIntentSource {
            reservation: self,
            source,
        })
    }
}

/// Genuine original factory read plus its original local SQL fence. Restored
/// intent alone cannot construct this object or grant repository access.
pub struct CatalogueIntentSource {
    reservation: CatalogueIntentReservation,
    source: Arc<RepositoryCatalogueRead>,
}
impl fmt::Debug for CatalogueIntentSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CatalogueIntentSource(<original observed intent source>)")
    }
}
impl CatalogueIntentSource {
    pub(in crate::storage) fn validate_cleanup_issuer_v9(
        &self,
        cache: &Cache,
    ) -> BoundaryResult<()> {
        if issuer(cache)? != self.reservation.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        Ok(())
    }
    pub(in crate::storage) fn validate_cleanup_snapshot_v9(
        &self,
        tx: &Transaction<'_>,
    ) -> BoundaryResult<()> {
        if !self.reservation.issuer.file_still_matches_v63() {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        validate_epoch(tx, &self.reservation)?;
        validate_payload(tx, &self.reservation)
    }
    pub(crate) fn workflow_owned_bytes_v9(&self) -> Option<usize> {
        std::mem::size_of::<Self>()
            .checked_add(2 * std::mem::size_of::<usize>())?
            .checked_add(self.reservation.issuer.path.capacity())?
            .checked_add(self.reservation.scope.host.capacity())?
            .checked_add(self.reservation.scope.login.capacity())?
            .checked_add(self.reservation.sync_source.capacity())?
            .checked_add(self.reservation.view_key.capacity())?
            // The reservation keeps an independent original source and intent.
            .checked_add(self.reservation.intent.workflow_owned_bytes_v9()?)?
            .checked_add(self.source.workflow_owned_bytes_v9()?)?
            .checked_add(self.reservation.original.workflow_owned_bytes_v9()?)
    }
    pub fn source(&self) -> &Arc<RepositoryCatalogueRead> {
        &self.source
    }
    pub fn intent(&self) -> &CatalogueIntentV1 {
        &self.reservation.intent
    }
    pub fn scope(&self) -> &Scope {
        &self.reservation.scope
    }
    pub fn saved_view_key(&self) -> &str {
        &self.reservation.view_key
    }
    pub fn sync_source(&self) -> &str {
        &self.reservation.sync_source
    }
    /// Convert the already committed candidate into the next ORIGINAL CAS
    /// expectation. This is pure metadata, not renewed source/access authority.
    pub(crate) fn advance(&self, committed: CommittedCatalogueIntent) -> Self {
        Self {
            source: self.source.clone(),
            reservation: CatalogueIntentReservation {
                issuer: self.reservation.issuer.clone(),
                scope: self.scope().clone(),
                original: self.reservation.original.clone(),
                sync_source: self.sync_source().into(),
                view_key: self.saved_view_key().into(),
                generation: self.reservation.generation,
                checkpoint: self.reservation.checkpoint,
                expected_payload: Some(committed.epoch),
                intent: committed.intent,
            },
        }
    }
}
pub(crate) struct CommittedCatalogueIntent {
    epoch: PayloadEpoch,
    intent: CatalogueIntentV1,
}

impl Cache {
    /// Validate the ORIGINAL issuer/epoch/payload before a closing access proof;
    /// never reserve a new source or silently rebase its expected payload.
    pub fn validate_repository_catalogue_intent_source(
        &self,
        original: &CatalogueIntentSource,
    ) -> BoundaryResult<()> {
        original
            .reservation
            .matches_context(original.source.context())?;
        self.validate_repository_catalogue_reservation(&original.reservation)
    }
    pub(crate) fn reserve_repository_catalogue_intent(
        &mut self,
        context: &RepositoryCatalogueContext,
    ) -> BoundaryResult<CatalogueIntentReservation> {
        self.reserve_repository_catalogue_intent_inner(context, None)
    }

    /// Same Immediate transaction: validate the ORIGINAL fence and saved bytes
    /// before deliberately replacing its reservation. This is not access.
    pub(crate) fn reserve_repository_catalogue_reread(
        &mut self,
        original: &CatalogueIntentSource,
        context: &RepositoryCatalogueContext,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> BoundaryResult<CatalogueIntentReservation> {
        self.reserve_repository_catalogue_intent_inner(context, Some((original, deadline, cancel)))
    }
    fn reserve_repository_catalogue_intent_inner(
        &mut self,
        context: &RepositoryCatalogueContext,
        reread: Option<(&CatalogueIntentSource, Instant, &CancellationToken)>,
    ) -> BoundaryResult<CatalogueIntentReservation> {
        check_reread_clock(reread)?;
        let mut intent = CatalogueIntentV1::new(context)
            .map_err(|_| CatalogueIntentBoundaryError::OriginalSource)?;
        let scope = intent.scope().clone();
        let original = intent.source().clone();
        // Closed, bounded source metadata only. Host/principal are separate PK
        // fields; opaque nodes remain case-sensitive and cannot become IDs.
        let serialized = serde_json::to_vec(&original)
            .map_err(|_| CatalogueIntentBoundaryError::OriginalSource)?;
        let suffix = format!("{:x}", Sha256::digest(&serialized));
        let sync_source = format!("repository-catalogue-source-v1:{suffix}");
        let view_key = format!("repository-catalogue-intent-v1:{suffix}");
        let original_issuer = issuer(self)?;
        if let Some((parent, _, _)) = reread {
            parent.reservation.matches_context(context)?;
            parent
                .intent()
                .validate_original_read(parent.source())
                .map_err(|_| CatalogueIntentBoundaryError::OriginalSource)?;
            if parent.reservation.issuer != original_issuer
                || parent.reservation.scope != scope
                || parent.reservation.original != original
                || parent.reservation.sync_source != sync_source
                || parent.reservation.view_key != view_key
            {
                return Err(CatalogueIntentBoundaryError::OriginalSource);
            }
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| CatalogueIntentBoundaryError::Database)?;
        check_reread_clock(reread)?;
        if let Some((parent, _, _)) = reread {
            validate_epoch(&transaction, &parent.reservation)?;
            validate_payload(&transaction, &parent.reservation)?;
        }
        let checkpoint = checkpoint(&transaction, &scope, &sync_source)?;
        let previous: Option<u64> = transaction
            .query_row(
                "SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",
                params![scope.host, scope.login, sync_source],
                |row| row.get(0),
            )
            .optional()
            .map_err(|_| CatalogueIntentBoundaryError::Database)?;
        let generation = previous
            .unwrap_or(0)
            .max(checkpoint.map_or(0, |c| c.0))
            .checked_add(1)
            .filter(|value| *value <= i64::MAX as u64)
            .ok_or(CatalogueIntentBoundaryError::OriginalSource)?;
        let payload = bounded_payload(&transaction, &scope, &view_key)?;
        let expected_payload = payload.as_deref().map(payload_epoch);
        if let Some(payload) = payload {
            let value = serde_json::from_str(&payload)
                .map_err(|_| CatalogueIntentBoundaryError::SavedIntentInvalid)?;
            intent = CatalogueIntentV1::from_value(&value, &scope, &original)
                .map_err(|_| CatalogueIntentBoundaryError::SavedIntentInvalid)?;
        }
        transaction.execute(
            "INSERT INTO sync_reservations VALUES(?1,?2,?3,?4,1) ON CONFLICT(host,login,source) DO UPDATE SET generation=excluded.generation,active=1",
            params![scope.host, scope.login, sync_source, generation],
        ).map_err(|_| CatalogueIntentBoundaryError::Database)?;
        if file_issuer(&original_issuer.path)? != original_issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        check_reread_clock(reread)?;
        transaction
            .commit()
            .map_err(|_| CatalogueIntentBoundaryError::Database)?;
        Ok(CatalogueIntentReservation {
            issuer: original_issuer,
            scope,
            original,
            sync_source,
            view_key,
            generation,
            checkpoint,
            expected_payload,
            intent,
        })
    }

    /// Call before catalogue FINAL gh proof, never after it. Exact bounded
    /// metadata queries only; not a saved_views or source-document scan.
    pub(crate) fn validate_repository_catalogue_reservation(
        &self,
        original: &CatalogueIntentReservation,
    ) -> BoundaryResult<()> {
        if issuer(self)? != original.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        let transaction = self
            .connection
            .unchecked_transaction()
            .map_err(|_| CatalogueIntentBoundaryError::Database)?;
        validate_epoch(&transaction, original)?;
        validate_payload(&transaction, original)?;
        if file_issuer(&original.issuer.path)? != original.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        transaction
            .commit()
            .map_err(|_| CatalogueIntentBoundaryError::Database)
    }

    /// The active original scope/source/checkpoint and original exact saved
    /// value are checked UNDER THE SAME write lock as the bounded UPSERT.
    /// Only an explicit fresh-principal pre/post worker may call this seam.
    pub(crate) fn commit_repository_catalogue_intent(
        &mut self,
        original: &CatalogueIntentSource,
        candidate: CatalogueIntentV1,
        deadline: Instant,
        cancel: &CancellationToken,
    ) -> BoundaryResult<CommittedCatalogueIntent> {
        live(deadline, cancel)?;
        validate_candidate(original, &candidate)?;
        let value = candidate
            .to_value()
            .map_err(|_| CatalogueIntentBoundaryError::CandidateInvalid)?;
        let payload = serde_json::to_string(&value)
            .map_err(|_| CatalogueIntentBoundaryError::CandidateInvalid)?;
        if payload.len() > INTENT_MAX_BYTES {
            return Err(CatalogueIntentBoundaryError::CandidateInvalid);
        }
        let original_reservation = &original.reservation;
        if issuer(self)? != original_reservation.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        live(deadline, cancel)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|_| CatalogueIntentBoundaryError::Database)?;
        live(deadline, cancel)?; // Busy wait consumes the ORIGINAL deadline.
        validate_epoch(&transaction, original_reservation)?;
        validate_payload(&transaction, original_reservation)?;
        if file_issuer(&original_reservation.issuer.path)? != original_reservation.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        transaction.execute(
            "INSERT INTO saved_views(host,login,name,payload) VALUES(?1,?2,?3,?4) ON CONFLICT(host,login,name) DO UPDATE SET payload=excluded.payload",
            params![original.scope().host, original.scope().login, original.saved_view_key(), payload],
        ).map_err(|_| CatalogueIntentBoundaryError::Database)?;
        live(deadline, cancel)?;
        validate_epoch(&transaction, original_reservation)?;
        if bounded_payload(&transaction, original.scope(), original.saved_view_key())?
            .as_deref()
            .map(payload_epoch)
            != Some(payload_epoch(&payload))
        {
            return Err(CatalogueIntentBoundaryError::SavedIntentChanged);
        }
        if file_issuer(&original_reservation.issuer.path)? != original_reservation.issuer {
            return Err(CatalogueIntentBoundaryError::OriginalSource);
        }
        transaction
            .commit()
            .map_err(|_| CatalogueIntentBoundaryError::Database)?;
        Ok(CommittedCatalogueIntent {
            epoch: payload_epoch(&payload),
            intent: candidate,
        })
    }
}

fn validate_epoch(
    transaction: &Transaction<'_>,
    original: &CatalogueIntentReservation,
) -> BoundaryResult<()> {
    let generation: Option<u64> = transaction.query_row(
        "SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3 AND active=1",
        params![original.scope.host, original.scope.login, original.sync_source], |row| row.get(0),
    ).optional().map_err(|_| CatalogueIntentBoundaryError::Database)?;
    if generation != Some(original.generation)
        || checkpoint(transaction, &original.scope, &original.sync_source)? != original.checkpoint
    {
        return Err(CatalogueIntentBoundaryError::OriginalSource);
    }
    Ok(())
}

fn check_reread_clock(
    reread: Option<(&CatalogueIntentSource, Instant, &CancellationToken)>,
) -> BoundaryResult<()> {
    if reread
        .is_some_and(|(_, deadline, cancel)| cancel.is_cancelled() || Instant::now() >= deadline)
    {
        Err(CatalogueIntentBoundaryError::CancelledOrExpired)
    } else {
        Ok(())
    }
}
fn checkpoint(
    transaction: &Transaction<'_>,
    scope: &Scope,
    source: &str,
) -> BoundaryResult<Checkpoint> {
    transaction.query_row(
        "SELECT generation,fetched_at,complete FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
        params![scope.host, scope.login, source], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
    ).optional().map_err(|_| CatalogueIntentBoundaryError::Database)
}
fn validate_payload(
    transaction: &Transaction<'_>,
    original: &CatalogueIntentReservation,
) -> BoundaryResult<()> {
    let current = bounded_payload(transaction, &original.scope, &original.view_key)?;
    if current.as_deref().map(payload_epoch) != original.expected_payload {
        return Err(CatalogueIntentBoundaryError::SavedIntentChanged);
    }
    Ok(())
}
fn bounded_payload(
    transaction: &Transaction<'_>,
    scope: &Scope,
    name: &str,
) -> BoundaryResult<Option<String>> {
    let row: Option<(i64, Option<String>)> = transaction.query_row(
        "SELECT length(CAST(payload AS BLOB)),CASE WHEN length(CAST(payload AS BLOB))<=?4 THEN payload ELSE NULL END FROM saved_views WHERE host=?1 AND login=?2 AND name=?3",
        params![scope.host, scope.login, name, INTENT_MAX_BYTES], |row| Ok((row.get(0)?, row.get(1)?)),
    ).optional().map_err(|_| CatalogueIntentBoundaryError::Database)?;
    match row {
        None => Ok(None),
        Some((bytes, Some(value)))
            if bytes >= 0
                && bytes as u64 <= INTENT_MAX_BYTES as u64
                && value.len() == bytes as usize =>
        {
            Ok(Some(value))
        }
        _ => Err(CatalogueIntentBoundaryError::SavedIntentInvalid),
    }
}
fn payload_epoch(value: &str) -> PayloadEpoch {
    PayloadEpoch {
        bytes: value.len(),
        hash: Sha256::digest(value.as_bytes()).into(),
    }
}
pub(super) fn issuer(cache: &Cache) -> BoundaryResult<Issuer> {
    let path = cache
        .connection
        .path()
        .filter(|path| !path.is_empty())
        .ok_or(CatalogueIntentBoundaryError::Database)?;
    file_issuer(&std::fs::canonicalize(path).map_err(|_| CatalogueIntentBoundaryError::Database)?)
}
fn file_issuer(path: &std::path::Path) -> BoundaryResult<Issuer> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| CatalogueIntentBoundaryError::Database)?;
    if !metadata.is_file() || metadata.nlink() != 1 {
        return Err(CatalogueIntentBoundaryError::OriginalSource);
    }
    Ok(Issuer {
        path: path.to_owned(),
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}
fn live(deadline: Instant, cancel: &CancellationToken) -> BoundaryResult<()> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        return Err(CatalogueIntentBoundaryError::CancelledOrExpired);
    }
    Ok(())
}

fn validate_candidate(
    original: &CatalogueIntentSource,
    candidate: &CatalogueIntentV1,
) -> BoundaryResult<()> {
    candidate
        .validate_original_read(original.source())
        .map_err(|_| CatalogueIntentBoundaryError::CandidateInvalid)?;
    if candidate.scope() != original.scope() || candidate.source() != &original.reservation.original
    {
        return Err(CatalogueIntentBoundaryError::CandidateInvalid);
    }
    // Retain UNCHANGED unresolved user intent. Every newly inserted/changed
    // reference must exactly match a genuine member of THIS original source.
    let mut changes = std::collections::BTreeMap::<
        u64,
        Vec<&crate::repository_catalogue_intent_v1::IntentReference>,
    >::new();
    let previous = original.intent();
    for (id, reference) in candidate.pins() {
        if previous.pins().get(id) != Some(reference) {
            changes.entry(*id).or_default().push(reference);
        }
    }
    for (name, members) in candidate.sets() {
        for (id, reference) in members {
            if previous
                .sets()
                .get(name)
                .and_then(|members| members.get(id))
                != Some(reference)
            {
                changes.entry(*id).or_default().push(reference);
            }
        }
    }
    for repository in original.source().repositories() {
        if let Some(references) = changes.remove(&repository.id) {
            if references.iter().any(|reference| {
                reference.node_id != repository.node_id
                    || reference.last_name != repository.full_name
            }) {
                return Err(CatalogueIntentBoundaryError::CandidateInvalid);
            }
        }
    }
    if !changes.is_empty() {
        return Err(CatalogueIntentBoundaryError::CandidateInvalid);
    }
    Ok(())
}
