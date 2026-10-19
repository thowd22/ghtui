//! Actual Metadata Base changes. The sealed two-response B1 receipt, paid
//! context and later fresh-I17 are independent of the visible page.
use super::*;
const LABEL: &str = "Base Metadata preserved label";
const MILESTONE: &str = "Base Metadata actual milestone";
const RETURN: &str = "Confirmed Conversation • read-only refresh";
const RETURN_CONTROLS: &[&str] = &["[r Refresh PR]", "[Esc Back to Inbox]"];
fn context_parent(changed: bool) -> Value {
    let mut raw = raw_parent(changed);
    raw["labels"] = json!([{"id":81,"node_id":"BASE_LABEL_81","name":LABEL}]);
    raw["assignees"] = json!([]);
    raw["milestone"] = json!({"id":91,"node_id":"BASE_MILESTONE_91","number":4,"title":MILESTONE});
    raw["requested_reviewers"] = json!([]);
    raw["requested_teams"] = json!([]);
    raw["locked"] = json!(false);
    raw
}
fn context(changed: bool) -> Vec<GhStep> {
    let parent = context_parent(changed);
    let mut issue = parent.clone();
    issue["id"] = json!(9007);
    issue["node_id"] = json!("BASE_ISSUE_9007");
    issue["pull_request"] = json!({"url":format!("https://{HOST}/api/v3/repos/{FIRST}/pulls/7"),"html_url":format!("https://{HOST}/{FIRST}/pull/7")});
    vec![
        get(&format!("repos/{FIRST}/pulls/7"), parent.clone()),
        get(&format!("repos/{FIRST}/issues/7"), issue),
        get(&format!("repos/{FIRST}"), repository(0)),
        get(&format!("repos/{FIRST}/pulls/7"), parent),
    ]
}
fn section() -> Vec<GhStep> {
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.extend(context(false));
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.extend(principal(42));
    assert_eq!(calls.len(), 10);
    calls
}
fn choose_actual_member(s: &mut Session, mouse: bool, name: &str) {
    if mouse {
        s.click_visible_action("Filter focused:");
    }
    s.send_key(b"\x01");
    s.paste(name);
    s.wait_for_screen(&format!("Filter focused: {name}"));
    let after = s.completed_cells_end();
    if mouse {
        s.click_visible_action(&format!("◆ eligible · {name}"));
    } else {
        s.send_key(b"\t");
    }
    form_frame(
        s,
        after,
        &format!("{PICKER} · Rows focused"),
        PICKER_CONTROLS,
    );
    control(s, mouse, PICKER, "[Enter Choose]", PICKER_CONTROLS);
    s.wait_for_overlay_close(PICKER);
    s.wait_for_owned_completed_action(&title(), "[Preview]", &[]);
}
fn paid_metadata(fail_context: bool) -> Vec<GhStep> {
    let mut calls = paid();
    let mut context = context(true);
    if fail_context {
        let mut raw = context_parent(true);
        raw["base"]["sha"] = json!("d".repeat(40));
        context[3] = get(&format!("repos/{FIRST}/pulls/7"), raw);
    }
    calls.splice(22..22, context);
    assert_eq!(calls.len(), 27);
    if fail_context {
        calls.pop();
    }
    calls
}
fn refresh_metadata() -> Vec<GhStep> {
    let mut calls = refresh();
    calls.splice(9..9, context(true));
    assert_eq!(calls.len(), 17);
    calls
}
fn focused_field(frame: &vt100::Screen) -> Option<usize> {
    use crate::support::completed_navigation_controls_v1::owned_action_position;
    let title = title();
    crate::support::overlay_body(frame, &title)?;
    let (_, footer) = owned_action_position(frame, &title, "[Esc Close]", &[])
        .or_else(|| owned_action_position(frame, &title, "[Keep draft / Close]", &[]))?;
    let mut focused = None;
    for (index, label) in [
        "Selected existing base branch (F2 Choose)",
        "Exact observed selected tip (read only)",
    ]
    .iter()
    .enumerate()
    {
        let Some((x, heading)) = owned_action_position(frame, &title, label, &[]) else {
            continue;
        };
        let row = heading.checked_add(1)?;
        if row < footer
            && frame.cell(row, x).is_some_and(|c| c.underline())
            && focused.replace(index).is_some()
        {
            return None;
        }
    }
    focused
}
fn base_control(s: &mut Session, mouse: bool, label: &str, controls: &[&str]) {
    use crate::support::completed_navigation_controls_v1::{owned_action_position, owned_focus_v1};
    let title = title();
    let until = Instant::now() + Duration::from_secs(8);
    if !mouse {
        for _ in 0..24 {
            s.poll_output(Duration::from_millis(1));
            let frame = s.completed_cells_screen().unwrap();
            let old = owned_focus_v1(
                frame,
                &title,
                label,
                FocusAttributeV1::Inverse,
                controls,
                &[],
            )
            .expect("Complete current Base control set");
            if old.target_focused {
                break;
            }
            let field = focused_field(frame);
            let after = s.completed_cells_end();
            s.send_key(b"\t");
            loop {
                s.poll_output(Duration::from_millis(1));
                if s.completed_cells_end() > after
                    && s.completed_cells_screen().is_some_and(|frame| {
                        owned_focus_v1(
                            frame,
                            &title,
                            label,
                            FocusAttributeV1::Inverse,
                            controls,
                            &[],
                        )
                        .is_some_and(|new| {
                            new.acknowledges_change_from(&old)
                                || new.target == old.target
                                    && focused_field(frame) != field
                                    && (focused_field(frame).is_some() || field.is_some())
                        })
                    })
                {
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "NEW actual Base field/footer after Tab\n{}",
                    s.screen()
                );
            }
        }
    }
    loop {
        s.poll_output(Duration::from_millis(1));
        let point = s.completed_cells_screen().and_then(|f| {
            owned_action_position(f, &title, label, &[]).filter(|(x, y)| {
                f.cell(*y, *x).is_some_and(|c| {
                    (mouse || c.inverse()) && (label != "[Confirm action]" || c.underline())
                })
            })
        });
        if let Some((x, y)) = point {
            if mouse {
                s.click(x, y)
            } else {
                s.send_key(b"\r")
            };
            return;
        }
        assert!(
            Instant::now() < until,
            "Exact useful Base control {label}\n{}",
            s.screen()
        );
    }
}
fn resize(s: &mut Session, cols: u16, rows: u16) {
    let title = title();
    let old = s.completed_cells_screen().map(|f| f.size());
    let after = s.completed_cells_end();
    s.resize(cols, rows);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if (old == Some((rows, cols)) || s.completed_cells_end() > after)
            && s.completed_cells_screen().is_some_and(|f| {
                f.size() == (rows, cols)
                    && crate::support::main_footer_visible(f)
                    && crate::support::overlay_body(f, &title).is_some_and(|body| {
                        body.lines().any(|line| {
                            let mut rest = line;
                            PREVIEW_CONTROLS.iter().all(|label| {
                                if let Some(n) = rest.find(label) {
                                    rest = &rest[n + label.len()..];
                                    true
                                } else {
                                    false
                                }
                            })
                        })
                    })
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW centered Base Form and three whole controls {cols}x{rows}\n{}",
            s.screen()
        );
    }
}
fn metadata(s: &mut Session, mouse: bool, needle: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    for _ in 0..32 {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_screen().is_some_and(|f| {
            crate::support::main_footer_visible(f)
                && crate::support::controls::control_painted(f, "[u Metadata]")
                && f.contents().contains(needle)
                && !f.contents().contains("[Confirm action]")
        }) {
            return;
        }
        let after = s.completed_cells_end();
        if mouse {
            s.wheel(20, 12, true)
        } else {
            s.send_key(b"\x1b[B")
        };
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after {
                break;
            }
            assert!(
                Instant::now() < until,
                "NEW actual Metadata scroll\n{}",
                s.screen()
            );
        }
    }
    panic!(
        "Actual full Metadata literal absent {needle}\n{}",
        s.screen()
    );
}
fn exact_base_sql(f: &Fixture, retained: bool) {
    state(f, "confirmed", retained);
    let db = raw(f);
    let attempt: i64 = db
        .query_row("SELECT attempt_id FROM pr_write_attempts", [], |r| r.get(0))
        .unwrap();
    let rows:Vec<(String,i64)>=db.prepare("SELECT kind,mutation_ordinal FROM sync_mutation_fences WHERE resource='GENUINE_CANONICAL_PR_NODE_1007' AND kind IN ('review','inline_comment') ORDER BY kind").unwrap().query_map([],|r|Ok((r.get(0)?,r.get(1)?))).unwrap().map(Result::unwrap).collect();
    assert_eq!(
        rows,
        vec![
            ("inline_comment".into(), attempt),
            ("review".into(), attempt)
        ]
    );
    let projection:Value=serde_json::from_str(&db.query_row("SELECT payload FROM records WHERE kind='pr-details' AND id='GENUINE_CANONICAL_PR_NODE_1007'",[],|r|r.get::<_,String>(0)).unwrap()).unwrap();
    assert_eq!(projection["summary"]["base_ref"], "next");
    assert_eq!(projection["summary"]["base_sha"], TIP);
    assert_eq!(projection["summary"]["head_sha"], "a".repeat(40));
}
fn journey(mouse: bool, personal: bool, fail_context: bool) {
    let mut steps = initial_steps(personal);
    let initial = steps.len();
    steps.extend(section());
    let section_end = steps.len();
    steps.extend(catalogue(json!(true)));
    let opened = steps.len();
    steps.extend(selected());
    let prepared = steps.len();
    steps.extend(paid_metadata(fail_context));
    let confirmed = steps.len();
    if fail_context {
        steps.extend(refresh_metadata());
    }
    let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[u Metadata]", b"u");
    s.wait_for_command_count(&f, section_end);
    s.wait_for_new_cursorless_condition(
        before,
        |f| {
            crate::support::main_footer_visible(f)
                && crate::support::controls::control_painted(f, "[u Metadata]")
                && f.contents().contains("Your repository permissions:")
        },
        "Genuine Metadata section10 before Base",
        Duration::from_secs(8),
    );
    metadata(&mut s, mouse, MILESTONE);
    metadata(&mut s, mouse, LABEL);
    exact(&s, &f, &steps[..section_end]);
    enter_base(&mut s, &f, mouse, opened);
    choose_actual_member(&mut s, mouse, "next");
    let before = s.completed_cells_end();
    base_control(&mut s, mouse, "[Preview]", FORM_CONTROLS);
    s.wait_for_command_count(&f, prepared);
    form_frame(
        &mut s,
        before,
        &title(),
        &["READ-ONLY PREVIEW", "AFTER Base next", "[Confirm action]"],
    );
    resize(&mut s, 80, 24);
    resize(&mut s, 120, 40);
    s.capture_completed_terminal_v1("metadata-base-v51-actual-preview");
    let before = s.completed_cells_end();
    base_control(&mut s, mouse, "[Confirm action]", PREVIEW_CONTROLS);
    s.wait_for_command_count(&f, confirmed);
    if fail_context {
        form_frame(
            &mut s,
            before,
            RETURN,
            &["DURABLY CONFIRMED", RETURN_CONTROLS[0], RETURN_CONTROLS[1]],
        );
        state(&f, "confirmed", true);
        let db = raw(&f);
        let marker:String=db.query_row("SELECT json_array(attempt_id,state,payload_sha256,confirmed_json) FROM pr_write_attempts",[],|r|r.get(0)).unwrap();
        let draft: String = db
            .query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'metadata-base-v32/%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let before = s.completed_cursorless_end();
        control(&mut s, mouse, RETURN, RETURN_CONTROLS[0], RETURN_CONTROLS);
        s.wait_for_command_count(&f, steps.len());
        s.wait_for_new_cursorless_condition(
            before,
            |f| {
                crate::support::main_footer_visible(f)
                    && crate::support::controls::control_painted(f, "[u Metadata]")
                    && !f.contents().contains(RETURN_CONTROLS[0])
            },
            "NEW17 actual B1 Metadata context, never expired paid binding",
            Duration::from_secs(8),
        );
        assert_eq!(db.query_row("SELECT json_array(attempt_id,state,payload_sha256,confirmed_json) FROM pr_write_attempts",[],|r|r.get::<_,String>(0)).unwrap(),marker);
        assert_eq!(
            db.query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'metadata-base-v32/%'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            draft
        );
    } else {
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > before
                && s.completed_cells_screen().is_some_and(|f| {
                    crate::support::main_footer_visible(f)
                        && crate::support::controls::control_painted(f, "[u Metadata]")
                        && f.contents()
                            .contains("Confirmed • fresh PR/search observation")
                        && !f.contents().contains("[Confirm action]")
                })
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "Fresh actual B1 Metadata after receipt\n{}",
                s.screen()
            );
        }
    }
    metadata(&mut s, mouse, MILESTONE);
    metadata(&mut s, mouse, LABEL);
    exact_base_sql(&f, fail_context);
    let calls = commands(&s, &f);
    assert_eq!(
        calls
            .iter()
            .filter(|r| r["args"].as_array().unwrap().iter().any(|a| a == "PATCH"))
            .count(),
        1
    );
    s.capture_completed_terminal_v1("metadata-base-v51-fresh-b1-context");
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[b Back]", b"b");
    s.wait_for_new_cursorless_condition(
        before,
        |f| {
            crate::support::main_footer_visible(f)
                && !crate::support::controls::control_painted(f, "[b Back]")
        },
        "Pure Base Metadata Back has no child",
        Duration::from_secs(8),
    );
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}
#[test]
fn pr_metadata_base_v32_metadata_tab_v51_personal_keyboard_sealed_b1_full_context() {
    journey(false, true, false);
}
#[test]
fn pr_metadata_base_v32_metadata_tab_v51_personal_mouse_sealed_b1_full_context() {
    journey(true, true, false);
}
#[test]
fn pr_metadata_base_v32_metadata_tab_v51_catalogue_keyboard_sealed_b1_full_context() {
    journey(false, false, false);
}
#[test]
fn pr_metadata_base_v32_metadata_tab_v51_catalogue_mouse_sealed_b1_full_context() {
    journey(true, false, false);
}
#[test]
fn pr_metadata_base_v32_metadata_tab_v51_keyboard_failed_context_new17_b1_refresh_no_resend() {
    journey(false, true, true);
}
#[test]
fn pr_metadata_base_v32_metadata_tab_v51_mouse_failed_context_new17_b1_refresh_no_resend() {
    journey(true, false, true);
}
fn refusals_from_metadata(mouse: bool) {
    for same in [true, false] {
        let personal = same;
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(section());
        let section_end = steps.len();
        steps.extend(catalogue(json!(true)));
        let opened = steps.len();
        let mut phase = if same {
            read(json!(true), None, Some("main"))
        } else {
            selected()
        };
        if same {
            phase.truncate(5);
        } else {
            phase[5].stdout = serde_json::to_vec(&branch("next", &"d".repeat(40))).unwrap();
            phase.truncate(6);
        }
        steps.extend(phase);
        let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
        let before = s.completed_cursorless_end();
        choose(&mut s, mouse, "[u Metadata]", b"u");
        s.wait_for_command_count(&f, section_end);
        s.wait_for_new_cursorless_condition(
            before,
            |f| {
                crate::support::main_footer_visible(f)
                    && f.contents().contains("Your repository permissions:")
                    && crate::support::controls::control_painted(f, "[u Metadata]")
            },
            "Actual Metadata section before exact no-change/drift",
            Duration::from_secs(8),
        );
        enter_base(&mut s, &f, mouse, opened);
        choose_actual_member(&mut s, mouse, if same { "main" } else { "next" });
        let before = s.completed_cells_end();
        base_control(&mut s, mouse, "[Preview]", FORM_CONTROLS);
        s.wait_for_command_count(&f, steps.len());
        if same {
            form_frame(
                &mut s,
                before,
                &title(),
                &[
                    "Selected Base branch is unchanged • no request sent",
                    "[Esc Close]",
                ],
            );
        } else {
            let until = Instant::now() + Duration::from_secs(8);
            loop {
                s.poll_output(Duration::from_millis(1));
                if s.completed_cells_end() > before
                    && s.completed_cells_screen().is_some_and(|frame| {
                        crate::support::main_footer_visible(frame)
                            && frame.contents().contains("Original Conversation unavailable • nothing resent • draft/journal retained")
                            && crate::support::overlay_body(frame, &title()).is_some_and(|body| {
                                body.contains("Selected existing base branch (F2 Choose):")
                                    && body.contains("next")
                                    && body.contains("Exact observed selected tip (read only):")
                                    && body.contains(&"c".repeat(40))
                                    && body.contains("[Esc Close]")
                                    && !body.contains("READ-ONLY PREVIEW")
                                    && !body.contains("[Confirm action]")
                            })
                    })
                {
                    break;
                }
                assert!(
                    Instant::now() < until,
                    "NEW same-owned selected Base Form and exact global drift refusal\n{}",
                    s.screen()
                );
            }
        }
        assert!(!s.screen().contains("READ-ONLY PREVIEW"));
        assert!(!s.screen().contains("[Confirm action]"));
        let db = raw(&f);
        assert_eq!(
            db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM sync_mutation_fences", [], |r| r
                .get::<_, u64>(0))
                .unwrap(),
            0
        );
        let calls = commands(&s, &f);
        assert!(!calls.iter().any(|row| row["args"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a == "PATCH")));
        base_control(&mut s, mouse, "[Esc Close]", FORM_CONTROLS);
        s.wait_for_overlay_close(&title());
        metadata(&mut s, mouse, LABEL);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
#[test]
fn pr_metadata_base_v32_metadata_tab_v51_keyboard_actual_nochange_and_selected_tip_drift_zero_post()
{
    refusals_from_metadata(false);
}
#[test]
fn pr_metadata_base_v32_metadata_tab_v51_mouse_actual_nochange_and_selected_tip_drift_zero_post() {
    refusals_from_metadata(true);
}
