//! Real producer + actual SQLite lineage contracts. State SQL below is an
//! explicit storage-only fixture, NOT proof of remote invocation or ACK UI.
use super::*;
use crate::pr_write_journal::review_action_lineage_v12::{
    ReviewActionBaseV12, ReviewActionOriginSeedV12,
};

struct LineageReal {
    s: Setup,
    source: Arc<ScopedReviewOperationSourceV3>,
    target: PrWriteTarget,
    base: ReviewActionBaseV12,
    reference: crate::pr_write_journal::PrPublicationReference,
    encoded: String,
}
fn real(confirmed: bool) -> LineageReal {
    let (s, _app, intent) = operation_fixture(1);
    let source = operation_source(&s, intent.clone());
    let path = database(&s.fixture);
    let target =
        PrWriteTarget::new(source.identity(), intent.original().parent().summary()).unwrap();
    let base = ReviewActionBaseV12::operation(&intent).unwrap();
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let write = source.prepared_write().unwrap();
    let permit = journal
        .begin_review_operation_v3(&target, write, source.origin_seed().unwrap())
        .unwrap();
    journal.claim(&permit, &target, write).unwrap();
    if confirmed {
        journal
            .mark_confirmed(
                &permit,
                &PrConfirmedEffect {
                    kind: PrEffectKind::Comment,
                    resource_id: source.pending_comment().unwrap().comment().id().to_string(),
                    node_id: Some(source.pending_comment().unwrap().comment().node().into()),
                },
            )
            .unwrap();
    } else {
        journal.mark_uncertain(&permit).unwrap();
    }
    let reference = permit.marker().publication_reference();
    let encoded = operations::draft_v3::encode(source.body(), Some(&reference)).unwrap();
    Cache::open(&path)
        .unwrap()
        .put_draft(target.scope(), base.key(), &encoded)
        .unwrap();
    LineageReal {
        s,
        source,
        target,
        base,
        reference,
        encoded,
    }
}
fn acknowledge_sql(
    real: &LineageReal,
    reference: &crate::pr_write_journal::PrPublicationReference,
) {
    // Explicit state fixture. Actual fresh-provider/runner ACK is exercised by
    // the separately declared restarted binary journeys, not this SQL helper.
    let mut db = rusqlite::Connection::open(database(&real.s.fixture)).unwrap();
    let tx = db.transaction().unwrap();
    assert_eq!(tx.execute("UPDATE pr_write_attempts SET state='acknowledged' WHERE attempt_id=?1 AND state='uncertain' AND confirmed_json IS NULL",[reference.attempt_id]).unwrap(),1);
    assert_eq!(tx.execute("UPDATE pr_write_scopes SET generation=generation+1 WHERE host=?1 AND principal=?2 AND pr_node=?3",
        rusqlite::params![real.target.scope().host,real.target.scope().login,real.target.node_id()]).unwrap(),1);
    tx.commit().unwrap();
}
fn revision(
    real: &LineageReal,
) -> Arc<crate::pr_write_journal::review_action_lineage_v12::ReviewAcknowledgedRevisionV12> {
    PrWriteJournal::open(&database(&real.s.fixture))
        .unwrap()
        .action_revision_v12(&real.base)
        .unwrap()
        .unwrap()
}
fn next_write(
    real: &LineageReal,
    revision: &crate::pr_write_journal::review_action_lineage_v12::ReviewAcknowledgedRevisionV12,
    body: &str,
) -> crate::pr_write_journal::PrWriteIntent {
    let mut write = real
        .source
        .origin_seed()
        .unwrap()
        .metadata()
        .write(body)
        .unwrap();
    revision.bind_write(&mut write).unwrap();
    write
}
#[test]
fn pending_reviews_v12_actual_uncertain_refuses_then_ack_uses_exact_root_no_replay_or_sql_write() {
    let real = real(false);
    let path = database(&real.s.fixture);
    assert!(PrWriteJournal::open(&path)
        .unwrap()
        .action_revision_v12(&real.base)
        .is_err());
    acknowledge_sql(&real, &real.reference);
    let before = (sql_state(&real.s.fixture), journal_state(&real));
    let commands_before = commands(&real.s.fixture).len();
    let revision = revision(&real);
    assert_ne!(revision.key(), real.base.key());
    assert_eq!(revision.reference(), &real.reference);
    assert_eq!(revision.label(), "Acknowledged original attempt");
    revision.validate(&path, &real.base).unwrap();
    assert_eq!(
        Cache::open(&path)
            .unwrap()
            .draft(real.target.scope(), real.base.key())
            .unwrap()
            .as_deref(),
        Some(real.encoded.as_str())
    );
    assert!(Cache::open(&path)
        .unwrap()
        .draft(real.target.scope(), revision.key())
        .unwrap()
        .is_none());
    assert_eq!((sql_state(&real.s.fixture), journal_state(&real)), before);
    assert_eq!(commands(&real.s.fixture).len(), commands_before);
    unchanged(&real);
}
#[test]
fn pending_reviews_v12_repeated_chain_is_indexed_restart_stable_and_old_v3_bytes_immutable() {
    let real = real(false);
    let path = database(&real.s.fixture);
    acknowledge_sql(&real, &real.reference);
    let db = rusqlite::Connection::open(&path).unwrap();
    let original: String = db
        .query_row(
            "SELECT origin_json FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",
            [real.reference.attempt_id],
            |r| r.get(0),
        )
        .unwrap();
    let r1 = revision(&real);
    let write = next_write(&real, &r1, "NEW explicit request only");
    let seed = ReviewActionOriginSeedV12::from_revision(
        &r1,
        Some(real.source.origin_seed().unwrap()),
        &write,
    )
    .unwrap();
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let permit = journal
        .begin_review_action_v12(&real.target, &write, &seed)
        .unwrap();
    journal.claim(&permit, &real.target, &write).unwrap();
    journal.mark_uncertain(&permit).unwrap();
    let first = permit.marker().publication_reference();
    let encoded = operations::draft_v3::encode("NEW explicit request only", Some(&first)).unwrap();
    Cache::open(&path)
        .unwrap()
        .put_draft(real.target.scope(), r1.key(), &encoded)
        .unwrap();
    assert!(journal.action_revision_v12(&real.base).is_err());
    acknowledge_sql(&real, &first);
    drop(journal);
    let r2 = revision(&real);
    assert_ne!(r1.key(), r2.key());
    assert_eq!(r2.reference(), &first);
    let indexed:(String,String,i64)=db.query_row("SELECT base_key_v12,action_tag_v12,parent_ack_attempt_v12 FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",[first.attempt_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).unwrap();
    assert_eq!(
        indexed,
        (
            real.base.key().into(),
            real.base.action().into(),
            real.reference.attempt_id
        )
    );
    assert_eq!(
        db.query_row(
            "SELECT origin_json FROM pr_write_review_operation_origins_v3 WHERE attempt_id=?1",
            [real.reference.attempt_id],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        original
    );
    let reopened = PrWriteJournal::open(&path)
        .unwrap()
        .action_revision_v12(&real.base)
        .unwrap()
        .unwrap();
    assert_eq!(reopened.key(), r2.key());
    assert_eq!(
        Cache::open(&path)
            .unwrap()
            .draft(real.target.scope(), r1.key())
            .unwrap(),
        Some(encoded)
    );
    let external = operations::draft_v3::encode("EXTERNAL different body", Some(&first)).unwrap();
    Cache::open(&path)
        .unwrap()
        .put_draft(real.target.scope(), r1.key(), &external)
        .unwrap();
    let before = (sql_state(&real.s.fixture), journal_state(&real));
    assert!(PrWriteJournal::open(&path)
        .unwrap()
        .action_revision_v12(&real.base)
        .is_err());
    assert_eq!((sql_state(&real.s.fixture), journal_state(&real)), before);
    unchanged(&real);
}
#[test]
fn pending_reviews_v12_confirmed_cleanup_history_has_distinct_label_and_actual_receipt() {
    let real = real(true);
    let r = revision(&real);
    assert_eq!(
        r.label(),
        "Confirmed original receipt — retained cleanup history"
    );
    assert_eq!(r.reference(), &real.reference);
    let db = rusqlite::Connection::open(database(&real.s.fixture)).unwrap();
    db.execute(
        "UPDATE pr_write_attempts SET confirmed_json=?2 WHERE attempt_id=?1",
        rusqlite::params![
            real.reference.attempt_id,
            r#"{"kind":"comment","resource_id":"999","node_id":"FOREIGN_COMMENT"}"#
        ],
    )
    .unwrap();
    assert!(PrWriteJournal::open(&database(&real.s.fixture))
        .unwrap()
        .action_revision_v12(&real.base)
        .is_err());
    unchanged(&real);
}
#[test]
fn pending_reviews_v12_atomic_origin_failure_rolls_back_attempt_generation_and_preserves_protected_root(
) {
    let real = real(false);
    let path = database(&real.s.fixture);
    acknowledge_sql(&real, &real.reference);
    let r = revision(&real);
    let write = next_write(&real, &r, "genuine new body");
    let seed = ReviewActionOriginSeedV12::from_revision(
        &r,
        Some(real.source.origin_seed().unwrap()),
        &write,
    )
    .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER deliberate_v12_origin_failure BEFORE INSERT ON pr_write_review_operation_origins_v3 WHEN NEW.base_key_v12 IS NOT NULL BEGIN SELECT RAISE(ABORT,'DELIBERATE_ORIGIN_FAILURE'); END;").unwrap();
    let before = (sql_state(&real.s.fixture), journal_state(&real));
    assert!(PrWriteJournal::open(&path)
        .unwrap()
        .begin_review_action_v12(&real.target, &write, &seed)
        .is_err());
    assert_eq!((sql_state(&real.s.fixture), journal_state(&real)), before);
    r.validate(&path, &real.base).unwrap();
    unchanged(&real);
}
#[test]
fn pending_reviews_v12_external_protected_replacement_or_newer_generation_refuses_before_begin() {
    let real = real(false);
    let path = database(&real.s.fixture);
    acknowledge_sql(&real, &real.reference);
    let r = revision(&real);
    let write = next_write(&real, &r, "new body");
    let seed = ReviewActionOriginSeedV12::from_revision(
        &r,
        Some(real.source.origin_seed().unwrap()),
        &write,
    )
    .unwrap();
    let external = operations::draft_v3::encode(
        "EXTERNAL replacement with unchanged real publication reference",
        Some(&real.reference),
    )
    .unwrap();
    Cache::open(&path)
        .unwrap()
        .put_draft(real.target.scope(), real.base.key(), &external)
        .unwrap();
    let before = (sql_state(&real.s.fixture), journal_state(&real));
    assert!(r.validate(&path, &real.base).is_err());
    assert!(PrWriteJournal::open(&path)
        .unwrap()
        .begin_review_action_v12(&real.target, &write, &seed)
        .is_err());
    assert_eq!((sql_state(&real.s.fixture), journal_state(&real)), before);
    unchanged(&real);
}
#[test]
fn pending_reviews_v12_missing_root_without_index_does_not_salt_from_unrelated_ack() {
    let real = real(false);
    let path = database(&real.s.fixture);
    acknowledge_sql(&real, &real.reference);
    Cache::open(&path)
        .unwrap()
        .remove_draft(real.target.scope(), real.base.key())
        .unwrap();
    let before = (sql_state(&real.s.fixture), journal_state(&real));
    assert!(PrWriteJournal::open(&path)
        .unwrap()
        .action_revision_v12(&real.base)
        .unwrap()
        .is_none());
    assert_eq!((sql_state(&real.s.fixture), journal_state(&real)), before);
    assert!(PrWriteJournal::open(&path)
        .unwrap()
        .original_publication(&real.target, &real.reference)
        .unwrap()
        .is_some());
    unchanged(&real);
}
#[test]
fn pending_reviews_v12_malformed_newest_origin_refuses_without_older_terminal_revival() {
    let real = real(false);
    let path = database(&real.s.fixture);
    acknowledge_sql(&real, &real.reference);
    let r = revision(&real);
    let write = next_write(&real, &r, "newbody");
    let seed = ReviewActionOriginSeedV12::from_revision(
        &r,
        Some(real.source.origin_seed().unwrap()),
        &write,
    )
    .unwrap();
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let permit = journal
        .begin_review_action_v12(&real.target, &write, &seed)
        .unwrap();
    journal.claim(&permit, &real.target, &write).unwrap();
    journal.mark_uncertain(&permit).unwrap();
    acknowledge_sql(&real, &permit.marker().publication_reference());
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("DROP TRIGGER pr_write_review_operation_origins_immutable_v3")
        .unwrap();
    db.execute(
        "UPDATE pr_write_review_operation_origins_v3 SET origin_json='{}' WHERE attempt_id=?1",
        [permit.marker().attempt_id],
    )
    .unwrap();
    let before = (sql_state(&real.s.fixture), journal_state(&real));
    assert!(journal.action_revision_v12(&real.base).is_err());
    assert_eq!((sql_state(&real.s.fixture), journal_state(&real)), before);
    unchanged(&real);
    db.execute(
        "UPDATE pr_write_review_operation_origins_v3 SET origin_json=?2 WHERE attempt_id=?1",
        rusqlite::params![permit.marker().attempt_id, "X".repeat(16 * 1024 + 1)],
    )
    .unwrap();
    let before = (sql_state(&real.s.fixture), journal_state(&real));
    assert!(journal.action_revision_v12(&real.base).is_err());
    assert_eq!((sql_state(&real.s.fixture), journal_state(&real)), before);
}
#[test]
fn pending_reviews_v12_postreceipt_external_parent_envelope_blocks_cleanup_without_altering_either_body(
) {
    let real = real(false);
    let path = database(&real.s.fixture);
    acknowledge_sql(&real, &real.reference);
    let revision = revision(&real);
    let write = next_write(&real, &revision, "NEW requested body");
    let seed = ReviewActionOriginSeedV12::from_revision(
        &revision,
        Some(real.source.origin_seed().unwrap()),
        &write,
    )
    .unwrap();
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let permit = journal
        .begin_review_action_v12(&real.target, &write, &seed)
        .unwrap();
    journal.claim(&permit, &real.target, &write).unwrap();
    let reference = permit.marker().publication_reference();
    let encoded = operations::draft_v3::encode("NEW requested body", Some(&reference)).unwrap();
    let cache = Cache::open(&path).unwrap();
    cache
        .put_draft(real.target.scope(), revision.key(), &encoded)
        .unwrap();
    let actual = real.source.pending_comment().unwrap().comment();
    journal
        .mark_confirmed(
            &permit,
            &PrConfirmedEffect {
                kind: PrEffectKind::Comment,
                resource_id: actual.id().to_string(),
                node_id: Some(actual.node().into()),
            },
        )
        .unwrap();
    seed.validate_owned_basis(&path, &real.target, permit.marker())
        .unwrap();
    cache
        .put_draft(
            real.target.scope(),
            real.base.key(),
            "EXTERNAL newer protected ancestor envelope",
        )
        .unwrap();
    let before = (sql_state(&real.s.fixture), journal_state(&real));
    assert!(seed
        .validate_owned_basis(&path, &real.target, permit.marker())
        .is_err());
    assert_eq!((sql_state(&real.s.fixture), journal_state(&real)), before);
    assert_eq!(
        cache.draft(real.target.scope(), revision.key()).unwrap(),
        Some(encoded)
    );
    assert_eq!(
        cache
            .draft(real.target.scope(), real.base.key())
            .unwrap()
            .as_deref(),
        Some("EXTERNAL newer protected ancestor envelope")
    );
    unchanged(&real);
}
#[test]
fn pending_reviews_v12_foreign_database_stamp_refuses_before_journal_migration_or_private_read() {
    let real = real(false);
    acknowledge_sql(&real, &real.reference);
    let revision = revision(&real);
    let foreign = real.s.fixture.root.join("foreign-lineage.sqlite3");
    let mut cache = Cache::open(&foreign).unwrap();
    cache
        .clear_scope(&Scope::github("foreign.invalid", Some(777)).unwrap(), false)
        .unwrap();
    drop(cache);
    let connection = rusqlite::Connection::open(&foreign).unwrap();
    let tables = || {
        connection
            .prepare("SELECT type,name,sql FROM sqlite_master ORDER BY type,name")
            .unwrap()
            .query_map([], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>()
    };
    let before = tables();
    assert!(revision.validate(&foreign, &real.base).is_err());
    assert_eq!(tables(), before);
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM drafts", [], |r| r.get::<_, usize>(0))
            .unwrap(),
        0
    );
    unchanged(&real);
}
#[test]
fn pending_reviews_v12_migration_nullable_index_and_no_new_table_preserves_legacy_origin() {
    let real = real(false);
    let path = database(&real.s.fixture);
    let db = rusqlite::Connection::open(&path).unwrap();
    let before: String = db
        .query_row(
            "SELECT origin_json FROM pr_write_review_operation_origins_v3",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let columns:usize=db.query_row("SELECT count(*) FROM pragma_table_info('pr_write_review_operation_origins_v3') WHERE name IN('base_key_v12','action_tag_v12','parent_ack_attempt_v12') AND [notnull]=0",[],|r|r.get(0)).unwrap();
    assert_eq!(columns, 3);
    assert_eq!(db.query_row("SELECT count(*) FROM pr_write_review_operation_origins_v3 WHERE base_key_v12 IS NULL AND action_tag_v12 IS NULL AND parent_ack_attempt_v12 IS NULL",[],|r|r.get::<_,usize>(0)).unwrap(),1);
    drop(PrWriteJournal::open(&path).unwrap());
    assert_eq!(
        db.query_row(
            "SELECT origin_json FROM pr_write_review_operation_origins_v3",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        before
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name LIKE '%lineage%'",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        0
    );
    unchanged(&real);
}

fn journal_state(real: &LineageReal) -> Vec<String> {
    let db = rusqlite::Connection::open(database(&real.s.fixture)).unwrap();
    ["SELECT json_array(attempt_id,host,principal,pr_node,repository,pr_number,command,selected_resource,observed_head,payload_sha256,state,confirmed_json,created_at) FROM pr_write_attempts ORDER BY attempt_id",
    "SELECT json_array(host,principal,pr_node,generation) FROM pr_write_scopes ORDER BY host,principal,pr_node",
    "SELECT json_array(attempt_id,repository_id,repository_node,pr_id,origin_json,base_key_v12,action_tag_v12,parent_ack_attempt_v12) FROM pr_write_review_operation_origins_v3 ORDER BY attempt_id"]
    .iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
#[test]
fn pending_reviews_v12_shared_submit_command_isolated_by_actual_action_key_and_nullable_legacy_root(
) {
    use images::reviews_v2::{ReviewRequestedActionV2 as Action, ScopedReviewSelectionV2};
    let mut s = inline_setup(|h, steps| {
        let row = review(h, "PENDING", "Actual source current summary", 42);
        steps.extend(open_steps(h, json!([row.clone()])));
        steps.extend(opening_steps(h));
        steps.extend(snapshot_steps(h, &row));
        steps.extend(closing_steps(h));
    });
    let (mut app, _) = owner(&mut s);
    let intent = ScopedReviewIntentV2::capture(&app).unwrap();
    let draft = open(&s, intent.clone()).unwrap();
    let selection = ScopedReviewSelectionV2::read(
        &draft,
        0,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let approve = ReviewActionBaseV12::review(&intent, Some(&selection), Action::Approve).unwrap();
    let comment =
        ReviewActionBaseV12::review(&intent, Some(&selection), Action::SubmitComment).unwrap();
    let changes =
        ReviewActionBaseV12::review(&intent, Some(&selection), Action::RequestChanges).unwrap();
    assert_ne!(approve.key(), comment.key());
    assert_ne!(comment.key(), changes.key());
    let target = PrWriteTarget::new(draft.identity(), intent.parent().summary()).unwrap();
    let write = crate::pr_write_journal::PrWriteIntent {
        command: Action::Approve.command().into(),
        selected_resource: format!("review/{}", selection.snapshot().review().id()),
        observed_head: intent.parent().head_sha().into(),
        values: std::collections::BTreeMap::from([(
            "summary".into(),
            "LEGACY protected local request".into(),
        )]),
    };
    let path = database(&s.fixture);
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let permit = journal.begin(&target, &write).unwrap();
    journal.claim(&permit, &target, &write).unwrap();
    journal.mark_uncertain(&permit).unwrap();
    let encoded=json!({"version":3,"values":{"summary":"LEGACY protected local request","comments":[]},"publication":permit.marker().publication_reference()}).to_string();
    Cache::open(&path)
        .unwrap()
        .put_draft(target.scope(), approve.key(), &encoded)
        .unwrap();
    let mut db = rusqlite::Connection::open(&path).unwrap();
    let tx = db.transaction().unwrap();
    tx.execute(
        "UPDATE pr_write_attempts SET state='acknowledged' WHERE attempt_id=?1",
        [permit.marker().attempt_id],
    )
    .unwrap();
    tx.execute("UPDATE pr_write_scopes SET generation=generation+1", [])
        .unwrap();
    tx.commit().unwrap();
    let revision = journal.action_revision_v12(&approve).unwrap().unwrap();
    assert!(journal.action_revision_v12(&comment).unwrap().is_none());
    assert!(journal.action_revision_v12(&changes).unwrap().is_none());
    use images::reviews_v2::{
        action_lineage_v12::{ActionContextV12, NeedsNewActionV12},
        browser_v2::{self, ReviewBrowserActionV2 as BrowserAction, ReviewBrowserV2},
        native_v2,
    };
    let display = images::reviews_v2::action_lineage_v12::prepare_notice(
        &intent,
        Action::Approve.label(),
        draft.identity(),
        &revision,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let notice = NeedsNewActionV12::prepare(
        ActionContextV12::Review {
            draft: draft.clone(),
            selection: Some(selection),
            action: Action::Approve,
        },
        revision,
        display,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let mut browser = ReviewBrowserV2::prepare(
        draft.clone(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(browser.install_new_action_v12(notice));
    let paint = |view: &mut ReviewBrowserV2, width, height| {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| {
                browser_v2::draw(
                    frame,
                    frame.area(),
                    view,
                    crate::theme::Theme::from_settings(0, false),
                    false,
                    &Default::default(),
                )
            })
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    };
    assert!(paint(&mut browser, 40, 14).contains("Resize to inspect New action"));
    assert!(!browser.interactive());
    let visible = paint(&mut browser, 80, 24);
    assert!(visible.contains("[N New action]"));
    assert!(visible.contains("[Esc Back]"));
    assert!(!visible.contains("LEGACY protected local request"));
    assert!(browser.interactive());
    let click = (0..24)
        .find_map(|row| {
            (0..80).find_map(|column| {
                let event = crossterm::event::MouseEvent {
                    kind: crossterm::event::MouseEventKind::Down(
                        crossterm::event::MouseButton::Left,
                    ),
                    row,
                    column,
                    modifiers: crossterm::event::KeyModifiers::empty(),
                };
                (browser_v2::mouse(&browser, event) == Some(BrowserAction::NewActionV12))
                    .then_some(event)
            })
        })
        .expect("Actual New control has a mouse hit at useful width");
    assert_eq!(
        browser_v2::mouse(&browser, click),
        Some(BrowserAction::NewActionV12)
    );
    browser.apply(BrowserAction::Focus(1));
    assert_eq!(
        browser.apply(BrowserAction::Activate),
        Some(BrowserAction::Close)
    );
    browser.apply(BrowserAction::Focus(-1));
    assert_eq!(
        browser.apply(BrowserAction::Activate),
        Some(BrowserAction::NewActionV12)
    );
    app.pr_file_images = None;
    app.scoped_reviews_v2 = Some(browser);
    app.overlay = Some(crate::app::Overlay::ScopedReviewsV2);
    native_v2::browser_action(&mut app, BrowserAction::NewActionV12);
    let work = app
        .scoped_reviews_work_v2
        .take()
        .expect("Actual owned New queues SAME Review work");
    assert!(work.matches(&app));
    app.form = Some(
        crate::forms::Form::pr(crate::forms::PrIntent {
            kind: crate::forms::PrFormKind::Comment,
            target: intent.parent().summary().clone(),
            identity: draft.identity().clone(),
        })
        .unwrap(),
    );
    app.overlay = Some(crate::app::Overlay::Form);
    assert!(
        !work.matches(&app),
        "Genuine newer Form cannot be replaced by old New completion"
    );
    assert_eq!(
        Cache::open(&path)
            .unwrap()
            .draft(target.scope(), approve.key())
            .unwrap(),
        Some(encoded)
    );
    assert_eq!(commands(&s.fixture).len(), s.steps.len());
    assert_eq!(git_state_resource(&s.fixture), s.git_before);
}
#[test]
fn pending_reviews_v12_cancelled_or_expired_notice_does_not_prepare_or_queue_new_action() {
    use images::reviews_v2::action_lineage_v12::{ActionContextV12, NeedsNewActionV12};
    let real = real(false);
    acknowledge_sql(&real, &real.reference);
    let revision = revision(&real);
    let display = images::reviews_v2::action_lineage_v12::prepare_notice(
        real.source.intent().original(),
        real.source.intent().action().label(),
        real.source.identity(),
        &revision,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let before = (sql_state(&real.s.fixture), journal_state(&real));
    let count = commands(&real.s.fixture).len();
    for expired in [false, true] {
        let cancel = crate::providers::runner::CancellationToken::default();
        if !expired {
            cancel.cancel();
        }
        assert!(NeedsNewActionV12::prepare(
            ActionContextV12::Operation(real.source.clone()),
            revision.clone(),
            display.clone(),
            &cancel,
            if expired {
                Instant::now() - Duration::from_millis(1)
            } else {
                Instant::now() + Duration::from_secs(30)
            }
        )
        .is_err());
    }
    assert_eq!((sql_state(&real.s.fixture), journal_state(&real)), before);
    assert_eq!(commands(&real.s.fixture).len(), count);
    unchanged(&real);
}
fn unchanged(real: &LineageReal) {
    assert_eq!(git_state_resource(&real.s.fixture), real.s.git_before);
    assert_eq!(
        fs::read(real.s.fixture.repo().join("LOCAL_DIRTY_CANARY")).unwrap(),
        b"LOCAL_WORKTREE_UNTOUCHED\0\xff\n"
    );
    let calls = commands(&real.s.fixture);
    assert_eq!(calls.len(), real.s.steps.len());
    for (actual, step) in calls.iter().zip(&real.s.steps) {
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(step.args));
        assert_eq!(
            actual["stdin_hex"],
            step.stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
    let current: Vec<_> = sql_state(&real.s.fixture)
        .into_iter()
        .filter(|row| {
            let value: Value = serde_json::from_str(row).unwrap();
            !value[2].as_str().is_some_and(|key| {
                key == real.base.key() || key.starts_with(&format!("{}/new-v12/", real.base.key()))
            })
        })
        .collect();
    assert_eq!(
        current, real.s.sql_before,
        "All independent unrelated drafts/checkpoints/cache/private canaries unchanged"
    );
}
