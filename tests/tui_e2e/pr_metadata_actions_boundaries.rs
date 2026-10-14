//! Native source/publication boundary journeys; strict queued transport and
//! independent durable/Git state are checked, never inferred from screenshots.
use super::*;

fn git_snapshot(f: &Fixture) -> (String, Vec<u8>, String) {
    (
        f.git(&["rev-parse", "HEAD"]),
        fs::read(f.repo().join(".git/index")).unwrap(),
        f.git(&["status", "--porcelain=v2", "-z"]),
    )
}
fn no_write(session: &mut Session, f: &Fixture, count: usize) {
    assert_eq!(mutation_count(&records(session, f, count)), 0);
    assert_no_publication(f);
}
fn prepare(session: &mut Session, f: &Fixture, mouse: bool, command: PrMetadataCommand) {
    open_pr(session, mouse);
    open_form(session, mouse, command);
    choose_value(session, f, mouse, command, 32);
    choose(session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("READ-ONLY PREVIEW");
    no_write(session, f, 32);
}
fn first_steps(command: PrMetadataCommand, p: &Value) -> Vec<GhStep> {
    let mut s = startup(p);
    s.extend(bound(identity(), p));
    s.extend(bound(identity(), p));
    s.extend(source(command, p));
    s
}

#[test]
fn pr_metadata_actions_boundary_keyboard_same_login_numeric_reassignment_hides_choices() {
    source_identity(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_same_login_numeric_reassignment_hides_choices() {
    source_identity(true);
}
fn source_identity(mouse: bool) {
    let f = fixture();
    let p = current_parent();
    let baseline = git_snapshot(&f);
    let mut s = startup(&p);
    s.extend(bound(identity(), &p));
    s.extend(bound(identity(), &p));
    s.push(values::get("user", json!({"id":43,"login":"requester"})));
    let mut final_id = bound(identity(), &p);
    final_id[1].stdout = serde_json::to_vec(&json!({"id":43,"login":"requester"})).unwrap();
    s.extend(final_id);
    assert_eq!(s.len(), 25);
    f.scripted_gh(&s);
    let mut session = Session::launch(&f, 120, 40);
    open_pr(&mut session, mouse);
    open_form(&mut session, mouse, PrMetadataCommand::AddLabel);
    choose(&mut session, mouse, "[F4 Choose]", b"\x1bOS");
    records(&mut session, &f, 25);
    session.wait_for_screen("private pages hidden");
    assert!(
        !session.screen().contains("Selected value:")
            && !session.screen().contains("Your repository permissions:")
            && !session.screen().contains("Private description")
    );
    no_write(&mut session, &f, 25);
    session.resize(80, 24);
    session.wait_for_screen("private pages hidden");
    preserve_git(&f, &baseline.0, &baseline.1, &baseline.2);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn pr_metadata_actions_boundary_keyboard_raw_stderr_capture_exhaustion_cannot_produce_source() {
    capture(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_raw_stderr_capture_exhaustion_cannot_produce_source() {
    capture(true);
}
fn capture(mouse: bool) {
    let f = fixture();
    let p = current_parent();
    let baseline = git_snapshot(&f);
    let mut s = startup(&p);
    s.extend(bound(identity(), &p));
    s.extend(bound(identity(), &p));
    let mut load = values::catalogue(PrMetadataCommand::AddLabel, &p, vec![]);
    load.truncate(5);
    load[4].stdout.clear();
    load[4].status = 1;
    load[4].stderr = vec![b'\x1b'; 128 * 1024 + 1];
    s.extend(load);
    s.extend(bound(identity(), &p));
    assert_eq!(s.len(), 29);
    f.scripted_gh(&s);
    let mut session = Session::launch(&f, 120, 40);
    open_pr(&mut session, mouse);
    open_form(&mut session, mouse, PrMetadataCommand::AddLabel);
    choose(&mut session, mouse, "[F4 Choose]", b"\x1bOS");
    records(&mut session, &f, 29);
    ready_form(&mut session, PrMetadataCommand::AddLabel);
    session.wait_for_screen("budget/cancellation reached");
    session.wait_for_screen("choices unavailable");
    assert!(!session.screen().contains("PR metadata choices · Label"));
    confirm(&mut session, mouse);
    session.wait_for_screen("Choose Preview");
    no_write(&mut session, &f, 29);
    close_form(&mut session, mouse);
    preserve_git(&f, &baseline.0, &baseline.1, &baseline.2);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn pr_metadata_actions_boundary_keyboard_complete_opposite_review_set_drift_refuses_before_claim() {
    drift(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_complete_opposite_review_set_drift_refuses_before_claim() {
    drift(true);
}
fn drift(mouse: bool) {
    let f = fixture();
    let command = PrMetadataCommand::RemoveReviewer;
    let p = current_parent();
    let baseline = git_snapshot(&f);
    let mut s = first_steps(command, &p);
    s.extend(bound(identity(), &p));
    let mut concurrent = p.clone();
    concurrent["requested_teams"] = json!([
        values::team(75, "current-team"),
        values::team(77, "concurrent-team")
    ]);
    let mut prep = values::preparation_from(command, &concurrent);
    prep.truncate(8);
    s.extend(prep);
    s.extend(bound(identity(), &p));
    assert_eq!(s.len(), 44);
    f.scripted_gh(&s);
    let mut session = Session::launch(&f, 120, 40);
    prepare(&mut session, &f, mouse, command);
    confirm(&mut session, mouse);
    records(&mut session, &f, 44);
    session.wait_for_screen("PR metadata intent refused");
    session.wait_for_screen("changed");
    no_write(&mut session, &f, 44);
    session.resize(80, 24);
    session.wait_for_screen("changed");
    close_form(&mut session, mouse);
    preserve_git(&f, &baseline.0, &baseline.1, &baseline.2);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn pr_metadata_actions_boundary_keyboard_confirmed_cleanup_failure_restart_views_exact_original_without_choices_or_resend(
) {
    confirmed_restart(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_confirmed_cleanup_failure_restart_views_exact_original_without_choices_or_resend(
) {
    confirmed_restart(true);
}
fn confirmed_restart(mouse: bool) {
    let f = fixture();
    let command = PrMetadataCommand::AddLabel;
    let p = current_parent();
    let changed = after(command);
    let baseline = git_snapshot(&f);
    let mut s = first_steps(command, &p);
    s.extend(publication(command, &p, &changed));
    s.extend(startup(&changed));
    s.extend(bound(identity(), &changed));
    s.extend(bound(identity(), &changed));
    s.extend(bound(identity(), &changed));
    s.extend(bound(identity(), &changed));
    assert_eq!(s.len(), 97);
    f.scripted_gh(&s);
    let mut first = Session::launch(&f, 120, 40);
    prepare(&mut first, &f, mouse, command);
    Connection::open(cache_path(&f)).unwrap().execute_batch("CREATE TRIGGER metadata_keep_original BEFORE DELETE ON drafts BEGIN SELECT RAISE(ABORT,'fixture retained original cleanup'); END;").unwrap();
    confirm(&mut first, mouse);
    records(&mut first, &f, 71);
    first.wait_for_screen("local draft retained");
    first.wait_for_screen("[View original]");
    assert_receipt(&f, command, &changed);
    let durable = Cache::open(&cache_path(&f))
        .unwrap()
        .draft(&scope(), "example/repo/pull/7/composer/metadata-add-label")
        .unwrap()
        .unwrap();
    let saved: Value = serde_json::from_str(&durable).unwrap();
    assert!(saved["publication"]["attempt_id"].as_i64().unwrap() > 0);
    close_form(&mut first, mouse);
    first.quit_and_assert_restored_terminal(mouse);
    let mut second = Session::launch(&f, 80, 24);
    open_pr(&mut second, mouse);
    menu(&mut second, mouse, title(command));
    records(&mut second, &f, 93);
    second.wait_for_screen("Recorded state: Confirmed");
    second.wait_for_screen("Original PR metadata publication record");
    assert!(!second.screen().contains("[F4 Choose]"));
    if mouse {
        second.click_visible_action("[View original]");
    } else {
        second.focus_visible_form_action("[View original]");
        second.send_key(b"\r");
    }
    records(&mut second, &f, 97);
    second.wait_for_screen("Recorded state: Confirmed");
    second.wait_for_screen("nothing resent or acknowledged");
    assert_eq!(mutation_count(&records(&mut second, &f, 97)), 1);
    assert_receipt(&f, command, &changed);
    assert_eq!(
        Cache::open(&cache_path(&f))
            .unwrap()
            .draft(&scope(), "example/repo/pull/7/composer/metadata-add-label")
            .unwrap()
            .unwrap(),
        durable
    );
    close_form(&mut second, mouse);
    preserve_git(&f, &baseline.0, &baseline.1, &baseline.2);
    second.quit_and_assert_restored_terminal(mouse);
}
