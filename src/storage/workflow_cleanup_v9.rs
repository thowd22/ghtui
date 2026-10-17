//! Prepared, restriction-only W9 cleanup predicates. No source or write permit.
use super::*;
use crate::{
    actions_write_journal::ActionsOwnedWorkflowRemoteHistoryV9,
    drafts::{AcknowledgedWorkflowCleanupV9, WorkflowCleanupBindingV9, WorkflowCleanupFailureV9},
    providers::{
        gh::{WorkflowCleanupSealV9, WorkflowJsonChargeV9, WorkflowJsonLedgerV9},
        runner::CancellationToken,
    },
    scoped_actions_workflow_edit_v9::{OpenedWorkflowLocalEditV9, WorkflowRemoteConfirmV9},
};
use anyhow::{bail, Context};
use rusqlite::{params_from_iter, types::Value as SqlValue, Transaction, TransactionBehavior};
use std::{
    os::unix::fs::MetadataExt,
    sync::Arc,
    time::{Duration, Instant},
};

// Only this module's fixed table/column profiles and the two actual original
// private source adapters below construct these predicates. Never a callback.
pub(crate) struct WorkflowRawRowsV9 {
    count_sql: String,
    count_values: Vec<SqlValue>,
    row_sql: String,
    rows: Vec<Vec<SqlValue>>,
    _retained_charge: WorkflowJsonChargeV9,
}
impl WorkflowRawRowsV9 {
    #[cfg(test)]
    pub(crate) fn test_actual_mapping_v9(
        cache: &Cache,
        mapping: &super::RemoteMappingCandidateV1,
        heap: &WorkflowJsonLedgerV9,
    ) -> anyhow::Result<Self> {
        let tx = cache.connection.unchecked_transaction()?;
        let row = mapping.prepare_cleanup_v9(&tx, heap)?;
        tx.commit()?;
        Ok(row)
    }
    #[cfg(test)]
    pub(crate) fn test_actual_source_v9(
        cache: &Cache,
        source: &super::CatalogueIntentSource,
        epoch: bool,
        heap: &WorkflowJsonLedgerV9,
    ) -> anyhow::Result<Self> {
        source.validate_cleanup_issuer_v9(cache)?;
        let tx = cache.connection.unchecked_transaction()?;
        source.validate_cleanup_snapshot_v9(&tx)?;
        let scope = source.scope();
        let row = if epoch {
            Self::capture(
                &tx,
                "sync_reservations",
                "host,login,source,generation,active",
                "host=?1 AND login=?2 AND source=?3",
                vec![
                    scope.host.clone().into(),
                    scope.login.clone().into(),
                    source.sync_source().to_owned().into(),
                ],
                heap,
            )?
        } else {
            Self::mapping(&tx, scope, source.saved_view_key(), heap)?
        };
        tx.commit()?;
        Ok(row)
    }
    #[cfg(test)]
    pub(crate) fn test_validate_v9(&self, cache: &Cache) -> anyhow::Result<()> {
        let tx = cache.connection.unchecked_transaction()?;
        self.validate(&tx)?;
        tx.commit()?;
        Ok(())
    }
    fn capture(
        tx: &Transaction<'_>,
        table: &str,
        fields: &str,
        predicate: &str,
        values: Vec<SqlValue>,
        heap: &WorkflowJsonLedgerV9,
    ) -> anyhow::Result<Self> {
        // Header/SQL/column scratch is reserved before the first formatting or
        // collection allocation. Payload+row containers are separately charged
        // from the SAME coherent scalar preflight before copying any row.
        let mut charge = heap.reserve_model_v9(32 * 1024)?;
        let columns = fields.split(',').collect::<Vec<_>>();
        let sizes = columns
            .iter()
            .map(|s| format!("COALESCE(length(CAST({s} AS BLOB)),0)"))
            .collect::<Vec<_>>()
            .join("+");
        let count_sql = format!("SELECT count(*) FROM {table} WHERE {predicate}");
        let (count, bytes): (usize, usize) = tx.query_row(
            &format!("SELECT count(*),COALESCE(SUM({sizes}),0) FROM {table} WHERE {predicate}"),
            params_from_iter(&values),
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if count > 10_000 || bytes > 8 * 1024 * 1024 {
            bail!("W9 cleanup metadata exceeds original bounded projection");
        }
        let bound = bytes
            .checked_mul(2)
            .and_then(|n| {
                n.checked_add(count.checked_mul(
                    std::mem::size_of::<Vec<SqlValue>>()
                        + columns.len() * std::mem::size_of::<SqlValue>(),
                )?)
            })
            .and_then(|n| n.checked_add(32 * 1024))
            .context("W9 cleanup capacity overflow")?;
        // A second debit is taken BEFORE rows/strings grow, not after decoding.
        let row_charge = heap.reserve_model_v9(bound)?;
        let mut rows = Vec::with_capacity(count);
        {
            let mut statement =
                tx.prepare(&format!("SELECT {fields} FROM {table} WHERE {predicate}"))?;
            let mut cursor = statement.query(params_from_iter(&values))?;
            while let Some(row) = cursor.next()? {
                if rows.len() == count {
                    bail!("W9 cleanup row count changed within snapshot");
                }
                let mut data = Vec::with_capacity(columns.len());
                for i in 0..columns.len() {
                    data.push(row.get::<_, SqlValue>(i)?);
                }
                rows.push(data);
            }
        }
        if rows.len() != count {
            bail!("W9 cleanup snapshot is incomplete");
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
        let row_sql = format!("SELECT count(*) FROM {table} WHERE {comparisons}");
        // SQL construction temporaries have ended. Actual copied capacities
        // remain covered by row_charge; release only the scratch reservation.
        charge.retain_actual_v9(0)?;
        Ok(Self {
            count_sql,
            count_values: values,
            row_sql,
            rows,
            _retained_charge: row_charge,
        })
    }
    fn validate(&self, tx: &Transaction<'_>) -> anyhow::Result<()> {
        let count: usize =
            tx.query_row(&self.count_sql, params_from_iter(&self.count_values), |r| {
                r.get(0)
            })?;
        if count != self.rows.len() {
            bail!("W9 cleanup original keyspace changed");
        }
        for row in &self.rows {
            let matched: usize =
                tx.query_row(&self.row_sql, params_from_iter(row), |r| r.get(0))?;
            if matched != 1 {
                bail!("W9 cleanup exact raw row changed");
            }
        }
        Ok(())
    }
    pub(in crate::storage) fn mapping(
        tx: &Transaction<'_>,
        scope: &Scope,
        key: &str,
        heap: &WorkflowJsonLedgerV9,
    ) -> anyhow::Result<Self> {
        Self::capture(
            tx,
            "saved_views",
            "host,login,name,payload",
            "host=?1 AND login=?2 AND name=?3",
            vec![
                scope.host.clone().into(),
                scope.login.clone().into(),
                key.to_owned().into(),
            ],
            heap,
        )
    }
    pub(crate) fn actual_attempt_v9(
        tx: &Transaction<'_>,
        scope: &Scope,
        id: i64,
        heap: &WorkflowJsonLedgerV9,
    ) -> anyhow::Result<Self> {
        Self::capture(tx,"actions_write_attempts","attempt_id,host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,claimed,receipt_json,created_at","host=?1 AND principal=?2 AND attempt_id=?3",vec![scope.host.clone().into(),scope.login.clone().into(),id.into()],heap)
    }
    pub(crate) fn actual_alias_v9(
        tx: &Transaction<'_>,
        scope: &Scope,
        key: &str,
        heap: &WorkflowJsonLedgerV9,
    ) -> anyhow::Result<[Self; 2]> {
        let values = || {
            vec![
                scope.host.clone().into(),
                scope.login.clone().into(),
                key.to_owned().into(),
            ]
        };
        Ok([
            Self::capture(
                tx,
                "actions_write_resources",
                "host,principal,resource_key,attempt_id",
                "host=?1 AND principal=?2 AND resource_key=?3",
                values(),
                heap,
            )?,
            Self::capture(
                tx,
                "actions_write_generations",
                "host,principal,resource_key,generation",
                "host=?1 AND principal=?2 AND resource_key=?3",
                values(),
                heap,
            )?,
        ])
    }
    pub(crate) fn actual_pr_v9(
        tx: &Transaction<'_>,
        target: &crate::pr_write_journal::PrWriteTarget,
        heap: &WorkflowJsonLedgerV9,
    ) -> anyhow::Result<Vec<Self>> {
        let scope = target.scope();
        let pair = || vec![scope.host.clone().into(), scope.login.clone().into()];
        let mut rows = Vec::with_capacity(6);
        for (table, fields) in [
            (
                "checkpoints",
                "host,login,source,generation,fetched_at,complete",
            ),
            ("sync_reservations", "host,login,source,generation,active"),
        ] {
            rows.push(Self::capture(
                tx,
                table,
                fields,
                "host=?1 AND login=?2",
                pair(),
                heap,
            )?);
        }
        for (table, fields) in [
            (
                "records",
                "host,login,kind,id,repository,fetched_at,complete",
            ),
            (
                "sync_binding_lookup_epochs",
                "host,login,namespace,repository,generation",
            ),
        ] {
            let mut values = pair();
            values.push(target.repository().to_owned().into());
            rows.push(Self::capture(
                tx,
                table,
                fields,
                "host=?1 AND login=?2 AND repository=?3 COLLATE NOCASE",
                values,
                heap,
            )?);
        }
        let values = || {
            vec![
                scope.host.clone().into(),
                scope.login.clone().into(),
                target.node_id().to_owned().into(),
            ]
        };
        rows.push(Self::capture(
            tx,
            "pr_write_scopes",
            "host,principal,pr_node,generation",
            "host=?1 AND principal=?2 AND pr_node=?3",
            values(),
            heap,
        )?);
        rows.push(Self::capture(tx,"pr_write_attempts","attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at","host=?1 AND principal=?2 AND pr_node=?3 AND state IN('pending','uncertain')",values(),heap)?);
        Ok(rows)
    }
}

pub(crate) struct PreparedWorkflowCleanupV9 {
    binding: WorkflowCleanupBindingV9,
    opened: Arc<OpenedWorkflowLocalEditV9>,
    saved_file: Arc<crate::git::workflow_edit_v9::WorkflowWorkingFileV9>,
    history: Arc<ActionsOwnedWorkflowRemoteHistoryV9>,
    ticket: Arc<()>,
    rows: Vec<WorkflowRawRowsV9>,
    file: (u64, u64),
    updated_at: i64,
    effective: Instant,
    cancel: CancellationToken,
    #[cfg(test)]
    test_heap: Arc<WorkflowJsonLedgerV9>,
    _retained_charge: WorkflowJsonChargeV9,
}
pub(crate) struct CommittedWorkflowCleanupV9 {
    ticket: Arc<()>,
    history: Arc<ActionsOwnedWorkflowRemoteHistoryV9>,
}
impl CommittedWorkflowCleanupV9 {
    pub(crate) fn matches_v9(
        &self,
        history: &Arc<ActionsOwnedWorkflowRemoteHistoryV9>,
        ticket: &std::sync::Weak<()>,
    ) -> bool {
        Arc::ptr_eq(history, &self.history) && ticket.ptr_eq(&Arc::downgrade(&self.ticket))
    }
}
fn identity(path: &Path) -> anyhow::Result<(u64, u64)> {
    let m = std::fs::symlink_metadata(path)?;
    // SAFETY: immutable effective uid query, Linux/macOS only.
    if !m.is_file()
        || m.nlink() != 1
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
    {
        bail!("W9 cleanup private database identity unavailable");
    }
    Ok((m.dev(), m.ino()))
}
impl Cache {
    pub(crate) fn prepare_workflow_cleanup_v9(
        &self,
        request: &WorkflowRemoteConfirmV9,
        history: &Arc<ActionsOwnedWorkflowRemoteHistoryV9>,
        binding: WorkflowCleanupBindingV9,
        heap: &Arc<WorkflowJsonLedgerV9>,
    ) -> Result<PreparedWorkflowCleanupV9, WorkflowCleanupFailureV9> {
        let cancel = request.cancellation();
        let effective = binding.effective_v9();
        let prepare = || -> anyhow::Result<_> {
            // Binding owns its own debit. This covers NEW envelope/row Vec /
            // fixed SQL-query temporaries only, not H or binding accumulated a
            // second time into later results.
            let charge = heap
                .reserve_model_v9(std::mem::size_of::<PreparedWorkflowCleanupV9>() + 64 * 1024)?;
            binding.check_request_v9(request, history, cancel, effective)?;
            let opened = request
                .preview()
                .selection()
                .committed()
                .staged()
                .saved()
                .opened();
            let path = self.binding_lookup_issuer()?;
            if path != binding.database_v9() {
                bail!("W9 cleanup database differs");
            }
            let file = identity(&path)?;
            let source = opened.intent().source();
            let original = source.original();
            // An ACK path or mirrored rows in another real database cannot
            // substitute for the actual source's original path/dev/inode.
            original.validate_cleanup_issuer_v9(self)?;
            // The existing actual factory/claim/result validators ran before
            // entry. These are exact raw restrictions, never new authority.
            let tx = self.connection.unchecked_transaction()?;
            {
                // Original catalogue payload validator may copy its fixed
                // 128KiB raw saved view before hashing. Its scratch is held
                // independently from the later exact raw snapshot allocation.
                let _scratch = heap.reserve_model_v9(256 * 1024 + 32 * 1024)?;
                original.validate_cleanup_snapshot_v9(&tx)?;
            }
            let scope = original.scope();
            let pair = || vec![scope.host.clone().into(), scope.login.clone().into()];
            let mut rows = Vec::with_capacity(16);
            for (table, fields) in [
                (
                    "checkpoints",
                    "host,login,source,generation,fetched_at,complete",
                ),
                ("sync_reservations", "host,login,source,generation,active"),
            ] {
                let mut values = pair();
                values.push(original.sync_source().to_owned().into());
                rows.push(WorkflowRawRowsV9::capture(
                    &tx,
                    table,
                    fields,
                    "host=?1 AND login=?2 AND source=?3",
                    values,
                    heap,
                )?);
            }
            rows.push(WorkflowRawRowsV9::mapping(
                &tx,
                scope,
                original.saved_view_key(),
                heap,
            )?);
            rows.push(opened.binding().prepare_cleanup_mapping_v9(&tx, heap)?);
            source.prepare_cleanup_pr_v9(&tx, &mut rows, heap)?;
            history.prepare_cleanup_rows_v9(&tx, &mut rows, heap)?;
            let updated_at:i64=tx.query_row("SELECT updated_at FROM drafts WHERE host=?1 AND login=?2 AND resource=?3 AND typeof(body)='text' AND body=?4 COLLATE BINARY AND typeof(updated_at)='integer'",
                params![scope.host,scope.login,binding.resource_v9(),binding.body_v9()],|r|r.get(0))?;
            tx.commit()?;
            binding.check_request_v9(request, history, cancel, effective)?;
            opened.binding().check_cleanup_origin_v9(cancel)?;
            let saved_file = request
                .preview()
                .selection()
                .committed()
                .staged()
                .saved()
                .file();
            saved_file.validate_cleanup_descriptor_v9(cancel, effective)?;
            if identity(&path)? != file {
                bail!("W9 cleanup database changed during prepare");
            }
            Ok(PreparedWorkflowCleanupV9 {
                binding,
                opened: opened.clone(),
                saved_file: saved_file.clone(),
                history: history.clone(),
                ticket: Arc::new(()),
                rows,
                file,
                updated_at,
                effective,
                cancel: cancel.clone(),
                #[cfg(test)]
                test_heap: heap.clone(),
                _retained_charge: charge,
            })
        };
        prepare().map_err(|_| WorkflowCleanupFailureV9::Retained)
    }
}
impl PreparedWorkflowCleanupV9 {
    #[cfg(test)]
    pub(crate) fn test_original_heap_v9(&self) -> &Arc<WorkflowJsonLedgerV9> {
        &self.test_heap
    }
    pub(crate) fn matches_request_v9(&self, request: &WorkflowRemoteConfirmV9) -> bool {
        request.started_cleanup_v9()
            && self
                .binding
                .check_request_v9(
                    request,
                    &self.history,
                    request.cancellation(),
                    self.effective,
                )
                .is_ok()
    }
    pub(crate) fn ticket_v9(&self) -> &Arc<()> {
        &self.ticket
    }
    pub(crate) fn history_v9(&self) -> &Arc<ActionsOwnedWorkflowRemoteHistoryV9> {
        &self.history
    }
    pub(crate) fn effective_v9(&self) -> Instant {
        self.effective
    }
    pub(crate) fn matches_ack_v9(&self, ack: &crate::drafts::DraftPublicationAckV2) -> bool {
        self.binding.matches_ack_v9(ack)
    }
    pub(crate) fn matches_cursor_v9(
        &self,
        original: &Arc<crate::providers::gh::ScopedActionsWorkflowV2>,
        phase: crate::providers::gh::WorkflowEditPhaseV9,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> bool {
        Arc::ptr_eq(original, self.opened.intent().original())
            && self.binding.phase_v9() == phase
            && self.cancel.same_instance(cancel)
            && self.effective == deadline
            && Instant::now() < self.effective
            && !cancel.is_cancelled()
    }
    pub(crate) fn commit_in_writer_v9(
        self,
        cache: &Cache,
        seal: &WorkflowCleanupSealV9,
        cleanup: &AcknowledgedWorkflowCleanupV9<'_>,
    ) -> Result<CommittedWorkflowCleanupV9, WorkflowCleanupFailureV9> {
        let current = || -> anyhow::Result<()> {
            cleanup.check_current_v9().map_err(anyhow::Error::msg)?;
            if !cleanup.matches_binding_v9(&self.binding)
                || !seal.matches_v9(&self.ticket, &self.cancel, self.effective)
                || Instant::now() >= self.effective
                || self.cancel.is_cancelled()
                || cache.binding_lookup_issuer()? != self.binding.database_v9()
                || identity(self.binding.database_v9())? != self.file
            {
                bail!("W9 cleanup actual seal/source/token/file changed");
            }
            self.opened
                .binding()
                .check_cleanup_origin_v9(&self.cancel)?;
            self.saved_file
                .validate_cleanup_descriptor_v9(&self.cancel, self.effective)?;
            Ok(())
        };
        current().map_err(|_| WorkflowCleanupFailureV9::Retained)?;
        struct RestoreTimeout<'a>(&'a Connection);
        impl Drop for RestoreTimeout<'_> {
            fn drop(&mut self) {
                let _ = self.0.busy_timeout(Duration::from_secs(2));
            }
        }
        cache
            .connection
            .busy_timeout(
                self.effective
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_secs(2)),
            )
            .map_err(|_| WorkflowCleanupFailureV9::Retained)?;
        let _restore = RestoreTimeout(&cache.connection);
        // Busy-timeout changes affect only this same existing writer connection.
        let tx = Transaction::new_unchecked(&cache.connection, TransactionBehavior::Immediate)
            .map_err(|_| WorkflowCleanupFailureV9::Retained)?;
        let result = (|| -> anyhow::Result<()> {
            current()?;
            for rows in &self.rows {
                rows.validate(&tx)?;
            }
            let removed=tx.execute("DELETE FROM drafts WHERE host=?1 AND login=?2 AND resource=?3 AND typeof(body)='text' AND body=?4 COLLATE BINARY AND typeof(updated_at)='integer' AND updated_at=?5",
                params![self.binding.scope_v9().host,self.binding.scope_v9().login,self.binding.resource_v9(),cleanup.body_v9(),self.updated_at])?;
            if removed != 1 {
                bail!("W9 cleanup exact acknowledged draft changed");
            }
            current()?;
            Ok(())
        })();
        if result.is_err() {
            return Err(if tx.rollback().is_ok() {
                WorkflowCleanupFailureV9::Retained
            } else {
                WorkflowCleanupFailureV9::Unknown
            });
        }
        // Once commit is attempted, a failed commit is conservatively UNKNOWN;
        // never infer retention or mint a Branch continuation from a reply loss.
        tx.commit().map_err(|_| WorkflowCleanupFailureV9::Unknown)?;
        current().map_err(|_| WorkflowCleanupFailureV9::Unknown)?;
        Ok(CommittedWorkflowCleanupV9 {
            ticket: self.ticket,
            history: self.history,
        })
    }
}
