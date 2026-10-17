//! Actual Text publication/inspection must never acquire Members lineage.
use super::*;

#[derive(Clone, Copy)]
enum TextOriginFaultV61 {
    BaseForeign,
    ActionForeign,
    ParentOne,
    BaseBlob,
    ActionBlob,
    ParentBlob,
}
impl TextOriginFaultV61 {
    fn assignment(self) -> &'static str {
        match self {
            Self::BaseForeign => "base_key_v48='metadata-members-v48/foreign'",
            Self::ActionForeign => "action_tag_v48='metadata-add-label-v48'",
            Self::ParentOne => "parent_ack_attempt_v48=1",
            Self::BaseBlob => "base_key_v48=X'00'",
            Self::ActionBlob => "action_tag_v48=X'00'",
            Self::ParentBlob => "parent_ack_attempt_v48=CAST('1' AS BLOB)",
        }
    }
    fn column(self) -> &'static str {
        match self {
            Self::BaseForeign | Self::BaseBlob => "base_key_v48",
            Self::ActionForeign | Self::ActionBlob => "action_tag_v48",
            Self::ParentOne | Self::ParentBlob => "parent_ack_attempt_v48",
        }
    }
    fn expected(self) -> rusqlite::types::Value {
        use rusqlite::types::Value as Sql;
        match self {
            Self::BaseForeign => Sql::Text("metadata-members-v48/foreign".into()),
            Self::ActionForeign => Sql::Text("metadata-add-label-v48".into()),
            Self::ParentOne => Sql::Integer(1),
            Self::BaseBlob | Self::ActionBlob => Sql::Blob(vec![0]),
            Self::ParentBlob => Sql::Blob(b"1".to_vec()),
        }
    }
}
type FullRowsV61 =
    std::collections::BTreeMap<String, (Vec<String>, Vec<Vec<rusqlite::types::Value>>)>;
fn full_rows_v61(db: &rusqlite::Connection) -> FullRowsV61 {
    let tables = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    tables
        .into_iter()
        .map(|table| {
            let mut statement = db
                .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
                .unwrap();
            let columns = statement
                .column_names()
                .iter()
                .map(|v| (*v).into())
                .collect::<Vec<_>>();
            let mut rows = statement
                .query_map([], |row| {
                    (0..columns.len())
                        .map(|i| row.get(i))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| format!("{row:?}"));
            (table, (columns, rows))
        })
        .collect()
}
fn schema_v61(db: &rusqlite::Connection) -> Vec<Vec<rusqlite::types::Value>> {
    db.prepare("SELECT type,name,tbl_name,rootpage,sql FROM sqlite_master ORDER BY type,name")
        .unwrap()
        .query_map([], |row| {
            (0..5)
                .map(|i| row.get(i))
                .collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
}
fn hostile_origin_v61(db: &mut rusqlite::Connection, attempt: i64, fault: TextOriginFaultV61) {
    use rusqlite::types::Value as Sql;
    let before = full_rows_v61(db);
    let schema = schema_v61(db);
    let update = format!(
        "UPDATE pr_write_metadata_origins_v25 SET {} WHERE attempt_id=?1",
        fault.assignment()
    );
    let rejected = db.execute(&update, [attempt]).unwrap_err();
    assert_eq!(
        rejected.sqlite_error_code(),
        Some(rusqlite::ErrorCode::ConstraintViolation)
    );
    assert!(
        matches!(&rejected, rusqlite::Error::SqliteFailure(error, Some(message))
        if error.extended_code == 1811 && message == "Immutable original metadata")
    );
    assert_eq!(schema_v61(db), schema);
    assert_eq!(full_rows_v61(db), before);
    let mut expected = before.clone();
    let (columns, rows) = expected.get_mut("pr_write_metadata_origins_v25").unwrap();
    let id = columns.iter().position(|v| v == "attempt_id").unwrap();
    let column = columns.iter().position(|v| v == fault.column()).unwrap();
    let mut changed = 0;
    for row in rows.iter_mut() {
        if row[id] == Sql::Integer(attempt) {
            assert_eq!(row[column], Sql::Null);
            row[column] = fault.expected();
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    rows.sort_by_key(|row| format!("{row:?}"));
    // The exact original trigger is restored before commit or provider I/O;
    // this deliberately hostile disposable database never grants authority.
    let tx = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    assert_eq!(schema_v61(&tx), schema);
    assert_eq!(full_rows_v61(&tx), before);
    let trigger: String = tx.query_row("SELECT sql FROM sqlite_master WHERE type='trigger' AND name='pr_write_metadata_origins_immutable_v25' AND tbl_name='pr_write_metadata_origins_v25'", [], |row| row.get(0)).unwrap();
    tx.execute_batch("DROP TRIGGER pr_write_metadata_origins_immutable_v25")
        .unwrap();
    assert_eq!(tx.query_row("SELECT count(*) FROM sqlite_master WHERE type='trigger' AND name='pr_write_metadata_origins_immutable_v25'", [], |row| row.get::<_, usize>(0)).unwrap(), 0);
    assert_eq!(tx.execute(&update, [attempt]).unwrap(), 1);
    tx.execute_batch(&trigger).unwrap();
    let restored: String = tx.query_row("SELECT sql FROM sqlite_master WHERE type='trigger' AND name='pr_write_metadata_origins_immutable_v25'", [], |row| row.get(0)).unwrap();
    assert_eq!(restored.as_bytes(), trigger.as_bytes());
    assert_eq!(schema_v61(&tx), schema);
    assert_eq!(full_rows_v61(&tx), expected);
    tx.commit().unwrap();
    assert_eq!(schema_v61(db), schema);
    assert_eq!(full_rows_v61(db), expected);
}

#[test]
fn pr_metadata_members_v57_text_all_three_null_lineage_columns_bind_origin_and_exact_ack_cas() {
    for fault in [
        TextOriginFaultV61::BaseForeign,
        TextOriginFaultV61::ActionForeign,
        TextOriginFaultV61::ParentOne,
        TextOriginFaultV61::BaseBlob,
        TextOriginFaultV61::ActionBlob,
        TextOriginFaultV61::ParentBlob,
    ] {
        let mutation = fault.assignment();
        let (s, original, history, reference) = inspected(Vec::new());
        let mut db = rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        let kinds:(String,String,String)=db.query_row("SELECT typeof(base_key_v48),typeof(action_tag_v48),typeof(parent_ack_attempt_v48) FROM pr_write_metadata_origins_v25 WHERE attempt_id=?1",[reference.attempt_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
        assert_eq!(kinds, ("null".into(), "null".into(), "null".into()));
        let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let target = target(&s, &original);
        let marker = journal
            .original_publication(&target, &reference)
            .unwrap()
            .unwrap();
        assert!(journal.metadata_origin_v25(&target, &marker).is_ok());
        let request = metadata::history::request(&history).unwrap();
        let current = request.view.current.as_ref().unwrap();
        assert!(metadata::history::ack_ready(&history));
        hostile_origin_v61(&mut db, reference.attempt_id, fault);
        let before = sql(&s.fixture);
        assert!(
            journal.metadata_origin_v25(&target, &marker).is_err(),
            "{mutation}"
        );
        assert!(
            metadata::history::acknowledge(
                &request,
                &s.api,
                &database(&s.fixture),
                &current.cancel,
                current.deadline
            )
            .is_err(),
            "{mutation}"
        );
        assert_eq!(sql(&s.fixture), before);
        assert_eq!(
            journal
                .original_publication(&target, &reference)
                .unwrap()
                .unwrap()
                .state,
            PrWriteState::Uncertain
        );
        assert!(!metadata::history::ack_ready(&history));
        assert!(Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&original.scope(), &original.resource())
            .unwrap()
            .is_some());
        exact(&s, s.steps.len());
    }
}

#[test]
fn pr_metadata_members_v57_text_nullable_lineage_graft_refuses_history_before_body_or_inspection() {
    for fault in [
        TextOriginFaultV61::BaseForeign,
        TextOriginFaultV61::ActionForeign,
        TextOriginFaultV61::ParentOne,
    ] {
        let mutation = fault.assignment();
        let (s, original, reference) = unknown(history_phase());
        let mut db = rusqlite::Connection::open_with_flags(
            database(&s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
        )
        .unwrap();
        let protected = Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&original.scope(), &original.resource())
            .unwrap();
        hostile_origin_v61(&mut db, reference.attempt_id, fault);
        let before = sql(&s.fixture);
        let opening = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
        assert!(
            metadata::history::open(
                opening.clone(),
                Field::Description,
                &bounded(&s, &opening, 1024 * 1024),
                &database(&s.fixture),
                opening.cancellation(),
                opening.deadline(),
                None
            )
            .is_err(),
            "{mutation}"
        );
        assert_eq!(sql(&s.fixture), before);
        assert_eq!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .draft(&original.scope(), &original.resource())
                .unwrap(),
            protected
        );
        assert_eq!(
            db.query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [reference.attempt_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "uncertain"
        );
        exact(&s, s.steps.len() - 4);
    }
}
