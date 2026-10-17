//! Exact bounded SQL-row fence prepared BEFORE final UID. This is local CAS
//! data only, never source/principal/permission or a restored publication.
use super::*;
#[derive(Clone)]
pub(in crate::actions_write_journal) struct HistoryRowFenceV6 {
    target: String,
    resources: String,
    kind: String,
    context: String,
    options: String,
    receipt: Option<String>,
    claimed: bool,
    created_at: u64,
}
pub(super) fn preflight(connection: &Connection, scope: &Scope, attempt_id: i64) -> Result<()> {
    let bytes:i64=connection.query_row(
        "SELECT length(CAST(target_json AS BLOB))+length(CAST(resources_json AS BLOB))+length(CAST(kind_json AS BLOB))+length(CAST(context_json AS BLOB))+length(CAST(options_json AS BLOB))+length(CAST(fingerprint_sha256 AS BLOB))+COALESCE(length(CAST(receipt_json AS BLOB)),0) FROM actions_write_attempts WHERE attempt_id=?1 AND host=?2 AND principal=?3",
        params![attempt_id,scope.host,scope.login],|row|row.get(0))?;
    if !(1..=128 * 1024).contains(&bytes) {
        bail!("Original history metadata exceeds128KiB before allocation");
    }
    Ok(())
}
impl HistoryRowFenceV6 {
    pub(in crate::actions_write_journal) fn claimed_v6(&self) -> bool {
        self.claimed
    }
    pub(in crate::actions_write_journal) fn capture(
        connection: &Connection,
        expected: &ActionsWriteMarker,
    ) -> Result<Self> {
        let scope = &expected.target.scope;
        let transaction = connection.unchecked_transaction()?;
        preflight(&transaction, scope, expected.attempt_id)?;
        // Existing canonical decoder under THIS same preflighted read snapshot,
        // not a nested transaction or a second metadata interpretation.
        let actual = read_attempt(&transaction, scope, expected.attempt_id)?
            .context("Original ACK row unavailable")?;
        let same_target = matches!((&actual.target.target,&expected.target.target),
            (ActionsTarget::Run{repository,id},ActionsTarget::Run{repository:original,id:original_id})
                |(ActionsTarget::Workflow{repository,id},ActionsTarget::Workflow{repository:original,id:original_id})
                if repository==original&&id==original_id);
        if !same_target
            || !actual.target.same_resources(&expected.target)
            || actual.kind != expected.kind
            || actual.context != expected.context
            || actual.options != expected.options
            || actual.fingerprint_sha256 != expected.fingerprint_sha256
            || actual.state != expected.state
            || actual.receipt != expected.receipt
            || actual.created_at != expected.created_at
        {
            bail!("Original ACK metadata changed before raw-row preparation");
        }
        actual.validate_metadata()?;
        let guard=transaction.query_row(
            "SELECT target_json,resources_json,kind_json,context_json,options_json,receipt_json,claimed,created_at FROM actions_write_attempts WHERE attempt_id=?1 AND host=?2 AND principal=?3",
            params![expected.attempt_id,scope.host,scope.login],|row|Ok(Self {
                target:row.get(0)?,resources:row.get(1)?,kind:row.get(2)?,context:row.get(3)?,
                options:row.get(4)?,receipt:row.get(5)?,claimed:row.get(6)?,created_at:row.get(7)?,
            }))?;
        if guard.retained_bytes() > 160 * 1024 {
            bail!("Original ACK raw-row retained capacity exceeds160KiB");
        }
        transaction.commit()?;
        Ok(guard)
    }
    pub(in crate::actions_write_journal) fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            + self.target.capacity()
            + self.resources.capacity()
            + self.kind.capacity()
            + self.context.capacity()
            + self.options.capacity()
            + self.receipt.as_ref().map_or(0, String::capacity)
    }
    pub(in crate::actions_write_journal) fn matches_state_v6(
        &self,
        connection: &Connection,
        marker: &ActionsWriteMarker,
    ) -> Result<bool> {
        // Bounded scalar predicate over PREPARED raw bytes, not another decoder
        // or a source/permission issuer. Include every raw metadata column.
        let count:i64=connection.query_row(
            "SELECT COUNT(*) FROM actions_write_attempts WHERE attempt_id=?1 AND host=?2 AND principal=?3 AND fingerprint_sha256=?4 AND state=?13 AND target_json=?5 AND resources_json=?6 AND kind_json=?7 AND context_json=?8 AND options_json=?9 AND receipt_json IS ?10 AND claimed=?11 AND created_at=?12",
            params![marker.attempt_id,marker.target.scope.host,marker.target.scope.login,marker.fingerprint_sha256,
                self.target,self.resources,self.kind,self.context,self.options,self.receipt,self.claimed,self.created_at,state_name(marker.state)],|row|row.get(0))?;
        Ok(count == 1)
    }
    pub(super) fn acknowledge(
        &self,
        transaction: &Connection,
        marker: &ActionsWriteMarker,
    ) -> Result<()> {
        // AFTER final UID: borrowed captured bytes become SQL CAS parameters.
        // No readback/body parse/string clone/serialization/rebase follows it.
        let changed=transaction.execute(
            "UPDATE actions_write_attempts SET state='acknowledged' WHERE attempt_id=?1 AND host=?2 AND principal=?3 AND fingerprint_sha256=?4 AND state='accepted_pending' AND target_json=?5 AND resources_json=?6 AND kind_json=?7 AND context_json=?8 AND options_json=?9 AND receipt_json IS ?10 AND claimed=?11 AND created_at=?12",
            params![marker.attempt_id,marker.target.scope.host,marker.target.scope.login,marker.fingerprint_sha256,
                self.target,self.resources,self.kind,self.context,self.options,self.receipt,self.claimed,self.created_at])?;
        if changed != 1 {
            bail!("Exact original ACK metadata changed during final principal read; protection retained");
        }
        Ok(())
    }
    pub(super) fn acknowledge_workflow_v6(
        &self,
        transaction: &Connection,
        marker: &ActionsWriteMarker,
        next: ActionsWriteState,
    ) -> Result<()> {
        self.acknowledge_state_v8(transaction, marker, next)
    }
    pub(super) fn acknowledge_state_v8(
        &self,
        transaction: &Connection,
        marker: &ActionsWriteMarker,
        next: ActionsWriteState,
    ) -> Result<()> {
        let changed=transaction.execute("UPDATE actions_write_attempts SET state=?13 WHERE attempt_id=?1 AND host=?2 AND principal=?3 AND fingerprint_sha256=?4 AND state=?14 AND target_json=?5 AND resources_json=?6 AND kind_json=?7 AND context_json=?8 AND options_json=?9 AND receipt_json IS ?10 AND claimed=?11 AND created_at=?12",
            params![marker.attempt_id,marker.target.scope.host,marker.target.scope.login,marker.fingerprint_sha256,
                self.target,self.resources,self.kind,self.context,self.options,self.receipt,self.claimed,self.created_at,state_name(next),state_name(marker.state)])?;
        if changed != 1 {
            bail!("Exact original workflow row changed during final principal read; protection retained");
        }
        Ok(())
    }
}
