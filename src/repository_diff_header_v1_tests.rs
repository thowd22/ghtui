use crate::{
    app::{App, View},
    git::{Comparison, OriginalComparisonSideV1},
    original_diff_sides_v1_test_support as support,
};
use ratatui::{backend::TestBackend, Terminal};

fn rendered(app: &mut App, width: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    let buffer = terminal.backend().buffer();
    (0..24)
        .map(|y| {
            (0..width)
                .map(|x| buffer[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn original_diff_sides_header_pins_exact_comparison_while_member_focus_scrolls_at80_120() {
    let (real, a, b, _) = support::fixture("sha256");
    let mut workspace = support::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    let source = support::source(&workspace);
    source.install(&mut workspace).unwrap();
    let selected = workspace.display_rows_v1().len() - 1;
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        repository_list: crate::ui_table::ListState {
            selected,
            top: selected,
        },
        repository_content_focused: true,
        ..Default::default()
    };
    let before = real.snapshot();
    real.clear_trace();
    for width in [80, 120] {
        let text = rendered(&mut app, width);
        assert!(text.contains("Saved comparison • actual Old/New content"));
        assert!(text.contains("TARGET new bytes"));
        // Changed files is a key-bar token, compact below 100 columns.
        let footer = text.lines().last().unwrap();
        assert!(footer.contains(if width < 100 {
            " f Files  "
        } else {
            " f Changed files  "
        }));
        assert!(!text.contains("[f Changed files]"));
        assert!(std::sync::Arc::ptr_eq(
            &source,
            app.workspace
                .as_ref()
                .unwrap()
                .original_comparison_v1
                .as_ref()
                .unwrap()
        ));
        assert_eq!(app.repository_list.selected, selected);
    }
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}

#[test]
fn original_diff_sides_header_pins_original_old_title_with_actual_c_body_at80_120() {
    let (real, a, b, c) = support::fixture("sha256");
    let mut workspace = support::workspace(&real, Comparison::BranchChanges { base: a, head: b });
    let picker = support::state(&workspace);
    let side = support::open(
        &workspace,
        &picker,
        b"shared",
        OriginalComparisonSideV1::Old,
    );
    picker
        .frame()
        .unwrap()
        .source()
        .install(&mut workspace)
        .unwrap();
    side.clone()
        .install(&mut workspace, side.commit_v1().clone())
        .unwrap();
    let selected = workspace.display_rows_v1().len() - 1;
    let mut app = App {
        view: View::Repository,
        workspace: Some(workspace),
        repository_list: crate::ui_table::ListState {
            selected,
            top: selected,
        },
        repository_content_focused: true,
        ..Default::default()
    };
    let before = real.snapshot();
    real.clear_trace();
    for width in [80, 120] {
        let text = rendered(&mut app, width);
        assert!(text.contains("Original old saved file"));
        assert!(text.contains("COMMON old bytes"));
        let footer = text.lines().last().unwrap();
        assert!(footer.starts_with(if width < 100 {
            " u Back  "
        } else {
            " u Back to comparison  "
        }));
        assert_eq!(app.workspace.as_ref().unwrap().revision, c);
        assert!(std::sync::Arc::ptr_eq(
            &side,
            app.workspace
                .as_ref()
                .unwrap()
                .original_diff_side_v1
                .as_ref()
                .unwrap()
        ));
        assert_eq!(app.repository_list.selected, selected);
    }
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
}
