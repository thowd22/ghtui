//! NEW UNDECLARED child of the SAME original GraphState. Numeric topology
//! borrows actual opaque pages; no Commit/message/string-lane body clones.
use super::*;
use std::mem::size_of;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct GraphOidV2 {
    bytes: [u8; 32],
    length: u8,
}
impl GraphOidV2 {
    fn parse(id: &str) -> Result<Self, String> {
        if !crate::git::original_actor_saved_sha(id) {
            return Err("Compact original graph requires genuine full40/64 saved IDs".into());
        }
        let mut bytes = [0; 32];
        for (n, pair) in id.as_bytes().chunks_exact(2).enumerate() {
            let digit = |v: u8| if v <= b'9' { v - b'0' } else { v - b'a' + 10 };
            bytes[n] = (digit(pair[0]) << 4) | digit(pair[1]);
        }
        Ok(Self {
            bytes,
            length: id.len() as u8,
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub(crate) struct GraphMemberV2 {
    pub page: u16,
    pub row: u16,
    pub node: u32,
}
#[derive(Clone, Copy, Debug)]
struct GraphWitnessV2 {
    page: u16,
    row: u16,
    parent: Option<u8>,
}
#[derive(Clone, Copy, Debug)]
struct GraphOidInfoV2 {
    witness: GraphWitnessV2,
    member: Option<GraphMemberV2>,
    color: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GraphNodeV2 {
    pub member: GraphMemberV2,
    pub oid: u32,
    pub column: u8,
    edges: u32,
    edges_len: u8,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GraphEdgeV2 {
    pub parent: u32,
    pub order: u8,
    pub from: u8,
    pub to: u8,
    pub color: u8,
}
pub(crate) struct GraphTopologyV2 {
    index: HashMap<GraphOidV2, u32>,
    identities: Box<[GraphOidInfoV2]>,
    nodes: Box<[GraphNodeV2]>,
    edges: Box<[GraphEdgeV2]>,
    frontier: Box<[u32]>,
    retained: usize,
}
impl std::fmt::Debug for GraphTopologyV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphTopologyV2")
            .field("nodes", &self.nodes.len())
            .field("frontier", &self.frontier.len())
            .field("numeric_bytes", &self.retained)
            .finish()
    }
}
impl GraphTopologyV2 {
    pub(crate) fn from_pages(
        pages: &[Arc<OriginalGraphPageV1>],
        byte_limit: usize,
    ) -> Result<Self, String> {
        let total = pages
            .iter()
            .try_fold(0usize, |sum, p| sum.checked_add(p.commits().len()))
            .filter(|n| *n <= MAX_NODES)
            .ok_or("Compact graph node count exceeds10k")?;
        if pages.is_empty() {
            return Err("Compact graph has no genuine original page".into());
        }
        let mut index = HashMap::with_capacity(total.saturating_add(MAX_LANES));
        let mut identities = Vec::<GraphOidInfoV2>::with_capacity(total.saturating_add(MAX_LANES));
        let mut nodes = Vec::with_capacity(total);
        let mut edges = Vec::new();
        let mut active = Vec::<u32>::new();
        let mut next_color = 0u8;
        let mut expected = 0usize;
        for (page_index, page) in pages.iter().enumerate() {
            if page.offset() != expected
                || page.walk_fingerprint() != pages[0].walk_fingerprint()
                || page.epoch() != pages[0].epoch()
                || page_index > 0 && !pages[page_index - 1].has_more()
            {
                return Err("Compact graph page/cursor/epoch identity differs; no rebase".into());
            }
            for (row_index, observed) in page.commits().iter().enumerate() {
                let commit = observed.commit();
                let member = GraphMemberV2 {
                    page: u16::try_from(page_index).map_err(|_| "Graph page index overflow")?,
                    row: u16::try_from(row_index).map_err(|_| "Graph member index overflow")?,
                    node: u32::try_from(nodes.len()).map_err(|_| "Graph node index overflow")?,
                };
                let witness = GraphWitnessV2 {
                    page: member.page,
                    row: member.row,
                    parent: None,
                };
                let oid = intern(
                    &mut index,
                    &mut identities,
                    &commit.id,
                    witness,
                    &mut next_color,
                    None,
                )?;
                if identities[oid as usize].member.is_some() {
                    return Err("Compact graph duplicate actual member".into());
                }
                let column = if let Some(column) = active.iter().position(|old| *old == oid) {
                    column
                } else {
                    if active.len() == MAX_LANES {
                        return Err("Original graph frontier exceeds128 numeric paths".into());
                    }
                    active.push(oid);
                    active.len() - 1
                };
                let color = identities[oid as usize].color;
                active.remove(column);
                identities[oid as usize].member = Some(member);
                let mut parents = Vec::with_capacity(commit.parents.len());
                for (order, parent) in commit.parents.iter().enumerate() {
                    let witness = GraphWitnessV2 {
                        page: member.page,
                        row: member.row,
                        parent: Some(
                            u8::try_from(order).map_err(|_| "Graph parent index exceeds64")?,
                        ),
                    };
                    let parent = intern(
                        &mut index,
                        &mut identities,
                        parent,
                        witness,
                        &mut next_color,
                        (order == 0).then_some(color),
                    )?;
                    if identities[parent as usize].member.is_some() {
                        return Err("Raw original parent was already emitted; no invented upward/shallow edge".into());
                    }
                    if !active.contains(&parent) {
                        if active.len() == MAX_LANES {
                            return Err("Original graph frontier exceeds128; no path loss".into());
                        }
                        if order == 0 {
                            active.insert(column.min(active.len()), parent);
                        } else {
                            active.push(parent);
                        }
                    }
                    parents.push(parent);
                }
                let edge_start =
                    u32::try_from(edges.len()).map_err(|_| "Graph edge index overflow")?;
                for (order, parent) in parents.into_iter().enumerate() {
                    let to = active
                        .iter()
                        .position(|value| *value == parent)
                        .ok_or("Original ordered parent lane unavailable")?;
                    edges.push(GraphEdgeV2 {
                        parent,
                        order: order as u8,
                        from: column as u8,
                        to: to as u8,
                        color: identities[parent as usize].color,
                    });
                }
                nodes.push(GraphNodeV2 {
                    member,
                    oid,
                    column: column as u8,
                    edges: edge_start,
                    edges_len: commit.parents.len() as u8,
                });
                expected += 1;
                let charged = topology_charge(
                    &index,
                    identities.len(),
                    nodes.len(),
                    edges.len(),
                    active.len(),
                )?;
                if charged > byte_limit {
                    return Err("Compact numeric graph exceeds unchanged8MiB active charge before projection".into());
                }
            }
            if page.next_offset() != expected {
                return Err("Compact page member accounting differs from actual cursor".into());
            }
        }
        let retained = topology_charge(
            &index,
            identities.len(),
            nodes.len(),
            edges.len(),
            active.len(),
        )?;
        Ok(Self {
            index,
            identities: identities.into_boxed_slice(),
            nodes: nodes.into_boxed_slice(),
            edges: edges.into_boxed_slice(),
            frontier: active.into_boxed_slice(),
            retained,
        })
    }
    pub(crate) fn bytes(&self) -> usize {
        self.retained
    }
    pub(crate) fn rows(&self) -> &[GraphNodeV2] {
        &self.nodes
    }
    pub(crate) fn frontier(&self) -> &[u32] {
        &self.frontier
    }
    pub(crate) fn member(&self, id: &str) -> Option<GraphMemberV2> {
        self.index
            .get(&GraphOidV2::parse(id).ok()?)
            .and_then(|index| self.identities[*index as usize].member)
    }
    pub(crate) fn commit<'a>(
        &self,
        pages: &'a [Arc<OriginalGraphPageV1>],
        member: GraphMemberV2,
    ) -> Option<&'a Commit> {
        self.observed(pages, member)
            .map(OriginalGraphCommitV1::commit)
    }
    pub(crate) fn observed<'a>(
        &self,
        pages: &'a [Arc<OriginalGraphPageV1>],
        member: GraphMemberV2,
    ) -> Option<&'a OriginalGraphCommitV1> {
        pages
            .get(usize::from(member.page))?
            .commits()
            .get(usize::from(member.row))
    }
    pub(crate) fn id<'a>(
        &self,
        pages: &'a [Arc<OriginalGraphPageV1>],
        oid: u32,
    ) -> Option<&'a str> {
        let witness = self.identities.get(oid as usize)?.witness;
        let commit = pages
            .get(usize::from(witness.page))?
            .commits()
            .get(usize::from(witness.row))?
            .commit();
        witness.parent.map_or(Some(commit.id.as_str()), |parent| {
            commit.parents.get(usize::from(parent)).map(String::as_str)
        })
    }
    pub(crate) fn edges(&self, node: &GraphNodeV2) -> &[GraphEdgeV2] {
        &self.edges[node.edges as usize..node.edges as usize + usize::from(node.edges_len)]
    }
    pub(crate) fn ancestors(&self, id: &str) -> Result<Vec<bool>, String> {
        let member = self
            .member(id)
            .ok_or("Original selected compact member unavailable")?;
        let mut result = vec![false; self.nodes.len()];
        let mut pending = vec![member.node];
        while let Some(index) = pending.pop() {
            if result[index as usize] {
                continue;
            }
            result[index as usize] = true;
            for edge in self.edges(&self.nodes[index as usize]) {
                if let Some(parent) = self.identities[edge.parent as usize].member {
                    pending.push(parent.node);
                }
            }
        }
        Ok(result)
    }
}
fn intern(
    index: &mut HashMap<GraphOidV2, u32>,
    ids: &mut Vec<GraphOidInfoV2>,
    raw: &str,
    witness: GraphWitnessV2,
    next: &mut u8,
    inherited: Option<u8>,
) -> Result<u32, String> {
    let key = GraphOidV2::parse(raw)?;
    if let Some(id) = index.get(&key) {
        return Ok(*id);
    }
    if ids.len() >= MAX_NODES + MAX_LANES {
        return Err("Compact original IDs exceed nodes+frontier bound".into());
    }
    let id = ids.len() as u32;
    let color = if let Some(color) = inherited {
        color
    } else {
        let color = *next;
        *next = (*next + 1) % 8;
        color
    };
    ids.push(GraphOidInfoV2 {
        witness,
        member: None,
        color,
    });
    index.insert(key, id);
    Ok(id)
}
fn topology_charge(
    index: &HashMap<GraphOidV2, u32>,
    ids: usize,
    nodes: usize,
    edges: usize,
    frontier: usize,
) -> Result<usize, String> {
    // No object/string bodies here. Use current actual hash capacity plus
    // conservative bucket/control margin; boxed arrays drop Vec spare capacity.
    let pieces = [
        index
            .capacity()
            .checked_mul(size_of::<(GraphOidV2, u32)>() + 8),
        ids.checked_mul(size_of::<GraphOidInfoV2>()),
        nodes.checked_mul(size_of::<GraphNodeV2>()),
        edges.checked_mul(size_of::<GraphEdgeV2>()),
        frontier.checked_mul(size_of::<u32>()),
        Some(512),
    ];
    pieces
        .into_iter()
        .try_fold(0usize, |sum, value| {
            sum.checked_add(value?).filter(|n| *n <= MAX_RETAINED)
        })
        .ok_or_else(|| "Compact graph metadata size overflow".to_owned())
}

#[cfg(test)]
#[path = "repository_graph_compact_v2_tests.rs"]
mod tests;
