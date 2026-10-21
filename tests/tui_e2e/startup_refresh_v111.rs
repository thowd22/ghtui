//! TASK-111: a Refresh pressed or clicked while the startup Inbox load still runs queues
//! behind that work and succeeds, instead of failing with `database is locked`.
use super::{configured, identity, initial, journal, refresh};
use crate::support::{GhStep, Session};
use ghtui::storage::{Cache, Scope};

#[test]
fn startup_refresh_v111_keyboard_refresh_during_startup_queues_behind_it_and_loads() {
    startup_refresh(false)
}

#[test]
fn startup_refresh_v111_mouse_refresh_during_startup_queues_behind_it_and_loads() {
    startup_refresh(true)
}

fn startup_refresh(mouse: bool) {
    let binary = std::path::Path::new(env!("CARGO_BIN_EXE_ghtui"));
    let fixture = configured();
    // The startup Inbox load is still waiting on its first gh call when Refresh arrives.
    let mut startup: Vec<GhStep> = identity("octocat").into_iter().take(1).collect();
    startup[0].delay_ms = 1500;
    let mut steps = startup;
    steps.extend(initial(false));
    fixture.scripted_gh(&steps);
    let (cols, rows) = if mouse { (80, 24) } else { (120, 40) };
    // An empty value falls back to the config default: the Inbox loads at open.
    let mut session = Session::launch_binary(
        binary,
        &fixture,
        cols,
        rows,
        &[("GHTUI_INBOX_ON_START", "")],
    );
    session.wait_for_screen("Loading your authenticated GitHub inbox");
    session.wait_for_command_count(&fixture, 1);
    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    session.wait_for_screen("Fresh");
    journal(&mut session, &fixture, steps.len());
    let screen = session.screen();
    assert!(!screen.contains("database is locked"), "{screen}");
    assert!(!screen.contains("Error:"), "{screen}");
    // The startup call was cancelled and journalled once; the refresh then ran the whole load.
    let commands = session.inspect_command_journal(&fixture);
    assert_eq!(
        commands
            .lines()
            .filter(|line| line.contains("search/issues"))
            .count(),
        3,
        "{commands}"
    );
    // The refresh published the Inbox into the account cache.
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    assert_eq!(cache.list(&scope, Some("pr")).unwrap().len(), 3);
    drop(cache);
    session.quit_and_assert_restored_terminal(mouse);
}
