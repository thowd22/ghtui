//! Real confirmed Base reference restart history, independent of edited draft
//! text/current head/tip. Registered only after the current Base gate closes.
use super::*;

#[test]
fn pr_metadata_base_keyboard_confirmed_original_restart_edits_and_new_intent_never_resend_or_release_receipt(
) {
    original(false);
}
#[test]
fn pr_metadata_base_mouse_confirmed_original_restart_edits_and_new_intent_never_resend_or_release_receipt(
) {
    original(true);
}
fn generation(fixture: &Fixture) -> i64 {
    Connection::open(cache_path(fixture))
        .unwrap()
        .query_row(
            "SELECT generation FROM pr_write_scopes WHERE host=?1 AND principal=?2 AND pr_node=?3",
            params![scope().host, scope().login, NODE],
            |row| row.get(0),
        )
        .unwrap()
}
fn original(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let before = original_parent();
    let (publishing, after) = base_publication(&before);
    let mut changed_head = after.clone();
    changed_head["head"]["sha"] = json!(CHANGED_TIP);
    let mut steps = startup(&before);
    steps.extend(ordinary_catalogue(&before));
    steps.extend(publishing);
    assert_eq!(steps.len(), 63);
    steps.extend(startup(&changed_head));
    steps.extend(identity());
    steps.extend(identity());
    steps.extend(identity());
    steps.extend(identity());
    assert_eq!(steps.len(), 89);
    steps[88].delay_ms = 200;
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    start(&mut session, mouse);
    form(&mut session, mouse);
    load_picker(&mut session, &fixture, mouse, 28);
    choose_branch(&mut session, mouse);
    preview_tip(&mut session, &fixture, mouse, 28);
    // Real cleanup failure retains the exact ref that the controller durably
    // saved before claim, rather than synthesizing a reference in the fixture.
    Connection::open(cache_path(&fixture)).unwrap().execute_batch(
        "CREATE TRIGGER keep_base_original BEFORE DELETE ON drafts WHEN OLD.host='github.invalid' AND OLD.login='user-id:42' AND OLD.resource='example/repo/pull/7/composer/base' BEGIN SELECT RAISE(ABORT,'fixture retain exact Base original'); END;").unwrap();
    submit(&mut session, mouse);
    session.wait_for_screen("Published • Change base branch");
    session.wait_for_overlay_close("┌ Change base branch ");
    unchanged_metadata(&mut session, mouse);
    let writes = observed_requests(&mut session, &fixture, 63);
    assert_eq!(mutation_count(&writes), 1);
    assert_confirmed(&fixture, &[("legacy-base", BASE)], &after);
    let saved = Cache::open(&cache_path(&fixture))
        .unwrap()
        .draft(&scope(), &format!("{REPOSITORY}/pull/7/composer/base"))
        .unwrap()
        .unwrap();
    let saved: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(saved["values"], json!({"body":BASE}));
    assert_eq!(
        saved["publication"]["attempt_id"],
        receipts(&fixture)[0].attempt_id
    );
    let original_receipt = receipts(&fixture)[0].confirmed.clone();
    let original_attempt = receipts(&fixture)[0].attempt_id;
    let original_generation = generation(&fixture);
    preserve_git(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);

    let mut restarted = Session::launch(&fixture, 80, 24);
    start(&mut restarted, mouse);
    menu(&mut restarted, mouse, "Change base branch");
    restarted.wait_for_screen("[View original]");
    restarted.wait_for_screen("[F9 New intent]");
    assert!(!restarted.screen().contains("[F4 Choose]"));
    replace_field(
        &mut restarted,
        mouse,
        "Base branch",
        "edited draft is not the original receipt",
    );
    if mouse {
        restarted.click_visible_action("[View original]");
    } else {
        restarted.focus_visible_form_action("[View original]");
        restarted.send_key(b"\r");
    }
    restarted.wait_for_screen("Original Base publication record");
    restarted.wait_for_screen("Recorded state: Confirmed • do not resend");
    restarted.wait_for_screen("Original observed head:");
    restarted.wait_for_screen(HEAD);
    let inspected = observed_requests(&mut restarted, &fixture, 85);
    assert_eq!(mutation_count(&inspected), 1);
    assert_eq!(inspected[81]["args"], json!(["--version"]));
    assert_eq!(inspected[82]["args"][1], "user");
    assert_eq!(inspected[83]["args"], json!(["--version"]));
    assert_eq!(inspected[84]["args"][1], "user");
    assert_eq!(generation(&fixture), original_generation);
    assert_eq!(receipts(&fixture)[0].attempt_id, original_attempt);
    assert_eq!(receipts(&fixture)[0].confirmed, original_receipt);
    assert_eq!(receipts(&fixture).len(), 1);
    // Another explicit original inspection still needs fresh principal proof,
    // but never any branch lookup, mutation, release or inferred new intent.
    if mouse {
        restarted.click_visible_action("[View original]");
    } else {
        restarted.focus_visible_form_action("[View original]");
        restarted.send_key(b"\r");
    }
    restarted.wait_for_screen("[Esc Cancel]");
    restarted.wait_for_command_count(&fixture, 89);
    restarted
        .try_wait_for_condition(
            |screen| {
                screen.contains("Recorded state: Confirmed • do not resend")
                    && screen.contains("[Keep draft / Close]")
                    && !screen.contains("[Esc Cancel]")
            },
            "completed second exact original read before New intent",
            Duration::from_secs(8),
        )
        .unwrap();
    restarted.poll_output(Duration::from_millis(1));
    let inspected = observed_requests(&mut restarted, &fixture, 89);
    assert_eq!(mutation_count(&inspected), 1);
    choose(&mut restarted, mouse, "[F9 New intent]", b"\x1b[20~");
    restarted.wait_for_screen("original durable protection remains");
    restarted.wait_for_screen("[F4 Choose]");
    choose(&mut restarted, mouse, "[Preview]", b"\x10");
    restarted.wait_for_screen(BASE_REQUIRED);
    assert_eq!(observed_requests(&mut restarted, &fixture, 89), inspected);
    assert_eq!(generation(&fixture), original_generation);
    assert_eq!(receipts(&fixture)[0].confirmed, original_receipt);
    assert_eq!(receipts(&fixture)[0].attempt_id, original_attempt);
    close_form(&mut restarted, mouse);
    unchanged_metadata(&mut restarted, mouse);
    preserve_git(&fixture, &head, &index, &status);
    restarted.quit_and_assert_restored_terminal(mouse);
    assert_eq!(observed_requests(&mut restarted, &fixture, 89), inspected);
    assert_eq!(receipts(&fixture).len(), 1);
    assert_eq!(generation(&fixture), original_generation);
    assert_eq!(receipts(&fixture)[0].confirmed, original_receipt);
    preserve_git(&fixture, &head, &index, &status);
}
