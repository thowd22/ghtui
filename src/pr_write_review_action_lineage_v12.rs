//! Explicit new review actions. These bounded observations are NOT source,
//! approval, retry, or reconciliation capabilities. Old V3 records stay V3.
use super::review_operation_history_v3::{bounded_encode, DatabaseV3, ReviewOperationOriginSeedV3};
use super::*;
use crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::{
    operations_v3::ScopedReviewOperationIntentV3, ReviewRequestedActionV2, ScopedReviewIntentV2,
    ScopedReviewSelectionV2,
};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservationV12 {
    version: u8,
    scope: Scope,
    repository: String,
    repository_id: u64,
    repository_node: String,
    pr_id: u64,
    pr_node: String,
    number: u64,
    base_key: String,
    action: String,
    command: String,
    selected_review: Option<u64>,
}
/// Constructed ONLY from an actual selected producer. Persisted observations
/// cannot construct this value or a genuine original navigation.
#[derive(Clone, Debug)]
pub(crate) struct ReviewActionBaseV12 {
    observation: ObservationV12,
}
impl ReviewActionBaseV12 {
    pub(crate) fn review(
        intent: &Arc<ScopedReviewIntentV2>,
        selection: Option<&Arc<ScopedReviewSelectionV2>>,
        action: ReviewRequestedActionV2,
    ) -> Result<Self> {
        if (action == ReviewRequestedActionV2::CreatePending) != selection.is_none()
            || selection.is_some_and(|selected| !Arc::ptr_eq(selected.intent(), intent))
        {
            bail!("New review action requires the exact actual selected producer");
        }
        let key = selection
            .map_or_else(
                || intent.draft_key(),
                |selected| selected.action_key(action),
            )
            .map_err(anyhow::Error::msg)?;
        Self::from_actual(
            intent,
            key,
            action.draft_tag(),
            action.command(),
            selection.map(|s| s.snapshot().review().id()),
        )
    }
    pub(crate) fn operation(intent: &Arc<ScopedReviewOperationIntentV3>) -> Result<Self> {
        let command = intent.action().command();
        Self::from_actual(
            intent.original(),
            intent.draft_key().map_err(anyhow::Error::msg)?,
            command,
            command,
            None,
        )
    }
    fn from_actual(
        intent: &Arc<ScopedReviewIntentV2>,
        key: String,
        action: &str,
        command: &str,
        selected_review: Option<u64>,
    ) -> Result<Self> {
        let parent = intent.parent();
        let result = Self {
            observation: ObservationV12 {
                version: 12,
                scope: intent.scope().clone(),
                repository: parent.repository().into(),
                repository_id: parent.repository_id(),
                repository_node: parent.repository_node().into(),
                pr_id: parent.database_id(),
                pr_node: parent.node_id().into(),
                number: parent.number(),
                base_key: key,
                action: action.into(),
                command: command.into(),
                selected_review,
            },
        };
        result.observation.validate()?;
        Ok(result)
    }
    pub(crate) fn key(&self) -> &str {
        &self.observation.base_key
    }
    pub(crate) fn action(&self) -> &str {
        &self.observation.action
    }
    pub(crate) fn scope(&self) -> &Scope {
        &self.observation.scope
    }
    fn target(&self) -> Result<PrWriteTarget> {
        PrWriteTarget::for_source(
            &self.observation.scope,
            &self.observation.repository,
            &self.observation.pr_node,
            self.observation.number,
        )
    }
}
impl ObservationV12 {
    fn validate(&self) -> Result<()> {
        let positive = |n: u64| n > 0 && n <= i64::MAX as u64;
        let digest = |value: &str| {
            value.len() == 64
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        };
        let command = match self.action.as_str() {
            "create" => "review-create-pending-v2",
            "summary" => "review-update-pending-v2",
            "comment" | "approve" | "request-changes" => "review-submit-v2",
            "discard" => "review-discard-v2",
            "dismiss" => "review-dismiss-v2",
            "review-pending-comment-add-v3"
            | "review-pending-comment-edit-v3"
            | "review-pending-comment-delete-v3"
            | "review-rerequest-v3" => self.action.as_str(),
            _ => bail!("Unknown closed review action"),
        };
        if self.version != 12
            || self.command != command
            || self.base_key.is_empty()
            || self.base_key.len() > 1024
            || !(self.base_key.starts_with("review-draft/")
                || self.base_key.starts_with("review-operation-v3/"))
            || !positive(self.repository_id)
            || !positive(self.pr_id)
            || !positive(self.number)
            || self.selected_review.is_some_and(|id| !positive(id))
            || self.repository_node.len() > 1024
            || self.pr_node.len() > 1024
            || self.base_key.contains('\0')
        {
            bail!("Malformed bounded review action observation");
        }
        canonical_identifier(&self.repository_node)?;
        canonical_identifier(&self.pr_node)?;
        let key_valid = if self.command.ends_with("-v3") {
            self.selected_review.is_none()
                && self
                    .base_key
                    .strip_prefix("review-operation-v3/")
                    .is_some_and(digest)
        } else if self.action == "create" {
            self.selected_review.is_none()
                && self
                    .base_key
                    .strip_prefix("review-draft/")
                    .is_some_and(digest)
        } else {
            self.base_key
                .strip_prefix("review-draft/")
                .and_then(|rest| rest.split_once("/review/"))
                .is_some_and(|(root, selected)| {
                    digest(root)
                        && selected.split_once('/').is_some_and(|(id, tag)| {
                            tag == self.action
                                && id
                                    .parse::<u64>()
                                    .ok()
                                    .filter(|value| positive(*value) && value.to_string() == id)
                                    == self.selected_review
                                && self.selected_review.is_some()
                        })
                })
        };
        if !key_valid {
            bail!("Closed actual action key/member/tag mismatch");
        }
        PrWriteTarget::for_source(&self.scope, &self.repository, &self.pr_node, self.number)?;
        bounded_encode(self)?;
        Ok(())
    }
    fn matches_marker(&self, marker: &PrWriteMarker) -> Result<()> {
        self.validate()?;
        if marker.target.scope() != &self.scope
            || marker.target.repository() != self.repository
            || marker.target.node_id() != self.pr_node
            || marker.target.number() != self.number
            || marker.command != self.command
        {
            bail!("Historical action belongs to another exact target/purpose");
        }
        if let Some(review) = self.selected_review {
            if marker.selected_resource != format!("review/{review}") {
                bail!("Historical selected review differs");
            }
        } else if self.action == "create" && marker.selected_resource != self.base_key {
            bail!("Historical create belongs to another actual draft key");
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum TerminalKindV12 {
    Acknowledged,
    Confirmed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TerminalV12 {
    reference: PrPublicationReference,
    kind: TerminalKindV12,
    receipt: Option<PrConfirmedEffect>,
}
/// Non-cloneable revision witness. Arc sharing retains ONE observation; it
/// never grants a write. Every New/Preview/begin revalidates original SQL.
#[derive(Debug)]
pub(crate) struct ReviewAcknowledgedRevisionV12 {
    database: DatabaseV3,
    base: ReviewActionBaseV12,
    terminal: TerminalV12,
    generation: i64,
    key: String,
    protected_fingerprint: Option<String>,
    protected_key: Option<String>,
    parent_ack: Option<i64>,
}
impl ReviewAcknowledgedRevisionV12 {
    pub(crate) fn key(&self) -> &str {
        &self.key
    }
    pub(crate) fn base_key(&self) -> &str {
        self.base.key()
    }
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        &self.terminal.reference
    }
    pub(crate) fn label(&self) -> &'static str {
        match self.terminal.kind {
            TerminalKindV12::Acknowledged => "Acknowledged original attempt",
            TerminalKindV12::Confirmed => "Confirmed original receipt — retained cleanup history",
        }
    }
    pub(crate) fn bind_write(&self, write: &mut PrWriteIntent) -> Result<()> {
        bind_revision(&self.base.observation, &self.terminal, &self.key, write)
    }
    pub(crate) fn validate(&self, database: &Path, base: &ReviewActionBaseV12) -> Result<()> {
        if self.database != DatabaseV3::capture(database)?
            || self.base.observation != base.observation
        {
            bail!("Original action revision/database/generation changed");
        }
        let journal = PrWriteJournal::open(database)?;
        if journal.source_generation(&base.target()?)? != self.generation {
            bail!("Original action revision generation changed");
        }
        let current = journal
            .action_revision_v12(base)?
            .context("Original terminal action disappeared")?;
        if current.key != self.key
            || current.terminal != self.terminal
            || current.protected_fingerprint != self.protected_fingerprint
            || current.protected_key != self.protected_key
            || current.parent_ack != self.parent_ack
        {
            bail!("Exact terminal action or protected envelope changed");
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewActionOriginSeedV12 {
    database: DatabaseV3,
    observation: ObservationV12,
    terminal: TerminalV12,
    key: String,
    generation: i64,
    parent_ack: Option<i64>,
    protected_fingerprint: Option<String>,
    protected_key: Option<String>,
    original_v3: Option<ReviewOperationOriginSeedV3>,
    selected_snapshot: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewActionOriginRecordV12 {
    seed: ReviewActionOriginSeedV12,
    reference: PrPublicationReference,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) enum ReviewObservedOriginV12 {
    Legacy(Box<super::review_operation_history_v3::ReviewOperationOriginRecordV3>),
    Lineage(Box<ReviewActionOriginRecordV12>),
}
impl ReviewObservedOriginV12 {
    /// Pure captured-index validation, not a source/lineage constructor. ACK
    /// captures these exact nullable bytes BEFORE its original final UID.
    pub(super) fn validate_ack_index_v17(
        &self,
        connection: &Connection,
        target: &PrWriteTarget,
        attempt: i64,
        base_key: Option<&str>,
        action: Option<&str>,
        parent_ack: Option<i64>,
    ) -> Result<()> {
        match self {
            Self::Legacy(_) if base_key.is_none() && action.is_none() && parent_ack.is_none() => {
                Ok(())
            }
            Self::Legacy(_) => bail!("Legacy origin cannot adopt nullable V12 ACK index metadata"),
            Self::Lineage(record) => {
                if base_key != Some(record.seed.observation.base_key.as_str())
                    || action != Some(record.seed.observation.action.as_str())
                    || parent_ack != record.seed.parent_ack
                {
                    bail!("Exact original lineage/index metadata differs before ACK");
                }
                validate_ancestor(connection, target, &record.seed, attempt)
            }
        }
    }
    pub(crate) fn metadata(&self)->Option<&crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::operations_v3::metadata_v3::OriginalReviewOperationMetadataV3>{
        match self {
            Self::Legacy(record) => Some(record.seed.metadata()),
            Self::Lineage(record) => record.seed.original_v3.as_ref().map(|seed| seed.metadata()),
        }
    }
    pub(crate) fn key(&self) -> &str {
        match self {
            Self::Legacy(record) => record.seed.metadata().draft_key(),
            Self::Lineage(record) => record.key(),
        }
    }
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        match self {
            Self::Legacy(record) => &record.reference,
            Self::Lineage(record) => record.reference(),
        }
    }
    pub(crate) fn history_lines(&self) -> Vec<String> {
        match self {
            Self::Legacy(record) => record.seed.metadata().history_lines(),
            Self::Lineage(record) => record.history_lines(),
        }
    }
    pub(crate) fn requested_text(&self, encoded: &str, marker: &PrWriteMarker) -> Result<String> {
        match self {
            Self::Lineage(record) => record.requested_text(encoded, marker),
            Self::Legacy(record) => {
                let(body,reference)=crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::operations_v3::draft_v3::decode(encoded).map_err(anyhow::Error::msg)?;
                if reference.as_ref() != Some(&record.reference)
                    || record
                        .seed
                        .metadata()
                        .write(&body)
                        .map_err(anyhow::Error::msg)?
                        .digest()?
                        != marker.payload_sha256
                {
                    bail!("Exact protected V3 body/reference changed");
                }
                Ok(body)
            }
        }
    }
    pub(crate) fn write(&self, body: &str) -> Result<PrWriteIntent> {
        match self {
            Self::Legacy(record) => record
                .seed
                .metadata()
                .write(body)
                .map_err(anyhow::Error::msg),
            Self::Lineage(record) => {
                let original = record
                    .seed
                    .original_v3
                    .as_ref()
                    .context("V2 lineage is not a V3 observation")?;
                let mut write = original
                    .metadata()
                    .write(body)
                    .map_err(anyhow::Error::msg)?;
                bind_revision(
                    &record.seed.observation,
                    &record.seed.terminal,
                    &record.seed.key,
                    &mut write,
                )?;
                Ok(write)
            }
        }
    }
    // Keep independently validated original database/target/marker/numeric IDs
    // explicit at this private decoding boundary.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn decode(
        encoded: &str,
        lineage: bool,
        database: &Path,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
        repo: u64,
        node: &str,
        pr: u64,
    ) -> Result<Self> {
        if lineage {
            return Ok(Self::Lineage(Box::new(
                ReviewActionOriginRecordV12::decode(
                    encoded, database, target, marker, repo, node, pr,
                )?,
            )));
        }
        let record: super::review_operation_history_v3::ReviewOperationOriginRecordV3 =
            serde_json::from_str(encoded)?;
        record.seed.validate_marker_v12(database, target, marker)?;
        let metadata = record.seed.metadata();
        if record.reference != marker.publication_reference()
            || metadata.repository_id() != repo
            || metadata.repository_node() != node
            || metadata.pr_id() != pr
        {
            bail!("Exact V3 origin/ref/index identity differs");
        }
        if marker.state == PrWriteState::Confirmed {
            let effect = marker
                .confirmed
                .as_ref()
                .context("Actual original receipt missing")?;
            if !(if marker.command == "review-rerequest-v3" {
                metadata.matches_reviewer_effect(effect)
            } else {
                metadata.matches_pending_effect(effect)
            }) {
                bail!("Actual V3 original receipt differs from selected member");
            }
        }
        Ok(Self::Legacy(Box::new(record)))
    }
}
impl ReviewActionOriginRecordV12 {
    pub(super) fn decode(
        encoded: &str,
        database: &Path,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
        repo: u64,
        node: &str,
        pr: u64,
    ) -> Result<Self> {
        if encoded.len() > 16 * 1024 {
            bail!("Action origin exceeds16KiB");
        }
        let record: Self = serde_json::from_str(encoded)?;
        record.validate(database, target, marker, repo, node, pr)?;
        Ok(record)
    }
    fn validate(
        &self,
        database: &Path,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
        repo: u64,
        node: &str,
        pr: u64,
    ) -> Result<()> {
        self.seed.observation.matches_marker(marker)?;
        if self.seed.database != DatabaseV3::capture(database)?
            || self.reference != marker.publication_reference()
            || self.seed.observation.repository_id != repo
            || self.seed.observation.repository_node != node
            || self.seed.observation.pr_id != pr
            || self.seed.selected_snapshot.is_some()
                != self.seed.observation.selected_review.is_some()
            || next_key(&self.seed.observation, &self.seed.terminal)? != self.seed.key
            || self.seed.terminal.reference.attempt_id <= 0
            || self.seed.terminal.reference.attempt_id >= marker.attempt_id
            || self.seed.terminal.reference.pr_node_id != target.node_id()
            || !original_publication::full_hex(
                &self.seed.terminal.reference.observed_head,
                &[40, 64],
            )
            || !original_publication::full_hex(&self.seed.terminal.reference.payload_sha256, &[64])
            || match self.seed.terminal.kind {
                TerminalKindV12::Acknowledged => {
                    self.seed.parent_ack != Some(self.seed.terminal.reference.attempt_id)
                }
                TerminalKindV12::Confirmed => self
                    .seed
                    .parent_ack
                    .is_some_and(|id| id >= self.seed.terminal.reference.attempt_id),
            }
            || self
                .seed
                .parent_ack
                .is_some_and(|id| id <= 0 || id >= marker.attempt_id)
            || self
                .seed
                .selected_snapshot
                .as_ref()
                .is_some_and(|digest| !original_publication::full_hex(digest, &[64]))
        {
            bail!("Closed original V12 origin/index/reference/ancestor mismatch");
        }
        if let Some(original) = &self.seed.original_v3 {
            original.validate_marker_v12(database, target, marker)?;
            if original.metadata().draft_key() != self.seed.observation.base_key
                || original.metadata().repository_id() != repo
                || original.metadata().repository_node() != node
                || original.metadata().pr_id() != pr
            {
                bail!("Actual V3 selected observation differs from V12 action base");
            }
        } else if self.seed.observation.command.ends_with("-v3") {
            bail!("V3 action lacks its actual closed original observation");
        }
        if marker.state == PrWriteState::Confirmed {
            terminal(
                marker,
                &self.seed.observation,
                self.seed.original_v3.as_ref(),
            )?;
        }
        bounded_encode(self)?;
        Ok(())
    }
    pub(crate) fn key(&self) -> &str {
        &self.seed.key
    }
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        &self.reference
    }
    pub(crate) fn history_lines(&self) -> Vec<String> {
        let mut lines=vec![format!("Explicit new-action lineage v12 • action {} • original terminal #{} {:?}",
            self.seed.observation.action,self.seed.terminal.reference.attempt_id,self.seed.terminal.kind),
            "Historical observation only. No resend, source, approval, or write permission comes from lineage.".into()];
        if let Some(original) = &self.seed.original_v3 {
            lines.extend(original.metadata().history_lines());
        }
        lines
    }
    pub(crate) fn requested_text(&self, encoded: &str, marker: &PrWriteMarker) -> Result<String> {
        if reference(encoded)?.as_ref() != Some(&self.reference)
            || marker.publication_reference() != self.reference
        {
            bail!("Exact original lineage body/reference differs");
        }
        let (text, mut write) = if let Some(original) = &self.seed.original_v3 {
            let (body,_)=crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::operations_v3::draft_v3::decode(encoded).map_err(anyhow::Error::msg)?;
            let write = original
                .metadata()
                .write(&body)
                .map_err(anyhow::Error::msg)?;
            (body, write)
        } else {
            use crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::ReviewDraftValuesV2;
            let raw: serde_json::Value = serde_json::from_str(encoded)?;
            let values =
                ReviewDraftValuesV2::decode_values(&serde_json::to_string(&raw["values"])?)
                    .map_err(anyhow::Error::msg)?;
            let mut fields = BTreeMap::from([("summary".into(), values.summary().into())]);
            if self.seed.observation.action == "create" {
                fields.insert(
                    "batch".into(),
                    values.encoded_values().map_err(anyhow::Error::msg)?,
                );
            }
            if let Some(snapshot) = &self.seed.selected_snapshot {
                fields.insert("selected_snapshot_sha256".into(), snapshot.clone());
            }
            let mut text = values.summary().to_owned();
            for comment in values.comments() {
                text.push('\n');
                text.push_str(comment.body());
            }
            (
                text,
                PrWriteIntent {
                    command: marker.command.clone(),
                    selected_resource: marker.selected_resource.clone(),
                    observed_head: marker.observed_head.clone(),
                    values: fields,
                },
            )
        };
        bind_revision(
            &self.seed.observation,
            &self.seed.terminal,
            &self.seed.key,
            &mut write,
        )?;
        if write.digest()? != marker.payload_sha256 {
            bail!("Actual protected V12 requested text fingerprint differs");
        }
        Ok(text)
    }
}
impl ReviewActionOriginSeedV12 {
    /// Extra lineage check INSIDE the existing original SQL guard. Own newer
    /// generation/state remains checked by validate_owned; this never renews
    /// the old witness or turns stored metadata into source permission.
    pub(crate) fn validate_owned_basis(
        &self,
        database: &Path,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
    ) -> Result<()> {
        if self.database != DatabaseV3::capture(database)? {
            bail!("Original action database replaced");
        }
        let journal = PrWriteJournal::open(database)?;
        let original = journal
            .original_publication(target, &self.terminal.reference)?
            .context("Original terminal action disappeared")?;
        if terminal(&original, &self.observation, self.original_v3.as_ref())? != self.terminal {
            bail!("Original terminal observation changed");
        }
        let current = journal
            .review_observed_origin_v12(target, &marker.publication_reference())?
            .context("Exact current lineage missing")?;
        let ReviewObservedOriginV12::Lineage(current) = current else {
            bail!("Current original lineage has another closed version");
        };
        if bounded_encode(&current.seed)? != bounded_encode(self)? {
            bail!("Current stored lineage changed");
        }
        if let (Some(key), Some(expected)) = (&self.protected_key, &self.protected_fingerprint) {
            let saved = Cache::open(database)?
                .bounded_draft(target.scope(), key, 128 * 1024)?
                .context("Original protected body disappeared")?;
            if format!("{:x}", Sha256::digest(saved.as_bytes())) != *expected {
                bail!("Original protected envelope replaced; retain all rows");
            }
        }
        Ok(())
    }
    pub(crate) fn from_revision(
        revision: &ReviewAcknowledgedRevisionV12,
        original_v3: Option<&ReviewOperationOriginSeedV3>,
        write: &PrWriteIntent,
    ) -> Result<Self> {
        if write
            .values
            .get("selected_snapshot_sha256")
            .is_some_and(|s| !original_publication::full_hex(s, &[64]))
        {
            bail!("Actual selected snapshot exceeds its scalar bound before clone");
        }
        let result = Self {
            database: revision.database.clone(),
            observation: revision.base.observation.clone(),
            terminal: revision.terminal.clone(),
            key: revision.key.clone(),
            generation: revision.generation,
            parent_ack: revision.parent_ack,
            protected_fingerprint: revision.protected_fingerprint.clone(),
            protected_key: revision.protected_key.clone(),
            original_v3: original_v3.cloned(),
            selected_snapshot: write.values.get("selected_snapshot_sha256").cloned(),
        };
        if result
            .selected_snapshot
            .as_ref()
            .is_some_and(|s| !original_publication::full_hex(s, &[64]))
        {
            bail!("Actual selected snapshot digest malformed");
        }
        bounded_encode(&result)?;
        Ok(result)
    }
    fn matches(
        &self,
        database: &Path,
        target: &PrWriteTarget,
        write: &PrWriteIntent,
    ) -> Result<()> {
        let expected = PrWriteTarget::for_source(
            &self.observation.scope,
            &self.observation.repository,
            &self.observation.pr_node,
            self.observation.number,
        )?;
        if expected != *target || self.database != DatabaseV3::capture(database)? {
            bail!("Original lineage target/database mismatch");
        }
        let mut plain = write.clone();
        plain
            .values
            .remove("explicit-new-review-action-v12")
            .context("Explicit action revision fingerprint missing")?;
        if plain.values.get("selected_snapshot_sha256") != self.selected_snapshot.as_ref() {
            bail!("Actual selected snapshot fingerprint differs");
        }
        if let Some(original) = &self.original_v3 {
            original.matches(database, target, &plain)?;
        }
        bind_revision(&self.observation, &self.terminal, &self.key, &mut plain)?;
        if plain.digest()? != write.digest()? {
            bail!("Original explicit revision fingerprint differs");
        }
        Ok(())
    }
}
// Pure historical digest reconstruction is deliberately NOT a revision
// constructor. Only action_revision_v12 issues a fresh current SQL witness.
fn bind_revision(
    observation: &ObservationV12,
    terminal: &TerminalV12,
    key: &str,
    write: &mut PrWriteIntent,
) -> Result<()> {
    observation.validate()?;
    if write.command != observation.command
        || observation
            .selected_review
            .is_some_and(|id| write.selected_resource != format!("review/{id}"))
        || observation.action == "create" && write.selected_resource != observation.base_key
    {
        bail!("Exact current action/selector mismatch");
    }
    let encoded = bounded_encode(&(observation, terminal, key))?;
    write.values.insert(
        "explicit-new-review-action-v12".into(),
        format!("{:x}", Sha256::digest(encoded.as_bytes())),
    );
    write.digest()?;
    Ok(())
}
pub(super) fn migrate(tx: &Transaction<'_>) -> Result<()> {
    let columns = {
        let mut statement =
            tx.prepare("PRAGMA table_info(pr_write_review_operation_origins_v3)")?;
        let names = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        names
    };
    for (column, kind) in [
        ("base_key_v12", "TEXT"),
        ("action_tag_v12", "TEXT"),
        ("parent_ack_attempt_v12", "INTEGER"),
    ] {
        if !columns.iter().any(|name| name == column) {
            tx.execute_batch(&format!(
                "ALTER TABLE pr_write_review_operation_origins_v3 ADD COLUMN {column} {kind}"
            ))?;
        }
    }
    tx.execute_batch("CREATE INDEX IF NOT EXISTS pr_write_review_action_latest_v12 ON pr_write_review_operation_origins_v3(base_key_v12,action_tag_v12,attempt_id DESC)")?;
    Ok(())
}
fn terminal(
    marker: &PrWriteMarker,
    base: &ObservationV12,
    original: Option<&ReviewOperationOriginSeedV3>,
) -> Result<TerminalV12> {
    base.matches_marker(marker)?;
    let kind=match(marker.state,marker.confirmed.as_ref()) {
        (PrWriteState::Acknowledged,None)=>TerminalKindV12::Acknowledged,
        (PrWriteState::Confirmed,Some(effect))=>{
            let valid=if let Some(original)=original {if marker.command=="review-rerequest-v3" {original.metadata().matches_reviewer_effect(effect)}
                else {original.metadata().matches_pending_effect(effect)}}else{
                let positive=effect.resource_id.parse::<u64>().ok().filter(|id|*id>0&&id.to_string()==effect.resource_id);
                effect.kind==if base.action=="discard" {PrEffectKind::ReviewDiscarded}else{PrEffectKind::Review}
                    && positive.is_some() && effect.node_id.as_ref().is_some_and(|node|!node.is_empty()&&node.len()<=1024)
                    && base.selected_review.is_none_or(|id|Some(id)==positive)
            };
            if !valid {bail!("Confirmed history lacks exact closed typed receipt");} TerminalKindV12::Confirmed
        },_=>bail!("Pending/uncertain history cannot open a new action; inspect then explicitly acknowledge first"),
    };
    Ok(TerminalV12 {
        reference: marker.publication_reference(),
        kind,
        receipt: marker.confirmed.clone(),
    })
}
fn next_key(base: &ObservationV12, terminal: &TerminalV12) -> Result<String> {
    let encoded = bounded_encode(&(
        "ghtui-explicit-new-review-action-v12",
        &base.base_key,
        &base.action,
        &terminal.reference,
    ))?;
    Ok(format!(
        "{}/new-v12/{:x}",
        &base.base_key,
        Sha256::digest(encoded.as_bytes())
    ))
}
fn reference(encoded: &str) -> Result<Option<PrPublicationReference>> {
    if encoded.len() > 128 * 1024 {
        bail!("Protected draft exceeds128KiB");
    }
    let value: serde_json::Value = serde_json::from_str(encoded)?;
    let fields = value
        .as_object()
        .context("Protected action draft is not an object")?;
    if fields.len() != 3
        || value.get("version").and_then(|v| v.as_u64()) != Some(3)
        || !fields.contains_key("publication")
        || !(fields.contains_key("body") || fields.contains_key("values"))
    {
        bail!("Protected action envelope malformed");
    }
    if value["publication"].is_null() {
        return Ok(None);
    }
    let fields = value["publication"]
        .as_object()
        .context("Protected reference malformed")?;
    if fields.len() != 4
        || ![
            "attempt_id",
            "pr_node_id",
            "observed_head",
            "payload_sha256",
        ]
        .iter()
        .all(|key| fields.contains_key(*key))
    {
        bail!("Protected reference has unknown fields");
    }
    if fields
        .get("attempt_id")
        .and_then(|v| v.as_i64())
        .is_none_or(|id| id <= 0)
        || fields
            .get("pr_node_id")
            .and_then(|v| v.as_str())
            .is_none_or(|node| {
                node.is_empty() || node.len() > 1024 || canonical_identifier(node).is_err()
            })
        || fields
            .get("observed_head")
            .and_then(|v| v.as_str())
            .is_none_or(|sha| !original_publication::full_hex(sha, &[40, 64]))
        || fields
            .get("payload_sha256")
            .and_then(|v| v.as_str())
            .is_none_or(|sha| !original_publication::full_hex(sha, &[64]))
    {
        bail!("Protected reference exceeds exact scalar bounds before clone");
    }
    let result: PrPublicationReference = serde_json::from_value(value["publication"].clone())?;
    if result.attempt_id <= 0
        || result.pr_node_id.len() > 1024
        || !original_publication::full_hex(&result.observed_head, &[40, 64])
        || !original_publication::full_hex(&result.payload_sha256, &[64])
    {
        bail!("Protected reference exceeds exact identity bounds");
    }
    Ok(Some(result))
}
impl PrWriteJournal {
    pub(crate) fn review_observed_origin_v12(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
    ) -> Result<Option<ReviewObservedOriginV12>> {
        let database = DatabaseV3::capture(&self.path)?;
        let generation = self.source_generation(target)?;
        let Some(marker) = self.original_publication(target, reference)? else {
            return Ok(None);
        };
        let info:Option<(u64,u64,usize,usize,bool)>=self.connection.query_row("SELECT repository_id,pr_id,length(CAST(origin_json AS BLOB)),length(CAST(repository_node AS BLOB)),base_key_v12 IS NOT NULL FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",[marker.attempt_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        let Some((repo, pr, bytes, node_bytes, lineage)) = info else {
            return Ok(None);
        };
        if bytes > 16 * 1024 || node_bytes > 1024 {
            bail!("Observed origin exceeds bounded metadata before read");
        }
        let(encoded,node):(String,String)=self.connection.query_row("SELECT origin_json,repository_node FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1 AND length(CAST(origin_json AS BLOB))<=16384 AND length(CAST(repository_node AS BLOB))<=1024",[marker.attempt_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let record = ReviewObservedOriginV12::decode(
            &encoded, lineage, &self.path, target, &marker, repo, &node, pr,
        )?;
        if let ReviewObservedOriginV12::Lineage(record) = &record {
            validate_ancestor(&self.connection, target, &record.seed, marker.attempt_id)?;
        }
        if generation != self.source_generation(target)?
            || database != DatabaseV3::capture(&self.path)?
        {
            bail!("Observed original origin changed while reading");
        }
        Ok(Some(record))
    }
    /// Bounded purpose-FIRST newest row. Malformed newest metadata refuses;
    /// it is never filtered away to revive an older acknowledged attempt.
    pub(crate) fn action_revision_v12(
        &self,
        base: &ReviewActionBaseV12,
    ) -> Result<Option<Arc<ReviewAcknowledgedRevisionV12>>> {
        base.observation.validate()?;
        let database = DatabaseV3::capture(&self.path)?;
        let target = base.target()?;
        let generation = self.source_generation(&target)?;
        if self.source_blocking(&target)?.is_some() {
            bail!("Original PR has an unresolved publication; inspect that exact attempt first");
        }
        let latest:Option<(i64,usize)>=self.connection.query_row("SELECT attempt_id,length(CAST(origin_json AS BLOB)) FROM pr_write_review_operation_origins_v3 WHERE base_key_v12=?1 AND action_tag_v12=?2 ORDER BY attempt_id DESC LIMIT 1",params![base.key(),base.action()],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let cache = Cache::open(&self.path)?;
        let (marker, original, parent_ack, protected_key, protected_fingerprint) = if let Some((
            id,
            bytes,
        )) = latest
        {
            if bytes > 16 * 1024 {
                bail!("Newest action origin exceeds16KiB before materialization");
            }
            let encoded:String=self.connection.query_row("SELECT origin_json FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1 AND length(CAST(origin_json AS BLOB))<=16384",[id],|r|r.get(0))?;
            let record: ReviewActionOriginRecordV12 = serde_json::from_str(&encoded)?;
            if record.seed.database != database
                || record.seed.observation != base.observation
                || record.reference.attempt_id != id
                || next_key(&base.observation, &record.seed.terminal)? != record.seed.key
            {
                bail!("Newest action indexed origin mismatch");
            }
            let marker = self
                .original_publication(&target, &record.reference)?
                .context("Newest actual action marker missing")?;
            record.validate(
                &self.path,
                &target,
                &marker,
                base.observation.repository_id,
                &base.observation.repository_node,
                base.observation.pr_id,
            )?;
            validate_ancestor(&self.connection, &target, &record.seed, id)?;
            let scalars:(u64,String,u64)=self.connection.query_row("SELECT repository_id,repository_node,pr_id FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1 AND length(CAST(repository_node AS BLOB))<=1024",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
            if scalars
                != (
                    base.observation.repository_id,
                    base.observation.repository_node.clone(),
                    base.observation.pr_id,
                )
            {
                bail!("Newest exact indexed repository/PR scalars differ");
            }
            let parent = self
                .original_publication(&target, &record.seed.terminal.reference)?
                .context("Original terminal ancestor missing")?;
            if let Some(original) = &record.seed.original_v3 {
                original.validate_marker_v12(&self.path, &target, &parent)?;
            }
            let basis = terminal(&parent, &base.observation, record.seed.original_v3.as_ref())?;
            if parent.attempt_id >= id || basis != record.seed.terminal {
                bail!("Actual action ancestry is contradictory");
            }
            let indexed:Option<i64>=self.connection.query_row("SELECT parent_ack_attempt_v12 FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",[id],|r|r.get(0))?;
            if indexed != record.seed.parent_ack {
                bail!("Original action ACK index differs");
            }
            let saved = cache.bounded_draft(target.scope(), &record.seed.key, 128 * 1024)?;
            if saved
                .as_ref()
                .map(|s| reference(s))
                .transpose()?
                .flatten()
                .is_some_and(|r| r != record.reference)
            {
                bail!("Current protected action row differs from its actual journal reference");
            }
            if let Some(saved) = &saved {
                record.requested_text(saved, &marker)?;
            }
            (
                marker,
                record.seed.original_v3,
                record.seed.parent_ack,
                Some(record.seed.key),
                saved.map(|s| format!("{:x}", Sha256::digest(s.as_bytes()))),
            )
        } else {
            let Some(saved) = cache.bounded_draft(target.scope(), base.key(), 128 * 1024)? else {
                return Ok(None);
            };
            let Some(reference) = reference(&saved)? else {
                return Ok(None);
            };
            let marker = self
                .original_publication(&target, &reference)?
                .context("Exact original protected action marker missing")?;
            let original = if base.key().starts_with("review-operation-v3/") {
                let original = self
                    .review_operation_original_v3(&target, &reference)?
                    .context("Protected legacy operation lacks exact V3 origin")?;
                let metadata = original.seed.metadata();
                if metadata.draft_key() != base.key()
                    || metadata.repository_id() != base.observation.repository_id
                    || metadata.repository_node() != base.observation.repository_node
                    || metadata.pr_id() != base.observation.pr_id
                {
                    bail!("Legacy original operation differs from current actual action");
                }
                let (body, saved_reference) = crate::scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::operations_v3::draft_v3::decode(&saved)
                    .map_err(anyhow::Error::msg)?;
                if saved_reference.as_ref() != Some(&reference)
                    || metadata
                        .write(&body)
                        .map_err(anyhow::Error::msg)?
                        .digest()?
                        != marker.payload_sha256
                {
                    bail!("Legacy protected body/reference differs from the actual publication fingerprint");
                }
                Some(original.seed)
            } else {
                None
            };
            (
                marker,
                original,
                None,
                Some(base.key().into()),
                Some(format!("{:x}", Sha256::digest(saved.as_bytes()))),
            )
        };
        let terminal = terminal(&marker, &base.observation, original.as_ref())?;
        let parent_ack = if terminal.kind == TerminalKindV12::Acknowledged {
            Some(marker.attempt_id)
        } else {
            parent_ack
        };
        let key = next_key(&base.observation, &terminal)?;
        if let Some(saved) = cache.bounded_draft(target.scope(), &key, 128 * 1024)? {
            if reference(&saved)?.is_some() {
                bail!("Unindexed protected successor cannot be resumed or overwritten");
            }
        }
        if generation != self.source_generation(&target)?
            || database != DatabaseV3::capture(&self.path)?
        {
            bail!("Action lineage changed while reading exact SQL");
        }
        Ok(Some(Arc::new(ReviewAcknowledgedRevisionV12 {
            database,
            base: base.clone(),
            terminal,
            generation,
            key,
            parent_ack,
            protected_key,
            protected_fingerprint,
        })))
    }
    pub(crate) fn begin_review_action_v12(
        &mut self,
        target: &PrWriteTarget,
        write: &PrWriteIntent,
        seed: &ReviewActionOriginSeedV12,
    ) -> Result<PrWritePermit> {
        seed.matches(&self.path, target, write)?;
        self.begin_with_observed_origin_v2(
            target,
            write,
            Some(ObservedOriginV2::ReviewActionV12(seed)),
        )
    }
}
fn validate_ancestor(
    connection: &Connection,
    target: &PrWriteTarget,
    seed: &ReviewActionOriginSeedV12,
    child: i64,
) -> Result<()> {
    if let Some(ack) = seed.parent_ack {
        let bytes: usize = connection.query_row(
            &format!(
                "SELECT {} FROM pr_write_attempts WHERE attempt_id=?1",
                source_metadata::METADATA_BYTES_SQL
            ),
            [ack],
            |r| r.get(0),
        )?;
        if bytes > 8192 || ack <= 0 || ack >= child {
            bail!("Actual ACK ancestor missing/oversized/out of order");
        }
        let raw=connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1 AND host=?2 AND principal=?3 AND pr_node=?4",params![ack,target.scope.host,target.scope.login,target.pr_node_id],read_marker)?;
        let marker = decode_marker(target, raw)?;
        seed.observation.matches_marker(&marker)?;
        if marker.state != PrWriteState::Acknowledged || marker.confirmed.is_some() {
            bail!("Stored ACK ancestor is not actual acknowledged history");
        }
    }
    Ok(())
}
pub(super) fn validate_begin(
    tx: &Transaction<'_>,
    path: &Path,
    target: &PrWriteTarget,
    seed: &ReviewActionOriginSeedV12,
) -> Result<()> {
    if seed.database != DatabaseV3::capture(path)? || seed.generation != generation(tx, target)? {
        bail!("New action original database/generation changed before begin");
    }
    let selected:Option<i64>=tx.query_row("SELECT attempt_id FROM pr_write_review_operation_origins_v3 WHERE base_key_v12=?1 AND action_tag_v12=?2 ORDER BY attempt_id DESC LIMIT 1",params![seed.observation.base_key,seed.observation.action],|r|r.get(0)).optional()?;
    if selected.is_some_and(|id| id != seed.terminal.reference.attempt_id) {
        bail!("Newer same-action lineage exists; original revision cannot begin");
    }
    validate_ancestor(tx, target, seed, i64::MAX)?;
    let bytes: usize = tx.query_row(
        &format!(
            "SELECT {} FROM pr_write_attempts WHERE attempt_id=?1",
            source_metadata::METADATA_BYTES_SQL
        ),
        [seed.terminal.reference.attempt_id],
        |r| r.get(0),
    )?;
    if bytes > 8192 {
        bail!("Original terminal metadata exceeds8KiB before begin");
    }
    let row:(String,String,String,String,Option<String>)=tx.query_row("SELECT state,command,selected_resource,payload_sha256,confirmed_json FROM pr_write_attempts WHERE attempt_id=?1 AND host=?2 AND principal=?3 AND pr_node=?4 AND observed_head=?5",params![seed.terminal.reference.attempt_id,target.scope.host,target.scope.login,target.pr_node_id,seed.terminal.reference.observed_head],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    if row.1 != seed.observation.command
        || seed
            .observation
            .selected_review
            .is_some_and(|id| row.2 != format!("review/{id}"))
        || seed.observation.action == "create" && row.2 != seed.observation.base_key
        || seed.original_v3.as_ref().is_some_and(|origin| {
            !origin
                .metadata()
                .write("")
                .is_ok_and(|write| write.selected_resource == row.2)
        })
        || row.3 != seed.terminal.reference.payload_sha256
        || row
            .4
            .as_deref()
            .map(serde_json::from_str::<PrConfirmedEffect>)
            .transpose()?
            != seed.terminal.receipt
        || match seed.terminal.kind {
            TerminalKindV12::Acknowledged => row.0 != "acknowledged" || row.4.is_some(),
            TerminalKindV12::Confirmed => row.0 != "confirmed" || row.4.is_none(),
        }
    {
        bail!("Original terminal state/reference changed");
    }
    if let (Some(key), Some(expected)) = (&seed.protected_key, &seed.protected_fingerprint) {
        let bytes:Option<usize>=tx.query_row("SELECT length(CAST(body AS BLOB)) FROM drafts WHERE host=?1 AND login=?2 AND resource=?3",params![target.scope.host,target.scope.login,key],|r|r.get(0)).optional()?;
        if bytes.is_none_or(|n| n > 128 * 1024) {
            bail!("Original protected draft missing/oversized before begin");
        }
        let body:String=tx.query_row("SELECT body FROM drafts WHERE host=?1 AND login=?2 AND resource=?3 AND length(CAST(body AS BLOB))<=131072",params![target.scope.host,target.scope.login,key],|r|r.get(0))?;
        if format!("{:x}", Sha256::digest(body.as_bytes())) != *expected {
            bail!("Original protected draft replaced before begin");
        }
    }
    Ok(())
}
pub(super) fn insert(
    tx: &Transaction<'_>,
    seed: &ReviewActionOriginSeedV12,
    marker: &PrWriteMarker,
) -> Result<()> {
    let encoded = bounded_encode(&ReviewActionOriginRecordV12 {
        seed: seed.clone(),
        reference: marker.publication_reference(),
    })?;
    tx.execute("INSERT INTO pr_write_review_operation_origins_v3(attempt_id,repository_id,repository_node,pr_id,origin_json,base_key_v12,action_tag_v12,parent_ack_attempt_v12) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![marker.attempt_id,seed.observation.repository_id,seed.observation.repository_node,seed.observation.pr_id,encoded,seed.observation.base_key,seed.observation.action,seed.parent_ack])?;
    Ok(())
}
