//! SAME journal and sole acknowledgement state transition, narrowed to one
//! exact supported Review blocker. No new ledger/table/body restoration.
use super::*;
use crate::providers::gh::ReviewRecoveryCurrentV11;
use std::{
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    time::Instant,
};

#[derive(Debug, PartialEq, Eq)]
pub(super) struct DatabaseV11 {
    path: Vec<u8>,
    device: u64,
    inode: u64,
}
impl DatabaseV11 {
    pub(super) fn capture(path: &Path) -> Result<Self> {
        let before = std::fs::symlink_metadata(path)?;
        if !before.is_file()
            || before.uid() != unsafe { libc::geteuid() }
            || before.nlink() != 1
            || before.mode() & 0o777 != 0o600
        {
            bail!("Exact original private review journal metadata unavailable");
        }
        let canonical = std::fs::canonicalize(path)?;
        let after = std::fs::symlink_metadata(&canonical)?;
        if !after.is_file()
            || after.dev() != before.dev()
            || after.ino() != before.ino()
            || after.nlink() != 1
            || after.uid() != before.uid()
            || after.mode() & 0o777 != 0o600
        {
            bail!("Original review journal changed during metadata observation");
        }
        let bytes = canonical.as_os_str().as_bytes();
        if bytes.is_empty() || bytes.len() > 4096 || bytes.contains(&0) {
            bail!("Original journal path exceeds bounds");
        }
        Ok(Self {
            path: bytes.into(),
            device: before.dev(),
            inode: before.ino(),
        })
    }
}
/// Nonserializable observation of an EXACT currently blocking reference, not
/// a publication permit. Only this journal can issue it after bounded reads.
#[derive(Debug)]
pub(crate) struct ReviewRecoveryJournalWitnessV11 {
    exact_v17: Option<super::exact_ack_v17::ExactAckRowV17>,
    database: DatabaseV11,
    issuer: PathBuf,
    target: PrWriteTarget,
    generation: i64,
    marker: PrWriteMarker,
}
impl ReviewRecoveryJournalWitnessV11 {
    pub(super) fn generation_v17(&self) -> i64 {
        self.generation
    }
    pub(crate) fn marker(&self) -> &PrWriteMarker {
        &self.marker
    }
    pub(crate) fn target(&self) -> &PrWriteTarget {
        &self.target
    }
}
fn same_marker(a: &PrWriteMarker, b: &PrWriteMarker) -> bool {
    a.publication_reference() == b.publication_reference()
        && a.target == b.target
        && a.command == b.command
        && a.selected_resource == b.selected_resource
        && a.state == b.state
        && a.confirmed == b.confirmed
        && a.created_at == b.created_at
}
impl PrWriteJournal {
    pub(crate) fn prepare_review_inspection_v11(
        &self,
        target: &PrWriteTarget,
        reference: &PrPublicationReference,
    ) -> Result<ReviewRecoveryJournalWitnessV11> {
        let database = DatabaseV11::capture(&self.path)?;
        let generation = self.source_generation(target)?;
        let marker = self
            .original_publication(target, reference)?
            .context("Exact original Review attempt unavailable")?;
        crate::pr_projection::review_recovery_v11::validate(&marker, self, target)
            .map_err(anyhow::Error::msg)?;
        let blocker = self
            .blocking(target)?
            .context("No unresolved Review attempt currently blocks this PR")?;
        if !same_marker(&blocker, &marker)
            || self.source_generation(target)? != generation
            || DatabaseV11::capture(&self.path)? != database
        {
            bail!("Original blocking Review reference changed during inspection capture");
        }
        let mut witness = ReviewRecoveryJournalWitnessV11 {
            exact_v17: None,
            database,
            issuer: self.path.clone(),
            target: target.clone(),
            generation,
            marker,
        };
        witness.exact_v17 = Some(super::exact_ack_v17::ExactAckRowV17::review(
            self, &witness,
        )?);
        self.validate_review_inspection_v11(&witness)?;
        Ok(witness)
    }
    pub(crate) fn validate_review_inspection_v11(
        &self,
        witness: &ReviewRecoveryJournalWitnessV11,
    ) -> Result<()> {
        if self.path != witness.issuer
            || DatabaseV11::capture(&self.path)? != witness.database
            || self.source_generation(&witness.target)? != witness.generation
        {
            bail!("Original journal/database/generation changed; inspect again explicitly");
        }
        let marker = self
            .original_publication(&witness.target, &witness.marker.publication_reference())?
            .context("Original blocking Review record unavailable")?;
        crate::pr_projection::review_recovery_v11::validate(&marker, self, &witness.target)
            .map_err(anyhow::Error::msg)?;
        let blocker = self
            .blocking(&witness.target)?
            .context("Original Review record is no longer the current blocker")?;
        if !same_marker(&marker, &witness.marker)
            || !same_marker(&blocker, &witness.marker)
            || self.source_generation(&witness.target)? != witness.generation
            || DatabaseV11::capture(&self.path)? != witness.database
        {
            bail!("Exact original Review reference/purpose/state changed; no blocker released");
        }
        if let Some(row) = &witness.exact_v17 {
            row.validate_current(self)?;
        }
        Ok(())
    }
    /// Explicit local acknowledgement only. Provider failure changes no SQL.
    /// SQL failure AFTER runner reconciliation leaves the durable blocker; no
    /// resend or automatic acknowledgement follows either partial outcome.
    pub(crate) fn acknowledge_review_inspection_v11(
        &mut self,
        provider: &GithubProvider,
        mut witness: ReviewRecoveryJournalWitnessV11,
        observation: ReviewRecoveryCurrentV11,
        current: &HostIdentity,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<usize> {
        if cancel.is_cancelled() || Instant::now() >= deadline {
            bail!("Original Review acknowledgement phase ended");
        }
        let exact = witness
            .exact_v17
            .take()
            .context("Original exact Review ACK row was not captured before final UID")?;
        exact.validate_current(self)?;
        if Scope::github(&current.host, current.account_id)? != *witness.target.scope()
            || current.host != observation.identity().host
            || current.account_id != observation.identity().account_id
            || !observation.matches_target(&witness.target)
        {
            bail!("Actual current numeric account differs from original Review inspection");
        }
        // SAME runner resource state, original generation/token/deadline.
        provider
            .reconcile_review_inspection_v11(observation, cancel, deadline)
            .map_err(anyhow::Error::msg)?;
        if cancel.is_cancelled() || Instant::now() >= deadline {
            bail!("Acknowledgement ended after runner reconciliation; durable blocker retained");
        }
        self.commit_exact_ack_v17(exact, cancel, deadline)
    }
}
