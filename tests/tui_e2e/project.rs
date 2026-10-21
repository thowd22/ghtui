//! Actual local project/subdirectory discovery with unchanged source/index/remotes.
use crate::support::{Fixture, Session};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    time::Duration,
};

// This fixture belongs to the native test target, not the developer checkout.
// Keep its small independent raw-state oracle local to these two journeys.
fn git(directory: &Path, args: &[&str]) -> Output {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "owned fixture git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn source_bytes(directory: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn collect(root: &Path, relative: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(root.join(relative)).unwrap() {
            let entry = entry.unwrap();
            let name = relative.join(entry.file_name());
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                collect(root, &name, out);
            } else {
                assert!(kind.is_file(), "actual source must be a regular file");
                out.insert(name.clone(), fs::read(root.join(name)).unwrap());
            }
        }
    }
    let mut bytes = BTreeMap::from([(
        PathBuf::from("Cargo.toml"),
        fs::read(directory.join("Cargo.toml")).unwrap(),
    )]);
    collect(directory, Path::new("src"), &mut bytes);
    bytes
}

#[derive(Debug, PartialEq, Eq)]
struct ReadOnlyWitness {
    source: BTreeMap<PathBuf, Vec<u8>>,
    index: Option<Vec<u8>>,
    head: Vec<u8>,
    config: Vec<u8>,
    refs: Vec<u8>,
    status: Vec<u8>,
    remotes: Vec<u8>,
    untracked: Option<Vec<u8>>,
}

impl ReadOnlyWitness {
    fn capture(directory: &Path) -> Self {
        Self {
            source: source_bytes(directory),
            index: fs::read(directory.join(".git/index")).ok(),
            head: fs::read(directory.join(".git/HEAD")).unwrap(),
            config: fs::read(directory.join(".git/config")).unwrap(),
            refs: git(
                directory,
                &[
                    "for-each-ref",
                    "--format=%(refname)%00%(objectname)%00%(symref)",
                ],
            )
            .stdout,
            status: git(
                directory,
                &["status", "--porcelain=v2", "-z", "--untracked-files=all"],
            )
            .stdout,
            remotes: git(directory, &["remote", "-v"]).stdout,
            untracked: fs::read(directory.join("untracked-canary.txt")).ok(),
        }
    }
}

fn exact_prepared_repository_source(session: &mut Session, directory: &Path, mouse: bool) {
    const TITLE: &str = "Repository source identity • read-only";
    const CONTROLS: &[&str] = &["[PgUp Previous]", "[PgDn More]", "[Esc Close]"];
    let exact = format!(
        "Local repository: {}",
        ghtui::git::display_path_bytes(directory.as_os_str().as_bytes())
    );
    session.wait_for_cursorless_condition(
        |frame| {
            frame.size() == (40, 120)
                && crate::support::main_footer_visible(frame)
                && crate::support::locate_screen(frame, "I Source").is_some()
        },
        "actual loaded local source control before its pure explanation",
        Duration::from_secs(8),
    );
    let before = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("I Source");
    } else {
        session.send_key(b"I");
    }
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            frame.size() == (40, 120)
                && crate::support::main_footer_visible(frame)
                && crate::support::overlay_body(frame, TITLE).is_some_and(|body| {
                    CONTROLS.iter().all(|label| body.contains(label))
                        && crate::owned_form_prose_v120::adjacent_literal(&body, &exact)
                })
        },
        "full exact local repository path in one owned completed prepared source document",
        Duration::from_secs(8),
    );
    let before = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.send_key(b"\x1b");
    }
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            crate::support::main_footer_visible(frame)
                && crate::support::locate_screen(frame, "Repository • focused view").is_some()
                && crate::support::locate_screen(frame, "I Source").is_some()
                && crate::support::overlay_body(frame, TITLE).is_none()
        },
        "pure Source identity Close returns to the same actual local Repository",
        Duration::from_secs(8),
    );
}
#[test]
fn project_git_init_keyboard_real_project_and_subdirectory_are_read_only() {
    project(false)
}
#[test]
fn project_git_init_mouse_real_project_and_subdirectory_are_read_only() {
    project(true)
}
fn project(mouse: bool) {
    let actual_source = source_bytes(Path::new(env!("CARGO_MANIFEST_DIR")));
    // Two states x root/subdirectory x both input tests = eight real sessions.
    for committed_dirty in [false, true] {
        let fixture = Fixture::new();
        let directory = fixture.root.join(
            "owned actual Rust project — café 雪 — full source identity wraps without truncation — no remote",
        );
        fs::create_dir(&directory).unwrap();
        for (relative, bytes) in &actual_source {
            let file = directory.join(relative);
            fs::create_dir_all(file.parent().unwrap()).unwrap();
            fs::write(file, bytes).unwrap();
        }
        assert_eq!(source_bytes(&directory), actual_source);
        git(&directory, &["init", "--initial-branch=main"]);
        git(
            &directory,
            &["config", "user.name", "Owned Project Fixture"],
        );
        git(
            &directory,
            &["config", "user.email", "project@example.invalid"],
        );
        assert!(directory.join(".git").is_dir());
        assert!(git(&directory, &["remote", "-v"]).stdout.is_empty());
        if committed_dirty {
            git(&directory, &["add", "--", "Cargo.toml", "src"]);
            git(
                &directory,
                &[
                    "-c",
                    "commit.gpgSign=false",
                    "commit",
                    "-m",
                    "Actual project source",
                ],
            );
            let file = directory.join("src/lib.rs");
            let mut bytes = fs::read(&file).unwrap();
            bytes.extend_from_slice(b"\n// staged project fixture change\n");
            fs::write(&file, &bytes).unwrap();
            git(&directory, &["add", "--", "src/lib.rs"]);
            bytes.extend_from_slice(b"// separate unstaged project fixture change\n");
            fs::write(file, bytes).unwrap();
            fs::write(
                directory.join("untracked-canary.txt"),
                b"never add or alter this file\n",
            )
            .unwrap();
        }
        let before = ReadOnlyWitness::capture(&directory);
        assert!(before.remotes.is_empty());
        if committed_dirty {
            assert!(before.status.windows(5).any(|bytes| bytes == b"1 MM "));
            assert!(before
                .status
                .split(|byte| *byte == 0)
                .any(|record| record == b"? untracked-canary.txt"));
            assert!(before.index.is_some());
            assert!(!before.refs.is_empty());
        } else {
            assert!(before.index.is_none());
            assert!(before.refs.is_empty());
        }
        let expected = directory.canonicalize().unwrap();
        for start in [directory.clone(), directory.join("src")] {
            let repository = ghtui::git::GitRepository::discover(&start)
                .unwrap()
                .unwrap();
            assert_eq!(repository.root.canonicalize().unwrap(), expected);
            let status = repository.status().unwrap();
            assert_eq!(status.branch.as_deref(), Some("main"));
            assert_eq!(status.head.is_some(), committed_dirty);
            assert_eq!(ReadOnlyWitness::capture(&directory), before);
            let mut session = Session::launch_from(&fixture, &start, 120, 40);
            session.wait_for_screen("Local Git workspace ready");
            // The compact header is not enough: retain the ENTIRE prepared path proof.
            assert!(session.inspect_command_journal(&fixture).is_empty());
            exact_prepared_repository_source(&mut session, &directory, mouse);
            assert!(session.inspect_command_journal(&fixture).is_empty());
            if mouse {
                session.click_visible_action("[t Tree]")
            } else {
                session.send_key(b"t")
            };
            session.wait_for_screen("Repository loaded • read-only exploration");
            session.resize(80, 24);
            if mouse {
                session.click_visible_action("[o Overview]")
            } else {
                session.send_key(b"o")
            };
            session.wait_for_screen("Branch: main");
            if !committed_dirty {
                session.wait_for_screen("Branch: main · no commits yet");
            }
            assert!(session.inspect_command_journal(&fixture).is_empty());
            session.quit_and_assert_restored_terminal(mouse);
            assert_eq!(
                ReadOnlyWitness::capture(&directory),
                before,
                "read-only project from {start:?}"
            );
        }
    }
}
