//! TASK-13 AC2: the PR Summary shows the full base and head SHAs and the issues its
//! Description closes; Metadata shows three-state permissions (an unreported flag is
//! unavailable, never false), requested reviewers and teams, linked issues and the fork
//! origin. Opening Metadata shows its loading state, a failed read shows its error and
//! keeps the Summary, and `u` reads again. Keys and clicks, 120x40 then 80x24; Git,
//! config and the cache stay unchanged, and every gh call is an exact read.
use super::*;
use std::time::Duration;

const BODY: &str = "Fixes #12 and closes org/beta#3\nSecond paragraph";
const HEAD: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const BASE: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const PERMISSIONS: &str =
    "Your repository permissions: read true • write unavailable • maintain false • admin false";
/// The part of the permissions row that fits one 80-column line (the rest wraps).
const PERMISSIONS_NARROW: &str =
    "Your repository permissions: read true • write unavailable • maintain false";

/// Replaces the shared fixture Description with one that closes two issues.
fn described(value: &mut Value) {
    match value {
        Value::String(text) if text == "A useful change\nSecond paragraph" => {
            *text = BODY.into();
        }
        Value::Object(map) => map.values_mut().for_each(described),
        Value::Array(values) => values.iter_mut().for_each(described),
        _ => {}
    }
}

/// The pr_details Summary and Metadata reads, then: a Metadata read whose issue read
/// fails after a slow repository read, and a second, complete Metadata read.
fn steps() -> (Vec<GhStep>, usize, usize) {
    let (all, before_files, _) = detail_steps_v83();
    let mut steps = all[..before_files].to_vec();
    let mut metadata = steps.split_off(15);
    assert_eq!(metadata.len(), 10);
    for step in &mut metadata {
        if step.args.get(1).is_some_and(|arg| arg == "repos/org/alpha") {
            // GitHub left `push` out: unknown, not denied.
            let mut repository = detail_repository_v83();
            repository["permissions"] = json!({"pull":true,"maintain":false,"admin":false});
            step.stdout = serde_json::to_vec(&repository).unwrap();
        }
    }
    let mut failing = metadata[..5].to_vec();
    assert_eq!(failing[2].args[1], "repos/org/alpha");
    failing[2].delay_ms = 1500;
    assert_eq!(failing[4].args[1], "repos/org/alpha/issues/7");
    failing[4].status = 1;
    failing[4].stdout = vec![];
    failing[4].stderr = b"gh: Server Error (HTTP 502)\n".to_vec();
    let summary = steps.len();
    steps.extend(failing);
    let failed = steps.len();
    steps.extend(metadata);
    for step in &mut steps {
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            described(&mut value);
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
    }
    (steps, summary, failed)
}

/// Scrolls the PR page (Down or the wheel) until `text` is painted.
fn observe(session: &mut Session, mouse: bool, text: &str) {
    for _ in 0..16 {
        if session
            .try_wait_for(text, Duration::from_millis(40))
            .is_ok()
        {
            return;
        }
        if mouse {
            session.wheel(10, 12, true);
        } else {
            session.send_key(b"\x1b[B");
        }
    }
    session.wait_for_screen(text);
}

/// Scrolls the PR page back to its first row (Up or the wheel).
fn to_top(session: &mut Session, mouse: bool) {
    for _ in 0..20 {
        if mouse {
            session.wheel(10, 12, false);
        } else {
            session.send_key(b"\x1b[A");
        }
    }
}

fn metadata_rows(session: &mut Session, mouse: bool) {
    // The rows are checked top to bottom, so start from the first one.
    to_top(session, mouse);
    session.wait_for_screen(PERMISSIONS_NARROW);
    for text in [
        "Fork origin: contributor/alpha • fork of org/alpha",
        "Base repository: org/alpha • cross-repository true",
        "Milestone: Launch",
        "@requested-reviewer review requested",
        "Review team: Security team (security) • @org/security",
        "Linked issues: #12, org/beta#3",
        "Edit access unknown",
    ] {
        observe(session, mouse, text);
    }
    assert!(!session.screen().contains("write false"));
}

fn journey(mouse: bool) {
    let fixture = configured();
    fixture.init_git();
    fixture.commit_file("kept.txt", b"saved bytes\n", "Summary context journey");
    std::fs::write(fixture.repo().join("kept.txt"), b"dirty bytes\n").unwrap();
    let git_paths = [".git/HEAD", ".git/index", ".git/config", "kept.txt"];
    let original_git = git_paths.map(|path| std::fs::read(fixture.repo().join(path)).unwrap());
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let original_config = std::fs::read(&config_path).unwrap();
    let (steps, summary, failed) = steps();
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_main_frame();
    let after = session.completed_cursorless_end();
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            frame.contents().contains("Inbox • focused view")
                && crate::support::main_footer_visible(frame)
        },
        "Inbox after local Git startup",
        Duration::from_secs(8),
    );
    refresh(&mut session, mouse);
    session.wait_for_screen("3 unique PRs across 5 queue matches");
    if mouse {
        session.click_visible_action("Improve auth")
    } else {
        session.send_key(b"\r")
    }
    // Summary: both full SHAs and the closing-keyword issues, from the loaded PR alone.
    session.wait_for_screen(&format!("Head {HEAD}"));
    session.wait_for_screen(&format!("Base {BASE}"));
    session.wait_for_screen("Linked issues: #12, org/beta#3 • closing keywords");
    journal(&mut session, &fixture, summary);
    let sql = detail_sql_v83(&fixture, false);

    // Loading, then a failed read: the error is named, the Summary stays.
    choose(&mut session, mouse, "[u Metadata]", b"u");
    session.wait_for_screen("Loading PR Metadata");
    session.wait_for_control("[Esc Cancel]");
    session.wait_for_screen("Metadata unavailable • GitHub read failed • u Metadata reads again");
    journal(&mut session, &fixture, failed);
    let screen = session.screen();
    assert!(screen.contains(&format!("Head {HEAD}")), "{screen}");
    assert!(!screen.contains("Your repository permissions"), "{screen}");
    assert!(!screen.contains("Loading PR Metadata"), "{screen}");
    assert_eq!(
        detail_sql_v83(&fixture, false),
        sql,
        "A failed read stores nothing"
    );

    // Reading again succeeds.
    choose(&mut session, mouse, "[u Metadata]", b"u");
    session.wait_for_screen(PERMISSIONS);
    journal(&mut session, &fixture, steps.len());
    metadata_rows(&mut session, mouse);

    // 80x24: the same facts stay reachable, on Metadata and back on Summary.
    session.resize(80, 24);
    metadata_rows(&mut session, mouse);
    choose(&mut session, mouse, "[o Summary]", b"o");
    observe(&mut session, mouse, "Linked issues: #12, org/beta#3");
    let after = session.completed_cursorless_end();
    session.resize(120, 40);
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            text.contains(&format!("Head {HEAD}")) && text.contains(&format!("Base {BASE}"))
        },
        "Full SHAs on the Summary after resizing back",
        Duration::from_secs(8),
    );
    journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);

    assert_eq!(std::fs::read(&config_path).unwrap(), original_config);
    for (path, original) in git_paths.into_iter().zip(original_git) {
        assert_eq!(
            std::fs::read(fixture.repo().join(path)).unwrap(),
            original,
            "Read-only PR context preserves Git bytes: {path}"
        );
    }
    let rows: Vec<Value> = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), steps.len());
    for (row, expected) in rows.iter().zip(&steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(row["stdin_hex"], "");
        assert!(!expected
            .args
            .iter()
            .any(|arg| matches!(arg.as_str(), "POST" | "PUT" | "PATCH" | "DELETE")));
    }
}

#[test]
fn pr_summary_context_v13_keyboard_loading_error_retry_and_narrow() {
    journey(false)
}

#[test]
fn pr_summary_context_v13_mouse_loading_error_retry_and_narrow() {
    journey(true)
}
