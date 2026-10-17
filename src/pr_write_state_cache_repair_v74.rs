//! Revocation-only witness. Only a genuine current State phase plus its exact
//! saved original can issue it; it cannot ACK, invoke, hydrate or mint a source.
use super::*;
use std::{sync::Arc, time::Instant};
pub(crate) struct StateCacheRepairV74 {
    file: DatabaseV3,
    path: PathBuf,
    source: Arc<PrCollaborationSourceV17>,
    marker: PrWriteMarker,
    generation: i64,
    raw_origin: String,
    raw_receipt: Option<String>,
    index: (u64, u64, u64, String, String, Option<i64>),
    disposition: Option<(String, String)>,
    cancel: CancellationToken,
    cutoff: Instant,
}
pub(crate) struct StateRepairOriginalV74<'a> {
    pub(crate) source: &'a Arc<PrCollaborationSourceV17>,
    pub(crate) actual: &'a OriginalPrStateV68,
    pub(crate) seed: &'a StateOriginSeedV68,
    pub(crate) reference: &'a PrPublicationReference,
    pub(crate) local: &'a crate::scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
    pub(crate) generation: i64,
    pub(crate) cancel: &'a CancellationToken,
    pub(crate) deadline: Instant,
}
impl StateCacheRepairV74 {
    pub(crate) fn source(&self) -> &Arc<PrCollaborationSourceV17> {
        &self.source
    }
    pub(crate) fn target(&self) -> &PrWriteTarget {
        &self.marker.target
    }
    pub(crate) fn ordinal(&self) -> i64 {
        self.marker.attempt_id
    }
    pub(crate) fn repository_id(&self) -> u64 {
        self.index.0
    }
    pub(crate) fn pr_id(&self) -> u64 {
        self.index.1
    }
    pub(crate) fn database(&self) -> &Path {
        &self.path
    }
    pub(crate) fn cutoff(&self) -> Instant {
        self.cutoff
    }
    /// Restriction-only comparison against an ACTUAL retained local guard.
    /// Returning true issues no source/guard, receipt or SQL permission.
    pub(crate) fn follows_original_guard_v75(
        &self,
        target: &PrWriteTarget,
        generation: i64,
    ) -> bool {
        &self.marker.target == target && self.generation > generation
    }
    pub(crate) fn validate_current_raw_v75(&self) -> Result<()> {
        let journal = PrWriteJournal::open(&self.path)?;
        let tx = journal.connection.unchecked_transaction()?;
        self.validate_transaction(&tx)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn validate_file_clock(
        &self,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<()> {
        if !self.cancel.same_instance(cancel)
            || cancel.is_cancelled()
            || Instant::now() >= deadline.min(self.cutoff)
            || DatabaseV3::capture(&self.path)? != self.file
        {
            bail!("State repair original file/token/cutoff changed");
        }
        Ok(())
    }
    /// Fixed equality SQL only. All raw strings/types were captured BEFORE
    /// any read close; there is no body/JSON materialization in this CAS.
    pub(crate) fn validate_transaction(&self, tx: &Transaction<'_>) -> Result<()> {
        let m = &self.marker;
        let t = &m.target;
        if generation(tx, t)? != self.generation {
            bail!("State repair generation moved");
        }
        let exact:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_attempts a JOIN pr_write_state_origins_v68 s ON s.attempt_id=a.attempt_id WHERE a.attempt_id=?1 AND a.host=?2 AND a.principal=?3 AND a.pr_node=?4 AND a.repository=?5 AND a.pr_number=?6 AND a.command=?7 AND a.selected_resource=?8 AND a.observed_head=?9 AND a.payload_sha256=?10 AND a.state=?11 AND typeof(a.confirmed_json)=typeof(?12) AND CAST(a.confirmed_json AS BLOB) IS CAST(?12 AS BLOB) AND a.created_at=?13 AND typeof(s.repository_id)='integer' AND s.repository_id=?14 AND typeof(s.pr_id)='integer' AND s.pr_id=?15 AND typeof(s.issue_id)='integer' AND s.issue_id=?16 AND typeof(s.command)='text' AND CAST(s.command AS BLOB)=CAST(?7 AS BLOB) AND typeof(s.origin_json)='text' AND CAST(s.origin_json AS BLOB)=CAST(?17 AS BLOB) AND typeof(s.base_key_v71)='text' AND CAST(s.base_key_v71 AS BLOB)=CAST(?18 AS BLOB) AND typeof(s.action_tag_v71)='text' AND CAST(s.action_tag_v71 AS BLOB)=CAST(?19 AS BLOB) AND typeof(s.parent_ack_attempt_v71)=typeof(?20) AND s.parent_ack_attempt_v71 IS ?20)",params![m.attempt_id,t.scope.host,t.scope.login,t.pr_node_id,t.repository,t.number,m.command,m.selected_resource,m.observed_head,m.payload_sha256,state_name(m.state),self.raw_receipt,m.created_at,self.index.0,self.index.1,self.index.2,self.raw_origin,self.index.3,self.index.4,self.index.5],|r|r.get(0))?;
        let latest:Option<i64>=tx.query_row("SELECT MAX(attempt_id) FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3",params![t.scope.host,t.scope.login,t.pr_node_id],|r|r.get(0))?;
        let expected_blockers = usize::from(matches!(
            m.state,
            PrWriteState::Pending | PrWriteState::Uncertain
        ));
        let blockers:usize=tx.query_row("SELECT count(*) FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND state IN('pending','uncertain')",params![t.scope.host,t.scope.login,t.pr_node_id],|r|r.get(0))?;
        let disposition=match &self.disposition {
            None=>tx.query_row("SELECT count(*)=0 FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1",[m.attempt_id],|r|r.get::<_,bool>(0))?,
            Some((kind,raw))=>tx.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1 AND typeof(kind)='text' AND CAST(kind AS BLOB)=CAST(?2 AS BLOB) AND typeof(receipt_json)='text' AND CAST(receipt_json AS BLOB)=CAST(?3 AS BLOB))",params![m.attempt_id,kind,raw],|r|r.get(0))?,
        };
        let foreign:usize=tx.query_row("SELECT (SELECT count(*) FROM pr_write_conversation_origins_v17 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_origins_v1 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_comment_origins_v2 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_thread_reaction_origins_v3 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_file_comment_origins_v75 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1)+(SELECT count(*) FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1)",[m.attempt_id],|r|r.get(0))?;
        if !exact
            || !disposition
            || foreign != 0
            || latest != Some(m.attempt_id)
            || blockers != expected_blockers
        {
            bail!("Exact State repair raw terminal/origin/index/disposition/blocker changed");
        }
        Ok(())
    }
}
impl PrWriteJournal {
    pub(crate) fn prepare_original_state_cache_repair_v74(
        &self,
        request: StateRepairOriginalV74<'_>,
    ) -> Result<StateCacheRepairV74> {
        let StateRepairOriginalV74 {
            source,
            actual,
            seed,
            reference,
            local,
            generation: expected_generation,
            cancel,
            deadline,
        } = request;
        actual
            .validate_repair_context_v74(cancel, deadline)
            .map_err(anyhow::Error::msg)?;
        if source.opening().database() != self.path
            || source.current().is_err()
            || !seed.0.matches_actual_v68(actual)
            || seed.0.database != DatabaseV3::capture(&self.path)?
            || source.opening().scope() != &seed.0.scope
            || !source.opening().admits_state_v68(seed.0.command)
        {
            bail!("State repair requires this genuine current producer/original seed");
        }
        let target = PrWriteTarget::new(actual.identity(), actual.observed().parent().summary())?;
        let attempt_bytes:usize=self.connection.query_row("SELECT length(CAST(host AS BLOB))+length(CAST(principal AS BLOB))+length(CAST(pr_node AS BLOB))+length(CAST(repository AS BLOB))+length(CAST(command AS BLOB))+length(CAST(selected_resource AS BLOB))+length(CAST(observed_head AS BLOB))+length(CAST(payload_sha256 AS BLOB))+length(CAST(state AS BLOB))+COALESCE(length(CAST(confirmed_json AS BLOB)),0)+24 FROM pr_write_attempts WHERE attempt_id=?1",[reference.attempt_id],|r|r.get(0))?;
        if attempt_bytes > 8192 {
            bail!("State repair attempt exceeds8KiB before marker copy");
        }
        let marker = self
            .original_publication(&target, reference)?
            .context("State repair saved marker missing")?;
        let record = self.state_origin_v68(&target, &marker)?;
        if bounded_encode(&record.seed)? != bounded_encode(&seed.0)? {
            bail!("State repair original seed/known reference mismatch");
        }
        let tx = self.connection.unchecked_transaction()?;
        let valid:bool=tx.query_row("SELECT typeof(host)='text' AND typeof(principal)='text' AND typeof(pr_node)='text' AND typeof(repository)='text' AND typeof(pr_number)='integer' AND typeof(command)='text' AND typeof(selected_resource)='text' AND typeof(observed_head)='text' AND typeof(payload_sha256)='text' AND typeof(state)='text' AND typeof(created_at)='integer' AND (typeof(confirmed_json)='null' OR typeof(confirmed_json)='text') FROM pr_write_attempts WHERE attempt_id=?1",[marker.attempt_id],|r|r.get(0))?;
        // The typed original reader above already preflights origin16KiB and
        // disposition8KiB. Repeat the raw attempt8KiB bound before its receipt.
        let attempt_bytes:usize=tx.query_row("SELECT length(CAST(host AS BLOB))+length(CAST(principal AS BLOB))+length(CAST(pr_node AS BLOB))+length(CAST(repository AS BLOB))+length(CAST(command AS BLOB))+length(CAST(selected_resource AS BLOB))+length(CAST(observed_head AS BLOB))+length(CAST(payload_sha256 AS BLOB))+length(CAST(state AS BLOB))+COALESCE(length(CAST(confirmed_json AS BLOB)),0)+24 FROM pr_write_attempts WHERE attempt_id=?1",[marker.attempt_id],|r|r.get(0))?;
        if !valid || attempt_bytes > 8192 {
            bail!("State repair scalar pre-copy bound/type exceeded");
        }
        let raw_origin: String = tx.query_row(
            "SELECT origin_json FROM pr_write_state_origins_v68 WHERE attempt_id=?1",
            [marker.attempt_id],
            |r| r.get(0),
        )?;
        let raw_receipt: Option<String> = tx.query_row(
            "SELECT confirmed_json FROM pr_write_attempts WHERE attempt_id=?1",
            [marker.attempt_id],
            |r| r.get(0),
        )?;
        let disposition = tx
            .query_row(
                "SELECT kind,receipt_json FROM pr_write_state_dispositions_v68 WHERE attempt_id=?1",
                [marker.attempt_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let captured = StateCacheRepairV74 {
            file: DatabaseV3::capture(&self.path)?,
            path: self.path.clone(),
            source: Arc::clone(source),
            generation: expected_generation,
            index: (
                record.seed.repository_id,
                record.seed.pr_id,
                record.seed.issue_id,
                record.seed.stable_base_v68()?,
                record.seed.command.key().into(),
                record.seed.lineage_v68.as_ref().and_then(|l| l.parent_ack),
            ),
            marker,
            raw_origin,
            raw_receipt,
            disposition,
            cancel: cancel.clone(),
            cutoff: deadline.min(actual.publication_cutoff_v68()),
        };
        captured.validate_transaction(&tx)?;
        tx.commit()?;
        local
            .validate_state_cache_repair_boundary_v75(&captured)
            .map_err(anyhow::Error::msg)?;
        captured.validate_file_clock(cancel, deadline)?;
        Ok(captured)
    }
}

/// Private piece of the SAME typed mark_state_confirmed transaction. Repair
/// itself cannot issue this transition or replace any nonNULL disposition.
pub(super) struct StateConfirmedFenceAdvanceV74 {
    scope: Scope,
    node: String,
    repository: String,
    repository_id: u64,
    pr_id: u64,
    ordinal: i64,
    origin: String,
    new_disposition: String,
    predecessor: Option<StateFencePredecessorV74>,
}
struct StateFencePredecessorV74 {
    repository_id: u64,
    pr_id: u64,
    repository: String,
    ordinal: i64,
    origin: String,
    disposition: Option<String>,
    status: String,
}
impl StateConfirmedFenceAdvanceV74 {
    pub(super) fn prepare(
        connection: &Connection,
        seed: &StateOriginSeedV68,
        receipt: &crate::providers::gh::StateReceiptV68,
        raw_disposition: &str,
    ) -> Result<Self> {
        if !seed.matches_receipt_v68(receipt)? {
            bail!("Only exact actual typed State receipt may prepare terminal advancement");
        }
        let ordinal = receipt.reference().attempt_id;
        let size:Option<usize>=connection.query_row("SELECT length(CAST(repository AS BLOB))+length(CAST(raw_origin_sha256 AS BLOB))+COALESCE(length(CAST(raw_disposition_sha256 AS BLOB)),0)+length(CAST(status AS BLOB))+32 FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2 AND pr_node=?3",params![seed.0.scope.host,seed.0.scope.login,seed.0.pr_node],|r|r.get(0)).optional()?;
        if size.is_some_and(|n| n > 4096) {
            bail!("State terminal predecessor exceeds4KiB before copy");
        }
        let predecessor=connection.query_row("SELECT repository_id,pr_id,repository,attempt_id,raw_origin_sha256,raw_disposition_sha256,status FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2 AND pr_node=?3 AND typeof(repository_id)='integer' AND typeof(pr_id)='integer' AND typeof(repository)='text' AND typeof(attempt_id)='integer' AND typeof(raw_origin_sha256)='text' AND (typeof(raw_disposition_sha256)='null' OR typeof(raw_disposition_sha256)='text') AND typeof(status)='text'",params![seed.0.scope.host,seed.0.scope.login,seed.0.pr_node],|r|Ok(StateFencePredecessorV74 {repository_id:r.get(0)?,pr_id:r.get(1)?,repository:r.get(2)?,ordinal:r.get(3)?,origin:r.get(4)?,disposition:r.get(5)?,status:r.get(6)?})).optional()?;
        if size.is_some() != predecessor.is_some() {
            bail!("State predecessor raw typeof differs");
        }
        let value = Self {
            scope: seed.0.scope.clone(),
            node: seed.0.pr_node.clone(),
            repository: seed.0.repository.to_ascii_lowercase(),
            repository_id: seed.0.repository_id,
            pr_id: seed.0.pr_id,
            ordinal,
            origin: crate::storage::state_origin_fingerprint_v74(connection, ordinal)?,
            new_disposition: crate::storage::state_disposition_fingerprint_v74(
                ordinal,
                raw_disposition,
            )?,
            predecessor,
        };
        if let Some(old) = &value.predecessor {
            if old.repository_id != value.repository_id
                || old.pr_id != value.pr_id
                || old.repository != value.repository
                || old.ordinal > ordinal
                || old.ordinal == ordinal
                    && (old.origin != value.origin
                        || old.disposition.is_some()
                        || old.status != "unavailable")
            {
                bail!("Only exact same-ordinal unresolved State predecessor can advance to actual Confirmed");
            }
        }
        Ok(value)
    }
    pub(super) fn apply(&self, tx: &Transaction<'_>) -> Result<()> {
        let exact=match &self.predecessor {
            None=>tx.query_row("SELECT count(*)=0 FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2 AND pr_node=?3",params![self.scope.host,self.scope.login,self.node],|r|r.get::<_,bool>(0))?,
            Some(old)=>tx.query_row("SELECT EXISTS(SELECT 1 FROM pr_write_state_filter_fences_v68 WHERE host=?1 AND principal=?2 AND pr_node=?3 AND typeof(repository_id)='integer' AND repository_id=?4 AND typeof(pr_id)='integer' AND pr_id=?5 AND typeof(repository)='text' AND CAST(repository AS BLOB)=CAST(?6 AS BLOB) AND typeof(attempt_id)='integer' AND attempt_id=?7 AND typeof(raw_origin_sha256)='text' AND CAST(raw_origin_sha256 AS BLOB)=CAST(?8 AS BLOB) AND typeof(raw_disposition_sha256)=typeof(?9) AND CAST(raw_disposition_sha256 AS BLOB) IS CAST(?9 AS BLOB) AND typeof(status)='text' AND CAST(status AS BLOB)=CAST(?10 AS BLOB))",params![self.scope.host,self.scope.login,self.node,old.repository_id,old.pr_id,old.repository,old.ordinal,old.origin,old.disposition,old.status],|r|r.get(0))?,
        };
        if !exact {
            bail!("State raw predecessor changed during terminal lock wait");
        }
        if self
            .predecessor
            .as_ref()
            .is_some_and(|row| row.ordinal == self.ordinal)
        {
            let changed=tx.execute("UPDATE pr_write_state_filter_fences_v68 SET raw_disposition_sha256=?1 WHERE host=?2 AND principal=?3 AND pr_node=?4 AND attempt_id=?5 AND typeof(raw_disposition_sha256)='null' AND status='unavailable' AND NOT EXISTS(SELECT 1 FROM pr_write_state_source_watermarks_v73 w WHERE w.host=?2 AND w.principal=?3 AND w.pr_node=?4 AND w.mutation_ordinal=?5)",params![self.new_disposition,self.scope.host,self.scope.login,self.node,self.ordinal])?;
            if changed != 1 {
                bail!("State same-ordinal terminal cannot replace hydrated/grafted metadata");
            }
        }
        Ok(())
    }
}
