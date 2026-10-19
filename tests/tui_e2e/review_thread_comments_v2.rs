//! SOURCE ONLY / UNRUN. Actual binary key/mouse journeys; strict command/stdin,
//! original SQL/body/ref/origin, live owned child, Git/config and TTY witnesses.
use super::*;
#[path = "file_comment_controls_v75.rs"]
mod file_controls_v75;
#[path = "review_thread_historical_v5.rs"]
mod historical_v5;
#[path = "review_thread_reactions_v3.rs"]
mod reactions_v3;
#[path = "review_thread_timeline_management_v5.rs"]
mod timeline_management_v5;
use ghtui::providers::gh::{THREAD_COMMENT_ABSENCE_QUERY_V2, THREAD_SELECTED_COMMENT_QUERY_V2};
const PANEL: &str = "Selected thread comments";
const HISTORY_V2: &str = "Comment publication history • inspect only";
const EDITED: &str = "Exact edited review comment 界\nsecond line";
// Only controlled negative-fixture setup may write. The inherited raw() helper
// remains read-only for every independent journal/origin/draft observation.
fn fixture_write_v2(fixture: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap()
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommentOpV2 {
    Edit,
    Delete,
}
impl CommentOpV2 {
    fn title(self) -> String {
        format!(
            "{} • {FIRST}#7",
            if self == Self::Edit {
                "Edit selected comment"
            } else {
                "Delete selected comment"
            }
        )
    }
    fn command(self) -> &'static str {
        if self == Self::Edit {
            "thread-comment-edit-v2"
        } else {
            "thread-comment-delete-v2"
        }
    }
    fn body(self) -> &'static str {
        if self == Self::Edit {
            EDITED
        } else {
            ""
        }
    }
    fn control(self) -> &'static str {
        if self == Self::Edit {
            "[e Edit]"
        } else {
            "[d Delete]"
        }
    }
    fn key(self) -> &'static [u8] {
        if self == Self::Edit {
            b"e"
        } else {
            b"d"
        }
    }
}
fn selected_comment_v2() -> Value {
    let mut value = selected_node(Operation::Reply)["comments"]["nodes"][1].clone();
    value["__typename"] = json!("PullRequestReviewComment");
    value["state"] = json!("SUBMITTED");
    value["outdated"] = json!(true);
    value["viewerCanUpdate"] = json!(true);
    value["viewerCanDelete"] = json!(true);
    value["viewerCanReact"] = json!(true);
    value["viewerDidAuthor"] = json!(false);
    value["pullRequest"]["baseRefOid"] = json!("b".repeat(40));
    value["pullRequestReview"] =
        json!({"id":"ACTUAL_REVIEW_600","fullDatabaseId":"9007199254740993","state":"COMMENTED"});
    value
}
fn rest_v2(body: &str, updated: &str) -> Value {
    json!({"id":9009,"node_id":"NATIVE_COMMENT_9","pull_request_review_id":9007199254740993u64,
    "pull_request_url":format!("https://{HOST}/repos/{FIRST}/pulls/7"),"user":{"id":77,"login":"native-thread-actor","type":"User","avatar_url":null},
    "body":body,"created_at":"2026-10-01T00:00:00Z","updated_at":updated,"path":"src/old.rs","commit_id":"c".repeat(40),"original_commit_id":"d".repeat(40)})
}
fn current_v2() -> Value {
    rest_v2(THREAD_BODY, "2026-10-02T00:00:00Z")
}
fn comment_observe_v2() -> Vec<GhStep> {
    vec![
        get("user", json!({"id":42,"login":"moderator-not-author"})),
        get(&format!("repos/{FIRST}"), repository(0)),
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
        gql(
            THREAD_SELECTED_COMMENT_QUERY_V2,
            json!({"id":"NATIVE_COMMENT_9"}),
            json!({"data":{"node":selected_comment_v2()}}),
        ),
        get(&format!("repos/{FIRST}/pulls/comments/9009"), current_v2()),
    ]
}
fn fresh_v2() -> Vec<GhStep> {
    let mut steps = opening(Operation::Reply);
    steps.extend(comment_observe_v2());
    steps.extend(closing());
    assert_eq!(steps.len(), 16);
    steps
}
fn confirm_v2(operation: CommentOpV2) -> Vec<GhStep> {
    let mut steps = opening(Operation::Reply);
    steps.extend(comment_observe_v2());
    steps.push(get(
        "user",
        json!({"id":42,"login":"same-current-action-account"}),
    ));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get(
        &format!("repos/{FIRST}/pulls/comments/9009"),
        current_v2(),
    ));
    steps.extend(closing());
    if operation == CommentOpV2::Edit {
        let observed = rest_v2(EDITED, "2026-10-15T02:00:00Z");
        let mut patch = GhStep::read(
            &[
                "api",
                &format!("repos/{FIRST}/pulls/comments/9009"),
                "--hostname",
                HOST,
                "--method",
                "PATCH",
                "--input",
                "-",
            ],
            &observed.to_string(),
        );
        patch.stdin = serde_json::to_vec(&json!({"body":EDITED})).unwrap();
        steps.push(patch);
        steps.push(get(&format!("repos/{FIRST}/pulls/comments/9009"), observed));
    } else {
        steps.push(GhStep::read(
            &[
                "api",
                &format!("repos/{FIRST}/pulls/comments/9009"),
                "--hostname",
                HOST,
                "--method",
                "DELETE",
                "--include",
            ],
            "HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n",
        ));
        steps.push(gql(
            THREAD_COMMENT_ABSENCE_QUERY_V2,
            json!({"id":"NATIVE_COMMENT_9"}),
            json!({"data":{"node":null}}),
        ));
    }
    steps.push(get(
        "user",
        json!({"id":42,"login":"fresh-visible-current"}),
    ));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get("user", json!({"id":42,"login":"fresh-final-current"})));
    assert_eq!(steps.len(), 26);
    steps
}
fn exact_v2(session: &Session, fixture: &Fixture, steps: &[GhStep], writes: usize) {
    let rows = commands(session, fixture);
    assert_eq!(rows.len(), steps.len());
    let mut count = 0;
    for (index, (row, step)) in rows.iter().zip(steps).enumerate() {
        assert_eq!(row["step"], index);
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        if step
            .args
            .iter()
            .any(|arg| matches!(arg.as_str(), "PATCH" | "DELETE"))
        {
            count += 1;
            assert_eq!(step.args[1], format!("repos/{FIRST}/pulls/comments/9009"));
        }
        if step.args.iter().any(|arg| arg == "POST") {
            assert_eq!(step.args[1], "graphql");
            let value: Value = serde_json::from_slice(&step.stdin).unwrap();
            assert!(value["query"].as_str().unwrap().starts_with("query("));
        }
        assert!(!step.args.iter().any(|arg| arg == "PUT"));
    }
    assert_eq!(count, writes);
}
fn untouched_v2(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    [
    "SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'thread-comment-draft-v2/%' ORDER BY host,login,resource",
    "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
    "SELECT json_array(host,login,kind,id,payload) FROM records WHERE login='user-id:43' ORDER BY host,login,kind,id",
    "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"]
    .into_iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn protected_v2(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    [
    "SELECT json_array(host,login,resource,body) FROM drafts WHERE resource LIKE 'thread-comment-draft-v2/%' ORDER BY host,login,resource",
    "SELECT json_array(attempt_id,host,principal,pr_node,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts ORDER BY attempt_id",
    "SELECT json_array(host,principal,pr_node,generation) FROM pr_write_scopes ORDER BY host,principal,pr_node",
    "SELECT json_array(attempt_id,repository_id,repository_node,pr_id,origin_json) FROM pr_write_thread_comment_origins_v2 ORDER BY attempt_id"]
    .into_iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn panel_ready_v2(session: &mut Session, words: &[&str]) {
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, PANEL).is_some_and(|body| {
                body.contains("[Esc Back]")
                    && body.contains("[e Edit]")
                    && body.contains("[d Delete]")
                    && words
                        .iter()
                        .all(|word| crate::support::controls::body_has(frame, &body, word))
            })
        },
        "Completed exact selected-comment panel/header/body",
        Duration::from_secs(8),
    );
}
fn control_v2(session: &mut Session, mouse: bool, title: &str, label: &str, key: &[u8]) {
    if mouse {
        let point = session.wait_for_owned_completed_action(title, label, &[]);
        session.click(point.0, point.1);
    } else {
        session.send_key(key);
    }
}
fn form_ready_v2(session: &mut Session, operation: CommentOpV2, preview: bool) {
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains(&operation.title())
                    && screen.contains(if preview {
                        "READ-ONLY PREVIEW • Confirm action"
                    } else {
                        "PR INTENT • Preview first"
                    })
                    && screen.contains("[Preview]")
                    && screen.contains("[Confirm action]")
                    && screen.contains("[Keep draft / Close]")
                    && (operation == CommentOpV2::Edit || !screen.contains("[External editor]"))
            },
            "Actual selected-comment Form/phase/full controls",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn form_control_v2(session: &mut Session, mouse: bool, operation: CommentOpV2, label: &str) {
    if mouse {
        let point = session.wait_for_owned_completed_action(&operation.title(), label, &[]);
        session.click(point.0, point.1);
    } else {
        let controls = if operation == CommentOpV2::Edit {
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
fn prepared_v2(session: &mut Session, fixture: &Fixture, mouse: bool, operation: CommentOpV2) {
    files(session, fixture, mouse);
    session.resize(120, 40);
    choose(session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(fixture, 58);
    owned(session, &["NATIVE_THREAD_9", "[c Comments]"]);
    control_v2(session, mouse, THREAD_TITLE, "[c Comments]", b"c");
    session.wait_for_command_count(fixture, 69);
    panel_ready_v2(
        session,
        &["Comment #9008", "Comment #9009", "@native-thread-actor"],
    );
    control_v2(session, mouse, PANEL, "Comment #9009", b"\x1b[B");
    panel_ready_v2(session, &["> Comment #9009"]);
    session.resize_owned_header(80, 24, PANEL, &["[e Edit]", "[Esc Back]"]);
    panel_ready_v2(session, &["> Comment #9009"]);
    session.resize_owned_header(120, 40, PANEL, &["[e Edit]", "[Esc Back]"]);
    control_v2(session, mouse, PANEL, operation.control(), operation.key());
    session.wait_for_command_count(fixture, 85);
    form_ready_v2(session, operation, false);
    if operation == CommentOpV2::Edit {
        if mouse {
            session.click_owned_form_field("PR INTENT • Preview first", "Body");
        }
        session.send_key(b"\x01");
        session.paste(EDITED);
        session
            .try_wait_for_condition(
                |screen| {
                    screen.contains("Exact edited review comment 界")
                        && screen.contains("second line")
                },
                "Actual exact replacement body, not old text append",
                Duration::from_secs(8),
            )
            .unwrap();
    }
    form_control_v2(session, mouse, operation, "[Preview]");
    session.wait_for_command_count(fixture, 101);
    form_ready_v2(session, operation, true);
    let endpoint = session.completed_cursorless_end();
    session.paste("POISON_READONLY_COMMENT_PREVIEW");
    session.wait_for_new_cursorless_condition(
        endpoint,
        |frame| {
            frame.contents().contains(&operation.title())
                && frame.contents().contains("READ-ONLY PREVIEW")
                && !frame.contents().contains("POISON_READONLY")
        },
        "Actual ignored paste preserves exact approved comment body",
        Duration::from_secs(8),
    );
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FinishV2 {
    Happy,
    PreCancel,
    PostCancel,
    ConfirmedCleanupFailure,
}
fn fresh_summary_v2(session: &mut Session, fixture: &Fixture, mouse: bool, start: usize) {
    let old = commands(session, fixture);
    assert_eq!(old.len(), start);
    choose(session, mouse, "[: Commands]", b":");
    overlay_ready(
        session,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    session.send_key(b"\x01");
    session.paste("Discover repositories");
    overlay_ready(
        session,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(session, mouse, "> Discover repositories", b"\r");
    session.wait_for_overlay_close("Commands · current context");
    ready(session, &[TITLE, "[s Source]", "[Esc Close]"]);
    assert_eq!(commands(session, fixture), old);
    discover(session, mouse);
    session.wait_for_command_count(fixture, start + 7);
    ready(session, &[TITLE, FIRST]);
    if !mouse {
        session.send_key(b"\t");
    }
    choose(session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_command_count(fixture, start + 18);
    ready(session, &[ROW_TITLE]);
    choose(session, mouse, "1 Inbox", b"1");
    choose(session, mouse, ROW_TITLE, b"\r");
    session.wait_for_command_count(fixture, start + 31);
    ready(session, &[DETAIL_TITLE]);
    session.wait_for_control("[J Thread history]");
}
fn detail_v2(session: &mut Session, mouse: bool, target: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let frame = session.completed_cursorless_screen().unwrap();
        let body = crate::support::overlay_body(frame, HISTORY_V2).unwrap();
        if body.contains(target) {
            return;
        }
        let endpoint = session.completed_cursorless_end();
        control_v2(session, mouse, HISTORY_V2, "[PgDn Next]", b"\x1b[6~");
        session.wait_for_new_cursorless_condition(
            endpoint,
            |frame| {
                crate::support::overlay_body(frame, HISTORY_V2)
                    .is_some_and(|current| current != body && current.contains("INSPECT ONLY"))
            },
            "One acknowledged exact history viewport change",
            deadline.saturating_duration_since(Instant::now()),
        );
        assert!(Instant::now() < deadline);
    }
}
fn journey_v2(mouse: bool, operation: CommentOpV2, finish: FinishV2, restart: bool, legacy: bool) {
    let mut write = confirm_v2(operation);
    let held = if finish == FinishV2::PostCancel {
        21
    } else {
        12
    };
    write[held].delay_ms = 6000;
    if matches!(finish, FinishV2::PreCancel | FinishV2::PostCancel) {
        write.truncate(held + 1);
    }
    if finish == FinishV2::ConfirmedCleanupFailure {
        write.truncate(25);
    }
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(browse(Operation::Reply));
    steps.extend(observation(Operation::Reply));
    steps.extend(fresh_v2());
    steps.extend(fresh_v2());
    steps.extend(write);
    let old_end = steps.len();
    let pending_history = restart && finish == FinishV2::PreCancel && !legacy;
    if restart {
        steps.extend(initial());
        steps.extend(history_v1::phase_history());
        let mut list = history_v1::phase_history();
        if pending_history {
            list[0].delay_ms = 6000;
        }
        steps.extend(list);
        steps.extend(history_v1::phase_history());
    }
    let fixture = resource_configured(&steps);
    Cache::open(&database(&fixture))
        .unwrap()
        .put_draft(
            &Scope::github(HOST, Some(43)).unwrap(),
            "foreign-comment-protected",
            FOREIGN,
        )
        .unwrap();
    if finish == FinishV2::ConfirmedCleanupFailure {
        fixture_write_v2(&fixture).execute_batch("CREATE TRIGGER refuse_comment_cleanup BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'thread-comment-draft-v2/%' BEGIN SELECT RAISE(ABORT,'actual controlled cleanup failure'); END;").unwrap();
    }
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    prepared_v2(&mut session, &fixture, mouse, operation);
    let unrelated = untouched_v2(&fixture);
    let trace = git_commands(&fixture);
    let endpoint = session.completed_cursorless_end();
    form_control_v2(&mut session, mouse, operation, "[Confirm action]");
    let child = held_child(&mut session, &fixture, 101 + held);
    let encoded: String = raw(&fixture)
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'thread-comment-draft-v2/%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let value: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["values"]["body"], operation.body());
    assert_eq!(value["version"], 2);
    assert!(value.get("approved").is_none());
    let row:(i64,String,String,String,String)=raw(&fixture).query_row("SELECT attempt_id,command,state,selected_resource,payload_sha256 FROM pr_write_attempts",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap();
    assert_eq!(row.0, value["publication"]["attempt_id"].as_i64().unwrap());
    assert_eq!(row.1, operation.command());
    assert_eq!(row.2, "pending");
    assert!(row.3.starts_with("thread-comment-v2/"));
    assert_eq!(row.4, value["publication"]["payload_sha256"]);
    let origin: String = raw(&fixture)
        .query_row(
            "SELECT origin_json FROM pr_write_thread_comment_origins_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let origin: Value = serde_json::from_str(&origin).unwrap();
    assert_eq!(origin["reference"], value["publication"]);
    assert_eq!(origin["seed"]["comment_id"], 9009);
    assert_eq!(origin["seed"]["author_account_id"], 77);
    assert_eq!(origin["seed"]["scope"]["login"], "user-id:42");
    assert_eq!(origin["seed"]["comment_commit"], "c".repeat(40));
    assert_eq!(origin["seed"]["head"], "a".repeat(40));
    let current = process_identity(child.pid)
        .unwrap()
        .expect("Actual SQL/ref/origin witness while same owned child remains live");
    assert_eq!(
        (current.birth, current.uid, current.session),
        (child.birth, child.uid, child.session)
    );
    assert!(!current.zombie);
    if matches!(finish, FinishV2::PreCancel | FinishV2::PostCancel) {
        control_v2(
            &mut session,
            mouse,
            &operation.title(),
            "[Esc Cancel]",
            b"\x1b",
        );
        session.wait_for_new_cursorless_condition(
            endpoint,
            |frame| {
                let text = frame.contents();
                text.contains("publication canceled")
                    && !text.contains(THREAD_BODY)
                    && !text.contains(EDITED)
                    && !text.contains(&operation.title())
            },
            "Original canceled comment write hides exact old private source",
            Duration::from_secs(8),
        );
        wait_original_child_gone(&mut session, &child);
        let wanted = if finish == FinishV2::PreCancel {
            "pending"
        } else {
            "uncertain"
        };
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let state: String = raw(&fixture)
                .query_row("SELECT state FROM pr_write_attempts", [], |row| row.get(0))
                .unwrap();
            if state == wanted {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Actual original worker must record {wanted}, got {state}"
            );
            session.poll_output(Duration::from_millis(1));
        }
        assert_eq!(
            raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'thread-comment-draft-v2/%'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            encoded
        );
    } else {
        session.wait_for_command_count(&fixture, old_end);
        session.wait_for_new_cursorless_condition(
            endpoint,
            |frame| {
                let text = frame.contents();
                text.contains("refresh explicitly")
                    && !text.contains(THREAD_BODY)
                    && !text.contains(EDITED)
                    && !text.contains(&operation.title())
                    && !text.contains("native-thread-actor")
            },
            "Typed comment outcome retires private source and actors, safe status only",
            Duration::from_secs(8),
        );
        wait_original_child_gone(&mut session, &child);
        let (state, effect): (String, String) = raw(&fixture)
            .query_row(
                "SELECT state,confirmed_json FROM pr_write_attempts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, "confirmed");
        let effect: Value = serde_json::from_str(&effect).unwrap();
        assert_eq!(effect["resource_id"], "9009");
        assert_eq!(effect["node_id"], "NATIVE_COMMENT_9");
        assert_eq!(
            effect["kind"],
            if operation == CommentOpV2::Edit {
                "comment"
            } else {
                "comment_deleted"
            }
        );
        let drafts: u64 = raw(&fixture)
            .query_row(
                "SELECT count(*) FROM drafts WHERE resource LIKE 'thread-comment-draft-v2/%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            drafts,
            u64::from(finish == FinishV2::ConfirmedCleanupFailure)
        );
    }
    let writes = usize::from(finish != FinishV2::PreCancel);
    exact_v2(&session, &fixture, &steps[..old_end], writes);
    assert_eq!(untouched_v2(&fixture), unrelated);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
    if restart {
        if legacy {
            fixture_write_v2(&fixture)
                .execute("DELETE FROM pr_write_thread_comment_origins_v2", [])
                .unwrap();
        }
        let protected = protected_v2(&fixture);
        let mut next = launch(&fixture, mouse);
        assert!(!next.screen().contains(EDITED));
        fresh_summary_v2(&mut next, &fixture, mouse, old_end);
        assert!(git_commands(&fixture).starts_with(&trace));
        let restarted_trace = git_commands(&fixture);
        choose(&mut next, mouse, "[J Thread history]", b"J");
        next.wait_for_command_count(&fixture, old_end + 39);
        history_v1::history_ready(&mut next, &["[c Comment history]"]);
        control_v2(
            &mut next,
            mouse,
            history_v1::HISTORY,
            "[c Comment history]",
            b"c",
        );
        if pending_history {
            let child = held_child(&mut next, &fixture, old_end + 39);
            next.wait_for_cursorless_condition(
                |frame| {
                    crate::support::overlay_body(frame, HISTORY_V2).is_some_and(|body| {
                        body.contains("Exact original history read pending")
                            && body.contains("[Enter Open]")
                            && body.contains("[Esc Back]")
                            && !body.contains(EDITED)
                    })
                },
                "Actual held fresh comment-history owner; disabled Open is painted",
                Duration::from_secs(8),
            );
            for (cols, rows) in [(80, 24), (120, 40)] {
                let identity = process_identity(child.pid)
                    .unwrap()
                    .expect("Same actual history source child live before disabled input");
                assert_eq!(
                    (identity.birth, identity.uid, identity.session),
                    (child.birth, child.uid, child.session)
                );
                assert!(!identity.zombie);
                next.resize_owned_header(cols, rows, HISTORY_V2, &["[Enter Open]", "[Esc Back]"]);
                let before = next.completed_cursorless_end();
                let calls = commands(&next, &fixture);
                assert_eq!(calls.len(), old_end + 40);
                control_v2(&mut next, mouse, HISTORY_V2, "[Enter Open]", b"\r");
                next.wait_for_new_cursorless_condition(
                    before,
                    |frame| {
                        crate::support::overlay_body(frame, HISTORY_V2).is_some_and(|body| {
                            body.contains("Exact original history read pending")
                                && !body.contains(EDITED)
                                && !body.contains("[Confirm action]")
                        })
                    },
                    "Genuinely disabled pending Open cannot enqueue another read or Form",
                    Duration::from_secs(8),
                );
                let identity = process_identity(child.pid)
                    .unwrap()
                    .expect("Same original history child still live after disabled input");
                assert_eq!(
                    (identity.birth, identity.uid, identity.session),
                    (child.birth, child.uid, child.session)
                );
                assert!(!identity.zombie);
                assert_eq!(commands(&next, &fixture), calls);
                assert_eq!(protected_v2(&fixture), protected);
                assert_eq!(git_commands(&fixture), restarted_trace);
            }
        }
        next.wait_for_command_count(&fixture, old_end + 47);
        next.wait_for_cursorless_condition(
            |frame| {
                crate::support::overlay_body(frame, HISTORY_V2).is_some_and(|body| {
                    body.contains(operation.command()) && body.contains("[Enter Open]")
                })
            },
            "Actual separate closed V2 protected history row",
            Duration::from_secs(8),
        );
        // Keep genuine output in its original receiver for one bounded hold;
        // command invocation alone must not acknowledge the loaded detail.
        let detail_opened_after = next.completed_cursorless_end();
        next.hold_observed_output_after_next_write_v1(Duration::from_millis(200));
        control_v2(&mut next, mouse, HISTORY_V2, "[Enter Open]", b"\r");
        next.wait_for_command_count(&fixture, old_end + 55);
        next.wait_for_new_cursorless_condition(
            detail_opened_after,
            |frame| {
                crate::support::overlay_body(frame, HISTORY_V2).is_some_and(|body| {
                    body.contains("Recorded comment author:")
                        && body.contains("Recorded attempt #")
                        && body.contains("INSPECT ONLY")
                        && !body.contains("Exact original history read pending")
                })
            },
            "Actual completed opened comment detail before any paging input",
            Duration::from_secs(8),
        );
        detail_v2(
            &mut next,
            mouse,
            if legacy {
                "Protected legacy comment/reaction"
            } else {
                "Historical actual comment #9009"
            },
        );
        if !legacy {
            detail_v2(
                &mut next,
                mouse,
                if operation == CommentOpV2::Edit {
                    EDITED.split('\n').next().unwrap()
                } else {
                    "Exact protected LOCAL requested body"
                },
            );
        } else {
            assert!(!next.screen().contains(EDITED));
        }
        let detail_inputs = next.finish_observed_output_hold_v1();
        assert_eq!(
            crate::support::receiver_delivery_gate_v1::maximum_unacknowledged(&detail_inputs),
            1,
            "Only one actual comment-history input before a completed viewport acknowledgement: {detail_inputs:?}"
        );
        for (cols, rows) in [(80, 24), (120, 40)] {
            next.resize_owned_header(cols, rows, HISTORY_V2, &["[Enter Open]", "[Esc Back]"]);
            let before = next.completed_cursorless_end();
            let calls = commands(&next, &fixture);
            control_v2(&mut next, mouse, HISTORY_V2, "[Enter Open]", b"\r");
            next.wait_for_new_cursorless_condition(
                before,
                |frame| {
                    crate::support::overlay_body(frame, HISTORY_V2).is_some_and(|body| {
                        body.contains("INSPECT ONLY") && !body.contains("[Confirm action]")
                    })
                },
                "Genuinely disabled detail Open has no new request/Form",
                Duration::from_secs(8),
            );
            assert_eq!(commands(&next, &fixture), calls);
            assert_eq!(protected_v2(&fixture), protected);
        }
        let before = next.completed_cursorless_end();
        control_v2(&mut next, mouse, HISTORY_V2, "[Esc Back]", b"\x1b");
        next.wait_for_new_cursorless_condition(
            before,
            |frame| {
                crate::support::overlay_body(frame, HISTORY_V2).is_some_and(|body| {
                    body.contains("Current inspecting account:")
                        && body.contains(operation.command())
                        && body.contains("INSPECT ONLY")
                        && !body.contains("Recorded comment author:")
                })
            },
            "Acknowledged detail-to-list Back before a second Escape can be sent",
            Duration::from_secs(8),
        );
        control_v2(&mut next, mouse, HISTORY_V2, "[Esc Back]", b"\x1b");
        history_v1::history_ready(&mut next, &["[c Comment history]"]);
        control_v2(&mut next, mouse, history_v1::HISTORY, "[Esc Back]", b"\x1b");
        next.wait_for_overlay_close(history_v1::HISTORY);
        assert_eq!(protected_v2(&fixture), protected);
        assert_eq!(untouched_v2(&fixture), unrelated);
        assert_eq!(git_state(&fixture), git);
        assert_eq!(git_commands(&fixture), restarted_trace);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        exact_v2(&next, &fixture, &steps, writes);
        assert_read_only_git(&fixture);
        next.quit_and_assert_restored_terminal(mouse);
    }
}
#[test]
fn review_threads_v2_comment_keyboard_edit_exact_ack_receipt_source_retirement() {
    journey_v2(false, CommentOpV2::Edit, FinishV2::Happy, false, false)
}
#[test]
fn review_threads_v2_comment_mouse_edit_exact_ack_receipt_source_retirement() {
    journey_v2(true, CommentOpV2::Edit, FinishV2::Happy, false, false)
}
#[test]
fn review_threads_v2_comment_keyboard_delete_exact_204_null_and_source_retirement() {
    journey_v2(false, CommentOpV2::Delete, FinishV2::Happy, false, false)
}
#[test]
fn review_threads_v2_comment_mouse_delete_exact_204_null_and_source_retirement() {
    journey_v2(true, CommentOpV2::Delete, FinishV2::Happy, false, false)
}
#[test]
fn review_threads_v2_comment_keyboard_prepost_cancel_pending_summary_inspection_no_resend() {
    journey_v2(false, CommentOpV2::Edit, FinishV2::PreCancel, true, false)
}
#[test]
fn review_threads_v2_comment_mouse_prepost_cancel_pending_summary_inspection_no_resend() {
    journey_v2(true, CommentOpV2::Edit, FinishV2::PreCancel, true, false)
}
#[test]
fn review_threads_v2_comment_keyboard_postinvoke_cancel_uncertain_summary_inspection_no_resend() {
    journey_v2(false, CommentOpV2::Edit, FinishV2::PostCancel, true, false)
}
#[test]
fn review_threads_v2_comment_mouse_postinvoke_cancel_uncertain_summary_inspection_no_resend() {
    journey_v2(true, CommentOpV2::Edit, FinishV2::PostCancel, true, false)
}
#[test]
fn review_threads_v2_comment_keyboard_confirmed_cleanup_failure_protected_inspection() {
    journey_v2(
        false,
        CommentOpV2::Delete,
        FinishV2::ConfirmedCleanupFailure,
        true,
        false,
    )
}
#[test]
fn review_threads_v2_comment_mouse_confirmed_cleanup_failure_protected_inspection() {
    journey_v2(
        true,
        CommentOpV2::Delete,
        FinishV2::ConfirmedCleanupFailure,
        true,
        false,
    )
}
#[test]
fn review_threads_v2_comment_keyboard_legacy_protection_does_not_invent_origin_or_open_body() {
    journey_v2(false, CommentOpV2::Edit, FinishV2::PreCancel, true, true)
}
#[test]
fn review_threads_v2_comment_mouse_legacy_protection_does_not_invent_origin_or_open_body() {
    journey_v2(true, CommentOpV2::Edit, FinishV2::PreCancel, true, true)
}
fn account_loss_v2(mouse: bool) {
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(browse(Operation::Reply));
    steps.extend(observation(Operation::Reply));
    steps.extend(fresh_v2());
    steps.extend(fresh_v2());
    steps.extend(principal(43));
    let fixture = resource_configured(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    prepared_v2(&mut session, &fixture, mouse, CommentOpV2::Edit);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let encoded = raw(&fixture)
            .query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'thread-comment-draft-v2/%'",
                [],
                |row| row.get::<_, String>(0),
            )
            .ok();
        if encoded
            .as_deref()
            .and_then(|encoded| serde_json::from_str::<Value>(encoded).ok())
            .is_some_and(|value| {
                value["values"]["body"] == EDITED && value["publication"].is_null()
            })
        {
            break;
        }
        assert!(Instant::now() < deadline);
        session.poll_output(Duration::from_millis(1));
    }
    let sql = protected_v2(&fixture);
    let unrelated = untouched_v2(&fixture);
    let trace = git_commands(&fixture);
    let endpoint = session.completed_cursorless_end();
    form_control_v2(&mut session, mouse, CommentOpV2::Edit, "[Confirm action]");
    session.wait_for_command_count(&fixture, 103);
    session.wait_for_new_cursorless_condition(endpoint,|frame|{let text=frame.contents();text.contains("refresh explicitly")&&!text.contains(EDITED)&&!text.contains(THREAD_BODY)&&!text.contains("native-thread-actor")&&!text.contains(PANEL)&&!text.contains(&CommentOpV2::Edit.title())},"Fresh actual numeric account mismatch hides exact original comment/PR/Form/actors before private return or write",Duration::from_secs(8));
    assert_eq!(protected_v2(&fixture), sql);
    assert_eq!(untouched_v2(&fixture), unrelated);
    assert_eq!(
        raw(&fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    exact_v2(&session, &fixture, &steps, 0);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn review_threads_v2_comment_keyboard_actual_numeric_account_loss_before_write_hides_body_and_keeps_draft(
) {
    account_loss_v2(false)
}
#[test]
fn review_threads_v2_comment_mouse_actual_numeric_account_loss_before_write_hides_body_and_keeps_draft(
) {
    account_loss_v2(true)
}
