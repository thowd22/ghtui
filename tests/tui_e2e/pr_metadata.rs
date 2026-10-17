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
#[path = "pr_metadata_current_v51.rs"]
mod current_v51;
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
    json!({"id":410,"node_id":"R_metadata_410","full_name":REPOSITORY,
        "owner":{"id":71,"node_id":"O_metadata_71","login":"example","type":"Organization"},
        "private":true,"archived":false,"default_branch":"main","permissions":{"pull":true,"push":true,"maintain":false,"admin":false}})
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
fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
#[derive(Debug)]
struct MetadataReceipt {
    attempt_id: i64,
    payload_sha256: String,
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
    let rows = sql.prepare("SELECT attempt_id,payload_sha256 FROM pr_write_attempts WHERE host=?1 AND principal=?2 AND pr_node=?3 AND repository=?4 AND pr_number=7 ORDER BY attempt_id LIMIT 3").unwrap()
        .query_map(params![scope.host,scope.login,NODE,REPOSITORY],|row|Ok(MetadataReceipt {attempt_id:row.get(0)?,payload_sha256:row.get(1)?})).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    assert_eq!(
        rows.len(),
        count,
        "No foreign original target may be hidden"
    );
    rows
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
    current_v51::happy(mouse);
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
    current_v51::concurrent(mouse);
}
