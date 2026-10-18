use super::*;
use crate::app::{Action, App, View};

const TRUECOLOR_THEMES: [ThemeName; 3] = [
    ThemeName::CatppuccinMocha,
    ThemeName::TokyoNight,
    ThemeName::GruvboxDark,
];

const STATUS: [SemanticToken; 6] = [
    SemanticToken::Success,
    SemanticToken::Failure,
    SemanticToken::Running,
    SemanticToken::Pending,
    SemanticToken::Cancelled,
    SemanticToken::Changed,
];

fn luminance(color: Color) -> f64 {
    let Color::Rgb(r, g, b) = color else {
        panic!("expected an RGB color, got {color:?}");
    };
    let channel = |v: u8| {
        let value = f64::from(v) / 255.0;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn contrast(style: Style) -> f64 {
    let (a, b) = (luminance(style.fg.unwrap()), luminance(style.bg.unwrap()));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

#[test]
fn design_tokens_v88_semantic_tokens_map_onto_roles() {
    let theme = Theme::new(ThemeName::CatppuccinMocha, ColorMode::TrueColor);
    for (token, role) in [
        (SemanticToken::Normal, Role::Text),
        (SemanticToken::Muted, Role::TextMuted),
        (SemanticToken::Added, Role::Success),
        (SemanticToken::Success, Role::Success),
        (SemanticToken::Removed, Role::Failure),
        (SemanticToken::Failure, Role::Failure),
        (SemanticToken::Running, Role::Warning),
        (SemanticToken::Changed, Role::Warning),
        (SemanticToken::Pending, Role::TextMuted),
        (SemanticToken::Head, Role::Head),
        (SemanticToken::Reference, Role::Branch),
        (SemanticToken::Accent, Role::Accent),
        (SemanticToken::Focus, Role::Accent),
    ] {
        assert_eq!(token.role(), role, "{token:?}");
        assert_eq!(theme.style(token).fg, theme.role(role).fg, "{token:?}");
        assert_eq!(theme.style(token).bg, theme.base().bg, "{token:?}");
    }
    // Focus and Accent share the hue; only Focus carries the non-color underline.
    let focus = theme.style(SemanticToken::Focus);
    let accent = theme.style(SemanticToken::Accent);
    assert!(focus.add_modifier.contains(Modifier::UNDERLINED));
    assert!(!accent.add_modifier.contains(Modifier::UNDERLINED));
    assert!(accent.add_modifier.contains(Modifier::BOLD));
    // Selection keeps reversing so tokens stay legible inside selected rows; the role is a bar.
    assert!(theme
        .style(SemanticToken::Selection)
        .add_modifier
        .contains(Modifier::REVERSED));
    assert_eq!(
        theme.role(Role::Selection).bg,
        Some(rgb(CATPPUCCIN_MOCHA.selection))
    );
    assert_eq!(theme.role(Role::Border).fg, Some(rgb(0x45475a)));
    assert_eq!(theme.role(Role::TextDim).fg, Some(rgb(0x6c7086)));
}

#[test]
fn design_tokens_v88_no_color_keeps_glyphs_labels_and_modifiers() {
    for name in ThemeName::ALL {
        let theme = Theme::new(name, ColorMode::NoColor);
        for role in Role::ALL {
            let style = theme.role(role);
            assert_eq!((style.fg, style.bg), (None, None), "{name:?} {role:?}");
        }
        assert!(theme
            .role(Role::Selection)
            .add_modifier
            .contains(Modifier::REVERSED));
        assert!(theme
            .role(Role::Accent)
            .add_modifier
            .contains(Modifier::BOLD));
        assert!(theme
            .style(SemanticToken::Failure)
            .add_modifier
            .contains(Modifier::BOLD));
        for token in STATUS {
            assert!(!token.symbol().is_empty());
            assert!(!token.label().is_empty());
            let text = Theme::badge_text(token);
            assert!(text.contains(token.symbol()) && text.contains(token.badge_label()));
            let badge = theme.badge(token);
            assert_eq!((badge.fg, badge.bg), (None, None));
            assert!(badge.add_modifier.contains(Modifier::REVERSED));
        }
    }
    // The NoColor theme forces monochrome whatever the terminal offers.
    assert_eq!(
        Theme::new(ThemeName::NoColor, ColorMode::TrueColor).mode,
        ColorMode::NoColor
    );
    assert_eq!(Theme::badge_text(SemanticToken::Success), " ✓ PASSED ");
    assert_eq!(Theme::badge_text(SemanticToken::Failure), " ✗ FAILURE ");
}

#[test]
fn design_tokens_v88_views_use_roles_not_raw_colors() {
    // Only the theme, image canvases and the pass-through terminal panes may name colors.
    const ALLOWED: [&str; 4] = [
        "theme.rs",
        "identity.rs",
        "read_only_pty_pane_v4.rs",
        "owned_pty_viewer_v4.rs",
    ];
    const RAW: [&str; 18] = [
        "Color::Rgb(",
        "Color::Indexed(",
        "Color::Black",
        "Color::Red",
        "Color::Green",
        "Color::Yellow",
        "Color::Blue",
        "Color::Magenta",
        "Color::Cyan",
        "Color::Gray",
        "Color::DarkGray",
        "Color::LightRed",
        "Color::LightGreen",
        "Color::LightYellow",
        "Color::LightBlue",
        "Color::LightMagenta",
        "Color::LightCyan",
        "Color::White",
    ];
    fn walk(dir: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
    }
    let mut files = vec![];
    walk(
        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );
    assert!(files.len() > 50);
    let mut offenders = vec![];
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        if ALLOWED.contains(&name.as_str()) || name.ends_with("_tests.rs") {
            continue;
        }
        let body = std::fs::read_to_string(&path).unwrap();
        // Inline `mod tests` blocks may build fixtures from raw colors.
        let body = body.split("#[cfg(test)]").next().unwrap_or_default();
        for (line, text) in body.lines().enumerate() {
            if RAW.iter().any(|raw| text.contains(raw)) {
                offenders.push(format!("{}:{}", path.display(), line + 1));
            }
        }
    }
    assert!(offenders.is_empty(), "raw colors in views: {offenders:?}");
}

#[test]
fn design_theme_v98_palettes_are_the_mockup_hex_values() {
    for (palette, expected) in [
        (
            CATPPUCCIN_MOCHA,
            [
                0x1e1e2e, 0xcdd6f4, 0xa6adc8, 0x6c7086, 0x45475a, 0x363a4f, 0x89b4fa, 0xa6e3a1,
                0xf38ba8, 0xf9e2af, 0x94e2d5, 0xcba6f7, 0xfab387, 0xf5c2e7,
            ],
        ),
        (
            TOKYO_NIGHT,
            [
                0x1a1b26, 0xc0caf5, 0x9aa5ce, 0x565f89, 0x3b4261, 0x283457, 0x7aa2f7, 0x9ece6a,
                0xf7768e, 0xe0af68, 0x7dcfff, 0xbb9af7, 0xff9e64, 0xff79c6,
            ],
        ),
        (
            GRUVBOX_DARK,
            [
                0x282828, 0xebdbb2, 0xa89984, 0x665c54, 0x504945, 0x3c3836, 0x83a598, 0xb8bb26,
                0xfb4934, 0xfabd2f, 0x8ec07c, 0xd3869b, 0xfe8019, 0xd3869b,
            ],
        ),
    ] {
        assert_eq!(
            [
                palette.bg,
                palette.fg,
                palette.muted,
                palette.dim,
                palette.border,
                palette.selection,
                palette.blue,
                palette.green,
                palette.red,
                palette.yellow,
                palette.cyan,
                palette.magenta,
                palette.orange,
                palette.pink,
            ],
            expected
        );
    }
    for name in TRUECOLOR_THEMES {
        let palette = Palette::for_theme(name).unwrap();
        let base = Theme::new(name, ColorMode::TrueColor).base();
        assert_eq!(base.fg, Some(rgb(palette.fg)));
        assert_eq!(base.bg, Some(rgb(palette.bg)));
        assert_eq!(
            Theme::new(name, ColorMode::TrueColor).backdrop().0,
            (palette.bg >> 16) as u8
        );
    }
    assert_eq!(Palette::for_theme(ThemeName::Ansi16), None);
}

#[test]
fn design_theme_v98_view_hues_and_fixed_data_colors() {
    for name in TRUECOLOR_THEMES {
        let palette = Palette::for_theme(name).unwrap();
        for (view, hue) in [
            (View::Inbox, palette.magenta),
            (View::Repository, palette.orange),
            (View::Search, palette.cyan),
            (View::Actions, palette.blue),
            (View::Dashboard, palette.green),
        ] {
            let theme = Theme::new(name, ColorMode::TrueColor).with_view(view);
            assert_eq!(
                theme.role(Role::Accent).fg,
                Some(rgb(hue)),
                "{name:?} {view:?}"
            );
            assert_eq!(theme.role(Role::BorderFocus).fg, Some(rgb(hue)));
            assert_eq!(theme.style(SemanticToken::Focus).fg, Some(rgb(hue)));
            // Data colors never follow the view.
            for (role, fixed) in [
                (Role::Branch, palette.cyan),
                (Role::Commit, palette.yellow),
                (Role::Event, palette.pink),
                (Role::Number, palette.orange),
                (Role::Success, palette.green),
                (Role::Failure, palette.red_text),
            ] {
                assert_eq!(theme.role(role).fg, Some(rgb(fixed)), "{name:?} {role:?}");
            }
        }
    }
    // App::theme follows the active view.
    let mut app = App::new();
    for view in View::ALL {
        app.view = view;
        assert_eq!(app.theme().hue, Hue::for_view(view));
    }
    // 16 colors keep one named hue per view.
    let low = Theme::new(ThemeName::Ansi16, ColorMode::TrueColor);
    let hues: std::collections::BTreeSet<String> = View::ALL
        .into_iter()
        .map(|view| format!("{:?}", low.with_view(view).role(Role::Accent).fg))
        .collect();
    assert_eq!(hues.len(), 5);
    // Lanes are a palette list: eight distinct colors, wrapping by slot.
    let theme = Theme::new(ThemeName::CatppuccinMocha, ColorMode::TrueColor);
    let lanes: std::collections::BTreeSet<String> = theme
        .lanes()
        .iter()
        .map(|style| format!("{:?}", style.fg))
        .collect();
    assert_eq!(lanes.len(), 8);
    assert_eq!(theme.graph(3), theme.graph(11));
}

#[test]
fn design_theme_v98_colorterm_detection_and_downsampling_keep_meaning() {
    for (term, colorterm, expected) in [
        (
            Some("xterm-256color"),
            Some("truecolor"),
            ColorMode::TrueColor,
        ),
        (Some("xterm-256color"), Some("24bit"), ColorMode::TrueColor),
        (Some("xterm-256color"), None, ColorMode::Indexed256),
        (Some("screen"), Some("256color"), ColorMode::Indexed256),
        (Some("xterm"), None, ColorMode::LowColor),
        (None, None, ColorMode::LowColor),
        (Some("dumb"), Some("truecolor"), ColorMode::NoColor),
    ] {
        assert_eq!(ColorMode::for_terminal(term, colorterm, false), expected);
    }
    assert_eq!(
        ColorMode::for_terminal(Some("xterm-256color"), Some("truecolor"), true),
        ColorMode::NoColor
    );
    // 256: palette colors land on the cube or gray ramp, never the terminal-defined 0..15.
    assert_eq!(
        downsample(Color::Rgb(255, 0, 0), ColorMode::Indexed256),
        Color::Indexed(196)
    );
    assert_eq!(
        downsample(Color::Rgb(0, 0, 0), ColorMode::Indexed256),
        Color::Indexed(16)
    );
    assert_eq!(
        downsample(Color::Rgb(128, 128, 128), ColorMode::Indexed256),
        Color::Indexed(244)
    );
    for name in TRUECOLOR_THEMES {
        let theme = Theme::new(name, ColorMode::Indexed256);
        let mut seen = std::collections::BTreeSet::new();
        for role in [
            Role::Success,
            Role::Failure,
            Role::Warning,
            Role::Info,
            Role::Text,
        ] {
            let Some(Color::Indexed(index)) = theme.role(role).fg else {
                panic!("{name:?} {role:?} is not indexed");
            };
            assert!(index >= 16, "{name:?} {role:?}");
            seen.insert(index);
        }
        assert_eq!(
            seen.len(),
            5,
            "{name:?} status colors stay distinct in 256 colors"
        );
        assert!(matches!(theme.base().bg, Some(Color::Indexed(16..=255))));
    }
    // 16: roles use a fixed named mapping on the terminal's own background.
    for name in [
        ThemeName::CatppuccinMocha,
        ThemeName::TokyoNight,
        ThemeName::GruvboxDark,
        ThemeName::Ansi16,
    ] {
        for mode in [ColorMode::LowColor, ColorMode::TrueColor] {
            let theme = Theme::new(name, mode);
            if mode == ColorMode::TrueColor && name != ThemeName::Ansi16 {
                continue;
            }
            assert_eq!(theme.base().bg, Some(Color::Reset));
            for (token, color) in [
                (SemanticToken::Success, Color::Green),
                (SemanticToken::Failure, Color::Red),
                (SemanticToken::Running, Color::Yellow),
                (SemanticToken::Reference, Color::Cyan),
            ] {
                assert_eq!(theme.style(token).fg, Some(color), "{name:?} {token:?}");
            }
        }
    }
    // Arbitrary content keeps its hue family, so pastels do not wash out to white.
    for (hex, index) in [
        (0xf38ba8, 9),
        (0xa6e3a1, 10),
        (0xf9e2af, 11),
        (0x89b4fa, 12),
        (0x7dcfff, 14),
        (0x1e1e2e, 0),
        (0xcdd6f4, 15),
    ] {
        assert_eq!(
            downsample(rgb(hex), ColorMode::LowColor),
            Color::Indexed(index),
            "{hex:06x}"
        );
    }
    assert_eq!(
        downsample(Color::Indexed(196), ColorMode::LowColor),
        Color::Indexed(9)
    );
    assert_eq!(downsample(Color::Reset, ColorMode::LowColor), Color::Reset);
    assert_eq!(downsample(rgb(0x123456), ColorMode::NoColor), Color::Reset);
}

#[test]
fn design_theme_v98_text_accents_and_badges_reach_4_5_contrast() {
    for name in TRUECOLOR_THEMES {
        for view in View::ALL {
            let theme = Theme::new(name, ColorMode::TrueColor).with_view(view);
            for role in [
                Role::Text,
                Role::TextMuted,
                Role::Accent,
                Role::Success,
                Role::Failure,
                Role::Warning,
                Role::Info,
                Role::Commit,
                Role::Branch,
                Role::Head,
                Role::Event,
                Role::Number,
            ] {
                let ratio = contrast(theme.role(role));
                assert!(ratio >= 4.5, "{name:?} {view:?} {role:?} {ratio:.2}");
            }
            let selected = theme.role(Role::Selection);
            assert!(contrast(selected) >= 4.5, "{name:?} selection");
            for token in STATUS {
                let ratio = contrast(theme.badge(token));
                assert!(ratio >= 4.5, "{name:?} {token:?} badge {ratio:.2}");
            }
        }
    }
}

#[test]
fn design_theme_v98_config_names_aliases_and_settings_cycle() {
    use crate::config::Config;
    for (text, expected) in [
        ("dark", ThemeName::CatppuccinMocha),
        ("light", ThemeName::Ansi16),
        ("catppuccin_mocha", ThemeName::CatppuccinMocha),
        ("tokyo_night", ThemeName::TokyoNight),
        ("gruvbox_dark", ThemeName::GruvboxDark),
        ("ansi16", ThemeName::Ansi16),
        ("no_color", ThemeName::NoColor),
    ] {
        let config: Config = toml::from_str(&format!("theme = \"{text}\"")).unwrap();
        assert_eq!(config.theme, expected, "{text}");
    }
    assert!(toml::from_str::<Config>("theme = \"solarized\"").is_err());
    assert_eq!(Config::default().theme, ThemeName::CatppuccinMocha);
    // Aliases are read-only: saving writes the bundled name.
    let config: Config = toml::from_str("theme = \"dark\"").unwrap();
    let written = toml::to_string(&config).unwrap();
    assert!(
        written.contains("theme = \"catppuccin_mocha\""),
        "{written}"
    );
    let labels: std::collections::BTreeSet<&str> =
        ThemeName::ALL.iter().map(|name| name.label()).collect();
    assert_eq!(labels.len(), ThemeName::ALL.len());
    for (index, name) in ThemeName::ALL.into_iter().enumerate() {
        assert_eq!(name.index(), index);
        assert_eq!(ThemeName::from_index(index + ThemeName::ALL.len()), name);
    }
    // Settings cycles every bundled theme and wraps; the app theme follows immediately.
    let mut app = App::new();
    app.color_mode = ColorMode::TrueColor;
    let mut seen = vec![app.theme().name];
    for _ in 0..ThemeName::ALL.len() {
        app.dispatch(Action::SettingsTheme);
        seen.push(app.theme().name);
    }
    assert_eq!(seen[..5], ThemeName::ALL);
    assert_eq!(seen[5], ThemeName::CatppuccinMocha);
}
