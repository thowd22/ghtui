//! Durable duplicate-write protection. SQLite work belongs in application workers.
#[path = "pr_write_original.rs"]
mod original_publication;
#[path = "pr_write_review_operation_history_v3.rs"]
pub(crate) mod review_operation_history_v3;
#[path = "pr_source_metadata.rs"]
mod source_metadata;
#[path = "pr_write_thread_comment_history_v2.rs"]
pub(crate) mod thread_comment_history_v2;
#[path = "pr_write_thread_history_v1.rs"]
pub(crate) mod thread_history_v1;
// Closed producer-specific union, never an arbitrary transaction callback.
enum ObservedOriginV2<'a> {
    ThreadV1(&'a thread_history_v1::ThreadOriginSeedV1),
    CommentV2(&'a thread_comment_history_v2::ThreadCommentOriginSeedV2),
    ReviewOperationV3(&'a review_operation_history_v3::ReviewOperationOriginSeedV3),
}

use crate::{
    config::validate_repository,
    providers::{
        gh::{GithubProvider, HostIdentity, PrObservation, PrSummary},
        runner::CancellationToken,
    },
    storage::{now, Cache, Scope},
};
use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Transaction, TransactionBehavior};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Duration,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrWriteTarget {
    scope: Scope,
    pr_node_id: String,
    repository: String,
    number: u64,
}
impl PrWriteTarget {
    pub(crate) fn for_source(
        scope: &Scope,
        repository: &str,
        node: &str,
        number: u64,
    ) -> Result<Self> {
        let id = scope
            .login
            .strip_prefix("user-id:")
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|id| *id > 0)
            .context("Source journal guard requires a positive verified principal")?;
        if Scope::github(scope.host.to_ascii_lowercase(), Some(id))? != *scope {
            bail!("Source journal guard requires canonical host/principal scope");
        }
        validate_repository(repository)?;
        canonical_identifier(node)?;
        if number == 0 || number > i64::MAX as u64 {
            bail!("Source journal guard requires a positive bounded PR number");
        }
        Ok(Self {
            scope: scope.clone(),
            pr_node_id: node.into(),
            repository: repository.into(),
            number,
        })
    }
    pub fn new(identity: &HostIdentity, target: &PrSummary) -> Result<Self> {
        if !identity.host.eq_ignore_ascii_case(&target.pr.host) {
            bail!("PR and verified account belong to different hosts");
        }
        validate_repository(&target.pr.repository)?;
        if target.pr.number == 0 || target.pr.number > i64::MAX as u64 {
            bail!("PR number must be a positive SQLite-compatible identity");
        }
        canonical_identifier(&target.pr.id)?;
        Ok(Self {
            scope: Scope::github(identity.host.to_ascii_lowercase(), identity.account_id)?,
            pr_node_id: target.pr.id.clone(),
            repository: target.pr.repository.clone(),
            number: target.pr.number,
        })
    }
    pub fn scope(&self) -> &Scope {
        &self.scope
    }
    pub fn node_id(&self) -> &str {
        &self.pr_node_id
    }
    pub fn repository(&self) -> &str {
        &self.repository
    }
    pub fn number(&self) -> u64 {
        self.number
    }
    fn same_barrier(&self, other: &Self) -> bool {
        self.scope == other.scope && self.pr_node_id == other.pr_node_id
    }
}

#[derive(Clone, Debug)]
pub struct PrWriteIntent {
    pub command: String,
    pub selected_resource: String,
    pub observed_head: String,
    pub values: BTreeMap<String, String>,
}
impl PrWriteIntent {
    pub fn digest(&self) -> Result<String> {
        canonical_identifier(&self.command)?;
        if !self.selected_resource.is_empty() {
            canonical_identifier(&self.selected_resource)?;
        }
        if !matches!(self.observed_head.len(), 40 | 64)
            || !self
                .observed_head
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            bail!("Observed PR head must be an immutable Git identity");
        }
        let bytes = serde_json::to_vec(&(
            &self.command,
            &self.selected_resource,
            &self.observed_head,
            &self.values,
        ))?;
        if bytes.len() > 1024 * 1024 {
            bail!("PR write intent exceeds the 1 MiB fingerprint budget");
        }
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrWriteState {
    Pending,
    Uncertain,
    Confirmed,
    Acknowledged,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrEffectKind {
    Comment,
    CommentDeleted,
    Reaction,
    ReactionRemoved,
    Subscription,
    Metadata,
    Review,
    ReviewDiscarded,
    Thread,
    ViewedFile,
    Lifecycle,
    Merge,
    CreatedPr,
    BranchContent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrConfirmedEffect {
    pub kind: PrEffectKind,
    /// Canonical ID or static state, never body/title/token/transport diagnostics.
    pub resource_id: String,
    pub node_id: Option<String>,
}
#[derive(Clone, Debug)]
pub struct PrWriteMarker {
    pub attempt_id: i64,
    pub target: PrWriteTarget,
    pub command: String,
    pub selected_resource: String,
    pub observed_head: String,
    pub payload_sha256: String,
    pub state: PrWriteState,
    pub confirmed: Option<PrConfirmedEffect>,
    pub created_at: u64,
}
/// Original publication metadata stored alongside a draft, without a second body.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrPublicationReference {
    pub attempt_id: i64,
    pub pr_node_id: String,
    pub observed_head: String,
    pub payload_sha256: String,
}
impl PrWriteMarker {
    pub fn publication_reference(&self) -> PrPublicationReference {
        PrPublicationReference {
            attempt_id: self.attempt_id,
            pr_node_id: self.target.pr_node_id.clone(),
            observed_head: self.observed_head.clone(),
            payload_sha256: self.payload_sha256.clone(),
        }
    }
}

/// Producer-only, nonserializable permission to attempt exactly one publication.
/// Dropping it does not clear the durable pending record.
#[derive(Debug)]
pub struct PrWritePermit {
    issuer: PathBuf,
    marker: PrWriteMarker,
    claimed: AtomicBool,
}
impl PrWritePermit {
    fn claim(&self) -> Result<()> {
        if self
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            bail!("This PR publication permit was already used; inspect before retrying");
        }
        Ok(())
    }
    pub fn marker(&self) -> &PrWriteMarker {
        &self.marker
    }
}

/// Retains the exact provider-produced snapshot and local journal generation.
/// It cannot be recreated from a saved draft or user-supplied JSON.
#[derive(Debug)]
pub struct PrJournalInspection {
    issuer: PathBuf,
    target: PrWriteTarget,
    generation: i64,
    observation: PrObservation,
}
impl PrJournalInspection {
    pub fn observation(&self) -> &PrObservation {
        &self.observation
    }
    pub fn target(&self) -> &PrWriteTarget {
        &self.target
    }
}

pub struct PrWriteJournal {
    connection: Connection,
    path: PathBuf,
}
impl PrWriteJournal {
    pub(crate) fn database_path(&self) -> &Path {
        &self.path
    }
    pub(crate) fn source_generation(&self, target: &PrWriteTarget) -> Result<i64> {
        generation(&self.connection, target)
    }
    /// At most one bounded metadata-only receipt for an exact source category.
    /// Never read discussion records, drafts, bodies or transport diagnostics.
    pub(crate) fn latest_source_receipt(
        &self,
        target: &PrWriteTarget,
        category: crate::storage::SyncBindingKind,
    ) -> Result<Option<PrWriteMarker>> {
        use crate::storage::SyncBindingKind::*;
        // Known thread PURPOSE is selected before shape validation so a newer
        // malformed receipt cannot revive older already-fenced evidence. The
        // closed thread helper still requires exact lowercase digest, command,
        // effect and node before any source fence/private read admission.
        // New review commands enter BOTH relevant selectors by purpose before
        // parsing. Never filter malformed owned resources out and revive an
        // older apparently valid receipt; the closed impact validator refuses.
        let selector=match category {PullRequestBody=>"(command IN('legacy-title','legacy-description'))",ConversationComment=>"(command IN('new-comment','edit-comment','delete-comment') AND (selected_resource LIKE 'Conversation/%' OR (command='new-comment' AND selected_resource='pr')))",Review=>"(command IN('legacy-approve','legacy-request-changes','legacy-review-comment','review-create-pending-v2','review-update-pending-v2','review-submit-v2','review-discard-v2','review-dismiss-v2','review-pending-comment-add-v3','review-pending-comment-edit-v3','review-pending-comment-delete-v3'))",InlineComment=>"((command IN('new-comment','edit-comment','delete-comment') AND selected_resource LIKE 'Review/%') OR (command='inline-comment' AND selected_resource LIKE 'inline/%' AND length(selected_resource)=71 AND substr(selected_resource,8) NOT GLOB '*[^0-9a-f]*') OR command IN('review-create-pending-v2','review-submit-v2','review-discard-v2','thread-reply','thread-resolve','thread-reopen','thread-comment-edit-v2','thread-comment-delete-v2','review-pending-comment-add-v3','review-pending-comment-edit-v3','review-pending-comment-delete-v3'))",IssueBody=>return Ok(None)};
        let bounded = source_metadata::METADATA_BYTES_SQL;
        let sql=format!("SELECT attempt_id,{bounded} FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND state='confirmed' AND {selector} ORDER BY attempt_id DESC LIMIT 1");
        let found: Option<(i64, usize)> = self
            .connection
            .query_row(
                &sql,
                params![target.scope.host, target.scope.login, target.pr_node_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((attempt, bytes)) = found else {
            return Ok(None);
        };
        if bytes > 8192 {
            bail!("Source receipt metadata exceeds its 8 KiB guard budget; private text remains hidden");
        }
        let sql=format!("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND attempt_id=?4 AND state='confirmed' AND {bounded}<=8192");
        let marker = self
            .connection
            .query_row(
                &sql,
                params![
                    target.scope.host,
                    target.scope.login,
                    target.pr_node_id,
                    attempt
                ],
                read_marker,
            )
            .optional()?
            .map(|row| decode_marker(target, row))
            .transpose()?;
        if marker.is_none() {
            bail!("Original receipt changed during source visibility inspection; private text remains hidden");
        }
        Ok(marker)
    }
    pub fn open(path: &Path) -> Result<Self> {
        // Reuse established directory/file permissions and Cache migration checks.
        drop(Cache::open(path)?);
        let path = std::fs::canonicalize(path)?;
        let mut connection = Connection::open(&path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch("CREATE TABLE IF NOT EXISTS pr_write_scopes(
          host TEXT NOT NULL, principal TEXT NOT NULL, pr_node TEXT NOT NULL,
          generation INTEGER NOT NULL DEFAULT 0, PRIMARY KEY(host,principal,pr_node));
          CREATE TABLE IF NOT EXISTS pr_write_attempts(
          attempt_id INTEGER PRIMARY KEY AUTOINCREMENT,host TEXT NOT NULL,principal TEXT NOT NULL,
          pr_node TEXT NOT NULL,repository TEXT NOT NULL,pr_number INTEGER NOT NULL,
          command TEXT NOT NULL,selected_resource TEXT NOT NULL,observed_head TEXT NOT NULL,
          payload_sha256 TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN('pending','uncertain','confirmed','acknowledged')),
          confirmed_json TEXT,created_at INTEGER NOT NULL);
          CREATE UNIQUE INDEX IF NOT EXISTS pr_write_one_blocking ON pr_write_attempts(host,principal,pr_node)
          WHERE state IN('pending','uncertain');
          CREATE INDEX IF NOT EXISTS pr_write_source_receipts
          ON pr_write_attempts(host,principal,pr_node,state,attempt_id);")?;
        thread_history_v1::migrate(&transaction)?;
        thread_comment_history_v2::migrate(&transaction)?;
        review_operation_history_v3::migrate(&transaction)?;
        transaction.commit()?;
        Ok(Self { connection, path })
    }
    pub fn begin(
        &mut self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
    ) -> Result<PrWritePermit> {
        self.begin_with_thread_origin_v1(target, intent, None)
    }
    pub fn blocking(&self, target: &PrWriteTarget) -> Result<Option<PrWriteMarker>> {
        blocking_row(&self.connection, target)
    }
    /// Check the exact producer, account, target and payload immediately before
    /// attempting a write. A restored or already acknowledged permit is unusable.
    pub fn claim(
        &mut self,
        permit: &PrWritePermit,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
    ) -> Result<()> {
        if permit.issuer != self.path
            || !permit.marker.target.same_barrier(target)
            || permit.marker.payload_sha256 != intent.digest()?
        {
            bail!("Publication permit does not match this journal, account, PR or intent");
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = blocking_row(&transaction, target)?
            .context("Publication is no longer pending; inspect before attempting another write")?;
        if current.attempt_id != permit.marker.attempt_id
            || current.payload_sha256 != permit.marker.payload_sha256
            || current.state != PrWriteState::Pending
        {
            bail!("Publication permit is stale or uncertain; inspect before retrying");
        }
        // Invalidate an inspection prepared before publication starts. The permit
        // remains conservative if commit fails: it cannot be used a second time.
        permit.claim()?;
        advance(&transaction, target)?;
        transaction.commit()?;
        Ok(())
    }
    /// For restoring an old draft only. Fresh deliberate identical comments remain possible.
    pub fn latest_matching(
        &self,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
    ) -> Result<Option<PrWriteMarker>> {
        let digest = intent.digest()?;
        let value=self.connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND payload_sha256=?4 ORDER BY attempt_id DESC LIMIT 1",params![target.scope.host,target.scope.login,target.pr_node_id,digest],read_marker).optional()?;
        value.map(|row| decode_marker(target, row)).transpose()
    }
    /// Restore detection uses the original head/digest, never a freshly rebuilt
    /// head. Editing any publication field creates a different deliberate intent.
    pub fn confirmed_original(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
        current_intent: &PrWriteIntent,
    ) -> Result<Option<PrWriteMarker>> {
        if reference.attempt_id <= 0 || reference.pr_node_id != target.pr_node_id {
            bail!("Restored publication metadata belongs to another PR");
        }
        let mut original = current_intent.clone();
        original.observed_head = reference.observed_head.clone();
        if original.digest()? != reference.payload_sha256 {
            return Ok(None);
        }
        self.confirmed_publication(target, reference)
    }
    /// Validate a receipt reference without recovering or retaining the body.
    /// Derived projections may only publish data for an actually confirmed
    /// attempt in this exact verified principal/canonical PR namespace.
    pub(crate) fn confirmed_publication(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
    ) -> Result<Option<PrWriteMarker>> {
        if reference.attempt_id <= 0
            || reference.pr_node_id != target.pr_node_id
            || !matches!(reference.observed_head.len(), 40 | 64)
            || !reference
                .observed_head
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
            || reference.payload_sha256.len() != 64
            || !reference
                .payload_sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        {
            bail!("Publication reference does not identify an exact original PR attempt");
        }
        let row = self.connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND attempt_id=?4 AND observed_head=?5 AND payload_sha256=?6 AND state='confirmed'",params![target.scope.host,target.scope.login,target.pr_node_id,reference.attempt_id,reference.observed_head,reference.payload_sha256],read_marker).optional()?;
        let marker = row.map(|row| decode_marker(target, row)).transpose()?;
        if marker
            .as_ref()
            .is_some_and(|marker| marker.confirmed.is_none())
        {
            bail!("Confirmed publication has no canonical receipt; projection refused");
        }
        Ok(marker)
    }
    pub fn mark_uncertain(&mut self, permit: &PrWritePermit) -> Result<()> {
        self.finish(permit, PrWriteState::Uncertain, None)
    }
    pub fn mark_confirmed(
        &mut self,
        permit: &PrWritePermit,
        effect: &PrConfirmedEffect,
    ) -> Result<()> {
        if !permit.claimed.load(Ordering::Acquire) {
            bail!("Cannot confirm a PR publication permit that was never used");
        }
        canonical_identifier(&effect.resource_id)?;
        if let Some(node) = &effect.node_id {
            canonical_identifier(node)?;
        }
        self.finish(permit, PrWriteState::Confirmed, Some(effect))
    }
    fn finish(
        &mut self,
        permit: &PrWritePermit,
        state: PrWriteState,
        effect: Option<&PrConfirmedEffect>,
    ) -> Result<()> {
        if permit.issuer != self.path {
            bail!("Publication permit belongs to another journal");
        }
        let marker = &permit.marker;
        let target = &marker.target;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let updated=transaction.execute("UPDATE pr_write_attempts SET state=?1,confirmed_json=?2 WHERE attempt_id=?3 AND host=?4 AND principal=?5 AND pr_node=?6 AND payload_sha256=?7 AND state IN('pending','uncertain')",params![state_name(state),effect.map(serde_json::to_string).transpose()?,marker.attempt_id,target.scope.host,target.scope.login,target.pr_node_id,marker.payload_sha256])?;
        if updated != 1 {
            bail!(
                "Publication record changed or was already completed; inspect it before proceeding"
            );
        }
        advance(&transaction, target)?;
        transaction.commit()?;
        Ok(())
    }
    pub fn prepare_inspection(
        &self,
        provider: &GithubProvider,
        identity: &HostIdentity,
        target: &PrSummary,
        cancel: &CancellationToken,
    ) -> Result<PrJournalInspection> {
        let journal_target = PrWriteTarget::new(identity, target)?;
        let before = generation(&self.connection, &journal_target)?;
        let observation = provider
            .inspect_pr_target(identity, target, cancel)
            .map_err(anyhow::Error::msg)?;
        let observed_target = PrWriteTarget::new(observation.identity(), observation.target())?;
        if !journal_target.same_barrier(&observed_target) {
            bail!("Inspection belongs to another verified PR account");
        }
        if generation(&self.connection, &journal_target)? != before {
            bail!("Publication changed while inspecting; inspect the current outcome again");
        }
        Ok(PrJournalInspection {
            issuer: self.path.clone(),
            target: journal_target,
            generation: before,
            observation,
        })
    }
    pub fn acknowledge(
        &mut self,
        provider: &GithubProvider,
        inspection: PrJournalInspection,
        current_identity: &HostIdentity,
        cancel: &CancellationToken,
    ) -> Result<usize> {
        if inspection.issuer != self.path {
            bail!("Inspection belongs to another journal");
        }
        let current = PrWriteTarget::new(current_identity, inspection.observation.target())?;
        if !current.same_barrier(&inspection.target)
            || !current_identity.same_account(inspection.observation.identity())
        {
            bail!("Account changed since inspection; no uncertainty marker was cleared");
        }
        if generation(&self.connection, &current)? != inspection.generation {
            bail!("Publication changed since inspection; inspect the current outcome again");
        }
        let fresh = provider.identity(cancel).map_err(anyhow::Error::msg)?;
        if !fresh.same_account(current_identity)
            || !fresh.same_account(inspection.observation.identity())
        {
            bail!("Account changed before acknowledgement; durable marker remains blocked");
        }
        // A provider refusal changes no durable state. If the later storage write
        // fails, the local pending marker still blocks every application writer.
        provider
            .acknowledge_pr_observation(inspection.observation)
            .map_err(anyhow::Error::msg)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if generation(&transaction, &current)? != inspection.generation {
            bail!("Publication changed during acknowledgement; durable marker remains blocked");
        }
        let changed=transaction.execute("UPDATE pr_write_attempts SET state='acknowledged' WHERE host=?1 AND principal=?2 AND pr_node=?3 AND state IN('pending','uncertain')",params![current.scope.host,current.scope.login,current.pr_node_id])?;
        advance(&transaction, &current)?;
        transaction.commit()?;
        Ok(changed)
    }
}

fn identifier(value: &str) -> Result<()> {
    if value.is_empty() || value.len() > 4096 || value.chars().any(char::is_control) {
        bail!("Expected a bounded nonempty canonical resource identifier");
    }
    Ok(())
}
fn canonical_identifier(value: &str) -> Result<()> {
    identifier(value)?;
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"/:_-.=+".contains(&byte))
    {
        bail!("Confirmed effects store canonical identifiers or static states, not content");
    }
    Ok(())
}
fn state_name(state: PrWriteState) -> &'static str {
    match state {
        PrWriteState::Pending => "pending",
        PrWriteState::Uncertain => "uncertain",
        PrWriteState::Confirmed => "confirmed",
        PrWriteState::Acknowledged => "acknowledged",
    }
}
fn generation(connection: &Connection, target: &PrWriteTarget) -> Result<i64> {
    Ok(connection
        .query_row(
            "SELECT generation FROM pr_write_scopes WHERE host=?1 AND principal=?2 AND pr_node=?3",
            params![target.scope.host, target.scope.login, target.pr_node_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(0))
}
fn advance(transaction: &Transaction, target: &PrWriteTarget) -> Result<()> {
    transaction.execute("INSERT INTO pr_write_scopes(host,principal,pr_node,generation) VALUES(?1,?2,?3,1) ON CONFLICT(host,principal,pr_node) DO UPDATE SET generation=generation+1",params![target.scope.host,target.scope.login,target.pr_node_id])?;
    Ok(())
}
type MarkerRow = (
    i64,
    String,
    u64,
    String,
    String,
    String,
    String,
    String,
    Option<String>,
    u64,
);
fn read_marker(row: &rusqlite::Row<'_>) -> rusqlite::Result<MarkerRow> {
    Ok((
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        row.get(4)?,
        row.get(5)?,
        row.get(6)?,
        row.get(7)?,
        row.get(8)?,
        row.get(9)?,
    ))
}
fn blocking_row(connection: &Connection, target: &PrWriteTarget) -> Result<Option<PrWriteMarker>> {
    connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND state IN('pending','uncertain') ORDER BY attempt_id DESC LIMIT 1",params![target.scope.host,target.scope.login,target.pr_node_id],read_marker).optional()?.map(|row|decode_marker(target,row)).transpose()
}
fn decode_marker(target: &PrWriteTarget, row: MarkerRow) -> Result<PrWriteMarker> {
    let state = match row.7.as_str() {
        "pending" => PrWriteState::Pending,
        "uncertain" => PrWriteState::Uncertain,
        "confirmed" => PrWriteState::Confirmed,
        "acknowledged" => PrWriteState::Acknowledged,
        _ => bail!("Unknown persisted PR publication state"),
    };
    let mut observed = target.clone();
    observed.repository = row.1;
    observed.number = row.2;
    Ok(PrWriteMarker {
        attempt_id: row.0,
        target: observed,
        command: row.3,
        selected_resource: row.4,
        observed_head: row.5,
        payload_sha256: row.6,
        state,
        confirmed: row
            .8
            .map(|value| serde_json::from_str(&value).context("Invalid confirmed effect record"))
            .transpose()?,
        created_at: row.9,
    })
}
