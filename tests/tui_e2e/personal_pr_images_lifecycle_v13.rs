//! The actual original child must be invoked before Cancel. Denied/current-UID
//! changes hide only the genuine original private page. No live network/write.
use super::*;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession};
#[derive(Clone, Copy)]
enum Outcome {
    Cancel,
    Denied,
    AccountChanged,
}
fn journey(mouse: bool) {
    for outcome in [Outcome::Cancel, Outcome::Denied, Outcome::AccountChanged] {
        let fixture = configured();
        let h = history::GitImagesV1::create(&fixture.repo(), false);
        let (mut steps, opened) = complete_steps();
        bind(&mut steps, &h);
        let files = steps.len();
        let mut image = phase(&h);
        match outcome {
            Outcome::Cancel => {
                image[4].delay_ms = 30_000;
                image.truncate(5);
            }
            Outcome::Denied => {
                image[2].status = 1;
                image[2].stdout.clear();
                image[2].stderr = b"gh: forbidden (HTTP 403)\n".to_vec();
                image.truncate(3);
            }
            Outcome::AccountChanged => {
                image.last_mut().unwrap().stdout =
                    serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
            }
        }
        steps.extend(image);
        fixture.scripted_gh(&steps);
        let paths = [
            ".git/HEAD",
            ".git/index",
            ".git/config",
            history::PATH,
            "base-only",
            "LOCAL_DIRTY_CANARY",
        ];
        let git = paths.map(|p| fs::read(fixture.repo().join(p)).unwrap());
        let refs = fixture.git(&["show-ref"]);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = Session::launch(&fixture, 120, 40);
        session.wait_for_screen("ghtui");
        refresh(&mut session, mouse);
        session.wait_for_screen("1 unique PRs across 3 queue matches");
        let before = session.completed_cursorless_end();
        if mouse {
            session.click_visible_action("Actual personal source")
        } else {
            session.send_key(b"\r")
        };
        frame(&mut session, before, |s| {
            s.contains(BODY) && crate::support::controls::text_has_control(s, "[f Files]")
        });
        journal(&mut session, &fixture, opened);
        let database = fixture.root.join("config/ghtui/cache.sqlite3");
        let cache = Cache::open(&database).unwrap();
        for uid in [42, 43] {
            cache
                .put_draft(
                    &Scope::github("github.invalid", Some(uid)).unwrap(),
                    "kept",
                    &format!("PRIVATE_{uid}"),
                )
                .unwrap();
        }
        ghtui::pr_write_journal::PrWriteJournal::open(&database).unwrap();
        let before_sql = sql(&fixture);
        let before = session.completed_cursorless_end();
        choose(&mut session, mouse, "[f Files]", b"f");
        frame(&mut session, before, |s| {
            s.contains("Personal PR Files") && s.contains("File src/界.rs")
        });
        journal(&mut session, &fixture, files);
        let before = session.completed_cursorless_end();
        choose(&mut session, mouse, "[N Next file]", b"N");
        frame(&mut session, before, |s| s.contains("▌ File src/界.rs"));
        let before = session.completed_cursorless_end();
        choose(&mut session, mouse, "[v Native diff]", b"v");
        if matches!(outcome, Outcome::Cancel) {
            // Fake gh records its exact real child before delaying the compare
            // response; Cancel is not sent against a guessed pre-invocation frame.
            session.wait_for_command_count(&fixture, steps.len());
            let journal = session.inspect_command_journal(&fixture);
            let row: Value = serde_json::from_str(journal.lines().last().unwrap()).unwrap();
            let pid = i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
            assert!(pid > 0);
            let child = process_identity(pid)
                .unwrap()
                .expect("actual delayed compare child");
            let owned = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
            assert!(owned.owns(&child));
            assert!(!child.zombie);
            session.wait_for_control("[Esc Cancel]");
            let cancel_before = session.completed_cursorless_end();
            choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
            frame(&mut session, cancel_before, |s| {
                s.contains("File src/界.rs")
                    && crate::support::controls::text_has_control(s, "[v Native diff]")
                    && !crate::support::controls::text_has_control(s, "[Esc Cancel]")
                    && !s.contains("Native PR diff •")
                    && !s.contains("Immutable file images")
            });
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while let Some(current) = process_identity(pid).unwrap() {
                assert_eq!(
                    (current.uid, current.session, current.birth),
                    (child.uid, child.session, child.birth)
                );
                assert!(
                    std::time::Instant::now() < deadline,
                    "original owned compare child must be reaped"
                );
                session.poll_output(Duration::from_millis(10));
            }
            assert!(!session.screen().contains(BODY));
            let back_before = session.completed_cursorless_end();
            choose(&mut session, mouse, "[b Back]", b"b");
            frame(&mut session, back_before, |s| {
                s.contains(BODY) && crate::support::controls::text_has_control(s, "[p PR actions]")
            });
        } else {
            frame(&mut session, before, |s| {
                s.contains("private content hidden")
                    && !s.contains(BODY)
                    && !s.contains(REPO)
                    && !s.contains(&h.h)
                    && !s.contains("File src/界.rs")
                    && !s.contains("Immutable file images")
                    && !s.contains("Native PR diff •")
            });
        }
        journal(&mut session, &fixture, steps.len());
        assert_eq!(sql(&fixture), before_sql);
        assert!(!session.screen().contains("PRIVATE_42"));
        assert!(!session.screen().contains("PRIVATE_43"));
        session.quit_and_assert_restored_terminal(mouse);
        for (path, expected) in paths.into_iter().zip(git) {
            assert_eq!(fs::read(fixture.repo().join(path)).unwrap(), expected);
        }
        assert_eq!(fixture.git(&["show-ref"]), refs);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        assert_eq!(sql(&fixture), before_sql);
    }
}
#[test]
fn personal_pr_entry_v13_images_keyboard_actual_held_cancel_denied_and_changed_numeric_principal() {
    journey(false)
}
#[test]
fn personal_pr_entry_v13_images_mouse_actual_held_cancel_denied_and_changed_numeric_principal() {
    journey(true)
}
