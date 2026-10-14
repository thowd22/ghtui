//! Test-only executable publication. Parent-written text never becomes an
//! executable inode; one external compiler builds the stable launcher first.
use std::{
    fs,
    io::{self, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};

pub fn data_path(program: &Path) -> PathBuf {
    let mut path = program.as_os_str().to_os_string();
    path.push(".fixture-shell-v1");
    path.into()
}
fn stable_launcher() -> io::Result<&'static PathBuf> {
    static LAUNCHER: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    LAUNCHER
        .get_or_init(|| {
            let directory = tempfile::Builder::new()
                .permissions(fs::Permissions::from_mode(0o700))
                .tempdir()
                .map_err(|e| e.to_string())?
                .keep();
            let binary = directory.join("fixture-shell-launcher-v1");
            let source = Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/fixture_shell_launcher_v1.rs");
            let output = Command::new("rustc")
                .arg("--edition=2021")
                .arg(&source)
                .arg("-o")
                .arg(&binary)
                .output()
                .map_err(|e| e.to_string())?;
            if !output.status.success() {
                return Err(format!(
                    "Actual fixture launcher compile failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ));
            }
            if !binary.is_file() {
                return Err("External compiler did not create the fixture launcher".into());
            }
            Ok(binary)
        })
        .as_ref()
        .map_err(|e| io::Error::other(e.clone()))
}
pub fn install(program: &Path, script: &[u8]) -> io::Result<PathBuf> {
    if !script.starts_with(b"#!/bin/sh\n") || script.len() > 1024 * 1024 {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "Only bounded explicit /bin/sh fixture scripts are admitted; invalid executable probes must stay genuine"));
    }
    if fs::symlink_metadata(program).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "Never overwrite an existing executable or cancellation probe",
        ));
    }
    let launcher = stable_launcher()?;
    let data = data_path(program);
    let mut writer = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&data)?;
    writer.write_all(script)?;
    drop(writer);
    std::os::unix::fs::symlink(launcher, program)?;
    Ok(data)
}
