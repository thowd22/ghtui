use super::*;
use crate::{
    forms::{Form, FormPurpose},
    git::{GitRepository, Operation, OriginalLocalActorOriginV1},
    local_actions::LocalCommand,
    local_commit_actor_origin_v1_test_support::{git, text},
    workspace::{RepositoryMode, Workspace},
};
use std::{fs, path::PathBuf};
pub(crate) struct Real {
    pub directory: tempfile::TempDir,
    pub root: PathBuf,
    pub app: App,
    pub sha: String,
}
impl Real {
    pub fn new() -> Self {
        Self::with_native_mutable(false)
    }
    fn with_native_mutable(native: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("original");
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q", "--initial-branch=main"], None);
        git(&root, &["config", "user.name", "Actual Local Actor"], None);
        git(
            &root,
            &["config", "user.email", "local@example.invalid"],
            None,
        );
        fs::write(root.join("conflict"), b"base\n").unwrap();
        fs::write(root.join("independent"), b"saved independent file\n").unwrap();
        git(&root, &["add", "--", "conflict"], None);
        git(&root, &["add", "--", "independent"], None);
        git(&root, &["commit", "-qm", "base"], None);
        fs::write(root.join("conflict"), b"stashed\n").unwrap();
        git(&root, &["stash", "push", "-m", "private"], None);
        let sha = text(&root, &["rev-parse", "refs/stash"]);
        fs::write(root.join("conflict"), b"ours\n").unwrap();
        git(&root, &["commit", "-qam", "ours"], None);
        if native {
            fs::write(root.join("independent"), b"unsaved independent file\n").unwrap();
        }
        let workspace = Workspace::load(
            &root,
            if native {
                RepositoryMode::Working
            } else {
                RepositoryMode::Overview
            },
            "HEAD",
            None,
        )
        .unwrap()
        .unwrap();
        // Obtain actual sealed mutable rows BEFORE the conflicting stash write;
        // never synthesize mutable authority from the later unmerged index.
        let native_state = native.then(|| {
            let mut state =
                crate::repository_native_diff_v1::NativeDiffStateV1::open_mutable(&workspace, 118)
                    .unwrap();
            let owner = crate::repository_diff_sides_worker_v1::OriginalDiffWorkerOwnerV1::begin(
                &workspace,
                None,
                Arc::new(
                    crate::repository_diff_sides_worker_v1::OriginalDiffWorkV1::MutableNative(
                        state.request().unwrap(),
                    ),
                ),
                Default::default(),
            )
            .unwrap();
            owner
                .prepare()
                .unwrap()
                .publish_native_mutable_v2(&workspace, &mut state)
                .unwrap();
            assert!(state.current_frame().is_some());
            state
        });
        let origin = workspace.actor_origin.clone().unwrap();
        let repository = GitRepository::discover(&root).unwrap().unwrap();
        let plan = repository
            .plan(Operation::StashApply {
                id: sha.clone(),
                pop: true,
            })
            .unwrap();
        let failed = repository
            .execute_stash_with_inspection_v1(&plan, true, origin.clone())
            .unwrap_err()
            .failed_intent()
            .unwrap()
            .clone();
        let mut app = App {
            view: View::Repository,
            workspace: Some(workspace),
            native_mutable_diff_v2: native_state,
            overlay: Some(crate::app::Overlay::Form),
            refresh_requested: false,
            ..Default::default()
        };
        let mut form = Form::local(LocalCommand::StashPop, app.local_context().unwrap()).unwrap();
        let FormPurpose::Local(intent) = &mut form.purpose else {
            panic!()
        };
        intent.original_stash_source_v1 = Some(origin);
        intent.failed_stash_v1 = Some(failed);
        app.form = Some(form);
        Self {
            directory,
            root,
            app,
            sha,
        }
    }
    pub fn owner(&mut self) -> Arc<OriginalStashRecoveryOwnerV1> {
        dispatch(&mut self.app, StashRecoveryActionV1::Inspect);
        let intent = self.app.original_stash_recovery_request_v1.take().unwrap();
        OriginalStashRecoveryOwnerV1::begin(&mut self.app, intent, Default::default()).unwrap()
    }
}
#[test]
fn stash_recovery_conflict_handoff_retires_genuine_prewrite_mutable_rows_before_workspace_replace()
{
    let mut real = Real::with_native_mutable(true);
    assert!(real
        .app
        .native_mutable_diff_v2
        .as_ref()
        .unwrap()
        .current_frame()
        .is_some());
    let snapshot = || {
        (
            fs::read(real.root.join(".git/index")).unwrap(),
            fs::read(real.root.join(".git/config")).unwrap(),
            fs::read(real.root.join("conflict")).unwrap(),
            fs::read(real.root.join("independent")).unwrap(),
            text(&real.root, &["rev-parse", "HEAD", "refs/stash"]),
        )
    };
    let before = snapshot();
    dispatch(&mut real.app, StashRecoveryActionV1::Conflict);
    let intent = real.app.original_stash_recovery_request_v1.take().unwrap();
    let owner =
        OriginalStashRecoveryOwnerV1::begin(&mut real.app, intent, Default::default()).unwrap();
    let prepared = owner.prepare().unwrap();
    assert!(prepared.workspace.is_some());
    assert!(accept(&mut real.app, &owner, prepared));
    assert!(real.app.native_mutable_diff_v2.is_none());
    assert!(real.app.native_diff_v1.is_none());
    assert!(real.app.original_diff_work_v1.is_none());
    assert!(real.app.original_diff_cancel_v1);
    assert!(real.app.hits.is_empty());
    assert!(real.app.form.is_none() && real.app.original_stash_recovery_v1.is_none());
    assert_eq!(
        real.app.workspace.as_ref().unwrap().mode,
        RepositoryMode::Overview
    );
    assert_eq!(
        snapshot(),
        before,
        "Conflict inspection cannot alter original failed-stash effects"
    );
}
#[test]
fn stash_recovery_explicit_conflict_handoff_uses_original_read_operation_and_preserves_failed_index(
) {
    let mut real = Real::new();
    dispatch(&mut real.app, StashRecoveryActionV1::Conflict);
    let intent = real.app.original_stash_recovery_request_v1.take().unwrap();
    let owner =
        OriginalStashRecoveryOwnerV1::begin(&mut real.app, intent, Default::default()).unwrap();
    let prepared = owner.prepare().unwrap();
    assert!(prepared.workspace.is_some());
    assert!(accept(&mut real.app, &owner, prepared));
    assert_eq!(
        real.app.workspace.as_ref().unwrap().mode,
        RepositoryMode::Overview
    );
}
#[test]
fn stash_recovery_factory_requires_genuine_failed_form_workspace_origin_not_draft_label() {
    let mut real = Real::new();
    let owner = real.owner();
    assert!(owner.owns(&real.app));
    let workspace = real.app.workspace.as_mut().unwrap();
    let repo = GitRepository::discover(&real.root).unwrap().unwrap();
    workspace.actor_origin = Some(OriginalLocalActorOriginV1::capture(&repo).unwrap());
    assert!(!owner.owns(&real.app));
    let before = owner.operation.accounting().unwrap();
    assert_eq!(before.children_started(), 0);
    let mut empty = App::default();
    dispatch(&mut empty, StashRecoveryActionV1::Inspect);
    assert!(empty.original_stash_recovery_request_v1.is_none());
    assert!(empty.original_stash_recovery_v1.is_none());
}
#[test]
fn stash_recovery_terminal_contract_escapes_real_control_paths_and_all_controls_clickable_after_resize(
) {
    let mut real = Real::new();
    let repository = GitRepository::discover(&real.root).unwrap().unwrap();
    let stages = repository.conflict_stages().unwrap();
    let path = "control-\x1b[2J\n\t\\n-�";
    let mut entries = vec![];
    for stage in stages {
        entries.extend_from_slice(
            format!("{} {} {}\t{path}\0", stage.mode, stage.id, stage.stage).as_bytes(),
        );
    }
    git(
        &real.root,
        &["update-index", "-z", "--index-info"],
        Some(&entries),
    );
    let owner = real.owner();
    let report = owner.prepare().unwrap();
    assert!(report.lines.iter().any(|line| line.contains(&real.sha)));
    assert!(report
        .lines
        .iter()
        .any(|line| line.contains("control-\\u{1b}[2J\\n\\t\\\\n-�")));
    assert!(report.lines.iter().all(|line| !line.contains('\x1b')));
    assert!(accept(&mut real.app, &owner, report));
    for width in [40, 80, 120] {
        let backend = ratatui::backend::TestBackend::new(width, 24);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| {
                assert!(draw(frame, &mut real.app));
            })
            .unwrap();
        assert_eq!(real.app.hits.len(), 3);
        assert!(terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .all(|cell| !cell.symbol().contains('\x1b')));
        for hit in &real.app.hits {
            assert!(hit.rect.width > 0 && hit.rect.right() <= width);
            assert!(hit.rect.bottom() <= 24);
        }
        assert!(real
            .app
            .hits
            .iter()
            .any(|h| h.action == Action::StashRecoveryV1(StashRecoveryActionV1::Back)));
    }
}
#[test]
fn stash_recovery_source_loss_hides_report_before_fresh_original_read_and_keeps_same_checkout() {
    let mut real = Real::new();
    let owner = real.owner();
    let report = owner.prepare().unwrap();
    assert!(accept(&mut real.app, &owner, report));
    assert!(!real
        .app
        .original_stash_recovery_v1
        .as_ref()
        .unwrap()
        .lines
        .is_empty());
    fs::rename(&real.root, real.directory.path().join("kept")).unwrap();
    fs::create_dir(&real.root).unwrap();
    let owner = real.owner();
    assert!(real
        .app
        .original_stash_recovery_v1
        .as_ref()
        .unwrap()
        .lines
        .is_empty());
    assert!(owner.prepare().is_err());
    assert_eq!(owner.operation.accounting().unwrap().children_started(), 0);
    assert!(fail(&mut real.app, &owner));
    assert!(real
        .app
        .original_stash_recovery_v1
        .as_ref()
        .unwrap()
        .lines
        .is_empty());
    assert_eq!(real.app.workspace.as_ref().unwrap().directory, real.root);
}
