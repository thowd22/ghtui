//! DECLARED/UNRUN. Actual Native/Timeline/Files -> genuine selected comment ->
//! historical content, never a manufactured current or original placement.
use super::*;
use immutable_images::history as git_history;
#[path = "../support/thread_historical_git_v5.rs"]
mod actual_git;
const OWNER: &str = "Historical comment source • placement UNKNOWN";
const NATIVE: &str = "Native PR diff •";
fn bind_historical(value: &mut Value, h: &git_history::GitImagesV1) {
    match value {
        Value::String(raw) if raw == &"c".repeat(40) => *raw = h.h.clone(),
        Value::String(raw) if raw == &"d".repeat(40) => *raw = h.m.clone(),
        Value::String(raw) if raw == "src/old.rs" => *raw = git_history::PATH.into(),
        Value::Array(rows) => {
            for row in rows {
                bind_historical(row, h);
            }
        }
        Value::Object(map) => {
            for row in map.values_mut() {
                bind_historical(row, h);
            }
        }
        _ => {}
    }
}
fn bind(steps: &mut [GhStep], h: &git_history::GitImagesV1) {
    for step in steps {
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            bind_historical(&mut value, h);
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
    }
}
fn bind_native(value: &mut Value, h: &git_history::GitImagesV1) {
    match value {
        Value::String(raw) if raw == &"a".repeat(40) => *raw = h.h.clone(),
        Value::String(raw) if raw == &"b".repeat(40) => *raw = h.b.clone(),
        Value::String(raw) if raw == "src/view.rs" => *raw = git_history::PATH.into(),
        Value::Array(rows) => {
            for row in rows {
                bind_native(row, h);
            }
        }
        Value::Object(map) => {
            for row in map.values_mut() {
                bind_native(row, h);
            }
        }
        _ => {}
    }
}
fn object_steps(h: &git_history::GitImagesV1, original: bool, blob: bool) -> Vec<GhStep> {
    let (commit, root, trees, sha, bytes) = if original {
        (&h.m, &h.tm, &h.old_trees, &h.old_blob, &h.old_bytes)
    } else {
        (&h.h, &h.th, &h.new_trees, &h.new_blob, &h.new_bytes)
    };
    let mut steps = vec![get(
        &format!("repos/{FIRST}/git/commits/{commit}"),
        json!({"sha":commit,"tree":{"sha":root}}),
    )];
    for (sha, value) in trees {
        steps.push(get(
            &format!("repos/{FIRST}/git/trees/{sha}"),
            value.clone(),
        ));
    }
    if blob {
        let mut step = GhStep::read(
            &[
                "api",
                &format!("repos/{FIRST}/git/blobs/{sha}"),
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
        step.stdout = bytes.clone();
        steps.push(step);
    }
    steps
}
fn historical_steps(h: &git_history::GitImagesV1, mode: u8) -> Vec<GhStep> {
    if mode == 5 {
        return principal(43);
    }
    let mut steps = opening(Operation::Reply);
    steps.extend(comment_observe_v2());
    assert_eq!(steps.len(), 12);
    steps.extend(object_steps(h, mode == 1, !matches!(mode, 1 | 7 | 9)));
    if mode == 4 {
        steps[15].delay_ms = 6000;
        steps.truncate(16);
    } else if mode == 6 {
        steps[12] = get(
            &format!("repos/{FIRST}/git/commits/{}", h.h),
            json!({"sha":"f".repeat(40),"tree":{"sha":h.th}}),
        );
        steps.truncate(13);
    } else {
        steps.extend(closing());
    }
    steps
}
fn owned_historical(session: &mut Session, words: &[&str]) {
    session.wait_for_cursorless_condition(
        |frame| {
            crate::support::overlay_body(frame, OWNER).is_some_and(|body| {
                body.contains("[Esc Back]")
                    && body.contains("[c Associated C]")
                    && body.contains("[o Original C0]")
                    && words.iter().all(|word| body.contains(word))
            })
        },
        "Actual completed owned historical role/content frame",
        Duration::from_secs(8),
    );
}
fn reveal(session: &mut Session, mouse: bool, wanted: &str) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        let frame = session
            .completed_cursorless_screen()
            .expect("Completed original historical frame");
        let before = crate::support::overlay_body(frame, OWNER).expect("Topmost historical owner");
        if before.contains(wanted) {
            return;
        }
        let endpoint = session.completed_cursorless_end();
        control_v2(session, mouse, OWNER, "[PgDn Next]", b"\x1b[6~");
        session.wait_for_new_cursorless_condition(
            endpoint,
            |frame| {
                crate::support::overlay_body(frame, OWNER)
                    .is_some_and(|body| body != before && body.contains("[Esc Back]"))
            },
            "One actual acknowledged historical viewport change before another input",
            deadline.saturating_duration_since(Instant::now()),
        );
        assert!(Instant::now() < deadline);
    }
}
fn journey(mouse: bool, mode: u8) {
    let content = (0..60)
        .map(|line| format!("Historical line{line:03} 界 original bytes\n"))
        .collect::<String>();
    let dir = tempfile::tempdir().unwrap();
    let mut h = if mode == 1 {
        // A genuine rename needs actual shared content. A two-line unrelated
        // old blob is correctly emitted by Git as delete+add, not a rename.
        let old = content.replace("line002", "prior002");
        git_history::GitImagesV1::create_renamed_with_bytes(
            dir.path(),
            false,
            old.as_bytes(),
            content.as_bytes(),
        )
    } else if mode == 2 {
        git_history::GitImagesV1::create_with_bytes(
            dir.path(),
            false,
            git_history::OLD,
            b"actual binary\0verified bytes\n",
        )
    } else if mode == 3 {
        git_history::GitImagesV1::create_with_bytes(
            dir.path(),
            false,
            git_history::OLD,
            b"version https://git-lfs.github.com/spec/v1\noid sha256:abcdef\nsize 999999\n",
        )
    } else if mode == 7 {
        git_history::GitImagesV1::create_with_bytes(
            dir.path(),
            false,
            git_history::OLD,
            &vec![b'x'; 512 * 1024 + 1],
        )
    } else {
        git_history::GitImagesV1::create_with_bytes(
            dir.path(),
            false,
            git_history::OLD,
            content.as_bytes(),
        )
    };
    if matches!(mode, 8 | 9) {
        actual_git::special(dir.path(), &mut h, mode == 8);
    }
    let timeline = matches!(mode, 1 | 3 | 7);
    let native = mode == 0;
    let (fixture, native_history, mut steps) = if native {
        let (f, h, s) = immutable_images::fixture_and_steps(|_| {});
        (f, Some(h), s)
    } else {
        let f = resource_configured(&[]);
        let mut s = initial();
        if !timeline {
            s.extend(resource_operation(false));
        }
        (f, None, s)
    };
    let base = steps.len();
    assert_eq!(
        base,
        if native {
            58
        } else if timeline {
            31
        } else {
            41
        }
    );
    let mut later = browse(Operation::Reply);
    later.extend(observation(Operation::Reply));
    later.extend(historical_steps(&h, mode));
    if let Some(original) = &native_history {
        immutable_images::bind_steps(&mut later, original);
        for step in &mut later {
            if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
                bind_native(&mut value, original);
                step.stdout = serde_json::to_vec(&value).unwrap();
            }
            if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdin) {
                bind_native(&mut value, original);
                step.stdin = serde_json::to_vec(&value).unwrap();
            }
        }
    }
    bind(&mut later, &h);
    steps.extend(later);
    fixture.scripted_gh(&steps);
    let git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    let mut native_before = None;
    if timeline {
        selected(&mut session, &fixture, mouse);
        session.resize(120, 40);
        choose(&mut session, mouse, "[t Timeline]", b"t");
        session.wait_for_cursorless_condition(
            |frame| {
                crate::support::main_footer_visible(frame)
                    && frame.contents().contains("[T All threads]")
                    && frame.contents().contains("review_requested")
            },
            "Actual original Timeline before fresh All threads",
            Duration::from_secs(8),
        );
        choose(&mut session, mouse, "[T All threads]", b"T");
    } else {
        files(&mut session, &fixture, mouse);
        session.resize(120, 40);
        if native {
            choose(&mut session, mouse, "[v Native diff]", b"v");
            session.wait_for_command_count(&fixture, 58);
            session.wait_for_cursorless_condition(
                |frame| {
                    crate::support::overlay_body(frame, NATIVE).is_some_and(|body| {
                        body.contains("new-no-newline") && body.contains("[o Options]")
                    })
                },
                "Genuine Git-built Native source before historical thread navigation",
                Duration::from_secs(8),
            );
            control_v2(&mut session, mouse, NATIVE, "[o Options] [q Quit]", b"o");
            session.wait_for_cursorless_condition(
                |frame| {
                    crate::support::overlay_body(frame, NATIVE).is_some_and(|body| {
                        body.contains("[T Threads]") && body.contains("[o Back to diff]")
                    })
                },
                "Current Native option controls",
                Duration::from_secs(8),
            );
            native_before = Some(session.wait_for_overlay_body(NATIVE));
            control_v2(&mut session, mouse, NATIVE, "[T Threads]", b"T");
        } else {
            choose(&mut session, mouse, "[T Threads]", b"T");
        }
    }
    session.wait_for_command_count(&fixture, base + 17);
    owned(
        &mut session,
        &["NATIVE_THREAD_9", "OUTDATED", "[c Comments]"],
    );
    control_v2(&mut session, mouse, THREAD_TITLE, "[c Comments]", b"c");
    session.wait_for_command_count(&fixture, base + 28);
    panel_ready_v2(&mut session, &["Comment #9009", "[h Historical source]"]);
    control_v2(&mut session, mouse, PANEL, "Comment #9009", b"\x1b[B");
    panel_ready_v2(&mut session, &["> Comment #9009"]);
    let original = session.wait_for_overlay_body(PANEL);
    let sql = sql_state(&fixture);
    let trace = git_commands(&fixture);
    let calls = commands(&session, &fixture).len();
    control_v2(&mut session, mouse, PANEL, "[h Historical source]", b"h");
    owned_historical(&mut session, &["Choose exact observed commit role"]);
    assert_eq!(commands(&session, &fixture).len(), calls);
    for (cols, rows) in [(80, 24), (120, 40)] {
        session.resize_owned_header(cols, rows, OWNER, &["[c Associated C]", "[Esc Back]"]);
        owned_historical(&mut session, &["Choose exact observed commit role"]);
    }
    let endpoint = session.completed_cursorless_end();
    control_v2(
        &mut session,
        mouse,
        OWNER,
        if mode == 1 {
            "[o Original C0]"
        } else {
            "[c Associated C]"
        },
        if mode == 1 { b"o" } else { b"c" },
    );
    if mode == 4 {
        let child = held_child(&mut session, &fixture, base + 28 + 15);
        owned_historical(&mut session, &["Loading exact historical"]);
        control_v2(&mut session, mouse, OWNER, "[Esc Back]", b"\x1b");
        wait_original_child_gone(&mut session, &child);
        panel_ready_v2(&mut session, &["> Comment #9009"]);
        assert_eq!(session.wait_for_overlay_body(PANEL), original);
    } else if matches!(mode, 5 | 6) {
        session.wait_for_command_count(&fixture, steps.len());
        session.wait_for_new_cursorless_condition(endpoint,|frame|{let text=frame.contents();text.contains("private views hidden")&&!text.contains(THREAD_BODY)&&!text.contains("native-thread-actor")&&!text.contains("VERIFIED HISTORICAL")},"Actual source/account failure hides original private body before historical or parent return",Duration::from_secs(8));
    } else {
        session.wait_for_command_count(&fixture, steps.len());
        owned_historical(&mut session, &["Verified exact historical tree"]);
        for (cols, rows) in [(80, 24), (120, 40)] {
            session.resize_owned_header(cols, rows, OWNER, &["[PgDn Next]", "[Esc Back]"]);
            owned_historical(&mut session, &["Verified exact historical tree"]);
        }
        let wanted = match mode {
            1 => "Exact candidate path absent",
            2 => "Binary/invalid UTF8",
            3 => "LFS POINTER",
            7 => "Observed size exceeds512KiB",
            8 => "SYMLINK bytes only",
            9 => "Submodule commit link only",
            _ => "Historical line032",
        };
        reveal(&mut session, mouse, wanted);
        let body = session.wait_for_overlay_body(OWNER);
        assert!(!body.contains("[Confirm action]"));
        assert!(!body.contains("[External editor]"));
        if !matches!(mode, 1 | 7 | 9) {
            assert!(steps.iter().any(|step| step
                .args
                .get(1)
                .is_some_and(|arg| arg.contains("/git/blobs/"))));
        }
        if matches!(mode, 1 | 7 | 9) {
            assert!(!steps.iter().any(|step| step
                .args
                .get(1)
                .is_some_and(|arg| arg.contains("/git/blobs/"))));
        }
        control_v2(&mut session, mouse, OWNER, "[Esc Back]", b"\x1b");
        panel_ready_v2(&mut session, &["> Comment #9009"]);
        assert_eq!(session.wait_for_overlay_body(PANEL), original);
        if let Some(native_before) = native_before {
            control_v2(&mut session, mouse, PANEL, "[Esc Back]", b"\x1b");
            owned(&mut session, &["[c Comments]", "NATIVE_THREAD_9"]);
            let endpoint = session.completed_cursorless_end();
            control_v2(&mut session, mouse, THREAD_TITLE, "[Esc Back]", b"\x1b");
            session.wait_for_new_cursorless_condition(endpoint,|frame|crate::support::overlay_body(frame,NATIVE).is_some_and(|body|body==native_before),"Pure Back preserves exact original Native options/viewport/range without any replacement load",Duration::from_secs(8));
        }
    }
    exact_v2(&session, &fixture, &steps, 0);
    assert_eq!(sql_state(&fixture), sql);
    no_attempts(&fixture);
    assert_eq!(git_state(&fixture), git);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn review_threads_v5_historical_keyboard_native_associated_content_scroll_resize_and_exact_back() {
    journey(false, 0)
}
#[test]
fn review_threads_v5_historical_mouse_native_associated_content_scroll_resize_and_exact_back() {
    journey(true, 0)
}
#[test]
fn review_threads_v5_historical_keyboard_timeline_original_renamed_path_absence_not_placement() {
    journey(false, 1)
}
#[test]
fn review_threads_v5_historical_mouse_timeline_original_renamed_path_absence_not_placement() {
    journey(true, 1)
}
#[test]
fn review_threads_v5_historical_keyboard_binary_verified_bytes_not_text() {
    journey(false, 2)
}
#[test]
fn review_threads_v5_historical_mouse_binary_verified_bytes_not_text() {
    journey(true, 2)
}
#[test]
fn review_threads_v5_historical_keyboard_timeline_lfs_pointer_no_download() {
    journey(false, 3)
}
#[test]
fn review_threads_v5_historical_mouse_timeline_lfs_pointer_no_download() {
    journey(true, 3)
}
#[test]
fn review_threads_v5_historical_keyboard_actual_held_blob_cancel_preserves_comments() {
    journey(false, 4)
}
#[test]
fn review_threads_v5_historical_mouse_actual_held_blob_cancel_preserves_comments() {
    journey(true, 4)
}
#[test]
fn review_threads_v5_historical_keyboard_actual_numeric_account_loss_no_private_return() {
    journey(false, 5)
}
#[test]
fn review_threads_v5_historical_mouse_actual_numeric_account_loss_no_private_return() {
    journey(true, 5)
}
#[test]
fn review_threads_v5_historical_keyboard_wrong_commit_echo_refuses_old_body() {
    journey(false, 6)
}
#[test]
fn review_threads_v5_historical_mouse_wrong_commit_echo_refuses_old_body() {
    journey(true, 6)
}
#[test]
fn review_threads_v5_historical_keyboard_timeline_oversized_no_blob_fetch() {
    journey(false, 7)
}
#[test]
fn review_threads_v5_historical_mouse_timeline_oversized_no_blob_fetch() {
    journey(true, 7)
}
#[test]
fn review_threads_v5_historical_keyboard_actual_symlink_bytes_never_followed() {
    journey(false, 8)
}
#[test]
fn review_threads_v5_historical_mouse_actual_symlink_bytes_never_followed() {
    journey(true, 8)
}
#[test]
fn review_threads_v5_historical_keyboard_actual_submodule_link_never_fetched() {
    journey(false, 9)
}
#[test]
fn review_threads_v5_historical_mouse_actual_submodule_link_never_fetched() {
    journey(true, 9)
}
