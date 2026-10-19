//! SOURCE ONLY / UNRUN. Real binary + installed Git/SQLite/strict offline gh.
//! Eight paired journeys. No handler-driven positives or loose API fixtures.
use super::publication_v3 as durable;
use super::*;
#[derive(Clone, Copy)]
enum Command {
    All,
    Failed,
    Job,
}
impl Command {
    fn label(self) -> &'static str {
        match self {
            Self::All => "Rerun all jobs",
            Self::Failed => "Rerun failed jobs",
            Self::Job => "Rerun selected job",
        }
    }
    fn form(self) -> String {
        format!("{} • original Actions", self.label())
    }
    fn history(self) -> String {
        format!("Original {} publication", self.label())
    }
    fn endpoint(self) -> String {
        match self {
            Self::All => format!("repos/{CURRENT}/actions/runs/123/rerun"),
            Self::Failed => format!("repos/{CURRENT}/actions/runs/123/rerun-failed-jobs"),
            Self::Job => format!("repos/{CURRENT}/actions/jobs/501/rerun"),
        }
    }
    fn kind(self) -> &'static str {
        match self {
            Self::All => "rerunall",
            Self::Failed => "rerunfailed",
            Self::Job => "rerunjob",
        }
    }
}
fn phase(value: Value, uid: u64) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend((0..2).map(|_| get(&format!("repos/{CURRENT}/actions/runs/123"), value.clone())));
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend(principal(uid));
    assert_eq!(out.len(), 8);
    out
}
fn full_jobs() -> Vec<Value> {
    (1000..1100)
        .map(|id| {
            let mut job = fresh_job(CURRENT);
            job["id"] = json!(id);
            job["node_id"] = json!(format!("JOB_{id}"));
            job["check_run_url"] = json!(format!(
                "https://{HOST}/api/v3/repos/{CURRENT}/check-runs/{id}"
            ));
            job
        })
        .collect()
}
fn job_proof(second: bool, invalid: bool) -> Vec<GhStep> {
    let mut job = fresh_job(CURRENT);
    if invalid {
        job["run_attempt"] = Value::Null;
    }
    let mut out = vec![get(&format!("repos/{CURRENT}/actions/jobs/501"), job)];
    if invalid {
        return out;
    }
    out.push(get(
        &format!("repos/{CURRENT}/actions/runs/123/attempts/2"),
        fresh_run(CURRENT),
    ));
    out.push(get(&format!("repos/{CURRENT}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
        json!({"total_count":if second{101}else{1},"jobs":if second{full_jobs()}else{vec![fresh_job(CURRENT)]}})));
    if second {
        out.push(get(
            &format!("repos/{CURRENT}/actions/runs/123/attempts/2/jobs?per_page=100&page=2"),
            json!({"total_count":101,"jobs":[fresh_job(CURRENT)]}),
        ));
    }
    out
}
fn post(command: Command, debug: bool, fails: bool) -> GhStep {
    let mut out = GhStep::read(
        &[
            "api",
            &command.endpoint(),
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        "{}",
    );
    out.stdin = serde_json::to_vec(&json!({"enable_debug_logging":debug})).unwrap();
    if fails {
        out.status = 1;
        out.stderr = b"PRIVATE_RERUN_FAILURE_NEVER_TO_TTY".to_vec();
    }
    out
}
fn confirm(
    command: Command,
    debug: bool,
    second: bool,
    receipt: Value,
    fails: bool,
) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.push(get(
        &format!("repos/{CURRENT}/actions/runs/123"),
        fresh_run(CURRENT),
    ));
    out.extend(principal(42));
    out.push(get(
        &format!("repos/{CURRENT}/actions/runs/123"),
        fresh_run(CURRENT),
    ));
    if matches!(command, Command::Job) {
        out.extend(job_proof(second, false));
    }
    out.push(get(
        &format!("repos/{CURRENT}/actions/runs/123"),
        fresh_run(CURRENT),
    ));
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend(principal(42));
    out.push(post(command, debug, fails));
    if !fails {
        out.push(get(&format!("repos/{CURRENT}/actions/runs/123"), receipt));
        out.push(get("user", json!({"id":42,"login":"octocat"})));
    }
    let extra = if matches!(command, Command::Job) {
        if second {
            4
        } else {
            3
        }
    } else {
        0
    };
    assert_eq!(out.len(), if fails { 12 + extra } else { 14 + extra });
    out
}
fn opening() -> Vec<GhStep> {
    let mut out = actions_protocol();
    out.extend(operation(ReadKind::Run, 42, CURRENT));
    out.extend(phase(fresh_run(CURRENT), 42));
    out
}
fn cells(session: &mut Session, after: usize, title: &str, words: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after
            && session
                .completed_cells_screen()
                .and_then(|frame| crate::support::overlay_body(frame, title))
                .is_some_and(|body| words.iter().all(|word| body.contains(word)))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "No NEW complete owned focused/cursorless {title}: {words:?}\n{}",
            session.screen()
        );
    }
}
fn control(session: &mut Session, mouse: bool, title: &str, label: &str) {
    let history =
        label == "[View original]" || label == "[F4 Acknowledge]" || title.starts_with("Original ");
    if mouse {
        let p = session.wait_for_owned_completed_action(title, label, &[]);
        session.click(p.0, p.1);
    } else if label == "[F4 Acknowledge]" {
        session.wait_for_owned_completed_action(title, label, &[]);
        session.send_key(b"\x1bOS");
    } else if label == "[Preview]" {
        // Same visible Ctrl-P Preview/Edit action from the sole debug field.
        session.wait_for_owned_completed_action(title, label, &[]);
        session.send_key(b"\x10");
    } else {
        let mut controls = if history {
            vec![
                "[View original]",
                "[F4 Acknowledge]",
                "[Keep draft / Close]",
            ]
        } else {
            vec!["[Preview]", "[Confirm action]", "[Keep draft / Close]"]
        };
        if session.completed_cells_screen().is_some_and(|screen| {
            crate::support::overlay_body(screen, title)
                .is_some_and(|body| body.contains("[F7 Error↑]"))
        }) {
            controls.extend(["[F7 Error↑]", "[F8 Error↓]"]);
        }
        session.focus_completed_owned_action_v1(
            title,
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &controls,
            &[],
        );
        session.send_key(b"\r");
    }
}
fn command_control(session: &mut Session, mouse: bool, command: Command) {
    if mouse {
        resources_v2::action(session, true, command.label(), b"");
    } else {
        session.focus_completed_actions_control_v2(command.label());
        session.send_key(b"\r");
    }
}
fn initial(session: &mut Session, fixture: &Fixture, mouse: bool, command: Command) {
    session.resize(120, 40);
    session.wait_for_main_frame();
    open_run(session, fixture, mouse, 24, CURRENT);
    session.wait_for_cursorless_condition(
        |f| {
            f.contents()
                .contains("Original scoped Actions target verified")
                && !f.contents().contains("read pending")
        },
        "Genuine current source before command",
        Duration::from_secs(8),
    );
    if matches!(command, Command::Job) {
        jobs(session, mouse);
    }
    let after = session.completed_cells_end();
    command_control(session, mouse, command);
    session.wait_for_command_count(fixture, 32);
    cells(
        session,
        after,
        &command.form(),
        &[
            "ORIGINAL RERUN INTENT",
            "Enable debug logging?",
            "Numeric repository 10000",
            "Current repository:",
            "[Preview]",
            "[Confirm action]",
        ],
    );
}
fn debug(session: &mut Session, mouse: bool, command: Command, value: bool) {
    let after = session.completed_cells_end();
    if mouse {
        let label = if value { "[true]" } else { "[false]" };
        let p = session.wait_for_owned_completed_action(&command.form(), label, &[]);
        session.click(p.0, p.1);
    } else {
        // Preview's control retains focus on return to inputs. BackTab once
        // reaches the actual sole field. The completed Show marker proves
        // cursor visibility, NOT its later emitted coordinate; there is only
        // one field in this exact owned Form. Never use its stale coordinate.
        let title = command.form();
        let deadline = Instant::now() + Duration::from_secs(8);
        let field_focused = |screen: &vt100::Screen| {
            !screen.hide_cursor()
                && crate::support::completed_navigation_controls_v1::owned_action_position(
                    screen,
                    &title,
                    "Enable debug logging?:",
                    &[],
                )
                .is_some()
        };
        if !session.completed_cells_screen().is_some_and(&field_focused) {
            let focus_after = session.completed_cells_end();
            session.send_key(b"\x1b[Z");
            loop {
                session.poll_output(Duration::from_millis(1));
                if session.completed_cells_end() > focus_after
                    && session.completed_cells_screen().is_some_and(&field_focused)
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Actual single-field cursor visibility after ONE BackTab"
                );
            }
        }
        session.send_key(b" ");
    }
    let title = command.form();
    let wanted = if value { "true" } else { "false" };
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        let actual = session
            .completed_cells_screen()
            .and_then(|f| crate::support::overlay_body(f, &title));
        if session.completed_cells_end() > after
            && actual.is_some_and(|body| {
                let mut lines = body.lines();
                while let Some(line) = lines.next() {
                    if line.contains("Enable debug logging?") {
                        return lines
                            .next()
                            .is_some_and(|line| line.trim().trim_matches('│').trim() == wanted);
                    }
                }
                false
            })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Actual focused debug value {wanted}, not choices text\n{}",
            session.screen()
        );
    }
}
fn preview(session: &mut Session, fixture: &Fixture, mouse: bool, command: Command, count: usize) {
    let after = session.completed_cells_end();
    control(session, mouse, &command.form(), "[Preview]");
    session.wait_for_command_count(fixture, count);
    cells(
        session,
        after,
        &command.form(),
        &["RERUN PREVIEW", H, "[Confirm action]"],
    );
}
fn exact(session: &Session, fixture: &Fixture, steps: &[GhStep], posts: usize) {
    let rows = commands(session, fixture);
    assert_eq!(rows.len(), steps.len(), "Exact actual bounded child count");
    for (actual, expected) in rows.iter().zip(steps) {
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(actual["matched"], true);
        assert_eq!(
            actual["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(
        rows.iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST"))
            .count(),
        posts
    );
    assert!(!rows.iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| matches!(arg.as_str(), Some("PUT" | "PATCH" | "DELETE" | "graphql")))));
}
fn marker(fixture: &Fixture, command: Command, debug: bool, state: &str, receipt: bool) {
    durable::marker(fixture, state, receipt);
    let db = durable::raw(fixture);
    let (kind, options): (String, String) = db
        .query_row(
            "SELECT kind_json,options_json FROM actions_write_attempts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(
        kind.trim_matches('"').to_ascii_lowercase().replace('_', ""),
        command.kind()
    );
    let options: Value = serde_json::from_str(&options).unwrap();
    assert_eq!(options["debug_logging"], debug);
    let rows = durable::attempts(fixture);
    let context: Value = serde_json::from_str(&rows[0].3).unwrap();
    assert_eq!(
        context["job_id"],
        if matches!(command, Command::Job) {
            json!(501)
        } else {
            Value::Null
        }
    );
}
fn close(session: &mut Session, mouse: bool, command: Command) {
    let title = if session
        .completed_cells_screen()
        .is_some_and(|f| f.contents().contains(&command.history()))
    {
        command.history()
    } else {
        command.form()
    };
    if session
        .completed_cells_screen()
        .and_then(|f| crate::support::overlay_body(f, &title))
        .is_some()
    {
        let after = session.completed_cells_end();
        control(session, mouse, &title, "[Keep draft / Close]");
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            session.poll_output(Duration::from_millis(1));
            if session.completed_cells_end() > after
                && session
                    .completed_cells_screen()
                    .is_some_and(|f| crate::support::overlay_body(f, &title).is_none())
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Actual original Form closed\n{}",
                session.screen()
            );
        }
    }
}
#[allow(clippy::too_many_arguments)] // independent transport/storage/Git/config/TTY witnesses
fn finish(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    command: Command,
    steps: &[GhStep],
    posts: usize,
    git: &GitState,
    config: &[u8],
    before: &[(String, Vec<Vec<rusqlite::types::Value>>)],
) {
    close(session, mouse, command);
    assert_eq!(git_state(fixture), *git);
    assert_eq!(durable::unrelated_sql(fixture), before);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_private_intents_unchanged(fixture);
    assert_read_only_git(fixture);
    session.quit_and_assert_restored_terminal(mouse);
    exact(session, fixture, steps, posts);
    assert_eq!(git_state(fixture), *git);
}
fn accepted(mouse: bool, command: Command, with_debug: bool, second: bool) {
    let mut receipt = fresh_run(CURRENT);
    if with_debug {
        receipt["run_attempt"] = json!(3);
        receipt["status"] = json!("queued");
        receipt["conclusion"] = Value::Null;
    }
    let mut steps = opening();
    steps.extend(phase(fresh_run(CURRENT), 42));
    let run_confirm = confirm(command, with_debug, second, receipt, false);
    assert_eq!(run_confirm.len(), if second { 18 } else { 14 });
    steps.extend(run_confirm);
    if !with_debug {
        steps.extend(phase(fresh_run(CURRENT), 42));
    }
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, command);
    let sql = durable::unrelated_sql(&fixture);
    if with_debug {
        debug(&mut session, mouse, command, true);
    }
    preview(&mut session, &fixture, mouse, command, 40);
    let after = session.completed_cells_end();
    control(&mut session, mouse, &command.form(), "[Confirm action]");
    let count = if second { 58 } else { 54 };
    session.wait_for_command_count(&fixture, count);
    cells(
        &mut session,
        after,
        &command.history(),
        &[
            "GitHub accepted rerun",
            "completion NOT confirmed",
            "[View original]",
        ],
    );
    marker(&fixture, command, with_debug, "accepted_pending", true);
    assert!(!session.screen().contains("Original cancellation confirmed"));
    if !with_debug {
        let after = session.completed_cells_end();
        control(&mut session, mouse, &command.history(), "[View original]");
        session.wait_for_command_count(&fixture, count + 8);
        cells(
            &mut session,
            after,
            &command.history(),
            &["still pending", "[F4 Acknowledge]"],
        );
        let before = durable::attempts(&fixture);
        let after = session.completed_cells_end();
        control(&mut session, mouse, &command.history(), "[F4 Acknowledge]");
        cells(
            &mut session,
            after,
            &command.history(),
            &["Inspect a current original result"],
        );
        assert_eq!(durable::attempts(&fixture), before);
    }
    finish(
        &mut session,
        &fixture,
        mouse,
        command,
        &steps,
        1,
        &git,
        &config,
        &sql,
    );
}
fn invalid_job(mouse: bool) {
    let command = Command::Job;
    let mut tail = confirm(command, false, false, fresh_run(CURRENT), false);
    let mut invalid = fresh_job(CURRENT);
    invalid["run_attempt"] = Value::Null;
    tail[7].stdout = invalid.to_string().into_bytes();
    tail.truncate(8);
    let mut steps = opening();
    steps.extend(phase(fresh_run(CURRENT), 42));
    steps.extend(tail);
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, command);
    let sql = durable::unrelated_sql(&fixture);
    preview(&mut session, &fixture, mouse, command, 40);
    control(&mut session, mouse, &command.form(), "[Confirm action]");
    session.wait_for_command_count(&fixture, 48);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if !durable::attempts(&fixture).is_empty()
            && session.completed_cells_screen().is_some_and(|f| {
                let text = f.contents();
                text.contains("scope is fenced")
                    && !text.contains(H)
                    && crate::support::locate_control(f, "[Esc Cancel]").is_none()
            })
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Actual selected job refusal completed"
        );
    }
    marker(&fixture, command, false, "pending", false);
    finish(
        &mut session,
        &fixture,
        mouse,
        command,
        &steps,
        0,
        &git,
        &config,
        &sql,
    );
}
fn edit_tiny(mouse: bool) {
    let command = Command::All;
    let mut steps = opening();
    steps.extend(phase(fresh_run(CURRENT), 42));
    steps.extend(phase(fresh_run(CURRENT), 42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, command);
    let sql = durable::unrelated_sql(&fixture);
    preview(&mut session, &fixture, mouse, command, 40);
    let after = session.completed_cells_end();
    control(&mut session, mouse, &command.form(), "[Preview]");
    cells(
        &mut session,
        after,
        &command.form(),
        &["ORIGINAL RERUN INTENT", "Enable debug logging?"],
    );
    debug(&mut session, mouse, command, true);
    let after = session.completed_cells_end();
    control(&mut session, mouse, &command.form(), "[Confirm action]");
    cells(
        &mut session,
        after,
        &command.form(),
        &["Fresh useful exact original Preview"],
    );
    assert_eq!(commands(&session, &fixture).len(), 40);
    assert!(durable::attempts(&fixture).is_empty());
    preview(&mut session, &fixture, mouse, command, 48);
    let old = session.wait_for_owned_completed_action(&command.form(), "[Confirm action]", &[]);
    session.resize_owned_single_action_modal_v2(
        40,
        10,
        "Original Actions • resize required",
        "[Esc Close]",
    );
    assert!(!session.screen().contains("[Confirm action]"));
    if mouse {
        session.click(old.0, old.1);
    } else {
        session.send_key(b"\r");
    }
    assert_eq!(commands(&session, &fixture).len(), 48);
    assert!(durable::attempts(&fixture).is_empty());
    durable::resize_form(
        &mut session,
        120,
        40,
        &command.form(),
        &["[Preview]", "[Confirm action]", "[Keep draft / Close]"],
    );
    finish(
        &mut session,
        &fixture,
        mouse,
        command,
        &steps,
        0,
        &git,
        &config,
        &sql,
    );
}
fn privacy(mouse: bool) {
    for receipt_drift in [false, true] {
        let command = Command::Failed;
        let mut tail = confirm(command, false, false, fresh_run(CURRENT), false);
        if receipt_drift {
            let mut raw = fresh_run(CURRENT);
            raw["node_id"] = json!("OTHER_RUN_NODE");
            tail[12].stdout = raw.to_string().into_bytes();
            tail.truncate(13);
        } else {
            tail[10] = get("user", json!({"id":43,"login":"octocat"}));
            tail.truncate(11);
        }
        let mut steps = opening();
        steps.extend(phase(fresh_run(CURRENT), 42));
        steps.extend(tail);
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        initial(&mut session, &fixture, mouse, command);
        let sql = durable::unrelated_sql(&fixture);
        preview(&mut session, &fixture, mouse, command, 40);
        let after = session.completed_cursorless_end();
        control(&mut session, mouse, &command.form(), "[Confirm action]");
        session.wait_for_command_count(&fixture, steps.len());
        complete(&mut session, after, &["private source hidden"]);
        marker(
            &fixture,
            command,
            false,
            if receipt_drift {
                "uncertain"
            } else {
                "pending"
            },
            false,
        );
        let screen = session.screen();
        assert!(
            !screen.contains(H),
            "hidden private head SHA stays visible:\n{screen}"
        );
        assert!(!session.screen().contains("PRIVATE_RERUN"));
        finish(
            &mut session,
            &fixture,
            mouse,
            command,
            &steps,
            usize::from(receipt_drift),
            &git,
            &config,
            &sql,
        );
    }
}
fn cancel_held(mouse: bool) {
    for after_post in [false, true] {
        let command = Command::Job;
        let mut tail = confirm(command, false, false, fresh_run(CURRENT), false);
        let held = if after_post { 15 } else { 7 };
        tail[held].delay_ms = 60_000;
        tail.truncate(held + 1);
        let mut steps = opening();
        steps.extend(phase(fresh_run(CURRENT), 42));
        steps.extend(tail);
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        initial(&mut session, &fixture, mouse, command);
        let sql = durable::unrelated_sql(&fixture);
        preview(&mut session, &fixture, mouse, command, 40);
        control(&mut session, mouse, &command.form(), "[Confirm action]");
        let child = entered(&mut session, &fixture, steps.len());
        marker(&fixture, command, false, "pending", false);
        let p = session.wait_for_owned_completed_action(&command.form(), "[Esc Cancel]", &[]);
        if mouse {
            session.click(p.0, p.1);
        } else {
            session.send_key(b"\x1b");
        }
        gone(&mut session, &child);
        let deadline = Instant::now() + Duration::from_secs(8);
        loop {
            session.poll_output(Duration::from_millis(1));
            if durable::attempts(&fixture)[0].0 == if after_post { "uncertain" } else { "pending" }
                && session.completed_cells_screen().is_some_and(|f| {
                    f.contents().contains("scope is fenced") && !f.contents().contains(H)
                })
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "Actual child absent + protected result + private page fenced"
            );
        }
        marker(
            &fixture,
            command,
            false,
            if after_post { "uncertain" } else { "pending" },
            false,
        );
        finish(
            &mut session,
            &fixture,
            mouse,
            command,
            &steps,
            usize::from(after_post),
            &git,
            &config,
            &sql,
        );
    }
}
fn restart(mouse: bool) {
    let command = Command::Job;
    let mut steps = opening();
    steps.extend(phase(fresh_run(CURRENT), 42));
    steps.extend(confirm(command, true, false, fresh_run(CURRENT), true));
    let first = steps.len();
    assert_eq!(first, 55);
    steps.extend(actions_protocol());
    steps.extend(operation(ReadKind::Run, 42, CURRENT));
    steps.extend(phase(fresh_run(CURRENT), 42));
    steps.extend(phase(fresh_run(CURRENT), 42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, command);
    debug(&mut session, mouse, command, true);
    preview(&mut session, &fixture, mouse, command, 40);
    let after = session.completed_cursorless_end();
    control(&mut session, mouse, &command.form(), "[Confirm action]");
    session.wait_for_command_count(&fixture, first);
    complete(&mut session, after, &["private source hidden"]);
    marker(&fixture, command, true, "uncertain", false);
    let marker_before = durable::attempts(&fixture);
    session.terminate_abruptly();
    let mut session = launch(&fixture, mouse);
    choose(&mut session, mouse, "[: Commands]", b":");
    overlay_ready(
        &mut session,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    session.send_key(b"\x01");
    session.paste("Discover repositories");
    overlay_ready(
        &mut session,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(&mut session, mouse, "> Discover repositories", b"\r");
    session.wait_for_overlay_close("Commands · current context");
    session.wait_for_screen(TITLE);
    discover(&mut session, mouse);
    session.wait_for_command_count(&fixture, first + 7);
    session.wait_for_screen("Repository ID: 10000");
    filter(
        &mut session,
        mouse,
        "repo0000",
        FIRST,
        "Repository ID: 10000",
    );
    choose(&mut session, mouse, "[Open Actions]", b"A");
    session.wait_for_command_count(&fixture, first + 14);
    session.wait_for_overlay_close(TITLE);
    actions_ready(&mut session);
    session.resize(120, 40);
    session.wait_for_main_frame();
    open_run(&mut session, &fixture, mouse, first + 24, CURRENT);
    jobs(&mut session, mouse);
    let after = session.completed_cells_end();
    command_control(&mut session, mouse, command);
    session.wait_for_command_count(&fixture, first + 32);
    cells(
        &mut session,
        after,
        &command.history(),
        &[
            "Outcome unknown",
            "RerunJob",
            "501",
            "debug",
            "[F4 Acknowledge]",
        ],
    );
    assert_eq!(durable::attempts(&fixture), marker_before);
    let sql = durable::unrelated_sql(&fixture);
    let after = session.completed_cells_end();
    control(&mut session, mouse, &command.history(), "[F4 Acknowledge]");
    session.wait_for_command_count(&fixture, first + 40);
    cells(
        &mut session,
        after,
        &command.history(),
        &["acknowledged", "no remote mutation"],
    );
    marker(&fixture, command, true, "acknowledged", false);
    finish(
        &mut session,
        &fixture,
        mouse,
        command,
        &steps,
        1,
        &git,
        &config,
        &sql,
    );
}
#[test]
fn scoped_actions_rerun_v4_keyboard_all_false_accepted_still_pending_no_false_completion() {
    accepted(false, Command::All, false, false);
}
#[test]
fn scoped_actions_rerun_v4_mouse_all_false_accepted_still_pending_no_false_completion() {
    accepted(true, Command::All, false, false);
}
#[test]
fn scoped_actions_rerun_v4_keyboard_failed_true_new_attempt_not_success() {
    accepted(false, Command::Failed, true, false);
}
#[test]
fn scoped_actions_rerun_v4_mouse_failed_true_new_attempt_not_success() {
    accepted(true, Command::Failed, true, false);
}
#[test]
fn scoped_actions_rerun_v4_keyboard_selected_job_omitted_attempt_page2_original501_not_url600() {
    accepted(false, Command::Job, false, true);
}
#[test]
fn scoped_actions_rerun_v4_mouse_selected_job_omitted_attempt_page2_original501_not_url600() {
    accepted(true, Command::Job, false, true);
}
#[test]
fn scoped_actions_rerun_v4_keyboard_explicit_null_job_attempt_zero_post_protected_claim() {
    invalid_job(false);
}
#[test]
fn scoped_actions_rerun_v4_mouse_explicit_null_job_attempt_zero_post_protected_claim() {
    invalid_job(true);
}
#[test]
fn scoped_actions_rerun_v4_keyboard_debug_edit_retires_preview_tiny_has_no_confirm() {
    edit_tiny(false);
}
#[test]
fn scoped_actions_rerun_v4_mouse_debug_edit_retires_preview_tiny_has_no_confirm() {
    edit_tiny(true);
}
#[test]
fn scoped_actions_rerun_v4_keyboard_final_uid_raw_receipt_drift_hide_private_no_resend() {
    privacy(false);
}
#[test]
fn scoped_actions_rerun_v4_mouse_final_uid_raw_receipt_drift_hide_private_no_resend() {
    privacy(true);
}
#[test]
fn scoped_actions_rerun_v4_keyboard_held_member_or_receipt_cancel_actual_pid_gone() {
    cancel_held(false);
}
#[test]
fn scoped_actions_rerun_v4_mouse_held_member_or_receipt_cancel_actual_pid_gone() {
    cancel_held(true);
}
#[test]
fn scoped_actions_rerun_v4_keyboard_restart_original_job_debug_history_ack_no_resend() {
    restart(false);
}
#[test]
fn scoped_actions_rerun_v4_mouse_restart_original_job_debug_history_ack_no_resend() {
    restart(true);
}
