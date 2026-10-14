//! UNREGISTERED real installed-Git self-contracts for the transparent recorder.
//! Intended private child of git_contracts.rs; no fake provider or TUI substitute.
use super::{git_support, raw_git_argv_v1 as raw};
use std::{
    collections::BTreeSet,
    ffi::{OsStr, OsString},
    fs,
    os::unix::ffi::OsStrExt,
    path::Path,
    process::{Command, Output},
};

fn run(program: &Path, repo: &Path, args: &[OsString], trace: &Path) -> Output {
    Command::new(program)
        .current_dir(repo)
        .args(args)
        .env("GIT_TRACE2_EVENT", trace)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .output()
        .unwrap()
}
fn state(fixture: &git_support::Fixture) -> Vec<Vec<u8>> {
    vec![
        git_support::git(&fixture.root, ["rev-parse", "HEAD"]).stdout,
        git_support::git(&fixture.root, ["status", "--porcelain=v2", "-z"]).stdout,
        fs::read(fixture.root.join(".git/index")).unwrap(),
        fs::read(fixture.root.join(".git/config")).unwrap(),
    ]
}
fn observer(fixture: &git_support::Fixture) -> (std::path::PathBuf, std::path::PathBuf) {
    let root = fixture.directory.path().join("quoted' recorder\nroot");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("bin")).unwrap();
    let program = raw::install(&root);
    (root, program)
}

#[test]
fn original_tree_jump_raw_git_argv_v1_real_git_distinct_fe_ff_case_newline_exact_objects_and_nul_argv(
) {
    let fixture = git_support::Fixture::linear();
    let paths: [&[u8]; 5] = [
        b"same-\xfe",
        b"same-\xff",
        b"Case.rs",
        b"case.rs",
        b"line\nname",
    ];
    let mut expected = Vec::new();
    let mut entries = Vec::new();
    for (index, path) in paths.iter().enumerate() {
        let body = format!("Independently distinct saved body {index}\n").into_bytes();
        let oid = String::from_utf8(
            git_support::git_input(&fixture.root, ["hash-object", "-w", "--stdin"], &body).stdout,
        )
        .unwrap()
        .trim()
        .to_owned();
        entries.extend_from_slice(format!("100644 blob {oid}\t").as_bytes());
        entries.extend_from_slice(path);
        entries.push(0);
        expected.push((body, oid));
    }
    assert_eq!(
        expected
            .iter()
            .map(|(_, oid)| oid)
            .collect::<BTreeSet<_>>()
            .len(),
        5
    );
    let tree =
        String::from_utf8(git_support::git_input(&fixture.root, ["mktree", "-z"], &entries).stdout)
            .unwrap();
    let chosen = String::from_utf8(
        git_support::git(
            &fixture.root,
            [
                "commit-tree",
                tree.trim(),
                "-p",
                &fixture.head(),
                "-m",
                "Byte exact object fixture",
            ],
        )
        .stdout,
    )
    .unwrap();
    let chosen = chosen.trim();
    let before = state(&fixture);
    let (root, program) = observer(&fixture);
    let trace = root.join("trace.jsonl");
    for (path, (body, oid)) in paths.iter().zip(&expected) {
        let args = [
            OsString::from("ls-tree"),
            "-z".into(),
            "--full-tree".into(),
            chosen.into(),
            "--".into(),
            OsStr::from_bytes(path).to_owned(),
        ];
        let actual = run(&program, &fixture.root, &args, &trace);
        let direct = git_support::git(&fixture.root, &args);
        assert!(actual.status.success());
        assert_eq!(actual.stdout, direct.stdout);
        assert_eq!(actual.stderr, direct.stderr);
        let args = [OsString::from("cat-file"), "blob".into(), oid.into()];
        let actual = run(&program, &fixture.root, &args, &trace);
        assert!(actual.status.success());
        assert_eq!(&actual.stdout, body);
        assert!(actual.stderr.is_empty());
        assert_eq!(
            raw::original_blob(&fixture.root, chosen, path),
            (oid.clone(), body.clone())
        );
        raw::exact_read(
            &raw::records(&root.join("raw-git-argv-v1")),
            chosen,
            path,
            oid,
        );
    }
    let records = raw::records(&root.join("raw-git-argv-v1"));
    assert_eq!(records.len(), 10);
    assert_eq!(
        records
            .iter()
            .map(|record| record.pid)
            .collect::<BTreeSet<_>>()
            .len(),
        10
    );
    assert_eq!(state(&fixture), before);
    let telemetry = raw::ascii_trace2(&trace);
    assert_eq!(
        telemetry
            .iter()
            .filter(|event| event["event"] == "start")
            .count(),
        10
    );
    assert!(telemetry.iter().any(|event| event["name"] == "ls-tree"));
    assert!(telemetry.iter().any(|event| event["name"] == "cat-file"));
}

#[test]
fn original_tree_jump_raw_git_argv_v1_preserves_empty_raw_args_and_real_git_failure_status_stdout_stderr(
) {
    let fixture = git_support::Fixture::linear();
    let before = state(&fixture);
    let (root, program) = observer(&fixture);
    let trace = root.join("trace.jsonl");
    let args = [
        OsString::from("rev-parse"),
        "--sq-quote".into(),
        "".into(),
        OsStr::from_bytes(b"same-\xfe").to_owned(),
        "quote' and\nline".into(),
    ];
    let actual = run(&program, &fixture.root, &args, &trace);
    let direct = git_support::git(&fixture.root, &args);
    assert!(actual.status.success());
    assert_eq!(actual.stdout, direct.stdout);
    assert_eq!(actual.stderr, direct.stderr);
    let args = [
        OsString::from("cat-file"),
        "blob".into(),
        "missing-byte-oracle-object".into(),
    ];
    let actual = run(&program, &fixture.root, &args, &trace);
    let direct = git_support::try_git(&fixture.root, &args);
    assert!(!actual.status.success());
    assert_eq!(actual.status.code(), direct.status.code());
    assert_eq!(actual.stdout, direct.stdout);
    assert_eq!(actual.stderr, direct.stderr);
    let records = raw::records(&root.join("raw-git-argv-v1"));
    assert_eq!(records.len(), 2);
    assert!(records.iter().any(|record| record.argv
        == [
            b"rev-parse".to_vec(),
            b"--sq-quote".to_vec(),
            Vec::new(),
            b"same-\xfe".to_vec(),
            b"quote' and\nline".to_vec()
        ]));
    assert_eq!(state(&fixture), before);
}

#[test]
fn original_tree_jump_raw_git_argv_v1_incomplete_raw_record_and_malformed_trace_cannot_be_accepted_as_evidence(
) {
    let fixture = git_support::Fixture::linear();
    let (root, _) = observer(&fixture);
    let records = root.join("raw-git-argv-v1");
    fs::write(records.join("123.argv"), b"ls-tree\0same-\xff").unwrap();
    assert!(std::panic::catch_unwind(|| raw::records(&records)).is_err());
    let bad = root.join("bad.jsonl");
    fs::write(
        &bad,
        b"{\"event\":\"start\",\"argv\":[\"same-\xff\"]}\nnot JSON\n",
    )
    .unwrap();
    assert!(std::panic::catch_unwind(|| raw::ascii_trace2(&bad)).is_err());
    fs::write(
        &bad,
        b"{\"event\":\"start\",\"argv\":[\"git\",\"ls-tree\",\"same-\xff\"]}\n",
    )
    .unwrap();
    let telemetry = raw::ascii_trace2(&bad);
    assert_eq!(telemetry.len(), 1);
    assert_eq!(telemetry[0]["argv"][1], "ls-tree");
    assert_eq!(telemetry[0]["argv"][2], "<nonASCII telemetry argument>");
}
