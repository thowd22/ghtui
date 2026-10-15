//! Real binary declarations, UNRUN until Root's serialized verifier. Five
//! independent actual processes retain only draft text/anchors, never approval.
use super::*;
const FIRST_BODY: &str = "First local comment to edit 界";
const SECOND_BODY: &str = "Second local comment to remove 界";
const KEPT_BODY: &str = "Edited first comment survives restarts 界";

fn saved(session: &mut Session, fixture: &Fixture, bodies: &[&str]) -> (String, String) {
    let find = || {
        raw(fixture).prepare("SELECT resource,body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'review-draft/%'").unwrap()
        .query_map([HOST],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).unwrap().map(Result::unwrap)
        .find(|(_,encoded)|serde_json::from_str::<Value>(encoded).is_ok_and(|value|value["version"]==3&&value["publication"].is_null()
            &&value["values"]["summary"]==SUMMARY&&value["values"]["comments"].as_array().is_some_and(|rows|
                rows.len()==bodies.len()&&rows.iter().zip(bodies).all(|(row,body)|row["body"]==*body))))
    };
    session.try_wait_for_condition(|_|find().is_some(),"Actual normal autosave ACK of exact retained ordered LOCAL comments, not publication approval",Duration::from_secs(8)).unwrap();
    find().unwrap()
}
fn field_next(session: &mut Session, mouse: bool, label: &str, body: &str) {
    choose(session, mouse, "[F6 Fields↓]", b"\x1b[17~");
    edited_field(session, &format!("{label}:"), body);
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", label);
    }
}
fn browser_batch(session: &mut Session, mouse: bool) {
    let before = session.completed_cursorless_end();
    choose(session, mouse, "[F10 Reviews]", b"\x1b[21~");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Scoped reviews")
                && text.contains("[B New batch]")
                && text.contains("[Esc Close]")
        },
        "NEW actual empty-catalogue browser before local batch open",
        Duration::from_secs(8),
    );
    choose(session, mouse, "[B New batch]", b"B");
    edited(session, SUMMARY);
}
fn rewrite_kept_readback(steps: &mut [GhStep]) {
    for step in steps {
        if !step.stdin.is_empty() {
            let mut payload: Value = serde_json::from_slice(&step.stdin).unwrap();
            if let Some(comments) = payload.get_mut("comments").and_then(Value::as_array_mut) {
                assert_eq!(comments.len(), 1);
                comments[0]["body"] = json!(KEPT_BODY);
                comments[0]["side"] = json!("LEFT");
                comments[0]["start_side"] = json!("LEFT");
            }
            step.stdin = serde_json::to_vec(&payload).unwrap();
        }
        if step
            .args
            .get(1)
            .is_some_and(|endpoint| endpoint.contains("reviews/810/comments"))
        {
            let mut rows: Value = serde_json::from_slice(&step.stdout).unwrap();
            assert_eq!(rows.as_array().unwrap().len(), 1);
            rows[0]["id"] = json!(910);
            rows[0]["node_id"] = json!("ACTUAL_NATIVE_COMMENT_910");
            rows[0]["body"] = json!(KEPT_BODY);
            rows[0]["side"] = json!("LEFT");
            rows[0]["start_side"] = json!("LEFT");
            step.stdout = serde_json::to_vec(&rows).unwrap();
        }
        if let Ok(mut actual) = serde_json::from_slice::<Value>(&step.stdout) {
            if actual["id"] == 810 && actual.get("state").is_some() {
                actual["body"] = json!(SUMMARY);
                step.stdout = serde_json::to_vec(&actual).unwrap();
            }
        }
    }
}
fn no_write(session: &Session, fixture: &Fixture, expected: &[GhStep], git: &[Value]) {
    exact_commands(session, fixture, expected, 0);
    assert_eq!(git_commands(fixture), git);
    assert_eq!(
        raw(fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
fn after_published_restart(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    previous: usize,
    steps: &[GhStep],
) {
    // SAME actual navigation as files_after_restart, but prior known creation
    // is honestly counted as ONE write, never disguised as read-only history.
    choose(session, mouse, "[: Commands]", b":");
    overlay_ready(
        session,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    session.send_key(b"\x01");
    session.paste("Discover repositories");
    overlay_ready(
        session,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(session, mouse, "> Discover repositories", b"\r");
    session.wait_for_overlay_close("Commands · current context");
    ready(session, &[TITLE, "[s Source]", "[Esc Close]"]);
    exact_commands(session, fixture, &steps[..previous], 1);
    discover(session, mouse);
    session.wait_for_command_count(fixture, previous + 7);
    ready(session, &[TITLE, FIRST]);
    if !mouse {
        session.send_key(b"\t");
    }
    choose(session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_command_count(fixture, previous + 18);
    ready(session, &[ROW_TITLE]);
    choose(session, mouse, "1 Inbox", b"1");
    choose(session, mouse, ROW_TITLE, b"\r");
    session.wait_for_command_count(fixture, previous + 31);
    ready(session, &[DETAIL_TITLE, DETAIL_BODY, "[b Back]"]);
    choose(session, mouse, "[f Files]", b"f");
    session.wait_for_command_count(fixture, previous + 41);
    ready(
        session,
        &["src/界.rs", "[O Old image]", "Old image unverified"],
    );
    choose(session, mouse, "[N Next file]", b"N");
    ready(session, &["> File src/界.rs"]);
}
fn journey(mouse: bool) {
    let (fixture, h, base) = fixture_and_steps(|_| {});
    let startup = base[..base.len() - 17].to_vec();
    let images = base[base.len() - 17..].to_vec();
    let mut steps = base.clone();
    steps.extend(source_open(&h, json!([])));
    let first_end = steps.len();
    steps.extend(base.clone());
    steps.extend(source_open(&h, json!([])));
    let second_end = steps.len();
    steps.extend(startup.clone());
    steps.extend(source_open(&h, json!([])));
    steps.extend(source_open(&h, json!([])));
    let third_end = steps.len();
    steps.extend(startup.clone());
    steps.extend(source_open(&h, json!([])));
    steps.extend(source_open(&h, json!([])));
    let create_preview_start = steps.len();
    let mut create_preview = opening(&h);
    create_preview.extend(images[3..images.len() - 3].iter().cloned());
    create_preview.push(catalogue(json!([])));
    create_preview.extend(closing(&h));
    steps.extend(create_preview);
    let create_confirm = steps.len();
    let (mut confirmation, _, _) = confirm(&h, Effect::Create, Failure::None, "LEFT", &images);
    rewrite_kept_readback(&mut confirmation);
    steps.extend(confirmation);
    let fourth_end = steps.len();
    steps.extend(startup.clone());
    steps.extend(source_open(
        &h,
        json!([row(&h, Effect::Create, Failure::None, true)]),
    ));
    let mut selected = selection(&h, Effect::Create, Failure::None, "LEFT");
    rewrite_kept_readback(&mut selected);
    steps.extend(selected);
    steps.extend(source_open(
        &h,
        json!([row(&h, Effect::Create, Failure::None, true)]),
    ));
    let mut submit_preview = opening(&h);
    let mut actual_snapshot = snapshot(&h, Effect::Create, Failure::None, true, "LEFT");
    rewrite_kept_readback(&mut actual_snapshot);
    submit_preview.extend(actual_snapshot);
    submit_preview.extend(closing(&h));
    steps.extend(submit_preview);
    let submit_confirm = steps.len();
    let (mut confirmation, _, _) = confirm(&h, Effect::Comment, Failure::None, "LEFT", &images);
    rewrite_kept_readback(&mut confirmation);
    steps.extend(confirmation);
    fixture.scripted_gh(&steps);
    foreign_draft(&fixture);
    let git = git_state(&fixture);
    let drafts = stable_drafts(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();

    // FIRST actual process: genuine LEFT range, one LOCAL comment. No Preview,
    // journal claim or API write; explicit keep draft then terminal restoration.
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let trace = git_commands(&fixture);
    choose(&mut session, mouse, "[O Old image]", b"O");
    overlay_ready(
        &mut session,
        "Immutable file images",
        &["[R Range]", "[V Batch]"],
    );
    selected_image_range(&mut session, mouse, "LEFT", &h.h);
    choose(&mut session, mouse, "[V Batch]", b"V");
    edited(&mut session, "");
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", "Review summary");
    }
    session.paste(SUMMARY);
    field_next(&mut session, mouse, "Exact selected pending comment", "");
    session.paste(FIRST_BODY);
    let first = saved(&mut session, &fixture, &[FIRST_BODY]);
    let original_first: Value =
        serde_json::from_str::<Value>(&first.1).unwrap()["values"]["comments"][0]["anchor"].clone();
    kept(&mut session, mouse);
    no_write(&session, &fixture, &steps[..first_end], &trace);
    session.quit_and_assert_restored_terminal(mouse);

    // SECOND process: fresh real Source/Files/RIGHT range. First comment is a
    // restored local field, not overwritten by the new selected anchor field.
    let mut session = launch(&fixture, mouse);
    files_after_restart(&mut session, &fixture, mouse, first_end, &steps);
    let trace = git_commands(&fixture);
    choose(&mut session, mouse, "[O Old image]", b"O");
    overlay_ready(
        &mut session,
        "Immutable file images",
        &["[R Range]", "[V Batch]"],
    );
    selected_image_range(&mut session, mouse, "RIGHT", &h.h);
    choose(&mut session, mouse, "[V Batch]", b"V");
    edited(&mut session, SUMMARY);
    field_next(&mut session, mouse, "Saved local comment #1", FIRST_BODY);
    field_next(&mut session, mouse, "Exact selected pending comment", "");
    session.paste(SECOND_BODY);
    let two = saved(&mut session, &fixture, &[FIRST_BODY, SECOND_BODY]);
    assert_eq!(two.0, first.0);
    let before_remove: Value = serde_json::from_str(&two.1).unwrap();
    assert_eq!(
        before_remove["values"]["comments"][0]["anchor"],
        original_first
    );
    assert_eq!(
        before_remove["values"]["comments"][1]["anchor"]["side"],
        "RIGHT"
    );
    kept(&mut session, mouse);
    no_write(&session, &fixture, &steps[..second_end], &trace);
    session.quit_and_assert_restored_terminal(mouse);

    // THIRD process: edit one original retained field; remove the other ONLY
    // through the explicit actual selected LOCAL action. Tiny geometry rejects
    // F12 with no hidden target and leaves exact durable bytes unchanged.
    let mut session = launch(&fixture, mouse);
    files_after_restart(&mut session, &fixture, mouse, second_end, &steps);
    let trace = git_commands(&fixture);
    browser_batch(&mut session, mouse);
    field_next(&mut session, mouse, "Saved local comment #1", FIRST_BODY);
    session.send_key(b"\x01");
    session.paste(KEPT_BODY);
    saved(&mut session, &fixture, &[KEPT_BODY, SECOND_BODY]);
    field_next(&mut session, mouse, "Saved local comment #2", SECOND_BODY);
    let before = saved(&mut session, &fixture, &[KEPT_BODY, SECOND_BODY]);
    session.resize_owned_single_action_modal_v2(
        40,
        14,
        "Original review • resize required",
        "[Esc Close]",
    );
    session.send_key(b"\x1b[24~");
    ready(&mut session, &["resize required", "[Esc Close]"]);
    assert_eq!(
        raw(&fixture)
            .query_row(
                "SELECT body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource=?2",
                rusqlite::params![HOST, before.0],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        before.1
    );
    session.resize(80, 24);
    edited_field(&mut session, "Saved local comment #2:", SECOND_BODY);
    ready(&mut session, &["[F12 Remove local comment]"]);
    choose(
        &mut session,
        mouse,
        "[F12 Remove local comment]",
        b"\x1b[24~",
    );
    let retained = saved(&mut session, &fixture, &[KEPT_BODY]);
    let retained_json: Value = serde_json::from_str(&retained.1).unwrap();
    assert_eq!(
        retained_json["values"]["comments"][0]["anchor"],
        original_first
    );
    assert!(retained_json["publication"].is_null());
    kept(&mut session, mouse);
    no_write(&session, &fixture, &steps[..third_end], &trace);
    session.quit_and_assert_restored_terminal(mouse);

    // FOURTH process: restart still cannot Confirm. Fresh canonical rehydration
    // and useful same-Doc Preview, poison-paste refusal, THEN explicit creation.
    let mut session = launch(&fixture, mouse);
    files_after_restart(&mut session, &fixture, mouse, third_end, &steps);
    let trace = git_commands(&fixture);
    browser_batch(&mut session, mouse);
    field_next(&mut session, mouse, "Saved local comment #1", KEPT_BODY);
    assert!(!session.screen().contains(SECOND_BODY));
    form_control(&mut session, mouse, Effect::Create, "[Confirm action]");
    assert_eq!(
        commands(&session, &fixture).len(),
        create_preview_start,
        "restored local values never restore Preview permission"
    );
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("READ-ONLY PREVIEW")
                && text.contains(KEPT_BODY)
                && text.contains("[Confirm action]")
                && !text.contains(SECOND_BODY)
        },
        "NEW exact rehydrated retained local comment Preview after restart",
        Duration::from_secs(8),
    );
    assert_eq!(commands(&session, &fixture).len(), create_confirm);
    let before = session.completed_cursorless_end();
    session.paste("LOCAL_BATCH_PREVIEW_POISON");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("READ-ONLY PREVIEW")
                && text.contains(KEPT_BODY)
                && !text.contains("LOCAL_BATCH_PREVIEW_POISON")
        },
        "Prepared Preview ignores poison without invalidating exact local batch",
        Duration::from_secs(8),
    );
    form_control(&mut session, mouse, Effect::Create, "[Confirm action]");
    ready(
        &mut session,
        &["Review #810 effect confirmed", "snapshot retired"],
    );
    exact_commands(&session, &fixture, &steps[..fourth_end], 1);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        raw(&fixture)
            .query_row(
                "SELECT count(*) FROM drafts WHERE resource=?1",
                [&retained.0],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    session.quit_and_assert_restored_terminal(mouse);

    // FIFTH process: freshly Resume actual server PENDING, then explicitly
    // submit COMMENT with exact retained comment readback. No local metadata
    // supplies a review ID, approval, write permit or submission on restart.
    let mut session = launch(&fixture, mouse);
    after_published_restart(&mut session, &fixture, mouse, fourth_end, &steps);
    let trace = git_commands(&fixture);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[F10 Reviews]", b"\x1b[21~");
    browser_ready(&mut session, before, "#810");
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[R Resume selected]", b"R");
    browser_ready(&mut session, before, KEPT_BODY);
    choose(&mut session, mouse, "[C Submit comment]", b"C");
    edited(&mut session, SUMMARY);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("READ-ONLY PREVIEW")
                && text.contains("Exact review 810")
                && text.contains("[Confirm action]")
        },
        "NEW actual server PENDING Submit Preview, not restored local publication permission",
        Duration::from_secs(8),
    );
    assert_eq!(commands(&session, &fixture).len(), submit_confirm);
    form_control(&mut session, mouse, Effect::Comment, "[Confirm action]");
    ready(
        &mut session,
        &["Review #810 effect confirmed", "snapshot retired"],
    );
    exact_commands(&session, &fixture, &steps, 2);
    let db = raw(&fixture);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_attempts WHERE state='confirmed'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
    assert_eq!(db.query_row("SELECT count(*) FROM pr_write_attempts WHERE command IN('review-create-pending-v2','review-submit-v2')",[],|row|row.get::<_,i64>(0)).unwrap(),2);
    drop(db);
    assert_eq!(stable_drafts(&fixture), drafts);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn pending_reviews_v10_keyboard_two_comments_restart_edit_remove_restart_preview_create_then_submit(
) {
    journey(false)
}
#[test]
fn pending_reviews_v10_mouse_two_comments_restart_edit_remove_restart_preview_create_then_submit() {
    journey(true)
}
