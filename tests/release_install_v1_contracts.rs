//! Real Bash runs of scripts/install.sh against offline file:// and loopback
//! HTTPS releases that scripts/package-release.sh assembles, plus workflow
//! shape checks. Stand-in binaries are tiny native ELF programs (see
//! release_support/stand_in.rs) because the installer reads the executable
//! header before it ever runs a download.
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::BufRead,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Child, Command, Output, Stdio},
};
use yaml_rust2::{Yaml, YamlLoader};

#[path = "release_support/stand_in.rs"]
mod stand_in;
use stand_in::{Arch, Program};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const X86: &str = "x86_64-unknown-linux-musl";
const ARM: &str = "aarch64-unknown-linux-musl";

fn host() -> &'static str {
    Arch::host().linux_target()
}
fn foreign() -> &'static str {
    Arch::foreign().linux_target()
}

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
        lab.uname("Linux", Arch::host().uname());
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
    fn marker(&self) -> PathBuf {
        self.root.join("executed")
    }
    fn uname(&self, os: &str, cpu: &str) {
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in -s) echo {os} ;; -m) echo {cpu} ;; *) echo {os} ;; esac\n"
        );
        executable(&self.root.join("fakebin/uname"), script.as_bytes());
    }
    fn stand_in(&self, name: &str, arch: Arch, program: &Program) -> PathBuf {
        let path = self.root.join(format!("stage/{name}"));
        executable(&path, &stand_in::elf(arch, program));
        path
    }
    /// A native stand-in for this host that answers `--version` like ghtui.
    fn fake_binary(&self, reported: &str) -> PathBuf {
        let message = format!("ghtui {reported}\n");
        self.stand_in(
            &format!("ghtui-{reported}"),
            Arch::host(),
            &Program {
                message: &message,
                ..Program::default()
            },
        )
    }
    /// The same for the other CPU: a valid binary this host cannot run.
    fn foreign_binary(&self, reported: &str) -> PathBuf {
        let message = format!("ghtui {reported}\n");
        self.stand_in(
            &format!("foreign-{reported}"),
            Arch::foreign(),
            &Program {
                message: &message,
                ..Program::default()
            },
        )
    }
    /// A host stand-in that creates `marker()` when it runs.
    fn tripwire(&self, reported: &str) -> PathBuf {
        let message = format!("ghtui {reported}\n");
        let marker = self.marker();
        self.stand_in(
            &format!("tripwire-{reported}"),
            Arch::host(),
            &Program {
                message: &message,
                touch: Some(&marker),
                ..Program::default()
            },
        )
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
    /// Publish a hand-made archive (bypassing the packager's checks) with a
    /// correct SHA256SUMS entry, so only the installer's own checks apply.
    fn publish_raw(&self, version: &str, target: &str, archive: &[u8]) {
        let exact = self.root.join(format!("release/download/{version}"));
        fs::create_dir_all(&exact).unwrap();
        let name = format!("ghtui-{version}-{target}.tar.gz");
        fs::write(exact.join(&name), archive).unwrap();
        fs::write(
            exact.join("SHA256SUMS"),
            format!("{}  {name}\n", sha256(archive)),
        )
        .unwrap();
    }
    /// tar.gz bytes of `members` inside a fresh payload directory that
    /// `prepare` fills.
    fn tarball(&self, label: &str, prepare: impl FnOnce(&Path), members: &[&str]) -> Vec<u8> {
        let payload = self.root.join(format!("stage/payload-{label}"));
        fs::create_dir_all(&payload).unwrap();
        prepare(&payload);
        let out = self.root.join(format!("stage/{label}.tar.gz"));
        let status = Command::new("tar")
            .arg("-C")
            .arg(&payload)
            .arg("-czPf")
            .arg(&out)
            .args(members)
            .status()
            .unwrap();
        assert!(status.success(), "tar {label}");
        fs::read(out).unwrap()
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
fn host_tool(tool: &str) -> PathBuf {
    ["/usr/bin", "/bin"]
        .iter()
        .map(|prefix| Path::new(prefix).join(tool))
        .find(|path| path.exists())
        .unwrap_or_else(|| panic!("host tool {tool} is missing"))
}

#[test]
fn v1_release_curl_pipe_bash_installs_latest_verified_binary_into_local_bin() {
    let lab = Lab::new();
    let binary = lab.fake_binary("0.1.0");
    let other = lab.foreign_binary("0.1.0");
    let exact = lab.publish("v0.1.0", &[(host(), &binary), (foreign(), &other)], true);
    let sums = fs::read_to_string(exact.join("SHA256SUMS")).unwrap();
    let archive = fs::read(exact.join(format!("ghtui-v0.1.0-{}.tar.gz", host()))).unwrap();
    assert!(sums.contains(&format!(
        "{}  ghtui-v0.1.0-{}.tar.gz",
        sha256(&archive),
        host()
    )));
    assert!(sums.contains(&format!("ghtui-v0.1.0-{}.tar.gz", foreign())));
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
    assert!(log.contains(&format!("ghtui v0.1.0 ({})", host())), "{log}");
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
    // The installed path itself runs and reports the release version.
    let reported = Command::new(lab.installed())
        .arg("--version")
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&reported.stdout), "ghtui 0.1.0\n");
    // No shell profile was written and nothing else appeared in HOME.
    let home: Vec<_> = fs::read_dir(lab.home())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(home, vec![std::ffi::OsString::from(".local")]);
    lab.assert_no_leftovers();

    // Repeating the same version is a no-op that reports the current state.
    let inode = std::os::unix::fs::MetadataExt::ino(&fs::metadata(lab.installed()).unwrap());
    let again = lab.install(&[]);
    assert_success(&again);
    assert!(text(&again).contains("ghtui v0.1.0 is already installed"));
    assert_eq!(
        std::os::unix::fs::MetadataExt::ino(&fs::metadata(lab.installed()).unwrap()),
        inode,
        "a same-version rerun must not rewrite the binary"
    );
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_maps_aarch64_and_pins_or_upgrades_exact_versions() {
    let lab = Lab::new();
    let old = lab.fake_binary("0.1.0");
    let old_foreign = lab.foreign_binary("0.1.0");
    lab.publish(
        "v0.1.0",
        &[(host(), &old), (foreign(), &old_foreign)],
        false,
    );
    let new = lab.fake_binary("0.2.0");
    let new_foreign = lab.foreign_binary("0.2.0");
    lab.publish("v0.2.0", &[(host(), &new), (foreign(), &new_foreign)], true);

    let pinned = lab.install(&["--version", "0.1.0"]);
    assert_success(&pinned);
    assert!(text(&pinned).contains(&format!("ghtui v0.1.0 ({})", host())));
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&old).unwrap());

    let latest = lab.install(&[]);
    assert_success(&latest);
    assert!(text(&latest).contains("installed ghtui v0.2.0"));
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&new).unwrap());

    // GHTUI_VERSION also works with curl | bash where flags are awkward, and
    // an explicit older version is a rollback.
    let rollback = lab
        .command()
        .env("GHTUI_VERSION", "v0.1.0")
        .arg(script("install.sh"))
        .output()
        .unwrap();
    assert_success(&rollback);
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&old).unwrap());

    // A version that was never published fails cleanly and keeps the binary.
    let missing = lab.install(&["--version", "v9.9.9"]);
    assert_failure(&missing, "download failed");
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&old).unwrap());

    // Both Linux CPU names map to their own archive. The other CPU's archive
    // passes every check up to running it, which fails on this host and
    // leaves the installed binary alone.
    // (A host with user-mode CPU emulation registered can run it; then the
    // install must succeed instead.)
    let emulated = Command::new(&new_foreign)
        .output()
        .is_ok_and(|output| output.status.success());
    let alias = if Arch::foreign() == Arch::Aarch64 {
        "arm64"
    } else {
        "amd64"
    };
    for cpu in [Arch::foreign().uname(), alias] {
        lab.uname("Linux", cpu);
        let output = lab.install(&["--version", "v0.2.0"]);
        if emulated {
            assert_success(&output);
            assert_success(&lab.install(&["--version", "v0.1.0"]));
            lab.uname("Linux", Arch::host().uname());
            assert_success(&lab.install(&["--version", "v0.1.0"]));
            continue;
        }
        assert_failure(
            &output,
            "does not run on this system; nothing was installed",
        );
        assert!(
            text(&output).contains(&format!("ghtui v0.2.0 ({})", foreign())),
            "{}",
            text(&output)
        );
        assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&old).unwrap());
    }
    for invalid in ["latest", "v1.2", "v1.2.3;rm", "1.2.3 4"] {
        lab.uname("Linux", Arch::host().uname());
        assert_failure(&lab.install(&["--version", invalid]), "invalid version");
    }
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_resolves_latest_once_and_requests_only_that_exact_version() {
    let lab = Lab::new();
    let old = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(host(), &old)], false);
    let new = lab.fake_binary("0.2.0");
    lab.publish("v0.2.0", &[(host(), &new)], true);
    // A recording curl in front of the real one, and only the documented
    // host tools on PATH: no Rust, Node or jq is available to the script.
    let log = lab.root.join("urls");
    executable(
        &lab.root.join("fakebin/curl"),
        format!(
            "#!/bin/sh\nfor last; do :; done\nprintf '%s\\n' \"$last\" >> '{}'\nexec '{}' \"$@\"\n",
            log.display(),
            host_tool("curl").display()
        )
        .as_bytes(),
    );
    let tools = tool_dir(&lab, "tools", INSTALL_TOOLS);
    let path = format!("{}:{}", lab.root.join("fakebin").display(), tools.display());
    for absent in ["cargo", "rustc", "node", "jq", "python3"] {
        assert!(!tools.join(absent).exists());
    }
    let output = lab
        .command()
        .env("PATH", &path)
        .arg(script("install.sh"))
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&new).unwrap());
    let base = lab.base();
    let archive = format!("ghtui-v0.2.0-{}.tar.gz", host());
    assert_eq!(
        fs::read_to_string(&log).unwrap(),
        format!(
            "{base}/latest/download/release-version.txt\n{base}/download/v0.2.0/SHA256SUMS\n{base}/download/v0.2.0/{archive}\n"
        )
    );

    // A pinned install never asks what "latest" is.
    fs::remove_file(&log).unwrap();
    let pinned = lab
        .command()
        .env("PATH", &path)
        .arg(script("install.sh"))
        .args(["--version", "v0.1.0"])
        .output()
        .unwrap();
    assert_success(&pinned);
    let urls = fs::read_to_string(&log).unwrap();
    assert!(!urls.contains("/latest/"), "{urls}");
    assert_eq!(urls.lines().count(), 2, "{urls}");
    assert!(urls.lines().all(|url| url.contains("/download/v0.1.0/")));
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&old).unwrap());

    // A latest pointer that names no valid version is refused before any
    // other request.
    fs::write(
        lab.root.join("release/latest/download/release-version.txt"),
        "latest\n",
    )
    .unwrap();
    fs::remove_file(&log).unwrap();
    assert_failure(
        &lab.command()
            .env("PATH", &path)
            .arg(script("install.sh"))
            .output()
            .unwrap(),
        "the latest release did not name a valid version",
    );
    assert_eq!(fs::read_to_string(&log).unwrap().lines().count(), 1);
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&old).unwrap());
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_checksum_mismatch_never_replaces_the_previous_binary() {
    let lab = Lab::new();
    let good = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(host(), &good)], true);
    assert_success(&lab.install(&[]));

    let next = lab.fake_binary("0.2.0");
    let exact = lab.publish("v0.2.0", &[(host(), &next)], false);
    let archive = exact.join(format!("ghtui-v0.2.0-{}.tar.gz", host()));
    let mut bytes = fs::read(&archive).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    fs::write(&archive, bytes).unwrap();
    let output = lab.install(&["--version", "v0.2.0"]);
    assert_failure(&output, "checksum mismatch");
    assert!(text(&output).contains("nothing was installed"));
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&good).unwrap());

    // A SHA256SUMS without an entry for this target is refused too, and so
    // are duplicate or malformed entries.
    let sums = exact.join("SHA256SUMS");
    fs::write(&sums, "0000  other.tar.gz\n").unwrap();
    assert_failure(
        &lab.install(&["--version", "v0.2.0"]),
        "SHA256SUMS has no single entry",
    );
    let name = format!("ghtui-v0.2.0-{}.tar.gz", host());
    fs::write(
        &sums,
        format!("{}  {name}\n{}  {name}\n", "a".repeat(64), "b".repeat(64)),
    )
    .unwrap();
    assert_failure(
        &lab.install(&["--version", "v0.2.0"]),
        "SHA256SUMS has no single entry",
    );
    fs::write(&sums, format!("{}  {name}\n", "Z".repeat(64))).unwrap();
    assert_failure(&lab.install(&["--version", "v0.2.0"]), "malformed checksum");
    fs::remove_file(&sums).unwrap();
    assert_failure(&lab.install(&["--version", "v0.2.0"]), "download failed");
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&good).unwrap());
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_refuses_unsafe_archives_and_wrong_version_binaries() {
    let lab = Lab::new();
    // Binary that reports a different version than the tag it shipped under.
    let wrong = lab.fake_binary("0.0.9");
    lab.publish("v0.3.0", &[(host(), &wrong)], false);
    assert_failure(
        &lab.install(&["--version", "v0.3.0"]),
        "reports 'ghtui 0.0.9', expected 'ghtui 0.3.0'",
    );
    assert!(!lab.installed().exists());

    // Correctly checksummed archives whose members are unsafe. None of them
    // may run the tripwire or install anything.
    let tripwire = fs::read(lab.tripwire("0.4.0")).unwrap();
    let write_tripwire = |dir: &Path, name: &str| executable(&dir.join(name), &tripwire);
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        (
            "extra member",
            lab.tarball(
                "extra",
                |dir| {
                    write_tripwire(dir, "ghtui");
                    fs::write(dir.join("evil"), b"x").unwrap();
                },
                &["ghtui", "evil"],
            ),
            "must contain exactly one file named ghtui",
        ),
        (
            "dot-slash path",
            lab.tarball("dot", |dir| write_tripwire(dir, "ghtui"), &["./ghtui"]),
            "must contain exactly one file named ghtui",
        ),
        (
            "parent path",
            lab.tarball(
                "parent",
                |dir| {
                    fs::create_dir(dir.join("inner")).unwrap();
                    write_tripwire(dir, "ghtui");
                },
                &["inner/../ghtui"],
            ),
            "must contain exactly one file named ghtui",
        ),
        (
            "symlink member",
            lab.tarball(
                "symlink",
                |dir| {
                    std::os::unix::fs::symlink("/bin/sh", dir.join("ghtui")).unwrap();
                },
                &["ghtui"],
            ),
            "must contain exactly one regular file named ghtui",
        ),
        (
            "directory member",
            lab.tarball(
                "directory",
                |dir| {
                    fs::create_dir(dir.join("ghtui")).unwrap();
                },
                &["ghtui"],
            ),
            "must contain exactly one file named ghtui",
        ),
        (
            "not gzip",
            b"this is not a tar.gz archive".to_vec(),
            "is not a valid tar.gz archive",
        ),
    ];
    for (label, archive, needle) in cases {
        lab.publish_raw("v0.4.0", host(), &archive);
        let output = lab.install(&["--version", "v0.4.0"]);
        assert_failure(&output, needle);
        assert!(!lab.marker().exists(), "{label}: the archive's binary ran");
        assert!(
            !lab.installed().exists(),
            "{label}: something was installed"
        );
        lab.assert_no_leftovers();
    }

    // A binary built for the other CPU under this CPU's archive name is
    // refused from its header, before it could run.
    let message = "ghtui 0.4.0\n".to_string();
    let marker = lab.marker();
    let foreign_tripwire = stand_in::elf(
        Arch::foreign(),
        &Program {
            message: &message,
            touch: Some(&marker),
            ..Program::default()
        },
    );
    let archive = lab.tarball(
        "foreign",
        |dir| executable(&dir.join("ghtui"), &foreign_tripwire),
        &["ghtui"],
    );
    lab.publish_raw("v0.4.0", host(), &archive);
    assert_failure(
        &lab.install(&["--version", "v0.4.0"]),
        &format!("holds a binary built for {}, not {}", foreign(), host()),
    );
    // A script is not a native binary at all.
    let archive = lab.tarball(
        "script",
        |dir| executable(&dir.join("ghtui"), b"#!/bin/sh\necho 'ghtui 0.4.0'\n"),
        &["ghtui"],
    );
    lab.publish_raw("v0.4.0", host(), &archive);
    assert_failure(
        &lab.install(&["--version", "v0.4.0"]),
        &format!("holds a binary built for unknown, not {}", host()),
    );
    assert!(!lab.marker().exists());
    assert!(!lab.installed().exists());

    // An existing symlink or directory at the destination is not ours to
    // replace or remove.
    let good = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(host(), &good)], true);
    fs::create_dir_all(lab.home().join(".local/bin")).unwrap();
    std::os::unix::fs::symlink("/bin/true", lab.installed()).unwrap();
    assert_failure(&lab.install(&[]), "is not a regular file");
    assert_failure(&lab.install(&["--uninstall"]), "is a symlink");
    assert_eq!(
        fs::read_link(lab.installed()).unwrap(),
        Path::new("/bin/true")
    );
    fs::remove_file(lab.installed()).unwrap();
    fs::create_dir(lab.installed()).unwrap();
    assert_failure(&lab.install(&[]), "is not a regular file");
    assert_failure(&lab.install(&["--uninstall"]), "is not a regular file");
    assert!(lab.installed().is_dir());
    fs::remove_dir(lab.installed()).unwrap();
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_rejects_unsupported_platforms_and_insecure_release_urls() {
    let lab = Lab::new();
    let binary = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(host(), &binary)], true);
    lab.uname("Linux", "riscv64");
    assert_failure(
        &lab.install(&[]),
        "unsupported Linux CPU architecture: riscv64",
    );
    lab.uname("FreeBSD", "amd64");
    assert_failure(&lab.install(&[]), "unsupported operating system: FreeBSD");
    lab.uname("Darwin", "ppc64");
    assert_failure(
        &lab.install(&[]),
        "unsupported macOS CPU architecture: ppc64",
    );
    lab.uname("Linux", Arch::host().uname());
    assert_failure(
        &lab.install(&["--release-base", "http://example.com/releases"]),
        "release base must be an https:// URL",
    );
    assert_failure(
        &lab.install(&["--release-base", "ftp://example.com/releases"]),
        "release base must be an https:// URL",
    );
    assert_failure(
        &lab.install(&["--install-dir", "relative/bin"]),
        "absolute path",
    );
    assert_failure(
        &lab.install(&["--install-dir", "/"]),
        "refusing to install into /",
    );
    assert_failure(&lab.install(&["--bogus"]), "unknown option: --bogus");
    assert_failure(&lab.install(&["--version"]), "--version needs a value");
    assert!(!lab.home().join(".local").exists());
    lab.assert_no_leftovers();
    let help = lab.install(&["--help"]);
    assert_success(&help);
    assert!(text(&help).contains("--uninstall"));
    assert!(!lab.home().join(".local").exists());
}

#[test]
fn v1_release_maps_macos_targets_and_explains_missing_macos_builds() {
    let lab = Lab::new();
    let binary = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(host(), &binary)], true);

    // A Linux-only release explains that macOS needs a source build, for
    // each Mac CPU, without downloading an archive.
    for (cpu, target) in [
        ("arm64", "aarch64-apple-darwin"),
        ("x86_64", "x86_64-apple-darwin"),
    ] {
        lab.uname("Darwin", cpu);
        let output = lab.install(&[]);
        assert_failure(
            &output,
            &format!("release v0.1.0 has no ghtui build for {target}; build from source"),
        );
        assert!(!lab.installed().exists());
    }
    // Under Rosetta an x86_64 shell on Apple silicon gets the arm64 build.
    executable(
        &lab.root.join("fakebin/sysctl"),
        b"#!/bin/sh\n[ \"$1 $2\" = '-n sysctl.proc_translated' ] && echo 1\n",
    );
    lab.uname("Darwin", "x86_64");
    assert_failure(&lab.install(&[]), "no ghtui build for aarch64-apple-darwin");

    // A release with macOS archives: the Mach-O header for this Mac's CPU
    // passes the checksum and target checks; this Linux host then cannot run
    // it, so nothing is installed. A Linux binary under the macOS name is
    // refused from its header. Native macOS runs are separate evidence.
    let dir = lab.root.join("stage/macos");
    fs::create_dir_all(&dir).unwrap();
    let mac_arm = dir.join("ghtui-arm");
    let mac_x86 = dir.join("ghtui-x86");
    executable(&mac_arm, &stand_in::macho(Arch::Aarch64, "ghtui 0.2.0"));
    executable(&mac_x86, &stand_in::macho(Arch::X86_64, "ghtui 0.2.0"));
    let linux = lab.fake_binary("0.2.0");
    lab.publish(
        "v0.2.0",
        &[
            (host(), &linux),
            ("aarch64-apple-darwin", &mac_arm),
            ("x86_64-apple-darwin", &mac_x86),
        ],
        true,
    );
    for (cpu, target) in [
        ("arm64", "aarch64-apple-darwin"),
        ("x86_64", "aarch64-apple-darwin"),
    ] {
        lab.uname("Darwin", cpu);
        let output = lab.install(&[]);
        assert_failure(
            &output,
            "does not run on this system; nothing was installed",
        );
        assert!(
            text(&output).contains(&format!("ghtui v0.2.0 ({target})")),
            "{}",
            text(&output)
        );
    }
    fs::remove_file(lab.root.join("fakebin/sysctl")).unwrap();
    lab.uname("Darwin", "x86_64");
    let output = lab.install(&[]);
    assert_failure(&output, "does not run on this system");
    assert!(text(&output).contains("(x86_64-apple-darwin)"));
    let archive = lab.tarball(
        "elf-as-mac",
        |dir| {
            fs::copy(&linux, dir.join("ghtui")).unwrap();
        },
        &["ghtui"],
    );
    lab.publish_raw("v0.3.0", "x86_64-apple-darwin", &archive);
    assert_failure(
        &lab.install(&["--version", "v0.3.0"]),
        &format!(
            "holds a binary built for {}, not x86_64-apple-darwin",
            host()
        ),
    );
    assert!(!lab.installed().exists());
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_packager_checks_targets_and_binary_headers() {
    let lab = Lab::new();
    let dist = lab.root.join("stage/packager");
    let package = |target: &str, binary: &Path| {
        Command::new("bash")
            .arg(script("package-release.sh"))
            .args(["archive", "v0.1.0", target])
            .arg(binary)
            .arg(&dist)
            .output()
            .unwrap()
    };
    let host_binary = lab.fake_binary("0.1.0");
    let foreign_binary = lab.foreign_binary("0.1.0");
    let mac = lab.root.join("stage/mac");
    executable(&mac, &stand_in::macho(Arch::Aarch64, "ghtui 0.1.0"));
    assert_success(&package(host(), &host_binary));
    assert_success(&package(foreign(), &foreign_binary));
    assert_success(&package("aarch64-apple-darwin", &mac));
    assert_failure(
        &package(foreign(), &host_binary),
        &format!("is built for {}, not {}", host(), foreign()),
    );
    assert_failure(
        &package("x86_64-apple-darwin", &mac),
        "is built for aarch64-apple-darwin, not x86_64-apple-darwin",
    );
    assert_failure(
        &package("riscv64gc-unknown-linux-gnu", &host_binary),
        "unsupported target",
    );
    let script_binary = lab.root.join("stage/script");
    executable(&script_binary, b"#!/bin/sh\necho ghtui\n");
    assert_failure(&package(host(), &script_binary), "is built for unknown");
    // Each archive holds exactly one regular executable named ghtui.
    let listing = Command::new("tar")
        .arg("-tvzf")
        .arg(dist.join(format!("ghtui-v0.1.0-{}.tar.gz", host())))
        .output()
        .unwrap();
    let listing = String::from_utf8_lossy(&listing.stdout).into_owned();
    assert_eq!(listing.lines().count(), 1, "{listing}");
    assert!(listing.starts_with("-rwxr-xr-x"), "{listing}");
    assert!(listing.trim_end().ends_with(" ghtui"), "{listing}");
}

#[test]
fn v1_release_truncated_download_runs_nothing() {
    let lab = Lab::new();
    let binary = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(host(), &binary)], true);
    let full = fs::read(script("install.sh")).unwrap();
    // Every prefix that stops before the final closing brace must be inert.
    let end = full.len() - full.iter().rev().position(|b| *b == b'}').unwrap() - 1;
    let lines: Vec<usize> = full
        .iter()
        .enumerate()
        .filter(|(_, byte)| **byte == b'\n')
        .map(|(at, _)| at + 1)
        .collect();
    let mut cuts = vec![0, 1, full.len() / 4, full.len() / 2, end - 10, end];
    // Common streamed prefixes: whole lines, as a slow network delivers them.
    cuts.extend(lines.iter().copied().step_by(7).filter(|cut| *cut <= end));
    for cut in cuts {
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
    lab.publish("v0.1.0", &[(host(), &binary)], true);
    assert_success(&lab.install(&[]));
    // Configuration, the SQLite cache that also holds drafts, the search
    // index, avatars (Linux and macOS locations) and a repository.
    let kept: Vec<(PathBuf, &str)> = vec![
        (
            lab.home().join(".config/ghtui/config.toml"),
            "version = 1\n",
        ),
        (
            lab.home().join(".config/ghtui/cache.sqlite3"),
            "SQLite format 3\0drafts",
        ),
        (lab.home().join(".config/ghtui/index/meta.json"), "{}"),
        (lab.home().join(".config/ghtui/avatars/1.png"), "png"),
        (
            lab.home()
                .join("Library/Application Support/ghtui/config.toml"),
            "version = 1\n",
        ),
        (
            lab.home().join("code/project/.git/HEAD"),
            "ref: refs/heads/main\n",
        ),
        (lab.home().join(".local/bin/other-tool"), "keep"),
    ];
    for (path, bytes) in &kept {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, bytes).unwrap();
    }

    let output = lab.install(&["--uninstall"]);
    assert_success(&output);
    assert!(text(&output).contains("configuration, cache, drafts and repositories were kept"));
    assert!(!lab.installed().exists());
    for (path, bytes) in &kept {
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            *bytes,
            "{path:?} changed"
        );
    }
    let again = lab.install(&["--uninstall"]);
    assert_success(&again);
    assert!(text(&again).contains("nothing to remove"));

    // Uninstall honours --install-dir and never touches the default one.
    let custom = lab.root.join("custom bin");
    assert_success(&lab.install(&["--install-dir", custom.to_str().unwrap()]));
    assert_success(&lab.install(&[]));
    assert_success(&lab.install(&["--uninstall", "--install-dir", custom.to_str().unwrap()]));
    assert!(!custom.join("ghtui").exists());
    assert!(lab.installed().exists());
    // Uninstall needs no download, curl or release: grep and rm suffice.
    let minimal = tool_dir(&lab, "uninstall-tools", &["grep", "rm"]);
    let no_tools = lab
        .command()
        .env("PATH", &minimal)
        .env("GHTUI_RELEASE_BASE", "https://unreachable.invalid")
        .arg(script("install.sh"))
        .arg("--uninstall")
        .output()
        .unwrap();
    assert_success(&no_tools);
    assert!(!lab.installed().exists());
}

/// A PATH directory holding only the named host tools, so a test controls
/// exactly which dependencies the installer can find.
fn tool_dir(lab: &Lab, name: &str, tools: &[&str]) -> PathBuf {
    let dir = lab.root.join(name);
    fs::create_dir_all(&dir).unwrap();
    for tool in tools {
        std::os::unix::fs::symlink(host_tool(tool), dir.join(tool)).unwrap();
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
    "od",
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
        ("--max-filesize", "209715200"),
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

/// A loopback HTTPS release server (Python's standard library) with a
/// self-signed certificate. `/redirect-https/<path>` redirects to the same
/// server and `/redirect-http/<path>` downgrades to plain http.
struct HttpsServer {
    child: Child,
    port: u16,
    cert: PathBuf,
}
impl Drop for HttpsServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
const HTTPS_SERVER: &str = r#"
import functools, http.server, ssl, sys
root, cert, key = sys.argv[1:4]
class Handler(http.server.SimpleHTTPRequestHandler):
    def log_message(self, *args):
        pass
    def do_GET(self):
        port = self.server.server_address[1]
        for prefix, target in (("/redirect-http/", "http://127.0.0.1:9/"), ("/redirect-https/", "https://127.0.0.1:%d/" % port)):
            if self.path.startswith(prefix):
                self.send_response(302)
                self.send_header("Location", target + self.path[len(prefix):])
                self.send_header("Content-Length", "0")
                self.end_headers()
                return
        super().do_GET()
server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), functools.partial(Handler, directory=root))
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain(cert, key)
server.socket = context.wrap_socket(server.socket, server_side=True)
print(server.server_address[1], flush=True)
server.serve_forever()
"#;
impl HttpsServer {
    fn start(lab: &Lab) -> Self {
        let tls = lab.root.join("tls");
        fs::create_dir_all(&tls).unwrap();
        let (cert, key) = (tls.join("cert.pem"), tls.join("key.pem"));
        let made = Command::new("openssl")
            .args([
                "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "2",
            ])
            .args([
                "-subj",
                "/CN=127.0.0.1",
                "-addext",
                "subjectAltName=IP:127.0.0.1",
            ])
            .arg("-keyout")
            .arg(&key)
            .arg("-out")
            .arg(&cert)
            .output()
            .expect("openssl is required for the loopback HTTPS release test");
        assert_success(&made);
        let mut child = Command::new("python3")
            .arg("-I")
            .arg("-c")
            .arg(HTTPS_SERVER)
            .arg(lab.root.join("release"))
            .arg(&cert)
            .arg(&key)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("python3 is required for the loopback HTTPS release test");
        let mut line = String::new();
        std::io::BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let port = line.trim().parse().expect("server port");
        Self { child, port, cert }
    }
    fn base(&self, prefix: &str) -> String {
        format!("https://127.0.0.1:{}{prefix}", self.port)
    }
}

#[test]
fn v1_release_real_https_verifies_certificates_and_refuses_downgrade_redirects() {
    let lab = Lab::new();
    let first = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(host(), &first)], false);
    let second = lab.fake_binary("0.2.0");
    lab.publish("v0.2.0", &[(host(), &second)], true);
    let server = HttpsServer::start(&lab);
    let install = |base: &str, trust: bool, version: &str| {
        let mut command = lab.command();
        command.env("GHTUI_RELEASE_BASE", base);
        if trust {
            command.env("CURL_CA_BUNDLE", &server.cert);
        }
        command
            .arg(script("install.sh"))
            .args(["--version", version])
            .output()
            .unwrap()
    };

    // An untrusted certificate fails even when ~/.curlrc asks for --insecure.
    fs::write(lab.home().join(".curlrc"), "insecure\n").unwrap();
    let untrusted = install(&server.base(""), false, "v0.1.0");
    assert_failure(&untrusted, "download failed: https://127.0.0.1:");
    assert!(
        text(&untrusted).to_lowercase().contains("certificate"),
        "{}",
        text(&untrusted)
    );
    assert!(!lab.installed().exists());

    // Trusting the server's certificate, HTTPS and HTTPS-to-HTTPS redirects
    // both install the verified bytes.
    assert_success(&install(&server.base(""), true, "v0.1.0"));
    assert_eq!(
        fs::read(lab.installed()).unwrap(),
        fs::read(&first).unwrap()
    );
    assert_success(&install(&server.base("/redirect-https"), true, "v0.2.0"));
    assert_eq!(
        fs::read(lab.installed()).unwrap(),
        fs::read(&second).unwrap()
    );

    // A redirect to plain http is refused before any connection is made,
    // and the installed binary stays as it was.
    let downgrade = install(&server.base("/redirect-http"), true, "v0.1.0");
    assert_failure(&downgrade, "download failed");
    assert!(
        text(&downgrade).to_lowercase().contains("protocol"),
        "{}",
        text(&downgrade)
    );
    assert_eq!(
        fs::read(lab.installed()).unwrap(),
        fs::read(&second).unwrap()
    );
    fs::remove_file(lab.home().join(".curlrc")).unwrap();
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_integrity_failure_never_executes_the_download() {
    let lab = Lab::new();
    let tripwire = lab.tripwire("0.5.0");
    let exact = lab.publish("v0.5.0", &[(host(), &tripwire)], true);
    let archive = exact.join(format!("ghtui-v0.5.0-{}.tar.gz", host()));
    let bytes = fs::read(&archive).unwrap();
    // A truncated (streamed prefix) archive, a wrong checksum and a malformed
    // SHA256SUMS all fail before the binary inside is ever run.
    for cut in [0, 1, 10, bytes.len() / 2, bytes.len() - 1] {
        fs::write(&archive, &bytes[..cut]).unwrap();
        assert_failure(&lab.install(&[]), "checksum mismatch");
    }
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
    assert!(
        !lab.marker().exists(),
        "the unverified download was executed"
    );
    assert!(!lab.installed().exists());
    // Control: the same tripwire, correctly listed, does run during the
    // version check, so the negative above is meaningful.
    fs::write(&sums, &listed).unwrap();
    assert_success(&lab.install(&["--version", "v0.5.0"]));
    assert!(lab.marker().exists());
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_concurrent_and_interrupted_upgrades_keep_a_working_binary() {
    let lab = Lab::new();
    let old = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(host(), &old)], false);
    assert_success(&lab.install(&["--version", "v0.1.0"]));
    let inode = std::os::unix::fs::MetadataExt::ino(&fs::metadata(lab.installed()).unwrap());

    // The new binary kills the installer while it is being verified.
    let killer = lab.stand_in(
        "killer",
        Arch::host(),
        &Program {
            message: "ghtui 0.2.0\n",
            kill_parent: true,
            ..Program::default()
        },
    );
    lab.publish("v0.2.0", &[(host(), &killer)], false);
    let interrupted = lab.install(&["--version", "v0.2.0"]);
    assert!(!interrupted.status.success(), "{}", text(&interrupted));
    assert_eq!(fs::read(lab.installed()).unwrap(), fs::read(&old).unwrap());
    assert_eq!(
        std::os::unix::fs::MetadataExt::ino(&fs::metadata(lab.installed()).unwrap()),
        inode
    );
    assert_eq!(mode(&lab.installed()), 0o755);
    lab.assert_no_leftovers();

    // Two upgrades at once both finish and leave one complete binary.
    let new = lab.fake_binary("0.3.0");
    lab.publish("v0.3.0", &[(host(), &new)], true);
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

    // A read-only install directory is refused up front; no sudo is tried.
    let locked = lab.root.join("locked");
    fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o555)).unwrap();
    let output = lab.install(&["--install-dir", locked.to_str().unwrap()]);
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
    if !text(&output).contains("installed ghtui") {
        // (root can write anywhere; the refusal only applies to normal users)
        assert_failure(&output, "is not writable (this installer never uses sudo)");
        assert!(!locked.join("ghtui").exists());
    }
    lab.assert_no_leftovers();
}

#[test]
fn v1_release_refuses_unrelated_files_and_guides_missing_tools_and_path() {
    let lab = Lab::new();
    let binary = lab.fake_binary("0.1.0");
    lab.publish("v0.1.0", &[(host(), &binary)], true);

    // An unrelated file named ghtui is neither replaced nor uninstalled.
    fs::create_dir_all(lab.home().join(".local/bin")).unwrap();
    let unrelated = "#!/bin/sh\necho someone else\n";
    fs::write(lab.installed(), unrelated).unwrap();
    assert_failure(&lab.install(&[]), "does not look like ghtui");
    assert_failure(&lab.install(&["--uninstall"]), "does not look like ghtui");
    assert_eq!(fs::read_to_string(lab.installed()).unwrap(), unrelated);
    fs::remove_file(lab.installed()).unwrap();

    // Run by a POSIX sh instead of Bash, the script explains what it needs
    // before doing anything.
    let dash = Command::new(host_tool("dash"))
        .env_clear()
        .env("HOME", lab.home())
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", lab.root.join("fakebin").display()),
        )
        .env("GHTUI_RELEASE_BASE", lab.base())
        .arg(script("install.sh"))
        .output()
        .unwrap();
    assert_failure(&dash, "this installer needs Bash 3.2 or newer");
    assert!(!lab.installed().exists());

    // Missing curl (or od) is reported with a fix before anything is written.
    let fakebin = lab.root.join("fakebin");
    for missing in ["curl", "od"] {
        let tools: Vec<&str> = INSTALL_TOOLS
            .iter()
            .copied()
            .filter(|tool| *tool != missing)
            .collect();
        let without = tool_dir(&lab, &format!("no-{missing}"), &tools);
        let output = lab
            .command()
            .env(
                "PATH",
                format!("{}:{}", fakebin.display(), without.display()),
            )
            .arg(script("install.sh"))
            .output()
            .unwrap();
        assert_failure(
            &output,
            &format!("{missing} is required; install it with your system package manager"),
        );
        assert!(!lab.installed().exists());
    }

    // Without git and gh the install succeeds and explains both, leaving gh
    // authentication to gh. Quoted spaces and Unicode paths work, and an
    // earlier ghtui on PATH is reported as shadowing the new one, without
    // being run or changed.
    let tools = tool_dir(&lab, "tools", INSTALL_TOOLS);
    let target = lab.root.join("home/my apps/ünïcode bin");
    let shadow = lab.root.join("shadow");
    fs::create_dir_all(&shadow).unwrap();
    let shadow_bytes = stand_in::elf(
        Arch::host(),
        &Program {
            message: "ghtui 0.0.1\n",
            touch: Some(&lab.marker()),
            ..Program::default()
        },
    );
    executable(&shadow.join("ghtui"), &shadow_bytes);
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
    assert!(
        log.contains(&format!("put {} before its directory", target.display())),
        "{log}"
    );
    assert_eq!(fs::read(shadow.join("ghtui")).unwrap(), shadow_bytes);
    assert!(!lab.marker().exists(), "the shadowing ghtui was run");
    assert!(log.contains("git is not installed"), "{log}");
    assert!(
        log.contains("gh is not installed") && log.contains("gh auth login"),
        "{log}"
    );
    assert!(!lab.home().join(".bashrc").exists());
    assert!(!lab.home().join(".profile").exists());
    assert!(!lab.home().join(".zshrc").exists());

    // GHTUI_INSTALL_DIR is the same override as --install-dir, and with
    // git and gh present nothing about them is printed.
    let env_dir = lab.root.join("env dir");
    let output = lab
        .command()
        .env("GHTUI_INSTALL_DIR", &env_dir)
        .arg(script("install.sh"))
        .output()
        .unwrap();
    assert_success(&output);
    assert_eq!(
        fs::read(env_dir.join("ghtui")).unwrap(),
        fs::read(&binary).unwrap()
    );
    if Path::new("/usr/bin/git").exists() {
        assert!(!text(&output).contains("git is not installed"));
    }
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
