//! Closed original metadata for all SIX Conversation purposes. No body copy,
//! restored permission, digest-to-source reconstruction or automatic retry.
use super::*;
use crate::{
    pr_actions::{PrCommand, PrOperation},
    providers::gh::{Actor, FreshConversationPublicationV17},
    scoped_pr_collaboration_source_v17::PrCollaborationSourceV17,
};
use review_operation_history_v3::{bounded_encode, DatabaseV3};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum ConversationActionV17 {
    Create,
    Edit,
    Delete,
    AddReaction,
    RemoveReaction,
    Subscription,
}
impl ConversationActionV17 {
    pub(crate) fn from_command(command: PrCommand) -> Result<Self> {
        Ok(match command {
            PrCommand::NewComment => Self::Create,
            PrCommand::EditComment => Self::Edit,
            PrCommand::DeleteComment => Self::Delete,
            PrCommand::AddReaction => Self::AddReaction,
            PrCommand::RemoveReaction => Self::RemoveReaction,
            PrCommand::Subscription => Self::Subscription,
            _ => bail!("File progress is not a Conversation purpose"),
        })
    }
    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::Create => "conversation-create-v17",
            Self::Edit => "conversation-edit-v17",
            Self::Delete => "conversation-delete-v17",
            Self::AddReaction => "conversation-reaction-add-v17",
            Self::RemoveReaction => "conversation-reaction-remove-v17",
            Self::Subscription => "conversation-subscription-v17",
        }
    }
    fn key(self) -> Option<&'static str> {
        match self {
            Self::Create | Self::Edit => Some("body"),
            Self::Delete => None,
            Self::AddReaction => Some("reaction"),
            Self::RemoveReaction => Some("reaction_id"),
            Self::Subscription => Some("subscription"),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedConversationSelectionV17 {
    pub id: u64,
    pub node: String,
    pub author_account: Option<u64>,
    pub author: Actor,
    pub updated: String,
    pub body_sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RecordedSubscriptionV17 {
    state: Option<crate::providers::gh::Subscription>,
    can_subscribe: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConversationOriginSeedV17 {
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
    pub action: ConversationActionV17,
    pub selected: Option<RecordedConversationSelectionV17>,
    pub reaction: Option<(u64, String)>,
    subscription: Option<RecordedSubscriptionV17>,
    pub observed_requester: String,
}
fn positive(id: u64) -> bool {
    (1..=i64::MAX as u64).contains(&id)
}
fn node(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024 && !value.chars().any(char::is_control)
}
fn sha(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
/// Worker-only stable form key, shared by actual Open and observed origin.
/// Raw parent/selection is descriptive here, never promoted to authority.
pub(crate) fn draft_key_v17(
    scope: &Scope,
    parent: &crate::providers::gh::PrResourceParentV1,
    command: PrCommand,
    selected: Option<&crate::pr_actions::SelectedComment>,
) -> Result<String> {
    let action = ConversationActionV17::from_command(command)?;
    draft_key_observed_v18(
        scope,
        parent,
        action,
        selected.map(|s| (s.observed.id, s.observed.node_id.as_str())),
    )
}
/// Exact same serialized V17 target tuple. The caller must already hold the
/// actual original read; numeric/path strings here are descriptive, not source.
pub(super) fn draft_key_observed_v18(
    scope: &Scope,
    parent: &crate::providers::gh::PrResourceParentV1,
    action: ConversationActionV17,
    selected: Option<(u64, &str)>,
) -> Result<String> {
    let target = serde_json::to_vec(&(
        scope,
        (parent.repository_id(), parent.repository_node()),
        (
            parent.database_id(),
            parent.node_id(),
            parent.number(),
            parent.head_sha(),
            parent.base_tip_sha(),
        ),
        action,
        selected,
    ))?;
    if target.len() > 16 * 1024 {
        bail!("Conversation draft target exceeds bound");
    }
    Ok(format!(
        "conversation-draft-v17/{:x}",
        Sha256::digest(target)
    ))
}
impl ConversationOriginSeedV17 {
    pub(crate) fn form_key(&self) -> Result<String> {
        Ok(format!(
            "conversation-draft-v17/{:x}",
            Sha256::digest(self.target_bytes()?)
        ))
    }
    pub(crate) fn validate_marker(
        &self,
        database: &Path,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<()> {
        self.validate()?;
        if !self.matches_database(database)?
            || self.scope != *target.scope()
            || self.repository != target.repository()
            || self.pr_node != target.node_id()
            || self.number != target.number()
            || self.action.command() != marker.command
            || self.draft_key()? != marker.selected_resource
            || self.head != marker.observed_head
            || marker.attempt_id <= 0
            || marker.payload_sha256.len() != 64
            || !sha(&marker.payload_sha256)
        {
            bail!("Exact original Conversation marker/origin/DB differs");
        }
        if marker.state == PrWriteState::Acknowledged {
            if marker.confirmed.is_some() {
                bail!("Acknowledged original has an unexpected receipt");
            }
        } else {
            crate::pr_projection::conversation_impact_v17::categories(
                marker,
                marker.state == PrWriteState::Confirmed,
            )
            .map_err(anyhow::Error::msg)?
            .context("Not a closed Conversation purpose")?;
        }
        Ok(())
    }
    /// Only an ACTUAL fresh observation on the original source can issue new
    /// origin metadata. Decoded historical metadata has no inverse producer.
    pub(crate) fn from_producer(
        source: &PrCollaborationSourceV17,
        actual: &FreshConversationPublicationV17,
        operation: &PrOperation,
        database: &Path,
    ) -> Result<Self> {
        let parent = actual.parent();
        let identity = actual.identity();
        if !GithubProvider::same_thread_history_parent_v1(source.parent(), parent)
            || Scope::github(&identity.host, identity.account_id)? != *source.opening().scope()
            || database != source.opening().database()
        {
            bail!("Actual original Conversation producer/parent/account differs");
        }
        let selected = actual
            .comment()
            .map(|comment| {
                if !GithubProvider::same_thread_history_parent_v1(comment.parent(), parent) {
                    bail!("Actual selected comment belongs to another original parent");
                }
                let observed = comment.observed();
                if observed.body.len() > 1024 * 1024
                    || observed.author.login.len() > 1024
                    || observed.author.actor_type.len() > 1024
                    || observed
                        .author
                        .display_name
                        .as_ref()
                        .is_some_and(|s| s.len() > 4096)
                    || observed
                        .author
                        .avatar_url
                        .as_ref()
                        .is_some_and(|s| s.len() > 4096)
                {
                    bail!("Actual observed Conversation metadata exceeds borrowed bounds");
                }
                Ok(RecordedConversationSelectionV17 {
                    id: observed.id,
                    node: observed.node_id.clone(),
                    author_account: comment.author_account(),
                    author: observed.author.clone(),
                    updated: observed.updated_at.clone(),
                    body_sha256: format!("{:x}", Sha256::digest(observed.body.as_bytes())),
                })
            })
            .transpose()?;
        let seed = Self {
            version: 17,
            database: DatabaseV3::capture(database)?,
            scope: source.opening().scope().clone(),
            repository_id: parent.repository_id(),
            repository_node: parent.repository_node().into(),
            repository: parent.repository().into(),
            pr_id: parent.database_id(),
            pr_node: parent.node_id().into(),
            number: parent.number(),
            head: parent.head_sha().into(),
            base: parent.base_tip_sha().into(),
            action: ConversationActionV17::from_command(operation.command_v17())?,
            selected,
            reaction: match operation {
                PrOperation::RemoveReaction(_, r) => Some((r.id, r.node_id.clone())),
                _ => None,
            },
            subscription: actual.subscription().map(|v| RecordedSubscriptionV17 {
                state: v.current().state,
                can_subscribe: v.current().viewer_can_subscribe,
            }),
            observed_requester: identity.login.clone(),
        };
        seed.validate()?;
        bounded_encode(&seed)?;
        Ok(seed)
    }
    pub(crate) fn validate(&self) -> Result<()> {
        let uid = self
            .scope
            .login
            .strip_prefix("user-id:")
            .and_then(|s| s.parse().ok());
        if self.version != 17
            || !uid.is_some_and(positive)
            || Scope::github(&self.scope.host, uid)? != self.scope
            || ![self.repository_id, self.pr_id, self.number]
                .into_iter()
                .all(positive)
            || ![&self.repository_node, &self.pr_node]
                .into_iter()
                .all(|v| node(v))
            || !crate::search::model::valid_sha(&self.head)
            || !crate::search::model::valid_sha(&self.base)
            || self.observed_requester.is_empty()
            || self.observed_requester.len() > 1024
            || matches!(
                self.action,
                ConversationActionV17::Create | ConversationActionV17::Subscription
            ) != self.selected.is_none()
            || (self.action == ConversationActionV17::RemoveReaction) != self.reaction.is_some()
            || (self.action == ConversationActionV17::Subscription) != self.subscription.is_some()
            || self.subscription.as_ref().is_some_and(|s| !s.can_subscribe)
        {
            bail!("Closed Conversation origin shape/account is invalid");
        }
        validate_repository(&self.repository)?;
        if self.selected.as_ref().is_some_and(|v| {
            !positive(v.id)
                || !node(&v.node)
                || v.author_account.is_some_and(|id| !positive(id))
                || !sha(&v.body_sha256)
                || v.updated.len() > 128
                || crate::github_workspace::timestamp_seconds(&v.updated).is_none()
                || v.author.login.len() > 1024
                || v.author.actor_type.len() > 1024
                || v.author
                    .display_name
                    .as_ref()
                    .is_some_and(|s| s.len() > 4096)
                || v.author.avatar_url.as_ref().is_some_and(|s| s.len() > 4096)
        }) || self
            .reaction
            .as_ref()
            .is_some_and(|(id, n)| !positive(*id) || !node(n))
        {
            bail!("Actual selected Conversation ID/node/actor/hash is invalid");
        }
        bounded_encode(self)?;
        Ok(())
    }
    fn target_bytes(&self) -> Result<Vec<u8>> {
        self.validate()?;
        Ok(serde_json::to_vec(&(
            &self.scope,
            (self.repository_id, &self.repository_node),
            (
                self.pr_id,
                &self.pr_node,
                self.number,
                &self.head,
                &self.base,
            ),
            self.action,
            self.selected.as_ref().map(|s| (s.id, &s.node)),
        ))?)
    }
    pub(crate) fn draft_key(&self) -> Result<String> {
        Ok(format!(
            "conversation-v17/{:x}",
            Sha256::digest(self.target_bytes()?)
        ))
    }
    pub(crate) fn write(&self, values: &BTreeMap<String, String>) -> Result<PrWriteIntent> {
        self.validate()?;
        let wanted = self.action.key();
        if values.len() != usize::from(wanted.is_some())
            || wanted.is_some_and(|key| !values.contains_key(key))
            || values.values().any(|v| v.len() > 65_536)
        {
            bail!("Conversation fields do not match their exact closed purpose");
        }
        let value = wanted
            .and_then(|key| values.get(key))
            .map(String::as_str)
            .unwrap_or("");
        match self.action {
            ConversationActionV17::Create | ConversationActionV17::Edit
                if value.trim().is_empty() =>
            {
                bail!("Comment text is required")
            }
            ConversationActionV17::AddReaction
                if ![
                    "+1", "-1", "laugh", "confused", "heart", "hooray", "rocket", "eyes",
                ]
                .contains(&value) =>
            {
                bail!("Unknown reaction content")
            }
            ConversationActionV17::RemoveReaction
                if self
                    .reaction
                    .as_ref()
                    .is_none_or(|(id, _)| id.to_string() != value) =>
            {
                bail!("Exact current-owned reaction ID differs")
            }
            ConversationActionV17::Subscription
                if !["subscribed", "unsubscribed", "ignored"].contains(&value) =>
            {
                bail!("Unknown requested subscription state")
            }
            _ => {}
        }
        let mut bound = values.clone();
        // Historical requester/author presentation is not numeric identity.
        // Username/avatar renames cannot alter this original payload binding.
        let observation = serde_json::to_vec(&(
            self.target_bytes()?,
            self.selected
                .as_ref()
                .map(|s| (s.author_account, &s.updated, &s.body_sha256)),
            &self.reaction,
            &self.subscription,
        ))?;
        bound.insert(
            "origin-v17".into(),
            format!("{:x}", Sha256::digest(observation)),
        );
        Ok(PrWriteIntent {
            command: self.action.command().into(),
            selected_resource: self.draft_key()?,
            observed_head: self.head.clone(),
            values: bound,
        })
    }
    pub(crate) fn matches_database(&self, database: &Path) -> Result<bool> {
        Ok(self.database == DatabaseV3::capture(database)?)
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ConversationOriginRecordV17 {
    pub seed: ConversationOriginSeedV17,
    pub reference: PrPublicationReference,
}
pub(super) fn migrate(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch("CREATE TABLE IF NOT EXISTS pr_write_conversation_origins_v17(
        attempt_id INTEGER PRIMARY KEY REFERENCES pr_write_attempts(attempt_id) ON DELETE CASCADE,
        repository_id INTEGER NOT NULL,pr_id INTEGER NOT NULL,origin_json TEXT NOT NULL);
        CREATE TRIGGER IF NOT EXISTS pr_write_conversation_origins_cleanup_v17 AFTER DELETE ON pr_write_attempts
        BEGIN DELETE FROM pr_write_conversation_origins_v17 WHERE attempt_id=OLD.attempt_id; END;
        CREATE TRIGGER IF NOT EXISTS pr_write_conversation_origins_immutable_v17 BEFORE UPDATE ON pr_write_conversation_origins_v17
        BEGIN SELECT RAISE(ABORT,'Immutable original Conversation metadata'); END;")?;
    Ok(())
}
pub(super) fn insert(
    transaction: &Transaction<'_>,
    seed: &ConversationOriginSeedV17,
    marker: &PrWriteMarker,
) -> Result<()> {
    let record = ConversationOriginRecordV17 {
        seed: seed.clone(),
        reference: marker.publication_reference(),
    };
    let encoded = bounded_encode(&record)?;
    transaction.execute("INSERT INTO pr_write_conversation_origins_v17(attempt_id,repository_id,pr_id,origin_json) VALUES(?1,?2,?3,?4)",
        params![marker.attempt_id, seed.repository_id as i64, seed.pr_id as i64, encoded])?;
    Ok(())
}
impl PrWriteJournal {
    pub(crate) fn acknowledge_conversation_inspection_v17(
        &mut self,
        provider: &GithubProvider,
        mut witness: ConversationRecoveryWitnessV17,
        observation: crate::providers::gh::ConversationRecoveryCurrentV17,
        cancel: &CancellationToken,
        deadline: std::time::Instant,
    ) -> Result<usize> {
        if cancel.is_cancelled()
            || std::time::Instant::now() >= deadline
            || !observation.matches_target(&witness.target)
        {
            bail!("Original Conversation ACK observation differs or ended");
        }
        let row = witness
            .exact
            .take()
            .context("Exact original raw ACK row unavailable")?;
        row.validate_current(self)?;
        provider
            .reconcile_conversation_inspection_v17(observation, cancel, deadline)
            .map_err(anyhow::Error::msg)?;
        // A SQL failure after reconciliation retains the durable blocker.
        // Never restore ambiguity, replay a body or automatically ACK.
        self.commit_exact_ack_v17(row, cancel, deadline)
    }
    pub(crate) fn conversation_origin_v17(
        &self,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<conversation_action_lineage_v18::ObservedConversationOriginV18> {
        let base: usize = self.connection.query_row(
            "SELECT count(*) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1",
            [marker.attempt_id],
            |r| r.get(0),
        )?;
        if base != 0 {
            bail!(
                "Mixed Base/Conversation origins cannot expose protected body or release a barrier"
            );
        }
        let bytes: usize = self.connection.query_row(
            "SELECT length(CAST(origin_json AS BLOB)) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",
            [marker.attempt_id], |r| r.get(0))?;
        if bytes > 16 * 1024 {
            bail!("Original Conversation metadata exceeds16KiB before read");
        }
        let sizes:(usize,usize)=self.connection.query_row("SELECT COALESCE(length(CAST(base_key_v18 AS BLOB)),0),COALESCE(length(CAST(action_tag_v18 AS BLOB)),0) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",[marker.attempt_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        if sizes.0 > 256 || sizes.1 > 128 {
            bail!("Original Conversation lineage index exceeds bound");
        }
        let (repo, pr, raw, base, action, ack): (u64,u64,String,Option<String>,Option<String>,Option<i64>) = self.connection.query_row(
            "SELECT repository_id,pr_id,origin_json,base_key_v18,action_tag_v18,parent_ack_attempt_v18 FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1",
            [marker.attempt_id], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
        conversation_action_lineage_v18::ObservedConversationOriginV18::decode(
            &raw,
            &self.path,
            target,
            marker,
            repo,
            pr,
            conversation_action_lineage_v18::ConversationLineageIndexV18 {
                base: base.as_deref(),
                action: action.as_deref(),
                parent_ack: ack,
            },
        )
    }
    pub(crate) fn conversation_history_v17(
        &self,
        target: &PrWriteTarget,
        parent: &crate::providers::gh::PrResourceParentV1,
    ) -> Result<ConversationHistoryV17> {
        let database = DatabaseV3::capture(&self.path)?;
        let generation = self.source_generation(target)?;
        if let Some(blocker) = self.blocking(target)? {
            // No foreign/thread/unknown-purpose barrier grants body access.
            self.conversation_origin_v17(target, &blocker)?;
        }
        let ids = {
            let mut statement = self.connection.prepare("SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND command IN('conversation-create-v17','conversation-edit-v17','conversation-delete-v17','conversation-reaction-add-v17','conversation-reaction-remove-v17','conversation-subscription-v17') ORDER BY attempt_id DESC LIMIT 65")?;
            let ids = statement
                .query_map(
                    params![target.scope.host, target.scope.login, target.pr_node_id],
                    |r| r.get::<_, i64>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids
        };
        let complete = ids.len() <= 64;
        let mut rows = Vec::new();
        for id in ids.into_iter().take(64) {
            let bytes: usize = self.connection.query_row(
                &format!(
                    "SELECT {} FROM pr_write_attempts WHERE attempt_id=?1",
                    source_metadata::METADATA_BYTES_SQL
                ),
                [id],
                |r| r.get(0),
            )?;
            if bytes > 8192 {
                bail!("Conversation journal metadata exceeds8KiB before read");
            }
            let raw = self.connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],read_marker)?;
            let marker = decode_marker(target, raw)?;
            let origin = self.conversation_origin_v17(target, &marker)?;
            if origin.seed().repository_id != parent.repository_id()
                || origin.seed().repository_node != parent.repository_node()
                || origin.seed().pr_id != parent.database_id()
                || origin.seed().pr_node != parent.node_id()
            {
                bail!("Current original parent differs from recorded Conversation history");
            }
            let key = origin.key()?;
            let saved = Cache::open(&self.path)?.bounded_draft(target.scope(), &key, 128 * 1024)?;
            let body = saved.map(|encoded| {
                #[derive(Deserialize)] #[serde(deny_unknown_fields)]
                struct Envelope { version:u8, values:BTreeMap<String,String>, publication:Option<PrPublicationReference> }
                let parsed:Envelope = serde_json::from_str(&encoded)?;
                if parsed.version!=17 || parsed.publication.as_ref()!=Some(&marker.publication_reference())
                    || origin.write(&parsed.values)?.digest()? != marker.payload_sha256 {
                    bail!("Original protected body/reference/fingerprint differs; nothing restored");
                }
                Ok::<_,anyhow::Error>((encoded,parsed.values))
            }).transpose()?;
            rows.push(ConversationHistoryRowV17 {
                marker,
                origin,
                body,
            });
        }
        if database != DatabaseV3::capture(&self.path)?
            || generation != self.source_generation(target)?
        {
            bail!("Original Conversation history changed during bounded read");
        }
        Ok(ConversationHistoryV17 {
            database,
            generation,
            complete,
            rows,
        })
    }
    pub(crate) fn validate_conversation_history_v17(
        &self,
        target: &PrWriteTarget,
        parent: &crate::providers::gh::PrResourceParentV1,
        expected: &ConversationHistoryV17,
    ) -> Result<()> {
        let actual = self.conversation_history_v17(target, parent)?;
        if expected.database != actual.database
            || expected.generation != actual.generation
            || expected.complete != actual.complete
            || expected.rows.len() != actual.rows.len()
        {
            bail!("Exact original Conversation history/body/generation moved");
        }
        for (a, b) in expected.rows.iter().zip(&actual.rows) {
            if !same_marker_v17(&a.marker, &b.marker)
                || bounded_encode(&a.origin)? != bounded_encode(&b.origin)?
                || a.body != b.body
            {
                bail!("Exact original Conversation history/body/generation moved");
            }
        }
        Ok(())
    }
    pub(crate) fn prepare_conversation_inspection_v17(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
    ) -> Result<ConversationRecoveryWitnessV17> {
        let marker = self
            .original_publication(target, reference)?
            .context("Exact Conversation attempt unavailable")?;
        let origin = self.conversation_origin_v17(target, &marker)?;
        let blocker = self
            .blocking(target)?
            .context("No original blocking Conversation attempt")?;
        if !same_marker_v17(&marker, &blocker)
            || !matches!(
                marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            )
        {
            bail!("Only the exact supported current Conversation blocker can be inspected for ACK");
        }
        let mut witness = ConversationRecoveryWitnessV17 {
            target: target.clone(),
            marker,
            origin,
            generation: self.source_generation(target)?,
            exact: None,
        };
        witness.exact = Some(exact_ack_v17::ExactAckRowV17::conversation(self, &witness)?);
        self.validate_conversation_inspection_v17(&witness)?;
        Ok(witness)
    }
    pub(crate) fn validate_conversation_inspection_v17(
        &self,
        witness: &ConversationRecoveryWitnessV17,
    ) -> Result<()> {
        let marker = self
            .original_publication(&witness.target, &witness.marker.publication_reference())?
            .context("Original blocker unavailable")?;
        let blocker = self
            .blocking(&witness.target)?
            .context("Original Conversation no longer blocks")?;
        let origin = self.conversation_origin_v17(&witness.target, &marker)?;
        if self.source_generation(&witness.target)? != witness.generation
            || !same_marker_v17(&marker, &witness.marker)
            || !same_marker_v17(&blocker, &witness.marker)
            || bounded_encode(&origin)? != bounded_encode(&witness.origin)?
        {
            bail!("Original Conversation inspection changed; no release");
        }
        if let Some(row) = &witness.exact {
            row.validate_current(self)?;
        }
        Ok(())
    }
    pub(crate) fn begin_conversation_v17(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        seed: &ConversationOriginSeedV17,
    ) -> Result<PrWritePermit> {
        let values = intent
            .values
            .iter()
            .filter(|(k, _)| k.as_str() != "origin-v17")
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if !seed.matches_database(&self.path)?
            || seed.scope != target.scope
            || seed.repository != target.repository
            || seed.pr_node != target.pr_node_id
            || seed.number != target.number
            || seed.write(&values)?.digest()? != intent.digest()?
        {
            bail!("Exact actual Conversation origin/durable intent differs");
        }
        self.begin_with_observed_origin_v2(
            target,
            intent,
            Some(ObservedOriginV2::ConversationV17(seed)),
        )
    }
}

fn same_marker_v17(a: &PrWriteMarker, b: &PrWriteMarker) -> bool {
    a.publication_reference() == b.publication_reference()
        && a.target == b.target
        && a.command == b.command
        && a.selected_resource == b.selected_resource
        && a.state == b.state
        && a.confirmed == b.confirmed
        && a.created_at == b.created_at
}
pub(crate) struct ConversationHistoryRowV17 {
    pub marker: PrWriteMarker,
    pub origin: conversation_action_lineage_v18::ObservedConversationOriginV18,
    pub body: Option<(String, BTreeMap<String, String>)>,
}
pub(crate) struct ConversationHistoryV17 {
    database: DatabaseV3,
    pub generation: i64,
    pub complete: bool,
    pub rows: Vec<ConversationHistoryRowV17>,
}
pub(crate) struct ConversationRecoveryWitnessV17 {
    pub(super) target: PrWriteTarget,
    pub(super) marker: PrWriteMarker,
    pub(super) generation: i64,
    pub(crate) origin: conversation_action_lineage_v18::ObservedConversationOriginV18,
    pub(super) exact: Option<exact_ack_v17::ExactAckRowV17>,
}
impl ConversationRecoveryWitnessV17 {
    pub(crate) fn target(&self) -> &PrWriteTarget {
        &self.target
    }
}
