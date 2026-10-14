//! Structured metadata freshness is not discussion-text mutation. This worker
//! hook retains bytes while fencing OLD label-filter metadata and late reads.
use super::{validate_recorded_marker, PrMetadataCommand, PrMetadataReceipt};
use crate::{
    pr_write_journal::{PrPublicationReference, PrWriteJournal, PrWriteState, PrWriteTarget},
    providers::gh::{HostIdentity, PrSummary},
    storage::{Cache, SyncBindingKind, SyncFence, SyncNamespace},
};
use std::path::Path;

/// Prepared invalidation instructions, not authentication or fresh-view proof.
/// Root clears/replaces matching current metadata and Dashboard frames, freshly
/// guards each retained read and closes the principal after its callback.
pub struct PrMetadataStructuredImpact {
    target: PrWriteTarget,
    source_epoch: i64,
    hidden_label_sources: Vec<String>,
}
impl PrMetadataStructuredImpact {
    pub fn target(&self) -> &PrWriteTarget {
        &self.target
    }
    pub fn source_epoch(&self) -> i64 {
        self.source_epoch
    }
    pub fn hidden_label_sources(&self) -> &[String] {
        &self.hidden_label_sources
    }
    pub fn notice(&self) -> &'static str {
        "Current PR metadata and Dashboard evidence require fresh replacement. Label-filter frames are metadata-stale; retained discussion bytes are not claimed changed."
    }
}
pub fn invalidate_confirmed_metadata(
    database: &Path,
    identity: &HostIdentity,
    target: &PrSummary,
    reference: &PrPublicationReference,
) -> Result<PrMetadataStructuredImpact, String> {
    let key = PrWriteTarget::new(identity, target).map_err(|e| e.to_string())?;
    invalidate_confirmed_metadata_for_target(database, &key, reference)
}
fn invalidate_confirmed_metadata_for_target(
    database: &Path,
    key: &PrWriteTarget,
    reference: &PrPublicationReference,
) -> Result<PrMetadataStructuredImpact, String> {
    let journal = PrWriteJournal::open(database).map_err(|e| e.to_string())?;
    let marker = journal
        .original_publication(key, reference)
        .map_err(|e| e.to_string())?
        .ok_or(
            "Exact original metadata publication missing; no structured invalidation authority",
        )?;
    validate_recorded_marker(&marker)?;
    let receipt = PrMetadataReceipt::decode(
        marker
            .confirmed
            .as_ref()
            .ok_or("Exact metadata receipt missing")?,
    )?;
    let source_epoch = journal.source_generation(key).map_err(|e| e.to_string())?;
    let hidden_label_sources = if matches!(
        receipt.command(),
        PrMetadataCommand::AddLabel | PrMetadataCommand::RemoveLabel
    ) {
        Cache::open(database)
            .map_err(|e| e.to_string())?
            .fence_resource_sync_once(
                key.scope(),
                &SyncFence {
                    namespace: SyncNamespace::Collaboration,
                    repository_id: Some(receipt.repository_id()),
                    repository: key.repository().to_ascii_lowercase(),
                    resource: key.node_id().into(),
                    kinds: label_kinds(),
                },
                marker.attempt_id as u64,
            )
            .map_err(|e| e.to_string())?
            .sources
    } else {
        vec![]
    };
    Ok(PrMetadataStructuredImpact {
        target: key.clone(),
        source_epoch,
        hidden_label_sources,
    })
}
/// Only a controller that invoked the publication adapter calls this. Pending
/// alone is NOT invocation. This conservative fence does not consume a future
/// confirmed receipt's ordinal or assert that a body/label actually changed.
pub(crate) fn invalidate_uncertain_metadata(
    database: &Path,
    plan: &super::PrMetadataPlan,
    reference: &PrPublicationReference,
) -> Result<(), String> {
    let journal = PrWriteJournal::open(database).map_err(|e| e.to_string())?;
    let marker = journal
        .original_publication(plan.target(), reference)
        .map_err(|e| e.to_string())?
        .ok_or("Exact uncertain metadata attempt missing")?;
    if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain
    ) || marker.command != plan.command().key()
        || marker.payload_sha256 != plan.intent().digest().map_err(|e| e.to_string())?
    {
        return Err("Uncertain structured impact belongs to another exact original attempt".into());
    }
    if matches!(
        plan.command(),
        PrMetadataCommand::AddLabel | PrMetadataCommand::RemoveLabel
    ) {
        Cache::open(database)
            .map_err(|e| e.to_string())?
            .fence_resource_sync(
                plan.target().scope(),
                &SyncFence {
                    namespace: SyncNamespace::Collaboration,
                    repository_id: Some(plan.repository_id()),
                    repository: plan.target().repository().to_ascii_lowercase(),
                    resource: plan.target().node_id().into(),
                    kinds: label_kinds(),
                },
            )
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn label_kinds() -> Vec<SyncBindingKind> {
    vec![
        SyncBindingKind::PullRequestBody,
        SyncBindingKind::ConversationComment,
        SyncBindingKind::Review,
        SyncBindingKind::InlineComment,
    ]
}

/// SQL-only exact-parent label-filter fence. Root must supply fresh original
/// principal/access plus its sealed original Cache proof before and after this
/// hook; this is NOT authentication or cache membership permission.
#[derive(Debug)]
pub struct PrMetadataLabelReadFence {
    database: std::path::PathBuf,
    target: PrWriteTarget,
    epoch: i64,
    unavailable_reason: Option<&'static str>,
}
impl PrMetadataLabelReadFence {
    pub fn unavailable_reason(&self) -> Option<&'static str> {
        self.unavailable_reason
    }
    pub fn validate(&self) -> Result<(), String> {
        let journal = PrWriteJournal::open(&self.database).map_err(|e| e.to_string())?;
        if journal
            .source_generation(&self.target)
            .map_err(|e| e.to_string())?
            != self.epoch
        {
            return Err("PR metadata changed during label-filter collection; suppress stale structured results and retry the READ".into());
        }
        Ok(())
    }
}
/// Measures original journal metadata UTF-8 bytes in SQLite BEFORE copying any
/// strings. Reads only the latest confirmed label reference and latest unresolved
/// label reference (at most two), never unbounded bodies/intent/receipt JSON.
pub fn repair_label_filter_reads(
    database: &Path,
    identity: &HostIdentity,
    target: &PrSummary,
) -> Result<PrMetadataLabelReadFence, String> {
    let key = PrWriteTarget::new(identity, target).map_err(|e| e.to_string())?;
    repair_label_filter_reads_for_target(database, &key)
}
/// Exact source identity adapter. No fabricated summary/head/body is required.
pub(crate) fn repair_label_filter_reads_for_target(
    database: &Path,
    key: &PrWriteTarget,
) -> Result<PrMetadataLabelReadFence, String> {
    use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
    let journal = PrWriteJournal::open(database).map_err(|e| e.to_string())?;
    let epoch = journal.source_generation(key).map_err(|e| e.to_string())?;
    let connection =
        Connection::open_with_flags(journal.database_path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| e.to_string())?;
    let transaction = connection
        .unchecked_transaction()
        .map_err(|e| e.to_string())?;
    let bytes="length(CAST(repository AS BLOB))+length(CAST(command AS BLOB))+length(CAST(selected_resource AS BLOB))+length(CAST(observed_head AS BLOB))+length(CAST(payload_sha256 AS BLOB))+length(CAST(COALESCE(confirmed_json,'') AS BLOB))";
    let mut references = vec![];
    for states in ["state='confirmed'", "state IN('pending','uncertain')"] {
        let selected:Option<(i64,usize)>=transaction.query_row(&format!("SELECT attempt_id,{bytes} FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND command IN('metadata-add-label','metadata-remove-label') AND {states} ORDER BY attempt_id DESC LIMIT 1"),params![key.scope().host,key.scope().login,key.node_id()],|row|Ok((row.get(0)?,row.get(1)?))).optional().map_err(|e|e.to_string())?;
        if let Some((attempt, size)) = selected {
            if size > 8192 {
                return Err("Original label metadata exceeds 8 KiB UTF8 budget; no structured results released".into());
            }
            let reference=transaction.query_row(&format!("SELECT observed_head,payload_sha256 FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND attempt_id=?4 AND {bytes}<=8192"),params![key.scope().host,key.scope().login,key.node_id(),attempt],|row|Ok(PrPublicationReference{attempt_id:attempt,pr_node_id:key.node_id().into(),observed_head:row.get(0)?,payload_sha256:row.get(1)?})).map_err(|e|e.to_string())?;
            references.push(reference);
        }
    }
    transaction.commit().map_err(|e| e.to_string())?;
    let mut unavailable_reason = None;
    for reference in references {
        let marker = journal
            .original_publication(key, &reference)
            .map_err(|e| e.to_string())?
            .ok_or("Original selected label attempt changed or disappeared; no results released")?;
        if marker.state == PrWriteState::Confirmed {
            invalidate_confirmed_metadata_for_target(database, key, &reference)?;
        } else {
            let id = marker
                .selected_resource
                .strip_prefix("labels:")
                .and_then(|s| s.parse::<u64>().ok())
                .filter(|id| *id > 0 && *id <= i64::MAX as u64)
                .ok_or("Unresolved label attempt has no exact closed selected member identity")?;
            if marker.selected_resource != format!("labels:{id}") || marker.confirmed.is_some() {
                return Err(
                    "Unresolved label attempt has contradictory metadata; no results released"
                        .into(),
                );
            }
            Cache::open(database)
                .map_err(|e| e.to_string())?
                .fence_resource_sync(
                    key.scope(),
                    &SyncFence {
                        namespace: SyncNamespace::Collaboration,
                        repository_id: None,
                        repository: key.repository().to_ascii_lowercase(),
                        resource: key.node_id().into(),
                        kinds: label_kinds(),
                    },
                )
                .map_err(|e| e.to_string())?;
            unavailable_reason=Some("Unresolved original label publication: no invocation/effect inferred. Cached label filters are unavailable until explicit inspection and fresh metadata hydration.");
        }
    }
    let fence = PrMetadataLabelReadFence {
        database: journal.database_path().to_owned(),
        target: key.clone(),
        epoch,
        unavailable_reason,
    };
    fence.validate()?;
    Ok(fence)
}
