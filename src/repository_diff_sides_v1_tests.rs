use super::*;
use crate::{git::Comparison, original_diff_sides_v1_test_support as support};
fn fixture() -> (
    crate::local_commit_actor_origin_v1_test_support::Real,
    OriginalDiffPickerV1,
) {
    let (real, a, b, _) = support::fixture("sha256");
    let workspace = support::workspace(&real, Comparison::Snapshots { base: a, head: b });
    let picker = support::state(&workspace);
    (real, picker)
}
fn prepare(picker: &mut OriginalDiffPickerV1, request: Arc<OriginalDiffQueryV1>) {
    let op =
        LocalGitNavigationReadV1::begin(request.intent().origin(), Default::default()).unwrap();
    let frame = OriginalDiffFrameV1::prepare(request, &op).unwrap();
    op.seal().unwrap();
    assert!(picker.accept(frame));
}
#[test]
fn original_diff_sides_query_old_ticket_refused_even_same_raw_member_survives() {
    let (real, mut picker) = fixture();
    let frame = picker.frame().unwrap();
    let index = (0..frame.len())
        .find(|index| {
            frame
                .member(*index)
                .unwrap()
                .path(Side::New)
                .is_some_and(|path| path.0 == b"shared")
        })
        .unwrap();
    let ticket = frame.ticket(index, Side::New).unwrap();
    real.clear_trace();
    let OriginalDiffEffectV1::Query(request) = picker
        .apply(OriginalDiffActionV1::Insert("shared".into()))
        .unwrap()
    else {
        panic!("worker query required")
    };
    assert!(picker
        .apply(OriginalDiffActionV1::Activate(ticket.clone()))
        .is_err());
    prepare(&mut picker, request);
    assert_eq!(picker.frame().unwrap().len(), 1);
    assert_eq!(
        picker.frame().unwrap().member(0).unwrap().index(),
        ticket.index()
    );
    assert!(picker
        .apply(OriginalDiffActionV1::Activate(ticket))
        .is_err());
    assert!(real.children().is_empty());
}
#[test]
fn original_diff_sides_query_editor_unicode_selection_caret_and_rejected_bounds() {
    let (_, mut picker) = fixture();
    picker
        .apply(OriginalDiffActionV1::Paste("café👩‍👩‍👧".into()))
        .unwrap();
    picker.apply(OriginalDiffActionV1::Backspace).unwrap();
    assert_eq!(picker.query.text, "café");
    picker.apply(OriginalDiffActionV1::SelectAll).unwrap();
    picker
        .apply(OriginalDiffActionV1::Insert("shared".into()))
        .unwrap();
    let before = picker.query.text.clone();
    assert!(picker
        .apply(OriginalDiffActionV1::Paste("x".repeat(257)))
        .is_err());
    assert!(picker
        .apply(OriginalDiffActionV1::Paste("hidden\ncontrol".into()))
        .is_err());
    assert_eq!(picker.query.text, before);
    picker
        .apply(OriginalDiffActionV1::Edge {
            end: false,
            select: false,
        })
        .unwrap();
    assert_eq!(picker.query.cursor, 0);
}
#[test]
fn original_diff_sides_absent_is_member_specific_explanation_no_new_read() {
    let (real, mut picker) = fixture();
    let frame = picker.frame().unwrap();
    let index = (0..frame.len())
        .find(|index| {
            frame
                .member(*index)
                .unwrap()
                .path(Side::New)
                .is_some_and(|path| path.0 == b"added")
        })
        .unwrap();
    let ticket = frame.ticket(index, Side::Old).unwrap();
    real.clear_trace();
    assert!(
        matches!(picker.apply(OriginalDiffActionV1::Activate(ticket)),Ok(OriginalDiffEffectV1::Explain(reason)) if reason.contains("Old side is absent"))
    );
    assert!(real.children().is_empty());
}
#[test]
fn original_diff_sides_late_query_success_and_failure_do_not_replace_new_source() {
    let (_, mut picker) = fixture();
    let old = picker.request().unwrap();
    let current = picker.request().unwrap();
    let op = LocalGitNavigationReadV1::begin(old.intent().origin(), Default::default()).unwrap();
    let frame = OriginalDiffFrameV1::prepare(old.clone(), &op).unwrap();
    op.seal().unwrap();
    assert!(!picker.accept(frame));
    assert!(!picker.fail(&old, "late diagnostic"));
    assert!(picker.accepts(&current));
}
#[test]
fn original_diff_sides_page_and_no_match_keep_honest_known_not_empty_comparison() {
    let (real, mut picker) = fixture();
    real.clear_trace();
    let OriginalDiffEffectV1::Query(request) = picker
        .apply(OriginalDiffActionV1::Insert(
            "not-in-this-original-source".into(),
        ))
        .unwrap()
    else {
        panic!()
    };
    prepare(&mut picker, request);
    assert_eq!(picker.frame().unwrap().known_matches(), 0);
    assert!(picker.frame().unwrap().status().contains("0 known matches"));
    assert!(!picker.frame().unwrap().source().members().is_empty());
    assert!(picker
        .apply(OriginalDiffActionV1::OpenSide(Side::New))
        .is_err());
    assert!(matches!(
        picker.apply(OriginalDiffActionV1::Page(1)),
        Ok(OriginalDiffEffectV1::None)
    ));
    assert!(real.children().is_empty());
}
