//! TASK-118: the worktree stack (the main checkout and every linked worktree, each with its
//! branch, change counts and ahead/behind) and the stale `core.worktree` explanation with its
//! choices. Every read here is a non-writing Git child; the one write (unsetting the stale
//! setting) runs only after the caller showed its preview and the person confirmed it, and
//! refuses when the setting changed since the preview.
use super::{parse_status, parse_worktrees_v9, run_git_with_kind, trim_line, GitRepository};
use crate::providers::runner::CancellationToken;
use std::{
    ffi::OsString,
    os::unix::ffi::OsStringExt,
    path::{Path, PathBuf},
};

/// At most this many worktree status reads per Overview load; the rest say "not read".
pub const STATUS_READS_V118: usize = 32;

/// Staged, unstaged, untracked and conflicted file counts of one worktree.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ChangeCountsV118 {
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub conflicted: usize,
}

impl ChangeCountsV118 {
    pub fn from_status(status: &super::RepoStatus) -> Self {
        let mut counts = Self::default();
        for entry in &status.entries {
            if entry.conflict {
                counts.conflicted += 1;
            } else if entry.untracked() {
                counts.untracked += 1;
            } else {
                if entry.staged() {
                    counts.staged += 1;
                }
                if !matches!(entry.worktree, '.' | ' ') {
                    counts.unstaged += 1;
                }
            }
        }
        counts
    }
    pub fn clean(&self) -> bool {
        *self == Self::default()
    }
    /// "clean" or "2 staged, 1 changed, 3 new, 1 conflicted".
    pub fn label(&self) -> String {
        if self.clean() {
            return "clean".into();
        }
        let mut parts = vec![];
        for (count, word) in [
            (self.conflicted, "conflicted"),
            (self.staged, "staged"),
            (self.unstaged, "changed"),
            (self.untracked, "new"),
        ] {
            if count > 0 {
                parts.push(format!("{count} {word}"));
            }
        }
        parts.join(", ")
    }
}

/// One worktree of the stack.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeEntryV118 {
    pub path: PathBuf,
    /// Short branch name; `None` when detached.
    pub branch: Option<String>,
    pub head: Option<String>,
    /// The main checkout (Git lists it first).
    pub main: bool,
    /// The worktree this Repository view shows.
    pub current: bool,
    pub locked: bool,
    pub prunable: bool,
    /// `None` when its status was not read (bounded) or could not be read.
    pub counts: Option<ChangeCountsV118>,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    /// Why the status is missing, when it is.
    pub unread: Option<&'static str>,
}

impl WorktreeEntryV118 {
    pub fn branch_label(&self) -> String {
        match &self.branch {
            Some(branch) => crate::providers::runner::sanitize_terminal(branch),
            None => "detached HEAD".into(),
        }
    }
}

impl GitRepository {
    /// The worktree stack: Git's own list (main checkout first), with each worktree's change
    /// counts and ahead/behind from a non-writing `status` run inside it. The current
    /// worktree reuses `current` (the status this Overview already read).
    pub fn worktree_stack_v118(
        &self,
        current: &super::RepoStatus,
    ) -> super::GitResult<Vec<WorktreeEntryV118>> {
        let listed = self.worktrees()?;
        let here = std::fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        let mut stack = vec![];
        let mut reads = 0;
        for (index, tree) in listed.into_iter().enumerate() {
            if tree.bare {
                continue;
            }
            let canonical = std::fs::canonicalize(&tree.path).unwrap_or_else(|_| tree.path.clone());
            let is_current = canonical == here;
            let mut entry = WorktreeEntryV118 {
                branch: tree.branch.as_deref().map(|branch| {
                    branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(branch)
                        .to_owned()
                }),
                head: tree.head.clone(),
                main: index == 0,
                current: is_current,
                locked: tree.locked,
                prunable: tree.prunable,
                counts: None,
                upstream: None,
                ahead: 0,
                behind: 0,
                unread: None,
                path: tree.path,
            };
            if is_current {
                entry.counts = Some(ChangeCountsV118::from_status(current));
                entry.upstream = current.upstream.clone();
                entry.ahead = current.ahead;
                entry.behind = current.behind;
            } else if tree.prunable || !entry.path.is_dir() {
                entry.unread = Some("folder missing");
            } else if reads >= STATUS_READS_V118 {
                entry.unread = Some("status not read");
            } else {
                reads += 1;
                match self.worktree_status_v118(&entry.path) {
                    Ok(status) => {
                        entry.counts = Some(ChangeCountsV118::from_status(&status));
                        entry.upstream = status.upstream;
                        entry.ahead = status.ahead;
                        entry.behind = status.behind;
                    }
                    Err(_) => entry.unread = Some("status unavailable"),
                }
            }
            stack.push(entry);
        }
        Ok(stack)
    }

    /// Non-writing `status` inside one listed worktree (untracked folders count once).
    fn worktree_status_v118(&self, path: &Path) -> super::GitResult<super::RepoStatus> {
        let output = run_git_with_kind(
            path,
            [
                "status",
                "--porcelain=v2",
                "-z",
                "--branch",
                "--untracked-files=normal",
            ],
            None,
            false,
            &self.cancellation,
        )?;
        if !output.success {
            return Err(output.error("Inspect worktree status"));
        }
        parse_status(&output.stdout)
    }
}

/// `~` for the home directory.
pub fn tilde_v118(path: &Path) -> String {
    let text = path.display().to_string();
    match std::env::var_os("HOME").filter(|home| !home.is_empty() && home != "/") {
        Some(home) => {
            let home = PathBuf::from(home);
            match path.strip_prefix(&home) {
                Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
                Ok(rest) => format!("~/{}", rest.display()),
                Err(_) => text,
            }
        }
        None => text,
    }
}

/// The deepest folder that holds every path of `paths` (at least two), or `None`.
pub fn common_parent_v118(paths: &[&Path]) -> Option<PathBuf> {
    if paths.len() < 2 {
        return None;
    }
    let mut common: Vec<_> = paths[0].parent()?.components().collect();
    for path in &paths[1..] {
        let parent: Vec<_> = path.parent()?.components().collect();
        let same = common
            .iter()
            .zip(parent.iter())
            .take_while(|(a, b)| a == b)
            .count();
        common.truncate(same);
    }
    // The filesystem root alone trims nothing useful.
    (common.len() > 1).then(|| common.iter().collect())
}

/// A worktree path for the stack: below `prefix` as `…/rest`, otherwise with `~`.
pub fn short_path_v118(path: &Path, prefix: Option<&Path>) -> String {
    if let Some(rest) = prefix.and_then(|prefix| path.strip_prefix(prefix).ok()) {
        if !rest.as_os_str().is_empty() {
            return format!("…/{}", rest.display());
        }
    }
    tilde_v118(path)
}

/// A checkout whose `core.worktree` points at another folder (left by another tool).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreWorktreeMismatchV118 {
    /// The folder ghtui was opened in (it holds the `.git` directory).
    pub checkout: PathBuf,
    /// The checkout's Git directory, whose `config` holds the setting.
    pub git_dir: PathBuf,
    /// The folder Git uses as the working tree because of the setting.
    pub configured: PathBuf,
    /// The setting exactly as written in the config file.
    pub setting: String,
    /// Whether the configured folder exists.
    pub configured_exists: bool,
    /// Linked worktrees listed in the Git directory: path, short branch, missing folder.
    pub linked: Vec<(PathBuf, Option<String>, bool)>,
}

fn read_v118(directory: &Path, args: &[&str], cancel: &CancellationToken) -> Option<Vec<u8>> {
    let output = run_git_with_kind(directory, args, None, false, cancel).ok()?;
    output.success.then_some(output.stdout)
}

fn path_v118(bytes: &[u8]) -> PathBuf {
    PathBuf::from(OsString::from_vec(trim_line(bytes).to_vec()))
}

/// The stale `core.worktree` of `directory`, when its effective working tree is elsewhere.
/// Non-writing reads only.
pub fn core_worktree_mismatch_v118(
    directory: &Path,
    cancel: &CancellationToken,
) -> Option<CoreWorktreeMismatchV118> {
    let git_dir = path_v118(&read_v118(
        directory,
        &["rev-parse", "--absolute-git-dir"],
        cancel,
    )?);
    if read_v118(directory, &["rev-parse", "--is-bare-repository"], cancel)
        .is_some_and(|bare| trim_line(&bare) == b"true")
    {
        return None;
    }
    let config = git_dir.join("config");
    let config = config.as_os_str().to_str()?.to_owned();
    let setting = read_v118(
        directory,
        &["config", "--file", &config, "--get", "core.worktree"],
        cancel,
    )?;
    let setting = String::from_utf8_lossy(trim_line(&setting)).into_owned();
    // Git resolves a relative setting against the Git directory; a missing folder makes
    // `--show-toplevel` fail, so fall back to that resolution.
    let configured = read_v118(directory, &["rev-parse", "--show-toplevel"], cancel)
        .map(|bytes| path_v118(&bytes))
        .unwrap_or_else(|| git_dir.join(&setting));
    let here = std::fs::canonicalize(directory).ok()?;
    if std::fs::canonicalize(&configured).is_ok_and(|root| here.starts_with(root)) {
        return None;
    }
    let listed = read_v118(
        directory,
        &[
            "--git-dir",
            git_dir.as_os_str().to_str()?,
            "worktree",
            "list",
            "--porcelain",
            "-z",
        ],
        cancel,
    )
    .map(|bytes| parse_worktrees_v9(&bytes))
    .unwrap_or_default();
    let linked = listed
        .into_iter()
        .skip(1)
        .filter(|tree| !tree.bare)
        .map(|tree| {
            let missing = tree.prunable || !tree.path.is_dir();
            let branch = tree.branch.map(|branch| {
                branch
                    .strip_prefix("refs/heads/")
                    .unwrap_or(&branch)
                    .to_owned()
            });
            (tree.path, branch, missing)
        })
        .collect();
    Some(CoreWorktreeMismatchV118 {
        checkout: here,
        configured_exists: configured.is_dir(),
        git_dir,
        configured,
        setting,
        linked,
    })
}

/// The exact command the confirmed unset runs.
pub fn unset_command_v118(mismatch: &CoreWorktreeMismatchV118) -> String {
    format!(
        "git config --file {} --unset core.worktree",
        mismatch.git_dir.join("config").display()
    )
}

/// Removes the stale setting after confirmation. Refuses when the setting is no longer the
/// one the preview showed.
pub fn unset_core_worktree_v118(
    mismatch: &CoreWorktreeMismatchV118,
    cancel: &CancellationToken,
) -> Result<(), String> {
    let config = mismatch.git_dir.join("config");
    let config = config
        .as_os_str()
        .to_str()
        .ok_or("The Git config path is not valid UTF-8; nothing changed")?
        .to_owned();
    let current = read_v118(
        &mismatch.checkout,
        &["config", "--file", &config, "--get", "core.worktree"],
        cancel,
    )
    .map(|bytes| String::from_utf8_lossy(trim_line(&bytes)).into_owned());
    if current.as_deref() != Some(mismatch.setting.as_str()) {
        return Err(
            "core.worktree changed since the preview; nothing was changed. Refresh to see it again."
                .into(),
        );
    }
    let output = run_git_with_kind(
        &mismatch.checkout,
        [
            "config",
            "--file",
            config.as_str(),
            "--unset",
            "core.worktree",
        ],
        None,
        true,
        cancel,
    )
    .map_err(|error| error.to_string())?;
    if !output.success {
        return Err(output.error("Unset core.worktree").to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(directory: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Fixture")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[test]
    fn worktree_stack_v118_lists_main_and_linked_with_counts_and_ahead_behind() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("main");
        std::fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q", "--initial-branch=main"]);
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        git(&root, &["add", "a.txt"]);
        git(&root, &["commit", "-qm", "first"]);
        git(&root, &["branch", "one"]);
        git(&root, &["branch", "two"]);
        let trees = temp.path().join("trees");
        let one = trees.join("one");
        let two = trees.join("two");
        git(
            &root,
            &["worktree", "add", "-q", one.to_str().unwrap(), "one"],
        );
        git(
            &root,
            &["worktree", "add", "-q", two.to_str().unwrap(), "two"],
        );
        git(&root, &["worktree", "lock", two.to_str().unwrap()]);
        // `one` tracks main and is one commit ahead with a staged, a changed and a new file.
        git(&one, &["branch", "--set-upstream-to=main"]);
        std::fs::write(one.join("b.txt"), "b\n").unwrap();
        git(&one, &["add", "b.txt"]);
        git(&one, &["commit", "-qm", "second"]);
        std::fs::write(one.join("a.txt"), "changed\n").unwrap();
        std::fs::write(one.join("c.txt"), "staged\n").unwrap();
        git(&one, &["add", "c.txt"]);
        std::fs::write(one.join("new.txt"), "new\n").unwrap();
        let index = PathBuf::from(
            git(
                &one,
                &["rev-parse", "--path-format=absolute", "--git-path", "index"],
            )
            .trim(),
        );
        let index_before = std::fs::read(&index).unwrap();
        let repo = GitRepository::discover_with_cancellation(&root, CancellationToken::default())
            .unwrap()
            .unwrap();
        let status = repo.status().unwrap();
        let stack = repo.worktree_stack_v118(&status).unwrap();
        assert_eq!(stack.len(), 3);
        assert!(stack[0].main && stack[0].current);
        assert_eq!(stack[0].branch.as_deref(), Some("main"));
        assert_eq!(stack[0].counts.unwrap().label(), "clean");
        let first = &stack[1];
        assert_eq!(first.branch.as_deref(), Some("one"));
        assert!(!first.main && !first.current && !first.locked);
        assert_eq!(first.counts.unwrap().label(), "1 staged, 1 changed, 1 new");
        assert_eq!(first.upstream.as_deref(), Some("main"));
        assert_eq!((first.ahead, first.behind), (1, 0));
        assert!(stack[2].locked);
        // Reading the stack wrote nothing in the linked worktree's index.
        assert_eq!(index_before, std::fs::read(&index).unwrap());
        let paths: Vec<&Path> = stack[1..]
            .iter()
            .map(|entry| entry.path.as_path())
            .collect();
        let prefix = common_parent_v118(&paths).unwrap();
        assert_eq!(
            std::fs::canonicalize(&prefix).unwrap(),
            std::fs::canonicalize(&trees).unwrap()
        );
        assert_eq!(short_path_v118(&stack[1].path, Some(&prefix)), "…/one");
    }

    #[test]
    fn short_paths_v118_use_tilde_and_need_two_paths_for_a_prefix() {
        let home = PathBuf::from(std::env::var_os("HOME").unwrap_or_default());
        if home.as_os_str().len() > 1 {
            assert_eq!(tilde_v118(&home.join("CODE/x")), "~/CODE/x");
        }
        assert_eq!(common_parent_v118(&[Path::new("/a/b/c")]), None);
        assert_eq!(
            common_parent_v118(&[Path::new("/a/b/c"), Path::new("/a/b/d")]),
            Some(PathBuf::from("/a/b"))
        );
        assert_eq!(
            common_parent_v118(&[Path::new("/a/c"), Path::new("/b/d")]),
            None
        );
        assert_eq!(
            short_path_v118(Path::new("/x/y"), Some(Path::new("/a"))),
            "/x/y"
        );
    }

    #[test]
    fn core_worktree_mismatch_v118_lists_choices_and_unset_checks_the_previewed_value() {
        let temp = tempfile::tempdir().unwrap();
        let checkout = temp.path().join("checkout");
        let elsewhere = temp.path().join("elsewhere");
        std::fs::create_dir(&checkout).unwrap();
        std::fs::create_dir(&elsewhere).unwrap();
        git(&checkout, &["init", "-q", "--initial-branch=main"]);
        std::fs::write(checkout.join("a.txt"), "a\n").unwrap();
        git(&checkout, &["add", "a.txt"]);
        git(&checkout, &["commit", "-qm", "first"]);
        git(&checkout, &["branch", "side"]);
        let linked = temp.path().join("linked");
        git(
            &checkout,
            &["worktree", "add", "-q", linked.to_str().unwrap(), "side"],
        );
        let cancel = CancellationToken::default();
        assert_eq!(core_worktree_mismatch_v118(&checkout, &cancel), None);
        git(
            &checkout,
            &["config", "core.worktree", elsewhere.to_str().unwrap()],
        );
        let mismatch = core_worktree_mismatch_v118(&checkout, &cancel).unwrap();
        assert_eq!(mismatch.setting, elsewhere.to_str().unwrap());
        assert!(mismatch.configured_exists);
        assert_eq!(
            std::fs::canonicalize(&mismatch.configured).unwrap(),
            std::fs::canonicalize(&elsewhere).unwrap()
        );
        assert_eq!(mismatch.linked.len(), 1);
        assert_eq!(mismatch.linked[0].1.as_deref(), Some("side"));
        assert!(unset_command_v118(&mismatch).ends_with("config --unset core.worktree"));
        // A setting changed after the preview is refused and left alone.
        let mut stale = mismatch.clone();
        stale.setting = "/somewhere/else".into();
        assert!(unset_core_worktree_v118(&stale, &cancel).is_err());
        assert_eq!(
            git(&checkout, &["config", "core.worktree"]).trim(),
            elsewhere.to_str().unwrap()
        );
        unset_core_worktree_v118(&mismatch, &cancel).unwrap();
        assert!(Command::new("git")
            .arg("-C")
            .arg(&checkout)
            .args(["config", "core.worktree"])
            .output()
            .unwrap()
            .stdout
            .is_empty());
        assert_eq!(core_worktree_mismatch_v118(&checkout, &cancel), None);
    }
}
