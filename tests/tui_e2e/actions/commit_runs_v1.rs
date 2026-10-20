//! TASK-52: a History commit's `A Actions runs` opens the shared Actions view on the runs of
//! that exact commit. The run list is read with `head_sha=<full SHA>` only; the view names the
//! full SHA and each run's event and attempt. By key (`A`) and by click (the detail pane's
//! `A actions runs` token), at 140x42 and again at 80x24. The checkout never changes.
use super::*;

const ORIGIN: &str = "git@github.invalid:example/repo.git";

fn commit_fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("unchanged.txt", b"preserve this checkout\n", "initial");
    fixture.git(&["remote", "add", "origin", ORIGIN]);
    Config {
        host: "github.invalid".into(),
        avatars: false,
        inbox_on_start: false,
        refresh_seconds: 3600,
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    fixture
}

fn commit_run(head: &str) -> Value {
    json!({"id":123,"workflow_id":7,"name":"CI matrix","display_title":"initial","head_sha":head,"head_branch":"main","event":"push","status":"completed","conclusion":"success","run_attempt":2,"created_at":"2026-10-13T12:00:00Z","updated_at":"2026-10-13T12:04:00Z","actor":{"login":"alice","type":"User"},"triggering_actor":{"login":"octocat","type":"User"}})
}

fn commit_runs(mouse: bool) {
    let fixture = commit_fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_eq!(head.len(), 40);
    let endpoint = format!("repos/example/repo/actions/runs?per_page=100&head_sha={head}");
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        read(
            &endpoint,
            json!({"total_count":1,"workflow_runs":[commit_run(&head)]}),
            false,
        ),
    ]);
    let mut session = Session::launch(&fixture, 140, 42);
    session.wait_for_screen("Recent commits");
    session.choose(mouse, "[g History]", b"g");
    session.wait_for_history_ready();
    session.wait_for_settled_footer();
    session.wait_for_screen("A actions runs");
    // Browsing History reads nothing from GitHub.
    assert_eq!(session.inspect_command_journal(&fixture), "");
    if mouse {
        let (x, y) = session
            .locate("A actions runs")
            .expect("the detail pane paints the actions runs token");
        session.click(x, y);
    } else {
        session.send_key(b"A");
    }
    session.wait_for_screen(&format!("Commit {head}"));
    session.wait_for_screen("1 runs of 1 reported • complete");
    wait_row(&mut session, &["main", "push"]);
    let rows = journal(&mut session, &fixture, 3);
    assert_eq!(rows[2]["args"], json!(api(&endpoint)));
    // The full-width list keeps the RUN cell with the attempt.
    session.resize(80, 24);
    session.wait_for_screen(&format!("Commit {head}"));
    wait_row(&mut session, &["push", &run_cell(123, 2)]);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert_eq!(fixture.git(&["remote", "get-url", "origin"]).trim(), ORIGIN);
    assert_eq!(
        std::fs::read(fixture.repo().join("unchanged.txt")).unwrap(),
        b"preserve this checkout\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(session.inspect_command_journal(&fixture).lines().count(), 3);
}

#[test]
fn actions_commit_runs_keyboard_history_commit_opens_exact_sha_runs() {
    commit_runs(false)
}
#[test]
fn actions_commit_runs_mouse_history_commit_opens_exact_sha_runs() {
    commit_runs(true)
}
