//! Purpose-specific observed review-operation origins. Caller must add this
//! closed arm to the EXISTING begin transaction; no second transaction or
//! callback. Restored metadata is never a provider source or write permit.
use super::*;
use crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::operations_v3::{
    metadata_v3::OriginalReviewOperationMetadataV3, ScopedReviewOperationSourceV3,
};
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
const ORIGIN_CAP: usize = 16 * 1024;
const HISTORY_ROWS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct DatabaseV3 {
    path: Vec<u8>,
    device: u64,
    inode: u64,
}
impl DatabaseV3 {
    pub(super) fn capture(path: &Path) -> Result<Self> {
        // Metadata only: do not open/close an ordinary SQLite FD or recreate a
        // missing DB. The existing Journal/ResourceLocalGuard owns SQL policy.
        let named = std::fs::symlink_metadata(path)?;
        if !named.is_file()
            || named.nlink() != 1
            || named.uid() != unsafe { libc::geteuid() }
            || named.mode() & 0o777 != 0o600
        {
            bail!("Original review-operation database identity unavailable");
        }
        let canonical = std::fs::canonicalize(path)?;
        let metadata = std::fs::symlink_metadata(&canonical)?;
        if !metadata.is_file()
            || metadata.dev() != named.dev()
            || metadata.ino() != named.ino()
            || metadata.nlink() != 1
            || metadata.uid() != named.uid()
            || metadata.mode() & 0o777 != 0o600
        {
            bail!("Original review-operation database changed while observing metadata");
        }
        let bytes = canonical.as_os_str().as_bytes();
        if bytes.is_empty() || bytes.len() > 4096 || bytes.contains(&0) {
            bail!("Original database name exceeds bounded observation");
        }
        Ok(Self {
            path: bytes.into(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewOperationOriginSeedV3 {
    database: DatabaseV3,
    metadata: OriginalReviewOperationMetadataV3,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewOperationOriginRecordV3 {
    pub seed: ReviewOperationOriginSeedV3,
    pub reference: PrPublicationReference,
}
pub(super) fn bounded_encode(value: &impl Serialize) -> Result<String> {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= ORIGIN_CAP)
                .ok_or_else(|| {
                    std::io::Error::other("Original review-operation record exceeds16KiB")
                })?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Count(0);
    serde_json::to_writer(&mut count, value)?;
    let mut bytes = Vec::with_capacity(count.0);
    serde_json::to_writer(&mut bytes, value)?;
    Ok(String::from_utf8(bytes)?)
}
impl ReviewOperationOriginSeedV3 {
    pub(crate) fn from_source(
        database: &Path,
        source: &ScopedReviewOperationSourceV3,
    ) -> Result<Self> {
        source
            .validate_local(database)
            .map_err(anyhow::Error::msg)?;
        let result = Self {
            database: DatabaseV3::capture(database)?,
            metadata: OriginalReviewOperationMetadataV3::from_source(source)
                .map_err(anyhow::Error::msg)?,
        };
        bounded_encode(&result)?;
        Ok(result)
    }
    pub(crate) fn metadata(&self) -> &OriginalReviewOperationMetadataV3 {
        &self.metadata
    }
    pub(super) fn validate_marker_v12(
        &self,
        database: &Path,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<()> {
        self.metadata.validate().map_err(anyhow::Error::msg)?;
        if self.database != DatabaseV3::capture(database)?
            || self.metadata.scope() != target.scope()
            || self.metadata.repository() != target.repository()
            || self.metadata.pr_node() != target.node_id()
            || self.metadata.number() != target.number()
            || self.metadata.command() != marker.command
            || self.metadata.head() != marker.observed_head
            || self
                .metadata
                .write("")
                .map_err(anyhow::Error::msg)?
                .selected_resource
                != marker.selected_resource
        {
            bail!("Exact V3 original target/selector/database differs from lineage marker");
        }
        Ok(())
    }
    pub(crate) fn matches(
        &self,
        database: &Path,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
    ) -> Result<()> {
        self.metadata.validate().map_err(anyhow::Error::msg)?;
        if self.database != DatabaseV3::capture(database)?
            || self.metadata.scope() != target.scope()
            || self.metadata.repository() != target.repository()
            || self.metadata.pr_node() != target.node_id()
            || self.metadata.number() != target.number()
            || self.metadata.command() != intent.command
            || self
                .metadata
                .write(
                    intent
                        .values
                        .get("body")
                        .context("Original operation body missing")?,
                )
                .map_err(anyhow::Error::msg)?
                .digest()?
                != intent.digest()?
        {
            bail!("Actual review-operation origin differs from this exact database/account/PR/payload");
        }
        // Whole future record preflight with ACTUAL strings and conservative
        // positive SQLite integer width, not an invented publication reference.
        #[derive(Serialize)]
        struct RefWidth<'a> {
            attempt_id: i64,
            pr_node_id: &'a str,
            observed_head: &'a str,
            payload_sha256: &'a str,
        }
        #[derive(Serialize)]
        struct Width<'a> {
            seed: &'a ReviewOperationOriginSeedV3,
            reference: RefWidth<'a>,
        }
        let digest = intent.digest()?;
        bounded_encode(&Width {
            seed: self,
            reference: RefWidth {
                attempt_id: i64::MAX,
                pr_node_id: target.node_id(),
                observed_head: &intent.observed_head,
                payload_sha256: &digest,
            },
        })?;
        Ok(())
    }
}
pub(super) fn migrate(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch("CREATE TABLE IF NOT EXISTS pr_write_review_operation_origins_v3(
      attempt_id INTEGER PRIMARY KEY REFERENCES pr_write_attempts(attempt_id) ON DELETE CASCADE,
      repository_id INTEGER NOT NULL,repository_node TEXT NOT NULL,pr_id INTEGER NOT NULL,origin_json TEXT NOT NULL);
      CREATE TRIGGER IF NOT EXISTS pr_write_review_operation_origins_cleanup_v3 AFTER DELETE ON pr_write_attempts
      BEGIN DELETE FROM pr_write_review_operation_origins_v3 WHERE attempt_id=OLD.attempt_id; END;
      CREATE TRIGGER IF NOT EXISTS pr_write_review_operation_origins_immutable_v3 BEFORE UPDATE ON pr_write_review_operation_origins_v3
      BEGIN SELECT RAISE(ABORT,'Immutable original review-operation metadata'); END;")?;
    Ok(())
}
/// SAME transaction as original attempt+generation. This closed insert is not
/// a standalone journal begin or a generic caller-supplied transaction callback.
pub(super) fn insert(
    transaction: &Transaction<'_>,
    seed: &ReviewOperationOriginSeedV3,
    marker: &PrWriteMarker,
) -> Result<()> {
    let record = ReviewOperationOriginRecordV3 {
        seed: seed.clone(),
        reference: marker.publication_reference(),
    };
    let encoded = bounded_encode(&record)?;
    transaction.execute("INSERT INTO pr_write_review_operation_origins_v3(attempt_id,repository_id,repository_node,pr_id,origin_json) VALUES(?1,?2,?3,?4,?5)",
        params![marker.attempt_id,seed.metadata.repository_id() as i64,seed.metadata.repository_node(),seed.metadata.pr_id() as i64,encoded])?;
    Ok(())
}
pub(crate) struct ReviewOperationHistoryRowV3 {
    pub marker: PrWriteMarker,
    pub origin: Option<review_action_lineage_v12::ReviewObservedOriginV12>,
}
pub(crate) struct ReviewOperationHistorySnapshotV3 {
    database: DatabaseV3,
    pub generation: i64,
    pub total: Option<usize>,
    pub rows: Vec<ReviewOperationHistoryRowV3>,
}
impl PrWriteJournal {
    pub(crate) fn begin_review_operation_v3(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        seed: &ReviewOperationOriginSeedV3,
    ) -> Result<PrWritePermit> {
        seed.matches(&self.path, target, intent)?;
        self.begin_with_observed_origin_v2(
            target,
            intent,
            Some(ObservedOriginV2::ReviewOperationV3(seed)),
        )
    }
    pub(crate) fn review_operation_original_v3(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
    ) -> Result<Option<ReviewOperationOriginRecordV3>> {
        let database = DatabaseV3::capture(&self.path)?;
        let epoch = self.source_generation(target)?;
        let Some(marker) = self.original_publication(target, reference)? else {
            return Ok(None);
        };
        let bytes:Option<usize>=self.connection.query_row("SELECT length(CAST(origin_json AS BLOB)) FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",[marker.attempt_id],|row|row.get(0)).optional()?;
        let Some(bytes) = bytes else {
            return Ok(None);
        };
        if bytes > ORIGIN_CAP {
            bail!("Original observed review operation exceeds16KiB before load");
        }
        let encoded:String=self.connection.query_row("SELECT origin_json FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1 AND length(CAST(origin_json AS BLOB))<=16384",[marker.attempt_id],|row|row.get(0))?;
        let record: ReviewOperationOriginRecordV3 = serde_json::from_str(&encoded)?;
        let original = record.seed.metadata();
        original.validate().map_err(anyhow::Error::msg)?;
        if record.seed.database != database
            || record.reference != *reference
            || original.scope() != target.scope()
            || original.repository() != target.repository()
            || original.pr_node() != target.node_id()
            || original.number() != target.number()
            || original.command() != marker.command
            || original.head() != marker.observed_head
            || original
                .write("")
                .map_err(anyhow::Error::msg)?
                .selected_resource
                != marker.selected_resource
            || epoch != self.source_generation(target)?
            || database != DatabaseV3::capture(&self.path)?
        {
            bail!("Exact original review-operation target/ref/database/generation mismatch");
        }
        Ok(Some(record))
    }
    /// Fresh Summary caller closes actual numeric principal/repository/PR and
    /// original SQL BEFORE displaying rows/body. No deleted-comment lookup or
    /// digest inversion is needed to inspect the protected original draft.
    pub(crate) fn review_operation_history_v3(
        &self,
        target: &PrWriteTarget,
        repository_id: u64,
        repository_node: &str,
        pr_id: u64,
    ) -> Result<ReviewOperationHistorySnapshotV3> {
        let database = DatabaseV3::capture(&self.path)?;
        let transaction = self.connection.unchecked_transaction()?;
        let generation = generation(&transaction, target)?;
        if generation < 0 {
            bail!("Invalid original review-operation history generation");
        }
        let mut ids = {
            let mut statement=transaction.prepare("SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND command IN('review-pending-comment-add-v3','review-pending-comment-edit-v3','review-pending-comment-delete-v3','review-rerequest-v3','review-create-pending-v2','review-update-pending-v2','review-submit-v2','review-discard-v2','review-dismiss-v2') AND state IN('pending','uncertain','confirmed') ORDER BY attempt_id DESC LIMIT ?4")?;
            let values = statement
                .query_map(
                    params![
                        target.scope.host,
                        target.scope.login,
                        target.pr_node_id,
                        HISTORY_ROWS + 1
                    ],
                    |row| row.get::<_, i64>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            values
        };
        let total = (ids.len() <= HISTORY_ROWS).then_some(ids.len());
        ids.truncate(HISTORY_ROWS);
        let mut rows = Vec::new();
        for id in ids {
            let bytes: usize = transaction.query_row(
                &format!(
                    "SELECT {} FROM pr_write_attempts WHERE attempt_id=?1",
                    source_metadata::METADATA_BYTES_SQL
                ),
                [id],
                |row| row.get(0),
            )?;
            if bytes > 8192 {
                bail!("Original review-operation journal metadata exceeds8KiB");
            }
            let raw=transaction.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],read_marker)?;
            if let Some(encoded) = &raw.8 {
                let value: serde_json::Value = serde_json::from_str(encoded)?;
                let object = value
                    .as_object()
                    .context("Closed original operation receipt required")?;
                if object.len() != 3
                    || !["kind", "resource_id", "node_id"]
                        .iter()
                        .all(|field| object.contains_key(*field))
                {
                    bail!("Unknown original operation receipt fields");
                }
            }
            let marker = decode_marker(target, raw)?;
            if marker.attempt_id <= 0
                || marker.target.repository() != target.repository()
                || marker.target.number() != target.number()
                || !crate::search::model::valid_sha(&marker.observed_head)
                || marker.payload_sha256.len() != 64
                || !marker
                    .payload_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                bail!("Original review-operation journal target/ref malformed");
            }
            let confirmed = marker.state == PrWriteState::Confirmed;
            crate::pr_projection::review_recovery_v11::shape(&marker, confirmed)
                .map_err(anyhow::Error::msg)?;
            let info:Option<(u64,u64,usize,bool)>=transaction.query_row("SELECT repository_id,pr_id,length(CAST(origin_json AS BLOB)),repository_node=?2 FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",
                params![id,repository_node],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
            let origin = if let Some((repo, pr, bytes, node_matches)) = info {
                if repo != repository_id || pr != pr_id || !node_matches || bytes > ORIGIN_CAP {
                    bail!("Original observed review-operation identity/bounds unavailable");
                }
                let encoded:String=transaction.query_row("SELECT origin_json FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1 AND length(CAST(origin_json AS BLOB))<=16384",[id],|row|row.get(0))?;
                let lineage:bool=transaction.query_row("SELECT base_key_v12 IS NOT NULL FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",[id],|row|row.get(0))?;
                let origin = review_action_lineage_v12::ReviewObservedOriginV12::decode(
                    &encoded,
                    lineage,
                    &self.path,
                    target,
                    &marker,
                    repository_id,
                    repository_node,
                    pr_id,
                )?;
                Some(origin)
            } else {
                None
            }; // legacy absence remains protected unsupported, never synthesized
            rows.push(ReviewOperationHistoryRowV3 { marker, origin });
        }
        transaction.commit()?;
        if database != DatabaseV3::capture(&self.path)?
            || generation != self.source_generation(target)?
        {
            bail!("Original history changed during bounded read");
        }
        Ok(ReviewOperationHistorySnapshotV3 {
            database,
            generation,
            total,
            rows,
        })
    }
    pub(crate) fn validate_review_operation_history_v3(
        &self,
        target: &PrWriteTarget,
        repository_id: u64,
        repository_node: &str,
        pr_id: u64,
        original: &ReviewOperationHistorySnapshotV3,
    ) -> Result<()> {
        let current =
            self.review_operation_history_v3(target, repository_id, repository_node, pr_id)?;
        if current.database != original.database
            || current.generation != original.generation
            || current.total != original.total
            || current.rows.len() != original.rows.len()
        {
            bail!(
                "Exact original review-operation history/ref changed before private presentation"
            );
        }
        for (a, b) in current.rows.iter().zip(&original.rows) {
            if a.marker.publication_reference() != b.marker.publication_reference()
                || a.marker.state != b.marker.state
                || a.marker.command != b.marker.command
                || a.marker.selected_resource != b.marker.selected_resource
                || a.marker.confirmed != b.marker.confirmed
                || a.marker.created_at != b.marker.created_at
                || serde_json::to_vec(&a.origin)? != serde_json::to_vec(&b.origin)?
            {
                bail!("Exact original review-operation history/ref changed before private presentation");
            }
        }
        Ok(())
    }
}
