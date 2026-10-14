//! NEW undeclared child of original_graph_page_v1. Allocation accounting
//! only; actual immutable Arcs identify ownership, never access authority.
use super::*;
use crate::repository_graph_v1::restore_v2::GraphMemoryChargeV2;
use std::mem::size_of;
impl OriginalGraphPageV1 {
    pub(crate) fn shared_memory_charges_v2(
        self: &Arc<Self>,
    ) -> Result<Vec<GraphMemoryChargeV2>, String> {
        let mut commit_bytes = size_of::<OriginalGraphCommitV1>()
            .checked_mul(self.commits.len())
            .ok_or("Original page byte charge overflow")?;
        for observed in self.commits.iter() {
            let c = observed.commit();
            let payload = c.id.capacity()
                + c.subject.capacity()
                + c.author.name.capacity()
                + c.author.email.capacity()
                + c.committer.name.capacity()
                + c.committer.email.capacity()
                + observed.message.capacity()
                + c.parents.capacity() * size_of::<String>()
                + c.parents.iter().map(String::capacity).sum::<usize>()
                + c.refs.capacity() * size_of::<String>()
                + c.refs.iter().map(String::capacity).sum::<usize>()
                + c.raw_refs.capacity() * size_of::<Vec<u8>>()
                + c.raw_refs.iter().map(Vec::capacity).sum::<usize>();
            commit_bytes = commit_bytes
                .checked_add(payload)
                .ok_or("Original page payload charge overflow")?;
        }
        let mut ref_bytes = size_of::<RefSnapshotV1>()
            + self.refs.captured_bytes.len()
            + self.refs.head.as_ref().map_or(0, String::capacity)
            + self.refs.by_oid.capacity() * (size_of::<(String, Vec<(Vec<u8>, String)>)>() + 8);
        for (id, names) in &self.refs.by_oid {
            ref_bytes = ref_bytes
                .checked_add(
                    id.capacity()
                        + names.capacity() * size_of::<(Vec<u8>, String)>()
                        + names
                            .iter()
                            .map(|(raw, label)| raw.capacity() + label.capacity())
                            .sum::<usize>(),
                )
                .ok_or("Original references charge overflow")?;
        }
        let shallow_bytes = size_of::<HashSet<String>>()
            + self.epoch.shallow.capacity() * (size_of::<String>() + 8)
            + self
                .epoch
                .shallow
                .iter()
                .map(String::capacity)
                .sum::<usize>();
        let charges = vec![
            GraphMemoryChargeV2 {
                kind: 1,
                pointer: Arc::as_ptr(self) as usize,
                bytes: size_of::<Self>()
                    + self.walk_tip.capacity()
                    + self.epoch.path.capacity()
                    + self.lookahead.as_ref().map_or(0, String::capacity)
                    + 32,
            },
            GraphMemoryChargeV2 {
                kind: 2,
                pointer: self.commits.as_ptr() as usize,
                bytes: commit_bytes + 32,
            },
            GraphMemoryChargeV2 {
                kind: 3,
                pointer: Arc::as_ptr(&self.refs) as usize,
                bytes: ref_bytes + 32,
            },
            GraphMemoryChargeV2 {
                kind: 4,
                pointer: Arc::as_ptr(&self.source) as usize,
                bytes: self.source.retained_memory_bytes_v2() + 32,
            },
            GraphMemoryChargeV2 {
                kind: 5,
                pointer: Arc::as_ptr(&self.epoch.shallow) as usize,
                bytes: shallow_bytes + 32,
            },
        ];
        if charges
            .iter()
            .any(|charge| charge.pointer == 0 || charge.bytes > 16 * 1024 * 1024)
        {
            return Err("Original immutable page component exceeds retention budget".into());
        }
        Ok(charges)
    }
}
