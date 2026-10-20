//! TASK-99.2: the History table's columns at 80 and 140 columns, chip spans with an explicit
//! `+N`, and chips that keep their fill (or normal video without color) on the `▌` row.
use super::*;
use crate::{
    config::ThemeName,
    git::GitActor,
    theme::{ColorMode, Palette},
    ui_table::{layout, Placed},
};
use ratatui::{backend::TestBackend, style::Modifier, Terminal};

fn theme(mode: ColorMode) -> Theme {
    Theme::new(ThemeName::CatppuccinMocha, mode)
}
fn chip(kind: Kind, label: &str) -> RefChip {
    RefChip {
        kind,
        label: label.into(),
        current: false,
        synced: label.ends_with(refs::SYNCED),
    }
}
fn parts(theme: Theme, chips: Vec<RefChip>) -> Parts {
    Parts {
        graph: vec![("●─╮".into(), theme.graph(0))],
        id: "a4a6e459449ecd7022fbfc2db0bd21b8b6202cbd".into(),
        chips,
        subject: "Merge feature/parser into main".into(),
        author: crate::identity::ActorIdentity::local(
            &GitActor {
                name: "Ada Lovelace".into(),
                email: "ada@example.invalid".into(),
                timestamp: 0,
            },
            crate::identity::GitRole::Author,
        ),
        age: "17h".into(),
        focused: false,
    }
}
fn shown(placed: &[Placed]) -> Vec<&'static str> {
    placed
        .iter()
        .map(|placed| HISTORY_COLUMNS[placed.column].title)
        .collect()
}

#[test]
fn history_refs_v992_columns_keep_message_and_drop_age_then_author_when_narrow() {
    // GRAPH 7, COMMIT 7, MESSAGE with chips 60, AUTHOR `KN Kateřina Novák` 17, AGE 3.
    let natural = [7, 7, 60, 17, 3];
    // 140x42: the pane interior less the bar slot.
    let wide = layout(&HISTORY_COLUMNS, &natural, 136);
    assert_eq!(
        shown(&wide),
        ["GRAPH", "COMMIT", "MESSAGE", "AUTHOR", "AGE"]
    );
    assert_eq!(wide[2].width, 60, "the message reaches its natural width");
    assert_eq!(wide[3].width, 17);
    // 80x24: every column stays, the message keeps well over its minimum.
    let narrow = layout(&HISTORY_COLUMNS, &natural, 76);
    assert_eq!(
        shown(&narrow),
        ["GRAPH", "COMMIT", "MESSAGE", "AUTHOR", "AGE"]
    );
    assert!(narrow[2].width >= 30, "{narrow:?}");
    assert!(narrow[2].width > narrow[3].width);
    // Narrower still: AGE leaves first, then AUTHOR; MESSAGE never drops below 24.
    let tighter = layout(&HISTORY_COLUMNS, &natural, 58);
    assert_eq!(shown(&tighter), ["GRAPH", "COMMIT", "MESSAGE", "AUTHOR"]);
    let tightest = layout(&HISTORY_COLUMNS, &natural, 50);
    assert_eq!(shown(&tightest), ["GRAPH", "COMMIT", "MESSAGE"]);
    assert!(tightest[2].width >= 24);
}

#[test]
fn history_refs_v992_message_cell_shows_chips_then_subject_and_counts_overflow() {
    let theme = theme(ColorMode::TrueColor);
    let row = parts(
        theme,
        vec![
            chip(Kind::Head, "HEAD"),
            chip(Kind::Local, "main ⇅"),
            chip(Kind::Remote, "origin/feature/parser"),
            chip(Kind::Tag, "v1.0.0"),
        ],
    );
    let full = message(&row, None, theme);
    assert_eq!(
        full.text,
        " HEAD   main ⇅   origin/feature/parser   v1.0.0  Merge feature/parser into main"
    );
    let styles = full
        .spans
        .iter()
        .filter(|(text, _)| text.trim().len() > 1 && !text.starts_with("Merge"))
        .map(|(_, style)| *style)
        .collect::<Vec<_>>();
    assert_eq!(
        styles,
        [
            chip_style(theme, Kind::Head),
            chip_style(theme, Kind::Local),
            chip_style(theme, Kind::Remote),
            chip_style(theme, Kind::Tag),
        ]
    );
    // Every ref kind reads differently.
    for (index, a) in styles.iter().enumerate() {
        for b in &styles[index + 1..] {
            assert_ne!(a, b);
        }
    }
    // 40 cells: the subject keeps 16, the chips get the rest and the overflow says how many.
    let cut = message(&row, Some(40), theme);
    assert_eq!(
        cut.text,
        " HEAD   main ⇅   +2  Merge feature/parser into main"
    );
    assert!(cut
        .spans
        .iter()
        .any(|(text, style)| text == " +2 " && *style == theme.badge(SemanticToken::Normal)));
}

fn render(mode: ColorMode, selected: bool) -> (ratatui::buffer::Buffer, Theme) {
    let theme = theme(mode);
    let row = parts(
        theme,
        vec![chip(Kind::Local, "main ⇅"), chip(Kind::Tag, "v1.0.0")],
    );
    let rows = [row];
    let table = Table {
        columns: &HISTORY_COLUMNS,
        header: true,
        focused: true,
        glyphs: false,
        empty: None,
    };
    let built = rows
        .iter()
        .map(|part| super::row(part, None, theme))
        .collect::<Vec<_>>();
    let natural = crate::ui_table::natural_widths(&HISTORY_COLUMNS, &built, theme.mode);
    let mut terminal = Terminal::new(TestBackend::new(100, 3)).unwrap();
    terminal
        .draw(|frame| {
            crate::ui_table::draw_table_at(
                frame,
                frame.area(),
                &table,
                &natural,
                1,
                |index| super::row(&rows[index], Some(60), theme),
                selected.then_some(0),
                0,
                theme,
            );
        })
        .unwrap();
    (terminal.backend().buffer().clone(), theme)
}
fn line(buffer: &ratatui::buffer::Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

#[test]
fn history_refs_v992_row_aligns_columns_and_selected_chips_keep_their_fill() {
    let (buffer, theme) = render(ColorMode::TrueColor, true);
    let header = line(&buffer, 0);
    let body = line(&buffer, 1);
    for title in ["GRAPH", "COMMIT", "MESSAGE", "AUTHOR", "AGE"] {
        assert!(header.contains(title), "{header}");
    }
    let at = |text: &str, needle: &str| text[..text.find(needle).unwrap()].chars().count();
    assert_eq!(at(&header, "COMMIT"), at(&body, "a4a6e45"));
    assert!(!body.contains("a4a6e459"), "rows show the 7-character SHA");
    assert_eq!(at(&header, "MESSAGE"), at(&body, " main ⇅"), "{body}");
    assert_eq!(at(&header, "AUTHOR"), at(&body, "AL Ada Lovelace"));
    assert!(body.trim_end().ends_with("17h"));
    assert!(body.starts_with('▌'), "{body}");
    let selection = crate::theme::rgb(
        Palette::for_theme(ThemeName::CatppuccinMocha)
            .unwrap()
            .selection,
    );
    let main = at(&body, "main ⇅") as u16;
    let tag = at(&body, "v1.0.0") as u16;
    assert_eq!(
        buffer[(main, 1)].bg,
        chip_style(theme, Kind::Local).bg.unwrap()
    );
    assert_eq!(
        buffer[(tag, 1)].bg,
        chip_style(theme, Kind::Tag).bg.unwrap()
    );
    let subject = at(&body, "Merge") as u16;
    assert_eq!(buffer[(subject, 1)].bg, selection);
    assert!(!buffer[(subject, 1)].modifier.contains(Modifier::REVERSED));

    // Without color the selected row is reversed and its chips turn back to normal video.
    let (buffer, _) = render(ColorMode::NoColor, true);
    let body = line(&buffer, 1);
    let main = at(&body, "main ⇅") as u16;
    let subject = at(&body, "Merge") as u16;
    assert!(!buffer[(main, 1)].modifier.contains(Modifier::REVERSED));
    assert!(buffer[(subject, 1)].modifier.contains(Modifier::REVERSED));
    let (buffer, _) = render(ColorMode::NoColor, false);
    assert!(buffer[(main, 1)].modifier.contains(Modifier::REVERSED));
    assert!(!buffer[(subject, 1)].modifier.contains(Modifier::REVERSED));
}

#[test]
fn history_refs_v992_span_cells_end_in_an_ellipsis_at_the_column_edge() {
    let theme = theme(ColorMode::TrueColor);
    let style = theme.role(Role::Text);
    let mut buffer = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 10, 1));
    crate::ui_table::put_spans_for_tests(
        &mut buffer,
        0,
        0,
        10,
        &[
            (" main ".into(), theme.chip(Role::Branch)),
            (" Fix the parser".into(), style),
        ],
    );
    let text: String = (0..10).map(|x| buffer[(x, 0)].symbol()).collect();
    assert_eq!(text, " main  Fi…");
    assert_eq!(buffer[(9, 0)].style().fg, style.fg);
    // A run that fills the column exactly while more follows still shows the cut.
    let mut buffer = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 6, 1));
    crate::ui_table::put_spans_for_tests(
        &mut buffer,
        0,
        0,
        6,
        &[
            (" main ".into(), theme.chip(Role::Branch)),
            ("x".into(), style),
        ],
    );
    let text: String = (0..6).map(|x| buffer[(x, 0)].symbol()).collect();
    assert_eq!(text, " main…");
}
