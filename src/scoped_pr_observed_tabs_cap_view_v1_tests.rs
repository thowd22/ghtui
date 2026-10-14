//! Original-section viewport preservation on bounded display refusal.
use super::*;
use crate::{
    app::View,
    runtime::{Background, PreparedView},
    scoped_pr_observed_tabs_v1::{install_if_current, prepare, take_input, ObservedPrTabV1},
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn scoped_pr_observed_tabs_cap_preserves_current_comment_row_scroll_and_content_or_control_focus() {
    for mouse in [false, true] {
        for content_focus in [false, true] {
            let fixture = Fixture::new();
            git(&fixture, &["init", "-q"]);
            git(&fixture, &["config", "user.name", "Fixture"]);
            git(
                &fixture,
                &["config", "user.email", "fixture@example.invalid"],
            );
            fs::write(fixture.repo().join("tracked"), b"original\n").unwrap();
            git(&fixture, &["add", "tracked"]);
            git(&fixture, &["commit", "-qm", "Initial"]);
            fs::write(fixture.repo().join("tracked"), b"private dirty bytes\n").unwrap();
            let mut steps = source_steps();
            let mut op = operation(REPO, 42);
            op[8] = list(
                &format!("repos/{REPO}/issues/7/timeline?per_page=100"),
                json!([{
                    "id":801,"event":"commented","created_at":"2026-10-02T00:00:00Z",
                    "body":"x\n".repeat(70_000),"actor":{"id":52,"login":"comment-author","type":"User"}
                }]),
            );
            steps.extend(op);
            fixture.scripted_gh(&steps);
            let source = load(&fixture);
            let scope = active(&source);
            let mut workspace = call(&fixture, source.clone(), scope.clone(), previous(&source))
                .unwrap()
                .workspace;
            let input = take_input(&mut workspace, ObservedPrTabV1::Conversation).unwrap();
            let section = prepare(
                source.clone(),
                scope.clone(),
                input,
                &CancellationToken::default(),
            )
            .unwrap();
            install_if_current(&mut workspace, section).unwrap();
            let row = workspace
                .pr
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .position(|row| row.comment.is_some() && row.label.contains("Actual conversation"))
                .unwrap();
            assert!(row > 1);
            let mut app = App {
                view: View::Inbox,
                github: Some(workspace),
                remote_scope: Some(scope.clone()),
                remote_scope_source: Some(source.clone()),
                selected: row,
                scroll: row - 1,
                focus: 3,
                github_content_focused: content_focus,
                ..Default::default()
            };
            let old_payload = app.github.as_ref().unwrap().pr.as_ref().unwrap().rows[row]
                .comment
                .as_ref()
                .unwrap()
                .clone();
            let old_rows = app
                .github
                .as_ref()
                .unwrap()
                .pr
                .as_ref()
                .unwrap()
                .rows
                .as_ptr();
            let cached = app.github.as_ref().unwrap().inbox.pull_requests.as_ptr();
            let before = sql_state(&fixture);
            let git_before = (
                git(&fixture, &["rev-parse", "HEAD"]),
                fs::read(fixture.repo().join(".git/index")).unwrap(),
                git(&fixture, &["status", "--porcelain=v2", "-z"]),
                fs::read(fixture.repo().join("tracked")).unwrap(),
            );
            let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            let expected_focus = if mouse {
                let (index, hit) = app
                    .hits
                    .iter()
                    .enumerate()
                    .find(|(_, hit)| {
                        hit.action == Action::Github(GithubAction::PrTab(PrTab::Timeline))
                    })
                    .unwrap();
                let rect = hit.rect;
                app.handle_event(Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: rect.x + 1,
                    row: rect.y,
                    modifiers: KeyModifiers::NONE,
                }));
                index
            } else {
                app.handle_event(Event::Key(KeyEvent::new(
                    KeyCode::Char('t'),
                    KeyModifiers::NONE,
                )));
                3
            };
            let prior_view = (row, row - 1, content_focus, expected_focus);
            assert_eq!(
                (
                    app.selected,
                    app.scroll,
                    app.github_content_focused,
                    app.focus
                ),
                prior_view,
                "the input tail must not reset the previous section before worker preparation"
            );
            assert_eq!(
                app.github_action.take(),
                Some(GithubAction::PrTab(PrTab::Timeline))
            );
            let navigation = app
                .github
                .as_ref()
                .unwrap()
                .scoped_pr_navigation
                .clone()
                .unwrap();
            let input =
                take_input(app.github.as_mut().unwrap(), ObservedPrTabV1::Timeline).unwrap();
            // Pending body wheel/PageDown cannot clamp an old comment viewport
            // to the cached Inbox length while its sole page is in the worker.
            app.dispatch(Action::Scroll(3));
            assert_eq!(
                (
                    app.selected,
                    app.scroll,
                    app.github_content_focused,
                    app.focus
                ),
                prior_view
            );
            let mut background = Background::new();
            let producer_source = source.clone();
            let producer_scope = scope.clone();
            assert!(background.submit_scoped_pr_tab(
                &mut app,
                scope.clone(),
                source.clone(),
                navigation,
                move |cancel| {
                    let result = prepare(producer_source, producer_scope, input, &cancel).unwrap();
                    assert!(result.display_limited);
                    Ok(PreparedView {
                        scoped_pr_tab: Some(result),
                        status: "Observed display cap • previous section retained".into(),
                        ..Default::default()
                    })
                }
            ));
            let deadline = Instant::now() + Duration::from_secs(3);
            while app.loading[View::Inbox.index()] {
                background.poll(&mut app);
                assert!(Instant::now() < deadline, "cap completion unavailable");
                thread::yield_now();
            }
            assert_eq!(
                (
                    app.selected,
                    app.scroll,
                    app.github_content_focused,
                    app.focus
                ),
                prior_view
            );
            let page = app.github.as_ref().unwrap().pr.as_ref().unwrap();
            assert_eq!(page.tab, PrTab::Conversation);
            assert_eq!(page.rows.as_ptr(),old_rows,"cap returns the unchanged actual previous prepared rows, not a rebuilt/truncated clone");
            assert!(Arc::ptr_eq(
                page.rows[app.selected].comment.as_ref().unwrap(),
                &old_payload
            ));
            assert_eq!(
                page.rows[app.selected]
                    .comment
                    .as_ref()
                    .unwrap()
                    .observed
                    .id,
                301
            );
            assert_eq!(
                page.rows[app.selected]
                    .comment
                    .as_ref()
                    .unwrap()
                    .observed
                    .node_id,
                "COMMENT_301"
            );
            assert_eq!(
                app.github.as_ref().unwrap().inbox.pull_requests.as_ptr(),
                cached
            );
            assert!(Arc::ptr_eq(app.remote_scope.as_ref().unwrap(), &scope));
            assert!(Arc::ptr_eq(
                app.remote_scope_source.as_ref().unwrap(),
                &source
            ));
            terminal.backend_mut().resize(80, 24);
            terminal
                .resize(ratatui::layout::Rect::new(0, 0, 80, 24))
                .unwrap();
            app.handle_event(Event::Resize(80, 24));
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            assert_eq!(app.selected, row);
            assert_eq!(app.scroll, row - 1);
            assert!(app
                .hits
                .iter()
                .any(|hit| hit.action == Action::Github(GithubAction::Back)));
            assert_eq!(sql_state(&fixture), before);
            exact(&fixture, &steps);
            assert_eq!(steps.len(), 20);
            assert_eq!(
                (
                    git(&fixture, &["rev-parse", "HEAD"]),
                    fs::read(fixture.repo().join(".git/index")).unwrap(),
                    git(&fixture, &["status", "--porcelain=v2", "-z"]),
                    fs::read(fixture.repo().join("tracked")).unwrap()
                ),
                git_before
            );
        }
    }
}
