//! SOURCE ONLY / UNRUN. Genuine catalogue+current Inbox factories; actual
//! existing Background channel and SQLite. No invented source/owner/receipt.
use super::*;
use crate::scoped_inbox_route_v1::tests::actual_clear_fixture_v5 as fixture;
use crate::{
    app::{Action, Overlay},
    pr_write_journal::{PrWriteIntent, PrWriteJournal, PrWriteTarget},
    storage::Record,
};
use rusqlite::{types::Value as SqlValue, Connection, OpenFlags};
use std::{collections::BTreeMap, fs, sync::mpsc};

fn sql(path: &Path) -> Connection {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}
fn write(path: &Path) -> Connection {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap()
}
type SqlWitnessV5 = Vec<(String, Vec<Vec<SqlValue>>)>;
type PresentationWitnessV5 = (
    String,
    String,
    [bool; 5],
    View,
    Option<Overlay>,
    usize,
    usize,
    usize,
);
fn dump(path: &Path) -> SqlWitnessV5 {
    let db = sql(path);
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().collect::<rusqlite::Result<Vec<_>>>().unwrap();
    names
        .into_iter()
        .map(|name| {
            let mut statement = db.prepare(&format!("SELECT * FROM \"{name}\"")).unwrap();
            let n = statement.column_count();
            let mut rows = statement
                .query_map([], |r| {
                    (0..n)
                        .map(|i| r.get(i))
                        .collect::<rusqlite::Result<Vec<SqlValue>>>()
                })
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            rows.sort_by_cached_key(|r| format!("{r:?}"));
            (name, rows)
        })
        .collect()
}
fn gh(f: &crate::test_support::Fixture) -> Vec<u8> {
    fs::read(f.root.join("gh-fixture/commands.jsonl")).unwrap()
}
fn own_count(path: &Path, table: &str) -> usize {
    sql(path)
        .query_row(
            &format!(
                "SELECT count(*) FROM {table} WHERE host='github.invalid' AND login='user-id:42'"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap()
}
fn intent(app: &mut App) {
    app.dispatch(Action::Github(GithubAction::ClearCache));
    assert_eq!(app.github_action, Some(GithubAction::ClearCache));
}
fn completion(bg: &mut Background) -> Completion {
    bg.completions
        .recv_timeout(Duration::from_secs(5))
        .expect("Actual local Clear worker finishes")
}
fn put(bg: &Background, c: Completion) {
    bg.inline_completion_sender_v1.send(c).unwrap();
}
fn observe(app: &App) -> PresentationWitnessV5 {
    (
        app.context.clone(),
        app.status.clone(),
        app.loading,
        app.view,
        app.overlay,
        app.inbox_list.selected,
        app.inbox_list.top,
        app.focus,
    )
}
fn actual_error_v5() -> Result<(), String> {
    Err("PRIVATE_ACTUAL_ERROR_AFTER_CLEAR".into())
}
fn actual_panic_v5() -> Result<(), String> {
    panic!("PRIVATE_ACTUAL_WORKER_PANIC_AFTER_CLEAR")
}

#[test]
fn cache_storage_catalogue_clear_v5_actual_two_rows_deleted_other_uid_drafts_saved_journal_preserved_and_replay_refuses(
) {
    let (f, mut app, path, _) = fixture();
    let before_gh = gh(&f);
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    let active = app.remote_scope.as_ref().unwrap().clone();
    assert_eq!(own_count(&path, "records"), 2);
    let mut cache = Cache::open(&path).unwrap();
    let foreign = Scope::github("github.invalid", Some(43)).unwrap();
    cache
        .commit_sync(
            &foreign,
            "foreign-source",
            7,
            &[Record::new(
                "pr",
                "foreign",
                "example/other",
                serde_json::json!({"body":"FOREIGN_PRIVATE"}),
            )],
            true,
        )
        .unwrap();
    cache
        .save_view(
            source.scope(),
            "other-preference",
            &serde_json::json!({"kept":true}),
        )
        .unwrap();
    cache
        .put_draft(&foreign, "foreign-draft", "FOREIGN_DRAFT")
        .unwrap();
    let actual = &app.github.as_ref().unwrap().inbox.pull_requests[0];
    let target = PrWriteTarget::for_source(
        source.scope(),
        &actual.repository,
        &actual.id,
        actual.number,
    )
    .unwrap();
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let _permit = journal
        .begin(
            &target,
            &PrWriteIntent {
                command: "new-comment".into(),
                selected_resource: String::new(),
                observed_head: "a".repeat(40),
                values: BTreeMap::from([("body".into(), "DURABLE_PENDING".into())]),
            },
        )
        .unwrap();
    let before = dump(&path);
    intent(&mut app);
    let owner = CatalogueClearOwnerV5::capture(&app, &path).unwrap();
    let mut bg = Background::new();
    assert!(bg.submit_catalogue_clear_v5(&mut app, &path));
    let c = completion(&mut bg);
    assert!(c.result.is_ok());
    put(&bg, c);
    assert!(bg.poll(&mut app));
    assert_eq!(own_count(&path, "records"), 0);
    assert_eq!(own_count(&path, "checkpoints"), 0);
    let after = dump(&path);
    for table in [
        "drafts",
        "saved_views",
        "pr_write_attempts",
        "pr_write_scopes",
    ] {
        assert_eq!(
            before
                .iter()
                .find(|r| r.0 == table)
                .expect("actual protected table"),
            after
                .iter()
                .find(|r| r.0 == table)
                .expect("same actual protected table"),
            "Exact protected {table}"
        );
    }
    assert_eq!(sql(&path).query_row("SELECT payload FROM records WHERE host='github.invalid' AND login='user-id:43' AND id='foreign'",[],|r|r.get::<_,String>(0)).unwrap(),"{\"body\":\"FOREIGN_PRIVATE\"}");
    assert!(journal.blocking(&target).unwrap().is_some());
    assert!(app.github.as_ref().unwrap().inbox.pull_requests.is_empty());
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &active));
    assert!(Cache::open(&path)
        .unwrap()
        .validate_repository_catalogue_intent_source(&source)
        .is_err());
    assert!(!app.loading[View::Inbox.index()]);
    assert!(!bg.current.contains_key(&View::Inbox.index()));
    let after = dump(&path);
    assert!(owner.execute().is_err());
    assert_eq!(dump(&path), after);
    assert_eq!(gh(&f), before_gh);
}

#[test]
fn cache_storage_catalogue_clear_v5_cancel_expiry_wrong_file_and_sql_clear_payload_drift_refuse_without_rebase(
) {
    for mode in 0..5 {
        let (f, mut app, path, _) = fixture();
        intent(&mut app);
        let mut owner = CatalogueClearOwnerV5::capture(&app, &path).unwrap();
        match mode {
            0 => owner.cancel.cancel(),
            1 => Arc::get_mut(&mut owner).unwrap().deadline = Instant::now(),
            2 => {
                let bad = f.root.join("missing/not-created.sqlite3");
                Arc::get_mut(&mut owner).unwrap().database = bad.clone();
                assert!(!bad.parent().unwrap().exists());
            }
            3 => Cache::open(&path)
                .unwrap()
                .clear_records(owner.scope())
                .unwrap(),
            _ => Cache::open(&path)
                .unwrap()
                .save_view(
                    owner.scope(),
                    owner.source.saved_view_key(),
                    &serde_json::json!({"changed":true}),
                )
                .unwrap(),
        }
        let before = dump(&path);
        let before_gh = gh(&f);
        assert!(owner.execute().is_err());
        assert_eq!(dump(&path), before);
        assert_eq!(gh(&f), before_gh);
        if mode == 2 {
            assert!(!f.root.join("missing").exists());
        }
    }
}

#[test]
fn cache_storage_catalogue_clear_v5_actual_lock_wait_consumes_original_deadline_and_trigger_failure_rolls_back(
) {
    for mode in 0..2 {
        let (f, mut app, path, _) = fixture();
        intent(&mut app);
        let mut owner = CatalogueClearOwnerV5::capture(&app, &path).unwrap();
        let before = dump(&path);
        let before_gh = gh(&f);
        if mode == 0 {
            Arc::get_mut(&mut owner).unwrap().deadline = Instant::now() + Duration::from_millis(30);
            let db = write(&path);
            db.execute_batch("BEGIN IMMEDIATE").unwrap();
            let (entered, ready) = mpsc::sync_channel(1);
            let (done, result) = mpsc::sync_channel(1);
            let task = thread::spawn(move || {
                entered.send(()).unwrap();
                done.send(owner.execute()).unwrap();
            });
            ready.recv_timeout(Duration::from_secs(1)).unwrap();
            thread::sleep(Duration::from_millis(70));
            db.execute_batch("ROLLBACK").unwrap();
            assert!(result
                .recv_timeout(Duration::from_secs(5))
                .unwrap()
                .is_err());
            task.join().unwrap();
        } else {
            write(&path).execute_batch("CREATE TRIGGER local_clear_failure BEFORE DELETE ON records BEGIN SELECT RAISE(ABORT,'PRIVATE_CLEAR_FAILURE'); END").unwrap();
            assert!(owner.execute().is_err());
        }
        assert_eq!(dump(&path), before);
        assert_eq!(gh(&f), before_gh);
        assert_eq!(own_count(&path, "records"), 2);
    }
}

#[test]
fn cache_storage_catalogue_clear_v5_full_disconnected_and_old_physical_slot_refusal_preserve_every_existing_owner(
) {
    let (f, mut app, path, _) = fixture();
    let before_gh = gh(&f);
    let before = dump(&path);
    let mut bg = Background::new();
    let mut release = Vec::new();
    for view in [View::Repository, View::Search] {
        let (entered, ready) = mpsc::sync_channel(1);
        let (send, receive) = mpsc::sync_channel(1);
        assert!(bg.submit(&mut app, view, move |_| {
            entered.send(()).unwrap();
            receive.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(PreparedView::default())
        }));
        ready.recv_timeout(Duration::from_secs(1)).unwrap();
        release.push(send);
    }
    for _ in 0..4 {
        assert!(bg.submit(&mut app, View::Actions, |_| Ok(PreparedView::default())));
    }
    intent(&mut app);
    let state = observe(&app);
    let generation = bg.generation;
    let slots = bg
        .current
        .iter()
        .map(|(s, (g, t))| (*s, *g, t.clone()))
        .collect::<Vec<_>>();
    assert!(!bg.submit_catalogue_clear_v5(&mut app, &path));
    assert_eq!(observe(&app), state);
    assert_eq!(bg.generation, generation);
    for (s, g, t) in slots {
        let now = &bg.current[&s];
        assert_eq!(now.0, g);
        assert!(now.1.same_instance(&t));
    }
    assert_eq!(dump(&path), before);
    assert_eq!(gh(&f), before_gh);
    for sender in release {
        sender.send(()).unwrap();
    }
    drop(bg);
    let mut bg = Background::new();
    let (tx, rx) = mpsc::sync_channel(1);
    drop(rx);
    bg.requests = Some(tx);
    let state = observe(&app);
    assert!(!bg.submit_catalogue_clear_v5(&mut app, &path));
    assert_eq!(observe(&app), state);
    assert!(bg.current.is_empty());
    bg.requests.take();
    let state = observe(&app);
    let before_generation = bg.generation;
    assert!(!bg.submit_catalogue_clear_v5(&mut app, &path));
    assert_eq!(observe(&app), state);
    assert_eq!(bg.generation, before_generation);
    assert!(bg.current.is_empty());
    assert_eq!(dump(&path), before);
    assert_eq!(gh(&f), before_gh);
    let mut bg = Background::new();
    let (send, receive) = mpsc::sync_channel(1);
    assert!(bg.submit(&mut app, View::Inbox, move |_| {
        receive.recv_timeout(Duration::from_secs(5)).unwrap();
        Ok(PreparedView::default())
    }));
    let old = bg.current[&View::Inbox.index()].clone();
    let state = observe(&app);
    assert!(!bg.submit_catalogue_clear_v5(&mut app, &path));
    assert_eq!(observe(&app), state);
    assert_eq!(bg.current[&View::Inbox.index()].0, old.0);
    assert!(!old.1.is_cancelled());
    send.send(()).unwrap();
}

#[test]
fn cache_storage_catalogue_clear_v5_actual_late_success_error_panic_wrong_facet_and_newer_navigation_never_publish(
) {
    for mode in 0..9 {
        let (f, mut app, path, _) = fixture();
        let before_gh = gh(&f);
        let mut bg = Background::new();
        intent(&mut app);
        let mut actual = CatalogueClearOwnerV5::capture(&app, &path).unwrap();
        if mode == 3 {
            Arc::get_mut(&mut actual).unwrap().after_sql_v5 = Some(actual_error_v5);
        }
        if mode == 6 {
            Arc::get_mut(&mut actual).unwrap().after_sql_v5 = Some(actual_panic_v5);
        }
        assert!(bg.submit_captured_catalogue_clear_v5(&mut app, actual));
        let mut c = completion(&mut bg);
        assert_eq!(c.result.is_err(), matches!(mode, 3 | 6));
        // Actual worker has already COMMITTED Clear. Rejected late paint is
        // not a claim that SQL was rolled back; every original row is zero.
        assert_eq!(own_count(&path, "records"), 0);
        let mut replacement = None;
        match mode {
            0 => app.overlay = Some(Overlay::Notice),
            1 => app.inbox_list.top += 1,
            2 => {
                let (next, new, _, _) = fixture();
                replacement = Some(next);
                app = new;
            }
            3 => {
                app.overlay = Some(Overlay::Notice);
            }
            4 => {
                c.result.as_mut().unwrap().clear_private = true;
                app.github.as_mut().unwrap().identity.account_id = Some(43);
            }
            5 => {
                c.result.as_mut().unwrap().remote_scope = app.remote_scope.clone();
            }
            6 => {
                app.view = View::Repository;
            }
            7 => {
                // Provider Fixture creates only a directory. Build an actual
                // repository before asking the real Workspace/Form factories
                // for the independently newer local CreateBranch presentation.
                for args in [
                    vec!["init", "--initial-branch=main"],
                    vec![
                        "-c",
                        "user.name=Local Clear fixture",
                        "-c",
                        "user.email=clear@example.invalid",
                        "-c",
                        "commit.gpgsign=false",
                        "-c",
                        "core.hooksPath=/dev/null",
                        "commit",
                        "--allow-empty",
                        "-m",
                        "Actual newer local Form source",
                    ],
                ] {
                    let output = std::process::Command::new("git")
                        .args(["-c", "maintenance.auto=false", "-c", "gc.auto=0"])
                        .args(args)
                        .current_dir(f.repo())
                        .env("GIT_CONFIG_NOSYSTEM", "1")
                        .env("GIT_CONFIG_GLOBAL", "/dev/null")
                        .output()
                        .unwrap();
                    assert!(output.status.success(), "{:?}", output);
                }
                let workspace = crate::workspace::Workspace::load(
                    &f.repo(),
                    crate::workspace::RepositoryMode::Working,
                    "HEAD",
                    None,
                )
                .unwrap()
                .unwrap();
                app.form = Some(
                    crate::forms::Form::local(
                        crate::local_actions::LocalCommand::CreateBranch,
                        crate::local_actions::LocalContext::from_workspace(&workspace, None, None),
                    )
                    .unwrap(),
                );
                app.overlay = Some(Overlay::Form);
            }
            _ => {
                c.view = View::Actions;
                app.github.as_mut().unwrap().identity.account_id = Some(43);
            }
        }
        app.status = "NEWER_STATUS_PRESERVED".into();
        app.notice = "NEWER_NOTICE_PRESERVED".into();
        let state = observe(&app);
        let form = app.form.as_ref().map(|form| {
            let (ticket, epoch) = form.presentation_v4();
            (Arc::clone(ticket), epoch, form.title.clone(), form.values())
        });
        if mode == 7 {
            assert!(matches!(
                &app.form.as_ref().unwrap().purpose,
                crate::forms::FormPurpose::Local(intent)
                    if intent.command == crate::local_actions::LocalCommand::CreateBranch
                        && intent.context.directory == f.repo()
                        && intent.context.status.head.is_some()
            ));
            assert_eq!(app.overlay, Some(Overlay::Form));
        }
        let committed_sql = dump(&path);
        let slot = bg.current[&View::Inbox.index()].clone();
        let source = app.remote_scope_source.clone();
        assert_eq!(bg.poll_catalogue_clear_v5(&mut app, &mut c), Some(false));
        assert_eq!(observe(&app), state);
        match (form, app.form.as_ref()) {
            (Some((ticket, epoch, title, values)), Some(actual)) => {
                assert!(Arc::ptr_eq(actual.presentation_v4().0, &ticket));
                assert_eq!(actual.presentation_v4().1, epoch);
                assert_eq!(actual.title, title);
                assert_eq!(actual.values(), values);
            }
            (None, None) => {}
            _ => panic!("Rejected old Clear completion must preserve the exact newer Form"),
        }
        assert_eq!(dump(&path), committed_sql);
        assert_eq!(app.notice, "NEWER_NOTICE_PRESERVED");
        assert!(app
            .remote_scope_source
            .as_ref()
            .zip(source.as_ref())
            .is_some_and(|(a, b)| Arc::ptr_eq(a, b)));
        assert_eq!(bg.current[&View::Inbox.index()].0, slot.0);
        assert!(bg.current[&View::Inbox.index()].1.same_instance(&slot.1));
        assert_eq!(gh(&f), before_gh);
        drop(replacement);
    }
}

#[test]
fn cache_storage_catalogue_clear_v5_actual_replacement_slot_and_account_loss_preserve_successor_token_loading_status(
) {
    for lose_account in [false, true] {
        let (f, mut app, path, _) = fixture();
        let before_gh = gh(&f);
        let mut bg = Background::new();
        intent(&mut app);
        assert!(bg.submit_catalogue_clear_v5(&mut app, &path));
        let mut c = completion(&mut bg);
        assert!(c.result.is_ok());
        let (entered, ready) = mpsc::sync_channel(1);
        let (release, held) = mpsc::sync_channel(1);
        assert!(bg.submit(&mut app, View::Inbox, move |_| {
            entered.send(()).unwrap();
            held.recv_timeout(Duration::from_secs(5)).unwrap();
            Ok(PreparedView::default())
        }));
        ready.recv_timeout(Duration::from_secs(1)).unwrap();
        let successor = bg.current[&View::Inbox.index()].clone();
        app.status = "NEWER_LOADING_STATUS".into();
        if lose_account {
            app.github.as_mut().unwrap().identity.account_id = Some(43);
        }
        let observed = observe(&app);
        let changed = bg.poll_catalogue_clear_v5(&mut app, &mut c);
        assert_eq!(changed, Some(lose_account));
        assert_eq!(app.status, "NEWER_LOADING_STATUS");
        assert_eq!(app.loading, observed.2);
        assert_eq!(bg.current[&View::Inbox.index()].0, successor.0);
        assert!(bg.current[&View::Inbox.index()]
            .1
            .same_instance(&successor.1));
        assert!(!successor.1.is_cancelled());
        if lose_account {
            assert!(app.github.is_none());
            assert!(!app.context.contains("example/one"));
        } else {
            assert_eq!(observe(&app), observed);
            assert!(app.github.is_some());
        }
        assert_eq!(own_count(&path, "records"), 0);
        assert_eq!(gh(&f), before_gh);
        release.send(()).unwrap();
    }
}
