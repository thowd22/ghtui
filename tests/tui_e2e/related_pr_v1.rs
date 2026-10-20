//! Genuine binary input and independent Git/gh/SQL/config/TTY oracles.
use super::*;
#[path = "related_pr_avatars_v1.rs"]
mod avatars_v1;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession};
const TITLE: &str = "Related PRs • exact original saved commit • read-only";
const PRIVATE: &str = "PRIVATE_RELATED_PR_CANARY";
fn pr() -> Value {
    json!({"id":9009,"node_id":"PR_related_9009","number":9,"title":PRIVATE,"body":"PRIVATE_RELATED_BODY_ORIGINAL","state":"closed","merged":true,"user":actor(201,"host-writer"),"base":{"repo":repo(),"sha":"a".repeat(40),"ref":"main"},"head":{"sha":"b".repeat(40),"ref":"feature"},"mergeable":true,"mergeable_state":"clean","changed_files":0,"created_at":"2023-11-14T22:13:20Z","updated_at":"2023-11-14T22:13:20Z","url":"https://github.invalid/api/v3/repos/example/original/pulls/9","html_url":"https://github.invalid/example/original/pull/9"})
}
fn discovery(sha: &str, uid: u64, final_uid: u64, empty: bool) -> Vec<GhStep> {
    let mut steps = principal(uid);
    if uid != 42 {
        return steps;
    }
    steps.push(get("repos/example/original", repo()));
    steps.push(get(
        &format!("repos/example/original/commits/{sha}?per_page=1"),
        json!({"sha":sha}),
    ));
    steps.push(get(
        &format!("repos/example/original/commits/{sha}/pulls?per_page=50&page=1"),
        if empty { json!([]) } else { json!([pr()]) },
    ));
    steps.push(get("repos/example/original", repo()));
    steps.extend(principal(final_uid));
    steps
}
fn opening(sha: &str) -> Vec<GhStep> {
    let mut steps = discovery(sha, 42, 42, false);
    steps.truncate(5);
    steps.push(get("repos/example/original/pulls/9", pr()));
    for path in [
        "issues/9/comments",
        "pulls/9/commits",
        "pulls/9/files",
        "pulls/9/reviews",
        "issues/9/timeline",
    ] {
        let mut step = get(
            &format!("repos/example/original/{path}?per_page=100"),
            if path == "issues/9/timeline" {
                json!([[{"event":"commented","created_at":"2023-11-14T22:13:20Z","body":"PRIVATE_RELATED_OBSERVED_TIMELINE","actor":actor(201,"host-writer")} ]])
            } else {
                json!([[]])
            },
        );
        step.args.extend(["--paginate".into(), "--slurp".into()]);
        steps.push(step);
    }
    steps.push(get("repos/example/original/pulls/9", pr()));
    steps.push(get("repos/example/original/pulls/9", pr()));
    steps.push(get("repos/example/original", repo()));
    steps.extend(principal(42));
    assert_eq!(steps.len(), 16);
    steps
}
fn completed(session: &mut Session, expected: &[&str], absent: &[&str]) {
    session.wait_for_cursorless_condition(
        |frame| {
            let screen = frame.contents();
            expected.iter().all(|s| screen.contains(s))
                && absent.iter().all(|s| !screen.contains(s))
        },
        "completed related PR visible/privacy state",
        Duration::from_secs(8),
    );
}
fn strict(session: &Session, fixture: &Fixture, count: usize) {
    let rows = gh(session, fixture);
    assert_eq!(rows.len(), count);
    assert!(rows
        .iter()
        .all(|r| r["matched"] == true && r["stdin_hex"] == ""));
    assert!(rows
        .iter()
        .all(|r| r["args"][0] == "--version" || r["args"][5] == "GET"));
    assert!(!rows.iter().any(|r| r["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a == "graphql" || a == "POST" || a == "PATCH" || a == "DELETE")));
}
#[derive(Clone, Copy)]
enum RelatedCase {
    Open,
    Empty,
    CurrentUid,
    FinalUid,
    Error,
    Cancel,
    Recreated,
    SourceEpoch,
    RawCap,
    Partial,
    ReplacedOrigin,
    OpenFinalUid,
    Actors,
    UnsafeState,
    OversizedActor,
}
fn journey_related(mouse: bool, case: RelatedCase) {
    let source = source("sha1");
    let fixture = &source.fixture;
    let mut steps = protocol(&source.original, Case::ReplacedOrigin); // genuine chooser only
    let mut first = discovery(&source.original, 42, 42, matches!(case, RelatedCase::Empty));
    if matches!(case, RelatedCase::Actors) {
        let mut bot = pr();
        bot["user"] = json!({"id":301,"node_id":"BOT_related_301","login":"ci[bot]","type":"Bot"});
        let mut unknown = pr();
        unknown["id"] = json!(9010);
        unknown["node_id"] = json!("PR_related_9010");
        unknown["number"] = json!(10);
        unknown["user"] = Value::Null;
        unknown["url"] = json!("https://github.invalid/api/v3/repos/example/original/pulls/10");
        unknown["html_url"] = json!("https://github.invalid/example/original/pull/10");
        first[4] = get(
            &format!(
                "repos/example/original/commits/{}/pulls?per_page=50&page=1",
                source.original
            ),
            json!([bot, unknown]),
        );
    }
    steps.extend(first);
    let expected = match case {
        RelatedCase::Open => {
            steps.extend(opening(&source.original));
            31
        }
        RelatedCase::Empty => 15,
        RelatedCase::Actors => 15,
        RelatedCase::CurrentUid => {
            steps.extend(discovery(&source.original, 43, 42, false));
            17
        }
        RelatedCase::FinalUid => {
            steps.extend(discovery(&source.original, 42, 43, false));
            23
        }
        RelatedCase::Error
        | RelatedCase::Cancel
        | RelatedCase::UnsafeState
        | RelatedCase::OversizedActor => {
            let mut follow = discovery(&source.original, 42, 42, false);
            follow.truncate(5);
            if matches!(case, RelatedCase::Cancel) {
                follow[4].delay_ms = 6000;
            } else if matches!(case, RelatedCase::Error) {
                follow[4].status = 1;
                follow[4].stdout.clear();
                follow[4].stderr = b"gh: Forbidden (HTTP 403)".to_vec();
            } else {
                let mut malformed = pr();
                if matches!(case, RelatedCase::UnsafeState) {
                    malformed["merged"] = json!(false);
                    malformed["state"] =
                        json!("open\u{1b}]52;c;PRIVATE_UNSAFE_OSC\u{7}\nPRIVATE_UNSAFE_ROW");
                } else {
                    malformed["user"]["login"] = json!("x".repeat(65_536));
                }
                follow[4] = get(
                    &format!(
                        "repos/example/original/commits/{}/pulls?per_page=50&page=1",
                        source.original
                    ),
                    json!([malformed]),
                );
            }
            steps.extend(follow);
            20
        }
        RelatedCase::Recreated => {
            let mut follow = principal(42);
            let mut changed = repo();
            changed["id"] = json!(102);
            changed["node_id"] = json!("RECREATED_related_102");
            follow.push(get("repos/example/original", changed));
            steps.extend(follow);
            18
        }
        RelatedCase::SourceEpoch => {
            let mut follow = discovery(&source.original, 42, 42, false);
            follow.truncate(4);
            steps.extend(follow);
            19
        }
        RelatedCase::RawCap => {
            let mut follow = discovery(&source.original, 42, 42, false);
            follow.truncate(5);
            follow[4].stdout = vec![b'x'; 1024 * 1024 + 1];
            steps.extend(follow);
            20
        }
        RelatedCase::Partial => {
            let mut follow = discovery(&source.original, 42, 42, false);
            follow.truncate(4);
            for page in 1..=4 {
                let rows = (0..50)
                    .map(|index| {
                        let mut v = pr();
                        let n = (page - 1) * 50 + index + 1;
                        v["id"] = json!(9000 + n);
                        v["node_id"] = json!(format!("PR_partial_{n}"));
                        v["number"] = json!(n);
                        v["url"] = json!(format!(
                            "https://github.invalid/api/v3/repos/example/original/pulls/{n}"
                        ));
                        v["html_url"] =
                            json!(format!("https://github.invalid/example/original/pull/{n}"));
                        v
                    })
                    .collect::<Vec<_>>();
                follow.push(get(
                    &format!(
                        "repos/example/original/commits/{}/pulls?per_page=50&page={page}",
                        source.original
                    ),
                    json!(rows),
                ));
            }
            follow.push(get("repos/example/original", repo()));
            follow.extend(principal(42));
            steps.extend(follow);
            26
        }
        RelatedCase::ReplacedOrigin => 15,
        RelatedCase::OpenFinalUid => {
            let mut follow = opening(&source.original);
            *follow.last_mut().unwrap() = get("user", json!({"id":43,"login":"octocat"}));
            steps.extend(follow);
            31
        }
    };
    fixture.scripted_gh(&steps);
    let config_path = fixture.root.join("config/ghtui/config.toml");
    Config {
        host: HOST.into(),
        refresh_seconds: 3600,
        avatars: false,
        ..Default::default()
    }
    .save(&config_path)
    .unwrap();
    let config = fs::read(&config_path).unwrap();
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(fixture)).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database(fixture)).unwrap());
    let mut before = state(fixture);
    let trace_path = fixture.root.join("git-role-trace.jsonl");
    let mut retained_original = None;
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        fixture,
        if mouse { 80 } else { 120 },
        if mouse { 24 } else { 40 },
        &[
            ("GIT_TRACE2_EVENT", trace_path.to_str().unwrap()),
            ("NO_COLOR", "1"),
        ],
    );
    saved_ready(
        &mut session,
        &source.head,
        &source.head,
        &["Recent commits"],
    );
    choose(&mut session, mouse, "[g History]", b"g");
    select_original(&mut session, mouse, &source.original);
    choose_key_bar(&mut session, mouse, "[a Author]");
    saved_ready(
        &mut session,
        &source.original,
        &source.head,
        &["Actor profile — Git author"],
    );
    choose_repository(&mut session, fixture, mouse);
    let original_sql = sql(fixture);
    let trace_before = trace(fixture);
    choose(&mut session, mouse, "[P Related PRs]", b"P");
    completed(
        &mut session,
        &[
            TITLE,
            "observed PRs",
            if matches!(case, RelatedCase::Empty) {
                "historical absence unknown"
            } else {
                PRIVATE
            },
        ],
        &[],
    );
    strict(&session, fixture, 15);
    assert_eq!(sql(fixture), original_sql);
    assert_eq!(
        trace(fixture).len(),
        trace_before.len() + 2,
        "Exactly original cat-file pair"
    );
    if matches!(case, RelatedCase::Open) {
        choose(&mut session, mouse, "[O Open selected PR]", b"O");
        completed(
            &mut session,
            &[
                TITLE,
                "Head bbbbb",
                "Base aaaaa",
                "PRIVATE_RELATED_BODY_ORIGINAL",
            ],
            &[],
        );
        strict(&session, fixture, 31);
        assert_eq!(trace(fixture).len(), trace_before.len() + 4);
        let observed = gh(&session, fixture);
        choose(&mut session, mouse, "[c Conversation]", b"c");
        completed(
            &mut session,
            &[TITLE, "PR description", "PRIVATE_RELATED_BODY_ORIGINAL"],
            &["Head bbbbb"],
        );
        assert_eq!(
            gh(&session, fixture),
            observed,
            "Observed tab no extra authorization/read"
        );
        for (button, key, body, absent) in [
            (
                "[h Commits]",
                b"h".as_slice(),
                "Commit = a saved version",
                "PR description",
            ),
            (
                "[t Timeline]",
                b"t".as_slice(),
                "PRIVATE_RELATED_OBSERVED_TIMELINE",
                "Commit = a saved version",
            ),
            (
                "[f Files]",
                b"f".as_slice(),
                "Canonical diff",
                "PRIVATE_RELATED_OBSERVED_TIMELINE",
            ),
            (
                "[o Summary]",
                b"o".as_slice(),
                "Head bbbbb",
                "Canonical diff",
            ),
        ] {
            choose(&mut session, mouse, button, key);
            completed(&mut session, &[TITLE, body], &[absent]);
            assert_eq!(gh(&session, fixture), observed);
        }
        choose(&mut session, mouse, "[k Checks]", b"k");
        completed(
            &mut session,
            &[TITLE, "no Checks/metadata/mutation authority", "Head bbbbb"],
            &[],
        );
        assert_eq!(gh(&session, fixture), observed);
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
        completed(
            &mut session,
            &[TITLE, PRIVATE],
            &["PRIVATE_RELATED_BODY_ORIGINAL"],
        );
    } else if matches!(case, RelatedCase::Actors) {
        completed(
            &mut session,
            &[TITLE, "[CI]", "@ci[bot]", "[bot]", "[?]", "unknown actor"],
            &["▀"],
        );
    } else if !matches!(case, RelatedCase::Empty) {
        if matches!(case, RelatedCase::ReplacedOrigin) {
            let preserved = fixture.root.join("related-original-kept");
            fs::rename(fixture.repo(), &preserved).unwrap();
            let output = Command::new("git")
                .args(["clone", "--quiet", "--no-hardlinks"])
                .arg(&preserved)
                .arg(fixture.repo())
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()
                .unwrap();
            assert!(output.status.success());
            fs::write(
                fixture.repo().join("role-scratch.txt"),
                b"replacement private untracked\n",
            )
            .unwrap();
            before = state(fixture);
            retained_original = Some((
                preserved.clone(),
                fs::read(preserved.join(".git/index")).unwrap(),
                fs::read(preserved.join("readme")).unwrap(),
                fs::read(preserved.join("role-scratch.txt")).unwrap(),
            ));
        }
        if matches!(case, RelatedCase::SourceEpoch) {
            let connection = rusqlite::Connection::open(database(fixture)).unwrap();
            assert_eq!(connection.execute("UPDATE sync_reservations SET generation=generation+1 WHERE host=?1 AND login=?2",rusqlite::params![HOST,"user-id:42"]).unwrap(),1);
        }
        let original_candidate_position = session
            .locate(PRIVATE)
            .expect("Actual old candidate before a new explicit read");
        if matches!(case, RelatedCase::OpenFinalUid) {
            choose(&mut session, mouse, "[O Open selected PR]", b"O");
        } else {
            choose(&mut session, mouse, "[r Discover]", b"r");
        }
        if matches!(case, RelatedCase::Cancel) {
            wait_gh(&mut session, fixture, 20);
            let pid = gh(&session, fixture)[19]["pid"].as_i64().unwrap() as i32;
            let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
            let original_child = process_identity(pid)
                .unwrap()
                .expect("Actual six-second gh child still live before Cancel");
            assert!(owner.owns(&original_child));
            assert!(!original_child.zombie);
            completed(&mut session, &[TITLE, "[Esc Cancel]"], &[PRIVATE]);
            let pending_journal = gh(&session, fixture);
            if mouse {
                session.click(original_candidate_position.0, original_candidate_position.1);
            } else {
                session.send_key(b"r");
                session.send_key(b"O");
            }
            session.poll_output(Duration::from_millis(30));
            completed(&mut session, &[TITLE, "[Esc Cancel]"], &[PRIVATE]);
            assert_eq!(gh(&session,fixture),pending_journal,"Capturing busy modal refuses stale row/duplicate discovery: no additional queue or child");
            choose(&mut session, mouse, "[Esc Cancel]", b"\x1b");
            saved_ready(
                &mut session,
                &source.original,
                &source.head,
                &["Actor profile — Git author"],
            );
            let until = Instant::now() + Duration::from_secs(3);
            loop {
                let Some(actual) = process_identity(pid).unwrap() else {
                    break;
                };
                assert!(owner.owns(&actual));
                assert_eq!(
                    (actual.pid, actual.uid, actual.session, actual.birth),
                    (
                        original_child.pid,
                        original_child.uid,
                        original_child.session,
                        original_child.birth
                    ),
                    "Only this captured original child identity can be observed/reaped"
                );
                if actual.zombie && actual.parent == std::process::id() as i32 {
                    let mut status = 0;
                    // SAFETY: exact positive original owned UID/birth/SID,
                    // now an actually adopted child; no global wait or signal.
                    assert_eq!(
                        unsafe { libc::waitpid(pid, &mut status, libc::WNOHANG) },
                        pid
                    );
                    assert!(libc::WIFEXITED(status) || libc::WIFSIGNALED(status));
                    assert!(process_identity(pid).unwrap().is_none());
                    break;
                }
                assert!(Instant::now() < until, "Exact owned gh child reaped");
                session.poll_output(Duration::from_millis(10));
            }
        } else if matches!(case, RelatedCase::Partial) {
            completed(
                &mut session,
                &[TITLE, "200 observed PRs", "partial bounded subset", PRIVATE],
                &[],
            );
        } else {
            completed(
                &mut session,
                &[TITLE, "Original related PR read unavailable"],
                &[
                    PRIVATE,
                    "PRIVATE_RELATED_BODY_ORIGINAL",
                    ROW,
                    "[r Discover]",
                    "[O Open selected PR]",
                    "PRIVATE_UNSAFE_OSC",
                    "PRIVATE_UNSAFE_ROW",
                ],
            );
            let failed_journal = gh(&session, fixture);
            if mouse {
                session.click(original_candidate_position.0, original_candidate_position.1);
            } else {
                session.send_key(b"r");
                session.send_key(b"O");
            }
            session.poll_output(Duration::from_millis(30));
            completed(
                &mut session,
                &[TITLE, "Original related PR read unavailable", "[Esc Back]"],
                &[PRIVATE, ROW, "[r Discover]", "[O Open selected PR]"],
            );
            assert_eq!(
                gh(&session, fixture),
                failed_journal,
                "Failed boundary cannot retry an old opaque choice or stale row"
            );
        }
    }
    strict(&session, fixture, expected);
    if matches!(case, RelatedCase::UnsafeState) {
        let raw = String::from_utf8_lossy(&session.transcript);
        assert!(!raw.contains("PRIVATE_UNSAFE_OSC"), "Rejected OSC payload never reaches raw terminal output, not merely an invisible screen cell");
        assert!(!raw.contains("PRIVATE_UNSAFE_ROW"));
    }
    if matches!(case, RelatedCase::OversizedActor) {
        assert!(!String::from_utf8_lossy(&session.transcript).contains(&"x".repeat(512)));
    }
    if !matches!(case, RelatedCase::Cancel) {
        choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    }
    completed(
        &mut session,
        &[
            "Actor profile — Git author",
            &format!("Saved: {}", source.original),
            &format!("HEAD: {}", source.head),
        ],
        &[PRIVATE, TITLE],
    );
    if matches!(
        case,
        RelatedCase::CurrentUid
            | RelatedCase::FinalUid
            | RelatedCase::Error
            | RelatedCase::Recreated
            | RelatedCase::SourceEpoch
            | RelatedCase::RawCap
            | RelatedCase::ReplacedOrigin
            | RelatedCase::OpenFinalUid
            | RelatedCase::UnsafeState
            | RelatedCase::OversizedActor
    ) {
        completed(
            &mut session,
            &["Actor profile — Git author"],
            &[ROW, PRIVATE, "[P Related PRs]", "[V Verify host report]"],
        );
        // Reach the real global chooser after returning. Opening it must not
        // revive the old private catalogue or trigger a hidden fresh read.
        let failed_journal = gh(&session, fixture);
        choose(&mut session, mouse, "[: Commands]", b":");
        overlay(
            &mut session,
            "Commands · current context",
            &["Filter focused:", "[Enter Choose]"],
        );
        session.send_key(b"\x01");
        session.paste("Discover repositories");
        overlay(
            &mut session,
            "Commands · current context",
            &["> Discover repositories"],
        );
        choose(&mut session, mouse, "> Discover repositories", b"\r");
        completed(
            &mut session,
            &[CHOOSER, "[s Source]", "[Esc Close]"],
            &[ROW, PRIVATE, "Repository ID: 101"],
        );
        assert_eq!(gh(&session, fixture), failed_journal);
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        completed(
            &mut session,
            &[
                "Actor profile — Git author",
                &format!("Saved: {}", source.original),
            ],
            &[CHOOSER, ROW, PRIVATE],
        );
        assert_eq!(gh(&session, fixture), failed_journal);
    }
    assert_eq!(state(fixture), before);
    assert_eq!(fs::read(&config_path).unwrap(), config);
    if !matches!(case, RelatedCase::SourceEpoch) {
        assert_eq!(sql(fixture), original_sql);
    }
    assert_read_only_trace(fixture);
    assert!(!fixture.root.join("role-curl-pid").exists());
    if let Some((root, index, work, scratch)) = retained_original {
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
        assert_eq!(fs::read(root.join("readme")).unwrap(), work);
        assert_eq!(fs::read(root.join("role-scratch.txt")).unwrap(), scratch);
    }
    assert!(gh(&session, fixture).iter().all(|r| r["args"][1]
        .as_str()
        .is_none_or(|a| !a.contains(&source.head))));
    session.send_key(b"q");
    session.assert_exit_and_restored(true);
    strict(&session, fixture, expected);
}
macro_rules! pair {
    ($key:ident,$mouse:ident,$case:ident) => {
        #[test]
        fn $key() {
            journey_related(false, RelatedCase::$case)
        }
        #[test]
        fn $mouse() {
            journey_related(true, RelatedCase::$case)
        }
    };
}
pair!(
    related_pr_keyboard_fresh_open_original_return,
    related_pr_mouse_fresh_open_original_return,
    Open
);
pair!(
    related_pr_keyboard_empty_unknown_absence,
    related_pr_mouse_empty_unknown_absence,
    Empty
);
pair!(
    related_pr_keyboard_current_numeric_uid_hides_old_body,
    related_pr_mouse_current_numeric_uid_hides_old_body,
    CurrentUid
);
pair!(
    related_pr_keyboard_final_numeric_uid_hides_captured_body,
    related_pr_mouse_final_numeric_uid_hides_captured_body,
    FinalUid
);
pair!(
    related_pr_keyboard_page_error_hides_old_body,
    related_pr_mouse_page_error_hides_old_body,
    Error
);
pair!(
    related_pr_keyboard_live_cancel_reaps_child,
    related_pr_mouse_live_cancel_reaps_child,
    Cancel
);
pair!(
    related_pr_keyboard_recreated_repository_hides_old_body,
    related_pr_mouse_recreated_repository_hides_old_body,
    Recreated
);
pair!(
    related_pr_keyboard_original_sql_epoch_hides_old_body,
    related_pr_mouse_original_sql_epoch_hides_old_body,
    SourceEpoch
);
pair!(
    related_pr_keyboard_actual_raw_capture_hides_old_body,
    related_pr_mouse_actual_raw_capture_hides_old_body,
    RawCap
);
pair!(
    related_pr_keyboard_partial_pages_preserve_unknown_coverage,
    related_pr_mouse_partial_pages_preserve_unknown_coverage,
    Partial
);
pair!(
    related_pr_keyboard_same_path_local_replacement_hides_old_body,
    related_pr_mouse_same_path_local_replacement_hides_old_body,
    ReplacedOrigin
);
pair!(
    related_pr_keyboard_open_final_uid_no_captured_private_body,
    related_pr_mouse_open_final_uid_no_captured_private_body,
    OpenFinalUid
);
pair!(
    related_pr_keyboard_observed_bot_and_unknown_actor_fallback,
    related_pr_mouse_observed_bot_and_unknown_actor_fallback,
    Actors
);
pair!(
    related_pr_keyboard_untrusted_state_refuses_before_terminal,
    related_pr_mouse_untrusted_state_refuses_before_terminal,
    UnsafeState
);
pair!(
    related_pr_keyboard_oversized_actor_scalar_hides_old_body,
    related_pr_mouse_oversized_actor_scalar_hides_old_body,
    OversizedActor
);

fn select_visible(session: &mut Session, mouse: bool, label: &str) {
    if mouse {
        session.click_visible_action(label);
        return;
    }
    session.send_key(b"\x1b[H");
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(10));
        if let Some((col, row)) = session.locate(label) {
            if session.cell_inverted(row, col) {
                session.send_key(b"\r");
                return;
            }
        }
        assert!(
            Instant::now() < until,
            "Actual canonical file row {label}: {}",
            session.screen()
        );
        session.send_key(b"\x1b[B");
        session.poll_output(Duration::from_millis(20));
    }
}
fn renamed_line_journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "old raw name.txt",
        b"root first line\nbaseline second line\n",
        "Original related file",
    );
    fixture.commit_file(
        "old raw name.txt",
        b"root first line\nresponsible related source line\n",
        "Responsible related source",
    );
    let responsible = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["mv", "old raw name.txt", "new raw name.txt"]);
    fixture.git(&["commit", "-qm", "Chosen related rename"]);
    let chosen = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file(
        "future-only",
        b"later unrelated branch bytes\n",
        "Later checkout is not provenance",
    );
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    assert_ne!(responsible, chosen);
    assert_ne!(head, chosen);
    let blame = fixture.git(&[
        "blame",
        "--line-porcelain",
        "-L",
        "2,2",
        &chosen,
        "--",
        "new raw name.txt",
    ]);
    assert_eq!(blame.split_whitespace().next(), Some(responsible.as_str()));
    fs::write(
        fixture.repo().join("new raw name.txt"),
        b"staged retained file bytes\n",
    )
    .unwrap();
    fixture.git(&["add", "--", "new raw name.txt"]);
    fs::write(
        fixture.repo().join("new raw name.txt"),
        b"dirty retained file bytes\n",
    )
    .unwrap();
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let refs = fixture.git(&["show-ref", "--head"]);
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let mut steps = protocol(&responsible, Case::ReplacedOrigin);
    steps.extend(discovery(&responsible, 42, 42, false));
    steps.extend(opening(&responsible));
    fixture.scripted_gh(&steps);
    Config {
        host: HOST.into(),
        refresh_seconds: 3600,
        avatars: false,
        ..Default::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&database(&fixture)).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&database(&fixture)).unwrap());
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("Recent commits");
    choose(&mut session, mouse, "[g History]", b"g");
    session.open_saved_graph_subject(mouse, "Chosen related rename", &chosen);
    choose(&mut session, mouse, "[t Tree]", b"t");
    session.wait_for_screen("Saved file tree");
    select_visible(&mut session, mouse, "new raw name.txt");
    session.wait_for_screen("responsible related source line");
    choose(&mut session, mouse, "[Y Blame]", b"Y");
    session.wait_for_screen("Blame lines 1–2");
    select_visible(&mut session, mouse, "│ responsible related source line");
    completed(
        &mut session,
        &[
            "File/line provenance — exact responsible saved version",
            "Original path: old raw name.txt",
            "Original line: 2 • chosen file line: 2",
            &responsible,
        ],
        &[],
    );
    choose_key_bar(&mut session, mouse, "[a Author]");
    session.wait_for_screen("Actor profile — Git author");
    choose_repository(&mut session, &fixture, mouse);
    let original_sql = sql(&fixture);
    choose(&mut session, mouse, "[P Related PRs]", b"P");
    completed(&mut session, &[TITLE, PRIVATE, &responsible], &[&head]);
    choose(&mut session, mouse, "[O Open selected PR]", b"O");
    completed(
        &mut session,
        &[TITLE, "PRIVATE_RELATED_BODY_ORIGINAL", "Head bbbbb"],
        &[],
    );
    strict(&session, &fixture, 31);
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    completed(
        &mut session,
        &[TITLE, PRIVATE],
        &["PRIVATE_RELATED_BODY_ORIGINAL"],
    );
    choose(&mut session, mouse, "[Esc Back]", b"\x1b");
    completed(
        &mut session,
        &["Actor profile — Git author", &responsible],
        &[TITLE, PRIVATE],
    );
    choose(&mut session, mouse, "[u File back]", b"u");
    completed(
        &mut session,
        &["Blame lines 1–2", "new raw name.txt", &chosen],
        &[PRIVATE],
    );
    select_visible(&mut session, mouse, "│ responsible related source line");
    completed(
        &mut session,
        &[
            "Original path: old raw name.txt",
            "Original line: 2 • chosen file line: 2",
            &responsible,
        ],
        &[PRIVATE],
    );
    assert_eq!(sql(&fixture), original_sql);
    strict(&session, &fixture, 31);
    let rows = gh(&session, &fixture);
    assert!(rows
        .iter()
        .all(|r| r["args"].as_array().unwrap().iter().all(|arg| arg
            .as_str()
            .is_none_or(|s| !s.contains(&chosen) && !s.contains(&head)))));
    assert_eq!(fixture.git(&["show-ref", "--head"]), refs);
    assert_eq!(fixture.git(&["status", "--porcelain=v2", "-z"]), status);
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
    assert_eq!(
        fs::read(fixture.repo().join("new raw name.txt")).unwrap(),
        b"dirty retained file bytes\n"
    );
    session.quit_and_assert_restored_terminal(mouse);
}
#[test]
fn related_pr_keyboard_renamed_file_responsible_not_chosen_or_head_exact_return() {
    renamed_line_journey(false)
}
#[test]
fn related_pr_mouse_renamed_file_responsible_not_chosen_or_head_exact_return() {
    renamed_line_journey(true)
}
