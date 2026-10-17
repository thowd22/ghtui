//! Real-binary read-only capability inspector journeys. These check visible
//! state, strict provider requests, scoped storage, Git and terminal restoration.
#[path = "capability_inspector_cli.rs"]
mod cli;
#[path = "capability_inspector_identity.rs"]
mod identity;

use crate::support::{Fixture, GhStep, Session};
use ghtui::{
    config::{Config, ThemeName},
    storage::{Cache, Record, Scope},
};
use serde_json::{json, Value};
use std::{fs, path::Path, time::Duration};

const TITLE: &str = "Capabilities • read-only inventory";
const HOST: &str = "github.invalid";
const LIMITED_HOST: &str = "enterprise.invalid";
const PRIVATE_CANARY: &str = "NeverDisplayedPrivateCapabilityRecordCanary";
const DRAFT: &str = "Retained original draft: never published by an inspector";
const INLINE_DETAIL: &str = "Action: Inline/multiline comments, replies and edits (pr-inline)";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scenario {
    MissingGh,
    AuthDenied,
    LimitedHost,
    InvalidVersion,
}
impl Scenario {
    fn host(self) -> &'static str {
        if self == Self::LimitedHost {
            LIMITED_HOST
        } else {
            HOST
        }
    }
    fn expected_requests(self) -> usize {
        match self {
            Self::MissingGh => 0,
            Self::AuthDenied => 2,
            Self::LimitedHost => 5,
            Self::InvalidVersion => 1,
        }
    }
}

fn version() -> GhStep {
    GhStep::read(
        &["--version"],
        "gh version 2.0.0 (limited offline fixture)\n",
    )
}
fn user(host: &str) -> GhStep {
    GhStep::read(
        &["api", "user", "--hostname", host, "--method", "GET"],
        "{\"id\":42,\"login\":\"fixture\"}",
    )
}
fn protocol(scenario: Scenario) -> Vec<GhStep> {
    match scenario {
        Scenario::MissingGh => vec![],
        Scenario::InvalidVersion => vec![GhStep::read(
            &["--version"],
            "gh version 2.invalid\nNeverDisplayedPrivateCapabilityRecordCanary\n",
        )],
        Scenario::AuthDenied => {
            let mut denied = user(HOST);
            denied.stdout.clear();
            denied.stderr =
                b"HTTP 401: fixture authentication denied; use gh auth login\n".to_vec();
            denied.status = 1;
            vec![version(), denied]
        }
        Scenario::LimitedHost => {
            let mut unsupported = GhStep::read(
                &[
                    "api",
                    "search/issues",
                    "--hostname",
                    LIMITED_HOST,
                    "--method",
                    "GET",
                    "-f",
                    "q=is:pr is:open author:fixture",
                    "-f",
                    "per_page=100",
                    "-f",
                    "page=1",
                ],
                "",
            );
            unsupported.stderr =
                b"Unknown JSON field: unsupported Enterprise search contract\n".to_vec();
            unsupported.status = 1;
            // Initial current identity then the existing automatic cached-view
            // freshness identity. The first query failure ends the collection.
            vec![
                version(),
                user(LIMITED_HOST),
                version(),
                user(LIMITED_HOST),
                unsupported,
            ]
        }
    }
}
fn fixture(scenario: Scenario) -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "preserved.txt",
        b"saved original\n",
        "Capability inspector saved version",
    );
    fixture.commit_file("staged.txt", b"saved staged base\n", "Second saved version");
    fixture.git(&["branch", "feature"]);
    fixture.git(&["tag", "inspector-saved"]);
    fs::write(
        fixture.repo().join("preserved.txt"),
        b"dirty local text must remain unchanged\n",
    )
    .unwrap();
    fs::write(
        fixture.repo().join("staged.txt"),
        b"explicitly staged bytes\n",
    )
    .unwrap();
    fixture.git(&["add", "--", "staged.txt"]);
    fs::write(
        fixture.repo().join("untracked-雪.txt"),
        b"untracked local bytes are not a GitHub identity\n",
    )
    .unwrap();
    Config {
        host: scenario.host().into(),
        avatars: false,
        refresh_seconds: 86_400,
        theme: if scenario == Scenario::MissingGh {
            ThemeName::NoColor
        } else {
            ThemeName::Dark
        },
        ..Config::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    for uid in [42, 43] {
        let scope = Scope::github(scenario.host(), Some(uid)).unwrap();
        cache
            .put(
                &scope,
                &Record::new(
                    "inspector-private-canary",
                    "retained",
                    "example/private",
                    json!({"body":PRIVATE_CANARY,"uid":uid}),
                ),
            )
            .unwrap();
        cache.put_draft(&scope, "inspector-draft", DRAFT).unwrap();
        cache
            .save_view(
                &scope,
                "inspector-saved-view",
                &json!({"repo":"example/private","query":"retained source","uid":uid}),
            )
            .unwrap();
    }
    fixture.scripted_gh(&protocol(scenario));
    fixture
}

#[derive(Debug, PartialEq, Eq)]
struct GitSnapshot {
    head: String,
    refs: String,
    status: String,
    staged: String,
    index: Vec<u8>,
    dirty: Vec<u8>,
    staged_file: Vec<u8>,
    untracked: Vec<u8>,
}
fn git_snapshot(fixture: &Fixture) -> GitSnapshot {
    GitSnapshot {
        head: fixture.git(&["rev-parse", "HEAD"]),
        refs: fixture.git(&["for-each-ref", "--format=%(refname)%00%(objectname)"]),
        status: fixture.git(&["status", "--porcelain=v1", "-z"]),
        staged: fixture.git(&["ls-files", "--stage"]),
        index: fs::read(fixture.repo().join(".git/index")).unwrap(),
        dirty: fs::read(fixture.repo().join("preserved.txt")).unwrap(),
        staged_file: fs::read(fixture.repo().join("staged.txt")).unwrap(),
        untracked: fs::read(fixture.repo().join("untracked-雪.txt")).unwrap(),
    }
}
fn launch(fixture: &Fixture, scenario: Scenario) -> Session {
    let trace = fixture
        .root
        .join("git-child-trace.log")
        .display()
        .to_string();
    let mut environment = vec![("GIT_TRACE", trace.as_str())];
    let only_git_path;
    if scenario == Scenario::MissingGh {
        // Remove only the generated fixture dependency, and never fall back to
        // a real gh on the developer PATH. The child still has installed Git.
        let git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join("git"))
            .find(|path| path.is_file())
            .expect("installed git required by this real Git contract");
        std::os::unix::fs::symlink(git, fixture.root.join("bin/git")).unwrap();
        fs::remove_file(fixture.root.join("bin/gh")).unwrap();
        only_git_path = fixture.root.join("bin").display().to_string();
        environment.push(("PATH", only_git_path.as_str()));
        environment.push(("NO_COLOR", "1"));
    }
    Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        fixture,
        120,
        40,
        &environment,
    )
}
fn activate(session: &mut Session, mouse: bool, label: &str, keys: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(keys);
    }
}
fn open(session: &mut Session, mouse: bool) {
    activate(session, mouse, "[: Commands]", b":");
    session.wait_for_screen("Inspect capabilities");
    if mouse {
        session.click_visible_action("Inspect capabilities");
    } else {
        session.send_key(b"\x1b[B\x1b[B\x1b[B\x1b[B\x1b[B\r");
    }
    session.wait_for_screen(TITLE);
    session.wait_for_screen("Filter focused:");
    session.wait_for_screen("[Esc Close]");
}
fn compact(text: &str) -> String {
    text.chars()
        .filter(|character| !character.is_whitespace())
        .collect()
}
fn overlay_text(session: &Session) -> String {
    session.visible_overlay_body(TITLE).unwrap_or_default()
}
fn wait_overlay(session: &mut Session, expected: &str) {
    let compact_expected = compact(expected);
    let deadline = std::time::Instant::now() + Duration::from_secs(8);
    loop {
        if compact(&overlay_text(session)).contains(&compact_expected) {
            return;
        }
        if std::time::Instant::now() >= deadline {
            session.save_artifacts();
            panic!(
                "Missing independently expected inspector text {expected:?}\n{}; artifacts: {}",
                session.screen(),
                session.artifact_dir.display()
            );
        }
        // Consume real terminal output. This is state synchronization, not a
        // readiness sleep or access to the application's direct handlers.
        session.poll_output(Duration::from_millis(20));
    }
}
fn filter(session: &mut Session, mouse: bool, text: &str) {
    if mouse {
        session.wait_for_screen("Filter");
        let (column, row) = session.locate("Filter").unwrap();
        session.click(column + 16, row);
    }
    session.send_key(b"\x01");
    session.paste(text);
    session.wait_for_screen("Filter focused:");
}
fn details(session: &mut Session, mouse: bool, label: &str, id: &str) {
    if mouse {
        session.click_visible_action(label);
        wait_overlay(session, &format!("Action: {label}"));
        // Clicking the actual detail content must have the same focus action
        // as Enter from the rows, not rely on a scroll control to gain focus.
        session.click_visible_action("Action:");
        session.wait_for_screen("details focused");
        session.click_visible_action("[+ Detail]");
        session.click_visible_action("[− Detail]");
    } else {
        session.send_key(b"\t\r");
    }
    session.wait_for_screen("details focused");
    wait_overlay(session, id);
}
fn scroll_detail_until(session: &mut Session, mouse: bool, text: &str) {
    scroll_detail_direction_until(session, mouse, text, true);
}
fn scroll_detail_up_until(session: &mut Session, mouse: bool, text: &str) {
    scroll_detail_direction_until(session, mouse, text, false);
}
fn scroll_detail_direction_until(session: &mut Session, mouse: bool, text: &str, down: bool) {
    for _ in 0..100 {
        if compact(&overlay_text(session)).contains(&compact(text)) {
            return;
        }
        if mouse && down {
            let (column, row) = session
                .locate("Requirements •")
                .expect("visible detail panel heading");
            session.wheel(column + 2, row + 2, true);
        } else if mouse {
            session.click_visible_action("[− Detail]");
        } else {
            session.send_key(if down { b"\x1b[B" } else { b"\x1b[A" });
        }
        session.poll_output(Duration::from_millis(20));
    }
    session.save_artifacts();
    panic!(
        "Could not scroll actual inspector to {text:?}\n{}; artifacts: {}",
        session.screen(),
        session.artifact_dir.display()
    );
}
fn close(session: &mut Session, mouse: bool, view: &str) {
    activate(session, mouse, "[Esc Close]", b"\x1b");
    session
        .try_wait_for_condition(
            |screen| !screen.contains(TITLE) && !screen.contains("[PgDn Rows]"),
            "capability overlay title and actual body to close",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| {
            session.save_artifacts();
            panic!("{error}; artifacts: {}", session.artifact_dir.display())
        });
    session.assert_focused_view(view);
}
fn requests(session: &Session, fixture: &Fixture) -> Vec<Value> {
    session
        .inspect_command_journal(fixture)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn assert_protocol(session: &Session, fixture: &Fixture, scenario: Scenario) {
    let actual = requests(session, fixture);
    let expected = protocol(scenario);
    assert_eq!(actual.len(), scenario.expected_requests());
    assert_eq!(actual.len(), expected.len());
    for (row, step) in actual.iter().zip(expected) {
        assert_eq!(row["matched"], true);
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["stdin_hex"], "");
        assert!(row["args"][0] == "--version" || row["args"][5] == "GET");
    }
}
fn trace(fixture: &Fixture) -> Vec<u8> {
    fs::read(fixture.root.join("git-child-trace.log")).unwrap_or_default()
}
fn assert_storage(fixture: &Fixture, scenario: Scenario) {
    let cache = Cache::open(&fixture.root.join("config/ghtui/cache.sqlite3")).unwrap();
    for uid in [42, 43] {
        let scope = Scope::github(scenario.host(), Some(uid)).unwrap();
        let rows = cache
            .list(&scope, Some("inspector-private-canary"))
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "retained");
        assert_eq!(rows[0].repository, "example/private");
        assert_eq!(rows[0].payload, json!({"body":PRIVATE_CANARY,"uid":uid}));
        assert_eq!(
            cache.draft(&scope, "inspector-draft").unwrap().as_deref(),
            Some(DRAFT)
        );
        assert_eq!(
            cache.saved_view(&scope, "inspector-saved-view").unwrap(),
            Some(json!({"repo":"example/private","query":"retained source","uid":uid}))
        );
    }
}

fn journey(scenario: Scenario, mouse: bool) {
    let fixture = fixture(scenario);
    let git_before = git_snapshot(&fixture);
    let config_before = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = launch(&fixture, scenario);
    session.wait_for_screen("Recent commits — saved versions");
    session.assert_focused_view("Repository");
    assert!(!trace(&fixture).is_empty(),"The child-only trace must witness actual local Git commands before it can prove no new ones");
    activate(&mut session, mouse, "1 Inbox", b"1");
    session.assert_focused_view("Inbox");
    activate(&mut session, mouse, "[r Refresh]", b"r");
    match scenario {
        Scenario::MissingGh => session.wait_for_screen("GitHub CLI is missing"),
        Scenario::AuthDenied => session.wait_for_screen("GitHub authentication failed"),
        Scenario::InvalidVersion => {
            session.wait_for_screen("GitHub CLI version could not be verified")
        }
        Scenario::LimitedHost => {
            session.wait_for_command_count(&fixture, 5);
            session.wait_for_screen("Offline/stale");
            session.wait_for_screen("Unknown JSON field");
        }
    }
    assert_protocol(&session, &fixture, scenario);
    let journal_before = session.inspect_command_journal(&fixture);
    let trace_before = trace(&fixture);
    open(&mut session, mouse);
    if scenario == Scenario::LimitedHost {
        wait_overlay(&mut session, "account #42");
    } else {
        wait_overlay(&mut session, "No verified numeric GitHub identity");
    }
    filter(&mut session, mouse, "pr-inline");
    session.wait_for_screen("1 matching");
    details(
        &mut session,
        mouse,
        "Inline/multiline comments",
        "pr-inline",
    );
    wait_overlay(&mut session, INLINE_DETAIL);
    scroll_detail_until(
        &mut session,
        mouse,
        match scenario {
            Scenario::MissingGh => "GitHub CLI is missing",
            Scenario::AuthDenied => "GitHub authentication failed",
            Scenario::LimitedHost => "Unknown JSON field",
            Scenario::InvalidVersion => "Check gh --version",
        },
    );
    scroll_detail_until(&mut session, mouse, "Implementation: TASK-19");
    scroll_detail_until(&mut session, mouse, "Transport: gh api pulls/comments");
    scroll_detail_until(
        &mut session,
        mouse,
        "Required permission class: comment; not observed/granted",
    );
    scroll_detail_until(&mut session, mouse, "Minimum operation version: unverified");
    scroll_detail_until(
        &mut session,
        mouse,
        "Preview/confirmation: handled by the exact native workflow",
    );
    // Reflow must retain the actual selected ID. The filter also contains
    // "pr-inline", so only the genuine full Action detail is a top predicate.
    // Return with bounded actual -1 scroll actions, never an assumed offset.
    session.resize(80, 24);
    scroll_detail_up_until(&mut session, mouse, INLINE_DETAIL);
    session.wait_for_screen("1 matching");
    assert!(!session.screen().contains(PRIVATE_CANARY));
    // Outside-overlay view/Refresh actions must not receive captured input.
    if mouse {
        let tab = session
            .locate("2 Repository")
            .expect("underlying public tab");
        session.click(tab.0, tab.1);
        let refresh = session
            .locate("[r Refresh]")
            .expect("underlying public footer");
        session.click(refresh.0, refresh.1);
    } else {
        session.send_key(b"4rq");
    }
    close(&mut session, mouse, "Inbox");
    assert_eq!(session.inspect_command_journal(&fixture), journal_before);
    assert_eq!(
        trace(&fixture),
        trace_before,
        "Inspector interactions must not spawn even read-only Git commands"
    );
    assert_protocol(&session, &fixture, scenario);

    // Reopening must reset palette selection to the genuine sixth row, not
    // carry an index which enters another view/workflow.
    session.resize(120, 40);
    open(&mut session, mouse);
    // Real bounded Unicode editing, not a direct TextBuffer invocation. The
    // rejected paste must preserve the actual visible query and keep scope
    // empty rather than broadening it or interpreting keys behind the overlay.
    session.paste("é界");
    session.wait_for_screen("No matching inventory rows");
    if mouse {
        let (column, row) = session.locate("Filter focused:").unwrap();
        session.click(column + 17, row);
    } else {
        session.send_key(b"\x1b[D");
    }
    session.paste("X");
    session.wait_for_screen("éX界");
    for rejected in ["x".repeat(513), "unsafe\nfilter".into()] {
        session.paste(&rejected);
        session.wait_for_screen("Filter refused");
        session.wait_for_screen("éX界");
        session.wait_for_screen("No matching inventory rows");
    }
    session.send_key(b"\x15");
    session.wait_for_screen("33 matching");
    // Page controls and hovered-list wheel retain genuine selected action IDs.
    activate(&mut session, mouse, "[PgDn Rows]", b"\x1b[6~");
    wait_overlay(&mut session, "pr-comment-edit-delete-react");
    activate(&mut session, mouse, "[PgUp Rows]", b"\x1b[5~");
    wait_overlay(&mut session, "personal-queues");
    if mouse {
        session.click_visible_action("PR summary, timeline");
    } else {
        session.send_key(b"\x1b[B");
    }
    wait_overlay(&mut session, "pr-details");
    if mouse {
        let (column, row) = session.locate("PR summary, timeline").unwrap();
        session.wheel(column, row, true);
    } else {
        session.send_key(b"\x1b[B");
    }
    wait_overlay(&mut session, "pr-checks");
    let (column, row) = session.locate("Checks, status, run jobs and logs").unwrap();
    assert!(
        session.cell_inverted(row, column),
        "Selection must be visible even without color"
    );
    if mouse {
        session.click_visible_action("[+ Detail]");
        session.click_visible_action("[− Detail]");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen("details focused");
    scroll_detail_until(
        &mut session,
        mouse,
        "static inventory records transport present",
    );
    scroll_detail_until(
        &mut session,
        mouse,
        "not native parity or a live support claim",
    );
    close(&mut session, mouse, "Inbox");
    assert_eq!(session.inspect_command_journal(&fixture), journal_before);
    assert_eq!(trace(&fixture), trace_before);
    open(&mut session, mouse);
    filter(&mut session, mouse, "actions admin runners");
    session.wait_for_screen("1 matching");
    details(
        &mut session,
        mouse,
        "Runner/group status",
        "actions-runners",
    );
    scroll_detail_until(&mut session, mouse, "Implementation: TASK-60");
    scroll_detail_until(
        &mut session,
        mouse,
        "Required permission class: admin; not observed/granted",
    );
    session.resize(120, 40);
    close(&mut session, mouse, "Inbox");
    assert_eq!(session.inspect_command_journal(&fixture), journal_before);
    assert_eq!(trace(&fixture), trace_before);
    // The dependency error must not block local repository/diff exploration.
    activate(&mut session, mouse, "2 Repository", b"2");
    session.assert_focused_view("Repository");
    activate(&mut session, mouse, "[w Working]", b"w");
    session.wait_for_screen("dirty local text must remain unchanged");
    let local_trace = trace(&fixture);
    assert!(local_trace.len()>trace_before.len(),"The same child must remain able to execute real local Git browsing after dependency failure");
    assert_protocol(&session, &fixture, scenario);
    assert_eq!(git_snapshot(&fixture), git_before);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config_before
    );
    assert_storage(&fixture, scenario);
    session.quit_and_assert_restored_terminal(mouse);
    assert_protocol(&session, &fixture, scenario);
    assert_eq!(trace(&fixture), local_trace);
    assert_eq!(git_snapshot(&fixture), git_before);
    assert_storage(&fixture, scenario);
}

#[test]
fn gh_provider_inspector_keyboard_missing_gh_keeps_local_git_and_readonly_setup_navigation() {
    journey(Scenario::MissingGh, false);
}
#[test]
fn gh_provider_inspector_mouse_missing_gh_keeps_local_git_and_readonly_setup_navigation() {
    journey(Scenario::MissingGh, true);
}
#[test]
fn gh_provider_inspector_keyboard_auth_denied_keeps_exact_permission_unknown_and_no_requests() {
    journey(Scenario::AuthDenied, false);
}
#[test]
fn gh_provider_inspector_mouse_auth_denied_keeps_exact_permission_unknown_and_no_requests() {
    journey(Scenario::AuthDenied, true);
}
#[test]
fn gh_provider_inspector_keyboard_limited_host_never_invents_native_version_or_admin_support() {
    journey(Scenario::LimitedHost, false);
}
#[test]
fn gh_provider_inspector_mouse_limited_host_never_invents_native_version_or_admin_support() {
    journey(Scenario::LimitedHost, true);
}

#[test]
fn gh_provider_identity_version_keyboard_invalid_cli_preserves_inspector_local_git_and_drafts() {
    journey(Scenario::InvalidVersion, false);
}
#[test]
fn gh_provider_identity_version_mouse_invalid_cli_preserves_inspector_local_git_and_drafts() {
    journey(Scenario::InvalidVersion, true);
}
