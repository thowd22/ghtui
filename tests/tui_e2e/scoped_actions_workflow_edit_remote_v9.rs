//! UNRUN actual binary/real TLS Git/strict offline gh remote continuations.
//! All fixture routing/configuration precedes genuine source capture.
use super::*;
use crate::support::workflow_tls_v9::WorkflowTlsRemoteV9;
use ghtui::{
    git::RawPath,
    search::{
        corpus::LocalSource,
        managed::{CorpusOwnership, CorpusRepository},
    },
};
use sha2::{Digest, Sha256};
#[path = "scoped_actions_workflow_edit_cleanup_v9.rs"]
mod cleanup_v9;
#[path = "scoped_actions_workflow_edit_remote_history_v9.rs"]
mod history_v9;
#[path = "scoped_actions_workflow_edit_remote_negative_v9.rs"]
mod negative_v9;
const REMOTE_HOST: &str = "ghe.example";
const REMOTE: &str = "workflow-head";
const BRANCH: &str = "workflow/edit";
const TITLE: &str = "PRIVATE_NATIVE_WORKFLOW_PR_TITLE";
const PR_BODY: &str = "PRIVATE_NATIVE_WORKFLOW_PR_BODY\nExact second line";

fn host_step(mut step: GhStep) -> GhStep {
    for arg in &mut step.args {
        *arg = arg.replace(HOST, REMOTE_HOST);
    }
    step.stdout = String::from_utf8(step.stdout)
        .unwrap()
        .replace(HOST, REMOTE_HOST)
        .into_bytes();
    step.stderr = String::from_utf8(step.stderr)
        .unwrap()
        .replace(HOST, REMOTE_HOST)
        .into_bytes();
    step
}
fn remote_get(endpoint: &str, value: Value) -> GhStep {
    host_step(get(endpoint, value))
}
fn numeric_repository() -> Value {
    let mut value = current_repository();
    value["permissions"]["push"] = json!(true);
    value["clone_url"] = json!(format!("https://{REMOTE_HOST}/{CURRENT}.git"));
    value["ssh_url"] = json!(format!("git@{REMOTE_HOST}:{CURRENT}.git"));
    value
}
fn present(branch: &str, sha: &str) -> GhStep {
    remote_get(
        &format!(
            "repos/{CURRENT}/git/ref/heads/{}",
            branch.replace('/', "%2F")
        ),
        json!({"ref":format!("refs/heads/{branch}"),"object":{"type":"commit","sha":sha}}),
    )
}
fn absent() -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{CURRENT}/git/ref/heads/workflow%2Fedit"),
            "--hostname",
            REMOTE_HOST,
            "--method",
            "GET",
            "--include",
        ],
        "HTTP/2 404 Not Found\nContent-Length: 23\r\n\r\n{\"message\":\"Not Found\"}",
    );
    step.status = 1;
    step.stderr = b"gh: Not Found (HTTP 404)\n".to_vec();
    step
}
fn remote_reads(c: &str, d: &str, create_pr: bool) -> Vec<GhStep> {
    let mut out = principal(42).into_iter().map(host_step).collect::<Vec<_>>();
    out.push(remote_get(&format!("repos/{FIRST}"), current_repository()));
    out.push(host_step(wf()));
    out.push(remote_get("repositories/10000", numeric_repository()));
    out.push(present("main", c));
    out.push(if create_pr {
        present(BRANCH, d)
    } else {
        absent()
    });
    if create_pr {
        out.push(remote_get(&format!("repos/{CURRENT}/pulls?state=open&head=example%3Aworkflow%2Fedit&base=main&per_page=100&page=1"),json!([])));
    }
    out.push(present("main", c));
    out.push(if create_pr {
        present(BRANCH, d)
    } else {
        absent()
    });
    out.push(remote_get("repositories/10000", numeric_repository()));
    out.push(host_step(wf()));
    out.push(remote_get(&format!("repos/{FIRST}"), current_repository()));
    out.extend(principal(42).into_iter().map(host_step));
    assert_eq!(out.len(), if create_pr { 15 } else { 14 });
    out
}
fn canonical_pr(c: &str, d: &str) -> Value {
    json!({"id":9901,"node_id":"PR_actual_workflow_9901","number":91,
        "title":TITLE,"body":PR_BODY,"state":"open","draft":false,
        "html_url":format!("https://{REMOTE_HOST}/{CURRENT}/pull/91"),
        "user":{"id":42,"login":"octocat","type":"User"},
        "base":{"ref":"main","sha":c,"repo":numeric_repository()},
        "head":{"ref":BRANCH,"sha":d,"repo":numeric_repository()},
        "labels":[],"assignees":[],"created_at":"2026-10-16T00:00:00Z","updated_at":"2026-10-16T00:00:00Z"})
}
fn pr_post(c: &str, d: &str) -> GhStep {
    let body = canonical_pr(c, d).to_string();
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{CURRENT}/pulls"),
            "--hostname",
            REMOTE_HOST,
            "--method",
            "POST",
            "--include",
            "--input",
            "-",
        ],
        &format!(
            "HTTP/2 201 Created\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ),
    );
    step.stdin=serde_json::to_vec(&json!({"head":"example:workflow/edit","head_repo":"ci","base":"main","title":TITLE,"body":PR_BODY,"draft":false})).unwrap();
    step
}
fn remote_confirm(c: &str, d: &str, pr: bool) -> Vec<GhStep> {
    let mut out = remote_reads(c, d, pr);
    if pr {
        out.push(pr_post(c, d));
        out.push(remote_get(
            &format!("repos/{CURRENT}/pulls/91"),
            canonical_pr(c, d),
        ));
    } else {
        out.push(present(BRANCH, d));
    }
    out.push(remote_get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(out.len(), if pr { 18 } else { 16 });
    out
}
struct RemoteFixture {
    fixture: Fixture,
    steps: Vec<GhStep>,
    c: String,
    tls: WorkflowTlsRemoteV9,
}
fn remote_fixture() -> RemoteFixture {
    let (f, steps, c) = fixture(|_| {
        (0..4)
            .flat_map(|_| [phase(false), phase(true)].concat())
            .collect()
    });
    // This is fixture configuration BEFORE launch/source capture, not a URL
    // rewrite or an association retrofitted after a publication approval.
    let tls = WorkflowTlsRemoteV9::start(&f.root, &f.repo())
        .expect("Actual owned TLS Git fixture, no protocol substitute");
    tls.configure_fixture_repository(&f.repo(), REMOTE).unwrap();
    assert_eq!(
        tls.canonical_url(),
        format!("https://{REMOTE_HOST}/{CURRENT}.git")
    );
    let path = f.root.join("config/ghtui/config.toml");
    let mut config = ghtui::config::Config::load(&path).unwrap();
    assert_eq!(config.host, HOST);
    config.host = REMOTE_HOST.into();
    config.save(&path).unwrap();
    let scope = Scope::github(REMOTE_HOST, Some(42)).unwrap();
    let mapping = CorpusRepository {
        scope: scope.clone(),
        repository: FIRST.into(),
        directory: RawPath(f.repo().as_os_str().as_bytes().to_vec()),
        source: LocalSource::Revision(c.clone()),
        ownership: CorpusOwnership::Existing,
        active: true,
        saved_commit: Some(c.clone()),
        disk_bytes: 0,
        disk_complete: false,
        clone_source: None,
    };
    Cache::open(&database(&f))
        .unwrap()
        .save_view(
            &scope,
            &format!("corpus-mapping-v1:{:x}", Sha256::digest(FIRST.as_bytes())),
            &serde_json::to_value(mapping).unwrap(),
        )
        .unwrap();
    let steps = steps.into_iter().map(host_step).collect::<Vec<_>>();
    f.scripted_gh(&steps);
    RemoteFixture {
        fixture: f,
        steps,
        c,
        tls,
    }
}
fn remote_sql(f: &Fixture) -> Vec<(String, Vec<Vec<rusqlite::types::Value>>)> {
    let mut rows = unrelated(f);
    rows.retain(|(name, _)| {
        !matches!(
            name.as_str(),
            "actions_write_attempts" | "actions_write_generations" | "actions_write_resources"
        )
    });
    rows
}
fn positive_aliases(r: &RemoteFixture, attempts: usize) {
    if attempts == 0 {
        assert_fresh_remote_journal_v248(&r.fixture);
        return;
    }
    // Independent explicit expected current/original/numeric alias set, never
    // derived by the product canonicalizer whose result is under test.
    let mut aliases = [
        format!("{REMOTE_HOST}/{CURRENT}/workflow/7"),
        format!("{REMOTE_HOST}/{FIRST}/workflow/7"),
        format!("{REMOTE_HOST}/repository-id/10000/actions/workflow/7"),
        format!("{REMOTE_HOST}/{CURRENT}/branch/{BRANCH}"),
        format!("{REMOTE_HOST}/{FIRST}/branch/{BRANCH}"),
        format!("{REMOTE_HOST}/repository-id/10000/git/branch/{BRANCH}"),
    ];
    aliases.sort();
    let db = publication_v3::raw(&r.fixture);
    let resources=db.prepare("SELECT host,principal,resource_key,attempt_id FROM actions_write_resources ORDER BY attempt_id,resource_key").unwrap()
        .query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?))).unwrap()
        .map(Result::unwrap).collect::<Vec<_>>();
    let expected = (1..=attempts)
        .flat_map(|id| {
            aliases.iter().map(move |key| {
                (
                    REMOTE_HOST.to_owned(),
                    "user-id:42".to_owned(),
                    key.clone(),
                    id as i64,
                )
            })
        })
        .collect::<Vec<_>>();
    assert_eq!(
        resources, expected,
        "All and only six actual scoped aliases per real remote attempt"
    );
    let generations=db.prepare("SELECT host,principal,resource_key,generation FROM actions_write_generations ORDER BY resource_key").unwrap()
        .query_map([],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,String>(2)?,row.get::<_,i64>(3)?))).unwrap()
        .map(Result::unwrap).collect::<Vec<_>>();
    let expected = if attempts == 0 {
        Vec::new()
    } else {
        aliases
            .iter()
            .map(|key| {
                (
                    REMOTE_HOST.to_owned(),
                    "user-id:42".to_owned(),
                    key.clone(),
                    3 * attempts as i64,
                )
            })
            .collect()
    };
    assert_eq!(
        generations, expected,
        "Exact begin/claim/confirmed advances, no hidden alias writes"
    );
}
fn journal(f: &Fixture) -> Vec<(String, String, String, String)> {
    assert!(
        remote_journal_schema_v248(f),
        "An actual remote attempt must have all three exact journal tables"
    );
    let db = publication_v3::raw(f);
    let mut query=db.prepare("SELECT kind_json,state,context_json,COALESCE(receipt_json,'') FROM actions_write_attempts ORDER BY attempt_id").unwrap();
    query
        .query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}
fn remote_journal_schema_v248(f: &Fixture) -> bool {
    let db = publication_v3::raw(f);
    assert_eq!(db.path(), database(f).to_str(),"Read the actual application's configured fixture database, never a second created database");
    journal_schema_names_v248(&db)
        .expect("Actual journal schema/SQL must be complete or genuinely all absent")
}
fn journal_schema_names_v248(db: &rusqlite::Connection) -> Result<bool, String> {
    let mut query = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name IN ('actions_write_attempts','actions_write_generations','actions_write_resources') ORDER BY name")
        .map_err(|error|error.to_string())?;
    let names = query
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    if names.is_empty() {
        Ok(false)
    } else if names
        == [
            "actions_write_attempts",
            "actions_write_generations",
            "actions_write_resources",
        ]
    {
        Ok(true)
    } else {
        Err("Partial journal schema is not fresh absence".into())
    }
}
fn assert_fresh_remote_journal_v248(f: &Fixture) {
    assert!(!remote_journal_schema_v248(f),"Zero expected remote attempts means genuine all-three schema absence before the first publication, not silently empty rows or swallowed SQL errors");
}
fn bare_git(tls: &WorkflowTlsRemoteV9, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .arg("--git-dir")
        .arg(tls.bare_repository())
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "Independent exact bare Git observation: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}
fn exact_remote(s: &Session, f: &Fixture, steps: &[GhStep]) {
    let actual = commands(s, f);
    assert_eq!(
        actual.len(),
        steps.len(),
        "No hidden source refresh, fallback, retry or resend"
    );
    for (row, step) in actual.iter().zip(steps) {
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["matched"], true);
        let expected = step
            .stdin
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        assert_eq!(
            row["stdin_hex"], expected,
            "Exact stdin, not a title/body substring or saved-ID permission"
        );
    }
}
fn actual_local_commit(
    r: &RemoteFixture,
    s: &mut Session,
    mouse: bool,
) -> (std::path::PathBuf, String) {
    initial(s, &r.fixture, mouse, &r.c);
    replace(s, mouse, 1, EDIT);
    replace(s, mouse, 4, TITLE);
    replace(s, mouse, 5, PR_BODY);
    let mut count = 41;
    for (before, after) in [
        ("NEW branch", "Actual isolated worktree/NEW branch observed"),
        (
            "SAVE EXACT WORKFLOW FILE",
            "Actual exact file Save observed",
        ),
        ("Stage selected file", "Actual exact path staged"),
        (
            "Commit staged changes",
            "Actual reviewed C..D commit observed",
        ),
    ] {
        local_step(s, &r.fixture, mouse, &mut count, &[before], &[after]);
    }
    assert_eq!(count, 109);
    let root = isolated_root(&r.fixture);
    let d = r
        .fixture
        .git(&["-C", root.to_str().unwrap(), "rev-parse", "HEAD"])
        .trim()
        .to_owned();
    assert_ne!(d, r.c);
    assert_eq!(
        r.fixture
            .git(&["-C", root.to_str().unwrap(), "rev-parse", "HEAD^"])
            .trim(),
        r.c
    );
    assert_eq!(
        r.fixture.git(&[
            "-C",
            root.to_str().unwrap(),
            "show",
            "HEAD:.github/workflows/ci.yml"
        ]),
        EDIT
    );
    assert_eq!(
        r.fixture
            .git(&["-C", root.to_str().unwrap(), "status", "--porcelain=v1"]),
        ""
    );
    field(s, mouse, 6);
    assert!(
        body(s).unwrap().contains("[Space Next]"),
        "Actual indexed head member remains keyboard/mouse accessible"
    );
    let after = s.completed_cells_end();
    if mouse {
        let (x, y) = s.wait_for_owned_completed_action(FORM, "[Space Next]", &[]);
        s.click(x, y);
    } else {
        s.send_key(b" ");
    }
    cells(s, after, &["example/repo0000", "repository ID 10000"]);
    replace(s, mouse, 7, REMOTE);
    replace(s, mouse, 8, "main");
    (root, d)
}
fn remote_cells(s: &mut Session, after: usize, words: &[&str]) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(1));
        if s.completed_cells_end() > after
            && body(s).is_some_and(|body| {
                labels(&body).iter().all(|label| body.contains(label))
                    && !body.contains("[Esc Cancel]")
                    && words
                        .iter()
                        .all(|word| crate::owned_form_prose_v120::adjacent_literal(&body, word))
            })
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "NEW complete actual remote phase footer/full facts {words:?}\n{}",
            s.screen()
        );
    }
}
fn preview(s: &mut Session, r: &RemoteFixture, mouse: bool, count: &mut usize, pr: bool, d: &str) {
    let after = s.completed_cells_end();
    action(s, mouse, "[Preview next step]");
    *count += if pr { 15 } else { 14 };
    s.wait_for_command_count(&r.fixture, *count);
    remote_cells(
        s,
        after,
        &[
            if pr {
                "Preview exact workflow pull request"
            } else {
                "Preview create-only workflow branch push"
            },
            &r.c,
            d,
            "main",
            BRANCH,
        ],
    );
}
fn confirm(s: &mut Session, r: &RemoteFixture, mouse: bool, count: &mut usize, pr: bool) {
    let after = s.completed_cells_end();
    action(s, mouse, "[Confirm step]");
    *count += if pr { 18 } else { 16 };
    s.wait_for_command_count(&r.fixture, *count);
    remote_cells(
        s,
        after,
        &[if pr {
            "Actual canonical PR #91 observed after HTTP201"
        } else {
            "Exact newly published branch D observed"
        }],
    );
    if pr {
        let before = (
            sql(&r.fixture),
            git_state(&r.fixture),
            commands(s, &r.fixture),
        );
        let after = s.completed_cells_end();
        s.send_key(b"\x10"); // Actual Preview shortcut cannot renew a result.
        remote_cells(s, after, &["read only", "never renew Preview or resend"]);
        let actual = body(s).unwrap();
        assert!(!actual.contains("[Confirm step]") && !actual.contains("[Preview next step]"));
        assert!(actual.contains("Actual canonical PR #91 observed after HTTP201"));
        assert_eq!((sql(&r.fixture), git_state(&r.fixture), commands(s, &r.fixture)),before,"Read-only result display is not approval, no marker/read/request may be queued by Preview");
    }
}
fn cleanup(tls: WorkflowTlsRemoteV9) {
    let report = tls
        .finish()
        .expect("Actual bounded TLS/backend/I/O cleanup must complete, never a leak waiver");
    assert!(report.exchanges <= 16 && report.exchanges == report.requests.len());
    // Three is the peak simultaneous I/O count, NOT the accumulated number
    // joined across real independent Git HTTP exchanges.
    assert_eq!(report.io_threads_joined, 1 + 2 * report.exchanges);
    for process in report.processes {
        assert!(
            process.application_pid > 0
                && process.remaining.is_empty()
                && process.errors.is_empty(),
            "Exact positive owned process cleanup: {process:?}"
        );
    }
}
fn positive(mouse: bool) {
    positive_environment_v9(mouse, false)
}
fn positive_environment_v9(mouse: bool, startup_case: bool) {
    use std::os::unix::{ffi::OsStringExt, fs::PermissionsExt};
    let mut r = remote_fixture();
    let f = &r.fixture;
    if !startup_case {
        let before = sql(f);
        // A separate, real isolated fixture DB proves a partial schema is
        // not accepted as fresh absence. SQL errors propagate from the helper.
        // It is never opened by the application or used as source authority.
        let probe =
            rusqlite::Connection::open(f.root.join("journal-oracle-contract.sqlite3")).unwrap();
        assert!(!journal_schema_names_v248(&probe).unwrap());
        probe
            .execute("CREATE TABLE actions_write_attempts (canary INTEGER)", [])
            .unwrap();
        assert!(journal_schema_names_v248(&probe).is_err());
        // The actual product DB is still independently all-three absent.
        assert_fresh_remote_journal_v248(f);
        assert_eq!(sql(f), before);
    }
    // Isolated process input BEFORE App/Source capture. No global env setter,
    // product TLS/config rewrite or test-only snapshot factory is involved.
    let startup_value =
        std::ffi::OsString::from_vec(b"PRIVATE_BOOTSTRAP_W9_\xff_\xc3_\n;$(not_executed)".to_vec());
    let hook_output = f.root.join("actual-workflow-hook-environment.bin");
    if startup_case {
        let hooks = f.root.join("workflow-startup-hooks");
        fs::create_dir(&hooks).unwrap();
        fs::set_permissions(&hooks, fs::Permissions::from_mode(0o700)).unwrap();
        let quoted = hook_output.to_str().unwrap().replace('\'', "'\\''");
        let hook = hooks.join("pre-commit");
        fs::write(&hook, format!("#!/bin/sh\nprintf '%s\\000%s\\000%s' \"$GHTUI_W9_PRIVATE_STARTUP_ENV\" \"$GIT_EDITOR\" \"$GIT_TERMINAL_PROMPT\" > '{quoted}'\n")).unwrap();
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
        f.git(&["config", "core.hooksPath", hooks.to_str().unwrap()]);
    }
    let original_git = git_state(f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let git_config = fs::read(f.repo().join(".git/config")).unwrap();
    let mut s = if startup_case {
        use std::ffi::OsStr;
        let trace = f.root.join("catalogue-git-trace.jsonl");
        let mut env = vec![
            (OsStr::new("GIT_TRACE2_EVENT"), trace.as_os_str()),
            (
                OsStr::new("GHTUI_W9_PRIVATE_STARTUP_ENV"),
                startup_value.as_os_str(),
            ),
            (
                OsStr::new("GIT_EDITOR"),
                OsStr::new("PRIVATE_BOOTSTRAP_EDITOR_MUST_BE_OVERRIDDEN"),
            ),
            (
                OsStr::new("GIT_TERMINAL_PROMPT"),
                OsStr::new("PRIVATE_BOOTSTRAP_PROMPT_MUST_BE_OVERRIDDEN"),
            ),
        ];
        if mouse {
            env.push((OsStr::new("NO_COLOR"), OsStr::new("1")));
        }
        let mut session = Session::launch_binary_os_env(
            Path::new(env!("CARGO_BIN_EXE_ghtui")),
            f,
            if mouse { 80 } else { 120 },
            if mouse { 24 } else { 40 },
            &env,
        );
        session.wait_for_screen("Recent commits");
        session.assert_focused_view("Repository");
        let tab = session.locate("4 Actions").unwrap();
        open_catalogue(&mut session, f, mouse, 0);
        discover_with_captured_actions_tab(&mut session, f, mouse, tab);
        filter(
            &mut session,
            mouse,
            "repo0000",
            FIRST,
            "Repository ID: 10000",
        );
        choose(&mut session, mouse, "[Open Actions]", b"A");
        session.wait_for_command_count(f, 14);
        session.wait_for_overlay_close("Repositories • explicit discovery");
        actions_ready(&mut session);
        session
    } else {
        start(f, mouse)
    };
    let (root, d) = actual_local_commit(&r, &mut s, mouse);
    if startup_case {
        let mut expected = startup_value.as_os_str().as_bytes().to_vec();
        expected.push(0);
        // Installed Git itself sets ':' in non-editor commit hooks. The
        // runner's fixed GIT_EDITOR=false is independently asserted at its
        // actual spec boundary; no hook/editor configuration is bypassed.
        expected.push(b':');
        expected.push(0);
        expected.push(b'0');
        assert!(fs::read(&hook_output).unwrap() == expected,
            "Actual installed Git hook must retain complete non-UTF8 startup bytes and fixed override precedence; values redacted");
        assert!(
            !s.screen().contains("PRIVATE_BOOTSTRAP"),
            "Private environment escaped to UI"
        );
    }
    let git = git_state(&r.fixture);
    // The genuine isolated local Commit deliberately creates ONE shared ref;
    // it may not change the original user's checkout/index/dirty bytes.
    assert_eq!(git.head, original_git.head);
    assert_eq!(git.index, original_git.index);
    assert_eq!(git.status, original_git.status);
    assert_eq!(git.staged_diff, original_git.staged_diff);
    assert_eq!(git.working_diff, original_git.working_diff);
    assert_eq!(git.dirty, original_git.dirty);
    assert_eq!(git.staged, original_git.staged);
    assert_eq!(git.untracked, original_git.untracked);
    let mut expected_refs = original_git
        .refs
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    expected_refs.push(format!("{d} refs/heads/{BRANCH}"));
    expected_refs.sort();
    let mut actual_refs = git.refs.lines().map(str::to_owned).collect::<Vec<_>>();
    actual_refs.sort();
    assert_eq!(
        actual_refs, expected_refs,
        "Only the actual reviewed local branch D was added"
    );
    let sql = remote_sql(&r.fixture);
    positive_aliases(&r, 0);
    r.steps.extend(remote_reads(&r.c, &d, false));
    r.steps.extend(remote_confirm(&r.c, &d, false));
    r.steps.extend(remote_reads(&r.c, &d, true));
    r.steps.extend(remote_confirm(&r.c, &d, true));
    r.fixture.scripted_gh(&r.steps);
    let mut count = 109;
    preview(&mut s, &r, mouse, &mut count, false, &d);
    confirm(&mut s, &r, mouse, &mut count, false);
    assert_eq!(
        bare_git(&r.tls, &["rev-parse", "refs/heads/workflow/edit"]).trim(),
        d
    );
    assert_eq!(
        bare_git(&r.tls, &["rev-parse", "refs/heads/main"]).trim(),
        r.c
    );
    let branch = journal(&r.fixture);
    assert_eq!(branch.len(), 1);
    assert_eq!(branch[0].1, "confirmed");
    assert_eq!(branch[0].0, "\"publish_workflow_branch\"");
    positive_aliases(&r, 1);
    assert!(branch[0].3.contains(&d));
    assert!(
        !branch[0].2.contains(TITLE)
            && !branch[0].2.contains(PR_BODY)
            && !branch[0].2.contains("PRIVATE_NATIVE_W9_EXACT_EDIT")
    );
    preview(&mut s, &r, mouse, &mut count, true, &d);
    confirm(&mut s, &r, mouse, &mut count, true);
    assert_eq!(count, 172);
    let rows = journal(&r.fixture);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.1 == "confirmed"));
    assert_eq!(rows[1].0, "\"create_workflow_pr\"");
    assert!(rows[1].3.contains("PR_actual_workflow_9901"));
    positive_aliases(&r, 2);
    assert_eq!(
        r.fixture
            .git(&["-C", root.to_str().unwrap(), "rev-parse", "HEAD"])
            .trim(),
        d
    );
    assert_eq!(git_state(&r.fixture), git);
    assert_eq!(
        fs::read(r.fixture.repo().join(".git/config")).unwrap(),
        git_config
    );
    positive_aliases(&r, 2);
    assert_eq!(remote_sql(&r.fixture), sql);
    assert_eq!(
        fs::read(r.fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    s.capture_completed_terminal_v1("task54-w9-real-tls-branch-and-canonical-pr");
    close(&mut s, mouse);
    s.quit_and_assert_restored_terminal(mouse);
    exact_remote(&s, &r.fixture, &r.steps);
    assert_eq!(remote_sql(&r.fixture), sql);
    assert_eq!(git_state(&r.fixture), git);
    let requests = r.tls.requests().unwrap();
    assert!(requests.iter().any(|request| request.method == "POST"
        && request.path == "/example/ci.git/git-receive-pack"
        && request.status == 200));
    assert!(!requests
        .iter()
        .any(|request| request.path.contains("PRIVATE_")));
    cleanup(r.tls);
}
#[test]
fn keyboard_task54_workflow_edit_v9_real_tls_push_then_canonical_pr_preserves_original() {
    positive(false)
}
#[test]
fn mouse_task54_workflow_edit_v9_real_tls_push_then_canonical_pr_preserves_original() {
    positive(true)
}
#[test]
fn keyboard_task54_workflow_edit_v9_actual_startup_non_utf8_env_and_fixed_overrides_survive_full_git_tls_flow(
) {
    positive_environment_v9(false, true)
}
#[test]
fn mouse_task54_workflow_edit_v9_actual_startup_non_utf8_env_and_fixed_overrides_survive_full_git_tls_flow(
) {
    positive_environment_v9(true, true)
}
