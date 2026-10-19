//! Real-binary observed PR sections, both input paths across six sessions;
//! each drives renamed observed tabs, bounded-display refusal and rapid Back.
use super::*;
#[path = "timeline_actor_scoped_v1.rs"]
mod timeline_actor_v1;

const RENAMED: &str = "example/renamed0000";
fn protocol(case: u8) -> Vec<GhStep> {
    let mut steps = collection(vec![json!([repository(0)])], 42);
    steps.extend(open_inbox());
    let mut op = open_pr();
    let mut current = repository(0);
    current["full_name"] = json!(RENAMED);
    op[2] = get(&format!("repos/{FIRST}"), current.clone());
    op[10] = get(&format!("repos/{FIRST}"), current);
    for step in &mut op[3..10] {
        step.args[1] = step.args[1].replace(FIRST, RENAMED);
    }
    let mut summary = super::summary();
    summary["body"] = json!(DETAIL_BODY);
    summary["repository_url"] = json!(format!("https://{HOST}/repos/{RENAMED}"));
    summary["html_url"] = json!(format!("https://{HOST}/{RENAMED}/pull/7"));
    summary["base"]["repo"]["full_name"] = json!(RENAMED);
    op[3] = get(&format!("repos/{RENAMED}/pulls/7"), summary.clone());
    op[9] = get(&format!("repos/{RENAMED}/pulls/7"), summary);
    let body = if case == 1 {
        "x\n".repeat(70_000)
    } else if case == 2 {
        format!(
            "Observed queued conversation header canary\n{}",
            "x\n".repeat(50_000)
        )
    } else {
        std::iter::once("Observed conversation header canary\n".into())
            .chain((0..200).map(|i| format!("Observed conversation line {i:03} • 界🦀\n")))
            .collect()
    };
    op[4] = paged(
        &format!("repos/{RENAMED}/issues/7/comments?per_page=100"),
        json!([{"id":301,"node_id":"COMMENT_301","body":body,"user":{"id":62,"node_id":"USER_62","login":"comment-author","type":"User"},"created_at":"2026-10-02T00:00:00Z"}]),
    );
    op[5] = paged(
        &format!("repos/{RENAMED}/pulls/7/commits?per_page=100"),
        json!([{"sha":"c".repeat(40),"commit":{"message":"Observed scoped commit","author":{"name":"Saved author"}},"parents":[]},{"sha":"e".repeat(40),"commit":{"message":"Observed child commit"},"parents":[{"sha":"d".repeat(40)}]}]),
    );
    op[8] = paged(
        &format!("repos/{RENAMED}/issues/7/timeline?per_page=100"),
        json!([{"id":801,"event":"review_requested","actor":{"id":63,"login":"timeline-user","type":"User"},"created_at":"2026-10-02T00:00:00Z","label":{"name":"observed-safe-label"}}]),
    );
    steps.extend(op);
    assert_eq!(steps.len(), 31);
    steps
}
fn journey(mouse: bool, case: u8) {
    let steps = protocol(case);
    let fixture = configured(&steps);
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(&fixture)).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database(&fixture)).unwrap());
    let git_before = git_state(&fixture);
    let config_before = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    session.wait_for_command_count(&fixture, 7);
    ready(&mut session, &[TITLE, FIRST]);
    if !mouse {
        session.send_key(b"\t");
    }
    choose(&mut session, mouse, "[Open My PRs]", b"\r");
    session.wait_for_overlay_close(TITLE);
    session.wait_for_command_count(&fixture, 18);
    ready(&mut session, &[ROW_TITLE]);
    choose(&mut session, mouse, "1 Inbox", b"1");
    choose(&mut session, mouse, ROW_TITLE, b"\r");
    session.wait_for_command_count(&fixture, 31);
    ready(
        &mut session,
        &[RENAMED, DETAIL_TITLE, "Head aaaaaaaaa", "[b Back]"],
    );
    let before = sql_state(&fixture);
    let gh = session.inspect_command_journal(&fixture);
    let git = git_commands(&fixture);
    if case == 2 {
        // Back stays at the SAME first scoped-control position both before and
        // while preparing. A real mouse report cannot hit a stale tab control.
        let back = session.wait_for_control("[b Back]");
        choose(&mut session, mouse, "[c Conversation]", b"c");
        if mouse {
            session.click(back.0 + 1, back.1);
        } else {
            session.send_key(b"b");
        }
        session
            .try_wait_for_condition(
                |screen| {
                    screen.contains(ROW_TITLE)
                        && screen.contains("Cached • fetched")
                        && !screen.contains(DETAIL_TITLE)
                        && !screen.contains("Observed queued conversation")
                        && !screen.contains("Loading Inbox")
                },
                "Rapid pure Back/cancel retains cached Inbox and cannot reopen a queued page",
                Duration::from_secs(8),
            )
            .unwrap();
    } else {
        choose(&mut session, mouse, "[c Conversation]", b"c");
        if case == 1 {
            session.try_wait_for_condition(|screen|screen.contains("previous section retained")&&screen.contains("Head aaaaaaaaa")&&screen.contains(DETAIL_TITLE)&&!screen.contains("Loading Inbox"),"Completed bounded display refusal retains original Summary without cloning huge selection bodies",Duration::from_secs(8)).unwrap();
        } else {
            ready(
                &mut session,
                &[
                    "PR description",
                    "comment-author",
                    "Observed conversation header canary",
                ],
            );
            session.resize(if mouse { 120 } else { 80 }, if mouse { 40 } else { 24 });
            ready(&mut session, &["[c Conversation]", "[b Back]"]);
            if mouse {
                let (column, row) = session.locate("Observed conversation").unwrap();
                session.wheel(column, row, true);
            } else {
                session.send_key(b"\x1b[6~");
            }
            ready(&mut session, &["Observed conversation line"]);
            choose(&mut session, mouse, "[h Commits]", b"h");
            ready(
                &mut session,
                &[
                    "Observed scoped commit",
                    "Observed child commit",
                    "Parent links unavailable or none",
                    &"d".repeat(40),
                ],
            );
            assert!(!session.screen().contains("none (first saved version)"));
            choose(&mut session, mouse, "[t Timeline]", b"t");
            ready(
                &mut session,
                &["timeline-user", "review_requested", "observed-safe-label"],
            );
            choose(&mut session, mouse, "[o Summary]", b"o");
            ready(
                &mut session,
                &[DETAIL_TITLE, "Head aaaaaaaaa", "Base bbbbbbbbb"],
            );
        }
    }
    choose(&mut session, mouse, "[? Help]", b"?");
    overlay_ready(
        &mut session,
        "Help • keyboard and mouse",
        &["Inbox • help for this view", "[Esc Close]"],
    );
    if mouse {
        session.close_overlay_by_mouse("Help • keyboard and mouse");
    } else {
        session.close_overlay_by_key("Help • keyboard and mouse");
    }
    if case != 2 {
        ready(&mut session, &[DETAIL_TITLE, "[b Back]"]);
        choose(&mut session, mouse, "[b Back]", b"b");
    }
    session.try_wait_for_condition(|screen|screen.contains(ROW_TITLE)&&screen.contains("Cached • fetched")&&screen.contains("PARTIAL")&&!screen.contains(DETAIL_TITLE)&&!screen.contains(DETAIL_BODY),"Completed pure Back erases observed tab/current renamed details and retains exact original cached row",Duration::from_secs(8)).unwrap();
    assert_eq!(session.inspect_command_journal(&fixture), gh);
    assert_eq!(git_commands(&fixture), git);
    assert_eq!(sql_state(&fixture), before);
    no_attempts(&fixture);
    exact_reads(&session, &fixture, &steps);
    assert_eq!(git_state(&fixture), git_before);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config_before
    );
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
    exact_reads(&session, &fixture, &steps);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state(&fixture), git_before);
}
#[test]
fn repo_catalog_scoped_pr_observed_tabs_keyboard_rename_pure_sections_back_and_display_cap() {
    for case in 0..3 {
        journey(false, case);
    }
}
#[test]
fn repo_catalog_scoped_pr_observed_tabs_mouse_rename_pure_sections_back_and_display_cap() {
    for case in 0..3 {
        journey(true, case);
    }
}
