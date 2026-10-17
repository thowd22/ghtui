//! Bounded all-ten history and exact local ACK on the SAME journal/transition.
//! Persisted origin/ref/body is inspect-only and cannot construct a producer.
use super::*;
use crate::providers::gh::MetadataMembersRecoveryCurrentV48;
use std::time::Instant;
pub(crate) struct MetadataMembersHistoryRowV48 {
    pub(crate) marker: PrWriteMarker,
    pub(crate) origin: MetadataMembersOriginRecordV48,
    pub(crate) body: Option<(String, BTreeMap<String, String>)>,
}
pub(crate) struct MetadataMembersHistoryV48 {
    database: DatabaseV3,
    pub(crate) generation: i64,
    pub(crate) complete: bool,
    pub(crate) rows: Vec<MetadataMembersHistoryRowV48>,
}
pub(crate) struct MetadataMembersRecoveryWitnessV48 {
    pub(in crate::pr_write_journal) target: PrWriteTarget,
    pub(in crate::pr_write_journal) marker: PrWriteMarker,
    pub(in crate::pr_write_journal) generation: i64,
    pub(crate) origin: MetadataMembersOriginRecordV48,
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
    pub(crate) fn metadata_members_history_v48(
        &self,
        target: &PrWriteTarget,
        parent: &PrResourceParentV1,
    ) -> Result<MetadataMembersHistoryV48> {
        let database = DatabaseV3::capture(&self.path)?;
        let generation = self.source_generation(target)?;
        if let Some(blocker) = self.blocking(target)? {
            let origin = self.metadata_members_origin_v48(target, &blocker)?;
            if !origin.seed.same_object(parent) {
                bail!("Other original purpose blocks member history");
            }
        }
        // Select newest actual rows before validating their known native
        // prefix/origin. A malformed/foreign indexed row cannot be filtered out
        // to resurrect an older apparently valid terminal.
        let ids = {
            let mut statement=self.connection.prepare(
            "SELECT a.attempt_id FROM pr_write_attempts AS a LEFT JOIN pr_write_metadata_origins_v25 AS o ON o.attempt_id=a.attempt_id WHERE a.host=?1 AND a.principal=?2 AND a.pr_node=?3 AND (a.selected_resource LIKE 'metadata-members-v48/%' OR o.base_key_v48 IS NOT NULL OR o.action_tag_v48 IS NOT NULL OR o.parent_ack_attempt_v48 IS NOT NULL) ORDER BY a.attempt_id DESC LIMIT 65")?;
            let ids = statement
                .query_map(
                    params![target.scope.host, target.scope.login, target.pr_node_id],
                    |row| row.get::<_, i64>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids
        };
        let complete = ids.len() <= 64;
        let mut rows = Vec::with_capacity(ids.len().min(64));
        let mut retained = rows.capacity() * std::mem::size_of::<MetadataMembersHistoryRowV48>();
        for id in ids.into_iter().take(64) {
            let bytes: usize = self.connection.query_row(
                &format!(
                    "SELECT {} FROM pr_write_attempts WHERE attempt_id=?1",
                    source_metadata::METADATA_BYTES_SQL
                ),
                [id],
                |row| row.get(0),
            )?;
            if bytes > 8192 {
                bail!("Member history row exceeds8KiB before copy");
            }
            let raw=self.connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],read_marker)?;
            let marker = decode_marker(target, raw)?;
            let origin = self.metadata_members_origin_v48(target, &marker)?;
            if !origin.seed.same_object(parent) {
                bail!("Member history actual parent differs");
            }
            retained = retained
                .checked_add(bytes)
                .and_then(|n| n.checked_add(bounded_encode(&origin).ok()?.len()))
                .filter(|n| *n <= 1024 * 1024)
                .context("Member history metadata exceeds1MiB")?;
            let allowance = (1024 * 1024usize)
                .checked_sub(retained)
                .context("Member history full")?
                .min(128 * 1024);
            let body = Cache::open(&self.path)?
                .bounded_draft(target.scope(), &marker.selected_resource, allowance)?
                .map(|encoded| -> Result<_> {
                    #[derive(Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Envelope {
                        version: u8,
                        values: BTreeMap<String, String>,
                        publication: Option<PrPublicationReference>,
                    }
                    let parsed: Envelope = serde_json::from_str(&encoded)?;
                    if parsed.version != 17
                        || parsed.publication.as_ref() != Some(&marker.publication_reference())
                        || origin.seed.write(&parsed.values)?.digest()? != marker.payload_sha256
                    {
                        bail!("Protected original member body/ref differs; never restore");
                    }
                    Ok((encoded, parsed.values))
                })
                .transpose()?;
            if let Some((encoded, values)) = &body {
                retained = retained
                    .checked_add(encoded.capacity())
                    .and_then(|n| {
                        n.checked_add(values.values().map(String::capacity).sum::<usize>())
                    })
                    .filter(|n| *n <= 1024 * 1024)
                    .context("Member history protected envelope exceeds1MiB")?;
            }
            rows.push(MetadataMembersHistoryRowV48 {
                marker,
                origin,
                body,
            });
        }
        if database != DatabaseV3::capture(&self.path)?
            || generation != self.source_generation(target)?
        {
            bail!("Member history original DB/generation changed");
        }
        Ok(MetadataMembersHistoryV48 {
            database,
            generation,
            complete,
            rows,
        })
    }
    pub(crate) fn validate_metadata_members_history_v48(
        &self,
        target: &PrWriteTarget,
        parent: &PrResourceParentV1,
        expected: &MetadataMembersHistoryV48,
    ) -> Result<()> {
        let actual = self.metadata_members_history_v48(target, parent)?;
        if expected.database != actual.database
            || expected.generation != actual.generation
            || expected.complete != actual.complete
            || expected.rows.len() != actual.rows.len()
        {
            bail!("Exact member history generation/window changed");
        }
        for (a, b) in expected.rows.iter().zip(&actual.rows) {
            if !same_marker(&a.marker, &b.marker)
                || bounded_encode(&a.origin)? != bounded_encode(&b.origin)?
                || a.body != b.body
            {
                bail!("Exact original member row/body/ref changed");
            }
        }
        Ok(())
    }
    pub(crate) fn prepare_metadata_members_inspection_v48(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
    ) -> Result<MetadataMembersRecoveryWitnessV48> {
        let marker = self
            .original_publication(target, reference)?
            .context("Exact member attempt absent")?;
        let blocker = self
            .blocking(target)?
            .context("No unresolved member blocker")?;
        let origin = self.metadata_members_origin_v48(target, &marker)?;
        if !same_marker(&marker, &blocker)
            || !matches!(
                marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            )
        {
            bail!("Only exact original member blocker can be acknowledged");
        }
        let mut witness = MetadataMembersRecoveryWitnessV48 {
            target: target.clone(),
            marker,
            generation: self.source_generation(target)?,
            origin,
            exact: None,
        };
        witness.exact = Some(exact_ack_v17::ExactAckRowV17::metadata_members_v48(
            self, &witness,
        )?);
        self.validate_metadata_members_inspection_v48(&witness)?;
        Ok(witness)
    }
    pub(crate) fn validate_metadata_members_inspection_v48(
        &self,
        witness: &MetadataMembersRecoveryWitnessV48,
    ) -> Result<()> {
        let marker = self
            .original_publication(&witness.target, &witness.marker.publication_reference())?
            .context("Original member attempt absent")?;
        let blocker = self
            .blocking(&witness.target)?
            .context("Member no longer blocks")?;
        let origin = self.metadata_members_origin_v48(&witness.target, &marker)?;
        if self.source_generation(&witness.target)? != witness.generation
            || !same_marker(&marker, &witness.marker)
            || !same_marker(&blocker, &witness.marker)
            || bounded_encode(&origin)? != bounded_encode(&witness.origin)?
        {
            bail!("Exact member inspection changed; no release");
        }
        if let Some(exact) = &witness.exact {
            exact.validate_current(self)?;
        }
        Ok(())
    }
    pub(crate) fn acknowledge_metadata_members_inspection_v48(
        &mut self,
        provider: &GithubProvider,
        mut witness: MetadataMembersRecoveryWitnessV48,
        observation: MetadataMembersRecoveryCurrentV48,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<usize> {
        if cancel.is_cancelled()
            || Instant::now() >= deadline.min(observation.deadline())
            || !observation.matches_target(&witness.target)
            || observation.actual().command() != witness.origin.seed.command()?
        {
            bail!("Actual member ACK purpose/target/clock differs");
        }
        let deadline = deadline.min(observation.deadline());
        let row = witness.exact.take().context("Exact member ACK consumed")?;
        row.validate_current(self)?;
        provider
            .reconcile_metadata_members_inspection_v48(observation, cancel, deadline)
            .map_err(anyhow::Error::msg)?;
        // Same sole transition: runner reconciliation does NOT release durable
        // uncertainty if this exact raw SQL CAS later refuses.
        self.commit_exact_ack_v17(row, cancel, deadline)
    }
}
