//! DECLARED / UNRUN. Genuine Git images/Native proof, no selected line/range
//! prerequisite or manufactured Images view for a whole FILE comment.
use super::super::super::review_threads_v1::file_comments_v75 as file;
use super::*;
fn bind_file_phase(steps: &mut [GhStep], h: &history::GitImagesV1) {
    for step in steps {
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            if value.get("head").is_some() && value.get("base").is_some() {
                h.bind_parent(&mut value);
            }
            if step
                .args
                .get(1)
                .is_some_and(|endpoint| endpoint.contains("/pulls/7/files?"))
            {
                value = json!([h.file()]);
            }
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
    }
}
fn journey(mouse: bool) {
    let (fixture, h, mut steps) = fixture_and_steps(|_| {});
    let mut opened = file::observation();
    bind_file_phase(&mut opened, &h);
    steps.extend(opened.clone());
    steps.extend(opened);
    fixture.scripted_gh(&steps);
    let git = readonly_git(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    session.resize(120, 40);
    choose(&mut session, mouse, "[v Native diff]", b"v");
    session.wait_for_command_count(&fixture, 58);
    nready(&mut session, "new-no-newline");
    focus(&mut session, mouse, "new-no-newline");
    act(&mut session, mouse, NATIVE, "[o Options]", b"o");
    act(&mut session, mouse, NATIVE, "[w Wrap]", b"w");
    act(&mut session, mouse, NATIVE, "[o Back to diff]", b"o");
    nready(&mut session, "new-no-newline");
    let original = session.wait_for_overlay_body("Native PR diff •");
    let trace = git_commands(&fixture);
    let sql = sql_state(&fixture);
    // K does NOT require a selected canonical range; P still does. Exact file
    // membership and permission are independently rechecked in the worker.
    act(&mut session, mouse, NATIVE, "[K FILE comment]", b"K");
    session.wait_for_command_count(&fixture, 69);
    file::form_ready(&mut session, false);
    if mouse {
        session.click_owned_form_field("PR INTENT • Preview first", "Body");
    }
    session.paste("Whole FILE from original Native • no line/range");
    file::form_control(&mut session, mouse, "[Preview]");
    session.wait_for_command_count(&fixture, 80);
    file::form_ready(&mut session, true);
    file::form_control(&mut session, mouse, "[Keep draft / Close]");
    session.wait_for_cursorless_condition(|f|crate::support::overlay_body(f,"Native PR diff •").is_some_and(|body|body==original)&&!f.contents().contains(&file::title())&&!f.contents().contains("Whole FILE from original Native"),"Pure Native return retains actual frame/query/options/selection/range/viewport; no compensating read",Duration::from_secs(8));
    assert_eq!(commands(&session, &fixture).len(), 80);
    assert_eq!(git_commands(&fixture), trace);
    assert_eq!(readonly_git(&fixture), git);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    let db = raw(&fixture);
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_file_comment_origins_v75",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    // Only the actual unsent file textarea may be newly persisted.
    let before_non_file: Vec<_> = sql
        .into_iter()
        .filter(|r| !r.contains("file-comment-draft-v75/"))
        .collect();
    let after_non_file: Vec<_> = sql_state(&fixture)
        .into_iter()
        .filter(|r| !r.contains("file-comment-draft-v75/"))
        .collect();
    assert_eq!(after_non_file, before_non_file);
    for (row, step) in commands(&session, &fixture).iter().zip(&steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        assert!(!step
            .args
            .iter()
            .any(|v| matches!(v.as_str(), "POST" | "PATCH" | "PUT" | "DELETE")));
    }
    assert_read_only_git(&fixture);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn file_comments_v75_keyboard_genuine_native_patch_without_range_preview_and_exact_pure_back() {
    journey(false)
}
#[test]
fn file_comments_v75_mouse_genuine_native_patch_without_range_preview_and_exact_pure_back() {
    journey(true)
}
