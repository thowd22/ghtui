//! Exact bounded original Metadata history and local ACK. Historical H/base,
//! requested value and receipt remain observations, never current permission.
use super::*;
use crate::providers::gh::{BaseRecoveryCurrentV32, PrResourceParentV1};
use std::time::Instant;
pub(crate) struct BaseHistoryRowV32 {
    pub(crate) marker: PrWriteMarker,
    pub(crate) origin: BaseOriginRecordV32,
    pub(crate) body: Option<(String, BTreeMap<String, String>)>,
}
pub(crate) struct BaseHistoryV32 {
    database: DatabaseV3,
    pub(crate) generation: i64,
    pub(crate) complete: bool,
    pub(crate) rows: Vec<BaseHistoryRowV32>,
}
pub(crate) struct BaseRecoveryWitnessV32 {
    pub(in crate::pr_write_journal) target: PrWriteTarget,
    pub(in crate::pr_write_journal) marker: PrWriteMarker,
    pub(in crate::pr_write_journal) generation: i64,
    pub(crate) origin: BaseOriginRecordV32,
    exact: Option<exact_ack_v17::ExactAckRowV17>,
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
    pub(crate) fn base_history_v32(
        &self,
        target: &PrWriteTarget,
        parent: &PrResourceParentV1,
    ) -> Result<BaseHistoryV32> {
        let database = DatabaseV3::capture(&self.path)?;
        let generation = self.source_generation(target)?;
        if let Some(blocker) = self.blocking(target)? {
            let origin = self.metadata_base_origin_v32(target, &blocker)?;
            if !origin.seed.same_object(parent) {
                bail!("A different exact purpose/field blocks metadata inspection");
            }
        }
        let ids = {
            let mut statement = self.connection.prepare("SELECT attempt_id FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND command=?4 ORDER BY attempt_id DESC LIMIT 65")?;
            let ids = statement
                .query_map(
                    params![
                        target.scope.host,
                        target.scope.login,
                        target.pr_node_id,
                        COMMAND_V32
                    ],
                    |r| r.get::<_, i64>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids
        };
        let complete = ids.len() <= 64;
        let mut rows = Vec::new();
        let mut retained = 0usize;
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
                bail!("Metadata journal row exceeds8KiB before copy");
            }
            let raw = self.connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1", [id], read_marker)?;
            let marker = decode_marker(target, raw)?;
            let origin = self.metadata_base_origin_v32(target, &marker)?;
            if !origin.seed.same_object(parent) {
                bail!("Actual current parent differs from original metadata object");
            }
            retained = retained
                .checked_add(bytes)
                .and_then(|n| n.checked_add(bounded_encode(&origin).ok()?.len()))
                .filter(|n| *n <= 1024 * 1024)
                .context("Metadata history retained metadata exceeds1MiB")?;
            let allowance = (1024 * 1024usize)
                .checked_sub(retained)
                .context("Metadata history capacity ended")?
                .min(128 * 1024);
            let saved = Cache::open(&self.path)?.bounded_draft(
                target.scope(),
                &marker.selected_resource,
                allowance,
            )?;
            let body = saved.map(|encoded| {
                #[derive(Deserialize)] #[serde(deny_unknown_fields)]
                struct Envelope { version:u8, values:BTreeMap<String,String>, publication:Option<PrPublicationReference> }
                let parsed: Envelope = serde_json::from_str(&encoded)?;
                if parsed.version != 17 || parsed.values.len() != 2
                    || parsed.values.get("base").is_none_or(|s| s.len() > 1024)
                    || parsed.values.get("tip").is_none_or(|s| s.len() > 64)
                    || parsed.publication.as_ref() != Some(&marker.publication_reference())
                    || origin.seed.write(&parsed.values)?.digest()? != marker.payload_sha256 {
                    bail!("Exact original metadata body/reference/fingerprint differs; no restoration");
                }
                Ok::<_, anyhow::Error>((encoded, parsed.values))
            }).transpose()?;
            if let Some((encoded, values)) = &body {
                retained = retained
                    .checked_add(encoded.len())
                    .and_then(|n| n.checked_add(values.values().map(String::len).sum::<usize>()))
                    .filter(|n| *n <= 1024 * 1024)
                    .context("Metadata history protected body exceeds1MiB")?;
            }
            rows.push(BaseHistoryRowV32 {
                marker,
                origin,
                body,
            });
        }
        if database != DatabaseV3::capture(&self.path)?
            || generation != self.source_generation(target)?
        {
            bail!("Exact metadata DB/generation changed during bounded read");
        }
        Ok(BaseHistoryV32 {
            database,
            generation,
            complete,
            rows,
        })
    }
    pub(crate) fn validate_base_history_v32(
        &self,
        target: &PrWriteTarget,
        parent: &PrResourceParentV1,
        expected: &BaseHistoryV32,
    ) -> Result<()> {
        let actual = self.base_history_v32(target, parent)?;
        if expected.database != actual.database
            || expected.generation != actual.generation
            || expected.complete != actual.complete
            || expected.rows.len() != actual.rows.len()
        {
            bail!("Exact metadata history/body/generation changed");
        }
        for (a, b) in expected.rows.iter().zip(&actual.rows) {
            if !same_marker(&a.marker, &b.marker)
                || bounded_encode(&a.origin)? != bounded_encode(&b.origin)?
                || a.body != b.body
            {
                bail!("Exact metadata original/ref/body changed");
            }
        }
        Ok(())
    }
    pub(crate) fn prepare_base_inspection_v32(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
    ) -> Result<BaseRecoveryWitnessV32> {
        let marker = self
            .original_publication(target, reference)?
            .context("Exact metadata attempt absent")?;
        let blocker = self
            .blocking(target)?
            .context("No exact unresolved metadata blocker")?;
        let origin = self.metadata_base_origin_v32(target, &marker)?;
        if !same_marker(&marker, &blocker)
            || !matches!(
                marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            )
        {
            bail!("Only the exact unresolved metadata attempt can be acknowledged");
        }
        let mut witness = BaseRecoveryWitnessV32 {
            target: target.clone(),
            marker,
            generation: self.source_generation(target)?,
            origin,
            exact: None,
        };
        witness.exact = Some(exact_ack_v17::ExactAckRowV17::metadata_base_v32(
            self, &witness,
        )?);
        self.validate_base_inspection_v32(&witness)?;
        Ok(witness)
    }
    pub(crate) fn validate_base_inspection_v32(
        &self,
        witness: &BaseRecoveryWitnessV32,
    ) -> Result<()> {
        let marker = self
            .original_publication(&witness.target, &witness.marker.publication_reference())?
            .context("Original metadata blocker absent")?;
        let blocker = self
            .blocking(&witness.target)?
            .context("Original metadata no longer blocks")?;
        let origin = self.metadata_base_origin_v32(&witness.target, &marker)?;
        if self.source_generation(&witness.target)? != witness.generation
            || !same_marker(&marker, &witness.marker)
            || !same_marker(&blocker, &witness.marker)
            || bounded_encode(&origin)? != bounded_encode(&witness.origin)?
        {
            bail!("Exact metadata inspection changed; no local release");
        }
        if let Some(exact) = &witness.exact {
            exact.validate_current(self)?;
        }
        Ok(())
    }
    pub(crate) fn acknowledge_base_inspection_v32(
        &mut self,
        provider: &GithubProvider,
        mut witness: BaseRecoveryWitnessV32,
        observation: BaseRecoveryCurrentV32,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<usize> {
        if cancel.is_cancelled() || Instant::now() >= deadline {
            bail!("Original metadata ACK clock ended; blocker retained");
        }
        if !observation.matches_target(&witness.target)
            || !witness
                .origin
                .seed
                .same_object(observation.actual().parent())
        {
            bail!("Actual metadata inspection target/field differs");
        }
        let row = witness
            .exact
            .take()
            .context("Original metadata raw ACK already consumed")?;
        row.validate_current(self)?;
        provider
            .reconcile_base_inspection_v32(observation, cancel, deadline)
            .map_err(anyhow::Error::msg)?;
        // Runner release precedes the SAME raw SQL CAS. Any CAS failure leaves
        // the durable blocker, so no resend or success is inferred.
        self.commit_exact_ack_v17(row, cancel, deadline)
    }
}
