//! Separately versioned selected-comment origins. V1's three thread purposes
//! remain unchanged. An origin is an observation, never permission/retry.
use super::*;
use crate::providers::gh::OriginalThreadCommentV2;
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};

pub(crate) const COMMENT_ORIGIN_CAP_V2: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum CommentActionV2 {
    Edit,
    Delete,
}
impl CommentActionV2 {
    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Edit => "thread-comment-edit-v2",
            Self::Delete => "thread-comment-delete-v2",
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Edit => "Edit selected comment",
            Self::Delete => "Delete selected comment",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DatabaseV2 {
    canonical_bytes: Vec<u8>,
    device: u64,
    inode: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedCommentActorV2 {
    pub login: String,
    pub actor_type: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
}
impl DatabaseV2 {
    fn capture(path: &Path) -> Result<Self> {
        drop(Cache::open(path)?);
        let canonical = std::fs::canonicalize(path)?;
        let metadata = std::fs::symlink_metadata(&canonical)?;
        let bytes = canonical.as_os_str().as_bytes();
        if !metadata.is_file() || bytes.is_empty() || bytes.len() > 4096 {
            bail!("Original comment database identity unavailable");
        }
        Ok(Self {
            canonical_bytes: bytes.into(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ThreadCommentOriginSeedV2 {
    version: u8,
    database: DatabaseV2,
    pub scope: Scope,
    pub repository_id: u64,
    pub repository_node: String,
    pub repository: String,
    pub pr_id: u64,
    pub pr_node: String,
    pub number: u64,
    pub head: String,
    pub base: String,
    pub thread_node: String,
    pub comment_id: u64,
    pub comment_node: String,
    pub author_account_id: Option<u64>,
    pub observed_author: Option<RecordedCommentActorV2>,
    pub review_id: u64,
    pub review_node: String,
    pub comment_commit: String,
    pub original_commit: Option<String>,
    pub observed_path: String,
    pub observed_updated: String,
    pub observed_body_sha256: String,
    pub action: CommentActionV2,
    pub observed_login: String,
}
fn positive(value: u64) -> bool {
    value > 0 && value <= i64::MAX as u64
}
impl ThreadCommentOriginSeedV2 {
    pub(crate) fn from_producer(
        database: &Path,
        identity: &HostIdentity,
        actual: &OriginalThreadCommentV2,
        action: CommentActionV2,
    ) -> Result<Self> {
        let parent = actual.original().parent();
        let comment = actual.comment();
        if identity.account_id != Some(actual.principal())
            || identity.host != parent.summary().pr.host
        {
            bail!("Actual selected comment principal/host differs from original producer");
        }
        let body = comment
            .body
            .as_deref()
            .context("Actual observed comment body unavailable")?;
        let commit = comment
            .commit_sha
            .as_deref()
            .context("Actual independent comment commit unavailable")?;
        let path = comment
            .observed_path
            .as_deref()
            .context("Actual observed raw path unavailable")?;
        let updated = comment
            .updated_at
            .as_ref()
            .context("Actual source timestamp unavailable")?
            .raw();
        if comment.author.as_ref().is_some_and(|author| {
            [
                author.actor.login.len(),
                author.actor.actor_type.len(),
                author.actor.display_name.as_ref().map_or(0, String::len),
                author.actor.avatar_url.as_ref().map_or(0, String::len),
            ]
            .into_iter()
            .any(|length| length > 4096)
        }) {
            bail!("Recorded comment author metadata exceeds borrowed scalar bounds");
        }
        let fields = [
            parent.repository_node(),
            parent.repository(),
            parent.node_id(),
            parent.head_sha(),
            parent.base_tip_sha(),
            &actual.original().thread().node_id,
            &comment.node_id,
            actual.review_node(),
            commit,
            comment.original_commit_sha.as_deref().unwrap_or(""),
            path,
            updated,
            identity.login.as_str(),
        ];
        if body.len() > 65_536
            || fields.iter().any(|value| value.len() > 4096)
            || fields.iter().map(|value| value.len()).sum::<usize>() > 12 * 1024
        {
            bail!("Actual selected-comment origin exceeds borrowed metadata bound");
        }
        let seed = Self {
            version: 2,
            database: DatabaseV2::capture(database)?,
            scope: Scope::github(&identity.host, identity.account_id)?,
            repository_id: parent.repository_id(),
            repository_node: parent.repository_node().into(),
            repository: parent.repository().into(),
            pr_id: parent.database_id(),
            pr_node: parent.node_id().into(),
            number: parent.number(),
            head: parent.head_sha().into(),
            base: parent.base_tip_sha().into(),
            thread_node: actual.original().thread().node_id.clone(),
            comment_id: comment.database_id.context("Actual RESTID unavailable")?,
            comment_node: comment.node_id.clone(),
            author_account_id: actual.author_account_id(),
            observed_author: comment
                .author
                .as_ref()
                .map(|author| RecordedCommentActorV2 {
                    login: author.actor.login.clone(),
                    actor_type: author.actor.actor_type.clone(),
                    display_name: author.actor.display_name.clone(),
                    avatar_url: author.actor.avatar_url.clone(),
                }),
            review_id: actual.review_id(),
            review_node: actual.review_node().into(),
            comment_commit: commit.into(),
            original_commit: comment.original_commit_sha.clone(),
            observed_path: path.into(),
            observed_updated: updated.into(),
            observed_body_sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
            action,
            observed_login: identity.login.clone(),
        };
        seed.validate()?;
        // Count before any encoded record/key allocation. No second body.
        bounded_bytes(&seed)?;
        Ok(seed)
    }
    fn validate(&self) -> Result<()> {
        let uid = self
            .scope
            .login
            .strip_prefix("user-id:")
            .and_then(|value| value.parse::<u64>().ok());
        if self.version != 2
            || !uid.is_some_and(positive)
            || Scope::github(&self.scope.host, uid)? != self.scope
            || ![
                self.repository_id,
                self.pr_id,
                self.number,
                self.comment_id,
                self.review_id,
            ]
            .into_iter()
            .all(positive)
            || self.author_account_id.is_some_and(|id| !positive(id))
            || self.observed_author.as_ref().is_some_and(|actor| {
                [
                    actor.login.len(),
                    actor.actor_type.len(),
                    actor.display_name.as_ref().map_or(0, String::len),
                    actor.avatar_url.as_ref().map_or(0, String::len),
                ]
                .into_iter()
                .any(|length| length > 4096)
            })
            || !crate::search::model::valid_sha(&self.head)
            || !crate::search::model::valid_sha(&self.base)
            || !crate::search::model::valid_sha(&self.comment_commit)
            || self
                .original_commit
                .as_deref()
                .is_some_and(|sha| !crate::search::model::valid_sha(sha))
            || self.database.canonical_bytes.is_empty()
            || self.database.canonical_bytes.len() > 4096
            || self.database.canonical_bytes.contains(&0)
            || !self.database.canonical_bytes.starts_with(b"/")
            || self.observed_body_sha256.len() != 64
            || !self
                .observed_body_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || self.observed_path.is_empty()
            || self.observed_path.len() > 4096
            || self.observed_path.contains('\0')
            || self.observed_login.len() > 4096
            || self.observed_updated.len() > 128
            || crate::github_workspace::timestamp_seconds(&self.observed_updated).is_none()
        {
            bail!("Closed actual selected-comment origin is invalid");
        }
        validate_repository(&self.repository)?;
        for node in [
            &self.repository_node,
            &self.pr_node,
            &self.thread_node,
            &self.comment_node,
            &self.review_node,
        ] {
            if node.len() > 512 {
                bail!("Comment origin opaque node exceeds bound");
            }
            canonical_identifier(node)?;
        }
        Ok(())
    }
    pub(crate) fn draft_key(&self) -> Result<String> {
        self.validate()?;
        Ok(format!(
            "thread-comment-draft-v2/{:x}",
            Sha256::digest(self.original_target_bytes()?)
        ))
    }
    fn original_target_bytes(&self) -> Result<Vec<u8>> {
        // Login is a separately labelled historical observation, NEVER target
        // identity. The original DB stamp is verified by begin/read guards,
        // not a portable/source permission or displayed username hash.
        bounded_bytes(&(
            &self.scope,
            (self.repository_id, &self.repository_node, &self.repository),
            (
                self.pr_id,
                &self.pr_node,
                self.number,
                &self.head,
                &self.base,
            ),
            (
                &self.thread_node,
                self.comment_id,
                &self.comment_node,
                self.author_account_id,
            ),
            (self.review_id, &self.review_node),
            (
                &self.comment_commit,
                &self.original_commit,
                &self.observed_path,
                &self.observed_updated,
                &self.observed_body_sha256,
            ),
            self.action,
        ))
    }
    pub(crate) fn write(&self, body: &str) -> Result<PrWriteIntent> {
        self.validate()?;
        if body.len() > 65_536 || (self.action == CommentActionV2::Delete && !body.is_empty()) {
            bail!("Comment action/body shape mismatch");
        }
        let selected = format!(
            "thread-comment-v2/{:x}",
            Sha256::digest(self.original_target_bytes()?)
        );
        let values = BTreeMap::from([
            ("body".into(), body.into()),
            ("action".into(), self.action.command().into()),
            ("thread-node".into(), self.thread_node.clone()),
            ("comment-node".into(), self.comment_node.clone()),
            ("comment-id".into(), self.comment_id.to_string()),
            ("review-id".into(), self.review_id.to_string()),
            ("repo-id".into(), self.repository_id.to_string()),
            ("pr-id".into(), self.pr_id.to_string()),
            ("base".into(), self.base.clone()),
            ("comment-commit".into(), self.comment_commit.clone()),
            ("observed-path".into(), self.observed_path.clone()),
        ]);
        Ok(PrWriteIntent {
            command: self.action.command().into(),
            selected_resource: selected,
            observed_head: self.head.clone(),
            values,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ThreadCommentOriginRecordV2 {
    pub seed: ThreadCommentOriginSeedV2,
    pub reference: PrPublicationReference,
}
fn bounded_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    struct Bound(usize);
    impl std::io::Write for Bound {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= COMMENT_ORIGIN_CAP_V2)
                .ok_or_else(|| std::io::Error::other("Comment origin exceeds16KiB"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bound = Bound(0);
    serde_json::to_writer(&mut bound, value)?;
    let mut encoded = Vec::with_capacity(bound.0);
    serde_json::to_writer(&mut encoded, value)?;
    Ok(encoded)
}
pub(super) fn migrate(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch("CREATE TABLE IF NOT EXISTS pr_write_thread_comment_origins_v2(
      attempt_id INTEGER PRIMARY KEY REFERENCES pr_write_attempts(attempt_id) ON DELETE CASCADE,
      repository_id INTEGER NOT NULL,repository_node TEXT NOT NULL,pr_id INTEGER NOT NULL,origin_json TEXT NOT NULL);
      CREATE TRIGGER IF NOT EXISTS pr_write_thread_comment_origins_cleanup_v2 AFTER DELETE ON pr_write_attempts
      BEGIN DELETE FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=OLD.attempt_id; END;
      CREATE TRIGGER IF NOT EXISTS pr_write_thread_comment_origins_immutable_v2 BEFORE UPDATE ON pr_write_thread_comment_origins_v2
      BEGIN SELECT RAISE(ABORT,'Immutable original selected-comment metadata'); END;")?;
    Ok(())
}
pub(super) fn insert(
    transaction: &Transaction<'_>,
    seed: &ThreadCommentOriginSeedV2,
    marker: &PrWriteMarker,
) -> Result<()> {
    let record = ThreadCommentOriginRecordV2 {
        seed: seed.clone(),
        reference: marker.publication_reference(),
    };
    let encoded = String::from_utf8(bounded_bytes(&record)?)?;
    transaction.execute("INSERT INTO pr_write_thread_comment_origins_v2(attempt_id,repository_id,repository_node,pr_id,origin_json) VALUES(?1,?2,?3,?4,?5)",
        params![marker.attempt_id,seed.repository_id as i64,seed.repository_node,seed.pr_id as i64,encoded])?;
    Ok(())
}
impl PrWriteJournal {
    pub(crate) fn begin_thread_comment_v2(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        seed: &ThreadCommentOriginSeedV2,
    ) -> Result<PrWritePermit> {
        seed.validate()?;
        if seed.database != DatabaseV2::capture(&self.path)?
            || seed.scope != target.scope
            || seed.repository != target.repository
            || seed.pr_node != target.pr_node_id
            || seed.number != target.number
            || seed
                .write(
                    intent
                        .values
                        .get("body")
                        .context("Exact comment draft body unavailable")?,
                )?
                .digest()?
                != intent.digest()?
        {
            bail!("Actual selected-comment metadata does not match exact original publication");
        }
        self.begin_with_observed_origin_v2(target, intent, Some(ObservedOriginV2::CommentV2(seed)))
    }
}
pub(crate) struct CommentHistoryRowV2 {
    pub marker: PrWriteMarker,
    pub origin: Option<ThreadCommentOriginRecordV2>,
}
pub(crate) struct CommentHistorySnapshotV2 {
    database: DatabaseV2,
    pub generation: i64,
    pub total: Option<usize>,
    pub rows: Vec<CommentHistoryRowV2>,
}
impl PrWriteJournal {
    pub(crate) fn thread_comment_history_v2(
        &self,
        target: &PrWriteTarget,
        repository_id: u64,
        repository_node: &str,
        pr_id: u64,
    ) -> Result<CommentHistorySnapshotV2> {
        let database = DatabaseV2::capture(&self.path)?;
        let transaction = self.connection.unchecked_transaction()?;
        let generation = generation(&transaction, target)?;
        if generation < 0 {
            bail!("Invalid original comment history generation");
        }
        let mut ids: Vec<i64> = {
            let mut statement=transaction.prepare("SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND command IN('thread-comment-edit-v2','thread-comment-delete-v2','thread-comment-reaction-add-v2','thread-comment-reaction-remove-v2') AND state IN('pending','uncertain','confirmed') ORDER BY attempt_id DESC LIMIT 65")?;
            let ids = statement
                .query_map(
                    params![target.scope.host, target.scope.login, target.pr_node_id],
                    |row| row.get(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids
        };
        let total = (ids.len() <= 64).then_some(ids.len());
        ids.truncate(64);
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
                bail!("Selected-comment historical attempt metadata exceeds8KiB");
            }
            let raw=transaction.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],read_marker)?;
            if let Some(encoded) = &raw.8 {
                let value: serde_json::Value = serde_json::from_str(encoded)?;
                let object = value
                    .as_object()
                    .context("Closed historical comment receipt required")?;
                if object.len() != 3
                    || !["kind", "resource_id", "node_id"]
                        .iter()
                        .all(|field| object.contains_key(*field))
                {
                    bail!("Unknown historical comment receipt field");
                }
            }
            let marker = decode_marker(target, raw)?;
            if marker.attempt_id <= 0
                || marker.target.repository != target.repository
                || marker.target.number != target.number
                || !crate::search::model::valid_sha(&marker.observed_head)
                || marker.payload_sha256.len() != 64
                || !marker
                    .payload_sha256
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                bail!("Invalid historical selected-comment target");
            }
            crate::pr_projection::thread_comment_impact_v2::categories(
                &marker,
                marker.state == PrWriteState::Confirmed,
            )
            .map_err(anyhow::Error::msg)?
            .context("Closed comment/reaction purpose required; unknown is not permission")?;
            let metadata:Option<(u64,u64,bool,usize)>=transaction.query_row("SELECT repository_id,pr_id,repository_node=?2,length(CAST(origin_json AS BLOB)) FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=?1",params![id,repository_node],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
            let origin = if let Some((stored_repo, stored_pr, node_matches, bytes)) = metadata {
                if stored_repo != repository_id
                    || stored_pr != pr_id
                    || !node_matches
                    || bytes > COMMENT_ORIGIN_CAP_V2
                {
                    bail!("Exact historical comment repository/PR metadata or byte bound mismatch");
                }
                let encoded:String=transaction.query_row("SELECT origin_json FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=?1 AND repository_id=?2 AND pr_id=?3 AND length(CAST(origin_json AS BLOB))<=16384",params![id,repository_id,pr_id],|row|row.get(0))?;
                let value: serde_json::Value = serde_json::from_str(&encoded)?;
                let reference = value
                    .get("reference")
                    .and_then(serde_json::Value::as_object)
                    .context("Exact closed historical reference required")?;
                let scope = value
                    .get("seed")
                    .and_then(|seed| seed.get("scope"))
                    .and_then(serde_json::Value::as_object)
                    .context("Exact historical numeric scope required")?;
                if reference.len() != 4
                    || ![
                        "attempt_id",
                        "pr_node_id",
                        "observed_head",
                        "payload_sha256",
                    ]
                    .iter()
                    .all(|field| reference.contains_key(*field))
                    || scope.len() != 2
                    || !["host", "login"]
                        .iter()
                        .all(|field| scope.contains_key(*field))
                {
                    bail!("Unknown historical reference/scope field");
                }
                let record: ThreadCommentOriginRecordV2 = serde_json::from_value(value)?;
                record.seed.validate()?;
                let seed = &record.seed;
                let shape = seed.write("")?;
                if seed.database != database
                    || seed.scope != target.scope
                    || seed.repository_id != repository_id
                    || seed.repository_node != repository_node
                    || seed.pr_id != pr_id
                    || seed.pr_node != target.pr_node_id
                    || seed.repository != target.repository
                    || seed.number != target.number
                    || record.reference != marker.publication_reference()
                    || seed.action.command() != marker.command
                    || shape.selected_resource != marker.selected_resource
                    || shape.observed_head != marker.observed_head
                    || marker.confirmed.as_ref().is_some_and(|effect| {
                        effect.resource_id != seed.comment_id.to_string()
                            || effect.node_id.as_deref() != Some(seed.comment_node.as_str())
                    })
                {
                    bail!("Exact historical comment origin/reference/receipt differs");
                }
                Some(record)
            } else {
                None
            };
            rows.push(CommentHistoryRowV2 { marker, origin });
        }
        transaction.commit()?;
        Ok(CommentHistorySnapshotV2 {
            database,
            generation,
            total,
            rows,
        })
    }
    pub(crate) fn validate_thread_comment_history_v2(
        &self,
        target: &PrWriteTarget,
        repository_id: u64,
        repository_node: &str,
        pr_id: u64,
        original: &CommentHistorySnapshotV2,
    ) -> Result<()> {
        let current =
            self.thread_comment_history_v2(target, repository_id, repository_node, pr_id)?;
        if current.database != original.database
            || current.generation != original.generation
            || current.total != original.total
            || current.rows.len() != original.rows.len()
        {
            bail!("Comment history changed during original inspection");
        }
        for (a, b) in current.rows.iter().zip(&original.rows) {
            if a.marker.publication_reference() != b.marker.publication_reference()
                || a.marker.state != b.marker.state
                || a.marker.command != b.marker.command
                || a.marker.selected_resource != b.marker.selected_resource
                || a.marker.confirmed != b.marker.confirmed
                || a.marker.created_at != b.marker.created_at
                || a.origin != b.origin
            {
                bail!("Exact protected comment record changed; reopen explicitly");
            }
        }
        Ok(())
    }
}
