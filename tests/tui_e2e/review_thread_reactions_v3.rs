//! DECLARED / UNRUN. Actual Native PR -> Threads -> selected comment actors ->
//! fresh no-body Form -> one guarded reaction write. Original source/Git/SQL,
//! command/stdin, exact owned live child and TTY are independently witnessed.
use super::*;
#[path = "file_reaction_controls_v75.rs"]
mod file_controls_v75;
#[path = "review_thread_timeline_reactions_v5.rs"]
mod timeline_reactions_v5;
const REACTIONS: &str = "Selected comment reactions";
const REACTION_HISTORY: &str = "Reaction publication history • inspect only";
const NATIVE_OWNER: &str = "Native PR diff •";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    Add,
    Remove,
}
impl Op {
    fn title(self) -> String {
        format!(
            "{} • {FIRST}#7",
            if self == Self::Add {
                "Add selected comment reaction"
            } else {
                "Remove my selected reaction"
            }
        )
    }
    fn command(self) -> &'static str {
        if self == Self::Add {
            "thread-reaction-add-v3"
        } else {
            "thread-reaction-remove-v3"
        }
    }
    fn control(self) -> &'static str {
        if self == Self::Add {
            "[5 heart]"
        } else {
            "[r Remove mine]"
        }
    }
    fn key(self) -> &'static [u8] {
        if self == Self::Add {
            b"5"
        } else {
            b"r"
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Finish {
    Happy,
    PreCancel,
    PostCancel,
    Partial,
    Legacy,
    CleanupFailure,
    PreReplace,
    PostReplace,
}
fn actor(id: u64, uid: Option<u64>, content: &str) -> Value {
    json!({"id":id,"node_id":format!("NATIVE_REACTION_{id}"),"content":content,
    "user":uid.map(|id|json!({"id":id,"login":"same-login-not-ownership","type":"User","avatar_url":null})),"created_at":"2026-10-03T00:00:00Z"})
}
fn actors() -> Value {
    json!([
        actor(7001, Some(42), "+1"),
        actor(7002, Some(99), "eyes"),
        actor(7003, None, "rocket")
    ])
}
fn endpoint(page: usize) -> String {
    format!("repos/{FIRST}/pulls/comments/9009/reactions?per_page=100&page={page}")
}
fn actor_steps() -> Vec<GhStep> {
    vec![
        get("user", json!({"id":42,"login":"renamed-numeric-viewer"})),
        get(&format!("repos/{FIRST}"), repository(0)),
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
        get(&format!("repos/{FIRST}/pulls/comments/9009"), current_v2()),
        get(&endpoint(1), actors()),
    ]
}
fn fresh() -> Vec<GhStep> {
    let mut steps = opening(Operation::Reply);
    steps.extend(comment_observe_v2());
    steps.extend(actor_steps());
    steps.extend(closing());
    assert_eq!(steps.len(), 21);
    steps
}
fn response(operation: Op) -> GhStep {
    let endpoint = format!(
        "repos/{FIRST}/pulls/comments/9009/reactions{}",
        if operation == Op::Add {
            String::new()
        } else {
            "/7001".into()
        }
    );
    let body = if operation == Op::Add {
        actor(7004, Some(42), "heart").to_string()
    } else {
        String::new()
    };
    let status = if operation == Op::Add {
        "201 Created"
    } else {
        "204 No Content"
    };
    let mut args = vec![
        "api".to_owned(),
        endpoint,
        "--hostname".into(),
        HOST.into(),
        "--method".into(),
        if operation == Op::Add {
            "POST".into()
        } else {
            "DELETE".into()
        },
        "--include".into(),
    ];
    if operation == Op::Add {
        args.extend(["--input".into(), "-".into()]);
    }
    let refs = args.iter().map(String::as_str).collect::<Vec<_>>();
    let mut step = GhStep::read(
        &refs,
        &format!(
            "HTTP/2.0 {status}\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    );
    if operation == Op::Add {
        step.stdin = serde_json::to_vec(&json!({"content":"heart"})).unwrap();
    }
    step
}
fn confirm_reaction(operation: Op, finish: Finish) -> Vec<GhStep> {
    let mut steps = opening(Operation::Reply);
    steps.extend(comment_observe_v2());
    steps.extend(actor_steps());
    steps.push(get(
        "user",
        json!({"id":42,"login":"current-action-account"}),
    ));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get(
        &format!("repos/{FIRST}/pulls/comments/9009"),
        current_v2(),
    ));
    steps.extend(closing());
    assert_eq!(steps.len(), 25);
    steps.push(response(operation));
    if finish == Finish::Partial {
        assert_eq!(operation, Op::Remove);
        for page in 1..=4 {
            let rows = (0..100)
                .map(|i| {
                    let id = 10_000 + page as u64 * 100 + i;
                    actor(id, Some(id + 1000), "eyes")
                })
                .collect::<Vec<_>>();
            steps.push(get(&endpoint(page), json!(rows)));
        }
        assert_eq!(steps.len(), 30);
    } else {
        let observed = if operation == Op::Add {
            let mut rows = actors();
            rows.as_array_mut()
                .unwrap()
                .push(actor(7004, Some(42), "heart"));
            rows
        } else {
            json!([actor(7002, Some(99), "eyes"), actor(7003, None, "rocket")])
        };
        steps.push(get(&endpoint(1), observed));
        steps.push(get("user", json!({"id":42,"login":"fresh-current-viewer"})));
        steps.push(get(&format!("repos/{FIRST}"), repository(0)));
        steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
        steps.push(get("user", json!({"id":42,"login":"fresh-final-viewer"})));
        assert_eq!(steps.len(), 31);
    }
    let held = if matches!(finish, Finish::PostCancel | Finish::PostReplace) {
        26
    } else {
        18
    };
    steps[held].delay_ms = 6000;
    if matches!(
        finish,
        Finish::PreCancel | Finish::Legacy | Finish::PostCancel
    ) {
        steps.truncate(held + 1);
    }
    if matches!(finish, Finish::CleanupFailure | Finish::PostReplace) {
        steps.truncate(30);
    }
    if finish == Finish::PreReplace {
        steps.truncate(23);
    }
    steps
}
fn bind_value(value: &mut Value, h: &immutable_images::history::GitImagesV1) {
    match value {
        Value::String(raw) if raw == &"a".repeat(40) => *raw = h.h.clone(),
        Value::String(raw) if raw == &"b".repeat(40) => *raw = h.b.clone(),
        Value::Array(rows) => {
            for row in rows {
                bind_value(row, h);
            }
        }
        Value::Object(map) => {
            // The shared actual thread fixture already belongs to the genuine
            // Git-built selected file. Verify it rather than rebinding an
            // obsolete path. Comment historical path and C/C0 stay independent.
            if map.get("id").and_then(Value::as_str) == Some("NATIVE_THREAD_9")
                && map.contains_key("isOutdated")
            {
                assert_eq!(
                    map.get("path").and_then(Value::as_str),
                    Some(immutable_images::history::PATH)
                );
            }
            for value in map.values_mut() {
                bind_value(value, h);
            }
            if map.get("id").and_then(Value::as_str) == Some("NATIVE_COMMENT_9")
                && map.contains_key("commit")
            {
                assert_eq!(map["commit"]["oid"], "c".repeat(40));
                assert_eq!(map["originalCommit"]["oid"], "d".repeat(40));
                assert_eq!(map["path"], "src/old.rs");
            }
        }
        _ => {}
    }
}
fn bind(steps: &mut [GhStep], h: &immutable_images::history::GitImagesV1) {
    immutable_images::bind_steps(steps, h);
    for step in steps {
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            bind_value(&mut value, h);
            step.stdout = value.to_string().into_bytes();
        }
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdin) {
            bind_value(&mut value, h);
            step.stdin = serde_json::to_vec(&value).unwrap();
        }
    }
}
fn exact_reactions(session: &Session, fixture: &Fixture, steps: &[GhStep], writes: usize) {
    let actual = commands(session, fixture);
    assert_eq!(actual.len(), steps.len());
    let mut count = 0;
    for (index, (row, step)) in actual.iter().zip(steps).enumerate() {
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
        if step.args.iter().any(|arg| arg == "POST" || arg == "DELETE") {
            if step.args[1] == "graphql" {
                let payload: Value = serde_json::from_slice(&step.stdin).unwrap();
                assert!(payload["query"].as_str().unwrap().starts_with("query("));
            } else {
                count += 1;
                assert!(
                    step.args[1] == format!("repos/{FIRST}/pulls/comments/9009/reactions")
                        || step.args[1]
                            == format!("repos/{FIRST}/pulls/comments/9009/reactions/7001")
                );
                assert!(step.args.iter().any(|arg| arg == "--include"));
                if step.args.iter().any(|arg| arg == "POST") {
                    assert_eq!(
                        serde_json::from_slice::<Value>(&step.stdin).unwrap(),
                        json!({"content":"heart"})
                    );
                } else {
                    assert!(step.stdin.is_empty());
                }
            }
        }
        assert!(!step.args.iter().any(|arg| arg == "PUT" || arg == "PATCH"));
    }
    assert_eq!(count, writes);
}
fn unrelated(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    [
    "SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'thread-reaction-draft-v3/%' ORDER BY host,login,resource",
    "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
    "SELECT json_array(host,login,kind,id,payload) FROM records WHERE login='user-id:43' ORDER BY host,login,kind,id",
    "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"]
    .into_iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn protected(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    [
    "SELECT json_array(host,login,resource,body) FROM drafts WHERE resource LIKE 'thread-reaction-draft-v3/%' ORDER BY host,login,resource",
    "SELECT json_array(attempt_id,host,principal,pr_node,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts ORDER BY attempt_id",
    "SELECT json_array(host,principal,pr_node,generation) FROM pr_write_scopes ORDER BY host,principal,pr_node",
    "SELECT json_array(attempt_id,repository_id,repository_node,pr_id,origin_json) FROM pr_write_thread_reaction_origins_v3 ORDER BY attempt_id"]
    .into_iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn text_projection(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    [
    "SELECT json_array(host,login,kind,id,repository,payload,source_updated_at,complete) FROM records ORDER BY host,login,kind,id",
    "SELECT json_array(host,login,source,generation,fetched_at,complete) FROM checkpoints ORDER BY host,login,source",
    "SELECT json_array(host,login,source,generation,active) FROM sync_reservations ORDER BY host,login,source",
    "SELECT json_array(host,login,namespace,repository,resource,kind,mutation_ordinal) FROM sync_mutation_fences ORDER BY host,login,namespace,repository,resource,kind",
    "SELECT json_array(host,login,source,namespace,repository_id,repository,resource,kind,reservation_generation,visible_generation,active) FROM sync_resource_bindings ORDER BY host,login,source"]
    .into_iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn reaction_ready(session: &mut Session, words: &[&str]) {
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, REACTIONS).is_some_and(|body| {
                body.contains("[5 heart]")
                    && body.contains("[r Remove mine]")
                    && body.contains("[Esc Back]")
                    && words
                        .iter()
                        .all(|word| crate::support::controls::body_has(frame, &body, word))
            })
        },
        "Completed original reaction actor panel, actual controls/body",
        Duration::from_secs(8),
    );
}
fn native(session: &mut Session, fixture: &Fixture, mouse: bool) -> String {
    files(session, fixture, mouse);
    session.resize(120, 40);
    choose(session, mouse, "[v Native diff]", b"v");
    session.wait_for_command_count(fixture, 58);
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, NATIVE_OWNER).is_some_and(|body| {
                body.contains("new-no-newline")
                    && body.contains("[o Options]")
                    && !body.contains("NOT CURRENT")
            })
        },
        "Actual Git-built current Native PR patch",
        Duration::from_secs(8),
    );
    control_v2(session, mouse, NATIVE_OWNER, "[o Options] [q Quit]", b"o");
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, NATIVE_OWNER).is_some_and(|body| {
                body.contains("[T Threads]") && body.contains("[o Back to diff]")
            })
        },
        "Owned Native options exposes the same T action, no duplicate click guess",
        Duration::from_secs(8),
    );
    session.wait_for_overlay_body(NATIVE_OWNER)
}
fn native_threads(session: &mut Session, fixture: &Fixture, mouse: bool) {
    control_v2(session, mouse, NATIVE_OWNER, "[T Threads]", b"T");
    session.wait_for_command_count(fixture, 75);
    owned(
        session,
        &[
            "NATIVE_THREAD_9",
            "native-thread-actor",
            "OUTDATED",
            "[c Comments]",
        ],
    );
}
fn reaction_panel(session: &mut Session, fixture: &Fixture, mouse: bool) {
    native_threads(session, fixture, mouse);
    control_v2(session, mouse, THREAD_TITLE, "[c Comments]", b"c");
    session.wait_for_command_count(fixture, 86);
    panel_ready_v2(session, &["Comment #9008", "Comment #9009"]);
    control_v2(session, mouse, PANEL, "Comment #9009", b"\x1b[B");
    panel_ready_v2(session, &["> Comment #9009"]);
    control_v2(session, mouse, PANEL, "[R Reactions]", b"R");
    session.wait_for_command_count(fixture, 107);
    reaction_ready(
        session,
        &["same-login-not-ownership", "COMPLETE", "reaction #7001"],
    );
}
fn form_ready_reaction(session: &mut Session, operation: Op, preview: bool) {
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
                    && !screen.contains("[External editor]")
                    && !screen.contains("Body:")
            },
            "Genuine no-body reaction Form/phase and full controls",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn form_action(session: &mut Session, mouse: bool, operation: Op, label: &str) {
    if mouse {
        let point = session.wait_for_owned_completed_action(&operation.title(), label, &[]);
        session.click(point.0, point.1);
    } else {
        session.focus_completed_owned_action_v1(
            &operation.title(),
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &["[Preview]", "[Confirm action]", "[Keep draft / Close]"],
            &[],
        );
        session.send_key(b"\r");
    }
}
fn prepared_reaction(session: &mut Session, fixture: &Fixture, mouse: bool, operation: Op) {
    let _ = native(session, fixture, mouse);
    reaction_panel(session, fixture, mouse);
    for (cols, rows) in [(80, 24), (120, 40)] {
        session.resize_owned_header(cols, rows, REACTIONS, &["[5 heart]", "[Esc Back]"]);
        reaction_ready(session, &["same-login-not-ownership", "COMPLETE"]);
    }
    control_v2(
        session,
        mouse,
        REACTIONS,
        operation.control(),
        operation.key(),
    );
    session.wait_for_command_count(fixture, 128);
    form_ready_reaction(session, operation, false);
    form_action(session, mouse, operation, "[Preview]");
    session.wait_for_command_count(fixture, 149);
    form_ready_reaction(session, operation, true);
    let before = session.completed_cursorless_end();
    session.paste("POISON_NO_BODY_REACTION_PREVIEW");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains(&operation.title())
                && text.contains("READ-ONLY PREVIEW")
                && !text.contains("POISON_NO_BODY")
                && !text.contains("[External editor]")
        },
        "Read-only no-body Preview ignores actual paste without losing approval",
        Duration::from_secs(8),
    );
}
fn detail(session: &mut Session, mouse: bool, wanted: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let frame = session.completed_cursorless_screen().unwrap();
        let body = crate::support::overlay_body(frame, REACTION_HISTORY).unwrap();
        if body.contains(wanted) {
            return;
        }
        let endpoint = session.completed_cursorless_end();
        control_v2(session, mouse, REACTION_HISTORY, "[PgDn Next]", b"\x1b[6~");
        session.wait_for_new_cursorless_condition(
            endpoint,
            |frame| {
                crate::support::overlay_body(frame, REACTION_HISTORY)
                    .is_some_and(|now| now != body && now.contains("INSPECT ONLY"))
            },
            "One acknowledged owned reaction history viewport change",
            deadline.saturating_duration_since(Instant::now()),
        );
        assert!(Instant::now() < deadline);
    }
}
fn restart_inspect(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    start: usize,
    legacy: bool,
    expected: &[String],
) {
    fresh_summary_v2(session, fixture, mouse, start);
    choose(session, mouse, "[J Thread history]", b"J");
    session.wait_for_command_count(fixture, start + 39);
    history_v1::history_ready(session, &["[R Reaction history]"]);
    control_v2(
        session,
        mouse,
        history_v1::HISTORY,
        "[R Reaction history]",
        b"R",
    );
    session.wait_for_command_count(fixture, start + 47);
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, REACTION_HISTORY).is_some_and(|body| {
                body.contains("thread-reaction-") && body.contains("[Enter Open]")
            })
        },
        "Fresh Summary/account bracket lists only exact closed V3 reaction attempts",
        Duration::from_secs(8),
    );
    control_v2(session, mouse, REACTION_HISTORY, "[Enter Open]", b"\r");
    session.wait_for_command_count(fixture, start + 55);
    detail(
        session,
        mouse,
        if legacy {
            "Protected legacy comment/reaction"
        } else {
            "Exact protected LOCAL no-body"
        },
    );
    for (cols, rows) in [(80, 24), (120, 40)] {
        session.resize_owned_header(
            cols,
            rows,
            REACTION_HISTORY,
            &["[Enter Open]", "[Esc Back]"],
        );
        let calls = commands(session, fixture);
        let endpoint = session.completed_cursorless_end();
        control_v2(session, mouse, REACTION_HISTORY, "[Enter Open]", b"\r");
        session.wait_for_new_cursorless_condition(
            endpoint,
            |frame| {
                crate::support::overlay_body(frame, REACTION_HISTORY).is_some_and(|body| {
                    body.contains("INSPECT ONLY")
                        && !body.contains("[Confirm action]")
                        && !body.contains("[External editor]")
                })
            },
            "Disabled historical Open never restores writer, approval or retry",
            Duration::from_secs(8),
        );
        assert_eq!(commands(session, fixture), calls);
        assert_eq!(protected(fixture), expected);
    }
    let detail_end = session.completed_cursorless_end();
    control_v2(session, mouse, REACTION_HISTORY, "[Esc Back]", b"\x1b");
    session.wait_for_new_cursorless_condition(
        detail_end,
        |frame| {
            crate::support::overlay_body(frame, REACTION_HISTORY).is_some_and(|body| {
                body.contains("> #")
                    && body.contains("[Enter Open]")
                    && !body.contains("Exact protected LOCAL no-body")
            })
        },
        "Actual reaction-history list after detail Back, before a separate second Back",
        Duration::from_secs(8),
    );
    let list_end = session.completed_cursorless_end();
    control_v2(session, mouse, REACTION_HISTORY, "[Esc Back]", b"\x1b");
    session.wait_for_new_cursorless_condition(
        list_end,
        |frame| {
            crate::support::overlay_body(frame, REACTION_HISTORY).is_none()
                && crate::support::overlay_body(frame, history_v1::HISTORY)
                    .is_some_and(|body| body.contains("[R Reaction history]"))
        },
        "Actual parent Thread history after reaction list Back, never stale nested footer",
        Duration::from_secs(8),
    );
    history_v1::history_ready(session, &["[R Reaction history]"]);
    control_v2(session, mouse, history_v1::HISTORY, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(history_v1::HISTORY);
    assert_eq!(protected(fixture), expected);
}
fn journey(mouse: bool, operation: Op, finish: Finish) {
    let (fixture, h, mut steps) = immutable_images::fixture_and_steps(|_| {});
    let mut later = browse(Operation::Reply);
    later.extend(observation(Operation::Reply));
    later.extend(fresh());
    later.extend(fresh());
    later.extend(fresh());
    later.extend(confirm_reaction(operation, finish));
    bind(&mut later, &h);
    steps.extend(later);
    let end = steps.len();
    let restart = !matches!(
        finish,
        Finish::Happy | Finish::PreReplace | Finish::PostReplace
    );
    if restart {
        let mut next = initial();
        next.extend(history_v1::phase_history());
        next.extend(history_v1::phase_history());
        next.extend(history_v1::phase_history());
        bind(&mut next, &h);
        steps.extend(next);
    }
    fixture.scripted_gh(&steps);
    Cache::open(&database(&fixture))
        .unwrap()
        .put_draft(
            &Scope::github(HOST, Some(43)).unwrap(),
            "foreign-protected-reaction",
            FOREIGN,
        )
        .unwrap();
    if finish == Finish::CleanupFailure {
        fixture_write_v2(&fixture).execute_batch("CREATE TRIGGER refuse_reaction_cleanup BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'thread-reaction-draft-v3/%' BEGIN SELECT RAISE(ABORT,'actual conditional cleanup failure'); END;").unwrap();
    }
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    prepared_reaction(&mut session, &fixture, mouse, operation);
    let other = unrelated(&fixture);
    let texts = text_projection(&fixture);
    let trace = git_commands(&fixture);
    let endpoint = session.completed_cursorless_end();
    form_action(&mut session, mouse, operation, "[Confirm action]");
    let held = if matches!(finish, Finish::PostCancel | Finish::PostReplace) {
        26
    } else {
        18
    };
    let child = held_child(&mut session, &fixture, 149 + held);
    let encoded: String = raw(&fixture)
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'thread-reaction-draft-v3/%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let value: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(value["version"], 3);
    assert_eq!(value["values"], json!({"body":""}));
    assert!(value.get("approved").is_none());
    let row:(i64,String,String,String,String)=raw(&fixture).query_row("SELECT attempt_id,command,state,selected_resource,payload_sha256 FROM pr_write_attempts",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap();
    assert_eq!(row.0, value["publication"]["attempt_id"].as_i64().unwrap());
    assert_eq!(row.1, operation.command());
    assert_eq!(row.2, "pending");
    assert!(row.3.starts_with("thread-reaction-v3/"));
    assert_eq!(row.4, value["publication"]["payload_sha256"]);
    let origin: String = raw(&fixture)
        .query_row(
            "SELECT origin_json FROM pr_write_thread_reaction_origins_v3",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let origin: Value = serde_json::from_str(&origin).unwrap();
    assert_eq!(origin["reference"], value["publication"]);
    assert_eq!(origin["seed"]["scope"]["login"], "user-id:42");
    assert_eq!(origin["seed"]["author_account_id"], 77);
    assert_eq!(origin["seed"]["comment_id"], 9009);
    assert_eq!(origin["seed"]["comment_node"], "NATIVE_COMMENT_9");
    assert_eq!(origin["seed"]["comment_commit"], "c".repeat(40));
    assert_eq!(origin["seed"]["head"], h.h);
    assert_ne!(origin["seed"]["comment_commit"], origin["seed"]["head"]);
    assert!(origin["seed"].get("body").is_none());
    if operation == Op::Remove {
        assert_eq!(origin["seed"]["selected"]["id"], 7001);
        assert_eq!(origin["seed"]["selected"]["account_id"], 42);
    }
    let now = process_identity(child.pid)
        .unwrap()
        .expect("Actual original held child stays live through SQL/ref/origin witness");
    assert_eq!(
        (now.birth, now.uid, now.session),
        (child.birth, child.uid, child.session)
    );
    assert!(!now.zombie);
    let external = if matches!(finish, Finish::PreReplace | Finish::PostReplace) {
        let mut changed = value.clone();
        changed["publication"]["payload_sha256"] = json!("f".repeat(64));
        let changed = changed.to_string();
        assert_ne!(changed, encoded);
        assert_eq!(
            fixture_write_v2(&fixture)
                .execute(
                    "UPDATE drafts SET body=?1 WHERE resource LIKE 'thread-reaction-draft-v3/%'",
                    [&changed]
                )
                .unwrap(),
            1
        );
        let now = process_identity(child.pid).unwrap().expect(
            "Same exact actual child remains held after external original-DB row replacement",
        );
        assert_eq!(
            (now.birth, now.uid, now.session),
            (child.birth, child.uid, child.session)
        );
        assert!(!now.zombie);
        Some(changed)
    } else {
        None
    };
    if matches!(
        finish,
        Finish::PreCancel | Finish::PostCancel | Finish::Legacy
    ) {
        control_v2(
            &mut session,
            mouse,
            &operation.title(),
            "[Esc Cancel]",
            b"\x1b",
        );
        session.wait_for_new_cursorless_condition(endpoint,|frame|{let text=frame.contents();text.contains("publication canceled")&&!text.contains(THREAD_BODY)&&!text.contains("same-login-not-ownership")&&!text.contains(&operation.title())&&!text.contains(NATIVE_OWNER)},"Cancel hides only original invoked/pending private source, never leaves old Native/actors",Duration::from_secs(8));
        wait_original_child_gone(&mut session, &child);
        let wanted = if finish == Finish::PostCancel {
            "uncertain"
        } else {
            "pending"
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
                "Actual original worker state {state}, expected {wanted}"
            );
            session.poll_output(Duration::from_millis(1));
        }
        assert_eq!(
            raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'thread-reaction-draft-v3/%'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            encoded
        );
    } else {
        session.wait_for_command_count(&fixture, end);
        session.wait_for_new_cursorless_condition(
            endpoint,
            |frame| {
                let text = frame.contents();
                let outcome = if finish == Finish::PreReplace {
                    text.contains("Reaction write not started • protected record retained")
                        && text.contains("inspect without resend")
                } else {
                    text.contains("refresh explicitly")
                };
                crate::support::main_footer_visible(frame)
                    && outcome
                    && !text.contains(THREAD_BODY)
                    && !text.contains("same-login-not-ownership")
                    && !text.contains(&operation.title())
                    && !text.contains(NATIVE_OWNER)
            },
            "Exact prewrite refusal or observed/unknown effect retires only original private presentation",
            Duration::from_secs(8),
        );
        wait_original_child_gone(&mut session, &child);
        let (state, effect): (String, Option<String>) = raw(&fixture)
            .query_row(
                "SELECT state,confirmed_json FROM pr_write_attempts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            state,
            if finish == Finish::Partial {
                "uncertain"
            } else if finish == Finish::PreReplace {
                "pending"
            } else {
                "confirmed"
            }
        );
        if matches!(finish, Finish::Partial | Finish::PreReplace) {
            assert!(effect.is_none());
        } else {
            let effect: Value = serde_json::from_str(effect.as_deref().unwrap()).unwrap();
            assert_eq!(
                effect["kind"],
                if operation == Op::Add {
                    "reaction"
                } else {
                    "reaction_removed"
                }
            );
            assert_eq!(
                effect["resource_id"],
                if operation == Op::Add { "7004" } else { "7001" }
            );
            assert_eq!(
                effect["node_id"],
                if operation == Op::Add {
                    "NATIVE_REACTION_7004"
                } else {
                    "NATIVE_REACTION_7001"
                }
            );
        }
        let count: u64 = raw(&fixture)
            .query_row(
                "SELECT count(*) FROM drafts WHERE resource LIKE 'thread-reaction-draft-v3/%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, u64::from(finish != Finish::Happy));
    }
    if let Some(external) = external {
        assert_eq!(
            raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'thread-reaction-draft-v3/%'",
                    [],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            external,
            "Exact later external row must survive original prePOST refusal or conditional cleanup"
        );
    }
    let writes = usize::from(!matches!(
        finish,
        Finish::PreCancel | Finish::Legacy | Finish::PreReplace
    ));
    assert_eq!(text_projection(&fixture), texts);
    exact_reactions(&session, &fixture, &steps[..end], writes);
    assert_eq!(unrelated(&fixture), other);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
    if restart {
        if finish == Finish::Legacy {
            fixture_write_v2(&fixture)
                .execute("DELETE FROM pr_write_thread_reaction_origins_v3", [])
                .unwrap();
        }
        let saved = protected(&fixture);
        let mut next = launch(&fixture, mouse);
        assert!(!next.screen().contains("same-login-not-ownership"));
        restart_inspect(
            &mut next,
            &fixture,
            mouse,
            end,
            finish == Finish::Legacy,
            &saved,
        );
        exact_reactions(&next, &fixture, &steps, writes);
        assert_eq!(unrelated(&fixture), other);
        assert_eq!(git_state(&fixture), git);
        assert!(git_commands(&fixture).starts_with(&trace));
        assert_read_only_git(&fixture);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        next.quit_and_assert_restored_terminal(mouse);
    }
}
#[test]
fn review_threads_v3_reaction_keyboard_native_add_actual201_actor_receipt_and_empty_text_impact() {
    journey(false, Op::Add, Finish::Happy)
}
#[test]
fn review_threads_v3_reaction_mouse_native_add_actual201_actor_receipt_and_empty_text_impact() {
    journey(true, Op::Add, Finish::Happy)
}
#[test]
fn review_threads_v3_reaction_keyboard_native_remove_owned_actual204_complete_absence() {
    journey(false, Op::Remove, Finish::Happy)
}
#[test]
fn review_threads_v3_reaction_mouse_native_remove_owned_actual204_complete_absence() {
    journey(true, Op::Remove, Finish::Happy)
}
#[test]
fn review_threads_v3_reaction_keyboard_prepost_cancel_pending_restart_no_resend() {
    journey(false, Op::Add, Finish::PreCancel)
}
#[test]
fn review_threads_v3_reaction_mouse_prepost_cancel_pending_restart_no_resend() {
    journey(true, Op::Add, Finish::PreCancel)
}
#[test]
fn review_threads_v3_reaction_keyboard_postinvoke_cancel_uncertain_restart_no_resend() {
    journey(false, Op::Add, Finish::PostCancel)
}
#[test]
fn review_threads_v3_reaction_mouse_postinvoke_cancel_uncertain_restart_no_resend() {
    journey(true, Op::Add, Finish::PostCancel)
}
#[test]
fn review_threads_v3_reaction_keyboard_partial_delete_readback_remains_uncertain_restart() {
    journey(false, Op::Remove, Finish::Partial)
}
#[test]
fn review_threads_v3_reaction_mouse_partial_delete_readback_remains_uncertain_restart() {
    journey(true, Op::Remove, Finish::Partial)
}
#[test]
fn review_threads_v3_reaction_keyboard_confirmed_cleanup_failure_protected_restart() {
    journey(false, Op::Remove, Finish::CleanupFailure)
}
#[test]
fn review_threads_v3_reaction_mouse_confirmed_cleanup_failure_protected_restart() {
    journey(true, Op::Remove, Finish::CleanupFailure)
}
#[test]
fn review_threads_v3_reaction_keyboard_protected_legacy_origin_never_reconstructed() {
    journey(false, Op::Add, Finish::Legacy)
}
#[test]
fn review_threads_v3_reaction_mouse_protected_legacy_origin_never_reconstructed() {
    journey(true, Op::Add, Finish::Legacy)
}
#[test]
fn review_threads_v3_reaction_keyboard_external_draft_replacement_before_post_refuses_and_retains_newer(
) {
    journey(false, Op::Add, Finish::PreReplace)
}
#[test]
fn review_threads_v3_reaction_mouse_external_draft_replacement_before_post_refuses_and_retains_newer(
) {
    journey(true, Op::Add, Finish::PreReplace)
}
#[test]
fn review_threads_v3_reaction_keyboard_external_draft_replacement_after_receipt_prevents_cleanup() {
    journey(false, Op::Add, Finish::PostReplace)
}
#[test]
fn review_threads_v3_reaction_mouse_external_draft_replacement_after_receipt_prevents_cleanup() {
    journey(true, Op::Add, Finish::PostReplace)
}

fn refusal(mouse: bool, account_loss: bool) {
    let (fixture, h, mut steps) = immutable_images::fixture_and_steps(|_| {});
    let mut later = browse(Operation::Reply);
    later.extend(observation(Operation::Reply));
    later.extend(fresh());
    if account_loss {
        later.extend(fresh());
        later.extend(fresh());
        later.extend(principal(43));
    }
    bind(&mut later, &h);
    steps.extend(later);
    fixture.scripted_gh(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    if account_loss {
        prepared_reaction(&mut session, &fixture, mouse, Op::Add);
    } else {
        let _ = native(&mut session, &fixture, mouse);
        reaction_panel(&mut session, &fixture, mouse);
    }
    if account_loss {
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let saved = raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'thread-reaction-draft-v3/%'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .ok();
            if saved
                .as_deref()
                .and_then(|encoded| serde_json::from_str::<Value>(encoded).ok())
                .is_some_and(|value| {
                    value["version"] == 3
                        && value["values"] == json!({"body":""})
                        && value["publication"].is_null()
                })
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Genuine initial normal autosave must exist before account-loss witness"
            );
            session.poll_output(Duration::from_millis(1));
        }
    }
    let before = sql_state(&fixture);
    let other = unrelated(&fixture);
    let trace = git_commands(&fixture);
    let endpoint = session.completed_cursorless_end();
    if account_loss {
        form_action(&mut session, mouse, Op::Add, "[Confirm action]");
        session.wait_for_command_count(&fixture, 151);
        session.wait_for_new_cursorless_condition(
            endpoint,
            |frame| {
                let text = frame.contents();
                text.contains("refresh explicitly")
                    && !text.contains(THREAD_BODY)
                    && !text.contains("same-login-not-ownership")
                    && !text.contains(&Op::Add.title())
                    && !text.contains(NATIVE_OWNER)
            },
            "Actual changed numeric UID hides original private source before journal/POST",
            Duration::from_secs(8),
        );
        assert_eq!(
            raw(&fixture)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
    } else {
        control_v2(&mut session, mouse, REACTIONS, "reaction #7002", b"\x1b[B");
        reaction_ready(&mut session, &["> eyes", "numeric UID Some(99)"]);
        let calls = commands(&session, &fixture);
        control_v2(&mut session, mouse, REACTIONS, "[r Remove mine]", b"r");
        session.wait_for_new_cursorless_condition(
            endpoint,
            |frame| {
                crate::support::overlay_body(frame, REACTIONS).is_some_and(|body| {
                    body.contains("numeric UID Some(99)") && !body.contains("[Confirm action]")
                })
            },
            "Same login/different numeric UID disabled Remove has no queue, Form or write",
            Duration::from_secs(8),
        );
        assert_eq!(commands(&session, &fixture), calls);
    }
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(unrelated(&fixture), other);
    exact_reactions(&session, &fixture, &steps, 0);
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
fn review_threads_v3_reaction_keyboard_foreign_numeric_actor_same_login_disabled_remove_no_effects()
{
    refusal(false, false)
}
#[test]
fn review_threads_v3_reaction_mouse_foreign_numeric_actor_same_login_disabled_remove_no_effects() {
    refusal(true, false)
}
#[test]
fn review_threads_v3_reaction_keyboard_actual_current_account_loss_before_write_no_private_return()
{
    refusal(false, true)
}
#[test]
fn review_threads_v3_reaction_mouse_actual_current_account_loss_before_write_no_private_return() {
    refusal(true, true)
}
fn pure_back(mouse: bool) {
    let (fixture, h, mut steps) = immutable_images::fixture_and_steps(|_| {});
    let mut later = browse(Operation::Reply);
    later.extend(observation(Operation::Reply));
    later.extend(fresh());
    bind(&mut later, &h);
    steps.extend(later);
    fixture.scripted_gh(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let native = native(&mut session, &fixture, mouse);
    let sql = sql_state(&fixture);
    let trace = git_commands(&fixture);
    reaction_panel(&mut session, &fixture, mouse);
    for (cols, rows) in [(80, 24), (120, 40)] {
        session.resize_owned_header(cols, rows, REACTIONS, &["[5 heart]", "[Esc Back]"]);
        reaction_ready(&mut session, &["same-login-not-ownership", "COMPLETE"]);
    }
    control_v2(&mut session, mouse, REACTIONS, "[Esc Back]", b"\x1b");
    panel_ready_v2(&mut session, &["> Comment #9009"]);
    control_v2(&mut session, mouse, PANEL, "[Esc Back]", b"\x1b");
    owned(&mut session, &["NATIVE_THREAD_9", "[c Comments]"]);
    let endpoint = session.completed_cursorless_end();
    control_v2(&mut session, mouse, THREAD_TITLE, "[Esc Back]", b"\x1b");
    session.wait_for_new_cursorless_condition(endpoint,|frame|crate::support::overlay_body(frame,NATIVE_OWNER).is_some_and(|body|body==native)&&crate::support::overlay_body(frame,THREAD_TITLE).is_none(),"Pure nested Back returns exact saved original Native options/viewport, no compensating request",Duration::from_secs(8));
    exact_reactions(&session, &fixture, &steps, 0);
    assert_eq!(sql_state(&fixture), sql);
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
fn review_threads_v3_reaction_keyboard_native_pure_nested_back_preserves_original_viewport() {
    pure_back(false)
}
#[test]
fn review_threads_v3_reaction_mouse_native_pure_nested_back_preserves_original_viewport() {
    pure_back(true)
}
fn long_actor(mouse: bool) {
    let (fixture, h, mut steps) = immutable_images::fixture_and_steps(|_| {});
    let mut later = browse(Operation::Reply);
    later.extend(observation(Operation::Reply));
    later.extend(fresh());
    let suffix = "VISIBLE_REACTION_ACTOR_SUFFIX";
    let login = format!("{}界e\u{301}_{suffix}", "long_".repeat(36));
    for step in &mut later {
        if step
            .args
            .get(1)
            .is_some_and(|endpoint| endpoint.contains("/reactions?"))
        {
            let mut rows: Value = serde_json::from_slice(&step.stdout).unwrap();
            rows[0]["user"]["login"] = json!(login);
            step.stdout = rows.to_string().into_bytes();
        }
    }
    bind(&mut later, &h);
    steps.extend(later);
    fixture.scripted_gh(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let original_native = native(&mut session, &fixture, mouse);
    let sql = sql_state(&fixture);
    let trace = git_commands(&fixture);
    native_threads(&mut session, &fixture, mouse);
    control_v2(&mut session, mouse, THREAD_TITLE, "[c Comments]", b"c");
    session.wait_for_command_count(&fixture, 86);
    panel_ready_v2(&mut session, &["Comment #9009"]);
    control_v2(&mut session, mouse, PANEL, "Comment #9009", b"\x1b[B");
    panel_ready_v2(&mut session, &["> Comment #9009"]);
    control_v2(&mut session, mouse, PANEL, "[R Reactions]", b"R");
    session.wait_for_command_count(&fixture, 107);
    reaction_ready(&mut session, &["COMPLETE", "numeric UID Some(99)"]);
    for (index, (cols, rows)) in [(80, 24), (120, 40)].into_iter().enumerate() {
        session.resize_owned_header(cols, rows, REACTIONS, &["[→ Right]", "[Home Top]"]);
        if index > 0 {
            let before = session.completed_cursorless_end();
            control_v2(&mut session, mouse, REACTIONS, "[Home Top]", b"\x1b[H");
            session.wait_for_new_cursorless_condition(
                before,
                |frame| {
                    crate::support::overlay_body(frame, REACTIONS).is_some_and(|body| {
                        body.contains("long_long_")
                            && body.contains("numeric UID Some(99)")
                            && !body.contains(suffix)
                    })
                },
                "Actual same-source horizontal reset acknowledged before further scrolling",
                Duration::from_secs(8),
            );
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            let frame = session.completed_cursorless_screen().unwrap();
            let previous = crate::support::overlay_body(frame, REACTIONS).unwrap();
            if previous.contains(suffix) {
                break;
            }
            let endpoint = session.completed_cursorless_end();
            control_v2(&mut session, mouse, REACTIONS, "[→ Right]", b"\x1b[C");
            session.wait_for_new_cursorless_condition(endpoint,|frame|crate::support::overlay_body(frame,REACTIONS).is_some_and(|body|body!=previous&&body.contains("[Home Top]")),"Exactly one acknowledged owned horizontal change before another action; final exact actor suffix required",deadline.saturating_duration_since(Instant::now()));
            assert!(
                Instant::now() < deadline,
                "Original8s budget, no blind scroll/timeout extension"
            );
        }
        assert_eq!(commands(&session, &fixture).len(), 107);
        assert_eq!(sql_state(&fixture), sql);
        assert_eq!(git_commands(&fixture), trace);
    }
    control_v2(&mut session, mouse, REACTIONS, "[Esc Back]", b"\x1b");
    panel_ready_v2(&mut session, &["> Comment #9009"]);
    control_v2(&mut session, mouse, PANEL, "[Esc Back]", b"\x1b");
    owned(&mut session, &["NATIVE_THREAD_9"]);
    let endpoint = session.completed_cursorless_end();
    control_v2(&mut session, mouse, THREAD_TITLE, "[Esc Back]", b"\x1b");
    session.wait_for_new_cursorless_condition(
        endpoint,
        |frame| {
            crate::support::overlay_body(frame, NATIVE_OWNER)
                .is_some_and(|body| body == original_native)
        },
        "Long actual actor browsing preserves exact original Native frame on pure Back",
        Duration::from_secs(8),
    );
    exact_reactions(&session, &fixture, &steps, 0);
    assert_eq!(sql_state(&fixture), sql);
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
fn review_threads_v3_reaction_keyboard_long_unicode_actor_heading_reachable_and_exact_native_back()
{
    long_actor(false)
}
#[test]
fn review_threads_v3_reaction_mouse_long_unicode_actor_heading_reachable_and_exact_native_back() {
    long_actor(true)
}

fn native_thread_write(mouse: bool, operation: Operation) {
    let (fixture, h, mut steps) = immutable_images::fixture_and_steps(|_| {});
    let mut later = browse(operation);
    later.extend(observation(operation));
    later.extend(observation(operation));
    later.extend(confirm(operation));
    bind(&mut later, &h);
    steps.extend(later);
    fixture.scripted_gh(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let _ = native(&mut session, &fixture, mouse);
    native_threads(&mut session, &fixture, mouse);
    let trace = git_commands(&fixture);
    control_v2(
        &mut session,
        mouse,
        THREAD_TITLE,
        operation.control(),
        operation.key(),
    );
    session.wait_for_command_count(&fixture, 86);
    form_ready(&mut session, operation, false);
    if operation == Operation::Reply {
        if mouse {
            session.click_owned_form_field("PR INTENT • Preview first", "Body");
        }
        session.paste(REPLY);
        session
            .try_wait_for_condition(
                |screen| {
                    screen.contains("Original native reply 界") && screen.contains("second line")
                },
                "Original bounded Native-origin thread reply body",
                Duration::from_secs(8),
            )
            .unwrap();
    }
    form_control(&mut session, mouse, operation, "[Preview]");
    session.wait_for_command_count(&fixture, 97);
    form_ready(&mut session, operation, true);
    let endpoint = session.completed_cursorless_end();
    form_control(&mut session, mouse, operation, "[Confirm action]");
    session.wait_for_command_count(&fixture, 117);
    session.wait_for_new_cursorless_condition(endpoint,|frame|{let text=frame.contents();text.contains("refresh explicitly")&&!text.contains(THREAD_BODY)&&!text.contains(REPLY)&&!text.contains(NATIVE_OWNER)&&!text.contains(&operation.title())},"Native-origin actual reply/resolve observes one receipt then retires original private view",Duration::from_secs(8));
    // Existing exact thread protocol accepts ONLY actual REST reply or exact
    // resolve mutation and compares every original GraphQL stdin/argv/ordinal.
    exact(&session, &fixture, &steps, 1);
    let (state, effect): (String, String) = raw(&fixture)
        .query_row(
            "SELECT state,confirmed_json FROM pr_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(state, "confirmed");
    let effect: Value = serde_json::from_str(&effect).unwrap();
    assert_eq!(
        effect["kind"],
        if operation == Operation::Reply {
            "comment"
        } else {
            "thread"
        }
    );
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
fn review_threads_v3_native_keyboard_actual_reply_guarded_receipt_and_private_retirement() {
    native_thread_write(false, Operation::Reply)
}
#[test]
fn review_threads_v3_native_mouse_actual_reply_guarded_receipt_and_private_retirement() {
    native_thread_write(true, Operation::Reply)
}
#[test]
fn review_threads_v3_native_keyboard_actual_resolve_guarded_state_and_private_retirement() {
    native_thread_write(false, Operation::Resolve)
}
#[test]
fn review_threads_v3_native_mouse_actual_resolve_guarded_state_and_private_retirement() {
    native_thread_write(true, Operation::Resolve)
}
