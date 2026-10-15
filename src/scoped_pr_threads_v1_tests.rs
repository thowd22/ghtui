//! DECLARED/UNRUN until Root serializes verification. All source capabilities
//! below come from the genuine existing Source/PR/Files worker factories.
use super::*;
#[path = "scoped_pr_threads_preparation_close_v4_tests.rs"]
mod preparation_close_v4;
#[path = "scoped_pr_thread_publication_v1_tests.rs"]
mod publication_v1;
use crate::providers::gh::METRICS_REVIEW_THREADS_QUERY;
use crate::scoped_pr_observed_tabs_v1::resources::threads_v1::{self as threads, native_v1, ui_v1};
use crate::{
    app::{Overlay, View},
    runtime::{scoped_threads_test_support_v1 as channel, Background},
};

const BODY: &str = "Original private thread body 界\ncontrol \u{1b}[2J stays data";
fn graph_parent() -> Value {
    json!({"__typename":"PullRequest","id":"GENUINE_PR_NODE_1007","fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),"repository":graph_repo()})
}
fn graph_repo() -> Value {
    json!({"id":"R_410","databaseId":410,"nameWithOwner":REPO})
}
fn thread_page(subject: Option<&str>, partial: bool) -> Value {
    let mut parent = graph_parent();
    let mut thread = json!({"id":"THREAD_ACTUAL_9","isResolved":false,"isOutdated":true,"path":"src/view.rs","diffSide":"LEFT","startDiffSide":"LEFT","line":null,"startLine":null,"originalLine":7,"originalStartLine":6,"viewerCanReply":true,"viewerCanResolve":false,"viewerCanUnresolve":false,"resolvedBy":null,"pullRequest":graph_parent(),"repository":graph_repo(),"comments":{"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[{"id":"COMMENT_ACTUAL_9","fullDatabaseId":"9009","body":BODY,"createdAt":"2026-10-01T00:00:00Z","updatedAt":"2026-10-02T00:00:00Z","path":"src/old.rs","line":null,"startLine":null,"originalLine":7,"originalStartLine":6,"commit":{"oid":"c".repeat(40)},"originalCommit":{"oid":"d".repeat(40)},"replyTo":null,"author":{"__typename":"User","databaseId":51,"login":"thread-actor","name":"Actual actor","avatarUrl":null},"pullRequest":graph_parent(),"repository":graph_repo()}]}});
    if let Some(subject) = subject {
        thread["subjectType"] = json!(subject);
    }
    parent["reviewThreads"] = json!({"totalCount":if partial {2} else {1},"nodes":[thread],"pageInfo":{"hasNextPage":partial,"endCursor":if partial {json!("next_actual")} else {Value::Null}}});
    json!({"data":{"node":parent}})
}
fn graph(value: Value) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &value.to_string(),
    );
    step.stdin = serde_json::to_vec(&json!({"query":METRICS_REVIEW_THREADS_QUERY,"variables":{"id":"GENUINE_PR_NODE_1007","first":25,"after":null,"commentsFirst":20,"includeComments":true}})).unwrap();
    step
}
fn thread_steps(subject: Option<&str>) -> Vec<GhStep> {
    let mut steps = principal(42, "octocat");
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(graph(thread_page(subject, false)));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.extend(principal(42, "octocat"));
    assert_eq!(steps.len(), 17);
    steps
}
struct Real {
    fixture: Fixture,
    app: App,
    steps: Vec<GhStep>,
    sql: Vec<String>,
    git: (String, String, Vec<u8>, Vec<u8>),
}
fn real(extra: Vec<GhStep>) -> Real {
    let mut steps = script(&resource_steps(PrResourceKindV1::Files, REPO));
    steps.extend(extra);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let (source, active, mut workspace) = opened(&fixture);
    let result = run_resource(
        &fixture,
        source.clone(),
        active.clone(),
        &mut workspace,
        PrResourceKindV1::Files,
        &Default::default(),
    )
    .unwrap();
    install_resource_if_current(&mut workspace, result).unwrap();
    let selected = (0..workspace.pr.as_ref().unwrap().rows.len())
        .find(|index| {
            matches!(
                workspace.scoped_pr_resource.as_ref().unwrap().row(*index),
                Some(PrResourceRowV1::FileList(0))
            )
        })
        .unwrap();
    let app = App {
        view: View::Inbox,
        github: Some(workspace),
        remote_scope_source: Some(source),
        remote_scope: Some(active),
        selected,
        ..Default::default()
    };
    let sql = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    Real {
        fixture,
        app,
        steps,
        sql,
        git,
    }
}
fn intact(real: &Real) {
    assert_eq!(sql_state(&real.fixture), real.sql);
    assert_eq!(git_state_resource(&real.fixture), real.git);
    let rows: Vec<Value> = fs::read_to_string(real.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    assert_eq!(rows.len(), real.steps.len());
    for (row, step) in rows.iter().zip(&real.steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        if step.args.iter().any(|arg| arg == "POST") {
            assert_eq!(step.args[1], "graphql");
            let raw: Value = serde_json::from_slice(&step.stdin).unwrap();
            assert!(raw["query"].as_str().unwrap().starts_with("query("));
            assert!(!raw["query"].as_str().unwrap().contains("mutation"));
        }
    }
    assert_eq!(
        sql(&real.fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
}
fn read(real: &Real) -> Result<Arc<threads::PreparedThreadsV1>, threads::ThreadBrowseFailureV1> {
    threads::load(
        threads::ThreadBrowseIntentV1::capture(&real.app).unwrap(),
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
}
#[test]
fn review_threads_v1_capture_is_actual_original_allocation_and_selection_without_reads() {
    let mut real = real(vec![]);
    let intent = threads::ThreadBrowseIntentV1::capture(&real.app).unwrap();
    assert_eq!(intent.path(), Some("src/view.rs"));
    assert!(intent.owns(&real.app));
    let old = real.app.selected;
    real.app.selected += 1;
    assert!(!intent.owns(&real.app));
    real.app.selected = old;
    real.app.remote_scope_source = None;
    assert!(!intent.owns(&real.app));
    assert!(!intent.owns(&App::default()));
    intact(&real);
}
#[test]
fn review_threads_v1_actual_parent_source_uid_and_original_placement_remain_read_only() {
    let real = real(thread_steps(Some("LINE")));
    let result = read(&real).unwrap();
    assert_eq!(result.identity.account_id, Some(42));
    assert_eq!(result.observation.parent.id, 1007);
    assert_eq!(result.observation.threads[0].node_id, "THREAD_ACTUAL_9");
    assert_eq!(result.observation.threads[0].placement.line, None);
    assert_eq!(
        result.observation.threads[0].placement.original_line,
        Some(7)
    );
    assert_eq!(
        result.observation.threads[0].comments[0].original_commit_sha,
        Some("d".repeat(40))
    );
    assert!(result.groups[0].summary.contains("OUTDATED"));
    assert!(result.groups[0].summary.contains("LINE"));
    assert_eq!(result.selected_file, vec![0]);
    assert!(result.groups[0].lines.iter().any(|line| line
        .text
        .contains("snapshot grants no write permission; open a fresh intent/Preview to recheck")));
    assert!(!result.groups[0]
        .lines
        .iter()
        .any(|line| line.text.contains("writes not exposed")));
    assert!(result.groups[0]
        .lines
        .iter()
        .any(|line| line.text.contains("\\u{1b}[2J")));
    assert!(!result.groups[0]
        .lines
        .iter()
        .any(|line| line.text.contains('\u{1b}')));
    intact(&real);
}
#[test]
fn review_threads_v1_resolver_is_actual_distinct_actor_and_source_timestamps_render_with_fallback()
{
    let mut steps = thread_steps(Some("LINE"));
    let mut page = thread_page(Some("LINE"), false);
    let thread = &mut page["data"]["node"]["reviewThreads"]["nodes"][0];
    thread["isResolved"] = json!(true);
    thread["resolvedBy"] = json!({"__typename":"User","databaseId":62,"login":"actual-resolver","name":"Actual Resolver","avatarUrl":null});
    steps[9] = graph(page);
    let mut real = real(steps);
    let prepared = read(&real).unwrap();
    assert_eq!(
        prepared.observation.threads[0]
            .resolved_by
            .as_ref()
            .unwrap()
            .account_id,
        Some(62)
    );
    assert_eq!(
        prepared.observation.threads[0].comments[0]
            .author
            .as_ref()
            .unwrap()
            .account_id,
        Some(51)
    );
    let resolver = prepared.groups[0]
        .lines
        .iter()
        .find(|line| line.text.starts_with("Resolver:"))
        .unwrap();
    assert!(resolver.text.contains("@actual-resolver"));
    assert!(!resolver.text.contains("thread-actor"));
    assert_eq!(resolver.actor.as_ref().unwrap().initials(), "AR");
    assert!(prepared.groups[0]
        .lines
        .iter()
        .any(|line| line.text == "Created (source UTC): 2026-10-01T00:00:00Z"));
    assert!(prepared.groups[0]
        .lines
        .iter()
        .any(|line| line.text == "Updated (source UTC): 2026-10-02T00:00:00Z"));
    let mut view = ui_v1::ThreadBrowserV1::pending(prepared.intent.clone());
    view.prepared = Some(prepared);
    real.app.scoped_threads_v1 = Some(view);
    real.app.overlay = Some(Overlay::PrThreadsV1);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            ui_v1::draw(frame, &mut real.app, area)
        })
        .unwrap();
    let screen: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    for actual in [
        "[AR]",
        "Resolver: @actual-resolver",
        "@thread-actor",
        "Created (source UTC): 2026-10-01T00:00:00Z",
        "Updated (source UTC): 2026-10-02T00:00:00Z",
    ] {
        assert!(
            screen.contains(actual),
            "Missing actual observed metadata {actual}: {screen}"
        );
    }
    intact(&real);
}
#[test]
fn review_threads_v1_null_or_deleted_resolver_and_missing_times_do_not_invent_author_or_chronology()
{
    for deleted in [false, true] {
        let mut steps = thread_steps(Some("LINE"));
        let mut page = thread_page(Some("LINE"), false);
        let thread = &mut page["data"]["node"]["reviewThreads"]["nodes"][0];
        thread["isResolved"] = json!(true);
        thread["resolvedBy"] = if deleted {
            json!({"__typename":"User","databaseId":null,"login":"ghost","name":null,"avatarUrl":null})
        } else {
            Value::Null
        };
        thread["comments"]["nodes"][0]["createdAt"] = Value::Null;
        thread["comments"]["nodes"][0]["updatedAt"] = json!("not-a-timestamp\u{1b}[2J");
        steps[9] = graph(page);
        let real = real(steps);
        let prepared = read(&real).unwrap();
        let actual = &prepared.observation.threads[0];
        assert!(actual.comments[0].created_at.is_none());
        assert!(actual.comments[0].updated_at.is_none());
        let resolver = prepared.groups[0]
            .lines
            .iter()
            .find(|line| line.text.starts_with("Resolver:"))
            .unwrap();
        assert_eq!(
            resolver.actor.as_ref().unwrap().initials(),
            if deleted { "×" } else { "?" }
        );
        assert!(!resolver.text.contains("thread-actor"));
        assert!(resolver.text.contains(if deleted {
            "@ghost [deleted]"
        } else {
            "unavailable"
        }));
        for field in ["Created", "Updated"] {
            assert!(prepared.groups[0]
                .lines
                .iter()
                .any(|line| line.text == format!("{field} (source UTC): unavailable")));
        }
        assert!(!prepared.groups[0]
            .lines
            .iter()
            .any(|line| line.text.contains("not-a-timestamp")));
        intact(&real);
    }
}
#[test]
fn review_threads_v1_file_subject_is_actual_schema_not_null_line_inference() {
    let real = real(thread_steps(Some("FILE")));
    let result = read(&real).unwrap();
    assert_eq!(
        result.observation.threads[0].subject_type,
        Some(crate::providers::gh::MetricsThreadSubjectV1::File)
    );
    assert!(result.groups[0].summary.contains("FILE"));
    intact(&real);
}
#[test]
fn review_threads_v1_idle_same_login_numeric_account_loss_retires_underlay_before_draw_or_back() {
    for draw_first in [true, false] {
        let mut real = real(thread_steps(Some("LINE")));
        let prepared = read(&real).unwrap();
        let mut view = ui_v1::ThreadBrowserV1::pending(prepared.intent.clone());
        view.prepared = Some(prepared);
        real.app.scoped_threads_v1 = Some(view);
        real.app.overlay = Some(Overlay::PrThreadsV1);
        let login = real.app.github.as_ref().unwrap().identity.login.clone();
        real.app.github.as_mut().unwrap().identity.account_id = Some(43);
        assert_eq!(real.app.github.as_ref().unwrap().identity.login, login);
        if !draw_first {
            real.app.handle_event(crossterm::event::Event::Key(
                crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Esc,
                    crossterm::event::KeyModifiers::NONE,
                ),
            ));
        }
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut real.app))
            .unwrap();
        let screen: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(
            real.app.github.is_none(),
            "Original numeric account was lost: closing the thread alone must not expose its private PR underlay"
        );
        assert!(real.app.scoped_threads_v1.is_none());
        assert!(real.app.pr_file_images.is_none());
        assert_ne!(real.app.overlay, Some(Overlay::PrThreadsV1));
        assert!(!screen.contains("thread-actor"));
        assert!(!screen.contains("PRIVATE_NATIVE_THREAD"));
        intact(&real);
    }
}
#[test]
fn review_threads_v1_retired_thread_cannot_clear_independently_newer_genuine_source_or_page() {
    let mut old = real(thread_steps(Some("LINE")));
    let prepared = read(&old).unwrap();
    let mut view = ui_v1::ThreadBrowserV1::pending(prepared.intent.clone());
    view.prepared = Some(prepared);
    old.app.scoped_threads_v1 = Some(view);
    old.app.overlay = Some(Overlay::PrThreadsV1);
    let mut newer = real(thread_steps(Some("FILE")));
    let _newer_prepared = read(&newer).unwrap();
    let source = newer.app.remote_scope_source.as_ref().unwrap().clone();
    let active = newer.app.remote_scope.as_ref().unwrap().clone();
    let rows = newer
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .rows
        .as_ptr();
    old.app.remote_scope_source = Some(source.clone());
    old.app.remote_scope = Some(active.clone());
    old.app.github = newer.app.github.take();
    assert!(ui_v1::retire_invalid(&mut old.app));
    assert!(old.app.scoped_threads_v1.is_none());
    assert!(Arc::ptr_eq(
        old.app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(Arc::ptr_eq(old.app.remote_scope.as_ref().unwrap(), &active));
    assert_eq!(
        old.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .rows
            .as_ptr(),
        rows
    );
    intact(&old);
    intact(&newer);
}
#[test]
fn review_threads_v1_omitted_and_unknown_subject_never_fabricate_file_or_line() {
    for subject in [None, Some("FUTURE\u{1b}[2J")] {
        let real = real(thread_steps(subject));
        let result = read(&real).unwrap();
        assert_eq!(result.observation.threads[0].subject_type, None);
        assert!(result.groups[0].summary.contains("SUBJECT UNKNOWN"));
        intact(&real);
    }
}
#[test]
fn review_threads_v1_same_login_reassigned_numeric_uid_refuses_before_thread_sql_body() {
    let real = real(principal(43, "octocat"));
    assert!(matches!(
        read(&real),
        Err(threads::ThreadBrowseFailureV1::Source)
    ));
    intact(&real);
}
#[test]
fn review_threads_v1_recreated_repository_id_refuses_before_any_private_thread_request() {
    let mut steps = principal(42, "octocat");
    steps.push(get(&format!("repos/{REPO}"), repo(999, REPO)));
    let real = real(steps);
    assert!(matches!(
        read(&real),
        Err(threads::ThreadBrowseFailureV1::Source)
    ));
    intact(&real);
}
#[test]
fn review_threads_v1_expired_or_canceled_original_read_starts_zero_new_children() {
    let real = real(vec![]);
    let intent = threads::ThreadBrowseIntentV1::capture(&real.app).unwrap();
    let cancel = CancellationToken::default();
    cancel.cancel();
    for (token, deadline) in [
        (cancel, Instant::now() + Duration::from_secs(30)),
        (CancellationToken::default(), Instant::now()),
    ] {
        assert!(threads::load(
            intent.clone(),
            &provider(&real.fixture),
            &database(&real.fixture),
            &token,
            deadline
        )
        .is_err());
    }
    intact(&real);
}
#[test]
fn review_threads_v1_actual_inbox_queue_install_and_back_preserve_exact_original_page() {
    let mut real = real(thread_steps(Some("LINE")));
    let rows = real
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .rows
        .as_ptr();
    native_v1::open(&mut real.app);
    let owner = real.app.scoped_threads_work_v1.take().unwrap();
    let mut background = Background::new();
    assert!(background.submit_scoped_threads_v1(
        &mut real.app,
        owner,
        provider(&real.fixture),
        database(&real.fixture)
    ));
    channel::fault_next(&mut background, 3);
    assert!(background.poll(&mut real.app));
    assert!(channel::generation(&background).is_none());
    assert!(real.app.scoped_threads_owner_v1.is_none());
    assert!(real
        .app
        .scoped_threads_v1
        .as_ref()
        .unwrap()
        .prepared
        .is_some());
    ui_v1::dispatch(&mut real.app, ui_v1::ThreadUiActionV1::Back);
    assert!(real.app.overlay.is_none());
    assert_eq!(
        real.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .rows
            .as_ptr(),
        rows
    );
    intact(&real);
}
#[test]
fn review_threads_v1_actual_mixed_and_foreign_completion_cannot_clear_slot_status_or_private_page()
{
    for mode in [0, 1] {
        let mut real = real(thread_steps(Some("LINE")));
        native_v1::open(&mut real.app);
        let owner = real.app.scoped_threads_work_v1.take().unwrap();
        let mut background = Background::new();
        assert!(background.submit_scoped_threads_v1(
            &mut real.app,
            owner.clone(),
            provider(&real.fixture),
            database(&real.fixture)
        ));
        channel::fault_next(&mut background, mode);
        let generation = channel::generation(&background);
        real.app.status = "UNCHANGED_CURRENT_THREAD_STATUS".into();
        assert!(!background.poll(&mut real.app));
        assert_eq!(channel::generation(&background), generation);
        assert_eq!(real.app.status, "UNCHANGED_CURRENT_THREAD_STATUS");
        assert!(real.app.loading[View::Inbox.index()]);
        assert!(Arc::ptr_eq(
            real.app.scoped_threads_owner_v1.as_ref().unwrap(),
            &owner
        ));
        assert!(background.cancel_scoped_threads_v1(&mut real.app));
        intact(&real);
    }
}
#[test]
fn review_threads_v1_newer_back_intent_vetoes_actual_success_and_error_before_any_effect() {
    for mode in [2, 3] {
        let mut real = real(thread_steps(Some("LINE")));
        native_v1::open(&mut real.app);
        let owner = real.app.scoped_threads_work_v1.take().unwrap();
        let mut background = Background::new();
        assert!(background.submit_scoped_threads_v1(
            &mut real.app,
            owner,
            provider(&real.fixture),
            database(&real.fixture)
        ));
        channel::fault_next(&mut background, mode);
        let generation = channel::generation(&background);
        real.app.github_action = Some(GithubAction::Back);
        real.app.status = "NEWER_BACK_MUST_SURVIVE".into();
        assert!(!background.poll(&mut real.app));
        assert_eq!(channel::generation(&background), generation);
        assert_eq!(real.app.status, "NEWER_BACK_MUST_SURVIVE");
        assert_eq!(real.app.github_action, Some(GithubAction::Back));
        assert!(real.app.github.is_some());
        real.app.github_action = None;
        assert!(background.cancel_scoped_threads_v1(&mut real.app));
        intact(&real);
    }
}
