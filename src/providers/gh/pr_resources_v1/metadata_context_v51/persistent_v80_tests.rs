//! Exercise the real bounded decoder across completed Original phases. These
//! are allocation-model contracts, not a substitute for the actual publisher
//! and keyboard/mouse PTY journeys that retain all three genuine Originals.
use super::*;

#[test]
fn pr_metadata_members_v80_three_completed_original_reads_keep_inherited_models_and_plans() {
    let mut inherited = 0;
    let mut expected_persistent = 0;
    for plan_bytes in [0, 4096, 8192] {
        inherited += plan_bytes;
        expected_persistent += plan_bytes;
        let mut phase = MetadataMembersHeapV62::new(inherited).unwrap();
        let before = phase.retained_bound();
        assert_eq!(before, inherited + 3 * CHILD_CAP_V51);
        let raw = phase.object(br#"{"login":"alice","id":7}"#).unwrap();
        let page = phase.eligible_page(br#"[{"id":11,"name":"bug"}]"#).unwrap();
        assert_eq!(raw["login"], "alice");
        assert_eq!(page[0]["id"], 11);
        let decoded_and_mapping = phase.retained_bound() - before;
        assert!(decoded_and_mapping > 0);
        expected_persistent += decoded_and_mapping;
        inherited = phase.finish_original_read_v80().unwrap();
        assert_eq!(inherited, expected_persistent);
    }
    // The actual post-write phase still pays its own live fixed-role scratch
    // and upgrades that scratch to the existing512KiB list allowance ONCE.
    let mut postwrite = MetadataMembersHeapV62::new(inherited).unwrap();
    let before = postwrite.retained_bound();
    assert!(postwrite.detail_pages(b"[[]]").unwrap().is_empty());
    let first = postwrite.retained_bound();
    assert!(first >= before + 3 * (512 * 1024 - CHILD_CAP_V51));
    assert!(postwrite.detail_pages(b"[[]]").unwrap().is_empty());
    let next = postwrite.retained_bound();
    assert!(next > first && next - first < CHILD_CAP_V51);
    // A live/detail heap cannot masquerade as a completed Original factory.
    assert!(postwrite.finish_original_read_v80().is_err());
}

#[test]
fn pr_metadata_members_v80_live_decoder_and_mapping_still_refuse_at_same_cap() {
    let inherited = MODEL_CAP_V51 - 3 * CHILD_CAP_V51 - 100;
    let mut phase = MetadataMembersHeapV62::new(inherited).unwrap();
    assert!(phase.object(b"{}").is_err()); // node+map before allocation
    let mut phase = MetadataMembersHeapV62::new(inherited - 1500).unwrap();
    // Raw empty object1152 fits, but its two mapping equivalents do not.
    assert!(phase.object(b"{}").is_err());
    assert!(MetadataMembersHeapV62::new(MODEL_CAP_V51 - 3 * CHILD_CAP_V51 + 1).is_err());
    assert!(MetadataMembersHeapV62::new(usize::MAX).is_err());
}

#[test]
fn pr_metadata_members_v80_completed_original_keeps_all_live_graph_debits() {
    let mut phase = MetadataMembersHeapV62::new(512 * 1024).unwrap();
    let before = phase.retained_bound();
    assert!(phase
        .object(br#"{"a":[],"actor":{"login":"alice"}}"#)
        .is_ok());
    let raw_and_mapping = phase.retained_bound() - before;
    assert_eq!(
        phase.finish_original_read_v80().unwrap(),
        512 * 1024 + raw_and_mapping
    );
    let mut detail = MetadataMembersHeapV62::new(MODEL_CAP_V51 - 3 * CHILD_CAP_V51).unwrap();
    assert!(detail.detail_pages(b"[[]]").is_err());
}
