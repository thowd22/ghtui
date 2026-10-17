//! Launch-checkout origin as the default Actions repository: no --repo, no
//! saved set. A busy history reads only the newest page and says so.
use super::*;

fn origin_fixture(origin: &str) -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("unchanged.txt", b"preserve this checkout\n", "initial");
    fixture.git(&["remote", "add", "origin", origin]);
    Config {
        host: "github.invalid".into(),
        avatars: false,
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    fixture
}

fn assert_checkout_preserved(fixture: &Fixture, head: &str, origin: &str) {
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert_eq!(fixture.git(&["remote"]).trim(), "origin");
    assert_eq!(fixture.git(&["remote", "get-url", "origin"]).trim(), origin);
}

fn origin_journey(mouse: bool) {
    let origin = "git@github.invalid:example/repo.git";
    let fixture = origin_fixture(origin);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        read("user", json!({"id":42,"login":"octocat"}), false),
        read(
            "repos/example/repo/actions/runs?per_page=100",
            json!({"total_count":451,"workflow_runs":[raw_run()]}),
            false,
        ),
    ]);
    let mut session = Session::launch(&fixture, 120, 40);
    // A detected origin is a default, not --repo: the local view keeps focus.
    session.assert_focused_view("Repository");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen("example/repo run 123 attempt 2");
    session.wait_for_screen("1 runs of 451 reported");
    session.wait_for_screen("Loaded 1 of 451 matching runs");
    let rows = journal(&mut session, &fixture, 3);
    assert!(rows
        .iter()
        .all(|row| !row["args"].to_string().contains("--paginate")));
    session.resize(80, 24);
    session.wait_for_screen("example/repo run 123");
    assert_checkout_preserved(&fixture, &head, origin);
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(session.inspect_command_journal(&fixture).lines().count(), 3);
}

fn other_host_journey(mouse: bool) {
    // Origin on a different host is never borrowed as a scope for this host.
    let origin = "git@github.com:example/repo.git";
    let fixture = origin_fixture(origin);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    fixture.scripted_gh(&[]);
    let mut session = Session::launch(&fixture, 120, 40);
    session.assert_focused_view("Repository");
    choose(&mut session, mouse, "Actions", b"4");
    session.wait_for_screen("Choose a repository scope before loading Actions.");
    session.wait_for_screen("Run ghtui inside a GitHub checkout");
    assert_eq!(session.inspect_command_journal(&fixture), "");
    assert_checkout_preserved(&fixture, &head, origin);
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(session.inspect_command_journal(&fixture), "");
}

#[test]
fn actions_origin_default_keyboard_detects_checkout_repository_and_reads_newest_page() {
    origin_journey(false)
}
#[test]
fn actions_origin_default_mouse_detects_checkout_repository_and_reads_newest_page() {
    origin_journey(true)
}
#[test]
fn actions_origin_default_keyboard_ignores_remote_on_another_host() {
    other_host_journey(false)
}
#[test]
fn actions_origin_default_mouse_ignores_remote_on_another_host() {
    other_host_journey(true)
}
