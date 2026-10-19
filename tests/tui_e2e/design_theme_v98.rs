//! TASK-98: bundled themes chosen in Settings recolor the real terminal cells, keep each view's
//! hue, survive resizing and persist under their bundled names.
use crate::support::{Fixture, Session};
use std::time::{Duration, Instant};
use vt100::Color;

const MOCHA: Palette = Palette {
    bg: 0x1e1e2e,
    hues: [0xcba6f7, 0xfab387, 0x94e2d5, 0x89b4fa, 0xa6e3a1],
};
const TOKYO: Palette = Palette {
    bg: 0x1a1b26,
    hues: [0xbb9af7, 0xff9e64, 0x7dcfff, 0x7aa2f7, 0x9ece6a],
};
const GRUVBOX: Palette = Palette {
    bg: 0x282828,
    hues: [0xd3869b, 0xfe8019, 0x8ec07c, 0x83a598, 0xb8bb26],
};

/// Background plus the Inbox, Repository, Search, Actions and Dashboard hues.
struct Palette {
    bg: u32,
    hues: [u32; 5],
}

fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl Palette {
    fn colors(&self) -> (Color, [Color; 5]) {
        (rgb(self.bg), self.hues.map(rgb))
    }
}

/// 16 colors: the terminal background and the named view hues.
const ANSI16: (Color, [Color; 5]) = (
    Color::Default,
    [
        Color::Idx(5),
        Color::Idx(9),
        Color::Idx(6),
        Color::Idx(4),
        Color::Idx(2),
    ],
);

/// The five swatch dots after `[Theme: …]` carry the view hues on the theme background.
fn swatch_matches(frame: &vt100::Screen, label: &str, expected: (Color, [Color; 5])) -> bool {
    let contents = frame.contents();
    let Some((row, line)) = contents
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains(label))
    else {
        return false;
    };
    let start = line[..line.find(label).unwrap()].chars().count() + label.chars().count();
    let (bg, hues) = expected;
    hues.iter().enumerate().all(|(index, hue)| {
        frame
            .cell(row as u16, (start + 2 + index * 2) as u16)
            .is_some_and(|cell| {
                cell.contents() == "●" && cell.fgcolor() == *hue && cell.bgcolor() == bg
            })
    })
}

/// The ` ghtui ` logo chip is filled with the active view's hue; the row behind it keeps the
/// theme background.
fn title_matches(frame: &vt100::Screen, hue: Color, bg: Color) -> bool {
    frame
        .cell(0, 1)
        .is_some_and(|cell| cell.contents() == "g" && cell.bold() && cell.bgcolor() == hue)
        && frame.cell(0, 7).is_some_and(|cell| cell.bgcolor() == bg)
}

fn wait_cells(
    session: &mut Session,
    after: usize,
    label: &str,
    predicate: impl Fn(&vt100::Screen) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(1));
        if session.completed_cells_end() > after
            && session.completed_cells_screen().is_some_and(&predicate)
        {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "No completed frame: {label}\n{}",
            session.screen()
        );
    }
}

fn next_theme(session: &mut Session, mouse: bool, current: &str, next: &str) {
    if mouse {
        session.click_visible_action(&format!("[Theme: {current}]"));
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_screen(&format!("[Theme: {next}]"));
}

fn theme_journey(mouse: bool, size: (u16, u16), resized: (u16, u16)) {
    let fixture = Fixture::new();
    let config_path = fixture.root.join("config/ghtui/config.toml");
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    // The pre-palette name stays readable and opens the default theme.
    std::fs::write(&config_path, "version = 1\ntheme = \"dark\"\n").unwrap();
    let mut session = Session::launch(&fixture, size.0, size.1);
    session.wait_for_screen("ghtui");
    let (bg, hues) = MOCHA.colors();
    wait_cells(&mut session, 0, "Catppuccin Mocha Inbox title", |frame| {
        title_matches(frame, hues[0], bg)
    });

    session.choose(mouse, "[s Settings]", b"s");
    session.wait_for_screen("[Theme: Catppuccin Mocha]");
    let after = session.completed_cells_end();
    if mouse {
        session.click_visible_action("[Theme: Catppuccin Mocha]");
    } else {
        session.send_key(b"\t\t\r");
    }
    session.wait_for_screen("[Theme: Tokyo Night]");
    wait_cells(&mut session, after, "Tokyo Night swatch", |frame| {
        swatch_matches(frame, "[Theme: Tokyo Night]", TOKYO.colors())
    });

    let after = session.completed_cells_end();
    next_theme(&mut session, mouse, "Tokyo Night", "Gruvbox Dark");
    wait_cells(&mut session, after, "Gruvbox Dark swatch", |frame| {
        swatch_matches(frame, "[Theme: Gruvbox Dark]", GRUVBOX.colors())
    });
    // The palette survives a resize while Settings stays open.
    let after = session.completed_cells_end();
    session.resize(resized.0, resized.1);
    wait_cells(&mut session, after, "Gruvbox Dark after resize", |frame| {
        swatch_matches(frame, "[Theme: Gruvbox Dark]", GRUVBOX.colors())
    });

    let after = session.completed_cells_end();
    next_theme(&mut session, mouse, "Gruvbox Dark", "16 colors");
    wait_cells(&mut session, after, "16-color swatch", |frame| {
        swatch_matches(frame, "[Theme: 16 colors]", ANSI16)
    });

    let after = session.completed_cells_end();
    next_theme(&mut session, mouse, "16 colors", "No color");
    wait_cells(
        &mut session,
        after,
        "No color without swatch or color",
        |frame| {
            let contents = frame.contents();
            let row = contents
                .lines()
                .position(|line| line.contains("[Theme: No color]"));
            row.is_some_and(|row| {
                !contents.lines().nth(row).unwrap().contains('●')
                    && (0..frame.size().1).all(|column| {
                        frame.cell(row as u16, column).is_some_and(|cell| {
                            cell.fgcolor() == Color::Default && cell.bgcolor() == Color::Default
                        })
                    })
            })
        },
    );

    // Wrapping returns to the default before choosing Tokyo Night again.
    next_theme(&mut session, mouse, "No color", "Catppuccin Mocha");
    next_theme(&mut session, mouse, "Catppuccin Mocha", "Tokyo Night");
    let after = session.completed_cells_end();
    if mouse {
        session.click_visible_action("[Save settings]");
    } else {
        // Past Density to Save settings.
        session.send_key(b"\t\t\r");
    }
    session.wait_for_screen("Settings saved locally");
    let (bg, hues) = TOKYO.colors();
    wait_cells(
        &mut session,
        after,
        "Tokyo Night Inbox title after save",
        |frame| title_matches(frame, hues[0], bg) && !frame.contents().contains("[Theme:"),
    );
    let saved = std::fs::read_to_string(&config_path).unwrap();
    assert!(saved.contains("theme = \"tokyo_night\""), "{saved}");
    assert_eq!(
        ghtui::config::Config::load(&config_path).unwrap().theme,
        ghtui::config::ThemeName::TokyoNight
    );
    assert!(
        session.inspect_command_journal(&fixture).is_empty(),
        "choosing a theme must not run any GitHub command"
    );

    // Actions owns blue: the title and the filled active tab follow the view.
    let after = session.completed_cells_end();
    if mouse {
        let (column, row) = session.locate("4 Actions").unwrap();
        session.click(column, row);
    } else {
        session.send_key(b"4");
    }
    wait_cells(&mut session, after, "Tokyo Night Actions hue", |frame| {
        let tab = frame
            .contents()
            .lines()
            .enumerate()
            .find_map(|(row, line)| {
                line.find("4 Actions")
                    .map(|at| (row as u16, line[..at].chars().count() as u16))
            });
        title_matches(frame, hues[3], bg)
            && tab.is_some_and(|(row, column)| {
                frame
                    .cell(row, column)
                    .is_some_and(|cell| cell.bgcolor() == hues[3] && cell.bold())
            })
    });
    session.capture_completed_terminal_v1(if mouse {
        "design-theme-v98-tokyo-actions-mouse"
    } else {
        "design-theme-v98-tokyo-actions-keyboard"
    });
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_theme_v98_keyboard_settings_switch_recolors_cells_140x42_then_80x24() {
    theme_journey(false, (140, 42), (80, 24));
}

#[test]
fn design_theme_v98_mouse_settings_switch_recolors_cells_80x24_then_140x42() {
    theme_journey(true, (80, 24), (140, 42));
}
