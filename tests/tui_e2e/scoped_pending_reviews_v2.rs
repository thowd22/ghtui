//! Real binary only: actual Source→Files→review browser/Images batch→shared
//! Form→fresh Preview→explicit Confirm. Strict offline gh scripts, real Git,
//! independent SQLite/command/config/owned-process/TTY effects, both inputs.
use super::*;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession, ProcessIdentity};
#[path = "scoped_review_avatars_v7.rs"]
mod avatars_v7;
#[path = "scoped_review_control_block_v12.rs"]
mod control_block_v12;
#[path = "form_preview_visible_review_v1.rs"]
mod form_preview_visible_v1;
#[path = "scoped_review_local_batch_v10.rs"]
mod local_batch_v10;
#[path = "scoped_review_operations_v3.rs"]
mod operations_v3;

// Complete actual19 controls, in production paint order. Shared observer
// verifies the owned WRAPPED block, never fragments or the old single row.
pub(super) const REVIEW_CONTROLS_V12: [&str; 19] = [
    "[B New batch]",
    "[R Resume selected]",
    "[U Summary]",
    "[C Submit comment]",
    "[A Approve]",
    "[X Request changes]",
    "[D Discard pending]",
    "[I Dismiss: eligibility UNKNOWN]",
    "[< Left]",
    "[> Right]",
    "[Esc Close]",
    "[Q Quit]",
    "[F2 Next comment]",
    "[F3 Edit pending]",
    "[F4 Delete pending]",
    "[G Add selected range]",
    "[F5 Re-request reviewer]",
    "[F6 Teams]",
    "[F7 Re-request team]",
];

const SUMMARY: &str = "Native review summary 界";
const COMMENT: &str = "Exact selected pending review comment 界";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Effect {
    Create,
    Summary,
    Comment,
    Approve,
    Changes,
    Discard,
    Dismiss,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    None,
    CancelBefore,
    CancelAfter,
    ExternalAfter,
    AccountBefore,
    StaleSubmit,
    UnknownAuthor,
    MissingSubmission,
    DeniedDismiss,
}
struct HeldOriginal {
    identity: ProcessIdentity,
    stopped: bool,
}
impl HeldOriginal {
    fn stop(identity: ProcessIdentity, owned: &OwnedPtySession) -> Self {
        assert!(identity.pid > 0 && identity.pid != std::process::id() as i32);
        assert!(owned.owns(&identity));
        let mut held = Self {
            identity,
            stopped: false,
        };
        held.signal(libc::SIGSTOP);
        held.stopped = true;
        held
    }
    fn signal(&self, signal: i32) {
        let now = process_identity(self.identity.pid)
            .unwrap()
            .expect("same original live subprocess identity before every signal");
        assert_eq!(
            (now.uid, now.birth, now.session, now.pid),
            (
                self.identity.uid,
                self.identity.birth,
                self.identity.session,
                self.identity.pid
            )
        );
        assert!(!now.zombie);
        // SAFETY: positive exact current UID/birth/SID revalidated above, never
        // a name, command/path match, process group or stale reused PID.
        assert_eq!(unsafe { libc::kill(now.pid, signal) }, 0);
    }
    fn resume(&mut self) {
        self.signal(libc::SIGCONT);
        self.stopped = false;
    }
}
impl Drop for HeldOriginal {
    fn drop(&mut self) {
        if self.stopped {
            if let Ok(Some(now)) = process_identity(self.identity.pid) {
                if !now.zombie
                    && (now.uid, now.birth, now.session, now.pid)
                        == (
                            self.identity.uid,
                            self.identity.birth,
                            self.identity.session,
                            self.identity.pid,
                        )
                {
                    // SAFETY: same exact current original identity, solely owned cleanup.
                    unsafe { libc::kill(now.pid, libc::SIGCONT) };
                }
            }
        }
    }
}
fn parent(h: &history::GitImagesV1) -> Value {
    let mut value = parent_resource();
    h.bind_parent(&mut value);
    value
}
fn opening(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent(h)));
    steps
}
fn closing(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = vec![
        get(&format!("repos/{FIRST}/pulls/7"), parent(h)),
        get(&format!("repos/{FIRST}"), repository(0)),
    ];
    steps.extend(principal(42));
    steps
}
fn row(h: &history::GitImagesV1, effect: Effect, failure: Failure, after: bool) -> Value {
    let state = if effect == Effect::Dismiss {
        if after {
            "DISMISSED"
        } else {
            "APPROVED"
        }
    } else if after {
        match effect {
            Effect::Comment => "COMMENTED",
            Effect::Approve => "APPROVED",
            Effect::Changes => "CHANGES_REQUESTED",
            _ => "PENDING",
        }
    } else {
        "PENDING"
    };
    let body = if after && !matches!(effect, Effect::Dismiss | Effect::Discard) {
        if effect == Effect::Approve {
            ""
        } else {
            SUMMARY
        }
    } else {
        "Actual server review body"
    };
    let old_head = effect == Effect::Discard || failure == Failure::StaleSubmit;
    let mut value = json!({"id":810,"node_id":"ACTUAL_NATIVE_REVIEW_810","state":state,"body":body,"commit_id":if old_head{&h.m}else{&h.h},
        "pull_request_url":format!("https://{HOST}/api/v3/repos/{FIRST}/pulls/7"),"user":{"id":if effect==Effect::Dismiss{51}else{42},"login":if effect==Effect::Dismiss{"actual-review-author"}else{"octocat"},"type":"User"}});
    if state != "PENDING" {
        value["submitted_at"] = if failure == Failure::MissingSubmission {
            Value::Null
        } else {
            json!("2026-10-15T00:00:00Z")
        };
    }
    value
}
fn comments(h: &history::GitImagesV1, effect: Effect, failure: Failure, side: &str) -> Value {
    if effect == Effect::Create {
        json!([{"id":910,"node_id":"ACTUAL_NATIVE_COMMENT_910","pull_request_review_id":810,
        "pull_request_url":format!("https://{HOST}/api/v3/repos/{FIRST}/pulls/7"),"user":{"id":42,"login":"octocat","type":"User"},
        "path":history::PATH,"side":side,"line":2,"start_line":1,"start_side":side,"body":COMMENT,"commit_id":h.h,"in_reply_to_id":null}])
    } else {
        json!([{"id":911,"node_id":"ACTUAL_NATIVE_COMMENT_911","pull_request_review_id":810,
        "pull_request_url":format!("https://{HOST}/api/v3/repos/{FIRST}/pulls/7"),"user":{"id":if effect==Effect::Dismiss{51}else{42},"login":if effect==Effect::Dismiss{"actual-review-author"}else{"octocat"},"type":"User"},
        "path":history::PATH,"side":"RIGHT","line":2,"start_line":1,"start_side":"RIGHT","body":"Actual existing server pending comment",
        "commit_id":if effect==Effect::Discard || failure==Failure::StaleSubmit{&h.m}else{&h.h},"in_reply_to_id":null}])
    }
}
fn catalogue(rows: Value) -> GhStep {
    get(
        &format!("repos/{FIRST}/pulls/7/reviews?per_page=100&page=1"),
        rows,
    )
}
fn source_open(h: &history::GitImagesV1, rows: Value) -> Vec<GhStep> {
    let mut steps = opening(h);
    steps.push(catalogue(rows));
    steps.extend(closing(h));
    steps
}
fn snapshot(
    h: &history::GitImagesV1,
    effect: Effect,
    failure: Failure,
    after: bool,
    side: &str,
) -> Vec<GhStep> {
    let actual = row(h, effect, failure, after);
    vec![
        get(
            &format!("repos/{FIRST}/pulls/7/reviews/810"),
            actual.clone(),
        ),
        get(
            &format!("repos/{FIRST}/pulls/7/reviews/810/comments?per_page=100&page=1"),
            comments(h, effect, failure, side),
        ),
        get(&format!("repos/{FIRST}/pulls/7/reviews/810"), actual),
    ]
}
fn selection(
    h: &history::GitImagesV1,
    effect: Effect,
    failure: Failure,
    side: &str,
) -> Vec<GhStep> {
    let mut steps = opening(h);
    steps.extend(snapshot(h, effect, failure, false, side));
    steps.extend(closing(h));
    steps
}
fn mutation(h: &history::GitImagesV1, effect: Effect, failure: Failure, side: &str) -> GhStep {
    let endpoint = format!("repos/{FIRST}/pulls/7/reviews");
    let (endpoint, method, payload) = match effect {
        Effect::Create => (
            endpoint,
            "POST",
            Some(
                json!({"body":SUMMARY,"commit_id":h.h,"comments":[{"path":history::PATH,"side":side,"line":2,"start_line":1,"start_side":side,"body":COMMENT}]}),
            ),
        ),
        Effect::Summary => (
            format!("{endpoint}/810"),
            "PUT",
            Some(json!({"body":SUMMARY})),
        ),
        Effect::Comment | Effect::Approve | Effect::Changes => (
            format!("{endpoint}/810/events"),
            "POST",
            Some(
                json!({"event":match effect{Effect::Approve=>"APPROVE",Effect::Changes=>"REQUEST_CHANGES",_=>"COMMENT"},"body":if effect==Effect::Approve{""}else{SUMMARY}}),
            ),
        ),
        Effect::Discard => (format!("{endpoint}/810"), "DELETE", None),
        Effect::Dismiss => (
            format!("{endpoint}/810/dismissals"),
            "PUT",
            Some(json!({"message":SUMMARY,"event":"DISMISS"})),
        ),
    };
    let mut args = vec!["api", &endpoint, "--hostname", HOST, "--method", method];
    if payload.is_some() {
        args.extend(["--input", "-"]);
    }
    let mut step = GhStep::read(
        &args,
        &row(h, effect, failure, effect != Effect::Discard).to_string(),
    );
    if let Some(payload) = payload {
        step.stdin = serde_json::to_vec(&payload).unwrap();
    }
    step
}
fn confirm(
    h: &history::GitImagesV1,
    effect: Effect,
    failure: Failure,
    side: &str,
    images: &[GhStep],
) -> (Vec<GhStep>, usize, Option<usize>) {
    let mut steps = opening(h);
    if effect == Effect::Create {
        // The immutable producer has its OWN opening raw parent, in addition
        // to this review phase's parent. Preserve both genuine reads.
        steps.extend(images[3..images.len() - 3].iter().cloned());
    } else {
        steps.extend(snapshot(h, effect, failure, false, side));
    }
    let pre_guard = steps.len();
    steps.extend(principal(42));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent(h)));
    if effect == Effect::Create {
        steps.push(catalogue(json!([])));
        steps.push(get(
            &format!("repos/{FIRST}/pulls/7/files?per_page=100&page=1"),
            json!([h.file()]),
        ));
    } else {
        steps.extend(snapshot(h, effect, failure, false, side));
    }
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent(h)));
    steps.extend(closing(h));
    if failure == Failure::AccountBefore {
        let last = steps.len() - 1;
        steps[last] = get("user", json!({"id":43,"login":"octocat"}));
        return (steps, usize::MAX, None);
    }
    if failure == Failure::CancelBefore {
        steps[pre_guard].delay_ms = 6_000;
        steps.truncate(pre_guard + 1);
        return (steps, usize::MAX, Some(pre_guard));
    }
    let write = steps.len();
    steps.push(mutation(h, effect, failure, side));
    if failure == Failure::DeniedDismiss {
        steps[write].stdout.clear();
        steps[write].status = 1;
        steps[write].stderr = b"HTTP 403: actual server-enforced review dismissal denied".to_vec();
        return (steps, write, None);
    }
    if effect == Effect::Discard {
        steps.push(catalogue(json!([])));
    } else {
        steps.extend(snapshot(h, effect, failure, true, side));
    }
    if failure == Failure::CancelAfter {
        steps[write + 1].delay_ms = 6_000;
        steps.truncate(write + 2);
        return (steps, write, Some(write + 1));
    }
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), parent(h)));
    if failure == Failure::ExternalAfter {
        let held = steps.len() - 1;
        steps[held].delay_ms = 6_000;
        return (steps, write, Some(held));
    }
    steps.push(get("user", json!({"id":42,"login":"octocat"})));
    (steps, write, None)
}
fn foreign_draft(fixture: &Fixture) {
    Cache::open(&database(fixture))
        .unwrap()
        .put_draft(
            &Scope::github(HOST, Some(43)).unwrap(),
            "review-draft/foreign-account-kept",
            "Foreign UID43 review draft must never publish or disappear",
        )
        .unwrap();
}
fn stable_drafts(fixture: &Fixture) -> Vec<String> {
    raw(fixture).prepare("SELECT json_array(host,login,resource,body) FROM drafts WHERE NOT(host=?1 AND login='user-id:42' AND resource LIKE 'review-draft/%') ORDER BY host,login,resource").unwrap()
    .query_map([HOST],|row|row.get(0)).unwrap().map(Result::unwrap).collect()
}
fn exact_commands(session: &Session, fixture: &Fixture, steps: &[GhStep], writes: usize) {
    let rows = commands(session, fixture);
    assert_eq!(rows.len(), steps.len());
    for (ordinal, (actual, step)) in rows.iter().zip(steps).enumerate() {
        assert_eq!(actual["step"], ordinal);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(step.args));
        assert_eq!(
            actual["stdin_hex"],
            step.stdin
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(
        rows.iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| matches!(arg.as_str(), Some("POST" | "PUT" | "PATCH" | "DELETE"))))
            .count(),
        writes
    );
}
fn edited(session: &mut Session, value: &str) {
    edited_field(session, "Review summary:", value)
}
fn edited_field(session: &mut Session, label: &str, value: &str) {
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("PR INTENT • Preview first")
                    && screen.contains(label)
                    && screen.contains(value)
                    && screen.contains("[Preview]")
                    && screen.contains("[Confirm action]")
                    && screen.contains("[Keep draft / Close]")
                    // The closing requester is distinct from a submitted
                    // review's actual author. Its shared Form context shows
                    // login + UID, not an invented @author label.
                    && screen.lines().any(|line| {
                        line.contains("octocat") && line.contains("numeric UID Some(42)")
                    })
            },
            "Actual original review editor fields/current body/full shared controls",
            Duration::from_secs(8),
        )
        .unwrap();
}
fn browser_ready(session: &mut Session, before: usize, fragment: &str) {
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Scoped reviews")
                && text.contains("[B New batch]")
                && text.contains("[R Resume selected]")
                && text.contains("[I Dismiss: eligibility UNKNOWN]")
                && text.contains(fragment)
        },
        "NEW completed original review browser frame, full useful controls and actual observed row",
        Duration::from_secs(8),
    );
}
fn effect_choice(effect: Effect) -> (&'static str, &'static [u8]) {
    match effect {
        Effect::Summary => ("[U Summary]", b"U"),
        Effect::Comment => ("[C Submit comment]", b"C"),
        Effect::Approve => ("[A Approve]", b"A"),
        Effect::Changes => ("[X Request changes]", b"X"),
        Effect::Discard => ("[D Discard pending]", b"D"),
        Effect::Dismiss => ("[I Dismiss: eligibility UNKNOWN]", b"I"),
        Effect::Create => ("[B New batch]", b"B"),
    }
}

fn form_title(effect: Effect) -> String {
    let action = match effect {
        Effect::Create => "Create server PENDING review — not submitted",
        Effect::Summary => "Update pending summary",
        Effect::Comment => "Submit comment review",
        Effect::Approve => "Submit approval",
        Effect::Changes => "Submit change request",
        Effect::Discard => "Discard exact server pending review",
        Effect::Dismiss => "Request dismissal — server eligibility UNKNOWN",
    };
    // The action label is honestly repeated in the requester and Preview
    // body. Only the complete actual header identifies ONE owning box.
    format!("{action} • {FIRST}#7")
}

fn form_control(session: &mut Session, mouse: bool, effect: Effect, label: &str) {
    let title = form_title(effect);
    if mouse {
        let point = session.wait_for_owned_completed_action(&title, label, &[]);
        session.click(point.0, point.1);
    } else {
        session.focus_completed_owned_action_v1(
            &title,
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &[
                "[Preview]",
                "[External editor]",
                "[Confirm action]",
                "[Keep draft / Close]",
            ],
            &[],
        );
        session.send_key(b"\r");
    }
}

/// Match all visible non-spacing text INSIDE the actual completed owned box,
/// across physical wrap boundaries. This does not decide stored body equality:
/// every original UTF-8 byte remains independently asserted in SQLite below.
fn owned_wrapped_text(frame: &vt100::Screen, title: &str, expected: &str) -> bool {
    let Some(body) = crate::support::overlay_body(frame, title) else {
        return false;
    };
    let visible = body
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    let expected = expected
        .chars()
        .filter(|character| !character.is_whitespace())
        .collect::<String>();
    visible.contains(&expected)
}

fn selected_image_range(session: &mut Session, mouse: bool, side: &str, head: &str) {
    let before = session.completed_cursorless_end();
    choose(
        session,
        mouse,
        if side == "RIGHT" {
            "[N New]"
        } else {
            "[O Old]"
        },
        if side == "RIGHT" { b"N" } else { b"O" },
    );
    let first_selection = format!("Selection {side} 1–1");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Immutable file images")
                && text.contains(&first_selection)
                && text.contains(head)
                && text.contains("[R Range]")
                && text.contains("[V Batch]")
        },
        "NEW actual chosen image side and current row geometry before selection",
        Duration::from_secs(8),
    );
    let before = session.completed_cursorless_end();
    if mouse {
        let prefix = if side == "RIGHT" { "R" } else { "L" };
        let first = session.locate(&format!("{prefix}      1")).unwrap();
        let last = session.locate(&format!("{prefix}      2")).unwrap();
        session.drag(first, last);
    } else {
        session.send_key(b"\x1b[1;2B");
    }
    let complete_selection = format!("Selection {side} 1–2");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Immutable file images")
                && text.contains(&complete_selection)
                && text.contains(head)
        },
        "NEW completed exact two-line image selection, not a stale pane or one-line range",
        Duration::from_secs(8),
    );
    let before = session.completed_cursorless_end();
    choose(session, mouse, "[R Range]", b"R");
    let canonical = format!("READ-ONLY {side} 1–2");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Immutable file images")
                && text.contains(&canonical)
                && text.contains(head)
                && text.contains("[V Batch]")
        },
        "NEW canonical original H-bound exact two-line range before batch capture",
        Duration::from_secs(8),
    );
}

fn journey(mouse: bool, effect: Effect, failure: Failure) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    let images = steps[steps.len() - 17..].to_vec();
    let anchored = effect == Effect::Create;
    if !anchored {
        steps.truncate(steps.len() - 17);
    }
    let actual_rows = if anchored {
        json!([])
    } else {
        json!([row(&h, effect, failure, false)])
    };
    steps.extend(source_open(&h, actual_rows.clone()));
    if !anchored {
        steps.extend(selection(&h, effect, failure, "RIGHT"));
    }
    let disabled = matches!(
        failure,
        Failure::StaleSubmit | Failure::UnknownAuthor | Failure::MissingSubmission
    );
    if !disabled {
        if !anchored {
            steps.extend(source_open(&h, actual_rows));
        }
        let mut preview = opening(&h);
        if anchored {
            preview.extend(images[3..images.len() - 3].iter().cloned());
            preview.push(catalogue(json!([])));
        } else {
            preview.extend(snapshot(&h, effect, failure, false, "RIGHT"));
        }
        preview.extend(closing(&h));
        steps.extend(preview);
    }
    let confirm_start = steps.len();
    let side = if mouse { "RIGHT" } else { "LEFT" };
    let (mut confirmation, write, held) = if disabled {
        (vec![], usize::MAX, None)
    } else {
        confirm(&h, effect, failure, side, &images)
    };
    // Create readback must reflect the actual selected side, not a guessed pane.
    if anchored {
        for step in &mut steps {
            if step
                .args
                .get(1)
                .is_some_and(|endpoint| endpoint.contains("reviews/810/comments"))
            {
                step.stdout = comments(&h, effect, failure, side).to_string().into_bytes();
            }
        }
    }
    steps.append(&mut confirmation);
    if failure == Failure::UnknownAuthor {
        for step in &mut steps {
            if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
                if value.get("head").is_some() && value.get("base").is_some() {
                    value["user"]["id"] = Value::Null;
                    step.stdout = value.to_string().into_bytes();
                }
            }
        }
    }
    fixture.scripted_gh(&steps);
    foreign_draft(&fixture);
    let before_git = git_state(&fixture);
    let before_drafts = stable_drafts(&fixture);
    let configuration = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let git_journal = git_commands(&fixture);
    if anchored {
        choose(&mut session, mouse, "[O Old image]", b"O");
        session.wait_for_command_count(&fixture, 58);
        overlay_ready(
            &mut session,
            "Immutable file images",
            &["[R Range]", "[V Batch]"],
        );
        selected_image_range(&mut session, mouse, side, &h.h);
        choose(&mut session, mouse, "[V Batch]", b"V");
    } else {
        let before = session.completed_cursorless_end();
        choose(&mut session, mouse, "[F10 Reviews]", b"\x1b[21~");
        browser_ready(&mut session, before, "#810");
        let before = session.completed_cursorless_end();
        choose(&mut session, mouse, "[R Resume selected]", b"R");
        browser_ready(
            &mut session,
            before,
            "Actual existing server pending comment",
        );
        if disabled {
            let (label, key) = effect_choice(effect);
            choose(&mut session, mouse, label, key);
            ready(
                &mut session,
                &[match failure {
                    Failure::UnknownAuthor => "author is unknown",
                    Failure::MissingSubmission => "actual submitted",
                    _ => "another H",
                }],
            );
            assert_eq!(commands(&session, &fixture).len(), steps.len());
            assert_eq!(
                raw(&fixture)
                    .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                        .get::<_, i64>(0))
                    .unwrap(),
                0
            );
            exact_commands(&session, &fixture, &steps, 0);
            assert_eq!(stable_drafts(&fixture), before_drafts);
            assert_eq!(git_state(&fixture), before_git);
            assert_eq!(git_commands(&fixture), git_journal);
            choose(&mut session, mouse, "[Esc Close]", b"\x1b");
            session.quit_and_assert_restored_terminal(mouse);
            return;
        }
        let (label, key) = effect_choice(effect);
        choose(&mut session, mouse, label, key);
    }
    if effect == Effect::Discard {
        session.wait_for_screen("Discard exact server pending review");
    } else {
        let label = if effect == Effect::Dismiss {
            "Dismissal reason (server privilege UNKNOWN)"
        } else {
            "Review summary"
        };
        edited_field(
            &mut session,
            &format!("{label}:"),
            if anchored || effect == Effect::Dismiss {
                ""
            } else {
                "Actual server review body"
            },
        );
        if mouse {
            session.click_owned_form_field("PR INTENT • Preview first", label);
        }
        session.send_key(b"\x01");
        if effect == Effect::Approve {
            session.send_key(b"\x7f");
        } else {
            session.paste(SUMMARY);
        }
        if anchored {
            choose(&mut session, mouse, "[F6 Fields↓]", b"\x1b[17~");
            if mouse {
                session.click_owned_form_field(
                    "PR INTENT • Preview first",
                    "Exact selected pending comment",
                );
            }
            session.paste(COMMENT);
        }
    }
    // Tiny active form: stale/direct keyboard cannot prepare or confirm while
    // target/body/controls are clipped. Full restore retains original text.
    session.resize_owned_single_action_modal_v2(
        40,
        12,
        "Original review • resize required",
        "[Esc Close]",
    );
    ready(&mut session, &["resize required", "[Esc Close]"]);
    let no_effect = commands(&session, &fixture).len();
    let tiny = session.completed_cursorless_end();
    if effect != Effect::Discard {
        session.send_key(b"\t");
    }
    session.send_key(b"\r");
    session.wait_for_new_cursorless_condition(
        tiny,
        |frame| {
            frame
                .contents()
                .contains("Resize to inspect current review")
        },
        "Actual keyboard Preview refused on resize-only current form, not a no-op shortcut",
        Duration::from_secs(8),
    );
    assert_eq!(commands(&session, &fixture).len(), no_effect);
    session.resize(if mouse { 80 } else { 120 }, if mouse { 24 } else { 40 });
    if effect != Effect::Discard {
        if anchored {
            edited_field(&mut session, "Exact selected pending comment:", COMMENT);
        } else if effect == Effect::Dismiss {
            edited_field(
                &mut session,
                "Dismissal reason (server privilege UNKNOWN):",
                SUMMARY,
            );
        } else {
            edited(
                &mut session,
                if effect == Effect::Approve {
                    ""
                } else {
                    SUMMARY
                },
            );
        }
    }
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[Preview]", b"\x10");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("READ-ONLY PREVIEW • Confirm action")
                && text.contains("[Confirm action]")
                && text.contains("Current H")
        },
        "NEW completed original review Preview with original H/effect/explicit Confirm",
        Duration::from_secs(8),
    );
    assert_eq!(
        commands(&session, &fixture).len(),
        confirm_start,
        "all expected fresh Preview reads complete before Confirm"
    );
    let poison_frame = session.completed_cursorless_end();
    session.paste("PREPARED_REVIEW_POISON_MUST_NOT_EDIT_OR_RETIRE_PREVIEW");
    session.wait_for_new_cursorless_condition(poison_frame,|frame|{let text=frame.contents();text.contains("READ-ONLY PREVIEW • Confirm action") && text.contains("[Confirm action]") && text.contains("Current H")
        && !text.contains("PREPARED_REVIEW_POISON_MUST_NOT_EDIT_OR_RETIRE_PREVIEW")},"Read-only prepared Preview ignores poison paste without invalidating the exact request or approval control",Duration::from_secs(8));
    assert_eq!(
        commands(&session, &fixture).len(),
        confirm_start,
        "read-only paste adds no transport or replacement Preview"
    );
    let before = session.completed_cursorless_end();
    form_control(&mut session, mouse, effect, "[Confirm action]");
    let mut expected_writes = usize::from(!matches!(
        failure,
        Failure::CancelBefore | Failure::AccountBefore
    ));
    if let Some(relative) = held {
        let ordinal = confirm_start + relative;
        session.wait_for_command_count(&fixture, ordinal + 1);
        let journal = commands(&session, &fixture);
        let pid = i32::try_from(journal[ordinal]["pid"].as_u64().unwrap()).unwrap();
        let original = process_identity(pid)
            .unwrap()
            .expect("actual held original review child must be live");
        let owned = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
        assert!(owned.owns(&original));
        assert!(!original.zombie);
        let mut held = HeldOriginal::stop(original.clone(), &owned);
        let database = raw(&fixture);
        let saved:(String,String)=database.query_row("SELECT resource,body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'review-draft/%' ORDER BY resource LIMIT 1",[HOST],|row|Ok((row.get(0)?,row.get(1)?))).unwrap();
        let body: Value = serde_json::from_str(&saved.1).unwrap();
        assert_eq!(body["version"], 3);
        assert_eq!(body["values"]["summary"], SUMMARY);
        assert!(body.get("approved").is_none());
        assert_eq!(
            database
                .query_row("SELECT state FROM pr_write_attempts", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            if failure == Failure::ExternalAfter {
                "confirmed"
            } else {
                "pending"
            }
        );
        drop(database);
        if failure == Failure::ExternalAfter {
            assert!(!fixture.root.join("cache.sqlite3").exists());
            // Controlled external replacement writes the existing real DB;
            // all independent observations continue to use read-only raw().
            let database = rusqlite::Connection::open_with_flags(
                super::database(&fixture),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
            )
            .unwrap();
            assert_eq!(database.execute("UPDATE drafts SET body=?1 WHERE host=?2 AND login='user-id:42' AND resource=?3",rusqlite::params!["EXTERNAL newer exact draft retained 界",HOST,saved.0]).unwrap(),1);
            held.resume();
        } else {
            choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
            session.wait_for_new_cursorless_condition(before,|frame|{let text=frame.contents();text.contains("canceled") && text.contains("nothing resent") && !text.contains(COMMENT) && !text.contains(SUMMARY) && !text.contains("old-no-newline")},
                "Owned Cancel hides exact original private review content without retry/rollback claim",Duration::from_secs(8));
            let end = Instant::now() + Duration::from_secs(5);
            loop {
                match process_identity(pid).unwrap() {
                    None => break,
                    Some(now) => {
                        assert_eq!(
                            (now.uid, now.birth, now.session),
                            (original.uid, original.birth, original.session)
                        );
                        assert!(Instant::now() < end);
                        session.poll_output(Duration::from_millis(10));
                    }
                }
            }
            let state = if failure == Failure::CancelAfter {
                "uncertain"
            } else {
                "pending"
            };
            session
                .try_wait_for_condition(
                    |_| {
                        raw(&fixture)
                            .query_row("SELECT state FROM pr_write_attempts", [], |row| {
                                row.get::<_, String>(0)
                            })
                            .is_ok_and(|actual| actual == state)
                    },
                    "Exact original pending/uncertain journal retained after actual owned cleanup",
                    Duration::from_secs(5),
                )
                .unwrap();
            let retained: String = raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource=?2",
                    rusqlite::params![HOST, saved.0],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(retained, saved.1);
            expected_writes = usize::from(failure == Failure::CancelAfter);
        }
    }
    if matches!(failure, Failure::CancelBefore | Failure::CancelAfter) {
    } else if failure == Failure::AccountBefore {
        session.wait_for_new_cursorless_condition(
            before,
            |frame| {
                let text = frame.contents();
                text.contains("not started") && !text.contains(SUMMARY) && !text.contains(COMMENT)
            },
            "Changed final numeric principal hides private content before POST",
            Duration::from_secs(8),
        );
        assert_eq!(
            raw(&fixture)
                .query_row("SELECT state FROM pr_write_attempts", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "pending"
        );
    } else if failure == Failure::DeniedDismiss {
        // Mouse stays at80 columns: full recovery guidance must fit without
        // enlarging the screen or accepting a clipped "inspect explicitly".
        session.wait_for_new_cursorless_condition(before,|frame|{let text=frame.contents();text.contains("outcome unknown") && text.contains("inspect explicitly") && !text.contains(SUMMARY) && !text.contains("Actual server review body")},
            "Actual denied invoked dismissal preserves conservative uncertainty, hides original private source and never retries",Duration::from_secs(8));
        assert_eq!(
            raw(&fixture)
                .query_row("SELECT state FROM pr_write_attempts", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "uncertain"
        );
        let retained:String=raw(&fixture).query_row("SELECT body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'review-draft/%'",[HOST],|row|row.get(0)).unwrap();
        let body: Value = serde_json::from_str(&retained).unwrap();
        assert_eq!(body["values"]["summary"], SUMMARY);
        assert!(body["publication"].is_object());
    } else if failure == Failure::ExternalAfter {
        session.wait_for_new_cursorless_condition(
            before,
            |frame| {
                let text = frame.contents();
                text.contains("effect confirmed")
                    && text.contains("source unavailable")
                    && !text.contains("EXTERNAL newer")
                    && !text.contains(SUMMARY)
            },
            "Confirmed effect safe status only; replaced external draft never cleaned up",
            Duration::from_secs(8),
        );
        assert_eq!(raw(&fixture).query_row("SELECT body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'review-draft/%'",[HOST],|row|row.get::<_,String>(0)).unwrap(),"EXTERNAL newer exact draft retained 界");
    } else {
        session.wait_for_new_cursorless_condition(before,|frame|{let text=frame.contents();text.contains("Review #810 effect confirmed") && text.contains("snapshot retired") && !text.contains(SUMMARY) && !text.contains(COMMENT)},
            "Typed receipt+journal+source closure+cleanup+terminal fence returns safe actual reviewID only",Duration::from_secs(8));
        assert_eq!(raw(&fixture).query_row("SELECT count(*) FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'review-draft/%'",[HOST],|row|row.get::<_,i64>(0)).unwrap(),0);
        assert_eq!(
            raw(&fixture)
                .query_row("SELECT state FROM pr_write_attempts", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "confirmed"
        );
    }
    if write != usize::MAX {
        assert!(commands(&session, &fixture)
            .get(confirm_start + write)
            .is_some());
    }
    exact_commands(&session, &fixture, &steps, expected_writes);
    assert_eq!(stable_drafts(&fixture), before_drafts);
    assert_eq!(git_state(&fixture), before_git);
    assert_eq!(git_commands(&fixture), git_journal);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        configuration
    );
    assert_eq!(
        raw(&fixture)
            .query_row(
                "SELECT count(*) FROM pr_write_attempts WHERE principal='user-id:43'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    session.quit_and_assert_restored_terminal(mouse);
}
macro_rules! paired {
    ($keyboard:ident,$mouse:ident,$effect:ident,$failure:ident) => {
        #[test]
        fn $keyboard() {
            journey(false, Effect::$effect, Failure::$failure)
        }
        #[test]
        fn $mouse() {
            journey(true, Effect::$effect, Failure::$failure)
        }
    };
}
paired!(
    pending_reviews_v2_native_keyboard_create_actual_selected_left_batch,
    pending_reviews_v2_native_mouse_create_actual_selected_right_batch,
    Create,
    None
);
paired!(
    pending_reviews_v2_native_keyboard_update_summary,
    pending_reviews_v2_native_mouse_update_summary,
    Summary,
    None
);
paired!(
    pending_reviews_v2_native_keyboard_submit_comment,
    pending_reviews_v2_native_mouse_submit_comment,
    Comment,
    None
);
paired!(
    pending_reviews_v2_native_keyboard_approve,
    pending_reviews_v2_native_mouse_approve,
    Approve,
    None
);
paired!(
    pending_reviews_v2_native_keyboard_request_changes,
    pending_reviews_v2_native_mouse_request_changes,
    Changes,
    None
);
paired!(
    pending_reviews_v2_native_keyboard_discard_old_head_pending,
    pending_reviews_v2_native_mouse_discard_old_head_pending,
    Discard,
    None
);
paired!(
    pending_reviews_v2_native_keyboard_dismiss_unknown_privilege,
    pending_reviews_v2_native_mouse_dismiss_unknown_privilege,
    Dismiss,
    None
);
paired!(
    pending_reviews_v2_native_keyboard_cancel_before_post,
    pending_reviews_v2_native_mouse_cancel_before_post,
    Create,
    CancelBefore
);
paired!(
    pending_reviews_v2_native_keyboard_cancel_after_post,
    pending_reviews_v2_native_mouse_cancel_after_post,
    Create,
    CancelAfter
);
paired!(
    pending_reviews_v2_native_keyboard_external_replacement_after_receipt,
    pending_reviews_v2_native_mouse_external_replacement_after_receipt,
    Create,
    ExternalAfter
);
paired!(
    pending_reviews_v2_native_keyboard_final_numeric_account_loss,
    pending_reviews_v2_native_mouse_final_numeric_account_loss,
    Create,
    AccountBefore
);
paired!(
    pending_reviews_v2_native_keyboard_stale_submit_disabled,
    pending_reviews_v2_native_mouse_stale_submit_disabled,
    Comment,
    StaleSubmit
);
paired!(
    pending_reviews_v2_native_keyboard_unknown_numeric_pr_author_disabled,
    pending_reviews_v2_native_mouse_unknown_numeric_pr_author_disabled,
    Approve,
    UnknownAuthor
);
paired!(
    pending_reviews_v2_native_keyboard_missing_observed_submission_disabled,
    pending_reviews_v2_native_mouse_missing_observed_submission_disabled,
    Dismiss,
    MissingSubmission
);
paired!(
    pending_reviews_v2_native_keyboard_actual_denied_dismissal_unknown_no_retry,
    pending_reviews_v2_native_mouse_actual_denied_dismissal_unknown_no_retry,
    Dismiss,
    DeniedDismiss
);

fn files_after_restart(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    previous: usize,
    steps: &[GhStep],
) {
    // The same strict fixture/journal/SQLite database spans all processes.
    // Do not reset counters to make a repeated startup look like a new fixture.
    // The cumulative journal already includes genuine `gh pr diff` image
    // preparation. The catalogue's generic all-GET journal helper cannot
    // describe this completed review history. Keep the SAME real navigation
    // and verify EVERY previous argv/stdin against its exact fixture instead.
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
    ready(session, &[TITLE, "[s Source]", "[Esc Close]"]);
    exact_commands(session, fixture, &steps[..previous], 0);
    discover(session, mouse);
    session.wait_for_command_count(fixture, previous + 7);
    ready(session, &[TITLE, FIRST]);
    if !mouse {
        session.send_key(b"\t");
    }
    choose(session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_command_count(fixture, previous + 18);
    ready(session, &[ROW_TITLE]);
    choose(session, mouse, "1 Inbox", b"1");
    choose(session, mouse, ROW_TITLE, b"\r");
    session.wait_for_command_count(fixture, previous + 31);
    ready(session, &[DETAIL_TITLE, DETAIL_BODY, "[b Back]"]);
    choose(session, mouse, "[f Files]", b"f");
    session.wait_for_command_count(fixture, previous + 41);
    ready(
        session,
        &[
            "src/界.rs",
            "[O Old image]",
            "Old side: current base-branch tip",
        ],
    );
    choose(session, mouse, "[N Next file]", b"N");
    ready(session, &["> File src/界.rs"]);
}
fn wait_saved_values(
    session: &mut Session,
    fixture: &Fixture,
    summary: &str,
    comment_count: usize,
) -> (String, String) {
    session.try_wait_for_condition(|_|raw(fixture).prepare("SELECT resource,body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'review-draft/%'").unwrap()
        .query_map([HOST],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).unwrap()
        .map(Result::unwrap).any(|(_,encoded)|serde_json::from_str::<Value>(&encoded).is_ok_and(|value|value["version"]==3 && value["values"]["summary"]==summary
            && value["values"]["comments"].as_array().is_some_and(|comments|comments.len()==comment_count) && value["publication"].is_null())),
        "Actual normal autosave ACK reached exact version3 values on original numeric scope/database, not a synthetic restored approval",Duration::from_secs(8)).unwrap();
    raw(fixture).prepare("SELECT resource,body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'review-draft/%'").unwrap()
        .query_map([HOST],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?))).unwrap().map(Result::unwrap)
        .find(|(_,encoded)|serde_json::from_str::<Value>(encoded).is_ok_and(|value|value["values"]["summary"]==summary)).unwrap()
}
fn kept(session: &mut Session, mouse: bool) {
    let before = session.completed_cursorless_end();
    choose(session, mouse, "[Keep draft / Close]", b"\x1b");
    session.wait_for_new_cursorless_condition(
        before,
        |frame| {
            let text = frame.contents();
            text.contains("Composer closed")
                && !text.contains("PR INTENT • Preview first")
                && !text.contains("READ-ONLY PREVIEW • Confirm action")
        },
        "NEW completed actual composer Close; exact local draft retained, no hidden active form",
        Duration::from_secs(8),
    );
}
fn batch_action_restart(mouse: bool) {
    const BATCH: &str = "Original batch body survives selected action/restarts 界";
    const ACTION: &str = "Selected summary has separate original draft key 界";
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    let startup = steps[..steps.len() - 17].to_vec();
    let rows = json!([row(&h, Effect::Summary, Failure::None, false)]);
    // The FIRST real process drafts a genuinely NEW local batch: no server
    // PENDING exists yet. Later independent processes below genuinely observe
    // server PENDING #810 and preserve the separate action/batch draft keys.
    steps.extend(source_open(&h, json!([])));
    let first_end = steps.len();
    for _ in 0..2 {
        steps.extend(startup.iter().cloned());
        steps.extend(source_open(&h, rows.clone()));
        steps.extend(selection(&h, Effect::Summary, Failure::None, "RIGHT"));
        steps.extend(source_open(&h, rows.clone()));
    }
    let second_end = first_end + startup.len() + 9 + 11 + 9;
    let before_preview = steps.len();
    let mut preview = opening(&h);
    preview.extend(snapshot(&h, Effect::Summary, Failure::None, false, "RIGHT"));
    preview.extend(closing(&h));
    steps.extend(preview);
    fixture.scripted_gh(&steps);
    foreign_draft(&fixture);
    let before_git = git_state(&fixture);
    let other_drafts = stable_drafts(&fixture);
    let configuration = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut first = launch(&fixture, mouse);
    files_selected(&mut first, &fixture, mouse);
    let first_git = git_commands(&fixture);
    choose(&mut first, mouse, "[O Old image]", b"O");
    first.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut first,
        "Immutable file images",
        &["[R Range]", "[V Batch]"],
    );
    selected_image_range(
        &mut first,
        mouse,
        if mouse { "RIGHT" } else { "LEFT" },
        &h.h,
    );
    choose(&mut first, mouse, "[V Batch]", b"V");
    edited(&mut first, "");
    if mouse {
        first.click_owned_form_field("PR INTENT • Preview first", "Review summary");
    }
    first.paste(BATCH);
    choose(&mut first, mouse, "[F6 Fields↓]", b"\x1b[17~");
    if mouse {
        first.click_owned_form_field(
            "PR INTENT • Preview first",
            "Exact selected pending comment",
        );
    }
    first.paste(COMMENT);
    edited_field(&mut first, "Exact selected pending comment:", COMMENT);
    let batch = wait_saved_values(&mut first, &fixture, BATCH, 1);
    let batch_value: Value = serde_json::from_str(&batch.1).unwrap();
    let comment = &batch_value["values"]["comments"][0];
    assert_eq!(comment["body"], COMMENT);
    assert_eq!(comment["anchor"]["head"], h.h);
    assert_eq!(comment["anchor"]["merge_base"], h.m);
    assert_eq!(comment["anchor"]["server_path"], history::PATH);
    assert_eq!(comment["anchor"]["first"], 1);
    assert_eq!(comment["anchor"]["last"], 2);
    assert!(batch_value["publication"].is_null());
    kept(&mut first, mouse);
    exact_commands(&first, &fixture, &steps[..first_end], 0);
    assert_eq!(git_commands(&fixture), first_git);
    first.quit_and_assert_restored_terminal(mouse);
    // A second real process loads a genuine new Source+Files and a real server
    // pending review before opening its isolated summary action draft.
    let mut second = launch(&fixture, mouse);
    files_after_restart(&mut second, &fixture, mouse, first_end, &steps);
    let second_git = git_commands(&fixture);
    let before = second.completed_cursorless_end();
    choose(&mut second, mouse, "[F10 Reviews]", b"\x1b[21~");
    browser_ready(&mut second, before, "#810");
    let before = second.completed_cursorless_end();
    choose(&mut second, mouse, "[R Resume selected]", b"R");
    browser_ready(
        &mut second,
        before,
        "Actual existing server pending comment",
    );
    choose(&mut second, mouse, "[U Summary]", b"U");
    edited(&mut second, "Actual server review body");
    if mouse {
        second.click_owned_form_field("PR INTENT • Preview first", "Review summary");
    }
    second.send_key(b"\x01");
    second.paste(ACTION);
    edited(&mut second, ACTION);
    let action = wait_saved_values(&mut second, &fixture, ACTION, 0);
    assert_eq!(action.0, format!("{}/review/810/summary", batch.0));
    assert_eq!(
        raw(&fixture)
            .query_row(
                "SELECT body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource=?2",
                rusqlite::params![HOST, batch.0],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        batch.1
    );
    kept(&mut second, mouse);
    exact_commands(&second, &fixture, &steps[..second_end], 0);
    assert_eq!(git_commands(&fixture), second_git);
    second.quit_and_assert_restored_terminal(mouse);
    // A third real process restores only the exact selected-action text. No
    // saved approval, row ID, canonical anchor or old source is restored.
    let mut third = launch(&fixture, mouse);
    files_after_restart(&mut third, &fixture, mouse, second_end, &steps);
    let third_git = git_commands(&fixture);
    let before = third.completed_cursorless_end();
    choose(&mut third, mouse, "[F10 Reviews]", b"\x1b[21~");
    browser_ready(&mut third, before, "#810");
    let before = third.completed_cursorless_end();
    choose(&mut third, mouse, "[R Resume selected]", b"R");
    browser_ready(&mut third, before, "Actual existing server pending comment");
    choose(&mut third, mouse, "[U Summary]", b"U");
    edited(&mut third, ACTION);
    assert!(!third.screen().contains(BATCH));
    assert!(!third.screen().contains(COMMENT));
    choose(&mut third, mouse, "[Confirm action]", b"\x13");
    assert_eq!(
        commands(&third, &fixture).len(),
        before_preview,
        "restart never restores Preview/Confirm permission"
    );
    assert_eq!(
        raw(&fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    let before = third.completed_cursorless_end();
    choose(&mut third, mouse, "[Preview]", b"\x10");
    third.wait_for_new_cursorless_condition(before,|frame|{let text=frame.contents();text.contains("READ-ONLY PREVIEW • Confirm action") && text.contains("Current H")
        && owned_wrapped_text(frame, &form_title(Effect::Summary), &format!("Exact summary/reason: {ACTION}"))},
        "Only a genuine fresh selected review/comments/source close produces new Preview after restart",Duration::from_secs(8));
    kept(&mut third, mouse);
    exact_commands(&third, &fixture, &steps, 0);
    for expected in [&batch, &action] {
        assert_eq!(
            raw(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource=?2",
                    rusqlite::params![HOST, expected.0],
                    |row| row.get::<_, String>(0)
                )
                .unwrap(),
            expected.1
        );
    }
    assert_eq!(stable_drafts(&fixture), other_drafts);
    assert_eq!(git_state(&fixture), before_git);
    assert_eq!(git_commands(&fixture), third_git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        configuration
    );
    assert_eq!(
        raw(&fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    third.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn pending_reviews_v2_native_keyboard_local_batch_selected_action_restart_isolation() {
    batch_action_restart(false)
}
#[test]
fn pending_reviews_v2_native_mouse_local_batch_selected_action_restart_isolation() {
    batch_action_restart(true)
}
