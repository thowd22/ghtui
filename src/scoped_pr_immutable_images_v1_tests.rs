//! UNREGISTERED child of genuine scoped resource contracts. Thirteen declarations,
//! strict subprocess + SQLite + actual divergent Git objects, not proof builders.
use super::*;
#[path = "scoped_pr_inline_tests_v1.rs"]
mod inline_comments;
#[path = "scoped_pr_native_diff_tests_v1.rs"]
mod native_diff;
use crate::immutable_git_v1_test_support as history;
use crate::providers::gh::{PrImageSideV1, PrImmutableFileLimitsV1};
use crate::scoped_pr_observed_tabs_v1::resources::immutable_images::{self as images, ui};

fn bind_steps(steps: &mut [GhStep], history: &history::GitImagesV1) {
    for step in steps {
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            if value.get("head").is_some() && value.get("base").is_some() {
                history.bind_parent(&mut value);
                step.stdout = value.to_string().into_bytes();
            }
            if step
                .args
                .get(1)
                .is_some_and(|endpoint| endpoint.contains("/pulls/7/files"))
            {
                let value = if step.args.iter().any(|value| value == "--slurp") {
                    json!([[history.file()]])
                } else {
                    json!([history.file()])
                };
                step.stdout = value.to_string().into_bytes();
            }
        }
        if step.args.first().is_some_and(|value| value == "pr") {
            step.stdout = history.patch.clone();
        }
    }
}
fn raw_blob(sha: &str, bytes: &[u8]) -> GhStep {
    let mut step = GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/git/blobs/{sha}"),
            "--hostname",
            HOST,
            "--method",
            "GET",
            "--header",
            "Accept: application/vnd.github.raw+json",
            "--allow-escape-sequences",
        ],
        "",
    );
    step.stdout = bytes.to_vec();
    step
}
fn image_steps(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut steps = principal(42, "current-image-reader");
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    let mut parent = resource_parent(REPO);
    h.bind_parent(&mut parent);
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent.clone()));
    steps.push(get(
        &format!("repos/{REPO}/compare/{}...{}?per_page=1&page=1", h.b, h.h),
        h.compare(),
    ));
    steps.push(get(
        &format!("repos/{REPO}/git/commits/{}", h.h),
        h.commit(true),
    ));
    steps.push(get(
        &format!("repos/{REPO}/git/commits/{}", h.m),
        h.commit(false),
    ));
    for (sha, value) in &h.old_trees {
        steps.push(get(&format!("repos/{REPO}/git/trees/{sha}"), value.clone()));
    }
    steps.push(raw_blob(&h.old_blob, &h.old_bytes));
    for (sha, value) in &h.new_trees {
        steps.push(get(&format!("repos/{REPO}/git/trees/{sha}"), value.clone()));
    }
    steps.push(raw_blob(&h.new_blob, &h.new_bytes));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.extend(principal(42, "reader-renamed-same-uid"));
    assert_eq!(steps.len(), 17);
    steps
}
struct Setup {
    fixture: Fixture,
    h: history::GitImagesV1,
    steps: Vec<GhStep>,
    source: Arc<CatalogueIntentSource>,
    active: Arc<ActiveRemoteScopeV1>,
    // The real Files allocation precedes image-input consumption. Retaining
    // this observation does not restore the selected page or confer permission.
    original_files: Arc<ScopedPrResourceViewV1>,
    workspace: GithubWorkspace,
    sql_before: Vec<String>,
    git_before: (String, String, Vec<u8>, Vec<u8>),
}
fn setup(sha256: bool, alter: impl FnOnce(&mut Vec<GhStep>)) -> Setup {
    setup_with_bytes(sha256, history::OLD, history::NEW, alter)
}
fn setup_with_bytes(
    sha256: bool,
    old: &[u8],
    new: &[u8],
    alter: impl FnOnce(&mut Vec<GhStep>),
) -> Setup {
    setup_with_history(
        |repo| history::GitImagesV1::create_with_bytes(repo, sha256, old, new),
        alter,
    )
}
fn setup_with_history(
    build: impl FnOnce(&std::path::Path) -> history::GitImagesV1,
    alter: impl FnOnce(&mut Vec<GhStep>),
) -> Setup {
    let fixture = Fixture::new();
    let h = build(&fixture.repo());
    let mut steps = script(&resource_steps(PrResourceKindV1::Files, REPO));
    bind_steps(&mut steps, &h);
    let mut read = image_steps(&h);
    alter(&mut read);
    steps.extend(read);
    fixture.scripted_gh(&steps);
    let (source, active, mut workspace) = opened(&fixture);
    let result = run_resource(
        &fixture,
        source.clone(),
        active.clone(),
        &mut workspace,
        PrResourceKindV1::Files,
        &Default::default(),
    )
    .unwrap();
    install_resource_if_current(&mut workspace, result).unwrap();
    let sql_before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    Setup {
        fixture,
        h,
        steps,
        source,
        active,
        original_files: workspace.scoped_pr_resource.as_ref().unwrap().clone(),
        workspace,
        sql_before,
        git_before,
    }
}
fn input(setup: &mut Setup) -> images::PrImmutableImageInputV1 {
    let view = setup.workspace.scoped_pr_resource.as_ref().unwrap();
    let row = (0..setup.workspace.pr.as_ref().unwrap().rows.len())
        .find(|row| matches!(view.row(*row), Some(PrResourceRowV1::FileList(0))))
        .unwrap();
    images::take_image_input(&mut setup.workspace, row).unwrap()
}
fn read(
    setup: &mut Setup,
    limits: &PrImmutableFileLimitsV1,
    cancel: &CancellationToken,
) -> Result<images::PrImmutableImageResultV1, PrResourceFailureV1> {
    let input = input(setup);
    images::read_images(
        setup.source.clone(),
        setup.active.clone(),
        input,
        &provider(&setup.fixture),
        &database(&setup.fixture),
        limits,
        &Default::default(),
        cancel,
    )
}
fn intact(setup: &Setup) {
    effects(
        &setup.fixture,
        &setup.sql_before,
        &setup.git_before,
        &setup.steps,
    );
    assert_eq!(
        fs::read(setup.fixture.repo().join("LOCAL_DIRTY_CANARY")).unwrap(),
        b"LOCAL_WORKTREE_UNTOUCHED\0\xff\n"
    );
}
fn image_frame(view: &mut ui::PrImageViewerV1) -> String {
    use ratatui::{backend::TestBackend, Terminal};
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| ui::draw(frame, frame.area(), view, App::default().theme()))
        .unwrap();
    let buffer = terminal.backend().buffer();
    (0..24)
        .map(|y| (0..80).map(|x| buffer[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
#[test]
fn immutable_images_genuine_present_empty_side_clears_other_cursor_range_and_visible_notice() {
    let mut s = setup_with_bytes(false, b"old\n", b"", |_| {});
    assert_eq!((s.h.additions, s.h.deletions), (0, 1));
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    assert!(!result.proof.observed().image(PrImageSideV1::New).absent());
    assert_eq!(
        result.proof.observed().image(PrImageSideV1::New).bytes(),
        Some(b"".as_slice())
    );
    let mut view = result.viewer.unwrap();
    view.apply(ui::PrImageUiActionV1::Range);
    assert!(image_frame(&mut view).contains("READ-ONLY LEFT 1–1"));
    view.apply(ui::PrImageUiActionV1::New);
    assert_eq!(view.selected(), (PrImageSideV1::New, 0, 0));
    assert!(view.canonical_range().is_none());
    let frame = image_frame(&mut view);
    assert!(frame.contains("NEW image has no selectable text lines"));
    assert!(!frame.contains("READ-ONLY LEFT"));
    view.apply(ui::PrImageUiActionV1::Range);
    assert!(view.canonical_range().is_none());
    assert!(!image_frame(&mut view).contains("READ-ONLY LEFT"));
    view.apply(ui::PrImageUiActionV1::Old);
    assert_eq!(view.selected(), (PrImageSideV1::Old, 1, 1));
    assert!(image_frame(&mut view).contains("Selection LEFT 1–1"));
    view.apply(ui::PrImageUiActionV1::Range);
    assert_eq!(view.canonical_range().unwrap().review_head(), s.h.h);
    intact(&s);
}
#[test]
fn immutable_images_genuine_divergence_crlf_no_newline_exact_47_and_readonly_left_head() {
    let mut s = setup(false, |_| {});
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    assert_eq!(result.proof.observed().merge_base(), s.h.m);
    assert_ne!(s.h.m, s.h.b);
    assert_eq!(
        result.proof.observed().image(PrImageSideV1::Old).bytes(),
        Some(history::OLD)
    );
    assert_eq!(
        result.proof.observed().image(PrImageSideV1::New).bytes(),
        Some(history::NEW)
    );
    assert_eq!(result.accounting.children_started(), 11);
    assert!(result.accounting.sealed());
    assert!(!result.display_limited);
    let left = result
        .proof
        .canonical_range(PrImageSideV1::Old, 1, 2)
        .unwrap();
    assert_eq!(left.review_head(), s.h.h);
    assert_eq!(left.merge_base(), s.h.m);
    assert_eq!(left.path(), history::PATH.as_bytes());
    assert!(result
        .proof
        .canonical_range(PrImageSideV1::New, 1, 3)
        .is_err());
    assert_eq!(s.steps.len(), 47);
    intact(&s);
}
#[test]
fn immutable_images_actual_sha256_trees_and_blobs_not_json_commit_hash_claim() {
    let mut s = setup(true, |_| {});
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    assert_eq!(s.h.h.len(), 64);
    assert_eq!(result.proof.observed().head_tree(), s.h.th);
    assert!(result.proof.observed().patch_associated());
    intact(&s);
}
#[test]
fn immutable_images_compare_base_contradiction_and_short_mergebase_refuse() {
    for short in [false, true] {
        let mut s = setup(false, |steps| {
            let mut raw: Value = serde_json::from_slice(&steps[4].stdout).unwrap();
            if short {
                raw["merge_base_commit"]["sha"] = json!("a".repeat(7));
            } else {
                raw["base_commit"]["sha"] = json!("c".repeat(40));
            }
            steps[4].stdout = raw.to_string().into_bytes();
            steps.truncate(5);
        });
        assert!(read(&mut s, &Default::default(), &Default::default()).is_err());
        intact(&s);
    }
}
#[test]
fn immutable_images_complete_tree_required_truncated_omitted_or_hash_mismatch_not_absence() {
    for mode in 0..3 {
        let mut s = setup(false, |steps| {
            let mut value: Value = serde_json::from_slice(&steps[7].stdout).unwrap();
            match mode {
                0 => value["truncated"] = json!(true),
                1 => {
                    value.as_object_mut().unwrap().remove("truncated");
                }
                _ => value["tree"] = json!([]),
            }
            steps[7].stdout = value.to_string().into_bytes();
            steps.truncate(8);
        });
        assert!(read(&mut s, &Default::default(), &Default::default()).is_err());
        intact(&s);
    }
}
#[test]
fn immutable_images_failed_tree_403_huge_multibyte_has_no_absence_or_next_child() {
    let mut s = setup(false, |steps| {
        steps[7].status = 1;
        steps[7].stdout = "界".repeat(350000).into_bytes();
        steps[7].stderr = b"gh: forbidden (HTTP 403)".to_vec();
        steps.truncate(8);
    });
    assert!(read(&mut s, &Default::default(), &Default::default()).is_err());
    intact(&s);
}
#[test]
fn immutable_images_actual_blob_digest_and_declared_size_contradiction_refuse() {
    for size in [false, true] {
        let mut s = setup(false, |steps| {
            if size {
                let mut raw: Value = serde_json::from_slice(&steps[8].stdout).unwrap();
                raw["tree"][0]["size"] = json!(900000);
                steps[8].stdout = raw.to_string().into_bytes();
                steps.truncate(9);
            } else {
                steps[9].stdout = b"wrong content".to_vec();
                steps.truncate(10);
            }
        });
        assert!(read(&mut s, &Default::default(), &Default::default()).is_err());
        intact(&s);
    }
}
#[test]
fn immutable_images_closing_full_parent_head_or_repo_replacement_refuse() {
    for parent in [false, true] {
        let mut s = setup(false, |steps| {
            let index = if parent { 13 } else { 14 };
            let mut raw: Value = serde_json::from_slice(&steps[index].stdout).unwrap();
            if parent {
                raw["head"]["sha"] = json!("f".repeat(40));
            } else {
                raw["id"] = json!(999);
                raw["node_id"] = json!("R_999");
            }
            steps[index].stdout = raw.to_string().into_bytes();
            steps.truncate(index + 1);
        });
        assert!(read(&mut s, &Default::default(), &Default::default()).is_err());
        intact(&s);
    }
}
#[test]
fn immutable_images_final_same_login_uid43_drops_all_prepared_private_bytes() {
    let mut s = setup(false, |steps| {
        steps[16] = get("user", json!({"id":43,"login":"current-image-reader"}));
    });
    let error = read(&mut s, &Default::default(), &Default::default()).unwrap_err();
    assert!(!error.to_string().contains("old-no-newline"));
    assert!(!sql_state(&s.fixture)
        .iter()
        .any(|row| row.contains("user-id:43")));
    intact(&s);
}
#[test]
fn immutable_images_sql_epoch_hook_after_closing_repo_before_principal_and_no_final_sql() {
    let mut s = setup(false, |steps| {
        steps.truncate(15);
    });
    let input = input(&mut s);
    let mut phases = vec![];
    let result = images::read_images_with_guard(
        s.source.clone(),
        s.active.clone(),
        input,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        |phase| {
            let count = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
                .unwrap()
                .lines()
                .count();
            phases.push((phase, count));
            if phase == PrResourcePhaseV1::BeforeClosingAccount {
                return Err(GithubError {
                    kind: GithubErrorKind::IdentityChanged,
                    message: "PRIVATE_SQL_CANARY".into(),
                });
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    assert_eq!(
        phases,
        vec![
            (PrResourcePhaseV1::AfterOpeningSource, 34),
            (PrResourcePhaseV1::BeforeClosingAccount, 45)
        ]
    );
    assert!(!result
        .unwrap_err()
        .to_string()
        .contains("PRIVATE_SQL_CANARY"));
    intact(&s);
}
#[test]
fn immutable_images_cancel_original_deadline_and_pool_are_not_new_per_child_clocks() {
    let mut s = setup(false, |steps| {
        steps.truncate(0);
    });
    let cancelled = CancellationToken::default();
    cancelled.cancel();
    assert!(read(&mut s, &Default::default(), &cancelled).is_err());
    intact(&s);
    let mut s = setup(false, |steps| {
        steps.truncate(4);
    });
    let limits = PrImmutableFileLimitsV1 {
        max_pool_bytes: 300000,
        ..Default::default()
    };
    assert!(read(&mut s, &limits, &Default::default()).is_err());
    intact(&s);
    let mut s = setup(false, |steps| {
        steps.truncate(4);
    });
    let input = input(&mut s);
    let mut access = RepositoryCatalogueAccessLimits::default();
    access.transport.max_duration = Duration::from_secs(1);
    let result = images::read_images_with_guard(
        s.source.clone(),
        s.active.clone(),
        input,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &access,
        &Default::default(),
        |phase| {
            assert_eq!(phase, PrResourcePhaseV1::AfterOpeningSource);
            std::thread::sleep(Duration::from_millis(1050));
            Ok(())
        },
    );
    assert!(result.is_err());
    intact(&s);
}
#[test]
fn immutable_images_canonical_range_view_keyboard_mouse_resize_all_controls_no_extra_commands() {
    use crossterm::event::{
        KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    use ratatui::{backend::TestBackend, Terminal};
    let mut s = setup(false, |_| {});
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let mut view = result.viewer.unwrap();
    let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            ui::draw(frame, area, &mut view, App::default().theme());
        })
        .unwrap();
    view.apply(ui::key(&view, KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT)).unwrap());
    view.apply(ui::PrImageUiActionV1::Range);
    assert_eq!(view.canonical_range().unwrap().end(), 2);
    terminal
        .draw(|frame| {
            let area = frame.area();
            ui::draw(frame, area, &mut view, App::default().theme());
        })
        .unwrap();
    let mouse = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 65,
        row: 9,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(
        ui::mouse(&view, mouse),
        Some(ui::PrImageUiActionV1::Point {
            side: PrImageSideV1::New,
            line: 2,
            extend: false
        })
    );
    view.apply(ui::mouse(&view, mouse).unwrap());
    assert_eq!(view.selected(), (PrImageSideV1::New, 2, 2));
    assert!(view.canonical_range().is_none());
    let frame = image_frame(&mut view);
    assert!(frame.contains("Selection RIGHT 2–2"));
    assert!(!frame.contains("READ-ONLY LEFT"));
    terminal
        .draw(|frame| {
            ui::draw(frame, frame.area(), &mut view, App::default().theme());
        })
        .unwrap();
    let first = MouseEvent { row: 8, ..mouse };
    view.apply(ui::mouse(&view, first).unwrap());
    let drag = MouseEvent {
        kind: MouseEventKind::Drag(MouseButton::Left),
        ..mouse
    };
    view.apply(ui::mouse(&view, drag).unwrap());
    assert_eq!(view.selected(), (PrImageSideV1::New, 1, 2));
    view.apply(ui::PrImageUiActionV1::Range);
    assert_eq!(view.canonical_range().unwrap().side(), PrImageSideV1::New);
    view.clear_layout();
    assert!(ui::mouse(&view, mouse).is_none());
    for key in ['O', 'N', 'A', 'L', 'C', 'R', '<', '>', 'b', 'q'] {
        assert!(ui::key(&view, KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)).is_some());
    }
    assert_eq!(
        ui::key(&view, KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
        Some(ui::PrImageUiActionV1::Horizontal(8))
    );
    assert_eq!(
        view.apply(ui::PrImageUiActionV1::Close),
        Some(ui::PrImageUiActionV1::Close)
    );
    intact(&s);
}
#[test]
fn immutable_images_owner_supersession_and_pure_close_back_cannot_reopen_private_view() {
    let mut s = setup(false, |_| {});
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let mut app = App {
        remote_scope_source: Some(s.source.clone()),
        remote_scope: Some(s.active.clone()),
        github: Some(s.workspace),
        ..Default::default()
    };
    let (view, limited) = images::install_images(&mut app, result).unwrap();
    assert!(!limited);
    app.pr_file_images = view;
    app.overlay = Some(crate::app::Overlay::PrFileImages);
    assert!(app.pr_file_images.as_ref().unwrap().owns(&app));
    app.remote_scope = Some(Arc::new(
        ActiveRemoteScopeV1::explicit_selection(s.source.source(), &[410]).unwrap(),
    ));
    assert!(images::native::retire_invalid(&mut app));
    assert!(app.pr_file_images.is_none());
    assert!(app.overlay.is_none());
    assert_eq!(sql_state(&s.fixture), s.sql_before);
    exact(&s.fixture, &s.steps);
    assert_eq!(git_state_resource(&s.fixture), s.git_before);
}
#[test]
fn immutable_images_actual_factory_display_row_bound_preserves_existing_files_view_focus() {
    let mut old = b"\n".repeat(40000);
    old.extend_from_slice(b"old-no-newline");
    let mut new = b"\n".repeat(40000);
    new.extend_from_slice(b"new-no-newline");
    let mut s = setup_with_bytes(false, &old, &new, |_| {});
    let previous = s
        .workspace
        .pr
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .map(|row| row.label.clone())
        .collect::<Vec<_>>();
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    assert!(result.display_limited);
    assert!(result.viewer.is_none());
    assert!(result.proof.observed().patch_associated());
    let mut app = App {
        github: Some(s.workspace),
        remote_scope: Some(s.active.clone()),
        remote_scope_source: Some(s.source.clone()),
        inbox_list: crate::ui_table::ListState {
            selected: 2,
            top: 1,
        },
        focus: 3,
        ..Default::default()
    };
    assert!(images::native::install(&mut app, result).unwrap());
    assert_eq!(
        (app.inbox_list.selected, app.inbox_list.top, app.focus),
        (2, 1, 3)
    );
    assert!(app.pr_file_images.is_none());
    assert_eq!(
        app.github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .rows
            .iter()
            .map(|row| row.label.clone())
            .collect::<Vec<_>>(),
        previous
    );
    assert_eq!(sql_state(&s.fixture), s.sql_before);
    assert_eq!(git_state_resource(&s.fixture), s.git_before);
    exact(&s.fixture, &s.steps);
}
#[test]
fn v1_pr_threads_line_comment_focuses_diff_line_and_p_verifies_extended_range() {
    use images::line_comment_v19::{apply, LineCommentTargetV19};
    use ui::PrImageUiActionV1 as A;
    let mut s = setup(false, |_| {});
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let mut view = result.viewer.unwrap();
    let path = result
        .proof
        .observed()
        .image(PrImageSideV1::New)
        .path()
        .to_vec();
    let other = LineCommentTargetV19 {
        path: b"another/file.rs".to_vec(),
        side: PrImageSideV1::New,
        line: 2,
    };
    assert!(
        !apply(&mut view, &other),
        "A different file is never guessed"
    );
    let beyond = LineCommentTargetV19 {
        path: path.clone(),
        side: PrImageSideV1::New,
        line: 100_000,
    };
    assert!(!apply(&mut view, &beyond));
    let target = LineCommentTargetV19 {
        path,
        side: PrImageSideV1::New,
        line: 2,
    };
    assert!(apply(&mut view, &target));
    assert_eq!(view.selected(), (PrImageSideV1::New, 2, 2));
    assert_eq!(view.visible, ui::ImageVisibleSideV1::New);
    assert_eq!(view.canonical_range().unwrap().end(), 2);
    let frame = image_frame(&mut view);
    assert!(frame.contains("P Comment on RIGHT line 2"), "{frame}");
    // Extending the selection drops the verified range; P alone re-verifies it.
    view.apply(A::Move {
        delta: -1,
        extend: true,
    });
    assert!(view.canonical_range().is_none());
    assert_eq!(view.apply(A::Comment), Some(A::Comment));
    let range = view.canonical_range().unwrap();
    assert_eq!((range.side(), range.end()), (PrImageSideV1::New, 2));
    assert_eq!(view.selected(), (PrImageSideV1::New, 1, 2));
    intact(&s);
}
