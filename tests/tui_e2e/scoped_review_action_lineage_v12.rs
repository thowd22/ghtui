//! Actual three/five-process recovery → explicit New → fresh Preview/Confirm.
//! Strict fixtures, owned live children and independent durable/Git/TTY effects.
//! SOURCE ONLY: no executed/selected/pass counts claimed in this packet.
use super::*;
use rusqlite::OptionalExtension;
const NEW_BODY: &str = "DELIBERATE NEW action text 界 — never replay the protected old body";
fn expected_key(root: &str, encoded: &str) -> String {
    use sha2::{Digest, Sha256};
    let saved: Value = serde_json::from_str(encoded).unwrap();
    let reference: ghtui::pr_write_journal::PrPublicationReference =
        serde_json::from_value(saved["publication"].clone()).unwrap();
    let bytes = serde_json::to_vec(&(
        "ghtui-explicit-new-review-action-v12",
        root,
        Operation::Edit.command(),
        &reference,
    ))
    .unwrap();
    format!("{root}/new-v12/{:x}", Sha256::digest(bytes))
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Scenario {
    Success,
    Back,
    AccountLoss,
    CancelBefore,
    CancelAfter,
    Chain,
    ConfirmedCleanup,
}
impl Scenario {
    fn successful(self) -> bool {
        matches!(self, Self::Success | Self::ConfirmedCleanup)
    }
}
fn payload(step: &mut GhStep) {
    fn replace(value: &mut Value) {
        match value {
            Value::String(text) if text == BODY => *text = NEW_BODY.into(),
            Value::Array(values) => values.iter_mut().for_each(replace),
            Value::Object(values) => values.values_mut().for_each(replace),
            _ => {}
        }
    }
    if !step.stdin.is_empty() {
        let mut value: Value = serde_json::from_slice(&step.stdin).unwrap();
        replace(&mut value);
        step.stdin = serde_json::to_vec(&value).unwrap();
    }
    if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
        replace(&mut value);
        step.stdout = serde_json::to_vec(&value).unwrap();
    }
}
fn fresh_open(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut result = initial();
    result.extend(resource_operation(false));
    bind_steps(&mut result, h);
    result.extend(source_open(h, json!([operation_row(h, Operation::Edit)])));
    let mut selected = opening(h);
    selected.extend(operation_snapshot(h, Operation::Edit));
    selected.extend(closing(h));
    result.extend(selected);
    result.extend(phase(h, Operation::Edit, &[]));
    result
}
fn confirmation(h: &history::GitImagesV1, scenario: Scenario) -> Vec<GhStep> {
    let mut steps = opening(h);
    steps.extend(observation(h, Operation::Edit));
    steps.extend(principal(42));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent(h)));
    steps.extend(observation(h, Operation::Edit));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent(h)));
    steps.extend(closing(h));
    // Actual last prewrite principal is live long enough to inspect the exact
    // SAME database pending row/ref/lineage BEFORE any second mutation.
    steps.last_mut().unwrap().delay_ms = if scenario == Scenario::CancelBefore {
        6_000
    } else {
        600
    };
    if scenario == Scenario::CancelBefore {
        return steps;
    }
    let mut mutation = mutation_v3(h, Operation::Edit);
    payload(&mut mutation);
    steps.push(mutation);
    let mut receipt = receipt(h, Operation::Edit);
    for step in &mut receipt {
        payload(step);
    }
    if matches!(scenario, Scenario::CancelAfter | Scenario::Chain) {
        receipt[0].delay_ms = 6_000;
        receipt.truncate(1);
        steps.extend(receipt);
    } else {
        steps.extend(receipt);
        steps.push(get("user", json!({"id":42,"login":"octocat"})));
        steps.push(get(&format!("repos/{FIRST}"), repository(0)));
        steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent(h)));
        steps.push(get("user", json!({"id":42,"login":"octocat"})));
    }
    steps
}
fn history_and_ack(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = initial();
    bind_steps(&mut steps, h);
    for _ in 0..2 {
        steps.extend(opening(h));
        steps.extend(closing(h));
    }
    steps.extend(opening(h));
    steps.push(catalogue(json!([operation_row(h, Operation::Edit)])));
    steps.extend(operation_snapshot(h, Operation::Edit));
    steps.extend(closing(h));
    steps.extend(opening(h));
    steps.extend(closing(h));
    steps
}
pub(super) fn steps(h: &history::GitImagesV1, scenario: Scenario) -> Vec<GhStep> {
    let mut steps = fresh_open(h);
    if scenario == Scenario::Back {
        return steps;
    }
    let mut new = phase(h, Operation::Edit, &[]);
    if scenario == Scenario::AccountLoss {
        *new.last_mut().unwrap() = get("user", json!({"id":43,"login":"octocat"}));
        steps.extend(new);
        return steps;
    }
    steps.extend(new);
    steps.extend(phase(h, Operation::Edit, &[]));
    steps.extend(confirmation(h, scenario));
    if scenario == Scenario::Chain {
        steps.extend(history_and_ack(h));
        steps.extend(fresh_open(h));
        steps.extend(phase(h, Operation::Edit, &[]));
    }
    steps
}
fn exact_script(session: &Session, fixture: &Fixture, steps: &[GhStep]) {
    let commands = commands(session, fixture);
    assert_eq!(commands.len(), steps.len());
    for (actual, expected) in commands.iter().zip(steps) {
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(
            actual["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
    }
}
fn original_kept_basis(fixture: &Fixture, original: &(String, String), confirmed: bool) {
    assert_eq!(
        raw(fixture)
            .query_row(
                "SELECT body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource=?2",
                rusqlite::params![HOST, original.0],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        original.1
    );
    let reference: Value = serde_json::from_str(&original.1).unwrap();
    let attempt = reference["publication"]["attempt_id"].as_i64().unwrap();
    assert_eq!(
        raw(fixture)
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [attempt],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        if confirmed {
            "confirmed"
        } else {
            "acknowledged"
        }
    );
}
fn open_notice(session: &mut Session, fixture: &Fixture, mouse: bool, previous: usize) {
    summary_after_restart(session, fixture, mouse, previous);
    choose(session, mouse, "[f Files]", b"f");
    session.wait_for_command_count(fixture, previous + 41);
    ready(session, &["[F10 Reviews]", "Old image unverified"]);
    choose(session, mouse, "[N Next file]", b"N");
    ready(session, &["> File src/界.rs"]);
    choose(session, mouse, "[F10 Reviews]", b"\x1b[21~");
    overlay_ready(
        session,
        "Scoped reviews",
        &["[R Resume selected]", "[F3 Edit pending]"],
    );
    let before = session.completed_cursorless_end();
    choose(session, mouse, "[R Resume selected]", b"R");
    browser_ready(session, before, "Actual existing server pending comment");
    let before = session.completed_cursorless_end();
    choose(session, mouse, "[F3 Edit pending]", b"\x1bOR");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Scoped reviews")
                && text.contains("Protected original action")
                && text.contains("[N New action]")
                && text.contains("[Esc Back]")
                && !text.contains(BODY)
                && !text.contains(NEW_BODY)
        },
        "Actual exact protected action notice; no old body/form/approval restored",
        Duration::from_secs(8),
    );
}
fn explicit_new(session: &mut Session, mouse: bool) {
    if mouse {
        let point =
            session.wait_for_owned_completed_action("Scoped reviews", "[N New action]", &[]);
        session.click(point.0, point.1);
    } else {
        session.focus_completed_owned_action_v1(
            "Scoped reviews",
            "[N New action]",
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &["[N New action]", "[Esc Back]"],
            &[],
        );
        session.send_key(b"\r");
    }
}
fn body_ready(session: &mut Session) {
    let title = format!("{} • {FIRST}#7", Operation::Edit.label());
    session.focus_completed_owned_action_v1(
        &title,
        "[Preview]",
        crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
        &[
            "[Preview]",
            "[External editor]",
            "[Confirm action]",
            "[Keep draft / Close]",
        ],
        &[],
    );
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            text.contains("PR INTENT")
                && text.contains("[Preview]")
                && text.contains("[Confirm action]")
                && text.contains("Actual existing server pending comment")
                && !text.contains(BODY)
                && !text.contains(NEW_BODY)
        },
        "Fresh CURRENT editor after explicit New; no protected prior body replay",
        Duration::from_secs(8),
    );
}
fn wait_gone(original: &ProcessIdentity) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        match process_identity(original.pid) {
            Ok(None) => return,
            Ok(Some(now))
                if (now.uid, now.birth, now.session)
                    != (original.uid, original.birth, original.session) =>
            {
                return
            }
            Ok(Some(_)) => {}
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {}
            Err(error) => panic!("Owned identity observer failed: {error}"),
        }
        assert!(
            Instant::now() < deadline,
            "Exact original child remains live/unproven"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}
fn held(session: &mut Session, fixture: &Fixture, ordinal: usize) -> HeldOriginal {
    session.wait_for_command_count(fixture, ordinal + 1);
    let commands = commands(session, fixture);
    let pid = i32::try_from(commands[ordinal]["pid"].as_u64().unwrap()).unwrap();
    let owned = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
    HeldOriginal::stop(process_identity(pid).unwrap().unwrap(), &owned)
}
fn current_saved(
    fixture: &Fixture,
    root: &(String, String),
    confirmed: bool,
) -> (String, String, i64) {
    let saved:(String,String)=raw(fixture).query_row("SELECT resource,body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'review-operation-v3/%/new-v12/%' ORDER BY updated_at DESC LIMIT 1",[HOST],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let encoded: Value = serde_json::from_str(&saved.1).unwrap();
    assert_eq!(encoded["body"], NEW_BODY);
    let attempt = encoded["publication"]["attempt_id"].as_i64().unwrap();
    assert!(attempt > 0);
    let actual:(String,String,Option<i64>)=raw(fixture).query_row("SELECT base_key_v12,action_tag_v12,parent_ack_attempt_v12 FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",[attempt],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    let old: Value = serde_json::from_str(&root.1).unwrap();
    assert_eq!(
        actual,
        (
            root.0.clone(),
            Operation::Edit.command().into(),
            if confirmed {
                None
            } else {
                Some(old["publication"]["attempt_id"].as_i64().unwrap())
            }
        )
    );
    assert_ne!(saved.0, root.0);
    assert_eq!(saved.0, expected_key(&root.0, &root.1));
    original_kept_basis(fixture, root, confirmed);
    (saved.0, saved.1, attempt)
}
#[allow(clippy::too_many_arguments)]
pub(super) fn continue_after_ack(
    fixture: &Fixture,
    h: &history::GitImagesV1,
    mouse: bool,
    scenario: Scenario,
    previous: usize,
    script: &[GhStep],
    original: &(String, String),
    stable_before: &[String],
    git: &GitState,
    config: &[u8],
) {
    let mut session = launch(fixture, mouse);
    open_notice(&mut session, fixture, mouse, previous);
    let confirmed = scenario == Scenario::ConfirmedCleanup;
    if confirmed {
        session.wait_for_cursorless_condition(|frame| {
            let text = frame.contents();
            text.contains("Confirmed original receipt")
                && !text.contains("Acknowledged original attempt")
                && !text.contains(BODY)
        }, "Fresh source separately labels actual Confirmed cleanup history; no fake ACK or old body", Duration::from_secs(8));
    }
    let notice_count = commands(&session, fixture).len();
    let db_before = sql_state(fixture);
    session.resize_owned_modal(80, 24, "Scoped reviews", &["[N New action]", "[Esc Back]"]);
    session.resize_owned_modal(40, 14, "Scoped reviews", &["[N New action]", "[Esc Back]"]);
    if mouse {
        let point =
            session.wait_for_owned_completed_action("Scoped reviews", "[N New action]", &[]);
        session.click(point.0, point.1);
    } else {
        session.send_key(b"N");
    }
    ready(&mut session, &["Resize to inspect New action"]);
    assert_eq!(commands(&session, fixture).len(), notice_count);
    assert_eq!(sql_state(fixture), db_before);
    session.resize_owned_modal(120, 40, "Scoped reviews", &["[N New action]", "[Esc Back]"]);
    if scenario == Scenario::Back {
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
        overlay_ready(
            &mut session,
            "Scoped reviews",
            &["[R Resume selected]", "[F3 Edit pending]"],
        );
        assert_eq!(sql_state(fixture), db_before);
        assert_eq!(actual_writes(&commands(&session, fixture)), 1);
    } else {
        let before = session.completed_cursorless_end();
        explicit_new(&mut session, mouse);
        if scenario == Scenario::AccountLoss {
            session.wait_for_new_cursorless_condition(
                before,
                |frame| {
                    let text = frame.contents();
                    crate::support::main_footer_visible(frame)
                        && !text.contains("Scoped reviews")
                        && !text.contains("PR INTENT")
                        && !text.contains(BODY)
                        && !text.contains("Actual existing server pending comment")
                },
                "Actual changed numeric UID refuses New before private Form/body effects",
                Duration::from_secs(8),
            );
            assert_eq!(actual_writes(&commands(&session, fixture)), 1);
        } else {
            body_ready(&mut session);
            if mouse {
                session.click_owned_form_field(
                    "PR INTENT • Preview first",
                    "Exact server PENDING comment",
                );
            } else {
                session.send_key(b"\x1b[Z");
            }
            session.send_key(b"\x01");
            session.paste(NEW_BODY);
            ready(&mut session, &[NEW_BODY]);
            let title = format!("{} • {FIRST}#7", Operation::Edit.label());
            control(&mut session, mouse, &title, "[Preview]", true);
            ready(
                &mut session,
                &["READ-ONLY PREVIEW", NEW_BODY, "[Confirm action]"],
            );
            let confirm_start = commands(&session, fixture).len();
            let confirm = confirmation(h, scenario);
            let pre = confirm.iter().position(|step| step.delay_ms > 0).unwrap();
            let end = session.completed_cursorless_end();
            control(&mut session, mouse, &title, "[Confirm action]", true);
            let mut pre_hold = held(&mut session, fixture, confirm_start + pre);
            let saved = current_saved(fixture, original, confirmed);
            assert_eq!(
                actual_writes(&commands(&session, fixture)),
                1,
                "Actual original SQLite terminal basis+Pending+lineage is visible before new POST"
            );
            if scenario == Scenario::CancelBefore {
                choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
            } else {
                pre_hold.resume();
                if matches!(scenario, Scenario::CancelAfter | Scenario::Chain) {
                    let write=confirm.iter().position(|step|actual_writes(&[json!({"stdin_hex":step.stdin.iter().map(|byte|format!("{byte:02x}")).collect::<String>()})])==1).unwrap();
                    let post_hold = held(&mut session, fixture, confirm_start + write + 1);
                    assert_eq!(current_saved(fixture, original, confirmed).1, saved.1);
                    choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
                    session.wait_for_new_cursorless_condition(
                        end,
                        |frame| {
                            let text = frame.contents();
                            crate::support::main_footer_visible(frame)
                                && text.contains("canceled")
                                && !text.contains(NEW_BODY)
                                && !text.contains("PR INTENT")
                        },
                        "Invoked new action cancellation hides original private body; no resend",
                        Duration::from_secs(8),
                    );
                    wait_gone(&post_hold.identity);
                }
            }
            if scenario.successful() {
                session.wait_for_new_cursorless_condition(end,|frame|{let text=frame.contents();crate::support::main_footer_visible(frame)&&text.contains("confirmed")&&!text.contains("PR INTENT")&&!text.contains(NEW_BODY)},
                    "Exact new typed receipt, visibility closure and terminal safe-only source retirement",Duration::from_secs(8));
                assert_eq!(
                    raw(fixture)
                        .query_row(
                            "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                            [saved.2],
                            |r| r.get::<_, String>(0)
                        )
                        .unwrap(),
                    "confirmed"
                );
                assert!(Cache::open(&database(fixture))
                    .unwrap()
                    .draft(&Scope::github(HOST, Some(42)).unwrap(), &saved.0)
                    .unwrap()
                    .is_none());
            } else if scenario == Scenario::CancelBefore {
                session.wait_for_new_cursorless_condition(
                    end,
                    |frame| {
                        let text = frame.contents();
                        crate::support::main_footer_visible(frame)
                            && text.contains("canceled")
                            && !text.contains(NEW_BODY)
                            && !text.contains("PR INTENT")
                    },
                    "Pre-invocation New action cancellation has no second remote write",
                    Duration::from_secs(8),
                );
                wait_gone(&pre_hold.identity);
                assert_eq!(actual_writes(&commands(&session, fixture)), 1);
            }
            if !scenario.successful() {
                assert_eq!(
                    raw(fixture)
                        .query_row(
                            "SELECT body FROM drafts WHERE resource=?1",
                            [&saved.0],
                            |r| r.get::<_, String>(0)
                        )
                        .unwrap(),
                    saved.1
                );
            }
            if scenario == Scenario::Chain {
                let restart_at = commands(&session, fixture).len();
                session.quit_and_assert_restored_terminal(mouse);
                drop(session);
                let mut recovery = launch(fixture, mouse);
                summary_after_restart(&mut recovery, fixture, mouse, restart_at);
                choose(&mut recovery, mouse, "[F9 Review history]", b"\x1b[20~");
                overlay_ready(
                    &mut recovery,
                    "Review operation history • inspect only",
                    &["[Enter Inspect]", "[r Refresh]", "[Esc Back]"],
                );
                choose(&mut recovery, mouse, "[Enter Inspect]", b"\r");
                overlay_ready(
                    &mut recovery,
                    "Review operation history • inspect only",
                    &[
                        "Recorded attempt #",
                        "[i Inspect current]",
                        "[a Acknowledge local]",
                    ],
                );
                choose(&mut recovery, mouse, "[i Inspect current]", b"i");
                ready(
                    &mut recovery,
                    &["FRESH CURRENT OUTCOME", "[a Acknowledge local]"],
                );
                let after = recovery.completed_cursorless_end();
                choose(&mut recovery, mouse, "[a Acknowledge local]", b"a");
                recovery.wait_for_new_cursorless_condition(
                    after,
                    |frame| {
                        let text = frame.contents();
                        crate::support::main_footer_visible(frame)
                            && text.contains("locally acknowledged")
                            && !text.contains(NEW_BODY)
                    },
                    "Fresh SAME runner/journal ACK for actual second attempt",
                    Duration::from_secs(8),
                );
                assert_eq!(
                    raw(fixture)
                        .query_row(
                            "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                            [saved.2],
                            |r| r.get::<_, String>(0)
                        )
                        .unwrap(),
                    "acknowledged"
                );
                let next = commands(&recovery, fixture).len();
                recovery.quit_and_assert_restored_terminal(mouse);
                drop(recovery);
                session = launch(fixture, mouse);
                open_notice(&mut session, fixture, mouse, next);
                explicit_new(&mut session, mouse);
                body_ready(&mut session);
                let next_key = expected_key(&original.0, &saved.1);
                assert_ne!(next_key, saved.0);
                let autosave_deadline = Instant::now() + Duration::from_secs(8);
                loop {
                    session.poll_output(Duration::from_millis(20));
                    let body: Option<String> = raw(fixture)
                        .query_row(
                            "SELECT body FROM drafts WHERE resource=?1",
                            [&next_key],
                            |r| r.get(0),
                        )
                        .optional()
                        .unwrap();
                    if let Some(body) = body {
                        let value: Value = serde_json::from_str(&body).unwrap();
                        assert!(value["publication"].is_null());
                        assert_eq!(value["body"], "Actual existing server pending comment");
                        break;
                    }
                    assert!(
                        Instant::now() < autosave_deadline,
                        "Fifth process did not save the exact second derived key from the actual terminal reference"
                    );
                }
                assert_eq!(
                    actual_writes(&commands(&session, fixture)),
                    2,
                    "Five-process repeated chain never replays either protected body or invocation"
                );
                assert_eq!(
                    raw(fixture)
                        .query_row(
                            "SELECT body FROM drafts WHERE resource=?1",
                            [&saved.0],
                            |r| r.get::<_, String>(0)
                        )
                        .unwrap(),
                    saved.1
                );
                let title = format!("{} • {FIRST}#7", Operation::Edit.label());
                let before = session.completed_cursorless_end();
                control(&mut session, mouse, &title, "[Keep draft / Close]", true);
                session.wait_for_new_cursorless_condition(
                    before,
                    |frame| {
                        crate::support::main_footer_visible(frame)
                            && !frame.contents().contains("PR INTENT")
                            && !frame.contents().contains(&title)
                    },
                    "Explicitly keep fifth-process draft and close its modal before Quit",
                    Duration::from_secs(8),
                );
            }
        }
    }
    session.wait_for_command_count(fixture, script.len());
    exact_script(&session, fixture, script);
    original_kept_basis(fixture, original, confirmed);
    assert_eq!(stable(fixture), stable_before);
    assert_eq!(git_state(fixture), *git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    if scenario == Scenario::Back {
        // Back leaves the real reviews browser, whose actual control is Q,
        // not the lowercase main-footer button hidden beneath the modal.
        choose(&mut session, mouse, "[Q Quit]", b"Q");
        session.assert_exit_and_restored(true);
    } else {
        session.quit_and_assert_restored_terminal(mouse);
    }
}
macro_rules! pair {
    ($keyboard:ident,$mouse:ident,$case:ident) => {
        #[test]
        fn $keyboard() {
            journey_with_lineage_v12(
                false,
                Operation::Edit,
                Refusal::RecoveryV11,
                Some(Scenario::$case),
            );
        }
        #[test]
        fn $mouse() {
            journey_with_lineage_v12(
                true,
                Operation::Edit,
                Refusal::RecoveryV11,
                Some(Scenario::$case),
            );
        }
    };
}
pair!(
    pending_reviews_v12_keyboard_explicit_new_confirm,
    pending_reviews_v12_mouse_explicit_new_confirm,
    Success
);
pair!(
    pending_reviews_v12_keyboard_back_and_tiny_no_effect,
    pending_reviews_v12_mouse_back_and_tiny_no_effect,
    Back
);
pair!(
    pending_reviews_v12_keyboard_current_account_loss,
    pending_reviews_v12_mouse_current_account_loss,
    AccountLoss
);
pair!(
    pending_reviews_v12_keyboard_original_prepost_cancel,
    pending_reviews_v12_mouse_original_prepost_cancel,
    CancelBefore
);
pair!(
    pending_reviews_v12_keyboard_invoked_cancel_protected,
    pending_reviews_v12_mouse_invoked_cancel_protected,
    CancelAfter
);
pair!(
    pending_reviews_v12_keyboard_five_process_repeated_chain,
    pending_reviews_v12_mouse_five_process_repeated_chain,
    Chain
);

#[test]
fn pending_reviews_v12_keyboard_confirmed_cleanup_sql_failure_restart_explicit_new_not_ack() {
    journey_with_lineage_v12(
        false,
        Operation::Edit,
        Refusal::ConfirmedCleanupV12,
        Some(Scenario::ConfirmedCleanup),
    );
}
#[test]
fn pending_reviews_v12_mouse_confirmed_cleanup_sql_failure_restart_explicit_new_not_ack() {
    journey_with_lineage_v12(
        true,
        Operation::Edit,
        Refusal::ConfirmedCleanupV12,
        Some(Scenario::ConfirmedCleanup),
    );
}
