//! Closed exact terminal/fence/fresh-data transaction. All payload encodings,
//! old membership keys and metadata predicates are prepared before final UID.
//! No generic callback/source mint/SQL getter is exposed to callers.
use super::*;
use crate::{
    pr_write_journal::conversation_refresh_v19::ConversationTerminalWitnessV19,
    providers::{
        gh::{
            ConversationDetailSnapshotV19, ConversationPageSupplementV61,
            ConversationRefreshCloseSealV19, ConversationRefreshPreparationV19,
        },
        runner::CancellationToken,
    },
    scoped_pr_collaboration_source_v17::initial_v17::{
        RefreshReadCloseSealV19, RefreshReadPreparationV19,
    },
    scoped_pr_collaboration_source_v17::PrCollaborationSourceV17,
    scoped_pr_observed_tabs_v1::resources::PrLocalSourceBoundaryV13,
};
use repository_catalogue_intents_v1::conversation_continuation_v19::PreparedCatalogueContinuationV19;
#[path = "conversation_refresh_v19/metadata_storage_v51.rs"]
mod metadata_storage_v51;
#[path = "conversation_refresh_v19/own_fence.rs"]
mod own_fence;
pub(crate) use metadata_storage_v51::validate_members_storage_peak_v61;
pub(crate) use metadata_storage_v51::validate_metadata_members_storage_peak_v62;
pub(crate) use metadata_storage_v51::validate_metadata_storage_peak_v51;
#[cfg(test)]
pub(crate) use own_fence::{after_actual_reservation_v20, observe_next_actual_reservation_v20};
pub(crate) use own_fence::{ConversationOwnFenceV19, PreparedConversationOwnFenceV19};
pub(crate) use own_fence::{
    PersonalConversationReadAttemptV20, PersonalConversationReservationWitnessV20,
};
use rusqlite::{params_from_iter, types::Value as SqlValue, Transaction, TransactionBehavior};
use std::{
    sync::{Arc, Weak},
    time::Instant,
};

const METADATA_BYTES_V19: usize = 64 * 1024;
const PREPARED_BYTES_V19: usize = 8 * 1024 * 1024;
struct RawRowsV19 {
    count_sql: String,
    count_values: Vec<SqlValue>,
    row_sql: String,
    rows: Vec<Vec<SqlValue>>,
}
impl RawRowsV19 {
    /// Only the closed fixed profiles below call this. No arbitrary query or
    /// callback is exported. Type+binary equality distinguishes NULL/absent.
    fn capture(
        tx: &Transaction<'_>,
        table: &str,
        fields: &str,
        predicate: &str,
        values: Vec<SqlValue>,
        remaining: &mut usize,
    ) -> Result<Self> {
        let columns = fields.split(',').collect::<Vec<_>>();
        let bytes = columns
            .iter()
            .map(|s| format!("COALESCE(length(CAST({s} AS BLOB)),0)"))
            .collect::<Vec<_>>()
            .join("+");
        let count_sql = format!("SELECT count(*) FROM {table} WHERE {predicate}");
        let (count, size): (usize, usize) = tx.query_row(
            &format!("SELECT count(*),COALESCE(SUM({bytes}),0) FROM {table} WHERE {predicate}"),
            params_from_iter(&values),
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if count > 512 || size > *remaining {
            bail!("Conversation exact metadata witness exceeds64KiB/512-row profile before copy");
        }
        if table == "sync_resource_bindings" && count > 32 {
            bail!("Conversation exact target exceeds32 bound sources; no partial publication");
        }
        *remaining -= size;
        let rows = {
            let mut statement =
                tx.prepare(&format!("SELECT {fields} FROM {table} WHERE {predicate}"))?;
            let rows = statement
                .query_map(params_from_iter(&values), |r| {
                    (0..columns.len())
                        .map(|i| r.get(i))
                        .collect::<rusqlite::Result<Vec<SqlValue>>>()
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            rows
        };
        if rows.len() != count {
            bail!("Conversation metadata changed within capture");
        }
        let comparisons = columns
            .iter()
            .enumerate()
            .map(|(i, c)| {
                format!(
                    "typeof({c})=typeof(?{}) AND CAST({c} AS BLOB) IS CAST(?{} AS BLOB)",
                    i + 1,
                    i + 1
                )
            })
            .collect::<Vec<_>>()
            .join(" AND ");
        Ok(Self {
            count_sql,
            count_values: values,
            row_sql: format!("SELECT count(*) FROM {table} WHERE {comparisons}"),
            rows,
        })
    }
    fn validate(&self, tx: &Transaction<'_>) -> Result<()> {
        let count: usize =
            tx.query_row(&self.count_sql, params_from_iter(&self.count_values), |r| {
                r.get(0)
            })?;
        if count != self.rows.len() {
            bail!("Conversation metadata keyspace changed; no fence/publication");
        }
        for row in &self.rows {
            if tx.query_row(&self.row_sql, params_from_iter(row), |r| {
                r.get::<_, usize>(0)
            })? != 1
            {
                bail!("Conversation metadata row changed; no stale derived result");
            }
        }
        Ok(())
    }
}
pub(super) struct EpochV19 {
    rows: Vec<RawRowsV19>,
}
impl EpochV19 {
    fn capture(
        tx: &Transaction<'_>,
        terminal: &ConversationTerminalWitnessV19,
        source: &Arc<PrCollaborationSourceV17>,
    ) -> Result<Self> {
        Self::capture_target(tx, terminal.target(), source)
    }
    pub(super) fn capture_state_repair_v74(
        tx: &Transaction<'_>,
        repair: &crate::pr_write_journal::state_origin_v68::StateCacheRepairV74,
    ) -> Result<Self> {
        repair.validate_transaction(tx)?;
        Self::capture_target(tx, repair.target(), repair.source())
    }
    fn capture_target(
        tx: &Transaction<'_>,
        target: &crate::pr_write_journal::PrWriteTarget,
        source: &Arc<PrCollaborationSourceV17>,
    ) -> Result<Self> {
        let scope = target.scope();
        let mut remaining = METADATA_BYTES_V19;
        let mut rows = Vec::new();
        for (table,fields,predicate,repo) in [
            ("sync_reservations","host,login,source,generation,active","host=?1 AND login=?2",false),
            ("checkpoints","host,login,source,generation,fetched_at,complete","host=?1 AND login=?2",false),
            ("checkpoint_records","host,login,source,kind,id","host=?1 AND login=?2",false),
            ("records","host,login,kind,id,repository,fetched_at,source_updated_at,complete","host=?1 AND login=?2 AND repository=?3 COLLATE NOCASE",true),
            ("sync_binding_lookup_epochs","host,login,namespace,repository,generation","host=?1 AND login=?2 AND repository=?3 COLLATE NOCASE",true),
            ("sync_resource_bindings","host,login,source,namespace,repository_id,repository,resource,kind,reservation_generation,visible_generation,active","host=?1 AND login=?2 AND repository=?3 COLLATE NOCASE AND resource=?4",true),
            ("sync_mutation_fences","host,login,namespace,repository,resource,kind,mutation_ordinal","host=?1 AND login=?2 AND repository=?3 COLLATE NOCASE AND resource=?4",true),
            ("pr_write_state_filter_fences_v68","host,principal,pr_node,repository_id,pr_id,repository,attempt_id,raw_origin_sha256,raw_disposition_sha256,status","host=?1 AND principal=?2",false),
            ("pr_write_state_source_watermarks_v73","host,principal,pr_node,source,source_generation,mutation_ordinal,repository_id,pr_id","host=?1 AND principal=?2",false),
        ] {
            let mut values=vec![SqlValue::Text(scope.host.clone()),SqlValue::Text(scope.login.clone())];
            if repo {values.push(SqlValue::Text(target.repository().into()));}
            if predicate.contains("?4"){values.push(SqlValue::Text(target.node_id().into()));}
            rows.push(RawRowsV19::capture(tx,table,fields,predicate,values,&mut remaining)?);
        }
        if let Some(catalogue) = source.opening().catalogue_source() {
            rows.push(RawRowsV19::capture(
                tx,
                "saved_views",
                "host,login,name,payload",
                "host=?1 AND login=?2 AND name=?3",
                vec![
                    SqlValue::Text(scope.host.clone()),
                    SqlValue::Text(scope.login.clone()),
                    SqlValue::Text(catalogue.saved_view_key().into()),
                ],
                &mut remaining,
            )?);
        }
        Ok(Self { rows })
    }
    pub(super) fn validate(&self, tx: &Transaction<'_>) -> Result<()> {
        for rows in &self.rows {
            rows.validate(tx)?;
        }
        Ok(())
    }
}

struct PreparedSourceV19 {
    source: String,
    generation: u64,
    binding: Option<SyncBinding>,
    complete: bool,
    rows: EncodedSourceV19,
}
struct LimitedPayloadV19 {
    bytes: Vec<u8>,
    cap: usize,
}
impl std::io::Write for LimitedPayloadV19 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(bytes.len())
            .is_none_or(|n| n > self.cap)
        {
            return Err(std::io::Error::other(
                "Prepared Conversation projection exceeds8MiB",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn encode_records(records: Vec<Record>, remaining: &mut usize) -> Result<Vec<EncodedRecordV19>> {
    if records.len() > 10_001 {
        bail!("Prepared Conversation projection exceeds10001 records");
    }
    let mut keys = std::collections::BTreeSet::new();
    let mut out = Vec::with_capacity(records.len());
    for record in records {
        let metadata = record.kind.len()
            + record.id.len()
            + record.repository.len()
            + record.source_updated_at.as_ref().map_or(0, String::len);
        if metadata > *remaining || !keys.insert((record.kind.clone(), record.id.clone())) {
            bail!("Conversation prepared record bounds/identity refused");
        }
        *remaining -= metadata;
        let mut writer = LimitedPayloadV19 {
            bytes: Vec::new(),
            cap: *remaining,
        };
        serde_json::to_writer(&mut writer, &record.payload)?;
        *remaining -= writer.bytes.len();
        out.push(EncodedRecordV19 {
            kind: record.kind,
            id: record.id,
            repository: record.repository,
            payload: String::from_utf8(writer.bytes)?,
            fetched_at: record.fetched_at,
            source_updated_at: record.source_updated_at,
            complete: record.complete,
        });
    }
    Ok(out)
}
fn old_members(
    tx: &Transaction<'_>,
    scope: &Scope,
    source: &str,
    remaining: &mut usize,
) -> Result<Vec<(String, String)>> {
    let (count,bytes):(usize,usize)=tx.query_row("SELECT count(*),COALESCE(SUM(length(CAST(kind AS BLOB))+length(CAST(id AS BLOB))),0) FROM checkpoint_records WHERE host=?1 AND login=?2 AND source=?3",params![scope.host,scope.login,source],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if count > 10_001 || bytes > *remaining {
        bail!("Old Conversation membership exceeds pre-UID prepared bound");
    }
    *remaining -= bytes;
    let mut statement=tx.prepare("SELECT kind,id FROM checkpoint_records WHERE host=?1 AND login=?2 AND source=?3 ORDER BY kind,id")?;
    let rows = statement
        .query_map(params![scope.host, scope.login, source], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}
fn next_generation(tx: &Transaction<'_>, scope: &Scope, source: &str, fenced: bool) -> Result<u64> {
    let (reserved,published):(Option<u64>,Option<u64>)=tx.query_row("SELECT (SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3),(SELECT generation FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3)",params![scope.host,scope.login,source],|r|Ok((r.get(0)?,r.get(1)?)))?;
    reserved
        .unwrap_or(0)
        .checked_add(u64::from(fenced && reserved.is_some()))
        .map(|n| n.max(published.unwrap_or(0)))
        .and_then(|n| n.checked_add(1))
        .filter(|n| *n <= i64::MAX as u64)
        .context("Fresh Conversation source generation exhausted")
}

pub(crate) struct PreparedConversationCommitV19 {
    cache: Cache,
    terminal: Arc<ConversationTerminalWitnessV19>,
    epoch: EpochV19,
    fence: Option<bindings::PreparedFenceV19>,
    state_fence_v68: Option<state_filter_v68::PreparedStateFenceV68>,
    catalogue: Option<PreparedCatalogueContinuationV19>,
    marker: Weak<()>,
    cutoff_v51: Instant,
    sources: Vec<PreparedSourceV19>,
    read_attempt_v20: Option<Arc<PersonalConversationReadAttemptV20>>,
    atomic_binding_v85: Option<crate::drafts::AtomicCleanupBindingV85>,
}
pub(crate) struct CommittedConversationProjectionV19 {
    catalogue: Option<Arc<CatalogueIntentSource>>,
}
// These CLOSED profiles share the exact same encoding/CAS/commit kernel.
// Neither accepts an arbitrary source counter, predicate or final-UID marker.
enum PreparationV19<'a> {
    PostAtomicV85(
        &'a ConversationRefreshPreparationV19,
        &'a crate::drafts::AtomicCleanupBindingV85,
    ),
    Read {
        own: &'a ConversationOwnFenceV19,
        actual: &'a RefreshReadPreparationV19,
        boundary: PrLocalSourceBoundaryV13<'a>,
        attempt: Option<&'a Arc<PersonalConversationReadAttemptV20>>,
    },
}
impl PreparationV19<'_> {
    fn cutoff_v51(&self) -> Instant {
        match self {
            Self::PostAtomicV85(actual, _) => actual.cutoff_v51(),
            Self::Read { actual, .. } => actual.cutoff_v51(),
        }
    }
    fn parent(&self) -> &Arc<crate::providers::gh::PrResourceParentV1> {
        match self {
            Self::PostAtomicV85(p, _) => p.parent(),
            Self::Read { actual, .. } => actual.parent(),
        }
    }
    fn identity(&self) -> &crate::providers::gh::HostIdentity {
        match self {
            Self::PostAtomicV85(p, _) => p.identity(),
            Self::Read { actual, .. } => actual.identity(),
        }
    }
    fn marker(&self) -> Weak<()> {
        match self {
            Self::PostAtomicV85(p, _) => p.marker_v19(),
            Self::Read { actual, .. } => actual.marker_v19(),
        }
    }
    fn validate_read(&self, cache: &Cache) -> Result<()> {
        match self {
            Self::PostAtomicV85(_, _) => Ok(()),
            Self::Read {
                own,
                boundary,
                attempt,
                ..
            } => match attempt {
                Some(attempt) if attempt.matches_own(own) => {
                    attempt.validate_during_read(cache, *boundary)
                }
                Some(_) => bail!("Personal refresh attempt belongs to another actual original"),
                None => own.validate_during_read(cache, *boundary),
            },
        }
    }
}
enum FinalSealV19 {
    Post(ConversationRefreshCloseSealV19),
    Read(RefreshReadCloseSealV19),
}
impl FinalSealV19 {
    fn cutoff_v51(&self) -> Instant {
        match self {
            Self::Post(actual) => actual.cutoff_v51(),
            Self::Read(actual) => actual.cutoff_v51(),
        }
    }
    fn matches(&self, marker: &Weak<()>, cutoff: Instant) -> bool {
        if self.cutoff_v51() != cutoff {
            return false;
        }
        match self {
            Self::Post(s) => s.matches_v19(marker),
            Self::Read(s) => s.matches_v19(marker),
        }
    }
}
impl CommittedConversationProjectionV19 {
    pub(crate) fn into_catalogue_v19(self) -> Option<Arc<CatalogueIntentSource>> {
        self.catalogue
    }
}
impl PreparedConversationCommitV19 {
    pub(crate) fn prepare_atomic_v85(
        terminal: Arc<ConversationTerminalWitnessV19>,
        source: &Arc<PrCollaborationSourceV17>,
        actual: &ConversationDetailSnapshotV19,
        preparation: &ConversationRefreshPreparationV19,
        supplement: &ConversationPageSupplementV61,
        binding: crate::drafts::AtomicCleanupBindingV85,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        let mut prepared = Self::prepare_v19(
            terminal,
            source,
            actual,
            PreparationV19::PostAtomicV85(preparation, &binding),
            supplement,
            cancel,
        )?;
        prepared.atomic_binding_v85 = Some(binding);
        Ok(prepared)
    }
    pub(crate) fn matches_atomic_ack_v85(
        &self,
        ack: &crate::drafts::DraftPublicationAckV2,
    ) -> bool {
        self.atomic_binding_v85.as_ref().is_some_and(|binding| {
            binding.matches_terminal_v85(&self.terminal) && binding.matches_ack_v85(ack)
        })
    }
    #[cfg(test)]
    pub(crate) fn accepts_atomic_resource_v86(
        &self,
        ack: &crate::drafts::DraftPublicationAckV2,
        client: &crate::drafts::DraftClient,
        resource: &str,
        encoded: &String,
        cancel: &CancellationToken,
    ) -> bool {
        // Closed test cooperation: exercise the actual before-copy checker
        // against this genuine prepared terminal; never return its authority.
        ack.atomic_charge_v85(
            client,
            crate::drafts::AtomicDraftInputV85 {
                database: self.terminal.database(),
                scope: self.terminal.target().scope(),
                resource,
                encoded,
                reference: self.terminal.reference(),
                terminal: &self.terminal,
            },
            cancel,
        )
        .is_ok()
    }
    pub(crate) fn atomic_cutoff_v85(&self) -> Instant {
        self.cutoff_v51.min(self.terminal.publication_cutoff_v51())
    }
    pub(crate) fn prepare_return_v19(
        own: &ConversationOwnFenceV19,
        actual: &ConversationDetailSnapshotV19,
        preparation: &RefreshReadPreparationV19,
        boundary: PrLocalSourceBoundaryV13<'_>,
        attempt: Option<&Arc<PersonalConversationReadAttemptV20>>,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        Self::prepare_v19(
            Arc::clone(own.terminal()),
            own.source(),
            actual,
            PreparationV19::Read {
                own,
                actual: preparation,
                boundary,
                attempt,
            },
            &ConversationPageSupplementV61::None,
            cancel,
        )
    }
    pub(crate) fn prepare_return_issue_links_v94(
        own: &ConversationOwnFenceV19,
        actual: &ConversationDetailSnapshotV19,
        preparation: &RefreshReadPreparationV19,
        boundary: PrLocalSourceBoundaryV13<'_>,
        attempt: Option<&Arc<PersonalConversationReadAttemptV20>>,
        supplement: &ConversationPageSupplementV61,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        if !matches!(supplement, ConversationPageSupplementV61::IssueLinksV87(_))
            || own.terminal().issue_link_command_v87().is_none()
        {
            bail!("Actual Issue terminal and its closed fresh context required");
        }
        Self::prepare_v19(
            Arc::clone(own.terminal()),
            own.source(),
            actual,
            PreparationV19::Read {
                own,
                actual: preparation,
                boundary,
                attempt,
            },
            supplement,
            cancel,
        )
    }
    pub(crate) fn prepare_return_members_v61(
        own: &ConversationOwnFenceV19,
        actual: &ConversationDetailSnapshotV19,
        preparation: &RefreshReadPreparationV19,
        boundary: PrLocalSourceBoundaryV13<'_>,
        attempt: Option<&Arc<PersonalConversationReadAttemptV20>>,
        supplement: &ConversationPageSupplementV61,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        if !matches!(supplement, ConversationPageSupplementV61::Members(_)) {
            bail!("Actual Members return supplement unavailable");
        }
        Self::prepare_v19(
            Arc::clone(own.terminal()),
            own.source(),
            actual,
            PreparationV19::Read {
                own,
                actual: preparation,
                boundary,
                attempt,
            },
            supplement,
            cancel,
        )
    }
    pub(crate) fn prepare_return_metadata_members_v62(
        own: &ConversationOwnFenceV19,
        actual: &ConversationDetailSnapshotV19,
        preparation: &RefreshReadPreparationV19,
        boundary: PrLocalSourceBoundaryV13<'_>,
        attempt: Option<&Arc<PersonalConversationReadAttemptV20>>,
        supplement: &ConversationPageSupplementV61,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        if !matches!(supplement, ConversationPageSupplementV61::Metadata(_))
            || own.terminal().members_command_v48().is_none()
        {
            bail!("Actual Metadata Members return supplement unavailable");
        }
        Self::prepare_v19(
            Arc::clone(own.terminal()),
            own.source(),
            actual,
            PreparationV19::Read {
                own,
                actual: preparation,
                boundary,
                attempt,
            },
            supplement,
            cancel,
        )
    }
    fn prepare_v19(
        terminal: Arc<ConversationTerminalWitnessV19>,
        source: &Arc<PrCollaborationSourceV17>,
        actual: &ConversationDetailSnapshotV19,
        preparation: PreparationV19<'_>,
        supplement: &ConversationPageSupplementV61,
        cancel: &CancellationToken,
    ) -> Result<Self> {
        if cancel.is_cancelled() || Instant::now() >= preparation.cutoff_v51() {
            bail!("Actual Conversation preparation expired");
        }
        if !Arc::ptr_eq(source, terminal.source_v19())
            || Scope::github(
                &preparation.identity().host,
                preparation.identity().account_id,
            )? != *terminal.target().scope()
            || !crate::providers::gh::GithubProvider::same_thread_history_parent_v1(
                terminal.parent(),
                actual.parent(),
            )
            || !Arc::ptr_eq(actual.parent(), preparation.parent())
            || source.opening().database() != terminal.database()
        {
            bail!("Actual Conversation refresh producer/parent/source differs");
        }
        // Revalidate the closed producer inside the kernel, before copying
        // old witnesses, capturing epochs or creating any payload Values.
        if let PreparationV19::PostAtomicV85(_, binding) = &preparation {
            if !binding.matches_terminal_v85(&terminal) {
                bail!("Atomic publication belongs to another actual terminal");
            }
            metadata_storage_v51::validate_atomic_storage_peak_v85(
                source, &terminal, actual, supplement, binding, cancel,
            )?;
        } else {
            match supplement {
                ConversationPageSupplementV61::IssueLinksV87(context) => {
                    metadata_storage_v51::validate_issue_link_storage_peak_v87(
                        source, &terminal, actual, context, cancel,
                    )?;
                }
                ConversationPageSupplementV61::Members(context) => {
                    validate_members_storage_peak_v61(source, &terminal, actual, context, cancel)?;
                }
                ConversationPageSupplementV61::Metadata(context)
                    if terminal.members_command_v48().is_some() =>
                {
                    validate_metadata_members_storage_peak_v62(
                        source, &terminal, actual, context, cancel,
                    )?;
                }
                ConversationPageSupplementV61::None
                | ConversationPageSupplementV61::Metadata(_) => {
                    if terminal.members_command_v48().is_some()
                        || terminal.issue_link_command_v87().is_some()
                    {
                        bail!("Actual Members terminal cannot omit its structured context");
                    }
                }
            }
        }
        terminal.validate_file_v19()?;
        let cache = Cache::open(terminal.database())?;
        preparation.validate_read(&cache)?;
        let tx = cache.connection.unchecked_transaction()?;
        terminal.validate_transaction_v19(&tx)?;
        let fence = if terminal.categories().is_empty() {
            None
        } else {
            Some(bindings::prepare_fence_v19(
                &tx,
                terminal.target().scope(),
                &SyncFence {
                    namespace: SyncNamespace::Collaboration,
                    repository_id: Some(terminal.repository_id()),
                    repository: terminal.target().repository().to_ascii_lowercase(),
                    resource: terminal.target().node_id().into(),
                    kinds: terminal.categories().to_vec(),
                },
                Some(terminal.reference().attempt_id as u64),
            )?)
        };
        let state_fence_v68 = state_filter_v68::PreparedStateFenceV68::prepare(&tx, &terminal)?;
        let fenced = fence.as_ref().is_some_and(|v| v.actually_fences())
            || state_fence_v68
                .as_ref()
                .is_some_and(|v| v.actually_fences());
        if matches!(preparation, PreparationV19::Read { .. }) && fenced {
            bail!("Explicit Refresh must retain its exact ALREADY applied own fence; no category replay");
        }
        let epoch = EpochV19::capture(&tx, &terminal, source)?;
        tx.commit()?;
        // The existing Catalogue validator has its own coherent read
        // transaction. Never nest it inside this prepared capture transaction.
        let catalogue = match &preparation {
            PreparationV19::PostAtomicV85(p, _) => source
                .opening()
                .catalogue_source()
                .map(|old| cache.prepare_catalogue_continuation_v19(old, p, fenced))
                .transpose()?,
            PreparationV19::Read { own, actual, .. } => {
                own.catalogue_for_read_v19(&cache, actual)?
            }
        };
        preparation.validate_read(&cache)?;
        let tx = cache.connection.unchecked_transaction()?;
        terminal.validate_transaction_v19(&tx)?;
        epoch.validate(&tx)?;
        // Preparation, closed envelope, final seal and prepared SQL retain
        // only fixed Instant scalars; charge them to the unchanged8MiB cap.
        let mut remaining = PREPARED_BYTES_V19
            .checked_sub(4 * std::mem::size_of::<Instant>())
            .context("Prepared Conversation cutoff capacity exceeded")?;
        if let PreparationV19::PostAtomicV85(_, binding) = &preparation {
            remaining = remaining
                .checked_sub(binding.retained_bytes_v85())
                .context("Atomic publication prepared capacity exceeded before copies")?;
        }
        let mut sources = Vec::new();
        let fetched_at = now();
        let snapshots =
            crate::search::collaboration::DiscussionSnapshot::from_conversation_refresh_v19(
                terminal.target().scope().clone(),
                actual,
                fetched_at,
            )
            .map_err(anyhow::Error::msg)?;
        for snapshot in snapshots {
            if cancel.is_cancelled() {
                bail!("Conversation snapshot preparation cancelled");
            }
            let name = snapshot.target.source();
            let binding = snapshot.target.binding().map_err(anyhow::Error::msg)?;
            let records = crate::search::collaboration::prepare_complete_conversation_records_v19(
                &snapshot, cancel,
            )
            .map_err(anyhow::Error::msg)?;
            let previous_members =
                old_members(&tx, terminal.target().scope(), &name, &mut remaining)?;
            let records = encode_records(records, &mut remaining)?;
            sources.push(PreparedSourceV19 {
                generation: next_generation(&tx, terminal.target().scope(), &name, fenced)?,
                source: name,
                binding: Some(binding),
                complete: snapshot.complete,
                rows: EncodedSourceV19 {
                    records,
                    previous_members,
                    published_at: fetched_at,
                },
            });
        }
        let projection_source = format!("pr-write-projection:{}", terminal.target().node_id());
        let previous_members = old_members(
            &tx,
            terminal.target().scope(),
            &projection_source,
            &mut remaining,
        )?;
        let projection = match supplement {
            ConversationPageSupplementV61::IssueLinksV87(context) => {
                crate::pr_projection::prepare_actual_issue_link_records_v87(
                    actual, context, &terminal,
                )
            }
            ConversationPageSupplementV61::Members(context) => {
                crate::pr_projection::prepare_actual_members_records_v61(
                    actual,
                    context,
                    terminal.target(),
                )
            }
            ConversationPageSupplementV61::Metadata(context)
                if terminal.members_command_v48().is_some() =>
            {
                crate::pr_projection::prepare_actual_metadata_members_records_v62(
                    actual,
                    context,
                    terminal.target(),
                    terminal
                        .members_command_v48()
                        .context("Actual Metadata Members terminal unavailable")?,
                )
            }
            ConversationPageSupplementV61::None | ConversationPageSupplementV61::Metadata(_) => {
                crate::pr_projection::prepare_actual_conversation_records_v19(
                    actual,
                    terminal.target(),
                )
            }
        }
        .map_err(anyhow::Error::msg)?;
        sources.push(PreparedSourceV19 {
            source: projection_source,
            generation: if state_fence_v68.is_some() {
                next_generation(
                    &tx,
                    terminal.target().scope(),
                    &format!("pr-write-projection:{}", terminal.target().node_id()),
                    fenced,
                )?
            } else {
                terminal.reference().attempt_id as u64
            },
            binding: None,
            complete: actual.details().coverage.complete,
            rows: EncodedSourceV19 {
                records: encode_records(projection, &mut remaining)?,
                previous_members,
                published_at: fetched_at,
            },
        });
        terminal.validate_transaction_v19(&tx)?;
        epoch.validate(&tx)?;
        tx.commit()?;
        terminal.validate_file_v19()?;
        let read_attempt_v20 = match &preparation {
            PreparationV19::Read { attempt, .. } => attempt.cloned(),
            PreparationV19::PostAtomicV85(_, _) => None,
        };
        Ok(Self {
            cache,
            terminal,
            epoch,
            fence,
            state_fence_v68,
            catalogue,
            marker: preparation.marker(),
            cutoff_v51: preparation.cutoff_v51(),
            sources,
            read_attempt_v20,
            atomic_binding_v85: None,
        })
    }
    pub(crate) fn commit_read_v19(
        self,
        seal: RefreshReadCloseSealV19,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<CommittedConversationProjectionV19> {
        self.commit_v19(FinalSealV19::Read(seal), cancel, deadline, None)
    }
    pub(crate) fn commit_publication_v85(
        self,
        seal: ConversationRefreshCloseSealV19,
        cleanup: &crate::drafts::AcknowledgedProjectionCleanupV85<'_>,
        cancel: &CancellationToken,
    ) -> std::result::Result<
        CommittedConversationProjectionV19,
        crate::drafts::AtomicPublicationFailureV85,
    > {
        use crate::drafts::AtomicPublicationFailureV85 as Failure;
        if !cleanup.same_token_v85(cancel)
            || !self.atomic_binding_v85.as_ref().is_some_and(|binding| {
                binding.matches_terminal_v85(&self.terminal) && cleanup.matches_binding_v85(binding)
            })
        {
            return Err(Failure::Retained(
                "Atomic cleanup/actual prepared projection binding differs".into(),
            ));
        }
        let deadline = self
            .atomic_cutoff_v85()
            .min(seal.cutoff_v51())
            .min(cleanup.deadline_v85());
        let committing = std::cell::Cell::new(false);
        self.commit_v19(
            FinalSealV19::Post(seal),
            cancel,
            deadline,
            Some(AtomicCommitV85 {
                cleanup,
                committing: &committing,
            }),
        )
        .map_err(|error| {
            if committing.get() {
                Failure::Unknown(error.to_string())
            } else {
                Failure::Retained(error.to_string())
            }
        })
    }
    fn commit_v19(
        mut self,
        seal: FinalSealV19,
        cancel: &CancellationToken,
        deadline: Instant,
        atomic: Option<AtomicCommitV85<'_>>,
    ) -> Result<CommittedConversationProjectionV19> {
        let deadline = deadline.min(self.cutoff_v51).min(seal.cutoff_v51());
        let check = || -> Result<()> {
            if cancel.is_cancelled() || Instant::now() >= deadline {
                bail!("Original Conversation terminal cancelled/expired");
            }
            Ok(())
        };
        check()?;
        if !seal.matches(&self.marker, self.cutoff_v51) {
            bail!("Original Conversation final UID/preparation seal differs");
        }
        self.terminal.validate_file_v19()?;
        if let Some(atomic) = &atomic {
            atomic
                .cleanup
                .check_current_v85()
                .map_err(anyhow::Error::msg)?;
        }
        let _busy = crate::storage::busy_v1::until(&self.cache.connection, deadline)?;
        let tx = self
            .cache
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        check()?;
        self.terminal.validate_file_v19()?;
        self.terminal.validate_transaction_v19(&tx)?;
        self.epoch.validate(&tx)?;
        if let Some(fence) = &self.fence {
            bindings::apply_prepared_fence_v19(&tx, fence)?;
        }
        if let Some(fence) = &self.state_fence_v68 {
            fence.apply(&tx)?;
        }
        if let Some(catalogue) = &self.catalogue {
            catalogue.apply_after_fence_v19(&tx)?;
        }
        for source in &self.sources {
            check()?;
            if self.state_fence_v68.is_some() && source.binding.is_none() {
                tx.execute("INSERT INTO sync_reservations VALUES(?1,?2,?3,?4,1) ON CONFLICT(host,login,source) DO UPDATE SET generation=excluded.generation,active=1",params![self.terminal.target().scope().host,self.terminal.target().scope().login,source.source,source.generation])?;
            }
            if let Some(binding) = &source.binding {
                tx.execute("INSERT INTO sync_reservations VALUES(?1,?2,?3,?4,1) ON CONFLICT(host,login,source) DO UPDATE SET generation=excluded.generation,active=1",params![self.terminal.target().scope().host,self.terminal.target().scope().login,source.source,source.generation])?;
                tx.execute("INSERT INTO sync_resource_bindings VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,NULL,1) ON CONFLICT(host,login,source) DO UPDATE SET reservation_generation=excluded.reservation_generation,active=1",params![self.terminal.target().scope().host,self.terminal.target().scope().login,source.source,"collaboration",binding.repository_id,binding.repository,binding.resource,match binding.kind {SyncBindingKind::PullRequestBody=>"pull_request_body",SyncBindingKind::ConversationComment=>"conversation_comment",_=>bail!("Closed Conversation binding category differs")},source.generation])?;
            }
            if !Cache::publish_sync_rows_v19(
                &tx,
                self.terminal.target().scope(),
                &source.source,
                source.generation,
                SyncRowsV19::Prepared(&source.rows),
                SyncPolicy {
                    complete: source.complete,
                    prune_missing: true,
                    reservation_authoritative: true,
                },
            )? {
                bail!("Newer/equal Conversation frame already published; no stale fence retained");
            }
            if source.binding.is_some() {
                bindings::mark_binding_published(
                    &tx,
                    self.terminal.target().scope(),
                    &source.source,
                    source.generation,
                )?;
            }
            if let Some(fence) = &self.state_fence_v68 {
                fence.published_source(&tx, &source.source, source.generation)?;
            }
        }
        if let Some(atomic) = &atomic {
            atomic.cleanup.delete_in_v85(&tx)?;
        }
        check()?;
        self.terminal.validate_file_v19()?;
        if let Some(atomic) = &atomic {
            // Short producer check only. Release its mutex before fsync; a
            // newer accepted revision afterward yields UNKNOWN in the worker.
            atomic
                .cleanup
                .check_current_v85()
                .map_err(anyhow::Error::msg)?;
            atomic.committing.set(true);
        }
        if let Some(attempt) = &self.read_attempt_v20 {
            attempt.mark_commit_unproven();
        }
        tx.commit()?;
        if let Some(attempt) = &self.read_attempt_v20 {
            attempt.completed_page();
        }
        Ok(CommittedConversationProjectionV19 {
            catalogue: self
                .catalogue
                .map(PreparedCatalogueContinuationV19::into_committed_v19),
        })
    }
}
struct AtomicCommitV85<'a> {
    cleanup: &'a crate::drafts::AcknowledgedProjectionCleanupV85<'a>,
    committing: &'a std::cell::Cell<bool>,
}
