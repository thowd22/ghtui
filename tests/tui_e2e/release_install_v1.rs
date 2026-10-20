//! Install the real ghtui executable with scripts/install.sh from an offline
//! file:// release, then drive the INSTALLED binary in a PTY.
use super::*;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");

fn run(command: &mut Command) -> String {
    let output = command.output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.status.success(), "{command:?} failed: {text}");
    text
}

/// Package the test build of ghtui as this crate's version, publish it in
/// the GitHub release layout and install it into the fixture's ~/.local/bin.
/// With GHTUI_E2E_RELEASE_DIST naming a directory of assembled release
/// assets (the release workflow's dist), those exact assets are installed
/// instead, so the journeys drive the artifact that will be published.
fn install_real_binary(fixture: &Fixture) -> PathBuf {
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let target = if cfg!(target_arch = "aarch64") {
        "aarch64-unknown-linux-musl"
    } else {
        "x86_64-unknown-linux-musl"
    };
    let dist = match std::env::var_os("GHTUI_E2E_RELEASE_DIST") {
        Some(dist) if !dist.is_empty() => PathBuf::from(dist),
        _ => {
            let dist = fixture.root.join("dist");
            let packager = Path::new(ROOT).join("scripts/package-release.sh");
            run(Command::new("bash")
                .arg(&packager)
                .args(["archive", &version, target, env!("CARGO_BIN_EXE_ghtui")])
                .arg(&dist));
            run(Command::new("bash")
                .arg(&packager)
                .args(["finish", &version])
                .arg(&dist));
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
    for layout in [format!("download/{version}"), "latest/download".into()] {
        let directory = release.join(layout);
        fs::create_dir_all(&directory).unwrap();
        for entry in fs::read_dir(&dist).unwrap() {
            let entry = entry.unwrap();
            fs::copy(entry.path(), directory.join(entry.file_name())).unwrap();
        }
    }
    let home = fixture.root.join("home");
    let log = run(Command::new("bash")
        .arg(release.join("latest/download/install.sh"))
        .env_clear()
        .env("HOME", &home)
        .env("PATH", "/usr/bin:/bin")
        .env("TMPDIR", fixture.root.join("cache"))
        .env(
            "GHTUI_RELEASE_BASE",
            format!("file://{}", release.display()),
        ));
    let installed = home.join(".local/bin/ghtui");
    assert!(
        log.contains(&format!(
            "installed ghtui {version} at {}",
            installed.display()
        )),
        "{log}"
    );
    assert_eq!(
        fs::read(&installed).unwrap(),
        released.stdout,
        "installed bytes differ from the released binary"
    );
    installed
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
    let fixture = Fixture::new();
    let installed = install_real_binary(&fixture);
    let mut session = Session::launch_binary(&installed, &fixture, 100, 30, &[]);
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
    session.resize(80, 24);
    if mouse {
        session.click_visible_action("Inbox");
    } else {
        session.send_key(b"1");
    }
    session.assert_focused_view("Inbox");
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "an installed binary must not mutate GitHub on startup"
    );
    session.quit_and_assert_restored_terminal(mouse);
    // The installer only wrote the executable: no profile edits in HOME.
    let entries: Vec<_> = fs::read_dir(fixture.root.join("home/.local/bin"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(entries, vec![std::ffi::OsString::from("ghtui")]);
    assert!(!fixture.root.join("home/.bashrc").exists());
    assert!(!fixture.root.join("home/.profile").exists());
}
