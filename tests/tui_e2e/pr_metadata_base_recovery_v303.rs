//! Unique original Base recovery pair. History is read only; a separately
//! opened current action is blank/unapproved, never reconstructed from a receipt.
use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

const HISTORY: &str = "Original Base history • example/repo#7";
const HISTORY_CONTROLS: &[&str] = &["[Inspect current]", "[Acknowledge local]", "[Esc Close]"];
const RETURN: &str = "Confirmed Conversation • read-only refresh";
const RETURN_CONTROLS: &[&str] = &["[r Refresh PR]", "[Esc Back to Inbox]"];
const READ_ONLY: &str = "Base fields are read only; use F2 Choose branch, then Preview or Close";
const FRESH_BLANK: &str =
    "{\"version\":17,\"values\":{\"base\":\"\",\"tip\":\"\"},\"publication\":null}";

fn current_phase(raw: &Value, list: bool, selected: bool) -> Vec<GhStep> {
    // SAME nine roles, but the old helper's hardcoded historical H must not
    // stand in for the different genuinely observed restart H.
    let mut calls = phase(raw);
    assert_eq!(calls.len(), 9);
    assert_eq!(calls[4].args[1], "graphql");
    let mut capability: Value = serde_json::from_slice(&calls[4].stdout).unwrap();
    assert_eq!(capability["data"]["node"]["id"], NODE);
    capability["data"]["node"]["headRefOid"] = raw["head"]["sha"].clone();
    calls[4].stdout = serde_json::to_vec(&capability).unwrap();
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
    if selected {
        assert!(!list);
        for _ in 0..2 {
            members.push(step(
                "repos/example/repo/branches/release%2Foctober",
                "GET",
                json!({"name":BASE,"commit":{"sha":TIP},"protected":false}),
                false,
            ));
        }
    }
    calls.splice(5..5, members);
    assert_eq!(
        calls.len(),
        9 + usize::from(list) + 2 * usize::from(selected)
    );
    calls
}

fn actual_key(raw: &Value) -> String {
    // Independent literal tuple, not the production serializer or the old
    // base_key()'s historical HEAD constant.
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
                raw["head"]["sha"].as_str().unwrap(),
                raw["base"]["sha"].as_str().unwrap(),
                raw["base"]["ref"].as_str().unwrap(),
            ))
            .unwrap()
        )
    )
}

fn own_action(s: &mut Session, mouse: bool, title: &str, label: &str, controls: &[&str]) {
    use crate::support::completed_navigation_controls_v1::{
        owned_action_position, FocusAttributeV1,
    };
    if !mouse {
        s.focus_completed_owned_action_v1(title, label, FocusAttributeV1::Inverse, controls, &[]);
    }
    let frame = s
        .completed_cells_screen()
        .expect("Actual completed owning Form");
    let (x, y) = owned_action_position(frame, title, label, &[]).expect("Whole current control");
    assert!(mouse || frame.cell(y, x).unwrap().inverse());
    if mouse {
        s.click(x, y)
    } else {
        s.send_key(b"\r")
    };
}

fn inbox_resize(s: &mut Session, columns: u16, rows: u16) {
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
            "NEW resized actual Inbox\n{}",
            s.screen()
        );
    }
}

fn restarted_metadata(s: &mut Session, mouse: bool, raw: &Value) {
    s.wait_for_main_frame();
    // A real80 frame precedes the temporary120 fullH/source witness. Return80
    // before actual History so narrow clipping/scrolling remains exercised.
    assert_eq!(s.completed_cells_screen().unwrap().size(), (24, 80));
    inbox_resize(s, 120, 40);
    let opening = s.completed_cells_end();
    open(s, mouse);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > opening
            && s.completed_cells_screen().is_some_and(|frame| {
                // The context moved into the content pane's top border; its
                // tail keeps the full head SHA.
                let expected = format!(
                    "example/repo#7 / {} ─╮",
                    raw["head"]["sha"].as_str().unwrap()
                );
                frame.contents().lines().any(|line| {
                    line.contains("Inbox • focused view") && line.trim_end().ends_with(&expected)
                }) && crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, "Inbox • focused view ─").is_some_and(
                        |body| {
                            body.contains(" u Metadata ")
                                && body.contains("Native collaboration")
                                && body.contains("@octocat PR description")
                                && body.contains("A fixture description")
                                && body.contains("@octocat comment #9 • 2026-10-13T00:00:00Z")
                                && body.contains("Existing starter comment")
                                && !body.contains("[Esc Cancel]")
                        },
                    )
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "NEW owning Conversation with EXACT current fullH breadcrumb\n{}",
            s.screen()
        );
    }
    let before = s.completed_cells_end();
    choose(s, mouse, "[u Metadata]", b"u");
    ready(
        s,
        before,
        "Inbox • focused view ─",
        &[
            "[u Metadata]",
            "[p PR actions]",
            "Your repository permissions: read true • write true",
            "Locked conversation: false",
            "Milestone: October release",
        ],
    );
    inbox_resize(s, 80, 24);
}

fn reservation() -> String {
    let key = format!(
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
        key,
        "personal-pr-source-v13/1a2b6de1378cbda0ac2ca29eab19e1a29af0240edb1704b89dc16933f104cb35"
    );
    key
}

fn next_read_sql(mut before: Vec<String>) -> Vec<String> {
    use rusqlite::types::Value as Sql;
    let key = reservation();
    let marker = format!("sync_reservations:[Text(\"github.invalid\"), Text(\"user-id:42\"), Text(\"{key}\"), Integer(");
    let matches = before
        .iter()
        .enumerate()
        .filter(|(_, row)| row.starts_with(&marker))
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert_eq!(
        matches.len(),
        1,
        "One independently named actual source reservation"
    );
    let index = matches[0];
    let old = before[index]
        .strip_prefix(&marker)
        .unwrap()
        .strip_suffix("), Integer(1)]")
        .unwrap()
        .parse::<i64>()
        .unwrap();
    assert!(old > 0);
    let row = |generation| {
        format!(
            "sync_reservations:{:?}",
            [
                Sql::Text("github.invalid".into()),
                Sql::Text("user-id:42".into()),
                Sql::Text(key.clone()),
                Sql::Integer(generation),
                Sql::Integer(1)
            ]
        )
    };
    assert_eq!(before[index], row(old));
    before[index] = row(old.checked_add(1).unwrap());
    before
}

fn history_open(s: &mut Session, f: &Fixture, mouse: bool, steps: &[GhStep], count: usize) {
    let expected = next_read_sql(raw_sql_v278(f));
    let before = s.completed_cells_end();
    menu(s, mouse, "Inspect Base history");
    s.wait_for_command_count(f, count);
    ready(s, before, HISTORY, HISTORY_CONTROLS);
    assert_eq!(mutation_count(&exact_current(s, f, steps, count)), 1);
    assert_eq!(
        raw_sql_v278(f),
        expected,
        "Exactly one named Personal read; every schema/table/sequence/protected row remains exact"
    );
}

fn history_fact(s: &mut Session, mouse: bool, literal: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    for _ in 0..32 {
        s.poll_output(Duration::from_millis(1));
        let frame = s
            .completed_cells_screen()
            .expect("Actual completed Base history");
        let body = crate::support::overlay_body(frame, HISTORY).expect("Same topmost Base history");
        assert!(HISTORY_CONTROLS.iter().all(|label| body.contains(label)));
        assert!(
            !body.contains("[Confirm action]")
                && !body.contains("[F2 Choose branch]")
                && !body.contains("[Esc Cancel]")
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
                "ONE scroll must produce NEW changed owning history, not hidden facts\n{}",
                s.screen()
            );
        }
    }
    panic!("Full literal is not actually visible in one owned history body: {literal}");
}

fn full_original_facts(s: &mut Session, mouse: bool, payload: &str) {
    history_fact(
        s,
        mouse,
        &format!("Currently observed Base {BASE} at {TIP} • head {CHANGED_TIP}"),
    );
    history_fact(
        s,
        mouse,
        &format!("Attempt #1 • Confirmed • historical H {HEAD} (not current authority)"),
    );
    history_fact(
        s,
        mouse,
        &format!("Historical Base main at {}", "b".repeat(40)),
    );
    history_fact(
        s,
        mouse,
        &format!("Requested Base {BASE} at {TIP} • exact fingerprint {payload}"),
    );
    history_fact(
        s,
        mouse,
        "Protected exact draft/reference retained; NOT editable or eligible",
    );
}

fn history_close(s: &mut Session, f: &Fixture, mouse: bool, steps: &[GhStep], count: usize) {
    let before_sql = raw_sql_v278(f);
    let before = s.completed_cells_end();
    own_action(s, mouse, HISTORY, "[Esc Close]", HISTORY_CONTROLS);
    ready(
        s,
        before,
        "Inbox • focused view ─",
        &[
            "[u Metadata]",
            "[p PR actions]",
            "Locked conversation: false",
        ],
    );
    assert_eq!(
        raw_sql_v278(f),
        before_sql,
        "Pure actual history Close never releases or changes SQL"
    );
    assert_eq!(mutation_count(&exact_current(s, f, steps, count)), 1);
}

fn protected_rows(f: &Fixture, old: &Value) -> Vec<String> {
    let db = Connection::open_with_flags(cache_path(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap();
    let (attempt,state,selected,head,payload,receipt):(i64,String,String,String,String,String)=db.query_row("SELECT attempt_id,state,selected_resource,observed_head,payload_sha256,confirmed_json FROM pr_write_attempts WHERE command='metadata-base-v32'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).unwrap();
    assert_eq!(
        (attempt, state.as_str(), selected.as_str(), head.as_str()),
        (1, "confirmed", actual_key(old).as_str(), HEAD)
    );
    assert_eq!(
        db.query_row("SELECT COUNT(*) FROM pr_write_attempts", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        1
    );
    let encoded: String = db
        .query_row(
            "SELECT origin_json FROM pr_write_metadata_base_origins_v32 WHERE attempt_id=1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let record: Value = serde_json::from_str(&encoded).unwrap();
    let reference =
        json!({"attempt_id":1,"pr_node_id":NODE,"observed_head":HEAD,"payload_sha256":payload});
    assert_eq!(record["reference"], reference);
    let seed: BaseSeed = serde_json::from_value(record["seed"].clone()).unwrap();
    assert_eq!(seed.version, 32);
    exact_database(f, &seed.database);
    assert_eq!(seed.scope, scope());
    assert_eq!(
        (
            seed.repository_id,
            seed.repository_node.as_str(),
            seed.repository.as_str(),
            seed.pr_id,
            seed.pr_node.as_str(),
            seed.number
        ),
        (410, "R_metadata_410", REPOSITORY, 1007, NODE, 7)
    );
    assert_eq!(
        (
            seed.head.as_str(),
            seed.base.as_str(),
            seed.base_ref.as_str(),
            seed.requested_ref.as_str(),
            seed.requested_tip.as_str()
        ),
        (HEAD, "b".repeat(40).as_str(), "main", BASE, TIP)
    );
    assert_eq!(
        (seed.requester.as_str(), seed.author_uid),
        ("octocat", Some(42))
    );
    let values = BTreeMap::from([
        ("base".into(), BASE.into()),
        ("tip".into(), TIP.into()),
        (
            "origin-v32".into(),
            format!("{:x}", Sha256::digest(serde_json::to_vec(&seed).unwrap())),
        ),
    ]);
    assert_eq!(
        payload,
        PrWriteIntent {
            command: "metadata-base-v32".into(),
            selected_resource: selected.clone(),
            observed_head: HEAD.into(),
            values
        }
        .digest()
        .unwrap()
    );
    let mut effect = Sha256::new();
    effect.update(BASE.as_bytes());
    effect.update([0]);
    effect.update(TIP.as_bytes());
    assert_eq!(
        serde_json::from_str::<Value>(&receipt).unwrap(),
        json!({"kind":"metadata","resource_id":format!("metadata-base-v32/{:x}",effect.finalize()),"node_id":NODE})
    );
    let body =
        saved(f, &selected).expect("Real exact cleanup trigger retains actual typed reference");
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"version":17,"values":{"base":BASE,"tip":TIP},"publication":reference})
    );
    assert_eq!(db.query_row("SELECT generation FROM pr_write_scopes WHERE host='github.invalid' AND principal='user-id:42' AND pr_node=?1",[NODE],|r|r.get::<_,i64>(0)).unwrap(),3);
    // Pin full raw journal rows and the exact original body+updated_at. Each
    // read phase ALSO compares the whole database, not only this protection set.
    let all = raw_sql_v278(f);
    let own_draft =
        format!("drafts:[Text(\"github.invalid\"), Text(\"user-id:42\"), Text(\"{selected}\"),");
    let rows = all
        .into_iter()
        .filter(|row| row.starts_with("pr_write_") || row.starts_with(&own_draft))
        .collect::<Vec<_>>();
    assert_eq!(
        rows.iter()
            .filter(|row| row.starts_with(&own_draft))
            .count(),
        1
    );
    assert!(rows
        .iter()
        .any(|row| row.contains(&encoded.replace('"', "\\\""))));
    rows
}

fn ignored_history_input(
    s: &mut Session,
    f: &Fixture,
    mouse: bool,
    steps: &[GhStep],
    count: usize,
) {
    let sql = raw_sql_v278(f);
    let before = s.completed_cells_screen().unwrap();
    let body = crate::support::overlay_body(before, HISTORY).unwrap();
    let after = s.completed_cells_end();
    if mouse {
        let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
            before,
            HISTORY,
            "[Inspect current]",
            &[],
        )
        .unwrap();
        s.click(x, y.saturating_sub(2));
    }
    s.paste("edited draft is not the original receipt");
    s.send_key(b"x");
    ready(s, after, HISTORY, HISTORY_CONTROLS);
    assert_eq!(
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), HISTORY).unwrap(),
        body,
        "Actual zero-field read-only history ignores input, not an invented edit-error"
    );
    assert_eq!(raw_sql_v278(f), sql);
    assert_eq!(mutation_count(&exact_current(s, f, steps, count)), 1);
    let after = s.completed_cells_end();
    own_action(s, mouse, HISTORY, "[Acknowledge local]", HISTORY_CONTROLS);
    ready(s, after, HISTORY, HISTORY_CONTROLS);
    history_fact(
        s,
        mouse,
        "Fresh useful exact Base inspection required before LOCAL ACK • never resend",
    );
    assert_eq!(
        raw_sql_v278(f),
        sql,
        "Unready ACK is refusal only, never an edit/release or retry"
    );
    assert_eq!(mutation_count(&exact_current(s, f, steps, count)), 1);
}

fn blank_visible(s: &Session) -> bool {
    let Some(frame) = s.completed_cells_screen() else {
        return false;
    };
    let Some(body) = crate::support::overlay_body(frame, BASE_TITLE) else {
        return false;
    };
    if !crate::support::main_footer_visible(frame)
        || !BASE_FORM.iter().all(|label| body.contains(label))
        || body.contains("[Confirm action]")
    {
        return false;
    }
    let rows = body.lines().collect::<Vec<_>>();
    [
        "Selected existing base branch (F2 Choose):",
        "Exact observed selected tip (read only):",
    ]
    .iter()
    .all(|label| {
        let at = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.trim() == *label)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        at.len() == 1
            && rows
                .get(at[0] + 1)
                .is_some_and(|value| value.chars().all(char::is_whitespace))
    })
}

fn focus_actual_base(s: &mut Session, mouse: bool) {
    use crate::support::completed_navigation_controls_v1::owned_action_position;
    if mouse {
        let frame = s.completed_cells_screen().unwrap();
        let (x, y) = owned_action_position(
            frame,
            BASE_TITLE,
            "Selected existing base branch (F2 Choose)",
            &[],
        )
        .unwrap();
        let after = s.completed_cells_end();
        s.click(x, y + 1);
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after
                && s.completed_cells_screen()
                    .is_some_and(|f| focused_base_field(f) == Some(0))
            {
                return;
            }
            assert!(
                Instant::now() < until,
                "Actual clicked field focus\n{}",
                s.screen()
            );
        }
    }
    let witness = |frame: &vt100::Screen| {
        let field = focused_base_field(frame);
        let footer = BASE_FORM
            .iter()
            .filter_map(|label| {
                owned_action_position(frame, BASE_TITLE, label, &[])
                    .filter(|(x, y)| frame.cell(*y, *x).is_some_and(|c| c.inverse()))
                    .map(|_| *label)
            })
            .collect::<Vec<_>>();
        assert!(field.is_some() && footer.is_empty() || field.is_none() && footer.len() == 1);
        (field, footer.first().copied())
    };
    let until = Instant::now() + Duration::from_secs(8);
    for _ in 0..16 {
        let old = witness(s.completed_cells_screen().unwrap());
        if old.0 == Some(0) {
            return;
        }
        let after = s.completed_cells_end();
        s.send_key(b"\t");
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after
                && s.completed_cells_screen()
                    .is_some_and(|f| witness(f) != old)
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "ONE Tab needs actual NEW underlined field/inverse footer change\n{}",
                s.screen()
            );
        }
    }
    panic!("Actual Base field did not receive keyboard focus");
}

fn seconds() -> i64 {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
}

fn model_blank_save(mut before: Vec<String>, key: &str, stamp: i64) -> Vec<String> {
    use rusqlite::types::Value as Sql;
    assert!(!before.iter().any(|row| row.starts_with(&format!(
        "drafts:[Text(\"github.invalid\"), Text(\"user-id:42\"), Text(\"{key}\"),"
    ))));
    let start = before
        .iter()
        .position(|row| row.starts_with("schema:table:drafts:"))
        .unwrap()
        + 1;
    let end = before[start..]
        .iter()
        .position(|row| row.starts_with("schema:"))
        .map_or(before.len(), |n| start + n);
    let row = format!(
        "drafts:{:?}",
        [
            Sql::Text("github.invalid".into()),
            Sql::Text("user-id:42".into()),
            Sql::Text(key.into()),
            Sql::Text(FRESH_BLANK.into()),
            Sql::Integer(stamp)
        ]
    );
    let at = (start..end).find(|&i| before[i] > row).unwrap_or(end);
    before.insert(at, row);
    before
}

pub(in crate::pr_actions::pr_metadata::base) fn original(mouse: bool) {
    let f = fixture();
    let head = f.git(&["rev-parse", "HEAD"]);
    let index = fs::read(f.repo().join(".git/index")).unwrap();
    let status = f.git(&["status", "--porcelain=v2", "-z"]);
    let config = fs::read(f.repo().join(".git/config")).unwrap();
    let old = canonical(original_parent());
    let old_key = actual_key(&old);
    assert_eq!(old_key, base_key(&old));
    let mut updated = old.clone();
    updated["base"]["ref"] = json!(BASE);
    updated["base"]["sha"] = json!(TIP);
    let mut current = updated.clone();
    current["head"]["sha"] = json!(CHANGED_TIP);
    let fresh_key = actual_key(&current);
    assert_ne!(fresh_key, old_key);
    let mut steps = startup(&old);
    steps.extend(current_phase(&old, true, false));
    steps.extend(current_phase(&old, false, true));
    steps.extend(current_phase(&old, false, true));
    steps.extend(post(&old, &updated, "base", BASE));
    assert_eq!(steps.len(), 73);
    steps.extend(startup(&current));
    assert_eq!(steps.len(), 98);
    steps.extend(current_phase(&current, false, false));
    assert_eq!(steps.len(), 107);
    steps.extend(current_phase(&current, false, false));
    assert_eq!(steps.len(), 116);
    steps.extend(current_phase(&current, true, false));
    assert_eq!(steps.len(), 126);
    f.scripted_gh(&steps);
    let mut first = Session::launch(&f, 120, 40);
    enter_metadata(&mut first, mouse);
    open_picker(&mut first, &f, mouse, 35);
    select(&mut first, mouse);
    preview_base(&mut first, &f, mouse, 46);
    let inert = draft(&mut first, &f, &old);
    assert_eq!(
        serde_json::from_str::<Value>(&inert).unwrap(),
        json!({"version":17,"values":{"base":BASE,"tip":TIP},"publication":null})
    );
    Connection::open(cache_path(&f)).unwrap().execute_batch(&format!("CREATE TRIGGER keep_original_base_v303 BEFORE DELETE ON drafts WHEN OLD.host='github.invalid' AND OLD.login='user-id:42' AND OLD.resource='{old_key}' BEGIN SELECT RAISE(ABORT,'fixture retains actual original Base reference'); END;")).unwrap();
    let after = first.completed_cells_end();
    base_action(&mut first, mouse, "[Confirm action]", BASE_PREVIEW);
    first.wait_for_command_count(&f, 73);
    ready(
        &mut first,
        after,
        RETURN,
        &[
            "DURABLY CONFIRMED",
            "NEVER resend",
            "[r Refresh PR]",
            "[Esc Back to Inbox]",
        ],
    );
    assert_eq!(
        mutation_count(&exact_current(&mut first, &f, &steps, 73)),
        1
    );
    let protected = protected_rows(&f, &old);
    let complete = raw_sql_v278(&f);
    let after = first.completed_cells_end();
    own_action(
        &mut first,
        mouse,
        RETURN,
        "[Esc Back to Inbox]",
        RETURN_CONTROLS,
    );
    ready(
        &mut first,
        after,
        "Inbox • focused view ─",
        &["Native collaboration"],
    );
    assert_eq!(raw_sql_v278(&f), complete);
    preserve_git(&f, &head, &index, &status);
    first.quit_and_assert_restored_terminal(mouse);
    assert_eq!(raw_sql_v278(&f), complete);
    let mut second = Session::launch(&f, 80, 24);
    restarted_metadata(&mut second, mouse, &current);
    exact_current(&mut second, &f, &steps, 98);
    assert_eq!(protected_rows(&f, &old), protected);
    assert_eq!(saved(&f, &fresh_key), None);
    let payload = receipts(&f)[0].payload_sha256.clone();
    history_open(&mut second, &f, mouse, &steps, 107);
    let listed = raw_sql_v278(&f);
    full_original_facts(&mut second, mouse, &payload);
    assert_eq!(raw_sql_v278(&f), listed);
    ignored_history_input(&mut second, &f, mouse, &steps, 107);
    assert_eq!(protected_rows(&f, &old), protected);
    history_close(&mut second, &f, mouse, &steps, 107);
    history_open(&mut second, &f, mouse, &steps, 116);
    let twice = raw_sql_v278(&f);
    full_original_facts(&mut second, mouse, &payload);
    assert_eq!(raw_sql_v278(&f), twice);
    assert_eq!(protected_rows(&f, &old), protected);
    history_close(&mut second, &f, mouse, &steps, 116);
    let expected_read = next_read_sql(raw_sql_v278(&f));
    let start = seconds();
    assert_eq!(saved(&f, &fresh_key), None);
    open_picker(&mut second, &f, mouse, 126);
    let before = second.completed_cells_end();
    picker_action(&mut second, mouse, "[Esc Back]");
    ready(&mut second, before, BASE_TITLE, BASE_FORM);
    second
        .try_wait_for_condition(
            |_| saved(&f, &fresh_key).as_deref() == Some(FRESH_BLANK),
            "Actual fresh current-key blank v17 autosave settles; no selection/reference",
            Duration::from_secs(8),
        )
        .unwrap();
    assert!(blank_visible(&second));
    let end = seconds();
    assert!((0..=10).contains(&(end - start)));
    let fresh = raw_sql_v278(&f);
    assert!((start..=end).any(|stamp|model_blank_save(expected_read.clone(),&fresh_key,stamp)==fresh),"Only one exact new-key blank draft with independently phase-bounded timestamp plus named read reservation may change; all other SQL/schema/sequence/protected bytes stay");
    assert_eq!(protected_rows(&f, &old), protected);
    focus_actual_base(&mut second, mouse);
    let after = second.completed_cells_end();
    second.paste("edited draft is not the original receipt");
    // A completed mouse focus frame can follow the paste write. It is not
    // evidence that the paste's read-only refusal has actually been rendered.
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        second.poll_output(Duration::from_millis(1));
        if second.completed_cells_end() > after
            && second.completed_cells_screen().is_some_and(|frame| {
                focused_base_field(frame) == Some(0)
                    && crate::support::overlay_body(frame, BASE_TITLE).is_some_and(|body| {
                        crate::owned_form_prose_v120::adjacent_literal(&body, READ_ONLY)
                    })
                    && blank_visible(&second)
            })
        {
            break;
        }
        assert!(
            Instant::now() < until,
            "NEW same-owned focused blank Base Form with full read-only refusal after paste\n{}",
            second.screen()
        );
    }
    let body =
        crate::support::overlay_body(second.completed_cells_screen().unwrap(), BASE_TITLE).unwrap();
    assert!(crate::owned_form_prose_v120::adjacent_literal(
        &body, READ_ONLY
    ));
    assert!(blank_visible(&second));
    second.capture_completed_terminal_v1("base-fresh-open-readonly-choice-guidance");
    assert_eq!(raw_sql_v278(&f), fresh);
    exact_current(&mut second, &f, &steps, 126);
    let after = second.completed_cells_end();
    base_action(&mut second, mouse, "[F2 Choose branch]", BASE_FORM);
    ready(&mut second, after, BASE_PICKER, PICKER_CONTROLS);
    let after = second.completed_cells_end();
    picker_action(&mut second, mouse, "[Esc Back]");
    ready(&mut second, after, BASE_TITLE, BASE_FORM);
    assert!(blank_visible(&second));
    let after = second.completed_cells_end();
    base_action(&mut second, mouse, "[Preview]", BASE_FORM);
    ready(
        &mut second,
        after,
        BASE_TITLE,
        &["Choose an actual member with F2", "[Preview]"],
    );
    assert!(blank_visible(&second));
    assert_eq!(raw_sql_v278(&f), fresh);
    assert_eq!(protected_rows(&f, &old), protected);
    let commands = exact_current(&mut second, &f, &steps, 126);
    assert_eq!(mutation_count(&commands), 1);
    close(&mut second, mouse, BASE_FORM);
    assert_eq!(raw_sql_v278(&f), fresh);
    assert_eq!(protected_rows(&f, &old), protected);
    preserve_git(&f, &head, &index, &status);
    assert_eq!(fs::read(f.repo().join(".git/config")).unwrap(), config);
    second.quit_and_assert_restored_terminal(mouse);
    assert_eq!(raw_sql_v278(&f), fresh);
    assert_eq!(exact_current(&mut second, &f, &steps, 126), commands);
    assert_eq!(protected_rows(&f, &old), protected);
    preserve_git(&f, &head, &index, &status);
}
