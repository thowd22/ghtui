//! Real bounded four-document decoder/accounting contracts. No fabricated
//! Source/Current/receipt; all-ten actual producer pipelines remain required.
use super::*;

fn decoded(existing: usize) -> MetadataContextDocumentsV51 {
    let mut documents = MetadataContextDocumentsV51::new(existing).unwrap();
    for raw in [
        br#"{"body":"raw body","labels":[{"id":11,"node_id":"LABEL_11","name":"new"}]}"#.as_slice(),
        br#"{"id":2007,"assignees":[{"id":21,"login":"alice"}]}"#.as_slice(),
        br#"{"id":10000,"permissions":{"pull":true,"push":null}}"#.as_slice(),
        br#"{"body":null,"requested_reviewers":[],"requested_teams":[]}"#.as_slice(),
    ] {
        documents.accept_fixed_document(raw).unwrap();
    }
    // Actual two Members result-set phases coexist with all four raw trees.
    documents
        .heap
        .charge::<serde_json::Error>(2 * (8 * 1024 + 2 * 512))
        .unwrap();
    assert_eq!(documents.documents.len(), 4);
    assert_eq!(documents.documents[0]["labels"][0]["id"], 11);
    assert_eq!(documents.documents[1]["assignees"][0]["id"], 21);
    documents
}

#[test]
fn pr_metadata_members_v82_actual_four_decoders_copy_trees_not_scratch_and_preserve_ordinary_math()
{
    let existing = 512 * 1024;
    let mut members = decoded(existing);
    let before = members.heap.bytes;
    let scratch = 3 * CHILD_CAP_V51;
    let trees_and_sets = before - existing - scratch;
    assert!(trees_and_sets > 2 * 8 * 1024);
    charge_context_conversion_v82(&mut members.heap, ContextConversionV82::Members).unwrap();
    assert_eq!(members.heap.existing, existing);
    assert_eq!(members.heap.bytes, existing + scratch + 3 * trees_and_sets);
    assert_eq!(
        members.heap.bytes - members.heap.existing,
        scratch + 3 * trees_and_sets
    );
    // Four original Values remain present and unchanged while conversion is
    // charged; no scratch/model reset or emptied retained graph is the proof.
    assert_eq!(members.documents.len(), 4);
    assert_eq!(members.documents[3]["body"], Value::Null);
    let mut ordinary = decoded(existing);
    charge_context_conversion_v82(&mut ordinary.heap, ContextConversionV82::Ordinary).unwrap();
    assert_eq!(ordinary.heap.existing, existing);
    assert_eq!(
        ordinary.heap.bytes,
        existing + 3 * (scratch + trees_and_sets)
    );
    assert_eq!(ordinary.heap.bytes - members.heap.bytes, 2 * scratch);
    assert_eq!(ordinary.documents, members.documents);
}

#[test]
fn pr_metadata_members_v82_actual_decoder_conversion_at_four_mib_keeps_inherited_and_rejects_next_model(
) {
    let probe = decoded(0);
    let scratch = 3 * CHILD_CAP_V51;
    let trees_and_sets = probe.heap.bytes - scratch;
    let existing = MODEL_CAP_V51 - scratch - 3 * trees_and_sets;
    let mut exact = decoded(existing);
    charge_context_conversion_v82(&mut exact.heap, ContextConversionV82::Members).unwrap();
    assert_eq!(exact.heap.bytes, MODEL_CAP_V51);
    assert_eq!(exact.heap.existing, existing);
    assert!(exact.heap.charge::<serde_json::Error>(1).is_err());
    assert_eq!(exact.heap.bytes, MODEL_CAP_V51);
    assert_eq!(exact.documents.len(), 4);
    let mut over = decoded(existing + 1);
    let before = over.heap.bytes;
    assert!(charge_context_conversion_v82(&mut over.heap, ContextConversionV82::Members).is_err());
    assert_eq!(
        over.heap.bytes, before,
        "Refusal never resets inherited or live allocations"
    );
    assert_eq!(over.heap.existing, existing + 1);
    assert_eq!(over.documents.len(), 4);
    // Old ordinary profile still refuses its original full-scratch multiplier
    // at the same limit; it does not inherit the closed Members alternative.
    let mut ordinary = decoded(existing);
    assert!(
        charge_context_conversion_v82(&mut ordinary.heap, ContextConversionV82::Ordinary).is_err()
    );
}
