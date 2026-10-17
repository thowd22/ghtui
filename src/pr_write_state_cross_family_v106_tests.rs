//! The actual raw ACK row never leaves its natural history module.
use super::*;
use crate::personal_pr_entry_v13::tests as personal;
use crate::pr_write_journal::cross_family_v106::fixtures::*;
pub(in crate::pr_write_journal) fn assert_final_ack_foreign_origin_v106() {
    let mut steps = issue_steps();
    steps.extend(issue_inspect_steps());
    steps.extend(state_bundle_steps());
    steps.extend(state_steps());
    let s = personal::setup(steps);
    let issue = open_issue(&s);
    let donor = unknown_issue(&s, &issue);
    ack_issue(&s, &issue, &donor);
    let state = open_state(&s);
    let victim = unknown_state(&s, &state);
    let (mut witness, current, cancel, deadline) = inspect_state(&s, &victim);
    let mut journal = PrWriteJournal::open(&database(&s)).unwrap();
    let row = witness
        .exact
        .take()
        .expect("Actual same-State raw ACK capture");
    row.validate_current(&journal).unwrap();
    s.api
        .validate_state_inspection_v68(&current, &cancel, deadline)
        .unwrap();
    s.api
        .reconcile_state_inspection_v68(current, &cancel, deadline)
        .unwrap();
    graft_issue(&s, donor.reference.attempt_id, victim.reference.attempt_id);
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
