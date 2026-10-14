//! Actual process death/receipt-store failure. The original ordered reference
//! survives; read-only restart never grants candidate authority or resends.
use super::*;

fn snap(f: &Fixture) -> (String, Vec<u8>, String) {
    (
        f.git(&["rev-parse", "HEAD"]),
        fs::read(f.repo().join(".git/index")).unwrap(),
        f.git(&["status", "--porcelain=v2", "-z"]),
    )
}
fn sql_original(f: &Fixture) -> (i64, String, Option<String>, String, String) {
    Connection::open(cache_path(f)).unwrap().query_row("SELECT attempt_id,state,confirmed_json,observed_head,payload_sha256 FROM pr_write_attempts WHERE host='github.invalid' AND principal='user-id:42' AND pr_node='PR_7' AND repository='example/repo' AND pr_number=7 AND command='metadata-add-label'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap()
}
fn close_record(session: &mut Session, mouse: bool) {
    session.wait_for_screen("[Keep draft / Close]");
    choose(session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_overlay_close("┌ Add label ");
}
fn ready(session: &mut Session, f: &Fixture, mouse: bool) {
    open_pr(session, mouse);
    open_form(session, mouse, PrMetadataCommand::AddLabel);
    choose_value(session, f, mouse, PrMetadataCommand::AddLabel, 32);
    choose(session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("Exact effect: Add label");
    assert_no_publication(f);
}
fn prefix(p: &Value) -> Vec<GhStep> {
    let mut s = startup(p);
    s.extend(bound(identity(), p));
    s.extend(bound(identity(), p));
    s.extend(source(PrMetadataCommand::AddLabel, p));
    s
}
fn open_original(session: &mut Session, f: &Fixture, mouse: bool, count: usize) {
    open_pr(session, mouse);
    menu(session, mouse, "Add label");
    records(session, f, count);
    session.wait_for_screen("Recorded state: Pending");
    session.wait_for_screen("ORIGINAL METADATA RECORD");
    session.wait_for_screen("[View original]");
    assert!(!session.screen().contains("[F4 Choose]"));
}

#[test]
fn pr_metadata_actions_recovery_keyboard_sigkill_invoked_child_retains_original_pending_without_resend(
) {
    crash(false);
}
#[test]
fn pr_metadata_actions_recovery_mouse_sigkill_invoked_child_retains_original_pending_without_resend(
) {
    crash(true);
}
fn crash(mouse: bool) {
    let f = fixture();
    let p = current_parent();
    let after = after(PrMetadataCommand::AddLabel);
    let git = snap(&f);
    let mut s = prefix(&p);
    s.extend(bound(identity(), &p));
    s.extend(values::preparation_from(PrMetadataCommand::AddLabel, &p));
    let mut invoked = values::publication_from(
        PrMetadataCommand::AddLabel,
        &after,
        values::mutation_input(PrMetadataCommand::AddLabel),
    )
    .remove(0);
    invoked.delay_ms = 20_000;
    s.push(invoked);
    s.extend(startup(&after));
    s.extend(bound(identity(), &after));
    s.extend(bound(identity(), &after));
    assert_eq!(s.len(), 69);
    f.scripted_gh(&s);
    let mut first = Session::launch(&f, 120, 40);
    ready(&mut first, &f, mouse);
    confirm(&mut first, mouse);
    let calls = records(&mut first, &f, 47);
    assert_eq!(mutation_count(&calls), 1);
    let original = sql_original(&f);
    assert!(original.0 > 0);
    assert_eq!(original.1, "pending");
    assert!(original.2.is_none());
    assert_eq!(original.3, "a".repeat(40));
    assert_eq!(original.4.len(), 64);
    let saved = Cache::open(&cache_path(&f))
        .unwrap()
        .draft(&scope(), "example/repo/pull/7/composer/metadata-add-label")
        .unwrap()
        .unwrap();
    let draft: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(draft["publication"]["attempt_id"], original.0);
    assert_eq!(draft["publication"]["payload_sha256"], original.4);
    let pid = i32::try_from(calls.last().unwrap()["pid"].as_i64().unwrap()).unwrap();
    assert!(pid > 0);
    first.terminate_abruptly();
    let result = unsafe { libc::kill(pid, libc::SIGKILL) };
    assert!(result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH));
    assert_eq!(sql_original(&f), original);
    let mut second = Session::launch(&f, 80, 24);
    open_original(&mut second, &f, mouse, 69);
    assert_eq!(mutation_count(&records(&mut second, &f, 69)), 1);
    assert_eq!(sql_original(&f), original);
    assert_eq!(
        Cache::open(&cache_path(&f))
            .unwrap()
            .draft(&scope(), "example/repo/pull/7/composer/metadata-add-label")
            .unwrap()
            .unwrap(),
        saved
    );
    close_record(&mut second, mouse);
    preserve_git(&f, &git.0, &git.1, &git.2);
    second.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn pr_metadata_actions_recovery_keyboard_actual_remote_confirmation_receipt_sql_failure_cannot_be_resent(
) {
    receipt(false);
}
#[test]
fn pr_metadata_actions_recovery_mouse_actual_remote_confirmation_receipt_sql_failure_cannot_be_resent(
) {
    receipt(true);
}
fn receipt(mouse: bool) {
    let f = fixture();
    let p = current_parent();
    let after = after(PrMetadataCommand::AddLabel);
    let git = snap(&f);
    let mut s = prefix(&p);
    s.extend(bound(identity(), &p));
    s.extend(values::preparation_from(PrMetadataCommand::AddLabel, &p));
    s.extend(values::publication_from(
        PrMetadataCommand::AddLabel,
        &after,
        values::mutation_input(PrMetadataCommand::AddLabel),
    ));
    s.extend(bound(identity(), &p));
    s.extend(startup(&after));
    s.extend(bound(identity(), &after));
    s.extend(bound(identity(), &after));
    assert_eq!(s.len(), 76);
    f.scripted_gh(&s);
    let mut first = Session::launch(&f, 120, 40);
    ready(&mut first, &f, mouse);
    Connection::open(cache_path(&f)).unwrap().execute_batch("CREATE TRIGGER metadata_receipt_fails BEFORE UPDATE ON pr_write_attempts WHEN NEW.state='confirmed' BEGIN SELECT RAISE(ABORT,'fixture receipt storage failed'); END;").unwrap();
    confirm(&mut first, mouse);
    records(&mut first, &f, 54);
    first.wait_for_screen("Published • private pages hidden • do not resend");
    first.wait_for_screen("[View original]");
    let original = sql_original(&f);
    assert_eq!(original.1, "pending");
    assert!(original.2.is_none());
    assert_eq!(mutation_count(&records(&mut first, &f, 54)), 1);
    assert!(!first.screen().contains("Your repository permissions:"));
    close_record(&mut first, mouse);
    first.quit_and_assert_restored_terminal(mouse);
    let mut second = Session::launch(&f, 80, 24);
    open_original(&mut second, &f, mouse, 76);
    assert_eq!(sql_original(&f), original);
    assert_eq!(mutation_count(&records(&mut second, &f, 76)), 1);
    assert!(!second.screen().contains("Recorded state: Confirmed"));
    close_record(&mut second, mouse);
    preserve_git(&f, &git.0, &git.1, &git.2);
    second.quit_and_assert_restored_terminal(mouse);
}
