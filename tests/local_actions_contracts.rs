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

fn has_line(lines: &[String], needle: &str) -> bool {
    lines.iter().any(|line| line.contains(needle))
}

#[test]
fn stash_worktrees_view_contents_pop_drop_locked_and_cancelled_removal() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    fs::write(fixture.repo().join("file.txt"), b"tracked work\n").unwrap();
    fs::write(fixture.repo().join("new.txt"), b"untracked work\n").unwrap();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    execute(&prepare(
        LocalCommand::StashSave,
        &context,
        &[("message", "with untracked"), ("untracked", "true")],
    ));
    let with_untracked = fixture.git(&["rev-parse", "refs/stash"]).trim().to_string();
    assert!(!fixture.repo().join("new.txt").exists());
    // View: the exact stash's tracked patch and saved untracked files, read-only.
    let view = prepare(
        LocalCommand::StashDrop,
        &context,
        &[("id", &with_untracked)],
    );
    for needle in [
        &format!("Exact stash commit: {with_untracked}") as &str,
        "Stash contents (read-only view of this exact stash):",
        "+tracked work",
        "Untracked files saved in this stash: 1",
        "  untracked new.txt",
    ] {
        assert!(
            has_line(&view.preview, needle),
            "{needle}: {:?}",
            view.preview
        );
    }
    // Unconfirmed drop is refused and changes nothing.
    assert!(view.execute(false, CancellationToken::default()).is_err());
    assert_eq!(
        fixture.git(&["rev-parse", "refs/stash"]),
        format!("{with_untracked}\n")
    );
    fs::write(fixture.repo().join("file.txt"), b"second work\n").unwrap();
    execute(&prepare(
        LocalCommand::StashSave,
        &context,
        &[("message", "tracked only"), ("untracked", "false")],
    ));
    let tracked_only = fixture.git(&["rev-parse", "refs/stash"]).trim().to_string();
    let pop = prepare(LocalCommand::StashPop, &context, &[("id", &tracked_only)]);
    assert!(has_line(
        &pop.preview,
        "Untracked files: not included in this stash"
    ));
    assert!(has_line(&pop.preview, "+second work"));
    assert!(!has_line(&pop.preview, "untracked new.txt"));
    execute(&pop);
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"second work\n"
    );
    let listed = fixture.git(&["stash", "list", "--format=%H"]);
    assert_eq!(listed, format!("{with_untracked}\n"));
    // Confirmed drop removes exactly the selected stash; working files are untouched.
    execute(&prepare(
        LocalCommand::StashDrop,
        &context,
        &[("id", &with_untracked)],
    ));
    assert_eq!(fixture.git(&["stash", "list"]), "");
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"second work\n"
    );
    assert!(!fixture.repo().join("new.txt").exists());

    // Worktree list: other worktrees show branch and dirty state and open read-only.
    fixture.git(&["branch", "linked"]);
    let linked = fixture.root.join("linked-worktree");
    fixture.git(&["worktree", "add", linked.to_str().unwrap(), "linked"]);
    let linked = linked.canonicalize().unwrap();
    fs::write(linked.join("file.txt"), b"linked dirty\n").unwrap();
    let overview = workspace(&fixture, RepositoryMode::Overview);
    let row = overview
        .rows
        .iter()
        .find(|row| row.action == Some(RowAction::OpenWorktree(linked.clone())))
        .expect("linked worktree row");
    assert!(
        row.label.contains("dirty: uncommitted changes"),
        "{}",
        row.label
    );
    assert!(row.label.contains("linked"), "{}", row.label);
    let opened = Workspace::load(&linked, RepositoryMode::Overview, "HEAD", None)
        .unwrap()
        .unwrap();
    assert_eq!(opened.directory, linked);
    assert_eq!(opened.status.branch.as_deref(), Some("linked"));
    let main_root = fixture.repo().canonicalize().unwrap();
    assert!(opened.rows.iter().any(
        |row| matches!(&row.action, Some(RowAction::OpenWorktree(path)) if *path == main_root)
    ));
    assert_eq!(
        fs::read(linked.join("file.txt")).unwrap(),
        b"linked dirty\n"
    );
    fs::write(linked.join("file.txt"), b"saved\n").unwrap();
    let clean = workspace(&fixture, RepositoryMode::Overview);
    assert!(clean.rows.iter().any(|row| row.action
        == Some(RowAction::OpenWorktree(linked.clone()))
        && row.label.contains("• clean")));
    // A locked worktree is refused before any confirmation.
    fixture.git(&["worktree", "lock", linked.to_str().unwrap()]);
    let locked = LocalPlan::prepare(
        LocalCommand::RemoveWorktree,
        &context,
        &values(&[("path", linked.to_str().unwrap())]),
        CancellationToken::default(),
    )
    .unwrap_err();
    assert!(locked.contains("locked"), "{locked}");
    assert!(linked.join(".git").exists());
    fixture.git(&["worktree", "unlock", linked.to_str().unwrap()]);
    // A cancelled (unconfirmed) removal leaves the worktree in place.
    let removal = prepare(
        LocalCommand::RemoveWorktree,
        &context,
        &[("path", linked.to_str().unwrap())],
    );
    assert!(removal
        .execute(false, CancellationToken::default())
        .is_err());
    assert!(linked.join(".git").exists());
    assert!(fixture
        .git(&["worktree", "list", "--porcelain"])
        .contains(linked.to_str().unwrap()));
}

#[test]
fn guided_bisect_dirty_refusal_detached_skip_interrupted_and_no_candidate() {
    let fixture = Fixture::new();
    fixture.init_git();
    let mut commits = vec![];
    for index in 0..5 {
        fixture.commit_file(
            "version.txt",
            format!("version {index}\n").as_bytes(),
            &format!("version {index}"),
        );
        commits.push(fixture.git(&["rev-parse", "HEAD"]).trim().to_string());
    }
    let good_bad = [("good", commits[0].as_str()), ("bad", commits[4].as_str())];
    // Dirty work is refused before any bisect state exists.
    fs::write(fixture.repo().join("version.txt"), b"dirty work\n").unwrap();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let refused = LocalPlan::prepare(
        LocalCommand::BisectStart,
        &context,
        &values(&good_bad),
        CancellationToken::default(),
    )
    .unwrap_err();
    assert!(refused.contains("Save or stash local changes"), "{refused}");
    assert!(!fixture.repo().join(".git/BISECT_LOG").exists());
    assert_eq!(
        fs::read(fixture.repo().join("version.txt")).unwrap(),
        b"dirty work\n"
    );
    fs::write(fixture.repo().join("version.txt"), b"version 4\n").unwrap();
    // Start from a detached HEAD; reset must restore that exact detached version.
    fixture.git(&["checkout", "-q", "--detach", &commits[4]]);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let started = prepare(LocalCommand::BisectStart, &context, &good_bad)
        .execute(true, CancellationToken::default())
        .unwrap();
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    let labels = started
        .workspace
        .rows
        .iter()
        .map(|row| row.label.clone())
        .collect::<Vec<_>>();
    assert!(
        labels
            .iter()
            .any(|label| label.starts_with(&format!("Candidate: {head} "))
                && label.contains("left to test after this")),
        "{labels:?}"
    );
    // Interrupted: a fresh load (as after a restart) shows the search and its recovery rows.
    let resumed = workspace(&fixture, RepositoryMode::Overview);
    assert!(resumed
        .rows
        .iter()
        .any(|row| row.label.contains("Bug search in progress")));
    for command in [
        LocalCommand::BisectGood,
        LocalCommand::BisectBad,
        LocalCommand::BisectSkip,
        LocalCommand::BisectReset,
    ] {
        assert!(
            resumed
                .rows
                .iter()
                .any(|row| row.action == Some(RowAction::Local(command))),
            "{command:?}"
        );
    }
    // Skip every candidate until Git reports no feasible single culprit.
    let mut no_candidate = None;
    for _ in 0..5 {
        let context = LocalContext::from_workspace(
            &workspace(&fixture, RepositoryMode::Overview),
            None,
            None,
        );
        match prepare(LocalCommand::BisectSkip, &context, &[])
            .execute(true, CancellationToken::default())
        {
            Ok(result) => assert!(result
                .workspace
                .rows
                .iter()
                .any(|row| row.label.contains("Bug search is paused"))),
            Err(error) => {
                no_candidate = Some(error);
                break;
            }
        }
    }
    let no_candidate = no_candidate.expect("skipping every candidate must stop");
    assert!(
        no_candidate.starts_with("No feasible candidate"),
        "{no_candidate}"
    );
    assert!(no_candidate.contains("could be any of"), "{no_candidate}");
    assert!(fixture.git(&["bisect", "log"]).contains("git bisect skip"));
    assert!(fixture.repo().join(".git/BISECT_LOG").exists());
    reconcile(&fixture.repo(), CancellationToken::default()).unwrap();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    execute(&prepare(LocalCommand::BisectReset, &context, &[]));
    assert_eq!(
        fixture.git(&["rev-parse", "HEAD"]),
        format!("{}\n", commits[4])
    );
    assert_eq!(fixture.git(&["branch", "--show-current"]), "");
    assert_eq!(fixture.git(&["status", "--porcelain"]), "");
    assert!(!fixture.repo().join(".git/BISECT_LOG").exists());
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

/// Runs installed Git in `directory` with the fixture's isolated configuration; returns success.
fn v1_gaps_git(directory: &std::path::Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(directory)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_EDITOR", "true")
        .output()
        .unwrap()
        .status
        .success()
}
fn v1_gaps_conflicting_branches(fixture: &Fixture) -> String {
    fixture.init_git();
    fixture.commit_file("file.txt", b"base\n", "base");
    fixture.git(&["switch", "-c", "feature"]);
    fixture.commit_file("file.txt", b"feature\n", "feature edit");
    let feature = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["switch", "main"]);
    fixture.commit_file("file.txt", b"main\n", "main edit");
    feature
}

#[test]
fn v1_local_gaps_rebase_cherry_pick_and_revert_pauses_name_operation_and_recovery_rows() {
    for (kind, banner) in [
        ("rebase", "⚠ REBASE PAUSED • 1 unresolved conflict"),
        (
            "cherry-pick",
            "⚠ CHERRY-PICK PAUSED • 1 unresolved conflict",
        ),
        ("revert", "⚠ REVERT PAUSED • 1 unresolved conflict"),
    ] {
        let fixture = Fixture::new();
        let feature = v1_gaps_conflicting_branches(&fixture);
        let paused = match kind {
            "rebase" => {
                fixture.git(&["switch", "feature"]);
                v1_gaps_git(&fixture.repo(), &["rebase", "main"])
            }
            "cherry-pick" => v1_gaps_git(&fixture.repo(), &["cherry-pick", &feature]),
            // Reverting the base commit conflicts with the later main edit.
            _ => v1_gaps_git(&fixture.repo(), &["revert", "--no-edit", "HEAD~1"]),
        };
        assert!(!paused, "{kind} fixture must stop on a conflict");
        let head = fixture.git(&["rev-parse", "HEAD"]);
        let view = workspace(&fixture, RepositoryMode::Overview);
        let labels: Vec<_> = view.rows.iter().map(|row| row.label.as_str()).collect();
        assert!(
            labels.iter().any(|label| label.starts_with(banner)),
            "{kind}: {labels:?}"
        );
        let position = |command| {
            view.rows
                .iter()
                .position(|row| row.action == Some(RowAction::Local(command)))
        };
        let continue_row = position(LocalCommand::Continue).expect(kind);
        let skip_row = position(LocalCommand::Skip).expect(kind);
        let abort_row = position(LocalCommand::Abort).expect(kind);
        assert!(continue_row < skip_row && skip_row < abort_row, "{kind}");
        let context = LocalContext::from_row(&view, continue_row);
        assert!(LocalCommand::Continue
            .disabled_reason(&context)
            .unwrap()
            .contains("Resolve and stage 1 conflicted file"));
        // Reading the paused state never moves HEAD or finishes the operation.
        assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
        assert!(view.status.operation.is_some());
    }
}

#[test]
fn v1_local_gaps_linked_worktree_discovery_reports_its_own_branch_and_paused_cherry_pick() {
    let fixture = Fixture::new();
    let feature = v1_gaps_conflicting_branches(&fixture);
    let linked = fixture.root.join("linked-worktree");
    fixture.git(&[
        "worktree",
        "add",
        "-b",
        "picker",
        linked.to_str().unwrap(),
        "main~1",
    ]);
    fs::write(linked.join("file.txt"), b"picker\n").unwrap();
    assert!(v1_gaps_git(&linked, &["commit", "-qam", "picker edit"]));
    assert!(!v1_gaps_git(&linked, &["cherry-pick", &feature]));
    let nested = linked.join("nested");
    fs::create_dir(&nested).unwrap();
    let view = Workspace::load(&nested, RepositoryMode::Overview, "HEAD", None)
        .unwrap()
        .unwrap();
    assert_eq!(view.status.branch.as_deref(), Some("picker"));
    assert!(view
        .rows
        .iter()
        .any(|row| row.label.starts_with("⚠ CHERRY-PICK PAUSED")));
    // The primary checkout is unaffected by the linked worktree's pause.
    let main = workspace(&fixture, RepositoryMode::Overview);
    assert_eq!(main.status.branch.as_deref(), Some("main"));
    assert!(main.status.operation.is_none());
}

#[test]
fn v1_local_gaps_partial_staging_refuses_rename_addition_deletion_and_binary() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("old.txt", b"one\ntwo\n", "saved");
    fixture.commit_file("gone.txt", b"gone\n", "saved gone");
    fixture.git(&["mv", "old.txt", "renamed.txt"]);
    fs::write(fixture.repo().join("added.txt"), b"added line\n").unwrap();
    fixture.git(&["add", "added.txt"]);
    fixture.git(&["rm", "-q", "gone.txt"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let view = workspace(&fixture, RepositoryMode::Staged);
    let mut refused = 0;
    for row in (0..view.rows.len())
        .filter(|row| matches!(view.rows[*row].action, Some(RowAction::DiffLine(_))))
    {
        let context = LocalContext::from_row(&view, row);
        let error = LocalPlan::prepare(
            LocalCommand::UnstageSelection,
            &context,
            &BTreeMap::new(),
            CancellationToken::default(),
        )
        .unwrap_err();
        assert!(
            error.contains("Stage the entire file for additions, deletions, renames or binary"),
            "{error}"
        );
        refused += 1;
    }
    assert!(refused >= 2, "fixture must expose staged changed lines");
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);

    // Binary changes are refused by the shared patch builder itself.
    let binary = ghtui::diff::Diff::parse(
        b"diff --git a/b.bin b/b.bin\nindex 1111111..2222222 100644\nBinary files a/b.bin and b/b.bin differ\n",
    )
    .unwrap();
    assert!(binary
        .selected_patch(0, &[(0, 0)])
        .unwrap_err()
        .contains("binary"));
}

#[test]
fn v1_local_gaps_failed_partial_apply_reports_git_error_and_discards_nothing() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"one\ntwo\n", "saved version");
    fs::write(fixture.repo().join("file.txt"), b"one\nadded\ntwo\n").unwrap();
    let view = workspace(&fixture, RepositoryMode::Working);
    let row = view
        .rows
        .iter()
        .position(|row| {
            row.label.contains("added") && matches!(row.action, Some(RowAction::DiffLine(_)))
        })
        .unwrap();
    let plan = prepare(
        LocalCommand::StageSelection,
        &LocalContext::from_row(&view, row),
        &[],
    );
    // Another Git process holds the index: `git apply --cached` cannot write it.
    let lock = fixture.repo().join(".git/index.lock");
    fs::write(&lock, b"").unwrap();
    let error = plan
        .execute(true, CancellationToken::default())
        .unwrap_err();
    assert!(error.contains("index.lock"), "{error}");
    assert!(error.contains("Refresh repository state"), "{error}");
    fs::remove_file(&lock).unwrap();
    assert_eq!(fixture.git(&["show", ":file.txt"]), "one\ntwo\n");
    assert_eq!(
        fs::read(fixture.repo().join("file.txt")).unwrap(),
        b"one\nadded\ntwo\n"
    );
}

#[test]
fn v1_local_gaps_failing_signer_and_noninteractive_editor_prompt_preserve_staged_work() {
    use std::os::unix::fs::PermissionsExt;
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    fs::write(fixture.repo().join("file.txt"), b"staged\n").unwrap();
    fixture.git(&["add", "file.txt"]);
    let signer = fixture.root.join("failing-signer");
    fs::write(
        &signer,
        b"#!/bin/sh\necho 'fixture signer has no secret key' >&2\nexit 2\n",
    )
    .unwrap();
    fs::set_permissions(&signer, fs::Permissions::from_mode(0o700)).unwrap();
    fixture.git(&["config", "commit.gpgsign", "true"]);
    fixture.git(&["config", "gpg.program", signer.to_str().unwrap()]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let staged = fixture.git(&["show", ":file.txt"]);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let failure = prepare(LocalCommand::Commit, &context, &[("message", "signed")])
        .execute(true, CancellationToken::default())
        .unwrap_err();
    assert!(failure.starts_with("Commit signing failed"), "{failure}");
    assert!(failure.contains("failed to sign"), "{failure}");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["show", ":file.txt"]), staged);

    // Hooks run without an interactive editor or credential prompt.
    fixture.git(&["config", "commit.gpgsign", "false"]);
    let hook = fixture.repo().join(".git/hooks/pre-commit");
    fs::write(
        &hook,
        b"#!/bin/sh\necho \"editor=$GIT_EDITOR prompt=$GIT_TERMINAL_PROMPT\" >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(hook, fs::Permissions::from_mode(0o700)).unwrap();
    reconcile(&fixture.repo(), CancellationToken::default()).unwrap();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let failure = prepare(LocalCommand::Commit, &context, &[("message", "hooked")])
        .execute(true, CancellationToken::default())
        .unwrap_err();
    // Git hands hooks a no-op editor (`:`) for a supplied message; ghtui's own is `false`.
    assert!(
        failure.contains("editor=: prompt=0") || failure.contains("editor=false prompt=0"),
        "{failure}"
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["show", ":file.txt"]), staged);
}

#[test]
fn v1_local_gaps_unstage_staged_rename_restores_both_paths_and_stale_stage_is_refused() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("old.txt", b"one\ntwo\n", "saved");
    fixture.git(&["mv", "old.txt", "renamed.txt"]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let context = LocalContext::from_workspace(
        &workspace(&fixture, RepositoryMode::Overview),
        Some(RawPath::new(b"renamed.txt".to_vec())),
        None,
    );
    let plan = prepare(LocalCommand::UnstageFile, &context, &[]);
    assert!(
        plan.preview
            .iter()
            .any(|line| line.contains("restore --staged -- renamed.txt old.txt")),
        "{:?}",
        plan.preview
    );
    execute(&plan);
    // Both sides of the rename leave the index together: no lone staged deletion.
    assert_eq!(fixture.git(&["show", ":old.txt"]), "one\ntwo\n");
    assert_eq!(fixture.git(&["ls-files", "--", "renamed.txt"]), "");
    assert_eq!(fixture.git(&["diff", "--cached", "--name-only"]), "");
    assert_eq!(
        fs::read(fixture.repo().join("renamed.txt")).unwrap(),
        b"one\ntwo\n"
    );
    assert!(!fixture.repo().join("old.txt").exists());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);

    // A file edited after its preview is not staged with bytes the user never saw.
    fs::write(fixture.repo().join("renamed.txt"), b"one\ntwo\nshown\n").unwrap();
    let context = LocalContext::from_workspace(
        &workspace(&fixture, RepositoryMode::Overview),
        Some(RawPath::new(b"renamed.txt".to_vec())),
        None,
    );
    let plan = prepare(LocalCommand::StageFile, &context, &[]);
    fs::write(fixture.repo().join("renamed.txt"), b"one\ntwo\nunseen\n").unwrap();
    assert!(plan.execute(true, CancellationToken::default()).is_err());
    assert_eq!(fixture.git(&["ls-files", "--", "renamed.txt"]), "");
    assert_eq!(
        fs::read(fixture.repo().join("renamed.txt")).unwrap(),
        b"one\ntwo\nunseen\n"
    );
}

#[test]
fn v1_local_commit_amend_preview_names_replaced_published_commit_and_keeps_it_in_reflog() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("z.txt", b"keep\n", "base");
    let base = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.commit_file("a.txt", b"draft\n", "Draft subject");
    let draft = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["branch", "published"]);
    fixture.git(&["branch", "--set-upstream-to=published"]);
    fs::write(fixture.repo().join("a.txt"), b"polished\n").unwrap();
    fixture.git(&["add", "a.txt"]);
    fs::write(fixture.repo().join("z.txt"), b"unstaged\n").unwrap();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let plan = prepare(
        LocalCommand::Amend,
        &context,
        &[("message", "Polished subject\n\nBody")],
    );
    let has = |needle: &str| plan.preview.iter().any(|line| line.contains(needle));
    assert!(
        has("Commit replaced by this amend; the reflog keeps it until it expires (1):"),
        "{:?}",
        plan.preview
    );
    assert!(has(&format!("{draft} Draft subject")), "{:?}", plan.preview);
    assert!(has("already on published"), "{:?}", plan.preview);
    assert!(has("+polished"), "{:?}", plan.preview);
    assert!(plan.execute(false, CancellationToken::default()).is_err());
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), draft);
    execute(&plan);
    assert_eq!(
        fixture
            .git(&["show", "--format=%P", "--no-patch", "HEAD"])
            .trim(),
        base
    );
    assert_eq!(fixture.git(&["show", "HEAD:a.txt"]), "polished\n");
    assert_eq!(fixture.git(&["show", "HEAD:z.txt"]), "keep\n");
    assert_eq!(
        fs::read(fixture.repo().join("z.txt")).unwrap(),
        b"unstaged\n"
    );
    assert_eq!(fixture.git(&["rev-parse", "published"]).trim(), draft);
    assert!(fixture
        .git(&["reflog", "--format=%H"])
        .lines()
        .any(|line| line == draft));
}

#[test]
fn v1_local_gaps_switch_untracked_collision_worktree_owned_branch_and_detached_head() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    fixture.git(&["switch", "-c", "other"]);
    fixture.commit_file("collide.txt", b"tracked on other\n", "other adds collide");
    fixture.git(&["switch", "main"]);
    fs::write(fixture.repo().join("collide.txt"), b"my untracked notes\n").unwrap();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let error = LocalPlan::prepare(
        LocalCommand::SwitchBranch,
        &context,
        &values(&[("name", "other")]),
        CancellationToken::default(),
    )
    .and_then(|plan| plan.execute(true, CancellationToken::default()).map(|_| ()))
    .unwrap_err();
    assert!(
        // Refused before Git by the dirty-state gate, or by Git itself.
        error.contains("Save or stash local changes") || error.contains("would be overwritten"),
        "{error}"
    );
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    assert_eq!(
        fs::read(fixture.repo().join("collide.txt")).unwrap(),
        b"my untracked notes\n"
    );
    fs::remove_file(fixture.repo().join("collide.txt")).unwrap();
    reconcile(&fixture.repo(), CancellationToken::default()).unwrap();

    // A branch checked out in a linked worktree can be neither switched to nor deleted here.
    let linked = fixture.root.join("linked-worktree");
    fixture.git(&[
        "worktree",
        "add",
        "-b",
        "linked",
        linked.to_str().unwrap(),
        &head,
    ]);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let error = LocalPlan::prepare(
        LocalCommand::SwitchBranch,
        &context,
        &values(&[("name", "linked")]),
        CancellationToken::default(),
    )
    .and_then(|plan| plan.execute(true, CancellationToken::default()).map(|_| ()))
    .unwrap_err();
    assert!(error.contains("worktree"), "{error}");
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    reconcile(&fixture.repo(), CancellationToken::default()).unwrap();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let delete = prepare(
        LocalCommand::DeleteBranch,
        &context,
        &[("name", "linked"), ("force", "true")],
    );
    assert!(
        v1_has(&delete, "linked is checked out in linked worktree"),
        "{:?}",
        delete.preview
    );
    let error = delete
        .execute(true, CancellationToken::default())
        .unwrap_err();
    assert!(
        error.starts_with("Branch is checked out in this or another worktree"),
        "{error}"
    );
    assert_eq!(fixture.git(&["rev-parse", "linked"]).trim(), head);
    reconcile(&fixture.repo(), CancellationToken::default()).unwrap();

    // Detached HEAD: named, pull requests refused, and a new branch can adopt the commit.
    fixture.git(&["switch", "--detach", &head]);
    let view = workspace(&fixture, RepositoryMode::Overview);
    assert!(view.status.detached);
    assert!(
        view.rows
            .iter()
            .any(|row| row.label.contains("detached HEAD")),
        "{:?}",
        view.rows.iter().map(|row| &row.label).collect::<Vec<_>>()
    );
    let context = LocalContext::from_workspace(&view, None, None);
    assert!(LocalCommand::CreatePullRequest
        .disabled_reason(&context)
        .unwrap()
        .contains("named local branch"));
    execute(&prepare(
        LocalCommand::CreateBranch,
        &context,
        &[("name", "adopted"), ("revision", &head)],
    ));
    execute(&prepare(
        LocalCommand::SwitchBranch,
        &context,
        &[("name", "adopted")],
    ));
    assert_eq!(fixture.git(&["branch", "--show-current"]), "adopted\n");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), head);
}

#[test]
fn v1_local_gaps_unreachable_remote_cancelled_push_remote_hook_and_pull_conflict() {
    use std::os::unix::fs::PermissionsExt;
    let (fixture, remote, local_head, remote_head) = v1_local_branches_diverged_fixture();
    let absent = fixture.root.join("absent.git");
    fixture.git(&["remote", "add", "missing", absent.to_str().unwrap()]);
    let refs = fixture.git(&["show-ref"]);

    // An unreachable remote changes nothing and says so.
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let error = prepare(LocalCommand::Fetch, &context, &[("remote", "missing")])
        .execute(true, CancellationToken::default())
        .unwrap_err();
    assert!(
        error.starts_with("The remote could not be reached. Nothing was changed"),
        "{error}"
    );
    assert_eq!(fixture.git(&["show-ref"]), refs);
    reconcile(&fixture.repo(), CancellationToken::default()).unwrap();

    // A cancelled write has no remote or local effect.
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let lease = prepare(
        LocalCommand::ForcePushLease,
        &context,
        &[
            ("remote", "origin"),
            ("branch", "main"),
            ("expected_remote", &remote_head),
        ],
    );
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(lease.execute(true, cancelled).is_err());
    assert_eq!(v1_remote_main(&remote), remote_head);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    reconcile(&fixture.repo(), CancellationToken::default()).unwrap();

    // A server-side hook refusal is reported as a remote refusal; the remote ref is unchanged.
    let hook = remote.join("hooks/pre-receive");
    fs::write(
        &hook,
        b"#!/bin/sh\necho 'fixture server policy' >&2\nexit 1\n",
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o700)).unwrap();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let error = prepare(
        LocalCommand::ForcePushLease,
        &context,
        &[
            ("remote", "origin"),
            ("branch", "main"),
            ("expected_remote", &remote_head),
        ],
    )
    .execute(true, CancellationToken::default())
    .unwrap_err();
    assert!(error.starts_with("The remote refused this push"), "{error}");
    assert!(error.contains("fixture server policy"), "{error}");
    assert_eq!(v1_remote_main(&remote), remote_head);
    fs::remove_file(&hook).unwrap();
    reconcile(&fixture.repo(), CancellationToken::default()).unwrap();

    // A merge pull that conflicts pauses the merge and keeps the local commit and files.
    fixture.commit_file("remote.txt", b"local conflicting edit\n", "local edit");
    let local_edit = fixture.git(&["rev-parse", "HEAD"]);
    assert_ne!(local_edit.trim(), local_head);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let pull = prepare(
        LocalCommand::Pull,
        &context,
        &[
            ("remote", "origin"),
            ("branch", "main"),
            ("strategy", "merge"),
        ],
    );
    let error = pull
        .execute(true, CancellationToken::default())
        .unwrap_err();
    assert!(
        error.starts_with("CONFLICT: the operation stopped"),
        "{error}"
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), local_edit);
    assert_eq!(v1_remote_main(&remote), remote_head);
    let paused = workspace(&fixture, RepositoryMode::Overview);
    assert!(paused.rows.iter().any(|row| row
        .label
        .starts_with("⚠ MERGE PAUSED • 1 unresolved conflict")));
    assert!(fs::read_to_string(fixture.repo().join("remote.txt"))
        .unwrap()
        .contains("local conflicting edit"));
}

/// Install a pre-push hook that records it started and then waits (bounded) for a release
/// file the test never creates: only cancellation ends the push.
fn v40_blocking_pre_push(fixture: &Fixture) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let started = fixture.root.join("push-started");
    let release = fixture.root.join("push-release");
    let hook = fixture.repo().join(".git/hooks/pre-push");
    fs::write(
        &hook,
        format!(
            "#!/bin/sh\n: > '{}'\ni=0\nwhile [ ! -e '{}' ] && [ $i -lt 600 ]; do sleep 0.1; i=$((i+1)); done\nexit 1\n",
            started.display(),
            release.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    started
}

#[test]
fn v40_remote_failures_report_partial_local_effects_and_unknown_cancelled_push() {
    let (fixture, remote, local_head, _) = v1_local_branches_diverged_fixture();
    // The remote moves again after the fixture's last fetch.
    let other = fixture.root.join("other-clone");
    assert!(v1_gaps_git(
        &fixture.root,
        &[
            "clone",
            "-q",
            remote.to_str().unwrap(),
            other.to_str().unwrap()
        ]
    ));
    fs::write(other.join("newer.txt"), b"newer\n").unwrap();
    assert!(v1_gaps_git(&other, &["add", "newer.txt"]));
    assert!(v1_gaps_git(
        &other,
        &[
            "-c",
            "user.name=Other",
            "-c",
            "user.email=o@example.invalid",
            "commit",
            "-qm",
            "newer remote",
        ]
    ));
    assert!(v1_gaps_git(&other, &["push", "-q", "origin", "main"]));
    let newer = v1_remote_main(&remote);
    let tracking_before = fixture
        .git(&["rev-parse", "origin/main"])
        .trim()
        .to_string();
    assert_ne!(tracking_before, newer);

    // ff-only refuses after its fetch step: the tracking ref moved, local main did not.
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let error = prepare(
        LocalCommand::Pull,
        &context,
        &[
            ("remote", "origin"),
            ("branch", "main"),
            ("strategy", "ff-only"),
        ],
    )
    .execute(true, CancellationToken::default())
    .unwrap_err();
    assert!(error.starts_with("Pull refused"), "{error}");
    assert!(
        error.contains(&format!(
            "Local effects: tracking origin/main moved {} → {}.",
            &tracking_before[..12],
            &newer[..12]
        )),
        "{error}"
    );
    assert!(
        error.contains("Remote effect: none; fetch and pull only read the remote."),
        "{error}"
    );
    assert!(!error.contains("branch main moved"), "{error}");
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), local_head);
    assert_eq!(fixture.git(&["rev-parse", "origin/main"]).trim(), newer);
    reconcile(&fixture.repo(), CancellationToken::default()).unwrap();

    // A push cancelled while Git runs reports unchanged local refs and an UNKNOWN remote.
    let started = v40_blocking_pre_push(&fixture);
    fixture.git(&["branch", "publish", &local_head]);
    let refs = fixture.git(&["show-ref"]);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let push = prepare(
        LocalCommand::Push,
        &context,
        &[
            ("remote", "origin"),
            ("branch", "publish"),
            ("set_upstream", "false"),
        ],
    );
    let token = CancellationToken::default();
    let canceller = {
        let token = token.clone();
        let started = started.clone();
        std::thread::spawn(move || {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while !started.exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            token.cancel();
        })
    };
    let error = push.execute(true, token).unwrap_err();
    canceller.join().unwrap();
    assert!(started.exists(), "the push never reached its hook: {error}");
    assert!(
        error.starts_with("Cancelled: Git was stopped on request."),
        "{error}"
    );
    assert!(
        error.contains("Local effects: no branch, tag or tracking ref changed."),
        "{error}"
    );
    assert!(error.contains("Remote effect: UNKNOWN"), "{error}");
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert!(!v1_gaps_git(
        &remote,
        &["rev-parse", "--verify", "-q", "refs/heads/publish"]
    ));
}

#[test]
fn v39_detached_head_refuses_pull_and_switch_preview_names_unreferenced_commits() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file.txt", b"saved\n", "initial");
    fixture.git(&["switch", "--detach"]);
    fixture.commit_file("detached.txt", b"detached work\n", "detached work");
    let detached = fixture.git(&["rev-parse", "HEAD"]).trim().to_string();
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    assert!(LocalCommand::Pull
        .disabled_reason(&context)
        .unwrap()
        .starts_with("Detached HEAD: pull updates the current branch"));
    assert!(LocalCommand::Fetch.disabled_reason(&context).is_none());
    let switch = prepare(LocalCommand::SwitchBranch, &context, &[("name", "main")]);
    assert!(
        v1_has(
            &switch,
            "DETACHED HEAD: 1 commit here is on no branch or tag."
        ),
        "{:?}",
        switch.preview
    );
    assert!(v1_has(
        &switch,
        &format!("After switching away from {detached} only the reflog finds it.")
    ));
    execute(&prepare(
        LocalCommand::CreateBranch,
        &context,
        &[("name", "kept"), ("revision", "HEAD")],
    ));
    assert_eq!(fixture.git(&["rev-parse", "kept"]).trim(), detached);
    let context =
        LocalContext::from_workspace(&workspace(&fixture, RepositoryMode::Overview), None, None);
    let switch = prepare(LocalCommand::SwitchBranch, &context, &[("name", "main")]);
    assert!(
        v1_has(
            &switch,
            "DETACHED HEAD: every commit here is on a branch or tag."
        ),
        "{:?}",
        switch.preview
    );
    execute(&switch);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    assert_eq!(fixture.git(&["rev-parse", "kept"]).trim(), detached);
}
