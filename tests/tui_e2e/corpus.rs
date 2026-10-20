//! Native selected-corpus journeys: exact paths, durable intent and actual local Git effects.
#[path = "corpus_completed_budget_v2.rs"]
mod corpus_completed_budget_v2;
#[path = "corpus_preview_v4.rs"]
mod corpus_preview_v4;
use crate::support::{Fixture, GhStep, Session};
use ghtui::{
    config::Config,
    providers::runner::CancellationToken,
    search::managed::CorpusManager,
    storage::{Cache, Scope},
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

fn identity() -> Vec<GhStep> {
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
            &json!({"login":"alice","id":42}).to_string(),
        ),
    ]
}
fn identities(count: usize) -> Vec<GhStep> {
    (0..count).flat_map(|_| identity()).collect()
}
fn fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "nested/日本語.txt",
        b"saved corpus evidence\n",
        "Saved corpus fixture",
    );
    Config {
        host: "github.invalid".into(),
        avatars: false,
        active_set: Some("explicit".into()),
        repository_sets: BTreeMap::from([("explicit".into(), vec!["org/app".into()])]),
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    fixture
}
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label)
    } else {
        session.send_key(key)
    }
}
fn search(session: &mut Session, mouse: bool) {
    choose(session, mouse, "3 Search", b"3");
    session.wait_for_control("[M Repositories]");
    session.wait_for_settled_footer();
}
fn manage(session: &mut Session, mouse: bool) {
    // Tokens move once a corpus read finishes; click M only in the settled bar.
    session.wait_for_settled_footer();
    choose(session, mouse, "[M Repositories]", b"M");
    session.wait_for_screen("Manage selected repositories");
    session.wait_for_screen("REPOSITORY INTENT");
}
fn field(session: &mut Session, mouse: bool, index: usize, label: &str) {
    // Every field remains accessible when the terminal is too short to show all 11.
    for _ in 0..11 {
        choose(session, mouse, "[F5 Fields↑]", b"\x1b[15~");
    }
    for _ in 0..index {
        choose(session, mouse, "[F6 Fields↓]", b"\x1b[17~");
    }
    // A later field can still be visible in an OLD viewport. Observe it as the
    // first field of the current intent before using its new mouse coordinates.
    // Fixed delays cannot establish that all queued scrolls have been drawn.
    session
        .try_wait_for_condition(
            |screen| {
                crate::support::first_form_field_value(screen, "REPOSITORY INTENT", label).is_some()
            },
            &format!("Current first repository field: {label}"),
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| {
            session.save_artifacts();
            panic!("{error}");
        });
}
fn set_field(session: &mut Session, mouse: bool, index: usize, label: &str, value: &str) {
    field(session, mouse, index, label);
    if mouse {
        let (x, y) = session.locate(label).unwrap();
        session.click(x, y + 1);
    }
    session.send_key(b"\x01");
    session.paste(value);
    session.wait_for_screen(value);
}
fn operation(session: &mut Session, mouse: bool, index: usize, label: &str) {
    field(session, mouse, 0, "Operation (explicit preview");
    let token = [
        "map",
        "clone",
        "fetch",
        "unmap",
        "quarantine",
        "restore",
        "acknowledge",
        "recover-orphan",
    ][index];
    for _ in 0..8 {
        let screen = session.screen();
        let current = crate::support::first_form_field_value(
            &screen,
            "REPOSITORY INTENT",
            "Operation (explicit preview",
        )
        .expect("Observed operation must be the current first field");
        if current == token {
            return;
        }
        let operations = [
            "map",
            "clone",
            "fetch",
            "unmap",
            "quarantine",
            "restore",
            "acknowledge",
            "recover-orphan",
        ];
        let next = operations[(operations
            .iter()
            .position(|value| *value == current)
            .expect("Actual operation token")
            + 1)
            % operations.len()];
        if mouse {
            // This indexed mouse action has the same semantics as Space.
            let (x, y) = session.locate("Operation (explicit preview").unwrap();
            session.click(x + 2, y + 2);
        } else {
            session.send_key(b" ");
        }
        // Do not send another cycle until THIS exact next value has appeared.
        // Several clicks against one old frame can otherwise wrap back to Map.
        session
            .try_wait_for_condition(
                |screen| {
                    crate::support::first_form_field_value(
                        screen,
                        "REPOSITORY INTENT",
                        "Operation (explicit preview",
                    ) == Some(next)
                },
                &format!("One operation cycle acknowledged as {next}"),
                Duration::from_secs(8),
            )
            .unwrap_or_else(|error| {
                session.save_artifacts();
                panic!("{error}");
            });
    }
    panic!(
        "Operation {label:?} must be selectable through the actual control\n{}",
        session.screen()
    );
}
fn preview(session: &mut Session, mouse: bool, effect: &str) {
    choose(session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("REPOSITORY PREVIEW");
    // The durable draft writer can replace the transient status footer immediately
    // after preparation. The actual prepared header and exact effect are the
    // readiness contract; never race a successful draft acknowledgement.
    session.wait_for_screen("[Confirm action]");
    let required = effect
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    for _ in 0..20 {
        if session
            .try_wait_for_condition(
                |screen| {
                    screen
                        .lines()
                        .filter_map(|line| line.split('│').max_by_key(|part| part.len()))
                        .collect::<Vec<_>>()
                        .join(" ")
                        .chars()
                        .filter(|character| !character.is_whitespace())
                        .collect::<String>()
                        .contains(&required)
                },
                effect,
                Duration::from_millis(150),
            )
            .is_ok()
        {
            return;
        }
        if mouse {
            let (x, y) = session.locate("REPOSITORY PREVIEW").unwrap();
            session.wheel(x + 10, y + 2, true);
        } else {
            session.send_key(b"\x1b[B");
        }
    }
    session.save_artifacts();
    panic!(
        "Exact preview effect must be visible, including after scrolling: {effect}\n{}",
        session.screen()
    );
}
fn confirm(session: &mut Session, mouse: bool, outcome: &str) {
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.focus_visible_form_action("[Confirm action]");
        session.send_key(b"\r");
    }
    session.wait_for_screen(outcome);
    session.wait_for_control("[M Repositories]");
    assert!(!session.screen().contains("Manage selected repositories"));
}
fn query(session: &mut Session, mouse: bool, input: &str) {
    choose(session, mouse, "[/ Query]", b"/");
    session.send_key(b"\x01");
    session.paste(input);
    choose(session, mouse, "[Enter Search]", b"\r");
}
fn manager(fixture: &Fixture) -> CorpusManager {
    CorpusManager::new(
        fixture.root.join("config/ghtui/corpus"),
        Scope::github("github.invalid", Some(42)).unwrap(),
    )
    .unwrap()
}
fn cache(fixture: &Fixture) -> Cache {
    Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap()
}
fn journal(session: &mut Session, fixture: &Fixture, expected: usize) {
    session.wait_for_command_count(fixture, expected);
    let rows = session
        .inspect_command_journal(fixture)
        .lines()
        .map(|row| serde_json::from_str::<Value>(row).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), expected);
    assert!(rows.iter().all(|row| row["matched"] == true));
    assert!(rows
        .iter()
        .all(|row| row["args"][0] == "--version" || row["args"][5] == "GET"));
}
fn git_at(path: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
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
    String::from_utf8(output.stdout).unwrap()
}

/// Release only this fixture's transfer on panic; never leave a sleeping child behind.
struct SlowTransfer {
    release: PathBuf,
    ready: PathBuf,
    pid: PathBuf,
    finished: PathBuf,
}
impl SlowTransfer {
    fn install(fixture: &Fixture) -> Self {
        let actual_git = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|directory| directory.join("git"))
            .find(|path| path.is_file())
            .unwrap()
            .canonicalize()
            .unwrap();
        let transfer = Self {
            release: fixture.root.join("transfer-release"),
            ready: fixture.root.join("transfer-ready"),
            pid: fixture.root.join("transfer-pid"),
            finished: fixture.root.join("transfer-finished"),
        };
        let quote = |path: &Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
        let script = format!(
            "#!/bin/sh\nif [ \"$3\" != clone ]; then exec {} \"$@\"; fi\n{} \"$@\" || exit $?\nprintf '%s' \"$$\" > {}\nprintf 'retained generated partial bytes\\n' > \"$6/retained.env\"\nprintf '%s' \"$6\" > {}\nwhile [ ! -f {} ]; do sleep 0.05; done\nprintf done > {}\n",
            quote(&actual_git),quote(&actual_git),quote(&transfer.pid),quote(&transfer.ready),quote(&transfer.release),quote(&transfer.finished),
        );
        let program = fixture.root.join("bin/git");
        fs::write(&program, script).unwrap();
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
        transfer
    }
    fn wait_ready(&self, session: &mut Session) -> (i32, PathBuf) {
        session
            .try_wait_for_condition(
                |_| self.ready.exists(),
                "actual installed Git clone completed inside transfer fixture",
                Duration::from_secs(8),
            )
            .unwrap();
        let pid = fs::read_to_string(&self.pid)
            .unwrap()
            .parse::<i32>()
            .unwrap();
        assert!(pid > 0);
        let path = PathBuf::from(fs::read_to_string(&self.ready).unwrap());
        assert!(path.join(".git").is_dir());
        assert_eq!(
            fs::read(path.join("retained.env")).unwrap(),
            b"retained generated partial bytes\n"
        );
        (pid, path)
    }
    fn release(&self) {
        fs::write(&self.release, b"explicit test release").unwrap();
    }
}
impl Drop for SlowTransfer {
    fn drop(&mut self) {
        let _ = fs::write(&self.release, b"fixture cleanup release");
        if self.finished.exists() {
            return;
        }
        if let Ok(pid) = fs::read_to_string(&self.pid) {
            if let Ok(pid) = pid.parse::<i32>() {
                // The PID was generated by this owned helper; its runner isolates its process group.
                if pid > 0 && unsafe { libc::kill(pid, 0) } == 0 {
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                }
            }
        }
    }
}
fn prepare_local_clone(session: &mut Session, fixture: &Fixture, mouse: bool, bare: &Path) {
    manage(session, mouse);
    operation(session, mouse, 1, "Clone explicitly");
    field(session, mouse, 5, "Clone origin");
    if mouse {
        session.click_visible_action("[Explicit local bare repository]");
    } else {
        session.send_key(b" ");
    }
    session.wait_for_screen("Clone origin (only explicit Clone) • Explicit local bare repository");
    session.wait_for_screen("local-bare");
    set_field(
        session,
        mouse,
        6,
        "Local bare directory",
        bare.to_str().unwrap(),
    );
    preview(session, mouse, "no worktree checkout or fork-parent fetch");
    assert!(!manager(fixture)
        .managed_destination("org/app")
        .unwrap()
        .exists());
}
fn start_confirm(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.focus_visible_form_action("[Confirm action]");
        session.send_key(b"\r");
    }
}

#[test]
fn selected_corpus_keyboard_abrupt_application_death_retains_live_lock_and_explicit_orphan_recovery(
) {
    abrupt_transfer(false)
}
#[test]
fn selected_corpus_mouse_abrupt_application_death_retains_live_lock_and_explicit_orphan_recovery() {
    abrupt_transfer(true)
}
fn abrupt_transfer(mouse: bool) {
    let fixture = fixture();
    let bare = fixture.root.join("crash-source.git");
    let output = Command::new("git")
        .args(["clone", "--bare"])
        .arg(fixture.repo())
        .arg(&bare)
        .output()
        .unwrap();
    assert!(output.status.success());
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let steps = identities(12); // First load/preview/pre-submit, blocked load, restart/recover preview+submit.
    fixture.scripted_gh(&steps);
    let transfer = SlowTransfer::install(&fixture);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    search(&mut session, mouse);
    prepare_local_clone(&mut session, &fixture, mouse, &bare);
    start_confirm(&mut session, mouse);
    let (pid, partial) = transfer.wait_ready(&mut session);
    session.wait_for_screen("Clone progress • Transferring");
    assert!(manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap()
        .is_empty());
    session.terminate_abruptly();
    assert_eq!(
        unsafe { libc::kill(pid, 0) },
        0,
        "transfer must outlive sudden application death"
    );
    assert!(manager(&fixture)
        .orphans(&cache(&fixture), &CancellationToken::default())
        .unwrap_err()
        .0
        .contains("active"));
    let mut restarted = Session::launch(&fixture, 80, 24);
    restarted.wait_for_screen("Recent commits");
    search(&mut restarted, mouse);
    choose(&mut restarted, mouse, "[M Repositories]", b"M");
    restarted.wait_for_screen("corpus operation is active");
    assert!(!restarted.screen().contains("Manage selected repositories"));
    assert!(partial.exists());
    transfer.release();
    restarted
        .try_wait_for_condition(
            |_| transfer.finished.exists(),
            "explicit generated transfer release",
            Duration::from_secs(8),
        )
        .unwrap();
    // Lock release, rather than a PID-only heuristic, proves no writer can still own this transfer.
    let last_orphan_error = std::cell::RefCell::new(String::new());
    restarted
        .try_wait_for_condition(
            |_| match manager(&fixture).orphans(&cache(&fixture), &CancellationToken::default()) {
                Ok(_) => true,
                Err(error) => {
                    *last_orphan_error.borrow_mut() = error.to_string();
                    false
                }
            },
            "orphaned transfer releases inherited ownership lock",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| {
            panic!(
                "{error}; last ownership inspection error: {}",
                last_orphan_error.borrow()
            )
        });
    let orphans = manager(&fixture)
        .orphans(&cache(&fixture), &CancellationToken::default())
        .unwrap();
    assert_eq!(orphans.len(), 1);
    assert!(manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap()
        .is_empty());
    manage(&mut restarted, mouse);
    operation(&mut restarted, mouse, 7, "Quarantine owned orphan");
    field(&mut restarted, mouse, 10, "Exact owned orphan");
    if mouse {
        let (x, y) = restarted.locate("Exact owned orphan").unwrap();
        restarted.click(x + 2, y + 2);
    } else {
        restarted.send_key(b" ");
    }
    preview(
        &mut restarted,
        mouse,
        "do not activate, delete, resume or retry it.",
    );
    assert!(partial.exists());
    confirm(&mut restarted, mouse, "Owned orphan preserved in recovery");
    let recoveries = manager(&fixture)
        .recoveries(&cache(&fixture), &CancellationToken::default())
        .unwrap();
    assert_eq!(recoveries.len(), 1);
    assert!(recoveries[0].requires_acknowledgement && !recoveries[0].restorable);
    assert_eq!(
        fs::read(Path::new(recoveries[0].directory.as_os_str()).join("checkout/retained.env"))
            .unwrap(),
        b"retained generated partial bytes\n"
    );
    assert!(!partial.exists());
    assert!(manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    journal(&mut restarted, &fixture, steps.len());
    restarted.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn selected_corpus_keyboard_cancel_real_transfer_retains_partial_and_requires_explicit_acknowledgement(
) {
    cancel_transfer(false)
}
#[test]
fn selected_corpus_mouse_cancel_real_transfer_retains_partial_and_requires_explicit_acknowledgement(
) {
    cancel_transfer(true)
}
fn cancel_transfer(mouse: bool) {
    let fixture = fixture();
    let bare = fixture.root.join("cancel-source.git");
    let output = Command::new("git")
        .args(["clone", "--bare"])
        .arg(fixture.repo())
        .arg(&bare)
        .output()
        .unwrap();
    assert!(output.status.success());
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let steps = identities(11); // load/preview/pre-submit, reload, acknowledge preview+submit.
    fixture.scripted_gh(&steps);
    let transfer = SlowTransfer::install(&fixture);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    search(&mut session, mouse);
    prepare_local_clone(&mut session, &fixture, mouse, &bare);
    start_confirm(&mut session, mouse);
    let (pid, partial) = transfer.wait_ready(&mut session);
    session.wait_for_screen("Clone progress • Transferring");
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    session.wait_for_screen("Repository operation needs attention");
    assert!(
        !transfer.finished.exists(),
        "Cancelled helper must not continue its delayed tail"
    );
    session.try_wait_for_condition(|_| unsafe { libc::kill(pid, 0) } == -1
        && std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH),
        "actual clone child is killed and reaped", Duration::from_secs(8)).unwrap();
    let recoveries = manager(&fixture)
        .recoveries(&cache(&fixture), &CancellationToken::default())
        .unwrap();
    assert_eq!(recoveries.len(), 1);
    assert!(recoveries[0].requires_acknowledgement && !recoveries[0].restorable);
    assert!(manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert!(!partial.exists());
    assert_eq!(
        fs::read(Path::new(recoveries[0].directory.as_os_str()).join("checkout/retained.env"))
            .unwrap(),
        b"retained generated partial bytes\n"
    );
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_screen("Composer closed");
    session.resize(80, 24);
    manage(&mut session, mouse);
    operation(&mut session, mouse, 6, "Acknowledge inspected partial");
    field(&mut session, mouse, 9, "Exact recovery identity");
    if mouse {
        let (x, y) = session.locate("Exact recovery identity").unwrap();
        session.click(x + 2, y + 2);
    } else {
        session.send_key(b" ");
    }
    preview(
        &mut session,
        mouse,
        "Do not remove bytes, restore or automatically retry a clone.",
    );
    confirm(&mut session, mouse, "Inspected recovery acknowledged");
    let retained = manager(&fixture)
        .recoveries(&cache(&fixture), &CancellationToken::default())
        .unwrap();
    assert_eq!(retained.len(), 1);
    assert!(!retained[0].requires_acknowledgement);
    assert_eq!(
        fs::read(Path::new(retained[0].directory.as_os_str()).join("checkout/retained.env"))
            .unwrap(),
        b"retained generated partial bytes\n"
    );
    assert!(manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn selected_corpus_keyboard_existing_map_preview_cancel_search_restart_and_clear_preserve_intent() {
    mapping(false)
}
#[test]
fn selected_corpus_mouse_existing_map_preview_cancel_search_restart_and_clear_preserve_intent() {
    mapping(true)
}
fn mapping(mouse: bool) {
    let fixture = fixture();
    fs::write(
        fixture.repo().join("nested/日本語.txt"),
        b"working corpus evidence\n",
    )
    .unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut steps = identities(10); // load, cancelled preview, reload, preview, submit: 20 reads.
    steps.extend(identities(2)); // Account-scoped local Search verifies before and after.
    steps.extend(identities(2)); // Inbox startup and refresh.
    for qualifier in ["author", "assignee", "review-requested"] {
        steps.push(GhStep::read(
            &[
                "api",
                "search/issues",
                "--hostname",
                "github.invalid",
                "--method",
                "GET",
                "-f",
                &format!("q=is:pr is:open {qualifier}:alice repo:org/app"),
                "-f",
                "per_page=100",
                "-f",
                "page=1",
            ],
            "{\"total_count\":0,\"incomplete_results\":false,\"items\":[]}",
        ));
    }
    steps.extend(identities(2)); // Restart menu restores durable mappings without clone.
    steps.extend(identities(2)); // Restored account-scoped local Search principal checks.
    steps.extend(identities(6)); // Explicit Unmap load/preview/submit preserves the user checkout.
    steps.extend(identities(2)); // Empty account-scoped Search still verifies its principal.
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    search(&mut session, mouse);
    manage(&mut session, mouse);
    set_field(
        &mut session,
        mouse,
        2,
        "Existing directory",
        fixture.repo().to_str().unwrap(),
    );
    preview(
        &mut session,
        mouse,
        "Never clone, checkout, fetch or delete it",
    );
    assert!(manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap()
        .is_empty());
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_screen("Composer closed");
    manage(&mut session, mouse);
    session.resize(80, 24);
    preview(&mut session, mouse, "Map this existing checkout");
    confirm(&mut session, mouse, "Mapped existing repository");
    // The mapped checkout's observed size and revision scope are visible.
    session.wait_for_screen("B on disk");
    query(&mut session, mouse, "working corpus evidence");
    session.wait_for_screen("1 results • Exact");
    session.wait_for_screen("org/app");
    let mappings = manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap();
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].path(), fixture.repo());
    assert!(mappings[0].active);
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Inbox • focused view");
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("No pull requests");
    session.wait_for_screen("Fresh • fetched");
    assert!(
        !cache(&fixture)
            .list(&Scope::github("github.invalid", Some(42)).unwrap(), None)
            .unwrap()
            .is_empty(),
        "Clear must remove real derived records, not an already-empty cache"
    );
    choose(&mut session, mouse, "[x Clear cache]", b"x");
    session.wait_for_screen("No loaded records");
    assert!(cache(&fixture)
        .list(&Scope::github("github.invalid", Some(42)).unwrap(), None)
        .unwrap()
        .is_empty());
    assert_eq!(
        manager(&fixture)
            .catalog(&cache(&fixture), &CancellationToken::default())
            .unwrap(),
        mappings
    );
    let mut restarted = session.restart_with_same_cache(&fixture, 80, 24);
    restarted.wait_for_screen("Recent commits");
    search(&mut restarted, mouse);
    manage(&mut restarted, mouse);
    restarted.wait_for_screen("org/app");
    choose(&mut restarted, mouse, "[Keep draft / Close]", b"\x1b");
    restarted.wait_for_screen("Composer closed");
    query(&mut restarted, mouse, "working corpus evidence");
    restarted.wait_for_screen("1 results • Exact");
    manage(&mut restarted, mouse);
    operation(&mut restarted, mouse, 3, "Unmap only");
    preview(
        &mut restarted,
        mouse,
        "Preserve every checkout and Git object on disk.",
    );
    confirm(&mut restarted, mouse, "Removed from selected corpus");
    query(&mut restarted, mouse, "working corpus evidence");
    restarted.wait_for_screen("Select an existing local checkout");
    restarted.wait_for_screen("no selected local corpus");
    assert!(
        !restarted.screen().contains("日本語.txt:1"),
        "Unmapped results must disappear; an empty source is not fabricated successful coverage"
    );
    assert!(
        !manager(&fixture)
            .catalog(&cache(&fixture), &CancellationToken::default())
            .unwrap()[0]
            .active
    );
    assert!(fixture.repo().join(".git").is_dir());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("nested/日本語.txt")).unwrap(),
        b"working corpus evidence\n"
    );
    journal(&mut restarted, &fixture, steps.len());
    restarted.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn selected_corpus_keyboard_explicit_clone_fetch_quarantine_restore_preserve_objects_and_ignored_bytes(
) {
    clone_management(false)
}
#[test]
fn selected_corpus_mouse_explicit_clone_fetch_quarantine_restore_preserve_objects_and_ignored_bytes(
) {
    clone_management(true)
}
fn clone_management(mouse: bool) {
    let fixture = fixture();
    let bare = fixture.root.join("explicit-bare.git");
    let output = Command::new("git")
        .args(["clone", "--bare"])
        .arg(fixture.repo())
        .arg(&bare)
        .output()
        .unwrap();
    assert!(output.status.success());
    let source_head = fixture.git(&["rev-parse", "HEAD"]);
    let source_index = fs::read(fixture.repo().join(".git/index")).unwrap();
    // Four explicit menu/preview/submit cycles plus two account-scoped local
    // searches, each independently verifying its principal before and after.
    let steps = identities(28);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    search(&mut session, mouse);
    manage(&mut session, mouse);
    operation(&mut session, mouse, 1, "Clone explicitly");
    field(&mut session, mouse, 5, "Clone origin");
    if mouse {
        session.click_visible_action("[Explicit local bare repository]");
    } else {
        session.send_key(b" ");
    }
    session.wait_for_screen("Clone origin (only explicit Clone) • Explicit local bare repository");
    session.wait_for_screen("local-bare");
    set_field(
        &mut session,
        mouse,
        6,
        "Local bare directory",
        bare.to_str().unwrap(),
    );
    preview(
        &mut session,
        mouse,
        "no worktree checkout or fork-parent fetch",
    );
    let destination = manager(&fixture).managed_destination("org/app").unwrap();
    assert!(!destination.exists());
    confirm(&mut session, mouse, "Explicit managed clone ready");
    let mappings = manager(&fixture)
        .catalog(&cache(&fixture), &CancellationToken::default())
        .unwrap();
    assert_eq!(mappings.len(), 1);
    assert_eq!(mappings[0].path(), destination);
    assert_eq!(git_at(&destination, &["rev-parse", "HEAD"]), source_head);
    assert!(!destination.join("nested/日本語.txt").exists());
    assert_eq!(
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        source_index
    );
    query(&mut session, mouse, "saved corpus evidence");
    session.wait_for_screen("1 results • Exact");
    session.wait_for_screen("saved refs/remotes/origin/HEAD");
    fixture.commit_file("next.txt", b"new fetched evidence\n", "New source version");
    fixture.git(&["push", bare.to_str().unwrap(), "main"]);
    let fixture_head_after_commit = fixture.git(&["rev-parse", "HEAD"]);
    let fixture_index_after_commit = fs::read(fixture.repo().join(".git/index")).unwrap();
    manage(&mut session, mouse);
    operation(&mut session, mouse, 2, "Fetch explicitly");
    preview(
        &mut session,
        mouse,
        "no checkout, merge or implicit extra repositories",
    );
    assert_eq!(git_at(&destination, &["rev-parse", "HEAD"]), source_head);
    confirm(&mut session, mouse, "Explicit fetch completed");
    assert_eq!(git_at(&destination, &["rev-parse", "HEAD"]), source_head);
    assert_eq!(
        git_at(&destination, &["rev-parse", "refs/remotes/origin/main"]),
        fixture.git(&["rev-parse", "HEAD"])
    );
    query(&mut session, mouse, "new fetched evidence");
    session.wait_for_screen("1 results • Exact");
    fs::write(
        destination.join(".ignored.env"),
        b"retained private fixture bytes\n",
    )
    .unwrap();
    fs::write(destination.join(".git/info/exclude"), b".ignored.env\n").unwrap();
    session.resize(80, 24);
    manage(&mut session, mouse);
    operation(&mut session, mouse, 4, "Quarantine owned");
    preview(&mut session, mouse, "Never erase user files");
    confirm(&mut session, mouse, "Owned directory moved to recovery");
    assert!(!destination.exists());
    let recovery = manager(&fixture)
        .recoveries(&cache(&fixture), &CancellationToken::default())
        .unwrap();
    assert_eq!(recovery.len(), 1);
    let retained = Path::new(recovery[0].directory.as_os_str());
    assert_eq!(
        fs::read(retained.join(".ignored.env")).unwrap(),
        b"retained private fixture bytes\n"
    );
    assert_eq!(git_at(retained, &["rev-parse", "HEAD"]), source_head);
    assert!(
        !manager(&fixture)
            .catalog(&cache(&fixture), &CancellationToken::default())
            .unwrap()[0]
            .active
    );
    manage(&mut session, mouse);
    operation(&mut session, mouse, 5, "Restore recovery");
    preview(
        &mut session,
        mouse,
        "without replacing an existing directory",
    );
    confirm(&mut session, mouse, "Restored exact owned directory");
    assert_eq!(
        fs::read(destination.join(".ignored.env")).unwrap(),
        b"retained private fixture bytes\n"
    );
    assert!(manager(&fixture)
        .recoveries(&cache(&fixture), &CancellationToken::default())
        .unwrap()
        .is_empty());
    assert!(
        manager(&fixture)
            .catalog(&cache(&fixture), &CancellationToken::default())
            .unwrap()[0]
            .active
    );
    assert!(!destination.join("next.txt").exists());
    // Source writes above were explicit fixture commits, not writes from management.
    assert_eq!(
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fixture_index_after_commit
    );
    assert_eq!(
        fixture.git(&["rev-parse", "HEAD"]),
        fixture_head_after_commit
    );
    journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
}
