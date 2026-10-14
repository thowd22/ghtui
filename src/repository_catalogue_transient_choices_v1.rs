//! Child of CatalogueWorkspace. Small event snapshots contain ONLY
//! intent IDs/pin deltas/query + an original Read Arc, never observed rows or
//! permissions. Genuine identity mapping is bounded WORKER work before I/O.
use super::{check_query, CatalogueWorkspace, MAX_ROWS};
use crate::{
    providers::{gh::RepositoryCatalogueRead, runner::CancellationToken},
    repository_catalogue_intent_v1::IntentSource,
    storage::Scope,
};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_CHOICES: usize = 4096;
const MAX_CHOICE_BYTES: usize = 128 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnresolvedCatalogueChoiceReasonV1 {
    NotObserved,
    OriginalNodeUnavailable,
    CurrentNodeUnavailable,
    NodeChanged,
}
impl UnresolvedCatalogueChoiceReasonV1 {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotObserved => "Not observed in this bounded read; absence is not deletion",
            Self::OriginalNodeUnavailable => {
                "Original opaque node unavailable; identity not guessed"
            }
            Self::CurrentNodeUnavailable => {
                "Current opaque node unavailable; old choice not restored"
            }
            Self::NodeChanged => {
                "Numeric ID now has a different opaque node; old intent not retargeted"
            }
        }
    }
}
#[derive(Serialize)]
struct ChoiceReferenceV1 {
    id: u64,
    node: Option<String>,
    selected: bool,
    pin: Option<bool>,
}
pub struct UnresolvedCatalogueChoiceV1 {
    original: ChoiceReferenceV1,
    reason: UnresolvedCatalogueChoiceReasonV1,
}
impl UnresolvedCatalogueChoiceV1 {
    pub fn id(&self) -> u64 {
        self.original.id
    }
    pub fn selected(&self) -> bool {
        self.original.selected
    }
    pub fn pin_delta(&self) -> Option<bool> {
        self.original.pin
    }
    pub fn reason(&self) -> UnresolvedCatalogueChoiceReasonV1 {
        self.reason
    }
}
impl std::fmt::Debug for UnresolvedCatalogueChoiceV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UnresolvedCatalogueChoiceV1")
            .field("id", &self.id())
            .field("reason", &self.reason)
            .finish_non_exhaustive()
    }
}

pub struct CatalogueTransientChoiceSnapshotV1 {
    original: Arc<RepositoryCatalogueRead>,
    selected: Arc<[u64]>,
    pins: Arc<[(u64, bool)]>,
    query: String,
    inherited: Arc<[UnresolvedCatalogueChoiceV1]>,
}
impl std::fmt::Debug for CatalogueTransientChoiceSnapshotV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CatalogueTransientChoiceSnapshotV1(<bounded local intent only>)")
    }
}
pub struct PreparedCatalogueTransientChoicesV1 {
    original: Arc<RepositoryCatalogueRead>,
    query: String,
    references: Arc<[ChoiceReferenceV1]>,
}
impl std::fmt::Debug for PreparedCatalogueTransientChoicesV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedCatalogueTransientChoicesV1(<bounded navigation intent only>)")
    }
}
pub struct RestoredCatalogueTransientChoicesV1 {
    source: Arc<RepositoryCatalogueRead>,
    selected: Vec<u64>,
    pins: Vec<(u64, bool)>,
    unresolved: Arc<[UnresolvedCatalogueChoiceV1]>,
    query: String,
}
#[derive(Serialize)]
struct BorrowChoiceV1<'a> {
    id: u64,
    node: Option<&'a str>,
    selected: bool,
    pin: Option<bool>,
}
#[derive(Serialize)]
struct BorrowIntentV1<'a> {
    query: &'a str,
    choices: &'a [BorrowChoiceV1<'a>],
}
#[derive(Serialize)]
struct BorrowEventIntentV1<'a> {
    selected: &'a BTreeSet<u64>,
    pins: &'a BTreeMap<u64, bool>,
    query: &'a str,
    inherited: &'a [BorrowChoiceV1<'a>],
}
struct ChoiceByteCounterV1(usize);
impl Write for ChoiceByteCounterV1 {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_CHOICE_BYTES.saturating_sub(self.0) {
            return Err(io::Error::other("bounded transient choice intent"));
        }
        self.0 += bytes.len();
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
fn same_source(old: &RepositoryCatalogueRead, new: &RepositoryCatalogueRead) -> Result<(), String> {
    let scope = |read: &RepositoryCatalogueRead| {
        Scope::github(&read.identity().host, read.identity().account_id)
            .map_err(|_| "Original positive choice scope unavailable".to_owned())
    };
    if old.identity().account_id.is_none_or(|uid| uid == 0)
        || new.identity().account_id.is_none_or(|uid| uid == 0)
        || scope(old)? != scope(new)?
        || IntentSource::from_original(old.context())?
            != IntentSource::from_original(new.context())?
    {
        return Err("Transient choices belong to a different original numeric account/owner/source; no restore".into());
    }
    Ok(())
}
fn check_worker(started: Instant, cancel: &CancellationToken) -> Result<(), String> {
    if cancel.is_cancelled() || started.elapsed() > Duration::from_secs(2) {
        Err(
            "Original transient choice preparation cancelled/expired; no partial intent or default"
                .into(),
        )
    } else {
        Ok(())
    }
}
impl CatalogueWorkspace {
    /// Bounded event work: <=4096 intent references and512-byte query only. No
    /// iteration over up to100k rows; no node/title/permission copies or I/O.
    pub fn snapshot_transient_choices(
        &self,
    ) -> Result<Arc<CatalogueTransientChoiceSnapshotV1>, String> {
        let original = self.original_source()?;
        check_query(self.query())?;
        if self.selected.len() > MAX_CHOICES
            || self.pin_changes.len() > MAX_CHOICES
            || self.unresolved_choices.len() > MAX_CHOICES
        {
            return Err("Transient choice reference cap exceeded before snapshot copies".into());
        }
        let mut ids = BTreeSet::new();
        for id in self
            .selected
            .iter()
            .copied()
            .chain(self.pin_changes.keys().copied())
            .chain(
                self.unresolved_choices
                    .iter()
                    .map(UnresolvedCatalogueChoiceV1::id),
            )
        {
            if id == 0 || (ids.insert(id) && ids.len() > MAX_CHOICES) {
                return Err("Transient choice union exceeds4096 positive IDs; no crop".into());
            }
        }
        let inherited = self
            .unresolved_choices
            .iter()
            .map(|choice| BorrowChoiceV1 {
                id: choice.id(),
                node: choice.original.node.as_deref(),
                selected: choice.selected(),
                pin: choice.pin_delta(),
            })
            .collect::<Vec<_>>();
        serde_json::to_writer(
            ChoiceByteCounterV1(0),
            &BorrowEventIntentV1 {
                selected: &self.selected,
                pins: &self.pin_changes,
                query: self.query(),
                inherited: &inherited,
            },
        )
        .map_err(|_| "Transient event intent exceeds128KiB before snapshot copies".to_owned())?;
        Ok(Arc::new(CatalogueTransientChoiceSnapshotV1 {
            original,
            selected: self.selected.iter().copied().collect::<Vec<_>>().into(),
            pins: self
                .pin_changes
                .iter()
                .map(|(id, pin)| (*id, *pin))
                .collect::<Vec<_>>()
                .into(),
            query: self.query().to_owned(),
            inherited: self.unresolved_choices.clone(),
        }))
    }
    pub fn unresolved_choice_count(&self) -> usize {
        self.unresolved_choices.len()
    }
    /// Explicit LOCAL discard only. Never saved payload, ghost pins, source
    /// generation, remote scope, acknowledgement, activation or provider call.
    pub fn discard_unresolved_choices(&mut self) {
        self.unresolved_choices = Arc::from([]);
        self.clear_layout();
    }
    /// Bounded prepared text, only after genuine same-scope fresh read. Missing
    /// intent is visible; no need to copy old repository rows/actors/permissions.
    pub fn unresolved_choices_notice(&self) -> String {
        let mut lines = vec![
            format!(
                "Unresolved local choices: {} • not saved or activated",
                self.unresolved_choices.len()
            ),
            "Re-read may recover exact original IDs/nodes; absence is not deletion.".into(),
            "Discard missing choices clears ONLY unsaved intent; durable sets/pins stay intact."
                .into(),
        ];
        for choice in self.unresolved_choices.iter().take(64) {
            let pin = match choice.pin_delta() {
                Some(true) => "Pin requested",
                Some(false) => "Unpin requested",
                None => "No pin change",
            };
            lines.push(format!(
                "ID {} • selected {} • {} • {}",
                choice.id(),
                choice.selected(),
                pin,
                choice.reason().label()
            ));
        }
        if self.unresolved_choices.len() > 64 {
            lines.push(format!(
                "{} additional unresolved intents retained, not displayed in this bounded notice",
                self.unresolved_choices.len() - 64
            ));
        }
        lines.join("\n")
    }
}
impl CatalogueTransientChoiceSnapshotV1 {
    pub fn original(&self) -> &Arc<RepositoryCatalogueRead> {
        &self.original
    }
    /// WORKER preflight BEFORE provider/cache/reservation. Walk original once,
    /// index only wanted IDs; bound serialized raw nodes BEFORE owned copies.
    pub fn prepare(
        &self,
        cancel: &CancellationToken,
    ) -> Result<PreparedCatalogueTransientChoicesV1, String> {
        let started = Instant::now();
        check_worker(started, cancel)?;
        check_query(&self.query)?;
        if self.original.repositories().len() > MAX_ROWS {
            return Err("Original choice observation exceeds100k row cap; no shortcut".into());
        }
        let wanted: BTreeSet<_> = self
            .selected
            .iter()
            .copied()
            .chain(self.pins.iter().map(|(id, _)| *id))
            .collect();
        let mut old = BTreeMap::new();
        for (index, row) in self.original.repositories().iter().enumerate() {
            if index % 128 == 0 {
                check_worker(started, cancel)?;
            }
            if wanted.contains(&row.id) {
                old.insert(row.id, row.node_id.as_deref());
            }
        }
        let pins: BTreeMap<_, _> = self.pins.iter().copied().collect();
        let selected: BTreeSet<_> = self.selected.iter().copied().collect();
        let mut refs = BTreeMap::<u64, BorrowChoiceV1<'_>>::new();
        for inherited in self.inherited.iter() {
            refs.insert(
                inherited.id(),
                BorrowChoiceV1 {
                    id: inherited.id(),
                    node: inherited.original.node.as_deref(),
                    selected: inherited.selected(),
                    pin: inherited.pin_delta(),
                },
            );
        }
        for id in wanted {
            let node = *old
                .get(&id)
                .ok_or("Selected intent lacks an original genuine row; no guessed node")?;
            if let Some(prior) = refs.get_mut(&id) {
                if prior.node != node
                    || pins
                        .get(&id)
                        .is_some_and(|pin| prior.pin.is_some_and(|old| *pin != old))
                {
                    return Err("New choice conflicts with unresolved original intent; explicitly discard missing choices first".into());
                }
                prior.selected |= selected.contains(&id);
                if let Some(pin) = pins.get(&id) {
                    prior.pin = Some(*pin);
                }
            } else {
                refs.insert(
                    id,
                    BorrowChoiceV1 {
                        id,
                        node,
                        selected: selected.contains(&id),
                        pin: pins.get(&id).copied(),
                    },
                );
            }
        }
        if refs.len() > MAX_CHOICES {
            return Err("Transient original choice union exceeds4096; no crop".into());
        }
        let borrowed = refs.into_values().collect::<Vec<_>>();
        serde_json::to_writer(
            ChoiceByteCounterV1(0),
            &BorrowIntentV1 {
                query: &self.query,
                choices: &borrowed,
            },
        )
        .map_err(|_| "Transient original intent exceeds128KiB before node copies".to_owned())?;
        check_worker(started, cancel)?;
        let references = borrowed
            .into_iter()
            .map(|reference| ChoiceReferenceV1 {
                id: reference.id,
                node: reference.node.map(str::to_owned),
                selected: reference.selected,
                pin: reference.pin,
            })
            .collect::<Vec<_>>()
            .into();
        Ok(PreparedCatalogueTransientChoicesV1 {
            original: self.original.clone(),
            query: self.query.clone(),
            references,
        })
    }
}
impl PreparedCatalogueTransientChoicesV1 {
    pub fn original(&self) -> &Arc<RepositoryCatalogueRead> {
        &self.original
    }
    pub fn restore(
        &self,
        source: Arc<RepositoryCatalogueRead>,
        cancel: &CancellationToken,
    ) -> Result<RestoredCatalogueTransientChoicesV1, String> {
        same_source(&self.original, &source)?;
        let started = Instant::now();
        check_worker(started, cancel)?;
        if source.repositories().len() > MAX_ROWS {
            return Err("Fresh transient mapping exceeds100k rows; no crop".into());
        }
        let wanted: BTreeSet<_> = self
            .references
            .iter()
            .map(|reference| reference.id)
            .collect();
        let mut fresh = BTreeMap::new();
        for (index, row) in source.repositories().iter().enumerate() {
            if index % 128 == 0 {
                check_worker(started, cancel)?;
            }
            if wanted.contains(&row.id) {
                fresh.insert(row.id, row.node_id.as_deref());
            }
        }
        let mut selected = Vec::new();
        let mut pins = Vec::new();
        let mut unresolved = Vec::new();
        for original in self.references.iter() {
            let reason = match (original.node.as_deref(), fresh.get(&original.id)) {
                (None, _) => Some(UnresolvedCatalogueChoiceReasonV1::OriginalNodeUnavailable),
                (_, None) => Some(UnresolvedCatalogueChoiceReasonV1::NotObserved),
                (Some(_), Some(None)) => {
                    Some(UnresolvedCatalogueChoiceReasonV1::CurrentNodeUnavailable)
                }
                (Some(old), Some(Some(current))) if old != *current => {
                    Some(UnresolvedCatalogueChoiceReasonV1::NodeChanged)
                }
                _ => None,
            };
            if let Some(reason) = reason {
                unresolved.push(UnresolvedCatalogueChoiceV1 {
                    original: ChoiceReferenceV1 {
                        id: original.id,
                        node: original.node.clone(),
                        selected: original.selected,
                        pin: original.pin,
                    },
                    reason,
                });
            } else {
                if original.selected {
                    selected.push(original.id);
                }
                if let Some(pin) = original.pin {
                    pins.push((original.id, pin));
                }
            }
        }
        check_worker(started, cancel)?;
        Ok(RestoredCatalogueTransientChoicesV1 {
            source,
            selected,
            pins,
            unresolved: unresolved.into(),
            query: self.query.clone(),
        })
    }
}
impl RestoredCatalogueTransientChoicesV1 {
    pub fn query(&self) -> &str {
        &self.query
    }
    pub fn unresolved(&self) -> &[UnresolvedCatalogueChoiceV1] {
        &self.unresolved
    }
    /// WORKER installation after restore_observed_pins(new genuine intent).
    /// Never carry complete rows, permissions, active scope or old row indices.
    pub fn install(self, workspace: &mut CatalogueWorkspace) -> Result<(), String> {
        if workspace
            .source()
            .is_none_or(|actual| !Arc::ptr_eq(actual, &self.source))
        {
            return Err(
                "Restored choices belong to a different fresh Read Arc; no publication".into(),
            );
        }
        workspace.selected = self.selected.into_iter().collect();
        workspace.pin_changes = self.pins.into_iter().collect();
        for (id, pin) in &workspace.pin_changes {
            if *pin {
                workspace.pins.insert(*id);
            } else {
                workspace.pins.remove(id);
            }
        }
        workspace.unresolved_choices = self.unresolved;
        workspace.choices_dirty = !workspace.selected.is_empty()
            || !workspace.pin_changes.is_empty()
            || !workspace.unresolved_choices.is_empty();
        workspace.clear_layout();
        Ok(())
    }
}

#[cfg(test)]
#[path = "repository_catalogue_transient_choices_v1_tests.rs"]
mod tests;
