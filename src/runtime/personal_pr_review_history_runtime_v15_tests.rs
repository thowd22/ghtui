//! Actual Background producer/completion, not handler/result substitutes.
//! No fake Catalogue/Files/Nav for a genuine ordinary Personal Summary.
use super::*;
use crate::{
    drafts::DraftWriter,
    scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::native_v2 as reviews,
};
use f::review_history_v15 as history_fixture;

fn queued_summary(outcome: usize) -> (f::Setup, Background, DraftWriter) {
    let mut steps = if outcome == 2 {
        Vec::new()
    } else {
        history_fixture::summary_phase()
    };
    if outcome == 1 {
        steps[7] = f::get("user", serde_json::json!({"id":43,"login":"octocat"}));
    }
    let mut s = f::setup(steps);
    history_fixture::seed(&mut s);
    s.app.dispatch(crate::app::Action::Github(
        crate::github_workspace::GithubAction::ReviewOperationHistoryV3,
    ));
    assert!(reviews::capture_requested_history_v15(
        &mut s.app,
        &f::database(&s.fixture)
    ));
    let work = s.app.scoped_reviews_work_v2.take().unwrap();
    let work = if outcome == 2 {
        reviews::actual_worker_panic_for_test(work)
    } else {
        work
    };
    let writer = DraftWriter::new(f::database(&s.fixture));
    let mut background = Background::new();
    assert!(background.submit_scoped_reviews_v2(
        &mut s.app,
        work,
        s.api.clone(),
        f::database(&s.fixture),
        writer.client()
    ));
    (s, background, writer)
}
#[test]
fn personal_pr_summary_v15_actual_background_admits_history_without_current_files_or_catalogue_source(
) {
    let (mut s, mut background, _writer) = queued_summary(0);
    let body = s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .details
        .summary
        .pr
        .body
        .as_ptr();
    let completion = actual(&mut background);
    assert!(completion.result.is_ok());
    put(&background, completion);
    assert!(background.poll(&mut s.app));
    assert!(s
        .app
        .scoped_review_history_v3
        .as_ref()
        .unwrap()
        .owns(&s.app));
    assert!(s.app.scoped_reviews_v2.is_none());
    assert!(s.app.personal_pr_source_v13.is_none() && s.app.remote_scope_source.is_none());
    assert!(s
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .is_none());
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .body
            .as_ptr(),
        body
    );
    assert_eq!(s.app.overlay, Some(crate::app::Overlay::ScopedReviewsV2));
    f::exact(&s, s.steps.len());
}
#[test]
fn personal_pr_summary_v15_actual_late_completion_and_original_account_loss_preserve_newer_slot_page_and_status(
) {
    for account_loss in [false, true] {
        for outcome in [0, 1, 2] {
            let (mut s, mut background, _writer) = queued_summary(outcome);
            let completion = actual(&mut background);
            assert_eq!(completion.result.is_err(), outcome == 2);
            if !account_loss {
                s.app = f::setup(Vec::new()).app;
            }
            assert!(background.submit(&mut s.app, View::Inbox, |_| Ok(PreparedView::default())));
            let successor = actual(&mut background);
            assert!(successor.result.is_ok());
            let (generation, token) = background
                .current
                .get(&View::Inbox.index())
                .cloned()
                .unwrap();
            s.app.status = "NEWER_SUMMARY_SLOT_STATUS".into();
            s.app.loading[View::Inbox.index()] = true;
            let page = s
                .app
                .github
                .as_ref()
                .unwrap()
                .pr
                .as_ref()
                .unwrap()
                .details
                .summary
                .pr
                .body
                .as_ptr();
            let context = s.app.context.clone();
            if account_loss {
                s.app.github.as_mut().unwrap().identity.account_id = Some(43);
                assert!(reviews::retire_invalid(&mut s.app));
            }
            put(&background, completion);
            let _ = background.poll(&mut s.app);
            assert_eq!(
                background.current.get(&View::Inbox.index()).unwrap().0,
                generation
            );
            assert!(background
                .current
                .get(&View::Inbox.index())
                .unwrap()
                .1
                .same_instance(&token));
            assert!(!token.is_cancelled());
            assert_eq!(s.app.status, "NEWER_SUMMARY_SLOT_STATUS");
            assert!(s.app.loading[View::Inbox.index()]);
            assert!(s.app.scoped_review_history_v3.is_none());
            if account_loss {
                assert!(s.app.github.is_none());
                assert!(!s.app.context.contains(f::REPO));
            } else {
                assert_eq!(
                    s.app
                        .github
                        .as_ref()
                        .unwrap()
                        .pr
                        .as_ref()
                        .unwrap()
                        .details
                        .summary
                        .pr
                        .body
                        .as_ptr(),
                    page
                );
                assert_eq!(s.app.context, context);
            }
            background.cancel(View::Inbox);
            drop(successor);
            f::exact(&s, s.steps.len());
        }
    }
}
