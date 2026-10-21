//! SOURCE-ONLY future child of scoped_pr_read_v1::tests; every contract obtains
//! genuine Source7 + selected PR13 before the separately counted resource read.
use super::*;
#[path = "scoped_pr_immutable_images_v1_tests.rs"]
mod immutable_images;
#[path = "scoped_pr_threads_v1_tests.rs"]
mod review_threads;
#[path = "scoped_pr_checks_v1_tests.rs"]
mod scoped_checks;
use crate::providers::gh::{
    PrResourceKindV1, PrResourceReadLimitsV1, RepositoryCatalogueAccessLimits,
};
use crate::scoped_pr_observed_tabs_v1::resources::*;
use crate::theme::SemanticToken;

const PATCH:&[u8]=b"diff --git a/src/view.rs b/src/view.rs\n--- a/src/view.rs\n+++ b/src/view.rs\n@@ -1,2 +1,2 @@\n context\n-old\n+new\n";
fn resource_parent(name: &str) -> Value {
    let mut raw = summary(name);
    raw["url"] = json!(format!("https://{HOST}/api/v3/repos/{name}/pulls/7"));
    raw
}
fn check_value(id: u64) -> Value {
    json!({"id":id,"node_id":format!("CHECK_{id}"),"name":format!("Build{id}"),"head_sha":"a".repeat(40),"status":"completed","conclusion":"failure","check_suite":{"id":901},"app":{"id":92},"user":{"id":54,"login":"ci-actor","type":"User"},"started_at":"2026-10-14T00:00:00Z","completed_at":null})
}
fn check_page(rows: Vec<Value>, total: u64) -> Value {
    json!({"total_count":total,"check_runs":rows})
}
fn status_page(name: &str) -> Value {
    json!({"sha":"a".repeat(40),"total_count":1,"repository":{"id":410,"node_id":"R_410","full_name":name},"statuses":[{"id":700,"node_id":"STATUS_700","context":"lint","state":"pending","creator":{"id":55,"login":"status-actor","type":"User"}}]})
}
fn resource_steps(kind: PrResourceKindV1, name: &str) -> Vec<GhStep> {
    let mut steps = principal(42, "fresh-resource-principal");
    steps.push(get(&format!("repos/{REPO}"), repo(410, name)));
    steps.push(get(&format!("repos/{name}/pulls/7"), resource_parent(name)));
    match kind {
        PrResourceKindV1::Files => {
            steps.push(get(&format!("repos/{name}/pulls/7/files?per_page=100&page=1"),json!([{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1,2 +1,2 @@\n context\n-old\n+new"}])));
            let mut step = GhStep::read(
                &[
                    "pr",
                    "diff",
                    "7",
                    "--repo",
                    &format!("{HOST}/{name}"),
                    "--color",
                    "never",
                ],
                "",
            );
            step.stdout = PATCH.to_vec();
            steps.push(step);
        }
        PrResourceKindV1::Checks => {
            steps.push(get(
                &format!(
                    "repos/{name}/commits/{}/check-runs?filter=latest&per_page=100&page=1",
                    "a".repeat(40)
                ),
                check_page(vec![check_value(600)], 1),
            ));
            steps.push(get(
                &format!(
                    "repos/{name}/commits/{}/status?per_page=100&page=1",
                    "a".repeat(40)
                ),
                status_page(name),
            ));
            steps.extend(required_steps(name, branch_protection(92), rules_page()));
        }
    }
    steps.push(get(&format!("repos/{name}/pulls/7"), resource_parent(name)));
    steps.push(get(&format!("repos/{REPO}"), repo(410, name)));
    steps.extend(principal(42, "renamed-same-positive-principal"));
    assert_eq!(
        steps.len(),
        if kind == PrResourceKindV1::Checks {
            12
        } else {
            10
        }
    );
    steps
}
/// Classic protection on `main` requiring Build600 from `app`.
fn branch_protection(app: u64) -> Value {
    json!({"name":"main","protected":true,"protection":{"enabled":true,"required_status_checks":{
        "enforcement_level":"non_admins","contexts":["Build600"],
        "checks":[{"context":"Build600","app_id":app}]}}})
}
/// One active ruleset requiring the `lint` status and an unreported `release-gate`.
fn rules_page() -> Value {
    json!([{"type":"pull_request","parameters":{}},
        {"type":"required_status_checks","parameters":{"required_status_checks":[
            {"context":"lint"},{"context":"release-gate"}]}}])
}
fn required_steps(name: &str, branch: Value, rules: Value) -> [GhStep; 2] {
    [
        get(&format!("repos/{name}/branches/main"), branch),
        get(
            &format!("repos/{name}/rules/branches/main?per_page=100&page=1"),
            rules,
        ),
    ]
}
fn script(op: &[GhStep]) -> Vec<GhStep> {
    let mut steps = source_steps();
    steps.extend(operation(REPO, 42));
    steps.extend_from_slice(op);
    steps
}
fn opened(
    fixture: &Fixture,
) -> (
    Arc<CatalogueIntentSource>,
    Arc<ActiveRemoteScopeV1>,
    GithubWorkspace,
) {
    git(fixture, &["init", "-b", "main"]);
    git(fixture, &["config", "user.name", "Resource fixture"]);
    git(
        fixture,
        &["config", "user.email", "resource@example.invalid"],
    );
    fs::write(fixture.repo().join("tracked"), b"saved baseline\n").unwrap();
    git(fixture, &["add", "tracked"]);
    git(fixture, &["commit", "-m", "resource baseline"]);
    fs::write(fixture.repo().join("tracked"), b"DIRTY_LOCAL_BYTES\n").unwrap();
    let original = load(fixture);
    let scope = active(&original);
    let workspace = call(
        fixture,
        original.clone(),
        scope.clone(),
        previous(&original),
    )
    .unwrap()
    .workspace;
    drop(crate::pr_write_journal::PrWriteJournal::open(&database(fixture)).unwrap());
    (original, scope, workspace)
}
fn git_state_resource(fixture: &Fixture) -> (String, String, Vec<u8>, Vec<u8>) {
    (
        git(fixture, &["rev-parse", "HEAD"]),
        git(fixture, &["status", "--porcelain=v1"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fs::read(fixture.repo().join("tracked")).unwrap(),
    )
}
fn run_resource(
    fixture: &Fixture,
    original: Arc<CatalogueIntentSource>,
    scope: Arc<ActiveRemoteScopeV1>,
    workspace: &mut GithubWorkspace,
    kind: PrResourceKindV1,
    limits: &PrResourceReadLimitsV1,
) -> Result<PrResourceResultV1, PrResourceFailureV1> {
    let input = take_resource_input(workspace, kind).unwrap();
    read_resource(
        original,
        scope,
        input,
        &provider(fixture),
        &database(fixture),
        limits,
        &RepositoryCatalogueAccessLimits::default(),
        &CancellationToken::default(),
    )
}
fn effects(
    fixture: &Fixture,
    before: &[String],
    git_before: &(String, String, Vec<u8>, Vec<u8>),
    steps: &[GhStep],
) {
    assert_eq!(sql_state(fixture), before);
    assert_eq!(&git_state_resource(fixture), git_before);
    exact(fixture, steps);
    let attempts: u64 = sql(fixture)
        .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(attempts, 0);
    let foreign:u64=sql(fixture).query_row("SELECT (SELECT count(*) FROM records WHERE login='user-id:43')+(SELECT count(*) FROM drafts WHERE login='user-id:43')+(SELECT count(*) FROM saved_views WHERE login='user-id:43')",[],|row|row.get(0)).unwrap();
    assert_eq!(foreign, 0);
}
fn static_failure(failure: &PrResourceFailureV1) {
    for text in [
        DRAFT,
        OLD,
        FRESH,
        "PRIVATE_DIAGNOSTIC",
        "PRIVATE_SQL_CANARY",
    ] {
        assert!(!failure.to_string().contains(text));
    }
}

#[test]
fn scoped_pr_resource_files_actual_raw_bytes_full_head_unknown_old_image_and_original_effects() {
    let op = resource_steps(PrResourceKindV1::Files, REPO);
    let steps = script(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, scope, mut workspace) = opened(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let inbox = workspace.inbox.pull_requests.as_ptr();
    let result = run_resource(
        &fixture,
        source.clone(),
        scope.clone(),
        &mut workspace,
        PrResourceKindV1::Files,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(result.accounting.children_started(), 4);
    assert_eq!(result.accounting.failed_reserved_bytes(), 0);
    assert!(result.accounting.sealed());
    let view = result.view.as_ref().unwrap();
    assert_eq!(view.observed().raw_patch(), Some(PATCH));
    assert_eq!(view.observed().parent().database_id(), 1007);
    assert_eq!(view.observed().parent().repository_id(), 410);
    assert_eq!(
        view.observed().parent().summary().pr.id,
        "GENUINE_PR_NODE_1007"
    );
    assert!(view.observed().parent().summary().pr.body.is_empty());
    let diff = view.canonical_diff().unwrap();
    assert_eq!(diff.head.as_deref(), Some("a".repeat(40).as_str()));
    assert!(diff.base.is_none());
    assert_eq!(diff.files[0].hunks[0].lines[2].new_line, Some(2));
    assert!(result
        .page
        .rows
        .iter()
        .any(|row| row.label == OLD_IMAGE_UNVERIFIED));
    assert!(result.page.diff.is_none());
    assert!(result.page.checks.is_none());
    assert!(Arc::ptr_eq(&source, &result.original_source));
    assert!(Arc::ptr_eq(&scope, &result.original_active));
    install_resource_if_current(&mut workspace, result).unwrap();
    assert_eq!(workspace.inbox.pull_requests.as_ptr(), inbox);
    assert_eq!(workspace.pr.as_ref().unwrap().tab, PrTab::Files);
    let mut app = App {
        github: Some(workspace),
        remote_scope: Some(scope.clone()),
        remote_scope_source: Some(source.clone()),
        ..Default::default()
    };
    let observed = app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_resource
        .as_ref()
        .unwrap()
        .observed() as *const _;
    let canonical = app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_resource
        .as_ref()
        .unwrap()
        .canonical_diff()
        .unwrap() as *const _;
    for action in [
        PrResourceUiActionV1::NextFile,
        PrResourceUiActionV1::NextHunk,
        PrResourceUiActionV1::PatchContext,
        PrResourceUiActionV1::Layout,
    ] {
        app.dispatch(Action::Close);
        let selected = app.inbox_list.selected;
        let native::PrResourceWorkV1::Loaded(input) = native::take_work(
            app.github.as_mut().unwrap(),
            GithubAction::Resource(action),
            selected,
        )
        .unwrap() else {
            panic!("loaded control must never choose a fresh reader");
        };
        let prepared = native::prepare_loaded(
            source.clone(),
            scope.clone(),
            *input,
            &CancellationToken::default(),
        )
        .unwrap();
        let effects = native::install_completion(
            &mut app,
            native::PrResourceCompletionV1::Loaded(Box::new(prepared)),
        )
        .unwrap();
        native::apply_effects(&mut app, effects);
        let current = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .as_ref()
            .unwrap();
        assert_eq!(current.observed() as *const _, observed);
        assert_eq!(current.canonical_diff().unwrap() as *const _, canonical);
        if action == PrResourceUiActionV1::PatchContext {
            assert!(app.notice.starts_with("Loaded patch context\n"));
            assert!(app.notice.contains("old Some"));
            assert!(app.notice.len() < 16000);
        }
        if action == PrResourceUiActionV1::Layout {
            assert_eq!(current.layout, crate::repository_view::DiffLayout::Split);
            assert!(app
                .github
                .as_ref()
                .unwrap()
                .pr
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .any(|row| row.label.contains('│')));
        }
    }
    app.dispatch(Action::Close);
    for (action, _, key) in PrResourceUiActionV1::CONTROLS {
        use crossterm::event::{
            Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
        };
        if action == PrResourceUiActionV1::Threads {
            app.inbox_list.selected =
                (0..app.github.as_ref().unwrap().pr.as_ref().unwrap().rows.len())
                    .find(|index| {
                        matches!(
                            app.github
                                .as_ref()
                                .unwrap()
                                .scoped_pr_resource
                                .as_ref()
                                .unwrap()
                                .row(*index),
                            Some(PrResourceRowV1::FileList(0))
                        )
                    })
                    .unwrap();
        }
        app.hits.clear();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(200, 24)).unwrap();
        terminal
            .draw(|frame| {
                let area = frame.area();
                draw_with_key_bar(frame, &mut app, area);
            })
            .unwrap();
        let rect = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::Github(GithubAction::Resource(action)))
            .unwrap()
            .rect;
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char(key),
            KeyModifiers::NONE,
        )));
        if action == PrResourceUiActionV1::CommentLine {
            // Comment line reuses Old image for a selected diff line, or explains.
            let line = |app: &mut App| {
                match app.github_action.take() {
                Some(queued) => {
                    assert_eq!(
                        queued,
                        GithubAction::Resource(PrResourceUiActionV1::OldImage)
                    );
                    assert!(app.pr_line_comment_target_v19.take().is_some());
                }
                None => assert_eq!(
                    app.status,
                    crate::scoped_pr_observed_tabs_v1::resources::immutable_images::line_comment_v19::CHOOSE_LINE
                ),
            }
            };
            line(&mut app);
            app.handle_event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: rect.x,
                row: rect.y,
                modifiers: KeyModifiers::NONE,
            }));
            line(&mut app);
            continue;
        }
        if action == PrResourceUiActionV1::Threads {
            assert!(app.github_action.is_none());
            assert_eq!(app.overlay, Some(crate::app::Overlay::PrThreadsV1));
            assert!(app
                .scoped_threads_work_v1
                .as_ref()
                .unwrap()
                .files_intent()
                .expect("Actual Files-origin thread operation")
                .owns(&app));
            crate::scoped_pr_observed_tabs_v1::resources::threads_v1::native_v1::close(&mut app);
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    draw_with_key_bar(frame, &mut app, area);
                })
                .unwrap();
            let current = app
                .hits
                .iter()
                .find(|hit| hit.action == Action::Github(GithubAction::Resource(action)))
                .unwrap()
                .rect;
            app.handle_event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: current.x,
                row: current.y,
                modifiers: KeyModifiers::NONE,
            }));
            assert_eq!(app.overlay, Some(crate::app::Overlay::PrThreadsV1));
            assert!(app
                .scoped_threads_work_v1
                .as_ref()
                .unwrap()
                .files_intent()
                .expect("Actual Files-origin thread operation")
                .owns(&app));
            crate::scoped_pr_observed_tabs_v1::resources::threads_v1::native_v1::close(&mut app);
            continue;
        }
        if action == PrResourceUiActionV1::NativeDiff {
            assert!(app.github_action.is_none());
            let work = app
                .pr_native_diff_work_v1
                .take()
                .expect("Native key captures actual original image work");
            assert!(app.github.as_ref().unwrap().pr.is_none());
            work.restore_not_submitted(&mut app);
            assert!(app.github.as_ref().unwrap().pr.is_some());
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    draw_with_key_bar(frame, &mut app, area);
                })
                .unwrap();
        } else {
            assert_eq!(
                app.github_action.take(),
                Some(GithubAction::Resource(action))
            );
        }
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: rect.x,
            row: rect.y,
            modifiers: KeyModifiers::NONE,
        }));
        if action == PrResourceUiActionV1::NativeDiff {
            assert!(app.github_action.is_none());
            let work = app
                .pr_native_diff_work_v1
                .take()
                .expect("Native mouse captures SAME original image work");
            assert!(app.github.as_ref().unwrap().pr.is_none());
            work.restore_not_submitted(&mut app);
            assert!(app.github.as_ref().unwrap().pr.is_some());
        } else {
            assert_eq!(
                app.github_action.take(),
                Some(GithubAction::Resource(action))
            );
        }
    }
    let selected = app.inbox_list.selected;
    assert!(matches!(
        native::take_work(
            app.github.as_mut().unwrap(),
            GithubAction::Resource(PrResourceUiActionV1::OldImage),
            selected,
        )
        .unwrap(),
        native::PrResourceWorkV1::Images(_)
    ), "Old image requires its own genuine fresh reader, never a loaded-patch explanation or base-tip image");
    effects(&fixture, &before, &git, &steps);
}
#[test]
fn scoped_pr_resource_checks_nullable_actor_time_and_required_rules_are_not_zero_or_bridge() {
    let mut op = resource_steps(PrResourceKindV1::Checks, REPO);
    let mut value = check_value(600);
    value["user"] = Value::Null;
    value["completed_at"] = Value::Null;
    op[4] = get(&op[4].args[1], check_page(vec![value], 1));
    let steps = script(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, scope, mut workspace) = opened(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let result = run_resource(
        &fixture,
        source,
        scope,
        &mut workspace,
        PrResourceKindV1::Checks,
        &Default::default(),
    )
    .unwrap();
    let view = result.view.as_ref().unwrap();
    assert!(view.observed().check_coverage().complete_observed());
    assert!(view.observed().status_coverage().complete_observed());
    assert_eq!(view.observed().checks()[0].id, 600);
    assert!(view.observed().checks()[0].actor_id.is_none());
    assert!(view.observed().checks()[0].completed_at.is_none());
    // Build600 (app 92, as pinned) failed, lint is pending, release-gate never ran.
    let labels: Vec<&str> = result.page.rows.iter().map(|r| r.label.as_str()).collect();
    assert!(labels.iter().any(|label| label
        .contains("Required on main: 1 failing, 1 pending, 1 not reported")
        && !label.contains("incomplete")));
    assert!(result
        .page
        .rows
        .iter()
        .any(|row| row.label.contains("Check#600")
            && row.label.contains(" • required")
            && row.role == SemanticToken::Failure));
    assert!(labels
        .iter()
        .any(|label| label.contains("Status#700") && label.contains(" • required")));
    assert!(labels
        .iter()
        .any(|label| label
            .contains("release-gate • required • expected, not reported for this head")));
    assert!(!labels
        .iter()
        .any(|label| label.contains("Required rules unavailable")));
    assert!(result
        .page
        .rows
        .iter()
        .any(|row| row.label.contains("not an Actions run")));
    assert!(result.page.checks.is_none());
    assert_eq!(result.accounting.children_started(), 6);
    effects(&fixture, &before, &git, &steps);
}
#[test]
fn scoped_pr_resource_required_contexts_match_by_name_and_the_app_the_rule_pins() {
    let mut op = resource_steps(PrResourceKindV1::Checks, REPO);
    // Classic protection pins Build600 to app 77, but app 92 reported it; the
    // ruleset pins the `lint` status to app 5, and statuses name no app.
    let rules = json!([{"type":"required_status_checks","parameters":{"required_status_checks":[
        {"context":"lint","integration_id":5}]}}]);
    let [branch, rules] = required_steps(REPO, branch_protection(77), rules);
    op[6] = branch;
    op[7] = rules;
    let steps = script(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, scope, mut workspace) = opened(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let result = run_resource(
        &fixture,
        source,
        scope,
        &mut workspace,
        PrResourceKindV1::Checks,
        &Default::default(),
    )
    .unwrap();
    let labels: Vec<&str> = result.page.rows.iter().map(|r| r.label.as_str()).collect();
    assert!(
        labels
            .iter()
            .any(|label| label.contains("Required on main: 1 not reported, 1 app unverified")),
        "{labels:#?}"
    );
    assert!(labels
        .iter()
        .any(|label| label.contains("Check#600") && label.contains(" • not required")));
    assert!(labels
        .iter()
        .any(|label| label.contains("Status#700") && label.contains(" • required unknown")));
    assert!(labels
        .iter()
        .any(|label| label.contains("Build600 • required • expected, not reported for this head")));
    assert!(labels
        .iter()
        .any(|label| label.contains("lint • required • reported without the app the rule pins")));
    assert_eq!(result.accounting.children_started(), 6);
    effects(&fixture, &before, &git, &steps);
}
#[test]
fn scoped_pr_resource_unreadable_rules_are_unavailable_not_an_empty_required_set() {
    for case in 0..2 {
        let mut op = resource_steps(PrResourceKindV1::Checks, REPO);
        if case == 0 {
            // A hidden branch-protection read stops before the rulesets read.
            op[6].status = 1;
            op[6].stdout = Vec::new();
            op[6].stderr = b"gh: Not Found (HTTP 404)".to_vec();
            op.remove(7);
        } else {
            op[7] = get(
                &op[7].args[1],
                Value::Array((0..100).map(|_| json!({"type":"pull_request"})).collect()),
            );
        }
        let steps = script(&op);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let (source, scope, mut workspace) = opened(&fixture);
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let result = run_resource(
            &fixture,
            source,
            scope,
            &mut workspace,
            PrResourceKindV1::Checks,
            &Default::default(),
        )
        .unwrap();
        let labels: Vec<&str> = result.page.rows.iter().map(|r| r.label.as_str()).collect();
        let expected = if case == 0 {
            "Required checks on main unavailable: branch protection"
        } else {
            "Required checks on main unavailable: rulesets exceed one page"
        };
        assert!(
            labels.iter().any(|label| label.starts_with(expected)),
            "{labels:#?}"
        );
        // Nothing is called required or optional while the rules are unknown.
        assert!(labels
            .iter()
            .filter(|label| label.contains("Check#") || label.contains("Status#"))
            .all(|label| label.ends_with(" • required unknown")));
        assert!(!labels
            .iter()
            .any(|label| label.contains("not required") || label.contains("required • ")));
        effects(&fixture, &before, &git, &steps);
    }
}
#[test]
fn scoped_pr_resource_raw_base_repository_id_node_missing_cannot_be_name_authority() {
    for case in 0..3 {
        let mut op = resource_steps(PrResourceKindV1::Files, REPO);
        let mut value = resource_parent(REPO);
        match case {
            0 => value["base"]["repo"]["id"] = json!(999),
            1 => value["base"]["repo"]["node_id"] = json!("RECREATED_NODE"),
            _ => value["base"]["repo"]
                .as_object_mut()
                .unwrap()
                .remove("node_id")
                .map(|_| ())
                .unwrap(),
        };
        op[3] = get(&op[3].args[1], value);
        op.truncate(4);
        let steps = script(&op);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let (source, scope, mut workspace) = opened(&fixture);
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let failure = run_resource(
            &fixture,
            source,
            scope,
            &mut workspace,
            PrResourceKindV1::Files,
            &Default::default(),
        )
        .unwrap_err();
        static_failure(&failure);
        effects(&fixture, &before, &git, &steps);
    }
}
#[test]
fn scoped_pr_resource_closing_pr_database_head_base_ref_node_lifecycle_drift_refuses() {
    for case in 0..6 {
        let mut op = resource_steps(PrResourceKindV1::Files, REPO);
        let mut value = resource_parent(REPO);
        match case {
            0 => value["id"] = json!(1008),
            1 => value["head"]["sha"] = json!("c".repeat(40)),
            2 => value["base"]["sha"] = json!("d".repeat(40)),
            3 => value["head"]["ref"] = json!("different"),
            4 => value["node_id"] = json!("DIFFERENT_PR"),
            _ => value["draft"] = Value::Null,
        };
        op[6] = get(&op[6].args[1], value);
        op.truncate(7);
        let steps = script(&op);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let (source, scope, mut workspace) = opened(&fixture);
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let failure = run_resource(
            &fixture,
            source,
            scope,
            &mut workspace,
            PrResourceKindV1::Files,
            &Default::default(),
        )
        .unwrap_err();
        static_failure(&failure);
        effects(&fixture, &before, &git, &steps);
    }
}
#[test]
fn scoped_pr_resource_current_owner_repo_permission_and_numeric_principal_refuse_no_fallback() {
    for case in 0..4 {
        let mut op = resource_steps(PrResourceKindV1::Checks, REPO);
        if case == 0 {
            op[1] = get("user", json!({"id":43,"login":"octocat"}));
            op.truncate(2);
        } else if case == 1 {
            op[2] = get(&format!("repos/{REPO}"), repo(999, REPO));
            op.truncate(3);
        } else if case == 2 {
            let mut value = repo(410, REPO);
            value["permissions"]["pull"] = Value::Null;
            op[2] = get(&format!("repos/{REPO}"), value);
            op.truncate(3);
        } else {
            op[11] = get("user", json!({"id":43,"login":"octocat"}));
        }
        let steps = script(&op);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let (source, scope, mut workspace) = opened(&fixture);
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let failure = run_resource(
            &fixture,
            source,
            scope,
            &mut workspace,
            PrResourceKindV1::Checks,
            &Default::default(),
        )
        .unwrap_err();
        static_failure(&failure);
        effects(&fixture, &before, &git, &steps);
    }
}
#[test]
fn scoped_pr_resource_same_id_rename_and_same_uid_login_rename_update_only_selected_page() {
    let name = "example/current-renamed";
    let op = resource_steps(PrResourceKindV1::Files, name);
    let steps = script(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, scope, mut workspace) = opened(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let cached = workspace.selected.clone();
    let oldnav = workspace.scoped_pr_navigation.clone().unwrap();
    let result = run_resource(
        &fixture,
        source.clone(),
        scope.clone(),
        &mut workspace,
        PrResourceKindV1::Files,
        &Default::default(),
    )
    .unwrap();
    assert_eq!(result.closed_identity.account_id, Some(42));
    assert_eq!(
        result.closed_identity.login,
        "renamed-same-positive-principal"
    );
    assert!(Arc::ptr_eq(&oldnav, &result.requested_navigation));
    install_resource_if_current(&mut workspace, result).unwrap();
    assert_eq!(workspace.selected, cached);
    assert_eq!(workspace.inbox.pull_requests[0].repository, REPO);
    assert_eq!(
        workspace.pr.as_ref().unwrap().details.summary.pr.repository,
        name
    );
    assert_eq!(source.source().repositories()[0].full_name, REPO);
    assert!(matches!(
        scope.consumer(&workspace.identity),
        crate::active_remote_scope_v1::RemoteScopeConsumerV1::Explicit(_)
    ));
    effects(&fixture, &before, &git, &steps);
}
#[test]
fn scoped_pr_resource_missing_coverage_unknown_and_genuine_observed_empty_are_distinct() {
    for missing in [true, false] {
        let mut op = resource_steps(PrResourceKindV1::Checks, REPO);
        op[4] = get(
            &op[4].args[1],
            if missing {
                json!({"check_runs":[]})
            } else {
                check_page(vec![], 0)
            },
        );
        let mut status = status_page(REPO);
        status["statuses"] = json!([]);
        status["total_count"] = json!(0);
        op[5] = get(&op[5].args[1], status);
        let steps = script(&op);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let (source, scope, mut workspace) = opened(&fixture);
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let result = run_resource(
            &fixture,
            source,
            scope,
            &mut workspace,
            PrResourceKindV1::Checks,
            &Default::default(),
        )
        .unwrap();
        let view = result.view.as_ref().unwrap();
        assert_eq!(view.observed().check_coverage().complete_empty(), !missing);
        assert!(view.observed().status_coverage().complete_empty());
        assert!(view.observed().checks().is_empty());
        // Nothing satisfies a required context, but "not reported" is only
        // claimed outright when both sources were enumerated completely.
        let (notice, role) = view.observed().required_rules_notice();
        assert!(notice.starts_with("Required on main: 3 not reported"));
        assert_eq!(notice.contains("observed results incomplete"), missing);
        assert_eq!(role, SemanticToken::Pending);
        let incomplete = result
            .page
            .rows
            .iter()
            .filter(|row| {
                row.label
                    .contains("not seen in the incomplete observed results")
            })
            .count();
        assert_eq!(incomplete, if missing { 3 } else { 0 });
        effects(&fixture, &before, &git, &steps);
    }
}
#[test]
fn v1_pr_threads_files_page_over_128kib_of_patches_still_lists_changed_files() {
    // Ordinary PRs exceed 128 KiB per 100-file page; that left the canonical
    // file list unloaded, so no inline, range or FILE comment could start.
    let mut op = resource_steps(PrResourceKindV1::Files, REPO);
    let patch = format!(
        "@@ -1,2 +1,2 @@\n context\n-old\n+{}",
        "n".repeat(200 * 1024)
    );
    op[4] = get(
        &op[4].args[1],
        json!([{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":patch}]),
    );
    let steps = script(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, scope, mut workspace) = opened(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let result = run_resource(
        &fixture,
        source,
        scope,
        &mut workspace,
        PrResourceKindV1::Files,
        &Default::default(),
    )
    .unwrap();
    let observed = result.view.as_ref().unwrap().observed();
    assert_eq!(observed.files().len(), 1);
    assert!(observed.file_coverage().complete_observed());
    effects(&fixture, &before, &git, &steps);
}
#[test]
fn v1_pr_files_binary_renamed_and_github_3000_file_cap_are_labelled_not_guessed() {
    // GitHub omits `patch` for binary and too-large files and never lists more
    // than 3000 files. Neither may read as an empty diff or a complete list.
    let mut op = resource_steps(PrResourceKindV1::Files, REPO);
    let mut parent = resource_parent(REPO);
    parent["changed_files"] = json!(3500);
    op[3] = get(&op[3].args[1], parent.clone());
    op[6] = get(&op[6].args[1], parent);
    let pages = (0..30).map(|page| {
        let rows = (0..100)
            .map(|i| match (page, i) {
                (0, 0) => json!({"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1,2 +1,2 @@\n context\n-old\n+new"}),
                (0, 1) => json!({"filename":"logo.png","status":"modified","additions":0,"deletions":0}),
                (0, 2) => json!({"filename":"src/new.rs","previous_filename":"src/old.rs","status":"renamed","additions":0,"deletions":0}),
                _ => json!({"filename":format!("gen/f{page:02}_{i:02}.txt"),"status":"added","additions":1,"deletions":0,"patch":"@@ -0,0 +1 @@\n+x"}),
            })
            .collect::<Vec<_>>();
        get(
            &format!("repos/{REPO}/pulls/7/files?per_page=100&page={}", page + 1),
            json!(rows),
        )
    });
    let diff = op.remove(5);
    op.splice(4..5, pages);
    op.insert(34, diff);
    let steps = script(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, scope, mut workspace) = opened(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let limits = PrResourceReadLimitsV1 {
        max_pages_per_source: 30,
        ..Default::default()
    };
    let result = run_resource(
        &fixture,
        source,
        scope,
        &mut workspace,
        PrResourceKindV1::Files,
        &limits,
    )
    .unwrap();
    let observed = result.view.as_ref().unwrap().observed();
    assert_eq!(observed.files().len(), 3000);
    assert!(!observed.file_coverage().complete_observed());
    assert!(!observed.files()[1].patch_supplied);
    let labels = result
        .page
        .rows
        .iter()
        .map(|row| row.label.as_str())
        .collect::<Vec<_>>();
    assert!(labels.contains(&"Files: 3000 of 3500 loaded • GitHub lists at most 3000 files"));
    assert!(labels.contains(&"File src/view.rs • modified"));
    assert!(labels.contains(&"File logo.png • modified • no API patch (binary or too large)"));
    assert!(labels.contains(
        &"File src/new.rs • renamed from src/old.rs • no API patch (binary or too large)"
    ));
    effects(&fixture, &before, &git, &steps);
}
#[test]
fn scoped_pr_resource_fixed_page_cap_and_repeated_context_no_invented_complete_or_success() {
    let mut op = resource_steps(PrResourceKindV1::Checks, REPO);
    op[4] = get(
        &op[4].args[1],
        check_page((1..=100).map(check_value).collect(), 200),
    );
    let mut status = status_page(REPO);
    status["total_count"] = json!(2);
    status["statuses"] = json!([{"id":700,"context":"Lint","state":"success"},{"id":701,"context":"lint","state":"failure"}]);
    op[5] = get(&op[5].args[1], status);
    let steps = script(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, scope, mut workspace) = opened(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let limits = PrResourceReadLimitsV1 {
        max_pages_per_source: 1,
        max_rows_per_source: 100,
        ..Default::default()
    };
    let result = run_resource(
        &fixture,
        source,
        scope,
        &mut workspace,
        PrResourceKindV1::Checks,
        &limits,
    )
    .unwrap();
    let view = result.view.as_ref().unwrap();
    assert_eq!(view.observed().check_coverage().requested_pages, 1);
    assert!(!view.observed().check_coverage().complete_observed());
    assert_eq!(view.observed().checks().len(), 100);
    assert!(view
        .observed()
        .statuses()
        .iter()
        .all(|row| !row.current_context_unique));
    assert!(
        result
            .page
            .rows
            .iter()
            .filter(|row| row.label.contains("unknown repeated context"))
            .count()
            == 2
    );
    effects(&fixture, &before, &git, &steps);
}
#[test]
fn scoped_pr_resource_sql_recapture_after_closing_repo_before_final_account_no_rebase() {
    let op = resource_steps(PrResourceKindV1::Files, REPO);
    let steps = script(&op[..8]);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, scope, mut workspace) = opened(&fixture);
    let git = git_state_resource(&fixture);
    let input = take_resource_input(&mut workspace, PrResourceKindV1::Files).unwrap();
    let mut phases = vec![];
    let result=read_resource_with_guard(source,scope,input,&provider(&fixture),&database(&fixture),&Default::default(),&Default::default(),&CancellationToken::default(),|phase|{
        let count=fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap().lines().count();phases.push((phase,count));
        if phase==PrResourcePhaseV1::BeforeClosingAccount {Connection::open(database(&fixture)).unwrap().execute("INSERT INTO pr_write_scopes(host,principal,pr_node,generation) VALUES(?1,'user-id:42','GENUINE_PR_NODE_1007',1)",[HOST]).unwrap();}
        Ok(())
    }).unwrap_err();
    static_failure(&result);
    assert_eq!(
        phases,
        vec![
            (PrResourcePhaseV1::AfterOpeningSource, 24),
            (PrResourcePhaseV1::BeforeClosingAccount, 28)
        ]
    );
    exact(&fixture, &steps);
    assert_eq!(git_state_resource(&fixture), git);
    assert_eq!(
        Cache::open(&database(&fixture))
            .unwrap()
            .draft(&Scope::github(HOST, Some(42)).unwrap(), "original-review")
            .unwrap()
            .as_deref(),
        Some(DRAFT)
    );
}
#[test]
fn scoped_pr_resource_failed_multibyte_capture_charged_and_no_child_over_remaining_pool() {
    let mut op = resource_steps(PrResourceKindV1::Checks, REPO);
    op[4].status = 1;
    op[4].stdout = "界".repeat(2000).into_bytes();
    op[4].stderr = "\x1b[0m".repeat(3000).into_bytes();
    // After failed4096 capture, neither another4096 source nor its4096 closing
    // hold fits10KiB. It must launch ONLY the already reserved closing parent:
    // no status page and no required-rule reads, which become Unavailable.
    op.drain(5..8);
    let steps = script(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, scope, mut workspace) = opened(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let limits = PrResourceReadLimitsV1 {
        max_document_bytes: 4096,
        max_pool_bytes: 10 * 1024,
        ..Default::default()
    };
    let result = run_resource(
        &fixture,
        source,
        scope,
        &mut workspace,
        PrResourceKindV1::Checks,
        &limits,
    )
    .unwrap();
    assert_eq!(result.accounting.failed_reserved_bytes(), 4096);
    assert_eq!(result.accounting.children_started(), 3);
    assert!(matches!(
        result.view.as_ref().unwrap().observed().required_checks(),
        Some(crate::providers::gh::RequiredChecksV1::Unavailable { .. })
    ));
    assert!(!result
        .view
        .as_ref()
        .unwrap()
        .observed()
        .status_coverage()
        .complete_empty());
    effects(&fixture, &before, &git, &steps);
}
#[test]
fn scoped_pr_resource_raw_nonutf8_binary_hunks_layout_context_and_display_cap_keep_coordinates() {
    for case in 0..3 {
        let capped = case != 0;
        let mut op = resource_steps(PrResourceKindV1::Files, REPO);
        op[5].stdout = if case == 1 {
            format!(
                "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -0,0 +1,70000 @@\n{}",
                "+x\n".repeat(70000)
            )
            .into_bytes()
        } else if case == 2 {
            let mut raw = b"diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -0,0 +1,1 @@\n+".to_vec();
            // One actual raw line fits the capture cap, while display escaping
            // and split-row copies exceed8MiB. Refuse BEFORE diff_rows copies.
            raw.extend(std::iter::repeat_n(1u8, 750_000));
            raw.push(b'\n');
            raw
        } else {
            b"diff --git a/x b/x\n--- \"a/raw\\377.rs\"\n+++ \"b/raw\\377.rs\"\n@@ -1,2 +1,2 @@\n context\n-\xffold\n+new\ndiff --git a/img b/img\nBinary files a/img and b/img differ\n".to_vec()
        };
        let steps = script(&op);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let (source, scope, mut workspace) = opened(&fixture);
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let previous_rows = workspace.pr.as_ref().unwrap().rows.as_ptr();
        let mut result = run_resource(
            &fixture,
            source,
            scope,
            &mut workspace,
            PrResourceKindV1::Files,
            &Default::default(),
        )
        .unwrap();
        assert_eq!(result.display_limited, capped);
        if capped {
            assert_eq!(result.page.rows.as_ptr(), previous_rows);
            assert_eq!(result.page.tab, PrTab::Summary);
        } else {
            let mut view = Arc::try_unwrap(result.view.take().unwrap()).unwrap();
            let diff = view.canonical_diff().unwrap();
            assert_eq!(
                diff.files[0].new_path.as_deref(),
                Some(b"raw\xff.rs".as_slice())
            );
            assert!(diff.files[1].binary);
            assert!(diff.base.is_none());
            let index = view.next(0, true).unwrap();
            let context = view.patch_context(index, 2).unwrap();
            assert!(context[0].contains("Loaded patch context only"));
            view.layout = crate::repository_view::DiffLayout::Split;
            let rows = view
                .prepare_rows(&CancellationToken::default())
                .unwrap()
                .unwrap();
            assert!(rows.iter().any(|row| row.label.contains('│')));
            assert!(rows
                .iter()
                .filter_map(|row| row.anchor.as_ref())
                .all(|anchor| anchor.base.is_none()
                    && anchor.head.as_deref() == Some("a".repeat(40).as_str())));
        }
        effects(&fixture, &before, &git, &steps);
    }
}
#[test]
fn scoped_pr_resource_scope_stop_cancel_back_and_callback_deadline_do_not_reopen_page() {
    for case in 0..3 {
        let full = resource_steps(PrResourceKindV1::Files, REPO);
        let op = if case == 0 {
            vec![]
        } else {
            full[..4].to_vec()
        };
        let steps = script(&op);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let (source, scope, mut workspace) = opened(&fixture);
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let input = take_resource_input(&mut workspace, PrResourceKindV1::Files).unwrap();
        if case == 0 {
            let empty =
                Arc::new(ActiveRemoteScopeV1::explicit_selection(source.source(), &[]).unwrap());
            let failure = read_resource(
                source,
                empty,
                input,
                &provider(&fixture),
                &database(&fixture),
                &Default::default(),
                &Default::default(),
                &CancellationToken::default(),
            )
            .unwrap_err();
            static_failure(&failure);
        } else {
            let token = CancellationToken::default();
            let trigger = token.clone();
            let mut access = RepositoryCatalogueAccessLimits::default();
            access.transport.max_duration = Duration::from_secs(1);
            let failure = read_resource_with_guard(
                source,
                scope,
                input,
                &provider(&fixture),
                &database(&fixture),
                &Default::default(),
                &access,
                &token,
                |phase| {
                    if phase == PrResourcePhaseV1::AfterOpeningSource {
                        if case == 1 {
                            trigger.cancel();
                        } else {
                            thread::sleep(Duration::from_millis(1100));
                        }
                    }
                    Ok(())
                },
            )
            .unwrap_err();
            static_failure(&failure);
        }
        assert!(workspace.pr.is_none());
        effects(&fixture, &before, &git, &steps);
    }
    let op = resource_steps(PrResourceKindV1::Files, REPO);
    let steps = script(&op);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, scope, mut workspace) = opened(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let result = run_resource(
        &fixture,
        source,
        scope,
        &mut workspace,
        PrResourceKindV1::Files,
        &Default::default(),
    )
    .unwrap();
    workspace.scoped_pr_navigation = None;
    assert!(install_resource_if_current(&mut workspace, result).is_err());
    assert!(workspace.pr.is_none());
    effects(&fixture, &before, &git, &steps);
}

/// The PR page and its key bar in one frame: the Files resource controls are footer tokens.
fn draw_with_key_bar(frame: &mut ratatui::Frame, app: &mut App, area: ratatui::layout::Rect) {
    let content = ratatui::layout::Rect::new(area.x, area.y, area.width, area.height - 1);
    crate::github_ui::draw(frame, app, content);
    let controls = crate::footer::main_controls(app);
    let bar = ratatui::layout::Rect::new(area.x, area.bottom() - 1, area.width, 1);
    crate::footer::draw(frame, app, bar, &controls);
}
