//! TASK-53: the loaded job log exports to an explicit new file by key (`x`) and by click,
//! after a read-only preview and explicit confirmation. No GitHub write, no Git change.
use super::*;

const LOG: &str = "matrix test linux\tUNKNOWN STEP\t2026-10-13T12:00:01.0000000Z Export first line\n\
matrix test linux\tUNKNOWN STEP\t2026-10-13T12:00:02.0000000Z ##[error]Process completed with exit code 101.\n";

#[test]
fn v1_actions_logs_keyboard_export_loaded_log_to_new_file() {
    export(false)
}
#[test]
fn v1_actions_logs_mouse_export_loaded_log_to_new_file() {
    export(true)
}

fn export(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let mut steps = initial();
    steps.extend(open_steps());
    steps.push(GhStep::read(
        &[
            "run",
            "view",
            "123",
            "--repo",
            "github.invalid/example/repo",
            "--attempt",
            "2",
            "--log",
            "--job",
            "501",
        ],
        LOG,
    ));
    let loaded = steps.len();
    steps.extend(compose_identity_steps());
    let opened = steps.len();
    steps.extend(current_account());
    steps.extend(current_account());
    let exported = steps.len();
    fixture.scripted_gh(&steps);
    let exports = fixture.root.join("exports");
    std::fs::create_dir_all(&exports).unwrap();
    let destination = exports.join("run-123-job-501.log");

    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen(&run_cell(123, 2));
    if mouse {
        session.click_visible_action(&run_cell(123, 2));
    } else {
        session.send_key(b"\r");
    }
    wait_row(&mut session, &["matrix test linux", "501"]);
    choose(&mut session, mouse, "[l Logs]", b"l");
    session.wait_for_screen("Export first line");
    session.wait_for_control("[x Export log]");
    journal(&mut session, &fixture, loaded);

    if mouse {
        command(&mut session, true, "[x Export log]");
    } else {
        session.send_key(b"x");
    }
    session.wait_for_screen("ACTIONS INTENT");
    session.wait_for_screen("New absolute destination (never overwrites)");
    session.paste(destination.to_str().unwrap());
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    session.wait_for_screen("destination: /");
    journal(&mut session, &fixture, opened);
    assert!(!destination.exists(), "preview never writes the file");
    confirm(&mut session, mouse, 1);
    session.wait_for_screen("Log for run 123 attempt 2 job 501 (2 lines) written to");
    let rows = journal(&mut session, &fixture, exported);
    // The file holds the same readable, sanitized log the view shows.
    let written = std::fs::read_to_string(&destination).unwrap();
    assert_eq!(written.lines().count(), 2, "{written}");
    assert!(written.contains("Export first line"));
    assert!(written.contains("##[error]Process completed with exit code 101."));
    assert_eq!(std::fs::read_dir(&exports).unwrap().count(), 1);
    assert!(rows
        .iter()
        .all(|row| row["args"].as_array().is_some_and(|args| !args
            .iter()
            .any(|arg| matches!(arg.as_str(), Some("POST" | "PUT" | "PATCH" | "DELETE"))))));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    session.resize(80, 24);
    session.wait_for_screen("Export first line");
    session.quit_and_assert_restored_terminal(mouse);
}
