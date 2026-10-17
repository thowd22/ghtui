//! Actual Background publication contracts, not simulated handler completion.
use super::*;
use crate::github_workspace::{GithubWorkspace, PrPage, PrTab};
use crate::providers::gh::{HostIdentity, PrDetails};
use std::time::{Duration, Instant};

fn workspace(tab: PrTab) -> GithubWorkspace {
    let identity = HostIdentity {
        host: "github.invalid".into(),
        login: "viewer".into(),
        account_id: Some(42),
        gh_version: "2.101.0".into(),
    };
    let summary = serde_json::from_value(serde_json::json!({"pr":{"host":"github.invalid","id":"PR_viewport_7","repository":"example/repo","number":7,"title":"Viewport contract","body":(0..60).map(|n|format!("old summary {n:02}")).collect::<Vec<_>>().join("\n"),"url":"https://github.invalid/example/repo/pull/7","author":{"login":"author","actor_type":"User"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-14T00:00:00Z","queues":[]},"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})).unwrap();
    let mut page = PrPage {
        details: PrDetails {
            summary,
            comments: vec![],
            commits: vec![],
            files: vec![],
            reviews: vec![],
            timeline: vec![],
            coverage: Default::default(),
        },
        tab,
        diff: None,
        checks: None,
        rows: vec![],
        context: None,
        metadata_display_v51: None,
        review_progress: None,
    };
    if tab == PrTab::Metadata {
        let repo = serde_json::json!({"host":"github.invalid","id":410,"full_name":"example/repo","description":"","default_branch":"main","private":true,"archived":false,"permissions":{"pull":true,"push":true,"maintain":false,"admin":false}});
        page.context = Some(serde_json::from_value(serde_json::json!({"repository":repo,"base_repository":repo,"head_repository":null,"issue_metadata":{"locked":false,"milestone":{"title":"October release"}},"requested_reviewers":[],"requested_teams":[],"is_cross_repository":false,"maintainer_can_modify":false,"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"source_urls":[]})).unwrap());
    }
    page.prepare_rows().unwrap();
    let mut workspace =
        GithubWorkspace::empty(identity, vec!["example/repo".into()], Default::default());
    workspace.pr = Some(page);
    workspace
}
fn poll(background: &mut Background, app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while app.loading[View::Inbox.index()] {
        background.poll(app);
        assert!(
            Instant::now() < deadline,
            "Actual held completion must publish"
        );
        std::thread::yield_now();
    }
}
#[test]
fn pr_metadata_viewport_v9_held_tab_completion_retires_old_scroll_after_both_input_semantics() {
    for mouse in [false, true] {
        let mut app = App {
            view: View::Inbox,
            github: Some(workspace(PrTab::Summary)),
            ..Default::default()
        };
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let mut background = Background::new();
        assert!(background.submit(&mut app, View::Inbox, move |_| {
            started_tx.send(()).unwrap();
            release_rx
                .recv_timeout(Duration::from_secs(3))
                .expect("Held test worker must be released, including assertion-failure cleanup");
            Ok(PreparedView {
                github: Some(workspace(PrTab::Metadata)),
                ..Default::default()
            })
        }));
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        // Mouse input belongs to a completed, currently painted viewport, just
        // as it does in the real binary; an unpainted App has no hover target.
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        for _ in 0..7 {
            if mouse {
                app.handle_event(crossterm::event::Event::Mouse(
                    crossterm::event::MouseEvent {
                        kind: crossterm::event::MouseEventKind::ScrollDown,
                        column: 10,
                        row: 16,
                        modifiers: crossterm::event::KeyModifiers::NONE,
                    },
                ));
            } else {
                app.dispatch(crate::app::Action::Scroll(3));
            }
        }
        assert_eq!(app.scroll, 21);
        assert_eq!(
            app.github.as_ref().unwrap().pr.as_ref().unwrap().tab,
            PrTab::Summary
        );
        release_tx.send(()).unwrap();
        poll(&mut background, &mut app);
        assert_eq!(app.scroll, 0);
        assert_eq!(app.selected, 0);
        assert_eq!(
            app.github.as_ref().unwrap().pr.as_ref().unwrap().tab,
            PrTab::Metadata
        );
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut app))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("Locked conversation: false"));
    }
}
#[test]
fn pr_metadata_viewport_v9_same_tab_refresh_retains_intentional_viewport_and_comment_identity() {
    let mut previous = workspace(PrTab::Conversation);
    previous.pr.as_mut().unwrap().details.summary.pr.body = "short description".into();
    let comment=serde_json::from_value(serde_json::json!({"id":77,"node_id":"IC_77","body":(0..80).map(|n|format!("retained original comment line {n:02}")).collect::<Vec<_>>().join("\n"),"author":{"login":"reviewer","actor_type":"User"},"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-14T00:00:00Z","url":"https://github.invalid/example/repo/issues/7#issuecomment-77"})).unwrap();
    previous.pr.as_mut().unwrap().details.comments.push(comment);
    previous.pr.as_mut().unwrap().prepare_rows().unwrap();
    assert!(previous.pr.as_ref().unwrap().rows.len() >= 80);
    assert!(20 + 30 <= previous.pr.as_ref().unwrap().rows.len());
    let selected = previous
        .pr
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|r| r.comment.as_ref().is_some_and(|c| c.observed.id == 77))
        .unwrap();
    let fresh = previous.clone();
    let mut app = App {
        view: View::Inbox,
        github: Some(previous),
        scroll: 20,
        selected,
        ..Default::default()
    };
    let mut background = Background::new();
    assert!(
        background.submit(&mut app, View::Inbox, move |_| Ok(PreparedView {
            github: Some(fresh),
            ..Default::default()
        }))
    );
    poll(&mut background, &mut app);
    assert_eq!(app.scroll, 20);
    assert_eq!(
        app.github.as_ref().unwrap().pr.as_ref().unwrap().rows[app.selected]
            .comment
            .as_ref()
            .unwrap()
            .observed
            .id,
        77
    );
    let expected = app.github.as_ref().unwrap().pr.as_ref().unwrap().rows[20]
        .label
        .clone();
    assert!(expected.contains("retained original comment line"));
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut app))
        .unwrap();
    assert_eq!(app.scroll, 20);
    let painted = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(painted.contains(&expected));
}
#[test]
fn pr_metadata_viewport_v9_superseded_held_tab_result_cannot_reset_newer_same_tab_viewport() {
    let mut app = App {
        view: View::Inbox,
        github: Some(workspace(PrTab::Summary)),
        scroll: 20,
        ..Default::default()
    };
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut background = Background::new();
    let (forward_tx, forward_rx) = mpsc::sync_channel(16);
    let original_rx = std::mem::replace(&mut background.completions, forward_rx);
    let (observed_tx, observed_rx) = mpsc::channel();
    let relay = std::thread::spawn(move || {
        for _ in 0..2 {
            let completed = original_rx.recv_timeout(Duration::from_secs(3)).unwrap();
            let generation = completed.generation;
            assert_eq!(completed.view, View::Inbox);
            assert!(completed.result.is_ok());
            forward_tx.send(completed).unwrap();
            observed_tx.send(generation).unwrap();
        }
    });
    assert!(background.submit(&mut app, View::Inbox, move |_| {
        started_tx.send(()).unwrap();
        release_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("Held test worker must be released, including assertion-failure cleanup");
        Ok(PreparedView {
            github: Some(workspace(PrTab::Metadata)),
            ..Default::default()
        })
    }));
    started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
    let original_generation = background.generation;
    assert!(
        background.submit(&mut app, View::Inbox, move |_| Ok(PreparedView {
            github: Some(workspace(PrTab::Summary)),
            ..Default::default()
        }))
    );
    release_tx.send(()).unwrap();
    let current_generation = background.generation;
    let delivered = [
        observed_rx.recv_timeout(Duration::from_secs(3)).unwrap(),
        observed_rx.recv_timeout(Duration::from_secs(3)).unwrap(),
    ];
    assert!(delivered.contains(&original_generation) && delivered.contains(&current_generation));
    relay.join().unwrap(); // both REAL worker completions actually enqueued.
    poll(&mut background, &mut app);
    assert_eq!(app.scroll, 20);
    assert_eq!(
        app.github.as_ref().unwrap().pr.as_ref().unwrap().tab,
        PrTab::Summary
    );
}
