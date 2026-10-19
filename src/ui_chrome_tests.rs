use super::*;
use crate::config::ThemeName;
use crate::identity::{ActorIdentity, ActorKind, InitialsChip};
use crate::theme::{ColorMode, CATPPUCCIN_MOCHA};
use ratatui::{backend::TestBackend, buffer::Buffer, style::Color, Terminal};

fn mocha() -> Theme {
    Theme::new(ThemeName::CatppuccinMocha, ColorMode::TrueColor)
}

fn row(buffer: &Buffer, y: u16) -> String {
    (0..buffer.area.width)
        .map(|x| buffer[(x, y)].symbol())
        .collect()
}

fn hex(color: u32) -> Color {
    crate::theme::rgb(color)
}

fn draw_app(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}

fn person(login: &str, name: &str) -> ActorIdentity {
    ActorIdentity {
        host: Some("github.com".into()),
        login: Some(login.into()),
        name: name.into(),
        email: None,
        avatar_url: None,
        kind: ActorKind::User,
        git_role: None,
        host_associated: true,
    }
}

#[test]
fn design_frames_v89_pane_block_is_rounded_with_breadcrumbs_and_a_non_color_focus_marker() {
    for theme in [mocha(), Theme::new(ThemeName::NoColor, ColorMode::NoColor)] {
        for focused in [true, false] {
            let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 4));
            ratatui::widgets::Widget::render(
                pane_block(theme, &["Inbox", "Fix playback"], focused),
                buffer.area,
                &mut buffer,
            );
            let top = row(&buffer, 0);
            assert!(top.starts_with('╭') && top.ends_with('╮'), "{top}");
            assert!(row(&buffer, 3).starts_with('╰') && row(&buffer, 3).ends_with('╯'));
            assert_eq!(top.contains("▶ Inbox › Fix playback"), focused, "{top}");
            assert!(top.contains("Inbox › Fix playback ─"), "{top}");
            // The title is bold when focused: the marker survives with no color at all.
            let title = top.find("Inbox").unwrap();
            let title_x = top[..title].chars().count() as u16;
            assert_eq!(
                buffer[(title_x, 0)].modifier.contains(Modifier::BOLD),
                focused
            );
            if theme.mode == ColorMode::TrueColor {
                let expected = if focused {
                    hex(CATPPUCCIN_MOCHA.magenta)
                } else {
                    hex(CATPPUCCIN_MOCHA.border)
                };
                assert_eq!(buffer[(0, 1)].fg, expected, "focused={focused}");
            }
        }
    }
    assert_eq!(crumb_title(&["A", "B"], false), " A › B ");
    assert_eq!(crumb_title(&["A"], true), " ▶ A ");
}

#[test]
fn design_frames_v89_header_merges_logo_tabs_and_navigation_and_keeps_hit_order() {
    for (width, height, rows) in [(140, 42, 1), (80, 24, 1), (40, 12, 2)] {
        let mut app = App::default();
        let buffer = draw_app(&mut app, width, height);
        assert_eq!(header_rows(&app, width), rows, "{width}");
        let header = row(&buffer, 0);
        assert!(header.starts_with(" ghtui "), "{header}");
        if width >= 80 {
            for tab in [
                "1 Inbox",
                "2 Repository",
                "3 Search",
                "4 Actions",
                "5 Dashboard",
            ] {
                assert!(header.contains(tab), "{tab} in {header}");
            }
            assert!(header.contains("5 Dashboard  Alt-← Back"), "{header}");
            assert!(!header.contains('['), "{header}");
        } else {
            assert!(row(&buffer, 1).starts_with("Alt-← Back"));
        }
        // Logo and the active tab are filled with the Inbox hue.
        assert_eq!(buffer[(1, 0)].bg, hex(CATPPUCCIN_MOCHA.magenta));
        let inbox = header.find("1 Inbox").unwrap() as u16;
        assert_eq!(buffer[(inbox, 0)].bg, hex(CATPPUCCIN_MOCHA.magenta));
        // Tabs, then Back: the registration order the hit tests and Tab order rely on.
        let views = app.hits[..5]
            .iter()
            .map(|hit| hit.action.clone())
            .collect::<Vec<_>>();
        assert_eq!(views, View::ALL.map(Action::View).to_vec());
        assert_eq!(
            app.hits[5].action,
            Action::Navigation(crate::navigation_trail::NavigationAction::Back)
        );
        // The content pane starts right under the header, with the context in its border.
        let top = row(&buffer, rows);
        assert!(top.starts_with("╭─ "), "{top}");
        assert!(top.contains("Inbox • focused view ─"), "{top}");
        // Long context keeps host and account whole and its most specific end.
        if width >= 80 {
            assert!(top.contains("─ github.com / personal queues / "), "{top}");
            assert!(top.ends_with("selected ─╮"), "{top}");
        }
        if width >= 140 {
            assert!(top.contains("github.com / personal queues / no repository selected"));
        }
        assert!(app
            .hits
            .iter()
            .all(|hit| hit.rect.right() <= width && hit.rect.bottom() <= height));
    }
}

#[test]
fn design_frames_v89_compact_scope_keeps_account_and_specific_end() {
    let head = "0123456789abcdef0123456789abcdef01234567";
    let scope = format!("github.com / @octocat / all repositories on host / org/repo#7 / {head}");
    assert_eq!(compact_scope(&scope, 200), scope);
    let narrow = compact_scope(&scope, 49);
    assert_eq!(crate::editor::display_width(&narrow), 49);
    assert!(narrow.starts_with("github.com / @octocat / …"), "{narrow}");
    assert!(narrow.ends_with(&head[head.len() - 20..]), "{narrow}");
    // A long first level (a local path) falls back to the shared prefix/suffix form.
    let local = format!("Local /{} / main / abcdef0", "deep/".repeat(20));
    let compact = compact_scope(&local, 40);
    assert!(compact.starts_with("Local ") && compact.ends_with("/ main / abcdef0"));
    assert!(crate::editor::display_width(&compact) <= 40);
    // A level cut inside keeps its last path segment, so the repository stays named, and one
    // with no segment boundary is dropped rather than shown mid-word.
    let inbox = "github.invalid / @octocat / example/repo0000 / My PRs";
    assert_eq!(
        compact_scope(inbox, 48),
        "github.invalid / @octocat / …/repo0000 / My PRs"
    );
    let run = "github.com / @octocat / owner-name / run 7 / abcdef0";
    assert_eq!(
        compact_scope(run, 46),
        "github.com / @octocat / … / run 7 / abcdef0"
    );
    assert_eq!(
        compact_scope(run, 42),
        "github.com / @octocat / … / run 7 / abcdef0"
    );
}

#[test]
fn design_frames_v89_active_tab_follows_view_and_focus_shows_marker() {
    let mut app = App::default();
    app.dispatch(Action::View(View::Actions));
    let buffer = draw_app(&mut app, 140, 42);
    let header = row(&buffer, 0);
    let actions = header.find("4 Actions").unwrap() as u16;
    assert_eq!(buffer[(actions, 0)].bg, hex(CATPPUCCIN_MOCHA.blue));
    assert_eq!(buffer[(1, 0)].bg, hex(CATPPUCCIN_MOCHA.blue));
    let inbox = header.find("1 Inbox").unwrap() as u16;
    assert_ne!(buffer[(inbox, 0)].bg, hex(CATPPUCCIN_MOCHA.magenta));
    // Inactive tabs show their key in their own view hue.
    assert_eq!(buffer[(inbox, 0)].fg, hex(CATPPUCCIN_MOCHA.magenta));
    let top = row(&buffer, 1);
    assert_eq!(
        app.actions_content_focused,
        top.contains("▶ Actions"),
        "{top}"
    );
    let border = if app.actions_content_focused {
        hex(CATPPUCCIN_MOCHA.blue)
    } else {
        hex(CATPPUCCIN_MOCHA.border)
    };
    assert_eq!(buffer[(0, 2)].fg, border);
    // Moving focus to the header removes the marker and the accent border.
    app.actions_content_focused = false;
    app.focus = 0;
    let buffer = draw_app(&mut app, 140, 42);
    assert!(!row(&buffer, 1).contains('▶'));
    assert_eq!(buffer[(0, 2)].fg, hex(CATPPUCCIN_MOCHA.border));
    // The keyboard-focused tab is underlined, the others are not.
    assert!(buffer[(inbox, 0)].modifier.contains(Modifier::UNDERLINED));
    assert!(!buffer[(actions, 0)].modifier.contains(Modifier::UNDERLINED));
}

#[test]
fn design_hierarchy_v94_section_header_keeps_provenance_and_partial_labels() {
    let theme = mocha();
    let text = |line: &Line| {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
    };
    let wide = section_header(
        theme,
        "Pull requests",
        "Fresh • fetched 19:51 UTC",
        Role::TextMuted,
        60,
    );
    assert_eq!(crate::editor::display_width(&text(&wide)), 60);
    assert!(text(&wide).starts_with("PULL REQUESTS"));
    assert!(text(&wide).ends_with("Fresh • fetched 19:51 UTC"));
    assert_eq!(wide.spans[0].style.fg, Some(hex(CATPPUCCIN_MOCHA.magenta)));
    assert_eq!(wide.spans[2].style.fg, Some(hex(CATPPUCCIN_MOCHA.muted)));
    // Narrow: the heading yields; the partial label stays readable.
    let narrow = section_header(
        theme,
        "Runs",
        "12 runs of 40 reported • PARTIAL",
        Role::Warning,
        34,
    );
    assert_eq!(text(&narrow), "12 runs of 40 reported • PARTIAL");
    assert_eq!(narrow.spans[0].style.fg, Some(hex(CATPPUCCIN_MOCHA.yellow)));
    let clipped = section_header(
        theme,
        "Runs",
        "Unknown coverage • not loaded",
        Role::Warning,
        12,
    );
    assert_eq!(text(&clipped), "Unknown cov…");
    assert_eq!(fit("abc", 3), "abc");
    assert_eq!(fit("abcd", 3), "ab…");
    let pair = label_value(theme, "Author", "@octocat", 9);
    assert_eq!(text(&pair), "Author    @octocat");
    assert_eq!(pair.spans[0].style.fg, Some(hex(CATPPUCCIN_MOCHA.muted)));
    assert_eq!(pair.spans[1].style.fg, Some(hex(CATPPUCCIN_MOCHA.fg)));
}

#[test]
fn design_chips_v95_initials_chip_has_a_stable_identity_hue_and_two_cells() {
    let octocat = person("octocat", "");
    let again = person("OctoCat", "");
    let hubot = person("hubot", "Hu Bot");
    assert_eq!(InitialsChip::slot(&octocat), InitialsChip::slot(&again));
    assert!(InitialsChip::slot(&octocat) < crate::theme::IDENTITY_HUES);
    assert_eq!(InitialsChip::text(&octocat, ColorMode::TrueColor), "OC");
    assert_eq!(InitialsChip::text(&hubot, ColorMode::TrueColor), "HB");
    assert_eq!(InitialsChip::text(&hubot, ColorMode::NoColor), "[HB]");
    let unknown = ActorIdentity {
        kind: ActorKind::Unknown,
        ..person("", "")
    };
    assert_eq!(InitialsChip::text(&unknown, ColorMode::TrueColor), "? ");
    // The hue is a pure function of the key: the same slot every session.
    let slots = [
        "alice", "bob", "carol", "dave", "erin", "frank", "grace", "heidi",
    ]
    .map(|login| InitialsChip::slot(&person(login, "")));
    assert_eq!(
        slots,
        ["alice", "bob", "carol", "dave", "erin", "frank", "grace", "heidi"]
            .map(|login| InitialsChip::slot(&person(login, "")))
    );
    assert!(slots.iter().any(|slot| *slot != slots[0]));
    for (theme, filled) in [
        (mocha(), true),
        (Theme::new(ThemeName::NoColor, ColorMode::NoColor), false),
    ] {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 1));
        ratatui::widgets::Widget::render(
            InitialsChip {
                identity: &octocat,
                theme,
            },
            buffer.area,
            &mut buffer,
        );
        let text = row(&buffer, 0);
        if filled {
            assert_eq!(text, "OC  ");
            let lane = CATPPUCCIN_MOCHA.lanes()[InitialsChip::slot(&octocat)];
            assert_eq!(buffer[(0, 0)].bg, hex(lane));
            assert_eq!(buffer[(0, 0)].fg, hex(CATPPUCCIN_MOCHA.ink));
            assert_ne!(buffer[(2, 0)].bg, hex(lane));
            assert_ne!(lane, CATPPUCCIN_MOCHA.red);
        } else {
            assert_eq!(text, "[OC]");
        }
    }
}

#[test]
fn design_chips_v95_chip_fill_meets_contrast_on_every_truecolor_theme() {
    let luminance = |color: Color| {
        let Color::Rgb(r, g, b) = color else {
            panic!("{color:?}")
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
    };
    for name in [
        ThemeName::CatppuccinMocha,
        ThemeName::TokyoNight,
        ThemeName::GruvboxDark,
    ] {
        let theme = Theme::new(name, ColorMode::TrueColor);
        let styles = (0..crate::theme::IDENTITY_HUES)
            .map(|slot| theme.identity_chip(slot))
            .chain(View::ALL.map(|view| theme.filled(Hue::for_view(view))));
        for style in styles {
            let (a, b) = (luminance(style.fg.unwrap()), luminance(style.bg.unwrap()));
            let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
            assert!(ratio >= 4.5, "{name:?} {style:?} {ratio}");
        }
    }
}

#[test]
fn design_overlay_inset_region_stays_inside_the_content_pane() {
    let screen = Rect::new(0, 0, 80, 24);
    let pane = Rect::new(0, 2, 80, 20);
    assert_eq!(overlay_region(screen, pane), Rect::new(2, 3, 76, 18));
    // A pane too small for a usable overlay keeps the whole-screen inset so Close stays visible.
    let tiny = Rect::new(0, 2, 80, 8);
    assert_eq!(
        overlay_region(screen, tiny),
        screen.inner(ratatui::layout::Margin::new(2, 2))
    );
}

#[test]
fn design_overlay_inset_help_and_commands_never_sit_on_the_pane_border() {
    for (width, height) in [(80u16, 24u16), (140, 42)] {
        for (overlay, title) in [
            (Action::Help, "Help • keyboard and mouse"),
            (Action::Palette, "Commands · current context"),
        ] {
            let mut app = App::default();
            app.dispatch(Action::View(View::Repository));
            app.dispatch(overlay);
            let buffer = draw_app(&mut app, width, height);
            let top = (0..height)
                .find(|y| buffer[(0, *y)].symbol() == "╭")
                .expect("pane top border");
            let pane_title = row(&buffer, top);
            assert!(!pane_title.contains("╭─╭"), "{pane_title}");
            assert!(!pane_title.contains(title), "{pane_title}");
            let overlay_row = (top + 1..height)
                .find(|y| row(&buffer, *y).contains(title))
                .unwrap_or_else(|| panic!("{title} at {width}x{height}"));
            let line = row(&buffer, overlay_row);
            assert_eq!(buffer[(0, overlay_row)].symbol(), "│", "{line}");
            assert!(line.contains(&format!("╭─ ▶ {title}")), "{line}");
            let left = line.chars().position(|c| c == '╭').unwrap() as u16;
            assert!(left >= 2, "{line}");
            // The overlay closes above the pane's bottom border, which stays intact.
            let bottom = (overlay_row + 1..height)
                .find(|y| buffer[(left, *y)].symbol() == "╰")
                .expect("overlay bottom border");
            assert!(bottom < height - 3, "bottom {bottom} at {width}x{height}");
            assert_eq!(buffer[(0, height - 3)].symbol(), "╰");
        }
    }
}

#[test]
fn design_overlay_inset_leaves_no_view_fragments_beside_a_full_width_overlay() {
    let mut app = App::default();
    app.dispatch(Action::View(View::Repository));
    app.dispatch(Action::Help);
    let buffer = draw_app(&mut app, 80, 24);
    let title = (0..24)
        .find(|y| row(&buffer, *y).contains("Help • keyboard and mouse"))
        .expect("Help overlay");
    // At 80 columns Help fills the overlay region: the columns between it and the pane
    // borders stay blank on every overlay row.
    for y in title..title + 10 {
        let line = row(&buffer, y);
        assert_eq!(buffer[(1, y)].symbol(), " ", "{line}");
        assert_eq!(buffer[(78, y)].symbol(), " ", "{line}");
    }
}

#[test]
fn design_overlay_inset_search_entry_focuses_content_until_tab() {
    let mut app = App::default();
    let _ = draw_app(&mut app, 80, 24);
    app.focus = 0;
    app.dispatch(Action::View(View::Search));
    assert!(app.search_content_focused);
    let buffer = draw_app(&mut app, 80, 24);
    let top = (0..24).find(|y| buffer[(0, *y)].symbol() == "╭").unwrap();
    assert!(row(&buffer, top).contains("▶ Search • focused view"));
    // Tab moves focus to the next header control: the marker leaves the pane.
    app.handle_event(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::from(crossterm::event::KeyCode::Tab),
    ));
    assert!(!app.search_content_focused);
    assert_eq!(app.focus, 1);
    let buffer = draw_app(&mut app, 80, 24);
    assert!(!row(&buffer, top).contains('▶'));
}

/// A row already painted with other text, as list rows paint their label under the chip slot.
fn stray_row(width: u16) -> Buffer {
    let mut buffer = Buffer::empty(Rect::new(0, 0, width, 1));
    buffer.set_string(0, 0, "x".repeat(usize::from(width)), Style::default());
    buffer
}

#[test]
fn design_chips_v95_chip_overwrites_every_cell_of_its_slot() {
    let requester = person("requester", "");
    for (theme, expected) in [
        (mocha(), "RE    "),
        (Theme::new(ThemeName::NoColor, ColorMode::NoColor), "[RE]  "),
    ] {
        let mut buffer = stray_row(6);
        ratatui::widgets::Widget::render(
            InitialsChip {
                identity: &requester,
                theme,
            },
            buffer.area,
            &mut buffer,
        );
        // A styled empty paragraph used to keep the earlier symbols: "RE d" / "OCmm".
        assert_eq!(row(&buffer, 0), expected);
    }
}

#[test]
fn design_chips_v95_chip_label_has_exactly_one_space_at_every_width() {
    use crate::identity::ListAvatar;
    let requester = person("requester", "");
    let label = "@requester PR description";
    for (theme, chip) in [
        (mocha(), "RE"),
        (Theme::new(ThemeName::NoColor, ColorMode::NoColor), "[RE]"),
    ] {
        let full = format!("{chip} {label}");
        let style = theme.style(crate::theme::SemanticToken::Normal);
        for width in 0..=40u16 {
            let mut buffer = stray_row(width);
            ListAvatar {
                identity: &requester,
                image: None,
                theme,
            }
            .render_with_label(buffer.area, &mut buffer, label, style);
            let text = row(&buffer, 0);
            let cells = usize::from(width);
            let expected = if cells >= full.chars().count() {
                // Wide rows: the whole line, then blank cells, never leftover row text.
                format!("{full}{}", " ".repeat(cells - full.chars().count()))
            } else if cells <= chip.len() {
                chip.chars().take(cells).collect()
            } else if cells == chip.len() + 1 {
                format!("{chip} ")
            } else {
                // Cut with an ellipsis, still after exactly one blank cell.
                format!("{chip} {}", fit(label, cells - chip.len() - 1))
            };
            assert_eq!(text, expected, "width {width}");
            assert!(!text.contains('x'), "stray cell at width {width}: {text}");
            if cells > chip.len() + 1 && cells < full.chars().count() {
                assert!(text.ends_with('…'), "width {width}: {text}");
            }
            if theme.mode != ColorMode::NoColor && width >= 3 {
                let lane = CATPPUCCIN_MOCHA.lanes()[InitialsChip::slot(&requester)];
                assert_eq!(buffer[(0, 0)].bg, hex(lane));
                assert_eq!(buffer[(1, 0)].bg, hex(lane));
                // The gap carries the row's background, not the chip fill.
                assert_ne!(buffer[(2, 0)].bg, hex(lane));
            }
        }
    }
}

#[test]
fn design_chips_v95_chip_lead_reports_its_cells_and_clamps_to_the_area() {
    use crate::identity::ListAvatar;
    let octocat = person("octocat", "");
    let avatar = |theme| ListAvatar {
        identity: &octocat,
        image: None,
        theme,
    };
    assert_eq!(avatar(mocha()).cells(), 2);
    let plain = Theme::new(ThemeName::NoColor, ColorMode::NoColor);
    assert_eq!(avatar(plain).cells(), 4);
    for (width, lead, text) in [
        (0, 0, ""),
        (1, 1, "O"),
        (2, 2, "OC"),
        (3, 3, "OC "),
        (8, 3, "OC xxxxx"),
    ] {
        let mut buffer = stray_row(width);
        let used = avatar(mocha()).render_lead(buffer.area, &mut buffer, Style::default());
        assert_eq!(
            (used, row(&buffer, 0)),
            (lead, text.to_owned()),
            "width {width}"
        );
    }
    // An area outside the buffer paints nothing and never panics.
    let mut buffer = stray_row(4);
    let used = avatar(mocha()).render_lead(Rect::new(10, 3, 6, 1), &mut buffer, Style::default());
    assert_eq!((used, row(&buffer, 0)), (0, "xxxx".to_owned()));
}

fn cells(buffer: &Buffer, rect: Rect) -> String {
    (rect.x..rect.right())
        .map(|x| buffer[(x, rect.y)].symbol())
        .collect()
}

fn source_app() -> (tempfile::TempDir, App) {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
    };
    git(&["init", "--initial-branch=main"]);
    git(&["config", "user.name", "Header Author"]);
    git(&["config", "user.email", "header@example.invalid"]);
    std::fs::write(dir.path().join("file.txt"), "saved\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-m", "saved header source"]);
    let workspace = crate::workspace::Workspace::load(
        dir.path(),
        crate::workspace::RepositoryMode::Graph,
        "HEAD",
        None,
    )
    .unwrap()
    .unwrap();
    let source = std::sync::Arc::new(
        crate::repository_source_identity::PreparedRepositorySourceIdentity::prepare(&workspace)
            .unwrap(),
    );
    let app = App {
        view: View::Repository,
        repository_source_identity: Some(source),
        workspace: Some(workspace),
        repository_content_focused: true,
        ..Default::default()
    };
    (dir, app)
}

fn press(
    app: &mut App,
    code: crossterm::event::KeyCode,
    modifiers: crossterm::event::KeyModifiers,
) {
    app.handle_event(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(code, modifiers),
    ));
}

fn click_at(app: &mut App, rect: Rect) {
    app.handle_event(crossterm::event::Event::Mouse(
        crossterm::event::MouseEvent {
            kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
            column: rect.x + rect.width / 2,
            row: rect.y,
            modifiers: crossterm::event::KeyModifiers::NONE,
        },
    ));
}

#[test]
fn design_header_v89_key_tokens_follow_the_configured_binding() {
    assert_eq!(key_token("alt+left"), "Alt-←");
    assert_eq!(key_token("ctrl+b"), "Ctrl-b");
    assert_eq!(key_token("alt+ctrl+pagedown"), "Alt-Ctrl-PgDn");
    assert_eq!(key_token("f5"), "F5");
    assert_eq!(key_token("backspace"), "Backspace");
    assert_eq!(key_token("q"), "q");
    assert_eq!(key_token("+"), "+");
    assert_eq!(key_token("ctrl++"), "Ctrl-+");
}

#[test]
fn design_header_v89_back_is_a_one_row_key_token_clicked_or_pressed_alike() {
    for (width, height) in [(80, 24), (140, 42)] {
        for mouse in [false, true] {
            let mut app = App::default();
            let buffer = draw_app(&mut app, width, height);
            assert_eq!(header_rows(&app, width), 1, "{width}");
            let header = row(&buffer, 0);
            assert!(!header.contains('[') && !header.contains(']'), "{header}");
            // The pane border follows on the very next row: nothing wrapped.
            assert!(row(&buffer, 1).starts_with("╭─"), "{}", row(&buffer, 1));
            let back = app.hits[5].clone();
            assert_eq!(
                back.action,
                Action::Navigation(crate::navigation_trail::NavigationAction::Back)
            );
            assert_eq!(back.rect.y, 0);
            assert!(back.rect.right() <= width);
            assert_eq!(cells(&buffer, back.rect), "Alt-← Back");
            // Footer style: the key in the accent hue and bold, the label muted.
            let key = buffer[(back.rect.x, 0)].clone();
            assert!(key.modifier.contains(Modifier::BOLD));
            assert_eq!(Some(key.fg), app.theme().role(Role::Accent).fg);
            let label = &buffer[(back.rect.right() - 1, 0)];
            assert!(!label.modifier.contains(Modifier::BOLD));
            assert_ne!(label.fg, key.fg);
            if mouse {
                click_at(&mut app, back.rect);
                assert_eq!(app.focus, 5);
            } else {
                press(
                    &mut app,
                    crossterm::event::KeyCode::Left,
                    crossterm::event::KeyModifiers::ALT,
                );
            }
            // No earlier page: the same explanation either way, nothing navigated.
            assert_eq!(app.overlay, Some(crate::app::Overlay::Notice), "{mouse}");
            assert!(
                app.notice.contains("No earlier navigation page"),
                "{}",
                app.notice
            );
            assert_eq!(app.view, View::Inbox);
        }
    }
}

#[test]
fn design_header_v89_back_keeps_its_label_when_a_long_key_does_not_fit() {
    let mut app = App::default();
    let mut bindings = crate::config::default_bindings();
    bindings.insert("back".into(), "alt+ctrl+pagedown".into());
    app.set_bindings(bindings);
    let buffer = draw_app(&mut app, 80, 24);
    assert_eq!(header_rows(&app, 80), 1);
    let back = app.hits[5].rect;
    assert_eq!(cells(&buffer, back), "Back");
    assert!(back.right() <= 80);
    assert!(row(&buffer, 1).starts_with("╭─"));
    let buffer = draw_app(&mut app, 140, 42);
    assert_eq!(cells(&buffer, app.hits[5].rect), "Alt-Ctrl-PgDn Back");
    // Keyboard focus underlines the whole token.
    app.focus = 5;
    app.github_content_focused = false;
    let buffer = draw_app(&mut app, 140, 42);
    let back = app.hits[5].rect;
    assert!((back.x..back.right()).all(|x| buffer[(x, 0)].modifier.contains(Modifier::UNDERLINED)));
}

#[test]
fn design_header_v89_source_identity_closes_the_pane_border_and_keeps_hit_order() {
    for (width, height, label) in [(80, 24, "I Source"), (140, 42, "I Source identity")] {
        for mouse in [false, true] {
            let (_dir, mut app) = source_app();
            let buffer = draw_app(&mut app, width, height);
            assert_eq!(header_rows(&app, width), 1);
            let header = row(&buffer, 0);
            assert!(header.contains("Alt-← Back"), "{header}");
            assert!(
                !header.contains("Source") && !header.contains('['),
                "{header}"
            );
            // Tabs, Source identity, then Back: the Tab order is unchanged.
            assert_eq!(app.hits[5].action, Action::RepositorySource);
            assert_eq!(
                app.hits[6].action,
                Action::Navigation(crate::navigation_trail::NavigationAction::Back)
            );
            let source = app.hits[5].rect;
            assert_eq!(source.y, 1);
            assert_eq!(cells(&buffer, source), label);
            let top = row(&buffer, 1);
            assert!(top.starts_with("╭─ ▶ Repository • focused view"), "{top}");
            assert!(top.ends_with(&format!("─ {label} ─╮")), "{top}");
            // The scope keeps the rest of the border, one dash clear of the token.
            assert!(top.contains("no repository selected ─ "), "{top}");
            assert!(top.contains(&format!(" ─ {label}")), "{top}");
            assert_eq!(crate::editor::display_width(&top), usize::from(width));
            assert!(buffer[(source.x, 1)].modifier.contains(Modifier::BOLD));
            // Tab focus on the token underlines it.
            app.repository_content_focused = false;
            app.focus = 5;
            let buffer = draw_app(&mut app, width, height);
            assert!((source.x..source.right())
                .all(|x| buffer[(x, 1)].modifier.contains(Modifier::UNDERLINED)));
            if mouse {
                // The overlay takes focus once open; the effect below is the evidence.
                click_at(&mut app, source);
            } else {
                app.repository_content_focused = true;
                press(
                    &mut app,
                    crossterm::event::KeyCode::Char('I'),
                    crossterm::event::KeyModifiers::NONE,
                );
            }
            assert_eq!(
                app.overlay,
                Some(crate::app::Overlay::RepositorySource),
                "{mouse}"
            );
        }
    }
}

#[test]
fn design_header_v89_narrow_terminals_keep_back_on_a_second_row() {
    let mut app = App::default();
    // Between the tabs' end and 80 columns Back drops its key before it would wrap.
    let buffer = draw_app(&mut app, 75, 20);
    assert_eq!(header_rows(&app, 75), 1);
    assert_eq!(cells(&buffer, app.hits[5].rect), "Back");
    assert!(row(&buffer, 1).starts_with("╭─"));
    let buffer = draw_app(&mut app, 60, 20);
    assert_eq!(header_rows(&app, 60), 2);
    assert!(
        row(&buffer, 1).starts_with("Alt-← Back"),
        "{}",
        row(&buffer, 1)
    );
    assert!(row(&buffer, 2).starts_with("╭─"));
}
