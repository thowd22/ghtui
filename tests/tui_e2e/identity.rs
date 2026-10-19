//! Actual keyboard/mouse input journeys for local identities and shared semantic meaning.
use crate::support::{Fixture, Session};
use std::os::unix::ffi::OsStrExt;
#[path = "../git_support/mod.rs"]
mod git_object_fixture;
use std::os::unix::fs::PermissionsExt;
#[path = "actor_avatar_exact_v1.rs"]
mod exact_avatar_v1;
#[path = "git_actor_labels_v1.rs"]
mod git_actor_labels_v1;

fn select_graph_subject(session: &mut Session, mouse: bool, subject: &str) {
    session.wait_for_cursorless_condition(
        |frame| frame.contents().contains(subject) && !frame.contents().contains("[Esc Cancel]"),
        "Actual original graph subject in a completed frame",
        std::time::Duration::from_secs(8),
    );
    if mouse {
        session.click_visible_action(subject);
        return;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        session.poll_output(std::time::Duration::from_millis(1));
        if let Some((column, row)) = session.locate(subject) {
            if session
                .completed_cursorless_screen()
                .is_some_and(|frame| frame.cell(row, column).is_some_and(|cell| cell.inverse()))
            {
                session.send_key(b"\r");
                return;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Original subject never keyboard-selected: {subject}\n{}",
            session.screen()
        );
        session.send_key(b"\x1b[B");
        session.poll_output(std::time::Duration::from_millis(50));
    }
}

fn fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.git(&["config", "user.name", "First Author"]);
    fixture.git(&["config", "user.email", "first@example.invalid"]);
    fixture.commit_file("readme", b"root text\n", "界 e\u{301} Root saved version");
    fixture.git(&["config", "user.name", "Current Committer"]);
    fixture.git(&["config", "user.email", "committer@example.invalid"]);
    std::fs::write(fixture.repo().join("readme"), b"new text\n").unwrap();
    fixture.git(&["add", "readme"]);
    fixture.git(&[
        "commit",
        "--author=Different Author <author@example.invalid>",
        "-m",
        "New saved version",
    ]);
    std::fs::write(fixture.repo().join("readme"), b"local work to preserve\n").unwrap();
    fixture
}

fn actor_journey(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    session.wait_for_screen("author Different Author");
    session.wait_for_screen("committer Current Committer");
    git_actor_labels_v1::choose_key_bar(&mut session, mouse, "[a Author]");
    session.wait_for_screen("Actor profile — Git author");
    session.wait_for_screen("[DA]");
    session.wait_for_screen("author@example.invalid");
    session.wait_for_screen("GitHub account association: unavailable");
    git_actor_labels_v1::choose_key_bar(&mut session, mouse, "[c Committer]");
    session.wait_for_screen("Actor profile — Git committer");
    session.wait_for_screen("[CC]");
    session.wait_for_screen("committer@example.invalid");
    session.resize(80, 24);
    if mouse {
        session.click_visible_action("[g Graph]");
    } else {
        session.send_key(b"g");
    }
    let original = fixture.git(&["rev-parse", "HEAD^"]);
    session.open_saved_graph_subject(mouse, "Root saved version", original.trim());
    session.wait_for_screen("author First Author");
    git_actor_labels_v1::choose_key_bar(&mut session, mouse, "[a Author]");
    session.wait_for_screen("Actor profile — Git author");
    session.wait_for_screen("first@example.invalid");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.repo().join("readme")).unwrap(),
        b"local work to preserve\n"
    );
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "Local actor inspection never needs GitHub or an image request"
    );
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actor_identity_keyboard_local_author_committer_profile_and_saved_revision() {
    actor_journey(false);
}
#[test]
fn actor_identity_mouse_local_author_committer_profile_and_saved_revision() {
    actor_journey(true);
}

fn semantic_fixture() -> Fixture {
    let fixture = fixture();
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file(
        "feature.txt",
        b"separate history\n",
        "Feature saved version",
    );
    fixture.git(&["switch", "main"]);
    fixture.commit_file("main.txt", b"main history\n", "Main saved version");
    fixture.git(&["merge", "--no-ff", "feature", "-m", "Join two histories"]);
    fixture
}

fn inspect_graph_and_changed_file(session: &mut Session, mouse: bool, fixture: &Fixture) {
    if mouse {
        session.click_visible_action("[g Graph]");
    } else {
        session.send_key(b"g");
    }
    session.wait_for_screen("loaded (limit 10000)");
    for (subject, reference) in [
        ("Feature saved version", "feature"),
        ("Main saved version", "HEAD^1"),
        ("Join two histories", "HEAD"),
    ] {
        let revision = fixture.git(&["rev-parse", reference]);
        session.open_saved_graph_subject(mouse, subject, revision.trim());
    }
    session.wait_for_screen("This merge joins 2 histories.");
    if mouse {
        session.click_visible_action("[w Working]");
    } else {
        session.send_key(b"w");
    }
    session.wait_for_screen("│readme ");
    session.wait_for_screen("-new text");
    session.wait_for_screen("+local work to preserve");
    session.wait_for_screen("Repository loaded • read-only exploration");
}

fn semantic_journey(mouse: bool) {
    let fixture = semantic_fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let parents = fixture.git(&["show", "--format=%P", "--no-patch", "HEAD"]);
    assert_eq!(parents.split_whitespace().count(), 2);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    inspect_graph_and_changed_file(&mut session, mouse, &fixture);
    git_actor_labels_v1::choose_key_bar_or_command(
        &mut session,
        mouse,
        "[l Legend]",
        "Inspect Git relationship legend",
    );
    session.wait_for_screen("Git graph legend");
    session.wait_for_screen("never permanent branch identities");
    session.wait_for_screen("+ added");
    session.wait_for_screen("! conflict");
    session.wait_for_screen("✓ passed");
    session.wait_for_screen("Repository loaded • read-only exploration");
    // Settings leaves a full Repository key bar first; Commands still opens it by mouse.
    git_actor_labels_v1::choose_key_bar_or_command(
        &mut session,
        mouse,
        "[s Settings]",
        "Open settings",
    );
    if mouse {
        session.click_visible_action("[Theme: Catppuccin Mocha]");
    } else {
        session.send_key(b"\t\t\r");
    }
    session.wait_for_screen("[Theme: Tokyo Night]");
    if mouse {
        for (current, next) in [
            ("[Theme: Tokyo Night]", "[Theme: Gruvbox Dark]"),
            ("[Theme: Gruvbox Dark]", "[Theme: 16 colors]"),
            ("[Theme: 16 colors]", "[Theme: No color]"),
        ] {
            session.click_visible_action(current);
            session.wait_for_screen(next);
        }
        session.click_visible_action("[Save settings]");
    } else {
        session.send_key(b"\r\r\r");
        session.wait_for_screen("[Theme: No color]");
        session.send_key(b"\t\r");
    }
    session.wait_for_screen("Settings saved");
    session.resize(80, 24);
    inspect_graph_and_changed_file(&mut session, mouse, &fixture);
    git_actor_labels_v1::choose_key_bar_or_command(
        &mut session,
        mouse,
        "[l Legend]",
        "Inspect Git relationship legend",
    );
    session.wait_for_screen("+ added");
    session.wait_for_screen("! conflict");
    session.wait_for_screen("✓ passed");
    session.wait_for_screen("Repository loaded • read-only exploration");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        std::fs::read(fixture.repo().join("readme")).unwrap(),
        b"local work to preserve\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    let config =
        ghtui::config::Config::load(&fixture.root.join("config/ghtui/config.toml")).unwrap();
    assert_eq!(config.theme, ghtui::config::ThemeName::NoColor);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn semantic_colors_keyboard_legend_theme_and_monochrome_meaning() {
    semantic_journey(false);
}
#[test]
fn semantic_colors_mouse_legend_theme_and_monochrome_meaning() {
    semantic_journey(true);
}

fn immutable_overrides_journey(mouse: bool) {
    let fixture = fixture();
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let root = fixture.git(&["rev-parse", "HEAD^"]).trim().to_owned();
    let original_blob = fixture.git(&["rev-parse", "HEAD:readme"]).trim().to_owned();
    let replacement_file = fixture.root.join("replacement-content");
    std::fs::write(
        &replacement_file,
        b"replacement bytes are not this snapshot\n",
    )
    .unwrap();
    let replacement_blob = fixture
        .git(&["hash-object", "-w", replacement_file.to_str().unwrap()])
        .trim()
        .to_owned();
    let root_tree = fixture
        .git(&["rev-parse", &format!("{root}^{{tree}}")])
        .trim()
        .to_owned();
    let replacement_commit = fixture
        .git(&["commit-tree", &root_tree, "-m", "Replacement root history"])
        .trim()
        .to_owned();
    fixture.git(&["replace", &original_blob, &replacement_blob]);
    fixture.git(&["replace", &head, &replacement_commit]);
    let graft_file = fixture.repo().join(".git/info/grafts");
    let graft_bytes = format!("{head}\n").into_bytes();
    std::fs::write(&graft_file, &graft_bytes).unwrap();
    // Prove ordinary Git really presents the altered bytes and ancestry. The TUI must not.
    assert_eq!(
        fixture.git(&["cat-file", "blob", &original_blob]),
        "replacement bytes are not this snapshot\n"
    );
    assert!(fixture
        .git(&["log", "--max-count=1", "--format=%P", "HEAD"])
        .trim()
        .is_empty());
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let refs = fixture.git(&["show-ref"]);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    session.wait_for_screen("Original snapshots • replacements: 2 • legacy grafts: present");
    if mouse {
        session.click_visible_action("[g Graph]");
    } else {
        session.send_key(b"g");
    }
    session.wait_for_screen("loaded (limit 10000)");
    session.wait_for_screen("New saved version");
    session.wait_for_screen("Root saved version");
    assert!(!session.screen().contains("Replacement root history"));
    select_graph_subject(&mut session, mouse, "New saved version");
    session.wait_for_screen("This saved version follows parent");
    if mouse {
        session.click_visible_action("[t Tree]");
    } else {
        session.send_key(b"t");
    }
    session.wait_for_screen("Saved file tree — browsing never checks out");
    if mouse {
        session.click_visible_action("readme");
    } else {
        session.send_key(b"\x1b[B\r");
    }
    session.wait_for_screen("readme at saved version");
    session.wait_for_screen("new text");
    assert!(!session.screen().contains("replacement bytes"));
    session.resize(80, 24);
    session.wait_for_screen("Original snapshots • replacements: 2 • legacy grafts: present");
    session.wait_for_screen("committer Current Committer");
    assert!(
        session.locate("committer Current Committer").unwrap().0 < 10,
        "The actual narrow redraw stacks actors instead of preserving the old wide right column"
    );
    session.wait_for_screen("new text");
    // At 80 columns the key bar paints Local operations as its compact `m Git ops`.
    session.choose_compact(mouse, "[m Local operations]", "[m Git ops]");
    session.wait_for_screen("Local operations • explicit preview and confirmation");
    if mouse {
        session.click_visible_action("Commit staged changes");
        session.close_overlay_by_mouse("Local operations • explicit preview and confirmation");
    } else {
        session.send_key(b"\x1b[B\x1b[B\x1b[B\x1b[B\r");
        session.close_overlay_by_key("Local operations • explicit preview and confirmation");
    }
    // A streamed narrow redraw can clear the title before its old menu body.
    // Verify the actual disabled-action body is gone as well, not just the title.
    session.wait_for_overlay_close("Commit staged changes");
    let guard = "History writes guarded: replacement/graft overrides active.";
    session.wait_for_screen(guard);
    assert!(
        !session.screen().contains("Local operations •"),
        "The disabled-action result is verified outside the pre-existing menu labels"
    );
    assert!(!session.screen().contains("Commit staged changes"));
    assert!(session.screen().contains("new text"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(std::fs::read(&graft_file).unwrap(), graft_bytes);
    assert_eq!(
        std::fs::read(fixture.repo().join("readme")).unwrap(),
        b"local work to preserve\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn immutable_overrides_keyboard_original_tree_graph_and_guarded_history() {
    immutable_overrides_journey(false);
}

#[test]
fn immutable_overrides_mouse_original_tree_graph_and_guarded_history() {
    immutable_overrides_journey(true);
}

fn raw_ref_override_journey(mouse: bool) {
    use std::os::unix::ffi::OsStringExt;
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("readme", b"original root bytes\n", "Original root");
    let root = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file("readme", b"original head bytes\n", "Original head");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["update-ref", "refs/heads/na\u{fffd}", &root]);
    let raw_branch = b"refs/heads/na\xff";
    let base = std::ffi::OsString::from_vec(b"refs/custom\xff/".to_vec());
    let mut packed = format!("# pack-refs with: peeled fully-peeled sorted\n{root} ").into_bytes();
    packed.extend_from_slice(base.as_bytes());
    packed.extend_from_slice(head.as_bytes());
    packed.push(b'\n');
    packed.extend_from_slice(head.as_bytes());
    packed.push(b' ');
    packed.extend_from_slice(raw_branch);
    packed.push(b'\n');
    let packed_path = fixture.repo().join(".git/packed-refs");
    std::fs::write(&packed_path, &packed).unwrap();
    let mut symbolic = b"ref: ".to_vec();
    symbolic.extend_from_slice(raw_branch);
    symbolic.push(b'\n');
    std::fs::write(fixture.repo().join(".git/HEAD"), &symbolic).unwrap();
    let effective = std::process::Command::new("git")
        .args(["show", "HEAD:readme"])
        .env("GIT_REPLACE_REF_BASE", &base)
        .current_dir(fixture.repo())
        .output()
        .unwrap();
    assert!(effective.status.success());
    assert_eq!(
        effective.stdout, b"original root bytes\n",
        "The active raw override really changes ordinary Git reads"
    );
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut session = Session::launch_binary_os_env(
        std::path::Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        120,
        40,
        &[(
            std::ffi::OsStr::new("GIT_REPLACE_REF_BASE"),
            base.as_os_str(),
        )],
    );
    session.wait_for_screen("Recent commits");
    session.wait_for_screen("Original snapshots • replacements: 1 • legacy grafts: absent");
    if mouse {
        session.click_visible_action("[g Graph]");
    } else {
        session.send_key(b"g");
    }
    session.wait_for_screen("loaded (limit 10000)");
    session.wait_for_screen("Original head");
    session.wait_for_screen("Original root");
    session.wait_for_screen(r"na\xff");
    select_graph_subject(&mut session, mouse, "Original head");
    session.wait_for_screen("This saved version follows parent");
    if mouse {
        session.click_visible_action("[t Tree]");
    } else {
        session.send_key(b"t");
    }
    session.wait_for_screen("Saved file tree — browsing never checks out");
    if mouse {
        session.click_visible_action("readme");
    } else {
        session.send_key(b"\x1b[B\r");
    }
    session.wait_for_screen("original head bytes");
    assert!(!session.screen().contains("original root bytes"));
    session.resize(80, 24);
    session.wait_for_screen("Original snapshots • replacements: 1 • legacy grafts: absent");
    // At 80 columns the key bar paints Local operations as its compact `m Git ops`.
    session.choose_compact(mouse, "[m Local operations]", "[m Git ops]");
    session.wait_for_screen("Local operations • explicit preview and confirmation");
    if mouse {
        session.click_visible_action("Commit staged changes");
        session.close_overlay_by_mouse("Local operations • explicit preview and confirmation");
    } else {
        session.send_key(b"\x1b[B\x1b[B\x1b[B\x1b[B\r");
        session.close_overlay_by_key("Local operations • explicit preview and confirmation");
    }
    session.wait_for_screen("History writes guarded: replacement/graft overrides active.");
    assert_eq!(std::fs::read(&packed_path).unwrap(), packed);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/HEAD")).unwrap(),
        symbolic
    );
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    assert_eq!(
        fixture.git(&["rev-parse", "refs/heads/na\u{fffd}"]).trim(),
        root
    );
    assert_eq!(
        std::fs::read(fixture.repo().join("readme")).unwrap(),
        b"original head bytes\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn immutable_overrides_keyboard_raw_ref_and_non_utf8_override_scope_never_alias_unicode() {
    raw_ref_override_journey(false);
}
#[test]
fn immutable_overrides_mouse_raw_ref_and_non_utf8_override_scope_never_alias_unicode() {
    raw_ref_override_journey(true);
}

fn raw_path_journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    let paths = [
        (
            ghtui::git::RawPath::new(b"na\xff".to_vec()),
            "invalid byte target\n",
        ),
        (
            ghtui::git::RawPath::new("na\u{fffd}".as_bytes().to_vec()),
            "genuine Unicode target\n",
        ),
        (
            ghtui::git::RawPath::new(br"na\xff".to_vec()),
            "literal escape target\n",
        ),
    ];
    for (path, body) in &paths {
        git_object_fixture::index_blob(&fixture.repo(), &path.0, body.as_bytes());
    }
    fixture.git(&["commit", "-m", "Distinct path names"]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = std::fs::read(fixture.repo().join(".git/index")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    for (path, body, downs) in [
        // Returning to the tree reselects the file opened last, so each next
        // (lower) row is one Down away.
        (&paths[2].0, paths[2].1.trim(), 1),
        (&paths[1].0, paths[1].1.trim(), 1),
        (&paths[0].0, paths[0].1.trim(), 1),
    ] {
        if mouse {
            session.click_visible_action("[t Tree]");
        } else {
            session.send_key(b"t");
        }
        session.wait_for_screen("Saved file tree — browsing never checks out");
        let label = path.display();
        if mouse {
            session.click_visible_action(&label);
        } else {
            for _ in 0..downs {
                session.send_key(b"\x1b[B");
            }
            session.send_key(b"\r");
        }
        session.wait_for_screen(&format!("{label} at saved version"));
        session.wait_for_screen(body);
        for (_, other) in &paths {
            if other.trim() != body {
                assert!(!session.screen().contains(other.trim()));
            }
        }
    }
    session.resize(80, 24);
    session.wait_for_screen("committer Fixture Author");
    session.wait_for_screen("invalid byte target");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        std::fs::read(fixture.repo().join(".git/index")).unwrap(),
        index
    );
    for (path, body) in &paths {
        let mut object = head.trim().as_bytes().to_vec();
        object.push(b':');
        object.extend_from_slice(&path.0);
        assert_eq!(
            git_object_fixture::git(
                &fixture.repo(),
                [
                    std::ffi::OsStr::new("show"),
                    std::ffi::OsStr::from_bytes(&object)
                ]
            )
            .stdout,
            body.as_bytes()
        );
    }
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn git_provider_keyboard_distinct_raw_path_labels_open_exact_original_bytes() {
    raw_path_journey(false);
}
#[test]
fn git_provider_mouse_distinct_raw_path_labels_open_exact_original_bytes() {
    raw_path_journey(true);
}

/// Alice's PR author line opens with her filled two-cell `AL` initials chip in her stable identity
/// hue, separated from `@alice` by one theme-background cell; the bracketed no-color form is absent.
fn alice_chip(frame: &vt100::Screen) -> bool {
    use ghtui::identity::{ActorIdentity, ActorKind, InitialsChip};
    let alice = ActorIdentity {
        host: Some("github.com".into()),
        login: Some("alice".into()),
        name: String::new(),
        email: None,
        avatar_url: None,
        kind: ActorKind::User,
        git_role: None,
        host_associated: true,
    };
    let hex = ghtui::theme::CATPPUCCIN_MOCHA.lanes()[InitialsChip::slot(&alice)];
    let hue = vt100::Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8);
    let contents = frame.contents();
    let mut seen = 0;
    for (row, line) in contents.lines().enumerate() {
        let Some(at) = line.find("@alice • ") else {
            continue;
        };
        // Two chip cells and exactly one blank cell before the login.
        let Some(chip) = (line[..at].chars().count() as u16).checked_sub(3) else {
            return false;
        };
        let filled = "AL".chars().enumerate().all(|(offset, letter)| {
            frame
                .cell(row as u16, chip + offset as u16)
                .is_some_and(|cell| {
                    cell.contents() == letter.to_string() && cell.bgcolor() == hue && cell.bold()
                })
        });
        let gap = frame
            .cell(row as u16, chip + 2)
            .is_some_and(|cell| cell.bgcolor() != hue);
        if !filled || !gap {
            return false;
        }
        seen += 1;
    }
    seen > 0 && !contents.contains("[AL]")
}

fn wait_alice_chip(session: &mut Session) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        session.poll_output(std::time::Duration::from_millis(1));
        if session.completed_cells_screen().is_some_and(alice_chip) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Alice's initials chip never painted\n{}",
            session.screen()
        );
    }
}

fn github_avatar_journey(mouse: bool, fail_image: bool, cancel_asset: bool) {
    use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
    use serde_json::json;
    let fixture = Fixture::new();
    let mut steps = crate::support::empty_inbox_steps("octocat");
    let pr = json!({"node_id":"PR_AVATAR","number":1,"title":"Avatar preview change","body":"Image identity remains visible","repository_url":"https://api.github.com/repos/org/avatar","html_url":"https://github.com/org/avatar/pull/1","user":{"login":"alice","type":"User","avatar_url":"https://avatars.githubusercontent.com/u/1?v=4"},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z"});
    for step in steps.iter_mut().skip(4) {
        step.stdout = serde_json::to_vec(
            &json!({"total_count":1,"incomplete_results":false,"items":[pr.clone()]}),
        )
        .unwrap();
    }
    fixture.scripted_gh(&steps);
    let pixels = RgbaImage::from_fn(2, 2, |_, y| {
        if y == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    let mut encoded = std::io::Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(pixels)
        .write_to(&mut encoded, ImageFormat::Png)
        .unwrap();
    let image = fixture.root.join("known-avatar.png");
    std::fs::write(&image, encoded.into_inner()).unwrap();
    let release = fixture.root.join("avatar-release");
    let journal = fixture.root.join("avatar-argv");
    let complete = fixture.root.join("avatar-complete");
    let asset_pid = fixture.root.join("avatar-pid");
    let curl = fixture.root.join("bin/curl");
    let effect = if fail_image {
        "printf 'fixture image unavailable\\n' >&2\nexit 22\n".into()
    } else {
        format!("cat '{}'\n", image.display())
    };
    let script=format!("#!/bin/sh\nprintf '%s' \"$$\" > '{}'\nprintf '%s\\n' \"$@\" >> '{}'\ntries=0\nwhile [ ! -e '{}' ] && [ \"$tries\" -lt 500 ]; do /bin/sleep 0.01; tries=$((tries + 1)); done\nprintf complete > '{}'\n{}",asset_pid.display(),journal.display(),release.display(),complete.display(),effect);
    crate::fixture_shell_publication_v1::install(&curl, script.as_bytes()).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    if mouse {
        session.click_visible_action("[r Refresh]");
    } else {
        session.send_key(b"r");
    }
    session.wait_for_screen("Avatar preview change");
    session.wait_for_screen("@alice");
    wait_alice_chip(&mut session);
    session.wait_for_command_count(&fixture, 7);
    if mouse {
        session.click_visible_action("[c Created by me]");
    } else {
        session.send_key(b"c");
    }
    session.wait_for_screen("Queue: Created by me");
    session.wait_for_screen("@alice");
    if cancel_asset {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        let pid = loop {
            if let Ok(value) = std::fs::read_to_string(&asset_pid) {
                if let Ok(pid) = value.parse::<i32>() {
                    break pid;
                }
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Image worker must start before quitting"
            );
            std::thread::yield_now();
        };
        assert!(pid > 0 && pid != std::process::id() as i32);
        assert!(!complete.exists());
        session.quit_and_assert_restored_terminal(mouse);
        assert_eq!(
            unsafe { libc::kill(pid, 0) },
            -1,
            "No isolated image child survives TUI exit"
        );
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ESRCH)
        );
        assert!(!complete.exists());
        assert!(!fixture.root.join("config/ghtui/avatars").exists());
        let gh_journal = session.inspect_command_journal(&fixture);
        assert_eq!(gh_journal.lines().count(), 7);
        assert!(gh_journal
            .lines()
            .all(
                |line| serde_json::from_str::<serde_json::Value>(line).unwrap()["matched"] == true
            ));
        return;
    }
    std::fs::write(&release, b"release now").unwrap();
    if !fail_image {
        if let Err(error) = session.try_wait_for("▀", std::time::Duration::from_secs(3)) {
            let records =
                std::fs::read_dir(fixture.root.join("config/ghtui/avatars")).map(|entries| {
                    entries
                        .flatten()
                        .map(|entry| std::fs::read_to_string(entry.path()).unwrap_or_default())
                        .collect::<Vec<_>>()
                });
            panic!(
                "{error}; asset completed={}, argv={:?}, cached={records:?}",
                complete.exists(),
                std::fs::read_to_string(&journal)
            );
        }
        assert!(String::from_utf8_lossy(&session.transcript).contains("38;2;255;0;0"));
        assert!(String::from_utf8_lossy(&session.transcript).contains("48;2;0;0;255"));
    } else {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !complete.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "Actual isolated image fixture must finish"
            );
            std::thread::yield_now();
        }
        wait_alice_chip(&mut session);
    }
    session.resize(80, 24);
    if mouse {
        session.click_visible_action("[a All]");
    } else {
        session.send_key(b"a");
    }
    session.wait_for_screen("Queue: All");
    session.wait_for_screen("@alice");
    let assets = std::fs::read_to_string(journal).unwrap();
    assert_eq!(
        assets.lines().filter(|line| *line == "--url").count(),
        1,
        "Queue changes share one real asset request"
    );
    assert!(assets.contains("--proto\n=https\n--max-redirs\n0"));
    let gh_journal = session.inspect_command_journal(&fixture);
    assert_eq!(gh_journal.lines().count(), 7);
    assert!(gh_journal
        .lines()
        .all(|line| serde_json::from_str::<serde_json::Value>(line).unwrap()["matched"] == true));
    if !fail_image {
        let cache = fixture.root.join("config/ghtui/avatars");
        let records = std::fs::read_dir(cache)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(records.len(), 1);
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(records[0].path()).unwrap()).unwrap();
        assert_eq!(saved["image"]["pixels"][0], json!([255, 0, 0, 255]));
        assert_eq!(saved["image"]["pixels"][2], json!([0, 0, 255, 255]));
    }
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn actor_identity_keyboard_slow_real_png_emission_and_cached_pixels() {
    github_avatar_journey(false, false, false);
}
#[test]
fn actor_identity_mouse_slow_real_png_emission_and_cached_pixels() {
    github_avatar_journey(true, false, false);
}
#[test]
fn actor_identity_keyboard_failed_avatar_keeps_username_navigation() {
    github_avatar_journey(false, true, false);
}
#[test]
fn actor_identity_mouse_failed_avatar_keeps_username_navigation() {
    github_avatar_journey(true, true, false);
}

#[test]
fn actor_identity_keyboard_quit_cancels_and_reaps_image_child() {
    github_avatar_journey(false, false, true);
}
#[test]
fn actor_identity_mouse_quit_cancels_and_reaps_image_child() {
    github_avatar_journey(true, false, true);
}
