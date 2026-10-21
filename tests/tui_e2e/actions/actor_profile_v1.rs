//! TASK-34 Actions run actors: on an open run, the explicit `@ Actor profile` control (key,
//! or a click on its bar token or Commands entry) opens one read-only notice with the
//! triggering actor (who started this attempt) and the run actor, each with the shared
//! GitHub profile: initials, @login, kind and host verification. The run's actor rows stay
//! read-only, and opening, scrolling, resizing and closing the profile reads nothing more
//! from gh and leaves the checkout untouched.
use super::{button, choose, fixture, initial, journal, open_steps, run_cell, wait_row};
use crate::support::Session;

fn open_profile(session: &mut Session, mouse: bool) {
    if mouse {
        button(session, "[@ Actor profile]");
    } else {
        session.send_key(b"@");
    }
    session.wait_for_screen("Actor profile • read-only");
}

fn close(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b");
    }
    session.wait_for_overlay_close("Actor profile • read-only");
}

fn journey(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let mut steps = initial();
    steps.extend(open_steps());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2))
    } else {
        session.send_key(b"\r")
    };
    wait_row(&mut session, &["matrix test linux", "501"]);
    session.wait_for_screen("triggering actor");
    session.wait_for_screen("@octocat");
    journal(&mut session, &fixture, steps.len());

    open_profile(&mut session, mouse);
    for text in [
        "Actor profile — Actions run actors",
        "Run 123 attempt 2 on example/repo",
        "Triggering actor (started attempt 2)",
        "[OC] @octocat",
        "Username: @octocat",
        "Kind: User account",
        "Verified: github.invalid reported this account on example/repo",
        "Signed-in account: @octocat (same login)",
    ] {
        session.wait_for_screen(text);
    }
    // The run actor sits below the first page of the notice.
    if mouse {
        session.click_visible_action("[PgDn More]");
    } else {
        session.send_key(b"\x1b[6~");
    }
    for text in [
        "Run actor (started the first attempt)",
        "[AL] @alice",
        "Username: @alice",
    ] {
        session.wait_for_screen(text);
    }
    session.resize(80, 24);
    session.wait_for_screen("Actor profile • read-only");
    close(&mut session, mouse);
    // At 80 columns the control may leave the bar; its key and Commands entry still work.
    open_profile(&mut session, mouse);
    session.wait_for_screen("Actor profile — Actions run actors");
    close(&mut session, mouse);
    session.wait_for_screen("triggering actor");

    // Read-only: no gh call beyond the scripted reads and no Git change.
    journal(&mut session, &fixture, steps.len());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actor_identity_keyboard_actions_run_triggering_and_run_actor_profiles() {
    journey(false);
}

#[test]
fn actor_identity_mouse_actions_run_triggering_and_run_actor_profiles() {
    journey(true);
}
