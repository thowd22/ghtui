//! UNREGISTERED child of original_saved_navigation_v1; genuine subprocess
//! keys/mouse and original Git/SQL/config/command/TTY effects, not handlers.
use super::*;
use std::io::Write;
use std::process::Stdio;
const TITLE: &str = "Jump to saved path • original source";
#[path = "original_tree_viewport_v2.rs"]
mod viewport_v2;

fn saved_root(native: &Native, session: &mut Session, mouse: bool, summary: &str) {
    session.wait_for_screen("Recent commits");
    choose(session, mouse, "[g Graph]", b"g");
    session.wait_for_screen("loaded (limit 10000)");
    session.open_saved_graph_subject(mouse, summary, &native.chosen);
    session.wait_for_screen(&native.chosen);
    choose(session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    session.wait_for_screen(&native.chosen);
}
fn jump(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[J Jump]", b"J");
    session.wait_for_screen(TITLE);
    session.wait_for_screen("Complete saved tree");
}
fn filter(session: &mut Session, mouse: bool, query: &str, known: &str) {
    if mouse {
        session.click_visible_action("Filter:");
    } else {
        session.send_key(b"/");
    }
    session.send_key(b"\x01");
    session.paste(query);
    session.wait_for_screen(known);
    if !mouse {
        session.send_key(b"\r");
    }
}
fn modal_resize(session: &mut Session, cols: u16, rows: u16) {
    session.resize_owned_modal(cols, rows, TITLE, &["[Enter Open]", "[Esc Close]"]);
}
fn crumb(session: &mut Session, mouse: bool, label: &str) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.focus_visible_action(label);
        session.send_key(b"\r");
    }
}
fn inventory_children(native: &Native) -> usize {
    starts(native)
        .iter()
        .filter(|event| {
            let args = event["argv"].as_array().unwrap();
            ["ls-tree", "-r", "-t", "-z", "--full-tree"]
                .iter()
                .all(|required| args.iter().any(|arg| arg.as_str() == Some(required)))
        })
        .count()
}
fn pure_delta(native: &Native, before: usize, session: &mut Session) {
    // A real owned redraw/input barrier, not a delay or direct App read.
    modal_resize(session, 120, 40);
    session.wait_for_screen(TITLE);
    assert_eq!(starts(native).len(), before);
}
#[test]
fn original_tree_jump_keyboard_nested_old_sha_exact_breadcrumbs_history_and_zero_read_collapse() {
    nested(false);
}
#[test]
fn original_tree_jump_mouse_nested_old_sha_exact_breadcrumbs_history_and_zero_read_collapse() {
    nested(true);
}
fn nested(mouse: bool) {
    let native = fixture(if mouse { "sha256" } else { "sha1" });
    let mut session = launch(&native, mouse);
    saved_root(&native, &mut session, mouse, "Chosen immutable rename");
    let old_view = session
        .locate("3 Search")
        .expect("Actual underlying Search control must exist before overlay");
    let before = starts(&native).len();
    jump(&mut session, mouse);
    assert_eq!(inventory_children(&native), 1);
    assert_eq!(starts(&native).len(), before + 1);
    if mouse {
        session.click(old_view.0, old_view.1);
    } else {
        session.send_key(b"3");
    }
    modal_resize(&mut session, 80, 24);
    session.wait_for_screen(TITLE);
    session.wait_for_screen(&native.chosen);
    assert!(!session.screen().contains("Search • focused view"));
    let before_rejection = starts(&native).len();
    session.paste(&"x".repeat(257));
    session.wait_for_screen("limited to256 raw bytes");
    session.wait_for_screen("rejected input");
    assert_eq!(starts(&native).len(), before_rejection);
    assert_eq!(inventory_children(&native), 1);
    let before = starts(&native).len();
    filter(&mut session, mouse, "nested/renamed.txt", "1 known matches");
    pure_delta(&native, before, &mut session);
    modal_resize(&mut session, 80, 24);
    session.wait_for_screen(&native.chosen);
    session.wait_for_screen("Exact saved path: nested/renamed.txt");
    select_row(&mut session, mouse, "file · nested/renamed.txt");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_screen("saved nested line 002");
    session.wait_for_screen(&native.chosen);
    session.wait_for_screen("[Folder: nested]");
    assert!(!session.screen().contains("future excluded source"));
    no_discovery_delta(&native, before);
    choose(&mut session, mouse, "[H History]", b"H");
    session.wait_for_screen("File history — chosen saved version");
    session.wait_for_screen("Rename: history.txt → nested/renamed.txt");
    session.choose_compact(mouse, "[u File back]", "[u Back]");
    session.wait_for_screen("saved nested line 002");
    // Loaded directory/root navigation and collapse use actual original raw
    // member tickets; no filesystem/revision/object read or hidden retry.
    let before = starts(&native).len();
    crumb(&mut session, mouse, "[Folder: nested]");
    session.wait_for_screen("Saved file tree • exact original source");
    session.wait_for_screen("nested/renamed.txt");
    select_row(&mut session, mouse, "▾ nested");
    session
        .try_wait_for_condition(
            |screen| !screen.contains("nested/renamed.txt"),
            "collapsed original directory",
            Duration::from_secs(8),
        )
        .unwrap();
    crumb(&mut session, mouse, "[Saved root]");
    session.wait_for_screen("▸ nested");
    session.resize(120, 40);
    assert_eq!(starts(&native).len(), before);
    // Cross-view history may retain tiny opaque original route metadata, not
    // cached file bodies/old pools or a newly guessed HEAD/source mapping.
    fresh_crumb_restore(
        &native,
        &mut session,
        mouse,
        "[Repository • Tree]",
        "▸ nested",
    );
    session.wait_for_screen("[Saved root]");
    session.wait_for_screen(&native.chosen);
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}

fn git_input(native: &Native, args: &[&str], input: &[u8]) -> String {
    let mut command = Command::new("git");
    command
        .current_dir(native.fixture.repo())
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Real fixture Git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
/// Actual immutable tree objects without creating50k working files. The later
/// checkout's exact current tree/index/dirty bytes stay intact in setup.
fn extra_tree(native: &mut Native, paths: &[Vec<u8>], summary: &str) {
    let blob = git_input(
        native,
        &["hash-object", "-w", "--stdin"],
        b"exact bounded saved path body\n",
    );
    let mut records = vec![];
    for path in paths {
        records.extend_from_slice(format!("100644 blob {blob}\t").as_bytes());
        records.extend_from_slice(path);
        records.push(0);
    }
    extra_tree_objects(native, &records, summary);
}
fn extra_tree_objects(native: &mut Native, records: &[u8], summary: &str) {
    let tree = git_input(native, &["mktree", "-z"], records);
    let chosen = git_input(
        native,
        &["commit-tree", &tree, "-p", &native.chosen, "-m", summary],
        b"",
    );
    let later_tree = native
        .fixture
        .git(&["rev-parse", "HEAD^{tree}"])
        .trim()
        .to_owned();
    let head = git_input(
        native,
        &[
            "commit-tree",
            &later_tree,
            "-p",
            &chosen,
            "-m",
            "Later checkout after bounded saved tree",
        ],
        b"",
    );
    native
        .fixture
        .git(&["update-ref", "refs/heads/main", &head]);
    native.chosen = chosen;
    native.before = state(&native.fixture);
}
#[test]
fn original_tree_jump_keyboard_large_actual_inventory_partial_paging_and_unknown_absence() {
    large(false);
}
#[test]
fn original_tree_jump_mouse_large_actual_inventory_partial_paging_and_unknown_absence() {
    large(true);
}
fn large(mouse: bool) {
    let mut native = fixture("sha1");
    let paths = (0..50_001)
        .map(|index| format!("file-{index:05}").into_bytes())
        .collect::<Vec<_>>();
    extra_tree(&mut native, &paths, "Chosen bounded immutable inventory");
    let mut session = launch(&native, mouse);
    saved_root(
        &native,
        &mut session,
        mouse,
        "Chosen bounded immutable inventory",
    );
    choose(&mut session, mouse, "[J Jump]", b"J");
    session.wait_for_screen(TITLE);
    session.wait_for_screen("50000 original rows");
    session.wait_for_screen("Partial saved tree; unseen paths unknown");
    assert_eq!(inventory_children(&native), 1);
    let before = starts(&native).len();
    if mouse {
        session.click_visible_action("[N Next100]");
    } else {
        session.focus_visible_action("[N Next100]");
        session.send_key(b"\r");
    }
    session.wait_for_screen("showing 101–200");
    if mouse {
        session.click_visible_action("[P Previous100]");
    } else {
        session.focus_visible_action("[P Previous100]");
        session.send_key(b"\r");
    }
    session.wait_for_screen("showing 1–100");
    filter(&mut session, mouse, "file-50000", "0 known matches");
    session.wait_for_screen("No known matching path");
    session.wait_for_screen("unseen paths unknown");
    pure_delta(&native, before, &mut session);
    filter(&mut session, mouse, "file-49999", "1 known matches");
    modal_resize(&mut session, 80, 24);
    session.wait_for_screen(&native.chosen);
    select_row(&mut session, mouse, "file · file-49999");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_screen("exact bounded saved path body");
    session.wait_for_screen(&native.chosen);
    no_discovery_delta(&native, before);
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn original_tree_jump_keyboard_raw_byte_names_keep_genuine_distinct_path_and_full_detail() {
    raw(false);
}
#[test]
fn original_tree_jump_mouse_raw_byte_names_keep_genuine_distinct_path_and_full_detail() {
    raw(true);
}
fn raw(mouse: bool) {
    let mut native = fixture("sha256");
    let paths: [&[u8]; 5] = [
        b"same-\xfe",
        b"same-\xff",
        b"line\nname",
        b"Case.rs",
        b"case.rs",
    ];
    let mut objects = Vec::new();
    let mut records = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let body = format!("independently distinct byte member {index}\n");
        let oid = git_input(&native, &["hash-object", "-w", "--stdin"], body.as_bytes());
        records.extend_from_slice(format!("100644 blob {oid}\t").as_bytes());
        records.extend_from_slice(path);
        records.push(0);
        objects.push((oid, body));
    }
    assert_eq!(
        objects
            .iter()
            .map(|(oid, _)| oid)
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        paths.len()
    );
    extra_tree_objects(
        &mut native,
        &records,
        "Chosen byte exact immutable inventory",
    );
    crate::raw_git_argv_v1::install(&native.fixture.root);
    let mut session = launch(&native, mouse);
    saved_root(
        &native,
        &mut session,
        mouse,
        "Chosen byte exact immutable inventory",
    );
    for (visit, index) in [1, 0, 3, 4, 2, 0].into_iter().enumerate() {
        jump(&mut session, mouse);
        let query = if index <= 1 {
            "same"
        } else if index == 2 {
            "line"
        } else {
            "case.rs"
        };
        filter(
            &mut session,
            mouse,
            query,
            if index == 2 {
                "1 known matches"
            } else {
                "2 known matches"
            },
        );
        modal_resize(&mut session, 80, 24);
        if index <= 1 {
            session.wait_for_screen("same-\\xfe");
            session.wait_for_screen("same-\\xff");
        }
        let displayed = [
            "same-\\xfe",
            "same-\\xff",
            "line\\nname",
            "Case.rs",
            "case.rs",
        ][index];
        let before = starts(&native).len();
        let old = crate::raw_git_argv_v1::records(&native.fixture.root.join("raw-git-argv-v1"));
        let old_pids = old
            .iter()
            .map(|record| record.pid)
            .collect::<std::collections::BTreeSet<_>>();
        select_row(&mut session, mouse, &format!("file · {displayed}"));
        session.wait_for_overlay_close(TITLE);
        session.wait_for_screen(objects[index].1.trim());
        session.wait_for_screen(displayed);
        session.wait_for_screen(&native.chosen);
        let current = crate::raw_git_argv_v1::records(&native.fixture.root.join("raw-git-argv-v1"));
        let delta = current
            .into_iter()
            .filter(|record| !old_pids.contains(&record.pid))
            .collect::<Vec<_>>();
        assert!(
            !delta.is_empty(),
            "Each actual selection needs fresh exact Git reads"
        );
        crate::raw_git_argv_v1::exact_read(&delta, &native.chosen, paths[index], &objects[index].0);
        assert_eq!(
            crate::raw_git_argv_v1::original_blob(
                native.fixture.repo().as_path(),
                &native.chosen,
                paths[index]
            ),
            (
                objects[index].0.clone(),
                objects[index].1.as_bytes().to_vec()
            )
        );
        no_discovery_delta(&native, before);
        assert_eq!(inventory_children(&native), visit + 1);
        if index <= 1 {
            assert!(!session.screen().contains(if index == 1 {
                "same-\\xfe"
            } else {
                "same-\\xff"
            }));
        }
    }
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn original_tree_jump_keyboard_pending_real_inventory_cancel_reaps_no_late_members_or_reads() {
    cancel_inventory(false);
}
#[test]
fn original_tree_jump_mouse_pending_real_inventory_cancel_reaps_no_late_members_or_reads() {
    cancel_inventory(true);
}
fn cancel_inventory(mouse: bool) {
    let native = fixture("sha1");
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
    let program = native.fixture.root.join("bin/git");
    fs::write(&program,format!("#!/bin/sh\ncase \" $* \" in *' ls-tree -r -t -z --full-tree '*) '{installed}' \"$@\" > '{root}/held-tree-output' || exit $?; printf '%s\\n' \"$$\" > '{root}/held-tree-pid'; while [ ! -f '{root}/release-tree' ]; do /bin/sleep 0.02; done; /bin/cat '{root}/held-tree-output';; *) exec '{installed}' \"$@\";; esac\n")).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    let mut session = launch(&native, mouse);
    saved_root(&native, &mut session, mouse, "Chosen immutable rename");
    let before = starts(&native).len();
    choose(&mut session, mouse, "[J Jump]", b"J");
    session.wait_for_screen(TITLE);
    let deadline = Instant::now() + Duration::from_secs(8);
    let pid = loop {
        if let Ok(text) = fs::read_to_string(native.fixture.root.join("held-tree-pid")) {
            if text.ends_with('\n') {
                if let Ok(pid) = text.trim().parse::<i32>() {
                    if pid > 0 {
                        break pid;
                    }
                }
            }
        }
        session.poll_output(Duration::from_millis(5));
        assert!(
            Instant::now() < deadline,
            "Actual inventory wrapper never entered"
        );
    };
    assert_eq!(unsafe { libc::kill(pid, 0) }, 0);
    assert_eq!(starts(&native).len(), before + 1);
    assert!(fs::read(native.fixture.root.join("held-tree-output"))
        .unwrap()
        .windows(b"nested/renamed.txt".len())
        .any(|window| window == b"nested/renamed.txt"));
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(TITLE);
    let deadline = Instant::now() + Duration::from_secs(8);
    while unsafe { libc::kill(pid, 0) } == 0 {
        session.poll_output(Duration::from_millis(5));
        assert!(
            Instant::now() < deadline,
            "Cancelled original inventory PID survived cleanup"
        );
    }
    let completed = starts(&native).len();
    session.resize(80, 24);
    session.wait_for_screen("Saved file tree");
    session.resize(120, 40);
    session.wait_for_control("[J Jump]");
    assert_eq!(starts(&native).len(), completed);
    assert!(!session.screen().contains("Complete saved tree"));
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
