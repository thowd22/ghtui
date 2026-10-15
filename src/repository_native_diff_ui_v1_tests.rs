use super::*;
use crate::{
    git::{Comparison, OriginalComparisonSideV1},
    original_diff_sides_v1_test_support as fixture,
    providers::local_git_navigation_read_v1::LocalGitNavigationReadV1,
};
use ratatui::{backend::TestBackend, Terminal};
fn loaded(
    width: u16,
) -> (
    crate::local_commit_actor_origin_v1_test_support::Real,
    crate::workspace::Workspace,
    NativeDiffStateV1,
) {
    let (real, a, b, _) = fixture::fixture("sha256");
    let mut workspace = fixture::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    fixture::source(&workspace).install(&mut workspace).unwrap();
    let mut state = NativeDiffStateV1::open(&workspace, width.saturating_sub(2)).unwrap();
    state.file = state
        .source()
        .unwrap()
        .members()
        .iter()
        .find(|member| {
            member
                .path(OriginalComparisonSideV1::New)
                .is_some_and(|path| path.0 == b"shared")
        })
        .unwrap()
        .index();
    prepare(&mut state);
    (real, workspace, state)
}
fn prepare(state: &mut NativeDiffStateV1) {
    let request = state.request().unwrap();
    let operation =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let prepared = request.prepare(&operation).unwrap();
    assert_eq!(operation.seal().unwrap().children_started(), 0);
    assert!(state.accept(prepared));
}
fn draw(
    layout: &mut NativeDiffLayoutV1,
    state: &mut NativeDiffStateV1,
    width: u16,
    height: u16,
) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            layout.draw(
                frame,
                state,
                frame.area(),
                crate::theme::Theme::from_settings(0, false),
            )
        })
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}
#[test]
fn native_diff_ui_borrowed_wide_split_and_narrow_focused_original_hits_zero_git() {
    let (real, workspace, mut state) = loaded(120);
    let before = real.snapshot();
    real.clear_trace();
    state.option(NativeDiffOptionChangeV1::Layout).unwrap();
    state
        .option(NativeDiffOptionChangeV1::Side(DiffSide::Old))
        .unwrap();
    prepare(&mut state);
    let mut layout = NativeDiffLayoutV1::default();
    let screen = draw(&mut layout, &mut state, 120, 24);
    assert!(screen.contains("COMMON old bytes"));
    assert!(screen.contains("TARGET new bytes"));
    assert!(layout
        .hits
        .iter()
        .any(|hit| matches!(&hit.action, NativeDiffActionV1::Point { right: true, .. })));
    let source = state.source().unwrap().clone();
    state.width = 38;
    prepare(&mut state);
    let screen = draw(&mut layout, &mut state, 40, 24);
    assert!(screen.contains("COMMON old bytes"));
    assert!(!screen.contains("TARGET new bytes"));
    assert!(state.frame().unwrap().split_focused());
    assert!(std::sync::Arc::ptr_eq(state.source().unwrap(), &source));
    assert!(std::sync::Arc::ptr_eq(
        &source,
        workspace.original_comparison_v1.as_ref().unwrap()
    ));
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
#[test]
fn native_diff_ui_drag_uses_two_genuine_current_epoch_endpoints_and_retired_hit_rejected() {
    let (_real, _workspace, mut state) = loaded(120);
    let mut layout = NativeDiffLayoutV1::default();
    draw(&mut layout, &mut state, 120, 24);
    let points: Vec<_> = layout
        .hits
        .iter()
        .filter_map(|hit| match &hit.action {
            NativeDiffActionV1::Point { ticket, .. }
                if matches!(
                    ticket.key().canonical().kind,
                    crate::diff::LineKind::Added | crate::diff::LineKind::Removed
                ) =>
            {
                Some((hit.rect, ticket.clone()))
            }
            _ => None,
        })
        .collect();
    assert_eq!(points.len(), 2);
    let (first, first_ticket) = &points[0];
    let (last, last_ticket) = &points[1];
    assert!(matches!(
        layout.mouse(MouseEventKind::Down(MouseButton::Left), first.x, first.y),
        Some(NativeDiffActionV1::Point { .. })
    ));
    layout.mouse(MouseEventKind::Drag(MouseButton::Left), last.x, last.y);
    let Some(NativeDiffActionV1::Drag { first: a, last: b }) =
        layout.mouse(MouseEventKind::Up(MouseButton::Left), last.x, last.y)
    else {
        panic!("actual drag result")
    };
    assert_eq!(&a, first_ticket);
    assert_eq!(&b, last_ticket);
    state.select_between(a, b).unwrap();
    prepare(&mut state);
    assert_eq!(state.range().unwrap().changed().len(), 2);
    assert_eq!(state.range().unwrap().side(), DiffSide::Both);
    assert!(state.select(first_ticket.clone(), false).is_err());
}
#[test]
fn native_diff_ui_all_options_click_and_keyboard_use_exact_same_actions_and_focus() {
    let (_real, _workspace, mut state) = loaded(40);
    let mut layout = NativeDiffLayoutV1 {
        controls: true,
        ..Default::default()
    };
    draw(&mut layout, &mut state, 40, 24);
    assert_eq!(layout.hits.len(), 24);
    let hits = layout.hits.clone();
    for hit in hits {
        assert_eq!(
            layout.mouse(
                MouseEventKind::Down(MouseButton::Left),
                hit.rect.x,
                hit.rect.y
            ),
            Some(hit.action)
        );
    }
    for _ in 0..layout.hits.len() {
        layout.cycle(false);
        assert!(layout.activated().is_some());
    }
    assert_eq!(
        layout.key(KeyCode::Char('l'), KeyModifiers::NONE),
        Some(NativeDiffActionV1::Option(NativeDiffOptionChangeV1::Layout))
    );
    assert_eq!(
        layout.key(KeyCode::Char('a'), KeyModifiers::NONE),
        Some(NativeDiffActionV1::Stage)
    );
    assert_eq!(
        layout.key(KeyCode::Down, KeyModifiers::NONE),
        Some(NativeDiffActionV1::FocusNext(false)),
        "options capture arrow navigation instead of moving hidden diff rows"
    );
}

#[test]
fn native_diff_ui_commands_exposes_same_guarded_open_or_explicit_unavailable_without_git() {
    let (real, workspace, _state) = loaded(120);
    let before = real.snapshot();
    real.clear_trace();
    let mut app = crate::app::App {
        view: crate::app::View::Repository,
        workspace: Some(workspace),
        ..Default::default()
    };
    let catalogue = crate::navigation_catalogue::PreparedCatalogue::prepare(&app, 1).unwrap();
    let entry = catalogue
        .entries
        .iter()
        .find(|entry| entry.id == "repository.native-diff")
        .unwrap();
    assert_eq!(
        entry.action,
        crate::app::Action::NativeDiffV1(NativeDiffActionV1::Open)
    );
    app.workspace = None;
    let catalogue = crate::navigation_catalogue::PreparedCatalogue::prepare(&app, 2).unwrap();
    assert!(
        matches!(&catalogue.entries.iter().find(|entry|entry.id=="repository.native-diff").unwrap().action,crate::app::Action::Explain(reason) if reason.contains("actual local saved comparison"))
    );
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
#[test]
fn native_diff_ui_tiny_close_clickable_and_pending_resize_has_no_original_row_hits() {
    let (_real, _workspace, mut state) = loaded(120);
    let mut layout = NativeDiffLayoutV1::default();
    draw(&mut layout, &mut state, 8, 3);
    assert_eq!(layout.hits.len(), 1);
    let hit = layout.hits[0].clone();
    assert_eq!(
        layout.mouse(
            MouseEventKind::Down(MouseButton::Left),
            hit.rect.x,
            hit.rect.y
        ),
        Some(NativeDiffActionV1::Close)
    );
    state.width = 38;
    state.request().unwrap();
    draw(&mut layout, &mut state, 40, 24);
    assert!(!layout
        .hits
        .iter()
        .any(|hit| matches!(hit.action, NativeDiffActionV1::Point { .. })));
}
#[test]
fn native_diff_ui_worker_paint_spans_disjoint_and_search_priority_no_foreground_match() {
    let (_real, _workspace, mut state) = loaded(120);
    state.query.insert("TARGET").unwrap();
    prepare(&mut state);
    let frame = state.frame().unwrap();
    let cells: Vec<_> = frame
        .rows()
        .iter()
        .flat_map(|row| std::iter::once(row.left()).chain(row.right()))
        .collect();
    assert!(cells.iter().any(|cell| cell
        .spans()
        .iter()
        .any(|span| span.role == NativeDiffPaintRoleV1::Search)));
    for cell in cells {
        for pair in cell.spans().windows(2) {
            assert!(pair[0].bytes.end <= pair[1].bytes.start);
        }
        for span in cell.spans() {
            assert!(cell.text().is_char_boundary(span.bytes.start));
            assert!(cell.text().is_char_boundary(span.bytes.end));
        }
    }
}

#[test]
fn native_diff_ui_exact_point_pairing_and_worker_space_preserve_original_old_focus_before_extension(
) {
    use crate::{
        app::{App, View},
        repository_diff_sides_worker_v1::{OriginalDiffWorkV1, OriginalDiffWorkerOwnerV1},
        repository_native_diff_native_v1::dispatch,
    };
    let (real, workspace, state) = loaded(120);
    let frame = state.current_frame().unwrap().clone();
    let (row, ticket) = frame
        .rows()
        .iter()
        .enumerate()
        .find_map(|(row, _)| {
            frame
                .ticket(row, false)
                .filter(|ticket| ticket.key().canonical().kind == crate::diff::LineKind::Removed)
                .map(|ticket| (row, ticket))
        })
        .unwrap();
    let before = real.snapshot();
    real.clear_trace();
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        native_diff_v1: Some(state),
        ..App::default()
    };
    let selected = app.native_diff_v1.as_ref().unwrap().selected;
    dispatch(
        &mut app,
        NativeDiffActionV1::Point {
            ticket: ticket.clone(),
            row: row + 1,
            right: false,
        },
    );
    assert_eq!(
        app.native_diff_v1.as_ref().unwrap().selected,
        selected,
        "current ticket cannot authorize another displayed row"
    );
    dispatch(
        &mut app,
        NativeDiffActionV1::Point {
            ticket: ticket.clone(),
            row,
            right: true,
        },
    );
    assert_eq!(
        app.native_diff_v1.as_ref().unwrap().selected,
        selected,
        "current left ticket cannot authorize a right-side cell"
    );
    dispatch(
        &mut app,
        NativeDiffActionV1::Point {
            ticket: ticket.clone(),
            row,
            right: false,
        },
    );
    assert_eq!(app.native_diff_v1.as_ref().unwrap().selected, row);
    dispatch(&mut app, NativeDiffActionV1::Select);
    let work = app.original_diff_work_v1.take().unwrap();
    assert!(matches!(work.as_ref(), OriginalDiffWorkV1::Native(_)));
    let owner = OriginalDiffWorkerOwnerV1::begin(
        app.workspace.as_ref().unwrap(),
        None,
        work,
        Default::default(),
    )
    .unwrap();
    let prepared = owner.prepare().unwrap();
    assert_eq!(owner.accounting().unwrap().children_started(), 0);
    assert!(owner.accounting().unwrap().sealed());
    prepared
        .publish_native_v1(
            app.workspace.as_ref().unwrap(),
            app.native_diff_v1.as_mut().unwrap(),
        )
        .unwrap();
    let state = app.native_diff_v1.as_ref().unwrap();
    assert_eq!(
        state.selected, row,
        "Space preparation restores exact original Old row before any extension input"
    );
    let current = state
        .current_frame()
        .unwrap()
        .ticket(state.selected, state.right)
        .unwrap();
    assert!(current.key().same(ticket.key()));
    assert_eq!(
        current.key().canonical().kind,
        crate::diff::LineKind::Removed
    );
    assert_eq!(state.range().unwrap().changed().len(), 1);
    assert_eq!(state.range().unwrap().side(), DiffSide::Both);
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_diff_ui_worker_matching_line_index_reveals_actual_wrapped_highlight_from_header_without_git(
) {
    use crate::{
        app::{App, View},
        git::Comparison,
        repository_native_diff_native_v1::dispatch,
    };
    let real = crate::local_commit_actor_origin_v1_test_support::Real::new("sha256");
    let old = fixture::commit(
        &real,
        &fixture::tree(&real, &[(b"code.rs", b"old\n", "100644")]),
        None,
        "Old",
    );
    let new_text = format!("let {}TARGET;\n", "prefix".repeat(21));
    let new = fixture::commit(
        &real,
        &fixture::tree(&real, &[(b"code.rs", new_text.as_bytes(), "100644")]),
        Some(&old),
        "New",
    );
    let mut workspace = fixture::workspace(
        &real,
        Comparison::Snapshots {
            base: old,
            head: new,
        },
    );
    fixture::source(&workspace).install(&mut workspace).unwrap();
    let mut state = NativeDiffStateV1::open(&workspace, 38).unwrap();
    state.option(NativeDiffOptionChangeV1::Wrap).unwrap();
    state.query.insert("TARGET").unwrap();
    let before = real.snapshot();
    real.clear_trace();
    prepare(&mut state);
    let frame = state.frame().unwrap();
    assert_eq!(frame.matches().len(), 1);
    let key = frame.matches()[0].key();
    let first = frame.first_matching_line_row(key).unwrap();
    assert!(
        first > frame.rows_for(key).unwrap().start,
        "matching fragment is not the original line's first wrapped row"
    );
    assert!(frame.rows()[first].left().text().contains("TARGET"));
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        native_diff_v1: Some(state),
        ..App::default()
    };
    dispatch(&mut app, NativeDiffActionV1::First);
    assert_eq!(app.native_diff_v1.as_ref().unwrap().selected, 0);
    dispatch(&mut app, NativeDiffActionV1::Match(1));
    assert_eq!(
        app.native_diff_v1.as_ref().unwrap().selected,
        first,
        "actual prepared matching fragment must be selected directly, not scanned in draw/input"
    );
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_diff_ui_invalid_query_labels_previous_preview_retained_range_and_has_no_row_hits() {
    let (real, _, mut state) = loaded(120);
    let frame = state.current_frame().unwrap().clone();
    let ticket = (0..frame.rows().len())
        .find_map(|row| {
            frame
                .ticket(row, false)
                .filter(|ticket| ticket.key().canonical().kind == crate::diff::LineKind::Removed)
        })
        .unwrap();
    state.select(ticket, false).unwrap();
    prepare(&mut state);
    let range = state.range().unwrap().clone();
    let preview = state.frame().unwrap().clone();
    let before = real.snapshot();
    real.clear_trace();
    state.regex = true;
    state.query.insert("[").unwrap();
    prepare(&mut state);
    let mut layout = NativeDiffLayoutV1::default();
    let screen = draw(&mut layout, &mut state, 120, 24);
    assert!(screen.contains("Range 1 retained • previous preview (not current)"));
    assert!(screen.contains(&format!(
        "Previous preview: {} • NOT CURRENT • no row hits",
        preview.source().members()[preview.file()].label()
    )));
    assert!(screen.contains("COMMON old bytes"));
    assert!(!layout
        .hits()
        .iter()
        .any(|hit| matches!(hit.action, NativeDiffActionV1::Point { .. })));
    assert!(state.current_frame().is_none());
    assert!(std::sync::Arc::ptr_eq(state.frame().unwrap(), &preview));
    assert!(std::sync::Arc::ptr_eq(state.range().unwrap(), &range));
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn native_diff_ui_wide_terminal_bounded512_current_mouse_ticket_and_input_resize_reflow_zero_git() {
    use crate::{
        app::{App, View},
        repository_diff_sides_worker_v1::OriginalDiffWorkerOwnerV1,
        repository_native_diff_native_v1::{capture, dispatch},
    };
    let (real, workspace, _) = loaded(120);
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        ..Default::default()
    };
    app.original_tree_viewport_v2.width = 600;
    let before = real.snapshot();
    real.clear_trace();
    dispatch(&mut app, NativeDiffActionV1::Open);
    assert_eq!(app.native_diff_v1.as_ref().unwrap().width, 512);
    let work = app.original_diff_work_v1.take().unwrap();
    let owner = OriginalDiffWorkerOwnerV1::begin(
        app.workspace.as_ref().unwrap(),
        None,
        work,
        Default::default(),
    )
    .unwrap();
    owner
        .prepare()
        .unwrap()
        .publish_native_v1(
            app.workspace.as_ref().unwrap(),
            app.native_diff_v1.as_mut().unwrap(),
        )
        .unwrap();
    assert_eq!(owner.accounting().unwrap().children_started(), 0);
    let screen = draw(
        &mut app.native_diff_layout_v1,
        app.native_diff_v1.as_mut().unwrap(),
        600,
        24,
    );
    assert!(screen.contains("bounded 512-column pane"));
    let (ticket, row, right) = app
        .native_diff_layout_v1
        .hits()
        .iter()
        .find_map(|hit| match &hit.action {
            NativeDiffActionV1::Point { ticket, row, right } => {
                Some((ticket.clone(), *row, *right))
            }
            _ => None,
        })
        .expect("actual bounded wide terminal has clickable prepared rows");
    assert!(app
        .native_diff_layout_v1
        .hits()
        .iter()
        .all(|hit| hit.rect.right() <= 513));
    dispatch(
        &mut app,
        NativeDiffActionV1::Point {
            ticket: ticket.clone(),
            row,
            right,
        },
    );
    assert_eq!(app.native_diff_v1.as_ref().unwrap().selected, row);
    assert!(capture(&mut app, &crossterm::event::Event::Resize(800, 24)));
    assert_eq!(app.native_diff_v1.as_ref().unwrap().width, 512);
    assert!(app.original_diff_work_v1.is_none());
    assert!(capture(&mut app, &crossterm::event::Event::Resize(80, 24)));
    assert_eq!(app.native_diff_v1.as_ref().unwrap().width, 78);
    assert!(app
        .native_diff_v1
        .as_ref()
        .unwrap()
        .current_frame()
        .is_none());
    assert!(app.native_diff_layout_v1.hits().is_empty());
    let work = app.original_diff_work_v1.take().unwrap();
    let owner = OriginalDiffWorkerOwnerV1::begin(
        app.workspace.as_ref().unwrap(),
        None,
        work,
        Default::default(),
    )
    .unwrap();
    owner
        .prepare()
        .unwrap()
        .publish_native_v1(
            app.workspace.as_ref().unwrap(),
            app.native_diff_v1.as_mut().unwrap(),
        )
        .unwrap();
    assert!(!app
        .native_diff_v1
        .as_ref()
        .unwrap()
        .current_frame()
        .unwrap()
        .accepts(&ticket));
    draw(
        &mut app.native_diff_layout_v1,
        app.native_diff_v1.as_mut().unwrap(),
        80,
        24,
    );
    assert!(app
        .native_diff_layout_v1
        .hits()
        .iter()
        .any(|hit| matches!(hit.action, NativeDiffActionV1::Point { .. })));
    assert_eq!(owner.accounting().unwrap().children_started(), 0);
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
