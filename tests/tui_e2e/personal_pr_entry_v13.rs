//! Genuine ordinary personal-queue -> canonical f Files -> cached Back.
//! SOURCE declarations only until the serialized Root runner executes them.
use super::*;
#[path = "file_comments_personal_v75.rs"]
mod file_comments_v75;
#[path = "navigation_context_v10.rs"]
mod navigation_context_v10;
use std::{fs, time::Duration};
const REPO: &str = "example/repo0000";
const BODY: &str = "PRIVATE_ORIGINAL_PERSONAL_BODY_ALLOCATION";
// Real Git startup may asynchronously focus Repository until explicit input.
// All Personal descendants use the actual View action before their existing
// Refresh; this action adds no provider/SQL/Git request or synthetic App state.
fn refresh(session: &mut Session, mouse: bool) {
    session.wait_for_cursorless_condition(
        |screen| {
            crate::support::main_footer_visible(screen) && screen.contents().contains("1 Inbox")
        },
        "Completed actual main frame before explicit Personal Inbox entry",
        Duration::from_secs(8),
    );
    let before = session.completed_cursorless_end();
    super::choose(session, mouse, "1 Inbox", b"1");
    session.wait_for_new_cursorless_condition(
        before,
        |screen| {
            crate::support::main_footer_visible(screen)
                && screen.contents().contains("Inbox • focused view")
        },
        "NEW completed explicit Inbox-focused frame before Personal Refresh",
        Duration::from_secs(8),
    );
    super::refresh(session, mouse);
}
fn parent() -> Value {
    json!({"id":1007,"node_id":"PERSONAL_PR_1007","number":7,"title":"Actual personal source",
        "body":BODY,"url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7"),
        "html_url":format!("https://github.invalid/{REPO}/pull/7"),
        "repository_url":format!("https://github.invalid/api/v3/repos/{REPO}"),
        "user":{"id":51,"node_id":"AUTHOR_51","login":"actual-author","type":"User"},
        "state":"open","draft":false,"merged":false,"changed_files":1,
        "head":{"sha":"a".repeat(40),"ref":"feature/personal","repo":null},
        "base":{"sha":"b".repeat(40),"ref":"main","repo":{"id":10000,"node_id":"R_10000","full_name":REPO}}})
}
fn repository() -> Value {
    json!({"id":10000,"node_id":"R_10000","full_name":REPO,
    "owner":{"id":71,"node_id":"OWNER_71","login":"example","type":"Organization"},
    "private":true,"fork":false,"archived":false,"default_branch":"main","permissions":{"pull":true}})
}
fn complete_steps() -> (Vec<GhStep>, usize) {
    let mut steps = identity("octocat");
    steps.extend(identity("octocat"));
    for queue in ["author", "assignee", "review-requested"] {
        steps.push(search("octocat", queue, vec![parent()], false));
    }
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    for endpoint in [
        "issues/7/comments",
        "pulls/7/commits",
        "pulls/7/files",
        "pulls/7/reviews",
        "issues/7/timeline",
    ] {
        let value = if endpoint == "pulls/7/files" {
            json!([[{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]])
        } else {
            json!([[]])
        };
        let mut args = api(&format!("repos/{REPO}/{endpoint}?per_page=100"));
        args.extend(["--paginate".into(), "--slurp".into()]);
        steps.push(read(args, value));
    }
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    let original = steps.len();
    steps.extend(identity("octocat"));
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    steps.push(read(api(&format!("repos/{REPO}/pulls/7/files?per_page=100&page=1")),json!([
        {"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}])));
    steps.push(GhStep::read(&["pr","diff","7","--repo",&format!("github.invalid/{REPO}"),"--color","never"],
        "diff --git a/src/view.rs b/src/view.rs\n--- a/src/view.rs\n+++ b/src/view.rs\n@@ -1 +1 @@\n-old\n+new\n"));
    steps.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.extend(identity("octocat"));
    (steps, original)
}
fn personal_thread_phase_v75() -> Vec<GhStep> {
    let repo = json!({"id":"R_10000","databaseId":10000,"nameWithOwner":REPO});
    let pr = json!({"__typename":"PullRequest","id":"PERSONAL_PR_1007","fullDatabaseId":"1007","number":7,"headRefOid":"a".repeat(40),"baseRefOid":"b".repeat(40),"repository":repo});
    let comment = json!({"id":"PERSONAL_FILE_COMMENT_9181","fullDatabaseId":"9181","body":"Actual Personal FILE thread • no line anchor","path":"src/view.rs","line":null,"startLine":null,"originalLine":null,"originalStartLine":null,"createdAt":"2026-10-15T01:00:00Z","updatedAt":"2026-10-15T01:00:00Z","commit":{"oid":"a".repeat(40)},"originalCommit":{"oid":"a".repeat(40)},"replyTo":null,"author":{"__typename":"User","databaseId":51,"login":"actual-personal-file-author","avatarUrl":null},"pullRequest":pr,"repository":repo});
    let thread = json!({"id":"PERSONAL_FILE_THREAD_9181","subjectType":"FILE","isResolved":false,"isOutdated":false,"path":"src/view.rs","diffSide":"RIGHT","startDiffSide":null,"line":null,"startLine":null,"originalLine":null,"originalStartLine":null,"viewerCanReply":false,"viewerCanResolve":false,"viewerCanUnresolve":false,"resolvedBy":null,"pullRequest":pr,"repository":repo,"comments":{"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[comment]}});
    let mut root = pr;
    root["reviewThreads"] =
        json!({"totalCount":1,"pageInfo":{"hasNextPage":false,"endCursor":null},"nodes":[thread]});
    let mut graph = GhStep::read(
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
        &json!({"data":{"node":root}}).to_string(),
    );
    graph.stdin=serde_json::to_vec(&json!({"query":ghtui::providers::gh::METRICS_REVIEW_THREADS_QUERY,"variables":{"id":"PERSONAL_PR_1007","first":25,"after":null,"commentsFirst":20,"includeComments":true}})).unwrap();
    let mut s = identity("octocat");
    s.push(read(api(&format!("repos/{REPO}")), repository()));
    s.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    s.push(read(api(&format!("repos/{REPO}")), repository()));
    s.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    s.push(read(api("user"), json!({"id":42,"login":"octocat"})));
    s.push(read(api(&format!("repos/{REPO}")), repository()));
    s.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    s.push(graph);
    s.push(read(api(&format!("repos/{REPO}")), repository()));
    s.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    s.push(read(api("user"), json!({"id":42,"login":"octocat"})));
    s.push(read(api(&format!("repos/{REPO}/pulls/7")), parent()));
    s.push(read(api(&format!("repos/{REPO}")), repository()));
    s.extend(identity("octocat"));
    assert_eq!(s.len(), 17);
    s
}
fn sql(f: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        f.root.join("config/ghtui/cache.sqlite3"),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name!='sync_reservations' ORDER BY name").unwrap()
        .query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut all = Vec::new();
    for name in names {
        let mut stmt = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let n = stmt.column_count();
        let mut rows = stmt
            .query_map([], |r| {
                let mut fields = Vec::new();
                for i in 0..n {
                    fields.push(format!("{:?}", r.get_ref(i)?));
                }
                Ok(format!("{name}:{fields:?}"))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        all.extend(rows);
    }
    all
}
fn frame(s: &mut Session, after: usize, mut predicate: impl FnMut(&str) -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cursorless_end() > after
            && s.completed_cursorless_screen()
                .is_some_and(|f| predicate(&f.contents()))
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "new completed owned Personal frame: {}",
            s.screen()
        );
    }
}
// Body-editing Forms finish their cell diff with cursor Show, not Hide. Keep
// fullscreen frame() strict and opt in here to the existing completed-cell
// tracker. A newer cells marker plus the exact owned title and entire footer
// must agree before either input path can continue.
fn form_frame(s: &mut Session, after: usize, title: &str, mut predicate: impl FnMut(&str) -> bool) {
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|screen| {
                let Some(body) = crate::support::overlay_body(screen, title) else {
                    return false;
                };
                predicate(&body)
                    && [
                        "[Preview]",
                        "[External editor]",
                        "[Confirm action]",
                        "[Keep draft / Close]",
                    ]
                    .iter()
                    .all(|label| {
                        crate::support::completed_navigation_controls_v1::owned_action_position(
                            screen,
                            title,
                            label,
                            &[],
                        )
                        .is_some()
                    })
            })
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "new completed owned Personal Form cells ({title}): {}",
            s.screen()
        );
    }
}
fn journey(mouse: bool) {
    let fixture = configured();
    fixture.git(&["init", "-q"]);
    fixture.git(&["config", "user.name", "Personal PTY"]);
    fixture.git(&["config", "user.email", "personal@example.invalid"]);
    fs::write(fixture.repo().join("kept"), b"saved\n").unwrap();
    fixture.git(&["add", "--", "kept"]);
    fixture.git(&["commit", "-qm", "Independent original"]);
    fs::write(fixture.repo().join("kept"), b"dirty original\n").unwrap();
    fs::write(fixture.repo().join("untracked"), b"untracked raw\n").unwrap();
    let git = [
        ".git/HEAD",
        ".git/index",
        ".git/config",
        "kept",
        "untracked",
    ]
    .map(|p| fs::read(fixture.repo().join(p)).unwrap());
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let (mut steps, original) = complete_steps();
    steps.extend(personal_thread_phase_v75());
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("1 unique PRs across 3 queue matches");
    let before = session.completed_cursorless_end();
    if mouse {
        session.click_visible_action("Actual personal source");
    } else {
        session.send_key(b"\r");
    }
    frame(&mut session, before, |s| {
        crate::support::controls::text_has_control(s, "[f Files]")
            && crate::support::controls::text_has_control(s, "[p PR actions]")
            && s.contains(BODY)
    });
    journal(&mut session, &fixture, original);
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
        s.contains("Personal PR Files")
            && s.contains("File src/view.rs")
            && s.contains("Current observed head")
    });
    journal(&mut session, &fixture, steps.len() - 17);
    assert_eq!(sql(&fixture), before_sql);
    let db = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let source: Vec<(String, String, u64, u64)> = db
        .prepare(
            "SELECT login,source,generation,active FROM sync_reservations ORDER BY login,source",
        )
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(source.len(), 1);
    assert_eq!(source[0].0, "user-id:42");
    assert!(source[0].1.starts_with("personal-pr-source-v13/"));
    assert_eq!((source[0].2, source[0].3), (1, 1));
    // Same genuine Personal Files source now reaches the actual common Thread
    // browser, not the old disabled-route notice or a Catalogue fallback.
    session.resize(80, 24);
    // Threads is scoped to the actual selected file, not the metadata row
    // selected when Files first opens. Use real key/mouse navigation and
    // independently prove that choosing it performs no provider or SQL work.
    session.select_completed_progress_file(mouse, "File src/view.rs");
    journal(&mut session, &fixture, steps.len() - 17);
    assert_eq!(sql(&fixture), before_sql);
    let files_before_threads = crate::support::overlay_body(
        session.completed_cursorless_screen().unwrap(),
        "Inbox • focused view",
    )
    .expect("Actual selected Personal Files body before Threads");
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[T Threads]", b"T");
    frame(&mut session, before, |s| {
        s.contains("PR review threads • read only")
            && s.contains("FILE")
            && s.contains("actual-personal-file-author")
            && !s.contains("[Submit]")
    });
    journal(&mut session, &fixture, steps.len());
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    session.wait_for_cursorless_condition(
        |frame| {
            let s = frame.contents();
            crate::support::overlay_body(frame, "Inbox • focused view").as_ref()
                == Some(&files_before_threads)
                && s.contains("▌ File src/view.rs")
                && !s.contains("PR review threads • read only")
                && !s.contains("actual-personal-file-author")
        },
        "Pure Back preserves the genuine Personal Files and erases thread-only actors/body",
        Duration::from_secs(8),
    );
    session.resize(40, 14);
    session.wait_for_control("[b Back]");
    session.resize(120, 40);
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[b Back]", b"b");
    frame(&mut session, before, |s| {
        s.contains(BODY)
            && crate::support::controls::text_has_control(s, "[p PR actions]")
            && !s.contains("Current observed head")
    });
    assert_eq!(sql(&fixture), before_sql);
    journal(&mut session, &fixture, steps.len());
    let before = session.completed_cursorless_end();
    choose(&mut session, mouse, "[b Back]", b"b");
    // Back to the queue: the PR page and its actions are gone. Its description may only
    // reappear in the queue's own Preview pane, which shows the search result's body.
    frame(&mut session, before, |s| {
        s.contains("1 unique PRs across 3 queue matches")
            && !crate::support::controls::text_has_control(s, "[p PR actions]")
            && s.lines()
                .filter(|line| line.contains(BODY))
                .all(|line| line.contains("││ "))
    });
    session.quit_and_assert_restored_terminal(mouse);
    for (path, expected) in [
        ".git/HEAD",
        ".git/index",
        ".git/config",
        "kept",
        "untracked",
    ]
    .into_iter()
    .zip(git)
    {
        assert_eq!(fs::read(fixture.repo().join(path)).unwrap(), expected);
    }
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(sql(&fixture), before_sql);
}
#[test]
fn personal_pr_entry_v13_keyboard_actual_cross_queue_files_back_and_independent_effects() {
    journey(false)
}
#[test]
fn personal_pr_entry_v13_mouse_actual_cross_queue_files_back_and_independent_effects() {
    journey(true)
}

#[path = "history_entry_pr_v993.rs"]
mod history_entry_pr_v993;
#[path = "personal_pr_images_v13.rs"]
mod images;
#[path = "personal_pr_review_history_v15.rs"]
mod review_history_v15;
