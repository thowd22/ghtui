//! Real Personal queue -> canonical Files -> SAME Reviews engine -> fresh
//! server-PENDING selection -> explicit submission. No line/Files fabrication,
//! source promotion, direct-handler substitute or legacy publication fallback.
use super::*;
const SUMMARY: &str = "Personal exact native pending review submission 界";
// The same complete controls as the actual Reviews browser, in paint order.
// Its full-screen wrapped footer cannot use the generic global-footer observer.
const REVIEW_CONTROLS_V15: [&str; 19] = [
    "[B New batch]",
    "[R Resume selected]",
    "[U Summary]",
    "[C Submit comment]",
    "[A Approve]",
    "[X Request changes]",
    "[D Discard pending]",
    "[I Dismiss: eligibility UNKNOWN]",
    "[< Left]",
    "[> Right]",
    "[Esc Close]",
    "[Q Quit]",
    "[F2 Next comment]",
    "[F3 Edit pending]",
    "[F4 Delete pending]",
    "[G Add selected range]",
    "[F5 Re-request reviewer]",
    "[F6 Teams]",
    "[F7 Re-request team]",
];
fn current_parent(h: &history::GitImagesV1) -> Value {
    let mut value = parent();
    h.bind_parent(&mut value);
    value
}
fn review(h: &history::GitImagesV1, submitted: bool) -> Value {
    let mut value = json!({"id":810,"node_id":"PERSONAL_PENDING_REVIEW_810","state":if submitted{"COMMENTED"}else{"PENDING"},
        "body":if submitted{SUMMARY}else{"Actual server review body"},"commit_id":h.h,
        "pull_request_url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7"),
        "user":{"id":42,"login":"octocat","type":"User"}});
    if submitted {
        value["submitted_at"] = json!("2026-10-15T00:00:00Z");
    }
    value
}
fn opening(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7")),
        current_parent(h),
    ));
    steps
}
fn closing(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = vec![
        read(api(&format!("repos/{REPO}/pulls/7")), current_parent(h)),
        read(api(&format!("repos/{REPO}")), repository()),
    ];
    steps.extend(identity("octocat"));
    steps
}
fn catalogue(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = opening(h);
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7/reviews?per_page=100&page=1")),
        json!([review(h, false)]),
    ));
    steps.extend(closing(h));
    steps
}
fn snapshot(h: &history::GitImagesV1, submitted: bool) -> Vec<GhStep> {
    vec![
        read(
            api(&format!("repos/{REPO}/pulls/7/reviews/810")),
            review(h, submitted),
        ),
        read(
            api(&format!(
                "repos/{REPO}/pulls/7/reviews/810/comments?per_page=100&page=1"
            )),
            json!([]),
        ),
        read(
            api(&format!("repos/{REPO}/pulls/7/reviews/810")),
            review(h, submitted),
        ),
    ]
}
fn select(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = opening(h);
    steps.extend(snapshot(h, false));
    steps.extend(closing(h));
    steps
}
fn confirmation(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = opening(h);
    steps.extend(snapshot(h, false));
    steps.extend(identity("octocat"));
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7")),
        current_parent(h),
    ));
    steps.extend(snapshot(h, false));
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7")),
        current_parent(h),
    ));
    steps.extend(closing(h));
    let mut args = api(&format!("repos/{REPO}/pulls/7/reviews/810/events"));
    let method = args.iter().position(|v| v == "GET").unwrap();
    args[method] = "POST".into();
    args.extend(["--input".into(), "-".into()]);
    let mut post = read(args, review(h, true));
    post.stdin = serde_json::to_vec(&json!({"event":"COMMENT","body":SUMMARY})).unwrap();
    steps.push(post);
    steps.extend(snapshot(h, true));
    steps.push(read(api("user"), json!({"id":42,"login":"octocat"})));
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7")),
        current_parent(h),
    ));
    steps.push(read(api("user"), json!({"id":42,"login":"octocat"})));
    steps
}
fn form_control(s: &mut Session, mouse: bool, label: &str) {
    let title = format!("Submit comment review • {REPO}#7");
    if mouse {
        let p = s.wait_for_owned_completed_action(&title, label, &[]);
        s.click(p.0, p.1)
    } else {
        s.focus_completed_owned_action_v1(
            &title,
            label,
            crate::support::completed_navigation_controls_v1::FocusAttributeV1::Inverse,
            &[
                "[Preview]",
                "[External editor]",
                "[Confirm action]",
                "[Keep draft / Close]",
            ],
            &[],
        );
        s.send_key(b"\r");
    }
}
fn raw(f: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        f.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap()
}
fn journey(mouse: bool) {
    let fixture = configured();
    let h = history::GitImagesV1::create(&fixture.repo(), false);
    let (mut steps, opened) = complete_steps();
    bind(&mut steps, &h);
    let files = steps.len();
    steps.extend(catalogue(&h));
    steps.extend(select(&h));
    steps.extend(catalogue(&h));
    steps.extend(select(&h));
    let before_confirm = steps.len();
    steps.extend(confirmation(&h));
    fixture.scripted_gh(&steps);
    let paths = [
        ".git/HEAD",
        ".git/index",
        ".git/config",
        history::PATH,
        "base-only",
        "LOCAL_DIRTY_CANARY",
    ];
    let git = paths.map(|path| fs::read(fixture.repo().join(path)).unwrap());
    let refs = fixture.git(&["show-ref"]);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = Session::launch(&fixture, 120, 40);
    s.wait_for_screen("ghtui");
    refresh(&mut s, mouse);
    s.wait_for_screen("1 unique PRs across 3 queue matches");
    let before = s.completed_cursorless_end();
    if mouse {
        s.click_visible_action("Actual personal source")
    } else {
        s.send_key(b"\r")
    };
    frame(&mut s, before, |f| {
        f.contains(BODY) && f.contains("[f Files]")
    });
    journal(&mut s, &fixture, opened);
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
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[f Files]", b"f");
    frame(&mut s, before, |f| {
        f.contains("Personal PR Files") && f.contains("File src/界.rs")
    });
    journal(&mut s, &fixture, files);
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[F10 Reviews]", b"\x1b[21~");
    frame(&mut s, before, |f| {
        f.contains("Scoped reviews")
            && f.contains("#810")
            && f.contains("[C Submit comment]")
            && f.contains("[R Resume selected]")
    });
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[R Resume selected]", b"R");
    frame(&mut s, before, |f| {
        f.contains("Scoped reviews")
            && f.contains("PENDING")
            && f.contains("Actual server review body")
    });
    s.resize_owned_control_block_v3(80, 24, "Scoped reviews", &REVIEW_CONTROLS_V15);
    s.wait_for_screen("[C Submit comment]");
    s.resize_owned_control_block_v3(120, 40, "Scoped reviews", &REVIEW_CONTROLS_V15);
    s.wait_for_screen("[R Resume selected]");
    let before = s.completed_cells_end();
    choose(&mut s, mouse, "[C Submit comment]", b"C");
    form_frame(
        &mut s,
        before,
        &format!("Submit comment review • {REPO}#7"),
        |f| {
            f.contains("PR INTENT • Preview first")
                && f.contains("Review summary:")
                && f.contains("Actual server review body")
                && f.contains("[Confirm action]")
        },
    );
    if mouse {
        s.click_owned_form_field("PR INTENT • Preview first", "Review summary")
    };
    s.send_key(b"\x01");
    s.paste(SUMMARY);
    s.wait_for_screen(SUMMARY);
    let before = s.completed_cells_end();
    form_control(&mut s, mouse, "[Preview]");
    form_frame(
        &mut s,
        before,
        &format!("Submit comment review • {REPO}#7"),
        |f| {
            f.contains("READ-ONLY PREVIEW • Confirm action")
                && f.contains("Current H")
                && f.contains("[Confirm action]")
        },
    );
    journal(&mut s, &fixture, before_confirm);
    let before = s.completed_cells_end();
    s.paste("POISON_PREPARED_REVIEW_MUST_NOT_EDIT");
    form_frame(
        &mut s,
        before,
        &format!("Submit comment review • {REPO}#7"),
        |f| {
            f.contains("READ-ONLY PREVIEW • Confirm action")
                && f.contains("[Confirm action]")
                && !f.contains("POISON_PREPARED_REVIEW_MUST_NOT_EDIT")
        },
    );
    let before = s.completed_cursorless_end();
    form_control(&mut s, mouse, "[Confirm action]");
    frame(&mut s, before, |f| {
        f.contains("Review #810 effect confirmed; snapshot retired; refresh")
            && f.contains("[q Quit]")
            && !f.contains("Scoped reviews")
            && !f.contains("Submit comment review •")
            && !f.contains(BODY)
            && !f.contains(SUMMARY)
    });
    super::publication_v14::exact_commands(&s, &fixture, &steps);
    let db = raw(&fixture);
    let row: (String, String, String, String) = db
        .query_row(
            "SELECT command,state,pr_node,confirmed_json FROM pr_write_attempts",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .unwrap();
    assert_eq!(
        (&*row.0, &*row.1, &*row.2),
        ("review-submit-v2", "confirmed", "PERSONAL_PR_1007")
    );
    assert_eq!(
        serde_json::from_str::<Value>(&row.3).unwrap()["resource_id"],
        "810"
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource LIKE 'review-draft/%'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_attempts WHERE principal='user-id:43'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    for uid in [42, 43] {
        assert_eq!(
            Cache::open(&database)
                .unwrap()
                .draft(&Scope::github("github.invalid", Some(uid)).unwrap(), "kept")
                .unwrap(),
            Some(format!("PRIVATE_{uid}"))
        );
    }
    drop(db);
    s.quit_and_assert_restored_terminal(mouse);
    for (path, expected) in paths.into_iter().zip(git) {
        assert_eq!(fs::read(fixture.repo().join(path)).unwrap(), expected);
    }
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
}
#[test]
fn personal_pr_common_v14_keyboard_existing_server_pending_review_uses_same_fresh_submission_engine(
) {
    journey(false)
}
#[test]
fn personal_pr_common_v14_mouse_existing_server_pending_review_uses_same_fresh_submission_engine() {
    journey(true)
}
