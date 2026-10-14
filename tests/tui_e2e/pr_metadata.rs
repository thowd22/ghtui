//! Registered native metadata journeys under active verification.
//! TASK-18 records actual results. The happy journeys require the Metadata
//! view-rebuild repair and a genuine four-read context refresh after publication.
use super::*;
use ghtui::pr_write_journal::PrWriteIntent;
use rusqlite::params;
use std::collections::BTreeMap;

#[path = "pr_metadata_failures.rs"]
mod failures;

#[path = "pr_metadata_base.rs"]
mod base;
#[path = "pr_metadata_actions.rs"]
mod metadata_actions;
#[path = "pr_metadata_preview.rs"]
mod preview;
#[path = "pr_metadata_viewport_v9.rs"]
mod viewport_v9;

const NODE: &str = "PR_example/repo_7";
const REPOSITORY: &str = "example/repo";
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const TITLE: &str = "Metadata title café 雪";
const BODY: &str = "Description edited in native TUI\nCloses #12; preserve reviewers and labels.";
const BASE: &str = "release/october";

fn repository() -> Value {
    json!({"id":410,"node_id":"R_metadata_410","full_name":REPOSITORY,"private":true,"archived":false,"default_branch":"main","permissions":{"pull":true,"push":true,"maintain":false,"admin":false}})
}
fn target(title: &str, body: &str, base: &str) -> Value {
    let mut value = pr();
    value["id"] = json!(1007);
    value["merged"] = json!(false);
    value["labels"] = issue()["labels"].clone();
    value["assignees"] = issue()["assignees"].clone();
    value["milestone"] = issue()["milestone"].clone();
    value["title"] = json!(title);
    value["body"] = json!(body);
    value["head"]["repo"] = repository();
    value["base"]["repo"] = repository();
    value["base"]["ref"] = json!(base);
    if base == BASE {
        value["base"]["sha"] = json!("c".repeat(40));
    }
    value["requested_reviewers"] = json!([{"id":43,"login":"reviewer","type":"User"}]);
    value["requested_teams"] = json!([{"id":55,"name":"Maintainers","slug":"maintainers"}]);
    value["maintainer_can_modify"] = json!(true);
    value
}
fn issue() -> Value {
    json!({"id":2007,"node_id":"I_metadata_7","number":7,"locked":false,"user":{"id":42,"login":"octocat","type":"User"},"html_url":"https://github.invalid/example/repo/pull/7","pull_request":{"url":"https://github.invalid/api/v3/repos/example/repo/pulls/7","html_url":"https://github.invalid/example/repo/pull/7"},"labels":[{"id":81,"name":"bug"},{"id":82,"name":"needs-triage"}],"assignees":[{"id":77,"login":"triager","type":"User"}],"milestone":{"number":5,"title":"October release"}})
}
fn substitute(value: &mut Value, current: &Value) {
    match value {
        Value::Object(map) if map.get("node_id").is_some_and(|id| id == NODE) => {
            *value = current.clone();
        }
        Value::Object(map) => {
            for child in map.values_mut() {
                substitute(child, current);
            }
        }
        Value::Array(values) => {
            for child in values {
                substitute(child, current);
            }
        }
        _ => {}
    }
}
fn at_target(steps: Vec<GhStep>, current: &Value) -> Vec<GhStep> {
    steps
        .into_iter()
        .map(|mut step| {
            if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
                substitute(&mut value, current);
                step.stdout = serde_json::to_vec(&value).unwrap();
            }
            step
        })
        .collect()
}
fn metadata(current: &Value) -> Vec<GhStep> {
    vec![
        step("repos/example/repo/pulls/7", "GET", current.clone(), false),
        step("repos/example/repo/issues/7", "GET", issue(), false),
        step("repos/example/repo", "GET", repository(), false),
        step("repos/example/repo/pulls/7", "GET", current.clone(), false),
    ]
}
fn publication(before: &Value, after: &Value, field: &str, value: &str) -> Vec<GhStep> {
    // Controller principal/current PR, provider principal/current PR, exact
    // selected-field PATCH and read-back, independent account-bracketed native
    // conversation/details, then genuine current Metadata (not old JSON reused).
    let mut steps = at_target(guard(), before);
    steps.extend(at_target(guard(), before));
    steps.push(write(
        "repos/example/repo/pulls/7",
        "PATCH",
        Some(json!({field:value})),
        after.clone(),
    ));
    steps.push(step(
        "repos/example/repo/pulls/7",
        "GET",
        after.clone(),
        false,
    ));
    steps.extend(at_target(inspected(vec![starter()]), after));
    steps.extend(metadata(after));
    steps.extend(identity());
    assert_eq!(steps.len(), 25);
    steps
}
fn fixture() -> Fixture {
    let fixture = configured();
    fixture.init_git();
    fixture.commit_file(
        "tracked.txt",
        b"saved baseline\n",
        "Local metadata baseline",
    );
    fs::write(fixture.repo().join("tracked.txt"), b"dirty local bytes\n").unwrap();
    fixture
}
fn preserve_git(fixture: &Fixture, head: &str, index: &[u8], status: &str) {
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(
        fs::read(fixture.repo().join("tracked.txt")).unwrap(),
        b"dirty local bytes\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("unrelated.txt")).unwrap(),
        b"unchanged local bytes\n"
    );
}
fn observed_requests(session: &mut Session, fixture: &Fixture, count: usize) -> Vec<Value> {
    let values = records(session, fixture, count);
    for record in &values {
        let args = record["args"].as_array().unwrap();
        assert!(
            args[0] == "--version"
                || (args[0] == "api"
                    && args.iter().any(|arg| arg == "GET")
                    && record["stdin_hex"] == "")
                || (args[1] == "repos/example/repo/pulls/7"
                    && args.iter().any(|arg| arg == "PATCH")),
            "No arbitrary gh, search write, unrelated resource or project mutation: {record}"
        );
    }
    values
}
fn observe(session: &mut Session, mouse: bool, text: &str) {
    for _ in 0..16 {
        if session
            .try_wait_for(text, Duration::from_millis(30))
            .is_ok()
        {
            return;
        }
        if mouse {
            session.wheel(10, 16, true);
        } else {
            session.send_key(b"\x1b[B");
        }
    }
    session.wait_for_screen(text);
}
fn unchanged_metadata(session: &mut Session, mouse: bool) {
    // Actors are native shared username/avatar-fallback rows, not JSON fields.
    for text in [
        "Locked conversation: false",
        "Milestone: October release",
        "Label: bug",
        "Label: needs-triage",
        "@triager assigned",
        "@reviewer review requested",
        "Review team: Maintainers (maintainers)",
    ] {
        observe(session, mouse, text);
    }
}
fn replace_field(session: &mut Session, mouse: bool, label: &str, value: &str) {
    session.wait_for_screen(&format!("{label}:"));
    if mouse {
        let (column, row) = session.locate(&format!("{label}:")).unwrap();
        session.click(column + 1, row + 1);
    }
    session.send_key(b"\x01");
    session.paste(value);
    session.wait_for_screen(value.lines().next().unwrap());
}
fn local_preview(session: &mut Session, fixture: &Fixture, mouse: bool, count: usize) {
    session.wait_for_screen("[Preview]");
    choose(session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW • Confirm action");
    session.wait_for_screen("PR metadata preview");
    session.wait_for_screen("Target: example/repo#7");
    session.wait_for_screen("Caller: @octocat");
    session.wait_for_screen(HEAD);
    session.wait_for_screen("[Confirm action]");
    assert!(!session.screen().contains("[Submit]"));
    let commands = observed_requests(session, fixture, count);
    assert_eq!(
        commands.len(),
        count,
        "Local preview sends no provider request"
    );
}
fn submit(session: &mut Session, mouse: bool) {
    if mouse {
        session.click_visible_action("[Confirm action]");
    } else {
        session.focus_visible_form_action("[Confirm action]");
        session.send_key(b"\r");
    }
}
fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
#[derive(Debug)]
struct MetadataReceipt {
    attempt_id: i64,
    command: String,
    state: String,
    selected_resource: String,
    observed_head: String,
    payload_sha256: String,
    confirmed: Value,
}
fn receipts(fixture: &Fixture) -> Vec<MetadataReceipt> {
    let sql = Connection::open(cache_path(fixture)).unwrap();
    let scope = scope();
    let count: usize = sql
        .query_row(
            "SELECT COUNT(*) FROM pr_write_attempts WHERE host=?1 AND principal=?2",
            params![scope.host, scope.login],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        count <= 3,
        "Cancellation/refusal cannot create extra intents"
    );
    let rows = sql.prepare("SELECT attempt_id,command,state,selected_resource,observed_head,payload_sha256,confirmed_json FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND repository=?4 AND pr_number=7 ORDER BY attempt_id LIMIT 3").unwrap()
        .query_map(params![scope.host,scope.login,NODE,REPOSITORY],|row|Ok(MetadataReceipt {attempt_id:row.get(0)?,command:row.get(1)?,state:row.get(2)?,selected_resource:row.get(3)?,observed_head:row.get(4)?,payload_sha256:row.get(5)?,confirmed:serde_json::from_str::<Value>(&row.get::<_,String>(6)?).unwrap()})).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    assert_eq!(
        rows.len(),
        count,
        "No foreign original target may be hidden"
    );
    rows
}
fn assert_confirmed(fixture: &Fixture, changed: &[(&str, &str)], current: &Value) {
    let observed = receipts(fixture);
    assert_eq!(observed.len(), changed.len());
    for (receipt, (command, value)) in observed.iter().zip(changed) {
        assert!(receipt.attempt_id > 0);
        assert_eq!(receipt.command, *command);
        assert_eq!(receipt.state, "confirmed");
        assert_eq!(receipt.selected_resource, "pr");
        assert_eq!(receipt.observed_head, HEAD);
        assert_eq!(
            receipt.payload_sha256,
            PrWriteIntent {
                command: (*command).into(),
                selected_resource: "pr".into(),
                observed_head: HEAD.into(),
                values: BTreeMap::from([("body".into(), (*value).into())]),
            }
            .digest()
            .unwrap()
        );
        assert_eq!(
            receipt.confirmed,
            json!({"kind":"metadata","resource_id":NODE,"node_id":NODE})
        );
    }
    assert!(observed
        .windows(2)
        .all(|pair| pair[0].attempt_id < pair[1].attempt_id));
    let cache = Cache::open(&cache_path(fixture)).unwrap();
    let frame = cache
        .sync_frame(&scope(), &format!("pr-write-projection:{NODE}"))
        .unwrap()
        .unwrap();
    assert!(frame.complete);
    assert_eq!(frame.generation, observed.last().unwrap().attempt_id as u64);
    let saved = &frame
        .records
        .iter()
        .find(|record| record.kind == "pr-details" && record.id == NODE)
        .unwrap()
        .payload;
    assert_eq!(saved["summary"]["pr"]["title"], current["title"]);
    assert_eq!(saved["summary"]["pr"]["body"], current["body"]);
    assert_eq!(saved["summary"]["base_ref"], current["base"]["ref"]);
    assert_eq!(saved["summary"]["base_sha"], current["base"]["sha"]);
    assert_eq!(saved["summary"]["head_sha"], HEAD);
    assert_eq!(saved["comments"].as_array().unwrap().len(), 1);
    assert_eq!(saved["comments"][0]["body"], "Existing starter comment");
    assert!(cache
        .list(&Scope::github("github.invalid", Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
}
fn wait_local_draft(session: &mut Session, fixture: &Fixture, key: &str, value: &str) {
    session
        .try_wait_for_condition(
            |_| {
                Cache::open(&cache_path(fixture))
                    .unwrap()
                    .draft(&scope(), &format!("{REPOSITORY}/pull/7/composer/{key}"))
                    .unwrap()
                    .and_then(|text| serde_json::from_str::<Value>(&text).ok())
                    .is_some_and(|draft| {
                        draft["version"] == 2
                            && draft["values"]["body"] == value
                            && draft["publication"].is_null()
                    })
            },
            "actual local draft saved without a publication reference",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
}

#[test]
fn pr_metadata_keyboard_cancel_then_title_description_base_from_metadata_preserve_other_fields() {
    journey(false);
}
#[test]
fn pr_metadata_mouse_cancel_then_title_description_base_from_metadata_preserve_other_fields() {
    journey(true);
}
fn journey(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let original = target("Native collaboration", "A fixture description", "main");
    let titled = target(TITLE, "A fixture description", "main");
    let described = target(TITLE, BODY, "main");
    let rebased = target(TITLE, BODY, BASE);
    let mut steps = at_target(initial(), &original);
    steps.extend(at_target(details(vec![starter()]), &original));
    steps.extend(metadata(&original));
    assert_eq!(steps.len(), 18);
    steps.extend(publication(&original, &titled, "title", TITLE));
    let title_count = steps.len();
    steps.extend(publication(&titled, &described, "body", BODY));
    let body_count = steps.len();
    steps.extend(base::ordinary_catalogue(&described));
    steps.extend(base::exact_branch(&described, &"c".repeat(40)));
    steps.extend(publication(&described, &rebased, "base", BASE));
    let final_count = steps.len();
    assert_eq!((title_count, body_count, final_count), (43, 68, 113));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    session.send_key(b"1");
    session.assert_focused_view("Inbox");
    open(&mut session, mouse);
    choose(&mut session, mouse, "[u Metadata]", b"u");
    unchanged_metadata(&mut session, mouse);
    observed_requests(&mut session, &fixture, 18);
    menu(&mut session, mouse, "Edit PR title");
    replace_field(&mut session, mouse, "Title", "Cancelled title stays local");
    local_preview(&mut session, &fixture, mouse, 18);
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_overlay_close("┌ Edit PR title ");
    wait_local_draft(
        &mut session,
        &fixture,
        "title",
        "Cancelled title stays local",
    );
    assert!(receipts(&fixture).is_empty());
    assert_eq!(
        mutation_count(&observed_requests(&mut session, &fixture, 18)),
        0
    );
    unchanged_metadata(&mut session, mouse);
    preserve_git(&fixture, &head, &index, &status);

    for (action, field, value, command, expected, count) in [
        (
            "Edit PR title",
            "Title",
            TITLE,
            "legacy-title",
            &titled,
            title_count,
        ),
        (
            "Edit PR description",
            "Description",
            BODY,
            "legacy-description",
            &described,
            body_count,
        ),
        (
            "Change base branch",
            "Base branch",
            BASE,
            "legacy-base",
            &rebased,
            final_count,
        ),
    ] {
        menu(&mut session, mouse, action);
        if field == "Title" {
            session.wait_for_screen("Cancelled title stays local");
        }
        let before = count - if field == "Base branch" { 35 } else { 25 };
        if field == "Base branch" {
            base::load_picker(&mut session, &fixture, mouse, before);
            base::choose_branch(&mut session, mouse);
        } else {
            replace_field(&mut session, mouse, field, value);
        }
        local_preview(&mut session, &fixture, mouse, before);
        submit(&mut session, mouse);
        session.wait_for_screen(&format!("Published • {action}"));
        session.wait_for_overlay_close(&format!("┌ {action} "));
        session.wait_for_screen(TITLE);
        unchanged_metadata(&mut session, mouse);
        let transport = observed_requests(&mut session, &fixture, count);
        let writes = transport
            .iter()
            .filter(|record| is_mutation(record))
            .collect::<Vec<_>>();
        let expected_changes = match command {
            "legacy-title" => vec![("legacy-title", TITLE)],
            "legacy-description" => vec![("legacy-title", TITLE), ("legacy-description", BODY)],
            _ => vec![
                ("legacy-title", TITLE),
                ("legacy-description", BODY),
                ("legacy-base", BASE),
            ],
        };
        assert_eq!(writes.len(), expected_changes.len());
        for (write, (changed, value)) in writes.iter().zip(&expected_changes) {
            let key = match *changed {
                "legacy-title" => "title",
                "legacy-description" => "body",
                _ => "base",
            };
            let hex = write["stdin_hex"].as_str().unwrap();
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                serde_json::from_slice::<Value>(&bytes).unwrap(),
                json!({key:value}),
                "Unchanged fields must not be sent"
            );
        }
        assert_confirmed(&fixture, &expected_changes, expected);
        assert!(
            Cache::open(&cache_path(&fixture))
                .unwrap()
                .draft(
                    &scope(),
                    &format!(
                        "{REPOSITORY}/pull/7/composer/{}",
                        command.strip_prefix("legacy-").unwrap()
                    )
                )
                .unwrap()
                .is_none(),
            "Only confirmed original drafts are removed"
        );
        choose(&mut session, mouse, "[c Conversation]", b"c");
        session.wait_for_screen(expected["body"].as_str().unwrap().lines().next().unwrap());
        session.wait_for_screen("Existing starter comment");
        session.wait_for_screen("@octocat");
        choose(&mut session, mouse, "[o Summary]", b"o");
        session.wait_for_screen(&format!(
            "feature → {}",
            expected["base"]["ref"].as_str().unwrap()
        ));
        session.wait_for_screen(HEAD);
        choose(&mut session, mouse, "[u Metadata]", b"u");
        unchanged_metadata(&mut session, mouse);
        assert_eq!(
            observed_requests(&mut session, &fixture, count),
            transport,
            "Local tabs and details must not resend or refetch already prepared context"
        );
        preserve_git(&fixture, &head, &index, &status);
        if field == "Title" {
            session.resize(80, 24);
            session.wait_for_screen("[u Metadata]");
            unchanged_metadata(&mut session, mouse);
        }
    }
    session.quit_and_assert_restored_terminal(mouse);
    let completed = observed_requests(&mut session, &fixture, final_count);
    assert_eq!(mutation_count(&completed), 3);
    assert_confirmed(
        &fixture,
        &[
            ("legacy-title", TITLE),
            ("legacy-description", BODY),
            ("legacy-base", BASE),
        ],
        &rebased,
    );
    preserve_git(&fixture, &head, &index, &status);
}

#[test]
fn pr_metadata_keyboard_concurrent_title_refuses_before_marker_and_preserves_requested_draft() {
    concurrent_title(false);
}
#[test]
fn pr_metadata_mouse_concurrent_title_refuses_before_marker_and_preserves_requested_draft() {
    concurrent_title(true);
}
fn concurrent_title(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let original = target("Native collaboration", "A fixture description", "main");
    let concurrent = target(
        "Someone else changed the title",
        "A fixture description",
        "main",
    );
    let mut steps = at_target(initial(), &original);
    steps.extend(at_target(details(vec![starter()]), &original));
    steps.extend(metadata(&original));
    steps.extend(at_target(guard(), &concurrent));
    assert_eq!(steps.len(), 21);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    session.send_key(b"1");
    open(&mut session, mouse);
    choose(&mut session, mouse, "[u Metadata]", b"u");
    unchanged_metadata(&mut session, mouse);
    menu(&mut session, mouse, "Edit PR title");
    replace_field(&mut session, mouse, "Title", TITLE);
    local_preview(&mut session, &fixture, mouse, 18);
    submit(&mut session, mouse);
    session.wait_for_screen("PR intent refused");
    session.wait_for_screen("changed since preview");
    session.wait_for_screen(TITLE);
    let refused = observed_requests(&mut session, &fixture, 21);
    assert_eq!(mutation_count(&refused), 0);
    assert!(receipts(&fixture).is_empty());
    wait_local_draft(&mut session, &fixture, "title", TITLE);
    session.resize(80, 24);
    session.wait_for_screen("changed since preview");
    session.wait_for_screen(TITLE);
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_overlay_close("┌ Edit PR title ");
    session.wait_for_screen("Native collaboration");
    unchanged_metadata(&mut session, mouse);
    assert_eq!(observed_requests(&mut session, &fixture, 21), refused);
    preserve_git(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    observed_requests(&mut session, &fixture, 21);
    assert!(receipts(&fixture).is_empty());
}
