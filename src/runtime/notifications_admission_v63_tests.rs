//! Source-only regression declarations. Actual original child/channel/Git/SQL;
//! no fabricated Completion or source/owner constructors.
use super::*;
use crate::{
    notifications_v63::tests as fixture,
    owned_pty_processes_test_support as process,
    repository_catalogue_read_limits_test_support_v1 as support,
    storage::{Cache, Scope},
    test_support::Fixture,
    workspace::{RepositoryMode, Workspace},
};
use std::{
    fs,
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

fn sql(path: &Path) -> Vec<(String, Vec<Vec<String>>)> {
    let connection =
        rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    let names = connection.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    ).unwrap().query_map([], |row| row.get::<_, String>(0)).unwrap()
        .map(Result::unwrap).collect::<Vec<_>>();
    names
        .into_iter()
        .map(|name| {
            let mut statement = connection
                .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
                .unwrap();
            let columns = statement.column_count();
            let mut result = Vec::new();
            let mut rows = statement.query([]).unwrap();
            while let Some(row) = rows.next().unwrap() {
                result.push(
                    (0..columns)
                        .map(|column| format!("{:?}", row.get_ref(column).unwrap()))
                        .collect::<Vec<_>>(),
                );
            }
            result.sort();
            (name, result)
        })
        .collect()
}
fn git(f: &Fixture, args: &[&str]) -> Vec<u8> {
    let output = Command::new("git")
        .args(["-c", "diff.autoRefreshIndex=false"])
        .args(args)
        .current_dir(f.repo())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn git_state(f: &Fixture) -> Vec<Vec<u8>> {
    vec![
        git(f, &["rev-parse", "HEAD"]),
        git(f, &["status", "--porcelain=v1", "-z"]),
        git(f, &["diff", "--binary"]),
        fs::read(f.repo().join(".git/index")).unwrap(),
        fs::read(f.repo().join(".git/config")).unwrap(),
        fs::read(f.repo().join("tracked")).unwrap(),
    ]
}
fn initialize(f: &Fixture) {
    git(f, &["init", "-q"]);
    fs::write(f.repo().join("tracked"), b"ORIGINAL_TRACKED\n").unwrap();
    git(f, &["add", "tracked"]);
    git(
        f,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "original",
        ],
    );
    fs::write(f.repo().join("tracked"), b"PRIVATE_WORKING_EDIT\0\xff\n").unwrap();
}
fn actual_child(f: &Fixture, index: usize) -> process::ProcessIdentity {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let log = fs::read_to_string(f.root.join("gh-fixture/commands.jsonl")).unwrap_or_default();
        if let Some(line) = log.lines().nth(index) {
            let row: serde_json::Value = serde_json::from_str(line).unwrap();
            assert_eq!(row["matched"], true);
            let pid = i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
            let actual = process::process_identity(pid)
                .unwrap()
                .expect("actual held original child");
            assert!(!actual.zombie);
            assert_eq!(actual.uid, unsafe { libc::geteuid() });
            return actual;
        }
        assert!(
            Instant::now() < until,
            "Actual notification child did not start: {log}"
        );
        std::thread::yield_now();
    }
}
fn gone(original: &process::ProcessIdentity) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match process::process_identity(original.pid) {
            Ok(None) => return,
            Ok(Some(now)) => {
                assert_eq!(now.uid, original.uid, "Foreign PID is not an absence proof");
                assert_eq!(now.birth, original.birth, "Reused PID is not a reap proof");
                assert_eq!(
                    now.session, original.session,
                    "Foreign session is not a reap proof"
                );
            }
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {}
            Err(error) => panic!("Original identity observation failed: {error}"),
        }
        assert!(
            Instant::now() < until,
            "Exact original notification child not reaped"
        );
        std::thread::yield_now();
    }
}

#[test]
fn notifications_v63_admission_held_actual_inbox_does_not_suppress_independent_real_repository_completion(
) {
    let f = Fixture::new();
    initialize(&f);
    let database = support::database(&f);
    Cache::open(&database)
        .unwrap()
        .put_draft(
            &Scope::github(support::HOST, Some(77)).unwrap(),
            "foreign-draft",
            "FOREIGN_PRIVATE_BODY",
        )
        .unwrap();
    let mut expected = fixture::steps(42);
    expected[2].delay_ms = 6_000;
    f.scripted_gh(&expected);
    let mut app = fixture::app();
    let work = fixture::captured(&mut app);
    let token = work.cancellation().clone();
    let mut background = Background::new();
    assert!(background.submit_notifications_v63(
        &mut app,
        work.clone(),
        support::provider(&f),
        database.clone()
    ));
    let held = actual_child(&f, 2);
    let (inbox_generation, inbox_token) = background.current[&View::Inbox.index()].clone();
    let before_git = git_state(&f);
    let before_sql = sql(&database);
    let path = f.repo();
    assert!(
        background.submit(&mut app, View::Repository, move |cancel| {
            let workspace = Workspace::load_with_cancellation(
                &path,
                RepositoryMode::Overview,
                "HEAD",
                None,
                cancel,
            )?
            .ok_or("Actual installed-Git workspace missing")?;
            Ok(PreparedView {
                workspace: Some(workspace),
                lines: vec!["ACTUAL_INSTALLED_GIT_REPOSITORY_COMPLETION".into()],
                context: Some("Repository-only current context".into()),
                status: "Repository-only current status".into(),
                ..Default::default()
            })
        })
    );
    let repository_generation = background.current[&View::Repository.index()].0;
    assert_ne!(repository_generation, inbox_generation);
    let status = app.status.clone();
    let context = app.context.clone();
    let original = work.original().clone();
    let until = Instant::now() + Duration::from_secs(5);
    while app.loading[View::Repository.index()] {
        background.poll(&mut app);
        assert!(
            Instant::now() < until,
            "Independent real Repository completion was suppressed"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        app.content[View::Repository.index()],
        ["ACTUAL_INSTALLED_GIT_REPOSITORY_COMPLETION"]
    );
    assert!(app.workspace.is_some());
    assert!(!background.current.contains_key(&View::Repository.index()));
    assert_eq!(background.current[&View::Inbox.index()].0, inbox_generation);
    assert!(background.current[&View::Inbox.index()]
        .1
        .same_instance(&inbox_token));
    assert!(app
        .notifications_owner_v63
        .as_ref()
        .is_some_and(|owner| Arc::ptr_eq(owner, &work)));
    assert!(app
        .notifications_v63
        .as_ref()
        .is_some_and(|view| Arc::ptr_eq(&view.original, &original)));
    assert!(app.notifications_v63.as_ref().unwrap().prepared.is_none());
    assert!(!token.is_cancelled());
    assert!(app.loading[View::Inbox.index()]);
    assert_eq!(app.status, status);
    assert_eq!(app.context, context);
    assert_eq!(git_state(&f), before_git);
    assert_eq!(sql(&database), before_sql);
    let still_live = process::process_identity(held.pid).unwrap().unwrap();
    assert_eq!(still_live.birth, held.birth);
    assert_eq!(still_live.uid, held.uid);
    assert_eq!(still_live.session, held.session);
    assert!(!still_live.zombie);
    assert!(background.cancel_notifications_v63(&mut app));
    gone(&held);
    assert!(token.is_cancelled());
    assert!(!app.loading[View::Inbox.index()]);
    assert_eq!(sql(&database), before_sql);
    assert_eq!(git_state(&f), before_git);
    support::exact(&f, &expected[..3]);
}

#[test]
fn notifications_v63_admission_full_and_disconnected_queue_preserve_all_presentation_sql_and_generation(
) {
    for disconnected in [false, true] {
        let f = Fixture::new();
        f.scripted_gh(&[]);
        let database = support::database(&f);
        Cache::open(&database)
            .unwrap()
            .put_draft(
                &Scope::github(support::HOST, Some(42)).unwrap(),
                "kept",
                "EXACT_ORIGINAL_DRAFT",
            )
            .unwrap();
        let mut app = fixture::app();
        let work = fixture::captured(&mut app);
        let original = work.original().clone();
        let before_sql = sql(&database);
        let status = app.status.clone();
        let context = app.context.clone();
        let mut background = Background::new();
        let generation = background.generation;
        let (sender, receiver) = mpsc::sync_channel(0);
        background.requests = Some(sender);
        let held = if disconnected {
            drop(receiver);
            None
        } else {
            Some(receiver)
        };
        assert!(!background.submit_notifications_v63(
            &mut app,
            work.clone(),
            support::provider(&f),
            database.clone()
        ));
        assert_eq!(background.generation, generation);
        assert!(background.current.is_empty());
        assert!(background.current_notifications_v63.is_none());
        assert!(app.notifications_owner_v63.is_none());
        assert!(!work.cancellation().is_cancelled());
        assert!(!app.loading[View::Inbox.index()]);
        assert_eq!(app.status, status);
        assert_eq!(app.context, context);
        assert!(app
            .notifications_v63
            .as_ref()
            .is_some_and(|view| Arc::ptr_eq(&view.original, &original)));
        assert_eq!(sql(&database), before_sql);
        support::exact(&f, &[]);
        drop(held);
    }
}

#[test]
fn notifications_v63_admission_occupied_actual_inbox_refuses_without_canceling_its_token_or_generation(
) {
    let f = Fixture::new();
    f.scripted_gh(&[]);
    let mut app = fixture::app();
    let work = fixture::captured(&mut app);
    let mut background = Background::new();
    let (started, start) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    assert!(background.submit(&mut app, View::Inbox, move |token| {
        started.send(token.clone()).unwrap();
        released.recv_timeout(Duration::from_secs(8)).unwrap();
        Ok(PreparedView::default())
    }));
    let token = start.recv_timeout(Duration::from_secs(5)).unwrap();
    let generation = background.generation;
    let status = app.status.clone();
    assert!(!background.submit_notifications_v63(
        &mut app,
        work,
        support::provider(&f),
        support::database(&f)
    ));
    assert_eq!(background.generation, generation);
    assert!(!token.is_cancelled());
    assert_eq!(background.current[&View::Inbox.index()].0, generation);
    assert!(background.current[&View::Inbox.index()]
        .1
        .same_instance(&token));
    assert!(app.loading[View::Inbox.index()]);
    assert_eq!(app.status, status);
    support::exact(&f, &[]);
    release.send(()).unwrap();
}

#[test]
fn notifications_v63_admission_generation_overflow_refuses_zero_children_and_zero_effects() {
    let f = Fixture::new();
    f.scripted_gh(&[]);
    let mut app = fixture::app();
    let work = fixture::captured(&mut app);
    let status = app.status.clone();
    let mut background = Background::new();
    background.generation = u64::MAX; // bounded scalar contract, never an owner constructor
    assert!(!background.submit_notifications_v63(
        &mut app,
        work.clone(),
        support::provider(&f),
        support::database(&f)
    ));
    assert_eq!(background.generation, u64::MAX);
    assert!(background.current.is_empty());
    assert!(!work.cancellation().is_cancelled());
    assert_eq!(app.status, status);
    assert!(!support::database(&f).exists());
    support::exact(&f, &[]);
}

#[test]
fn notifications_v63_integration_actual_completion_missing_foreign_original_and_mixed_facets_preserve_slot_and_private_presentation(
) {
    for mode in 0..4 {
        let f = Fixture::new();
        initialize(&f);
        let expected = fixture::steps(42);
        f.scripted_gh(&expected);
        let database = support::database(&f);
        Cache::open(&database)
            .unwrap()
            .put_draft(
                &Scope::github(support::HOST, Some(77)).unwrap(),
                "integration-foreign-draft",
                "FOREIGN_PRIVATE_BODY",
            )
            .unwrap();
        let mut app = fixture::app();
        let work = fixture::captured(&mut app);
        // The foreign descriptor is captured through the SAME real input
        // factory on an independently allocated actual cached Inbox. It is
        // neither deserialized evidence nor a fabricated source/owner.
        let mut other_app = fixture::app();
        let foreign = fixture::captured(&mut other_app);
        assert!(!Arc::ptr_eq(work.original(), foreign.original()));
        let mut background = Background::new();
        assert!(background.submit_notifications_v63(
            &mut app,
            work.clone(),
            support::provider(&f),
            database.clone(),
        ));
        let mut completion = background
            .completions
            .recv_timeout(Duration::from_secs(5))
            .expect("actual original provider worker must finish before faulting");
        assert!(completion
            .result
            .as_ref()
            .unwrap()
            .notifications_v63
            .is_some());
        let remote = completion.remote_scope_owner.as_mut().unwrap();
        assert!(remote.owns(&app));
        match mode {
            0 => {
                // A lost nested owner plus non-typed generic payload MUST NOT
                // escape through generic runtime admission after logical loss.
                remote.notifications_v63 = None;
                app.notifications_owner_v63 = None;
                completion.result = Ok(PreparedView {
                    status: "MISSING_OWNER_PRIVATE_POISON".into(),
                    clear_private: true,
                    ..Default::default()
                });
            }
            1 => {
                remote.original =
                    ScopedRemoteOriginalV13::Notifications(foreign.original().clone());
            }
            2 => remote.notifications_v63 = None,
            _ => {
                let prepared = completion.result.as_mut().unwrap();
                prepared.status = "MIXED_RESULT_PRIVATE_POISON".into();
                prepared.clear_private = true;
            }
        }
        if mode <= 2 {
            assert!(!completion.remote_scope_owner.as_ref().unwrap().owns(&app));
        }
        let (generation, token) = background.current[&View::Inbox.index()].clone();
        let status = app.status.clone();
        let context = app.context.clone();
        let loading = app.loading;
        let overlay = app.overlay;
        let hits = app
            .hits
            .iter()
            .map(|h| (h.rect, h.action.clone()))
            .collect::<Vec<_>>();
        let before_sql = sql(&database);
        let before_git = git_state(&f);
        // Reinsert only this ACTUAL completed message into its existing
        // receiver. No replacement Completion, queue or handler substitutes.
        background
            .inline_completion_sender_v1
            .send(completion)
            .unwrap();
        assert!(!background.poll(&mut app));
        assert_eq!(background.current[&View::Inbox.index()].0, generation);
        assert!(background.current[&View::Inbox.index()]
            .1
            .same_instance(&token));
        assert!(!token.is_cancelled());
        assert_eq!(app.loading, loading);
        assert_eq!(app.status, status);
        assert_eq!(app.context, context);
        assert_eq!(app.overlay, overlay);
        assert_eq!(
            app.hits
                .iter()
                .map(|h| (h.rect, h.action.clone()))
                .collect::<Vec<_>>(),
            hits
        );
        assert!(app.notifications_v63.as_ref().unwrap().prepared.is_none());
        assert!(Arc::ptr_eq(
            &app.notifications_v63.as_ref().unwrap().original,
            work.original()
        ));
        assert_eq!(sql(&database), before_sql);
        assert_eq!(git_state(&f), before_git);
        support::exact(&f, &expected);
        // Restore ONLY the exact genuine App marker for owned cancellation;
        // no result/private body/source is restored or published by cleanup.
        app.notifications_owner_v63 = Some(work.clone());
        assert!(background.cancel_notifications_v63(&mut app));
        assert!(token.is_cancelled());
        assert_eq!(sql(&database), before_sql);
        assert_eq!(git_state(&f), before_git);
    }
}

#[test]
fn notifications_v63_integration_missing_entire_remote_preserves_exact_logical_and_physical_owner()
{
    let f = Fixture::new();
    initialize(&f);
    let expected = fixture::steps(42);
    f.scripted_gh(&expected);
    let database = support::database(&f);
    let mut app = fixture::app();
    let work = fixture::captured(&mut app);
    let mut background = Background::new();
    assert!(background.submit_notifications_v63(
        &mut app,
        work.clone(),
        support::provider(&f),
        database.clone()
    ));
    let mut completion = background
        .completions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(completion
        .result
        .as_ref()
        .unwrap()
        .notifications_v63
        .is_some());
    let (generation, token) = background.current[&View::Inbox.index()].clone();
    // Lose BOTH actual transport descriptors and the App marker. Only the
    // genuine retained logical generation may reject this missing-owner case.
    completion.remote_scope_owner = None;
    completion.result = Ok(PreparedView {
        status: "REMOTE_NONE_PRIVATE_POISON".into(),
        clear_private: true,
        ..Default::default()
    });
    app.notifications_owner_v63 = None;
    let status = app.status.clone();
    let context = app.context.clone();
    let loading = app.loading;
    let overlay = app.overlay;
    let hits = app
        .hits
        .iter()
        .map(|h| (h.rect, h.action.clone()))
        .collect::<Vec<_>>();
    let before_sql = sql(&database);
    let before_git = git_state(&f);
    background
        .inline_completion_sender_v1
        .send(completion)
        .unwrap();
    assert!(!background.poll(&mut app));
    assert_eq!(background.current[&View::Inbox.index()].0, generation);
    assert!(background.current[&View::Inbox.index()]
        .1
        .same_instance(&token));
    assert!(background
        .current_notifications_v63
        .as_ref()
        .is_some_and(|(g, owner)| *g == generation && Arc::ptr_eq(owner, &work)));
    assert!(!token.is_cancelled());
    assert_eq!(app.status, status);
    assert_eq!(app.context, context);
    assert_eq!(app.loading, loading);
    assert_eq!(app.overlay, overlay);
    assert_eq!(
        app.hits
            .iter()
            .map(|h| (h.rect, h.action.clone()))
            .collect::<Vec<_>>(),
        hits
    );
    assert!(app.notifications_v63.as_ref().unwrap().prepared.is_none());
    assert!(Arc::ptr_eq(
        &app.notifications_v63.as_ref().unwrap().original,
        work.original()
    ));
    assert_eq!(sql(&database), before_sql);
    assert_eq!(git_state(&f), before_git);
    support::exact(&f, &expected);
    app.notifications_owner_v63 = Some(work);
    assert!(background.cancel_notifications_v63(&mut app));
    assert!(token.is_cancelled());
    assert_eq!(sql(&database), before_sql);
    assert_eq!(git_state(&f), before_git);
}

#[test]
fn notifications_v63_integration_declared_original_without_nested_owner_cannot_retire_actual_repository_slot(
) {
    let f = Fixture::new();
    initialize(&f);
    let expected = fixture::steps(42);
    f.scripted_gh(&expected);
    let database = support::database(&f);
    let mut app = fixture::app();
    let work = fixture::captured(&mut app);
    let mut background = Background::new();
    assert!(background.submit_notifications_v63(
        &mut app,
        work.clone(),
        support::provider(&f),
        database.clone()
    ));
    let mut notification = background
        .completions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(notification
        .result
        .as_ref()
        .unwrap()
        .notifications_v63
        .is_some());
    let mut declared = notification.remote_scope_owner.take().unwrap();
    assert!(declared.owns(&app));
    let path = f.repo();
    assert!(
        background.submit(&mut app, View::Repository, move |cancel| {
            let workspace = Workspace::load_with_cancellation(
                &path,
                RepositoryMode::Overview,
                "HEAD",
                None,
                cancel,
            )?
            .ok_or("Actual repository unavailable")?;
            Ok(PreparedView {
                workspace: Some(workspace),
                lines: vec!["ACTUAL_REPOSITORY_NOT_NOTIFICATION".into()],
                ..Default::default()
            })
        })
    );
    let mut repository = background
        .completions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(repository.view, View::Repository);
    assert!(repository.result.as_ref().unwrap().workspace.is_some());
    assert!(repository.remote_scope_owner.is_none());
    let (generation, token) = background.current[&View::Repository.index()].clone();
    let (inbox_generation, inbox_token) = background.current[&View::Inbox.index()].clone();
    // Fault ONLY the transport metadata of this actual installed-Git
    // completion with the genuine declared original. No generation, token,
    // source, Workspace or Completion is fabricated or rebound to authorize it.
    declared.notifications_v63 = None;
    repository.remote_scope_owner = Some(declared);
    assert!(!repository.remote_scope_owner.as_ref().unwrap().owns(&app));
    let before_sql = sql(&database);
    let before_git = git_state(&f);
    let status = app.status.clone();
    let context = app.context.clone();
    let loading = app.loading;
    let overlay = app.overlay;
    let hits = app
        .hits
        .iter()
        .map(|h| (h.rect, h.action.clone()))
        .collect::<Vec<_>>();
    background
        .inline_completion_sender_v1
        .send(repository)
        .unwrap();
    assert!(!background.poll(&mut app));
    assert_eq!(background.current[&View::Repository.index()].0, generation);
    assert!(background.current[&View::Repository.index()]
        .1
        .same_instance(&token));
    assert_eq!(background.current[&View::Inbox.index()].0, inbox_generation);
    assert!(background.current[&View::Inbox.index()]
        .1
        .same_instance(&inbox_token));
    assert!(!token.is_cancelled() && !inbox_token.is_cancelled());
    assert!(app.workspace.is_none());
    assert_eq!(app.status, status);
    assert_eq!(app.context, context);
    assert_eq!(app.loading, loading);
    assert_eq!(app.overlay, overlay);
    assert_eq!(
        app.hits
            .iter()
            .map(|h| (h.rect, h.action.clone()))
            .collect::<Vec<_>>(),
        hits
    );
    assert!(app.notifications_v63.as_ref().unwrap().prepared.is_none());
    assert_eq!(sql(&database), before_sql);
    assert_eq!(git_state(&f), before_git);
    support::exact(&f, &expected);
    assert!(background.cancel_notifications_v63(&mut app));
    background.cancel(View::Repository);
    assert!(token.is_cancelled() && inbox_token.is_cancelled());
}

#[test]
fn notifications_v63_integration_older_missing_owner_never_blocks_genuine_newer_generic_inbox_completion(
) {
    let f = Fixture::new();
    initialize(&f);
    let expected = fixture::steps(42);
    f.scripted_gh(&expected);
    let database = support::database(&f);
    let mut app = fixture::app();
    let work = fixture::captured(&mut app);
    let mut background = Background::new();
    assert!(background.submit_notifications_v63(
        &mut app,
        work.clone(),
        support::provider(&f),
        database.clone()
    ));
    let mut old = background
        .completions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(old.result.as_ref().unwrap().notifications_v63.is_some());
    let old_token = work.cancellation().clone();
    let old_generation = old.generation;
    old.remote_scope_owner = None;
    old.result = Ok(PreparedView {
        status: "OLD_MISSING_OWNER_POISON".into(),
        clear_private: true,
        ..Default::default()
    });
    let (started, start) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let path = f.repo();
    assert!(background.submit(&mut app, View::Inbox, move |cancel| {
        started.send(cancel.clone()).unwrap();
        released.recv_timeout(Duration::from_secs(8)).unwrap();
        let actual = Workspace::load_with_cancellation(
            &path,
            RepositoryMode::Overview,
            "HEAD",
            None,
            cancel,
        )?
        .ok_or("Actual installed-Git source unavailable")?;
        Ok(PreparedView {
            lines: vec![format!(
                "Actual newer generic Inbox from {}",
                actual.directory.display()
            )],
            status: "ACTUAL_NEWER_GENERIC_INBOX_FINISHED".into(),
            ..Default::default()
        })
    }));
    let token = start.recv_timeout(Duration::from_secs(5)).unwrap();
    let generation = background.current[&View::Inbox.index()].0;
    assert_ne!(generation, old_generation);
    assert!(old_token.is_cancelled());
    // SAME normal main-loop orphan retirement; never manually clear a current
    // owner to grant admission to the newer generic result.
    background.retire_stale_notifications_v63(&mut app);
    assert!(app.notifications_owner_v63.is_none());
    assert!(background.current_notifications_v63.is_none());
    assert_eq!(background.current[&View::Inbox.index()].0, generation);
    let before_sql = sql(&database);
    let before_git = git_state(&f);
    let status = app.status.clone();
    let context = app.context.clone();
    let overlay = app.overlay;
    background.inline_completion_sender_v1.send(old).unwrap();
    assert!(!background.poll(&mut app));
    assert_eq!(background.current[&View::Inbox.index()].0, generation);
    assert!(background.current[&View::Inbox.index()]
        .1
        .same_instance(&token));
    assert!(!token.is_cancelled());
    assert!(app.loading[View::Inbox.index()]);
    assert_eq!(app.status, status);
    assert_eq!(app.context, context);
    assert_eq!(app.overlay, overlay);
    assert_eq!(sql(&database), before_sql);
    assert_eq!(git_state(&f), before_git);
    release.send(()).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while app.loading[View::Inbox.index()] {
        background.poll(&mut app);
        assert!(
            Instant::now() < until,
            "Genuine newer generic Inbox result was suppressed"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        app.content[View::Inbox.index()],
        [format!(
            "Actual newer generic Inbox from {}",
            f.repo().display()
        )]
    );
    assert_eq!(app.status, "ACTUAL_NEWER_GENERIC_INBOX_FINISHED");
    assert!(!background.current.contains_key(&View::Inbox.index()));
    assert_eq!(sql(&database), before_sql);
    assert_eq!(git_state(&f), before_git);
    support::exact(&f, &expected);
}

#[test]
fn notifications_v63_integration_same_generation_actual_close_cancel_still_preserves_slot_before_drain(
) {
    let f = Fixture::new();
    initialize(&f);
    let expected = fixture::steps(42);
    f.scripted_gh(&expected);
    let database = support::database(&f);
    let mut app = fixture::app();
    let work = fixture::captured(&mut app);
    let mut background = Background::new();
    assert!(background.submit_notifications_v63(
        &mut app,
        work.clone(),
        support::provider(&f),
        database.clone(),
    ));
    let completion = background
        .completions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(completion
        .result
        .as_ref()
        .unwrap()
        .notifications_v63
        .is_some());
    let (generation, token) = background.current[&View::Inbox.index()].clone();
    assert!(!app.notifications_cancel_v63);
    // Actual visible Close queues cancellation; no manual owner/source/ticket
    // construction and no generic flag reset grants publication admission.
    app.dispatch(crate::app::Action::Close);
    assert!(app.notifications_cancel_v63);
    assert!(app.notifications_v63.is_none());
    assert!(app
        .notifications_owner_v63
        .as_ref()
        .is_some_and(|owner| Arc::ptr_eq(owner, &work)));
    assert!(background
        .current_notifications_v63
        .as_ref()
        .is_some_and(|(current, owner)| *current == generation && Arc::ptr_eq(owner, &work)));
    let status = app.status.clone();
    let context = app.context.clone();
    let overlay = app.overlay;
    let hits = app
        .hits
        .iter()
        .map(|hit| (hit.rect, hit.action.clone()))
        .collect::<Vec<_>>();
    let before_sql = sql(&database);
    let before_git = git_state(&f);
    background
        .inline_completion_sender_v1
        .send(completion)
        .unwrap();
    assert!(!background.poll(&mut app));
    assert_eq!(background.current[&View::Inbox.index()].0, generation);
    assert!(background.current[&View::Inbox.index()]
        .1
        .same_instance(&token));
    assert!(!token.is_cancelled());
    assert!(app.loading[View::Inbox.index()] && app.notifications_cancel_v63);
    assert_eq!(app.status, status);
    assert_eq!(app.context, context);
    assert_eq!(app.overlay, overlay);
    assert_eq!(
        app.hits
            .iter()
            .map(|hit| (hit.rect, hit.action.clone()))
            .collect::<Vec<_>>(),
        hits
    );
    assert_eq!(sql(&database), before_sql);
    assert_eq!(git_state(&f), before_git);
    assert!(background.cancel_notifications_v63(&mut app));
    assert!(token.is_cancelled());
    assert!(!background.current.contains_key(&View::Inbox.index()));
    assert!(!app.loading[View::Inbox.index()]);
    assert_eq!(sql(&database), before_sql);
    assert_eq!(git_state(&f), before_git);
    support::exact(&f, &expected);
}

#[test]
fn notifications_v63_integration_actual_retirement_orphan_cancel_keeps_newer_generic_inbox_progress(
) {
    let f = Fixture::new();
    initialize(&f);
    let expected = fixture::steps(42);
    f.scripted_gh(&expected);
    let database = support::database(&f);
    let mut app = fixture::app();
    let work = fixture::captured(&mut app);
    let mut background = Background::new();
    assert!(background.submit_notifications_v63(
        &mut app,
        work.clone(),
        support::provider(&f),
        database.clone(),
    ));
    let old = background
        .completions
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert!(old.result.as_ref().unwrap().notifications_v63.is_some());
    let old_epoch = app.command_source_epoch;
    let old_generation = old.generation;
    let (started, start) = mpsc::sync_channel(1);
    let (release, released) = mpsc::sync_channel(1);
    let path = f.repo();
    assert!(background.submit(&mut app, View::Inbox, move |cancel| {
        started.send(cancel.clone()).unwrap();
        released.recv_timeout(Duration::from_secs(8)).unwrap();
        let workspace = Workspace::load_with_cancellation(
            &path,
            RepositoryMode::Overview,
            "HEAD",
            None,
            cancel,
        )?
        .ok_or("Actual newer installed-Git source unavailable")?;
        Ok(PreparedView {
            lines: vec![format!(
                "Actual current generic Inbox {}",
                workspace.directory.display()
            )],
            status: "ACTUAL_ORPHAN_CANCEL_GENERIC_FINISHED".into(),
            ..Default::default()
        })
    }));
    let token = start.recv_timeout(Duration::from_secs(5)).unwrap();
    let generation = background.current[&View::Inbox.index()].0;
    assert_ne!(generation, old_generation);
    assert_ne!(app.command_source_epoch, old_epoch);
    assert!(work.cancellation().is_cancelled());
    assert!(
        !app.notifications_cancel_v63,
        "Main's first cancellation-drain sees no flag yet"
    );
    assert!(background.current_notifications_v63.is_none());
    assert!(app
        .notifications_owner_v63
        .as_ref()
        .is_some_and(|owner| Arc::ptr_eq(owner, &work)));
    background.retire_stale_notifications_v63(&mut app);
    assert!(
        app.notifications_cancel_v63,
        "Actual old presentation retirement creates the later cancellation signal"
    );
    assert!(app.notifications_owner_v63.is_none() && app.notifications_v63.is_none());
    assert!(background.current_notifications_v63.is_none());
    assert_eq!(background.current[&View::Inbox.index()].0, generation);
    assert!(background.current[&View::Inbox.index()]
        .1
        .same_instance(&token));
    assert!(!token.is_cancelled());
    let before_sql = sql(&database);
    let before_git = git_state(&f);
    release.send(()).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while app.loading[View::Inbox.index()] {
        background.poll(&mut app);
        assert!(
            Instant::now() < until,
            "An orphan cancellation cannot discard this genuine newer completion"
        );
        std::thread::yield_now();
    }
    assert!(
        app.notifications_cancel_v63,
        "No blanket cancellation reset or fixture drain enables progress"
    );
    assert_eq!(
        app.content[View::Inbox.index()],
        [format!(
            "Actual current generic Inbox {}",
            f.repo().display()
        )]
    );
    assert_eq!(app.status, "ACTUAL_ORPHAN_CANCEL_GENERIC_FINISHED");
    assert!(!background.current.contains_key(&View::Inbox.index()));
    assert!(!token.is_cancelled());
    assert_eq!(sql(&database), before_sql);
    assert_eq!(git_state(&f), before_git);
    support::exact(&f, &expected);
    drop(old);
}
