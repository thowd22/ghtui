//! Strict native catalogue avatar journeys through the genuine source factory.
//! Actual execution and integrated checkpoint evidence are recorded in Backlog.
use super::*;
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::io::Cursor;

const ASSET_URL: &str = "https://avatars.githubusercontent.com/u/71?v=4";
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Ready,
    Failed,
    Monochrome,
    Retired,
}

fn asset_repository(index: u64) -> Value {
    let mut row = repository(index);
    row["owner"]["name"] = json!("Example Org");
    row["owner"]["avatar_url"] = json!(ASSET_URL);
    row
}
fn asset_protocol(mode: Mode) -> Vec<GhStep> {
    let mut steps = collection(vec![json!([asset_repository(0), asset_repository(1)])], 42);
    if mode == Mode::Retired {
        steps.extend(collection(vec![json!([asset_repository(0)])], 43));
    }
    assert_eq!(steps.len(), if mode == Mode::Retired { 14 } else { 7 });
    steps
}
struct PublicAsset {
    release: std::path::PathBuf,
    argv: std::path::PathBuf,
    env_keys: std::path::PathBuf,
    pid: std::path::PathBuf,
    complete: std::path::PathBuf,
}
fn public_asset(fixture: &Fixture, mode: Mode) -> PublicAsset {
    let image = fixture.root.join("catalogue-owner.png");
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_fn(2, 2, |_, y| {
        if y == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    }))
    .write_to(&mut bytes, ImageFormat::Png)
    .unwrap();
    fs::write(&image, bytes.into_inner()).unwrap();
    let files = PublicAsset {
        release: fixture.root.join("owner-release"),
        argv: fixture.root.join("owner-argv"),
        env_keys: fixture.root.join("owner-env-keys"),
        pid: fixture.root.join("owner-pid"),
        complete: fixture.root.join("owner-complete"),
    };
    let effect = if mode == Mode::Failed {
        "printf 'offline asset unavailable\\n' >&2\nexit 22\n".into()
    } else {
        format!("/bin/cat '{}'\n", image.display())
    };
    // This isolated executable serves local bytes only. Its explicit release
    // gate is controlled provider latency, not an arbitrary readiness sleep.
    let script=format!("#!/bin/sh\nprintf '%s' \"$$\" > '{}'\nprintf '%s\\n' \"$@\" >> '{}'\n/usr/bin/env | /usr/bin/sed 's/=.*//' > '{}'\ntries=0\nwhile [ ! -e '{}' ] && [ \"$tries\" -lt 600 ]; do /bin/sleep 0.01; tries=$((tries + 1)); done\nprintf complete > '{}'\n{}",files.pid.display(),files.argv.display(),files.env_keys.display(),files.release.display(),files.complete.display(),effect);
    let program = fixture.root.join("bin/curl");
    crate::fixture_shell_publication_v1::install(&program, script.as_bytes()).unwrap();
    files
}
fn launch_assets(fixture: &Fixture, mouse: bool, mode: Mode) -> Session {
    let trace = fixture.root.join("catalogue-git-trace.jsonl");
    let mut env = vec![("GIT_TRACE2_EVENT", trace.to_str().unwrap())];
    if mode == Mode::Monochrome {
        if mouse {
            env.push(("TERM", "dumb"));
        } else {
            env.push(("NO_COLOR", "1"));
        }
    }
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        fixture,
        if mouse { 80 } else { 120 },
        if mouse { 24 } else { 40 },
        &env,
    );
    ready(
        &mut session,
        &["Repository • focused view", "Recent commits"],
    );
    session
}
fn wait_asset_started(session: &mut Session, files: &PublicAsset) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(raw) = fs::read_to_string(&files.pid) {
            if let Ok(pid) = raw.parse::<i32>() {
                assert!(pid > 0);
                return pid;
            }
        }
        assert!(
            Instant::now() < deadline,
            "Actual isolated public avatar child must start"
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn wait_asset_completed(session: &mut Session, files: &PublicAsset) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !files.complete.exists() {
        assert!(
            Instant::now() < deadline,
            "Actual public asset child completion"
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn wait_asset_reaped(session: &mut Session, pid: i32) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        // SAFETY: this positive PID came from the owned fixture's actual image
        // child, never a user process or an unresolved environment variable.
        if unsafe { libc::kill(pid, 0) } == -1 {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Actual isolated public asset PID must be reaped"
        );
        session.poll_output(Duration::from_millis(10));
    }
}
fn assert_public_asset(files: &PublicAsset, expected: usize) {
    let args = fs::read_to_string(&files.argv).unwrap_or_default();
    assert_eq!(
        args.lines().filter(|line| *line == "--url").count(),
        expected,
        "Actual public requests, not GH or per-frame repeated fetches"
    );
    if expected > 0 {
        assert!(args.starts_with("-q\n--fail\n"));
        assert!(args.contains("--proto\n=https\n--max-redirs\n0"));
        assert!(args.contains("--max-filesize\n1048576"));
        assert!(args.ends_with(&format!("--url\n{ASSET_URL}\n")));
        assert!(!args.lines().any(|line| line == "--user"
            || line == "--netrc"
            || line == "--cookie"
            || line.contains("Authorization")));
        let env = fs::read_to_string(&files.env_keys).unwrap();
        for name in [
            "GH_TOKEN",
            "GITHUB_TOKEN",
            "GH_ENTERPRISE_TOKEN",
            "GITHUB_ENTERPRISE_TOKEN",
            "GH_HOST",
            "GH_CONFIG_DIR",
            "CURL_HOME",
            "HTTPS_PROXY",
            "HTTP_PROXY",
        ] {
            assert!(
                !env.lines().any(|line| line == name),
                "Public asset child must not inherit {name}"
            );
        }
    } else {
        assert!(
            !files.pid.exists(),
            "No image child on unsupported/no-color surface"
        );
    }
}
fn assert_local_state(fixture: &Fixture, config: &[u8], before: &GitState) {
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    assert_eq!(git_state(fixture), *before);
    assert_read_only_git(fixture);
    let cache = Cache::open(&database(fixture)).unwrap();
    let original = Scope::github(HOST, Some(42)).unwrap();
    assert_eq!(
        cache.draft(&original, "catalogue-draft").unwrap(),
        Some("Original42 draft".into())
    );
    assert!(
        cache.saved_views(&original).unwrap().is_empty(),
        "Select/pin/avatar is not Save/activation authority"
    );
    let other = Scope::github(HOST, Some(43)).unwrap();
    assert!(cache.saved_views(&other).unwrap().is_empty());
    assert!(cache.list(&other, None).unwrap().is_empty());
    assert!(cache.draft(&other, "catalogue-draft").unwrap().is_none());
}
fn owner_detail(session: &mut Session, mouse: bool) {
    let point = session
        .locate("Repository ID: 10000")
        .expect("Actual selected detail geometry before owner scrolling");
    if mouse {
        session.click(point.0, point.1);
    } else {
        // Filter() ends in the list. Tab moves into the real detail panel;
        // one-line arrows cannot skip an owner as a wide-screen page might.
        session.send_key(b"\t");
    }
    for _ in 0..32 {
        session.poll_output(Duration::from_millis(20));
        if session.screen().contains("Owner: @example [organization]") {
            break;
        }
        if mouse {
            session.wheel(point.0, point.1, true);
        } else {
            session.send_key(b"\x1b[B");
        }
    }
    ready(session, &["Owner: @example [organization]"]);
    for _ in 0..32 {
        session.poll_output(Duration::from_millis(20));
        if session
            .screen()
            .contains("Full repository name: example/repo0000")
        {
            break;
        }
        if mouse {
            session.wheel(point.0, point.1, false);
        } else {
            session.send_key(b"\x1b[A");
        }
    }
    ready(
        session,
        &[
            "Repository ID: 10000",
            "Full repository name: example/repo0000",
        ],
    );
    if mouse {
        session.click_visible_action("◆ repository · example/repo0000");
    } else {
        session.send_key(b"\x1b[Z");
    }
}
fn image_at_current_row(session: &mut Session) {
    ready(session, &[TITLE, FIRST, "Repository ID: 10000", "▀"]);
    let raw = String::from_utf8_lossy(&session.transcript);
    assert!(raw.contains("38;2;255;0;0"));
    assert!(raw.contains("48;2;0;0;255"));
    let (_, row) = session.locate("◆ repository · example/repo0000").unwrap();
    let pixel = session
        .screen()
        .lines()
        .nth(usize::from(row))
        .unwrap()
        .contains('▀');
    assert!(
        pixel,
        "Ready bytes belong to current genuine repository row"
    );
}
fn journey(mouse: bool, mode: Mode) {
    journey_with_writer(mouse, mode, false);
}
fn journey_with_writer(mouse: bool, mode: Mode, hold_writer: bool) {
    let fixture = configured(&asset_protocol(mode));
    let path = fixture.root.join("config/ghtui/config.toml");
    let mut config = Config::load(&path).unwrap();
    config.avatars = true;
    config.save(&path).unwrap();
    let config_before = fs::read(&path).unwrap();
    let git_before = git_state(&fixture);
    let files = public_asset(&fixture, mode);
    #[cfg(target_os = "linux")]
    let mut inherited_writer = hold_writer.then(|| {
        let data = crate::fixture_shell_publication_v1::data_path(&fixture.root.join("bin/curl"));
        let holder =
            crate::support::inherited_fixture_writer_v1::HeldFixtureWriter::hold(&[&data]).unwrap();
        assert!(holder.pid() > 0);
        holder
    });
    let _ = hold_writer;
    let mut session = launch_assets(&fixture, mouse, mode);
    open_catalogue(&mut session, &fixture, mouse, 0);
    discover(&mut session, mouse);
    ready(
        &mut session,
        &[TITLE, "Observed: 2", FIRST, "Repository ID: 10000", "[EO]"],
    );
    assert_commands(&session, &fixture, 7);
    let trace = git_commands(&fixture);
    let loaded_records = Cache::open(&database(&fixture))
        .unwrap()
        .list(&Scope::github(HOST, Some(42)).unwrap(), None)
        .unwrap();
    let pending = if mode == Mode::Monochrome {
        None
    } else {
        Some(wait_asset_started(&mut session, &files))
    };
    // Meaningful live filter/selection and latest resized row hits while public
    // work is still pending. No need to await image bytes to navigate.
    filter(
        &mut session,
        mouse,
        "repo0000",
        FIRST,
        "Repository ID: 10000",
    );
    choose(&mut session, mouse, "[Space Select]", b" ");
    choose(&mut session, mouse, "[p Pin]", b"p");
    ready(&mut session, &["Selected: yes", "Pinned: yes"]);
    owner_detail(&mut session, mouse);
    session.resize_owned_modal(
        if mouse { 120 } else { 80 },
        if mouse { 40 } else { 24 },
        TITLE,
        &["[q Quit]", "[Esc Close]"],
    );
    ready(
        &mut session,
        &[
            TITLE,
            FIRST,
            "Repository ID: 10000",
            "Selected: yes",
            "Pinned: yes",
        ],
    );
    owner_detail(&mut session, mouse);
    assert_commands(&session, &fixture, 7);
    assert_eq!(git_commands(&fixture), trace);
    assert_local_state(&fixture, &config_before, &git_before);
    assert_eq!(
        Cache::open(&database(&fixture))
            .unwrap()
            .list(&Scope::github(HOST, Some(42)).unwrap(), None)
            .unwrap(),
        loaded_records,
        "Pure avatar/navigation work preserves admitted original SQL records"
    );
    if mode == Mode::Retired {
        choose(&mut session, mouse, "[r Retry]", b"r");
        session.wait_for_command_count(&fixture, 14);
        ready(
            &mut session,
            &[
                TITLE,
                "private repositories hidden",
                "original saved intent/drafts retained",
            ],
        );
        assert!(!session.screen().contains(FIRST));
        assert!(!session.screen().contains("@example"));
        assert!(!session.screen().contains("▀"));
        assert!(!session.screen().contains("Observed: 2"));
        wait_asset_reaped(&mut session, pending.unwrap());
        assert!(!files.complete.exists());
        assert!(!fixture.root.join("config/ghtui/avatars").exists());
        assert_public_asset(&files, 1);
        assert_commands(&session, &fixture, 14);
        choose(&mut session, mouse, "[Esc Close]", b"\x1b");
        session.wait_for_overlay_close(TITLE);
        session.wait_for_main_frame();
        ready(
            &mut session,
            &["Repository • focused view", "Recent commits"],
        );
        finish(
            &mut session,
            &fixture,
            mouse,
            14,
            &git_before,
            &config_before,
        );
        assert_local_state(&fixture, &config_before, &git_before);
        return;
    }
    if mode == Mode::Monochrome {
        ready(&mut session, &["[EO]"]);
        assert!(!session.screen().contains("▀"));
        assert_public_asset(&files, 0);
        assert!(!fixture.root.join("config/ghtui/avatars").exists());
    } else {
        fs::write(&files.release, b"explicit fixture image release").unwrap();
        wait_asset_completed(&mut session, &files);
        wait_asset_reaped(&mut session, pending.unwrap());
        if mode == Mode::Ready {
            image_at_current_row(&mut session);
        } else {
            ready(&mut session, &["[EO]"]);
            assert!(!session.screen().contains("▀"));
        }
        assert_public_asset(&files, 1);
    }
    // Moving between genuine rows of the same owner cannot repeat the image
    // request or turn the request's login into a current permission receipt.
    filter(
        &mut session,
        mouse,
        "repo0001",
        "example/repo0001",
        "Repository ID: 10001",
    );
    ready(
        &mut session,
        &[TITLE, "example/repo0001", "Repository ID: 10001"],
    );
    filter(
        &mut session,
        mouse,
        "repo0000",
        FIRST,
        "Repository ID: 10000",
    );
    if mode == Mode::Ready {
        image_at_current_row(&mut session);
    } else {
        ready(&mut session, &["[EO]"]);
    }
    assert_public_asset(&files, if mode == Mode::Monochrome { 0 } else { 1 });
    assert_commands(&session, &fixture, 7);
    assert_eq!(git_commands(&fixture), trace);
    assert_local_state(&fixture, &config_before, &git_before);
    assert_eq!(
        Cache::open(&database(&fixture))
            .unwrap()
            .list(&Scope::github(HOST, Some(42)).unwrap(), None)
            .unwrap(),
        loaded_records,
        "Image fetch/decode/filter does not write canonical SQLite records"
    );
    // These journeys deliberately made unsaved Select/Pin edits. Exercise the
    // genuine non-destructive Back and explicit local Discard & Close workflow.
    super::read_limits_v1::discard_close(&mut session, mouse);
    ready(
        &mut session,
        &["Repository • focused view", "Recent commits"],
    );
    finish(
        &mut session,
        &fixture,
        mouse,
        7,
        &git_before,
        &config_before,
    );
    assert_local_state(&fixture, &config_before, &git_before);
    assert_public_asset(&files, if mode == Mode::Monochrome { 0 } else { 1 });
    #[cfg(target_os = "linux")]
    if let Some(holder) = inherited_writer.as_mut() {
        holder.finish().unwrap();
    }
}

#[test]
fn test_framework_fixture_shell_publication_v1_keyboard_avatar_stable_launcher_exact_original_effects(
) {
    journey_with_writer(false, Mode::Ready, true);
}
#[test]
fn test_framework_fixture_shell_publication_v1_mouse_avatar_stable_launcher_exact_original_effects()
{
    journey_with_writer(true, Mode::Ready, true);
}

#[test]
fn repo_catalog_avatar_keyboard_delayed_public_png_actual_owner_original_rows() {
    journey(false, Mode::Ready);
}
#[test]
fn repo_catalog_avatar_mouse_delayed_public_png_actual_owner_original_rows() {
    journey(true, Mode::Ready);
}
#[test]
fn repo_catalog_avatar_keyboard_failed_asset_honest_owner_fallback() {
    journey(false, Mode::Failed);
}
#[test]
fn repo_catalog_avatar_mouse_failed_asset_honest_owner_fallback() {
    journey(true, Mode::Failed);
}
#[test]
fn repo_catalog_avatar_keyboard_no_color_no_image_child_keeps_owner_and_hits() {
    journey(false, Mode::Monochrome);
}
#[test]
fn repo_catalog_avatar_mouse_unsupported_terminal_no_image_child_keeps_owner_and_hits() {
    journey(true, Mode::Monochrome);
}
#[test]
fn repo_catalog_avatar_keyboard_uid43_source_retirement_reaps_pending_public_child() {
    journey(false, Mode::Retired);
}
#[test]
fn repo_catalog_avatar_mouse_uid43_source_retirement_reaps_pending_public_child() {
    journey(true, Mode::Retired);
}
