//! Two cancelled post-exit pipe holders must not occupy both actual workers.
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession};
use crate::support::{empty_inbox_steps, Fixture, GhStep, Session};
use std::{
    fs,
    time::{Duration, Instant},
};

fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
fn state(f: &Fixture) -> Vec<Vec<u8>> {
    vec![
        f.git(&["rev-parse", "HEAD"]).into_bytes(),
        f.git(&["show-ref"]).into_bytes(),
        f.git(&["--no-optional-locks", "status", "--porcelain=v2", "-z"])
            .into_bytes(),
        fs::read(f.repo().join(".git/index")).unwrap(),
        fs::read(f.repo().join(".git/config")).unwrap(),
        fs::read(f.repo().join("preserved.txt")).unwrap(),
    ]
}
fn journey(mouse: bool) {
    let f = Fixture::new();
    f.init_git();
    f.commit_file("preserved.txt", b"saved\n", "saved pipe fixture");
    fs::write(
        f.repo().join("preserved.txt"),
        b"uncommitted bytes retained\0\xff",
    )
    .unwrap();
    let version = || GhStep::read(&["--version"], "gh version 2.101.0\n");
    let mut steps = vec![version(), version()];
    steps.extend(empty_inbox_steps("pipe-recovered"));
    f.scripted_gh(&steps);
    for step in 0..2 {
        fs::write(
            f.root.join(format!("gh-fixture/{step:03}.pipe-holder-ms")),
            "8000",
        )
        .unwrap();
    }
    let original = state(&f);
    let mut session = Session::launch(&f, 120, 40);
    session.wait_for_screen("ghtui");
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.assert_focused_view("Inbox");
    let config = fs::read(f.root.join("config/ghtui/config.toml")).ok();
    let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    let mut holders = Vec::new();
    for step in 0..2 {
        choose(&mut session, mouse, "[r Refresh]", b"r");
        session.wait_for_command_count(&f, step + 1);
        let deadline = Instant::now() + Duration::from_secs(2);
        let holder = loop {
            if let Ok(record) =
                fs::read_to_string(f.root.join(format!("gh-fixture/{step:03}.pipe-holder-pid")))
            {
                if let Some(decimal) = record.strip_suffix('\n') {
                    if !decimal.is_empty() && decimal.bytes().all(|b| b.is_ascii_digit()) {
                        let pid: i32 = decimal.parse().unwrap();
                        let identity = process_identity(pid)
                            .unwrap()
                            .expect("actual finite pipe helper");
                        assert!(owner.owns(&identity));
                        assert_eq!(
                            unsafe { libc::getpgid(pid) },
                            pid,
                            "genuinely distinct helper group"
                        );
                        break identity;
                    }
                }
            }
            assert!(Instant::now() < deadline, "complete owned helper PID");
            session.poll_output(Duration::from_millis(2));
        };
        let journal = session.inspect_command_journal(&f);
        let calls: Vec<serde_json::Value> = journal
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        let parent = calls[step]["pid"].as_i64().unwrap() as i32;
        session
            .try_wait_for_condition(
                |_| process_identity(parent).is_ok_and(|p| p.is_none()),
                "original version child actually reaped before Cancel",
                Duration::from_secs(2),
            )
            .unwrap();
        choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
        session.wait_for_screen("Cancelled");
        assert_eq!(
            session.inspect_command_journal(&f).lines().count(),
            step + 1
        );
        holders.push(holder);
        if step == 0 {
            session.resize(80, 24);
            session.wait_for_control("[r Refresh]");
        }
    }
    // Both production worker slots were used by exited parents with live
    // inherited pipes. The next read must finish before either holder releases.
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session
        .try_wait_for_condition(
            |screen| screen.contains("@pipe-recovered") && screen.contains("Fresh • fetched"),
            "fresh read uses cancelled workers before inherited holders exit",
            Duration::from_secs(3),
        )
        .unwrap();
    session.wait_for_command_count(&f, steps.len());
    for holder in &holders {
        let current = process_identity(holder.pid)
            .unwrap()
            .expect("helper must still hold pipes during recovery proof");
        assert!(owner.owns(&current) && !current.zombie);
        assert_eq!(current.birth, holder.birth);
    }
    let records = session.inspect_command_journal(&f);
    let calls: Vec<serde_json::Value> = records
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(calls.len(), steps.len());
    for (call, expected) in calls.iter().zip(&steps) {
        assert_eq!(call["matched"], true);
        assert_eq!(call["args"], serde_json::json!(expected.args));
        assert_eq!(call["stdin_hex"], "");
    }
    assert_eq!(state(&f), original);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).ok(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
    drop(session); // exact owned-session cleanup includes the distinct groups.
    for holder in holders {
        assert!(process_identity(holder.pid).unwrap().is_none());
    }
}

#[test]
fn process_runner_pipe_keyboard_cancelled_post_exit_pipes_release_both_workers_and_preserve_git() {
    journey(false);
}
#[test]
fn process_runner_pipe_mouse_cancelled_post_exit_pipes_release_both_workers_and_preserve_git() {
    journey(true);
}
