//! Members-specific actual producer/PrPlan/inspection/SQLite lineage contracts.
//! Corrupt rows below are adversarial storage inputs, never producer authority.
use super::*;
use crate::pr_write_journal::{
    metadata_members_v48::lineage_v52::{MembersActionBaseV48, MembersTerminalRevisionV48},
    metadata_members_v48::MetadataMembersOriginSeedV48,
    PrPublicationReference,
};

fn writable(s: &Setup) -> rusqlite::Connection {
    rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
    )
    .unwrap()
}
#[derive(Clone, Copy)]
enum OriginFaultV61 {
    MalformedOrigin,
    BaseNull,
    ActionNull,
    ParentOne,
    BaseBlob,
    ActionBlob,
    ParentBlob,
}
impl OriginFaultV61 {
    fn assignment(self) -> &'static str {
        match self {
            Self::MalformedOrigin => "origin_json='{'",
            Self::BaseNull => "base_key_v48=NULL",
            Self::ActionNull => "action_tag_v48=NULL",
            Self::ParentOne => "parent_ack_attempt_v48=1",
            Self::BaseBlob => "base_key_v48=CAST(base_key_v48 AS BLOB)",
            Self::ActionBlob => "action_tag_v48=CAST(action_tag_v48 AS BLOB)",
            Self::ParentBlob => "parent_ack_attempt_v48=CAST('1' AS BLOB)",
        }
    }
    fn column(self) -> &'static str {
        match self {
            Self::MalformedOrigin => "origin_json",
            Self::BaseNull | Self::BaseBlob => "base_key_v48",
            Self::ActionNull | Self::ActionBlob => "action_tag_v48",
            Self::ParentOne | Self::ParentBlob => "parent_ack_attempt_v48",
        }
    }
    fn expected(self, before: &rusqlite::types::Value) -> rusqlite::types::Value {
        use rusqlite::types::Value as Sql;
        match self {
            Self::MalformedOrigin => Sql::Text("{".into()),
            Self::BaseNull | Self::ActionNull => Sql::Null,
            Self::ParentOne => Sql::Integer(1),
            Self::BaseBlob | Self::ActionBlob => match before {
                Sql::Text(value) => Sql::Blob(value.as_bytes().to_vec()),
                _ => panic!("Actual original textual lineage column required"),
            },
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
fn hostile_origin_v61(db: &mut rusqlite::Connection, attempt: i64, fault: OriginFaultV61) {
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
            row[column] = fault.expected(&row[column]);
            changed += 1;
        }
    }
    assert_eq!(changed, 1);
    rows.sort_by_key(|row| format!("{row:?}"));
    // No provider I/O or production callback may run while this disposable
    // hostile fixture's trigger is absent. DDL rolls back on any failure.
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
fn raw_attempts(s: &Setup) -> Vec<String> {
    let db = writable(s);
    let mut q = db.prepare("SELECT json_array(a.attempt_id,a.command,a.selected_resource,a.observed_head,a.payload_sha256,a.state,a.confirmed_json,o.repository_id,o.pr_id,o.origin_json,o.base_key_v48,o.action_tag_v48,o.parent_ack_attempt_v48) FROM pr_write_attempts a LEFT JOIN pr_write_metadata_origins_v25 o USING(attempt_id) ORDER BY a.attempt_id").unwrap();
    q.query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}
fn history_steps(inspect: bool) -> Vec<GhStep> {
    let mut calls = principal();
    calls.push(get(&format!("repos/{REPO}"), repo()));
    calls.push(get(&format!("repos/{REPO}/pulls/7"), raw()));
    if inspect {
        calls.push(get(&format!("repos/{REPO}/issues/7"), issue(&raw())));
        calls.push(graphql(Cmd::AddLabel, current(), false));
    }
    calls.push(get(&format!("repos/{REPO}/pulls/7"), raw()));
    calls.push(get(&format!("repos/{REPO}"), repo()));
    calls.extend(principal());
    assert_eq!(calls.len(), if inspect { 10 } else { 8 });
    calls
}
fn uncertain_post() -> Vec<GhStep> {
    let mut calls = post(Cmd::AddLabel);
    calls[1].status = 1;
    calls[1].stdout.clear();
    calls[1].stderr = b"actual member readback unavailable; no receipt inferred".to_vec();
    calls.truncate(2);
    calls
}
fn fresh_api(s: &Setup) -> (GithubProvider, CancellationToken, Instant) {
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    (
        s.api
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
            .unwrap(),
        cancel,
        deadline,
    )
}
fn painted(s: &mut Setup, form: Form) -> Form {
    assert!(s.app.form.is_none());
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(crate::app::Overlay::Form);
    paint(&mut s.app, 120, 40);
    let form = s.app.form.take().unwrap();
    s.app.overlay = None;
    form
}
fn selected(s: &Setup) -> Form {
    let original = PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).unwrap();
    let api = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let mut form = member_ui::open(
        original.clone(),
        Cmd::AddLabel,
        &api,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    form.dispatch(&FormAction::OpenPicker(0));
    assert!(form.metadata_picker.is_some());
    form.dispatch(&FormAction::MetadataPicker(
        PrMetadataPickerAction::IndexedRow(0),
    ));
    form.dispatch(&FormAction::MetadataPicker(PrMetadataPickerAction::Choose));
    assert!(form.metadata_picker.is_none());
    assert!(!form.dirty);
    form
}
fn preview(s: &mut Setup, mut form: Form) -> (Form, Arc<member_ui::MetadataMembersPreviewV48>) {
    let request = member_ui::request(&form).unwrap();
    let (api, cancel, deadline) = fresh_api(s);
    let preview = member_ui::MetadataMembersPreviewV48::prepare(
        &request,
        &api,
        &database(&s.fixture),
        &cancel,
        deadline,
    )
    .unwrap();
    assert!(member_ui::accept_preview(
        &mut form,
        &request,
        preview.clone()
    ));
    (painted(s, form), preview)
}
fn explicit_new(s: &mut Setup, mut form: Form) -> Form {
    let old = form.publication.clone().unwrap();
    let before = raw_attempts(s);
    let drafts = writable(s).query_row("SELECT json_group_array(json_array(resource,body)) FROM (SELECT resource,body FROM drafts ORDER BY resource)", [], |r|r.get::<_,String>(0)).unwrap();
    let request = member_ui::new_request_v52(&form).unwrap();
    let (api, cancel, deadline) = fresh_api(s);
    let fresh = request
        .prepare(&api, &database(&s.fixture), &cancel, deadline)
        .unwrap();
    assert!(!fresh.notice_v52);
    assert_eq!(fresh.lineage_v52.as_ref().unwrap().reference(), &old);
    assert!(member_ui::accept_new_v52(&mut form, &request, fresh));
    assert!(!form.preview && !form.dirty && form.publication.is_none());
    assert_eq!(raw_attempts(s), before);
    assert_eq!(writable(s).query_row("SELECT json_group_array(json_array(resource,body)) FROM (SELECT resource,body FROM drafts ORDER BY resource)", [], |r|r.get::<_,String>(0)).unwrap(), drafts);
    form
}
fn uncertain(
    s: &Setup,
    form: &Form,
    preview: &Arc<member_ui::MetadataMembersPreviewV48>,
) -> PrPublicationReference {
    let mut writer = DraftWriter::new(database(&s.fixture));
    let body = ui::encoded_draft(form).unwrap();
    save_and_ack(&mut writer, form, &form.resource(), &body, s);
    let client = writer.client();
    let (api, cancel, deadline) = fresh_api(s);
    let fence = client
        .capture_publication_v2(
            &database(&s.fixture),
            &form.scope(),
            &form.resource(),
            &cancel,
            deadline,
        )
        .unwrap();
    let outcome = PrPlan::execute_original_metadata_members_v48(
        preview,
        &api,
        &database(&s.fixture),
        &client,
        fence,
        &cancel,
        deadline,
    )
    .unwrap();
    assert_eq!(outcome.state, State::Uncertain);
    assert!(outcome.refreshed_page_v19.is_none());
    assert!(outcome.draft_retained());
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
            .unwrap()
            .as_deref(),
        Some(
            crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(
                &preview.values,
                Some(&outcome.reference),
            )
            .unwrap()
            .as_str()
        )
    );
    outcome.reference
}
fn inspected(s: &mut Setup, original: Arc<PrCollaborationOpenIntentV17>) -> Form {
    let (api, cancel, deadline) = fresh_api(s);
    let listed = member_ui::history::open(
        original,
        &api,
        &database(&s.fixture),
        &cancel,
        deadline,
        None,
    )
    .unwrap();
    let request = member_ui::history::request(&listed).unwrap();
    let (api, cancel, deadline) = fresh_api(s);
    let observed = member_ui::history::open(
        request.view.original.clone(),
        &api,
        &database(&s.fixture),
        &cancel,
        deadline,
        Some(&request),
    )
    .unwrap();
    let observed = painted(s, observed);
    assert!(member_ui::history::ack_ready(&observed));
    observed
}
fn acknowledge(s: &Setup, history: &Form, reference: &PrPublicationReference) {
    let request = member_ui::history::request(history).unwrap();
    let current = request.view.current.as_ref().unwrap();
    let before = raw_attempts(s);
    let target = &request
        .view
        .snapshot
        .rows
        .iter()
        .find(|row| row.marker.publication_reference() == *reference)
        .unwrap()
        .marker
        .target;
    let generation = PrWriteJournal::open(&database(&s.fixture))
        .unwrap()
        .source_generation(target)
        .unwrap();
    assert_eq!(
        member_ui::history::acknowledge(
            &request,
            &s.api,
            &database(&s.fixture),
            &current.cancel,
            current.deadline
        )
        .unwrap(),
        (reference.clone(), 1)
    );
    let after = raw_attempts(s);
    assert_eq!(after.len(), before.len());
    for (a, b) in before.iter().zip(&after) {
        let mut expected: Value = serde_json::from_str(a).unwrap();
        if expected[0].as_i64() == Some(reference.attempt_id) {
            assert_eq!(expected[5], "uncertain");
            expected[5] = json!("acknowledged");
        }
        assert_eq!(serde_json::from_str::<Value>(b).unwrap(), expected);
    }
    assert_eq!(
        PrWriteJournal::open(&database(&s.fixture))
            .unwrap()
            .source_generation(target)
            .unwrap(),
        generation + 1
    );
    assert!(member_ui::history::acknowledge(
        &request,
        &s.api,
        &database(&s.fixture),
        &current.cancel,
        current.deadline
    )
    .is_err());
}
fn first_ack(tail: Vec<GhStep>) -> (Setup, Form, PrPublicationReference) {
    let mut calls = selected_phase(Cmd::AddLabel);
    calls.extend(uncertain_post());
    calls.extend(history_steps(false));
    calls.extend(history_steps(true));
    calls.extend(tail);
    let (mut s, form, preview) = prepared(Cmd::AddLabel, calls);
    let reference = uncertain(&s, &form, &preview);
    let history = inspected(&mut s, preview.intent.source.opening().clone());
    acknowledge(&s, &history, &reference);
    (s, form, reference)
}
fn next_calls() -> Vec<GhStep> {
    let mut calls = phase(Cmd::AddLabel, raw(), candidate(Cmd::AddLabel), current());
    for _ in 0..4 {
        calls.extend(selected_phase(Cmd::AddLabel));
    }
    calls.extend(uncertain_post());
    calls.extend(history_steps(false));
    calls.extend(history_steps(true));
    calls
}

fn observed_head(calls: Vec<GhStep>, head: &str) -> Vec<GhStep> {
    fn replace(value: &mut Value, head: &str) {
        match value {
            Value::Object(map) => {
                if let Some(Value::Object(actual)) = map.get_mut("head") {
                    if actual.contains_key("sha") {
                        actual.insert("sha".into(), json!(head));
                    }
                }
                if map.contains_key("headRefOid") {
                    map.insert("headRefOid".into(), json!(head));
                }
                for child in map.values_mut() {
                    replace(child, head);
                }
            }
            Value::Array(rows) => {
                for row in rows {
                    replace(row, head);
                }
            }
            _ => {}
        }
    }
    calls
        .into_iter()
        .map(|mut call| {
            if let Ok(mut value) = serde_json::from_slice::<Value>(&call.stdout) {
                replace(&mut value, head);
                call.stdout = serde_json::to_vec(&value).unwrap();
            }
            call
        })
        .collect()
}

#[test]
fn pr_metadata_members_v57_fresh_changed_head_keeps_stable_action_but_binds_new_current_draft() {
    let head = "c".repeat(40);
    let mut tail = observed_head(opening(), &head);
    tail.extend(observed_head(
        phase(Cmd::AddLabel, raw(), candidate(Cmd::AddLabel), current()),
        &head,
    ));
    for _ in 0..3 {
        tail.extend(observed_head(selected_phase(Cmd::AddLabel), &head));
    }
    let (mut s, old_form, old_reference) = first_ack(tail);
    let old_rows = raw_attempts(&s);
    let old_row: Value = serde_json::from_str(&old_rows[0]).unwrap();
    let old_origin: Value = serde_json::from_str(old_row[9].as_str().unwrap()).unwrap();
    let cancel = CancellationToken::default();
    let mut workspace = s
        .app
        .github
        .take()
        .unwrap()
        .refresh(&s.api, &database(&s.fixture), &cancel)
        .unwrap();
    workspace.select_index(0);
    let selected_key = workspace.selected.clone().unwrap();
    workspace = workspace.open(&selected_key, &s.api, &cancel).unwrap();
    assert_eq!(
        workspace.pr.as_ref().unwrap().details.summary.head_sha,
        head
    );
    s.app.context = workspace.context();
    s.app.github = Some(workspace);
    let form = selected(&s);
    let (form, notice) = preview(&mut s, form);
    assert!(notice.notice_v52);
    assert_eq!(
        notice.lineage_v52.as_ref().unwrap().reference(),
        &old_reference
    );
    assert_eq!(
        notice.seed.base_key(),
        old_origin["seed"]["base_key"].as_str().unwrap()
    );
    let form = explicit_new(&mut s, form);
    let (form, fresh) = preview(&mut s, form);
    assert!(!fresh.notice_v52);
    let (stable, current_draft) = fresh.actual.action_keys_v48(&fresh.plan).unwrap();
    assert_eq!(stable, old_origin["seed"]["base_key"]);
    assert_ne!(
        current_draft,
        old_form.resource(),
        "Head-bound root draft must change even though the stable action identity does not"
    );
    assert_ne!(form.resource(), old_form.resource());
    assert_ne!(
        form.resource(),
        current_draft,
        "Explicit New is bound to the exact terminal reference, not a fresh root fallback"
    );
    assert_eq!(fresh.seed.write(&fresh.values).unwrap().observed_head, head);
    assert_eq!(old_reference.observed_head, "a".repeat(40));
    assert_eq!(raw_attempts(&s), old_rows);
    exact(&s, s.steps.len());
}

#[test]
fn pr_metadata_members_v57_three_real_terminal_new_ack_chain_preserves_root_and_exact_ancestors() {
    let mut tail = next_calls();
    tail.extend(next_calls());
    let (mut s, _, first) = first_ack(tail);
    let root = raw_attempts(&s)[0].clone();
    let mut refs = vec![first];
    for _ in 0..2 {
        let form = selected(&s);
        let (form, notice) = preview(&mut s, form);
        assert!(notice.notice_v52 && !form.dirty);
        assert_eq!(form.publication.as_ref(), refs.last());
        let form = explicit_new(&mut s, form);
        let (form, p) = preview(&mut s, form);
        assert!(!p.notice_v52);
        let reference = uncertain(&s, &form, &p);
        let history = inspected(&mut s, p.intent.source.opening().clone());
        acknowledge(&s, &history, &reference);
        refs.push(reference);
        assert_eq!(raw_attempts(&s)[0], root);
    }
    let rows = raw_attempts(&s);
    assert_eq!(rows.len(), 3);
    let parsed = rows
        .iter()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert!(parsed
        .iter()
        .all(|r| r[5] == "acknowledged" && r[6].is_null()));
    let keys = parsed
        .iter()
        .map(|r| r[2].as_str().unwrap())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(keys.len(), 3);
    assert_eq!(parsed[0][10], parsed[1][10]);
    assert_eq!(parsed[1][10], parsed[2][10]);
    assert!(parsed[0][12].is_null());
    assert_eq!(parsed[1][12], refs[0].attempt_id);
    assert_eq!(parsed[2][12], refs[0].attempt_id);
    for r in &parsed {
        let origin: Value = serde_json::from_str(r[9].as_str().unwrap()).unwrap();
        assert_eq!(origin["reference"]["attempt_id"], r[0]);
    }
    exact(&s, s.steps.len());
}

type RevisionV57 = anyhow::Result<Option<Arc<MembersTerminalRevisionV48>>>;
fn revision(
    s: &Setup,
    form: &Form,
    corrupt: impl FnOnce() -> Vec<String>,
) -> (RevisionV57, Vec<String>) {
    let request = member_ui::request(form).unwrap();
    let (api, cancel, deadline) = fresh_api(s);
    request
        .intent
        .source
        .with_metadata_members_phase_v48(
            &api,
            &database(&s.fixture),
            &cancel,
            deadline,
            members::MetadataMembersPhasePurposeV48::Preview(
                &request.plan,
                request.intent.actual.limits(),
            ),
            |phase| {
                let local = phase.local(&database(&s.fixture))?;
                let seed = MetadataMembersOriginSeedV48::from_producer(
                    &request.intent.source,
                    phase.actual(),
                    &request.plan,
                    &request.values,
                    &database(&s.fixture),
                )
                .unwrap();
                let base = MembersActionBaseV48::from_producer(
                    &seed,
                    &phase,
                    &request.plan,
                    &database(&s.fixture),
                )
                .unwrap();
                // The actual source/producer/local guard is already genuine.
                // Corruption then targets the exact lookup, not an earlier
                // unrelated source/repair refusal standing in for this test.
                let before = corrupt();
                let result = PrWriteJournal::open(&database(&s.fixture))
                    .unwrap()
                    .members_action_revision_v52(&base);
                phase.finish(|| {
                    local.validate_boundary_v13(
                        &database(&s.fixture),
                        request.intent.source.local_boundary(),
                    )
                })?;
                Ok((result, before))
            },
        )
        .unwrap()
}
enum BadNewest {
    Malformed,
    Dangling,
    Foreign,
}
fn insert_dangling_origin_v79(db: &rusqlite::Connection, id: i64, parent: i64) {
    use rusqlite::types::Value as Sql;
    const INSERT: &str = "INSERT INTO pr_write_metadata_origins_v25 SELECT ?1,repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48 FROM pr_write_metadata_origins_v25 WHERE attempt_id=?2";
    assert!(db.is_autocommit());
    db.pragma_update(None, "foreign_keys", true).unwrap();
    let before = full_rows_v61(db);
    let schema = schema_v61(db);
    let refused = db
        .execute(INSERT, rusqlite::params![id, parent])
        .unwrap_err();
    assert!(matches!(refused, rusqlite::Error::SqliteFailure(error, _)
        if error.extended_code == 787));
    assert_eq!(full_rows_v61(db), before);
    assert_eq!(schema_v61(db), schema);
    let mut expected = before;
    let (columns, rows) = expected.get_mut("pr_write_metadata_origins_v25").unwrap();
    let attempt = columns
        .iter()
        .position(|name| name == "attempt_id")
        .unwrap();
    assert!(!rows.iter().any(|row| row[attempt] == Sql::Integer(id)));
    let mut orphan = rows
        .iter()
        .find(|row| row[attempt] == Sql::Integer(parent))
        .unwrap()
        .clone();
    orphan[attempt] = Sql::Integer(id);
    rows.push(orphan);
    rows.sort_by_key(|row| format!("{row:?}"));
    // Deliberate corruption of this disposable fixture's ONE connection only.
    // No child/provider/validator runs until FK enforcement is restored.
    db.pragma_update(None, "foreign_keys", false).unwrap();
    let inserted = db.execute(INSERT, rusqlite::params![id, parent]);
    db.pragma_update(None, "foreign_keys", true).unwrap();
    assert_eq!(inserted.unwrap(), 1);
    assert!(db
        .pragma_query_value(None, "foreign_keys", |row| row.get::<_, bool>(0))
        .unwrap());
    assert_eq!(schema_v61(db), schema);
    assert_eq!(full_rows_v61(db), expected);
    let violations = db
        .prepare("PRAGMA foreign_key_check(pr_write_metadata_origins_v25)")
        .unwrap()
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(
        violations,
        vec![(
            "pr_write_metadata_origins_v25".into(),
            id,
            "pr_write_attempts".into()
        )]
    );
}
fn bad_newest(kind: BadNewest) {
    let mut tail = phase(Cmd::AddLabel, raw(), candidate(Cmd::AddLabel), current());
    tail.extend(selected_phase(Cmd::AddLabel));
    let (s, _, reference) = first_ack(tail);
    let form = selected(&s);
    let mut db = writable(&s);
    let id = reference.attempt_id + 100;
    let (result, before) = revision(&s, &form, || {
        if !matches!(kind, BadNewest::Dangling) {
            db.execute("INSERT INTO pr_write_attempts SELECT ?1,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at FROM pr_write_attempts WHERE attempt_id=?2",rusqlite::params![id,reference.attempt_id]).unwrap();
        }
        if matches!(kind, BadNewest::Dangling) {
            insert_dangling_origin_v79(&db, id, reference.attempt_id);
        } else {
            db.execute("INSERT INTO pr_write_metadata_origins_v25 SELECT ?1,repository_id,pr_id,origin_json,base_key_v48,action_tag_v48,parent_ack_attempt_v48 FROM pr_write_metadata_origins_v25 WHERE attempt_id=?2",rusqlite::params![id,reference.attempt_id]).unwrap();
        }
        match kind {
            BadNewest::Malformed => {
                hostile_origin_v61(&mut db, id, OriginFaultV61::MalformedOrigin);
            }
            BadNewest::Foreign => {
                db.execute(
                    "UPDATE pr_write_attempts SET pr_node='FOREIGN_NEWEST_PR' WHERE attempt_id=?1",
                    [id],
                )
                .unwrap();
            }
            BadNewest::Dangling => {}
        }
        sql(&s.fixture)
    });
    assert!(
        result.is_err(),
        "Newest exact index must refuse; never resurrect the older valid ACK"
    );
    assert_eq!(sql(&s.fixture), before);
    assert_eq!(
        writable(&s)
            .query_row(
                "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                [reference.attempt_id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "acknowledged"
    );
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_members_v57_malformed_newest_index_never_falls_back_to_older_ack() {
    bad_newest(BadNewest::Malformed)
}
#[test]
fn pr_metadata_members_v57_dangling_newest_index_never_falls_back_to_older_ack() {
    bad_newest(BadNewest::Dangling)
}
#[test]
fn pr_metadata_members_v57_foreign_newest_index_never_falls_back_to_older_ack() {
    bad_newest(BadNewest::Foreign)
}

#[test]
fn pr_metadata_members_v57_each_raw_lineage_column_and_type_mutation_refuses_exact_ack_without_release(
) {
    for fault in [
        OriginFaultV61::BaseNull,
        OriginFaultV61::ActionNull,
        OriginFaultV61::ParentOne,
        OriginFaultV61::BaseBlob,
        OriginFaultV61::ActionBlob,
        OriginFaultV61::ParentBlob,
    ] {
        let mutation = fault.assignment();
        let mut calls = selected_phase(Cmd::AddLabel);
        calls.extend(uncertain_post());
        calls.extend(history_steps(false));
        calls.extend(history_steps(true));
        let (mut s, form, p) = prepared(Cmd::AddLabel, calls);
        let reference = uncertain(&s, &form, &p);
        let history = inspected(&mut s, p.intent.source.opening().clone());
        let request = member_ui::history::request(&history).unwrap();
        let current = request.view.current.as_ref().unwrap();
        hostile_origin_v61(&mut writable(&s), reference.attempt_id, fault);
        let before = sql(&s.fixture);
        assert!(
            member_ui::history::acknowledge(
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
        assert!(!member_ui::history::ack_ready(&history));
        assert_eq!(
            writable(&s)
                .query_row(
                    "SELECT state FROM pr_write_attempts WHERE attempt_id=?1",
                    [reference.attempt_id],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
            "uncertain"
        );
        assert!(Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&form.scope(), &form.resource())
            .unwrap()
            .is_some());
        exact(&s, s.steps.len());
    }
}
