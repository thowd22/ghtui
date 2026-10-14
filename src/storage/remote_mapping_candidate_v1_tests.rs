//! UNRUN bounded candidate/CAS contracts. Never association authority.
use super::*;
use crate::{
    git::RawPath,
    search::{
        corpus::LocalSource,
        managed::{CorpusOwnership, CorpusRepository},
    },
    test_support::Fixture,
};
use serde_json::json;
fn mapping(scope: &Scope, path: &std::path::Path) -> CorpusRepository {
    use std::os::unix::ffi::OsStrExt;
    CorpusRepository {
        scope: scope.clone(),
        repository: "example/remote".into(),
        directory: RawPath(path.as_os_str().as_bytes().to_vec()),
        source: LocalSource::Revision("a".repeat(40)),
        ownership: CorpusOwnership::Existing,
        active: true,
        saved_commit: Some("a".repeat(40)),
        disk_bytes: 0,
        disk_complete: false,
        clone_source: None,
    }
}
#[test]
fn remote_tree_v1_mapping_scope_uid_and_exact_payload_change_do_not_adopt_latest() {
    let fixture = Fixture::new();
    let database = fixture.root.join("cache.sqlite3");
    let cache = Cache::open(&database).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let key = crate::search::managed::mapping_key("example/remote");
    let raw = serde_json::to_value(mapping(&scope, &fixture.repo())).unwrap();
    cache.save_view(&scope, &key, &raw).unwrap();
    let original = cache
        .remote_mapping_candidate_v1(&scope, &key)
        .unwrap()
        .unwrap();
    assert_eq!(
        original.mapping().directory,
        mapping(&scope, &fixture.repo()).directory
    );
    cache
        .validate_remote_mapping_candidate_v1(&original)
        .unwrap();
    let other = Scope::github("github.invalid", Some(43)).unwrap();
    assert!(cache
        .remote_mapping_candidate_v1(&other, &key)
        .unwrap()
        .is_none());
    cache
        .save_view(&scope, &key, &json!({"new":"PRIVATE_MAPPING_CANARY"}))
        .unwrap();
    assert!(cache
        .validate_remote_mapping_candidate_v1(&original)
        .is_err());
    assert!(cache.remote_mapping_candidate_v1(&scope, &key).is_err());
}
#[test]
fn remote_tree_v1_mapping_utf8_preflight_and_unknown_principal_never_parse_or_copy() {
    let fixture = Fixture::new();
    let cache = Cache::open(&fixture.root.join("cache.sqlite3")).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let key = crate::search::managed::mapping_key("example/remote");
    cache
        .save_view(&scope, &key, &json!({"oversized":"界".repeat(45000)}))
        .unwrap();
    assert!(cache.remote_mapping_candidate_v1(&scope, &key).is_err());
    let unknown = Scope::new("github.invalid", "octocat").unwrap();
    assert!(cache.remote_mapping_candidate_v1(&unknown, &key).is_err());
    assert!(cache
        .remote_mapping_candidate_v1(&scope, "unrelated-view")
        .is_err());
}
#[test]
fn remote_tree_v1_mapping_foreign_database_issuer_and_replacement_are_refused() {
    let first = Fixture::new();
    let second = Fixture::new();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let key = crate::search::managed::mapping_key("example/remote");
    let cache = Cache::open(&first.root.join("cache.sqlite3")).unwrap();
    let raw = serde_json::to_value(mapping(&scope, &first.repo())).unwrap();
    cache.save_view(&scope, &key, &raw).unwrap();
    let original = cache
        .remote_mapping_candidate_v1(&scope, &key)
        .unwrap()
        .unwrap();
    let foreign = Cache::open(&second.root.join("cache.sqlite3")).unwrap();
    foreign.save_view(&scope, &key, &raw).unwrap();
    assert!(foreign
        .validate_remote_mapping_candidate_v1(&original)
        .is_err());
    cache
        .validate_remote_mapping_candidate_v1(&original)
        .unwrap();
}
