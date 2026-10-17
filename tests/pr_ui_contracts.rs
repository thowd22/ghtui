use ghtui::{
    app::{Action, App, Overlay},
    forms::{Form, PrFormKind, PrIntent},
    github_workspace::{GithubWorkspace, PrPage, PrTab},
    pr_actions::{PrActionContext, PrCommand},
    pr_write_journal::PrPublicationReference,
    providers::gh::{HostIdentity, PrDetails},
};
use serde_json::json;

fn workspace() -> GithubWorkspace {
    let identity = HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "offline".into(),
    };
    let details:PrDetails = serde_json::from_value(json!({
        "summary":{"pr":{"host":"github.invalid","id":"PR_exact","repository":"org/repo","number":7,"title":"Exact PR","body":"Description","url":"https://github.invalid/org/repo/pull/7","author":{"login":"author","actor_type":"User"},"state":"open","draft":false,"created_at":"t0","updated_at":"t1","queues":[]},"head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"topic","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0},
        "comments":[{"id":9,"node_id":"IC_9","body":"Comment first line\nComment second line","author":{"login":"octocat","actor_type":"User"},"created_at":"t0","updated_at":"t1","url":"https://github.invalid/org/repo/pull/7#issuecomment-9"}],"commits":[],"files":[],"reviews":[],"timeline":[],"coverage":{"loaded":1,"total_matches":1,"complete":true,"reasons":[]}
    })).unwrap();
    let mut workspace =
        GithubWorkspace::empty(identity, vec!["org/repo".into()], Default::default());
    workspace.pr = Some(PrPage {
        details: details.clone(),
        tab: PrTab::Conversation,
        diff: None,
        checks: None,
        rows: vec![],
        context: None,
        metadata_display_v51: None,
        review_progress: None,
    });
    workspace.replace_pr_details(details).unwrap();
    workspace
}

#[test]
fn conversation_comments_account_change_is_not_confused_with_resource_node_staleness() {
    assert!(ghtui::pr_ui::account_failure(
        "Account changed during PR inspection; private effects are not acknowledged"
    ));
    assert!(ghtui::pr_ui::account_failure(
        "GitHub identity changed; old account pages are unavailable"
    ));
    assert!(!ghtui::pr_ui::account_failure(
        "PR identity changed during effect inspection; no different resource is acknowledged"
    ));
    assert!(!ghtui::pr_ui::account_failure(
        "Comment body changed; refresh before editing"
    ));
    let mut app = App::new();
    app.github = Some(workspace());
    app.github.as_mut().unwrap().identity.account_id = None;
    assert!(app
        .pr_controls()
        .into_iter()
        .filter(|(label, _)| !label.contains("Inspect") && !label.contains("inspected"))
        .all(|(_, action)| matches!(action, Action::Explain(_))));
}

#[test]
fn conversation_comments_v2_form_restores_original_reference_not_current_head_and_rejects_atomically(
) {
    let workspace = workspace();
    let context = PrActionContext::from_workspace(&workspace, None).unwrap();
    let mut form = Form::pr_action(PrCommand::NewComment, context.clone()).unwrap();
    form.restore_draft(r#"{"body":"Legacy café\n雪"}"#).unwrap();
    assert_eq!(form.values()["body"], "Legacy café\n雪");
    assert!(form.restored_draft);
    assert!(form.publication.is_none());
    form.publication = Some(PrPublicationReference {
        attempt_id: 3,
        pr_node_id: "PR_exact".into(),
        observed_head: "c".repeat(40),
        payload_sha256: "d".repeat(64),
    });
    let draft = form.draft();
    let mut restored = Form::pr_action(PrCommand::NewComment, context).unwrap();
    restored.restore_draft(&draft).unwrap();
    assert_eq!(restored.publication, form.publication);
    assert_ne!(
        restored.publication.as_ref().unwrap().observed_head,
        workspace.pr.as_ref().unwrap().details.summary.head_sha
    );
    for (key, value) in [
        ("pr_node_id", json!("PR_another")),
        ("attempt_id", json!(0)),
        ("observed_head", json!("bad-sha")),
        ("payload_sha256", json!("bad-digest")),
    ] {
        let mut malformed: serde_json::Value = serde_json::from_str(&draft).unwrap();
        malformed["publication"][key] = value;
        malformed["values"]["body"] = json!("Must not overwrite");
        assert!(restored.restore_draft(&malformed.to_string()).is_err());
        assert_eq!(restored.values()["body"], "Legacy café\n雪");
        assert_eq!(restored.publication, form.publication);
    }
    let mut legacy = Form::pr(PrIntent {
        kind: PrFormKind::Comment,
        target: workspace.pr.as_ref().unwrap().details.summary.clone(),
        identity: workspace.identity.clone(),
    })
    .unwrap();
    legacy.restore_draft(&draft).unwrap();
    assert_eq!(legacy.publication, form.publication);
    assert_eq!(legacy.values()["body"], "Legacy café\n雪");
}

#[test]
fn conversation_comments_exact_rows_and_paged_menu_share_mouse_keyboard_targets_and_disabled_ack() {
    let mut app = App::new();
    app.github = Some(workspace());
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| ghtui::ui::draw(frame, &mut app))
        .unwrap();
    let rows = app
        .hits
        .iter()
        .filter_map(|hit| match hit.action {
            Action::GithubRow(index) => Some(index),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        rows.len() >= 3,
        "header and every body line must retain canonical target"
    );
    for index in rows {
        app.dispatch(Action::GithubRow(index));
        let context = app.pr_context().unwrap();
        assert_eq!(context.comment.unwrap().observed.node_id, "IC_9");
        app.dispatch(Action::Github(
            ghtui::github_workspace::GithubAction::Actions,
        ));
        assert_eq!(
            app.selected, index,
            "opening menu cannot replace exact selection"
        );
        assert!(app
            .pr_controls()
            .iter()
            .any(|(label, action)| label == "Edit selected comment"
                && matches!(action, Action::Pr(_))));
        app.overlay = None;
    }
    app.overlay = Some(Overlay::PrActions);
    terminal
        .draw(|frame| ghtui::ui::draw(frame, &mut app))
        .unwrap();
    let page = app.pr_menu_rows;
    let next = app
        .hits
        .iter()
        .find(|hit| hit.action == Action::MenuScroll(page as i32))
        .unwrap()
        .clone();
    app.dispatch(next.action);
    assert_eq!(app.pr_action_index, page);
    app.pr_action_index = app
        .pr_controls()
        .iter()
        .position(|(label, _)| label == "Acknowledge inspected result")
        .unwrap();
    terminal
        .draw(|frame| ghtui::ui::draw(frame, &mut app))
        .unwrap();
    assert!(app.hits.iter().any(|hit|matches!(&hit.action,Action::Explain(reason) if reason.contains("Inspect this exact PR"))));
    let selected = app.pr_action_index;
    app.handle_event(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
    assert_eq!(app.overlay, Some(Overlay::Notice));
    assert!(app.notice.contains("refresh is not") || app.notice.contains("Refresh is not"));
    assert!(app.pr_action.is_none());
    assert_eq!(app.pr_action_index, selected);
}

fn reaction_page() -> ghtui::pr_actions::PrReactionPage {
    let workspace = workspace();
    let mut context = PrActionContext::from_workspace(&workspace, None).unwrap();
    let comment = workspace
        .pr
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .find_map(|row| row.comment.as_deref().cloned())
        .unwrap();
    context.comment = Some(comment);
    context.reactions = serde_json::from_value(json!([
        {"id":71,"node_id":"RE_71","content":"heart","actor":{"login":"octocat","actor_type":"User"},"account_id":42,"created_at":"2026-10-13T00:00:00Z"},
        {"id":72,"node_id":"RE_72","content":"rocket","actor":{"login":"octocat","actor_type":"User"},"account_id":43,"created_at":"2026-10-13T00:01:00Z"},
        {"id":73,"node_id":"RE_73","content":"eyes","actor":{"login":"robot","actor_type":"Bot"},"account_id":44,"created_at":"2026-10-13T00:02:00Z"},
        {"id":74,"node_id":"RE_74","content":"laugh","actor":{"login":"ghost","actor_type":"Deleted"},"account_id":null,"created_at":""}
    ])).unwrap();
    context.reactions_fetched_at = 1791849600;
    context.reactions_complete = true;
    ghtui::pr_actions::PrReactionPage::from_context(&context).unwrap()
}

#[test]
fn conversation_comments_reaction_catalog_is_readonly_paged_shared_actor_and_overlay_safe() {
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::{backend::TestBackend, Terminal};
    for (width, height) in [(120, 40), (80, 24), (40, 12)] {
        let page = reaction_page();
        let display = ghtui::pr_ui::reaction_display(&page);
        let all = display
            .iter()
            .map(|row| row.line.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(all.contains("account #42 • yours"));
        assert!(all.contains("account #43 • another actor"));
        assert!(all.contains("@robot [bot]"));
        assert!(all.contains("@ghost [deleted]"));
        assert!(all.contains("ownership unverified"));
        assert!(all.contains("Created: unavailable"));
        let mut app = App::new();
        app.github = Some(workspace());
        app.pr_reactions = Some(page);
        app.pr_reaction_display = display;
        app.overlay = Some(Overlay::PrReactions);
        app.pr_pending_message = "uncertain original publication".into();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| ghtui::ui::draw(frame, &mut app))
            .unwrap();
        assert!(app
            .hits
            .iter()
            .all(|hit| matches!(hit.action, Action::PrReactionScroll(_) | Action::Close)));
        assert!(app
            .hits
            .iter()
            .all(|hit| hit.rect.right() <= width && hit.rect.bottom() <= height));
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::PageDown,
            KeyModifiers::NONE,
        )));
        assert!(app.pr_reaction_scroll > 0);
        terminal
            .draw(|frame| ghtui::ui::draw(frame, &mut app))
            .unwrap();
        let before = app.pr_reaction_scroll;
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::ScrollUp,
            column: 10,
            row: 8,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(app.pr_reaction_scroll < before);
        app.handle_event(Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
        assert_eq!(app.focus, 1);
        terminal
            .draw(|frame| ghtui::ui::draw(frame, &mut app))
            .unwrap();
        let more = app.hits[1].rect;
        assert!(terminal.backend().buffer()[(more.x, more.y)]
            .modifier
            .contains(ratatui::style::Modifier::UNDERLINED));
        for (index, hit) in app.hits.iter().enumerate() {
            assert_eq!(
                terminal.backend().buffer()[(hit.rect.x, hit.rect.y)]
                    .modifier
                    .contains(ratatui::style::Modifier::UNDERLINED),
                index == app.focus,
                "Only the actual focused reaction control may be underlined"
            );
        }
        let close = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::Close)
            .unwrap()
            .rect;
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: close.x,
            row: close.y,
            modifiers: KeyModifiers::NONE,
        }));
        assert!(app.pr_reactions.is_none());
        assert!(app.pr_reaction_display.is_empty());
        assert_eq!(app.overlay, None);
        assert_eq!(app.pr_pending_message, "uncertain original publication");
        assert!(app.pr_action.is_none());
    }
}

#[test]
fn conversation_comments_current_subscription_is_distinct_from_requested_draft_and_null() {
    use ghtui::providers::gh::{CurrentPrSubscription, Subscription};
    use ratatui::{backend::TestBackend, Terminal};
    for state in [
        Some(Subscription::Ignored),
        Some(Subscription::Unsubscribed),
        None,
    ] {
        let mut context = PrActionContext::from_workspace(&workspace(), None).unwrap();
        context.current_subscription = Some(CurrentPrSubscription {
            state,
            viewer_can_subscribe: true,
        });
        let mut form = Form::pr_action(PrCommand::Subscription, context).unwrap();
        let expected = match state {
            Some(Subscription::Ignored) => "ignored",
            Some(Subscription::Unsubscribed) => "unsubscribed",
            _ => "",
        };
        assert_eq!(form.values()["subscription"], expected);
        form.restore_draft(r#"{"subscription":"subscribed"}"#)
            .unwrap();
        let mut app = App::new();
        app.form = Some(form);
        app.overlay = Some(Overlay::Form);
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
        terminal
            .draw(|frame| ghtui::ui::draw(frame, &mut app))
            .unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(screen.contains(&format!(
            "Current subscription: {}",
            if expected.is_empty() {
                "unavailable"
            } else {
                expected
            }
        )));
        assert!(screen.contains("Requested subscription state"));
        assert_eq!(
            app.form.as_ref().unwrap().values()["subscription"],
            "subscribed"
        );
        assert!(app.pr_action.is_none());
    }
}
