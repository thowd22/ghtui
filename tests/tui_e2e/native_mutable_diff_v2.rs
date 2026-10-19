//! Genuine current local comparisons through the same native diff controller.
use super::*;
const LOCAL_TITLE: &str = "Native local diff • observed snapshot";
#[path = "native_mutable_plan_v2.rs"]
mod native_mutable_plan_v2;
#[path = "native_uncommitted_v3.rs"]
mod native_uncommitted_v3;
fn ack(session: &mut Session, after: usize, mut predicate: impl FnMut(&str) -> bool, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cursorless_end() > after
            && session
                .completed_cursorless_screen()
                .is_some_and(|frame| predicate(&frame.contents()))
        {
            return;
        }
        assert!(Instant::now() < deadline, "{label}");
    }
}
fn ready(session: &mut Session, text: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            screen.contains(LOCAL_TITLE)
                && screen.contains("Native local diff ready")
                && screen.contains(text)
        },
        "completed genuine local native frame",
        Duration::from_secs(8),
    );
}
fn open_local(native: &Native, session: &mut Session, mouse: bool, staged: bool) {
    let before = session.completed_cursorless_end();
    choose(
        session,
        mouse,
        if staged { "[i Staged]" } else { "[w Working]" },
        if staged { b"i" } else { b"w" },
    );
    ack(
        session,
        before,
        |screen| {
            (screen.contains("v Native diff") || screen.contains("v Diff  "))
                && screen.contains("user staged bytes")
                && screen.contains(if staged {
                    "saved checkout tracked"
                } else {
                    "user dirty bytes"
                })
        },
        "loaded genuine Working/Staged comparison before native entry",
    );
    let count = starts(native).len();
    session.choose_compact(mouse, "[v Native diff]", "[v Diff]");
    ready(session, "user staged bytes");
    assert_eq!(
        starts(native).len() - count,
        if staged { 23 } else { 22 },
        "same original operation: factory and final fresh check, no discovery"
    );
    no_discovery(native, count);
}
fn focus(session: &mut Session, mouse: bool, text: &str) {
    let label = |line: &str| {
        line.strip_prefix("│>")
            .or_else(|| line.strip_prefix("│ "))
            .map(|text| text.trim_end_matches('│').trim().to_owned())
    };
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            screen.contains(LOCAL_TITLE)
                && screen.contains("Native local diff ready")
                && screen.contains("File 1/1: tracked → tracked")
                && screen
                    .lines()
                    .filter_map(label)
                    .any(|row| row.starts_with("index "))
                && screen.contains(text)
        },
        "complete original native catalogue and actual index/content rows before cursor input",
        Duration::from_secs(8),
    );
    let selected = |screen: &str| {
        screen
            .lines()
            .any(|line| line.starts_with("│>") && line.contains(text))
    };
    if mouse {
        let before = session.completed_cursorless_end();
        let (x, y) = session
            .locate(text)
            .expect("genuine visible local changed row");
        session.click(x, y);
        ack(
            session,
            before,
            selected,
            "exact clicked local row must be selected before range action",
        );
    } else {
        // A newer ready repaint alone is not acknowledgment of Home/Down: it
        // can have been emitted before that input was consumed. Bind each
        // step to the actual original row shown by the previous full frame.
        let original = session.completed_cursorless_screen().unwrap().contents();
        let original_index = original
            .lines()
            .filter_map(label)
            .find(|row| row.starts_with("index "))
            .unwrap_or_else(|| {
                panic!(
                    "actual original canonical index metadata row missing from captured completed frame:\n{original}"
                )
            });
        let first = original
            .lines()
            .find_map(|line| {
                line.split_once("File 1/1: ")
                    .map(|(_, name)| name.trim_end_matches('│').trim().to_owned())
            })
            .expect("actual original changed-file catalogue label");
        assert_eq!(
            first, "tracked → tracked",
            "independent fixture's canonical raw-path pair"
        );
        let selected_label = |screen: &str| {
            screen.lines().find_map(|line| {
                line.strip_prefix("│>")
                    .map(|text| text.trim_end_matches('│').trim().to_owned())
            })
        };
        let before = session.completed_cursorless_end();
        session.send_key(b"\x1b[H");
        ack(
            session,
            before,
            |screen| {
                screen.contains(LOCAL_TITLE)
                    && screen.contains("Native local diff ready")
                    && screen.contains(&original_index)
                    && selected_label(screen).as_ref() == Some(&first)
            },
            "completed original exact local Home file-header selection",
        );
        for attempt in 0..16 {
            if selected(&session.completed_cursorless_screen().unwrap().contents()) {
                return;
            }
            assert!(
                attempt < 15,
                "independently named actual local row must be selected"
            );
            let screen = session.completed_cursorless_screen().unwrap().contents();
            let rows = screen.lines().collect::<Vec<_>>();
            let current = rows
                .iter()
                .position(|line| line.starts_with("│>"))
                .expect("actual selected original native row before Down");
            let next = rows
                .get(current + 1)
                .and_then(|line| label(line))
                .filter(|next| !next.is_empty() && !next.starts_with('['))
                .expect("next actual visible original native row, not a guessed target or footer");
            assert_ne!(
                selected_label(&screen).as_ref(),
                Some(&next),
                "fixture rows must be independently distinguishable"
            );
            let before = session.completed_cursorless_end();
            session.send_key(b"\x1b[B");
            ack(
                session,
                before,
                |screen| {
                    screen.contains(LOCAL_TITLE)
                        && screen.contains("Native local diff ready")
                        && screen.contains(&first)
                        && screen.contains(&original_index)
                        && selected_label(screen).as_ref() == Some(&next)
                },
                "new complete exact predicted next original local row after one Down",
            );
        }
    }
}
#[test]
fn native_mutable_diff_keyboard_working_staged_exact_rows_range_search_resize_source_and_readonly()
{
    journey(false);
}
#[test]
fn native_mutable_diff_mouse_working_staged_exact_rows_range_search_resize_source_and_readonly() {
    journey(true);
}
fn journey(mouse: bool) {
    let native = fixture("sha256");
    let mut session = launch(&native, mouse);
    session.wait_for_screen("Recent commits");
    for staged in [false, true] {
        open_local(&native, &mut session, mouse, staged);
        let old = if staged {
            "saved checkout tracked"
        } else {
            "user staged bytes"
        };
        let new = if staged {
            "user staged bytes"
        } else {
            "user dirty bytes"
        };
        ready(&mut session, old);
        session.wait_for_screen(new);
        session.wait_for_screen("Unsaved content: author unknown");
        assert!(!session.screen().contains("New commit:"));
        focus(&mut session, mouse, old);
        let before = starts(&native).len();
        choose(&mut session, mouse, "[h Hunk]", b"h");
        ready(&mut session, "Selected 2 (hidden 0)");
        assert_eq!(
            starts(&native).len() - before,
            10,
            "cached row selection freshly validates real mutable state before and after projection"
        );
        session.resize_owned_modal(40, 24, LOCAL_TITLE, &["[Esc Close]", "[o Options]"]);
        ready(&mut session, "Selected 2 (hidden 0)");
        session.resize_owned_modal(120, 40, LOCAL_TITLE, &["[Esc Close]", "[o Options]"]);
        ready(&mut session, "Selected 2 (hidden 0)");
        choose(&mut session, mouse, "Exact:", b"/");
        session.paste("bytes");
        session.send_key(b"\r");
        let patch = git_bytes(
            &native.fixture,
            if staged {
                &["diff", "--cached", "--no-ext-diff"]
            } else {
                &["diff", "--no-ext-diff"]
            },
            None,
        );
        let matches = String::from_utf8(patch)
            .unwrap()
            .lines()
            .filter(|line| {
                (line.starts_with('+') && !line.starts_with("+++"))
                    || (line.starts_with('-') && !line.starts_with("---"))
            })
            .filter(|line| line.contains("bytes"))
            .count();
        assert_eq!(
            matches,
            if staged { 1 } else { 2 },
            "independent installed-Git changed-line query scope"
        );
        ready(&mut session, &format!("{matches} matches"));
        let before = starts(&native).len();
        choose(&mut session, mouse, "[I Source]", b"I");
        session.wait_for_cursorless_condition(
            |frame| {
                let screen = frame.contents();
                screen.contains("Repository source identity • read-only")
                    && screen.contains(if staged {
                        "index vs HEAD; mutable"
                    } else {
                        "working files vs index; mutable"
                    })
            },
            "genuine original mutable source identity",
            Duration::from_secs(8),
        );
        assert_eq!(
            starts(&native).len(),
            before,
            "source labels are already prepared; no provider read"
        );
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        ready(&mut session, "Selected 2 (hidden 0)");
        effects(&native, &session);
        let before = session.completed_cursorless_end();
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        ack(
            &mut session,
            before,
            |screen| {
                screen.contains("Native local diff closed")
                    && !screen.contains(LOCAL_TITLE)
                    && (screen.contains("v Native diff") || screen.contains("v Diff  "))
            },
            "completed exact native local Close returns loaded comparison",
        );
        effects(&native, &session);
    }
    session.quit_and_assert_restored_terminal(mouse);
}
