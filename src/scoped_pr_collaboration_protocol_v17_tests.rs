//! Actual ordinary queue/detail factory, SAME original Core/installed strict gh,
//! six fresh Open/Preview paths. No decoded source or fabricated capability.
use super::*;
use crate::{
    forms::FormPurpose, scoped_pr_collaboration_draft_v17::OpenedConversationDraftV17,
    scoped_pr_collaboration_plan_v17::ConversationPreviewV17, scoped_pr_collaboration_ui_v17 as ui,
};
pub(crate) const TEXT: &str = "Modern exact Conversation desired body 界";
const OLD: &str = "Original actual issue-comment body";
#[path = "scoped_pr_collaboration_local_values_v21_tests.rs"]
mod local_values_v21;
#[path = "novice_usability_pr_display_v33_tests.rs"]
mod novice_display_v33;
#[path = "scoped_pr_collaboration_publication_v17_tests.rs"]
pub(crate) mod publication;
fn comment() -> Value {
    json!({"id":501,"node_id":"IC_501","body":OLD,
        "issue_url":format!("https://github.invalid/repos/{REPO}/issues/7"),
        "url":format!("https://github.invalid/repos/{REPO}/issues/comments/501"),
        "user":{"id":43,"node_id":"AUTHOR_43","login":"octocat","type":"User"},
        "created_at":"2026-10-15T00:00:00Z","updated_at":"2026-10-15T00:00:00Z"})
}
fn gql(query: &str, variables: Value, value: Value) -> GhStep {
    let mut step = get("graphql", value);
    let method = step.args.iter_mut().find(|v| v.as_str() == "GET").unwrap();
    *method = "POST".into();
    step.args.extend(["--input".into(), "-".into()]);
    step.stdin = serde_json::to_vec(&json!({"query":query,"variables":variables})).unwrap();
    step
}
fn capability(allowed: Option<bool>) -> Value {
    json!({"data":{"node":{"__typename":"IssueComment","id":"IC_501","fullDatabaseId":"501",
        "body":OLD,"createdAt":"2026-10-15T00:00:00Z","updatedAt":"2026-10-15T00:00:00Z",
        "viewerCanUpdate":allowed,"viewerCanDelete":allowed,"viewerCanReact":allowed,"viewerDidAuthor":false,
        "repository":{"id":"R_10000","nameWithOwner":REPO},
        "pullRequest":{"id":"PERSONAL_PR_1007","fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),
            "repository":{"id":"R_10000","nameWithOwner":REPO}}}}})
}
fn reactions() -> Value {
    json!([
    {"id":601,"node_id":"REACTION_601","content":"heart","user":{"id":42,"login":"octocat","type":"User"},"created_at":"2026-10-15T00:00:00Z"},
    {"id":602,"node_id":"REACTION_602","content":"eyes","user":{"id":43,"login":"octocat","type":"User"},"created_at":"2026-10-15T00:00:00Z"}])
}
fn observed(command: PrCommand, opening: bool, allowed: Option<bool>) -> Vec<GhStep> {
    use crate::providers::gh::{CONVERSATION_COMMENT_QUERY_V17, PR_CURRENT_SUBSCRIPTION_QUERY};
    match command {
        PrCommand::NewComment => vec![],
        PrCommand::Subscription => vec![gql(
            PR_CURRENT_SUBSCRIPTION_QUERY,
            json!({"id":"PERSONAL_PR_1007"}),
            json!({"data":{"node":{"__typename":"PullRequest","id":"PERSONAL_PR_1007","number":7,
                "headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"repository":{"nameWithOwner":REPO},
                "viewerSubscription":"UNSUBSCRIBED","viewerCanSubscribe":true}}}),
        )],
        _ => {
            let mut calls = vec![
                get(&format!("repos/{REPO}/issues/comments/501"), comment()),
                gql(
                    CONVERSATION_COMMENT_QUERY_V17,
                    json!({"id":"IC_501"}),
                    capability(allowed),
                ),
            ];
            if command == PrCommand::RemoveReaction || opening && command == PrCommand::AddReaction
            {
                calls.push(get(
                    &format!("repos/{REPO}/issues/comments/501/reactions?per_page=100&page=1"),
                    reactions(),
                ));
            }
            calls
        }
    }
}
pub(crate) fn phase(command: PrCommand, opening: bool, allowed: Option<bool>) -> Vec<GhStep> {
    let mut steps = parent_phase();
    steps.splice(4..4, observed(command, opening, allowed));
    steps
}
fn fixture(command: PrCommand, tail: Vec<GhStep>) -> Setup {
    let f = Fixture::new();
    let mut steps = opening();
    let step = steps
        .iter_mut()
        .find(|s| s.args.iter().any(|v| v.contains("issues/7/comments?")))
        .unwrap();
    step.stdout = serde_json::to_vec(&json!([[comment()]])).unwrap();
    steps.extend(tail);
    let mut s = setup_with_steps(f, steps);
    if !matches!(command, PrCommand::NewComment | PrCommand::Subscription) {
        let workspace = s
            .app
            .github
            .take()
            .unwrap()
            .load_tab(PrTab::Conversation, &s.api, &CancellationToken::default())
            .unwrap();
        s.app.github = Some(workspace);
        s.app.inbox_list.selected = s
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
        s.app.inbox_list.top = 0;
    }
    s
}
fn exact_read(s: &Setup) {
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|r| serde_json::from_str::<Value>(r).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), s.steps.len());
    for (row, step) in rows.iter().zip(&s.steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        assert!(!step.stdin.windows(8).any(|w| w == b"mutation"));
        assert!(!step
            .args
            .iter()
            .any(|v| matches!(v.as_str(), "PATCH" | "PUT" | "DELETE")));
    }
    assert_eq!(sql(&s.fixture), s.before_sql);
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (path, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(path)).unwrap(), expected);
    }
}
fn journey(command: PrCommand) {
    let mut tail = phase(command, true, Some(true));
    tail.extend(phase(command, false, Some(true)));
    let mut s = fixture(command, tail);
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let bounded = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let opened = OpenedConversationDraftV17::open(
        original.clone(),
        command,
        &bounded,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    assert_eq!(opened.intent.source.parent().repository_id(), 10000);
    if let Some(selected) = &opened.intent.selected {
        assert_eq!(selected.observed.author.login, "octocat");
        assert_eq!(opened.intent.source.identity().account_id, Some(42));
        assert!(
            opened.intent.author.as_ref().unwrap().label.contains("43"),
            "actual moderator is not original author"
        );
    }
    if command == PrCommand::RemoveReaction {
        let values = &opened.form.fields[0].choices;
        assert!(values.contains(&"601".into()));
        assert!(
            !values.contains(&"602".into()),
            "same login/different positive numeric UID cannot be selected for removal"
        );
    }
    let mut form = ui::form_from_draft(opened);
    if matches!(command, PrCommand::NewComment | PrCommand::EditComment) {
        ui::replace_editor_result(&mut form, TEXT).unwrap();
    }
    if command == PrCommand::AddReaction {
        let index = form.fields[0]
            .choices
            .iter()
            .position(|v| v == "heart")
            .unwrap();
        form.dispatch(&crate::forms::FormAction::Choice(0, index));
    }
    if command == PrCommand::Subscription {
        let index = form.fields[0]
            .choices
            .iter()
            .position(|v| v == "ignored")
            .unwrap();
        form.dispatch(&crate::forms::FormAction::Choice(0, index));
    }
    let request = ui::request(&form).unwrap();
    let cancel = CancellationToken::default();
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
        &cancel,
        deadline,
    )
    .unwrap();
    assert!(ui::accept_preview(&mut form, &request, preview.clone()));
    assert_eq!(
        form.focus,
        form.fields.len(),
        "Initial Preview focus is the first visible footer, never the hidden body"
    );
    assert!(Arc::ptr_eq(
        form.preview_display_v1.document.as_ref().unwrap(),
        preview.display()
    ));
    s.app.form = Some(form);
    s.app.overlay = Some(crate::app::Overlay::Form);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    let form = s.app.form.as_mut().unwrap();
    assert!(!form.preview_display_v1.geometry_rejected);
    assert!(
        ui::confirmed_preview(form).is_ok(),
        "actual useful current Doc paint required"
    );
    let before = ui::request(form).unwrap();
    let text = form.values();
    form.paste("POISON_AT_PREVIEW");
    assert_eq!(form.values(), text);
    assert!(before.matches_form(form));
    for _ in 0..8 {
        form.key(
            crossterm::event::KeyCode::Tab,
            crossterm::event::KeyModifiers::NONE,
        );
        assert!(
            form.focus >= form.fields.len(),
            "prepared Preview never focuses hidden editor"
        );
    }
    assert!(matches!(form.purpose, FormPurpose::PrCollaborationV17(_)));
    // Real new-purpose paint at tiny geometry must revoke useful confirmation
    // without changing the request/source or doing any transport/SQL work.
    let before = ui::request(form).unwrap();
    let mut tiny = ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 10)).unwrap();
    tiny.draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    assert!(ui::confirmed_preview(s.app.form.as_ref().unwrap()).is_err());
    assert!(before.matches_form(s.app.form.as_ref().unwrap()));
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    assert!(ui::confirmed_preview(s.app.form.as_ref().unwrap()).is_ok());
    exact_read(&s);
}
#[test]
fn conversation_comments_v17_create_actual_original_open_preview_doc_and_poison_readonly() {
    journey(PrCommand::NewComment)
}
#[test]
fn conversation_comments_v17_edit_moderator_capability_keeps_original_author_uid() {
    journey(PrCommand::EditComment)
}
#[test]
fn conversation_comments_v17_delete_bodyless_actual_initial_autosave_intent() {
    journey(PrCommand::DeleteComment)
}
#[test]
fn conversation_comments_v17_reaction_add_loaded_actual_actors_not_login_ownership() {
    journey(PrCommand::AddReaction)
}
#[test]
fn conversation_comments_v17_reaction_remove_exact_positive_owned_id_not_same_login() {
    journey(PrCommand::RemoveReaction)
}
#[test]
fn conversation_comments_v17_subscription_explicit_requested_state_not_toggle() {
    journey(PrCommand::Subscription)
}
fn subscription_caption_case_v135(actual: Option<&str>, expected: &str, requested: &str) {
    let mut calls = phase(PrCommand::Subscription, true, Some(true));
    let graph = &mut calls[4];
    assert_eq!(graph.args[1], "graphql");
    let mut response: Value = serde_json::from_slice(&graph.stdout).unwrap();
    assert_eq!(response["data"]["node"]["viewerCanSubscribe"], true);
    response["data"]["node"]["viewerSubscription"] = json!(actual);
    graph.stdout = serde_json::to_vec(&response).unwrap();
    let mut s = fixture(PrCommand::Subscription, calls);
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let bounded = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let opened = OpenedConversationDraftV17::open(
        original.clone(),
        PrCommand::Subscription,
        &bounded,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    let source = opened.intent.source.clone();
    assert_eq!(
        opened.intent.subscription.as_ref().unwrap().state_label(),
        expected
    );
    let form = ui::form_from_draft(opened);
    assert_eq!(
        form.values()["subscription"],
        actual.map_or("", |_| expected)
    );
    s.app.form = Some(form);
    s.app.overlay = Some(crate::app::Overlay::Form);
    let reservations = || {
        let db = rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let mut q = db
            .prepare("SELECT host,login,source,generation,active FROM sync_reservations ORDER BY host,login,source")
            .unwrap();
        let rows = q
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, i64>(4)?,
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows
    };
    let before_reservations = reservations();
    let before_commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    for mode in [
        crate::theme::ColorMode::TrueColor,
        crate::theme::ColorMode::NoColor,
    ] {
        s.app.color_mode = mode;
        for (width, height) in [(80, 24), (120, 40)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal.draw(|f| crate::ui::draw(f, &mut s.app)).unwrap();
            let rows = terminal
                .backend()
                .buffer()
                .content
                .chunks(usize::from(width))
                .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
                .collect::<Vec<_>>();
            let current = rows
                .iter()
                .position(|line| line.contains(&format!("Current subscription: {expected}")))
                .unwrap();
            let target = rows
                .iter()
                .position(|line| line.contains("Requested subscription state"))
                .unwrap();
            assert!(
                current < target,
                "Actual observation must be separate from the requested field: {rows:?}"
            );
            let choice = s.app.form.as_ref().unwrap().fields[0]
                .choices
                .iter()
                .position(|v| v == requested)
                .unwrap();
            s.app
                .dispatch(crate::app::Action::Form(crate::forms::FormAction::Choice(
                    0, choice,
                )));
            terminal.draw(|f| crate::ui::draw(f, &mut s.app)).unwrap();
            let rows = terminal
                .backend()
                .buffer()
                .content
                .chunks(usize::from(width))
                .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
                .collect::<Vec<_>>();
            assert!(
                rows.iter()
                    .any(|line| line.contains(&format!("Current subscription: {expected}"))),
                "Requested choice must not rewrite the actual observation: {rows:?}"
            );
            let form = s.app.form.as_ref().unwrap();
            assert_eq!(form.values()["subscription"], requested);
            assert!(!form.preview_requested && !form.submit_requested && !form.submitting);
            let FormPurpose::PrCollaborationV17(state) = &form.purpose else {
                panic!("Actual collaboration Form required")
            };
            assert_eq!(
                state.current_subscription().unwrap().state_label(),
                expected
            );
            assert!(Arc::ptr_eq(
                &ui::request(form).unwrap().intent().source,
                &source
            ));
            assert!(Arc::ptr_eq(source.opening(), &original));
            assert_eq!(reservations(), before_reservations);
            assert_eq!(
                fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
                before_commands
            );
            exact_read(&s);
        }
    }
}
#[test]
fn conversation_comments_v135_current_subscription_known_observation_is_not_the_requested_choice() {
    for (actual, expected, requested) in [
        ("SUBSCRIBED", "subscribed", "ignored"),
        ("UNSUBSCRIBED", "unsubscribed", "subscribed"),
        ("IGNORED", "ignored", "unsubscribed"),
    ] {
        subscription_caption_case_v135(Some(actual), expected, requested);
    }
}
#[test]
fn conversation_comments_v135_current_subscription_null_stays_unavailable_after_explicit_choice() {
    subscription_caption_case_v135(None, "unavailable (null)", "ignored");
}
#[test]
fn conversation_comments_v138_blank_subscription_preview_keeps_actual_form_before_queue_then_explicit_choice_prepares(
) {
    use crate::{
        app::{Action, Overlay, View},
        drafts::DraftWriter,
        forms::FormAction,
        runtime::{scoped_inline_test_support_v1 as channel, Background},
        scoped_pr_collaboration_native_v17 as native,
    };
    let mut calls = phase(PrCommand::Subscription, true, Some(true));
    let mut next = phase(PrCommand::Subscription, false, Some(true));
    for calls in [&mut calls, &mut next] {
        let mut value: Value = serde_json::from_slice(&calls[4].stdout).unwrap();
        assert_eq!(value["data"]["node"]["viewerCanSubscribe"], true);
        value["data"]["node"]["viewerSubscription"] = Value::Null;
        calls[4].stdout = serde_json::to_vec(&value).unwrap();
    }
    let opening_children = calls.len();
    calls.extend(next);
    let mut s = fixture(PrCommand::Subscription, calls);
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let opened = OpenedConversationDraftV17::open(
        original.clone(),
        PrCommand::Subscription,
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    let source = opened.intent.source.clone();
    s.app.form = Some(ui::form_from_draft(opened));
    s.app.overlay = Some(Overlay::Form);
    let blank = ui::request(s.app.form.as_ref().unwrap()).unwrap();
    assert_eq!(blank.values()["subscription"], "");
    let page = allocations(&s.app);
    let before_sql = sql(&s.fixture);
    let before_commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let reservations = || {
        let db = rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let mut statement = db.prepare("SELECT json_array(host,login,source,generation,active) FROM sync_reservations ORDER BY host,login,source").unwrap();
        let rows = statement
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows
    };
    let before_reservations = reservations();
    let loading = s.app.loading;
    let status = s.app.status.clone();
    let mut background = Background::new();
    s.app.dispatch(Action::Form(FormAction::Preview));
    let epoch = s.app.form.as_ref().unwrap().presentation_v4().1;
    native::prepare_requested(&mut s.app);
    let form = s.app.form.as_ref().unwrap();
    assert!(blank.owns_presentation(form));
    assert_eq!(form.presentation_v4().1, epoch);
    assert_eq!(form.error, "Choose a supported subscription state.");
    assert!(form.preview_requested && !form.submit_requested && !form.submitting && !form.preview);
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    assert_eq!(channel::generation(&background), None);
    assert_eq!(s.app.loading, loading);
    assert_eq!(s.app.status, status);
    assert_eq!(s.app.overlay, Some(Overlay::Form));
    assert_eq!(allocations(&s.app), page);
    assert!(source.owns_presentation(&s.app));
    assert_eq!(sql(&s.fixture), before_sql);
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
            .get::<_, usize>(0))
            .unwrap(),
        0
    );
    assert_eq!(reservations(), before_reservations);
    assert_eq!(
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before_commands
    );
    assert_eq!(
        before_commands
            .iter()
            .filter(|byte| **byte == b'\n')
            .count(),
        s.steps.len() - opening_children
    );
    let choice = form.fields[0]
        .choices
        .iter()
        .position(|value| value == "ignored")
        .unwrap();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    let painted = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(painted.contains("Choose a supported subscription state."));
    assert!(painted.contains("Current subscription: unavailable"));
    assert!(painted.contains("[Preview]"));
    s.app.dispatch(Action::Form(FormAction::Choice(0, choice)));
    s.app.dispatch(Action::Form(FormAction::Preview));
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert_eq!(work.phase(), native::ConversationPhaseKindV17::Preview);
    let writer = DraftWriter::new(database(&s.fixture));
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Actual explicitly chosen Preview worker must finish"
        );
        std::thread::yield_now();
    }
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    let form = s.app.form.as_ref().unwrap();
    assert!(form.preview && form.error.is_empty() && !form.submitting);
    let request = ui::request(form).unwrap();
    assert!(Arc::ptr_eq(&request.intent().source, &source));
    assert_eq!(request.values()["subscription"], "ignored");
    assert!(ui::confirmed_preview(form).is_ok());
    assert_eq!(allocations(&s.app), page);
    let after_preview_sql = sql(&s.fixture);
    let after_preview_commands =
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    for mode in [
        crate::theme::ColorMode::TrueColor,
        crate::theme::ColorMode::NoColor,
    ] {
        s.app.color_mode = mode;
        for (width, height) in [(80, 24), (120, 40)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut s.app))
                .unwrap();
            let rows = terminal
                .backend()
                .buffer()
                .content
                .chunks(usize::from(width))
                .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
                .collect::<Vec<_>>();
            let current = rows
                .iter()
                .position(|row| row.contains("Current subscription: unavailable (null)"))
                .unwrap();
            let requested = rows
                .iter()
                .position(|row| row.contains("Requested subscription state: ignored"))
                .unwrap();
            assert!(current < requested, "Actual current observation and prepared requested effect must remain separate: {rows:?}");
            assert!(rows.iter().any(|row| row.contains("READ-ONLY PREVIEW")));
            assert!(ui::confirmed_preview(s.app.form.as_ref().unwrap()).is_ok());
            assert_eq!(sql(&s.fixture), after_preview_sql);
            assert_eq!(
                fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
                after_preview_commands
            );
            assert!(Arc::ptr_eq(
                &ui::request(s.app.form.as_ref().unwrap())
                    .unwrap()
                    .intent()
                    .source,
                &source
            ));
        }
    }
    exact_read(&s);
}
#[test]
fn conversation_comments_v17_missing_or_false_actual_update_capability_refuses_no_attempt_or_post()
{
    for allowed in [None, Some(false)] {
        let s = fixture(PrCommand::EditComment, {
            let mut calls = phase(PrCommand::EditComment, true, allowed);
            calls.truncate(6);
            calls
        });
        let original =
            PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
        let bounded = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
            .unwrap();
        let result = OpenedConversationDraftV17::open(
            original.clone(),
            PrCommand::EditComment,
            &bounded,
            &database(&s.fixture),
            original.cancellation(),
            original.deadline(),
        );
        let error = match result {
            Ok(_) => {
                panic!("Actual false/missing update capability cannot issue an editable intent")
            }
            Err(error) => error,
        };
        assert!(
            matches!(
                error.kind,
                crate::providers::gh::GithubErrorKind::Permission
            ),
            "Actual false/unknown permission is NOT numeric-account/source loss: {error:?}"
        );
        let connection = rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            0
        );
        exact_read(&s);
    }
}

#[test]
fn conversation_comments_v18_subscription_malformed_errors_and_partial_data_never_issue_source() {
    for errors in [
        json!({"message":"malformed error object with apparently valid data"}),
        json!("malformed errors string"),
        json!(false),
        json!(7),
        json!([{"message":"partial current subscription response"}]),
    ] {
        let mut calls = phase(PrCommand::Subscription, true, Some(true));
        let graph = &mut calls[4];
        assert_eq!(graph.args[1], "graphql");
        let mut response: Value = serde_json::from_slice(&graph.stdout).unwrap();
        assert_eq!(response["data"]["node"]["viewerCanSubscribe"], true);
        assert_eq!(
            response["data"]["node"]["viewerSubscription"],
            "UNSUBSCRIBED"
        );
        response["errors"] = errors;
        graph.stdout = serde_json::to_vec(&response).unwrap();
        calls.truncate(5);
        let s = fixture(PrCommand::Subscription, calls);
        let original =
            PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
        let bounded = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
            .unwrap();
        assert!(OpenedConversationDraftV17::open(
            original.clone(),
            PrCommand::Subscription,
            &bounded,
            &database(&s.fixture),
            original.cancellation(),
            original.deadline(),
        ).is_err(), "Malformed errors may not mint current subscription or a closed source from partial data");
        let connection = rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            connection
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM pr_write_conversation_origins_v17",
                    [],
                    |r| r.get::<_, usize>(0)
                )
                .unwrap(),
            0
        );
        exact_read(&s);
    }
}

#[test]
fn conversation_comments_v18_actual_preview_initial_focus_tab_backtab_enter_and_doc_are_visible() {
    use crate::forms::FormAction;
    use crossterm::event::{KeyCode, KeyModifiers};
    let (s, mut form, preview) = publication::prepared(PrCommand::NewComment, vec![]);
    assert!(form.preview);
    assert_eq!(form.focus, form.fields.len());
    assert!(Arc::ptr_eq(
        form.preview_display_v1.document.as_ref().unwrap(),
        preview.display()
    ));
    let original = ui::request(&form).unwrap();
    assert!(original.matches_confirm(&form));
    assert_eq!(form.key(KeyCode::Tab, KeyModifiers::NONE), None);
    assert_eq!(form.focus, form.fields.len() + 1);
    assert!(
        !original.matches_confirm(&form),
        "Actual accepted input must retire an already captured queued request"
    );
    assert!(ui::request(&form).unwrap().matches_confirm(&form));
    assert_eq!(form.key(KeyCode::BackTab, KeyModifiers::NONE), None);
    assert_eq!(form.focus, form.fields.len());
    assert!(ui::request(&form).unwrap().matches_confirm(&form));
    assert_eq!(
        form.key(KeyCode::Enter, KeyModifiers::NONE),
        Some(FormAction::Preview)
    );
    assert!(
        ui::request(&form).unwrap().matches_confirm(&form),
        "Fresh current input retains the same useful Preview until its chosen action is dispatched"
    );
    assert!(!original.matches_confirm(&form));
    assert!(Arc::ptr_eq(
        form.preview_display_v1.document.as_ref().unwrap(),
        preview.display()
    ));
    exact_read(&s);
}

#[test]
fn conversation_comments_v101_actual_busy_form_has_owned_cancel_and_ignores_tab() {
    use crate::{
        app::{Action, Overlay},
        forms::FormAction,
    };
    use crossterm::event::{KeyCode, KeyModifiers};
    use ratatui::{backend::TestBackend, Terminal};

    let (mut s, mut form, preview) = publication::prepared(PrCommand::NewComment, vec![]);
    let request = ui::request(&form).unwrap();
    form.submitting = true;
    form.focus = form.fields.len() + 2;
    let focus = form.focus;
    let epoch = form.presentation_v4().1;
    s.app.overlay = Some(Overlay::Form);
    s.app.form = Some(form);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(text.contains("[Esc Cancel]"));
    let cancel = s
        .app
        .hits
        .iter()
        .find(|hit| hit.action == Action::Form(FormAction::Close))
        .expect("actual same-purpose owned footer Close control")
        .clone();
    let actual_label = (cancel.rect.x..cancel.rect.right())
        .map(|x| terminal.backend().buffer()[(x, cancel.rect.y)].symbol())
        .collect::<String>();
    assert_eq!(actual_label, "[Esc Cancel]");
    let form = s.app.form.as_mut().unwrap();
    for key in [KeyCode::Tab, KeyCode::BackTab] {
        assert_eq!(form.key(key, KeyModifiers::NONE), None);
        assert_eq!(form.focus, focus, "busy Form never promises Tab traversal");
        assert_eq!(form.presentation_v4().1, epoch);
        assert!(request.matches_form(form));
        assert!(Arc::ptr_eq(
            form.preview_display_v1.document.as_ref().unwrap(),
            preview.display()
        ));
    }
    assert_eq!(
        form.key(KeyCode::Esc, KeyModifiers::NONE),
        Some(FormAction::Close)
    );
    assert_eq!(cancel.action, Action::Form(FormAction::Close));
    assert_eq!(form.focus, focus);
    assert!(Arc::ptr_eq(
        form.preview_display_v1.document.as_ref().unwrap(),
        preview.display()
    ));
    exact_read(&s);
}
