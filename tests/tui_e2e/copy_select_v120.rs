//! TASK-120: copying text out of the real binary over the History fixture at 140x42.
//! The mouse run drags over the full SHA in the commit detail: the cells are painted reversed,
//! release copies exactly that text as an OSC 52 escape in the PTY output, and a fake
//! `wl-copy` on PATH (with a Wayland display set) receives the same bytes on stdin. The
//! keyboard run presses `y` for the selected commit's SHA with the same two effects. Both
//! runs release mouse capture with F12 (the bar shows `F12 Mouse off` and the terminal gets
//! the disable sequence), then restore it, by key or by clicking the token. HEAD, refs, the
//! index, status and config never change.
use super::*;
use base64::Engine as _;
use ghtui::config::Config;
use std::{path::Path, time::Duration};
use support::history_fixture::*;

/// The OSC 52 clipboard escape a terminal receives for `text`.
pub fn osc52(text: &str) -> Vec<u8> {
    let mut bytes = b"\x1b]52;c;".to_vec();
    bytes.extend_from_slice(
        base64::engine::general_purpose::STANDARD
            .encode(text)
            .as_bytes(),
    );
    bytes.push(0x07);
    bytes
}

/// Waits for the next OSC 52 escape after byte `from` and returns its decoded text.
pub fn wait_for_osc52_text(s: &mut Session, from: usize) -> String {
    let start = s.wait_for_output_bytes(from, b"\x1b]52;c;", "an OSC 52 copy") + 7;
    let end = s.wait_for_output_bytes(start, b"\x07", "the end of the OSC 52 copy");
    let encoded = s.transcript[start..end].to_vec();
    String::from_utf8(
        base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .expect("OSC 52 payload is base64"),
    )
    .expect("OSC 52 payload is UTF-8")
}

/// A stand-in `wl-copy` that stores its stdin and arguments under the fixture.
fn fake_wl_copy(fixture: &Fixture) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let out = fixture.root.join("clipboard.txt");
    let script = fixture.root.join("bin/wl-copy");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" > '{0}.args'\ncat > '{0}'\n",
            out.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    out
}

fn wait_for_file(path: &Path, expected: &str) {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        if std::fs::read_to_string(path).is_ok_and(|text| text == expected) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "fake wl-copy never received {expected:?}: {:?}",
            std::fs::read_to_string(path)
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn footer(s: &Session) -> String {
    let screen = s.completed_cursorless_screen().unwrap();
    screen
        .contents()
        .lines()
        .rev()
        .find(|row| !row.trim().is_empty())
        .unwrap_or_default()
        .to_owned()
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    let repo = HistoryRepo::build(&fixture, HistoryOptions::default());
    Config {
        refresh_seconds: 3600,
        avatars: false,
        ..Default::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let clipboard = fake_wl_copy(&fixture);
    let baseline = repo.state();
    let mut s = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        140,
        42,
        &[("WAYLAND_DISPLAY", "wayland-ghtui-test")],
    );
    s.wait_for_screen("Recent commits");
    s.choose(mouse, "[g History]", b"g");
    s.wait_for_history_ready();
    s.wait_for_settled_footer();
    s.wait_for_history_selected(repo.sha("tweak"));
    let sha = repo.sha("tweak").to_owned();
    s.wait_for_screen(&sha);

    let before = s.transcript.len();
    if mouse {
        // Drag over the full SHA in the detail pane, first cell to last.
        let (x, y) = s.locate(&sha).expect("full SHA in the detail");
        s.drag((x, y), (x + 39, y));
        s.wait_for_output_bytes(before, &osc52(&sha), "OSC 52 copy of the dragged SHA");
        s.wait_for_screen("Copied selection (40 chars) via terminal OSC 52 + wl-copy");
        // The selection stays painted (reversed) until the next key.
        let screen = s.completed_cursorless_screen().unwrap();
        assert!(
            (x..x + 40).all(|column| screen.cell(y, column).is_some_and(|cell| cell.inverse())),
            "the dragged SHA is highlighted\n{}",
            s.screen()
        );
    } else {
        s.send_key(b"y");
        s.wait_for_output_bytes(before, &osc52(&sha), "OSC 52 copy of the selected SHA");
        s.wait_for_screen("Copied commit SHA (40 chars) via terminal OSC 52 + wl-copy");
    }
    // The clipboard program got the same text on stdin, in the foreground, with no shell.
    wait_for_file(&clipboard, &sha);
    assert_eq!(
        std::fs::read_to_string(clipboard.with_extension("txt.args")).unwrap(),
        "--foreground\n"
    );
    assert_eq!(repo.state(), baseline);

    // F12 releases mouse capture for native terminal selection; the bar says so.
    let before = s.transcript.len();
    s.send_key(b"\x1b[24~");
    s.wait_for_output_bytes(before, b"\x1b[?1000l", "mouse capture disabled");
    s.wait_for_screen("Mouse released");
    s.wait_for_cursorless_condition(
        |screen| {
            screen
                .contents()
                .lines()
                .rev()
                .find(|row| !row.trim().is_empty())
                .is_some_and(|row| row.contains("F12 Mouse off"))
        },
        "F12 Mouse off in the key bar",
        Duration::from_secs(8),
    );
    // Restore by the same key, or by the token (a terminal that still reports the click).
    let before = s.transcript.len();
    if mouse {
        s.click_visible_action("[F12 Mouse off]");
    } else {
        s.send_key(b"\x1b[24~");
    }
    s.wait_for_output_bytes(before, b"\x1b[?1000h", "mouse capture restored");
    s.wait_for_screen("Mouse restored");
    s.wait_for_cursorless_condition(
        |screen| {
            screen
                .contents()
                .lines()
                .rev()
                .find(|row| !row.trim().is_empty())
                .is_some_and(|row| !row.contains("Mouse off"))
        },
        "key bar without the Mouse off token",
        Duration::from_secs(8),
    );
    assert!(!footer(&s).contains("F12"), "{}", footer(&s));
    assert_eq!(repo.state(), baseline);
    s.quit_and_assert_restored_terminal(mouse);
}

/// Refs, status and index bytes of a plain fixture repository.
fn plain_state(fixture: &Fixture) -> (String, String, Option<Vec<u8>>) {
    (
        fixture.git(&["for-each-ref"]),
        fixture.git(&["status", "--porcelain"]),
        std::fs::read(fixture.repo().join(".git/index")).ok(),
    )
}

/// The key bar row of the latest completed frame.
fn key_bar(s: &Session) -> String {
    footer(s)
}

/// Repository Overview: choose the changed file's row, then `y` (keyboard) or the `y Copy`
/// key-bar token (mouse) copies exactly its repository path. At 80x24 the key bar keeps its
/// Global tail. Nothing in Git changes and no gh command runs.
fn repository_path_journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("notes.txt", b"first\n", "First note");
    std::fs::write(fixture.repo().join("notes.txt"), b"edited\n").unwrap();
    let baseline = plain_state(&fixture);
    // The Repository bar fills 120 columns, so the lowest-priority y Copy shows from 160.
    let mut s = Session::launch(&fixture, 160, 40);
    s.wait_for_screen("Recent commits");
    s.wait_for_screen("Changes · 1 unstaged");
    if mouse {
        crate::local_management_v41::activate_row(&mut s, mouse, "notes.txt");
    } else {
        // The changed file is the first row of the keyboard order (Up from it stays there),
        // so one Down then Ups select it without counting rows; Enter chooses it once the
        // bar is on it.
        s.send_key(b"\x1b[B");
        s.send_key(&b"\x1b[A".repeat(8));
        s.try_wait_for_condition(
            |screen| {
                screen
                    .lines()
                    .any(|line| line.contains("notes.txt") && line.contains('▌'))
            },
            "the changed file row selected",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}\n{}", s.screen()));
        s.send_key(b"\r");
    }
    // Choosing the row reinstalls the workspace, whose status line would replace a copy's
    // confirmation: wait until it is in place.
    s.wait_for_screen("Selected working file: notes.txt");
    s.wait_for_settled_footer();
    s.wait_for_cursorless_condition(
        |screen| {
            screen
                .contents()
                .lines()
                .rev()
                .find(|row| !row.trim().is_empty())
                .is_some_and(|row| row.contains("y Copy") && row.contains("q Quit"))
        },
        "y Copy in the Repository key bar",
        Duration::from_secs(8),
    );
    let before = s.transcript.len();
    if mouse {
        s.click_visible_action("[y Copy]");
    } else {
        s.send_key(b"y");
    }
    s.wait_for_output_bytes(before, &osc52("notes.txt"), "OSC 52 copy of the file path");
    s.wait_for_screen("Copied path (9 chars) via terminal OSC 52");
    assert_eq!(plain_state(&fixture), baseline);

    // At 80x24 the bar still ends with the Global tail; y Copy is the first to leave it.
    s.resize(80, 24);
    s.wait_for_settled_footer();
    let bar = key_bar(&s);
    for token in ["? Help", ": Cmds", "q Quit"] {
        assert!(bar.contains(token), "{token} missing at 80x24: {bar}");
    }
    assert!(!bar.contains('['), "{bar}");
    assert!(
        !bar.contains("y Copy"),
        "y Copy leaves a full bar first: {bar}"
    );
    s.resize(160, 40);
    s.wait_for_settled_footer();
    assert_eq!(plain_state(&fixture), baseline);
    assert!(s.inspect_command_journal(&fixture).is_empty());
    s.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn copy_select_v120_keyboard_y_copies_repository_file_path() {
    repository_path_journey(false);
}

#[test]
fn copy_select_v120_mouse_y_copy_token_copies_repository_file_path() {
    repository_path_journey(true);
}

#[test]
fn copy_select_v120_keyboard_y_copies_sha_and_f12_toggles_mouse() {
    journey(false);
}

#[test]
fn copy_select_v120_mouse_drag_copies_selection_and_token_restores_mouse() {
    journey(true);
}
