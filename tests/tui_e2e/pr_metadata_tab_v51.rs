//! Actual Metadata-tab transport and same publisher; no Summary fallback.
use super::*;
const LABEL: &str = "metadata-preserved-label";
const MILESTONE: &str = "Actual Metadata milestone";
const RETURN: &str = "Confirmed Conversation • read-only refresh";
const RETURN_CONTROLS: &[&str] = &["[r Refresh PR]", "[Esc Back to Inbox]"];

fn context_parent(field: Field, changed: bool) -> Value {
    let mut raw = parent(field, changed);
    raw["labels"] = json!([{"id":81,"node_id":"LABEL_METADATA_81","name":LABEL}]);
    raw["assignees"] = json!([]);
    raw["milestone"] =
        json!({"id":91,"node_id":"MILESTONE_METADATA_91","number":4,"title":MILESTONE});
    raw["requested_reviewers"] = json!([]);
    raw["requested_teams"] = json!([]);
    raw["locked"] = json!(false);
    raw
}
fn context_issue(field: Field, changed: bool) -> Value {
    let mut raw = context_parent(field, changed);
    raw["id"] = json!(9007);
    raw["node_id"] = json!("ISSUE_METADATA_9007");
    raw["pull_request"] = json!({"url":format!("https://{HOST}/api/v3/repos/{FIRST}/pulls/7"),"html_url":format!("https://{HOST}/{FIRST}/pull/7")});
    raw
}
fn context(field: Field, changed: bool) -> Vec<GhStep> {
    vec![
        get(
            &format!("repos/{FIRST}/pulls/7"),
            context_parent(field, changed),
        ),
        get(
            &format!("repos/{FIRST}/issues/7"),
            context_issue(field, changed),
        ),
        get(&format!("repos/{FIRST}"), repository(0)),
        get(
            &format!("repos/{FIRST}/pulls/7"),
            context_parent(field, changed),
        ),
    ]
}
fn section(field: Field) -> Vec<GhStep> {
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.extend(context(field, false));
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.extend(principal(42));
    assert_eq!(calls.len(), 10);
    calls
}
fn paid(field: Field, fail_context: bool) -> Vec<GhStep> {
    let mut calls = post(field);
    let mut fresh = context(field, true);
    if fail_context {
        let mut changed = context_parent(field, true);
        changed["body"] = json!("CONTRADICTORY_CONTEXT_NEVER_ADOPT");
        fresh[3] = get(&format!("repos/{FIRST}/pulls/7"), changed);
    }
    calls.splice(11..11, fresh);
    assert_eq!(calls.len(), 16);
    if fail_context {
        calls.pop();
    }
    calls
}
fn fresh(field: Field) -> Vec<GhStep> {
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), parent(field, true)));
    calls.extend(post(field)[5..10].iter().cloned());
    calls.extend(context(field, true));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), parent(field, true)));
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.extend(principal(42));
    assert_eq!(calls.len(), 17);
    calls
}
fn visible_metadata(s: &mut Session, mouse: bool, needle: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    for _ in 0..32 {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_screen().is_some_and(|frame| {
            crate::support::main_footer_visible(frame)
                && frame.contents().contains("[u Metadata]")
                && frame.contents().contains(needle)
                && !frame.contents().contains("[Confirm action]")
        }) {
            return;
        }
        let before = s.completed_cells_end();
        if mouse {
            s.wheel(20, 12, true);
        } else {
            s.send_key(b"\x1b[B");
        }
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > before {
                break;
            }
            assert!(
                Instant::now() < until,
                "NEW actual Metadata scroll frame\n{}",
                s.screen()
            );
        }
    }
    panic!(
        "Complete actual Metadata literal absent: {needle}\n{}",
        s.screen()
    );
}
fn journey(mouse: bool, personal: bool, fail_context: bool) {
    let field = if personal {
        Field::Description
    } else {
        Field::Title
    };
    let mut steps = initial_steps(personal);
    let initial = steps.len();
    steps.extend(section(field));
    let section_end = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let opened = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    let previewed = steps.len();
    steps.extend(read_phase(field, false, json!(true)));
    steps.extend(paid(field, fail_context));
    let confirmed = steps.len();
    if fail_context {
        steps.extend(fresh(field));
    }
    let f = fixture(&steps);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    enter(
        &mut s,
        &f,
        personal,
        mouse,
        initial,
        false,
        &steps[..initial],
    );
    let stable = independent(&f);
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[u Metadata]", b"u");
    s.wait_for_command_count(&f, section_end);
    s.wait_for_new_cursorless_condition(
        before,
        |frame| {
            crate::support::main_footer_visible(frame)
                && frame.contents().contains("[u Metadata]")
                && frame.contents().contains("Your repository permissions:")
        },
        "Genuine Metadata section owns SAME original page after ten actual children",
        Duration::from_secs(8),
    );
    visible_metadata(&mut s, mouse, MILESTONE);
    visible_metadata(&mut s, mouse, LABEL);
    exact(&s, &f, &steps[..section_end]);
    s.capture_completed_terminal_v1("metadata-v51-real-section-before-edit");
    open(&mut s, &f, mouse, field, opened);
    fill(&mut s, mouse, field, requested(field));
    resize_form(&mut s, 120, 40, field);
    preview(&mut s, &f, mouse, field, previewed);
    resize_form(&mut s, 80, 24, field);
    resize_form(&mut s, 120, 40, field);
    let before = s.completed_cursorless_end();
    let before_cells = s.completed_cells_end();
    control(
        &mut s,
        mouse,
        &form_title(field),
        "[Confirm action]",
        FOOTER,
    );
    s.wait_for_command_count(&f, confirmed);
    if fail_context {
        form_frame(
            &mut s,
            before_cells,
            RETURN,
            &["DURABLY CONFIRMED", RETURN_CONTROLS[0], RETURN_CONTROLS[1]],
        );
        assert!(!s.screen().contains("CONTRADICTORY_CONTEXT_NEVER_ADOPT"));
        assert_original(&f, field, "confirmed", true);
        let marker_before = marker(&f);
        let encoded_before = draft(&f);
        let after = s.completed_cursorless_end();
        control(&mut s, mouse, RETURN, RETURN_CONTROLS[0], RETURN_CONTROLS);
        s.wait_for_command_count(&f, steps.len());
        s.wait_for_new_cursorless_condition(
            after,
            |frame| {
                crate::support::main_footer_visible(frame)
                    && frame.contents().contains("[u Metadata]")
                    && !frame.contents().contains(RETURN_CONTROLS[0])
            },
            "NEW17 read installs actual B0 Metadata without resend",
            Duration::from_secs(8),
        );
        assert_eq!(marker(&f), marker_before);
        assert_eq!(draft(&f), encoded_before);
        assert_original(&f, field, "confirmed", true);
    } else {
        terminal(&mut s, before, "Confirmed • fresh PR/search observation");
        assert_original(&f, field, "confirmed", false);
    }
    visible_metadata(&mut s, mouse, MILESTONE);
    visible_metadata(&mut s, mouse, LABEL);
    let payload:String=raw(&f).query_row("SELECT payload FROM records WHERE kind='pr-details' AND id='GENUINE_CANONICAL_PR_NODE_1007'",[],|r|r.get(0)).unwrap();
    let payload: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        payload["summary"]["pr"][if field == Field::Title {
            "title"
        } else {
            "body"
        }],
        requested(field)
    );
    let actual = commands(&s, &f);
    assert_eq!(
        actual
            .iter()
            .filter(|row| row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "PATCH"))
            .count(),
        1,
        "One actual remote change only, explicit Refresh is read-only"
    );
    s.capture_completed_terminal_v1("metadata-v51-real-fresh-context");
    let after = s.completed_cursorless_end();
    choose(&mut s, mouse, "[b Back]", b"b");
    s.wait_for_new_cursorless_condition(
        after,
        |frame| {
            crate::support::main_footer_visible(frame) && !frame.contents().contains("[b Back]")
        },
        "Pure Metadata Back has no child",
        Duration::from_secs(8),
    );
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}
#[test]
fn pr_metadata_tab_v51_personal_keyboard_body_from_metadata_fresh_actual_context() {
    journey(false, true, false);
}
#[test]
fn pr_metadata_tab_v51_personal_mouse_body_from_metadata_fresh_actual_context() {
    journey(true, true, false);
}
#[test]
fn pr_metadata_tab_v51_catalogue_keyboard_title_from_metadata_fresh_actual_context() {
    journey(false, false, false);
}
#[test]
fn pr_metadata_tab_v51_catalogue_mouse_title_from_metadata_fresh_actual_context() {
    journey(true, false, false);
}
#[test]
fn pr_metadata_tab_v51_keyboard_confirmed_context_failure_new17_refresh_no_resend() {
    journey(false, true, true);
}
#[test]
fn pr_metadata_tab_v51_mouse_confirmed_context_failure_new17_refresh_no_resend() {
    journey(true, false, true);
}
