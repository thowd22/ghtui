use crate::support::{Fixture, Session};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, time::Duration};

const SPARSE: &str = "Sparse checkout: some tracked files are hidden from working files";
const SAVED: &[u8] = b"original sparse file from Git objects\n";
const DIRTY: &[u8] = b"dirty visible file stays local\n";

#[derive(Debug, PartialEq, Eq)]
struct State {
    head: String,
    refs: String,
    status: String,
    index: Vec<u8>,
    config: Vec<u8>,
    worktree_config: Option<Vec<u8>>,
    patterns: Vec<u8>,
    dirty: Vec<u8>,
    untracked: Vec<u8>,
}
fn optional_bytes(path: &Path) -> Option<Vec<u8>> {
    match fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("Cannot inspect {}: {error}", path.display()),
    }
}
fn state(fixture: &Fixture) -> State {
    State {
        head: fixture.git(&["rev-parse", "HEAD"]),
        refs: fixture.git(&["show-ref"]),
        status: fixture.git(&["--no-optional-locks", "status", "--porcelain=v2", "-z"]),
        index: fs::read(fixture.repo().join(".git/index")).unwrap(),
        config: fs::read(fixture.repo().join(".git/config")).unwrap(),
        worktree_config: optional_bytes(&fixture.repo().join(".git/config.worktree")),
        patterns: fs::read(fixture.repo().join(".git/info/sparse-checkout")).unwrap(),
        dirty: fs::read(fixture.repo().join("visible/inside.txt")).unwrap(),
        untracked: fs::read(fixture.repo().join("untracked.txt")).unwrap(),
    }
}
fn activate(session: &mut Session, mouse: bool, label: &str, key: &[u8]) {
    if mouse {
        session.click_visible_action(label);
    } else {
        session.send_key(key);
    }
}
fn complete(session: &mut Session, expected: &[&str]) {
    session.wait_for_cursorless_condition(
        |frame| expected.iter().all(|text| frame.contents().contains(text)),
        "actual completed sparse-repository view",
        Duration::from_secs(8),
    );
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file(
        "visible/inside.txt",
        b"saved visible file\n",
        "Visible saved version",
    );
    fixture.commit_file("outside/sparse-only.txt", SAVED, "Saved sparse file");
    fixture.git(&["sparse-checkout", "set", "--cone", "visible"]);
    assert!(!fixture.repo().join("outside").exists());
    fs::write(fixture.repo().join("visible/inside.txt"), DIRTY).unwrap();
    fs::write(
        fixture.repo().join("untracked.txt"),
        b"untracked sparse fixture\n",
    )
    .unwrap();
    let original = state(&fixture);
    let trace = fixture.root.join("sparse-git-trace2.jsonl");
    let trace_string = trace.to_str().unwrap();
    let mut session = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        120,
        40,
        &[("GIT_TRACE2_EVENT", trace_string)],
    );
    session.assert_focused_view("Repository");
    complete(&mut session, &[SPARSE, "Saved sparse file"]);
    activate(&mut session, mouse, "[w Working]", b"w");
    session.wait_for_screen("dirty visible file stays local");
    session.resize(80, 24);
    activate(&mut session, mouse, "[t Tree]", b"t");
    complete(&mut session, &["Saved file tree", "▸ outside", "▸ visible"]);
    // Home selects the explanatory tree caption; Down selects the first folder.
    if mouse {
        session.click_visible_action("outside");
    } else {
        session.send_key(b"\x1b[H\x1b[B\r");
    }
    complete(&mut session, &["▾ outside", "sparse-only.txt"]);
    if mouse {
        session.click_visible_action("sparse-only.txt");
    } else {
        session.send_key(b"\x1b[B\r");
    }
    complete(&mut session, &["original sparse file from Git objects"]);
    session.capture_completed_terminal_v1("git-provider-sparse-saved-file-80x24");
    assert!(
        !fixture.repo().join("outside").exists(),
        "Saved content must not populate the sparse working tree"
    );
    session.resize(120, 40);
    activate(&mut session, mouse, "[o Overview]", b"o");
    complete(&mut session, &[SPARSE, "Saved sparse file"]);
    session.capture_completed_terminal_v1("git-provider-sparse-overview-120x40");
    assert_eq!(state(&fixture), original);
    assert!(session.inspect_command_journal(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
    assert_eq!(state(&fixture), original);
    assert!(!fixture.repo().join("outside").exists());
    assert!(!fixture.repo().join(".git/index.lock").exists());
    assert!(session.inspect_command_journal(&fixture).is_empty());
    let trace_bytes = fs::read(&trace).unwrap();
    let rows = std::str::from_utf8(&trace_bytes)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect::<Vec<_>>();
    let starts = rows
        .iter()
        .filter(|row| row["event"] == "start")
        .collect::<Vec<_>>();
    assert!(
        starts.len() > 5,
        "Trace must witness actual production Git discovery, status and object reads"
    );
    assert!(starts.iter().any(|row| row["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "ls-tree")));
    assert!(starts.iter().any(|row| row["argv"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "cat-file")));
    for row in &starts {
        for arg in row["argv"].as_array().unwrap() {
            assert!(
                ![
                    "checkout",
                    "switch",
                    "fetch",
                    "sparse-checkout",
                    "reset",
                    "read-tree",
                    "update-index",
                    "add",
                    "commit"
                ]
                .iter()
                .any(|command| arg.as_str() == Some(*command)),
                "Read-only navigation launched a mutation or transport: {row}"
            );
        }
    }
    fs::create_dir_all(&session.artifact_dir).unwrap();
    fs::copy(&trace, session.artifact_dir.join("sparse-git-trace2.jsonl")).unwrap();
    let proof = json!({"kind":"actual-offline-real-git-sparse-read","requested_input_path":if mouse {"mouse"} else {"keyboard"},"actual_input_evidence":"independent shared-framework journey journal","head":original.head.trim(),"index_sha256":digest(&original.index),"config_sha256":digest(&original.config),"worktree_config_sha256":original.worktree_config.as_deref().map(digest),"sparse_patterns_sha256":digest(&original.patterns),"dirty_sha256":digest(&original.dirty),"untracked_sha256":digest(&original.untracked),"trace_sha256":digest(&trace_bytes),"git_starts":starts.len(),"exact_state_unchanged":true,"excluded_working_directory_absent":true,"gh_commands":0});
    fs::write(
        session.artifact_dir.join("sparse-read-proof.json"),
        serde_json::to_vec_pretty(&proof).unwrap(),
    )
    .unwrap();
    eprintln!("Sparse read evidence: {}", session.artifact_dir.display());
}

#[test]
fn git_provider_sparse_keyboard_saved_outside_cone_preserves_raw_index_and_working_files() {
    journey(false);
}
#[test]
fn git_provider_sparse_mouse_saved_outside_cone_preserves_raw_index_and_working_files() {
    journey(true);
}
