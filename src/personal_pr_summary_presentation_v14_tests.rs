//! Actual existing personal queue/detail factories, not decoded source fixtures.
use super::super::tests as f;
use super::*;
use crate::app::Overlay;
#[path = "personal_pr_summary_history_v75_tests.rs"]
mod history_v75;

#[test]
fn personal_pr_entry_v13_summary_capture_is_bounded_cached_and_does_not_mint_source_or_move_body() {
    let mut s = f::setup(Vec::new());
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.tab, PrTab::Summary);
    let body = page.details.summary.pr.body.as_ptr();
    let rows = page.rows.as_ptr();
    let parent =
        PersonalPrSummaryPresentationV14::capture(&s.app, &f::database(&s.fixture)).unwrap();
    assert!(parent.matches_capture(&s.app));
    assert!(parent.owns_presentation(&s.app));
    assert_eq!(parent.scope().login, "user-id:42");
    assert_eq!(parent.database(), f::database(&s.fixture));
    assert_eq!(parent.parent().identity().account_id, Some(42));
    assert!(parent.deadline() > Instant::now());
    assert!(s.app.personal_pr_source_v13.is_none());
    assert!(s.app.personal_pr_entry_work_v13.is_none());
    assert!(s
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .is_none());
    s.app.overlay = Some(Overlay::Notice);
    assert!(
        !parent.matches_capture(&s.app),
        "capture never treats an overlay as idle"
    );
    assert!(
        parent.owns_presentation(&s.app),
        "display provenance does not claim owned overlay permission"
    );
    parent.cancellation().cancel();
    assert!(!parent.matches_capture(&s.app));
    assert!(
        parent.owns_presentation(&s.app),
        "canceled initial operation cannot erase current readonly display"
    );
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.details.summary.pr.body.as_ptr(), body);
    assert_eq!(page.rows.as_ptr(), rows);
    f::exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_summary_exact_account_loss_hides_actual_private_context_body_without_newer_status_effect(
) {
    let mut s = f::setup(Vec::new());
    let parent =
        PersonalPrSummaryPresentationV14::capture(&s.app, &f::database(&s.fixture)).unwrap();
    s.app.content[View::Inbox.index()] = vec![s
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
        .clone()];
    let status = s.app.status.clone();
    s.app.loading[View::Inbox.index()] = true; // successor slot's visible loading
    s.app.github.as_mut().unwrap().identity.account_id = Some(43);
    assert!(parent.same_presentation(&s.app));
    assert!(!parent.owns_presentation(&s.app));
    assert!(parent.retire_account_mismatch(&mut s.app));
    assert!(s.app.github.is_none());
    assert_eq!(s.app.status, status);
    assert!(s.app.loading[View::Inbox.index()]);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    let screen = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(!screen.contains(f::BODY));
    assert!(!screen.contains(f::REPO));
    assert!(!screen.contains(&"a".repeat(40)));
    assert!(screen.contains("private content hidden"));
    assert_eq!(s.app.status, status);
    assert!(s.app.loading[View::Inbox.index()]);
    f::exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_summary_new_actual_page_ticket_survives_old_account_cleanup() {
    let mut tail = f::opening();
    tail.drain(..5);
    let mut s = f::setup(tail);
    let parent =
        PersonalPrSummaryPresentationV14::capture(&s.app, &f::database(&s.fixture)).unwrap();
    let github = s.app.github.take().unwrap();
    let selected = github.selected.clone().unwrap();
    s.app.github = Some(
        github
            .open(&selected, &s.api, &CancellationToken::default())
            .unwrap(),
    );
    let next = s
        .app
        .github
        .as_ref()
        .unwrap()
        .personal_pr_page_ticket_v13
        .clone()
        .unwrap();
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
    let context = s.app.context.clone();
    let content = s.app.content[View::Inbox.index()].clone();
    let status = s.app.status.clone();
    s.app.github.as_mut().unwrap().identity.account_id = Some(43);
    assert!(!parent.same_presentation(&s.app));
    assert!(!parent.retire_account_mismatch(&mut s.app));
    assert!(Arc::ptr_eq(
        s.app
            .github
            .as_ref()
            .unwrap()
            .personal_pr_page_ticket_v13
            .as_ref()
            .unwrap(),
        &next
    ));
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
    assert_eq!(s.app.context, context);
    assert_eq!(s.app.content[View::Inbox.index()], content);
    assert_eq!(s.app.status, status);
    f::exact(&s, s.steps.len());
}
