//! Explicit metadata source intent. No automatic organization hydration or commit indexing.
use super::{collaboration::DiscussionTarget, commits::CommitSelection, QueryError};
use crate::storage::Scope;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataSources {
    #[serde(default)]
    pub discussions: Vec<DiscussionTarget>,
    #[serde(default)]
    pub commits: Vec<CommitSelection>,
}
impl MetadataSources {
    pub fn is_empty(&self) -> bool {
        self.discussions.is_empty() && self.commits.is_empty()
    }
    pub fn repositories(&self) -> BTreeSet<String> {
        self.discussions
            .iter()
            .map(|target| target.repository.clone())
            .chain(self.commits.iter().map(|target| target.repository.clone()))
            .collect()
    }
    pub fn validate(&self, scope: &Scope) -> Result<(), QueryError> {
        if self.discussions.len() > 1000 || self.commits.len() > 1000 {
            return Err(QueryError(
                "Choose at most 1000 explicit discussion sources/history repositories".into(),
            ));
        }
        let mut discussions = BTreeSet::new();
        for target in &self.discussions {
            target.validate()?;
            if &target.scope != scope || !discussions.insert(target.source()) {
                return Err(QueryError("Metadata sources belong to another account or repeat a canonical discussion category".into()));
            }
        }
        let mut commits = BTreeSet::new();
        for target in &self.commits {
            crate::config::validate_repository(&target.repository)
                .map_err(|error| QueryError(error.to_string()))?;
            if !commits.insert(target.repository.to_ascii_lowercase())
                || !target.directory.is_absolute()
                || !(1..10_000).contains(&target.max_commits)
                || target.revision.trim().is_empty()
                || target.revision.len() > 16 * 1024
            {
                return Err(QueryError("Commit sources require distinct repositories, exact absolute directories, explicit revision and 1..9999 loaded-message scope".into()));
            }
        }
        Ok(())
    }
}
