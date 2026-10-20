//! TASK-99.4: the History commit detail in the real binary over the History fixture
//! (branches, a merge, an octopus merge, tags, a local bare remote, a detached HEAD in the
//! mouse run). At 140x42 the detail sits beside the table: short and full SHA, chips, author
//! and committer, relative and UTC dates, clickable parents and the changed files with counts.
//! A parent link moves the bar inside the same window; Enter opens the diff and `Y` blame,
//! each returning with Back to the same row, `f` sets the file's `path:` chip (Esc Clear
//! returns), and `y` shows the SHA to copy, by
//! key and by click. At 80x24 `D` opens the same detail as a drill-down that follows parents and returns
//! with `u`. HEAD, refs, status, the index and config never change.
use super::*;
use ghtui::config::Config;
use std::{path::Path, time::Duration};
use support::history_fixture::*;

const BACK_KEY: &[u8] = b"\x1b[1;3D";

/// Cells of row `y` from column `from`, wide continuations skipped, with their columns.
fn row_cells(screen: &vt100::Screen, y: u16, from: u16) -> (String, Vec<u16>) {
    let (_, columns) = screen.size();
    let mut text = String::new();
    let mut xs = Vec::new();
    for x in from..columns {
        let Some(cell) = screen.cell(y, x) else {
            continue;
        };
        if cell.is_wide_continuation() {
            continue;
        }
        let contents = cell.contents();
        let contents = if contents.is_empty() {
            " ".to_owned()
        } else {
            contents
        };
        for _ in contents.chars() {
            xs.push(x);
        }
        text.push_str(&contents);
    }
    (text, xs)
}

/// The detail pane's left border: the column after the History list pane.
fn detail_left(screen: &vt100::Screen) -> Option<u16> {
    let (_, right, _, _) = history_pane(screen)?;
    let (_, columns) = screen.size();
    (right + 1 < columns).then_some(right + 1)
}

/// The text of the region right of `left` (the detail pane, or the whole drill-down).
fn region_text(screen: &vt100::Screen, left: u16) -> String {
    (0..screen.size().0)
        .map(|y| row_cells(screen, y, left).0)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Where `text` starts right of `left`, scanning rows from the top.
fn locate_from(screen: &vt100::Screen, left: u16, text: &str) -> Option<(u16, u16)> {
    (0..screen.size().0).find_map(|y| {
        let (line, xs) = row_cells(screen, y, left);
        let byte = line.find(text)?;
        let index = line[..byte].chars().count();
        xs.get(index).map(|x| (*x, y))
    })
}

fn detail(s: &Session) -> String {
    let screen = s.completed_cursorless_screen().unwrap();
    detail_left(screen).map_or_else(String::new, |left| region_text(screen, left))
}

/// Waits until the detail pane shows every text in `wanted`.
fn wait_detail(s: &mut Session, wanted: &[&str]) {
    let wanted: Vec<String> = wanted.iter().map(|text| (*text).to_owned()).collect();
    s.wait_for_cursorless_condition(
        |screen| {
            detail_left(screen).is_some_and(|left| {
                let text = region_text(screen, left);
                wanted.iter().all(|want| text.contains(want.as_str()))
            })
        },
        &format!("commit detail with {wanted:?}"),
        Duration::from_secs(10),
    );
}

/// Waits until the whole screen right of column 0 shows every text in `wanted`.
fn wait_all(s: &mut Session, wanted: &[&str]) {
    let wanted: Vec<String> = wanted.iter().map(|text| (*text).to_owned()).collect();
    s.wait_for_cursorless_condition(
        |screen| {
            let text = screen.contents();
            wanted.iter().all(|want| text.contains(want.as_str()))
        },
        &format!("screen with {wanted:?}"),
        Duration::from_secs(10),
    );
}

/// Clicks `text` inside the detail pane (the same SHA may also be a table row).
fn click_in_detail(s: &mut Session, text: &str) {
    let screen = s.completed_cursorless_screen().unwrap();
    let left = detail_left(screen).expect("the detail pane is shown");
    let (x, y) = locate_from(screen, left, text)
        .unwrap_or_else(|| panic!("{text:?} not in the detail\n{}", s.screen()));
    s.click(x, y);
}

/// Moves the bar to `key`: Down/Up presses, or a click on its SHA cell.
fn select(s: &mut Session, repo: &HistoryRepo, mouse: bool, key: &str) {
    let target = repo.short(key).to_owned();
    if mouse {
        s.click_history_row(&target);
    } else {
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(30));
            let rows = s.history_rows();
            let selected = rows.iter().position(|row| row.selected);
            let wanted = rows.iter().position(|row| row.sha == target);
            match (selected, wanted) {
                (Some(at), Some(wanted)) if at == wanted => break,
                (Some(at), Some(wanted)) if wanted < at => s.send_key(b"\x1b[A"),
                _ => s.send_key(b"\x1b[B"),
            }
            assert!(
                std::time::Instant::now() < deadline,
                "{key} never reached by the keyboard\n{}",
                s.screen()
            );
            s.poll_output(Duration::from_millis(60));
        }
    }
    s.wait_for_history_selected(repo.sha(key));
}

/// Global Back (header token or Alt-←) to the History table with the bar on `key`.
fn back_to(s: &mut Session, repo: &HistoryRepo, mouse: bool, key: &str) {
    s.choose(mouse, "Alt-← Back", BACK_KEY);
    s.wait_for_history_ready();
    s.wait_for_history_selected(repo.sha(key));
    s.wait_for_settled_footer();
}

fn journey(mouse: bool) {
    let fixture = Fixture::new();
    let repo = HistoryRepo::build(
        &fixture,
        HistoryOptions {
            detached_at: mouse.then_some("release"),
            ..HistoryOptions::default()
        },
    );
    Config {
        refresh_seconds: 3600,
        avatars: false,
        ..Default::default()
    }
    .save(&fixture.root.join("config/ghtui/config.toml"))
    .unwrap();
    let baseline = repo.state();
    let env: Vec<(&str, &str)> = if mouse {
        vec![("NO_COLOR", "1")]
    } else {
        vec![]
    };
    let mut s = Session::launch_binary(
        Path::new(env!("CARGO_BIN_EXE_ghtui")),
        &fixture,
        140,
        42,
        &env,
    );
    s.wait_for_screen("Recent commits");
    s.choose(mouse, "[g History]", b"g");
    s.wait_for_history_ready();
    s.wait_for_settled_footer();

    // 140x42: the commit under the bar is detailed beside the table.
    let opened = if mouse { "release" } else { "tweak" };
    s.wait_for_history_selected(repo.sha(opened));
    let commit = repo.commit(opened);
    wait_detail(
        &mut s,
        &[
            repo.short(opened),
            repo.sha(opened),
            commit.subject,
            commit.author.name,
            commit.author.initials,
            &repo.utc(opened),
            " ago · ",
            "Parent",
            repo.short(commit.parents[0]),
            "MESSAGE",
            "CHANGED FILES",
            "enter diff",
        ],
    );
    let text = detail(&s);
    for chip in if mouse {
        ["HEAD", "release/1.0 ⇅", "v1.0.0"].as_slice()
    } else {
        ["HEAD", "main"].as_slice()
    } {
        assert!(text.contains(chip), "{chip} chip missing\n{text}");
    }
    let (left, ..) = history_pane(s.completed_cursorless_screen().unwrap()).unwrap();
    let pane = detail_left(s.completed_cursorless_screen().unwrap()).unwrap();
    assert!(
        pane > left + 80,
        "the table keeps most of the width: {pane}"
    );

    // The octopus merge: four parent links and its three files against parent 1.
    select(&mut s, &repo, mouse, "octopus");
    let octopus = repo.commit("octopus");
    wait_detail(
        &mut s,
        &[
            repo.sha("octopus"),
            "Merge topics a, b and c",
            "Parents",
            repo.short("rename"),
            repo.short("topic_a"),
            repo.short("topic_b"),
            repo.short("topic_c"),
            "v0.2.0",
            "3 files · +3 −0 · vs parent 1",
            "A notes/a.md",
            "A notes/b.md",
            "A notes/c.md",
        ],
    );
    assert_eq!(octopus.parents.len(), 4);
    assert_eq!(repo.state(), baseline);

    // A parent link moves the bar to that parent inside the same window.
    let top = s.history_rows()[0].sha.clone();
    if mouse {
        click_in_detail(&mut s, repo.short("topic_b"));
    } else {
        s.send_key(b"\t");
        s.wait_for_screen("Commit detail focused");
        s.send_key(b"\x1b[H");
        s.send_key(b"\x1b[B");
        s.send_key(b"\x1b[B");
        s.send_key(b"\r");
    }
    s.wait_for_history_selected(repo.sha("topic_b"));
    wait_detail(&mut s, &[repo.sha("topic_b"), "Add topic b notes"]);
    assert_eq!(s.history_rows()[0].sha, top, "the window stays put");
    if !mouse {
        // Shift-Tab returns the focus from the detail to the table.
        s.send_key(b"\x1b[Z");
        s.wait_for_screen("Repository list focused");
    }
    assert_eq!(repo.state(), baseline);

    // Enter (or the `enter diff` token) opens the diff against the first parent.
    select(&mut s, &repo, mouse, "manual");
    wait_detail(
        &mut s,
        &[
            repo.sha("manual"),
            "1 file · +1 −1",
            "M docs/manual/guide.md",
        ],
    );
    if mouse {
        click_in_detail(&mut s, "enter diff");
    } else {
        s.send_key(b"\r");
    }
    wait_all(&mut s, &["Saved commit changes", "Compared parent 1:"]);
    assert!(history_columns(s.completed_cursorless_screen().unwrap()).is_none());
    back_to(&mut s, &repo, mouse, "manual");

    // f follows the selected file's history from this commit; Y blames it here.
    wait_detail(&mut s, &["M docs/manual/guide.md"]);
    if mouse {
        click_in_detail(&mut s, "f file history");
    } else {
        s.send_key(b"f");
    }
    // File history is History with the file's `path:` chip (TASK-99); Esc Clear returns to
    // every commit with the bar where it was.
    let row = s.wait_for_history_filter("path chip for the guide", |row| {
        row.removable
            .iter()
            .any(|(chip, _)| chip == "path: docs/manual/guide.md")
    });
    assert!(row.text.contains("path: docs/manual/guide.md ✕"), "{row:?}");
    s.wait_for_history_selected(repo.sha("manual"));
    s.wait_for_settled_footer();
    s.choose(mouse, "[Esc Clear filter]", b"\x1b");
    s.wait_for_cursorless_condition(
        |screen| history_filter_row(screen).is_none() && history_ready(screen),
        "path chip cleared",
        Duration::from_secs(8),
    );
    s.wait_for_history_selected(repo.sha("manual"));
    wait_detail(&mut s, &["M docs/manual/guide.md"]);
    if mouse {
        click_in_detail(&mut s, "Y blame");
    } else {
        s.send_key(b"Y");
    }
    wait_all(
        &mut s,
        &["Line blame — chosen saved text", "Guide line two, revised."],
    );
    back_to(&mut s, &repo, mouse, "manual");

    // y shows the full SHA to copy (no clipboard integration is added).
    if mouse {
        click_in_detail(&mut s, "y copy SHA");
    } else {
        s.send_key(b"y");
    }
    let body = s.wait_for_overlay_body("Commit SHA");
    assert!(body.contains(repo.sha("manual")), "{body}");
    if mouse {
        s.close_overlay_by_mouse("Commit SHA");
    } else {
        s.close_overlay_by_key("Commit SHA");
    }
    assert_eq!(repo.state(), baseline);

    // 80x24: no split; D opens the drill-down of the row under the bar.
    s.resize(80, 24);
    s.wait_for_main_frame();
    s.wait_for_history_ready();
    s.wait_for_history_selected(repo.sha("manual"));
    s.wait_for_settled_footer();
    assert!(detail_left(s.completed_cursorless_screen().unwrap()).is_none());
    s.choose(mouse, "[D Details]", b"D");
    let title = format!("Commit {}", repo.short("manual"));
    wait_all(
        &mut s,
        &[
            &title,
            repo.sha("manual"),
            "Revise the manual guide",
            ADA.name,
            &repo.utc("manual"),
            repo.short("octopus"),
        ],
    );
    assert!(history_columns(s.completed_cursorless_screen().unwrap()).is_none());
    // The parent link (or Home, Enter) follows the parent inside the drill-down.
    if mouse {
        let screen = s.completed_cursorless_screen().unwrap();
        let (x, y) = locate_from(screen, 0, &format!("Parent    {}", repo.short("octopus")))
            .unwrap_or_else(|| panic!("parent link missing\n{}", s.screen()));
        s.click(x + 10, y);
    } else {
        s.send_key(b"\x1b[H");
        s.send_key(b"\r");
    }
    let title = format!("Commit {}", repo.short("octopus"));
    wait_all(
        &mut s,
        &[&title, repo.sha("octopus"), "Merge topics a, b and c"],
    );
    // u returns to the table with the bar on the followed parent.
    s.wait_for_settled_footer();
    s.choose(mouse, "[u Back]", b"u");
    s.wait_for_history_ready();
    s.wait_for_history_selected(repo.sha("octopus"));

    assert_eq!(repo.state(), baseline);
    assert!(s.inspect_command_journal(&fixture).is_empty());
    s.quit_and_assert_restored_terminal(mouse);
    assert_eq!(repo.state(), baseline);
}

#[test]
fn history_detail_v994_keyboard_pane_parents_diff_file_history_blame_and_drilldown() {
    journey(false)
}
#[test]
fn history_detail_v994_mouse_detached_no_color_pane_parents_diff_file_history_blame_and_drilldown()
{
    journey(true)
}
