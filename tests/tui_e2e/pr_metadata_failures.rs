//! Confirmed metadata writes survive denied reads and final principal changes.
use super::*;

#[test]
fn pr_metadata_keyboard_confirmed_context_denial_hides_old_rows_without_resend() {
    failure(false, false);
}
#[test]
fn pr_metadata_mouse_confirmed_context_denial_hides_old_rows_without_resend() {
    failure(true, false);
}
#[test]
fn pr_metadata_keyboard_confirmed_final_reused_login_new_uid_hides_old_rows_without_resend() {
    failure(false, true);
}
#[test]
fn pr_metadata_mouse_confirmed_final_reused_login_new_uid_hides_old_rows_without_resend() {
    failure(true, true);
}
fn hidden(session: &mut Session) {
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("Publication is confirmed. Do not resend")
                    && screen.contains("private pages hidden")
                    && !screen.contains("[u Metadata]")
                    && !screen.contains("[Submit]")
                    && !screen.contains("[Confirm action]")
                    && !screen.contains("[Keep draft / Close]")
                    && !screen.contains("Native collaboration")
                    && !screen.contains(TITLE)
                    && !screen.contains("@triager")
                    && !screen.contains("@reviewer")
                    && !screen.contains("October release")
                    && !screen.contains("example/repo")
            },
            "complete confirmed-but-private-hidden native frame",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
}
fn original_confirmation(fixture: &Fixture) -> (i64, String, Value) {
    let rows = receipts(fixture);
    assert_eq!(rows.len(), 1);
    let original = &rows[0];
    assert_eq!(original.command, "legacy-title");
    assert_eq!(original.state, "confirmed");
    assert_eq!(original.selected_resource, "pr");
    assert_eq!(original.observed_head, HEAD);
    assert!(original.attempt_id > 0);
    assert_eq!(
        original.payload_sha256,
        PrWriteIntent {
            command: "legacy-title".into(),
            selected_resource: "pr".into(),
            observed_head: HEAD.into(),
            values: BTreeMap::from([("body".into(), TITLE.into())]),
        }
        .digest()
        .unwrap()
    );
    assert_eq!(
        original.confirmed,
        json!({"kind":"metadata","resource_id":NODE,"node_id":NODE})
    );
    (
        original.attempt_id,
        original.payload_sha256.clone(),
        original.confirmed.clone(),
    )
}
fn failure(mouse: bool, account_change: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let database = cache_path(&fixture);
    let intent = json!({"query":"original UID42 metadata view intent","repository":REPOSITORY});
    Cache::open(&database)
        .unwrap()
        .save_view(&scope(), "metadata-private-view", &intent)
        .unwrap();
    let original = target("Native collaboration", "A fixture description", "main");
    let titled = target(TITLE, "A fixture description", "main");
    let mut steps = at_target(initial(), &original);
    steps.extend(at_target(details(vec![starter()]), &original));
    steps.extend(metadata(&original));
    let mut changing = publication(&original, &titled, "title", TITLE);
    if account_change {
        changing.last_mut().unwrap().stdout =
            serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    } else {
        // The original PATCH/read-back and all eleven inspection calls succeed.
        // Only the new independent issue-metadata read is denied afterward.
        changing.truncate(21);
        changing.last_mut().unwrap().stdout.clear();
        changing.last_mut().unwrap().status = 1;
        changing.last_mut().unwrap().stderr =
            b"HTTP 403 Forbidden: fixture context access revoked".to_vec();
    }
    let count = 18 + changing.len();
    assert_eq!(count, if account_change { 43 } else { 39 });
    steps.extend(changing);
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    session.send_key(b"1");
    open(&mut session, mouse);
    choose(&mut session, mouse, "[u Metadata]", b"u");
    unchanged_metadata(&mut session, mouse);
    let old = session.locate("Native collaboration").unwrap();
    menu(&mut session, mouse, "Edit PR title");
    replace_field(&mut session, mouse, "Title", TITLE);
    local_preview(&mut session, &fixture, mouse, 18);
    submit(&mut session, mouse);
    hidden(&mut session);
    let before = observed_requests(&mut session, &fixture, count);
    assert_eq!(mutation_count(&before), 1);
    let receipt = original_confirmation(&fixture);
    session.resize(80, 24);
    hidden(&mut session);
    if mouse {
        session.click(old.0.min(70), old.1.min(18));
    } else {
        session.send_key(b"\r");
    }
    // Actual focus after the stale input and the post-exit journal both guard
    // against delayed extra opening/publication, not only a screen snapshot.
    session.focus_visible_action("[q Quit]");
    hidden(&mut session);
    assert_eq!(observed_requests(&mut session, &fixture, count), before);
    let cache = Cache::open(&database).unwrap();
    assert_eq!(
        cache.saved_view(&scope(), "metadata-private-view").unwrap(),
        Some(intent)
    );
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.saved_views(&other).unwrap().is_empty());
    let sql = Connection::open(&database).unwrap();
    for table in ["pr_write_attempts", "pr_write_scopes"] {
        let count: u64 = sql
            .query_row(
                &format!("SELECT COUNT(*) FROM {table} WHERE host=?1 AND principal=?2"),
                params![other.host, other.login],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "Changed account cannot inherit publication state");
    }
    assert_eq!(original_confirmation(&fixture), receipt);
    assert!(
        cache
            .draft(&scope(), &format!("{REPOSITORY}/pull/7/composer/title"))
            .unwrap()
            .is_none(),
        "Confirmed original draft is removed, not left as an automatic retry"
    );
    preserve_git(&fixture, &head, &index, &status);
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(observed_requests(&mut session, &fixture, count), before);
    assert_eq!(original_confirmation(&fixture), receipt);
    preserve_git(&fixture, &head, &index, &status);
}
