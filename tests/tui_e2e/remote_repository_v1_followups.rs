//! SOURCE-ONLY, UNREGISTERED/UNRUN. Future child of remote_repository_v1.
//! Every journey uses the real binary, strict actual processes and real SQL/Git.
use super::*;
#[path = "remote_repository_v1_followup_fixtures.rs"]
mod extra;

fn select_root_row(session: &mut Session, mouse: bool, objects: &fx::Objects, name: &str) {
    if mouse {
        session.click_visible_action(name);
    } else {
        session.send_key(b"\x1b[H");
        for _ in 0..extra::row_index(objects, name) {
            session.send_key(b"\x1b[B");
        }
    }
    let prefix = if name == "module" {
        "◇"
    } else if name == "link" {
        "↗"
    } else {
        "·"
    };
    ready(session, &[REMOTE, &format!("> {prefix} {name}")]);
}
fn close_notice(session: &mut Session, mouse: bool, restored: &[&str]) {
    choose(session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(DETAIL);
    ready(session, restored);
}
fn hidden_main(session: &mut Session) {
    session.wait_for_cursorless_condition(
        |frame| {
            let contents = frame.contents();
            super::super::super::support::main_footer_visible(frame)
                && contents.contains("Remote access unavailable")
                && contents.contains("private pages hidden")
                && !contents.contains(REMOTE)
                && !contents.contains("old remote saved content")
                && !contents.contains("PRIVATE_")
        },
        "completed original local frame after actual private blob retirement",
        Duration::from_secs(8),
    );
    assert!(!session.screen().contains(REMOTE));
    assert!(!session.screen().contains("old remote saved content"));
    assert!(!session.screen().contains("PRIVATE_"));
}
fn private_file_steps(objects: &fx::Objects) -> Vec<GhStep> {
    let mut steps = fx::source_steps();
    steps.extend(default_operation(objects));
    steps.extend(directory(objects));
    steps.extend(file(objects));
    assert_eq!(steps.len(), 31);
    steps
}
fn open_private_body(session: &mut Session, fixture: &Fixture, mouse: bool, objects: &fx::Objects) {
    open(session, fixture, mouse);
    choose_nested(session, mouse, objects);
    open_file(session, mouse);
    session.wait_for_cursorless_condition(
        |frame| {
            let contents = frame.contents();
            contents.contains(REMOTE)
                && contents.contains("old remote saved content")
                && contents.contains(&objects.commit)
                && contents.contains("[r Refresh]")
                && !contents.contains("read pending")
        },
        "genuine completed original private file body before failure/cancel",
        Duration::from_secs(8),
    );
    assert_commands(session, fixture, 31);
}
fn refresh_private_body(session: &mut Session, mouse: bool) {
    choose(session, mouse, "[r Refresh]", b"r");
}
fn no_original_rewrite(
    fixture: &Fixture,
    original: &(String, u64, bool),
    saved: &[(String, String)],
) {
    assert_eq!(&extra::source_row(fixture), original);
    assert_eq!(extra::saved_bytes(fixture), saved);
}
fn special_content(mouse: bool) {
    let (fixture, mut objects) = configured_remote();
    extra::extended_objects(&fixture, &mut objects);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = fx::source_steps();
    steps.extend(default_operation(&objects));
    for name in ["binary.bin", "lfs.txt", "link"] {
        steps.extend(extra::blob_operation(&fixture, &objects, name));
        steps.extend(extra::reload_root(&objects));
    }
    assert_eq!(steps.len(), 59);
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open(&mut session, &fixture, mouse);
    let source = extra::source_row(&fixture);
    let saved = extra::saved_bytes(&fixture);
    let mut expected = 17;
    for (name, kind, body) in [
        (
            "binary.bin",
            "Binary bytes (not executed)",
            "Binary file: 4 bytes",
        ),
        (
            "lfs.txt",
            "LFS pointer only (object not downloaded)",
            "Git LFS pointer: 1234567 bytes",
        ),
        (
            "link",
            "Symlink target (not followed)",
            "Symbolic link pointing to nested/日本語.txt",
        ),
    ] {
        select_root_row(&mut session, mouse, &objects, name);
        choose(&mut session, mouse, "[Enter Open]", b"\r");
        ready(&mut session, &[REMOTE, kind, body]);
        expected += 7;
        assert_commands(&session, &fixture, expected);
        assert!(!session.screen().contains("new checked-out content"));
        choose(&mut session, mouse, "[u Up]", b"u");
        ready(
            &mut session,
            &[REMOTE, "Complete immediate directory", "oversized.bin"],
        );
        expected += 7;
        assert_commands(&session, &fixture, expected);
    }
    let journal = session.inspect_command_journal(&fixture);
    for (name, reason) in [
        ("oversized.bin", "no content downloaded"),
        ("module", "never automatically fetched"),
    ] {
        select_root_row(&mut session, mouse, &objects, name);
        choose(&mut session, mouse, "[Enter Open]", b"\r");
        overlay_ready(&mut session, DETAIL, &[reason, "[Esc Back]"]);
        assert_eq!(session.inspect_command_journal(&fixture), journal);
        close_notice(
            &mut session,
            mouse,
            &[REMOTE, "Complete immediate directory"],
        );
    }
    no_original_rewrite(&fixture, &source, &saved);
    close_remote(&mut session, mouse);
    assert_commands(&session, &fixture, 59);
    preserved_remote(&fixture, &objects, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn manual_commit(mouse: bool) {
    let (fixture, objects) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = fx::source_steps();
    steps.extend(default_operation(&objects));
    steps.extend(fx::prefix());
    steps.push(fx::get(
        &format!("repos/example/remote/git/commits/{}", objects.commit),
        json!({"sha":objects.commit,"tree":{"sha":objects.tree}}),
    ));
    steps.push(fx::get(
        &format!("repos/example/remote/git/trees/{}", objects.tree),
        objects.root_json.clone(),
    ));
    steps.extend(fx::suffix(42));
    assert_eq!(steps.len(), 25);
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open(&mut session, &fixture, mouse);
    let source = extra::source_row(&fixture);
    let saved = extra::saved_bytes(&fixture);
    let original_commands = session.inspect_command_journal(&fixture);
    choose(&mut session, mouse, "[C Commit]", b"C");
    overlay_ready(
        &mut session,
        COMMIT,
        &[
            "[Enter Browse]",
            "[Esc Back]",
            "[Ctrl-U Clear]",
            "[Ctrl-A All]",
        ],
    );
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        COMMIT,
        &["[PgUp]", "[PgDn]"],
    );
    if mouse {
        session.click_visible_action("[click or type full saved SHA]");
    }
    session.paste("deadbeef");
    choose(&mut session, mouse, "[Enter Browse]", b"\r");
    overlay_ready(
        &mut session,
        COMMIT,
        &["lowercase saved SHA required", "no request sent"],
    );
    assert_eq!(session.inspect_command_journal(&fixture), original_commands);
    choose(&mut session, mouse, "[Ctrl-U Clear]", b"\x15");
    session.paste(&"a".repeat(40));
    ready(&mut session, &[COMMIT, &"a".repeat(40)]);
    choose(&mut session, mouse, "[Ctrl-A All]", b"\x01");
    session.paste(&objects.commit);
    ready(&mut session, &[COMMIT, &objects.commit]);
    assert!(
        !session.screen().contains(&"a".repeat(40)),
        "Actual selected replacement, not append/refuse"
    );
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_overlay_close(COMMIT);
    ready(&mut session, &[REMOTE, "Complete immediate directory"]);
    assert_eq!(session.inspect_command_journal(&fixture), original_commands);
    no_original_rewrite(&fixture, &source, &saved);
    choose(&mut session, mouse, "[C Commit]", b"C");
    overlay_ready(
        &mut session,
        COMMIT,
        &["[click or type full saved SHA]", "[Enter Browse]"],
    );
    if mouse {
        session.click_visible_action("[click or type full saved SHA]");
    }
    session.paste(&objects.commit);
    ready(&mut session, &[COMMIT, &objects.commit]);
    choose(&mut session, mouse, "[Enter Browse]", b"\r");
    session.wait_for_overlay_close(COMMIT);
    ready(
        &mut session,
        &[REMOTE, "Complete immediate directory", &objects.commit],
    );
    assert_commands(&session, &fixture, 25);
    no_original_rewrite(&fixture, &source, &saved);
    close_remote(&mut session, mouse);
    preserved_remote(&fixture, &objects, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn closed_payload_denial(mouse: bool) {
    let (fixture, objects) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = private_file_steps(&objects);
    let mut denied = file(&objects);
    denied[3].status = 1;
    denied[3].stdout.clear();
    denied[3].stderr = b"gh: PRIVATE_DENIED_TREE_CANARY (HTTP 403)".to_vec();
    steps.extend(denied);
    assert_eq!(steps.len(), 38);
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open_private_body(&mut session, &fixture, mouse, &objects);
    let source = extra::source_row(&fixture);
    let saved = extra::saved_bytes(&fixture);
    refresh_private_body(&mut session, mouse);
    overlay_ready(
        &mut session,
        DETAIL,
        &["private tree/content hidden", "[Esc Back]"],
    );
    session.wait_for_cursorless_condition(
        |frame| {
            let contents = frame.contents();
            contents.contains(DETAIL)
                && contents.contains("private tree/content hidden")
                && contents.contains("[Esc Back]")
                && !contents.contains("old remote saved content")
                && !contents.contains("PRIVATE_DENIED_TREE_CANARY")
        },
        "completed closed403 notice after actual private file body erasure",
        Duration::from_secs(8),
    );
    assert_commands(&session, &fixture, 38);
    assert!(!session.screen().contains("PRIVATE_DENIED_TREE_CANARY"));
    assert!(!session.screen().contains("日本語.txt"));
    close_notice(
        &mut session,
        mouse,
        &["default snapshot unavailable", "no main/HEAD guessed"],
    );
    no_original_rewrite(&fixture, &source, &saved);
    close_remote(&mut session, mouse);
    preserved_remote(&fixture, &objects, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn source_replacement(mouse: bool) {
    let (fixture, objects) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = private_file_steps(&objects);
    steps.extend(fx::prefix()); // SQL epoch refusal BEFORE any private tree child/final proof.
    assert_eq!(steps.len(), 34);
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open_private_body(&mut session, &fixture, mouse, &objects);
    let original = extra::source_row(&fixture);
    let saved = extra::saved_bytes(&fixture);
    assert_eq!(original.1, 1);
    let connection = rusqlite::Connection::open(database(&fixture)).unwrap();
    assert_eq!(connection.execute("UPDATE sync_reservations SET generation=2 WHERE host=?1 AND login='user-id:42' AND source=?2 AND generation=1 AND active=1", rusqlite::params![HOST, original.0]).unwrap(), 1);
    drop(connection);
    refresh_private_body(&mut session, mouse);
    hidden_main(&mut session);
    assert_commands(&session, &fixture, 34);
    assert_eq!(
        extra::source_row(&fixture),
        (original.0, 2, true),
        "Independent edit retained; no rebase/adoption/new reservation"
    );
    assert_eq!(extra::saved_bytes(&fixture), saved);
    preserved_remote(&fixture, &objects, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn recreated_repository(mouse: bool) {
    let (fixture, objects) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = private_file_steps(&objects);
    steps.extend(fx::user(42, "octocat"));
    let mut replacement = fx::repo(json!(true));
    replacement["id"] = json!(999);
    replacement["node_id"] = json!("R_DIFFERENT_SAME_NAME_999");
    steps.push(fx::get("repos/example/remote", replacement));
    steps.extend(fx::user(42, "octocat"));
    assert_eq!(steps.len(), 36);
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open_private_body(&mut session, &fixture, mouse, &objects);
    let source = extra::source_row(&fixture);
    let saved = extra::saved_bytes(&fixture);
    refresh_private_body(&mut session, mouse);
    hidden_main(&mut session);
    assert_commands(&session, &fixture, 36);
    assert!(!session.screen().contains("R_DIFFERENT_SAME_NAME_999"));
    no_original_rewrite(&fixture, &source, &saved);
    preserved_remote(&fixture, &objects, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn raw_capture_loss(mouse: bool) {
    let (fixture, objects) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = private_file_steps(&objects);
    let mut operation = file(&objects);
    operation[3].stderr = b"PRIVATE_RAW_CAPTURE_CANARY ".repeat(50000);
    assert!(operation[3].stderr.len() > 1024 * 1024);
    steps.extend(operation.into_iter().take(4)); // Exhausted pool/output forbids hidden closing children.
    assert_eq!(steps.len(), 35);
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open_private_body(&mut session, &fixture, mouse, &objects);
    let source = extra::source_row(&fixture);
    let saved = extra::saved_bytes(&fixture);
    refresh_private_body(&mut session, mouse);
    hidden_main(&mut session);
    assert_commands(&session, &fixture, 35);
    no_original_rewrite(&fixture, &source, &saved);
    preserved_remote(&fixture, &objects, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}
fn parent_capture_loss(mouse: bool) {
    let (fixture, objects) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = private_file_steps(&objects);
    let mut opening = fx::prefix();
    opening[2].stderr = b"PRIVATE_PARENT_CAPTURE_CANARY ".repeat(50000);
    assert!(opening[2].stderr.len() > 1024 * 1024);
    steps.extend(opening);
    assert_eq!(steps.len(), 34);
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open_private_body(&mut session, &fixture, mouse, &objects);
    let source = extra::source_row(&fixture);
    let saved = extra::saved_bytes(&fixture);
    refresh_private_body(&mut session, mouse);
    hidden_main(&mut session);
    assert_commands(&session, &fixture, 34);
    no_original_rewrite(&fixture, &source, &saved);
    preserved_remote(&fixture, &objects, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn remote_tree_v1_parent_capture_keyboard_exhaustion_no_hidden_principal_or_private_body() {
    parent_capture_loss(false);
}
#[test]
fn remote_tree_v1_parent_capture_mouse_exhaustion_no_hidden_principal_or_private_body() {
    parent_capture_loss(true);
}
fn cancel_live_child(mouse: bool) {
    let (fixture, objects) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = private_file_steps(&objects);
    let mut operation = file(&objects);
    operation[3].delay_ms = 10000;
    operation[3].stderr = b"PRIVATE_LATE_CANCELLED_CANARY".to_vec();
    steps.extend(operation.into_iter().take(4));
    assert_eq!(steps.len(), 35);
    fixture.scripted_gh(&steps);
    fs::write(
        fixture.root.join("gh-fixture/031.journal-prefix-delay-ms"),
        "200",
    )
    .unwrap();
    let mut session = launch(&fixture, mouse);
    open_private_body(&mut session, &fixture, mouse, &objects);
    let source = extra::source_row(&fixture);
    let saved = extra::saved_bytes(&fixture);
    let partial_before = session.observed_partial_command_journal_v1();
    refresh_private_body(&mut session, mouse);
    extra::wait_commands(&mut session, &fixture, 35);
    assert!(
        session.observed_partial_command_journal_v1() > partial_before,
        "Real split journal append was observed before its committing newline"
    );
    let rows = commands(&session, &fixture);
    let child = i32::try_from(rows.last().unwrap()["pid"].as_u64().unwrap()).unwrap();
    assert_eq!(
        unsafe { libc::kill(child, 0) },
        0,
        "Real delayed original child is live BEFORE cancel"
    );
    overlay_ready(
        &mut session,
        "Remote repository • read pending",
        &["[Esc Cancel]", "old private tree/content hidden"],
    );
    session.wait_for_cursorless_condition(
        |frame| {
            let contents = frame.contents();
            contents.contains("Remote repository • read pending")
                && contents.contains("[Esc Cancel]")
                && !contents.contains("old remote saved content")
                && !contents.contains("PRIVATE_LATE_CANCELLED_CANARY")
        },
        "completed pending refresh suppresses actual old file bytes before cancel",
        Duration::from_secs(8),
    );
    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
    session.wait_for_overlay_close("Remote repository • read pending");
    session.wait_for_cursorless_condition(
        |frame| {
            let contents = frame.contents();
            super::super::super::support::main_footer_visible(frame)
                && contents.contains("Recent commits")
                && !contents.contains(REMOTE)
                && !contents.contains("old remote saved content")
                && !contents.contains("PRIVATE_LATE_CANCELLED_CANARY")
        },
        "completed original local frame after explicit remote child cancel",
        Duration::from_secs(8),
    );
    extra::child_reaped(&mut session, child);
    assert_commands(&session, &fixture, 35);
    assert!(!session.screen().contains("PRIVATE_LATE_CANCELLED_CANARY"));
    assert!(!session.screen().contains(REMOTE));
    no_original_rewrite(&fixture, &source, &saved);
    preserved_remote(&fixture, &objects, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}

fn current_uid_loss(mouse: bool, after_capture: bool) {
    let (fixture, objects) = configured_remote();
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut steps = private_file_steps(&objects);
    let count = if after_capture {
        steps.extend(fx::prefix());
        steps.push(fx::blob_step(&objects));
        steps.extend(fx::suffix(43));
        38
    } else {
        steps.extend(fx::user(43, "octocat"));
        33
    };
    assert_eq!(steps.len(), count);
    fixture.scripted_gh(&steps);
    let mut session = launch(&fixture, mouse);
    open_private_body(&mut session, &fixture, mouse, &objects);
    let source = extra::source_row(&fixture);
    let saved = extra::saved_bytes(&fixture);
    refresh_private_body(&mut session, mouse);
    hidden_main(&mut session);
    assert_commands(&session, &fixture, count);
    no_original_rewrite(&fixture, &source, &saved);
    preserved_remote(&fixture, &objects, &config, None);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn remote_tree_v1_followup_keyboard_binary_lfs_symlink_gitlink_and_oversized() {
    special_content(false);
}
#[test]
fn remote_tree_v1_followup_mouse_binary_lfs_symlink_gitlink_and_oversized() {
    special_content(true);
}
#[test]
fn remote_tree_v1_followup_keyboard_manual_invalid_replace_cancel_exact_commit() {
    manual_commit(false);
}
#[test]
fn remote_tree_v1_followup_mouse_manual_invalid_replace_cancel_exact_commit() {
    manual_commit(true);
}
#[test]
fn remote_tree_v1_followup_keyboard_closed403_hides_old_tree_without_default() {
    closed_payload_denial(false);
}
#[test]
fn remote_tree_v1_followup_mouse_closed403_hides_old_tree_without_default() {
    closed_payload_denial(true);
}
#[test]
fn remote_tree_v1_followup_keyboard_original_sql_source_changed_no_payload() {
    source_replacement(false);
}
#[test]
fn remote_tree_v1_followup_mouse_original_sql_source_changed_no_payload() {
    source_replacement(true);
}
#[test]
fn remote_tree_v1_followup_keyboard_recreated_same_name_no_tree_adoption() {
    recreated_repository(false);
}
#[test]
fn remote_tree_v1_followup_mouse_recreated_same_name_no_tree_adoption() {
    recreated_repository(true);
}
#[test]
fn remote_tree_v1_followup_keyboard_raw_capture_exhaustion_no_hidden_probe() {
    raw_capture_loss(false);
}
#[test]
fn remote_tree_v1_followup_mouse_raw_capture_exhaustion_no_hidden_probe() {
    raw_capture_loss(true);
}
#[test]
fn remote_tree_v1_followup_test_framework_command_journal_keyboard_real_delayed_child_cancel_and_reap(
) {
    cancel_live_child(false);
}
#[test]
fn remote_tree_v1_followup_test_framework_command_journal_mouse_real_delayed_child_cancel_and_reap()
{
    cancel_live_child(true);
}

#[test]
fn remote_tree_v1_followup_keyboard_current_uid43_retires_actual_private_blob_without_payload() {
    current_uid_loss(false, false);
}
#[test]
fn remote_tree_v1_followup_mouse_current_uid43_retires_actual_private_blob_without_payload() {
    current_uid_loss(true, false);
}

#[test]
fn remote_tree_v1_followup_keyboard_final_uid43_after_blob_capture_cannot_republish_private_body() {
    current_uid_loss(false, true);
}
#[test]
fn remote_tree_v1_followup_mouse_final_uid43_after_blob_capture_cannot_republish_private_body() {
    current_uid_loss(true, true);
}
