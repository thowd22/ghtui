//! Deterministic actual-Git post-completion budget and recovery barriers.
//! Test-only gates control scheduling, never the Git result, usage or resource.
use super::*;
use serde_json::Value;
use std::{
    cell::RefCell,
    process::Command,
    sync::{atomic::AtomicUsize, mpsc},
};

pub(super) struct CloneWitnessV2 {
    pub before_monitor: Box<dyn FnOnce() + Send>,
    pub after_child: Box<dyn FnOnce(bool, bool) + Send>,
    pub monitor_scans: Arc<AtomicUsize>,
}
thread_local! {
    static WITNESS: RefCell<Option<CloneWitnessV2>> = const { RefCell::new(None) };
}
pub(super) fn take_witness() -> Option<CloneWitnessV2> {
    WITNESS.with(|slot| slot.borrow_mut().take())
}

struct InstalledWitness;
impl Drop for InstalledWitness {
    fn drop(&mut self) {
        WITNESS.with(|slot| {
            slot.borrow_mut().take();
        });
    }
}
fn delay_monitor_until_actual_child_finished() -> (InstalledWitness, Arc<AtomicUsize>) {
    delay_monitor_with_after_child(|| {})
}

fn delay_monitor_with_after_child(
    after_child: impl FnOnce() + Send + 'static,
) -> (InstalledWitness, Arc<AtomicUsize>) {
    let (entered, entry) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let scans = Arc::new(AtomicUsize::new(0));
    let witness = CloneWitnessV2 {
        before_monitor: Box::new(move || {
            entered.send(()).unwrap();
            released
                .recv_timeout(Duration::from_secs(8))
                .expect("bounded explicit monitor release after actual child");
        }),
        after_child: Box::new(move |actual_exit_ok, actual_done| {
            entry
                .recv_timeout(Duration::from_secs(8))
                .expect("actual monitor entered its test-only gate");
            assert!(
                actual_exit_ok,
                "only a genuine installed-Git success can establish the post-completion branch"
            );
            assert!(
                actual_done,
                "production done flag must be set before monitor release"
            );
            after_child();
            release.send(()).unwrap();
        }),
        monitor_scans: scans.clone(),
    };
    WITNESS.with(|slot| {
        assert!(slot.borrow().is_none());
        *slot.borrow_mut() = Some(witness);
    });
    (InstalledWitness, scans)
}

fn git(directory: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_ALLOW_PROTOCOL", "file")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "genuine fixture Git failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn clone_children(trace: &Path) -> usize {
    fs::read_to_string(trace)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|row| {
            row["event"] == "start"
                && row["argv"].as_array().is_some_and(|arguments| {
                    arguments
                        .iter()
                        .any(|value| value.as_str() == Some("clone"))
                })
        })
        .count()
}

#[test]
fn selected_corpus_completed_budget_v2_actual_fast_clone_refusal_blocks_fresh_cache_case_alias() {
    let fixture = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let work = fixture.path().join("source working");
    fs::create_dir(&work).unwrap();
    git(
        &work,
        &["init", "--initial-branch=main", "--object-format=sha1"],
    );
    git(&work, &["config", "user.name", "Original corpus fixture"]);
    git(&work, &["config", "user.email", "corpus@example.invalid"]);
    fs::write(work.join("original.txt"), b"original saved corpus bytes\n").unwrap();
    git(&work, &["add", "--", "original.txt"]);
    git(&work, &["commit", "-m", "Original actual source"]);
    let saved = git(&work, &["rev-parse", "HEAD"]).trim().to_owned();
    let bare = fixture.path().join("source bare.git");
    git(
        &work,
        &[
            "clone",
            "--bare",
            work.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );
    let trace = fixture.path().join("original-clone-trace.jsonl");
    let scope = Scope::github("github.invalid", Some(7)).unwrap();
    let cache = Cache::memory().unwrap();
    let mut manager = CorpusManager::new(fixture.path().join("original managed"), scope.clone())
        .unwrap()
        .with_env("GIT_TRACE2_EVENT", trace.as_os_str());
    manager.limits.max_observed_disk_bytes = 1;
    let operation = CorpusOperation::Clone {
        repository: "org/app".into(),
        source: CloneSource::LocalBare(raw(&bare)),
        depth: Some(1),
    };
    let cancel = CancellationToken::default();
    let plan = manager.prepare(&cache, operation, &cancel).unwrap();
    let (_installed, scans) = delay_monitor_until_actual_child_finished();
    let refusal = manager.execute(&cache, &plan, true, &cancel).unwrap_err();
    assert!(
        refusal
            .0
            .contains("Completed clone exceeds observed disk budget"),
        "must hit real final usage refusal, not monitor cancellation: {refusal}"
    );
    assert!(!cancel.is_cancelled());
    assert_eq!(
        scans.load(Ordering::Acquire),
        0,
        "monitor was causally excluded until production done=true"
    );
    assert_eq!(
        clone_children(&trace),
        1,
        "exactly one genuine installed-Git clone completed"
    );
    assert!(manager.catalog(&cache, &cancel).unwrap().is_empty());
    assert!(!manager.managed_destination("org/app").unwrap().exists());
    let recoveries = manager.recoveries(&cache, &cancel).unwrap();
    assert_eq!(recoveries.len(), 1);
    let recovery = &recoveries[0];
    assert!(recovery.requires_acknowledgement);
    let retained = PathBuf::from(recovery.directory.as_os_str()).join("checkout");
    assert_eq!(git(&retained, &["rev-parse", "HEAD"]).trim(), saved);
    assert_eq!(
        git(&retained, &["show", "HEAD:original.txt"]),
        "original saved corpus bytes\n"
    );
    assert_eq!(manager.progress.snapshot().phase, CorpusPhase::Retained);
    let runner = RUNNER.get_or_init(CommandRunner::default);
    let resource = manager.resource("org/app");
    let generation = runner.resource_generation(&resource);
    let fresh_cache = Cache::memory().unwrap();
    let alias = CorpusManager::new(manager.root.clone(), scope)
        .unwrap()
        .with_env("GIT_TRACE2_EVENT", trace.as_os_str());
    assert_eq!(alias.resource("ORG/App"), resource);
    let next = alias
        .prepare(
            &fresh_cache,
            CorpusOperation::Clone {
                repository: "ORG/App".into(),
                source: CloneSource::LocalBare(raw(&bare)),
                depth: Some(1),
            },
            &CancellationToken::default(),
        )
        .unwrap();
    let attempted = alias.execute(&fresh_cache, &next, true, &CancellationToken::default());
    eprintln!(
        "ACTUAL_COMPLETED_CLONE_BUDGET_WITNESS {}",
        serde_json::json!({
            "monitor_scans":scans.load(Ordering::Acquire), "actual_first_child_exit":0,
            "first_refusal":refusal.0, "original_saved_head":saved, "retained":retained,
            "resource":resource, "generation_after_refusal":generation,
            "generation_after_alias":runner.resource_generation(&resource), "actual_clone_children":clone_children(&trace),
            "alias_returned_ready":attempted.is_ok(), "alias_error":attempted.as_ref().err().map(ToString::to_string),
        })
    );
    let blocked = attempted.unwrap_err();
    assert!(
        blocked.0.contains("Previous write outcome is unknown"),
        "same original resource must reject a fresh-cache/case-alias retry: {blocked}"
    );
    assert_eq!(
        clone_children(&trace),
        1,
        "barrier refusal must run zero additional clone children"
    );
    assert_eq!(
        runner.resource_generation(&resource),
        generation,
        "rejected alias does not start a new write generation"
    );
    assert!(!manager.managed_destination("org/app").unwrap().exists());
    assert_eq!(git(&retained, &["rev-parse", "HEAD"]).trim(), saved);
}

struct ActualCorpusV2 {
    fixture: tempfile::TempDir,
    bare: PathBuf,
    saved: String,
    trace: PathBuf,
}

impl ActualCorpusV2 {
    fn new() -> Self {
        let fixture = tempfile::Builder::new()
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir()
            .unwrap();
        let work = fixture.path().join("source working");
        fs::create_dir(&work).unwrap();
        git(
            &work,
            &["init", "--initial-branch=main", "--object-format=sha1"],
        );
        git(&work, &["config", "user.name", "Original corpus fixture"]);
        git(&work, &["config", "user.email", "corpus@example.invalid"]);
        fs::write(work.join("original.txt"), b"original saved corpus bytes\n").unwrap();
        git(&work, &["add", "--", "original.txt"]);
        git(&work, &["commit", "-m", "Original actual source"]);
        let saved = git(&work, &["rev-parse", "HEAD"]).trim().to_owned();
        let bare = fixture.path().join("source bare.git");
        git(
            &work,
            &[
                "clone",
                "--bare",
                work.to_str().unwrap(),
                bare.to_str().unwrap(),
            ],
        );
        let trace = fixture.path().join("original-clone-trace.jsonl");
        Self {
            fixture,
            bare,
            saved,
            trace,
        }
    }

    fn manager(&self, root: &str, account_id: u64, budget: u64) -> CorpusManager {
        let mut manager = CorpusManager::new(
            self.fixture.path().join(root),
            Scope::github("github.invalid", Some(account_id)).unwrap(),
        )
        .unwrap()
        .with_env("GIT_TRACE2_EVENT", self.trace.as_os_str());
        manager.limits.max_observed_disk_bytes = budget;
        manager
    }

    fn clone_operation(&self, repository: &str) -> CorpusOperation {
        CorpusOperation::Clone {
            repository: repository.into(),
            source: CloneSource::LocalBare(raw(&self.bare)),
            depth: Some(1),
        }
    }

    fn refuse_completed_budget(&self, manager: &CorpusManager, cache: &Cache) -> RecoveryEntry {
        let cancel = CancellationToken::default();
        let plan = manager
            .prepare(cache, self.clone_operation("org/app"), &cancel)
            .unwrap();
        let (_installed, scans) = delay_monitor_until_actual_child_finished();
        let refusal = manager.execute(cache, &plan, true, &cancel).unwrap_err();
        assert!(
            refusal
                .0
                .contains("Completed clone exceeds observed disk budget"),
            "{refusal}"
        );
        assert!(!cancel.is_cancelled());
        assert_eq!(scans.load(Ordering::Acquire), 0);
        assert_eq!(clone_children(&self.trace), 1);
        assert!(manager.catalog(cache, &cancel).unwrap().is_empty());
        let entries = manager.recoveries(cache, &cancel).unwrap();
        assert_eq!(entries.len(), 1);
        let recovery = entries.into_iter().next().unwrap();
        assert!(recovery.requires_acknowledgement);
        assert!(!recovery.restorable);
        self.assert_original(&PathBuf::from(recovery.directory.as_os_str()).join("checkout"));
        recovery
    }

    fn assert_original(&self, checkout: &Path) {
        assert_eq!(git(checkout, &["rev-parse", "HEAD"]).trim(), self.saved);
        assert_eq!(
            git(checkout, &["show", "HEAD:original.txt"]),
            "original saved corpus bytes\n"
        );
    }

    fn assert_runner_barrier(&self, resource: &str, generation: u64) {
        let runner = RUNNER.get_or_init(CommandRunner::default);
        let before = fs::read(&self.trace).unwrap();
        let result = runner
            .run(
                CommandSpec::new("git")
                    .args(["--version"])
                    .write(resource)
                    .env("GIT_TRACE2_EVENT", self.trace.as_os_str()),
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert_eq!(
            result.kind,
            crate::providers::runner::FailureKind::ReconciliationRequired
        );
        assert!(result.outcome_unknown);
        assert_eq!(
            fs::read(&self.trace).unwrap(),
            before,
            "barrier refuses before any child starts"
        );
        assert_eq!(runner.resource_generation(resource), generation);
    }
}

#[test]
fn selected_corpus_completed_budget_v2_distinct_physical_root_and_numeric_uid_remain_independent() {
    let fixture = ActualCorpusV2::new();
    let original_cache = Cache::memory().unwrap();
    let original = fixture.manager("original managed", 7, 1);
    let recovery = fixture.refuse_completed_budget(&original, &original_cache);
    let retained = PathBuf::from(recovery.directory.as_os_str()).join("checkout");
    let runner = RUNNER.get_or_init(CommandRunner::default);
    let resource = original.resource("org/app");
    let generation = runner.resource_generation(&resource);
    fixture.assert_runner_barrier(&resource, generation);

    for (root, uid) in [("independent managed", 7), ("original managed", 8)] {
        let independent = fixture.manager(root, uid, 512 * 1024 * 1024);
        assert_ne!(independent.resource("ORG/App"), resource);
        let cache = Cache::memory().unwrap();
        let cancel = CancellationToken::default();
        let plan = independent
            .prepare(&cache, fixture.clone_operation("ORG/App"), &cancel)
            .unwrap();
        let outcome = independent.execute(&cache, &plan, true, &cancel).unwrap();
        let mapping = outcome.mapping.unwrap();
        assert_eq!(mapping.scope, independent.scope);
        assert_eq!(
            mapping.saved_commit.as_deref(),
            Some(fixture.saved.as_str())
        );
        fixture.assert_original(&mapping.path());
        assert_eq!(
            runner.resource_generation(&independent.resource("org/app")),
            1
        );
    }
    assert_eq!(clone_children(&fixture.trace), 3);
    assert_eq!(runner.resource_generation(&resource), generation);
    fixture.assert_original(&retained);
    assert_eq!(
        serde_json::to_value(
            original
                .recoveries(&original_cache, &CancellationToken::default())
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(vec![recovery]).unwrap(),
    );
}

#[test]
fn selected_corpus_completed_budget_v2_clear_restart_requires_exact_fresh_ack_then_separate_confirm(
) {
    use crate::storage::Record;
    let fixture = ActualCorpusV2::new();
    let database = fixture.fixture.path().join("corpus.sqlite3");
    let mut cache = Cache::open(&database).unwrap();
    let manager = fixture.manager("original managed", 7, 1);
    let scope = manager.scope.clone();
    let foreign = Scope::github("github.invalid", Some(8)).unwrap();
    let record = Record::new(
        "derived",
        "original",
        "org/app",
        serde_json::json!({"private":"original"}),
    );
    cache.put(&scope, &record).unwrap();
    cache.put(&foreign, &record).unwrap();
    cache
        .put_draft(&scope, "composer", "original unsent draft")
        .unwrap();
    cache
        .put_draft(&foreign, "composer", "foreign unsent draft")
        .unwrap();
    let saved_view = serde_json::json!({"query":"original scoped saved search"});
    cache
        .save_view(&scope, "original-search", &saved_view)
        .unwrap();
    let recovery = fixture.refuse_completed_budget(&manager, &cache);
    let retained = PathBuf::from(recovery.directory.as_os_str()).join("checkout");
    let runner = RUNNER.get_or_init(CommandRunner::default);
    let resource = manager.resource("org/app");
    let generation = runner.resource_generation(&resource);
    cache.clear_records(&scope).unwrap();
    drop(cache);
    drop(manager);

    let cache = Cache::open(&database).unwrap();
    let manager = fixture.manager("original managed", 7, 512 * 1024 * 1024);
    let cancel = CancellationToken::default();
    assert!(cache.get(&scope, "derived", "original").unwrap().is_none());
    assert_eq!(
        cache.get(&foreign, "derived", "original").unwrap(),
        Some(record)
    );
    assert_eq!(
        cache.draft(&scope, "composer").unwrap().as_deref(),
        Some("original unsent draft")
    );
    assert_eq!(
        cache.draft(&foreign, "composer").unwrap().as_deref(),
        Some("foreign unsent draft")
    );
    assert_eq!(
        cache.saved_view(&scope, "original-search").unwrap(),
        Some(saved_view)
    );
    assert_eq!(
        serde_json::to_value(manager.recoveries(&cache, &cancel).unwrap()).unwrap(),
        serde_json::to_value(vec![recovery.clone()]).unwrap(),
    );
    let refusal = manager
        .prepare(&cache, fixture.clone_operation("ORG/App"), &cancel)
        .unwrap_err();
    assert!(refusal.0.contains("explicitly acknowledge"), "{refusal}");
    let wrong = manager
        .prepare(
            &cache,
            CorpusOperation::AcknowledgeRecovery {
                repository: "org/other".into(),
                recovery_id: recovery.id.clone(),
            },
            &cancel,
        )
        .unwrap_err();
    assert!(wrong.0.contains("another repository"), "{wrong}");
    let operation = CorpusOperation::AcknowledgeRecovery {
        repository: "org/app".into(),
        recovery_id: recovery.id.clone(),
    };
    let old_ack = manager.prepare(&cache, operation.clone(), &cancel).unwrap();
    let unconfirmed = manager
        .execute(&cache, &old_ack, false, &cancel)
        .unwrap_err();
    assert!(unconfirmed
        .0
        .contains("explicit target/effect confirmation"));
    let mut changed = recovery.clone();
    changed
        .reason
        .push_str("; separately observed current retained state");
    manager.put_recovery(&cache, &changed).unwrap();
    let stale = manager
        .execute(&cache, &old_ack, true, &cancel)
        .unwrap_err();
    assert!(stale.0.contains("preview is stale"), "{stale}");
    fixture.assert_runner_barrier(&resource, generation);
    assert_eq!(clone_children(&fixture.trace), 1);
    assert!(
        manager
            .recovery(&cache, &recovery.id)
            .unwrap()
            .requires_acknowledgement
    );

    let fresh_ack = manager.prepare(&cache, operation, &cancel).unwrap();
    let acknowledged = manager.execute(&cache, &fresh_ack, true, &cancel).unwrap();
    assert!(!acknowledged.recovery.unwrap().requires_acknowledgement);
    assert!(acknowledged.mapping.is_none());
    assert_eq!(
        clone_children(&fixture.trace),
        1,
        "acknowledgement never retries a clone"
    );
    assert_eq!(runner.resource_generation(&resource), generation);
    let replay = manager
        .execute(&cache, &fresh_ack, true, &cancel)
        .unwrap_err();
    assert!(replay.0.contains("preview is stale"), "{replay}");
    let fresh_clone = manager
        .prepare(&cache, fixture.clone_operation("org/app"), &cancel)
        .unwrap();
    assert!(manager
        .execute(&cache, &fresh_clone, false, &cancel)
        .is_err());
    assert_eq!(clone_children(&fixture.trace), 1);
    let published = manager
        .execute(&cache, &fresh_clone, true, &cancel)
        .unwrap()
        .mapping
        .unwrap();
    assert_eq!(clone_children(&fixture.trace), 2);
    assert_eq!(runner.resource_generation(&resource), generation + 1);
    fixture.assert_original(&published.path());
    fixture.assert_original(&retained);
    assert_eq!(
        cache.draft(&scope, "composer").unwrap().as_deref(),
        Some("original unsent draft")
    );
}

#[test]
fn selected_corpus_completed_budget_v2_recovery_directory_failure_still_blocks_same_resource() {
    let fixture = ActualCorpusV2::new();
    let cache = Cache::memory().unwrap();
    let manager = fixture.manager("original managed", 7, 1);
    let cancel = CancellationToken::default();
    let plan = manager
        .prepare(&cache, fixture.clone_operation("org/app"), &cancel)
        .unwrap();
    let obstruction = manager.account_root().join("recovery");
    let actual_obstruction = obstruction.clone();
    let (_installed, scans) = delay_monitor_with_after_child(move || {
        fs::write(
            &actual_obstruction,
            b"owned recovery obstacle; preserve these bytes",
        )
        .unwrap();
    });
    let error = manager.execute(&cache, &plan, true, &cancel).unwrap_err();
    assert!(
        error.0.contains("Managed storage path is not a directory"),
        "{error}"
    );
    assert_eq!(scans.load(Ordering::Acquire), 0);
    assert!(!cancel.is_cancelled());
    assert_eq!(clone_children(&fixture.trace), 1);
    assert_eq!(
        fs::read(&obstruction).unwrap(),
        b"owned recovery obstacle; preserve these bytes"
    );
    assert_eq!(manager.progress.snapshot().phase, CorpusPhase::Retained);
    assert!(manager
        .progress
        .snapshot()
        .error
        .unwrap()
        .contains("Completed clone exceeds observed disk budget"));
    assert!(manager.recoveries(&cache, &cancel).unwrap().is_empty());
    let orphans = manager.orphans(&cache, &cancel).unwrap();
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].kind, OrphanKind::StagedTransfer);
    let retained = PathBuf::from(orphans[0].directory.as_os_str()).join("checkout");
    fixture.assert_original(&retained);
    let fresh_cache = Cache::memory().unwrap();
    let alias = fixture.manager("original managed", 7, 512 * 1024 * 1024);
    let refusal = alias
        .prepare(&fresh_cache, fixture.clone_operation("ORG/App"), &cancel)
        .unwrap_err();
    assert!(refusal.0.contains("Owned orphan bytes"), "{refusal}");
    let resource = manager.resource("org/app");
    assert_eq!(alias.resource("ORG/App"), resource);
    let runner = RUNNER.get_or_init(CommandRunner::default);
    let generation = runner.resource_generation(&resource);
    assert_eq!(
        generation, 2,
        "one actual write and one completed-result retirement"
    );
    fixture.assert_runner_barrier(&resource, generation);
    assert!(
        !runner.mark_reconciled_generation(&resource, generation - 1),
        "old observations cannot reconcile current recovery failure"
    );
    fixture.assert_runner_barrier(&resource, generation);
    assert!(!manager.managed_destination("org/app").unwrap().exists());
    fixture.assert_original(&retained);
}

#[test]
fn selected_corpus_completed_budget_v2_recovery_sql_failure_preserves_bytes_and_early_barrier() {
    let fixture = ActualCorpusV2::new();
    let database = fixture.fixture.path().join("corpus.sqlite3");
    let cache = Cache::open(&database).unwrap();
    let connection = rusqlite::Connection::open(&database).unwrap();
    connection.execute_batch("CREATE TRIGGER refuse_original_recovery BEFORE INSERT ON saved_views WHEN NEW.name LIKE 'corpus-recovery-v1:%' BEGIN SELECT RAISE(ABORT, 'actual original recovery SQL refusal'); END;").unwrap();
    drop(connection);
    let manager = fixture.manager("original managed", 7, 1);
    let cancel = CancellationToken::default();
    let plan = manager
        .prepare(&cache, fixture.clone_operation("org/app"), &cancel)
        .unwrap();
    let (_installed, scans) = delay_monitor_until_actual_child_finished();
    let failure = manager.execute(&cache, &plan, true, &cancel).unwrap_err();
    assert!(failure.0.contains("Directory retained at"), "{failure}");
    assert!(
        failure.0.contains("actual original recovery SQL refusal"),
        "{failure}"
    );
    assert_eq!(scans.load(Ordering::Acquire), 0);
    assert_eq!(clone_children(&fixture.trace), 1);
    assert!(manager.recoveries(&cache, &cancel).unwrap().is_empty());
    assert!(manager.catalog(&cache, &cancel).unwrap().is_empty());
    assert!(!manager.managed_destination("org/app").unwrap().exists());
    let retained_entries = fs::read_dir(manager.account_root().join("recovery"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(retained_entries.len(), 1);
    let retained = retained_entries[0].join("checkout");
    fixture.assert_original(&retained);
    let resource = manager.resource("org/app");
    let runner = RUNNER.get_or_init(CommandRunner::default);
    let generation = runner.resource_generation(&resource);
    assert_eq!(generation, 2);
    fixture.assert_runner_barrier(&resource, generation);
    drop(cache);
    let reopened = Cache::open(&database).unwrap();
    assert!(manager.recoveries(&reopened, &cancel).unwrap().is_empty());
    let fresh = fixture.manager("original managed", 7, 512 * 1024 * 1024);
    let fresh_cache = Cache::memory().unwrap();
    let new_plan = fresh
        .prepare(&fresh_cache, fixture.clone_operation("ORG/App"), &cancel)
        .unwrap();
    let blocked = fresh
        .execute(&fresh_cache, &new_plan, true, &cancel)
        .unwrap_err();
    assert!(
        blocked.0.contains("Previous write outcome is unknown"),
        "{blocked}"
    );
    assert_eq!(clone_children(&fixture.trace), 1);
    assert_eq!(runner.resource_generation(&resource), generation);
    fixture.assert_original(&retained);
}
