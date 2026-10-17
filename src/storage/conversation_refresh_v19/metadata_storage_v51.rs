//! Restriction-only preflight of actual Metadata prepared serialization peak.
//! No records/body DTO is cloned, source issued or publication altered here.
use super::*;
use crate::providers::gh::ActualMetadataTabContextV51;
use crate::search::collaboration::{DiscussionTarget, ParentKind};
use crate::search::model::DiscussionKind;

struct CountV51(usize);
impl std::io::Write for CountV51 {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0 = self
            .0
            .checked_add(bytes.len())
            .filter(|n| *n <= PREPARED_BYTES_V19)
            .ok_or_else(|| std::io::Error::other("Metadata serialized preflight exceeds8MiB"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct PeakV51(usize);
impl PeakV51 {
    fn add(&mut self, bytes: usize) -> Result<()> {
        self.0 = self
            .0
            .checked_add(bytes)
            .filter(|n| *n <= PREPARED_BYTES_V19)
            .context("Actual Metadata simultaneous prepared allocations exceed8MiB before clone")?;
        Ok(())
    }
    fn product(&mut self, n: usize, each: usize) -> Result<()> {
        self.add(
            n.checked_mul(each)
                .context("Metadata preflight arithmetic overflow")?,
        )
    }
}
/// Before DiscussionSnapshot::from_conversation_refresh_v19, documents(),
/// serde_json::to_value/to_vec, encode_records, epoch or old-members copying.
/// Every term is based on the SAME actual snapshot/context/terminal/DB.
pub(crate) fn validate_metadata_storage_peak_v51(
    source: &Arc<PrCollaborationSourceV17>,
    terminal: &Arc<ConversationTerminalWitnessV19>,
    actual: &ConversationDetailSnapshotV19,
    context: &ActualMetadataTabContextV51,
    cancel: &CancellationToken,
) -> Result<()> {
    if !source.opening().metadata_tab_v51() {
        bail!("Exact Metadata prepared profile unavailable");
    }
    validate_page_storage_peak_v61(
        source,
        terminal,
        actual,
        context.retained_bytes(),
        None,
        0,
        cancel,
    )
}
/// Same borrowed serialization/SQL/clone-overlap accounting, closed solely to
/// an actual Members terminal and its provider-produced display supplement.
pub(crate) fn validate_members_storage_peak_v61(
    source: &Arc<PrCollaborationSourceV17>,
    terminal: &Arc<ConversationTerminalWitnessV19>,
    actual: &ConversationDetailSnapshotV19,
    context: &crate::providers::gh::ActualMembersPageContextV61,
    cancel: &CancellationToken,
) -> Result<()> {
    let command = terminal
        .members_command_v48()
        .context("Actual Members terminal unavailable")?;
    if source.opening().metadata_members_tab_v61(command)
        || !source.opening().admits_metadata_members_v61(command)
        || !context.validates_command_v62(command)
    {
        bail!("Exact ordinary Members prepared profile unavailable");
    }
    context
        .validate_snapshot_v61(actual)
        .map_err(anyhow::Error::msg)?;
    validate_page_storage_peak_v61(
        source,
        terminal,
        actual,
        context.retained_bytes_v61(),
        Some((
            context.encoded_bytes_v62().map_err(anyhow::Error::msg)?,
            context.retained_bytes_v61(),
            0,
        )),
        0,
        cancel,
    )
}
pub(crate) fn validate_metadata_members_storage_peak_v62(
    source: &Arc<PrCollaborationSourceV17>,
    terminal: &Arc<ConversationTerminalWitnessV19>,
    actual: &ConversationDetailSnapshotV19,
    context: &ActualMetadataTabContextV51,
    cancel: &CancellationToken,
) -> Result<()> {
    let command = terminal
        .members_command_v48()
        .context("Actual Members terminal unavailable")?;
    if !source.opening().metadata_members_tab_v61(command) {
        bail!("Exact Metadata Members source purpose unavailable");
    }
    context
        .validate_members_snapshot_v62(command, actual)
        .map_err(anyhow::Error::msg)?;
    validate_page_storage_peak_v61(
        source,
        terminal,
        actual,
        context.retained_bytes(),
        Some((
            context
                .members_encoded_bytes_v62(command)
                .map_err(anyhow::Error::msg)?,
            context
                .members_context_copy_bytes_v62(command)
                .map_err(anyhow::Error::msg)?,
            context
                .members_producer_overlap_v62(command)
                .map_err(anyhow::Error::msg)?,
        )),
        0,
        cancel,
    )
}
pub(super) fn validate_atomic_storage_peak_v85(
    source: &Arc<PrCollaborationSourceV17>,
    terminal: &Arc<ConversationTerminalWitnessV19>,
    actual: &ConversationDetailSnapshotV19,
    supplement: &ConversationPageSupplementV61,
    binding: &crate::drafts::AtomicCleanupBindingV85,
    cancel: &CancellationToken,
) -> Result<()> {
    if !binding.matches_terminal_v85(terminal) {
        bail!("Foreign atomic terminal before copy");
    }
    let mut issue_extra_v87 = 0;
    let (retained, members) = match supplement {
        ConversationPageSupplementV61::None => {
            if terminal.members_command_v48().is_some()
                || terminal.issue_link_command_v87().is_some()
                || source.opening().metadata_tab_v51()
            {
                bail!("Atomic native page cannot omit its actual purpose context");
            }
            (0, None)
        }
        ConversationPageSupplementV61::IssueLinksV87(context) => {
            context
                .validate_snapshot_v87(actual)
                .map_err(anyhow::Error::msg)?;
            if !context.matches_source_v87(source)
                || !terminal.validates_issue_link_context_v87(context)
            {
                bail!("Foreign manual Issue atomic context");
            }
            issue_extra_v87 = context
                .fixed_record_copy_peak_v87(actual, terminal)
                .map_err(anyhow::Error::msg)?;
            (context.retained_bound_v87(), None)
        }
        ConversationPageSupplementV61::Members(context) => {
            let command = terminal
                .members_command_v48()
                .context("Actual Members terminal unavailable")?;
            if source.opening().metadata_members_tab_v61(command)
                || !source.opening().admits_metadata_members_v61(command)
                || !context.validates_command_v62(command)
            {
                bail!("Foreign ordinary Members atomic context");
            }
            context
                .validate_snapshot_v61(actual)
                .map_err(anyhow::Error::msg)?;
            (
                context.retained_bytes_v61(),
                Some((
                    context.encoded_bytes_v62().map_err(anyhow::Error::msg)?,
                    context.retained_bytes_v61(),
                    0,
                )),
            )
        }
        ConversationPageSupplementV61::Metadata(context) => {
            if let Some(command) = terminal.members_command_v48() {
                if !source.opening().metadata_members_tab_v61(command) {
                    bail!("Foreign Metadata Members atomic purpose");
                }
                context
                    .validate_members_snapshot_v62(command, actual)
                    .map_err(anyhow::Error::msg)?;
                (
                    context.retained_bytes(),
                    Some((
                        context
                            .members_encoded_bytes_v62(command)
                            .map_err(anyhow::Error::msg)?,
                        context
                            .members_context_copy_bytes_v62(command)
                            .map_err(anyhow::Error::msg)?,
                        context
                            .members_producer_overlap_v62(command)
                            .map_err(anyhow::Error::msg)?,
                    )),
                )
            } else {
                if !source.opening().metadata_tab_v51()
                    || !crate::providers::gh::GithubProvider::same_thread_history_parent_v1(
                        actual.parent(),
                        context.parent(),
                    )
                {
                    bail!("Foreign Metadata atomic context");
                }
                (context.retained_bytes(), None)
            }
        }
    };
    validate_page_storage_peak_v61(
        source,
        terminal,
        actual,
        retained,
        members,
        binding
            .retained_bytes_v85()
            .checked_add(issue_extra_v87)
            .context("Issue atomic preclone overlap overflow")?,
        cancel,
    )
}
pub(super) fn validate_issue_link_storage_peak_v87(
    source: &Arc<PrCollaborationSourceV17>,
    terminal: &Arc<ConversationTerminalWitnessV19>,
    actual: &ConversationDetailSnapshotV19,
    context: &crate::providers::gh::ActualIssueLinkPageContextV87,
    cancel: &CancellationToken,
) -> Result<()> {
    context
        .validate_snapshot_v87(actual)
        .map_err(anyhow::Error::msg)?;
    if !context.matches_source_v87(source) || !terminal.validates_issue_link_context_v87(context) {
        bail!("Foreign manual Issue prepared context");
    }
    let copies = context
        .fixed_record_copy_peak_v87(actual, terminal)
        .map_err(anyhow::Error::msg)?;
    validate_page_storage_peak_v61(
        source,
        terminal,
        actual,
        context.retained_bound_v87(),
        None,
        copies,
        cancel,
    )
}
fn validate_page_storage_peak_v61(
    source: &Arc<PrCollaborationSourceV17>,
    terminal: &Arc<ConversationTerminalWitnessV19>,
    actual: &ConversationDetailSnapshotV19,
    context_retained: usize,
    members_encoded: Option<(usize, usize, usize)>,
    atomic_extra_v85: usize,
    cancel: &CancellationToken,
) -> Result<()> {
    if !Arc::ptr_eq(source, terminal.source_v19()) || cancel.is_cancelled() {
        bail!("Exact prepared page source unavailable");
    }
    terminal.validate_file_v19()?;
    let model = actual
        .metadata_model_bytes_v51()
        .map_err(anyhow::Error::msg)?;
    let mut peak = PeakV51(
        model
            .checked_add(context_retained)
            .context("Metadata retained charge overflow")?,
    );
    peak.add(atomic_extra_v85)?;
    if let Some((encoded, context_copies, producer_overlap)) = members_encoded {
        // Actual original/plan lives ONCE through finalUID; unlike context it
        // is not cloned into twelve serialized record/Value representations.
        peak.add(producer_overlap)?;
        // The original PrContext remains alive until page assembly. Its Value
        // record then coexists with the ordinary details/comment Values and
        // encoded rows. Twelve retained equivalents cover the original, the
        // generated Value tree (including arbitrary issue_metadata BTreeMap
        // keys/nodes), serializer traversal/scratch and Vec spare capacity.
        // This extra debit is Members-only; existing None/Metadata arithmetic
        // and record paths are unchanged.
        peak.product(12, context_copies)?;
        // Counted directly from the SAME actual PrContext without a Value or
        // byte-buffer allocation. Sixfold escaping with 2x String/Vec growth
        // covers serialization scratch plus encoded payload while the Value
        // and original context still live; encode_records enforces the actual
        // remaining8MiB bound again after preparation.
        peak.product(12, encoded)?;
        // One fixed Record, one EncodedRecord, vector growth and key-set nodes
        // coexist. Generated kind/id/repository/timestamp strings have these
        // exact actual lengths; 1KiB per fixed field covers map/tree nodes.
        let target = terminal.target();
        let metadata = "pr-metadata-context-v48"
            .len()
            .checked_add(target.node_id().len())
            .and_then(|n| n.checked_add(target.repository().len()))
            .and_then(|n| n.checked_add(actual.details().summary.pr.updated_at.len()))
            .context("Members record metadata capacity overflow")?;
        peak.product(12, metadata)?;
        peak.add(2 * std::mem::size_of::<Record>())?;
        peak.add(2 * std::mem::size_of::<EncodedRecordV19>())?;
        peak.add(8 * 1024)?;
    }
    let details = actual.details();
    let mut serialized = CountV51(0);
    serde_json::to_writer(&mut serialized, details)?;
    let comments = details.comments.len();
    let documents = comments
        .checked_add(1)
        .context("Metadata document count overflow")?;
    // Actual body/comments can coexist in: DiscussionSnapshots, Documents,
    // Value records, projection details+comment Values, plus worst sixfold
    // escaped serde scratch and Vec spare growth. Twelve complete retained
    // model copies conservatively cover these12 simultaneous content terms.
    // This is a pre-clone restriction, not an assertion that clones are free.
    peak.product(12, model)?;
    // Discussion encoded bodies+contexts and the details/comment projection
    // each contain only actual data above. Bound six complete serialized
    // details equivalents, with2x Vec/String capacity growth, separately from
    // the Value/DTO copies. Metadata-specific fields/keys are charged below.
    peak.product(12, serialized.0)?;
    let target = terminal.target();
    let metadata_strings = [
        target.scope().host.as_str(),
        target.scope().login.as_str(),
        target.repository(),
        target.node_id(),
        actual.parent().head_sha(),
        details.summary.pr.state.as_str(),
    ]
    .into_iter()
    .try_fold(0usize, |n, v| {
        n.checked_add(v.len())
            .context("Metadata target string overflow")
    })?;
    // Each Document/entry/context/destination repeats at most16 of the actual
    // host/scope/repo/node/head/state strings. Fixed generated captions/source
    // hashes fit1KiB/document. Sixfold escaping and2x retained capacity coexist.
    let per_document = metadata_strings
        .checked_mul(16)
        .and_then(|n| n.checked_add(1024))
        .and_then(|n| n.checked_mul(12))
        .context("Metadata DTO metadata overflow")?;
    peak.product(documents, per_document)?;
    // Fixed serializer object fields: <=128/document (including nested actor,
    // key, destination and DiscussionContext); details projection<=128 fixed,
    // plus20/comment,24/commit,8/file. Each key reserves a full1KiB BTreeMap
    // node/split+Value/key allocation. Existing arbitrary review/timeline Map
    // nodes already appear in the actual model charge copied12 times above.
    let fields = documents
        .checked_mul(128)
        .and_then(|n| n.checked_add(128))
        .and_then(|n| comments.checked_mul(20).and_then(|v| n.checked_add(v)))
        .and_then(|n| {
            details
                .commits
                .len()
                .checked_mul(24)
                .and_then(|v| n.checked_add(v))
        })
        .and_then(|n| {
            details
                .files
                .len()
                .checked_mul(8)
                .and_then(|v| n.checked_add(v))
        })
        .context("Metadata payload field count overflow")?;
    peak.product(fields, 1024)?;
    // Entry/Document/Record/EncodedRecord Vec spare storage and validation/key
    // BTreeSets coexist.8KiB per actual document covers their real fixed structs,
    // Vec doubling, borrowed node sets and owned stable-id/source key copies.
    peak.product(
        documents
            .checked_add(4)
            .context("Metadata row count overflow")?,
        8 * 1024,
    )?;
    peak.add(4 * std::mem::size_of::<Instant>())?;
    let db = rusqlite::Connection::open_with_flags(
        terminal.database(),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let tx = db.unchecked_transaction()?;
    let mut witness_bytes = 0usize;
    // SAME closed profiles/where clauses as EpochV19::capture. SQL scalar
    // length/count prechecks happen BEFORE copying any row/body witness.
    for(table,fields,predicate,repo)in[
        ("sync_reservations","host,login,source,generation,active","host=?1 AND login=?2",false),
        ("checkpoints","host,login,source,generation,fetched_at,complete","host=?1 AND login=?2",false),
        ("checkpoint_records","host,login,source,kind,id","host=?1 AND login=?2",false),
        ("records","host,login,kind,id,repository,fetched_at,source_updated_at,complete","host=?1 AND login=?2 AND repository=?3 COLLATE NOCASE",true),
        ("sync_binding_lookup_epochs","host,login,namespace,repository,generation","host=?1 AND login=?2 AND repository=?3 COLLATE NOCASE",true),
        ("sync_resource_bindings","host,login,source,namespace,repository_id,repository,resource,kind,reservation_generation,visible_generation,active","host=?1 AND login=?2 AND repository=?3 COLLATE NOCASE AND resource=?4",true),
        ("sync_mutation_fences","host,login,namespace,repository,resource,kind,mutation_ordinal","host=?1 AND login=?2 AND repository=?3 COLLATE NOCASE AND resource=?4",true),
    ]{
        let mut values=vec![SqlValue::Text(target.scope().host.clone()),SqlValue::Text(target.scope().login.clone())];
        if repo{values.push(SqlValue::Text(target.repository().into()));}
        if predicate.contains("?4"){values.push(SqlValue::Text(target.node_id().into()));}
        witness_precheck(&tx,table,fields,predicate,&values,&mut witness_bytes,&mut peak)?;
    }
    if let Some(catalogue) = source.opening().catalogue_source() {
        witness_precheck(
            &tx,
            "saved_views",
            "host,login,name,payload",
            "host=?1 AND login=?2 AND name=?3",
            &[
                SqlValue::Text(target.scope().host.clone()),
                SqlValue::Text(target.scope().login.clone()),
                SqlValue::Text(catalogue.saved_view_key().into()),
            ],
            &mut witness_bytes,
            &mut peak,
        )?;
    }
    for category in [
        DiscussionKind::PullRequestBody,
        DiscussionKind::ConversationComment,
    ] {
        let target = DiscussionTarget {
            scope: target.scope().clone(),
            repository_id: terminal.repository_id(),
            repository: target.repository().into(),
            parent_kind: ParentKind::PullRequest,
            parent_node: target.node_id().into(),
            number: target.number(),
            category,
        };
        target.validate().map_err(anyhow::Error::msg)?;
        membership_precheck(&tx, terminal.target().scope(), &target.source(), &mut peak)?;
    }
    membership_precheck(
        &tx,
        target.scope(),
        &format!("pr-write-projection:{}", target.node_id()),
        &mut peak,
    )?;
    tx.commit()?;
    terminal.validate_file_v19()?;
    if cancel.is_cancelled() {
        bail!("Metadata preflight cancelled");
    }
    Ok(())
}
fn witness_precheck(
    tx: &Transaction<'_>,
    table: &str,
    fields: &str,
    predicate: &str,
    values: &[SqlValue],
    remaining: &mut usize,
    peak: &mut PeakV51,
) -> Result<()> {
    let columns = fields.split(',').collect::<Vec<_>>();
    let lengths = columns
        .iter()
        .map(|field| format!("COALESCE(length(CAST({field} AS BLOB)),0)"))
        .collect::<Vec<_>>()
        .join("+");
    let (count, bytes): (usize, usize) = tx.query_row(
        &format!("SELECT count(*),COALESCE(SUM({lengths}),0) FROM {table} WHERE {predicate}"),
        params_from_iter(values),
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    *remaining = remaining
        .checked_add(bytes)
        .filter(|n| *n <= METADATA_BYTES_V19)
        .context("Metadata exact SQL witness exceeds64KiB")?;
    if count > 512 || (table == "sync_resource_bindings" && count > 32) {
        bail!("Metadata exact SQL witness count exceeds existing bound");
    }
    peak.product(bytes, 2)?; // owned SQL strings/blobs plus statement scratch
    peak.product(
        count,
        columns
            .len()
            .checked_mul(2 * std::mem::size_of::<SqlValue>())
            .and_then(|n| n.checked_add(256))
            .context("Metadata SQL row storage overflow")?,
    )?;
    Ok(())
}
fn membership_precheck(
    tx: &Transaction<'_>,
    scope: &Scope,
    source: &str,
    peak: &mut PeakV51,
) -> Result<()> {
    let(count,bytes):(usize,usize)=tx.query_row("SELECT count(*),COALESCE(SUM(length(CAST(kind AS BLOB))+length(CAST(id AS BLOB))),0) FROM checkpoint_records WHERE host=?1 AND login=?2 AND source=?3",params![scope.host,scope.login,source],|r|Ok((r.get(0)?,r.get(1)?)))?;
    if count > 10_001 {
        bail!("Metadata old membership exceeds existing10001 bound");
    }
    peak.product(bytes, 2)?;
    peak.product(count, 512)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn database() -> (tempfile::TempDir, rusqlite::Connection) {
        let root = tempfile::tempdir().unwrap();
        // Only this fixture-owned private directory changes; inherited umask,
        // production cache validation and sibling directories stay untouched.
        std::fs::set_permissions(root.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = root.path().join("metadata-preclone.sqlite");
        let _ = Cache::open(&path).unwrap();
        let db = rusqlite::Connection::open(path).unwrap();
        (root, db)
    }
    #[test]
    fn conversation_comments_metadata_tab_v51_storage_peak_overflow_and_escaped_count_are_bounded()
    {
        let mut peak = PeakV51(PREPARED_BYTES_V19 - 16);
        peak.add(16).unwrap();
        assert_eq!(peak.0, PREPARED_BYTES_V19);
        assert!(peak.add(1).is_err());
        assert!(PeakV51(0).product(usize::MAX, 2).is_err());
        let body = "\0\u{1}\"\\界".repeat(128);
        let encoded = serde_json::to_vec(&body).unwrap();
        let mut count = CountV51(0);
        serde_json::to_writer(&mut count, &body).unwrap();
        assert_eq!(count.0, encoded.len());
        assert!(count.0 > body.len());
        let mut count = CountV51(PREPARED_BYTES_V19 - 1);
        assert!(std::io::Write::write(&mut count, b"xx").is_err());
    }
    #[test]
    fn conversation_comments_metadata_tab_v51_sql_witness_actual_lengths_counts_before_clone() {
        let (_root, mut db) = database();
        let tx = db.transaction().unwrap();
        tx.execute(
            "INSERT INTO saved_views VALUES('github.invalid','user-id:42','actual-view',?1)",
            ["界".repeat(64)],
        )
        .unwrap();
        let values = [
            SqlValue::Text("github.invalid".into()),
            SqlValue::Text("user-id:42".into()),
        ];
        let mut peak = PeakV51(0);
        let mut bytes = 0;
        witness_precheck(
            &tx,
            "saved_views",
            "host,login,name,payload",
            "host=?1 AND login=?2",
            &values,
            &mut bytes,
            &mut peak,
        )
        .unwrap();
        assert_eq!(
            bytes,
            "github.invalid".len() + "user-id:42".len() + "actual-view".len() + 3 * 64
        );
        assert!(peak.0 > 2 * bytes);
        tx.execute(
            "UPDATE saved_views SET payload=?1",
            ["x".repeat(METADATA_BYTES_V19 + 1)],
        )
        .unwrap();
        assert!(witness_precheck(
            &tx,
            "saved_views",
            "host,login,name,payload",
            "host=?1 AND login=?2",
            &values,
            &mut 0,
            &mut PeakV51(0)
        )
        .is_err());
        tx.execute("DELETE FROM saved_views", []).unwrap();
        tx.execute_batch("WITH RECURSIVE n(x) AS(VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<513) INSERT INTO saved_views SELECT 'github.invalid','user-id:42',CAST(x AS TEXT),'{}' FROM n;").unwrap();
        assert!(witness_precheck(
            &tx,
            "saved_views",
            "host,login,name,payload",
            "host=?1 AND login=?2",
            &values,
            &mut 0,
            &mut PeakV51(0)
        )
        .is_err());
        assert_eq!(
            tx.query_row("SELECT count(*) FROM saved_views", [], |r| r
                .get::<_, usize>(0))
                .unwrap(),
            513,
            "Restriction checks never delete or partly copy old witnesses"
        );
    }
    #[test]
    fn conversation_comments_metadata_tab_v51_old_membership_preflight_is_exact_and_no_partial_clear(
    ) {
        let (_root, mut db) = database();
        let tx = db.transaction().unwrap();
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        tx.execute_batch("INSERT INTO records VALUES('github.invalid','user-id:42','actual-kind','actual-member','example/repo','{}',0,NULL,1); INSERT INTO checkpoint_records VALUES('github.invalid','user-id:42','actual-source','actual-kind','actual-member');").unwrap();
        let mut peak = PeakV51(0);
        membership_precheck(&tx, &scope, "actual-source", &mut peak).unwrap();
        assert_eq!(
            peak.0,
            2 * ("actual-kind".len() + "actual-member".len()) + 512
        );
        assert!(membership_precheck(
            &tx,
            &scope,
            "actual-source",
            &mut PeakV51(PREPARED_BYTES_V19 - 512)
        )
        .is_err());
        tx.execute_batch("WITH RECURSIVE n(x) AS(VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<10002) INSERT INTO records SELECT 'github.invalid','user-id:42','many',CAST(x AS TEXT),'example/repo','{}',0,NULL,1 FROM n; INSERT INTO checkpoint_records SELECT host,login,'over-count',kind,id FROM records WHERE kind='many';").unwrap();
        assert!(membership_precheck(&tx, &scope, "over-count", &mut PeakV51(0)).is_err());
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM checkpoint_records WHERE source='over-count'",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            10002
        );
        assert_eq!(
            tx.query_row(
                "SELECT count(*) FROM checkpoint_records WHERE source='actual-source'",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap(),
            1
        );
    }
}
