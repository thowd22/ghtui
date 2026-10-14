mod support;
use ghtui::{
    git::RawPath,
    local_actions::{LocalCommand, LocalContext, LocalInventory, LocalPlan},
    local_picker::{choices, LocalPicker, LocalValue, PickerAction, PickerOutcome},
    providers::runner::CancellationToken,
    workspace::{RepositoryMode, Workspace},
};
use std::{collections::BTreeMap, fs, os::unix::ffi::OsStrExt, process::Command};
use support::Fixture;

fn context(fixture: &Fixture) -> LocalContext {
    LocalContext::from_workspace(
        &Workspace::load(&fixture.repo(), RepositoryMode::Overview, "HEAD", None)
            .unwrap()
            .unwrap(),
        None,
        None,
    )
}

#[test]
fn local_picker_availability_is_pure_and_distinguishes_existing_targets_from_new_values() {
    use ghtui::local_picker::field_supported;
    assert!(field_supported(LocalCommand::SwitchBranch, "name"));
    assert!(field_supported(LocalCommand::RecoverBranch, "revision"));
    assert!(field_supported(LocalCommand::RemoveWorktree, "path"));
    assert!(field_supported(LocalCommand::StashApply, "id"));
    assert!(field_supported(LocalCommand::Push, "remote"));
    assert!(!field_supported(LocalCommand::CreateBranch, "name"));
    assert!(!field_supported(LocalCommand::AddWorktree, "path"));
    assert!(!field_supported(LocalCommand::Commit, "message"));
    assert!(!field_supported(LocalCommand::Commit, "revision"));
    assert!(LocalValue::Revision { id: "a".repeat(40) }
        .editor_text(LocalCommand::Commit, "revision")
        .is_err());
}

#[test]
fn local_picker_non_utf8_packed_ref_keeps_saved_id_but_disables_encoded_name_writes() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file", b"saved bytes\n", "saved");
    let head = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    let mut packed =
        format!("# pack-refs with: peeled fully-peeled sorted\n{head} refs/heads/raw-")
            .into_bytes();
    packed.extend_from_slice(b"\xff\n");
    fs::write(fixture.repo().join(".git/packed-refs"), &packed).unwrap();
    let inventory = LocalInventory::load(&fixture.repo(), CancellationToken::default()).unwrap();
    let entries = choices(LocalCommand::SwitchBranch, "name", &inventory);
    let raw = entries
        .iter()
        .find(|choice| choice.label.contains("non-UTF8 ref"))
        .unwrap();
    assert!(raw
        .disabled_reason
        .as_ref()
        .unwrap()
        .contains("display escapes are never passed to Git"));
    assert_eq!(raw.value, LocalValue::Revision { id: head.clone() });
    let mut chooser = LocalPicker::new(LocalCommand::SwitchBranch, "name", &inventory);
    chooser.dispatch(PickerAction::Insert("non-UTF8".into()));
    assert_eq!(chooser.dispatch(PickerAction::Choose), PickerOutcome::None);
    assert!(chooser
        .message
        .contains("non-UTF8 references are unavailable"));
    let compare = ghtui::comparison::picker(1, &inventory).unwrap();
    let selected = compare
        .options
        .iter()
        .find(|choice| choice.label.contains("non-UTF8 ref"))
        .unwrap();
    assert!(selected.disabled_reason.is_none());
    assert_eq!(selected.editor_text, head);
    assert!(matches!(selected.value, LocalValue::Revision { .. }));
    assert_eq!(
        fs::read(fixture.repo().join(".git/packed-refs")).unwrap(),
        packed
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]).trim(), head);
}

#[test]
fn branches_tags_local_picker_keeps_canonical_ref_identity_and_rejects_a_moved_target() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file", b"first\n", "first");
    let old = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.git(&["branch", "review"]);
    let inventory = LocalInventory::load(&fixture.repo(), CancellationToken::default()).unwrap();
    let choice = choices(LocalCommand::SwitchBranch, "name", &inventory)
        .into_iter()
        .find(|choice| choice.editor_text == "review")
        .unwrap();
    assert_eq!(
        choice.value,
        LocalValue::Reference {
            name: "refs/heads/review".into(),
            id: old.clone()
        }
    );
    let selected = BTreeMap::from([("name".into(), choice.value)]);
    let values = BTreeMap::from([("name".into(), choice.editor_text)]);
    let plan = LocalPlan::prepare_selected(
        LocalCommand::SwitchBranch,
        &context(&fixture),
        &values,
        &selected,
        CancellationToken::default(),
    )
    .unwrap();
    assert!(plan.execute(false, CancellationToken::default()).is_err());
    fixture.commit_file("file", b"second\n", "second");
    fixture.git(&["branch", "-f", "review", "HEAD"]);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let error = LocalPlan::prepare_selected(
        LocalCommand::SwitchBranch,
        &context(&fixture),
        &values,
        &selected,
        CancellationToken::default(),
    )
    .unwrap_err();
    assert!(error.contains("Selected reference changed"));
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(fixture.git(&["branch", "--show-current"]), "main\n");
    assert_eq!(fs::read(fixture.repo().join(".git/index")).unwrap(), index);
}

#[test]
fn stash_worktrees_local_picker_roundtrips_raw_paths_and_never_targets_a_lossy_sibling() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file", b"saved\n", "saved");
    fixture.git(&["branch", "linked"]);
    let linked = fixture.root.join(support::exact_worktree_component());
    let result = Command::new("git")
        .arg("-C")
        .arg(fixture.repo())
        .args(["worktree", "add", "--"])
        .arg(&linked)
        .arg("linked")
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let sibling = fixture.root.join("linked-�");
    fs::create_dir(&sibling).unwrap();
    fs::write(sibling.join("important"), b"not the selected worktree\n").unwrap();
    let inventory = LocalInventory::load(&fixture.repo(), CancellationToken::default()).unwrap();
    let choice = choices(LocalCommand::RemoveWorktree, "path", &inventory)
        .into_iter()
        .find(|choice| choice.value.worktree_path().as_ref() == Some(&linked))
        .unwrap();
    let serialized = serde_json::to_string(&choice.value).unwrap();
    let restored: LocalValue = serde_json::from_str(&serialized).unwrap();
    assert_eq!(
        restored.worktree_path().unwrap().as_os_str().as_bytes(),
        linked.as_os_str().as_bytes()
    );
    let selected = BTreeMap::from([("path".into(), restored)]);
    let values = BTreeMap::from([("path".into(), choice.editor_text)]);
    let plan = LocalPlan::prepare_selected(
        LocalCommand::RemoveWorktree,
        &context(&fixture),
        &values,
        &selected,
        CancellationToken::default(),
    )
    .unwrap();
    assert!(plan.execute(false, CancellationToken::default()).is_err());
    assert!(linked.exists());
    plan.execute(true, CancellationToken::default()).unwrap();
    assert!(!linked.exists());
    assert_eq!(
        fs::read(sibling.join("important")).unwrap(),
        b"not the selected worktree\n"
    );
}

#[test]
fn reflog_recovery_local_picker_uses_saved_sha_not_a_shifting_selector() {
    let fixture = Fixture::new();
    fixture.init_git();
    fixture.commit_file("file", b"old\n", "older state");
    let old = fixture.git(&["rev-parse", "HEAD"]).trim().to_owned();
    fixture.commit_file("file", b"new\n", "current state");
    let head = fixture.git(&["rev-parse", "HEAD"]);
    fs::write(fixture.repo().join("file"), b"dirty work\n").unwrap();
    let inventory = LocalInventory::load(&fixture.repo(), CancellationToken::default()).unwrap();
    let choice = choices(LocalCommand::RecoverBranch, "revision", &inventory)
        .into_iter()
        .find(|choice| choice.label.contains("older state") && choice.editor_text == old)
        .unwrap();
    assert_eq!(choice.value, LocalValue::Revision { id: old.clone() });
    let values = BTreeMap::from([
        ("name".into(), "rescue-picked".into()),
        ("revision".into(), choice.editor_text),
    ]);
    let selected = BTreeMap::from([("revision".into(), choice.value)]);
    LocalPlan::prepare_selected(
        LocalCommand::RecoverBranch,
        &context(&fixture),
        &values,
        &selected,
        CancellationToken::default(),
    )
    .unwrap()
    .execute(true, CancellationToken::default())
    .unwrap();
    assert_eq!(
        fixture.git(&["rev-parse", "rescue-picked"]),
        format!("{old}\n")
    );
    assert_eq!(fixture.git(&["rev-parse", "HEAD"]), head);
    assert_eq!(
        fs::read(fixture.repo().join("file")).unwrap(),
        b"dirty work\n"
    );
}

#[test]
fn local_picker_fuzzy_filter_and_disabled_choice_preserve_typed_identity() {
    let inventory = LocalInventory {
        directory: "/tmp/selected-repo".into(),
        references: vec![],
        remotes: vec![],
        stashes: vec![],
        reflog: vec![],
        worktrees: vec![ghtui::git::Worktree {
            path: "/tmp/selected-repo".into(),
            head: Some("a".repeat(40)),
            branch: Some("refs/heads/main".into()),
            bare: false,
            locked: false,
            prunable: false,
        }],
    };
    let mut picker = LocalPicker::new(LocalCommand::RemoveWorktree, "path", &inventory);
    assert_eq!(
        picker.dispatch(PickerAction::Insert("srepo".into())),
        PickerOutcome::None
    );
    assert_eq!(picker.visible.len(), 1);
    assert_eq!(picker.dispatch(PickerAction::Choose), PickerOutcome::None);
    assert!(picker.message.contains("cannot be removed"));
    assert_eq!(
        picker.dispatch(PickerAction::Insert("does-not-exist".into())),
        PickerOutcome::None
    );
    assert!(picker.visible.is_empty());
    assert_eq!(picker.dispatch(PickerAction::Choose), PickerOutcome::None);
    assert_eq!(picker.dispatch(PickerAction::Clear), PickerOutcome::None);
    assert_eq!(picker.visible.len(), 1);
    assert_eq!(picker.dispatch(PickerAction::Close), PickerOutcome::Closed);
    let path = LocalValue::Worktree {
        path: RawPath::new(b"/tmp/raw\tpath\n".to_vec()),
        head: None,
        branch: None,
    };
    assert_eq!(
        path.editor_text(LocalCommand::RemoveWorktree, "path")
            .unwrap(),
        "/tmp/raw\\tpath\\n"
    );
    assert!(path.editor_text(LocalCommand::Commit, "path").is_err());
}
