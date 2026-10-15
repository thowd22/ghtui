//! DECLARED/UNRUN. Actual live owned child -> external SAME-DB SQL replacement
//! -> unchanged producer ACK -> final guard/cleanup refusal. No fixture hook
//! alters application revisions, source permission or the expected command cap.
use super::*;
const REPLACEMENT: &str = "PRIVATE_EXTERNAL_THREAD_DRAFT_REPLACEMENT_MUST_SURVIVE";
fn journey(mouse: bool, after_receipt: bool) {
    let operation = Operation::Reply;
    let mut write = confirm(operation);
    // Prewrite closing repo GET precedes original final SQL guard/principal.
    // Postwrite raw PR GET precedes original local cleanup/final numeric UID.
    let held = if after_receipt { 18 } else { 11 };
    write[held].delay_ms = 6000;
    write.truncate(held + 1);
    let mut steps = initial();
    steps.extend(resource_operation(false));
    steps.extend(browse(operation));
    steps.extend(observation(operation));
    steps.extend(observation(operation));
    steps.extend(write);
    let fixture = resource_configured(&steps);
    Cache::open(&database(&fixture))
        .unwrap()
        .put_draft(
            &Scope::github(HOST, Some(43)).unwrap(),
            "foreign-thread-unrelated-original",
            FOREIGN,
        )
        .unwrap();
    let git = git_state(&fixture);
    let sql = untouched_sql(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    prepared(&mut session, &fixture, mouse, operation);
    let git_journal = git_commands(&fixture);
    let before = session.completed_cursorless_end();
    form_control(&mut session, mouse, operation, "[Confirm action]");
    let child = held_child(&mut session, &fixture, 80 + held);
    let db = raw(&fixture);
    let (key,original):(String,String)=db.query_row("SELECT resource,body FROM drafts WHERE login='user-id:42' AND resource LIKE 'thread-draft/%'",[],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
    let original: Value = serde_json::from_str(&original).unwrap();
    assert_eq!(original["values"]["body"], REPLY);
    let mut replacement = original.clone();
    replacement["values"]["body"] = json!(REPLACEMENT);
    let encoded = replacement.to_string();
    let before_marker: (i64, String, String, String) = db
        .query_row(
            "SELECT attempt_id,state,payload_sha256,observed_head FROM pr_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        before_marker.1,
        if after_receipt {
            "confirmed"
        } else {
            "pending"
        }
    );
    Cache::open(&database(&fixture))
        .unwrap()
        .put_draft(&Scope::github(HOST, Some(42)).unwrap(), &key, &encoded)
        .unwrap();
    let live = process_identity(child.pid)
        .unwrap()
        .expect("SQL replacement occurs while genuine exact original provider child is live");
    assert_eq!(
        (live.uid, live.birth, live.session),
        (child.uid, child.birth, child.session)
    );
    assert!(!live.zombie);
    session.wait_for_new_cursorless_condition(before,|frame|{
        let text=frame.contents();text.contains(if after_receipt{"Thread write observed; current source unavailable"}else{"Thread publication not started"})
        &&!text.contains(THREAD_BODY)&&!text.contains(REPLACEMENT)&&!text.contains(FOREIGN)&&!text.contains("Original native reply 界")&&!text.contains("native-thread-actor")&&!text.contains(&operation.title())
    },"Exact original draft envelope changed: zero retry, private views retired and external replacement preserved",Duration::from_secs(8));
    wait_original_child_gone(&mut session, &child);
    let retained: String = raw(&fixture)
        .query_row(
            "SELECT body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource=?2",
            rusqlite::params![HOST, key],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(retained, encoded);
    let after_marker: (i64, String, String, String) = raw(&fixture)
        .query_row(
            "SELECT attempt_id,state,payload_sha256,observed_head FROM pr_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(after_marker, before_marker);
    let count: u64 = raw(&fixture)
        .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(count, 1);
    session.send_key(b"\r");
    session.poll_output(Duration::from_millis(10));
    exact(&session, &fixture, &steps, usize::from(after_receipt));
    assert_eq!(untouched_sql(&fixture), sql);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), git_journal);
    assert_read_only_git(&fixture);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
    let durable = sql_state(&fixture);
    let mut restarted = launch(&fixture, mouse);
    assert!(!restarted.screen().contains(REPLACEMENT));
    assert_eq!(sql_state(&fixture), durable);
    exact(&restarted, &fixture, &steps, usize::from(after_receipt));
    restarted.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn review_threads_v1_publication_keyboard_external_draft_replaced_before_final_sql_guard_never_posts(
) {
    journey(false, false)
}
#[test]
fn review_threads_v1_publication_mouse_external_draft_replaced_before_final_sql_guard_never_posts()
{
    journey(true, false)
}
#[test]
fn review_threads_v1_publication_keyboard_external_draft_replaced_after_receipt_preserves_newer_body(
) {
    journey(false, true)
}
#[test]
fn review_threads_v1_publication_mouse_external_draft_replaced_after_receipt_preserves_newer_body()
{
    journey(true, true)
}
