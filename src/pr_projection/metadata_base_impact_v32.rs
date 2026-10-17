//! Exactly the typed Base comparison-context effect, not a text-body change.
use crate::{
    pr_write_journal::{metadata_base_origin_v32, PrWriteJournal, PrWriteMarker, PrWriteTarget},
    storage::SyncBindingKind,
};
pub(crate) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    if marker.command != metadata_base_origin_v32::COMMAND_V32 {
        return Ok(None);
    }
    metadata_base_origin_v32::validate_shape(marker, confirmed).map_err(|e| e.to_string())?;
    Ok(Some(vec![
        SyncBindingKind::Review,
        SyncBindingKind::InlineComment,
    ]))
}
pub(crate) fn validate_original(
    marker: &PrWriteMarker,
    journal: &PrWriteJournal,
    target: &PrWriteTarget,
) -> Result<(), String> {
    if marker.command != metadata_base_origin_v32::COMMAND_V32 {
        return Ok(());
    }
    journal
        .metadata_base_origin_v32(target, marker)
        .map_err(|e| e.to_string())?;
    Ok(())
}
