//! Both inputs, actual saved and mutable providers, strict independent effects.
use super::*;
const SAVED: &str = "Native saved diff • original source";
const LOCAL: &str = "Native local diff • observed snapshot";
#[path = "native_full_side_failures_v3.rs"]
mod failures;
fn ack(session: &mut Session, after: usize, predicate: impl Fn(&str) -> bool, label: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cursorless_end() > after
            && session
                .completed_cursorless_screen()
                .is_some_and(|frame| predicate(&frame.contents()))
        {
            return;
        }
        assert!(Instant::now() < until, "new completed {label}");
    }
}
fn patch(session: &mut Session, title: &str, text: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            screen.contains(title)
                && screen.contains("Native ")
                && screen.contains("diff ready")
                && screen.contains(text)
        },
        "current original patch",
        Duration::from_secs(8),
    );
}
fn full(session: &mut Session, side: &str, text: &str) {
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            screen.contains(&format!("FULL {side} file"))
                && screen.contains("Full-file context ready")
                && screen.contains(text)
                && screen.contains("[b Back patch]")
        },
        "current exact full-side frame",
        Duration::from_secs(8),
    );
}
fn show(session: &mut Session, mouse: bool, side: &str) {
    choose(session, mouse, "[o Options]", b"o");
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains("[F Full Old]") && frame.contents().contains("[V Full New]")
        },
        "owned native Options full-side actions",
        Duration::from_secs(8),
    );
    choose(
        session,
        mouse,
        if side == "Old" {
            "[F Full Old]"
        } else {
            "[V Full New]"
        },
        if side == "Old" { b"F" } else { b"V" },
    );
}
fn selected(screen: &str, text: &str) -> bool {
    screen
        .lines()
        .any(|line| line.starts_with("│>") && line.contains(text))
}
fn focus(session: &mut Session, mouse: bool, text: &str) {
    let before = session.completed_cursorless_end();
    if mouse {
        let (x, y) = session.locate(text).expect("visible genuine changed row");
        session.click(x, y);
        ack(
            session,
            before,
            |screen| selected(screen, text),
            "actual mouse-selected changed row",
        );
        return;
    }
    let screen = session.completed_cursorless_screen().unwrap().contents();
    let file = screen
        .lines()
        .find_map(|line| {
            line.split_once("File 1/1: ")
                .map(|(_, rest)| rest.trim_end_matches('│').trim().to_owned())
        })
        .unwrap();
    session.send_key(b"\x1b[H");
    ack(
        session,
        before,
        |screen| selected(screen, &file),
        "actual original Home file header",
    );
    for _ in 0..24 {
        let screen = session.completed_cursorless_screen().unwrap().contents();
        if selected(&screen, text) {
            return;
        }
        let lines = screen.lines().collect::<Vec<_>>();
        let at = lines
            .iter()
            .position(|line| line.starts_with("│>"))
            .unwrap();
        let next = lines
            .get(at + 1)
            .unwrap()
            .trim_start_matches('│')
            .trim_end_matches('│')
            .trim()
            .to_owned();
        assert!(
            !next.is_empty() && !next.starts_with('['),
            "actual next original patch row"
        );
        let before = session.completed_cursorless_end();
        session.send_key(b"\x1b[B");
        ack(
            session,
            before,
            |screen| selected(screen, &next),
            "actual predicted next patch row",
        );
    }
    panic!("named original changed line not reached");
}
fn independent(native: &Native) -> Vec<Vec<u8>> {
    let f = &native.fixture;
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let result = vec![
        index.clone(),
        git_bytes(f, &["show-ref", "--head"], None),
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
    assert_eq!(fs::read(f.repo().join(".git/index")).unwrap(), index);
    result
}
fn effects(native: &Native, session: &Session, before: &[Vec<u8>]) {
    assert_eq!(independent(native), before);
    assert_eq!(sql(&native.fixture), native.sql);
    assert_eq!(
        fs::read(native.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        native.config
    );
    assert!(session.inspect_command_journal(&native.fixture).is_empty());
    assert!(fs::read(native.fixture.root.join("curl-journal"))
        .unwrap()
        .is_empty());
    assert!(!starts(native)
        .iter()
        .any(
            |event| event["argv"].as_array().unwrap().iter().any(|arg| matches!(
                arg.as_str(),
                Some(
                    "apply"
                        | "add"
                        | "reset"
                        | "checkout"
                        | "switch"
                        | "commit"
                        | "fetch"
                        | "push"
                        | "update-ref"
                )
            ))
        ));
}
#[test]
fn native_full_side_keyboard_saved_c_b_long_context_mutable_index_raw_back_and_readonly() {
    journey(false);
}
#[test]
fn native_full_side_mouse_saved_c_b_long_context_mutable_index_raw_back_and_readonly() {
    journey(true);
}
fn journey(mouse: bool) {
    saved(mouse);
    mutable(mouse);
}
fn saved(mouse: bool) {
    let mut native = fixture("sha256");
    let original = (1..=100)
        .map(|line| format!("line{line:03} unchanged context\n"))
        .collect::<String>();
    let changed = original.replace("line080 unchanged context", "line080 TARGET new bytes");
    native.c = commit(
        &native.fixture,
        &tree(
            &native.fixture,
            &[(b"context.rs", original.as_bytes(), "100644")],
        ),
        None,
        "Common",
    );
    native.a = commit(
        &native.fixture,
        &tree(
            &native.fixture,
            &[(
                b"context.rs",
                b"REQUESTED A must not substitute Old\n",
                "100644",
            )],
        ),
        Some(&native.c),
        "Requested",
    );
    native.b = commit(
        &native.fixture,
        &tree(
            &native.fixture,
            &[(b"context.rs", changed.as_bytes(), "100644")],
        ),
        Some(&native.c),
        "Target",
    );
    native
        .fixture
        .git(&["update-ref", "refs/heads/requested-A", &native.a]);
    native
        .fixture
        .git(&["update-ref", "refs/heads/target-B", &native.b]);
    native.fixture.git(&["tag", "-f", "common-C", &native.c]);
    let before = independent(&native);
    let mut session = launch(&native, mouse);
    open_comparison(&native, &mut session, mouse, true);
    session.choose_compact(mouse, "[v Native diff]", "[v Diff]");
    patch(&mut session, SAVED, "line080 TARGET new bytes");
    assert!(!session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains("line005 unchanged context"));
    focus(&mut session, mouse, "line080 TARGET new bytes");
    choose(&mut session, mouse, "[h Hunk]", b"h");
    patch(&mut session, SAVED, "Selected 2 (hidden 0)");
    let count = starts(&native).len();
    show(&mut session, mouse, "Old");
    full(&mut session, "Old", "line005 unchanged context");
    let scroll_before = session.completed_cursorless_end();
    if mouse {
        let (x, y) = session.locate("line005 unchanged context").unwrap();
        session.wheel(x, y, true);
    } else {
        session.send_key(b"\x1b[6~");
    }
    ack(
        &mut session,
        scroll_before,
        |screen| {
            screen.contains("Full-file context ready")
                && !screen.contains("line001 unchanged context")
                && screen.contains("line005 unchanged context")
        },
        "actual full-file wheel/PageDown scrolls while retaining read-only source",
    );
    let home = session.completed_cursorless_end();
    session.send_key(b"\x1b[H");
    ack(
        &mut session,
        home,
        |screen| selected(screen, "line001 unchanged context"),
        "actual full-file Home restores line1 focus before Source",
    );
    assert!(session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains(&native.c));
    assert!(!session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains("REQUESTED A must not"));
    let expected = git_bytes(
        &native.fixture,
        &["show", &format!("{}:context.rs", native.c)],
        None,
    );
    assert_eq!(expected, original.as_bytes());
    assert!(starts(&native).len() > count);
    assert!(starts(&native)[count..].iter().any(|event| event["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg.as_str() == Some("ls-tree"))));
    session.resize_owned_modal(80, 24, SAVED, &["[b Back patch]", "[o Options]"]);
    full(&mut session, "Old", "line005 unchanged context");
    let pure = starts(&native).len();
    let source_title = "Repository source identity • read-only";
    choose(&mut session, mouse, "[I Source]", b"I");
    session.wait_for_cursorless_condition(
        |frame| frame.contents().contains(source_title) && frame.contents().contains("[PgDn More]"),
        "owned Source overlay captures full-side shortcuts",
        Duration::from_secs(8),
    );
    let before_body =
        crate::support::overlay_body(session.completed_cursorless_screen().unwrap(), source_title)
            .unwrap();
    let previous = session.completed_cursorless_end();
    session.send_key(b"FVb");
    choose(&mut session, mouse, "[PgDn More]", b"\x1b[6~");
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cursorless_end() > previous
            && session
                .completed_cursorless_screen()
                .and_then(|frame| crate::support::overlay_body(frame, source_title))
                .is_some_and(|body| body != before_body)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "actual Source page change is the causal barrier after captured F/V/b"
        );
    }
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_overlay_close(source_title);
    full(&mut session, "Old", "line005 unchanged context");
    assert_eq!(
        starts(&native).len(),
        pure,
        "Source captured F/V/b and performed zero Git"
    );
    session.resize_owned_modal(120, 40, SAVED, &["[b Back patch]", "[o Options]"]);
    full(&mut session, "Old", "line005 unchanged context");
    let zero = starts(&native).len();
    choose(&mut session, mouse, "[o Options]", b"o");
    choose(&mut session, mouse, "[a Stage disabled]", b"a");
    choose(&mut session, mouse, "[o Back to file]", b"o");
    session.wait_for_screen("Full-file context is read-only");
    assert_eq!(starts(&native).len(), zero);
    let prior = session.completed_cursorless_end();
    choose(&mut session, mouse, "[V Full New]", b"V");
    ack(
        &mut session,
        prior,
        |screen| {
            screen.contains("FULL New file")
                && screen.contains("Full-file context ready")
                && screen.contains(&native.b)
        },
        "actual New saved endpoint",
    );
    full(&mut session, "New", "line005 unchanged context");
    if mouse {
        session.click_visible_action("Exact:");
    } else {
        session.send_key(b"/");
    }
    session.paste("line080");
    session.wait_for_screen("1 matching lines");
    if mouse {
        session.click_visible_action("[n Match]");
    } else {
        session.send_key(b"\r");
        session.send_key(b"n");
    }
    full(&mut session, "New", "line080 TARGET new bytes");
    assert!(selected(
        &session.completed_cursorless_screen().unwrap().contents(),
        "line080 TARGET new bytes"
    ));
    session.resize_owned_modal(40, 24, SAVED, &["[b Back patch]", "[o Options]"]);
    full(&mut session, "New", "line080 TARGET new bytes");
    session.resize_owned_modal(600, 40, SAVED, &["[b Back patch]", "[o Options]"]);
    full(&mut session, "New", "line080 TARGET new bytes");
    assert!(session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains("bounded 512-column pane"));
    let before_back = starts(&native).len();
    choose(&mut session, mouse, "[b Back patch]", b"b");
    patch(&mut session, SAVED, "Selected 2 (hidden 0)");
    assert_eq!(
        starts(&native).len(),
        before_back,
        "Saved Back reprojects same canonical source/range with zero Git"
    );
    assert!(selected(&session.completed_cursorless_screen().unwrap().contents(),"line080 TARGET new bytes"),"Back restores exact original New changed row even after different full-file search and width");
    effects(&native, &session, &before);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native saved diff closed");
    effects(&native, &session, &before);
    session.quit_and_assert_restored_terminal(mouse);
}
fn mutable(mouse: bool) {
    let native = fixture("sha1");
    let before = independent(&native);
    let mut session = launch(&native, mouse);
    session.wait_for_screen("Recent commits");
    choose(&mut session, mouse, "[w Working]", b"w");
    session.wait_for_cursorless_condition(
        |frame| {
            (frame.contents().contains("v Native diff") || frame.contents().contains("v Diff  "))
                && frame.contents().contains("user dirty bytes")
        },
        "actual loaded Working source",
        Duration::from_secs(8),
    );
    session.choose_compact(mouse, "[v Native diff]", "[v Diff]");
    patch(&mut session, LOCAL, "user dirty bytes");
    focus(&mut session, mouse, "user dirty bytes");
    choose(&mut session, mouse, "[h Hunk]", b"h");
    patch(&mut session, LOCAL, "Selected 2 (hidden 0)");
    show(&mut session, mouse, "Old");
    full(&mut session, "Old", "user staged bytes");
    assert!(session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains("Observed INDEX stage0 object"));
    choose(&mut session, mouse, "[V Full New]", b"V");
    full(&mut session, "New", "user dirty bytes");
    assert!(session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains("RAW UNSAVED"));
    if mouse {
        let (x, y) = session.locate("user dirty bytes").unwrap();
        session.click(x, y);
    } else {
        session.send_key(b"\x1b[H");
    }
    full(&mut session, "New", "user dirty bytes");
    let zero = starts(&native).len();
    choose(&mut session, mouse, "[o Options]", b"o");
    choose(&mut session, mouse, "[a Stage disabled]", b"a");
    choose(&mut session, mouse, "[o Back to file]", b"o");
    session.wait_for_screen("Full-file context is read-only");
    assert_eq!(starts(&native).len(), zero);
    assert!(!session.screen().contains("Confirm exact selected"));
    choose(&mut session, mouse, "[b Back patch]", b"b");
    patch(&mut session, LOCAL, "Selected 2 (hidden 0)");
    assert!(
        selected(
            &session.completed_cursorless_screen().unwrap().contents(),
            "user dirty bytes"
        ),
        "Back restores the original actual unsaved changed-row focus, not a full-file display row"
    );
    effects(&native, &session, &before);
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_screen("Native local diff closed");
    effects(&native, &session, &before);
    session.quit_and_assert_restored_terminal(mouse);
}
