//! Install the real ghtui executable with scripts/install.sh from an offline
//! file:// release, then drive the INSTALLED binary in a PTY.
use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

#[path = "../release_support/stand_in.rs"]
mod stand_in;

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn run(command: &mut Command) -> String {
    let output = command.output().unwrap();
    let text = output_text(&output);
    assert!(output.status.success(), "{command:?} failed: {text}");
    text
}

fn output_text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// The release installed into a fixture: the installed path, the exact
/// bytes inside the published archive, and where the release lives.
struct Installed {
    path: PathBuf,
    released: Vec<u8>,
    release: PathBuf,
    version: String,
    target: &'static str,
}

fn installer(fixture: &Fixture, release: &Path, args: &[&str]) -> Output {
    Command::new("bash")
        .arg(release.join("latest/download/install.sh"))
        .args(args)
        .env_clear()
        .env("HOME", fixture.root.join("home"))
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", fixture.root.join("cache"))
        .env(
            "GHTUI_RELEASE_BASE",
            format!("file://{}", release.display()),
        )
        .output()
        .unwrap()
}

fn publish(release: &Path, version: &str, dist: &Path, latest: bool) {
    let mut layouts = vec![format!("download/{version}")];
    if latest {
        layouts.push("latest/download".into());
    }
    for layout in layouts {
        let directory = release.join(layout);
        fs::create_dir_all(&directory).unwrap();
        for entry in fs::read_dir(dist).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), directory.join(entry.file_name())).unwrap();
        }
    }
}

fn package(version: &str, target: &str, binary: &Path, dist: &Path) {
    let packager = Path::new(ROOT).join("scripts/package-release.sh");
    run(Command::new("bash")
        .arg(&packager)
        .args(["archive", version, target])
        .arg(binary)
        .arg(dist));
    run(Command::new("bash")
        .arg(&packager)
        .args(["finish", version])
        .arg(dist));
}

/// Package the test build of ghtui as this crate's version, publish it in
/// the GitHub release layout and install it into the fixture's ~/.local/bin.
/// With GHTUI_E2E_RELEASE_DIST naming a directory of assembled release
/// assets (the release workflow's dist), those exact assets are installed
/// instead, so the journeys drive the artifact that will be published.
/// There is no fallback to the development binary: the journeys launch the
/// installed path, whose bytes, version and target are checked here.
fn install_real_binary(fixture: &Fixture) -> Installed {
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let target = stand_in::Arch::host().linux_target();
    let dist = match std::env::var_os("GHTUI_E2E_RELEASE_DIST") {
        Some(dist) if !dist.is_empty() => PathBuf::from(dist),
        _ => {
            let dist = fixture.root.join("dist");
            package(
                &version,
                target,
                Path::new(env!("CARGO_BIN_EXE_ghtui")),
                &dist,
            );
            dist
        }
    };
    let archive = dist.join(format!("ghtui-{version}-{target}.tar.gz"));
    let released = Command::new("tar")
        .arg("-xzOf")
        .arg(&archive)
        .arg("ghtui")
        .output()
        .unwrap();
    assert!(released.status.success(), "cannot read {archive:?}");
    let release = fixture.root.join("release");
    publish(&release, &version, &dist, true);
    let output = installer(fixture, &release, &[]);
    let log = output_text(&output);
    assert!(output.status.success(), "{log}");
    let installed = fixture.root.join("home/.local/bin/ghtui");
    assert!(
        log.contains(&format!(
            "installed ghtui {version} at {}",
            installed.display()
        )),
        "{log}"
    );
    let installed = Installed {
        path: installed,
        released: released.stdout,
        release,
        version,
        target,
    };
    assert_installed_identity(&installed);
    installed
}

/// Independent oracles on the installed file: exact archive bytes, mode,
/// the ELF machine of this host, and the version it reports.
fn assert_installed_identity(installed: &Installed) {
    use std::os::unix::fs::PermissionsExt;
    let bytes = fs::read(&installed.path).unwrap();
    assert_eq!(
        bytes, installed.released,
        "installed bytes differ from the released binary"
    );
    assert_eq!(
        fs::metadata(&installed.path).unwrap().permissions().mode() & 0o777,
        0o755
    );
    let machine = u16::from_le_bytes([bytes[18], bytes[19]]);
    let expected = match installed.target {
        "x86_64-unknown-linux-musl" => 62,
        _ => 183,
    };
    assert_eq!(&bytes[..4], b"\x7fELF");
    assert_eq!(machine, expected, "installed binary is for another CPU");
    let reported = run(Command::new(&installed.path).arg("--version"));
    assert_eq!(
        reported.trim(),
        format!("ghtui {}", &installed.version[1..])
    );
}

/// A committed repository with a dirty working file, so the journeys can
/// check that browsing the installed binary changes no Git state.
fn release_fixture() -> Fixture {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("notes.txt", b"released\n", "Installed release history");
    fs::write(fixture.repo().join("notes.txt"), b"dirty installed bytes\n").unwrap();
    fixture
}

/// Save a configuration file where the installed binary reads it, so a
/// journey can check its exact bytes stay unchanged.
fn seed_config(directory: &Path) -> Vec<u8> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(directory)
        .unwrap();
    let path = directory.join("config.toml");
    ghtui::config::Config::load(&path)
        .unwrap()
        .save(&path)
        .unwrap();
    let bytes = fs::read(&path).unwrap();
    assert!(!bytes.is_empty(), "the seeded configuration is empty");
    bytes
}

/// Every regular file below `root` with its exact bytes.
fn tree_bytes(root: &Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    let mut files = std::collections::BTreeMap::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() {
                pending.push(entry.path());
            } else {
                files.insert(entry.path(), fs::read(entry.path()).unwrap());
            }
        }
    }
    files
}

/// Every strict fake-gh record matched its scripted read; none is a write.
fn assert_read_only_journal(session: &Session, fixture: &Fixture, count: usize) {
    let journal = session.inspect_command_journal(fixture);
    let records: Vec<serde_json::Value> = journal
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), count, "{journal}");
    for record in &records {
        assert_eq!(record["matched"], true, "{record}");
        let args: Vec<&str> = record["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|arg| arg.as_str().unwrap())
            .collect();
        assert!(
            args == ["--version"] || args.windows(2).any(|pair| pair == ["--method", "GET"]),
            "an installed binary sent a non-read gh command: {args:?}"
        );
    }
}

/// The empty personal Inbox load, with the first queue read held so the
/// loading state is on screen before the load completes.
fn delayed_inbox_steps() -> Vec<GhStep> {
    let mut steps = empty_inbox_steps("octocat");
    assert_eq!(steps.len(), 7);
    steps[4].delay_ms = 1_500;
    steps
}

/// Each top-level view: its header control, its key, and text only that
/// view paints for this fixture (no GitHub scope, a local checkout).
const VIEWS: [(&str, &str, &[u8], &str); 5] = [
    (
        "Inbox",
        "1 Inbox",
        b"1",
        "Use Refresh to load your authenticated GitHub inbox.",
    ),
    ("Search", "3 Search", b"3", "Exact mode"),
    (
        "Actions",
        "4 Actions",
        b"4",
        "Choose a repository scope before loading Actions.",
    ),
    ("Dashboard", "5 Dashboard", b"5", "Dashboard scope needed"),
    (
        "Repository",
        "2 Repository",
        b"2",
        "Installed release history",
    ),
];

#[test]
fn v1_release_keyboard_installed_binary_tours_every_view_resize_focus_and_loading() {
    installed_tour(false);
}

#[test]
fn v1_release_mouse_installed_binary_tours_every_view_resize_focus_and_loading() {
    installed_tour(true);
}

/// The installed binary, at 140x42 and 80x24: every top-level view by key
/// or by header click, the Inbox loading state, a pane focus change and
/// back. GitHub sees only matched reads, and Git and the configuration
/// are unchanged.
fn installed_tour(mouse: bool) {
    let fixture = release_fixture();
    let installed = install_real_binary(&fixture);
    let config = seed_config(&fixture.root.join("config/ghtui"));
    fixture.scripted_gh(&delayed_inbox_steps());
    let before = git_state(&fixture);
    let mut session = Session::launch_binary(&installed.path, &fixture, 140, 42, &[]);
    session.assert_focused_view("Repository");
    session.wait_for_screen("Installed release history");
    for (view, label, key, text) in VIEWS {
        session.choose(mouse, label, key);
        session.assert_focused_view(view);
        session.wait_for_screen(text);
        session.resize(80, 24);
        assert_eq!(session.size(), (80, 24));
        session.assert_focused_view(view);
        session.wait_for_screen(text);
        session.resize(140, 42);
        assert_eq!(session.size(), (140, 42));
        session.assert_focused_view(view);
        session.wait_for_screen(text);
    }
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "browsing the views sent a gh command"
    );

    // Focus moves from the Repository list to its Details preview and back.
    session.wait_for_settled_footer();
    if mouse {
        let (column, row) = session.locate("Details").expect("Details preview pane");
        session.click(column, row + 6);
    } else {
        session.send_key(b"\t");
    }
    session.wait_for_screen("Preview focused");
    session.wait_for_screen("▶ Details");
    if mouse {
        let (column, row) = session
            .locate("Recent commits — saved versions")
            .expect("Repository list pane");
        session.click(column, row + 8);
    } else {
        session.send_key(b"\x1b[Z");
        session.wait_for_screen("Repository list focused");
    }
    if let Err(error) = session.try_wait_for_condition(
        |screen| !screen.contains("▶ Details") && screen.contains("Installed release history"),
        "focus returned to the Repository list",
        std::time::Duration::from_secs(8),
    ) {
        session.save_artifacts();
        panic!("{error}");
    }

    // Loading state: the held queue read keeps the Inbox load visible.
    session.choose(mouse, "1 Inbox", b"1");
    session.assert_focused_view("Inbox");
    session.wait_for_settled_footer();
    session.choose(mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Loading Inbox");
    session.wait_for_command_count(&fixture, 7);
    session.wait_for_screen("0 unique PRs across 0 queue matches");
    session.wait_for_settled_footer();
    assert_read_only_journal(&session, &fixture, 7);
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(
        git_state(&fixture),
        before,
        "touring the installed binary changed Git state"
    );
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config,
        "touring the installed binary rewrote its configuration"
    );
    assert!(
        fixture.root.join("config/ghtui/cache.sqlite3").is_file(),
        "the completed Inbox load stored nothing in the private cache"
    );
}

#[test]
fn v1_release_keyboard_installed_binary_reports_missing_gh_and_missing_git() {
    missing_tools(false);
}

#[test]
fn v1_release_mouse_installed_binary_reports_missing_gh_and_missing_git() {
    missing_tools(true);
}

/// Error states of the installed binary with a dependency missing from
/// PATH. Without gh, local Git keeps working and the Inbox refresh says
/// how to install gh; without git, the Repository view says Git could not
/// start. Neither run touches the repository or the configuration.
fn missing_tools(mouse: bool) {
    let fixture = release_fixture();
    let installed = install_real_binary(&fixture);
    let config = seed_config(&fixture.root.join("config/ghtui"));
    let before = git_state(&fixture);
    let host_git = std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|directory| directory.join("git"))
        .find(|path| path.is_file())
        .expect("installed git");

    // gh missing: a PATH that holds only git, never a developer's real gh.
    let only_git = fixture.root.join("only-git");
    fs::create_dir(&only_git).unwrap();
    std::os::unix::fs::symlink(&host_git, only_git.join("git")).unwrap();
    let path = only_git.display().to_string();
    let mut session = Session::launch_binary(
        &installed.path,
        &fixture,
        80,
        24,
        &[("PATH", path.as_str())],
    );
    session.assert_focused_view("Repository");
    session.wait_for_screen("Installed release history");
    session.choose(mouse, "1 Inbox", b"1");
    session.assert_focused_view("Inbox");
    session.wait_for_settled_footer();
    session.choose(mouse, "[r Refresh]", b"r");
    session.wait_for_screen("GitHub CLI is missing");
    session.resize(140, 42);
    session.wait_for_screen("Install gh and run gh auth login");
    session.choose(mouse, "2 Repository", b"2");
    session.assert_focused_view("Repository");
    session.wait_for_screen("Installed release history");
    session.quit_and_assert_restored_terminal(mouse);
    assert!(session.inspect_command_journal(&fixture).is_empty());

    // git missing: a PATH that holds only the strict fake gh.
    let only_gh = fixture.root.join("only-gh");
    fs::create_dir(&only_gh).unwrap();
    std::os::unix::fs::symlink(
        fs::read_link(fixture.root.join("bin/gh")).unwrap(),
        only_gh.join("gh"),
    )
    .unwrap();
    let path = only_gh.display().to_string();
    let mut session = Session::launch_binary(
        &installed.path,
        &fixture,
        140,
        42,
        &[("PATH", path.as_str())],
    );
    session.assert_focused_view("Inbox");
    session.choose(mouse, "2 Repository", b"2");
    session.assert_focused_view("Repository");
    session.wait_for_screen("Could not start git");
    session.resize(80, 24);
    session.wait_for_screen("Could not start git");
    assert!(!session.screen().contains("Installed release history"));
    session.quit_and_assert_restored_terminal(mouse);
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "the missing-tool runs sent a gh command"
    );

    assert_eq!(
        git_state(&fixture),
        before,
        "a missing-tool run changed Git"
    );
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config,
        "a missing-tool run rewrote the configuration"
    );
}

/// Upgrade from a previous release (a stand-in reporting 0.0.1) to the
/// real one, use it so it writes its own configuration and cache in the
/// default per-user location, then uninstall: only the owned executable
/// goes; configuration, cache, drafts, other tools and the repository stay.
#[test]
fn v1_release_upgrade_from_previous_version_then_owned_uninstall_keeps_config_and_cache() {
    use ghtui::storage::{Cache, Scope};
    let fixture = release_fixture();
    let host = stand_in::Arch::host();
    let previous = fixture.root.join("stand-in-previous");
    fs::write(
        &previous,
        stand_in::elf(
            host,
            &stand_in::Program {
                message: "ghtui 0.0.1\n",
                ..Default::default()
            },
        ),
    )
    .unwrap();
    let previous_dist = fixture.root.join("dist-previous");
    package("v0.0.1", host.linux_target(), &previous, &previous_dist);
    let release = fixture.root.join("release");
    publish(&release, "v0.0.1", &previous_dist, true);
    let first = installer(&fixture, &release, &[]);
    assert!(first.status.success(), "{}", output_text(&first));
    assert!(output_text(&first).contains("installed ghtui v0.0.1"));
    let path = fixture.root.join("home/.local/bin/ghtui");
    assert_eq!(fs::read(&path).unwrap(), fs::read(&previous).unwrap());
    assert_eq!(
        run(Command::new(&path).arg("--version")).trim(),
        "ghtui 0.0.1"
    );

    // The upgrade: the real release becomes latest and replaces it.
    let installed = install_real_binary(&fixture);
    assert_eq!(installed.path, path);

    // Use the upgraded binary with its default per-user configuration.
    let home = fixture.root.join("home");
    let data = home.join(".config/ghtui");
    let config = seed_config(&data);
    fixture.scripted_gh(&empty_inbox_steps("octocat"));
    let before = git_state(&fixture);
    let data_env = data.display().to_string();
    let xdg = home.join(".config").display().to_string();
    let mut session = Session::launch_binary(
        &installed.path,
        &fixture,
        100,
        30,
        &[
            ("GHTUI_CONFIG_DIR", data_env.as_str()),
            ("XDG_CONFIG_HOME", xdg.as_str()),
        ],
    );
    session.assert_focused_view("Repository");
    session.send_key(b"1");
    session.assert_focused_view("Inbox");
    session.wait_for_settled_footer();
    session.send_key(b"r");
    session.wait_for_command_count(&fixture, 7);
    session.wait_for_screen("0 unique PRs across 0 queue matches");
    assert_read_only_journal(&session, &fixture, 7);
    session.quit_and_assert_restored_terminal(false);
    assert_eq!(fs::read(data.join("config.toml")).unwrap(), config);
    let scope = Scope::github("github.com", Some(42)).unwrap();
    {
        let cache = Cache::open(&data.join("cache.sqlite3")).unwrap();
        cache
            .put_draft(&scope, "release-draft", "Keep this draft across uninstall")
            .unwrap();
    }
    let other_tool = home.join(".local/bin/other-tool");
    fs::write(&other_tool, b"#!/bin/sh\nexit 0\n").unwrap();
    let kept = tree_bytes(&data);
    assert!(kept.contains_key(&data.join("config.toml")));
    assert!(kept.contains_key(&data.join("cache.sqlite3")));

    let uninstall = installer(&fixture, &release, &["--uninstall"]);
    let log = output_text(&uninstall);
    assert!(uninstall.status.success(), "{log}");
    assert!(
        log.contains("configuration, cache, drafts and repositories were kept"),
        "{log}"
    );
    assert!(!path.exists(), "uninstall left the executable");
    assert_eq!(fs::read(&other_tool).unwrap(), b"#!/bin/sh\nexit 0\n");
    assert_eq!(tree_bytes(&data), kept, "uninstall changed ghtui data");
    assert_eq!(
        git_state(&fixture),
        before,
        "uninstall changed the repository"
    );
    let cache = Cache::open(&data.join("cache.sqlite3")).unwrap();
    assert_eq!(
        cache.draft(&scope, "release-draft").unwrap().as_deref(),
        Some("Keep this draft across uninstall")
    );
}

fn git_state(fixture: &Fixture) -> (String, String, String, Vec<u8>, Vec<u8>) {
    (
        fixture.git(&["rev-parse", "HEAD"]),
        fixture.git(&["for-each-ref", "--format=%(refname) %(objectname)"]),
        fixture.git(&["status", "--porcelain=v1"]),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fs::read(fixture.repo().join("notes.txt")).unwrap(),
    )
}

#[test]
fn v1_release_keyboard_installed_binary_starts_navigates_and_quits() {
    installed_journey(false);
}

#[test]
fn v1_release_mouse_installed_binary_starts_navigates_and_quits() {
    installed_journey(true);
}

fn installed_journey(mouse: bool) {
    let fixture = release_fixture();
    let installed = install_real_binary(&fixture);
    drive_installed(&fixture, &installed.path, mouse);
    // The installer only wrote the executable: no profile edits in HOME.
    let entries: Vec<_> = fs::read_dir(fixture.root.join("home/.local/bin"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from("ghtui")]);
    assert!(!fixture.root.join("home/.bashrc").exists());
    assert!(!fixture.root.join("home/.profile").exists());
}

/// One representative journey on the installed path: Help overlay, the
/// local Repository history of the fixture commit, a resize to 80x24 and
/// back to the Inbox, then a clean quit. Git state and the gh journal are
/// checked independently of the screen.
fn drive_installed(fixture: &Fixture, binary: &Path, mouse: bool) {
    let before = git_state(fixture);
    let mut session = Session::launch_binary(binary, fixture, 100, 30, &[]);
    session.wait_for_screen("ghtui");
    if mouse {
        session.click_visible_action("[? Help]");
    } else {
        session.send_key(b"?");
    }
    session.wait_for_screen("Help • keyboard and mouse");
    if mouse {
        session.close_overlay_by_mouse("Help • keyboard and mouse");
        session.click_visible_action("Repository");
    } else {
        session.close_overlay_by_key("Help • keyboard and mouse");
        session.send_key(b"2");
    }
    session.assert_focused_view("Repository");
    session.wait_for_screen("Branch: main");
    session.wait_for_screen("Installed release history");
    session.resize(80, 24);
    if mouse {
        session.click_visible_action("Inbox");
    } else {
        session.send_key(b"1");
    }
    session.assert_focused_view("Inbox");
    assert!(
        session.inspect_command_journal(fixture).is_empty(),
        "an installed binary must not mutate GitHub on startup"
    );
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(
        git_state(fixture),
        before,
        "browsing with the installed binary changed Git state"
    );
}

/// Upgrade to a newer release, roll back to the real one by explicit
/// version, then attempt a tampered release: the rolled-back binary stays
/// byte-for-byte intact and both input methods still drive it.
#[test]
fn v1_release_upgrade_rollback_and_failed_install_keep_the_installed_binary_working() {
    let fixture = release_fixture();
    let real = install_real_binary(&fixture);
    let host = stand_in::Arch::host();

    // A newer native release (a stand-in that only reports its version).
    let newer = fixture.root.join("stand-in-newer");
    fs::write(
        &newer,
        stand_in::elf(
            host,
            &stand_in::Program {
                message: "ghtui 99.0.0\n",
                ..Default::default()
            },
        ),
    )
    .unwrap();
    let newer_dist = fixture.root.join("dist-newer");
    package("v99.0.0", real.target, &newer, &newer_dist);
    publish(&real.release, "v99.0.0", &newer_dist, true);
    let upgrade = installer(&fixture, &real.release, &[]);
    assert!(upgrade.status.success(), "{}", output_text(&upgrade));
    assert!(output_text(&upgrade).contains("installed ghtui v99.0.0"));
    assert_eq!(fs::read(&real.path).unwrap(), fs::read(&newer).unwrap());

    // Explicit rollback to the real release.
    let rollback = installer(&fixture, &real.release, &["--version", &real.version]);
    assert!(rollback.status.success(), "{}", output_text(&rollback));
    assert_installed_identity(&real);
    drive_installed(&fixture, &real.path, false);

    // A tampered newer release fails its checksum and leaves the binary.
    let tampered_dist = fixture.root.join("dist-tampered");
    package("v98.0.0", real.target, &newer, &tampered_dist);
    let archive = tampered_dist.join(format!("ghtui-v98.0.0-{}.tar.gz", real.target));
    let mut bytes = fs::read(&archive).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(&archive, bytes).unwrap();
    publish(&real.release, "v98.0.0", &tampered_dist, false);
    let failed = installer(&fixture, &real.release, &["--version", "v98.0.0"]);
    assert!(!failed.status.success(), "{}", output_text(&failed));
    assert!(output_text(&failed).contains("checksum mismatch"));
    assert_installed_identity(&real);
    assert_eq!(
        fs::read_dir(fixture.root.join("cache"))
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .starts_with("ghtui-install.")
            })
            .count(),
        0,
        "the failed install left its staging directory"
    );
    drive_installed(&fixture, &real.path, true);
}
