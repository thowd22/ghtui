//! Bounded worker reads and presentation policy. Never unlocks or sends effects.
use super::*;

const MAX_ROWS: usize = 256;
const MAX_METADATA_BYTES: i64 = 128 * 1024;

#[derive(Clone, Debug)]
pub struct ActionsUnresolvedPage {
    pub markers: Vec<ActionsWriteMarker>,
    pub total: u64,
    pub complete: bool,
}

/// A fresh observed transition says nothing about who caused it. An accepted
/// request without observable effect has a separate, explicitly labelled ACK.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ActionsInspectionDisposition {
    Pending { state: ActionsWriteState },
    TransitionObserved,
    AcceptedStillPending,
    AcceptedEffectUnverified,
}

impl ActionsInspectionDisposition {
    pub fn can_acknowledge(&self) -> bool {
        matches!(self, Self::Pending { .. } | Self::TransitionObserved)
    }
    pub fn can_acknowledge_inspected_acceptance(&self) -> bool {
        matches!(self, Self::AcceptedEffectUnverified)
    }
}

impl ActionsJournalInspection {
    /// Presentation only. The consuming journal ACK rechecks both durable and
    /// provider generations and fresh principal before it can release a lease.
    pub fn disposition(&self) -> Result<ActionsInspectionDisposition> {
        match self.marker.state {
            ActionsWriteState::Pending | ActionsWriteState::Uncertain => {
                Ok(ActionsInspectionDisposition::Pending {
                    state: self.marker.state,
                })
            }
            ActionsWriteState::AcceptedPending => {
                if require_accepted_transition(&self.marker, &self.observation).is_ok() {
                    Ok(ActionsInspectionDisposition::TransitionObserved)
                } else if require_unverified_acceptance(&self.marker, &self.observation).is_ok() {
                    Ok(ActionsInspectionDisposition::AcceptedEffectUnverified)
                } else {
                    Ok(ActionsInspectionDisposition::AcceptedStillPending)
                }
            }
            _ => bail!("This original Actions publication no longer awaits acknowledgement"),
        }
    }
}

impl ActionsWriteMarker {
    /// Private lease policy assumes an SQL resource prefilter. This public
    /// version independently applies positive scope and exact resource overlap.
    pub fn blocks_target(&self, target: &ActionsWriteTarget, kind: ActionsWriteKind) -> bool {
        if self.target.scope != target.scope
            || !scope_target::positive_canonical_scope(&target.scope)
            || !self
                .target
                .resources
                .iter()
                .any(|key| target.resources.contains(key))
        {
            return false;
        }
        self.blocks(target, kind)
    }
}

impl ActionsWriteJournal {
    /// One coherent, bounded metadata-only snapshot. A partial page is never a
    /// proof that an unseen target is unblocked; begin() remains authoritative.
    pub fn list_unresolved(&self, scope: &Scope, limit: usize) -> Result<ActionsUnresolvedPage> {
        if !(1..=MAX_ROWS).contains(&limit) {
            bail!("Actions publication list requires a limit between 1 and 256");
        }
        let provider = validate_scope(scope)?;
        let transaction = self.connection.unchecked_transaction()?;
        let total: u64 = transaction.query_row(
            "SELECT COUNT(*) FROM actions_write_attempts WHERE host=?1 AND principal=?2 AND state IN('pending','uncertain','accepted_pending')",
            params![scope.host, scope.login],
            |row| row.get(0),
        )?;
        let ids = {
            let mut statement = transaction.prepare(
                "SELECT attempt_id,length(CAST(target_json AS BLOB))+length(CAST(resources_json AS BLOB))+length(CAST(kind_json AS BLOB))+length(CAST(context_json AS BLOB))+length(CAST(options_json AS BLOB))+length(CAST(fingerprint_sha256 AS BLOB))+COALESCE(length(CAST(receipt_json AS BLOB)),0) FROM actions_write_attempts WHERE host=?1 AND principal=?2 AND state IN('pending','uncertain','accepted_pending') ORDER BY attempt_id DESC LIMIT ?3",
            )?;
            let rows = statement
                .query_map(params![scope.host, scope.login, limit as i64], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
                })?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut markers = Vec::with_capacity(ids.len());
        for (id, bytes) in ids {
            if id <= 0 || !(1..=MAX_METADATA_BYTES).contains(&bytes) {
                bail!("Original Actions publication exceeds its bounded metadata budget; protection remains");
            }
            let marker = read_attempt(&transaction, scope, id)?
                .context("Actions publication changed during its coherent snapshot")?;
            validate_saved_marker(&provider, &marker)?;
            markers.push(marker);
        }
        transaction.commit()?;
        Ok(ActionsUnresolvedPage {
            complete: markers.len() as u64 == total,
            markers,
            total,
        })
    }
}

pub(super) fn validate_scope(scope: &Scope) -> Result<GithubProvider> {
    if !scope_target::positive_canonical_scope(scope) {
        bail!("Actions publication reads require the canonical host/principal scope");
    }
    GithubProvider::new(scope.host.clone()).map_err(anyhow::Error::msg)
}

pub(super) fn validate_saved_marker(
    provider: &GithubProvider,
    marker: &ActionsWriteMarker,
) -> Result<()> {
    if provider.host() != marker.target.scope.host {
        bail!("Original Actions publication belongs to a different canonical host");
    }
    marker.validate_metadata()
}
