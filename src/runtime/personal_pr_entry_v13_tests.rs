//! Actual Request/Completion delivery. No direct invented owners or alternate
//! channel models; strict fixtures independently prove every provider child.
use super::*;
use crate::personal_pr_entry_v13::{self as native, tests as f};
use std::{sync::mpsc, time::Duration};

#[path = "personal_pr_images_v13_tests.rs"]
mod images;
#[path = "personal_pr_review_history_runtime_v15_tests.rs"]
mod review_history_v15;

fn actual(background: &mut Background) -> Completion {
    background
        .completions
        .recv_timeout(Duration::from_secs(8))
        .expect("actual original worker completion")
}
fn put(background: &Background, completion: Completion) {
    background
        .inline_completion_sender_v1
        .send(completion)
        .unwrap();
}
fn queued() -> (f::Setup, Background, Arc<native::PersonalPrEntryOwnerV13>) {
    let mut s = f::setup(f::phase());
    let work = native::PersonalPrEntryWorkV13::capture(&s.app, &f::database(&s.fixture)).unwrap();
    let mut background = Background::new();
    assert!(background.submit_personal_pr_entry_v13(&mut s.app, work, s.api.clone()));
    let owner = s.app.personal_pr_entry_owner_v13.clone().unwrap();
    (s, background, owner)
}
#[test]
fn personal_pr_entry_v13_runtime_actual_channel_publish_and_back_preserve_original_allocations() {
    let (mut s, mut background, _) = queued();
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    let body = page.details.summary.pr.body.as_ptr();
    let rows = page.rows.as_ptr();
    let completion = actual(&mut background);
    assert!(completion.result.is_ok());
    put(&background, completion);
    assert!(background.poll(&mut s.app));
    assert!(s.app.personal_pr_source_v13.is_some());
    assert!(!background.current.contains_key(&View::Inbox.index()));
    assert!(!s.app.loading[View::Inbox.index()]);
    s.app.dispatch(crate::app::Action::Github(
        crate::github_workspace::GithubAction::Back,
    ));
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.rows.as_ptr(), rows);
    assert_eq!(page.details.summary.pr.body.as_ptr(), body);
    assert!(s.app.personal_pr_source_v13.is_none());
    f::exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_runtime_full_and_disconnected_queue_do_not_take_page_or_touch_epochs() {
    for disconnected in [false, true] {
        let mut s = f::setup(Vec::new());
        let work =
            native::PersonalPrEntryWorkV13::capture(&s.app, &f::database(&s.fixture)).unwrap();
        let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
        let body = page.details.summary.pr.body.as_ptr();
        let rows = page.rows.as_ptr();
        let mut background = Background::new();
        let original_generation = background.generation;
        let original_status = s.app.status.clone();
        let (sender, receiver) = mpsc::sync_channel(0);
        background.requests = Some(sender);
        let held = if disconnected {
            drop(receiver);
            None
        } else {
            Some(receiver)
        };
        assert!(!background.submit_personal_pr_entry_v13(&mut s.app, work, s.api.clone()));
        assert_eq!(background.generation, original_generation);
        assert!(background.current.is_empty());
        assert!(background.current_personal_pr_entry_v13.is_none());
        assert!(s.app.personal_pr_entry_owner_v13.is_none());
        assert_eq!(s.app.status, original_status);
        assert!(!s.app.loading[View::Inbox.index()]);
        let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
        assert_eq!(page.rows.as_ptr(), rows);
        assert_eq!(page.details.summary.pr.body.as_ptr(), body);
        f::exact(&s, f::opening().len());
        drop(held);
    }
}
#[test]
fn personal_pr_entry_v13_runtime_independently_live_inbox_refuses_before_cancel_or_status_effect() {
    let mut s = f::setup(Vec::new());
    let work = native::PersonalPrEntryWorkV13::capture(&s.app, &f::database(&s.fixture)).unwrap();
    let mut background = Background::new();
    assert!(background.submit(&mut s.app, View::Inbox, |_| Ok(PreparedView::default())));
    let (generation, token) = background.current.get(&View::Inbox.index()).unwrap();
    let generation = *generation;
    let token = token.clone();
    let status = s.app.status.clone();
    assert!(!background.submit_personal_pr_entry_v13(&mut s.app, work, s.api.clone()));
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
    assert_eq!(s.app.status, status);
    assert!(s.app.loading[View::Inbox.index()]);
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
            .body,
        f::BODY
    );
    f::exact(&s, f::opening().len());
}
#[test]
fn personal_pr_entry_v13_runtime_cancel_after_real_completion_discards_late_private_result() {
    let (mut s, mut background, owner) = queued();
    let completion = actual(&mut background);
    let rows = s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .rows
        .as_ptr();
    s.app.dispatch(crate::app::Action::Cancel);
    assert!(s.app.personal_pr_entry_cancel_v13);
    s.app.personal_pr_entry_cancel_v13 = false;
    assert!(background.cancel_personal_pr_entry_v13(&mut s.app));
    assert!(owner.cancellation().is_cancelled());
    let status = s.app.status.clone();
    put(&background, completion);
    assert!(!background.poll(&mut s.app));
    assert!(s.app.personal_pr_source_v13.is_none());
    assert_eq!(s.app.status, status);
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .rows
            .as_ptr(),
        rows
    );
    f::exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_runtime_mixed_success_cannot_cross_closed_result_boundary() {
    let (mut s, mut background, owner) = queued();
    let mut completion = actual(&mut background);
    completion.result.as_mut().unwrap().clear_private = true;
    let status = s.app.status.clone();
    let generation = background.current.get(&View::Inbox.index()).unwrap().0;
    put(&background, completion);
    assert!(!background.poll(&mut s.app));
    assert_eq!(s.app.status, status);
    assert!(s.app.github.is_some());
    assert_eq!(
        background.current.get(&View::Inbox.index()).unwrap().0,
        generation
    );
    assert!(Arc::ptr_eq(
        s.app.personal_pr_entry_owner_v13.as_ref().unwrap(),
        &owner
    ));
    assert!(background.cancel_personal_pr_entry_v13(&mut s.app));
    f::exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_runtime_original_account_loss_hides_page_despite_lost_physical_fence() {
    let (mut s, mut background, _) = queued();
    let mut old = actual(&mut background);
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
    let private_context = s.app.context.clone();
    assert!(private_context.contains(f::REPO));
    assert!(private_context.contains(&"a".repeat(40)));
    // Real generic replacement, not a fabricated generation or owner.
    assert!(background.submit(&mut s.app, View::Inbox, |_| Ok(PreparedView::default())));
    let successor = background
        .current
        .get(&View::Inbox.index())
        .unwrap()
        .clone();
    let status = s.app.status.clone();
    s.app.github.as_mut().unwrap().identity.account_id = Some(43);
    background.retire_stale_personal_pr_entry_v13(&mut s.app);
    assert!(s.app.github.is_none());
    assert!(s.app.personal_pr_source_v13.is_none());
    let current = background.current.get(&View::Inbox.index()).unwrap();
    assert_eq!(current.0, successor.0);
    assert!(current.1.same_instance(&successor.1));
    assert!(!successor.1.is_cancelled());
    assert!(s.app.loading[View::Inbox.index()]);
    assert_eq!(s.app.status, status);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let painted = buffer
        .content
        .chunks(120)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!painted.contains(f::BODY));
    assert!(!painted.contains(f::REPO));
    assert!(!painted.contains(&"a".repeat(40)));
    assert!(!painted.contains("Actual personal source"));
    assert!(painted.contains("private content hidden"));
    assert_eq!(s.app.status, status);
    assert!(s.app.loading[View::Inbox.index()]);
    assert_eq!(
        background.poll_personal_pr_entry_v13(&mut s.app, &mut old),
        Some(false)
    );
    f::exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_runtime_new_actual_page_and_successor_slot_survive_old_owner_cleanup() {
    let mut tail = f::phase();
    let mut next = f::opening();
    next.drain(..5);
    tail.extend(next);
    let mut s = f::setup(tail);
    let work = native::PersonalPrEntryWorkV13::capture(&s.app, &f::database(&s.fixture)).unwrap();
    let mut background = Background::new();
    assert!(background.submit_personal_pr_entry_v13(&mut s.app, work, s.api.clone()));
    let mut old = actual(&mut background);
    let github = s.app.github.take().unwrap();
    let key = github.selected.clone().unwrap();
    s.app.github = Some(
        github
            .open(&key, &s.api, &CancellationToken::default())
            .unwrap(),
    );
    let ticket = s
        .app
        .github
        .as_ref()
        .unwrap()
        .personal_pr_page_ticket_v13
        .clone()
        .unwrap();
    let rows = s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .rows
        .as_ptr();
    assert!(background.submit(&mut s.app, View::Inbox, |_| Ok(PreparedView::default())));
    let successor = background
        .current
        .get(&View::Inbox.index())
        .unwrap()
        .clone();
    let status = s.app.status.clone();
    background.retire_stale_personal_pr_entry_v13(&mut s.app);
    assert!(Arc::ptr_eq(
        s.app
            .github
            .as_ref()
            .unwrap()
            .personal_pr_page_ticket_v13
            .as_ref()
            .unwrap(),
        &ticket
    ));
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .rows
            .as_ptr(),
        rows
    );
    assert_eq!(
        background.current.get(&View::Inbox.index()).unwrap().0,
        successor.0
    );
    assert!(!successor.1.is_cancelled());
    assert_eq!(s.app.status, status);
    assert_eq!(
        background.poll_personal_pr_entry_v13(&mut s.app, &mut old),
        Some(false)
    );
    f::exact(&s, s.steps.len());
}
