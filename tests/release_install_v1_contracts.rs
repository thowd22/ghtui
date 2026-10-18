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
        let mut command = Command::new("bash");
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
}
