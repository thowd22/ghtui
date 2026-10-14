#[path = "support/fixture_shell_publication_v1.rs"]
mod fixture_shell_publication_v1;
#[cfg(target_os = "linux")]
#[path = "support/inherited_fixture_writer_v1.rs"]
mod inherited_fixture_writer_v1;
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::PermissionsExt,
    },
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn root() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn script(root: &Path) -> Vec<u8> {
    format!("#!/bin/sh\nprintf '%s\\000' \"$0\" \"$@\" > '{}/argv'\nprintf '%s' \"$$\" > '{}/pid'\nprintf '%s' \"$FIXTURE_PRESERVED_ENV\" > '{}/environment'\n/bin/cat > '{}/stdin'\nprintf 'stdout unchanged\\n'\nprintf 'stderr unchanged\\n' >&2\nexit 37\n",root.display(),root.display(),root.display(),root.display()).into_bytes()
}
/// Genuine kernel probes stay genuine: external installed shell writes literal
/// stdin bytes; the forking harness never owns their executable write handles.
fn publish_kernel_probe(program: &Path, bytes: &[u8]) -> io::Result<()> {
    if bytes.len() > 1024 * 1024 || fs::symlink_metadata(program).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Bounded new owned executable probe required",
        ));
    }
    let mut child = Command::new("/bin/sh")
        .args([
            "-c",
            "umask 077; set -C; /bin/cat > \"$1\" || exit $?; /bin/chmod 700 \"$1\"",
            "ghtui-kernel-probe-writer",
        ])
        .arg(program)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let result = child
        .stdin
        .take()
        .expect("Explicitly piped probe bytes")
        .write_all(bytes);
    let output = child.wait_with_output()?;
    result?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "External literal probe writer failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    Ok(())
}
#[test]
fn test_framework_fixture_shell_publication_v1_exact_raw_argv_original_pid_stdin_env_interpreter_and_status(
) {
    for through_path in [false, true] {
        let root = root();
        let program = root.path().join("quoted' probe\nprogram");
        let data = fixture_shell_publication_v1::install(&program, &script(root.path())).unwrap();
        assert!(fs::symlink_metadata(&program)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::metadata(&data).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let raw = OsString::from_vec(if cfg!(target_os = "macos") {
            b"raw-\nname".to_vec()
        } else {
            b"raw-\xff".to_vec()
        });
        let args = [
            OsString::from(""),
            raw,
            OsString::from("spaces ' \"\n界"),
            OsString::from("--job"),
            OsString::from("501"),
        ];
        let mut command = Command::new(if through_path {
            Path::new(program.file_name().unwrap())
        } else {
            &program
        });
        command
            .args(&args)
            .env("PATH", root.path())
            .env("FIXTURE_PRESERVED_ENV", "exact environment 界")
            .current_dir(root.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while !root.path().join("pid").exists() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let actual = loop {
            let value = fs::read_to_string(root.path().join("pid")).unwrap();
            if !value.is_empty() {
                break value.parse::<u32>().unwrap();
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        };
        assert_eq!(
            actual,
            child.id(),
            "No launcher fork or replacement child PID"
        );
        #[cfg(target_os = "linux")]
        assert_eq!(
            fs::read_link(format!("/proc/{actual}/exe")).unwrap(),
            fs::canonicalize("/bin/sh").unwrap()
        );
        child
            .stdin
            .take()
            .unwrap()
            .write_all(b"original\0stdin\nbytes")
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(37));
        assert_eq!(output.stdout, b"stdout unchanged\n");
        assert_eq!(output.stderr, b"stderr unchanged\n");
        assert_eq!(
            fs::read(root.path().join("stdin")).unwrap(),
            b"original\0stdin\nbytes"
        );
        assert_eq!(
            fs::read_to_string(root.path().join("environment")).unwrap(),
            "exact environment 界"
        );
        let mut expected = vec![program.as_os_str().as_bytes().to_vec()];
        expected.extend(args.iter().map(|s| s.as_os_str().as_bytes().to_vec()));
        let bytes = fs::read(root.path().join("argv")).unwrap();
        let actual = bytes
            .split(|b| *b == 0)
            .map(<[u8]>::to_vec)
            .collect::<Vec<_>>();
        expected.push(vec![]);
        assert_eq!(actual, expected);
    }
}
#[cfg(target_os = "linux")]
#[test]
fn test_framework_fixture_shell_publication_v1_actual_inherited_writer_old_executable_busy_new_data_executes_once(
) {
    let root = root();
    let old = root.path().join("old-script");
    publish_kernel_probe(&old, b"#!/bin/sh\nprintf old\n").unwrap();
    let program = root.path().join("new-script");
    let data = fixture_shell_publication_v1::install(
        &program,
        b"#!/bin/sh\nprintf 'exactly once\\n' >> \"$1\"\nprintf 'new execution\\n'\n",
    )
    .unwrap();
    let mut holder = inherited_fixture_writer_v1::HeldFixtureWriter::hold(&[&old, &data]).unwrap();
    assert!(holder.pid() > 0);
    assert_eq!(
        Command::new(&old).output().unwrap_err().raw_os_error(),
        Some(libc::ETXTBSY)
    );
    let journal = root.path().join("journal");
    let output = Command::new(&program).arg(&journal).output().unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"new execution\n");
    assert_eq!(fs::read(&journal).unwrap(), b"exactly once\n");
    holder.finish().unwrap();
    assert_eq!(Command::new(&old).output().unwrap().stdout, b"old");
}
#[test]
fn test_framework_fixture_shell_publication_v1_rejects_unsupported_shebang_and_preserves_genuine_bad_executable(
) {
    let root = root();
    for body in [
        b"not executable text".as_slice(),
        b"#!/not/an/installed/interpreter\n",
        b"#!/bin/bash\nprintf unsupported\n",
    ] {
        let program = root.path().join(format!("bad{}", body.len()));
        assert!(fixture_shell_publication_v1::install(&program, body).is_err());
        assert!(!program.exists());
        assert!(!fixture_shell_publication_v1::data_path(&program).exists());
    }
    let program = root.path().join("deliberate-bad-executable");
    let original = b"#!/not/an/installed/interpreter\n";
    publish_kernel_probe(&program, original).unwrap();
    assert!(fixture_shell_publication_v1::install(&program, b"#!/bin/sh\nprintf wrong\n").is_err());
    assert_eq!(fs::read(&program).unwrap(), original);
    assert_eq!(
        Command::new(&program).output().unwrap_err().kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(!fixture_shell_publication_v1::data_path(&program).exists());
}
#[test]
fn test_framework_fixture_shell_publication_v1_alias_or_changed_interpreter_data_is_refused_not_reinterpreted(
) {
    for alias in [true, false] {
        let root = root();
        let program = root.path().join("probe");
        let data =
            fixture_shell_publication_v1::install(&program, b"#!/bin/sh\nprintf SHOULD_NOT_RUN\n")
                .unwrap();
        if alias {
            let saved = root.path().join("saved-original-data");
            fs::rename(&data, &saved).unwrap();
            std::os::unix::fs::symlink(&saved, &data).unwrap();
        } else {
            fs::write(&data, b"#!/bin/bash\nprintf SHOULD_NOT_RUN\n").unwrap();
        }
        let output = Command::new(&program).output().unwrap();
        assert_eq!(output.status.code(), Some(125));
        assert!(output.stdout.is_empty());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Fixture shell launcher refused"));
    }
}
