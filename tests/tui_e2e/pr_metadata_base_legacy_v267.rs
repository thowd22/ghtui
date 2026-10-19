//! Dedicated migration of the ORIGINAL Base happy pair, not a newer test alias.
//! Pure current protocol/ownership helpers are copied from the authenticated
//! V266 current_v51 fixture; this child retains the old pair's unique journey.
use super::*;
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};
#[path = "pr_metadata_base_recovery_v303.rs"]
pub(super) mod recovery_v303;
const QUERY:&str="query($id:ID!){node(id:$id){__typename ... on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanUpdate repository{id nameWithOwner}}}}";
const BASE_TITLE: &str = "Change PR Base • example/repo#7";
const BASE_PICKER: &str = "PR metadata choices · Base branch";
const BASE_FORM: &[&str] = &["[F2 Choose branch]", "[Preview]", "[Esc Close]"];
const BASE_PREVIEW: &[&str] = &["[Preview]", "[Confirm action]", "[Keep draft / Close]"];
const PICKER_CONTROLS: &[&str] = &[
    "[Enter Choose]",
    "[Esc Back]",
    "[Ctrl-U Clear]",
    "[PgUp]",
    "[PgDn]",
];
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Database {
    path: Vec<u8>,
    device: u64,
    inode: u64,
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
            // Missing-restored-selection error keeps Preview focused. Move
            // directly back to its preceding F2 control, not across the two
            // separately painted Error controls outside this ordinary set.
            if label == "[F2 Choose branch]"
                && frame.contents().contains("Choose an actual member with F2")
            {
                s.send_key(b"\x1b[Z");
            } else {
                s.send_key(b"\t");
            }
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
            &format!("BEFORE Base main at {}", "b".repeat(40)),
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
    assert_eq!(calls.len(), 24);
    calls
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

fn exact_database(f: &Fixture, database: &Database) {
    use std::os::unix::{ffi::OsStrExt, fs::MetadataExt};
    let path = fs::canonicalize(cache_path(f)).unwrap();
    let metadata = fs::metadata(&path).unwrap();
    assert_eq!(database.path, path.as_os_str().as_bytes());
    assert_eq!(database.device, metadata.dev());
    assert_eq!(database.inode, metadata.ino());
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
                    && text.contains("[u Metadata]")
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
                    && text.contains("[u Metadata]")
                    && text.contains("@octocat PR description")
                    && text.contains("A fixture description")
                    && text.contains("@octocat comment #9 • 2026-10-13T00:00:00Z")
                    && text.contains("Existing starter comment")
                    && !text.contains("[Esc Cancel]")
                    && !text.contains("Verifying original Conversation")
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
                    && text.contains("[u Metadata]")
                    && text.contains("[p PR actions]")
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
                        && text.contains("[u Metadata]")
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

fn ready(s: &mut Session, after: usize, title: &str, labels: &[&str]) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, title)
                        .is_some_and(|body| labels.iter().all(|label| body.contains(label)))
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW actual owned Base {title}/{labels:?}\n{}",
            s.screen()
        );
    }
}
fn unselected(s: &mut Session, f: &Fixture, original: &Value, after: usize) {
    use crate::support::completed_navigation_controls_v1::{owned_focus_v1, FocusAttributeV1};
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                // The SAME complete topmost Form must show BOTH exact labels
                // and their actual empty value rows, not merely omit a name.
                if !crate::support::main_footer_visible(frame)
                    || owned_focus_v1(
                        frame,
                        BASE_TITLE,
                        "[Esc Close]",
                        FocusAttributeV1::Inverse,
                        BASE_FORM,
                        &[],
                    )
                    .is_none()
                {
                    return false;
                }
                let Some(body) = crate::support::overlay_body(frame, BASE_TITLE) else {
                    return false;
                };
                if !BASE_FORM.iter().all(|control| body.contains(control)) {
                    return false;
                }
                let rows = body.lines().collect::<Vec<_>>();
                let mut values = Vec::new();
                for label in [
                    "Selected existing base branch (F2 Choose):",
                    "Exact observed selected tip (read only):",
                ] {
                    let matches = rows
                        .iter()
                        .enumerate()
                        .filter(|(_, row)| row.trim() == label)
                        .map(|(index, _)| index)
                        .collect::<Vec<_>>();
                    if matches.len() != 1
                        || !rows
                            .get(matches[0] + 1)
                            .is_some_and(|row| row.chars().all(char::is_whitespace))
                    {
                        return false;
                    }
                    values.push(matches[0] + 1);
                }
                values[0] < values[1]
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "NEW same-owned Base Form with BOTH actual empty selection/tip value rows\n{}",
            s.screen()
        );
    }
    let blank = json!({"version":17,"values":{"base":"","tip":""},"publication":null});
    s.try_wait_for_condition(
        |_| {
            saved(f, &base_key(original))
                .is_some_and(|body| serde_json::from_str::<Value>(&body).unwrap() == blank)
        },
        "Actual accepted autosave settles exact blank v17 selection/tip and no publication",
        Duration::from_secs(8),
    )
    .unwrap();
    assert_eq!(draft_blank(f, original), blank);
}
fn exact_current(s: &mut Session, f: &Fixture, expected: &[GhStep], count: usize) -> Vec<Value> {
    s.wait_for_command_count(f, count);
    let until = Instant::now() + Duration::from_secs(8);
    let rows = loop {
        let raw = s.inspect_command_journal(f);
        let prefix = raw.rfind('\n').map_or("", |last| &raw[..=last]);
        let rows = prefix
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert!(rows.len() <= count, "Unexpected additional strict command");
        if rows.len() == count {
            break rows;
        }
        assert!(
            Instant::now() < until,
            "All exact expected journal lines must complete"
        );
        s.poll_output(Duration::from_millis(1));
    };
    for (index, (actual, wanted)) in rows.iter().zip(&expected[..count]).enumerate() {
        assert_eq!(actual["step"], index);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(wanted.args));
        assert_eq!(
            actual["stdin_hex"],
            wanted
                .stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
    }
    rows
}
fn no_write(s: &mut Session, f: &Fixture, steps: &[GhStep], count: usize) {
    assert_eq!(mutation_count(&exact_current(s, f, steps, count)), 0);
    assert!(receipts(f).is_empty());
}
fn picker_action(s: &mut Session, mouse: bool, label: &str) {
    if !mouse {
        let notices = [
            format!("{BASE_PICKER} · Filter focused"),
            format!("{BASE_PICKER} · Rows focused"),
        ];
        s.focus_completed_owned_action_v1(
            BASE_PICKER,
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            PICKER_CONTROLS,
            &notices.each_ref().map(String::as_str),
        );
    }
    let frame = s
        .completed_cells_screen()
        .expect("Actual completed owned picker");
    let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
        frame,
        BASE_PICKER,
        label,
        &[],
    )
    .expect("Whole actual picker control");
    if mouse {
        s.click(x, y);
    } else {
        assert!(frame.cell(y, x).unwrap().inverse());
        s.send_key(b"\r");
    }
}
fn query(s: &mut Session, mouse: bool, text: &str) {
    if mouse {
        let caption = if s.completed_cells_screen().is_some_and(|frame| {
            crate::support::overlay_body(frame, BASE_PICKER)
                .is_some_and(|body| body.contains("Filter focused:"))
        }) {
            "Filter focused:"
        } else {
            "Filter:"
        };
        s.click_visible_action(caption);
    }
    let after = s.completed_cells_end();
    s.send_key(b"\x01");
    s.paste(text);
    ready(s, after, BASE_PICKER, &[&format!("Filter focused: {text}")]);
}
fn select(s: &mut Session, mouse: bool) {
    query(s, mouse, BASE);
    s.wait_for_screen(&format!("◆ eligible · {BASE}"));
    s.wait_for_screen(TIP);
    let after = s.completed_cells_end();
    if mouse {
        s.click_visible_action(&format!("◆ eligible · {BASE}"));
    } else {
        s.send_key(b"\t");
    }
    ready(
        s,
        after,
        &format!("{BASE_PICKER} · Rows focused"),
        PICKER_CONTROLS,
    );
    picker_action(s, mouse, "[Enter Choose]");
    s.wait_for_overlay_close(BASE_PICKER);
    s.wait_for_owned_completed_action(BASE_TITLE, "[Preview]", &[]);
}
fn open_picker(s: &mut Session, f: &Fixture, mouse: bool, count: usize) {
    let after = s.completed_cells_end();
    menu(s, mouse, "Change base branch");
    s.wait_for_command_count(f, count);
    ready(s, after, BASE_PICKER, PICKER_CONTROLS);
}
fn close(s: &mut Session, mouse: bool, controls: &[&str]) {
    let after = s.completed_cells_end();
    base_action(s, mouse, controls.last().unwrap(), controls);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::main_footer_visible(frame)
                    && !frame.contents().contains("[Esc Cancel]")
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains("[u Metadata]")
                                && body.contains("Locked conversation: false")
                                && !body.contains(BASE_TITLE)
                                && !body.contains(BASE_PICKER)
                        },
                    )
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW actual Metadata after pure Base Close\n{}",
            s.screen()
        );
    }
}
fn saved(f: &Fixture, key: &str) -> Option<String> {
    Connection::open_with_flags(cache_path(f),rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap()
        .query_row("SELECT body FROM drafts WHERE host='github.invalid' AND login='user-id:42' AND resource=?1",[key],|row|row.get(0))
        .optional().unwrap()
}
fn draft(s: &mut Session, f: &Fixture, raw: &Value) -> String {
    let key = base_key(raw);
    s.try_wait_for_condition(
        |_| {
            saved(f, &key).is_some_and(|raw| {
                let envelope: Value = serde_json::from_str(&raw).unwrap();
                envelope
                    == json!({"version":17,"values":{"base":BASE,"tip":TIP},"publication":null})
            })
        },
        "Exact current v17 Base+tip draft persisted without selection or approval",
        Duration::from_secs(8),
    )
    .unwrap();
    saved(f, &key).unwrap()
}

/// Only the original two Base happy names call this dedicated full journey.
pub(super) fn happy(mouse: bool) {
    let f = fixture();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let original = canonical(original_parent());
    assert_eq!(original["base"]["ref"], "main");
    assert_eq!(original["base"]["sha"], "b".repeat(40));
    assert_eq!(original["head"]["sha"], HEAD);
    let mut updated = original.clone();
    updated["base"]["ref"] = json!(BASE);
    updated["base"]["sha"] = json!(TIP);
    for field in [
        "id",
        "node_id",
        "number",
        "title",
        "body",
        "head",
        "labels",
        "assignees",
        "requested_reviewers",
        "requested_teams",
        "milestone",
    ] {
        assert_eq!(
            updated[field], original[field],
            "Unmodified exact original field {field}"
        );
    }
    let mut steps = startup(&original);
    assert_eq!(steps.len(), 24);
    steps.extend(base_read(&original, true, None));
    let first_open = steps.len();
    steps.extend(base_read(&original, false, Some(BASE)));
    let first_preview = steps.len();
    steps.extend(startup(&original));
    let restart = steps.len();
    steps.extend(base_read(&original, true, None));
    let reopened = steps.len();
    steps.extend(base_read(&original, false, Some(BASE)));
    let second_preview = steps.len();
    steps.extend(base_read(&original, false, Some(BASE)));
    steps.extend(post(&original, &updated, "base", BASE));
    let published = steps.len();
    steps.extend(section(&updated));
    let section_end = steps.len();
    steps.extend(base_read(&updated, true, None));
    let nochange_open = steps.len();
    let mut nochange = base_read(&updated, false, Some(BASE));
    assert_eq!(nochange.len(), 11);
    for (call, endpoint, method) in [
        (&nochange[1], "user", "GET"),
        (&nochange[2], "repos/example/repo", "GET"),
        (&nochange[3], "repos/example/repo/pulls/7", "GET"),
        (&nochange[4], "graphql", "POST"),
    ] {
        assert_eq!(
            &call.args[..6],
            [
                "api",
                endpoint,
                "--hostname",
                "github.invalid",
                "--method",
                method
            ]
        );
    }
    assert_eq!(nochange[0].args, ["--version"]);
    assert_eq!(&nochange[4].args[6..], ["--input", "-"]);
    assert_eq!(
        serde_json::from_slice::<Value>(&nochange[4].stdin).unwrap(),
        json!({"query":QUERY,"variables":{"id":NODE}})
    );
    // Actual same-ref raw/capability observation returns refusal-only
    // BaseNoChange before selected branch reads or closing/source issuance.
    nochange.truncate(5);
    steps.extend(nochange);
    let total = steps.len();
    assert_eq!(
        (
            first_open,
            first_preview,
            restart,
            reopened,
            second_preview,
            published,
            section_end,
            nochange_open,
            total
        ),
        (34, 45, 69, 79, 90, 117, 127, 137, 142),
        "Core6/current protocol phase inventory, not legacy count offsets"
    );
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata(&mut s, mouse);
    open_picker(&mut s, &f, mouse, first_open);
    query(&mut s, mouse, "branch-that-is-not-observed");
    s.wait_for_screen("No observed matching choices; scope unchanged");
    picker_action(&mut s, mouse, "[Enter Choose]");
    s.wait_for_screen("no observed matching member");
    no_write(&mut s, &f, &steps, first_open);
    query(&mut s, mouse, BASE);
    s.wait_for_screen(TIP);
    resize(&mut s, 80, 24, BASE_PICKER, PICKER_CONTROLS);
    s.wait_for_screen(TIP);
    let pending_row = s.completed_cells_end();
    if mouse {
        s.click_visible_action(&format!("◆ eligible · {BASE}"));
    } else {
        s.send_key(b"\t");
    }
    ready(
        &mut s,
        pending_row,
        &format!("{BASE_PICKER} · Rows focused"),
        PICKER_CONTROLS,
    );
    no_write(&mut s, &f, &steps, first_open);
    preserve_git(&f, &head, &index, &status);
    let after = s.completed_cells_end();
    picker_action(&mut s, mouse, "[Esc Back]");
    unselected(&mut s, &f, &original, after);
    no_write(&mut s, &f, &steps, first_open);
    preserve_git(&f, &head, &index, &status);
    let after = s.completed_cells_end();
    base_action(&mut s, mouse, "[F2 Choose branch]", BASE_FORM);
    ready(&mut s, after, BASE_PICKER, PICKER_CONTROLS);
    select(&mut s, mouse);
    preview_base(&mut s, &f, mouse, first_preview);
    no_write(&mut s, &f, &steps, first_preview);
    close(&mut s, mouse, BASE_PREVIEW);
    let durable = draft(&mut s, &f, &original);
    no_write(&mut s, &f, &steps, first_preview);
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);

    let mut s = Session::launch(&f, 80, 24);
    enter_metadata(&mut s, mouse);
    exact_current(&mut s, &f, &steps, restart);
    open_picker(&mut s, &f, mouse, reopened);
    assert_eq!(saved(&f, &base_key(&original)), Some(durable.clone()));
    let after = s.completed_cells_end();
    picker_action(&mut s, mouse, "[Esc Back]");
    ready(&mut s, after, BASE_TITLE, BASE_FORM);
    s.wait_for_screen(BASE);
    s.wait_for_screen(TIP);
    let after = s.completed_cells_end();
    base_action(&mut s, mouse, "[Preview]", BASE_FORM);
    ready(
        &mut s,
        after,
        BASE_TITLE,
        &["Choose an actual member with F2", "[Preview]"],
    );
    no_write(&mut s, &f, &steps, reopened);
    assert_eq!(saved(&f, &base_key(&original)), Some(durable));
    let after = s.completed_cells_end();
    base_action(&mut s, mouse, "[F2 Choose branch]", BASE_FORM);
    ready(&mut s, after, BASE_PICKER, PICKER_CONTROLS);
    select(&mut s, mouse);
    preview_base(&mut s, &f, mouse, second_preview);
    no_write(&mut s, &f, &steps, second_preview);
    let after = s.completed_cells_end();
    base_action(&mut s, mouse, "[Confirm action]", BASE_PREVIEW);
    s.wait_for_command_count(&f, published);
    metadata_return(&mut s, after);
    unchanged_metadata(&mut s, mouse);
    let commands = exact_current(&mut s, &f, &steps, published);
    assert_eq!(mutation_count(&commands), 1);
    let mutation = commands.iter().find(|row| is_mutation(row)).unwrap();
    assert_eq!(
        mutation["args"],
        json!([
            "api",
            "repos/example/repo/pulls/7",
            "--hostname",
            "github.invalid",
            "--method",
            "PATCH",
            "--input",
            "-"
        ])
    );
    let hex = mutation["stdin_hex"].as_str().unwrap();
    let bytes = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        json!({"base":BASE})
    );
    confirmed_base(&f, &original);
    actual_projection(&f, &updated, 1);
    assert!(saved(&f, &base_key(&original)).is_none());
    preserve_git(&f, &head, &index, &status);
    choose(&mut s, mouse, "[o Summary]", b"o");
    s.wait_for_screen("feature → release/october");
    s.wait_for_screen(HEAD);
    assert_eq!(
        exact_current(&mut s, &f, &steps, published),
        commands,
        "Summary and unchanged H are local, never resend"
    );
    let after = s.completed_cells_end();
    choose(&mut s, mouse, "[u Metadata]", b"u");
    s.wait_for_command_count(&f, section_end);
    ready(
        &mut s,
        after,
        "Inbox • focused view ─",
        &["Locked conversation: false", "Your repository permissions:"],
    );
    unchanged_metadata(&mut s, mouse);
    open_picker(&mut s, &f, mouse, nochange_open);
    select(&mut s, mouse);
    // Settle the known inert v17 draft BEFORE the refusal phase. This exact
    // whole-database preimage includes the one legitimate new selected draft;
    // no arbitrary draft/mapping/table filter or poststate expected value.
    let inert = draft(&mut s, &f, &updated);
    let nochange_sql = raw_sql_v278(&f);
    let after = s.completed_cells_end();
    base_action(&mut s, mouse, "[Preview]", BASE_FORM);
    s.wait_for_command_count(&f, total);
    ready(
        &mut s,
        after,
        BASE_TITLE,
        &["Selected Base branch is unchanged", "[Preview]"],
    );
    let all = exact_current(&mut s, &f, &steps, total);
    assert_eq!(mutation_count(&all), 1);
    assert_eq!(raw_sql_v278(&f), nochange_sql);
    assert_eq!(saved(&f, &base_key(&updated)), Some(inert.clone()));
    confirmed_base(&f, &original);
    // No Confirm button is advertised for NoChange. The real key still runs
    // its normal guarded Submit semantics; never synthesize an invisible hit.
    let after = s.completed_cells_end();
    s.send_key(b"\x1b[13;5u");
    ready(
        &mut s,
        after,
        BASE_TITLE,
        &["Preview", "[F2 Choose branch]"],
    );
    assert!(!s
        .completed_cells_screen()
        .unwrap()
        .contents()
        .contains("[Confirm action]"));
    assert_eq!(exact_current(&mut s, &f, &steps, total), all);
    assert_eq!(raw_sql_v278(&f), nochange_sql);
    assert_eq!(saved(&f, &base_key(&updated)), Some(inert.clone()));
    confirmed_base(&f, &original);
    actual_projection(&f, &updated, 1);
    close(&mut s, mouse, BASE_FORM);
    unchanged_metadata(&mut s, mouse);
    preserve_git(&f, &head, &index, &status);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_current(&mut s, &f, &steps, total), all);
    assert_eq!(raw_sql_v278(&f), nochange_sql);
    assert_eq!(saved(&f, &base_key(&updated)), Some(inert));
    confirmed_base(&f, &original);
    preserve_git(&f, &head, &index, &status);
}

fn catalogue_pages(raw: &Value, pages: Vec<Value>) -> Vec<GhStep> {
    let count = pages.len();
    assert!((1..=10).contains(&count));
    let mut calls = phase(raw);
    calls.splice(
        5..5,
        pages.into_iter().enumerate().map(|(page, rows)| {
            step(
                &format!("repos/example/repo/branches?per_page=100&page={}", page + 1),
                "GET",
                rows,
                false,
            )
        }),
    );
    assert_eq!(
        calls.len(),
        9 + count,
        "Core6/rawPR/capability/closingraw plus exact bounded pages"
    );
    calls
}
pub(super) fn partial(mouse: bool) {
    let f = fixture();
    let original = canonical(original_parent());
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut rows = vec![
        json!({"name":"unverified","commit":{"sha":null}}),
        branch(BASE, TIP),
    ];
    rows.extend((2..1000).map(|n| branch(&format!("topic/{n:04}"), TIP)));
    assert_eq!(rows.len(), 1000);
    let pages = rows.chunks(100).map(|rows| json!(rows)).collect::<Vec<_>>();
    assert_eq!(pages.len(), 10);
    let mut steps = startup(&original);
    steps.extend(catalogue_pages(&original, pages));
    let opened = steps.len();
    steps.extend(base_read(&original, false, Some(BASE)));
    let end = steps.len();
    assert_eq!((opened, end), (43, 54));
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata(&mut s, mouse);
    open_picker(&mut s, &f, mouse, opened);
    s.wait_for_screen("Partial catalogue: observed choices only");
    s.wait_for_screen("Pages 10 / 10 requested • rows 1000 read / 1 excluded / 0 duplicate");
    s.wait_for_screen("! disabled · unverified");
    picker_action(&mut s, mouse, "[Enter Choose]");
    s.wait_for_screen("Branch name/full tip unavailable; no target guessed");
    no_write(&mut s, &f, &steps, opened);
    query(&mut s, mouse, "not-observed-by-the-cap");
    s.wait_for_screen("No observed matching choices; scope unchanged");
    picker_action(&mut s, mouse, "[Enter Choose]");
    s.wait_for_screen("Choose disabled: no observed matching member");
    no_write(&mut s, &f, &steps, opened);
    select(&mut s, mouse);
    preview_base(&mut s, &f, mouse, end);
    no_write(&mut s, &f, &steps, end);
    close(&mut s, mouse, BASE_PREVIEW);
    draft(&mut s, &f, &original);
    no_write(&mut s, &f, &steps, end);
    preserve_git(&f, &head, &index, &status);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    s.quit_and_assert_restored_terminal(mouse);
    no_write(&mut s, &f, &steps, end);
    preserve_git(&f, &head, &index, &status);
}
pub(super) fn empty_and_forbidden(mouse: bool) {
    let f = fixture();
    let original = canonical(original_parent());
    assert_eq!(original["base"]["ref"], "main");
    assert_eq!(original["base"]["sha"], "b".repeat(40));
    assert_eq!(original["head"]["sha"], HEAD);
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut steps = startup(&original);
    steps.extend(catalogue_pages(&original, vec![json!([])]));
    let empty = steps.len();
    let mut denied = catalogue_pages(&original, vec![json!([])]);
    assert!(denied[5].args[1].ends_with("branches?per_page=100&page=1"));
    denied[5].stdout.clear();
    denied[5].status = 1;
    denied[5].stderr = b"gh: Forbidden PRIVATE_BRANCH_403_CANARY (HTTP 403)\n".to_vec();
    steps.extend(denied);
    let end = steps.len();
    assert_eq!((empty, end), (34, 44));
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata(&mut s, mouse);
    open_picker(&mut s, &f, mouse, empty);
    s.wait_for_screen("Complete catalogue: observed indexed choices");
    s.wait_for_screen("Pages 1 / 1 requested • rows 0 read / 0 excluded / 0 duplicate");
    s.wait_for_screen("No observed matching choices; scope unchanged");
    picker_action(&mut s, mouse, "[Enter Choose]");
    s.wait_for_screen("Choose disabled: no observed matching member");
    no_write(&mut s, &f, &steps, empty);
    preserve_git(&f, &head, &index, &status);
    let after = s.completed_cells_end();
    picker_action(&mut s, mouse, "[Esc Back]");
    unselected(&mut s, &f, &original, after);
    no_write(&mut s, &f, &steps, empty);
    preserve_git(&f, &head, &index, &status);
    close(&mut s, mouse, BASE_FORM);
    // F2 would reopen the SAME retained empty catalogue without IO. Only a
    // new actual menu OPEN asks the genuine fresh producer for another read.
    open_picker(&mut s, &f, mouse, end);
    s.wait_for_screen("Unavailable: inspect reasons below; Choose is disabled");
    s.wait_for_screen("Branch endpoint forbidden (HTTP 403)");
    assert!(!s.screen().contains("Complete catalogue:"));
    assert!(!s.screen().contains("PRIVATE_BRANCH_403_CANARY"));
    picker_action(&mut s, mouse, "[Enter Choose]");
    s.wait_for_screen("Choose disabled: no observed matching member");
    no_write(&mut s, &f, &steps, end);
    preserve_git(&f, &head, &index, &status);
    let after = s.completed_cells_end();
    picker_action(&mut s, mouse, "[Esc Back]");
    unselected(&mut s, &f, &original, after);
    no_write(&mut s, &f, &steps, end);
    preserve_git(&f, &head, &index, &status);
    close(&mut s, mouse, BASE_FORM);
    unchanged_metadata(&mut s, mouse);
    let blank = json!({"version":17,"values":{"base":"","tip":""},"publication":null});
    s.try_wait_for_condition(
        |_| {
            saved(&f, &base_key(&original))
                .is_some_and(|body| serde_json::from_str::<Value>(&body).unwrap() == blank)
        },
        "Actual ordered blank draft Save settles after pure Close",
        Duration::from_secs(8),
    )
    .unwrap();
    assert_eq!(draft_blank(&f, &original), blank);
    no_write(&mut s, &f, &steps, end);
    preserve_git(&f, &head, &index, &status);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    s.quit_and_assert_restored_terminal(mouse);
    no_write(&mut s, &f, &steps, end);
    preserve_git(&f, &head, &index, &status);
}
fn draft_blank(f: &Fixture, raw: &Value) -> Value {
    serde_json::from_str(
        &saved(f, &base_key(raw)).expect("Actual normal current blank draft retained on Close"),
    )
    .unwrap()
}

pub(super) fn changed_account(mouse: bool) {
    let f = fixture();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let original = canonical(original_parent());
    let mut steps = startup(&original);
    let mut changed = base_read(&original, true, None);
    assert_eq!(changed.len(), 10);
    assert_eq!(changed[7].args[1], "repos/example/repo");
    assert_eq!(changed[9].args[1], "user");
    assert_eq!(
        serde_json::from_slice::<Value>(&changed[1].stdout).unwrap(),
        json!({"id":42,"login":"octocat"})
    );
    // The actual LAST numeric observation changes identity, not a repository
    // response mislabeled as user and not a different displayed login.
    changed[9].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    steps.extend(changed);
    assert_eq!(steps.len(), 34);
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata(&mut s, mouse);
    let old = crate::support::completed_navigation_controls_v1::owned_action_position(
        s.completed_cells_screen().unwrap(),
        "Inbox • focused view ─",
        "[u Metadata]",
        &[],
    )
    .expect("Actual old Metadata action before numeric account drift");
    let after = s.completed_cells_end();
    menu(&mut s, mouse, "Change base branch");
    s.wait_for_command_count(&f, steps.len());
    account_hidden(&mut s, after);
    no_write(&mut s, &f, &steps, steps.len());
    let commands = exact_current(&mut s, &f, &steps, steps.len());
    preserve_git(&f, &head, &index, &status);
    assert!(saved(&f, &base_key(&original)).is_none());
    let after = s.completed_cells_end();
    s.resize(80, 24);
    account_hidden(&mut s, after);
    let after = s.completed_cells_end();
    if mouse {
        s.click(old.0.min(70), old.1.min(18));
    } else {
        s.send_key(b"\r");
    }
    s.focus_footer_control("[q Quit]");
    account_hidden(&mut s, after);
    assert_eq!(exact_current(&mut s, &f, &steps, steps.len()), commands);
    let cache = Cache::open(&cache_path(&f)).unwrap();
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.saved_views(&other).unwrap().is_empty());
    let sql =
        Connection::open_with_flags(cache_path(&f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    for table in ["pr_write_attempts", "pr_write_scopes"] {
        let count: u64 = sql
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE host=?1 AND principal=?2"),
                params![other.host, other.login],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }
    assert!(saved(&f, &base_key(&original)).is_none());
    preserve_git(&f, &head, &index, &status);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_current(&mut s, &f, &steps, steps.len()), commands);
    assert!(receipts(&f).is_empty());
    preserve_git(&f, &head, &index, &status);
}
fn account_hidden(s: &mut Session, after: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains(
                            "Original Conversation retired • refresh explicitly • nothing resent",
                        )
                        },
                    )
                    && ![
                        BASE_PICKER,
                        BASE_TITLE,
                        "[F4 Choose]",
                        "[F2 Choose branch]",
                        "[u Metadata]",
                        "[Confirm action]",
                        "[Esc Cancel]",
                        "Native collaboration",
                        REPOSITORY,
                        "@triager",
                        "@reviewer",
                        "October release",
                        BASE,
                        TIP,
                        HEAD,
                    ]
                    .iter()
                    .any(|private| frame.contents().contains(private))
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW complete account-changed private-retired frame; no old choice/metadata after stale input\n{}",
            s.screen()
        );
    }
}

fn raw_sql_v278(f: &Fixture) -> Vec<String> {
    let db = Connection::open_with_flags(cache_path(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    let schema = db
        .prepare("SELECT type,name,COALESCE(sql,'') FROM sqlite_master ORDER BY type,name")
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let mut result = Vec::new();
    for (kind, name, sql) in schema {
        result.push(format!("schema:{kind}:{name}:{sql}"));
        if kind != "table" {
            continue;
        }
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|column| row.get::<_, rusqlite::types::Value>(column))
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
fn changed_tip_retired_v278(s: &mut Session, after: usize, expected_status: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                let (rows, columns) = frame.size();
                crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains(
                            "Original Conversation retired • refresh explicitly • nothing resent",
                        )
                        },
                    )
                    && frame
                        .contents_between(
                            rows.saturating_sub(2),
                            0,
                            rows.saturating_sub(2),
                            columns,
                        )
                        .trim_end()
                        == expected_status
                    && ![
                        BASE_TITLE,
                        BASE_PICKER,
                        "[F2 Choose branch]",
                        "[Confirm action]",
                        "[Esc Cancel]",
                        "[u Metadata]",
                        "Native collaboration",
                        "A fixture description",
                        REPOSITORY,
                        BASE,
                        TIP,
                        CHANGED_TIP,
                        HEAD,
                        "@triager",
                        "@reviewer",
                        "October release",
                    ]
                    .iter()
                    .any(|private| text.contains(private))
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW exact Base Confirm failure/private retirement; no stale choice or empty/loading substitute\n{}",
            s.screen()
        );
    }
}
pub(super) fn stale_tip_observed_input(mouse: bool, controlled: bool) {
    let f = fixture();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let original = canonical(original_parent());
    assert_eq!(original["base"]["ref"], "main");
    let mut steps = startup(&original);
    steps.extend(base_read(&original, true, None));
    let opened = steps.len();
    steps.extend(base_read(&original, false, Some(BASE)));
    let previewed = steps.len();
    let mut changed = base_read(&original, false, Some(BASE));
    assert_eq!(
        changed[5].args[1],
        "repos/example/repo/branches/release%2Foctober"
    );
    changed[5].stdout = serde_json::to_vec(&branch(BASE, CHANGED_TIP)).unwrap();
    changed.truncate(6);
    steps.extend(changed);
    let refused = steps.len();
    assert_eq!((opened, previewed, refused), (34, 45, 51));
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata(&mut s, mouse);
    open_picker(&mut s, &f, mouse, opened);
    select(&mut s, mouse);
    preview_base(&mut s, &f, mouse, previewed);
    let durable = draft(&mut s, &f, &original);
    no_write(&mut s, &f, &steps, previewed);
    let before = raw_sql_v278(&f);
    let obsolete = crate::support::completed_navigation_controls_v1::owned_action_position(
        s.completed_cells_screen().unwrap(),
        BASE_TITLE,
        "[Confirm action]",
        &[],
    )
    .expect("Actual old Preview Confirm hit before its source is retired");
    if controlled {
        s.hold_observed_output_after_next_write_v1(Duration::from_millis(200));
    }
    let after = s.completed_cells_end();
    base_action(&mut s, mouse, "[Confirm action]", BASE_PREVIEW);
    if controlled {
        let writes = s.finish_observed_output_hold_v1();
        assert_eq!(
            crate::support::receiver_delivery_gate_v1::maximum_unacknowledged(&writes),
            1,
            "ONE actual successful PTY input per acknowledged completed focus transition; actual journal: {writes:?}"
        );
    }
    s.wait_for_command_count(&f, refused);
    let failure_status =
        "Original Conversation unavailable • nothing resent • draft/journal retained";
    changed_tip_retired_v278(&mut s, after, failure_status);
    assert!(s
        .completed_cells_screen()
        .unwrap()
        .contents()
        .contains("Original Conversation unavailable • nothing resent • draft/journal retained"));
    no_write(&mut s, &f, &steps, refused);
    let commands = exact_current(&mut s, &f, &steps, refused);
    assert_eq!(saved(&f, &base_key(&original)), Some(durable.clone()));
    assert_eq!(raw_sql_v278(&f), before);
    preserve_git(&f, &head, &index, &status);
    let after = s.completed_cells_end();
    if mouse {
        s.click(obsolete.0, obsolete.1);
    } else {
        s.send_key(b"\r");
    }
    // The full initial refusal was already observed above. Once that Form is
    // retired, stale Enter activates the current Inbox navigation hit, whose
    // normal Action::View sets this exact status; no old Confirm is revived.
    let after_probe_status = if mouse {
        failure_status
    } else {
        "Inbox • focused view"
    };
    changed_tip_retired_v278(&mut s, after, after_probe_status);
    assert_eq!(exact_current(&mut s, &f, &steps, refused), commands);
    assert_eq!(raw_sql_v278(&f), before);
    let after = s.completed_cells_end();
    s.resize(80, 24);
    changed_tip_retired_v278(&mut s, after, after_probe_status);
    assert_eq!(saved(&f, &base_key(&original)), Some(durable));
    assert_eq!(raw_sql_v278(&f), before);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    preserve_git(&f, &head, &index, &status);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_current(&mut s, &f, &steps, refused), commands);
    assert_eq!(raw_sql_v278(&f), before);
    assert!(receipts(&f).is_empty());
    preserve_git(&f, &head, &index, &status);
}

fn failed_base_open_v279(s: &mut Session, after: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    && text.contains("Original Conversation unavailable")
                    && text.contains("nothing resent")
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains("example/repo#7")
                                && body.contains("Locked conversation: false")
                                && body.contains("Milestone: October release")
                        },
                    )
                    && text.contains("[u Metadata]")
                    && ![
                        BASE_TITLE,
                        BASE_PICKER,
                        "[F2 Choose branch]",
                        "[Enter Choose]",
                        "[Confirm action]",
                        "[Esc Cancel]",
                        "Verifying original Conversation",
                        "Complete catalogue:",
                        "Branch: release/october",
                    ]
                    .iter()
                    .any(|forbidden| text.contains(forbidden))
                    && !text.chars().any(|ch| matches!(ch, '\u{1}' | '\u{2}'))
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW actual failed Base opening must preserve original Metadata, not admit a chooser/source or a loading/empty substitute\n{}",
            s.screen()
        );
    }
}
pub(super) fn raw_stderr(mouse: bool) {
    let f = fixture();
    let original = canonical(original_parent());
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut steps = startup(&original);
    let mut opening = base_read(&original, true, None);
    assert_eq!(opening.len(), 10);
    assert_eq!(
        opening[5].args[1],
        "repos/example/repo/branches?per_page=100&page=1"
    );
    opening[5].stderr = b"\x01\x02".repeat(70_000);
    assert_eq!(opening[5].stderr.len(), 140_000);
    assert!(opening[5].stderr.len() > 128 * 1024);
    // Positive typed OutputLimit propagates out of this exact child. No
    // closing raw PR/repo/version/user and no separately renewed identity.
    opening.truncate(6);
    steps.extend(opening);
    assert_eq!(steps.len(), 30);
    f.scripted_gh(&steps);
    let mut s = Session::launch(&f, 120, 40);
    enter_metadata(&mut s, mouse);
    no_write(&mut s, &f, &steps, 24);
    assert_eq!(saved(&f, &base_key(&original)), None);
    let mut expected_sql = raw_sql_v278(&f);
    // The actual Metadata read already owns generation1. Opening Base reserves
    // exactly generation2 before its catalogue child, even if that child hits
    // OutputLimit. This local invalidation is not a completed Source or write.
    // The independent literal tuple pins the one permitted row; all other
    // schema, sequences and typed rows remain in the complete preimage.
    let reservation = format!(
        "personal-pr-source-v13/{:x}",
        Sha256::digest(
            serde_json::to_vec(&(
                json!({"host":"github.invalid","login":"user-id:42"}),
                410u64,
                "R_metadata_410",
                1007u64,
                "PR_example/repo_7",
                7u64,
            ))
            .unwrap()
        )
    );
    assert_eq!(
        reservation,
        "personal-pr-source-v13/1a2b6de1378cbda0ac2ca29eab19e1a29af0240edb1704b89dc16933f104cb35"
    );
    let reservation_row = |generation| {
        format!(
            "sync_reservations:{:?}",
            [
                rusqlite::types::Value::Text("github.invalid".into()),
                rusqlite::types::Value::Text("user-id:42".into()),
                rusqlite::types::Value::Text(reservation.clone()),
                rusqlite::types::Value::Integer(generation),
                rusqlite::types::Value::Integer(1),
            ]
        )
    };
    assert_eq!(
        expected_sql
            .iter()
            .filter(|row| row.starts_with("sync_reservations:"))
            .count(),
        1
    );
    let row = expected_sql
        .iter()
        .position(|row| row == &reservation_row(1))
        .expect("Exact original Metadata reservation generation1, active1");
    expected_sql[row] = reservation_row(2);
    let visible = crate::support::overlay_body(
        s.completed_cells_screen().unwrap(),
        "Inbox • focused view ─",
    )
    .expect("Actual original Metadata before Base opening");
    let after = s.completed_cells_end();
    menu(&mut s, mouse, "Change base branch");
    s.wait_for_command_count(&f, 30);
    failed_base_open_v279(&mut s, after);
    assert_eq!(
        crate::support::overlay_body(
            s.completed_cells_screen().unwrap(),
            "Inbox • focused view ─"
        )
        .unwrap(),
        visible,
        "Original Metadata body and fields remain exactly visible at the same viewport"
    );
    no_write(&mut s, &f, &steps, 30);
    let commands = exact_current(&mut s, &f, &steps, 30);
    assert_eq!(
        commands[29]["args"][1],
        "repos/example/repo/branches?per_page=100&page=1"
    );
    assert_eq!(saved(&f, &base_key(&original)), None);
    assert_eq!(raw_sql_v278(&f), expected_sql);
    preserve_git(&f, &head, &index, &status);
    let after = s.completed_cells_end();
    s.resize(80, 24);
    failed_base_open_v279(&mut s, after);
    // Every old actor/team/label/milestone assertion remains actual visible
    // evidence, using real viewport movement rather than hidden model data.
    unchanged_metadata(&mut s, mouse);
    assert_eq!(exact_current(&mut s, &f, &steps, 30), commands);
    assert_eq!(raw_sql_v278(&f), expected_sql);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(exact_current(&mut s, &f, &steps, 30), commands);
    assert_eq!(raw_sql_v278(&f), expected_sql);
    assert_eq!(saved(&f, &base_key(&original)), None);
    assert!(receipts(&f).is_empty());
    preserve_git(&f, &head, &index, &status);
}
