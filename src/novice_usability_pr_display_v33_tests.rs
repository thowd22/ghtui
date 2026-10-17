//! Actual provider/page/intent factories; captions never create permission.
use super::*;
use crate::{
    app::{Action, Overlay},
    forms::FormAction,
    theme::ColorMode,
};
use crossterm::event::{KeyCode, KeyModifiers};

fn actual(bot: bool, preview: bool) -> (Setup, crate::forms::Form) {
    let mut steps = opening();
    let mut observed = comment();
    if bot {
        observed["user"]["type"] = json!("Bot");
    }
    let opening_comments = steps
        .iter_mut()
        .find(|s| s.args.iter().any(|v| v.contains("issues/7/comments?")))
        .unwrap();
    opening_comments.stdout = serde_json::to_vec(&json!([[observed]])).unwrap();
    steps.extend(phase(PrCommand::EditComment, true, Some(true)));
    if preview {
        steps.extend(phase(PrCommand::EditComment, false, Some(true)));
    }
    for step in &mut steps {
        if let Ok(mut raw) = serde_json::from_slice::<Value>(&step.stdout) {
            if bot && raw.get("issue_url").is_some() {
                raw["user"]["type"] = json!("Bot");
                step.stdout = serde_json::to_vec(&raw).unwrap();
            } else if raw["data"]["node"]["__typename"] == "IssueComment" {
                // Edit remains genuinely allowed. Other reported capabilities
                // are distinct descriptive facts, not implied permissions.
                raw["data"]["node"]["viewerCanDelete"] = json!(false);
                raw["data"]["node"]["viewerCanReact"] = Value::Null;
                step.stdout = serde_json::to_vec(&raw).unwrap();
            }
        }
    }
    let mut s = setup_with_steps(Fixture::new(), steps);
    let workspace = s
        .app
        .github
        .take()
        .unwrap()
        .load_tab(PrTab::Conversation, &s.api, &CancellationToken::default())
        .unwrap();
    s.app.github = Some(workspace);
    s.app.selected = s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|r| r.comment.is_some())
        .unwrap();
    s.app.scroll = 0;
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let opened = OpenedConversationDraftV17::open(
        original.clone(),
        PrCommand::EditComment,
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    (s, ui::form_from_draft(opened))
}
fn paint(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(usize::from(width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
fn full_sql(fixture: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut all = Vec::new();
    for name in names {
        let mut q = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = q.column_count();
        let mut rows = q
            .query_map([], |r| {
                Ok(format!(
                    "{name}:{:?}",
                    (0..columns)
                        .map(|n| r.get::<_, rusqlite::types::Value>(n))
                        .collect::<rusqlite::Result<Vec<_>>>()?
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        all.extend(rows);
    }
    all
}

#[test]
fn novice_usability_pr_dispatch_v33_genuine_requester_author_bot_and_unavailable_ids_are_distinct_readonly(
) {
    for bot in [false, true] {
        let (mut s, form) = actual(bot, false);
        let request = ui::request(&form).unwrap();
        let intent = request.intent();
        assert_eq!(intent.source.identity().account_id, Some(42));
        let author = intent.author.as_ref().unwrap();
        assert!(author.label.contains("account #43 (not requester)"));
        assert_eq!(author.observed.actor_type, if bot { "Bot" } else { "User" });
        assert!(author.label.contains(&author.identity.label()));
        let commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        let sql_before = full_sql(&s.fixture);
        let missing = crate::scoped_pr_collaboration_draft_v17::prepare_actor_v17(
            &intent.source.identity().host,
            &author.observed,
            None,
            &CancellationToken::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert!(missing
            .label
            .contains("account ID unavailable (not requester)"));
        assert!(!missing.label.contains("#42"));
        assert_eq!(missing.identity, author.identity);
        let lines = intent.original_lines.join("\n");
        assert!(lines.contains("Requester") && lines.contains("account #42"));
        assert!(
            lines.contains("update: reported allowed")
                && lines.contains("delete: reported not allowed")
                && lines.contains("react: not reported")
        );
        assert!(
            lines.contains("Viewer is author: reported no")
                && lines.contains("not login ownership")
        );
        s.app.form = Some(form);
        s.app.overlay = Some(Overlay::Form);
        for (width, height) in [(80, 24), (120, 40)] {
            let text = paint(&mut s.app, width, height);
            assert!(text.contains("account #42"));
            assert!(!text.contains("Some(") && !text.contains("numeric UID"));
        }
        assert_eq!(full_sql(&s.fixture), sql_before);
        assert_eq!(
            fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
            commands
        );
        exact_read(&s);
    }
}

#[test]
fn novice_usability_pr_dispatch_v33_actual_prepared_preview_preserves_full_ids_doc_focus_geometry_and_no_color(
) {
    let (mut s, mut form) = actual(false, true);
    ui::replace_editor_result(&mut form, TEXT).unwrap();
    let request = ui::request(&form).unwrap();
    let original = request.intent().source.clone();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let preview = ConversationPreviewV17::prepare(
        request.intent().clone(),
        request.values().clone(),
        &provider,
        &database(&s.fixture),
        &CancellationToken::default(),
        deadline,
    )
    .unwrap();
    let lines = preview.lines().join("\n");
    for required in [
        "actual repository #10000 / PR #1007",
        "IC_501",
        OLD,
        TEXT,
        "author account #43",
        &"a".repeat(40),
        &"b".repeat(40),
    ] {
        assert!(lines.contains(required), "{required}: {lines}");
    }
    assert!(!lines.contains("Some(") && !lines.contains("None"));
    assert!(lines.len() <= 8 * 1024 * 1024 && preview.lines().len() <= 50_000);
    assert!(ui::accept_preview(&mut form, &request, preview.clone()));
    assert_eq!(form.focus, form.fields.len());
    s.app.form = Some(form);
    s.app.overlay = Some(Overlay::Form);
    let commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let sql_before = full_sql(&s.fixture);
    for mode in [
        ColorMode::TrueColor,
        ColorMode::LowColor,
        ColorMode::NoColor,
    ] {
        s.app.no_color = mode == ColorMode::NoColor;
        s.app.color_mode = mode;
        for (width, height) in [(80, 24), (120, 40)] {
            let text = paint(&mut s.app, width, height);
            assert!(text.contains("account #42") && text.contains("[Confirm action]"));
            let form = s.app.form.as_mut().unwrap();
            assert!(Arc::ptr_eq(
                form.preview_display_v1.document.as_ref().unwrap(),
                preview.display()
            ));
            assert!(Arc::ptr_eq(
                &ui::request(form).unwrap().intent().source,
                &original
            ));
            assert!(ui::confirmed_preview(form).is_ok());
            form.key(KeyCode::Tab, KeyModifiers::NONE);
            form.key(KeyCode::BackTab, KeyModifiers::NONE);
            assert!(form.focus >= form.fields.len());
            assert!(form.key(KeyCode::Enter, KeyModifiers::NONE).is_some());
            assert!(s
                .app
                .hits
                .iter()
                .any(|h| matches!(h.action, Action::Form(FormAction::Submit))));
        }
    }
    paint(&mut s.app, 40, 10);
    assert!(ui::confirmed_preview(s.app.form.as_ref().unwrap()).is_err());
    paint(&mut s.app, 120, 40);
    assert!(ui::confirmed_preview(s.app.form.as_ref().unwrap()).is_ok());
    assert_eq!(full_sql(&s.fixture), sql_before);
    assert_eq!(
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        commands
    );
    exact_read(&s);
}
