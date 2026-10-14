//! Additional native current/eligible/cancel/recovery boundaries. Expected
//! source facts and queued argv are fixed independently of the observed output.
use super::*;

fn source_steps(command: PrMetadataCommand, p: &Value, pages: Vec<Vec<Value>>) -> Vec<GhStep> {
    let mut source = values::catalogue(command, p, vec![]);
    let template = source[4].clone();
    let mut out = source.drain(..4).collect::<Vec<_>>();
    for (page, rows) in pages.into_iter().enumerate() {
        let mut step = template.clone();
        step.args[1] = step.args[1].replace("&page=1", &format!("&page={}", page + 1));
        step.stdout = serde_json::to_vec(&rows).unwrap();
        out.push(step);
    }
    out.extend(source.into_iter().skip(1));
    out.extend(bound(identity(), p));
    out
}
fn source_setup(
    command: PrMetadataCommand,
    p: &Value,
    source: Vec<GhStep>,
) -> (Fixture, Vec<GhStep>) {
    let f = fixture();
    let mut s = startup(p);
    s.extend(bound(identity(), p));
    s.extend(bound(identity(), p));
    s.extend(source);
    let _ = command;
    (f, s)
}
fn effects(session: &mut Session, f: &Fixture, count: usize) {
    assert_eq!(mutation_count(&records(session, f, count)), 0);
    assert_no_publication(f);
}
fn snapshot(f: &Fixture) -> (String, Vec<u8>, String) {
    (
        f.git(&["rev-parse", "HEAD"]),
        fs::read(f.repo().join(".git/index")).unwrap(),
        f.git(&["status", "--porcelain=v2", "-z"]),
    )
}
fn filter(session: &mut Session, mouse: bool, text: &str) {
    if mouse {
        session.click_visible_action("Filter");
    }
    session.send_key(b"\x01");
    session.paste(text);
    session.wait_for_screen(text);
}

#[test]
fn pr_metadata_actions_boundary_keyboard_partial_known_label_and_nullable_disabled_member() {
    partial(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_partial_known_label_and_nullable_disabled_member() {
    partial(true);
}
fn partial(mouse: bool) {
    let command = PrMetadataCommand::AddLabel;
    let p = current_parent();
    let mut rows = vec![
        json!({"id":null,"node_id":null,"name":"unverified"}),
        values::candidate(command),
    ];
    rows.extend((82..1080).map(|id| values::label(id, &format!("topic/{id}"))));
    assert_eq!(rows.len(), 1000);
    let (f, s) = source_setup(
        command,
        &p,
        source_steps(
            command,
            &p,
            rows.chunks(100).map(<[Value]>::to_vec).collect(),
        ),
    );
    assert_eq!(s.len(), 41);
    f.scripted_gh(&s);
    let git = snapshot(&f);
    let mut session = Session::launch(&f, 120, 40);
    open_pr(&mut session, mouse);
    open_form(&mut session, mouse, command);
    choose(&mut session, mouse, "[F4 Choose]", b"\x1bOS");
    records(&mut session, &f, 41);
    session.wait_for_screen("Partial catalogue: observed choices only");
    session.wait_for_screen("unverified");
    choose(&mut session, mouse, "[Enter Choose]", b"\r");
    session.wait_for_screen("disabled");
    effects(&mut session, &f, 41);
    filter(&mut session, mouse, "new 🐛");
    if mouse {
        session.click_visible_action("[Enter Choose]");
    } else {
        session.focus_visible_form_action("[Enter Choose]");
        session.send_key(b"\r");
    }
    session.wait_for_overlay_close("PR metadata choices · Label");
    ready_form(&mut session, command);
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("Exact effect: Add label");
    session.wait_for_screen("L_81");
    effects(&mut session, &f, 41);
    close_form(&mut session, mouse);
    preserve_git(&f, &git.0, &git.1, &git.2);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn pr_metadata_actions_boundary_keyboard_known_empty_vs_forbidden_source_never_infers_clear() {
    empty(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_known_empty_vs_forbidden_source_never_infers_clear() {
    empty(true);
}
fn empty(mouse: bool) {
    let command = PrMetadataCommand::AddLabel;
    let p = current_parent();
    let first = source_steps(command, &p, vec![vec![]]);
    let (f, mut s) = source_setup(command, &p, first);
    let mut forbidden = source_steps(command, &p, vec![vec![]]);
    forbidden[4].status = 1;
    forbidden[4].stdout.clear();
    forbidden[4].stderr = b"gh: Forbidden catalogue (HTTP 403)\n".to_vec();
    s.extend(forbidden);
    assert_eq!(s.len(), 42);
    f.scripted_gh(&s);
    let git = snapshot(&f);
    let mut session = Session::launch(&f, 120, 40);
    open_pr(&mut session, mouse);
    open_form(&mut session, mouse, command);
    choose(&mut session, mouse, "[F4 Choose]", b"\x1bOS");
    records(&mut session, &f, 32);
    session.wait_for_screen("Complete catalogue");
    session.wait_for_screen("No observed matching choices");
    choose(&mut session, mouse, "[Enter Choose]", b"\r");
    session.wait_for_screen("no observed matching member");
    effects(&mut session, &f, 32);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close("PR metadata choices · Label");
    ready_form(&mut session, command);
    choose(&mut session, mouse, "[F4 Choose]", b"\x1bOS");
    records(&mut session, &f, 42);
    session.wait_for_screen("Unavailable");
    session.wait_for_screen("Forbidden");
    assert!(!session.screen().contains("Complete catalogue"));
    choose(&mut session, mouse, "[Enter Choose]", b"\r");
    effects(&mut session, &f, 42);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close("PR metadata choices · Label");
    close_form(&mut session, mouse);
    preserve_git(&f, &git.0, &git.1, &git.2);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn pr_metadata_actions_boundary_keyboard_missing_current_array_is_unknown_not_empty() {
    unknown(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_missing_current_array_is_unknown_not_empty() {
    unknown(true);
}
fn unknown(mouse: bool) {
    let command = PrMetadataCommand::RemoveLabel;
    let p = current_parent();
    let mut absent = p.clone();
    absent["labels"] = Value::Null;
    let (f, s) = source_setup(command, &p, source(command, &absent));
    assert_eq!(s.len(), 32);
    f.scripted_gh(&s);
    let git = snapshot(&f);
    let mut session = Session::launch(&f, 120, 40);
    open_pr(&mut session, mouse);
    open_form(&mut session, mouse, command);
    choose(&mut session, mouse, "[F4 Choose]", b"\x1bOS");
    records(&mut session, &f, 32);
    session.wait_for_screen("missing/null/malformed");
    assert!(!session.screen().contains("Current PR metadata members"));
    confirm(&mut session, mouse);
    session.wait_for_screen("Choose Preview");
    effects(&mut session, &f, 32);
    close_form(&mut session, mouse);
    preserve_git(&f, &git.0, &git.1, &git.2);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn pr_metadata_actions_boundary_keyboard_prewrite_403_same_uid_hides_old_private_snapshot() {
    prewrite_access(false);
}
#[test]
fn pr_metadata_actions_boundary_mouse_prewrite_403_same_uid_hides_old_private_snapshot() {
    prewrite_access(true);
}
fn prewrite_access(mouse: bool) {
    let command = PrMetadataCommand::AddLabel;
    let p = current_parent();
    let (f, mut s) = source_setup(command, &p, source(command, &p));
    s.extend(bound(identity(), &p));
    s.push(values::get("user", json!({"id":42,"login":"requester"})));
    let mut denied = values::get("repos/example/repo", Value::Null);
    denied.status = 1;
    denied.stdout.clear();
    denied.stderr = b"gh: Forbidden repository (HTTP 403)\n".to_vec();
    s.push(denied);
    s.extend(bound(identity(), &p));
    assert_eq!(s.len(), 38);
    f.scripted_gh(&s);
    let git = snapshot(&f);
    let mut session = Session::launch(&f, 120, 40);
    open_pr(&mut session, mouse);
    open_form(&mut session, mouse, command);
    choose_value(&mut session, &f, mouse, command, 32);
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_screen("Exact effect: Add label");
    confirm(&mut session, mouse);
    records(&mut session, &f, 38);
    session.wait_for_screen("private pages hidden");
    assert!(
        !session.screen().contains("Your repository permissions:")
            && !session.screen().contains("Selected value:")
            && !session.screen().contains("Unchanged title")
    );
    effects(&mut session, &f, 38);
    preserve_git(&f, &git.0, &git.1, &git.2);
    session.quit_and_assert_restored_terminal(mouse);
}
