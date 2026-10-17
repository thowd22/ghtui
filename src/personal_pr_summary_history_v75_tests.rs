//! DECLARED / UNRUN. Genuine personal opening/detail and shared descriptor;
//! no source, Files, membership receipt or permission is constructed by tests.
use super::*;
use crate::scoped_pr_observed_tabs_v1::resources::threads_v1::{
    history_v1 as history, native_v1 as native,
};
fn history_steps(uid: u64) -> Vec<crate::test_support::GhStep> {
    let mut steps = f::principal();
    steps[1] = f::get(
        "user",
        serde_json::json!({"id":uid,"login":"current-numeric-history-viewer"}),
    );
    if uid != 42 {
        return steps;
    }
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repository()));
    steps.push(f::get(&format!("repos/{}/pulls/7", f::REPO), f::parent()));
    steps.push(f::get(&format!("repos/{}/pulls/7", f::REPO), f::parent()));
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repository()));
    steps.extend(f::principal());
    assert_eq!(steps.len(), 8);
    steps
}
#[test]
fn file_comments_v75_personal_summary_history_uses_eight_original_children_without_files_or_source_mint(
) {
    let s = f::setup(history_steps(42));
    let intent =
        history::ThreadHistoryIntentV1::capture_personal(&s.app, &f::database(&s.fixture)).unwrap();
    let cancel = intent.personal_summary().unwrap().cancellation().clone();
    let deadline = intent.personal_summary().unwrap().deadline();
    let prepared = history::files_v75::load(
        intent.clone(),
        None,
        &s.api,
        &f::database(&s.fixture),
        &cancel,
        deadline,
    )
    .unwrap();
    assert!(prepared.labels.is_empty());
    assert!(prepared.detail.is_empty());
    assert_eq!(prepared.identity.account_id, Some(42));
    assert!(intent.owns(&s.app));
    assert!(s.app.personal_pr_source_v13.is_none());
    assert!(s.app.remote_scope_source.is_none());
    assert!(s
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .is_none());
    assert!(s.app.github.as_ref().unwrap().scoped_pr_resource.is_none());
    f::exact(&s, s.steps.len());
}
#[test]
fn file_comments_v75_personal_history_rejects_wrong_numeric_account_before_body_and_preserves_all_effects(
) {
    let s = f::setup(history_steps(43));
    let intent =
        history::ThreadHistoryIntentV1::capture_personal(&s.app, &f::database(&s.fixture)).unwrap();
    let cancel = intent.personal_summary().unwrap().cancellation().clone();
    let deadline = intent.personal_summary().unwrap().deadline();
    assert!(history::files_v75::load(
        intent,
        None,
        &s.api,
        &f::database(&s.fixture),
        &cancel,
        deadline
    )
    .is_err());
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
    f::exact(&s, s.steps.len());
}
#[test]
fn file_comments_v75_personal_initial_history_adopts_exact_descriptor_token_and_explicit_refresh_has_new_phase(
) {
    let mut s = f::setup(Vec::new());
    s.app.github_action = Some(crate::github_workspace::GithubAction::ThreadHistoryV1);
    native::capture_personal_history_requested_v75(&mut s.app, &f::database(&s.fixture));
    assert!(s.app.github_action.is_none());
    let original = s
        .app
        .scoped_thread_history_v1
        .as_ref()
        .unwrap()
        .intent
        .clone();
    let descriptor = original.personal_summary().unwrap();
    let first = s.app.scoped_threads_work_v1.take().unwrap();
    assert!(first
        .cancellation()
        .same_instance(descriptor.cancellation()));
    assert_eq!(first.deadline(), descriptor.deadline());
    first.cancellation().cancel();
    assert!(
        original.owns(&s.app),
        "canceling an operation is not a 30s presentation lease"
    );
    native::refresh_history(&mut s.app);
    let next = s.app.scoped_threads_work_v1.as_ref().unwrap();
    assert!(!next.cancellation().same_instance(first.cancellation()));
    assert!(Arc::ptr_eq(next.history_intent().unwrap(), &original));
    assert!(!next.expired());
    assert!(s.app.personal_pr_source_v13.is_none());
    f::exact(&s, s.steps.len());
}
#[test]
fn file_comments_v75_personal_summary_idle_account_loss_hides_private_underlay_without_touching_new_slot_status(
) {
    let mut s = f::setup(Vec::new());
    s.app.github_action = Some(crate::github_workspace::GithubAction::ThreadHistoryV1);
    native::capture_personal_history_requested_v75(&mut s.app, &f::database(&s.fixture));
    let work = s.app.scoped_threads_work_v1.take().unwrap();
    s.app.status = "INDEPENDENT_SUCCESSOR_STATUS".into();
    s.app.loading[View::Inbox.index()] = true;
    s.app.github.as_mut().unwrap().identity.account_id = Some(43);
    assert!(history::ui::retire_invalid(&mut s.app));
    assert!(s.app.scoped_thread_history_v1.is_none());
    assert!(s.app.github.is_none());
    assert_eq!(s.app.status, "INDEPENDENT_SUCCESSOR_STATUS");
    assert!(s.app.loading[View::Inbox.index()]);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut s.app))
        .unwrap();
    let text = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(!text.contains(f::BODY));
    assert!(!text.contains(f::REPO));
    assert!(!text.contains("FILE comment publication history"));
    assert!(!work.presentation_matches(&s.app));
    f::exact(&s, s.steps.len());
}
