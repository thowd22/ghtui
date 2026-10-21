//! Current genuine Metadata/Core fixture, scoped to the migrated legacy names.
use super::*;
use sha2::{Digest, Sha256};
#[path = "pr_metadata_read_retirement_v70.rs"]
mod read_retirement_v70;
const FOOTER: &[&str] = &[
    "[Preview]",
    "[External editor]",
    "[Confirm action]",
    "[Keep draft / Close]",
];
const QUERY:&str="query($id:ID!){node(id:$id){__typename ... on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanUpdate repository{id nameWithOwner}}}}";
const BASE_TITLE: &str = "Change PR Base • example/repo#7";
const BASE_PICKER: &str = "PR metadata choices · Base branch";
const BASE_FORM: &[&str] = &["[F2 Choose branch]", "[Preview]", "[Esc Close]"];
const BASE_PREVIEW: &[&str] = &["[Preview]", "[Confirm action]", "[Keep draft / Close]"];
const PICKER_CONTROLS: &[&str] = &["[Enter Choose]", "[Esc Back]"];
const TIP: &str = "cccccccccccccccccccccccccccccccccccccccc";

fn base_read(raw: &Value, list: bool, selected: Option<&str>) -> Vec<GhStep> {
    let mut calls = phase(raw);
    let mut members = Vec::new();
    if list {
        members.push(step(
            "repos/example/repo/branches?per_page=100&page=1",
            "GET",
            json!([{"name":"main","commit":{"sha":"b".repeat(40)},"protected":false},
                {"name":BASE,"commit":{"sha":TIP},"protected":false}]),
            false,
        ));
    }
    if let Some(name) = selected {
        let encoded = if name == BASE {
            "release%2Foctober"
        } else {
            name
        };
        for _ in 0..2 {
            members.push(step(&format!("repos/example/repo/branches/{encoded}"), "GET",
                json!({"name":name,"commit":{"sha":if name=="main"{"b".repeat(40)}else{TIP.into()}},"protected":false}), false));
        }
    }
    calls.splice(5..5, members);
    assert_eq!(
        calls.len(),
        9 + usize::from(list) + 2 * usize::from(selected.is_some())
    );
    calls
}
fn focused_base_field(frame: &vt100::Screen) -> Option<usize> {
    use crate::support::completed_navigation_controls_v1::owned_action_position;
    crate::support::overlay_body(frame, BASE_TITLE)?;
    let (_, footer) = owned_action_position(frame, BASE_TITLE, "[Esc Close]", &[])
        .or_else(|| owned_action_position(frame, BASE_TITLE, "[Keep draft / Close]", &[]))?;
    let mut focused = None;
    for (index, label) in [
        "Selected existing base branch (F2 Choose)",
        "Exact observed selected tip (read only)",
    ]
    .iter()
    .enumerate()
    {
        let Some((x, heading)) = owned_action_position(frame, BASE_TITLE, label, &[]) else {
            continue;
        };
        let value = heading.checked_add(1)?;
        if value < footer
            && frame.cell(value, x).is_some_and(|cell| cell.underline())
            && focused.replace(index).is_some()
        {
            return None;
        }
    }
    focused
}
fn base_action(s: &mut Session, mouse: bool, label: &str, controls: &[&str]) {
    use crate::support::completed_navigation_controls_v1::{owned_focus_v1, FocusAttributeV1};
    if !mouse {
        let until = Instant::now() + Duration::from_secs(8);
        for _ in 0..24 {
            s.poll_output(Duration::from_millis(1));
            let frame = s
                .completed_cells_screen()
                .expect("Actual completed Base Form");
            let previous = owned_focus_v1(
                frame,
                BASE_TITLE,
                label,
                FocusAttributeV1::Inverse,
                controls,
                &[],
            )
            .expect("Whole control in SAME topmost Base Form");
            if previous.target_focused {
                break;
            }
            let field = focused_base_field(frame);
            let after = s.completed_cells_end();
            s.send_key(b"\t");
            loop {
                s.poll_output(Duration::from_millis(1));
                if s.completed_cells_end() > after
                    && s.completed_cells_screen().is_some_and(|frame| {
                        owned_focus_v1(
                            frame,
                            BASE_TITLE,
                            label,
                            FocusAttributeV1::Inverse,
                            controls,
                            &[],
                        )
                        .is_some_and(|current| {
                            current.acknowledges_change_from(&previous)
                                || current.target == previous.target
                                    && focused_base_field(frame) != field
                                    && (focused_base_field(frame).is_some() || field.is_some())
                        })
                    })
                {
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "NEW same-owned Base field/footer focus required\n{}",
                    s.screen()
                );
            }
        }
    }
    // base_action has already crossed both actual field positions. Do not ask
    // the footer-only shared helper to traverse those positions again.
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let point = s.completed_cells_screen().and_then(|frame| {
            crate::support::completed_navigation_controls_v1::owned_action_position(
                frame,
                BASE_TITLE,
                label,
                &[],
            )
            .filter(|(x, y)| {
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
            "Exact enabled Base control {label}\n{}",
            s.screen()
        );
    }
}
fn open_base(s: &mut Session, f: &Fixture, mouse: bool, end: usize, name: &str) {
    let before = s.completed_cells_end();
    menu(s, mouse, "Change base branch");
    s.wait_for_command_count(f, end);
    ready(s, before, BASE_PICKER, PICKER_CONTROLS);
    if mouse {
        s.click_visible_action("Filter focused:");
    }
    s.send_key(b"\x01");
    s.paste(name);
    s.wait_for_screen(&format!("Filter focused: {name}"));
    let rows_after = s.completed_cells_end();
    if mouse {
        s.click_visible_action(&format!("◆ eligible · {name}"));
    } else {
        s.send_key(b"\t");
    }
    ready(
        s,
        rows_after,
        &format!("{BASE_PICKER} · Rows focused"),
        PICKER_CONTROLS,
    );
    action(s, mouse, BASE_PICKER, "[Enter Choose]", PICKER_CONTROLS);
    s.wait_for_overlay_close(BASE_PICKER);
    s.wait_for_owned_completed_action(BASE_TITLE, "[Preview]", &[]);
}
fn preview_base(s: &mut Session, f: &Fixture, mouse: bool, end: usize) {
    let before = s.completed_cells_end();
    base_action(s, mouse, "[Preview]", BASE_FORM);
    s.wait_for_command_count(f, end);
    ready(
        s,
        before,
        BASE_TITLE,
        &[
            "READ-ONLY PREVIEW",
            "AFTER Base release/october",
            "[Confirm action]",
        ],
    );
}
fn resize(s: &mut Session, cols: u16, rows: u16, title: &str, controls: &[&str]) {
    let previous = s.completed_cells_screen().map(|f| f.size());
    let before = s.completed_cells_end();
    s.resize(cols, rows);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if (previous == Some((rows, cols)) || s.completed_cells_end() > before)
            && s.completed_cells_screen().is_some_and(|f| {
                f.size() == (rows, cols)
                    && crate::support::main_footer_visible(f)
                    && crate::support::overlay_body(f, title).is_some_and(|body| {
                        body.lines().any(|line| {
                            let mut remaining = line;
                            controls.iter().all(|label| {
                                if let Some(n) = remaining.find(label) {
                                    remaining = &remaining[n + label.len()..];
                                    true
                                } else {
                                    false
                                }
                            })
                        })
                    })
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW centered {title}/{cols}x{rows} with all ordered controls\n{}",
            s.screen()
        );
    }
}
fn principal() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
        step("user", "GET", json!({"id":42,"login":"octocat"}), false),
    ]
}
fn canonical(mut raw: Value) -> Value {
    raw["url"] = json!("https://github.invalid/api/v3/repos/example/repo/pulls/7");
    raw["milestone"]["id"] = json!(5);
    raw["head"]["repo"] = repository();
    raw["base"]["repo"] = repository();
    raw
}
fn linked(raw: &Value) -> Value {
    let mut value = issue();
    value["milestone"]["id"] = json!(5);
    for key in ["title", "body", "state", "user"] {
        value[key] = raw[key].clone();
    }
    value
}
fn context(raw: &Value) -> Vec<GhStep> {
    vec![
        step("repos/example/repo/pulls/7", "GET", raw.clone(), false),
        step("repos/example/repo/issues/7", "GET", linked(raw), false),
        step("repos/example/repo", "GET", repository(), false),
        step("repos/example/repo/pulls/7", "GET", raw.clone(), false),
    ]
}
fn section(raw: &Value) -> Vec<GhStep> {
    let mut calls = principal();
    calls.push(step("repos/example/repo", "GET", repository(), false));
    calls.extend(context(raw));
    calls.push(step("repos/example/repo", "GET", repository(), false));
    calls.extend(principal());
    assert_eq!(calls.len(), 10);
    calls
}
fn phase(raw: &Value) -> Vec<GhStep> {
    let mut calls = principal();
    calls.push(step("repos/example/repo", "GET", repository(), false));
    calls.push(step(
        "repos/example/repo/pulls/7",
        "GET",
        raw.clone(),
        false,
    ));
    let mut gql = step(
        "graphql",
        "POST",
        json!({"data":{"node":{"__typename":"PullRequest","id":NODE,"fullDatabaseId":"1007","number":7,"headRefOid":HEAD,"baseRefOid":raw["base"]["sha"],"headRefName":"feature","baseRefName":raw["base"]["ref"],"state":"OPEN","isDraft":false,"merged":false,"viewerCanUpdate":true,"repository":{"id":"R_metadata_410","nameWithOwner":REPOSITORY}}}}),
        false,
    );
    gql.args.extend(["--input".into(), "-".into()]);
    gql.stdin = serde_json::to_vec(&json!({"query":QUERY,"variables":{"id":NODE}})).unwrap();
    calls.push(gql);
    calls.push(step(
        "repos/example/repo/pulls/7",
        "GET",
        raw.clone(),
        false,
    ));
    calls.push(step("repos/example/repo", "GET", repository(), false));
    calls.extend(principal());
    assert_eq!(calls.len(), 9);
    calls
}
fn startup(raw: &Value) -> Vec<GhStep> {
    let mut calls = at_target(initial(), raw);
    calls.extend(at_target(details(vec![starter()]), raw));
    calls.extend(section(raw));
    for call in &mut calls {
        if call.args == ["--version"] {
            call.stdout = b"gh version 2.99.0 (2026-10-01)\n".to_vec();
        }
    }
    assert_eq!(calls.len(), 25);
    calls
}
fn exact_current(s: &mut Session, f: &Fixture, steps: &[GhStep], count: usize) -> Vec<Value> {
    s.wait_for_command_count(f, count);
    let rows = records(s, f, count);
    assert_eq!(rows.len(), count);
    for (n, (row, expected)) in rows.iter().zip(&steps[..count]).enumerate() {
        assert_eq!(row["step"], n);
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(
            row["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    rows
}
fn ready(s: &mut Session, after: usize, title: &str, labels: &[&str]) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::overlay_body(frame, title).is_some_and(|body| {
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
            "NEW exact Metadata Form {title}/{labels:?}\n{}",
            s.screen()
        );
    }
}
fn form_title(field: &str) -> String {
    format!("Edit PR {field} • {REPOSITORY}#7")
}
fn focus_preview_reverse(s: &mut Session, title: &str, controls: &[&str]) {
    use crate::support::completed_navigation_controls_v1::{owned_focus_v1, FocusAttributeV1};
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let Some(previous) = s.completed_cells_screen().and_then(|frame| {
            owned_focus_v1(
                frame,
                title,
                "[Preview]",
                FocusAttributeV1::Inverse,
                controls,
                &[],
            )
        }) else {
            assert!(
                Instant::now() < until,
                "Actual complete SAME Metadata owner before Preview BackTab\n{}",
                s.screen()
            );
            continue;
        };
        if previous.target_focused {
            return;
        }
        // Confirm -> External editor -> Preview stays on actual visible
        // controls. Forward traversal would cross the editable Body, whose
        // underline is not a footer inverse. A starting Body -> last footer
        // is also a real observed focus change, never an assumed skip.
        let after = s.completed_cells_end();
        s.send_key(b"\x1b[Z");
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after
                && s.completed_cells_screen().is_some_and(|frame| {
                    owned_focus_v1(
                        frame,
                        title,
                        "[Preview]",
                        FocusAttributeV1::Inverse,
                        controls,
                        &[],
                    )
                    .is_some_and(|current| current.acknowledges_change_from(&previous))
                })
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "ONE BackTab requires NEW SAME owned Metadata footer inverse\n{}",
                s.screen()
            );
        }
    }
}
fn action(s: &mut Session, mouse: bool, title: &str, label: &str, controls: &[&str]) {
    if !mouse && label == "[Preview]" {
        focus_preview_reverse(s, title, controls);
    } else if !mouse {
        s.focus_completed_owned_action_v1(
            title,
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            controls,
            &[],
        );
    }
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let point = s.completed_cells_screen().and_then(|frame| {
            crate::support::completed_navigation_controls_v1::owned_action_position(
                frame,
                title,
                label,
                &[],
            )
            .filter(|(x, y)| {
                frame.cell(*y, *x).is_some_and(|cell| {
                    (label != "[Confirm action]" || cell.underline()) && (mouse || cell.inverse())
                })
            })
        });
        if let Some((x, y)) = point {
            if mouse {
                s.click(x, y);
            } else {
                s.send_key(b"\r");
            }
            return;
        }
        assert!(
            Instant::now() < until,
            "Exact actual Metadata action enabled {title}/{label}\n{}",
            s.screen()
        );
    }
}
fn open_field(s: &mut Session, f: &Fixture, mouse: bool, field: &str, end: usize) {
    let before = s.completed_cells_end();
    menu(
        s,
        mouse,
        if field == "Title" {
            "Edit PR title"
        } else {
            "Edit PR description"
        },
    );
    s.wait_for_command_count(f, end);
    ready(s, before, &form_title(field), FOOTER);
}
fn preview(s: &mut Session, f: &Fixture, mouse: bool, field: &str, end: usize) {
    let before = s.completed_cells_end();
    action(s, mouse, &form_title(field), "[Preview]", FOOTER);
    s.wait_for_command_count(f, end);
    ready(
        s,
        before,
        &form_title(field),
        &[
            "READ-ONLY PREVIEW",
            "[Confirm action]",
            "PR metadata preview",
        ],
    );
}
fn key(raw: &Value, field: ghtui::providers::gh::PrMetadataTextFieldV25) -> String {
    format!(
        "metadata-text-v25/{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                scope(),
                410u64,
                "R_metadata_410",
                1007u64,
                NODE,
                7u64,
                HEAD,
                raw["base"]["sha"].as_str().unwrap(),
                field
            ))
            .unwrap()
        )
    )
}
fn wait_draft(
    s: &mut Session,
    f: &Fixture,
    raw: &Value,
    field: ghtui::providers::gh::PrMetadataTextFieldV25,
    body: &str,
) {
    let key = key(raw, field);
    s.try_wait_for_condition(
        |_| {
            Cache::open(&cache_path(f))
                .unwrap()
                .draft(&scope(), &key)
                .unwrap()
                .and_then(|v| serde_json::from_str::<Value>(&v).ok())
                .is_some_and(|v| {
                    v["version"] == 17 && v["values"]["body"] == body && v["publication"].is_null()
                })
        },
        "Exact accepted current draft durable without publication/approval",
        Duration::from_secs(8),
    )
    .unwrap();
}
fn post(before: &Value, after: &Value, field: &str, value: &str) -> Vec<GhStep> {
    let mut calls = vec![
        write(
            "repos/example/repo/pulls/7",
            "PATCH",
            Some(json!({field:value})),
            after.clone(),
        ),
        step("repos/example/repo/pulls/7", "GET", after.clone(), false),
        step("user", "GET", json!({"id":42,"login":"octocat"}), false),
        step("repos/example/repo", "GET", repository(), false),
        step("repos/example/repo/pulls/7", "GET", after.clone(), false),
    ];
    calls.extend(
        at_target(details(vec![starter()]), after)[1..6]
            .iter()
            .cloned(),
    );
    calls.push(step(
        "repos/example/repo/pulls/7",
        "GET",
        after.clone(),
        false,
    ));
    calls.extend(context(after));
    calls.push(step(
        "user",
        "GET",
        json!({"id":42,"login":"octocat"}),
        false,
    ));
    assert_eq!(calls.len(), 16);
    assert_eq!(before["head"]["sha"], after["head"]["sha"]);
    calls
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Database {
    path: Vec<u8>,
    device: u64,
    inode: u64,
}
fn exact_database(f: &Fixture, database: &Database) {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
    let path = fs::canonicalize(cache_path(f)).unwrap();
    let metadata = fs::metadata(&path).unwrap();
    assert_eq!(database.path, path.as_os_str().as_bytes());
    assert_eq!(database.device, metadata.dev());
    assert_eq!(database.inode, metadata.ino());
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct TextSeed {
    version: u8,
    database: Database,
    scope: Scope,
    repository_id: u64,
    repository_node: String,
    repository: String,
    pr_id: u64,
    pr_node: String,
    number: u64,
    head: String,
    base: String,
    field: ghtui::providers::gh::PrMetadataTextFieldV25,
    before_sha256: String,
    requested_sha256: String,
    requester: String,
    author_uid: Option<u64>,
}
fn confirmed_text(
    f: &Fixture,
    before: &Value,
    field: ghtui::providers::gh::PrMetadataTextFieldV25,
    value: &str,
    retained: bool,
) -> String {
    let db = Connection::open(cache_path(f)).unwrap();
    let command = if field == ghtui::providers::gh::PrMetadataTextFieldV25::Title {
        "metadata-title-v25"
    } else {
        "metadata-description-v25"
    };
    let(row_id,state,selected,head,payload,receipt):(i64,String,String,String,String,String)=db.query_row("SELECT attempt_id,state,selected_resource,observed_head,payload_sha256,confirmed_json FROM pr_write_attempts WHERE command=?1",[command],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
    assert!(row_id > 0);
    assert_eq!(state, "confirmed");
    assert_eq!(head, HEAD);
    assert_eq!(selected, key(before, field));
    let encoded: String = db
        .query_row(
            "SELECT origin_json FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1",
            [row_id],
            |r| r.get(0),
        )
        .unwrap();
    let record: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        record["reference"],
        json!({"attempt_id":row_id,"pr_node_id":NODE,"observed_head":HEAD,"payload_sha256":payload})
    );
    let seed: TextSeed = serde_json::from_value(record["seed"].clone()).unwrap();
    assert_eq!(seed.version, 25);
    exact_database(f, &seed.database);
    assert_eq!(seed.scope, scope());
    assert_eq!(seed.repository_id, 410);
    assert_eq!(seed.repository_node, "R_metadata_410");
    assert_eq!(seed.repository, REPOSITORY);
    assert_eq!(seed.pr_id, 1007);
    assert_eq!(seed.pr_node, NODE);
    assert_eq!(seed.number, 7);
    assert_eq!(seed.head, HEAD);
    assert_eq!(seed.base, before["base"]["sha"].as_str().unwrap());
    assert_eq!(seed.field, field);
    assert_eq!(seed.author_uid, Some(42));
    assert_eq!(seed.requester, "octocat");
    assert_eq!(
        seed.before_sha256,
        format!(
            "{:x}",
            Sha256::digest(
                before[if field == ghtui::providers::gh::PrMetadataTextFieldV25::Title {
                    "title"
                } else {
                    "body"
                }]
                .as_str()
                .unwrap()
                .as_bytes()
            )
        )
    );
    let requested_hash = format!("{:x}", Sha256::digest(value.as_bytes()));
    assert_eq!(seed.requested_sha256, requested_hash);
    let origin_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&seed).unwrap()));
    assert_eq!(
        payload,
        PrWriteIntent {
            command: command.into(),
            selected_resource: selected.clone(),
            observed_head: HEAD.into(),
            values: BTreeMap::from([
                ("body".into(), value.into()),
                ("origin-v25".into(), origin_hash)
            ])
        }
        .digest()
        .unwrap()
    );
    let receipt: Value = serde_json::from_str(&receipt).unwrap();
    assert_eq!(
        receipt,
        json!({"kind":"metadata","resource_id":format!("metadata-text-v25/{}/{requested_hash}",if field==ghtui::providers::gh::PrMetadataTextFieldV25::Title{"title"}else{"description"}),"node_id":NODE})
    );
    let draft = Cache::open(&cache_path(f))
        .unwrap()
        .draft(&scope(), &selected)
        .unwrap();
    assert_eq!(draft.is_some(), retained);
    if let Some(draft) = draft {
        let draft: Value = serde_json::from_str(&draft).unwrap();
        assert_eq!(draft["version"], 17);
        assert_eq!(draft["values"]["body"], value);
        assert!(draft["publication"].is_object());
        assert!(draft.get("approval").is_none());
        assert!(draft.get("fingerprint").is_none());
    }
    encoded
}
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct BaseSeed {
    version: u8,
    database: Database,
    scope: Scope,
    repository_id: u64,
    repository_node: String,
    repository: String,
    pr_id: u64,
    pr_node: String,
    number: u64,
    head: String,
    base: String,
    base_ref: String,
    requested_ref: String,
    requested_tip: String,
    requester: String,
    author_uid: Option<u64>,
}
fn base_key(raw: &Value) -> String {
    format!(
        "metadata-base-v32/{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                scope(),
                410u64,
                "R_metadata_410",
                1007u64,
                NODE,
                7u64,
                HEAD,
                raw["base"]["sha"].as_str().unwrap(),
                raw["base"]["ref"].as_str().unwrap()
            ))
            .unwrap()
        )
    )
}
fn confirmed_base(f: &Fixture, before: &Value) {
    let db = Connection::open(cache_path(f)).unwrap();
    let (attempt,state,selected,head,payload,receipt):(i64,String,String,String,String,String)=db.query_row(
        "SELECT attempt_id,state,selected_resource,observed_head,payload_sha256,confirmed_json FROM pr_write_attempts WHERE command='metadata-base-v32'",[],
        |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
    assert!(attempt > 0);
    assert_eq!(state, "confirmed");
    assert_eq!(selected, base_key(before));
    assert_eq!(head, HEAD);
    let encoded: String = db
        .query_row(
            "SELECT origin_json FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=?1",
            [attempt],
            |r| r.get(0),
        )
        .unwrap();
    let recorded: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(
        recorded["reference"],
        json!({"attempt_id":attempt,"pr_node_id":NODE,"observed_head":HEAD,"payload_sha256":payload})
    );
    let seed: BaseSeed = serde_json::from_value(recorded["seed"].clone()).unwrap();
    assert_eq!(seed.version, 32);
    exact_database(f, &seed.database);
    assert_eq!(seed.scope, scope());
    assert_eq!(seed.repository_id, 410);
    assert_eq!(seed.repository_node, "R_metadata_410");
    assert_eq!(seed.repository, REPOSITORY);
    assert_eq!(seed.pr_id, 1007);
    assert_eq!(seed.pr_node, NODE);
    assert_eq!(seed.number, 7);
    assert_eq!(seed.head, HEAD);
    assert_eq!(seed.base, before["base"]["sha"].as_str().unwrap());
    assert_eq!(seed.base_ref, before["base"]["ref"].as_str().unwrap());
    assert_eq!(seed.requested_ref, BASE);
    assert_eq!(seed.requested_tip, TIP);
    assert_eq!(seed.requester, "octocat");
    assert_eq!(seed.author_uid, Some(42));
    assert_eq!(
        payload,
        PrWriteIntent {
            command: "metadata-base-v32".into(),
            selected_resource: selected.clone(),
            observed_head: HEAD.into(),
            values: BTreeMap::from([
                ("base".into(), BASE.into()),
                ("tip".into(), TIP.into()),
                (
                    "origin-v32".into(),
                    format!("{:x}", Sha256::digest(serde_json::to_vec(&seed).unwrap()))
                )
            ])
        }
        .digest()
        .unwrap()
    );
    let mut selected_digest = Sha256::new();
    selected_digest.update(BASE.as_bytes());
    selected_digest.update([0]);
    selected_digest.update(TIP.as_bytes());
    assert_eq!(
        serde_json::from_str::<Value>(&receipt).unwrap(),
        json!({"kind":"metadata","resource_id":format!("metadata-base-v32/{:x}",selected_digest.finalize()),"node_id":NODE})
    );
    assert!(Cache::open(&cache_path(f))
        .unwrap()
        .draft(&scope(), &selected)
        .unwrap()
        .is_none());
    let fences:Vec<(String,i64)>=db.prepare("SELECT kind,mutation_ordinal FROM sync_mutation_fences WHERE host=?1 AND login=?2 AND repository=?3 AND resource=?4 AND kind IN ('review','inline_comment') ORDER BY kind").unwrap()
        .query_map(params![scope().host,scope().login,REPOSITORY,NODE],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().map(Result::unwrap).collect();
    // Exact category rows are independently checked below using the real
    // persisted attempt, not an inferred target display name.
    assert_eq!(
        fences,
        vec![
            ("inline_comment".into(), attempt),
            ("review".into(), attempt)
        ],
        "Both comparison categories belong to the SAME actual Base attempt"
    );
}
fn actual_projection(f: &Fixture, expected: &Value, count: usize) {
    let cache = Cache::open(&cache_path(f)).unwrap();
    let frame = cache
        .sync_frame(&scope(), &format!("pr-write-projection:{NODE}"))
        .unwrap()
        .unwrap();
    assert!(frame.complete);
    let saved = &frame
        .records
        .iter()
        .find(|r| r.kind == "pr-details" && r.id == NODE)
        .unwrap()
        .payload;
    assert_eq!(saved["summary"]["pr"]["title"], expected["title"]);
    assert_eq!(saved["summary"]["pr"]["body"], expected["body"]);
    assert_eq!(saved["summary"]["base_ref"], expected["base"]["ref"]);
    assert_eq!(saved["summary"]["base_sha"], expected["base"]["sha"]);
    assert_eq!(saved["summary"]["head_sha"], HEAD);
    assert_eq!(saved["comments"].as_array().unwrap().len(), 1);
    assert_eq!(saved["comments"][0]["body"], "Existing starter comment");
    let attempts = receipts(f);
    assert_eq!(attempts.len(), count);
    assert!(attempts
        .windows(2)
        .all(|p| p[0].attempt_id < p[1].attempt_id));
    assert_eq!(
        frame.generation,
        u64::try_from(attempts.last().unwrap().attempt_id).unwrap()
    );
    assert!(cache
        .list(&Scope::github("github.invalid", Some(43)).unwrap(), None)
        .unwrap()
        .is_empty());
}
fn metadata_return(s: &mut Session, after: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|f| {
                let text = f.contents();
                crate::support::main_footer_visible(f)
                    && crate::support::controls::control_painted(f, "[u Metadata]")
                    && text.contains("Confirmed • fresh PR/search observation")
                    && !text.contains("[Confirm action]")
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "Actual fresh Metadata after one Confirm\n{}",
            s.screen()
        );
    }
}
fn close_text(s: &mut Session, mouse: bool, field: &str) {
    let before = s.completed_cells_end();
    action(s, mouse, &form_title(field), "[Keep draft / Close]", FOOTER);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > before
            && s.completed_cells_screen().is_some_and(|f| {
                let text = f.contents();
                crate::support::main_footer_visible(f)
                    && crate::support::controls::control_painted(f, "[u Metadata]")
                    && !text.contains("[Keep draft / Close]")
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW actual Metadata after pure Close\n{}",
            s.screen()
        );
    }
}
fn enter_metadata_from_conversation(s: &mut Session, mouse: bool, opening: usize) {
    // The legacy open helper's starter text is also present in the preceding
    // Summary. Observe the actual completed, idle Conversation before ONE u;
    // a key sent during its pending transition is correctly not replayed.
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > opening
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    && text.contains("example/repo#7")
                    && text.contains(HEAD)
                    && crate::support::controls::control_painted(frame, "[u Metadata]")
                    && text.contains("@octocat PR description")
                    && text.contains("A fixture description")
                    && text.contains("@octocat comment #9 • 2026-10-13T00:00:00Z")
                    && text.contains("Existing starter comment")
                    && !text.contains("[Esc Cancel]")
                    && !text.contains("Verifying original Conversation")
                    && !text.contains("Loading PR Metadata")
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "NEW completed idle original Conversation before Metadata input\n{}",
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
                // Metadata renders actual context, not the full commit hash.
                // The complete H was witnessed on Conversation above and is
                // still independently checked by strict gh and stored records.
                crate::support::main_footer_visible(frame)
                    && crate::support::controls::control_painted(frame, "[u Metadata]")
                    && crate::support::controls::control_painted(frame, "[p PR actions]")
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains("example/repo#7")
                                && body
                                    .contains("Your repository permissions: read true • write true")
                                && body.contains("Locked conversation: false")
                                && body.contains("Milestone: October release")
                                && body
                                    .contains("Source: https://github.invalid/example/repo/pull/7")
                        },
                    )
                    && !text.contains("[Esc Cancel]")
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "NEW completed actual Metadata section after one input\n{}",
            s.screen()
        );
    }
}
fn enter_metadata(s: &mut Session, mouse: bool) {
    s.wait_for_main_frame();
    let original_size = s.completed_cells_screen().unwrap().size();
    if original_size.1 < 120 {
        let before = s.completed_cells_end();
        s.resize(120, 40);
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > before
                && s.completed_cells_screen().is_some_and(|frame| {
                    frame.size() == (40, 120) && crate::support::main_footer_visible(frame)
                })
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "NEW temporary wide original frame before full PR/H witness\n{}",
                s.screen()
            );
        }
    }
    let opening = s.completed_cells_end();
    open(s, mouse);
    enter_metadata_from_conversation(s, mouse, opening);
    if original_size.1 < 120 {
        let before = s.completed_cells_end();
        s.resize(original_size.1, original_size.0);
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > before
                && s.completed_cells_screen().is_some_and(|frame| {
                    let text = frame.contents();
                    frame.size() == original_size
                        && crate::support::main_footer_visible(frame)
                        && text.contains("example/repo#7")
                        && crate::support::controls::control_painted(frame, "[u Metadata]")
                        && text.contains("Locked conversation: false")
                        && text.contains("Milestone: October release")
                        && !text.contains("[Esc Cancel]")
                })
            {
                break;
            }
            assert!(Instant::now() < until, "NEW original narrow Metadata fields/controls/footer after full identity closure\n{}", s.screen());
        }
    }
    unchanged_metadata(s, mouse);
}
fn reopened_metadata_v70(s: &mut Session, before: usize, expected: &Value) {
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            crate::support::main_footer_visible(frame)
                && crate::support::controls::control_painted(frame, "[u Metadata]")
                && crate::support::controls::control_painted(frame, "[p PR actions]")
                && !text.contains("[Esc Cancel]")
                && !text.contains("Verifying original Conversation")
                && !text.contains("Loading PR Metadata")
                && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                    |body| {
                        body.contains("example/repo#7")
                            && body.contains(expected["title"].as_str().unwrap())
                            && body.contains("Source: https://github.invalid/example/repo/pull/7")
                            && body.contains("Your repository permissions: read true • write true")
                            && body.contains("Locked conversation: false")
                            && body.contains("Milestone: October release")
                    },
                )
        },
        "NEW completed actual Metadata before any assertion-driven scrolling",
        Duration::from_secs(8),
    );
}
fn no_publication(s: &mut Session, f: &Fixture, steps: &[GhStep], count: usize) {
    assert_eq!(mutation_count(&exact_current(s, f, steps, count)), 0);
    assert!(receipts(f).is_empty());
}
fn disabled_confirm(s: &mut Session, f: &Fixture, steps: &[GhStep], mouse: bool, count: usize) {
    let title = form_title("Title");
    if !mouse {
        s.focus_completed_owned_action_v1(
            &title,
            "[Confirm action]",
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            FOOTER,
            &[],
        );
    }
    let frame = s.completed_cells_screen().unwrap();
    let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
        frame,
        &title,
        "[Confirm action]",
        &[],
    )
    .expect("Real disabled Confirm in SAME owned current Form");
    assert!(
        !frame.cell(y, x).unwrap().underline(),
        "No prepared/useful approval must remain visibly disabled"
    );
    if mouse {
        s.click(x, y)
    } else {
        s.send_key(b"\r")
    };
    // A disabled click need not change pixels. It never grants a prepared
    // request, and the complete literal owner plus independent journal prove it.
    s.wait_for_owned_completed_action(&title, "[Preview]", &[]);
    no_publication(s, f, steps, count);
}
fn edit_title_again(s: &mut Session, mouse: bool) {
    let title = form_title("Title");
    let after = s.completed_cells_end();
    action(s, mouse, &title, "[Preview]", FOOTER);
    ready(
        s,
        after,
        &title,
        &[
            "PR INTENT • Preview first",
            "Title:",
            "[Keep draft / Close]",
        ],
    );
    if !mouse {
        // Preview is immediately after the single real text field. BackTab
        // selects that field; do not type into a footer after leaving Preview.
        let after = s.completed_cells_end();
        s.send_key(b"\x1b[Z");
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after
                && s.completed_cells_screen().is_some_and(|frame| {
                    crate::support::form_field_value_position(frame, &title, "Title")
                        .is_some_and(|(x, y)| frame.cell(y, x).is_some_and(|c| c.underline()))
                })
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "Actual underlined Title after BackTab\n{}",
                s.screen()
            );
        }
    }
}
fn preview_literal(s: &mut Session, mouse: bool, text: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    for _ in 0..32 {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_screen().is_some_and(|frame| {
            crate::support::overlay_body(frame, &form_title("Title"))
                .is_some_and(|body| body.contains(text))
        }) {
            return;
        }
        let after = s.completed_cells_end();
        if mouse {
            s.wheel(15, 12, true)
        } else {
            s.send_key(b"\x1b[B")
        };
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after {
                break;
            }
            assert!(
                Instant::now() < until,
                "NEW actual preview scroll\n{}",
                s.screen()
            );
        }
    }
    panic!(
        "Exact current/requested Preview literal absent: {text}\n{}",
        s.screen()
    );
}
pub(super) fn preview_restart(mouse: bool) {
    use ghtui::providers::gh::PrMetadataTextFieldV25 as Field;
    const RETAINED: &str = "Editor retained metadata title café 雪";
    let f = fixture();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let original = canonical(target(
        "Native collaboration",
        "A fixture description",
        "main",
    ));
    let updated = canonical(target(RETAINED, "A fixture description", "main"));
    let mut steps = startup(&original);
    let mut nochange = Vec::new();
    for _ in 0..2 {
        steps.extend(phase(&original));
        let opened = steps.len();
        steps.extend(phase(&original)[..5].iter().cloned());
        nochange.push((opened, steps.len()));
    }
    steps.extend(base_read(&original, true, None));
    let base_open = steps.len();
    steps.extend(phase(&original)[..5].iter().cloned());
    let base_nochange = steps.len();
    steps.extend(phase(&original));
    let opened = steps.len();
    steps.extend(phase(&original));
    let first_preview = steps.len();
    steps.extend(phase(&original));
    let second_preview = steps.len();
    steps.extend(phase(&original));
    let editor_preview = steps.len();
    steps.extend(startup(&original));
    let restart_startup = steps.len();
    steps.extend(phase(&original));
    let restart_open = steps.len();
    steps.extend(phase(&original));
    let restart_preview = steps.len();
    steps.extend(phase(&original));
    steps.extend(post(&original, &updated, "title", RETAINED));
    let end = steps.len();
    assert_eq!(
        (
            base_open,
            base_nochange,
            opened,
            first_preview,
            second_preview,
            editor_preview,
            restart_startup,
            restart_open,
            restart_preview,
            end
        ),
        (63, 68, 77, 86, 95, 104, 129, 138, 147, 172)
    );
    f.scripted_gh(&steps);
    let editor = f.root.join("metadata fixture editor.sh");
    fs::write(&editor,format!("#!/bin/sh\nstty -a > \"$GH_FIXTURE_DIR/metadata-editor-termios.txt\"\nprintf '%s' '{RETAINED}' > \"$1\"\n")).unwrap();
    let command = format!("sh '{}'", editor.display());
    let mut s = Session::launch_binary(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &f,
        120,
        40,
        &[("VISUAL", ""), ("EDITOR", &command)],
    );
    enter_metadata(&mut s, mouse);
    for (field, (opened, refused)) in ["Title", "Description"].into_iter().zip(nochange) {
        open_field(&mut s, &f, mouse, field, opened);
        let before = s.completed_cells_end();
        action(&mut s, mouse, &form_title(field), "[Preview]", FOOTER);
        s.wait_for_command_count(&f, refused);
        ready(
            &mut s,
            before,
            &form_title(field),
            &[
                "No change: requested metadata exactly matches current state",
                "[Keep draft / Close]",
            ],
        );
        close_text(&mut s, mouse, field);
        no_publication(&mut s, &f, &steps, refused);
        preserve_git(&f, &head, &index, &status);
    }
    open_base(&mut s, &f, mouse, base_open, "main");
    let before = s.completed_cells_end();
    base_action(&mut s, mouse, "[Preview]", BASE_FORM);
    s.wait_for_command_count(&f, base_nochange);
    ready(
        &mut s,
        before,
        BASE_TITLE,
        &[
            "Selected Base branch is unchanged • no request sent",
            "[Esc Close]",
        ],
    );
    base_action(&mut s, mouse, "[Esc Close]", BASE_FORM);
    s.wait_for_overlay_close(BASE_TITLE);
    no_publication(&mut s, &f, &steps, base_nochange);
    preserve_git(&f, &head, &index, &status);
    open_field(&mut s, &f, mouse, "Title", opened);
    replace_field(&mut s, mouse, "Title", "Unapproved draft title");
    disabled_confirm(&mut s, &f, &steps, mouse, opened);
    preview(&mut s, &f, mouse, "Title", first_preview);
    preview_literal(&mut s, mouse, "Current Title");
    preview_literal(&mut s, mouse, "Native collaboration");
    preview_literal(&mut s, mouse, "Requested Title");
    preview_literal(&mut s, mouse, "Unapproved draft title");
    resize(&mut s, 80, 24, &form_title("Title"), FOOTER);
    preview_literal(&mut s, mouse, "Unapproved draft title");
    edit_title_again(&mut s, mouse);
    replace_field(&mut s, mouse, "Title", "Edited after preview title");
    disabled_confirm(&mut s, &f, &steps, mouse, first_preview);
    preview(&mut s, &f, mouse, "Title", second_preview);
    let after = s.completed_cells_end();
    action(
        &mut s,
        mouse,
        &form_title("Title"),
        "[External editor]",
        FOOTER,
    );
    ready(
        &mut s,
        after,
        &form_title("Title"),
        &[RETAINED, "PR INTENT • Preview first", "[Preview]"],
    );
    let modes = fs::read_to_string(f.root.join("gh-fixture/metadata-editor-termios.txt")).unwrap();
    assert!(modes
        .split_whitespace()
        .any(|v| v == "icanon" || v == "icanon;"));
    assert!(!modes.contains("-icanon"));
    disabled_confirm(&mut s, &f, &steps, mouse, second_preview);
    preview(&mut s, &f, mouse, "Title", editor_preview);
    preview_literal(&mut s, mouse, RETAINED);
    close_text(&mut s, mouse, "Title");
    wait_draft(&mut s, &f, &original, Field::Title, RETAINED);
    no_publication(&mut s, &f, &steps, editor_preview);
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    let mut restarted = Session::launch(&f, 80, 24);
    enter_metadata(&mut restarted, mouse);
    exact_current(&mut restarted, &f, &steps, restart_startup);
    open_field(&mut restarted, &f, mouse, "Title", restart_open);
    restarted.wait_for_screen(RETAINED);
    restarted.wait_for_screen("PR INTENT • Preview first");
    disabled_confirm(&mut restarted, &f, &steps, mouse, restart_open);
    preview(&mut restarted, &f, mouse, "Title", restart_preview);
    preview_literal(&mut restarted, mouse, "Current Title");
    preview_literal(&mut restarted, mouse, "Native collaboration");
    preview_literal(&mut restarted, mouse, "Requested Title");
    preview_literal(&mut restarted, mouse, RETAINED);
    resize(&mut restarted, 120, 40, &form_title("Title"), FOOTER);
    let stamp = restarted.completed_cells_end();
    action(
        &mut restarted,
        mouse,
        &form_title("Title"),
        "[Confirm action]",
        FOOTER,
    );
    restarted.wait_for_command_count(&f, end);
    metadata_return(&mut restarted, stamp);
    unchanged_metadata(&mut restarted, mouse);
    let actual = exact_current(&mut restarted, &f, &steps, end);
    assert_eq!(mutation_count(&actual), 1);
    confirmed_text(&f, &original, Field::Title, RETAINED, false);
    actual_projection(&f, &updated, 1);
    preserve_git(&f, &head, &index, &status);
    restarted.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_current(&mut restarted, &f, &steps, end), actual);
    confirmed_text(&f, &original, Field::Title, RETAINED, false);
    preserve_git(&f, &head, &index, &status);
}
pub(super) fn happy(mouse: bool) {
    use ghtui::providers::gh::PrMetadataTextFieldV25 as Field;
    let f = fixture();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let original = canonical(target(
        "Native collaboration",
        "A fixture description",
        "main",
    ));
    let titled = canonical(target(TITLE, "A fixture description", "main"));
    let described = canonical(target(TITLE, BODY, "main"));
    let rebased = canonical(target(TITLE, BODY, BASE));
    let mut steps = startup(&original);
    steps.extend(phase(&original));
    let cancelled_open = steps.len();
    steps.extend(phase(&original));
    let cancelled_preview = steps.len();
    let mut endpoints = Vec::new();
    for (before, after, field, value) in [
        (&original, &titled, "title", TITLE),
        (&titled, &described, "body", BODY),
    ] {
        steps.extend(phase(before));
        let opened = steps.len();
        steps.extend(phase(before));
        let prepared = steps.len();
        steps.extend(phase(before));
        steps.extend(post(before, after, field, value));
        let confirmed = steps.len();
        steps.extend(section(after));
        endpoints.push((opened, prepared, confirmed, steps.len()));
    }
    steps.extend(base_read(&described, true, None));
    let base_open = steps.len();
    steps.extend(base_read(&described, false, Some(BASE)));
    let base_preview = steps.len();
    steps.extend(base_read(&described, false, Some(BASE)));
    steps.extend(post(&described, &rebased, "base", BASE));
    let base_confirmed = steps.len();
    steps.extend(section(&rebased));
    let total = steps.len();
    assert_eq!(
        (
            cancelled_open,
            cancelled_preview,
            base_open,
            base_preview,
            base_confirmed,
            total
        ),
        (34, 43, 159, 170, 197, 207)
    );
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata(&mut s, mouse);
    open_field(&mut s, &f, mouse, "Title", cancelled_open);
    replace_field(&mut s, mouse, "Title", "Cancelled title stays local");
    preview(&mut s, &f, mouse, "Title", cancelled_preview);
    close_text(&mut s, mouse, "Title");
    wait_draft(
        &mut s,
        &f,
        &original,
        Field::Title,
        "Cancelled title stays local",
    );
    assert!(receipts(&f).is_empty());
    assert_eq!(
        mutation_count(&exact_current(&mut s, &f, &steps, cancelled_preview)),
        0
    );
    preserve_git(&f, &head, &index, &status);
    for (n, (before, after, field, value)) in [
        (&original, &titled, "Title", TITLE),
        (&titled, &described, "Description", BODY),
    ]
    .into_iter()
    .enumerate()
    {
        let (opened, prepared, confirmed, section_end) = endpoints[n];
        open_field(&mut s, &f, mouse, field, opened);
        if n == 0 {
            s.wait_for_screen("Cancelled title stays local");
        }
        replace_field(&mut s, mouse, field, value);
        preview(&mut s, &f, mouse, field, prepared);
        resize(&mut s, 80, 24, &form_title(field), FOOTER);
        resize(&mut s, 120, 40, &form_title(field), FOOTER);
        let stamp = s.completed_cells_end();
        action(
            &mut s,
            mouse,
            &form_title(field),
            "[Confirm action]",
            FOOTER,
        );
        s.wait_for_command_count(&f, confirmed);
        metadata_return(&mut s, stamp);
        confirmed_text(
            &f,
            before,
            if n == 0 {
                Field::Title
            } else {
                Field::Description
            },
            value,
            false,
        );
        actual_projection(&f, after, n + 1);
        unchanged_metadata(&mut s, mouse);
        let commands = exact_current(&mut s, &f, &steps, confirmed);
        assert_eq!(mutation_count(&commands), n + 1);
        choose(&mut s, mouse, "[c Conversation]", b"c");
        s.wait_for_screen(after["body"].as_str().unwrap().lines().next().unwrap());
        s.wait_for_screen("Existing starter comment");
        s.wait_for_screen("@octocat");
        choose(&mut s, mouse, "[o Summary]", b"o");
        s.wait_for_screen(&format!(
            "feature → {}",
            after["base"]["ref"].as_str().unwrap()
        ));
        s.wait_for_screen(HEAD);
        assert_eq!(
            exact_current(&mut s, &f, &steps, confirmed),
            commands,
            "Prepared Conversation/Summary are local, never resend"
        );
        let before_metadata = s.completed_cursorless_end();
        choose(&mut s, mouse, "[u Metadata]", b"u");
        s.wait_for_command_count(&f, section_end);
        reopened_metadata_v70(&mut s, before_metadata, after);
        unchanged_metadata(&mut s, mouse);
        exact_current(&mut s, &f, &steps, section_end);
        preserve_git(&f, &head, &index, &status);
    }
    open_base(&mut s, &f, mouse, base_open, BASE);
    preview_base(&mut s, &f, mouse, base_preview);
    resize(&mut s, 80, 24, BASE_TITLE, BASE_PREVIEW);
    resize(&mut s, 120, 40, BASE_TITLE, BASE_PREVIEW);
    let stamp = s.completed_cells_end();
    base_action(&mut s, mouse, "[Confirm action]", BASE_PREVIEW);
    s.wait_for_command_count(&f, base_confirmed);
    metadata_return(&mut s, stamp);
    unchanged_metadata(&mut s, mouse);
    confirmed_base(&f, &described);
    actual_projection(&f, &rebased, 3);
    choose(&mut s, mouse, "[c Conversation]", b"c");
    s.wait_for_screen(BODY.lines().next().unwrap());
    s.wait_for_screen("Existing starter comment");
    choose(&mut s, mouse, "[o Summary]", b"o");
    s.wait_for_screen(&format!("feature → {BASE}"));
    s.wait_for_screen(HEAD);
    let before_metadata = s.completed_cursorless_end();
    choose(&mut s, mouse, "[u Metadata]", b"u");
    s.wait_for_command_count(&f, total);
    reopened_metadata_v70(&mut s, before_metadata, &rebased);
    unchanged_metadata(&mut s, mouse);
    let commands = exact_current(&mut s, &f, &steps, total);
    assert_eq!(mutation_count(&commands), 3);
    for (row, (field, value)) in commands.iter().filter(|r| is_mutation(r)).zip([
        ("title", TITLE),
        ("body", BODY),
        ("base", BASE),
    ]) {
        let hex = row["stdin_hex"].as_str().unwrap();
        let bytes = (0..hex.len())
            .step_by(2)
            .map(|n| u8::from_str_radix(&hex[n..n + 2], 16).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap(),
            json!({field:value}),
            "Only actual selected field is transmitted"
        );
    }
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_current(&mut s, &f, &steps, total), commands);
    confirmed_text(&f, &original, Field::Title, TITLE, false);
    confirmed_text(&f, &titled, Field::Description, BODY, false);
    confirmed_base(&f, &described);
    actual_projection(&f, &rebased, 3);
    preserve_git(&f, &head, &index, &status);
}
fn hidden_failure(s: &mut Session, after: usize, cols: u16, rows: u16, context_return: bool) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                frame.size() == (rows, cols)
                    && crate::support::main_footer_visible(frame)
                    && text.contains("Confirmed; visibility unavailable; no resend")
                    && (!context_return || text.contains("DURABLY CONFIRMED"))
                    && [
                        " u Metadata ",
                        "[Submit]",
                        "[Confirm action]",
                        "[Keep draft / Close]",
                        "Native collaboration",
                        TITLE,
                        "@triager",
                        "@reviewer",
                        "October release",
                        "example/repo#7",
                    ]
                    .iter()
                    .all(|old| !text.contains(old))
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "Actual complete confirmed/private-hidden Metadata failure frame\n{}",
            s.screen()
        );
    }
}
fn complete_current_sql_v65(f: &Fixture) -> Vec<String> {
    let db = Connection::open_with_flags(cache_path(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    let tables = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut result = Vec::new();
    for table in tables {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|column| row.get::<_, rusqlite::types::Value>(column))
                    .collect::<Result<Vec<_>, _>>()
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
pub(super) fn failure(mouse: bool, account_change: bool) {
    let f = fixture();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let database = cache_path(&f);
    let intent = json!({"query":"original UID42 metadata view intent","repository":REPOSITORY});
    Cache::open(&database)
        .unwrap()
        .save_view(&scope(), "metadata-private-view", &intent)
        .unwrap();
    let original = canonical(target(
        "Native collaboration",
        "A fixture description",
        "main",
    ));
    let titled = canonical(target(TITLE, "A fixture description", "main"));
    let mut steps = startup(&original);
    steps.extend(phase(&original));
    let opened = steps.len();
    steps.extend(phase(&original));
    let prepared = steps.len();
    steps.extend(phase(&original));
    let mut paid = post(&original, &titled, "title", TITLE);
    if account_change {
        paid[15].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    } else {
        paid[12].status = 1;
        paid[12].stdout.clear();
        paid[12].stderr = b"HTTP 403 Forbidden: fixture context access revoked".to_vec();
        paid.truncate(13);
    }
    steps.extend(paid);
    let end = steps.len();
    assert_eq!(end, if account_change { 68 } else { 65 });
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata(&mut s, mouse);
    let old = s.locate("Native collaboration").unwrap();
    open_field(&mut s, &f, mouse, "Title", opened);
    replace_field(&mut s, mouse, "Title", TITLE);
    preview(&mut s, &f, mouse, "Title", prepared);
    let before = s.completed_cells_end();
    action(
        &mut s,
        mouse,
        &form_title("Title"),
        "[Confirm action]",
        FOOTER,
    );
    s.wait_for_command_count(&f, end);
    hidden_failure(&mut s, before, 120, 40, !account_change);
    let transport = exact_current(&mut s, &f, &steps, end);
    assert_eq!(mutation_count(&transport), 1);
    let receipt = confirmed_text(
        &f,
        &original,
        ghtui::providers::gh::PrMetadataTextFieldV25::Title,
        TITLE,
        true,
    );
    let before = s.completed_cells_end();
    s.resize(80, 24);
    hidden_failure(&mut s, before, 80, 24, !account_change);
    // Old underlay coordinates/PR-actions key do not grant source authority.
    let before = s.completed_cells_end();
    if mouse {
        s.click(old.0.min(70), old.1.min(18));
    } else {
        s.send_key(b"p");
    }
    hidden_failure(&mut s, before, 80, 24, !account_change);
    assert_eq!(exact_current(&mut s, &f, &steps, end), transport);
    let cache = Cache::open(&database).unwrap();
    assert_eq!(
        cache.saved_view(&scope(), "metadata-private-view").unwrap(),
        Some(intent)
    );
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.saved_views(&other).unwrap().is_empty());
    let db = Connection::open(&database).unwrap();
    for table in ["pr_write_attempts", "pr_write_scopes"] {
        let count: u64 = db
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE host=?1 AND principal=?2"),
                params![other.host, other.login],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "Changed numeric account cannot inherit publication state"
        );
    }
    assert_eq!(
        confirmed_text(
            &f,
            &original,
            ghtui::providers::gh::PrMetadataTextFieldV25::Title,
            TITLE,
            true
        ),
        receipt
    );
    preserve_git(&f, &head, &index, &status);
    if !account_change {
        const RETURN: &str = "Confirmed Conversation • read-only refresh";
        const CONTROLS: &[&str] = &["[r Refresh PR]", "[Esc Back to Inbox]"];
        // Same-account context denial hides private details, not the actual
        // pre-existing Inbox queue. Back strips every detail/read ticket and
        // restores that genuine cached row without making it a fresh source.
        // The cached Inbox table row: PR cell then TITLE cell on one line.
        let cached_pr = format!("{REPOSITORY}#7 ");
        let cached_title = original["title"].as_str().unwrap();
        let before_back_sql = complete_current_sql_v65(&f);
        let before = s.completed_cells_end();
        action(&mut s, mouse, RETURN, CONTROLS[1], CONTROLS);
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > before
                && s.completed_cells_screen().is_some_and(|frame| {
                    let text = frame.contents();
                    crate::support::main_footer_visible(frame)
                        && text.lines().any(|line| {
                            line.find(&cached_pr).is_some_and(|at| {
                                line[at + cached_pr.len()..]
                                    .trim_start()
                                    .starts_with(cached_title)
                            })
                        })
                        && text.contains("Cached Inbox only")
                        && !text.contains(RETURN)
                        && [
                            "[r Refresh PR]",
                            "[Confirm action]",
                            "[Keep draft / Close]",
                            TITLE,
                            original["body"].as_str().unwrap(),
                            "Edit PR Title",
                            "PR INTENT",
                            "Your repository",
                            " u Metadata ",
                            "@triager",
                            "@reviewer",
                        ]
                        .iter()
                        .all(|v| !text.contains(v))
                })
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "Actual Return Back -> NEW genuine cached Inbox with old private details hidden before Quit\n{}",
                s.screen()
            );
        }
        assert_eq!(exact_current(&mut s, &f, &steps, end), transport);
        assert_eq!(complete_current_sql_v65(&f), before_back_sql);
        assert_eq!(
            confirmed_text(
                &f,
                &original,
                ghtui::providers::gh::PrMetadataTextFieldV25::Title,
                TITLE,
                true
            ),
            receipt
        );
    }
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_current(&mut s, &f, &steps, end), transport);
    assert_eq!(
        confirmed_text(
            &f,
            &original,
            ghtui::providers::gh::PrMetadataTextFieldV25::Title,
            TITLE,
            true
        ),
        receipt
    );
    preserve_git(&f, &head, &index, &status);
}
pub(super) fn concurrent(mouse: bool) {
    let f = fixture();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let original = canonical(target(
        "Native collaboration",
        "A fixture description",
        "main",
    ));
    let changed = canonical(target(
        "Someone else changed the title",
        "A fixture description",
        "main",
    ));
    let mut steps = startup(&original);
    steps.extend(phase(&original));
    let opened = steps.len();
    steps.extend(phase(&original));
    let previewed = steps.len();
    let mut refusing = phase(&changed);
    refusing.truncate(5);
    steps.extend(refusing);
    let refused = steps.len();
    steps.extend(startup(&changed));
    let fresh_page = steps.len();
    steps.extend(phase(&changed));
    let reopened = steps.len();
    assert_eq!(
        (opened, previewed, refused, fresh_page, reopened),
        (34, 43, 48, 73, 82)
    );
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata(&mut s, mouse);
    exact_current(&mut s, &f, &steps, 25);
    open_field(&mut s, &f, mouse, "Title", opened);
    replace_field(&mut s, mouse, "Title", TITLE);
    preview(&mut s, &f, mouse, "Title", previewed);
    let before = s.completed_cursorless_end();
    action(
        &mut s,
        mouse,
        &form_title("Title"),
        "[Confirm action]",
        FOOTER,
    );
    s.wait_for_command_count(&f, refused);
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            crate::support::main_footer_visible(frame)
                && frame
                    .contents()
                    .contains("Original Conversation unavailable")
                && !frame.contents().contains("[Confirm action]")
        },
        "Actual changed-field refusal retires old Form without marker/write",
        Duration::from_secs(8),
    );
    let no_writes = exact_current(&mut s, &f, &steps, refused);
    assert_eq!(mutation_count(&no_writes), 0);
    assert!(receipts(&f).is_empty());
    wait_draft(
        &mut s,
        &f,
        &original,
        ghtui::providers::gh::PrMetadataTextFieldV25::Title,
        TITLE,
    );
    // Explicit new actual producer read observes the concurrent title and
    // restores only inert requested text, never the old Preview/permission.
    choose(&mut s, mouse, "1 Inbox", b"1");
    s.wait_for_screen("Inbox • focused view");
    choose(&mut s, mouse, "[r Refresh]", b"r");
    s.wait_for_screen("Someone else changed the title");
    s.wait_for_screen("Fresh • fetched");
    choose(&mut s, mouse, "Someone else changed the title", b"\r");
    s.wait_for_screen("1 comments");
    let opening = s.completed_cells_end();
    choose(&mut s, mouse, "[c Conversation]", b"c");
    s.wait_for_screen("Existing starter comment");
    enter_metadata_from_conversation(&mut s, mouse, opening);
    unchanged_metadata(&mut s, mouse);
    exact_current(&mut s, &f, &steps, fresh_page);
    open_field(&mut s, &f, mouse, "Title", reopened);
    s.wait_for_screen(TITLE);
    s.wait_for_screen("PR INTENT • Preview first");
    assert!(!s.screen().contains("READ-ONLY PREVIEW"));
    s.resize(80, 24);
    s.wait_for_screen(TITLE);
    let before = s.completed_cursorless_end();
    action(
        &mut s,
        mouse,
        &form_title("Title"),
        "[Keep draft / Close]",
        FOOTER,
    );
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            crate::support::main_footer_visible(frame)
                && !frame.contents().contains("[Keep draft / Close]")
        },
        "Pure Close retains requested draft after explicit fresh read",
        Duration::from_secs(8),
    );
    unchanged_metadata(&mut s, mouse);
    let final_rows = exact_current(&mut s, &f, &steps, reopened);
    assert_eq!(mutation_count(&final_rows), 0);
    assert!(receipts(&f).is_empty());
    wait_draft(
        &mut s,
        &f,
        &original,
        ghtui::providers::gh::PrMetadataTextFieldV25::Title,
        TITLE,
    );
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_current(&mut s, &f, &steps, reopened), final_rows);
    preserve_git(&f, &head, &index, &status);
}
