//! The actual raw ACK row never leaves its natural history module.
use super::*;
use crate::personal_pr_entry_v13::tests as personal;
use crate::pr_write_journal::cross_family_v106::fixtures::*;
pub(in crate::pr_write_journal) fn assert_final_ack_foreign_origin_v106() {
    let mut steps = state_bundle_steps();
    steps.extend(state_steps());
    steps.extend(issue_steps());
    steps.extend(issue_inspect_steps());
    let s = personal::setup(steps);
    let state = open_state(&s);
    let donor = unknown_state(&s, &state);
    ack_state(&s, &donor);
    let issue = open_issue(&s);
    let victim = unknown_issue(&s, &issue);
    let (mut witness, current, cancel, deadline) = inspect_issue(&s, &issue, &victim);
    let mut journal = PrWriteJournal::open(&database(&s)).unwrap();
    let row = witness
        .exact
        .take()
        .expect("Actual same-Issue raw ACK capture");
    row.validate_current(&journal).unwrap();
    s.api
        .validate_issue_link_inspection_v87(&current, &cancel, deadline)
        .unwrap();
    s.api
        .reconcile_issue_link_inspection_v87(current, &cancel, deadline)
        .unwrap();
    graft_state(&s, donor.reference.attempt_id, victim.reference.attempt_id);
    let fault = image(&s);
    assert!(journal
        .commit_exact_ack_v17(row, &cancel, deadline)
        .is_err());
    assert_eq!(
        image(&s),
        fault,
        "Final SAME raw CAS must refuse the late foreign origin and roll back every row"
    );
    assert_eq!(
        journal
            .blocking(&victim.target)
            .unwrap()
            .unwrap()
            .publication_reference(),
        victim.reference
    );
    assert_eq!(
        Cache::open(&database(&s))
            .unwrap()
            .draft(
                victim.target.scope(),
                &victim.permit.marker().selected_resource
            )
            .unwrap()
            .as_deref(),
        Some(victim.body.as_str())
    );
    exact_transport_and_git(&s, s.steps.len());
}
