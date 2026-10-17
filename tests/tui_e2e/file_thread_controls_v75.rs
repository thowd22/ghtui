//! DECLARED / UNRUN. Actually create FILE first, then explicitly reload and
//! observe the server's opaque FILE thread before any existing thread action.
use super::super::file_comments_v75 as file;
use super::*;
const FILE_BODY_V75: &str = "PRIVATE_NATIVE_WHOLE_FILE_BODY 界\nsecond FILE line";
pub(super) fn adapted(steps: Vec<GhStep>) -> Vec<GhStep> {
    let mut steps = steps;
    super::file_follow_v75::adapt(&mut steps);
    steps
}
fn original_file_sql(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    ["SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts WHERE command='file-comment-create-v75' ORDER BY attempt_id","SELECT json_array(attempt_id,repository_id,repository_node,pr_id,origin_json) FROM pr_write_file_comment_origins_v75 ORDER BY attempt_id"].into_iter().flat_map(|q|db.prepare(q).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn other_sql(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    ["SELECT json_array(host,login,resource,body) FROM drafts WHERE login='user-id:43' ORDER BY host,login,resource","SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name","SELECT json_array(host,login,kind,id,payload) FROM records WHERE login='user-id:43' ORDER BY host,login,kind,id","SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"].into_iter().flat_map(|q|db.prepare(q).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
pub(super) fn with_created_file(
    mouse: bool,
    extra: Vec<GhStep>,
    act: impl FnOnce(&mut Session, &Fixture, usize),
) {
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(file::observation());
    steps.extend(file::observation());
    steps.extend(file::happy_confirmation_v75());
    let created = steps.len();
    assert_eq!(created, 81);
    steps.extend(initial());
    steps.extend(resource_operation(false));
    let base = steps.len();
    assert_eq!(base, 122);
    steps.extend(extra);
    let fixture = resource_configured(&steps);
    Cache::open(&database(&fixture))
        .unwrap()
        .put_draft(
            &Scope::github(HOST, Some(43)).unwrap(),
            "foreign-actual-file-control-v75",
            FOREIGN,
        )
        .unwrap();
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files(&mut session, &fixture, mouse);
    session.resize(120, 40);
    let trace = git_commands(&fixture);
    let other = other_sql(&fixture);
    choose(&mut session, mouse, "[K FILE comment]", b"K");
    session.wait_for_command_count(&fixture, 52);
    file::form_ready(&mut session, false);
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", "Body");
    }
    session.paste(FILE_BODY_V75);
    file::form_control(&mut session, mouse, "[Preview]");
    session.wait_for_command_count(&fixture, 63);
    file::form_ready(&mut session, true);
    let at = session.completed_cursorless_end();
    file::form_control(&mut session, mouse, "[Confirm action]");
    session.wait_for_command_count(&fixture, created);
    session.wait_for_new_cursorless_condition(
        at,
        |frame| {
            let s = frame.contents();
            s.contains("FILE comment posted #9181")
                && s.contains("refresh explicitly")
                && !s.contains(FILE_BODY_V75)
                && !s.contains(&file::title())
        },
        "Original actual FILE comment201/GET/FILE-node confirmed before any follow action",
        Duration::from_secs(8),
    );
    let file_witness = original_file_sql(&fixture);
    assert_eq!(file_witness.len(), 2);
    file::fresh_summary(&mut session, &fixture, mouse, created);
    choose(&mut session, mouse, "[f Files]", b"f");
    session.wait_for_command_count(&fixture, base);
    ready(&mut session, &["File src/界.rs", "[T Threads]"]);
    choose(&mut session, mouse, "[N Next file]", b"N");
    ready(&mut session, &["> File src/界.rs"]);
    act(&mut session, &fixture, base);
    session.wait_for_command_count(&fixture, steps.len());
    let rows = commands(&session, &fixture);
    assert_eq!(rows.len(), steps.len());
    let mut writes = 0;
    for (ordinal, (row, step)) in rows.iter().zip(&steps).enumerate() {
        assert_eq!(row["step"], ordinal);
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        if step
            .args
            .iter()
            .any(|a| matches!(a.as_str(), "POST" | "PATCH" | "DELETE" | "PUT"))
        {
            if step.args.get(1).is_some_and(|e| e == "graphql") {
                let input: Value = serde_json::from_slice(&step.stdin).unwrap();
                let query = input["query"].as_str().unwrap();
                if query.starts_with("mutation(") {
                    writes += 1;
                    assert_eq!(input["variables"]["input"]["threadId"], "NATIVE_THREAD_9");
                } else {
                    assert!(query.starts_with("query("));
                }
            } else {
                writes += 1;
            }
        }
        assert!(!step.args.iter().any(|a| a == "PUT"));
    }
    assert_eq!(
        writes, 2,
        "Exactly original FILE creation plus one genuine follow mutation, no replay"
    );
    assert_eq!(original_file_sql(&fixture), file_witness);
    assert_eq!(
        raw(&fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        2
    );
    assert_eq!(other_sql(&fixture), other);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
}
fn journey(mouse: bool, operation: Operation) {
    let mut extra = browse(operation);
    extra.extend(observation(operation));
    extra.extend(observation(operation));
    extra.extend(confirm(operation));
    let extra = adapted(extra);
    assert_eq!(extra.len(), 59);
    with_created_file(mouse, extra, |session, fixture, base| {
        choose(session, mouse, "[T Threads]", b"T");
        session.wait_for_command_count(fixture, base + 17);
        owned(
            session,
            &["FILE", "actual-file-author", operation.control()],
        );
        for (cols, rows) in [(80, 24), (120, 40)] {
            session.resize_owned_header(
                cols,
                rows,
                THREAD_TITLE,
                &[operation.control(), "[Esc Back]"],
            );
        }
        choose(session, mouse, operation.control(), operation.key());
        session.wait_for_command_count(fixture, base + 28);
        form_ready(session, operation, false);
        form_control(session, mouse, operation, "[Preview]");
        session.wait_for_command_count(fixture, base + 39);
        form_ready(session, operation, true);
        let at = session.completed_cursorless_end();
        form_control(session, mouse, operation, "[Confirm action]");
        session.wait_for_command_count(fixture, base + 59);
        session.wait_for_new_cursorless_condition(
            at,
            |frame| {
                let s = frame.contents();
                s.contains("refresh explicitly")
                    && !s.contains(FILE_BODY_V75)
                    && !s.contains(THREAD_TITLE)
                    && !s.contains(&operation.title())
            },
            "Actual FILE thread state observed; original source/private models retired",
            Duration::from_secs(8),
        );
        let(state,effect,origin):(String,String,String)=raw(fixture).query_row("SELECT a.state,a.confirmed_json,o.origin_json FROM pr_write_attempts a JOIN pr_write_thread_origins_v1 o USING(attempt_id) WHERE a.command=?1",[operation.command()],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(state, "confirmed");
        let effect: Value = serde_json::from_str(&effect).unwrap();
        assert_eq!(effect["kind"], "thread");
        assert_eq!(
            effect["resource_id"],
            if operation == Operation::Resolve {
                "resolved"
            } else {
                "unresolved"
            }
        );
        assert_eq!(effect["node_id"], "NATIVE_THREAD_9");
        let origin: Value = serde_json::from_str(&origin).unwrap();
        assert_eq!(origin["seed"]["thread_node"], "NATIVE_THREAD_9");
        assert!(
            origin["seed"]["root"].is_null(),
            "State changes bind opaque thread, not invented REST-root authority"
        );
        assert_eq!(origin["seed"]["head"], "a".repeat(40));
        assert_eq!(origin["seed"]["base"], "b".repeat(40));
    });
}
#[test]
fn file_comments_v75_keyboard_actual_created_file_thread_resolve_same_original_root() {
    journey(false, Operation::Resolve)
}
#[test]
fn file_comments_v75_mouse_actual_created_file_thread_resolve_same_original_root() {
    journey(true, Operation::Resolve)
}
#[test]
fn file_comments_v75_keyboard_actual_created_file_thread_reopen_observed_resolver_not_causality() {
    journey(false, Operation::Reopen)
}
#[test]
fn file_comments_v75_mouse_actual_created_file_thread_reopen_observed_resolver_not_causality() {
    journey(true, Operation::Reopen)
}
