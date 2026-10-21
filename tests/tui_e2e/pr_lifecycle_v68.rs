//! Real original-family State entry, bodyless initial Saved ACK, exact10-role
//! Preview and pure Close. No app injection, fake Source or effect screenshot.
//! Full mutation/queue-recovery families remain required, not aliased here.
use super::*;
#[path = "pr_lifecycle_source_v80.rs"]
mod source_v80;
use std::collections::BTreeMap;
const STATE_QUERY: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanClose viewerCanReopen repository{id nameWithOwner}}}}";
const STATE_INTENT: &[&str] = &["[Preview]", "[Esc Close]"];
const STATE_PREVIEW: &[&str] = &["[Preview]", "[Confirm action]", "[Esc Close]"];
type SqlRows = BTreeMap<String, Vec<Vec<rusqlite::types::Value>>>;
fn all_rows(f: &Fixture) -> SqlRows {
    let db = raw(f);
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([],|r|r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    let mut result = BTreeMap::new();
    for name in names {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |r| {
                (0..columns)
                    .map(|i| r.get(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .collect::<Result<Vec<Vec<rusqlite::types::Value>>, _>>()
            .unwrap();
        rows.sort_by_cached_key(|r| format!("{r:?}"));
        result.insert(name, rows);
    }
    result
}
fn state_parent() -> Value {
    let mut parent = parent_resource();
    parent["issue_url"] = json!(format!("https://{HOST}/api/v3/repos/{FIRST}/issues/7"));
    parent
}
fn state_phase(capability: Value) -> Vec<GhStep> {
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), state_parent()));
    calls.push(get(
        &format!("repos/{FIRST}/issues/7"),
        json!({
            "id":2007,"node_id":"INDEPENDENT_ISSUE_2007","number":7,"state":"open",
            "url":format!("https://{HOST}/api/v3/repos/{FIRST}/issues/7"),
            "html_url":format!("https://{HOST}/{FIRST}/pull/7"),
            "repository_url":format!("https://{HOST}/api/v3/repos/{FIRST}"),
            "pull_request":{"url":format!("https://{HOST}/api/v3/repos/{FIRST}/pulls/7")}
        }),
    ));
    calls.push(graphql(STATE_QUERY,json!({"id":"GENUINE_CANONICAL_PR_NODE_1007"}),json!({"data":{"node":{
        "__typename":"PullRequest","id":"GENUINE_CANONICAL_PR_NODE_1007","fullDatabaseId":"1007","number":7,
        "headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"headRefName":"feature/scoped-open","baseRefName":"main",
        "state":"OPEN","isDraft":false,"merged":false,"viewerCanClose":capability,"viewerCanReopen":false,
        "repository":{"id":"REPO_catalogue_10000","nameWithOwner":FIRST}
    }}})));
    calls.push(get(&format!("repos/{FIRST}/pulls/7"), state_parent()));
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.extend(principal(42));
    assert_eq!(calls.len(), 10, "ONE original Core6 plus fixed State pool4");
    calls
}
fn state_title() -> String {
    format!("Close without merging PR • {FIRST}#7")
}
fn state_form(s: &mut Session, after: usize, preview: bool) {
    form_frame(
        s,
        after,
        &state_title(),
        if preview { STATE_PREVIEW } else { STATE_INTENT },
    );
    let body =
        crate::support::overlay_body(s.completed_cells_screen().unwrap(), &state_title()).unwrap();
    assert!(body.contains(if preview {
        "READ-ONLY PREVIEW"
    } else {
        "PR INTENT • Preview first"
    }));
    assert!(
        body.contains("account 42")
            && !body.contains("Some(")
            && !body.contains("[External editor]")
    );
    if !preview {
        assert!(!body.contains("[Confirm action]"));
    }
}
fn resize_state(s: &mut Session, width: u16, height: u16, controls: &[&str]) {
    let after = s.completed_cells_end();
    s.resize(width, height);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|screen| {
                screen.size() == (height, width)
                    && crate::support::main_footer_visible(screen)
                    && crate::support::overlay_body(screen, &state_title()).is_some_and(|body| {
                        body.lines().any(|row| {
                            let mut rest = row;
                            controls.iter().all(|label| {
                                if let Some(offset) = rest.find(label) {
                                    rest = &rest[offset + label.len()..];
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
            Instant::now() < deadline,
            "NEW exact centered State Form and ordered controls at {width}x{height}\n{}",
            s.screen()
        );
    }
}
fn saved_bodyless(s: &mut Session, f: &Fixture) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(2));
        let rows=raw(f).prepare("SELECT resource,body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'state-v68/%' ORDER BY resource").unwrap()
            .query_map([HOST],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?))).unwrap()
            .collect::<Result<Vec<_>,_>>().unwrap();
        if rows.len() == 1
            && s.completed_cells_screen().is_some_and(|screen| {
                crate::support::overlay_body(screen, &state_title()).is_some()
                    && screen
                        .contents()
                        .contains("Draft saved locally • nothing published")
            })
        {
            assert_eq!(rows[0].0.len(), "state-v68/".len() + 64);
            assert_eq!(
                rows[0].1,
                "{\"version\":17,\"values\":{},\"publication\":null}"
            );
            return;
        }
        if Instant::now() >= deadline {
            s.save_artifacts();
            panic!(
                "Actual State initial Saved ACK and exact bodyless envelope missing\n{}",
                s.screen()
            );
        }
    }
}
fn assert_no_attempt(f: &Fixture) {
    for table in [
        "pr_write_attempts",
        "pr_write_state_origins_v68",
        "pr_write_state_dispositions_v68",
        "pr_write_state_filter_fences_v68",
        "pr_write_state_source_watermarks_v73",
    ] {
        assert_eq!(
            raw(f)
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r
                    .get::<_, usize>(0))
                .unwrap(),
            0
        );
    }
}
fn close_current(s: &mut Session, mouse: bool, controls: &[&str]) {
    let before = s.completed_cursorless_end();
    control(s, mouse, &state_title(), "[Esc Close]", controls);
    s.wait_for_new_cursorless_condition(
        before,
        |screen| {
            crate::support::main_footer_visible(screen)
                && crate::support::controls::control_painted(screen, "[b Back]")
                && !screen.contents().contains("[Esc Close]")
                && !screen.contents().contains("READ-ONLY PREVIEW")
        },
        "NEW original PR after exact State Close",
        Duration::from_secs(8),
    );
}
fn state_preview_cancel(mouse: bool) {
    for personal in [false, true] {
        let mut steps = initial_steps(personal);
        let initial = steps.len();
        steps.extend(state_phase(json!(true)));
        let opened = steps.len();
        steps.extend(state_phase(json!(true)));
        let f = resource_configured(&steps);
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
        let before = s.completed_cells_end();
        crate::pr_actions::menu(&mut s, mouse, "Close without merging");
        s.wait_for_command_count(&f, opened);
        state_form(&mut s, before, false);
        saved_bodyless(&mut s, &f);
        let stable = all_rows(&f);
        assert_no_attempt(&f);
        let before = s.completed_cells_end();
        control(&mut s, mouse, &state_title(), "[Preview]", STATE_INTENT);
        s.wait_for_command_count(&f, steps.len());
        state_form(&mut s, before, true);
        // The actual prepared document is paged at80x24. Inspect its numeric
        // identity/effect facts in a NEW larger centered owned Form, not by
        // assuming every prepared source line is visible in the small frame.
        resize_state(&mut s, 120, 40, STATE_PREVIEW);
        let body =
            crate::support::overlay_body(s.completed_cells_screen().unwrap(), &state_title())
                .unwrap();
        assert!(
            body.contains("repository #10000")
                && body.contains("PR #1007")
                && body.contains("Independent issue #2007")
                && body.contains("requested closed")
        );
        // TASK-25 AC6: the requesting account is its own preview row with
        // the shared filled initials chip, like the PR action previews. The
        // mouse launch runs under NO_COLOR, where the chip is `[OC] octocat`.
        crate::pr_actions::wait_preview_actor_chips(
            &mut s,
            &[crate::pr_actions::ACCOUNT_ROW],
            mouse,
        );
        assert_eq!(
            all_rows(&f),
            stable,
            "Readonly State Preview preserves EVERY raw SQL row"
        );
        assert_no_attempt(&f);
        exact(&s, &f, &steps);
        resize_state(&mut s, 80, 24, STATE_PREVIEW);
        close_current(&mut s, mouse, STATE_PREVIEW);
        assert_eq!(
            all_rows(&f),
            stable,
            "Pure Close cannot mutate the Saved body, journal or cache"
        );
        assert_eq!(git_state(&f), git);
        assert_eq!(
            fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        exact(&s, &f, &steps);
        assert_read_only_git(&f);
        s.quit_and_assert_restored_terminal(mouse);
    }
}
fn state_unavailable(mouse: bool) {
    for personal in [false, true] {
        for value in [json!(false), Value::Null] {
            let mut steps = initial_steps(personal);
            let initial = steps.len();
            steps.extend(state_phase(value));
            let f = resource_configured(&steps);
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
            let before = s.completed_cells_end();
            crate::pr_actions::menu(&mut s, mouse, "Close without merging");
            s.wait_for_command_count(&f, steps.len());
            state_form(&mut s, before, false);
            let stable = all_rows(&f);
            let before = s.completed_cells_end();
            control(&mut s, mouse, &state_title(), "[Preview]", STATE_INTENT);
            form_frame(
                &mut s,
                before,
                &state_title(),
                &[
                    "Actual command capability unavailable or denied",
                    "[Preview]",
                    "[Esc Close]",
                ],
            );
            assert_no_attempt(&f);
            assert_eq!(
                all_rows(&f),
                stable,
                "Denied/null capability queues no read, Save or attempt"
            );
            assert_eq!(git_state(&f), git);
            exact(&s, &f, &steps);
            // Error scrolling controls are real footer actions, not skipped
            // by a no-op Tab. Escape directly invokes the same owned Close.
            let before = s.completed_cursorless_end();
            let close = s.wait_for_owned_completed_action(&state_title(), "[Esc Close]", &[]);
            if mouse {
                s.click(close.0, close.1);
            } else {
                s.send_key(b"\x1b");
            }
            s.wait_for_new_cursorless_condition(
                before,
                |screen| {
                    crate::support::main_footer_visible(screen)
                        && !screen.contents().contains("[Esc Close]")
                },
                "Denied State same-action Close",
                Duration::from_secs(8),
            );
            assert_eq!(all_rows(&f), stable);
            assert_eq!(
                fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
                config
            );
            assert_read_only_git(&f);
            s.quit_and_assert_restored_terminal(mouse);
        }
    }
}
#[test]
fn pr_lifecycle_v68_keyboard_bodyless_saved_preview_cancel_preserves_all_rows_and_git() {
    state_preview_cancel(false);
}
#[test]
fn pr_lifecycle_v68_mouse_bodyless_saved_preview_cancel_preserves_all_rows_and_git() {
    state_preview_cancel(true);
}
#[test]
fn pr_lifecycle_v68_keyboard_actual_false_null_capability_retains_readonly_intent_with_zero_effects(
) {
    state_unavailable(false);
}
#[test]
fn pr_lifecycle_v68_mouse_actual_false_null_capability_retains_readonly_intent_with_zero_effects() {
    state_unavailable(true);
}
