//! Genuine installed Git/SQLite/provider source; no invented closed report.
use super::*;
use crate::repository_commit_actor_native_v1::tests::Kit;
use crate::test_support::GhStep;
use crate::workspace::Workspace;
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
fn protocol(sha: &str) -> Vec<GhStep> {
    let mut steps = super::tests::protocol(sha);
    let repo: Value = serde_json::from_slice(&steps[9].stdout).unwrap();
    let summary = json!({"id":9009,"node_id":"PR_safe_9009","number":9,"title":"Original retained timeline source","body":(0..60).map(|i|format!("Original retained Summary line {i:03}")).collect::<Vec<_>>().join("\n"),"state":"open","merged":false,"user":{"id":301,"node_id":"USER_301","login":"actual-pr-author","type":"User"},"base":{"repo":repo,"sha":"a".repeat(40),"ref":"main"},"head":{"sha":"b".repeat(40),"ref":"feature"},"mergeable":true,"mergeable_state":"clean","url":"https://github.invalid/api/v3/repos/example/original/pulls/9","html_url":"https://github.invalid/example/original/pull/9"});
    steps[11] = get(
        &format!("repos/example/original/commits/{sha}/pulls?per_page=50&page=1"),
        json!([summary.clone()]),
    );
    let mut open = steps[7..12].to_vec();
    open.push(get("repos/example/original/pulls/9", summary.clone()));
    for path in [
        "issues/9/comments",
        "pulls/9/commits",
        "pulls/9/files",
        "pulls/9/reviews",
        "issues/9/timeline",
    ] {
        let rows = if path == "issues/9/timeline" {
            json!([[{"event":"commented","body":"x\n".repeat(70_000),"user":{"login":"not-cloned-author","type":"User"},"actor":{"login":"not-cloned-generator","type":"Bot"}}]])
        } else {
            json!([[]])
        };
        let mut step = get(&format!("repos/example/original/{path}?per_page=100"), rows);
        step.args.extend(["--paginate".into(), "--slurp".into()]);
        open.push(step);
    }
    open.push(get("repos/example/original/pulls/9", summary.clone()));
    open.push(get("repos/example/original/pulls/9", summary));
    open.extend(steps[12..15].iter().cloned());
    assert_eq!(open.len(), 16);
    steps.extend(open);
    steps
}
fn current(format: &str) -> (App, Kit) {
    let mut kit = Kit::new(format, false, 42);
    kit.fixture.scripted_gh(&protocol(&kit.sha));
    let mut app = App {
        view: View::Repository,
        workspace: Some(std::mem::replace(
            &mut kit.workspace,
            Workspace::load(
                &kit.fixture.repo(),
                crate::workspace::RepositoryMode::Graph,
                &kit.sha,
                None,
            )
            .unwrap()
            .unwrap(),
        )),
        local_commit_actors: std::mem::take(&mut kit.state),
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
    assert!(accept(&mut app, &owner, result));
    dispatch(&mut app, RelatedPrActionV1::Open(0));
    app.related_pr_requested_v1 = false;
    let owner = begin(&mut app, Default::default()).unwrap();
    let result = read(
        &owner,
        &kit.provider,
        &kit.fixture.root.join("native.sqlite3"),
    )
    .unwrap();
    assert!(accept(&mut app, &owner, result));
    assert_eq!(owner.operation.accounting().unwrap().children_started(), 2);
    assert!(owner.operation.accounting().unwrap().sealed());
    (app, kit)
}
#[test]
fn timeline_actor_v1_related_genuine_sha1_sha256_cap_retains_same_closed_source_rows_focus_scroll()
{
    for format in ["sha1", "sha256"] {
        let (mut app, kit) = current(format);
        let before = kit.children();
        let state = app.related_pr_v1.as_mut().unwrap();
        state.scroll = 20;
        state.focus = 3;
        let original = state.snapshot.as_ref().unwrap().clone();
        let profile = state.profile.clone();
        let choice = state.choice.clone();
        let navigation = state.original_navigation.clone();
        let page = state.viewer.as_ref().unwrap().pr.as_ref().unwrap();
        assert!(page.rows.len() > 40);
        let rows = page.rows.as_ptr();
        let row20 = page.rows[20].label.clone();
        let body = page.details.timeline[0]["body"].as_str().unwrap().as_ptr();
        let count = page.rows.len();
        dispatch(&mut app, RelatedPrActionV1::Tab(PrTab::Timeline));
        let state = app.related_pr_v1.as_ref().unwrap();
        let page = state.viewer.as_ref().unwrap().pr.as_ref().unwrap();
        assert_eq!(page.tab, PrTab::Summary);
        assert_eq!(page.rows.as_ptr(), rows);
        assert_eq!(page.rows.len(), count);
        assert_eq!(page.rows[20].label, row20);
        assert_eq!(
            page.details.timeline[0]["body"].as_str().unwrap().as_ptr(),
            body
        );
        assert_eq!((state.scroll, state.focus, state.selected), (20, 3, 0));
        assert!(Arc::ptr_eq(state.snapshot.as_ref().unwrap(), &original));
        assert!(Arc::ptr_eq(&state.profile, &profile));
        assert!(Arc::ptr_eq(&state.choice, &choice));
        assert!(Arc::ptr_eq(&state.original_navigation, &navigation));
        assert!(state.status.contains("Timeline display limit reached"));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
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
            .map(|c| c.symbol())
            .collect();
        assert!(painted.contains(&row20));
        assert!(painted.contains("Timeline display limit reached"));
        assert!(!painted.contains("not-cloned-author"));
        assert_eq!(kit.children(), before);
        kit.assert_reads(31);
        assert!(!kit.fixture.root.join("actor-curl-unexpected").exists());
        assert_eq!(
            fs::read(kit.fixture.repo().join("tracked")).unwrap(),
            b"dirty\n"
        );
        assert_eq!(
            fs::read(kit.fixture.repo().join("scratch")).unwrap(),
            b"untracked\n"
        );
    }
}
#[test]
fn timeline_actor_v1_related_display_refusal_does_not_retain_bodies_across_actual_failed_owner() {
    let (mut app, kit) = current("sha1");
    dispatch(&mut app, RelatedPrActionV1::Tab(PrTab::Timeline));
    assert!(app.related_pr_v1.as_ref().unwrap().viewer.is_some());
    dispatch(&mut app, RelatedPrActionV1::Discover);
    app.related_pr_requested_v1 = false;
    let owner = begin(&mut app, Default::default()).unwrap();
    assert!(owner.owns(&app));
    assert!(fail(&mut app, &owner));
    let state = app.related_pr_v1.as_ref().unwrap();
    assert!(state.boundary_failed);
    assert!(state.viewer.is_none());
    assert!(state.snapshot.is_none());
    assert!(state.status.contains("private repositories/bodies hidden"));
    assert!(!fail(&mut app, &owner));
    kit.assert_reads(31);
}
