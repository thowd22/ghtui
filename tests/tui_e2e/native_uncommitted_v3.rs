//! Genuine normal-entry all-tracked comparison, never a synthetic Working result.
use super::*;
const PALETTE: &str = "Commands · current context";
const SOURCE: &str = "Repository source identity • read-only";
const ALL_COMMAND: &str = "Show all tracked edits since saved HEAD (staged + unstaged)";

fn state(native: &Native) -> Vec<Vec<u8>> {
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
    assert_eq!(
        fs::read(f.repo().join(".git/index")).unwrap(),
        index,
        "independent observer cannot refresh index bytes"
    );
    result
}
fn effects(native: &Native, session: &Session, before: &[Vec<u8>]) {
    assert_eq!(state(native), before);
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
fn palette_entry(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[: Commands]", b":");
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session
            .visible_overlay_body(PALETTE)
            .is_some_and(|body| body.contains("Filter focused:") && body.contains("[Enter Choose]"))
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "actual owned palette with visible query caret"
        );
    }
    // This editor intentionally shows its caret; it must not be asserted with
    // a cursorless-frame barrier belonging to a different pane.
    session.paste("all tracked edits");
    let selected = format!("> {ALL_COMMAND}");
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.visible_overlay_body(PALETTE).is_some_and(|body| {
            body.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .contains(&selected)
        }) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "actual unique selected All tracked edits result, not query field"
        );
    }
    choose(session, mouse, "> Show all tracked edits", b"\r");
    session.wait_for_overlay_close(PALETTE);
}
fn open(session: &mut Session, mouse: bool, width: u16, unborn: bool) {
    let before = session.completed_cursorless_end();
    if width >= 120 {
        choose(session, mouse, "[e All edits]", b"e");
    } else {
        palette_entry(session, mouse);
    }
    ack(
        session,
        before,
        |screen| {
            screen.contains("[v Native diff]")
                && screen.contains("All uncommitted tracked edits compared to the saved version")
                && screen.contains("user dirty bytes")
                && (unborn || screen.contains("saved checkout tracked"))
                && !screen.contains("user staged bytes")
        },
        "actual loaded combined tracked comparison before Native",
    );
    choose(session, mouse, "[v Native diff]", b"v");
    ready(session, "user dirty bytes");
    assert!(session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains("all tracked edits"));
}
fn source(session: &mut Session, mouse: bool, unborn: bool, head: Option<&str>) {
    choose(session, mouse, "[I Source]", b"I");
    let mut bodies = String::new();
    for page in 0..14 {
        session.wait_for_cursorless_condition(
            |frame| {
                crate::support::overlay_body(frame, SOURCE)
                    .is_some_and(|body| body.contains("[Esc Close]"))
            },
            "completed original owned Source page",
            Duration::from_secs(8),
        );
        let body =
            crate::support::overlay_body(session.completed_cursorless_screen().unwrap(), SOURCE)
                .unwrap();
        bodies.push_str(&body);
        bodies.push(' ');
        let normalized = bodies.split_whitespace().collect::<Vec<_>>().join(" ");
        if normalized.contains("Uncommitted: all tracked edits since saved HEAD")
            && normalized.contains("Untracked files are separate and not included")
            && head.is_none_or(|head| normalized.contains(head))
            && (!unborn || normalized.contains("No saved HEAD comparison base observed"))
        {
            choose(session, mouse, "[Esc Close]", b"\x1b");
            session.wait_for_overlay_close(SOURCE);
            return;
        }
        assert!(
            page < 13,
            "full actual tracked scope/base explanation must be visible, not background text"
        );
        let before = session.completed_cursorless_end();
        choose(session, mouse, "[PgDn More]", b"\x1b[6~");
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            session.poll_output(Duration::from_millis(1));
            if session.completed_cursorless_end() > before
                && session.completed_cursorless_screen().is_some_and(|frame| {
                    crate::support::overlay_body(frame, SOURCE).is_some_and(|next| next != body)
                })
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "new completed Source body must actually advance, not acknowledge an older repaint"
            );
        }
    }
}
#[test]
fn native_uncommitted_keyboard_normal_entry_exact_tracked_scope_source_restore_and_disabled_write()
{
    journey(false);
}
#[test]
fn native_uncommitted_mouse_normal_entry_exact_tracked_scope_source_restore_and_disabled_write() {
    journey(true);
}
fn journey(mouse: bool) {
    for (width, format, unborn) in [
        (120, "sha256", false),
        (80, "sha1", false),
        (80, "sha256", true),
    ] {
        let native = fixture(format);
        if unborn {
            native
                .fixture
                .git(&["checkout", "--orphan", "native-uncommitted-unborn"]);
        }
        let before = state(&native);
        let head = if unborn {
            None
        } else {
            Some(
                String::from_utf8(git_bytes(&native.fixture, &["rev-parse", "HEAD"], None))
                    .unwrap()
                    .trim()
                    .to_owned(),
            )
        };
        let mut session = launch(&native, mouse);
        session.wait_for_cursorless_condition(
            |frame| {
                let screen = frame.contents();
                screen.contains("Local Git workspace ready")
                    && screen.contains("[o Overview]")
                    && screen.contains(if unborn {
                        "Untracked"
                    } else {
                        "Recent commits"
                    })
            },
            "actual initial local source admitted before mode input, not navigation shell",
            Duration::from_secs(8),
        );
        if width == 80 {
            session.resize(width, 24);
            session.wait_for_cursorless_condition(
                |frame| frame.size().1 == width && frame.contents().contains("[: Commands]"),
                "actual narrow root before All edits palette",
                Duration::from_secs(8),
            );
        }
        open(&mut session, mouse, width, unborn);
        let screen = session.completed_cursorless_screen().unwrap().contents();
        assert!(!screen.contains("user staged bytes"));
        assert!(!screen.contains("user untracked bytes"));
        assert!(screen.contains("Unsaved content: author unknown"));
        assert!(screen.contains("Unsaved content: committer unknown"));
        if !unborn {
            focus(&mut session, mouse, "saved checkout tracked");
        } else {
            choose(&mut session, mouse, "Exact:", b"/");
            session.paste("user dirty bytes");
            session.send_key(b"\r");
            ready(&mut session, "1 matches");
            let previous = session.completed_cursorless_end();
            choose(&mut session, mouse, "[n Match]", b"n");
            ack(
                &mut session,
                previous,
                |screen| {
                    screen
                        .lines()
                        .any(|line| line.starts_with("│>") && line.contains("user dirty bytes"))
                },
                "exact original unborn added-line match selected",
            );
        }
        choose(&mut session, mouse, "[h Hunk]", b"h");
        ready(
            &mut session,
            if unborn {
                "Selected 1 (hidden 0)"
            } else {
                "Selected 2 (hidden 0)"
            },
        );
        let no_preview = starts(&native).len();
        if mouse {
            choose(&mut session, true, "[o Options]", b"o");
            session.wait_for_cursorless_condition(
                |frame| {
                    frame.contents().contains(LOCAL_TITLE)
                        && frame.contents().contains("[a Preview selected]")
                },
                "actual visible combined preview control before refusal",
                Duration::from_secs(8),
            );
        }
        choose(&mut session, mouse, "[a Preview selected]", b"a");
        session.wait_for_cursorless_condition(
            |frame| {
                frame.contents().contains(LOCAL_TITLE)
                    && frame.contents().contains("no inferred index direction")
                    && !frame.contents().contains("Selected patch • confirm")
            },
            "explicit all-tracked disabled writer with no fake plan",
            Duration::from_secs(8),
        );
        assert_eq!(
            starts(&native).len(),
            no_preview,
            "disabled combined writer must perform zeroGit"
        );
        if mouse {
            let previous = session.completed_cursorless_end();
            choose(&mut session, true, "[o Back to diff]", b"o");
            ack(
                &mut session,
                previous,
                |screen| {
                    screen.contains(LOCAL_TITLE)
                        && screen.contains("File 1/1:")
                        && screen.contains("[o Options]")
                        && screen.contains("no inferred index direction")
                        && !screen.contains("[o Back to diff]")
                },
                "actual native diff body after disabled Options before Source",
            );
        }
        let identity_reads = starts(&native).len();
        source(&mut session, mouse, unborn, head.as_deref());
        assert_eq!(
            starts(&native).len(),
            identity_reads,
            "prepared exact source explanation is pure"
        );
        let selected = if unborn {
            "Selected 1 (hidden 0)"
        } else {
            "Selected 2 (hidden 0)"
        };
        session.wait_for_cursorless_condition(
            |frame| {
                let screen = frame.contents();
                screen.contains(LOCAL_TITLE)
                    && screen.contains("all tracked edits")
                    && screen.contains(selected)
                    && screen.contains("no inferred index direction")
                    && screen.contains("[o Options]")
                    && !screen.contains(SOURCE)
                    && !screen.contains("NOT CURRENT")
            },
            "exact current original range after pure Source Close retains disabled explanation",
            Duration::from_secs(8),
        );
        session.resize_owned_modal(40, 24, LOCAL_TITLE, &["[Esc Close]", "[o Options]"]);
        ready(&mut session, selected);
        session.resize_owned_modal(
            width,
            if width == 80 { 24 } else { 40 },
            LOCAL_TITLE,
            &["[Esc Close]", "[o Options]"],
        );
        ready(&mut session, selected);
        effects(&native, &session, &before);
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        session.wait_for_overlay_close(LOCAL_TITLE);
        if !unborn {
            let previous = session.completed_cursorless_end();
            choose(&mut session, mouse, "[o Overview]", b"o");
            ack(
                &mut session,
                previous,
                |screen| screen.contains("Recent commits"),
                "actual Overview before original combined Back",
            );
            let previous = session.completed_cursorless_end();
            choose(&mut session, mouse, "[Back]", b"\x1b[1;3D");
            ack(
                &mut session,
                previous,
                |screen| {
                    screen.contains("All uncommitted tracked edits compared to the saved version")
                        && screen.contains("saved checkout tracked")
                        && screen.contains("user dirty bytes")
                        && !screen.contains("user staged bytes")
                },
                "restored original all-tracked kind/fingerprint, not Working relabel",
            );
        }
        effects(&native, &session, &before);
        session.quit_and_assert_restored_terminal(mouse);
    }
}
