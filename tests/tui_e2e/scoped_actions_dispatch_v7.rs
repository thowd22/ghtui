//! SOURCE ONLY/UNRUN. Native optional Dispatch through actual Catalogue,
//! exact ref/schema/Preview/Confirm; no DTO-issued source or live mutation.
use super::*;
#[path = "novice_usability_dispatch_v42.rs"]
mod novice_usability_v42;
#[path = "scoped_actions_dispatch_lifecycle_v7.rs"]
mod task54_w7_lifecycle;
#[path = "scoped_actions_dispatch_read_v7.rs"]
mod task54_w7_returned_run;
#[path = "scoped_actions_dispatch_typed_v7.rs"]
mod task54_w7_typed;
const C: &str = "cccccccccccccccccccccccccccccccccccccccc";
const INTENT: &str = "Dispatch original workflow • inspect schema first";
const HISTORY: &str = "Original Dispatch publication • inspect only; no resend";
const SECRET: &str = "PRIVATE_TRANSIENT_DISPATCH_INPUT";
const YAML: &str = "name: PRIVATE_DISPATCH\non:\n  workflow_dispatch:\n    inputs:\n      message:\n        type: string\n        required: true\njobs:\n  test:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo NEVER_RUN_LOCALLY\n";
#[derive(Clone, Copy)]
enum Protocol {
    Compatible,
    Detailed,
}
impl Protocol {
    fn version(self) -> &'static str {
        match self {
            Self::Compatible => "2022-11-28",
            Self::Detailed => "2026-03-10",
        }
    }
    fn detailed(self) -> bool {
        matches!(self, Self::Detailed)
    }
}
fn workflow() -> Value {
    json!({"id":7,"node_id":"WF_7","name":"PRIVATE_DISPATCH","path":".github/workflows/ci.yml","state":"active"})
}
fn get_versioned(endpoint: &str, value: Value, p: Protocol) -> GhStep {
    let mut step = get(endpoint, value);
    step.args.extend([
        "--header".into(),
        format!("X-GitHub-Api-Version: {}", p.version()),
    ]);
    step
}
fn missing(endpoint: &str, p: Protocol) -> GhStep {
    let mut step = get_versioned(endpoint, Value::Null, p);
    step.status = 1;
    step.stdout.clear();
    step.stderr = b"gh: Not Found (HTTP 404)".to_vec();
    step
}
fn reference() -> Value {
    json!({"ref":"refs/heads/main","object":{"type":"commit","sha":C}})
}
fn raw(yaml: &str, p: Option<Protocol>) -> GhStep {
    let mut args = vec![
        "api".to_owned(),
        format!("repos/{CURRENT}/contents/.github/workflows/ci.yml?ref={C}"),
        "--hostname".into(),
        HOST.into(),
        "--method".into(),
        "GET".into(),
        "--header".into(),
        "Accept: application/vnd.github.raw+json".into(),
    ];
    if let Some(p) = p {
        args.extend([
            "--header".into(),
            format!("X-GitHub-Api-Version: {}", p.version()),
        ]);
    }
    args.push("--allow-escape-sequences".into());
    GhStep::read(&args.iter().map(String::as_str).collect::<Vec<_>>(), yaml)
}
fn schema_phase(p: Protocol) -> Vec<GhStep> {
    let body = vec![
        get_versioned("versions", json!(["2022-11-28", "2026-03-10"]), p),
        get_versioned(
            &format!("repos/{CURRENT}/actions/workflows/7"),
            workflow(),
            p,
        ),
        get_versioned(
            &format!("repos/{CURRENT}/git/ref/heads/main"),
            reference(),
            p,
        ),
        missing(&format!("repos/{CURRENT}/git/ref/tags/main"), p),
        get_versioned(&format!("repos/{CURRENT}/commits/{C}"), json!({"sha":C}), p),
        raw(YAML, Some(p)),
        get_versioned(
            &format!("repos/{CURRENT}/git/ref/heads/main"),
            reference(),
            p,
        ),
        missing(&format!("repos/{CURRENT}/git/ref/tags/main"), p),
        get_versioned(
            &format!("repos/{CURRENT}/actions/workflows/7"),
            workflow(),
            p,
        ),
    ];
    let out = resources_v2::resource(body, 42);
    assert_eq!(out.len(), 15);
    out
}
fn opening() -> Vec<GhStep> {
    let mut steps = actions_protocol();
    steps.extend(resources_v2::resource(
        vec![paged(
            &format!("repos/{CURRENT}/actions/workflows?per_page=100"),
            json!({"total_count":1,"workflows":[workflow()]}),
        )],
        42,
    ));
    steps.extend(resources_v2::resource(
        vec![
            get(&format!("repos/{CURRENT}/actions/workflows/7"), workflow()),
            get(&format!("repos/{CURRENT}/commits/main"), json!({"sha":C})),
            raw("name: PRIVATE_DISPATCH\non: [push]\njobs: {}\n", None),
            get(&format!("repos/{CURRENT}/actions/workflows/7"), workflow()),
        ],
        42,
    ));
    steps.extend(resources_v2::resource(
        vec![
            get(&format!("repos/{CURRENT}/actions/workflows/7"), workflow()),
            get(&format!("repos/{CURRENT}/actions/workflows/7"), workflow()),
        ],
        42,
    ));
    assert_eq!(steps.len(), 39);
    steps
}
fn returned_run() -> Value {
    let mut value = fresh_run(CURRENT);
    value["id"] = json!(124);
    value["node_id"] = json!("WR_124");
    value["run_attempt"] = json!(1);
    value["head_sha"] = json!(C);
    value["event"] = json!("workflow_dispatch");
    value["check_suite_id"] = json!(902);
    value["status"] = json!("queued");
    value["conclusion"] = Value::Null;
    value
}
fn confirmation(p: Protocol, uid: u64) -> Vec<GhStep> {
    let phase = schema_phase(p);
    let mut out = principal(42);
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend_from_slice(&phase[3..9]);
    out.extend(principal(42));
    out.push(get_versioned(
        &format!("repos/{CURRENT}/actions/workflows/7"),
        workflow(),
        p,
    ));
    out.extend_from_slice(&phase[9..12]);
    out.push(get(&format!("repos/{FIRST}"), current_repository()));
    out.extend(principal(42));
    let body = json!({"workflow_run_id":124}).to_string();
    let response = if p.detailed() {
        format!(
            "HTTP/2.0 200 OK\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        )
    } else {
        "HTTP/2.0 204 No Content\nContent-Length: 0\r\n\r\n".into()
    };
    let mut post = GhStep::read(
        &[
            "api",
            &format!("repos/{CURRENT}/actions/workflows/7/dispatches"),
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--header",
            &format!("X-GitHub-Api-Version: {}", p.version()),
            "--include",
            "--input",
            "-",
        ],
        &response,
    );
    post.stdin = serde_json::to_vec(&json!({"ref":"main","inputs":{"message":SECRET}})).unwrap();
    out.push(post);
    if p.detailed() {
        out.push(get_versioned(
            &format!("repos/{CURRENT}/actions/runs/124"),
            returned_run(),
            p,
        ));
    }
    out.push(get("user", json!({"id":uid,"login":"octocat"})));
    assert_eq!(out.len(), if p.detailed() { 21 } else { 20 });
    out
}
fn owned(s: &mut Session, after: usize, title: &str, words: &[&str]) {
    let until = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen()
                .and_then(|frame| crate::support::overlay_body(frame, title))
                .is_some_and(|body| {
                    !body.contains("[Esc Cancel]")
                        && words.iter().all(|word| {
                            if matches!(*word, "No actual current POST200 returned-run proof") {
                                crate::owned_form_prose_v120::adjacent_literal(&body, word)
                            } else {
                                // Controls, exact IDs, hashes and input labels
                                // must exist literally in this ONE owned frame.
                                body.contains(word)
                            }
                        })
                })
        {
            return;
        }
        assert!(
            std::time::Instant::now() < until,
            "Actual NEW owned Dispatch body {title} {words:?}\n{}",
            s.screen()
        );
    }
}
fn footer_labels(body: &str) -> Vec<&'static str> {
    let mut labels = if body.contains("[View original]") {
        vec![
            "[View original]",
            "[F6 Acknowledge locally]",
            if body.contains("[F11 Open run]") {
                "[F11 Open run]"
            } else {
                "[F11 Run unavailable]"
            },
            "[Close]",
        ]
    } else {
        vec![
            if body.contains("[Inspect schema]") {
                "[Inspect schema]"
            } else {
                "[Preview]"
            },
            "[Confirm action]",
            if body.contains("[Esc Cancel]") {
                "[Esc Cancel]"
            } else {
                "[Keep draft / Close]"
            },
        ]
    };
    if body.contains("[F7 Error↑]") {
        labels.extend(["[F7 Error↑]", "[F8 Error↓]"]);
    } else if body.contains("[F7 Info↑]") {
        labels.extend(["[F7 Info↑]", "[F8 Info↓]"]);
    }
    labels
}
fn resize_form(s: &mut Session, width: u16, height: u16, title: &str, controls: &[&str]) {
    use crate::support::completed_navigation_controls_v1::{owned_focus_v1, FocusAttributeV1};
    let after = s.completed_cells_end();
    // Keep the actual shared SIGWINCH/global-frame acknowledgement. An active
    // field completes cells at Show, not necessarily a cursorless Hide.
    s.resize(width, height);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                if frame.size() != (height, width)
                    || owned_focus_v1(
                        frame,
                        title,
                        controls[0],
                        FocusAttributeV1::Inverse,
                        controls,
                        &[],
                    )
                    .is_none()
                {
                    return false;
                }
                let Some(body) = crate::support::overlay_body(frame, title) else {
                    return false;
                };
                let primary: Vec<_> = controls
                    .iter()
                    .copied()
                    .filter(|label| !label.starts_with("[F7 ") && !label.starts_with("[F8 "))
                    .collect();
                let help: Vec<_> = controls
                    .iter()
                    .copied()
                    .filter(|label| label.starts_with("[F7 ") || label.starts_with("[F8 "))
                    .collect();
                let help_positions: Option<Vec<_>> = help
                    .iter()
                    .map(|label| {
                        crate::support::completed_navigation_controls_v1::owned_action_position(
                            frame,
                            title,
                            label,
                            &[],
                        )
                    })
                    .collect();
                let bottom =
                    crate::support::completed_navigation_controls_v1::owned_action_position(
                        frame,
                        title,
                        primary[0],
                        &[],
                    )
                    .unwrap()
                    .1;
                body.lines()
                    .last()
                    .is_some_and(|row| row.trim() == primary.join(" "))
                    && help_positions.is_some_and(|points| {
                        points.iter().all(|point| point.1 + 1 == bottom)
                            && points
                                .windows(2)
                                .zip(help.windows(2))
                                .all(|(points, labels)| {
                                    points[0].0 + (labels[0].chars().count() as u16) < points[1].0
                                })
                    })
            })
        {
            return;
        }
        assert!(Instant::now() < until,
            "NEW exact-size topmost Dispatch Form, whole ordered bottom controls and separate help row\n{}", s.screen());
    }
}
fn underlined_field(frame: &vt100::Screen, title: &str) -> Option<(String, (u16, u16, u16))> {
    // completed_cells_screen observes cell diffs at Show/Hide, BEFORE final
    // cursor placement. Only actual cell styling can witness field focus.
    crate::support::overlay_body(frame, title)?;
    let (column, top) = crate::support::locate_screen(frame, title)?;
    let (_, footer) = crate::support::locate_screen(frame, "[Keep draft / Close]")?;
    if top >= footer {
        return None;
    }
    let mut focused = None;
    for label_y in top + 1..footer.saturating_sub(1) {
        let border = (column..frame.size().1).find(|x| {
            frame
                .cell(label_y, *x)
                .is_some_and(|cell| cell.contents() == "│")
        })?;
        let row: String = (column..border)
            .map(|x| {
                frame
                    .cell(label_y, x)
                    .map_or_else(String::new, |cell| cell.contents())
            })
            .collect();
        let Some((label, _)) = row.trim().split_once(':') else {
            continue;
        };
        let label = label.trim();
        let Some((left, value_y)) = crate::support::form_field_value_position(frame, title, label)
        else {
            continue;
        };
        if value_y != label_y + 1 {
            continue;
        }
        // forms::draw uses horizontal Margin(2): the cell immediately before
        // the right border is padding, outside the actual value rectangle.
        let right = border.checked_sub(1)?;
        if left < right
            && (left..right).all(|x| frame.cell(value_y, x).is_some_and(|cell| cell.underline()))
            && focused
                .replace((label.to_owned(), (left, value_y, right - left)))
                .is_some()
        {
            return None; // Never accept ambiguous simultaneously focused fields.
        }
    }
    focused
}
fn tab_once(s: &mut Session, title: &str, target: &str, labels: &[&str], until: Instant) {
    use crate::support::completed_navigation_controls_v1::{owned_focus_v1, FocusAttributeV1};
    let frame = s
        .completed_cells_screen()
        .expect("Actual completed owned Form before Tab");
    let prior = owned_focus_v1(frame, title, target, FocusAttributeV1::Inverse, labels, &[])
        .expect("SAME topmost owned Form and complete target footer");
    let field = underlined_field(frame, title);
    let size = frame.size();
    let after = s.completed_cells_end();
    s.send_key(b"\t");
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                frame.size() == size
                    && owned_focus_v1(frame, title, target, FocusAttributeV1::Inverse, labels, &[])
                        .is_some_and(|next| {
                            next.acknowledges_change_from(&prior)
                                || (next.target == prior.target
                                    && underlined_field(frame, title)
                                        .is_some_and(|next_field| Some(next_field) != field))
                        })
            })
        {
            return;
        }
        assert!(Instant::now()<until,"ONE Tab requires NEW same-owned footer or actual underlined logical field/rectangle change\n{}",s.screen());
    }
}
fn action(s: &mut Session, mouse: bool, title: &str, label: &str) {
    let body = s
        .completed_cells_screen()
        .and_then(|frame| crate::support::overlay_body(frame, title))
        .expect("Actual completed original Dispatch Form");
    let labels = footer_labels(&body);
    if mouse {
        let (x, y) = s.wait_for_owned_completed_action(title, label, &[]);
        s.click(x, y);
    } else {
        let until = Instant::now() + Duration::from_secs(8);
        for _ in 0..60 {
            let frame = s
                .completed_cells_screen()
                .expect("Actual completed owned Form");
            let focus = crate::support::completed_navigation_controls_v1::owned_focus_v1(
                frame,
                title,
                label,
                crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
                &labels,
                &[],
            )
            .expect("Actual topmost owned target");
            if focus.target_focused {
                s.send_key(b"\r");
                return;
            }
            tab_once(s, title, label, &labels, until);
        }
        panic!("Actual inverse target footer not reached before Enter: {label}");
    }
}
fn field(s: &mut Session, mouse: bool, label: &str) {
    if mouse {
        let until = Instant::now() + Duration::from_secs(8);
        let mut forwards = false;
        for _ in 0..60 {
            s.poll_output(Duration::from_millis(1));
            if let Some((point, body)) = s.completed_cells_screen().and_then(|frame| {
                let body = crate::support::overlay_body(frame, INTENT)?;
                Some((
                    crate::support::form_field_value_position(frame, INTENT, label),
                    body,
                ))
            }) {
                if let Some((x, y)) = point {
                    let frame = s.completed_cells_screen().unwrap();
                    let labels = footer_labels(&body);
                    assert!(crate::support::completed_navigation_controls_v1::owned_focus_v1(
                        frame, INTENT, labels[0],
                        crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
                        &labels, &[],
                    ).is_some(), "Actual topmost complete editable Dispatch Form");
                    if underlined_field(frame, INTENT).is_some_and(|(actual, _)| actual == label) {
                        return;
                    }
                    let size = frame.size();
                    let after = s.completed_cells_end();
                    s.click(x, y);
                    loop {
                        s.poll_output(Duration::from_millis(1));
                        if s.completed_cells_end() > after
                            && s.completed_cells_screen().is_some_and(|frame| {
                                frame.size() == size
                                    && crate::support::completed_navigation_controls_v1::owned_focus_v1(
                                    frame, INTENT, labels[0],
                                    crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
                                    &labels, &[],
                                ).is_some()
                                    && underlined_field(frame, INTENT)
                                        .is_some_and(|(actual, _)| actual == label)
                            })
                        {
                            return;
                        }
                        assert!(Instant::now() < until,
                            "ONE field click needs NEW same-owned UNDERLINED logical field before choice geometry\n{}", s.screen());
                    }
                }
                let frame = s.completed_cells_screen().unwrap();
                let prior_field = underlined_field(frame, INTENT).map(|(actual, _)| actual);
                if prior_field.as_deref() == Some("Exact ref namespace") {
                    forwards = true;
                }
                // A later input can request an earlier field. Reach the actual
                // underlined first field with F5, then traverse forwards with
                // F6; never keep clicking a saturated last-field control.
                let scroll = if forwards {
                    "[F6 Fields↓]"
                } else {
                    "[F5 Fields↑]"
                };
                let mut controls = footer_labels(&body);
                controls.extend(["[F5 Fields↑]", "[F6 Fields↓]"]);
                assert!(
                    controls.iter().all(|control| body.contains(control)),
                    "Actual complete editable Dispatch controls before field scroll"
                );
                let prior = crate::support::completed_navigation_controls_v1::owned_focus_v1(
                    frame,
                    INTENT,
                    scroll,
                    crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
                    &controls,
                    &[],
                )
                .expect("SAME completed editable Dispatch Form and complete field controls");
                let size = frame.size();
                let after = s.completed_cells_end();
                // Wheel deliberately scrolls the prepared summary. Editable
                // fields have their own actual shared FieldScroll action.
                let (x, y) = prior.target;
                s.click(x, y);
                loop {
                    s.poll_output(Duration::from_millis(1));
                    if s.completed_cells_end() > after
                        && s.completed_cells_screen().is_some_and(|frame| {
                            frame.size() == size
                                && crate::support::overlay_body(frame, INTENT).is_some_and(|body| {
                                    controls.iter().all(|control| body.contains(control))
                                })
                                && crate::support::completed_navigation_controls_v1::owned_focus_v1(
                                    frame,
                                    INTENT,
                                    scroll,
                                    crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
                                    &controls,
                                    &[],
                                )
                                .is_some_and(|next| next.target == prior.target)
                                && underlined_field(frame, INTENT)
                                    .is_some_and(|(actual, _)| Some(actual) != prior_field)
                        })
                    {
                        break;
                    }
                    assert!(
                        Instant::now() < until,
                        "ONE field scroll needs NEW same-size owned Dispatch controls and changed UNDERLINED logical field\n{}",
                        s.screen()
                    );
                }
            }
            assert!(
                Instant::now() < until,
                "Actual completed owned Dispatch field {label}\n{}",
                s.screen()
            );
        }
        panic!("Actual bounded Dispatch field not visible: {label}");
    }
    let until = std::time::Instant::now() + Duration::from_secs(8);
    for _ in 0..60 {
        let current = s.completed_cells_screen().expect("Actual completed fields");
        let body =
            crate::support::overlay_body(current, INTENT).expect("Actual owned editable Dispatch");
        let labels = footer_labels(&body);
        assert!(
            crate::support::completed_navigation_controls_v1::owned_focus_v1(
                current,
                INTENT,
                labels[0],
                crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
                &labels,
                &[],
            )
            .is_some(),
            "Actual topmost complete editable Dispatch Form"
        );
        if underlined_field(current, INTENT).is_some_and(|(actual, _)| actual == label) {
            return;
        }
        tab_once(s, INTENT, labels[0], &labels, until);
    }
    panic!("Actual Dispatch field not reached: {label}");
}
fn begin(s: &mut Session, f: &Fixture, mouse: bool, p: Protocol) {
    s.resize(120, 40);
    s.wait_for_main_frame();
    let after = s.completed_cursorless_end();
    resources_v2::action(s, mouse, "Workflows", b"W");
    s.wait_for_command_count(f, 21);
    complete(s, after, &["PRIVATE_DISPATCH", ".github/workflows/ci.yml"]);
    let after = s.completed_cursorless_end();
    selected_row(s, mouse, "PRIVATE_DISPATCH");
    s.wait_for_command_count(f, 31);
    complete(s, after, &["Requested ref main", C]);
    let after = s.completed_cells_end();
    if mouse {
        resources_v2::action(s, true, "Dispatch workflow", b"");
    } else {
        s.focus_completed_actions_control_v2("Dispatch workflow");
        s.send_key(b"\r");
    }
    s.wait_for_command_count(f, 39);
    owned(
        s,
        after,
        INTENT,
        &["ORIGINAL WORKFLOW DISPATCH", "[Inspect schema]"],
    );
    if p.detailed() {
        field(s, mouse, "Host API protocol (must be positively supported)");
        let after = s.completed_cells_end();
        if mouse {
            let (x, y) = s.wait_for_owned_completed_action(INTENT, "[2026-03-10]", &[]);
            s.click(x, y);
        } else {
            s.send_key(b" ");
        }
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after
                && s.completed_cells_screen().is_some_and(|frame| {
                    let Some((label, (x, y, width))) = underlined_field(frame, INTENT) else {
                        return false;
                    };
                    let value: String = (x..x + width)
                        .map(|column| {
                            frame
                                .cell(y, column)
                                .map_or_else(String::new, |cell| cell.contents())
                        })
                        .collect();
                    let selected =
                        crate::support::completed_navigation_controls_v1::owned_action_position(
                            frame,
                            INTENT,
                            "[2026-03-10]",
                            &[],
                        );
                    label == "Host API protocol (must be positively supported)"
                        && value.trim() == "2026-03-10"
                        && selected.is_some_and(|(x, y)| {
                            frame.cell(y, x).is_some_and(|cell| cell.inverse())
                        })
                })
            {
                break;
            }
            assert!(Instant::now() < until,
                "Actual protocol VALUE and inverse indexed selection, not an unselected caption\n{}", s.screen());
        }
    }
}
fn inspect_schema(s: &mut Session, f: &Fixture, mouse: bool) {
    let after = s.completed_cells_end();
    action(s, mouse, INTENT, "[Inspect schema]");
    s.wait_for_command_count(f, 54);
    owned(s, after, INTENT, &["EXACT DISPATCH SCHEMA", "[Preview]", C]);
}
fn preview(s: &mut Session, f: &Fixture, mouse: bool) {
    field(
        s,
        mouse,
        "message • transient input value • masked; never saved",
    );
    let after = s.completed_cells_end();
    s.paste(SECRET);
    owned(s, after, INTENT, &["[Preview]"]);
    assert!(!s
        .completed_cells_screen()
        .unwrap()
        .contents()
        .contains(SECRET));
    let after = s.completed_cells_end();
    action(s, mouse, INTENT, "[Preview]");
    s.wait_for_command_count(f, 69);
    owned(
        s,
        after,
        INTENT,
        &["DISPATCH PREVIEW", "[Confirm action]", C],
    );
    assert!(!s
        .completed_cells_screen()
        .unwrap()
        .contents()
        .contains(SECRET));
}
fn metadata(f: &Fixture) -> (String, String, String, String) {
    publication_v3::raw(f).query_row(
        "SELECT state,context_json,options_json,COALESCE(receipt_json,'') FROM actions_write_attempts",
        [],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap()
}
fn unrelated(f: &Fixture) -> Vec<(String, Vec<Vec<rusqlite::types::Value>>)> {
    let mut data = sql(f);
    data.retain(|(name, _)| !name.starts_with("actions_write_") && name != "drafts");
    let db = publication_v3::raw(f);
    let mut query=db.prepare("SELECT host,login,resource,body FROM drafts WHERE resource NOT LIKE 'scoped-actions-v7/%' ORDER BY host,login,resource").unwrap();
    let rows = query
        .query_map([], |row| {
            (0..4)
                .map(|i| row.get(i))
                .collect::<rusqlite::Result<Vec<rusqlite::types::Value>>>()
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    data.push(("unrelated_drafts".into(), rows));
    data
}
fn exact(s: &Session, f: &Fixture, expected: &[GhStep], posts: usize) {
    let rows = commands(s, f);
    assert_eq!(rows.len(), expected.len());
    for (row, step) in rows.iter().zip(expected) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(
        rows.iter()
            .filter(|r| r["args"].as_array().unwrap().iter().any(|a| a == "POST"))
            .count(),
        posts
    );
    assert!(!rows.iter().any(|r| r["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| matches!(a.as_str(), Some("PUT" | "PATCH" | "DELETE")))));
}
fn close(s: &mut Session, mouse: bool, title: &str) {
    let after = s.completed_cells_end();
    action(
        s,
        mouse,
        title,
        if title == HISTORY {
            "[Close]"
        } else {
            "[Keep draft / Close]"
        },
    );
    s.wait_for_new_cursorless_condition(
        after,
        |frame| {
            crate::support::overlay_body(frame, title).is_none()
                && crate::support::main_footer_visible(frame)
        },
        "Actual owned Dispatch Close returns usable host",
        Duration::from_secs(8),
    );
}
fn success(mouse: bool, p: Protocol) {
    let mut expected = opening();
    expected.extend(schema_phase(p));
    expected.extend(schema_phase(p));
    expected.extend(confirmation(p, 42));
    let f = configured_native(&expected);
    let git = git_state(&f);
    let cfg = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = start(&f, mouse);
    begin(&mut s, &f, mouse, p);
    let sql = unrelated(&f);
    inspect_schema(&mut s, &f, mouse);
    preview(&mut s, &f, mouse);
    resize_form(
        &mut s,
        80,
        24,
        INTENT,
        &["[Preview]", "[Confirm action]", "[Keep draft / Close]"],
    );
    let tty = s.actual_terminal_witness_v4().unwrap();
    let after = s.completed_cells_end();
    action(&mut s, mouse, INTENT, "[Confirm action]");
    s.wait_for_command_count(&f, expected.len());
    owned(
        &mut s,
        after,
        HISTORY,
        &[
            "[View original]",
            "[F6 Acknowledge locally]",
            if p.detailed() {
                "[F11 Open run]"
            } else {
                "[F11 Run unavailable]"
            },
            "[Close]",
            "Accepted; completion unknown",
            "[F7 Info↑]",
            "[F8 Info↓]",
        ],
    );
    let shown = crate::support::overlay_body(s.completed_cells_screen().unwrap(), HISTORY).unwrap();
    assert!(shown.contains("account #42") && shown.contains("workflow #7"));
    assert!(
        shown.find("Accepted; completion unknown").unwrap()
            < shown.find("Original request").unwrap()
    );
    assert!(shown.contains("Tab: buttons • Esc: close"));
    assert!(
        !shown.contains("Ctrl-A: select all")
            && !shown.contains("Ctrl-P: preview")
            && !shown.contains("Esc: keep draft")
    );
    assert!(!shown.contains("Some(") && !shown.contains("None") && !shown.contains("[F7 Error↑]"));
    s.capture_completed_terminal_v1(if p.detailed() {
        "task67-dispatch-accepted200-completion-unknown-80"
    } else {
        "task67-dispatch-accepted204-no-run-80"
    });
    let (state, context, options, receipt) = metadata(&f);
    assert_eq!(state, "accepted_pending");
    for text in [&context, &options, &receipt] {
        assert!(!text.contains(SECRET));
        assert!(!text.contains("NEVER_RUN_LOCALLY"));
    }
    let r: Value = serde_json::from_str(&receipt).unwrap();
    if p.detailed() {
        assert_eq!(r["observed_run_id"], 124);
        assert_eq!(r["original_dispatch_run_v7"]["head_sha"], C);
    } else {
        assert!(r["observed_run_id"].is_null());
        let before = commands(&s, &f).len();
        action(&mut s, mouse, HISTORY, "[F11 Run unavailable]");
        owned(
            &mut s,
            after,
            HISTORY,
            &["No actual current POST200 returned-run proof"],
        );
        assert_eq!(commands(&s, &f).len(), before);
    }
    assert_eq!(unrelated(&f), sql);
    assert_eq!(git_state(&f), git);
    assert_eq!(s.actual_terminal_witness_v4().unwrap(), tty);
    s.capture_completed_terminal_v1(if p.detailed() {
        "dispatch-modern200-actual-run"
    } else {
        "dispatch-compatible204-no-run"
    });
    close(&mut s, mouse, HISTORY);
    s.quit_and_assert_restored_terminal(mouse);
    exact(&s, &f, &expected, 1);
    assert_eq!(git_state(&f), git);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        cfg
    );
    assert_private_intents_unchanged(&f);
    assert_read_only_git(&f);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_compatible204_secret_preview_exactpost_no_guessedrun() {
    success(false, Protocol::Compatible);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_compatible204_secret_preview_exactpost_no_guessedrun() {
    success(true, Protocol::Compatible);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_detailed200_actual_run_receipt_at80columns() {
    success(false, Protocol::Detailed);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_detailed200_actual_run_receipt_at80columns() {
    success(true, Protocol::Detailed);
}
#[derive(Clone, Copy)]
enum Refusal {
    Version,
    Ambiguous,
    RawFlag,
    RawUtf8,
    MovedRef,
    RecreatedWorkflow,
    RefDisappeared,
    OppositeRecreated,
}
fn schema_refusal(mouse: bool, kind: Refusal) {
    let p = Protocol::Compatible;
    let mut phase = schema_phase(p);
    match kind {
        Refusal::Version => {
            phase[3].stdout = json!(["2026-03-10"]).to_string().into_bytes();
            phase.truncate(4);
        }
        Refusal::Ambiguous => {
            phase[6].status = 0;
            phase[6].stderr.clear();
            phase[6].stdout = json!({"ref":"refs/tags/main","object":{"type":"commit","sha":C}})
                .to_string()
                .into_bytes();
            phase.truncate(7);
        }
        Refusal::RawFlag => {
            phase[8].status = 1;
            phase[8].stdout.clear();
            phase[8].stderr =
                b"unknown flag: --allow-escape-sequences PRIVATE_RAW_DIAGNOSTIC".to_vec();
            phase.truncate(9);
        }
        Refusal::RawUtf8 => {
            phase[8].stdout = vec![0xfe, 0xff];
            phase.truncate(9);
        }
        Refusal::MovedRef => {
            let mut changed = reference();
            changed["object"]["sha"] = json!("d".repeat(40));
            phase[9].stdout = changed.to_string().into_bytes();
            phase.truncate(10);
        }
        Refusal::RecreatedWorkflow => {
            let mut changed = workflow();
            changed["node_id"] = json!("WF_RECREATED");
            phase[11].stdout = changed.to_string().into_bytes();
            phase.truncate(12);
        }
        Refusal::RefDisappeared => {
            phase[9].status = 1;
            phase[9].stdout.clear();
            phase[9].stderr = b"gh: Not Found (HTTP 404)".to_vec();
            phase.truncate(10);
        }
        Refusal::OppositeRecreated => {
            phase[10].status = 0;
            phase[10].stderr.clear();
            phase[10].stdout = json!({"ref":"refs/tags/main","object":{"type":"commit","sha":C}})
                .to_string()
                .into_bytes();
            phase.truncate(11);
        }
    }
    let mut expected = opening();
    expected.extend(phase);
    let f = configured_native(&expected);
    let git = git_state(&f);
    let cfg = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = start(&f, mouse);
    begin(&mut s, &f, mouse, p);
    let tty = s.actual_terminal_witness_v4().unwrap();
    let sql = unrelated(&f);
    let after = s.completed_cells_end();
    action(&mut s, mouse, INTENT, "[Inspect schema]");
    s.wait_for_command_count(&f, expected.len());
    s.wait_for_new_cursorless_condition(
        after,
        |frame| {
            crate::support::overlay_body(frame, INTENT).is_none()
                && frame.contents().contains("private")
                && !frame.contents().contains("PRIVATE_RAW_DIAGNOSTIC")
                && crate::support::main_footer_visible(frame)
        },
        "Original unsupported/ambiguous/changed schema fails closed with no private diagnostics",
        Duration::from_secs(8),
    );
    let db = publication_v3::raw(&f);
    let table:i64=db.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='actions_write_attempts'",[],|r|r.get(0)).unwrap();
    if table == 1 {
        assert_eq!(
            db.query_row("SELECT COUNT(*) FROM actions_write_attempts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    assert_eq!(unrelated(&f), sql);
    assert_eq!(git_state(&f), git);
    assert_eq!(s.actual_terminal_witness_v4().unwrap(), tty);
    s.quit_and_assert_restored_terminal(mouse);
    exact(&s, &f, &expected, 0);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        cfg
    );
    assert_private_intents_unchanged(&f);
    assert_read_only_git(&f);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_unsupported_version_no_fallback_or_post() {
    schema_refusal(false, Refusal::Version);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_unsupported_version_no_fallback_or_post() {
    schema_refusal(true, Refusal::Version);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_ambiguous_same_branch_tag_name_no_post() {
    schema_refusal(false, Refusal::Ambiguous);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_ambiguous_same_branch_tag_name_no_post() {
    schema_refusal(true, Refusal::Ambiguous);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_raw_flag_failure_is_one_shot_redacted() {
    schema_refusal(false, Refusal::RawFlag);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_raw_flag_failure_is_one_shot_redacted() {
    schema_refusal(true, Refusal::RawFlag);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_nonutf8_yaml_has_no_lossy_schema_or_post() {
    schema_refusal(false, Refusal::RawUtf8);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_nonutf8_yaml_has_no_lossy_schema_or_post() {
    schema_refusal(true, Refusal::RawUtf8);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_ref_moves_during_schema_no_silent_rebase() {
    schema_refusal(false, Refusal::MovedRef);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_ref_moves_during_schema_no_silent_rebase() {
    schema_refusal(true, Refusal::MovedRef);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_recreated_workflow_node_no_post() {
    schema_refusal(false, Refusal::RecreatedWorkflow);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_recreated_workflow_node_no_post() {
    schema_refusal(true, Refusal::RecreatedWorkflow);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_keyboard_selected_ref_disappears_or_opposite_recreated_at_close(
) {
    schema_refusal(false, Refusal::RefDisappeared);
    schema_refusal(false, Refusal::OppositeRecreated);
}
#[test]
fn scoped_actions_dispatch_v7_task54_w7_mouse_selected_ref_disappears_or_opposite_recreated_at_close(
) {
    schema_refusal(true, Refusal::RefDisappeared);
    schema_refusal(true, Refusal::OppositeRecreated);
}
#[derive(Clone, Copy)]
enum AttemptRefusal {
    RefMoved,
    FinalUid,
    WrongRunHead,
}
fn attempt_refusal(mouse: bool, kind: AttemptRefusal) {
    let p = if matches!(kind, AttemptRefusal::WrongRunHead) {
        Protocol::Detailed
    } else {
        Protocol::Compatible
    };
    let mut tail = confirmation(
        p,
        if matches!(kind, AttemptRefusal::FinalUid) {
            999
        } else {
            42
        },
    );
    let posts = if matches!(kind, AttemptRefusal::RefMoved) {
        let mut changed = reference();
        changed["object"]["sha"] = json!("d".repeat(40));
        tail[12].stdout = changed.to_string().into_bytes();
        tail.truncate(13);
        0
    } else if matches!(kind, AttemptRefusal::WrongRunHead) {
        let mut changed = returned_run();
        changed["head_sha"] = json!("d".repeat(40));
        tail[19].stdout = changed.to_string().into_bytes();
        tail.truncate(20);
        1
    } else {
        1
    };
    let mut expected = opening();
    expected.extend(schema_phase(p));
    expected.extend(schema_phase(p));
    expected.extend(tail);
    let f = configured_native(&expected);
    let git = git_state(&f);
    let cfg = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = start(&f, mouse);
    begin(&mut s, &f, mouse, p);
    let sql = unrelated(&f);
    inspect_schema(&mut s, &f, mouse);
    preview(&mut s, &f, mouse);
    let tty = s.actual_terminal_witness_v4().unwrap();
    let after = s.completed_cells_end();
    action(&mut s, mouse, INTENT, "[Confirm action]");
    s.wait_for_command_count(&f, expected.len());
    s.wait_for_new_cursorless_condition(after,|frame|{
        crate::support::overlay_body(frame,INTENT).is_none()&&crate::support::overlay_body(frame,HISTORY).is_none()
            &&frame.contents().contains("private")&&crate::support::main_footer_visible(frame)
    },"Actual failed original Confirm hides only old private presentation; durable protection remains",Duration::from_secs(8));
    assert_eq!(
        metadata(&f).0,
        match kind {
            AttemptRefusal::RefMoved => "pending",
            AttemptRefusal::FinalUid => "accepted_pending",
            AttemptRefusal::WrongRunHead => "uncertain",
        }
    );
    assert_eq!(unrelated(&f), sql);
    assert_eq!(git_state(&f), git);
    assert_eq!(s.actual_terminal_witness_v4().unwrap(), tty);
    s.quit_and_assert_restored_terminal(mouse);
    exact(&s, &f, &expected, posts);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        cfg
    );
    assert_private_intents_unchanged(&f);
    assert_read_only_git(&f);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_confirm_ref_moved_keeps_pending_no_post() {
    attempt_refusal(false, AttemptRefusal::RefMoved);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_confirm_ref_moved_keeps_pending_no_post() {
    attempt_refusal(true, AttemptRefusal::RefMoved);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_postwrite_uid_loss_keeps_receipt_private() {
    attempt_refusal(false, AttemptRefusal::FinalUid);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_postwrite_uid_loss_keeps_receipt_private() {
    attempt_refusal(true, AttemptRefusal::FinalUid);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_actual_returned_run_wrong_head_is_uncertain() {
    attempt_refusal(false, AttemptRefusal::WrongRunHead);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_actual_returned_run_wrong_head_is_uncertain() {
    attempt_refusal(true, AttemptRefusal::WrongRunHead);
}
fn tiny(mouse: bool) {
    let p = Protocol::Compatible;
    let mut expected = opening();
    expected.extend(schema_phase(p));
    expected.extend(schema_phase(p));
    let f = configured_native(&expected);
    let git = git_state(&f);
    let cfg = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = start(&f, mouse);
    begin(&mut s, &f, mouse, p);
    let sql = unrelated(&f);
    inspect_schema(&mut s, &f, mouse);
    field(
        &mut s,
        mouse,
        "message • transient input value • masked; never saved",
    );
    let before_error = commands(&s, &f).len();
    let after = s.completed_cells_end();
    s.paste(&"x".repeat(16 * 1024 + 1));
    owned(
        &mut s,
        after,
        INTENT,
        &[
            "Dispatch input exceeds its byte limit",
            "[F7 Error↑]",
            "[F8 Error↓]",
        ],
    );
    resize_form(
        &mut s,
        80,
        20,
        INTENT,
        &[
            "[Preview]",
            "[Confirm action]",
            "[Keep draft / Close]",
            "[F7 Error↑]",
            "[F8 Error↓]",
        ],
    );
    let frame = s.completed_cells_screen().unwrap();
    assert_eq!(frame.size(), (20, 80));
    assert!(
        underlined_field(frame, INTENT).is_some_and(|(label, (_, _, width))| {
            label == "message • transient input value • masked; never saved" && width > 0
        })
    );
    assert_eq!(commands(&s, &f).len(), before_error);
    assert_eq!(unrelated(&f), sql);
    assert_eq!(git_state(&f), git);
    resize_form(
        &mut s,
        120,
        40,
        INTENT,
        &[
            "[Preview]",
            "[Confirm action]",
            "[Keep draft / Close]",
            "[F7 Error↑]",
            "[F8 Error↓]",
        ],
    );
    preview(&mut s, &f, mouse);
    s.resize_owned_single_action_modal_v2(
        40,
        10,
        "Original Actions • resize required",
        "[Esc Close]",
    );
    s.wait_for_cursorless_condition(
        |frame| {
            frame.size() == (10, 40)
                && frame.contents().contains("resize required")
                && !frame.contents().contains("[Confirm action]")
        },
        "Actual new tiny original Actions surface has no hidden Confirm",
        Duration::from_secs(8),
    );
    let before = commands(&s, &f).len();
    if mouse {
        let (x, y) = s.wait_for_owned_completed_action(
            "Original Actions • resize required",
            "[Esc Close]",
            &[],
        );
        s.click(x, y);
    } else {
        s.send_key(b"\x1b");
    }
    s.resize(120, 40);
    s.wait_for_main_frame();
    assert_eq!(commands(&s, &f).len(), before);
    assert_eq!(unrelated(&f), sql);
    assert_eq!(git_state(&f), git);
    s.quit_and_assert_restored_terminal(mouse);
    exact(&s, &f, &expected, 0);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        cfg
    );
    assert_private_intents_unchanged(&f);
    assert_read_only_git(&f);
}
#[test]
fn scoped_actions_dispatch_v7_keyboard_tiny_no_hidden_confirm_and_safe_close() {
    tiny(false);
}
#[test]
fn scoped_actions_dispatch_v7_mouse_tiny_no_hidden_confirm_and_safe_close() {
    tiny(true);
}
