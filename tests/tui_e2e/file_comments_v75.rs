//! DECLARED / UNRUN. Genuine source/Files, actual terminal input and a strict
//! offline single-invocation protocol. Not snapshots/direct handler coverage.
use super::*;
use rusqlite::OptionalExtension;
const BODY: &str = "PRIVATE_NATIVE_WHOLE_FILE_BODY 界\nsecond FILE line";
const PATH: &str = "src/界.rs";
const HISTORY: &str = "FILE comment publication history • inspect only";
const QUERY: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequestReviewComment{id fullDatabaseId subjectType body state path line startLine originalLine originalStartLine commit{oid} originalCommit{oid} replyTo{id fullDatabaseId} createdAt updatedAt author{__typename login avatarUrl ... on User{databaseId name} ... on Bot{databaseId}} pullRequestReview{id fullDatabaseId state} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Finish {
    Happy,
    HappyFollow,
    PreviewClose,
    StaleMember,
    ChangedHead,
    RecreatedRepository,
    AccountLoss,
    CancelThenFreshSource,
    CancelPre,
    CancelPost,
    CleanupFailed,
    ReplacePre,
    ReplacePost,
    Malformed,
}
pub(crate) fn title() -> String {
    format!("FILE comment • {FIRST}#7")
}
fn members() -> Value {
    json!([{"filename":PATH,"status":"modified","additions":2,"deletions":1}])
}
fn opening() -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    steps.push(get(
        &format!("repos/{FIRST}/pulls/7/files?per_page=100&page=1"),
        members(),
    ));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    assert_eq!(steps.len(), 6);
    steps
}
fn last_guard() -> Vec<GhStep> {
    vec![
        get(
            "user",
            json!({"id":42,"login":"renamed-same-positive-file-author"}),
        ),
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
    ]
}
fn close() -> Vec<GhStep> {
    let mut s = vec![get(&format!("repos/{FIRST}"), repository(0))];
    s.extend(principal(42));
    s
}
pub(crate) fn observation() -> Vec<GhStep> {
    let mut s = opening();
    s.extend(last_guard());
    s.extend(close());
    assert_eq!(s.len(), 11);
    s
}
fn rest() -> Value {
    json!({"id":9181,"node_id":"NATIVE_FILE_COMMENT_9181","pull_request_review_id":600,
        "pull_request_url":format!("https://{HOST}/api/v3/repos/{FIRST}/pulls/7"),"path":PATH,"body":BODY,
        "commit_id":"a".repeat(40),"original_commit_id":"a".repeat(40),"subject_type":"file",
        "line":null,"start_line":null,"original_line":null,"original_start_line":null,"in_reply_to_id":null,
        "user":{"id":42,"login":"actual-file-author","type":"User","avatar_url":null},
        "created_at":"2026-10-15T01:00:00Z","updated_at":"2026-10-15T01:00:00Z"})
}
fn receipts(malformed: bool) -> Vec<GhStep> {
    let mut response = rest();
    if malformed {
        response["user"]["id"] = json!(43);
    }
    let body = response.to_string();
    let mut post = GhStep::read(
        &[
            "api",
            &format!("repos/{FIRST}/pulls/7/comments"),
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--include",
            "--input",
            "-",
        ],
        &format!("HTTP/2.0 201 Created\nContent-Type: application/json\r\n\r\n{body}"),
    );
    post.stdin = serde_json::to_vec(
        &json!({"body":BODY,"commit_id":"a".repeat(40),"path":PATH,"subject_type":"file"}),
    )
    .unwrap();
    if malformed {
        return vec![post];
    }
    let mut pr = graph_parent();
    pr["baseRefOid"] = json!("b".repeat(40));
    let response = json!({"data":{"node":{"__typename":"PullRequestReviewComment","id":"NATIVE_FILE_COMMENT_9181","fullDatabaseId":"9181","subjectType":"FILE","body":BODY,"state":"SUBMITTED","path":PATH,
        "line":null,"startLine":null,"originalLine":null,"originalStartLine":null,"replyTo":null,
        "commit":{"oid":"a".repeat(40)},"originalCommit":{"oid":"a".repeat(40)},"createdAt":"2026-10-15T01:00:00Z","updatedAt":"2026-10-15T01:00:00Z",
        "author":{"__typename":"User","databaseId":42,"login":"actual-file-author","avatarUrl":null},"pullRequestReview":{"id":"NATIVE_FILE_REVIEW_600","fullDatabaseId":"600","state":"COMMENTED"},"pullRequest":pr,"repository":graph_repo()}}});
    let mut node = GhStep::read(
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
        &response.to_string(),
    );
    node.stdin =
        serde_json::to_vec(&json!({"query":QUERY,"variables":{"id":"NATIVE_FILE_COMMENT_9181"}}))
            .unwrap();
    vec![
        post,
        get(&format!("repos/{FIRST}/pulls/comments/9181"), rest()),
        node,
    ]
}
fn visibility() -> Vec<GhStep> {
    vec![
        get("user", json!({"id":42,"login":"after-write-account"})),
        get(&format!("repos/{FIRST}"), repository(0)),
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
        get("user", json!({"id":42,"login":"final-positive-account"})),
    ]
}
fn history_phase() -> Vec<GhStep> {
    let mut s = principal(42);
    s.push(get(&format!("repos/{FIRST}"), repository(0)));
    s.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    s.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    s.push(get(&format!("repos/{FIRST}"), repository(0)));
    s.extend(principal(42));
    assert_eq!(s.len(), 8);
    s
}
fn protected_sql(f: &Fixture) -> Vec<String> {
    let db = raw(f);
    [
    "SELECT json_array(host,login,resource,body) FROM drafts WHERE resource LIKE 'file-comment-draft-v75/%' ORDER BY host,login,resource",
    "SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts ORDER BY attempt_id",
    "SELECT json_array(host,principal,pr_node,generation) FROM pr_write_scopes ORDER BY host,principal,pr_node",
    "SELECT json_array(attempt_id,repository_id,repository_node,pr_id,origin_json) FROM pr_write_file_comment_origins_v75 ORDER BY attempt_id",
].into_iter().flat_map(|q|db.prepare(q).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
pub(crate) fn fresh_summary(session: &mut Session, fixture: &Fixture, mouse: bool, start: usize) {
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
    assert_eq!(commands(session, fixture).len(), start);
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
fn history_ready(session: &mut Session, title: &str, words: &[&str]) {
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, title).is_some_and(|body| {
                body.contains("[Esc Back]")
                    && body.contains("[q Quit]")
                    && words
                        .iter()
                        .all(|word| crate::support::controls::body_has(frame, &body, word))
            })
        },
        "Actual complete original owned FILE/history controls/body",
        Duration::from_secs(8),
    );
}
fn inspect_restart(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    start: usize,
    confirmed: bool,
) {
    let durable = protected_sql(fixture);
    let mut restarted = session.restart_with_same_cache(fixture, 120, 40);
    fresh_summary(&mut restarted, fixture, mouse, start);
    let root = "Thread publication history • inspect only";
    choose(&mut restarted, mouse, "[J Thread history]", b"J");
    let child = held_child(&mut restarted, fixture, start + 31);
    history_ready(&mut restarted, root, &["pending"]);
    let previous =
        crate::support::overlay_body(restarted.completed_cursorless_screen().unwrap(), root)
            .unwrap();
    let endpoint = restarted.completed_cursorless_end();
    if mouse {
        let (x, y) = restarted.wait_for_owned_completed_action(root, "[Enter Open]", &[]);
        restarted.click(x, y);
    } else {
        restarted.send_key(b"\r");
    }
    restarted.wait_for_new_cursorless_condition(
        endpoint,
        |frame| crate::support::overlay_body(frame, root).is_some_and(|body| body == previous),
        "Actual disabled pending Open yields same owned view, not a fresh request",
        Duration::from_secs(8),
    );
    assert_eq!(commands(&restarted, fixture).len(), start + 32);
    assert_eq!(protected_sql(fixture), durable);
    assert!(process_identity(child.pid)
        .unwrap()
        .is_some_and(|p| p.birth == child.birth));
    restarted.wait_for_command_count(fixture, start + 39);
    history_ready(
        &mut restarted,
        root,
        &[
            "[F FILE history]",
            "Fresh current",
            "Read-only local history",
        ],
    );
    choose(&mut restarted, mouse, "[F FILE history]", b"F");
    restarted.wait_for_command_count(fixture, start + 47);
    let actual_state: String = raw(fixture)
        .query_row(
            "SELECT state FROM pr_write_attempts WHERE command='file-comment-create-v75'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let expected_label = match actual_state.as_str() {
        "confirmed" if confirmed => "Confirmed",
        "pending" if !confirmed => "Pending",
        "uncertain" if !confirmed => "Uncertain",
        _ => panic!("Unexpected original protected FILE state"),
    };
    history_ready(&mut restarted, HISTORY, &[expected_label]);
    restarted.resize_owned_header(80, 24, HISTORY, &["[Enter Open]", "[Esc Back]"]);
    restarted.resize_owned_header(120, 40, HISTORY, &["[Enter Open]", "[Esc Back]"]);
    choose(&mut restarted, mouse, "[Enter Open]", b"\r");
    restarted.wait_for_command_count(fixture, start + 55);
    history_ready(&mut restarted, HISTORY, &["INSPECT ONLY"]);
    // The complete body is paged one acknowledged owned viewport at a time.
    let end = Instant::now() + Duration::from_secs(8);
    loop {
        let frame = restarted.completed_cursorless_screen().unwrap();
        let body = crate::support::overlay_body(frame, HISTORY).unwrap();
        if body.contains("PRIVATE_NATIVE_WHOLE_FILE_BODY") {
            break;
        }
        let old = body;
        let at = restarted.completed_cursorless_end();
        choose(&mut restarted, mouse, "[PgDn Next]", b"\x1b[6~");
        restarted.wait_for_new_cursorless_condition(
            at,
            |frame| {
                crate::support::overlay_body(frame, HISTORY)
                    .is_some_and(|body| body != old && body.contains("[Esc Back]"))
            },
            "One owned FILE detail viewport change",
            end.saturating_duration_since(Instant::now()),
        );
        assert!(Instant::now() < end);
    }
    let at = restarted.completed_cursorless_end();
    let previous =
        crate::support::overlay_body(restarted.completed_cursorless_screen().unwrap(), HISTORY)
            .unwrap();
    if mouse {
        let (x, y) = restarted.wait_for_owned_completed_action(HISTORY, "[Enter Open]", &[]);
        restarted.click(x, y);
    } else {
        restarted.send_key(b"\r");
    }
    restarted.wait_for_new_cursorless_condition(
        at,
        |frame| crate::support::overlay_body(frame, HISTORY).is_some_and(|body| body == previous),
        "Actual disabled detail Open preserves exact current owned body",
        Duration::from_secs(8),
    );
    assert_eq!(commands(&restarted, fixture).len(), start + 55);
    assert_eq!(protected_sql(fixture), durable);
    assert!(restarted.completed_cursorless_end() >= at);
    choose(&mut restarted, mouse, "[Esc Back]", b"\x1b");
    history_ready(&mut restarted, HISTORY, &[expected_label]);
    choose(&mut restarted, mouse, "[Esc Back]", b"\x1b");
    history_ready(&mut restarted, root, &["[F FILE history]"]);
    choose(&mut restarted, mouse, "[Esc Back]", b"\x1b");
    restarted.wait_for_cursorless_condition(
        |f| {
            let s = f.contents();
            s.contains(DETAIL_TITLE)
                && !s.contains(HISTORY)
                && !s.contains("PRIVATE_NATIVE_WHOLE_FILE_BODY")
        },
        "Exact restored current Summary; historical private body erased",
        Duration::from_secs(8),
    );
    assert_eq!(protected_sql(fixture), durable);
    restarted.quit_and_assert_restored_terminal(mouse);
}
fn confirm(finish: Finish) -> (Vec<GhStep>, Option<usize>) {
    let mut s = opening();
    s.extend(last_guard());
    s.extend(close());
    match finish {
        Finish::ChangedHead => {
            let mut raw = parent_resource();
            raw["head"]["sha"] = json!("d".repeat(40));
            s[3].stdout = serde_json::to_vec(&raw).unwrap();
            s.truncate(4);
            return (s, None);
        }
        Finish::RecreatedRepository => {
            let mut raw = repository(0);
            raw["id"] = json!(70000);
            raw["node_id"] = json!("RECREATED_REPO_NODE_70000");
            s[2].stdout = serde_json::to_vec(&raw).unwrap();
            s.truncate(3);
            return (s, None);
        }
        Finish::AccountLoss => {
            s[1].stdout = json!({"id":43,"login":"different-current-account"})
                .to_string()
                .into_bytes();
            s.truncate(2);
            return (s, None);
        }
        _ => {}
    }
    if finish == Finish::StaleMember {
        s[4].stdout = json!([{"filename":"src/different.rs","status":"modified"}])
            .to_string()
            .into_bytes();
        s.truncate(5);
        return (s, None);
    }
    s.extend(receipts(finish == Finish::Malformed));
    if finish != Finish::Malformed {
        s.extend(visibility());
    }
    let held = match finish {
        Finish::CancelPre
        | Finish::CancelThenFreshSource
        | Finish::ReplacePre
        | Finish::CleanupFailed => Some(6),
        Finish::CancelPost => Some(12),
        Finish::ReplacePost => Some(16),
        _ => None,
    };
    if let Some(index) = held {
        s[index].delay_ms = 6000;
        if matches!(
            finish,
            Finish::CancelPre | Finish::CancelThenFreshSource | Finish::CancelPost
        ) {
            s.truncate(index + 1);
        }
        if finish == Finish::ReplacePre {
            s.truncate(9);
        }
    }
    if finish == Finish::CleanupFailed {
        s.truncate(17);
    }
    if finish == Finish::ReplacePost {
        s.truncate(17);
    }
    if matches!(finish, Finish::Happy | Finish::HappyFollow) {
        assert_eq!(s.len(), 18);
    }
    (s, held)
}
pub(crate) fn happy_confirmation_v75() -> Vec<GhStep> {
    confirm(Finish::Happy).0
}
pub(crate) fn form_control(session: &mut Session, mouse: bool, label: &str) {
    if mouse {
        let p = session.wait_for_owned_completed_action(&title(), label, &[]);
        session.click(p.0, p.1);
    } else {
        let mut controls = vec![
            "[Preview]",
            "[External editor]",
            "[Confirm action]",
            "[Keep draft / Close]",
        ];
        if session
            .completed_cursorless_screen()
            .is_some_and(|frame| frame.contents().contains("[F7 Error↑]"))
        {
            controls.extend(["[F7 Error↑]", "[F8 Error↓]"]);
        }
        session.focus_completed_owned_action_v1(
            &title(),
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &controls,
            &[],
        );
        session.send_key(b"\r");
    }
}
fn form_action_ready(session: &mut Session, label: &str, required: &[&str]) {
    let title = title();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_screen().is_some_and(|frame| {
            crate::support::overlay_body(frame, &title).is_some_and(|body| {
                required
                    .iter()
                    .all(|text| crate::support::controls::body_has(frame, &body, text))
            }) && crate::support::completed_navigation_controls_v1::owned_action_position(
                frame,
                &title,
                label,
                &[],
            )
            .is_some()
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "No completed owned FILE form {label} with required text {required:?}\n{}",
            session.screen()
        );
    }
}
pub(crate) fn form_ready(session: &mut Session, preview: bool) {
    let phase = if preview {
        "READ-ONLY PREVIEW • Confirm action"
    } else {
        "PR INTENT • Preview first"
    };
    form_action_ready(
        session,
        if preview {
            "[Confirm action]"
        } else {
            "[Preview]"
        },
        &[phase],
    );
}
fn resize_file_form_v82(session: &mut Session, cols: u16, rows: u16) {
    let after = session.completed_cursorless_end();
    session.resize(cols, rows);
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            frame.size() == (rows, cols)
                && crate::support::overlay_body(frame, &title()).is_some_and(|body| {
                    body.lines().last().is_some_and(|row| {
                        row.trim()
                            == "[Preview] [External editor] [Confirm action] [Keep draft / Close]"
                    })
                })
        },
        "Resized actual centered FILE Form owns its complete ordered footer",
        Duration::from_secs(8),
    );
}
fn sql_unrelated(f: &Fixture) -> Vec<String> {
    let db = raw(f);
    [
    "SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'file-comment-draft-v75/%' ORDER BY host,login,resource",
    "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
    "SELECT json_array(host,login,kind,id,payload) FROM records WHERE login='user-id:43' ORDER BY host,login,kind,id",
    "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"
].into_iter().flat_map(|q|db.prepare(q).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn held_child(
    session: &mut Session,
    fixture: &Fixture,
    ordinal: usize,
) -> crate::support::owned_pty_processes_v1::ProcessIdentity {
    session.wait_for_command_count(fixture, ordinal + 1);
    let pid = i32::try_from(commands(session, fixture)[ordinal]["pid"].as_u64().unwrap()).unwrap();
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    let child = process_identity(pid)
        .unwrap()
        .expect("Actual original FILE child is live before test effect/cancel");
    assert!(owner.owns(&child));
    assert!(!child.zombie);
    child
}
fn protected(f: &Fixture) -> Value {
    let db = raw(f);
    let text: String = db
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'file-comment-draft-v75/%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["version"], 75);
    assert_eq!(value["values"]["body"], BODY);
    assert!(value.get("approved").is_none());
    let row:(i64,String,String,String,String)=db.query_row("SELECT attempt_id,command,observed_head,payload_sha256,state FROM pr_write_attempts WHERE command='file-comment-create-v75'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap();
    assert_eq!(value["publication"]["attempt_id"], row.0);
    assert_eq!(value["publication"]["payload_sha256"], row.3);
    assert_eq!(row.2, "a".repeat(40));
    assert_eq!(row.1, "file-comment-create-v75");
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_file_comment_origins_v75 WHERE attempt_id=?1",
            [row.0],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        1
    );
    value
}
pub(crate) fn exact(session: &Session, f: &Fixture, steps: &[GhStep], writes: usize) {
    let actual = commands(session, f);
    assert_eq!(actual.len(), steps.len());
    let mut count = 0;
    for (row, step) in actual.iter().zip(steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        if step.args.iter().any(|v| v == "POST") && step.args[1] != "graphql" {
            count += 1;
            let v: Value = serde_json::from_slice(&step.stdin).unwrap();
            if step.args[1].ends_with("/comments/9181/replies") {
                assert_eq!(v.as_object().unwrap().len(), 1);
                assert_eq!(v["body"], "Original native reply 界\nsecond line");
            } else {
                assert_eq!(v["subject_type"], "file");
                assert_eq!(v.as_object().unwrap().len(), 4);
                for k in [
                    "line",
                    "side",
                    "start_line",
                    "start_side",
                    "position",
                    "in_reply_to",
                ] {
                    assert!(v.get(k).is_none());
                }
            }
        }
        assert!(!step
            .args
            .iter()
            .any(|v| matches!(v.as_str(), "PATCH" | "DELETE" | "PUT")));
    }
    assert_eq!(count, writes);
}
fn journey(mouse: bool, finish: Finish) {
    journey_inner(mouse, finish, None);
}
fn journey_inner(mouse: bool, finish: Finish, special: Option<Value>) {
    let capture_frames_v85 = finish == Finish::PreviewClose && special.is_none();
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(observation());
    steps.extend(observation());
    let write_start = steps.len();
    let (write, held) = if finish == Finish::PreviewClose {
        (Vec::new(), None)
    } else {
        confirm(finish)
    };
    steps.extend(write);
    let original_end = steps.len();
    if finish == Finish::CancelThenFreshSource {
        let mut reload = initial();
        for step in &mut reload {
            if step
                .args
                .get(1)
                .is_some_and(|s| s == &format!("repos/{FIRST}/pulls/7"))
            {
                let mut actual = parent_resource();
                actual["body"] = json!("NEW_CURRENT_SUMMARY_AFTER_EXPLICIT_CANCEL");
                step.stdout = serde_json::to_vec(&actual).unwrap();
            }
        }
        steps.extend(reload);
    }
    let restart = matches!(
        finish,
        Finish::CancelPre | Finish::CancelPost | Finish::CleanupFailed
    );
    if restart {
        steps.extend(initial());
        let mut list = history_phase();
        list[0].delay_ms = 6000;
        steps.extend(list);
        steps.extend(history_phase());
        steps.extend(history_phase());
    }
    if finish == Finish::HappyFollow {
        steps.extend(initial());
        steps.extend(resource_operation(false));
        steps.extend(publication_v1::file_follow_v75::steps());
    }
    if let Some(member) = special {
        for step in &mut steps {
            if step
                .args
                .get(1)
                .is_some_and(|endpoint| endpoint.contains("/pulls/7/files?"))
            {
                let value = if step.args.iter().any(|value| value == "--slurp") {
                    json!([[member.clone()]])
                } else {
                    json!([member.clone()])
                };
                step.stdout = serde_json::to_vec(&value).unwrap();
            }
            if step.args.first().is_some_and(|value| value == "pr")
                && step.args.get(1).is_some_and(|value| value == "diff")
            {
                step.stdout=match member["status"].as_str(){Some("renamed")=>format!("diff --git a/src/old.rs b/{PATH}\nsimilarity index 100%\nrename from src/old.rs\nrename to {PATH}\n"),Some("removed")=>format!("diff --git a/{PATH} b/{PATH}\ndeleted file mode 100644\n--- a/{PATH}\n+++ /dev/null\n@@ -1 +0,0 @@\n-old\n"),_=>format!("diff --git a/{PATH} b/{PATH}\nBinary files a/{PATH} and b/{PATH} differ\n")}.into_bytes();
            }
        }
    }
    let fixture = resource_configured(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files(&mut session, &fixture, mouse);
    let unrelated = sql_unrelated(&fixture);
    let trace = git_commands(&fixture);
    session.resize(120, 40);
    choose(&mut session, mouse, "[K FILE comment]", b"K");
    session.wait_for_command_count(&fixture, 52);
    form_ready(&mut session, false);
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", "Body");
    }
    session.paste(BODY);
    form_action_ready(
        &mut session,
        "[Preview]",
        &["PRIVATE_NATIVE_WHOLE_FILE_BODY", "second FILE line"],
    );
    if capture_frames_v85 {
        session.capture_completed_terminal_v1("file-comment-intent");
    }
    form_control(&mut session, mouse, "[Preview]");
    session.wait_for_command_count(&fixture, 63);
    form_ready(&mut session, true);
    let endpoint = session.completed_cursorless_end();
    session.paste("READ_ONLY_FILE_POISON");
    session.wait_for_new_cursorless_condition(
        endpoint,
        |f| {
            let s = f.contents();
            s.contains(&title())
                && s.contains("READ-ONLY PREVIEW")
                && !s.contains("READ_ONLY_FILE_POISON")
        },
        "Exact readonly FILE Preview ignores real paste",
        Duration::from_secs(8),
    );
    if capture_frames_v85 {
        session.capture_completed_terminal_v1("file-comment-preview");
    }
    assert_eq!(commands(&session, &fixture).len(), write_start);
    if finish == Finish::PreviewClose {
        if !mouse {
            for (key, expected) in [
                (b"\t".as_slice(), "[External editor]"),
                (b"\t".as_slice(), "[Confirm action]"),
                (b"\t".as_slice(), "[Keep draft / Close]"),
                (b"\t".as_slice(), "[Preview]"),
                (b"\x1b[Z".as_slice(), "[Keep draft / Close]"),
                (b"\x1b[Z".as_slice(), "[Confirm action]"),
                (b"\x1b[Z".as_slice(), "[External editor]"),
                (b"\x1b[Z".as_slice(), "[Preview]"),
            ] {
                let (x, y) = session.wait_for_owned_completed_action(&title(), expected, &[]);
                let after = session.completed_cursorless_end();
                session.send_key(key);
                session.wait_for_new_cursorless_condition(
                    after,
                    |frame| {
                        frame.contents().contains(&title())
                            && frame.contents().contains("READ-ONLY PREVIEW")
                            && frame.cell(y, x).is_some_and(|cell| cell.inverse())
                    },
                    "Actual FILE Preview keyboard cycle focuses each visible footer, never hidden Body",
                    Duration::from_secs(8),
                );
            }
        }
        resize_file_form_v82(&mut session, 80, 24);
        form_ready(&mut session, true);
        let after = session.completed_cursorless_end();
        session.resize(40, 10);
        session.wait_for_new_cursorless_condition(
            after,
            |frame| {
                frame.size() == (10, 40)
                    && crate::support::overlay_body(frame, "Preview • resize required")
                        .is_some_and(|body| {
                            let rows: Vec<_> = body.lines().map(str::trim).collect();
                            rows.ends_with(&[
                                "[Ctrl+P Edit fields]",
                                "[Ctrl+Enter Confirm unavailable]",
                                "[Esc Close]",
                            ])
                        })
                    && !frame.contents().contains("[Confirm action]")
                    && !frame.contents().contains("PRIVATE_NATIVE_WHOLE_FILE_BODY")
            },
            "Actual tiny centered FILE Preview has all three accessible controls and no hidden confirmation/body",
            Duration::from_secs(8),
        );
        let after = session.completed_cursorless_end();
        if mouse {
            let point = session.wait_for_owned_completed_action(
                "Preview • resize required",
                "[Ctrl+Enter Confirm unavailable]",
                &[],
            );
            session.click(point.0, point.1);
        } else {
            session.send_key(b"\x1b[13;5u");
        }
        session.wait_for_new_cursorless_condition(
            after,
            |frame| {
                frame.contents().contains("Confirm refused")
                    && frame.contents().contains("[Esc Close]")
                    && !frame.contents().contains("[Confirm action]")
            },
            "Actual tiny FILE Confirm key/click is visibly refused without any new request/publication",
            Duration::from_secs(8),
        );
        if capture_frames_v85 {
            session.capture_completed_terminal_v1("file-comment-tiny-confirm-refused");
        }
        assert_eq!(commands(&session, &fixture).len(), write_start);
        assert_eq!(
            raw(&fixture)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            raw(&fixture)
                .query_row(
                    "SELECT count(*) FROM pr_write_file_comment_origins_v75",
                    [],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
            0
        );
        resize_file_form_v82(&mut session, 120, 40);
        form_ready(&mut session, true);
        form_control(&mut session, mouse, "[Keep draft / Close]");
        session.wait_for_cursorless_condition(
            |f| {
                let s = f.contents();
                crate::support::controls::control_painted(f, "[K FILE comment]")
                    && s.contains("> File src/界.rs")
                    && !s.contains(&title())
                    && !s.contains(BODY)
            },
            "Pure Back exact Files, private Form gone without transport",
            Duration::from_secs(8),
        );
        assert_eq!(
            raw(&fixture)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
    } else {
        let before = session.completed_cursorless_end();
        form_control(&mut session, mouse, "[Confirm action]");
        let mut witness = None;
        if let Some(held) = held {
            let child = held_child(&mut session, &fixture, write_start + held);
            let saved = protected(&fixture);
            assert!(process_identity(child.pid)
                .unwrap()
                .is_some_and(|p| p.birth == child.birth
                    && p.uid == child.uid
                    && p.session == child.session
                    && !p.zombie));
            if finish == Finish::CleanupFailed {
                rusqlite::Connection::open(database(&fixture)).unwrap().execute_batch("CREATE TRIGGER file_cleanup_real_failure_v75 BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'file-comment-draft-v75/%' BEGIN SELECT RAISE(ABORT,'actual protected FILE cleanup failure'); END;").unwrap();
            }
            if matches!(finish, Finish::ReplacePre | Finish::ReplacePost) {
                let mut replacement = saved.clone();
                replacement["values"]["body"] = json!("EXTERNAL_NEWER_FILE_DRAFT_MUST_SURVIVE");
                let key:String=raw(&fixture).query_row("SELECT resource FROM drafts WHERE resource LIKE 'file-comment-draft-v75/%'",[],|r|r.get(0)).unwrap();
                Cache::open(&database(&fixture))
                    .unwrap()
                    .put_draft(
                        &Scope::github(HOST, Some(42)).unwrap(),
                        &key,
                        &replacement.to_string(),
                    )
                    .unwrap();
                witness = Some(replacement);
            }
            if matches!(
                finish,
                Finish::CancelPre | Finish::CancelThenFreshSource | Finish::CancelPost
            ) {
                choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
                wait_original_child_gone(&mut session, &child);
            }
        }
        session.wait_for_command_count(&fixture, original_end);
        session.wait_for_new_cursorless_condition(
            before,
            |f| {
                let s = f.contents();
                !s.contains(&title())
                    && !s.contains("PRIVATE_NATIVE_WHOLE_FILE_BODY")
                    && (s.contains("refresh explicitly")
                        || s.contains("reload explicitly")
                        || s.contains("canceled")
                        || s.contains("not started"))
            },
            "Current owned FILE result/cancel retires original private bodies",
            Duration::from_secs(8),
        );
        let db = raw(&fixture);
        let state: Option<String> = db
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE command='file-comment-create-v75'",
                [],
                |r| r.get(0),
            )
            .optional()
            .unwrap();
        if matches!(
            finish,
            Finish::StaleMember
                | Finish::ChangedHead
                | Finish::RecreatedRepository
                | Finish::AccountLoss
        ) {
            assert_eq!(state, None);
            let encoded: String = db
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'file-comment-draft-v75/%'",
                    [],
                    |row| row.get(0),
                )
                .unwrap();
            let draft: Value = serde_json::from_str(&encoded).unwrap();
            assert_eq!(draft["values"]["body"], BODY);
            assert!(draft["publication"].is_null());
            assert_eq!(
                db.query_row(
                    "SELECT count(*) FROM pr_write_file_comment_origins_v75",
                    [],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
                0
            );
        } else if matches!(
            finish,
            Finish::Happy | Finish::HappyFollow | Finish::CleanupFailed | Finish::ReplacePost
        ) {
            assert_eq!(state.as_deref(), Some("confirmed"));
            let effect:String=db.query_row("SELECT confirmed_json FROM pr_write_attempts WHERE command='file-comment-create-v75'",[],|r|r.get(0)).unwrap();
            let effect: Value = serde_json::from_str(&effect).unwrap();
            assert_eq!(effect["resource_id"], "9181");
            assert_eq!(effect["node_id"], "NATIVE_FILE_COMMENT_9181");
        } else {
            assert!(matches!(state.as_deref(), Some("pending" | "uncertain")));
        }
        if let Some(witness) = witness {
            let text: String = db
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'file-comment-draft-v75/%'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(serde_json::from_str::<Value>(&text).unwrap(), witness);
        }
        if matches!(
            finish,
            Finish::CleanupFailed
                | Finish::CancelPre
                | Finish::CancelThenFreshSource
                | Finish::CancelPost
                | Finish::Malformed
        ) {
            let value = protected(&fixture);
            assert_eq!(value["values"]["body"], BODY);
        }
    }
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(sql_unrelated(&fixture), unrelated);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    let writes = usize::from(!matches!(
        finish,
        Finish::PreviewClose
            | Finish::StaleMember
            | Finish::ChangedHead
            | Finish::RecreatedRepository
            | Finish::AccountLoss
            | Finish::CancelPre
            | Finish::CancelThenFreshSource
            | Finish::ReplacePre
    ));
    exact(&session, &fixture, &steps[..original_end], writes);
    assert_read_only_git(&fixture);
    if restart {
        inspect_restart(
            &mut session,
            &fixture,
            mouse,
            original_end,
            finish == Finish::CleanupFailed,
        );
        assert_eq!(sql_unrelated(&fixture), unrelated);
        assert_eq!(git_state(&fixture), git);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        let rows = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        let rows: Vec<Value> = rows
            .lines()
            .map(|v| serde_json::from_str(v).unwrap())
            .collect();
        assert_eq!(rows.len(), steps.len());
        for (row, step) in rows.iter().zip(&steps) {
            assert_eq!(row["matched"], true);
            assert_eq!(row["args"], json!(step.args));
            assert_eq!(
                row["stdin_hex"],
                step.stdin
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<String>()
            );
        }
    } else if finish == Finish::CancelThenFreshSource {
        let protected_before = protected_sql(&fixture);
        let source_rows = |fixture: &Fixture| {
            let db = raw(fixture);
            let rows=db.prepare("SELECT json_array(source,generation,active) FROM sync_reservations WHERE host=?1 AND login='user-id:42' AND source LIKE 'repository-catalogue-source-v1:%' ORDER BY source").unwrap().query_map([HOST],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
            assert!(!rows.is_empty());
            rows
        };
        let old_source_rows = source_rows(&fixture);
        fresh_summary(&mut session, &fixture, mouse, original_end);
        session.wait_for_cursorless_condition(|frame|{let s=frame.contents();s.contains("NEW_CURRENT_SUMMARY_AFTER_EXPLICIT_CANCEL")&&crate::support::controls::control_painted(frame, "[J Thread history]")&&!s.contains("PRIVATE_NATIVE_WHOLE_FILE_BODY")&&!s.contains(&title())},"Explicit user reload admits a genuinely fresh source/private Summary, never reuses old canceled Form",Duration::from_secs(8));
        assert_ne!(
            source_rows(&fixture),
            old_source_rows,
            "Fresh source must have a real new original reservation/generation"
        );
        assert_eq!(protected_sql(&fixture), protected_before);
        let at = session.completed_cursorless_end();
        session.resize(80, 24);
        session.wait_for_control("[J Thread history]");
        session.resize(120, 40);
        session.wait_for_new_cursorless_condition(
            at,
            |frame| {
                let s = frame.contents();
                s.contains("NEW_CURRENT_SUMMARY_AFTER_EXPLICIT_CANCEL")
                    && !s.contains("PRIVATE_NATIVE_WHOLE_FILE_BODY")
                    && !s.contains(&title())
            },
            "Fresh source survives genuine resize after canceled original child is gone",
            Duration::from_secs(8),
        );
        assert_eq!(protected_sql(&fixture), protected_before);
        assert_eq!(git_state(&fixture), git);
        assert_eq!(git_commands(&fixture), trace);
        assert_eq!(sql_unrelated(&fixture), unrelated);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        exact(&session, &fixture, &steps, 0);
        session.quit_and_assert_restored_terminal(mouse);
    } else if finish == Finish::HappyFollow {
        fresh_summary(&mut session, &fixture, mouse, original_end);
        choose(&mut session, mouse, "[f Files]", b"f");
        session.wait_for_command_count(&fixture, original_end + 41);
        ready(&mut session, &[PATH]);
        session.wait_for_control("[T Threads]");
        choose(&mut session, mouse, "[N Next file]", b"N");
        ready(&mut session, &["> File src/界.rs"]);
        publication_v1::file_follow_v75::follow(&mut session, &fixture, mouse, original_end + 41);
        assert_eq!(git_state(&fixture), git);
        assert_eq!(git_commands(&fixture), trace);
        assert_eq!(sql_unrelated(&fixture), unrelated);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        exact(&session, &fixture, &steps, 2);
        session.quit_and_assert_restored_terminal(mouse);
    } else {
        session.quit_and_assert_restored_terminal(mouse);
    }
}
macro_rules! pair {
    ($key:ident,$mouse:ident,$finish:ident) => {
        #[test]
        fn $key() {
            journey(false, Finish::$finish)
        }
        #[test]
        fn $mouse() {
            journey(true, Finish::$finish)
        }
    };
}
pair!(
    file_comments_v75_keyboard_exact_201_get_file_node_and_safe_retirement,
    file_comments_v75_mouse_exact_201_get_file_node_and_safe_retirement,
    Happy
);
pair!(
    file_comments_v75_keyboard_create_then_explicit_fresh_file_thread_and_actual_root_reply,
    file_comments_v75_mouse_create_then_explicit_fresh_file_thread_and_actual_root_reply,
    HappyFollow
);
pair!(
    file_comments_v75_keyboard_readonly_preview_resize_cancel_pure_files_back,
    file_comments_v75_mouse_readonly_preview_resize_cancel_pure_files_back,
    PreviewClose
);
pair!(
    file_comments_v75_keyboard_fresh_member_change_refuses_before_post,
    file_comments_v75_mouse_fresh_member_change_refuses_before_post,
    StaleMember
);
pair!(
    file_comments_v75_keyboard_actual_fresh_head_change_refuses_before_sql_attempt_or_post,
    file_comments_v75_mouse_actual_fresh_head_change_refuses_before_sql_attempt_or_post,
    ChangedHead
);
pair!(
    file_comments_v75_keyboard_actual_recreated_numeric_repository_refuses_same_locator_no_post,
    file_comments_v75_mouse_actual_recreated_numeric_repository_refuses_same_locator_no_post,
    RecreatedRepository
);
pair!(
    file_comments_v75_keyboard_actual_current_numeric_account_loss_hides_body_before_private_reads,
    file_comments_v75_mouse_actual_current_numeric_account_loss_hides_body_before_private_reads,
    AccountLoss
);
pair!(
    file_comments_v75_keyboard_actual_cancel_then_explicit_fresh_source_preserves_new_summary,
    file_comments_v75_mouse_actual_cancel_then_explicit_fresh_source_preserves_new_summary,
    CancelThenFreshSource
);
pair!(
    file_comments_v75_keyboard_actual_held_prepost_sql_ack_pending_cancel,
    file_comments_v75_mouse_actual_held_prepost_sql_ack_pending_cancel,
    CancelPre
);
pair!(
    file_comments_v75_keyboard_actual_held_postinvoke_readback_cancel_uncertain,
    file_comments_v75_mouse_actual_held_postinvoke_readback_cancel_uncertain,
    CancelPost
);
pair!(
    file_comments_v75_keyboard_actual_confirmed_cleanup_sql_failure_preserves_body,
    file_comments_v75_mouse_actual_confirmed_cleanup_sql_failure_preserves_body,
    CleanupFailed
);
pair!(
    file_comments_v75_keyboard_actual_external_original_db_replacement_beforepost,
    file_comments_v75_mouse_actual_external_original_db_replacement_beforepost,
    ReplacePre
);
pair!(
    file_comments_v75_keyboard_actual_external_original_db_replacement_beforecleanup,
    file_comments_v75_mouse_actual_external_original_db_replacement_beforecleanup,
    ReplacePost
);
pair!(
    file_comments_v75_keyboard_malformed_numeric_author_receipt_uncertain_no_resend,
    file_comments_v75_mouse_malformed_numeric_author_receipt_uncertain_no_resend,
    Malformed
);
#[test]
fn file_comments_v75_keyboard_renamed_removed_binary_large_absent_patch_members_have_no_fabricated_anchor(
) {
    special_members(false);
}
#[test]
fn file_comments_v75_mouse_renamed_removed_binary_large_absent_patch_members_have_no_fabricated_anchor(
) {
    special_members(true);
}
fn special_members(mouse: bool) {
    for member in [
        json!({"filename":PATH,"previous_filename":"src/old.rs","status":"renamed","additions":0,"deletions":0}),
        json!({"filename":PATH,"status":"removed","additions":0,"deletions":1}),
        json!({"filename":PATH,"status":"modified","additions":0,"deletions":0,"patch":null}),
        json!({"filename":PATH,"status":"modified","additions":100000,"deletions":100000,"patch":null}),
    ] {
        journey_inner(mouse, Finish::Happy, Some(member));
    }
}
