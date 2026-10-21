//! Real read retirement, accepted same-slot replacement and explicit cancel.
//! All scenarios preserve the original strict protocol and independent effects.
use super::*;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession, ProcessIdentity};

fn reservation_row(f: &Fixture) -> Vec<rusqlite::types::Value> {
    let scope = ghtui::storage::Scope::github("github.invalid", Some(42)).unwrap();
    // Independent DATA-effect oracle only. This fixed key never becomes a
    // Source, Current, permission, generation witness or provider argument.
    let encoded =
        serde_json::to_vec(&(&scope, 410u64, "R_metadata_410", 1007u64, NODE, 7u64)).unwrap();
    let source = format!("personal-pr-source-v13/{:x}", Sha256::digest(encoded));
    let db = Connection::open_with_flags(cache_path(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    db.query_row("SELECT host,login,source,generation,active FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",params![scope.host,scope.login,source],|row|(0..5).map(|i|row.get(i)).collect()).unwrap()
}
fn expected_read_sql(
    before: &[String],
    reservation: &[rusqlite::types::Value],
    prepared_sections: i64,
) -> Vec<String> {
    use rusqlite::types::Value as Sql;
    assert_eq!(reservation.len(), 5);
    assert_eq!(reservation[4], Sql::Integer(1));
    let Sql::Integer(generation) = reservation[3] else {
        panic!("Canonical reservation generation INTEGER")
    };
    assert!(generation > 0);
    let old = format!("sync_reservations:{reservation:?}");
    assert_eq!(before.iter().filter(|row| *row == &old).count(), 1);
    let mut next = reservation.to_vec();
    next[3] = Sql::Integer(generation.checked_add(prepared_sections).unwrap());
    let new = format!("sync_reservations:{next:?}");
    let mut result = before
        .iter()
        .map(|row| {
            if row == &old {
                new.clone()
            } else {
                row.clone()
            }
        })
        .collect::<Vec<_>>();
    result.sort();
    result
}
fn full_sql(f: &Fixture) -> Vec<String> {
    let mut rows = complete_current_sql_v65(f);
    rows.sort();
    rows
}
fn schema(f: &Fixture) -> Vec<(String, String)> {
    let db = Connection::open_with_flags(cache_path(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    let mut query = db
        .prepare("SELECT name,COALESCE(sql,'') FROM sqlite_master ORDER BY name")
        .unwrap();
    let rows = query
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}

fn summary(s: &mut Session, mouse: bool) {
    let before = s.completed_cursorless_end();
    choose(s, mouse, "[o Summary]", b"o");
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            crate::support::main_footer_visible(frame)
                && text.contains("feature → main")
                && text.contains(HEAD)
                && crate::support::controls::control_painted(frame, "[u Metadata]")
                && !crate::support::controls::control_painted(frame, "[Esc Cancel]")
        },
        "NEW genuine idle Summary before held Metadata request",
        Duration::from_secs(8),
    );
}
fn held(s: &mut Session, f: &Fixture, index: usize) -> ProcessIdentity {
    s.wait_for_command_count(f, index + 1);
    let record = records(s, f, index + 1).pop().unwrap();
    assert_eq!(record["step"], index);
    assert_eq!(record["matched"], true);
    let pid = i32::try_from(record["pid"].as_u64().unwrap()).unwrap();
    let identity = process_identity(pid).unwrap().unwrap();
    assert!(!identity.zombie);
    let session =
        OwnedPtySession::capture(u32::try_from(s.owned_application_pid()).unwrap()).unwrap();
    assert!(session.owns(&identity));
    identity
}
fn reaped(s: &mut Session, old: &ProcessIdentity) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        s.poll_output(Duration::from_millis(2));
        match process_identity(old.pid) {
            Ok(None) => return,
            Ok(Some(now)) => assert_eq!(
                (now.pid, now.uid, now.birth, now.session),
                (old.pid, old.uid, old.birth, old.session)
            ),
            #[cfg(target_os = "linux")]
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {}
            Err(error) => panic!("Exact owned child observation: {error}"),
        }
        assert!(
            Instant::now() < until,
            "Exact old child must disappear, not stay zombie"
        );
    }
}
fn no_busy(s: &mut Session, before: usize) {
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            crate::support::main_footer_visible(frame)
                && crate::support::controls::control_painted(frame, "[u Metadata]")
                && !crate::support::controls::control_painted(frame, "[Esc Cancel]")
        },
        "NEW old read slot retired without replay or stale page install",
        Duration::from_secs(8),
    );
}
fn journey(mouse: bool, mode: u8) {
    let f = fixture();
    let raw = canonical(target(
        "Native collaboration",
        "A fixture description",
        "main",
    ));
    let mut steps = startup(&raw);
    let startup_end = steps.len();
    let mut pending = section(&raw);
    pending.last_mut().unwrap().delay_ms = 30_000;
    steps.extend(pending);
    let first_end = steps.len();
    let replacement_end = if mode == 1 {
        // Existing genuine Personal reconcile_page: current identity2 then
        // actual pr_details7, not a replacement source/receipt/DTO factory.
        let mut replacement = principal();
        replacement.extend(at_target(details(vec![starter()]), &raw));
        assert_eq!(replacement.len(), 9);
        replacement.last_mut().unwrap().delay_ms = 2000;
        steps.extend(replacement);
        steps.len()
    } else {
        first_end
    };
    steps.extend(section(&raw));
    f.scripted_gh(&steps);
    let env = if mouse {
        vec![("NO_COLOR", "1")]
    } else {
        Vec::new()
    };
    let mut s = Session::launch_binary(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &f,
        if mouse { 80 } else { 120 },
        if mouse { 24 } else { 40 },
        &env,
    );
    enter_metadata(&mut s, mouse);
    exact_current(&mut s, &f, &steps, startup_end);
    let before_sql = full_sql(&f);
    let before_reservation = reservation_row(&f);
    let before_schema = schema(&f);
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    summary(&mut s, mouse);
    choose(&mut s, mouse, "[u Metadata]", b"u");
    let old = held(&mut s, &f, first_end - 1);
    s.wait_for_owned_completed_action("Inbox • focused view ─", " u Metadata ", &[]);
    s.wait_for_screen("Loading PR Metadata");
    s.wait_for_control("[Esc Cancel]");
    match mode {
        0 => {
            let before = s.completed_cursorless_end();
            // Same-page scrolling is harmless (covered by the paired
            // viewport journeys). A real newer tab must still retire this
            // exact held read without installing its late Metadata result.
            choose(&mut s, mouse, "[h Commits]", b"h");
            s.wait_for_new_cursorless_condition(
                before,
                |frame| {
                    let text = frame.contents();
                    crate::support::main_footer_visible(frame)
                        && text.contains(
                            "Commit = a saved version • author writes, committer records it",
                        )
                        && !text.contains("Your repository permissions:")
                        && !crate::support::controls::control_painted(frame, "[Esc Cancel]")
                },
                "NEW actual Commits tab owns the page, not late Metadata",
                Duration::from_secs(8),
            );
            reaped(&mut s, &old);
            assert!(!s.screen().contains("Loading PR Metadata"));
            // Real other-view navigation stays usable after retiring Inbox.
            choose(&mut s, mouse, "2 Repository", b"2");
            s.wait_for_cursorless_condition(
                |frame| {
                    crate::support::main_footer_visible(frame)
                        && frame.contents().contains("Repository • focused view")
                },
                "Actual Repository after retired Inbox read",
                Duration::from_secs(8),
            );
            choose(&mut s, mouse, "1 Inbox", b"1");
            s.wait_for_cursorless_condition(
                |frame| {
                    crate::support::main_footer_visible(frame)
                        && crate::support::controls::control_painted(frame, "[u Metadata]")
                        && !crate::support::controls::control_painted(frame, "[Esc Cancel]")
                },
                "Actual cached original Inbox after Repository return",
                Duration::from_secs(8),
            );
            // Metadata's fresh read starts from the actual Summary page,
            // never a synthetic tab or a fallback source reconstructed from
            // the retired worker's result.
            summary(&mut s, mouse);
        }
        1 => {
            let before = s.completed_cursorless_end();
            // While the read is held, `Esc Cancel` joins the key bar and `r Refresh` stays on
            // it: both the key and a click replace the held read.
            choose(&mut s, mouse, "[r Refresh]", b"r");
            let next = held(&mut s, &f, replacement_end - 1);
            assert_ne!(next.pid, old.pid);
            reaped(&mut s, &old);
            let now = process_identity(next.pid).unwrap().unwrap();
            assert_eq!(
                (now.uid, now.birth, now.session),
                (next.uid, next.birth, next.session)
            );
            assert!(
                !now.zombie,
                "Old retirement cannot cancel the live replacement"
            );
            s.wait_for_new_cursorless_condition(
                before,
                |frame| {
                    let text = frame.contents();
                    crate::support::main_footer_visible(frame)
                        && crate::support::controls::control_painted(frame, "[u Metadata]")
                        && text.contains("feature → main")
                        && text.contains(HEAD)
                        && text.contains("Fresh • fetched")
                        && !crate::support::controls::control_painted(frame, "[Esc Cancel]")
                },
                "NEW actual same-Inbox replacement completes despite late old result",
                Duration::from_secs(8),
            );
        }
        2 => {
            let before = s.completed_cursorless_end();
            choose(&mut s, mouse, "[Esc Cancel]", b"\x1b");
            no_busy(&mut s, before);
            reaped(&mut s, &old);
            s.wait_for_screen("Cancelled • r Refresh to retry");
        }
        _ => unreachable!(),
    }
    assert!(receipts(&f).is_empty());
    assert_eq!(
        full_sql(&f),
        expected_read_sql(&before_sql, &before_reservation, 1),
        "Read retirement/replacement cannot publish or change protected rows"
    );
    preserve_git(&f, &head, &index, &status);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    exact_current(&mut s, &f, &steps, replacement_end);
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[u Metadata]", b"u");
    s.wait_for_command_count(&f, steps.len());
    reopened_metadata_v70(&mut s, before, &raw);
    unchanged_metadata(&mut s, mouse);
    assert!(receipts(&f).is_empty());
    assert_eq!(
        full_sql(&f),
        expected_read_sql(&before_sql, &before_reservation, 2)
    );
    assert_eq!(schema(&f), before_schema);
    let commands = exact_current(&mut s, &f, &steps, steps.len());
    assert_eq!(mutation_count(&commands), 0);
    preserve_git(&f, &head, &index, &status);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_current(&mut s, &f, &steps, steps.len()), commands);
    preserve_git(&f, &head, &index, &status);
}
#[test]
fn pr_metadata_read_retirement_v70_keyboard_navigation_replacement_escape_and_fresh_metadata() {
    for mode in 0..3 {
        journey(false, mode)
    }
}
#[test]
fn pr_metadata_read_retirement_v70_mouse_navigation_replacement_escape_and_fresh_metadata() {
    for mode in 0..3 {
        journey(true, mode)
    }
}
