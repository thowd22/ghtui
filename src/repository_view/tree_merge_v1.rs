//! Atomic bounded keyed merge, raw paths never lowercased/display-parsed.
use super::*;
use std::collections::BTreeMap;

impl TreeBrowser {
    pub fn merge_bounded_keyed_v1(&mut self, incoming: Vec<TreeEntry>) -> Result<(), String> {
        let mut nodes = BTreeMap::<&[u8], &TreeEntry>::new();
        let mut bytes = 0usize;
        for node in self.entries.iter().chain(incoming.iter()) {
            if node.path.0.is_empty()
                || node.path.0.len() > 4096
                || node.path.0.contains(&0)
                || !crate::git::original_actor_saved_sha(&node.id)
                || !matches!(
                    (node.mode.as_str(), node.kind.as_str()),
                    ("040000", "tree")
                        | ("100644" | "100755" | "120000", "blob")
                        | ("160000", "commit")
                )
            {
                return Err("Original tree merge member exceeds raw bounds or has contradictory object type".into());
            }
            if let Some(old) = nodes.get(node.path.0.as_slice()) {
                if old.id != node.id || old.mode != node.mode || old.kind != node.kind {
                    return Err("Original immutable tree path has conflicting object/mode; no replacement merge".into());
                }
            } else {
                bytes = bytes
                    .checked_add(node.path.0.len())
                    .ok_or("Tree name budget overflow")?;
                nodes.insert(node.path.0.as_slice(), node);
                if nodes.len() > 50_000 || bytes > 8 * 1024 * 1024 {
                    return Err("Original tree merge exceeds50k rows/8MiB names".into());
                }
            }
        }
        // All source/raw collision/budget checks finish before modifying self.
        let mut existing: BTreeSet<Vec<u8>> = self
            .entries
            .iter()
            .map(|entry| entry.path.0.clone())
            .collect();
        self.entries.extend(
            incoming
                .into_iter()
                .filter(|entry| existing.insert(entry.path.0.clone())),
        );
        self.entries.sort_by(|a, b| a.path.0.cmp(&b.path.0));
        Ok(())
    }
    pub fn expanded_v1(&self, path: &RawPath) -> bool {
        self.expanded.contains(&path.0)
    }
}

#[cfg(test)]
#[path = "tree_merge_v1_tests.rs"]
mod tests;
