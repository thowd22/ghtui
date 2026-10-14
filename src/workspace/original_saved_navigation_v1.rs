//! Actual installed-Git reads through the admitted original source handle.
//! Public wrappers are read-only; Root supplies one pre-queue operation in Main.
use super::*;
use crate::providers::local_git_navigation_read_v1::LocalGitNavigationReadV1;

impl Workspace {
    pub fn open_original_with_operation_v1(
        &mut self,
        action: RowAction,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<(), String> {
        operation.check()?;
        let pure = matches!(&action, RowAction::Graph(action) if !matches!(action, crate::repository_graph_v1::GraphActionV1::More | crate::repository_graph_v1::GraphActionV1::CompareParent(_)))
            || matches!(
                &action,
                RowAction::Actor(_)
                    | RowAction::WorkingFile(_)
                    | RowAction::DiffLine(_)
                    | RowAction::Legend
            )
            || matches!(&action, RowAction::Commit(sha) if self.graph_state_v1.as_ref().is_some_and(|state| state.selected_commit(sha).is_some()) || self.graph.rows.iter().any(|row| row.commit.id == *sha));
        if pure {
            self.open_from_repository_v1(action, operation.cancellation().clone(), None)?;
            return operation.check();
        }
        let origin = self.actor_origin.as_ref()
            .ok_or("Original loaded Git source is unavailable; Refresh explicitly before reading, no path was rediscovered")?
            .clone();
        origin.with_saved_navigation_read_v1(operation, |repository| {
            self.open_from_repository_v1(
                action,
                operation.cancellation().clone(),
                Some(repository),
            )?;
            if self
                .actor_origin
                .as_ref()
                .is_none_or(|next| !Arc::ptr_eq(next, &origin))
            {
                return Err(
                    "Nested navigation changed its original Git source; no result admitted".into(),
                );
            }
            Ok(())
        })
    }

    pub fn load_original_with_operation_v1(
        &self,
        mode: RepositoryMode,
        revision: &str,
        operation: &LocalGitNavigationReadV1,
    ) -> Result<Self, String> {
        if mode == self.mode && self.graph_retention_visible_v2() {
            return self.refresh_original_graph_v3(operation);
        }
        let origin = self
            .actor_origin
            .as_ref()
            .ok_or("Original loaded Git source is unavailable; no path/revision source inferred")?
            .clone();
        if matches!(
            mode,
            RepositoryMode::Graph | RepositoryMode::Tree | RepositoryMode::CommitChanges
        ) && !(crate::git::original_actor_saved_sha(revision)
            || revision == "HEAD" && self.status.head.is_none())
        {
            return Err("Saved navigation requires the original full canonical saved SHA".into());
        }
        origin.with_saved_navigation_read_v1(operation, |repository| {
            let previous_tree = self
                .tree
                .as_ref()
                .filter(|tree| tree.revision == revision)
                .cloned();
            Self::load_from_repository_v1(
                repository,
                mode,
                revision,
                previous_tree,
                Some(origin.clone()),
            )?
            .ok_or("Original local Git repository is unavailable".into())
        })
    }
}

#[cfg(test)]
#[path = "original_saved_navigation_v1_tests.rs"]
pub(crate) mod tests;

#[cfg(test)]
#[path = "original_unborn_navigation_v1_tests.rs"]
mod unborn_tests;
