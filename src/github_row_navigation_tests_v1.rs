//! Navigation/rendering contracts only. DTOs never mint an access/write proof;
//! actual scoped Check600/Status700 provenance is exercised by real PTYs.
use super::*;
use crate::app::{Action, App, View};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
fn app(interactive: bool) -> App {
    let mut github = GithubWorkspace::empty(
        HostIdentity {
            host: "github.invalid".into(),
            login: "current-user".into(),
            account_id: Some(42),
            gh_version: "2.99.0".into(),
        },
        vec![],
        BTreeSet::new(),
    );
    let pr = PullRequest {
        host: "github.invalid".into(),
        id: "PR_navigation_7".into(),
        repository: "example/actual".into(),
        number: 7,
        title: "Observed row navigation".into(),
        body: String::new(),
        url: String::new(),
        author: Default::default(),
        state: "open".into(),
        draft: false,
        created_at: String::new(),
        updated_at: String::new(),
        queues: BTreeSet::new(),
    };
    let summary = crate::providers::gh::PrSummary {
        pr,
        head_sha: "a".repeat(40),
        base_sha: "b".repeat(40),
        head_ref: "feature".into(),
        base_ref: "main".into(),
        merged: false,
        auto_merge: false,
        merge_commit_sha: None,
        mergeable: None,
        mergeable_state: "unknown".into(),
        additions: 0,
        deletions: 0,
        changed_files: 0,
    };
    let mut rows = (0..if interactive { 6 } else { 30 })
        .map(|n| PrRow::text(format!("Noninteractive metadata {n}")))
        .collect::<Vec<_>>();
    if interactive {
        for (index, id) in [(2, 11), (4, 22)] {
            rows[index].label = format!("Original observed comment {id}");
            rows[index].comment = Some(std::sync::Arc::new(PrCommentSelection {
                kind: crate::providers::gh::CommentKind::Conversation,
                observed: crate::providers::gh::ConversationComment {
                    id,
                    node_id: format!("COMMENT_{id}"),
                    body: "Observed body".into(),
                    author: Default::default(),
                    created_at: String::new(),
                    updated_at: String::new(),
                    url: String::new(),
                    card_v115: None,
                },
            }));
        }
    }
    github.pr = Some(PrPage {
        details: PrDetails {
            summary,
            comments: vec![],
            commits: vec![],
            files: vec![],
            reviews: vec![],
            timeline: vec![],
            coverage: Coverage {
                complete: true,
                ..Default::default()
            },
        },
        tab: PrTab::Conversation,
        diff: None,
        checks: None,
        required_checks: None,
        rows,
        context: None,
        metadata_display_v51: None,
        review_progress: None,
    });
    App {
        view: View::Inbox,
        github: Some(github),
        github_content_focused: true,
        ..Default::default()
    }
}
fn key(app: &mut App, code: KeyCode) {
    app.handle_event(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)));
}
fn draw(
    app: &mut App,
    width: u16,
    height: u16,
) -> ratatui::Terminal<ratatui::backend::TestBackend> {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    app.hits.clear();
    terminal
        .draw(|frame| {
            let area = frame.area();
            crate::github_ui::draw(frame, app, area)
        })
        .unwrap();
    terminal
}
fn targets(app: &App) -> Vec<usize> {
    app.hits
        .iter()
        .filter_map(|hit| {
            if let Action::GithubRow(index) = hit.action {
                Some(index)
            } else {
                None
            }
        })
        .collect()
}
#[test]
fn completed_frame_actions_pr_navigation_first_down_skips_notice_rows_and_matches_painted_mouse_targets(
) {
    for (width, height) in [(80, 24), (120, 40)] {
        let mut keyboard = app(true);
        draw(&mut keyboard, width, height);
        assert_eq!(targets(&keyboard), vec![2, 4]);
        assert_eq!(keyboard.inbox_list.selected, 0);
        key(&mut keyboard, KeyCode::Down);
        assert_eq!(
            keyboard.inbox_list.selected, 2,
            "FIRST Down must select a genuinely painted/clickable row, not invisible metadata"
        );
        let frame = draw(&mut keyboard, width, height);
        let hit = keyboard
            .hits
            .iter()
            .find(|hit| hit.action == Action::GithubRow(2))
            .unwrap();
        // The cursor is the `▌` bar on the Selection background, never a `>` marker.
        let buffer = frame.backend().buffer();
        assert_eq!(
            buffer[(hit.rect.x, hit.rect.y)].symbol(),
            crate::ui_table::SELECTION_BAR
        );
        let selection = crate::ui_table::selection_style(keyboard.theme());
        assert_eq!(
            buffer[(hit.rect.x + 5, hit.rect.y)].bg,
            selection.bg.unwrap()
        );
        let painted = (0..width)
            .map(|x| buffer[(x, hit.rect.y)].symbol())
            .collect::<String>();
        assert!(
            painted.starts_with("▌ Original observed comment 11"),
            "{painted}"
        );
        assert!(!(0..height).any(|y| (0..width).any(|x| buffer[(x, y)].symbol() == ">")));
        // Rows that are not targets never take the bar.
        let other = keyboard
            .hits
            .iter()
            .find(|hit| hit.action == Action::GithubRow(4))
            .unwrap();
        assert_eq!(buffer[(other.rect.x, other.rect.y)].symbol(), " ");
        let mut mouse = app(true);
        draw(&mut mouse, width, height);
        let hit = mouse
            .hits
            .iter()
            .find(|hit| hit.action == Action::GithubRow(2))
            .unwrap()
            .rect;
        mouse.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: hit.x,
            row: hit.y,
            modifiers: KeyModifiers::NONE,
        }));
        assert_eq!(mouse.inbox_list.selected, keyboard.inbox_list.selected);
        assert!(
            keyboard.github_action.is_none()
                && keyboard.pr_action.is_none()
                && mouse.github_action.is_none()
                && mouse.pr_action.is_none(),
            "Selection never queues API/action authority"
        );
    }
}
#[test]
fn completed_frame_actions_pr_navigation_skips_metadata_both_directions_and_clamps_real_edges() {
    let mut app = app(true);
    app.github_visible_rows = 2;
    for (code, expected) in [
        (KeyCode::Down, 2),
        (KeyCode::Down, 4),
        (KeyCode::Down, 4),
        (KeyCode::Up, 2),
        (KeyCode::Up, 2),
    ] {
        key(&mut app, code);
        assert_eq!(app.inbox_list.selected, expected);
    }
    app.inbox_list.selected = 0;
    key(&mut app, KeyCode::Up);
    assert_eq!(app.inbox_list.selected, 2);
    app.inbox_list.selected = usize::MAX;
    key(&mut app, KeyCode::Down);
    assert_eq!(app.inbox_list.selected, 4);
    assert!(app.github_action.is_none() && app.pr_action.is_none());
}
#[test]
fn completed_frame_actions_pr_navigation_noninteractive_detail_still_scrolls_without_fake_selection(
) {
    let mut app = app(false);
    draw(&mut app, 80, 24);
    assert!(targets(&app).is_empty());
    key(&mut app, KeyCode::Down);
    assert_eq!(app.inbox_list.top, 1);
    assert_eq!(app.inbox_list.selected, 0);
    key(&mut app, KeyCode::Up);
    assert_eq!(app.inbox_list.top, 0);
    for _ in 0..50 {
        key(&mut app, KeyCode::Down);
    }
    assert_eq!(app.inbox_list.top, 29);
    assert_eq!(app.inbox_list.selected, 0);
    assert!(app.github_action.is_none() && app.pr_action.is_none());
}
