//! TASK-92 list plus preview split: side by side from 120 columns, drill-down below; the Inbox
//! preview shows only the cached PR summary (no read is queued while the cursor moves); the
//! Actions Runs preview shows the selected run and its jobs once loaded; Tab, Shift-Tab and a
//! click move the pane focus, whose `▶` marker and accent bar follow it.
use super::*;
use crate::{
    actions_workspace::{RunPage, ScopedRun},
    github_workspace::{GithubAction, GithubWorkspace, PrKey},
    providers::gh::{ActionsJob, Actor, HostIdentity, PullRequest, Queue, RunAttempt, WorkflowRun},
    ui_chrome::{split_list_preview, FOCUS_MARKER, SPLIT_MIN_WIDTH},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};
use std::collections::BTreeSet;

fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "thowd22".into(),
        account_id: Some(42),
        gh_version: "offline".into(),
    }
}

fn actor(login: &str) -> Actor {
    Actor {
        login: login.into(),
        display_name: None,
        avatar_url: None,
        actor_type: "User".into(),
    }
}

fn pull_request(number: u64, title: &str, author: &str) -> PullRequest {
    PullRequest {
        host: "github.invalid".into(),
        id: format!("PR_{number}"),
        repository: "example/actual".into(),
        number,
        title: title.into(),
        body: format!("Body of pull request {number}.\nSecond line {number}."),
        url: format!("https://github.invalid/example/actual/pull/{number}"),
        author: actor(author),
        state: "open".into(),
        draft: number == 3,
        created_at: format!("2026-01-0{number}T10:00:00Z"),
        updated_at: format!("2026-01-0{number}T12:30:00Z"),
        queues: BTreeSet::from([Queue::Authored]),
    }
}

fn inbox() -> App {
    let mut github = GithubWorkspace::empty(identity(), vec![], BTreeSet::new());
    github.inbox.pull_requests = vec![
        pull_request(1, "Fix dark playback", "thowd22"),
        pull_request(2, "Theme tokens for panes", "sam-ops"),
        pull_request(3, "Pin runner images", "kjones"),
    ];
    github.selected = github.inbox.pull_requests.first().map(Into::into);
    App {
        view: View::Inbox,
        github: Some(github),
        github_content_focused: true,
        ..Default::default()
    }
}

fn run(id: u64, name: &str, conclusion: &str) -> ScopedRun {
    ScopedRun {
        repository: "example/repo".into(),
        run: WorkflowRun {
            id,
            workflow_id: 7,
            name: name.into(),
            title: "Release".into(),
            head_sha: format!("b7cdcd3{id:033}"),
            head_branch: "main".into(),
            event: "workflow_dispatch".into(),
            status: "completed".into(),
            conclusion: conclusion.into(),
            attempt: 1,
            actor: actor("thowd22"),
            triggering_actor: actor("thowd22"),
            created_at: "2026-10-13T12:00:00Z".into(),
            updated_at: "2026-10-13T12:08:37Z".into(),
            url: String::new(),
        },
    }
}

fn job(id: u64, name: &str, conclusion: &str) -> ActionsJob {
    ActionsJob {
        id,
        run_id: 124,
        attempt: 1,
        head_sha: String::new(),
        name: name.into(),
        status: "completed".into(),
        conclusion: conclusion.into(),
        started_at: "2026-10-13T12:00:00Z".into(),
        completed_at: "2026-10-13T12:08:29Z".into(),
        runner_id: 0,
        runner_name: String::new(),
        runner_group_name: String::new(),
        labels: vec![],
        check_run_url: String::new(),
        steps: vec![],
    }
}

fn actions() -> App {
    let mut workspace =
        crate::actions_workspace::ActionsWorkspace::new(identity(), vec!["example/repo".into()]);
    workspace.runs = vec![run(123, "CI", "success"), run(124, "Release", "failure")];
    // Run 124 was opened before: its attempt and jobs are loaded; run 123's are not.
    workspace.run = Some(RunPage {
        repository: "example/repo".into(),
        attempt: RunAttempt {
            run: workspace.runs[1].run.clone(),
            jobs: vec![
                job(1, "build aarch64-musl", "cancelled"),
                job(2, "build x86_64-musl", "failure"),
            ],
            definition_note: String::new(),
        },
        selected_job: None,
        logs: None,
        annotations: vec![],
    });
    workspace.prepare_rows();
    App {
        view: View::Actions,
        actions: Some(workspace),
        actions_content_focused: true,
        ..Default::default()
    }
}

fn paint(app: &mut App, width: u16, height: u16) -> (Buffer, Vec<String>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer().clone();
    let lines = (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    (buffer, lines)
}

fn key(app: &mut App, code: KeyCode) {
    let modifiers = if code == KeyCode::BackTab {
        KeyModifiers::SHIFT
    } else {
        KeyModifiers::NONE
    };
    app.handle_event(Event::Key(KeyEvent::new(code, modifiers)));
}

fn click(app: &mut App, column: u16, row: u16) {
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }));
}

/// The text of the screen's columns `[from, to)`.
fn columns(lines: &[String], from: u16, to: u16) -> Vec<String> {
    lines
        .iter()
        .map(|line| {
            line.chars()
                .skip(usize::from(from))
                .take(usize::from(to - from))
                .collect()
        })
        .collect()
}

fn holds(lines: &[String], needle: &str) -> bool {
    lines.iter().any(|line| line.contains(needle))
}

/// The top border row of the pane that starts at column `x`.
fn title(lines: &[String], area: Rect) -> String {
    columns(lines, area.x, area.right())[usize::from(area.y)].clone()
}

#[test]
fn design_split_v92_split_list_preview_is_side_by_side_from_120_columns_only() {
    let narrow = split_list_preview(Rect::new(0, 1, SPLIT_MIN_WIDTH - 1, 30));
    assert_eq!(narrow.list, Rect::new(0, 1, 119, 30));
    assert_eq!(narrow.preview, None);
    for width in [SPLIT_MIN_WIDTH, 140, 200] {
        let split = split_list_preview(Rect::new(0, 1, width, 30));
        let preview = split.preview.unwrap();
        assert_eq!(split.list.x, 0);
        assert_eq!(split.list.right(), preview.x, "the panes touch");
        assert_eq!(preview.right(), width, "together they fill the area");
        assert_eq!((split.list.y, preview.y, preview.height), (1, 1, 30));
        assert!(
            split.list.width > preview.width,
            "the list keeps the larger share"
        );
    }
    assert_eq!(split_list_preview(Rect::new(0, 0, 140, 30)).list.width, 84);
}

#[test]
fn design_split_v92_inbox_preview_shows_the_cached_summary_beside_the_list_without_reads() {
    let mut app = inbox();
    let (_, lines) = paint(&mut app, 140, 42);
    let preview = app
        .preview_area_v92
        .expect("a preview beside the list at 140");
    assert_eq!(preview.x, 84);
    let right = columns(&lines, preview.x, preview.right());
    let left = columns(&lines, 0, preview.x);
    assert!(
        title(&lines, preview).contains("Preview"),
        "{}",
        lines.join("\n")
    );
    assert!(
        title(&lines, preview).contains("Enter Open"),
        "{}",
        lines.join("\n")
    );
    // The selected (first) row's cached summary: title, reference, author chip, both full UTC
    // timestamps, link and body; nothing that would need another read.
    for needle in [
        "Fix dark playback",
        "example/actual#1",
        "OPEN",
        "TH thowd22",
        "2026-01-01T10:00:00Z",
        "2026-01-01T12:30:00Z",
        "https://github.invalid/example/actual/pull/1",
        "DESCRIPTION",
        "Body of pull request 1.",
        "Second line 1.",
    ] {
        assert!(
            holds(&right, needle),
            "{needle} missing:\n{}",
            right.join("\n")
        );
    }
    // The list keeps its table on the left with every row and no preview text.
    for needle in ["example/actual#1", "example/actual#2", "example/actual#3"] {
        assert!(holds(&left, needle), "{}", left.join("\n"));
    }
    assert!(!holds(&left, "Body of pull request"));
    // Moving the cursor re-presents the next cached PR; no gh read or refresh is queued.
    key(&mut app, KeyCode::Down);
    let (_, lines) = paint(&mut app, 140, 42);
    let right = columns(&lines, preview.x, preview.right());
    assert!(
        holds(&right, "Theme tokens for panes"),
        "{}",
        right.join("\n")
    );
    assert!(holds(&right, "SO sam-ops"), "{}", right.join("\n"));
    assert!(!holds(&right, "Fix dark playback"));
    key(&mut app, KeyCode::Down);
    let (_, lines) = paint(&mut app, 140, 42);
    let right = columns(&lines, preview.x, preview.right());
    assert!(holds(&right, "OPEN · DRAFT"), "{}", right.join("\n"));
    assert!(app.github_action.is_none(), "{:?}", app.github_action);
    assert!(!app.refresh_requested);
    assert!(app.github.as_ref().unwrap().pr.is_none());
}

#[test]
fn design_split_v92_inbox_drills_down_below_120_columns() {
    for (width, height) in [(80u16, 24u16), (119, 30)] {
        let mut app = inbox();
        let (_, lines) = paint(&mut app, width, height);
        assert_eq!(app.preview_area_v92, None, "{width}");
        assert!(
            !holds(&lines, "Body of pull request"),
            "{}",
            lines.join("\n")
        );
        assert!(!holds(&lines, "─ Preview"), "{}", lines.join("\n"));
        assert!(holds(&lines, "example/actual#3"));
        // Enter drills down: it opens the selected PR page (the read runs in the runtime).
        key(&mut app, KeyCode::Enter);
        assert_eq!(
            app.github_action,
            Some(GithubAction::Open(PrKey::from(
                &app.github.as_ref().unwrap().inbox.pull_requests[0]
            )))
        );
    }
}

#[test]
fn design_split_v92_tab_shift_tab_and_clicks_move_the_pane_focus_marker() {
    let mut app = inbox();
    let (buffer, lines) = paint(&mut app, 140, 42);
    let preview = app.preview_area_v92.unwrap();
    let list_title = |lines: &[String]| title(lines, Rect::new(0, preview.y, preview.x, 1));
    let marker = format!("{FOCUS_MARKER} Preview");
    assert!(
        list_title(&lines).contains(FOCUS_MARKER),
        "{}",
        list_title(&lines)
    );
    assert!(!title(&lines, preview).contains(&marker));
    let theme = app.theme();
    let bar_row = (0..42u16)
        .find(|y| buffer[(1, *y)].symbol() == crate::ui_table::SELECTION_BAR)
        .expect("the list bar");
    assert_eq!(
        buffer[(1, bar_row)].fg,
        theme.role(Role::Accent).fg.unwrap()
    );

    // Tab: the preview takes the focus; the list keeps its selection with a dim bar.
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.content_pane, Pane::Preview);
    let (buffer, lines) = paint(&mut app, 140, 42);
    assert!(
        title(&lines, preview).contains(&marker),
        "{}",
        title(&lines, preview)
    );
    assert!(
        !list_title(&lines).contains(FOCUS_MARKER),
        "{}",
        list_title(&lines)
    );
    assert_eq!(
        buffer[(1, bar_row)].symbol(),
        crate::ui_table::SELECTION_BAR
    );
    assert_eq!(
        buffer[(1, bar_row)].fg,
        theme.role(Role::TextDim).fg.unwrap()
    );
    // ↓ scrolls the preview, not the list; Enter opens the previewed PR.
    key(&mut app, KeyCode::Down);
    assert_eq!(app.preview_scroll_v92, 1);
    assert_eq!(app.github.as_ref().unwrap().selected_index(), 0);
    // Shift-Tab returns to the list.
    key(&mut app, KeyCode::BackTab);
    assert_eq!(app.content_pane, Pane::List);
    assert!(app.github_content_focused);

    // A click on the preview body focuses it; a click on a list row opens that row's PR as
    // before and returns the focus to the list pane.
    paint(&mut app, 140, 42);
    click(&mut app, preview.x + 10, preview.y + 6);
    assert_eq!(app.content_pane, Pane::Preview);
    assert!(app.github_action.is_none(), "a preview click reads nothing");
    let (_, lines) = paint(&mut app, 140, 42);
    assert!(title(&lines, preview).contains(&marker));
    let row = lines
        .iter()
        .position(|line| line.contains("example/actual#2"))
        .unwrap() as u16;
    click(&mut app, 5, row);
    assert_eq!(app.content_pane, Pane::List);
    assert_eq!(
        app.github_action,
        Some(GithubAction::Open(PrKey::from(
            &app.github.as_ref().unwrap().inbox.pull_requests[1]
        )))
    );

    // The border's `Enter Open` token is clickable and Enter in the focused preview opens the
    // same selected PR.
    let mut app = inbox();
    paint(&mut app, 140, 42);
    let open = app
        .hits
        .iter()
        .find(|hit| hit.rect.y == preview.y && hit.rect.x > preview.x)
        .cloned()
        .expect("the Open token hit");
    let expected = Some(GithubAction::Open(PrKey::from(
        &app.github.as_ref().unwrap().inbox.pull_requests[0],
    )));
    click(&mut app, open.rect.x + 1, open.rect.y);
    assert_eq!(app.github_action, expected);
    let mut app = inbox();
    paint(&mut app, 140, 42);
    key(&mut app, KeyCode::Tab);
    key(&mut app, KeyCode::Enter);
    assert_eq!(app.github_action, expected);
}

#[test]
fn design_split_v92_actions_runs_preview_shows_the_run_and_its_loaded_jobs() {
    let mut app = actions();
    let (_, lines) = paint(&mut app, 140, 42);
    let preview = app.preview_area_v92.expect("Runs preview at 140");
    let right = columns(&lines, preview.x, preview.right());
    // Run 123's jobs were never read: the preview says so instead of reading them.
    assert!(
        title(&lines, preview).contains("Run 123"),
        "{}",
        lines.join("\n")
    );
    assert!(title(&lines, preview).contains("✓ SUCCESS"));
    for needle in [
        "CI · workflow_dispatch · main",
        "b7cdcd3 · TH thowd22 · 8m",
        &format!("b7cdcd3{:033}", 123),
        "2026-10-13T12:00:00Z",
        "JOBS",
        "Jobs load when the run opens (Enter)",
    ] {
        assert!(holds(&right, needle), "{needle}:\n{}", right.join("\n"));
    }
    key(&mut app, KeyCode::Down);
    let (_, lines) = paint(&mut app, 140, 42);
    let right = columns(&lines, preview.x, preview.right());
    assert!(title(&lines, preview).contains("Run 124"));
    assert!(title(&lines, preview).contains("✗ FAILURE"));
    let cancelled = right
        .iter()
        .find(|line| line.contains("build aarch64-musl"))
        .unwrap_or_else(|| panic!("{}", right.join("\n")));
    let failed = right
        .iter()
        .find(|line| line.contains("build x86_64-musl"))
        .unwrap();
    assert!(
        cancelled.contains("⊘") && cancelled.contains("CANCELLED"),
        "{cancelled}"
    );
    assert!(
        failed.contains("✗") && failed.contains("FAILURE"),
        "{failed}"
    );
    // Status and duration columns line up.
    assert_eq!(cancelled.find("CANCELLED"), failed.find("FAILURE"));
    assert!(cancelled.contains("8m") && failed.contains("8m"));
    assert!(app.actions_action.is_none(), "{:?}", app.actions_action);
    // Enter in the focused preview opens the run, as Enter on its row does.
    key(&mut app, KeyCode::Tab);
    assert_eq!(app.content_pane, Pane::Preview);
    key(&mut app, KeyCode::Enter);
    assert!(
        matches!(
            app.actions_action,
            Some(crate::actions_workspace::ActionsAction::OpenRun { id: 124, .. })
        ),
        "{:?}",
        app.actions_action
    );
    // Below 120 columns the Runs list keeps the whole pane.
    let mut app = actions();
    let (_, lines) = paint(&mut app, 80, 24);
    assert_eq!(app.preview_area_v92, None);
    assert!(!holds(&lines, "Jobs load when"), "{}", lines.join("\n"));
}
