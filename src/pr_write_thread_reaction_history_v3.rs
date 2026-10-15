//! CLOSED version3 selected-comment reaction origins. V1/V2 record formats and
//! validators are untouched. Historical metadata never grants write permission.
use super::*;
use crate::providers::gh::{
    OriginalThreadReactionsV3, ThreadReactionContentV3 as Content,
    ThreadReactionMutationV3 as Mutation,
};
use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
const ORIGIN_CAP: usize = 16 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ReactionActionV3 {
    Add(Content),
    Remove(u64),
}
impl ReactionActionV3 {
    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Add(_) => "thread-reaction-add-v3",
            Self::Remove(_) => "thread-reaction-remove-v3",
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Add(_) => "Add selected comment reaction",
            Self::Remove(_) => "Remove my selected reaction",
        }
    }
    pub(crate) fn mutation(self) -> Mutation {
        match self {
            Self::Add(content) => Mutation::Add(content),
            Self::Remove(id) => Mutation::Remove(id),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DatabaseV3 {
    canonical_bytes: Vec<u8>,
    device: u64,
    inode: u64,
}
impl DatabaseV3 {
    fn capture(path: &Path) -> Result<Self> {
        drop(Cache::open(path)?);
        let canonical = std::fs::canonicalize(path)?;
        let metadata = std::fs::symlink_metadata(&canonical)?;
        let bytes = canonical.as_os_str().as_bytes();
        if !metadata.is_file() || bytes.is_empty() || bytes.len() > 4096 {
            bail!("Original reaction database identity unavailable");
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
pub(crate) struct RecordedReactionV3 {
    pub id: u64,
    pub node: String,
    pub content: Content,
    pub account_id: u64,
    pub created_at: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ThreadReactionOriginSeedV3 {
    version: u8,
    database: DatabaseV3,
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
    pub observed_author: Option<thread_comment_history_v2::RecordedCommentActorV2>,
    pub comment_commit: String,
    pub original_commit: Option<String>,
    pub observed_path: String,
    pub observed_updated: String,
    pub observed_body_sha256: String,
    pub review_id: u64,
    pub review_node: String,
    pub observed_login: String,
    pub action: ReactionActionV3,
    pub content: Content,
    pub selected: Option<RecordedReactionV3>,
}
fn positive(id: u64) -> bool {
    (1..=i64::MAX as u64).contains(&id)
}
fn digest(raw: &str) -> bool {
    raw.len() == 64
        && raw
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl ThreadReactionOriginSeedV3 {
    pub(crate) fn from_producer(
        database: &Path,
        identity: &HostIdentity,
        actual: &OriginalThreadReactionsV3,
        action: ReactionActionV3,
    ) -> Result<Self> {
        let selected = actual.comment();
        let parent = selected.original().parent();
        let comment = selected.comment();
        if identity.account_id != Some(selected.principal())
            || identity.host != parent.summary().pr.host
        {
            bail!("Reaction producer numeric principal/host mismatch");
        }
        let (content, reaction) = match action {
            ReactionActionV3::Add(content) => (content, None),
            ReactionActionV3::Remove(id) => {
                let row = actual
                    .selected(id)
                    .context("Actual selected reaction ID unavailable")?;
                if row.account_id() != identity.account_id {
                    bail!("Selected reaction belongs to another/unknown numeric actor");
                }
                (
                    row.content(),
                    Some(RecordedReactionV3 {
                        id: row.id(),
                        node: row.node().into(),
                        content: row.content(),
                        account_id: row
                            .account_id()
                            .context("Actual reaction owner unavailable")?,
                        created_at: row.created_at().into(),
                    }),
                )
            }
        };
        let body = comment
            .body
            .as_deref()
            .context("Actual comment body observation unavailable")?;
        let commit = comment
            .commit_sha
            .as_deref()
            .context("Independent observed comment C unavailable")?;
        let path = comment
            .observed_path
            .as_deref()
            .context("Actual comment path unavailable")?;
        let updated = comment
            .updated_at
            .as_ref()
            .context("Actual comment timestamp unavailable")?
            .raw();
        let fields = [
            parent.repository_node(),
            parent.repository(),
            parent.node_id(),
            parent.head_sha(),
            parent.base_tip_sha(),
            &selected.original().thread().node_id,
            &comment.node_id,
            selected.review_node(),
            commit,
            comment.original_commit_sha.as_deref().unwrap_or(""),
            path,
            updated,
            identity.login.as_str(),
        ];
        if body.len() > 65_536
            || fields.iter().any(|value| value.len() > 4096)
            || fields.iter().map(|value| value.len()).sum::<usize>() > 12 * 1024
            || comment.author.as_ref().is_some_and(|actor| {
                [
                    actor.actor.login.len(),
                    actor.actor.actor_type.len(),
                    actor.actor.display_name.as_ref().map_or(0, String::len),
                    actor.actor.avatar_url.as_ref().map_or(0, String::len),
                ]
                .into_iter()
                .any(|n| n > 4096)
            })
        {
            bail!("Reaction origin borrowed scalar metadata exceeds bounds");
        }
        let seed = Self {
            version: 3,
            database: DatabaseV3::capture(database)?,
            scope: Scope::github(&identity.host, identity.account_id)?,
            repository_id: parent.repository_id(),
            repository_node: parent.repository_node().into(),
            repository: parent.repository().into(),
            pr_id: parent.database_id(),
            pr_node: parent.node_id().into(),
            number: parent.number(),
            head: parent.head_sha().into(),
            base: parent.base_tip_sha().into(),
            thread_node: selected.original().thread().node_id.clone(),
            comment_id: comment
                .database_id
                .context("Actual comment RESTID unavailable")?,
            comment_node: comment.node_id.clone(),
            author_account_id: selected.author_account_id(),
            observed_author: comment.author.as_ref().map(|actor| {
                thread_comment_history_v2::RecordedCommentActorV2 {
                    login: actor.actor.login.clone(),
                    actor_type: actor.actor.actor_type.clone(),
                    display_name: actor.actor.display_name.clone(),
                    avatar_url: actor.actor.avatar_url.clone(),
                }
            }),
            comment_commit: commit.into(),
            original_commit: comment.original_commit_sha.clone(),
            observed_path: path.into(),
            observed_updated: updated.into(),
            observed_body_sha256: format!("{:x}", Sha256::digest(body.as_bytes())),
            review_id: selected.review_id(),
            review_node: selected.review_node().into(),
            observed_login: identity.login.clone(),
            action,
            content,
            selected: reaction,
        };
        seed.validate()?;
        bounded_bytes(&seed)?;
        Ok(seed)
    }
    fn validate(&self) -> Result<()> {
        let uid = self
            .scope
            .login
            .strip_prefix("user-id:")
            .and_then(|raw| raw.parse::<u64>().ok());
        if self.version != 3
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
            || ![
                self.head.as_str(),
                self.base.as_str(),
                self.comment_commit.as_str(),
            ]
            .into_iter()
            .all(crate::search::model::valid_sha)
            || self
                .original_commit
                .as_deref()
                .is_some_and(|sha| !crate::search::model::valid_sha(sha))
            || !digest(&self.observed_body_sha256)
            || self.database.canonical_bytes.is_empty()
            || self.database.canonical_bytes.len() > 4096
            || !self.database.canonical_bytes.starts_with(b"/")
            || self.database.canonical_bytes.contains(&0)
            || self.observed_path.is_empty()
            || self.observed_path.len() > 4096
            || self.observed_path.contains('\0')
            || self.observed_login.len() > 4096
            || self.observed_updated.len() > 128
            || crate::github_workspace::timestamp_seconds(&self.observed_updated).is_none()
        {
            bail!("Closed reaction origin metadata invalid");
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
                bail!("Reaction opaque node exceeds bound");
            }
            canonical_identifier(node)?;
        }
        if self.observed_author.as_ref().is_some_and(|actor| {
            [
                actor.login.len(),
                actor.actor_type.len(),
                actor.display_name.as_ref().map_or(0, String::len),
                actor.avatar_url.as_ref().map_or(0, String::len),
            ]
            .into_iter()
            .any(|n| n > 4096)
        }) {
            bail!("Recorded author metadata exceeds bound");
        }
        match (self.action, &self.selected) {
            (ReactionActionV3::Add(content), None) if content == self.content => {}
            (ReactionActionV3::Remove(id), Some(row))
                if id == row.id
                    && positive(id)
                    && Some(row.account_id) == uid
                    && row.content == self.content
                    && row.node.len() <= 512
                    && !row.node.is_empty()
                    && !row.node.chars().any(char::is_control)
                    && row.created_at.len() <= 128
                    && crate::github_workspace::timestamp_seconds(&row.created_at).is_some() => {}
            _ => bail!("Reaction action/actual selected ownership observation differs"),
        }
        Ok(())
    }
    fn target_bytes(&self) -> Result<Vec<u8>> {
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
            self.content,
            &self.selected,
        ))
    }
    pub(crate) fn draft_key(&self) -> Result<String> {
        self.validate()?;
        Ok(format!(
            "thread-reaction-draft-v3/{:x}",
            Sha256::digest(self.target_bytes()?)
        ))
    }
    pub(crate) fn write(&self) -> Result<PrWriteIntent> {
        self.validate()?;
        Ok(PrWriteIntent {
            command: self.action.command().into(),
            selected_resource: format!(
                "thread-reaction-v3/{:x}",
                Sha256::digest(self.target_bytes()?)
            ),
            observed_head: self.head.clone(),
            values: BTreeMap::from([
                ("action".into(), self.action.command().into()),
                ("content".into(), self.content.as_str().into()),
                ("comment-id".into(), self.comment_id.to_string()),
                ("comment-node".into(), self.comment_node.clone()),
                ("thread-node".into(), self.thread_node.clone()),
                (
                    "reaction-id".into(),
                    self.selected
                        .as_ref()
                        .map_or_else(String::new, |row| row.id.to_string()),
                ),
                (
                    "reaction-node".into(),
                    self.selected
                        .as_ref()
                        .map_or_else(String::new, |row| row.node.clone()),
                ),
                ("comment-commit".into(), self.comment_commit.clone()),
                ("observed-path".into(), self.observed_path.clone()),
                ("base".into(), self.base.clone()),
                ("repo-id".into(), self.repository_id.to_string()),
                ("pr-id".into(), self.pr_id.to_string()),
            ]),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ThreadReactionOriginRecordV3 {
    pub seed: ThreadReactionOriginSeedV3,
    pub reference: PrPublicationReference,
}
fn bounded_bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    struct Bound(usize);
    impl std::io::Write for Bound {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= ORIGIN_CAP)
                .ok_or_else(|| std::io::Error::other("Reaction origin exceeds16KiB"))?;
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
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    tx.execute_batch("CREATE TABLE IF NOT EXISTS pr_write_thread_reaction_origins_v3(attempt_id INTEGER PRIMARY KEY REFERENCES pr_write_attempts(attempt_id) ON DELETE CASCADE,repository_id INTEGER NOT NULL,repository_node TEXT NOT NULL,pr_id INTEGER NOT NULL,origin_json TEXT NOT NULL);
    CREATE TRIGGER IF NOT EXISTS pr_write_thread_reaction_origins_cleanup_v3 AFTER DELETE ON pr_write_attempts BEGIN DELETE FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=OLD.attempt_id; END;
    CREATE TRIGGER IF NOT EXISTS pr_write_thread_reaction_origins_immutable_v3 BEFORE UPDATE ON pr_write_thread_reaction_origins_v3 BEGIN SELECT RAISE(ABORT,'Immutable original reaction metadata'); END;")?;
    Ok(())
}
pub(super) fn insert(
    tx: &Transaction<'_>,
    seed: &ThreadReactionOriginSeedV3,
    marker: &PrWriteMarker,
) -> Result<()> {
    let record = ThreadReactionOriginRecordV3 {
        seed: seed.clone(),
        reference: marker.publication_reference(),
    };
    let encoded = String::from_utf8(bounded_bytes(&record)?)?;
    tx.execute("INSERT INTO pr_write_thread_reaction_origins_v3(attempt_id,repository_id,repository_node,pr_id,origin_json) VALUES(?1,?2,?3,?4,?5)",params![marker.attempt_id,seed.repository_id as i64,seed.repository_node,seed.pr_id as i64,encoded])?;
    Ok(())
}
impl PrWriteJournal {
    pub(crate) fn begin_thread_reaction_v3(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        seed: &ThreadReactionOriginSeedV3,
    ) -> Result<PrWritePermit> {
        seed.validate()?;
        if seed.database != DatabaseV3::capture(&self.path)?
            || seed.scope != target.scope
            || seed.repository != target.repository
            || seed.pr_node != target.pr_node_id
            || seed.number != target.number
            || seed.write()?.digest()? != intent.digest()?
        {
            bail!("Actual reaction origin does not match exact original journal publication");
        }
        self.begin_with_observed_origin_v2(target, intent, Some(ObservedOriginV2::ReactionV3(seed)))
    }
}
pub(crate) struct ReactionHistoryRowV3 {
    pub marker: PrWriteMarker,
    pub origin: Option<ThreadReactionOriginRecordV3>,
}
pub(crate) struct ReactionHistorySnapshotV3 {
    database: DatabaseV3,
    pub generation: i64,
    pub total: Option<usize>,
    pub rows: Vec<ReactionHistoryRowV3>,
}
impl PrWriteJournal {
    pub(crate) fn thread_reaction_history_v3(
        &self,
        target: &PrWriteTarget,
        repository_id: u64,
        repository_node: &str,
        pr_id: u64,
    ) -> Result<ReactionHistorySnapshotV3> {
        let database = DatabaseV3::capture(&self.path)?;
        let tx = self.connection.unchecked_transaction()?;
        let generation = generation(&tx, target)?;
        if generation < 0 {
            bail!("Invalid original reaction journal generation");
        }
        let mut ids: Vec<i64> = {
            let mut query=tx.prepare("SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND command IN('thread-reaction-add-v3','thread-reaction-remove-v3') AND state IN('pending','uncertain','confirmed') ORDER BY attempt_id DESC LIMIT 65")?;
            let rows = query
                .query_map(
                    params![target.scope.host, target.scope.login, target.pr_node_id],
                    |row| row.get(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        let total = (ids.len() <= 64).then_some(ids.len());
        ids.truncate(64);
        let mut rows = Vec::new();
        for id in ids {
            let bytes: usize = tx.query_row(
                &format!(
                    "SELECT {} FROM pr_write_attempts WHERE attempt_id=?1",
                    source_metadata::METADATA_BYTES_SQL
                ),
                [id],
                |row| row.get(0),
            )?;
            if bytes > 8192 {
                bail!("Reaction history attempt metadata exceeds8KiB");
            }
            let raw=tx.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],read_marker)?;
            if let Some(encoded) = &raw.8 {
                let value: serde_json::Value = serde_json::from_str(encoded)?;
                let object = value
                    .as_object()
                    .context("Closed reaction receipt unavailable")?;
                if object.len() != 3
                    || !["kind", "resource_id", "node_id"]
                        .iter()
                        .all(|field| object.contains_key(*field))
                {
                    bail!("Unknown historical reaction receipt field");
                }
            }
            let marker = decode_marker(target, raw)?;
            if marker.attempt_id <= 0
                || marker.target.repository != target.repository
                || marker.target.number != target.number
                || !crate::search::model::valid_sha(&marker.observed_head)
                || !digest(&marker.payload_sha256)
            {
                bail!("Invalid exact historical reaction target");
            }
            crate::pr_projection::thread_reaction_impact_v3::categories(
                &marker,
                marker.state == PrWriteState::Confirmed,
            )
            .map_err(anyhow::Error::msg)?
            .context("Unknown reaction purpose has no history authority")?;
            let stored:Option<(u64,u64,bool,usize)>=tx.query_row("SELECT repository_id,pr_id,repository_node=?2,length(CAST(origin_json AS BLOB)) FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=?1",params![id,repository_node],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).optional()?;
            let origin = if let Some((repo, pr, node_equal, bytes)) = stored {
                if repo != repository_id || pr != pr_id || !node_equal || bytes > ORIGIN_CAP {
                    bail!("Historical reaction actual repository/PR/source metadata differs");
                }
                let encoded:String=tx.query_row("SELECT origin_json FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=?1 AND repository_id=?2 AND pr_id=?3 AND length(CAST(origin_json AS BLOB))<=16384",params![id,repository_id,pr_id],|row|row.get(0))?;
                let value: serde_json::Value = serde_json::from_str(&encoded)?;
                let reference = value
                    .get("reference")
                    .and_then(serde_json::Value::as_object)
                    .context("Exact historical reaction reference unavailable")?;
                let scope = value
                    .get("seed")
                    .and_then(|value| value.get("scope"))
                    .and_then(serde_json::Value::as_object)
                    .context("Exact historical reaction numeric scope unavailable")?;
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
                    bail!("Unknown historical reaction scope/reference field");
                }
                let record: ThreadReactionOriginRecordV3 = serde_json::from_value(value)?;
                let seed = &record.seed;
                seed.validate()?;
                let shape = seed.write()?;
                if seed.database != database
                    || seed.scope != target.scope
                    || seed.repository_id != repository_id
                    || seed.repository_node != repository_node
                    || seed.repository != target.repository
                    || seed.pr_id != pr_id
                    || seed.pr_node != target.pr_node_id
                    || seed.number != target.number
                    || record.reference != marker.publication_reference()
                    || shape.command != marker.command
                    || shape.selected_resource != marker.selected_resource
                    || shape.observed_head != marker.observed_head
                    || shape.digest()? != marker.payload_sha256
                    || seed.selected.as_ref().is_some_and(|row| {
                        marker.confirmed.as_ref().is_some_and(|effect| {
                            effect.resource_id != row.id.to_string()
                                || effect.node_id.as_deref() != Some(row.node.as_str())
                        })
                    })
                {
                    bail!(
                        "Exact historical reaction origin/intent/reference/owned receipt differs"
                    );
                }
                Some(record)
            } else {
                None
            };
            rows.push(ReactionHistoryRowV3 { marker, origin });
        }
        tx.commit()?;
        Ok(ReactionHistorySnapshotV3 {
            database,
            generation,
            total,
            rows,
        })
    }
    pub(crate) fn validate_thread_reaction_history_v3(
        &self,
        target: &PrWriteTarget,
        repository_id: u64,
        repository_node: &str,
        pr_id: u64,
        old: &ReactionHistorySnapshotV3,
    ) -> Result<()> {
        let current =
            self.thread_reaction_history_v3(target, repository_id, repository_node, pr_id)?;
        if current.database != old.database
            || current.generation != old.generation
            || current.total != old.total
            || current.rows.len() != old.rows.len()
        {
            bail!("Reaction history changed during original inspection");
        }
        for (a, b) in current.rows.iter().zip(&old.rows) {
            if a.marker.publication_reference() != b.marker.publication_reference()
                || a.marker.command != b.marker.command
                || a.marker.selected_resource != b.marker.selected_resource
                || a.marker.state != b.marker.state
                || a.marker.confirmed != b.marker.confirmed
                || a.marker.created_at != b.marker.created_at
                || a.origin != b.origin
            {
                bail!("Exact protected reaction record changed; explicitly reopen");
            }
        }
        Ok(())
    }
}
