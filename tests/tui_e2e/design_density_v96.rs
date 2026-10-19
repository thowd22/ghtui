//! TASK-96: one Inbox journey run once per density, by keyboard and by mouse. Comfortable (the
//! default) splits the list and its preview from 120 columns, keeps the `PULL REQUESTS` section
//! header and two blank cells between table columns; Compact keeps the whole list, puts the
//! freshness on the queue summary row and one blank cell between columns. Settings › Density
//! switches live, Save settings writes `density` to config.toml, and no gh command runs.
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
    rows && match density {
        Density::Comfortable => preview == wide && section && !summary && gap == Some(2),
        Density::Compact => !preview && !section && summary && gap == Some(1),
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
    fixture.scripted_gh(&initial(false));
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
