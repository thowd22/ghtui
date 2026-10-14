//! Genuine original-scope/domain/widget contracts for observed PR sections.
use super::*;
use crate::scoped_pr_observed_tabs_v1::{
    install_if_current, prepare as prepare_page, take_input, ObservedPrTabFailureV1,
    ObservedPrTabV1,
};

#[derive(Debug)]
struct TestPreparation {
    workspace: GithubWorkspace,
    original_source: Arc<CatalogueIntentSource>,
    original_active: Arc<ActiveRemoteScopeV1>,
    display_limited: bool,
}
// Test-only convenience around the production PAGE-ONLY move/install seam.
// It never manufactures a navigation tuple, source, current access or principal.
fn prepare(
    original: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    mut workspace: GithubWorkspace,
    tab: ObservedPrTabV1,
    cancel: &CancellationToken,
) -> Result<TestPreparation, ObservedPrTabFailureV1> {
    let input = take_input(&mut workspace, tab)?;
    let result = prepare_page(original, active, input, cancel)?;
    let original_source = result.original_source.clone();
    let original_active = result.original_active.clone();
    let display_limited = result.display_limited;
    install_if_current(&mut workspace, result)?;
    Ok(TestPreparation {
        workspace,
        original_source,
        original_active,
        display_limited,
    })
}

fn observed_fixture(
    name: &str,
    body: Option<&str>,
) -> (
    Fixture,
    Arc<CatalogueIntentSource>,
    Arc<ActiveRemoteScopeV1>,
    GithubWorkspace,
    Vec<GhStep>,
) {
    let fixture = Fixture::new();
    let mut steps = source_steps();
    let mut op = operation(name, 42);
    if let Some(body) = body {
        op[4] = list(
            &format!("repos/{name}/issues/7/comments?per_page=100"),
            json!([{"id":301,"node_id":"COMMENT_301","body":body,"user":{"id":52,"login":"comment-author","type":"User"},"created_at":"2026-10-02T00:00:00Z"}]),
        );
    }
    steps.extend(op);
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let scope = active(&original);
    let result = call(
        &fixture,
        original.clone(),
        scope.clone(),
        previous(&original),
    )
    .unwrap();
    assert!(result.workspace.scoped_pr_navigation.is_some());
    (fixture, original, scope, result.workspace, steps)
}
#[test]
fn scoped_pr_observed_tabs_large_comment_shares_one_payload_and_preserves_exact_action_identity() {
    let body = (0..10_000)
        .map(|i| format!("Comment line {i:05} • 界🦀\n"))
        .collect::<String>();
    let (fixture, original, scope, workspace, steps) = observed_fixture(REPO, Some(&body));
    let before = sql_state(&fixture);
    let result = prepare(
        original.clone(),
        scope.clone(),
        workspace,
        ObservedPrTabV1::Conversation,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(!result.display_limited);
    let page = result.workspace.pr.as_ref().unwrap();
    let payloads = page
        .rows
        .iter()
        .filter_map(|row| row.comment.as_ref())
        .collect::<Vec<_>>();
    assert_eq!(payloads.len(), 10_001);
    let first = payloads[0];
    assert!(payloads.iter().all(|payload| Arc::ptr_eq(first, payload)));
    assert_eq!(first.observed.body, body);
    assert_eq!(first.observed.id, 301);
    assert_eq!(first.observed.node_id, "COMMENT_301");
    assert_eq!(
        Arc::strong_count(first),
        10_001,
        "one payload allocation shared by actual header and body rows"
    );
    let selected = page
        .rows
        .iter()
        .position(|row| row.label.contains("Comment line 09999"))
        .unwrap();
    let mut app = App {
        github: Some(result.workspace),
        selected,
        ..Default::default()
    };
    let context = app.pr_context().unwrap();
    let comment = context.comment.unwrap();
    assert_eq!(comment.observed.id, 301);
    assert_eq!(comment.observed.node_id, "COMMENT_301");
    assert_eq!(comment.observed.body, body);
    let owner = app.github.as_ref().unwrap().pr.as_ref().unwrap().rows[selected]
        .comment
        .as_ref()
        .unwrap();
    assert!(context.workspace.pr.as_ref().unwrap().rows[selected]
        .comment
        .as_ref()
        .is_some_and(|copy| Arc::ptr_eq(owner, copy)));
    app.dispatch(Action::Github(GithubAction::Back));
    assert!(app.github_action.is_none() && app.pr_action.is_none());
    assert!(app.github.as_ref().unwrap().scoped_pr_navigation.is_none());
    exact(&fixture, &steps);
    assert_eq!(sql_state(&fixture), before);
}
#[test]
fn scoped_pr_observed_tabs_all_four_sections_zero_extra_children_sql_and_same_whole_owner_pair() {
    let (fixture, original, scope, mut workspace, steps) = observed_fixture(REPO, None);
    let before = sql_state(&fixture);
    for tab in [
        ObservedPrTabV1::Conversation,
        ObservedPrTabV1::Commits,
        ObservedPrTabV1::Timeline,
        ObservedPrTabV1::Summary,
    ] {
        let result = prepare(
            original.clone(),
            scope.clone(),
            workspace,
            tab,
            &CancellationToken::default(),
        )
        .unwrap();
        assert!(!result.display_limited);
        assert!(Arc::ptr_eq(&result.original_active, &scope));
        assert!(Arc::ptr_eq(&result.original_source, &original));
        workspace = result.workspace;
        let page = workspace.pr.as_ref().unwrap();
        assert_eq!(page.tab, tab.tab());
        assert!(
            page.context.is_none()
                && page.diff.is_none()
                && page.checks.is_none()
                && page.review_progress.is_none()
        );
        if tab == ObservedPrTabV1::Commits {
            let text = page
                .rows
                .iter()
                .map(|row| row.label.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            assert!(text.contains("Parent links unavailable or none in this response"));
            assert!(!text.contains("first saved version"));
        }
        assert!(!workspace.inbox.coverage.complete);
        assert_eq!(workspace.inbox.coverage.reasons, [PREVIOUS_INBOX_NOTICE]);
        exact(&fixture, &steps);
        assert_eq!(sql_state(&fixture), before);
    }
    for tab in [
        PrTab::Files,
        PrTab::Checks,
        PrTab::Metadata,
        PrTab::Progress,
    ] {
        assert!(ObservedPrTabV1::from_tab(tab).is_none());
    }
}
#[test]
fn scoped_pr_observed_tabs_missing_foreign_or_moving_captured_page_never_substitutes() {
    for mode in 0..8 {
        let (fixture, original, scope, mut workspace, steps) = observed_fixture(REPO, None);
        let before = sql_state(&fixture);
        match mode {
            0 => workspace.scoped_pr_navigation = None,
            1 => {
                workspace.pr.as_mut().unwrap().details.summary.pr.repository =
                    "foreign/private".into()
            }
            2 => workspace.pr.as_mut().unwrap().details.summary.pr.id = "OTHER_PR".into(),
            3 => workspace.pr.as_mut().unwrap().details.summary.head_sha = "e".repeat(40),
            4 => workspace.selected.as_mut().unwrap().number = 99,
            5 => workspace.pr = None,
            6 => workspace.pr.as_mut().unwrap().details.summary.base_sha = "f".repeat(40),
            _ => workspace.pr.as_mut().unwrap().details.summary.pr.number = 99,
        };
        let error = prepare(
            original,
            scope,
            workspace,
            ObservedPrTabV1::Conversation,
            &CancellationToken::default(),
        )
        .unwrap_err();
        assert_eq!(error, ObservedPrTabFailureV1::SelectedPageUnavailable);
        assert!(!error.to_string().contains(FRESH));
        exact(&fixture, &steps);
        assert_eq!(sql_state(&fixture), before);
    }
}
#[test]
fn scoped_pr_observed_tabs_same_id_canonical_rename_preserves_original_cached_back_and_scope() {
    let (fixture, original, scope, workspace, steps) = observed_fixture("example/renamed", None);
    let before = sql_state(&fixture);
    let result = prepare(
        original.clone(),
        scope.clone(),
        workspace,
        ObservedPrTabV1::Conversation,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(!result.display_limited);
    assert_eq!(
        result
            .workspace
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .repository,
        "example/renamed"
    );
    assert_eq!(result.workspace.selected.as_ref().unwrap().repository, REPO);
    assert!(Arc::ptr_eq(&result.original_active, &scope));
    let mut app = App {
        github: Some(result.workspace),
        remote_scope: Some(scope),
        remote_scope_source: Some(original),
        ..Default::default()
    };
    app.dispatch(Action::Github(GithubAction::Back));
    assert!(
        app.github.as_ref().unwrap().pr.is_none()
            && app.github.as_ref().unwrap().scoped_pr_navigation.is_none()
    );
    assert_eq!(
        app.github
            .as_ref()
            .unwrap()
            .selected
            .as_ref()
            .unwrap()
            .repository,
        REPO
    );
    exact(&fixture, &steps);
    assert_eq!(sql_state(&fixture), before);
}
#[test]
fn scoped_pr_observed_tabs_large_array_refuses_before_rows_and_retains_original_summary_without_truncation(
) {
    let body = "x\n".repeat(70_000);
    let (fixture, original, scope, workspace, steps) = observed_fixture(REPO, Some(&body));
    let before = sql_state(&fixture);
    let old_rows = workspace
        .pr
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .map(|row| row.label.clone())
        .collect::<Vec<_>>();
    let result = prepare(
        original,
        scope,
        workspace,
        ObservedPrTabV1::Conversation,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(result.display_limited);
    let page = result.workspace.pr.unwrap();
    assert_eq!(page.tab, PrTab::Summary);
    assert_eq!(
        page.rows
            .iter()
            .map(|row| row.label.clone())
            .collect::<Vec<_>>(),
        old_rows
    );
    assert_eq!(page.details.comments[0].body, body);
    assert!(page.rows.iter().all(|row| row.comment.is_none()));
    exact(&fixture, &steps);
    assert_eq!(sql_state(&fixture), before);
}
#[test]
fn scoped_pr_observed_tabs_text_cap_and_exact_page_only_install_cannot_retarget() {
    let (fixture, original, scope, mut workspace, steps) = observed_fixture(REPO, None);
    let before = sql_state(&fixture);
    let cached = workspace.inbox.pull_requests.as_ptr();
    let input = take_input(&mut workspace, ObservedPrTabV1::Conversation).unwrap();
    assert!(workspace.pr.is_none());
    assert_eq!(workspace.inbox.pull_requests.as_ptr(), cached);
    let result = prepare_page(original, scope, input, &CancellationToken::default()).unwrap();
    let old_navigation = workspace.scoped_pr_navigation.as_ref().unwrap();
    workspace.scoped_pr_navigation = Some(Arc::new(old_navigation.as_ref().clone()));
    assert_eq!(
        install_if_current(&mut workspace, result).unwrap_err(),
        ObservedPrTabFailureV1::SelectedPageUnavailable
    );
    assert!(workspace.pr.is_none());
    assert_eq!(workspace.inbox.pull_requests.as_ptr(), cached);
    exact(&fixture, &steps);
    assert_eq!(sql_state(&fixture), before);
    let (fixture, original, scope, mut workspace, steps) = observed_fixture(REPO, None);
    let before = sql_state(&fixture);
    let page = workspace.pr.as_mut().unwrap();
    // Defensive large-display input, attached to an actually factory-observed
    // review; it cannot mint source/resource authority or enter a writer.
    page.details.reviews[0]["body"] = json!("🦀".repeat(400_000));
    let old = page
        .rows
        .iter()
        .map(|row| row.label.clone())
        .collect::<Vec<_>>();
    let result = prepare(
        original,
        scope,
        workspace,
        ObservedPrTabV1::Conversation,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(result.display_limited);
    assert_eq!(result.workspace.pr.as_ref().unwrap().tab, PrTab::Summary);
    assert_eq!(
        result
            .workspace
            .pr
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|row| row.label.clone())
            .collect::<Vec<_>>(),
        old
    );
    exact(&fixture, &steps);
    assert_eq!(sql_state(&fixture), before);
}
#[test]
fn scoped_pr_observed_tabs_cancelled_or_unresolved_original_scope_never_broadens_or_reads() {
    for mode in 0..3 {
        let (fixture, original, scope, mut workspace, steps) = observed_fixture(REPO, None);
        let before = sql_state(&fixture);
        let token = CancellationToken::default();
        let scope = if mode == 1 {
            token.cancel();
            scope
        } else if mode == 0 {
            Arc::new(ActiveRemoteScopeV1::explicit_selection(original.source(), &[]).unwrap())
        } else {
            // Same login is no principal continuity. Pure preparation does not
            // probe/authenticate/rebase or render the old42 observed body.
            workspace.identity.account_id = Some(43);
            scope
        };
        let error = prepare(
            original,
            scope,
            workspace,
            ObservedPrTabV1::Timeline,
            &token,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            ObservedPrTabFailureV1::Cancelled | ObservedPrTabFailureV1::OriginalScopeUnavailable
        ));
        exact(&fixture, &steps);
        assert_eq!(sql_state(&fixture), before);
    }
}
#[test]
fn scoped_pr_observed_tabs_pending_back_is_visible_clickable_keyboard_accessible_and_resize_safe() {
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::{backend::TestBackend, Terminal};
    for mouse in [false, true] {
        let (fixture, original, scope, mut workspace, steps) = observed_fixture(REPO, None);
        let before = sql_state(&fixture);
        let input = take_input(&mut workspace, ObservedPrTabV1::Conversation).unwrap();
        let mut app = App {
            view: crate::app::View::Inbox,
            github: Some(workspace),
            remote_scope: Some(scope.clone()),
            remote_scope_source: Some(original.clone()),
            loading: [true, false, false, false, false],
            ..Default::default()
        };
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        let old_hit = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::Github(GithubAction::Back))
            .unwrap()
            .rect;
        assert!(!app.hits.iter().any(|hit| matches!(
            hit.action,
            Action::Github(GithubAction::Filter(_)) | Action::Github(GithubAction::Open(_))
        )));
        terminal.backend_mut().resize(80, 24);
        terminal
            .resize(ratatui::layout::Rect::new(0, 0, 80, 24))
            .unwrap();
        app.handle_event(Event::Resize(80, 24));
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        let back = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::Github(GithubAction::Back))
            .unwrap()
            .rect;
        assert!(back.right() <= 80 && back.bottom() <= 24);
        assert_eq!(
            back, old_hit,
            "same prepared Back semantics survive narrower terminal"
        );
        if mouse {
            app.handle_event(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: back.x + 1,
                row: back.y,
                modifiers: KeyModifiers::NONE,
            }));
        } else {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char('b'),
                KeyModifiers::NONE,
            )));
        }
        assert!(app.scoped_pr_tab_cancel_requested);
        assert!(app.status.contains("Previous cached Inbox • no new reads"));
        assert!(!app.status.contains("Loading Inbox"));
        assert!(!app.loading[crate::app::View::Inbox.index()]);
        assert!(app.github.as_ref().unwrap().scoped_pr_navigation.is_none());
        let late = prepare_page(original, scope, input, &CancellationToken::default()).unwrap();
        assert_eq!(
            install_if_current(app.github.as_mut().unwrap(), late).unwrap_err(),
            ObservedPrTabFailureV1::SelectedPageUnavailable
        );
        assert!(app.github.as_ref().unwrap().pr.is_none());
        exact(&fixture, &steps);
        assert_eq!(sql_state(&fixture), before);
    }
}

#[test]
fn scoped_pr_observed_tabs_initial_summary_actual_factory_row_cap_is_display_refusal_not_empty_or_permission_denial(
) {
    let fixture = Fixture::new();
    let mut steps = source_steps();
    let mut op = operation(REPO, 42);
    let mut large = summary(REPO);
    large["body"] = json!("x\n".repeat(70_000));
    op[3] = get(&format!("repos/{REPO}/pulls/7"), large.clone());
    op[9] = get(&format!("repos/{REPO}/pulls/7"), large);
    steps.extend(op);
    fixture.scripted_gh(&steps);
    let original = load(&fixture);
    let active = active(&original);
    let before = sql_state(&fixture);
    let previous = previous(&original);
    let previous_body = previous.inbox.pull_requests[0].body.clone();
    let error = call(&fixture, original.clone(), active, previous).unwrap_err();
    assert!(matches!(
        error,
        ScopedPrReadFailureV1::PreparedDisplayUnavailable
    ));
    assert!(error.to_string().contains("display bound"));
    assert!(!error.to_string().contains("permission") && !error.to_string().contains(FRESH));
    assert_eq!(previous_body, OLD);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(original.scope(), &Scope::github(HOST, Some(42)).unwrap());
    exact(&fixture, &steps);
    assert_eq!(
        steps.len(),
        20,
        "closing original SQL/account proof happened before pure display refusal; no retries"
    );
}
