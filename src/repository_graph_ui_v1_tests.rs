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
        selected,
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
    assert!(app.selected >= app.scroll);
    assert!(app.selected < app.scroll + app.repository_visible_rows);
    let expected = Action::RepositoryRow(action);
    let hit = app.hits.iter().find(|hit| hit.action == expected).unwrap();
    let text = (hit.rect.x..hit.rect.right())
        .map(|x| terminal.backend().buffer()[(x, hit.rect.y)].symbol())
        .collect::<String>();
    assert!(text.contains(&real.original[..8]));
    assert!(terminal.backend().buffer()[(hit.rect.x, hit.rect.y)]
        .modifier
        .contains(ratatui::style::Modifier::REVERSED));
    app.scroll = 0;
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut app))
        .unwrap();
    assert_eq!(
        app.scroll, 0,
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
    for width in [120, 80, 40] {
        let mut terminal = Terminal::new(TestBackend::new(width, 8)).unwrap();
        let mut hits = vec![];
        terminal
            .draw(|frame| {
                let area = frame.area();
                assert!(
                    draw_controls(
                        frame,
                        &loaded,
                        &mut hits,
                        area,
                        Theme::from_settings(0, false),
                        false,
                        GraphControlFocusV2 {
                            hit: usize::MAX,
                            content_focused: false
                        }
                    ) <= 6
                );
            })
            .unwrap();
        assert_eq!(
            hits.len(),
            12,
            "Every exposed graph control must retain a full hit at width {width}"
        );
        for (code, label) in [
            (KeyCode::Char('<'), "[< Newer loaded]"),
            (KeyCode::Char('>'), "[> Older loaded]"),
            (KeyCode::Home, "[Home First loaded]"),
            (KeyCode::End, "[End Last loaded]"),
            (KeyCode::Char('x'), "[x Discard graph Back]"),
            (KeyCode::Char('N'), "[N More history]"),
            (KeyCode::Char('P'), "[P Parents]"),
            (KeyCode::Char('D'), "[D Details]"),
            (KeyCode::Char('l'), "[l Legend]"),
            (KeyCode::Left, "[← Graph]"),
            (KeyCode::Right, "[Graph →]"),
        ] {
            let action = key(&loaded, code).unwrap();
            let expected = loaded.graph_action_reason_v1(&action).map_or_else(
                || Action::RepositoryRow(RowAction::Graph(action)),
                Action::Explain,
            );
            let hit = hits
                .iter()
                .find(|hit| {
                    hit.action == expected
                        && (hit.rect.x..hit.rect.right())
                            .map(|col| terminal.backend().buffer()[(col, hit.rect.y)].symbol())
                            .collect::<String>()
                            == label
                })
                .unwrap();
            let mut painted = String::new();
            for column in hit.rect.x..hit.rect.right() {
                painted.push_str(terminal.backend().buffer()[(column, hit.rect.y)].symbol());
            }
            assert_eq!(painted, label);
            let mut keyboard = crate::app::App {
                view: crate::app::View::Repository,
                workspace: Some(loaded.clone()),
                ..Default::default()
            };
            keyboard.handle_event(crossterm::event::Event::Key(KeyEvent::new(
                code,
                crossterm::event::KeyModifiers::NONE,
            )));
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
    assert!(controls(&loaded).is_empty());
    assert_eq!(key(&loaded, KeyCode::Char('N')), None);
    loaded.actor_profile = None;
    loaded.mode = RepositoryMode::Tree;
    assert!(controls(&loaded).is_empty());
    loaded.mode = RepositoryMode::Graph;
    loaded.file_provenance = Some(crate::workspace::FileProvenance::new(
        real.original.clone(),
        crate::git::RawPath(b"file".to_vec()),
    ));
    loaded.file_provenance.as_mut().unwrap().view = crate::workspace::ProvenanceView::History;
    assert!(controls(&loaded).is_empty());
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
