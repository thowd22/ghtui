//! Genuine original schema, typed POST receipt and durable journal factories.
use super::*;
use crate::actions_write_journal::ActionsWriteJournal;
use crate::{
    app::Action,
    theme::{ColorMode, SemanticToken},
};
use dispatch_ui::DispatchOutcomeFeedbackV33 as Feedback;

fn full_sql(fixture: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open_with_flags(
        f::database(fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut all = Vec::new();
    for name in names {
        let mut q = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = q.column_count();
        let mut rows = q
            .query_map([], |r| {
                Ok(format!(
                    "{name}:{:?}",
                    (0..columns)
                        .map(|n| r.get::<_, rusqlite::types::Value>(n))
                        .collect::<rusqlite::Result<Vec<_>>>()?
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        all.extend(rows);
    }
    all
}
fn paint(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}
fn text(buffer: &ratatui::buffer::Buffer) -> String {
    buffer
        .content
        .chunks(usize::from(buffer.area.width))
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn novice_usability_pr_dispatch_v33_genuine_204_and_200_are_pending_info_not_success_at80_120_and_no_color(
) {
    for protocol in [
        DispatchProtocolV7::Compatible2022Acceptance204,
        DispatchProtocolV7::Detailed2026Run200,
    ] {
        let (fixture, provider, mut app, steps) = prepared(protocol, confirm_phase(protocol, 42));
        let source = app.scoped_actions_v1.as_ref().unwrap().source.clone();
        let _drafts = confirm(&mut app, &provider, &fixture);
        let before = full_sql(&fixture);
        let git = f::git_state(&fixture);
        let expected = if protocol.detailed() {
            Feedback::AcceptedWithRun
        } else {
            Feedback::AcceptedWithoutRun
        };
        assert_eq!(metadata(&fixture).0, "accepted_pending");
        for mode in [
            ColorMode::TrueColor,
            ColorMode::LowColor,
            ColorMode::NoColor,
        ] {
            app.no_color = mode == ColorMode::NoColor;
            app.color_mode = mode;
            for (width, height) in [(80, 24), (120, 40)] {
                let buffer = paint(&mut app, width, height);
                let shown = text(&buffer);
                let form = app.form.as_ref().unwrap();
                assert_eq!(dispatch_ui::feedback_v33(form), Some(expected));
                assert_eq!(expected.token(), SemanticToken::Pending);
                assert!(shown.contains("Accepted; completion unknown"));
                assert!(shown.contains("[F7 Info↑]") && shown.contains("[F8 Info↓]"));
                assert!(
                    !shown.contains("[F7 Error↑]")
                        && !shown.contains("Some(")
                        && !shown.contains("None")
                );
                assert!(shown.contains("account #42") && shown.contains("workflow #7"));
                assert!(!shown.contains(SECRET) && !shown.contains("NEVER_RUN_LOCALLY"));
                assert_eq!(
                    buffer[(form.error_rect.x, form.error_rect.y)].fg,
                    app.theme()
                        .style(SemanticToken::Pending)
                        .fg
                        .unwrap_or(ratatui::style::Color::Reset)
                );
                assert_eq!(
                    app.hits
                        .iter()
                        .filter(|h| matches!(h.action, Action::Form(FormAction::ErrorScroll(_))))
                        .count(),
                    2
                );
                assert!(Arc::ptr_eq(
                    &app.scoped_actions_v1.as_ref().unwrap().source,
                    &source
                ));
                assert!(!form.plan_ready());
            }
        }
        let form = app.form.as_ref().unwrap();
        let request = dispatch_ui::capture(form, DispatchPhaseV7::Inspect).unwrap();
        assert!(
            request.opened().retained()
                >= std::mem::size_of::<dispatch_ui::DispatchComposerStateV7>()
        );
        assert!(request.opened().retained() <= 8 * 1024 * 1024);
        assert_eq!(request.created_run().is_some(), protocol.detailed());
        assert_eq!(full_sql(&fixture), before);
        assert_eq!(f::git_state(&fixture), git);
        exact(&fixture, &steps);
    }
}

#[test]
fn scoped_actions_dispatch_v7_novice_usability_result_first_full_proof_local_help_and_no_color() {
    for protocol in [
        DispatchProtocolV7::Compatible2022Acceptance204,
        DispatchProtocolV7::Detailed2026Run200,
    ] {
        let (fixture, provider, mut app, steps) = prepared(protocol, confirm_phase(protocol, 42));
        let _drafts = confirm(&mut app, &provider, &fixture);
        let before = full_sql(&fixture);
        let git = f::git_state(&fixture);
        let document = app
            .form
            .as_ref()
            .unwrap()
            .preview_display_v1
            .document
            .as_ref()
            .unwrap()
            .clone();
        let mut viewport = Default::default();
        let mut logical = std::collections::BTreeMap::<usize, String>::new();
        for top in 0..document.rows_at(64).unwrap() {
            let window = document.window(&mut viewport, 64, 1, top).unwrap();
            for row in window.rows() {
                logical
                    .entry(row.original_row())
                    .or_default()
                    .push_str(row.text());
            }
        }
        let complete = logical.values().cloned().collect::<Vec<_>>().join("\n");
        for fact in [
            "Original request",
            "numeric repository",
            "workflow 7",
            ".github/workflows/ci.yml",
            protocol.version(),
            "branch name: main",
            "Observed chosen ref tip:",
            "Actual peeled definition commit:",
            C,
            "Omit does not suppress a server default",
            "not an atomic ref/ABA guarantee",
            "Close keeps its protected record",
            "Acknowledgement is local only",
        ] {
            assert!(
                complete.contains(fact),
                "Full actual prepared proof missing {fact}: {complete}"
            );
        }
        assert!(!complete.contains(SECRET) && !complete.contains("NEVER_RUN_LOCALLY"));
        for mode in [
            ColorMode::TrueColor,
            ColorMode::LowColor,
            ColorMode::NoColor,
        ] {
            app.color_mode = mode;
            app.no_color = mode == ColorMode::NoColor;
            for (width, height) in [(80, 20), (80, 24), (120, 40)] {
                let buffer = paint(&mut app, width, height);
                let shown = text(&buffer);
                assert!(
                    shown.find("Accepted; completion unknown").unwrap()
                        < shown.find("Original request").unwrap()
                );
                assert!(shown.contains("Tab: buttons • Esc: close"));
                assert!(
                    !shown.contains("Ctrl-A: select all")
                        && !shown.contains("Ctrl-P: preview")
                        && !shown.contains("Esc: keep draft")
                );
                assert!(shown.contains("[F6 Acknowledge locally]"));
                let expected = if protocol.detailed() {
                    "[F11 Open run]"
                } else {
                    "[F11 Run unavailable]"
                };
                assert!(shown.contains(expected));
                let form = app.form.as_ref().unwrap();
                assert!(Arc::ptr_eq(
                    form.preview_display_v1.document.as_ref().unwrap(),
                    &document
                ));
                assert_eq!(
                    dispatch_ui::run_display_ready_v42(form),
                    protocol.detailed()
                );
                let hit = app
                    .hits
                    .iter()
                    .find(|hit| matches!(hit.action, Action::Form(FormAction::OpenDispatchRunV7)))
                    .unwrap();
                assert_eq!(usize::from(hit.rect.width), expected.len());
                assert!(hit.rect.right() <= width);
                assert!(hit.rect.y > form.error_rect.y);
                assert!(!form.plan_ready());
            }
        }
        assert_eq!(full_sql(&fixture), before);
        assert_eq!(f::git_state(&fixture), git);
        exact(&fixture, &steps);
    }
}

#[test]
fn scoped_actions_dispatch_v7_novice_usability_actual_run_display_never_promotes_wrong_reference_or_error(
) {
    let protocol = DispatchProtocolV7::Detailed2026Run200;
    let (fixture, provider, mut app, steps) = prepared(protocol, confirm_phase(protocol, 42));
    let _drafts = confirm(&mut app, &provider, &fixture);
    paint(&mut app, 120, 40);
    let before = full_sql(&fixture);
    let git = f::git_state(&fixture);
    let actual = app.form.as_ref().unwrap().clone();
    assert!(dispatch_ui::run_display_ready_v42(&actual));
    for missing in [true, false] {
        let mut changed = actual.clone();
        if missing {
            changed.actions_publication = None;
        } else {
            changed.actions_publication.as_mut().unwrap().attempt_id += 1;
        }
        assert!(!dispatch_ui::run_display_ready_v42(&changed));
        assert_eq!(
            dispatch_ui::run_display_label_v42(&changed),
            "[F11 Run unavailable]"
        );
        assert!(!dispatch_ui::result_heading_v42(&changed).starts_with("Accepted"));
    }
    let mut changed = actual.clone();
    changed.error = "A genuinely unrelated local failure; not accepted-state proof".into();
    assert!(dispatch_ui::feedback_v33(&changed).is_none());
    assert!(!dispatch_ui::result_heading_v42(&changed).starts_with("Accepted"));
    changed = actual.clone();
    changed.submitting = true;
    assert!(!dispatch_ui::run_display_ready_v42(&changed));
    changed = actual.clone();
    changed.preview_display_v1.geometry_rejected = true;
    assert!(!dispatch_ui::run_display_ready_v42(&changed));
    app.form = Some(actual);
    let small = paint(&mut app, 79, 20);
    assert!(text(&small).contains("[Esc Close]"));
    assert!(!dispatch_ui::run_display_ready_v42(
        app.form.as_ref().unwrap()
    ));
    assert_eq!(full_sql(&fixture), before);
    assert_eq!(f::git_state(&fixture), git);
    exact(&fixture, &steps);
}

#[test]
fn novice_usability_pr_dispatch_v33_saved_full_receipts_and_unissued_foreign_error_feedback_fail_closed(
) {
    for protocol in [
        DispatchProtocolV7::Compatible2022Acceptance204,
        DispatchProtocolV7::Detailed2026Run200,
    ] {
        let (fixture, provider, mut app, steps) = prepared(protocol, confirm_phase(protocol, 42));
        // Real current Preview has no history/result. HTTP-looking local text
        // cannot invent an annotation or permission.
        app.form.as_mut().unwrap().error = Feedback::AcceptedWithoutRun.message().into();
        assert!(dispatch_ui::feedback_v33(app.form.as_ref().unwrap()).is_none());
        assert!(text(&paint(&mut app, 120, 40)).contains("[F7 Error↑]"));
        let _drafts = confirm(&mut app, &provider, &fixture);
        let before = full_sql(&fixture);
        let git = f::git_state(&fixture);
        let form = app.form.as_ref().unwrap();
        let reference = form.actions_publication.clone().unwrap();
        let identity = dispatch_ui::capture(form, DispatchPhaseV7::Inspect)
            .unwrap()
            .opened()
            .identity()
            .clone();
        let marker = ActionsWriteJournal::open(&f::database(&fixture))
            .unwrap()
            .original_publication(&provider, &identity, &reference)
            .unwrap();
        let lines = crate::actions_routes::original_lines(&marker).join("\n");
        assert!(
            lines.contains("Saved Dispatch #")
                && lines.contains(&format!("Saved workflow: #7 • repository {}", f::REPO))
        );
        assert!(
            lines.contains("completion not confirmed")
                && !lines.contains("Some(")
                && !lines.contains("Workflow {")
        );
        if protocol.detailed() {
            for fact in [
                "Returned run #124",
                "attempt #1",
                "workflow #7",
                "WR_124",
                "check suite #902",
                C,
                "workflow_dispatch",
            ] {
                assert!(
                    lines.contains(fact),
                    "Missing original receipt fact {fact}: {lines}"
                );
            }
        } else {
            assert!(lines.contains("Run ID not returned") && !lines.contains("Returned run #"));
        }
        let original = app.form.as_ref().unwrap().clone();
        app.form.as_mut().unwrap().actions_publication = None;
        assert!(dispatch_ui::feedback_v33(app.form.as_ref().unwrap()).is_none());
        app.form = Some(original.clone());
        app.form
            .as_mut()
            .unwrap()
            .actions_publication
            .as_mut()
            .unwrap()
            .attempt_id += 1;
        assert!(dispatch_ui::feedback_v33(app.form.as_ref().unwrap()).is_none());
        app.form = Some(original.clone());
        app.form.as_mut().unwrap().error =
            "Actual local ACK failed; original receipt protected".into();
        assert!(dispatch_ui::feedback_v33(app.form.as_ref().unwrap()).is_none());
        assert!(text(&paint(&mut app, 120, 40)).contains("[F7 Error↑]"));
        app.form = Some(original);
        app.form.as_mut().unwrap().invalidate_plan();
        assert!(dispatch_ui::feedback_v33(app.form.as_ref().unwrap()).is_none());
        assert_eq!(full_sql(&fixture), before);
        assert_eq!(f::git_state(&fixture), git);
        exact(&fixture, &steps);
    }
    // Actual malformed returned head and actual numeric-account loss retain
    // their existing private retirement, not a display-classification waiver.
    for uid_loss in [false, true] {
        let protocol = DispatchProtocolV7::Detailed2026Run200;
        let mut tail = confirm_phase(protocol, if uid_loss { 999 } else { 42 });
        if !uid_loss {
            let mut raw = returned_run();
            raw["head_sha"] = json!("d".repeat(40));
            tail[19].stdout = raw.to_string().into_bytes();
            tail.truncate(20);
        }
        let (fixture, provider, mut app, steps) = prepared(protocol, tail);
        let _drafts = confirm(&mut app, &provider, &fixture);
        assert!(app.form.is_none() && app.scoped_actions_v1.is_none());
        assert_eq!(
            metadata(&fixture).0,
            if uid_loss {
                "accepted_pending"
            } else {
                "uncertain"
            }
        );
        exact(&fixture, &steps);
    }
}
