//! NEW undeclared child of the SAME Graph state. Retained immutable pages are
//! local continuity evidence, not GitHub access, a read clock or approval.
use super::*;
#[path = "repository_graph_retention_v3.rs"]
mod retention_v3;
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;
pub(crate) use retention_v3::GraphVisitMetadataV3;
use sha2::{Digest, Sha256};
use std::{
    mem::size_of,
    sync::{Mutex, Weak},
};

pub(crate) const GRAPH_VISIT_BYTES_V2: usize = 16 * 1024 * 1024;
pub(crate) const GRAPH_VISITS_V2: usize = 64;
// Shared navigation contracts bound each original recipe/bookmark to16KiB.
// Reserve that metadata per source BEFORE any visit/position copies; actual
// pending immutable position objects below are additionally charged on lease.
const VISIT_METADATA_RESERVATION: usize = 16 * 1024;
const MAX_PENDING_POSITIONS: usize = 256;
const MAX_CHARGES: usize = 300;
const MAX_COMPONENTS: usize = GRAPH_VISITS_V2 * MAX_CHARGES;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GraphMemoryChargeV2 {
    pub kind: u8,
    pub pointer: usize,
    pub bytes: usize,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ComponentKey {
    kind: u8,
    pointer: usize,
}
#[derive(Clone, Copy, Debug)]
struct RetainedComponent {
    bytes: usize,
    leases: usize,
}

/// Bounded POSITION only. No serialized position can construct a snapshot.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct GraphPositionV2 {
    pub(crate) member: compact_v2::GraphMemberV2,
    pub(crate) selected: String,
    pub(crate) view: GraphViewV1,
    pub(crate) projection_start: usize,
    pub(crate) horizontal: usize,
    pub(crate) comparison_parent: Option<usize>,
}
impl GraphPositionV2 {
    pub(crate) fn navigation_metadata_bytes_v2(&self) -> Result<usize, String> {
        if !crate::git::original_actor_saved_sha(&self.selected)
            || self.member.page >= 50
            || self.member.row >= 200
            || self.member.node >= 10_000
            || self.projection_start >= 10_000
            || !self.projection_start.is_multiple_of(200)
            || self.horizontal > 2048
            || self.comparison_parent.is_some_and(|n| n >= 64)
            || (self.view == GraphViewV1::Comparison) != self.comparison_parent.is_some()
        {
            return Err("Graph navigation position exceeds bounded metadata".into());
        }
        Ok(self.retained_bytes())
    }
    pub(crate) fn retained_bytes(&self) -> usize {
        size_of::<Self>() + self.selected.capacity()
    }
}

/// Minted only by a genuine immutable Graph state. Contains no legacy graph,
/// prepared row bodies, Workspace, report, token, ledger or completed clock.
pub(crate) struct GraphSnapshotV2 {
    origin: Arc<OriginalLocalActorOriginV1>,
    pages: Arc<[Arc<OriginalGraphPageV1>]>,
    components: Arc<[GraphMemoryChargeV2]>,
    source_key: [u8; 32],
    loaded: usize,
}
impl std::fmt::Debug for GraphSnapshotV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphSnapshotV2")
            .field("loaded", &self.loaded)
            .field("pages", &self.pages.len())
            .finish()
    }
}
impl GraphSnapshotV2 {
    fn mint(state: &OriginalGraphStateV1) -> Self {
        let mut hash = Sha256::new();
        hash.update((Arc::as_ptr(&state.origin) as usize).to_be_bytes());
        hash.update((state.pages.as_ptr() as usize).to_be_bytes());
        hash.update(state.walk_tip().as_bytes());
        hash.update(state.last_page().walk_fingerprint());
        hash.update(state.loaded().to_be_bytes());
        hash.update(state.last_page().epoch().fingerprint());
        Self {
            origin: state.origin.clone(),
            pages: state.pages.clone(),
            components: state.shared_charges_v2.clone(),
            source_key: hash.finalize().into(),
            loaded: state.loaded(),
        }
    }
    pub(crate) fn source_key(&self) -> [u8; 32] {
        self.source_key
    }
    pub(crate) fn loaded(&self) -> usize {
        self.loaded
    }
    pub(crate) fn walk_tip(&self) -> &str {
        self.pages[0].walk_tip()
    }
    pub(crate) fn origin(&self) -> &Arc<OriginalLocalActorOriginV1> {
        &self.origin
    }
    pub(crate) fn same_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.origin, &other.origin)
            && Arc::ptr_eq(&self.pages, &other.pages)
            && self.source_key == other.source_key
            && self.loaded == other.loaded
    }
    pub(crate) fn validate_position(&self, position: &GraphPositionV2) -> Result<(), String> {
        if !crate::git::original_actor_saved_sha(self.walk_tip())
            || !crate::git::original_actor_saved_sha(&position.selected)
            || position.horizontal > 2048
            || position.member.node as usize >= self.loaded()
            || !position
                .projection_start
                .is_multiple_of(super::projection_v2::GRAPH_PROJECTION_COMMITS_V2)
            || position.projection_start >= self.loaded
            || position.comparison_parent.is_some_and(|n| n >= 64)
        {
            return Err("Graph position exceeds its genuine snapshot bounds".into());
        }
        let page = self
            .pages
            .get(usize::from(position.member.page))
            .ok_or("Graph position page is not retained")?;
        let observed = page
            .commits()
            .get(usize::from(position.member.row))
            .ok_or("Graph position member is not retained")?;
        if page.offset() + usize::from(position.member.row) != position.member.node as usize
            || observed.commit().id != position.selected
            || (position.member.node as usize) < position.projection_start
            || position.member.node as usize
                >= position
                    .projection_start
                    .saturating_add(super::projection_v2::GRAPH_PROJECTION_COMMITS_V2)
            || position
                .comparison_parent
                .is_some_and(|n| n >= observed.commit().parents.len())
            || (position.view == GraphViewV1::Comparison) != position.comparison_parent.is_some()
        {
            return Err("Graph position is not the exact original page/member/view".into());
        }
        Ok(())
    }
    fn charges(self: &Arc<Self>) -> Result<Vec<GraphMemoryChargeV2>, String> {
        // Capacities/body lengths were observed ONCE in the worker. Event-side
        // admission touches at most300 tiny component records, never10k rows.
        let mut charges = self.components.to_vec();
        charges.push(GraphMemoryChargeV2 {
            kind: 7,
            pointer: Arc::as_ptr(self) as usize,
            bytes: size_of::<Self>() + 32,
        });
        charges.push(GraphMemoryChargeV2 {
            kind: 9,
            pointer: self.components.as_ptr() as usize,
            bytes: self.components.len() * size_of::<GraphMemoryChargeV2>() + 32,
        });
        if charges.len() > MAX_CHARGES
            || charges
                .iter()
                .any(|c| c.pointer == 0 || c.bytes == 0 || c.bytes > GRAPH_VISIT_BYTES_V2)
        {
            return Err("Graph immutable component accounting exceeds original bounds".into());
        }
        Ok(charges)
    }
    /// NEW original operation supplied by the existing queue. Exactly selected
    /// raw header/roles/parents and FS/shallow continuity before and after pure
    /// preparation; no reread from page1, HEAD rebase or revived pool.
    pub(crate) fn restore(
        self: &Arc<Self>,
        position: &GraphPositionV2,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<RestoredGraphV2, String> {
        self.validate_position(position)?;
        operation.check()?;
        self.origin
            .with_saved_navigation_read_v1(operation, |handle| {
                let page = &self.pages[usize::from(position.member.page)];
                let member = &page.commits()[usize::from(position.member.row)];
                handle
                    .original_graph_member_v1(page, member, operation.deadline())
                    .map_err(|e| e.to_string())?;
                let state = OriginalGraphStateV1::from_retained_pages_v2(
                    self.origin.clone(),
                    self.pages.clone(),
                )?;
                let actual = state.snapshot_v2();
                if !actual.same_source(self) {
                    return Err("Restored graph source changed".into());
                }
                actual.validate_position(position)?;
                let presentation = GraphPresentationV1::prepare_window_v2(
                    &state,
                    &position.selected,
                    if position.view == GraphViewV1::Comparison {
                        GraphViewV1::Parents
                    } else {
                        position.view
                    },
                    position.projection_start,
                )?;
                handle
                    .validate_graph_epoch_v1(state.last_page(), operation.deadline())
                    .map_err(|e| e.to_string())?;
                operation.check()?;
                Ok(RestoredGraphV2 {
                    state,
                    presentation,
                    position: position.clone(),
                })
            })
    }
}
pub(crate) struct RestoredGraphV2 {
    pub state: Arc<OriginalGraphStateV1>,
    pub presentation: Arc<GraphPresentationV1>,
    pub position: GraphPositionV2,
}
impl std::fmt::Debug for RestoredGraphV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RestoredGraphV2")
            .field("state", &self.state)
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}
impl OriginalGraphStateV1 {
    pub(crate) fn snapshot_v2(&self) -> Arc<GraphSnapshotV2> {
        self.snapshot_v2
            .get_or_init(|| Arc::new(GraphSnapshotV2::mint(self)))
            .clone()
    }
    pub(crate) fn position_v2(
        &self,
        selected: &str,
        view: GraphViewV1,
        projection_start: usize,
        horizontal: usize,
        comparison_parent: Option<usize>,
    ) -> Result<GraphPositionV2, String> {
        let member = self
            .topology
            .member(selected)
            .ok_or("Graph position selected member absent")?;
        let position = GraphPositionV2 {
            member,
            selected: selected.into(),
            view,
            projection_start,
            horizontal,
            comparison_parent,
        };
        self.snapshot_v2().validate_position(&position)?;
        Ok(position)
    }
}

struct GraphLedgerV2 {
    components: HashMap<ComponentKey, RetainedComponent>,
    leases: HashMap<[u8; 32], Weak<GraphLeaseV2>>,
    bytes: usize,
    positions: usize,
}
/// Bounds only. Preallocated metadata is charged even when no pages are held.
/// Arc clones of a lease do not double-charge, pending workers hold the SAME
/// lease, and only last actual Drop releases an immutable component.
pub(crate) struct GraphRetentionV2 {
    ledger: Arc<Mutex<GraphLedgerV2>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GraphRetentionAccountingV2 {
    pub bytes: usize,
    pub sources: usize,
    pub components: usize,
    pub positions: usize,
}
impl Default for GraphRetentionV2 {
    fn default() -> Self {
        Self::new()
    }
}
impl GraphRetentionV2 {
    pub(crate) fn new() -> Self {
        let components = HashMap::with_capacity(MAX_COMPONENTS);
        let leases = HashMap::with_capacity(GRAPH_VISITS_V2);
        let bytes = components.capacity() * (size_of::<(ComponentKey, RetainedComponent)>() + 8)
            + leases.capacity() * (size_of::<([u8; 32], Weak<GraphLeaseV2>)>() + 8)
            + size_of::<GraphLedgerV2>()
            + size_of::<Self>()
            + 128;
        assert!(
            bytes < GRAPH_VISIT_BYTES_V2,
            "Fixed metadata fits bounded registry"
        );
        Self {
            ledger: Arc::new(Mutex::new(GraphLedgerV2 {
                components,
                leases,
                bytes,
                positions: 0,
            })),
        }
    }
    pub(crate) fn accounting(&self) -> Result<GraphRetentionAccountingV2, String> {
        let ledger = self
            .ledger
            .lock()
            .map_err(|_| "Graph retention lock unavailable")?;
        Ok(GraphRetentionAccountingV2 {
            bytes: ledger.bytes,
            sources: ledger.leases.len(),
            components: ledger.components.len(),
            positions: ledger.positions,
        })
    }
    pub(crate) fn admit(
        &self,
        snapshot: Arc<GraphSnapshotV2>,
    ) -> Result<Arc<GraphLeaseV2>, String> {
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| "Graph retention lock unavailable")?;
        if let Some(existing) = ledger
            .leases
            .get(&snapshot.source_key)
            .and_then(Weak::upgrade)
        {
            let matches = existing.snapshot.same_source(&snapshot);
            drop(ledger);
            if !matches {
                return Err("Graph source key cannot substitute equal text/different pages".into());
            }
            return Ok(existing);
        }
        if ledger.leases.len() == GRAPH_VISITS_V2 {
            return Err(
                "Graph Back limit64 sources reached • Discard graph Back before leaving".into(),
            );
        }
        let mut charges = snapshot.charges()?;
        charges.sort_unstable_by_key(|c| (c.kind, c.pointer));
        for pair in charges.windows(2) {
            if pair[0].kind == pair[1].kind
                && pair[0].pointer == pair[1].pointer
                && pair[0].bytes != pair[1].bytes
            {
                return Err("Same immutable Arc has contradictory memory accounting".into());
            }
        }
        charges.dedup_by_key(|c| (c.kind, c.pointer));
        let private_bytes = size_of::<GraphLeaseV2>()
            + 32
            + VISIT_METADATA_RESERVATION
            + charges.capacity() * size_of::<GraphMemoryChargeV2>();
        let mut added = private_bytes;
        let mut new_components = 0usize;
        for charge in &charges {
            let key = ComponentKey {
                kind: charge.kind,
                pointer: charge.pointer,
            };
            if let Some(existing) = ledger.components.get(&key) {
                if existing.bytes != charge.bytes {
                    return Err("Retained actual Arc size changed".into());
                }
            } else {
                added = added
                    .checked_add(charge.bytes)
                    .ok_or("Graph retention byte overflow")?;
                new_components += 1;
            }
        }
        let next = ledger
            .bytes
            .checked_add(added)
            .filter(|b| *b <= GRAPH_VISIT_BYTES_V2)
            .ok_or("Graph Back limit16MiB reached • Discard graph Back before leaving")?;
        if ledger.components.len() + new_components > MAX_COMPONENTS {
            return Err("Graph retention component cap reached before leaving".into());
        }
        for charge in &charges {
            let entry = ledger
                .components
                .entry(ComponentKey {
                    kind: charge.kind,
                    pointer: charge.pointer,
                })
                .or_insert(RetainedComponent {
                    bytes: charge.bytes,
                    leases: 0,
                });
            entry.leases += 1;
        }
        ledger.bytes = next;
        let lease = Arc::new(GraphLeaseV2 {
            snapshot,
            charges,
            private_bytes,
            ledger: self.ledger.clone(),
        });
        ledger
            .leases
            .insert(lease.snapshot.source_key, Arc::downgrade(&lease));
        Ok(lease)
    }
}
pub(crate) struct GraphLeaseV2 {
    snapshot: Arc<GraphSnapshotV2>,
    charges: Vec<GraphMemoryChargeV2>,
    private_bytes: usize,
    ledger: Arc<Mutex<GraphLedgerV2>>,
}
impl GraphLeaseV2 {
    pub(crate) fn snapshot(&self) -> &Arc<GraphSnapshotV2> {
        &self.snapshot
    }
    pub(crate) fn validate_position(&self, position: &GraphPositionV2) -> Result<(), String> {
        self.snapshot.validate_position(position)
    }
    pub(crate) fn position(
        self: &Arc<Self>,
        position: &GraphPositionV2,
    ) -> Result<Arc<GraphPositionLeaseV2>, String> {
        self.validate_position(position)?;
        let bytes = size_of::<GraphPositionLeaseV2>() + position.retained_bytes() + 32;
        let mut ledger = self
            .ledger
            .lock()
            .map_err(|_| "Graph retention lock unavailable")?;
        let next = ledger
            .bytes
            .checked_add(bytes)
            .filter(|n| *n <= GRAPH_VISIT_BYTES_V2)
            .ok_or("Graph pending position exceeds16MiB retention before leaving")?;
        if ledger.positions >= MAX_PENDING_POSITIONS {
            return Err("Graph pending position cap reached before leaving".into());
        }
        ledger.bytes = next;
        ledger.positions += 1;
        Ok(Arc::new(GraphPositionLeaseV2 {
            source: self.clone(),
            position: position.clone(),
            bytes,
        }))
    }
}
pub(crate) struct GraphPositionLeaseV2 {
    source: Arc<GraphLeaseV2>,
    position: GraphPositionV2,
    bytes: usize,
}
impl GraphPositionLeaseV2 {
    pub(crate) fn source(&self) -> &Arc<GraphLeaseV2> {
        &self.source
    }
    pub(crate) fn position(&self) -> &GraphPositionV2 {
        &self.position
    }
}
impl std::fmt::Debug for GraphPositionLeaseV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphPositionLeaseV2")
            .field("position", &self.position)
            .finish_non_exhaustive()
    }
}
impl Drop for GraphPositionLeaseV2 {
    fn drop(&mut self) {
        let mut ledger = self.source.ledger.lock().unwrap_or_else(|p| p.into_inner());
        ledger.positions -= 1;
        ledger.bytes = ledger.bytes.saturating_sub(self.bytes);
    }
}
impl std::fmt::Debug for GraphLeaseV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphLeaseV2")
            .field("loaded", &self.snapshot.loaded)
            .finish()
    }
}
impl Drop for GraphLeaseV2 {
    fn drop(&mut self) {
        let mut ledger = self
            .ledger
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        ledger.leases.remove(&self.snapshot.source_key);
        ledger.bytes = ledger.bytes.saturating_sub(self.private_bytes);
        for charge in &self.charges {
            let key = ComponentKey {
                kind: charge.kind,
                pointer: charge.pointer,
            };
            if let Some(entry) = ledger.components.get_mut(&key) {
                entry.leases -= 1;
                if entry.leases == 0 {
                    ledger.components.remove(&key);
                    ledger.bytes = ledger.bytes.saturating_sub(charge.bytes);
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "repository_graph_restore_v2_tests.rs"]
mod tests;
