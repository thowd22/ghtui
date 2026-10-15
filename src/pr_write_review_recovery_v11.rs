//! SAME journal and sole acknowledgement state transition, narrowed to one
//! exact supported Review blocker. No new ledger/table/body restoration.
use super::*;
use crate::providers::gh::ReviewRecoveryCurrentV11;
use std::{
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
    time::Instant,
};

#[derive(Debug, PartialEq, Eq)]
struct DatabaseV11 {
    path: Vec<u8>,
    device: u64,
    inode: u64,
}
impl DatabaseV11 {
    fn capture(path: &Path) -> Result<Self> {
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
    database: DatabaseV11,
    issuer: PathBuf,
    target: PrWriteTarget,
    generation: i64,
    marker: PrWriteMarker,
}
impl ReviewRecoveryJournalWitnessV11 {
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
        Ok(ReviewRecoveryJournalWitnessV11 {
            database,
            issuer: self.path.clone(),
            target: target.clone(),
            generation,
            marker,
        })
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
        Ok(())
    }
    /// Explicit local acknowledgement only. Provider failure changes no SQL.
    /// SQL failure AFTER runner reconciliation leaves the durable blocker; no
    /// resend or automatic acknowledgement follows either partial outcome.
    pub(crate) fn acknowledge_review_inspection_v11(
        &mut self,
        provider: &GithubProvider,
        witness: ReviewRecoveryJournalWitnessV11,
        observation: ReviewRecoveryCurrentV11,
        current: &HostIdentity,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<usize> {
        if cancel.is_cancelled() || Instant::now() >= deadline {
            bail!("Original Review acknowledgement phase ended");
        }
        self.validate_review_inspection_v11(&witness)?;
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
        self.validate_review_inspection_v11(&witness)?;
        self.connection.busy_timeout(
            deadline
                .saturating_duration_since(Instant::now())
                .min(Duration::from_secs(2)),
        )?;
        let transaction = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        if generation(&transaction, &witness.target)? != witness.generation {
            bail!("A newer Review write retired this inspection");
        }
        let changed=transaction.execute("UPDATE pr_write_attempts SET state='acknowledged' WHERE attempt_id=?1 AND host=?2 AND principal=?3 AND pr_node=?4 AND command=?5 AND selected_resource=?6 AND observed_head=?7 AND payload_sha256=?8 AND state=?9 AND confirmed_json IS NULL",
            params![witness.marker.attempt_id,witness.target.scope.host,witness.target.scope.login,witness.target.pr_node_id,
                witness.marker.command,witness.marker.selected_resource,witness.marker.observed_head,witness.marker.payload_sha256,state_name(witness.marker.state)])?;
        if changed != 1 {
            bail!("Exact blocking Review attempt changed; no newer record acknowledged");
        }
        if cancel.is_cancelled()
            || Instant::now() >= deadline
            || DatabaseV11::capture(&self.path)? != witness.database
        {
            bail!("Original acknowledgement clock/database ended; SQL transaction rolled back");
        }
        advance(&transaction, &witness.target)?;
        transaction.commit()?;
        Ok(changed)
    }
}
