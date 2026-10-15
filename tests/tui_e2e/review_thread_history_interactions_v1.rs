//! DECLARED / UNRUN. Actual held HistoryList source, real header resize and
//! disabled Open inputs. Existing protected restart/GH/SQL/Git/TTY journey
//! retains its original oracles; these controls never create a write intent.
use super::*;
use crate::support::owned_pty_processes_v1::ProcessIdentity;

fn original_child_live(session: &Session, original: &ProcessIdentity) {
    let current = process_identity(original.pid)
        .expect("Actual held source identity must be readable; unknown is not live/absent")
        .expect("Actual original HistoryList child must still be held before disabled input");
    assert_eq!(
        (current.pid, current.uid, current.birth, current.session),
        (original.pid, original.uid, original.birth, original.session),
    );
    assert!(!current.zombie);
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    assert!(owner.owns(&current));
}
fn completed_history(session: &mut Session, pending: bool) {
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, HISTORY).is_some_and(|body| {
                body.contains("[Enter Open]")
                    && body.contains("[Esc Back]")
                    && body.contains("[q Quit]")
                    && body.contains(if pending {
                        "Exact original history read pending"
                    } else {
                        "Inspect only"
                    })
                    && !body.contains("[Confirm action]")
                    && !body.contains("[External editor]")
                    && !body.contains("READ-ONLY PREVIEW")
                    && (!pending || !body.contains("Original native reply 界"))
            })
        },
        "Actual completed topmost history header/footer; no Form or publication controls",
        Duration::from_secs(8),
    );
}
fn disabled_open(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    pending: bool,
    expected_commands: usize,
    child: Option<&ProcessIdentity>,
) {
    completed_history(session, pending);
    if let Some(child) = child {
        original_child_live(session, child);
    }
    let before = session.completed_cursorless_screen().unwrap();
    let before_body = crate::support::overlay_body(before, HISTORY).unwrap();
    let protected = protected_sql(fixture);
    let unrelated = untouched_sql(fixture);
    let git = git_state(fixture);
    let trace = git_commands(fixture);
    let command_rows = commands(session, fixture);
    assert_eq!(command_rows.len(), expected_commands);
    let endpoint = session.completed_cursorless_end();
    if mouse {
        // Deliberately click the genuinely PAINTED disabled widget, not an
        // enabled hit or a guessed row. Pending/detail guards must ignore it.
        let point = session.wait_for_owned_completed_action(HISTORY, "[Enter Open]", &[]);
        session.click(point.0, point.1);
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_new_cursorless_condition(
        endpoint,
        |frame| {
            crate::support::overlay_body(frame, HISTORY).is_some_and(|body| body == before_body)
        },
        "Disabled actual Open input yields same owned history, not a new list/read/Form",
        Duration::from_secs(8),
    );
    if let Some(child) = child {
        original_child_live(session, child);
    }
    assert_eq!(commands(session, fixture), command_rows);
    assert_eq!(protected_sql(fixture), protected);
    assert_eq!(untouched_sql(fixture), unrelated);
    assert_eq!(git_state(fixture), git);
    assert_eq!(git_commands(fixture), trace);
    assert_read_only_git(fixture);
}
fn resize_owned_history(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    pending: bool,
    expected_commands: usize,
    child: Option<&ProcessIdentity>,
) {
    let original_size = session.completed_cursorless_screen().unwrap().size();
    assert!(matches!(original_size, (24, 80) | (40, 120)));
    let targets = if original_size == (24, 80) {
        [(120, 40), (80, 24)]
    } else {
        [(80, 24), (120, 40)]
    };
    for (columns, rows) in targets {
        if let Some(child) = child {
            original_child_live(session, child);
        }
        let protected = protected_sql(fixture);
        let unrelated = untouched_sql(fixture);
        let git = git_state(fixture);
        let trace = git_commands(fixture);
        let command_rows = commands(session, fixture);
        assert_eq!(command_rows.len(), expected_commands);
        assert_ne!(
            session.completed_cursorless_screen().unwrap().size(),
            (rows, columns)
        );
        session.resize_owned_header(columns, rows, HISTORY, &["[Enter Open]", "[Esc Back]"]);
        completed_history(session, pending);
        let frame = session.completed_cursorless_screen().unwrap();
        assert_eq!(frame.size(), (rows, columns));
        let body = crate::support::overlay_body(frame, HISTORY).unwrap();
        if pending {
            assert!(!body.contains("Recorded intent account"));
            assert!(!body.contains("Original native reply 界"));
        } else {
            // The fresh source header stays independent of historical target
            // metadata. Old H/ref/body remain exactly in the original SQL row.
            assert!(body.contains("Current H") && body.contains(&"a".repeat(40)));
            assert!(body.contains("repo #10000") && body.contains("PR #1007"));
            detail_contains(session, mouse, "Original native reply 界");
        }
        disabled_open(session, fixture, mouse, pending, expected_commands, child);
        assert_eq!(commands(session, fixture), command_rows);
        assert_eq!(protected_sql(fixture), protected);
        assert_eq!(untouched_sql(fixture), unrelated);
        assert_eq!(git_state(fixture), git);
        assert_eq!(git_commands(fixture), trace);
    }
    assert_eq!(
        session.completed_cursorless_screen().unwrap().size(),
        original_size
    );
}
pub(super) fn pending(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    expected_commands: usize,
    child: &ProcessIdentity,
) {
    completed_history(session, true);
    disabled_open(
        session,
        fixture,
        mouse,
        true,
        expected_commands,
        Some(child),
    );
    resize_owned_history(
        session,
        fixture,
        mouse,
        true,
        expected_commands,
        Some(child),
    );
}
pub(super) fn detail(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    expected_commands: usize,
) {
    completed_history(session, false);
    disabled_open(session, fixture, mouse, false, expected_commands, None);
    resize_owned_history(session, fixture, mouse, false, expected_commands, None);
    detail_contains(session, mouse, "Original native reply 界");
}
#[test]
fn review_threads_v1_history_keyboard_actual_pending_detail_disabled_open_and_owned_narrow_wide_resize_have_no_effects(
) {
    journey_with_interactions(false, Protected::Pending, false, false, false, true);
}
#[test]
fn review_threads_v1_history_mouse_actual_pending_detail_disabled_open_and_owned_narrow_wide_resize_have_no_effects(
) {
    journey_with_interactions(true, Protected::Pending, false, false, false, true);
}
