//! TASK-91 table contracts: column layout at 80 and 140 columns, the order columns leave in,
//! ellipsis and right alignment, the `▌` selection bar on the Selection background, scrolling
//! and the shared age/duration/short-SHA helpers. The Actions tables are the reference adapter.
use super::*;
use crate::{
    actions_workspace::{ActionsView, ActionsWorkspace, ScopedRun, RUN_COLUMNS},
    config::ThemeName,
    providers::gh::{Actor, HostIdentity, WorkflowRun},
};
use ratatui::{backend::TestBackend, style::Color, Terminal};

fn theme(mode: ColorMode) -> Theme {
    Theme::new(ThemeName::CatppuccinMocha, mode)
}

fn run(id: u64, name: &str, branch: &str, event: &str, login: &str) -> ScopedRun {
    ScopedRun {
        repository: "example/repo".into(),
        run: WorkflowRun {
            id,
            workflow_id: 7,
            name: name.into(),
            title: "Fix checkout".into(),
            head_sha: "a4a6e459449ecd7022fbfc2db0bd21b8b6202cbd".into(),
            head_branch: branch.into(),
            event: event.into(),
            status: "completed".into(),
            conclusion: if id.is_multiple_of(2) {
                "success"
            } else {
                "failure"
            }
            .into(),
            attempt: if id == 123 { 2 } else { 1 },
            actor: Actor {
                login: login.into(),
                display_name: None,
                avatar_url: None,
                actor_type: "User".into(),
            },
            triggering_actor: Actor {
                login: login.into(),
                display_name: None,
                avatar_url: None,
                actor_type: "User".into(),
            },
            created_at: "2026-10-13T12:00:00Z".into(),
            updated_at: "2026-10-13T12:08:29Z".into(),
            url: String::new(),
        },
    }
}

fn workspace() -> ActionsWorkspace {
    let mut workspace = ActionsWorkspace::new(
        HostIdentity {
            host: "github.invalid".into(),
            login: "thowd22".into(),
            account_id: Some(42),
            gh_version: "offline".into(),
        },
        vec!["example/repo".into()],
    );
    workspace.runs = vec![
        run(123, "CI matrix", "feature", "pull_request", "alice"),
        run(124, "Release", "main", "workflow_dispatch", "thowd22"),
        run(39196660893, "CI", "main", "push", "thowd22"),
    ];
    workspace.prepare_rows();
    workspace
}

fn draw_actions(
    workspace: &mut ActionsWorkspace,
    width: u16,
    height: u16,
) -> (Vec<String>, crate::actions_ui::ActionsRender) {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    let mut render = None;
    terminal
        .draw(|frame| {
            render = Some(crate::actions_ui::draw(
                frame,
                workspace,
                frame.area(),
                theme(ColorMode::TrueColor),
                true,
            ));
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let rows = (0..height)
        .map(|y| (0..width).map(|x| buffer[(x, y)].symbol()).collect())
        .collect();
    (rows, render.unwrap())
}

fn titles(placed: &[Placed], columns: &[Column]) -> Vec<&'static str> {
    placed.iter().map(|p| columns[p.column].title).collect()
}

#[test]
fn design_table_v91_layout_keeps_every_run_column_at_140_and_drops_by_priority_at_80() {
    // Natural widths of a real Runs page: Release, main, workflow_dispatch, a4a6e45,
    // `TH thowd22`, 7h, 59m 12s and an 11-digit run id.
    let natural = [8, 6, 17, 7, 10, 3, 7, 11];
    let wide = layout(&RUN_COLUMNS, &natural, 140 - 2 - 3);
    assert_eq!(titles(&wide, &RUN_COLUMNS).len(), 8);
    // Every column gets its natural width; columns are separated by exactly two blanks.
    for (placed, width) in wide.iter().zip(natural) {
        assert_eq!(placed.width, width.max(RUN_COLUMNS[placed.column].min));
    }
    for pair in wide.windows(2) {
        assert_eq!(pair[1].x, pair[0].x + pair[0].width + COLUMN_GAP);
    }
    // 80 columns: the run id leaves first, the workflow and age never do.
    let narrow = layout(&RUN_COLUMNS, &natural, 80 - 2 - 3);
    assert_eq!(
        titles(&narrow, &RUN_COLUMNS),
        ["WORKFLOW", "BRANCH", "EVENT", "COMMIT", "ACTOR", "AGE", "TOOK"]
    );
    let last = narrow.last().unwrap();
    assert!(last.x + last.width <= 75);
}

#[test]
fn design_table_v91_columns_leave_in_priority_order_and_flex_shrinks_to_min() {
    let order = (20..=90)
        .rev()
        .map(|width| {
            titles(
                &layout(&RUN_COLUMNS, &[8, 6, 17, 7, 10, 3, 7, 11], width),
                &RUN_COLUMNS,
            )
        })
        .collect::<Vec<_>>();
    let mut left = vec![];
    for pair in order.windows(2) {
        for title in &pair[0] {
            if !pair[1].contains(title) {
                left.push(*title);
            }
        }
    }
    assert_eq!(
        left,
        ["RUN", "TOOK", "EVENT", "ACTOR", "COMMIT", "BRANCH"],
        "highest priority value leaves first"
    );
    // Before EVENT leaves it shrinks to its min: flex columns give up width first.
    let tight = layout(&RUN_COLUMNS, &[8, 6, 17, 7, 10, 3, 7, 11], 49);
    let event = tight
        .iter()
        .find(|p| RUN_COLUMNS[p.column].title == "EVENT");
    assert_eq!(event.map(|p| p.width), Some(5));
    // A single column left alone is cut to the row.
    let one = layout(&[Column::new("ONLY", 40)], &[60], 10);
    assert_eq!(
        one,
        [Placed {
            column: 0,
            x: 0,
            width: 10
        }]
    );
}

#[test]
fn design_table_v91_truncates_with_ellipsis_and_right_aligns_numbers() {
    assert_eq!(aligned("workflow_dispatch", 8, Align::Left), "workflo…");
    assert_eq!(aligned("7h", 4, Align::Right), "  7h");
    assert_eq!(aligned("39196660893", 5, Align::Right), "3919…");
    assert_eq!(display_width(&aligned("界界界", 4, Align::Left)), 3);
    assert_eq!(
        short_sha("a4a6e459449ecd7022fbfc2db0bd21b8b6202cbd"),
        "a4a6e45"
    );
    assert_eq!(short_sha("not-a-sha"), "not-a-sha");
    assert_eq!(age_from_seconds(42), "42s");
    assert_eq!(age_from_seconds(3599), "59m");
    assert_eq!(age_from_seconds(7 * 3600 + 5), "7h");
    assert_eq!(age_from_seconds(29 * 86400), "29d");
    assert_eq!(age_from_seconds(35 * 86400), "1mo");
    assert_eq!(age_from_seconds(340 * 86400), "11mo");
    assert_eq!(age_from_seconds(1059 * 86400), "2y");
    let created = crate::github_workspace::timestamp_seconds("2026-10-13T12:00:00Z").unwrap();
    assert_eq!(
        age_at("2026-10-13T12:00:00Z", created + 7200).as_deref(),
        Some("2h")
    );
    assert_eq!(age_at("yesterday", created), None);
    assert_eq!(
        duration("2026-10-13T12:00:00Z", "2026-10-13T12:08:29Z").as_deref(),
        Some("8m 29s")
    );
    assert_eq!(duration("2026-10-13T12:00:00Z", ""), None);
}

#[test]
fn design_table_v91_ensure_visible_scrolls_the_least_distance() {
    let mut state = ListState {
        selected: 12,
        top: 0,
    };
    state.ensure_visible(5);
    assert_eq!(state.top, 8);
    state.selected = 9;
    state.ensure_visible(5);
    assert_eq!(state.top, 8, "a visible selection never scrolls");
    state.selected = 2;
    state.ensure_visible(5);
    assert_eq!(state.top, 2);
    state.ensure_visible(0);
    assert_eq!(state.top, 2);
}

#[test]
fn design_table_v91_selection_bar_paints_on_selection_background_without_reverse() {
    let rows = vec![
        TableRow::cells(vec!["CI".into(), "7h".into()]).with_glyph(Some(SemanticToken::Success)),
        TableRow::cells(vec!["Release".into(), "2h".into()])
            .with_glyph(Some(SemanticToken::Failure))
            .with_action(Some(7u8)),
    ];
    let columns = [
        Column::new("WORKFLOW", 8).flex(1),
        Column::new("AGE", 3).right(),
    ];
    for (mode, focused) in [
        (ColorMode::TrueColor, true),
        (ColorMode::TrueColor, false),
        (ColorMode::NoColor, true),
    ] {
        let theme = theme(mode);
        let mut terminal = Terminal::new(TestBackend::new(30, 4)).unwrap();
        let mut state = ListState {
            selected: 1,
            top: 0,
        };
        let mut render = None;
        terminal
            .draw(|frame| {
                render = Some(draw_table(
                    frame,
                    frame.area(),
                    &Table {
                        columns: &columns,
                        header: true,
                        focused,
                        glyphs: true,
                        empty: None,
                    },
                    &rows,
                    &mut state,
                    theme,
                ));
            })
            .unwrap();
        let render = render.unwrap();
        let buffer = terminal.backend().buffer();
        let line = |y: u16| (0..30).map(|x| buffer[(x, y)].symbol()).collect::<String>();
        // Natural widths: spare width stays blank at the right, AGE is right-aligned.
        assert_eq!(line(0).trim_end(), "   WORKFLOW  AGE");
        assert_eq!(line(1).trim_end(), " ✓ CI         7h");
        assert_eq!(line(2).trim_end(), "▌✗ Release    2h");
        // One hit per body row, carrying the row action or none.
        assert_eq!(render.hits.len(), 2);
        assert_eq!(render.hits[1].action, Some(7));
        assert_eq!(render.hits[1].rect, Rect::new(0, 2, 30, 1));
        assert_eq!(render.visible_rows, 3);
        let bar = &buffer[(0, 2)];
        if mode == ColorMode::NoColor {
            assert!(bar.modifier.contains(Modifier::REVERSED));
        } else {
            // The whole row sits on the Selection background, text keeps its foreground.
            let selection = crate::theme::rgb(
                crate::theme::Palette::for_theme(ThemeName::CatppuccinMocha)
                    .unwrap()
                    .selection,
            );
            for x in 0..30 {
                assert_eq!(buffer[(x, 2)].bg, selection, "x={x}");
                assert!(!buffer[(x, 2)].modifier.contains(Modifier::REVERSED));
            }
            assert_ne!(buffer[(0, 1)].bg, selection);
            let accent = theme.role(Role::Accent).fg;
            assert_eq!(Some(bar.fg) == accent, focused);
            assert_ne!(buffer[(1, 2)].fg, Color::Reset);
        }
    }
}

#[test]
fn design_table_v91_actions_runs_render_aligned_columns_at_140_and_80() {
    for (width, height) in [(140u16, 42u16), (80, 24)] {
        let mut workspace = workspace();
        workspace.selected = 1;
        let (rows, render) = draw_actions(&mut workspace, width, height);
        let header = rows
            .iter()
            .position(|row| row.contains("WORKFLOW") && row.contains("COMMIT"))
            .expect("sticky header");
        let body = &rows[header + 1..header + 4];
        let cell = |row: &str, x: usize| row.chars().nth(x).unwrap();
        let column = |title: &str| {
            let byte = rows[header].find(title).unwrap();
            rows[header][..byte].chars().count()
        };
        // The bar marks only the selected row; everything else keeps a blank lead.
        assert!(body[1].starts_with("▌✓ Release"), "{:?}", body[1]);
        assert!(body[0].starts_with(" ✗ CI matrix"), "{:?}", body[0]);
        assert!(body[2].starts_with(" ✗ CI "), "{:?}", body[2]);
        assert!(!rows.join("\n").contains("> "));
        // Columns line up: branch and commit start at the header's column in every row.
        for title in ["BRANCH", "COMMIT", "ACTOR"] {
            let x = column(title);
            for row in body {
                assert_ne!(cell(row, x), ' ', "{title} misaligned in {row:?}");
                assert_eq!(cell(row, x - 1), ' ', "{title} misaligned in {row:?}");
            }
        }
        // Short SHA in the list, full SHA nowhere in the list rows.
        assert!(body.iter().all(|row| row.contains(" a4a6e45 ")));
        assert!(!rows.join("\n").contains("a4a6e459449e"));
        // Actor cells: the initials chip, one blank, the login without `@`.
        assert!(body[0].contains("AL alice"), "{:?}", body[0]);
        assert!(body[1].contains("TH thowd22"), "{:?}", body[1]);
        // Age is right-aligned under its header.
        let age_end = column("AGE") + 2;
        assert!(body.iter().all(|row| cell(row, age_end) == 'd'));
        if width >= 140 {
            assert!(rows[header].contains("RUN") && body[0].contains("123 #2"));
            assert!(body[1].contains("workflow_dispatch") && body[1].contains("8m 29s"));
        } else {
            // The run id leaves first so the event keeps its whole name.
            assert!(!rows[header].contains("RUN") && rows[header].contains("TOOK"));
            assert!(body[1].contains("workflow_dispatch"));
        }
        // Each body row is one full-width click target opening its exact run.
        let hit = render
            .hits
            .iter()
            .find(|hit| hit.rect.y == (header + 1) as u16)
            .expect("row click target");
        assert_eq!(hit.rect.width, width);
        assert_eq!(
            hit.action,
            crate::actions_workspace::ActionsAction::OpenRun {
                repository: "example/repo".into(),
                id: 123,
                attempt: 2,
            }
        );
        assert!(render.hits.iter().all(|hit| hit.rect.y != header as u16));
    }
}

#[test]
fn design_table_v91_actions_run_details_keep_full_sha_and_utc_with_a_jobs_table() {
    let mut workspace = workspace();
    let run = workspace.runs[0].run.clone();
    workspace.view = ActionsView::Run;
    workspace.run = Some(crate::actions_workspace::RunPage {
        repository: "example/repo".into(),
        attempt: crate::providers::gh::RunAttempt {
            run,
            jobs: vec![crate::providers::gh::ActionsJob {
                id: 501,
                run_id: 123,
                attempt: 2,
                head_sha: "a4a6e459449ecd7022fbfc2db0bd21b8b6202cbd".into(),
                name: "matrix test linux".into(),
                status: "completed".into(),
                conclusion: "failure".into(),
                started_at: "2026-10-13T12:00:00Z".into(),
                completed_at: "2026-10-13T12:02:29Z".into(),
                runner_id: 1,
                runner_name: "hosted".into(),
                runner_group_name: String::new(),
                labels: vec!["ubuntu-latest".into()],
                check_run_url: String::new(),
                steps: vec![],
            }],
            definition_note: "Attempt source: verified".into(),
        },
        selected_job: None,
        logs: None,
        annotations: vec![],
    });
    workspace.prepare_rows();
    for width in [140u16, 80] {
        let (rows, _) = draw_actions(&mut workspace, width, 30);
        let text = rows.join("\n");
        assert!(text.contains("Exact commit a4a6e459449ecd7022fbfc2db0bd21b8b6202cbd"));
        let header = rows
            .iter()
            .position(|row| row.contains("JOB") && row.contains("STATUS"))
            .expect("inline jobs header");
        let job = &rows[header + 1];
        assert!(job.starts_with(" ✗ matrix test linux"), "{job:?}");
        assert!(job.contains("failure") && job.contains("2m 29s"), "{job:?}");
        // The run's actors render as chips with their role beside them.
        assert!(text.contains("AL alice  run actor"), "{text}");
    }
    // The UTC creation time stays in the detail row at full width.
    let (rows, _) = draw_actions(&mut workspace, 140, 30);
    assert!(rows.join("\n").contains("2026-10-13T12:00:00Z"));
}

#[test]
fn design_table_v91_empty_table_shows_its_message_under_the_header() {
    let columns = [Column::new("NAME", 4)];
    let mut terminal = Terminal::new(TestBackend::new(20, 3)).unwrap();
    let mut state = ListState::default();
    terminal
        .draw(|frame| {
            let render = draw_table::<u8>(
                frame,
                frame.area(),
                &Table {
                    columns: &columns,
                    header: true,
                    focused: true,
                    glyphs: false,
                    empty: Some("No runs"),
                },
                &[],
                &mut state,
                theme(ColorMode::TrueColor),
            );
            assert!(render.hits.is_empty());
        })
        .unwrap();
    let buffer = terminal.backend().buffer();
    let line = |y: u16| (0..20).map(|x| buffer[(x, y)].symbol()).collect::<String>();
    assert_eq!(line(0).trim_end(), "  NAME");
    assert_eq!(line(1).trim_end(), "  No runs");
}
