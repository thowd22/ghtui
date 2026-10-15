//! Closed local-recovery purpose validation, never a new effect router or a
//! general acknowledgement allowlist. Existing text impact helpers unchanged.
use crate::pr_write_journal::{PrWriteJournal, PrWriteMarker, PrWriteState, PrWriteTarget};
pub(crate) fn shape(marker: &PrWriteMarker, confirmed: bool) -> Result<(), String> {
    match marker.command.as_str() {
        "review-create-pending-v2"
        | "review-update-pending-v2"
        | "review-submit-v2"
        | "review-discard-v2"
        | "review-dismiss-v2"
        | "review-pending-comment-add-v3"
        | "review-pending-comment-edit-v3"
        | "review-pending-comment-delete-v3" => {
            super::review_impact_v2::categories(marker, confirmed)?
                .ok_or("Closed original Review selector required")?;
        }
        "review-rerequest-v3" => {
            super::review_rerequest_impact_v3::categories(marker, confirmed)?
                .ok_or("Closed original reviewer-request selector required")?;
        }
        _ => return Err("Unsupported original Review recovery purpose".into()),
    }
    Ok(())
}
pub(crate) fn validate(
    marker: &PrWriteMarker,
    journal: &PrWriteJournal,
    target: &PrWriteTarget,
) -> Result<(), String> {
    if !matches!(
        marker.state,
        PrWriteState::Pending | PrWriteState::Uncertain
    ) || marker.confirmed.is_some()
        || &marker.target != target
    {
        return Err("Choose the exact currently blocking unresolved Review attempt".into());
    }
    shape(marker, false)?;
    match marker.command.as_str(){
        "review-create-pending-v2"|"review-update-pending-v2"|"review-submit-v2"|"review-discard-v2"|"review-dismiss-v2"=>{
            super::review_impact_v2::categories(marker,false)?.ok_or("Closed original Review selector required")?;
        },
        "review-pending-comment-add-v3"|"review-pending-comment-edit-v3"|"review-pending-comment-delete-v3"=>{
            super::review_operation_impact_v3::categories(marker,false)?.ok_or("Closed pending-comment purpose required")?;
            super::review_operation_impact_v3::validate_original(marker,journal,target)?;
        },
        "review-rerequest-v3"=>{
            super::review_rerequest_impact_v3::categories(marker,false)?.ok_or("Closed reviewer-request purpose required")?;
            journal.review_observed_origin_v12(target,&marker.publication_reference()).map_err(|error|error.to_string())?
                .ok_or("Known reviewer-request purpose lacks its actual atomic original observation")?;
        },
        _=>return Err("This attempt is not a supported exact Review recovery purpose; no foreign blocker released".into()),
    }
    Ok(())
}
