use super::*;
use crate::git::{Comparison, GitRepository};
use crate::providers::runner::CancellationToken;
use std::path::Path;

fn git(dir: &Path, args: &[&str]) {
    let status = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=Cache Test",
            "-c",
            "user.email=cache@test.invalid",
        ])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

/// A repository whose HEAD rewrites `notes.txt` (and renames nothing).
fn repository() -> (tempfile::TempDir, GitRepository) {
    let dir = tempfile::tempdir().unwrap();
    git(dir.path(), &["init", "-q", "-b", "main"]);
    let old: String = (0..40).map(|line| format!("line {line}\n")).collect();
    std::fs::write(dir.path().join("notes.txt"), &old).unwrap();
    git(dir.path(), &["add", "notes.txt"]);
    git(dir.path(), &["commit", "-q", "-m", "old"]);
    let new = old
        .replace("line 5\n", "line five\n")
        .replace("line 30\n", "line thirty\n");
    std::fs::write(dir.path().join("notes.txt"), new).unwrap();
    git(dir.path(), &["commit", "-q", "-am", "new"]);
    let repo = GitRepository::discover_with_cancellation(dir.path(), CancellationToken::default())
        .unwrap()
        .unwrap();
    (dir, repo)
}

/// The real-repository tests share the process-wide cache; one at a time keeps their hit
/// counts and the eviction exact.
fn serial() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn head_changes() -> Comparison {
    Comparison::CommitChanges {
        commit: "HEAD".into(),
        parent: 0,
    }
}

#[test]
fn comparison_cache_v110_reopening_the_same_comparison_reuses_it() {
    let _serial = serial();
    let (_dir, repo) = repository();
    let first = repo.compare_reusing_v110(&head_changes()).unwrap();
    assert_eq!(repo.comparison_cache_hits_v110(), 0);
    let second = repo.compare_reusing_v110(&head_changes()).unwrap();
    assert_eq!(repo.comparison_cache_hits_v110(), 1, "the reopen was a hit");
    assert_eq!(first, second, "the reused comparison is identical");
    // A fresh handle on the same repository (as each workspace load makes) reuses it too.
    let again = GitRepository::discover_with_cancellation(&repo.root, CancellationToken::default())
        .unwrap()
        .unwrap();
    assert_eq!(again.compare_reusing_v110(&head_changes()).unwrap(), first);
    assert_eq!(repo.comparison_cache_hits_v110(), 2);
    // The same saved versions asked for by full id keep the caller's question.
    let head = second.head.clone().unwrap();
    let by_id = Comparison::CommitChanges {
        commit: head,
        parent: 0,
    };
    let reused = repo.compare_reusing_v110(&by_id).unwrap();
    assert_eq!(reused.comparison, by_id);
    assert_eq!(reused.diff, first.diff);
    assert_eq!(repo.comparison_cache_hits_v110(), 3);
    // Plain compare (the native diff sides' exact read sequence) never consults the cache.
    assert_eq!(repo.compare(&head_changes()).unwrap(), first);
    assert_eq!(repo.comparison_cache_hits_v110(), 3);
}

#[test]
fn comparison_cache_v110_changed_inputs_recompute() {
    let _serial = serial();
    let (dir, repo) = repository();
    let first = repo.compare_reusing_v110(&head_changes()).unwrap();
    let hunks = first.diff.files[0].hunks.len();
    assert_eq!(hunks, 2, "default context keeps the two edits apart");

    // A configuration change (diff context) recomputes: one hunk per edit becomes one hunk.
    git(dir.path(), &["config", "diff.context", "20"]);
    let wider = repo.compare_reusing_v110(&head_changes()).unwrap();
    assert_eq!(repo.comparison_cache_hits_v110(), 0, "config change missed");
    assert_eq!(wider.diff.files[0].hunks.len(), 1);
    git(dir.path(), &["config", "--unset", "diff.context"]);

    // An attribute change on a changed path recomputes: the file is now binary.
    let back = repo.compare_reusing_v110(&head_changes()).unwrap();
    assert_eq!(back.diff, first.diff, "the original comparison again");
    let hits_before = repo.comparison_cache_hits_v110();
    std::fs::write(dir.path().join(".gitattributes"), "notes.txt -diff\n").unwrap();
    let binary = repo.compare_reusing_v110(&head_changes()).unwrap();
    assert_eq!(
        repo.comparison_cache_hits_v110(),
        hits_before,
        "attribute change missed"
    );
    assert!(binary.diff.files[0].binary, "{:?}", binary.diff.files[0]);
    std::fs::remove_file(dir.path().join(".gitattributes")).unwrap();

    // Different saved versions (another parent / other commits) are different inputs.
    let snapshots = repo
        .compare_reusing_v110(&Comparison::Snapshots {
            base: "HEAD".into(),
            head: "HEAD~1".into(),
        })
        .unwrap();
    assert_ne!(snapshots.diff, first.diff);
    assert_eq!(snapshots.description, "Compare two saved versions");
    // Working-tree comparisons are never cached.
    let hits = repo.comparison_cache_hits_v110();
    repo.compare_reusing_v110(&Comparison::Working).unwrap();
    repo.compare_reusing_v110(&Comparison::Working).unwrap();
    assert_eq!(repo.comparison_cache_hits_v110(), hits);
}

fn key(name: &str) -> ComparisonKeyV110 {
    ComparisonKeyV110 {
        common_dir: PathBuf::from("/cache-test/.git"),
        root: PathBuf::from("/cache-test"),
        args: vec!["diff".into(), name.into()],
        capture_limit: 1024,
        description: "d".into(),
        base: None,
        head: None,
        common_ancestor: None,
        config: Vec::new(),
        diff_opts: None,
    }
}

fn cached(patch_bytes: usize) -> CachedComparisonV110 {
    CachedComparisonV110 {
        result: Arc::new(ComparisonResult {
            comparison: Comparison::Snapshots {
                base: "a".into(),
                head: "b".into(),
            },
            description: "d".into(),
            base: None,
            head: None,
            common_ancestor: None,
            diff: crate::diff::Diff::default(),
            patch: vec![b'x'; patch_bytes],
            observed: None,
        }),
        attribute_paths: Vec::new(),
        attributes: Vec::new(),
    }
}

#[test]
fn comparison_cache_v110_memory_budget_evicts_least_recently_used() {
    let one = estimated_cost_v110(&key("a"), &cached(10_000));
    let mut cache = ComparisonCacheV110::new(one * 2 + one / 2, 8);
    assert!(cache.insert(key("a"), cached(10_000)));
    assert!(cache.insert(key("b"), cached(10_000)));
    assert!(cache.hit(&key("a")), "a is now the most recently used");
    assert!(cache.insert(key("c"), cached(10_000)));
    assert!(cache.contains(&key("a")) && cache.contains(&key("c")));
    assert!(!cache.contains(&key("b")), "b was least recently used");
    assert!(cache.used() <= one * 2 + one / 2);
    // Larger than the whole budget: not kept, nothing else lost.
    assert!(!cache.insert(key("huge"), cached(one * 4)));
    assert_eq!(cache.len(), 2);
    // The entry count is bounded too.
    let mut few = ComparisonCacheV110::new(usize::MAX, 2);
    for name in ["a", "b", "c"] {
        assert!(few.insert(key(name), cached(1)));
    }
    assert_eq!(few.len(), 2);
    assert!(!few.contains(&key("a")));
    // Replacing a key keeps the accounting exact.
    assert!(few.insert(key("c"), cached(1)));
    assert_eq!(few.len(), 2);
    few.remove(&key("b"));
    few.remove(&key("c"));
    assert_eq!((few.len(), few.used()), (0, 0));
}

#[test]
fn comparison_cache_v110_evicted_comparison_is_recomputed() {
    let _serial = serial();
    let (_dir, repo) = repository();
    let first = repo.compare_reusing_v110(&head_changes()).unwrap();
    // Push this repository's entry out of the shared cache under memory pressure.
    {
        let mut cache = shared_comparison_cache_v110()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for index in 0..COMPARISON_CACHE_ENTRIES_V110 {
            cache.insert(key(&format!("pressure-{index}")), cached(1));
        }
    }
    let again = repo.compare_reusing_v110(&head_changes()).unwrap();
    assert_eq!(repo.comparison_cache_hits_v110(), 0, "evicted: recomputed");
    assert_eq!(again, first);
    {
        let mut cache = shared_comparison_cache_v110()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for index in 0..COMPARISON_CACHE_ENTRIES_V110 {
            cache.remove(&key(&format!("pressure-{index}")));
        }
    }
}
