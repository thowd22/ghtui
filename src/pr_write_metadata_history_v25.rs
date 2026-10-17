//! Exact bounded original Metadata history and local ACK. Historical H/base,
//! requested value and receipt remain observations, never current permission.
use super::*;
use crate::providers::gh::{MetadataRecoveryCurrentV25, PrResourceParentV1};
use std::time::Instant;
pub(crate) struct MetadataHistoryRowV25 {
    pub(crate) marker: PrWriteMarker,
    pub(crate) origin: MetadataOriginRecordV25,
    pub(crate) body: Option<(String, BTreeMap<String, String>)>,
}
pub(crate) struct MetadataHistoryV25 {
    database: DatabaseV3,
    pub(crate) generation: i64,
    pub(crate) complete: bool,
    pub(crate) field: PrMetadataTextFieldV25,
    pub(crate) rows: Vec<MetadataHistoryRowV25>,
}
pub(crate) struct MetadataRecoveryWitnessV25 {
    pub(in crate::pr_write_journal) target: PrWriteTarget,
    pub(in crate::pr_write_journal) marker: PrWriteMarker,
    pub(in crate::pr_write_journal) generation: i64,
    pub(crate) origin: MetadataOriginRecordV25,
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
    pub(crate) fn metadata_history_v25(
        &self,
        target: &PrWriteTarget,
        parent: &PrResourceParentV1,
        field: PrMetadataTextFieldV25,
    ) -> Result<MetadataHistoryV25> {
        let database = DatabaseV3::capture(&self.path)?;
        let generation = self.source_generation(target)?;
        if let Some(blocker) = self.blocking(target)? {
            let origin = self.metadata_origin_v25(target, &blocker)?;
            if origin.seed.field != field || !origin.seed.same_object(parent) {
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
                        field.command()
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
            let origin = self.metadata_origin_v25(target, &marker)?;
            if origin.seed.field != field || !origin.seed.same_object(parent) {
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
                if parsed.version != 17 || parsed.values.len() != 1
                    || parsed.values.get("body").is_none_or(|s| s.len() > 65_536)
                    || parsed.publication.as_ref() != Some(&marker.publication_reference())
                    || origin.seed.write(&parsed.values)?.digest()? != marker.payload_sha256 {
                    bail!("Exact original metadata body/reference/fingerprint differs; no restoration");
                }
                Ok::<_, anyhow::Error>((encoded, parsed.values))
            }).transpose()?;
            if let Some((encoded, values)) = &body {
                retained = retained
                    .checked_add(encoded.len())
                    .and_then(|n| n.checked_add(values["body"].len()))
                    .filter(|n| *n <= 1024 * 1024)
                    .context("Metadata history protected body exceeds1MiB")?;
            }
            rows.push(MetadataHistoryRowV25 {
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
        Ok(MetadataHistoryV25 {
            database,
            generation,
            complete,
            field,
            rows,
        })
    }
    pub(crate) fn validate_metadata_history_v25(
        &self,
        target: &PrWriteTarget,
        parent: &PrResourceParentV1,
        expected: &MetadataHistoryV25,
    ) -> Result<()> {
        let actual = self.metadata_history_v25(target, parent, expected.field)?;
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
    pub(crate) fn prepare_metadata_inspection_v25(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
    ) -> Result<MetadataRecoveryWitnessV25> {
        let marker = self
            .original_publication(target, reference)?
            .context("Exact metadata attempt absent")?;
        let blocker = self
            .blocking(target)?
            .context("No exact unresolved metadata blocker")?;
        let origin = self.metadata_origin_v25(target, &marker)?;
        if !same_marker(&marker, &blocker)
            || !matches!(
                marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            )
        {
            bail!("Only the exact unresolved metadata attempt can be acknowledged");
        }
        let mut witness = MetadataRecoveryWitnessV25 {
            target: target.clone(),
            marker,
            generation: self.source_generation(target)?,
            origin,
            exact: None,
        };
        witness.exact = Some(exact_ack_v17::ExactAckRowV17::metadata(self, &witness)?);
        self.validate_metadata_inspection_v25(&witness)?;
        Ok(witness)
    }
    pub(crate) fn validate_metadata_inspection_v25(
        &self,
        witness: &MetadataRecoveryWitnessV25,
    ) -> Result<()> {
        let marker = self
            .original_publication(&witness.target, &witness.marker.publication_reference())?
            .context("Original metadata blocker absent")?;
        let blocker = self
            .blocking(&witness.target)?
            .context("Original metadata no longer blocks")?;
        let origin = self.metadata_origin_v25(&witness.target, &marker)?;
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
    pub(crate) fn acknowledge_metadata_inspection_v25(
        &mut self,
        provider: &GithubProvider,
        mut witness: MetadataRecoveryWitnessV25,
        observation: MetadataRecoveryCurrentV25,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<usize> {
        if cancel.is_cancelled() || Instant::now() >= deadline {
            bail!("Original metadata ACK clock ended; blocker retained");
        }
        if !observation.matches_target(&witness.target)
            || observation.actual().observed().field() != witness.origin.seed.field
        {
            bail!("Actual metadata inspection target/field differs");
        }
        let row = witness
            .exact
            .take()
            .context("Original metadata raw ACK already consumed")?;
        row.validate_current(self)?;
        provider
            .reconcile_metadata_inspection_v25(observation, cancel, deadline)
            .map_err(anyhow::Error::msg)?;
        // Runner release precedes the SAME raw SQL CAS. Any CAS failure leaves
        // the durable blocker, so no resend or success is inferred.
        self.commit_exact_ack_v17(row, cancel, deadline)
    }
}
