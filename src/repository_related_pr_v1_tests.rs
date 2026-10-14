use super::*;
use crate::repository_commit_actor_native_v1::tests::Kit;
use crate::test_support::GhStep;
use serde_json::{json, Value};
use std::fs;
fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        &value.to_string(),
    )
}
pub(crate) fn protocol(sha: &str) -> Vec<GhStep> {
    let owner =
        json!({"id":71,"node_id":"OWNER_native_71","login":"example","type":"Organization"});
    let repo = json!({"id":101,"node_id":"REPO_native_101","full_name":"example/original","owner":owner,"permissions":{"pull":true},"archived":false,"private":true});
    let mut steps = vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":42,"login":"octocat"})),
        get("orgs/example", owner.clone()),
        get(
            "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",
            json!([repo.clone()]),
        ),
        get("orgs/example", owner),
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":42,"login":"octocat"})),
    ];
    steps.extend(vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":42,"login":"changed-login"})),
        get("repos/example/original", repo.clone()),
        get(
            &format!("repos/example/original/commits/{sha}?per_page=1"),
            json!({"sha":sha}),
        ),
        get(
            &format!("repos/example/original/commits/{sha}/pulls?per_page=50&page=1"),
            json!([]),
        ),
        get("repos/example/original", repo),
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        get("user", json!({"id":42,"login":"octocat"})),
    ]);
    steps
}
#[test]
fn related_pr_native_genuine_operation_closed_observation_and_exact_workspace_return() {
    let kit = Kit::new("sha256", false, 42);
    kit.fixture.scripted_gh(&protocol(&kit.sha));
    let mut app = App {
        view: View::Repository,
        workspace: Some(kit.workspace),
        local_commit_actors: kit.state,
        ..Default::default()
    };
    let original = app
        .workspace
        .as_ref()
        .unwrap()
        .selected_commit
        .as_ref()
        .unwrap()
        .id
        .clone();
    let origin = app
        .workspace
        .as_ref()
        .unwrap()
        .actor_origin
        .as_ref()
        .unwrap()
        .clone();
    dispatch(&mut app, RelatedPrActionV1::Discover);
    app.related_pr_requested_v1 = false;
    let owner = begin(&mut app, Default::default()).unwrap();
    assert!(owner.owns(&app));
    let result = read(
        &owner,
        &kit.provider,
        &kit.fixture.root.join("native.sqlite3"),
    )
    .unwrap();
    assert!(result.matches(&owner));
    assert!(accept(&mut app, &owner, result));
    assert_eq!(owner.operation.accounting().unwrap().children_started(), 2);
    assert!(owner.operation.accounting().unwrap().sealed());
    dispatch(&mut app, RelatedPrActionV1::Back);
    assert!(app.related_pr_v1.is_none());
    let workspace = app.workspace.as_ref().unwrap();
    assert_eq!(workspace.selected_commit.as_ref().unwrap().id, original);
    assert!(Arc::ptr_eq(
        workspace.actor_origin.as_ref().unwrap(),
        &origin
    ));
    let rows = fs::read_to_string(kit.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(rows.lines().count(), 15);
    assert!(rows
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .all(|r| r["matched"] == true));
}
#[test]
fn related_pr_native_retired_owner_cannot_publish_success_error_or_old_private_body() {
    let kit = Kit::new("sha1", false, 42);
    kit.fixture.scripted_gh(&protocol(&kit.sha));
    let mut app = App {
        view: View::Repository,
        workspace: Some(kit.workspace),
        local_commit_actors: kit.state,
        ..Default::default()
    };
    dispatch(&mut app, RelatedPrActionV1::Discover);
    app.related_pr_requested_v1 = false;
    let old = begin(&mut app, Default::default()).unwrap();
    let result = read(
        &old,
        &kit.provider,
        &kit.fixture.root.join("native.sqlite3"),
    )
    .unwrap();
    let replacement = begin(&mut app, Default::default()).unwrap();
    assert!(!old.owns(&app));
    assert!(!accept(&mut app, &old, result));
    assert!(!fail(&mut app, &old));
    assert!(replacement.owns(&app));
    dispatch(&mut app, RelatedPrActionV1::Cancel);
    assert!(replacement.cancellation().is_cancelled());
    assert!(app.related_pr_v1.is_none());
}
#[test]
fn related_pr_native_resize_focus_click_and_keyboard_same_cancel_without_io() {
    let kit = Kit::new("sha1", false, 42);
    let mut app = App {
        view: View::Repository,
        workspace: Some(kit.workspace),
        local_commit_actors: kit.state,
        ..Default::default()
    };
    dispatch(&mut app, RelatedPrActionV1::Discover);
    app.related_pr_requested_v1 = false;
    let owner = begin(&mut app, Default::default()).unwrap();
    for width in [120, 80, 40] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, 24)).unwrap();
        terminal
            .draw(|f| {
                assert!(draw(f, &mut app));
            })
            .unwrap();
        let state = app.related_pr_v1.as_ref().unwrap();
        assert_eq!(state.hits.len(), 1);
        assert_eq!(state.hits[0].1, RelatedPrActionV1::Cancel);
        assert!(state.snapshot.is_none());
        assert!(state.viewer.is_none());
        assert!(event(&mut app, Event::Resize(width, 24)));
        assert!(app.hits.is_empty());
    }
    assert_eq!(owner.operation.accounting().unwrap().children_started(), 0);
    assert!(event(
        &mut app,
        Event::Key(crossterm::event::KeyEvent::new(
            KeyCode::Esc,
            KeyModifiers::NONE
        ))
    ));
    assert!(owner.cancellation().is_cancelled());
}

#[test]
fn related_pr_genuine_prepared_candidate_terminal_safe_bounded_label_and_shared_initials() {
    let kit = Kit::new("sha1", false, 42);
    let mut steps = protocol(&kit.sha);
    let title = format!(
        "Visible prefix\u{1b}]52;c;PRIVATE_OSC_PAYLOAD\u{7}\n{}",
        "🌈".repeat(16_000)
    );
    let mut repo =
        serde_json::from_str::<Value>(std::str::from_utf8(&steps[9].stdout).unwrap()).unwrap();
    // Reuse the actual original canonical repository payload from the genuine
    // strict protocol; no manually minted catalogue or closed-result token.
    repo["full_name"] = json!("example/original");
    steps[11] = get(
        &format!(
            "repos/example/original/commits/{}/pulls?per_page=50&page=1",
            kit.sha
        ),
        json!([{"id":9009,"node_id":"PR_safe_9009","number":9,"title":title,"body":"original","state":"open","merged":false,
            "user":{"id":301,"node_id":"BOT_301","login":"ci[bot]","type":"Bot"},"base":{"repo":repo},
            "url":"https://github.invalid/api/v3/repos/example/original/pulls/9","html_url":"https://github.invalid/example/original/pull/9"}]),
    );
    kit.fixture.scripted_gh(&steps);
    let mut app = App {
        view: View::Repository,
        workspace: Some(kit.workspace),
        local_commit_actors: kit.state,
        ..Default::default()
    };
    dispatch(&mut app, RelatedPrActionV1::Discover);
    app.related_pr_requested_v1 = false;
    let owner = begin(&mut app, Default::default()).unwrap();
    let result = read(
        &owner,
        &kit.provider,
        &kit.fixture.root.join("native.sqlite3"),
    )
    .unwrap();
    let candidate = &result.closed.candidates()[0];
    assert_eq!(candidate.database_id(), 9009);
    assert_eq!(
        candidate.pr().title,
        title,
        "Full observed original title is not rewritten into its display projection"
    );
    assert!(candidate.display_label().len() < 4096);
    assert!(candidate.display_label().contains('…'));
    assert!(candidate.display_label().chars().all(|ch| !ch.is_control()));
    assert!(!candidate.display_label().contains("PRIVATE_OSC_PAYLOAD"));
    assert!(accept(&mut app, &owner, result));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24)).unwrap();
    terminal
        .draw(|frame| {
            assert!(draw(frame, &mut app));
        })
        .unwrap();
    let painted: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(painted.contains("[CI]"));
    assert!(
        painted.contains("@ci[bot] [bot]"),
        "Username remains visible even when the full title needs a bounded projection"
    );
    assert!(painted.contains("Visible prefix"));
    assert!(!painted.contains("PRIVATE_OSC_PAYLOAD"));
    assert!(painted.chars().all(|ch| !ch.is_control()));
    assert!(app
        .related_pr_v1
        .as_ref()
        .unwrap()
        .snapshot
        .as_ref()
        .unwrap()
        .candidates()[0]
        .display_label()
        .contains("@ci[bot] [bot]"));
    assert_eq!(owner.operation.accounting().unwrap().children_started(), 2);
    let journal = fs::read_to_string(kit.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(journal.lines().count(), 15);
    assert!(!kit.fixture.root.join("actor-curl-unexpected").exists());
}
