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
            comparison: workspace
                .comparison
                .as_ref()
                .map(|result| &result.comparison),
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
            _ => None,
        }
    }
}
