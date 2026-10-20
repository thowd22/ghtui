//! Real Bash runs of scripts/install.sh against offline file:// releases that
//! scripts/package-release.sh assembles, plus workflow shape checks.
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};
use yaml_rust2::{Yaml, YamlLoader};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const X86: &str = "x86_64-unknown-linux-musl";
const ARM: &str = "aarch64-unknown-linux-musl";

struct Lab {
    _temp: tempfile::TempDir,
    root: PathBuf,
}

impl Lab {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        for dir in ["home", "fakebin", "release", "stage", "tmp"] {
            fs::create_dir(root.join(dir)).unwrap();
        }
        let lab = Self { _temp: temp, root };
        lab.uname("Linux", "x86_64");
        lab
    }
    fn home(&self) -> PathBuf {
        self.root.join("home")
    }
    fn installed(&self) -> PathBuf {
        self.home().join(".local/bin/ghtui")
    }
    fn base(&self) -> String {
        format!("file://{}", self.root.join("release").display())
    }
    fn uname(&self, os: &str, cpu: &str) {
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in -s) echo {os} ;; -m) echo {cpu} ;; *) echo {os} ;; esac\n"
        );
        executable(&self.root.join("fakebin/uname"), script.as_bytes());
    }
    /// A tiny stand-in executable that answers `--version` like ghtui.
    fn fake_binary(&self, reported: &str) -> PathBuf {
        let path = self.root.join(format!("stage/ghtui-{reported}"));
        executable(
            &path,
            format!("#!/bin/sh\necho \"ghtui {reported}\"\n").as_bytes(),
        );
        path
    }
    /// Assemble assets with the real packager and publish them in the
    /// GitHub layout: download/<tag>/<asset> and latest/download/<asset>.
    fn publish(&self, version: &str, binaries: &[(&str, &Path)], latest: bool) -> PathBuf {
        let dist = self.root.join(format!("stage/dist-{version}"));
        for (target, binary) in binaries {
            let output = Command::new("bash")
                .arg(script("package-release.sh"))
                .args(["archive", version, target])
                .arg(binary)
                .arg(&dist)
                .output()
                .unwrap();
            assert_success(&output);
        }
        let output = Command::new("bash")
            .arg(script("package-release.sh"))
            .args(["finish", version])
            .arg(&dist)
            .output()
            .unwrap();
        assert_success(&output);
        let exact = self.root.join(format!("release/download/{version}"));
        copy_dir(&dist, &exact);
        if latest {
            copy_dir(&dist, &self.root.join("release/latest/download"));
        }
        exact
    }
    fn command(&self) -> Command {
        // Absolute: a test may give the child a PATH without bash on it,
        // and Command resolves the program with the child's PATH.
        let mut command = Command::new("/bin/bash");
        command
            .env_clear()
            .env("HOME", self.home())
            .env("TMPDIR", self.root.join("tmp"))
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.root.join("fakebin").display()),
            )
            .env("GHTUI_RELEASE_BASE", self.base())
            .current_dir(&self.root);
        command
    }
    fn install(&self, args: &[&str]) -> Output {
        self.command()
            .arg(script("install.sh"))
            .args(args)
            .output()
            .unwrap()
    }
    fn assert_no_leftovers(&self) {
        assert_eq!(
            fs::read_dir(self.root.join("tmp")).unwrap().count(),
            0,
            "staging directory left behind"
        );
        if let Ok(entries) = fs::read_dir(self.home().join(".local/bin")) {
            let names: Vec<_> = entries
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect();
            assert!(
                names.iter().all(|name| name == "ghtui"),
                "unexpected files in ~/.local/bin: {names:?}"
            );
        }
    }
}

fn script(name: &str) -> PathBuf {
    Path::new(ROOT).join("scripts").join(name)
}
fn executable(path: &Path, bytes: &[u8]) {
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}
fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        fs::copy(entry.path(), to.join(entry.file_name())).unwrap();
    }
}
fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
fn assert_success(output: &Output) {
    assert!(output.status.success(), "command failed: {}", text(output));
}
fn assert_failure(output: &Output, needle: &str) {
    assert!(
        !output.status.success(),
        "expected failure: {}",
        text(output)
    );
    assert!(
        text(output).contains(needle),
        "missing {needle:?} in: {}",
        text(output)
    );
}
fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn mode(path: &Path) -> u32 {
    fs::metadata(path).unwrap().permissions().mode() & 0o777
}

#[test]
fn v1_release_curl_pipe_bash_installs_latest_verified_binary_into_local_bin() {
    let lab = Lab::new();
    let binary = lab.fake_binary("0.1.0");
    let exact = lab.publish("v0.1.0", &[(X86, &binary), (ARM, &binary)], true);
    let sums = fs::read_to_string(exact.join("SHA256SUMS")).unwrap();
    let archive = fs::read(exact.join(format!("ghtui-v0.1.0-{X86}.tar.gz"))).unwrap();
    assert!(sums.contains(&format!("{}  ghtui-v0.1.0-{X86}.tar.gz", sha256(&archive))));
    assert!(sums.contains(&format!("ghtui-v0.1.0-{ARM}.tar.gz")));
    assert!(sums.contains("  install.sh\n"));
    assert_eq!(
        fs::read_to_string(exact.join("release-version.txt")).unwrap(),
        "v0.1.0\n"
    );

    // The documented one-liner: curl the released install.sh and pipe it to bash.
    let one_liner = format!(
        "curl -fsSL {}/latest/download/install.sh | bash",
        lab.base()
    );
    let output = lab.command().args(["-c", &one_liner]).output().unwrap();
    assert_success(&output);
    let log = text(&output);
    assert!(log.contains(&format!(
        "installed ghtui v0.1.0 at {}",
        lab.installed().display()
    )));
    assert!(log.contains("is not on your PATH"), "{log}");
    assert_eq!(
        fs::read(lab.installed()).unwrap(),
        fs::read(&binary).unwrap()
    );
    assert_eq!(mode(&lab.installed()), 0o755);
    // No shell profile was written and nothing else appeared in HOME.
    let home: Vec<_> = fs::read_dir(lab.home())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(home, vec![std::ffi::OsString::from(".local")]);
    lab.assert_no_leftovers();

    // Repeating the same version is a no-op that reports the current state.
    let again = lab.install(&[]);
    assert_success(&again);
    assert!(text(&again).contains("ghtui v0.1.0 is already installed"));
}

#[test]
fn v1_release_maps_aarch64_and_pins_or_upgrades_exact_versions() {
    let lab = Lab::new();
    lab.uname("Linux", "aarch64");
    let old_arm = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(ARM, &old_arm)], false);
    let new_arm = lab.fake_binary("0.2.0");
    lab.publish("v0.2.0", &[(ARM, &new_arm)], true);

    let pinned = lab.install(&["--version", "0.1.0"]);
    assert_success(&pinned);
    assert!(text(&pinned).contains(&format!("ghtui v0.1.0 ({ARM})")));
    assert_eq!(
        fs::read(lab.installed()).unwrap(),
        fs::read(&old_arm).unwrap()
    );

    let latest = lab.install(&[]);
    assert_success(&latest);
    assert!(text(&latest).contains("installed ghtui v0.2.0"));
    assert_eq!(
        fs::read(lab.installed()).unwrap(),
        fs::read(&new_arm).unwrap()
    );

    // GHTUI_VERSION also works with curl | bash where flags are awkward.
    let rollback = lab
        .command()
        .env("GHTUI_VERSION", "v0.1.0")
        .arg(script("install.sh"))
        .output()
        .unwrap();
    assert_success(&rollback);
    assert_eq!(
        fs::read(lab.installed()).unwrap(),
        fs::read(&old_arm).unwrap()
    );

    // A version that was never published fails cleanly and keeps the binary.
    let missing = lab.install(&["--version", "v9.9.9"]);
    assert_failure(&missing, "download failed");
    assert_eq!(
        fs::read(lab.installed()).unwrap(),
        fs::read(&old_arm).unwrap()
    );
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_checksum_mismatch_never_replaces_the_previous_binary() {
    let lab = Lab::new();
    let good = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(X86, &good)], true);
    assert_success(&lab.install(&[]));

    let next = lab.fake_binary("0.2.0");
    let exact = lab.publish("v0.2.0", &[(X86, &next)], false);
    let archive = exact.join(format!("ghtui-v0.2.0-{X86}.tar.gz"));
    let mut bytes = fs::read(&archive).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(&archive, bytes).unwrap();
    let output = lab.install(&["--version", "v0.2.0"]);
    assert_failure(&output, "checksum mismatch");
    assert!(text(&output).contains("nothing was installed"));
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&good).unwrap());

    // A SHA256SUMS without an entry for this target is refused too.
    fs::write(exact.join("SHA256SUMS"), "0000  other.tar.gz\n").unwrap();
    assert_failure(
        &lab.install(&["--version", "v0.2.0"]),
        "SHA256SUMS has no single entry",
    );
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&good).unwrap());
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_refuses_unsafe_archives_and_wrong_version_binaries() {
    let lab = Lab::new();
    // Binary that reports a different version than the tag it shipped under.
    let wrong = lab.fake_binary("0.0.9");
    lab.publish("v0.3.0", &[(X86, &wrong)], false);
    assert_failure(
        &lab.install(&["--version", "v0.3.0"]),
        "reports 'ghtui 0.0.9', expected 'ghtui 0.3.0'",
    );
    assert!(!lab.installed().exists());

    // Archive with an extra member, correctly checksummed.
    let exact = lab.root.join("release/download/v0.4.0");
    let payload = lab.root.join("stage/payload");
    fs::create_dir_all(&payload).unwrap();
    fs::create_dir_all(&exact).unwrap();
    executable(&payload.join("ghtui"), b"#!/bin/sh\necho 'ghtui 0.4.0'\n");
    fs::write(payload.join("evil"), b"x").unwrap();
    let name = format!("ghtui-v0.4.0-{X86}.tar.gz");
    let status = Command::new("tar")
        .arg("-C")
        .arg(&payload)
        .arg("-czf")
        .arg(exact.join(&name))
        .args(["ghtui", "evil"])
        .status()
        .unwrap();
    assert!(status.success());
    let digest = sha256(&fs::read(exact.join(&name)).unwrap());
    fs::write(exact.join("SHA256SUMS"), format!("{digest}  {name}\n")).unwrap();
    assert_failure(
        &lab.install(&["--version", "v0.4.0"]),
        "must contain exactly one file named ghtui",
    );
    assert!(!lab.installed().exists());

    // An existing symlink at the destination is not ours to replace.
    let good = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(X86, &good)], true);
    fs::create_dir_all(lab.home().join(".local/bin")).unwrap();
    std::os::unix::fs::symlink("/bin/true", lab.installed()).unwrap();
    assert_failure(&lab.install(&[]), "is not a regular file");
    assert_eq!(
        fs::read_link(lab.installed()).unwrap(),
        Path::new("/bin/true")
    );
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_rejects_unsupported_platforms_and_insecure_release_urls() {
    let lab = Lab::new();
    let binary = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(X86, &binary)], true);
    lab.uname("Darwin", "arm64");
    assert_failure(
        &lab.install(&[]),
        "macOS release binaries are not published yet",
    );
    lab.uname("Linux", "riscv64");
    assert_failure(
        &lab.install(&[]),
        "unsupported Linux CPU architecture: riscv64",
    );
    lab.uname("FreeBSD", "amd64");
    assert_failure(&lab.install(&[]), "unsupported operating system: FreeBSD");
    lab.uname("Linux", "x86_64");
    assert_failure(
        &lab.install(&["--release-base", "http://example.com/releases"]),
        "release base must be an https:// URL",
    );
    assert_failure(
        &lab.install(&["--install-dir", "relative/bin"]),
        "absolute path",
    );
    assert_failure(&lab.install(&["--bogus"]), "unknown option: --bogus");
    assert!(!lab.home().join(".local").exists());
    lab.assert_no_leftovers();
    let help = lab.install(&["--help"]);
    assert_success(&help);
    assert!(text(&help).contains("--uninstall"));
}

#[test]
fn v1_release_truncated_download_runs_nothing() {
    let lab = Lab::new();
    let binary = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(X86, &binary)], true);
    let full = fs::read(script("install.sh")).unwrap();
    // Every prefix that stops before the final closing brace must be inert.
    let end = full.len() - full.iter().rev().position(|b| *b == b'}').unwrap() - 1;
    for cut in [full.len() / 4, full.len() / 2, end - 10, end] {
        let mut child = lab
            .command()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        use std::io::Write;
        child.stdin.take().unwrap().write_all(&full[..cut]).unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            !text(&output).contains("installing ghtui"),
            "prefix {cut} started an install: {}",
            text(&output)
        );
        assert!(
            !lab.home().join(".local").exists(),
            "prefix {cut} wrote files"
        );
    }
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_uninstall_removes_only_the_executable() {
    let lab = Lab::new();
    let binary = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(X86, &binary)], true);
    assert_success(&lab.install(&[]));
    let config = lab.home().join(".config/ghtui/config.toml");
    fs::create_dir_all(config.parent().unwrap()).unwrap();
    fs::write(&config, "version = 1\n").unwrap();
    let other = lab.home().join(".local/bin/other-tool");
    fs::write(&other, "keep").unwrap();

    let output = lab.install(&["--uninstall"]);
    assert_success(&output);
    assert!(text(&output).contains("configuration, cache and drafts were kept"));
    assert!(!lab.installed().exists());
    assert_eq!(fs::read_to_string(&config).unwrap(), "version = 1\n");
    assert_eq!(fs::read_to_string(&other).unwrap(), "keep");
    let again = lab.install(&["--uninstall"]);
    assert_success(&again);
    assert!(text(&again).contains("nothing to remove"));
}

/// A PATH directory holding only the named host tools, so a test controls
/// exactly which dependencies the installer can find.
fn tool_dir(lab: &Lab, name: &str, tools: &[&str]) -> PathBuf {
    let dir = lab.root.join(name);
    fs::create_dir_all(&dir).unwrap();
    for tool in tools {
        let found = ["/usr/bin", "/bin"]
            .iter()
            .map(|prefix| Path::new(prefix).join(tool))
            .find(|path| path.exists())
            .unwrap_or_else(|| panic!("host tool {tool} is missing"));
        std::os::unix::fs::symlink(found, dir.join(tool)).unwrap();
    }
    dir
}
const INSTALL_TOOLS: &[&str] = &[
    "curl",
    "tar",
    "gzip",
    "mktemp",
    "awk",
    "grep",
    "head",
    "tr",
    "sha256sum",
    "cmp",
    "cp",
    "chmod",
    "mv",
    "rm",
    "mkdir",
];

#[test]
fn v1_release_transport_is_certificate_verified_https_only_and_bounded() {
    let lab = Lab::new();
    // A recording curl that refuses to transfer anything.
    let log = lab.root.join("curl-args");
    executable(
        &lab.root.join("fakebin/curl"),
        format!(
            "#!/bin/sh\nfor arg in \"$@\"; do printf '%s\\n' \"$arg\"; done >> '{}'\nexit 22\n",
            log.display()
        )
        .as_bytes(),
    );
    let output = lab.install(&["--release-base", "https://releases.invalid/ghtui"]);
    assert_failure(
        &output,
        "download failed: https://releases.invalid/ghtui/latest/download/release-version.txt",
    );
    let args: Vec<String> = fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect();
    // --disable must be first so ~/.curlrc cannot weaken the transfer.
    assert_eq!(args[0], "--disable");
    for (flag, value) in [
        ("--proto", "=https"),
        ("--proto-redir", "=https"),
        ("--max-time", "600"),
        ("--connect-timeout", "20"),
        ("--max-redirs", "10"),
    ] {
        let at = args
            .iter()
            .position(|arg| arg == flag)
            .unwrap_or_else(|| panic!("{flag} missing: {args:?}"));
        assert_eq!(args[at + 1], value, "{flag}");
    }
    for required in ["--fail", "--tlsv1.2"] {
        assert!(args.iter().any(|arg| arg == required), "{required} missing");
    }
    for insecure in ["-k", "--insecure", "--proto-default", "--cacert"] {
        assert!(
            !args.iter().any(|arg| arg == insecure),
            "{insecure} in {args:?}"
        );
    }
    assert!(!lab.installed().exists());
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_integrity_failure_never_executes_the_download() {
    let lab = Lab::new();
    let marker = lab.root.join("executed");
    let tripwire = lab.root.join("stage/tripwire");
    executable(
        &tripwire,
        format!(
            "#!/bin/sh\ntouch '{}'\necho 'ghtui 0.5.0'\n",
            marker.display()
        )
        .as_bytes(),
    );
    let exact = lab.publish("v0.5.0", &[(X86, &tripwire)], true);
    let archive = exact.join(format!("ghtui-v0.5.0-{X86}.tar.gz"));
    let bytes = fs::read(&archive).unwrap();
    // A truncated (streamed prefix) archive, a wrong checksum and a malformed
    // SHA256SUMS all fail before the binary inside is ever run.
    fs::write(&archive, &bytes[..bytes.len() / 2]).unwrap();
    assert_failure(&lab.install(&[]), "checksum mismatch");
    fs::write(&archive, &bytes).unwrap();
    let sums = exact.join("SHA256SUMS");
    let listed = fs::read_to_string(&sums).unwrap();
    fs::write(&sums, listed.replacen(&sha256(&bytes), &"0".repeat(64), 1)).unwrap();
    assert_failure(&lab.install(&["--version", "v0.5.0"]), "checksum mismatch");
    fs::write(&sums, "not a checksum file\n").unwrap();
    assert_failure(
        &lab.install(&["--version", "v0.5.0"]),
        "SHA256SUMS has no single entry",
    );
    assert!(!marker.exists(), "the unverified download was executed");
    assert!(!lab.installed().exists());
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_concurrent_and_interrupted_upgrades_keep_a_working_binary() {
    let lab = Lab::new();
    let old = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(X86, &old)], false);
    assert_success(&lab.install(&["--version", "v0.1.0"]));

    // The new binary kills the installer while it is being verified.
    let killer = lab.root.join("stage/killer");
    executable(
        &killer,
        b"#!/bin/sh\nkill -TERM $PPID\nsleep 1\necho 'ghtui 0.2.0'\n",
    );
    lab.publish("v0.2.0", &[(X86, &killer)], false);
    let interrupted = lab.install(&["--version", "v0.2.0"]);
    assert!(!interrupted.status.success(), "{}", text(&interrupted));
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&old).unwrap());
    assert_eq!(mode(&lab.installed()), 0o755);
    lab.assert_no_leftovers();

    // Two upgrades at once both finish and leave one complete binary.
    let new = lab.fake_binary("0.3.0");
    lab.publish("v0.3.0", &[(X86, &new)], true);
    let children: Vec<_> = (0..2)
        .map(|_| {
            lab.command()
                .arg(script("install.sh"))
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap()
        })
        .collect();
    for child in children {
        assert_success(&child.wait_with_output().unwrap());
    }
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&new).unwrap());
    assert_eq!(mode(&lab.installed()), 0o755);
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_refuses_unrelated_files_and_guides_missing_tools_and_path() {
    let lab = Lab::new();
    let binary = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(X86, &binary)], true);

    // An unrelated file named ghtui is neither replaced nor uninstalled.
    fs::create_dir_all(lab.home().join(".local/bin")).unwrap();
    let unrelated = "#!/bin/sh\necho someone else\n";
    fs::write(lab.installed(), unrelated).unwrap();
    assert_failure(&lab.install(&[]), "does not look like ghtui");
    assert_failure(&lab.install(&["--uninstall"]), "does not look like ghtui");
    assert_eq!(fs::read_to_string(lab.installed()).unwrap(), unrelated);
    fs::remove_file(lab.installed()).unwrap();

    // Missing curl is reported with a fix before anything is written.
    let fakebin = lab.root.join("fakebin");
    let no_curl = tool_dir(&lab, "no-curl", &INSTALL_TOOLS[1..]);
    let output = lab
        .command()
        .env(
            "PATH",
            format!("{}:{}", fakebin.display(), no_curl.display()),
        )
        .arg(script("install.sh"))
        .output()
        .unwrap();
    assert_failure(
        &output,
        "curl is required; install it with your system package manager",
    );
    assert!(!lab.installed().exists());

    // Without git and gh the install succeeds and explains both, leaving gh
    // authentication to gh. Quoted spaces and Unicode paths work, and an
    // earlier ghtui on PATH is reported as shadowing the new one.
    let tools = tool_dir(&lab, "tools", INSTALL_TOOLS);
    let target = lab.root.join("home/my apps/ünïcode bin");
    let shadow = lab.root.join("shadow");
    fs::create_dir_all(&shadow).unwrap();
    executable(&shadow.join("ghtui"), b"#!/bin/sh\necho 'ghtui 0.0.1'\n");
    let output = lab
        .command()
        .env(
            "PATH",
            format!(
                "{}:{}:{}:{}",
                fakebin.display(),
                tools.display(),
                shadow.display(),
                target.display()
            ),
        )
        .arg(script("install.sh"))
        .arg("--install-dir")
        .arg(&target)
        .output()
        .unwrap();
    assert_success(&output);
    let log = text(&output);
    assert_eq!(
        fs::read(target.join("ghtui")).unwrap(),
        fs::read(&binary).unwrap()
    );
    assert!(
        log.contains(&format!(
            "another ghtui earlier on PATH shadows this one: {}",
            shadow.join("ghtui").display()
        )),
        "{log}"
    );
    assert!(log.contains("git is not installed"), "{log}");
    assert!(
        log.contains("gh is not installed") && log.contains("gh auth login"),
        "{log}"
    );
    assert!(!lab.home().join(".bashrc").exists());
    assert!(!lab.home().join(".profile").exists());
    lab.assert_no_leftovers();
}

fn yaml(name: &str) -> Yaml {
    let path = Path::new(ROOT).join(".github/workflows").join(name);
    let source = fs::read_to_string(path).unwrap();
    YamlLoader::load_from_str(&source).unwrap().remove(0)
}
fn runs(job: &Yaml) -> String {
    job["steps"]
        .as_vec()
        .unwrap()
        .iter()
        .filter_map(|step| step["run"].as_str().or(step["uses"].as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn v1_release_workflows_build_both_musl_targets_and_gate_ci() {
    let release = yaml("release.yml");
    assert_eq!(release["on"]["push"]["tags"][0].as_str(), Some("v*"));
    let build = &release["jobs"]["build"];
    let targets: Vec<_> = build["strategy"]["matrix"]["include"]
        .as_vec()
        .unwrap()
        .iter()
        .map(|entry| entry["target"].as_str().unwrap())
        .collect();
    assert_eq!(targets, [X86, ARM]);
    let build_steps = runs(build);
    assert!(build_steps.contains("cargo zigbuild --locked --release"));
    assert!(build_steps.contains("--version"));
    assert!(build_steps.contains("scripts/package-release.sh archive"));
    let publish = &release["jobs"]["publish"];
    assert_eq!(publish["permissions"]["contents"].as_str(), Some("write"));
    let publish_steps = runs(publish);
    assert!(publish_steps.contains("scripts/package-release.sh finish"));
    assert!(publish_steps.contains("bash dist/install.sh --release-base \"file://"));
    assert!(publish_steps.contains("gh release create"));
    assert_eq!(release["permissions"]["contents"].as_str(), Some("read"));

    // The exact archive is installed by the script and driven by the
    // installed-binary keyboard/mouse PTY journeys before upload.
    let install_check = build_steps.find("GHTUI_E2E_RELEASE_DIST").unwrap();
    assert!(build_steps[install_check..].contains("cargo test --locked --test tui_e2e v1_release"));
    assert!(install_check < build_steps.find("actions/upload-artifact").unwrap());
    // Upload as a draft, prove the uploaded bytes are the tested bytes, and
    // only then publish; a partial matrix is refused first.
    let order = [
        "missing archive for",
        "bash dist/install.sh",
        "--verify-tag --draft",
        "gh release download",
        "sha256sum -c SHA256SUMS",
        "--draft=false --latest",
    ];
    let positions: Vec<_> = order
        .iter()
        .map(|needle| {
            publish_steps
                .find(needle)
                .unwrap_or_else(|| panic!("publish lacks {needle}"))
        })
        .collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "{order:?} out of order"
    );
    assert_eq!(publish["permissions"]["id-token"].as_str(), Some("write"));
    assert!(publish_steps.contains("actions/attest-build-provenance"));
    assert_eq!(
        release["concurrency"]["cancel-in-progress"].as_bool(),
        Some(false)
    );
    assert!(release["on"]["pull_request"].is_badvalue());
    assert!(release["on"]["pull_request_target"].is_badvalue());

    let ci = yaml("ci.yml");
    assert_eq!(ci["env"]["RUST_TEST_THREADS"].as_str(), Some("2"));
    let all: String = ["lint", "installer", "test"]
        .iter()
        .map(|job| runs(&ci["jobs"][*job]))
        .collect::<Vec<_>>()
        .join("\n");
    for needle in [
        "cargo fmt --all --check",
        "cargo clippy --locked --all-targets -- -D warnings",
        "node scripts/test-all.mjs",
        "shellcheck scripts/install.sh",
    ] {
        assert!(all.contains(needle), "ci.yml lacks {needle}");
    }
    // Fork pull requests run with a read-only token and no secrets.
    assert_eq!(ci["permissions"]["contents"].as_str(), Some("read"));
    assert!(ci["on"]["pull_request_target"].is_badvalue());
    assert!(
        all.contains("actions/upload-artifact"),
        "no failure log upload"
    );

    // Every action names a ref and no checkout leaves a token in .git.
    for workflow in [&ci, &release] {
        for job in workflow["jobs"].as_hash().unwrap().values() {
            for step in job["steps"].as_vec().unwrap() {
                if let Some(uses) = step["uses"].as_str() {
                    let (_, reference) = uses.split_once('@').expect("unpinned action");
                    assert!(!reference.is_empty() && reference != "main" && reference != "master");
                    if uses.starts_with("actions/checkout@") {
                        assert_eq!(step["with"]["persist-credentials"].as_bool(), Some(false));
                    }
                }
            }
        }
    }
}
