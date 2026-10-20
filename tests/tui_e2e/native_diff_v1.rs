//! Actual saved Native pane. Parent owns the genuine Git/SQL/fake-gh fixtures.
use super::*;
#[path = "native_external_diff_v4.rs"]
mod native_external_diff_v4;
const NATIVE_TITLE: &str = "Native saved diff • original source";
fn ready(session: &mut Session, text: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            screen.contains(NATIVE_TITLE)
                && screen.contains("Native saved diff ready")
                && screen.contains(text)
        },
        "completed exact original native diff frame",
        Duration::from_secs(8),
    );
}
/// A completed frame drawn after `after` with every part and no projection in flight.
fn settled(session: &mut Session, after: usize, parts: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cursorless_end() > after
            && session.completed_cursorless_screen().is_some_and(|frame| {
                let screen = frame.contents();
                screen.contains(NATIVE_TITLE)
                    && !screen.contains("Preparing native diff")
                    && parts.iter().all(|part| screen.contains(part))
            })
        {
            return;
        }
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "No new settled native diff frame with {parts:?}\n{}; artifacts: {}",
                session.screen(),
                session.artifact_dir.display()
            );
        }
    }
}
fn open(native: &Native, session: &mut Session, mouse: bool) {
    open_comparison(native, session, mouse, true);
    let before = starts(native).len();
    session.choose_compact(mouse, "[v Native diff]", "[v Diff]");
    let names = git_bytes(
        &native.fixture,
        &["diff", "--name-only", "-z", &native.c, &native.b, "--"],
        None,
    );
    assert_eq!(
        names.split(|byte| *byte == 0).next(),
        Some(&b"Case"[..]),
        "independent installed-Git first canonical path"
    );
    ready(session, "uppercase actual target");
    assert_eq!(
        starts(native).len() - before,
        9,
        "same genuine original source producer, no second parser"
    );
}
fn shared(native: &Native, session: &mut Session, mouse: bool) {
    let names = git_bytes(
        &native.fixture,
        &["diff", "--name-only", "-z", &native.c, &native.b, "--"],
        None,
    );
    let index = names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .position(|name| name == b"shared")
        .unwrap();
    let count = names
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .count();
    for current in 0..index {
        let before = session.completed_cursorless_end();
        choose(session, mouse, "[] File]", b"]");
        let expected = format!("File {}/{}:", current + 2, count);
        session.wait_for_cursorless_condition(
            |frame| {
                let screen = frame.contents();
                screen.contains("Native saved diff ready") && screen.contains(&expected)
            },
            "file projection is admitted",
            Duration::from_secs(8),
        );
        assert!(
            session.completed_cursorless_end() > before,
            "strict new canonical file frame"
        );
    }
    ready(session, "COMMON old bytes");
    session.wait_for_screen("TARGET new bytes");
}
#[test]
fn native_diff_keyboard_original_actual_c_split_wrap_query_range_generated_and_readonly() {
    journey(false);
}
#[test]
fn native_diff_mouse_original_actual_c_split_wrap_query_range_generated_and_readonly() {
    journey(true);
}
fn journey(mouse: bool) {
    let native = fixture("sha256");
    let mut session = launch(&native, mouse);
    open(&native, &mut session, mouse);
    shared(&native, &mut session, mouse);
    session.capture_completed_terminal_v1("git-native-diff");
    let before = starts(&native).len();
    session.resize_owned_modal(600, 24, NATIVE_TITLE, &["[Esc Close]", "[o Options]"]);
    ready(&mut session, "bounded 512-column pane");
    if mouse {
        let (x, y) = session.locate("COMMON old bytes").unwrap();
        session.click(x, y);
    } else {
        let mut before = session.completed_cursorless_end();
        session.send_key(b"\x1b[H");
        for attempt in 0..16 {
            session.wait_for_cursorless_condition(
                |frame| frame.contents().contains("Native saved diff ready"),
                "owned native focus frame",
                Duration::from_secs(8),
            );
            if session
                .completed_cursorless_screen()
                .unwrap()
                .contents()
                .lines()
                .any(|line| line.starts_with("│>") && line.contains("COMMON old bytes"))
            {
                break;
            }
            assert!(
                attempt < 15,
                "canonical old changed row must be selected, not a header"
            );
            before = session.completed_cursorless_end();
            session.send_key(b"\x1b[B");
            let deadline = Instant::now() + Duration::from_secs(8);
            while session.completed_cursorless_end() <= before {
                session.poll_output(Duration::from_millis(1));
                assert!(Instant::now() < deadline, "new complete actual Down frame");
            }
        }
        assert!(session.completed_cursorless_end() > before);
    }
    choose(&mut session, mouse, "[h Hunk]", b"h");
    ready(&mut session, "Selected 2 (hidden 0)");
    choose(&mut session, mouse, "[o Options]", b"o");
    choose(&mut session, mouse, "[l Layout]", b"l");
    choose(&mut session, mouse, "[O Old side]", b"O");
    choose(&mut session, mouse, "[o Back to diff]", b"o");
    ready(&mut session, "Selected 2 (hidden 0)");
    assert!(session.screen().contains("COMMON old bytes"));
    assert!(session.screen().contains("TARGET new bytes"));
    session.resize_owned_modal(40, 27, NATIVE_TITLE, &["[Esc Close]", "[o Options]"]);
    ready(&mut session, "Selected 2 (hidden 1)");
    assert!(session.screen().contains("COMMON old bytes"));
    assert!(!session.screen().contains("TARGET new bytes"));
    choose(&mut session, mouse, "[o Options]", b"o");
    choose(&mut session, mouse, "[N New side]", b"N");
    choose(&mut session, mouse, "[o Back to diff]", b"o");
    ready(&mut session, "TARGET new bytes");
    choose(&mut session, mouse, "[o Options]", b"o");
    choose(&mut session, mouse, "[w Wrap]", b"w");
    choose(&mut session, mouse, "[g Mark generated]", b"g");
    choose(&mut session, mouse, "[G Hide generated]", b"G");
    choose(&mut session, mouse, "[o Back to diff]", b"o");
    ready(&mut session, "Selected 2 (hidden 2)");
    assert!(!session.screen().contains("TARGET new bytes"));
    choose(&mut session, mouse, "[o Options]", b"o");
    // Each step waits for the app's own new frame: the resize below must not overtake a
    // queued click, or the click lands on the 120x40 layout instead of this one.
    let unhide = session.completed_cursorless_end();
    choose(&mut session, mouse, "[G Hide generated]", b"G");
    // The projection must land before Stage: a result published later replaces its status.
    session.wait_for_new_cursorless_condition(
        unhide,
        |frame| frame.contents().contains("Preparing native diff"),
        "unhide generated projection in flight",
        Duration::from_secs(8),
    );
    let preparing = session.completed_cursorless_end();
    settled(
        &mut session,
        preparing,
        &["Native saved diff ready", "[a Stage disabled]"],
    );
    let stage = session.completed_cursorless_end();
    choose(&mut session, mouse, "[a Stage disabled]", b"a");
    settled(
        &mut session,
        stage,
        &["Stage disabled:", "[o Back to diff]"],
    );
    let back = session.completed_cursorless_end();
    choose(&mut session, mouse, "[o Back to diff]", b"o");
    settled(&mut session, back, &["Stage disabled:", "[o Options]"]);
    session.resize_owned_modal(120, 40, NATIVE_TITLE, &["[Esc Close]", "[o Options]"]);
    ready(&mut session, "TARGET new bytes");
    if mouse {
        session.click_visible_action("Exact:");
    } else {
        session.send_key(b"/");
    }
    session.paste("TARGET");
    session.send_key(b"\r");
    ready(&mut session, "1 matches");
    assert_eq!(
        starts(&native).len(),
        before,
        "all cached projection/query/options/range/resize controls use zero Git children"
    );
    effects(&native, &session);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native saved diff closed");
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}

fn small_fixture() -> Native {
    let mut native = fixture("sha256");
    let old =
        b"fn main() {\n    let old_name = 1;\n    println!(\"wide \xe7\x95\x8ce\xcc\x81\");\n}\n";
    let new =
        b"fn main() {\n    let new_name = 2;\n    println!(\"wide \xe7\x95\x8ce\xcc\x81\");\n}\n";
    native.c = commit(
        &native.fixture,
        &tree(&native.fixture, &[(b"code.rs", old, "100644")]),
        None,
        "Native-common",
    );
    native.a = commit(
        &native.fixture,
        &tree(
            &native.fixture,
            &[(b"code.rs", b"REQUESTED A IS NOT OLD\n", "100644")],
        ),
        Some(&native.c),
        "Native-requested",
    );
    native.b = commit(
        &native.fixture,
        &tree(&native.fixture, &[(b"code.rs", new, "100644")]),
        Some(&native.c),
        "Native-target",
    );
    native
        .fixture
        .git(&["update-ref", "refs/heads/requested-A", &native.a]);
    native
        .fixture
        .git(&["update-ref", "refs/heads/target-B", &native.b]);
    native.before = state(&native.fixture);
    native
}
fn selected(session: &Session, text: &str) -> bool {
    session.completed_cursorless_screen().is_some_and(|frame| {
        frame
            .contents()
            .lines()
            .any(|line| line.starts_with("│>") && line.contains(text))
    })
}
fn focus(session: &mut Session, mouse: bool, text: &str) {
    if mouse {
        let before = session.completed_cursorless_end();
        let (x, y) = session
            .locate(text)
            .expect("genuine currently drawn changed text");
        session.click(x, y);
        let deadline = Instant::now() + Duration::from_secs(8);
        while session.completed_cursorless_end() <= before || !selected(session, text) {
            session.poll_output(Duration::from_millis(1));
            assert!(
                Instant::now() < deadline,
                "new exact clicked original native row"
            );
        }
        return;
    }
    let before = session.completed_cursorless_end();
    session.send_key(b"\x1b[H");
    let deadline = Instant::now() + Duration::from_secs(8);
    while session.completed_cursorless_end() <= before || !selected(session, "code.rs → code.rs")
    {
        session.poll_output(Duration::from_millis(1));
        assert!(
            Instant::now() < deadline,
            "new exact Home-selected original file header"
        );
    }
    for attempt in 0..16 {
        if selected(session, text) {
            return;
        }
        assert!(attempt < 15, "original changed line must be selected");
        let screen = session.completed_cursorless_screen().unwrap().contents();
        let lines: Vec<_> = screen.lines().collect();
        let row = lines
            .iter()
            .position(|line| line.starts_with("│>"))
            .unwrap();
        let expected: String = lines
            .get(row + 1)
            .expect("actual following visible original row")
            .chars()
            .skip(2)
            .collect();
        let before = session.completed_cursorless_end();
        session.send_key(b"\x1b[B");
        let deadline = Instant::now() + Duration::from_secs(8);
        while session.completed_cursorless_end() <= before
            || !session.completed_cursorless_screen().is_some_and(|frame| {
                frame.contents().lines().any(|line| {
                    line.starts_with("│>") && line.chars().skip(2).collect::<String>() == expected
                })
            })
        {
            session.poll_output(Duration::from_millis(1));
            assert!(Instant::now() < deadline, "new completed native Down frame");
        }
    }
}
fn source_all(session: &mut Session, mouse: bool, shas: [&str; 3]) {
    const TITLE: &str = "Repository source identity • read-only";
    let mut found = [false; 3];
    for page in 0..12 {
        session.wait_for_cursorless_condition(
            |frame| crate::support::overlay_body(frame, TITLE).is_some(),
            "complete owned source page",
            Duration::from_secs(8),
        );
        let body = session
            .completed_cursorless_screen()
            .and_then(|frame| crate::support::overlay_body(frame, TITLE))
            .expect("completed bounded source dialog");
        for (i, sha) in shas.iter().enumerate() {
            found[i] |= body.contains(sha);
        }
        if found.iter().all(|found| *found) {
            return;
        }
        assert!(
            page < 11,
            "all three independent full SHAs must appear inside the owned source dialog"
        );
        let before = session.completed_cursorless_end();
        choose(session, mouse, "[PgDn More]", b"\x1b[6~");
        let deadline = Instant::now() + Duration::from_secs(8);
        while session.completed_cursorless_end() <= before
            || session
                .completed_cursorless_screen()
                .and_then(|frame| crate::support::overlay_body(frame, TITLE))
                .as_ref()
                .is_none_or(|current| current == &body)
        {
            session.poll_output(Duration::from_millis(1));
            assert!(
                Instant::now() < deadline,
                "genuine source-detail page must advance, not match background SHAs"
            );
        }
    }
}
#[test]
fn native_diff_keyboard_original_range_source_identity_invalid_regex_paste_and_recovery() {
    scope(false);
}
#[test]
fn native_diff_mouse_original_drag_source_identity_invalid_regex_paste_and_recovery() {
    scope(true);
}
fn scope(mouse: bool) {
    let native = small_fixture();
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, true);
    choose(&mut session, mouse, "[: Commands]", b":");
    session.wait_for_screen("Commands");
    session.paste("native saved diff");
    session.wait_for_screen("Explore native saved diff");
    if mouse {
        session.click_visible_action("Explore native saved diff");
    } else {
        session.send_key(b"\r");
    }
    ready(&mut session, "let old_name = 1;");
    session.wait_for_screen("let new_name = 2;");
    session.wait_for_screen("Native-target Writer");
    session.wait_for_screen("Native-target Recorder");
    assert!(!session.screen().contains("REQUESTED A IS NOT OLD"));
    let before = starts(&native).len();
    if mouse {
        let first = session.locate("let old_name = 1;").unwrap();
        let last = session.locate("let new_name = 2;").unwrap();
        session.drag(first, last);
    } else {
        focus(&mut session, false, "let old_name = 1;");
        session.send_key(b" ");
        ready(&mut session, "Selected 1 (hidden 0)");
        assert!(selected(&session,"let old_name = 1;"),"Space publication must preserve exact Old selection BEFORE extension; never seek to compensate");
        session.send_key(b"\x1b[1;2B");
    }
    ready(&mut session, "Selected 2 (hidden 0)");
    choose(&mut session, mouse, "[I Source]", b"I");
    session.wait_for_screen("Repository source identity");
    source_all(&mut session, mouse, [&native.a, &native.c, &native.b]);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    ready(&mut session, "Selected 2 (hidden 0)");
    choose(&mut session, mouse, "[o Options]", b"o");
    choose(&mut session, mouse, "[r Exact / regex]", b"r");
    choose(&mut session, mouse, "[o Back to diff]", b"o");
    if mouse {
        session.click_visible_action("Regex:");
    } else {
        session.send_key(b"/");
    }
    session.paste("[");
    session.send_key(b"\r");
    session.wait_for_cursorless_condition(
        |frame| frame.contents().contains("Error:") && frame.contents().contains("Regex:"),
        "actual bounded regex error has no selectable old frame",
        Duration::from_secs(8),
    );
    choose(&mut session, mouse, "[h Hunk]", b"h");
    session.wait_for_screen("Choose a genuine changed-line row");
    if mouse {
        session.click_visible_action("Regex:");
    } else {
        session.send_key(b"/");
    }
    session.send_key(b"\x15");
    session.paste("new_name");
    session.send_key(b"\r");
    ready(&mut session, "1 matches");
    assert!(session.screen().contains("Selected 2 (hidden 0)"));
    if mouse {
        session.click_visible_action("Regex:");
    } else {
        session.send_key(b"/");
    }
    session.send_key(b"\x01");
    session.paste(&"x".repeat(257));
    session.send_key(b"\r");
    session.wait_for_screen("paste refused");
    assert!(session.screen().contains("Regex: new_name"));
    assert_eq!(
        starts(&native).len(),
        before,
        "range/drag/identity/invalid regex/oversized paste/recovery do not spawn Git"
    );
    effects(&native, &session);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native saved diff closed");
    session.quit_and_assert_restored_terminal(mouse);
}

fn metadata_fixture(empty: bool) -> Native {
    let mut native = fixture("sha256");
    let old_tree = tree(&native.fixture, &[(b"z-small.rs", b"old\n", "100644")]);
    native.c = commit(&native.fixture, &old_tree, None, "Metadata-common");
    native.a = commit(
        &native.fixture,
        &tree(
            &native.fixture,
            &[(b"z-small.rs", b"REQUESTED A IS NOT OLD\n", "100644")],
        ),
        Some(&native.c),
        "Metadata-requested",
    );
    let large = vec![b'x'; 800_000];
    let new_tree = if empty {
        old_tree
    } else {
        tree(
            &native.fixture,
            &[
                (b"a-large.rs", &large, "100644"),
                (b"z-small.rs", b"small actual target\n", "100644"),
            ],
        )
    };
    native.b = commit(
        &native.fixture,
        &new_tree,
        Some(&native.c),
        "Metadata-target",
    );
    native
        .fixture
        .git(&["update-ref", "refs/heads/requested-A", &native.a]);
    native
        .fixture
        .git(&["update-ref", "refs/heads/target-B", &native.b]);
    native.before = state(&native.fixture);
    native
}

#[test]
fn native_diff_keyboard_metadata_first_oversize_source_then_other_file_no_fake_range_readonly() {
    metadata(false, false);
}
#[test]
fn native_diff_mouse_metadata_first_oversize_source_then_other_file_no_fake_range_readonly() {
    metadata(true, false);
}
#[test]
fn native_diff_keyboard_metadata_complete_empty_no_fake_file_range_or_write_readonly() {
    metadata(false, true);
}
#[test]
fn native_diff_mouse_metadata_complete_empty_no_fake_file_range_or_write_readonly() {
    metadata(true, true);
}
fn metadata(mouse: bool, empty: bool) {
    let native = metadata_fixture(empty);
    let names = git_bytes(
        &native.fixture,
        &["diff", "--name-only", "-z", &native.c, &native.b, "--"],
        None,
    );
    assert_eq!(
        names,
        if empty {
            b"".as_slice()
        } else {
            b"a-large.rs\0z-small.rs\0".as_slice()
        },
        "independent installed-Git canonical catalogue"
    );
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, true);
    let before = starts(&native).len();
    session.choose_compact(mouse, "[v Native diff]", "[v Diff]");
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            screen.contains(NATIVE_TITLE)
                && screen.contains("No prepared rows • no selection authority")
                && if empty {
                    screen.contains("No changed files in this complete original comparison")
                } else {
                    screen.contains("original catalogue")
                        && screen.contains("retained; no current row/range authority")
                }
        },
        "genuine source-only outcome without invented rows or selections",
        Duration::from_secs(8),
    );
    assert_eq!(
        starts(&native).len() - before,
        9,
        "actual first original producer remains one sealed nine-child operation"
    );
    no_discovery(&native, before);
    if empty {
        assert!(!session
            .completed_cursorless_screen()
            .unwrap()
            .contents()
            .contains("File 1/"));
    } else {
        assert!(session
            .completed_cursorless_screen()
            .unwrap()
            .contents()
            .contains("File 1/2: [old absent] → a-large.rs"));
    }
    choose(&mut session, mouse, "[h Hunk]", b"h");
    session.wait_for_cursorless_condition(
        |frame| {
            frame
                .contents()
                .contains("Choose a genuine changed-line row")
                && frame
                    .contents()
                    .contains("No prepared rows • no selection authority")
        },
        "no hunk can manufacture source-only row authority",
        Duration::from_secs(8),
    );
    choose(&mut session, mouse, "[I Source]", b"I");
    source_all(&mut session, mouse, [&native.a, &native.c, &native.b]);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close("Repository source identity • read-only");
    choose(&mut session, mouse, "[o Options]", b"o");
    session.wait_for_screen("[a Stage disabled]");
    choose(&mut session, mouse, "[a Stage disabled]", b"a");
    session.wait_for_screen("Stage disabled: this is a saved comparison");
    choose(&mut session, mouse, "[o Back to diff]", b"o");
    choose(&mut session, mouse, "[] File]", b"]");
    if empty {
        session.wait_for_cursorless_condition(
            |frame| {
                frame.contents().contains("No admitted changed files")
                    && frame
                        .contents()
                        .contains("No prepared rows • no selection authority")
            },
            "complete empty source cannot invent a next file",
            Duration::from_secs(8),
        );
    } else {
        ready(&mut session, "small actual target");
        assert!(session
            .completed_cursorless_screen()
            .unwrap()
            .contents()
            .contains("File 2/2: z-small.rs → z-small.rs"));
        assert!(!session
            .completed_cursorless_screen()
            .unwrap()
            .contents()
            .contains("REQUESTED A IS NOT OLD"));
    }
    assert_eq!(
        starts(&native).len(),
        before + 9,
        "source-only controls and cached recovery add no Git children"
    );
    effects(&native, &session);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native saved diff closed");
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}

/// TASK-31: two hunks, a whitespace-only pair and two `mark` lines for the controls below.
fn explore_fixture() -> Native {
    let mut native = fixture("sha256");
    let body = |first: &str, spaced: &str, tail: &str| {
        let mut text = format!("fn main() {{\n{first}\n");
        for n in 1..=12 {
            text.push_str(&format!("    let keep_{n} = {n};\n"));
        }
        text.push_str(&format!("{spaced}\n{tail}\n}}\n"));
        text.into_bytes()
    };
    let old = body(
        "    let old_name = 1; // mark one",
        "    let spaced = 3;",
        "    let tail = \"old\"; // mark two",
    );
    let new = body(
        "    let new_name = 2;",
        "    let  spaced = 3;",
        "    let tail = \"new\";",
    );
    native.c = commit(
        &native.fixture,
        &tree(&native.fixture, &[(b"code.rs", &old, "100644")]),
        None,
        "Native-common",
    );
    native.a = commit(
        &native.fixture,
        &tree(
            &native.fixture,
            &[(b"code.rs", b"REQUESTED A IS NOT OLD\n", "100644")],
        ),
        Some(&native.c),
        "Native-requested",
    );
    native.b = commit(
        &native.fixture,
        &tree(&native.fixture, &[(b"code.rs", &new, "100644")]),
        Some(&native.c),
        "Native-target",
    );
    native
        .fixture
        .git(&["update-ref", "refs/heads/requested-A", &native.a]);
    native
        .fixture
        .git(&["update-ref", "refs/heads/target-B", &native.b]);
    native.before = state(&native.fixture);
    native
}
/// A new completed, ready native frame that satisfies `check`.
fn ready_when(session: &mut Session, after: usize, label: &str, check: impl Fn(&str) -> bool) {
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let screen = frame.contents();
            screen.contains(NATIVE_TITLE)
                && screen.contains("Native saved diff ready")
                && !screen.contains("Preparing native diff")
                && check(&screen)
        },
        label,
        Duration::from_secs(8),
    );
}
/// The row carrying the `>` cursor shows `text`.
fn cursor_on(screen: &str, text: &str) -> bool {
    screen
        .lines()
        .any(|line| line.contains("│>") && line.contains(text))
}
/// One Options control: the key directly, or by mouse Options, the control, Back to diff.
fn control(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action("[o Options]");
        session.wait_for_screen("[o Back to diff]");
        session.click_visible_action(label);
        session.click_visible_action("[o Back to diff]");
        session.wait_for_screen("[o Options]");
    } else {
        session.send_key(key);
    }
}
/// Is the first cell of `text` on the completed frame bold?
fn bold_at(session: &Session, text: &str) -> Option<bool> {
    let screen = session.completed_cursorless_screen()?;
    let (x, y) = crate::support::locate_screen(screen, text)?;
    screen.cell(y, x).map(|cell| cell.bold())
}
#[test]
fn native_diff_keyboard_hunks_matches_context_whitespace_syntax_within_line_readonly() {
    explore(false);
}
#[test]
fn native_diff_mouse_hunks_matches_context_whitespace_syntax_within_line_readonly() {
    explore(true);
}
fn explore(mouse: bool) {
    let native = explore_fixture();
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, true);
    choose(&mut session, mouse, "[: Commands]", b":");
    session.wait_for_screen("Commands");
    session.paste("native saved diff");
    session.wait_for_screen("Explore native saved diff");
    if mouse {
        session.click_visible_action("Explore native saved diff");
    } else {
        session.send_key(b"\r");
    }
    ready(&mut session, "keep_1 = 1;");
    let before = starts(&native).len();

    // i Within-line: the changed bytes of a paired line are bold, and plain once it is off.
    assert_eq!(
        bold_at(&session, "new_name"),
        Some(true),
        "{}",
        session.screen()
    );
    for on in [false, true] {
        let after = session.completed_cursorless_end();
        control(&mut session, mouse, "[i Within-line]", b"i");
        ready_when(&mut session, after, "within-line toggled", |s| {
            s.contains("old_name")
        });
        let deadline = Instant::now() + Duration::from_secs(8);
        while bold_at(&session, "new_name") != Some(on) {
            assert!(
                Instant::now() < deadline,
                "within-line {on}\n{}",
                session.screen()
            );
            session.poll_output(Duration::from_millis(10));
        }
    }

    // y Syntax: the metadata line says when highlighting is off, and recovers.
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[y Syntax]", b"y");
    ready_when(&mut session, after, "syntax off", |s| {
        s.contains("Syntax off; original patch text")
    });
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[y Syntax]", b"y");
    ready_when(&mut session, after, "syntax on", |s| {
        !s.contains("Syntax off") && s.contains("old_name")
    });

    // c Context: unchanged lines leave and return; the changed lines stay.
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[c Context]", b"c");
    ready_when(&mut session, after, "context hidden", |s| {
        !s.contains("keep_1 = 1;") && s.contains("old_name") && s.contains("new_name")
    });
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[c Context]", b"c");
    ready_when(&mut session, after, "context shown", |s| {
        s.contains("keep_1 = 1;")
    });

    // s Whitespace cycles Show, Mark, hide whitespace-only pairs, Show.
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[s Whitespace]", b"s");
    ready_when(&mut session, after, "whitespace marked", |s| {
        s.contains("let··spaced")
    });
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[s Whitespace]", b"s");
    ready_when(&mut session, after, "whitespace-only pair hidden", |s| {
        !s.contains("spaced") && s.contains("mark two")
    });
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, "[s Whitespace]", b"s");
    ready_when(&mut session, after, "whitespace shown", |s| {
        s.contains("let  spaced")
    });

    // Search, then { } hunk navigation and n / p match navigation move the cursor row.
    if mouse {
        session.click_visible_action("Exact:");
    } else {
        session.send_key(b"/");
    }
    session.paste("mark");
    session.send_key(b"\r");
    ready(&mut session, "2 matches");
    for (label, key, row) in [
        ("[{ Previous hunk]", &b"{"[..], "fn main() {"),
        ("[} Next hunk]", b"}", "keep_10 = 10;"),
        ("[{ Previous hunk]", b"{", "fn main() {"),
        ("[n Matching line]", b"n", "mark one"),
        ("[n Matching line]", b"n", "mark two"),
        ("[p Previous match]", b"p", "mark one"),
    ] {
        let after = session.completed_cursorless_end();
        control(&mut session, mouse, label, key);
        ready_when(
            &mut session,
            after,
            &format!("{label} selects {row}"),
            |s| cursor_on(s, row),
        );
    }
    assert_eq!(
        starts(&native).len(),
        before,
        "every exploration control reuses the prepared source: no Git child"
    );
    effects(&native, &session);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native saved diff closed");
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
