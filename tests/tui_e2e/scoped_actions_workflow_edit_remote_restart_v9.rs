//! SOURCE ONLY / UNRUN. Real process restart and independently fresh Catalogue
//! source; persisted IDs select READs, never restore a Commit/permission.
use super::*;
const ACTIONS: &str = "Actions • focused view ─";
const CATALOGUE: &str = "Repositories • explicit discovery";
#[derive(Clone, Copy)]
enum RestartV9 {
    ConfirmedBranch,
    ConfirmedPr,
    AcknowledgedPr,
}
fn list_phase_v9() -> Vec<GhStep> {
    let mut steps = principal(42).into_iter().map(host_step).collect::<Vec<_>>();
    steps.extend((0..2).map(|_| remote_get(&format!("repos/{FIRST}"), current_repository())));
    steps.extend(principal(42).into_iter().map(host_step));
    assert_eq!(steps.len(), 6);
    steps
}
fn actions_body_v9(s: &Session) -> Option<String> {
    s.completed_cells_screen()
        .and_then(|frame| crate::support::overlay_body(frame, ACTIONS))
}
fn actual_actions_v9(s: &mut Session, after: usize, words: &[&str]) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::main_footer_visible(frame)
                    && crate::support::locate_control(frame, "[b Cancel read]").is_none()
                    && actions_body_v9(s).is_some_and(|body| {
                        words.iter().all(|word| body.contains(word))
                            && !body.contains("Original current read pending")
                    })
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW complete exact Actions facts {words:?}\n{}",
            s.screen()
        );
    }
}
fn actual_ack_v9(s: &mut Session, after: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        // The recovery result keeps Refresh history and Back in the key bar, and the
        // acknowledged control and busy Cancel read have left it.
        if s.completed_cells_end() > after
            && actions_body_v9(s)
                .is_some_and(|body| body.contains("Inspect original Actions publication"))
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::locate_control(frame, "[r Refresh history]").is_some()
                    && crate::support::locate_control(frame, "[b Back]").is_some()
                    && crate::support::locate_control(frame, "[b Cancel read]").is_none()
            })
            && s.completed_actions_control("Acknowledge inspected state")
                .is_none()
        {
            return;
        }
        assert!(Instant::now() < until, "NEW nonbusy actual ACK result: prior actionable Inspect and busy read are ineligible\n{}", s.screen());
    }
}
fn actions_fact_v9(s: &mut Session, mouse: bool, fact: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let current = actions_body_v9(s).expect("SAME completed owned historical Actions body");
        assert!(s
            .completed_cells_screen()
            .is_some_and(
                |frame| crate::support::locate_control(frame, "[b Cancel read]").is_none()
            ));
        if current.contains(fact) {
            return;
        }
        let end = s.completed_cells_end();
        let size = s.completed_cells_screen().unwrap().size();
        if mouse {
            s.wheel(size.1 / 2, size.0.saturating_sub(5), true);
        } else {
            s.send_key(b"\x1b[6~");
        }
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > end
                && actions_body_v9(s).is_some_and(|next| next != current)
            {
                break;
            }
            assert!(
                Instant::now() < until,
                "ONE actual scroll needs NEW changed SAME owned body, no hidden facts\n{}",
                s.screen()
            );
        }
        assert!(
            Instant::now() < until,
            "Complete factual line never visible: {fact}\n{}",
            s.screen()
        );
    }
}
fn fresh_source_v9(r: &RemoteFixture, mouse: bool, offset: usize) -> Session {
    let mut s = launch(&r.fixture, mouse);
    let prior = commands(&s, &r.fixture);
    assert_eq!(prior.len(), offset);
    choose(&mut s, mouse, "[: Commands]", b":");
    overlay_ready(
        &mut s,
        "Commands · current context",
        &["Filter focused:", "[Enter Choose]"],
    );
    s.send_key(b"\x01");
    s.paste("Discover repositories");
    overlay_ready(
        &mut s,
        "Commands · current context",
        &["> Discover repositories"],
    );
    choose(&mut s, mouse, "> Discover repositories", b"\r");
    s.wait_for_overlay_close("Commands · current context");
    ready(&mut s, &[CATALOGUE, "[s Source]", "[Esc Close]"]);
    assert_eq!(
        commands(&s, &r.fixture),
        prior,
        "Pure restart/open never replays a saved publication"
    );
    let end = s.completed_cells_end();
    discover(&mut s, mouse);
    s.wait_for_command_count(&r.fixture, offset + 7);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > end
            && s.completed_cells_screen().is_some_and(|frame| {
                crate::support::overlay_body(frame, CATALOGUE).is_some_and(|body| {
                    body.contains(FIRST)
                        && body.contains("Repository ID: 10000")
                        && body.contains("Selected: no")
                        && body.contains("[/ Filter]")
                })
            })
        {
            break;
        }
        assert!(Instant::now() < until, "NEW genuinely loaded Catalogue before Filter (complete cells, not cursor coordinates)\n{}", s.screen());
    }
    filter(&mut s, mouse, "repo0000", FIRST, "Repository ID: 10000");
    choose(&mut s, mouse, "[Open Actions]", b"A");
    s.wait_for_command_count(&r.fixture, offset + 14);
    s.wait_for_overlay_close(CATALOGUE);
    actual_actions_v9(
        &mut s,
        0,
        &[
            CURRENT,
            "0 runs of 0 reported",
            "No records reported in this scope.",
        ],
    );
    s
}
fn open_publications_v9(s: &mut Session, mouse: bool) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let ready = s.completed_cells_screen().is_some_and(|frame| {
            crate::support::overlay_body(frame, ACTIONS).is_some_and(|body| {
                body.contains(CURRENT)
                    && body.contains("0 runs of 0 reported • complete")
                    && body.contains("No records reported in this scope.")
            }) && crate::support::locate_control(frame, "[b Cancel read]").is_none()
                && !frame.contents().contains("Commands · current context")
        }) && s.completed_actions_control("[P Publications]").is_some();
        if ready {
            break;
        }
        assert!(
            Instant::now() < until,
            "NEW complete exact current empty-Runs frame with the Publications token\n{}",
            s.screen()
        );
    }
    s.activate_completed_actions_control(mouse, "[P Publications]", b"P");
}
fn source_reservation_only_v9(
    before: &[(String, Vec<Vec<rusqlite::types::Value>>)],
    after: &[(String, Vec<Vec<rusqlite::types::Value>>)],
) {
    use rusqlite::types::Value as V;
    let mut expected = before.to_vec();
    let (_, rows) = expected
        .iter_mut()
        .find(|(table, _)| table == "sync_reservations")
        .unwrap();
    assert_eq!(rows.len(), 1);
    let row = &mut rows[0];
    assert_eq!(row.len(), 5);
    assert_eq!(row[0], V::Text(REMOTE_HOST.into()));
    assert_eq!(row[1], V::Text("user-id:42".into()));
    assert!(matches!(&row[2],V::Text(key) if key.starts_with("repository-catalogue-source-v1:")));
    assert_eq!(row[4], V::Integer(1));
    let V::Integer(generation) = row[3] else {
        panic!("Actual existing reservation generation required");
    };
    assert!(generation > 0);
    row[3] = V::Integer(generation.checked_add(1).unwrap());
    assert_eq!(after,expected,"Only the ONE actual second Catalogue reservation advances; every unrelated SQL row stays exact");
}
fn restart_v9(mouse: bool, mode: RestartV9) {
    let pr = !matches!(mode, RestartV9::ConfirmedBranch);
    let already_ack = matches!(mode, RestartV9::AcknowledgedPr);
    let mut r = remote_fixture();
    let original = git_state(&r.fixture);
    let config = fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap();
    let git_config = fs::read(r.fixture.repo().join(".git/config")).unwrap();
    let mut s = start(&r.fixture, mouse);
    let (root, d) = actual_local_commit(&r, &mut s, mouse);
    let post_d = git_state(&r.fixture);
    assert_eq!(
        (
            &post_d.head,
            &post_d.index,
            &post_d.status,
            &post_d.staged_diff,
            &post_d.working_diff,
            &post_d.dirty,
            &post_d.staged,
            &post_d.untracked
        ),
        (
            &original.head,
            &original.index,
            &original.status,
            &original.staged_diff,
            &original.working_diff,
            &original.dirty,
            &original.staged,
            &original.untracked
        )
    );
    let mut refs = original.refs.lines().map(str::to_owned).collect::<Vec<_>>();
    refs.push(format!("{d} refs/heads/{BRANCH}"));
    refs.sort();
    let mut actual = post_d.refs.lines().map(str::to_owned).collect::<Vec<_>>();
    actual.sort();
    assert_eq!(actual, refs);
    r.steps.extend(remote_reads(&r.c, &d, false));
    r.steps.extend(remote_confirm(&r.c, &d, false));
    if pr {
        r.steps.extend(remote_reads(&r.c, &d, true));
        r.steps.extend(remote_confirm(&r.c, &d, true));
    }
    if already_ack {
        r.steps.extend(history_phase_v9(&r.c, &d, true));
        r.steps.extend(history_phase_v9(&r.c, &d, true));
    }
    r.fixture.scripted_gh(&r.steps);
    let mut count = 109;
    preview(&mut s, &r, mouse, &mut count, false, &d);
    confirm(&mut s, &r, mouse, &mut count, false);
    if pr {
        preview(&mut s, &r, mouse, &mut count, true, &d);
        confirm(&mut s, &r, mouse, &mut count, true);
    }
    let attempts = if pr { 2 } else { 1 };
    positive_aliases(&r, attempts);
    if already_ack {
        let end = s.completed_cells_end();
        action(&mut s, mouse, "[F4 View]");
        count += 12;
        s.wait_for_command_count(&r.fixture, count);
        remote_cells(&mut s, end, &["ORIGINAL ACTIONS ATTEMPT"]);
        let end = s.completed_cells_end();
        action(&mut s, mouse, "[F6 ACK local]");
        count += 12;
        s.wait_for_command_count(&r.fixture, count);
        remote_cells(&mut s, end, &["ORIGINAL ACTIONS ATTEMPT"]);
        visible_fact_v9(
            &mut s,
            mouse,
            "Explicit W9 observation ACK committed locally",
        );
    }
    let saved = journal(&r.fixture);
    assert_eq!(saved.len(), attempts);
    assert_eq!(
        saved.last().unwrap().1,
        if already_ack {
            "acknowledged"
        } else {
            "confirmed"
        }
    );
    let protected = raw_tables_v9(&r.fixture);
    close(&mut s, mouse);
    s.quit_and_assert_restored_terminal(mouse);
    exact_remote(&s, &r.fixture, &r.steps);
    let before_discovery = remote_sql(&r.fixture);
    let mut opening = actions_protocol();
    opening[10].stdout = json!({"total_count":0,"workflow_runs":[]})
        .to_string()
        .into_bytes();
    r.steps.extend(opening.into_iter().map(host_step));
    r.steps.extend(list_phase_v9());
    r.steps.extend(history_phase_v9(&r.c, &d, pr));
    if !already_ack {
        r.steps.extend(history_phase_v9(&r.c, &d, pr));
    }
    r.fixture.scripted_gh(&r.steps);
    let mut s = fresh_source_v9(&r, mouse, count);
    count += 14;
    source_reservation_only_v9(&before_discovery, &remote_sql(&r.fixture));
    assert_eq!(
        raw_tables_v9(&r.fixture),
        protected,
        "Fresh Source never acknowledges/rebases saved write"
    );
    let unrelated = remote_sql(&r.fixture);
    let end = s.completed_cells_end();
    open_publications_v9(&mut s, mouse);
    count += 6;
    s.wait_for_command_count(&r.fixture, count);
    let target = format!("Original #{attempts}");
    let state = if already_ack {
        "Acknowledged"
    } else {
        "Confirmed"
    };
    actual_actions_v9(
        &mut s,
        end,
        &["Original Actions publications", &target, state],
    );
    assert_eq!(raw_tables_v9(&r.fixture), protected);
    let end = s.completed_cells_end();
    selected_row(&mut s, mouse, &target);
    count += 12;
    s.wait_for_command_count(&r.fixture, count);
    actual_actions_v9(
        &mut s,
        end,
        &[
            "Inspect original Actions publication",
            "ORIGINAL ACTIONS ATTEMPT",
        ],
    );
    for fact in [
        format!("Saved C {}", r.c),
        format!("Saved D {d}"),
        format!("Saved exact head ref {BRANCH}"),
    ] {
        actions_fact_v9(&mut s, mouse, &fact);
    }
    assert_eq!(
        raw_tables_v9(&r.fixture),
        protected,
        "Fresh historical observation is READ-only"
    );
    if !already_ack {
        let end = s.completed_cells_end();
        if mouse {
            let (x, y) = s.wait_for_control("[Acknowledge inspected state]");
            s.click(x, y);
        } else {
            s.focus_completed_actions_control_v2("Acknowledge inspected state");
            s.send_key(b"\r");
        }
        count += 12;
        s.wait_for_command_count(&r.fixture, count);
        actual_ack_v9(&mut s, end);
        for fact in [
            "Explicit W9 observation ACK committed locally",
            "effect UNVERIFIED",
            "original receipt retained if present",
            "no remote request repeated",
        ] {
            actions_fact_v9(&mut s, mouse, fact);
        }
    }
    let terminal = journal(&r.fixture);
    assert_eq!(terminal.len(), saved.len());
    for (index, (before, after)) in saved.iter().zip(&terminal).enumerate() {
        assert_eq!(
            (&after.0, &after.2, &after.3),
            (&before.0, &before.2, &before.3)
        );
        assert_eq!(
            after.1,
            if index + 1 == attempts {
                "acknowledged"
            } else {
                "confirmed"
            }
        );
    }
    let db = publication_v3::raw(&r.fixture);
    let generations = db
        .prepare("SELECT generation FROM actions_write_generations ORDER BY resource_key")
        .unwrap()
        .query_map([], |row| row.get::<_, i64>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(generations, vec![(3 * attempts + 1) as i64; 6]);
    assert_eq!(
        db.query_row("SELECT count(*) FROM actions_write_resources", [], |row| {
            row.get::<_, usize>(0)
        })
        .unwrap(),
        6 * attempts
    );
    assert_eq!(db.query_row("SELECT count(*) FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'scoped-actions-v9/%'",[REMOTE_HOST],|row|row.get::<_,usize>(0)).unwrap(),0);
    let terminal_sql = sql(&r.fixture);
    let terminal_protected = raw_tables_v9(&r.fixture);
    let end = s.completed_cells_end();
    if mouse {
        let (x, y) = s.wait_for_control("[b Back]");
        s.click(x, y);
    } else {
        s.send_key(b"b");
    }
    actual_actions_v9(
        &mut s,
        end,
        &["0 runs of 0 reported", "No records reported in this scope."],
    );
    assert_eq!(commands(&s, &r.fixture).len(), count);
    assert_eq!(sql(&r.fixture), terminal_sql);
    assert_eq!(raw_tables_v9(&r.fixture), terminal_protected);
    assert_eq!(remote_sql(&r.fixture), unrelated);
    assert_eq!(git_state(&r.fixture), post_d);
    assert_eq!(
        fs::read(r.fixture.repo().join(".git/config")).unwrap(),
        git_config
    );
    assert_eq!(
        fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(
        r.fixture
            .git(&["-C", root.to_str().unwrap(), "rev-parse", "HEAD"])
            .trim(),
        d
    );
    assert_eq!(
        bare_git(&r.tls, &["rev-parse", "refs/heads/workflow/edit"]).trim(),
        d
    );
    assert_eq!(
        bare_git(&r.tls, &["rev-parse", "refs/heads/main"]).trim(),
        r.c
    );
    s.capture_completed_terminal_v1("task54-w9-fresh-source-restart-typed-history-ack");
    s.quit_and_assert_restored_terminal(mouse);
    exact_remote(&s, &r.fixture, &r.steps);
    assert_eq!(sql(&r.fixture), terminal_sql);
    assert_eq!(git_state(&r.fixture), post_d);
    cleanup(r.tls);
}
#[test]
fn keyboard_task54_workflow_edit_v9_real_restart_confirmed_branch_fresh_source_inspect_ack() {
    restart_v9(false, RestartV9::ConfirmedBranch)
}
#[test]
fn mouse_task54_workflow_edit_v9_real_restart_confirmed_branch_fresh_source_inspect_ack() {
    restart_v9(true, RestartV9::ConfirmedBranch)
}
#[test]
fn keyboard_task54_workflow_edit_v9_real_restart_confirmed_pr_fresh_source_inspect_ack() {
    restart_v9(false, RestartV9::ConfirmedPr)
}
#[test]
fn mouse_task54_workflow_edit_v9_real_restart_confirmed_pr_fresh_source_inspect_ack() {
    restart_v9(true, RestartV9::ConfirmedPr)
}
#[test]
fn keyboard_task54_workflow_edit_v9_real_restart_acknowledged_pr_stays_read_only_without_resend() {
    restart_v9(false, RestartV9::AcknowledgedPr)
}
#[test]
fn mouse_task54_workflow_edit_v9_real_restart_acknowledged_pr_stays_read_only_without_resend() {
    restart_v9(true, RestartV9::AcknowledgedPr)
}
