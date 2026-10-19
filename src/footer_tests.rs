//! TASK-90 key bar contracts: pure layout, compaction order, disabled/Mode filtering,
//! key resolution and the hit regions the painted bar registers.
use super::*;
use crate::theme::{ColorMode, Theme};
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::{backend::TestBackend, layout::Rect, Terminal};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Probe {
    Refresh,
    Details,
    Back,
    Repositories,
    Save,
    Mode,
    Help,
    Commands,
    Quit,
}

fn tail() -> Vec<KeyControl<Probe>> {
    vec![
        KeyControl::new("?", "Help", Probe::Help)
            .with_priority(PINNED)
            .in_group(ControlGroup::Global),
        KeyControl::new(":", "Commands", Probe::Commands)
            .with_compact("Cmds")
            .with_priority(PINNED)
            .in_group(ControlGroup::Global),
        KeyControl::new("q", "Quit", Probe::Quit)
            .with_priority(PINNED)
            .in_group(ControlGroup::Global),
    ]
}

fn view() -> Vec<KeyControl<Probe>> {
    let mut controls = vec![
        KeyControl::new("r", "Refresh", Probe::Refresh).with_priority(10),
        KeyControl::new("i", "Details", Probe::Details).with_priority(12),
        KeyControl::new("b", "Back", Probe::Back)
            .with_priority(13)
            .in_group(ControlGroup::Navigate),
        KeyControl::new("R", "Repositories", Probe::Repositories)
            .with_compact("Repos")
            .with_priority(24),
        KeyControl::new("S", "Save dashboard", Probe::Save).with_priority(40),
    ];
    controls.extend(tail());
    controls
}

fn tokens(controls: &[KeyControl<Probe>], width: u16) -> Vec<String> {
    layout(controls, width)
        .into_iter()
        .map(|placed| controls[placed.index].token(placed.compact))
        .collect()
}

/// The painted row text the placements describe, for exact spacing checks.
fn row(controls: &[KeyControl<Probe>], width: u16) -> String {
    let mut text = " ".repeat(usize::from(width));
    for placed in layout(controls, width) {
        let token = controls[placed.index].token(placed.compact);
        let start = usize::from(placed.x);
        text.replace_range(start..start + token.len(), &token);
    }
    text
}

fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn design_footer_v90_layout_keeps_full_labels_at_140_and_compacts_at_80() {
    let controls = view();
    assert_eq!(
        row(&controls, 140).trim_end(),
        " r Refresh  i Details  b Back  R Repositories  S Save dashboard  ? Help  : Commands  q Quit"
    );
    let narrow = row(&controls, 80);
    assert_eq!(
        narrow.trim_end(),
        " r Refresh  i Details  b Back  R Repos  S Save dashboard  ? Help  : Cmds  q Quit"
    );
    for width in [140, 80] {
        let placed = layout(&controls, width);
        assert_eq!(placed[0].x, 1, "one lead column before the first token");
        for pair in placed.windows(2) {
            assert_eq!(
                pair[1].x,
                pair[0].x + pair[0].width + 2,
                "tokens are separated by exactly two spaces at {width}"
            );
        }
        assert!(placed.iter().all(|p| p.x + p.width <= width));
        assert!(placed.iter().all(|p| p.compact == (width < COMPACT_WIDTH)));
    }
}

#[test]
fn design_footer_v90_compaction_drops_highest_priority_number_first_then_rightmost_tie() {
    let controls = view();
    // 80 columns fit everything compact; narrower rows shed Save, then Repositories, then Back.
    assert_eq!(tokens(&controls, 70).len(), 7);
    assert!(!tokens(&controls, 70).contains(&"S Save dashboard".to_owned()));
    let at_56 = tokens(&controls, 56);
    assert_eq!(
        at_56,
        [
            "r Refresh",
            "i Details",
            "b Back",
            "? Help",
            ": Cmds",
            "q Quit"
        ]
    );
    let at_40 = tokens(&controls, 40);
    assert_eq!(at_40, ["r Refresh", "? Help", ": Cmds", "q Quit"]);
    // Equal priorities leave from the right.
    let tied = vec![
        KeyControl::new("a", "Alpha", Probe::Refresh).with_priority(20),
        KeyControl::new("b", "Bravo", Probe::Details).with_priority(20),
        KeyControl::new("c", "Charlie", Probe::Back).with_priority(20),
    ];
    assert_eq!(tokens(&tied, 20), ["a Alpha", "b Bravo"]);
    assert_eq!(tokens(&tied, 12), ["a Alpha"]);
    // The pinned tail never leaves for width, and is never cut mid-token.
    for width in [24, 23, 16, 8, 1, 0] {
        let placed = layout(&controls, width);
        assert!(placed
            .iter()
            .all(|p| controls[p.index].priority == PINNED && p.x + p.width <= width));
    }
    assert_eq!(tokens(&controls, 23), ["? Help", ": Cmds", "q Quit"]);
    assert_eq!(tokens(&controls, 16), ["? Help", ": Cmds"]);
}

#[test]
fn design_footer_v90_disabled_and_mode_controls_leave_the_bar() {
    let mut controls = view();
    controls[1] = controls[1]
        .clone()
        .disabled_by(Some("Select a card first".into()));
    controls.insert(
        0,
        KeyControl::new("W", "Workflows", Probe::Mode).in_group(ControlGroup::Mode),
    );
    let shown = tokens(&controls, 140);
    assert!(!shown.iter().any(|token| token.contains("Details")));
    assert!(!shown.iter().any(|token| token.contains("Workflows")));
    assert!(shown.contains(&"r Refresh".to_owned()));
    // `disabled_by(None)` leaves the control enabled.
    let enabled = KeyControl::new("x", "Extra", Probe::Refresh).disabled_by(None);
    assert!(enabled.disabled.is_none());
}

#[test]
fn design_footer_v90_resolve_key_acts_and_a_disabled_key_explains() {
    let mut controls = view();
    controls[1] = controls[1]
        .clone()
        .disabled_by(Some("Select a card first".into()));
    controls.push(KeyControl::new("W", "Workflows", Probe::Mode).in_group(ControlGroup::Mode));
    controls.push(KeyControl::new("F10", "Reviews", Probe::Save));
    controls.push(KeyControl::new("Space", "Toggle", Probe::Back));
    let none = KeyModifiers::NONE;
    assert_eq!(
        resolve(&controls, &press(KeyCode::Char('r'), none)),
        Some(Resolved::Act(Probe::Refresh))
    );
    assert_eq!(
        resolve(&controls, &press(KeyCode::Char('i'), none)),
        Some(Resolved::Explain("Select a card first".into()))
    );
    // Uppercase keys arrive with Shift; Mode controls resolve even though the bar omits them.
    assert_eq!(
        resolve(&controls, &press(KeyCode::Char('R'), KeyModifiers::SHIFT)),
        Some(Resolved::Act(Probe::Repositories))
    );
    assert_eq!(
        resolve(&controls, &press(KeyCode::Char('W'), KeyModifiers::SHIFT)),
        Some(Resolved::Act(Probe::Mode))
    );
    assert_eq!(
        resolve(&controls, &press(KeyCode::F(10), none)),
        Some(Resolved::Act(Probe::Save))
    );
    assert_eq!(
        resolve(&controls, &press(KeyCode::Char(' '), none)),
        Some(Resolved::Act(Probe::Back))
    );
    // Ctrl/Alt chords, releases and unbound keys never resolve.
    assert_eq!(
        resolve(&controls, &press(KeyCode::Char('r'), KeyModifiers::CONTROL)),
        None
    );
    assert_eq!(
        resolve(&controls, &press(KeyCode::Char('r'), KeyModifiers::ALT)),
        None
    );
    let mut release = press(KeyCode::Char('r'), none);
    release.kind = KeyEventKind::Release;
    assert_eq!(resolve(&controls, &release), None);
    assert_eq!(resolve(&controls, &press(KeyCode::Char('z'), none)), None);
    assert!(key_matches(
        "Ctrl-R",
        &press(KeyCode::Char('r'), KeyModifiers::CONTROL)
    ));
    assert!(key_matches("Esc", &press(KeyCode::Esc, none)));
    assert!(key_matches("PgDn", &press(KeyCode::PageDown, none)));
    assert!(!key_matches("F10", &press(KeyCode::F(1), none)));
    // App actions: a disabled key becomes Action::Explain with the same reason.
    let app_controls = vec![
        KeyControl::new("r", "Refresh", Action::Refresh),
        KeyControl::new("S", "Settings", Action::Settings).disabled_by(Some("Busy".into())),
    ];
    assert_eq!(
        resolve_key(&app_controls, press(KeyCode::Char('r'), none)),
        Some(Action::Refresh)
    );
    assert_eq!(
        resolve_key(
            &app_controls,
            press(KeyCode::Char('S'), KeyModifiers::SHIFT)
        ),
        Some(Action::Explain("Busy".into()))
    );
}

#[test]
fn design_footer_v90_render_returns_hit_regions_in_order_with_key_accent_and_focus_underline() {
    let theme = Theme::new(
        crate::config::ThemeName::CatppuccinMocha,
        ColorMode::TrueColor,
    );
    for (width, height) in [(140, 42), (80, 24)] {
        let controls = view();
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        let mut targets = Vec::new();
        let area = Rect::new(0, height - 1, width, 1);
        terminal
            .draw(|frame| targets = render(frame, area, theme, &controls, Some(1)))
            .unwrap();
        let placed = layout(&controls, width);
        assert_eq!(targets.len(), placed.len());
        let buffer = terminal.backend().buffer();
        for (position, ((rect, action), placed)) in targets.iter().zip(&placed).enumerate() {
            let control = &controls[placed.index];
            assert_eq!(*action, control.action);
            assert_eq!(
                *rect,
                Rect::new(placed.x, height - 1, placed.width, 1),
                "the whole token is the click target"
            );
            let text = (rect.x..rect.right())
                .map(|x| buffer[(x, rect.y)].symbol())
                .collect::<String>();
            assert_eq!(text, control.token(placed.compact));
            let key = &buffer[(rect.x, rect.y)];
            let label = &buffer[(rect.right() - 1, rect.y)];
            assert_eq!(key.fg, theme.role(Role::Accent).fg.unwrap());
            assert!(key.modifier.contains(Modifier::BOLD));
            assert_eq!(label.fg, theme.role(Role::TextMuted).fg.unwrap());
            assert_eq!(
                key.modifier.contains(Modifier::UNDERLINED),
                position == 1,
                "only the focused token is underlined"
            );
        }
    }
}

#[test]
fn design_footer_v90_tab_strip_holds_mode_controls_and_disabled_tabs_explain() {
    let mut app = crate::app::App {
        view: crate::app::View::Actions,
        ..Default::default()
    };
    let controls = vec![
        KeyControl::new("R", "Runs", Action::Refresh)
            .in_group(ControlGroup::Mode)
            .with_selected(true),
        KeyControl::new("W", "Workflows", Action::Settings)
            .in_group(ControlGroup::Mode)
            .disabled_by(Some("No workflows are loaded".into())),
        KeyControl::new("r", "Refresh", Action::Refresh),
    ];
    let mut terminal = Terminal::new(TestBackend::new(60, 3)).unwrap();
    terminal
        .draw(|frame| {
            crate::ui_chrome::tab_strip(frame, &mut app, Rect::new(0, 1, 60, 1), &controls)
        })
        .unwrap();
    assert_eq!(app.hits.len(), 2, "only Mode controls become tabs");
    assert_eq!(app.hits[0].action, Action::Refresh);
    assert_eq!(
        app.hits[1].action,
        Action::Explain("No workflows are loaded".into())
    );
    let text = (0..60)
        .map(|x| terminal.backend().buffer()[(x, 1)].symbol())
        .collect::<String>();
    assert!(text.starts_with(" R Runs   W Workflows "));
    assert!(!text.contains("Refresh"));
}

#[test]
fn design_footer_v90_tab_strip_marks_tabs_that_do_not_fit() {
    let tabs = |count: usize| {
        (0..count)
            .map(|index| {
                KeyControl::new(index.to_string(), "Tab", Action::Refresh)
                    .in_group(ControlGroup::Mode)
            })
            .collect::<Vec<_>>()
    };
    let paint = |controls: &[KeyControl], width: u16| {
        let mut app = crate::app::App::default();
        let mut terminal = Terminal::new(TestBackend::new(width, 1)).unwrap();
        terminal
            .draw(|frame| {
                crate::ui_chrome::tab_strip(frame, &mut app, Rect::new(0, 0, width, 1), controls)
            })
            .unwrap();
        let text = (0..width)
            .map(|x| terminal.backend().buffer()[(x, 0)].symbol().to_owned())
            .collect::<String>();
        (app.hits.len(), text)
    };
    // Compact tabs are 5 columns, two apart: three fit 19 columns whole.
    let (hits, text) = paint(&tabs(3), 19);
    assert_eq!(hits, 3, "{text}");
    assert!(!text.contains('…'), "{text}");
    // Five tabs in the same row: a dim `…` says the row continues, never a cut tab.
    let (hits, text) = paint(&tabs(5), 19);
    assert_eq!(hits, 2, "{text}");
    assert_eq!(text.trim_end(), "0 Tab  1 Tab  …");
}

#[test]
fn design_footer_v90_bar_under_an_open_dialog_is_painted_inactive() {
    let mut app = crate::app::App::default();
    let theme = app.theme();
    let controls = vec![KeyControl::new("r", "Refresh", Action::Refresh)];
    for (overlay, accent) in [(None, true), (Some(crate::app::Overlay::Help), false)] {
        app.overlay = overlay;
        let mut terminal = Terminal::new(TestBackend::new(40, 1)).unwrap();
        terminal
            .draw(|frame| draw(frame, &mut app, Rect::new(0, 0, 40, 1), &controls))
            .unwrap();
        let key = terminal.backend().buffer()[(1, 0)].clone();
        assert_eq!(key.symbol(), "r");
        assert_eq!(
            key.fg == theme.role(Role::Accent).fg.unwrap(),
            accent,
            "{overlay:?}"
        );
    }
}
