//! Actual Catalogue/Definition/Core and SQL factories for restriction-only raw
//! cleanup predicates. These tests NEVER construct a cleanup seal, Branch or
//! publication permit from saved data. Full UID/Commit/Push tests are paired PTY.
use super::*;
use crate::{
    providers::gh::WorkflowJsonLedgerV9,
    storage::workflow_cleanup_v9::WorkflowRawRowsV9,
    storage::{Cache, RemoteMappingCandidateV1},
};
use rusqlite::{params, Connection, OpenFlags};
use sha2::{Digest, Sha256};
use std::time::{Duration, Instant};
type ActualHeapFixtureV9 = (
    Fixture,
    GithubProvider,
    App,
    Arc<WorkflowJsonLedgerV9>,
    CancellationToken,
    Vec<GhStep>,
);
fn actual_heap() -> ActualHeapFixtureV9 {
    let (fixture, provider, app, all) = setup("active", r::wrap(vec![], 42));
    let intent = local_intent(&app);
    let cancel = CancellationToken::default();
    let phase = provider
        .clone()
        .with_catalogue_read_deadline(Instant::now() + Duration::from_secs(30));
    let mut core = phase
        .begin_repository_catalogue_access(
            intent.source().original().source(),
            &crate::scoped_actions_publication_v3::access_limits(),
            &cancel,
        )
        .unwrap();
    core.verify(intent.source().reference(), &cancel).unwrap();
    let (window, post) = core
        .reserve_workflow_edit_transport_v9(
            intent.original(),
            crate::providers::gh::WorkflowEditPhaseV9::Bind,
            1,
            &cancel,
        )
        .unwrap();
    assert!(post.is_none());
    let measured = phase.clone().with_measured_read_window(window).unwrap();
    let heap = Arc::new(
        WorkflowJsonLedgerV9::test_factory_observation_v9(&measured, intent.original(), &cancel)
            .unwrap(),
    );
    core.verify(intent.source().reference(), &cancel).unwrap();
    core.close(&cancel).unwrap();
    f::exact(&fixture, &all);
    (fixture, provider, app, heap, cancel, all)
}
fn raw(fixture: &Fixture) -> Connection {
    Connection::open_with_flags(f::database(fixture), OpenFlags::SQLITE_OPEN_READ_WRITE).unwrap()
}
fn actual_mapping(fixture: &Fixture, app: &App) -> (RemoteMappingCandidateV1, String) {
    use crate::search::{
        corpus::LocalSource,
        managed::{CorpusOwnership, CorpusRepository},
    };
    use std::os::unix::ffi::OsStrExt;
    let source = local_intent(app).source().original().clone();
    let key = format!("corpus-mapping-v1:{:x}", Sha256::digest(f::REPO.as_bytes()));
    let mapping = CorpusRepository {
        scope: source.scope().clone(),
        repository: f::REPO.into(),
        directory: crate::git::RawPath(fixture.repo().as_os_str().as_bytes().to_vec()),
        source: LocalSource::Revision(f::git(fixture, &["rev-parse", "HEAD"]).trim().into()),
        ownership: CorpusOwnership::Existing,
        active: true,
        saved_commit: None,
        disk_bytes: 0,
        disk_complete: false,
        clone_source: None,
    };
    let cache = Cache::open(&f::database(fixture)).unwrap();
    cache
        .save_view(
            source.scope(),
            &key,
            &serde_json::to_value(mapping).unwrap(),
        )
        .unwrap();
    let actual = cache
        .remote_mapping_candidate_v1(source.scope(), &key)
        .unwrap()
        .unwrap();
    (actual, key)
}
#[derive(Clone, Copy)]
enum MappingChange {
    None,
    Whitespace,
    Blob,
    Null,
    Absent,
    ForeignOnly,
    ForeignAlso,
    Oversized,
}
fn mapping_case(change: MappingChange) {
    let (fixture, _provider, app, heap, _cancel, all) = actual_heap();
    let (mapping, key) = actual_mapping(&fixture, &app);
    let cache = Cache::open(&f::database(&fixture)).unwrap();
    let before_remaining = heap.remaining_v9().unwrap();
    let fence = WorkflowRawRowsV9::test_actual_mapping_v9(&cache, &mapping, &heap).unwrap();
    let scope = local_intent(&app).source().original().scope().clone();
    let db = raw(&fixture);
    let predicate = "host=?1 AND login=?2 AND name=?3";
    match change {
        MappingChange::None => {}
        MappingChange::Whitespace => {
            assert_eq!(
                db.execute(
                    &format!("UPDATE saved_views SET payload=payload||' ' WHERE {predicate}"),
                    params![scope.host, scope.login, key]
                )
                .unwrap(),
                1
            );
        }
        MappingChange::Blob => {
            assert_eq!(
                db.execute(
                    &format!(
                        "UPDATE saved_views SET payload=CAST(payload AS BLOB) WHERE {predicate}"
                    ),
                    params![scope.host, scope.login, key]
                )
                .unwrap(),
                1
            );
        }
        // NULL is prohibited by the actual schema; the genuine failed SQL must
        // retain the exact original row instead of pretending a null factory.
        MappingChange::Null => {
            assert!(db
                .execute(
                    &format!("UPDATE saved_views SET payload=NULL WHERE {predicate}"),
                    params![scope.host, scope.login, key]
                )
                .is_err());
        }
        MappingChange::Absent => {
            assert_eq!(
                db.execute(
                    &format!("DELETE FROM saved_views WHERE {predicate}"),
                    params![scope.host, scope.login, key]
                )
                .unwrap(),
                1
            );
        }
        MappingChange::ForeignOnly | MappingChange::ForeignAlso => {
            assert_eq!(db.execute("INSERT INTO saved_views(host,login,name,payload) SELECT host,'user-id:43',name,payload FROM saved_views WHERE host=?1 AND login=?2 AND name=?3",params![scope.host,scope.login,key]).unwrap(),1);
            if matches!(change, MappingChange::ForeignOnly) {
                assert_eq!(
                    db.execute(
                        &format!("DELETE FROM saved_views WHERE {predicate}"),
                        params![scope.host, scope.login, key]
                    )
                    .unwrap(),
                    1
                );
            }
        }
        MappingChange::Oversized => {
            assert_eq!(
                db.execute(
                    &format!("UPDATE saved_views SET payload=?4 WHERE {predicate}"),
                    params![scope.host, scope.login, key, "x".repeat(128 * 1024 + 1)]
                )
                .unwrap(),
                1
            );
        }
    }
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let valid = fence.test_validate_v9(&cache).is_ok();
    assert_eq!(
        valid,
        matches!(
            change,
            MappingChange::None | MappingChange::Null | MappingChange::ForeignAlso
        )
    );
    if matches!(change, MappingChange::Oversized) {
        assert!(WorkflowRawRowsV9::test_actual_mapping_v9(&cache, &mapping, &heap).is_err());
    }
    assert_eq!(
        (all_sql(&fixture), f::git_state(&fixture)),
        before,
        "Restriction-only predicates cannot repair or remove any deliberate preimage"
    );
    drop(fence);
    assert_eq!(
        heap.remaining_v9().unwrap(),
        before_remaining,
        "Every raw/key/row allocation drops before debit restoration"
    );
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_cleanup_raw_mapping_unchanged_is_exact_read_only() {
    mapping_case(MappingChange::None)
}
#[test]
fn task54_workflow_edit_v9_cleanup_raw_mapping_semantically_equal_json_drift_refuses() {
    mapping_case(MappingChange::Whitespace)
}
#[test]
fn task54_workflow_edit_v9_cleanup_raw_mapping_blob_cannot_equal_same_text_bytes() {
    mapping_case(MappingChange::Blob)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_null_constraint_preserves_exact_original() {
    mapping_case(MappingChange::Null)
}
#[test]
fn task54_workflow_edit_v9_cleanup_mapping_absence_never_becomes_ack() {
    mapping_case(MappingChange::Absent)
}
#[test]
fn task54_workflow_edit_v9_cleanup_foreign_identical_mapping_cannot_substitute_original() {
    mapping_case(MappingChange::ForeignOnly)
}
#[test]
fn task54_workflow_edit_v9_cleanup_unrelated_foreign_mapping_does_not_block_original() {
    mapping_case(MappingChange::ForeignAlso)
}
#[test]
fn task54_workflow_edit_v9_cleanup_mapping_oversize_refuses_before_copy_and_restores_debit() {
    mapping_case(MappingChange::Oversized)
}
#[derive(Clone, Copy)]
enum SourceChange {
    None,
    NewSavedIntent,
    Epoch,
    Clear,
}
fn source_case(change: SourceChange) {
    let (fixture, _provider, app, heap, _cancel, all) = actual_heap();
    let intent = local_intent(&app);
    let source = intent.source().original();
    let mut cache = Cache::open(&f::database(&fixture)).unwrap();
    let fence = WorkflowRawRowsV9::test_actual_source_v9(
        &cache,
        source,
        matches!(change, SourceChange::Epoch | SourceChange::Clear),
        &heap,
    )
    .unwrap();
    match change {
        SourceChange::None => {}
        SourceChange::NewSavedIntent => {
            cache
                .save_view(
                    source.scope(),
                    source.saved_view_key(),
                    &serde_json::json!({"deliberate_newer":"not_authority"}),
                )
                .unwrap();
        }
        SourceChange::Epoch => {
            assert_eq!(raw(&fixture).execute("UPDATE sync_reservations SET generation=generation+1 WHERE host=?1 AND login=?2 AND source=?3",params![source.scope().host,source.scope().login,source.sync_source()]).unwrap(),1);
        }
        SourceChange::Clear => cache.clear_scope(source.scope(), false).unwrap(),
    }
    let before = (all_sql(&fixture), f::git_state(&fixture));
    assert_eq!(
        fence.test_validate_v9(&cache).is_ok(),
        matches!(change, SourceChange::None)
    );
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_original_source_snapshot_is_not_new_authority() {
    source_case(SourceChange::None)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_original_source_refuses_mirrored_foreign_database_before_snapshot(
) {
    let (fixture, _provider, app, heap, _cancel, all) = actual_heap();
    let intent = local_intent(&app);
    let source = intent.source().original();
    let original = f::database(&fixture);
    let foreign = original.with_file_name("foreign-workflow-cleanup.sqlite3");
    // An independently real Cache with byte-identical original saved rows is
    // still not the actual source's issuer. No constructed source/permission.
    std::fs::copy(&original, &foreign).unwrap();
    let original_bytes = std::fs::read(&original).unwrap();
    let cache = Cache::open(&foreign).unwrap();
    let foreign_bytes = std::fs::read(&foreign).unwrap();
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let remaining = heap.remaining_v9().unwrap();
    for epoch in [false, true] {
        assert!(WorkflowRawRowsV9::test_actual_source_v9(&cache, source, epoch, &heap).is_err());
        assert_eq!(heap.remaining_v9().unwrap(), remaining);
        assert_eq!(std::fs::read(&original).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&foreign).unwrap(), foreign_bytes);
        assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    }
    let actual = Cache::open(&original).unwrap();
    assert!(WorkflowRawRowsV9::test_actual_source_v9(&actual, source, false, &heap).is_ok());
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_cleanup_new_saved_intent_cannot_rebase_original_snapshot() {
    source_case(SourceChange::NewSavedIntent)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_generation_drift_preserves_newer_source() {
    source_case(SourceChange::Epoch)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_clear_false_does_not_reconstruct_original() {
    source_case(SourceChange::Clear)
}
#[test]
fn task54_workflow_edit_v9_cleanup_occupied_same_ledger_refuses_before_mapping_copy_and_restores_every_byte(
) {
    let (fixture, _provider, app, heap, _cancel, all) = actual_heap();
    let (mapping, _) = actual_mapping(&fixture, &app);
    let cache = Cache::open(&f::database(&fixture)).unwrap();
    let remaining = heap.remaining_v9().unwrap();
    let occupied = heap.reserve_model_v9(remaining - 1024).unwrap();
    let before = (all_sql(&fixture), f::git_state(&fixture));
    assert!(WorkflowRawRowsV9::test_actual_mapping_v9(&cache, &mapping, &heap).is_err());
    assert_eq!(heap.remaining_v9().unwrap(), 1024);
    assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    drop(occupied);
    assert_eq!(heap.remaining_v9().unwrap(), remaining);
    f::exact(&fixture, &all);
}
fn publication_profile_case(large: bool, workflow: bool, replaced: bool) {
    let (fixture, _provider, app, _heap, cancel, all) = actual_heap();
    let intent = local_intent(&app);
    let writer = crate::drafts::DraftWriter::new(f::database(&fixture));
    let client = writer.client();
    let deadline = Instant::now() + Duration::from_secs(5);
    let fence = if workflow {
        client.capture_workflow_edit_publication_v9(
            &f::database(&fixture),
            &intent,
            &cancel,
            deadline,
        )
    } else {
        client.capture_publication_v2(
            &f::database(&fixture),
            intent.source().original().scope(),
            intent.key(),
            &cancel,
            deadline,
        )
    }
    .unwrap();
    let body = "PRIVATE_EXACT_DRAFT_BODY\n".repeat(if large { 6000 } else { 4 });
    assert_eq!(body.len() > 128 * 1024, large);
    let before = (all_sql(&fixture), f::git_state(&fixture));
    let ack = client.save_publication_and_wait_v2(fence, body.clone(), &cancel);
    if large && !workflow {
        assert!(ack.is_err());
        assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
    } else {
        let ack = ack.unwrap();
        if replaced {
            Cache::open(&f::database(&fixture))
                .unwrap()
                .put_draft(
                    intent.source().original().scope(),
                    intent.key(),
                    "PRIVATE_NEWER_DRAFT_MUST_REMAIN",
                )
                .unwrap();
        }
        let pre_remove = (all_sql(&fixture), f::git_state(&fixture));
        let removed = client.remove_publication_and_wait_v2(ack, &cancel);
        assert_eq!(removed.is_ok(), !replaced);
        if replaced {
            assert_eq!((all_sql(&fixture), f::git_state(&fixture)), pre_remove);
        } else {
            assert_eq!((all_sql(&fixture), f::git_state(&fixture)), before);
        }
    }
    drop(client);
    drop(writer);
    f::exact(&fixture, &all);
}
#[test]
fn task54_workflow_edit_v9_cleanup_legacy_none_key_spelling_cannot_raise_standard128() {
    publication_profile_case(true, false, false)
}
#[test]
fn task54_workflow_edit_v9_cleanup_actual_workflow361_profile_keeps_large_exact_queue_remove() {
    publication_profile_case(true, true, false)
}
#[test]
fn task54_workflow_edit_v9_cleanup_legacy_none_exact_body_replacement_is_preserved() {
    publication_profile_case(false, false, true)
}
