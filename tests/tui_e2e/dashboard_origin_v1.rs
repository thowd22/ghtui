//! TASK-82 AC1 on the real executable: with no --repo, no saved set and no local mapping,
//! Dashboard is scoped to the launch checkout's origin on the configured host, and the
//! Repository view keeps local focus. Strict offline fake gh; keyboard and mouse.
use super::*;

const ORIGIN: &str = "git@github.invalid:example/app.git";

fn origin_fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "tracked.txt",
        b"original Dashboard fixture bytes\n",
        "Original Dashboard history",
    );
    fs::write(
        fixture.repo().join("tracked.txt"),
        b"local dirty Dashboard bytes\n",
    )
    .unwrap();
    fixture.git(&["remote", "add", "origin", ORIGIN]);
    Config {
        host: gh_fixture::HOST.into(),
        avatars: false,
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    fixture
}

fn origin_dashboard(mouse: bool) {
    let fixture = origin_fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    // Exactly the origin repository's inventory: any other repository would not match.
    let steps = refresh(&[APP]);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    // A detected origin is a default, not --repo: the local view keeps focus.
    session.assert_focused_view("Repository");
    start(&mut session, mouse);
    cards_ready(&mut session, 1, 1);
    journal(&mut session, &fixture, steps.len());
    assert!(!session.screen().contains("Dashboard scope needed"));
    session.resize(80, 24);
    wait_card_value(&mut session, "Needs my review", "1");
    exit(&mut session, mouse);
    journal(&mut session, &fixture, steps.len());
    assert_eq!(fixture.git(&["remote", "get-url", "origin"]).trim(), ORIGIN);
    metadata_only(&fixture);
    preserve(&fixture, &head, &index, &status);
}

#[test]
fn dashboards_origin_keyboard_scopes_dashboard_to_checkout_origin() {
    origin_dashboard(false)
}
#[test]
fn dashboards_origin_mouse_scopes_dashboard_to_checkout_origin() {
    origin_dashboard(true)
}

fn exit(session: &mut Session, mouse: bool) {
    session.quit_and_assert_restored_terminal(mouse);
}
