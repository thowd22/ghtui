//! Member metadata changes structured filters, not discussion TEXT. The
//! reserved selector identifies only a fail-closed grammar; actual origin
//! validation is mandatory before any impact/repair is admitted.
use crate::{
    pr_metadata_actions::{PrMetadataCommand, PrMetadataReceipt},
    pr_write_journal::{
        metadata_members_v48::MetadataOriginDispositionV48, PrWriteJournal, PrWriteMarker,
        PrWriteState, PrWriteTarget,
    },
    storage::SyncBindingKind,
};
pub(crate) fn validate_original(
    marker: &PrWriteMarker,
    journal: &PrWriteJournal,
    target: &PrWriteTarget,
) -> Result<(), String> {
    if !PrMetadataCommand::ALL
        .into_iter()
        .any(|command| command.key() == marker.command)
        && !marker
            .selected_resource
            .starts_with("metadata-members-v48/")
    {
        return Ok(());
    }
    match journal
        .metadata_origin_disposition_v48(target, marker)
        .map_err(|e| e.to_string())?
    {
        MetadataOriginDispositionV48::LegacyOriginless => Ok(()),
        MetadataOriginDispositionV48::TextV25 => {
            Err("Foreign Text original on a member metadata purpose".into())
        }
        MetadataOriginDispositionV48::MembersV48(record) => {
            record
                .seed
                .validate_marker(journal.database_path(), target, marker)
                .map_err(|e| e.to_string())?;
            Ok(())
        }
    }
}
pub(crate) fn categories(
    marker: &PrWriteMarker,
    confirmed: bool,
) -> Result<Option<Vec<SyncBindingKind>>, String> {
    let Some(key) = marker
        .selected_resource
        .strip_prefix("metadata-members-v48/")
    else {
        return Ok(None);
    };
    if key.len() != 64
        || !key
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err("Malformed known native member selector; no legacy fallback".into());
    }
    let command = PrMetadataCommand::ALL
        .into_iter()
        .find(|command| command.key() == marker.command)
        .ok_or("Unknown closed native member purpose")?;
    if confirmed {
        let receipt = PrMetadataReceipt::decode(
            marker
                .confirmed
                .as_ref()
                .filter(|_| marker.state == PrWriteState::Confirmed)
                .ok_or("Actual member receipt unavailable")?,
        )?;
        if receipt.command() != command {
            return Err("Exact member receipt purpose differs".into());
        }
    } else if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain
    ) || marker.confirmed.is_some()
    {
        return Err("Possible native member impact requires exact unresolved original, never inferred invocation".into());
    }
    Ok(Some(vec![]))
}
