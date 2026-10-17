//! Bounded saved observations and exact local ACK of a manual Issue attempt.
//! A history row never supplies a Current, picker choice, live seed or permit.
use super::*;
use crate::providers::gh::PrResourceParentV1;

#[cfg(test)]
#[path = "pr_write_issue_cross_family_v106_tests.rs"]
pub(in crate::pr_write_journal) mod cross_family_v106;

pub(crate) struct IssueLinkHistoryRowV87 {
    pub(crate) marker: PrWriteMarker,
    pub(crate) origin: IssueLinkOriginRecordV87,
    pub(crate) body: Option<(String, BTreeMap<String, String>)>,
}
pub(crate) struct IssueLinkHistoryV87 {
    database: DatabaseV3,
    pub(crate) generation: i64,
    pub(crate) complete: bool,
    pub(crate) command: IssueLinkCommandV87,
    pub(crate) rows: Vec<IssueLinkHistoryRowV87>,
    retained: usize,
    inherited: usize,
}
impl IssueLinkHistoryV87 {
    pub(crate) fn retained_bound_v87(&self) -> usize {
        self.retained
    }
}
/// Issued by exact journal validation, never deserialized. Its selected Issue
/// facts may constrain a fresh read-only inspection, not authorize a write.
pub(crate) struct IssueLinkRecoveryWitnessV87 {
    pub(in crate::pr_write_journal) target: PrWriteTarget,
    pub(in crate::pr_write_journal) marker: PrWriteMarker,
    pub(in crate::pr_write_journal) generation: i64,
    pub(crate) origin: IssueLinkOriginRecordV87,
    exact: Option<exact_ack_v17::ExactAckRowV17>,
}
impl IssueLinkRecoveryWitnessV87 {
    pub(crate) fn command(&self) -> Result<IssueLinkCommandV87> {
        command(&self.origin.seed.command)
    }
    pub(crate) fn selected_issue_v87(&self) -> (&str, u64, &str, &str, u64, u64) {
        let s = &self.origin.seed;
        (
            &s.issue_repository,
            s.issue_repository_id,
            &s.issue_repository_node,
            &s.issue_node,
            s.issue_id,
            s.issue_number,
        )
    }
    pub(crate) fn matches_parent_v87(
        &self,
        parent: &PrResourceParentV1,
        identity: &crate::providers::gh::HostIdentity,
    ) -> bool {
        self.origin.seed.same_object(parent)
            && Scope::github(&identity.host, identity.account_id)
                .is_ok_and(|s| s == self.origin.seed.scope)
    }
    pub(crate) fn reference(&self) -> &PrPublicationReference {
        &self.origin.reference
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
    pub(crate) fn acknowledge_issue_link_inspection_v87(
        &mut self,
        provider: &GithubProvider,
        mut witness: IssueLinkRecoveryWitnessV87,
        observed: crate::providers::gh::IssueLinkRecoveryCurrentV87,
        cancel: &CancellationToken,
        deadline: std::time::Instant,
    ) -> Result<usize> {
        let deadline = deadline.min(observed.deadline());
        if cancel.is_cancelled()
            || std::time::Instant::now() >= deadline
            || !observed.matches_witness_v87(&witness)
        {
            bail!("Exact current Issue inspection/clock differs; blocker retained");
        }
        let row = witness
            .exact
            .take()
            .context("Issue raw ACK already consumed")?;
        row.validate_current(self)?;
        provider
            .reconcile_issue_link_inspection_v87(observed, cancel, deadline)
            .map_err(anyhow::Error::msg)?;
        // Both in-memory versions reconcile once before this SAME raw CAS.
        // A failed SQL CAS retains the durable secondary/PR blocker; it does
        // not restore this consumed inspection or authorize automatic resend.
        self.commit_exact_ack_v17(row, cancel, deadline)
    }
    #[cfg(test)]
    pub(crate) fn issue_link_history_v87(
        &self,
        target: &PrWriteTarget,
        parent: &PrResourceParentV1,
        command: IssueLinkCommandV87,
    ) -> Result<IssueLinkHistoryV87> {
        self.issue_link_history_with_overlap_v87(
            target,
            parent,
            command,
            parent.retained_heap_bytes_v2(),
        )
    }
    pub(crate) fn issue_link_history_with_overlap_v87(
        &self,
        target: &PrWriteTarget,
        parent: &PrResourceParentV1,
        command: IssueLinkCommandV87,
        inherited: usize,
    ) -> Result<IssueLinkHistoryV87> {
        // This entire bounded history can occupy at most1MiB, including its
        // fixed temporary encoding overlap. Reserve room beside the actual
        // caller's still-live models BEFORE the first row/String is copied.
        inherited
            .checked_add(1024 * 1024)
            .filter(|n| *n <= 4 * 1024 * 1024)
            .context("Actual Issue history models leave no fixed local-copy overlap")?;
        let database = DatabaseV3::capture(&self.path)?;
        let generation = self.source_generation(target)?;
        if let Some(blocker) = self.blocking(target)? {
            let origin = self.issue_link_origin_v87(target, &blocker)?;
            if origin.seed.command != command.command() || !origin.seed.same_object(parent) {
                bail!("Different exact purpose/current object blocks Issue history");
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
                        command.command()
                    ],
                    |row| row.get::<_, i64>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            ids
        };
        // Eight KiB raw attempt +16KiB original +128KiB envelope and its
        // decoded scalar copies are charged BEFORE each protected body read.
        // No more than64 entries/1MiB of retained metadata/body are copied.
        let complete = ids.len() <= 64;
        let mut retained = std::mem::size_of::<IssueLinkHistoryV87>()
            + ids.capacity() * std::mem::size_of::<i64>();
        let mut rows = Vec::new();
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
                bail!("Issue history attempt exceeds8KiB before copy");
            }
            let raw=self.connection.query_row("SELECT attempt_id,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?1",[id],read_marker)?;
            let marker = decode_marker(target, raw)?;
            let origin = self.issue_link_origin_v87(target, &marker)?;
            if origin.seed.command != command.command() || !origin.seed.same_object(parent) {
                bail!("Actual current PR differs from original Issue history");
            }
            retained = retained
                .checked_add(2 * bytes)
                .and_then(|n| n.checked_add(4 * bounded_encode(&origin).ok()?.len()))
                .and_then(|n| n.checked_add(2 * std::mem::size_of::<IssueLinkHistoryRowV87>()))
                .filter(|n| *n <= 1024 * 1024)
                .context("Issue history retained overlap exceeds1MiB")?;
            let available = (1024 * 1024usize)
                .checked_sub(retained)
                .context("Issue history capacity ended")?
                / 4;
            let saved = Cache::open(&self.path)?.bounded_draft(
                target.scope(),
                &marker.selected_resource,
                available.min(128 * 1024),
            )?;
            let body = saved.map(|encoded| {
                // This new purpose has ONE canonical version17 serializer and
                // a fixed seven-scalar selected identity. Re-encode that known
                // actual original instead of parsing arbitrary saved maps/keys
                // before a tree bound. Noncanonical/tampered saved bytes refuse.
                let values=origin.seed.values()?;
                let expected=crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(&values,Some(&marker.publication_reference()))
                    .map_err(anyhow::Error::msg)?;
                if encoded!=expected || origin.seed.write(&values)?.digest()?!=marker.payload_sha256 {
                    bail!("Protected Issue canonical envelope/ref/fingerprint differs; no restoration");
                }
                Ok::<_,anyhow::Error>((encoded,values))
            }).transpose()?;
            if let Some((encoded, _)) = &body {
                retained = retained
                    .checked_add(4 * encoded.capacity())
                    .filter(|n| *n <= 1024 * 1024)
                    .context("Issue history protected-body overlap exceeds1MiB")?;
            }
            rows.push(IssueLinkHistoryRowV87 {
                marker,
                origin,
                body,
            });
        }
        if database != DatabaseV3::capture(&self.path)?
            || generation != self.source_generation(target)?
        {
            bail!("Issue history DB/generation changed during bounded read");
        }
        Ok(IssueLinkHistoryV87 {
            database,
            generation,
            complete,
            command,
            rows,
            retained,
            inherited,
        })
    }
    pub(crate) fn validate_issue_link_history_v87(
        &self,
        target: &PrWriteTarget,
        parent: &PrResourceParentV1,
        expected: &IssueLinkHistoryV87,
    ) -> Result<()> {
        let current = self.issue_link_history_with_overlap_v87(
            target,
            parent,
            expected.command,
            expected.inherited,
        )?;
        if current.database != expected.database
            || current.generation != expected.generation
            || current.complete != expected.complete
            || current.rows.len() != expected.rows.len()
        {
            bail!("Exact Issue history DB/rows/generation changed");
        }
        for (a, b) in current.rows.iter().zip(&expected.rows) {
            if !same_marker(&a.marker, &b.marker)
                || bounded_encode(&a.origin)? != bounded_encode(&b.origin)?
                || a.body != b.body
            {
                bail!("Exact Issue history original/body changed");
            }
        }
        Ok(())
    }
    pub(crate) fn prepare_issue_link_inspection_v87(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
    ) -> Result<IssueLinkRecoveryWitnessV87> {
        let marker = self
            .original_publication(target, reference)?
            .context("Exact Issue attempt absent")?;
        let blocker = self
            .blocking(target)?
            .context("No unresolved Issue blocker")?;
        if !same_marker(&marker, &blocker)
            || !matches!(
                marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            )
        {
            bail!("Only the exact unresolved Issue attempt may be inspected for local ACK");
        }
        let origin = self.issue_link_origin_v87(target, &marker)?;
        if selected_latest(&self.connection, &self.path, &origin.seed, None)?
            != Some(marker.attempt_id)
        {
            bail!("A newer selected-Issue attempt exists; no fallback");
        }
        let mut witness = IssueLinkRecoveryWitnessV87 {
            target: target.clone(),
            marker,
            generation: self.source_generation(target)?,
            origin,
            exact: None,
        };
        witness.exact = Some(exact_ack_v17::ExactAckRowV17::issue_link_v87(
            self, &witness,
        )?);
        self.validate_issue_link_inspection_v87(&witness)?;
        Ok(witness)
    }
    pub(crate) fn validate_issue_link_inspection_v87(
        &self,
        witness: &IssueLinkRecoveryWitnessV87,
    ) -> Result<()> {
        let marker = self
            .original_publication(&witness.target, &witness.marker.publication_reference())?
            .context("Issue blocker absent")?;
        let blocker = self
            .blocking(&witness.target)?
            .context("Issue attempt no longer blocks")?;
        let origin = self.issue_link_origin_v87(&witness.target, &marker)?;
        if self.source_generation(&witness.target)? != witness.generation
            || !same_marker(&marker, &witness.marker)
            || !same_marker(&blocker, &witness.marker)
            || bounded_encode(&origin)? != bounded_encode(&witness.origin)?
            || selected_latest(&self.connection, &self.path, &origin.seed, None)?
                != Some(marker.attempt_id)
        {
            bail!("Exact Issue inspection/raw/origin/secondary latest changed");
        }
        if let Some(exact) = &witness.exact {
            exact.validate_current(self)?;
        }
        Ok(())
    }
}
