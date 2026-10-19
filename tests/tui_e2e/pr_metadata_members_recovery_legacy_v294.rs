//! Original recovery journeys on the actual current Members protocol. These
//! helpers drive real Sessions; they construct no production Source or receipt.
use super::*;
use serde::Serialize;
use std::{
    collections::BTreeMap,
    os::unix::{ffi::OsStrExt, fs::MetadataExt},
};

const SELECTED_LABEL: &str = "new 🐛 • actual #81 • L_81";
const CAPABILITY_REFUSAL: &str =
    "Actual capability refused • nothing sent • original draft retained";

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

fn select_label(s: &mut Session, mouse: bool) {
    let after = s.completed_cells_end();
    control_v283(s, mouse, LABEL_FORM, "[Choose member]", SELECT_CONTROLS);
    ready_v283(
        s,
        after,
        LABEL_PICKER,
        &["◆ eligible · Label: new 🐛", "[Enter Choose]", "[Esc Back]"],
    );
    if mouse {
        let frame = s.completed_cells_screen().unwrap();
        let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
            frame,
            LABEL_PICKER,
            "◆ eligible · Label: new 🐛",
            &[],
        )
        .expect("Exact eligible original Label81 row, not its detail copy");
        let after = s.completed_cells_end();
        s.click(x, y);
        ready_v283(
            s,
            after,
            &format!("{LABEL_PICKER} · Rows focused"),
            &["Filter:", "◆ eligible · Label: new 🐛", "[Enter Choose]"],
        );
    }
    let after = s.completed_cells_end();
    control_v283(s, mouse, LABEL_PICKER, "[Enter Choose]", PICKER_CONTROLS);
    ready_v283(
        s,
        after,
        LABEL_FORM,
        &[SELECTED_LABEL, "[Preview]", "[Esc Close]"],
    );
}

fn actual_label_preview(s: &mut Session, f: &Fixture, mouse: bool, steps: &[GhStep]) -> String {
    let after = s.completed_cells_end();
    menu(s, mouse, "Add label");
    s.wait_for_command_count(f, 35);
    ready_v283(s, after, LABEL_FORM, SELECT_CONTROLS);
    exact_v282(s, f, steps, 35);
    let before_select = whole_sql_v282(f);
    select_label(s, mouse);
    assert_eq!(
        whole_sql_v282(f),
        before_select,
        "A genuine row choice alone saves/publishes nothing"
    );
    let key = selected_draft_key_v283();
    assert_eq!(saved_v283(f, &key), None);
    let untouched = unrelated_sql_v283(f, &key);
    let after = s.completed_cells_end();
    control_v283(s, mouse, LABEL_FORM, "[Preview]", SELECT_CONTROLS);
    s.wait_for_command_count(f, 46);
    ready_v283(
        s,
        after,
        LABEL_FORM,
        &[
            "READ-ONLY PREVIEW",
            "Exact effect: Add label",
            "L_81",
            "[Confirm action]",
        ],
    );
    let expected = json!({"version":17,"values":{"body":SELECTED_LABEL},"publication":null});
    s.try_wait_for_condition(
        |_| {
            saved_v283(f, &key)
                .is_some_and(|body| serde_json::from_str::<Value>(&body).unwrap() == expected)
        },
        "Actual exact inert selected Label81 v17 autosave before Confirm",
        Duration::from_secs(8),
    )
    .unwrap();
    assert_eq!(unrelated_sql_v283(f, &key), untouched);
    exact_v282(s, f, steps, 46);
    saved_v283(f, &key).unwrap()
}

fn capability_retired(s: &mut Session, after: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                let (rows, columns) = frame.size();
                crate::support::main_footer_visible(frame)
                    && frame
                        .contents_between(
                            rows.saturating_sub(2),
                            0,
                            rows.saturating_sub(2),
                            columns,
                        )
                        .trim_end()
                        == CAPABILITY_REFUSAL
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            crate::owned_form_prose_v120::adjacent_literal(&body,
                            "Original Conversation retired • refresh explicitly • nothing resent")
                        },
                    )
                    && ![
                        "[Choose member]",
                        "[Enter Choose]",
                        "[Confirm action]",
                        "[Esc Cancel]",
                        "[Keep draft / Close]",
                        " u Metadata ",
                        "Your repository permissions:",
                        "Selected value:",
                        "Selected actual member:",
                        "Private description",
                        "Unchanged title",
                        "existing ☃",
                        "new 🐛",
                        "current-reviewer",
                        "current-assignee",
                        "current-team",
                        "Milestone 5",
                        REPO,
                        HEAD,
                        PR_NODE,
                        "L_81",
                        "PRIVATE_PREWRITE_403_CANARY",
                    ]
                    .iter()
                    .any(|private| text.contains(private))
            })
        {
            return;
        }
        assert!(Instant::now() < until,
            "NEW same-action capability refusal + full owned private retirement; no empty/loading substitute\n{}", s.screen());
    }
}

fn paid_label(changed: &Value) -> Vec<GhStep> {
    let command = PrMetadataCommand::AddLabel;
    let mut calls = vec![
        values::graphql(
            model::providers::gh::PR_METADATA_UPDATE_MUTATION,
            json!({"input":values::mutation_input(command)}),
            json!({"data":{"updatePullRequest":{"pullRequest":{"id":PR_NODE}}}}),
        ),
        values::graphql(
            LABEL_QUERY,
            json!({"id":PR_NODE}),
            values::members(command, changed),
        ),
        values::get("repos/example/repo", repository_v282()),
        values::get("repos/example/repo/pulls/7", changed.clone()),
        values::get("repos/example/repo/issues/7", issue_v282(changed)),
        values::get("user", json!({"id":42,"login":"requester"})),
        values::get("user", json!({"id":42,"login":"requester"})),
        values::get("repos/example/repo", repository_v282()),
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
    calls.push(values::get("repos/example/repo/pulls/7", changed.clone()));
    calls.extend([
        values::get("repos/example/repo/pulls/7", changed.clone()),
        values::get("repos/example/repo/issues/7", issue_v282(changed)),
        values::get("repos/example/repo", repository_v282()),
        values::get("repos/example/repo/pulls/7", changed.clone()),
        values::get("user", json!({"id":42,"login":"requester"})),
    ]);
    assert_eq!(calls.len(), 22);
    calls
}
fn history_roles(changed: &Value, inspect: bool) -> Vec<GhStep> {
    let mut reads = principal_v282();
    reads.push(values::get("repos/example/repo", repository_v282()));
    reads.push(values::get("repos/example/repo/pulls/7", changed.clone()));
    if inspect {
        reads.push(values::get(
            "repos/example/repo/issues/7",
            issue_v282(changed),
        ));
        reads.push(values::graphql(
            LABEL_QUERY,
            json!({"id":PR_NODE}),
            values::members(PrMetadataCommand::AddLabel, changed),
        ));
    }
    reads.push(values::get("repos/example/repo/pulls/7", changed.clone()));
    reads.push(values::get("repos/example/repo", repository_v282()));
    reads.extend(principal_v282());
    assert_eq!(reads.len(), if inspect { 10 } else { 8 });
    assert!(
        !reads
            .iter()
            .any(|step| step.args.iter().any(|a| a.contains("per_page"))),
        "Actual original history has no eligible catalogue or hidden mutation"
    );
    reads
}
fn exact_published(s: &mut Session, f: &Fixture, expected: &[GhStep], count: usize) -> Vec<Value> {
    s.wait_for_command_count(f, count);
    let until = Instant::now() + Duration::from_secs(8);
    let rows = loop {
        let raw = s.inspect_command_journal(f);
        let prefix = raw.rfind('\n').map_or("", |last| &raw[..=last]);
        let rows = prefix
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert!(rows.len() <= count, "No unplanned later child or resend");
        if rows.len() == count {
            break rows;
        }
        assert!(
            Instant::now() < until,
            "Every strict JSON journal record must complete"
        );
        s.poll_output(Duration::from_millis(1));
    };
    for (i, (actual, expected)) in rows.iter().zip(&expected[..count]).enumerate() {
        assert_eq!(actual["step"], i);
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
    assert_eq!(
        mutation_count(&rows),
        1,
        "One original actual mutation, never resend"
    );
    rows
}
#[derive(Serialize)]
struct Reference {
    attempt_id: i64,
    pr_node_id: String,
    observed_head: String,
    payload_sha256: String,
}
#[derive(Serialize)]
struct ExpectedRecord<'a> {
    seed: &'a Seed,
    reference: &'a Reference,
}
#[derive(Serialize)]
struct Protected<'a> {
    version: u8,
    values: BTreeMap<&'static str, &'static str>,
    publication: &'a Reference,
}
struct OriginalMarkerV304 {
    attempt: i64,
    host: String,
    principal: String,
    pr_node: String,
    repository: String,
    command: String,
    number: u64,
    selected: String,
    head: String,
    payload: String,
    state: String,
    confirmed: Option<String>,
    created_at: u64,
}
fn verify_original(f: &Fixture, state: &str) -> String {
    let expected = expected_seed(f, PrMetadataCommand::AddLabel);
    let key = selected_draft_key_v283();
    assert_eq!(expected.draft_key, key);
    let digest = model::pr_write_journal::PrWriteIntent {
        command: "metadata-add-label".into(),
        selected_resource: key.clone(),
        observed_head: HEAD.into(),
        values: BTreeMap::from([
            ("body".into(), SELECTED_LABEL.into()),
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
    let marker=db.query_row(
        "SELECT attempt_id,host,principal,pr_node,repository,command,pr_number,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts",[],
        |r|Ok(OriginalMarkerV304 {attempt:r.get(0)?,host:r.get(1)?,principal:r.get(2)?,pr_node:r.get(3)?,repository:r.get(4)?,command:r.get(5)?,number:r.get(6)?,selected:r.get(7)?,head:r.get(8)?,payload:r.get(9)?,state:r.get(10)?,confirmed:r.get(11)?,created_at:r.get(12)?})).unwrap();
    assert_eq!(
        (
            marker.attempt,
            marker.host.as_str(),
            marker.principal.as_str(),
            marker.pr_node.as_str(),
            marker.repository.as_str(),
            marker.command.as_str(),
            marker.number
        ),
        (
            1,
            HOST,
            "user-id:42",
            PR_NODE,
            REPO,
            "metadata-add-label",
            7
        )
    );
    assert_eq!(
        (
            &marker.selected,
            &marker.head,
            &marker.payload,
            marker.state.as_str()
        ),
        (&key, &HEAD.to_owned(), &digest, state)
    );
    assert!(
        marker.created_at > 0
            && marker.created_at
                <= std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
    );
    let expected_generation = match state {
        "confirmed" => {
            let receipt = serde_json::to_string(&expected.expected_effect).unwrap();
            assert_eq!(marker.confirmed, Some(receipt));
            let decoded = PrMetadataReceipt::decode(&expected.expected_effect).unwrap();
            assert_eq!(decoded.command(), PrMetadataCommand::AddLabel);
            assert_eq!(decoded.repository_id(), 410);
            assert_eq!(decoded.selected_id(), Some(81));
            assert_eq!(decoded.result_ids(), [(80, None), (81, None)]);
            3
        }
        "pending" => {
            assert_eq!(marker.confirmed, None);
            2
        }
        _ => panic!("Only exact supported original states"),
    };
    assert_eq!(db.query_row("SELECT generation FROM pr_write_scopes WHERE host='github.invalid' AND principal='user-id:42' AND pr_node='PR_7'",[],|r|r.get::<_,i64>(0)).unwrap(),expected_generation);
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, usize>(0))
            .unwrap(),
        1
    );
    let (repo,pr,origin,base,tag,ack):(u64,u64,String,String,String,Option<i64>)=db.query_row(
        "SELECT repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48 FROM pr_write_metadata_origins_v25 WHERE attempt_id=1",[],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
    assert_eq!(
        (repo, pr, base, tag, ack),
        (
            410,
            1007,
            expected.base_key.clone(),
            "metadata-add-label".into(),
            None
        )
    );
    assert_eq!(origin,serde_json::to_string(&ExpectedRecord{seed:&expected,reference:&reference}).unwrap(),
        "Full native origin/digests/real database identity independently derived, not a stored-seed echo");
    let body = serde_json::to_string(&Protected {
        version: 17,
        values: BTreeMap::from([("body", SELECTED_LABEL)]),
        publication: &reference,
    })
    .unwrap();
    assert_eq!(saved_v283(f, &key), Some(body.clone()));
    body
}
fn reservation_generation(f: &Fixture) -> i64 {
    Connection::open_with_flags(cache_path(f),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
        .query_row("SELECT generation FROM sync_reservations WHERE host='github.invalid' AND login='user-id:42' AND source='personal-pr-source-v13/97974eedbadbf9ab292d5def48389c54eca7625642b37fe885f571951bf0c1d6' AND active=1",[],|r|r.get(0)).unwrap()
}
const HISTORY: &str = "Original member metadata history • example/repo#7";
const HISTORY_CONTROLS: &[&str] = &["[Inspect current]", "[Acknowledge local]", "[Esc Close]"];
fn resize_history_source(s: &mut Session, columns: u16, rows: u16) {
    let before = s.completed_cells_end();
    s.resize(columns, rows);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > before
            && s.completed_cells_screen().is_some_and(|frame| {
                frame.size() == (rows, columns)
                    && crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some()
                    && !frame.contents().contains("[Esc Cancel]")
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW actual resized owning Inbox before/after source capture\n{}",
            s.screen()
        );
    }
}
fn history_fact(s: &mut Session, mouse: bool, literal: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    for _ in 0..32 {
        s.poll_output(Duration::from_millis(1));
        let frame = s
            .completed_cells_screen()
            .expect("Completed original history");
        let body =
            crate::support::overlay_body(frame, HISTORY).expect("Same topmost actual history");
        assert!(HISTORY_CONTROLS.iter().all(|label| body.contains(label)));
        assert!(
            !body.contains("[Esc Cancel]")
                && !body.contains("[Confirm action]")
                && !body.contains("[Choose member]")
        );
        if crate::owned_form_prose_v120::adjacent_literal(&body, literal) {
            return;
        }
        let before = s.completed_cells_end();
        if mouse {
            let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
                frame,
                HISTORY,
                "[Inspect current]",
                &[],
            )
            .unwrap();
            s.wheel(x, y.saturating_sub(2), true);
        } else {
            s.send_key(b"\x1b[B");
        }
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > before
                && s.completed_cells_screen().is_some_and(|frame| {
                    crate::support::overlay_body(frame, HISTORY)
                        .is_some_and(|now| now != body && !now.contains("[Esc Cancel]"))
                })
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "ONE scroll needs NEW changed same-owned history, never hidden facts\n{}",
                s.screen()
            );
        }
    }
    panic!("Full exact historical literal not actually visible: {literal}");
}
fn history_open(s: &mut Session, f: &Fixture, mouse: bool, steps: &[GhStep], count: usize) {
    let before_sql = whole_sql_v282(f);
    let old = reservation_generation(f);
    let expected = next_reservation_v290(before_sql, old);
    let before = s.completed_cells_end();
    menu(s, mouse, "Inspect member metadata history");
    s.wait_for_command_count(f, count);
    ready_v283(s, before, HISTORY, HISTORY_CONTROLS);
    exact_published(s, f, steps, count);
    assert_eq!(whole_sql_v282(f),expected,"Exactly one literal Personal read reservation advances; all raw SQL/schema/sequence elsewhere stays");
}
fn history_close(s: &mut Session, f: &Fixture, mouse: bool, steps: &[GhStep], count: usize) {
    let before = whole_sql_v282(f);
    let after = s.completed_cells_end();
    control_v283(s, mouse, HISTORY, "[Esc Close]", HISTORY_CONTROLS);
    ready_v283(
        s,
        after,
        "Inbox • focused view ─",
        &["[u Metadata]", "[p PR actions]", "example/repo#7"],
    );
    assert_eq!(
        whole_sql_v282(f),
        before,
        "Pure history Close has no SQL/ACK/Source/write effect"
    );
    exact_published(s, f, steps, count);
}
fn original_facts(s: &mut Session, mouse: bool, state: &str, payload: &str) {
    history_fact(s, mouse, &format!("Attempt #1 • Add label • {state}"));
    history_fact(
        s,
        mouse,
        &format!("Historical H {HEAD} • exact ref {payload} • NOT current authority"),
    );
    history_fact(
        s,
        mouse,
        "Protected requested selection (NOT editable/restored):",
    );
    history_fact(s, mouse, SELECTED_LABEL);
}
fn current_payload(f: &Fixture) -> String {
    Connection::open_with_flags(cache_path(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
        .query_row(
            "SELECT payload_sha256 FROM pr_write_attempts WHERE attempt_id=1",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
pub(in crate::pr_actions::pr_metadata::metadata_actions) fn confirmed_restart(mouse: bool) {
    let f = fixture();
    let p = original_v282();
    let changed = raw(Some(PrMetadataCommand::AddLabel));
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut steps = startup_v282(&p);
    steps.extend(label_open_v282(&p));
    steps.extend(label_open_v282(&p));
    steps.extend(label_open_v282(&p));
    steps.extend(paid_label(&changed));
    assert_eq!(steps.len(), 79);
    steps.extend(startup_v282(&changed));
    steps.extend(history_roles(&changed, false));
    assert_eq!(steps.len(), 111);
    steps.extend(history_roles(&changed, false));
    assert_eq!(steps.len(), 119);
    f.scripted_gh(&steps);
    let mut first = Session::launch(&f, 120, 40);
    enter_metadata_v282(&mut first, mouse);
    actual_label_preview(&mut first, &f, mouse, &steps);
    let key = selected_draft_key_v283();
    Connection::open(cache_path(&f)).unwrap().execute_batch(&format!(
        "CREATE TRIGGER metadata_keep_original BEFORE DELETE ON drafts WHEN OLD.host='github.invalid' AND OLD.login='user-id:42' AND OLD.resource='{key}' BEGIN SELECT RAISE(ABORT,'fixture retains exact original cleanup'); END;"
    )).unwrap();
    let before = first.completed_cells_end();
    control_v283(
        &mut first,
        mouse,
        LABEL_FORM,
        "[Confirm action]",
        PREVIEW_CONTROLS,
    );
    first.wait_for_command_count(&f, 79);
    ready_v283(
        &mut first,
        before,
        "Confirmed Conversation • read-only refresh",
        &[
            "DURABLY CONFIRMED",
            "NEVER resend",
            "[r Refresh PR]",
            "[Esc Back to Inbox]",
        ],
    );
    exact_published(&mut first, &f, &steps, 79);
    let original = verify_original(&f, "confirmed");
    let before_close = whole_sql_v282(&f);
    let after = first.completed_cells_end();
    control_v283(
        &mut first,
        mouse,
        "Confirmed Conversation • read-only refresh",
        "[Esc Back to Inbox]",
        &["[r Refresh PR]", "[Esc Back to Inbox]"],
    );
    ready_v283(
        &mut first,
        after,
        "Inbox • focused view ─",
        &["Unchanged title"],
    );
    assert_eq!(whole_sql_v282(&f), before_close);
    preserve_git(&f, &head, &index, &status);
    first.quit_and_assert_restored_terminal(mouse);
    assert_eq!(whole_sql_v282(&f), before_close);
    let mut second = Session::launch(&f, 80, 24);
    // Preserve real80-column restart/history coverage, then expose the FULL
    // exact breadcrumb at120 for genuine source capture before returning80.
    second.wait_for_main_frame();
    resize_history_source(&mut second, 120, 40);
    enter_metadata_v282(&mut second, mouse);
    exact_published(&mut second, &f, &steps, 103);
    resize_history_source(&mut second, 80, 24);
    history_open(&mut second, &f, mouse, &steps, 111);
    let listed = whole_sql_v282(&f);
    let payload = current_payload(&f);
    original_facts(&mut second, mouse, "Confirmed", &payload);
    assert_eq!(saved_v283(&f, &key), Some(original.clone()));
    assert_eq!(whole_sql_v282(&f), listed);
    history_close(&mut second, &f, mouse, &steps, 111);
    history_open(&mut second, &f, mouse, &steps, 119);
    let twice = whole_sql_v282(&f);
    original_facts(&mut second, mouse, "Confirmed", &payload);
    assert_eq!(verify_original(&f, "confirmed"), original);
    assert_eq!(whole_sql_v282(&f), twice);
    history_close(&mut second, &f, mouse, &steps, 119);
    let commands = exact_published(&mut second, &f, &steps, 119);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    preserve_git(&f, &head, &index, &status);
    second.quit_and_assert_restored_terminal(mouse);
    assert_eq!(whole_sql_v282(&f), twice);
    assert_eq!(exact_published(&mut second, &f, &steps, 119), commands);
    preserve_git(&f, &head, &index, &status);
}
pub(in crate::pr_actions::pr_metadata::metadata_actions) fn receipt_failure(mouse: bool) {
    let f = fixture();
    let p = original_v282();
    let changed = raw(Some(PrMetadataCommand::AddLabel));
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut steps = startup_v282(&p);
    steps.extend(label_open_v282(&p));
    steps.extend(label_open_v282(&p));
    steps.extend(label_open_v282(&p));
    let mut paid = paid_label(&changed);
    paid.truncate(6);
    steps.extend(paid);
    assert_eq!(steps.len(), 63);
    steps.extend(startup_v282(&changed));
    steps.extend(history_roles(&changed, false));
    assert_eq!(steps.len(), 95);
    steps.extend(history_roles(&changed, true));
    assert_eq!(steps.len(), 105);
    f.scripted_gh(&steps);
    let mut first = Session::launch(&f, 120, 40);
    enter_metadata_v282(&mut first, mouse);
    actual_label_preview(&mut first, &f, mouse, &steps);
    Connection::open(cache_path(&f)).unwrap().execute_batch("CREATE TRIGGER metadata_receipt_fails BEFORE UPDATE ON pr_write_attempts WHEN NEW.state='confirmed' BEGIN SELECT RAISE(ABORT,'fixture real receipt store refused'); END;").unwrap();
    let after = first.completed_cells_end();
    control_v283(
        &mut first,
        mouse,
        LABEL_FORM,
        "[Confirm action]",
        PREVIEW_CONTROLS,
    );
    first.wait_for_command_count(&f, 63);
    // Actual output wraps the label and this entire independent expected
    // numeric/hash receipt over adjacent physical rows. Keep every byte,
    // within ONE completed owning body, not merely an effect-name prefix.
    let safe_effect = format!(
        "Actual safe effect: {}",
        expected_seed(&f, PrMetadataCommand::AddLabel)
            .expected_effect
            .resource_id
    );
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        first.poll_output(Duration::from_millis(1));
        if first.completed_cells_end() > after
            && first.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                let (rows, cols) = frame.size();
                crate::support::main_footer_visible(frame)
                    && frame
                        .contents_between(rows.saturating_sub(2), 0, rows.saturating_sub(2), cols)
                        .trim_end()
                        == "Observed effect; journal unavailable; no resend • ref#1"
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains("Original Conversation retired")
                                && crate::owned_form_prose_v120::adjacent_literal(
                                    &body,
                                    &safe_effect,
                                )
                        },
                    )
                    && ![
                        "[Confirm action]",
                        "[Choose member]",
                        "[Esc Cancel]",
                        "[Keep draft / Close]",
                        "Private description",
                        "Unchanged title",
                    ]
                    .iter()
                    .any(|private| text.contains(private))
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual observed effect is NOT a durable Confirmed receipt or new private source\n{}",
            first.screen()
        );
    }
    exact_published(&mut first, &f, &steps, 63);
    let original = verify_original(&f, "pending");
    let protected = whole_sql_v282(&f);
    first.quit_and_assert_restored_terminal(mouse);
    assert_eq!(whole_sql_v282(&f), protected);
    let mut second = Session::launch(&f, 80, 24);
    second.wait_for_main_frame();
    resize_history_source(&mut second, 120, 40);
    enter_metadata_v282(&mut second, mouse);
    exact_published(&mut second, &f, &steps, 87);
    resize_history_source(&mut second, 80, 24);
    history_open(&mut second, &f, mouse, &steps, 95);
    let payload = current_payload(&f);
    original_facts(&mut second, mouse, "Pending", &payload);
    let before_inspect = whole_sql_v282(&f);
    let generation = reservation_generation(&f);
    let expected = next_reservation_v290(before_inspect, generation);
    let before = second.completed_cells_end();
    control_v283(
        &mut second,
        mouse,
        HISTORY,
        "[Inspect current]",
        HISTORY_CONTROLS,
    );
    second.wait_for_command_count(&f, 105);
    ready_v283(&mut second, before, HISTORY, HISTORY_CONTROLS);
    history_fact(
        &mut second,
        mouse,
        &format!(
            "Fresh actual Add label • H {HEAD} • base {} • observation, not causal proof",
            "b".repeat(40)
        ),
    );
    history_fact(&mut second, mouse, "Currently observed new 🐛 • #81 • L_81");
    let commands = exact_published(&mut second, &f, &steps, 105);
    assert_eq!(whole_sql_v282(&f), expected);
    assert_eq!(verify_original(&f, "pending"), original);
    history_close(&mut second, &f, mouse, &steps, 105);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    preserve_git(&f, &head, &index, &status);
    second.quit_and_assert_restored_terminal(mouse);
    assert_eq!(whole_sql_v282(&f), expected);
    assert_eq!(exact_published(&mut second, &f, &steps, 105), commands);
    preserve_git(&f, &head, &index, &status);
}

pub(in crate::pr_actions::pr_metadata::metadata_actions) fn crash(mouse: bool) {
    use crate::support::owned_pty_processes_v1::process_identity;
    let f = fixture();
    let p = original_v282();
    let changed = raw(Some(PrMetadataCommand::AddLabel));
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut steps = startup_v282(&p);
    steps.extend(label_open_v282(&p));
    steps.extend(label_open_v282(&p));
    steps.extend(label_open_v282(&p));
    let mut invoked = paid_label(&changed).remove(0);
    invoked.delay_ms = 20_000;
    steps.push(invoked);
    assert_eq!(steps.len(), 58);
    steps.extend(startup_v282(&changed));
    steps.extend(history_roles(&changed, false));
    assert_eq!(steps.len(), 90);
    steps.extend(history_roles(&changed, true));
    assert_eq!(steps.len(), 100);
    f.scripted_gh(&steps);
    let mut first = Session::launch(&f, 120, 40);
    enter_metadata_v282(&mut first, mouse);
    actual_label_preview(&mut first, &f, mouse, &steps);
    let before = first.completed_cells_end();
    control_v283(
        &mut first,
        mouse,
        LABEL_FORM,
        "[Confirm action]",
        PREVIEW_CONTROLS,
    );
    let calls = exact_published(&mut first, &f, &steps, 58);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        first.poll_output(Duration::from_millis(1));
        if first.completed_cells_end() > before
            && first.completed_cells_screen().is_some_and(|frame| {
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, LABEL_FORM).is_some_and(|body| {
                        body.contains("[Esc Cancel]") && !body.contains("[Confirm action]")
                    })
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual same-owned busy Form while genuine mutation child is held\n{}",
            first.screen()
        );
    }
    let child_pid = i32::try_from(calls.last().unwrap()["pid"].as_i64().unwrap()).unwrap();
    let child = process_identity(child_pid)
        .unwrap()
        .expect("Actual held mutation child");
    let leader = process_identity(first.owned_application_pid())
        .unwrap()
        .unwrap();
    assert!(child.pid > 0 && !child.zombie);
    assert_eq!(child.parent, leader.pid);
    assert_eq!(child.session, leader.session);
    assert_eq!(child.uid, leader.uid);
    assert!(child.birth >= leader.birth);
    let original = verify_original(&f, "pending");
    let protected = whole_sql_v282(&f);
    first.terminate_abruptly();
    let current = process_identity(child_pid)
        .unwrap()
        .expect("SAME held child survives its application SIGKILL");
    assert_eq!(
        (current.pid, current.session, current.uid, current.birth),
        (child.pid, child.session, child.uid, child.birth)
    );
    assert!(!current.zombie);
    // SAFETY: the positive PID is the just-revalidated actual child of this
    // owned session (UID/birth/SID pinned before the deliberate app death).
    assert_eq!(unsafe { libc::kill(child_pid, libc::SIGKILL) }, 0);
    assert_eq!(verify_original(&f, "pending"), original);
    assert_eq!(whole_sql_v282(&f), protected);
    // Existing Session Drop performs exact-owned descendant cleanup/reaping,
    // including an abruptly terminated app. Never accept an adopted zombie.
    drop(first);
    assert!(process_identity(child_pid).unwrap().is_none());
    assert_eq!(whole_sql_v282(&f), protected);
    preserve_git(&f, &head, &index, &status);
    let mut second = Session::launch(&f, 80, 24);
    second.wait_for_main_frame();
    resize_history_source(&mut second, 120, 40);
    enter_metadata_v282(&mut second, mouse);
    exact_published(&mut second, &f, &steps, 82);
    resize_history_source(&mut second, 80, 24);
    history_open(&mut second, &f, mouse, &steps, 90);
    let payload = current_payload(&f);
    original_facts(&mut second, mouse, "Pending", &payload);
    let expected = next_reservation_v290(whole_sql_v282(&f), reservation_generation(&f));
    let after = second.completed_cells_end();
    control_v283(
        &mut second,
        mouse,
        HISTORY,
        "[Inspect current]",
        HISTORY_CONTROLS,
    );
    second.wait_for_command_count(&f, 100);
    ready_v283(&mut second, after, HISTORY, HISTORY_CONTROLS);
    history_fact(
        &mut second,
        mouse,
        &format!(
            "Fresh actual Add label • H {HEAD} • base {} • observation, not causal proof",
            "b".repeat(40)
        ),
    );
    history_fact(&mut second, mouse, "Currently observed new 🐛 • #81 • L_81");
    let commands = exact_published(&mut second, &f, &steps, 100);
    assert_eq!(verify_original(&f, "pending"), original);
    assert_eq!(whole_sql_v282(&f), expected);
    history_close(&mut second, &f, mouse, &steps, 100);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    preserve_git(&f, &head, &index, &status);
    second.quit_and_assert_restored_terminal(mouse);
    assert_eq!(whole_sql_v282(&f), expected);
    assert_eq!(exact_published(&mut second, &f, &steps, 100), commands);
    assert!(process_identity(child_pid).unwrap().is_none());
    preserve_git(&f, &head, &index, &status);
}

pub(in crate::pr_actions::pr_metadata::metadata_actions) fn prewrite_access(mouse: bool) {
    let f = fixture();
    let p = original_v282();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let raw_head = fs::read(f.repo().join(".git/HEAD")).unwrap();
    let mut steps = startup_v282(&p);
    steps.extend(label_open_v282(&p));
    steps.extend(label_open_v282(&p));
    assert_eq!(steps.len(), 46);
    let mut confirm = principal_v282();
    let mut denied = values::get("repos/example/repo", Value::Null);
    denied.status = 1;
    denied.stdout.clear();
    // The classifier requires the genuine trailing HTTP envelope. No private
    // raw diagnostic transfers into the fixed Permission presentation.
    denied.stderr = b"gh: Forbidden PRIVATE_PREWRITE_403_CANARY (HTTP 403)\n".to_vec();
    confirm.push(denied);
    assert_eq!(confirm.len(), 3);
    assert_eq!(confirm[0].args, ["--version"]);
    assert_eq!(
        serde_json::from_slice::<Value>(&confirm[1].stdout).unwrap(),
        json!({"id":42,"login":"requester"})
    );
    assert_eq!(confirm[2].args[1], "repos/example/repo");
    steps.extend(confirm);
    assert_eq!(
        steps.len(),
        49,
        "24 + actual Open11 + Preview11 + denied Confirm3; no closing/write"
    );
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata_v282(&mut s, mouse);
    exact_v282(&mut s, &f, &steps, 24);
    let durable = actual_label_preview(&mut s, &f, mouse, &steps);
    let key = selected_draft_key_v283();
    let before_confirm = whole_sql_v282(&f);
    assert_no_publication(&f);
    let stale_confirm = crate::support::completed_navigation_controls_v1::owned_action_position(
        s.completed_cells_screen().unwrap(),
        LABEL_FORM,
        "[Confirm action]",
        &[],
    )
    .expect("Actual genuine preview's complete Confirm hit before capability loss");
    let after = s.completed_cells_end();
    control_v283(
        &mut s,
        mouse,
        LABEL_FORM,
        "[Confirm action]",
        PREVIEW_CONTROLS,
    );
    s.wait_for_command_count(&f, 49);
    capability_retired(&mut s, after);
    let commands = exact_v282(&mut s, &f, &steps, 49);
    assert_eq!(whole_sql_v282(&f), before_confirm);
    assert_eq!(saved_v283(&f, &key), Some(durable.clone()));
    // The former input/control cannot reconstruct a Form or a Source. Use a
    // disabled action key on keyboard, exact stale old hit on mouse.
    let after = s.completed_cells_end();
    if mouse {
        s.click(stale_confirm.0, stale_confirm.1);
    } else {
        s.send_key(b"u");
    }
    capability_retired(&mut s, after);
    assert_eq!(exact_v282(&mut s, &f, &steps, 49), commands);
    assert_eq!(whole_sql_v282(&f), before_confirm);
    let after = s.completed_cells_end();
    s.resize(80, 24);
    capability_retired(&mut s, after);
    assert_eq!(whole_sql_v282(&f), before_confirm);
    assert_eq!(saved_v283(&f, &key), Some(durable));
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    assert_eq!(fs::read(f.repo().join(".git/HEAD")).unwrap(), raw_head);
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_v282(&mut s, &f, &steps, 49), commands);
    assert_eq!(whole_sql_v282(&f), before_confirm);
    preserve_git(&f, &head, &index, &status);
}
