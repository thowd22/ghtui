//! Actual binary, same original Core/Collaboration owner and one-field publisher.
//! Every journey compares the entire strict gh journal and independent SQL/Git;
//! input helpers are task-local and do not weaken existing read-only observers.
use super::*;
use ghtui::providers::gh::PrMetadataTextFieldV25 as Field;
#[path = "pr_metadata_manual_refresh_v45.rs"]
mod manual_refresh_v45;
#[path = "pr_metadata_members_v50.rs"]
mod members_v50;
#[path = "pr_metadata_tab_v51.rs"]
mod metadata_tab_v51;

const QUERY: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanUpdate repository{id nameWithOwner}}}}";
const NEW_TITLE: &str = "Native exact metadata title 界";
const NEW_BODY: &str = "Native exact description café 界\nSecond metadata line";
const POISON: &str = "NEVER_ADOPT_FROM_METADATA_PREVIEW";
const FOOTER: &[&str] = &[
    "[Preview]",
    "[External editor]",
    "[Confirm action]",
    "[Keep draft / Close]",
];
const HISTORY_FOOTER: &[&str] = &["[Inspect current]", "[Acknowledge local]", "[Esc Close]"];

fn control(s: &mut Session, mouse: bool, title: &str, label: &str, controls: &[&str]) {
    if !mouse && label == "[Preview]" {
        let editor_focused = s.completed_cells_screen().is_some_and(|frame| {
            crate::support::completed_navigation_controls_v1::owned_action_position(
                frame,
                title,
                "[External editor]",
                &[],
            )
            .is_some_and(|(x, y)| frame.cell(y, x).is_some_and(|cell| cell.inverse()))
        });
        if editor_focused {
            // This actual editor-return focus has Preview immediately before
            // it. Do not traverse the body (not a footer control) to get there.
            let after = s.completed_cells_end();
            s.send_key(b"\x1b[Z");
            let until = Instant::now() + Duration::from_secs(8);
            loop {
                s.poll_output(Duration::from_millis(1));
                let focused = s.completed_cells_end() > after
                    && s.completed_cells_screen().is_some_and(|frame| {
                        crate::support::overlay_body(frame, title)
                            .is_some_and(|body| controls.iter().all(|label| body.contains(label)))
                            && crate::support::completed_navigation_controls_v1::owned_action_position(
                                frame, title, label, &[],
                            )
                            .is_some_and(|(x, y)| frame.cell(y, x).is_some_and(|cell| cell.inverse()))
                    });
                if focused {
                    s.send_key(b"\r");
                    return;
                }
                assert!(
                    Instant::now() < until,
                    "Exact NEW Preview focus after External editor BackTab\n{}",
                    s.screen()
                );
            }
        }
    }
    if !matches!(label, "[Confirm action]" | "[Acknowledge local]") {
        return super::control(s, mouse, title, label, controls);
    }
    // Visible labels are not enabled actions. Useful Doc paint occurs after
    // the first footer calculation, and resize revokes its previous proof.
    if !mouse {
        s.focus_completed_owned_action_v1(title, label, FocusAttributeV1::Inverse, controls, &[]);
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
                frame
                    .cell(*y, *x)
                    .is_some_and(|cell| cell.underline() && (mouse || cell.inverse()))
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
            "Exact useful enabled metadata control absent: {title}/{label}\n{}",
            s.screen()
        );
    }
}

fn resize_form(s: &mut Session, cols: u16, rows: u16, field: Field) {
    let title = form_title(field);
    let before_size = s.completed_cells_screen().map(|frame| frame.size());
    let after = s.completed_cells_end();
    // Centered Forms retain the real global footer. The full-screen modal
    // helper instead requires controls at the terminal's bottom border.
    s.resize(cols, rows);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let complete = (before_size == Some((rows, cols)) || s.completed_cells_end() > after)
            && s.completed_cells_screen().is_some_and(|frame| {
                frame.size() == (rows, cols)
                    && crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, &title).is_some_and(|body| {
                        body.lines().any(|row| {
                            let mut rest = row;
                            FOOTER.iter().all(|label| {
                                if let Some(offset) = rest.find(label) {
                                    rest = &rest[offset + label.len()..];
                                    true
                                } else {
                                    false
                                }
                            })
                        })
                    })
            });
        if complete {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW exact centered metadata Form/all four ordered controls at {cols}x{rows}\n{}",
            s.screen()
        );
    }
}

fn requested(field: Field) -> &'static str {
    if field == Field::Title {
        NEW_TITLE
    } else {
        NEW_BODY
    }
}
fn menu_label(field: Field) -> &'static str {
    if field == Field::Title {
        "Edit PR title"
    } else {
        "Edit PR description"
    }
}
fn form_title(field: Field) -> String {
    format!("Edit PR {} • {FIRST}#7", field.label())
}
fn history_title(field: Field) -> String {
    format!("Original {} history • {FIRST}#7", field.label())
}
fn command(field: Field) -> &'static str {
    if field == Field::Title {
        "metadata-title-v25"
    } else {
        "metadata-description-v25"
    }
}
fn parent(field: Field, changed: bool) -> Value {
    let mut raw = parent_resource();
    if changed {
        raw[if field == Field::Title {
            "title"
        } else {
            "body"
        }] = json!(requested(field));
    }
    raw
}
fn capability(allowed: Value) -> Value {
    json!({"data":{"node":{"__typename":"PullRequest","id":"GENUINE_CANONICAL_PR_NODE_1007",
        "fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),
        "headRefName":"feature/scoped-open","baseRefName":"main","state":"OPEN","isDraft":false,
        "merged":false,"viewerCanUpdate":allowed,"repository":{"id":"REPO_catalogue_10000","nameWithOwner":FIRST}}}})
}
fn read_phase(field: Field, changed: bool, allowed: Value) -> Vec<GhStep> {
    let raw = parent(field, changed);
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), raw.clone()));
    calls.push(graphql(
        QUERY,
        json!({"id":"GENUINE_CANONICAL_PR_NODE_1007"}),
        capability(allowed),
    ));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), raw));
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.extend(principal(42));
    assert_eq!(
        calls.len(),
        9,
        "six original Core and three measured children"
    );
    calls
}
fn post(field: Field) -> Vec<GhStep> {
    let mut values = serde_json::Map::new();
    values.insert(
        if field == Field::Title {
            "title"
        } else {
            "body"
        }
        .into(),
        json!(requested(field)),
    );
    let mut calls = vec![
        write(
            &format!("repos/{FIRST}/pulls/7"),
            "PATCH",
            Some(Value::Object(values)),
            parent(field, true),
        ),
        get(&format!("repos/{FIRST}/pulls/7"), parent(field, true)),
    ];
    let mut visibility = visibility_with_comments_v19(json!([comment(501, OLD)]));
    visibility[2].stdout = serde_json::to_vec(&parent(field, true)).unwrap();
    visibility[8].stdout = serde_json::to_vec(&parent(field, true)).unwrap();
    calls.extend(visibility);
    assert_eq!(calls.len(), 12);
    calls
}
fn writable(f: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(database(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
        .unwrap()
}
fn independent(f: &Fixture) -> Vec<String> {
    let db = raw(f);
    ["SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'metadata-text-v25/%' ORDER BY host,login,resource",
     "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
     "SELECT json_array(host,login,kind,id,repository,payload,fetched_at,source_updated_at,complete) FROM records WHERE login='user-id:43' OR kind='metadata-independent' ORDER BY host,login,kind,id",
     "SELECT json_array(host,login,source,generation,complete) FROM checkpoints WHERE login='user-id:43' ORDER BY host,login,source",
     "SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"]
        .iter().flat_map(|q|db.prepare(q).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn complete_sql_v46(f: &Fixture) -> Vec<String> {
    let db = raw(f);
    let tables = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([], |row| row.get::<_, String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut result = Vec::new();
    for table in tables {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |row| {
                (0..columns)
                    .map(|index| row.get::<_, rusqlite::types::Value>(index))
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
fn fixture(steps: &[GhStep]) -> Fixture {
    let f = resource_configured(steps);
    for uid in [42, 43] {
        Cache::open(&database(&f))
            .unwrap()
            .put_draft(
                &Scope::github(HOST, Some(uid)).unwrap(),
                "metadata-independent",
                &format!("METADATA_KEPT_{uid}"),
            )
            .unwrap();
        Cache::open(&database(&f))
            .unwrap()
            .put(
                &Scope::github(HOST, Some(uid)).unwrap(),
                &ghtui::storage::Record::new(
                    "metadata-independent",
                    format!("record-{uid}"),
                    FIRST,
                    json!({"private":format!("KEPT_RECORD_{uid}")}),
                ),
            )
            .unwrap();
    }
    f
}
fn launch_with_editor(f: &Fixture, mouse: bool) -> Session {
    let editor = f.root.join("metadata exact editor.sh");
    fs::write(&editor,format!("#!/bin/sh\nstty -a > \"$GH_FIXTURE_DIR/metadata-editor-termios.txt\"\nprintf '%s' '{NEW_BODY}' > \"$1\"\n")).unwrap();
    let command = format!("sh '{}'", editor.display());
    let trace = f.root.join("catalogue-git-trace.jsonl");
    let mut env = vec![
        ("VISUAL", ""),
        ("EDITOR", command.as_str()),
        ("GIT_TRACE2_EVENT", trace.to_str().unwrap()),
    ];
    if mouse {
        env.push(("NO_COLOR", "1"));
    }
    let mut s = Session::launch_binary(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        f,
        if mouse { 80 } else { 120 },
        if mouse { 24 } else { 40 },
        &env,
    );
    s.wait_for_screen("Recent commits");
    s.assert_focused_view("Repository");
    s
}
fn open(s: &mut Session, f: &Fixture, mouse: bool, field: Field, end: usize) {
    let after = s.completed_cells_end();
    crate::pr_actions::menu(s, mouse, menu_label(field));
    s.wait_for_command_count(f, end);
    form_frame(s, after, &form_title(field), FOOTER);
    // Actual author and requester are different genuine numeric observations.
    assert!(s.screen().contains("actual-pr-author"));
    assert!(!s.screen().contains("Some(42)"));
}
fn fill(s: &mut Session, mouse: bool, field: Field, text: &str) {
    if mouse {
        s.click_owned_form_field(&form_title(field), field.label());
    }
    s.send_key(b"\x01");
    s.paste(text);
    // Replacing a scrolled long description retains its editor viewport.
    // Actual Ctrl-Home exposes the first line without changing any body byte.
    let after = s.completed_cells_end();
    s.send_key(b"\x1b[1;5H");
    let mut expected = text.lines().collect::<Vec<_>>();
    expected.extend_from_slice(FOOTER);
    form_frame(s, after, &form_title(field), &expected);
}
fn preview(s: &mut Session, f: &Fixture, mouse: bool, field: Field, end: usize) {
    let after = s.completed_cells_end();
    control(s, mouse, &form_title(field), "[Preview]", FOOTER);
    s.wait_for_command_count(f, end);
    form_frame(
        s,
        after,
        &form_title(field),
        &["READ-ONLY PREVIEW", "[Confirm action]"],
    );
    let after = s.completed_cells_end();
    s.paste(POISON);
    form_frame(
        s,
        after,
        &form_title(field),
        &["READ-ONLY PREVIEW", "[Confirm action]"],
    );
    assert!(!s.screen().contains(POISON));
    s.capture_completed_terminal_v1("metadata-original-real-prepared-preview");
}
fn close(s: &mut Session, mouse: bool, field: Field) {
    let before = s.completed_cursorless_end();
    control(s, mouse, &form_title(field), "[Keep draft / Close]", FOOTER);
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            crate::support::main_footer_visible(frame)
                && !frame.contents().contains("[Confirm action]")
        },
        "Actual metadata Close retains draft without invocation",
        Duration::from_secs(8),
    );
}
fn terminal(s: &mut Session, before: usize, needle: &str) {
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            crate::support::main_footer_visible(frame)
                && frame.contents().contains(needle)
                && !frame.contents().contains("[Confirm action]")
        },
        "Exact terminal metadata state in completed owned page",
        Duration::from_secs(8),
    );
}
fn draft(f: &Fixture) -> String {
    raw(f)
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'metadata-text-v25/%'",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
fn marker(f: &Fixture) -> String {
    raw(f).query_row("SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts",[],|r|r.get(0)).unwrap()
}
fn assert_original(f: &Fixture, field: Field, state: &str, retained: bool) {
    let db = raw(f);
    let (purpose, observed, head): (String, String, String) = db
        .query_row(
            "SELECT command,state,observed_head FROM pr_write_attempts",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (purpose, observed, head),
        (command(field).into(), state.into(), "a".repeat(40))
    );
    let (count, origin): (u64, String) = db
        .query_row(
            "SELECT count(*),origin_json FROM pr_write_metadata_origins_v25",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(count, 1);
    let origin: Value = serde_json::from_str(&origin).unwrap();
    assert_eq!(origin["seed"]["version"], 25);
    assert_eq!(
        origin["seed"]["field"],
        if field == Field::Title {
            "title"
        } else {
            "description"
        }
    );
    assert_eq!(origin["seed"]["repository_id"], 10000);
    assert_eq!(origin["seed"]["pr_id"], 1007);
    assert_eq!(origin["seed"]["pr_node"], "GENUINE_CANONICAL_PR_NODE_1007");
    assert_eq!(origin["seed"]["author_uid"], 61);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource LIKE 'metadata-text-v25/%'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        u64::from(retained)
    );
    if retained {
        let encoded: Value = serde_json::from_str(&draft(f)).unwrap();
        assert_eq!(encoded["values"]["body"], requested(field));
        assert!(encoded["publication"].is_object());
        assert!(encoded.get("approval").is_none());
    }
}
fn finish(
    mut s: Session,
    f: &Fixture,
    mouse: bool,
    steps: &[GhStep],
    git: &GitState,
    config: &[u8],
    stable: &[String],
) {
    assert_eq!(independent(f), stable);
    assert_eq!(&git_state(f), git);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact(&s, f, steps);
    assert_read_only_git(f);
    s.quit_and_assert_restored_terminal(mouse);
}
fn happy(mouse: bool, personal: bool, field: Field) {
    let mut steps = initial_steps(personal);
    let initial = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let opened = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let prepared = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    steps.extend(post(field));
    let f = fixture(&steps);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = if personal {
        launch_with_editor(&f, mouse)
    } else {
        launch(&f, mouse)
    };
    enter(
        &mut s,
        &f,
        personal,
        mouse,
        initial,
        false,
        &steps[..initial],
    );
    if personal {
        let before = s.completed_cursorless_end();
        choose(&mut s, mouse, "[c Conversation]", b"c");
        s.wait_for_new_cursorless_condition(
            before,
            |frame| {
                crate::support::main_footer_visible(frame)
                    && crate::support::controls::control_painted(frame, "[c Conversation]")
            },
            "Actual Personal Conversation, no fabricated Files source",
            Duration::from_secs(8),
        );
    }
    let stable = independent(&f);
    open(&mut s, &f, mouse, field, opened);
    if personal {
        let after = s.completed_cells_end();
        control(
            &mut s,
            mouse,
            &form_title(field),
            "[External editor]",
            FOOTER,
        );
        form_frame(
            &mut s,
            after,
            &form_title(field),
            &[NEW_BODY.lines().next().unwrap(), "[Preview]"],
        );
        let modes =
            fs::read_to_string(f.root.join("gh-fixture/metadata-editor-termios.txt")).unwrap();
        assert!(modes.contains("icanon") && !modes.contains("-icanon"));
    } else {
        fill(&mut s, mouse, field, requested(field));
    }
    // Native child uses both useful geometries without accepting stale paint.
    resize_form(&mut s, 120, 40, field);
    preview(&mut s, &f, mouse, field, prepared);
    resize_form(&mut s, 80, 24, field);
    resize_form(&mut s, 120, 40, field);
    let before = s.completed_cursorless_end();
    control(
        &mut s,
        mouse,
        &form_title(field),
        "[Confirm action]",
        FOOTER,
    );
    s.wait_for_command_count(&f, steps.len());
    terminal(&mut s, before, "Confirmed • fresh PR/search observation");
    assert_original(&f, field, "confirmed", false);
    let payload:String=raw(&f).query_row("SELECT payload FROM records WHERE kind='pr-details' AND id='GENUINE_CANONICAL_PR_NODE_1007'",[],|r|r.get(0)).unwrap();
    let payload: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        payload["summary"]["pr"][if field == Field::Title {
            "title"
        } else {
            "body"
        }],
        requested(field)
    );
    let after = s.completed_cursorless_end();
    choose(&mut s, mouse, "[b Back]", b"b");
    s.wait_for_new_cursorless_condition(
        after,
        |frame| {
            crate::support::main_footer_visible(frame)
                && !crate::support::controls::control_painted(frame, "[b Back]")
        },
        "Pure Back preserves exact refreshed source without new HTTP",
        Duration::from_secs(8),
    );
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}

fn refusal(mouse: bool) {
    // Each subcase is an actual independent application/source, never a DTO
    // permission stub. A missing capability is not inferred as false/true.
    for mode in ["nochange", "denied", "unknown"] {
        let field = Field::Title;
        let mut steps = initial_steps(true);
        let initial = steps.len();
        let mut opening = read_phase(
            field,
            false,
            if mode == "unknown" {
                Value::Null
            } else {
                json!(mode != "denied")
            },
        );
        if mode == "unknown" {
            opening.truncate(5);
        }
        steps.extend(opening);
        let opened = steps.len();
        if mode != "unknown" {
            let mut pre = read_phase(field, false, json!(mode != "denied"));
            pre.truncate(5);
            steps.extend(pre);
        }
        let f = fixture(&steps);
        let git = git_state(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut s = launch(&f, mouse);
        enter(&mut s, &f, true, mouse, initial, false, &steps[..initial]);
        let stable = independent(&f);
        if mode == "unknown" {
            let before = s.completed_cursorless_end();
            crate::pr_actions::menu(&mut s, mouse, menu_label(field));
            s.wait_for_command_count(&f, opened);
            terminal(&mut s, before, "unavailable");
        } else {
            open(&mut s, &f, mouse, field, opened);
            if mode == "denied" {
                fill(&mut s, mouse, field, requested(field));
            }
            let before = s.completed_cells_end();
            control(&mut s, mouse, &form_title(field), "[Preview]", FOOTER);
            s.wait_for_command_count(&f, steps.len());
            form_frame(
                &mut s,
                before,
                &form_title(field),
                &["[Preview]", "[Keep draft / Close]"],
            );
            assert!(!s.screen().contains("READ-ONLY PREVIEW"));
            close(&mut s, mouse, field);
        }
        assert_eq!(
            raw(&f)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn drift(mouse: bool) {
    for changed in [
        "head",
        "field",
        "repository",
        "account",
        "null_to_empty",
        "empty_to_null",
    ] {
        let field = Field::Description;
        let mut steps = initial_steps(true);
        let initial = steps.len();
        let nullable_before = match changed {
            "null_to_empty" => Some(Value::Null),
            "empty_to_null" => Some(json!("")),
            _ => None,
        };
        let mut opening = read_phase(field, false, json!(true));
        if let Some(before) = &nullable_before {
            for index in [3, 5] {
                let mut raw: Value = serde_json::from_slice(&opening[index].stdout).unwrap();
                raw["body"] = before.clone();
                opening[index].stdout = serde_json::to_vec(&raw).unwrap();
            }
        }
        steps.extend(opening);
        let opened = steps.len();
        let mut current = read_phase(field, false, json!(true));
        match changed {
            "head" | "field" | "null_to_empty" | "empty_to_null" => {
                let mut raw = parent(field, false);
                if changed == "head" {
                    raw["head"]["sha"] = json!("c".repeat(40));
                } else if changed == "field" {
                    raw["body"] = json!("INDEPENDENT_NEW_DESCRIPTION");
                } else {
                    let mut first = parent(field, false);
                    first["body"] = nullable_before.clone().unwrap();
                    current[3].stdout = serde_json::to_vec(&first).unwrap();
                    raw["body"] = if changed == "null_to_empty" {
                        json!("")
                    } else {
                        Value::Null
                    };
                }
                current[5].stdout = serde_json::to_vec(&raw).unwrap();
                current.truncate(6);
            }
            "repository" => {
                let mut repo = repository(0);
                repo["id"] = json!(20000);
                current[2].stdout = serde_json::to_vec(&repo).unwrap();
                current.truncate(3);
            }
            "account" => {
                current[8].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap()
            }
            _ => unreachable!(),
        }
        steps.extend(current);
        let f = fixture(&steps);
        let git = git_state(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut s = launch(&f, mouse);
        enter(&mut s, &f, true, mouse, initial, false, &steps[..initial]);
        let stable = independent(&f);
        open(&mut s, &f, mouse, field, opened);
        fill(&mut s, mouse, field, requested(field));
        let before_draft = {
            let until = Instant::now() + Duration::from_secs(8);
            loop {
                let saved: Option<String> = raw(&f)
                    .query_row(
                        "SELECT body FROM drafts WHERE resource LIKE 'metadata-text-v25/%'",
                        [],
                        |r| r.get(0),
                    )
                    .optional()
                    .unwrap();
                if let Some(saved) = saved {
                    let encoded: Value = serde_json::from_str(&saved).unwrap();
                    if encoded["values"]["body"] == requested(field) {
                        assert!(encoded["publication"].is_null());
                        break saved;
                    }
                }
                assert!(
                    Instant::now() < until,
                    "Actual accepted metadata draft commit before any source-loss Preview"
                );
                s.poll_output(Duration::from_millis(2));
            }
        };
        let before_sql = complete_sql_v46(&f);
        let before = s.completed_cells_end();
        control(&mut s, mouse, &form_title(field), "[Preview]", FOOTER);
        s.wait_for_command_count(&f, steps.len());
        if changed != "head" {
            s.wait_for_new_cursorless_condition(
                before,
                |frame| {
                    crate::support::main_footer_visible(frame)
                        && !frame.contents().contains(NEW_BODY.lines().next().unwrap())
                        && !frame.contents().contains("Second metadata line")
                        && !frame.contents().contains(DETAIL_BODY)
                        && !frame.contents().contains(&form_title(field))
                        && !frame.contents().contains("[Confirm action]")
                        && !frame.contents().contains("[Keep draft / Close]")
                },
                "Exact closing field/repository/account/null source loss hides private old Form and body",
                Duration::from_secs(8),
            );
        } else {
            form_frame(
                &mut s,
                before,
                &form_title(field),
                &["[Preview]", "[Keep draft / Close]"],
            );
            assert!(!s.screen().contains("READ-ONLY PREVIEW"));
            close(&mut s, mouse, field);
        }
        assert_eq!(
            draft(&f),
            before_draft,
            "Every source-loss refusal retains the exact accepted draft/ref bytes"
        );
        assert_eq!(
            complete_sql_v46(&f),
            before_sql,
            "Read-only refusal/Close cannot change any SQL table or reservation"
        );
        assert_eq!(
            raw(&f)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}

fn canceled(mouse: bool) {
    let field = Field::Title;
    let mut steps = initial_steps(false);
    let initial = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let opened = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let prepared = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let paid = post(field);
    steps.push(paid[0].clone());
    let held_index = steps.len();
    let mut read = paid[1].clone();
    read.delay_ms = 30_000;
    steps.push(read);
    let f = fixture(&steps);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, false, mouse, initial, false, &steps[..initial]);
    let stable = independent(&f);
    open(&mut s, &f, mouse, field, opened);
    fill(&mut s, mouse, field, requested(field));
    preview(&mut s, &f, mouse, field, prepared);
    control(
        &mut s,
        mouse,
        &form_title(field),
        "[Confirm action]",
        FOOTER,
    );
    s.wait_for_command_count(&f, steps.len());
    let rows = commands(&s, &f);
    let pid = i32::try_from(rows[held_index]["pid"].as_u64().unwrap()).unwrap();
    let original = process_identity(pid).unwrap().unwrap();
    let owned =
        OwnedPtySession::capture(u32::try_from(s.owned_application_pid()).unwrap()).unwrap();
    assert!(owned.owns(&original));
    assert!(!original.zombie);
    let encoded = draft(&f);
    control(
        &mut s,
        mouse,
        &form_title(field),
        "[Esc Cancel]",
        &["[Esc Cancel]"],
    );
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(2));
        let row: Value = serde_json::from_str(&marker(&f)).unwrap();
        assert_eq!(draft(&f), encoded);
        if row[10] == "uncertain" {
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual canceled worker must settle durable uncertainty"
        );
    }
    choose(&mut s, mouse, "2 Repository", b"2");
    s.assert_focused_view("Repository");
    s.wait_for_screen("Recent commits");
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match process_identity(pid) {
            Ok(None) => break,
            Ok(Some(now))
                if (now.uid, now.birth, now.session)
                    != (original.uid, original.birth, original.session) =>
            {
                break
            }
            Ok(Some(_)) => {}
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {}
            Err(e) => panic!("Exact held child identity observation failed: {e}"),
        };
        assert!(
            Instant::now() < until,
            "Original held readback session must be cleaned"
        );
        s.poll_output(Duration::from_millis(2));
    }
    assert!(!s.screen().contains(NEW_TITLE));
    assert_original(&f, field, "uncertain", true);
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}

fn history_open(s: &mut Session, f: &Fixture, mouse: bool, field: Field, end: usize) {
    let before = s.completed_cells_end();
    crate::pr_actions::menu(s, mouse, &format!("Inspect {} history", field.label()));
    s.wait_for_command_count(f, end);
    form_frame(
        s,
        before,
        &history_title(field),
        &[
            "ORIGINAL METADATA HISTORY",
            "[Inspect current]",
            "[Acknowledge local]",
        ],
    );
}
fn restart_recovery(mouse: bool) {
    let field = Field::Description;
    let mut steps = initial_steps(true);
    let first = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let opened = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let prepared = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let mut failed = post(field).remove(0);
    failed.status = 1;
    failed.stdout.clear();
    failed.stderr = b"HTTP 403: actual uncertain one-field invocation".to_vec();
    steps.push(failed);
    let invoked = steps.len();
    steps.extend(initial_steps(true));
    let restarted = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let listed = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let f = fixture(&steps);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, first, false, &steps[..first]);
    let stable = independent(&f);
    open(&mut s, &f, mouse, field, opened);
    fill(&mut s, mouse, field, requested(field));
    preview(&mut s, &f, mouse, field, prepared);
    let before = s.completed_cursorless_end();
    control(
        &mut s,
        mouse,
        &form_title(field),
        "[Confirm action]",
        FOOTER,
    );
    s.wait_for_command_count(&f, invoked);
    terminal(&mut s, before, "Unknown outcome");
    assert_original(&f, field, "uncertain", true);
    let encoded = draft(&f);
    let original = marker(&f);
    exact(&s, &f, &steps[..invoked]);
    s.quit_and_assert_restored_terminal(mouse);
    let mut s = launch(&f, mouse);
    enter(
        &mut s,
        &f,
        true,
        mouse,
        restarted,
        false,
        &steps[..restarted],
    );
    history_open(&mut s, &f, mouse, field, listed);
    assert_eq!(draft(&f), encoded);
    assert_eq!(marker(&f), original);
    // Merely listing never clears ambiguity or creates restored permission.
    let before = s.completed_cells_end();
    control(
        &mut s,
        mouse,
        &history_title(field),
        "[Inspect current]",
        HISTORY_FOOTER,
    );
    s.wait_for_command_count(&f, steps.len());
    form_frame(
        &mut s,
        before,
        &history_title(field),
        &["[Acknowledge local]", "Currently observed Description:"],
    );
    let before = s.completed_cursorless_end();
    control(
        &mut s,
        mouse,
        &history_title(field),
        "[Acknowledge local]",
        HISTORY_FOOTER,
    );
    terminal(&mut s, before, "Local ACK");
    assert_original(&f, field, "acknowledged", true);
    assert_eq!(draft(&f), encoded);
    let old: Value = serde_json::from_str(&original).unwrap();
    let now: Value = serde_json::from_str(&marker(&f)).unwrap();
    for i in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 11, 12] {
        assert_eq!(now[i], old[i]);
    }
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}

fn cleanup_failed(mouse: bool) {
    let field = Field::Title;
    let mut steps = initial_steps(true);
    let initial = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let opened = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let prepared = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let paid = post(field); // Same final numeric UID precedes cleanup failure.
    steps.extend(paid);
    let published = steps.len();
    // Startup is a separately observed snapshot; the fresh history phase
    // below, not cached title/default body, provides the current field.
    steps.extend(initial_steps(true));
    let restarted = steps.len();
    steps.extend(read_phase(field, true, json!(true)));
    let f = fixture(&steps);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, initial, false, &steps[..initial]);
    let stable = independent(&f);
    open(&mut s, &f, mouse, field, opened);
    fill(&mut s, mouse, field, requested(field));
    preview(&mut s, &f, mouse, field, prepared);
    writable(&f).execute_batch("CREATE TRIGGER metadata_native_cleanup_refusal BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'metadata-text-v25/%' BEGIN SELECT RAISE(ABORT,'actual one-field cleanup refuses'); END;").unwrap();
    let before = s.completed_cursorless_end();
    control(
        &mut s,
        mouse,
        &form_title(field),
        "[Confirm action]",
        FOOTER,
    );
    s.wait_for_command_count(&f, published);
    terminal(&mut s, before, "Confirmed; visibility unavailable");
    assert_original(&f, field, "confirmed", true);
    let encoded = draft(&f);
    let original = marker(&f);
    exact(&s, &f, &steps[..published]);
    // A completed durable status/main footer is not proof that no modal
    // owns input: the genuine fallback Return is still above that footer.
    const RETURN: &str = "Confirmed Conversation • read-only refresh";
    const RETURN_CONTROLS: &[&str] = &["[r Refresh PR]", "[Esc Back to Inbox]"];
    form_frame(
        &mut s,
        before,
        RETURN,
        &["DURABLY CONFIRMED", RETURN_CONTROLS[0], RETURN_CONTROLS[1]],
    );
    let before_sql = complete_sql_v46(&f);
    let frame = s.completed_cursorless_end();
    control(&mut s, mouse, RETURN, RETURN_CONTROLS[1], RETURN_CONTROLS);
    s.wait_for_new_cursorless_condition(
        frame,
        |screen| {
            let text = screen.contents();
            crate::support::main_footer_visible(screen)
                && text.contains("Cached Inbox only")
                && !text.contains(RETURN)
                && !text.contains(RETURN_CONTROLS[0])
                && !crate::support::controls::control_painted(screen, "[b Back]")
                && !text.contains(DETAIL_BODY)
                && !text.contains(NEW_TITLE)
        },
        "Actual Return Back removes old PR/tickets before the single main-frame Quit",
        Duration::from_secs(8),
    );
    assert_eq!(
        complete_sql_v46(&f),
        before_sql,
        "Pure Return Back has zero SQL/reservation effects"
    );
    assert_eq!(draft(&f), encoded);
    assert_eq!(marker(&f), original);
    assert_original(&f, field, "confirmed", true);
    exact(&s, &f, &steps[..published]);
    s.quit_and_assert_restored_terminal(mouse);
    let mut s = launch(&f, mouse);
    enter(
        &mut s,
        &f,
        true,
        mouse,
        restarted,
        false,
        &steps[..restarted],
    );
    history_open(&mut s, &f, mouse, field, steps.len());
    let before = s.completed_cells_end();
    control(
        &mut s,
        mouse,
        &history_title(field),
        "[Inspect current]",
        HISTORY_FOOTER,
    );
    form_frame(
        &mut s,
        before,
        &history_title(field),
        &["Confirmed", "No supported unresolved metadata attempt"],
    );
    assert_eq!(draft(&f), encoded);
    assert_eq!(marker(&f), original);
    assert_original(&f, field, "confirmed", true);
    control(
        &mut s,
        mouse,
        &history_title(field),
        "[Esc Close]",
        HISTORY_FOOTER,
    );
    s.wait_for_overlay_close(&history_title(field));
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}

fn pure_close(mouse: bool) {
    for fail in [false, true] {
        let field = Field::Description;
        let mut steps = initial_steps(true);
        let initial = steps.len();
        steps.extend(read_phase(field, false, json!(true)));
        let opened = steps.len();
        if !fail {
            steps.extend(read_phase(field, false, json!(true)));
        }
        let f = fixture(&steps);
        let git = git_state(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut s = launch(&f, mouse);
        enter(&mut s, &f, true, mouse, initial, false, &steps[..initial]);
        let stable = independent(&f);
        open(&mut s, &f, mouse, field, opened);
        let db = writable(&f);
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(body) = db
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'metadata-text-v25/%'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .optional()
                .unwrap()
            {
                let body: Value = serde_json::from_str(&body).unwrap();
                assert!(body["publication"].is_null());
                break;
            }
            assert!(
                Instant::now() < until,
                "Genuine initial autosave completion"
            );
            s.poll_output(Duration::from_millis(2));
        }
        if fail {
            db.execute_batch("CREATE TRIGGER metadata_native_save_failure BEFORE INSERT ON drafts WHEN NEW.resource LIKE 'metadata-text-v25/%' BEGIN SELECT RAISE(ABORT,'actual metadata save refuses'); END;").unwrap();
        } else {
            db.execute_batch("BEGIN IMMEDIATE; UPDATE drafts SET body=body WHERE resource='metadata-independent';").unwrap();
        }
        fill(&mut s, mouse, field, NEW_BODY);
        resize_form(&mut s, 80, 24, field);
        resize_form(&mut s, 120, 40, field);
        close(&mut s, mouse, field);
        exact(&s, &f, &steps[..opened]);
        let before = s.completed_cells_end();
        crate::pr_actions::menu(&mut s, mouse, menu_label(field));
        if fail {
            form_frame(
                &mut s,
                before,
                &form_title(field),
                &[
                    NEW_BODY.lines().next().unwrap(),
                    "Local draft save failed.",
                    "[Keep draft / Close]",
                ],
            );
            assert!(!s.screen().contains("READ-ONLY PREVIEW"));
        } else {
            s.wait_for_new_cursorless_condition(
                before,
                |frame| {
                    frame
                        .contents()
                        .contains("Queued behind accepted local draft saves")
                        && !frame.contents().contains("[Confirm action]")
                },
                "Existing accepted-save cutoff before any original Core child",
                Duration::from_secs(8),
            );
            exact(&s, &f, &steps[..opened]);
            db.execute_batch("COMMIT;").unwrap();
            s.wait_for_command_count(&f, steps.len());
            form_frame(
                &mut s,
                before,
                &form_title(field),
                &[
                    NEW_BODY.lines().next().unwrap(),
                    "[Preview]",
                    "[Keep draft / Close]",
                ],
            );
            let encoded: Value = serde_json::from_str(&draft(&f)).unwrap();
            assert_eq!(encoded["values"]["body"], NEW_BODY);
            assert!(encoded["publication"].is_null());
        }
        assert_eq!(
            raw(&f)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
        close(&mut s, mouse, field);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}

#[test]
fn pr_metadata_original_v25_catalogue_title_keyboard_exact_patch_fresh_page_back() {
    happy(false, false, Field::Title)
}
#[test]
fn pr_metadata_original_v25_catalogue_title_mouse_exact_patch_fresh_page_back() {
    happy(true, false, Field::Title)
}
#[test]
fn pr_metadata_original_v25_personal_conversation_description_keyboard_exact_bytes() {
    happy(false, true, Field::Description)
}
#[test]
fn pr_metadata_original_v25_personal_conversation_description_mouse_exact_bytes() {
    happy(true, true, Field::Description)
}
#[test]
fn pr_metadata_original_v25_keyboard_nochange_denied_unknown_never_write() {
    refusal(false)
}
#[test]
fn pr_metadata_original_v25_mouse_nochange_denied_unknown_never_write() {
    refusal(true)
}
#[test]
fn pr_metadata_original_v25_keyboard_changed_head_field_repository_account_refuse() {
    drift(false)
}
#[test]
fn pr_metadata_original_v25_mouse_changed_head_field_repository_account_refuse() {
    drift(true)
}
#[test]
fn pr_metadata_original_v25_keyboard_held_readback_cancel_new_view_exact_cleanup() {
    canceled(false)
}
#[test]
fn pr_metadata_original_v25_mouse_held_readback_cancel_new_view_exact_cleanup() {
    canceled(true)
}
#[test]
fn pr_metadata_original_v25_keyboard_unknown_restart_fresh_inspect_explicit_local_ack() {
    restart_recovery(false)
}
#[test]
fn pr_metadata_original_v25_mouse_unknown_restart_fresh_inspect_explicit_local_ack() {
    restart_recovery(true)
}
#[test]
fn pr_metadata_original_v25_keyboard_confirmed_cleanup_failure_restart_not_ack() {
    cleanup_failed(false)
}
#[test]
fn pr_metadata_original_v25_mouse_confirmed_cleanup_failure_restart_not_ack() {
    cleanup_failed(true)
}
#[test]
fn pr_metadata_original_v25_keyboard_close_accepted_failed_save_reopen_no_replay() {
    pure_close(false)
}
#[test]
fn pr_metadata_original_v25_mouse_close_accepted_failed_save_reopen_no_replay() {
    pure_close(true)
}
