//! DECLARED / UNRUN. Starts from the genuine author/assignee/review-requested
//! Personal queues and actual Files source, never hidden catalogue discovery.
use super::*;
#[path = "file_comments_personal_recovery_v75.rs"]
mod recovery_v75;
use crate::repository_catalogue_native_v1::scoped_pr_read_v1::resources::review_threads_v1::file_comments_v75 as file;
fn bind_value(value: &mut Value) {
    match value {
        Value::String(text) => match text.as_str() {
            "GENUINE_CANONICAL_PR_NODE_1007" => *text = "PERSONAL_PR_1007".into(),
            "REPO_catalogue_10000" => *text = "R_10000".into(),
            "src/界.rs" => *text = "src/view.rs".into(),
            _ => {}
        },
        Value::Array(values) => {
            for value in values {
                bind_value(value);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                bind_value(value);
            }
        }
        _ => {}
    }
}
fn bind(steps: &mut [GhStep]) {
    for step in steps {
        if !step.stdin.is_empty() {
            let mut value: Value = serde_json::from_slice(&step.stdin).unwrap();
            bind_value(&mut value);
            step.stdin = serde_json::to_vec(&value).unwrap();
        }
        if step.args.iter().any(|v| v == "--include") {
            let raw = std::str::from_utf8(&step.stdout).unwrap();
            let (header, body) = raw.split_once("\r\n\r\n").unwrap();
            let mut value: Value = serde_json::from_str(body).unwrap();
            bind_value(&mut value);
            step.stdout = format!("{header}\r\n\r\n{value}").into_bytes();
        } else if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            if value.get("head").is_some() && value.get("base").is_some() {
                value = parent();
            }
            if value.get("permissions").is_some() && value.get("full_name").is_some() {
                value = repository();
            }
            bind_value(&mut value);
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
    }
}
fn journey(mouse: bool) {
    journey_with_recovery(mouse, None)
}
fn journey_with_recovery(mouse: bool, recovery: Option<recovery_v75::RecoveryV75>) {
    let fixture = configured();
    fixture.git(&["init", "-q"]);
    fixture.git(&["config", "user.name", "Personal FILE"]);
    fixture.git(&["config", "user.email", "file@example.invalid"]);
    fs::write(fixture.repo().join("kept"), b"saved\n").unwrap();
    fixture.git(&["add", "--", "kept"]);
    fixture.git(&["commit", "-qm", "Original independent Personal FILE"]);
    fs::write(fixture.repo().join("kept"), b"dirty personal FILE\n").unwrap();
    let git = [".git/HEAD", ".git/index", ".git/config", "kept"]
        .map(|path| fs::read(fixture.repo().join(path)).unwrap());
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let (mut steps, opened) = complete_steps();
    let files = steps.len();
    let mut phase = file::observation();
    bind(&mut phase);
    steps.extend(phase.clone());
    steps.extend(phase);
    let mut write = file::happy_confirmation_v75();
    bind(&mut write);
    let write_start = steps.len();
    if let Some(recovery) = recovery {
        write[6].delay_ms = 6000;
        write.truncate(if recovery.pending() { 7 } else { 17 });
    }
    steps.extend(write);
    let original_end = steps.len();
    if let Some(recovery) = recovery {
        recovery_v75::append_steps(&mut steps, recovery);
    }
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("1 unique PRs across 3 queue matches");
    if mouse {
        session.click_visible_action("Actual personal source");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen(BODY);
    journal(&mut session, &fixture, opened);
    choose(&mut session, mouse, "[f Files]", b"f");
    journal(&mut session, &fixture, files);
    session.wait_for_cursorless_condition(
        |f| {
            let s = f.contents();
            s.contains("Personal PR Files")
                && s.contains("File src/view.rs")
                && s.contains("[K FILE comment]")
        },
        "Actual original Personal Files source and FILE control, not Catalogue",
        Duration::from_secs(8),
    );
    session.select_completed_progress_file(mouse, "File src/view.rs");
    assert_eq!(
        fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap()
            .lines()
            .count(),
        files
    );
    // Full original SQL witness AFTER legitimate Personal source publication.
    let before = sql(&fixture);
    choose(&mut session, mouse, "[K FILE comment]", b"K");
    session.wait_for_command_count(&fixture, files + 11);
    file::form_ready(&mut session, false);
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", "Body");
    }
    session.paste("PRIVATE_NATIVE_WHOLE_FILE_BODY 界\nsecond FILE line");
    file::form_control(&mut session, mouse, "[Preview]");
    session.wait_for_command_count(&fixture, files + 22);
    file::form_ready(&mut session, true);
    let at = session.completed_cursorless_end();
    file::form_control(&mut session, mouse, "[Confirm action]");
    if let Some(recovery) = recovery {
        recovery_v75::hold_or_cancel(&mut session, &fixture, mouse, write_start, recovery);
    }
    session.wait_for_command_count(&fixture, original_end);
    session.wait_for_new_cursorless_condition(at,|f|{let s=f.contents();!s.contains(BODY)&&!s.contains("PRIVATE_NATIVE_WHOLE_FILE_BODY")&&!s.contains(&file::title())&&if recovery.is_some_and(|r|r.pending()){s.contains("canceled")||s.contains("reload explicitly")}else if recovery.is_some(){s.contains("FILE comment observed")&&s.contains("source unavailable")}else{s.contains("FILE comment posted #9181")&&s.contains("refresh explicitly")}},"Typed Personal FILE outcome retires exact original private page/Form without replay or private underlay",Duration::from_secs(8));
    let db = rusqlite::Connection::open_with_flags(
        fixture.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let(row,origin):(String,String)=db.query_row("SELECT a.state,o.origin_json FROM pr_write_attempts a JOIN pr_write_file_comment_origins_v75 o USING(attempt_id)",[],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    if recovery.is_some_and(|r| r.pending()) {
        assert!(matches!(row.as_str(), "pending" | "uncertain"));
    } else {
        assert_eq!(row, "confirmed");
    }
    let origin: Value = serde_json::from_str(&origin).unwrap();
    assert_eq!(origin["seed"]["pr_node"], "PERSONAL_PR_1007");
    assert_eq!(origin["seed"]["repository_node"], "R_10000");
    assert_eq!(origin["seed"]["path"], "src/view.rs");
    assert_eq!(origin["seed"]["scope"]["login"], "user-id:42");
    // No alternate auth/catalogue discovery child exists in the exact fixture.
    if let Some(recovery) = recovery {
        session =
            recovery_v75::inspect_restart(&mut session, &fixture, mouse, original_end, recovery);
    }
    file::exact(
        &session,
        &fixture,
        &steps,
        usize::from(!recovery.is_some_and(|r| r.pending())),
    );
    assert!(!steps.iter().any(|step| step
        .args
        .get(1)
        .is_some_and(|s| s == "user/repos" || s.starts_with("orgs/"))));
    let after = sql(&fixture);
    for line in before.iter().filter(|line| {
        line.starts_with("saved_views:")
            || line.contains("PRIVATE_43")
            || line.starts_with("actions_write_attempts:")
    }) {
        assert!(after.contains(line));
    }
    for (path, expected) in [".git/HEAD", ".git/index", ".git/config", "kept"]
        .into_iter()
        .zip(git)
    {
        assert_eq!(fs::read(fixture.repo().join(path)).unwrap(), expected);
    }
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn file_comments_v75_keyboard_personal_all_three_actual_queues_files_create_no_catalogue() {
    journey(false)
}
#[test]
fn file_comments_v75_mouse_personal_all_three_actual_queues_files_create_no_catalogue() {
    journey(true)
}
