//! A closed notification-domain journal. No PR/Actions schema, target, legacy
//! protocol bytes or generic mutation callback is reused or relaxed here.
use super::{
    repository_catalogue_intents_v1::{issuer, Issuer},
    *,
};
use crate::providers::{
    gh::notifications_v63::NotificationWriteOriginV63, runner::CancellationToken,
};
use rusqlite::{Transaction, TransactionBehavior};
use sha2::{Digest, Sha256};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};

pub(super) fn initialize(transaction: &Transaction<'_>) -> Result<()> {
    transaction.execute_batch("CREATE TABLE IF NOT EXISTS notification_write_attempts_v63 (
        id INTEGER PRIMARY KEY,
        host TEXT NOT NULL,account_id INTEGER NOT NULL CHECK(account_id>0),
        thread_id TEXT NOT NULL CHECK(length(thread_id) BETWEEN 1 AND 64),
        repository_id INTEGER NOT NULL CHECK(repository_id>0),repository_node TEXT NOT NULL,
        subject_kind TEXT NOT NULL,subject_key TEXT NOT NULL,
        action TEXT NOT NULL CHECK(action IN('mark_read','ignore','unignore')),
        requested INTEGER NOT NULL CHECK(requested IN(0,1)),observed_updated_at TEXT NOT NULL,
        original_source_key TEXT NOT NULL,original_generation INTEGER NOT NULL CHECK(original_generation>0),
        intent_digest TEXT NOT NULL CHECK(length(intent_digest)=64),
        state TEXT NOT NULL CHECK(state IN('pending','uncertain','confirmed','acknowledged')),
        receipt_json TEXT CHECK(receipt_json IS NULL OR length(CAST(receipt_json AS BLOB))<=8192),
        origin_json TEXT NOT NULL CHECK(length(CAST(origin_json AS BLOB))<=16384),
        created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL,
        generation INTEGER NOT NULL CHECK(generation>0));
        CREATE INDEX IF NOT EXISTS notification_write_target_v63 ON
        notification_write_attempts_v63(host,account_id,thread_id,state,id);
        CREATE INDEX IF NOT EXISTS notification_write_generation_v63 ON
        notification_write_attempts_v63(host,account_id,thread_id,generation);
        CREATE INDEX IF NOT EXISTS notification_write_receipt_v63 ON
        notification_write_attempts_v63(host,account_id,id) WHERE receipt_json IS NOT NULL;")?;
    Ok(())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotificationRequestedV63 {
    MarkRead,
    Ignore,
    Unignore,
}
impl NotificationRequestedV63 {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "mark_read" => Some(Self::MarkRead),
            "ignore" => Some(Self::Ignore),
            "unignore" => Some(Self::Unignore),
            _ => None,
        }
    }
    pub(crate) fn command(self) -> &'static str {
        match self {
            Self::MarkRead => "mark_read",
            Self::Ignore => "ignore",
            Self::Unignore => "unignore",
        }
    }
    fn requested(self) -> bool {
        self != Self::Unignore
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NotificationWriteStateV63 {
    Pending,
    Uncertain,
    Confirmed,
    Acknowledged,
}
fn state(raw: &str) -> Result<NotificationWriteStateV63> {
    Ok(match raw {
        "pending" => NotificationWriteStateV63::Pending,
        "uncertain" => NotificationWriteStateV63::Uncertain,
        "confirmed" => NotificationWriteStateV63::Confirmed,
        "acknowledged" => NotificationWriteStateV63::Acknowledged,
        _ => bail!("Unknown notification journal state; protected record retained"),
    })
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NotificationWriteReferenceV63 {
    attempt: i64,
    digest: String,
}
impl std::fmt::Debug for NotificationWriteReferenceV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationWriteReferenceV63(<durable reference; no authority>)")
    }
}
impl NotificationWriteReferenceV63 {
    pub(crate) fn attempt_id(&self) -> i64 {
        self.attempt
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredOriginV63 {
    version: u8,
    host: String,
    account_id: u64,
    thread_id: String,
    repository_id: u64,
    repository_node: String,
    repository_name: String,
    subject_kind: String,
    subject_key: String,
    observed_updated_at: String,
    source: String,
    source_generation: u64,
    database: String,
}
fn origin_record(
    origin: &NotificationWriteOriginV63,
    witness: &NotificationSourceWitnessV63,
) -> Result<(StoredOriginV63, String)> {
    if origin.scope() != witness.scope()
        || origin.source() != witness.source()
        || origin.generation() != witness.generation()
    {
        bail!("Original notification target/local witness mismatch; nothing claimed");
    }
    let account_id = origin
        .scope()
        .login
        .strip_prefix("user-id:")
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|id| *id > 0 && *id <= i64::MAX as u64)
        .context("Original numeric notification account unavailable")?;
    let source = origin
        .source()
        .strip_prefix("notifications-v63:")
        .filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .context("Original notification source fingerprint unavailable")?;
    let record = StoredOriginV63 {
        version: 1,
        host: origin.scope().host.clone(),
        account_id,
        thread_id: origin.thread().into(),
        repository_id: origin.repository_id(),
        repository_node: origin.repository_node().into(),
        repository_name: origin.repository_name().into(),
        subject_kind: origin.subject_kind().into(),
        subject_key: origin.subject_key().into(),
        observed_updated_at: origin.updated().into(),
        source: source.into(),
        source_generation: origin.generation(),
        database: witness.issuer_v63().fingerprint_v63(),
    };
    // Genuine scalar producer bounds keep this well below16KiB before encoding;
    // no notification title, body, URL, credentials or second draft is stored.
    let json = serde_json::to_string(&record)?;
    if json.len() > 16 * 1024 {
        bail!("Notification origin exceeds16KiB; nothing claimed");
    }
    Ok((record, json))
}
pub(crate) struct NotificationWriteMarkerV63 {
    reference: NotificationWriteReferenceV63,
    state: NotificationWriteStateV63,
    generation: u64,
    action: String,
    origin_json: String,
    receipt_json: Option<String>,
}
impl std::fmt::Debug for NotificationWriteMarkerV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationWriteMarkerV63(<protected original attempt>)")
    }
}
impl NotificationWriteMarkerV63 {
    pub(crate) fn reference(&self) -> &NotificationWriteReferenceV63 {
        &self.reference
    }
    pub(crate) fn state(&self) -> NotificationWriteStateV63 {
        self.state
    }
    pub(crate) fn command(&self) -> &str {
        &self.action
    }
}
/// A local once-only claim, NOT remote permission. The sole actual publisher
/// must still close the exact fresh source/account phase before invocation.
pub(crate) struct NotificationWritePermitV63 {
    issuer: Issuer,
    marker: NotificationWriteMarkerV63,
    claimed: AtomicBool,
}
impl std::fmt::Debug for NotificationWritePermitV63 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationWritePermitV63(<once-only protected local claim>)")
    }
}
impl NotificationWritePermitV63 {
    pub(crate) fn marker(&self) -> &NotificationWriteMarkerV63 {
        &self.marker
    }
    pub(crate) fn claim_once(&self) -> Result<()> {
        if self
            .claimed
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            bail!("Notification attempt already claimed; inspect, never resend automatically");
        }
        Ok(())
    }
}
fn live(
    origin: &NotificationWriteOriginV63,
    token: &CancellationToken,
    deadline: Instant,
) -> Result<()> {
    if !origin.phase_matches(token, deadline) {
        bail!("Original notification token/deadline changed; nothing claimed");
    }
    Ok(())
}
impl Cache {
    pub(crate) fn validate_notification_marker_v63(
        &self,
        marker: &NotificationWriteMarkerV63,
        origin: &NotificationWriteOriginV63,
        witness: &NotificationSourceWitnessV63,
        token: &CancellationToken,
        deadline: Instant,
    ) -> Result<()> {
        live(origin, token, deadline)?;
        self.validate_notification_source_v63(witness, token, deadline)?;
        let saved: StoredOriginV63 = serde_json::from_str(&marker.origin_json)?;
        let action = NotificationRequestedV63::parse(&marker.action)
            .context("Unknown notification purpose; history remains protected")?;
        let digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                &saved,
                action.command(),
                action.requested()
            ))?)
        );
        if saved.version != 1
            || saved.host != origin.scope().host
            || Some(saved.account_id)
                != origin
                    .scope()
                    .login
                    .strip_prefix("user-id:")
                    .and_then(|v| v.parse().ok())
            || saved.thread_id != origin.thread()
            || saved.repository_id != origin.repository_id()
            || saved.repository_node != origin.repository_node()
            || saved.subject_kind != origin.subject_kind()
            || saved.subject_key != origin.subject_key()
            || saved.database != witness.issuer_v63().fingerprint_v63()
            || digest != marker.reference.digest
        {
            bail!("Original protected notification origin/ref/target mismatch");
        }
        let row:Option<(String,String,u64,String,Option<String>,bool)>=self.connection.query_row("SELECT CASE WHEN length(CAST(intent_digest AS BLOB))=64 THEN intent_digest END,CASE WHEN length(CAST(state AS BLOB))<=16 THEN state END,generation,CASE WHEN length(CAST(origin_json AS BLOB))<=16384 THEN origin_json END,CASE WHEN length(CAST(receipt_json AS BLOB))<=8192 THEN receipt_json END,receipt_json IS NULL OR length(CAST(receipt_json AS BLOB))<=8192 FROM notification_write_attempts_v63 WHERE id=?1",
            [marker.reference.attempt],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        let state_name = match marker.state {
            NotificationWriteStateV63::Pending => "pending",
            NotificationWriteStateV63::Uncertain => "uncertain",
            NotificationWriteStateV63::Confirmed => "confirmed",
            NotificationWriteStateV63::Acknowledged => "acknowledged",
        };
        if row
            .as_ref()
            .is_none_or(|(digest, state, generation, origin, receipt, bounded)| {
                !bounded
                    || digest != &marker.reference.digest
                    || state != state_name
                    || *generation != marker.generation
                    || origin != &marker.origin_json
                    || receipt != &marker.receipt_json
            })
        {
            bail!("Notification marker/ref/state/generation snapshot changed; inspect again");
        }
        let latest:u64=self.connection.query_row("SELECT MAX(generation) FROM notification_write_attempts_v63 WHERE host=?1 AND account_id=?2 AND thread_id=?3",params![saved.host,saved.account_id,saved.thread_id],|r|r.get(0))?;
        if latest != marker.generation {
            bail!("A newer notification attempt exists; no older acknowledgement");
        }
        self.validate_notification_source_v63(witness, token, deadline)?;
        live(origin, token, deadline)
    }
    /// Explicit LOCAL acknowledgement only, never a server request. Ordering is
    /// runner barrier first, exact SQL CAS second, as in existing Review recovery.
    /// A partial runner-only success MUST retain the SQL blocker and report
    /// failure; it never enables resend or claims completed acknowledgement.
    pub(crate) fn acknowledge_notification_inspection_v63(
        &mut self,
        provider: &crate::providers::gh::GithubProvider,
        inspection: &crate::providers::gh::NotificationInspectionV63,
        token: &CancellationToken,
        deadline: Instant,
    ) -> Result<()> {
        let deadline = deadline.min(inspection.deadline_v63());
        let (origin, marker, witness) = inspection
            .local_gate_v63(provider, token, deadline)
            .context("Fresh exact original notification inspection unavailable")?;
        if !matches!(
            marker.state,
            NotificationWriteStateV63::Pending | NotificationWriteStateV63::Uncertain
        ) || marker.receipt_json.is_some()
        {
            bail!("Only exact unresolved notification attempts admit explicit local ACK");
        }
        self.validate_notification_marker_v63(marker, origin, witness, token, deadline)?;
        provider
            .reconcile_notification_inspection_v63(inspection, token, deadline)
            .map_err(anyhow::Error::msg)?;
        self.validate_notification_marker_v63(marker, origin, witness, token, deadline)?;
        self.connection.busy_timeout(
            deadline
                .saturating_duration_since(Instant::now())
                .min(std::time::Duration::from_secs(2)),
        )?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        witness.validate_transaction_v63(&transaction)?;
        let state = if marker.state == NotificationWriteStateV63::Pending {
            "pending"
        } else {
            "uncertain"
        };
        let changed=transaction.execute("UPDATE notification_write_attempts_v63 SET state='acknowledged',updated_at=?1 WHERE id=?2 AND intent_digest=?3 AND generation=?4 AND state=?5 AND receipt_json IS NULL AND origin_json=?6 AND generation=(SELECT MAX(generation) FROM notification_write_attempts_v63 WHERE host=?7 AND account_id=?8 AND thread_id=?9)",
            params![now(),marker.reference.attempt,marker.reference.digest,marker.generation,state,marker.origin_json,origin.scope().host,origin.scope().login.strip_prefix("user-id:").context("Numeric account unavailable")?,origin.thread()])?;
        live(origin, token, deadline)?;
        if changed != 1 || !witness.issuer_v63().file_still_matches_v63() {
            bail!("Local ACK SQL CAS/database changed after runner reconciliation; protected record retained");
        }
        transaction.commit()?;
        Ok(())
    }
    /// Only the closed typed producer may supply a server receipt. The same
    /// transaction checks the original source and exact immutable local claim;
    /// a returned receipt is an observed state, never causality or a retry.
    pub(crate) fn confirm_notification_write_v63(
        &mut self,
        permit: &NotificationWritePermitV63,
        origin: &NotificationWriteOriginV63,
        witness: &NotificationSourceWitnessV63,
        receipt: &crate::providers::gh::NotificationWriteReceiptV63,
        token: &CancellationToken,
        deadline: Instant,
    ) -> Result<()> {
        live(origin, token, deadline)?;
        if !receipt.matches_origin(origin, &permit.marker.action) {
            bail!("Notification receipt is not this original purpose/target");
        }
        self.validate_notification_permit_v63(permit, origin, witness, token, deadline)?;
        let encoded = receipt.encoded()?;
        if encoded.len() > 8192 {
            bail!("Notification receipt exceeds8KiB; protected claim retained");
        }
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        live(origin, token, deadline)?;
        witness.validate_transaction_v63(&transaction)?;
        let changed=transaction.execute("UPDATE notification_write_attempts_v63 SET state='confirmed',receipt_json=?1,updated_at=?2 WHERE id=?3 AND intent_digest=?4 AND generation=?5 AND state='pending' AND origin_json=?6 AND receipt_json IS NULL",
            params![encoded,now(),permit.marker.reference.attempt,permit.marker.reference.digest,permit.marker.generation,permit.marker.origin_json])?;
        if changed != 1 || !permit.issuer.file_still_matches_v63() {
            bail!("Notification claim changed before confirmation; do not resend");
        }
        live(origin, token, deadline)?;
        transaction.commit()?;
        Ok(())
    }
    /// Best-effort durable uncertainty is never a new remote attempt. A
    /// canceled/expired SQL phase leaves Pending intact and protected.
    pub(crate) fn uncertain_notification_write_v63(
        &mut self,
        permit: &NotificationWritePermitV63,
        token: &CancellationToken,
        deadline: Instant,
    ) -> Result<()> {
        if token.is_cancelled() || Instant::now() >= deadline {
            bail!("Original clock/token closed; protected Pending retained");
        }
        if issuer(self).map_err(anyhow::Error::new)? != permit.issuer {
            bail!("Original notification database changed; protected claim retained");
        }
        let changed=self.connection.execute("UPDATE notification_write_attempts_v63 SET state='uncertain',updated_at=?1 WHERE id=?2 AND intent_digest=?3 AND generation=?4 AND state='pending' AND origin_json=?5 AND receipt_json IS NULL",
            params![now(),permit.marker.reference.attempt,permit.marker.reference.digest,permit.marker.generation,permit.marker.origin_json])?;
        if changed != 1 {
            bail!("Notification claim is no longer the exact Pending generation");
        }
        Ok(())
    }
    pub(crate) fn begin_notification_write_v63(
        &mut self,
        origin: &NotificationWriteOriginV63,
        witness: &NotificationSourceWitnessV63,
        requested: NotificationRequestedV63,
        token: &CancellationToken,
        deadline: Instant,
    ) -> Result<NotificationWritePermitV63> {
        live(origin, token, deadline)?;
        self.validate_notification_source_v63(witness, token, deadline)?;
        let (record, origin_json) = origin_record(origin, witness)?;
        let digest = format!(
            "{:x}",
            Sha256::digest(serde_json::to_vec(&(
                &record,
                requested.command(),
                requested.requested()
            ))?)
        );
        let original = issuer(self).map_err(anyhow::Error::new)?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        live(origin, token, deadline)?;
        witness.validate_transaction_v63(&transaction)?;
        let blocker:Option<i64>=transaction.query_row("SELECT id FROM notification_write_attempts_v63 WHERE host=?1 AND account_id=?2 AND thread_id=?3 AND state IN('pending','uncertain') ORDER BY id DESC LIMIT 1",
            params![record.host,record.account_id,record.thread_id],|r|r.get(0)).optional()?;
        if blocker.is_some() {
            bail!("This notification has a protected pending/uncertain attempt; inspect explicitly before any new action");
        }
        let generation:u64=transaction.query_row("SELECT COALESCE(MAX(generation),0)+1 FROM notification_write_attempts_v63 WHERE host=?1 AND account_id=?2 AND thread_id=?3",
            params![record.host,record.account_id,record.thread_id],|r|r.get(0))?;
        if generation == 0 || generation > i64::MAX as u64 {
            bail!("Notification journal generation exhausted");
        }
        let stamp = now();
        transaction.execute("INSERT INTO notification_write_attempts_v63(host,account_id,thread_id,repository_id,repository_node,subject_kind,subject_key,action,requested,observed_updated_at,original_source_key,original_generation,intent_digest,state,origin_json,created_at,updated_at,generation)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'pending',?14,?15,?15,?16)",
            params![record.host,record.account_id,record.thread_id,record.repository_id,record.repository_node,record.subject_kind,record.subject_key,
                requested.command(),requested.requested(),record.observed_updated_at,record.source,record.source_generation,digest,origin_json,stamp,generation])?;
        let attempt = transaction.last_insert_rowid();
        live(origin, token, deadline)?;
        witness.validate_transaction_v63(&transaction)?;
        if !original.file_still_matches_v63() {
            bail!("Original notification database replaced before claim commit");
        }
        transaction.commit()?;
        live(origin, token, deadline)?;
        if issuer(self).map_err(anyhow::Error::new)? != original {
            bail!("Original notification database changed after claim; protected record may exist, do not resend");
        }
        Ok(NotificationWritePermitV63 {
            issuer: original,
            marker: NotificationWriteMarkerV63 {
                reference: NotificationWriteReferenceV63 { attempt, digest },
                state: NotificationWriteStateV63::Pending,
                generation,
                action: requested.command().into(),
                origin_json,
                receipt_json: None,
            },
            claimed: AtomicBool::new(false),
        })
    }
    pub(crate) fn validate_notification_permit_v63(
        &self,
        permit: &NotificationWritePermitV63,
        origin: &NotificationWriteOriginV63,
        witness: &NotificationSourceWitnessV63,
        token: &CancellationToken,
        deadline: Instant,
    ) -> Result<()> {
        live(origin, token, deadline)?;
        self.validate_notification_source_v63(witness, token, deadline)?;
        if issuer(self).map_err(anyhow::Error::new)? != permit.issuer {
            bail!("Foreign original notification journal database");
        }
        let actual:Option<(String,String,u64,String)>=self.connection.query_row("SELECT CASE WHEN length(CAST(intent_digest AS BLOB))=64 THEN intent_digest END,CASE WHEN length(CAST(state AS BLOB))<=16 THEN state END,generation,CASE WHEN length(CAST(origin_json AS BLOB))<=16384 THEN origin_json END FROM notification_write_attempts_v63 WHERE id=?1",
            params![permit.marker.reference.attempt],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        if actual
            .as_ref()
            .is_none_or(|(digest, state, generation, origin)| {
                digest != &permit.marker.reference.digest
                    || state != "pending"
                    || *generation != permit.marker.generation
                    || origin != &permit.marker.origin_json
            })
        {
            bail!("Original notification attempt changed; protected record retained");
        }
        let (_, expected) = origin_record(origin, witness)?;
        if expected != permit.marker.origin_json {
            bail!("Original notification target/ref fingerprint changed");
        }
        self.validate_notification_source_v63(witness, token, deadline)?;
        live(origin, token, deadline)
    }
    /// Read-only metadata for this actual live selected notification. Callers
    /// prepare presentation BEFORE the SAME Core's closing SQL/final UID.
    /// It grants no ACK, publication, body restore, retry or reconciliation.
    pub(crate) fn notification_attempts_v63(
        &self,
        origin: &NotificationWriteOriginV63,
        witness: &NotificationSourceWitnessV63,
        token: &CancellationToken,
        deadline: Instant,
    ) -> Result<Vec<NotificationWriteMarkerV63>> {
        live(origin, token, deadline)?;
        self.validate_notification_source_v63(witness, token, deadline)?;
        let account = origin
            .scope()
            .login
            .strip_prefix("user-id:")
            .and_then(|v| v.parse::<u64>().ok())
            .context("Original numeric notification account unavailable")?;
        let mut statement=self.connection.prepare("SELECT id,
            CASE WHEN length(CAST(intent_digest AS BLOB))=64 THEN intent_digest END,
            CASE WHEN length(CAST(state AS BLOB))<=16 THEN state END,generation,
            CASE WHEN length(CAST(action AS BLOB))<=32 THEN action END,
            CASE WHEN length(CAST(origin_json AS BLOB))<=16384 THEN origin_json END,
            CASE WHEN length(CAST(receipt_json AS BLOB))<=8192 THEN receipt_json END,
            receipt_json IS NULL OR length(CAST(receipt_json AS BLOB))<=8192
            FROM notification_write_attempts_v63 WHERE host=?1 AND account_id=?2 AND thread_id=?3 ORDER BY id DESC LIMIT 8")?;
        let mapped = statement.query_map(
            params![origin.scope().host, account, origin.thread()],
            |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, u64>(3)?,
                    r.get::<_, String>(4)?,
                    r.get::<_, String>(5)?,
                    r.get::<_, Option<String>>(6)?,
                    r.get::<_, bool>(7)?,
                ))
            },
        )?;
        let mut markers = Vec::with_capacity(8);
        for row in mapped {
            let (
                attempt,
                digest,
                raw,
                generation,
                action,
                origin_json,
                receipt_json,
                receipt_bounded,
            ) = row?;
            if !receipt_bounded
                || origin_json.len() > 16 * 1024
                || receipt_json.as_ref().is_some_and(|s| s.len() > 8192)
            {
                bail!("Notification journal metadata exceeds its closed bound; private history hidden");
            }
            let saved: StoredOriginV63 = serde_json::from_str(&origin_json)?;
            let requested = NotificationRequestedV63::parse(&action)
                .context("Unknown protected notification purpose")?;
            let expected_digest = format!(
                "{:x}",
                Sha256::digest(serde_json::to_vec(&(
                    &saved,
                    requested.command(),
                    requested.requested()
                ))?)
            );
            if saved.version != 1
                || saved.host != origin.scope().host
                || saved.account_id != account
                || saved.thread_id != origin.thread()
                || saved.repository_id != origin.repository_id()
                || saved.repository_node != origin.repository_node()
                || saved.subject_kind != origin.subject_kind()
                || saved.subject_key != origin.subject_key()
                || saved.database != witness.issuer_v63().fingerprint_v63()
                || !matches!(action.as_str(), "mark_read" | "ignore" | "unignore")
                || attempt <= 0
                || generation == 0
                || digest.len() != 64
                || digest != expected_digest
                || !digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                bail!("Notification history target/database metadata mismatched; no row promoted");
            }
            markers.push(NotificationWriteMarkerV63 {
                reference: NotificationWriteReferenceV63 { attempt, digest },
                state: state(&raw)?,
                generation,
                action,
                origin_json,
                receipt_json,
            });
            live(origin, token, deadline)?;
        }
        self.validate_notification_source_v63(witness, token, deadline)?;
        Ok(markers)
    }
}
