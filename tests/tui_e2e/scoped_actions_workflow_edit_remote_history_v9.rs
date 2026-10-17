//! UNRUN actual full local Commit/TLS Push/201 PR then SAME retained Form
//! READ/explicit local ACK. No saved Commit/permission or handler substitutes.
use super::*;
#[path = "scoped_actions_workflow_edit_remote_restart_v9.rs"]
mod restart_v9;

fn resource_v9(c: &str, d: &str, pr: bool) -> GhStep {
    let value = if pr {
        canonical_pr(c, d)
    } else {
        json!({"ref":format!("refs/heads/{BRANCH}"),"object":{"type":"commit","sha":d}})
    };
    let body = value.to_string();
    GhStep::read(
        &[
            "api",
            &if pr {
                format!("repos/{CURRENT}/pulls/91")
            } else {
                format!("repos/{CURRENT}/git/ref/heads/workflow%2Fedit")
            },
            "--hostname",
            REMOTE_HOST,
            "--method",
            "GET",
            "--include",
        ],
        &format!(
            "HTTP/2 200 OK\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    )
}
fn history_phase_v9(c: &str, d: &str, pr: bool) -> Vec<GhStep> {
    let mut out = principal(42).into_iter().map(host_step).collect::<Vec<_>>();
    out.push(remote_get(&format!("repos/{FIRST}"), current_repository()));
    out.extend((0..2).map(|_| remote_get("repositories/10000", numeric_repository())));
    out.push(resource_v9(c, d, pr));
    out.push(resource_v9(c, d, pr));
    out.extend((0..2).map(|_| remote_get("repositories/10000", numeric_repository())));
    out.push(remote_get(&format!("repos/{FIRST}"), current_repository()));
    out.extend(principal(42).into_iter().map(host_step));
    assert_eq!(
        out.len(),
        12,
        "SAME repository-origin Core6+six historical roles, no hidden child"
    );
    out
}
fn raw_tables_v9(f: &Fixture) -> Vec<(String, Vec<Vec<rusqlite::types::Value>>)> {
    let mut rows = sql(f);
    rows.retain(|(name, _)| {
        matches!(
            name.as_str(),
            "actions_write_attempts" | "actions_write_resources" | "actions_write_generations"
        )
    });
    rows
}
fn visible_fact_v9(s: &mut Session, mouse: bool, fact: &str) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let current = body(s).expect("SAME actual completed W9 Form");
        assert!(
            !current.contains("[Esc Cancel]"),
            "Never scroll a still-busy historical read"
        );
        assert!(labels(&current).iter().all(|label| current.contains(label)));
        if current.contains(fact) {
            return;
        }
        let after = s.completed_cells_end();
        let size = s.completed_cells_screen().unwrap().size();
        if mouse {
            s.wheel(size.1 / 2, size.0 / 2, true);
        } else {
            s.send_key(b"\x1b[6~");
        }
        loop {
            s.poll_output(Duration::from_millis(1));
            if s.completed_cells_end() > after && body(s).is_some_and(|next| next != current) {
                break;
            }
            assert!(
                Instant::now() < until,
                "ONE viewport input needs NEW changed SAME owned history body\n{}",
                s.screen()
            );
        }
        assert!(
            Instant::now() < until,
            "Full factual text must actually be visible: {fact}\n{}",
            s.screen()
        );
    }
}
fn own_history_v9(mouse: bool, pr: bool) {
    let mut r = remote_fixture();
    let original = git_state(&r.fixture);
    let config = fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = start(&r.fixture, mouse);
    let (root, d) = actual_local_commit(&r, &mut s, mouse);
    let post_d = git_state(&r.fixture);
    assert_eq!(post_d.head, original.head);
    assert_eq!(post_d.index, original.index);
    assert_eq!(post_d.status, original.status);
    assert_eq!(post_d.staged_diff, original.staged_diff);
    assert_eq!(post_d.working_diff, original.working_diff);
    assert_eq!(post_d.dirty, original.dirty);
    assert_eq!(post_d.staged, original.staged);
    assert_eq!(post_d.untracked, original.untracked);
    let mut expected = original.refs.lines().map(str::to_owned).collect::<Vec<_>>();
    expected.push(format!("{d} refs/heads/{BRANCH}"));
    expected.sort();
    let mut actual = post_d.refs.lines().map(str::to_owned).collect::<Vec<_>>();
    actual.sort();
    assert_eq!(actual, expected, "Only actual isolated D branch added");
    let unrelated = remote_sql(&r.fixture);
    r.steps.extend(remote_reads(&r.c, &d, false));
    r.steps.extend(remote_confirm(&r.c, &d, false));
    if pr {
        r.steps.extend(remote_reads(&r.c, &d, true));
        r.steps.extend(remote_confirm(&r.c, &d, true));
    }
    r.steps.extend(history_phase_v9(&r.c, &d, pr));
    r.steps.extend(history_phase_v9(&r.c, &d, pr));
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
    let rows_before = journal(&r.fixture);
    let protected_before = raw_tables_v9(&r.fixture);
    let after = s.completed_cells_end();
    action(&mut s, mouse, "[F4 View]");
    count += 12;
    s.wait_for_command_count(&r.fixture, count);
    remote_cells(&mut s, after, &["ORIGINAL ACTIONS ATTEMPT"]);
    assert_eq!(
        raw_tables_v9(&r.fixture),
        protected_before,
        "Inspect never changes the original protected row"
    );
    for fact in [
        format!("Saved C {}", r.c),
        format!("Saved D {d}"),
        format!("Saved exact head ref {BRANCH}"),
        "effect/causality UNVERIFIED".into(),
    ] {
        visible_fact_v9(&mut s, mouse, &fact);
    }
    let after = s.completed_cells_end();
    if !mouse && !pr {
        // ONE existing keyboard family exercises the real shortcut only
        // after the SAME full inspected facts and inverse whole ACK target.
        // The PR keyboard family retains its original Tab/Enter activation.
        action_with_activation(&mut s, false, "[F6 ACK local]", b"\x1b[17~");
    } else {
        action(&mut s, mouse, "[F6 ACK local]");
    }
    count += 12;
    s.wait_for_command_count(&r.fixture, count);
    remote_cells(&mut s, after, &["ORIGINAL ACTIONS ATTEMPT"]);
    visible_fact_v9(
        &mut s,
        mouse,
        "Explicit W9 observation ACK committed locally",
    );
    visible_fact_v9(&mut s, mouse, "effect UNVERIFIED");
    let rows_after = journal(&r.fixture);
    assert_eq!(rows_after.len(), rows_before.len());
    for (index, (before, after)) in rows_before.iter().zip(&rows_after).enumerate() {
        assert_eq!(
            (&after.0, &after.2, &after.3),
            (&before.0, &before.2, &before.3),
            "Exact original kind/context/typed receipt never rewritten"
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
    assert_eq!(
        generations,
        vec![(3 * attempts + 1) as i64; 6],
        "One exact ACK advances all and only six original aliases once"
    );
    assert_eq!(
        db.query_row("SELECT count(*) FROM actions_write_resources", [], |row| {
            row.get::<_, usize>(0)
        })
        .unwrap(),
        6 * attempts
    );
    assert_eq!(db.query_row("SELECT count(*) FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'scoped-actions-v9/%'",[REMOTE_HOST],|row|row.get::<_,usize>(0)).unwrap(),0,"Exact successful publication draft cleanup, unrelated drafts independently preserved");
    let acknowledged = raw_tables_v9(&r.fixture);
    let after = s.completed_cells_end();
    action(&mut s, mouse, "[F6 ACK local]");
    remote_cells(&mut s, after, &["effect remains UNVERIFIED"]);
    assert_eq!(
        commands(&s, &r.fixture).len(),
        count,
        "Disabled repeat ACK never creates work/source/clock or a resend"
    );
    assert_eq!(raw_tables_v9(&r.fixture), acknowledged);
    assert_eq!(remote_sql(&r.fixture), unrelated);
    assert_eq!(git_state(&r.fixture), post_d);
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
    assert_eq!(
        fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    s.capture_completed_terminal_v1("task54-w9-actual-own-remote-inspect-local-ack");
    close(&mut s, mouse);
    s.quit_and_assert_restored_terminal(mouse);
    exact_remote(&s, &r.fixture, &r.steps);
    cleanup(r.tls);
}
#[test]
fn keyboard_task54_workflow_edit_v9_actual_push_own_form_inspect_and_explicit_local_ack() {
    own_history_v9(false, false)
}
#[test]
fn mouse_task54_workflow_edit_v9_actual_push_own_form_inspect_and_explicit_local_ack() {
    own_history_v9(true, false)
}
#[test]
fn keyboard_task54_workflow_edit_v9_actual_push_then_pr_own_form_inspect_and_explicit_local_ack() {
    own_history_v9(false, true)
}
#[test]
fn mouse_task54_workflow_edit_v9_actual_push_then_pr_own_form_inspect_and_explicit_local_ack() {
    own_history_v9(true, true)
}
