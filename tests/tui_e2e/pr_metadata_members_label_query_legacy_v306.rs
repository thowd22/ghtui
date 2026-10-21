//! Original query-before-original recovery; no newer journey alias or production source.
const CANARY: &str = "LabelQueryCanary unchanged retained canonical discussion body";
const CANARY_PREFIX: &str = "LabelQueryCanary unchanged retained canonical";
const QUERY_TEXT: &str = "LabelQueryCanary type:pr label:\"existing ☃\"";
const SEARCH: &str = "Search • focused view ─";
const SOURCES: &str = "Search sources • explicit read-only hydration";
const REMOVE_FORM: &str = "Remove label • example/repo#7";
const REMOVE_PICKER: &str = "Current PR metadata members · Label";
const SELECTED: &str = "existing ☃ • actual #80 • L_80";
use super::*;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
};

// Independent pure expected-fact codec from the immutable current happy
// fixture, not its journey/test or any production authority constructor.
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
    raw["body"] = json!(CANARY);
    archived_facts(&mut raw);
    raw
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

fn enter_metadata_query(s: &mut Session, mouse: bool) {
    s.wait_for_main_frame();
    s.wait_for_screen("ghtui");
    // This fixture has its own actual PR title and requester. The ancestor
    // navigation helper belongs to the different Native collaboration PR.
    choose(s, mouse, "1 Inbox", b"1");
    s.wait_for_screen("Inbox • focused view");
    choose(s, mouse, "[r Refresh]", b"r");
    s.wait_for_screen("Unchanged title");
    s.wait_for_screen("Fresh • fetched");
    choose(s, mouse, "Unchanged title", b"\r");
    s.wait_for_screen("1 comments");
    let before = s.completed_cells_end();
    choose(s, mouse, "[c Conversation]", b"c");
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > before
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    // Full H belongs to this SAME completed frame's actual
                    // context in the content pane's top border, not the
                    // separately owned Conversation body. At 120 columns the
                    // compact scope elides middle segments but keeps the
                    // host/actor head and the full head SHA tail.
                    && text.lines().any(|line| {
                        line.contains("Inbox • focused view")
                            && line.contains("github.invalid / @requester / ")
                            && line.trim_end().ends_with(&format!(" / {HEAD} ─╮"))
                    })
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains("example/repo#7")
                                && body.contains("@requester PR description")
                                && body.contains(CANARY)
                                && body.contains("@octocat comment #9")
                                && body.contains("Existing starter comment")
                        },
                    )
                    && crate::support::controls::control_painted(frame, "[u Metadata]")
                    && !text.contains("[Esc Cancel]")
                    && !text.contains("Verifying original Conversation") && !text.contains("Loading PR Metadata")
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "NEW completed genuine Conversation before Metadata\n{}",
            s.screen()
        );
    }
    let before = s.completed_cells_end();
    choose(s, mouse, "[u Metadata]", b"u");
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > before
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    && crate::support::controls::control_painted(frame, "[u Metadata]")
                    && crate::support::controls::control_painted(frame, "[p PR actions]")
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains("example/repo#7")
                                && body
                                    .contains("Your repository permissions: read true • write true")
                                && body.contains("Locked conversation: false")
                                && body.contains("Milestone: Milestone 5")
                        },
                    )
                    && !text.contains("[Esc Cancel]")
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW actual Metadata context under original requester42\n{}",
            s.screen()
        );
    }
}

fn remove_phase(p: &Value) -> Vec<GhStep> {
    let mut calls = label_open_v282(p);
    assert_eq!(calls.len(), 11);
    let gql: Value = serde_json::from_slice(&calls[6].stdin).unwrap();
    assert_eq!(gql, json!({"query":LABEL_QUERY,"variables":{"id":PR_NODE}}));
    assert_eq!(
        calls[5].args[1],
        "repos/example/repo/labels?per_page=100&page=1"
    );
    // This existing eligible page is independent of the actual removal
    // selection, which must come from current80, NOT candidate81.
    calls[6].stdout =
        serde_json::to_vec(&values::members(PrMetadataCommand::RemoveLabel, p)).unwrap();
    calls
}

fn paid_remove(changed: &Value) -> Vec<GhStep> {
    let command = PrMetadataCommand::RemoveLabel;
    assert_eq!(
        values::mutation_input(command),
        json!({"pullRequestId":"PR_7","labelIds":[]})
    );
    assert_eq!(changed["labels"], json!([]));
    let mut calls = vec![
        values::graphql(
            model::providers::gh::PR_METADATA_UPDATE_MUTATION,
            json!({"input":{"pullRequestId":PR_NODE,"labelIds":[]}}),
            json!({"data":{"updatePullRequest":{"pullRequest":{"id":PR_NODE}}}}),
        ),
        values::graphql(
            LABEL_QUERY,
            json!({"id":PR_NODE}),
            values::members(command, changed),
        ),
        values::get("repos/example/repo", repo()),
        values::get("repos/example/repo/pulls/7", changed.clone()),
        values::get("repos/example/repo/issues/7", issue_v282(changed)),
        values::get("user", json!({"id":42,"login":"requester"})),
        values::get("user", json!({"id":42,"login":"requester"})),
        values::get("repos/example/repo", repo()),
        values::get("repos/example/repo/pulls/7", changed.clone()),
        values::get("repos/example/repo/issues/7", issue_v282(changed)),
        values::graphql(
            LABEL_QUERY,
            json!({"id":PR_NODE}),
            values::members(command, changed),
        ),
    ];
    for (endpoint, rows) in [
        ("issues/7/comments", json!([starter()])),
        ("pulls/7/commits", json!([])),
        ("pulls/7/files", json!([])),
        ("pulls/7/reviews", json!([])),
        ("issues/7/timeline", json!([])),
    ] {
        let mut read = values::get(
            &format!("repos/{REPO}/{endpoint}?per_page=100"),
            json!([rows]),
        );
        read.args.extend(["--paginate".into(), "--slurp".into()]);
        calls.push(read);
    }
    calls.extend([
        values::get("repos/example/repo/pulls/7", changed.clone()),
        values::get("repos/example/repo/pulls/7", changed.clone()),
        values::get("repos/example/repo/issues/7", issue_v282(changed)),
        values::get("repos/example/repo", repo()),
        values::get("repos/example/repo/pulls/7", changed.clone()),
        values::get("user", json!({"id":42,"login":"requester"})),
    ]);
    assert_eq!(calls.len(), 22);
    calls
}

fn hydration(p: &Value) -> Vec<GhStep> {
    let mut calls = crate::hydration_fixture::pr_load(p, vec![], vec![], vec![], 42);
    for call in &mut calls {
        if call.args == ["--version"] {
            call.stdout = b"gh version 2.99.0 (2026-10-01)\n".to_vec();
        } else if call.args.get(1).is_some_and(|s| s == "user") {
            call.stdout = serde_json::to_vec(&json!({"id":42,"login":"requester"})).unwrap();
        } else if call.args.get(1).is_some_and(|s| s == "repos/example/repo") {
            call.stdout = serde_json::to_vec(&repo()).unwrap();
        } else if call.args.get(1).is_some_and(|s| s == "graphql") {
            let mut input: Value = serde_json::from_slice(&call.stdin).unwrap();
            input["variables"]["id"] = json!(PR_NODE);
            call.stdin = serde_json::to_vec(&input).unwrap();
            let mut output: Value = serde_json::from_slice(&call.stdout).unwrap();
            output["data"]["node"]["id"] = json!(PR_NODE);
            call.stdout = serde_json::to_vec(&output).unwrap();
        }
    }
    assert_eq!(calls.len(), 29);
    calls
}

fn cached_query(success: bool) -> Vec<GhStep> {
    let mut calls = principal_v282();
    calls.push(values::get("repos/example/repo", repo()));
    if success {
        calls.push(values::get("repos/example/repo", repo()));
    }
    calls.extend(principal_v282());
    assert_eq!(calls.len(), if success { 6 } else { 5 });
    calls
}

fn published(s: &mut Session, f: &Fixture, steps: &[GhStep], count: usize) -> Vec<Value> {
    s.wait_for_command_count(f, count);
    let until = Instant::now() + Duration::from_secs(8);
    let rows = loop {
        let raw = s.inspect_command_journal(f);
        let prefix = raw.rfind('\n').map_or("", |at| &raw[..=at]);
        let rows = prefix
            .lines()
            .map(|row| serde_json::from_str::<Value>(row).unwrap())
            .collect::<Vec<_>>();
        assert!(rows.len() <= count);
        if rows.len() == count {
            break rows;
        }
        assert!(Instant::now() < until, "Actual complete strict125 journal");
        s.poll_output(Duration::from_millis(1));
    };
    for (i, (actual, wanted)) in rows.iter().zip(&steps[..count]).enumerate() {
        assert_eq!(actual["step"], i);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(wanted.args));
        assert_eq!(
            actual["stdin_hex"],
            wanted
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        assert!(actual["pid"].as_u64().is_some_and(|pid| pid > 0));
    }
    assert_eq!(mutation_count(&rows), usize::from(count >= 114));
    rows
}

fn action(s: &mut Session, mouse: bool, title: &str, label: &str, controls: &[&str]) {
    if !mouse {
        let notices = [
            format!("{title} · Filter focused"),
            format!("{title} · Rows focused"),
        ];
        let names = notices.each_ref().map(String::as_str);
        s.focus_completed_owned_action_v1(
            title,
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            controls,
            if title == REMOVE_PICKER { &names } else { &[] },
        );
    }
    let frame = s.completed_cells_screen().unwrap();
    let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
        frame,
        title,
        label,
        &[],
    )
    .expect("Full exact own control");
    assert!(mouse || frame.cell(y, x).unwrap().inverse());
    if mouse {
        s.click(x, y)
    } else {
        s.send_key(b"\r")
    };
}

fn initial_search_ready(s: &mut Session, after: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::main_footer_visible(frame)
                    && frame
                        .contents()
                        .contains("Selected local search corpus ready • no clone or fetch")
                    && crate::support::overlay_body(frame, SEARCH).is_some_and(|body| {
                        ["[O Options]", "[/ Query]"]
                            .iter()
                            .all(|needle| body.contains(needle))
                            && !body.contains("[Esc Cancel]")
                    })
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW completed prepared Search corpus before editing source fields\n{}",
            s.screen()
        );
    }
}

fn source_field(s: &mut Session, mouse: bool, label: &str, key: &[u8], value: &str) {
    let before = s.completed_cells_end();
    choose(s, mouse, label, key);
    s.send_key(b"\x01");
    s.paste(value);
    s.send_key(b"\r");
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > before
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, SOURCES).is_some_and(|body| {
                        body.lines().any(|line| {
                            line.trim()
                                .strip_prefix(label)
                                .is_some_and(|tail| tail.trim() == value)
                        }) && body.contains("[l Load source]")
                            && body.contains("[Esc Close]")
                            && !body.contains("Loading source")
                    })
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW owned source editor must show exact {label} = {value:?} before Load\n{}",
            s.screen()
        );
    }
}

fn load_source(s: &mut Session, f: &Fixture, mouse: bool, steps: &[GhStep]) {
    let before = s.completed_cells_end();
    choose(s, mouse, "[H Sources]", b"H");
    ready_v283(
        s,
        before,
        SOURCES,
        &["[p PR]", "[l Load source]", "[Esc Close]"],
    );
    choose(s, mouse, "[p PR]", b"p");
    source_field(s, mouse, "[r Repository]", b"r", REPO);
    source_field(s, mouse, "[v PR/issue number]", b"v", "7");
    published(s, f, steps, 25);
    let before = s.completed_cells_end();
    choose(s, mouse, "[l Load source]", b"l");
    s.wait_for_command_count(f, 54);
    ready_v283(
        s,
        before,
        SOURCES,
        &["Loaded 1 cached discussion records", "[Esc Close]"],
    );
    published(s, f, steps, 54);
    let before = s.completed_cells_end();
    choose(s, mouse, "[Esc Close]", b"\x1b");
    ready_v283(s, before, SEARCH, &["h GitHub", "e Exact", "[/ Query]"]);
}

fn query(s: &mut Session, mouse: bool) -> usize {
    let before = s.completed_cells_end();
    choose(s, mouse, "[/ Query]", b"/");
    ready_v283(s, before, SEARCH, &["[/ Query]", "e Exact", "h GitHub"]);
    s.send_key(b"\x01");
    let before = s.completed_cells_end();
    s.paste(QUERY_TEXT);
    ready_v283(s, before, SEARCH, &[QUERY_TEXT, "e Exact"]);
    let before = s.completed_cells_end();
    choose(s, mouse, "[Enter Search]", b"\r");
    before
}

type Sql = rusqlite::types::Value;
#[derive(Clone, Debug, PartialEq)]
struct DatabaseImage {
    schema: Vec<(String, String, String)>,
    tables: BTreeMap<String, Vec<Vec<Sql>>>,
}
fn sorted(rows: &mut [Vec<Sql>]) {
    rows.sort_by_key(|row| format!("{row:?}"));
}
fn image(f: &Fixture) -> DatabaseImage {
    let db = Connection::open_with_flags(cache_path(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    let schema = db
        .prepare("SELECT type,name,COALESCE(sql,'') FROM sqlite_master ORDER BY type,name")
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut tables = BTreeMap::new();
    for (kind, name, _) in &schema {
        if kind != "table" {
            continue;
        }
        let mut st = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let cols = st.column_count();
        let mut rows = st
            .query_map([], |r| {
                (0..cols)
                    .map(|i| r.get::<_, Sql>(i))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        sorted(&mut rows);
        tables.insert(name.clone(), rows);
    }
    DatabaseImage { schema, tables }
}
fn source_keys() -> BTreeMap<String, String> {
    [
        "conversation_comment",
        "inline_comment",
        "pull_request_body",
        "review",
    ]
    .into_iter()
    .map(|kind| {
        // ParentKind uses its explicit PascalCase wire variant, while the
        // category contract uses snake_case. Keep this independent literal
        // codec; do not ask production or a POST image for expected keys.
        let key = format!(
            "collaboration-v1:{}",
            hash(&(scope(), 410u64, REPO, "PullRequest", PR_NODE, kind))
        );
        (kind.into(), key)
    })
    .collect()
}
fn assert_bindings(db: &DatabaseImage, visible: bool) {
    let keys = source_keys();
    let rows = &db.tables["sync_resource_bindings"];
    for (kind, key) in keys {
        let selected = rows
            .iter()
            .filter(|row| {
                row[0] == Sql::Text(HOST.into())
                    && row[1] == Sql::Text("user-id:42".into())
                    && row[2] == Sql::Text(key.clone())
            })
            .collect::<Vec<_>>();
        assert_eq!(selected.len(), 1);
        let row = selected[0];
        assert_eq!(
            &row[3..8],
            &[
                Sql::Text("collaboration".into()),
                Sql::Integer(410),
                Sql::Text(REPO.into()),
                Sql::Text(PR_NODE.into()),
                Sql::Text(kind)
            ]
        );
        assert!(matches!(row[8],Sql::Integer(n)if n>0));
        if visible {
            assert_eq!(row[9], row[8]);
        } else {
            assert_eq!(row[9], Sql::Null);
            assert_eq!(row[10], Sql::Integer(0));
        }
    }
}
fn repaired(mut db: DatabaseImage) -> DatabaseImage {
    assert_bindings(&db, true);
    let keys = source_keys();
    let fences = db.tables.get_mut("sync_mutation_fences").unwrap();
    for kind in keys.keys() {
        let prefix = [
            Sql::Text(HOST.into()),
            Sql::Text("user-id:42".into()),
            Sql::Text("collaboration".into()),
            Sql::Text(REPO.into()),
            Sql::Text(PR_NODE.into()),
            Sql::Text(kind.clone()),
        ];
        assert!(!fences.iter().any(|row| row[..6] == prefix));
        let mut row = prefix.to_vec();
        row.push(Sql::Integer(1));
        fences.push(row);
    }
    sorted(fences);
    let mut reservation_keys = Vec::new();
    for row in db.tables.get_mut("sync_reservations").unwrap() {
        assert_eq!(row.len(), 5);
        if row[0] == Sql::Text(HOST.into()) && row[1] == Sql::Text("user-id:42".into()) {
            let Sql::Integer(old) = &row[3] else {
                panic!("Actual typed generation")
            };
            let old = *old;
            assert!(old > 0);
            reservation_keys.push(row[2].clone());
            row[3] = Sql::Integer(old.saturating_add(1));
            row[4] = Sql::Integer(0);
        }
    }
    assert!(
        reservation_keys.len() >= 5,
        "All4 hydrated categories plus actual Personal source are enumerated before repair"
    );
    let mut target_sources = Vec::new();
    for row in db.tables.get_mut("sync_resource_bindings").unwrap() {
        assert_eq!(row.len(), 11);
        if row[0] == Sql::Text(HOST.into()) && row[1] == Sql::Text("user-id:42".into()) {
            row[10] = Sql::Integer(0);
            if row[3] == Sql::Text("collaboration".into())
                && row[4] == Sql::Integer(410)
                && row[5] == Sql::Text(REPO.into())
                && row[6] == Sql::Text(PR_NODE.into())
            {
                let Sql::Text(kind) = &row[7] else {
                    panic!("Actual typed category")
                };
                let key = keys.get(kind).expect("Only four exact PR categories");
                assert_eq!(row[2], Sql::Text(key.clone()));
                target_sources.push(key.clone());
                row[9] = Sql::Null;
            }
        }
    }
    target_sources.sort();
    assert_eq!(
        target_sources,
        keys.into_values()
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    );
    // The established lookup triggers are part of the exact repair effect:
    // one fence INSERT per category, OLD+NEW on each bound reservation UPDATE,
    // one scope-wide active UPDATE per binding, and one visibility UPDATE per
    // selected source. Derive counts ONLY from this PRE image, never POST SQL.
    let bindings = db.tables["sync_resource_bindings"]
        .iter()
        .filter(|row| row[0] == Sql::Text(HOST.into()) && row[1] == Sql::Text("user-id:42".into()))
        .collect::<Vec<_>>();
    assert_eq!(bindings.len(), 4);
    assert!(bindings.iter().all(|row| {
        row[3] == Sql::Text("collaboration".into())
            && row[5] == Sql::Text(REPO.into())
            && target_sources
                .iter()
                .any(|source| row[2] == Sql::Text(source.clone()))
    }));
    let reserved_bindings = bindings
        .iter()
        .filter(|row| reservation_keys.contains(&row[2]))
        .count();
    assert_eq!(reserved_bindings, 4);
    let bumps =
        i64::try_from(4 + 2 * reserved_bindings + bindings.len() + target_sources.len()).unwrap();
    let lookup_key = [
        Sql::Text(HOST.into()),
        Sql::Text("user-id:42".into()),
        Sql::Text("collaboration".into()),
        Sql::Text(REPO.into()),
    ];
    let epochs = db
        .tables
        .get_mut("sync_binding_lookup_epochs")
        .unwrap()
        .iter_mut()
        .filter(|row| row[..4] == lookup_key)
        .collect::<Vec<_>>();
    assert_eq!(epochs.len(), 1);
    let epoch = epochs.into_iter().next().unwrap();
    assert_eq!(epoch.len(), 5);
    let Sql::Integer(old_epoch) = epoch[4] else {
        panic!("Exact typed PRE lookup epoch");
    };
    epoch[4] = Sql::Integer(old_epoch.checked_add(bumps).unwrap());
    for rows in db.tables.values_mut() {
        sorted(rows);
    }
    db
}

#[derive(Serialize)]
struct Reference {
    attempt_id: i64,
    pr_node_id: String,
    observed_head: String,
    payload_sha256: String,
}
#[derive(Serialize)]
struct Record<'a> {
    seed: &'a Seed,
    reference: &'a Reference,
}
#[derive(Serialize)]
struct Draft<'a> {
    version: u8,
    values: BTreeMap<&'a str, &'a str>,
    publication: &'a Reference,
}

fn seconds() -> i64 {
    i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

fn confirmed_image(
    mut before: DatabaseImage,
    seed: &Seed,
    created: i64,
    saved: i64,
) -> DatabaseImage {
    let payload = model::pr_write_journal::PrWriteIntent {
        command: "metadata-remove-label".into(),
        selected_resource: seed.draft_key.clone(),
        observed_head: HEAD.into(),
        values: BTreeMap::from([
            ("body".into(), SELECTED.into()),
            ("origin-members-v48".into(), hash(seed)),
            (
                "requested-plan-v48".into(),
                seed.requested_plan_sha256.clone(),
            ),
        ]),
    }
    .digest()
    .unwrap();
    let reference = Reference {
        attempt_id: 1,
        pr_node_id: PR_NODE.into(),
        observed_head: HEAD.into(),
        payload_sha256: payload.clone(),
    };
    let attempts = before.tables.get_mut("pr_write_attempts").unwrap();
    assert!(attempts.is_empty());
    attempts.push(vec![
        Sql::Integer(1),
        Sql::Text(HOST.into()),
        Sql::Text("user-id:42".into()),
        Sql::Text(PR_NODE.into()),
        Sql::Text(REPO.into()),
        Sql::Integer(7),
        Sql::Text("metadata-remove-label".into()),
        Sql::Text(seed.draft_key.clone()),
        Sql::Text(HEAD.into()),
        Sql::Text(payload),
        Sql::Text("confirmed".into()),
        Sql::Text(serde_json::to_string(&seed.expected_effect).unwrap()),
        Sql::Integer(created),
    ]);
    let origins = before
        .tables
        .get_mut("pr_write_metadata_origins_v25")
        .unwrap();
    assert!(origins.is_empty());
    origins.push(vec![
        Sql::Integer(1),
        Sql::Integer(410),
        Sql::Integer(1007),
        Sql::Text(
            serde_json::to_string(&Record {
                seed,
                reference: &reference,
            })
            .unwrap(),
        ),
        Sql::Text(seed.base_key.clone()),
        Sql::Text("metadata-remove-label".into()),
        Sql::Null,
    ]);
    let scopes = before.tables.get_mut("pr_write_scopes").unwrap();
    let prefix = [
        Sql::Text(HOST.into()),
        Sql::Text("user-id:42".into()),
        Sql::Text(PR_NODE.into()),
    ];
    let found = scopes
        .iter()
        .enumerate()
        .filter(|(_, row)| row[..3] == prefix)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert!(found.len() <= 1);
    if let Some(&at) = found.first() {
        assert_eq!(scopes[at][3], Sql::Integer(0));
        scopes[at][3] = Sql::Integer(3);
    } else {
        let mut row = prefix.to_vec();
        row.push(Sql::Integer(3));
        scopes.push(row);
    }
    let drafts = before.tables.get_mut("drafts").unwrap();
    let at = drafts
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row[0] == Sql::Text(HOST.into())
                && row[1] == Sql::Text("user-id:42".into())
                && row[2] == Sql::Text(seed.draft_key.clone())
        })
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert_eq!(at.len(), 1);
    let Sql::Text(inert) = &drafts[at[0]][3] else {
        panic!("Actual selected inert draft")
    };
    assert_eq!(
        serde_json::from_str::<Value>(inert).unwrap(),
        json!({"version":17,"values":{"body":SELECTED},"publication":null})
    );
    drafts[at[0]][3] = Sql::Text(
        serde_json::to_string(&Draft {
            version: 17,
            values: BTreeMap::from([("body", SELECTED)]),
            publication: &reference,
        })
        .unwrap(),
    );
    drafts[at[0]][4] = Sql::Integer(saved);
    let sequences = before.tables.get_mut("sqlite_sequence").unwrap();
    let at = sequences
        .iter()
        .enumerate()
        .filter(|(_, row)| row[0] == Sql::Text("pr_write_attempts".into()))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert!(at.len() <= 1);
    if let Some(&index) = at.first() {
        assert_eq!(sequences[index][1], Sql::Integer(0));
        sequences[index][1] = Sql::Integer(1);
    } else {
        sequences.push(vec![Sql::Text("pr_write_attempts".into()), Sql::Integer(1)]);
    }
    for rows in before.tables.values_mut() {
        sorted(rows);
    }
    before
}
fn protected(f: &Fixture) -> Vec<String> {
    let expected = expected_seed(f, PrMetadataCommand::RemoveLabel);
    assert_eq!(expected.selected_id, 80);
    assert_eq!(expected.selected_node, "L_80");
    let digest = model::pr_write_journal::PrWriteIntent {
        command: "metadata-remove-label".into(),
        selected_resource: expected.draft_key.clone(),
        observed_head: HEAD.into(),
        values: BTreeMap::from([
            ("body".into(), SELECTED.into()),
            ("origin-members-v48".into(), hash(&expected)),
            (
                "requested-plan-v48".into(),
                expected.requested_plan_sha256.clone(),
            ),
        ]),
    }
    .digest()
    .unwrap();
    let reference = Reference {
        attempt_id: 1,
        pr_node_id: PR_NODE.into(),
        observed_head: HEAD.into(),
        payload_sha256: digest.clone(),
    };
    let db = Connection::open_with_flags(cache_path(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    let got:Vec<Sql>=db.query_row("SELECT attempt_id,host,principal,pr_node,repository,command,pr_number,selected_resource,observed_head,payload_sha256,state,confirmed_json FROM pr_write_attempts",[],|r|(0..12).map(|i|r.get(i)).collect()).unwrap();
    assert_eq!(
        got,
        vec![
            Sql::Integer(1),
            Sql::Text(HOST.into()),
            Sql::Text("user-id:42".into()),
            Sql::Text(PR_NODE.into()),
            Sql::Text(REPO.into()),
            Sql::Text("metadata-remove-label".into()),
            Sql::Integer(7),
            Sql::Text(expected.draft_key.clone()),
            Sql::Text(HEAD.into()),
            Sql::Text(digest),
            Sql::Text("confirmed".into()),
            Sql::Text(serde_json::to_string(&expected.expected_effect).unwrap())
        ]
    );
    let receipt = PrMetadataReceipt::decode(&expected.expected_effect).unwrap();
    assert_eq!(receipt.command(), PrMetadataCommand::RemoveLabel);
    assert_eq!(receipt.selected_id(), Some(80));
    assert!(receipt.result_ids().is_empty());
    let (repo_id,pr_id,origin,base,tag,ack):(u64,u64,String,String,String,Option<i64>)=db.query_row("SELECT repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48 FROM pr_write_metadata_origins_v25 WHERE attempt_id=1",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
    assert_eq!(
        (repo_id, pr_id, base, tag, ack),
        (
            410,
            1007,
            expected.base_key.clone(),
            "metadata-remove-label".into(),
            None
        )
    );
    assert_eq!(
        origin,
        serde_json::to_string(&Record {
            seed: &expected,
            reference: &reference
        })
        .unwrap()
    );
    let body = serde_json::to_string(&Draft {
        version: 17,
        values: BTreeMap::from([("body", SELECTED)]),
        publication: &reference,
    })
    .unwrap();
    assert_eq!(saved_v283(f, &expected.draft_key), Some(body));
    assert_eq!(db.query_row("SELECT generation FROM pr_write_scopes WHERE host='github.invalid' AND principal='user-id:42' AND pr_node='PR_7'",[],|r|r.get::<_,i64>(0)).unwrap(),3);
    let marker = format!(
        "drafts:[Text(\"{HOST}\"), Text(\"user-id:42\"), Text(\"{}\"),",
        expected.draft_key
    );
    whole_sql_v282(f)
        .into_iter()
        .filter(|row| row.starts_with("pr_write_") || row.starts_with(&marker))
        .collect()
}

pub(in crate::pr_actions::pr_metadata::metadata_actions) fn label_query(mouse: bool) {
    let f = fixture();
    let p = raw(None);
    let changed = raw(Some(PrMetadataCommand::RemoveLabel));
    assert_eq!(p["body"], CANARY);
    assert_eq!(changed["body"], CANARY);
    assert_eq!(changed["labels"], json!([]));
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut steps = startup_v282(&p);
    steps.extend(hydration(&p));
    assert_eq!(steps.len(), 54);
    steps.extend(cached_query(true));
    assert_eq!(steps.len(), 60);
    steps.extend(remove_phase(&p));
    assert_eq!(steps.len(), 71);
    steps.extend(remove_phase(&p));
    assert_eq!(steps.len(), 82);
    steps.extend(remove_phase(&p));
    steps.extend(paid_remove(&changed));
    assert_eq!(steps.len(), 115);
    steps.extend(cached_query(false));
    assert_eq!(steps.len(), 120);
    steps.extend(cached_query(true));
    assert_eq!(steps.len(), 126);
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata_query(&mut s, mouse);
    let before = s.completed_cells_end();
    choose(&mut s, mouse, "3 Search", b"3");
    // Search controls are also painted in its loading shell. Source field
    // actions intentionally refuse then; wait for the actual prepared corpus.
    initial_search_ready(&mut s, before);
    // TASK-121: the mode tabs wait under Options; this journey keeps them shown and Exact.
    let before = s.completed_cells_end();
    choose(&mut s, mouse, "[O Options]", b"O");
    ready_v283(
        &mut s,
        before,
        SEARCH,
        &["[O Hide options]", "h GitHub", "e Exact"],
    );
    choose(&mut s, mouse, "e Exact", b"e");
    published(&mut s, &f, &steps, 25);
    load_source(&mut s, &f, mouse, &steps);
    let before = query(&mut s, mouse);
    s.wait_for_command_count(&f, 60);
    ready_v283(
        &mut s,
        before,
        SEARCH,
        &[
            "1 results • Exact",
            CANARY_PREFIX,
            "Unchanged title",
            "[Ctrl↓ More]",
        ],
    );
    // At120 columns the split preview intentionally clips the60-character
    // body. Prove the ENTIRE unchanged literal in actual wider rendered cells,
    // then restore120; neither resizing nor painting may issue reads or SQL.
    let before_resize = image(&f);
    let wide = s.completed_cells_end();
    s.resize(200, 40);
    ready_v283(&mut s, wide, SEARCH, &["1 results • Exact", CANARY]);
    assert_eq!(s.completed_cells_screen().unwrap().size(), (40, 200));
    s.capture_completed_terminal_v1("task79-label-query-complete-body");
    let standard = s.completed_cells_end();
    s.resize(120, 40);
    ready_v283(
        &mut s,
        standard,
        SEARCH,
        &[
            "1 results • Exact",
            CANARY_PREFIX,
            "Unchanged title",
            "[Ctrl↓ More]",
        ],
    );
    assert_eq!(s.completed_cells_screen().unwrap().size(), (40, 120));
    assert_eq!(
        image(&f),
        before_resize,
        "Read-only viewport changes preserve all SQL"
    );
    published(&mut s, &f, &steps, 60);
    let hydrated = image(&f);
    assert_bindings(&hydrated, true);
    let records = hydrated.tables["records"].clone();
    assert!(records.iter().any(|row|row[2]==Sql::Text("collaboration-document-v1".into())&&matches!(&row[5],Sql::Text(payload)if serde_json::from_str::<Value>(payload).unwrap()["body"]==CANARY)));
    let before = s.completed_cells_end();
    choose(&mut s, mouse, "1 Inbox", b"1");
    ready_v283(
        &mut s,
        before,
        "Inbox • focused view ─",
        &[
            "[u Metadata]",
            "[p PR actions]",
            "Locked conversation: false",
            "Label: existing ☃",
        ],
    );
    published(&mut s, &f, &steps, 60);
    let before = s.completed_cells_end();
    menu(&mut s, mouse, "Remove label");
    s.wait_for_command_count(&f, 71);
    ready_v283(&mut s, before, REMOVE_FORM, SELECT_CONTROLS);
    published(&mut s, &f, &steps, 71);
    let before_select = image(&f);
    let before = s.completed_cells_end();
    action(
        &mut s,
        mouse,
        REMOVE_FORM,
        "[Choose member]",
        SELECT_CONTROLS,
    );
    ready_v283(
        &mut s,
        before,
        REMOVE_PICKER,
        &[
            "◆ current · Current label: existing ☃ (#80)",
            "[Enter Choose]",
            "[Esc Back]",
        ],
    );
    if mouse {
        let frame = s.completed_cells_screen().unwrap();
        let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
            frame,
            REMOVE_PICKER,
            "◆ current · Current label: existing ☃ (#80)",
            &[],
        )
        .unwrap();
        let before = s.completed_cells_end();
        s.click(x, y);
        ready_v283(
            &mut s,
            before,
            &format!("{REMOVE_PICKER} · Rows focused"),
            &["[Enter Choose]", "Current label: existing ☃ (#80)"],
        );
    }
    let before = s.completed_cells_end();
    action(
        &mut s,
        mouse,
        REMOVE_PICKER,
        "[Enter Choose]",
        PICKER_CONTROLS,
    );
    ready_v283(&mut s, before, REMOVE_FORM, &[SELECTED, "[Preview]"]);
    assert_eq!(
        image(&f),
        before_select,
        "Choose alone is not journal/save/write authority"
    );
    let before = s.completed_cells_end();
    action(&mut s, mouse, REMOVE_FORM, "[Preview]", SELECT_CONTROLS);
    s.wait_for_command_count(&f, 82);
    ready_v283(
        &mut s,
        before,
        REMOVE_FORM,
        &[
            "READ-ONLY PREVIEW",
            "Exact effect: Remove label",
            "[Confirm action]",
        ],
    );
    let (_, key) = expected_keys(PrMetadataCommand::RemoveLabel);
    let blank = json!({"version":17,"values":{"body":SELECTED},"publication":null});
    s.try_wait_for_condition(
        |_| {
            saved_v283(&f, &key)
                .is_some_and(|body| serde_json::from_str::<Value>(&body).unwrap() == blank)
        },
        "Actual selected80 inert v17 autosave settles BEFORE Confirm",
        Duration::from_secs(8),
    )
    .unwrap();
    published(&mut s, &f, &steps, 82);
    Connection::open(cache_path(&f)).unwrap().execute_batch("CREATE TRIGGER metadata_label_fence_fails BEFORE INSERT ON sync_mutation_fences BEGIN SELECT RAISE(ABORT,'fixture actual label visibility failure'); END;").unwrap();
    let before_confirm = image(&f);
    assert_bindings(&before_confirm, true);
    let clock_before = seconds();
    let before = s.completed_cells_end();
    action(
        &mut s,
        mouse,
        REMOVE_FORM,
        "[Confirm action]",
        PREVIEW_CONTROLS,
    );
    s.wait_for_command_count(&f, 115);
    let safe = format!(
        "Actual safe effect: {}",
        expected_seed(&f, PrMetadataCommand::RemoveLabel)
            .expected_effect
            .resource_id
    );
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > before
            && s.completed_cells_screen().is_some_and(|frame| {
                let (rows, cols) = frame.size();
                crate::support::main_footer_visible(frame)
                    && frame
                        .contents_between(rows - 2, 0, rows - 2, cols)
                        .trim_end()
                        == "Confirmed; visibility unavailable; no resend • ref#1"
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains("Original Conversation retired")
                                && crate::owned_form_prose_v120::adjacent_literal(&body, &safe)
                        },
                    )
                    && !frame.contents().contains(CANARY)
                    && !frame.contents().contains(CANARY_PREFIX)
                    && ![
                        "[Confirm action]",
                        "[Choose member]",
                        "[Keep draft / Close]",
                        "[Esc Cancel]",
                        "Unchanged title",
                    ]
                    .iter()
                    .any(|private| frame.contents().contains(private))
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual fixed confirmed-but-hidden retirement; no invented Return/Form\n{}",
            s.screen()
        );
    }
    published(&mut s, &f, &steps, 115);
    let original = protected(&f);
    let refused = image(&f);
    let clock_after = seconds();
    assert!((0..=30).contains(&(clock_after - clock_before)));
    let seed = expected_seed(&f, PrMetadataCommand::RemoveLabel);
    assert!((clock_before..=clock_after).any(|created| (created..=clock_after).any(|saved|
        confirmed_image(before_confirm.clone(), &seed, created, saved)==refused)),
        "Whole preConfirm DB changes ONLY independently encoded attempt1/claimed-origin/confirmed-gen3/protected draft+reference/sequence; both own timestamps must be within the actual Confirm phase, no POSTstate copied as expectation");
    assert_eq!(refused.tables["records"], records);
    assert_eq!(
        refused.tables["sync_reservations"],
        before_confirm.tables["sync_reservations"]
    );
    assert_eq!(
        refused.tables["sync_resource_bindings"],
        before_confirm.tables["sync_resource_bindings"]
    );
    assert_eq!(
        refused.tables["sync_mutation_fences"],
        before_confirm.tables["sync_mutation_fences"]
    );
    let before = s.completed_cells_end();
    choose(&mut s, mouse, "3 Search", b"3");
    ready_v283(&mut s, before, SEARCH, &["h GitHub", "e Exact"]);
    published(&mut s, &f, &steps, 115);
    let before = query(&mut s, mouse);
    s.wait_for_command_count(&f, 120);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > before
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, SEARCH).is_some_and(|body| {
                        body.contains("h GitHub")
                            && !body.contains("1 results • Exact")
                            && !body.contains(CANARY)
                            && !body.contains(CANARY_PREFIX)
                            && !body.contains("Unchanged title")
                    })
                    && text
                        .lines()
                        .any(|row| row.starts_with("Search failed • prior results hidden:"))
            })
        {
            break;
        }
        assert!(Instant::now()<until,"Actual fresh failed query hides prior body/results, not an empty-success substitute\n{}",s.screen());
    }
    published(&mut s, &f, &steps, 120);
    assert_eq!(image(&f),refused,"Actual failed repair rolls back ALL SQL/schema/sequence and keeps bindings/raw bodies/receipt exact");
    assert_eq!(protected(&f), original);
    Connection::open(cache_path(&f))
        .unwrap()
        .execute_batch("DROP TRIGGER metadata_label_fence_fails")
        .unwrap();
    let before_repair = image(&f);
    let expected = repaired(before_repair.clone());
    let before = query(&mut s, mouse);
    s.wait_for_command_count(&f, 126);
    ready_v283(
        &mut s,
        before,
        SEARCH,
        &["0 results • Exact", "PARTIAL", "h GitHub"],
    );
    let commands = published(&mut s, &f, &steps, 126);
    let after = image(&f);
    assert_eq!(after,expected,"Exact4 ordinal rows +each enumerated PREscope reservation/binding fence ONLY; no table filtering or POSTstate accepted as expected");
    assert_bindings(&after, false);
    assert_eq!(after.tables["records"], records);
    assert_eq!(protected(&f), original);
    preserve_git(&f, &head, &index, &status);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(image(&f), expected);
    assert_eq!(published(&mut s, &f, &steps, 126), commands);
    assert_eq!(protected(&f), original);
    preserve_git(&f, &head, &index, &status);
}
