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
    // Continue is planned from refreshed state: a stale conflicted status refuses it.
    let resolved =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    execute(&prepare(LocalCommand::Continue, &resolved, &[]));
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
        if ghtui::git::bisect_reports_culprit(&result.output) {
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

/// Local main and the bare remote's main each gain one commit; `topic` holds the remote-only commit.
fn v1_local_branches_diverged_fixture() -> (Fixture, std::path::PathBuf, String, String) {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    let remote = fixture.root.join("remote.git");
    assert!(Command::new("git")
        .args(["init", "--bare", "--initial-branch=main"])
        .arg(&remote)
        .output()
        .unwrap()
        .status
        .success());
    fixture.git(&["remote", "add", "origin", remote.to_str().unwrap()]);
    fixture.git(&["push", "-u", "origin", "main"]);
    fixture.git(&["switch", "-c", "side"]);
    fixture.commit_file("remote.txt", b"remote side\n", "remote side");
    let remote_head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["push", "origin", "side:main"]);
    fixture.git(&["switch", "main"]);
    fixture.git(&["branch", "-D", "side"]);
    fixture.commit_file("local.txt", b"local side\n", "local side");
    let local_head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["fetch", "origin"]);
    fixture.git(&["branch", "--no-track", "topic", "origin/main"]);
    (fixture, remote, local_head, remote_head)
}
fn v1_remote_main(remote: &std::path::Path) -> String {
    let output = Command::new("git")
        .arg("--git-dir")
        .arg(remote)
        .args(["rev-parse", "refs/heads/main"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_string()
}
fn v1_has(plan: &LocalPlan, needle: &str) -> bool {
    plan.preview.iter().any(|line| line.contains(needle))
}

#[test]
fn v1_local_branches_diverged_pull_push_previews_explain_and_failures_preserve_refs() {
    let (fixture, remote, local_head, remote_head) = v1_local_branches_diverged_fixture();
    let overview = workspace(&fixture, RepositoryMode::Overview);
    assert!(overview.rows.iter().any(|row| row
        .label
        .contains("1 commits to send, 1 commits to receive • DIVERGED")));
    let context = LocalContext::from_workspace(&overview, None, None);

    let pull = prepare(
        LocalCommand::Pull,
        &context,
        &[
            ("remote", "origin"),
            ("branch", "main"),
            ("strategy", "ff-only"),
        ],
    );
    assert!(
        v1_has(&pull, "DIVERGED: main and origin/main"),
        "{:?}",
        pull.preview
    );
    assert!(v1_has(&pull, "Fast-forward only will refuse"));
    let error = pull
        .execute(true, CancellationToken::default())
        .unwrap_err();
    assert!(
        error.starts_with("Pull refused: local and remote histories have diverged"),
        "{error}"
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), local_head);

    let push = prepare(
        LocalCommand::Push,
        &context,
        &[
            ("remote", "origin"),
            ("branch", "main"),
            ("set_upstream", "false"),
        ],
    );
    assert!(v1_has(&push, "REMOTE AHEAD"), "{:?}", push.preview);
    assert!(v1_has(&push, "Local branch main tracks origin/main"));
    // A failed write requires an explicit refresh before the next write.
    let error = push
        .execute(true, CancellationToken::default())
        .unwrap_err();
    assert!(error.starts_with("An earlier Git write"), "{error}");
    reconcile(&fixture.repo(), CancellationToken::default()).unwrap();
    let error = push
        .execute(true, CancellationToken::default())
        .unwrap_err();
    assert!(
        error.starts_with("Push rejected: the remote branch has commits"),
        "{error}"
    );
    assert_eq!(v1_remote_main(&remote), remote_head);
    reconcile(&fixture.repo(), CancellationToken::default()).unwrap();

    let rebase = prepare(
        LocalCommand::Pull,
        &context,
        &[
            ("remote", "origin"),
            ("branch", "main"),
            ("strategy", "rebase"),
        ],
    );
    assert!(v1_has(&rebase, "Rebase replays your 1 local commit on top"));
    execute(&rebase);
    assert_eq!(fixture.git(&["rev-parse", "HEAD~1"]).trim(), remote_head);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let push = prepare(
        LocalCommand::Push,
        &context,
        &[
            ("remote", "origin"),
            ("branch", "main"),
            ("set_upstream", "false"),
        ],
    );
    assert!(v1_has(&push, "sends 1 commit;"), "{:?}", push.preview);
    assert!(!v1_has(&push, "REMOTE AHEAD"));
    execute(&push);
    assert_eq!(
        v1_remote_main(&remote),
        fixture.git(&["rev-parse", "HEAD"]).trim()
    );
}

#[test]
fn v1_local_branches_first_push_defaults_tracking_and_unmerged_delete_shows_recovery() {
    let (fixture, remote, _, remote_head) = v1_local_branches_diverged_fixture();
    fixture.git(&["switch", "-c", "feature"]);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    assert!(workspace(&fixture, RepositoryMode::Overview)
        .rows
        .iter()
        .any(|row| row.label.contains("Branch: feature") && row.label.contains("no upstream")));
    let defaults = ghtui::local_actions::fields(LocalCommand::Push, &context);
    assert_eq!(
        defaults
            .iter()
            .find(|field| field.key == "set_upstream")
            .unwrap()
            .value,
        "true"
    );
    let push = prepare(
        LocalCommand::Push,
        &context,
        &[
            ("remote", "origin"),
            ("branch", "feature"),
            ("set_upstream", "true"),
        ],
    );
    assert!(
        v1_has(&push, "FIRST PUSH: creates branch feature on origin."),
        "{:?}",
        push.preview
    );
    assert!(v1_has(&push, "Local branch feature has no upstream"));
    execute(&push);
    assert_eq!(
        fixture.git(&["rev-parse", "--abbrev-ref", "@{upstream}"]),
        "origin/feature\n"
    );
    let delete = prepare(
        LocalCommand::DeleteBranch,
        &context,
        &[("name", "topic"), ("force", "false")],
    );
    assert!(v1_has(
        &delete,
        &format!("Recovery: branch topic currently points at {remote_head}")
    ));
    assert!(v1_has(
        &delete,
        "UNMERGED: topic has 1 commit not in the current branch"
    ));
    let error = delete
        .execute(true, CancellationToken::default())
        .unwrap_err();
    assert!(error.starts_with("Branch not deleted"), "{error}");
    assert_eq!(fixture.git(&["rev-parse", "topic"]).trim(), remote_head);
    assert_eq!(v1_remote_main(&remote), remote_head);
}

#[test]
fn v1_local_recovery_paused_merge_overview_rows_marker_warning_and_named_version() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"base\n", "base");
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("file.txt", b"theirs\n", "theirs");
    fixture.git(&["switch", "main"]);
    fixture.commit_file("file.txt", b"ours\n", "ours");
    fs::write(fixture.repo().join("other.txt"), b"stashed\n").unwrap();
    fixture.git(&["stash", "push", "-u", "-m", "parked work"]);
    let _ = Command::new("git")
        .args(["merge", "feature"])
        .current_dir(fixture.repo())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    let view = workspace(&fixture, RepositoryMode::Overview);
    let labels: Vec<_> = view.rows.iter().map(|row| row.label.as_str()).collect();
    assert!(
        labels
            .iter()
            .any(|label| label.starts_with("⚠ MERGE PAUSED • 1 unresolved conflict")),
        "{labels:?}"
    );
    assert!(labels
        .iter()
        .any(|label| label.starts_with("Saved stashes: 1 • newest stash@{0}")));
    let position = |action: RowAction| {
        view.rows
            .iter()
            .position(|row| row.action.as_ref() == Some(&action))
    };
    let conflict = position(RowAction::Conflict(RawPath::new(b"file.txt".to_vec()))).unwrap();
    let continue_row = position(RowAction::Local(LocalCommand::Continue)).unwrap();
    let abort_row = position(RowAction::Local(LocalCommand::Abort)).unwrap();
    assert!(conflict < continue_row && continue_row < abort_row);
    assert!(position(RowAction::Local(LocalCommand::Skip)).is_none());
    // Unresolved Continue is refused before any form or Git child.
    let context = LocalContext::from_row(&view, continue_row);
    assert!(LocalCommand::Continue
        .disabled_reason(&context)
        .unwrap()
        .contains("Resolve and stage 1 conflicted file"));
    assert!(LocalCommand::Merge
        .disabled_reason(&context)
        .unwrap()
        .contains("merge is paused"));
    let selected = LocalContext::from_row(&view, conflict);
    let stage = prepare(LocalCommand::StageFile, &selected, &[]);
    assert!(stage
        .preview
        .iter()
        .any(|line| line.contains("still contains <<<<<<<")));
    execute(&prepare(
        LocalCommand::ResolveConflict,
        &selected,
        &[("stage", "theirs")],
    ));
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"theirs\n"
    );
    let stage = prepare(LocalCommand::StageFile, &selected, &[]);
    assert!(!stage
        .preview
        .iter()
        .any(|line| line.contains("still contains <<<<<<<")));
    execute(&stage);
    let resolved = workspace(&fixture, RepositoryMode::Overview);
    assert!(resolved
        .rows
        .iter()
        .any(|row| row.label == "⚠ MERGE PAUSED • all conflicts resolved and staged"));
    let row = resolved
        .rows
        .iter()
        .position(|row| row.action == Some(RowAction::Local(LocalCommand::Continue)))
        .unwrap();
    assert_eq!(
        LocalCommand::Continue.disabled_reason(&LocalContext::from_row(&resolved, row)),
        None
    );
    assert!(fixture.repo().join(".git/MERGE_HEAD").exists());
}
