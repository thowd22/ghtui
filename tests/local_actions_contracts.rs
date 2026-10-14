mod support;
use ghtui::{
    git::RawPath,
    local_actions::{
        reconcile, redact_remote_credentials, LocalCommand, LocalContext, LocalInventory, LocalPlan,
    },
    providers::runner::CancellationToken,
    workspace::{RepositoryMode, RowAction, Workspace},
};
use std::{collections::BTreeMap, fs, process::Command};
use support::Fixture;

fn workspace(fixture: &Fixture, mode: RepositoryMode) -> Workspace {
    Workspace::load(&fixture.repo(), mode, "HEAD", None)
        .unwrap()
        .unwrap()
}
fn values(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

#[test]
fn staging_untracked_text_binary_and_symlink_previews_disclose_bytes_without_target_reads() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("saved", b"saved\n", "saved version");
    fs::write(fixture.repo().join("new.txt"), b"new body\nsecond line\n").unwrap();
    fs::write(fixture.repo().join("new.bin"), b"binary\0body").unwrap();
    fs::write(fixture.repo().join("invalid-utf8.bin"), b"invalid\xffbody").unwrap();
    let outside = fixture.root.join("outside-private");
    fs::write(&outside, b"referenced content must not be read\n").unwrap();
    symlink(&outside, fixture.repo().join("new.link")).unwrap();
    for (name, expected) in [
        ("new.txt", "new body"),
        ("new.bin", "Binary new file: 11 bytes"),
        ("invalid-utf8.bin", "Binary new file: 12 bytes"),
        ("new.link", "Symbolic link target:"),
    ] {
        let context = LocalContext::from_workspace(
            &workspace(&fixture, RepositoryMode::Overview),
            Some(RawPath::new(name.as_bytes().to_vec())),
            None,
        );
        let plan = prepare(LocalCommand::StageFile, &context, &[]);
        assert!(
            plan.preview.iter().any(|line| line.contains(expected)),
            "{:?}",
            plan.preview
        );
        assert!(!plan
            .preview
            .iter()
            .any(|line| line.contains("referenced content must not be read")));
        assert_eq!(fixture.git(&["ls-files", "--", name]), "");
        assert!(plan.execute(false, CancellationToken::default()).is_err());
        assert_eq!(fixture.git(&["ls-files", "--", name]), "");
    }
}
fn prepare(command: LocalCommand, context: &LocalContext, inputs: &[(&str, &str)]) -> LocalPlan {
    LocalPlan::prepare(
        command,
        context,
        &values(inputs),
        CancellationToken::default(),
    )
    .unwrap()
}
fn execute(plan: &LocalPlan) {
    plan.execute(true, CancellationToken::default()).unwrap();
}

#[test]
fn staging_exact_raw_file_requires_confirmation_and_preserves_unrelated_edits() {
    let fixture = Fixture::new();
    fixture.init_git();
    let name = "日本語\tfile.txt";
    fixture.commit_file(name, b"saved\n", "saved version");
    fs::write(fixture.repo().join(name), b"selected edits\n").unwrap();
    fs::write(fixture.repo().join("unrelated.txt"), b"unrelated edits\n").unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let view = workspace(&fixture, RepositoryMode::Overview);
    let path = RawPath::new(name.as_bytes().to_vec());
    let row = view
        .rows
        .iter()
        .position(|row| row.action == Some(RowAction::WorkingFile(path.clone())))
        .unwrap();
    let context = LocalContext::from_row(&view, row);
    assert_eq!(context.selected_path, Some(path));
    let plan = prepare(LocalCommand::StageFile, &context, &[]);
    assert!(plan
        .preview
        .iter()
        .any(|line| line.contains("日本語\\tfile.txt")));
    assert!(plan.execute(false, CancellationToken::default()).is_err());
    assert_eq!(fixture.git(&["diff", "--cached", "--name-only"]), "");
    execute(&plan);
    assert_eq!(
        fixture.git(&["show", &format!(":{name}")]),
        "selected edits\n"
    );
    assert!(fixture
        .git(&["status", "--porcelain"])
        .contains("?? unrelated.txt"));
    execute(&prepare(LocalCommand::UnstageFile, &context, &[]));
    assert_eq!(fixture.git(&["show", &format!(":{name}")]), "saved\n");
    assert_eq!(
        fs::read(fixture.repo().join(name)).unwrap(),
        b"selected edits\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("unrelated.txt")).unwrap(),
        b"unrelated edits\n"
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
}

#[test]
fn staging_canonical_selected_line_rejects_stale_render_before_planning() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"one\ntwo\n", "saved version");
    fs::write(
        fixture.repo().join("file.txt"),
        b"one\nfirst insertion\nsecond insertion\ntwo\n",
    )
    .unwrap();
    let view = workspace(&fixture, RepositoryMode::Working);
    let row = view
        .rows
        .iter()
        .position(|row| {
            row.label.contains("first insertion")
                && matches!(row.action, Some(RowAction::DiffLine(_)))
        })
        .unwrap();
    let context = LocalContext::from_row(&view, row);
    let plan = prepare(LocalCommand::StageSelection, &context, &[]);
    assert!(plan.preview.iter().any(|line| line == "+first insertion"));
    assert!(!plan.preview.iter().any(|line| line == "+second insertion"));
    execute(&plan);
    assert_eq!(
        fixture.git(&["show", ":file.txt"]),
        "one\nfirst insertion\ntwo\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"one\nfirst insertion\nsecond insertion\ntwo\n"
    );
    assert!(LocalPlan::prepare(
        LocalCommand::StageSelection,
        &context,
        &BTreeMap::new(),
        CancellationToken::default()
    )
    .unwrap_err()
    .contains("stale"));
}

#[test]
fn local_commit_exact_staged_preview_rejects_changed_state_then_commits_full_message() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    fs::write(fixture.repo().join("file.txt"), b"staged\n").unwrap();
    fixture.git(&["add", "file.txt"]);
    fs::write(fixture.repo().join("file.txt"), b"still unstaged\n").unwrap();
    let before = fixture.git(&["rev-parse", "HEAD"]);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let message = "Native commit 日本語\n\nFull multiline message preserved.\n";
    let plan = prepare(LocalCommand::Commit, &context, &[("message", message)]);
    assert!(plan.preview.iter().any(|line| line == "+staged"));
    assert!(!plan.preview.iter().any(|line| line == "+still unstaged"));
    fs::write(fixture.repo().join("other.txt"), b"changed after preview\n").unwrap();
    assert!(plan
        .execute(true, CancellationToken::default())
        .unwrap_err()
        .contains("changed"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), before);
    execute(&prepare(
        LocalCommand::Commit,
        &context,
        &[("message", message)],
    ));
    assert_ne!(fixture.git(&["rev-parse", "HEAD"]), before);
    assert_eq!(fixture.git(&["show", "HEAD:file.txt"]), "staged\n");
    assert_eq!(
        fixture.git(&["log", "-1", "--format=%B"]),
        format!("{message}\n")
    );
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"still unstaged\n"
    );
}

#[test]
fn branches_tags_native_intents_retain_exact_targets_and_cancel_deletion() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    execute(&prepare(
        LocalCommand::CreateBranch,
        &context,
        &[("name", "feature"), ("revision", &head)],
    ));
    execute(&prepare(
        LocalCommand::SwitchBranch,
        &context,
        &[("name", "feature")],
    ));
    execute(&prepare(
        LocalCommand::RenameBranch,
        &context,
        &[("old", "feature"), ("new", "reviewed")],
    ));
    execute(&prepare(
        LocalCommand::CreateTag,
        &context,
        &[("name", "v-test"), ("revision", &head)],
    ));
    assert_eq!(fixture.git(&["branch", "--show-current"]), "reviewed\n");
    assert_eq!(fixture.git(&["rev-parse", "v-test"]), format!("{head}\n"));
    let deletion = prepare(LocalCommand::DeleteTag, &context, &[("name", "v-test")]);
    assert!(deletion.mutation.destructive);
    assert!(deletion
        .execute(false, CancellationToken::default())
        .is_err());
    assert_eq!(fixture.git(&["rev-parse", "v-test"]), format!("{head}\n"));
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
}

#[test]
fn remote_git_explicit_bare_push_and_url_preview_never_exposes_credentials() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    let remote = fixture.root.join("remote.git");
    assert!(Command::new("git")
        .args(["init", "--bare"])
        .arg(&remote)
        .output()
        .unwrap()
        .status
        .success());
    fixture.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let plan = prepare(
        LocalCommand::Push,
        &context,
        &[
            ("remote", "origin"),
            ("branch", "main"),
            ("set_upstream", "true"),
        ],
    );
    assert!(plan
        .preview
        .iter()
        .any(|line| line.contains(remote.to_str().unwrap())));
    assert!(plan.execute(false, CancellationToken::default()).is_err());
    execute(&plan);
    let observed = Command::new("git")
        .arg("--git-dir")
        .arg(remote)
        .args(["rev-parse", "refs/heads/main"])
        .output()
        .unwrap();
    assert!(observed.status.success());
    assert_eq!(
        String::from_utf8(observed.stdout).unwrap(),
        fixture.git(&["rev-parse", "HEAD"])
    );
    assert_eq!(
        fixture.git(&["rev-parse", "--abbrev-ref", "@{upstream}"]),
        "origin/main\n"
    );
    let safe =
        redact_remote_credentials("origin (https://alice:secret-token@github.com/org/project.git)");
    assert_eq!(
        safe,
        "origin (https://[userinfo redacted]@github.com/org/project.git)"
    );
}

#[test]
fn local_commit_rejecting_hook_preserves_head_index_and_requires_explicit_recovery() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    fs::write(fixture.repo().join("file.txt"), b"staged\n").unwrap();
    fixture.git(&["add", "file.txt"]);
    let hook = fixture.repo().join(".git/hooks/pre-commit");
    fs::write(
        &hook,
        b"#!/bin/sh\necho 'fixture hook rejected commit' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(hook, fs::Permissions::from_mode(0o700)).unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let staged = fixture.git(&["show", ":file.txt"]);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let failure = prepare(
        LocalCommand::Commit,
        &context,
        &[("message", "rejected commit")],
    )
    .execute(true, CancellationToken::default())
    .unwrap_err();
    assert!(failure.contains("fixture hook rejected commit"));
    assert!(failure.contains("Refresh repository state"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["show", ":file.txt"]), staged);
    let refreshed = reconcile(&fixture.repo(), CancellationToken::default()).unwrap();
    assert_eq!(refreshed.status.head.unwrap(), head.trim());
    assert!(refreshed.status.entries.iter().any(|entry| entry.staged()));
}

#[test]
fn stash_worktrees_inventory_uses_immutable_stash_ids_and_blocks_dirty_removal() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fs::write(fixture.repo().join("file.txt"), b"important dirty work\n").unwrap();
    fs::write(fixture.repo().join("untracked.txt"), b"kept untracked\n").unwrap();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    execute(&prepare(
        LocalCommand::StashSave,
        &context,
        &[("message", "safe stash"), ("untracked", "false")],
    ));
    let inventory = LocalInventory::load(&fixture.repo(), CancellationToken::default()).unwrap();
    assert_eq!(inventory.stashes.len(), 1);
    let stash = inventory.stashes[0].id.clone();
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"saved\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("untracked.txt")).unwrap(),
        b"kept untracked\n"
    );
    let application = prepare(LocalCommand::StashApply, &context, &[("id", &stash)]);
    assert!(application
        .preview
        .iter()
        .any(|line| line.contains(&format!("Exact stash commit: {stash}"))));
    execute(&application);
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"important dirty work\n"
    );
    assert_eq!(
        LocalInventory::load(&fixture.repo(), CancellationToken::default())
            .unwrap()
            .stashes[0]
            .id,
        stash
    );
    execute(&prepare(
        LocalCommand::CreateBranch,
        &context,
        &[("name", "linked"), ("revision", &head)],
    ));
    let linked = fixture.root.join("linked-worktree");
    execute(&prepare(
        LocalCommand::AddWorktree,
        &context,
        &[("path", linked.to_str().unwrap()), ("branch", "linked")],
    ));
    fs::write(linked.join("file.txt"), b"linked dirty work\n").unwrap();
    let removal = LocalPlan::prepare(
        LocalCommand::RemoveWorktree,
        &context,
        &values(&[("path", linked.to_str().unwrap())]),
        CancellationToken::default(),
    )
    .unwrap_err();
    assert!(removal.to_ascii_lowercase().contains("dirty"));
    assert_eq!(
        fs::read(linked.join("file.txt")).unwrap(),
        b"linked dirty work\n"
    );
    fs::write(linked.join("file.txt"), b"saved\n").unwrap();
    execute(&prepare(
        LocalCommand::RemoveWorktree,
        &context,
        &[("path", linked.to_str().unwrap())],
    ));
    assert!(!linked.exists());
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"important dirty work\n"
    );
}

#[test]
fn git_sequencers_native_conflict_choice_stage_continue_retains_two_parents() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"base\n", "base");
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("file.txt", b"theirs\n", "theirs");
    let theirs = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["switch", "main"]);
    fixture.commit_file("file.txt", b"ours\n", "ours");
    let ours = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    assert!(
        prepare(LocalCommand::Merge, &context, &[("revision", &theirs)])
            .execute(true, CancellationToken::default())
            .is_err()
    );
    let view = reconcile(&fixture.repo(), CancellationToken::default()).unwrap();
    assert_eq!(view.status.conflicts(), 1);
    let path = RawPath::new(b"file.txt".to_vec());
    let row = view
        .rows
        .iter()
        .position(|row| row.action == Some(RowAction::Conflict(path.clone())))
        .unwrap();
    let conflict = LocalContext::from_row(&view, row);
    execute(&prepare(
        LocalCommand::ResolveConflict,
        &conflict,
        &[("stage", "2")],
    ));
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"ours\n"
    );
    assert!(fixture
        .git(&["ls-files", "--unmerged"])
        .contains("file.txt"));
    execute(&prepare(LocalCommand::StageFile, &conflict, &[]));
    execute(&prepare(LocalCommand::Continue, &conflict, &[]));
    assert_eq!(fixture.git(&["rev-parse", "HEAD^1"]), format!("{ours}\n"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD^2"]), format!("{theirs}\n"));
    assert_eq!(fixture.git(&["ls-files", "--unmerged"]), "");
    assert!(workspace(&fixture, RepositoryMode::Overview)
        .status
        .operation
        .is_none());
}

#[test]
fn reflog_recovery_exact_rescue_ref_does_not_reset_dirty_work() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"old version\n", "old");
    let old = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.commit_file("file.txt", b"new version\n", "new");
    let current = fixture.git(&["rev-parse", "HEAD"]);
    fs::write(fixture.repo().join("file.txt"), b"important dirty work\n").unwrap();
    let inventory = LocalInventory::load(&fixture.repo(), CancellationToken::default()).unwrap();
    assert!(inventory.reflog.iter().any(|entry| entry.id == old));
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    execute(&prepare(
        LocalCommand::RecoverBranch,
        &context,
        &[("name", "rescue"), ("revision", &old)],
    ));
    assert_eq!(fixture.git(&["rev-parse", "rescue"]), format!("{old}\n"));
    let reset = prepare(
        LocalCommand::Reset,
        &context,
        &[("revision", &old), ("mode", "hard")],
    );
    assert!(reset.mutation.destructive);
    assert!(reset.execute(false, CancellationToken::default()).is_err());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), current);
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"important dirty work\n"
    );
}

#[test]
fn guided_bisect_native_classification_progress_and_reset_restore_original_branch() {
    let fixture = Fixture::new();
    fixture.init_git();
    let mut commits = vec![];
    for index in 0..6 {
        fixture.commit_file(
            "version.txt",
            format!("version {index}\n").as_bytes(),
            &format!("version {index}"),
        );
        commits.push(fixture.git(&["rev-parse", "HEAD"]).trim().to_string());
    }
    let start = fixture.git(&["rev-parse", "HEAD"]);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    execute(&prepare(
        LocalCommand::BisectStart,
        &context,
        &[("good", &commits[0]), ("bad", &commits[5])],
    ));
    let mut culprit = None;
    for _ in 0..8 {
        let view = workspace(&fixture, RepositoryMode::Overview);
        let current = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
        let index = commits.iter().position(|id| *id == current).unwrap();
        let context = LocalContext::from_workspace(&view, None, None);
        let result = prepare(
            if index >= 3 {
                LocalCommand::BisectBad
            } else {
                LocalCommand::BisectGood
            },
            &context,
            &[],
        )
        .execute(true, CancellationToken::default())
        .unwrap();
        assert!(result
            .workspace
            .rows
            .iter()
            .any(|row| row.label.contains("Bug search") || row.label.contains("Culprit found:")));
        if result.output.contains("first bad commit") {
            culprit = Some(result.workspace);
            break;
        }
    }
    let culprit = culprit.expect("fixture must converge to a unique first bad commit");
    assert_eq!(culprit.status.head.as_ref().unwrap(), &commits[3]);
    assert!(culprit
        .rows
        .iter()
        .any(|row| row.label.contains("Culprit found:")));
    let context = LocalContext::from_workspace(&culprit, None, None);
    execute(&prepare(LocalCommand::BisectReset, &context, &[]));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), start);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
}
