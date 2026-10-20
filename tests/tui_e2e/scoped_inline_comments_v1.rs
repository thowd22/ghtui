//! Actual initial top-level inline publication through the original scoped
//! range and shared composer. No live service, synthetic source or handler calls.
use super::*;
#[path = "form_preview_visible_inline_v1.rs"]
mod form_preview_visible_v1;
#[path = "pr_line_comment_v19.rs"]
mod line_comment_v19;
#[path = "scoped_suggestions_v1.rs"]
mod suggestions_v1;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession};
pub(super) const BODY: &str = "Native exact inline body 界\nsecond line";
pub(super) fn source_phase(h: &history::GitImagesV1, two_parents: bool) -> Vec<GhStep> {
    let mut parent = parent_resource();
    h.bind_parent(&mut parent);
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent.clone()));
    if two_parents {
        steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent));
    }
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    steps
}
fn receipt(h: &history::GitImagesV1, side: &str, body: &str) -> Value {
    json!({"id":901,"node_id":"PRRC_native_901","pull_request_url":format!("https://{HOST}/repos/{FIRST}/pulls/7"),"user":{"id":42,"login":"octocat","type":"User"},"path":history::PATH,"side":side,"line":2,"start_line":1,"start_side":side,"commit_id":h.h,"body":body})
}
pub(super) fn confirm_steps(h: &history::GitImagesV1, side: &str) -> Vec<GhStep> {
    confirm_steps_with(h, side, BODY)
}
pub(super) fn confirm_steps_with(h: &history::GitImagesV1, side: &str, body: &str) -> Vec<GhStep> {
    let mut parent = parent_resource();
    h.bind_parent(&mut parent);
    let mut steps = resource_operation(false);
    bind_steps(&mut steps, h);
    steps.truncate(7); // Same actual original Files parser/transport.
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent.clone()));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    let payload = json!({"path":history::PATH,"side":side,"line":2,"start_line":1,"start_side":side,"commit_id":h.h,"body":body});
    steps[7].delay_ms = 6_000; // Exact final measured parent AFTER durable claim.
    let mut post = GhStep::read(
        &[
            "api",
            &format!("repos/{FIRST}/pulls/7/comments"),
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &receipt(h, side, body).to_string(),
    );
    post.stdin = serde_json::to_vec(&payload).unwrap();
    steps.push(post);
    steps.push(get(
        &format!("repos/{FIRST}/pulls/comments/901"),
        receipt(h, side, body),
    ));
    // Exactly four prepaid visibility children; no second session/version.
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent));
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(steps.len(), 17);
    steps
}
pub(super) fn stable_drafts(fixture: &Fixture) -> Vec<String> {
    let db = raw(fixture);
    let rows=db.prepare("SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'inline/%' ORDER BY host,login,resource").unwrap().query_map([],|row|row.get(0)).unwrap().map(Result::unwrap).collect();
    rows
}
pub(super) fn exact_commands(session: &Session, fixture: &Fixture, steps: &[GhStep], posts: usize) {
    let rows = commands(session, fixture);
    assert_eq!(rows.len(), steps.len());
    for (ordinal, (actual, step)) in rows.iter().zip(steps).enumerate() {
        assert_eq!(actual["step"], ordinal);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(step.args));
        let stdin = step
            .stdin
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(actual["stdin_hex"], stdin);
    }
    assert_eq!(
        rows.iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST"))
            .count(),
        posts
    );
    assert!(!rows.iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| matches!(arg.as_str(), Some("PUT" | "PATCH" | "DELETE")))));
}
pub(super) fn editing_ready(session: &mut Session, body: &str) {
    session.try_wait_for_condition(|screen|{
        let lines=screen.lines().collect::<Vec<_>>();
        lines.iter().position(|line|line.contains("PR INTENT • Preview first")).is_some_and(|phase|{
            lines.get(phase+1).is_some_and(|line|line.contains("[F2 Full anchor details]"))
                && lines.get(phase+2).is_some_and(|line|line.contains("Body:"))
                && lines.get(phase+3).is_some_and(|line|line.contains(body.split('\n').next().unwrap_or("")))
        }) && screen.contains("@octocat")&&screen.contains("[?]")&&screen.contains("[Preview]")&&screen.contains("[External editor]")&&screen.contains("[Confirm action]")&&screen.contains("[Keep draft / Close]")
    },"Owned inline phase/first body field/current value/full footer after actual editing input",Duration::from_secs(8)).unwrap();
}
fn journey(mouse: bool, side: &str, cancel_after_post: bool) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    let fresh_images = steps[steps.len() - 17..].to_vec();
    steps.extend(source_phase(&h, true));
    steps.extend(fresh_images);
    let mut confirm = confirm_steps(&h, side);
    if cancel_after_post {
        confirm[7].delay_ms = 0;
        confirm[12].delay_ms = 6_000; // Actual readback child AFTER one successful POST.
        confirm.truncate(13);
    }
    let confirm_start = steps.len();
    steps.extend(confirm);
    fixture.scripted_gh(&steps);
    let before_git = git_state(&fixture);
    let before_drafts = stable_drafts(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    choose(&mut session, mouse, "[O Old image]", b"O");
    session.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut session,
        "Immutable file images",
        &[
            "[R Range]",
            "[P Comment]",
            "old-no-newline",
            "new-no-newline",
        ],
    );
    let git_journal = git_commands(&fixture);
    if side == "RIGHT" {
        choose(&mut session, mouse, "[N New]", b"N");
        ready(&mut session, &["Selection RIGHT 1–1"]);
    } else {
        choose(&mut session, mouse, "[O Old]", b"O");
        ready(&mut session, &["Selection LEFT 1–1"]);
    }
    if mouse {
        let prefix = if side == "LEFT" { "L" } else { "R" };
        let first = session.locate(&format!("{prefix}      1")).unwrap();
        let last = session.locate(&format!("{prefix}      2")).unwrap();
        session.drag(first, last);
    } else {
        session.send_key(b"\x1b[1;2B");
    }
    choose(&mut session, mouse, "[R Range]", b"R");
    ready(&mut session, &[&format!("READ-ONLY {side} 1–2"), &h.h]);
    choose(&mut session, mouse, "[P Comment]", b"P");
    session.wait_for_command_count(&fixture, 66);
    editing_ready(&mut session, "");
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", "Body");
    }
    session.paste(BODY);
    editing_ready(&mut session, BODY);
    choose(&mut session, mouse, "[F2 Full anchor details]", b"\x1bOQ");
    overlay_ready(
        &mut session,
        "Original inline anchor",
        &[
            "Canonical API filename:",
            "Selected image path:",
            &format!("{side} lines 1–2"),
            &h.h,
            &h.m,
            "[Esc Back to draft]",
        ],
    );
    session.resize(80, 24); // Centered form/details retains the actual main footer.
    overlay_ready(&mut session, "Original inline anchor", &[&h.h, &h.m]);
    if mouse {
        session.click_visible_action("[↓ Down]");
        session.click_visible_action("[↑ Up]");
        session.click_visible_action("[Esc Back to draft]");
    } else {
        session.send_key(b"\x1b[6~");
        session.send_key(b"\x1b[5~");
        session.send_key(b"\x1b");
    }
    editing_ready(&mut session, BODY);
    session.resize(120, 40);
    editing_ready(&mut session, BODY);
    assert_eq!(
        commands(&session, &fixture).len(),
        66,
        "Loaded details/resize/Back performs no GH work"
    );
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_command_count(&fixture, 83);
    ready(
        &mut session,
        &[
            "READ-ONLY PREVIEW • Confirm action",
            &format!("{side} lines 1–2"),
            &h.h,
            "[Confirm action]",
        ],
    );
    assert!(commands(&session, &fixture).iter().all(|row| !row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "POST")));
    let before = session.completed_cursorless_end();
    crate::local::confirm(&mut session, mouse, 1);
    let held_ordinal = confirm_start + if cancel_after_post { 12 } else { 7 };
    session.wait_for_command_count(&fixture, held_ordinal + 1);
    let held_journal = commands(&session, &fixture);
    let child_pid = i32::try_from(held_journal[held_ordinal]["pid"].as_u64().unwrap()).unwrap();
    let owned = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    let held = process_identity(child_pid)
        .unwrap()
        .expect("Actual six-second pre-POST child must still be live");
    assert!(owned.owns(&held));
    assert!(!held.zombie);
    assert_eq!(
        held_journal
            .iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST"))
            .count(),
        usize::from(cancel_after_post)
    );
    let original = raw(&fixture);
    let encoded: String = original
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'inline/%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let draft: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(draft["values"]["body"], BODY);
    assert_eq!(draft["version"], 2);
    assert!(draft.get("approved").is_none());
    let pending: (i64, String, String, String, String) = original
        .query_row(
            "SELECT attempt_id,pr_node,observed_head,payload_sha256,state FROM pr_write_attempts",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(
        pending.0,
        draft["publication"]["attempt_id"].as_i64().unwrap()
    );
    assert_eq!(pending.1, "GENUINE_CANONICAL_PR_NODE_1007");
    assert_eq!(pending.2, h.h);
    assert_eq!(pending.3, draft["publication"]["payload_sha256"]);
    assert_eq!(pending.4, "pending");
    let still_live = process_identity(child_pid)
        .unwrap()
        .expect("Original cache witness must precede held child's actual finish");
    assert_eq!(
        (still_live.uid, still_live.session, still_live.birth),
        (held.uid, held.session, held.birth)
    );
    assert!(!still_live.zombie);
    drop(original);
    if cancel_after_post {
        // An enabled details overlay would consume the following ONE Cancel
        // instead of canceling the actual invoked operation: its proof fails.
        choose(&mut session, mouse, "[F2 Full anchor details]", b"\x1bOQ");
        ready(
            &mut session,
            &[
                "READ-ONLY PREVIEW • Confirm action",
                "[Esc Cancel]",
                "[External editor]",
                "[Confirm action]",
            ],
        );
        choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
        session.wait_for_new_cursorless_condition(before,|frame| {
            let text=frame.contents();
            text.contains("canceled")&&text.contains("nothing resent")
                &&!text.contains("Inline comment •")&&!text.contains("old-no-newline")
                &&!text.contains("new-no-newline")&&!text.contains("Native exact inline body")&&!text.contains(DETAIL_BODY)
        },"Cancel after actual POST hides ORIGINAL private body and reports no resend, never rolled back or confirmed",Duration::from_secs(8));
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match process_identity(child_pid).unwrap() {
                None => break,
                Some(now) => {
                    assert_eq!(
                        (now.uid, now.session, now.birth),
                        (held.uid, held.session, held.birth)
                    );
                    assert!(
                        Instant::now() < deadline,
                        "Original readback child must be reaped after Cancel"
                    );
                    session.poll_output(Duration::from_millis(10));
                }
            }
        }
        session
            .try_wait_for_condition(
                |_| {
                    raw(&fixture)
                        .query_row("SELECT state FROM pr_write_attempts", [], |row| {
                            row.get::<_, String>(0)
                        })
                        .is_ok_and(|state| state == "uncertain")
                },
                "Original durable journal becomes uncertain after invoked readback cancellation",
                Duration::from_secs(5),
            )
            .unwrap();
        let retained: Value = serde_json::from_str(
            &raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'inline/%'",
                    [],
                    |row| row.get::<_, String>(0),
                )
                .unwrap(),
        )
        .unwrap();
        assert_eq!(retained["values"]["body"], BODY);
        assert_eq!(retained["publication"], draft["publication"]);
        session.send_key(b"\r");
        assert_eq!(
            commands(&session, &fixture).len(),
            steps.len(),
            "No late close result, automatic retry, reconciliation or resend"
        );
        assert_eq!(stable_drafts(&fixture), before_drafts);
        assert_eq!(git_state(&fixture), before_git);
        assert_eq!(git_commands(&fixture), git_journal);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        exact_commands(&session, &fixture, &steps, 1);
        session.quit_and_assert_restored_terminal(mouse);
        return;
    }
    session.wait_for_command_count(&fixture, steps.len());
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Comment posted")
                && text.contains("refresh explicitly")
                && !text.contains("Inline comment •")
                && !text.contains("old-no-newline")
                && !text.contains("new-no-newline")
                && !text.contains("Native exact inline body")
                && !text.contains(DETAIL_BODY)
        },
        "Confirmed real comment ID only; original snapshot/private images/form retired",
        Duration::from_secs(8),
    );
    let db = raw(&fixture);
    assert!(
        process_identity(child_pid).unwrap().is_none(),
        "Actual runner reaped the same original prePOST child before readback completed"
    );
    let row:(String,String,String,String,String)=db.query_row("SELECT command,selected_resource,observed_head,state,confirmed_json FROM pr_write_attempts",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).unwrap();
    assert_eq!(row.0, "inline-comment");
    assert!(row.1.starts_with("inline/") && row.1.len() == 71);
    assert_eq!(row.2, h.h);
    assert_eq!(row.3, "confirmed");
    let receipt: Value = serde_json::from_str(&row.4).unwrap();
    assert_eq!(receipt["resource_id"], "901");
    let drafts: u64 = db
        .query_row(
            "SELECT count(*) FROM drafts WHERE resource LIKE 'inline/%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(drafts, 0);
    let foreign:u64=db.query_row("SELECT (SELECT count(*) FROM records WHERE login='user-id:43')+(SELECT count(*) FROM drafts WHERE login='user-id:43')+(SELECT count(*) FROM pr_write_attempts WHERE principal='user-id:43')+(SELECT count(*) FROM actions_write_attempts)",[],|row|row.get(0)).unwrap();
    assert_eq!(foreign, 0);
    assert_eq!(stable_drafts(&fixture), before_drafts);
    assert_eq!(git_state(&fixture), before_git);
    assert_eq!(git_commands(&fixture), git_journal);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact_commands(&session, &fixture, &steps, 1);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn inline_comments_keyboard_original_left_and_right_range_preview_explicit_confirm_retire_snapshot()
{
    journey(false, "LEFT", false);
    journey(false, "RIGHT", false)
}
#[test]
fn inline_comments_mouse_original_drag_left_and_right_range_preview_explicit_confirm_retire_snapshot(
) {
    journey(true, "LEFT", false);
    journey(true, "RIGHT", false)
}
#[test]
fn inline_comments_keyboard_cancel_after_actual_post_preserves_pending_body_and_hides_original_source(
) {
    journey(false, "LEFT", true)
}
#[test]
fn inline_comments_mouse_cancel_after_actual_post_preserves_pending_body_and_hides_original_source()
{
    journey(true, "RIGHT", true)
}
fn refusal_journey(mouse: bool) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    steps.extend(principal(43));
    fixture.scripted_gh(&steps);
    let git_before = git_state(&fixture);
    let drafts = stable_drafts(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    choose(&mut session, mouse, "[O Old image]", b"O");
    session.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut session,
        "Immutable file images",
        &["[P Comment]", "old-no-newline", "new-no-newline"],
    );
    let journal = git_commands(&fixture);
    let sql_before = sql_state(&fixture);
    // P verifies the current LEFT 1–1 selection itself, then refuses it.
    choose(&mut session, mouse, "[P Comment]", b"P");
    ready(
        &mut session,
        &["LEFT endpoint must be deleted", "old-no-newline"],
    );
    assert_eq!(commands(&session, &fixture).len(), 58);
    session.resize_owned_modal(
        80,
        24,
        "Immutable file images",
        &["[> Right]", "[Esc Close]", "[b Back]", "[q Quit]"],
    );
    choose(&mut session, mouse, "[O Old]", b"O");
    choose(&mut session, mouse, "[R Range]", b"R");
    ready(&mut session, &["READ-ONLY LEFT 1–1", &h.h]);
    choose(&mut session, mouse, "[P Comment]", b"P");
    ready(
        &mut session,
        &[
            "LEFT endpoint must be deleted",
            "No side converted",
            "old-no-newline",
        ],
    );
    assert_eq!(
        commands(&session, &fixture).len(),
        58,
        "Invalid LEFT context endpoint cannot read SQL/private draft or start GH work"
    );
    if mouse {
        let first = session.locate("L      1").unwrap();
        let last = session.locate("L      2").unwrap();
        session.drag(first, last);
    } else {
        session.send_key(b"\x1b[1;2B");
    }
    choose(&mut session, mouse, "[R Range]", b"R");
    ready(&mut session, &["READ-ONLY LEFT 1–2", &h.h]);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[P Comment]", b"P");
    session.wait_for_command_count(&fixture, steps.len());
    session.wait_for_new_cursorless_condition(before,|frame|{let text=frame.contents();text.contains("source unavailable")&&text.contains("private views hidden")&&!text.contains("old-no-newline")&&!text.contains("new-no-newline")&&!text.contains(DETAIL_BODY)&&!text.contains("Inline comment •")},"Actual current numeric UID change refuses original private draft before SQL/body and hides previously visible original images",Duration::from_secs(8));
    assert_eq!(sql_state(&fixture), sql_before);
    assert_eq!(stable_drafts(&fixture), drafts);
    assert_eq!(git_state(&fixture), git_before);
    assert_eq!(git_commands(&fixture), journal);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact_commands(&session, &fixture, &steps, 0);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn inline_comments_keyboard_explicit_range_left_context_and_changed_current_uid_fail_closed() {
    refusal_journey(false)
}
#[test]
fn inline_comments_mouse_explicit_range_left_context_and_changed_current_uid_fail_closed() {
    refusal_journey(true)
}
/// TASK-19 AC3: an older-head draft is carried only into an explicit NEW
/// selection, explained there, and a Preview on a moved head keeps the text
/// again for re-anchoring with no write.
fn reanchor_journey(mouse: bool) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    // Preview's first four reads; the PR parent now reports a moved head.
    let mut stale = steps[steps.len() - 17..steps.len() - 13].to_vec();
    assert!(json!(stale[3].args).to_string().contains("pulls/7"));
    let mut moved: Value = serde_json::from_slice(&stale[3].stdout).unwrap();
    moved["head"]["sha"] = json!(h.m);
    stale[3].stdout = moved.to_string().into_bytes();
    steps.extend(source_phase(&h, true));
    steps.extend(stale);
    fixture.scripted_gh(&steps);
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let slot = format!(
        "inline/reanchor/{}/{}/{:x}",
        repository(0)["id"],
        parent_resource()["id"],
        <sha2::Sha256 as sha2::Digest>::digest(history::PATH.as_bytes())
    );
    let scope = ghtui::storage::Scope::github(HOST, Some(42)).unwrap();
    // Text an earlier Preview kept when the head moved under LEFT 3–4.
    ghtui::storage::Cache::open(&database)
        .unwrap()
        .put_draft(
            &scope,
            &slot,
            &json!({"version":1,"body":BODY,"from":"LEFT 3–4 at 0123456789ab"}).to_string(),
        )
        .unwrap();
    let before_git = git_state(&fixture);
    let before_drafts = stable_drafts(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    choose(&mut session, mouse, "[O Old image]", b"O");
    session.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut session,
        "Immutable file images",
        &[
            "[R Range]",
            "[P Comment]",
            "old-no-newline",
            "new-no-newline",
        ],
    );
    let git_journal = git_commands(&fixture);
    choose(&mut session, mouse, "[O Old]", b"O");
    ready(&mut session, &["Selection LEFT 1–1"]);
    if mouse {
        let first = session.locate("L      1").unwrap();
        let last = session.locate("L      2").unwrap();
        session.drag(first, last);
    } else {
        session.send_key(b"\x1b[1;2B");
    }
    choose(&mut session, mouse, "[R Range]", b"R");
    ready(&mut session, &["READ-ONLY LEFT 1–2", &h.h]);
    choose(&mut session, mouse, "[P Comment]", b"P");
    session.wait_for_command_count(&fixture, 66);
    editing_ready(&mut session, BODY);
    ready(&mut session, &["re-anchored"]);
    choose(&mut session, mouse, "[F2 Full anchor details]", b"\x1bOQ");
    overlay_ready(
        &mut session,
        "Original inline anchor",
        &[
            "RE-ANCHORED • text carried from LEFT 3–4 at 0123456789ab",
            "LEFT lines 1–2",
            &h.h,
            "[Esc Back to draft]",
        ],
    );
    if mouse {
        session.click_visible_action("[Esc Back to draft]");
    } else {
        session.send_key(b"\x1b");
    }
    editing_ready(&mut session, BODY);
    let db = raw(&fixture);
    let slots = |db: &rusqlite::Connection| -> u64 {
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource LIKE 'inline/reanchor/%'",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };
    assert_eq!(slots(&db), 0, "carried text leaves the re-anchor slot");
    let owned: Value = serde_json::from_str(
        &db.query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'inline/%' AND resource NOT LIKE 'inline/reanchor/%'",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(owned["values"]["body"], BODY);
    assert_eq!(owned["publication"], Value::Null);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_command_count(&fixture, steps.len());
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("PR changed since this selection")
                && text.contains("draft text kept")
                && !text.contains("old-no-newline")
                && !text.contains("new-no-newline")
                && !text.contains("READ-ONLY PREVIEW")
        },
        "Moved head refuses Preview, explains the re-anchor path and hides the stale images",
        Duration::from_secs(8),
    );
    let kept: Value = serde_json::from_str(
        &db.query_row(
            "SELECT body FROM drafts WHERE resource=?1",
            rusqlite::params![slot],
            |row| row.get::<_, String>(0),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(
        kept,
        json!({"version":1,"body":BODY,"from":format!("LEFT 1–2 at {}", &h.h[..12])})
    );
    let attempts: u64 = db
        .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(attempts, 0, "no write began on the moved head");
    assert_eq!(stable_drafts(&fixture), before_drafts);
    assert_eq!(git_state(&fixture), before_git);
    assert_eq!(git_commands(&fixture), git_journal);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact_commands(&session, &fixture, &steps, 0);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn inline_comments_keyboard_changed_head_keeps_text_and_new_selection_carries_it() {
    reanchor_journey(false)
}
#[test]
fn inline_comments_mouse_changed_head_keeps_text_and_new_selection_carries_it() {
    reanchor_journey(true)
}
fn held_preview_paste(mouse: bool) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    let mut preview = steps[steps.len() - 17..].to_vec();
    preview[0].delay_ms = 6_000;
    steps.extend(source_phase(&h, true));
    steps.extend(preview);
    fixture.scripted_gh(&steps);
    let git_before = git_state(&fixture);
    let drafts = stable_drafts(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    choose(&mut session, mouse, "[O Old image]", b"O");
    session.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut session,
        "Immutable file images",
        &[
            "[R Range]",
            "[P Comment]",
            "old-no-newline",
            "new-no-newline",
        ],
    );
    let journal = git_commands(&fixture);
    choose(&mut session, mouse, "[O Old]", b"O");
    if mouse {
        let first = session.locate("L      1").unwrap();
        let last = session.locate("L      2").unwrap();
        session.drag(first, last);
    } else {
        session.send_key(b"\x1b[1;2B");
    }
    choose(&mut session, mouse, "[R Range]", b"R");
    ready(&mut session, &["READ-ONLY LEFT 1–2", &h.h]);
    choose(&mut session, mouse, "[P Comment]", b"P");
    session.wait_for_command_count(&fixture, 66);
    editing_ready(&mut session, "");
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", "Body");
    }
    session.paste(BODY);
    editing_ready(&mut session, BODY);
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_command_count(&fixture, 67);
    let child_pid =
        i32::try_from(commands(&session, &fixture)[66]["pid"].as_u64().unwrap()).unwrap();
    let owned = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    let held = process_identity(child_pid)
        .unwrap()
        .expect("Actual held original PREVIEW child is live");
    assert!(owned.owns(&held) && !held.zombie);
    ready(&mut session, &["PR INTENT • Preview first", "[Esc Cancel]"]);
    session.paste("PASTE_MUST_NOT_EDIT_AN_INFLIGHT_PREVIEW");
    session.wait_for_command_count(&fixture, 83);
    ready(
        &mut session,
        &[
            "READ-ONLY PREVIEW • Confirm action",
            "[Keep draft / Close]",
            "@octocat",
            "[?]",
        ],
    );
    let title = format!("Inline comment • {FIRST}#7 • LEFT 1–2");
    let original_body = BODY.split('\n').next().unwrap();
    if !session
        .completed_cursorless_screen()
        .unwrap()
        .contents()
        .contains(original_body)
    {
        // At 80x24 the bounded prepared source/anchor explanation fills the
        // initial viewport. Verify the preserved body through actual input,
        // not by accepting hidden text or increasing terminal dimensions.
        let before = session.completed_cursorless_end();
        if mouse {
            session.wheel(30, 12, true);
        } else {
            session.send_key(b"\x1b[6~");
        }
        session.wait_for_new_cursorless_condition(
            before,
            |frame| {
                crate::support::overlay_body(frame, &title).is_some_and(|body| {
                    body.contains("READ-ONLY PREVIEW • Confirm action")
                        && body.contains(original_body)
                        && body.contains("[Keep draft / Close]")
                        && body.contains("@octocat")
                        && body.contains("[?]")
                })
            },
            "Actual owned scrolled Preview visibly retains original body after rejected pending paste",
            Duration::from_secs(8),
        );
    }
    ready(&mut session, &[original_body]);
    assert!(!session
        .screen()
        .contains("PASTE_MUST_NOT_EDIT_AN_INFLIGHT_PREVIEW"));
    assert!(
        process_identity(child_pid).unwrap().is_none(),
        "Same original Preview child completed and was reaped, not a replacement operation"
    );
    let db = raw(&fixture);
    let encoded: String = db
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'inline/%'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    let body: Value = serde_json::from_str(&encoded).unwrap();
    assert_eq!(body["values"]["body"], BODY);
    assert!(body["publication"].is_null());
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    drop(db);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_new_cursorless_condition(before,|frame|{let text=frame.contents();!text.contains("Inline comment •")&&!text.contains("Native exact inline body")&&!text.contains("PASTE_MUST_NOT_EDIT_AN_INFLIGHT_PREVIEW")},"Actual completed original composer close after preserved Preview; no hidden busy form or poison text",Duration::from_secs(8));
    assert_eq!(stable_drafts(&fixture), drafts);
    assert_eq!(git_state(&fixture), git_before);
    assert_eq!(git_commands(&fixture), journal);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact_commands(&session, &fixture, &steps, 0);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn inline_comments_keyboard_pending_preview_rejects_paste_without_canceling_owner_or_sticking_busy()
{
    held_preview_paste(false)
}
#[test]
fn inline_comments_mouse_pending_preview_rejects_paste_without_canceling_owner_or_sticking_busy() {
    held_preview_paste(true)
}
