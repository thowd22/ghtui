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
