//! UNRUN actual binary declarations: genuine Notification→PR Files→the SAME
//! immutable/native engine→pure Back, plus exact fail-closed target/cancel cases.
use super::*;
use crate::immutable_git_v1 as history;
const ID: u64 = 9007199254740993;
const PR_TITLE: &str = "ACTUAL_NOTIFICATION_FOLLOW_PR_TITLE";
fn parent(h: &history::GitImagesV1) -> Value {
    let mut raw = json!({"id":9007199254740995u64,"node_id":"ACTUAL_NOTIFICATION_FOLLOW_PR_NODE","number":7,"title":PR_TITLE,"body":"Not loaded by Files follow",
        "url":format!("https://api.github.com/repos/{REPO}/pulls/7"),"html_url":format!("https://github.com/{REPO}/pull/7"),
        "user":{"id":51,"login":"genuine-pr-author","type":"User"},"state":"open","draft":false,"merged":false,"changed_files":1,
        "head":{"sha":h.h,"ref":"feature","repo":{"id":991,"node_id":"ACTUAL_HEAD_REPO_NODE","full_name":"fork/source"}},
        "base":{"sha":h.b,"ref":"main","repo":{"id":91000,"node_id":"ACTUAL_NOTIFICATION_REPOSITORY_NODE","full_name":REPO}}});
    h.bind_parent(&mut raw);
    raw
}
fn follow_phase(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.push(get(&format!("notifications/threads/{ID}"), row(ID)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent(h)));
    steps.push(get(
        &format!("repos/{REPO}/pulls/7/files?per_page=100&page=1"),
        json!([h.file()]),
    ));
    let mut patch = GhStep::read(
        &[
            "pr",
            "diff",
            "7",
            "--repo",
            &format!("{HOST}/{REPO}"),
            "--color",
            "never",
        ],
        "",
    );
    patch.stdout = h.patch.clone();
    steps.push(patch);
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent(h)));
    steps.push(get(&format!("notifications/threads/{ID}"), row(ID)));
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 12);
    steps
}
fn blob(sha: &str, bytes: &[u8]) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/git/blobs/{sha}"),
            "--hostname",
            HOST,
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.raw+json",
            "--allow-escape-sequences",
        ],
        "",
    );
    step.stdout = bytes.to_vec();
    step
}
fn images(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = principal(42);
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent(h)));
    steps.push(get(
        &format!("repos/{REPO}/compare/{}...{}?per_page=1&page=1", h.b, h.h),
        h.compare(),
    ));
    steps.push(get(
        &format!("repos/{REPO}/git/commits/{}", h.h),
        h.commit(true),
    ));
    steps.push(get(
        &format!("repos/{REPO}/git/commits/{}", h.m),
        h.commit(false),
    ));
    for (sha, value) in &h.old_trees {
        steps.push(get(&format!("repos/{REPO}/git/trees/{sha}"), value.clone()));
    }
    steps.push(blob(&h.old_blob, &h.old_bytes));
    for (sha, value) in &h.new_trees {
        steps.push(get(&format!("repos/{REPO}/git/trees/{sha}"), value.clone()));
    }
    steps.push(blob(&h.new_blob, &h.new_bytes));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent(h)));
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 17);
    steps
}
fn fixture_follow(
    alter: impl FnOnce(&history::GitImagesV1, &mut Vec<GhStep>),
) -> (Fixture, history::GitImagesV1, Vec<GhStep>) {
    let f = Fixture::new();
    let h = history::GitImagesV1::create(&f.repo(), false);
    f.commit_file(
        "dirty界.txt",
        b"ORIGINAL_TRACKED_DIRTY\n",
        "actual tracked working file",
    );
    fs::write(f.repo().join("dirty界.txt"), b"UNCHANGED_WORKTREE\n").unwrap();
    fs::write(f.repo().join("staged.txt"), b"UNCHANGED_INDEX\n").unwrap();
    f.git(&["add", "--", "staged.txt"]);
    fs::write(f.repo().join("scratch.txt"), b"UNCHANGED_UNTRACKED\n").unwrap();
    let cache = ghtui::storage::Cache::open(&database(&f)).unwrap();
    cache
        .put_draft(
            &ghtui::storage::Scope::github(HOST, Some(99)).unwrap(),
            "foreign",
            "EXACT_FOREIGN_DRAFT",
        )
        .unwrap();
    drop(cache);
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(&f)).unwrap());
    let mut steps = empty_inbox_steps("octocat");
    steps.extend(phase(false, false, false, false));
    let mut follow = follow_phase(&h);
    alter(&h, &mut follow);
    steps.extend(follow);
    f.scripted_gh(&steps);
    (f, h, steps)
}
fn action(session: &mut Session, mouse: bool, title: &str, label: &str, key: &[u8]) {
    // The PR page's controls are key-bar tokens; other owners keep their bracketed rows.
    let point = if title == PR_TITLE {
        session.wait_for_control(label)
    } else {
        session.wait_for_owned_completed_action(title, label, &[])
    };
    if mouse {
        session.click(point.0, point.1);
    } else {
        session.send_key(key);
    }
}
fn files(session: &mut Session, after: usize, patch_observed: bool) -> String {
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            let text = frame.contents();
            text.contains(PR_TITLE)
                && text.contains(history::PATH)
                && crate::support::controls::control_painted(frame, "[v Native diff]")
                && text.contains(if patch_observed {
                    "Showing the PR patch from gh pr diff"
                } else {
                    "Patch not loaded; file list only"
                })
                && !text.contains(TITLE)
        },
        "Actual completed notification-bound PR Files, not a partial/old body",
        Duration::from_secs(8),
    );
    session.completed_cursorless_screen().unwrap().contents()
}
fn open_follow(session: &mut Session, mouse: bool) -> usize {
    let after = session.completed_cells_end();
    let controls = [
        "[b Back / Cancel]",
        "[q Quit]",
        "[h History]",
        "[Enter Details]",
        "[r Refresh]",
        "[a Include read]",
        "[p Participating]",
        "[f Follow PR]",
    ];
    choose_with_controls(session, mouse, "[f Follow PR]", b"f", &controls);
    after
}
fn happy(mouse: bool) {
    let (f, h, steps) = fixture_follow(|h, steps| steps.extend(images(h)));
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    let browser = ready(&mut session, after, PRIVATE);
    let after = open_follow(&mut session, mouse);
    files(&mut session, after, true);
    session.wait_for_command_count(&f, 27);
    assert_eq!(witness(&f), before);
    assert_sql_generation(&f, &[1]);
    session.capture_completed_terminal_v1(if mouse {
        "notifications-pr-files-mouse"
    } else {
        "notifications-pr-files-keyboard"
    });
    // Same loaded action semantics choose the genuine retained file row before
    // a new immutable operation; no label→path or line1/range is invented.
    let point = session.wait_for_control("[N Next file]");
    let after = session.completed_cells_end();
    if mouse {
        session.click(point.0, point.1);
    } else {
        session.send_key(b"N");
    }
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            frame
                .contents()
                .lines()
                .any(|line| line.contains("▌ File ") && line.contains(history::PATH))
        },
        "Actual selected typed Files member after one loaded action",
        Duration::from_secs(8),
    );
    action(&mut session, mouse, PR_TITLE, "[v Native diff]", b"v");
    session.wait_for_command_count(&f, 44);
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, "Native PR diff •").is_some_and(|body| {
                body.contains("new-no-newline")
                    && body.contains("[o Options]")
                    && !body.contains("NOT CURRENT")
            })
        },
        "Actual canonical immutable notification PR native viewer",
        Duration::from_secs(8),
    );
    let native = session.wait_for_overlay_body("Native PR diff •");
    assert_ne!(h.b, h.m);
    assert_ne!(h.m, h.h);
    session.capture_completed_terminal_v1(if mouse {
        "notifications-pr-native-mouse"
    } else {
        "notifications-pr-native-keyboard"
    });
    action(
        &mut session,
        mouse,
        "Native PR diff •",
        "[o Options] [q Quit]",
        b"o",
    );
    session.wait_for_overlay_body("Native PR diff •");
    let options_controls = ["[o Back to diff]", "[E External]"];
    for (columns, rows) in [(120, 40), (80, 24)] {
        let after_resize = session.completed_cursorless_end();
        session.resize_owned_header(columns, rows, "Native PR diff •", &options_controls);
        session.wait_for_new_cursorless_condition(
            after_resize,
            |frame| {
                frame.size() == (rows, columns)
                    && crate::support::owned_resize_controls_ready_v1(
                        frame,
                        "Native PR diff •",
                        &options_controls,
                        crate::support::ResizeToolbarV1::Header,
                    )
            },
            "Actual NEW complete Native Options geometry with both current original controls",
            Duration::from_secs(8),
        );
    }
    action(
        &mut session,
        mouse,
        "Native PR diff •",
        "[o Back to diff]",
        b"o",
    );
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, "Native PR diff •")
                .is_some_and(|body| body == native)
        },
        "Native options Back keeps exact original source/viewport/range",
        Duration::from_secs(8),
    );
    action(
        &mut session,
        mouse,
        "Native PR diff •",
        "[Esc Close] [o Options] [q Quit]",
        b"\x1b",
    );
    session.wait_for_overlay_body("Immutable file images");
    action(
        &mut session,
        mouse,
        "Immutable file images",
        "[Esc Close]",
        b"\x1b",
    );
    session.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            text.contains(PR_TITLE)
                && crate::support::controls::control_painted(frame, "[b Back]")
                && crate::support::overlay_body(frame, "Immutable file images").is_none()
        },
        "Original genuine Files after closing nested viewer",
        Duration::from_secs(8),
    );
    let after = session.completed_cells_end();
    action(&mut session, mouse, PR_TITLE, "[b Back]", b"b");
    let restored = ready(&mut session, after, PRIVATE);
    assert_eq!(
        restored, browser,
        "Pure return restores exact query/selection/scroll; no re-read"
    );
    exact(&session, &f, &steps);
    assert_eq!(witness(&f), before);
    assert_sql_generation(&f, &[1]);
    quit(&mut session, mouse);
}
fn changed_parent(mouse: bool) {
    let (f, _h, steps) = fixture_follow(|_h, steps| {
        assert_eq!(
            steps[7].args,
            get(&format!("repos/{REPO}/pulls/7"), Value::Null).args
        );
        let mut value: Value = serde_json::from_slice(&steps[7].stdout).unwrap();
        value["head"]["sha"] = json!("f".repeat(40));
        steps[7] = get(&format!("repos/{REPO}/pulls/7"), value);
        steps.truncate(8);
    });
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    ready(&mut session, after, PRIVATE);
    let after = open_follow(&mut session, mouse);
    ready(
        &mut session,
        after,
        "Original notification PR/source could not be verified",
    );
    let raw = session.completed_cursorless_screen().unwrap().contents();
    assert!(!raw.contains(PR_TITLE));
    assert!(!crate::support::controls::control_painted(
        session.completed_cursorless_screen().unwrap(),
        "[v Native diff]"
    ));
    exact(&session, &f, &steps);
    assert_eq!(witness(&f), before);
    assert_sql_generation(&f, &[1]);
    quit(&mut session, mouse);
}
fn lost_source(mouse: bool) {
    for recreated in [false, true] {
        let (f, _h, steps) = fixture_follow(|_h, steps| {
            if recreated {
                let mut repo = repository();
                repo["id"] = json!(91001);
                steps[2] = get(&format!("repos/{REPO}"), repo);
                steps.truncate(3);
            } else {
                steps[1] = get("user", json!({"id":43,"login":"new-account"}));
                steps.truncate(2);
            }
        });
        let mut session = start(&f);
        let before = witness(&f);
        let after = open(&mut session, mouse);
        ready(&mut session, after, PRIVATE);
        let after = open_follow(&mut session, mouse);
        session.wait_for_new_cursorless_condition(after,|frame|{let text=frame.contents();!text.contains(PRIVATE)&&!text.contains(PR_TITLE)&&text.contains("Private presentation retired")},"Actual changed numeric account/recreated repository erases old body BEFORE failure effects",Duration::from_secs(8));
        exact(&session, &f, &steps);
        assert_eq!(witness(&f), before);
        session.quit_and_assert_restored_terminal(mouse);
    }
}
fn cancel(mouse: bool) {
    let (f, h, mut steps) = fixture_follow(|_h, steps| {
        steps[3].delay_ms = 6000;
        steps.truncate(4);
    });
    steps.extend(phase(false, false, false, false));
    steps.extend(follow_phase(&h));
    f.scripted_gh(&steps);
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    ready(&mut session, after, PRIVATE);
    let after = open_follow(&mut session, mouse);
    session.wait_for_command_count(&f, 19);
    let rows = session
        .inspect_command_journal(&f)
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let pid = i32::try_from(rows[18]["pid"].as_u64().unwrap()).unwrap();
    let child = process_identity(pid)
        .unwrap()
        .expect("Original six-second real thread GET live BEFORE cancellation");
    assert!(!child.zombie);
    let application_pid = u32::try_from(session.owned_application_pid())
        .expect("owned application PID must be positive and fit u32");
    assert!(OwnedPtySession::capture(application_pid)
        .unwrap()
        .owns(&child));
    session.wait_for_new_cursorless_condition(
        after,
        |frame| {
            crate::support::overlay_body(frame, TITLE)
                .is_some_and(|body| body.contains("Verifying current account"))
        },
        "Genuine pending follow with owned Cancel",
        Duration::from_secs(8),
    );
    main_back(&mut session, mouse);
    child_gone(&mut session, &child);
    assert_eq!(witness(&f), before);
    let after = open(&mut session, mouse);
    ready(&mut session, after, PRIVATE);
    let after = open_follow(&mut session, mouse);
    files(&mut session, after, true);
    let after = session.completed_cells_end();
    action(&mut session, mouse, PR_TITLE, "[b Back]", b"b");
    ready(&mut session, after, PRIVATE);
    assert_eq!(witness(&f), before);
    assert_sql_generation(&f, &[2]);
    exact(&session, &f, &steps);
    quit(&mut session, mouse);
}
fn unsupported(mouse: bool) {
    let (f, _h, mut steps) = fixture_follow(|_, steps| steps.clear());
    let mut page: Value = {
        let stdout = std::str::from_utf8(&steps[9].stdout).unwrap();
        serde_json::from_str(stdout.split_once("\r\n\r\n").unwrap().1).unwrap()
    };
    page[0]["subject"]["url"] = Value::Null;
    steps[9] = super::page(false, false, 1, page, false);
    // An unresolved subject names no issue or PR, so the list makes NO
    // batched actor read and the row shows an unknown actor.
    assert!(graphql_read(&steps[11]));
    steps.remove(11);
    f.scripted_gh(&steps);
    let mut session = start(&f);
    let before = witness(&f);
    let after = open(&mut session, mouse);
    ready(
        &mut session,
        after,
        "Review requested • Subject unavailable • ",
    );
    let snapshot = session.wait_for_overlay_body(TITLE);
    let point = session.wait_for_owned_completed_action(TITLE, "[f Follow PR]", &[]);
    let after = session.completed_cells_end();
    if mouse {
        session.click(point.0, point.1);
    } else {
        session.send_key(b"f");
    }
    session.wait_for_new_cursorless_condition(
        after,
        |frame| crate::support::overlay_body(frame, TITLE).is_some_and(|body| body == snapshot),
        "Actually disabled canonical PR follow cannot queue from nullable target",
        Duration::from_secs(8),
    );
    exact(&session, &f, &steps);
    assert_eq!(witness(&f), before);
    assert_sql_generation(&f, &[1]);
    quit(&mut session, mouse);
}
fn unavailable_patch(mouse: bool) {
    for capped in [false, true] {
        let (f, _h, steps) = fixture_follow(|_, phase| {
            if capped {
                phase[6].stdout = vec![b'x'; 256 * 1024 + 1];
            } else {
                phase[6].status = 1;
                phase[6].stderr = b"gh: actual bounded patch unavailable\n".to_vec();
            }
        });
        let mut session = start(&f);
        let before = witness(&f);
        let after = open(&mut session, mouse);
        let original = ready(&mut session, after, PRIVATE);
        let after = open_follow(&mut session, mouse);
        files(&mut session, after, false);
        session.wait_for_command_count(&f, 27);
        let after = session.completed_cells_end();
        action(&mut session, mouse, PR_TITLE, "[N Next file]", b"N");
        session.wait_for_new_cursorless_condition(
            after,
            |frame| {
                frame
                    .contents()
                    .lines()
                    .any(|row| row.contains("▌ File ") && row.contains(history::PATH))
            },
            "A genuine retained file member remains browsable without claiming a patch",
            Duration::from_secs(8),
        );
        let after = session.completed_cells_end();
        action(&mut session, mouse, PR_TITLE, "[v Native diff]", b"v");
        session.wait_for_new_cursorless_condition(
            after,
            |frame| {
                let text = frame.contents();
                text.contains("patch is unavailable")
                    && text.contains(PR_TITLE)
                    && crate::support::overlay_body(frame, "Native PR diff •").is_none()
                    && crate::support::overlay_body(frame, "Immutable file images").is_none()
            },
            "Missing/capped actual patch refuses Native BEFORE queue/child or proof",
            Duration::from_secs(8),
        );
        exact(&session, &f, &steps);
        assert_eq!(witness(&f), before);
        assert_sql_generation(&f, &[1]);
        let after = session.completed_cells_end();
        action(&mut session, mouse, PR_TITLE, "[b Back]", b"b");
        assert_eq!(ready(&mut session, after, PRIVATE), original);
        exact(&session, &f, &steps);
        assert_eq!(witness(&f), before);
        quit(&mut session, mouse);
    }
}
#[test]
fn notifications_v63_keyboard_patch_refusal_and_cap_keep_files_but_never_queue_native() {
    unavailable_patch(false);
}
#[test]
fn notifications_v63_mouse_patch_refusal_and_cap_keep_files_but_never_queue_native() {
    unavailable_patch(true);
}
#[test]
fn notifications_v63_keyboard_genuine_pr_follow_shared_native_and_pure_back() {
    happy(false);
}
#[test]
fn notifications_v63_mouse_genuine_pr_follow_shared_native_and_pure_back() {
    happy(true);
}
#[test]
fn notifications_v63_keyboard_changed_full_parent_h_refuses_no_private_pr() {
    changed_parent(false);
}
#[test]
fn notifications_v63_mouse_changed_full_parent_h_refuses_no_private_pr() {
    changed_parent(true);
}
#[test]
fn notifications_v63_keyboard_actual_numeric_uid_recreated_repository_hide_before_private_body() {
    lost_source(false);
}
#[test]
fn notifications_v63_mouse_actual_numeric_uid_recreated_repository_hide_before_private_body() {
    lost_source(true);
}
#[test]
fn notifications_v63_keyboard_actual_held_thread_cancel_then_fresh_source_no_late_pr() {
    cancel(false);
}
#[test]
fn notifications_v63_mouse_actual_held_thread_cancel_then_fresh_source_no_late_pr() {
    cancel(true);
}
#[test]
fn notifications_v63_keyboard_nullable_target_disabled_no_source_or_child() {
    unsupported(false);
}
#[test]
fn notifications_v63_mouse_nullable_target_disabled_no_source_or_child() {
    unsupported(true);
}
