mod git_support;
#[path = "support/providers.rs"]
#[allow(dead_code)] // Managed-clone contracts use the strict API fixture subset.
mod provider_support;
use ghtui::{
    git::{GitRepository, RawPath},
    providers::runner::CancellationToken,
    search::{
        corpus::LocalSource,
        managed::{CloneSource, CorpusManager, CorpusOperation, CorpusOwnership, CorpusPhase},
    },
    storage::{Cache, Record, Scope},
};
use git_support::{git, Fixture};
use provider_support::GhStep;
use serde_json::{json, Value};
use std::{
    ffi::OsStr,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::Path,
    time::{Duration, Instant},
};

fn scope() -> Scope {
    Scope::github("github.invalid", Some(7)).unwrap()
}
fn manager(fixture: &Fixture) -> CorpusManager {
    CorpusManager::new(fixture.directory.path().join("managed"), scope()).unwrap()
}
fn raw(path: &Path) -> RawPath {
    RawPath::new(path.as_os_str().as_bytes().to_vec())
}
fn bare(fixture: &Fixture) -> std::path::PathBuf {
    let path = fixture.directory.path().join("source bare.git");
    git(
        &fixture.root,
        [
            OsStr::new("clone"),
            OsStr::new("--bare"),
            fixture.root.as_os_str(),
            path.as_os_str(),
        ],
    );
    path
}
fn clone_operation(path: &Path) -> CorpusOperation {
    CorpusOperation::Clone {
        repository: "org/app".into(),
        source: CloneSource::LocalBare(raw(path)),
        depth: Some(1),
    }
}
fn apply(
    manager: &CorpusManager,
    cache: &Cache,
    operation: CorpusOperation,
) -> ghtui::search::managed::CorpusOutcome {
    let plan = manager
        .prepare(cache, operation, &CancellationToken::default())
        .unwrap();
    manager
        .execute(cache, &plan, true, &CancellationToken::default())
        .unwrap()
}

fn github_repository() -> ghtui::providers::gh::Repository {
    serde_json::from_value(json!({
        "host":"github.invalid", "id":101, "full_name":"org/app",
        "description":"selected fixture", "default_branch":"main",
        "private":true, "archived":false,
        "permissions":{"pull":true,"push":false,"maintain":false,"admin":false}
    }))
    .unwrap()
}

fn github_identity_steps(id: u64, repo_id: u64, name: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        GhStep::read(
            &[
                "api",
                "user",
                "--hostname",
                "github.invalid",
                "--method",
                "GET",
            ],
            &json!({"login":"alice","id":id}).to_string(),
        ),
        GhStep::read(
            &[
                "api",
                "repos/org/app",
                "--hostname",
                "github.invalid",
                "--method",
                "GET",
            ],
            &json!({"id":repo_id,"full_name":name,"default_branch":"main","private":true,
                "permissions":{"pull":true,"push":false,"maintain":false,"admin":false}})
            .to_string(),
        ),
    ]
}

fn provider_journal(fixture: &provider_support::Fixture) -> Vec<Value> {
    std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn selected_corpus_mapping_unmap_preserves_dirty_index_ignored_bytes_and_account_boundaries() {
    let fixture = Fixture::linear();
    fixture.write("readme.txt", b"user unsaved bytes\n");
    fixture.write(".gitignore", b"ignored.env\n");
    fixture.write("ignored.env", b"private fixture bytes\n");
    let head = fixture.head();
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let cache = Cache::memory().unwrap();
    let manager = manager(&fixture);
    let operation = CorpusOperation::Map {
        repository: "org/app".into(),
        directory: raw(&fixture.root),
        source: LocalSource::Worktree,
    };
    let plan = manager
        .prepare(&cache, operation.clone(), &CancellationToken::default())
        .unwrap();
    assert!(plan.effect.contains("Never clone"));
    assert!(manager
        .execute(&cache, &plan, false, &CancellationToken::default())
        .is_err());
    assert!(!manager.root.exists());
    assert!(manager
        .catalog(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    let outcome = apply(&manager, &cache, operation);
    let mapping = outcome.mapping.unwrap();
    assert_eq!(mapping.ownership, CorpusOwnership::Existing);
    assert!(mapping.active);
    assert_eq!(mapping.saved_commit.as_deref(), Some(head.as_str()));
    assert!(mapping.disk_bytes > 0);
    assert!(mapping.selection().is_some());
    assert!(manager
        .prepare(
            &cache,
            CorpusOperation::Quarantine {
                repository: "org/app".into()
            },
            &CancellationToken::default()
        )
        .unwrap_err()
        .0
        .contains("user checkouts are unmap-only"));
    apply(
        &manager,
        &cache,
        CorpusOperation::Unmap {
            repository: "org/app".into(),
        },
    );
    assert!(
        !manager
            .catalog(&cache, &CancellationToken::default())
            .unwrap()[0]
            .active
    );
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.root.join("readme.txt")).unwrap(),
        b"user unsaved bytes\n"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("ignored.env")).unwrap(),
        b"private fixture bytes\n"
    );
    let other = CorpusManager::new(
        manager.root.clone(),
        Scope::github("github.invalid", Some(8)).unwrap(),
    )
    .unwrap();
    assert!(other
        .catalog(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert!(other
        .recoveries(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
}

#[test]
fn selected_corpus_real_bare_clone_fetch_saved_scope_moves_without_checkout_or_local_head_change() {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let head = fixture.head();
    let cache = Cache::memory().unwrap();
    let manager = manager(&fixture);
    let operation = clone_operation(&remote);
    let plan = manager
        .prepare(&cache, operation.clone(), &CancellationToken::default())
        .unwrap();
    assert!(plan
        .preview
        .iter()
        .any(|line| line.contains("size unknown")));
    assert!(plan.effect.contains("no worktree checkout"));
    assert!(!manager.managed_destination("org/app").unwrap().exists());
    let mapping = apply(&manager, &cache, operation).mapping.unwrap();
    assert!(matches!(mapping.ownership, CorpusOwnership::Managed { .. }));
    assert!(!mapping.path().join("readme.txt").exists());
    let local = GitRepository::discover(mapping.path()).unwrap().unwrap();
    assert!(local.status().unwrap().shallow);
    assert_eq!(local.resolve_original("HEAD").unwrap(), head);
    assert_eq!(
        local
            .blob(&head, &RawPath::new(b"readme.txt".to_vec()))
            .unwrap(),
        b"first\nchanged\n"
    );
    fixture.write("readme.txt", b"new fetched source\n");
    let updated = fixture.commit("Upstream advanced");
    git(
        &fixture.root,
        [
            OsStr::new("push"),
            remote.as_os_str(),
            OsStr::new("HEAD:refs/heads/main"),
        ],
    );
    let outcome = apply(
        &manager,
        &cache,
        CorpusOperation::Fetch {
            repository: "org/app".into(),
            remote: "origin".into(),
        },
    );
    let current = outcome.mapping.unwrap();
    assert_eq!(current.saved_commit.as_deref(), Some(updated.as_str()));
    assert_eq!(
        local.resolve_original("HEAD").unwrap(),
        head,
        "Fetch never checks out or moves a local saved branch"
    );
    assert!(!mapping.path().join("readme.txt").exists());
    assert_eq!(
        local
            .blob(&updated, &RawPath::new(b"readme.txt".to_vec()))
            .unwrap(),
        b"new fetched source\n"
    );
    assert_eq!(
        current.source,
        LocalSource::Revision("refs/remotes/origin/HEAD".into())
    );
    assert!(current.disk_complete);
    // Size and revision scope are what the UI shows for this mapping.
    let footprint = current.footprint();
    assert!(footprint.contains(" on disk • saved refs/remotes/origin/HEAD = "));
    assert!(footprint.ends_with(&updated[..12]));
    // A force-rewritten upstream ref moves the saved scope; old-only content leaves it.
    git(
        &fixture.root,
        [
            OsStr::new("reset"),
            OsStr::new("--hard"),
            OsStr::new("HEAD~1"),
        ],
    );
    fixture.write("readme.txt", b"rewritten source\n");
    let rewritten = fixture.commit("Upstream rewritten");
    assert_ne!(rewritten, updated);
    git(
        &fixture.root,
        [
            OsStr::new("push"),
            OsStr::new("--force"),
            remote.as_os_str(),
            OsStr::new("HEAD:refs/heads/main"),
        ],
    );
    let rewritten_mapping = apply(
        &manager,
        &cache,
        CorpusOperation::Fetch {
            repository: "org/app".into(),
            remote: "origin".into(),
        },
    )
    .mapping
    .unwrap();
    assert_eq!(
        rewritten_mapping.saved_commit.as_deref(),
        Some(rewritten.as_str())
    );
    assert_eq!(local.resolve_original("HEAD").unwrap(), head);
    let corpus = ghtui::search::corpus::Corpus::collect(
        scope(),
        &[rewritten_mapping.selection().unwrap()],
        &ghtui::search::corpus::CorpusLimits::default(),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(corpus
        .documents
        .iter()
        .any(|document| document.body.contains("rewritten source")));
    assert!(!corpus
        .documents
        .iter()
        .any(|document| document.body.contains("new fetched source")));
}

#[test]
fn selected_corpus_owned_quarantine_restore_retains_ignored_untracked_and_collision_bytes() {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let cache = Cache::memory().unwrap();
    let manager = manager(&fixture);
    let mapping = apply(&manager, &cache, clone_operation(&remote))
        .mapping
        .unwrap();
    std::fs::write(mapping.path().join(".gitignore"), b"ignored.env\n").unwrap();
    std::fs::write(
        mapping.path().join("ignored.env"),
        b"ignored but must preserve\n",
    )
    .unwrap();
    std::fs::write(mapping.path().join("note"), b"untracked user note\n").unwrap();
    let outcome = apply(
        &manager,
        &cache,
        CorpusOperation::Quarantine {
            repository: "org/app".into(),
        },
    );
    let recovery = outcome.recovery.unwrap();
    let recovered = std::path::PathBuf::from(recovery.directory.as_os_str());
    assert!(recovery.restorable);
    assert!(!mapping.path().exists());
    assert_eq!(
        std::fs::read(recovered.join("ignored.env")).unwrap(),
        b"ignored but must preserve\n"
    );
    assert_eq!(
        std::fs::read(recovered.join("note")).unwrap(),
        b"untracked user note\n"
    );
    assert!(
        !manager
            .catalog(&cache, &CancellationToken::default())
            .unwrap()[0]
            .active
    );
    std::fs::create_dir(mapping.path()).unwrap();
    std::fs::write(
        mapping.path().join("user.txt"),
        b"unrelated collision bytes",
    )
    .unwrap();
    let restore = CorpusOperation::Restore {
        repository: "org/app".into(),
        recovery_id: recovery.id.clone(),
    };
    assert!(manager
        .prepare(&cache, restore.clone(), &CancellationToken::default())
        .unwrap_err()
        .0
        .contains("never overwrite"));
    assert_eq!(
        std::fs::read(mapping.path().join("user.txt")).unwrap(),
        b"unrelated collision bytes"
    );
    assert!(recovered.exists());
    let preserved = fixture
        .directory
        .path()
        .join("unrelated collision preserved");
    std::fs::rename(mapping.path(), &preserved).unwrap();
    let restored = apply(&manager, &cache, restore).mapping.unwrap();
    assert!(restored.active);
    assert!(!recovered.exists());
    assert_eq!(
        std::fs::read(restored.path().join("ignored.env")).unwrap(),
        b"ignored but must preserve\n"
    );
    assert_eq!(
        std::fs::read(preserved.join("user.txt")).unwrap(),
        b"unrelated collision bytes"
    );
    assert!(manager
        .recoveries(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
}

#[test]
fn selected_corpus_managed_owner_symlink_occupied_and_stale_source_guards_preserve_targets() {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let cache = Cache::memory().unwrap();
    let manager = manager(&fixture);
    let plan = manager
        .prepare(
            &cache,
            clone_operation(&remote),
            &CancellationToken::default(),
        )
        .unwrap();
    fixture.write("next", b"changed source");
    fixture.commit("Source moved");
    git(
        &fixture.root,
        [
            OsStr::new("push"),
            remote.as_os_str(),
            OsStr::new("HEAD:refs/heads/main"),
        ],
    );
    assert!(manager
        .execute(&cache, &plan, true, &CancellationToken::default())
        .unwrap_err()
        .0
        .contains("stale"));
    assert!(!manager.managed_destination("org/app").unwrap().exists());
    let mapping = apply(&manager, &cache, clone_operation(&remote))
        .mapping
        .unwrap();
    let owner = mapping.path().join(".git/ghtui-managed-owner.json");
    let original = std::fs::read(&owner).unwrap();
    std::fs::write(&owner, b"corrupt unrelated ownership").unwrap();
    assert!(manager
        .prepare(
            &cache,
            CorpusOperation::Quarantine {
                repository: "org/app".into()
            },
            &CancellationToken::default()
        )
        .is_err());
    assert!(mapping.path().exists());
    std::fs::write(&owner, &original).unwrap();
    let symlink = fixture.directory.path().join("symlink-managed-root");
    std::os::unix::fs::symlink(&manager.root, &symlink).unwrap();
    assert!(CorpusManager::new(symlink, scope()).is_err());
    assert!(CorpusManager::new(std::path::PathBuf::from("/"), scope()).is_err());
    assert!(CorpusManager::new(
        manager.root.clone(),
        Scope::new("github.invalid", "alice").unwrap()
    )
    .is_err());
    let occupied = manager.managed_destination("org/occupied").unwrap();
    std::fs::create_dir(&occupied).unwrap();
    std::fs::write(occupied.join("user-data"), b"preserve").unwrap();
    assert!(manager
        .prepare(
            &cache,
            CorpusOperation::Clone {
                repository: "org/occupied".into(),
                source: CloneSource::LocalBare(raw(&remote)),
                depth: Some(1)
            },
            &CancellationToken::default()
        )
        .is_err());
    assert_eq!(
        std::fs::read(occupied.join("user-data")).unwrap(),
        b"preserve"
    );
}

#[test]
fn selected_corpus_fetch_preview_binds_actual_remote_configuration() {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let cache = Cache::memory().unwrap();
    let manager = manager(&fixture);
    let mapping = apply(&manager, &cache, clone_operation(&remote))
        .mapping
        .unwrap();
    let plan = manager
        .prepare(
            &cache,
            CorpusOperation::Fetch {
                repository: "org/app".into(),
                remote: "origin".into(),
            },
            &CancellationToken::default(),
        )
        .unwrap();
    let changed = fixture.directory.path().join("different bare.git");
    git(
        &fixture.root,
        [
            OsStr::new("clone"),
            OsStr::new("--bare"),
            fixture.root.as_os_str(),
            changed.as_os_str(),
        ],
    );
    git(
        &mapping.path(),
        [
            OsStr::new("remote"),
            OsStr::new("set-url"),
            OsStr::new("origin"),
            changed.as_os_str(),
        ],
    );
    assert!(manager
        .execute(&cache, &plan, true, &CancellationToken::default())
        .unwrap_err()
        .0
        .contains("stale"));
    assert_eq!(
        git(&mapping.path(), ["remote", "get-url", "origin"]).stdout,
        format!("{}\n", changed.display()).as_bytes()
    );
}

#[test]
fn selected_corpus_actual_cancelled_clone_reaps_child_retains_partial_bytes_and_requires_inspection(
) {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let program = fixture.directory.path().join("slow-git");
    let ready = fixture.directory.path().join("clone-ready");
    let pid = fixture.directory.path().join("clone-pid");
    let finished = fixture.directory.path().join("clone-finished");
    let script=format!("#!/bin/sh\ngit \"$@\" || exit $?\nprintf '%s' \"$$\" > '{}'\nprintf ready > '{}'\nsleep 30\nprintf unexpected > '{}'\n",pid.display(),ready.display(),finished.display());
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cache = Cache::memory().unwrap();
    let manager = manager(&fixture).with_git_program(program);
    let progress = manager.progress.clone();
    let target = raw(&manager.managed_destination("org/app").unwrap());
    let observer = CorpusManager::new(manager.root.clone(), scope()).unwrap();
    let cancel = CancellationToken::default();
    let operation = clone_operation(&remote);
    let plan = manager.prepare(&cache, operation.clone(), &cancel).unwrap();
    let token = cancel.clone();
    let worker = std::thread::spawn(move || {
        let result = manager.execute(&cache, &plan, true, &token);
        (manager, cache, result)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !ready.exists() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let started = ready.exists();
    let observation_deadline = Instant::now() + Duration::from_secs(1);
    while progress.snapshot().observed_entries == 0 && Instant::now() < observation_deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    let transferring = progress.snapshot();
    let staging = std::fs::read_dir(&observer.root)
        .unwrap()
        .filter_map(Result::ok)
        .find(|entry| entry.path().join("staging").is_dir())
        .unwrap()
        .path()
        .join("staging");
    let private_transfer = std::fs::read_dir(staging)
        .unwrap()
        .map(Result::unwrap)
        .find(|entry| entry.file_name().to_string_lossy().starts_with("clone-"))
        .unwrap()
        .path();
    let transfer_permissions = std::fs::metadata(private_transfer)
        .unwrap()
        .permissions()
        .mode();
    let orphan_inspection =
        observer.orphans(&Cache::memory().unwrap(), &CancellationToken::default());
    cancel.cancel();
    let (manager, cache, result) = worker.join().unwrap();
    assert!(
        started,
        "Real installed Git clone must finish inside wrapper before cancellation"
    );
    assert_eq!(transferring.phase, CorpusPhase::Transferring);
    assert_eq!(transferring.target, Some(target));
    assert!(transferring.observed_disk_bytes > 0);
    assert!(transferring.observed_entries > 0);
    assert_eq!(
        transfer_permissions & 0o777,
        0o700,
        "The generated clone envelope must be private at creation, not fixed after transfer"
    );
    assert!(
        orphan_inspection
            .unwrap_err()
            .0
            .contains("operation is active"),
        "Orphan inspection must not race the actual live clone worker"
    );
    assert!(result.unwrap_err().0.contains("Partial bytes retained"));
    assert!(!finished.exists());
    let child = std::fs::read_to_string(pid)
        .unwrap()
        .parse::<i32>()
        .unwrap();
    assert_eq!(
        unsafe { libc::kill(child, 0) },
        -1,
        "CommandRunner reaped the actual wrapper child"
    );
    assert!(manager
        .catalog(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    let recovery = manager
        .recoveries(&cache, &CancellationToken::default())
        .unwrap()
        .pop()
        .unwrap();
    assert!(recovery.requires_acknowledgement && !recovery.restorable);
    let retained_progress = progress.snapshot();
    assert_eq!(retained_progress.phase, CorpusPhase::Retained);
    assert_eq!(
        retained_progress.recovery_directory,
        Some(recovery.directory.clone())
    );
    assert!(retained_progress
        .error
        .as_ref()
        .is_some_and(|error| error.len() <= 2048));
    let retained = std::path::PathBuf::from(recovery.directory.as_os_str()).join("checkout");
    let repo = GitRepository::discover(retained).unwrap().unwrap();
    assert_eq!(repo.resolve_original("HEAD").unwrap(), fixture.head());
    assert!(!manager.managed_destination("org/app").unwrap().exists());
    assert!(manager
        .prepare(&cache, operation.clone(), &CancellationToken::default())
        .unwrap_err()
        .0
        .contains("acknowledge"));
    apply(
        &manager,
        &cache,
        CorpusOperation::AcknowledgeRecovery {
            repository: "org/app".into(),
            recovery_id: recovery.id,
        },
    );
    assert!(manager
        .prepare(&cache, operation, &CancellationToken::default())
        .is_ok());
}

#[test]
fn selected_corpus_disk_budget_cancels_and_fake_success_never_publishes_phantom_repo() {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let cache = Cache::memory().unwrap();
    let mut manager = manager(&fixture);
    manager.limits.max_observed_disk_bytes = 1;
    let operation = clone_operation(&remote);
    let plan = manager
        .prepare(&cache, operation, &CancellationToken::default())
        .unwrap();
    let failure = manager
        .execute(&cache, &plan, true, &CancellationToken::default())
        .unwrap_err();
    assert!(failure.0.contains("budget"));
    assert!(manager
        .catalog(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert!(!manager.managed_destination("org/app").unwrap().exists());
    assert_eq!(
        manager
            .recoveries(&cache, &CancellationToken::default())
            .unwrap()
            .len(),
        1
    );
    let independent = CorpusManager::new(
        fixture.directory.path().join("independent-managed"),
        scope(),
    )
    .unwrap();
    let independent_cache = Cache::memory().unwrap();
    let mapping = apply(&independent, &independent_cache, clone_operation(&remote))
        .mapping
        .unwrap();
    assert_eq!(
        GitRepository::discover(mapping.path())
            .unwrap()
            .unwrap()
            .resolve_original("HEAD")
            .unwrap(),
        fixture.head(),
        "An uncertain clone must not block the same account/repository at a different physical root"
    );
    assert_eq!(
        manager.managed_destination("org/app").unwrap(),
        manager.managed_destination("ORG/App").unwrap(),
        "ASCII repository case aliases must preserve one owned target identity"
    );
    let alias = CorpusOperation::Clone {
        repository: "ORG/App".into(),
        source: CloneSource::LocalBare(raw(&remote)),
        depth: Some(1),
    };
    assert!(manager
        .prepare(&cache, alias.clone(), &CancellationToken::default())
        .unwrap_err()
        .0
        .contains("acknowledge"));
    let fresh_cache = Cache::memory().unwrap();
    let alias_manager = CorpusManager::new(manager.root.clone(), scope()).unwrap();
    let alias_plan = alias_manager
        .prepare(&fresh_cache, alias, &CancellationToken::default())
        .unwrap();
    assert!(alias_manager
        .execute(&fresh_cache, &alias_plan, true, &CancellationToken::default())
        .unwrap_err()
        .0
        .contains("Previous write outcome is unknown"),
        "The same physical root/canonical account/repository remains blocked even through case aliases and a different Cache handle"
    );
    assert!(!manager.managed_destination("org/app").unwrap().exists());
    let program = fixture.directory.path().join("empty-success-git");
    std::fs::write(&program, b"#!/bin/sh\nexit 0\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let manager = CorpusManager::new(fixture.directory.path().join("other-managed"), scope())
        .unwrap()
        .with_git_program(program);
    let cache = Cache::memory().unwrap();
    let plan = manager
        .prepare(
            &cache,
            clone_operation(&remote),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(manager
        .execute(&cache, &plan, true, &CancellationToken::default())
        .unwrap_err()
        .0
        .contains("did not create a real repository"));
    assert!(manager
        .catalog(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert!(!manager.managed_destination("org/app").unwrap().exists());
    assert_eq!(
        manager
            .recoveries(&cache, &CancellationToken::default())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn selected_corpus_github_clone_keeps_gh_host_auth_protocol_and_exact_no_upstream_policy() {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let api = provider_support::Fixture::new();
    let mut steps = Vec::new();
    for _ in 0..4 {
        steps.extend(github_identity_steps(7, 101, "org/app"));
    }
    api.scripted_gh(&steps);
    let wrapper = fixture.directory.path().join("managed-gh");
    // API calls use the strict offline fake-gh; only this exact clone argv delegates to installed
    // Git and an explicitly supplied local bare fixture. No network or second authentication.
    std::fs::write(&wrapper, br##"#!/bin/sh
if [ "$1" != repo ]; then exec "$GH_READ_FIXTURE_BIN" "$@"; fi
[ "$#" -eq 10 ] || exit 91
[ "$1" = repo ] && [ "$2" = clone ] && [ "$3" = github.invalid/org/app ] || exit 92
checkout="$4"
[ "$5" = --no-upstream ] && [ "$6" = -- ] && [ "$7" = --no-checkout ] || exit 93
[ "$8" = --single-branch ] && [ "$9" = --no-tags ] && [ "${10}" = --depth=1 ] || exit 94
case "$checkout" in "$GH_MANAGED_ACCOUNT/staging/clone-"*/checkout) ;; *) exit 95 ;; esac
[ "$GIT_TERMINAL_PROMPT" = 0 ] && [ "$GH_PROMPT_DISABLED" = 1 ] || exit 96
printf '%s\n' "$@" > "$GH_CLONE_LOG"
exec git -c protocol.file.allow=always clone --no-local "$GH_BARE" "$checkout" --no-checkout --single-branch --no-tags --depth=1
"##).unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let manager = manager(&fixture);
    let account = manager
        .managed_destination("org/app")
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let log = fixture.directory.path().join("clone-argv");
    let manager = manager
        .with_gh_program(wrapper)
        .with_env("GH_READ_FIXTURE_BIN", api.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", api.root.join("gh-fixture"))
        .with_env("GH_MANAGED_ACCOUNT", account)
        .with_env("GH_CLONE_LOG", &log)
        .with_env("GH_BARE", remote);
    let cache = Cache::memory().unwrap();
    let outcome = apply(
        &manager,
        &cache,
        CorpusOperation::Clone {
            repository: "org/app".into(),
            source: CloneSource::GitHub(github_repository()),
            depth: Some(1),
        },
    );
    let mapping = outcome.mapping.unwrap();
    assert_eq!(
        mapping.saved_commit.as_deref(),
        Some(fixture.head().as_str())
    );
    assert_eq!(
        mapping.clone_source,
        Some(CloneSource::GitHub(github_repository()))
    );
    assert!(!mapping.path().join("readme.txt").exists());
    let cloned = GitRepository::discover(mapping.path()).unwrap().unwrap();
    assert!(cloned.status().unwrap().shallow);
    assert_eq!(
        cloned.remotes().unwrap().len(),
        1,
        "No fork upstream may be added or fetched"
    );
    let argv = std::fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(&argv[..3], ["repo", "clone", "github.invalid/org/app"]);
    assert_eq!(
        &argv[4..],
        [
            "--no-upstream",
            "--",
            "--no-checkout",
            "--single-branch",
            "--no-tags",
            "--depth=1"
        ]
    );
    let commands = provider_journal(&api);
    assert_eq!(commands.len(), 12);
    assert!(commands.iter().all(|record| record["matched"] == true));
    assert!(commands.iter().all(|record| {
        let args = record["args"].as_array().unwrap();
        args[0] == "--version" || (args[0] == "api" && args[5] == "GET")
    }), "Only gh identity/repository reads accompany the explicit gh clone; no auth setup or protocol/global configuration writes");
}

#[test]
fn selected_corpus_github_clone_rechecks_account_and_canonical_repo_before_any_clone() {
    for (id, repo_id, name, expected_calls) in [
        (8, 101, "org/app", 5),
        (7, 999, "org/app", 6),
        (7, 101, "org/renamed", 6),
    ] {
        let fixture = Fixture::linear();
        let api = provider_support::Fixture::new();
        let mut steps = github_identity_steps(7, 101, "org/app");
        steps.extend(github_identity_steps(id, repo_id, name));
        api.scripted_gh(&steps);
        let manager = manager(&fixture)
            .with_gh_program(api.root.join("bin/gh"))
            .with_env("GH_FIXTURE_DIR", api.root.join("gh-fixture"));
        let cache = Cache::memory().unwrap();
        let operation = CorpusOperation::Clone {
            repository: "org/app".into(),
            source: CloneSource::GitHub(github_repository()),
            depth: Some(1),
        };
        let plan = manager
            .prepare(&cache, operation, &CancellationToken::default())
            .unwrap();
        let failure = manager
            .execute(&cache, &plan, true, &CancellationToken::default())
            .unwrap_err();
        assert!(
            failure.0.contains("account") || failure.0.contains("ID/name/default branch"),
            "{failure}"
        );
        assert!(!manager.managed_destination("org/app").unwrap().exists());
        assert!(manager
            .catalog(&cache, &CancellationToken::default())
            .unwrap()
            .is_empty());
        let commands = provider_journal(&api);
        assert_eq!(commands.len(), expected_calls);
        assert!(commands.iter().all(|record| record["matched"] == true));
        assert!(!commands.iter().any(|record| record["args"][0] == "repo"));
    }
}

#[test]
fn selected_corpus_durable_intent_survives_legacy_migration_cache_clear_restart_and_restore() {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let database = fixture.directory.path().join("persistent.sqlite3");
    let cache = Cache::open(&database).unwrap();
    let manager = manager(&fixture);
    let mapping = apply(&manager, &cache, clone_operation(&remote))
        .mapping
        .unwrap();
    let published = manager.progress.snapshot();
    assert_eq!(published.phase, CorpusPhase::Published);
    assert_eq!(published.saved_commit, mapping.saved_commit);
    assert_eq!(published.observed_disk_bytes, mapping.disk_bytes);
    assert!(published.observed_entries > 0 && published.observation_complete);
    std::fs::write(mapping.path().join(".gitignore"), b"ignored.env\n").unwrap();
    std::fs::write(
        mapping.path().join("ignored.env"),
        b"retained private ignored bytes\n",
    )
    .unwrap();
    let recovery = apply(
        &manager,
        &cache,
        CorpusOperation::Quarantine {
            repository: "org/app".into(),
        },
    )
    .recovery
    .unwrap();
    assert!(!mapping.path().exists());
    let intent = cache.saved_views(&scope()).unwrap();
    assert_eq!(intent.len(), 2);
    for (name, payload) in &intent {
        let (kind, id) = name.split_once(':').unwrap();
        cache
            .put(&scope(), &Record::new(kind, id, "org/app", payload.clone()))
            .unwrap();
        cache.remove_saved_view(&scope(), name).unwrap();
    }
    cache
        .put(
            &scope(),
            &Record::new(
                "pull_request",
                "derived",
                "org/app",
                json!({"body":"derived cache"}),
            ),
        )
        .unwrap();
    let other = Scope::github("github.invalid", Some(8)).unwrap();
    cache
        .save_view(
            &other,
            "search:Private",
            &json!({"query":"other account intent"}),
        )
        .unwrap();
    drop(cache);
    // Initialization migrates stored explicit intent before an unopened Corpus UI can clear cache.
    // No manager catalog/recovery reads occur before this clear, and no filesystem adoption.
    let mut cache = Cache::open(&database).unwrap();
    cache.clear_records(&scope()).unwrap();
    assert!(cache.list(&scope(), None).unwrap().is_empty());
    assert_eq!(cache.saved_views(&scope()).unwrap(), intent);
    assert_eq!(cache.saved_views(&other).unwrap().len(), 1);
    drop(cache);
    let cache = Cache::open(&database).unwrap();
    let catalog = manager
        .catalog(&cache, &CancellationToken::default())
        .unwrap();
    assert_eq!(catalog.len(), 1);
    assert!(!catalog[0].active);
    assert_eq!(catalog[0].directory, mapping.directory);
    let recoveries = manager
        .recoveries(&cache, &CancellationToken::default())
        .unwrap();
    assert_eq!(recoveries.len(), 1);
    assert_eq!(recoveries[0].id, recovery.id);
    let other_manager = CorpusManager::new(manager.root.clone(), other).unwrap();
    assert!(other_manager
        .catalog(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert!(other_manager
        .recoveries(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    let restored = apply(
        &manager,
        &cache,
        CorpusOperation::Restore {
            repository: "org/app".into(),
            recovery_id: recovery.id,
        },
    )
    .mapping
    .unwrap();
    assert!(restored.active);
    assert_eq!(
        std::fs::read(restored.path().join("ignored.env")).unwrap(),
        b"retained private ignored bytes\n"
    );
    assert_eq!(
        GitRepository::discover(restored.path())
            .unwrap()
            .unwrap()
            .resolve_original("HEAD")
            .unwrap(),
        fixture.head()
    );
    assert!(manager
        .recoveries(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert_eq!(
        cache.saved_views(&scope()).unwrap().len(),
        1,
        "Restore consumes only its exact durable recovery intent"
    );
}

#[test]
fn selected_corpus_insecure_existing_root_is_refused_without_chmod_or_seed_deletion() {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let manager = manager(&fixture);
    std::fs::create_dir(&manager.root).unwrap();
    std::fs::set_permissions(&manager.root, std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(
        manager.root.join("user.txt"),
        b"existing user directory bytes",
    )
    .unwrap();
    let cache = Cache::memory().unwrap();
    let plan = manager
        .prepare(
            &cache,
            clone_operation(&remote),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(manager
        .execute(&cache, &plan, true, &CancellationToken::default())
        .unwrap_err()
        .0
        .contains("private (0700)"));
    assert_eq!(
        std::fs::metadata(&manager.root)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o755
    );
    assert_eq!(
        std::fs::read(manager.root.join("user.txt")).unwrap(),
        b"existing user directory bytes"
    );
    assert!(!manager.managed_destination("org/app").unwrap().exists());
    assert!(manager
        .catalog(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
}

#[test]
fn selected_corpus_failed_clone_diagnostics_are_bounded_redacted_and_retained_without_mapping() {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let program = fixture.directory.path().join("failing-git");
    std::fs::write(&program,b"#!/bin/sh\nprintf '%s\\n' 'fatal: https://user:password@example.invalid/org/app?token=secret ghp_fake_test_token' >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let manager = manager(&fixture).with_git_program(program);
    let cache = Cache::memory().unwrap();
    let plan = manager
        .prepare(
            &cache,
            clone_operation(&remote),
            &CancellationToken::default(),
        )
        .unwrap();
    let failure = manager
        .execute(&cache, &plan, true, &CancellationToken::default())
        .unwrap_err();
    assert!(failure.0.contains("REMOTE URL REDACTED"));
    assert!(
        !failure.0.contains("password")
            && !failure.0.contains("secret")
            && !failure.0.contains("ghp_fake")
    );
    let recovery = manager
        .recoveries(&cache, &CancellationToken::default())
        .unwrap()
        .pop()
        .unwrap();
    assert!(
        !recovery.reason.contains("password")
            && !recovery.reason.contains("secret")
            && !recovery.reason.contains("ghp_fake")
    );
    assert!(recovery.requires_acknowledgement && !recovery.restorable);
    let progress = manager.progress.snapshot();
    assert_eq!(progress.phase, CorpusPhase::Retained);
    assert_eq!(progress.recovery_directory, Some(recovery.directory));
    assert!(progress.error.unwrap().chars().count() <= 512);
    assert!(manager
        .catalog(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
}

#[test]
fn selected_corpus_local_bare_protocol_guard_blocks_network_rewrite_with_offline_negative_control()
{
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let config = fixture.directory.path().join("rewrite.gitconfig");
    git(
        &fixture.root,
        [
            OsStr::new("config"),
            OsStr::new("--file"),
            config.as_os_str(),
            OsStr::new("url.https://github.invalid/blocked.insteadOf"),
            remote.as_os_str(),
        ],
    );
    let helpers = fixture.directory.path().join("helpers");
    std::fs::create_dir(&helpers).unwrap();
    let sentinel = fixture.directory.path().join("http-helper-ran");
    let helper = helpers.join("git-remote-https");
    std::fs::write(
        &helper,
        b"#!/bin/sh\nprintf intercepted > \"$HTTP_HELPER_SENTINEL\"\nexit 1\n",
    )
    .unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o755)).unwrap();
    let negative = std::process::Command::new("git")
        .args([
            OsStr::new("clone"),
            OsStr::new("--no-local"),
            remote.as_os_str(),
            fixture
                .directory
                .path()
                .join("negative-checkout")
                .as_os_str(),
            OsStr::new("--no-checkout"),
        ])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", &config)
        .env("GIT_EXEC_PATH", &helpers)
        .env("GIT_ALLOW_PROTOCOL", "https")
        .env("HTTP_HELPER_SENTINEL", &sentinel)
        .output()
        .unwrap();
    assert!(!negative.status.success());
    assert_eq!(std::fs::read(&sentinel).unwrap(),b"intercepted","The negative control actually attempts rewritten HTTP, through an offline helper rather than network");
    std::fs::remove_file(&sentinel).unwrap();
    let manager = manager(&fixture)
        .with_env("GIT_CONFIG_NOSYSTEM", "1")
        .with_env("GIT_CONFIG_GLOBAL", config)
        .with_env("GIT_EXEC_PATH", helpers)
        .with_env("HTTP_HELPER_SENTINEL", &sentinel);
    let cache = Cache::memory().unwrap();
    let plan = manager
        .prepare(
            &cache,
            clone_operation(&remote),
            &CancellationToken::default(),
        )
        .unwrap();
    let failure = manager
        .execute(&cache, &plan, true, &CancellationToken::default())
        .unwrap_err();
    assert!(
        failure.0.contains("transport 'https' not allowed"),
        "{failure}"
    );
    assert!(
        !sentinel.exists(),
        "Explicit local-bare clone must not invoke any network helper"
    );
    assert!(manager
        .catalog(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert_eq!(
        manager
            .recoveries(&cache, &CancellationToken::default())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn selected_corpus_remapping_same_owned_directory_keeps_marker_and_recoverable_ownership() {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let manager = manager(&fixture);
    let cache = Cache::memory().unwrap();
    let mapping = apply(&manager, &cache, clone_operation(&remote))
        .mapping
        .unwrap();
    let marker = std::fs::read(mapping.path().join(".git/ghtui-managed-owner.json")).unwrap();
    let updated = apply(
        &manager,
        &cache,
        CorpusOperation::Map {
            repository: "ORG/App".into(),
            directory: mapping.directory.clone(),
            source: LocalSource::Revision("refs/heads/main".into()),
        },
    )
    .mapping
    .unwrap();
    assert_eq!(updated.ownership, mapping.ownership);
    assert_eq!(
        updated.repository, mapping.repository,
        "Case aliases do not rewrite the ownership marker binding"
    );
    assert_eq!(updated.clone_source, mapping.clone_source);
    assert_eq!(
        std::fs::read(updated.path().join(".git/ghtui-managed-owner.json")).unwrap(),
        marker
    );
    let quarantined = apply(
        &manager,
        &cache,
        CorpusOperation::Quarantine {
            repository: "org/app".into(),
        },
    );
    assert!(quarantined.recovery.unwrap().restorable);
    assert!(!mapping.path().exists());
}

#[test]
fn selected_corpus_missing_intent_is_not_adopted_and_owned_publication_recovers_explicitly() {
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let manager = manager(&fixture);
    let cache = Cache::memory().unwrap();
    let mapping = apply(&manager, &cache, clone_operation(&remote))
        .mapping
        .unwrap();
    std::fs::write(mapping.path().join("retained.env"), b"orphan user bytes\n").unwrap();
    assert!(manager
        .orphans(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    let name = cache
        .saved_views(&scope())
        .unwrap()
        .into_iter()
        .find(|(_, value)| value["active"] == true)
        .unwrap()
        .0;
    cache.remove_saved_view(&scope(), &name).unwrap(); // Represents publication before metadata commit.
    let orphans = manager
        .orphans(&cache, &CancellationToken::default())
        .unwrap();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].directory, mapping.directory);
    assert_eq!(orphans[0].saved_commit, mapping.saved_commit);
    assert_eq!(
        orphans[0].kind,
        ghtui::search::managed::OrphanKind::PublishedCheckout
    );
    assert!(
        manager
            .catalog(&cache, &CancellationToken::default())
            .unwrap()
            .is_empty(),
        "Read-only inspection cannot activate/adopt an orphan"
    );
    assert!(manager
        .prepare(
            &cache,
            clone_operation(&remote),
            &CancellationToken::default()
        )
        .unwrap_err()
        .0
        .contains("Owned orphan"));
    let operation = CorpusOperation::RecoverOwnedOrphan {
        repository: "org/app".into(),
        directory: orphans[0].directory.clone(),
    };
    let plan = manager
        .prepare(&cache, operation, &CancellationToken::default())
        .unwrap();
    assert!(manager
        .execute(&cache, &plan, false, &CancellationToken::default())
        .is_err());
    assert!(mapping.path().exists());
    let outcome = manager
        .execute(&cache, &plan, true, &CancellationToken::default())
        .unwrap();
    assert!(!outcome.mapping.as_ref().unwrap().active);
    assert_eq!(
        outcome.mapping.as_ref().unwrap().clone_source,
        None,
        "Lost remote association is not guessed"
    );
    let recovery = outcome.recovery.unwrap();
    assert!(recovery.restorable);
    assert_eq!(
        std::fs::read(Path::new(recovery.directory.as_os_str()).join("retained.env")).unwrap(),
        b"orphan user bytes\n"
    );
    assert!(manager
        .orphans(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    let restored = apply(
        &manager,
        &cache,
        CorpusOperation::Restore {
            repository: "org/app".into(),
            recovery_id: recovery.id,
        },
    )
    .mapping
    .unwrap();
    assert!(restored.active);
    assert_eq!(
        std::fs::read(restored.path().join("retained.env")).unwrap(),
        b"orphan user bytes\n"
    );
}

#[test]
fn selected_corpus_two_numeric_accounts_clone_same_repository_at_one_private_base_without_aliasing()
{
    let fixture = Fixture::linear();
    let remote = bare(&fixture);
    let first = manager(&fixture);
    let second_scope = Scope::github("github.invalid", Some(8)).unwrap();
    let second = CorpusManager::new(first.root.clone(), second_scope.clone()).unwrap();
    let cache = Cache::memory().unwrap();
    let first_mapping = apply(&first, &cache, clone_operation(&remote))
        .mapping
        .unwrap();
    let second_mapping = apply(&second, &cache, clone_operation(&remote))
        .mapping
        .unwrap();
    assert_ne!(first_mapping.directory, second_mapping.directory);
    assert_eq!(
        first_mapping.path().parent().unwrap().parent().unwrap(),
        first.root
    );
    assert_eq!(
        second_mapping.path().parent().unwrap().parent().unwrap(),
        first.root
    );
    assert_eq!(
        first
            .catalog(&cache, &CancellationToken::default())
            .unwrap()
            .as_slice(),
        std::slice::from_ref(&first_mapping)
    );
    assert_eq!(
        second
            .catalog(&cache, &CancellationToken::default())
            .unwrap()
            .as_slice(),
        std::slice::from_ref(&second_mapping)
    );
    assert_eq!(second_mapping.scope, second_scope);
    assert_eq!(
        GitRepository::discover(first_mapping.path())
            .unwrap()
            .unwrap()
            .resolve_original("HEAD")
            .unwrap(),
        fixture.head()
    );
    assert_eq!(
        GitRepository::discover(second_mapping.path())
            .unwrap()
            .unwrap()
            .resolve_original("HEAD")
            .unwrap(),
        fixture.head()
    );
    apply(
        &first,
        &cache,
        CorpusOperation::Quarantine {
            repository: "org/app".into(),
        },
    );
    assert!(!first_mapping.path().exists());
    assert!(second_mapping.path().exists());
    assert!(second
        .recoveries(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert!(second
        .orphans(&cache, &CancellationToken::default())
        .unwrap()
        .is_empty());
}
