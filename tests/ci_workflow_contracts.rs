//! Local contracts for .github/workflows/ci.yml and release.yml (TASK-69).
//!
//! These checks prove the workflow files' shape and run their version and
//! partial-matrix guard scripts with real Bash; they are NOT evidence of a
//! hosted or native GitHub run.
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
};
use yaml_rust2::{Yaml, YamlLoader};

const ROOT: &str = env!("CARGO_MANIFEST_DIR");
const WORKFLOWS: [&str; 2] = ["ci.yml", "release.yml"];
const X86: &str = "x86_64-unknown-linux-musl";
const ARM: &str = "aarch64-unknown-linux-musl";

fn source(name: &str) -> String {
    fs::read_to_string(Path::new(ROOT).join(".github/workflows").join(name)).unwrap()
}
fn yaml(name: &str) -> Yaml {
    YamlLoader::load_from_str(&source(name)).unwrap().remove(0)
}
fn jobs(workflow: &Yaml) -> Vec<(String, &Yaml)> {
    workflow["jobs"]
        .as_hash()
        .unwrap()
        .iter()
        .map(|(name, job)| (name.as_str().unwrap().to_owned(), job))
        .collect()
}
fn steps(job: &Yaml) -> &Vec<Yaml> {
    job["steps"].as_vec().unwrap()
}
fn step_named<'a>(job: &'a Yaml, name: &str) -> &'a Yaml {
    steps(job)
        .iter()
        .find(|step| step["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("no step named {name}"))
}
fn runs(job: &Yaml) -> String {
    steps(job)
        .iter()
        .filter_map(|step| step["run"].as_str().or(step["uses"].as_str()))
        .collect::<Vec<_>>()
        .join("\n")
}
fn uses(job: &Yaml) -> Vec<&str> {
    steps(job)
        .iter()
        .filter_map(|step| step["uses"].as_str())
        .collect()
}
fn permission_values(permissions: &Yaml) -> Vec<(String, String)> {
    permissions
        .as_hash()
        .unwrap_or_else(|| panic!("permissions must be an explicit map: {permissions:?}"))
        .iter()
        .map(|(k, v)| {
            (
                k.as_str().unwrap().to_owned(),
                v.as_str().unwrap().to_owned(),
            )
        })
        .collect()
}
fn bash(script: &str, dir: &Path, envs: &[(&str, &str)]) -> Output {
    Command::new("bash")
        .arg("-eo")
        .arg("pipefail")
        .arg("-c")
        .arg(script)
        .current_dir(dir)
        .envs(envs.iter().copied())
        .output()
        .unwrap()
}
fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn ci_workflow_every_action_is_pinned_to_a_full_commit_sha_with_its_release() {
    let mut seen = 0;
    for name in WORKFLOWS {
        for line in source(name).lines() {
            let Some(rest) = line
                .trim_start()
                .trim_start_matches("- ")
                .strip_prefix("uses:")
            else {
                continue;
            };
            let (reference, comment) = rest
                .split_once('#')
                .unwrap_or_else(|| panic!("{name}: `{line}` lacks a # release comment"));
            let reference = reference.trim();
            let (action, sha) = reference
                .split_once('@')
                .unwrap_or_else(|| panic!("{name}: unpinned action {reference}"));
            assert!(
                action.split('/').count() >= 2 && !action.starts_with('.'),
                "{name}: unexpected action {action}"
            );
            assert!(
                sha.len() == 40
                    && sha
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase()),
                "{name}: {action} is pinned to `{sha}`, not a full commit SHA"
            );
            assert!(
                comment
                    .trim()
                    .chars()
                    .next()
                    .is_some_and(|c| c == 'v' || c.is_ascii_digit()),
                "{name}: {action} comment `{comment}` does not name the release"
            );
            seen += 1;
        }
        // The parsed view agrees: every step `uses` is a SHA pin.
        for (job_name, job) in jobs(&yaml(name)) {
            for action in uses(job) {
                let sha = action.rsplit_once('@').unwrap().1;
                assert_eq!(sha.len(), 40, "{name}/{job_name}: {action}");
            }
        }
    }
    assert!(seen >= 15, "only {seen} pinned actions found");
}

#[test]
fn ci_workflow_permissions_are_explicit_least_privilege_and_fork_safe() {
    for name in WORKFLOWS {
        let workflow = yaml(name);
        let text = source(name);
        assert_eq!(
            permission_values(&workflow["permissions"]),
            [("contents".to_owned(), "read".to_owned())],
            "{name}: top-level token must be read-only"
        );
        // Fork code only ever runs under `pull_request`, never with the base
        // repository's token or secrets.
        for trigger in ["pull_request_target", "workflow_run", "issue_comment"] {
            assert!(workflow["on"][trigger].is_badvalue(), "{name}: {trigger}");
        }
        assert!(!text.contains("secrets."), "{name} references a secret");
        for (job_name, job) in jobs(&workflow) {
            let permissions = permission_values(&job["permissions"]);
            assert!(!permissions.is_empty(), "{name}/{job_name}: no permissions");
            let writes: Vec<_> = permissions
                .iter()
                .filter(|(_, level)| level != "read" && level != "none")
                .map(|(scope, _)| scope.as_str())
                .collect();
            if name == "release.yml" && job_name == "publish" {
                assert_eq!(writes, ["contents", "id-token", "attestations"]);
                assert_eq!(
                    job["if"].as_str(),
                    Some("${{ github.event_name == 'push' && startsWith(github.ref, 'refs/tags/v') }}")
                );
                assert_eq!(job["environment"].as_str(), Some("release"));
                assert!(job["needs"]
                    .as_vec()
                    .unwrap()
                    .iter()
                    .any(|need| need.as_str() == Some("build")));
            } else {
                assert!(writes.is_empty(), "{name}/{job_name} can write: {writes:?}");
            }
            assert!(
                job["timeout-minutes"].as_i64().is_some(),
                "{name}/{job_name}: no timeout"
            );
            for step in steps(job) {
                if step["uses"]
                    .as_str()
                    .is_some_and(|uses| uses.starts_with("actions/checkout@"))
                {
                    assert_eq!(
                        step["with"]["persist-credentials"].as_bool(),
                        Some(false),
                        "{name}/{job_name}: checkout keeps the token in .git"
                    );
                }
                if let Some(run) = step["run"].as_str() {
                    if run.contains("GH_TOKEN") || run.contains("gh release") {
                        assert_eq!((name, job_name.as_str()), ("release.yml", "publish"));
                    }
                }
            }
        }
    }
    let release = yaml("release.yml");
    assert!(release["on"]["pull_request"].is_badvalue());
    assert_eq!(release["on"]["push"]["tags"][0].as_str(), Some("v*"));
}

#[test]
fn ci_workflow_concurrency_caches_shell_and_failure_artifacts_are_explicit() {
    let ci = yaml("ci.yml");
    let release = yaml("release.yml");
    for workflow in [&ci, &release] {
        assert!(workflow["concurrency"]["group"].as_str().is_some());
        // bash -eo pipefail, so `cargo … | tee log` fails when cargo fails.
        assert_eq!(workflow["defaults"]["run"]["shell"].as_str(), Some("bash"));
    }
    assert_eq!(
        ci["concurrency"]["cancel-in-progress"].as_str(),
        Some("${{ github.event_name == 'pull_request' }}")
    );
    assert_eq!(
        release["concurrency"]["cancel-in-progress"].as_bool(),
        Some(false)
    );

    // CI caches: one key per job/platform, written only from main.
    let mut keys = Vec::new();
    for (job_name, job) in jobs(&ci) {
        for step in steps(job) {
            if step["uses"]
                .as_str()
                .is_some_and(|uses| uses.starts_with("Swatinem/rust-cache@"))
            {
                assert_eq!(
                    step["with"]["save-if"].as_str(),
                    Some("${{ github.ref == 'refs/heads/main' }}"),
                    "{job_name}"
                );
                keys.push(step["with"]["shared-key"].as_str().unwrap().to_owned());
            }
        }
    }
    let mut unique = keys.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), keys.len(), "shared cache keys {keys:?}");
    assert!(keys.len() >= 3);
    // Release builds start cold so no cache can reach a published binary.
    let release_text = source("release.yml");
    assert!(!release_text.contains("rust-cache@") && !release_text.contains("actions/cache@"));

    // Every job that runs tests uploads its log when it fails.
    for (workflow, job_name) in [(&ci, "test"), (&ci, "macos"), (&release, "build")] {
        let job = &workflow["jobs"][job_name];
        assert!(
            steps(job).iter().any(|step| {
                step["if"].as_str() == Some("${{ failure() }}")
                    && step["uses"]
                        .as_str()
                        .is_some_and(|uses| uses.starts_with("actions/upload-artifact@"))
            }),
            "{job_name} uploads no failure artifact"
        );
    }
}

#[test]
fn ci_workflow_matrices_are_native_and_tools_are_pinned() {
    let ci = yaml("ci.yml");
    let all: String = jobs(&ci)
        .iter()
        .map(|(_, job)| runs(job))
        .collect::<Vec<_>>()
        .join("\n");
    for needle in [
        "cargo fmt --all --check",
        "cargo clippy --locked --all-targets -- -D warnings",
        "node scripts/test-all.mjs",
        "cargo test --locked --lib",
        "shellcheck scripts/install.sh scripts/package-release.sh",
    ] {
        assert!(all.contains(needle), "ci.yml lacks {needle}");
    }
    // Native macOS is reported on both CPUs on every run, never skipped.
    let macos = &ci["jobs"]["macos"];
    assert!(macos["if"].is_badvalue());
    let runners: Vec<_> = macos["strategy"]["matrix"]["include"]
        .as_vec()
        .unwrap()
        .iter()
        .map(|entry| entry["runner"].as_str().unwrap())
        .collect();
    assert_eq!(runners, ["macos-15", "macos-15-intel"]);

    let release = yaml("release.yml");
    let build = &release["jobs"]["build"];
    let matrix: Vec<_> = build["strategy"]["matrix"]["include"]
        .as_vec()
        .unwrap()
        .iter()
        .map(|entry| {
            (
                entry["target"].as_str().unwrap(),
                entry["runner"].as_str().unwrap(),
            )
        })
        .collect();
    // Each target builds and runs on a runner of its own CPU.
    assert_eq!(matrix, [(X86, "ubuntu-24.04"), (ARM, "ubuntu-24.04-arm")]);

    // Toolchain and build tools carry exact versions.
    for workflow in [&ci, &release] {
        for (job_name, job) in jobs(workflow) {
            for step in steps(job) {
                let uses = step["uses"].as_str().unwrap_or_default();
                if uses.starts_with("dtolnay/rust-toolchain@") {
                    assert_eq!(
                        step["with"]["toolchain"].as_str(),
                        Some("1.93.1"),
                        "{job_name}"
                    );
                }
                if uses.starts_with("mlugg/setup-zig@") {
                    assert_eq!(step["with"]["version"].as_str(), Some("0.13.0"));
                }
                if uses.starts_with("taiki-e/install-action@") {
                    let tool = step["with"]["tool"].as_str().unwrap();
                    let (_, version) = tool.split_once('@').expect("unversioned tool");
                    assert!(version.split('.').count() == 3, "{tool}");
                }
            }
        }
    }
}

#[test]
fn ci_workflow_release_gates_run_in_order_before_promotion() {
    let release = yaml("release.yml");
    let build = runs(&release["jobs"]["build"]);
    let assemble = runs(&release["jobs"]["assemble"]);
    let publish = runs(&release["jobs"]["publish"]);
    let in_order = |text: &str, needles: &[&str]| {
        let positions: Vec<_> = needles
            .iter()
            .map(|needle| {
                text.find(needle)
                    .unwrap_or_else(|| panic!("lacks {needle}"))
            })
            .collect();
        assert!(
            positions.windows(2).all(|pair| pair[0] < pair[1]),
            "{needles:?} out of order"
        );
    };
    in_order(
        &build,
        &[
            "does not match Cargo.toml version",
            "Cargo.lock ghtui version",
            "cargo zigbuild --locked --release",
            "--version)\" = \"ghtui ${RELEASE_TAG#v}\"",
            "scripts/package-release.sh archive",
            "GHTUI_E2E_RELEASE_DIST=\"$check\" cargo test --locked --test tui_e2e v1_release",
            "no installed-binary PTY journey ran",
            "sha256sum ghtui-*.tar.gz) > \"tested/",
            "actions/upload-artifact@",
        ],
    );
    in_order(
        &assemble,
        &[
            "missing archive for",
            "sha256sum -c ../tested/*.sha256",
            "scripts/package-release.sh finish",
            "sha256sum -c SHA256SUMS",
            "release-version.txt)\" = \"$RELEASE_TAG\"",
            "bash dist/install.sh --release-base \"file://",
        ],
    );
    in_order(
        &publish,
        &[
            "sha256sum -c SHA256SUMS",
            "actions/attest-build-provenance@",
            "gh attestation verify",
            "--verify-tag --draft",
            "gh release download",
            "cmp --",
            "--draft=false --latest",
        ],
    );
    assert!(publish.contains("--draft=false --prerelease"));
    let needs = |job: &str| -> Vec<String> {
        match &release["jobs"][job]["needs"] {
            Yaml::String(one) => vec![one.clone()],
            Yaml::Array(many) => many
                .iter()
                .map(|n| n.as_str().unwrap().to_owned())
                .collect(),
            other => panic!("{job} needs {other:?}"),
        }
    };
    assert_eq!(needs("assemble"), ["build"]);
    assert_eq!(needs("publish"), ["build", "assemble"]);
    assert_eq!(
        release["jobs"]["build"]["strategy"]["fail-fast"].as_bool(),
        Some(true)
    );
}

/// Every `run:` script is valid Bash once `${{ … }}` expressions are
/// replaced by a placeholder word.
#[test]
fn ci_workflow_every_run_script_parses_as_bash() {
    let mut checked = 0;
    for name in WORKFLOWS {
        for (job_name, job) in jobs(&yaml(name)) {
            for step in steps(job) {
                let Some(run) = step["run"].as_str() else {
                    continue;
                };
                let mut script = String::new();
                let mut rest = run;
                while let Some(start) = rest.find("${{") {
                    script.push_str(&rest[..start]);
                    let end = rest[start..].find("}}").expect("unterminated expression");
                    script.push_str("EXPR");
                    rest = &rest[start + end + 2..];
                }
                script.push_str(rest);
                let output = Command::new("bash")
                    .args(["-n", "-c", &script])
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{name}/{job_name}: {}\n{script}",
                    stderr(&output)
                );
                checked += 1;
            }
        }
    }
    assert!(checked >= 20, "only {checked} run scripts");
}

fn release_step(job: &str, name: &str) -> String {
    let release = yaml("release.yml");
    step_named(&release["jobs"][job], name)["run"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[test]
fn ci_workflow_version_guard_refuses_tag_cargo_or_lock_disagreement() {
    let script = release_step("build", "Check the tag matches Cargo.toml and Cargo.lock");
    let version = fs::read_to_string(Path::new(ROOT).join("Cargo.toml"))
        .unwrap()
        .lines()
        .find_map(|line| line.strip_prefix("version = \""))
        .unwrap()
        .trim_end_matches('"')
        .to_owned();
    let tag = format!("v{version}");
    let ok = bash(&script, Path::new(ROOT), &[("RELEASE_TAG", &tag)]);
    assert!(ok.status.success(), "{}", stderr(&ok));

    let wrong = bash(&script, Path::new(ROOT), &[("RELEASE_TAG", "v99.0.0")]);
    assert!(!wrong.status.success());
    assert!(stderr(&wrong).contains("tag v99.0.0 does not match Cargo.toml version"));

    // A stale Cargo.lock is refused even when the tag matches Cargo.toml.
    let temp = tempfile::tempdir().unwrap();
    fs::copy(
        Path::new(ROOT).join("Cargo.toml"),
        temp.path().join("Cargo.toml"),
    )
    .unwrap();
    let lock = fs::read_to_string(Path::new(ROOT).join("Cargo.lock")).unwrap();
    let stale = lock.replacen(
        &format!("name = \"ghtui\"\nversion = \"{version}\""),
        "name = \"ghtui\"\nversion = \"0.0.0-stale\"",
        1,
    );
    assert_ne!(stale, lock, "Cargo.lock has no ghtui package entry");
    fs::write(temp.path().join("Cargo.lock"), stale).unwrap();
    let stale_run = bash(&script, temp.path(), &[("RELEASE_TAG", &tag)]);
    assert!(!stale_run.status.success());
    assert!(stderr(&stale_run).contains("Cargo.lock ghtui version 0.0.0-stale does not match"));
}

struct AssembleLab {
    _temp: tempfile::TempDir,
    root: PathBuf,
}
impl AssembleLab {
    /// A workspace shaped like the assemble job after download-artifact.
    fn new(targets: &[&str]) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().to_path_buf();
        fs::create_dir_all(root.join("scripts")).unwrap();
        for script in ["install.sh", "package-release.sh"] {
            let to = root.join("scripts").join(script);
            fs::copy(Path::new(ROOT).join("scripts").join(script), &to).unwrap();
            fs::set_permissions(&to, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::create_dir_all(root.join("dist")).unwrap();
        fs::create_dir_all(root.join("tested")).unwrap();
        for target in targets {
            let archive = format!("ghtui-v1.2.3-{target}.tar.gz");
            fs::write(
                root.join("dist").join(&archive),
                format!("tested bytes {target}"),
            )
            .unwrap();
            let digest = bash(&format!("cd dist && sha256sum {archive}"), &root, &[]);
            assert!(digest.status.success());
            fs::write(
                root.join("tested").join(format!("{target}.sha256")),
                digest.stdout,
            )
            .unwrap();
        }
        Self { _temp: temp, root }
    }
    fn assemble(&self) -> Output {
        let script = release_step(
            "assemble",
            "Write install.sh, release-version.txt and SHA256SUMS",
        );
        bash(&script, &self.root, &[("RELEASE_TAG", "v1.2.3")])
    }
}

#[test]
fn ci_workflow_assemble_refuses_partial_matrix_and_untested_bytes() {
    // Both targets with their tested digests assemble every asset.
    let lab = AssembleLab::new(&[X86, ARM]);
    let output = lab.assemble();
    assert!(output.status.success(), "{}", stderr(&output));
    let sums = fs::read_to_string(lab.root.join("dist/SHA256SUMS")).unwrap();
    for asset in [
        format!("ghtui-v1.2.3-{X86}.tar.gz"),
        format!("ghtui-v1.2.3-{ARM}.tar.gz"),
        "install.sh".to_owned(),
    ] {
        assert!(
            sums.contains(&format!("  {asset}\n")),
            "{asset} not in {sums}"
        );
    }
    assert_eq!(
        fs::read_to_string(lab.root.join("dist/release-version.txt")).unwrap(),
        "v1.2.3\n"
    );
    assert_eq!(
        fs::read(lab.root.join("dist/install.sh")).unwrap(),
        fs::read(Path::new(ROOT).join("scripts/install.sh")).unwrap()
    );

    // A missing target is refused before anything is assembled.
    let partial = AssembleLab::new(&[X86]);
    let output = partial.assemble();
    assert!(!output.status.success());
    assert!(stderr(&output).contains(&format!("missing archive for {ARM}")));
    assert!(!partial.root.join("dist/SHA256SUMS").exists());

    // Bytes that differ from the ones the PTY journeys tested are refused.
    let tampered = AssembleLab::new(&[X86, ARM]);
    fs::write(
        tampered
            .root
            .join(format!("dist/ghtui-v1.2.3-{ARM}.tar.gz")),
        "different bytes",
    )
    .unwrap();
    let output = tampered.assemble();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("FAILED"));
    assert!(!tampered.root.join("dist/SHA256SUMS").exists());
}

const TMPDIR_STEP: &str = "Keep test temporary files outside the checkout";

fn step_index(job: &Yaml, needle: &str) -> usize {
    steps(job)
        .iter()
        .position(|step| {
            step["run"].as_str().is_some_and(|run| run.contains(needle))
                || step["name"].as_str() == Some(needle)
        })
        .unwrap_or_else(|| panic!("no step runs or is named {needle}"))
}

fn upload_paths(job: &Yaml) -> String {
    steps(job)
        .iter()
        .filter(|step| {
            step["if"].as_str() == Some("${{ failure() }}")
                && step["uses"]
                    .as_str()
                    .is_some_and(|uses| uses.starts_with("actions/upload-artifact@"))
        })
        .filter_map(|step| step["with"]["path"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// AC1: the Linux suite and the macOS job run with TMPDIR outside the
/// checkout and the test-only Git maintenance policy, run the coverage audits
/// on every run, refuse zero selected or ignored tests, and upload the PTY
/// screen artifacts when they fail.
#[test]
fn ci_workflow_test_jobs_audit_count_tests_and_keep_pty_artifacts() {
    let ci = yaml("ci.yml");
    let test = &ci["jobs"]["test"];
    let macos = &ci["jobs"]["macos"];

    // TMPDIR is set before any test command in both jobs.
    for (job_name, job, first_test) in [
        ("test", test, "ci-test-counts.mjs"),
        ("macos", macos, "cargo build --locked --all-targets"),
    ] {
        let tmp = step_index(job, TMPDIR_STEP);
        assert!(
            step_named(job, TMPDIR_STEP)["if"].is_badvalue(),
            "{job_name}: TMPDIR step is conditional"
        );
        assert!(
            tmp < step_index(job, first_test),
            "{job_name}: TMPDIR set too late"
        );
        assert!(
            step_named(job, TMPDIR_STEP)["run"]
                .as_str()
                .unwrap()
                .contains(r#"echo "TMPDIR=$tmp" >> "$GITHUB_ENV""#),
            "{job_name}"
        );
    }

    // Both audits run unconditionally, before the full suite, and each must
    // select tests in every binary it runs.
    for (name, command) in [
        (
            "Coverage inventory audit",
            "node scripts/ci-test-counts.mjs --each -- cargo test --locked --test coverage_inventory_v64",
        ),
        (
            "Theme and identity audit",
            "node scripts/ci-test-counts.mjs --each -- cargo test --locked --lib audit_v64_",
        ),
    ] {
        let step = step_named(test, name);
        assert!(step["if"].is_badvalue(), "{name} is conditional");
        assert!(
            step["run"].as_str().unwrap().starts_with(command),
            "{name}: {:?}",
            step["run"]
        );
        assert!(step_index(test, name) < step_index(test, "Full suite"));
    }
    // The audit filters still select real, non-ignored tests.
    let theme_audit =
        fs::read_to_string(Path::new(ROOT).join("src/theme_audit_v64_tests.rs")).unwrap();
    assert!(theme_audit.matches("#[test]\nfn audit_v64_").count() >= 3);
    let inventory =
        fs::read_to_string(Path::new(ROOT).join("tests/coverage_inventory_v64.rs")).unwrap();
    assert!(inventory.matches("#[test]\nfn coverage_v64_").count() >= 2);

    // The full suite goes through test-all.mjs (TMPDIR + Git policy) and its
    // log must show a floor of passed tests with none ignored.
    let full = step_named(test, "Full suite");
    assert!(full["run"]
        .as_str()
        .unwrap()
        .starts_with("node scripts/test-all.mjs 2>&1 | tee \"$RUNNER_TEMP/test-all.log\""));
    let count = step_named(test, "Full suite selected test count");
    assert_eq!(count["if"].as_str(), full["if"].as_str());
    let count_run = count["run"].as_str().unwrap();
    assert!(count_run.contains("--log \"$RUNNER_TEMP/test-all.log\""));
    let floor: u64 = count_run
        .split_once("--min-passed ")
        .unwrap()
        .1
        .split_whitespace()
        .next()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        floor >= 6000,
        "floor {floor} is too low to catch dropped targets"
    );
    assert!(step_index(test, "Full suite") < step_index(test, "Full suite selected test count"));
    assert!(step_named(test, "Filtered PTY journeys")["run"]
        .as_str()
        .unwrap()
        .starts_with(
            "node scripts/ci-test-counts.mjs -- cargo test --locked --test tui_e2e -- \"$FILTER\""
        ));
    assert!(step_named(macos, "Library tests")["run"]
        .as_str()
        .unwrap()
        .starts_with("node scripts/ci-test-counts.mjs -- cargo test --locked --lib"));
    assert!(step_named(test, "Node script tests")["run"]
        .as_str()
        .unwrap()
        .contains("tests/ci_test_counts.test.mjs"));

    // Every cargo test in ci.yml goes through a wrapper that applies the
    // test-only Git environment, instead of a hand-copied env block.
    for (job_name, job) in jobs(&ci) {
        for step in steps(job) {
            let Some(run) = step["run"].as_str() else {
                continue;
            };
            for line in run.lines().filter(|line| line.contains("cargo test")) {
                assert!(
                    line.trim_start()
                        .starts_with("node scripts/ci-test-counts.mjs"),
                    "{job_name}: `{line}` bypasses ci-test-counts.mjs"
                );
            }
        }
    }

    // Failure uploads keep the PTY screens and transcripts of failed journeys.
    let release = yaml("release.yml");
    for (job_name, job) in [
        ("test", test),
        ("macos", macos),
        ("release build", &release["jobs"]["build"]),
    ] {
        let paths = upload_paths(job);
        assert!(
            paths.contains("target/tui-artifacts/"),
            "{job_name}: {paths}"
        );
    }
    let paths = upload_paths(test);
    assert!(paths.contains("${{ runner.temp }}/test-all.log"));
    assert!(paths.contains("${{ runner.temp }}/audits.log"));

    // No required test is ignored anywhere in the tree.
    let mut files = Vec::new();
    for dir in ["src", "tests"] {
        let mut stack = vec![Path::new(ROOT).join(dir)];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|ext| ext == "rs") {
                    files.push(path);
                }
            }
        }
    }
    assert!(files.len() > 50);
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for line in text.lines() {
            assert!(
                !line.trim_start().starts_with("#[ignore"),
                "{}: {line}",
                file.display()
            );
        }
    }
}

/// The TMPDIR step refuses a temporary directory inside the checkout and
/// otherwise exports one outside it (run with real Bash).
#[test]
fn ci_workflow_tmpdir_step_refuses_a_directory_inside_the_checkout() {
    let script = step_named(&yaml("ci.yml")["jobs"]["test"], TMPDIR_STEP)["run"]
        .as_str()
        .unwrap()
        .to_owned();
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().join("workspace");
    let outside = temp.path().join("runner-temp");
    fs::create_dir_all(&workspace).unwrap();
    fs::create_dir_all(&outside).unwrap();
    let env_file = temp.path().join("github-env");

    let ok = bash(
        &script,
        &workspace,
        &[
            ("RUNNER_TEMP", outside.to_str().unwrap()),
            ("GITHUB_WORKSPACE", workspace.to_str().unwrap()),
            ("GITHUB_ENV", env_file.to_str().unwrap()),
        ],
    );
    assert!(ok.status.success(), "{}", stderr(&ok));
    assert_eq!(
        fs::read_to_string(&env_file).unwrap(),
        format!("TMPDIR={}/ghtui-tmp\n", outside.display())
    );
    assert!(outside.join("ghtui-tmp").is_dir());

    fs::remove_file(&env_file).unwrap();
    let inside = workspace.join("target");
    let refused = bash(
        &script,
        &workspace,
        &[
            ("RUNNER_TEMP", inside.to_str().unwrap()),
            ("GITHUB_WORKSPACE", workspace.to_str().unwrap()),
            ("GITHUB_ENV", env_file.to_str().unwrap()),
        ],
    );
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stdout).contains("is inside the checkout"));
    assert!(!env_file.exists());
}

/// The zero-selected-tests guard the workflow relies on refuses a log in
/// which nothing passed and one with an ignored test, and accepts a real one.
#[test]
fn ci_workflow_test_count_guard_refuses_zero_selected_and_ignored_tests() {
    let temp = tempfile::tempdir().unwrap();
    let script = Path::new(ROOT).join("scripts/ci-test-counts.mjs");
    let check = |log: &str, args: &[&str]| {
        let path = temp.path().join("cargo.log");
        fs::write(&path, log).unwrap();
        Command::new("node")
            .arg(&script)
            .args(args)
            .arg("--log")
            .arg(&path)
            .output()
            .unwrap()
    };
    let line = |passed: u32, ignored: u32, filtered: u32| {
        format!("test result: ok. {passed} passed; 0 failed; {ignored} ignored; 0 measured; {filtered} filtered out; finished in 0.10s\n")
    };
    let zero = check(&line(0, 0, 37), &[]);
    assert!(!zero.status.success());
    assert!(stderr(&zero).contains("only 0 test(s) passed"));
    let nothing = check("error: could not compile `ghtui`\n", &[]);
    assert!(!nothing.status.success());
    let ignored = check(&line(5, 1, 0), &[]);
    assert!(!ignored.status.success());
    assert!(stderr(&ignored).contains("ignored"));
    let empty_binary = check(&format!("{}{}", line(3, 0, 0), line(0, 0, 9)), &["--each"]);
    assert!(!empty_binary.status.success());
    let floor = check(&line(5999, 0, 0), &["--min-passed", "6000"]);
    assert!(!floor.status.success());
    let ok = check(
        &format!("{}{}", line(6000, 0, 0), line(0, 0, 0)),
        &["--min-passed", "6000"],
    );
    assert!(ok.status.success(), "{}", stderr(&ok));
    assert!(String::from_utf8_lossy(&ok.stdout).contains("6000 passed"));
}
