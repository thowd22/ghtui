//! Actual binary explicit selected-write journeys, using installed Git as oracle.
use super::*;
const PLAN_TITLE: &str = "Selected patch • confirm";
fn preview(session: &mut Session, mouse: bool) {
    if mouse
        && !session
            .completed_cursorless_screen()
            .unwrap()
            .contents()
            .contains("[a Preview selected]")
    {
        choose(session, true, "[o Options]", b"o");
        session.wait_for_cursorless_condition(
            |frame| {
                frame.contents().contains(LOCAL_TITLE)
                    && frame.contents().contains("[a Preview selected]")
            },
            "completed visible clickable preview control in native options",
            Duration::from_secs(8),
        );
    }
    choose(session, mouse, "[a Preview selected]", b"a");
}
fn plan_ready(session: &mut Session, command: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            let s = frame.contents();
            s.contains(PLAN_TITLE)
                && s.contains("Ready preview")
                && s.contains(command)
                && s.contains("[Enter Confirm]")
        },
        "completed original exact selected-plan preview",
        Duration::from_secs(8),
    );
}
fn observed(native: &Native) -> Vec<Vec<u8>> {
    let f = &native.fixture;
    let before = fs::read(f.repo().join(".git/index")).unwrap();
    let state = vec![
        git_bytes(f, &["rev-parse", "HEAD"], None),
        git_bytes(f, &["show-ref"], None),
        fs::read(f.repo().join(".git/index")).unwrap(),
        git_bytes(
            f,
            &["--no-optional-locks", "status", "--porcelain=v2", "-z"],
            None,
        ),
        git_bytes(
            f,
            &[
                "-c",
                "diff.autoRefreshIndex=false",
                "diff",
                "--cached",
                "--binary",
                "--no-ext-diff",
            ],
            None,
        ),
        git_bytes(
            f,
            &[
                "-c",
                "diff.autoRefreshIndex=false",
                "diff",
                "--binary",
                "--no-ext-diff",
            ],
            None,
        ),
        fs::read(f.repo().join("tracked")).unwrap(),
        fs::read(f.repo().join("scratch")).unwrap(),
        fs::read(f.repo().join(".git/config")).unwrap(),
    ];
    assert_eq!(
        fs::read(f.repo().join(".git/index")).unwrap(),
        before,
        "independent observer itself must preserve full raw index bytes"
    );
    state
}
fn apply_count(native: &Native) -> usize {
    starts(native)
        .iter()
        .filter(|event| {
            event["argv"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg.as_str() == Some("apply"))
        })
        .count()
}
fn private_effects(native: &Native, session: &Session) {
    assert_eq!(sql(&native.fixture), native.sql);
    assert_eq!(
        fs::read(native.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        native.config
    );
    assert!(session.inspect_command_journal(&native.fixture).is_empty());
    assert!(fs::read(native.fixture.root.join("curl-journal"))
        .unwrap()
        .is_empty());
    assert!(!crate::raw_git_argv_v1::ascii_trace2(
        &native.fixture.root.join("diff-sides-trace.jsonl")
    )
    .iter()
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
                    | "push"
                    | "clone"
                    | "merge"
                    | "rebase"
                    | "update-ref"
            )
        )));
}
fn selected(native: &Native, session: &mut Session, mouse: bool, staged: bool) {
    open_local(native, session, mouse, staged);
    focus(
        session,
        mouse,
        if staged {
            "saved checkout tracked"
        } else {
            "user staged bytes"
        },
    );
    choose(session, mouse, "[h Hunk]", b"h");
    ready(session, "Selected 2 (hidden 0)");
}
#[test]
fn native_mutable_plan_keyboard_exact_one_line_stage_unstage_preserves_unselected_changes() {
    one_line(false);
}
#[test]
fn native_mutable_plan_mouse_exact_one_line_stage_unstage_preserves_unselected_changes() {
    one_line(true);
}
fn one_line(mouse: bool) {
    for staged in [false, true] {
        let native = fixture("sha256");
        if staged {
            fs::write(
                native.fixture.repo().join("tracked"),
                b"saved checkout tracked\nselected one\nuser staged bytes\n",
            )
            .unwrap();
            native.fixture.git(&["add", "--", "tracked"]);
            fs::write(native.fixture.repo().join("tracked"), b"user dirty bytes\n").unwrap();
        } else {
            fs::write(
                native.fixture.repo().join("tracked"),
                b"user staged bytes\nselected one\nuser dirty bytes\n",
            )
            .unwrap();
        }
        let mut session = launch(&native, mouse);
        session.wait_for_screen("Recent commits");
        open_local(&native, &mut session, mouse, staged);
        focus(&mut session, mouse, "selected one");
        if mouse {
            choose(&mut session, true, "[o Options]", b"o");
            session.wait_for_cursorless_condition(
                |frame| frame.contents().contains("[Space Select line]"),
                "completed actual mouse parity for single original line",
                Duration::from_secs(8),
            );
        }
        choose(&mut session, mouse, "[Space Select line]", b" ");
        ready(&mut session, "Selected 1 (hidden 0)");
        let before = observed(&native);
        preview(&mut session, mouse);
        plan_ready(
            &mut session,
            if staged {
                "Unstage selected lines"
            } else {
                "Stage selected lines"
            },
        );
        assert_eq!(observed(&native), before);
        assert_eq!(apply_count(&native), 0);
        let screen = session.completed_cursorless_screen().unwrap().contents();
        assert!(
            screen.contains(if staged {
                "-selected one"
            } else {
                "+selected one"
            }),
            "actual exact selected patch is displayed"
        );
        choose(&mut session, mouse, "[Enter Confirm]", b"\r");
        session.wait_for_cursorless_condition(
            |frame| {
                let s = frame.contents();
                s.contains(PLAN_TITLE)
                    && s.contains("completed • index changed only")
                    && s.contains("old diff rows retired")
            },
            "completed exact one-line selected write",
            Duration::from_secs(8),
        );
        assert_eq!(apply_count(&native), 1);
        assert_eq!(
            git_bytes(&native.fixture, &["show", ":tracked"], None),
            if staged {
                b"saved checkout tracked\nuser staged bytes\n".as_slice()
            } else {
                b"user staged bytes\nselected one\n".as_slice()
            }
        );
        let after = observed(&native);
        assert_ne!(after[2], before[2]);
        for field in [0, 1, 6, 7, 8] {
            assert_eq!(after[field], before[field]);
        }
        private_effects(&native, &session);
        session.quit_and_assert_restored_terminal(mouse);
    }
}
#[test]
fn native_mutable_plan_keyboard_preview_back_confirm_working_stage_and_staged_unstage() {
    write_journey(false);
}
#[test]
fn native_mutable_plan_mouse_preview_back_confirm_working_stage_and_staged_unstage() {
    write_journey(true);
}
fn write_journey(mouse: bool) {
    for staged in [false, true] {
        let native = fixture("sha256");
        let mut session = launch(&native, mouse);
        session.wait_for_screen("Recent commits");
        selected(&native, &mut session, mouse, staged);
        let before = observed(&native);
        let command = if staged {
            "Unstage selected lines"
        } else {
            "Stage selected lines"
        };
        preview(&mut session, mouse);
        plan_ready(&mut session, command);
        assert_eq!(apply_count(&native), 0);
        assert_eq!(observed(&native), before);
        private_effects(&native, &session);
        session.resize_owned_modal(40, 8, PLAN_TITLE, &["[Esc Close]", "[Enter disabled]"]);
        let tiny = session.completed_cursorless_screen().unwrap().contents();
        assert!(tiny.contains("confirmation disabled") && !tiny.contains("[Enter Confirm]"));
        let tiny_before = session.completed_cursorless_end();
        choose(&mut session, mouse, "[Enter disabled]", b"\r");
        session.wait_for_cursorless_condition(
            |frame| {
                let s = frame.contents();
                s.contains(PLAN_TITLE)
                    && s.contains("Confirm refused; resize to34x14")
                    && !s.contains("[Enter Confirm]")
            },
            "completed disabled tiny preview refuses explicit confirmation",
            Duration::from_secs(8),
        );
        assert!(
            session.completed_cursorless_end() > tiny_before,
            "disabled explanation is a new complete actual frame"
        );
        assert_eq!(apply_count(&native), 0);
        assert_eq!(observed(&native), before);
        session.resize_owned_modal(40, 24, PLAN_TITLE, &["[Esc Back]", "[Enter Confirm]"]);
        plan_ready(&mut session, command);
        session.send_key(b"\x1b[6~\x1b[5~\x1b[C\x1b[D");
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
        ready(&mut session, "Selected 2 (hidden 0)");
        session.resize_owned_modal(120, 40, LOCAL_TITLE, &["[Esc Close]", "[o Options]"]);
        ready(&mut session, "Selected 2 (hidden 0)");
        assert_eq!(apply_count(&native), 0);
        assert_eq!(observed(&native), before);
        preview(&mut session, mouse);
        plan_ready(&mut session, command);
        choose(&mut session, mouse, "[Enter Confirm]", b"\r");
        session.wait_for_cursorless_condition(
            |frame| {
                let s = frame.contents();
                s.contains(PLAN_TITLE)
                    && s.contains("old diff rows retired")
                    && s.contains("completed • index changed only")
                    && s.contains("[r Reload/inspect]")
                    && !s.contains("[Enter Confirm]")
            },
            "completed independently confirmed selected index-only result",
            Duration::from_secs(8),
        );
        assert_eq!(
            apply_count(&native),
            1,
            "exact one real cached apply, no automatic retry"
        );
        assert_eq!(
            git_bytes(&native.fixture, &["show", ":tracked"], None),
            if staged {
                b"saved checkout tracked\n".as_slice()
            } else {
                b"user dirty bytes\n".as_slice()
            }
        );
        let after = observed(&native);
        assert_ne!(
            after[2], before[2],
            "full raw index changed only after explicit Confirm"
        );
        for field in [0, 1, 6, 7, 8] {
            assert_eq!(
                after[field], before[field],
                "refs/worktree/untracked/config preserved"
            );
        }
        let complete = session.completed_cursorless_end();
        session.send_key(b"\r\r");
        ack(
            &mut session,
            complete,
            |s| s.contains(PLAN_TITLE) && s.contains("old diff rows retired"),
            "completed refused replay at current result",
        );
        assert_eq!(apply_count(&native), 1);
        assert_eq!(
            observed(&native),
            after,
            "repeated Enter cannot replay attempt or refresh raw index"
        );
        private_effects(&native, &session);
        session.quit_and_assert_restored_terminal(mouse);
    }
}
#[test]
fn native_mutable_plan_keyboard_external_worktree_change_refuses_confirm_and_explicit_reload() {
    stale_journey(false);
}
#[test]
fn native_mutable_plan_mouse_external_worktree_change_refuses_confirm_and_explicit_reload() {
    stale_journey(true);
}
fn stale_journey(mouse: bool) {
    let native = fixture("sha1");
    let mut session = launch(&native, mouse);
    session.wait_for_screen("Recent commits");
    selected(&native, &mut session, mouse, false);
    preview(&mut session, mouse);
    plan_ready(&mut session, "Stage selected lines");
    fs::write(
        native.fixture.repo().join("tracked"),
        b"external after preview\n",
    )
    .unwrap();
    let before = observed(&native);
    choose(&mut session, mouse, "[Enter Confirm]", b"\r");
    session.wait_for_cursorless_condition(
        |frame| {
            let s = frame.contents();
            s.contains(PLAN_TITLE)
                && s.contains("old diff rows retired")
                && s.contains("No automatic retry")
                && s.contains("[r Reload/inspect]")
                && !s.contains("[Enter Confirm]")
        },
        "owned stale confirmation diagnostic and retired rows",
        Duration::from_secs(8),
    );
    assert_eq!(apply_count(&native), 0);
    assert_eq!(observed(&native), before);
    private_effects(&native, &session);
    choose(&mut session, mouse, "[r Reload/inspect]", b"r");
    session.wait_for_cursorless_condition(
        |frame| {
            let s = frame.contents();
            !s.contains(PLAN_TITLE)
                && s.contains("external after preview")
                && s.contains("[v Native diff]")
        },
        "explicit fresh repository inspection after refused stale write",
        Duration::from_secs(8),
    );
    assert_eq!(apply_count(&native), 0);
    assert_eq!(observed(&native), before);
    private_effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
