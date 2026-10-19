//! Source-only child of original_saved_navigation_v1. No registration or run.
//! Real Git corruption/repair is confined to explicit temporary loose objects.
use super::*;

fn tree_before_file(native: &Native, session: &mut Session, mouse: bool) {
    session.wait_for_screen("Recent commits");
    choose(session, mouse, "[g Graph]", b"g");
    session.wait_for_screen("loaded (limit 10000)");
    session.open_saved_graph_subject(mouse, "Chosen immutable rename", &native.chosen);
    session.wait_for_screen(&native.chosen);
    choose(session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    select_row(session, mouse, "▸ nested");
    session.wait_for_screen("renamed.txt");
}

fn hold_loose_object(native: &Native, sha: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    assert!(matches!(sha.len(), 40 | 64));
    assert!(sha.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let path = native
        .fixture
        .repo()
        .join(".git/objects")
        .join(&sha[..2])
        .join(&sha[2..]);
    assert!(
        path.is_file(),
        "Fixture must genuinely contain this loose object, not pretend a packed object was removed"
    );
    let held = native.fixture.root.join(format!("held-exact-object-{sha}"));
    fs::rename(&path, &held).unwrap();
    (path, held)
}

fn failure_is_still_owned(native: &Native, session: &mut Session, prior: usize) {
    session.wait_for_screen("Error:");
    assert!(!session
        .screen()
        .contains("File history — chosen saved version"));
    assert!(!session.screen().contains("future excluded source"));
    no_discovery_delta(native, prior);
    let completed = starts(native).len();
    session.resize(80, 24);
    session.wait_for_screen("Error:");
    session.resize(120, 40);
    session.wait_for_screen("Error:");
    assert_eq!(
        starts(native).len(),
        completed,
        "Resize/focus must not silently retry, fetch or retarget a failed immutable read"
    );
    assert!(session.inspect_command_journal(&native.fixture).is_empty());
    assert!(fs::read(native.fixture.root.join("curl-journal"))
        .unwrap()
        .is_empty());
    assert_eq!(sql(&native.fixture), native.sql);
}

#[test]
fn original_saved_boundaries_keyboard_missing_blob_no_head_fallback_explicit_repair() {
    missing_blob(false);
}
#[test]
fn original_saved_boundaries_mouse_missing_blob_no_head_fallback_explicit_repair() {
    missing_blob(true);
}
fn missing_blob(mouse: bool) {
    let native = fixture("sha256");
    let blob = native
        .fixture
        .git(&[
            "rev-parse",
            &format!("{}:nested/renamed.txt", native.chosen),
        ])
        .trim()
        .to_owned();
    let mut session = launch(&native, mouse);
    tree_before_file(&native, &mut session, mouse);
    let (path, held) = hold_loose_object(&native, &blob);
    let prior = starts(&native).len();
    select_row(&mut session, mouse, "· renamed.txt");
    failure_is_still_owned(&native, &mut session, prior);
    assert!(
        !path.exists(),
        "The application must not fetch or manufacture the missing blob"
    );
    assert!(!session.screen().contains("saved nested line 002"));
    fs::rename(&held, &path).unwrap();
    let prior = starts(&native).len();
    select_row(&mut session, mouse, "· renamed.txt");
    session.wait_for_screen("saved nested line 002");
    session.wait_for_screen(&native.chosen);
    no_discovery_delta(&native, prior);
    assert!(!session.screen().contains("future excluded source"));
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn original_saved_boundaries_keyboard_missing_commit_history_cannot_rebase_after_repair() {
    missing_commit(false);
}
#[test]
fn original_saved_boundaries_mouse_missing_commit_history_cannot_rebase_after_repair() {
    missing_commit(true);
}
fn missing_commit(mouse: bool) {
    let native = fixture("sha1");
    let mut session = launch(&native, mouse);
    saved_file(&native, &mut session, mouse);
    let (path, held) = hold_loose_object(&native, &native.chosen);
    let prior = starts(&native).len();
    choose(&mut session, mouse, "[H History]", b"H");
    failure_is_still_owned(&native, &mut session, prior);
    assert!(!path.exists());
    assert!(
        session.screen().contains("saved nested line"),
        "Honest prior immutable file remains; no fake empty or HEAD file may replace it"
    );
    fs::rename(&held, &path).unwrap();
    let prior = starts(&native).len();
    choose(&mut session, mouse, "[H History]", b"H");
    session.wait_for_screen("File history — chosen saved version");
    session.wait_for_screen("Rename: history.txt → nested/renamed.txt");
    session.wait_for_screen(&native.chosen);
    no_discovery_delta(&native, prior);
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn original_saved_boundaries_keyboard_working_path_deleted_saved_blob_still_exact() {
    deleted_working_path(false);
}
#[test]
fn original_saved_boundaries_mouse_working_path_deleted_saved_blob_still_exact() {
    deleted_working_path(true);
}
fn deleted_working_path(mouse: bool) {
    let native = fixture("sha256");
    let mut session = launch(&native, mouse);
    tree_before_file(&native, &mut session, mouse);
    let path = native.fixture.repo().join("nested/renamed.txt");
    let held = native.fixture.root.join("original-user-dirty-file");
    fs::rename(&path, &held).unwrap();
    let prior = starts(&native).len();
    select_row(&mut session, mouse, "· renamed.txt");
    session.wait_for_screen("saved nested line 002");
    session.wait_for_screen(&native.chosen);
    no_discovery_delta(&native, prior);
    assert!(
        !path.exists(),
        "Read-only browsing must not repair/check out the missing working file"
    );
    assert_eq!(fs::read(&held).unwrap(), b"user dirty nested bytes\n");
    assert!(!session.screen().contains("future excluded source"));
    let prior = starts(&native).len();
    choose(&mut session, mouse, "[Y Blame]", b"Y");
    session.wait_for_screen("Blame lines 1–100 of 145");
    provenance_row(&mut session, mouse, "saved nested line 002");
    session.wait_for_screen("Original path: history.txt");
    session.wait_for_screen(&native.responsible);
    no_discovery_delta(&native, prior);
    assert!(!path.exists());
    fs::rename(&held, &path).unwrap();
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn original_saved_boundaries_keyboard_same_working_root_replaced_git_locator_zero_read() {
    git_locator(false);
}
#[test]
fn original_saved_boundaries_mouse_same_working_root_replaced_git_locator_zero_read() {
    git_locator(true);
}
fn git_locator(mouse: bool) {
    let native = fixture("sha1");
    let mut session = launch(&native, mouse);
    saved_file(&native, &mut session, mouse);
    let git_dir = native.fixture.repo().join(".git");
    let held = native.fixture.root.join("held-original-git-directory");
    fs::rename(&git_dir, &held).unwrap();
    let original_inode = std::os::unix::fs::MetadataExt::ino(&fs::metadata(&held).unwrap());
    let replacement = native.fixture.root.join("replacement-object-repository");
    let output = Command::new("git")
        .args(["clone", "--quiet", "--no-hardlinks"])
        .arg(&held)
        .arg(&replacement)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Real original Git clone: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::rename(replacement.join(".git"), &git_dir).unwrap();
    assert_ne!(
        std::os::unix::fs::MetadataExt::ino(&fs::metadata(&git_dir).unwrap()),
        original_inode
    );
    let replacement_state = state(&native.fixture);
    let prior = starts(&native).len();
    choose(&mut session, mouse, "[H History]", b"H");
    session.wait_for_screen("Error:");
    session.wait_for_screen("source/locator changed");
    assert_eq!(
        starts(&native).len(),
        prior,
        "Same root/SHA cannot bypass original .git identity before any child"
    );
    assert!(!session
        .screen()
        .contains("File history — chosen saved version"));
    session.resize(80, 24);
    session.wait_for_screen("Error:");
    session.resize(120, 40);
    session.wait_for_screen("source/locator changed");
    assert_eq!(starts(&native).len(), prior);
    assert_eq!(state(&native.fixture), replacement_state);
    assert_eq!(sql(&native.fixture), native.sql);
    assert!(session.inspect_command_journal(&native.fixture).is_empty());
    assert!(fs::read(native.fixture.root.join("curl-journal"))
        .unwrap()
        .is_empty());
    assert_eq!(
        fs::read(native.fixture.repo().join("nested/renamed.txt")).unwrap(),
        b"user dirty nested bytes\n"
    );
    assert_eq!(
        fs::read(native.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        native.config
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn original_saved_boundaries_keyboard_symlink_not_regular_no_fake_line_attribution() {
    object_type(false);
}
#[test]
fn original_saved_boundaries_mouse_symlink_not_regular_no_fake_line_attribution() {
    object_type(true);
}
fn object_type(mouse: bool) {
    let mut native = fixture("sha1");
    std::os::unix::fs::symlink(
        "nested/renamed.txt",
        native.fixture.repo().join("saved-link"),
    )
    .unwrap();
    native.fixture.git(&["add", "--", "saved-link"]);
    native.fixture.git(&[
        "commit",
        "--only",
        "-qm",
        "Special object saved source",
        "--",
        "saved-link",
    ]);
    native.chosen = native.fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    native.before = state(&native.fixture);
    let mut session = launch(&native, mouse);
    session.wait_for_screen("Recent commits");
    choose(&mut session, mouse, "[g Graph]", b"g");
    session.wait_for_screen("loaded (limit 10000)");
    session.open_saved_graph_subject(mouse, "Special object saved source", &native.chosen);
    choose(&mut session, mouse, "[D Details]", b"D");
    session.wait_for_screen("This saved version follows parent");
    assert!(!session.screen().contains("Reading original local source"));
    choose(&mut session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    select_row(&mut session, mouse, "saved-link");
    session.wait_for_screen("Symbolic link pointing to nested/renamed.txt");
    let prior = starts(&native).len();
    choose(&mut session, mouse, "[Y Blame]", b"Y");
    session.wait_for_screen("symlink/submodule is not a regular text file");
    no_discovery_delta(&native, prior);
    session.resize(80, 24);
    session.wait_for_screen("not a regular text file");
    assert!(!session.screen().contains("Original line:"));
    for (label, key) in [
        ("[G Responsible graph]", b"G".as_slice()),
        ("[D Introduced diff]", b"D".as_slice()),
        ("[F Original file]", b"F".as_slice()),
    ] {
        let prior = starts(&native).len();
        // The genuine Tree mode tab, captured before the explanation overlay opens.
        let underlying_tree = session.wait_for_control("[t Tree]");
        choose(&mut session, mouse, label, key);
        session.wait_for_screen("Select an exact history entry or attributed line");
        assert_eq!(
            starts(&native).len(),
            prior,
            "Disabled target explanation must not read/follow link or invent a line"
        );
        if mouse {
            session.click(underlying_tree.0, underlying_tree.1);
            session.wait_for_screen("Select an exact history entry or attributed line");
            assert_eq!(starts(&native).len(), prior);
        }
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        session.wait_for_overlay_close("Action unavailable • explanation only");
        session.wait_for_screen("not a regular text file");
    }
    assert!(!session.screen().contains("Original immutable file"));
    effects(&native, &session);
    session.quit_and_assert_restored_terminal(mouse);
}
