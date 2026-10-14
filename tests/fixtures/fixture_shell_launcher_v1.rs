//! Test-only stable executable. Fixture text is DATA, never a newly written
//! executable inode. Compile externally before publishing fixture symlinks.
#![deny(unsafe_code)]
use std::{
    ffi::{OsStr, OsString},
    fs,
    io::{self, Read},
    os::unix::{fs::MetadataExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
};

fn data_path(program: &Path) -> PathBuf {
    let mut name = program.as_os_str().to_os_string();
    name.push(".fixture-shell-v1");
    name.into()
}
fn original_program(arg0: &OsStr) -> io::Result<PathBuf> {
    // current_exe may name either the alias or its target across platforms:
    // https://doc.rust-lang.org/std/env/fn.current_exe.html
    // This selects isolated test data, never authentication/write authority.
    let executable = fs::canonicalize(std::env::current_exe()?)?;
    let input = Path::new(arg0);
    let candidates: Vec<_> = if input.components().count() > 1 || input.is_absolute() {
        vec![input.to_owned()]
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|directory| directory.join(input))
            .collect()
    };
    candidates
        .into_iter()
        .find(|candidate| {
            fs::symlink_metadata(candidate).is_ok_and(|m| m.file_type().is_symlink())
                && fs::canonicalize(candidate).is_ok_and(|actual| actual == executable)
                && data_path(candidate).is_file()
        })
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "Exact fixture launcher alias unavailable",
            )
        })
}
fn execute() -> io::Result<()> {
    let mut args = std::env::args_os();
    let arg0 = args
        .next()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Missing original program"))?;
    let program = original_program(&arg0)?;
    let data = data_path(&program);
    let metadata = fs::symlink_metadata(&data)?;
    if !metadata.is_file()
        || metadata.nlink() != 1
        || metadata.mode() & 0o077 != 0
        || metadata.len() > 1024 * 1024
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Fixture shell data must remain one private regular file",
        ));
    }
    let mut prefix = [0; 10];
    fs::File::open(&data)?.read_exact(&mut prefix)?;
    if &prefix != b"#!/bin/sh\n" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Only the explicit /bin/sh fixture interpreter is admitted",
        ));
    }
    // exec preserves PID, SID, environment, stdin/out/err and original working
    // directory. Source the data after shifting its single internal argument:
    // the fixture sees original resolved script $0 and byte-exact original $@.
    let error = Command::new("/bin/sh")
        .args([
            OsString::from("-c"),
            OsString::from("_ghtui_fixture_source_v1=$1; shift; . \"$_ghtui_fixture_source_v1\""),
            program.into_os_string(),
            data.into_os_string(),
        ])
        .args(args)
        .exec();
    Err(error)
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("Fixture shell launcher refused: {error}");
        std::process::exit(125);
    }
}
