//! SOURCE ONLY / UNRUN. Eight paired actual-binary journeys. Installed Git,
//! strict gh, real SQLite/restart, exact positive child identity and TTY return.
use super::publication_v3 as durable;
use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    Force,
    Delete,
    Logs,
}
impl Command {
    fn label(self) -> &'static str {
        match self {
            Self::Force => "Force-cancel run",
            Self::Delete => "Delete run",
            Self::Logs => "Delete run logs",
        }
    }
    fn form(self) -> String {
        format!("{} • original Actions", self.label())
    }
    fn history(self) -> String {
        format!("Original {} publication", self.label())
    }
    fn endpoint(self) -> String {
        format!(
            "repos/{CURRENT}/actions/runs/123{}",
            match self {
                Self::Force => "/force-cancel",
                Self::Delete => "",
                Self::Logs => "/logs",
            }
        )
    }
    fn kind(self) -> &'static str {
        match self {
            Self::Force => "forcecancel",
            Self::Delete => "deleterun",
            Self::Logs => "deletelogs",
        }
    }
}
fn running() -> Value {
    let mut value = fresh_run(CURRENT);
    value["status"] = json!("in_progress");
    value["conclusion"] = Value::Null;
    value
}
fn phase(uid: u64) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend((0..2).map(|_| get(&format!("repos/{CURRENT}/actions/runs/123"), running())));
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend(principal(uid));
    assert_eq!(out.len(), 8);
    out
}
fn open_read() -> Vec<GhStep> {
    let mut out = operation(ReadKind::Run, 42, CURRENT);
    for step in &mut out {
        if step
            .args
            .get(1)
            .is_some_and(|path| path.ends_with("/runs/123") || path.ends_with("/attempts/2"))
        {
            step.stdout = running().to_string().into_bytes();
        }
    }
    out
}
fn opening() -> Vec<GhStep> {
    let mut out = actions_protocol();
    out.extend(open_read());
    out.extend(phase(42));
    out
}
fn ack(command: Command) -> GhStep {
    GhStep::read(
        &[
            "api",
            &command.endpoint(),
            "--hostname",
            HOST,
            "--method",
            if command == Command::Force {
                "POST"
            } else {
                "DELETE"
            },
            "--include",
        ],
        &format!(
            "HTTP/2.0 {} Accepted\nContent-Length: 0\r\n\r\n",
            if command == Command::Force { 202 } else { 204 }
        ),
    )
}
fn missing() -> GhStep {
    let mut step = get(&format!("repos/{CURRENT}/actions/runs/123"), json!({}));
    step.status = 1;
    step.stdout.clear();
    step.stderr = b"gh: Not Found (HTTP 404)".to_vec();
    step
}
fn confirm(command: Command, unavailable: bool) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.push(get(&format!("repos/{CURRENT}/actions/runs/123"), running()));
    out.extend(principal(42));
    out.extend((0..2).map(|_| get(&format!("repos/{CURRENT}/actions/runs/123"), running())));
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend(principal(42));
    out.push(ack(command));
    out.push(if unavailable {
        missing()
    } else {
        get(&format!("repos/{CURRENT}/actions/runs/123"), running())
    });
    if command == Command::Delete {
        out.push(get(&format!("repos/{CURRENT}"), current_repository()));
    }
    out.push(get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(out.len(), if command == Command::Delete { 15 } else { 14 });
    out
}
fn history_phase(selected: bool) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    if selected {
        out.push(missing());
    }
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend(principal(42));
    assert_eq!(out.len(), if selected { 7 } else { 6 });
    out
}
fn cells(session: &mut Session, after: usize, title: &str, words: &[&str]) {
    let deadline = Instant::now() + Duration::from_secs(8);
    let compact = |value: &str| {
        value
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after
            && session
                .completed_cells_screen()
                .and_then(|frame| crate::support::overlay_body(frame, title))
                .is_some_and(|body| {
                    let body = compact(&body);
                    words.iter().all(|word| body.contains(&compact(word)))
                })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "NEW actual owned {title} {words:?}\n{}",
            session.screen()
        );
    }
}
fn control(session: &mut Session, mouse: bool, title: &str, label: &str) {
    if mouse {
        let (x, y) = session.wait_for_owned_completed_action(title, label, &[]);
        session.click(x, y);
        return;
    }
    session.wait_for_owned_completed_action(title, label, &[]);
    if label == "[F4 Acknowledge]" {
        session.send_key(b"\x1bOS");
        return;
    }
    if label == "[F6 ACK receipt]" {
        session.send_key(b"\x1b[17~");
        return;
    }
    let body = session
        .completed_cells_screen()
        .and_then(|frame| crate::support::overlay_body(frame, title))
        .expect("actual owned modal");
    let mut controls = if body.contains("[View original]") {
        vec!["[View original]", "[F4 Acknowledge]"]
    } else {
        vec!["[Preview]", "[Confirm action]"]
    };
    if body.contains("[F6 ACK receipt]") {
        controls.push("[F6 ACK receipt]");
    }
    controls.push(if body.contains("[Esc Cancel]") {
        "[Esc Cancel]"
    } else {
        "[Keep draft / Close]"
    });
    if body.contains("[F7 Error↑]") {
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
fn scroll_to(session: &mut Session, mouse: bool, title: &str, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    let compact = |value: &str| {
        value
            .chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    let expected = compact(needle);
    let mut last_scroll = None;
    loop {
        session.poll_output(Duration::from_millis(1));
        if last_scroll.is_some_and(|end| session.completed_cells_end() <= end) {
            assert!(
                Instant::now() < deadline,
                "Original history scroll did not repaint"
            );
            continue;
        }
        if session
            .completed_cells_screen()
            .and_then(|frame| crate::support::overlay_body(frame, title))
            .is_some_and(|body| compact(&body).contains(&expected))
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Original owned history must visibly show {needle}\n{}",
            session.screen()
        );
        let point = session.wait_for_owned_completed_action(title, "[Keep draft / Close]", &[]);
        last_scroll = Some(session.completed_cells_end());
        if mouse {
            session.wheel(point.0, point.1.saturating_sub(3), true);
        } else {
            session.send_key(b"\x1b[6~");
        }
    }
}
fn command(session: &mut Session, mouse: bool, command: Command) {
    // Match a complete actual button: the bare Delete run prefix also occurs
    // in the distinct Delete run logs action and is deliberately ambiguous.
    let label = format!("[{}]", command.label());
    if mouse {
        resources_v2::action(session, true, &label, b"");
    } else {
        session.focus_completed_actions_control_v2(&label);
        session.send_key(b"\r");
    }
}
fn initial(session: &mut Session, fixture: &Fixture, mouse: bool, command_kind: Command) {
    session.resize(120, 40);
    session.wait_for_main_frame();
    open_run(session, fixture, mouse, 24, CURRENT);
    session.wait_for_cursorless_condition(
        |frame| {
            frame
                .contents()
                .contains("Original scoped Actions target verified")
                && !frame.contents().contains("read pending")
        },
        "Actual exact run before control",
        Duration::from_secs(8),
    );
    let after = session.completed_cells_end();
    command(session, mouse, command_kind);
    session.wait_for_command_count(fixture, 32);
    cells(
        session,
        after,
        &command_kind.form(),
        &[
            "ORIGINAL WHOLE RUN",
            "Numeric repository 10000",
            "[Preview]",
            "[Confirm action]",
        ],
    );
}
fn force_choice(session: &mut Session, mouse: bool) {
    let title = Command::Force.form();
    let after = session.completed_cells_end();
    if mouse {
        let (x, y) = session.wait_for_owned_completed_action(&title, "[true]", &[]);
        session.click(x, y);
    } else {
        // A completed visible-cursor frame with this sole actual field proves
        // field focus, not a later cursor coordinate. Preview retains footer
        // focus, so exactly ONE BackTab returns to the actual input first.
        let field_focused = |frame: &vt100::Screen| {
            !frame.hide_cursor()
                && crate::support::overlay_body(frame, &title)
                    .is_some_and(|body| body.contains("Escalate force cancellation"))
        };
        if !session.completed_cells_screen().is_some_and(&field_focused) {
            let focus_after = session.completed_cells_end();
            session.send_key(b"\x1b[Z");
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                session.poll_output(Duration::from_millis(1));
                if session.completed_cells_end() > focus_after
                    && session.completed_cells_screen().is_some_and(&field_focused)
                {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Actual one-field force input visible cursor after ONE BackTab"
                );
            }
        }
        session.send_key(b" ");
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after
            && session
                .completed_cells_screen()
                .and_then(|frame| crate::support::overlay_body(frame, &title))
                .is_some_and(|body| {
                    let mut lines = body.lines();
                    while let Some(line) = lines.next() {
                        if line.contains("Escalate force cancellation") {
                            return lines.next().is_some_and(|value| {
                                value.trim().trim_matches('│').trim() == "true"
                            });
                        }
                    }
                    false
                })
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Actual force value true, not choices list\n{}",
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
        &["PREVIEW", H, "[Confirm action]"],
    );
}
fn close(session: &mut Session, mouse: bool, command: Command) {
    let title = if session
        .completed_cells_screen()
        .is_some_and(|frame| frame.contents().contains(&command.history()))
    {
        command.history()
    } else {
        command.form()
    };
    if session
        .completed_cells_screen()
        .and_then(|frame| crate::support::overlay_body(frame, &title))
        .is_some()
    {
        let after = session.completed_cells_end();
        control(session, mouse, &title, "[Keep draft / Close]");
        session.wait_for_new_cursorless_condition(
            after,
            |frame| crate::support::overlay_body(frame, &title).is_none(),
            "Actual original Form closed, no resend",
            Duration::from_secs(8),
        );
    }
}
fn exact(session: &Session, fixture: &Fixture, steps: &[GhStep], writes: usize) {
    let actual = commands(session, fixture);
    assert_eq!(
        actual.len(),
        steps.len(),
        "Exact original measured/prepaid children, no fallback"
    );
    for (actual, expected) in actual.iter().zip(steps) {
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(actual["matched"], true);
        assert_eq!(
            actual["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(
        actual
            .iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST" || arg == "DELETE"))
            .count(),
        writes
    );
    assert!(!actual.iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| matches!(arg.as_str(), Some("PATCH" | "PUT" | "graphql")))));
}
fn marker(fixture: &Fixture, command: Command, state: &str) {
    let db = durable::raw(fixture);
    let (kind,actual,context,receipt):(String,String,String,Option<String>)=db.query_row(
        "SELECT kind_json,state,context_json,receipt_json FROM actions_write_attempts ORDER BY attempt_id DESC LIMIT 1",[],
        |row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
    assert_eq!(
        kind.trim_matches('"').to_ascii_lowercase().replace('_', ""),
        command.kind()
    );
    assert_eq!(actual, state);
    let context: Value = serde_json::from_str(&context).unwrap();
    assert_eq!(context["run_id"], 123);
    assert_eq!(context["attempt"], 2);
    assert_eq!(context["head_sha"], H);
    assert_eq!(context["original_repository_v3"]["repository_id"], 10000);
    assert_eq!(context["original_repository_v3"]["original_name"], FIRST);
    assert_eq!(context["original_run_v3"]["run_node"], "WR_123");
    assert_eq!(context["original_run_v3"]["workflow_id"], 7);
    assert_eq!(context["original_run_v3"]["check_suite_id"], 901);
    assert!(context["original_pr_v3"].is_null());
    assert_eq!(
        receipt.is_some(),
        matches!(state, "accepted_pending" | "confirmed" | "acknowledged")
    );
}
#[allow(clippy::too_many_arguments)]
fn finish(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    command: Command,
    steps: &[GhStep],
    writes: usize,
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
    exact(session, fixture, steps, writes);
    assert_eq!(git_state(fixture), *git);
}
fn force(mouse: bool) {
    let mut steps = opening();
    steps.extend(phase(42));
    steps.extend(confirm(Command::Force, false));
    steps.extend(phase(42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, Command::Force);
    let before = durable::unrelated_sql(&fixture);
    // A real false default cannot even renew/read Preview, let alone POST.
    let after = session.completed_cells_end();
    control(&mut session, mouse, &Command::Force.form(), "[Preview]");
    cells(
        &mut session,
        after,
        &Command::Force.form(),
        &["Choose force_ack=true", "[Preview]"],
    );
    exact(&session, &fixture, &steps[..32], 0);
    force_choice(&mut session, mouse);
    preview(&mut session, &fixture, mouse, Command::Force, 40);
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        &Command::Force.form(),
        "[Confirm action]",
    );
    session.wait_for_command_count(&fixture, 54);
    cells(
        &mut session,
        after,
        &Command::Force.history(),
        &["completion NOT confirmed", "[View original]"],
    );
    marker(&fixture, Command::Force, "accepted_pending");
    let resized_after = session.completed_cells_end();
    session.resize(80, 24);
    cells(
        &mut session,
        resized_after,
        &Command::Force.history(),
        &[
            "[View original]",
            "[F4 Acknowledge]",
            "[Keep draft / Close]",
            "[F7 Error↑]",
            "[F8 Error↓]",
        ],
    );
    assert_eq!(session.completed_cells_screen().unwrap().size(), (24, 80));
    for label in [
        "[View original]",
        "[F4 Acknowledge]",
        "[Keep draft / Close]",
        "[F7 Error↑]",
        "[F8 Error↓]",
    ] {
        session.wait_for_owned_completed_action(&Command::Force.history(), label, &[]);
    }
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        &Command::Force.history(),
        "[View original]",
    );
    session.wait_for_command_count(&fixture, 62);
    scroll_to(
        &mut session,
        mouse,
        &Command::Force.history(),
        "still pending",
    );
    cells(
        &mut session,
        after,
        &Command::Force.history(),
        &["still pending", "[F4 Acknowledge]"],
    );
    let saved = durable::attempts(&fixture);
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        &Command::Force.history(),
        "[F4 Acknowledge]",
    );
    cells(
        &mut session,
        after,
        &Command::Force.history(),
        &["Inspect a current original result"],
    );
    assert_eq!(durable::attempts(&fixture), saved);
    finish(
        &mut session,
        &fixture,
        mouse,
        Command::Force,
        &steps,
        1,
        &git,
        &config,
        &before,
    );
}
fn escalation(mouse: bool) {
    let mut cancel = confirm(Command::Force, false);
    cancel[11] = GhStep::read(
        &[
            "api",
            &format!("repos/{CURRENT}/actions/runs/123/cancel"),
            "--hostname",
            HOST,
            "--method",
            "POST",
        ],
        "{}",
    );
    let mut steps = opening();
    steps.extend(phase(42));
    steps.extend(cancel);
    steps.extend(open_read());
    steps.extend(phase(42));
    steps.extend(phase(42));
    steps.extend(confirm(Command::Force, false));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    session.resize(120, 40);
    session.wait_for_main_frame();
    open_run(&mut session, &fixture, mouse, 24, CURRENT);
    let after = session.completed_cells_end();
    resources_v2::action(&mut session, mouse, "[C Cancel / history]", b"C");
    session.wait_for_command_count(&fixture, 32);
    cells(
        &mut session,
        after,
        "Cancel original Actions run",
        &["[Preview]", "[Confirm action]"],
    );
    let before = durable::unrelated_sql(&fixture);
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        "Cancel original Actions run",
        "[Preview]",
    );
    session.wait_for_command_count(&fixture, 40);
    cells(
        &mut session,
        after,
        "Cancel original Actions run",
        &["CANCEL PREVIEW"],
    );
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        "Cancel original Actions run",
        "[Confirm action]",
    );
    session.wait_for_command_count(&fixture, 54);
    cells(
        &mut session,
        after,
        "Original Cancel publication",
        &["completion NOT confirmed", "[View original]"],
    );
    let old = durable::attempts(&fixture);
    assert_eq!(old.len(), 1);
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        "Original Cancel publication",
        "[Keep draft / Close]",
    );
    complete(&mut session, after, &["Composer closed"]);
    let after = session.completed_cursorless_end();
    resources_v2::action(&mut session, mouse, "Refresh actual run", b"r");
    session.wait_for_command_count(&fixture, 64);
    complete(&mut session, after, &["Run 123", "in_progress"]);
    let after = session.completed_cells_end();
    command(&mut session, mouse, Command::Force);
    session.wait_for_command_count(&fixture, 72);
    cells(
        &mut session,
        after,
        &Command::Force.form(),
        &["old Cancel remains unchanged/protected", "[Preview]"],
    );
    force_choice(&mut session, mouse);
    preview(&mut session, &fixture, mouse, Command::Force, 80);
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        &Command::Force.form(),
        "[Confirm action]",
    );
    session.wait_for_command_count(&fixture, 94);
    cells(
        &mut session,
        after,
        &Command::Force.history(),
        &["completion NOT confirmed", "[View original]"],
    );
    let all = durable::attempts(&fixture);
    assert_eq!(all.len(), 2);
    assert_eq!(all[0], old[0]);
    marker(&fixture, Command::Force, "accepted_pending");
    let context: Value = serde_json::from_str(&all[1].3).unwrap();
    assert_eq!(context["force_escalation_v5"]["cancel"]["attempt_id"], 1);
    finish(
        &mut session,
        &fixture,
        mouse,
        Command::Force,
        &steps,
        2,
        &git,
        &config,
        &before,
    );
}
fn reopened_catalogue_ready_v141(session: &mut Session, after: usize) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(2));
        if session.completed_cells_end() > after
            && session.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                !text.contains(SOURCE)
                    && !text.contains("Commands · current context")
                    && !text.contains("Original worker pending")
                    && crate::support::overlay_body(frame, TITLE).is_some_and(|body| {
                        [
                            "Observed account #42",
                            FIRST,
                            "Repository ID: 10000",
                            "Filter focused:",
                            "Selected: no • Pinned: no",
                            "[/ Filter]",
                            "[Space Select]",
                            "[p Pin]",
                            "[Open Actions]",
                            "[q Quit]",
                            "[Esc Close]",
                        ]
                        .iter()
                        .all(|label| body.contains(label))
                    })
            })
        {
            return;
        }
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "NEW completed owned repository inventory required before Filter input\n{}",
                session.screen()
            );
        }
    }
}
fn reopen_empty(session: &mut Session, fixture: &Fixture, mouse: bool, offset: usize) {
    session.resize(120, 40);
    session.wait_for_main_frame();
    choose(session, mouse, "[: Commands]", b":");
    overlay_ready(
        session,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    session.send_key(b"\x01");
    session.paste("Discover repositories");
    overlay_ready(
        session,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(session, mouse, "> Discover repositories", b"\r");
    session.wait_for_overlay_close("Commands · current context");
    ready(session, &[TITLE, "[s Source]"]);
    let before_discover = session.completed_cells_end();
    discover(session, mouse);
    session.wait_for_command_count(fixture, offset + 7);
    // A command entry proves child invocation, not Loaded publication. The
    // pending shell correctly ignores paste; wait for its real successor box.
    // Query focus intentionally displays the cursor, so use completed CELLS.
    reopened_catalogue_ready_v141(session, before_discover);
    filter(session, mouse, "repo0000", FIRST, "Repository ID: 10000");
    choose(session, mouse, "[Open Actions]", b"A");
    session.wait_for_command_count(fixture, offset + 14);
    session.wait_for_overlay_close(TITLE);
    complete(
        session,
        0,
        &["Actions • focused view", "0 runs of 0 reported"],
    );
}
fn publications_from_empty_runs(session: &mut Session, mouse: bool) {
    // The same title also occurs in this read's status line. Include its real
    // border so the owning box is unique; do not weaken the shared parser.
    const TITLE: &str = "Actions • focused view ─";
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut previous = None;
    for _ in 0..30 {
        let (page, publication, next) = loop {
            session.poll_output(Duration::from_millis(1));
            let observed = session.completed_cells_screen().and_then(|frame| {
                let body = crate::support::overlay_body(frame, TITLE)?;
                if !body.contains("0 runs of 0 reported • complete")
                    || !body.contains("No records reported in this scope.")
                    || !body.contains(CURRENT)
                    || body.contains("[b Cancel read]")
                    || body.contains("Original current read pending")
                {
                    return None;
                }
                let row = body
                    .lines()
                    .find(|line| line.contains(" • [] change control page"))?;
                let token = row
                    .split(" • [] change control page")
                    .next()?
                    .split_whitespace()
                    .last()?;
                let (current, total) = token.split_once('/')?;
                let page = (current.parse::<usize>().ok()?, total.parse::<usize>().ok()?);
                if page.0 == 0 || page.0 > page.1 || page.1 > 30 {
                    return None;
                }
                let point = |label| {
                    crate::support::completed_navigation_controls_v1::owned_action_position(
                        frame,
                        TITLE,
                        label,
                        &["Action unavailable", "Commands · current context"],
                    )
                };
                Some((page, point("[P Publications]"), point("[Controls →]")?))
            });
            if let Some(observed) = observed {
                if previous.is_none_or(|(end, page)| {
                    session.completed_cells_end() > end && observed.0 != page
                }) {
                    break observed;
                }
            }
            assert!(
                Instant::now() < deadline,
                "No NEW completed owned empty-Runs controls page\n{}",
                session.screen()
            );
        };
        if let Some((x, y)) = publication {
            if mouse {
                session.click(x, y);
            } else {
                session.send_key(b"P");
            }
            return;
        }
        assert!(
            page.0 < page.1 && Instant::now() < deadline,
            "Actual empty-Runs pages must expose Publications\n{}",
            session.screen()
        );
        previous = Some((session.completed_cells_end(), page));
        if mouse {
            session.click(next.0, next.1);
        } else {
            session.send_key(b"]");
        }
    }
    panic!("Publications not reached within30 actual empty-Runs pages");
}
fn delete_restart(mouse: bool) {
    let mut steps = opening();
    steps.extend(phase(42));
    steps.extend(confirm(Command::Delete, true));
    let first = steps.len();
    assert_eq!(first, 55);
    let mut second = actions_protocol();
    second[10].stdout = json!({"total_count":0,"workflow_runs":[]})
        .to_string()
        .into_bytes();
    steps.extend(second);
    steps.extend(history_phase(false));
    steps.extend(history_phase(true));
    steps.extend(history_phase(true));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, Command::Delete);
    let before = durable::unrelated_sql(&fixture);
    preview(&mut session, &fixture, mouse, Command::Delete, 40);
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        &Command::Delete.form(),
        "[Confirm action]",
    );
    session.wait_for_command_count(&fixture, 55);
    cells(
        &mut session,
        after,
        &Command::Delete.history(),
        &[
            "HTTP204",
            "deletion/causality NOT confirmed",
            "[View original]",
        ],
    );
    marker(&fixture, Command::Delete, "accepted_pending");
    close(&mut session, mouse, Command::Delete);
    session.quit_and_assert_restored_terminal(mouse);
    exact(&session, &fixture, &steps[..55], 1);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(durable::unrelated_sql(&fixture), before);
    let receipt = durable::attempts(&fixture)[0].2.clone();
    let mut session = launch(&fixture, mouse);
    reopen_empty(&mut session, &fixture, mouse, first);
    let before = durable::unrelated_sql(&fixture);
    let after = session.completed_cursorless_end();
    publications_from_empty_runs(&mut session, mouse);
    session.wait_for_command_count(&fixture, 75);
    complete(
        &mut session,
        after,
        &["Original Actions publications", "Original #1", "DeleteRun"],
    );
    selected_row(&mut session, mouse, "Original #1");
    session.wait_for_command_count(&fixture, 82);
    complete(
        &mut session,
        after,
        &[
            "Inspect original Actions publication",
            "NOT proof of deletion",
            "EFFECT UNVERIFIED",
        ],
    );
    let before_ack = durable::attempts(&fixture);
    assert_eq!(before_ack[0].0, "accepted_pending");
    let after = session.completed_cursorless_end();
    let label = "Acknowledge inspected acceptance (effect unverified)";
    if mouse {
        resources_v2::action(&mut session, true, label, b"");
    } else {
        session.focus_completed_actions_control_v2(label);
        session.send_key(b"\r");
    }
    session.wait_for_command_count(&fixture, 89);
    complete(
        &mut session,
        after,
        &["acceptance ACK committed locally", "effect UNVERIFIED"],
    );
    marker(&fixture, Command::Delete, "acknowledged");
    assert_eq!(durable::attempts(&fixture)[0].2, receipt);
    finish(
        &mut session,
        &fixture,
        mouse,
        Command::Delete,
        &steps,
        1,
        &git,
        &config,
        &before,
    );
}
fn logs_ack(mouse: bool) {
    let mut steps = opening();
    steps.extend(phase(42));
    steps.extend(confirm(Command::Logs, false));
    steps.extend(phase(42));
    steps.extend(phase(42));
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, Command::Logs);
    let before = durable::unrelated_sql(&fixture);
    preview(&mut session, &fixture, mouse, Command::Logs, 40);
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        &Command::Logs.form(),
        "[Confirm action]",
    );
    session.wait_for_command_count(&fixture, 54);
    cells(
        &mut session,
        after,
        &Command::Logs.history(),
        &["effect UNVERIFIED", "[View original]"],
    );
    marker(&fixture, Command::Logs, "accepted_pending");
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        &Command::Logs.history(),
        "[View original]",
    );
    session.wait_for_command_count(&fixture, 62);
    cells(
        &mut session,
        after,
        &Command::Logs.history(),
        &["[F6 ACK receipt]", "UNVERIFIED"],
    );
    scroll_to(
        &mut session,
        mouse,
        &Command::Logs.history(),
        "F4 does not confirm deletion; separate F6 acknowledges only inspected typed acceptance.",
    );
    let saved = durable::attempts(&fixture);
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        &Command::Logs.history(),
        "[F4 Acknowledge]",
    );
    cells(
        &mut session,
        after,
        &Command::Logs.history(),
        &["Inspect a current original result"],
    );
    assert_eq!(durable::attempts(&fixture), saved);
    let after = session.completed_cells_end();
    control(
        &mut session,
        mouse,
        &Command::Logs.history(),
        "[F6 ACK receipt]",
    );
    session.wait_for_command_count(&fixture, 70);
    cells(
        &mut session,
        after,
        &Command::Logs.history(),
        &["acknowledged", "effect UNVERIFIED", "no remote mutation"],
    );
    marker(&fixture, Command::Logs, "acknowledged");
    assert_eq!(durable::attempts(&fixture)[0].2, saved[0].2);
    finish(
        &mut session,
        &fixture,
        mouse,
        Command::Logs,
        &steps,
        1,
        &git,
        &config,
        &before,
    );
}
fn forbidden(mouse: bool) {
    let mut tail = confirm(Command::Delete, true);
    tail[12].stderr = b"gh: Resource not accessible (HTTP 403)".to_vec();
    tail.truncate(13);
    let mut steps = opening();
    steps.extend(phase(42));
    steps.extend(tail);
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, Command::Delete);
    let before = durable::unrelated_sql(&fixture);
    preview(&mut session, &fixture, mouse, Command::Delete, 40);
    let after = session.completed_cursorless_end();
    control(
        &mut session,
        mouse,
        &Command::Delete.form(),
        "[Confirm action]",
    );
    session.wait_for_command_count(&fixture, 53);
    complete(&mut session, after, &["private source hidden"]);
    assert!(!session.screen().contains(H));
    marker(&fixture, Command::Delete, "uncertain");
    finish(
        &mut session,
        &fixture,
        mouse,
        Command::Delete,
        &steps,
        1,
        &git,
        &config,
        &before,
    );
}
fn changed_uid(mouse: bool) {
    let mut tail = confirm(Command::Force, false);
    tail[10] = get("user", json!({"id":43,"login":"octocat"}));
    tail.truncate(11);
    let mut steps = opening();
    steps.extend(phase(42));
    steps.extend(tail);
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, Command::Force);
    let before = durable::unrelated_sql(&fixture);
    force_choice(&mut session, mouse);
    preview(&mut session, &fixture, mouse, Command::Force, 40);
    let after = session.completed_cursorless_end();
    control(
        &mut session,
        mouse,
        &Command::Force.form(),
        "[Confirm action]",
    );
    session.wait_for_command_count(&fixture, 51);
    complete(&mut session, after, &["private source hidden"]);
    assert!(!session.screen().contains(H));
    marker(&fixture, Command::Force, "pending");
    finish(
        &mut session,
        &fixture,
        mouse,
        Command::Force,
        &steps,
        0,
        &git,
        &config,
        &before,
    );
}
fn held(mouse: bool) {
    let mut tail = confirm(Command::Logs, false);
    tail[12].delay_ms = 60_000;
    tail.truncate(13);
    let mut steps = opening();
    steps.extend(phase(42));
    steps.extend(tail);
    let fixture = configured_native(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = start(&fixture, mouse);
    initial(&mut session, &fixture, mouse, Command::Logs);
    let before = durable::unrelated_sql(&fixture);
    preview(&mut session, &fixture, mouse, Command::Logs, 40);
    control(
        &mut session,
        mouse,
        &Command::Logs.form(),
        "[Confirm action]",
    );
    let child = entered(&mut session, &fixture, 53);
    let after = session.completed_cursorless_end();
    if mouse {
        let (x, y) =
            session.wait_for_owned_completed_action(&Command::Logs.form(), "[Esc Cancel]", &[]);
        session.click(x, y);
    } else {
        session.send_key(b"\x1b");
    }
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            text.contains("scope is fenced")
                && text.contains("private pages hidden")
                && !text.contains(&Command::Logs.form())
                && !text.contains(&Command::Logs.history())
                && !text.contains(H)
                && !text.contains("@alice")
                && crate::support::main_footer_visible(frame)
        },
        "Actual owned writer cancellation hides private source with usable main footer",
        Duration::from_secs(8),
    );
    gone(&mut session, &child);
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        if durable::attempts(&fixture)[0].0 == "uncertain" {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "actual entered writer must settle protected uncertain"
        );
        session.poll_output(Duration::from_millis(10));
    }
    marker(&fixture, Command::Logs, "uncertain");
    finish(
        &mut session,
        &fixture,
        mouse,
        Command::Logs,
        &steps,
        1,
        &git,
        &config,
        &before,
    );
}
fn tiny(mouse: bool) {
    for kind in [Command::Force, Command::Delete, Command::Logs] {
        let mut steps = opening();
        steps.extend(phase(42));
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        initial(&mut session, &fixture, mouse, kind);
        let before = durable::unrelated_sql(&fixture);
        if kind == Command::Force {
            force_choice(&mut session, mouse);
        }
        preview(&mut session, &fixture, mouse, kind, 40);
        let old = session.wait_for_owned_completed_action(&kind.form(), "[Confirm action]", &[]);
        session.resize_owned_single_action_modal_v2(
            40,
            10,
            "Original Actions • resize required",
            "[Esc Close]",
        );
        let after = session.completed_cursorless_end();
        if mouse {
            session.click(old.0, old.1);
        } else {
            session.send_key(b"\r");
        }
        session.wait_for_new_cursorless_condition(
            after,
            |frame| frame.contents().contains("resize required"),
            "hidden confirmation cannot execute",
            Duration::from_secs(8),
        );
        assert!(durable::attempts(&fixture).is_empty());
        exact(&session, &fixture, &steps, 0);
        let after = session.completed_cursorless_end();
        if mouse {
            let (x, y) = session.wait_for_owned_completed_action(
                "Original Actions • resize required",
                "[Esc Close]",
                &[],
            );
            session.click(x, y);
        } else {
            session.send_key(b"\x1b");
        }
        session.wait_for_new_cursorless_condition(
            after,
            |frame| {
                !frame
                    .contents()
                    .contains("Original Actions • resize required")
            },
            "Actual tiny Close",
            Duration::from_secs(8),
        );
        finish(
            &mut session,
            &fixture,
            mouse,
            kind,
            &steps,
            0,
            &git,
            &config,
            &before,
        );
    }
}
#[test]
fn scoped_actions_control_v5_keyboard_force_explicit_choice_202_pending_no_false_ack() {
    force(false)
}
#[test]
fn scoped_actions_control_v5_mouse_force_explicit_choice_202_pending_no_false_ack() {
    force(true)
}
#[test]
fn scoped_actions_control_v5_keyboard_only_one_accepted_cancel_explicit_force_escalation() {
    escalation(false)
}
#[test]
fn scoped_actions_control_v5_mouse_only_one_accepted_cancel_explicit_force_escalation() {
    escalation(true)
}
#[test]
fn scoped_actions_control_v5_keyboard_delete204_restart_empty_runs_read_ack_no_live_target() {
    delete_restart(false)
}
#[test]
fn scoped_actions_control_v5_mouse_delete204_restart_empty_runs_read_ack_no_live_target() {
    delete_restart(true)
}
#[test]
fn scoped_actions_control_v5_keyboard_logs204_separate_f6_acceptance_no_deleted_flag() {
    logs_ack(false)
}
#[test]
fn scoped_actions_control_v5_mouse_logs204_separate_f6_acceptance_no_deleted_flag() {
    logs_ack(true)
}
#[test]
fn scoped_actions_control_v5_keyboard_delete403_protected_unknown_no_fallback() {
    forbidden(false)
}
#[test]
fn scoped_actions_control_v5_mouse_delete403_protected_unknown_no_fallback() {
    forbidden(true)
}
#[test]
fn scoped_actions_control_v5_keyboard_changed_uid_before_force_never_invokes() {
    changed_uid(false)
}
#[test]
fn scoped_actions_control_v5_mouse_changed_uid_before_force_never_invokes() {
    changed_uid(true)
}
#[test]
fn scoped_actions_control_v5_keyboard_held_logs_receipt_cancel_exact_owned_child_reaped() {
    held(false)
}
#[test]
fn scoped_actions_control_v5_mouse_held_logs_receipt_cancel_exact_owned_child_reaped() {
    held(true)
}
#[test]
fn scoped_actions_control_v5_keyboard_all_three_tiny_no_hidden_approval_and_clickable_close() {
    tiny(false)
}
#[test]
fn scoped_actions_control_v5_mouse_all_three_tiny_no_hidden_approval_and_clickable_close() {
    tiny(true)
}

struct HeldFinalUidV6(ProcessIdentity);
impl HeldFinalUidV6 {
    fn stop(child: ProcessIdentity) -> Self {
        assert!(child.pid > 0 && !child.zombie);
        assert_eq!(process_identity(child.pid).unwrap().as_ref(), Some(&child));
        // SAFETY: entered() already proves exact positive original child,
        // UID/birth/parent/SID. Rechecked immediately, never a name or group.
        assert_eq!(unsafe { libc::kill(child.pid, libc::SIGSTOP) }, 0);
        Self(child)
    }
}
impl Drop for HeldFinalUidV6 {
    fn drop(&mut self) {
        if let Ok(Some(now)) = process_identity(self.0.pid) {
            if !now.zombie
                && (now.pid, now.uid, now.birth, now.session)
                    == (self.0.pid, self.0.uid, self.0.birth, self.0.session)
            {
                // SAFETY: only the same verified original stopped child.
                unsafe { libc::kill(now.pid, libc::SIGCONT) };
            }
        }
    }
}
fn ack_row_snapshot_v6(fixture: &Fixture) -> (Vec<String>, Vec<String>) {
    let db = durable::raw(fixture);
    let values = |sql: &str| {
        db.prepare(sql)
            .unwrap()
            .query_map([], |row| row.get::<_, String>(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    (values("SELECT json_array(attempt_id,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,claimed,receipt_json,created_at) FROM actions_write_attempts ORDER BY attempt_id"),
     values("SELECT json_array(host,principal,resource_key,generation) FROM actions_write_generations ORDER BY host,principal,resource_key"))
}
fn ack_sql_drift_v6(mouse: bool) {
    for field in ["receipt_json", "context_json", "options_json"] {
        let mut steps = opening();
        steps.extend(phase(42));
        steps.extend(confirm(Command::Delete, true));
        steps.extend(history_phase(true));
        let mut ack = history_phase(true);
        ack.last_mut().unwrap().delay_ms = 5_000;
        steps.extend(ack);
        assert_eq!(steps.len(), 69);
        let fixture = configured_native(&steps);
        let git = git_state(&fixture);
        let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
        let mut session = start(&fixture, mouse);
        initial(&mut session, &fixture, mouse, Command::Delete);
        let before = durable::unrelated_sql(&fixture);
        preview(&mut session, &fixture, mouse, Command::Delete, 40);
        let after = session.completed_cells_end();
        control(
            &mut session,
            mouse,
            &Command::Delete.form(),
            "[Confirm action]",
        );
        session.wait_for_command_count(&fixture, 55);
        cells(
            &mut session,
            after,
            &Command::Delete.history(),
            &["HTTP204", "[View original]"],
        );
        let after = session.completed_cursorless_end();
        control(
            &mut session,
            mouse,
            &Command::Delete.history(),
            "[View original]",
        );
        session.wait_for_command_count(&fixture, 62);
        complete(
            &mut session,
            after,
            &[
                "Inspect original Actions publication",
                "NOT proof of deletion",
                "EFFECT UNVERIFIED",
            ],
        );
        let label = "Acknowledge inspected acceptance (effect unverified)";
        if mouse {
            resources_v2::action(&mut session, true, label, b"");
        } else {
            session.focus_completed_actions_control_v2(label);
            session.send_key(b"\r");
        }
        let original = entered(&mut session, &fixture, 69);
        let held = HeldFinalUidV6::stop(original.clone());
        let db = rusqlite::Connection::open(database(&fixture)).unwrap();
        let sql = match field {
            "receipt_json" => "SELECT receipt_json FROM actions_write_attempts WHERE attempt_id=1",
            "context_json" => "SELECT context_json FROM actions_write_attempts WHERE attempt_id=1",
            "options_json" => "SELECT options_json FROM actions_write_attempts WHERE attempt_id=1",
            _ => unreachable!(),
        };
        let body: String = db.query_row(sql, [], |row| row.get(0)).unwrap();
        let mut body: Value = serde_json::from_str(&body).unwrap();
        match field {
            "receipt_json" => body["observed_attempt"] = json!(3),
            "context_json" => body["head_sha"] = json!("b".repeat(40)),
            "options_json" => body["debug_logging"] = json!(true),
            _ => unreachable!(),
        }
        let sql = match field {
            "receipt_json" => {
                "UPDATE actions_write_attempts SET receipt_json=?1 WHERE attempt_id=1"
            }
            "context_json" => {
                "UPDATE actions_write_attempts SET context_json=?1 WHERE attempt_id=1"
            }
            "options_json" => {
                "UPDATE actions_write_attempts SET options_json=?1 WHERE attempt_id=1"
            }
            _ => unreachable!(),
        };
        assert_eq!(db.execute(sql, [body.to_string()]).unwrap(), 1);
        drop(db);
        let edited = ack_row_snapshot_v6(&fixture);
        let after = session.completed_cursorless_end();
        drop(held);
        session.wait_for_new_cursorless_condition(
            after,
            |frame| {
                let text = frame.contents();
                text.contains("scope is fenced")
                    && text.contains("private pages hidden")
                    && !text.contains(H)
                    && !text.contains(&Command::Delete.history())
                    && crate::support::main_footer_visible(frame)
            },
            "Actual changed original ACK refused after last UID, private source hidden",
            Duration::from_secs(8),
        );
        gone(&mut session, &original);
        assert_eq!(
            ack_row_snapshot_v6(&fixture),
            edited,
            "No ACK, generation advance, body repair or adopted metadata"
        );
        let db = durable::raw(&fixture);
        assert_eq!(
            db.query_row(
                "SELECT state FROM actions_write_attempts WHERE attempt_id=1",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
            "accepted_pending"
        );
        drop(db);
        finish(
            &mut session,
            &fixture,
            mouse,
            Command::Delete,
            &steps,
            1,
            &git,
            &config,
            &before,
        );
    }
}
#[test]
fn scoped_actions_history_cas_v6_keyboard_last_uid_receipt_context_options_drift_no_ack_or_resend()
{
    ack_sql_drift_v6(false)
}
#[test]
fn scoped_actions_history_cas_v6_mouse_last_uid_receipt_context_options_drift_no_ack_or_resend() {
    ack_sql_drift_v6(true)
}
