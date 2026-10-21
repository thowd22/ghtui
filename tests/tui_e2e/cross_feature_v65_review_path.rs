//! TASK-65 `complete_journeys`, full PR review path on the real executable, joined into ONE
//! strict offline fake-gh script: Inbox -> PR -> Files -> immutable images -> an inline LEFT
//! 1-2 range -> V Batch -> resume the server PENDING review -> G Add the inline comment to it
//! (exactly one GraphQL mutation) -> fresh Refresh -> PR -> Files -> F10 Reviews -> resume ->
//! A Approve with a summary (exactly one review event POST). By keyboard and by mouse, with a
//! resize of the review browser between 120x40 and 80x24. Declared from the Personal images
//! module so it shares the genuine Personal queue/Files/images fixtures; see
//! `cross_feature_v65.rs` for the other journeys and the AC3 recovery journeys.
use super::*;

const ADDED: &str = "Cross-feature inline note on old lines 1-2 界";
const SUMMARY: &str = "Cross-feature approval after the inline note 界";
const EXISTING: &str = "Actual existing server pending comment";
const REVIEW_NODE: &str = "ACTUAL_NATIVE_REVIEW_810";
const PENDING_REVIEW: &str = "query($id:ID!){node(id:$id){__typename ... on PullRequestReview{id fullDatabaseId body state commit{oid} author{__typename login ... on User{databaseId} ... on Bot{databaseId}} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}}}}";
const FIELDS: &str = "__typename id fullDatabaseId body state path line startLine commit{oid} updatedAt author{__typename login ... on User{databaseId} ... on Bot{databaseId}} pullRequestReview{id fullDatabaseId state} pullRequest{id fullDatabaseId number headRefOid baseRefOid repository{id databaseId nameWithOwner}} repository{id databaseId nameWithOwner}";
// The complete review browser controls, in paint order (same list as personal_pr_reviews_v14).
const REVIEW_CONTROLS: [&str; 19] = [
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

fn current(h: &history::GitImagesV1) -> Value {
    let mut value = parent();
    h.bind_parent(&mut value);
    value
}
fn pr_read(h: &history::GitImagesV1) -> GhStep {
    read(api(&format!("repos/{REPO}/pulls/7")), current(h))
}
fn repo_read() -> GhStep {
    read(api(&format!("repos/{REPO}")), repository())
}
fn user_read() -> GhStep {
    read(api("user"), json!({"id":42,"login":"octocat"}))
}
fn opening(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.push(repo_read());
    steps.push(pr_read(h));
    steps
}
fn closing(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = vec![pr_read(h), repo_read()];
    steps.extend(identity("octocat"));
    steps
}
/// The viewer's own server PENDING review, or the same review after its Approve event.
fn review(h: &history::GitImagesV1, approved: bool) -> Value {
    let mut value = json!({"id":810,"node_id":REVIEW_NODE,
        "state":if approved {"APPROVED"} else {"PENDING"},
        "body":if approved {SUMMARY} else {"Actual server review body"},"commit_id":h.h,
        "pull_request_url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7"),
        "user":{"id":42,"login":"octocat","type":"User"}});
    if approved {
        value["submitted_at"] = json!("2026-10-21T00:00:00Z");
    }
    value
}
fn existing(h: &history::GitImagesV1) -> Value {
    json!({"id":911,"node_id":"ACTUAL_NATIVE_COMMENT_911","pull_request_review_id":810,
        "pull_request_url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7"),
        "user":{"id":42,"login":"octocat","type":"User"},
        "path":history::PATH,"side":"RIGHT","line":2,"start_line":1,"start_side":"RIGHT",
        "body":EXISTING,"commit_id":h.h,"in_reply_to_id":null})
}
/// The review's comments before (only the existing one) and after the inline Add.
fn comments(h: &history::GitImagesV1, added: bool) -> Value {
    let mut all = vec![existing(h)];
    if added {
        let mut note = existing(h);
        note["id"] = json!(912);
        note["node_id"] = json!("ACTUAL_NATIVE_COMMENT_912");
        note["body"] = json!(ADDED);
        note["side"] = json!("LEFT");
        note["start_side"] = json!("LEFT");
        all.push(note);
    }
    json!(all)
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
fn snapshot(h: &history::GitImagesV1, added: bool, approved: bool) -> Vec<GhStep> {
    let row = review(h, approved);
    vec![
        read(
            api(&format!("repos/{REPO}/pulls/7/reviews/810")),
            row.clone(),
        ),
        read(
            api(&format!(
                "repos/{REPO}/pulls/7/reviews/810/comments?per_page=100&page=1"
            )),
            comments(h, added),
        ),
        read(api(&format!("repos/{REPO}/pulls/7/reviews/810")), row),
    ]
}
fn selection(h: &history::GitImagesV1, added: bool) -> Vec<GhStep> {
    let mut steps = opening(h);
    steps.extend(snapshot(h, added, false));
    steps.extend(closing(h));
    steps
}
fn graphql(payload: Value, response: Value) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            "graphql",
            "--hostname",
            "github.invalid",
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &response.to_string(),
    );
    step.stdin = serde_json::to_vec(&payload).unwrap();
    step
}
fn graph_parent(h: &history::GitImagesV1) -> Value {
    let actual = current(h);
    let repo = repository();
    json!({"id":actual["node_id"],"fullDatabaseId":actual["id"],"number":7,"headRefOid":h.h,"baseRefOid":h.b,
        "repository":{"id":repo["node_id"],"databaseId":repo["id"],"nameWithOwner":REPO}})
}
/// The PENDING review re-observed by GraphQL node, after its REST snapshot.
fn observation(h: &history::GitImagesV1) -> Vec<GhStep> {
    let parent = graph_parent(h);
    let node = json!({"__typename":"PullRequestReview","id":REVIEW_NODE,"fullDatabaseId":"810",
        "body":"Actual server review body","state":"PENDING","submittedAt":null,"commit":{"oid":h.h},
        "author":{"__typename":"User","databaseId":42,"id":"ACTUAL_USER_51","login":"octocat","name":null,"avatarUrl":null},
        "pullRequest":parent,"repository":parent["repository"]});
    let mut steps = snapshot(h, false, false);
    steps.push(graphql(
        json!({"query":PENDING_REVIEW,"variables":{"id":REVIEW_NODE}}),
        json!({"data":{"node":node}}),
    ));
    steps
}
/// The one write of the Add: a LEFT 1-2 thread added to the PENDING review.
fn add_thread(h: &history::GitImagesV1) -> GhStep {
    let parent = graph_parent(h);
    let added = json!({"__typename":"PullRequestReviewComment","id":"ACTUAL_NATIVE_COMMENT_912","fullDatabaseId":"912",
        "body":ADDED,"state":"PENDING","path":history::PATH,"line":2,"startLine":1,"commit":{"oid":h.h},
        "updatedAt":"2026-10-21T09:00:00Z","author":{"__typename":"User","databaseId":42,"login":"octocat"},
        "pullRequestReview":{"id":REVIEW_NODE,"fullDatabaseId":"810","state":"PENDING"},
        "pullRequest":parent,"repository":parent["repository"]});
    let input = json!({"pullRequestReviewId":REVIEW_NODE,"path":history::PATH,
        "line":2,"side":"LEFT","startLine":1,"startSide":"LEFT","subjectType":"LINE","body":ADDED});
    let fields = format!(
        "thread{{id comments(first:2){{nodes{{{FIELDS}}} totalCount pageInfo{{hasNextPage}}}}}}"
    );
    graphql(
        json!({"query":format!("mutation($input:AddPullRequestReviewThreadInput!){{addPullRequestReviewThread(input:$input){{{fields}}}}}"),
            "variables":{"input":input}}),
        json!({"data":{"addPullRequestReviewThread":{"thread":{"id":"ACTUAL_NATIVE_PENDING_THREAD_912",
            "comments":{"nodes":[added],"totalCount":1,"pageInfo":{"hasNextPage":false}}}}}}),
    )
}
/// Inline Add: V Batch, Resume, G Add, Preview and Confirm, after the images phase.
fn add_phase(h: &history::GitImagesV1, images: &[GhStep]) -> (Vec<GhStep>, usize) {
    let inner = &images[3..images.len() - 3];
    let mut steps = catalogue(h);
    steps.extend(selection(h, false));
    let mut editor = opening(h);
    editor.extend(observation(h));
    editor.extend(closing(h));
    steps.extend(editor);
    let mut preview = opening(h);
    preview.extend(inner.iter().cloned());
    preview.extend(observation(h));
    preview.extend(closing(h));
    steps.extend(preview);
    let confirm = steps.len();
    steps.extend(opening(h));
    steps.extend(inner.iter().cloned());
    steps.extend(observation(h));
    steps.extend(identity("octocat"));
    steps.push(pr_read(h));
    steps.extend(observation(h));
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7/files?per_page=100&page=1")),
        json!([h.file()]),
    ));
    steps.push(pr_read(h));
    steps.extend(closing(h));
    steps.push(add_thread(h));
    steps.extend(snapshot(h, true, false));
    steps.extend([user_read(), repo_read(), pr_read(h), user_read()]);
    (steps, confirm)
}
/// Approve: F10 Reviews, Resume, A Approve, Preview and Confirm of the same review.
fn approve_phase(h: &history::GitImagesV1) -> (Vec<GhStep>, usize) {
    let mut steps = catalogue(h);
    steps.extend(selection(h, true));
    steps.extend(catalogue(h));
    steps.extend(selection(h, true));
    let confirm = steps.len();
    steps.extend(opening(h));
    steps.extend(snapshot(h, true, false));
    steps.extend(identity("octocat"));
    steps.push(pr_read(h));
    steps.extend(snapshot(h, true, false));
    steps.push(pr_read(h));
    steps.extend(closing(h));
    let mut args = api(&format!("repos/{REPO}/pulls/7/reviews/810/events"));
    let method = args.iter().position(|v| v == "GET").unwrap();
    args[method] = "POST".into();
    args.extend(["--input".into(), "-".into()]);
    let mut post = read(args, review(h, true));
    post.stdin = serde_json::to_vec(&json!({"event":"APPROVE","body":SUMMARY})).unwrap();
    steps.push(post);
    steps.extend(snapshot(h, true, true));
    steps.extend([user_read(), repo_read(), pr_read(h), user_read()]);
    (steps, confirm)
}

fn owned_control(s: &mut Session, mouse: bool, title: &str, label: &str) {
    if mouse {
        let point = s.wait_for_owned_completed_action(title, label, &[]);
        s.click(point.0, point.1);
    } else {
        s.focus_completed_owned_action_v1(
            title,
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
/// Refresh the Inbox, open the PR and its Files, as the Personal queue does.
fn open_files(s: &mut Session, fixture: &Fixture, mouse: bool, opened: usize, files: usize) {
    let refreshed = s.completed_cursorless_end();
    refresh(s, mouse);
    // After a restart the cached queue is painted at once. Opening its row before this
    // refresh completes would cancel the refresh between --version and user, so wait for
    // the NEW fresh queue and its exact reads (all but the seven PR detail reads).
    s.wait_for_new_cursorless_condition(
        refreshed,
        |screen| {
            let text = screen.contents();
            crate::support::main_footer_visible(screen)
                && text.contains("Fresh • fetched")
                && text.contains("1 unique PRs across 3 queue matches")
        },
        "Fresh Personal queue after the explicit refresh, not restored cached rows",
        Duration::from_secs(8),
    );
    journal(s, fixture, opened - 7);
    let before = s.completed_cursorless_end();
    if mouse {
        s.click_visible_action("Actual personal source")
    } else {
        s.send_key(b"\r")
    };
    frame(s, before, |f| {
        f.contains(BODY) && crate::support::controls::text_has_control(f, "[f Files]")
    });
    journal(s, fixture, opened);
    let before = s.completed_cursorless_end();
    choose(s, mouse, "[f Files]", b"f");
    frame(s, before, |f| {
        f.contains("Personal PR Files") && f.contains("File src/界.rs")
    });
    journal(s, fixture, files);
}
fn resume(s: &mut Session, mouse: bool) {
    let before = s.completed_cursorless_end();
    choose(s, mouse, "[R Resume selected]", b"R");
    frame(s, before, |f| {
        f.contains("Scoped reviews") && f.contains("PENDING") && f.contains(EXISTING)
    });
}
fn writes(rows: &[Value]) -> usize {
    rows.iter()
        .filter(|row| {
            let args = row["args"].as_array().unwrap();
            if !args.iter().any(|v| v == "POST") {
                return false;
            }
            if args.get(1).is_some_and(|v| v == "graphql") {
                let hex = row["stdin_hex"].as_str().unwrap();
                let bytes = (0..hex.len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                    .collect::<Vec<_>>();
                let payload: Value = serde_json::from_slice(&bytes).unwrap();
                return payload["query"]
                    .as_str()
                    .is_some_and(|q| q.starts_with("mutation"));
            }
            true
        })
        .count()
}
fn attempt_state(s: &mut Session, fixture: &Fixture, command: &str) -> String {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let db = rusqlite::Connection::open_with_flags(
            fixture.root.join("config/ghtui/cache.sqlite3"),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let state: String = db
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE command=?1",
                [command],
                |r| r.get(0),
            )
            .unwrap();
        // The attempt row settles just after the fixture command returns.
        if state == "confirmed" || std::time::Instant::now() >= deadline {
            return state;
        }
        s.poll_output(Duration::from_millis(20));
    }
}

fn journey(mouse: bool) {
    let fixture = configured();
    let h = history::GitImagesV1::create(&fixture.repo(), false);
    let (mut steps, opened) = complete_steps();
    bind(&mut steps, &h);
    let files = steps.len();
    let images = phase(&h);
    steps.extend(images.clone());
    let imaged = steps.len();
    let (add, add_confirm) = add_phase(&h, &images);
    let add_confirm = steps.len() + add_confirm;
    steps.extend(add);
    let added = steps.len();
    let (mut again, reopened) = complete_steps();
    bind(&mut again, &h);
    let reopened = steps.len() + reopened;
    steps.extend(again);
    let refiled = steps.len();
    let (approve, approve_confirm) = approve_phase(&h);
    let approve_confirm = steps.len() + approve_confirm;
    steps.extend(approve);
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

    // Inbox -> PR -> Files.
    let mut s = Session::launch(&fixture, 120, 40);
    s.wait_for_screen("ghtui");
    open_files(&mut s, &fixture, mouse, opened, files);

    // Files -> immutable images -> an inline LEFT 1-2 range on the diff.
    choose(&mut s, mouse, "[N Next file]", b"N");
    s.wait_for_screen("▌ File src/界.rs");
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[O Old image]", b"O");
    frame(&mut s, before, |f| {
        f.contains("Immutable file images")
            && f.contains("old-no-newline")
            && f.contains("new-no-newline")
    });
    journal(&mut s, &fixture, imaged);
    owned(&mut s, mouse, "Immutable file images", "[O Old]", b"O");
    s.wait_for_screen("Selection LEFT 1–1");
    if mouse {
        let first = s.locate("L      1").unwrap();
        let last = s.locate("L      2").unwrap();
        s.drag(first, last)
    } else {
        s.send_key(b"\x1b[1;2B")
    };
    s.wait_for_screen("Selection LEFT 1–2");
    owned(&mut s, mouse, "Immutable file images", "[R Range]", b"R");
    s.wait_for_screen("READ-ONLY LEFT 1–2");

    // V Batch opens the review browser; resume the viewer's PENDING review.
    let before = s.completed_cursorless_end();
    owned(&mut s, mouse, "Immutable file images", "[V Batch]", b"V");
    frame(&mut s, before, |f| {
        f.contains("Scoped reviews") && f.contains("#810") && f.contains("[G Add selected range]")
    });
    resume(&mut s, mouse);

    // G Add: the inline comment goes into the PENDING review, not a separate comment.
    let title = format!("Add comment to exact server PENDING review — not submitted • {REPO}#7");
    choose(&mut s, mouse, "[G Add selected range]", b"G");
    s.wait_for_screen("Add comment to exact server PENDING review — not submitted");
    s.wait_for_screen("[Confirm action]");
    if mouse {
        s.click_owned_form_field("PR INTENT • Preview first", "Exact server PENDING comment");
    }
    s.send_key(b"\x01");
    s.paste(ADDED);
    s.wait_for_screen(ADDED);
    let before = s.completed_cursorless_end();
    owned_control(&mut s, mouse, &title, "[Preview]");
    s.wait_for_new_cursorless_condition(
        before,
        |screen| {
            let text = screen.contents();
            text.contains("READ-ONLY PREVIEW")
                && text.contains("[Confirm action]")
                && text.contains(&h.h)
        },
        "Add Preview names the current head and offers Confirm",
        Duration::from_secs(8),
    );
    journal(&mut s, &fixture, add_confirm);
    let outcome = s.completed_cursorless_end();
    owned_control(&mut s, mouse, &title, "[Confirm action]");
    s.wait_for_new_cursorless_condition(
        outcome,
        |screen| {
            let text = screen.contents();
            let (rows, cols) = screen.size();
            let status = screen
                .rows(0, cols)
                .nth(usize::from(rows.saturating_sub(2)))
                .unwrap_or_default();
            crate::support::main_footer_visible(screen)
                && status.contains("confirmed")
                && !text.contains("Scoped reviews")
                && !text.contains("READ-ONLY PREVIEW")
                && !text.contains(ADDED)
        },
        "Add confirmed; the private review source is retired until an explicit refresh",
        Duration::from_secs(8),
    );
    journal(&mut s, &fixture, added);
    assert_eq!(
        attempt_state(&mut s, &fixture, "review-pending-comment-add-v3"),
        "confirmed"
    );

    // The confirmed write fences the original Personal scope, so an in-session Refresh sends
    // nothing ("no fallback or query sent"). Restart from a fresh scope, as a user would, and
    // continue the same strict script: Inbox -> PR -> Files -> F10 Reviews -> resume.
    s.quit_and_assert_restored_terminal(mouse);
    let mut s = Session::launch(&fixture, 120, 40);
    s.wait_for_screen("ghtui");
    open_files(&mut s, &fixture, mouse, reopened, refiled);
    let before = s.completed_cursorless_end();
    choose(&mut s, mouse, "[F10 Reviews]", b"\x1b[21~");
    frame(&mut s, before, |f| {
        f.contains("Scoped reviews") && f.contains("#810") && f.contains("[A Approve]")
    });
    resume(&mut s, mouse);
    s.resize_owned_control_block_v3(80, 24, "Scoped reviews", &REVIEW_CONTROLS);
    s.wait_for_screen("[A Approve]");
    s.resize_owned_control_block_v3(120, 40, "Scoped reviews", &REVIEW_CONTROLS);
    s.wait_for_screen("[R Resume selected]");

    // A Approve with a summary: Preview, then exactly one review event.
    let title = format!("Submit approval • {REPO}#7");
    let before = s.completed_cells_end();
    choose(&mut s, mouse, "[A Approve]", b"A");
    form_frame(&mut s, before, &title, |f| {
        f.contains("PR INTENT • Preview first") && f.contains("Review summary:")
    });
    if mouse {
        s.click_owned_form_field("PR INTENT • Preview first", "Review summary")
    };
    s.send_key(b"\x01");
    s.paste(SUMMARY);
    s.wait_for_screen(SUMMARY);
    let before = s.completed_cells_end();
    owned_control(&mut s, mouse, &title, "[Preview]");
    form_frame(&mut s, before, &title, |f| {
        f.contains("READ-ONLY PREVIEW • Confirm action")
            && f.contains("Current H")
            && f.contains("[Confirm action]")
    });
    journal(&mut s, &fixture, approve_confirm);
    let before = s.completed_cursorless_end();
    owned_control(&mut s, mouse, &title, "[Confirm action]");
    frame(&mut s, before, |f| {
        f.contains("Review #810 effect confirmed; snapshot retired; refresh")
            && crate::support::controls::text_has_control(f, "[q Quit]")
            && !f.contains("Scoped reviews")
            && !f.contains(SUMMARY)
            && !f.contains(BODY)
    });

    // Independent effects: the exact script, two writes, two confirmed attempts.
    let rows: Vec<Value> = s
        .inspect_command_journal(&fixture)
        .lines()
        .map(|row| serde_json::from_str(row).unwrap())
        .collect();
    assert_eq!(rows.len(), steps.len());
    for (row, step) in rows.iter().zip(&steps) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    assert_eq!(writes(&rows), 2, "one Add mutation and one Approve event");
    assert_eq!(
        attempt_state(&mut s, &fixture, "review-submit-v2"),
        "confirmed"
    );
    let db = rusqlite::Connection::open_with_flags(
        fixture.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource LIKE 'review-draft/%'",
            [],
            |r| r.get::<_, u64>(0)
        )
        .unwrap(),
        0,
        "the submitted approval leaves no review draft"
    );
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
fn complete_journeys_v65_keyboard_inbox_files_inline_add_to_pending_review_then_approve() {
    journey(false)
}
#[test]
fn complete_journeys_v65_mouse_inbox_files_inline_add_to_pending_review_then_approve() {
    journey(true)
}
