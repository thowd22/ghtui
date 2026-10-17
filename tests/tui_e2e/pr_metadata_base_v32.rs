//! Actual binary Base member workflow. No line/DTO/text creates membership;
//! strict transport, durable SQL, raw Git/config and owned TTY are independent.
use super::*;
#[path = "pr_metadata_base_atomic_v85.rs"]
mod atomic_v85;
#[path = "pr_metadata_base_feedback_v314.rs"]
mod feedback_v314;
#[path = "pr_metadata_base_tab_v51.rs"]
mod metadata_tab_v51;
const TIP: &str = "cccccccccccccccccccccccccccccccccccccccc";
const QUERY:&str="query($id:ID!){node(id:$id){__typename ... on PullRequest{id fullDatabaseId number headRefOid baseRefOid headRefName baseRefName state isDraft merged viewerCanUpdate repository{id nameWithOwner}}}}";
const PICKER: &str = "PR metadata choices · Base branch";
const PICKER_CONTROLS: &[&str] = &[
    "[Enter Choose]",
    "[Esc Back]",
    "[Ctrl-U Clear]",
    "[PgUp]",
    "[PgDn]",
];
const FORM_CONTROLS: &[&str] = &["[F2 Choose branch]", "[Preview]", "[Esc Close]"];
const PREVIEW_CONTROLS: &[&str] = &["[Preview]", "[Confirm action]", "[Keep draft / Close]"];
const HISTORY_CONTROLS: &[&str] = &["[Inspect current]", "[Acknowledge local]", "[Esc Close]"];
fn title() -> String {
    format!("Change PR Base • {FIRST}#7")
}
fn history_title() -> String {
    format!("Original Base history • {FIRST}#7")
}
fn raw_parent(changed: bool) -> Value {
    let mut raw = parent_resource();
    if changed {
        raw["base"]["sha"] = json!(TIP);
        raw["base"]["ref"] = json!("next");
    }
    raw
}
fn capability(allowed: Value) -> Value {
    json!({"data":{"node":{"__typename":"PullRequest","id":"GENUINE_CANONICAL_PR_NODE_1007","fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"headRefName":"feature/scoped-open","baseRefName":"main","state":"OPEN","isDraft":false,"merged":false,"viewerCanUpdate":allowed,"repository":{"id":"REPO_catalogue_10000","nameWithOwner":FIRST}}}})
}
fn branch(name: &str, tip: &str) -> Value {
    json!({"name":name,"commit":{"sha":tip},"protected":false})
}
fn read(allowed: Value, rows: Option<Value>, selected: Option<&str>) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), raw_parent(false)));
    steps.push(graphql(
        QUERY,
        json!({"id":"GENUINE_CANONICAL_PR_NODE_1007"}),
        capability(allowed),
    ));
    if let Some(rows) = rows {
        steps.push(get(
            &format!("repos/{FIRST}/branches?per_page=100&page=1"),
            rows,
        ));
    }
    if let Some(name) = selected {
        for _ in 0..2 {
            steps.push(get(
                &format!("repos/{FIRST}/branches/{name}"),
                branch(
                    name,
                    if name == "main" {
                        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    } else {
                        TIP
                    },
                ),
            ));
        }
    }
    steps.push(get(&format!("repos/{FIRST}/pulls/7"), raw_parent(false)));
    steps.push(get(&format!("repos/{FIRST}"), repository(0)));
    steps.extend(principal(42));
    assert_eq!(
        steps.len(),
        9 + usize::from(rows_present(&steps)) + 2 * usize::from(selected.is_some())
    );
    steps
}
fn rows_present(steps: &[GhStep]) -> bool {
    steps.iter().any(|s| {
        s.args
            .iter()
            .any(|a| a.contains("branches?per_page=100&page=1"))
    })
}
fn catalogue(allowed: Value) -> Vec<GhStep> {
    read(
        allowed,
        Some(json!([
            branch("main", &"b".repeat(40)),
            branch("next", TIP)
        ])),
        None,
    )
}
fn selected() -> Vec<GhStep> {
    read(json!(true), None, Some("next"))
}
fn receipts() -> Vec<GhStep> {
    vec![
        write(
            &format!("repos/{FIRST}/pulls/7"),
            "PATCH",
            Some(json!({"base":"next"})),
            raw_parent(true),
        ),
        get(&format!("repos/{FIRST}/pulls/7"), raw_parent(true)),
    ]
}
fn visibility() -> Vec<GhStep> {
    let mut calls = visibility_with_comments_v19(json!([comment(501, OLD)]));
    for index in [2, 8] {
        calls[index].stdout = serde_json::to_vec(&raw_parent(true)).unwrap();
    }
    calls
}
fn paid() -> Vec<GhStep> {
    let mut calls = selected();
    calls.extend(receipts());
    calls.extend(visibility());
    assert_eq!(calls.len(), 23);
    calls
}
fn refresh() -> Vec<GhStep> {
    let mut calls = principal(42);
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    let visible = visibility();
    calls.extend(visible[2..9].iter().cloned());
    calls.push(get(&format!("repos/{FIRST}"), repository(0)));
    calls.extend(principal(42));
    assert_eq!(calls.len(), 13);
    calls
}
fn independent(f: &Fixture) -> Vec<String> {
    let db = raw(f);
    ["SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'metadata-base-v32/%' ORDER BY host,login,resource","SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name","SELECT json_array(host,login,kind,id,repository,payload,fetched_at,source_updated_at,complete) FROM records WHERE login='user-id:43' OR kind='base-independent' ORDER BY host,login,kind,id","SELECT json_array(host,login,source,generation,complete) FROM checkpoints WHERE login='user-id:43' ORDER BY host,login,source","SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"].iter().flat_map(|q|db.prepare(q).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn fixture(steps: &[GhStep]) -> Fixture {
    let f = resource_configured(steps);
    for uid in [42, 43] {
        let scope = Scope::github(HOST, Some(uid)).unwrap();
        let cache = Cache::open(&database(&f)).unwrap();
        cache
            .put_draft(&scope, "base-independent", &format!("BASE_KEPT_{uid}"))
            .unwrap();
        cache
            .put(
                &scope,
                &ghtui::storage::Record::new(
                    "base-independent",
                    format!("row-{uid}"),
                    FIRST,
                    json!({"private":format!("KEPT_BASE_{uid}")}),
                ),
            )
            .unwrap();
    }
    f
}
fn enter_base(s: &mut Session, f: &Fixture, mouse: bool, opened: usize) {
    let after = s.completed_cells_end();
    crate::pr_actions::menu(s, mouse, "Change base branch");
    s.wait_for_command_count(f, opened);
    form_frame(s, after, PICKER, PICKER_CONTROLS);
    s.capture_completed_terminal_v1("base-actual-member-picker");
}
fn choose_member(s: &mut Session, mouse: bool, name: &str) {
    // Query starts focused in the actual picker, not a command palette.
    if mouse {
        s.click_visible_action("Filter focused:");
    }
    s.send_key(b"\x01");
    s.paste(name);
    s.wait_for_screen(&format!("Filter focused: {name}"));
    if mouse {
        s.click_visible_action(&format!("◆ eligible · {name}"));
    }
    picker_control(s, mouse, "[Enter Choose]");
    s.wait_for_overlay_close(PICKER);
    s.wait_for_owned_completed_action(&title(), "[Preview]", &[]);
}
fn picker_control(s: &mut Session, mouse: bool, label: &str) {
    if mouse {
        return super::control(s, true, PICKER, label, PICKER_CONTROLS);
    }
    // Query -> Rows changes the SAME owned title without any footer inverse.
    // Full titles are unique; the shorter Filter caption appears twice.
    let notices = [
        format!("{PICKER} · Filter focused"),
        format!("{PICKER} · Rows focused"),
    ];
    let notices = notices.each_ref().map(String::as_str);
    s.focus_completed_owned_action_v1(
        PICKER,
        label,
        FocusAttributeV1::Inverse,
        PICKER_CONTROLS,
        &notices,
    );
    s.send_key(b"\r");
}
fn control(s: &mut Session, mouse: bool, title: &str, label: &str, controls: &[&str]) {
    if !mouse && label == "[Esc Cancel]" && title.starts_with("Change PR Base • ") {
        // A busy Form accepts only its real Esc -> Close action; Tab is
        // intentionally ignored and cannot produce a footer-focus barrier.
        s.wait_for_owned_completed_action(title, label, &[]);
        s.send_key(b"\x1b");
        return;
    }
    if !mouse && title.starts_with("Change PR Base • ") {
        focus_base_control(s, title, label, controls);
        // Confirm still requires the independent useful/enabled underline
        // below; visible or focused text alone never authorizes the action.
        if !matches!(label, "[Confirm action]" | "[Acknowledge local]") {
            s.send_key(b"\r");
            return;
        }
    } else if !matches!(label, "[Confirm action]" | "[Acknowledge local]") {
        return super::control(s, mouse, title, label, controls);
    } else if !mouse {
        s.focus_completed_owned_action_v1(title, label, FocusAttributeV1::Inverse, controls, &[]);
    }
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let point = s.completed_cells_screen().and_then(|frame| {
            crate::support::completed_navigation_controls_v1::owned_action_position(
                frame,
                title,
                label,
                &[],
            )
            .filter(|(x, y)| {
                frame
                    .cell(*y, *x)
                    .is_some_and(|cell| cell.underline() && (mouse || cell.inverse()))
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
            "Exact enabled Base control absent: {title}/{label}\n{}",
            s.screen()
        );
    }
}
fn focused_base_field(frame: &vt100::Screen, title: &str) -> Option<usize> {
    use crate::support::completed_navigation_controls_v1::owned_action_position;
    crate::support::overlay_body(frame, title)?;
    let (_, footer) = owned_action_position(frame, title, "[Esc Close]", &[])
        .or_else(|| owned_action_position(frame, title, "[Keep draft / Close]", &[]))?;
    let mut focused = None;
    for (index, label) in [
        "Selected existing base branch (F2 Choose)",
        "Exact observed selected tip (read only)",
    ]
    .iter()
    .enumerate()
    {
        let Some((x, heading)) = owned_action_position(frame, title, label, &[]) else {
            continue;
        };
        let value = heading.checked_add(1)?;
        if value < footer
            && frame.cell(value, x).is_some_and(|cell| cell.underline())
            && focused.replace(index).is_some()
        {
            return None;
        }
    }
    focused
}
fn focus_base_control(s: &mut Session, title: &str, label: &str, controls: &[&str]) {
    use crate::support::completed_navigation_controls_v1::owned_focus_v1;
    let until = Instant::now() + Duration::from_secs(8);
    for _ in 0..24 {
        s.poll_output(Duration::from_millis(1));
        let frame = s
            .completed_cells_screen()
            .expect("Actual completed Base Form");
        let previous = owned_focus_v1(
            frame,
            title,
            label,
            FocusAttributeV1::Inverse,
            controls,
            &[],
        )
        .expect("Whole target control in the SAME topmost Base Form");
        if previous.target_focused {
            return;
        }
        let previous_field = focused_base_field(frame, title);
        let after = s.completed_cells_end();
        s.send_key(b"\t");
        loop {
            s.poll_output(Duration::from_millis(1));
            let changed = s.completed_cells_end() > after
                && s.completed_cells_screen().is_some_and(|frame| {
                    owned_focus_v1(
                        frame,
                        title,
                        label,
                        FocusAttributeV1::Inverse,
                        controls,
                        &[],
                    )
                    .is_some_and(|current| {
                        let field = focused_base_field(frame, title);
                        current.acknowledges_change_from(&previous)
                            || (current.target == previous.target
                                && field != previous_field
                                && (field.is_some() || previous_field.is_some()))
                    })
                });
            if changed {
                break;
            }
            assert!(
                Instant::now() < until,
                "ONE Tab requires NEW same-owned Base footer or actual underlined field change\n{}",
                s.screen()
            );
        }
    }
    panic!("Actual keyboard Base control not reached: {label}");
}
fn resize_base_form(s: &mut Session, cols: u16, rows: u16) {
    let title = title();
    let old_size = s.completed_cells_screen().map(|frame| frame.size());
    let after = s.completed_cells_end();
    s.resize(cols, rows);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        let complete = (old_size == Some((rows, cols)) || s.completed_cells_end() > after)
            && s.completed_cells_screen().is_some_and(|frame| {
                frame.size() == (rows, cols)
                    && crate::support::main_footer_visible(frame)
                    && crate::support::overlay_body(frame, &title).is_some_and(|body| {
                        body.lines().any(|row| {
                            let mut rest = row;
                            PREVIEW_CONTROLS.iter().all(|label| {
                                if let Some(offset) = rest.find(label) {
                                    rest = &rest[offset + label.len()..];
                                    true
                                } else {
                                    false
                                }
                            })
                        })
                    })
            });
        if complete {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW centered Base preview/all three whole ordered controls at {cols}x{rows}\n{}",
            s.screen()
        );
    }
}
fn preview(s: &mut Session, f: &Fixture, mouse: bool, prepared: usize) {
    let after = s.completed_cells_end();
    control(s, mouse, &title(), "[Preview]", FORM_CONTROLS);
    s.wait_for_command_count(f, prepared);
    form_frame(
        s,
        after,
        &title(),
        &["READ-ONLY PREVIEW", "[Confirm action]", "AFTER Base next"],
    );
    let after = s.completed_cells_end();
    s.paste("BASE_PREVIEW_POISON_NOT_A_MEMBER");
    form_frame(
        s,
        after,
        &title(),
        &["READ-ONLY PREVIEW", "[Confirm action]"],
    );
    assert!(!s.screen().contains("BASE_PREVIEW_POISON_NOT_A_MEMBER"));
    s.capture_completed_terminal_v1("base-exact-prepared-preview");
}
fn state(f: &Fixture, expected: &str, retained: bool) {
    let db = raw(f);
    let (purpose, status, head, receipt): (String, String, String, Option<String>) = db
        .query_row(
            "SELECT command,state,observed_head,confirmed_json FROM pr_write_attempts",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (purpose, status, head),
        ("metadata-base-v32".into(), expected.into(), "a".repeat(40))
    );
    assert_eq!(receipt.is_some(), expected == "confirmed");
    let json: String = db
        .query_row(
            "SELECT origin_json FROM pr_write_metadata_base_origins_v32",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let origin: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(origin["seed"]["base"], "b".repeat(40));
    assert_eq!(origin["seed"]["base_ref"], "main");
    assert_eq!(origin["seed"]["requested_ref"], "next");
    assert_eq!(origin["seed"]["requested_tip"], TIP);
    assert_eq!(origin["seed"]["repository_id"], 10000);
    assert_eq!(origin["seed"]["pr_id"], 1007);
    assert_eq!(origin["seed"]["author_uid"], 61);
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource LIKE 'metadata-base-v32/%'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        u64::from(retained)
    );
    if retained {
        let body: String = db
            .query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'metadata-base-v32/%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let saved: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(saved["values"], json!({"base":"next","tip":TIP}));
        assert!(saved["publication"].is_object());
        assert!(saved.get("approval").is_none());
    }
}
fn finish(
    mut s: Session,
    f: &Fixture,
    mouse: bool,
    steps: &[GhStep],
    git: &GitState,
    config: &[u8],
    stable: &[String],
) {
    assert_eq!(independent(f), stable);
    assert_eq!(&git_state(f), git);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    exact(&s, f, steps);
    assert_read_only_git(f);
    s.quit_and_assert_restored_terminal(mouse);
}
fn begin(
    steps: &[GhStep],
    personal: bool,
    mouse: bool,
    initial: usize,
) -> (Fixture, Session, GitState, Vec<u8>, Vec<String>) {
    let f = fixture(steps);
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
    (f, s, git, config, stable)
}
fn happy(mouse: bool, personal: bool) {
    let mut steps = initial_steps(personal);
    let initial = steps.len();
    steps.extend(catalogue(json!(true)));
    let opened = steps.len();
    steps.extend(selected());
    let prepared = steps.len();
    steps.extend(paid());
    let (f, mut s, git, config, stable) = begin(&steps, personal, mouse, initial);
    enter_base(&mut s, &f, mouse, opened);
    choose_member(&mut s, mouse, "next");
    preview(&mut s, &f, mouse, prepared);
    resize_base_form(&mut s, 120, 40);
    resize_base_form(&mut s, 80, 24);
    control(
        &mut s,
        mouse,
        &title(),
        "[Confirm action]",
        PREVIEW_CONTROLS,
    );
    s.wait_for_command_count(&f, steps.len());
    refreshed_v19::wait_fresh_page_v19(&mut s);
    state(&f, "confirmed", false);
    let db = raw(&f);
    let parent:Value=serde_json::from_str(&db.query_row("SELECT payload FROM records WHERE kind='pr-details' AND id='GENUINE_CANONICAL_PR_NODE_1007'",[],|r|r.get::<_,String>(0)).unwrap()).unwrap();
    assert_eq!(parent["summary"]["base_ref"], "next");
    assert_eq!(parent["summary"]["base_sha"], TIP);
    let mut st=db.prepare("SELECT kind,mutation_ordinal FROM sync_mutation_fences WHERE resource='GENUINE_CANONICAL_PR_NODE_1007' ORDER BY kind").unwrap();
    let rows = st
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let attempt = db
        .query_row("SELECT attempt_id FROM pr_write_attempts", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap();
    assert_eq!(
        rows,
        vec![
            ("inline_comment".into(), attempt),
            ("review".into(), attempt)
        ]
    );
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}
fn pure_picker(mouse: bool) {
    let mut steps = initial_steps(false);
    let initial = steps.len();
    steps.extend(catalogue(json!(true)));
    let opened = steps.len();
    steps.extend(selected());
    let prepared = steps.len();
    let (f, mut s, git, config, stable) = begin(&steps, false, mouse, initial);
    let before = raw(&f)
        .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| {
            r.get::<_, u64>(0)
        })
        .unwrap();
    enter_base(&mut s, &f, mouse, opened);
    choose_member(&mut s, mouse, "next");
    preview(&mut s, &f, mouse, prepared);
    resize_base_form(&mut s, 80, 24);
    resize_base_form(&mut s, 120, 40);
    control(
        &mut s,
        mouse,
        &title(),
        "[Keep draft / Close]",
        PREVIEW_CONTROLS,
    );
    s.wait_for_overlay_close(&title());
    assert_eq!(
        raw(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        before
    );
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}
fn refusals(mouse: bool) {
    for kind in ["tip", "same", "permission", "uid", "repository"] {
        let mut steps = initial_steps(false);
        let initial = steps.len();
        steps.extend(catalogue(json!(kind != "permission")));
        let opened = steps.len();
        if kind != "permission" {
            let mut phase = if kind == "same" {
                read(json!(true), None, Some("main"))
            } else {
                selected()
            };
            if kind == "tip" {
                phase[5].stdout = serde_json::to_vec(&branch("next", &"d".repeat(40))).unwrap();
                phase.truncate(6);
            } else if kind == "same" {
                phase.truncate(5);
            } else if kind == "uid" {
                phase.last_mut().unwrap().stdout =
                    serde_json::to_vec(&json!({"id":43,"login":"octocat"})).unwrap();
            } else {
                let mut actual = repository(0);
                actual["id"] = json!(10001);
                phase[2].stdout = serde_json::to_vec(&actual).unwrap();
                phase.truncate(3);
            }
            steps.extend(phase);
        }
        let (f, mut s, git, config, stable) = begin(&steps, false, mouse, initial);
        enter_base(&mut s, &f, mouse, opened);
        if kind == "permission" {
            s.wait_for_screen("disabled");
            let before = commands(&s, &f).len();
            picker_control(&mut s, mouse, "[Enter Choose]");
            s.wait_for_screen("disabled");
            assert_eq!(commands(&s, &f).len(), before);
            picker_control(&mut s, mouse, "[Esc Back]");
            s.wait_for_overlay_close(PICKER);
            control(&mut s, mouse, &title(), "[Esc Close]", FORM_CONTROLS);
        } else {
            choose_member(&mut s, mouse, if kind == "same" { "main" } else { "next" });
            let refusal_frame = s.completed_cursorless_end();
            control(&mut s, mouse, &title(), "[Preview]", FORM_CONTROLS);
            s.wait_for_command_count(&f, steps.len());
            if matches!(kind, "uid" | "repository") {
                s.wait_for_new_cursorless_condition(
                    refusal_frame,
                    |frame| {
                        crate::support::main_footer_visible(frame)
                            && !frame.contents().contains(DETAIL_BODY)
                            && !frame.contents().contains(&title())
                            && !frame.contents().contains(TIP)
                    },
                    "Actual original principal/repository loss hides Base/private underlay",
                    Duration::from_secs(8),
                );
            } else {
                s.wait_for_screen(if kind == "same" {
                    "unchanged"
                } else {
                    "unavailable"
                });
                assert!(!s.screen().contains("READ-ONLY PREVIEW"));
                control(&mut s, mouse, &title(), "[Esc Close]", FORM_CONTROLS);
            }
        }
        assert_eq!(
            raw(&f)
                .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
fn unknown_restart_ack(mouse: bool) {
    let mut steps = initial_steps(true);
    let initial = steps.len();
    steps.extend(catalogue(json!(true)));
    let opened = steps.len();
    steps.extend(selected());
    let prepared = steps.len();
    steps.extend(selected());
    let mut patch = receipts().remove(0);
    patch.stdout =
        serde_json::to_vec(&json!({"message":"ambiguous actual Base invocation"})).unwrap();
    steps.push(patch);
    let first = steps.len();
    steps.extend(initial_steps(true));
    let restart = steps.len();
    steps.extend(read(json!(true), None, None));
    let listed = steps.len();
    steps.extend(read(json!(true), None, None));
    let (f, mut s, git, config, stable) = begin(&steps, true, mouse, initial);
    enter_base(&mut s, &f, mouse, opened);
    choose_member(&mut s, mouse, "next");
    preview(&mut s, &f, mouse, prepared);
    control(
        &mut s,
        mouse,
        &title(),
        "[Confirm action]",
        PREVIEW_CONTROLS,
    );
    s.wait_for_command_count(&f, first);
    s.wait_for_screen("Unknown outcome");
    state(&f, "uncertain", true);
    let original_draft: String = raw(&f)
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'metadata-base-v32/%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    s.quit_and_assert_restored_terminal(mouse);
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, restart, false, &steps[..restart]);
    let after = s.completed_cells_end();
    crate::pr_actions::menu(&mut s, mouse, "Inspect Base history");
    s.wait_for_command_count(&f, listed);
    form_frame(&mut s, after, &history_title(), HISTORY_CONTROLS);
    assert!(s.screen().contains("inspect only"));
    let after = s.completed_cells_end();
    control(
        &mut s,
        mouse,
        &history_title(),
        "[Inspect current]",
        HISTORY_CONTROLS,
    );
    s.wait_for_command_count(&f, steps.len());
    form_frame(&mut s, after, &history_title(), HISTORY_CONTROLS);
    control(
        &mut s,
        mouse,
        &history_title(),
        "[Acknowledge local]",
        HISTORY_CONTROLS,
    );
    s.wait_for_screen("Local ACK");
    state(&f, "acknowledged", true);
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT body FROM drafts WHERE resource LIKE 'metadata-base-v32/%'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        original_draft
    );
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}
fn cleanup_refresh(mouse: bool) {
    let mut steps = initial_steps(true);
    let initial = steps.len();
    steps.extend(catalogue(json!(true)));
    let opened = steps.len();
    steps.extend(selected());
    let prepared = steps.len();
    steps.extend(paid());
    let confirmed = steps.len();
    steps.extend(refresh());
    let (f, mut s, git, config, stable) = begin(&steps, true, mouse, initial);
    let db = rusqlite::Connection::open_with_flags(
        database(&f),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap();
    db.execute_batch("CREATE TRIGGER actual_base_cleanup_fails_v32 BEFORE DELETE ON drafts WHEN OLD.resource LIKE 'metadata-base-v32/%' BEGIN SELECT RAISE(ABORT,'actual cleanup failure');END;").unwrap();
    enter_base(&mut s, &f, mouse, opened);
    choose_member(&mut s, mouse, "next");
    preview(&mut s, &f, mouse, prepared);
    control(
        &mut s,
        mouse,
        &title(),
        "[Confirm action]",
        PREVIEW_CONTROLS,
    );
    s.wait_for_command_count(&f, confirmed);
    s.wait_for_screen("Confirmed; visibility unavailable");
    state(&f, "confirmed", true);
    let marker: String = db
        .query_row(
            "SELECT json_array(attempt_id,state,confirmed_json) FROM pr_write_attempts",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let saved: String = db
        .query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'metadata-base-v32/%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let terminal = "Confirmed Conversation • read-only refresh";
    s.wait_for_owned_completed_action(terminal, "[r Refresh PR]", &[]);
    let before_refresh = s.completed_cursorless_end();
    control(
        &mut s,
        mouse,
        terminal,
        "[r Refresh PR]",
        &["[r Refresh PR]", "[Esc Back to Inbox]"],
    );
    s.wait_for_command_count(&f, steps.len());
    s.wait_for_new_cursorless_condition(
        before_refresh,
        |frame| {
            let text = frame.contents();
            crate::support::main_footer_visible(frame)
                && text.contains("Fresh original PR/search observation")
                && text.contains("read only • nothing resent")
                && text.contains(&format!("{FIRST}#7 • {DETAIL_TITLE}"))
                && text.contains(&format!("https://{HOST}/{FIRST}/pull/7"))
                && text.contains(&format!("Head {}", "a".repeat(40)))
                && text.contains(&format!("Base {TIP}"))
                && text.contains("feature/scoped-open → next")
                && text.contains(DETAIL_BODY)
                && text.contains("[b Back]")
                && !text.contains(terminal)
                && !text.contains("[r Refresh PR]")
                && !text.contains("[Confirm action]")
                && !text.contains("READ-ONLY PREVIEW")
        },
        "NEW exact original H/canonical PR and B1 next/full tip after actual Refresh13",
        Duration::from_secs(8),
    );
    let projection: Value = serde_json::from_str(
        &db.query_row(
            "SELECT payload FROM records WHERE kind='pr-details' AND id='GENUINE_CANONICAL_PR_NODE_1007'",
            [],
            |r| r.get::<_, String>(0),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(projection["summary"]["head_sha"], "a".repeat(40));
    assert_eq!(projection["summary"]["base_ref"], "next");
    assert_eq!(projection["summary"]["base_sha"], TIP);
    assert_eq!(
        db.query_row(
            "SELECT json_array(attempt_id,state,confirmed_json) FROM pr_write_attempts",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        marker
    );
    assert_eq!(
        db.query_row(
            "SELECT body FROM drafts WHERE resource LIKE 'metadata-base-v32/%'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        saved
    );
    state(&f, "confirmed", true);
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}
fn held_cancel(mouse: bool) {
    let mut steps = initial_steps(false);
    let initial = steps.len();
    steps.extend(catalogue(json!(true)));
    let opened = steps.len();
    let mut reading = selected();
    reading.truncate(6);
    reading.last_mut().unwrap().delay_ms = 30_000;
    let held = steps.len() + 5;
    steps.extend(reading);
    let (f, mut s, git, config, stable) = begin(&steps, false, mouse, initial);
    enter_base(&mut s, &f, mouse, opened);
    choose_member(&mut s, mouse, "next");
    control(&mut s, mouse, &title(), "[Preview]", FORM_CONTROLS);
    s.wait_for_command_count(&f, steps.len());
    let pid = i32::try_from(commands(&s, &f)[held]["pid"].as_u64().unwrap()).unwrap();
    let identity = process_identity(pid).unwrap().unwrap();
    let owner =
        OwnedPtySession::capture(u32::try_from(s.owned_application_pid()).unwrap()).unwrap();
    assert!(owner.owns(&identity));
    assert!(!identity.zombie);
    control(&mut s, mouse, &title(), "[Esc Cancel]", &["[Esc Cancel]"]);
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        match process_identity(pid) {
            Ok(None) => break,
            Ok(Some(current))
                if (current.pid, current.uid, current.birth, current.session)
                    != (identity.pid, identity.uid, identity.birth, identity.session) =>
            {
                break
            }
            Ok(Some(_)) => {}
            Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {}
            Err(e) => panic!("Exact held provider identity unavailable: {e}"),
        };
        assert!(
            Instant::now() < until,
            "Exact original held Base provider still exists"
        );
        s.poll_output(Duration::from_millis(1));
    }
    assert_eq!(
        raw(&f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert!(!s.screen().contains("BASE PREVIEW •"));
    finish(s, &f, mouse, &steps, &git, &config, &stable);
}
fn wrong_receipt(mouse: bool) {
    for field in ["ref", "head", "repository"] {
        let mut steps = initial_steps(false);
        let initial = steps.len();
        steps.extend(catalogue(json!(true)));
        let opened = steps.len();
        steps.extend(selected());
        let prepared = steps.len();
        steps.extend(selected());
        let mut receipt = receipts();
        let mut wrong = raw_parent(true);
        match field {
            "ref" => wrong["base"]["ref"] = json!("unselected"),
            "head" => wrong["head"]["sha"] = json!("d".repeat(40)),
            _ => wrong["base"]["repo"]["id"] = json!(10001),
        };
        receipt[1].stdout = serde_json::to_vec(&wrong).unwrap();
        steps.extend(receipt);
        let (f, mut s, git, config, stable) = begin(&steps, false, mouse, initial);
        enter_base(&mut s, &f, mouse, opened);
        choose_member(&mut s, mouse, "next");
        preview(&mut s, &f, mouse, prepared);
        control(
            &mut s,
            mouse,
            &title(),
            "[Confirm action]",
            PREVIEW_CONTROLS,
        );
        s.wait_for_command_count(&f, steps.len());
        s.wait_for_screen("Unknown outcome");
        state(&f, "uncertain", true);
        assert_eq!(
            commands(&s, &f)
                .iter()
                .filter(|r| r["args"].as_array().unwrap().iter().any(|a| a == "PATCH"))
                .count(),
            1
        );
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
#[test]
fn pr_metadata_base_v32_keyboard_actual_picker_preview_resize_close_zero_post() {
    pure_picker(false)
}
#[test]
fn pr_metadata_base_v32_mouse_actual_picker_preview_resize_close_zero_post() {
    pure_picker(true)
}
#[test]
fn pr_metadata_base_v32_keyboard_catalogue_exact_member_single_patch_get_new_base_pair() {
    happy(false, false)
}
#[test]
fn pr_metadata_base_v32_mouse_catalogue_exact_member_single_patch_get_new_base_pair() {
    happy(true, false)
}
#[test]
fn pr_metadata_base_v32_keyboard_personal_original_same_core_single_patch_get_new_base_pair() {
    happy(false, true)
}
#[test]
fn pr_metadata_base_v32_mouse_personal_original_same_core_single_patch_get_new_base_pair() {
    happy(true, true)
}
#[test]
fn pr_metadata_base_v32_keyboard_actual_tip_drift_nochange_permission_refuse_zero_post() {
    refusals(false)
}
#[test]
fn pr_metadata_base_v32_mouse_actual_tip_drift_nochange_permission_refuse_zero_post() {
    refusals(true)
}
#[test]
fn pr_metadata_base_v32_keyboard_personal_unknown_restart_current_inspect_explicit_local_ack_no_resend(
) {
    unknown_restart_ack(false)
}
#[test]
fn pr_metadata_base_v32_mouse_personal_unknown_restart_current_inspect_explicit_local_ack_no_resend(
) {
    unknown_restart_ack(true)
}
#[test]
fn pr_metadata_base_v32_keyboard_actual_confirmed_cleanup_sql_failure_explicit_new_base_refresh() {
    cleanup_refresh(false)
}
#[test]
fn pr_metadata_base_v32_mouse_actual_confirmed_cleanup_sql_failure_explicit_new_base_refresh() {
    cleanup_refresh(true)
}
#[test]
fn pr_metadata_base_v32_keyboard_actual_held_branch_preview_cancel_exact_child_cleanup_zero_post() {
    held_cancel(false)
}
#[test]
fn pr_metadata_base_v32_mouse_actual_held_branch_preview_cancel_exact_child_cleanup_zero_post() {
    held_cancel(true)
}
#[test]
fn pr_metadata_base_v32_keyboard_independent_get_ref_head_repository_mismatch_stays_uncertain_no_resend(
) {
    wrong_receipt(false)
}
#[test]
fn pr_metadata_base_v32_mouse_independent_get_ref_head_repository_mismatch_stays_uncertain_no_resend(
) {
    wrong_receipt(true)
}
