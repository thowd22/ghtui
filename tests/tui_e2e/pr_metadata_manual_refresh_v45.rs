//! Real original entry, typed durable Metadata receipt and explicit read-only
//! Return. No source/receipt injection; every gh argv/stdin is independently exact.
use super::*;
const RETURN: &str = "Confirmed Conversation • read-only refresh";
const REFRESH: &str = "[r Refresh PR]";
const BACK: &str = "[Esc Back to Inbox]";
const CONTROLS: &[&str] = &[REFRESH, BACK];

fn read_phase(field: Field) -> Vec<GhStep> {
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), parent(field, true)));
    calls.extend(post(field)[5..10].iter().cloned());
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), parent(field, true)));
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.extend(principal(42));
    assert_eq!(calls.len(), 13, "Core6 plus SAME measured7");
    calls
}
fn reservations(f: &Fixture) -> Vec<(String, String, String, i64, i64)> {
    raw(f).prepare("SELECT host,login,source,generation,active FROM sync_reservations ORDER BY host,login,source").unwrap()
        .query_map([], |r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).unwrap().map(Result::unwrap).collect()
}
fn next_personal(
    before: &[(String, String, String, i64, i64)],
) -> Vec<(String, String, String, i64, i64)> {
    let mut result = before.to_vec();
    let mut changed = 0;
    for row in &mut result {
        if row.0 == HOST && row.1 == "user-id:42" && row.2.starts_with("personal-pr-source-v13/") {
            assert!(row.3 > 0);
            row.3 += 1;
            row.4 = 1;
            changed += 1;
        }
    }
    assert_eq!(
        changed, 1,
        "Only the actual original Personal reservation advances"
    );
    result
}
fn protected(f: &Fixture) -> Vec<String> {
    complete_sql_v46(f)
        .into_iter()
        .filter(|row| {
            row.starts_with("pr_write_")
                || row.starts_with("drafts:")
                || row.starts_with("sync_mutation_fences:")
        })
        .collect()
}
fn resize_return(s: &mut Session, width: u16, height: u16) {
    let old_size = s.completed_cells_screen().map(|screen| screen.size());
    let after = s.completed_cells_end();
    s.resize(width, height);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if (old_size == Some((height, width)) || s.completed_cells_end() > after)
            && s.completed_cells_screen().is_some_and(|screen| {
                screen.size() == (height, width)
                    && crate::support::main_footer_visible(screen)
                    && crate::support::overlay_body(screen, RETURN).is_some_and(|body| {
                        body.contains("DURABLY CONFIRMED")
                            && body.lines().any(|row| {
                                row.find(REFRESH)
                                    .zip(row.find(BACK))
                                    .is_some_and(|(a, b)| a < b)
                            })
                    })
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW completed exact useful Return geometry/controls\n{}",
            s.screen()
        );
    }
}
fn refresh(s: &mut Session, mouse: bool) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let enabled = s.completed_cells_screen().is_some_and(|screen| {
            crate::support::completed_navigation_controls_v1::owned_action_position(
                screen,
                RETURN,
                REFRESH,
                &[],
            )
            .is_some_and(|(x, y)| screen.cell(y, x).is_some_and(|cell| cell.underline()))
        });
        if enabled {
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual useful enabled metadata Refresh control\n{}",
            s.screen()
        );
    }
    control(s, mouse, RETURN, REFRESH, CONTROLS);
}
fn journey(mouse: bool, personal: bool, lost_uid: bool) {
    let field = if personal {
        Field::Description
    } else {
        Field::Title
    };
    let mut steps = initial_steps(personal);
    let initial = steps.len();
    steps.extend(super::read_phase(field, false, json!(true)));
    let opened = steps.len();
    steps.extend(super::read_phase(field, false, json!(true)));
    let previewed = steps.len();
    steps.extend(super::read_phase(field, false, json!(true)));
    let paid = post(field); // Same final numeric UID precedes cleanup refusal.
    steps.extend(paid);
    let published = steps.len();
    let first_failed = personal && !lost_uid;
    if first_failed {
        let mut failed = read_phase(field);
        failed[4].status = 1;
        failed[4].stdout.clear();
        failed[4].stderr = b"Actual metadata read child unavailable, not a write".to_vec();
        failed.truncate(5);
        steps.extend(failed);
    }
    let mut fresh = read_phase(field);
    if lost_uid {
        fresh[12].stdout = serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
    }
    steps.extend(fresh);
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
    let independent = super::independent(&f);
    open(&mut s, &f, mouse, field, opened);
    fill(&mut s, mouse, field, requested(field));
    preview(&mut s, &f, mouse, field, previewed);
    writable(&f).execute_batch("CREATE TRIGGER metadata_manual_cleanup_v45 BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'metadata-text-v25/%' BEGIN SELECT RAISE(ABORT,'retain genuine manual refresh original'); END;").unwrap();
    let frame = s.completed_cells_end();
    control(
        &mut s,
        mouse,
        &form_title(field),
        "[Confirm action]",
        FOOTER,
    );
    s.wait_for_command_count(&f, published);
    form_frame(&mut s, frame, RETURN, &["DURABLY CONFIRMED", REFRESH, BACK]);
    assert_original(&f, field, "confirmed", true);
    let original = protected(&f);
    let original_draft = draft(&f);
    let original_marker = marker(&f);
    writable(&f)
        .execute_batch("DROP TRIGGER metadata_manual_cleanup_v45")
        .unwrap();
    resize_return(&mut s, 80, 24);
    resize_return(&mut s, 120, 40);
    s.capture_completed_terminal_v1("metadata-manual-genuine-confirmed-readonly-return");
    exact(&s, &f, &steps[..published]);
    if first_failed {
        let before = complete_sql_v46(&f)
            .into_iter()
            .filter(|row| !row.starts_with("sync_reservations:"))
            .collect::<Vec<_>>();
        let expected_reservation = next_personal(&reservations(&f));
        let frame = s.completed_cells_end();
        refresh(&mut s, mouse);
        s.wait_for_command_count(&f, published + 5);
        form_frame(&mut s, frame, RETURN, &["Read failed", REFRESH, BACK]);
        assert_eq!(protected(&f), original);
        assert_eq!(
            complete_sql_v46(&f)
                .into_iter()
                .filter(|row| !row.starts_with("sync_reservations:"))
                .collect::<Vec<_>>(),
            before
        );
        assert_eq!(reservations(&f), expected_reservation);
        exact(&s, &f, &steps[..published + 5]);
    }
    let before_sql = complete_sql_v46(&f);
    let expected_reservation = personal.then(|| next_personal(&reservations(&f)));
    let before = s.completed_cursorless_end();
    refresh(&mut s, mouse);
    s.wait_for_command_count(&f, steps.len());
    s.wait_for_new_cursorless_condition(
        before,
        |screen| {
            let text = screen.contents();
            crate::support::main_footer_visible(screen)
                && !text.contains(RETURN)
                && !text.contains(REFRESH)
                && if lost_uid {
                    !text.contains(requested(field).lines().next().unwrap())
                        && !text.contains(DETAIL_BODY)
                } else {
                    text.contains("Fresh original PR/search observation")
                        && text.contains(requested(field).lines().next().unwrap())
                        && crate::support::controls::control_painted(screen, "[b Back]")
                }
        },
        "Actual new Metadata page or exact numeric-account privacy retirement after full13 read",
        Duration::from_secs(8),
    );
    assert_eq!(protected(&f), original);
    assert_eq!(draft(&f), original_draft);
    assert_eq!(marker(&f), original_marker);
    assert_original(&f, field, "confirmed", true);
    if lost_uid {
        assert_eq!(
            complete_sql_v46(&f)
                .into_iter()
                .filter(|row| !row.starts_with("sync_reservations:"))
                .collect::<Vec<_>>(),
            before_sql
                .into_iter()
                .filter(|row| !row.starts_with("sync_reservations:"))
                .collect::<Vec<_>>()
        );
        assert_eq!(reservations(&f), expected_reservation.unwrap());
    } else {
        s.capture_completed_terminal_v1("metadata-manual-actual-fresh-source-page");
        let observed:String = raw(&f).query_row("SELECT json_extract(payload,?1) FROM records WHERE host=?2 AND login='user-id:42' AND kind='pr-details' AND id='GENUINE_CANONICAL_PR_NODE_1007'", rusqlite::params![if field==Field::Title {"$.summary.pr.title"} else {"$.summary.pr.body"}, HOST], |row|row.get(0)).unwrap();
        assert_eq!(observed, requested(field));
    }
    assert_eq!(
        raw(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT count(*) FROM pr_write_attempts WHERE state='acknowledged'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    finish(s, &f, mouse, &steps, &git, &config, &independent);
}

#[test]
fn pr_metadata_original_v25_keyboard_manual_refresh_catalogue_personal_retry_and_uid_loss() {
    journey(false, false, false);
    journey(false, true, false);
    journey(false, true, true);
}
#[test]
fn pr_metadata_original_v25_mouse_manual_refresh_catalogue_personal_retry_and_uid_loss() {
    journey(true, false, false);
    journey(true, true, false);
    journey(true, true, true);
}
