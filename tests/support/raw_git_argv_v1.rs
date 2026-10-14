//! Test-only transparent installed-Git argv recording. Raw identifiers come
//! from per-PID NUL bytes, never Trace2's UTF8/display representation.
use std::{
    ffi::{OsStr, OsString},
    fs,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, PartialEq, Eq)]
pub struct RawGitArgvV1 {
    pub pid: u32,
    pub argv: Vec<Vec<u8>>,
}

fn quoted_path(path: &Path) -> Vec<u8> {
    let mut quoted = vec![b'\''];
    for byte in path.as_os_str().as_bytes() {
        if *byte == b'\'' {
            quoted.extend_from_slice(b"'\\''");
        } else {
            quoted.push(*byte);
        }
    }
    quoted.push(b'\'');
    quoted
}

/// Fixture setup only. Installs into an EXISTING child-only bin directory;
/// rejects an existing wrapper rather than overwriting cancellation probes.
pub fn install(root: &Path) -> PathBuf {
    let installed = std::env::split_paths(&std::env::var_os("PATH").unwrap())
        .map(|directory| directory.join("git"))
        .find(|path| {
            path.is_file()
                && fs::metadata(path)
                    .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
        })
        .map(|path| fs::canonicalize(path).unwrap())
        .expect("Actual installed git is required; missing is not zero-read proof");
    let program = root.join("bin/git");
    assert!(!program.exists(), "Do not replace an existing Git probe");
    let records = root.join("raw-git-argv-v1");
    fs::create_dir(&records).unwrap();
    fs::set_permissions(&records, fs::Permissions::from_mode(0o700)).unwrap();
    let mut script = b"#!/bin/sh\nset -C\nrecord_dir=".to_vec();
    script.extend_from_slice(&quoted_path(&records));
    script.extend_from_slice(
        b"\nprintf '%s\\000' \"$@\" > \"$record_dir/$$.argv\" || exit 91\nexec ",
    );
    script.extend_from_slice(&quoted_path(&installed));
    script.extend_from_slice(b" \"$@\"\n");
    fs::write(&program, script).unwrap();
    fs::set_permissions(&program, fs::Permissions::from_mode(0o700)).unwrap();
    program
}

pub fn records(directory: &Path) -> Vec<RawGitArgvV1> {
    let mut result = Vec::new();
    for entry in fs::read_dir(directory).expect("Actual raw-argv recording directory is required") {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_str().expect("Recorder filenames are ASCII PIDs");
        let pid: u32 = name
            .strip_suffix(".argv")
            .expect("Only per-PID argv records belong here")
            .parse()
            .unwrap();
        assert!(pid > 0);
        let bytes = fs::read(entry.path()).unwrap();
        assert_eq!(bytes.last(), Some(&0), "Incomplete raw argv is not proof");
        let argv = bytes[..bytes.len() - 1]
            .split(|byte| *byte == 0)
            .map(<[u8]>::to_vec)
            .collect();
        result.push(RawGitArgvV1 { pid, argv });
    }
    result.sort_by_key(|record| record.pid);
    result
}

/// Byte-exact actual command windows, allowing only genuine leading Git
/// global options. No synthesized SHA:path expression or lossy comparison.
pub fn exact_read(records: &[RawGitArgvV1], chosen: &str, path: &[u8], oid: &str) {
    let tree = [
        b"ls-tree".as_slice(),
        b"-z",
        b"--full-tree",
        chosen.as_bytes(),
        b"--",
        path,
    ];
    assert!(
        records.iter().any(|record| record.argv.len() >= tree.len()
            && record.argv[record.argv.len() - tree.len()..]
                .iter()
                .map(Vec::as_slice)
                .eq(tree)),
        "No actual ls-tree chosenSHA -- exact raw path child"
    );
    let blob = [b"cat-file".as_slice(), b"blob", oid.as_bytes()];
    assert!(
        records.iter().any(|record| record.argv.len() >= blob.len()
            && record.argv[record.argv.len() - blob.len()..]
                .iter()
                .map(Vec::as_slice)
                .eq(blob)),
        "No actual cat-file blob exact object ID child"
    );
}

/// Independent original Git-object proof, not the TUI's cached body/parser.
/// Returns the actual original regular-file blob and confirms full raw path.
pub fn original_blob(repo: &Path, chosen: &str, path: &[u8]) -> (String, Vec<u8>) {
    let output = Command::new("git")
        .current_dir(repo)
        .args([
            OsString::from("ls-tree"),
            "-z".into(),
            "--full-tree".into(),
            chosen.into(),
            "--".into(),
            OsStr::from_bytes(path).to_owned(),
        ])
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(output.status.success());
    let mut entries = output.stdout.split(|byte| *byte == 0);
    let entry = entries.next().unwrap();
    assert_eq!(entries.next(), Some(b"".as_slice()));
    assert!(
        entries.next().is_none(),
        "Exact fixture path must be unique"
    );
    let tab = entry.iter().position(|byte| *byte == b'\t').unwrap();
    assert_eq!(&entry[tab + 1..], path);
    let header = std::str::from_utf8(&entry[..tab]).unwrap();
    let fields = header.split(' ').collect::<Vec<_>>();
    assert_eq!(&fields[..2], ["100644", "blob"]);
    assert_eq!(fields.len(), 3);
    let oid = fields[2];
    assert!(matches!(oid.len(), 40 | 64) && oid.bytes().all(|byte| byte.is_ascii_hexdigit()));
    let body = Command::new("git")
        .current_dir(repo)
        .args(["cat-file", "blob", oid])
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(body.status.success());
    (oid.into(), body.stdout)
}

/// Trace2 may contain nonUTF8 argv. Replacement decoding is ONLY ASCII
/// command/count telemetry: strict JSON parse remains mandatory, and all
/// nonASCII arguments are erased rather than retained as identity evidence.
pub fn ascii_trace2(path: &Path) -> Vec<serde_json::Value> {
    let bytes = fs::read(path).expect("Actual initialized Trace2 is required");
    String::from_utf8_lossy(&bytes)
        .lines()
        .map(|line| {
            let mut value: serde_json::Value = serde_json::from_str(line).unwrap();
            let event = value["event"].as_str().unwrap();
            assert!(event.is_ascii());
            if event == "cmd_name" {
                assert!(value["name"].as_str().unwrap().is_ascii());
            }
            if event == "start" {
                for arg in value["argv"].as_array_mut().unwrap() {
                    let text = arg.as_str().unwrap();
                    if !text.is_ascii() {
                        *arg = serde_json::Value::String("<nonASCII telemetry argument>".into());
                    }
                }
            }
            value
        })
        .collect()
}
