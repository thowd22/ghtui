//! Genuine same-Background admission failures; no command is run by the queue.
use super::*;
fn rejected(confirmed: bool) {
    for disconnected in [false, true] {
        let real = Real::new("sha256");
        let mut app = app(&real, RepositoryMode::Working);
        let mut background = Background::new();
        ui::begin(&mut app);
        if confirmed {
            submit(&mut background, &mut app);
            paint(&mut app);
            ui::dispatch(&mut app, A::Confirm);
        }
        let work = app.original_diff_work_v1.take().unwrap();
        let (tx, receiver) = mpsc::sync_channel::<Request>(0);
        let receiver = if disconnected {
            drop(receiver);
            None
        } else {
            Some(receiver)
        };
        background.requests = Some(tx);
        let before = state(&real);
        real.clear_trace();
        assert!(!background.submit_original_diff_sides_v1(&mut app, work.clone()));
        assert!(!app.loading[View::Repository.index()]);
        assert!(background.current_original_diff_owner_v1.is_none());
        assert!(app.native_mutable_plan_v2.is_some());
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| {
                assert!(ui::draw(frame, &mut app, frame.area()));
            })
            .unwrap();
        let screen = terminal
            .backend()
            .buffer()
            .content
            .chunks(120)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            screen.contains("Selected request was not submitted"),
            "actual modal must show failed admission, not a perpetual running phase: {screen}"
        );
        assert!(!screen.contains("[Enter Confirm]") && !screen.contains("Confirming once"));
        assert!(screen.contains("[r Reload/inspect]"));
        let state = app.native_mutable_diff_v2.as_ref().unwrap();
        assert!(state.current_frame().is_none() && state.range().is_none());
        if let OriginalDiffWorkV1::MutableConfirm(request) = work.as_ref() {
            let operation = LocalGitNavigationReadV1::begin(
                request.source().intent().origin(),
                Default::default(),
            )
            .unwrap();
            assert!(
                request.prepare(&operation).is_err(),
                "queue-rejected explicit confirmation cannot bind a renewed operation"
            );
        }
        assert!(real.children().is_empty());
        assert_eq!(super::state(&real), before);
        drop(receiver);
    }
}
#[test]
fn native_mutable_plan_queue_full_and_disconnected_preview_has_visible_owned_diagnostic_zero_git() {
    rejected(false);
}
#[test]
fn native_mutable_plan_queue_full_and_disconnected_confirm_retires_exact_purpose_and_old_hits_zero_git(
) {
    rejected(true);
}
