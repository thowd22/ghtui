//! Real-binary measured refresh journeys sharing the scoped refresh framework.
//! Both inputs verify complete three-repository reads and preserved selection.
use super::*;

const THIRD: &str = "example/refreshed0002";

#[test]
fn repo_catalog_measured_keyboard_three_repo_complete_refresh_and_original_selection() {
    three(false);
}
#[test]
fn repo_catalog_measured_mouse_three_repo_complete_refresh_and_original_selection() {
    three(true);
}
fn three(mouse: bool) {
    let canonical = [NEW_FIRST, NEW_SECOND, THIRD];
    let mut steps = setup_protocol(3);
    steps.extend(refresh_protocol(
        3,
        &canonical,
        "Three genuinely admitted repositories",
    ));
    steps.extend(refresh_protocol(
        3,
        &canonical,
        "Measured three-repository refresh",
    ));
    assert_eq!(
        steps.len(),
        60,
        "Source7+Save4+Activate7+Refresh21+Refresh21, not a truncated first/second repository"
    );
    let fixture = fixture(&steps);
    seed_old_named(&fixture, &canonical);
    let before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original = open_saved(&mut session, &fixture, mouse, 3);
    refresh(&mut session, mouse);
    session.wait_for_command_count(&fixture, 39);
    fresh(&mut session, 3, "Three genuinely admitted repositories");
    for name in canonical {
        assert!(
            session.screen().contains(name),
            "Every requested canonical repository has an actual observed row"
        );
    }
    exact(&session, &fixture, &steps[..39]);
    let initial = frame(&fixture, &canonical).unwrap();
    assert_eq!(initial.generation, 9);
    assert!(initial.complete);
    let rows = initial
        .records
        .iter()
        .filter(|row| row.kind == "pr")
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 3);
    assert!(rows
        .iter()
        .all(|row| row.payload["queues"].as_array().unwrap().len() == 3));
    assert_eq!(assert_intent(&fixture, &ids(3)), original);
    choose(&mut session, mouse, "[m Assigned to me]", b"m");
    ready(&mut session, &["Queue: Assigned to me"]);
    let selected = if mouse {
        NEW_FIRST
    } else {
        session.send_key(b"\x1b[B\x1b[B");
        THIRD
    };
    ready(&mut session, &[&format!("> {selected}#7")]);
    refresh(&mut session, mouse);
    session.wait_for_command_count(&fixture, 60);
    fresh(&mut session, 3, "Measured three-repository refresh");
    ready(&mut session, &["Queue: Assigned to me"]);
    choose(&mut session, mouse, "[m Assigned to me]", b"m");
    ready(&mut session, &[&format!("> {selected}#7")]);
    let current = frame(&fixture, &canonical).unwrap();
    assert_eq!(current.generation, 10);
    assert!(current.complete);
    assert_eq!(
        current
            .records
            .iter()
            .filter(|row| row.kind == "pr")
            .count(),
        3
    );
    assert!(current
        .records
        .iter()
        .filter(|row| row.kind == "pr")
        .all(|row| row.payload["title"] == "Measured three-repository refresh"));
    exact(&session, &fixture, &steps);
    let gh = session.inspect_command_journal(&fixture);
    let trace = git_commands(&fixture);
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    fresh(&mut session, 3, "Measured three-repository refresh");
    help_barrier(&mut session, mouse);
    assert_eq!(session.inspect_command_journal(&fixture), gh);
    assert_eq!(git_commands(&fixture), trace);
    choose(&mut session, mouse, "3 Search", b"3");
    local_search_ready(&mut session);
    assert_eq!(assert_intent(&fixture, &ids(3)), original);
    assert_eq!(session.inspect_command_journal(&fixture), gh);
    no_attempts(&fixture);
    finish(&mut session, &fixture, mouse, 60, &before, &config);
    exact(&session, &fixture, &steps);
}
