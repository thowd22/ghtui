//! SOURCE-ONLY local-metadata/real-SQL contracts. Test JSON is explicitly
//! untrusted local intent, never an opaque RepositoryCatalogueRead or auth proof.
use super::*;
use crate::storage::{Cache, Record};
use serde_json::json;

fn scope() -> Scope {
    Scope::github("github.invalid", Some(42)).unwrap()
}
fn source() -> IntentSource {
    IntentSource {
        kind: IntentSourceKind::Organization,
        owner: Some("example".into()),
        owner_id: Some(71),
        owner_node_id: Some("OWNER_71".into()),
    }
}
fn reference(id: u64, name: &str, node: Option<&str>) -> Value {
    json!({"id":id,"node_id":node,"last_name":name})
}
fn local() -> Value {
    json!({"version":1,"scope":scope(),"source":source(),"pins":{"11000":reference(11000,"example/repo1000",Some("R_11000"))},"sets":{"focused":{"10000":reference(10000,"example/repo0000",Some("R_10000")),"11000":reference(11000,"example/repo1000",Some("R_11000"))}}})
}
fn decode(value: &Value) -> Result<CatalogueIntentV1, String> {
    CatalogueIntentV1::from_value(value, &scope(), &source())
}

#[test]
fn repo_catalog_intent_exact_roundtrip_members_nodes_and_empty_scope_do_not_broaden() {
    let value = local();
    let intent = decode(&value).unwrap();
    assert_eq!(intent.to_value().unwrap(), value);
    assert_eq!(
        intent.sets()["focused"].keys().copied().collect::<Vec<_>>(),
        vec![10000, 11000]
    );
    assert_eq!(intent.pins()[&11000].node_id.as_deref(), Some("R_11000"));
    let mut empty = local();
    empty["pins"] = json!({});
    empty["sets"] = json!({"none":{}});
    let empty = decode(&empty).unwrap();
    assert!(empty.sets()["none"].is_empty());
    assert!(empty.pins().is_empty());
    assert_eq!(empty.to_value().unwrap()["sets"]["none"], json!({}));
}
#[test]
fn repo_catalog_intent_scope_owner_kind_and_original_node_never_rebase_by_login() {
    for mutation in 0..6 {
        let mut value = local();
        match mutation {
            0 => value["scope"]["login"] = json!("user-id:43"),
            1 => value["scope"]["host"] = json!("elsewhere.invalid"),
            2 => value["scope"]["login"] = json!("octocat"),
            3 => value["source"]["owner_id"] = json!(72),
            4 => value["source"]["owner_node_id"] = json!("OWNER_other"),
            _ => value["source"]["kind"] = json!("PublicUser"),
        }
        assert!(
            decode(&value).is_err(),
            "Mutation{mutation} silently rebased original metadata"
        );
    }
    assert!(CatalogueIntentV1::from_value(
        &local(),
        &Scope::new("github.invalid", "user-id:042").unwrap(),
        &source()
    )
    .is_err());
}
#[test]
fn repo_catalog_intent_canonical_map_keys_and_positive_ids_have_no_node_number_fallback() {
    for key in ["0", "010000", "-1", "R_10000", "18446744073709551616"] {
        let mut value = local();
        value["sets"]["focused"] = json!({key:reference(10000,"example/repo0000",Some("10000"))});
        assert!(decode(&value).is_err());
    }
    let mut mismatch = local();
    mismatch["sets"]["focused"]["10000"]["id"] = json!(10001);
    assert!(decode(&mismatch).is_err());
    let mut actual_numeric_node = local();
    actual_numeric_node["sets"]["focused"]["10000"]["node_id"] = json!("10000");
    assert_eq!(
        decode(&actual_numeric_node).unwrap().sets()["focused"][&10000]
            .node_id
            .as_deref(),
        Some("10000")
    );
}
#[test]
fn repo_catalog_intent_actual_nodes_global_contradictions_but_numeric_owner_namespace_distinct() {
    let mut same_node = local();
    same_node["sets"]["focused"]["10000"]["node_id"] = json!("R_11000");
    assert!(decode(&same_node).is_err());
    let mut same_id = local();
    same_id["sets"]["focused"]["11000"]["node_id"] = json!("R_replacement");
    assert!(decode(&same_id).is_err());
    let mut owner_node = local();
    owner_node["sets"]["focused"]["10000"]["node_id"] = json!("OWNER_71");
    assert!(decode(&owner_node).is_err());
    let mut same_numeric = local();
    same_numeric["sets"]["focused"] =
        json!({"71":reference(71,"example/same-number",Some("R_71"))});
    assert_eq!(decode(&same_numeric).unwrap().sets()["focused"][&71].id, 71);
}
#[test]
fn repo_catalog_intent_rename_reference_case_exact_and_recreated_name_keeps_both_original_ids() {
    let mut value = local();
    value["source"]["owner"] = json!("EXAMPLE");
    value["sets"]["focused"] = json!({
        "10000":reference(10000,"EXAMPLE/Old-Case",Some("R_10000")),
        "10001":reference(10001,"EXAMPLE/Old-Case",Some("R_new_10001"))
    });
    let intent = decode(&value).unwrap();
    assert_eq!(
        intent.sets()["focused"].len(),
        2,
        "Same-slug recreation is not an ID alias/dedup"
    );
    assert_eq!(
        intent.sets()["focused"][&10000].last_name,
        "EXAMPLE/Old-Case"
    );
    assert_eq!(intent.to_value().unwrap(), value);
    let mut foreign = value;
    foreign["sets"]["focused"]["10000"]["last_name"] = json!("other/Old-Case");
    assert!(decode(&foreign).is_err());
}
#[test]
fn repo_catalog_intent_preclone_byte_set_and_field_caps_refuse_without_partial_defaults() {
    let mut huge = local();
    huge["unused"] = json!("界".repeat(INTENT_MAX_BYTES));
    assert!(decode(&huge).unwrap_err().contains("128KiB"));
    let mut fields = local();
    fields["sets"]["focused"]["10000"]["node_id"] = json!("x".repeat(1025));
    assert!(decode(&fields).is_err());
    fields = local();
    fields["sets"]["focused"]["10000"]["last_name"] = json!("example/line\nbreak");
    assert!(decode(&fields).is_err());
    let mut names = serde_json::Map::new();
    for index in 0..=MAX_SETS {
        names.insert(format!("set-{index}"), json!({}));
    }
    let mut many = local();
    many["sets"] = Value::Object(names);
    assert!(decode(&many).is_err());
    assert_eq!(decode(&local()).unwrap().sets()["focused"].len(), 2);
}
#[test]
fn repo_catalog_intent_real_sql_clear_derivatives_preserves_original_intent_draft_and_fences() {
    let mut cache = Cache::memory().unwrap();
    let original = scope();
    let intent = decode(&local()).unwrap().to_value().unwrap();
    cache
        .save_view(&original, "catalogue-intent-v1", &intent)
        .unwrap();
    cache
        .put_draft(&original, "draft", "Original42 draft")
        .unwrap();
    let generation = cache
        .reserve_sync_generation(&original, "catalogue-source-org71")
        .unwrap();
    let record = Record::new(
        "repository-catalogue-observation-v1",
        "10000",
        "example/repo0000",
        json!({"id":10000,"name":"example/repo0000"}),
    );
    assert!(cache
        .commit_reserved_sync(
            &original,
            "catalogue-source-org71",
            generation,
            std::slice::from_ref(&record),
            true
        )
        .unwrap());
    let inflight = cache
        .reserve_sync_generation(&original, "catalogue-source-org71")
        .unwrap();
    cache.clear_records(&original).unwrap();
    assert!(cache.list(&original, None).unwrap().is_empty());
    assert!(cache
        .sync_reservation(&original, "catalogue-source-org71")
        .unwrap()
        .is_none());
    assert!(!cache
        .commit_reserved_sync(
            &original,
            "catalogue-source-org71",
            inflight,
            &[record],
            true
        )
        .unwrap());
    let saved = cache
        .saved_view_bounded(&original, "catalogue-intent-v1", INTENT_MAX_BYTES)
        .unwrap()
        .unwrap();
    assert_eq!(decode(&saved).unwrap().to_value().unwrap(), intent);
    assert_eq!(
        cache.draft(&original, "draft").unwrap().as_deref(),
        Some("Original42 draft")
    );
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(cache
        .saved_view_bounded(&other, "catalogue-intent-v1", INTENT_MAX_BYTES)
        .unwrap()
        .is_none());
    assert!(cache.draft(&other, "draft").unwrap().is_none());
}
#[test]
fn repo_catalog_intent_real_sql_target_revoke_preserves_intent_but_scope_clear_deletes_it() {
    let mut cache = Cache::memory().unwrap();
    let original = scope();
    let intent = local();
    cache
        .save_view(&original, "catalogue-intent-v1", &intent)
        .unwrap();
    cache
        .put_draft(&original, "draft", "Original42 draft")
        .unwrap();
    cache
        .put(
            &original,
            &Record::new(
                "repository",
                "10000",
                "example/repo0000",
                json!({"id":10000}),
            ),
        )
        .unwrap();
    cache
        .put(
            &original,
            &Record::new(
                "repository",
                "11000",
                "example/repo1000",
                json!({"id":11000}),
            ),
        )
        .unwrap();
    assert_eq!(
        cache
            .revoke_repository(&original, "example/repo0000")
            .unwrap(),
        1
    );
    assert_eq!(cache.list(&original, None).unwrap().len(), 1);
    assert_eq!(
        cache
            .saved_view_bounded(&original, "catalogue-intent-v1", INTENT_MAX_BYTES)
            .unwrap()
            .unwrap(),
        intent
    );
    cache.clear_scope(&original, false).unwrap();
    assert!(
        cache
            .saved_view_bounded(&original, "catalogue-intent-v1", INTENT_MAX_BYTES)
            .unwrap()
            .is_none(),
        "Current false means preserve drafts ONLY, not saved-view intent"
    );
    assert_eq!(
        cache.draft(&original, "draft").unwrap().as_deref(),
        Some("Original42 draft")
    );
}
#[test]
fn repo_catalog_intent_closed_schema_never_restores_membership_permission_or_publisher_booleans() {
    for field in ["complete", "authorized", "can_open", "generation"] {
        let mut value = local();
        value[field] = json!(true);
        assert!(
            decode(&value).is_err(),
            "Cached{field} became an authority constructor"
        );
    }
    let mut unknown_node = local();
    unknown_node["pins"] = json!({});
    unknown_node["sets"]["focused"]["10000"]["node_id"] = Value::Null;
    assert!(decode(&unknown_node).unwrap().sets()["focused"][&10000]
        .node_id
        .is_none());
}
