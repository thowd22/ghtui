//! Closed two-selected-marker recovery, not a journal history enumerator or
//! source/access constructor. The original composite is NEVER split.
use super::*;
use std::{fs, os::unix::fs::MetadataExt};

#[derive(Debug)]
pub(super) struct ReviewRepairSelectionV8 {
    target: PrWriteTarget,
    generation: i64,
    database: PathBuf,
    device: u64,
    inode: u64,
    review: PrWriteMarker,
    inline: PrWriteMarker,
}
fn same_marker(a: &PrWriteMarker, b: &PrWriteMarker) -> bool {
    a.target == b.target
        && a.publication_reference() == b.publication_reference()
        && a.command == b.command
        && a.selected_resource == b.selected_resource
        && a.state == b.state
        && a.confirmed == b.confirmed
        && a.created_at == b.created_at
}
fn selected(
    journal: &PrWriteJournal,
    target: &PrWriteTarget,
    kind: SyncBindingKind,
) -> Result<PrWriteMarker, String> {
    let marker = journal
        .latest_source_receipt(target, kind)
        .map_err(|error| error.to_string())?
        .ok_or("Original composite selected receipt disappeared; private text remains hidden")?;
    require_source_marker(target, &marker)?;
    super::super::review_operation_impact_v3::validate_original(&marker, journal, target)?;
    let categories = impact_categories(&marker)?;
    if !super::super::review_impact_v2::matches_source(&marker, true, kind, &categories)? {
        return Err("Selected original receipt no longer has its exact closed category set".into());
    }
    Ok(marker)
}
impl ReviewRepairSelectionV8 {
    /// None only for a NON-composite effect; its existing closed router still
    /// owns validation. No generic contains(), retry or inferred old receipt.
    pub(super) fn prepare(
        journal: &PrWriteJournal,
        target: &PrWriteTarget,
        marker: &PrWriteMarker,
        categories: &[SyncBindingKind],
    ) -> Result<Option<(Self, Vec<PrWriteMarker>)>, String> {
        let both = [SyncBindingKind::Review, SyncBindingKind::InlineComment];
        let base = super::super::metadata_base_impact_v32::categories(marker, true)?;
        let review = super::super::review_impact_v2::categories(marker, true)?;
        if base.as_deref() != Some(both.as_slice()) && review.as_deref() != Some(both.as_slice()) {
            return Ok(None);
        }
        // A pure closed category is not proof of an actual atomic origin.
        super::super::metadata_base_impact_v32::validate_original(marker, journal, target)?;
        if categories != both {
            return Err("Original review composite category set changed".into());
        }
        let generation = journal
            .source_generation(target)
            .map_err(|error| error.to_string())?;
        let metadata =
            fs::symlink_metadata(journal.database_path()).map_err(|error| error.to_string())?;
        if !private_file(&metadata) {
            return Err("Original composite database is not one regular owned0600 file".into());
        }
        let review = selected(journal, target, SyncBindingKind::Review)?;
        let inline = selected(journal, target, SyncBindingKind::InlineComment)?;
        let repair = if same_marker(&review, &inline) {
            if !same_marker(marker, &review) {
                return Err(
                    "Original composite is no longer the current exact selected pair".into(),
                );
            }
            vec![review.clone()]
        } else {
            let (historical, current, current_kind) = if same_marker(marker, &review) {
                (&review, &inline, SyncBindingKind::InlineComment)
            } else if same_marker(marker, &inline) {
                (&inline, &review, SyncBindingKind::Review)
            } else {
                return Err(
                    "Original composite is not either actual current selected receipt".into(),
                );
            };
            // The two actual selected records alone justify this bounded
            // dependency. Never enumerate/guess an intermediate old history.
            if historical.attempt_id >= current.attempt_id
                || impact_categories(historical)? != both
                || impact_categories(current)? != [current_kind]
            {
                return Err("Selected history is not original composite then a newer closed single-category effect".into());
            }
            vec![historical.clone(), current.clone()]
        };
        let selection = Self {
            target: target.clone(),
            generation,
            database: journal.database_path().to_owned(),
            device: metadata.dev(),
            inode: metadata.ino(),
            review,
            inline,
        };
        selection.validate(journal.database_path(), journal)?;
        Ok(Some((selection, repair)))
    }
    pub(super) fn same_selection(&self, other: &Self) -> bool {
        self.target == other.target
            && self.generation == other.generation
            && self.database == other.database
            && self.device == other.device
            && self.inode == other.inode
            && same_marker(&self.review, &other.review)
            && same_marker(&self.inline, &other.inline)
    }
    pub(super) fn validate(&self, database: &Path, journal: &PrWriteJournal) -> Result<(), String> {
        let path = fs::canonicalize(database).map_err(|error| error.to_string())?;
        let metadata = fs::symlink_metadata(database).map_err(|error| error.to_string())?;
        if path != self.database
            || journal.database_path() != self.database
            || !private_file(&metadata)
            || metadata.dev() != self.device
            || metadata.ino() != self.inode
            || journal
                .source_generation(&self.target)
                .map_err(|error| error.to_string())?
                != self.generation
        {
            return Err("Original selected composite database/source generation changed; private results remain hidden".into());
        }
        let review = selected(journal, &self.target, SyncBindingKind::Review)?;
        let inline = selected(journal, &self.target, SyncBindingKind::InlineComment)?;
        if !same_marker(&self.review, &review) || !same_marker(&self.inline, &inline) {
            return Err("Exact selected Review/Inline original references changed during recovery; private results remain hidden".into());
        }
        Ok(())
    }
}
fn private_file(metadata: &fs::Metadata) -> bool {
    // SAFETY: read-only current Unix process owner, not a GitHub principal.
    metadata.file_type().is_file()
        && metadata.nlink() == 1
        && metadata.uid() == unsafe { libc::geteuid() }
        && metadata.mode() & 0o777 == 0o600
}
#[cfg(test)]
#[path = "review_repair_selection_v8_tests.rs"]
mod tests;
