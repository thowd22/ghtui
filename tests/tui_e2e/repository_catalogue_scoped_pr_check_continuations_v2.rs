//! UNREGISTERED child of scoped check choices. Sixteen real-binary declarations
//! with separate keyboard/mouse runs. Prediction/fixtures, NOT executed proof.
use super::*;
#[path = "repository_catalogue_scoped_pr_check_corrective_v3.rs"]
mod corrective_v3;
#[path = "repository_catalogue_scoped_pr_failed_step_navigation_v8.rs"]
mod failed_steps_v8;

const ANN: &str = "REPORTED_ANNOTATION_PRIVATE_CANARY";
const FRESH_LOG: &str = "NEW_FINITE_WATCH_CAPTURE_CANARY";
fn overlay_ready(session: &mut Session, title: &str, required: &[&str]) {
    let exact = |text: &str| {
        text.chars()
            .filter(|c| !c.is_whitespace())
            .collect::<String>()
    };
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(10));
        if session.visible_overlay_body(title).is_some_and(|body| {
            required
                .iter()
                .all(|label| exact(&body).contains(&exact(label)))
        }) {
            return;
        }
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "Actual owned overlay {title} missing complete wrapped text {required:?}\n{}",
                session.screen()
            );
        }
    }
}
fn completed_action(session: &mut Session, previous: usize, label: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(10));
        if session.completed_cursorless_end() > previous
            && session.screen().contains(label)
            && !session.screen().contains("[Esc Cancel]")
        {
            return;
        }
        if Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "Actual completed Actions transition missing {label}\n{}",
                session.screen()
            );
        }
    }
}
fn check_annotations(id: u64, total: Option<u64>) -> Value {
    let mut v = selected_check();
    v["id"] = json!(id);
    if id != 600 {
        v["node_id"] = json!(format!("CHECK_{id}"));
    }
    v["url"] = json!(format!(
        "https://{HOST}/api/v3/repos/{FIRST}/check-runs/{id}"
    ));
    v["output"] = json!({"annotations_count":total});
    v
}
fn annotations_group(member: u64, permission: bool, lost: bool) -> Vec<GhStep> {
    let id = if member == 501 { 600 } else { 601 };
    let mut op = principal(42);
    op.push(get(&format!("repos/{FIRST}"), repository(0)));
    let member_jobs = if member == 501 {
        vec![observed_job()]
    } else {
        member_rows(123)
    };
    op.extend([who(42),get(&format!("repos/{FIRST}"),current_repo()),get(&format!("repos/{FIRST}/pulls/7"),parent_resource()),
        get(&format!("repos/{FIRST}/actions/runs/123"),chosen_run(123)),get(&format!("repos/{FIRST}/actions/runs/123/attempts/2"),chosen_run(123)),
        get(&format!("repos/{FIRST}/actions/runs/123/attempts/2/jobs?per_page=100&page=1"),json!({"total_count":member_jobs.len(),"jobs":member_jobs})),
        get(&format!("repos/{FIRST}/check-runs/{id}"),check_annotations(id,Some(1))),
        get(&format!("repos/{FIRST}/check-runs/{id}/annotations?per_page=100&page=1"),json!([{"path":"src/界.rs","start_line":7,"end_line":9,"annotation_level":"failure","title":"Reported exact test","message":format!("{ANN} • 界🦀\n{}",(0..50).map(|n|format!("annotation detail {n:02}")).collect::<Vec<_>>().join("\n")),"raw_details":"raw reported details"}])),
        get(&format!("repos/{FIRST}/check-runs/{id}"),check_annotations(id,Some(1))),get(&format!("repos/{FIRST}/actions/jobs/{member}"),if member==501{observed_job()}else{chosen_job(123,member)}),
        get(&format!("repos/{FIRST}/actions/runs/123"),chosen_run(123)),get(&format!("repos/{FIRST}/pulls/7"),parent_resource()),get(&format!("repos/{FIRST}"),current_repo()),who(if lost{43}else{42}),get(&format!("repos/{FIRST}"),repository(0))]);
    op.extend(principal(42));
    assert_eq!(op.len(), 20);
    if permission {
        op[10].stdout.clear();
        op[10].stderr = b"gh: category unavailable (HTTP 403)".to_vec();
        op[10].status = 1;
    }
    if lost {
        op.truncate(17);
    }
    op
}
fn advance_group() -> Vec<GhStep> {
    let mut latest = observed_run();
    latest["run_attempt"] = json!(3);
    latest["check_suite_id"] = json!(901);
    let mut op = principal(42);
    op.push(get(&format!("repos/{FIRST}"), repository(0)));
    op.extend([
        who(42),
        get(&format!("repos/{FIRST}"), current_repo()),
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
        get(&format!("repos/{FIRST}/check-runs/600"), selected_check()),
        get(&format!("repos/{FIRST}/actions/runs/123"), latest.clone()),
        get(&format!("repos/{FIRST}/actions/runs/123"), latest),
        get(&format!("repos/{FIRST}/check-runs/600"), selected_check()),
        get(&format!("repos/{FIRST}/pulls/7"), parent_resource()),
        get(&format!("repos/{FIRST}"), current_repo()),
        who(42),
        get(&format!("repos/{FIRST}"), repository(0)),
    ]);
    op.extend(principal(42));
    assert_eq!(op.len(), 16);
    op
}
fn baseline(chooser: bool) -> Vec<GhStep> {
    let mut v = initial();
    v.extend(resource_operation(true));
    v.extend(if chooser {
        candidate_initial(false)
    } else {
        bridge()
    });
    assert_eq!(v.len(), 59);
    v
}
fn finite_logs() -> Vec<GhStep> {
    let mut op = logs();
    op[7] = GhStep::read(
        &[
            "run",
            "view",
            "123",
            "--repo",
            &format!("{HOST}/{FIRST}"),
            "--attempt",
            "2",
            "--log",
            "--job",
            "501",
        ],
        &format!("{FRESH_LOG}\nScoped exact log line 00\nScoped exact log line 01"),
    );
    op
}
#[derive(Clone, Copy)]
enum Case {
    MatchedAnnotations,
    OtherAnnotations,
    LoadedSearch,
    StatusWatch,
    LogWatchStop,
    AttemptAdvance,
    CategoryPermission,
    LateScopeLoss,
}
fn open_selected_job(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    job: u64,
    count: usize,
) {
    let label = format!("Job {job}");
    let previous = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action(&label)
    } else {
        session.focus_visible_action(&label);
        session.send_key(b"\r")
    };
    completed_action(session, previous, "Jobs and steps");
    for _ in 0..8 {
        if session.screen().contains("Step 2") && session.screen().contains("Test exact source") {
            break;
        }
        let previous = session.completed_cursorless_end();
        if mouse {
            session.wheel(40, 18, true);
        } else {
            session.send_key(b"\x1b[B");
        }
        completed_action(session, previous, "Jobs and steps");
    }
    ready(session, &["Step 2", "Test exact source"]);
    assert_eq!(commands(session, fixture).len(), count);
}
fn query(session: &mut Session, mouse: bool, regex: bool, text: &str) {
    query_for_job(session, mouse, 501, regex, text);
}
pub(in super::super) fn query_for_job(
    session: &mut Session,
    mouse: bool,
    job: u64,
    regex: bool,
    text: &str,
) {
    control(session, mouse, "Search current logs", b"/");
    let target = format!("captured job {job}");
    overlay_ready(
        session,
        "Loaded-log search",
        &[&target, "[Search]", "[Esc Close]"],
    );
    session.paste(text);
    if regex {
        if mouse {
            session.click_visible_action("[Regex]")
        } else {
            session.send_key(b"\t\t\r")
        }
    }
    if mouse {
        session.click_visible_action("[Search]")
    } else if regex {
        session.send_key(b"\t\r")
    } else {
        session.send_key(b"\r")
    }
}
struct ReadOnlyEvidence<'a> {
    sql: &'a [String],
    git: &'a GitState,
    config: &'a [u8],
    trace: &'a [Value],
}
fn finish_read(
    session: &mut Session,
    fixture: &Fixture,
    mouse: bool,
    expected: &[GhStep],
    evidence: ReadOnlyEvidence<'_>,
) {
    if session.screen().contains("Action unavailable") {
        close_notice(session, mouse);
    }
    session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
    session.wait_for_main_frame();
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    assert_eq!(sql_state(fixture), evidence.sql);
    no_attempts(fixture);
    assert_eq!(&git_state(fixture), evidence.git);
    assert_eq!(git_commands(fixture), evidence.trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        evidence.config
    );
    session.quit_and_assert_restored_terminal(mouse);
    exact_reads(session, fixture, expected);
}
fn journey(mouse: bool, case: Case) {
    let other = matches!(case, Case::OtherAnnotations);
    let mut expected = baseline(other);
    if other {
        expected.extend(selected_read(123));
    }
    match case {
        Case::MatchedAnnotations
        | Case::OtherAnnotations
        | Case::CategoryPermission
        | Case::LateScopeLoss => expected.extend(annotations_group(
            if other { 502 } else { 501 },
            matches!(case, Case::CategoryPermission),
            matches!(case, Case::LateScopeLoss),
        )),
        Case::LoadedSearch => expected.extend(logs()),
        Case::StatusWatch => {
            expected.extend(selected_read(123));
            expected.extend(selected_read(123));
            expected.extend(selected_read(123));
        }
        Case::LogWatchStop => {
            expected.extend(logs());
            expected.extend(finite_logs());
            expected.extend(finite_logs());
            let mut pending = finite_logs();
            pending[7].delay_ms = 3000;
            pending[7].stdout = b"LATE_STOPPED_LOG_CAPTURE_CANARY".to_vec();
            pending.truncate(8);
            expected.extend(pending);
        }
        Case::AttemptAdvance => expected.extend(advance_group()),
    }
    let fixture = resource_configured(&expected);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    if other {
        candidate_source(&mut session, &fixture, mouse, false);
        pick(&mut session, &fixture, mouse, 0, 77, false);
    } else {
        open_run(&mut session, &fixture, mouse);
    }
    let before = sql_state(&fixture);
    let trace = git_commands(&fixture);
    match case {
        Case::MatchedAnnotations
        | Case::OtherAnnotations
        | Case::CategoryPermission
        | Case::LateScopeLoss => {
            let count = if other { 77 } else { 59 };
            open_selected_job(
                &mut session,
                &fixture,
                mouse,
                if other { 502 } else { 501 },
                count,
            );
            control(&mut session, mouse, "[N Annotations]", b"N");
            session.wait_for_command_count(&fixture, expected.len());
            if matches!(case, Case::LateScopeLoss) {
                ready(&mut session, &["Original repository scope is fenced"]);
                assert!(!session.screen().contains(ANN));
                assert!(!session.screen().contains("Exact observed build"));
                assert!(!session.screen().contains("Run 123 • attempt 2"));
                choose(&mut session, mouse, "3 Search", b"3");
                ready(
                    &mut session,
                    &["Original repository scope is fenced", "Search"],
                );
                assert_eq!(commands(&session, &fixture).len(), expected.len());
            } else {
                ready(
                    &mut session,
                    &["API-visible annotations", "not a canonical file range"],
                );
                assert!(!session.screen().contains("line 0"));
                if matches!(case, Case::CategoryPermission) {
                    ready(&mut session, &["PARTIAL / unknown"]);
                    assert!(!session.screen().contains(ANN));
                    assert!(!session.screen().contains("observed source complete"));
                } else {
                    select_observed_row(&mut session, mouse, "Reported exact test");
                    control(&mut session, mouse, "[d Reported details]", b"d");
                    overlay_ready(
                        &mut session,
                        "Reported annotation",
                        &[ANN, "reported lines", "not server ID"],
                    );
                    for _ in 0..12 {
                        if session.screen().contains("annotation detail 30") {
                            break;
                        }
                        if mouse {
                            let (x, y) = session.locate("annotation detail").unwrap();
                            session.wheel(x, y, true);
                        } else {
                            session.send_key(b"\x1b[6~");
                        }
                        session.poll_output(Duration::from_millis(50));
                    }
                    ready(&mut session, &["annotation detail 30"]);
                    session.resize_owned_modal(
                        100,
                        30,
                        "Reported annotation",
                        &["[Esc Close]", "[q Quit]"],
                    );
                    if mouse {
                        session.click_visible_action("[Esc Close]")
                    } else {
                        session.send_key(b"\x1b")
                    };
                    ready(&mut session, &["API-visible annotations"]);
                    assert_eq!(commands(&session, &fixture).len(), expected.len());
                }
            }
        }
        Case::LoadedSearch => {
            open_selected_job(&mut session, &fixture, mouse, 501, 59);
            control(&mut session, mouse, "[l Logs]", b"l");
            session.wait_for_command_count(&fixture, 74);
            ready(&mut session, &["EXACT_MATCHED_JOB_LOG_CANARY"]);
            query(&mut session, mouse, false, "界🦀");
            ready(&mut session, &["1 matches in captured job 501 attempt 2"]);
            assert_eq!(commands(&session, &fixture).len(), 74);
            assert_eq!(sql_state(&fixture), before);
            query(
                &mut session,
                mouse,
                true,
                r"^Scoped exact log line (00|30)$",
            );
            ready(&mut session, &["2 matches in captured job 501 attempt 2"]);
            control(&mut session, mouse, "[n Next match]", b"n");
            ready(&mut session, &["Scoped exact log line 30"]);
            control(&mut session, mouse, "[x Clear matches]", b"x");
            ready(&mut session, &["matches cleared"]);
            query(&mut session, mouse, true, "[");
            overlay_ready(
                &mut session,
                "Action unavailable",
                &[
                    "Regex/query preparation unavailable",
                    "original captured logs unchanged",
                ],
            );
            close_notice(&mut session, mouse);
            assert_eq!(commands(&session, &fixture).len(), 74);
        }
        Case::StatusWatch => {
            control(&mut session, mouse, "[r Refresh]", b"r");
            session.wait_for_command_count(&fixture, 77);
            ready(
                &mut session,
                &["Original scoped status refreshed", "Run 123 • attempt 2"],
            );
            control(&mut session, mouse, "[w Watch", b"w");
            ready(&mut session, &["Finite scoped watch armed"]);
            session.wait_for_command_count(&fixture, 95);
            ready(&mut session, &["tick 1/12"]);
            session.wait_for_command_count(&fixture, 113);
            ready(&mut session, &["tick 2/12"]);
            control(&mut session, mouse, "[w Stop watch]", b"w");
            ready(&mut session, &["Finite scoped watch stopped"]);
            session.poll_output(Duration::from_secs(6));
            assert_eq!(commands(&session, &fixture).len(), 113);
            assert!(session.screen().contains("Run 123 • attempt 2"));
        }
        Case::LogWatchStop => {
            open_selected_job(&mut session, &fixture, mouse, 501, 59);
            control(&mut session, mouse, "[l Logs]", b"l");
            session.wait_for_command_count(&fixture, 74);
            ready(&mut session, &["EXACT_MATCHED_JOB_LOG_CANARY"]);
            control(&mut session, mouse, "[r Refresh]", b"r");
            session.wait_for_command_count(&fixture, 89);
            ready(&mut session, &[FRESH_LOG]);
            control(&mut session, mouse, "[w Watch", b"w");
            session.wait_for_command_count(&fixture, 104);
            ready(&mut session, &[FRESH_LOG, "tick 1/12"]);
            session.wait_for_command_count(&fixture, 112);
            control(&mut session, mouse, "[w Stop watch]", b"w");
            ready(&mut session, &["Finite scoped watch stopped"]);
            session.poll_output(Duration::from_secs(6));
            assert_eq!(commands(&session, &fixture).len(), 112);
            assert!(!session.screen().contains("LATE_STOPPED_LOG_CAPTURE_CANARY"));
            assert!(session.screen().contains(FRESH_LOG));
        }
        Case::AttemptAdvance => {
            control(&mut session, mouse, "[w Watch", b"w");
            session.wait_for_command_count(&fixture, 75);
            overlay_ready(
                &mut session,
                "Action unavailable",
                &[
                    "Original observed attempt is historical",
                    "latest attempt advanced",
                    "no latest attempt adopted",
                ],
            );
            close_notice(&mut session, mouse);
            ready(&mut session, &["Run 123 • attempt 2"]);
            session.poll_output(Duration::from_secs(6));
            assert_eq!(commands(&session, &fixture).len(), 75);
            assert!(!session.screen().contains("Run 123 • attempt 3"));
            assert_eq!(sql_state(&fixture), before);
        }
    }
    assert_eq!(commands(&session, &fixture).len(), expected.len());
    finish_read(
        &mut session,
        &fixture,
        mouse,
        &expected,
        ReadOnlyEvidence {
            sql: &before,
            git: &git,
            config: &config,
            trace: &trace,
        },
    );
}
#[test]
fn scoped_continued_annotations_keyboard_reported_detail_resize_no_range() {
    journey(false, Case::MatchedAnnotations)
}
#[test]
fn scoped_continued_annotations_mouse_reported_detail_resize_no_range() {
    journey(true, Case::MatchedAnnotations)
}
#[test]
fn scoped_continued_other502_annotations_keyboard_actual601_not_origin600() {
    journey(false, Case::OtherAnnotations)
}
#[test]
fn scoped_continued_other502_annotations_mouse_actual601_not_origin600() {
    journey(true, Case::OtherAnnotations)
}
fn foreign_annotation_suite(mouse: bool) {
    let mut expected = baseline(true);
    expected.extend(selected_read(123));
    let mut op = annotations_group(502, false, false);
    let mut wrong = check_annotations(601, Some(1));
    wrong["check_suite"]["id"] = json!(999);
    op[9] = get(&format!("repos/{FIRST}/check-runs/601"), wrong);
    op.truncate(10);
    expected.extend(op);
    assert_eq!(expected.len(), 87);
    let fixture = resource_configured(&expected);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    candidate_source(&mut session, &fixture, mouse, false);
    pick(&mut session, &fixture, mouse, 0, 77, false);
    let before = sql_state(&fixture);
    let trace = git_commands(&fixture);
    open_selected_job(&mut session, &fixture, mouse, 502, 77);
    let previous = session.completed_cursorless_end();
    control(&mut session, mouse, "[N Annotations]", b"N");
    session.wait_for_command_count(&fixture, 87);
    completed_action(&mut session, previous, "Original scoped read unavailable");
    assert!(session.screen().contains("private results hidden"));
    assert!(!session.screen().contains(ANN));
    assert!(!session.screen().contains("Exact observed build"));
    assert!(!session.screen().contains("Run 123 • attempt 2"));
    finish_read(
        &mut session,
        &fixture,
        mouse,
        &expected,
        ReadOnlyEvidence {
            sql: &before,
            git: &git,
            config: &config,
            trace: &trace,
        },
    );
}
#[test]
fn scoped_continued_other502_foreign_annotation_suite_keyboard_hides_before_page() {
    foreign_annotation_suite(false);
}
#[test]
fn scoped_continued_other502_foreign_annotation_suite_mouse_hides_before_page() {
    foreign_annotation_suite(true);
}
#[test]
fn scoped_continued_loaded_search_keyboard_exact_regex_unicode_invalid_zero_reads() {
    journey(false, Case::LoadedSearch)
}
#[test]
fn scoped_continued_loaded_search_mouse_exact_regex_unicode_invalid_zero_reads() {
    journey(true, Case::LoadedSearch)
}
#[test]
fn scoped_continued_refresh_watch_keyboard_two_fresh_ticks_stop_no_legacy() {
    journey(false, Case::StatusWatch)
}
#[test]
fn scoped_continued_refresh_watch_mouse_two_fresh_ticks_stop_no_legacy() {
    journey(true, Case::StatusWatch)
}
#[test]
fn scoped_continued_log_watch_keyboard_fresh_capture_queued_stop_no_late_body() {
    journey(false, Case::LogWatchStop)
}
#[test]
fn scoped_continued_log_watch_mouse_fresh_capture_queued_stop_no_late_body() {
    journey(true, Case::LogWatchStop)
}
#[test]
fn scoped_continued_attempt_advance_keyboard_original2_latest3_no_adoption() {
    journey(false, Case::AttemptAdvance)
}
#[test]
fn scoped_continued_attempt_advance_mouse_original2_latest3_no_adoption() {
    journey(true, Case::AttemptAdvance)
}
#[test]
fn scoped_continued_annotations_keyboard_category403_unknown_not_empty() {
    journey(false, Case::CategoryPermission)
}
#[test]
fn scoped_continued_annotations_mouse_category403_unknown_not_empty() {
    journey(true, Case::CategoryPermission)
}
#[test]
fn scoped_continued_annotations_keyboard_late_uid43_hides_and_no_fallback() {
    journey(false, Case::LateScopeLoss)
}
#[test]
fn scoped_continued_annotations_mouse_late_uid43_hides_and_no_fallback() {
    journey(true, Case::LateScopeLoss)
}
