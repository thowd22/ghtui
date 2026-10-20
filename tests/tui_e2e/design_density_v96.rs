//! TASK-96: one Inbox journey run once per density, by keyboard and by mouse. Comfortable (the
//! default) splits the list and its preview from 120 columns, keeps the `PULL REQUESTS` section
//! header and two blank cells between table columns; Compact keeps the whole list, puts the
//! freshness on the queue summary row and one blank cell between columns. Settings › Density
//! switches live, Save settings writes `density` to config.toml, and no gh command runs.
//! Comfortable items are two lines (the table row over a muted `Open · updated …` detail
//! line) and Compact keeps one; a click on either line, or Enter, opens the same PR.
use super::*;
use crate::support::with_density;
use ghtui::config::Density;

/// Cells from the `PR` column title to the `TITLE` column title on the Inbox header row.
fn title_offset(screen: &str) -> Option<usize> {
    let header = screen
        .lines()
        .find(|line| line.contains(" PR ") && line.contains("TITLE"))?;
    let chars = header.chars().collect::<Vec<_>>();
    let find = |needle: &str| {
        (0..chars.len()).find(|start| {
            chars[*start..]
                .iter()
                .collect::<String>()
                .starts_with(needle)
        })
    };
    Some(find("TITLE")? - find("PR ")?)
}

/// The screen line of the Inbox table row for `reference` (the line that also holds its
/// title, so the split preview's own `owner/repo#N` line is never taken for it).
fn item_row(screen: &str, reference: &str) -> Option<usize> {
    let title = match reference {
        "org/alpha#7" => "Improve auth",
        "org/beta#7" => "Repair parser",
        _ => "Update docs",
    };
    screen
        .lines()
        .position(|line| line.contains(reference) && line.contains(title))
}

/// Whether the screen shows the Inbox at `density`; `wide` screens split at Comfortable.
fn laid_out(screen: &str, density: Density, wide: bool) -> bool {
    let preview = screen.contains("─ Preview");
    let section = screen.contains("PULL REQUESTS");
    let summary = screen
        .lines()
        .any(|line| line.contains("Queue:") && line.contains("Fresh • fetched"));
    // `org/alpha#7` is the widest PR reference: 11 cells, then the column gap.
    let gap = title_offset(screen).map(|offset| offset.saturating_sub(11));
    let rows = ["org/alpha#7", "org/beta#7", "org/gamma#9"]
        .iter()
        .all(|reference| screen.contains(reference));
    // Comfortable: each PR's row is followed by its detail line; Compact: by the next PR.
    let below = |reference: &str| {
        let lines = screen.lines().collect::<Vec<_>>();
        item_row(screen, reference)
            .and_then(|row| lines.get(row + 1).copied())
            .unwrap_or_default()
    };
    let two_line = below("org/alpha#7").contains("Open · updated")
        && below("org/beta#7").contains("Open · updated")
        && below("org/gamma#9").contains("Open · updated");
    let one_line =
        below("org/alpha#7").contains("org/beta#7") && !screen.contains("Open · updated");
    rows && match density {
        Density::Comfortable => {
            preview == wide && section && !summary && gap == Some(2) && two_line
        }
        Density::Compact => !preview && !section && summary && gap == Some(1) && one_line,
    }
}

fn wait_laid_out(session: &mut Session, density: Density, wide: bool) {
    session
        .try_wait_for_condition(
            |screen| laid_out(screen, density, wide),
            &format!("the Inbox at {density:?} density (wide: {wide})"),
            std::time::Duration::from_secs(8),
        )
        .unwrap_or_else(|error| panic!("{error}\n{}", session.screen()));
}

/// Opens Settings, toggles Density from `from`, saves, and checks the saved file.
fn toggle_density(session: &mut Session, fixture: &Fixture, mouse: bool, from: Density) {
    let to = from.next();
    if mouse {
        // The loaded Inbox key bar leaves Settings to the Commands palette, clicked.
        session.invoke_via_palette(true, "Open settings");
    } else {
        session.send_key(b"s");
    }
    session.wait_for_screen("Settings • local preferences");
    let current = format!("[Density: {}]", from.label());
    session.wait_for_screen(&current);
    if mouse {
        session.click_visible_action(&current);
    } else {
        // Host, Repository set, Theme, then Density.
        session.send_key(b"\t\t\t\r");
    }
    session.wait_for_screen(&format!("[Density: {}]", to.label()));
    // Theme is a separate control: toggling Density leaves it alone.
    session.wait_for_screen("[Theme: Catppuccin Mocha]");
    if mouse {
        session.click_visible_action("[Save settings]");
    } else {
        session.send_key(b"\t\r");
    }
    session.wait_for_screen("Settings saved locally");
    let saved =
        ghtui::config::Config::load(&fixture.root.join("config/ghtui/config.toml")).unwrap();
    assert_eq!(saved.density, to);
    assert_eq!(saved.host, "github.invalid", "other preferences are kept");
    assert!(!saved.avatars);
}

fn density_journey(density: Density, mouse: bool) {
    let fixture = configured();
    let saved = with_density(&fixture, density);
    assert_eq!(saved.density, density);
    // The journey ends by opening `Improve auth` (its summary and conversation pages).
    let mut steps = initial(false);
    steps.extend(pages().into_iter().take(7));
    fixture.scripted_gh(&steps);
    let mut session = Session::launch(&fixture, 140, 42);
    session.wait_for_screen("ghtui");
    refresh(&mut session, mouse);
    session.wait_for_screen("Improve auth");
    journal(&mut session, &fixture, 7);
    wait_laid_out(&mut session, density, true);
    let title = crate::support::view_title("Inbox");
    session.wait_for_selected_row(&title, "Improve auth");
    // The cursor moves the same way at both densities; it never reads.
    session.send_key(b"\x1b[B");
    session.wait_for_selected_row(&title, "Repair parser");

    // Settings switches the density live and saves it; the selection is kept.
    toggle_density(&mut session, &fixture, mouse, density);
    wait_laid_out(&mut session, density.next(), true);
    session.wait_for_selected_row(&title, "Repair parser");

    // At 80x24 neither density splits; the toggled one still applies.
    session.resize(80, 24);
    wait_laid_out(&mut session, density.next(), false);
    session.wait_for_selected_row(&title, "Repair parser");
    toggle_density(&mut session, &fixture, mouse, density.next());
    wait_laid_out(&mut session, density, false);
    session.wait_for_selected_row(&title, "Repair parser");
    // And back at 140x42 the original density's layout returns.
    session.resize(140, 42);
    wait_laid_out(&mut session, density, true);

    // Presenting and switching density ran no gh command.
    journal(&mut session, &fixture, 7);

    // Either line of an item opens it: the detail line below `Improve auth` by click at
    // Comfortable (its row at Compact), or Up then Enter by keyboard.
    if mouse {
        let row = item_row(&session.screen(), "org/alpha#7").expect("the Improve auth row") as u16;
        let line = match density {
            Density::Comfortable => row + 1,
            Density::Compact => row,
        };
        assert!(
            density == Density::Compact
                || session
                    .screen()
                    .lines()
                    .nth(usize::from(line))
                    .is_some_and(|text| text.contains("Open · updated")),
            "{}",
            session.screen()
        );
        session.click(20, line);
    } else {
        session.send_key(b"\x1b[A");
        session.wait_for_selected_row(&title, "Improve auth");
        session.send_key(b"\r");
    }
    session.wait_for_screen("1 comments");
    journal(&mut session, &fixture, 14);
    session.quit_and_assert_restored_terminal(mouse);
}

#[test]
fn design_density_v96_comfortable_keyboard_inbox_split_then_compact_at_140x42_and_80x24() {
    density_journey(Density::Comfortable, false)
}
#[test]
fn design_density_v96_comfortable_mouse_inbox_split_then_compact_at_140x42_and_80x24() {
    density_journey(Density::Comfortable, true)
}
#[test]
fn design_density_v96_compact_keyboard_inbox_whole_list_then_comfortable_at_140x42_and_80x24() {
    density_journey(Density::Compact, false)
}
#[test]
fn design_density_v96_compact_mouse_inbox_whole_list_then_comfortable_at_140x42_and_80x24() {
    density_journey(Density::Compact, true)
}
