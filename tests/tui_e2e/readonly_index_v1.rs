//! Actual cached-apply stat-cache eligibility, never an observer-normalized index.
use super::*;
#[path = "../support/readonly_index_v1.rs"]
mod actual;
use crate::positive_pid_publication_v1 as positive_pid;
const LOCAL: &str = "Native local diff • observed snapshot";
#[track_caller]
fn ack(session: &mut Session, after: usize, mut predicate: impl FnMut(&str) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cursorless_end() > after
            && session
                .completed_cursorless_screen()
                .is_some_and(|f| predicate(&f.contents()))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "new completed exact read-only index frame"
        );
    }
}
fn ready(session: &mut Session, text: &str) {
    session.wait_for_cursorless_condition(
        |f| {
            let s = f.contents();
            s.contains(LOCAL) && s.contains("Native local diff ready") && s.contains(text)
        },
        "actual current mutable diff frame",
        Duration::from_secs(8),
    );
}
fn check(native: &Native, session: &Session, before: &actual::Snapshot) {
    assert_eq!(actual::snapshot(&native.fixture.repo()), *before);
    assert_eq!(sql(&native.fixture), native.sql);
    assert_eq!(
        fs::read(native.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        native.config
    );
    assert!(session.inspect_command_journal(&native.fixture).is_empty());
    assert!(fs::read(native.fixture.root.join("curl-journal"))
        .unwrap()
        .is_empty());
    let records = crate::raw_git_argv_v1::records(&native.fixture.root.join("raw-git-argv-v1"));
    assert!(!records.is_empty());
    for record in records {
        let prefix: [&[u8]; 6] = [
            b"--no-pager",
            b"--literal-pathspecs",
            b"-c",
            b"color.ui=false",
            b"-c",
            b"diff.autoRefreshIndex=false",
        ];
        assert!(record.argv.len() > prefix.len());
        assert!(
            record.argv[..6].iter().map(Vec::as_slice).eq(prefix),
            "exact read policy argv, not ignored options: {:?}",
            record.argv
        );
        assert!(!record.argv.iter().any(|a| matches!(
            a.as_slice(),
            b"add"
                | b"apply"
                | b"reset"
                | b"checkout"
                | b"switch"
                | b"commit"
                | b"fetch"
                | b"clone"
                | b"push"
                | b"merge"
                | b"rebase"
                | b"update-index"
                | b"update-ref"
        )));
    }
}
fn open(native: &Native, session: &mut Session, mouse: bool, staged: bool) {
    let before = session.completed_cursorless_end();
    choose(
        session,
        mouse,
        if staged { "[i Staged]" } else { "[w Working]" },
        if staged { b"i" } else { b"w" },
    );
    ack(session, before, |s| {
        s.contains("[v Native diff]")
            && s.contains(if staged {
                "staged eligible bytes"
            } else {
                "working tracked bytes"
            })
    });
    choose(session, mouse, "[v Native diff]", b"v");
    ready(
        session,
        if staged {
            "saved eligible bytes"
        } else {
            "staged tracked bytes"
        },
    );
    assert!(!session
        .screen()
        .contains("Repository changed while loading"));
    assert!(!session.screen().contains("Diff paths and patch disagree"));
    assert!(!starts(native).is_empty());
}
fn focus(session: &mut Session, mouse: bool, text: &str) {
    let selected = |s: &str| s.lines().any(|l| l.starts_with("│>") && l.contains(text));
    if mouse {
        let before = session.completed_cursorless_end();
        let (x, y) = session.locate(text).unwrap();
        session.click(x, y);
        ack(session, before, selected);
    } else {
        let before = session.completed_cursorless_end();
        session.send_key(b"\x1b[H");
        ack(session, before, |s| {
            s.contains(LOCAL) && s.contains("Native local diff ready")
        });
        for attempt in 0..16 {
            if selected(&session.completed_cursorless_screen().unwrap().contents()) {
                return;
            }
            assert!(attempt < 15, "genuine changed row must be selected");
            let before = session.completed_cursorless_end();
            session.send_key(b"\x1b[B");
            ack(session, before, |s| {
                s.contains(LOCAL) && s.contains("Native local diff ready")
            });
        }
    }
}
#[test]
fn readonly_index_keyboard_working_staged_native_query_range_resize_source_back_preserve_raw() {
    journey(false);
}
#[test]
fn readonly_index_mouse_working_staged_native_query_range_resize_source_back_preserve_raw() {
    journey(true);
}
fn journey(mouse: bool) {
    let native = fixture(if mouse { "sha1" } else { "sha256" });
    actual::prepare(
        &native.fixture.repo(),
        if mouse { "sha1" } else { "sha256" },
    );
    let control_before = actual::snapshot(&native.fixture.repo());
    actual::ordinary_diff(&native.fixture.repo());
    let mut control_after = actual::snapshot(&native.fixture.repo());
    assert_ne!(
        control_before.index, control_after.index,
        "actual PTY fixture must be stat-cache eligible"
    );
    control_after.index.clone_from(&control_before.index);
    assert_eq!(control_before, control_after);
    actual::make_eligible(&native.fixture.repo());
    let before = actual::snapshot(&native.fixture.repo());
    crate::raw_git_argv_v1::install(&native.fixture.root);
    let mut session = launch(&native, mouse);
    session.wait_for_screen("Recent commits");
    check(&native, &session, &before);
    for staged in [false, true] {
        open(&native, &mut session, mouse, staged);
        let old = if staged {
            "saved eligible bytes"
        } else {
            "staged tracked bytes"
        };
        focus(&mut session, mouse, old);
        choose(&mut session, mouse, "[h Hunk]", b"h");
        ready(&mut session, "Selected 2 (hidden 0)");
        session.resize_owned_modal(40, 24, LOCAL, &["[Esc Close]", "[o Options]"]);
        ready(&mut session, "Selected 2 (hidden 0)");
        session.resize_owned_modal(120, 40, LOCAL, &["[Esc Close]", "[o Options]"]);
        ready(&mut session, old);
        choose(&mut session, mouse, "Exact:", b"/");
        session.paste("bytes");
        session.send_key(b"\r");
        ready(&mut session, "2 matches");
        choose(&mut session, mouse, "[o Options]", b"o");
        let after = session.completed_cursorless_end();
        choose(&mut session, mouse, "[a Preview selected]", b"a");
        ack(&mut session, after, |s| {
            s.contains("Selected patch • confirm")
                && s.contains("Ready preview • no changes until explicit Confirm")
                && s.contains(if staged {
                    "Unstage selected lines"
                } else {
                    "Stage selected lines"
                })
                && s.contains("[Esc Back]")
                && s.contains("[Enter Confirm]")
        });
        // Preparing a genuine selected patch is still read-only. Do not
        // normalize the raw index or permit even a check/apply subprocess.
        check(&native, &session, &before);
        let after = session.completed_cursorless_end();
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
        ack(&mut session, after, |s| {
            s.contains(LOCAL)
                && s.contains(if staged {
                    "File 1/2: eligible → eligible"
                } else {
                    "File 1/1: tracked → tracked"
                })
                && s.contains("Selected 2 (hidden 0)")
                && s.contains("2 matches")
                && s.contains("[I Source]")
                && s.contains("Native local diff ready")
                && !s.contains("Selected patch • confirm")
                && !s.contains("[o Back to diff]")
        });
        choose(&mut session, mouse, "[I Source]", b"I");
        session.wait_for_cursorless_condition(
            |f| {
                let s = f.contents();
                s.contains("Repository source identity • read-only")
                    && s.contains(if staged {
                        "index vs HEAD; mutable"
                    } else {
                        "working files vs index; mutable"
                    })
            },
            "actual original mutable source",
            Duration::from_secs(8),
        );
        let after = session.completed_cursorless_end();
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        ack(&mut session, after, |s| {
            s.contains(LOCAL)
                && s.contains("Selected 2 (hidden 0)")
                && s.contains("Native local diff ready")
                && !s.contains("Repository source identity • read-only")
        });
        check(&native, &session, &before);
        let after = session.completed_cursorless_end();
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        ack(&mut session, after, |s| {
            s.contains("Native local diff closed")
                && !s.contains(LOCAL)
                && s.contains("[v Native diff]")
        });
        check(&native, &session, &before);
    }
    session.quit_and_assert_restored_terminal(mouse);
}

fn gate(native: &Native) {
    let program = crate::raw_git_argv_v1::install(&native.fixture.root);
    let data = crate::support::fixture_shell_publication_v1::data_path(&program);
    let script = fs::read(&data).unwrap();
    let marker = b"\nexec ";
    let positions = script
        .windows(marker.len())
        .enumerate()
        .filter_map(|(n, s)| (s == marker).then_some(n))
        .collect::<Vec<_>>();
    assert_eq!(positions.len(), 1);
    let root = native.fixture.root.to_str().unwrap();
    assert!(root.is_ascii() && !root.contains('\''));
    let held=format!("\nfor arg do\n if [ \"$arg\" = status ] && [ -f '{root}/index-arm' ] && mkdir '{root}/index-claimed' 2>/dev/null; then\n printf '%s\\n' \"$$\" > '{root}/index-pid-publishing' || exit 91\n mv '{root}/index-pid-publishing' '{root}/index-pid' || exit 92\n while [ ! -f '{root}/index-release' ]; do sleep 1; done\n fi\ndone\n");
    let mut combined = script[..positions[0]].to_vec();
    combined.extend_from_slice(held.as_bytes());
    combined.extend_from_slice(&script[positions[0]..]);
    fs::write(data, combined).unwrap();
}
fn held(session: &mut Session, native: &Native) -> ProcessIdentity {
    let owner =
        OwnedPtySession::capture(session.owned_application_pid().try_into().unwrap()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        match fs::read(native.fixture.root.join("index-pid")) {
            Ok(bytes) => {
                if let Some(pid) = positive_pid::complete_positive_pid(&bytes).unwrap() {
                    let identity = process_identity(pid)
                        .unwrap()
                        .expect("genuinely held Git wrapper");
                    assert!(owner.owns(&identity) && !identity.zombie);
                    return identity;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("actual gate PID publication: {e}"),
        }
        assert!(
            Instant::now() < deadline,
            "actual owned held Git child published completely"
        );
    }
}
#[test]
fn readonly_index_keyboard_cancel_owned_live_read_reaps_and_never_reopens() {
    cancel(false);
}
#[test]
fn readonly_index_mouse_cancel_owned_live_read_reaps_and_never_reopens() {
    cancel(true);
}
fn cancel(mouse: bool) {
    let native = fixture("sha256");
    actual::prepare(&native.fixture.repo(), "sha256");
    let before = actual::snapshot(&native.fixture.repo());
    gate(&native);
    let mut session = launch(&native, mouse);
    session.wait_for_screen("Recent commits");
    open(&native, &mut session, mouse, false);
    fs::write(
        native.fixture.root.join("index-arm"),
        b"hold actual next status\n",
    )
    .unwrap();
    choose(&mut session, mouse, "Exact:", b"/");
    session.paste("bytes");
    session.send_key(b"\r");
    let child = held(&mut session, &native);
    session.wait_for_cursorless_condition(
        |f| f.contents().contains(LOCAL) && f.contents().contains("NOT CURRENT"),
        "actual pending original native read",
        Duration::from_secs(8),
    );
    let after = session.completed_cursorless_end();
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    ack(&mut session, after, |s| {
        s.contains("Native local diff closed") && !s.contains(LOCAL)
    });
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let current = process_identity(child.pid).unwrap();
        if current.as_ref().is_none_or(|p| {
            p.birth != child.birth || p.uid != child.uid || p.session != child.session
        }) {
            break;
        }
        session.poll_output(Duration::from_millis(1));
        assert!(
            Instant::now() < deadline,
            "exact owned live original read reaped after cancellation"
        );
    }
    fs::write(
        native.fixture.root.join("index-release"),
        b"old read cannot reopen\n",
    )
    .unwrap();
    check(&native, &session, &before);
    assert!(!session.screen().contains(LOCAL));
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn readonly_index_keyboard_stale_current_projection_refuses_without_writes() {
    stale(false);
}
#[test]
fn readonly_index_mouse_stale_current_projection_refuses_without_writes() {
    stale(true);
}
fn stale(mouse: bool) {
    let native = fixture("sha1");
    actual::prepare(&native.fixture.repo(), "sha1");
    crate::raw_git_argv_v1::install(&native.fixture.root);
    let mut session = launch(&native, mouse);
    session.wait_for_screen("Recent commits");
    open(&native, &mut session, mouse, false);
    fs::write(
        native.fixture.repo().join("tracked"),
        b"independent user changed working bytes\n",
    )
    .unwrap();
    let before = actual::snapshot(&native.fixture.repo());
    choose(&mut session, mouse, "Exact:", b"/");
    session.paste("bytes");
    session.send_key(b"\r");
    session.wait_for_cursorless_condition(
        |f| {
            let s = f.contents();
            s.contains(LOCAL) && s.contains("NOT CURRENT") && s.contains("Mutable diff is stale")
        },
        "stale source refused before current rows/range authority",
        Duration::from_secs(8),
    );
    check(&native, &session, &before);
    let after = session.completed_cursorless_end();
    choose(&mut session, mouse, "[Esc Close]", b"\x1b");
    ack(&mut session, after, |s| {
        s.contains("Native local diff closed") && !s.contains(LOCAL)
    });
    check(&native, &session, &before);
    session.quit_and_assert_restored_terminal(mouse);
}
