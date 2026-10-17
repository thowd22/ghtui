//! Native organization scope and secret form: exact CLI compatibility, no replay.
use super::*;
use ghtui::storage::{Cache, Record, Scope};
use std::{fs, path::Path};
const SECRET: &str = "synthetic-native-empty-selection-secret-never-persisted";
const CANARY: &str = "retained-private-cli-scope-canary";
#[derive(Clone, Copy)]
enum Case {
    Old,
    Custom,
    Supported,
}
impl Case {
    fn version(self) -> &'static str {
        match self {
            Self::Old => "gh version 2.74.0\n",
            Self::Custom => "gh version 2.101.0-custom\n",
            Self::Supported => "gh version 2.75.0\n",
        }
    }
    fn count(self) -> usize {
        if matches!(self, Self::Supported) {
            26
        } else {
            18
        }
    }
}
fn account_version(version: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], version),
        read("user", json!({"id":42,"login":"octocat"}), false),
    ]
}
fn protocol(case: Case) -> Vec<GhStep> {
    let mut steps = initial();
    steps.extend(compose_identity_steps()); // Choose scope OPEN4.
    steps.extend(current_account()); // Explicit local-only scope confirmation2.
    steps.push(read(
        "orgs/example/actions/secrets?per_page=100",
        json!([{"secrets":[]}]),
        true,
    ));
    steps.extend(current_account()); // Final identity2 after scope load.
    steps.extend(compose_identity_steps()); // Actual selected secret form OPEN4.
    steps.extend(account_version(case.version())); // Fresh confirmation preflight2.
    if matches!(case, Case::Supported) {
        steps.extend(account_version(case.version())); // Actual provider checks again.
        steps.push(read(
            "orgs/example/actions/secrets?per_page=100",
            json!([{"secrets":[]}]),
            true,
        ));
        let mut write = GhStep::read(
            &[
                "secret",
                "set",
                "DEPLOY_TOKEN",
                "--app",
                "actions",
                "--org",
                "example",
                "--visibility",
                "selected",
                "--no-repos-selected",
            ],
            "",
        );
        write.stdin = SECRET.as_bytes().to_vec();
        steps.push(write);
        steps.push(read("orgs/example/actions/secrets?per_page=100",json!([{"secrets":[{"name":"DEPLOY_TOKEN","visibility":"selected","created_at":"2026-10-17T00:00:00Z","updated_at":"2026-10-17T00:01:00Z"}]}]),true));
        steps.push(read(
            "orgs/example/actions/secrets/DEPLOY_TOKEN/repositories?per_page=100",
            json!([{"repositories":[]}]),
            true,
        ));
        steps.extend(current_account());
    }
    assert_eq!(steps.len(), case.count());
    steps
}
#[derive(Debug, PartialEq)]
struct GitState {
    head: String,
    refs: String,
    status: String,
    index: Vec<u8>,
    dirty: Vec<u8>,
    staged: Vec<u8>,
    untracked: Vec<u8>,
}
fn git_state(f: &Fixture) -> GitState {
    let status = f.git(&["status", "--porcelain=v1", "-z"]);
    GitState {
        head: f.git(&["rev-parse", "HEAD"]),
        refs: f.git(&["for-each-ref", "--format=%(refname)%00%(objectname)"]),
        status,
        index: fs::read(f.repo().join(".git/index")).unwrap(),
        dirty: fs::read(f.repo().join("unchanged.txt")).unwrap(),
        staged: fs::read(f.repo().join("staged.txt")).unwrap(),
        untracked: fs::read(f.repo().join("untracked-雪.txt")).unwrap(),
    }
}
fn assert_saved(f: &Fixture) {
    let cache = Cache::open(&f.root.join("config/ghtui/cache.sqlite3")).unwrap();
    for id in [42, 43] {
        let scope = Scope::github("github.invalid", Some(id)).unwrap();
        assert_eq!(
            cache
                .draft(&scope, "cli-preflight-retained")
                .unwrap()
                .as_deref(),
            Some(CANARY)
        );
        assert_eq!(
            cache.list(&scope, Some("cli-preflight-canary")).unwrap()[0].payload,
            json!({"body":CANARY,"id":id})
        );
        assert_eq!(
            cache.saved_view(&scope, "cli-preflight-view").unwrap(),
            Some(json!({"repo":"example/private","id":id}))
        );
    }
}
fn no_plaintext(f: &Fixture, s: &Session) {
    assert!(!s.screen().contains(SECRET));
    assert!(!String::from_utf8_lossy(&s.transcript).contains(SECRET));
    for suffix in ["", "-wal", "-shm"] {
        let file = f.root.join(format!("config/ghtui/cache.sqlite3{suffix}"));
        if file.exists() {
            assert!(!fs::read(file)
                .unwrap()
                .windows(SECRET.len())
                .any(|bytes| bytes == SECRET.as_bytes()));
        }
    }
}
fn exact(s: &mut Session, f: &Fixture, case: Case) {
    let rows = journal(s, f, case.count());
    let expected = protocol(case);
    for (row, step) in rows.iter().zip(expected) {
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(
            row["stdin_hex"],
            if step.args[0] == "secret" {
                "[REDACTED]"
            } else {
                ""
            }
        );
    }
    assert_eq!(
        rows.iter().filter(|row| row["args"][0] == "secret").count(),
        usize::from(matches!(case, Case::Supported))
    );
}
fn confirm_visible(s: &mut Session, mouse: bool) {
    if mouse {
        s.click_visible_action("[Confirm action]");
    } else {
        s.focus_visible_form_action("[Confirm action]");
        s.send_key(b"\r");
    }
}
fn error_contains(s: &Session, expected: &str) -> bool {
    let compact = |text: &str| {
        text.chars()
            .filter(|ch| !ch.is_whitespace())
            .collect::<String>()
    };
    compact(&s.visible_overlay_body("Set secret").unwrap_or_default()).contains(&compact(expected))
}
fn scroll_error_to(s: &mut Session, mouse: bool, expected: &str, down: bool) {
    for _ in 0..12 {
        if error_contains(s, expected) {
            return;
        }
        choose(
            s,
            mouse,
            if down {
                "[F8 Error↓]"
            } else {
                "[F7 Error↑]"
            },
            if down { b"\x1b[19~" } else { b"\x1b[18~" },
        );
        s.poll_output(std::time::Duration::from_millis(40));
    }
    s.save_artifacts();
    panic!(
        "Missing actual owned error text {expected:?}\n{}; artifacts: {}",
        s.screen(),
        s.artifact_dir.display()
    );
}
fn journey(case: Case, mouse: bool) {
    let f = fixture();
    f.commit_file("staged.txt", b"saved\n", "saved staged file");
    f.git(&["branch", "preserved-branch"]);
    f.git(&["tag", "preserved-tag"]);
    fs::write(f.repo().join("unchanged.txt"), b"dirty bytes preserved\n").unwrap();
    fs::write(f.repo().join("staged.txt"), b"staged bytes preserved\n").unwrap();
    f.git(&["add", "--", "staged.txt"]);
    fs::write(
        f.repo().join("untracked-雪.txt"),
        b"untracked bytes preserved\n",
    )
    .unwrap();
    let cache = Cache::open(&f.root.join("config/ghtui/cache.sqlite3")).unwrap();
    for id in [42, 43] {
        let scope = Scope::github("github.invalid", Some(id)).unwrap();
        cache
            .put(
                &scope,
                &Record::new(
                    "cli-preflight-canary",
                    "original",
                    "example/private",
                    json!({"body":CANARY,"id":id}),
                ),
            )
            .unwrap();
        cache
            .put_draft(&scope, "cli-preflight-retained", CANARY)
            .unwrap();
        cache
            .save_view(
                &scope,
                "cli-preflight-view",
                &json!({"repo":"example/private","id":id}),
            )
            .unwrap();
    }
    drop(cache);
    f.scripted_gh(&protocol(case));
    let before = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let trace_path = f.root.join("git-child-trace.log").display().to_string();
    let mut s = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &f,
        120,
        40,
        &[("GIT_TRACE", trace_path.as_str())],
    );
    s.wait_for_screen("Recent commits — saved versions");
    let trace = fs::read(&trace_path).unwrap();
    assert!(!trace.is_empty());
    choose(&mut s, mouse, "Actions", b"4");
    s.wait_for_screen("example/repo run 123");
    command(&mut s, mouse, "Choose Actions scope");
    s.wait_for_screen("ACTIONS INTENT");
    if mouse {
        s.click_visible_action("[organization]");
    } else {
        s.send_key(b"  ");
    }
    focus_field(&mut s, mouse, "Environment or organization name", 1);
    s.paste("example");
    choose(&mut s, mouse, "[Preview]", b"\x10");
    s.wait_for_screen("READ-ONLY PREVIEW");
    confirm_visible(&mut s, mouse);
    s.wait_for_screen("Actions settings scope selected");
    journal(&mut s, &f, 12);
    command(&mut s, mouse, "Set secret");
    s.wait_for_screen("ACTIONS INTENT");
    s.paste("DEPLOY_TOKEN");
    focus_field(&mut s, mouse, "New secret value", 1);
    s.paste(SECRET);
    focus_field(&mut s, mouse, "Visibility", 1);
    if mouse {
        s.click_visible_action("[selected]");
    } else {
        s.send_key(b" ");
    }
    choose(&mut s, mouse, "[Preview]", b"\x10");
    s.wait_for_screen("READ-ONLY PREVIEW");
    s.wait_for_screen("Selected repositories: none");
    journal(&mut s, &f, 16);
    no_plaintext(&f, &s);
    if mouse && matches!(case, Case::Supported) {
        s.capture_completed_terminal_v1("secret-cli-empty-selection-preview");
    }
    s.resize(80, 24);
    s.wait_for_screen("READ-ONLY PREVIEW");
    confirm_visible(&mut s, mouse);
    if matches!(case, Case::Supported) {
        s.wait_for_screen("Secret metadata confirmed");
        let rows = publication_support::durable_rows(&f.root.join("config/ghtui/cache.sqlite3"));
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].kind, json!("set_secret"));
        assert_eq!(rows[0].state, "confirmed");
        assert_eq!(
            rows[0].target["Secret"]["scope"],
            json!({"Organization":{"organization":"example"}})
        );
    } else {
        s.wait_for_screen("gh 2.75.0 required");
        s.wait_for_screen("--no-repos-selected");
        if mouse {
            s.capture_completed_terminal_v1(if matches!(case, Case::Old) {
                "secret-cli-old-refusal-80x24"
            } else {
                "secret-cli-custom-refusal-80x24"
            });
        }
        assert!(
            publication_support::durable_rows(&f.root.join("config/ghtui/cache.sqlite3"))
                .is_empty()
        );
        scroll_error_to(&mut s, mouse, "Use a reviewed stable build", true);
        choose(&mut s, mouse, "[F8 Error↓]", b"\x1b[19~");
        scroll_error_to(&mut s, mouse, "no secret invocation started", true);
        choose(&mut s, mouse, "[F7 Error↑]", b"\x1b[18~");
        scroll_error_to(
            &mut s,
            mouse,
            if matches!(case, Case::Old) {
                "Current version: 2.74.0"
            } else {
                "Current version: 2.101.0"
            },
            false,
        );
        s.resize(120, 40);
        for _ in 0..6 {
            if s.screen().contains("DEPLOY_TOKEN") {
                break;
            }
            choose(&mut s, mouse, "[F5 Fields↑]", b"\x1b[15~");
            s.poll_output(std::time::Duration::from_millis(20));
        }
        s.wait_for_screen("DEPLOY_TOKEN");
        choose(&mut s, mouse, "[F6 Fields↓]", b"\x1b[17~");
        s.try_wait_for_condition(
            |screen| {
                crate::support::first_form_field_value(screen, "ACTIONS INTENT", "New secret value")
                    == Some("")
            },
            "actual refused form exposes a cleared transient secret field",
            std::time::Duration::from_secs(8),
        )
        .unwrap();
        assert!(!s.screen().contains("ORIGINAL ACTIONS ATTEMPT"));
        close_form(&mut s, mouse);
    }
    exact(&mut s, &f, case);
    no_plaintext(&f, &s);
    assert_saved(&f);
    assert_eq!(fs::read(&trace_path).unwrap(), trace);
    assert_eq!(git_state(&f), before);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    s.quit_and_assert_restored_terminal(mouse);
    exact(&mut s, &f, case);
    no_plaintext(&f, &s);
    assert_saved(&f);
    assert_eq!(git_state(&f), before);
}
#[test]
fn gh_provider_secret_cli_keyboard_old_current_cli_refuses_before_marker() {
    journey(Case::Old, false);
}
#[test]
fn gh_provider_secret_cli_mouse_old_current_cli_refuses_before_marker() {
    journey(Case::Old, true);
}
#[test]
fn gh_provider_secret_cli_keyboard_custom_current_cli_refuses_before_marker() {
    journey(Case::Custom, false);
}
#[test]
fn gh_provider_secret_cli_mouse_custom_current_cli_refuses_before_marker() {
    journey(Case::Custom, true);
}
#[test]
fn gh_provider_secret_cli_keyboard_supported_floor_keeps_exact_empty_selection() {
    journey(Case::Supported, false);
}
#[test]
fn gh_provider_secret_cli_mouse_supported_floor_keeps_exact_empty_selection() {
    journey(Case::Supported, true);
}
