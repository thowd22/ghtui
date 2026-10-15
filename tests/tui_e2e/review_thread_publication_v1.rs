//! DECLARED / UNRUN: genuine initial source, selected thread and shared Form;
//! actual transport/input/Git/SQL/TTY witnesses, never direct App dispatch.
use super::*;
#[path = "review_thread_draft_witness_v1.rs"]
mod draft_witness_v1;
use ghtui::providers::gh::{
    METRICS_THREAD_COMMENTS_QUERY, THREAD_RESOLUTION_MUTATION_FIELDS_V2,
    THREAD_RESOLUTION_STATE_QUERY_V2,
};
const REPLY: &str = "Original native reply 界\nsecond line";
const FOREIGN: &str = "PRIVATE_FOREIGN_THREAD_DRAFT_MUST_NEVER_BE_ADOPTED";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Reply,
    Resolve,
    Reopen,
}
impl Operation {
    fn label(self) -> &'static str {
        match self {
            Self::Reply => "Reply to thread",
            Self::Resolve => "Resolve thread",
            Self::Reopen => "Reopen thread",
        }
    }
    fn control(self) -> &'static str {
        match self {
            Self::Reply => "[p Reply]",
            Self::Resolve => "[r Resolve]",
            Self::Reopen => "[u Reopen]",
        }
    }
    fn key(self) -> &'static [u8] {
        match self {
            Self::Reply => b"p",
            Self::Resolve => b"r",
            Self::Reopen => b"u",
        }
    }
    fn command(self) -> &'static str {
        match self {
            Self::Reply => "thread-reply",
            Self::Resolve => "thread-resolve",
            Self::Reopen => "thread-reopen",
        }
    }
    fn was_resolved(self) -> bool {
        self == Self::Reopen
    }
    fn body(self) -> &'static str {
        if self == Self::Reply {
            REPLY
        } else {
            ""
        }
    }
    fn title(self) -> String {
        format!("{} • {FIRST}#7", self.label())
    }
}
fn gql(query: &str, variables: Value, output: Value) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &output.to_string(),
    );
    step.stdin = serde_json::to_vec(&json!({"query":query,"variables":variables})).unwrap();
    step
}
fn writable_page(operation: Operation) -> Value {
    let mut value = page();
    let node = &mut value["data"]["node"]["reviewThreads"]["nodes"][0];
    node["viewerCanReply"] = json!(true);
    node["isResolved"] = json!(operation.was_resolved());
    node["viewerCanResolve"] = json!(!operation.was_resolved());
    node["viewerCanUnresolve"] = json!(operation.was_resolved());
    node["pullRequest"]["baseRefOid"] = json!("b".repeat(40));
    // The actual original is deliberately LAST and has a commit distinct from H.
    let root = node["comments"]["nodes"][0].clone();
    let mut child = root.clone();
    child["id"] = json!("NATIVE_COMMENT_8");
    child["fullDatabaseId"] = json!("9008");
    child["replyTo"] = json!({"id":"NATIVE_COMMENT_9"});
    child["body"] = json!("Observed child is not the reply root");
    node["comments"]["nodes"] = json!([child, root]);
    node["comments"]["totalCount"] = json!(2);
    value
}
fn selected_node(operation: Operation) -> Value {
    let mut node = writable_page(operation)["data"]["node"]["reviewThreads"]["nodes"][0].clone();
    node["__typename"] = json!("PullRequestReviewThread");
    node
}
fn browse(operation: Operation) -> Vec<GhStep> {
    let mut steps = phase();
    steps[9].stdout = writable_page(operation).to_string().into_bytes();
    steps
}
fn opening(operation: Operation) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(gql(
        METRICS_THREAD_COMMENTS_QUERY,
        json!({"id":"NATIVE_THREAD_9","first":100,"after":null}),
        json!({"data":{"node":selected_node(operation)}}),
    ));
    assert_eq!(steps.len(), 7);
    steps
}
fn closing() -> Vec<GhStep> {
    let mut steps = vec![
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
        get(&format!("repos/{FIRST}"), repository(0)),
    ];
    steps.extend(principal(42));
    steps
}
fn observation(operation: Operation) -> Vec<GhStep> {
    let mut steps = opening(operation);
    steps.extend(closing());
    assert_eq!(steps.len(), 11);
    steps
}
fn reply_receipt() -> Value {
    json!({"id":901,"node_id":"NATIVE_REPLY_901","pull_request_url":format!("https://{HOST}/repos/{FIRST}/pulls/7"),"user":{"id":42,"login":"renamed-same-numeric-user","type":"User"},"path":"src/old.rs","commit_id":"c".repeat(40),"body":REPLY,"in_reply_to_id":9009})
}
fn receipts(operation: Operation) -> Vec<GhStep> {
    if operation == Operation::Reply {
        let mut post = GhStep::read(
            &[
                "api",
                &format!("repos/{FIRST}/pulls/7/comments/9009/replies"),
                "--hostname",
                HOST,
                "--method",
                "POST",
                "--input",
                "-",
            ],
            &reply_receipt().to_string(),
        );
        post.stdin = serde_json::to_vec(&json!({"body":REPLY})).unwrap();
        return vec![
            post,
            get(
                &format!("repos/{FIRST}/pulls/comments/901"),
                reply_receipt(),
            ),
        ];
    }
    let desired = operation == Operation::Resolve;
    let mut node = selected_node(operation);
    node.as_object_mut().unwrap().remove("comments");
    node["isResolved"] = json!(desired);
    node["resolvedBy"] =
        json!({"__typename":"User","databaseId":88,"login":"observed-resolver","avatarUrl":null});
    let (name, input) = if desired {
        ("resolveReviewThread", "ResolveReviewThreadInput")
    } else {
        ("unresolveReviewThread", "UnresolveReviewThreadInput")
    };
    vec![
        gql(&format!("mutation($input:{input}!){{{name}(input:$input){{{THREAD_RESOLUTION_MUTATION_FIELDS_V2}}}}}"),json!({"input":{"threadId":"NATIVE_THREAD_9"}}),json!({"data":{name:{"thread":node.clone()}}})),
        gql(THREAD_RESOLUTION_STATE_QUERY_V2,json!({"id":"NATIVE_THREAD_9"}),json!({"data":{"node":node}})),
    ]
}
fn confirm(operation: Operation) -> Vec<GhStep> {
    let mut steps = opening(operation);
    steps.push(get(
        "user",
        json!({"id":42,"login":"renamed-same-numeric-user"}),
    ));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.extend(closing());
    steps.extend(receipts(operation));
    steps.push(get("user", json!({"id":42,"login":"same-positive-uid"})));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get("user", json!({"id":42,"login":"same-positive-uid"})));
    assert_eq!(
        steps.len(),
        20,
        "One actual measured prewrite and fixed six postwrite children"
    );
    steps
}
fn untouched_sql(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    ["SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'thread-draft/%' ORDER BY host,login,resource",
     "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
     "SELECT json_array(host,login,kind,id,payload) FROM records WHERE login='user-id:43' ORDER BY host,login,kind,id",
     "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"]
        .into_iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn exact(session: &Session, fixture: &Fixture, steps: &[GhStep], writes: usize) {
    let rows = commands(session, fixture);
    assert_eq!(rows.len(), steps.len());
    let mut actual_writes = 0;
    for (ordinal, (row, step)) in rows.iter().zip(steps).enumerate() {
        assert_eq!(row["step"], ordinal);
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        if step.args.iter().any(|arg| arg == "POST") {
            if step.args[1] == "graphql" {
                let input: Value = serde_json::from_slice(&step.stdin).unwrap();
                let query = input["query"].as_str().unwrap();
                if query.starts_with("mutation(") {
                    actual_writes += 1;
                    assert_eq!(input["variables"]["input"]["threadId"], "NATIVE_THREAD_9");
                } else {
                    assert!(query.starts_with("query("));
                }
            } else {
                actual_writes += 1;
                assert!(step.args[1].ends_with("/comments/9009/replies"));
            }
        }
        assert!(!step
            .args
            .iter()
            .any(|arg| matches!(arg.as_str(), "PUT" | "PATCH" | "DELETE")));
    }
    assert_eq!(actual_writes, writes);
}
fn form_ready(session: &mut Session, operation: Operation, preview: bool) {
    session
        .try_wait_for_condition(
            |screen| {
                let phase = if preview {
                    "READ-ONLY PREVIEW • Confirm action"
                } else {
                    "PR INTENT • Preview first"
                };
                screen.contains(&format!("┌ {}", operation.title()))
                    && screen.contains(phase)
                    && screen.contains("[Preview]")
                    && screen.contains("[Confirm action]")
                    && screen.contains("[Keep draft / Close]")
                    && !screen.contains("[Esc Cancel]")
                    && if operation == Operation::Reply {
                        screen.contains("Body:") || preview
                    } else {
                        !screen.contains("Body:") && !screen.contains("[External editor]")
                    }
            },
            "Actual owned original-thread Form/phase and complete controls",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn form_control(session: &mut Session, mouse: bool, operation: Operation, label: &str) {
    if mouse {
        let point = session.wait_for_owned_completed_action(&operation.title(), label, &[]);
        session.click(point.0, point.1);
    } else {
        let controls = if operation == Operation::Reply {
            vec![
                "[Preview]",
                "[External editor]",
                "[Confirm action]",
                "[Keep draft / Close]",
            ]
        } else {
            vec!["[Preview]", "[Confirm action]", "[Keep draft / Close]"]
        };
        session.focus_completed_owned_action_v1(
            &operation.title(),
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &controls,
            &[],
        );
        session.send_key(b"\r");
    }
}
fn prepared(session: &mut Session, fixture: &Fixture, mouse: bool, operation: Operation) {
    files(session, fixture, mouse);
    session.resize(120, 40);
    choose(session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(fixture, 58);
    owned(
        session,
        &[THREAD_BODY, "NATIVE_THREAD_9", operation.control()],
    );
    choose(session, mouse, operation.control(), operation.key());
    session.wait_for_command_count(fixture, 69);
    // Avoid claiming narrow footer/body visibility before the actual resize.
    session.resize(120, 40);
    form_ready(session, operation, false);
    if operation == Operation::Reply {
        if mouse {
            session.click_owned_form_field("PR INTENT • Preview first", "Body");
        }
        session.paste(REPLY);
        session
            .try_wait_for_condition(
                |screen| {
                    screen.contains("Original native reply 界")
                        && screen.contains("second line")
                        && screen.contains("[Keep draft / Close]")
                },
                "Actual entered bounded reply body",
                Duration::from_secs(8),
            )
            .unwrap();
    }
    form_control(session, mouse, operation, "[Preview]");
    session.wait_for_command_count(fixture, 80);
    form_ready(session, operation, true);
    let before = session.completed_cursorless_end();
    session.paste("PASTE_MUST_NOT_RETIRE_OR_EDIT_A_READ_ONLY_THREAD_PREVIEW");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains(&operation.title())
                && text.contains("READ-ONLY PREVIEW • Confirm action")
                && text.contains("[Confirm action]")
                && !text.contains("PASTE_MUST_NOT_RETIRE_OR_EDIT")
        },
        "Ignored actual paste preserves exact read-only thread Preview and Confirm",
        Duration::from_secs(8),
    );
}
fn held_child(
    session: &mut Session,
    fixture: &Fixture,
    ordinal: usize,
) -> crate::support::owned_pty_processes_v1::ProcessIdentity {
    session.wait_for_command_count(fixture, ordinal + 1);
    let pid = i32::try_from(commands(session, fixture)[ordinal]["pid"].as_u64().unwrap()).unwrap();
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    let identity = process_identity(pid)
        .unwrap()
        .expect("Actual held original child live before SQL witness/cancel");
    assert!(owner.owns(&identity));
    assert!(!identity.zombie);
    identity
}
fn draft_and_pending(fixture: &Fixture, operation: Operation) -> Value {
    let db = raw(fixture);
    let encoded: String = db
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'thread-draft/%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let value: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["version"], 2);
    assert_eq!(value["values"]["body"], operation.body());
    assert!(value.get("approved").is_none());
    let row:(i64,String,String,String,String,String)=db.query_row("SELECT attempt_id,command,pr_node,observed_head,payload_sha256,state FROM pr_write_attempts",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?))).unwrap();
    assert_eq!(row.0, value["publication"]["attempt_id"].as_i64().unwrap());
    assert_eq!(row.1, operation.command());
    assert_eq!(row.2, "GENUINE_CANONICAL_PR_NODE_1007");
    assert_eq!(row.3, "a".repeat(40));
    assert_eq!(row.4, value["publication"]["payload_sha256"]);
    assert_eq!(row.5, "pending");
    value
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Finish {
    Happy,
    CancelBeforePost,
    CancelAfterPost,
}
fn journey(mouse: bool, operation: Operation, finish: Finish) {
    let mut write = confirm(operation);
    let held = match finish {
        Finish::Happy | Finish::CancelBeforePost => 7,
        Finish::CancelAfterPost => 15,
    };
    write[held].delay_ms = 6000;
    if finish != Finish::Happy {
        write.truncate(held + 1);
    }
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(browse(operation));
    steps.extend(observation(operation));
    steps.extend(observation(operation));
    steps.extend(write);
    let fixture = resource_configured(&steps);
    Cache::open(&database(&fixture))
        .unwrap()
        .put_draft(
            &Scope::github(HOST, Some(43)).unwrap(),
            "foreign-thread-unrelated-original",
            FOREIGN,
        )
        .unwrap();
    let git = git_state(&fixture);
    let sql = untouched_sql(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    prepared(&mut session, &fixture, mouse, operation);
    assert!(!session.screen().contains(FOREIGN));
    let git_journal = git_commands(&fixture);
    let before = session.completed_cursorless_end();
    form_control(&mut session, mouse, operation, "[Confirm action]");
    let child = held_child(&mut session, &fixture, 80 + held);
    let draft = draft_and_pending(&fixture, operation);
    let still_live = process_identity(child.pid)
        .unwrap()
        .expect("Original SQL ACK+journal/body/ref witness BEFORE held child finish");
    assert_eq!(
        (still_live.birth, still_live.uid, still_live.session),
        (child.birth, child.uid, child.session)
    );
    assert!(!still_live.zombie);
    if finish == Finish::Happy {
        session.wait_for_command_count(&fixture, 100);
        session.wait_for_new_cursorless_condition(before,|frame|{let text=frame.contents();text.contains("original snapshot retired")&&text.contains("refresh explicitly")&&!text.contains(THREAD_BODY)&&!text.contains("native-thread-actor")&&!text.contains("Original native reply 界")&&!text.contains(DETAIL_BODY)&&!text.contains(&operation.title())},"Typed observed receipt retires original private thread/PR/Form/actors, safe status only",Duration::from_secs(8));
        assert!(
            process_identity(child.pid)
                .unwrap()
                .is_none_or(|current| current.birth != child.birth),
            "Runner reaped exact original held prewrite child before receipt admission"
        );
        let db = raw(&fixture);
        let (command, state, selected, effect): (String, String, String, String) = db
            .query_row(
                "SELECT command,state,selected_resource,confirmed_json FROM pr_write_attempts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(command, operation.command());
        assert_eq!(state, "confirmed");
        assert!(selected.starts_with(if operation == Operation::Reply {
            "thread-reply/"
        } else {
            "thread-state/"
        }));
        let effect: Value = serde_json::from_str(&effect).unwrap();
        assert_eq!(
            effect["resource_id"],
            if operation == Operation::Reply {
                "901"
            } else if operation == Operation::Resolve {
                "resolved"
            } else {
                "unresolved"
            }
        );
        assert_eq!(
            effect["node_id"],
            if operation == Operation::Reply {
                "NATIVE_REPLY_901"
            } else {
                "NATIVE_THREAD_9"
            }
        );
        let count: u64 = db
            .query_row(
                "SELECT count(*) FROM drafts WHERE resource LIKE 'thread-draft/%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
        exact(&session, &fixture, &steps, 1);
    } else {
        if mouse {
            let point =
                session.wait_for_owned_completed_action(&operation.title(), "[Esc Cancel]", &[]);
            session.click(point.0, point.1);
        } else {
            session.send_key(b"\x1b");
        }
        session.wait_for_new_cursorless_condition(
            before,
            |frame| {
                let text = frame.contents();
                !text.contains(THREAD_BODY)
                    && !text.contains("Original native reply 界")
                    && !text.contains(&operation.title())
                    && !text.contains(DETAIL_BODY)
                    && text.contains("publication canceled")
            },
            "Actual cancelled write retires SAME original private source, never a newer source",
            Duration::from_secs(8),
        );
        wait_original_child_gone(&mut session, &child);
        let retained: Value = serde_json::from_str(
            &raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'thread-draft/%'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            retained, draft,
            "No late normal autosave overwrites original body/reference"
        );
        let expected = if finish == Finish::CancelAfterPost {
            "uncertain"
        } else {
            "pending"
        };
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let state: String = raw(&fixture)
                .query_row("SELECT state FROM pr_write_attempts", [], |row| row.get(0))
                .unwrap();
            if state == expected {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Actual original worker must durably record {expected}, observed {state}"
            );
            session.poll_output(Duration::from_millis(1));
        }
        session.send_key(b"\r");
        session.poll_output(Duration::from_millis(10));
        exact(
            &session,
            &fixture,
            &steps,
            usize::from(finish == Finish::CancelAfterPost),
        );
    }
    assert_eq!(untouched_sql(&fixture), sql);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), git_journal);
    assert_read_only_git(&fixture);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
    // A fresh real binary may start, but startup alone must not adopt old
    // publication history as permission, issue remote work or resend anything.
    if finish != Finish::Happy {
        let retained = sql_state(&fixture);
        let mut restarted = launch(&fixture, mouse);
        assert!(!restarted.screen().contains("Original native reply 界"));
        assert_eq!(sql_state(&fixture), retained);
        exact(
            &restarted,
            &fixture,
            &steps,
            usize::from(finish == Finish::CancelAfterPost),
        );
        restarted.quit_and_assert_restored_terminal(mouse);
    }
}
#[test]
fn review_threads_v1_publication_keyboard_reply_actual_root_last_ack_receipt_and_terminal_retirement(
) {
    journey(false, Operation::Reply, Finish::Happy)
}
#[test]
fn review_threads_v1_publication_mouse_reply_actual_root_last_ack_receipt_and_terminal_retirement()
{
    journey(true, Operation::Reply, Finish::Happy)
}
#[test]
fn review_threads_v1_publication_keyboard_resolve_exact_state_receipt_not_comment_body_mutation() {
    journey(false, Operation::Resolve, Finish::Happy)
}
#[test]
fn review_threads_v1_publication_mouse_resolve_exact_state_receipt_not_comment_body_mutation() {
    journey(true, Operation::Resolve, Finish::Happy)
}
#[test]
fn review_threads_v1_publication_keyboard_reopen_exact_state_receipt_not_inferred_resolver() {
    journey(false, Operation::Reopen, Finish::Happy)
}
#[test]
fn review_threads_v1_publication_mouse_reopen_exact_state_receipt_not_inferred_resolver() {
    journey(true, Operation::Reopen, Finish::Happy)
}
#[test]
fn review_threads_v1_publication_keyboard_prepost_cancel_retains_original_draft_and_pending_no_resend(
) {
    journey(false, Operation::Reply, Finish::CancelBeforePost)
}
#[test]
fn review_threads_v1_publication_mouse_prepost_cancel_retains_original_draft_and_pending_no_resend()
{
    journey(true, Operation::Reply, Finish::CancelBeforePost)
}
#[test]
fn review_threads_v1_publication_keyboard_postinvoke_cancel_retains_original_draft_and_unknown_no_resend(
) {
    journey(false, Operation::Reply, Finish::CancelAfterPost)
}
#[test]
fn review_threads_v1_publication_mouse_postinvoke_cancel_retains_original_draft_and_unknown_no_resend(
) {
    journey(true, Operation::Reply, Finish::CancelAfterPost)
}
fn changed_uid(mouse: bool) {
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(browse(Operation::Reply));
    steps.extend(principal(43));
    let fixture = resource_configured(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files(&mut session, &fixture, mouse);
    let sql = sql_state(&fixture);
    session.resize(120, 40);
    choose(&mut session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(&fixture, 58);
    owned(
        &mut session,
        &[THREAD_BODY, "@native-thread-actor", "[p Reply]"],
    );
    let trace = git_commands(&fixture);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[p Reply]", b"p");
    session.wait_for_command_count(&fixture, 60);
    session.wait_for_new_cursorless_condition(before,|frame|{
        let text=frame.contents();
        text.contains("Original thread source unavailable")&&text.contains("private views hidden")&&!text.contains(THREAD_BODY)&&!text.contains("native-thread-actor")&&!text.contains(DETAIL_BODY)&&!text.contains("Reply to thread •")
    },"Actual already-visible private thread then same-login numeric UID43 refuses original draft BEFORE private SQL/materialization",Duration::from_secs(8));
    assert_eq!(sql_state(&fixture), sql);
    no_attempts(&fixture);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_read_only_git(&fixture);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact(&session, &fixture, &steps, 0);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn review_threads_v1_publication_keyboard_actual_private_thread_then_current_uid_loss_refuses_draft(
) {
    changed_uid(false)
}
#[test]
fn review_threads_v1_publication_mouse_actual_private_thread_then_current_uid_loss_refuses_draft() {
    changed_uid(true)
}
