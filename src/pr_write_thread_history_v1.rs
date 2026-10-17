//! Closed thread-only observed origins. These records are history, NEVER a
//! publication permit. The body remains solely in the existing draft table.
use super::*;
use crate::providers::gh::OriginalThreadPublicationV1;
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};

pub const THREAD_ORIGIN_CAP_V1: usize = 16 * 1024;
pub const THREAD_HISTORY_ROWS_V1: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DatabaseV1 {
    canonical_bytes: Vec<u8>,
    device: u64,
    inode: u64,
}
impl DatabaseV1 {
    fn capture(path: &Path) -> Result<Self> {
        // Established private-directory/file/UID/link policy is not relaxed.
        drop(Cache::open(path)?);
        let canonical = std::fs::canonicalize(path)?;
        let metadata = std::fs::symlink_metadata(&canonical)?;
        let canonical_bytes = canonical.as_os_str().as_bytes();
        if canonical_bytes.len() > 4096 || !metadata.is_file() {
            bail!("Original thread database identity unavailable");
        }
        Ok(Self {
            canonical_bytes: canonical_bytes.into(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ThreadRootHistoryV1 {
    pub node: String,
    pub rest_id: u64,
    pub commit: String,
    pub path: String,
}
/// Private construction requires the actual freshly selected provider receipt.
/// Persisting this observation adds NO server permission or current state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ThreadOriginSeedV1 {
    version: u8,
    database: DatabaseV1,
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
    pub command: String,
    pub observed_resolved: Option<bool>,
    pub root: Option<ThreadRootHistoryV1>,
    pub draft_key: String,
    pub observed_login: Option<String>,
}
impl ThreadOriginSeedV1 {
    pub(crate) fn from_producer(
        database: &Path,
        identity: &HostIdentity,
        actual: &OriginalThreadPublicationV1,
        command: &str,
        draft_key: &str,
    ) -> Result<Self> {
        let parent = actual.parent();
        let root = if command == "thread-reply" {
            Some(actual.original_reply_root().map_err(anyhow::Error::msg)?)
        } else {
            None
        };
        // Bound BORROWED scalars before cloning; body/tokens are never inputs.
        let fields = [
            parent.repository_node(),
            parent.repository(),
            parent.node_id(),
            parent.head_sha(),
            parent.base_tip_sha(),
            &actual.thread().node_id,
            command,
            draft_key,
            identity.login.as_str(),
            root.map_or("", |root| root.node_id.as_str()),
            root.and_then(|root| root.commit_sha.as_deref())
                .unwrap_or(""),
            root.and_then(|root| root.observed_path.as_deref())
                .unwrap_or(""),
        ];
        if fields.iter().any(|field| field.len() > 4096)
            || fields.iter().map(|field| field.len()).sum::<usize>() > 12 * 1024
        {
            bail!("Thread origin observation exceeds bounded metadata");
        }
        let seed = Self {
            version: 1,
            database: DatabaseV1::capture(database)?,
            scope: Scope::github(&identity.host, identity.account_id)?,
            repository_id: parent.repository_id(),
            repository_node: parent.repository_node().into(),
            repository: parent.repository().into(),
            pr_id: parent.database_id(),
            pr_node: parent.node_id().into(),
            number: parent.number(),
            head: parent.head_sha().into(),
            base: parent.base_tip_sha().into(),
            thread_node: actual.thread().node_id.clone(),
            command: command.into(),
            observed_resolved: actual.thread().resolved,
            root: root.map(|root| ThreadRootHistoryV1 {
                node: root.node_id.clone(),
                rest_id: root.database_id.expect("actual root checked"),
                commit: root
                    .commit_sha
                    .as_ref()
                    .expect("actual root checked")
                    .clone(),
                path: root
                    .observed_path
                    .as_ref()
                    .expect("actual root checked")
                    .clone(),
            }),
            draft_key: draft_key.into(),
            observed_login: Some(identity.login.clone()),
        };
        seed.validate()?;
        Ok(seed)
    }
    fn validate(&self) -> Result<()> {
        let positive = |id: u64| id > 0 && id <= i64::MAX as u64;
        let uid = self
            .scope
            .login
            .strip_prefix("user-id:")
            .and_then(|v| v.parse::<u64>().ok());
        if self.version != 1
            || !uid.is_some_and(positive)
            || Scope::github(&self.scope.host, uid)? != self.scope
            || !positive(self.repository_id)
            || !positive(self.pr_id)
            || !positive(self.number)
            || !crate::search::model::valid_sha(&self.head)
            || !crate::search::model::valid_sha(&self.base)
            || !matches!(
                self.command.as_str(),
                "thread-reply" | "thread-resolve" | "thread-reopen"
            )
            || self.database.canonical_bytes.is_empty()
            || self.database.canonical_bytes.len() > 4096
            || self.database.canonical_bytes.contains(&0)
            || !self.database.canonical_bytes.starts_with(b"/")
        {
            bail!("Invalid closed thread origin");
        }
        validate_repository(&self.repository)?;
        for field in [&self.repository_node, &self.pr_node, &self.thread_node] {
            if field.len() > 512 {
                bail!("Thread origin node exceeds bound");
            }
            canonical_identifier(field)?;
        }
        if self
            .observed_login
            .as_ref()
            .is_some_and(|login| login.len() > 4096)
            || self.draft_key != self.expected_draft_key()?
        {
            bail!("Original thread draft key/actor metadata mismatch");
        }
        match (&self.root, self.command.as_str()) {
            (Some(root), "thread-reply") => {
                canonical_identifier(&root.node)?;
                if root.node.len() > 512
                    || !positive(root.rest_id)
                    || !crate::search::model::valid_sha(&root.commit)
                    || root.path.is_empty()
                    || root.path.len() > 4096
                    || root.path.contains('\0')
                {
                    bail!("Invalid recorded actual thread root");
                }
            }
            (None, "thread-resolve" | "thread-reopen") if self.observed_resolved.is_some() => {}
            _ => bail!("Thread command/root/observed-state mismatch"),
        }
        Ok(())
    }
    fn original_bytes(&self) -> Result<Vec<u8>> {
        // EXACT original producer tuple, including Option wrappers. Never
        // invert the digest or substitute current H for historical root C.
        Ok(serde_json::to_vec(&(
            &self.scope,
            self.repository_id,
            &self.repository_node,
            self.pr_id,
            &self.pr_node,
            self.number,
            &self.head,
            &self.base,
            &self.thread_node,
            &self.command,
            self.root.as_ref().map(|root| {
                (
                    &root.node,
                    Some(root.rest_id),
                    Some(&root.commit),
                    Some(&root.path),
                )
            }),
        ))?)
    }
    fn expected_draft_key(&self) -> Result<String> {
        Ok(format!(
            "thread-draft/{:x}",
            Sha256::digest(self.original_bytes()?)
        ))
    }
    pub(crate) fn write(&self, body: &str) -> Result<PrWriteIntent> {
        self.validate()?;
        if body.len() > 65_536 || (self.command != "thread-reply" && !body.is_empty()) {
            bail!("Historical thread body exceeds or disagrees with its command");
        }
        let mut selected = self.original_bytes()?;
        let prefix = if self.command == "thread-reply" {
            "thread-reply"
        } else {
            selected.extend_from_slice(if self.observed_resolved == Some(true) {
                b"\0resolved"
            } else {
                b"\0unresolved"
            });
            "thread-state"
        };
        let mut values = BTreeMap::from([
            ("body".into(), body.into()),
            ("thread-node".into(), self.thread_node.clone()),
            ("action".into(), self.command.clone()),
            ("base".into(), self.base.clone()),
            ("repo-id".into(), self.repository_id.to_string()),
            ("pr-id".into(), self.pr_id.to_string()),
        ]);
        if let Some(root) = &self.root {
            values.insert("root-node".into(), root.node.clone());
            values.insert("root-id".into(), root.rest_id.to_string());
            values.insert("parent-comment-commit".into(), root.commit.clone());
        }
        Ok(PrWriteIntent {
            command: self.command.clone(),
            selected_resource: format!("{prefix}/{:x}", Sha256::digest(selected)),
            observed_head: self.head.clone(),
            values,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ThreadOriginRecordV1 {
    pub seed: ThreadOriginSeedV1,
    pub reference: PrPublicationReference,
}
pub(crate) struct ThreadHistoryRowV1 {
    pub marker: PrWriteMarker,
    /// None is an explicitly unsupported old record, NOT an empty origin.
    pub origin: Option<ThreadOriginRecordV1>,
}
pub(crate) struct ThreadHistorySnapshotV1 {
    database: DatabaseV1,
    pub generation: i64,
    pub total: Option<usize>,
    pub rows: Vec<ThreadHistoryRowV1>,
}
pub(super) fn migrate(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch("CREATE TABLE IF NOT EXISTS pr_write_thread_origins_v1(
      attempt_id INTEGER PRIMARY KEY REFERENCES pr_write_attempts(attempt_id) ON DELETE CASCADE,
      repository_id INTEGER NOT NULL,repository_node TEXT NOT NULL,pr_id INTEGER NOT NULL,origin_json TEXT NOT NULL);
      CREATE TRIGGER IF NOT EXISTS pr_write_thread_origins_cleanup_v1 AFTER DELETE ON pr_write_attempts
      BEGIN DELETE FROM pr_write_thread_origins_v1 WHERE attempt_id=OLD.attempt_id; END;
      CREATE TRIGGER IF NOT EXISTS pr_write_thread_origins_immutable_v1 BEFORE UPDATE ON pr_write_thread_origins_v1
      BEGIN SELECT RAISE(ABORT,'Immutable original thread metadata'); END;")?;
    Ok(())
}
fn bounded_encode(record: &ThreadOriginRecordV1) -> Result<String> {
    struct Bound(usize);
    impl std::io::Write for Bound {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= THREAD_ORIGIN_CAP_V1)
                .ok_or_else(|| std::io::Error::other("Thread origin exceeds16KiB"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut bound = Bound(0);
    serde_json::to_writer(&mut bound, record)?;
    let mut bytes = Vec::with_capacity(bound.0);
    serde_json::to_writer(&mut bytes, record)?;
    Ok(String::from_utf8(bytes)?)
}
impl PrWriteJournal {
    pub(crate) fn begin_thread_v1(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        seed: &ThreadOriginSeedV1,
    ) -> Result<PrWritePermit> {
        seed.validate()?;
        if seed.database != DatabaseV1::capture(&self.path)?
            || seed.scope != target.scope
            || seed.repository != target.repository
            || seed.pr_node != target.pr_node_id
            || seed.number != target.number
            || seed
                .write(
                    intent
                        .values
                        .get("body")
                        .context("Thread body field unavailable")?,
                )?
                .digest()?
                != intent.digest()?
        {
            bail!("Actual original thread metadata does not match this exact publication");
        }
        self.begin_with_thread_origin_v1(target, intent, Some(seed))
    }
    pub(super) fn begin_with_thread_origin_v1(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        origin: Option<&ThreadOriginSeedV1>,
    ) -> Result<PrWritePermit> {
        self.begin_with_observed_origin_v2(target, intent, origin.map(ObservedOriginV2::ThreadV1))
    }
    pub(super) fn begin_with_observed_origin_v2(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        origin: Option<ObservedOriginV2<'_>>,
    ) -> Result<PrWritePermit> {
        let digest = intent.digest()?;
        if state_origin_v68::recognized(&intent.command, &intent.selected_resource)
            != matches!(&origin, Some(ObservedOriginV2::StateV68(_)))
        {
            bail!("Known State request requires its exact genuine observed State origin");
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if blocking_row(&transaction, target)?.is_some() {
            bail!("This PR has an unresolved publication. Inspect and explicitly acknowledge it before another write, including a different action or body.");
        }
        if let Some(ObservedOriginV2::ReviewActionV12(seed)) = &origin {
            review_action_lineage_v12::validate_begin(&transaction, &self.path, target, seed)?;
        }
        if let Some(ObservedOriginV2::ConversationActionV18(seed)) = &origin {
            conversation_action_lineage_v18::validate_begin(
                &transaction,
                &self.path,
                target,
                seed,
            )?;
        }
        if let Some(ObservedOriginV2::MetadataMembersV48(seed)) = &origin {
            metadata_members_v48::validate_begin(&transaction, &self.path, target, seed)?;
        }
        if let Some(ObservedOriginV2::StateV68(seed)) = &origin {
            state_origin_v68::lineage_v68::validate_begin(&transaction, &self.path, target, seed)?;
        }
        if let Some(ObservedOriginV2::IssueLinkV87(seed)) = &origin {
            issue_link_origin_v87::validate_begin(&transaction, &self.path, target, seed)?;
        } else if issue_link_origin_v87::recognized(&intent.command) {
            bail!("Known Issue-link purpose requires its actual native producer; no originless fallback");
        }
        advance(&transaction, target)?;
        let created_at = now();
        transaction.execute("INSERT INTO pr_write_attempts(host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'pending',?10)",params![target.scope.host,target.scope.login,target.pr_node_id,target.repository,target.number as i64,intent.command,intent.selected_resource,intent.observed_head,digest,created_at])?;
        let attempt_id = transaction.last_insert_rowid();
        let marker = PrWriteMarker {
            attempt_id,
            target: target.clone(),
            command: intent.command.clone(),
            selected_resource: intent.selected_resource.clone(),
            observed_head: intent.observed_head.clone(),
            payload_sha256: digest,
            state: PrWriteState::Pending,
            confirmed: None,
            created_at,
        };
        if let Some(ObservedOriginV2::ThreadV1(seed)) = &origin {
            let record = ThreadOriginRecordV1 {
                seed: (*seed).clone(),
                reference: marker.publication_reference(),
            };
            let encoded = bounded_encode(&record)?;
            transaction.execute("INSERT INTO pr_write_thread_origins_v1(attempt_id,repository_id,repository_node,pr_id,origin_json) VALUES(?1,?2,?3,?4,?5)", params![attempt_id,seed.repository_id as i64,seed.repository_node,seed.pr_id as i64,encoded])?;
        }
        match origin {
            Some(ObservedOriginV2::CommentV2(seed)) => {
                thread_comment_history_v2::insert(&transaction, seed, &marker)?
            }
            Some(ObservedOriginV2::ReactionV3(seed)) => {
                thread_reaction_history_v3::insert(&transaction, seed, &marker)?
            }
            Some(ObservedOriginV2::ReviewOperationV3(seed)) => {
                review_operation_history_v3::insert(&transaction, seed, &marker)?
            }
            Some(ObservedOriginV2::ReviewActionV12(seed)) => {
                review_action_lineage_v12::insert(&transaction, seed, &marker)?
            }
            Some(ObservedOriginV2::FileCommentV75(seed)) => {
                file_comment_history_v75::insert(&transaction, seed, &marker)?
            }
            Some(ObservedOriginV2::ConversationV17(seed)) => {
                conversation_history_v17::insert(&transaction, seed, &marker)?
            }
            Some(ObservedOriginV2::ConversationActionV18(seed)) => {
                conversation_action_lineage_v18::insert(&transaction, seed, &marker)?
            }
            Some(ObservedOriginV2::MetadataTextV25(seed)) => {
                metadata_origin_v25::insert(&transaction, seed, &marker)?
            }
            Some(ObservedOriginV2::MetadataMembersV48(seed)) => {
                metadata_members_v48::insert(&transaction, seed, &marker)?
            }
            Some(ObservedOriginV2::MetadataBaseV32(seed)) => {
                metadata_base_origin_v32::insert(&transaction, seed, &marker)?
            }
            Some(ObservedOriginV2::StateV68(seed)) => {
                state_origin_v68::insert(&transaction, seed, &marker)?
            }
            Some(ObservedOriginV2::IssueLinkV87(seed)) => {
                issue_link_origin_v87::insert(&transaction, seed, &marker)?
            }
            _ => {}
        }
        // An encoding/metadata/SQL failure above drops this SAME transaction:
        // no attempt, generation, permit or POST can escape a partial begin.
        transaction.commit()?;
        Ok(PrWritePermit {
            issuer: self.path.clone(),
            marker,
            claimed: AtomicBool::new(false),
        })
    }
    /// Caller must close fresh numeric principal/repo/PR BEFORE presenting any
    /// returned origin. Scalar actual IDs are checked BEFORE reading JSON.
    pub(crate) fn thread_history_v1(
        &self,
        target: &PrWriteTarget,
        repository_id: u64,
        repository_node: &str,
        pr_id: u64,
    ) -> Result<ThreadHistorySnapshotV1> {
        let database = DatabaseV1::capture(&self.path)?;
        let transaction = self.connection.unchecked_transaction()?;
        let generation = generation(&transaction, target)?;
        if generation < 0 {
            bail!("Invalid current journal generation");
        }
        let mut rows = Vec::new();
        let mut ids: Vec<i64> = {
            let mut statement = transaction.prepare("SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND command IN('thread-reply','thread-resolve','thread-reopen') AND state IN('pending','uncertain','confirmed') ORDER BY attempt_id DESC LIMIT ?4")?;
            let selected = statement
                .query_map(
                    params![
                        target.scope.host,
                        target.scope.login,
                        target.pr_node_id,
                        THREAD_HISTORY_ROWS_V1 + 1
                    ],
                    |row| row.get(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            selected
        };
        let total = (ids.len() <= THREAD_HISTORY_ROWS_V1).then_some(ids.len());
        ids.truncate(THREAD_HISTORY_ROWS_V1);
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
                bail!("Thread attempt metadata exceeds8KiB");
            }
            let raw=transaction.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],read_marker)?;
            if let Some(encoded) = &raw.8 {
                let value: serde_json::Value = serde_json::from_str(encoded)?;
                let object = value
                    .as_object()
                    .context("Closed historical receipt required")?;
                if object.len() != 3
                    || !["kind", "resource_id", "node_id"]
                        .iter()
                        .all(|field| object.contains_key(*field))
                {
                    bail!("Unknown historical receipt field");
                }
            }
            let marker = decode_marker(target, raw)?;
            if marker.attempt_id <= 0
                || !crate::search::model::valid_sha(&marker.observed_head)
                || marker.payload_sha256.len() != 64
                || !marker
                    .payload_sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                || marker.target.repository != target.repository
                || marker.target.number != target.number
            {
                bail!("Historical canonical target mismatch");
            }
            crate::pr_projection::thread_impact_v1::categories(
                &marker,
                marker.state == PrWriteState::Confirmed,
            )
            .map_err(anyhow::Error::msg)?
            .context("Closed thread purpose required")?;
            let metadata:Option<(u64,u64,usize,bool)>=transaction.query_row("SELECT repository_id,pr_id,length(CAST(origin_json AS BLOB)),repository_node=?2 FROM pr_write_thread_origins_v1 WHERE attempt_id=?1",params![id,repository_node],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
            let origin = if let Some((stored_repo, stored_pr, bytes, node_matches)) = metadata {
                if stored_repo != repository_id
                    || !node_matches
                    || stored_pr != pr_id
                    || bytes > THREAD_ORIGIN_CAP_V1
                {
                    bail!("Historical original repository/PR identity or bound mismatch");
                }
                let encoded:String=transaction.query_row("SELECT origin_json FROM pr_write_thread_origins_v1 WHERE attempt_id=?1 AND repository_id=?2 AND pr_id=?3 AND length(CAST(origin_json AS BLOB))<=16384",params![id,repository_id,pr_id],|row|row.get(0))?;
                let value: serde_json::Value =
                    serde_json::from_str(&encoded).context("Closed thread origin unavailable")?;
                let reference = value
                    .get("reference")
                    .and_then(serde_json::Value::as_object)
                    .context("Closed exact history reference required")?;
                if reference.len() != 4
                    || ![
                        "attempt_id",
                        "pr_node_id",
                        "observed_head",
                        "payload_sha256",
                    ]
                    .iter()
                    .all(|field| reference.contains_key(*field))
                {
                    bail!("Unknown original thread reference field");
                }
                let scope = value
                    .get("seed")
                    .and_then(|seed| seed.get("scope"))
                    .and_then(serde_json::Value::as_object)
                    .context("Closed recorded numeric scope required")?;
                if scope.len() != 2
                    || !["host", "login"]
                        .iter()
                        .all(|field| scope.contains_key(*field))
                {
                    bail!("Unknown recorded scope field");
                }
                let record: ThreadOriginRecordV1 =
                    serde_json::from_value(value).context("Closed thread origin unavailable")?;
                record.seed.validate()?;
                let shape = record.seed.write("")?;
                if record.seed.command != "thread-reply"
                    && marker.confirmed.as_ref().is_some_and(|effect| {
                        effect.node_id.as_deref() != Some(record.seed.thread_node.as_str())
                    })
                {
                    bail!("Recorded state receipt belongs to another exact thread");
                }
                if record.seed.database != database
                    || record.seed.scope != target.scope
                    || record.seed.repository_id != repository_id
                    || record.seed.repository_node != repository_node
                    || record.seed.pr_id != pr_id
                    || record.seed.repository != target.repository
                    || record.seed.pr_node != target.pr_node_id
                    || record.seed.number != target.number
                    || record.reference != marker.publication_reference()
                    || record.seed.command != marker.command
                    || shape.observed_head != marker.observed_head
                    || shape.selected_resource != marker.selected_resource
                {
                    bail!("Recorded original thread reference/issuer mismatch");
                }
                Some(record)
            } else {
                None
            };
            rows.push(ThreadHistoryRowV1 { marker, origin });
        }
        transaction.commit()?;
        Ok(ThreadHistorySnapshotV1 {
            database,
            generation,
            total,
            rows,
        })
    }
    pub(crate) fn validate_thread_history_v1(
        &self,
        target: &PrWriteTarget,
        repository_id: u64,
        repository_node: &str,
        pr_id: u64,
        original: &ThreadHistorySnapshotV1,
    ) -> Result<()> {
        let current = self.thread_history_v1(target, repository_id, repository_node, pr_id)?;
        if current.database != original.database
            || current.generation != original.generation
            || current.total != original.total
            || current.rows.len() != original.rows.len()
        {
            bail!("Thread history changed; reopen explicitly");
        }
        for (current, old) in current.rows.iter().zip(&original.rows) {
            if current.marker.publication_reference() != old.marker.publication_reference()
                || current.marker.state != old.marker.state
                || current.marker.command != old.marker.command
                || current.marker.selected_resource != old.marker.selected_resource
                || current.marker.confirmed != old.marker.confirmed
                || current.marker.created_at != old.marker.created_at
                || current.origin != old.origin
            {
                bail!("Exact historical record changed during inspection");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "pr_write_thread_history_v1_tests.rs"]
mod tests;
