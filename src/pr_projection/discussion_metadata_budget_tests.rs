use super::*;

const BYTES_SQL: &str = "length(CAST(repository AS BLOB))+length(CAST(command AS BLOB))+length(CAST(selected_resource AS BLOB))+length(CAST(observed_head AS BLOB))+length(CAST(payload_sha256 AS BLOB))+length(CAST(COALESCE(confirmed_json,'') AS BLOB))";

fn key(target: &DiscussionTarget) -> PrWriteTarget {
    PrWriteTarget::for_source(
        &target.scope,
        &target.repository,
        &target.parent_node,
        target.number,
    )
    .unwrap()
}

#[test]
fn collaboration_index_source_metadata_confirmed_utf8_budget_precedes_receipt_parse() {
    let (_dir, database) = fixture();
    let selected = target(DiscussionKind::ConversationComment);
    let (cache, generation) = cached(&database, &selected);
    let publication = confirmed(&database, &selected);
    let sql = Connection::open(&database).unwrap();
    let json: String = sql
        .query_row(
            "SELECT confirmed_json FROM pr_write_attempts WHERE attempt_id=?1",
            [publication.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut json: serde_json::Value = serde_json::from_str(&json).unwrap();
    json["metadata_padding"] = json!("🦀".repeat(2400));
    sql.execute(
        "UPDATE pr_write_attempts SET confirmed_json=?1 WHERE attempt_id=?2",
        rusqlite::params![
            serde_json::to_string(&json).unwrap(),
            publication.attempt_id
        ],
    )
    .unwrap();
    let (characters, bytes): (u64, u64) = sql
        .query_row(
            &format!("SELECT length(repository)+length(command)+length(selected_resource)+length(observed_head)+length(payload_sha256)+length(confirmed_json),{BYTES_SQL} FROM pr_write_attempts WHERE attempt_id=?1"),
            [publication.attempt_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(characters < 8192 && bytes > 8192);
    let error = guard_discussion_reads(&database, &selected.scope, std::slice::from_ref(&selected))
        .unwrap_err();
    assert!(error.contains("8 KiB"), "{error}");
    assert_eq!(
        cache
            .binding_status(&selected.scope, &selected.source())
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(generation)
    );
    assert_eq!(
        sql.query_row("SELECT state FROM pr_write_attempts", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "confirmed"
    );
    assert_eq!(
        sql.query_row("SELECT COUNT(*) FROM sync_mutation_fences", [], |row| row
            .get::<_, u64>(
            0
        ))
        .unwrap(),
        0
    );
}

#[test]
fn collaboration_index_source_metadata_pending_oversize_is_not_parsed_or_cross_scope_loaded() {
    let (_dir, database) = fixture();
    let selected = target(DiscussionKind::ConversationComment);
    let (cache, generation) = cached(&database, &selected);
    let mut journal = PrWriteJournal::open(&database).unwrap();
    let permit = journal.begin(&key(&selected), &journal_intent()).unwrap();
    let sql = Connection::open(&database).unwrap();
    for oversized in ["broken-json{".repeat(1000), "🦀".repeat(2400)] {
        sql.execute(
            "UPDATE pr_write_attempts SET confirmed_json=?1 WHERE attempt_id=?2",
            rusqlite::params![oversized, permit.marker().attempt_id],
        )
        .unwrap();
        let error =
            guard_discussion_reads(&database, &selected.scope, std::slice::from_ref(&selected))
                .unwrap_err();
        assert!(error.contains("8 KiB"), "preflight before JSON: {error}");
        let mut other = selected.clone();
        other.scope = Scope::github("github.invalid", Some(43)).unwrap();
        assert!(
            guard_discussion_reads(&database, &other.scope, std::slice::from_ref(&other))
                .unwrap()
                .unavailable_sources
                .is_empty()
        );
        other.scope = selected.scope.clone();
        other.parent_node = "AnotherActualNode".into();
        assert!(
            guard_discussion_reads(&database, &other.scope, std::slice::from_ref(&other))
                .unwrap()
                .unavailable_sources
                .is_empty()
        );
    }
    assert_eq!(
        cache
            .binding_status(&selected.scope, &selected.source())
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(generation)
    );
    assert_eq!(
        sql.query_row("SELECT state FROM pr_write_attempts", [], |row| row
            .get::<_, String>(0))
            .unwrap(),
        "pending"
    );
    assert_eq!(
        sql.query_row("SELECT COUNT(*) FROM sync_mutation_fences", [], |row| row
            .get::<_, u64>(
            0
        ))
        .unwrap(),
        0
    );
}

#[test]
fn collaboration_index_source_metadata_byte_boundary_preserves_exact_receipt_and_refuses_next_byte()
{
    let (_dir, database) = fixture();
    let selected = target(DiscussionKind::ConversationComment);
    let publication = confirmed(&database, &selected);
    let journal = PrWriteJournal::open(&database).unwrap();
    let sql = Connection::open(&database).unwrap();
    let receipt: String = sql
        .query_row(
            "SELECT confirmed_json FROM pr_write_attempts WHERE attempt_id=?1",
            [publication.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    let mut receipt: serde_json::Value = serde_json::from_str(&receipt).unwrap();
    receipt["metadata_padding"] = json!("");
    sql.execute(
        "UPDATE pr_write_attempts SET confirmed_json=?1 WHERE attempt_id=?2",
        rusqlite::params![
            serde_json::to_string(&receipt).unwrap(),
            publication.attempt_id
        ],
    )
    .unwrap();
    let baseline: usize = sql
        .query_row(
            &format!("SELECT {BYTES_SQL} FROM pr_write_attempts WHERE attempt_id=?1"),
            [publication.attempt_id],
            |row| row.get(0),
        )
        .unwrap();
    assert!(baseline < 8192);
    receipt["metadata_padding"] = json!("x".repeat(8192 - baseline));
    sql.execute(
        "UPDATE pr_write_attempts SET confirmed_json=?1 WHERE attempt_id=?2",
        rusqlite::params![
            serde_json::to_string(&receipt).unwrap(),
            publication.attempt_id
        ],
    )
    .unwrap();
    assert_eq!(
        sql.query_row(
            &format!("SELECT {BYTES_SQL} FROM pr_write_attempts WHERE attempt_id=?1"),
            [publication.attempt_id],
            |row| row.get::<_, usize>(0)
        )
        .unwrap(),
        8192
    );
    let marker = journal
        .latest_source_receipt(&key(&selected), SyncBindingKind::ConversationComment)
        .unwrap()
        .unwrap();
    assert_eq!(marker.attempt_id, publication.attempt_id);
    assert_eq!(marker.state, PrWriteState::Confirmed);
    assert_eq!(marker.confirmed.unwrap().resource_id, "10");
    receipt["metadata_padding"] = json!("x".repeat(8193 - baseline));
    sql.execute(
        "UPDATE pr_write_attempts SET confirmed_json=?1 WHERE attempt_id=?2",
        rusqlite::params![
            serde_json::to_string(&receipt).unwrap(),
            publication.attempt_id
        ],
    )
    .unwrap();
    assert!(journal
        .latest_source_receipt(&key(&selected), SyncBindingKind::ConversationComment)
        .unwrap_err()
        .to_string()
        .contains("8 KiB"));
    assert!(journal
        .latest_source_receipt(&key(&selected), SyncBindingKind::IssueBody)
        .unwrap()
        .is_none());
}
