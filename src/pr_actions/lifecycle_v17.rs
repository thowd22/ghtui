//! The SAME durable pending -> persisted/ACK -> claim transition for legacy
//! PrPlan and closed Original mode. Provider/source checks stay purpose typed.
use super::*;
use crate::pr_write_journal::PrWritePermit;

impl PrPlan {
    pub(crate) fn claim_persisted_v17<T>(
        journal: &mut PrWriteJournal,
        permit: &PrWritePermit,
        target: &PrWriteTarget,
        intent: &PrWriteIntent,
        persist: impl FnOnce(&PrWriteMarker) -> Result<T, String>,
    ) -> Result<T, String> {
        // begin has already committed the original reference. Even a failed
        // save or unknown ACK leaves that exact Pending barrier, never a retry.
        let persisted = persist(permit.marker()).map_err(|error| format!(
            "Publication metadata was not acknowledged. No GitHub write was sent; original pending reference remains protected. {error}"))?;
        journal
            .claim(permit, target, intent)
            .map_err(|error| error.to_string())?;
        Ok(persisted)
    }
}
