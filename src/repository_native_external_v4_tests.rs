use super::*;
use crate::{
    git::{Comparison, OriginalComparisonSideV1},
    local_commit_actor_origin_v1_test_support::{git, Real},
    original_diff_sides_v1_test_support as f,
    providers::owned_pty_viewer_v4::tests::{fixture, Fixture},
};
use std::{fs, process::Command};
#[path = "repository_native_external_controller_v4_tests.rs"]
mod controller;

struct Tools {
    _directory: tempfile::TempDir,
    renderer: PathBuf,
    pager: Fixture,
    evidence: PathBuf,
}
fn tools(tool: ExternalToolV4, body: &str) -> Tools {
    let node = Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = std::str::from_utf8(&node.stdout).unwrap().trim();
    assert!(Path::new(node).is_absolute());
    let directory = tempfile::tempdir().unwrap();
    let renderer = directory.path().join(tool.program());
    let evidence = directory.path().join("actual-renderer.json");
    let version = if tool == ExternalToolV4::Delta {
        "delta 0.18.2"
    } else {
        "Difftastic 0.67.0"
    };
    let expected = if tool == ExternalToolV4::Delta {
        vec![
            "--no-gitconfig",
            "--paging=never",
            "--detect-dark-light=never",
            "--width=80",
            "--line-numbers",
        ]
    } else {
        vec![
            "--display=side-by-side-show-both",
            "--color=always",
            "--width=80",
            "--context=3",
            "--byte-limit=1048576",
            "--graph-limit=250000",
            "--parse-error-limit=0",
            "--strip-cr=off",
            "--",
        ]
    };
    let code=format!("#!{node}\n'use strict';const fs=require('node:fs');const args=process.argv.slice(2);if(args.length===1&&args[0]==='--version'){{process.stdout.write({version});process.exit(0)}}const expected={expected};if(JSON.stringify(args.slice(0,expected.length))!==JSON.stringify(expected))process.exit(71);const pair={pair};if(args.length!==expected.length+(pair?2:0))process.exit(72);for(const key of ['GH_TOKEN','GITHUB_TOKEN','DELTA_FEATURES','BAT_PAGER','DFT_DISPLAY','LESSOPEN','LD_PRELOAD','DYLD_INSERT_LIBRARIES'])if(process.env[key]!==undefined)process.exit(73);if(process.env.SHELL!=='/bin/false'||process.env.PAGER!=='cat')process.exit(74);let chunks=[];process.stdin.on('data',part=>chunks.push(part));process.stdin.on('end',()=>{{let record={{args,cwd:process.cwd(),input:Buffer.concat(chunks).toString('hex'),pid:process.pid}};if(pair){{record.old=fs.readFileSync(args[expected.length]).toString('hex');record.new=fs.readFileSync(args[expected.length+1]).toString('hex');record.oldMode=fs.statSync(args[expected.length]).mode&511;record.newMode=fs.statSync(args[expected.length+1]).mode&511;}}fs.writeFileSync({evidence},JSON.stringify(record));{body}}});\n",
        version=serde_json::to_string(&format!("{version}\n")).unwrap(),expected=serde_json::to_string(&expected).unwrap(),pair=tool==ExternalToolV4::Difftastic,evidence=serde_json::to_string(&evidence).unwrap());
    fs::write(&renderer, code).unwrap();
    fs::set_permissions(&renderer, fs::Permissions::from_mode(0o700)).unwrap();
    Tools {
        _directory: directory,
        renderer,
        pager: fixture(""),
        evidence,
    }
}
fn observed(real: &Real) -> Vec<Vec<u8>> {
    vec![
        fs::read(real.root.join(".git/index")).unwrap(),
        fs::read(real.root.join(".git/config")).unwrap(),
        fs::read(real.root.join("tracked")).unwrap(),
        fs::read(real.root.join("scratch")).unwrap(),
        git(&real.root, &["show-ref", "--head"], None),
        git(
            &real.root,
            &[
                "-c",
                "diff.autoRefreshIndex=false",
                "diff",
                "--binary",
                "--no-ext-diff",
            ],
            None,
        ),
        git(
            &real.root,
            &[
                "-c",
                "diff.autoRefreshIndex=false",
                "diff",
                "--cached",
                "--binary",
                "--no-ext-diff",
            ],
            None,
        ),
    ]
}
fn operation(workspace: &crate::workspace::Workspace) -> LocalGitNavigationReadV1 {
    LocalGitNavigationReadV1::begin(workspace.actor_origin.as_ref().unwrap(), Default::default())
        .unwrap()
}
fn bound<'a>(
    operation: &'a LocalGitNavigationReadV1,
    tools: &Tools,
) -> ExternalRendererBoundsV4<'a, LocalGitNavigationReadV1> {
    let mut bounds = ExternalRendererBoundsV4::local(operation, DisplayPermitV4::launch()).unwrap();
    bounds.programs_override = Some((tools.renderer.clone(), tools.pager.program.clone()));
    bounds
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn native_external_diff_delta_genuine_sha1_sha256_captured_comparison_bytes_not_rows() {
    for format in ["sha1", "sha256"] {
        let (real, a, b, c) = f::fixture(format);
        let workspace = f::workspace(
            &real,
            Comparison::BranchChanges {
                base: a.clone(),
                head: b.clone(),
            },
        );
        let source = f::source(&workspace);
        assert_eq!(source.result().diff.base.as_deref(), Some(c.as_str()));
        assert_ne!(c, a);
        let before = observed(&real);
        let op = operation(&workspace);
        let tools = tools(
            ExternalToolV4::Delta,
            "process.stdout.write('\\x1b[32mactual-rendered\\x1b[0m');",
        );
        let bounds = bound(&op, &tools);
        let prepared = prepare_external_v4(
            &bounds,
            ExternalToolV4::Delta,
            ExternalOriginalInputV4::Patch {
                bytes: &source.result().patch,
                scope: ExternalPatchScopeV4::LocalCapturedComparison,
            },
            80,
            || op.check(),
        )
        .unwrap();
        let record: serde_json::Value =
            serde_json::from_slice(&fs::read(&tools.evidence).unwrap()).unwrap();
        assert_eq!(record["input"], hex(&source.result().patch));
        assert!(!Path::new(record["cwd"].as_str().unwrap()).exists());
        assert_eq!(
            op.accounting().unwrap().children_started(),
            0,
            "foreign renderer must NOT enter measured installed-Git pool"
        );
        assert!(prepared
            .scope
            .contains("WHOLE ORIGINAL CAPTURED COMPARISON"));
        assert_eq!(prepared.tool, ExternalToolV4::Delta);
        assert_eq!(prepared.width, 80);
        assert!(prepared.capture.bytes() > 0);
        assert_eq!(observed(&real), before);
        op.seal().unwrap();
    }
}
#[test]
fn native_external_diff_structural_genuine_old_c_new_b_file_bytes_modes_and_no_exit_code() {
    let (real, a, b, c) = f::fixture("sha256");
    let workspace = f::workspace(
        &real,
        Comparison::BranchChanges {
            base: a.clone(),
            head: b.clone(),
        },
    );
    let picker = f::state(&workspace);
    let old = f::open(
        &workspace,
        &picker,
        b"shared",
        OriginalComparisonSideV1::Old,
    );
    let new = f::open(
        &workspace,
        &picker,
        b"shared",
        OriginalComparisonSideV1::New,
    );
    assert_eq!(old.sha(), c);
    assert_eq!(new.sha(), b);
    assert_ne!(old.sha(), a);
    let before = observed(&real);
    let op = operation(&workspace);
    let tools = tools(
        ExternalToolV4::Difftastic,
        "process.stdout.write('structural-observed');",
    );
    let bounds = bound(&op, &tools);
    let prepared = prepare_external_v4(
        &bounds,
        ExternalToolV4::Difftastic,
        ExternalOriginalInputV4::TextSides {
            old: &old.preview().content,
            new: &new.preview().content,
            old_absent: false,
            new_absent: false,
            suffix: "rs",
            new_material_bytes: old.preview().content.capacity() + new.preview().content.capacity(),
        },
        80,
        || op.check(),
    )
    .unwrap();
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(&tools.evidence).unwrap()).unwrap();
    assert_eq!(record["old"], hex(b"COMMON old bytes\n"));
    assert_eq!(record["new"], hex(b"TARGET new bytes\n"));
    assert_eq!(record["input"], "");
    assert_eq!(record["oldMode"], 256);
    assert_eq!(record["newMode"], 256);
    assert!(!record["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "--exit-code"));
    assert!(!Path::new(record["cwd"].as_str().unwrap()).exists());
    assert!(prepared.scope.contains("selected genuine full sides"));
    assert_eq!(observed(&real), before);
}
#[test]
fn native_external_diff_renderer_exit_two_generic_error_contains_no_private_body_or_stderr() {
    let (real, a, b, _) = f::fixture("sha1");
    let workspace = f::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let source = f::source(&workspace);
    let before = observed(&real);
    let op = operation(&workspace);
    let tools=tools(ExternalToolV4::Delta,"process.stderr.write('PRIVATE-ERROR-BODY');process.stdout.write('PRIVATE-PARTIAL');process.exit(2);");
    let bounds = bound(&op, &tools);
    let error = prepare_external_v4(
        &bounds,
        ExternalToolV4::Delta,
        ExternalOriginalInputV4::Patch {
            bytes: &source.result().patch,
            scope: ExternalPatchScopeV4::LocalCapturedComparison,
        },
        80,
        || op.check(),
    )
    .err()
    .unwrap();
    assert!(error.contains("exited2"));
    assert!(!error.contains("PRIVATE"));
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(&tools.evidence).unwrap()).unwrap();
    assert!(!Path::new(record["cwd"].as_str().unwrap()).exists());
    assert_eq!(observed(&real), before);
}
#[test]
fn native_external_diff_renderer_overflow_is_not_partial_success_or_extra_source_access() {
    let (real, a, b, _) = f::fixture("sha1");
    let workspace = f::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let source = f::source(&workspace);
    let op = operation(&workspace);
    let tools = tools(
        ExternalToolV4::Delta,
        "process.stdout.write(Buffer.alloc(600000,65));",
    );
    let bounds = bound(&op, &tools);
    let error = prepare_external_v4(
        &bounds,
        ExternalToolV4::Delta,
        ExternalOriginalInputV4::Patch {
            bytes: &source.result().patch,
            scope: ExternalPatchScopeV4::LocalCapturedComparison,
        },
        80,
        || op.check(),
    )
    .err()
    .unwrap();
    assert!(error.contains("512KiB"), "{error}");
    assert!(error.contains("partial"));
    assert_eq!(op.accounting().unwrap().children_started(), 0);
}
#[test]
fn native_external_diff_original_sealed_cancelled_and_changed_source_refuse_before_tool() {
    let (real, a, b, _) = f::fixture("sha1");
    let workspace = f::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let source = f::source(&workspace);
    let tools = tools(
        ExternalToolV4::Delta,
        "process.stdout.write('not-authorized');",
    );
    let op = operation(&workspace);
    let bounds = bound(&op, &tools);
    op.cancellation().cancel();
    assert!(prepare_external_v4(
        &bounds,
        ExternalToolV4::Delta,
        ExternalOriginalInputV4::Patch {
            bytes: &source.result().patch,
            scope: ExternalPatchScopeV4::LocalCapturedComparison
        },
        80,
        || op.check()
    )
    .is_err());
    assert!(!tools.evidence.exists());
    let op = operation(&workspace);
    let bounds = bound(&op, &tools);
    op.seal().unwrap();
    assert!(prepare_external_v4(
        &bounds,
        ExternalToolV4::Delta,
        ExternalOriginalInputV4::Patch {
            bytes: &source.result().patch,
            scope: ExternalPatchScopeV4::LocalCapturedComparison
        },
        80,
        || op.check()
    )
    .is_err());
    assert!(!tools.evidence.exists());
    let op = operation(&workspace);
    let bounds = bound(&op, &tools);
    let changed = f::workspace(
        &real,
        Comparison::Snapshots {
            base: source.result().diff.head.clone().unwrap(),
            head: source.result().diff.base.clone().unwrap(),
        },
    );
    assert!(
        !source.intent().owns(&changed),
        "independently loaded inverse comparison must be another genuine owner"
    );
    assert!(prepare_external_v4(
        &bounds,
        ExternalToolV4::Delta,
        ExternalOriginalInputV4::Patch {
            bytes: &source.result().patch,
            scope: ExternalPatchScopeV4::LocalCapturedComparison
        },
        80,
        || if source.intent().owns(&changed) {
            op.check()
        } else {
            Err("Original source no longer owns actual loaded comparison".into())
        }
    )
    .is_err());
    assert!(!tools.evidence.exists());
}
#[test]
fn native_external_diff_git_only_window_still_refuses_foreign_renderer_with_zero_children() {
    let (real, a, b, _) = f::fixture("sha1");
    let workspace = f::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let op = operation(&workspace);
    let result = op.window().run(
        CommandSpec::new("delta").args(["--version"]).fresh_read(),
        op.cancellation(),
    );
    assert!(result.is_err());
    assert_eq!(op.accounting().unwrap().children_started(), 0);
}
#[test]
fn native_external_diff_protocol_absence_and_scope_caps_refuse_not_invented_empty_blob() {
    let bytes = vec![b'x'; 1024 * 1024 + 1];
    assert!(ExternalOriginalInputV4::Patch {
        bytes: &bytes,
        scope: ExternalPatchScopeV4::PrSelectedFile
    }
    .validate(ExternalToolV4::Delta)
    .is_err());
    assert!(ExternalOriginalInputV4::Patch {
        bytes: &bytes,
        scope: ExternalPatchScopeV4::LocalCapturedComparison
    }
    .validate(ExternalToolV4::Delta)
    .is_ok());
    assert!(ExternalOriginalInputV4::TextSides {
        old: b"not-empty",
        new: b"new",
        old_absent: true,
        new_absent: false,
        suffix: "rs",
        new_material_bytes: 0
    }
    .validate(ExternalToolV4::Difftastic)
    .is_err());
    assert!(ExternalOriginalInputV4::TextSides {
        old: b"",
        new: b"new",
        old_absent: true,
        new_absent: false,
        suffix: "../raw",
        new_material_bytes: 0
    }
    .validate(ExternalToolV4::Difftastic)
    .is_err());
}
#[test]
fn native_external_diff_missing_program_and_unsupported_pager_never_invoke_renderer_body() {
    let (real, a, b, _) = f::fixture("sha1");
    let workspace = f::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let source = f::source(&workspace);
    let tools = tools(
        ExternalToolV4::Delta,
        "process.stdout.write('not-authorized');",
    );
    let op = operation(&workspace);
    let mut bounds = bound(&op, &tools);
    bounds.programs_override = Some((
        tools._directory.path().join("absent-tool"),
        tools.pager.program.clone(),
    ));
    assert!(prepare_external_v4(
        &bounds,
        ExternalToolV4::Delta,
        ExternalOriginalInputV4::Patch {
            bytes: &source.result().patch,
            scope: ExternalPatchScopeV4::LocalCapturedComparison
        },
        80,
        || op.check()
    )
    .is_err());
    assert!(!tools.evidence.exists());
    assert!(VerifiedPagerV4::from_version(tools.pager.program.clone(), b"less 643\n").is_err());
}
#[test]
fn native_external_diff_pager_actual_held_version_atomic_replacement_cannot_inherit_patched_identity(
) {
    let (real, a, b, _) = f::fixture("sha256");
    let workspace = f::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let source = f::source(&workspace);
    let before = observed(&real);
    let tools = tools(
        ExternalToolV4::Delta,
        "process.stdout.write('MUST-NOT-RENDER');",
    );
    let node = Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .unwrap();
    assert!(node.status.success());
    let node = std::str::from_utf8(&node.stdout).unwrap().trim();
    let entered = tools._directory.path().join("actual-pager-probe.json");
    let release = tools._directory.path().join("release-actual-version");
    let code=format!("#!{node}\n'use strict';const fs=require('node:fs');if(JSON.stringify(process.argv.slice(2))!==JSON.stringify(['--version']))process.exit(91);const entered={};fs.writeFileSync(entered+'.partial',JSON.stringify({{pid:process.pid,args:process.argv.slice(2),cwd:process.cwd()}}));fs.renameSync(entered+'.partial',entered);const timer=setInterval(()=>{{if(fs.existsSync({})){{clearInterval(timer);process.stdout.write('less 710\\n')}}}},2);\n",serde_json::to_string(&entered).unwrap(),serde_json::to_string(&release).unwrap());
    fs::write(&tools.pager.program, code).unwrap();
    fs::set_permissions(&tools.pager.program, fs::Permissions::from_mode(0o700)).unwrap();
    let original = fs::metadata(&tools.pager.program).unwrap();
    let renderer = tools.renderer.clone();
    let pager = tools.pager.program.clone();
    let op = operation(&workspace);
    struct Release(PathBuf);
    impl Drop for Release {
        fn drop(&mut self) {
            let _ = fs::write(&self.0, b"release actual version child");
        }
    }
    let released = Release(release.clone());
    let worker = std::thread::spawn(move || {
        let mut bounds = ExternalRendererBoundsV4::local(&op, DisplayPermitV4::launch()).unwrap();
        bounds.programs_override = Some((renderer, pager));
        let error = prepare_external_v4(
            &bounds,
            ExternalToolV4::Delta,
            ExternalOriginalInputV4::Patch {
                bytes: &source.result().patch,
                scope: ExternalPatchScopeV4::LocalCapturedComparison,
            },
            80,
            || op.check(),
        )
        .err()
        .expect("replacement cannot inherit actual old executable version");
        assert_eq!(op.seal().unwrap().children_started(), 0);
        error
    });
    let deadline = Instant::now() + Duration::from_millis(800);
    while !entered.exists() {
        assert!(
            Instant::now() < deadline,
            "actual held version child must enter within its original1s probe budget"
        );
        std::thread::yield_now();
    }
    let record: serde_json::Value = serde_json::from_slice(&fs::read(&entered).unwrap()).unwrap();
    assert_eq!(record["args"], serde_json::json!(["--version"]));
    let pid = i32::try_from(record["pid"].as_i64().unwrap()).unwrap();
    assert!(
        unsafe { libc::getsid(pid) } > 0,
        "journal PID must be an actual still-held original child"
    );
    let replacement = tools._directory.path().join("replacement-less");
    fs::write(&replacement, format!("#!{node}\nprocess.exit(99);\n")).unwrap();
    fs::set_permissions(&replacement, fs::Permissions::from_mode(0o700)).unwrap();
    fs::rename(&replacement, &tools.pager.program).unwrap();
    let current = fs::metadata(&tools.pager.program).unwrap();
    assert_ne!(
        (original.dev(), original.ino()),
        (current.dev(), current.ino())
    );
    drop(released);
    let error = worker.join().unwrap();
    assert!(error.contains("changed during probe"));
    assert!(
        !tools.evidence.exists(),
        "renderer body and live pager must remain unstarted"
    );
    assert!(!Path::new(record["cwd"].as_str().unwrap()).exists());
    assert_eq!(observed(&real), before);
}
