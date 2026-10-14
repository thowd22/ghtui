//! Staged real-SQL original-receipt/metadata attachment contracts, not PTY evidence.
#[path = "case_spelling_tests.rs"]
mod case_spelling_tests;
use super::*;
use crate::pr_write_journal::{
    PrConfirmedEffect, PrEffectKind, PrPublicationReference, PrWriteIntent, PrWriteJournal,
    PrWriteTarget,
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::{fs, os::unix::fs::PermissionsExt};

fn persistent() -> (tempfile::TempDir, PathBuf, Cache) {
    let dir = tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap();
    let database = dir.path().join("cache.sqlite3");
    let cache = Cache::open(&database).unwrap();
    (dir, database, cache)
}
fn attach(database: &Path, cache: &Cache, parents: &[DiscussionTarget]) -> CurrentVisibility {
    let context = context();
    let mut current = current(cache, &[], &context);
    current
        .attach_discovered(
            database,
            cache,
            &context,
            parents,
            &DiscussionLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap();
    current
}
fn attempt(database: &Path, parent: &DiscussionTarget, confirmed: bool) -> PrPublicationReference {
    let target = PrWriteTarget::for_source(
        &parent.scope,
        &parent.repository,
        &parent.parent_node,
        parent.number,
    )
    .unwrap();
    let intent = PrWriteIntent {
        command: "legacy-description".into(),
        selected_resource: "pr".into(),
        observed_head: "a".repeat(40),
        values: BTreeMap::from([("body".into(), "original private publication".into())]),
    };
    let mut journal = PrWriteJournal::open(database).unwrap();
    let permit = journal.begin(&target, &intent).unwrap();
    journal.claim(&permit, &target, &intent).unwrap();
    if confirmed {
        journal
            .mark_confirmed(
                &permit,
                &PrConfirmedEffect {
                    kind: PrEffectKind::Metadata,
                    resource_id: "updated".into(),
                    node_id: None,
                },
            )
            .unwrap();
    } else {
        journal.mark_uncertain(&permit).unwrap();
    }
    permit.marker().publication_reference()
}
fn remote() -> RemoteCandidate {
    remote_parent("old-index-fragment", node_identity("PR_parent"))
}
fn result(cache: &Cache, visibility: &CurrentVisibility) -> BlendPage {
    let context = context();
    run(
        cache,
        &context,
        &local_page(&context, vec![]),
        &[remote_list(&context, vec![remote()])],
        visibility,
    )
}
fn damage_documents(database: &Path) {
    Connection::open(database)
        .unwrap()
        .execute(
            "UPDATE records SET payload='MALFORMED_DOCUMENT_BODY_CANARY' WHERE kind='collaboration-document-v1'",
            [],
        )
        .unwrap();
}
fn documents_still_damaged(database: &Path) {
    let sql = Connection::open(database).unwrap();
    let count: u64 = sql
        .query_row(
            "SELECT COUNT(*) FROM records WHERE kind='collaboration-document-v1' AND payload='MALFORMED_DOCUMENT_BODY_CANARY'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(count > 0);
}

#[test]
fn collaboration_index_blend_discovered_unselected_current_body_overlays_old_remote_fragment() {
    let (_dir, database, mut cache) = persistent();
    let body = snapshot(
        DiscussionKind::PullRequestBody,
        "needle current unselected parent",
    );
    publish(&mut cache, &body);
    let current = attach(&database, &cache, std::slice::from_ref(&body.target));
    let page = result(&cache, &current);
    assert_eq!(page.page.hits.len(), 1);
    let hit = &page.page.hits[0];
    assert_eq!(hit.document.body, "needle current unselected parent");
    assert!(!hit.snippet.contains("stale remote body"));
    assert!(matches!(
        page.origins.get(&hit.document.key.stable_id()),
        Some(CandidateOrigin::CurrentCanonical(_))
    ));
    assert!(hit
        .document
        .metadata
        .actors
        .iter()
        .any(|actor| actor.login == "alice"));
    assert_eq!(page.reports[0].admitted, 1);
    assert_eq!(
        current.validate_document(&cache, &hit.document).unwrap(),
        Some(body.target.clone())
    );
    let mut forged = (*hit.document).clone();
    forged.body.push_str(" altered payload");
    assert!(current.validate_document(&cache, &forged).is_err());
    let comment = snapshot(
        DiscussionKind::ConversationComment,
        "needle separate comment",
    );
    publish(&mut cache, &comment);
    let fresh = attach(&database, &cache, std::slice::from_ref(&body.target));
    let comments = cached(&cache, std::slice::from_ref(&comment.target));
    assert!(
        fresh
            .validate_document(&cache, &comments[0])
            .unwrap()
            .is_none(),
        "body proof cannot authorize comments"
    );
    assert!(
        current.validate(&cache).is_err(),
        "new matching source metadata requires a fresh attachment"
    );
}

#[test]
fn collaboration_index_blend_discovered_removed_source_blocks_without_any_document_read() {
    let (_dir, database, mut cache) = persistent();
    let body = snapshot(
        DiscussionKind::PullRequestBody,
        "needle removed private body",
    );
    publish(&mut cache, &body);
    collaboration::invalidate(&mut cache, &body.target).unwrap();
    damage_documents(&database);
    let current = attach(&database, &cache, std::slice::from_ref(&body.target));
    let page = result(&cache, &current);
    assert!(page.page.hits.is_empty());
    assert_eq!(page.reports[0].suppressed, 1);
    assert!(!page.page.coverage.complete);
    documents_still_damaged(&database);
}

#[test]
fn collaboration_index_blend_discovered_first_source_failed_sql_receipt_hides_until_new_authority()
{
    let (_dir, database, mut cache) = persistent();
    let body = snapshot(DiscussionKind::PullRequestBody, "needle new genuine body");
    let receipt = attempt(&database, &body.target, true);
    let sql = Connection::open(&database).unwrap();
    sql.execute_batch("CREATE TRIGGER fail_first_tombstone BEFORE INSERT ON sync_mutation_fences BEGIN SELECT RAISE(ABORT,'fixture first-source SQL failure'); END;").unwrap();
    let context = context();
    let mut current = current(&cache, &[], &context);
    let error = current
        .attach_discovered(
            &database,
            &cache,
            &context,
            std::slice::from_ref(&body.target),
            &DiscussionLimits::default(),
            &CancellationToken::default(),
        )
        .unwrap_err();
    assert!(
        error.0.contains("private metadata remains hidden"),
        "{error:?}"
    );
    assert_eq!(
        sql.query_row("SELECT COUNT(*) FROM sync_resource_bindings", [], |row| row
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        sql.query_row(
            "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
            [receipt.attempt_id],
            |row| row.get::<_, String>(0)
        )
        .unwrap(),
        "confirmed"
    );
    sql.execute_batch("DROP TRIGGER fail_first_tombstone;")
        .unwrap();
    let fenced = attach(&database, &cache, std::slice::from_ref(&body.target));
    assert!(result(&cache, &fenced).page.hits.is_empty());
    assert_eq!(
        sql.query_row(
            "SELECT mutation_ordinal FROM sync_mutation_fences",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        receipt.attempt_id
    );
    publish(&mut cache, &body);
    let fresh = attach(&database, &cache, std::slice::from_ref(&body.target));
    assert_eq!(
        result(&cache, &fresh).page.hits[0].document.body,
        "needle new genuine body"
    );
    assert_eq!(
        sql.query_row("SELECT COUNT(*) FROM sync_mutation_fences", [], |row| row
            .get::<_, u64>(
            0
        ))
        .unwrap(),
        1,
        "receipt replay protection is retained, not permanent denial"
    );
}

#[test]
fn collaboration_index_blend_discovered_unknown_journal_change_invalidates_old_proof_and_never_resends(
) {
    let (_dir, database, mut cache) = persistent();
    let body = snapshot(DiscussionKind::PullRequestBody, "needle prior body");
    publish(&mut cache, &body);
    let prior = attach(&database, &cache, std::slice::from_ref(&body.target));
    let reference = attempt(&database, &body.target, false);
    assert!(
        prior.validate(&cache).is_err(),
        "journal generation catches unknown invocation before cache fence exists"
    );
    let context = context();
    assert!(blend(
        &cache,
        &context,
        &local_page(&context, vec![]),
        &[remote_list(&context, vec![remote()])],
        &prior,
        &CancellationToken::default()
    )
    .is_err());
    damage_documents(&database);
    let gated = attach(&database, &cache, std::slice::from_ref(&body.target));
    assert!(result(&cache, &gated).page.hits.is_empty());
    documents_still_damaged(&database);
    let sql = Connection::open(&database).unwrap();
    assert_eq!(
        sql.query_row(
            "SELECT COUNT(*),state,confirmed_json FROM pr_write_attempts WHERE attempt_id=?1",
            [reference.attempt_id],
            |row| Ok((
                row.get::<_, u64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?
            ))
        )
        .unwrap(),
        (1, "uncertain".into(), None)
    );
    assert_eq!(
        sql.query_row("SELECT COUNT(*) FROM sync_mutation_fences", [], |row| row
            .get::<_, u64>(
            0
        ))
        .unwrap(),
        0,
        "unknown is not confirmed receipt authority"
    );
}

#[test]
fn collaboration_index_blend_discovered_scope_repository_number_type_and_issuer_contradictions_refuse(
) {
    let (_dir, database, mut cache) = persistent();
    let body = snapshot(DiscussionKind::PullRequestBody, "needle old canonical body");
    publish(&mut cache, &body);
    damage_documents(&database);
    for mode in [
        "scope",
        "repository_id",
        "number",
        "category",
        "parent_type",
    ] {
        let mut target = body.target.clone();
        match mode {
            "scope" => target.scope = Scope::github("github.invalid", Some(43)).unwrap(),
            "repository_id" => target.repository_id = 102,
            "number" => target.number = 8,
            "category" => target.category = DiscussionKind::Review,
            "parent_type" => {
                target.parent_kind = ParentKind::Issue;
                target.category = DiscussionKind::IssueBody;
            }
            _ => unreachable!(),
        }
        let context = context();
        let mut visibility = current(&cache, &[], &context);
        let error = visibility
            .attach_discovered(
                &database,
                &cache,
                &context,
                &[target],
                &DiscussionLimits::default(),
                &CancellationToken::default(),
            )
            .unwrap_err();
        assert!(
            !error.0.contains("expected value"),
            "identity proof precedes document parse: {mode}: {error:?}"
        );
    }
    let (_other_dir, other_database, _other_cache) = persistent();
    let context = context();
    let mut visibility = current(&cache, &[], &context);
    assert!(visibility
        .attach_discovered(
            &other_database,
            &cache,
            &context,
            &[],
            &DiscussionLimits::default(),
            &CancellationToken::default()
        )
        .unwrap_err()
        .0
        .contains("different issuers"));
    documents_still_damaged(&database);
}

#[test]
fn collaboration_index_blend_discovered_partial_cap_and_outside_page_changes_are_not_absence() {
    let (_dir, database, cache) = persistent();
    let body = snapshot(DiscussionKind::PullRequestBody, "needle parent");
    let mut sql = Connection::open(&database).unwrap();
    let transaction = sql.transaction().unwrap();
    {
        let mut statement=transaction.prepare("INSERT INTO sync_resource_bindings(host,login,source,namespace,repository_id,repository,resource,kind,reservation_generation,visible_generation,active) VALUES('github.invalid','user-id:42',?1,'collaboration',?2,'org/app','PR_parent','pull_request_body',1,NULL,0)").unwrap();
        for index in 1..=5001 {
            statement
                .execute(params![format!("collaboration-v1:{index:064x}"), index])
                .unwrap();
        }
    }
    transaction.commit().unwrap();
    let visibility = attach(&database, &cache, std::slice::from_ref(&body.target));
    assert!(result(&cache, &visibility).page.hits.is_empty());
    visibility.validate(&cache).unwrap();
    sql.execute(
        "UPDATE sync_resource_bindings SET active=1 WHERE source=?1",
        [format!("collaboration-v1:{:064x}", 5001)],
    )
    .unwrap();
    assert!(
        visibility.validate(&cache).is_err(),
        "same-count outside-page metadata invalidates retained lookup proof"
    );
}

#[test]
fn collaboration_index_blend_discovered_missing_oversized_partial_suppressed_unavailable_metadata_never_reads_bodies(
) {
    let (_dir, database, mut cache) = persistent();
    let body = snapshot(
        DiscussionKind::PullRequestBody,
        "needle retained private body",
    );
    for mode in [
        "missing",
        "oversized",
        "partial",
        "suppressed",
        "unavailable",
        "revoked",
    ] {
        publish(&mut cache, &body);
        let sql = Connection::open(&database).unwrap();
        let state: String = sql
            .query_row(
                "SELECT payload FROM records WHERE kind='collaboration-state-v1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let mut state: Value = serde_json::from_str(&state).unwrap();
        match mode {
            "missing" => {
                sql.execute(
                    "DELETE FROM records WHERE kind='collaboration-state-v1'",
                    [],
                )
                .unwrap();
            }
            "oversized" => state["metadata_padding"] = json!("🦀".repeat(33000)),
            "partial" => state["enumeration_complete"] = json!(false),
            "suppressed" => state["suppressed_threads"] = json!(["ACTUAL_THREAD"]),
            "unavailable" => state["access"] = json!({"Unavailable":"budget"}),
            "revoked" => state["access"] = json!({"Revoked":"access"}),
            _ => unreachable!(),
        }
        if mode != "missing" {
            sql.execute(
                "UPDATE records SET payload=?1 WHERE kind='collaboration-state-v1'",
                [serde_json::to_string(&state).unwrap()],
            )
            .unwrap();
        }
        damage_documents(&database);
        let visibility = attach(&database, &cache, std::slice::from_ref(&body.target));
        assert!(result(&cache, &visibility).page.hits.is_empty(), "{mode}");
        documents_still_damaged(&database);
    }
}

#[test]
fn collaboration_index_blend_discovered_empty_attachment_suppresses_numeric_missing_and_unverified_nodes(
) {
    let (_dir, database, cache) = persistent();
    let visibility = attach(&database, &cache, &[]);
    let context = context();
    let candidates = vec![
        remote_parent(
            "numeric",
            RemoteIdentity::Numeric {
                database_id: 700,
                repository_id: Some(101),
            },
        ),
        remote_parent("missing", RemoteIdentity::Unavailable),
        remote(),
    ];
    let page = run(
        &cache,
        &context,
        &local_page(&context, vec![]),
        &[remote_list(&context, candidates)],
        &visibility,
    );
    assert!(page.page.hits.is_empty());
    assert_eq!(page.reports[0].suppressed, 3);
    assert!(!page.page.coverage.complete);
}
