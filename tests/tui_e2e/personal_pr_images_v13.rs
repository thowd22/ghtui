//! Actual ordinary personal PR -> canonical Files -> verified images -> SAME
//! Native renderer/query/Source -> cached Back. Source-only declarations.
use super::*;
use crate::immutable_git_v1 as history;
#[path = "personal_pr_images_lifecycle_v13.rs"]
mod lifecycle;
#[path = "personal_pr_publication_v14.rs"]
mod publication_v14;
#[path = "personal_pr_reviews_v14.rs"]
mod reviews_v14;
fn bind(steps: &mut [GhStep], h: &history::GitImagesV1) {
    for step in steps {
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            if value.get("head").is_some() && value.get("base").is_some() {
                h.bind_parent(&mut value);
            }
            if step
                .args
                .get(1)
                .is_some_and(|v| v.contains("/pulls/7/files"))
            {
                value = if step.args.iter().any(|v| v == "--slurp") {
                    json!([[h.file()]])
                } else {
                    json!([h.file()])
                };
            }
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
        if step.args.first().is_some_and(|v| v == "pr") {
            step.stdout = h.patch.clone();
        }
    }
}
fn phase(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    let mut p = parent();
    h.bind_parent(&mut p);
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), p.clone()));
    for (endpoint, value) in [
        (
            format!("compare/{}...{}?per_page=1&page=1", h.b, h.h),
            h.compare(),
        ),
        (format!("git/commits/{}", h.h), h.commit(true)),
        (format!("git/commits/{}", h.m), h.commit(false)),
    ] {
        steps.push(read(api(&format!("repos/{REPO}/{endpoint}")), value));
    }
    for (sha, value) in &h.old_trees {
        steps.push(read(
            api(&format!("repos/{REPO}/git/trees/{sha}")),
            value.clone(),
        ));
    }
    steps.push(blob(&h.old_blob, &h.old_bytes));
    for (sha, value) in &h.new_trees {
        steps.push(read(
            api(&format!("repos/{REPO}/git/trees/{sha}")),
            value.clone(),
        ));
    }
    steps.push(blob(&h.new_blob, &h.new_bytes));
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), p));
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.extend(identity("octocat"));
    steps
}
fn blob(sha: &str, body: &[u8]) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/git/blobs/{sha}"),
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.raw+json",
            "--allow-escape-sequences",
        ],
        "",
    );
    step.stdout = body.to_vec();
    step
}
fn native_ready(s: &mut Session, after: usize, text: &str) {
    frame(s, after, |f| {
        f.contains("Native PR diff •")
            && f.contains("[o Options]")
            && f.contains(text)
            && !f.contains("NOT CURRENT")
            && !f.contains("preparing")
    });
}
fn owned(s: &mut Session, mouse: bool, title: &str, label: &str, key: &[u8]) {
    let point = if title == "Native PR diff •" && matches!(label, "[o Options]" | "[Esc Close]") {
        native_footer(s, label)
    } else {
        s.wait_for_owned_completed_action(title, label, &[])
    };
    if mouse {
        s.click(point.0, point.1)
    } else {
        s.send_key(key)
    }
}
fn native_footer(s: &mut Session, label: &str) -> (u16, u16) {
    const FOOTER: &str = "[Esc Close] [o Options] [q Quit]";
    let offset = FOOTER
        .find(label)
        .expect("declared exact Native footer control");
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if let Some(point) = s.completed_cells_screen().and_then(|screen| {
            // This COMPLETE, unique toolbar is the actual bottom block. The
            // duplicated header Options/Close captions cannot satisfy it.
            let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
                screen,
                "Native PR diff •",
                FOOTER,
                &[],
            )?;
            (y == screen.size().0.checked_sub(2)?)
                .then_some((x + u16::try_from(offset).unwrap(), y))
        }) {
            return point;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Completed owned Native bottom toolbar for {label}: {}",
            s.screen()
        );
    }
}
fn journey(mouse: bool) {
    let fixture = configured();
    let h = history::GitImagesV1::create(&fixture.repo(), false);
    let (mut steps, opened) = complete_steps();
    bind(&mut steps, &h);
    let files = steps.len();
    steps.extend(phase(&h));
    fixture.scripted_gh(&steps);
    let paths = [
        ".git/HEAD",
        ".git/index",
        ".git/config",
        history::PATH,
        "base-only",
        "LOCAL_DIRTY_CANARY",
    ];
    let git = paths.map(|p| fs::read(fixture.repo().join(p)).unwrap());
    let refs = fixture.git(&["show-ref"]);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("1 unique PRs across 3 queue matches");
    let before = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("Actual personal source")
    } else {
        session.send_key(b"\r")
    };
    frame(&mut session, before, |s| {
        s.contains(BODY) && crate::support::controls::text_has_control(s, "[f Files]")
    });
    journal(&mut session, &fixture, opened);
    let database = fixture.root.join("config/ghtui/cache.sqlite3");
    let cache = Cache::open(&database).unwrap();
    for uid in [42, 43] {
        cache
            .put_draft(
                &Scope::github("github.invalid", Some(uid)).unwrap(),
                "kept",
                &format!("PRIVATE_{uid}"),
            )
            .unwrap();
    }
    ghtui::pr_write_journal::PrWriteJournal::open(&database).unwrap();
    let before_sql = sql(&fixture);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[f Files]", b"f");
    frame(&mut session, before, |s| {
        s.contains("Personal PR Files") && s.contains("File src/界.rs")
    });
    journal(&mut session, &fixture, files);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[N Next file]", b"N");
    frame(&mut session, before, |s| s.contains("> File src/界.rs"));
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[} Next hunk]", b"}");
    frame(&mut session, before, |s| {
        s.contains("> @@ -1,2 +1,2") && s.contains("new-no-newline")
    });
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[X Patch context]", b"X");
    frame(&mut session, before, |s| {
        s.contains("Loaded patch context")
            && s.contains("old Some")
            && s.contains("new Some")
            && s.contains("[Esc Close]")
    });
    if mouse {
        session.close_overlay_by_mouse("Loaded patch context")
    } else {
        session.close_overlay_by_key("Loaded patch context")
    };
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[L Layout]", b"L");
    frame(&mut session, before, |s| {
        s.lines()
            .any(|line| line.matches("context\\r").count() == 2 && line.contains('│'))
            && s.contains("Loaded patch navigation")
    });
    journal(&mut session, &fixture, files);
    assert_eq!(sql(&fixture), before_sql);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[O Old image]", b"O");
    frame(&mut session, before, |s| {
        s.contains("Immutable file images")
            && s.contains("OLD (merge-base M)")
            && s.contains("NEW (review head H)")
            && s.contains("old-no-newline")
            && s.contains("new-no-newline")
    });
    journal(&mut session, &fixture, steps.len());
    assert_eq!(sql(&fixture), before_sql);
    let before = session.completed_cursorless_end();
    owned(
        &mut session,
        mouse,
        "Immutable file images",
        "[v Native diff]",
        b"v",
    );
    native_ready(&mut session, before, "new-no-newline");
    assert!(!session.screen().contains(BODY));
    assert!(!session.screen().contains("PRIVATE_42"));
    let before = session.completed_cursorless_end();
    owned(&mut session, mouse, "Native PR diff •", "[o Options]", b"o");
    frame(&mut session, before, |s| {
        s.contains("[o Back to diff]") && s.contains("[/ Search patch]")
    });
    owned(
        &mut session,
        mouse,
        "Native PR diff •",
        "[/ Search patch]",
        b"/",
    );
    let before = session.completed_cursorless_end();
    session.send_key(b"new-no-newline");
    session.send_key(b"\x1b");
    frame(&mut session, before, |s| {
        s.contains("Native PR diff •")
            && s.contains("new-no-newline")
            && s.contains("Selected 0 (hidden 0) • 1 matches • hidden rows 0")
            && !s.contains("NOT CURRENT")
            && !s.contains("preparing")
    });
    session.resize_owned_modal(80, 24, "Native PR diff •", &["[Esc Close]", "[o Options]"]);
    session.wait_for_screen("Native PR diff •");
    session.resize_owned_modal(120, 40, "Native PR diff •", &["[Esc Close]", "[o Options]"]);
    // The same current prepared query/source persists through a pure resize.
    session.wait_for_screen("Selected 0 (hidden 0) • 1 matches • hidden rows 0");
    let before = session.completed_cursorless_end();
    owned(&mut session, mouse, "Native PR diff •", "[I Source]", b"I");
    session.wait_for_new_cursorless_condition(
        before,
        |screen| {
            let Some(body) = crate::support::overlay_body(screen, "Native PR source • read-only")
            else {
                return false;
            };
            // The exact M wraps after its field label. Join only whitespace
            // inside this complete owned Source box, never its covered page
            // or shortened digest, and require both labelled full identities.
            let compact: String = body.chars().filter(|c| !c.is_whitespace()).collect();
            compact.contains("Repository:example/repo0000•ID10000")
                && compact.contains(&format!("ActualOLDcommonstartingpointM:{}", h.m))
                && compact.contains(&format!("ReviewHEADH/NEW:{}", h.h))
        },
        "Completed owned Source retains exact original repository, M and H across hard wrapping",
        Duration::from_secs(8),
    );
    if mouse {
        session.click_visible_action("[Esc Close]")
    } else {
        session.close_overlay_by_key("Native PR source • read-only")
    };
    session.wait_for_screen("[I Source]");
    let before = session.completed_cursorless_end();
    owned(
        &mut session,
        mouse,
        "Native PR diff •",
        "[Esc Close]",
        b"\x1b",
    );
    frame(&mut session, before, |s| {
        s.contains("Immutable file images") && !s.contains("[o Back to diff]")
    });
    let before = session.completed_cursorless_end();
    owned(
        &mut session,
        mouse,
        "Immutable file images",
        "[b Back]",
        b"b",
    );
    frame(&mut session, before, |s| {
        s.contains(BODY)
            && crate::support::controls::text_has_control(s, "[p PR actions]")
            && !s.contains("Immutable file images")
    });
    assert_eq!(sql(&fixture), before_sql);
    journal(&mut session, &fixture, steps.len());
    session.quit_and_assert_restored_terminal(mouse);
    for (path, expected) in paths.into_iter().zip(git) {
        assert_eq!(fs::read(fixture.repo().join(path)).unwrap(), expected);
    }
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(sql(&fixture), before_sql);
}
#[test]
fn personal_pr_entry_v13_images_keyboard_same_native_query_source_resize_back_preserve_all_effects()
{
    journey(false)
}
#[test]
fn personal_pr_entry_v13_images_mouse_same_native_query_source_resize_back_preserve_all_effects() {
    journey(true)
}
