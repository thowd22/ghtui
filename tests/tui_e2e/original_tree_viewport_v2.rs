//! Actual binary input/effect journeys for the original saved-tree viewport.
//! No UTF8 Trace2 argv is ever used to identify a raw Git destination.
use super::*;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession, ProcessIdentity};
use crate::support::GhStep;
const PALETTE_V2: &str = "Commands · current context";
const FOLDER_V2: &str = "Saved folder • original detail";

#[test]
fn original_tree_viewport_keyboard_palette40_same_index_reason_resize_and_no_reads() {
    palette40(false);
}
#[test]
fn original_tree_viewport_mouse_palette40_same_index_reason_resize_and_no_reads() {
    palette40(true);
}
fn palette40(mouse: bool) {
    let native = fixture(if mouse { "sha256" } else { "sha1" });
    install_git_observer(&native, None);
    let mut session = launch(&native, mouse);
    saved_root(&native, &mut session, mouse, "Chosen immutable rename");
    jump(&mut session, mouse);
    filter_v2(&mut session, mouse, "nested/renamed.txt", "1 known matches");
    let before = current_count(&native);
    narrow_jump(&mut session, 40);
    commands_v2(&mut session, mouse, true);
    session.send_key(b"\x01");
    session.paste("repository.tree.previous");
    session.send_key(b"\t");
    session.wait_for_cursorless_condition(
        |frame| {
            frame
                .contents()
                .contains("> Previous100 original saved paths")
                && frame.contents().contains("ID: repository.tree.previous")
                && frame.contents().contains("[Enter Choose]")
                && frame.contents().contains("[Esc Close]")
        },
        "actual34-cell selected original index/detail and controls",
        Duration::from_secs(8),
    );
    for label in ["[Pg↑]", "[Pg↓]", "[−]", "[+]", "[Tab]"] {
        session.wait_for_screen(label);
    }
    session.resize(80, 24);
    session.wait_for_cursorless_condition(
        |frame| {
            frame
                .contents()
                .contains("> Previous100 original saved paths [why unavailable]")
                && frame.contents().contains("ID: repository.tree.previous")
        },
        "same catalogue/index at actual74-cell width",
        Duration::from_secs(8),
    );
    session.resize(40, 24);
    session.wait_for_cursorless_condition(
        |frame| {
            frame
                .contents()
                .contains("> Previous100 original saved paths")
                && frame.contents().contains("ID: repository.tree.previous")
        },
        "same original index retained after40→80→40",
        Duration::from_secs(8),
    );
    if mouse {
        session.click_visible_action("Selected command");
    } else {
        session.send_key(b"\t");
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(5));
        if session
            .visible_overlay_body(PALETTE_V2)
            .is_some_and(|body| {
                // Both actual displayed fragments must be visible. Detail rows
                // break after a space, so the trimmed rows rejoin with one.
                body.lines()
                    .map(str::trim)
                    .collect::<Vec<_>>()
                    .join(" ")
                    .contains("unseen paths remain unknown")
            })
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Exact disabled original reason suffix cannot be reached at34cells\n{}",
            session.screen()
        );
        if mouse {
            let (x, y) = session.locate("Selected command").unwrap();
            session.wheel(x + 2, y + 2, true);
        } else {
            session.send_key(b"\x1b[B");
        }
    }
    assert_eq!(
        current_count(&native),
        before,
        "Width-only commands reuse SAME source catalogue; no engine/provider read"
    );
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.focus_visible_form_action("[Esc Close]");
        session.send_key(b"\r");
    }
    session.wait_for_overlay_close(PALETTE_V2);
    session.wait_for_screen(TITLE);
    session.wait_for_screen("1 known matches");
    narrow_jump(&mut session, 120);
    choose_focus(&mut session, mouse, "[Esc Close]");
    session.wait_for_overlay_close(TITLE);
    assert_eq!(current_count(&native), before);
    effects_v2(&native, 0);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn original_tree_viewport_keyboard_deep_body_focus_folders_and_width40_exact_detail() {
    deep_body(false);
}
#[test]
fn original_tree_viewport_mouse_deep_body_focus_folders_and_width40_exact_detail() {
    deep_body(true);
}
#[test]
fn original_tree_viewport_keyboard_distinct_raw_objects_exact_byte_argv_and_compact_paging() {
    raw_objects(false);
}
#[test]
fn original_tree_viewport_mouse_distinct_raw_objects_exact_byte_argv_and_compact_paging() {
    raw_objects(true);
}
#[test]
fn original_tree_viewport_keyboard_actual_full_queue_owned_error_and_new_explicit_retry() {
    full_queue(false);
}
#[test]
fn original_tree_viewport_mouse_actual_full_queue_owned_error_and_new_explicit_retry() {
    full_queue(true);
}
#[test]
fn original_tree_viewport_keyboard_close_reaps_actual_inventory_descendant_no_late_result() {
    cancel_descendant(false);
}
#[test]
fn original_tree_viewport_mouse_close_reaps_actual_inventory_descendant_no_late_result() {
    cancel_descendant(true);
}

fn shell_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
fn actual_git() -> String {
    let output = Command::new("sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(output.status.success());
    let path = String::from_utf8(output.stdout).unwrap().trim().to_owned();
    assert!(Path::new(&path).is_absolute());
    path
}
/// A transparent per-child NUL argv record, BEFORE exec of the real installed
/// Git. Test setup Git is outside child PATH and outside this observation.
fn install_git_observer(native: &Native, held: Option<&str>) {
    let root = shell_literal(native.fixture.root.to_str().unwrap());
    let installed = shell_literal(&actual_git());
    let directory = native.fixture.root.join("v2-raw-argv");
    fs::create_dir(&directory).unwrap();
    let hold = match held {
        None => String::new(),
        Some("inventory") => "case \" $* \" in *' ls-tree -r -t -z --full-tree '*) kind=inventory;; *) kind=;; esac\n".into(),
        Some("search") => {
            let head=native.fixture.git(&["rev-parse","HEAD"]);
            assert_ne!(head.trim(),native.chosen);
            format!("case \" $* \" in *' ls-tree -r -t -z --full-tree {} '*) kind=search;; *) kind=;; esac\n",head.trim())
        }
        Some(other) => panic!("Unexpected test-only hold kind {other}"),
    };
    let gate = if held.is_some() {
        format!(
            "{hold}if [ -n \"$kind\" ] && [ ! -f \"$root/v2-$kind-pid\" ]; then\n\
             {installed} \"$@\" > \"$root/v2-$kind-output\"; result=$?\n\
             printf '%s\\n' \"$$\" > \"$root/v2-$kind-pid\"\n\
             /bin/sleep 30 & descendant=$!\n\
             printf '%s\\n' \"$descendant\" > \"$root/v2-$kind-descendant\"\n\
             while [ ! -f \"$root/v2-release-$kind\" ]; do /bin/sleep 0.02; done\n\
             kill \"$descendant\" 2>/dev/null; wait \"$descendant\" 2>/dev/null\n\
             /bin/cat \"$root/v2-$kind-output\"; exit \"$result\"\n\
             fi\n"
        )
    } else {
        String::new()
    };
    let program = native.fixture.root.join("bin/git");
    fs::write(
        &program,
        format!(
            "#!/bin/sh\nset -C\nroot={root}\nprintf '%s\\0' \"$@\" > \"$root/v2-raw-argv/$$\" || exit 91\n{gate}exec {installed} \"$@\"\n"
        ),
    )
    .unwrap();
    fs::set_permissions(program, fs::Permissions::from_mode(0o755)).unwrap();
}
fn argv_records(native: &Native) -> Vec<Vec<Vec<u8>>> {
    raw_records_v2(native)
        .into_iter()
        .map(|record| record.argv)
        .collect()
}
fn raw_records_v2(native: &Native) -> Vec<crate::raw_git_argv_v1::RawGitArgvV1> {
    let mut files = fs::read_dir(native.fixture.root.join("v2-raw-argv"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert!(
        files.len() <= 512,
        "Actual test child count exceeded bounded oracle"
    );
    files.sort();
    files
        .iter()
        .map(|file| {
            let pid = file
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .parse::<u32>()
                .unwrap();
            assert!(pid > 0);
            let bytes = fs::read(file).unwrap();
            assert!(bytes.len() <= 64 * 1024);
            assert!(bytes.ends_with(&[0]), "Complete NUL argv record required");
            let argv = bytes[..bytes.len() - 1]
                .split(|byte| *byte == 0)
                .map(<[u8]>::to_vec)
                .collect();
            crate::raw_git_argv_v1::RawGitArgvV1 { pid, argv }
        })
        .collect()
}
fn raw_delta_v2(
    native: &Native,
    previous: &std::collections::BTreeSet<u32>,
    member: &(Vec<u8>, String, String),
) {
    let delta = raw_records_v2(native)
        .into_iter()
        .filter(|record| !previous.contains(&record.pid))
        .collect::<Vec<_>>();
    assert!(
        !delta.is_empty(),
        "This selection must produce NEW genuine installed Git children, not ever-seen records"
    );
    crate::raw_git_argv_v1::exact_read(&delta, &native.chosen, &member.0, &member.1);
    assert_eq!(
        crate::raw_git_argv_v1::original_blob(
            native.fixture.repo().as_path(),
            &native.chosen,
            &member.0
        ),
        (member.1.clone(), format!("{}\n", member.2).into_bytes())
    );
}
fn new_raw_body_v2(session: &mut Session, previous: usize, body: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(5));
        if session.completed_cursorless_end() > previous
            && session
                .completed_cursorless_screen()
                .is_some_and(|frame| frame.contents().contains(body))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "No NEW completed frame for exact original raw body {body:?}\n{}",
            session.screen()
        );
    }
}
/// Git's own Trace2 can include nonUTF8 argv. Lossy decode is restricted to
/// finite command-name/count telemetry, never resource/path/member matching.
fn trace_names(native: &Native) -> Vec<String> {
    let bytes = fs::read(native.fixture.root.join("nested-git-trace.jsonl")).unwrap();
    assert!(bytes.len() <= 8 * 1024 * 1024);
    assert!(bytes.ends_with(b"\n"));
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            let event: Value = serde_json::from_str(&String::from_utf8_lossy(line))
                .expect("Every emitted Trace2 line must parse; no default acceptance");
            if event["event"] == "cmd_name" {
                Some(
                    event["name"]
                        .as_str()
                        .expect("Actual command-name string required")
                        .to_owned(),
                )
            } else {
                None
            }
        })
        .collect()
}
fn inventory_count(native: &Native) -> usize {
    argv_records(native)
        .iter()
        .filter(|args| {
            [b"ls-tree".as_slice(), b"-r", b"-t", b"-z", b"--full-tree"]
                .iter()
                .all(|expected| args.iter().any(|arg| arg.as_slice() == *expected))
                && args
                    .last()
                    .is_some_and(|arg| arg == native.chosen.as_bytes())
        })
        .count()
}
fn effects_v2(native: &Native, gh_count: usize) {
    assert_eq!(state(&native.fixture), native.before);
    assert_eq!(sql(&native.fixture), native.sql);
    assert_eq!(
        fs::read(native.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        native.config
    );
    assert!(fs::read(native.fixture.root.join("curl-journal"))
        .unwrap()
        .is_empty());
    let journal = strict_journal(native);
    assert_eq!(
        journal.len(),
        gh_count,
        "Exact actual strict-gh child count"
    );
    for command in &journal {
        assert_eq!(
            command["matched"], true,
            "No accepted-by-default gh request"
        );
        assert_eq!(
            command["args"],
            json!(["--version"]),
            "The only permitted nonzero journal is the explicitly occupied version-error fixture"
        );
    }
    let names = trace_names(native);
    assert!(!names.is_empty(), "Real installed Git telemetry must exist");
    for name in names {
        assert!(
            matches!(
                name.as_str(),
                "rev-parse"
                    | "status"
                    | "ls-files"
                    | "for-each-ref"
                    | "config"
                    | "diff"
                    // The History Commit detail's changed files (TASK-99.4).
                    | "diff-tree"
                    | "log"
                    | "cat-file"
                    | "ls-tree"
                    | "rev-list"
                    | "merge-base"
                    | "symbolic-ref"
                    | "check-attr"
                    | "show-ref"
                    | "blame"
                    | "reflog"
                    | "stash"
                    | "worktree"
                    | "hash-object"
            ),
            "Unexpected actual Git command, not a read-only blacklist omission: {name}"
        );
    }
    for args in argv_records(native) {
        let command = args
            .iter()
            .position(|arg| {
                [
                    b"rev-parse".as_slice(),
                    b"status",
                    b"ls-files",
                    b"for-each-ref",
                    b"config",
                    b"diff",
                    b"diff-tree",
                    b"log",
                    b"cat-file",
                    b"ls-tree",
                    b"rev-list",
                    b"merge-base",
                    b"symbolic-ref",
                    b"check-attr",
                    b"show-ref",
                    b"blame",
                    b"reflog",
                    b"stash",
                    b"worktree",
                    b"hash-object",
                ]
                .contains(&arg.as_slice())
            })
            .expect("Raw argv must contain an allowed installed-Git read");
        let tail = &args[command + 1..];
        match args[command].as_slice() {
            b"config" => assert!(
                tail.iter().any(|arg| [
                    b"--list".as_slice(),
                    b"--get",
                    b"--get-regexp",
                    b"--get-all",
                    b"--default=false",
                ]
                .contains(&arg.as_slice())),
                "Git config must be explicit read/default query"
            ),
            b"symbolic-ref" => assert!(
                tail.iter()
                    .all(|arg| arg.starts_with(b"-") || arg == b"HEAD"),
                "No symbolic-ref update destination"
            ),
            b"stash" => assert_eq!(tail.first().unwrap(), b"list"),
            b"worktree" => assert_eq!(tail.first().unwrap(), b"list"),
            b"reflog" => assert_eq!(tail.first().unwrap(), b"show"),
            b"hash-object" => assert!(
                !tail.iter().any(|arg| arg == b"-w"),
                "Hash-only empty tree observations must not write objects"
            ),
            _ => {}
        }
        assert!(
            !tail.iter().any(|arg| [
                b"--output".as_slice(),
                b"--delete",
                b"--replace-all",
                b"--add",
                b"--remove-section",
                b"--rename-section",
                b"--unset",
                b"--unset-all",
            ]
            .contains(&arg.as_slice())),
            "Read argument policy must reject explicit write flags"
        );
    }
}
fn current_count(native: &Native) -> usize {
    argv_records(native).len()
}
fn until_v2(session: &mut Session, predicate: impl Fn() -> bool, message: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        session.poll_output(Duration::from_millis(5));
        assert!(Instant::now() < deadline, "{message}\n{}", session.screen());
    }
}
fn alive_v2(pid: i32) -> bool {
    process_identity(pid)
        .unwrap()
        .is_some_and(|identity| !identity.zombie)
}
fn positive_pid(path: &Path) -> Option<i32> {
    fs::read_to_string(path)
        .ok()
        .filter(|text| text.ends_with('\n'))
        .and_then(|text| text.trim().parse::<i32>().ok())
        .filter(|pid| *pid > 0)
}
fn entered_pid(
    native: &Native,
    session: &mut Session,
    kind: &str,
) -> (ProcessIdentity, ProcessIdentity) {
    let parent = native.fixture.root.join(format!("v2-{kind}-pid"));
    let descendant = native.fixture.root.join(format!("v2-{kind}-descendant"));
    until_v2(
        session,
        || positive_pid(&parent).is_some() && positive_pid(&descendant).is_some(),
        "Actual wrapper/descendant newline-complete PID entry required",
    );
    let pair = (
        process_identity(positive_pid(&parent).unwrap())
            .unwrap()
            .unwrap(),
        process_identity(positive_pid(&descendant).unwrap())
            .unwrap()
            .unwrap(),
    );
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    assert!(owner.owns(&pair.0) && owner.owns(&pair.1));
    assert!(alive_v2(pair.0.pid) && alive_v2(pair.1.pid));
    assert_eq!(pair.1.parent, pair.0.pid);
    pair
}
fn reaped(
    native: &Native,
    session: &mut Session,
    kind: &str,
    pair: (ProcessIdentity, ProcessIdentity),
) {
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    until_v2(
        session,
        || {
            [&pair.0, &pair.1].iter().all(|expected| {
                let Some(current) = process_identity(expected.pid).unwrap() else {
                    return true;
                };
                assert!(owner.owns(&current));
                assert_eq!(
                    (current.pid, current.uid, current.session, current.birth),
                    (expected.pid, expected.uid, expected.session, expected.birth),
                    "Only the original actual owned process may be observed/reaped"
                );
                // Canonical PTY harness is a Linux subreaper. A canceled
                // descendant can therefore be an ACTUAL adopted zombie here;
                // signal0 is not evidence it is still running or already gone.
                if current.zombie && current.parent == std::process::id() as i32 {
                    let mut status = 0;
                    // SAFETY: exact positive original owned UID/birth/SID, now
                    // an observed adopted child. Nonblocking, no global wait.
                    let waited = unsafe { libc::waitpid(current.pid, &mut status, libc::WNOHANG) };
                    assert_eq!(
                        waited, current.pid,
                        "Owned adopted zombie must actually reap"
                    );
                    assert!(libc::WIFEXITED(status) || libc::WIFSIGNALED(status));
                    eprintln!(
                        "Actual original owned {kind} child {} adopted zombie reaped by harness",
                        current.pid
                    );
                    assert!(process_identity(current.pid).unwrap().is_none());
                    true
                } else {
                    false
                }
            })
        },
        "Actual child and long-lived descendant must both be gone",
    );
    fs::write(
        native.fixture.root.join(format!("v2-release-{kind}")),
        b"release only after observed cleanup\n",
    )
    .unwrap();
}
fn choose_focus(session: &mut Session, mouse: bool, label: &str) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.focus_visible_action(label);
        session.send_key(b"\r");
    }
}
fn field_v2(session: &mut Session, mouse: bool, label: &str) {
    choose_focus(session, mouse, &format!("[{label}]"));
    session.wait_for_screen(&format!("Detail: {label}"));
}
fn narrow_jump(session: &mut Session, width: u16) {
    let failed = session.screen().contains("[Retry]");
    session.resize_owned_modal(
        width,
        24,
        TITLE,
        if failed {
            &["[Retry]", "[Cmds]", "[Esc Close]"]
        } else if width == 40 {
            &["[Open]", "[Cmds]", "[Esc Close]"]
        } else {
            &["[Enter Open]", "[Esc Close]"]
        },
    );
}
/// Fields are prepared for the ACTUAL full-screen inner38-cell viewport.
/// Every visual fragment, including final suffix, must become visible through
/// genuine focus/wheel/key scrolling. No serialized App state is consulted.
fn full_field_v2(session: &mut Session, mouse: bool, value: &str, title: &str) {
    assert!(value.is_ascii());
    let chunks = value
        .as_bytes()
        .chunks(38)
        .map(|bytes| std::str::from_utf8(bytes).unwrap())
        .collect::<Vec<_>>();
    if !mouse {
        session.send_key(b"\x1b[H");
    }
    for chunk in chunks {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            session.poll_output(Duration::from_millis(5));
            let body = session.wait_for_overlay_body(title);
            if body.contains(chunk) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Exact visual field fragment unavailable: {chunk:?}\n{}",
                session.screen()
            );
            if mouse {
                // Field activation owns the actual detail panel; click/wheel
                // uses its visible caption/value rather than underlying view.
                let (x, y) = session
                    .locate("Detail:")
                    .expect("Owned detail caption must remain visible");
                session.wheel(x + 2, y + 2, true);
            } else {
                session.send_key(b"\x1b[B");
            }
        }
    }
}
fn commands_v2(session: &mut Session, mouse: bool, narrow: bool) {
    let owned = session.screen().contains(TITLE);
    if mouse {
        session.click_visible_action(if owned {
            if narrow {
                "[Cmds]"
            } else {
                "[Commands]"
            }
        } else if session.screen().contains(": Cmds") {
            "[: Cmds]"
        } else {
            "[: Commands]"
        });
    } else {
        session.send_key(if owned { b"\x10" } else { b":" });
    } // Scoped CtrlP, not ':' in Filter; normal global palette unchanged.
    session.wait_for_screen(PALETTE_V2);
    session.wait_for_overlay_body(PALETTE_V2);
}
fn palette_v2(session: &mut Session, mouse: bool, query: &str, label: &str) {
    if mouse {
        session.click_visible_action("Filter focused:");
    }
    // Paste is a real editor event in both input journeys; activation remains
    // the chosen key or mouse path, never a direct command handler.
    session.send_key(b"\x01");
    session.paste(query);
    let selected = format!("> {label}");
    session.send_key(b"\t");
    session.wait_for_cursorless_condition(
        |frame| frame.contents().contains(&selected),
        "completed genuine indexed palette result before activation",
        Duration::from_secs(8),
    );
    if mouse {
        session.click_visible_action(&selected);
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_overlay_close(PALETTE_V2);
}
fn filter_v2(session: &mut Session, mouse: bool, query: &str, known: &str) {
    if mouse {
        session.click_visible_action("Filter:");
    } else {
        session.focus_visible_action("Filter:");
    }
    session.send_key(b"\x01");
    session.paste(query);
    session.wait_for_screen(known);
    if !mouse {
        session.send_key(b"\r");
    }
}
fn install_chosen(native: &mut Native, tree: &str, summary: &str) {
    let chosen = git_input(
        native,
        &["commit-tree", tree, "-p", &native.chosen, "-m", summary],
        b"",
    );
    let later_tree = native
        .fixture
        .git(&["rev-parse", "HEAD^{tree}"])
        .trim()
        .to_owned();
    let later = git_input(
        native,
        &[
            "commit-tree",
            &later_tree,
            "-p",
            &chosen,
            "-m",
            "Later checkout V2 stays separate",
        ],
        b"",
    );
    native
        .fixture
        .git(&["update-ref", "refs/heads/main", &later]);
    native.chosen = chosen;
    native.before = state(&native.fixture);
}
fn deep_fixture(native: &mut Native) -> String {
    let blob = git_input(
        native,
        &["hash-object", "-w", "--stdin"],
        &(1..=32)
            .map(|line| format!("deep original body row {line:02}\n"))
            .collect::<String>()
            .into_bytes(),
    );
    let mut tree = git_input(
        native,
        &["mktree", "-z"],
        format!("100644 blob {blob}\tactual.txt\0").as_bytes(),
    );
    for index in (0..24).rev() {
        tree = git_input(
            native,
            &["mktree", "-z"],
            format!("040000 tree {tree}\tdeep-folder-{index:02}\0").as_bytes(),
        );
    }
    install_chosen(native, &tree, "Chosen deep original viewport");
    (0..24)
        .map(|index| format!("deep-folder-{index:02}"))
        .collect::<Vec<_>>()
        .join("/")
        + "/actual.txt"
}
fn deep_body(mouse: bool) {
    let mut native = fixture(if mouse { "sha256" } else { "sha1" });
    let path = deep_fixture(&mut native);
    install_git_observer(&native, None);
    let mut session = launch(&native, mouse);
    saved_root(
        &native,
        &mut session,
        mouse,
        "Chosen deep original viewport",
    );
    jump(&mut session, mouse);
    filter_v2(&mut session, mouse, "actual.txt", "1 known matches");
    choose_focus(&mut session, mouse, "[Enter Open]");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_screen("deep original body row 01");
    // A long path may be cut to fit the chip; the bar still sits on the chosen version.
    path_history(&mut session, mouse, "path: ", &native.chosen);
    path_history_back(&mut session, mouse, "deep original body row 01");
    session.resize(80, 24);
    session.resize(40, 24);
    for line in 1..=4 {
        session.wait_for_screen(&format!("deep original body row {line:02}"));
    }
    session.wait_for_screen("[Folders ·");
    let before = current_count(&native);
    let (x, y) = session.locate("[Folders ·").unwrap();
    if mouse {
        session.click(x, y);
    } else {
        let label = session
            .screen()
            .lines()
            .find(|line| line.contains("[Folders ·"))
            .unwrap()
            .split("[Folders ·")
            .nth(1)
            .unwrap()
            .split(']')
            .next()
            .unwrap()
            .to_owned();
        session.focus_visible_action(&format!("[Folders ·{label}]"));
        session.send_key(b"\r");
    }
    session.wait_for_screen(PALETTE_V2);
    // Existing palette is inset2 cells and retains the rendered main footer.
    // It is NOT a full-screen modal; use the real main-frame resize fence and
    // then independently require its completed actual overlay border.
    session.resize(80, 24);
    session.wait_for_overlay_body(PALETTE_V2);
    palette_v2(
        &mut session,
        mouse,
        "repository.tree.folder-detail.24",
        "Inspect exact folder",
    );
    session.wait_for_screen(FOLDER_V2);
    session.resize_owned_modal(40, 24, FOLDER_V2, &["[− Detail]", "[Esc Close]"]);
    field_v2(&mut session, mouse, "Path");
    full_field_v2(
        &mut session,
        mouse,
        path.rsplit_once('/').unwrap().0,
        FOLDER_V2,
    );
    field_v2(&mut session, mouse, "Saved");
    full_field_v2(&mut session, mouse, &native.chosen, FOLDER_V2);
    choose_focus(&mut session, mouse, "[Esc Close]");
    session.wait_for_overlay_close(FOLDER_V2);
    session.wait_for_screen("deep original body row");
    session.resize(120, 40);
    assert_eq!(
        current_count(&native),
        before,
        "Cached crumbs/detail/width/focus are metadata, zero Git children"
    );
    commands_v2(&mut session, mouse, false);
    palette_v2(
        &mut session,
        mouse,
        "Open original folder Saved root",
        "Open original folder [Saved root]",
    );
    session.wait_for_screen("▸ deep-folder-00");
    assert_eq!(current_count(&native), before);
    effects_v2(&native, 0);
    session.quit_and_assert_restored_terminal(mouse);
}
fn raw_fixture(native: &mut Native) -> Vec<(Vec<u8>, String, String)> {
    let mut members = vec![];
    for (path, body) in [
        (b"same-\xfe".as_slice(), "RAW FE ORIGINAL, NOT FF\n"),
        (b"same-\xff".as_slice(), "RAW FF ORIGINAL, NOT FE\n"),
        (b"line\nname".as_slice(), "RAW LF ORIGINAL, NOT CASE\n"),
        (b"Case.rs".as_slice(), "UPPER CASE ORIGINAL\n"),
        (b"case.rs".as_slice(), "lower case original\n"),
    ] {
        members.push((
            path.to_vec(),
            git_input(native, &["hash-object", "-w", "--stdin"], body.as_bytes()),
            body.trim().into(),
        ));
    }
    for index in 0..121 {
        let path = format!("z-page-{index:03}").into_bytes();
        let body = format!("original pager body {index:03}");
        let oid = git_input(native, &["hash-object", "-w", "--stdin"], body.as_bytes());
        members.push((path, oid, body));
    }
    let mut raw = vec![];
    for (path, oid, _) in &members {
        raw.extend_from_slice(format!("100644 blob {oid}\t").as_bytes());
        raw.extend_from_slice(path);
        raw.push(0);
    }
    let tree = git_input(native, &["mktree", "-z"], &raw);
    install_chosen(native, &tree, "Chosen distinct raw viewport");
    members
}
fn raw_objects(mouse: bool) {
    let mut native = fixture("sha256");
    let members = raw_fixture(&mut native);
    assert_ne!(
        members[0].1, members[1].1,
        "Real FE/FF blobs must be distinct"
    );
    install_git_observer(&native, None);
    let mut session = launch(&native, mouse);
    saved_root(&native, &mut session, mouse, "Chosen distinct raw viewport");
    jump(&mut session, mouse);
    narrow_jump(&mut session, 40);
    choose_focus(&mut session, mouse, "[N ›]");
    session.wait_for_screen("showing 101–");
    commands_v2(&mut session, mouse, true);
    session.resize(80, 24);
    session.wait_for_overlay_body(PALETTE_V2);
    palette_v2(
        &mut session,
        mouse,
        "Previous100 original saved paths",
        "Previous100 original saved paths",
    );
    session.wait_for_screen(TITLE);
    session.wait_for_screen("showing 1–100");
    filter_v2(&mut session, mouse, "same", "2 known matches");
    narrow_jump(&mut session, 40);
    let old_position = session.locate("file · same-\\xff").unwrap();
    field_v2(&mut session, mouse, "Saved");
    full_field_v2(&mut session, mouse, &native.chosen, TITLE);
    field_v2(&mut session, mouse, "Path");
    session.wait_for_screen("same-\\xfe");
    let original_pids = raw_records_v2(&native)
        .into_iter()
        .map(|record| record.pid)
        .collect();
    let original_frame = session.completed_cursorless_end();
    // Selecting the actual FF member must change detail before Open, not
    // silently use FE because labels share prefix/invalid UTF8 replacement.
    if mouse {
        session.click(old_position.0, old_position.1);
    } else {
        filter_v2(&mut session, false, "same-\\xff", "1 known matches");
        choose_focus(&mut session, false, "[Open]");
    }
    session.wait_for_overlay_close(TITLE);
    session.wait_for_screen("RAW FF ORIGINAL, NOT FE");
    assert!(!session.screen().contains("RAW FE ORIGINAL, NOT FF"));
    new_raw_body_v2(&mut session, original_frame, "RAW FF ORIGINAL, NOT FE");
    raw_delta_v2(&native, &original_pids, &members[1]);
    let before = current_count(&native);
    session.resize(120, 40);
    commands_v2(&mut session, mouse, false);
    palette_v2(
        &mut session,
        mouse,
        "Jump to original saved path",
        "Jump to original saved path",
    );
    session.wait_for_screen(TITLE);
    session.wait_for_screen("Complete saved tree");
    filter_v2(&mut session, mouse, "same-\\xfe", "1 known matches");
    let original_pids = raw_records_v2(&native)
        .into_iter()
        .map(|record| record.pid)
        .collect();
    let original_frame = session.completed_cursorless_end();
    select_row(&mut session, mouse, "file · same-\\xfe");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_screen("RAW FE ORIGINAL, NOT FF");
    assert!(!session.screen().contains("RAW FF ORIGINAL, NOT FE"));
    new_raw_body_v2(&mut session, original_frame, "RAW FE ORIGINAL, NOT FF");
    raw_delta_v2(&native, &original_pids, &members[0]);
    assert!(
        current_count(&native) > before,
        "Fresh original file read is observed, not a cached screen"
    );
    assert_eq!(
        inventory_count(&native),
        2,
        "Reopening intentionally new picker reads one inventory per source intent"
    );
    // Repeat the SAME actual FE body/OID: neither its earlier screen nor its
    // earlier argv records may satisfy this independently captured selection.
    session.resize(120, 40);
    commands_v2(&mut session, mouse, false);
    palette_v2(
        &mut session,
        mouse,
        "Jump to original saved path",
        "Jump to original saved path",
    );
    session.wait_for_screen(TITLE);
    session.wait_for_screen("Complete saved tree");
    filter_v2(&mut session, mouse, "same-\\xfe", "1 known matches");
    let original_pids = raw_records_v2(&native)
        .into_iter()
        .map(|record| record.pid)
        .collect();
    let original_frame = session.completed_cursorless_end();
    select_row(&mut session, mouse, "file · same-\\xfe");
    session.wait_for_overlay_close(TITLE);
    new_raw_body_v2(&mut session, original_frame, "RAW FE ORIGINAL, NOT FF");
    raw_delta_v2(&native, &original_pids, &members[0]);
    assert_eq!(
        inventory_count(&native),
        3,
        "Repeat FE intentionally owns a NEW genuine picker/read, not old cached proof"
    );
    effects_v2(&native, 0);
    session.resize(120, 40);
    session.quit_and_assert_restored_terminal(mouse);
}
fn strict_journal(native: &Native) -> Vec<Value> {
    let bytes = match fs::read(native.fixture.root.join("gh-fixture/commands.jsonl")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return vec![],
        Err(error) => panic!("Cannot independently read strict fixture journal: {error}"),
    };
    assert!(
        bytes.is_empty() || bytes.ends_with(b"\n"),
        "Only a newline-complete strict journal is evidence"
    );
    std::str::from_utf8(&bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn hold_unrelated_version(native: &Native) {
    let mut version = GhStep::read(&["--version"], "");
    version.status = 1;
    version.stderr = b"explicit unrelated occupied-worker version failure\n".to_vec();
    native.fixture.scripted_gh(&[version]);
    let program = native.fixture.root.join("bin/gh");
    let actual = native.fixture.root.join("bin/gh-v2-strict");
    fs::rename(&program, &actual).unwrap();
    let root = shell_literal(native.fixture.root.to_str().unwrap());
    let actual = shell_literal(actual.to_str().unwrap());
    fs::write(&program, format!(
        "#!/bin/sh\nroot={root}\n{actual} \"$@\" > \"$root/v2-gh-output\" 2> \"$root/v2-gh-error\"; result=$?\n\
         printf '%s\\n' \"$$\" > \"$root/v2-gh-pid\"\n/bin/sleep 30 & descendant=$!\n\
         printf '%s\\n' \"$descendant\" > \"$root/v2-gh-descendant\"\n\
         while [ ! -f \"$root/v2-release-gh\" ]; do /bin/sleep 0.02; done\n\
         kill \"$descendant\" 2>/dev/null; wait \"$descendant\" 2>/dev/null\n\
         /bin/cat \"$root/v2-gh-output\"; /bin/cat \"$root/v2-gh-error\" >&2; exit \"$result\"\n"
    )).unwrap();
    fs::set_permissions(program, fs::Permissions::from_mode(0o755)).unwrap();
}
/// A retry may legitimately find the ordinary queue still full after a Git
/// child exits. Each input needs NEW owned feedback, never an old error or an
/// automatic resend. Pending work is allowed to finish without another input.
fn retry_with_owned_feedback_v2(session: &mut Session, mouse: bool, deadline: Instant) -> bool {
    fn rejected_epoch(body: &str) -> Option<u64> {
        body.lines().find_map(|line| {
            line.trim()
                .strip_prefix("Request #")?
                .strip_suffix(" rejected")?
                .parse()
                .ok()
        })
    }
    let point = session.wait_for_owned_completed_action(TITLE, "[Retry]", &[PALETTE_V2]);
    if !mouse {
        session.focus_visible_action("[Retry]");
    }
    let previous_epoch = rejected_epoch(
        &crate::support::overlay_body(session.completed_cells_screen().unwrap(), TITLE).unwrap(),
    )
    .expect("Actual currently displayed rejected request identity");
    let after = session.completed_cells_end();
    if mouse {
        session.click(point.0, point.1);
    } else {
        session.send_key(b"\r");
    }
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after {
            if let Some(body) = session
                .completed_cells_screen()
                .and_then(|frame| crate::support::overlay_body(frame, TITLE))
            {
                if body.contains("nested/renamed.txt") && body.contains("[Esc Close]") {
                    if body.contains("Complete saved tree") && body.contains("1 known matches") {
                        return true;
                    }
                    if body.contains("Original tree queue full")
                        && body.contains("Retry is explicit")
                        && body.contains("[Retry]")
                    {
                        if let Some(epoch) =
                            rejected_epoch(&body).filter(|epoch| *epoch > previous_epoch)
                        {
                            assert_eq!(
                                Some(epoch),
                                previous_epoch.checked_add(1),
                                "One explicit input creates exactly the next rejected request"
                            );
                            eprintln!(
                                "Explicit retry received NEW owned request#{epoch} queue-full feedback: {after} -> {}",
                                session.completed_cells_end()
                            );
                            return false;
                        }
                    }
                }
            }
        }
        assert!(
            Instant::now() < deadline,
            "Original combined8s explicit-retry budget; no blind input or timeout extension\n{}",
            session.screen()
        );
    }
}

fn full_queue(mouse: bool) {
    let native = fixture("sha256");
    install_git_observer(&native, Some("search"));
    hold_unrelated_version(&native);
    let mut session = launch(&native, mouse);
    saved_root(&native, &mut session, mouse, "Chosen immutable rename");
    choose(&mut session, mouse, "3 Search", b"3");
    session.wait_for_screen("Selected local search corpus ready");
    choose(&mut session, mouse, "v Saved version", b"v");
    session.wait_for_screen("Search scope changed");
    choose(&mut session, mouse, "[/ Query]", b"/");
    session.paste("future");
    choose(&mut session, mouse, "[Enter Search]", b"\r");
    let git = entered_pid(&native, &mut session, "search");
    assert!(!fs::read(native.fixture.root.join("v2-search-output"))
        .unwrap()
        .is_empty());
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_screen("Use Refresh to load your authenticated GitHub inbox.");
    choose(&mut session, mouse, "[r Refresh]", b"r");
    let gh = entered_pid(&native, &mut session, "gh");
    let journal = strict_journal(&native);
    assert_eq!(journal.len(), 1);
    assert_eq!(journal[0]["args"], json!(["--version"]));
    assert_eq!(journal[0]["matched"], true);
    assert!(
        alive_v2(git.0.pid) && alive_v2(gh.0.pid),
        "BOTH actual ordinary worker waits must be entered"
    );
    choose(&mut session, mouse, "2 Repository", b"2");
    session.wait_for_screen("Saved file tree");
    let before = current_count(&native);
    choose(&mut session, mouse, "[J Jump]", b"J");
    session.wait_for_screen(TITLE);
    session.wait_for_screen("Original source/filter pending");
    for query in [
        "queue-one",
        "queue-two",
        "queue-three",
        "nested/renamed.txt",
    ] {
        session.send_key(b"\x01");
        session.paste(query);
        session.wait_for_screen(query); // Actual processed query/frame barrier each request.
        assert!(alive_v2(git.0.pid) && alive_v2(gh.0.pid));
    }
    session.wait_for_screen("Original tree queue full");
    session.wait_for_screen("Retry is explicit");
    session.wait_for_screen("[Retry]");
    assert!(!session.screen().contains("file · nested/renamed.txt"));
    assert_eq!(inventory_count(&native), 0);
    assert_eq!(
        current_count(&native),
        before,
        "Queued/canceled original requests have no child"
    );
    narrow_jump(&mut session, 40);
    assert_eq!(
        current_count(&native),
        before,
        "Resize must not rebase/retry a failed original query"
    );
    // Controlled negative: retry while BOTH original workers are still held.
    // A second honest owned failure is valid, but must be newly acknowledged.
    assert!(!retry_with_owned_feedback_v2(
        &mut session,
        mouse,
        Instant::now() + Duration::from_secs(8)
    ));
    assert!(alive_v2(git.0.pid) && alive_v2(gh.0.pid));
    assert_eq!(inventory_count(&native), 0);
    assert_eq!(current_count(&native), before);
    session.capture_completed_terminal_v1("tree-queue-owned-busy-40x24");
    fs::write(
        native.fixture.root.join("v2-release-search"),
        b"release after actual queue failure\n",
    )
    .unwrap();
    fs::write(
        native.fixture.root.join("v2-release-gh"),
        b"release after actual queue failure\n",
    )
    .unwrap();
    reaped(&native, &mut session, "search", git);
    reaped(&native, &mut session, "gh", gh);
    let retry_deadline = Instant::now() + Duration::from_secs(8);
    while !retry_with_owned_feedback_v2(&mut session, mouse, retry_deadline) {
        // This is another explicit action ONLY after a fresh owned rejection,
        // not a rerun of a failed test or a pending/admitted operation.
        assert_eq!(inventory_count(&native), 0);
        assert!(Instant::now() < retry_deadline);
    }
    assert_eq!(
        inventory_count(&native),
        1,
        "ONLY explicit NEW operation reads actual inventory"
    );
    narrow_jump(&mut session, 80);
    session.capture_completed_terminal_v1("tree-queue-explicit-ready-80x24");
    select_row(&mut session, mouse, "file · nested/renamed.txt");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_screen("saved nested line 002");
    session.resize(120, 40);
    effects_v2(&native, 1);
    session.quit_and_assert_restored_terminal(mouse);
}
fn cancel_descendant(mouse: bool) {
    let native = fixture(if mouse { "sha256" } else { "sha1" });
    install_git_observer(&native, Some("inventory"));
    let mut session = launch(&native, mouse);
    saved_root(&native, &mut session, mouse, "Chosen immutable rename");
    choose(&mut session, mouse, "[J Jump]", b"J");
    session.wait_for_screen(TITLE);
    let pair = entered_pid(&native, &mut session, "inventory");
    assert!(
        fs::read(native.fixture.root.join("v2-inventory-output"))
            .unwrap()
            .windows(b"nested/renamed.txt".len())
            .any(|window| window == b"nested/renamed.txt"),
        "Actual installed Git emitted genuine held members"
    );
    let before = current_count(&native);
    let before_args = argv_records(&native);
    narrow_jump(&mut session, 40);
    if mouse {
        // The header tabs stay live shell controls (TASK-93); a click on the picker's own
        // frame stays the picker's and never reaches the content under it.
        let border = session.locate("Jump to saved path").unwrap();
        session.click(border.0, border.1);
    } else {
        session.focus_visible_action("[Open]");
        session.send_key(b"3");
    }
    session.wait_for_screen(TITLE);
    assert!(!session.screen().contains("Search • focused view"));
    choose_focus(&mut session, mouse, "[Esc Close]");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_screen("Original saved-path picker closed");
    reaped(&native, &mut session, "inventory", pair);
    session.resize(80, 24);
    session.wait_for_screen("Saved file tree");
    session.resize(120, 40);
    assert!(!session.screen().contains("Complete saved tree"));
    assert!(!session.screen().contains("file · nested/renamed.txt"));
    let mut remaining = argv_records(&native);
    for args in before_args {
        if let Some(index) = remaining.iter().position(|actual| actual == &args) {
            remaining.remove(index);
        } else {
            panic!("Actual prior argv record disappeared");
        }
    }
    assert_eq!(
        current_count(&native),
        before,
        "Releasing original child cannot publish/read after Close; actual extra argv {remaining:?}"
    );
    assert_eq!(inventory_count(&native), 1);
    effects_v2(&native, 0);

    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains("Saved file tree") && !frame.contents().contains("Esc Cancel")
        },
        "Close retires original loading without leaving a disabled tree",
        Duration::from_secs(8),
    );
    let original_object = native
        .fixture
        .git(&[
            "rev-parse",
            &format!("{}:nested/renamed.txt", native.chosen),
        ])
        .trim()
        .to_owned();
    let before_new_read = argv_records(&native);
    jump(&mut session, mouse);
    filter_v2(&mut session, mouse, "nested/renamed.txt", "1 known matches");
    select_row(&mut session, mouse, "file · nested/renamed.txt");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_screen("saved nested line 002");
    let mut new_args = argv_records(&native);
    for args in before_new_read {
        let index = new_args.iter().position(|actual| *actual == args).unwrap();
        new_args.remove(index);
    }
    assert!(new_args
        .iter()
        .any(|args| args.iter().any(|arg| arg == b"ls-tree")
            && args.iter().any(|arg| arg == b"-z")
            && args.iter().any(|arg| arg == native.chosen.as_bytes())
            && args.last().is_some_and(|arg| arg == b"nested/renamed.txt")));
    assert!(new_args
        .iter()
        .any(|args| args.iter().any(|arg| arg == b"cat-file")
            && args
                .last()
                .is_some_and(|arg| arg == original_object.as_bytes())));
    assert_eq!(
        inventory_count(&native),
        2,
        "Only the NEW explicit Jump reads again"
    );
    effects_v2(&native, 0);
    session.quit_and_assert_restored_terminal(mouse);
}
