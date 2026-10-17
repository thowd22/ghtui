//! Actual same-engine execution. These direct worker contracts deliberately
//! do NOT prove native prequeue ordering; paired binary/Background tests do.
use super::*;
use crate::{
    drafts::DraftWriter,
    pr_actions::PrPlan,
    pr_write_journal::{PrWriteJournal, PrWriteState},
    scoped_pr_collaboration_plan_v17::ConversationPublicationStateV17,
};
#[path = "scoped_pr_collaboration_refresh_publication_v19_tests.rs"]
pub(crate) mod refresh_v19;
#[path = "scoped_pr_collaboration_runtime_v17_tests.rs"]
mod runtime;

fn write(endpoint: &str, method: &str, payload: Option<Value>, response: Value) -> GhStep {
    let mut step = get(endpoint, response);
    *step.args.iter_mut().find(|v| v.as_str() == "GET").unwrap() = method.into();
    if let Some(payload) = payload {
        step.args.extend(["--input".into(), "-".into()]);
        step.stdin = serde_json::to_vec(&payload).unwrap();
    }
    step
}
fn http(
    endpoint: &str,
    method: &str,
    payload: Option<Value>,
    status: u16,
    response: Option<Value>,
) -> GhStep {
    let mut step = write(endpoint, method, None, Value::Null);
    step.args.push("--include".into());
    if let Some(payload) = payload {
        step.args.extend(["--input".into(), "-".into()]);
        step.stdin = serde_json::to_vec(&payload).unwrap();
    }
    let body = response.map(|v| v.to_string()).unwrap_or_default();
    step.stdout = format!(
        "HTTP/2.0 {status} {}\nContent-Length: {}\r\n\r\n{body}",
        if status == 204 {
            "No Content"
        } else {
            "Created"
        },
        body.len()
    )
    .into_bytes();
    step
}
fn changed_comment(id: u64) -> Value {
    let mut raw = comment();
    raw["id"] = json!(id);
    raw["node_id"] = json!(format!("IC_{id}"));
    raw["body"] = json!(TEXT);
    if id == 901 {
        raw["user"]["id"] = json!(42);
    }
    raw
}
fn reaction(id: u64, content: &str, uid: u64) -> Value {
    json!({"id":id,"node_id":format!("REACTION_{id}"),"content":content,
        "user":{"id":uid,"login":"octocat","type":"User"},"created_at":"2026-10-15T00:00:00Z"})
}
pub(crate) fn receipts(command: PrCommand) -> Vec<GhStep> {
    let endpoint = format!("repos/{REPO}/issues/comments/501");
    match command {
        PrCommand::NewComment=>vec![write(&format!("repos/{REPO}/issues/7/comments"),"POST",Some(json!({"body":TEXT})),changed_comment(901)),get(&format!("repos/{REPO}/issues/comments/901"),changed_comment(901))],
        PrCommand::EditComment=>vec![write(&endpoint,"PATCH",Some(json!({"body":TEXT})),changed_comment(501)),get(&endpoint,changed_comment(501))],
        PrCommand::DeleteComment=>vec![http(&endpoint,"DELETE",None,204,None),gql("query($id:ID!){node(id:$id){__typename ... on IssueComment{id}}}",json!({"id":"IC_501"}),json!({"data":{"node":null}}))],
        PrCommand::AddReaction=>vec![http(&format!("{endpoint}/reactions"),"POST",Some(json!({"content":"heart"})),201,Some(reaction(901,"heart",42))),get(&format!("{endpoint}/reactions?per_page=100&page=1"),json!([reaction(901,"heart",42)]))],
        PrCommand::RemoveReaction=>vec![http(&format!("{endpoint}/reactions/601"),"DELETE",None,204,None),get(&format!("{endpoint}/reactions?per_page=100&page=1"),json!([reaction(602,"eyes",43)]))],
        PrCommand::Subscription=>vec![gql("mutation($input:UpdateSubscriptionInput!){updateSubscription(input:$input){subscribable{id viewerSubscription}}}",json!({"input":{"subscribableId":"PERSONAL_PR_1007","state":"IGNORED"}}),json!({"data":{"updateSubscription":{"subscribable":{"id":"PERSONAL_PR_1007","viewerSubscription":"IGNORED"}}}})),
            gql(crate::providers::gh::PR_CURRENT_SUBSCRIPTION_QUERY,json!({"id":"PERSONAL_PR_1007"}),json!({"data":{"node":{"__typename":"PullRequest","id":"PERSONAL_PR_1007","number":7,"headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"repository":{"nameWithOwner":REPO},"viewerSubscription":"IGNORED","viewerCanSubscribe":true}}}))],
        _=>unreachable!(),
    }
}
pub(crate) fn visibility() -> Vec<GhStep> {
    visibility_for(PrCommand::NewComment)
}
pub(crate) fn visibility_for(command: PrCommand) -> Vec<GhStep> {
    let mut summary = parent();
    summary["created_at"] = json!("2026-10-15T00:00:00Z");
    summary["updated_at"] = json!("2026-10-15T00:00:00Z");
    summary["additions"] = json!(1);
    summary["deletions"] = json!(1);
    let comments = match command {
        PrCommand::DeleteComment => json!([]),
        PrCommand::NewComment => json!([comment(), changed_comment(901)]),
        PrCommand::EditComment => json!([changed_comment(501)]),
        _ => json!([comment()]),
    };
    let mut calls = vec![
        get("user", json!({"id":42,"login":"octocat"})),
        get(&format!("repos/{REPO}"), repository()),
        get(&format!("repos/{REPO}/pulls/7"), parent()),
    ];
    for (endpoint, rows) in [
        ("issues/7/comments", comments),
        ("pulls/7/commits", json!([])),
        (
            "pulls/7/files",
            json!([{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]),
        ),
        ("pulls/7/reviews", json!([])),
        ("issues/7/timeline", json!([])),
    ] {
        let mut child = get(
            &format!("repos/{REPO}/{endpoint}?per_page=100"),
            json!([rows]),
        );
        child.args.extend(["--paginate".into(), "--slurp".into()]);
        calls.push(child);
    }
    calls.push(get(&format!("repos/{REPO}/pulls/7"), summary));
    calls.push(get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(calls.len(), 10);
    calls
}
pub(crate) fn prepared(
    command: PrCommand,
    tail: Vec<GhStep>,
) -> (Setup, crate::forms::Form, Arc<ConversationPreviewV17>) {
    let mut calls = phase(command, true, Some(true));
    calls.extend(phase(command, false, Some(true)));
    calls.extend(tail);
    let mut s = fixture(command, calls);
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
    let mut form = ui::form_from_draft(opened);
    if matches!(command, PrCommand::NewComment | PrCommand::EditComment) {
        ui::replace_editor_result(&mut form, TEXT).unwrap();
    }
    if matches!(command, PrCommand::AddReaction | PrCommand::Subscription) {
        let value = if command == PrCommand::AddReaction {
            "heart"
        } else {
            "ignored"
        };
        let at = form.fields[0]
            .choices
            .iter()
            .position(|s| s == value)
            .unwrap();
        form.dispatch(&crate::forms::FormAction::Choice(0, at));
    }
    let request = ui::request(&form).unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let bounded = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let preview = ConversationPreviewV17::prepare(
        request.intent().clone(),
        request.values().clone(),
        &bounded,
        &database(&s.fixture),
        &cancel,
        deadline,
    )
    .unwrap();
    assert!(ui::accept_preview(&mut form, &request, preview.clone()));
    s.app.overlay = Some(crate::app::Overlay::Form);
    s.app.form = Some(form);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    assert!(ui::confirmed_preview(s.app.form.as_ref().unwrap()).is_ok());
    let form = s.app.form.take().unwrap();
    s.app.overlay = None;
    (s, form, preview)
}
pub(crate) fn execute(
    s: &Setup,
    form: &crate::forms::Form,
    preview: &Arc<ConversationPreviewV17>,
) -> crate::scoped_pr_collaboration_plan_v17::ConversationPublicationV17 {
    let mut writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    let scope = ui::request(form)
        .unwrap()
        .intent()
        .source
        .opening()
        .scope()
        .clone();
    writer
        .save(
            scope.clone(),
            form.resource(),
            ui::encoded_draft(form).unwrap(),
        )
        .unwrap();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let fence = client
        .capture_publication_v2(
            &database(&s.fixture),
            &scope,
            &form.resource(),
            &cancel,
            deadline,
        )
        .unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    PrPlan::execute_original_conversation_v17(
        preview,
        &api,
        &database(&s.fixture),
        &client,
        fence,
        &cancel,
        deadline,
    )
    .unwrap()
}
pub(crate) fn exact_protocol(s: &Setup) {
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), s.steps.len());
    for (n, (actual, step)) in rows.iter().zip(&s.steps).enumerate() {
        assert_eq!(actual["step"], n);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(step.args));
        assert_eq!(
            actual["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (path, bytes) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(path)).unwrap(), bytes);
    }
    for uid in [42, 43] {
        assert_eq!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .draft(&Scope::github("github.invalid", Some(uid)).unwrap(), "kept")
                .unwrap()
                .as_deref(),
            Some(format!("PRIVATE_{uid}").as_str())
        );
    }
}
#[test]
fn conversation_comments_v17_six_actual_writes_typed_receipts_original_pool_and_atomic_cleanup() {
    for command in [
        PrCommand::NewComment,
        PrCommand::EditComment,
        PrCommand::DeleteComment,
        PrCommand::AddReaction,
        PrCommand::RemoveReaction,
        PrCommand::Subscription,
    ] {
        let mut tail = phase(command, false, Some(true));
        tail.extend(receipts(command));
        tail.extend(visibility_for(command));
        let (s, form, preview) = prepared(command, tail);
        let outcome = execute(&s, &form, &preview);
        assert_eq!(outcome.state, ConversationPublicationStateV17::Confirmed);
        assert!(!outcome.draft_retained());
        assert!(outcome.effect.is_some());
        let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let request = ui::request(&form).unwrap();
        let target = crate::pr_write_journal::PrWriteTarget::new(
            request.intent().source.identity(),
            request.intent().source.parent().summary(),
        )
        .unwrap();
        let marker = journal
            .original_publication(&target, &outcome.reference)
            .unwrap()
            .unwrap();
        assert_eq!(marker.state, PrWriteState::Confirmed);
        assert_eq!(
            marker.command,
            crate::pr_write_journal::conversation_history_v17::ConversationActionV17::from_command(
                command
            )
            .unwrap()
            .command()
        );
        assert!(journal.conversation_origin_v17(&target, &marker).is_ok());
        assert!(Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(target.scope(), &form.resource())
            .unwrap()
            .is_none());
        assert!(journal.blocking(&target).unwrap().is_none());
        exact_protocol(&s);
    }
}
#[test]
fn conversation_comments_v17_denied_actual_post_keeps_exact_body_reference_unknown_and_no_retry() {
    let command = PrCommand::NewComment;
    let mut tail = phase(command, false, Some(true));
    let mut denied = receipts(command).remove(0);
    denied.status = 1;
    denied.stdout.clear();
    denied.stderr = b"HTTP 403: test target denies conversation".to_vec();
    tail.push(denied);
    let (s, form, preview) = prepared(command, tail);
    let outcome = execute(&s, &form, &preview);
    assert_eq!(outcome.state, ConversationPublicationStateV17::Uncertain);
    assert!(outcome.effect.is_none());
    assert!(outcome.draft_retained());
    let request = ui::request(&form).unwrap();
    let target = crate::pr_write_journal::PrWriteTarget::new(
        request.intent().source.identity(),
        request.intent().source.parent().summary(),
    )
    .unwrap();
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let marker = journal.blocking(&target).unwrap().unwrap();
    assert_eq!(marker.state, PrWriteState::Uncertain);
    assert_eq!(marker.publication_reference(), outcome.reference);
    let encoded = Cache::open(&database(&s.fixture))
        .unwrap()
        .draft(target.scope(), &form.resource())
        .unwrap()
        .unwrap();
    let body: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(body["values"]["body"], TEXT);
    assert_eq!(
        body["publication"],
        serde_json::to_value(&outcome.reference).unwrap()
    );
    assert!(body.get("approval").is_none());
    exact_protocol(&s);
}

#[test]
fn conversation_comments_v17_final_numeric_uid_change_after_claim_never_posts_and_retains_exact_pending(
) {
    let command = PrCommand::NewComment;
    let mut tail = phase(command, false, Some(true));
    *tail.last_mut().unwrap() = get("user", json!({"id":43,"login":"octocat"}));
    let (s, form, preview) = prepared(command, tail);
    let outcome = execute(&s, &form, &preview);
    assert_eq!(
        outcome.state,
        ConversationPublicationStateV17::NotStartedProtected
    );
    assert!(outcome.effect.is_none());
    assert!(outcome.draft_retained());
    let request = ui::request(&form).unwrap();
    let target = crate::pr_write_journal::PrWriteTarget::new(
        request.intent().source.identity(),
        request.intent().source.parent().summary(),
    )
    .unwrap();
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let marker = journal.blocking(&target).unwrap().unwrap();
    assert_eq!(marker.state, PrWriteState::Pending);
    assert_eq!(marker.publication_reference(), outcome.reference);
    let encoded = Cache::open(&database(&s.fixture))
        .unwrap()
        .draft(target.scope(), &form.resource())
        .unwrap()
        .unwrap();
    let saved: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(saved["values"]["body"], TEXT);
    assert_eq!(
        saved["publication"],
        serde_json::to_value(&outcome.reference).unwrap()
    );
    exact_protocol(&s);
    assert!(!s
        .steps
        .iter()
        .any(|step| step.stdin.windows(8).any(|v| v == b"mutation")));
    assert!(!s.steps.iter().any(|step| step
        .args
        .iter()
        .any(|v| matches!(v.as_str(), "PATCH" | "DELETE"))));
    assert!(!s.steps.iter().any(|step| step
        .args
        .iter()
        .any(|v| v == &format!("repos/{REPO}/issues/7/comments"))));
}
