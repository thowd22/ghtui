//! Undeclared real-source/TestBackend contracts, not substitutes for PTY.
use super::*;
use crate::{
    local_commit_actor_origin_v1_test_support::{git, Real},
    workspace::RepositoryMode,
};
use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{backend::TestBackend, Terminal};

fn workspace(real: &Real) -> Workspace {
    Workspace::load(&real.root, RepositoryMode::Graph, &real.original, None)
        .unwrap()
        .unwrap()
}

#[test]
fn commit_graph_v3_resize_keeps_real_selected_commit_painted_without_resetting_manual_scroll() {
    let real = Real::new("sha256");
    let loaded = workspace(&real);
    let selected = loaded.graph_focus_row_v1().unwrap();
    let action = loaded.display_rows_v1()[selected].action.clone().unwrap();
    let mut app = crate::app::App {
        view: crate::app::View::Repository,
        repository_list: crate::ui_table::ListState {
            selected,
            ..Default::default()
        },
        workspace: Some(loaded),
        repository_visible_rows: 30,
        repository_content_focused: true,
        ..Default::default()
    };
    let baseline = real.snapshot();
    real.clear_trace();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut app))
        .unwrap();
    assert!(app.repository_visible_rows > 0);
    assert!(app.repository_list.selected >= app.repository_list.top);
    assert!(app.repository_list.selected < app.repository_list.top + app.repository_visible_rows);
    let expected = Action::RepositoryRow(action);
    let hit = app.hits.iter().find(|hit| hit.action == expected).unwrap();
    let text = (hit.rect.x..hit.rect.right())
        .map(|x| terminal.backend().buffer()[(x, hit.rect.y)].symbol())
        .collect::<String>();
    // The History table shows the 7-character SHA and the `▌` bar on the selected row.
    assert!(text.contains(&real.original[..7]));
    assert_eq!(
        terminal.backend().buffer()[(hit.rect.x, hit.rect.y)].symbol(),
        crate::ui_table::SELECTION_BAR
    );
    app.repository_list.top = 0;
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut app))
        .unwrap();
    assert_eq!(
        app.repository_list.top, 0,
        "An unchanged frame must preserve deliberate body scrolling"
    );
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), baseline);
}
#[test]
fn commit_graph_v1_controls_wrap_all_same_actions_key_mouse_at120_80_40_without_reads() {
    let real = Real::new("sha1");
    let loaded = workspace(&real);
    let baseline = real.snapshot();
    real.clear_trace();
    let controls = controls(&loaded, false);
    assert_eq!(
        controls.len(),
        12,
        "Every graph action keeps one key-bar control"
    );
    for width in [140, 80, 40] {
        let mut terminal = Terminal::new(TestBackend::new(width, 2)).unwrap();
        let mut hits = vec![];
        terminal
            .draw(|frame| {
                hits = crate::footer::render(
                    frame,
                    ratatui::layout::Rect::new(0, 1, width, 1),
                    Theme::from_settings(0, false),
                    &controls,
                    None,
                )
                .into_iter()
                .map(|(rect, action)| crate::app::HitRegion { rect, action })
                .collect();
            })
            .unwrap();
        for (code, token, compact) in [
            (KeyCode::Char('<'), "< Newer loaded", "< Newer"),
            (KeyCode::Char('>'), "> Older loaded", "> Older"),
            (KeyCode::Home, "Home First loaded", "Home First"),
            (KeyCode::End, "End Last loaded", "End Last"),
            (KeyCode::Char('x'), "x Discard graph Back", "x Discard"),
            (KeyCode::Char('N'), "N More history", "N More"),
            (KeyCode::Char('P'), "P Parents", "P Parents"),
            (KeyCode::Char('D'), "D Details", "D Details"),
            (KeyCode::Char('l'), "l Legend", "l Legend"),
            (KeyCode::Left, "← Pan left", "← Pan left"),
            (KeyCode::Right, "→ Pan right", "→ Pan right"),
        ] {
            let action = key(&loaded, code).unwrap();
            let expected = loaded.graph_action_reason_v1(&action).map_or_else(
                || Action::RepositoryRow(RowAction::Graph(action)),
                Action::Explain,
            );
            let event = KeyEvent::new(code, crossterm::event::KeyModifiers::NONE);
            assert_eq!(
                crate::footer::resolve_key(&controls, event),
                Some(expected.clone()),
                "The key resolves to its token's action at {width}"
            );
            let mut keyboard = crate::app::App {
                view: crate::app::View::Repository,
                workspace: Some(loaded.clone()),
                ..Default::default()
            };
            keyboard.handle_event(crossterm::event::Event::Key(event));
            let Some(hit) = hits.iter().find(|hit| hit.action == expected) else {
                // Disabled or dropped for width: the key still answers, never a dead token.
                assert!(
                    width < 140 || matches!(expected, Action::Explain(_)),
                    "{token} must be painted at {width}"
                );
                if matches!(expected, Action::Explain(_)) {
                    assert_eq!(keyboard.overlay, Some(crate::app::Overlay::Notice));
                }
                continue;
            };
            let painted = (hit.rect.x..hit.rect.right())
                .map(|col| terminal.backend().buffer()[(col, hit.rect.y)].symbol())
                .collect::<String>();
            assert_eq!(painted, if width < 100 { compact } else { token });
            let mut mouse = crate::app::App {
                view: crate::app::View::Repository,
                workspace: Some(loaded.clone()),
                hits: hits.clone(),
                ..Default::default()
            };
            mouse.handle_event(crossterm::event::Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: hit.rect.x,
                row: hit.rect.y,
                modifiers: crossterm::event::KeyModifiers::NONE,
            }));
            assert_eq!(keyboard.repository_action, mouse.repository_action);
            assert_eq!(keyboard.status, mouse.status);
            if code == KeyCode::Char('N') {
                assert!(keyboard.overlay.is_some());
                assert!(keyboard.repository_action.is_none());
            }
        }
    }
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), baseline);
}
#[test]
fn commit_graph_v1_original_history_actor_tree_do_not_get_stale_graph_hits_or_keys() {
    let real = Real::new("sha256");
    let mut loaded = workspace(&real);
    loaded.actor_profile = Some(crate::identity::GitRole::Author);
    assert!(controls(&loaded, false).is_empty());
    assert_eq!(key(&loaded, KeyCode::Char('N')), None);
    loaded.actor_profile = None;
    loaded.mode = RepositoryMode::Tree;
    assert!(controls(&loaded, false).is_empty());
    loaded.mode = RepositoryMode::Graph;
    loaded.file_provenance = Some(crate::workspace::FileProvenance::new(
        real.original.clone(),
        crate::git::RawPath(b"file".to_vec()),
    ));
    loaded.file_provenance.as_mut().unwrap().view = crate::workspace::ProvenanceView::History;
    assert!(controls(&loaded, false).is_empty());
    assert_eq!(key(&loaded, KeyCode::Char('P')), None);
    assert!(line(&loaded, 0, Theme::from_settings(0, false), 40).is_none());
}
#[test]
fn commit_graph_v1_horizontal_scroll_preserves_wide_combining_glyphs_and_exact_row_action() {
    let real = Real::new("sha1");
    let tree = String::from_utf8(git(&real.root, &["rev-parse", "HEAD^{tree}"], None)).unwrap();
    let subject = format!("{} e\u{301}", "界".repeat(64));
    let tip = String::from_utf8(git(
        &real.root,
        &[
            "commit-tree",
            tree.trim(),
            "-p",
            &real.original,
            "-m",
            &subject,
        ],
        None,
    ))
    .unwrap();
    let mut loaded = Workspace::load(&real.root, RepositoryMode::Graph, tip.trim(), None)
        .unwrap()
        .unwrap();
    let focus = loaded.graph_focus_row_v1().unwrap();
    let expected = loaded.display_rows_v1()[focus].action.clone();
    for left in [0, 1, 8, 23, 127] {
        loaded.graph_horizontal_v1 = left;
        let rendered = line(&loaded, focus, Theme::from_settings(0, false), 40).unwrap();
        let text = rendered
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        assert!(ratatui::text::Line::from(text.as_str()).width() <= 40);
        assert!(!text.starts_with('\u{301}'));
        assert_eq!(loaded.display_rows_v1()[focus].action, expected);
        let full = line(&loaded, focus, Theme::from_settings(0, false), 4096).unwrap();
        let text = full
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>();
        for span in loaded
            .graph_paint_v1(focus)
            .unwrap()
            .iter()
            .filter(|span| span.color.is_none())
        {
            assert!(
                text.contains(&span.text),
                "Panning lanes must preserve the selected member and actor labels"
            );
        }
    }
    let baseline = real.snapshot();
    real.clear_trace();
    for action in [
        GraphActionV1::Parents,
        GraphActionV1::Details,
        GraphActionV1::Legend,
    ] {
        loaded.open(RowAction::Graph(action)).unwrap();
        assert_eq!(loaded.graph_horizontal_v1, 127);
        for (index, row) in loaded.display_rows_v1().iter().enumerate() {
            let rendered = line(&loaded, index, Theme::from_settings(0, false), 4096).unwrap();
            let text = rendered
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>();
            assert_eq!(
                text, row.label,
                "Subview semantic text must not inherit lane clipping"
            );
        }
        loaded.open(RowAction::Graph(GraphActionV1::Back)).unwrap();
        assert_eq!(loaded.graph_horizontal_v1, 127);
        for (index, row) in loaded.display_rows_v1().iter().enumerate() {
            if loaded
                .graph_paint_v1(index)
                .unwrap()
                .iter()
                .all(|span| span.color.is_none())
            {
                let rendered = line(&loaded, index, Theme::from_settings(0, false), 4096).unwrap();
                let text = rendered
                    .spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>();
                assert_eq!(
                    text, row.label,
                    "Timeline explanation/count headers must remain readable"
                );
            }
        }
    }
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), baseline);
}
