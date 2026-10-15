//! Real failed Git pop, actual keyboard/mouse, original source and effect proof.
use crate::{
    local, local_management, raw_git_argv_v1,
    support::{Fixture, Session},
};
use ghtui::local_actions::LocalCommand;
use std::{fs, time::Duration};
fn completed(session: &mut Session, expected: &[&str], absent: &[&str]) {
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            expected.iter().all(|s| text.contains(s)) && absent.iter().all(|s| !text.contains(s))
        },
        "completed original stash inspection",
        Duration::from_secs(8),
    );
}
fn completed_new(session: &mut Session, after: usize, expected: &[&str], absent: &[&str]) {
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            expected.iter().all(|s| text.contains(s)) && absent.iter().all(|s| !text.contains(s))
        },
        "NEW completed original stash inspection",
        Duration::from_secs(8),
    );
}
fn choice(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label)
    } else {
        session.send_key(key)
    }
}
fn sql(fixture: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open(fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    let mut statement = db
        .prepare("SELECT host,login,resource,body FROM drafts ORDER BY host,login,resource")
        .unwrap();
    statement
        .query_map([], |row| {
            Ok(format!(
                "{}|{}|{}|{}",
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?
            ))
        })
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
fn journey(mouse: bool, changed: bool) {
    journey_extended(mouse, changed, false);
}
fn journey_extended(mouse: bool, changed: bool, cancel: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("conflict.txt", b"base\n", "base");
    fixture.commit_file("stable.txt", b"untouched\n", "stable");
    fs::write(
        fixture.repo().join("conflict.txt"),
        b"stashed private bytes\n",
    )
    .unwrap();
    fixture.git(&["stash", "push", "-m", "PRIVATE_STASH_CANARY"]);
    let sha = fixture.git(&["rev-parse", "refs/stash"]).trim().to_owned();
    fixture.commit_file("conflict.txt", b"ours current bytes\n", "ours");
    fs::write(fixture.repo().join("scratch"), b"untracked survives\n").unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let config = fs::read(fixture.repo().join(".git/config")).unwrap();
    raw_git_argv_v1::install(&fixture.root);
    if cancel {
        let data = crate::fixture_shell_publication_v1::data_path(&fixture.root.join("bin/git"));
        let script = fs::read_to_string(&data).unwrap();
        assert_eq!(script.matches("exec ").count(), 1);
        let hold=format!("case \" $* \" in *' --unmerged '*) if [ -f '{}/hold-read' ]; then printf '%s' \"$$\" > '{}/held-read.pid'; sleep 6; fi;; esac\nexec ",fixture.root.display(),fixture.root.display());
        fs::write(data, script.replace("exec ", &hold)).unwrap();
    }
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    local_management::operation(&mut session, mouse, LocalCommand::StashPop);
    if mouse {
        let (x, y) = session.locate("Exact stash commit SHA:").unwrap();
        session.click(x, y + 1);
    }
    session.send_key(b"\x15");
    session.paste(&sha);
    session.wait_for_screen(&sha);
    local::preview(&mut session, mouse);
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen("[F3 Inspect stash result]");
    assert!(session.screen().contains("LOCAL INTENT • Preview first"));
    let refs = fixture.git(&["show-ref"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let file = fs::read(fixture.repo().join("conflict.txt")).unwrap();
    let stages = fixture.git(&["ls-files", "--unmerged"]);
    assert_eq!(stages.lines().count(), 3);
    assert_eq!(fixture.git(&["rev-parse", "refs/stash"]).trim(), sha);
    assert!(!fixture.repo().join(".git/MERGE_HEAD").exists());
    let first_inspect = session.completed_cursorless_end();
    choice(&mut session, mouse, "[F3 Inspect stash result]", b"\x1bOR");
    completed_new(
        &mut session,
        first_inspect,
        &[
            "Original stash result",
            "retained in stash list",
            "Unmerged stage rows: 3",
            "no retry/drop/reconciliation",
        ],
        &["PRIVATE_STASH_CANARY"],
    );
    let drafts = sql(&fixture);
    let body = session.wait_for_overlay_body("Original stash result");
    assert!(body.contains(&sha));
    for stage in stages.lines() {
        let id = stage.split_whitespace().nth(1).unwrap();
        assert!(body.contains(id), "Actual unmerged object {id} missing");
    }
    session.resize_owned_modal(
        80,
        24,
        "Original stash result",
        &["[R Inspect]", "[C First conflict]", "[Esc Back]"],
    );
    completed(
        &mut session,
        &["Original stash result", "[Esc Back]"],
        &["PRIVATE_STASH_CANARY"],
    );
    choice(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_screen("[F3 Inspect stash result]");
    let second_inspect = session.completed_cursorless_end();
    if cancel {
        fs::write(fixture.root.join("hold-read"), b"hold actual explicit read").unwrap();
    }
    choice(&mut session, mouse, "[F3 Inspect stash result]", b"\x1bOR");
    if cancel {
        use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession};
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        let pid = loop {
            if let Ok(text) = fs::read_to_string(fixture.root.join("held-read.pid")) {
                if let Ok(pid) = text.parse::<i32>() {
                    break pid;
                }
            }
            assert!(std::time::Instant::now() < deadline);
            session.poll_output(Duration::from_millis(1));
        };
        let identity = process_identity(pid)
            .unwrap()
            .expect("Actual held Git child must be live BEFORE Back");
        let owned = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
        assert!(owned.owns(&identity));
        assert!(!identity.zombie);
        choice(&mut session, mouse, "[Esc Back]", b"\x1b");
        session.wait_for_screen("[F3 Inspect stash result]");
        loop {
            if process_identity(pid).unwrap().is_none() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "Original held child not gone/reaped after Cancel"
            );
            session.poll_output(Duration::from_millis(1));
        }
        fs::remove_file(fixture.root.join("hold-read")).unwrap();
        let fresh = session.completed_cursorless_end();
        choice(&mut session, mouse, "[F3 Inspect stash result]", b"\x1bOR");
        completed_new(
            &mut session,
            fresh,
            &[
                "Original stash result",
                "retained in stash list",
                "Unmerged stage rows: 3",
            ],
            &["PRIVATE_STASH_CANARY"],
        );
    }
    completed_new(
        &mut session,
        second_inspect,
        &["Original stash result", "retained in stash list"],
        &["PRIVATE_STASH_CANARY"],
    );
    if changed {
        let kept = fixture.root.join("kept-original");
        fs::rename(fixture.repo(), &kept).unwrap();
        fs::create_dir(fixture.repo()).unwrap();
        let count = raw_git_argv_v1::records(&fixture.root.join("raw-git-argv-v1")).len();
        choice(&mut session, mouse, "[R Inspect]", b"r");
        completed(
            &mut session,
            &[
                "Original stash result",
                "Unavailable",
                "observations hidden",
            ],
            &[&sha, "retained in stash list", "Stage 1"],
        );
        assert_eq!(
            raw_git_argv_v1::records(&fixture.root.join("raw-git-argv-v1")).len(),
            count,
            "Source loss before observer must spawn zero children"
        );
        fs::rename(fixture.repo(), fixture.root.join("empty-replacement")).unwrap();
        fs::rename(kept, fixture.repo()).unwrap();
        choice(&mut session, mouse, "[Esc Back]", b"\x1b");
        session.wait_for_screen("[F3 Inspect stash result]");
        local::close(&mut session, mouse);
    } else {
        let conflict_open = session.completed_cursorless_end();
        choice(&mut session, mouse, "[C First conflict]", b"c");
        completed_new(
            &mut session,
            conflict_open,
            &[
                "Original conflict viewer loaded",
                "Conflict in conflict.txt",
                "ours current bytes",
            ],
            &["Original stash result"],
        );
        let scrolled = session.completed_cursorless_end();
        if mouse {
            session.wheel(40, 15, true);
        } else {
            session.send_key(b"\x1b[6~");
        }
        completed_new(
            &mut session,
            scrolled,
            &["Original conflict viewer loaded", "stashed private bytes"],
            &["Original stash result"],
        );
    }
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(fs::read(fixture.repo().join("conflict.txt")).unwrap(), file);
    assert_eq!(
        fs::read(fixture.repo().join("stable.txt")).unwrap(),
        b"untouched\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("scratch")).unwrap(),
        b"untracked survives\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        config
    );
    assert_eq!(sql(&fixture), drafts);
    let records = raw_git_argv_v1::records(&fixture.root.join("raw-git-argv-v1"));
    assert_eq!(
        records
            .iter()
            .filter(|r| r
                .argv
                .windows(2)
                .any(|w| w == [b"stash".to_vec(), b"pop".to_vec()]))
            .count(),
        1,
        "Exactly original confirmed pop; no resend"
    );
    assert!(
        !records.iter().any(|r| r
            .argv
            .windows(2)
            .any(|w| w == [b"stash".to_vec(), b"drop".to_vec()])),
        "Read-only inspection never drops stash"
    );
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "Local Git never authenticates with gh"
    );
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn stash_recovery_keyboard_actual_pop_inspect_return_and_conflict() {
    journey(false, false)
}
#[test]
fn stash_recovery_mouse_actual_pop_inspect_return_and_conflict() {
    journey(true, false)
}
#[test]
fn stash_recovery_keyboard_visible_old_report_erased_before_source_loss_read() {
    journey(false, true)
}
#[test]
fn stash_recovery_mouse_visible_old_report_erased_before_source_loss_read() {
    journey(true, true)
}
#[test]
fn stash_recovery_keyboard_actual_held_read_cancel_reaps_original_child_and_explicit_fresh_read() {
    journey_extended(false, false, true)
}
#[test]
fn stash_recovery_mouse_actual_held_read_cancel_reaps_original_child_and_explicit_fresh_read() {
    journey_extended(true, false, true)
}

fn linked(mouse: bool) {
    use std::os::unix::ffi::OsStrExt;
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("conflict.txt", b"base\n", "base");
    fs::write(
        fixture.repo().join("conflict.txt"),
        b"stashed linked bytes\n",
    )
    .unwrap();
    fixture.git(&["stash", "push", "-m", "linked private stash"]);
    let sha = fixture.git(&["rev-parse", "refs/stash"]).trim().to_owned();
    fixture.commit_file("conflict.txt", b"main current bytes\n", "current");
    fixture.git(&["branch", "linked"]);
    let checkout = fixture
        .root
        .join(crate::support::exact_worktree_component());
    let output = std::process::Command::new("git")
        .current_dir(fixture.repo())
        .arg("worktree")
        .arg("add")
        .arg("--")
        .arg(&checkout)
        .arg("linked")
        .output()
        .unwrap();
    assert!(output.status.success());
    let main_index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let main_head = fixture.git(&["rev-parse", "HEAD"]);
    let config = fs::read(fixture.repo().join(".git/config")).unwrap();
    let current = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .current_dir(&checkout)
            .args(args)
            .output()
            .unwrap();
        assert!(output.status.success());
        String::from_utf8(output.stdout).unwrap()
    };
    raw_git_argv_v1::install(&fixture.root);
    let cwd_records = fixture.root.join("raw-linked-git-cwd-v1");
    fs::create_dir(&cwd_records).unwrap();
    let data = crate::fixture_shell_publication_v1::data_path(&fixture.root.join("bin/git"));
    let script = fs::read_to_string(&data).unwrap();
    assert_eq!(script.matches("exec ").count(), 1);
    fs::write(
        data,
        script.replace(
            "exec ",
            &format!(
                "printf '%s\\000' \"$PWD\" > '{}/'\"$$\"\nexec ",
                cwd_records.display()
            ),
        ),
    )
    .unwrap();
    let mut session = Session::launch_from(&fixture, &checkout, 120, 40);
    session.wait_for_screen("Recent commits");
    local_management::operation(&mut session, mouse, LocalCommand::StashPop);
    if mouse {
        let (x, y) = session.locate("Exact stash commit SHA:").unwrap();
        session.click(x, y + 1);
    }
    session.send_key(b"\x15");
    session.paste(&sha);
    session.wait_for_screen(&sha);
    local::preview(&mut session, mouse);
    local::confirm(&mut session, mouse, 1);
    session.wait_for_screen("[F3 Inspect stash result]");
    let stages = current(&["ls-files", "--unmerged"]);
    assert_eq!(stages.lines().count(), 3);
    let directory = std::process::Command::new("git")
        .current_dir(&checkout)
        .args(["rev-parse", "--absolute-git-dir"])
        .output()
        .unwrap();
    assert!(directory.status.success());
    let raw_directory = directory
        .stdout
        .strip_suffix(b"\n")
        .expect("Actual Git directory has its one output terminator");
    let index_path =
        std::path::PathBuf::from(std::ffi::OsStr::from_bytes(raw_directory)).join("index");
    let index = fs::read(&index_path).unwrap();
    let file = fs::read(checkout.join("conflict.txt")).unwrap();
    let drafts = sql(&fixture);
    let before = session.completed_cursorless_end();
    choice(&mut session, mouse, "[F3 Inspect stash result]", b"\x1bOR");
    completed_new(
        &mut session,
        before,
        &[
            "Original stash result",
            "retained in stash list",
            crate::support::exact_worktree_label(),
            &sha,
        ],
        &[],
    );
    let body = session.wait_for_overlay_body("Original stash result");
    for stage in stages.lines() {
        assert!(body.contains(stage.split_whitespace().nth(1).unwrap()));
    }
    choice(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_screen("[F3 Inspect stash result]");
    local::close(&mut session, mouse);
    assert_eq!(current(&["ls-files", "--unmerged"]), stages);
    assert_eq!(fs::read(&index_path).unwrap(), index);
    assert_eq!(fs::read(checkout.join("conflict.txt")).unwrap(), file);
    assert_eq!(sql(&fixture), drafts);
    assert_eq!(fixture.git(&["ls-files", "--unmerged"]), "");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), main_head);
    assert_eq!(
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        main_index
    );
    assert_eq!(
        fs::read(fixture.repo().join("conflict.txt")).unwrap(),
        b"main current bytes\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        config
    );
    assert_eq!(fixture.git(&["rev-parse", "refs/stash"]).trim(), sha);
    let records = raw_git_argv_v1::records(&fixture.root.join("raw-git-argv-v1"));
    let writes: Vec<_> = records
        .iter()
        .filter(|r| {
            r.argv
                .windows(2)
                .any(|w| w == [b"stash".to_vec(), b"pop".to_vec()])
        })
        .collect();
    assert_eq!(writes.len(), 1);
    let cwd = fs::read(cwd_records.join(writes[0].pid.to_string())).unwrap();
    assert_eq!(cwd.last(), Some(&0));
    assert_eq!(
        &cwd[..cwd.len() - 1],
        checkout.as_os_str().as_bytes(),
        "Actual confirmed pop must run in the original linked checkout, not main or a display path"
    );
    assert!(!records
        .iter()
        .any(|r| r.argv.iter().any(|arg| arg == b"drop")));
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn stash_recovery_keyboard_original_linked_worktree_pop_leaves_main_checkout_untouched() {
    linked(false)
}
#[test]
fn stash_recovery_mouse_original_linked_worktree_pop_leaves_main_checkout_untouched() {
    linked(true)
}
