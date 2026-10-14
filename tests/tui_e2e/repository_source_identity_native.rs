//! Registered compiled-binary keyboard/mouse source-identity journeys.
//! This child borrows the genuine fixture/Trace2/palette helpers rather than
//! copying handlers. Each journey checks both real SHA-1 and SHA-256 sources.
use super::*;
use std::{collections::BTreeMap, path::PathBuf};

const SOURCE_TITLE: &str = "Repository source identity • read-only";
const SOURCE_HEADING: &str = "Repository source identity • original prepared read";
const SOURCE_CONTROL: &str = "[Source identity]";
const SOURCE_KEY: &[u8] = b"I"; // Repository-only source explanation binding.

#[test]
fn repository_source_identity_native_keyboard_saved_checkout_mutable_and_no_io() {
    source_journey(false);
}

#[test]
fn repository_source_identity_native_mouse_saved_checkout_mutable_and_no_io() {
    source_journey(true);
}

fn source_fixture(object_format: &str) -> LocalFixture {
    let fixture = Fixture::new();
    fixture.scripted_gh(&[]);
    fixture.git(&[
        "init",
        "--initial-branch=main",
        &format!("--object-format={object_format}"),
    ]);
    fixture.git(&["config", "user.name", "Source Integrator"]);
    fixture.git(&["config", "user.email", "integrator@example.invalid"]);
    fs::create_dir_all(fixture.repo().join("nested")).unwrap();
    for (path, body, message, author) in [
        (
            "nested/日本語.txt",
            "saved navigation beacon α\nunchanged\n".as_bytes(),
            "Original navigation ancestry",
            "Original Source Writer <original@example.invalid>",
        ),
        (
            "staged.txt",
            b"saved stage baseline\n".as_slice(),
            "Later navigation version",
            "Later Source Writer <later@example.invalid>",
        ),
    ] {
        fs::write(fixture.repo().join(path), body).unwrap();
        fixture.git(&["add", "--", path]);
        fixture.git(&[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--author",
            author,
            "-m",
            message,
        ]);
    }
    let original = fixture.git(&["rev-parse", "HEAD^1"]).trim().to_owned();
    fixture.git(&["tag", "navigation-original", &original]);
    fs::write(
        fixture.repo().join("nested/日本語.txt"),
        "working navigation needle β\nunchanged\n".as_bytes(),
    )
    .unwrap();
    fs::write(
        fixture.repo().join("staged.txt"),
        b"staged navigation bytes\n",
    )
    .unwrap();
    fixture.git(&["add", "--", "staged.txt"]);
    fs::write(
        fixture.repo().join("scratch界.txt"),
        "untouched scratch content 🦀\n".as_bytes(),
    )
    .unwrap();
    let before = state(&fixture);
    assert_eq!(
        original.len(),
        if object_format == "sha256" { 64 } else { 40 }
    );
    assert_ne!(original, before.head.trim());
    assert!(before.status.contains("日本語.txt") && before.status.contains("staged.txt"));
    LocalFixture {
        fixture,
        original,
        before,
    }
}

/// Native oracle for exact persisted effects, not a fictitious SQL-read counter.
/// Opening/scroll has no SQL path in Root's separately audited prepared handler;
/// here ALL real config/cache files (including any SQLite/WAL/SHM) must retain
/// exact bytes, and an absent database must remain absent.
fn persisted_files(fixture: &Fixture) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Vec<u8>>) {
        if !path.exists() {
            return;
        }
        let metadata = fs::symlink_metadata(path).unwrap();
        assert!(
            !metadata.file_type().is_symlink(),
            "Fixture storage must not follow symlinks"
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), result);
            }
        } else {
            assert!(metadata.is_file() && metadata.len() <= 2 * 1024 * 1024);
            assert!(result.len() < 100, "Bounded fixture storage inventory");
            result.insert(
                path.strip_prefix(root).unwrap().to_owned(),
                fs::read(path).unwrap(),
            );
        }
    }
    let mut result = BTreeMap::new();
    for name in ["config", "cache"] {
        visit(&fixture.root, &fixture.root.join(name), &mut result);
    }
    result
}

fn source_body(session: &mut Session) -> String {
    body_ready(
        session,
        SOURCE_TITLE,
        &["[PgUp Previous]", "[PgDn More]", "[Esc Close]"],
    )
}

fn changed_body(session: &mut Session, previous: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(10));
        if let Some(body) = session.visible_overlay_body(SOURCE_TITLE) {
            if body.contains("[PgUp Previous]")
                && body.contains("[PgDn More]")
                && body.contains("[Esc Close]")
                && body != previous
            {
                return body;
            }
        }
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "Source scroll produced no actual complete changed body\n{}",
                session.screen()
            );
        }
    }
}

fn reset_detail(session: &mut Session, mouse: bool) {
    for _ in 0..32 {
        let body = source_body(session);
        if body.contains(SOURCE_HEADING) {
            return;
        }
        choose(session, mouse, "[PgUp Previous]", b"\x1b[5~");
        changed_body(session, &body);
    }
    session.save_artifacts();
    panic!(
        "Source detail could not return to the actual first row\n{}",
        session.screen()
    );
}

fn find_detail(session: &mut Session, mouse: bool, expected: &str) {
    reset_detail(session, mouse);
    let expected = expected.split_whitespace().collect::<Vec<_>>().join(" ");
    let deadline = Instant::now() + Duration::from_secs(8);
    for _ in 0..32 {
        let body = source_body(session);
        if body
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .contains(&expected)
        {
            return;
        }
        if Instant::now() >= deadline {
            break;
        }
        choose(session, mouse, "[PgDn More]", b"\x1b[6~");
        changed_body(session, &body);
    }
    session.save_artifacts();
    panic!(
        "Genuine source detail missing {expected:?}\n{}",
        session.screen()
    );
}

fn detail_wheel_parity(session: &mut Session, mouse: bool) {
    reset_detail(session, mouse);
    let original = source_body(session);
    if mouse {
        let (column, row) = session.locate("[PgDn More]").unwrap();
        session.wheel(column + 1, row.saturating_sub(2), true);
    } else {
        session.send_key(b"\x1b[B");
    }
    let moved = changed_body(session, &original);
    if mouse {
        let (column, row) = session.locate("[PgDn More]").unwrap();
        session.wheel(column + 1, row.saturating_sub(2), false);
    } else {
        session.send_key(b"\x1b[A");
    }
    changed_body(session, &moved);
    body_ready(session, SOURCE_TITLE, &[SOURCE_HEADING, "[Esc Close]"]);
}

fn close_source(session: &mut Session, mouse: bool) {
    close_help(session, mouse, SOURCE_TITLE);
    if let Err(error) = session.try_wait_for_condition(
        |screen| !screen.contains(SOURCE_HEADING) && !screen.contains("[PgDn More]"),
        "Source body and paging controls cleared before underlying input",
        Duration::from_secs(8),
    ) {
        session.save_artifacts();
        panic!("{error}");
    }
    session.assert_focused_view("Repository");
}

fn assert_pure_source(
    local: &LocalFixture,
    session: &Session,
    reads: usize,
    saved: &BTreeMap<PathBuf, Vec<u8>>,
) {
    assert_eq!(
        starts(&local.fixture),
        reads,
        "Prepared source opening/detail/resize must run zero new Git commands"
    );
    assert_eq!(
        persisted_files(&local.fixture),
        *saved,
        "No persisted/SQLite sidecar creation or byte changes"
    );
    check_effects(local, session);
}

fn source_journey(mouse: bool) {
    for object_format in ["sha1", "sha256"] {
        let local = source_fixture(object_format);
        let trace_path = local.fixture.root.join("navigation-child-git-trace.jsonl");
        let mut session = Session::launch_binary(
            Path::new(env!("CARGO_BIN_EXE_ghtui")),
            &local.fixture,
            120,
            40,
            &[
                ("GIT_TRACE2_EVENT", trace_path.to_str().unwrap()),
                ("NO_COLOR", "1"),
            ],
        );
        session.wait_for_screen("Recent commits");
        command(&mut session, mouse, "Show Graph");
        session.wait_for_screen("● saved version");
        session.open_saved_graph_subject(mouse, "Original navigation ancestry", &local.original);
        session.wait_for_screen(&format!("Saved: {}", local.original));
        session.wait_for_screen(&format!("HEAD: {}", local.before.head.trim()));
        session.wait_for_screen("Original Source Writer");
        session.wait_for_screen("Source Integrator");
        let selected = session.locate("Original navigation ancestry").unwrap();
        assert!(session.cell_inverted(selected.1, selected.0));
        let background_tab = session.locate("3 Search").unwrap();
        let reads = starts(&local.fixture);
        assert!(reads > 0, "Actual child Git trace must be present");
        let saved = persisted_files(&local.fixture);
        choose(&mut session, mouse, SOURCE_CONTROL, SOURCE_KEY);
        body_ready(&mut session, SOURCE_TITLE, &[SOURCE_HEADING, "[Esc Close]"]);
        if mouse {
            session.click(background_tab.0, background_tab.1);
        } else {
            session.send_key(b"3");
        }
        find_detail(
            &mut session,
            mouse,
            &format!("Checkout HEAD: {}", local.before.head.trim()),
        );
        find_detail(
            &mut session,
            mouse,
            &format!("Viewed saved version: {}", local.original),
        );
        find_detail(
            &mut session,
            mouse,
            &format!(
                "Displayed Git author and committer belong to saved commit: {}",
                local.original
            ),
        );
        find_detail(
            &mut session,
            mouse,
            "Neither role identifies who made current working/index edits.",
        );
        detail_wheel_parity(&mut session, mouse);
        session.resize(80, 24);
        find_detail(
            &mut session,
            mouse,
            &format!("Checkout HEAD: {}", local.before.head.trim()),
        );
        find_detail(
            &mut session,
            mouse,
            &format!("Viewed saved version: {}", local.original),
        );
        // Full64-digit IDs must be readable in detail AND the underlying Saved/
        // HEAD rows, never accepted only as a truncated long-path context line.
        assert_pure_source(&local, &session, reads, &saved);
        close_source(&mut session, mouse);
        session.wait_for_screen(&format!("Saved: {}", local.original));
        session.wait_for_screen(&format!("HEAD: {}", local.before.head.trim()));
        let selected = session.locate("Original navigation ancestry").unwrap();
        assert!(
            session.cell_inverted(selected.1, selected.0),
            "Source detail/resize preserves exact original commit focus"
        );
        assert_pure_source(&local, &session, reads, &saved);
        for (command_label, content, explanation) in [
            (
                "Show Working diff",
                "working navigation needle β",
                "Working: index (next-commit contents) to current working files. Both are mutable",
            ),
            (
                "Show Staged diff",
                "staged navigation bytes",
                "Staged: observed saved HEAD to index (next-commit contents). The index is mutable",
            ),
        ] {
            command(&mut session, mouse, command_label);
            session.wait_for_screen(content);
            session.wait_for_screen(&format!("Saved: {}", local.before.head.trim()));
            let reads = starts(&local.fixture);
            let saved = persisted_files(&local.fixture);
            choose(&mut session, mouse, SOURCE_CONTROL, SOURCE_KEY);
            find_detail(&mut session, mouse, explanation);
            find_detail(
                &mut session,
                mouse,
                &format!("Checkout HEAD: {}", local.before.head.trim()),
            );
            find_detail(
                &mut session,
                mouse,
                "Source IDs describe that prepared read only.",
            );
            assert_pure_source(&local, &session, reads, &saved);
            close_source(&mut session, mouse);
            session.wait_for_screen(content);
            assert_pure_source(&local, &session, reads, &saved);
        }
        check_effects(&local, &session);
        session.quit_and_assert_restored_terminal(mouse);
    }
}
