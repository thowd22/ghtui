//! Native controller contracts over bounded local intent and actual workers.
//! Local DTOs below exercise actual intent/UI ownership, NOT a provider proof.
//! No PrCheckRunObservation/PrChecksRoute is fabricated; native PTYs are separate.
use super::*;
use crate::{
    app::Overlay,
    github_workspace::{Freshness, PrPage},
    providers::gh::{Actor, Coverage, PrDetails, PullRequest},
    runtime::Background,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};

fn identity(id: u64) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "same-login".into(),
        account_id: Some(id),
        gh_version: "fixture".into(),
    }
}
fn summary() -> PrSummary {
    PrSummary {
        pr: PullRequest {
            host: "github.invalid".into(),
            id: "PR_checks_7".into(),
            repository: "example/repo".into(),
            number: 7,
            title: "Checked original PR".into(),
            body: "PRIVATE_SOURCE_BODY".into(),
            url: "https://github.invalid/example/repo/pull/7".into(),
            author: Actor {
                login: "author".into(),
                actor_type: "User".into(),
                ..Default::default()
            },
            state: "open".into(),
            draft: false,
            created_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-13T00:00:00Z".into(),
            queues: Default::default(),
        },
        head_sha: "a".repeat(40),
        base_sha: "b".repeat(40),
        head_ref: "Feature/雪".into(),
        base_ref: "main".into(),
        merged: false,
        auto_merge: false,
        merge_commit_sha: None,
        mergeable: None,
        mergeable_state: "unknown".into(),
        additions: 0,
        deletions: 0,
        changed_files: 0,
    }
}
fn github() -> GithubWorkspace {
    let mut github = GithubWorkspace::empty(
        identity(42),
        vec!["aaa/unrelated".into(), "example/repo".into()],
        Default::default(),
    );
    let original = summary();
    let checks = vec![
        Check {
            id: 600,
            name: "Exact failed CI".into(),
            status: "completed".into(),
            conclusion: "failure".into(),
            url: "https://github.invalid/aaa/unrelated/actions/runs/999".into(),
            head_sha: original.head_sha.clone(),
            kind: "check".into(),
        },
        Check {
            id: 601,
            name: "Another real check".into(),
            status: "in_progress".into(),
            conclusion: String::new(),
            url: String::new(),
            head_sha: original.head_sha.clone(),
            kind: "check".into(),
        },
        Check {
            id: 0,
            name: "Legacy status context".into(),
            status: "pending".into(),
            conclusion: String::new(),
            url: "https://github.invalid/example/repo".into(),
            head_sha: original.head_sha.clone(),
            kind: "status".into(),
        },
    ];
    let mut page = PrPage {
        details: PrDetails {
            summary: original,
            comments: vec![],
            commits: vec![],
            files: vec![],
            reviews: vec![],
            timeline: vec![],
            coverage: Coverage::default(),
        },
        tab: PrTab::Checks,
        diff: None,
        checks: Some(checks),
        required_checks: None,
        rows: vec![],
        context: None,
        metadata_display_v51: None,
        review_progress: None,
    };
    page.prepare_rows().unwrap();
    github.pr = Some(page);
    github.freshness = Freshness::Cached { fetched_at: 1 };
    github
}
fn app() -> App {
    let github = github();
    App {
        view: View::Inbox,
        context: github.context(),
        github: Some(github),
        github_content_focused: true,
        ..Default::default()
    }
}
fn key(code: KeyCode) -> Event {
    Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
}
fn click(rect: Rect) -> Event {
    Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: rect.x,
        row: rect.y,
        modifiers: KeyModifiers::NONE,
    })
}
fn draw(app: &mut App, width: u16, height: u16) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal
}
fn screen(terminal: &Terminal<TestBackend>) -> String {
    let buffer = terminal.backend().buffer();
    (0..buffer.area.height)
        .map(|y| {
            (0..buffer.area.width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn hit(app: &App, action: &Action) -> Rect {
    app.hits
        .iter()
        .find(|hit| &hit.action == action)
        .expect("Actual rendered shared action has a hit")
        .rect
}
fn assert_no_unrelated_route(app: &App) {
    assert!(app.actions.is_none());
    assert!(app.actions_action.is_none());
    assert!(!app.actions_load_requested);
    assert!(app.github_action.is_none());
    assert!(app.pr_action.is_none());
}

#[test]
fn pr_checks_native_capture_excludes_unbounded_presentation_before_copy_and_retains_exact_facts() {
    let mut github = github();
    let page = github.pr.as_mut().unwrap();
    let huge = "PRIVATE_PRESENTATION_CANARY".repeat(100_000);
    page.details.summary.pr.title = huge.clone();
    page.details.summary.pr.body = huge.clone();
    page.details.summary.pr.author.login = huge.clone();
    page.details.summary.pr.author.display_name = Some(huge.clone());
    page.details.summary.pr.author.avatar_url = Some(huge);
    let origin = PrChecksOrigin::capture(&github, 0, 3).unwrap();
    assert!(origin.summary.pr.title.is_empty());
    assert!(origin.summary.pr.body.is_empty());
    assert_eq!(origin.summary.pr.author, Actor::default());
    assert!(origin.summary.pr.url.is_empty());
    assert!(origin.summary.pr.created_at.is_empty());
    assert_eq!(origin.summary.pr.id, "PR_checks_7");
    assert_eq!(origin.summary.pr.repository, "example/repo");
    assert_eq!(origin.summary.head_sha, "a".repeat(40));
    assert_eq!(origin.summary.base_sha, "b".repeat(40));
    assert_eq!(origin.summary.head_ref, "Feature/雪");
    assert_eq!(origin.summary.base_ref, "main");
    assert_eq!(origin.identity.account_id, Some(42));
    assert_eq!(origin.check.id, 600);
    assert_eq!((origin.row, origin.top), (0, 3));
    assert!(!format!("{origin:?}").contains("PRIVATE_"));
    // Only bounded semantic facts are admitted, not unlimited hidden comparisons.
    github.pr.as_mut().unwrap().checks.as_mut().unwrap()[0].name = "雪".repeat(1400);
    assert!(PrChecksOrigin::capture(&github, 0, 0).is_err());
}

#[test]
fn pr_checks_native_live_arc_not_equal_text_and_exact_row_source_numeric_owner_must_match() {
    let mut current = app();
    current.dispatch(Action::PrChecks(PrChecksUiAction::Open));
    let owner = current.pr_checks_origin.clone().unwrap();
    assert!(owns_origin(&current, &owner));
    let equal_text =
        Arc::new(PrChecksOrigin::capture(current.github.as_ref().unwrap(), 0, 0).unwrap());
    assert!(!owns_origin(&current, &equal_text));
    assert!(owns_origin(&current, &owner.clone()));
    current.github.as_mut().unwrap().identity.login = "genuine-renamed-login".into();
    assert!(owns_origin(&current, &owner));
    for change in 0..8 {
        let mut changed = app();
        changed.pr_checks_origin = Some(owner.clone());
        let github = changed.github.as_mut().unwrap();
        if change == 0 {
            github.identity.account_id = Some(43);
        } else if change == 1 {
            github.identity.account_id = None;
        } else {
            let page = github.pr.as_mut().unwrap();
            match change {
                2 => page.checks.as_mut().unwrap().swap(0, 1),
                3 => page.details.summary.head_sha = "c".repeat(40),
                4 => page.details.summary.head_ref = "feature/雪".into(),
                5 => page.details.summary.base_sha = "d".repeat(40),
                6 => page.details.summary.pr.id = "PR_other".into(),
                7 => page.tab = PrTab::Summary,
                _ => unreachable!(),
            }
        }
        assert!(
            !owns_origin(&changed, &owner),
            "Changed source case{change} must lose ownership"
        );
    }
    current.dispatch(Action::GithubRow(1));
    assert!(current.pr_checks_origin.is_none());
    assert!(!owns_origin(&current, &owner));
    assert_no_unrelated_route(&current);
}

#[test]
fn pr_checks_native_testbackend_row_focus_open_key_click_and_disabled_status_at_three_widths() {
    for (width, height) in [(120, 40), (80, 24), (40, 24)] {
        for mouse in [false, true] {
            let mut current = app();
            let terminal = draw(&mut current, width, height);
            assert!(screen(&terminal).contains("Exact failed CI"));
            let row = hit(&current, &Action::GithubRow(0));
            current.handle_event(click(row));
            assert!(current.github_content_focused);
            assert_eq!(current.inbox_list.selected, 0);
            assert!(!current.pr_checks_request);
            let terminal = draw(&mut current, width, height);
            let open = hit(&current, &Action::PrChecks(PrChecksUiAction::Open));
            assert!(open.right() <= width && open.bottom() <= height);
            // The key-bar token, compact below 100 columns.
            assert!(
                screen(&terminal).contains(if width < crate::footer::COMPACT_WIDTH {
                    "Enter Check"
                } else {
                    "Enter Open check"
                })
            );
            if mouse {
                current.handle_event(click(open));
            } else {
                current.handle_event(key(KeyCode::Enter));
            }
            assert!(current.pr_checks_request);
            let origin = current.pr_checks_origin.as_ref().unwrap();
            assert_eq!(origin.summary.pr.repository, "example/repo");
            assert_eq!(origin.check.id, 600);
            assert!(owns_origin(&current, origin));
            let catalogue =
                crate::navigation_catalogue::PreparedCatalogue::prepare(&current, 9).unwrap();
            assert_eq!(
                catalogue
                    .entries
                    .iter()
                    .find(|entry| entry.id == "pr.check.selected")
                    .unwrap()
                    .action,
                Action::PrChecks(PrChecksUiAction::Open)
            );
            assert_no_unrelated_route(&current);
            assert!(
                current.pr_checks_route.is_none(),
                "Local intent is not an API correlation proof"
            );
        }
        for mouse in [false, true] {
            let mut current = app();
            current.inbox_list.selected = 2;
            draw(&mut current, width, height);
            let row = hit(&current, &Action::GithubRow(2));
            current.handle_event(click(row));
            draw(&mut current, width, height);
            if mouse {
                // Disabled control exposes its same semantic reason, never a run.
                // A disabled Open check leaves the key bar (no dead click target); the palette
                // entry carries the same semantic reason, and so does its Enter key.
                assert!(!current.hits.iter().any(|hit| matches!(&hit.action, Action::Explain(reason) if reason.contains("status") || reason.contains("CheckRun"))));
                let catalogue =
                    crate::navigation_catalogue::PreparedCatalogue::prepare(&current, 9).unwrap();
                let entry = catalogue
                    .entries
                    .iter()
                    .find(|entry| entry.id == "pr.check.selected")
                    .unwrap()
                    .action
                    .clone();
                current.dispatch(entry);
            } else {
                current.handle_event(key(KeyCode::Enter));
            }
            assert!(!current.pr_checks_request);
            assert!(current.pr_checks_origin.is_none());
            assert_no_unrelated_route(&current);
            assert_eq!(current.overlay, Some(Overlay::Notice));
            let catalogue =
                crate::navigation_catalogue::PreparedCatalogue::prepare(&current, 9).unwrap();
            assert!(
                matches!(&catalogue.entries.iter().find(|entry| entry.id == "pr.check.selected").unwrap().action, Action::Explain(reason) if reason.contains("CheckRun"))
            );
            let shown = screen(&draw(&mut current, width, height));
            assert!(
                shown.contains("status") || shown.contains("CheckRun"),
                "Disabled source reason must remain visible at{width}"
            );
        }
    }
}

#[test]
fn pr_checks_native_accepted_github_or_private_invalidation_retires_original_before_late_admission()
{
    for outcome in 0..3 {
        let mut current = app();
        current.dispatch(Action::PrChecks(PrChecksUiAction::Open));
        let owner = current.pr_checks_origin.clone().unwrap();
        let mut background = Background::new();
        assert!(background.submit(&mut current, View::Inbox, move |_| {
            Ok(match outcome {
                0 => PreparedView {
                    github: Some(github()),
                    status: "Fresh exact Github source accepted".into(),
                    ..Default::default()
                },
                1 => PreparedView {
                    clear_private: true,
                    status: "Current identity unavailable • previous private pages hidden".into(),
                    ..Default::default()
                },
                2 => PreparedView {
                    invalidate_pr_metadata: true,
                    status: "Original metadata changed • previous source hidden".into(),
                    ..Default::default()
                },
                _ => unreachable!(),
            })
        }));
        let deadline = Instant::now() + Duration::from_secs(3);
        while current.loading[View::Inbox.index()] {
            background.poll(&mut current);
            assert!(
                Instant::now() < deadline,
                "Actual prepared worker completion must arrive"
            );
            std::thread::yield_now();
        }
        assert!(current.pr_checks_origin.is_none());
        assert!(current.pr_checks_route.is_none());
        assert!(!current.pr_checks_request);
        assert!(!owns_origin(&current, &owner));
        assert_no_unrelated_route(&current);
        if outcome == 0 {
            assert!(current.github.is_some());
        } else {
            assert!(current.github.is_none());
        }
        // The old pointer is still readable local metadata, but cannot pass the
        // exact live-owner gate used by late source publication. No fake route.
        assert_eq!(owner.check.id, 600);
        assert!(!owns_origin(&current, &owner.clone()));
    }
}

#[test]
fn pr_checks_native_denied_current_repository_never_initializes_or_reads_a_cold_private_ledger() {
    use crate::test_support::{Fixture, GhStep};
    let fixture = Fixture::new();
    let user = GhStep::read(
        &[
            "api",
            "user",
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        r#"{"id":42,"login":"same-login"}"#,
    );
    let mut denied = GhStep::read(
        &[
            "api",
            "repos/example/repo",
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        "",
    );
    denied.status = 1;
    denied.stderr = b"gh: PRIVATE_DENIED_REPO_DIAGNOSTIC (HTTP 403)".to_vec();
    fixture.scripted_gh(&[
        GhStep::read(&["--version"], "gh version 2.71.0 (fixture)\n"),
        user.clone(),
        user,
        denied,
    ]);
    let provider = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let original = Arc::new(PrChecksOrigin::capture(&github(), 0, 0).unwrap());
    let database = fixture.root.join("cold-private.sqlite3");
    let view = load(
        original.clone(),
        &provider,
        &database,
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(view.clear_private);
    assert!(Arc::ptr_eq(
        view.pr_checks_origin.as_ref().unwrap(),
        &original
    ));
    assert!(view.actions.is_none() && view.pr_checks_route.is_none());
    assert!(!view.status.contains("PRIVATE_DENIED_REPO_DIAGNOSTIC"));
    assert!(!database.exists(), "Numeric identity alone must not open/create a private checks ledger before genuine repository/PR/check access");
    let rows: Vec<serde_json::Value> =
        std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
    assert_eq!(rows.len(), 4);
    assert!(rows
        .iter()
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    assert!(!fixture.root.join("cold-private.sqlite3-wal").exists());
    assert!(!fixture.root.join("cold-private.sqlite3-shm").exists());
}

#[test]
fn pr_checks_native_late_failure_is_bound_to_original_owner_and_cannot_clear_new_private_context() {
    assert!(
        unavailable_action(&ActionsAction::Watch).is_some(),
        "Watch must not bypass the owned route's fresh read bracket through ordinary polling"
    );
    assert!(unavailable_action(&ActionsAction::Refresh).is_some());
    assert!(unavailable_action(&ActionsAction::Logs(Some(501))).is_none());
    for replacement in 0..3 {
        let mut current = app();
        current.dispatch(Action::PrChecks(PrChecksUiAction::Open));
        let original = current.pr_checks_origin.clone().unwrap();
        let (release, blocked) = std::sync::mpsc::channel();
        let mut background = Background::new();
        assert!(background.submit(&mut current, View::Inbox, move |_| {
            blocked.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(hidden(original))
        }));
        match replacement {
            0 => current.dispatch(Action::View(View::Repository)),
            1 => {
                // Identical local facts with a different source owner are not
                // the original request, even while its generation is current.
                current.pr_checks_origin = Some(Arc::new(
                    PrChecksOrigin::capture(current.github.as_ref().unwrap(), 0, 0).unwrap(),
                ));
            }
            2 => {} // A still-owned failure must clear its private page.
            _ => unreachable!(),
        }
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while current.loading[View::Inbox.index()] {
            background.poll(&mut current);
            assert!(
                Instant::now() < deadline,
                "The actual blocked completion must arrive"
            );
            std::thread::yield_now();
        }
        if replacement == 2 {
            assert!(current.github.is_none());
            assert!(current.pr_checks_origin.is_none());
            assert_eq!(
                current.status,
                "PR check context changed • private pages hidden"
            );
        } else {
            assert!(
                current.github.is_some(),
                "A retired check failure must not erase another context"
            );
            assert!(current.status.contains("superseded"));
            if replacement == 1 {
                assert!(current.pr_checks_origin.is_some());
            }
        }
    }
}

#[test]
fn pr_checks_native_overlay_resize_stale_hits_and_unsupported_back_never_redirect_default_repo() {
    let mut current = app();
    draw(&mut current, 120, 40);
    let stale_open = hit(&current, &Action::PrChecks(PrChecksUiAction::Open));
    current.dispatch(Action::Help);
    draw(&mut current, 120, 40);
    current.handle_event(click(stale_open));
    assert!(current.pr_checks_origin.is_none());
    assert!(!current.pr_checks_request);
    assert_no_unrelated_route(&current);
    current.handle_event(key(KeyCode::Esc));
    draw(&mut current, 120, 40);
    let old_open = hit(&current, &Action::PrChecks(PrChecksUiAction::Open));
    current.handle_event(Event::Resize(40, 24));
    assert!(
        current.hits.is_empty(),
        "Geometry change invalidates old click targets before redraw"
    );
    current.handle_event(click(old_open));
    assert!(current.pr_checks_origin.is_none());
    assert!(!current.pr_checks_request);
    assert_no_unrelated_route(&current);
    draw(&mut current, 40, 24);
    current.dispatch(Action::PrChecks(PrChecksUiAction::Back));
    assert!(current.pr_checks_origin.is_none());
    assert!(current.pr_checks_route.is_none());
    assert_no_unrelated_route(&current);
    assert_eq!(current.view, View::Inbox);
    assert!(current.notice.contains("Original PR Checks source changed"));
    assert!(!current.context.contains("aaa/unrelated / Actions"));
}
