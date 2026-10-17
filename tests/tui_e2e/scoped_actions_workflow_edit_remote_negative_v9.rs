//! SOURCE ONLY / UNRUN. Nested under the exact W9 remote helper checkpoint.
//! Actual binary, actual local D, real owned TLS Git, strict offline GH.
//! Tree owns registration and production; these cases never fabricate permits.
use super::*;

const DRAFT_KEY: &str = "scoped-actions-v9/repository/10000/workflow/7/local-edit";
type SqlRowsV66 = Vec<(String, Vec<Vec<rusqlite::types::Value>>)>;
struct LocalProofV66 {
    root: std::path::PathBuf,
    d: String,
    git: GitState,
    config: Vec<u8>,
    git_config: Vec<u8>,
    unrelated_sql: SqlRowsV66,
}

fn unrelated_remote_sql_v66(f: &Fixture) -> SqlRowsV66 {
    let mut rows = unrelated(f);
    // These three actual owned families are checked independently in
    // aliases_v66/journal/reference_draft below, not silently ignored.
    rows.retain(|(name, _)| {
        !matches!(
            name.as_str(),
            "actions_write_attempts" | "actions_write_generations" | "actions_write_resources"
        )
    });
    rows
}

fn aliases_v66(r: &RemoteFixture, d: &str, states: &[&str]) {
    if states.is_empty() {
        assert_fresh_remote_journal_v248(&r.fixture);
        return;
    }
    let mut aliases = vec![
        format!("{REMOTE_HOST}/{CURRENT}/workflow/7"),
        format!("{REMOTE_HOST}/{FIRST}/workflow/7"),
        format!("{REMOTE_HOST}/repository-id/10000/actions/workflow/7"),
        format!("{REMOTE_HOST}/{CURRENT}/branch/{BRANCH}"),
        format!("{REMOTE_HOST}/{FIRST}/branch/{BRANCH}"),
        format!("{REMOTE_HOST}/repository-id/10000/git/branch/{BRANCH}"),
    ];
    aliases.sort();
    let db = publication_v3::raw(&r.fixture);
    let resources = db.prepare("SELECT host,principal,resource_key,attempt_id FROM actions_write_resources ORDER BY attempt_id,resource_key").unwrap()
        .query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?))).unwrap()
        .map(Result::unwrap).collect::<Vec<_>>();
    let expected = (1..=states.len())
        .flat_map(|attempt| {
            aliases.iter().map(move |key| {
                (
                    REMOTE_HOST.to_owned(),
                    "user-id:42".to_owned(),
                    key.clone(),
                    attempt as i64,
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        resources, expected,
        "Only the exact current/original/numeric aliases for actual attempts exist"
    );
    let generations = db.prepare("SELECT host,principal,resource_key,generation FROM actions_write_generations ORDER BY resource_key").unwrap()
        .query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?))).unwrap()
        .map(Result::unwrap).collect::<Vec<_>>();
    let generation = states
        .iter()
        .map(|state| if *state == "pending" { 2 } else { 3 })
        .sum::<i64>();
    let expected = if states.is_empty() {
        Vec::new()
    } else {
        aliases
            .iter()
            .map(|key| {
                (
                    REMOTE_HOST.to_owned(),
                    "user-id:42".to_owned(),
                    key.clone(),
                    generation,
                )
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        generations, expected,
        "Exact begin+claim+terminal advancement; no foreign alias or blanket exclusion"
    );
    let markers = db.prepare("SELECT host,principal,resources_json,context_json,state,claimed,created_at FROM actions_write_attempts ORDER BY attempt_id").unwrap()
        .query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,String>(3)?,row.get::<_,String>(4)?,row.get::<_,i64>(5)?,row.get::<_,i64>(6)?))).unwrap()
        .map(Result::unwrap).collect::<Vec<_>>();
    assert_eq!(markers.len(), states.len());
    for (marker, state) in markers.iter().zip(states) {
        assert_eq!(marker.0, REMOTE_HOST);
        assert_eq!(marker.1, "user-id:42");
        assert_eq!(
            serde_json::from_str::<Vec<String>>(&marker.2).unwrap(),
            aliases
        );
        let context: Value = serde_json::from_str(&marker.3).unwrap();
        let remote = &context["workflow_edit_v9"];
        assert_eq!(remote["head_repository_id"], 10000);
        assert_eq!(remote["head_original_name"], FIRST);
        assert_eq!(remote["head_current_name"], CURRENT);
        assert_eq!(remote["head_ref"], BRANCH);
        assert_eq!(remote["base_ref"], "main");
        assert_eq!(remote["original_commit"], r.c);
        assert_eq!(remote["committed_head"], d);
        assert_eq!(remote["base_commit"], r.c);
        assert_eq!(marker.4, *state);
        assert_eq!(marker.5, 1);
        assert!(marker.6 > 0);
    }
}

fn original_checkout(before: &GitState, after: &GitState) {
    assert_eq!(after.head, before.head);
    assert_eq!(after.index, before.index);
    assert_eq!(after.status, before.status);
    assert_eq!(after.staged_diff, before.staged_diff);
    assert_eq!(after.working_diff, before.working_diff);
    assert_eq!(after.dirty, before.dirty);
    assert_eq!(after.staged, before.staged);
    assert_eq!(after.untracked, before.untracked);
}

fn expected_edit_draft() -> String {
    use std::fmt::Write;
    let mut raw = "ghtui-workflow-edit-draft-v9\n".to_owned();
    for value in ["isolated", EDIT, BRANCH, "Update workflow", TITLE, PR_BODY] {
        write!(&mut raw, "{}:{value}", value.len()).unwrap();
    }
    raw
}

fn exact_edit_draft(s: &mut Session, f: &Fixture) -> String {
    let expected = expected_edit_draft();
    let scope = Scope::github(REMOTE_HOST, Some(42)).unwrap();
    let cache = Cache::open(&database(f)).unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(raw) = cache.draft(&scope, DRAFT_KEY).unwrap() {
            if raw == expected {
                return raw;
            }
        }
        assert!(
            Instant::now() < until,
            "Actual exact accepted W9 text must really be saved; accepted != durable\n{}",
            s.screen()
        );
        s.poll_output(Duration::from_millis(1));
    }
}

fn bare_branch(r: &RemoteFixture) -> String {
    bare_git(
        &r.tls,
        &[
            "for-each-ref",
            "--format=%(objectname)",
            "refs/heads/workflow/edit",
        ],
    )
}

fn actual_d(r: &RemoteFixture, s: &mut Session, mouse: bool) -> (std::path::PathBuf, String) {
    let before = git_state(&r.fixture);
    let (root, d) = actual_local_commit(r, s, mouse);
    let after = git_state(&r.fixture);
    original_checkout(&before, &after);
    let mut expected = before.refs.lines().map(str::to_owned).collect::<Vec<_>>();
    expected.push(format!("{d} refs/heads/{BRANCH}"));
    expected.sort();
    let mut actual = after.refs.lines().map(str::to_owned).collect::<Vec<_>>();
    actual.sort();
    assert_eq!(
        actual, expected,
        "Only the actual reviewed NEW local branch D is added"
    );
    assert_eq!(
        r.fixture
            .git(&["-C", root.to_str().unwrap(), "rev-parse", "HEAD^"])
            .trim(),
        r.c
    );
    assert_eq!(exact_edit_draft(s, &r.fixture), expected_edit_draft());
    assert_fresh_remote_journal_v248(&r.fixture);
    aliases_v66(r, &d, &[]);
    (root, d)
}

fn retired(s: &mut Session, after: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && s.completed_cells_screen().is_some_and(|frame| {
                let text = frame.contents();
                crate::support::main_footer_visible(frame)
                    // Privacy fences the actual original repository scope;
                    // the next main-loop pass replaces the transient Actions
                    // status with this fixed explanation. Join only physical
                    // wrapped rows inside the SAME completed focused body.
                    && crate::support::overlay_body(frame, "Actions • focused view").is_some_and(
                        |body| {
                            body.lines()
                                .map(str::trim)
                                .filter(|line| !line.is_empty())
                                .collect::<Vec<_>>()
                                .join(" ")
                                == "Original repository scope is fenced • private pages hidden • no fallback or query sent"
                        },
                    )
                    && crate::support::overlay_body(frame, FORM).is_none()
                    && [
                        TITLE,
                        "PRIVATE_NATIVE_W9_EXACT_EDIT",
                        "Exact second line",
                        "[Confirm step]",
                        "[Esc Cancel]",
                    ]
                    .iter()
                    .all(|word| !text.contains(word))
            })
        {
            return;
        }
        assert!(Instant::now()<until,
            "NEW actual failure must hide ONLY its private original and expose usable main controls\n{}",s.screen());
    }
}

fn no_post(s: &Session, f: &Fixture) {
    assert!(commands(s, f).iter().all(|row| !row["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| matches!(arg.as_str(), Some("POST" | "PATCH" | "PUT" | "DELETE")))));
}

fn reference_draft(f: &Fixture, expected_attempt: i64) -> String {
    let raw = Cache::open(&database(f))
        .unwrap()
        .draft(&Scope::github(REMOTE_HOST, Some(42)).unwrap(), DRAFT_KEY)
        .unwrap()
        .expect("Actual claimed attempt retains its exact durable reference, never an absent ACK");
    let value: Value = serde_json::from_str(&raw).unwrap();
    assert_eq!(value["version"], 9);
    assert_eq!(value["purpose"], "scoped-actions-workflow-remote-v9");
    assert_eq!(value["actions_publication"]["attempt_id"], expected_attempt);
    assert!(value["actions_publication"]["fingerprint_sha256"]
        .as_str()
        .is_some_and(|v| v.len() == 64));
    for private in [TITLE, PR_BODY, "PRIVATE_NATIVE_W9_EXACT_EDIT"] {
        assert!(
            !raw.contains(private),
            "Reference is not private text replay permission"
        );
    }
    raw
}

fn uncertain(f: &Fixture, kinds: &[&str]) {
    let rows = journal(f);
    assert_eq!(rows.len(), kinds.len());
    for (index, (row, kind)) in rows.iter().zip(kinds).enumerate() {
        assert_eq!(row.0, serde_json::to_string(kind).unwrap());
        assert_eq!(
            row.1,
            if index + 1 == rows.len() {
                "uncertain"
            } else {
                "confirmed"
            }
        );
        assert!(
            !row.2.contains(TITLE)
                && !row.2.contains(PR_BODY)
                && !row.2.contains("PRIVATE_NATIVE_W9_EXACT_EDIT")
        );
        if index + 1 == rows.len() {
            assert!(
                row.3.is_empty(),
                "Mismatching/failed effect is not a fabricated receipt"
            );
        }
    }
    let db = publication_v3::raw(f);
    let claimed: i64 = db
        .query_row(
            "SELECT claimed FROM actions_write_attempts ORDER BY attempt_id DESC LIMIT 1",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        claimed, 1,
        "Actual sole journal claim, not a fixture-injected uncertain DTO"
    );
    reference_draft(f, kinds.len() as i64);
}

fn assert_tls(r: &RemoteFixture, receive: bool) {
    let requests = r.tls.requests().unwrap();
    assert!(!requests.is_empty() && requests.len() <= 16);
    assert!(
        requests.iter().any(|row| row.method == "GET"
            && row.path == "/example/ci.git/info/refs?service=git-upload-pack"),
        "Actual installed Git reached the owned TLS backend, not a fixture transport substitute"
    );
    assert_eq!(
        requests
            .iter()
            .filter(|row| row.method == "POST" && row.path == "/example/ci.git/git-receive-pack")
            .count(),
        usize::from(receive),
        "Exactly one actual receive-pack attempt, never an automatic retry"
    );
    assert!(requests.iter().all(|row| matches!(
        row.path,
        "/example/ci.git/info/refs?service=git-upload-pack"
            | "/example/ci.git/info/refs?service=git-receive-pack"
            | "/example/ci.git/git-upload-pack"
            | "/example/ci.git/git-receive-pack"
    ) && row.status == 200
        && row.request_bytes <= 1024 * 1024 + 16 * 1024
        && row.response_bytes <= 1024 * 1024));
}

fn cleanup_negative_v66(tls: WorkflowTlsRemoteV9) {
    let report = tls
        .finish()
        .expect("Actual TLS/CGI/thread/session cleanup, no leak waiver");
    assert!(!report.requests.is_empty());
    assert!(report.exchanges <= 16 && report.exchanges == report.requests.len());
    // This is a cumulative JOIN count, not the live-thread peak. The same
    // serial service has2 joined I/O workers per exchange and1 service join;
    // at most3 are live, preserving the unchanged6-thread fixture cap.
    assert_eq!(report.io_threads_joined, 1 + 2 * report.exchanges);
    assert!(!report.processes.is_empty());
    for process in report.processes {
        assert!(
            process.application_pid > 0
                && process.remaining.is_empty()
                && process.errors.is_empty()
        );
        assert!(
            process_identity(process.application_pid).unwrap().is_none(),
            "Exact positive fixture child must actually be absent after its owned wait"
        );
    }
}

fn preserved_local(
    r: &RemoteFixture,
    root: &std::path::Path,
    d: &str,
    git: &GitState,
    config: &[u8],
    git_config: &[u8],
    unrelated_sql: &SqlRowsV66,
) {
    assert_eq!(
        git_state(&r.fixture),
        *git,
        "Full post-D Git, including exact refs, cannot change on remote refusal"
    );
    assert_eq!(
        r.fixture
            .git(&["-C", root.to_str().unwrap(), "rev-parse", "HEAD"])
            .trim(),
        d
    );
    assert_eq!(
        r.fixture
            .git(&["-C", root.to_str().unwrap(), "status", "--porcelain=v1"]),
        ""
    );
    assert_eq!(
        fs::read(root.join(".github/workflows/ci.yml")).unwrap(),
        EDIT.as_bytes()
    );
    assert_eq!(
        fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(
        fs::read(r.fixture.repo().join(".git/config")).unwrap(),
        git_config
    );
    assert_eq!(unrelated_remote_sql_v66(&r.fixture), *unrelated_sql);
    assert_workflow_private_intents_v276(&r.fixture);
}

fn finish_retired(
    r: RemoteFixture,
    mut s: Session,
    mouse: bool,
    proof: LocalProofV66,
    receive: bool,
) {
    let LocalProofV66 {
        root,
        d,
        git,
        config,
        git_config,
        unrelated_sql,
    } = proof;
    // Independently validate every authorized own row FIRST, then pin all
    // raw SQL, including timestamps and journal rows, across resize and Quit.
    let terminal_sql = sql(&r.fixture);
    preserved_local(&r, &root, &d, &git, &config, &git_config, &unrelated_sql);
    let before = s.completed_cells_end();
    s.resize(80, 24);
    retired(&mut s, before);
    exact_remote(&s, &r.fixture, &r.steps);
    assert_eq!(sql(&r.fixture), terminal_sql);
    assert_tls(&r, receive);
    s.capture_completed_terminal_v1("task54-w9-negative-original-retired-no-resend");
    // Failure already removed the exact old Form; no invisible close/key
    // fallback, no private guessed history, and no extra source refresh.
    s.quit_and_assert_restored_terminal(mouse);
    exact_remote(&s, &r.fixture, &r.steps);
    assert_eq!(sql(&r.fixture), terminal_sql);
    preserved_local(&r, &root, &d, &git, &config, &git_config, &unrelated_sql);
    assert_tls(&r, receive);
    cleanup_negative_v66(r.tls);
}

fn refused_read(mouse: bool, malformed: Option<bool>) {
    let mut r = remote_fixture();
    // This is real server fixture setup BEFORE Source capture; no write
    // capability can come from the selected repository/branch name.
    if malformed.is_none() {
        bare_git(&r.tls, &["update-ref", "refs/heads/workflow/edit", &r.c]);
    }
    let bare_before = bare_git(&r.tls, &["show-ref"]);
    let config = fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap();
    let git_config = fs::read(r.fixture.repo().join(".git/config")).unwrap();
    let mut s = start(&r.fixture, mouse);
    let (root, d) = actual_d(&r, &mut s, mouse);
    let git = git_state(&r.fixture);
    let protected = unrelated_remote_sql_v66(&r.fixture);
    let mut read = remote_reads(&r.c, &d, false);
    if let Some(auth) = malformed {
        assert!(read[6].args.contains(&"--include".into()));
        read[6].stdout = if auth {
            b"HTTP/2 403 Forbidden\nContent-Length: 23\r\n\r\n{\"message\":\"Not Found\"}".to_vec()
        } else {
            b"HTTP/2 404 Not Found\nContent-Length: 4\r\n\r\nnull".to_vec()
        };
        read[6].stderr = b"PRIVATE_NOT_ABSENCE_DIAGNOSTIC".to_vec();
        read.truncate(7); // Real Git empty, actual base GET, exact raw refusal.
    } else {
        read.truncate(5); // Actual installed Git observes existing branch first.
    }
    r.steps.extend(read);
    r.fixture.scripted_gh(&r.steps); // D was observed before any future script.
    let before = s.completed_cells_end();
    action(&mut s, mouse, "[Preview next step]");
    s.wait_for_command_count(&r.fixture, r.steps.len());
    retired(&mut s, before);
    assert_fresh_remote_journal_v248(&r.fixture);
    aliases_v66(&r, &d, &[]);
    assert_eq!(exact_edit_draft(&mut s, &r.fixture), expected_edit_draft());
    no_post(&s, &r.fixture);
    assert_eq!(bare_git(&r.tls, &["show-ref"]), bare_before);
    if malformed.is_none() {
        assert_eq!(bare_branch(&r).trim(), r.c);
    } else {
        assert_eq!(bare_branch(&r), "");
    }
    finish_retired(
        r,
        s,
        mouse,
        LocalProofV66 {
            root,
            d,
            git,
            config,
            git_config,
            unrelated_sql: protected,
        },
        false,
    );
}

fn remote_drift(mouse: bool) {
    let mut r = remote_fixture();
    let mut s = start(&r.fixture, mouse);
    let (root, d) = actual_d(&r, &mut s, mouse);
    let git = git_state(&r.fixture);
    let config = fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap();
    let old_config = fs::read(r.fixture.repo().join(".git/config")).unwrap();
    let protected = unrelated_remote_sql_v66(&r.fixture);
    let bare_before = bare_git(&r.tls, &["show-ref"]);
    r.steps.extend(remote_reads(&r.c, &d, false));
    // Same original Preview, then actual local config changes. It is NOT
    // retroactively granted a new association by a name/URL or fake DTO.
    let confirm_prefix = remote_reads(&r.c, &d, false)[..5].to_vec();
    r.steps.extend(confirm_prefix);
    r.fixture.scripted_gh(&r.steps);
    let mut count = 109;
    preview(&mut s, &r, mouse, &mut count, false, &d);
    assert_eq!(count, 123);
    let changed_url = "https://ghe.example/example/other.git";
    r.fixture.git(&[
        "-C",
        root.to_str().unwrap(),
        "config",
        "--local",
        "--replace-all",
        &format!("remote.{REMOTE}.url"),
        changed_url,
    ]);
    let expected_config = String::from_utf8(old_config)
        .unwrap()
        .replace(r.tls.canonical_url(), changed_url)
        .into_bytes();
    assert_eq!(
        fs::read(r.fixture.repo().join(".git/config")).unwrap(),
        expected_config
    );
    let before = s.completed_cells_end();
    action(&mut s, mouse, "[Confirm step]");
    s.wait_for_command_count(&r.fixture, 128);
    retired(&mut s, before);
    assert_fresh_remote_journal_v248(&r.fixture);
    aliases_v66(&r, &d, &[]);
    assert_eq!(exact_edit_draft(&mut s, &r.fixture), expected_edit_draft());
    no_post(&s, &r.fixture);
    assert_eq!(bare_git(&r.tls, &["show-ref"]), bare_before);
    assert_eq!(bare_branch(&r), "");
    finish_retired(
        r,
        s,
        mouse,
        LocalProofV66 {
            root,
            d,
            git,
            config,
            git_config: expected_config,
            unrelated_sql: protected,
        },
        false,
    );
}

fn receive_rejected(mouse: bool) {
    let mut r = remote_fixture();
    let hook = r.tls.bare_repository().join("hooks/pre-receive");
    fs::write(
        &hook,
        b"#!/bin/sh\nprintf '%s\\n' 'PRIVATE_SERVER_HOOK_REJECTED' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    let bare_before = bare_git(&r.tls, &["show-ref"]);
    let mut s = start(&r.fixture, mouse);
    let (root, d) = actual_d(&r, &mut s, mouse);
    let git = git_state(&r.fixture);
    let config = fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap();
    let git_config = fs::read(r.fixture.repo().join(".git/config")).unwrap();
    let protected = unrelated_remote_sql_v66(&r.fixture);
    r.steps.extend(remote_reads(&r.c, &d, false));
    let mut confirm = remote_reads(&r.c, &d, false);
    let mut missing = remote_get(
        &format!("repos/{CURRENT}/git/ref/heads/workflow%2Fedit"),
        json!({"message":"Not Found"}),
    );
    missing.status = 1;
    missing.stderr = b"gh: Not Found (HTTP 404)\n".to_vec();
    confirm.push(missing); // Actual code observes paid REST after failed push/read.
    assert_eq!(confirm.len(), 15);
    r.steps.extend(confirm);
    r.fixture.scripted_gh(&r.steps);
    let mut count = 109;
    preview(&mut s, &r, mouse, &mut count, false, &d);
    let before = s.completed_cells_end();
    action(&mut s, mouse, "[Confirm step]");
    s.wait_for_command_count(&r.fixture, 138);
    retired(&mut s, before);
    uncertain(&r.fixture, &["publish_workflow_branch"]);
    aliases_v66(&r, &d, &["uncertain"]);
    no_post(&s, &r.fixture);
    assert_eq!(
        bare_git(&r.tls, &["show-ref"]),
        bare_before,
        "Real receive hook refused; no guessed branch effect"
    );
    assert_eq!(bare_branch(&r), "");
    finish_retired(
        r,
        s,
        mouse,
        LocalProofV66 {
            root,
            d,
            git,
            config,
            git_config,
            unrelated_sql: protected,
        },
        true,
    );
}

fn canonical_mismatch(mouse: bool) {
    let mut r = remote_fixture();
    let mut s = start(&r.fixture, mouse);
    let (root, d) = actual_d(&r, &mut s, mouse);
    let git = git_state(&r.fixture);
    let config = fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap();
    let git_config = fs::read(r.fixture.repo().join(".git/config")).unwrap();
    let protected = unrelated_remote_sql_v66(&r.fixture);
    r.steps.extend(remote_reads(&r.c, &d, false));
    r.steps.extend(remote_confirm(&r.c, &d, false));
    r.steps.extend(remote_reads(&r.c, &d, true));
    let mut confirm_steps = remote_confirm(&r.c, &d, true);
    let mut wrong = canonical_pr(&r.c, &d);
    wrong["head"]["sha"] = json!("e".repeat(40));
    confirm_steps[16].stdout = serde_json::to_vec(&wrong).unwrap();
    confirm_steps.truncate(17); // No fabricated final UID/receipt after canonical mismatch.
    r.steps.extend(confirm_steps);
    r.fixture.scripted_gh(&r.steps);
    let mut count = 109;
    preview(&mut s, &r, mouse, &mut count, false, &d);
    confirm(&mut s, &r, mouse, &mut count, false);
    assert_eq!(bare_branch(&r).trim(), d);
    let first = journal(&r.fixture);
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].1, "confirmed");
    preview(&mut s, &r, mouse, &mut count, true, &d);
    let before = s.completed_cells_end();
    action(&mut s, mouse, "[Confirm step]");
    s.wait_for_command_count(&r.fixture, 171);
    retired(&mut s, before);
    uncertain(
        &r.fixture,
        &["publish_workflow_branch", "create_workflow_pr"],
    );
    aliases_v66(&r, &d, &["confirmed", "uncertain"]);
    assert_eq!(
        journal(&r.fixture)[0],
        first[0],
        "Later PR mismatch cannot rewrite confirmed branch receipt"
    );
    let posts = commands(&s, &r.fixture)
        .into_iter()
        .filter(|row| {
            row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        posts.len(),
        1,
        "One exact canonical PR POST, never a retry/guessed secondPR"
    );
    assert_eq!(posts[0]["args"][1], format!("repos/{CURRENT}/pulls"));
    assert_eq!(bare_branch(&r).trim(), d);
    assert_eq!(
        bare_git(&r.tls, &["rev-parse", "refs/heads/main"]).trim(),
        r.c
    );
    finish_retired(
        r,
        s,
        mouse,
        LocalProofV66 {
            root,
            d,
            git,
            config,
            git_config,
            unrelated_sql: protected,
        },
        true,
    );
}

struct StoppedPrewrite(ProcessIdentity);
impl StoppedPrewrite {
    fn capture(s: &Session, child: ProcessIdentity) -> Self {
        let leader = process_identity(s.owned_application_pid())
            .unwrap()
            .unwrap();
        assert!(child.pid > 0 && child.pid != leader.pid && !child.zombie);
        assert_eq!(
            (child.parent, child.uid, child.session),
            (leader.pid, leader.uid, leader.session)
        );
        assert!(child.birth >= leader.birth);
        assert_eq!(process_identity(child.pid).unwrap().as_ref(), Some(&child));
        // SAFETY: exact immediately revalidated positive original UID/SID/birth
        // child only, never a name, process group or harness session.
        assert_eq!(unsafe { libc::kill(child.pid, libc::SIGSTOP) }, 0);
        Self(child)
    }
}
impl Drop for StoppedPrewrite {
    fn drop(&mut self) {
        if let Ok(Some(current)) = process_identity(self.0.pid) {
            if !current.zombie
                && (current.uid, current.session, current.birth)
                    == (self.0.uid, self.0.session, self.0.birth)
            {
                // SAFETY: resumes only this exact still-owned stopped child.
                assert_eq!(unsafe { libc::kill(current.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}

fn prewrite_cancel(mouse: bool) {
    let mut r = remote_fixture();
    let mut s = start(&r.fixture, mouse);
    let (root, d) = actual_d(&r, &mut s, mouse);
    let git = git_state(&r.fixture);
    let config = fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap();
    let git_config = fs::read(r.fixture.repo().join(".git/config")).unwrap();
    let protected = unrelated_remote_sql_v66(&r.fixture);
    let bare_before = bare_git(&r.tls, &["show-ref"]);
    r.steps.extend(remote_reads(&r.c, &d, false));
    let mut confirm = remote_reads(&r.c, &d, false);
    assert_eq!(
        confirm[13]
            .args
            .iter()
            .filter(|arg| arg.as_str() == "user")
            .count(),
        1
    );
    confirm[13].delay_ms = 5000;
    r.steps.extend(confirm);
    r.fixture.scripted_gh(&r.steps);
    let mut count = 109;
    preview(&mut s, &r, mouse, &mut count, false, &d);
    let before = s.completed_cells_end();
    action(&mut s, mouse, "[Confirm step]");
    let child = entered(&mut s, &r.fixture, 137);
    let stopped = StoppedPrewrite::capture(&s, child.clone());
    let until = Instant::now() + Duration::from_secs(8);
    let point = loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > before {
            if let Some(point) = s.completed_cells_screen().and_then(|frame| {
                crate::support::completed_navigation_controls_v1::owned_action_position(
                    frame,
                    FORM,
                    "[Esc Cancel]",
                    &[],
                )
            }) {
                break point;
            }
        }
        assert!(
            Instant::now() < until,
            "Actual original busy Form must expose SAME complete Cancel control\n{}",
            s.screen()
        );
    };
    let rows = journal(&r.fixture);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "\"publish_workflow_branch\"");
    assert_eq!(rows[0].1, "pending");
    assert!(rows[0].3.is_empty());
    aliases_v66(&r, &d, &["pending"]);
    let reference = reference_draft(&r.fixture, 1);
    let before = s.completed_cells_end();
    if mouse {
        s.click(point.0, point.1);
    } else {
        s.send_key(b"\x1b");
    }
    drop(stopped);
    gone(&mut s, &child);
    // Explicit cancellation is not account loss. The same original Form
    // remains inspect-only with busy retired and Confirm disabled. This is
    // neither a renewed approval nor a fabricated remote result Form.
    cells(&mut s, before, &["[Keep draft / Close]"]);
    let frame = s.completed_cells_screen().unwrap();
    let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
        frame,
        FORM,
        "[Confirm step]",
        &[],
    )
    .unwrap();
    assert!(
        frame.cell(y, x).is_some_and(|cell| !cell.underline()),
        "Canceled original cannot retain an enabled confirmation"
    );
    assert_eq!(
        journal(&r.fixture),
        rows,
        "Known not-invoked prewrite cancellation retains pending barrier, no automatic ACK"
    );
    assert_eq!(reference_draft(&r.fixture, 1), reference);
    no_post(&s, &r.fixture);
    assert_eq!(bare_git(&r.tls, &["show-ref"]), bare_before);
    assert_eq!(bare_branch(&r), "");
    preserved_local(&r, &root, &d, &git, &config, &git_config, &protected);
    let stable = sql(&r.fixture);
    exact_remote(&s, &r.fixture, &r.steps);
    assert_tls(&r, false);
    // This actual disabled action remains reachable by both input families,
    // but inspection-required state cannot create a new phase or resend.
    let before = s.completed_cells_end();
    action(&mut s, mouse, "[Confirm step]");
    cells(&mut s, before, &["[Keep draft / Close]"]);
    exact_remote(&s, &r.fixture, &r.steps);
    assert_eq!(sql(&r.fixture), stable);
    close(&mut s, mouse);
    exact_remote(&s, &r.fixture, &r.steps);
    assert_eq!(sql(&r.fixture), stable);
    s.quit_and_assert_restored_terminal(mouse);
    exact_remote(&s, &r.fixture, &r.steps);
    assert_eq!(sql(&r.fixture), stable);
    assert_eq!(journal(&r.fixture), rows);
    assert_eq!(reference_draft(&r.fixture, 1), reference);
    preserved_local(&r, &root, &d, &git, &config, &git_config, &protected);
    cleanup_negative_v66(r.tls);
}

#[test]
fn keyboard_task54_workflow_edit_v9_existing_remote_branch_refuses_without_push_or_claim() {
    refused_read(false, None)
}
#[test]
fn mouse_task54_workflow_edit_v9_existing_remote_branch_refuses_without_push_or_claim() {
    refused_read(true, None)
}
#[test]
fn keyboard_task54_workflow_edit_v9_raw404_malformed_and_auth_are_not_absence() {
    for auth in [false, true] {
        refused_read(false, Some(auth));
    }
}
#[test]
fn mouse_task54_workflow_edit_v9_raw404_malformed_and_auth_are_not_absence() {
    for auth in [false, true] {
        refused_read(true, Some(auth));
    }
}
#[test]
fn keyboard_task54_workflow_edit_v9_configured_remote_drift_refuses_original_preview() {
    remote_drift(false)
}
#[test]
fn mouse_task54_workflow_edit_v9_configured_remote_drift_refuses_original_preview() {
    remote_drift(true)
}
#[test]
fn keyboard_task54_workflow_edit_v9_actual_receive_hook_refusal_retains_uncertain_without_retry() {
    receive_rejected(false)
}
#[test]
fn mouse_task54_workflow_edit_v9_actual_receive_hook_refusal_retains_uncertain_without_retry() {
    receive_rejected(true)
}
#[test]
fn keyboard_task54_workflow_edit_v9_created201_canonical_mismatch_stays_protected() {
    canonical_mismatch(false)
}
#[test]
fn mouse_task54_workflow_edit_v9_created201_canonical_mismatch_stays_protected() {
    canonical_mismatch(true)
}
#[test]
fn keyboard_task54_workflow_edit_v9_held_prewrite_cancel_reaps_exact_child_without_effect() {
    prewrite_cancel(false)
}
#[test]
fn mouse_task54_workflow_edit_v9_held_prewrite_cancel_reaps_exact_child_without_effect() {
    prewrite_cancel(true)
}
