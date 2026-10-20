//! `f` File history from outside History (TASK-99.3): a file result in Search or a changed
//! file in PR Files opens History with that file's `path:` chip. History reads the local
//! repository only, so a file from another repository explains why instead. Reads only.
use crate::{
    app::{Action, App, View},
    git::RawPath,
    repository_graph_v1::GraphActionV1,
    search::model::Destination,
    workspace::RowAction,
};

/// The file, and the saved version whose latest change History selects (HEAD when None).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileHistoryTargetV993 {
    pub revision: Option<String>,
    pub path: RawPath,
}

pub const LOADING: &str =
    "The local repository is still loading • File history opens once it is ready";
pub const NO_LOCAL: &str =
    "File history reads a local Git repository • start ghtui inside a checkout";
pub const NO_COMMIT: &str = "This repository has no saved commits yet • no file history to show";

/// The local repository History would read, or why there is none.
fn local(app: &App) -> Result<&crate::workspace::Workspace, String> {
    let workspace = app.workspace.as_ref().ok_or_else(|| {
        if app.loading[View::Repository.index()] {
            LOADING.to_owned()
        } else {
            NO_LOCAL.to_owned()
        }
    })?;
    if workspace.actor_origin.is_none() {
        return Err(NO_LOCAL.into());
    }
    if workspace.status.head.is_none() {
        return Err(NO_COMMIT.into());
    }
    Ok(workspace)
}

/// Why a file of GitHub repository `repository` has no local History, if it has none.
fn same_repository(app: &App, repository: &str) -> Result<(), String> {
    match &app.local_github_repository_v993 {
        Some(local) if local.eq_ignore_ascii_case(repository) => Ok(()),
        Some(local) => Err(format!(
            "File history needs a local checkout of {} • this checkout is {} • start ghtui in that repository to follow its files",
            crate::app::safe_text(repository),
            crate::app::safe_text(local),
        )),
        None => Err(format!(
            "File history needs a local checkout of {} • this checkout has no GitHub remote for it",
            crate::app::safe_text(repository),
        )),
    }
}

fn same_directory(a: &std::path::Path, b: &std::path::Path) -> bool {
    a == b
        || std::fs::canonicalize(a)
            .ok()
            .zip(std::fs::canonicalize(b).ok())
            .is_some_and(|(a, b)| a == b)
}

/// The selected Search result as a File history target.
pub fn search_target(app: &App) -> Result<FileHistoryTargetV993, String> {
    let search = app
        .search
        .as_ref()
        .ok_or("Search first, then select a file result")?;
    let destination = search
        .destination(app.search_controls.selected)
        .map_err(|_| "Select a file result first".to_owned())?;
    let workspace = local(app)?;
    let target = match destination {
        Destination::LocalFile {
            directory,
            path,
            revision,
            ..
        } => {
            if !same_directory(&directory, &workspace.directory) {
                return Err(format!(
                    "File history reads this repository only ({}) • this result is in {}",
                    crate::app::safe_text(&workspace.directory.display().to_string()),
                    crate::app::safe_text(&directory.display().to_string()),
                ));
            }
            FileHistoryTargetV993 {
                // A working-file result has no saved version: History selects HEAD's change.
                revision: revision.filter(|revision| crate::search::model::valid_sha(revision)),
                path,
            }
        }
        Destination::RemoteFile {
            repository, path, ..
        }
        | Destination::RemoteCodeCandidate {
            repository, path, ..
        } => {
            same_repository(app, &repository)?;
            FileHistoryTargetV993 {
                revision: None,
                path,
            }
        }
        _ => return Err("File history needs a file result • this result is not a file".into()),
    };
    crate::search::model::validate_raw_path(&target.path).map_err(|error| error.to_string())?;
    Ok(target)
}

/// The selected PR Files row as a File history target.
pub fn pr_target(app: &App) -> Result<FileHistoryTargetV993, String> {
    let github = app.github.as_ref().ok_or("Open a pull request first")?;
    let page = github
        .pr
        .as_ref()
        .filter(|page| page.tab == crate::github_workspace::PrTab::Files)
        .ok_or("Open the PR's Files tab and select a file first")?;
    let row = app.inbox_list.selected;
    let path = github
        .scoped_pr_resource
        .as_ref()
        .and_then(|view| view.file_path_at(row))
        .or_else(|| page.file_path_at(row))
        .ok_or("Select a changed file or one of its diff lines first")?;
    same_repository(app, &page.details.summary.pr.repository)?;
    local(app)?;
    crate::search::model::validate_raw_path(&path).map_err(|error| error.to_string())?;
    Ok(FileHistoryTargetV993 {
        revision: None,
        path,
    })
}

/// The action of a File history control: open, or explain why not.
pub fn action(target: Result<FileHistoryTargetV993, String>) -> Action {
    target.map_or_else(Action::Explain, Action::FileHistoryV993)
}

/// Shows History with the target's path chip: the Repository view, then one read that opens
/// History over every branch, filters it to the file (following renames) and selects its
/// latest change.
pub fn open(app: &mut App, target: FileHistoryTargetV993) {
    if let Err(reason) = local(app) {
        app.dispatch(Action::Explain(reason));
        return;
    }
    app.dispatch(Action::View(View::Repository));
    app.dispatch(Action::RepositoryRow(RowAction::Graph(
        GraphActionV1::OpenPath {
            revision: target.revision,
            path: target.path,
        },
    )));
}

#[cfg(test)]
#[path = "entry_tests.rs"]
mod tests;
