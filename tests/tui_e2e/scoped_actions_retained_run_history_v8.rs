//! SOURCE ONLY/UNRUN. Real binary, genuine initial PR source and SAME writer,
//! exact original historical read/ACK, Git/SQL/config/TTY and positive-PID effects.
use super::*;
#[path = "scoped_actions_retained_history_lifecycle_v8.rs"]
mod lifecycle;
use crate::scoped_actions_owned_body_v120 as owned_v120;
#[test]
fn scoped_actions_retained_history_v8_keyboard_own_pr_cancel_observed_ack() {
    positive(false, C::Cancel);
}
#[test]
fn scoped_actions_retained_history_v8_mouse_own_pr_cancel_observed_ack() {
    positive(true, C::Cancel);
}
#[test]
fn scoped_actions_retained_history_v8_keyboard_rerun_all_failed_actual_job_a_advance() {
    for c in [C::RerunAll, C::RerunFailed, C::RerunJob] {
        positive(false, c);
    }
}
#[test]
fn scoped_actions_retained_history_v8_mouse_rerun_all_failed_actual_job_a_advance() {
    for c in [C::RerunAll, C::RerunFailed, C::RerunJob] {
        positive(true, c);
    }
}
#[test]
fn scoped_actions_retained_history_v8_keyboard_deleted_run404_inspect_f6_only() {
    positive(false, C::DeleteRun);
}
#[test]
fn scoped_actions_retained_history_v8_mouse_deleted_run404_inspect_f6_only() {
    positive(true, C::DeleteRun);
}
#[test]
fn scoped_actions_retained_history_v8_keyboard_logs_f4_disabled_f6_acceptance() {
    positive(false, C::DeleteLogs);
}
#[test]
fn scoped_actions_retained_history_v8_mouse_logs_f4_disabled_f6_acceptance() {
    positive(true, C::DeleteLogs);
}
#[test]
fn scoped_actions_retained_history_v8_keyboard_held_uid_exact_raw_receipt_context_options() {
    held(false, false);
}
#[test]
fn scoped_actions_retained_history_v8_mouse_held_uid_exact_raw_receipt_context_options() {
    held(true, false);
}
#[test]
fn scoped_actions_retained_history_v8_keyboard_owned_cancel_and_current_uid_parent_drift() {
    held(false, true);
    privacy(false);
}
#[test]
fn scoped_actions_retained_history_v8_mouse_owned_cancel_and_current_uid_parent_drift() {
    held(true, true);
    privacy(true);
}
#[test]
fn scoped_actions_retained_history_v8_keyboard_raw_over256k_keeps_protection_and_terminal() {
    oversized_raw(false);
}
#[test]
fn scoped_actions_retained_history_v8_mouse_raw_over256k_keeps_protection_and_terminal() {
    oversized_raw(true);
}
use crate::support::owned_pty_processes_v1::{process_identity, ProcessIdentity};
use ghtui::actions_workspace::ActionsCommand as C;
fn m(a: u64, status: &str, conclusion: &str) -> Value {
    let mut v = observed_run();
    v["run_attempt"] = json!(a);
    v["status"] = json!(status);
    v["conclusion"] = if conclusion.is_empty() {
        Value::Null
    } else {
        json!(conclusion)
    };
    v
}
fn initial_model(c: C) -> Value {
    if matches!(c, C::Cancel | C::ForceCancel | C::DeleteRun | C::DeleteLogs) {
        m(2, "in_progress", "")
    } else {
        m(2, "completed", "failure")
    }
}
fn run_steps(mut steps: Vec<GhStep>, v: &Value) -> Vec<GhStep> {
    for s in &mut steps {
        if s.args.get(1).is_some_and(|s| {
            s.ends_with("/actions/runs/123") || s.ends_with("/actions/runs/123/attempts/2")
        }) {
            s.stdout = v.to_string().into_bytes();
        }
    }
    steps
}
fn hist(v: Option<Value>, uid: u64) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    for _ in 0..2 {
        let missing = v.is_none();
        let mut s = get(
            &format!("repos/{FIRST}/actions/runs/123"),
            v.clone().unwrap_or(Value::Null),
        );
        if missing {
            s.status = 1;
            s.stdout.clear();
            s.stderr = b"gh: Not Found (HTTP 404)".to_vec();
        }
        out.push(s);
    }
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.extend(principal(uid));
    assert_eq!(out.len(), 10);
    out
}
fn start_steps(c: C) -> Vec<GhStep> {
    let initial = initial_model(c);
    let mut out = run_steps(steps(vec![]), &initial);
    out.extend(run_steps(phase(), &initial));
    out.extend(run_steps(phase(), &initial));
    let receipt = if matches!(c, C::RerunAll | C::RerunFailed | C::RerunJob) {
        Some(m(3, "queued", ""))
    } else if c == C::DeleteRun {
        None
    } else {
        Some(initial.clone())
    };
    out.extend(native_confirm(c, &initial, receipt, 42));
    out
}
fn title(c: C) -> String {
    if c == C::Cancel {
        "Cancel original Actions run".into()
    } else {
        format!("{} • original Actions", c.label())
    }
}
fn ht(c: C) -> String {
    if c == C::Cancel {
        "Original Cancel publication".into()
    } else {
        format!("Original {} publication", c.label())
    }
}
fn act(s: &mut Session, mouse: bool, title: &str, label: &str) {
    if mouse {
        let (x, y) = s.wait_for_owned_completed_action(title, label, &[]);
        s.click(x, y);
        return;
    }
    s.wait_for_owned_completed_action(title, label, &[]);
    if label == "[F4 Acknowledge]" {
        s.send_key(b"\x1bOS");
        return;
    }
    if label == "[F6 ACK receipt]" {
        s.send_key(b"\x1b[17~");
        return;
    }
    let text = s
        .completed_cells_screen()
        .and_then(|f| crate::support::overlay_body(f, title))
        .unwrap();
    let mut labels = if text.contains("[View original]") {
        vec!["[View original]", "[F4 Acknowledge]"]
    } else {
        vec!["[Preview]", "[Confirm action]"]
    };
    if text.contains("[F6 ACK receipt]") {
        labels.push("[F6 ACK receipt]");
    }
    labels.push(if text.contains("[Esc Cancel]") {
        "[Esc Cancel]"
    } else {
        "[Keep draft / Close]"
    });
    if text.contains("[F7 Error↑]") {
        labels.extend(["[F7 Error↑]", "[F8 Error↓]"]);
    }
    s.focus_completed_owned_action_v1(
        title,
        label,
        crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
        &labels,
        &[],
    );
    s.send_key(b"\r");
}
fn open_preview(s: &mut Session, f: &Fixture, mouse: bool, c: C, offset: usize) {
    if offset == 0 {
        open_run(s, f, mouse);
    } else {
        open_run_offset_v120(s, f, mouse, offset);
    }
    s.wait_for_command_count(f, offset + 59);
    let after = s.completed_cells_end();
    if mouse {
        control(s, true, &format!("[{}]", c.label()), b"");
    } else {
        s.focus_completed_actions_control_v2(&format!("[{}]", c.label()));
        s.send_key(b"\r");
    }
    s.wait_for_command_count(f, offset + 71);
    complete_cells(
        s,
        after,
        &title(c),
        &["Numeric repository", "Current repository:"],
    );
    if c == C::ForceCancel {
        let changed_after = s.completed_cells_end();
        if mouse {
            let (x, y) = s.wait_for_owned_completed_action(&title(c), "[true]", &[]);
            s.click(x, y);
        } else {
            let focused = |frame: &vt100::Screen| {
                !frame.hide_cursor()
                    && crate::support::overlay_body(frame, &title(c))
                        .is_some_and(|b| b.contains("Escalate force cancellation"))
            };
            if !s.completed_cells_screen().is_some_and(&focused) {
                let before = s.completed_cells_end();
                s.send_key(b"\x1b[Z");
                let until = std::time::Instant::now() + Duration::from_secs(8);
                loop {
                    s.poll_output(Duration::from_millis(1));
                    if s.completed_cells_end() > before
                        && s.completed_cells_screen().is_some_and(&focused)
                    {
                        break;
                    }
                    assert!(
                        std::time::Instant::now() < until,
                        "Actual sole force input caret"
                    );
                }
            }
            s.send_key(b" ");
        }
        let until = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > changed_after
                && s.completed_cells_screen()
                    .and_then(|frame| crate::support::overlay_body(frame, &title(c)))
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
                break;
            }
            assert!(
                std::time::Instant::now() < until,
                "Actual force value true in a new completed Form, not its choices list"
            );
        }
    }
    let after = s.completed_cells_end();
    act(s, mouse, &title(c), "[Preview]");
    s.wait_for_command_count(f, offset + 83);
    complete_cells(s, after, &title(c), &["PREVIEW", H, "[Confirm action]"]);
}
fn selected_offset_v120(s: &mut Session, f: &Fixture, mouse: bool, offset: usize) {
    let prior = commands(s, f);
    assert_eq!(prior.len(), offset);
    choose(s, mouse, "[: Commands]", b":");
    overlay_ready(
        s,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    s.send_key(b"\x01");
    s.paste("Discover repositories");
    overlay_ready(
        s,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(s, mouse, "> Discover repositories", b"\r");
    s.wait_for_overlay_close("Commands · current context");
    ready(s, &[TITLE, "[s Source]", "[Esc Close]"]);
    assert_eq!(
        commands(s, f),
        prior,
        "Pure discovery opening preserves every prior write record"
    );
    discover(s, mouse);
    s.wait_for_command_count(f, offset + 7);
    ready(s, &[TITLE, FIRST]);
    if !mouse {
        s.send_key(b"\t");
    }
    choose(s, mouse, "[Open My PRs]", b"\r");
    s.wait_for_overlay_close(TITLE);
    s.wait_for_command_count(f, offset + 18);
    ready(s, &[ROW_TITLE]);
    choose(s, mouse, "1 Inbox", b"1");
    choose(s, mouse, ROW_TITLE, b"\r");
    s.wait_for_command_count(f, offset + 31);
    ready(s, &[DETAIL_TITLE, DETAIL_BODY, "[b Back]"]);
}
fn open_run_offset_v120(s: &mut Session, f: &Fixture, mouse: bool, offset: usize) {
    selected_offset_v120(s, f, mouse, offset);
    choose(s, mouse, "[k Checks]", b"k");
    s.wait_for_command_count(f, offset + 41);
    ready(
        s,
        &[
            "Required rules unavailable",
            "Exact observed build",
            "ci-actor",
        ],
    );
    select_observed_row(s, mouse, "Exact observed build");
    // Open check joins the key bar only once a check row is selected.
    s.wait_for_control("[Enter Open check]");
    if mouse {
        s.click_visible_action("[Enter Open check]");
    } else {
        // `Enter Open check` is a key-bar token: one Enter opens the selected check.
        s.wait_for_control("[Enter Open check]");
        s.send_key(b"\r");
    }
    s.wait_for_command_count(f, offset + 59);
    ready(
        s,
        &[
            "Run 123 • attempt 2",
            "Job 501",
            "verified exact endpoint",
            "alice",
            H,
        ],
    );
    assert!(!s.screen().contains(DETAIL_BODY));
    assert!(!s.screen().contains(PRIVATE_DRAFT));
    assert!(!s.screen().contains("run 999"));
}
fn begin(s: &mut Session, f: &Fixture, mouse: bool, c: C, count: usize) {
    begin_offset(s, f, mouse, c, count, 0);
}
fn begin_offset(s: &mut Session, f: &Fixture, mouse: bool, c: C, count: usize, offset: usize) {
    open_preview(s, f, mouse, c, offset);
    let after = s.completed_cells_end();
    act(s, mouse, &title(c), "[Confirm action]");
    s.wait_for_command_count(f, count);
    complete_cells(s, after, &ht(c), &["[View original]"]);
}
fn state(f: &Fixture) -> String {
    raw(f)
        .query_row(
            "SELECT state FROM actions_write_attempts ORDER BY attempt_id DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap()
}
fn journal_rows(f: &Fixture) -> Vec<String> {
    let db = raw(f);
    let mut q=db.prepare("SELECT json_array(attempt_id,state,claimed,receipt_json,context_json,options_json,resources_json) FROM actions_write_attempts ORDER BY attempt_id").unwrap();
    let v = q
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    v
}
fn exact_v8(s: &Session, f: &Fixture, expected: &[GhStep], writes: usize) {
    let rows = commands(s, f);
    assert_eq!(rows.len(), expected.len());
    for (r, e) in rows.iter().zip(expected) {
        assert_eq!(r["matched"], true);
        assert_eq!(r["args"], json!(e.args));
        assert_eq!(
            r["stdin_hex"],
            e.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(
        rows.iter()
            .filter(|r| r["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|a| matches!(a.as_str(), Some("POST" | "DELETE" | "PUT" | "PATCH"))))
            .count(),
        writes
    );
}
fn close(s: &mut Session, mouse: bool, c: C) {
    if s.completed_cells_screen()
        .and_then(|f| crate::support::overlay_body(f, &ht(c)))
        .is_some()
    {
        let after = s.completed_cells_end();
        act(s, mouse, &ht(c), "[Keep draft / Close]");
        s.wait_for_new_cursorless_condition(
            after,
            |f| {
                crate::support::overlay_body(f, &ht(c)).is_none()
                    && crate::support::main_footer_visible(f)
            },
            "Exact retained history Close",
            Duration::from_secs(8),
        );
    }
}
fn positive(mouse: bool, c: C) {
    let value = if matches!(c, C::RerunAll | C::RerunFailed | C::RerunJob) {
        Some(m(3, "queued", ""))
    } else if c == C::DeleteRun {
        None
    } else {
        Some(m(2, "completed", "cancelled"))
    };
    let acceptance = matches!(c, C::DeleteRun | C::DeleteLogs);
    let mut expected = start_steps(c);
    let confirmed = expected.len();
    expected.extend(hist(value.clone(), 42));
    expected.extend(hist(value.clone(), 42));
    expected.extend(hist(value, 42));
    let f = resource_configured(&expected);
    let git = git_state(&f);
    let cfg = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    begin(&mut s, &f, mouse, c, confirmed);
    let tty = s.actual_terminal_witness_v4().unwrap();
    assert!(tty.is_raw());
    let sql = unrelated(&f);
    let trace = git_commands(&f);
    let before = journal_rows(&f);
    assert_eq!(state(&f), "accepted_pending");
    let after = s.completed_cells_end();
    act(&mut s, mouse, &ht(c), "[View original]");
    s.wait_for_command_count(&f, confirmed + 10);
    owned_v120::visible(
        &mut s,
        mouse,
        after,
        &ht(c),
        if acceptance {
            &["EFFECT UNVERIFIED • F4 does not confirm deletion; separate F6 acknowledges only inspected typed acceptance. HTTP404 is unavailable, not deletion or causality proof."]
        } else {
            &["Later original run transition observed"]
        },
    );
    if acceptance {
        let after = s.completed_cells_end();
        act(&mut s, mouse, &ht(c), "[F4 Acknowledge]");
        complete_cells(
            &mut s,
            after,
            &ht(c),
            &["Inspect a current original result"],
        );
        assert_eq!(journal_rows(&f), before);
    }
    let mut controls = vec!["[View original]", "[F4 Acknowledge]"];
    if acceptance {
        controls.push("[F6 ACK receipt]");
    }
    controls.push("[Keep draft / Close]");
    if s.completed_cells_screen()
        .and_then(|frame| crate::support::overlay_body(frame, &ht(c)))
        .is_some_and(|body| body.contains("[F7 Error↑]"))
    {
        controls.extend(["[F7 Error↑]", "[F8 Error↓]"]);
    }
    // Centered Form: actual main SIGWINCH/footer, then its complete owned row.
    let expected_controls = controls[..if acceptance { 4 } else { 3 }].join(" ");
    let owned_controls = |frame: &vt100::Screen| {
        frame.size() == (24, 80)
            && crate::support::overlay_body(frame, &ht(c)).is_some_and(|body| {
                body.lines()
                    .last()
                    .is_some_and(|row| row.trim() == expected_controls)
            })
    };
    if s.completed_cursorless_screen()
        .is_some_and(|frame| frame.size() == (24, 80))
    {
        assert!(s.completed_cursorless_screen().is_some_and(owned_controls));
    } else {
        let resized_after = s.completed_cursorless_end();
        s.resize(80, 24);
        s.wait_for_new_cursorless_condition(
            resized_after,
            owned_controls,
            "Actual resized centered original result owns every complete ordered control",
            Duration::from_secs(8),
        );
    }
    let after = s.completed_cells_end();
    act(
        &mut s,
        mouse,
        &ht(c),
        if acceptance {
            "[F6 ACK receipt]"
        } else {
            "[F4 Acknowledge]"
        },
    );
    s.wait_for_command_count(&f, confirmed + 20);
    owned_v120::visible(
        &mut s,
        mouse,
        after,
        &ht(c),
        if acceptance {
            &["Explicit inspected acceptance acknowledged locally • effect UNVERIFIED • original receipt retained • no remote mutation/retry"]
        } else {
            &["Explicit original observation acknowledged locally • no causal claim • no remote mutation/retry"]
        },
    );
    assert_eq!(
        state(&f),
        if acceptance {
            "acknowledged"
        } else {
            "transition_observed"
        }
    );
    let after = s.completed_cells_end();
    act(&mut s, mouse, &ht(c), "[View original]");
    s.wait_for_command_count(&f, confirmed + 30);
    owned_v120::visible(
        &mut s,
        mouse,
        after,
        &ht(c),
        &["Original result already closed or current observation unavailable • no inferred completed effect or new approval."],
    );
    assert_eq!(unrelated(&f), sql);
    assert_eq!(git_commands(&f), trace);
    assert_eq!(git_state(&f), git);
    close(&mut s, mouse, c);
    assert_eq!(
        s.actual_terminal_witness_v4().unwrap(),
        tty,
        "Native host retains actual termios/SID/foreground through history and Back"
    );
    s.quit_and_assert_restored_terminal(mouse);
    exact_v8(&s, &f, &expected, 1);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        cfg
    );
    assert_eq!(git_state(&f), git);
    assert_read_only_git(&f);
}
struct Stop(ProcessIdentity);
impl Stop {
    fn capture(s: &Session, pid: i32) -> Self {
        let p = process_identity(s.owned_application_pid())
            .unwrap()
            .unwrap();
        let c = process_identity(pid).unwrap().unwrap();
        assert!(pid > 0 && pid != p.pid && !c.zombie);
        assert_eq!((c.parent, c.uid, c.session), (p.pid, p.uid, p.session));
        assert_eq!(process_identity(pid).unwrap().as_ref(), Some(&c));
        // SAFETY: exact original positive-PID/UID/birth/SID child verified above.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        Self(c)
    }
}
impl Drop for Stop {
    fn drop(&mut self) {
        if let Ok(Some(c)) = process_identity(self.0.pid) {
            if !c.zombie
                && (c.uid, c.birth, c.session) == (self.0.uid, self.0.birth, self.0.session)
            {
                // SAFETY: only original verified stopped child, never group/name/foreign PID.
                assert_eq!(unsafe { libc::kill(c.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}
fn child(s: &mut Session, f: &Fixture, count: usize) -> ProcessIdentity {
    s.wait_for_command_count(f, count);
    let rows = commands(s, f);
    let pid = i32::try_from(rows.last().unwrap()["pid"].as_i64().unwrap()).unwrap();
    let c = process_identity(pid).unwrap().unwrap();
    let p = process_identity(s.owned_application_pid())
        .unwrap()
        .unwrap();
    assert_eq!((c.parent, c.uid, c.session), (p.pid, p.uid, p.session));
    assert!(!c.zombie);
    c
}
fn gone(s: &mut Session, c: &ProcessIdentity) {
    let until = std::time::Instant::now() + Duration::from_secs(4);
    while let Some(now) = process_identity(c.pid).unwrap() {
        assert_eq!(
            (now.birth, now.uid, now.session),
            (c.birth, c.uid, c.session)
        );
        assert!(std::time::Instant::now() < until);
        s.poll_output(Duration::from_millis(10));
    }
}
fn held(mouse: bool, cancel: bool) {
    let changes: Vec<Option<&str>> = if cancel {
        vec![None]
    } else {
        vec![
            Some("receipt_json"),
            Some("context_json"),
            Some("options_json"),
        ]
    };
    for column in changes {
        let c = C::RerunAll;
        let mut expected = start_steps(c);
        let confirmed = expected.len();
        expected.extend(hist(Some(m(3, "queued", "")), 42));
        let mut ack = hist(Some(m(3, "queued", "")), 42);
        ack.last_mut().unwrap().delay_ms = if cancel { 60_000 } else { 5_000 };
        expected.extend(ack);
        let f = resource_configured(&expected);
        let git = git_state(&f);
        let cfg = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let mut s = launch(&f, mouse);
        begin(&mut s, &f, mouse, c, confirmed);
        let tty = s.actual_terminal_witness_v4().unwrap();
        assert!(tty.is_raw());
        let sql = unrelated(&f);
        let trace = git_commands(&f);
        let after = s.completed_cells_end();
        act(&mut s, mouse, &ht(c), "[View original]");
        s.wait_for_command_count(&f, confirmed + 10);
        owned_v120::visible(
            &mut s,
            mouse,
            after,
            &ht(c),
            &["Later original run transition observed"],
        );
        act(&mut s, mouse, &ht(c), "[F4 Acknowledge]");
        let captured = child(&mut s, &f, confirmed + 20);
        let stopped = if cancel {
            None
        } else {
            Some(Stop::capture(&s, captured.pid))
        };
        if let Some(column) = column {
            // This is deliberately an external concurrent writer; ordinary
            // SQL observation helpers remain READ_ONLY.
            let db = rusqlite::Connection::open(database(&f)).unwrap();
            let original: String = db
                .query_row(
                    &format!("SELECT {column} FROM actions_write_attempts"),
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let mut value: Value = serde_json::from_str(&original).unwrap();
            match column {
                "receipt_json" => value["observed_attempt"] = json!(4),
                "context_json" => value["head_sha"] = json!("b".repeat(40)),
                _ => value["debug_logging"] = json!(true),
            }
            assert_eq!(
                db.execute(
                    &format!("UPDATE actions_write_attempts SET {column}=?1"),
                    [value.to_string()]
                )
                .unwrap(),
                1
            );
        }
        let edited = journal_rows(&f);
        let canceled_after = s.completed_cursorless_end();
        if cancel {
            let (x, y) = s.wait_for_owned_completed_action(&ht(c), "[Esc Cancel]", &[]);
            if mouse {
                s.click(x, y);
            } else {
                s.send_key(b"\x1b");
            }
        }
        drop(stopped);
        if cancel {
            // This is a READ/ACK cancellation, not a fresh write Confirm or
            // foreign source. The product deliberately retains this exact
            // protected Form/source and retires its old prepared body/purpose.
            s.wait_for_new_cursorless_condition(
                canceled_after,
                |frame| {
                    frame
                        .contents()
                        .contains("Original Actions preparation canceled")
                        && crate::support::overlay_body(frame, &ht(c)).is_some_and(|body| {
                            !body.contains("[Esc Cancel]")
                                && body.contains("[Keep draft / Close]")
                                && !body.contains("Later original run transition observed")
                        })
                },
                "Exact canceled read retains nonbusy OWN protected Form, not a private result",
                Duration::from_secs(8),
            );
        } else {
            // Genuine current UID/parent/raw SQL drift still MUST retire the
            // original private source; never treat it as harmless cancellation.
            s.wait_for_cursorless_condition(|frame|!frame.contents().contains(&ht(c))&&frame.contents().contains("private")&&crate::support::main_footer_visible(frame),
                "Original changed exact history refuses private result and preserves protected receipt",Duration::from_secs(8));
        }
        gone(&mut s, &captured);
        assert_eq!(state(&f), "accepted_pending");
        assert_eq!(journal_rows(&f), edited);
        assert_eq!(unrelated(&f), sql);
        assert_eq!(git_state(&f), git);
        assert_eq!(git_commands(&f), trace);
        assert_eq!(s.actual_terminal_witness_v4().unwrap(), tty);
        if cancel {
            // The full one-row status does not fit80. Perform an actual resize
            // to inspect EVERY word, rather than accepting a clipped prefix.
            if s.completed_cursorless_screen().unwrap().size() != (40, 120) {
                let resized_after = s.completed_cursorless_end();
                s.resize(120, 40);
                s.wait_for_new_cursorless_condition(
                    resized_after,
                    |frame| {
                        frame.size() == (40, 120)
                            && crate::support::overlay_body(frame, &ht(c)).is_some_and(|body| {
                                body.lines().last().is_some_and(|row| {
                                    row.trim()
                                        == "[View original] [F4 Acknowledge] [Keep draft / Close]"
                                })
                            })
                    },
                    "Actual resized own canceled Form has THREE whole ordered controls",
                    Duration::from_secs(8),
                );
            }
            assert!(s.completed_cursorless_screen().unwrap().contents().contains(
                "Original Actions preparation canceled • retained source • no mutation or renewed purpose"));
            assert_eq!(journal_rows(&f), edited);
            close(&mut s, mouse, c);
        }
        s.quit_and_assert_restored_terminal(mouse);
        exact_v8(&s, &f, &expected, 1);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            cfg
        );
    }
}
fn privacy(mouse: bool) {
    for changed_parent in [false, true] {
        let c = C::RerunAll;
        let mut expected = start_steps(c);
        let confirmed = expected.len();
        let mut phase = hist(
            Some(m(3, "queued", "")),
            if changed_parent { 42 } else { 999 },
        );
        if changed_parent {
            let mut parent = parent_resource();
            parent["head"]["sha"] = json!("b".repeat(40));
            phase[6].stdout = parent.to_string().into_bytes();
            phase.truncate(7);
        }
        expected.extend(phase);
        let f = resource_configured(&expected);
        let git = git_state(&f);
        let mut s = launch(&f, mouse);
        begin(&mut s, &f, mouse, c, confirmed);
        let sql = unrelated(&f);
        let original = journal_rows(&f);
        act(&mut s, mouse, &ht(c), "[View original]");
        s.wait_for_command_count(&f, expected.len());
        s.wait_for_cursorless_condition(
            |frame| {
                frame.contents().contains("private")
                    && !frame.contents().contains(&ht(c))
                    && crate::support::main_footer_visible(frame)
            },
            "Actual UID/current parent drift hides original private result",
            Duration::from_secs(8),
        );
        assert_eq!(journal_rows(&f), original);
        assert_eq!(unrelated(&f), sql);
        assert_eq!(git_state(&f), git);
        s.quit_and_assert_restored_terminal(mouse);
        exact_v8(&s, &f, &expected, 1);
    }
}
fn oversized_raw(mouse: bool) {
    let c = C::RerunAll;
    let mut expected = start_steps(c);
    let confirmed = expected.len();
    let mut observed = m(3, "queued", "");
    observed["unused_private_payload"] = json!("PRIVATE_RAW_CAP_CANARY".repeat(14_000));
    let mut phase = hist(Some(observed), 42);
    phase.truncate(5);
    expected.extend(phase);
    let f = resource_configured(&expected);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    begin(&mut s, &f, mouse, c, confirmed);
    let tty = s.actual_terminal_witness_v4().unwrap();
    let sql = unrelated(&f);
    let original = journal_rows(&f);
    let trace = git_commands(&f);
    let after = s.completed_cells_end();
    act(&mut s, mouse, &ht(c), "[View original]");
    s.wait_for_command_count(&f, expected.len());
    s.wait_for_new_cursorless_condition(
        after,
        |frame| {
            !frame.contents().contains(&ht(c))
                && !frame.contents().contains("PRIVATE_RAW_CAP_CANARY")
                && frame.contents().contains("private")
                && crate::support::main_footer_visible(frame)
        },
        "Actual over-cap private raw read retires its owned view, not the protected receipt",
        Duration::from_secs(8),
    );
    assert_eq!(journal_rows(&f), original);
    assert_eq!(state(&f), "accepted_pending");
    assert_eq!(unrelated(&f), sql);
    assert_eq!(git_state(&f), git);
    assert_eq!(git_commands(&f), trace);
    assert_eq!(s.actual_terminal_witness_v4().unwrap(), tty);
    s.quit_and_assert_restored_terminal(mouse);
    exact_v8(&s, &f, &expected, 1);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&f);
}

fn native_confirm(command: C, before: &Value, receipt: Option<Value>, uid: u64) -> Vec<GhStep> {
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(
        &format!("repos/{FIRST}/check-runs/600"),
        selected_check(),
    ));
    out.push(get(
        &format!("repos/{FIRST}/actions/runs/123"),
        before.clone(),
    ));
    out.extend(principal(42));
    out.push(get(
        &format!("repos/{FIRST}/actions/runs/123"),
        before.clone(),
    ));
    if command == C::RerunJob {
        let mut job = observed_job();
        job["run_attempt"] = json!(2);
        out.push(get(&format!("repos/{FIRST}/actions/jobs/501"), job.clone()));
        out.push(get(
            &format!("repos/{FIRST}/actions/runs/123/attempts/2"),
            before.clone(),
        ));
        out.push(get(
            &format!("repos/{FIRST}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),
            json!({"total_count":1,"jobs":[job]}),
        ));
    }
    out.push(get(&format!("repos/{FIRST}/pulls/7"), parent_resource()));
    out.push(get(
        &format!("repos/{FIRST}/check-runs/600"),
        selected_check(),
    ));
    out.push(get(
        &format!("repos/{FIRST}/actions/runs/123"),
        before.clone(),
    ));
    out.push(get(&format!("repos/{FIRST}"), repository(0)));
    out.extend(principal(42));
    let path = match command {
        C::RerunAll => "/rerun",
        C::RerunFailed => "/rerun-failed-jobs",
        C::RerunJob => "/rerun",
        C::Cancel => "/cancel",
        C::ForceCancel => "/force-cancel",
        C::DeleteRun => "",
        C::DeleteLogs => "/logs",
        _ => panic!("closed Run kind"),
    };
    let endpoint = if command == C::RerunJob {
        format!("repos/{FIRST}/actions/jobs/501{path}")
    } else {
        format!("repos/{FIRST}/actions/runs/123{path}")
    };
    let delete = matches!(command, C::DeleteRun | C::DeleteLogs);
    let rerun = matches!(command, C::RerunAll | C::RerunFailed | C::RerunJob);
    let mut args: Vec<String> = vec![
        "api".into(),
        endpoint,
        "--hostname".into(),
        HOST.into(),
        "--method".into(),
        if delete {
            "DELETE".into()
        } else {
            "POST".into()
        },
    ];
    if rerun {
        args.extend(["--input".into(), "-".into()]);
    }
    if delete || command == C::ForceCancel {
        args.push("--include".into());
    }
    let mut post = GhStep::read(
        &args.iter().map(String::as_str).collect::<Vec<_>>(),
        if delete {
            "HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n"
        } else if command == C::ForceCancel {
            "HTTP/2.0 202 Accepted\nContent-Length: 0\r\n\r\n"
        } else {
            "{}"
        },
    );
    if rerun {
        post.stdin = serde_json::to_vec(&json!({"enable_debug_logging":false})).unwrap();
    }
    out.push(post);
    let missing = receipt.is_none();
    let mut s = get(
        &format!("repos/{FIRST}/actions/runs/123"),
        receipt.unwrap_or(Value::Null),
    );
    if missing {
        s.status = 1;
        s.stdout.clear();
        s.stderr = b"gh: Not Found (HTTP 404)".to_vec();
    }
    out.push(s);
    if command == C::DeleteRun {
        out.push(get(&format!("repos/{FIRST}"), repository(0)));
    }
    out.push(get("user", json!({"id":uid,"login":"octocat"})));
    out
}
