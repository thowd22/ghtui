//! Long-root real-binary display observations and independently guarded Git effects.
use crate::{
    local, local_management,
    support::{self, Fixture, Session},
};
use ghtui::{
    git::{display_path_bytes, RawPath},
    local_actions::LocalCommand,
};
use std::{
    collections::BTreeMap,
    ffi::{OsStr, OsString},
    fs,
    os::unix::ffi::{OsStrExt, OsStringExt},
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

const OBSERVED: &str = "Full observed target • read-only";
const INPUT: &str = "Full current worktree input • read-only";
const SOURCE: &str = "Repository source identity • read-only";

fn long_fixture() -> Fixture {
    let mut fixture = Fixture::new();
    let initial = fixture.root.clone();
    let mut root = initial.join("genuine-long-workspace-界e\u{301}");
    for n in 0..7 {
        root = root.join(format!("segment-{n:02}-original-observed-local-directory"));
    }
    fs::create_dir_all(&root).unwrap();
    for name in ["home", "config", "cache", "bin", "repo", "gh-fixture"] {
        fs::rename(initial.join(name), root.join(name)).unwrap();
    }
    fixture.root = root;
    assert!(fixture.repo().as_os_str().as_bytes().len() > 330);
    fixture
}

fn git_path(fixture: &Fixture, path: &Path, branch: &str) {
    let output = Command::new("git")
        .arg("-C")
        .arg(fixture.repo())
        .args(["worktree", "add", "--"])
        .arg(path)
        .arg(branch)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(crate) fn activate(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if matches!(label, "[F4 Choose]" | "[F2 Full input]") {
        // Editable forms end with a visible editor caret, not Hide. Their
        // existing canonical phase/footer/caret observer owns this control.
        session.wait_for_screen("LOCAL INTENT • Preview first");
        session.wait_for_screen("[Keep draft / Close]");
        if mouse {
            session.click_owned_form_field(
                "LOCAL INTENT • Preview first",
                "Exact linked worktree directory",
            );
            session.click_visible_action(label);
        } else {
            session.send_key(key);
        }
        return;
    }
    let before = session.completed_cursorless_end();
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if let Some(position) = session
            .completed_cursorless_screen()
            .and_then(|frame| support::locate_screen(frame, label))
        {
            if mouse {
                session.click(position.0, position.1);
            } else {
                session.send_key(key);
            }
            break;
        }
        assert!(
            Instant::now() < deadline,
            "No completed control {label:?}\n{}",
            session.screen()
        );
    }
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cursorless_end() > before {
            let acknowledged = session.completed_cursorless_screen().is_some_and(|frame| {
                let text = frame.contents();
                match label {
                    "[Esc Close]" => !text.contains(SOURCE) && support::main_footer_visible(frame),
                    "[Esc Back]" => {
                        !text.contains(OBSERVED)
                            && !text.contains(INPUT)
                            && text.contains("Choose path • local inventory")
                            && text.contains("[Enter Choose]")
                    }
                    "[Enter Choose]" => false, // An editable form owns its actual caret.
                    _ => true,
                }
            });
            if acknowledged {
                return;
            }
        }
        if matches!(label, "[Esc Back]" | "[Enter Choose]") {
            let screen = session.screen();
            if screen.contains("LOCAL INTENT • Preview first")
                && screen.contains("[Keep draft / Close]")
                && !screen.contains(INPUT)
                && !screen.contains("Choose path • local inventory")
                && !screen.contains(OBSERVED)
                && !screen.contains("Entered intent text, NOT a verified")
            {
                return;
            }
        }
        assert!(
            Instant::now() < deadline,
            "No completed frame after {label:?}\n{}",
            session.screen()
        );
    }
}

fn assert_preview_target(session: &mut Session, mouse: bool, target: &Path) {
    let expected = format!(
        "Exact worktree directory: {}",
        RawPath::new(target.as_os_str().as_bytes().to_vec()).display()
    );
    for _ in 0..32 {
        let deadline = Instant::now() + Duration::from_secs(8);
        let (body, position) = loop {
            session.poll_output(Duration::from_millis(1));
            if let Some(frame) = session.completed_cursorless_screen() {
                if let Some(body) = support::overlay_body(frame, "Remove linked worktree") {
                    if body.contains("READ-ONLY PREVIEW")
                        && body.contains("[Confirm action]")
                        && body.contains("[Keep draft / Close]")
                    {
                        break (
                            body,
                            support::locate_screen(frame, "READ-ONLY PREVIEW").unwrap(),
                        );
                    }
                }
            }
            assert!(
                Instant::now() < deadline,
                "No completed exact guarded Preview\n{}",
                session.screen()
            );
        };
        if logical_body(&body).contains(&expected) {
            return;
        }
        let before = session.completed_cursorless_end();
        if mouse {
            session.wheel(position.0, position.1 + 2, true);
        } else {
            session.send_key(b"\x1b[B\x1b[B\x1b[B");
        }
        while session.completed_cursorless_end() <= before {
            session.poll_output(Duration::from_millis(1));
            assert!(Instant::now() < deadline, "No completed Preview scroll");
        }
    }
    panic!(
        "Full independently verified raw target never visible: {expected:?}\n{}",
        session.screen()
    );
}

fn completed_body(session: &mut Session, title: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if let Some(body) = session
            .completed_cursorless_screen()
            .and_then(|frame| support::overlay_body(frame, title))
        {
            if body.contains("[PgUp Previous]")
                && body.contains("[PgDn More]")
                && (body.contains("[Esc Back]") || body.contains("[Esc Close]"))
            {
                return body;
            }
        }
        assert!(
            Instant::now() < deadline,
            "No completed owned detail body {title:?}\n{}",
            session.screen()
        );
    }
}

fn logical_body(body: &str) -> String {
    // Enclosing borders are measured by the canonical cell helper. Its first
    // inner column is the renderer's one-cell padding. Keep internal spaces,
    // backslashes, replacement characters and every other actual value byte.
    body.lines()
        .take(body.lines().count().saturating_sub(2))
        .map(|line| line.strip_prefix(' ').unwrap_or(line).trim_end())
        .collect()
}

pub(crate) fn verify_full_input(session: &mut Session, mouse: bool, text: &str) {
    activate(session, mouse, "[F2 Full input]", b"\x1bOQ");
    assert_complete_detail(
        session,
        mouse,
        INPUT,
        &format!("Full entered text: {}", display_path_bytes(text.as_bytes())),
    );
    activate(session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_screen("LOCAL INTENT • Preview first");
}

pub(crate) fn assert_complete_detail(
    session: &mut Session,
    mouse: bool,
    title: &str,
    expected: &str,
) {
    if title == SOURCE {
        activate(session, mouse, "[PgUp Previous]", b"\x1b[H");
    } else {
        activate(session, mouse, "[Home First]", b"\x1b[H");
    }
    if mouse && title == SOURCE {
        // At most 32 display pages under the bounded metadata budget. This
        // fixture has fewer than four; reset using actual visible controls.
        activate(session, true, "[PgUp Previous]", b"\x1b[H");
        activate(session, true, "[PgUp Previous]", b"\x1b[H");
    }
    // Scroll in three-row increments so each long scalar must be visible whole
    // in one independently completed viewport, rather than splicing aliases.
    for _ in 0..32 {
        let body = completed_body(session, title);
        if logical_body(&body).contains(expected) {
            return;
        }
        let before = session.completed_cursorless_end();
        if mouse {
            let frame = session.completed_cursorless_screen().unwrap();
            let (x, y) = support::locate_screen(frame, "[PgDn More]").unwrap();
            // Actual mouse wheel over the owned body, not a direct action.
            session.wheel(x, y.saturating_sub(2), true);
        } else {
            session.send_key(b"\x1b[B\x1b[B\x1b[B");
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        while session.completed_cursorless_end() <= before {
            session.poll_output(Duration::from_millis(1));
            assert!(Instant::now() < deadline, "No completed details scroll");
        }
    }
    panic!(
        "Full escaped observation never visible: {expected:?}\n{}",
        session.screen()
    );
}

fn sqlite_image(fixture: &Fixture) -> BTreeMap<String, Vec<String>> {
    let connection = rusqlite::Connection::open_with_flags(
        fixture.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let tables: Vec<(String, String)> = connection
        .prepare("SELECT name, sql FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let mut image = BTreeMap::new();
    for (name, sql) in tables {
        let mut statement = connection
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let count = statement.column_count();
        let rows = statement
            .query_map([], |row| {
                (0..count)
                    .map(|n| row.get_ref(n).map(|v| format!("{v:?}")))
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map(|v| v.join(" | "))
            })
            .unwrap();
        let mut values: Vec<_> = rows.map(Result::unwrap).collect();
        values.sort();
        values.insert(0, sql);
        image.insert(name, values);
    }
    assert!(!image.is_empty(), "Read the genuine application's schema");
    image
}

fn files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, current: &Path, result: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(current).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if entry.file_type().unwrap().is_dir() {
                walk(root, &path, result);
            } else {
                result.insert(
                    display_path_bytes(path.strip_prefix(root).unwrap().as_os_str().as_bytes()),
                    fs::read(path).unwrap(),
                );
            }
        }
    }
    let mut result = BTreeMap::new();
    walk(root, root, &mut result);
    result
}

fn launch(fixture: &Fixture, env: &[(&OsStr, &OsStr)]) -> Session {
    let mut session = Session::launch_binary_os_env(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        fixture,
        120,
        40,
        env,
    );
    session.wait_for_screen("Recent commits");
    session.wait_for_main_frame();
    session
}

fn source_journey(mouse: bool) {
    let fixture = long_fixture();
    fixture.init_git();
    fixture.commit_file("readme", b"original root bytes\n", "Original root");
    let root = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file("readme", b"original head bytes\n", "Original head");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["update-ref", "refs/heads/na\u{fffd}", &root]);
    let raw_branch = b"refs/heads/na\xff";
    let base = OsString::from_vec(b"refs/custom\xff/".to_vec());
    let mut packed = format!("# pack-refs with: peeled fully-peeled sorted\n{root} ").into_bytes();
    packed.extend_from_slice(base.as_bytes());
    packed.extend_from_slice(head.as_bytes());
    packed.push(b'\n');
    packed.extend_from_slice(head.as_bytes());
    packed.push(b' ');
    packed.extend_from_slice(raw_branch);
    packed.push(b'\n');
    fs::write(fixture.repo().join(".git/packed-refs"), &packed).unwrap();
    let mut symbolic = b"ref: ".to_vec();
    symbolic.extend_from_slice(raw_branch);
    symbolic.push(b'\n');
    fs::write(fixture.repo().join(".git/HEAD"), &symbolic).unwrap();
    let ordinary = Command::new("git")
        .args(["show", "HEAD:readme"])
        .env("GIT_REPLACE_REF_BASE", &base)
        .current_dir(fixture.repo())
        .output()
        .unwrap();
    assert!(ordinary.status.success());
    assert_eq!(ordinary.stdout, b"original root bytes\n");
    let git_before = files(&fixture.repo().join(".git"));
    let trace = fixture.root.join("app-git-trace.json");
    let mut session = launch(
        &fixture,
        &[
            (OsStr::new("GIT_REPLACE_REF_BASE"), base.as_os_str()),
            (OsStr::new("GIT_TRACE2_EVENT"), trace.as_os_str()),
            (
                OsStr::new("NO_COLOR"),
                OsStr::new(if mouse { "1" } else { "" }),
            ),
        ],
    );
    session.wait_for_screen(r"na\xff");
    session.resize(80, 24);
    session.wait_for_screen(r"na\xff");
    let frame = session.completed_cursorless_screen().unwrap();
    assert!(support::locate_screen(frame, r"na\xff").is_some());
    assert!(support::locate_screen(frame, "I Source").is_some());
    let sql = sqlite_image(&fixture);
    let config = files(&fixture.root.join("config/ghtui"));
    let before_trace = fs::read(&trace).unwrap();
    assert!(!before_trace.is_empty());
    activate(&mut session, mouse, "I Source", b"I");
    assert_complete_detail(
        &mut session,
        mouse,
        SOURCE,
        &display_path_bytes(fixture.repo().as_os_str().as_bytes()),
    );
    assert_complete_detail(&mut session, mouse, SOURCE, &head);
    assert_complete_detail(&mut session, mouse, SOURCE, r"Observed branch name: na\xff");
    activate(&mut session, mouse, "[PgUp Previous]", b"\x1b[5~");
    session.resize(120, 40);
    assert_complete_detail(
        &mut session,
        mouse,
        SOURCE,
        &display_path_bytes(fixture.repo().as_os_str().as_bytes()),
    );
    activate(&mut session, mouse, "[Esc Close]", b"\x1b");
    session.wait_for_main_frame();
    assert_eq!(
        fs::read(&trace).unwrap(),
        before_trace,
        "Source details issue no Git subprocess"
    );
    assert_eq!(sqlite_image(&fixture), sql);
    assert_eq!(files(&fixture.root.join("config/ghtui")), config);
    assert_eq!(files(&fixture.repo().join(".git")), git_before);
    assert_eq!(
        fs::read(fixture.repo().join("readme")).unwrap(),
        b"original head bytes\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

fn worktree_journey(mouse: bool) {
    let fixture = long_fixture();
    fixture.init_git();
    fixture.commit_file("saved", b"source preserved\n", "Original checkout");
    fixture.commit_file("staged", b"saved index bytes\n", "Original staged source");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    assert!(!fixture.git(&["show-ref"]).is_empty());
    let target = fixture.root.join(support::exact_worktree_component());
    let unicode = fixture.root.join("linked-�");
    let literal = fixture.root.join("linked-\\xff");
    for (path, branch) in [
        (&target, "linked"),
        (&unicode, "unicode"),
        (&literal, "literal"),
    ] {
        fixture.git(&["branch", branch]);
        git_path(&fixture, path, branch);
    }
    fs::write(unicode.join("saved"), b"Unicode sibling protected\n").unwrap();
    fs::write(literal.join("saved"), b"Literal escape sibling protected\n").unwrap();
    fs::write(fixture.repo().join("staged"), b"private staged bytes\n").unwrap();
    fixture.git(&["add", "--", "staged"]);
    fs::write(fixture.repo().join("saved"), b"private working bytes\n").unwrap();
    fs::write(
        fixture.repo().join("untracked"),
        b"private untracked bytes\n",
    )
    .unwrap();
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let staged = fixture.git(&["diff", "--cached", "--binary"]);
    let refs = fixture.git(&["show-ref"]);
    let trace = fixture.root.join("app-git-trace.json");
    let mut session = launch(
        &fixture,
        &[
            (OsStr::new("GIT_TRACE2_EVENT"), trace.as_os_str()),
            (
                OsStr::new("NO_COLOR"),
                OsStr::new(if mouse { "1" } else { "" }),
            ),
        ],
    );
    session.resize(80, 24);
    local_management::operation(&mut session, mouse, LocalCommand::RemoveWorktree);
    activate(&mut session, mouse, "[F4 Choose]", b"\x1bOS");
    session.wait_for_screen("Choose path • local inventory");
    // The default real inventory row is the protected current/main workspace.
    activate(&mut session, mouse, "[F2 Details]", b"\x1bOQ");
    assert_complete_detail(
        &mut session,
        mouse,
        OBSERVED,
        &format!(
            "Observed worktree directory: {}",
            display_path_bytes(fixture.repo().as_os_str().as_bytes())
        ),
    );
    assert_complete_detail(
        &mut session,
        mouse,
        OBSERVED,
        "Unavailable choice: The current/main workspace cannot be removed here.",
    );
    activate(&mut session, mouse, "[Esc Back]", b"\x1b");
    if mouse {
        session.click_visible_action("[Enter Choose]");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("current/main workspace cannot be removed");
    assert!(session.screen().contains("Choose path • local inventory"));
    session.send_key(b"\x15");
    session.paste("no-existing-target-under-this-long-root");
    session.wait_for_screen("No matching targets. Scope was not broadened.");
    activate(&mut session, mouse, "[F2 Details]", b"\x1bOQ");
    session.wait_for_screen("No selected observed target; nothing chosen or verified.");
    assert!(!session.screen().contains(OBSERVED));
    assert!(fixture.repo().exists() && target.exists());
    session.send_key(b"\x15");
    session.paste(support::exact_worktree_label());
    session.wait_for_screen(&format!("Filter: {}", support::exact_worktree_label()));
    let sql = sqlite_image(&fixture);
    let config = files(&fixture.root.join("config/ghtui"));
    let trace_before = fs::read(&trace).unwrap();
    activate(&mut session, mouse, "[F2 Details]", b"\x1bOQ");
    assert_complete_detail(
        &mut session,
        mouse,
        OBSERVED,
        &format!(
            "Observed worktree directory: {}",
            RawPath::new(target.as_os_str().as_bytes().to_vec()).display()
        ),
    );
    assert_complete_detail(
        &mut session,
        mouse,
        OBSERVED,
        &format!("Observed worktree HEAD: {}", head.trim()),
    );
    activate(&mut session, mouse, "[PgDn More]", b"\x1b[6~");
    activate(&mut session, mouse, "[PgUp Previous]", b"\x1b[5~");
    activate(&mut session, mouse, "[End Last]", b"\x1b[F");
    activate(&mut session, mouse, "[Home First]", b"\x1b[H");
    session.resize(120, 40);
    assert_complete_detail(
        &mut session,
        mouse,
        OBSERVED,
        &format!(
            "Observed worktree directory: {}",
            RawPath::new(target.as_os_str().as_bytes().to_vec()).display()
        ),
    );
    session.send_key(b"\r"); // Details have no Choose/Confirm authority.
    completed_body(&mut session, OBSERVED);
    activate(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_screen("Choose path • local inventory");
    assert_eq!(fs::read(&trace).unwrap(), trace_before);
    assert_eq!(sqlite_image(&fixture), sql);
    assert_eq!(files(&fixture.root.join("config/ghtui")), config);
    activate(&mut session, mouse, "[Enter Choose]", b"\r");
    session.wait_for_screen("LOCAL INTENT • Preview first");
    let input_sql = sqlite_image(&fixture);
    let input_config = files(&fixture.root.join("config/ghtui"));
    let input_trace = fs::read(&trace).unwrap();
    activate(&mut session, mouse, "[F2 Full input]", b"\x1bOQ");
    let entered = RawPath::new(target.as_os_str().as_bytes().to_vec()).display();
    assert_complete_detail(
        &mut session,
        mouse,
        INPUT,
        &format!(
            "Full entered text: {}",
            display_path_bytes(entered.as_bytes())
        ),
    );
    session.resize(80, 24);
    assert_complete_detail(
        &mut session,
        mouse,
        INPUT,
        &format!(
            "Full entered text: {}",
            display_path_bytes(entered.as_bytes())
        ),
    );
    activate(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_screen("LOCAL INTENT • Preview first");
    assert_eq!(sqlite_image(&fixture), input_sql);
    assert_eq!(files(&fixture.root.join("config/ghtui")), input_config);
    assert_eq!(fs::read(&trace).unwrap(), input_trace);
    assert!(target.exists());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    // Observed details grant no live authority. Change the actual linked HEAD
    // after choosing and require fresh Preview to reject the old observation.
    fs::write(target.join("saved"), b"external linked commit\n").unwrap();
    for args in [
        &["add", "--", "saved"][..],
        &["commit", "-m", "External linked source update"][..],
    ] {
        let output = Command::new("git")
            .arg("-C")
            .arg(&target)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Fixture Author")
            .env("GIT_AUTHOR_EMAIL", "fixture@example.invalid")
            .env("GIT_COMMITTER_NAME", "Fixture Author")
            .env("GIT_COMMITTER_EMAIL", "fixture@example.invalid")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let current_refs = fixture.git(&["show-ref"]);
    assert_ne!(current_refs, refs);
    if mouse {
        session.click_visible_action("[Preview]");
    } else {
        session.send_key(b"\x10");
    }
    session.wait_for_screen("Selected linked worktree changed");
    assert!(target.exists());
    assert_eq!(
        fs::read(target.join("saved")).unwrap(),
        b"external linked commit\n"
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    activate(&mut session, mouse, "[F4 Choose]", b"\x1bOS");
    session.wait_for_screen("Choose path • local inventory");
    session.send_key(b"\x15");
    session.paste(support::exact_worktree_label());
    session.wait_for_screen(&format!("Filter: {}", support::exact_worktree_label()));
    activate(&mut session, mouse, "[F2 Details]", b"\x1bOQ");
    assert_complete_detail(
        &mut session,
        mouse,
        OBSERVED,
        &format!(
            "Observed worktree directory: {}",
            display_path_bytes(target.as_os_str().as_bytes())
        ),
    );
    activate(&mut session, mouse, "[Esc Back]", b"\x1b");
    activate(&mut session, mouse, "[Enter Choose]", b"\r");
    session.wait_for_screen("LOCAL INTENT • Preview first");
    // Only the existing freshly verified Git Preview authorizes removal.
    local::preview(&mut session, mouse);
    assert_preview_target(&mut session, mouse, &target);
    assert!(target.exists());
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen("Completed • Remove linked worktree");
    assert!(!target.exists());
    assert_eq!(
        fs::read(unicode.join("saved")).unwrap(),
        b"Unicode sibling protected\n"
    );
    assert_eq!(
        fs::read(literal.join("saved")).unwrap(),
        b"Literal escape sibling protected\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("saved")).unwrap(),
        b"private working bytes\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("staged")).unwrap(),
        b"private staged bytes\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("untracked")).unwrap(),
        b"private untracked bytes\n"
    );
    assert_eq!(fixture.git(&["diff", "--cached", "--binary"]), staged);
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["show-ref"]), current_refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn long_local_identity_keyboard_raw_header_source_full_worktree_observation_and_guarded_effect() {
    source_journey(false);
    worktree_journey(false);
}
#[test]
fn long_local_identity_mouse_raw_header_source_full_worktree_observation_and_guarded_effect() {
    source_journey(true);
    worktree_journey(true);
}
