//! NEW undeclared private child replacing old copying implementations, NOT a
//! second graph model. All retained commit bodies remain in actual Git pages.
use super::compact_v2::GraphTopologyV2;
use super::restore_v2::GraphMemoryChargeV2;
use super::*;
use std::mem::size_of;

impl OriginalGraphStateV1 {
    pub(crate) fn start(
        origin: Arc<OriginalLocalActorOriginV1>,
        page: Arc<OriginalGraphPageV1>,
    ) -> Result<Arc<Self>, String> {
        Self::from_retained_pages_v2(origin, Arc::from([page]))
    }
    pub(crate) fn append(
        old: &Arc<Self>,
        page: Arc<OriginalGraphPageV1>,
    ) -> Result<Arc<Self>, String> {
        if !old.last_page().has_more()
            || page.offset() != old.loaded()
            || page.walk_tip() != old.walk_tip()
            || page.walk_fingerprint() != old.last_page().walk_fingerprint()
            || page.epoch() != old.last_page().epoch()
            || !page.matches_source(&old.origin)
        {
            return Err("Original Graph Arc/cursor/epoch/source changed; no append/rebase".into());
        }
        // Fifty tiny Arc pointers maximum. No old topology/body clones.
        let mut pages = Vec::with_capacity(old.pages.len() + 1);
        pages.extend(old.pages.iter().cloned());
        pages.push(page);
        Self::from_retained_pages_v2(old.origin.clone(), Arc::from(pages))
    }
    pub(crate) fn from_retained_pages_v2(
        origin: Arc<OriginalLocalActorOriginV1>,
        pages: Arc<[Arc<OriginalGraphPageV1>]>,
    ) -> Result<Arc<Self>, String> {
        let first = pages
            .first()
            .ok_or("Original Graph has no genuine first page")?;
        if first.offset() != 0
            || pages.len() > MAX_NODES.div_ceil(200)
            || pages.iter().any(|page| !page.matches_source(&origin))
        {
            return Err("Original Graph first page/source mismatch".into());
        }
        let (bodies, shared_charges_v2) = retained_page_bytes_v2(&origin, &pages)?;
        let topology = GraphTopologyV2::from_pages(
            &pages,
            MAX_RETAINED
                .checked_sub(bodies)
                .ok_or("Graph raw metadata cannot fit before any old model/body clone")?,
        )?;
        let retained_bytes = bodies
            .checked_add(topology.bytes())
            .filter(|n| *n <= MAX_RETAINED)
            .ok_or("Graph retained raw+numeric metadata exceeds unchanged8MiB")?;
        let last = pages.last().expect("genuine first page");
        let coverage = if last.capped() {
            GraphCoverageV1::Capped
        } else if last.has_more() {
            GraphCoverageV1::MoreAvailable
        } else if topology.frontier().is_empty() {
            GraphCoverageV1::AvailableWalkExhausted
        } else if pages.iter().flat_map(|p| p.commits()).any(|row| {
            last.epoch().is_shallow_commit(&row.commit().id) && !row.commit().parents.is_empty()
        }) {
            GraphCoverageV1::ShallowBoundary
        } else {
            GraphCoverageV1::UnavailableParents
        };
        let state = Arc::new(Self {
            origin,
            pages,
            topology,
            retained_bytes,
            coverage,
            shared_charges_v2,
            snapshot_v2: std::sync::OnceLock::new(),
        });
        // Mint eagerly in the worker; foreground getters are only Arc clones.
        let _ = state.snapshot_v2();
        Ok(state)
    }
    pub(crate) fn loaded_member_v2(&self, id: &str) -> Option<compact_v2::GraphMemberV2> {
        self.topology.member(id)
    }
    /// Lane rows `start..end` of the loaded walk (TASK-99.1). The layout always
    /// runs from the first loaded commit, so columns and colors of a row never
    /// depend on the window or on how many pages are loaded.
    pub(crate) fn lane_rows_v991(
        &self,
        start: usize,
        end: usize,
    ) -> Result<Vec<crate::history::lanes::GraphRow>, String> {
        use crate::history::lanes::{LaneCommit, LaneLayout, LaneOptions, HISTORY_LANE_PALETTE};
        let end = end.min(self.loaded());
        let epoch = self.last_page().epoch();
        let mut layout = LaneLayout::new(LaneOptions {
            palette: HISTORY_LANE_PALETTE,
            max_columns: HISTORY_VISIBLE_LANES_V991,
            max_lanes: MAX_LANES,
        });
        let mut rows = Vec::with_capacity(end.saturating_sub(start));
        for (index, node) in self.topology.rows()[..end].iter().enumerate() {
            let commit = self
                .topology
                .commit(&self.pages, node.member)
                .ok_or("Compact graph actual member lost")?;
            let lane = LaneCommit::new(
                commit.id.as_str(),
                commit.parents.iter().map(String::as_str),
            )
            .with_boundary(epoch.is_shallow_commit(&commit.id));
            let row = layout.push(&lane)?;
            if index >= start {
                rows.push(row);
            }
        }
        Ok(rows)
    }
    pub(crate) fn member_at_v2(&self, index: usize) -> Option<&Commit> {
        let member = self.topology.rows().get(index)?.member;
        self.topology.commit(&self.pages, member)
    }
    #[cfg(test)]
    pub(crate) fn retained_bytes_v2(&self) -> usize {
        self.retained_bytes
    }
    /// Tiny numeric interpretation only; used by exact code assertions and
    /// renderer preparation, never by foreground string/body reconstruction.
    #[cfg(test)]
    pub(crate) fn topology_v2(&self) -> &GraphTopologyV2 {
        &self.topology
    }
}
fn retained_page_bytes_v2(
    origin: &Arc<OriginalLocalActorOriginV1>,
    pages: &Arc<[Arc<OriginalGraphPageV1>]>,
) -> Result<(usize, Arc<[GraphMemoryChargeV2]>), String> {
    let mut components = HashMap::<(u8, usize), usize>::with_capacity(pages.len() * 5);
    for page in pages.iter() {
        for GraphMemoryChargeV2 {
            kind,
            pointer,
            bytes,
        } in page.shared_memory_charges_v2()?
        {
            if let Some(old) = components.insert((kind, pointer), bytes) {
                if old != bytes {
                    return Err("Actual immutable graph Arc memory changed".into());
                }
            }
        }
    }
    components.insert(
        (6, pages.as_ptr() as usize),
        pages.len() * size_of::<Arc<OriginalGraphPageV1>>() + 32,
    );
    components.insert(
        (8, Arc::as_ptr(origin) as usize),
        origin.retained_graph_memory_bytes_v2() + 32,
    );
    let bytes = components
        .values()
        .try_fold(
            size_of::<OriginalGraphStateV1>()
                + components.len() * size_of::<GraphMemoryChargeV2>()
                + 256,
            |sum, bytes| sum.checked_add(*bytes).filter(|n| *n <= MAX_RETAINED),
        )
        .ok_or("Graph raw metadata cannot fit before any old model/body clone")?;
    let charges = components
        .into_iter()
        .map(|((kind, pointer), bytes)| GraphMemoryChargeV2 {
            kind,
            pointer,
            bytes,
        })
        .collect::<Vec<_>>();
    Ok((bytes, Arc::from(charges)))
}
