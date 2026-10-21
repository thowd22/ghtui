//! ORIGINAL20 happy journeys, not aliases to current Members test scenarios.
//! Old requester/Unicode/IDs/opposite members remain; only real current wire
//! producers/controls/receipt-key ABI replace obsolete85/F4/localPreview.
use super::*;
use crate::support::completed_navigation_controls_v1::{owned_action_position, FocusAttributeV1};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
};

const REPO: &str = "example/repo";
const SELECT: &[&str] = &["[Choose member]", "[Preview]", "[Esc Close]"];
const CLEAR: &[&str] = &["[Preview]", "[Esc Close]"];
const PREVIEW: &[&str] = &["[Preview]", "[Confirm action]", "[Keep draft / Close]"];
const PICKER: &[&str] = &[
    "[Enter Choose]",
    "[Esc Back]",
    "[Ctrl-U Clear]",
    "[PgUp]",
    "[PgDn]",
];

fn principal() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        values::get("user", json!({"id":42,"login":"requester"})),
    ]
}
fn repo() -> Value {
    let mut raw = values::repository();
    raw["owner"] = json!({"id":310,"node_id":"O_310","login":"example","type":"Organization"});
    raw
}
fn archived_facts(raw: &mut Value) {
    for row in raw["labels"].as_array_mut().unwrap() {
        row["archived_at"] = Value::Null;
        row["archived_by"] = Value::Null;
    }
}
fn raw(changed: Option<PrMetadataCommand>) -> Value {
    let mut raw = changed.map_or_else(current_parent, after);
    raw["url"] = json!("https://github.invalid/api/v3/repos/example/repo/pulls/7");
    raw["head"]["repo"] = repo();
    raw["base"]["repo"] = repo();
    archived_facts(&mut raw);
    raw
}
fn issue(raw: &Value) -> Value {
    let mut value = values::issue_from(raw);
    value["url"] = json!("https://github.invalid/api/v3/repos/example/repo/issues/7");
    value
}
fn candidate(command: PrMetadataCommand) -> Value {
    let mut value = values::candidate(command);
    if values::field(command) == "labels" {
        value["archived_at"] = Value::Null;
        value["archived_by"] = Value::Null;
    }
    value
}
fn context(raw: &Value) -> Vec<GhStep> {
    vec![
        values::get("repos/example/repo/pulls/7", raw.clone()),
        values::get("repos/example/repo/issues/7", issue(raw)),
        values::get("repos/example/repo", repo()),
        values::get("repos/example/repo/pulls/7", raw.clone()),
    ]
}
fn section(raw: &Value) -> Vec<GhStep> {
    let mut calls = principal();
    calls.push(values::get("repos/example/repo", repo()));
    calls.extend(context(raw));
    calls.push(values::get("repos/example/repo", repo()));
    calls.extend(principal());
    assert_eq!(calls.len(), 10);
    calls
}
fn startup(raw: &Value) -> Vec<GhStep> {
    let mut calls = bound(initial(), raw);
    calls.extend(bound(details(vec![starter()]), raw));
    for call in &mut calls {
        if call.args == ["--version"] {
            call.stdout = b"gh version 2.99.0 (2026-10-01)\n".to_vec();
        }
    }
    calls.extend(section(raw));
    assert_eq!(calls.len(), 25);
    calls
}
fn gql(command: PrMetadataCommand, raw: &Value, selected: bool) -> GhStep {
    let mut query =
        model::providers::gh::pr_metadata_members_query(values::field(command)).unwrap();
    let mut vars = json!({"id":"PR_7"});
    let mut response = values::members(command, raw);
    if values::field(command) == "teams" {
        if selected && command == PrMetadataCommand::AddTeam {
            query = query
                .replace(
                    ",$candidate:ID!,$includeCandidate:Boolean!",
                    ",$candidate:ID!",
                )
                .replace(" @include(if:$includeCandidate)", "");
            vars["candidate"] = json!("T_76");
        } else {
            query=query.replace(",$candidate:ID!,$includeCandidate:Boolean!","").replace(" candidate:node(id:$candidate) @include(if:$includeCandidate){__typename ...on Team{id slug organization{login}}}","");
            response["data"]
                .as_object_mut()
                .unwrap()
                .remove("candidate");
        }
    }
    values::graphql(&query, vars, response)
}
fn phase(command: PrMetadataCommand, selected: bool) -> Vec<GhStep> {
    let raw = raw(None);
    let endpoint = match values::field(command) {
        "labels" => "labels",
        "assignees" => "assignees",
        "reviewers" => "collaborators",
        "teams" => "teams",
        _ => "milestones?state=open",
    };
    let separator = if endpoint.contains('?') { '&' } else { '?' };
    let mut calls = principal();
    calls.extend([
        values::get("repos/example/repo", repo()),
        values::get("repos/example/repo/pulls/7", raw.clone()),
        values::get("repos/example/repo/issues/7", issue(&raw)),
        values::get(
            &format!("repos/{REPO}/{endpoint}{separator}per_page=100&page=1"),
            json!([candidate(command)]),
        ),
        gql(command, &raw, selected),
        values::get("repos/example/repo/pulls/7", raw),
        values::get("repos/example/repo", repo()),
    ]);
    calls.extend(principal());
    assert_eq!(calls.len(), 11);
    calls
}
fn paid(command: PrMetadataCommand) -> Vec<GhStep> {
    let raw = raw(Some(command));
    let reviews = matches!(values::field(command), "reviewers" | "teams");
    let mutation = if reviews {
        model::providers::gh::PR_METADATA_REQUEST_REVIEWS_MUTATION
    } else {
        model::providers::gh::PR_METADATA_UPDATE_MUTATION
    };
    let mut calls = vec![
        values::graphql(
            mutation,
            json!({"input":values::mutation_input(command)}),
            if reviews {
                json!({"data":{"requestReviews":{"pullRequest":{"id":"PR_7"}}}})
            } else {
                json!({"data":{"updatePullRequest":{"pullRequest":{"id":"PR_7"}}}})
            },
        ),
        gql(command, &raw, true),
        values::get("repos/example/repo", repo()),
        values::get("repos/example/repo/pulls/7", raw.clone()),
        values::get("repos/example/repo/issues/7", issue(&raw)),
        values::get("user", json!({"id":42,"login":"requester"})),
        values::get("user", json!({"id":42,"login":"requester"})),
        values::get("repos/example/repo", repo()),
        values::get("repos/example/repo/pulls/7", raw.clone()),
        values::get("repos/example/repo/issues/7", issue(&raw)),
        gql(command, &raw, true),
    ];
    for (endpoint, rows) in [
        ("issues/7/comments", json!([starter()])),
        ("pulls/7/commits", json!([])),
        ("pulls/7/files", json!([])),
        ("pulls/7/reviews", json!([])),
        ("issues/7/timeline", json!([])),
    ] {
        let mut child = values::get(
            &format!("repos/{REPO}/{endpoint}?per_page=100"),
            json!([rows]),
        );
        child.args.extend(["--paginate".into(), "--slurp".into()]);
        calls.push(child);
    }
    calls.push(values::get("repos/example/repo/pulls/7", raw.clone()));
    calls.extend(context(&raw));
    calls.push(values::get("user", json!({"id":42,"login":"requester"})));
    assert_eq!(calls.len(), 22);
    calls
}
fn form_title(command: PrMetadataCommand) -> String {
    format!("{} • {REPO}#7", command.label())
}
fn controls(command: PrMetadataCommand) -> &'static [&'static str] {
    if command == PrMetadataCommand::ClearMilestone {
        CLEAR
    } else {
        SELECT
    }
}
fn ready(s: &mut Session, before: usize, title: &str, labels: &[&str]) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > before
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, title).is_some_and(|body| {
                        labels
                            .iter()
                            .all(|label| crate::support::controls::body_has(frame, &body, label))
                    })
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW exact owned {title}/{labels:?}\n{}",
            s.screen()
        );
    }
}
fn action(s: &mut Session, mouse: bool, title: &str, label: &str, controls: &[&str]) {
    if !mouse {
        s.focus_completed_owned_action_v1(title, label, FocusAttributeV1::Inverse, controls, &[]);
    }
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let point = s.completed_cells_screen().and_then(|frame| {
            crate::support::overlay_body(frame, title)
                .filter(|body| controls.iter().all(|text| body.contains(text)))?;
            owned_action_position(frame, title, label, &[]).filter(|(x, y)| {
                frame.cell(*y, *x).is_some_and(|cell| {
                    (mouse || cell.inverse()) && (label != "[Confirm action]" || cell.underline())
                })
            })
        });
        if let Some((x, y)) = point {
            if mouse {
                s.click(x, y)
            } else {
                s.send_key(b"\r")
            };
            return;
        }
        assert!(
            Instant::now() < until,
            "Actual enabled whole control {title}/{label}\n{}",
            s.screen()
        );
    }
}
fn picker_title(command: PrMetadataCommand) -> &'static str {
    match command {
        PrMetadataCommand::AddLabel => "PR metadata choices · Label",
        PrMetadataCommand::RemoveLabel => "Current PR metadata members · Label",
        PrMetadataCommand::AddAssignee => "PR metadata choices · Assignee",
        PrMetadataCommand::RemoveAssignee => "Current PR metadata members · Assignee",
        PrMetadataCommand::AddReviewer => "PR metadata choices · Reviewer",
        PrMetadataCommand::RemoveReviewer => "Current PR metadata members · Reviewer",
        PrMetadataCommand::AddTeam => "PR metadata choices · Reviewer team",
        PrMetadataCommand::RemoveTeam => "Current PR metadata members · Reviewer team",
        _ => "PR metadata choices · Milestone",
    }
}
fn select(s: &mut Session, mouse: bool, command: PrMetadataCommand) {
    if command == PrMetadataCommand::ClearMilestone {
        return;
    }
    let title = form_title(command);
    let before = s.completed_cells_end();
    action(s, mouse, &title, "[Choose member]", controls(command));
    ready(
        s,
        before,
        picker_title(command),
        &[selected_name(command), "[Enter Choose]", "[Esc Back]"],
    );
    if mouse {
        s.click_visible_action(selected_name(command));
    }
    let before = s.completed_cells_end();
    if mouse {
        action(s, true, picker_title(command), "[Enter Choose]", PICKER);
    } else {
        let notices = [
            format!("{} · Filter focused", picker_title(command)),
            format!("{} · Rows focused", picker_title(command)),
        ];
        s.focus_completed_owned_action_v1(
            picker_title(command),
            "[Enter Choose]",
            FocusAttributeV1::Inverse,
            PICKER,
            &notices.each_ref().map(String::as_str),
        );
        s.send_key(b"\r");
    }
    ready(s, before, &title, controls(command));
    let expected = selected_value(command);
    assert!(
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), &title)
            .unwrap()
            .contains(&expected)
    );
}
fn selected(command: PrMetadataCommand) -> (u64, String) {
    let (id, prefix) = match command {
        PrMetadataCommand::AddLabel => (81, "L"),
        PrMetadataCommand::RemoveLabel => (80, "L"),
        PrMetadataCommand::AddAssignee => (61, "U"),
        PrMetadataCommand::RemoveAssignee => (60, "U"),
        PrMetadataCommand::AddReviewer => (71, "U"),
        PrMetadataCommand::RemoveReviewer => (70, "U"),
        PrMetadataCommand::AddTeam => (76, "T"),
        PrMetadataCommand::RemoveTeam => (75, "T"),
        PrMetadataCommand::SetMilestone => (86, "M"),
        PrMetadataCommand::ClearMilestone => (85, "M"),
    };
    (id, format!("{prefix}_{id}"))
}
fn selected_value(command: PrMetadataCommand) -> String {
    if command == PrMetadataCommand::ClearMilestone {
        return "Clear the actual current milestone".into();
    }
    let (id, node) = selected(command);
    format!("{} • actual #{id} • {node}", selected_name(command))
}
fn exact(s: &mut Session, f: &Fixture, steps: &[GhStep], count: usize) -> Vec<Value> {
    let rows = records(s, f, count);
    for (index, (actual, expected)) in rows.iter().zip(&steps[..count]).enumerate() {
        assert_eq!(actual["step"], index);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(
            actual["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        assert!(actual["pid"].as_u64().is_some_and(|pid| pid > 0));
    }
    rows
}
fn db(f: &Fixture) -> Connection {
    Connection::open_with_flags(cache_path(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
}
fn sql(f: &Fixture) -> Vec<String> {
    let db = db(f);
    let mut schema = db
        .prepare("SELECT type,name,tbl_name,rootpage,sql FROM sqlite_master ORDER BY type,name")
        .unwrap();
    let mut result = schema
        .query_map([], |r| {
            (0..5)
                .map(|n| r.get::<_, rusqlite::types::Value>(n))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap()
        .map(Result::unwrap)
        .map(|row| format!("schema:{row:?}"))
        .collect::<Vec<_>>();
    let names = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    for name in names {
        let mut query = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = query.column_count();
        let mut rows = query
            .query_map([], |r| {
                (0..columns)
                    .map(|n| r.get::<_, rusqlite::types::Value>(n))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .map(|row| format!("{name}:{row:?}"))
            .collect::<Vec<_>>();
        rows.sort();
        result.extend(rows);
    }
    result
}
fn saved(f: &Fixture, command: PrMetadataCommand) -> (String, String) {
    let db = db(f);
    let mut query=db.prepare("SELECT resource,body FROM drafts WHERE host='github.invalid' AND login='user-id:42' AND resource LIKE 'metadata-members-v48/%' ORDER BY resource").unwrap();
    let rows = query
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    let (key, body) = &rows[0];
    let value: Value = serde_json::from_str(body).unwrap();
    assert_eq!(
        value,
        json!({"version":17,"values":{"body":selected_value(command)},"publication":null})
    );
    #[derive(Serialize)]
    struct Envelope {
        version: u8,
        values: BTreeMap<String, String>,
        publication: Option<()>,
    }
    assert_eq!(
        body,
        &serde_json::to_string(&Envelope {
            version: 17,
            values: BTreeMap::from([("body".into(), selected_value(command))]),
            publication: None
        })
        .unwrap()
    );
    assert_eq!(key, &expected_keys(command).1);
    rows.into_iter().next().unwrap()
}
fn hash(value: &impl Serialize) -> String {
    format!("{:x}", Sha256::digest(serde_json::to_vec(value).unwrap()))
}
fn bytes_hash(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}
fn expected_keys(command: PrMetadataCommand) -> (String, String) {
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let (id, node) = selected(command);
    let namespace = if values::field(command) == "teams" {
        Some("example")
    } else {
        None
    };
    let base = format!(
        "metadata-members-base-v48/{}",
        hash(&(
            "ghtui-metadata-members-base-v48",
            scope,
            410u64,
            "R_410",
            1007u64,
            "PR_7",
            7u64,
            2007u64,
            "I_2007",
            command.key(),
            id,
            node,
            hash(&namespace)
        ))
    );
    let draft = format!(
        "metadata-members-v48/{}",
        hash(&(
            "ghtui-metadata-members-draft-v48",
            &base,
            "a".repeat(40),
            "b".repeat(40)
        ))
    );
    (base, draft)
}
#[derive(Serialize)]
struct Member {
    id: u64,
    node: String,
    number: Option<u64>,
    name: String,
    namespace: Option<String>,
}
fn members(raw: &Value, field: &str) -> Vec<Member> {
    let key = match field {
        "reviewers" => "requested_reviewers",
        "teams" => "requested_teams",
        other => other,
    };
    let list = if field == "milestone" {
        if raw[key].is_null() {
            vec![]
        } else {
            vec![&raw[key]]
        }
    } else {
        raw[key].as_array().unwrap().iter().collect()
    };
    let name = match field {
        "labels" => "name",
        "teams" => "slug",
        "milestone" => "title",
        _ => "login",
    };
    let mut members = list
        .into_iter()
        .map(|r| Member {
            id: r["id"].as_u64().unwrap(),
            node: r["node_id"].as_str().unwrap().into(),
            number: if field == "milestone" {
                Some(r["number"].as_u64().unwrap())
            } else {
                None
            },
            name: r[name].as_str().unwrap().into(),
            namespace: if field == "teams" {
                Some("example".into())
            } else {
                None
            },
        })
        .collect::<Vec<_>>();
    members.sort_by_key(|m| m.id);
    members
}
#[derive(Serialize)]
struct Database {
    path: Vec<u8>,
    device: u64,
    inode: u64,
}
#[derive(Serialize)]
struct Seed {
    version: u8,
    database: Database,
    scope: Scope,
    repository_id: u64,
    repository_node: String,
    repository: String,
    pr_id: u64,
    pr_node: String,
    number: u64,
    issue_id: u64,
    issue_node: String,
    head: String,
    base: String,
    head_ref_sha256: String,
    base_ref_sha256: String,
    action: String,
    selected_id: u64,
    selected_node: String,
    selected_namespace_sha256: String,
    base_key: String,
    draft_key: String,
    parent_ack_attempt: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lineage_v52: Option<()>,
    before_sha256: String,
    after_sha256: String,
    expected_effect: model::pr_write_journal::PrConfirmedEffect,
    requested_plan_sha256: String,
    values_sha256: String,
}
fn expected_seed(f: &Fixture, command: PrMetadataCommand) -> Seed {
    let field = values::field(command);
    let before = members(&raw(None), field);
    let after = members(&raw(Some(command)), field);
    let preserved = match field {
        "reviewers" => members(&raw(None), "teams"),
        "teams" => members(&raw(None), "reviewers"),
        _ => vec![],
    };
    let before_sha = hash(&(&before, &preserved));
    let after_sha = hash(&(&after, &preserved));
    let (id, node) = selected(command);
    let result = after
        .iter()
        .map(|m| format!("{}.{}", m.id, m.number.unwrap_or(0)))
        .collect::<Vec<_>>()
        .join("+");
    let opposite = preserved
        .iter()
        .map(|m| m.id.to_string())
        .collect::<Vec<_>>()
        .join("+");
    let effect = model::pr_write_journal::PrConfirmedEffect {
        kind: model::pr_write_journal::PrEffectKind::Metadata,
        resource_id: format!(
            "metadata-v2/{}/410/1007/2007/{id}/{before_sha}/{after_sha}/{}/{}",
            command.key(),
            if result.is_empty() { "empty" } else { &result },
            if opposite.is_empty() {
                "empty"
            } else {
                &opposite
            }
        ),
        node_id: Some("PR_7".into()),
    };
    let values = BTreeMap::from([
        ("before_sha256".into(), before_sha.clone()),
        ("after_sha256".into(), after_sha.clone()),
        ("base_sha".into(), "b".repeat(40)),
        ("base_ref_sha256".into(), hash(&"main")),
        ("head_ref_sha256".into(), hash(&"feature")),
        ("canonical_ids".into(), "410/1007/2007/42".into()),
    ]);
    let requested = model::pr_write_journal::PrWriteIntent {
        command: command.key().into(),
        selected_resource: format!("{field}:{id}"),
        observed_head: "a".repeat(40),
        values,
    }
    .digest()
    .unwrap();
    let path = fs::canonicalize(cache_path(f)).unwrap();
    let stat = fs::symlink_metadata(&path).unwrap();
    assert!(stat.is_file());
    assert_eq!(stat.mode() & 0o777, 0o600);
    assert_eq!(stat.nlink(), 1);
    let (base_key, draft_key) = expected_keys(command);
    let namespace = if field == "teams" {
        Some("example")
    } else {
        None
    };
    Seed {
        version: 48,
        database: Database {
            path: path.as_os_str().as_bytes().to_vec(),
            device: stat.dev(),
            inode: stat.ino(),
        },
        scope: Scope::github("github.invalid", Some(42)).unwrap(),
        repository_id: 410,
        repository_node: "R_410".into(),
        repository: REPO.into(),
        pr_id: 1007,
        pr_node: "PR_7".into(),
        number: 7,
        issue_id: 2007,
        issue_node: "I_2007".into(),
        head: "a".repeat(40),
        base: "b".repeat(40),
        head_ref_sha256: bytes_hash(b"feature"),
        base_ref_sha256: bytes_hash(b"main"),
        action: command.key().into(),
        selected_id: id,
        selected_node: node,
        selected_namespace_sha256: hash(&namespace),
        base_key,
        draft_key,
        parent_ack_attempt: None,
        lineage_v52: None,
        before_sha256: before_sha,
        after_sha256: after_sha,
        expected_effect: effect,
        requested_plan_sha256: requested,
        values_sha256: hash(&BTreeMap::from([("body", selected_value(command))])),
    }
}
fn receipt(f: &Fixture, command: PrMetadataCommand, saved_key: &str, seed: Seed) {
    let db = db(f);
    let(id,host,uid,node,purpose,state,key,repo,number,head,payload,confirmed):(i64,String,String,String,String,String,String,String,u64,String,String,String)=db.query_row("SELECT attempt_id,host,principal,pr_node,command,state,selected_resource,repository,pr_number,observed_head,payload_sha256,confirmed_json FROM pr_write_attempts",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?,r.get(9)?,r.get(10)?,r.get(11)?))).unwrap();
    assert!(id > 0);
    assert_eq!(
        (
            host.as_str(),
            uid.as_str(),
            node.as_str(),
            purpose.as_str(),
            state.as_str(),
            repo.as_str(),
            number
        ),
        (
            "github.invalid",
            "user-id:42",
            "PR_7",
            command.key(),
            "confirmed",
            REPO,
            7
        )
    );
    assert_eq!(head, "a".repeat(40));
    assert_eq!(key, saved_key);
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        1
    );
    let effect: model::pr_write_journal::PrConfirmedEffect =
        serde_json::from_str(&confirmed).unwrap();
    assert_eq!(effect, seed.expected_effect);
    assert!(!confirmed.contains("Private description") && !confirmed.contains("requester"));
    let decoded = PrMetadataReceipt::decode(&effect).unwrap();
    assert_eq!(decoded.command(), command);
    assert_eq!(decoded.repository_id(), 410);
    assert_eq!(decoded.selected_id(), Some(selected(command).0));
    let field = values::field(command);
    let actual_after = members(&raw(Some(command)), field);
    assert_eq!(
        decoded.result_ids(),
        actual_after
            .iter()
            .map(|m| (m.id, m.number))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        decoded.preserved_review_ids(),
        match field {
            "reviewers" => vec![75],
            "teams" => vec![70],
            _ => vec![],
        }
    );
    let (origin,base,action,parent_ack):(String,String,String,Option<i64>)=db.query_row("SELECT origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48 FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    let observed: Value = serde_json::from_str(&origin).unwrap();
    assert_eq!(observed["seed"],serde_json::to_value(&seed).unwrap(),"ALL actual original scalars/digests independently derived from fixture IDs/sets/refs/realDB, not accepted from stored output");
    assert_eq!(
        (base, action, parent_ack),
        (seed.base_key.clone(), command.key().into(), None)
    );
    let write = model::pr_write_journal::PrWriteIntent {
        command: command.key().into(),
        selected_resource: key.clone(),
        observed_head: "a".repeat(40),
        values: BTreeMap::from([
            ("body".into(), selected_value(command)),
            ("origin-members-v48".into(), hash(&seed)),
            (
                "requested-plan-v48".into(),
                seed.requested_plan_sha256.clone(),
            ),
        ]),
    };
    assert_eq!(payload, write.digest().unwrap());
    assert_eq!(
        observed["reference"],
        json!({"attempt_id":id,"pr_node_id":"PR_7","observed_head":"a".repeat(40),"payload_sha256":payload})
    );
    assert_eq!(db.query_row("SELECT count(*) FROM drafts WHERE host='github.invalid' AND login='user-id:42' AND resource=?1",[saved_key],|r|r.get::<_,usize>(0)).unwrap(),0);
    let context:String=db.query_row("SELECT payload FROM records WHERE host='github.invalid' AND login='user-id:42' AND kind='pr-metadata-context-v48' AND id='PR_7'",[],|r|r.get(0)).unwrap();
    let context: Value = serde_json::from_str(&context).unwrap();
    let after = raw(Some(command));
    for key in ["labels", "assignees", "milestone"] {
        assert_eq!(
            context["issue_metadata"][key], after[key],
            "Changed/unmodified complete actual structured field {key}"
        );
    }
    assert_eq!(context["issue_metadata"]["id"], 2007);
    assert_eq!(context["issue_metadata"]["node_id"], "I_2007");
    let users = context["requested_reviewers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["login"].as_str().unwrap())
        .collect::<Vec<_>>();
    let expected = after["requested_reviewers"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["login"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(users, expected);
    let teams = context["requested_teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["id"].as_u64().unwrap(), r["slug"].as_str().unwrap()))
        .collect::<Vec<_>>();
    let expected = after["requested_teams"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| (r["id"].as_u64().unwrap(), r["slug"].as_str().unwrap()))
        .collect::<Vec<_>>();
    assert_eq!(teams, expected);
    let (projection,repository,complete,updated):(String,String,bool,String)=db.query_row("SELECT payload,repository,complete,source_updated_at FROM records WHERE host='github.invalid' AND login='user-id:42' AND kind='pr-details' AND id='PR_7'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
    assert_eq!(
        (repository.as_str(), complete, updated.as_str()),
        (REPO, true, "2026-10-13T00:00:00Z")
    );
    let projection: Value = serde_json::from_str(&projection).unwrap();
    assert_eq!(projection["summary"]["pr"]["id"], "PR_7");
    assert_eq!(projection["summary"]["pr"]["body"], original_body());
    assert_eq!(projection["summary"]["pr"]["title"], "Unchanged title");
    assert_eq!(projection["summary"]["head_sha"], "a".repeat(40));
    assert_eq!(projection["summary"]["base_sha"], "b".repeat(40));
    assert_eq!(projection["comments"].as_array().unwrap().len(), 1);
    assert_eq!(projection["comments"][0]["id"], 9);
    assert_eq!(
        projection["comments"][0]["body"],
        "Existing starter comment"
    );
    for kind in ["commits", "files", "reviews", "timeline"] {
        assert_eq!(projection[kind], json!([]));
    }
    let mut query=db.prepare("SELECT kind,mutation_ordinal FROM sync_mutation_fences WHERE host='github.invalid' AND login='user-id:42' AND namespace='collaboration' AND repository='example/repo' AND resource='PR_7' ORDER BY kind").unwrap();
    let fences = query
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u64>(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let kinds = if field == "labels" {
        vec![
            "conversation_comment",
            "inline_comment",
            "pull_request_body",
            "review",
        ]
    } else {
        vec![]
    };
    assert_eq!(
        fences,
        kinds
            .iter()
            .map(|kind| (kind.to_string(), u64::try_from(id).unwrap()))
            .collect::<Vec<_>>()
    );
}
fn original_body() -> &'static str {
    "Private description must never be in a receipt"
}
fn metadata_frame(s: &mut Session, before: usize) {
    s.wait_for_new_cursorless_condition(before,|frame| {
        let text=frame.contents();crate::support::main_footer_visible(frame)
            && crate::support::overlay_body(frame,"Inbox • focused view ─").is_some_and(|body|body.contains("example/repo#7")&&body.contains("Source: https://github.invalid/example/repo/pull/7")&&body.contains("Your repository permissions: read true • write true")&&body.contains("Locked conversation: false"))
            && crate::support::controls::control_painted(frame, "[u Metadata]")&&crate::support::controls::control_painted(frame, "[p PR actions]")
            && !text.contains("[Esc Cancel]")&&!text.contains("[Confirm action]")
    },"NEW actual same-owned Metadata rows/fullSource/permissions/mainfooter, no Summary substitute",Duration::from_secs(8));
}
fn launch(f: &Fixture, mouse: bool, narrow: bool) -> Session {
    let trace = f.root.join("legacy-members-v96-git-trace.jsonl");
    let mut env = vec![("GIT_TRACE2_EVENT", trace.to_str().unwrap())];
    if mouse {
        env.push(("NO_COLOR", "1"));
    }
    let mut s = Session::launch_binary(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        f,
        if narrow { 80 } else { 120 },
        if narrow { 24 } else { 40 },
        &env,
    );
    s.wait_for_screen("Recent commits");
    s.assert_focused_view("Repository");
    s
}
fn enter(s: &mut Session, mouse: bool) {
    let original = s.completed_cells_screen().unwrap().size();
    let before = s.completed_cursorless_end();
    choose(s, mouse, "1 Inbox", b"1");
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            crate::support::main_footer_visible(frame)
                && frame.contents().contains("Inbox • focused view")
        },
        "ONE actual Inbox navigation before Refresh",
        Duration::from_secs(8),
    );
    choose(s, mouse, "[r Refresh]", b"r");
    s.wait_for_screen("Unchanged title");
    s.wait_for_screen("Fresh • fetched");
    choose(s, mouse, "Unchanged title", b"\r");
    s.wait_for_screen("1 comments");
    if original.1 < 120 {
        let before = s.completed_cursorless_end();
        s.resize(120, 40);
        s.wait_for_new_cursorless_condition(
            before,
            |frame| frame.size() == (40, 120) && crate::support::main_footer_visible(frame),
            "NEW temporary wide current frame before exact fullH witness",
            Duration::from_secs(8),
        );
    }
    let before = s.completed_cursorless_end();
    choose(s, mouse, "[c Conversation]", b"c");
    s.wait_for_new_cursorless_condition(before,|frame|{let text=frame.contents();crate::support::main_footer_visible(frame)&&text.contains("example/repo#7")&&text.contains(&"a".repeat(40))&&text.contains("@requester PR description")&&text.contains("Private description must never be in a receipt")&&text.contains("@octocat comment #9 • 2026-10-13T00:00:00Z")&&text.contains("Existing starter comment")&&!text.contains("Verifying original Conversation")&&!text.contains("[Esc Cancel]")},"NEW completed idle genuine Conversation/fullPR+H+actual description/comment rows before ONE u",Duration::from_secs(8));
    let before = s.completed_cursorless_end();
    choose(s, mouse, "[u Metadata]", b"u");
    metadata_frame(s, before);
    if original.1 < 120 {
        let before = s.completed_cursorless_end();
        s.resize(original.1, original.0);
        metadata_frame(s, before);
    }
}
fn open_current(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    command: PrMetadataCommand,
    end: usize,
    steps: &[GhStep],
) {
    let before = s.completed_cells_end();
    menu(s, mouse, title(command));
    s.wait_for_command_count(f, end);
    ready(s, before, &form_title(command), controls(command));
    let body =
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), &form_title(command))
            .unwrap();
    assert!(body.contains("Requester requester • account 42 • original PR metadata • no resend"));
    assert!(body.contains("Original author @requester • account #42 (not requester)"));
    assert!(body.contains("Selected actual member:"));
    assert!(!body.contains("[Confirm action]") && !body.contains("[View original]"));
    if command != PrMetadataCommand::ClearMilestone {
        let rows = body.lines().collect::<Vec<_>>();
        let headings = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.trim() == "Selected actual member:")
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert_eq!(headings.len(), 1, "One actual owned selected-member field");
        assert!(
            rows.get(headings[0] + 1)
                .is_some_and(|row| row.chars().all(char::is_whitespace)),
            "NEW original selection stays empty, never restored approval/member from Saved17"
        );
    }
    exact(s, f, steps, end);
    assert_no_publication(f);
}
fn close(s: &mut Session, mouse: bool, command: PrMetadataCommand, preview: bool) {
    let before = s.completed_cursorless_end();
    action(
        s,
        mouse,
        &form_title(command),
        if preview {
            "[Keep draft / Close]"
        } else {
            "[Esc Close]"
        },
        if preview { PREVIEW } else { controls(command) },
    );
    metadata_frame(s, before);
}
fn preview(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    command: PrMetadataCommand,
    end: usize,
    steps: &[GhStep],
) -> (String, String) {
    let before = s.completed_cells_end();
    action(
        s,
        mouse,
        &form_title(command),
        "[Preview]",
        controls(command),
    );
    s.wait_for_command_count(f, end);
    ready(
        s,
        before,
        &form_title(command),
        &[
            "READ-ONLY PREVIEW",
            "[Confirm action]",
            "[Keep draft / Close]",
        ],
    );
    if command.aggregate_review_replacement() {
        observe(s, mouse, "replacement");
    }
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let acknowledged = s.completed_cells_screen().is_some_and(|frame| {
            crate::support::main_footer_visible(frame)
                && frame
                    .contents()
                    .contains("Draft saved locally • nothing published")
                && crate::support::overlay_body(frame, &form_title(command))
                    .is_some_and(|body| PREVIEW.iter().all(|label| body.contains(label)))
        });
        let present=db(f).query_row("SELECT count(*) FROM drafts WHERE host='github.invalid' AND login='user-id:42' AND resource=?1 AND json_extract(body,'$.version')=17 AND json_type(body,'$.publication')='null' AND json_extract(body,'$.values.body')=?2",rusqlite::params![expected_keys(command).1,selected_value(command)],|r|r.get::<_,usize>(0)).unwrap()==1;
        if acknowledged && present {
            break;
        }
        assert!(
            Instant::now() < until,
            "Exact actual DraftWriter Saved ACK before confirming or restarting\n{}",
            s.screen()
        );
    }
    exact(s, f, steps, end);
    assert_no_publication(f);
    s.capture_completed_terminal_v1("legacy-members-v96-fresh-preview-saved17-no-publication");
    saved(f, command)
}
fn foreign(f: &Fixture) -> Vec<String> {
    let db = db(f);
    let mut result = Vec::new();
    for (table, predicate) in [
        ("drafts", "resource='legacy-v96-unrelated'"),
        ("records", "kind='legacy-v96-unrelated'"),
    ] {
        let mut query = db
            .prepare(&format!("SELECT * FROM {table} WHERE {predicate}"))
            .unwrap();
        let columns = query.column_count();
        let mut rows = query
            .query_map([], |r| {
                (0..columns)
                    .map(|index| r.get::<_, rusqlite::types::Value>(index))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .map(|row| format!("{table}:{row:?}"))
            .collect::<Vec<_>>();
        rows.sort();
        result.extend(rows);
    }
    result
}
pub(super) fn happy(command: PrMetadataCommand, mouse: bool) {
    let fixture = fixture();
    let original = raw(None);
    let changed = raw(Some(command));
    for key in [
        "id", "node_id", "number", "title", "body", "user", "head", "base", "draft", "state",
        "merged",
    ] {
        assert_eq!(original[key], changed[key], "Unmodified canonical {key}");
    }
    let changed_key = match values::field(command) {
        "reviewers" => "requested_reviewers",
        "teams" => "requested_teams",
        other => other,
    };
    for key in [
        "labels",
        "assignees",
        "requested_reviewers",
        "requested_teams",
        "milestone",
    ] {
        if key != changed_key {
            assert_eq!(
                original[key], changed[key],
                "Unmodified/opposite member {key}"
            );
        }
    }
    let mut steps = startup(&original);
    steps.extend(phase(command, false));
    assert_eq!(steps.len(), 36);
    steps.extend(phase(command, false));
    assert_eq!(steps.len(), 47);
    steps.extend(phase(command, true));
    assert_eq!(steps.len(), 58);
    steps.extend(startup(&original));
    assert_eq!(steps.len(), 83);
    steps.extend(phase(command, false));
    assert_eq!(steps.len(), 94);
    steps.extend(phase(command, true));
    assert_eq!(steps.len(), 105);
    steps.extend(phase(command, true));
    steps.extend(paid(command));
    assert_eq!(steps.len(), 138);
    fixture.scripted_gh(&steps);
    for uid in [42, 43] {
        Cache::open(&cache_path(&fixture))
            .unwrap()
            .put_draft(
                &Scope::github("github.invalid", Some(uid)).unwrap(),
                "legacy-v96-unrelated",
                &format!("FOREIGN_DRAFT_KEPT_{uid}"),
            )
            .unwrap();
        Cache::open(&cache_path(&fixture))
            .unwrap()
            .put(
                &Scope::github("github.invalid", Some(uid)).unwrap(),
                &model::storage::Record::new(
                    "legacy-v96-unrelated",
                    format!("record-{uid}"),
                    REPO,
                    json!({"private":format!("FOREIGN_RECORD_KEPT_{uid}")}),
                ),
            )
            .unwrap();
    }
    let foreign_before = foreign(&fixture);
    assert_eq!(
        foreign_before.len(),
        4,
        "Two actual drafts and two actual record sentinels"
    );
    for uid in [42, 43] {
        assert!(foreign_before
            .iter()
            .any(|row| row.starts_with("drafts:")
                && row.contains(&format!("FOREIGN_DRAFT_KEPT_{uid}"))));
        assert!(foreign_before.iter().any(|row| row.starts_with("records:")
            && row.contains(&format!("FOREIGN_RECORD_KEPT_{uid}"))));
    }
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let local_config = fs::read(fixture.repo().join(".git/config")).unwrap();
    let raw_head = fs::read(fixture.repo().join(".git/HEAD")).unwrap();
    let all_refs = fixture.git(&["show-ref", "--head"]);
    let mut s = launch(&fixture, mouse, false);
    enter(&mut s, mouse);
    observed_metadata(&mut s, mouse, &original);
    exact(&mut s, &fixture, &steps, 25);
    open_current(&mut s, &fixture, mouse, command, 36, &steps);
    select(&mut s, mouse, command);
    let before_sql = sql(&fixture);
    let before_commands = fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let before = s.completed_cells_end();
    let body =
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), &form_title(command))
            .unwrap();
    assert!(!body.contains("[Confirm action]") && !body.contains("[Submit]"));
    s.send_key(b"\x1b[13;5u");
    ready(
        &mut s,
        before,
        &form_title(command),
        &[
            "Fresh useful exact member Preview required before Confirm",
            "[Preview]",
        ],
    );
    assert_eq!(sql(&fixture),before_sql,"Actual hidden keyboard Confirm guard has ZERO full raw SQL effects for BOTH input journeys");
    assert_eq!(
        fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before_commands
    );
    assert_no_publication(&fixture);
    close(&mut s, mouse, command, false);
    preserve_git(&fixture, &head, &index, &status);
    let before = s.completed_cursorless_end();
    s.resize(80, 24);
    metadata_frame(&mut s, before);
    open_current(&mut s, &fixture, mouse, command, 47, &steps);
    select(&mut s, mouse, command);
    let saved_before = preview(&mut s, &fixture, mouse, command, 58, &steps);
    let before_close = sql(&fixture);
    close(&mut s, mouse, command, true);
    assert_eq!(sql(&fixture),before_close,"Pure Close preserves entire independently read database, including Saved17/canaries/journalabsence");
    assert_eq!(foreign(&fixture), foreign_before);
    exact(&mut s, &fixture, &steps, 58);
    preserve_git(&fixture, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        local_config
    );
    assert_eq!(
        fs::read(fixture.repo().join(".git/HEAD")).unwrap(),
        raw_head
    );
    assert_eq!(fixture.git(&["show-ref", "--head"]), all_refs);
    // NEW real binary + NEW original source; never restore Preview/approval
    // from the accepted saved version17 envelope or cached Metadata page.
    let mut s = launch(&fixture, mouse, true);
    enter(&mut s, mouse);
    exact(&mut s, &fixture, &steps, 83);
    assert_eq!(saved(&fixture, command), saved_before);
    open_current(&mut s, &fixture, mouse, command, 94, &steps);
    assert_eq!(saved(&fixture, command), saved_before);
    let body =
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), &form_title(command))
            .unwrap();
    assert!(!body.contains("READ-ONLY PREVIEW") && !body.contains("[Confirm action]"));
    select(&mut s, mouse, command);
    let fresh_saved = preview(&mut s, &fixture, mouse, command, 105, &steps);
    assert_eq!(fresh_saved, saved_before);
    assert_eq!(mutation_count(&exact(&mut s, &fixture, &steps, 105)), 0);
    // Pin all independently expected original scalars, effect and actual
    // canonical DB path/devino BEFORE the real Confirm can change anything.
    let expected = expected_seed(&fixture, command);
    assert_eq!(expected.draft_key, fresh_saved.0);
    let before = s.completed_cursorless_end();
    action(
        &mut s,
        mouse,
        &form_title(command),
        "[Confirm action]",
        PREVIEW,
    );
    s.wait_for_command_count(&fixture, 138);
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            crate::support::main_footer_visible(frame)
                && frame
                    .contents()
                    .contains("Confirmed • fresh PR/search observation")
                && !frame.contents().contains("[Confirm action]")
        },
        "Actual completed confirmed fresh Metadata, not merely an old underlay after last child",
        Duration::from_secs(8),
    );
    metadata_frame(&mut s, before);
    receipt(&fixture, command, &fresh_saved.0, expected);
    observed_metadata(&mut s, mouse, &changed);
    let journal = exact(&mut s, &fixture, &steps, 138);
    assert_eq!(mutation_count(&journal), 1);
    assert_eq!(
        journal[116]["args"],
        json!([
            "api",
            "graphql",
            "--hostname",
            "github.invalid",
            "--method",
            "POST",
            "--input",
            "-"
        ])
    );
    assert_eq!(
        journal[116]["stdin_hex"],
        steps[116]
            .stdin
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    );
    let bytes = journal[116]["stdin_hex"]
        .as_str()
        .unwrap()
        .as_bytes()
        .chunks_exact(2)
        .map(|part| u8::from_str_radix(std::str::from_utf8(part).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    let payload: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        payload["variables"]["input"],
        values::mutation_input(command)
    );
    assert_eq!(foreign(&fixture), foreign_before);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    preserve_git(&fixture, &head, &index, &status);
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        local_config
    );
    assert_eq!(
        fs::read(fixture.repo().join(".git/HEAD")).unwrap(),
        raw_head
    );
    assert_eq!(fixture.git(&["show-ref", "--head"]), all_refs);
    s.capture_completed_terminal_v1(
        "legacy-members-v96-confirmed-fresh-metadata-one-write-no-resend",
    );
    s.quit_and_assert_restored_terminal(mouse);
}
