use super::{
    invalidate_confirmed_metadata, PrMetadataPlan, PrMetadataReceipt, PrMetadataStructuredImpact,
};
use crate::{
    pr_projection::invalidate_confirmed_discussions,
    pr_write_journal::{
        PrPublicationReference, PrWriteJournal, PrWriteMarker, PrWriteState, PrWriteTarget,
    },
    providers::{
        gh::{GithubProvider, HostIdentity, PrMetadataReadLimits, PrSummary},
        runner::CancellationToken,
    },
};

pub enum PrMetadataExecution {
    Refused {
        error: String,
    },
    /// A typed fresh provider failure cannot authorize any previously displayed
    /// private target, even when the final numeric principal is unchanged.
    RefusedPrivate {
        error: String,
    },
    RestoredAlreadyConfirmed {
        marker: PrWriteMarker,
    },
    Uncertain {
        publication: PrWriteMarker,
        error: String,
    },
    Confirmed {
        receipt: PrMetadataReceipt,
        publication: PrWriteMarker,
        private_visible: bool,
        impact: PrMetadataStructuredImpact,
    },
    ConfirmedButJournalFailed {
        receipt: PrMetadataReceipt,
        publication: PrWriteMarker,
        error: String,
    },
    ConfirmedButRefreshFailed {
        receipt: PrMetadataReceipt,
        publication: PrWriteMarker,
        error: String,
        private_visible: bool,
    },
}
/// No view loading or resend is performed by this core. The Root worker uses
/// this typed result to replace its prepared view and retain original reference.
pub struct PrMetadataRefresh {
    pub private_visible: bool,
}
pub struct PrMetadataExecutionContext<'a> {
    pub provider: &'a GithubProvider,
    pub journal: &'a mut PrWriteJournal,
    pub confirmed: bool,
    pub restored: Option<&'a PrPublicationReference>,
    pub limits: &'a PrMetadataReadLimits,
    pub cancel: &'a CancellationToken,
}

/// Original durable reference lookup must run BEFORE new candidate/body/head
/// validation or any transport. The UI must call this when opening a restored
/// original; a receipt is inspection, not authorization for edited new intent.
pub fn restore_original(
    journal: &PrWriteJournal,
    identity: &HostIdentity,
    original: &PrSummary,
    reference: &PrPublicationReference,
) -> Result<Option<PrWriteMarker>, String> {
    let target = PrWriteTarget::new(identity, original).map_err(|e| e.to_string())?;
    let marker = journal
        .original_publication(&target, reference)
        .map_err(|e| e.to_string())?;
    if let Some(marker) = &marker {
        if marker.state != PrWriteState::Confirmed {
            return Ok(None);
        }
        validate_recorded_marker(marker)?;
        // Original reference recovery never reloads a source or resends. Repair
        // durable label visibility before presenting private original metadata.
        invalidate_confirmed_metadata(journal.database_path(), identity, original, reference)?;
    }
    Ok(marker)
}
/// One closed policy for exact original recovery and the explicit
/// no-discussion-text-impact activation hook; no arbitrary command prefixes.
pub fn validate_recorded_marker(marker: &PrWriteMarker) -> Result<(), String> {
    if marker.state != PrWriteState::Confirmed {
        return Err("No confirmed typed metadata receipt".into());
    }
    let decoded = marker
        .confirmed
        .as_ref()
        .ok_or("Original confirmed publication has no typed receipt")
        .and_then(|receipt| {
            PrMetadataReceipt::decode(receipt)
                .map_err(|_| "Original confirmed receipt has invalid typed metadata shape")
        })?;
    if decoded.command().key() != marker.command
        || marker.selected_resource
            != format!(
                "{}:{}",
                decoded.command().field(),
                decoded.selected_id().unwrap_or(0)
            )
        || marker.confirmed.as_ref().and_then(|r| r.node_id.as_deref())
            != Some(marker.target.node_id())
    {
        return Err("Original publication belongs to another operation; inspect it without reloading or resending edited fields".into());
    }
    Ok(())
}

impl PrMetadataPlan {
    pub fn execute(
        &self,
        context: PrMetadataExecutionContext<'_>,
        mut persist: impl FnMut(&PrWriteMarker) -> Result<(), String>,
        mut refresh: impl FnMut(
            &PrMetadataReceipt,
            &PrMetadataStructuredImpact,
        ) -> Result<PrMetadataRefresh, String>,
    ) -> PrMetadataExecution {
        let PrMetadataExecutionContext {
            provider,
            journal,
            confirmed,
            restored,
            limits,
            cancel,
        } = context;
        let refused = |error: String| PrMetadataExecution::Refused { error };
        // This path intentionally ignores current edited values/head and does
        // not create a fresh session or recover approval from a saved draft.
        if let Some(reference) = restored {
            match restore_original(journal,self.identity(),self.original(),reference) {
                Ok(Some(marker))=>return PrMetadataExecution::RestoredAlreadyConfirmed{marker},
                Ok(None)=>return refused("Original referenced publication is not confirmed; inspect its durable state before a new intent, never resend automatically".into()),
                Err(error)=>return refused(error),
            }
        }
        if !confirmed || cancel.is_cancelled() {
            return refused("Fresh explicit effect confirmation is required; draft restoration does not approve publication".into());
        }
        match journal.blocking(self.target()) {
            Ok(Some(_))=>return refused("An original Pending/Uncertain PR publication blocks this write; inspect and explicitly acknowledge it".into()),
            Err(error)=>return refused(error.to_string()),Ok(None)=>{}
        }
        let mut session = match provider.prepare_metadata_publication(self, limits, cancel) {
            Ok(session) => session,
            Err(error)
                if error.repository_access_lost()
                    || matches!(
                        error.kind,
                        crate::providers::gh::GithubErrorKind::Authentication
                            | crate::providers::gh::GithubErrorKind::IdentityChanged
                    ) =>
            {
                return PrMetadataExecution::RefusedPrivate {
                    error: error.to_string(),
                }
            }
            Err(error) => return refused(error.to_string()),
        };
        let permit = match journal.begin(self.target(), self.intent()) {
            Ok(permit) => permit,
            Err(error) => return refused(error.to_string()),
        };
        let marker = permit.marker().clone();
        if let Err(error) = persist(&marker) {
            return refused(format!("Original Pending marker was retained, but its ordered draft/reference flush failed; no child write: {error}"));
        }
        if let Err(error) = session.ready(self, cancel) {
            return refused(format!(
                "Original Pending marker retained; shared budget expired before claim: {error}"
            ));
        }
        if let Err(error) = journal.claim(&permit, self.target(), self.intent()) {
            return refused(error.to_string());
        }
        let published = match session.publish(provider, self, cancel) {
            Ok(proof) => proof,
            Err(error) => {
                let saved = journal.mark_uncertain(&permit);
                let diagnostic=match saved { Ok(())=>error.to_string(),Err(saved)=>format!("{error}; durable uncertainty update failed, original Pending barrier retained: {saved}") };
                let impact = super::invalidation::invalidate_uncertain_metadata(
                    journal.database_path(),
                    self,
                    &marker.publication_reference(),
                );
                let diagnostic=match impact {Ok(())=>diagnostic,Err(failure)=>format!("{diagnostic}; structured filter visibility could not be fenced; old metadata/Dashboard must stay hidden: {failure}")};
                return PrMetadataExecution::Uncertain {
                    publication: marker,
                    error: diagnostic,
                };
            }
        };
        if !published.matches(self) {
            let _ = journal.mark_uncertain(&permit);
            return PrMetadataExecution::Uncertain {
                publication: marker,
                error: "Actual metadata confirmation belongs to another immutable original request"
                    .into(),
            };
        }
        let receipt = match self.receipt() {
            Ok(receipt) => receipt,
            Err(error) => {
                let _ = journal.mark_uncertain(&permit);
                return PrMetadataExecution::Uncertain {
                    publication: marker,
                    error,
                };
            }
        };
        // The exact actual-ID receipt is saved BEFORE any optional view read.
        if let Err(error) = journal.mark_confirmed(&permit, &receipt.effect()) {
            return PrMetadataExecution::ConfirmedButJournalFailed { receipt,publication:marker,error:format!("Exact effect was observed but its receipt could not be saved; original barrier retained, no resend: {error}") };
        }
        let publication = match journal
            .original_publication(self.target(), &marker.publication_reference())
        {
            Ok(Some(publication)) if publication.state == PrWriteState::Confirmed => publication,
            Ok(_) => {
                return PrMetadataExecution::ConfirmedButJournalFailed {
                    receipt,
                    publication: marker,
                    error: "Saved original confirmation cannot be recaptured; no resend".into(),
                }
            }
            Err(error) => {
                return PrMetadataExecution::ConfirmedButJournalFailed {
                    receipt,
                    publication: marker,
                    error: error.to_string(),
                }
            }
        };
        if let Err(error) = invalidate_confirmed_discussions(
            journal.database_path(),
            published.identity(),
            self.original(),
            &publication.publication_reference(),
        ) {
            return PrMetadataExecution::ConfirmedButRefreshFailed {
                receipt,
                publication,
                error: error.to_string(),
                private_visible: false,
            };
        }
        let impact = match invalidate_confirmed_metadata(
            journal.database_path(),
            published.identity(),
            self.original(),
            &publication.publication_reference(),
        ) {
            Ok(impact) => impact,
            Err(error) => {
                return PrMetadataExecution::ConfirmedButRefreshFailed {
                    receipt,
                    publication,
                    error,
                    private_visible: false,
                }
            }
        };
        match refresh(&receipt, &impact) {
            Ok(state) => PrMetadataExecution::Confirmed {
                receipt,
                publication,
                private_visible: state.private_visible,
                impact,
            },
            Err(error) => PrMetadataExecution::ConfirmedButRefreshFailed {
                receipt,
                publication,
                error,
                private_visible: false,
            },
        }
    }
}
