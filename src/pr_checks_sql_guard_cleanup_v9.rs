//! Exact original PR owned projection prepared before final UID.
use super::*;
use crate::{providers::gh::WorkflowJsonLedgerV9, storage::workflow_cleanup_v9::WorkflowRawRowsV9};
use rusqlite::OptionalExtension;
impl PrChecksSqlGuard {
    pub(crate) fn prepare_workflow_cleanup_v9(
        &self,
        transaction: &rusqlite::Transaction<'_>,
        rows: &mut Vec<WorkflowRawRowsV9>,
        heap: &WorkflowJsonLedgerV9,
    ) -> anyhow::Result<()> {
        if private_file(&self.database).map_err(anyhow::Error::msg)? != self.file
            || stamp(transaction, &self.target).map_err(anyhow::Error::msg)?
                != self.cache_metadata_v4
        {
            anyhow::bail!("Original PR cache/file projection changed");
        }
        let generation:Option<i64>=transaction.query_row("SELECT generation FROM pr_write_scopes WHERE host=?1 AND principal=?2 AND pr_node=?3",params![self.target.scope().host,self.target.scope().login,self.target.node_id()],|r|r.get(0)).optional()?;
        let blocking:i64=transaction.query_row("SELECT count(*) FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND state IN('pending','uncertain')",params![self.target.scope().host,self.target.scope().login,self.target.node_id()],|r|r.get(0))?;
        if generation.unwrap_or(0) != self.pr_generation || blocking != 0 {
            anyhow::bail!("Original PR epoch/protection changed");
        }
        rows.extend(WorkflowRawRowsV9::actual_pr_v9(
            transaction,
            &self.target,
            heap,
        )?);
        Ok(())
    }
}
fn stamp(
    transaction: &rusqlite::Transaction<'_>,
    target: &PrWriteTarget,
) -> Result<[u8; 32], String> {
    let claimed_actions = true;
    let mut hash = Sha256::new();
    hash.update(b"ghtui-pr-checks-local-fence:v1");
    for value in [
        target.scope().host.as_str(),
        target.scope().login.as_str(),
        target.repository(),
        target.node_id(),
    ] {
        hash.update(value.len().to_le_bytes());
        hash.update(value.as_bytes());
    }
    hash.update(target.number().to_le_bytes());
    let mut total_rows = 0usize;
    let mut total_bytes = 0usize;
    for table in TABLES {
        // Only the purpose-closed live claimed-publication validator uses this
        // projection. Ordinary guards still hash the full Actions generation
        // table and unresolved page exactly as before.
        if claimed_actions && table.name == "actions_write_generations" {
            continue;
        }
        let lengths = table
            .fields
            .split(',')
            .take(table.keys)
            .map(|field| format!("length(CAST({field} AS BLOB))"))
            .collect::<Vec<_>>();
        let largest = if lengths.len() == 1 {
            lengths[0].clone()
        } else {
            format!("max({})", lengths.join(","))
        };
        let mut statement = transaction
            .prepare(&format!(
                "SELECT COUNT(*),COALESCE(SUM({}),0),COALESCE(MAX({}),0) FROM {} WHERE {}",
                table.sizes, largest, table.name, table.predicate
            ))
            .map_err(|error| error.to_string())?;
        let parameters = || {
            let mut args = vec![target.scope().host.as_str(), target.scope().login.as_str()];
            if table.predicate.contains("?3") {
                args.push(target.repository());
            }
            args
        };
        let (rows, bytes, largest): (usize, usize, usize) = statement
            .query_row(rusqlite::params_from_iter(parameters()), |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|error| error.to_string())?;
        total_rows = total_rows
            .checked_add(rows)
            .ok_or("Checks metadata row accounting overflow")?;
        total_bytes = total_bytes
            .checked_add(bytes)
            .ok_or("Checks metadata byte accounting overflow")?;
        if total_rows > MAX_EPOCH_ROWS || total_bytes > MAX_METADATA_BYTES || largest > 4096 {
            return Err("PR checks local metadata budget exceeded before epoch/key copy; prior frames hidden".into());
        }
        hash.update(table.name.as_bytes());
        hash.update(rows.to_le_bytes());
        let mut read = transaction
            .prepare(&format!(
                "SELECT {} FROM {} WHERE {} ORDER BY {}",
                table.fields, table.name, table.predicate, table.order
            ))
            .map_err(|error| error.to_string())?;
        let mut cursor = read
            .query(rusqlite::params_from_iter(parameters()))
            .map_err(|error| error.to_string())?;
        let mut copied = 0;
        while let Some(row) = cursor.next().map_err(|error| error.to_string())? {
            for index in 0..table.keys {
                let key: String = row.get(index).map_err(|error| error.to_string())?;
                if key.is_empty() || key.len() > 4096 || key.chars().any(char::is_control) {
                    return Err("Malformed bounded checks metadata key; prior frames hidden".into());
                }
                hash.update(key.len().to_le_bytes());
                hash.update(key.as_bytes());
            }
            for index in 0..table.numbers {
                let value: i64 = row
                    .get(table.keys + index)
                    .map_err(|error| error.to_string())?;
                if value < 0 || (table.boolean && index + 1 == table.numbers && value > 1) {
                    return Err(
                        "Malformed bounded checks metadata generation/state; prior frames hidden"
                            .into(),
                    );
                }
                hash.update(value.to_le_bytes());
            }
            copied += 1;
        }
        if copied != rows {
            return Err("Checks metadata changed during coherent local read".into());
        }
    }
    Ok(hash.finalize().into())
}
