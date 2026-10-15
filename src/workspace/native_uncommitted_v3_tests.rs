use super::*;
use crate::{
    app::{Action, App, View},
    git::{MutableDiffIntentV2, MutableDiffKindV2, MutableDiffSourceV2},
    local_commit_actor_origin_v1_test_support::{git, text, Real},
    navigation_local_source_v1::{decode_mode, mode_number, OriginalSavedVisitV1},
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
    repository_native_diff_v1::NativeDiffStateV1,
    repository_native_mutable_plan_v2::NativeMutablePreviewRequestV2,
};
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};
use std::fs;

fn loaded(real: &Real, mode: RepositoryMode) -> Workspace {
    Workspace::load(&real.root, mode, "HEAD", None)
        .unwrap()
        .unwrap()
}
fn read(workspace: &Workspace) -> Arc<MutableDiffSourceV2> {
    let intent = MutableDiffIntentV2::capture(workspace).unwrap();
    let operation = LocalGitNavigationReadV1::begin(intent.origin(), Default::default()).unwrap();
    let source = MutableDiffSourceV2::read(intent, &operation).unwrap();
    operation.seal().unwrap();
    source
}
fn publish(state: &mut NativeDiffStateV1<MutableDiffSourceV2>) {
    let request = state.request().unwrap();
    let operation =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let prepared = request.prepare(&operation).unwrap();
    operation.seal().unwrap();
    assert!(state.accept(prepared));
}

#[test]
fn native_uncommitted_real_sha1_sha256_net_edits_include_staged_and_working_not_reverted_or_untracked(
) {
    for format in ["sha1", "sha256"] {
        let real = Real::new(format);
        for path in ["stage-only", "work-only", "reverted"] {
            fs::write(real.root.join(path), format!("saved {path}\n")).unwrap();
        }
        git(&real.root, &["add", "-A"], None);
        git(&real.root, &["commit", "-qm", "all tracked baseline"], None);
        fs::write(real.root.join("stage-only"), b"staged-only actual bytes\n").unwrap();
        fs::write(real.root.join("reverted"), b"intermediate staged bytes\n").unwrap();
        git(&real.root, &["add", "--", "stage-only", "reverted"], None);
        fs::write(real.root.join("work-only"), b"unstaged-only actual bytes\n").unwrap();
        fs::write(real.root.join("reverted"), b"saved reverted\n").unwrap();
        fs::write(real.root.join("untracked-only"), b"never part of diff\n").unwrap();
        let before = real.snapshot();
        let workspace = loaded(&real, RepositoryMode::Uncommitted);
        let source = read(&workspace);
        assert_eq!(source.intent().kind(), MutableDiffKindV2::Uncommitted);
        assert_eq!(workspace.mode, RepositoryMode::Uncommitted);
        assert_eq!(source.result().comparison, Comparison::Uncommitted);
        let paths = source
            .members()
            .iter()
            .map(|member| {
                member
                    .path(crate::repository_view::DiffSide::New)
                    .unwrap()
                    .0
                    .clone()
            })
            .collect::<Vec<_>>();
        assert_eq!(paths, [b"stage-only".to_vec(), b"work-only".to_vec()]);
        let patch = std::str::from_utf8(&source.result().patch).unwrap();
        assert!(patch.contains("+staged-only actual bytes"));
        assert!(patch.contains("+unstaged-only actual bytes"));
        assert!(!patch.contains("intermediate staged bytes"));
        assert!(!patch.contains("never part of diff"));
        assert_eq!(
            source.result().base.as_deref(),
            Some(text(&real.root, &["rev-parse", "HEAD"]).as_str())
        );
        assert!(source.result().head.is_none());
        assert_eq!(real.snapshot(), before);
    }
}

#[test]
fn native_uncommitted_exact_mode_kind_capture_rejects_working_staged_relabels_without_git() {
    let real = Real::new("sha256");
    let workspace = loaded(&real, RepositoryMode::Uncommitted);
    real.clear_trace();
    let intent = MutableDiffIntentV2::capture(&workspace).unwrap();
    for mode in [
        RepositoryMode::Working,
        RepositoryMode::Staged,
        RepositoryMode::Graph,
    ] {
        let mut wrong = workspace.clone();
        wrong.mode = mode;
        assert!(MutableDiffIntentV2::capture(&wrong).is_err());
        assert!(!intent.owns(&wrong));
    }
    assert!(real.children().is_empty());
}

#[test]
fn native_uncommitted_stable_mode_tags_preserve_old_seven_and_append_only_seven() {
    let expected = [
        RepositoryMode::Overview,
        RepositoryMode::Graph,
        RepositoryMode::Tree,
        RepositoryMode::Working,
        RepositoryMode::Staged,
        RepositoryMode::Branch,
        RepositoryMode::CommitChanges,
        RepositoryMode::Uncommitted,
    ];
    for (tag, mode) in expected.into_iter().enumerate() {
        assert_eq!(mode_number(mode), tag as u8);
        assert_eq!(decode_mode(tag as u8).unwrap(), mode);
        assert_eq!(RepositoryMode::ALL[tag].0, mode);
    }
    assert!(decode_mode(8).is_err());
    assert_eq!(RepositoryMode::ALL[7].2, 'e');
    assert_ne!(RepositoryMode::ALL[7].2, 'a');
}

#[test]
fn native_uncommitted_original_saved_navigation_restores_exact_mode_fingerprint_and_original_source(
) {
    let real = Real::new("sha1");
    let workspace = loaded(&real, RepositoryMode::Uncommitted);
    let visit = OriginalSavedVisitV1::capture(&workspace).unwrap();
    assert_eq!(visit.label(), "Repository • All tracked edits");
    let before = real.snapshot();
    real.clear_trace();
    let operation = LocalGitNavigationReadV1::begin(visit.origin(), Default::default()).unwrap();
    let restored = visit.restore(&operation).unwrap();
    operation.seal().unwrap();
    assert_eq!(restored.mode, RepositoryMode::Uncommitted);
    assert_eq!(
        restored.comparison.as_ref().unwrap().comparison,
        Comparison::Uncommitted
    );
    assert_eq!(
        restored.comparison.as_ref().unwrap().observed,
        workspace.comparison.as_ref().unwrap().observed
    );
    assert!(Arc::ptr_eq(
        restored.actor_origin.as_ref().unwrap(),
        workspace.actor_origin.as_ref().unwrap()
    ));
    assert!(!real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_uncommitted_original_navigation_refuses_new_working_bytes_not_relabeling_old_diff() {
    let real = Real::new("sha256");
    let workspace = loaded(&real, RepositoryMode::Uncommitted);
    let visit = OriginalSavedVisitV1::capture(&workspace).unwrap();
    fs::write(real.root.join("tracked"), b"external changed source\n").unwrap();
    let before = real.snapshot();
    let operation = LocalGitNavigationReadV1::begin(visit.origin(), Default::default()).unwrap();
    let error = visit.restore(&operation).unwrap_err();
    assert!(error.contains("Original mutable files/index state changed"));
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_uncommitted_selected_original_hunk_never_grants_native_preview_or_write_direction() {
    let real = Real::new("sha256");
    let workspace = loaded(&real, RepositoryMode::Uncommitted);
    let mut state = NativeDiffStateV1::open_mutable(&workspace, 118).unwrap();
    publish(&mut state);
    state.select_hunk(0).unwrap();
    publish(&mut state);
    assert!(!state.range().unwrap().changed().is_empty());
    let before = real.snapshot();
    real.clear_trace();
    let error = NativeMutablePreviewRequestV2::capture(&state, &workspace).unwrap_err();
    assert!(error.contains("no inferred index direction"));
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        native_mutable_diff_v2: Some(state),
        ..Default::default()
    };
    crate::repository_native_mutable_plan_native_v2::begin(&mut app);
    assert!(app.native_mutable_plan_v2.is_none());
    assert!(app.original_diff_work_v1.is_none());
    assert!(app
        .native_mutable_diff_v2
        .as_ref()
        .unwrap()
        .status
        .contains("no inferred index direction"));
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_uncommitted_source_metadata_is_explicit_tracked_baseline_and_mutable_unknown_actor() {
    let real = Real::new("sha1");
    let workspace = loaded(&real, RepositoryMode::Uncommitted);
    let source = read(&workspace);
    real.clear_trace();
    let identity = crate::repository_source_identity::PreparedRepositorySourceIdentity::prepare_native_mutable_v2(&workspace.status, &source).unwrap();
    assert!(identity
        .compact()
        .iter()
        .any(|row| row.text.contains("working files vs HEAD; mutable")));
    assert!(identity
        .detail_text()
        .contains("Uncommitted: all tracked edits since saved HEAD"));
    assert!(identity
        .detail_text()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .contains("Untracked files are separate and not included"));
    use crate::repository_native_diff_content_v2::NativeDiffPurposeV2;
    let (_, _, author, committer) = source.actor_labels();
    assert!(author.contains("author unknown"));
    assert!(committer.contains("committer unknown"));
    assert!(real.children().is_empty());
}

#[test]
fn native_uncommitted_repository_toolbar_and_palette_same_keyboard_mouse_exact_mode_no_io() {
    let real = Real::new("sha256");
    let workspace = loaded(&real, RepositoryMode::Working);
    let before = real.snapshot();
    real.clear_trace();
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace.clone()),
        ..Default::default()
    };
    let mut terminal = Terminal::new(TestBackend::new(120, 24)).unwrap();
    terminal
        .draw(|frame| crate::repository_ui::draw(frame, &mut app, Rect::new(1, 3, 118, 18)))
        .unwrap();
    let hit = app
        .hits
        .iter()
        .find(|hit| hit.action == Action::RepositoryMode(RepositoryMode::Uncommitted))
        .unwrap()
        .clone();
    let actual = (hit.rect.x..hit.rect.right())
        .map(|x| terminal.backend().buffer()[(x, hit.rect.y)].symbol())
        .collect::<String>();
    assert_eq!(actual, "[e All edits]");
    let mut key = App {
        view: View::Repository,
        workspace: Some(workspace),
        ..Default::default()
    };
    key.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('e'),
        KeyModifiers::NONE,
    )));
    app.handle_event(Event::Mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: hit.rect.x,
        row: hit.rect.y,
        modifiers: KeyModifiers::NONE,
    }));
    assert_eq!(app.repository_mode, RepositoryMode::Uncommitted);
    assert_eq!(key.repository_mode, app.repository_mode);
    assert!(app.repository_load_requested && key.repository_load_requested);
    for width in [40, 80, 120] {
        let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
        app.hits.clear();
        terminal
            .draw(|frame| {
                crate::repository_ui::draw(frame, &mut app, Rect::new(1, 3, width - 2, 18))
            })
            .unwrap();
        for hit in app
            .hits
            .iter()
            .filter(|hit| hit.action == Action::RepositoryMode(RepositoryMode::Uncommitted))
        {
            let text = (hit.rect.x..hit.rect.right())
                .map(|x| terminal.backend().buffer()[(x, hit.rect.y)].symbol())
                .collect::<String>();
            assert!(matches!(text.as_str(), "[e Edits]" | "[e All edits]"));
        }
        let catalogue = crate::navigation_catalogue::PreparedCatalogue::prepare(&app, 42).unwrap();
        let item = catalogue
            .entries
            .iter()
            .find(|item| item.id == "repository.mode.7")
            .unwrap();
        assert_eq!(
            item.action,
            Action::RepositoryMode(RepositoryMode::Uncommitted)
        );
        assert!(item
            .label
            .contains("all tracked edits since saved HEAD (staged + unstaged)"));
    }
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
