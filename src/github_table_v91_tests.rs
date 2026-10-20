//! TASK-91 Inbox queue and PR Commits/Files/Checks tables: aligned columns under titles, the
//! `▌` bar on the Selection background (no `>` marker), short SHAs and relative ages in rows,
//! the full SHA in a detail row, and one click target per row with the same actions as before.
use super::footer_v90_tests::{detail, inbox};
use super::*;
use crate::github_workspace::{PrKey, PrTab};
use crate::providers::gh::{Actor, ChangedFile, Check, CommitSummary};
use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

fn actor(login: &str) -> Actor {
    Actor {
        login: login.into(),
        display_name: None,
        avatar_url: None,
        actor_type: "User".into(),
    }
}

fn paint(app: &mut App, width: u16, height: u16) -> (Buffer, Vec<String>) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    app.hits.clear();
    terminal
        .draw(|frame| draw(frame, app, Rect::new(0, 0, width, height)))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let lines = (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    (buffer, lines)
}

fn line_with<'a>(lines: &'a [String], parts: &[&str]) -> (usize, &'a String) {
    lines
        .iter()
        .enumerate()
        .find(|(_, line)| parts.iter().all(|part| line.contains(part)))
        .unwrap_or_else(|| panic!("no line holds {parts:?}:\n{}", lines.join("\n")))
}

/// The display column where `needle` starts in `line`.
fn column(line: &str, needle: &str) -> usize {
    crate::editor::display_width(&line[..line.find(needle).unwrap()])
}

fn assert_selected(app: &App, buffer: &Buffer, y: u16) {
    let selection = crate::ui_table::selection_style(app.theme());
    assert_eq!(buffer[(0, y)].symbol(), crate::ui_table::SELECTION_BAR);
    for x in [0, 4, buffer.area.width - 1] {
        assert_eq!(buffer[(x, y)].bg, selection.bg.unwrap_or_default(), "x={x}");
        assert!(buffer[(x, y)].modifier.contains(selection.add_modifier));
    }
}

fn no_marker(lines: &[String]) {
    assert!(
        lines.iter().all(|line| !line.contains("> ")),
        "{}",
        lines.join("\n")
    );
}

#[test]
fn design_table_v91_inbox_rows_are_aligned_table_rows_with_bar_ages_and_open_targets() {
    for (width, height) in [(80u16, 24u16), (140, 42)] {
        let mut app = inbox(3);
        {
            let github = app.github.as_mut().unwrap();
            for (index, pr) in github.inbox.pull_requests.iter_mut().enumerate() {
                pr.author = actor(["alice", "bob", "carol"][index]);
                pr.created_at = "2026-01-01T00:00:00Z".into();
            }
            github.inbox.pull_requests[1].title = "Fix dark, silent and out-of-sync preview playback across every assembled viewer path".into();
            github.inbox.pull_requests[2].draft = true;
            github.select_index(1);
        }
        let (buffer, lines) = paint(&mut app, width, height);
        no_marker(&lines);
        let (header_y, header) = line_with(&lines, &["PR", "TITLE", "AUTHOR", "AGE"]);
        let (y1, first) = line_with(&lines, &["example/actual#1", "Footer row 1", "alice"]);
        let (y2, second) = line_with(&lines, &["example/actual#2", "Fix dark", "bob"]);
        let (y3, third) = line_with(&lines, &["example/actual#3", "[draft]", "carol"]);
        // Comfortable (the default, TASK-96): each PR is its row over a muted detail line.
        assert_eq!((y1, y2, y3), (header_y + 1, header_y + 3, header_y + 5));
        // Columns line up under their titles; the login has no `@`; the long title is cut.
        for row in [first, second, third] {
            assert_eq!(column(row, "example/"), column(header, "PR"), "{row}");
            assert!(!row.contains("@alice") && !row.contains("@bob"), "{row}");
        }
        assert_eq!(column(first, "Footer"), column(header, "TITLE"));
        // At 80 columns the long title is cut with an ellipsis; at 140 it fits whole.
        assert_eq!(second.contains('…'), width < 140, "{second}");
        assert_eq!(second.contains("viewer path"), width >= 140, "{second}");
        // The age is relative (days up to a month, then months and years) and right-aligned
        // under AGE.
        let age = crate::ui_table::relative_age("2026-01-01T00:00:00Z");
        let age_end = column(header, "AGE") + 3;
        for row in [first, second, third] {
            let cells = row.chars().collect::<Vec<_>>();
            let painted = cells[age_end - age.chars().count()..age_end]
                .iter()
                .collect::<String>();
            assert_eq!(painted, age, "{row}");
            assert_eq!(cells[age_end - age.chars().count() - 1], ' ', "{row}");
            assert!(cells.get(age_end).is_none_or(|cell| *cell == ' '), "{row}");
            assert!(!row.contains("old"), "{row}");
        }
        if width >= 140 {
            assert!(
                header.contains("QUEUE") && first.contains("Created"),
                "{first}"
            );
        }
        assert_eq!(buffer[(0, y1 as u16)].symbol(), " ");
        assert_selected(&app, &buffer, y2 as u16);
        // One click target per painted row, each opening that exact pull request.
        let github = app.github.as_ref().unwrap();
        let opens = app
            .hits
            .iter()
            .filter_map(|hit| match &hit.action {
                Action::Github(GithubAction::Open(key)) => Some((hit.rect, key.clone())),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(opens.len(), 3);
        for ((rect, key), (y, pr)) in opens
            .iter()
            .zip([y1, y2, y3].into_iter().zip(&github.inbox.pull_requests))
        {
            assert_eq!(*rect, Rect::new(0, y as u16, width, 2));
            assert_eq!(*key, PrKey::from(pr));
        }
        assert_eq!(
            app.github_visible_rows,
            (usize::from(height) - header_y - 1) / 2
        );
    }
}

#[test]
fn design_table_v91_inbox_empty_queue_keeps_titles_and_hint() {
    let mut app = inbox(0);
    let (_, lines) = paint(&mut app, 80, 24);
    let (header_y, _) = line_with(&lines, &["PR", "TITLE", "AGE"]);
    assert!(lines[header_y + 1].trim().len() > 4, "{}", lines.join("\n"));
    assert!(!app
        .hits
        .iter()
        .any(|hit| matches!(hit.action, Action::Github(GithubAction::Open(_)))));
}

fn with_page(tab: PrTab, edit: impl FnOnce(&mut crate::github_workspace::PrPage)) -> App {
    let mut app = detail(tab);
    let page = app.github.as_mut().unwrap().pr.as_mut().unwrap();
    edit(page);
    page.prepare_rows().unwrap();
    app
}

#[test]
fn design_table_v91_pr_commits_show_short_sha_rows_and_full_sha_details() {
    let full = "c0ffee1".to_string() + &"9".repeat(33);
    for width in [80u16, 140] {
        let mut app = with_page(PrTab::Commits, |page| {
            page.details.commits = vec![CommitSummary {
                sha: full.clone(),
                message: "Fix preview playback\n\nLong body".into(),
                author: actor("alice"),
                committer: actor("bob"),
                author_name: "Alice A".into(),
                committer_name: "Bob B".into(),
                authored_at: "2026-01-01T00:00:00Z".into(),
                parents: vec!["d".repeat(40)],
            }];
        });
        let (_, lines) = paint(&mut app, width, 24);
        no_marker(&lines);
        let (header_y, header) = line_with(&lines, &["SHA", "MESSAGE", "AUTHOR", "AGE"]);
        let (row_y, row) = line_with(&lines, &["c0ffee1", "Fix preview playback", "alice"]);
        assert_eq!(row_y, header_y + 1);
        assert!(!row.contains(&full) && !row.contains("@alice"), "{row}");
        assert_eq!(column(row, "c0ffee1"), column(header, "SHA"));
        assert_eq!(column(row, "Fix"), column(header, "MESSAGE"));
        // The full SHA and UTC time stay visible in the commit's detail rows.
        line_with(&lines, &[&format!("Commit {full}")]);
        line_with(&lines, &["Parents: "]);
        line_with(&lines, &["author Alice A • 2026-01-01T00:00:00Z"]);
    }
}

#[test]
fn design_table_v91_pr_files_rows_align_counts_under_their_titles() {
    let mut app = with_page(PrTab::Files, |page| {
        page.details.files = vec![
            ChangedFile {
                path: "src/player.rs".into(),
                previous_path: Some("src/old_player.rs".into()),
                status: "renamed".into(),
                additions: 1234,
                deletions: 5,
                patch: None,
            },
            ChangedFile {
                path: "README.md".into(),
                previous_path: None,
                status: "modified".into(),
                additions: 3,
                deletions: 89,
                patch: None,
            },
        ];
    });
    let (_, lines) = paint(&mut app, 80, 24);
    no_marker(&lines);
    let (_, header) = line_with(&lines, &["STATUS", "PATH", "ADD", "DEL"]);
    let (_, renamed) = line_with(
        &lines,
        &["renamed", "src/player.rs (was src/old_player.rs)"],
    );
    let (_, modified) = line_with(&lines, &["modified", "README.md"]);
    let add_end = column(header, "ADD") + 3;
    let del_end = column(header, "DEL") + 3;
    assert_eq!(column(renamed, "+1234") + 5, add_end);
    assert_eq!(column(modified, "+3") + 2, add_end);
    assert_eq!(column(renamed, "−5") + 2, del_end);
    assert_eq!(column(modified, "−89") + 3, del_end);
    assert_eq!(column(renamed, "src/"), column(header, "PATH"));
}

#[test]
fn design_table_v91_pr_checks_are_a_table_with_glyphs_bar_and_row_targets() {
    let head = "a".repeat(40);
    let check = |id: u64, name: &str, status: &str, conclusion: &str| Check {
        id,
        name: name.into(),
        status: status.into(),
        conclusion: conclusion.into(),
        url: String::new(),
        head_sha: head.clone(),
        kind: "check_run".into(),
    };
    let mut app = with_page(PrTab::Checks, |page| {
        page.checks = Some(vec![
            check(1, "build", "completed", "success"),
            check(2, "lint", "completed", "failure"),
            check(3, "deploy", "in_progress", ""),
        ]);
    });
    app.inbox_list.selected = 1;
    let (buffer, lines) = paint(&mut app, 80, 24);
    no_marker(&lines);
    line_with(&lines, &[&format!("Head {head}")]);
    let (header_y, header) = line_with(&lines, &["CHECK", "STATUS", "COMMIT"]);
    let (build_y, build) = line_with(&lines, &["build", "success", "aaaaaaa"]);
    let (lint_y, lint) = line_with(&lines, &["lint", "failure"]);
    let (_, deploy) = line_with(&lines, &["deploy", "in_progress"]);
    assert_eq!(build_y, header_y + 1);
    for (row, glyph) in [(build, "✓"), (lint, "✗"), (deploy, "↻")] {
        assert_eq!(row.chars().nth(1).unwrap().to_string(), glyph, "{row}");
        assert!(!row.contains(&head), "{row}");
    }
    assert_eq!(column(build, "build"), column(header, "CHECK"));
    assert_eq!(column(lint, "failure"), column(header, "STATUS"));
    assert_selected(&app, &buffer, lint_y as u16);
    let targets = app
        .hits
        .iter()
        .filter_map(|hit| match hit.action {
            Action::GithubRow(index) => Some((index, hit.rect.y as usize)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(targets, vec![(0, build_y), (1, lint_y), (2, lint_y + 1)]);
}
