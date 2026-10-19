//! TASK54W7 actual POST200 -> same original read engine -> protected Back.
//! Strict offline protocol declarations; UNCOMPILED/UNRUN until Root verifies.
use super::*;

fn returned_read(kind: ReadKind, uid: u64) -> Vec<GhStep> {
    fn actual_returned_fields(value: &mut Value) {
        match value {
            Value::Array(rows) => rows.iter_mut().for_each(actual_returned_fields),
            Value::Object(fields) => {
                if fields.get("id").and_then(Value::as_u64) == Some(123)
                    && fields.contains_key("workflow_id")
                {
                    *value = returned_run();
                    return;
                }
                if fields.contains_key("head_sha") {
                    fields.insert("head_sha".into(), json!(C));
                }
                if fields.get("run_id").and_then(Value::as_u64) == Some(123) {
                    fields.insert("run_id".into(), json!(124));
                    fields.insert("run_attempt".into(), json!(1));
                }
                if fields.contains_key("check_suite") {
                    fields.insert("check_suite".into(), json!({"id":902}));
                }
                fields.values_mut().for_each(actual_returned_fields);
            }
            _ => {}
        }
    }
    let mut rows = operation(kind, uid, CURRENT);
    for row in &mut rows {
        for arg in &mut row.args {
            *arg = arg
                .replace("/runs/123", "/runs/124")
                .replace("/attempts/2", "/attempts/1");
        }
        if row.args.first().is_some_and(|arg| arg == "run") {
            assert_eq!(row.args[2], "123");
            row.args[2] = "124".into();
            let at = row.args.iter().position(|arg| arg == "--attempt").unwrap();
            assert_eq!(row.args[at + 1], "2");
            row.args[at + 1] = "1".into();
        } else if let Ok(mut value) = serde_json::from_slice::<Value>(&row.stdout) {
            actual_returned_fields(&mut value);
            row.stdout = serde_json::to_vec(&value).unwrap();
        }
    }
    rows
}

pub(super) fn prepare_confirmed(mouse: bool, tail: Vec<GhStep>) -> (Fixture, Session, Vec<GhStep>) {
    let p = Protocol::Detailed;
    let mut expected = opening();
    expected.extend(schema_phase(p));
    expected.extend(schema_phase(p));
    expected.extend(confirmation(p, 42));
    let confirmed = expected.len();
    expected.extend(tail);
    let f = configured_native(&expected);
    let mut s = start(&f, mouse);
    begin(&mut s, &f, mouse, p);
    inspect_schema(&mut s, &f, mouse);
    preview(&mut s, &f, mouse);
    let after = s.completed_cells_end();
    action(&mut s, mouse, INTENT, "[Confirm action]");
    s.wait_for_command_count(&f, confirmed);
    owned(
        &mut s,
        after,
        HISTORY,
        &[
            "[View original]",
            "[F6 Acknowledge locally]",
            "[F11 Open run]",
            "[Close]",
        ],
    );
    let receipt: Value = serde_json::from_str(&metadata(&f).3).unwrap();
    assert_eq!(receipt["original_dispatch_run_v7"]["run_id"], 124);
    assert_eq!(receipt["original_dispatch_run_v7"]["attempt"], 1);
    assert_eq!(receipt["original_dispatch_run_v7"]["head_sha"], C);
    (f, s, expected)
}

fn follow_through(mouse: bool) {
    let mut tail = Vec::new();
    let mut ends = Vec::new();
    for (index, kind) in [
        ReadKind::Run,
        ReadKind::Run,
        ReadKind::Run,
        ReadKind::Logs,
        ReadKind::Annotations,
    ]
    .into_iter()
    .enumerate()
    {
        let mut phase = returned_read(kind, 42);
        if index == 2 {
            // The actual Job501 activation is selected-member Attempt, not
            // the prior unselected Run/Jobs reads. Keep BOTH fresh job GETs.
            let jobs = phase
                .iter()
                .position(|step| {
                    step.args.iter().any(|arg| {
                        arg == &format!(
                            "repos/{CURRENT}/actions/runs/124/attempts/1/jobs?per_page=100"
                        )
                    })
                })
                .unwrap();
            let mut job = fresh_job(CURRENT);
            job["run_id"] = json!(124);
            job["run_attempt"] = json!(1);
            job["head_sha"] = json!(C);
            phase.splice(
                jobs + 1..jobs + 1,
                [
                    get(&format!("repos/{CURRENT}/actions/jobs/501"), job.clone()),
                    get(&format!("repos/{CURRENT}/actions/jobs/501"), job),
                ],
            );
        }
        tail.extend(phase);
        ends.push(tail.len());
    }
    let (f, mut s, expected) = prepare_confirmed(mouse, tail);
    let confirmed = expected.len() - ends.last().copied().unwrap();
    let original = metadata(&f);
    assert_eq!(original.0, "accepted_pending");
    let git = git_state(&f);
    let protected = unrelated(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let tty = s.actual_terminal_witness_v4().unwrap();
    let before = s.completed_cursorless_end();
    action(&mut s, mouse, HISTORY, "[F11 Open run]");
    s.wait_for_command_count(&f, confirmed + ends[0]);
    complete(
        &mut s,
        before,
        &["Run 124", C, "[b Back to Dispatch receipt]"],
    );
    s.resize(80, 24);
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[j Jobs]", b"j");
    s.wait_for_command_count(&f, confirmed + ends[1]);
    let until = Instant::now() + Duration::from_secs(8);
    let point = loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cursorless_end() > before {
            if let Some(point) = s.completed_cursorless_screen().and_then(|frame| {
                if !crate::support::main_footer_visible(frame) {
                    return None;
                }
                let body = crate::support::overlay_body(frame, "Actions • focused view ─")?;
                if !frame.contents().contains("/ Jobs and steps /")
                    || !body.contains("Run 124 • attempt 1")
                    || !body.contains(C)
                    || !body.contains("Actual exact-attempt jobs")
                    // Its disabled writes left the key bar; the read-only status remains.
                    || !frame.contents().contains("Actual returned Dispatch run")
                    || crate::support::locate_control(frame, "[b Cancel read]").is_some()
                    || body.contains("Original current read pending")
                {
                    return None;
                }
                crate::support::completed_navigation_controls_v1::progress_selection_in(
                    frame,
                    "Actions • focused view ─",
                    "Job 501",
                )
                .map(|selection| selection.target)
            }) {
                break point;
            }
        }
        assert!(Instant::now() < until,
            "NEW actual Dispatch Jobs page, exact returned Run/A/H, READ ONLY and Job501 before selection\n{}", s.screen());
    };
    let before = s.completed_cursorless_end();
    if mouse {
        s.click(point.0, point.1);
    } else {
        selected_row(&mut s, false, "Job 501");
    }
    s.wait_for_command_count(&f, confirmed + ends[2]);
    complete(
        &mut s,
        before,
        &["Step 2", "Actual failed source step", "[N Annotations]"],
    );
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[l Logs]", b"l");
    s.wait_for_command_count(&f, confirmed + ends[3]);
    complete(
        &mut s,
        before,
        &[
            "PRIVATE_SCOPED_ACTIONS_JOB_501_LOG",
            "Actual failed source step",
        ],
    );
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[N Annotations]", b"N");
    s.wait_for_command_count(&f, confirmed + ends[4]);
    complete(
        &mut s,
        before,
        &["PRIVATE_SCOPED_ACTIONS_ANNOTATION", "reported metadata"],
    );
    assert_eq!(metadata(&f), original);
    let count = commands(&s, &f).len();
    let after = s.completed_cells_end();
    choose(&mut s, mouse, "[b Back to Dispatch receipt]", b"b");
    owned(
        &mut s,
        after,
        HISTORY,
        &[
            "[View original]",
            "[F6 Acknowledge locally]",
            "[F11 Open run]",
            "[Close]",
        ],
    );
    assert_eq!(
        commands(&s, &f).len(),
        count,
        "Pure Back cannot renew any source or dispatch purpose"
    );
    assert_eq!(metadata(&f), original);
    assert_eq!(unrelated(&f), protected);
    assert_eq!(git_state(&f), git);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(s.actual_terminal_witness_v4().unwrap(), tty);
    s.capture_completed_terminal_v1("task54-w7-returned-run-protected-back");
    close(&mut s, mouse, HISTORY);
    s.quit_and_assert_restored_terminal(mouse);
    exact(&s, &f, &expected, 1);
    assert_private_intents_unchanged(&f);
    assert_read_only_git(&f);
}

#[test]
fn scoped_actions_dispatch_v7_task54_w7_keyboard_actual_run_jobs_logs_annotations_and_pure_back() {
    follow_through(false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_mouse_actual_run_jobs_logs_annotations_and_pure_back() {
    follow_through(true);
}

fn held_open(mouse: bool) {
    for resize in [false, true] {
        let mut tail = returned_read(ReadKind::Run, 42);
        tail[3].delay_ms = 60_000;
        tail.truncate(4);
        let (f, mut s, expected) = prepare_confirmed(mouse, tail);
        let original = metadata(&f);
        let protected = unrelated(&f);
        let git = git_state(&f);
        let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
        let tty = s.actual_terminal_witness_v4().unwrap();
        action(&mut s, mouse, HISTORY, "[F11 Open run]");
        let child = entered(&mut s, &f, expected.len());
        let after = s.completed_cells_end();
        if resize {
            resize_form(
                &mut s,
                80,
                24,
                HISTORY,
                &[
                    "[View original]",
                    "[F6 Acknowledge locally]",
                    "[F11 Open run]",
                    "[Close]",
                ],
            );
        } else if mouse {
            let (x, y) = s.wait_for_owned_completed_action(HISTORY, "[Close]", &[]);
            s.click(x, y);
        } else {
            s.send_key(b"\x1b");
        }
        s.wait_for_new_cursorless_condition(after,|frame|{
            crate::support::overlay_body(frame,HISTORY).is_some_and(|body|
                body.contains("[View original]")&&body.contains("[F6 Acknowledge locally]")&&body.contains("[Close]")
                &&!body.contains("[Esc Cancel]"))
            &&frame.contents().contains("canceled")&&frame.contents().contains("no mutation")
        },"Exact held returned read canceled; same nonbusy protected receipt and no renewed purpose",Duration::from_secs(8));
        gone(&mut s, &child);
        assert_eq!(metadata(&f), original);
        assert_eq!(unrelated(&f), protected);
        assert_eq!(git_state(&f), git);
        assert_eq!(s.actual_terminal_witness_v4().unwrap(), tty);
        close(&mut s, mouse, HISTORY);
        s.quit_and_assert_restored_terminal(mouse);
        exact(&s, &f, &expected, 1);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        assert_private_intents_unchanged(&f);
        assert_read_only_git(&f);
    }
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_keyboard_held_actual_run_cancel_resize_and_child_reaped() {
    held_open(false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_mouse_held_actual_run_cancel_resize_and_child_reaped() {
    held_open(true);
}
fn returned_uid_loss(mouse: bool) {
    let tail = returned_read(ReadKind::Run, 999);
    let (f, mut s, expected) = prepare_confirmed(mouse, tail);
    let original = metadata(&f);
    let protected = unrelated(&f);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let tty = s.actual_terminal_witness_v4().unwrap();
    let after = s.completed_cells_end();
    action(&mut s, mouse, HISTORY, "[F11 Open run]");
    s.wait_for_command_count(&f, expected.len());
    s.wait_for_new_cursorless_condition(
        after,
        |frame| {
            crate::support::main_footer_visible(frame)
                && crate::support::overlay_body(frame, HISTORY).is_none()
                && frame.contents().contains("private")
                && !frame.contents().contains(SECRET)
                && !frame.contents().contains("WR_124")
        },
        "Fresh returned Run final UID loss retires only old private read/receipt presentation",
        Duration::from_secs(8),
    );
    assert_eq!(metadata(&f), original);
    assert_eq!(unrelated(&f), protected);
    assert_eq!(git_state(&f), git);
    assert_eq!(s.actual_terminal_witness_v4().unwrap(), tty);
    s.quit_and_assert_restored_terminal(mouse);
    exact(&s, &f, &expected, 1);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_private_intents_unchanged(&f);
    assert_read_only_git(&f);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_keyboard_returned_run_final_uid_loss_no_receipt_or_write_reissue(
) {
    returned_uid_loss(false);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_mouse_returned_run_final_uid_loss_no_receipt_or_write_reissue(
) {
    returned_uid_loss(true);
}
