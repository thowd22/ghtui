//! Exact original State history. Saved request/ref/origin is inspect-only;
//! neither historical H/B nor an ACK manufactures a new transition producer.
use super::*;
use crate::providers::gh::StateRecoveryCurrentV68;
use std::time::Instant;

#[cfg(test)]
#[path = "pr_write_state_cross_family_v106_tests.rs"]
pub(in crate::pr_write_journal) mod cross_family_v106;

pub(crate) struct StateHistoryRowV74 {
    pub(crate) marker: PrWriteMarker,
    pub(crate) origin: StateOriginRecordV68,
    pub(crate) body: Option<String>,
}
pub(crate) struct StateHistoryV74 {
    database: DatabaseV3,
    pub(crate) generation: i64,
    pub(crate) complete: bool,
    pub(crate) command: PrStateCommand,
    pub(crate) rows: Vec<StateHistoryRowV74>,
}
pub(crate) struct StateRecoveryWitnessV74 {
    pub(in crate::pr_write_journal) target: PrWriteTarget,
    pub(in crate::pr_write_journal) marker: PrWriteMarker,
    pub(in crate::pr_write_journal) generation: i64,
    pub(crate) origin: StateOriginRecordV68,
    database: DatabaseV3,
    original_cache: Option<std::sync::Arc<crate::storage::StateAckQueueBoundaryV78>>,
    exact: Option<exact_ack_v17::ExactAckRowV17>,
}
impl StateRecoveryWitnessV74 {
    /// Data-only scope of this genuine original journal witness, not current
    /// access, a Source factory or permission to issue any SQL transition.
    pub(crate) fn cache_scope_v77(&self) -> &Scope {
        self.target.scope()
    }
    pub(crate) fn matches_original_queue_v78(
        &self,
        original: &crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    ) -> bool {
        self.target.scope() == original.scope()
            && original.admits_state_v68(self.origin.seed.command)
    }
    pub(crate) fn cache_file_v78(&self) -> StateOriginalFileV78 {
        StateOriginalFileV78(self.database.clone())
    }
    pub(crate) fn cache_guard_v78(
        &self,
    ) -> Result<std::sync::Arc<crate::storage::StateAckQueueBoundaryV78>> {
        self.original_cache
            .as_ref()
            .map(std::sync::Arc::clone)
            .context("Original pre-read State queue boundary absent")
    }
}
/// File-only restriction from the actual typed journal witness. No arbitrary
/// path/issuer constructor, source, principal or SQL authority is exposed.
pub(crate) struct StateOriginalFileV78(DatabaseV3);
impl StateOriginalFileV78 {
    pub(crate) fn validate(&self, path: &Path) -> Result<()> {
        if self.0 != DatabaseV3::capture(path)? {
            bail!("Original State inspection file changed");
        }
        Ok(())
    }
}
fn same_marker(a: &PrWriteMarker, b: &PrWriteMarker) -> bool {
    a.target == b.target
        && a.publication_reference() == b.publication_reference()
        && a.command == b.command
        && a.selected_resource == b.selected_resource
        && a.state == b.state
        && a.confirmed == b.confirmed
        && a.created_at == b.created_at
}
impl PrWriteJournal {
    pub(crate) fn state_history_v74(
        &self,
        target: &PrWriteTarget,
        parent: &PrResourceParentV1,
        command: PrStateCommand,
    ) -> Result<StateHistoryV74> {
        if !command_supported(command) {
            bail!("No native State history for this command");
        }
        let database = DatabaseV3::capture(&self.path)?;
        let generation = self.source_generation(target)?;
        if let Some(blocker) = self.blocking(target)? {
            let origin = self.state_origin_v68(target, &blocker)?;
            if origin.seed.command != command || !origin.seed.same_object_v74(parent) {
                bail!("A different exact original purpose blocks State history");
            }
        }
        // Include known malformed native rows BEFORE origin validation. Do
        // not filter them away to resurrect an older apparently valid row.
        let mut statement = self.connection.prepare("SELECT a.attempt_id FROM pr_write_attempts a LEFT JOIN pr_write_state_origins_v68 s ON s.attempt_id=a.attempt_id WHERE a.host=?1 AND a.principal=?2 AND a.pr_node=?3 AND (a.command LIKE 'state-%' OR a.selected_resource LIKE 'state-v68/%' OR s.attempt_id IS NOT NULL) ORDER BY a.attempt_id DESC LIMIT 65")?;
        let ids = statement
            .query_map(
                params![target.scope.host, target.scope.login, target.pr_node_id],
                |r| r.get::<_, i64>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        drop(statement);
        let complete = ids.len() <= 64;
        let mut rows = Vec::with_capacity(ids.len().min(64));
        let mut retained = rows
            .capacity()
            .checked_mul(std::mem::size_of::<StateHistoryRowV74>())
            .context("State history layout overflow")?;
        for id in ids.into_iter().take(64) {
            let bytes: usize = self.connection.query_row(
                &format!(
                    "SELECT {} FROM pr_write_attempts WHERE attempt_id=?1",
                    source_metadata::METADATA_BYTES_SQL
                ),
                [id],
                |r| r.get(0),
            )?;
            let origin_bytes:usize=self.connection.query_row("SELECT length(CAST(origin_json AS BLOB))+COALESCE(length(CAST(base_key_v71 AS BLOB)),0)+COALESCE(length(CAST(action_tag_v71 AS BLOB)),0)+64 FROM pr_write_state_origins_v68 WHERE attempt_id=?1",[id],|r|r.get(0))?;
            if bytes > 8192 || origin_bytes > ORIGIN_CAP + 384 {
                bail!("State history raw row/origin exceeds bounded profile before copy");
            }
            // Count raw plus decoded envelope conservatively before strings
            // are materialized, including the bounded serialized disposition.
            retained = retained
                .checked_add(bytes)
                .and_then(|n| n.checked_add(origin_bytes.checked_mul(3)?))
                .and_then(|n| n.checked_add(3 * (8192 + 32)))
                .filter(|n| *n <= 1024 * 1024)
                .context("State history metadata exceeds1MiB before copy")?;
            let raw=self.connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],read_marker)?;
            let marker = decode_marker(target, raw)?;
            let origin = self.state_origin_v68(target, &marker)?;
            if !origin.seed.same_object_v74(parent) {
                bail!("Historical State parent identity differs from actual current object");
            }
            if origin.seed.command != command {
                continue;
            }
            let remaining = (1024 * 1024usize)
                .checked_sub(retained)
                .context("State history capacity exhausted")?;
            let body = Cache::open(&self.path)?.bounded_draft(
                target.scope(),
                &marker.selected_resource,
                remaining.min(8192),
            )?;
            if let Some(encoded) = &body {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Envelope {
                    version: u8,
                    values: BTreeMap<String, String>,
                    publication: Option<PrPublicationReference>,
                }
                let parsed: Envelope = serde_json::from_str(encoded)?;
                if parsed.version != 17
                    || !parsed.values.is_empty()
                    || parsed.publication.as_ref() != Some(&marker.publication_reference())
                    || origin.seed.write(&parsed.values)?.digest()? != marker.payload_sha256
                {
                    bail!("Protected State request/ref/fingerprint differs; never restore");
                }
                retained = retained
                    .checked_add(encoded.capacity())
                    .filter(|n| *n <= 1024 * 1024)
                    .context("State history protected envelope exceeds1MiB")?;
            }
            rows.push(StateHistoryRowV74 {
                marker,
                origin,
                body,
            });
        }
        if database != DatabaseV3::capture(&self.path)?
            || generation != self.source_generation(target)?
        {
            bail!("State history original file/generation changed");
        }
        Ok(StateHistoryV74 {
            database,
            generation,
            complete,
            command,
            rows,
        })
    }
    pub(crate) fn validate_state_history_v74(
        &self,
        target: &PrWriteTarget,
        parent: &PrResourceParentV1,
        expected: &StateHistoryV74,
    ) -> Result<()> {
        let current = self.state_history_v74(target, parent, expected.command)?;
        if current.database != expected.database
            || current.generation != expected.generation
            || current.complete != expected.complete
            || current.rows.len() != expected.rows.len()
        {
            bail!("State history exact window/generation changed");
        }
        for (actual, old) in current.rows.iter().zip(&expected.rows) {
            if !same_marker(&actual.marker, &old.marker)
                || bounded_encode(&actual.origin)? != bounded_encode(&old.origin)?
                || actual.body != old.body
            {
                bail!("State history exact raw original/ref/body changed");
            }
        }
        Ok(())
    }
    pub(crate) fn prepare_state_inspection_v74(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
        original: &crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17,
    ) -> Result<StateRecoveryWitnessV74> {
        if original.database() != self.path.as_path() {
            bail!("Original State queue database path differs");
        }
        let marker = self
            .original_publication(target, reference)?
            .context("Exact State attempt absent")?;
        let blocker = self
            .blocking(target)?
            .context("No original State blocker")?;
        let origin = self.state_origin_v68(target, &marker)?;
        if !same_marker(&marker, &blocker)
            || !matches!(
                marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            )
        {
            bail!("Only exact unresolved original State blocker can be acknowledged");
        }
        let mut witness = StateRecoveryWitnessV74 {
            target: target.clone(),
            marker,
            generation: self.source_generation(target)?,
            origin,
            database: DatabaseV3::capture(&self.path)?,
            original_cache: None,
            exact: None,
        };
        let tx = self.connection.unchecked_transaction()?;
        witness.original_cache = Some(std::sync::Arc::new(
            crate::storage::StateAckQueueBoundaryV78::capture(&tx, &witness, original)?,
        ));
        tx.commit()?;
        self.validate_state_inspection_v74(&witness)?;
        Ok(witness)
    }
    /// Finish raw preparation after the legitimate own Personal reservation,
    /// retaining the ORIGINAL pre-read queue boundary without any recapture.
    pub(crate) fn close_state_inspection_v78(
        &self,
        witness: &mut StateRecoveryWitnessV74,
    ) -> Result<()> {
        self.validate_state_inspection_v74(witness)?;
        if witness.exact.is_some() {
            bail!("Original State inspection already prepared");
        }
        let original_cache = witness
            .original_cache
            .take()
            .context("Original pre-read State cache boundary absent")?;
        witness.exact = Some(exact_ack_v17::ExactAckRowV17::state_v74(
            self,
            witness,
            original_cache,
        )?);
        self.validate_state_inspection_v74(witness)
    }
    pub(crate) fn validate_state_inspection_v74(
        &self,
        witness: &StateRecoveryWitnessV74,
    ) -> Result<()> {
        if witness.database != DatabaseV3::capture(&self.path)? {
            bail!("Original State inspection database file changed");
        }
        if let Some(original_cache) = &witness.original_cache {
            original_cache.validate(&self.connection)?;
        }
        let marker = self
            .original_publication(&witness.target, &witness.marker.publication_reference())?
            .context("Original State attempt absent")?;
        let blocker = self
            .blocking(&witness.target)?
            .context("Original State no longer blocks")?;
        let origin = self.state_origin_v68(&witness.target, &marker)?;
        if self.source_generation(&witness.target)? != witness.generation
            || !same_marker(&marker, &witness.marker)
            || !same_marker(&blocker, &witness.marker)
            || bounded_encode(&origin)? != bounded_encode(&witness.origin)?
        {
            bail!("Exact original State inspection changed; durable blocker retained");
        }
        if let Some(exact) = &witness.exact {
            exact.validate_current(self)?;
        }
        Ok(())
    }
    pub(crate) fn acknowledge_state_inspection_v74(
        &mut self,
        provider: &GithubProvider,
        mut witness: StateRecoveryWitnessV74,
        observation: StateRecoveryCurrentV68,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<usize> {
        let actual = observation.actual();
        if cancel.is_cancelled()
            || Instant::now() >= deadline.min(observation.deadline())
            || !observation.matches_target(&witness.target)
            || actual.observed().command() != witness.origin.seed.command
            || actual.observed().issue_id() != witness.origin.seed.issue_id
            || actual.observed().issue_node() != witness.origin.seed.issue_node
            || !witness
                .origin
                .seed
                .same_object_v74(actual.observed().parent())
        {
            bail!("Actual State read/ACK command, independent issue or clock differs");
        }
        let deadline = deadline.min(observation.deadline());
        let row = witness.exact.take().context("Exact State ACK consumed")?;
        row.validate_current(self)?;
        provider
            .reconcile_state_inspection_v68(observation, cancel, deadline)
            .map_err(anyhow::Error::msg)?;
        // Runner reconciliation is not durable journal ACK. Failed raw SQL
        // CAS remains blocked, protected and nonresendable.
        self.commit_exact_ack_v17(row, cancel, deadline)
    }
}
