//! Real-binary journeys. Each input path runs SHA-1 and SHA-256
//! objects with genuine empty author/committer fields, not invented identities.
use super::*;
use ghtui::{
    config::Config,
    storage::{Cache, Scope},
};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[path = "local_commit_actor_association_native_v1.rs"]
mod association_native_v1;

const AUTHORLESS: &str = "Saved unknown author";
const COMMITTERLESS: &str = "Saved unknown committer";
const DRAFT: &str = "git-role-original-draft";
const INTENT: &str = "git-role-original-intent";

#[derive(Debug, PartialEq, Eq)]
struct GitState {
    head: String,
    refs: String,
    index: Vec<u8>,
    staged: String,
    dirty: String,
    work: Vec<u8>,
    untracked: Vec<u8>,
}
fn state(fixture: &Fixture) -> GitState {
    GitState {
        head: fixture.git(&["rev-parse", "HEAD"]),
        refs: fixture.git(&["show-ref", "--head"]),
        index: fs::read(fixture.repo().join(".git/index")).unwrap(),
        staged: fixture.git(&["diff", "--cached", "--binary"]),
        dirty: fixture.git(&["diff", "--binary"]),
        work: fs::read(fixture.repo().join("readme")).unwrap(),
        untracked: fs::read(fixture.repo().join("role-scratch.txt")).unwrap(),
    }
}
fn object(fixture: &Fixture, bytes: &str) -> String {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(fixture.repo())
        .args(["hash-object", "-t", "commit", "-w", "--stdin"])
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(bytes.as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "Real commit object: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}
struct Source {
    fixture: Fixture,
    original: String,
    head: String,
    original_object: String,
    head_object: String,
}
fn source(format: &str) -> Source {
    let fixture = Fixture::new();
    fixture.git(&[
        "init",
        "-q",
        "--initial-branch=main",
        &format!("--object-format={format}"),
    ]);
    fixture.git(&["config", "user.name", "Fixture Root"]);
    fixture.git(&["config", "user.email", "root@example.invalid"]);
    fixture.commit_file("readme", b"original saved bytes\n", "Genuine root");
    let parent = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let tree = fixture.git(&["rev-parse", "HEAD^{tree}"]).trim().to_owned();
    let original_object = format!("tree {tree}\nparent {parent}\nauthor  <> 1700000000 +0000\ncommitter Exact Recorder <record@example.invalid> 1700000010 +0000\n\n{AUTHORLESS}\n");
    let original = object(&fixture, &original_object);
    let head_object = format!("tree {tree}\nparent {original}\nauthor Exact Writer <write@example.invalid> 1700000020 +0000\ncommitter  <> 1700000030 +0000\n\n{COMMITTERLESS}\n");
    let head = object(&fixture, &head_object);
    fixture.git(&["update-ref", "refs/heads/main", &head]);
    fixture.git(&["tag", "original-git-role", &original]);
    fs::write(
        fixture.repo().join("readme"),
        b"staged original user bytes\n",
    )
    .unwrap();
    fixture.git(&["add", "--", "readme"]);
    fs::write(
        fixture.repo().join("readme"),
        b"dirty original user bytes\n",
    )
    .unwrap();
    fs::write(
        fixture.repo().join("role-scratch.txt"),
        b"untracked original user bytes\n",
    )
    .unwrap();
    let forbidden_curl = fixture.root.join("bin/curl");
    let script = format!("#!/bin/sh\nprintf '%s\\n' \"$$\" > '{}'/role-curl-pid\nprintf '%s\\n' \"$@\" > '{}'/role-curl-argv\nexit 93\n", fixture.root.display(), fixture.root.display());
    fs::write(&forbidden_curl, script).unwrap();
    fs::set_permissions(&forbidden_curl, fs::Permissions::from_mode(0o755)).unwrap();
    Source {
        fixture,
        original,
        head,
        original_object,
        head_object,
    }
}
fn database(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("config/ghtui/cache.sqlite3")
}
fn trace(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("git-role-trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|event| event["event"] == "start")
        .collect()
}
fn assert_read_only_trace(fixture: &Fixture) {
    let events = trace(fixture);
    assert!(!events.is_empty(), "Real Git read trace must be observed");
    for event in events {
        let argv = event["argv"].as_array().unwrap();
        let mut args = argv.iter().skip(1).map(|arg| arg.as_str().unwrap());
        let mut command = None;
        while let Some(arg) = args.next() {
            if matches!(
                arg,
                "-c" | "-C" | "--git-dir" | "--work-tree" | "--namespace"
            ) {
                assert!(args.next().is_some());
            } else if !arg.starts_with('-') {
                command = Some(arg);
                break;
            }
        }
        assert!(
            matches!(
                command,
                Some(
                    "rev-parse"
                        | "status"
                        | "log"
                        | "show"
                        | "show-ref"
                        | "diff"
                        | "diff-tree"
                        | "ls-tree"
                        | "cat-file"
                        | "for-each-ref"
                        | "rev-list"
                        | "symbolic-ref"
                        | "config"
                        | "check-attr"
                )
            ),
            "No mutation/checkout/clone in actual child trace: {event}"
        );
    }
}
fn ready(session: &mut Session, labels: &[&str]) {
    session
        .try_wait_for_condition(
            |screen| labels.iter().all(|label| screen.contains(label)),
            &format!("Complete original Git actor frame: {labels:?}"),
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
}
/// Waits until no new Git child has started for a second and no read shows `Esc Cancel`.
fn settle_trace(session: &mut Session, fixture: &Fixture) {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut count = trace(fixture).len();
    let mut quiet = Instant::now();
    loop {
        session.poll_output(Duration::from_millis(50));
        let now = trace(fixture).len();
        if now != count || session.screen().contains("Esc Cancel") {
            count = now;
            quiet = Instant::now();
        } else if quiet.elapsed() >= Duration::from_secs(1) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Git reads did not settle\n{}",
            session.screen()
        );
    }
}
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
/// A Repository key-bar action such as `[a Author]`: by keyboard its key; by mouse its token
/// where the settled bar paints it, otherwise its Commands entry (low-priority tokens leave
/// the bar at narrow widths, yet the same action stays one click path away).
pub(crate) fn choose_key_bar(session: &mut Session, mouse: bool, label: &str) {
    choose_key_bar_or_command(session, mouse, label, label);
}
/// `choose_key_bar` for a token whose action joins an existing Commands entry with its own
/// caption (`[l Legend]` is `Inspect Git relationship legend` there).
pub(crate) fn choose_key_bar_or_command(
    session: &mut Session,
    mouse: bool,
    label: &str,
    command: &str,
) {
    if !mouse {
        session.choose_control(false, label);
        return;
    }
    session.wait_for_settled_footer();
    if session
        .try_wait_for_control(label, Duration::from_millis(500))
        .is_some()
    {
        session.click_visible_action(label);
    } else {
        session.invoke_via_palette(true, command);
    }
}
fn saved_ready(session: &mut Session, selected: &str, head: &str, labels: &[&str]) {
    // TASK-119: the plain Overview names the commit in its Branch section (`Last commit` for
    // the checked-out one, otherwise `Selected commit`) with its full id on its own line and
    // both roles, instead of the `Viewed:`/`In use:` rows above the other views.
    if labels.contains(&"Recent commits") {
        let title = if selected == head {
            "Last commit · "
        } else {
            "Selected commit · "
        };
        let id = format!("  {selected}");
        let mut expected = vec![title, id.as_str()];
        expected.extend_from_slice(labels);
        ready(session, &expected);
        return;
    }
    let saved = format!("Viewed: {selected}");
    let checkout = format!("In use: {head}");
    let mut expected = vec![saved.as_str(), checkout.as_str()];
    expected.extend_from_slice(labels);
    ready(session, &expected);
}
fn profile(
    session: &mut Session,
    source: &Source,
    mouse: bool,
    author: bool,
    selected: &str,
    expected: &str,
) {
    let before = trace(&source.fixture);
    let (button, title) = if author {
        ("[a Author]", "Actor profile — Git author")
    } else {
        ("[c Committer]", "Actor profile — Git committer")
    };
    choose_key_bar(session, mouse, button);
    saved_ready(
        session,
        selected,
        &source.head,
        &[title, expected, "GitHub account association: unavailable"],
    );
    assert_eq!(
        trace(&source.fixture),
        before,
        "Prepared local profile does not resolve Git or fetch remote identity"
    );
    assert!(!session.screen().contains("@Exact"));
    assert!(!session.screen().contains('▀'));
    assert!(!source.fixture.root.join("role-curl-pid").exists());
    assert!(session.inspect_command_journal(&source.fixture).is_empty());
}
fn select_original(session: &mut Session, mouse: bool, original: &str) {
    ready(session, &["Read-only · ", " commits loaded"]);
    let rows = session.wait_for_history_ready();
    assert!(
        rows.iter()
            .any(|row| row.selected && row.message.starts_with("Saved unknown")),
        "History opens on the {COMMITTERLESS} HEAD commit"
    );
    session.open_saved_graph_subject(mouse, AUTHORLESS, original);
}
fn durable(fixture: &Fixture, intent: &Value) {
    let cache = Cache::open(&database(fixture)).unwrap();
    let scope = Scope::github("github.com", Some(42)).unwrap();
    assert_eq!(
        cache.draft(&scope, DRAFT).unwrap().as_deref(),
        Some("Original42 unsent Git role text")
    );
    assert_eq!(
        cache.saved_views(&scope).unwrap(),
        vec![(INTENT.into(), intent.clone())]
    );
    assert!(
        cache.list(&scope, None).unwrap().is_empty(),
        "Local actor navigation writes no remote source frames"
    );
    let other = Scope::github("github.com", Some(43)).unwrap();
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.saved_views(&other).unwrap().is_empty());
    assert!(cache.draft(&other, DRAFT).unwrap().is_none());
}
fn journey(mouse: bool) {
    for format in ["sha1", "sha256"] {
        let source = source(format);
        let fixture = &source.fixture;
        assert_eq!(
            source.original.len(),
            if format == "sha256" { 64 } else { 40 }
        );
        assert_ne!(source.original, source.head);
        let config_path = fixture.root.join("config/ghtui/config.toml");
        Config {
            refresh_seconds: 3600,
            avatars: true,
            ..Default::default()
        }
        .save(&config_path)
        .unwrap();
        let config_before = fs::read(&config_path).unwrap();
        let before = state(fixture);
        let intent = json!({"kind":"original local navigation intent", "saved":source.original});
        {
            let cache = Cache::open(&database(fixture)).unwrap();
            let scope = Scope::github("github.com", Some(42)).unwrap();
            cache
                .put_draft(&scope, DRAFT, "Original42 unsent Git role text")
                .unwrap();
            cache.save_view(&scope, INTENT, &intent).unwrap();
        }
        let trace_path = fixture.root.join("git-role-trace.jsonl");
        let mut env = vec![("GIT_TRACE2_EVENT", trace_path.to_str().unwrap())];
        if mouse {
            env.push(("NO_COLOR", "1"));
        }
        let (cols, rows) = if mouse { (80, 24) } else { (120, 40) };
        let mut session = Session::launch_binary(
            Path::new(env!("CARGO_BIN_EXE_ghtui")),
            fixture,
            cols,
            rows,
            &env,
        );
        saved_ready(
            &mut session,
            &source.head,
            &source.head,
            &[
                "Recent commits",
                "author Exact Writer",
                "committer unknown name [email unavailable]",
                "Local Git workspace ready • working files preserved",
            ],
        );
        profile(
            &mut session,
            &source,
            mouse,
            false,
            &source.head,
            "[?] committer unknown name [email unavailable] [local Git]",
        );
        profile(
            &mut session,
            &source,
            mouse,
            true,
            &source.head,
            "[EW] author Exact Writer <write@example.invalid> [local Git]",
        );
        choose(&mut session, mouse, "[g History]", b"g");
        select_original(&mut session, mouse, &source.original);
        // History shows the saved commit's row with its empty-name author chip (`author u…`);
        // the Commit detail (≥120 columns) or the identity rows (when a SHA-1 leaves room)
        // carry both roles, and the profiles below check each role's complete text.
        saved_ready(
            &mut session,
            &source.original,
            &source.head,
            &[AUTHORLESS, "author u"],
        );
        if format == "sha1" || !mouse {
            // At 80 columns `In use: <sha>` leaves room for the start of the committer name;
            // the profile below checks its complete text.
            ready(&mut session, &["author unknown name [e", "Exact Recor"]);
        }
        // A clicked History row is saved by a background read that can still be starting
        // when the frame already names it; the profile's no-Git check begins after it.
        settle_trace(&mut session, fixture);
        profile(
            &mut session,
            &source,
            mouse,
            true,
            &source.original,
            "[?] author unknown name [email unavailable] [local Git]",
        );
        profile(
            &mut session,
            &source,
            mouse,
            false,
            &source.original,
            "[ER] committer Exact Recorder <record@example.invalid> [local Git]",
        );
        let profile_trace = trace(fixture);
        let (cols, rows) = if mouse { (120, 40) } else { (80, 24) };
        session.resize(cols, rows);
        saved_ready(
            &mut session,
            &source.original,
            &source.head,
            &[
                "Actor profile — Git committer",
                "committer Exact Recorder",
                "author unknown name [email unavailable]",
                "GitHub account association: unavailable",
            ],
        );
        assert_eq!(
            trace(fixture),
            profile_trace,
            "Resize does not re-resolve the selected actor"
        );
        session.focus_footer_control("[q Quit]");
        assert!(
            session.inspect_command_journal(fixture).is_empty(),
            "Strict fake-gh rejects all unexpected local identity requests"
        );
        assert!(!fixture.root.join("role-curl-pid").exists());
        assert!(!fixture.root.join("role-curl-argv").exists());
        assert!(!fixture.root.join("config/ghtui/avatars").exists());
        durable(fixture, &intent);
        session.quit_and_assert_restored_terminal(mouse);
        assert_eq!(state(fixture), before);
        assert_eq!(fs::read(&config_path).unwrap(), config_before);
        assert_eq!(
            fixture.git(&["cat-file", "commit", &source.original]),
            source.original_object
        );
        assert_eq!(
            fixture.git(&["cat-file", "commit", &source.head]),
            source.head_object
        );
        assert_read_only_trace(fixture);
        durable(fixture, &intent);
    }
}

#[test]
fn git_actor_roles_keyboard_real_missing_role_profiles_keep_original_sha_and_no_remote_work() {
    journey(false);
}
#[test]
fn git_actor_roles_mouse_real_missing_role_profiles_keep_original_sha_and_no_remote_work() {
    journey(true);
}
