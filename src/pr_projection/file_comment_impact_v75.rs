//! Only the closed FILE-create purpose affects Inline discussion freshness.
//! A known malformed newest marker refuses before private admission; it cannot
//! fall through or revive an older well-formed receipt.
use crate::{
    pr_write_journal::{file_comment_history_v75, PrWriteMarker, PrWriteState},
    storage::SyncBindingKind,
};
pub(crate) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    if marker.command != file_comment_history_v75::COMMAND {
        return Ok(None);
    }
    file_comment_history_v75::validate_marker(marker).map_err(|e| e.to_string())?;
    if (confirmed && marker.state != PrWriteState::Confirmed)
        || (!confirmed
            && !matches!(
                marker.state,
                PrWriteState::Pending | PrWriteState::Uncertain
            ))
    {
        return Err(
            "FILE freshness requires exact Confirmed or protected Pending/Uncertain state".into(),
        );
    }
    Ok(Some(vec![SyncBindingKind::InlineComment]))
}
