//! Two known metadata text purposes, never a generic Metadata passthrough.
use crate::{
    pr_write_journal::{metadata_origin_v25, PrWriteJournal, PrWriteMarker, PrWriteTarget},
    storage::SyncBindingKind,
};
pub(crate) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    if !metadata_origin_v25::recognized(&marker.command) {
        return Ok(None);
    }
    metadata_origin_v25::validate_shape(marker, confirmed).map_err(|e| e.to_string())?;
    Ok(Some(vec![SyncBindingKind::PullRequestBody]))
}
pub(crate) fn validate_original(
    marker: &PrWriteMarker,
    journal: &PrWriteJournal,
    target: &PrWriteTarget,
) -> Result<(), String> {
    if !metadata_origin_v25::recognized(&marker.command) {
        return Ok(());
    }
    journal
        .metadata_origin_v25(target, marker)
        .map_err(|e| e.to_string())?;
    Ok(())
}
