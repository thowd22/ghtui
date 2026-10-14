//! Six exact-image subprocess journeys for active GitHub actor observations.
//! Child of identity.rs; uses real local Git fixture + actual gh/public runners.
use super::*;
use crate::support::GhStep;
use ghtui::{
    config::Config,
    storage::{Cache, Scope},
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde_json::{json, Value};
#[path = "actor_avatar_projection_v1.rs"]
mod bounded_projection_v1;
use std::{
    fs,
    io::Cursor,
    path::Path,
    time::{Duration, Instant},
};

const RED_URL: &str = "https://avatars.githubusercontent.com/u/1?v=4";
const GREEN_URL: &str = "https://avatars.githubusercontent.com/u/2?v=4";
const RED_TITLE: &str = "Exact red avatar row";
const GREEN_TITLE: &str = "Exact green avatar row";
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Ready,
    NoColor,
    AccountUnavailable,
}
fn raw_pr(number: u64, title: &str, url: &str) -> Value {
    json!({"node_id":format!("PR_ACTUAL_AVATAR_{number}"),"id":500+number,"number":number,"title":title,"body":"Actual independent image source","repository_url":"https://api.github.com/repos/org/avatar","html_url":format!("https://github.com/org/avatar/pull/{number}"),"user":{"login":"same-login","name":"Shared Source","type":"User","avatar_url":url},"state":"open","draft":false,"created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z"})
}
fn protocol(mode: Mode) -> Vec<GhStep> {
    let mut steps = crate::support::empty_inbox_steps("octocat");
    for step in steps.iter_mut().skip(4) {
        step.stdout=serde_json::to_vec(&json!({"total_count":2,"incomplete_results":false,"items":[raw_pr(1,RED_TITLE,RED_URL),raw_pr(2,GREEN_TITLE,GREEN_URL)]})).unwrap();
    }
    if mode == Mode::AccountUnavailable {
        steps.push(GhStep::read(
            &["--version"],
            "gh version 2.99.0 (2026-10-01)\n",
        ));
        let mut denied = GhStep::read(
            &["api", "user", "--hostname", "github.com", "--method", "GET"],
            "",
        );
        denied.status = 1;
        denied.stderr = b"gh: authentication required\n".to_vec();
        steps.push(denied);
    }
    assert_eq!(
        steps.len(),
        if mode == Mode::AccountUnavailable {
            9
        } else {
            7
        }
    );
    steps
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct GitState {
    head: String,
    refs: String,
    index: Vec<u8>,
    staged: String,
    dirty: String,
    work: Vec<u8>,
    untracked: Vec<u8>,
}
fn git_state(fixture: &Fixture) -> GitState {
    GitState {
        head: fixture.git(&["rev-parse", "HEAD"]),
        refs: fixture.git(&["show-ref", "--head"]),
        index: fs::read(fixture.repo().join(".git/index")).unwrap(),
        staged: fixture.git(&["diff", "--cached", "--binary"]),
        dirty: fixture.git(&["diff", "--binary"]),
        work: fs::read(fixture.repo().join("readme")).unwrap(),
        untracked: fs::read(fixture.repo().join("avatar-untracked.txt")).unwrap(),
    }
}
fn database(fixture: &Fixture) -> std::path::PathBuf {
    fixture.root.join("config/ghtui/cache.sqlite3")
}
fn trace(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("avatar-git-trace.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .filter(|event| event["event"] == "start")
        .collect()
}
fn assert_read_only_git(fixture: &Fixture) {
    for event in trace(fixture) {
        for arg in event["argv"].as_array().unwrap() {
            assert!(!matches!(
                arg.as_str().unwrap_or(""),
                "add"
                    | "reset"
                    | "commit"
                    | "checkout"
                    | "switch"
                    | "update-ref"
                    | "fetch"
                    | "clone"
                    | "pull"
                    | "push"
                    | "merge"
                    | "rebase"
                    | "stash"
                    | "rm"
            ));
        }
    }
}
fn assert_commands(session: &Session, fixture: &Fixture, count: usize) {
    let journal = session.inspect_command_journal(fixture);
    assert_eq!(journal.lines().count(), count);
    for line in journal.lines() {
        let row: Value = serde_json::from_str(line).unwrap();
        assert_eq!(row["matched"], true);
        assert_eq!(row["stdin_hex"], "");
    }
}
struct Asset {
    release: std::path::PathBuf,
}
fn asset_file(fixture: &Fixture, tag: &str, suffix: &str) -> std::path::PathBuf {
    fixture.root.join(format!("exact-{tag}-{suffix}"))
}
fn assets(fixture: &Fixture) -> Asset {
    for (tag, color) in [("red", [255, 0, 0, 255]), ("green", [0, 255, 0, 255])] {
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba(color)))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        fs::write(asset_file(fixture, tag, "png"), bytes.into_inner()).unwrap();
    }
    let release = fixture.root.join("exact-asset-release");
    let program = fixture.root.join("bin/curl");
    let script=format!("#!/bin/sh\nfor arg; do url=$arg; done\ncase \"$url\" in '{}' ) tag=red ;; '{}' ) tag=green ;; *) exit 92 ;; esac\nprintf '%s' \"$$\" > '{}/exact-'\"$tag\"'-pid'\nprintf '%s\\n' \"$@\" > '{}/exact-'\"$tag\"'-argv'\n/usr/bin/env | /usr/bin/sed 's/=.*//' > '{}/exact-'\"$tag\"'-envkeys'\ntries=0\nwhile [ ! -e '{}' ] && [ \"$tries\" -lt 600 ]; do /bin/sleep 0.01; tries=$((tries + 1)); done\nprintf complete > '{}/exact-'\"$tag\"'-complete'\n/bin/cat '{}/exact-'\"$tag\"'-png'\n",RED_URL,GREEN_URL,fixture.root.display(),fixture.root.display(),fixture.root.display(),release.display(),fixture.root.display(),fixture.root.display());
    fs::write(&program, script).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).unwrap();
    Asset { release }
}
fn ready(session: &mut Session, labels: &[&str]) {
    session
        .try_wait_for_condition(
            |screen| labels.iter().all(|label| screen.contains(label)),
            &format!("Actual exact-actor frame with {labels:?}"),
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
}
fn choose(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label)
    } else {
        session.send_key(key)
    }
}
fn wait_pids(session: &mut Session, fixture: &Fixture) -> [i32; 2] {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let pids = ["red", "green"].map(|tag| {
            fs::read_to_string(asset_file(fixture, tag, "pid"))
                .ok()
                .and_then(|raw| raw.parse::<i32>().ok())
        });
        if let [Some(red), Some(green)] = pids {
            assert!(red > 0 && green > 0 && red != green);
            return [red, green];
        }
        assert!(
            Instant::now() < deadline,
            "Two actual distinct public image child PIDs"
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn assert_reaped(session: &mut Session, pids: [i32; 2]) {
    let deadline = Instant::now() + Duration::from_secs(3);
    for pid in pids {
        loop {
            // SAFETY: PID belongs to this owned fixture's observed public child.
            if unsafe { libc::kill(pid, 0) } == -1 {
                assert_eq!(
                    std::io::Error::last_os_error().raw_os_error(),
                    Some(libc::ESRCH)
                );
                break;
            }
            assert!(
                Instant::now() < deadline,
                "No actual asset child survives retirement"
            );
            session.poll_output(Duration::from_millis(10));
        }
    }
}
fn actual_row_colors(session: &Session, cols: u16, rows: u16) -> bool {
    // Independently parse actual emitted terminal attributes. This is neither a
    // direct App invocation nor proof of Kitty/iTerm/Sixel displayed pixels.
    let mut parser = vt100::Parser::new(rows, cols, 0);
    parser.process(&session.transcript);
    for (title, color) in [
        (RED_TITLE, vt100::Color::Rgb(255, 0, 0)),
        (GREEN_TITLE, vt100::Color::Rgb(0, 255, 0)),
    ] {
        let Some((_, y)) = session.locate(title) else {
            return false;
        };
        let Some(line) = session
            .screen()
            .lines()
            .nth(usize::from(y + 1))
            .map(str::to_owned)
        else {
            return false;
        };
        let Some(x) = line.chars().position(|ch| ch == '▀') else {
            return false;
        };
        let Some(cell) = parser.screen().cell(y + 1, x as u16) else {
            return false;
        };
        if cell.contents() != "▀" || cell.fgcolor() != color || cell.bgcolor() != color {
            return false;
        }
    }
    true
}
fn pixels_ready(session: &mut Session, cols: u16, rows: u16) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !actual_row_colors(session, cols, rows) {
        assert!(
            Instant::now() < deadline,
            "Actual same-login rows must have DIFFERENT exact red/green foreground/background; {}",
            session.screen()
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn assert_asset_journal(fixture: &Fixture, expected: bool) {
    for (tag, url) in [("red", RED_URL), ("green", GREEN_URL)] {
        if !expected {
            assert!(!asset_file(fixture, tag, "pid").exists());
            continue;
        }
        let args = fs::read_to_string(asset_file(fixture, tag, "argv")).unwrap();
        assert!(args.starts_with("-q\n--fail\n"));
        assert!(args.contains("--proto\n=https\n--max-redirs\n0"));
        assert!(args.contains("--max-filesize\n1048576"));
        assert_eq!(args.lines().filter(|line| *line == "--url").count(), 1);
        assert!(args.ends_with(&format!("--url\n{url}\n")));
        assert!(!args.contains("Authorization"));
        let env = fs::read_to_string(asset_file(fixture, tag, "envkeys")).unwrap();
        for forbidden in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GH_ENTERPRISE_TOKEN",
            "GITHUB_ENTERPRISE_TOKEN",
            "GH_HOST",
            "GH_CONFIG_DIR",
            "CURL_HOME",
            "HTTP_PROXY",
            "HTTPS_PROXY",
        ] {
            assert!(!env.lines().any(|key| key == forbidden));
        }
    }
}
fn assert_durable_intent(fixture: &Fixture, saved: &Value) {
    let cache = Cache::open(&database(fixture)).unwrap();
    let scope = Scope::github("github.com", Some(42)).unwrap();
    assert_eq!(
        cache
            .draft(&scope, "exact-avatar-draft")
            .unwrap()
            .as_deref(),
        Some("Original42 unsent text")
    );
    assert_eq!(
        cache.saved_views(&scope).unwrap(),
        vec![("exact-avatar-intent".into(), saved.clone())]
    );
    let other = Scope::github("github.com", Some(43)).unwrap();
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.saved_views(&other).unwrap().is_empty());
    assert!(cache.draft(&other, "exact-avatar-draft").unwrap().is_none());
}
fn journey(mouse: bool, mode: Mode) {
    let fixture = fixture();
    fs::write(
        fixture.repo().join("avatar-untracked.txt"),
        b"Keep my untracked bytes\n",
    )
    .unwrap();
    fixture.scripted_gh(&protocol(mode));
    let config_path = fixture.root.join("config/ghtui/config.toml");
    let config = Config {
        refresh_seconds: 3600,
        avatars: true,
        ..Default::default()
    };
    config.save(&config_path).unwrap();
    let config_before = fs::read(&config_path).unwrap();
    let git_before = git_state(&fixture);
    let saved = json!({"kind":"observed navigation intent","private_original":"Keep42 exact source selection"});
    {
        let cache = Cache::open(&database(&fixture)).unwrap();
        let scope = Scope::github("github.com", Some(42)).unwrap();
        cache
            .put_draft(&scope, "exact-avatar-draft", "Original42 unsent text")
            .unwrap();
        cache
            .save_view(&scope, "exact-avatar-intent", &saved)
            .unwrap();
    }
    let assets = assets(&fixture);
    let trace_path = fixture.root.join("avatar-git-trace.jsonl");
    let mut env = vec![("GIT_TRACE2_EVENT", trace_path.to_str().unwrap())];
    if mode == Mode::NoColor {
        env.push(("NO_COLOR", "1"));
    }
    let (cols, rows) = if mouse { (80, 24) } else { (120, 40) };
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        cols,
        rows,
        &env,
    );
    ready(
        &mut session,
        &[
            "Recent commits",
            "author Different Author",
            "committer Current Committer",
        ],
    );
    choose(&mut session, mouse, "1 Inbox", b"1");
    ready(&mut session, &["Inbox • focused view"]);
    choose(&mut session, mouse, "[r Refresh]", b"r");
    ready(
        &mut session,
        &[RED_TITLE, GREEN_TITLE, "@same-login", "[SS]"],
    );
    session.wait_for_command_count(&fixture, 7);
    assert_commands(&session, &fixture, 7);
    let loaded_records = Cache::open(&database(&fixture))
        .unwrap()
        .list(&Scope::github("github.com", Some(42)).unwrap(), None)
        .unwrap();
    let loaded_trace = trace(&fixture);
    let pids = if mode == Mode::NoColor {
        None
    } else {
        Some(wait_pids(&mut session, &fixture))
    };
    choose(&mut session, mouse, "[c Created by me]", b"c");
    ready(
        &mut session,
        &["Queue: Created by me", RED_TITLE, GREEN_TITLE],
    );
    let (new_cols, new_rows) = if mouse { (120, 40) } else { (80, 24) };
    session.resize(new_cols, new_rows);
    ready(&mut session, &[RED_TITLE, GREEN_TITLE]);
    assert_commands(&session, &fixture, 7);
    assert_eq!(trace(&fixture), loaded_trace);
    assert_durable_intent(&fixture, &saved);
    if mode == Mode::AccountUnavailable {
        choose(&mut session, mouse, "[r Refresh]", b"r");
        session.wait_for_command_count(&fixture, 9);
        session
            .try_wait_for_condition(
                |screen| {
                    screen.contains("Identity unavailable")
                        && screen.contains("private data hidden")
                        && !screen.contains(RED_TITLE)
                        && !screen.contains(GREEN_TITLE)
                        && !screen.contains("@same-login")
                        && !screen.contains('▀')
                },
                "Actual accountNone privacy frame drops both previous exact images/actors",
                Duration::from_secs(8),
            )
            .unwrap_or_else(|error| panic!("{error}"));
        assert_reaped(&mut session, pids.unwrap());
        for tag in ["red", "green"] {
            assert!(!asset_file(&fixture, tag, "complete").exists());
        }
        assert!(!fixture.root.join("config/ghtui/avatars").exists());
        assert_asset_journal(&fixture, true);
        assert_commands(&session, &fixture, 9);
    } else if mode == Mode::NoColor {
        assert!(!session.screen().contains('▀'));
        assert_asset_journal(&fixture, false);
        assert!(!fixture.root.join("config/ghtui/avatars").exists());
    } else {
        fs::write(
            &assets.release,
            b"Explicit release after responsive actual keyboard/mouse filter and resize",
        )
        .unwrap();
        pixels_ready(&mut session, new_cols, new_rows);
        assert_reaped(&mut session, pids.unwrap());
        assert_asset_journal(&fixture, true);
        choose(&mut session, mouse, "[a All]", b"a");
        ready(&mut session, &["Queue: All", RED_TITLE, GREEN_TITLE]);
        pixels_ready(&mut session, new_cols, new_rows);
        assert_asset_journal(&fixture, true);
        assert_commands(&session, &fixture, 7);
    }
    if mode != Mode::AccountUnavailable {
        assert_eq!(
            Cache::open(&database(&fixture))
                .unwrap()
                .list(&Scope::github("github.com", Some(42)).unwrap(), None)
                .unwrap(),
            loaded_records,
            "Image and pure queue navigation do not write canonical SQL frames"
        );
    }
    assert_eq!(trace(&fixture), loaded_trace);
    assert_durable_intent(&fixture, &saved);
    choose(&mut session, mouse, "2 Repository", b"2");
    ready(
        &mut session,
        &[
            "Repository • focused view",
            "author Different Author",
            "committer Current Committer",
        ],
    );
    session
        .try_wait_for_condition(
            |screen| {
                screen.contains("Repository • focused view")
                    && screen.contains("author Different Author")
                    && screen.contains("committer Current Committer")
                    && !screen.contains("@same-login")
                    && !screen.contains('▀')
            },
            "Complete local actor frame has erased the previous remote actor/image cells",
            Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}"));
    assert!(!session.screen().contains("@same-login"));
    assert!(!session.screen().contains('▀'));
    session.focus_visible_action("[q Quit]");
    assert_commands(
        &session,
        &fixture,
        if mode == Mode::AccountUnavailable {
            9
        } else {
            7
        },
    );
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(git_state(&fixture), git_before);
    assert_read_only_git(&fixture);
    assert_durable_intent(&fixture, &saved);
    assert_eq!(fs::read(&config_path).unwrap(), config_before);
}
#[test]
fn actor_avatar_exact_keyboard_same_login_different_url_rows_use_two_current_images() {
    journey(false, Mode::Ready)
}
#[test]
fn actor_avatar_exact_mouse_same_login_different_url_rows_use_two_current_images() {
    journey(true, Mode::Ready)
}
#[test]
fn actor_avatar_exact_keyboard_no_color_keeps_both_source_identities_without_image_child() {
    journey(false, Mode::NoColor)
}
#[test]
fn actor_avatar_exact_mouse_no_color_keeps_both_source_identities_without_image_child() {
    journey(true, Mode::NoColor)
}
#[test]
fn actor_avatar_exact_keyboard_account_none_cancels_two_cold_children_preserves_original_intent() {
    journey(false, Mode::AccountUnavailable)
}
#[test]
fn actor_avatar_exact_mouse_account_none_cancels_two_cold_children_preserves_original_intent() {
    journey(true, Mode::AccountUnavailable)
}
