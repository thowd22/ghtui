//! Descendant of the shared native PR journey module; uses actual screen-driven
//! menu/form helpers, strict fake-gh and independent storage/Git observations.
use super::*;

const VIEW_QUERY:&str="query($id:ID!,$cursor:String){node(id:$id){... on PullRequest{files(first:100,after:$cursor){nodes{path viewerViewedState} pageInfo{hasNextPage endCursor}}}}}";
const FILE: &str = "src/α file.rs";
fn target(head: &str) -> Value {
    let mut value = pr();
    value["head"]["sha"] = json!(head);
    value["changed_files"] = json!(2);
    value
}
fn replace_targets(value: &mut Value, head: &str) {
    match value {
        Value::Object(map)
            if map
                .get("node_id")
                .is_some_and(|id| id == "PR_example/repo_7") =>
        {
            *value = target(head)
        }
        Value::Object(map) => {
            for value in map.values_mut() {
                replace_targets(value, head)
            }
        }
        Value::Array(values) => {
            for value in values {
                replace_targets(value, head)
            }
        }
        _ => {}
    }
}
fn at_head(steps: Vec<GhStep>, head: &str) -> Vec<GhStep> {
    steps.into_iter().map(|mut step|{if let Ok(mut value)=serde_json::from_slice::<Value>(&step.stdout){replace_targets(&mut value,head);if step.args.get(1).is_some_and(|endpoint|endpoint=="repos/example/repo/pulls/7/files?per_page=100"){value=json!([[{"filename":FILE,"status":"modified","additions":1,"deletions":0,"patch":"@@ -1 +1 @@\n-before\n+after"},{"filename":"other.rs","status":"modified","additions":1,"deletions":0,"patch":"@@ -1 +1 @@\n-old\n+new"}]]);}step.stdout=serde_json::to_vec(&value).unwrap();}step}).collect()
}
fn viewed(state: &str) -> GhStep {
    write(
        "graphql",
        "POST",
        Some(json!({"query":VIEW_QUERY,"variables":{"id":"PR_example/repo_7","cursor":null}})),
        json!({"data":{"node":{"files":{"nodes":[{"path":FILE,"viewerViewedState":state},{"path":"other.rs","viewerViewedState":"UNVIEWED"}],"pageInfo":{"hasNextPage":false,"endCursor":null}}}}}),
    )
}
fn progress_read(head: &str, state: &str) -> Vec<GhStep> {
    let mut steps = identity();
    steps.push(step(
        "repos/example/repo/pulls/7",
        "GET",
        target(head),
        false,
    ));
    steps.push(viewed(state));
    steps.push(step(
        "repos/example/repo/pulls/7",
        "GET",
        target(head),
        false,
    ));
    steps.extend(identity());
    steps
}
fn preview_steps(head: &str, state: &str) -> Vec<GhStep> {
    let mut steps = at_head(guard(), head);
    steps.extend(progress_read(head, state));
    steps
}
fn publication(head: &str, before: &str, mark: bool) -> Vec<GhStep> {
    let mut steps = at_head(guard(), head);
    steps.extend(progress_read(head, before));
    steps.extend(at_head(guard(), head));
    let (name, input) = if mark {
        ("markFileAsViewed", "MarkFileAsViewedInput")
    } else {
        ("unmarkFileAsViewed", "UnmarkFileAsViewedInput")
    };
    steps.push(write("graphql","POST",Some(json!({"query":format!("mutation($input:{input}!){{{name}(input:$input){{pullRequest{{id}}}}}}"),"variables":{"input":{"pullRequestId":"PR_example/repo_7","path":FILE}}})),json!({"data":{name:{"pullRequest":{"id":"PR_example/repo_7"}}}})));
    steps.push(viewed(if mark { "VIEWED" } else { "UNVIEWED" }));
    steps.extend(at_head(inspected(vec![starter()]), head));
    steps
}
fn select_file(session: &mut Session, mouse: bool, state: &str) {
    let label = format!("{state} • {FILE}");
    session.select_completed_progress_file(mouse, &label);
}
fn original_receipts(fixture: &Fixture) -> Vec<(String, String, Value, Value)> {
    Connection::open(cache_path(fixture)).unwrap().prepare("SELECT command,state,confirmed_json,observed_head FROM pr_write_attempts ORDER BY attempt_id").unwrap().query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,serde_json::from_str::<Value>(&row.get::<_,String>(2)?).unwrap(),json!(row.get::<_,String>(3)?)))).unwrap().collect::<Result<Vec<_>,_>>().unwrap()
}

#[test]
fn viewed_files_keyboard_marks_unmarks_restarts_and_keeps_changed_head_progress_honest() {
    journey(false);
}
#[test]
fn viewed_files_mouse_marks_unmarks_restarts_and_keeps_changed_head_progress_honest() {
    journey(true);
}
fn journey(mouse: bool) {
    journey_with_preselected(mouse, false);
}
#[test]
fn completed_frame_actions_keyboard_already_selected_file_does_not_step_past_target() {
    journey_with_preselected(false, true);
}
#[test]
fn completed_frame_actions_mouse_already_selected_file_keeps_exact_publication_target() {
    journey_with_preselected(true, true);
}
fn journey_with_preselected(mouse: bool, preselected: bool) {
    let fixture = configured();
    fixture.init_git();
    fixture.commit_file(
        "baseline.txt",
        b"saved baseline\n",
        "Untouched local baseline",
    );
    fs::write(
        fixture.repo().join("baseline.txt"),
        b"local dirty baseline\n",
    )
    .unwrap();
    let original_head = fixture.git(&["rev-parse", "HEAD"]);
    let original_index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let original_dirty = fs::read(fixture.repo().join("unrelated.txt")).unwrap();
    let original_baseline = fs::read(fixture.repo().join("baseline.txt")).unwrap();
    let first = "a".repeat(40);
    let changed = "c".repeat(40);
    let mut steps = at_head(initial(), &first);
    steps.extend(at_head(details(vec![starter()]), &first));
    steps.extend(progress_read(&first, "UNVIEWED"));
    let first_progress = steps.len();
    steps.extend(progress_read(&first, "UNVIEWED"));
    steps.extend(preview_steps(&first, "UNVIEWED"));
    steps.extend(publication(&first, "UNVIEWED", true));
    let marked_count = steps.len();
    steps.extend(at_head(initial(), &changed));
    steps.extend(at_head(details(vec![starter()]), &changed));
    steps.extend(progress_read(&changed, "DISMISSED"));
    let resumed_count = steps.len();
    steps.extend(progress_read(&changed, "DISMISSED"));
    steps.extend(preview_steps(&changed, "DISMISSED"));
    steps.extend(publication(&changed, "DISMISSED", false));
    let final_count = steps.len();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    session.send_key(b"1");
    session.assert_focused_view("Inbox");
    open(&mut session, mouse);
    choose(&mut session, mouse, "[v Review progress]", b"v");
    session.wait_for_screen("Review progress: 0/2 viewed");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, first_progress)),
        0
    );
    session.resize(80, 24);
    if preselected {
        select_file(&mut session, mouse, "UNVIEWED");
        session.wait_for_screen(&format!("> UNVIEWED • {FILE}"));
    }
    select_file(&mut session, mouse, "UNVIEWED");
    menu(&mut session, mouse, "Mark selected file viewed");
    session.wait_for_screen("PR INTENT • Preview first");
    session.wait_for_screen("Local PR intent ready");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, first_progress + 7)),
        0
    );
    preview(&mut session, mouse);
    session.wait_for_screen("Exact canonical file: src/α file.rs");
    session.wait_for_screen("Requested server state: VIEWED");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, first_progress + 17)),
        0
    );
    confirm(&mut session, mouse);
    session.wait_for_screen("Published • Mark selected file viewed");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, marked_count)),
        1
    );
    let receipts = original_receipts(&fixture);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].0, "mark-viewed");
    assert_eq!(receipts[0].1, "confirmed");
    assert_eq!(receipts[0].2["kind"], "viewed_file");
    assert_eq!(receipts[0].3, first);
    assert!(receipts[0].2["resource_id"]
        .as_str()
        .unwrap()
        .ends_with("/VIEWED"));
    session.wait_for_overlay_close("╭ Mark selected file viewed ");
    session.wait_for_screen("Review progress: 1/2 viewed");
    assert!(session.screen().contains("@octocat"));
    session.quit_and_assert_restored_terminal(mouse);
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let saved = Cache::open(&cache_path(&fixture))
        .unwrap()
        .saved_view(&scope, "pr-review-position:v1:PR_example/repo_7")
        .unwrap()
        .unwrap();
    assert_eq!(saved["path"], FILE);
    assert!(saved.get("viewed").is_none());
    assert!(saved.get("state").is_none());
    let mut session = Session::launch(&fixture, 80, 24);
    session.wait_for_screen("ghtui");
    session.send_key(b"1");
    session.assert_focused_view("Inbox");
    open(&mut session, mouse);
    choose(&mut session, mouse, "[v Review progress]", b"v");
    session.wait_for_screen("CHANGED SINCE VIEWED • src/α file.rs");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, resumed_count)),
        1
    );
    assert!(session.screen().contains("Review progress: 0/2 viewed"));
    session.resize(120, 40);
    menu(&mut session, mouse, "Mark selected file unviewed");
    session.wait_for_screen("PR INTENT • Preview first");
    session.wait_for_screen("Local PR intent ready");
    assert_eq!(
        mutation_count(&records(&mut session, &fixture, resumed_count + 7)),
        1
    );
    preview(&mut session, mouse);
    session.wait_for_screen("Current server state: CHANGED SINCE VIEWED");
    session.wait_for_screen("Requested server state: UNVIEWED");
    confirm(&mut session, mouse);
    session.wait_for_screen("Published • Mark selected file unviewed");
    let journal = records(&mut session, &fixture, final_count);
    assert_eq!(mutation_count(&journal), 2);
    let receipts = original_receipts(&fixture);
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0].3, first);
    assert_eq!(receipts[1].0, "unmark-viewed");
    assert_eq!(receipts[1].1, "confirmed");
    assert_eq!(receipts[1].3, changed);
    assert!(receipts[1].2["resource_id"]
        .as_str()
        .unwrap()
        .ends_with("/UNVIEWED"));
    session.wait_for_overlay_close("╭ Mark selected file unviewed ");
    session.wait_for_screen("UNVIEWED • src/α file.rs");
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), original_head);
    assert_eq!(
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        original_index
    );
    assert_eq!(
        fs::read(fixture.repo().join("unrelated.txt")).unwrap(),
        original_dirty
    );
    assert_eq!(
        fs::read(fixture.repo().join("baseline.txt")).unwrap(),
        original_baseline
    );
}
