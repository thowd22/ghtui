//! Source-only declarations: real Git/proof/strict gh/SQL and actual SAME
//! Background worker. No invented rows/Images state or direct-handler PTY proof.
use super::*;
use crate::{
    app::{App, Overlay, View},
    repository_native_diff_projection_v1::NativeDiffLineTicketV1,
    repository_native_diff_v1::NativeDiffOptionChangeV1,
    repository_view::DiffSide,
    runtime::{scoped_inline_test_support_v1 as channel, Background},
    scoped_pr_native_diff_native_v1 as prnative,
    scoped_pr_native_diff_source_v1::ScopedPrNativeSourceV1,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use images::inline_v1::{native_entry_v1 as entry, native_v1 as inline, ui_v1 as composer};
const OLD_NATIVE: &[u8] = b"context before\nold one\nold two\ncontext after\n";
const NEW_NATIVE: &[u8] = b"context before\nnew one\nnew two\ncontext after\n";
#[path = "scoped_pr_native_threads_v3_tests.rs"]
mod threads_v3;
fn loaded(extra: impl FnOnce(&history::GitImagesV1, &mut Vec<GhStep>)) -> (Setup, App, Background) {
    load_setup(
        setup_with_bytes(false, OLD_NATIVE, NEW_NATIVE, |_| {}),
        extra,
    )
}
fn load_setup(
    mut s: Setup,
    extra: impl FnOnce(&history::GitImagesV1, &mut Vec<GhStep>),
) -> (Setup, App, Background) {
    extra(&s.h, &mut s.steps);
    s.fixture.scripted_gh(&s.steps);
    let images = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let mut app = App {
        view: View::Inbox,
        remote_scope_source: Some(s.source.clone()),
        remote_scope: Some(s.active.clone()),
        github: Some(s.workspace.clone()),
        ..Default::default()
    };
    app.original_tree_viewport_v2.width = 120;
    images::native::install(&mut app, images).unwrap();
    prnative::open_images(&mut app);
    let mut background = Background::new();
    project(&s, &mut app, &mut background);
    (s, app, background)
}
fn finish(background: &mut Background, app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while app.loading[View::Inbox.index()] {
        background.poll(app);
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
fn project(s: &Setup, app: &mut App, background: &mut Background) {
    let work = app.pr_native_diff_work_v1.take().unwrap();
    assert!(background.submit_pr_native_diff_v1(
        app,
        work,
        provider(&s.fixture),
        database(&s.fixture)
    ));
    finish(background, app);
    assert!(app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .current_frame()
        .is_some());
}
fn queue(s: &Setup, app: &mut App, background: &mut Background) {
    let request = app.pr_native_diff_v1.as_mut().unwrap().request().unwrap();
    prnative::queue_projection(app, request);
    project(s, app, background);
}
fn line(app: &App, side: DiffSide, number: u32) -> NativeDiffLineTicketV1<ScopedPrNativeSourceV1> {
    let frame = app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .current_frame()
        .unwrap();
    frame
        .rows()
        .iter()
        .enumerate()
        .find_map(|(row, cells)| {
            [false, true].into_iter().find_map(|right| {
                cells
                    .cell(right)
                    .and_then(|cell| cell.key())
                    .filter(|key| {
                        key.side() == side
                            && match side {
                                DiffSide::Old => key.canonical().old_line == Some(number),
                                DiffSide::New => key.canonical().new_line == Some(number),
                                DiffSide::Both => false,
                            }
                    })
                    .and_then(|_| frame.ticket(row, right))
            })
        })
        .unwrap()
}
fn select(s: &Setup, app: &mut App, background: &mut Background, side: DiffSide) {
    let state = app.pr_native_diff_v1.as_mut().unwrap();
    state.option(NativeDiffOptionChangeV1::Layout).unwrap();
    state.option(NativeDiffOptionChangeV1::Side(side)).unwrap();
    queue(s, app, background);
    let first = line(app, side, 2);
    let last = line(app, side, 3);
    app.pr_native_diff_v1
        .as_mut()
        .unwrap()
        .select_between(first, last)
        .unwrap();
    queue(s, app, background);
    assert_eq!(
        app.pr_native_diff_v1
            .as_ref()
            .unwrap()
            .range()
            .unwrap()
            .side(),
        side
    );
}
fn submit(s: &Setup, app: &mut App, background: &mut Background, writer: &DraftWriter) {
    let work = app
        .scoped_inline_work_v1
        .take()
        .expect("genuine captured inline work");
    assert!(background.submit_scoped_inline_v1(
        app,
        work,
        provider(&s.fixture),
        database(&s.fixture),
        writer.client()
    ));
}
fn open(s: &Setup, app: &mut App, background: &mut Background, writer: &DraftWriter) {
    entry::open(app);
    submit(s, app, background, writer);
    finish(background, app);
    assert_eq!(app.overlay, Some(Overlay::Form));
    assert!(!app.form.as_ref().unwrap().plan_ready());
}
fn close(app: &mut App) {
    app.dispatch(crate::app::Action::Form(crate::forms::FormAction::Close));
}
fn shape_steps(h: &history::GitImagesV1, side: &str) -> Vec<GhStep> {
    let mut steps = confirm_steps(h);
    for step in &mut steps {
        if !step.stdin.is_empty() {
            let mut v: Value = serde_json::from_slice(&step.stdin).unwrap();
            v["side"] = json!(side);
            v["start_side"] = json!(side);
            v["start_line"] = json!(2);
            v["line"] = json!(3);
            step.stdin = serde_json::to_vec(&v).unwrap();
        }
        if let Ok(mut v) = serde_json::from_slice::<Value>(&step.stdout) {
            if v.get("path").is_some() && v.get("commit_id").is_some() {
                v["side"] = json!(side);
                v["start_side"] = json!(side);
                v["start_line"] = json!(2);
                v["line"] = json!(3);
                step.stdout = serde_json::to_vec(&v).unwrap();
            }
        }
    }
    steps
}
#[test]
fn native_inline_comments_closed_left_right_multiline_keeps_actual_m_h_and_canonical_proof() {
    for side in [DiffSide::Old, DiffSide::New] {
        let (s, mut app, mut bg) = loaded(|_, _| {});
        select(&s, &mut app, &mut bg, side);
        let before = commands(&s.fixture).len();
        entry::open(&mut app);
        let work = app.scoped_inline_work_v1.as_ref().unwrap();
        let intent = work.intent();
        assert!(work.matches(&app));
        assert_eq!((intent.start(), intent.end()), (2, 3));
        assert_eq!(intent.review_head(), s.h.h);
        assert_eq!(intent.merge_base(), s.h.m);
        assert_ne!(s.h.m, s.h.b);
        assert_eq!(
            intent.side(),
            if side == DiffSide::Old {
                crate::providers::gh::DiffSide::Left
            } else {
                crate::providers::gh::DiffSide::Right
            }
        );
        assert_eq!(intent.selected_image_path(), history::PATH.as_bytes());
        assert!(entry::event(
            &mut app,
            &Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE))
        ));
        assert!(app.scoped_inline_work_v1.is_none());
        assert_eq!(commands(&s.fixture).len(), before);
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn native_inline_comments_actual_open_and_pure_form_back_preserve_exact_frame_range_query_and_viewport(
) {
    let (s, mut app, mut bg) = loaded(|h, steps| steps.extend(source_fence_steps(h, true)));
    select(&s, &mut app, &mut bg, DiffSide::Old);
    let writer = DraftWriter::new(database(&s.fixture));
    let state = app.pr_native_diff_v1.as_mut().unwrap();
    state.query.insert("old").unwrap();
    queue(&s, &mut app, &mut bg);
    let state = app.pr_native_diff_v1.as_ref().unwrap();
    let selected = state.capture_current_range_v2().unwrap();
    let frame = state.current_frame().unwrap().clone();
    let viewport = (state.selected, state.scroll, state.right, state.horizontal);
    open(&s, &mut app, &mut bg, &writer);
    let calls = commands(&s.fixture).len();
    composer::replace_editor_result(app.form.as_mut().unwrap(), "bounded edited draft body")
        .unwrap();
    close(&mut app);
    let state = app.pr_native_diff_v1.as_ref().unwrap();
    assert!(selected.matches(state));
    assert!(Arc::ptr_eq(&frame, state.current_frame().unwrap()));
    assert_eq!(
        (state.selected, state.scroll, state.right, state.horizontal),
        viewport
    );
    assert_eq!(state.query.text, "old");
    assert_eq!(commands(&s.fixture).len(), calls);
    assert!(app.scoped_inline_work_v1.is_none());
    assert!(!app.form.as_ref().unwrap().plan_ready());
    unchanged_git_and_exact_script(&s);
}
#[test]
fn native_inline_comments_form_resize_then_back_reprojects_same_source_and_range_not_inline_open() {
    let (s, mut app, mut bg) = loaded(|h, steps| steps.extend(source_fence_steps(h, true)));
    select(&s, &mut app, &mut bg, DiffSide::New);
    let source = app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .source()
        .unwrap()
        .clone();
    let range = app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .range()
        .unwrap()
        .clone();
    let writer = DraftWriter::new(database(&s.fixture));
    open(&s, &mut app, &mut bg, &writer);
    let calls = commands(&s.fixture).len();
    app.handle_event(Event::Resize(80, 24));
    close(&mut app);
    assert!(app.scoped_inline_work_v1.is_none());
    project(&s, &mut app, &mut bg);
    let state = app.pr_native_diff_v1.as_ref().unwrap();
    assert_eq!(state.width, 78);
    assert!(Arc::ptr_eq(state.source().unwrap(), &source));
    assert!(Arc::ptr_eq(state.range().unwrap(), &range));
    assert_eq!(commands(&s.fixture).len(), calls);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn native_inline_comments_full_file_position_cannot_mint_range_or_any_open_request() {
    let (s, mut app, mut bg) = loaded(|_, _| {});
    select(&s, &mut app, &mut bg, DiffSide::Old);
    let request = {
        let state = app.pr_native_diff_v1.as_mut().unwrap();
        state.open_full_side(DiffSide::New).unwrap();
        state.request().unwrap()
    };
    prnative::queue_projection(&mut app, request);
    let work = app.pr_native_diff_work_v1.take().unwrap();
    assert!(bg.submit_pr_native_diff_v1(
        &mut app,
        work,
        provider(&s.fixture),
        database(&s.fixture)
    ));
    finish(&mut bg, &mut app);
    assert!(app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .current_full_side()
        .is_some());
    let before = commands(&s.fixture).len();
    entry::open(&mut app);
    assert!(app.scoped_inline_work_v1.is_none());
    assert!(app.notice.contains("Full-file rows"));
    assert_eq!(commands(&s.fixture).len(), before);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn native_inline_comments_unified_both_hunk_is_disabled_without_side_conversion() {
    let (s, mut app, mut bg) = loaded(|_, _| {});
    app.pr_native_diff_v1
        .as_mut()
        .unwrap()
        .select_hunk(0)
        .unwrap();
    queue(&s, &mut app, &mut bg);
    entry::open(&mut app);
    assert!(app.scoped_inline_work_v1.is_none());
    assert!(app.notice.contains("no single review side"));
    assert_eq!(
        app.pr_native_diff_v1
            .as_ref()
            .unwrap()
            .range()
            .unwrap()
            .side(),
        DiffSide::Both
    );
    unchanged_git_and_exact_script(&s);
}
#[test]
fn native_inline_comments_pending_native_origin_paste_and_underlay_actions_are_captured_without_body_or_write(
) {
    let (s, mut app, mut bg) = loaded(|_, _| {});
    select(&s, &mut app, &mut bg, DiffSide::Old);
    entry::open(&mut app);
    let work = app.scoped_inline_work_v1.as_ref().unwrap().clone();
    for event in [
        Event::Paste("POISON_BODY".into()),
        Event::Key(KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE)),
        Event::Key(KeyEvent::new(KeyCode::Char('P'), KeyModifiers::NONE)),
    ] {
        app.handle_event(event);
        assert!(Arc::ptr_eq(
            app.scoped_inline_work_v1.as_ref().unwrap(),
            &work
        ));
        assert!(app.form.is_none());
    }
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    assert!(app.scoped_inline_work_v1.is_none());
    assert!(app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .current_frame()
        .is_some());
    unchanged_git_and_exact_script(&s);
}
#[test]
fn native_inline_comments_actual_pending_open_cancel_keeps_current_native_range_and_no_mutation() {
    let (s, mut app, mut bg) = loaded(|h, steps| steps.extend(source_fence_steps(h, true)));
    select(&s, &mut app, &mut bg, DiffSide::Old);
    let selected = app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .capture_current_range_v2()
        .unwrap();
    let writer = DraftWriter::new(database(&s.fixture));
    entry::open(&mut app);
    submit(&s, &mut app, &mut bg, &writer);
    channel::fault_next(&mut bg, 4);
    assert!(bg.cancel_scoped_inline_v1(&mut app));
    assert!(!bg.poll(&mut app));
    assert!(selected.matches(app.pr_native_diff_v1.as_ref().unwrap()));
    assert!(app.form.is_none());
    assert!(app.remote_scope.is_some() && app.github.is_some());
    unchanged_git_and_exact_script(&s);
}
#[test]
fn native_inline_comments_native_origin_queued_success_and_caught_panic_reject_newer_state_before_all_effects(
) {
    for panic in [false, true] {
        for mutation in 0..8 {
            let (s, mut app, mut bg) = loaded(|h, steps| {
                if !panic {
                    steps.extend(source_fence_steps(h, true))
                }
            });
            select(&s, &mut app, &mut bg, DiffSide::Old);
            entry::open(&mut app);
            if panic {
                let work = app.scoped_inline_work_v1.take().unwrap();
                app.scoped_inline_work_v1 = Some(inline::actual_worker_panic_for_test(work));
            }
            let writer = DraftWriter::new(database(&s.fixture));
            submit(&s, &mut app, &mut bg, &writer);
            channel::fault_next(&mut bg, if panic { 3 } else { 4 });
            match mutation {
                0 => app.repository_load_requested = true,
                1 => app.original_stash_recovery_cancel_requested_v1 = true,
                2 => app.scoped_threads_cancel_v1 = true,
                3 => {
                    app.pr_native_diff_v1
                        .as_mut()
                        .unwrap()
                        .query
                        .insert("newer query")
                        .unwrap();
                }
                4 => app.github.as_mut().unwrap().identity.account_id = Some(43),
                5 => {
                    app.github
                        .as_mut()
                        .unwrap()
                        .pr
                        .as_mut()
                        .unwrap()
                        .details
                        .summary
                        .head_sha = "f".repeat(40)
                }
                6 => app.original_tree_viewport_v2.width = 80,
                _ => app.navigation_restore_inflight = true,
            }
            let gen = channel::generation(&bg);
            app.status = "NEWER_NATIVE_INLINE_STATE".into();
            assert!(!bg.poll(&mut app));
            assert_eq!(channel::generation(&bg), gen);
            assert_eq!(app.status, "NEWER_NATIVE_INLINE_STATE");
            assert!(app.loading[View::Inbox.index()]);
            assert!(app.form.is_none());
            assert!(app.github.is_some());
            assert!(bg.cancel_scoped_inline_v1(&mut app));
            unchanged_git_and_exact_script(&s);
        }
    }
}
#[test]
fn native_inline_comments_left_right_multiline_publish_use_same_preview_confirm_journal_and_exact_receipt(
) {
    for side in [DiffSide::Old, DiffSide::New] {
        let text = if side == DiffSide::Old {
            "LEFT"
        } else {
            "RIGHT"
        };
        let (s, mut app, mut bg) = loaded(|h, steps| {
            steps.extend(source_fence_steps(h, true));
            steps.extend(image_steps(h));
            steps.extend(shape_steps(h, text));
        });
        select(&s, &mut app, &mut bg, side);
        let writer = DraftWriter::new(database(&s.fixture));
        open(&s, &mut app, &mut bg, &writer);
        composer::replace_editor_result(app.form.as_mut().unwrap(), INLINE_BODY).unwrap();
        app.form.as_mut().unwrap().preview_requested = true;
        inline::capture_form(&mut app);
        submit(&s, &mut app, &mut bg, &writer);
        finish(&mut bg, &mut app);
        assert!(app.form.as_ref().unwrap().plan_ready());
        app.form.as_mut().unwrap().submit_requested = true;
        inline::capture_form(&mut app);
        submit(&s, &mut app, &mut bg, &writer);
        finish(&mut bg, &mut app);
        assert!(app.status.contains("Comment posted #901"));
        assert!(app.pr_native_diff_v1.is_none());
        let row: (String, String) = sql(&s.fixture)
            .query_row("SELECT state,command FROM pr_write_attempts", [], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .unwrap();
        assert_eq!(row, ("confirmed".into(), "inline-comment".into()));
        let calls = commands(&s.fixture);
        let posts = calls
            .iter()
            .filter(|row| {
                row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg == "POST")
            })
            .count();
        assert_eq!(posts, 1);
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn native_inline_comments_actual_renamed_left_keeps_old_lookup_path_and_new_server_filename() {
    let old = (1..=24)
        .map(|n| format!("line{n:03} original context\n"))
        .collect::<String>();
    let new = old
        .replace("line002 original context", "line002 changed context")
        .replace("line003 original context", "line003 changed context");
    let setup = setup_with_history(
        |repo| {
            history::GitImagesV1::create_renamed_with_bytes(
                repo,
                false,
                old.as_bytes(),
                new.as_bytes(),
            )
        },
        |_| {},
    );
    let (s, mut app, mut bg) = load_setup(setup, |_, _| {});
    select(&s, &mut app, &mut bg, DiffSide::Old);
    entry::open(&mut app);
    let intent = app.scoped_inline_work_v1.as_ref().unwrap().intent();
    assert_eq!(
        intent.selected_image_path(),
        history::PREVIOUS_PATH.as_bytes()
    );
    assert_eq!(intent.server_path(), history::PATH);
    assert_ne!(
        intent.selected_image_path(),
        intent.server_path().as_bytes()
    );
    assert_eq!((intent.start(), intent.end()), (2, 3));
    assert_eq!(s.h.previous_path.as_deref(), Some(history::PREVIOUS_PATH));
    assert!(String::from_utf8_lossy(&s.h.patch).contains("rename from src/previous 界.rs"));
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
    unchanged_git_and_exact_script(&s);
}
#[test]
fn native_inline_comments_actual_provider_failure_queued_before_new_query_cannot_hide_or_change_newer_state(
) {
    let (s, mut app, mut bg) = loaded(|h, steps| {
        let mut fail = source_fence_steps(h, true);
        fail.truncate(2);
        fail[1].status = 1;
        fail[1].stderr = b"PRIVATE_NATIVE_INLINE_SOURCE_REFUSED".to_vec();
        steps.extend(fail);
    });
    select(&s, &mut app, &mut bg, DiffSide::New);
    entry::open(&mut app);
    let writer = DraftWriter::new(database(&s.fixture));
    submit(&s, &mut app, &mut bg, &writer);
    channel::fault_next(&mut bg, 4);
    app.pr_native_diff_v1
        .as_mut()
        .unwrap()
        .query
        .insert("newer immutable query")
        .unwrap();
    let generation = channel::generation(&bg);
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    app.status = "NEWER_QUERY_UNTOUCHED".into();
    assert!(!bg.poll(&mut app));
    assert_eq!(channel::generation(&bg), generation);
    assert!(Arc::ptr_eq(
        app.remote_scope_source.as_ref().unwrap(),
        &source
    ));
    assert!(app.github.is_some());
    assert!(app.pr_native_diff_v1.is_some());
    assert_eq!(app.status, "NEWER_QUERY_UNTOUCHED");
    assert!(app.form.is_none());
    assert!(bg.cancel_scoped_inline_v1(&mut app));
    unchanged_git_and_exact_script(&s);
}
#[test]
fn native_inline_comments_comment_and_pending_cancel_share_real_mouse_keyboard_hits_at_40_80_120() {
    use crate::repository_native_diff_ui_v1::NativeDiffActionV1 as A;
    use crossterm::event::{MouseButton, MouseEvent, MouseEventKind};
    for width in [40, 80, 120] {
        for mouse in [false, true] {
            let (s, mut app, mut bg) = loaded(|_, _| {});
            select(&s, &mut app, &mut bg, DiffSide::New);
            app.handle_event(Event::Resize(width, 32));
            if app.pr_native_diff_work_v1.is_some() {
                project(&s, &mut app, &mut bg);
            }
            app.pr_native_diff_layout_v1.controls = true;
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, 32)).unwrap();
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    assert!(crate::repository_native_diff_native_v1::draw(
                        frame, &mut app, area
                    ));
                })
                .unwrap();
            let hit = app
                .pr_native_diff_layout_v1
                .hits()
                .iter()
                .find(|hit| hit.action == A::Comment)
                .expect("visible P control has actual same-action hit");
            assert!(hit.rect.width > 0 && hit.rect.right() <= width);
            let point = (hit.rect.x, hit.rect.y);
            assert_eq!(
                app.pr_native_diff_layout_v1
                    .key(KeyCode::Char('P'), KeyModifiers::NONE),
                Some(A::Comment)
            );
            if mouse {
                app.handle_event(Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: point.0,
                    row: point.1,
                    modifiers: KeyModifiers::NONE,
                }));
            } else {
                app.handle_event(Event::Key(KeyEvent::new(
                    KeyCode::Char('P'),
                    KeyModifiers::NONE,
                )));
            }
            assert!(app.scoped_inline_work_v1.is_some());
            terminal
                .draw(|frame| {
                    let area = frame.area();
                    assert!(crate::repository_native_diff_native_v1::draw(
                        frame, &mut app, area
                    ));
                })
                .unwrap();
            let cancel = app
                .pr_native_diff_layout_v1
                .hits()
                .iter()
                .find(|hit| hit.action == A::CancelInline)
                .unwrap()
                .rect;
            assert!(cancel.width > 0);
            if mouse {
                app.handle_event(Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: cancel.x,
                    row: cancel.y,
                    modifiers: KeyModifiers::NONE,
                }));
            } else {
                app.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
            }
            assert!(app.scoped_inline_work_v1.is_none());
            assert!(app.pr_native_diff_v1.as_ref().unwrap().range().is_some());
            unchanged_git_and_exact_script(&s);
        }
    }
}
#[test]
fn native_inline_comments_home_from_right_side_keeps_header_focus_visible_without_changing_range() {
    let (s, mut app, mut bg) = loaded(|_, _| {});
    select(&s, &mut app, &mut bg, DiffSide::New);
    let state = app.pr_native_diff_v1.as_ref().unwrap();
    assert!(
        state.right,
        "actual selected RIGHT range is the starting cursor"
    );
    assert!(state
        .current_frame()
        .unwrap()
        .rows()
        .first()
        .unwrap()
        .right()
        .is_none());
    let original = state.range().unwrap().clone();
    app.handle_event(Event::Key(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE)));
    let state = app.pr_native_diff_v1.as_ref().unwrap();
    assert_eq!(state.selected, 0);
    assert!(!state.right, "Home must focus the real visible header cell");
    assert!(Arc::ptr_eq(state.range().unwrap(), &original));
    assert!(app.scoped_inline_work_v1.is_none());
    assert!(app.pr_native_diff_work_v1.is_none());
    let writes: u64 = sql(&s.fixture)
        .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(writes, 0);
    unchanged_git_and_exact_script(&s);
}
#[test]
fn native_inline_comments_external_chooser_cannot_mint_a_concurrent_composer_or_review_anchor() {
    let (s, mut app, mut bg) = loaded(|_, _| {});
    select(&s, &mut app, &mut bg, DiffSide::New);
    let original = app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .capture_current_range_v2()
        .unwrap();
    app.pr_native_diff_v1
        .as_mut()
        .unwrap()
        .choose_external_v4(ratatui::layout::Rect::new(0, 0, 120, 40))
        .unwrap();
    let before = commands(&s.fixture).len();
    entry::open(&mut app);
    assert!(app.scoped_inline_work_v1.is_none());
    assert!(app.scoped_inline_owner_v1.is_none());
    assert!(app.form.is_none());
    assert!(app.notice.contains("Return from the owned external viewer"));
    assert!(app
        .pr_native_diff_v1
        .as_ref()
        .unwrap()
        .external_v4
        .is_some());
    assert!(original.matches(app.pr_native_diff_v1.as_ref().unwrap()));
    assert_eq!(commands(&s.fixture).len(), before);
    unchanged_git_and_exact_script(&s);
}
