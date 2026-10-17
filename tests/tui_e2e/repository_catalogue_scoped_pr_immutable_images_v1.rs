//! Real-binary journeys. Source7 + MyPR11 + PR13 + Files10
//! + actual immutable17 =58; viewer/range/layout/Back add ZERO gh/SQL calls.
use super::*;
pub(super) use crate::immutable_git_v1 as history;
#[path = "scoped_inline_comments_v1.rs"]
mod inline_comments;
#[path = "scoped_pending_reviews_v2.rs"]
mod pending_reviews_v2;
#[path = "scoped_pr_native_diff_v1.rs"]
mod scoped_pr_native_diff_v1;

pub(super) fn fixture_and_steps(
    alter: impl FnOnce(&mut Vec<GhStep>),
) -> (Fixture, history::GitImagesV1, Vec<GhStep>) {
    fixture_with_history(|repo| history::GitImagesV1::create(repo, false), alter)
}
fn fixture_with_bytes(
    old: &[u8],
    new: &[u8],
    alter: impl FnOnce(&mut Vec<GhStep>),
) -> (Fixture, history::GitImagesV1, Vec<GhStep>) {
    fixture_with_history(
        |repo| history::GitImagesV1::create_with_bytes(repo, false, old, new),
        alter,
    )
}
fn fixture_with_history(
    build: impl FnOnce(&Path) -> history::GitImagesV1,
    alter: impl FnOnce(&mut Vec<GhStep>),
) -> (Fixture, history::GitImagesV1, Vec<GhStep>) {
    // The existing fixture supplies the actual local bare remote/config/drafts.
    let fixture = resource_configured(&[]);
    let h = build(&fixture.repo());
    let mut steps = initial();
    steps.extend(resource_operation(false));
    bind_steps(&mut steps, &h);
    let mut parent = parent_resource();
    h.bind_parent(&mut parent);
    let mut image = principal(42);
    image.push(get(&format!("repos/{FIRST}"), repository(0)));
    image.push(get(&format!("repos/{FIRST}/pulls/7"), parent.clone()));
    image.push(get(
        &format!("repos/{FIRST}/compare/{}...{}?per_page=1&page=1", h.b, h.h),
        h.compare(),
    ));
    image.push(get(
        &format!("repos/{FIRST}/git/commits/{}", h.h),
        h.commit(true),
    ));
    image.push(get(
        &format!("repos/{FIRST}/git/commits/{}", h.m),
        h.commit(false),
    ));
    for (sha, value) in &h.old_trees {
        image.push(get(
            &format!("repos/{FIRST}/git/trees/{sha}"),
            value.clone(),
        ));
    }
    image.push(raw_blob(&h.old_blob, &h.old_bytes));
    for (sha, value) in &h.new_trees {
        image.push(get(
            &format!("repos/{FIRST}/git/trees/{sha}"),
            value.clone(),
        ));
    }
    image.push(raw_blob(&h.new_blob, &h.new_bytes));
    image.push(get(&format!("repos/{FIRST}/pulls/7"), parent));
    image.push(get(&format!("repos/{FIRST}"), repository(0)));
    image.extend(principal(42));
    assert_eq!(image.len(), 17);
    alter(&mut image);
    steps.extend(image);
    fixture.scripted_gh(&steps);
    (fixture, h, steps)
}
pub(super) fn bind_steps(steps: &mut [GhStep], h: &history::GitImagesV1) {
    for step in steps {
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            if value.get("head").is_some() && value.get("base").is_some() {
                h.bind_parent(&mut value);
            }
            if step
                .args
                .get(1)
                .is_some_and(|endpoint| endpoint.contains("/pulls/7/files"))
            {
                value = if step.args.iter().any(|arg| arg == "--slurp") {
                    json!([[h.file()]])
                } else {
                    json!([h.file()])
                };
            }
            if let Some(rows) = value.get_mut("check_runs").and_then(Value::as_array_mut) {
                for row in rows {
                    row["head_sha"] = json!(h.h);
                }
            }
            if value.get("statuses").is_some() {
                value["sha"] = json!(h.h);
            }
            step.stdout = value.to_string().into_bytes();
        }
        if step.args.first().is_some_and(|arg| arg == "pr") {
            step.stdout = h.patch.clone();
        }
        for arg in &mut step.args {
            if arg.contains("/commits/") {
                *arg = arg.replace(&"a".repeat(40), &h.h);
            }
        }
    }
}
fn raw_blob(sha: &str, bytes: &[u8]) -> GhStep {
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
    step.stdout = bytes.to_vec();
    step
}
fn files_selected(session: &mut Session, fixture: &Fixture, mouse: bool) {
    selected(session, fixture, mouse);
    choose(session, mouse, "[f Files]", b"f");
    session.wait_for_command_count(fixture, 41);
    ready(
        session,
        &[
            "src/界.rs",
            "[O Old image]",
            "Old side: current base-branch tip",
        ],
    );
    choose(session, mouse, "[N Next file]", b"N");
    ready(session, &["> File src/界.rs"]);
}
fn happy(mouse: bool) {
    let (fixture, h, steps) = fixture_and_steps(|_| {});
    let git_before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    let journal = git_commands(&fixture);
    choose(&mut session, mouse, "[O Old image]", b"O");
    session.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut session,
        "Immutable file images",
        &[
            "OLD (merge-base M)",
            "NEW (review head H)",
            "old-no-newline",
            "new-no-newline",
            "[R Range]",
            "[b Back]",
        ],
    );
    assert!(session.screen().contains(&h.m));
    assert!(!session.screen().contains(PRIVATE_DRAFT));
    assert!(!session.screen().contains(DETAIL_BODY));
    choose(&mut session, mouse, "[O Old]", b"O");
    ready(&mut session, &["OLD (merge-base M)", "old-no-newline"]);
    if mouse {
        let first = session.locate("L      1").unwrap();
        let last = session.locate("L      2").unwrap();
        session.drag(first, last);
    } else {
        session.send_key(b"\x1b[1;2B");
    }
    choose(&mut session, mouse, "[R Range]", b"R");
    ready(&mut session, &["READ-ONLY LEFT 1–2", &h.h]);
    // LEFT is the M image but the review publication target remains original H.
    assert_ne!(h.m, h.h);
    assert_eq!(commands(&session, &fixture).len(), 58);
    choose(&mut session, mouse, "[> Right]", b">");
    session
        .try_wait_for_condition(
            |screen| {
                screen.lines().any(|line| {
                    line.contains("L      2")
                        && line.contains("ewline")
                        && !line.contains("old-no-newline")
                })
            },
            "Actual horizontal scrolling changes old image text, not canonical line numbers",
            Duration::from_secs(8),
        )
        .unwrap();
    choose(&mut session, mouse, "[< Left]", b"<");
    ready(&mut session, &["L      2", "old-no-newline"]);
    choose(&mut session, mouse, "[N New]", b"N");
    ready(
        &mut session,
        &[
            "NEW (review head H)",
            "new-no-newline",
            "Selection RIGHT 1–1",
        ],
    );
    assert!(!session.screen().contains("READ-ONLY LEFT"));
    if mouse {
        let first = session.locate("R      1").unwrap();
        let last = session.locate("R      2").unwrap();
        session.drag(first, last);
    } else {
        session.send_key(b"\x1b[1;2B");
    }
    choose(&mut session, mouse, "[R Range]", b"R");
    ready(&mut session, &["READ-ONLY RIGHT 1–2", &h.h]);
    choose(&mut session, mouse, "[A Both]", b"A");
    choose(&mut session, mouse, "[L Layout]", b"L");
    ready(&mut session, &["OLD (merge-base M)", "NEW (review head H)"]);
    choose(&mut session, mouse, "[C Context]", b"C");
    session.resize_owned_modal(
        80,
        24,
        "Immutable file images",
        &["[Esc Close]", "[b Back]", "[q Quit]"],
    );
    ready(
        &mut session,
        &["[Esc Close]", "old-no-newline", "new-no-newline"],
    );
    if mouse {
        let (column, row) = session.locate("old-no-newline").unwrap();
        session.wheel(column, row, true);
    } else {
        session.send_key(b"\x1b[6~");
    }
    session.try_wait_for_condition(
        |screen| !screen.contains("L      1") && screen.contains("L      2") && screen.contains("R      1"),
        "Actual hovered wheel or keyboard page scroll changes the old image while current-side context remains",
        Duration::from_secs(8),
    ).unwrap();
    if mouse {
        let (column, row) = session.locate("old-no-newline").unwrap();
        session.wheel(column, row, false);
    } else {
        session.send_key(b"\x1b[5~");
    }
    ready(
        &mut session,
        &["L      1", "R      1", "old-no-newline", "new-no-newline"],
    );
    session.resize_owned_modal(
        120,
        40,
        "Immutable file images",
        &["[Esc Close]", "[b Back]", "[q Quit]"],
    );
    ready(&mut session, &["[Esc Close]", "[b Back]", "[q Quit]"]);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state(&fixture), git_before);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(git_commands(&fixture), journal);
    if mouse {
        session.click_visible_action("[b Back]");
    } else {
        session.focus_visible_action("[b Back]");
        session.send_key(b"\r");
    }
    session.wait_for_overlay_close("Immutable file images");
    session.wait_for_cursorless_condition(
        |frame| {
            let contents = frame.contents();
            contents.contains(ROW_TITLE)
                && !contents.contains("Immutable file images")
                && !contents.contains("old-no-newline")
                && !contents.contains("new-no-newline")
        },
        "Completed original Files frame after image Back, with both private images erased",
        Duration::from_secs(8),
    );
    let restored = session.completed_cursorless_screen().unwrap().contents();
    assert!(!restored.contains("old-no-newline"));
    assert!(!restored.contains("new-no-newline"));
    unchanged(&session, &fixture, &before, &steps);
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(git_state(&fixture), git_before);
}
#[test]
fn catalogue_immutable_file_images_keyboard_crlf_shift_ranges_layout_resize_cached_back() {
    happy(false);
}
#[test]
fn catalogue_immutable_file_images_mouse_drag_both_sides_layout_context_resize_cached_back() {
    happy(true);
}
fn refusal(mouse: bool) {
    let (fixture, _, steps) = fixture_and_steps(|image| {
        image[16] = get("user", json!({"id":43,"login":"octocat"}));
    });
    let before_git = git_state(&fixture);
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    choose(&mut session, mouse, "[O Old image]", b"O");
    session.wait_for_command_count(&fixture, 58);
    session.try_wait_for_condition(|screen|visible_words(screen).contains("Original repository scope is fenced • private pages hidden • no fallback or query sent"),
        "Original immutable source refused; exact tagged scope remains fenced",Duration::from_secs(8)).unwrap();
    let frame = session.screen();
    for private in [
        "old-no-newline",
        "new-no-newline",
        PRIVATE_DRAFT,
        DETAIL_BODY,
        ROW_TITLE,
    ] {
        assert!(!frame.contains(private), "private stale canary: {private}");
    }
    session.resize(80, 24);
    choose(&mut session, mouse, "2 Repository", b"2");
    ready(&mut session, &["Repository"]);
    choose(&mut session, mouse, "1 Inbox", b"1");
    session.wait_for_command_count(&fixture, 58);
    assert_eq!(commands(&session, &fixture).len(), 58);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state(&fixture), before_git);
    no_attempts(&fixture);
    exact_reads(&session, &fixture, &steps);
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn catalogue_immutable_file_images_keyboard_final_uid_drift_private_retirement_no_fallback() {
    refusal(false);
}
#[test]
fn catalogue_immutable_file_images_mouse_final_uid_drift_private_retirement_no_fallback() {
    refusal(true);
}

fn owned_quit(mouse: bool) {
    let (fixture, _, steps) = fixture_and_steps(|_| {});
    let before_git = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    let git_journal = git_commands(&fixture);
    choose(&mut session, mouse, "[O Old image]", b"O");
    session.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut session,
        "Immutable file images",
        &["old-no-newline", "new-no-newline", "[q Quit]"],
    );
    // The helper sends actual q / clicks the owned footer, not App handlers.
    session.quit_and_assert_restored_terminal(mouse);
    unchanged(&session, &fixture, &before, &steps);
    assert_eq!(git_state(&fixture), before_git);
    assert_eq!(git_commands(&fixture), git_journal);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
}
#[test]
fn catalogue_immutable_file_images_keyboard_owned_quit_restores_terminal_no_extra_reads() {
    owned_quit(false);
}
#[test]
fn catalogue_immutable_file_images_mouse_owned_quit_restores_terminal_no_extra_reads() {
    owned_quit(true);
}

fn present_empty_side(mouse: bool) {
    let (fixture, h, steps) = fixture_with_bytes(b"old\n", b"", |_| {});
    assert_eq!((h.additions, h.deletions), (0, 1));
    let git_before = git_state(&fixture);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, mouse);
    files_selected(&mut session, &fixture, mouse);
    let before = sql_state(&fixture);
    let git_journal = git_commands(&fixture);
    choose(&mut session, mouse, "[O Old image]", b"O");
    session.wait_for_command_count(&fixture, 58);
    overlay_ready(
        &mut session,
        "Immutable file images",
        &[
            "OLD (merge-base M)",
            "NEW (review head H)",
            "Some(Text) • 0 bytes",
            "[R Range]",
        ],
    );
    choose(&mut session, mouse, "[O Old]", b"O");
    choose(&mut session, mouse, "[R Range]", b"R");
    ready(&mut session, &["READ-ONLY LEFT 1–1", &h.h]);
    choose(&mut session, mouse, "[N New]", b"N");
    ready(
        &mut session,
        &[
            "NEW (review head H)",
            "Some(Text) • 0 bytes",
            "NEW image has no selectable text lines",
        ],
    );
    assert!(!session.screen().contains("READ-ONLY LEFT"));
    assert!(!session.screen().contains("L      1"));
    choose(&mut session, mouse, "[R Range]", b"R");
    ready(
        &mut session,
        &["Selection is outside one actual canonical PR hunk/side"],
    );
    assert!(!session.screen().contains("READ-ONLY LEFT"));
    choose(&mut session, mouse, "[O Old]", b"O");
    ready(&mut session, &["Selection LEFT 1–1", "L      1"]);
    choose(&mut session, mouse, "[R Range]", b"R");
    ready(&mut session, &["READ-ONLY LEFT 1–1", &h.h]);
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        "Immutable file images",
        &["[Esc Close]", "[b Back]", "[q Quit]"],
    );
    if mouse {
        session.close_overlay_by_mouse("Immutable file images");
    } else {
        session.close_overlay_by_key("Immutable file images");
    }
    ready(&mut session, &["[f Files]", "[k Checks]", "[b Back]"]);
    choose(&mut session, mouse, "[b Back]", b"b");
    ready(&mut session, &[ROW_TITLE]);
    unchanged(&session, &fixture, &before, &steps);
    assert_eq!(git_state(&fixture), git_before);
    assert_eq!(git_commands(&fixture), git_journal);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn catalogue_immutable_file_images_keyboard_present_empty_side_no_stale_range_and_canonical_old_return(
) {
    present_empty_side(false);
}
#[test]
fn catalogue_immutable_file_images_mouse_present_empty_side_no_stale_range_and_canonical_old_return(
) {
    present_empty_side(true);
}
