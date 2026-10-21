//! TASK-119: the Repository Overview in plain language, as sections: Branch (name, where it
//! pushes, ahead/behind, Push and Pull rows and the last commit's full id and UTC time),
//! Changes (counts that open the Working and Staged diffs, then the changed files), Worktrees
//! (TASK-118: a summary that expands to the whole stack), Stashes, Recent commits, and the
//! reflog recovery behind its own entry. Each section starts at a hidden heading row whose
//! label is one of [`SECTIONS`]; the Repository panels draw every section as its own panel.
use super::{
    rescue_candidates_from_reflogs_v1, CommitCellsV91, OverviewLinkV119, RepositoryMode, RowAction,
    Workspace,
};
use crate::{
    git::{ChangeCountsV118, GitRepository, RepoStatus},
    local_actions::LocalCommand,
    providers::runner::sanitize_terminal,
    repository_view::{paused_operation_rows_v1, workspace_rows, SemanticRole},
};

pub const CHANGES: &str = "Changes";
pub const WORKTREES: &str = "Worktrees";
pub const STASHES: &str = "Stashes";
pub const RECENT: &str = "Recent commits";
pub const RECOVER: &str = "Recover lost commits";
/// The hidden heading rows, in drawing order (Branch has none: it starts the list).
pub const SECTIONS: [&str; 5] = [CHANGES, WORKTREES, STASHES, RECENT, RECOVER];
/// The summary row's text after the number of worktrees.
pub const STACK_HINT: &str = "Enter shows or hides them";

fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// `Branch: main · tracking origin/main · 2 to push, 1 to pull`, in plain words.
pub fn branch_line(status: &RepoStatus) -> String {
    let Some(branch) = status.branch.as_deref() else {
        return if status.detached {
            "Branch: detached HEAD · not on a branch; commits here belong to no branch".into()
        } else {
            "Branch: none yet".into()
        };
    };
    let branch = sanitize_terminal(branch);
    if status.head.is_none() {
        return format!("Branch: {branch} · no commits yet");
    }
    let Some(upstream) = status.upstream.as_deref() else {
        return format!("Branch: {branch} · no upstream yet · Push publishes it");
    };
    let upstream = sanitize_terminal(upstream);
    let state = match (status.ahead, status.behind) {
        (0, 0) => "up to date as of the last fetch".to_owned(),
        (ahead, 0) => format!("{ahead} to push"),
        (0, behind) => format!("{behind} to pull"),
        (ahead, behind) => format!("{ahead} to push, {behind} to pull · diverged: pull first"),
    };
    format!("Branch: {branch} · tracking {upstream} · {state}")
}

/// The Branch section's buttons: Push and Pull with their counts, or Push to publish a branch
/// without an upstream. None for a detached HEAD or a branch without commits. They open the
/// usual Local operation forms (preview and confirmation first).
pub fn branch_buttons(status: &RepoStatus) -> Vec<(String, LocalCommand)> {
    if status.branch.is_none() || status.head.is_none() {
        return vec![];
    }
    match status.upstream.as_deref().map(sanitize_terminal) {
        Some(upstream) => vec![
            (
                format!(
                    "Push → {upstream} · {}",
                    match status.ahead {
                        0 => "nothing to send".into(),
                        ahead => plural(ahead, "commit to send", "commits to send"),
                    }
                ),
                LocalCommand::Push,
            ),
            (
                format!(
                    "Pull ← {upstream} · {}",
                    match status.behind {
                        0 => "nothing new as of the last fetch".into(),
                        behind => plural(behind, "commit to receive", "commits to receive"),
                    }
                ),
                LocalCommand::Pull,
            ),
        ],
        None => vec![(
            "Push to publish this branch (sets where it pushes to)".into(),
            LocalCommand::Push,
        )],
    }
}

/// The commit the Overview shows in its Branch section: `Last commit` (the checked-out one)
/// or the commit chosen from the lists, with its UTC time and subject, its full id on its own
/// line (a SHA-256 id fits 80 columns), and its author and committer.
pub fn commit_lines(commit: &crate::git::Commit, last: bool) -> Vec<String> {
    let same = commit.author.name == commit.committer.name
        && commit.author.email == commit.committer.email;
    vec![
        format!(
            "{} · {} · {}",
            if last {
                "Last commit"
            } else {
                "Selected commit"
            },
            crate::git_actor_labels_v1::utc_datetime(commit.author.timestamp),
            sanitize_terminal(&commit.subject)
        ),
        format!("  {}", commit.id),
        if same {
            format!(
                "  author and committer {}",
                crate::git_actor_labels_v1::commit_actors(commit)
            )
        } else {
            format!("  {}", crate::git_actor_labels_v1::commit_actors(commit))
        },
    ]
}

/// The Changes section's links: the unstaged and staged counts open the Working and Staged
/// diffs (the `w` and `i` mode keys open the same views).
pub fn change_links(status: &RepoStatus) -> Vec<(String, RepositoryMode)> {
    let counts = ChangeCountsV118::from_status(status);
    let mut links = vec![];
    if counts.unstaged > 0 {
        links.push((
            format!(
                "Unstaged: {} · w opens the Working diff",
                plural(counts.unstaged, "file changed", "files changed")
            ),
            RepositoryMode::Working,
        ));
    }
    if counts.staged > 0 {
        links.push((
            format!(
                "Staged: {} · i opens the Staged diff",
                plural(
                    counts.staged,
                    "file ready to commit",
                    "files ready to commit"
                )
            ),
            RepositoryMode::Staged,
        ));
    }
    links
}

impl Workspace {
    pub(super) fn overview_rows_v119(&mut self, repo: &GitRepository) -> Result<(), String> {
        let status = self.status.clone();
        let commits = repo.log(12).map_err(|error| error.to_string())?;

        // Branch. The row starts the list and holds the first selection, outside the keyboard
        // order as it always was (Up from the first file stays there); a click opens History,
        // whose `g` key the tab strip shows.
        self.push(&branch_line(&status), SemanticRole::Head, None);
        let paused = paused_operation_rows_v1(&status);
        let mut changes = vec![];
        for (index, row) in workspace_rows(&status).into_iter().enumerate() {
            let upstream = row.label.starts_with("Upstream ")
                || row.label.starts_with("Git-effective upstream ");
            if index == 0 || upstream {
                continue;
            }
            let action = match row.path.clone() {
                Some(path) if row.role == SemanticRole::Conflict => Some(RowAction::Conflict(path)),
                Some(path) => Some(RowAction::WorkingFile(path)),
                None => paused.iter().find_map(|(label, _, command)| {
                    (*label == row.label)
                        .then_some(*command)
                        .flatten()
                        .map(RowAction::Local)
                }),
            };
            let is_paused = paused.iter().any(|(label, _, _)| *label == row.label);
            if row.path.is_some() || is_paused || row.label.starts_with("Working files are clean") {
                changes.push((row.label, row.role, action));
            } else {
                // History overrides, shallow and sparse notes stay with the branch.
                self.push(&row.label, row.role, action);
            }
        }

        // Changes.
        self.push(CHANGES, SemanticRole::Reference, None);
        // The unstaged and staged counts are drawn by the Changes panel as links, outside the
        // keyboard order: one Down from the Branch row still reaches the first changed file.
        // The paused-operation banner, the files and its Continue/Skip/Abort rows keep their
        // order.
        for (label, role, action) in changes {
            self.push(&label, role, action);
        }

        // Worktrees (TASK-118).
        self.push(WORKTREES, SemanticRole::Reference, None);
        let stack = if repo.linked_worktree_count_v1() > 0 {
            repo.worktree_stack_v118(&status).unwrap_or_default()
        } else {
            vec![]
        };
        // A single checkout leaves the section empty; the panel says so with a link to add one.
        if stack.len() > 1 {
            let changed = stack
                .iter()
                .filter(|entry| entry.counts.is_some_and(|counts| !counts.clean()))
                .count();
            self.push(
                &format!(
                    "{}: this checkout + {} linked · {} · {STACK_HINT}",
                    plural(stack.len(), "worktree", "worktrees"),
                    stack.len() - 1,
                    match changed {
                        0 => "all clean".to_owned(),
                        changed => format!("{changed} with changes"),
                    }
                ),
                SemanticRole::Reference,
                Some(RowAction::OverviewV119(OverviewLinkV119::ToggleWorktrees)),
            );
            let linked: Vec<&std::path::Path> = stack
                .iter()
                .filter(|entry| !entry.main)
                .map(|entry| entry.path.as_path())
                .collect();
            let prefix = crate::git::common_parent_v118(&linked);
            if let Some(prefix) = &prefix {
                self.push(
                    &format!("Linked worktrees are in {}", crate::git::tilde_v118(prefix)),
                    SemanticRole::Normal,
                    None,
                );
            }
            let width = stack
                .iter()
                .map(|entry| crate::editor::display_width(&entry.branch_label()))
                .max()
                .unwrap_or(0)
                .min(28);
            for entry in &stack {
                let branch = entry.branch_label();
                let pad = width.saturating_sub(crate::editor::display_width(&branch));
                let mut parts = vec![];
                if entry.main {
                    parts.push("main checkout".to_owned());
                }
                if entry.current {
                    parts.push("you are here".into());
                }
                parts.push(match (entry.counts, entry.unread) {
                    (Some(counts), _) => counts.label(),
                    (None, Some(reason)) => reason.into(),
                    (None, None) => "status unavailable".into(),
                });
                if entry.upstream.is_some() && (entry.ahead > 0 || entry.behind > 0) {
                    parts.push(format!("↑{} ↓{}", entry.ahead, entry.behind));
                }
                if entry.locked {
                    parts.push("locked".into());
                }
                if entry.prunable {
                    parts.push("prunable".into());
                }
                let path = if entry.main {
                    crate::git::tilde_v118(&entry.path)
                } else {
                    crate::git::short_path_v118(&entry.path, prefix.as_deref())
                };
                let label = format!(
                    "⎇ {branch}{} · {} · {}",
                    " ".repeat(pad),
                    parts.join(" · "),
                    sanitize_terminal(&path)
                );
                let openable = !entry.prunable && entry.path.is_dir();
                let role = match entry.counts {
                    _ if !openable => SemanticRole::Boundary,
                    Some(counts) if !counts.clean() => SemanticRole::Changed,
                    _ => SemanticRole::Normal,
                };
                self.push(
                    &label,
                    role,
                    openable.then(|| RowAction::OpenWorktree(entry.path.clone())),
                );
            }
            self.push(
                "Add or remove a linked worktree · m Local operations",
                SemanticRole::Normal,
                Some(RowAction::Local(LocalCommand::AddWorktree)),
            );
        }

        // Stashes.
        self.push(STASHES, SemanticRole::Reference, None);
        let stashes = repo.stash_overview_v1().unwrap_or_default();
        // No stashes leaves the section empty; the panel says so with a link to save one.
        if let Some(newest) = stashes.first() {
            self.push(
                &format!(
                    "{} · newest {}: {} · Enter to apply one",
                    plural(stashes.len(), "stash", "stashes"),
                    sanitize_terminal(
                        newest
                            .selector
                            .strip_prefix("refs/")
                            .unwrap_or(&newest.selector)
                    ),
                    sanitize_terminal(&newest.subject)
                ),
                SemanticRole::Reference,
                Some(RowAction::Local(LocalCommand::StashApply)),
            );
        }

        // Recent commits.
        self.push(RECENT, SemanticRole::Reference, None);
        let mut recent = vec![];
        for commit in commits {
            if self.selected_commit.is_none() {
                self.selected_commit = Some(commit.clone());
            }
            let label = format!(
                "● {} {} • {}",
                &commit.id[..8.min(commit.id.len())],
                commit.subject,
                crate::git_actor_labels_v1::commit_summary(&commit)
            );
            recent.push(commit.id.clone());
            let cells = CommitCellsV91 {
                id: commit.id.clone(),
                subject: commit.subject.clone(),
                actors: crate::git_actor_labels_v1::commit_actors(&commit),
                authored: commit.author.timestamp,
                author: crate::git_actor_labels_v1::actor_text(&commit.author),
                committer: crate::git_actor_labels_v1::actor_text(&commit.committer),
                committed: commit.committer.timestamp,
                actor: (commit.author.name == commit.committer.name
                    && commit.author.email == commit.committer.email)
                    .then(|| commit.author.clone()),
            };
            self.push(
                &label,
                SemanticRole::Normal,
                Some(RowAction::Commit(commit.id)),
            );
            if let Some(row) = self.rows.last_mut() {
                row.cells = Some(Box::new(cells));
            }
        }

        // Recovery, behind its own entry.
        let candidates = rescue_candidates_from_reflogs_v1(repo, &recent, 8);
        if !candidates.is_empty() {
            // One read-only batch check: a reflog line can outlive its pruned commit, which
            // is then stated, never offered.
            let ids: Vec<String> = candidates.iter().map(|c| c.id.clone()).collect();
            let present = repo
                .commits_present(&ids)
                .unwrap_or_else(|_| vec![true; ids.len()]);
            self.push(RECOVER, SemanticRole::Reference, None);
            self.push(
                &format!(
                    "{} left behind by amend, reset, rebase or branch moves · Enter shows or hides them",
                    plural(candidates.len(), "commit", "commits")
                ),
                SemanticRole::Changed,
                Some(RowAction::OverviewV119(OverviewLinkV119::ToggleRecovery)),
            );
            self.push(
                "From local reflogs (HEAD and branches); entries expire after gc.reflogExpire (about 90 days).",
                SemanticRole::Boundary,
                None,
            );
            self.push(
                "Pruned commits are gone and cannot be recovered; this is not a universal undo.",
                SemanticRole::Boundary,
                None,
            );
            self.push(
                "To recover one: select it, Enter to inspect it, then m → Create rescue branch",
                SemanticRole::Boundary,
                None,
            );
            for (candidate, present) in candidates.into_iter().zip(present) {
                let subject = if candidate.subject.is_empty() {
                    "(subject not in reflog)"
                } else {
                    &candidate.subject
                };
                if present {
                    self.push(
                        &format!(
                            "↺ {} {} • {} • {}",
                            &candidate.id[..8],
                            subject,
                            candidate.reason,
                            candidate.selector
                        ),
                        SemanticRole::Changed,
                        Some(RowAction::Commit(candidate.id)),
                    );
                } else {
                    self.push(
                        &format!(
                            "✗ {} {} • {} • {} • pruned: commit is gone, cannot recover",
                            &candidate.id[..8],
                            subject,
                            candidate.reason,
                            candidate.selector
                        ),
                        SemanticRole::Boundary,
                        None,
                    );
                }
            }
        }
        Ok(())
    }
}
