//! Actual ordinary authored/assigned/review-requested entry uses canonical f,
//! SAME Images/Inline Form/Inbox worker, original SQL and one exact POST. The
//! source declarations remain UNRUN until Root's serialized actual-binary gate.
use super::*;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession};
const COMMENT: &str = "Native Personal exact original inline body 界";
#[derive(Clone, Copy)]
enum Outcome {
    Confirmed,
    CancelReadback,
    ExternalReplacement,
}
fn current_parent(h: &history::GitImagesV1) -> Value {
    let mut p = parent();
    h.bind_parent(&mut p);
    p
}
fn open_phase(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = identity("octocat");
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7")),
        current_parent(h),
    ));
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7")),
        current_parent(h),
    ));
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.extend(identity("octocat"));
    steps
}
fn receipt(h: &history::GitImagesV1) -> Value {
    json!({"id":901,"node_id":"PERSONAL_NATIVE_INLINE_901","pull_request_url":format!("https://github.invalid/repos/{REPO}/pulls/7"),
    "user":{"id":42,"login":"octocat","type":"User"},"path":history::PATH,"side":"LEFT","line":2,"start_line":1,"start_side":"LEFT","commit_id":h.h,"body":COMMENT})
}
fn confirm_phase(h: &history::GitImagesV1, outcome: Outcome) -> Vec<GhStep> {
    let (mut all, opened) = complete_steps();
    bind(&mut all, h);
    let mut steps = all[opened..].to_vec();
    steps.truncate(7);
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7")),
        current_parent(h),
    ));
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.extend(identity("octocat"));
    let mut args = api(&format!("repos/{REPO}/pulls/7/comments"));
    let method = args.iter().position(|v| v == "GET").unwrap();
    args[method] = "POST".into();
    args.extend(["--input".into(), "-".into()]);
    let mut post = read(args, receipt(h));
    post.stdin=serde_json::to_vec(&json!({"path":history::PATH,"side":"LEFT","line":2,"start_line":1,"start_side":"LEFT","commit_id":h.h,"body":COMMENT})).unwrap();
    steps.push(post);
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/comments/901")),
        receipt(h),
    ));
    steps.push(read(api("user"), json!({"id":42,"login":"octocat"})));
    steps.push(read(api(&format!("repos/{REPO}")), repository()));
    steps.push(read(
        api(&format!("repos/{REPO}/pulls/7")),
        current_parent(h),
    ));
    steps.push(read(api("user"), json!({"id":42,"login":"octocat"})));
    assert_eq!(steps.len(), 17);
    match outcome {
        Outcome::Confirmed => {}
        Outcome::CancelReadback => {
            steps[12].delay_ms = 30_000;
            steps.truncate(13);
        }
        Outcome::ExternalReplacement => {
            steps[15].delay_ms = 6_000;
            steps.truncate(16);
        }
    }
    steps
}
fn database(f: &Fixture) -> std::path::PathBuf {
    f.root.join("config/ghtui/cache.sqlite3")
}
fn db(f: &Fixture) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(database(f), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap()
}
fn stable_private(f: &Fixture) -> Vec<String> {
    let db = db(f);
    let mut all = vec![];
    for query in [
        "SELECT json_array(host,login,resource,body) FROM drafts WHERE resource NOT LIKE 'inline/%' ORDER BY host,login,resource",
        "SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id",
        "SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name",
        "SELECT json_array(host,login,source,generation,complete) FROM checkpoints ORDER BY host,login,source",
    ] {
        all.extend(db.prepare(query).unwrap().query_map([], |row| row.get::<_, String>(0))
            .unwrap().map(Result::unwrap));
    }
    all
}
pub(super) fn exact_commands(s: &Session, f: &Fixture, steps: &[GhStep]) {
    let rows = s
        .inspect_command_journal(f)
        .lines()
        .map(|row| serde_json::from_str::<Value>(row).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), steps.len());
    for (row, step) in rows.iter().zip(steps) {
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
    assert_eq!(
        rows.iter()
            .filter(|row| row["args"].as_array().unwrap().iter().any(|v| v == "POST"))
            .count(),
        1
    );
    assert!(!rows.iter().any(|row| row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| matches!(v.as_str(), Some("PATCH" | "PUT" | "DELETE")))));
}
fn owned_form(s: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    let title = format!("Inline comment • {REPO}#7 • LEFT 1–2");
    let point = s.wait_for_owned_completed_action(&title, label, &[]);
    if mouse {
        s.click(point.0, point.1)
    } else {
        s.send_key(key)
    }
}
fn journey(mouse: bool) {
    for outcome in [
        Outcome::Confirmed,
        Outcome::CancelReadback,
        Outcome::ExternalReplacement,
    ] {
        let fixture = configured();
        let h = history::GitImagesV1::create(&fixture.repo(), false);
        let (mut steps, opened) = complete_steps();
        bind(&mut steps, &h);
        let files = steps.len();
        steps.extend(phase(&h));
        steps.extend(open_phase(&h));
        steps.extend(phase(&h));
        let confirmed_at = steps.len();
        steps.extend(confirm_phase(&h, outcome));
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
        let cache = Cache::open(&database(&fixture)).unwrap();
        for uid in [42, 43] {
            cache
                .put_draft(
                    &Scope::github("github.invalid", Some(uid)).unwrap(),
                    "kept",
                    &format!("PRIVATE_{uid}"),
                )
                .unwrap();
        }
        ghtui::pr_write_journal::PrWriteJournal::open(&database(&fixture)).unwrap();
        let stable = stable_private(&fixture);
        let before = s.completed_cursorless_end();
        choose(&mut s, mouse, "[f Files]", b"f");
        frame(&mut s, before, |f| {
            f.contains("Personal PR Files") && f.contains("File src/界.rs")
        });
        journal(&mut s, &fixture, files);
        choose(&mut s, mouse, "[N Next file]", b"N");
        s.wait_for_screen("> File src/界.rs");
        let before = s.completed_cursorless_end();
        choose(&mut s, mouse, "[O Old image]", b"O");
        frame(&mut s, before, |f| {
            f.contains("Immutable file images")
                && f.contains("old-no-newline")
                && f.contains("new-no-newline")
        });
        owned(&mut s, mouse, "Immutable file images", "[O Old]", b"O");
        s.wait_for_screen("Selection LEFT 1–1");
        if mouse {
            let first = s.locate("L      1").unwrap();
            let last = s.locate("L      2").unwrap();
            s.drag(first, last)
        } else {
            s.send_key(b"\x1b[1;2B")
        };
        owned(&mut s, mouse, "Immutable file images", "[R Range]", b"R");
        s.wait_for_screen("READ-ONLY LEFT 1–2");
        let before = s.completed_cells_end();
        owned(&mut s, mouse, "Immutable file images", "[P Comment]", b"P");
        form_frame(
            &mut s,
            before,
            &format!("Inline comment • {REPO}#7 • LEFT 1–2"),
            |f| {
                f.contains("PR INTENT • Preview first")
                    && f.contains("Body:")
                    && f.contains("[Preview]")
                    && f.contains("[F2 Full anchor details]")
            },
        );
        if mouse {
            s.click_owned_form_field("PR INTENT • Preview first", "Body")
        };
        s.paste(COMMENT);
        s.wait_for_screen(COMMENT);
        let before = s.completed_cells_end();
        owned_form(&mut s, mouse, "[Preview]", b"\x10");
        form_frame(
            &mut s,
            before,
            &format!("Inline comment • {REPO}#7 • LEFT 1–2"),
            |f| {
                f.contains("READ-ONLY PREVIEW • Confirm action")
                    && f.contains("LEFT lines 1–2")
                    && f.contains(COMMENT)
                    && f.contains("[Confirm action]")
            },
        );
        journal(&mut s, &fixture, confirmed_at);
        s.paste("POISON_PREPARED_PREVIEW_MUST_NOT_EDIT");
        assert!(!s.screen().contains("POISON_PREPARED_PREVIEW_MUST_NOT_EDIT"));
        s.resize(80, 24);
        s.wait_for_screen("READ-ONLY PREVIEW • Confirm action");
        s.resize(120, 40);
        s.wait_for_screen("[Confirm action]");
        let before = s.completed_cursorless_end();
        crate::local::confirm(&mut s, mouse, 1);
        let child = if matches!(outcome, Outcome::Confirmed) {
            None
        } else {
            s.wait_for_command_count(&fixture, steps.len());
            let records = s.inspect_command_journal(&fixture);
            let last: Value = serde_json::from_str(records.lines().last().unwrap()).unwrap();
            assert_eq!(last["matched"], true);
            let pid = i32::try_from(last["pid"].as_u64().unwrap()).unwrap();
            assert!(pid > 0);
            let original = process_identity(pid)
                .unwrap()
                .expect("actual owned delayed receipt/visibility child");
            let owner = OwnedPtySession::capture(s.owned_application_pid() as u32).unwrap();
            assert!(owner.owns(&original));
            assert!(!original.zombie);
            let row: (String, String) = db(&fixture)
                .query_row("SELECT state,command FROM pr_write_attempts", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .unwrap();
            assert_eq!(row.1, "inline-comment");
            assert_eq!(
                row.0,
                if matches!(outcome, Outcome::CancelReadback) {
                    "pending"
                } else {
                    "confirmed"
                }
            );
            let encoded: String = db(&fixture)
                .query_row(
                    "SELECT body FROM drafts WHERE resource LIKE 'inline/%'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            let value: Value = serde_json::from_str(&encoded).unwrap();
            assert_eq!(value["values"]["body"], COMMENT);
            assert!(value.get("approval").is_none());
            match outcome {
                Outcome::CancelReadback => owned_form(&mut s, mouse, "[Esc Cancel]", b"\x1b"),
                Outcome::ExternalReplacement => {
                    let external = rusqlite::Connection::open_with_flags(
                        database(&fixture),
                        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
                    )
                    .unwrap();
                    assert_eq!(external.execute("UPDATE drafts SET body='EXTERNAL_NEW_BODY_REF' WHERE resource LIKE 'inline/%'",[]).unwrap(),1);
                }
                Outcome::Confirmed => unreachable!(),
            }
            Some(original)
        };
        frame(&mut s, before, |f| {
            let safe = match outcome {
                Outcome::Confirmed => f.contains("Comment posted #901"),
                Outcome::CancelReadback => f.contains("canceled") && f.contains("nothing resent"),
                Outcome::ExternalReplacement => {
                    f.contains("Comment posted; current source unavailable")
                        && f.contains("refresh explicitly")
                }
            };
            safe && !f.contains("Inline comment •")
                && !f.contains("old-no-newline")
                && !f.contains("new-no-newline")
                && !f.contains(COMMENT)
                && !f.contains(BODY)
                && f.contains("[q Quit]")
        });
        if let Some(original) = child {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while let Some(now) = process_identity(original.pid).unwrap() {
                assert_eq!(
                    (now.uid, now.birth, now.session),
                    (original.uid, original.birth, original.session)
                );
                assert!(std::time::Instant::now() < deadline);
                s.poll_output(Duration::from_millis(10));
            }
        }
        exact_commands(&s, &fixture, &steps);
        let final_state: String = db(&fixture)
            .query_row("SELECT state FROM pr_write_attempts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            final_state,
            if matches!(outcome, Outcome::CancelReadback) {
                "uncertain"
            } else {
                "confirmed"
            }
        );
        match outcome {
            Outcome::Confirmed => assert_eq!(
                db(&fixture)
                    .query_row(
                        "SELECT count(*) FROM drafts WHERE resource LIKE 'inline/%'",
                        [],
                        |r| r.get::<_, u64>(0)
                    )
                    .unwrap(),
                0
            ),
            Outcome::ExternalReplacement => assert_eq!(
                db(&fixture)
                    .query_row(
                        "SELECT body FROM drafts WHERE resource LIKE 'inline/%'",
                        [],
                        |r| r.get::<_, String>(0)
                    )
                    .unwrap(),
                "EXTERNAL_NEW_BODY_REF"
            ),
            Outcome::CancelReadback => assert_eq!(
                serde_json::from_str::<Value>(
                    &db(&fixture)
                        .query_row(
                            "SELECT body FROM drafts WHERE resource LIKE 'inline/%'",
                            [],
                            |r| r.get::<_, String>(0)
                        )
                        .unwrap()
                )
                .unwrap()["values"]["body"],
                COMMENT
            ),
        }
        assert_eq!(
            db(&fixture)
                .query_row(
                    "SELECT count(*) FROM pr_write_attempts WHERE principal='user-id:43'",
                    [],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(stable_private(&fixture), stable);
        s.quit_and_assert_restored_terminal(mouse);
        for (path, expected) in paths.into_iter().zip(git) {
            assert_eq!(fs::read(fixture.repo().join(path)).unwrap(), expected);
        }
        assert_eq!(fixture.git(&["show-ref"]), refs);
        assert_eq!(
            fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
            config
        );
        assert_eq!(stable_private(&fixture), stable);
    }
}
#[test]
fn personal_pr_common_v14_keyboard_original_inline_confirm_cancel_external_sql_and_effects() {
    journey(false)
}
#[test]
fn personal_pr_common_v14_mouse_original_inline_confirm_cancel_external_sql_and_effects() {
    journey(true)
}
