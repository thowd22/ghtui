//! Read-only stash contents, worktree dirty status (TASK-41) and structured bug-search
//! progress (TASK-44). Nothing here writes to the repository.
use super::{
    original_actor_saved_sha, run_git_with_kind, GitError, GitRepository, GitResult, RawPath,
};
use std::path::Path;

/// One exact stash: tracked changes against its base, and the untracked files it saved
/// (`None` when the stash saved no untracked files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashContentsV41 {
    pub patch: Vec<u8>,
    pub untracked: Option<Vec<RawPath>>,
}

impl GitRepository {
    /// Non-writing `status` inside one listed worktree with this repository's cancellation.
    pub fn worktree_dirty_v41(&self, path: &Path) -> GitResult<bool> {
        let output = run_git_with_kind(
            path,
            ["status", "--porcelain", "-z", "--untracked-files=all"],
            None,
            false,
            &self.cancellation,
        )?;
        if !output.success {
            return Err(output.error("Inspect worktree status"));
        }
        Ok(!output.stdout.is_empty())
    }

    pub fn stash_contents_v41(&self, id: &str) -> GitResult<StashContentsV41> {
        if !original_actor_saved_sha(id) {
            return Err(GitError("Stash view needs the exact full stash SHA".into()));
        }
        let base = format!("{id}^1");
        let patch = self.read(&[
            "diff",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            &base,
            id,
            "--",
        ])?;
        let third = format!("{id}^3");
        let untracked = if self
            .run(["rev-parse", "--verify", "--quiet", third.as_str()], None)?
            .success
        {
            let bytes = self.read(&["ls-tree", "-r", "-z", "--name-only", &third])?;
            Some(
                bytes
                    .split(|byte| *byte == 0)
                    .filter(|name| !name.is_empty())
                    .map(|name| RawPath::new(name.to_vec()))
                    .collect(),
            )
        } else {
            None
        };
        Ok(StashContentsV41 { patch, untracked })
    }
}

/// Structured bug-search progress parsed from `git bisect` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BisectProgressV44 {
    pub candidate: String,
    pub subject: String,
    pub remaining: usize,
    pub steps: usize,
}

/// `Bisecting: N revision(s) left to test after this (roughly M step(s))` followed by
/// `[<full sha>] subject`. Any other shape is None; nothing is guessed.
pub fn bisect_progress_v44(text: &str) -> Option<BisectProgressV44> {
    let mut lines = text.lines().map(str::trim);
    let header = lines.find(|line| line.starts_with("Bisecting: "))?;
    let (remaining, rest) = header.strip_prefix("Bisecting: ")?.split_once(' ')?;
    let remaining = remaining.parse().ok()?;
    let steps = rest
        .split_once("(roughly ")
        .and_then(|(_, tail)| tail.split_once(' '))
        .and_then(|(count, _)| count.parse().ok())?;
    let (candidate, subject) = lines.next()?.strip_prefix('[')?.split_once(']')?;
    if !original_actor_saved_sha(candidate) {
        return None;
    }
    Some(BisectProgressV44 {
        candidate: candidate.into(),
        subject: subject.trim().into(),
        remaining,
        steps,
    })
}

/// Git stops with "only 'skip'ped commits left" when every remaining version was skipped.
pub fn bisect_only_skipped_v44(text: &str) -> bool {
    text.contains("only 'skip'ped commits left")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn guided_bisect_progress_parses_candidate_and_remaining_scope() {
        let sha = "1f0aba9f5d9e4dba3192b6caa20f0c982e0a2387";
        let progress = bisect_progress_v44(&format!(
            "Bisecting: 3 revisions left to test after this (roughly 2 steps)\n[{sha}] version 3\n"
        ))
        .unwrap();
        assert_eq!(
            progress,
            BisectProgressV44 {
                candidate: sha.into(),
                subject: "version 3".into(),
                remaining: 3,
                steps: 2,
            }
        );
        let single = bisect_progress_v44(&format!(
            "Bisecting: 0 revisions left to test after this (roughly 0 steps)\n[{sha}] c\n"
        ))
        .unwrap();
        assert_eq!((single.remaining, single.steps), (0, 0));
        assert!(bisect_progress_v44("Bisecting: 1 revision left\n[short] c").is_none());
        assert!(bisect_progress_v44(&format!("{sha} is the first bad commit")).is_none());
        assert!(bisect_only_skipped_v44(
            "There are only 'skip'ped commits left to test.\nThe first bad commit could be any of:"
        ));
        assert!(!bisect_only_skipped_v44("Bisecting: 1 revision left"));
    }
}
