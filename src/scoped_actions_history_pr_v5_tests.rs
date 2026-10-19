//! UNRUN. Genuine Summary13, optional original Run18/Open12, current parent8.
//! Old durable H/B are historical; only actual CURRENT parent opens/closes READ.
use super::*;
use crate::{
    actions_write_journal::{
        ActionsReceipt, ActionsReceiptKind, ActionsWriteJournal, ActionsWriteState,
    },
    scoped_actions_native_v1::{self as native, publication_v3 as publication},
    scoped_actions_summary_history_v5::ActionsSummaryHistoryOriginV5 as Origin,
};

fn parent(head: &str) -> Value {
    let mut raw = resource_parent(REPO);
    raw["head"]["sha"] = json!(head);
    raw
}
fn history_phase(head: &str, closing: &str, uid: u64) -> Vec<GhStep> {
    let mut out = principal(42, "history-open");
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.push(get(&format!("repos/{REPO}/pulls/7"), parent(head)));
    out.push(get(&format!("repos/{REPO}/pulls/7"), parent(closing)));
    out.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    out.extend(principal(uid, "history-close"));
    assert_eq!(out.len(), 8);
    out
}
fn summary_app(fixture: &Fixture) -> App {
    let (source, active, github) = opened(fixture);
    App {
        view: View::Inbox,
        github: Some(github),
        remote_scope: Some(active),
        remote_scope_source: Some(source),
        ..Default::default()
    }
}
fn work(app: &mut App) -> Arc<crate::scoped_actions_v1::ScopedActionsWorkV1> {
    native::journal_v5::open_summary(app);
    native::take_work(app).expect("actual source-bound Summary history read")
}
#[test]
fn scoped_actions_control_v5_genuine_summary_history_no_run_read_write_target_or_fabricated_files_capability(
) {
    let steps = script(&history_phase(&"a".repeat(40), &"a".repeat(40), 42));
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = summary_app(&fixture);
    let provider = provider(&fixture);
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let nav = app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .as_ref()
        .unwrap()
        .clone();
    let work = work(&mut app);
    assert!(work.page.run().is_none());
    let owner = native::ScopedActionsOwnerV1::new(work).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let prepared = publication::execute(&owner, &provider, &database(&fixture), None).unwrap();
    assert!(publication::publish(&mut app, &owner, prepared));
    app.scoped_actions_owner_v1 = None;
    assert!(app.actions.as_ref().unwrap().publication_rows.is_empty());
    let count = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .count();
    assert!(native::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::DeleteRun)
    ));
    assert!(app.scoped_actions_work_v1.is_none());
    assert_eq!(app.overlay, Some(crate::app::Overlay::Notice));
    assert!(app.notice.contains("Protected history is READ/ACK only"));
    assert!(native::intercept(&mut app, &ActionsAction::Back));
    assert_eq!(
        app.view,
        View::Actions,
        "Back cannot bypass the actual owned explanation"
    );
    assert_eq!(app.overlay, Some(crate::app::Overlay::Notice));
    app.dispatch(Action::Close);
    assert!(
        app.overlay.is_none(),
        "Normal Close dismisses only the owned explanation"
    );
    assert!(native::intercept(&mut app, &ActionsAction::Back));
    assert_eq!(app.view, View::Inbox);
    assert!(Arc::ptr_eq(
        &nav,
        app.github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap()
    ));
    assert_eq!(
        fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap()
            .lines()
            .count(),
        count
    );
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state_resource(&fixture), git);
    exact(&fixture, &steps);
}
#[test]
fn scoped_actions_control_v5_summary_history_changed_closing_head_or_numeric_uid_never_publishes_private_rows(
) {
    for uid in [42, 43] {
        let closing = if uid == 42 {
            "c".repeat(40)
        } else {
            "a".repeat(40)
        };
        let mut phase = history_phase(&"a".repeat(40), &closing, uid);
        if uid == 42 {
            phase.truncate(5);
        }
        let steps = script(&phase);
        let fixture = Fixture::new();
        fixture.scripted_gh(&steps);
        let mut app = summary_app(&fixture);
        let before = sql_state(&fixture);
        let git = git_state_resource(&fixture);
        let owner = native::ScopedActionsOwnerV1::new(work(&mut app)).unwrap();
        app.scoped_actions_owner_v1 = Some(owner.clone());
        assert!(
            publication::execute(&owner, &provider(&fixture), &database(&fixture), None).is_err()
        );
        assert!(app.actions.as_ref().unwrap().publication_rows.is_empty());
        assert_eq!(sql_state(&fixture), before);
        assert_eq!(git_state_resource(&fixture), git);
        exact(&fixture, &steps);
    }
}
#[test]
fn scoped_actions_control_v5_summary_history_original_navigation_pointer_and_newer_overlay_are_pre_effect_guards(
) {
    let steps = script(&[]);
    let fixture = Fixture::new();
    fixture.scripted_gh(&steps);
    let mut app = summary_app(&fixture);
    let origin = Origin::capture(&app).unwrap();
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let selected = app.inbox_list.selected;
    app.inbox_list.selected = selected + 1;
    assert!(!origin.current(&app));
    app.inbox_list.selected = selected;
    app.overlay = Some(crate::app::Overlay::Help);
    let status = app.status.clone();
    let hits = format!("{:?}", app.hits);
    native::journal_v5::open_summary(&mut app);
    assert!(app.scoped_actions_v1.is_none());
    assert!(app.scoped_actions_work_v1.is_none());
    assert_eq!(app.overlay, Some(crate::app::Overlay::Help));
    assert_eq!(app.status, status);
    assert_eq!(format!("{:?}", app.hits), hits);
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state_resource(&fixture), git);
    exact(&fixture, &steps);
}
#[test]
fn scoped_actions_control_v5_current_pushed_summary_can_list_old_actual_pr_run_receipt_without_retargeting_heads(
) {
    let newer = "c".repeat(40);
    let mut reload = operation(REPO, 42);
    for step in &mut reload {
        if step.args.get(1) == Some(&format!("repos/{REPO}/pulls/7")) {
            step.stdout = parent(&newer).to_string().into_bytes();
        }
    }
    let mut tail = actions_rerun_v4::phase();
    tail.extend(reload);
    tail.extend(history_phase(&newer, &newer, 42));
    let (fixture, provider, mut app, steps) = actions_rerun_v4::setup(tail);
    assert!(native::intercept(
        &mut app,
        &ActionsAction::Compose(ActionsCommand::DeleteRun)
    ));
    let initial = native::take_work(&mut app).unwrap();
    let original = initial.page.run().unwrap().clone();
    let original_source = initial.source.clone();
    let owner = native::ScopedActionsOwnerV1::new(initial).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let result = publication::execute(&owner, &provider, &database(&fixture), None).unwrap();
    assert!(publication::publish(&mut app, &owner, result));
    app.scoped_actions_owner_v1 = None;
    let plan = crate::actions_workspace::ActionsPlan::prepare(
        app.actions.as_ref().unwrap(),
        ActionsCommand::DeleteRun,
        &Default::default(),
        &provider,
        &CancellationToken::default(),
    )
    .unwrap();
    let descriptor = plan
        .scoped_run_descriptor_v4(
            &provider,
            &original_source,
            &original,
            None,
            crate::providers::gh::RunMutation::DeleteRun,
        )
        .unwrap();
    let mut journal = ActionsWriteJournal::open(&database(&fixture)).unwrap();
    let permit = journal
        .begin(&descriptor.target, &descriptor.intent)
        .unwrap();
    journal
        .claim(&permit, &descriptor.target, &descriptor.intent)
        .unwrap();
    journal
        .mark_accepted_pending(
            &permit,
            &ActionsReceipt {
                kind: ActionsReceiptKind::RunDeletionAccepted,
                resource_id: Some(123),
                observed_attempt: Some(2),
                observed_run_id: None,
                original_dispatch_run_v7: None,
                workflow_remote_v9: None,
            },
        )
        .unwrap();
    let reference = permit.marker().publication_reference();
    drop(journal);
    // A new genuine selected PR read produces the NEW navigation and CURRENT
    // parent. No saved old head, DTO or existing RunTarget is reissued.
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    let active = app.remote_scope.as_ref().unwrap().clone();
    let github = call(&fixture, source.clone(), active.clone(), previous(&source))
        .unwrap()
        .workspace;
    app = App {
        view: View::Inbox,
        github: Some(github),
        remote_scope_source: Some(source),
        remote_scope: Some(active),
        ..Default::default()
    };
    let before = sql_state(&fixture);
    let git = git_state_resource(&fixture);
    let owner = native::ScopedActionsOwnerV1::new(work(&mut app)).unwrap();
    app.scoped_actions_owner_v1 = Some(owner.clone());
    let result = publication::execute(&owner, &provider, &database(&fixture), None).unwrap();
    assert!(publication::publish(&mut app, &owner, result));
    assert_eq!(app.actions.as_ref().unwrap().publication_rows.len(), 1);
    assert!(app.actions.as_ref().unwrap().publication_rows[0]
        .details
        .iter()
        .any(|line| line.contains(&"a".repeat(40))));
    let marker = ActionsWriteJournal::open(&database(&fixture))
        .unwrap()
        .original_publication(
            &provider,
            &app.actions.as_ref().unwrap().identity,
            &reference,
        )
        .unwrap();
    assert_eq!(marker.state, ActionsWriteState::AcceptedPending);
    assert_eq!(
        marker.context.original_pr_v3.as_ref().unwrap().head_sha,
        "a".repeat(40)
    );
    assert_eq!(marker.context.head_sha, Some("a".repeat(40)));
    assert_eq!(
        app.github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .head_sha,
        newer
    );
    assert_eq!(sql_state(&fixture), before);
    assert_eq!(git_state_resource(&fixture), git);
    exact(&fixture, &steps);
}
