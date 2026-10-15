//! Actual Git maintenance remains foreground while real PTY sessions browse.
use super::support::{Fixture, Session};
use std::{collections::BTreeMap, ffi::OsStr, fs, path::Path, process::Command};

fn policy() -> BTreeMap<String, String> {
    // Execute the SAME immutable composer used by both verification entrypoints.
    // Keep only indexed Git config, not unrelated parent secrets/environment.
    let result = Command::new("node")
        .args([
            "--input-type=module",
            "-e",
            "import {testEnvironment} from './scripts/test-env.mjs'; console.log(JSON.stringify(Object.fromEntries(Object.entries(testEnvironment()).filter(([key]) => /^GIT_CONFIG_(COUNT|KEY_[0-9]+|VALUE_[0-9]+)$/.test(key)))));",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(result.status.success(), "test-child policy failed");
    serde_json::from_slice(&result.stdout).unwrap()
}

fn git(root: &Path, args: &[&str], policy: &BTreeMap<String, String>, trace: &Path) -> Vec<u8> {
    let result = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .envs(policy)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TRACE2_EVENT", trace)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "actual fixture Git {args:?}: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    result.stdout
}

fn initialize(fixture: &Fixture, policy: &BTreeMap<String, String>, trace: &Path) {
    for args in [
        vec!["init", "--initial-branch=main"],
        vec!["config", "user.name", "Maintenance Fixture"],
        vec!["config", "user.email", "fixture@example.invalid"],
        // Deliberately opposite repository policy. Child settings must override
        // it WITHOUT changing repository bytes or global Git configuration.
        vec!["config", "maintenance.autoDetach", "true"],
        vec!["config", "gc.autoDetach", "true"],
    ] {
        git(&fixture.repo(), &args, policy, trace);
    }
    fs::write(fixture.repo().join("file.txt"), b"saved content\n").unwrap();
    git(&fixture.repo(), &["add", "--", "file.txt"], policy, trace);
    git(
        &fixture.repo(),
        &[
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-m",
            "Foreground saved version",
        ],
        policy,
        trace,
    );
}

fn journey(mouse: bool) {
    let policy = policy();
    let fixture = Fixture::new();
    let trace = fixture.root.join("git-trace.jsonl");
    initialize(&fixture, &policy, &trace);
    fs::write(fixture.repo().join("file.txt"), b"dirty working content\n").unwrap();
    let head = git(&fixture.repo(), &["rev-parse", "HEAD"], &policy, &trace);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let config = fs::read(fixture.repo().join(".git/config")).unwrap();
    let env = policy
        .iter()
        .map(|(key, value)| (OsStr::new(key), OsStr::new(value)))
        .collect::<Vec<_>>();
    let mut session = Session::launch_binary_os_env(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        120,
        40,
        &env,
    );
    session.wait_for_screen("ghtui");
    if mouse {
        session.click_visible_action("Repository");
    } else {
        session.send_key(b"2");
    }
    session.wait_for_screen("Foreground saved version");
    // The harness is NOW a real Linux subreaper. Later fixture commits used to
    // leave a detached Git zombie for every subsequent journey in this binary.
    for _ in 0..3 {
        let later = Fixture::new();
        initialize(&later, &policy, &trace);
        assert_eq!(
            git(&later.repo(), &["show", "HEAD:file.txt"], &policy, &trace),
            b"saved content\n"
        );
        assert!(git(&later.repo(), &["status", "--porcelain"], &policy, &trace).is_empty());
    }
    if mouse {
        session.click_visible_action("[w Working]");
    } else {
        session.send_key(b"w");
    }
    session.wait_for_screen("dirty working content");
    session.resize(80, 24);
    if mouse {
        session.click_visible_action("[? Help]");
    } else {
        session.send_key(b"?");
    }
    session.wait_for_screen("Help • keyboard and mouse");
    if mouse {
        session.click_visible_action("[Esc Close]");
    } else {
        session.close_overlay_by_key("Help • keyboard and mouse");
    }
    let events = fs::read_to_string(&trace).unwrap();
    let maintenance = events
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .filter(|event| {
            event["event"] == "child_start"
                && event["argv"]
                    .as_array()
                    .is_some_and(|args| args.iter().any(|arg| arg == "maintenance"))
        })
        .collect::<Vec<_>>();
    assert_eq!(maintenance.len(), 4);
    for event in maintenance {
        let args = event["argv"].as_array().unwrap();
        assert!(args.iter().any(|arg| arg == "--no-detach"));
        assert!(!args.iter().any(|arg| arg == "--detach"));
    }
    assert_eq!(
        git(&fixture.repo(), &["rev-parse", "HEAD"], &policy, &trace),
        head
    );
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join(".git/config")).unwrap(),
        config
    );
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"dirty working content\n"
    );
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn test_framework_git_maintenance_keyboard_later_fixtures_and_read_only_navigation() {
    journey(false);
}

#[test]
fn test_framework_git_maintenance_mouse_later_fixtures_resize_and_read_only_navigation() {
    journey(true);
}
