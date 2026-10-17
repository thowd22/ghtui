//! DECLARED / UNRUN. A fresh server thread observation joins an actual FILE
//! root ID to its genuine opaque thread; no thread ID is derived from comment.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
enum FixtureSide {
    Request,
    Response,
}
fn bind(value: &mut Value, side: FixtureSide) {
    match value {
        Value::String(text) => {
            *text = match text.as_str() {
                "NATIVE_COMMENT_9" => "NATIVE_FILE_COMMENT_9181".into(),
                "9009" => "9181".into(),
                "src/old.rs" => "src/界.rs".into(),
                THREAD_BODY => "PRIVATE_NATIVE_WHOLE_FILE_BODY 界\nsecond FILE line".into(),
                old if old == "c".repeat(40) || old == "d".repeat(40) => "a".repeat(40),
                _ => return,
            };
        }
        Value::Number(n) if n.as_u64() == Some(9009) => *value = json!(9181),
        Value::Array(values) => {
            for value in values {
                bind(value, side);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                bind(value, side);
            }
            // Requests need exact identity substitution only. Response-only
            // author/date/FILE fields must never leak into ID-only variables.
            if side == FixtureSide::Request {
                return;
            }
            if values.contains_key("subjectType") {
                values.insert("subjectType".into(), json!("FILE"));
                values.insert("isOutdated".into(), json!(false));
            }
            if values.get("id") == Some(&json!("NATIVE_THREAD_9"))
                && values.contains_key("isResolved")
            {
                values.insert("path".into(), json!("src/界.rs"));
            }
            for key in ["line", "startLine", "originalLine", "originalStartLine"] {
                if values.contains_key(key) {
                    values.insert(key.into(), Value::Null);
                }
            }
            if values.get("id") == Some(&json!("NATIVE_FILE_COMMENT_9181")) {
                values.insert("author".into(),json!({"__typename":"User","databaseId":42,"login":"actual-file-author","avatarUrl":null}));
                if values.contains_key("viewerDidAuthor") {
                    values.insert("viewerDidAuthor".into(), json!(true));
                }
                values.insert("outdated".into(), json!(false));
                values.insert("createdAt".into(), json!("2026-10-15T01:00:00Z"));
                if values
                    .get("updatedAt")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.starts_with("2026-10-01"))
                {
                    values.insert("updatedAt".into(), json!("2026-10-15T01:00:00Z"));
                }
            }
            if values.get("node_id") == Some(&json!("NATIVE_FILE_COMMENT_9181")) {
                values.insert(
                    "user".into(),
                    json!({"id":42,"login":"actual-file-author","type":"User","avatar_url":null}),
                );
                values.insert("subject_type".into(), json!("file"));
                values.insert("in_reply_to_id".into(), Value::Null);
                values.insert("pull_request_review_id".into(), json!(600));
                values.insert("created_at".into(), json!("2026-10-15T01:00:00Z"));
                if values
                    .get("updated_at")
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.starts_with("2026-10-01"))
                {
                    values.insert("updated_at".into(), json!("2026-10-15T01:00:00Z"));
                }
                for key in ["line", "start_line", "original_line", "original_start_line"] {
                    values.insert(key.into(), Value::Null);
                }
            }
            if values.get("id") == Some(&json!("ACTUAL_REVIEW_600")) {
                values.insert("id".into(), json!("NATIVE_FILE_REVIEW_600"));
                values.insert("fullDatabaseId".into(), json!("600"));
            }
        }
        _ => {}
    }
}
pub(super) fn adapt(steps: &mut [GhStep]) {
    for step in steps {
        for arg in &mut step.args {
            *arg = arg.replace("/comments/9009", "/comments/9181");
        }
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            bind(&mut value, FixtureSide::Response);
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
        if !step.stdin.is_empty() {
            let mut value: Value = serde_json::from_slice(&step.stdin).unwrap();
            bind(&mut value, FixtureSide::Request);
            step.stdin = serde_json::to_vec(&value).unwrap();
        }
    }
}
pub(crate) fn steps() -> Vec<GhStep> {
    let mut steps = browse(Operation::Reply);
    steps.extend(observation(Operation::Reply));
    steps.extend(observation(Operation::Reply));
    steps.extend(confirm(Operation::Reply));
    adapt(&mut steps);
    assert_eq!(steps.len(), 59);
    steps
}
pub(crate) fn follow(session: &mut Session, fixture: &Fixture, mouse: bool, start: usize) {
    choose(session, mouse, "[T Threads]", b"T");
    session.wait_for_command_count(fixture, start + 17);
    owned(session, &["FILE", "actual-file-author", "[p Reply]"]);
    choose(session, mouse, "[p Reply]", b"p");
    session.wait_for_command_count(fixture, start + 28);
    form_ready(session, Operation::Reply, false);
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", "Body");
    }
    session.paste(REPLY);
    form_control(session, mouse, Operation::Reply, "[Preview]");
    session.wait_for_command_count(fixture, start + 39);
    form_ready(session, Operation::Reply, true);
    let at = session.completed_cursorless_end();
    form_control(session, mouse, Operation::Reply, "[Confirm action]");
    session.wait_for_command_count(fixture, start + 59);
    session.wait_for_new_cursorless_condition(
        at,
        |frame| {
            let s = frame.contents();
            s.contains("Reply posted #901")
                && s.contains("refresh explicitly")
                && !s.contains("PRIVATE_NATIVE_WHOLE_FILE_BODY")
                && !s.contains("Original native reply 界")
                && !s.contains(THREAD_TITLE)
        },
        "Fresh actual FILE thread root reply verified by original controller; safe receipt only",
        Duration::from_secs(8),
    );
    let db = raw(fixture);
    let(count,command,selected):(u64,String,String)=db.query_row("SELECT (SELECT count(*) FROM pr_write_attempts),command,selected_resource FROM pr_write_attempts WHERE command='thread-reply'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(count, 2);
    assert_eq!(command, "thread-reply");
    assert!(selected.starts_with("thread-reply/"));
    let original: String = db
        .query_row(
            "SELECT origin_json FROM pr_write_thread_origins_v1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let original: Value = serde_json::from_str(&original).unwrap();
    assert_eq!(original["seed"]["root"]["rest_id"], 9181);
    assert_eq!(original["seed"]["root"]["node"], "NATIVE_FILE_COMMENT_9181");
    assert_eq!(original["seed"]["root"]["commit"], "a".repeat(40));
    assert_eq!(original["seed"]["root"]["path"], "src/界.rs");
}
