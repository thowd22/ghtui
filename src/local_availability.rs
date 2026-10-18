//! Borrowed local-operation availability, not an execution context or Git plan.
//! LocalCommand::disabled_reason and command discovery both delegate here so
//! menus and discovery share exactly one set of explanation semantics.
use crate::{
    git::{Comparison, RawPath, RepoStatus},
    local_actions::{LocalCommand, LocalContext},
    repository_view::DiffAnchor,
    workspace::{RowAction, Workspace},
};

pub struct LocalAvailability<'a> {
    status: &'a RepoStatus,
    path: Option<&'a RawPath>,
    anchor: Option<&'a DiffAnchor>,
    comparison: Option<&'a Comparison>,
}
impl<'a> LocalAvailability<'a> {
    pub(crate) fn for_native_selection_v2(
        status: &'a RepoStatus,
        anchor: &'a DiffAnchor,
        comparison: &'a Comparison,
    ) -> Self {
        Self {
            status,
            path: anchor.new_path.as_ref().or(anchor.old_path.as_ref()),
            anchor: Some(anchor),
            comparison: Some(comparison),
        }
    }
    pub fn from_context(context: &'a LocalContext) -> Self {
        Self {
            status: &context.status,
            path: context.selected_path.as_ref(),
            anchor: context.selected_anchor.as_ref(),
            comparison: context.comparison.as_ref().map(|result| &result.comparison),
        }
    }
    /// Mirrors the real App priority: explicit canonical selection first, then
    /// the current prepared row. No status/diff/body/path clone or I/O occurs.
    pub fn from_workspace(
        workspace: &'a Workspace,
        selected_row: usize,
        selected_anchor: Option<&'a DiffAnchor>,
    ) -> Self {
        let row = workspace
            .display_rows_v1()
            .get(selected_row)
            .and_then(|row| row.action.as_ref());
        let anchor = selected_anchor.or(match row {
            Some(RowAction::DiffLine(anchor)) => Some(anchor),
            _ => None,
        });
        let path = if selected_anchor.is_some() {
            None
        } else {
            match row {
                Some(
                    RowAction::WorkingFile(path)
                    | RowAction::Conflict(path)
                    | RowAction::File(path),
                ) => Some(path),
                _ => None,
            }
        }
        .or_else(|| {
            anchor.and_then(|anchor| anchor.new_path.as_ref().or(anchor.old_path.as_ref()))
        });
        Self {
            status: &workspace.status,
            path,
            anchor,
            comparison: workspace.comparison_v1().map(|result| &result.comparison),
        }
    }
    pub fn disabled_reason(&self, command: LocalCommand) -> Option<String> {
        use LocalCommand::*;
        if self.status.has_history_overrides()
            && !matches!(
                command,
                StageFile | UnstageFile | StageSelection | UnstageSelection | Fetch
            )
        {
            return Some("History writes guarded: replacement/graft overrides active.".into());
        }
        match command {
            StageFile | UnstageFile | DiscardFile | ResolveConflict if self.path.is_none() => {
                Some("Select an exact changed file first.".into())
            }
            StageSelection | UnstageSelection if self.anchor.is_none() => {
                Some("Select a canonical added or removed diff line first.".into())
            }
            StageSelection if !matches!(self.comparison, Some(Comparison::Working)) => Some(
                "Select an unstaged change in Working diff; saved history is read-only.".into(),
            ),
            UnstageSelection if !matches!(self.comparison, Some(Comparison::Staged)) => {
                Some("Select a staged change in Staged diff; saved history is read-only.".into())
            }
            Continue | Skip | Abort if self.status.operation.is_none() => {
                Some("There is no paused Git operation.".into())
            }
            ResolveConflict
                if !self
                    .status
                    .entries
                    .iter()
                    .any(|entry| entry.conflict && Some(&entry.path) == self.path) =>
            {
                Some("Select a conflicted file first.".into())
            }
            _ => paused_operation_reason_v1(self.status, command),
        }
    }
}

/// Paused merge/rebase/cherry-pick/revert guidance. Starting another history
/// operation, switching, pulling or stashing on top of a paused operation is
/// refused before any form opens, and Continue explains unresolved conflicts.
pub(crate) fn paused_operation_reason_v1(
    status: &RepoStatus,
    command: LocalCommand,
) -> Option<String> {
    use crate::git::Sequencer;
    use LocalCommand::*;
    let operation = status.operation.as_ref()?;
    let conflicts = status.conflicts();
    if *operation == Sequencer::Bisect {
        return matches!(command, Continue | Skip | Abort).then(|| {
            "A bug search is active; mark versions good/bad/skip or End bug search.".into()
        });
    }
    match command {
        Continue | Commit if conflicts > 0 => Some(format!(
            "Resolve and stage {conflicts} conflicted file{} first.",
            if conflicts == 1 { "" } else { "s" }
        )),
        Skip if *operation == Sequencer::Merge => {
            Some("A merge has no commit to skip; continue or abort it.".into())
        }
        Amend | Merge | Rebase | CherryPick | Revert | SwitchBranch | Pull | StashSave
        | StashApply | StashPop | BisectStart => Some(format!(
            "A {} is paused; continue or abort it first.",
            operation_name_v1(operation)
        )),
        _ => None,
    }
}

pub(crate) fn operation_name_v1(operation: &crate::git::Sequencer) -> &'static str {
    use crate::git::Sequencer;
    match operation {
        Sequencer::Merge => "merge",
        Sequencer::Rebase => "rebase",
        Sequencer::CherryPick => "cherry-pick",
        Sequencer::Revert => "revert",
        Sequencer::Bisect => "bug search",
    }
}

#[cfg(test)]
mod v1_local_recovery_tests {
    use super::*;
    use crate::git::{Sequencer, StatusEntry};
    use crate::repository_view::paused_operation_rows_v1;

    fn status(operation: Option<Sequencer>, conflicts: usize) -> RepoStatus {
        RepoStatus {
            branch: Some("main".into()),
            operation,
            entries: (0..conflicts)
                .map(|index| StatusEntry {
                    path: RawPath::new(format!("file{index}.txt").into_bytes()),
                    original_path: None,
                    index: 'U',
                    worktree: 'U',
                    conflict: true,
                    submodule: false,
                })
                .collect(),
            ..RepoStatus::default()
        }
    }
    fn reason(status: &RepoStatus, command: LocalCommand) -> Option<String> {
        LocalAvailability {
            status,
            path: None,
            anchor: None,
            comparison: None,
        }
        .disabled_reason(command)
    }

    #[test]
    fn v1_local_recovery_paused_merge_guards_conflicting_actions() {
        use LocalCommand::*;
        let merge = status(Some(Sequencer::Merge), 2);
        assert_eq!(
            reason(&merge, Continue).as_deref(),
            Some("Resolve and stage 2 conflicted files first.")
        );
        assert!(reason(&merge, Commit).unwrap().contains("conflicted"));
        assert!(reason(&merge, Skip).unwrap().contains("no commit to skip"));
        for command in [
            Merge,
            Rebase,
            CherryPick,
            Revert,
            SwitchBranch,
            Pull,
            StashSave,
            StashPop,
            Amend,
        ] {
            assert_eq!(
                reason(&merge, command).as_deref(),
                Some("A merge is paused; continue or abort it first."),
                "{command:?}"
            );
        }
        assert_eq!(reason(&merge, Abort), None);
        assert_eq!(reason(&merge, CreateBranch), None);
        assert_eq!(reason(&merge, RecoverBranch), None);

        let rebase = status(Some(Sequencer::Rebase), 0);
        assert_eq!(reason(&rebase, Continue), None);
        assert_eq!(reason(&rebase, Skip), None);
        assert!(reason(&rebase, Merge).unwrap().contains("rebase is paused"));

        let bisect = status(Some(Sequencer::Bisect), 0);
        assert!(reason(&bisect, Continue).unwrap().contains("bug search"));
        assert_eq!(reason(&bisect, BisectGood), None);
        assert_eq!(reason(&bisect, Merge), None);

        let clean = status(None, 0);
        assert_eq!(reason(&clean, Merge), None);
        assert!(reason(&clean, Continue).unwrap().contains("no paused"));
    }

    #[test]
    fn v1_local_recovery_paused_rows_name_next_step_and_actions() {
        let rows = paused_operation_rows_v1(&status(Some(Sequencer::CherryPick), 1));
        assert!(rows[0]
            .0
            .contains("CHERRY-PICK PAUSED • 1 unresolved conflict"));
        assert!(rows[1].0.starts_with("Next: open each [conflicted] file"));
        let commands: Vec<_> = rows.iter().filter_map(|row| row.2).collect();
        assert_eq!(
            commands,
            vec![
                LocalCommand::Continue,
                LocalCommand::Skip,
                LocalCommand::Abort
            ]
        );
        let merge = paused_operation_rows_v1(&status(Some(Sequencer::Merge), 0));
        assert!(merge[0].0.contains("all conflicts resolved"));
        assert!(merge.iter().all(|row| row.2 != Some(LocalCommand::Skip)));
        assert!(paused_operation_rows_v1(&status(None, 0)).is_empty());
        let stash = paused_operation_rows_v1(&status(None, 1));
        assert!(stash[0]
            .0
            .starts_with("⚠ 1 unresolved conflict without a paused operation"));
        assert!(stash.iter().all(|row| row.2.is_none()));
    }

    #[test]
    fn v1_local_recovery_reflog_rescue_candidates_are_left_behind_commits() {
        use crate::repository_view::rescue_candidates_v1;
        let zero = "0".repeat(40);
        let (a, b, c, d) = (
            "a".repeat(40),
            "b".repeat(40),
            "c".repeat(40),
            "d".repeat(40),
        );
        let log = format!(
            "{zero} {a} T <t@e> 1 +0000\tcommit (initial): first\n\
             {a} {b} T <t@e> 2 +0000\tcommit: draft work\n\
             {b} {c} T <t@e> 3 +0000\tcommit (amend): polished work\n\
             {c} {a} T <t@e> 4 +0000\tcheckout: moving from main to old\n\
             {a} {d} T <t@e> 5 +0000\treset: moving to HEAD~1\n\
             {d} {c} T <t@e> 6 +0000\treset: moving to {c}\n\
             not a reflog line\n"
        );
        let found = rescue_candidates_v1(log.as_bytes(), std::slice::from_ref(&c), 8);
        assert_eq!(found.len(), 3, "{found:?}");
        assert_eq!(found[0].id, d);
        assert_eq!(found[0].selector, "HEAD@{0}");
        assert_eq!(found[0].reason, "before reset");
        assert_eq!(found[1].id, a);
        assert_eq!(found[1].selector, "HEAD@{1}");
        assert_eq!(found[1].subject, "moving from main to old");
        assert_eq!(found[2].id, b);
        assert_eq!(found[2].reason, "before amend");
        assert_eq!(found[2].subject, "draft work");
        assert_eq!(rescue_candidates_v1(log.as_bytes(), &[c], 1).len(), 1);
        assert!(rescue_candidates_v1(b"", &[], 8).is_empty());
    }

    #[test]
    fn v1_local_recovery_conflict_version_names_and_markers() {
        use crate::local_actions::{conflict_markers_v1, conflict_stage_v1};
        assert_eq!(conflict_stage_v1("ours"), Ok(2));
        assert_eq!(conflict_stage_v1("theirs"), Ok(3));
        assert_eq!(conflict_stage_v1("base"), Ok(1));
        assert_eq!(conflict_stage_v1("2"), Ok(2));
        assert!(conflict_stage_v1("mine").is_err());
        let dir = std::env::temp_dir().join(format!("ghtui-v1-markers-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let marked = dir.join("marked.txt");
        std::fs::write(
            &marked,
            b"<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> feature\n",
        )
        .unwrap();
        let clean = dir.join("clean.txt");
        std::fs::write(&clean, b"ours\n<<<<<<< not closed\n").unwrap();
        assert!(conflict_markers_v1(&marked));
        assert!(!conflict_markers_v1(&clean));
        assert!(!conflict_markers_v1(&dir.join("absent")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
