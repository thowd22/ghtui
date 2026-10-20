use super::*;
use crate::providers::gh::{Actor, PullRequest};
use std::os::unix::fs::PermissionsExt;

fn identity(id: u64) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(id),
        gh_version: "fixture".into(),
    }
}
fn target() -> PrSummary {
    PrSummary {
        pr: PullRequest {
            host: "github.invalid".into(),
            repository: "example/repo".into(),
            number: 7,
            id: "PR_7".into(),
            title: "Review progress fixture".into(),
            state: "OPEN".into(),
            draft: false,
            body: String::new(),
            author: Actor {
                login: "alice".into(),
                actor_type: "User".into(),
                ..Default::default()
            },
            created_at: "2026-10-13T12:00:00Z".into(),
            updated_at: "2026-10-13T12:01:00Z".into(),
            url: "https://github.invalid/example/repo/pull/7".into(),
            queues: BTreeSet::new(),
        },
        head_sha: "a".repeat(40),
        base_sha: "b".repeat(40),
        head_ref: "feature".into(),
        base_ref: "main".into(),
        merged: false,
        auto_merge: false,
        merge_commit_sha: None,
        mergeable: Some(true),
        mergeable_state: "clean".into(),
        additions: 1,
        deletions: 1,
        changed_files: 3,
    }
}
fn files() -> Vec<ViewedFile> {
    [
        ("src/α file.rs", "VIEWED"),
        ("other.rs", "UNVIEWED"),
        ("changed.rs", "DISMISSED"),
    ]
    .map(|(path, state)| ViewedFile {
        path: path.into(),
        state: state.into(),
    })
    .to_vec()
}

#[test]
fn viewed_files_progress_preserves_server_states_exact_paths_and_semantic_labels() {
    let progress = ReviewProgress::prepare(&identity(42), &target(), &files()).unwrap();
    assert!(progress.complete());
    assert!(progress.matches(&identity(42), &target()));
    assert_eq!(progress.selected(0), None);
    assert_eq!(progress.selected(1), None);
    assert_eq!(progress.selected(2).unwrap().path, "src/α file.rs");
    assert_eq!(progress.selected(4).unwrap().state, ViewedState::Changed);
    assert_eq!(progress.selected(5), None);
    assert_eq!(progress.selected_row("other.rs"), Some(3));
    let rows = progress.rows();
    assert!(rows[0].0.contains("1/3 viewed"));
    assert!(rows[0].0.contains("1 changed since viewed"));
    assert_eq!(rows[2].1, SemanticToken::Success);
    assert_eq!(rows[4].1, SemanticToken::Pending);
    assert!(rows[4].0.contains("CHANGED SINCE VIEWED"));
    assert_eq!(ViewedState::Changed.server_value(), Some("DISMISSED"));
}

#[test]
fn viewed_files_progress_missing_unknown_and_partial_never_become_unviewed_or_zero() {
    let mut loaded = files();
    loaded[0].state.clear();
    loaded[1].state = "future-host-state".into();
    let progress = ReviewProgress::prepare(&identity(42), &target(), &loaded).unwrap();
    assert!(!progress.complete());
    assert_eq!(progress.files()[0].state, ViewedState::Unavailable);
    assert_eq!(progress.files()[1].state, ViewedState::Unavailable);
    assert!(progress.rows()[0].0.contains("PARTIAL / UNAVAILABLE"));
    assert!(progress.rows()[0].0.contains("missing states are not zero"));
    let missing = ReviewProgress::prepare(&identity(42), &target(), &[]).unwrap();
    assert!(!missing.complete());
    assert!(missing.rows()[0].0.contains("PARTIAL / UNAVAILABLE"));
    assert!(!missing.rows()[0].0.contains("0/0 viewed"));
}

#[test]
fn viewed_files_progress_rejects_duplicate_invalid_paths_and_bounds_metadata() {
    let mut loaded = files();
    loaded.push(loaded[0].clone());
    assert!(ReviewProgress::prepare(&identity(42), &target(), &loaded).is_err());
    for path in ["", "/absolute", "a/../b", "a\0b"] {
        assert!(ReviewProgress::prepare(
            &identity(42),
            &target(),
            &[ViewedFile {
                path: path.into(),
                state: "VIEWED".into()
            }],
        )
        .is_err());
    }
    let path = "x".repeat(4097);
    assert!(ReviewProgress::prepare(
        &identity(42),
        &target(),
        &[ViewedFile {
            path,
            state: "VIEWED".into()
        }]
    )
    .is_err());
    let too_many = (0..10_001)
        .map(|index| ViewedFile {
            path: format!("file-{index}"),
            state: "UNVIEWED".into(),
        })
        .collect::<Vec<_>>();
    assert!(ReviewProgress::prepare(&identity(42), &target(), &too_many).is_err());
    let oversized = (0..1024)
        .map(|index| ViewedFile {
            path: format!("{index}-{}", "x".repeat(2048)),
            state: "UNVIEWED".into(),
        })
        .collect::<Vec<_>>();
    assert!(ReviewProgress::prepare(&identity(42), &target(), &oversized).is_err());
}

#[test]
fn viewed_files_progress_requires_positive_scope_and_current_original_pr_comparison() {
    let progress = ReviewProgress::prepare(&identity(42), &target(), &files()).unwrap();
    assert!(!progress.matches(&identity(43), &target()));
    let mut unavailable = identity(42);
    unavailable.account_id = None;
    assert!(ReviewProgress::prepare(&unavailable, &target(), &files()).is_err());
    unavailable.account_id = Some(0);
    assert!(ReviewProgress::prepare(&unavailable, &target(), &files()).is_err());
    let mut changed = target();
    changed.head_sha = "c".repeat(40);
    assert!(!progress.matches(&identity(42), &changed));
    assert!(require_same_target(&target(), &changed).is_err());
    changed = target();
    changed.pr.id = "RECREATED".into();
    assert!(!progress.matches(&identity(42), &changed));
    assert!(require_same_target(&target(), &changed).is_err());
    changed = target();
    changed.base_sha = "c".repeat(40);
    assert!(!progress.matches(&identity(42), &changed));
    changed = target();
    changed.pr.repository = "example/other".into();
    assert!(!progress.matches(&identity(42), &changed));
    changed = target();
    changed.pr.number = 8;
    assert!(!progress.matches(&identity(42), &changed));
    assert_ne!(
        file_resource("src/α file.rs"),
        file_resource("src/α file2.rs")
    );
    assert_eq!(file_resource("src/α file.rs").len(), 69);
}

#[test]
fn viewed_files_progress_restart_restores_only_account_bound_position_not_server_state() {
    let directory = tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let database = directory.path().join("cache.sqlite3");
    let old = ReviewProgress::prepare(&identity(42), &target(), &files()).unwrap();
    old.remember("src/α file.rs", &database).unwrap();
    assert!(old.remember("outside.rs", &database).is_err());
    let mut changed_target = target();
    changed_target.head_sha = "c".repeat(40);
    let mut changed_files = files();
    changed_files[0].state = "DISMISSED".into();
    let mut fresh =
        ReviewProgress::prepare(&identity(42), &changed_target, &changed_files).unwrap();
    fresh.restore_position(&database).unwrap();
    assert_eq!(fresh.resume_path.as_deref(), Some("src/α file.rs"));
    assert_eq!(fresh.files()[0].state, ViewedState::Changed);
    assert!(!fresh.rows()[0].0.contains("1/3 viewed"));
    let mut other =
        ReviewProgress::prepare(&identity(43), &changed_target, &changed_files).unwrap();
    other.restore_position(&database).unwrap();
    assert!(other.resume_path.is_none());
    let mut removed =
        ReviewProgress::prepare(&identity(42), &changed_target, &changed_files[1..]).unwrap();
    removed.restore_position(&database).unwrap();
    assert!(removed.resume_path.is_none());
    let stored = Cache::open(&database)
        .unwrap()
        .saved_view(
            &Scope::github("github.invalid", Some(42)).unwrap(),
            "pr-review-position:v1:PR_7",
        )
        .unwrap()
        .unwrap();
    assert_eq!(stored["path"], "src/α file.rs");
    assert!(stored.get("viewed").is_none());
    assert!(stored.get("state").is_none());
}

#[test]
fn viewed_files_unsupported_host_state_disables_marking_and_never_counts_as_zero() {
    // A host/account that omits viewerViewedState (or reports a state this
    // client does not know) exposes no capability: both marks stay disabled.
    let mut unsupported = files();
    unsupported[1].state = String::new();
    unsupported[2].state = "FUTURE_STATE".into();
    let progress = ReviewProgress::prepare(&identity(42), &target(), &unsupported).unwrap();
    assert!(!progress.complete());
    let rows = progress.rows();
    assert!(rows[0]
        .0
        .starts_with("PARTIAL / UNAVAILABLE review progress"));
    assert!(rows[0].0.contains("missing states are not zero"));
    assert_eq!(rows[0].1, SemanticToken::Pending);
    let unknown = progress.selected(3).unwrap();
    assert_eq!(unknown.state, ViewedState::Unavailable);
    assert_eq!(rows[3].1, SemanticToken::Muted);
    assert!(rows[3].0.starts_with("VIEWED STATE UNAVAILABLE • "));
    assert_eq!(unknown.state.server_value(), None);
    for viewed in [true, false] {
        assert!(file_disabled_reason(&unknown, viewed)
            .unwrap()
            .contains("capability is unavailable"));
    }
    // Known states disable only the no-op direction.
    let viewed = progress.selected(2).unwrap();
    assert!(file_disabled_reason(&viewed, true)
        .unwrap()
        .contains("already VIEWED"));
    assert_eq!(file_disabled_reason(&viewed, false), None);
    let changed = FileProgress {
        path: "changed.rs".into(),
        state: ViewedState::Changed,
    };
    assert_eq!(file_disabled_reason(&changed, true), None);
    assert_eq!(file_disabled_reason(&changed, false), None);
}
