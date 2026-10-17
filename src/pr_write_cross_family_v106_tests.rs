//! Genuine cross-family journal/worker boundaries. Foreign rows are faults,
//! never issuers. Every negative snapshots the complete post-fault SQL image.
use super::*;
#[path = "pr_write_cross_family_v106_fixtures.rs"]
pub(in crate::pr_write_journal) mod fixtures;
use crate::{personal_pr_entry_v13::tests as personal, pr_state_model::PrStateCommand};
use fixtures::*;

#[test]
fn cross_family_state_issue_distinct_attempt_history_and_shared_unresolved_barrier_are_exact() {
    // One real unresolved family is ACKed through a genuine fresh read before
    // the other family begins. We never fabricate two healthy live permits.
    let mut steps = state_bundle_steps();
    steps.extend(state_steps());
    steps.extend(issue_steps());
    // The fresh opposite read reaches its real observed State, then the
    // ordinary local guard sees the shared blocker before issuing a Base or
    // closing/publishing a new Source. Do not invent a second live permit.
    steps.extend(state_steps().into_iter().take(6));
    let s = personal::setup(steps);
    let state = open_state(&s);
    let old = unknown_state(&s, &state);
    ack_state(&s, &old);
    let issue = open_issue(&s);
    assert_eq!(issue.catalogue.len(), 1);
    assert!(PrWriteJournal::open(&database(&s))
        .unwrap()
        .issue_link_action_revision_v87(&issue.base)
        .unwrap()
        .is_none());
    let current = unknown_issue(&s, &issue);
    let journal = PrWriteJournal::open(&database(&s)).unwrap();
    assert_eq!(old.target, current.target);
    assert!(old.reference.attempt_id < current.reference.attempt_id);
    let previous = journal
        .original_publication(&old.target, &old.reference)
        .unwrap()
        .unwrap();
    assert_eq!(previous.state, PrWriteState::Acknowledged);
    journal.state_origin_v68(&old.target, &previous).unwrap();
    journal
        .issue_link_origin_v87(
            &current.target,
            &journal.blocking(&current.target).unwrap().unwrap(),
        )
        .unwrap();
    let before = image(&s);
    let db = database(&s);
    let original =
        crate::pr_collaboration_origin_v17::PrCollaborationOpenIntentV17::capture_state_v68(
            &s.app,
            &db,
            PrStateCommand::Close,
        )
        .unwrap();
    let bounded = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    assert!(
        crate::scoped_pr_collaboration_source_v17::state_v68::open_state_v68(
            crate::scoped_pr_collaboration_source_v17::state_v68::StateOpenRequestV68 {
                opening: std::sync::Arc::clone(&original),
                command: PrStateCommand::Close
            },
            &bounded,
            &db,
            original.cancellation(),
            original.deadline(),
            |_, _, _| panic!(
                "Shared unresolved barrier must refuse before exposing an eligible State Base"
            ),
            |_: &()| Ok(())
        )
        .is_err()
    );
    assert_eq!(
        image(&s),
        after_opposite_personal_read_reservation(before),
        "Only the genuine Personal read reservation advances; the opposite unresolved barrier is not another seed"
    );
    assert_eq!(
        journal
            .blocking(&current.target)
            .unwrap()
            .unwrap()
            .publication_reference(),
        current.reference
    );
    assert_eq!(
        Cache::open(&database(&s))
            .unwrap()
            .draft(old.target.scope(), &old.permit.marker().selected_resource)
            .unwrap()
            .as_deref(),
        Some(old.body.as_str())
    );
    exact_transport_and_git(&s, s.steps.len());
}

#[test]
fn cross_family_state_issue_current_origin_graft_refuses_history_and_ack_capture() {
    for state_victim in [true, false] {
        if state_victim {
            let mut steps = issue_steps();
            steps.extend(issue_inspect_steps());
            steps.extend(state_bundle_steps());
            let s = personal::setup(steps);
            let issue = open_issue(&s);
            let old = unknown_issue(&s, &issue);
            ack_issue(&s, &issue, &old);
            let state = open_state(&s);
            let current = unknown_state(&s, &state);
            let journal = PrWriteJournal::open(&database(&s)).unwrap();
            journal
                .state_origin_v68(
                    &current.target,
                    &journal.blocking(&current.target).unwrap().unwrap(),
                )
                .unwrap();
            graft_issue(&s, old.reference.attempt_id, current.reference.attempt_id);
            let fault = image(&s);
            assert!(journal
                .state_origin_v68(
                    &current.target,
                    &journal.blocking(&current.target).unwrap().unwrap()
                )
                .is_err());
            assert!(journal
                .state_history_v74(
                    &current.target,
                    state.source.parent(),
                    PrStateCommand::Close
                )
                .is_err());
            assert!(journal
                .prepare_state_inspection_v74(
                    &current.target,
                    &current.reference,
                    state.source.opening()
                )
                .is_err());
            assert_eq!(image(&s), fault);
            exact_transport_and_git(&s, s.steps.len());
        } else {
            let mut steps = state_bundle_steps();
            steps.extend(state_steps());
            steps.extend(issue_steps());
            let s = personal::setup(steps);
            let state = open_state(&s);
            let old = unknown_state(&s, &state);
            ack_state(&s, &old);
            let issue = open_issue(&s);
            let current = unknown_issue(&s, &issue);
            let journal = PrWriteJournal::open(&database(&s)).unwrap();
            journal
                .issue_link_origin_v87(
                    &current.target,
                    &journal.blocking(&current.target).unwrap().unwrap(),
                )
                .unwrap();
            graft_state(&s, old.reference.attempt_id, current.reference.attempt_id);
            let fault = image(&s);
            assert!(journal
                .issue_link_origin_v87(
                    &current.target,
                    &journal.blocking(&current.target).unwrap().unwrap()
                )
                .is_err());
            assert!(journal
                .issue_link_history_v87(
                    &current.target,
                    issue.source.parent(),
                    crate::pr_issue_links_v87::IssueLinkCommandV87::LinkIssue
                )
                .is_err());
            assert!(journal
                .prepare_issue_link_inspection_v87(&current.target, &current.reference)
                .is_err());
            assert_eq!(image(&s), fault);
            exact_transport_and_git(&s, s.steps.len());
        }
    }
}

#[test]
fn cross_family_state_issue_exact_ack_final_cas_rechecks_reciprocal_origin_absence() {
    state_origin_v68::history_v74::cross_family_v106::assert_final_ack_foreign_origin_v106();
    issue_link_origin_v87::history_v87::cross_family_v106::assert_final_ack_foreign_origin_v106();
}

#[test]
fn cross_family_state_issue_terminal_and_claimed_receipt_reject_grafted_current_attempt() {
    fixtures::assert_real_receipt_terminal_grafts();
}

#[test]
fn cross_family_state_issue_one_lineage_kernel_retains_three_row_bound_and_latest_no_fallback() {
    assert!(
        conversation_action_lineage_v18::kernel_v52::reserve_origin_box_moves_v79().unwrap()
            <= 64 * 1024
    );
    let mut steps = Vec::new();
    for index in 0..4 {
        steps.extend(state_bundle_steps());
        if index < 3 {
            steps.extend(state_steps());
        }
    }
    // Three real ACK terminals, then a fourth NEW capture. Each source and
    // seed is newly issued; protected requests are never replayed as approval.
    let s = personal::setup(steps);
    let mut protected = Vec::new();
    let mut last = None;
    for _ in 0..3 {
        let state = open_state(&s);
        let journal = PrWriteJournal::open(&database(&s)).unwrap();
        let revision = journal.state_action_revision_v68(&state.base).unwrap();
        assert_eq!(revision.is_some(), last.is_some());
        if let Some(revision) = &revision {
            assert_eq!(Some(revision.reference()), last.as_ref());
        }
        let state = fixtures::state_with_revision(state, revision.as_ref());
        let a = unknown_state(&s, &state);
        ack_state(&s, &a);
        last = Some(a.reference.clone());
        protected.push((a.permit.marker().selected_resource.clone(), a.body));
    }
    let state = open_state(&s);
    let journal = PrWriteJournal::open(&database(&s)).unwrap();
    let revision = journal
        .state_action_revision_v68(&state.base)
        .unwrap()
        .unwrap();
    assert_eq!(Some(revision.reference()), last.as_ref());
    for (key, body) in &protected {
        assert_eq!(
            Cache::open(&database(&s))
                .unwrap()
                .draft(state.base.target().scope(), key)
                .unwrap()
                .as_ref(),
            Some(body)
        );
    }
    fixtures::fault_newest_state_index(&s, last.unwrap().attempt_id);
    let fault = image(&s);
    assert!(
        journal.state_action_revision_v68(&state.base).is_err(),
        "Malformed newest index must not select any older terminal"
    );
    assert_eq!(image(&s), fault);
    exact_transport_and_git(&s, s.steps.len());
}

#[test]
fn cross_family_state_issue_prpage_and_stateinbox_results_cannot_exchange_owners() {
    fixtures::assert_actual_worker_owner_exclusion();
}
