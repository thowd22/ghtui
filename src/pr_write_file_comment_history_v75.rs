//! Closed FILE-comment origins. No line, range, existing thread or write
//! permission is reconstructed from this historical metadata.
use super::*;
use crate::providers::gh::OriginalPrFileCommentV75;
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
const ORIGIN_CAP: usize = 16 * 1024;
pub(crate) const COMMAND: &str = "file-comment-create-v75";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DatabaseV75 {
    path: Vec<u8>,
    device: u64,
    inode: u64,
}
impl DatabaseV75 {
    fn capture(path: &Path) -> Result<Self> {
        drop(Cache::open(path)?);
        let canonical = std::fs::canonicalize(path)?;
        let metadata = std::fs::symlink_metadata(&canonical)?;
        let bytes = canonical.as_os_str().as_bytes();
        if !metadata.is_file() || bytes.is_empty() || bytes.len() > 4096 {
            bail!("Original file-comment database identity unavailable");
        }
        Ok(Self {
            path: bytes.into(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileCommentOriginSeedV75 {
    version: u8,
    database: DatabaseV75,
    pub scope: Scope,
    pub repository_id: u64,
    pub repository_node: String,
    pub repository: String,
    pub pr_id: u64,
    pub pr_node: String,
    pub number: u64,
    pub head: String,
    pub base: String,
    pub path: String,
    pub previous_path: Option<String>,
    pub status: Option<String>,
    pub blob: Option<String>,
    pub observed_login: String,
}
fn positive(value: u64) -> bool {
    (1..=i64::MAX as u64).contains(&value)
}
fn path_valid(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 4096
        && !path.starts_with('/')
        && !path.contains('\0')
        && !path.split('/').any(|part| matches!(part, "" | "." | ".."))
}
fn digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn bounded(value: &impl Serialize) -> Result<Vec<u8>> {
    struct Bound(usize);
    impl std::io::Write for Bound {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= ORIGIN_CAP)
                .ok_or_else(|| std::io::Error::other("File-comment origin exceeds16KiB"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut count = Bound(0);
    serde_json::to_writer(&mut count, value)?;
    let mut bytes = Vec::with_capacity(count.0);
    serde_json::to_writer(&mut bytes, value)?;
    Ok(bytes)
}
impl FileCommentOriginSeedV75 {
    /// Pure bounded display/draft key only, not an origin constructor. The
    /// later genuine worker must independently issue and compare its seed.
    pub(crate) fn observed_draft_key(
        scope: &Scope,
        parent: &crate::providers::gh::PrResourceParentV1,
        file: &crate::providers::gh::ObservedPrFileV1,
    ) -> Result<String> {
        if !path_valid(&file.path)
            || file
                .previous_path
                .as_deref()
                .is_some_and(|p| !path_valid(p))
        {
            bail!("Actual observed FILE key is unbounded");
        }
        let bytes = bounded(&(
            scope,
            (
                parent.repository_id(),
                parent.repository_node(),
                parent.repository(),
            ),
            (
                parent.database_id(),
                parent.node_id(),
                parent.number(),
                parent.head_sha(),
                parent.base_tip_sha(),
            ),
            (
                &file.path,
                &file.previous_path,
                &file.status,
                &file.blob_sha,
            ),
            "FILE",
        ))?;
        Ok(format!(
            "file-comment-draft-v75/{:x}",
            Sha256::digest(bytes)
        ))
    }
    pub(crate) fn from_producer(
        database: &Path,
        identity: &HostIdentity,
        actual: &OriginalPrFileCommentV75,
    ) -> Result<Self> {
        let p = actual.parent();
        let f = actual.file();
        if identity.account_id != Some(actual.principal())
            || identity.host != p.summary().pr.host
            || identity.login.len() > 4096
        {
            bail!("Actual file-comment producer/principal differs");
        }
        // Bound borrowed real scalar values before retaining any clone. No
        // source, Current, permission, body or line is serialized here.
        if [
            p.repository_node(),
            p.node_id(),
            f.path.as_str(),
            f.previous_path.as_deref().unwrap_or(""),
            f.status.as_deref().unwrap_or(""),
            f.blob_sha.as_deref().unwrap_or(""),
        ]
        .into_iter()
        .any(|s| s.len() > 4096)
        {
            bail!("File-comment origin scalar exceeds bound");
        }
        let result = Self {
            version: 75,
            database: DatabaseV75::capture(database)?,
            scope: Scope::github(&identity.host, identity.account_id)?,
            repository_id: p.repository_id(),
            repository_node: p.repository_node().into(),
            repository: p.repository().into(),
            pr_id: p.database_id(),
            pr_node: p.node_id().into(),
            number: p.number(),
            head: p.head_sha().into(),
            base: p.base_tip_sha().into(),
            path: f.path.clone(),
            previous_path: f.previous_path.clone(),
            status: f.status.clone(),
            blob: f.blob_sha.clone(),
            observed_login: identity.login.clone(),
        };
        result.validate()?;
        bounded(&result)?;
        Ok(result)
    }
    fn validate(&self) -> Result<()> {
        let uid = self
            .scope
            .login
            .strip_prefix("user-id:")
            .and_then(|s| s.parse::<u64>().ok())
            .filter(|n| positive(*n));
        if self.version != 75
            || Scope::github(&self.scope.host, uid)? != self.scope
            || ![self.repository_id, self.pr_id, self.number]
                .into_iter()
                .all(positive)
            || ![self.head.as_str(), self.base.as_str()]
                .into_iter()
                .all(crate::search::model::valid_sha)
            || !path_valid(&self.path)
            || self
                .previous_path
                .as_deref()
                .is_some_and(|s| !path_valid(s))
            || self.status.as_ref().is_some_and(|s| s.len() > 4096)
            || self
                .blob
                .as_deref()
                .is_some_and(|s| !crate::search::model::valid_sha(s))
            || self.observed_login.len() > 4096
            || self.database.path.is_empty()
            || self.database.path.len() > 4096
            || !self.database.path.starts_with(b"/")
            || self.database.path.contains(&0)
        {
            bail!("Closed FILE origin metadata invalid");
        }
        validate_repository(&self.repository)?;
        for node in [&self.repository_node, &self.pr_node] {
            if node.len() > 1024 {
                bail!("FILE origin node exceeds bound");
            }
            canonical_identifier(node)?;
        }
        Ok(())
    }
    fn target_bytes(&self) -> Result<Vec<u8>> {
        bounded(&(
            &self.scope,
            (self.repository_id, &self.repository_node, &self.repository),
            (
                self.pr_id,
                &self.pr_node,
                self.number,
                &self.head,
                &self.base,
            ),
            (&self.path, &self.previous_path, &self.status, &self.blob),
            "FILE",
        ))
    }
    pub(crate) fn draft_key(&self) -> Result<String> {
        self.validate()?;
        Ok(format!(
            "file-comment-draft-v75/{:x}",
            Sha256::digest(self.target_bytes()?)
        ))
    }
    pub(crate) fn write(&self, body: &str) -> Result<PrWriteIntent> {
        self.validate()?;
        if body.len() > 65_536 {
            bail!("FILE body exceeds64KiB; nothing truncated");
        }
        Ok(PrWriteIntent {
            command: COMMAND.into(),
            selected_resource: format!(
                "file-comment-v75/{:x}",
                Sha256::digest(self.target_bytes()?)
            ),
            observed_head: self.head.clone(),
            values: BTreeMap::from([
                ("body".into(), body.into()),
                ("subject-type".into(), "file".into()),
                ("path".into(), self.path.clone()),
                ("base".into(), self.base.clone()),
                ("repo-id".into(), self.repository_id.to_string()),
                ("pr-id".into(), self.pr_id.to_string()),
            ]),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FileCommentOriginRecordV75 {
    pub seed: FileCommentOriginSeedV75,
    pub reference: PrPublicationReference,
}
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS pr_write_file_comment_origins_v75(attempt_id INTEGER PRIMARY KEY REFERENCES pr_write_attempts(attempt_id) ON DELETE CASCADE,repository_id INTEGER NOT NULL,repository_node TEXT NOT NULL,pr_id INTEGER NOT NULL,origin_json TEXT NOT NULL);
        CREATE TRIGGER IF NOT EXISTS pr_write_file_comment_origins_cleanup_v75 AFTER DELETE ON pr_write_attempts BEGIN DELETE FROM pr_write_file_comment_origins_v75 WHERE attempt_id=OLD.attempt_id; END;
        CREATE TRIGGER IF NOT EXISTS pr_write_file_comment_origins_immutable_v75 BEFORE UPDATE ON pr_write_file_comment_origins_v75 BEGIN SELECT RAISE(ABORT,'Immutable original FILE comment metadata'); END;")?;
    Ok(())
}
pub(super) fn insert(
    tx: &Transaction<'_>,
    seed: &FileCommentOriginSeedV75,
    marker: &PrWriteMarker,
) -> Result<()> {
    let record = FileCommentOriginRecordV75 {
        seed: seed.clone(),
        reference: marker.publication_reference(),
    };
    let encoded = String::from_utf8(bounded(&record)?)?;
    tx.execute("INSERT INTO pr_write_file_comment_origins_v75(attempt_id,repository_id,repository_node,pr_id,origin_json) VALUES(?1,?2,?3,?4,?5)",params![marker.attempt_id,seed.repository_id as i64,seed.repository_node,seed.pr_id as i64,encoded])?;
    Ok(())
}
impl PrWriteJournal {
    pub(crate) fn begin_file_comment_v75(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        seed: &FileCommentOriginSeedV75,
    ) -> Result<PrWritePermit> {
        seed.validate()?;
        if seed.database != DatabaseV75::capture(&self.path)?
            || seed.scope != target.scope
            || seed.repository != target.repository
            || seed.pr_node != target.pr_node_id
            || seed.number != target.number
            || seed
                .write(
                    intent
                        .values
                        .get("body")
                        .context("Exact FILE body unavailable")?,
                )?
                .digest()?
                != intent.digest()?
        {
            bail!("Actual FILE origin does not match exact publication/database");
        }
        self.begin_with_observed_origin_v2(
            target,
            intent,
            Some(ObservedOriginV2::FileCommentV75(seed)),
        )
    }
}
pub(crate) struct FileCommentHistoryRowV75 {
    pub marker: PrWriteMarker,
    pub origin: Option<FileCommentOriginRecordV75>,
}
pub(crate) struct FileCommentHistorySnapshotV75 {
    database: DatabaseV75,
    pub generation: i64,
    pub complete: bool,
    pub rows: Vec<FileCommentHistoryRowV75>,
}
impl PrWriteJournal {
    pub(crate) fn file_comment_history_v75(
        &self,
        target: &PrWriteTarget,
        repository_id: u64,
        repository_node: &str,
        pr_id: u64,
    ) -> Result<FileCommentHistorySnapshotV75> {
        let database = DatabaseV75::capture(&self.path)?;
        let tx = self.connection.unchecked_transaction()?;
        let generation = generation(&tx, target)?;
        if generation < 0 {
            bail!("FILE history generation invalid");
        }
        let mut ids: Vec<i64> = {
            let mut q=tx.prepare("SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND command='file-comment-create-v75' AND state IN('pending','uncertain','confirmed') ORDER BY attempt_id DESC LIMIT 65")?;
            let rows = q
                .query_map(
                    params![target.scope.host, target.scope.login, target.pr_node_id],
                    |r| r.get(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        let complete = ids.len() <= 64;
        ids.truncate(64);
        let mut rows = Vec::new();
        for id in ids {
            let size: usize = tx.query_row(
                &format!(
                    "SELECT {} FROM pr_write_attempts WHERE attempt_id=?1",
                    source_metadata::METADATA_BYTES_SQL
                ),
                [id],
                |r| r.get(0),
            )?;
            if size > 8192 {
                bail!("FILE history marker exceeds8KiB");
            }
            let raw=tx.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],read_marker)?;
            if let Some(encoded) = &raw.8 {
                let value: serde_json::Value = serde_json::from_str(encoded)?;
                let object = value
                    .as_object()
                    .context("Closed FILE receipt unavailable")?;
                if object.len() != 3
                    || !["kind", "resource_id", "node_id"]
                        .iter()
                        .all(|f| object.contains_key(*f))
                {
                    bail!("Unknown FILE receipt metadata");
                }
            }
            let marker = decode_marker(target, raw)?;
            validate_marker(&marker)?;
            if marker.target.repository != target.repository
                || marker.target.number != target.number
            {
                bail!("FILE historical target differs");
            }
            let scalars:Option<(u64,usize,u64,usize)>=tx.query_row("SELECT repository_id,length(CAST(repository_node AS BLOB)),pr_id,length(CAST(origin_json AS BLOB)) FROM pr_write_file_comment_origins_v75 WHERE attempt_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
            let origin = if let Some((repo, node_bytes, pr, bytes)) = scalars {
                if repo != repository_id
                    || node_bytes == 0
                    || node_bytes > 1024
                    || pr != pr_id
                    || bytes > ORIGIN_CAP
                {
                    bail!("FILE history origin IDs/bound differ");
                }
                let node: String = tx.query_row(
                    "SELECT repository_node FROM pr_write_file_comment_origins_v75 WHERE attempt_id=?1",
                    [id],
                    |r| r.get(0),
                )?;
                if node != repository_node {
                    bail!("FILE history original repository node differs");
                }
                let encoded: String = tx.query_row(
                    "SELECT origin_json FROM pr_write_file_comment_origins_v75 WHERE attempt_id=?1",
                    [id],
                    |r| r.get(0),
                )?;
                let record: FileCommentOriginRecordV75 = serde_json::from_str(&encoded)?;
                record.seed.validate()?;
                bounded(&record)?;
                if record.seed.database != database
                    || record.seed.scope != target.scope
                    || record.seed.repository != target.repository
                    || record.seed.repository_id != repository_id
                    || record.seed.repository_node != repository_node
                    || record.seed.pr_id != pr_id
                    || record.seed.pr_node != target.pr_node_id
                    || record.seed.number != target.number
                    || record.reference != marker.publication_reference()
                    || record.seed.head != marker.observed_head
                    || record.seed.write("")?.selected_resource != marker.selected_resource
                {
                    bail!("Exact FILE original record/reference differs");
                }
                Some(record)
            } else {
                None
            };
            rows.push(FileCommentHistoryRowV75 { marker, origin });
        }
        tx.commit()?;
        Ok(FileCommentHistorySnapshotV75 {
            database,
            generation,
            complete,
            rows,
        })
    }
    pub(crate) fn validate_file_comment_history_v75(
        &self,
        target: &PrWriteTarget,
        repo: u64,
        node: &str,
        pr: u64,
        old: &FileCommentHistorySnapshotV75,
    ) -> Result<()> {
        let now = self.file_comment_history_v75(target, repo, node, pr)?;
        if now.database != old.database
            || now.generation != old.generation
            || now.complete != old.complete
            || now.rows.len() != old.rows.len()
            || now.rows.iter().zip(&old.rows).any(|(a, b)| {
                a.marker.publication_reference() != b.marker.publication_reference()
                    || a.marker.target != b.marker.target
                    || a.marker.command != b.marker.command
                    || a.marker.selected_resource != b.marker.selected_resource
                    || a.marker.created_at != b.marker.created_at
                    || a.marker.state != b.marker.state
                    || a.marker.confirmed != b.marker.confirmed
                    || a.origin != b.origin
            })
        {
            bail!("Original FILE history changed during fresh read");
        }
        Ok(())
    }
}
pub(crate) fn validate_marker(marker: &PrWriteMarker) -> Result<()> {
    if marker.command != COMMAND
        || !marker
            .selected_resource
            .strip_prefix("file-comment-v75/")
            .is_some_and(digest)
        || marker.attempt_id <= 0
        || !digest(&marker.payload_sha256)
        || !crate::search::model::valid_sha(&marker.observed_head)
    {
        bail!("Unknown/malformed closed FILE publication marker");
    }
    match (marker.state, marker.confirmed.as_ref()) {
        (PrWriteState::Confirmed, Some(effect))
            if effect.kind == PrEffectKind::Comment
                && effect
                    .resource_id
                    .parse::<u64>()
                    .ok()
                    .filter(|n| positive(*n))
                    .is_some_and(|n| n.to_string() == effect.resource_id)
                && effect.node_id.as_deref().is_some_and(|node| {
                    !node.is_empty() && node.len() <= 1024 && !node.chars().any(char::is_control)
                }) => {}
        (PrWriteState::Pending | PrWriteState::Uncertain | PrWriteState::Acknowledged, None) => {}
        _ => bail!("Closed FILE publication state/receipt differs"),
    }
    Ok(())
}
