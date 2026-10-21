//! UNREGISTERED normal-product queue journeys. The one unrelated fake-gh
//! version read occupies a worker; it is not navigation authentication/proof.
use super::*;
use crate::support::GhStep;

#[test]
fn original_saved_queued_keyboard_cancel_before_start_has_zero_repository_child() {
    queued(false);
}
#[test]
fn original_saved_queued_mouse_cancel_before_start_has_zero_repository_child() {
    queued(true);
}

fn alive(pid: i32) -> bool {
    (unsafe { libc::kill(pid, 0) }) == 0
}
fn until(session: &mut Session, ready: impl Fn() -> bool, message: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !ready() {
        session.poll_output(Duration::from_millis(5));
        assert!(Instant::now() < deadline, "{message}\n{}", session.screen());
    }
}
fn queued(mouse: bool) {
    let native = fixture("sha256");
    let mut version = GhStep::read(&["--version"], "gh version 2.90.1\n");
    version.delay_ms = 30_000; // Only a live-PID fence proves entry, never this delay alone.
    native.fixture.scripted_gh(&[version]);
    let installed = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(installed.status.success());
    let installed = String::from_utf8(installed.stdout)
        .unwrap()
        .trim()
        .to_owned();
    let root = native.fixture.root.display().to_string();
    let git = native.fixture.root.join("bin/git");
    // Initial/local Tree uses nonrecursive ls-tree; only the explicitly
    // submitted saved-source Search read reaches this real command boundary.
    let script=format!("#!/bin/sh\ncase \" $* \" in *' ls-tree -r '*) '{installed}' \"$@\" > '{root}/queued-held-tree-output' || exit $?; printf '%s\\n' \"$$\" > '{root}/queued-held-tree-pid'; while [ ! -f '{root}/queued-release-tree' ]; do /bin/sleep 0.02; done; /bin/cat '{root}/queued-held-tree-output';; *) exec '{installed}' \"$@\";; esac\n");
    fs::write(&git, script).unwrap();
    fs::set_permissions(&git, fs::Permissions::from_mode(0o755)).unwrap();
    let mut session = launch(&native, mouse);
    saved_file(&native, &mut session, mouse);
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
    session.search_option(mouse, "e Exact", b"e");
    session.search_option(mouse, "v Saved version", b"v");
    session.wait_for_screen("Search scope changed");
    choose(&mut session, mouse, "[/ Query]", b"/");
    session.paste("future");
    choose(&mut session, mouse, "[Enter Search]", b"\r");
    let tree_pid_path = native.fixture.root.join("queued-held-tree-pid");
    until(
        &mut session,
        || {
            fs::read_to_string(&tree_pid_path).is_ok_and(|text| {
                text.ends_with('\n') && text.trim().parse::<i32>().is_ok_and(|pid| pid > 0)
            })
        },
        "Actual saved-revision Search child never occupied first worker",
    );
    let tree_pid = fs::read_to_string(&tree_pid_path)
        .unwrap()
        .trim()
        .parse::<i32>()
        .unwrap();
    assert!(alive(tree_pid));
    assert!(
        !fs::read(native.fixture.root.join("queued-held-tree-output"))
            .unwrap()
            .is_empty()
    );
    assert!(starts(&native).iter().any(|event| event["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "-r")));
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Use Refresh to load your authenticated GitHub inbox.");
    choose(&mut session, mouse, "[r Refresh]", b"r");
    until(
        &mut session,
        || {
            let records = session_journal(&native);
            records.len() == 1
        },
        "Actual unrelated strict version child never occupied second worker",
    );
    let journal = session_journal(&native);
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0]["args"], json!(["--version"]));
    assert_eq!(journal[0]["matched"], true);
    let gh_pid = journal[0]["pid"].as_i64().unwrap() as i32;
    assert!(alive(tree_pid) && alive(gh_pid),"Both actual runner waits must occupy the two ordinary Background slots before queueing Repo");
    choose(&mut session, mouse, "2 Repository", b"2");
    session.wait_for_screen("saved nested line 002");
    let prior = starts(&native).len();
    choose(&mut session, mouse, "[f File history]", b"f");
    session.wait_for_screen("Reading original local source");
    assert!(alive(tree_pid) && alive(gh_pid));
    assert_eq!(
        starts(&native).len(),
        prior,
        "Original Repo request must still be queued, not merely started then canceled"
    );
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    session.wait_for_screen("Cancelled");
    assert!(alive(tree_pid) && alive(gh_pid));
    assert_eq!(starts(&native).len(), prior);
    assert!(no_path_history(&session), "{}", session.screen());
    choose(&mut session, mouse, "1 Inbox", b"1");
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    session.wait_for_screen("Cancelled");
    until(
        &mut session,
        || !alive(gh_pid),
        "Unrelated canceled gh version child must be reaped",
    );
    choose(&mut session, mouse, "3 Search", b"3");
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    session.wait_for_screen("Cancelled");
    until(
        &mut session,
        || !alive(tree_pid),
        "Canceled actual saved Search child must be reaped",
    );
    fs::write(
        native.fixture.root.join("queued-release-tree"),
        b"released after observed cancel cleanup\n",
    )
    .unwrap();
    choose(&mut session, mouse, "2 Repository", b"2");
    session.wait_for_screen("saved nested line 002");
    session.resize(80, 24);
    session.wait_for_screen(&native.chosen);
    session.resize(120, 40);
    session.wait_for_screen("saved nested line 002");
    assert_eq!(
        starts(&native).len(),
        prior,
        "Releasing/reaping other workers cannot start the canceled original Repo read"
    );
    assert!(no_path_history(&session), "{}", session.screen());
    assert!(!session.screen().contains("GitHub reports this commit"));
    assert_eq!(state(&native.fixture), native.before);
    assert_eq!(sql(&native.fixture), native.sql);
    assert_eq!(
        fs::read(native.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        native.config
    );
    assert!(fs::read(native.fixture.root.join("curl-journal"))
        .unwrap()
        .is_empty());
    let journal = session_journal(&native);
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0]["args"], json!(["--version"]));
    assert_eq!(journal[0]["matched"], true);
    let trace = fs::read_to_string(native.fixture.root.join("nested-git-trace.jsonl")).unwrap();
    assert!(!trace
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .any(|event| event["event"] == "cmd_name"
            && matches!(
                event["name"].as_str(),
                Some(
                    "add"
                        | "reset"
                        | "checkout"
                        | "switch"
                        | "commit"
                        | "fetch"
                        | "clone"
                        | "push"
                        | "update-ref"
                )
            )));
    session.quit_and_assert_restored_terminal(mouse);
}
fn session_journal(native: &Native) -> Vec<Value> {
    // Same original public fixture journal, read only to fence actual entry.
    let path = native.fixture.root.join("gh-fixture/commands.jsonl");
    let Ok(bytes) = fs::read_to_string(path) else {
        return vec![];
    };
    if !bytes.ends_with('\n') {
        return vec![];
    } // Entry is not fully emitted yet.
    bytes
        .lines()
        .map(|line| {
            serde_json::from_str(line)
                .expect("Fully emitted strict journal entry must remain valid JSON")
        })
        .collect()
}
