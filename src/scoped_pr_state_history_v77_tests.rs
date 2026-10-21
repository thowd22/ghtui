//! Actual claimed mutation failure, genuine fresh State inspection and SAME
//! central raw ACK transaction. No fabricated source/receipt/DTO permission.
use super::*;
use crate::{
    app::{Action, View},
    drafts::DraftWriter,
    forms::FormAction,
    pr_actions::PrPlan,
    pr_write_journal::{
        state_origin_v68::StateOriginSeedV68, PrPublicationReference, PrWriteJournal, PrWriteState,
        PrWriteTarget,
    },
    runtime::Background,
    scoped_pr_collaboration_native_v17 as native,
    scoped_pr_collaboration_source_v17::state_phase_v68::{
        StatePhaseRequestV68, StatePublicationAckV68,
    },
    scoped_pr_collaboration_ui_v17::state_v68::history_v74 as history,
};
type Sql = BTreeMap<String, Vec<Vec<rusqlite::types::Value>>>;
struct Unknown {
    s: Setup,
    writer: DraftWriter,
    reference: PrPublicationReference,
    target: PrWriteTarget,
    key: String,
    protected: String,
}
fn sql_rows(s: &Setup) -> Sql {
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([],|r|r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    let mut result = BTreeMap::new();
    for name in names {
        let mut statement = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = statement.column_count();
        let mut rows = statement
            .query_map([], |r| {
                (0..columns)
                    .map(|i| r.get::<_, rusqlite::types::Value>(i))
                    .collect::<Result<Vec<_>, _>>()
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        rows.sort_by_cached_key(|r| format!("{r:?}"));
        result.insert(name, rows);
    }
    result
}
fn recorded_calls(s: &Setup) -> Vec<Value> {
    let raw = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let end = raw.iter().rposition(|b| *b == b'\n').map_or(0, |n| n + 1);
    raw[..end]
        .split(|b| *b == b'\n')
        .filter(|s| !s.is_empty())
        .map(|s| serde_json::from_slice(s).unwrap())
        .collect()
}
fn assert_effects(unknown: &Unknown, through: usize) {
    let rows = recorded_calls(&unknown.s);
    assert_eq!(rows.len(), through);
    let mut mutations = 0;
    for (ordinal, (actual, expected)) in rows.iter().zip(&unknown.s.steps).enumerate() {
        assert_eq!(actual["step"], ordinal);
        assert_eq!(actual["matched"], true);
        assert_eq!(actual["args"], json!(expected.args));
        assert_eq!(
            actual["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
        mutations += usize::from(expected.stdin.windows(8).any(|v| v == b"mutation"));
    }
    assert_eq!(
        mutations, 1,
        "No retry/ACK/New action resends the protected original"
    );
    assert_eq!(
        git(&unknown.s.fixture, &["rev-parse", "HEAD"]),
        unknown.s.git[0]
    );
    for (path, expected) in [
        (".git/index", &unknown.s.git[1]),
        (".git/config", &unknown.s.git[2]),
        ("kept", &unknown.s.git[3]),
    ] {
        assert_eq!(
            &fs::read(unknown.s.fixture.repo().join(path)).unwrap(),
            expected
        );
    }
    let cache = Cache::open(&database(&unknown.s.fixture)).unwrap();
    assert_eq!(
        cache
            .draft(unknown.target.scope(), &unknown.key)
            .unwrap()
            .as_deref(),
        Some(unknown.protected.as_str())
    );
    let db = rusqlite::Connection::open(database(&unknown.s.fixture)).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_state_dispositions_v68",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_state_source_watermarks_v73",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        0
    );
}
fn unknown() -> Unknown {
    unknown_with_inspect_hold(None)
}
fn unknown_with_inspect_hold(ordinal: Option<usize>) -> Unknown {
    let mut steps = phase(capability(json!(true)));
    steps.extend(phase(capability(json!(true))));
    let mut mutation = super::receipt_v68::receipt_steps().remove(0);
    mutation.stdout.clear();
    mutation.status = 1;
    mutation.stderr = b"gh: service unavailable (HTTP 503)\n".to_vec();
    steps.push(mutation);
    steps.extend(phase(capability(json!(true)))); // actual History open10
    steps.extend(phase(capability(json!(true)))); // separate actual Inspect10
    if let Some(ordinal) = ordinal {
        assert!(ordinal < 10);
        let index = steps.len() - 10 + ordinal;
        steps[index].delay_ms = 1000;
    }
    let s = setup(steps);
    let opening = PrCollaborationOpenIntentV17::capture_state_v68(
        &s.app,
        &database(&s.fixture),
        PrStateCommand::Close,
    )
    .unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(opening.deadline(), 1024 * 1024)
        .unwrap();
    let (source, _, base) = source::state_v68::open_state_v68(
        source::state_v68::StateOpenRequestV68 {
            opening: Arc::clone(&opening),
            command: PrStateCommand::Close,
        },
        &provider,
        &database(&s.fixture),
        opening.cancellation(),
        opening.deadline(),
        |_, _, base| Ok(base.unwrap().clone()),
        |_| Ok(()),
    )
    .unwrap();
    let key = base.key().to_owned();
    let mut writer = DraftWriter::new(database(&s.fixture));
    super::receipt_v68::saved_bodyless(&mut writer, &s, &key);
    let client = writer.client();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    let fence = client
        .capture_publication_v2(
            &database(&s.fixture),
            opening.scope(),
            &key,
            &cancel,
            deadline,
        )
        .unwrap();
    let (reference, target, protected) = source
        .with_state_phase_v68(
            StatePhaseRequestV68 {
                provider: &provider,
                database: &database(&s.fixture),
                cancel: &cancel,
                deadline,
                command: PrStateCommand::Close,
                postwrite: true,
            },
            |mut phase| {
                let actual = Arc::clone(phase.actual());
                let local = phase.local(&database(&s.fixture))?;
                let seed =
                    StateOriginSeedV68::from_producer(&source, &actual, &database(&s.fixture))
                        .unwrap();
                let write = seed.write(&BTreeMap::new()).unwrap();
                let mut journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
                let permit = journal
                    .begin_state_v68(local.target_v17(), &write, &seed)
                    .unwrap();
                let reference = permit.marker().publication_reference();
                let target = permit.marker().target.clone();
                let protected = crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(
                    &BTreeMap::new(),
                    Some(&reference),
                )
                .unwrap();
                let begun = local.generation_v17() + 1;
                let claimed = begun + 1;
                let ack = PrPlan::claim_persisted_v17(
                    &mut journal,
                    &permit,
                    local.target_v17(),
                    &write,
                    |marker| {
                        let ack = client
                            .save_publication_and_wait_v2(fence, protected.clone(), &cancel)
                            .map_err(|e| e.to_string())?;
                        local
                            .validate_owned_boundary_v17(
                                &database(&s.fixture),
                                source.local_boundary(),
                                marker,
                                begun,
                                PrWriteState::Pending,
                                None,
                            )
                            .map_err(|e| e.to_string())?;
                        ack.check_current(
                            &client,
                            &database(&s.fixture),
                            opening.scope(),
                            &key,
                            &cancel,
                        )?;
                        Ok(ack)
                    },
                )
                .unwrap();
                let mut captures = phase.take_captures()?;
                let failure = phase
                    .publish(
                        &provider,
                        &mut captures,
                        StatePublicationAckV68 {
                            permit: &permit,
                            ack: &ack,
                            drafts: &client,
                            key: &key,
                        },
                        || {
                            local.validate_owned_boundary_v17(
                                &database(&s.fixture),
                                source.local_boundary(),
                                permit.marker(),
                                claimed,
                                PrWriteState::Pending,
                                None,
                            )
                        },
                    )
                    .err()
                    .expect("Real invoked mutation failure, not a fake terminal DTO");
                assert!(matches!(
                    failure,
                    crate::providers::gh::StatePublicationFailureV68::WriteMayHaveStarted(_)
                ));
                assert_eq!(captures.consumed_requests(), 1);
                journal.mark_uncertain(&permit).unwrap();
                assert_eq!(journal.source_generation(&target).unwrap(), 3);
                assert_eq!(
                    journal
                        .blocking(&target)
                        .unwrap()
                        .unwrap()
                        .publication_reference(),
                    reference
                );
                Ok((reference, target, protected))
            },
        )
        .unwrap();
    let unknown = Unknown {
        s,
        writer,
        reference,
        target,
        key,
        protected,
    };
    assert_effects(&unknown, unknown.s.steps.len() - 20);
    unknown
}
fn history_ready(unknown: &mut Unknown) -> Background {
    let opening = PrCollaborationOpenIntentV17::capture_state_v68(
        &unknown.s.app,
        &database(&unknown.s.fixture),
        PrStateCommand::Close,
    )
    .unwrap();
    let provider = unknown
        .s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(opening.deadline(), 1024 * 1024)
        .unwrap();
    let form = history::open(
        source::state_v68::StateOpenRequestV68 {
            opening: Arc::clone(&opening),
            command: PrStateCommand::Close,
        },
        &provider,
        &database(&unknown.s.fixture),
        opening.cancellation(),
        opening.deadline(),
        None,
    )
    .unwrap();
    assert_eq!(history::state(&form).unwrap().view.snapshot.rows.len(), 1);
    assert!(!history::ack_ready(&form));
    install_form(&mut unknown.s, form);
    paint(&mut unknown.s.app, 120, 40);
    assert_effects(unknown, unknown.s.steps.len() - 10);
    Background::new()
}
fn inspected(unknown: &mut Unknown) -> Background {
    let mut background = history_ready(unknown);
    let mut expected_sql = sql_rows(&unknown.s);
    let own = expected_sql.get_mut("sync_reservations").unwrap().iter_mut()
        .filter(|r| matches!(&r[2],rusqlite::types::Value::Text(s) if s.starts_with("personal-pr-source-v13/")))
        .collect::<Vec<_>>();
    assert_eq!(
        own.len(),
        1,
        "One actual Personal PR-read source, separate from original queue"
    );
    let own = own.into_iter().next().unwrap();
    assert_eq!(own[3], rusqlite::types::Value::Integer(2));
    assert_eq!(own[4], rusqlite::types::Value::Integer(1));
    own[3] = rusqlite::types::Value::Integer(3);
    run(
        &mut unknown.s,
        &mut background,
        &unknown.writer,
        FormAction::Preview,
    );
    assert!(history::state(unknown.s.app.form.as_ref().unwrap())
        .unwrap()
        .view
        .current
        .is_some());
    assert!(
        !history::ack_ready(unknown.s.app.form.as_ref().unwrap()),
        "Fresh inspection is not useful until its own frame is painted"
    );
    paint(&mut unknown.s.app, 120, 40);
    assert!(history::ack_ready(unknown.s.app.form.as_ref().unwrap()));
    assert_eq!(sql_rows(&unknown.s),expected_sql,
        "Healthy Inspect advances ONLY the actual own Personal reservation2→3; original queue boundary stays exact");
    assert_effects(unknown, unknown.s.steps.len());
    background
}
fn run(s: &mut Setup, background: &mut Background, writer: &DraftWriter, action: FormAction) {
    s.app.dispatch(Action::Form(action));
    native::prepare_requested(&mut s.app);
    let work = s
        .app
        .scoped_pr_collaboration_work_v17
        .take()
        .expect("Actual exact Form action requests the original worker");
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    let deadline = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.poll(&mut s.app);
        assert!(
            Instant::now() < deadline,
            "Actual State history/ACK worker did not finish"
        );
        std::thread::yield_now();
    }
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
}
fn exact_blocker(unknown: &Unknown) {
    let journal = PrWriteJournal::open(&database(&unknown.s.fixture)).unwrap();
    let marker = journal.blocking(&unknown.target).unwrap().unwrap();
    assert_eq!(marker.publication_reference(), unknown.reference);
    assert_eq!(marker.state, PrWriteState::Uncertain);
    assert!(marker.confirmed.is_none());
    assert_effects(unknown, unknown.s.steps.len());
}
#[test]
fn pr_lifecycle_v68_legacy_ack_requires_actual_stamp_and_exact_checkpoint_and_never_infers_absence()
{
    // Every original is captured from the real startup->Personal page issuer.
    // Mutation cases below are negative disposable races, never source data.
    for mode in 0..7 {
        let mut unknown = unknown();
        if mode == 0 {
            // Keeps the same actual page but invalidates the producer stamp's
            // generation digest. Missing reservation alone cannot replace it.
            unknown.s.app.github.as_mut().unwrap().generation += 1;
            assert!(unknown
                .s
                .app
                .github
                .as_ref()
                .unwrap()
                .legacy_queue_v80()
                .is_none());
        }
        let original = PrCollaborationOpenIntentV17::capture_state_v68(
            &unknown.s.app,
            &database(&unknown.s.fixture),
            PrStateCommand::Close,
        )
        .unwrap();
        let queue = original.state_cache_source_v78().unwrap();
        if mode == 6 {
            // The original PRESENT branch still accepts only its exact actual
            // normal reservation tuple; no synthetic tuple is used here.
            Cache::open(&database(&unknown.s.fixture))
                .unwrap()
                .reserve_sync_generation(unknown.target.scope(), queue)
                .unwrap();
        }
        let db = rusqlite::Connection::open(database(&unknown.s.fixture)).unwrap();
        let reservation: usize = db
            .query_row(
                "SELECT count(*) FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3",
                rusqlite::params![
                    unknown.target.scope().host,
                    unknown.target.scope().login,
                    queue
                ],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            reservation,
            usize::from(mode == 6),
            "Only the explicit normal reservation case has a reservation"
        );
        let generation: u64 = db
            .query_row(
                "SELECT generation FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
                rusqlite::params![
                    unknown.target.scope().host,
                    unknown.target.scope().login,
                    queue
                ],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(generation, 1);
        assert_eq!(
            original.matches_state_legacy_checkpoint_v95(generation),
            mode != 0
        );
        if mode == 1 {
            assert_eq!(db.execute(
                "UPDATE checkpoints SET generation=generation+1 WHERE host=?1 AND login=?2 AND source=?3",
                rusqlite::params![unknown.target.scope().host, unknown.target.scope().login, queue],
            ).unwrap(), 1);
        } else if mode == 2 {
            assert_eq!(
                db.execute(
                    "DELETE FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3",
                    rusqlite::params![
                        unknown.target.scope().host,
                        unknown.target.scope().login,
                        queue
                    ],
                )
                .unwrap(),
                1
            );
        }
        let journal = PrWriteJournal::open(&database(&unknown.s.fixture)).unwrap();
        let before_capture = sql_rows(&unknown.s);
        let witness =
            journal.prepare_state_inspection_v74(&unknown.target, &unknown.reference, &original);
        assert_eq!(
            sql_rows(&unknown.s),
            before_capture,
            "Capture never writes or repairs"
        );
        if mode < 3 {
            assert!(
                witness.is_err(),
                "Missing genuine stamp/checkpoint or changed generation must refuse"
            );
        } else {
            let guard = witness.unwrap().cache_guard_v78().unwrap();
            guard
                .validate_at_database_v78(&database(&unknown.s.fixture))
                .unwrap();
            if mode == 3 {
                assert_eq!(db.execute(
                    "UPDATE checkpoints SET fetched_at=fetched_at+1 WHERE host=?1 AND login=?2 AND source=?3",
                    rusqlite::params![unknown.target.scope().host, unknown.target.scope().login, queue],
                ).unwrap(), 1);
            } else if mode == 4 {
                Cache::open(&database(&unknown.s.fixture))
                    .unwrap()
                    .reserve_sync_generation(unknown.target.scope(), queue)
                    .unwrap();
            }
            let after_race = sql_rows(&unknown.s);
            if mode == 5 {
                let other = unknown.s.fixture.root.join("different-state-cache.sqlite3");
                Cache::open(&other).unwrap();
                assert!(guard.validate_at_database_v78(&other).is_err());
                guard
                    .validate_at_database_v78(&database(&unknown.s.fixture))
                    .unwrap();
            } else if mode < 5 {
                assert!(
                    guard
                        .validate_at_database_v78(&database(&unknown.s.fixture))
                        .is_err(),
                    "Exact None reservation and complete checkpoint tuple cannot advance"
                );
            } else {
                guard
                    .validate_at_database_v78(&database(&unknown.s.fixture))
                    .unwrap();
            }
            assert_eq!(sql_rows(&unknown.s), after_race);
        }
        assert_effects(&unknown, unknown.s.steps.len() - 20);
        let blocker = journal.blocking(&unknown.target).unwrap().unwrap();
        assert_eq!(blocker.publication_reference(), unknown.reference);
        assert_eq!(blocker.state, PrWriteState::Uncertain);
        assert!(blocker.confirmed.is_none());
    }
}
/// TASK-25 AC6: the real State history form keeps every state's text label
/// and draws it in the shared semantic ink: the fresh PR state in lifecycle
/// ink, the uncertain attempt in Conflict ink and its head in Head ink.
#[test]
fn pr_lifecycle_v68_state_history_rows_keep_labels_in_shared_semantic_ink() {
    use crate::theme::SemanticToken;
    let mut unknown = unknown();
    let _background = history_ready(&mut unknown);
    unknown.s.app.no_color = false;
    unknown.s.app.color_mode = crate::theme::ColorMode::TrueColor;
    let theme = unknown.s.app.theme();
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut unknown.s.app))
        .unwrap();
    let buffer = terminal.backend().buffer().clone();
    let ink = |label: &str| {
        (0..buffer.area.height)
            .find_map(|y| {
                let row: String = (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_owned())
                    .collect();
                let at = row.find(label)?;
                let x = row[..at].chars().count() as u16;
                Some(
                    (0..label.chars().count() as u16)
                        .map(|offset| buffer[(x + offset, y)].fg)
                        .collect::<Vec<_>>(),
                )
            })
            .unwrap_or_else(|| panic!("{label:?} not drawn"))
    };
    for (label, token) in [
        ("Fresh observed state open", SemanticToken::Added),
        ("Attempt #", SemanticToken::Conflict),
        (" • Uncertain • ", SemanticToken::Conflict),
        ("Historical head ", SemanticToken::Head),
    ] {
        let wanted = theme.style(token).fg.unwrap();
        assert!(
            ink(label).iter().all(|fg| *fg == wanted),
            "{label:?} is not drawn in {token:?} ink"
        );
    }
    // Rows that name no state keep the plain base ink.
    assert_ne!(
        ink("Current observation is not exclusive")[0],
        theme.style(SemanticToken::Conflict).fg.unwrap()
    );
    // Painting reads nothing: still only the history-open reads, no write.
    assert_effects(&unknown, unknown.s.steps.len() - 10);
}
#[test]
fn pr_lifecycle_v68_real_unknown_then_fresh_inspect_local_ack_changes_only_exact_rows_without_source_or_hydration(
) {
    use rusqlite::types::Value as SqlValue;
    let mut unknown = unknown();
    let mut background = inspected(&mut unknown);
    let mut expected = sql_rows(&unknown.s);
    let attempts = expected.get_mut("pr_write_attempts").unwrap();
    assert_eq!(attempts.len(), 1);
    assert_eq!(
        attempts[0][0],
        SqlValue::Integer(unknown.reference.attempt_id)
    );
    assert_eq!(attempts[0][10], SqlValue::Text("uncertain".into()));
    assert_eq!(attempts[0][11], SqlValue::Null);
    attempts[0][10] = SqlValue::Text("acknowledged".into());
    let scopes = expected.get_mut("pr_write_scopes").unwrap();
    assert_eq!(scopes.len(), 1);
    assert_eq!(scopes[0][3], SqlValue::Integer(3));
    scopes[0][3] = SqlValue::Integer(4);
    run(
        &mut unknown.s,
        &mut background,
        &unknown.writer,
        FormAction::Submit,
    );
    assert_eq!(
        sql_rows(&unknown.s),
        expected,
        "Full raw SQL, only independently known ACK state+generation change"
    );
    assert!(unknown.s.app.form.is_none() && unknown.s.app.overlay.is_none());
    let workspace = unknown.s.app.github.as_ref().unwrap();
    assert!(
        workspace.pr.is_none()
            && workspace.personal_pr_page_ticket_v13.is_none()
            && workspace.notification_pr_page_ticket_v63.is_none()
            && workspace.scoped_pr_navigation.is_none()
            && workspace.scoped_pr_resource.is_none()
    );
    assert!(workspace.state_membership_unavailable_v68() && !workspace.inbox.coverage.complete);
    assert!(unknown.s.app.status.contains("LOCAL State ACK"));
    let journal = PrWriteJournal::open(&database(&unknown.s.fixture)).unwrap();
    assert!(journal.blocking(&unknown.target).unwrap().is_none());
    let old = journal
        .original_publication(&unknown.target, &unknown.reference)
        .unwrap()
        .unwrap();
    assert_eq!(old.state, PrWriteState::Acknowledged);
    assert!(old.confirmed.is_none());
    assert_effects(&unknown, unknown.s.steps.len());
}
#[test]
fn pr_lifecycle_v68_useful_state_inspection_resize_and_bodyless_field_graft_cannot_queue_ack() {
    let mut unknown = unknown();
    let mut background = inspected(&mut unknown);
    let sql = sql_rows(&unknown.s);
    let request = history::request(unknown.s.app.form.as_ref().unwrap()).unwrap();
    paint(&mut unknown.s.app, 40, 10);
    assert!(!history::ack_ready(unknown.s.app.form.as_ref().unwrap()));
    unknown.s.app.dispatch(Action::Form(FormAction::Submit));
    native::prepare_requested(&mut unknown.s.app);
    assert!(unknown.s.app.scoped_pr_collaboration_work_v17.is_none());
    paint(&mut unknown.s.app, 120, 40);
    assert!(history::ack_ready(unknown.s.app.form.as_ref().unwrap()));
    // Queue admission must repeat useful geometry after preparation. This is
    // a genuinely prepared exact ACK work item, not a mocked result handler.
    unknown.s.app.dispatch(Action::Form(FormAction::Submit));
    native::prepare_requested(&mut unknown.s.app);
    let prepared = unknown
        .s
        .app
        .scoped_pr_collaboration_work_v17
        .take()
        .unwrap();
    assert!(prepared.matches(&unknown.s.app));
    paint(&mut unknown.s.app, 40, 10);
    assert!(!prepared.matches(&unknown.s.app));
    assert!(!background.submit_scoped_pr_collaboration_v17(
        &mut unknown.s.app,
        prepared,
        unknown.s.api.clone(),
        database(&unknown.s.fixture),
        unknown.writer.client()
    ));
    assert!(unknown.s.app.scoped_pr_collaboration_owner_v17.is_none());
    assert!(!unknown.s.app.loading[View::Inbox.index()]);
    assert!(crate::runtime::scoped_inline_test_support_v1::generation(&background).is_none());
    assert_eq!(sql_rows(&unknown.s), sql);
    paint(&mut unknown.s.app, 120, 40);
    let form = unknown.s.app.form.as_mut().unwrap();
    form.fields.push(
        crate::forms::Field::text("grafted", "Foreign input", "must remain private", false)
            .unwrap(),
    );
    assert!(!request.matches(form));
    unknown.s.app.dispatch(Action::Form(FormAction::Submit));
    native::prepare_requested(&mut unknown.s.app);
    assert!(unknown.s.app.scoped_pr_collaboration_work_v17.is_none());
    assert_eq!(sql_rows(&unknown.s), sql);
    exact_blocker(&unknown);
}
#[test]
fn pr_lifecycle_v68_actual_clear_after_state_inspection_refuses_same_ack_transaction_and_preserves_original(
) {
    let mut unknown = unknown();
    let mut background = inspected(&mut unknown);
    Cache::open(&database(&unknown.s.fixture))
        .unwrap()
        .clear_records(unknown.target.scope())
        .unwrap();
    let after_clear = sql_rows(&unknown.s);
    run(
        &mut unknown.s,
        &mut background,
        &unknown.writer,
        FormAction::Submit,
    );
    assert_eq!(
        sql_rows(&unknown.s),
        after_clear,
        "No ACK, repair, ref rewrite or source hydration after actual Clear"
    );
    exact_blocker(&unknown);
}
#[test]
fn pr_lifecycle_v68_exact_raw_timestamp_drift_after_state_inspection_cannot_release_uncertain_attempt(
) {
    let mut unknown = unknown();
    let mut background = inspected(&mut unknown);
    let db = rusqlite::Connection::open(database(&unknown.s.fixture)).unwrap();
    assert_eq!(
        db.execute(
            "UPDATE pr_write_attempts SET created_at=created_at+1 WHERE attempt_id=?1",
            [unknown.reference.attempt_id]
        )
        .unwrap(),
        1
    );
    let hostile = sql_rows(&unknown.s);
    run(
        &mut unknown.s,
        &mut background,
        &unknown.writer,
        FormAction::Submit,
    );
    assert_eq!(
        sql_rows(&unknown.s),
        hostile,
        "Exact created_at CAS, no silent repair or permission"
    );
    exact_blocker(&unknown);
}

#[test]
fn pr_lifecycle_v68_original_pre_core_queue_clear_refuses_before_any_child_or_personal_reservation()
{
    let mut unknown = unknown();
    let _background = history_ready(&mut unknown);
    let request = history::request(unknown.s.app.form.as_ref().unwrap()).unwrap();
    let original = Arc::clone(&request.view.original);
    let journal = PrWriteJournal::open(&database(&unknown.s.fixture)).unwrap();
    let witness = journal
        .prepare_state_inspection_v74(&unknown.target, &unknown.reference, &original)
        .unwrap();
    let guard = witness.cache_guard_v78().unwrap();
    Cache::open(&database(&unknown.s.fixture))
        .unwrap()
        .clear_records(unknown.target.scope())
        .unwrap();
    let after_clear = sql_rows(&unknown.s);
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(8);
    let provider = unknown
        .s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
        .unwrap();
    assert!(source::state_v68::with_history_v74(
        source::state_v68::StateHistoryReadV78 {
            request: source::state_v68::StateOpenRequestV68 {
                opening: original,
                command: PrStateCommand::Close,
            },
            inspection_cache: Some(&guard),
        },
        &provider,
        &database(&unknown.s.fixture),
        &cancel,
        deadline,
        |_, _| -> crate::providers::gh::GithubResult<()> {
            panic!("Clear cannot reach read preparation or issue an own reservation");
        },
        |_, _| Ok(()),
    )
    .is_err());
    assert_eq!(sql_rows(&unknown.s), after_clear);
    assert_effects(&unknown, unknown.s.steps.len() - 10);
    let marker = journal.blocking(&unknown.target).unwrap().unwrap();
    assert_eq!(marker.publication_reference(), unknown.reference);
    assert_eq!(marker.state, PrWriteState::Uncertain);
    assert!(marker.confirmed.is_none());
}

struct HeldInspectChild(crate::owned_pty_processes_test_support::ProcessIdentity);
impl HeldInspectChild {
    fn stop(pid: i32) -> Self {
        use crate::owned_pty_processes_test_support as owned;
        let parent = owned::process_identity(i32::try_from(std::process::id()).unwrap())
            .unwrap()
            .unwrap();
        let child = owned::process_identity(pid).unwrap().unwrap();
        assert!(pid > 0 && pid != parent.pid && !child.zombie);
        assert_eq!(
            (child.parent, child.uid, child.session),
            (parent.pid, parent.uid, parent.session)
        );
        assert!(child.birth >= parent.birth);
        assert_eq!(owned::process_identity(pid).unwrap().as_ref(), Some(&child));
        // SAFETY: exact positive direct child birth/UID/session rechecked.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        Self(child)
    }
}
impl Drop for HeldInspectChild {
    fn drop(&mut self) {
        if let Ok(Some(now)) = crate::owned_pty_processes_test_support::process_identity(self.0.pid)
        {
            if !now.zombie
                && (now.pid, now.uid, now.session, now.birth)
                    == (self.0.pid, self.0.uid, self.0.session, self.0.birth)
            {
                // SAFETY: resume only same owned child, including unwinding.
                assert_eq!(unsafe { libc::kill(now.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}
fn held_inspection_clear(ordinal: usize) {
    let mut unknown = unknown_with_inspect_hold(Some(ordinal));
    let mut background = history_ready(&mut unknown);
    unknown.s.app.dispatch(Action::Form(FormAction::Preview));
    native::prepare_requested(&mut unknown.s.app);
    let work = unknown
        .s
        .app
        .scoped_pr_collaboration_work_v17
        .take()
        .unwrap();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut unknown.s.app,
        work,
        unknown.s.api.clone(),
        database(&unknown.s.fixture),
        unknown.writer.client(),
    ));
    let base = unknown.s.steps.len() - 10;
    let end = Instant::now() + Duration::from_secs(8);
    let held = loop {
        let rows = recorded_calls(&unknown.s);
        if let Some(row) = rows.get(base + ordinal) {
            assert_eq!(row["matched"], true);
            assert_eq!(row["args"], json!(unknown.s.steps[base + ordinal].args));
            break HeldInspectChild::stop(i32::try_from(row["pid"].as_u64().unwrap()).unwrap());
        }
        assert!(
            Instant::now() < end,
            "Actual owned Inspect boundary child never started"
        );
        std::thread::yield_now();
    };
    let identity = held.0.clone();
    Cache::open(&database(&unknown.s.fixture))
        .unwrap()
        .clear_records(unknown.target.scope())
        .unwrap();
    let after_clear = sql_rows(&unknown.s);
    drop(held);
    while unknown.s.app.loading[View::Inbox.index()] {
        background.poll(&mut unknown.s.app);
        assert!(
            Instant::now() < end,
            "Actual held Inspect worker did not finish within original bound"
        );
        std::thread::yield_now();
    }
    loop {
        match crate::owned_pty_processes_test_support::process_identity(identity.pid) {
            Ok(None) => break,
            Ok(Some(now)) if now.birth != identity.birth => break,
            _ => {}
        }
        assert!(
            Instant::now() < end,
            "Exact original Inspect child absence unproven"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        sql_rows(&unknown.s),
        after_clear,
        "No own reservation, ACK, repair, draft or source change after Clear"
    );
    if ordinal == 0 {
        // Version/principal/repo/rawPR/independent issue are observed. The
        // original cache guard runs BEFORE the prepare callback's GraphQL
        // child and BEFORE any own Personal reservation/closing work.
        assert_effects(&unknown, base + 5);
        assert!(unknown.s.app.form.is_none());
    } else {
        // Final UID is the last child; no SQL/read is appended after close.
        // Even if a readonly inspection was prepared, SAME central ACK must
        // refuse the original invalidated queue boundary without any resend.
        assert_eq!(ordinal, 9);
        assert_effects(&unknown, unknown.s.steps.len());
        paint(&mut unknown.s.app, 120, 40);
        assert!(history::state(unknown.s.app.form.as_ref().unwrap())
            .unwrap()
            .view
            .current
            .is_some());
        assert!(
            history::ack_ready(unknown.s.app.form.as_ref().unwrap()),
            "Final UID prepared a readonly useful observation, not a cache/ACK permission"
        );
        run(
            &mut unknown.s,
            &mut background,
            &unknown.writer,
            FormAction::Submit,
        );
        assert_eq!(sql_rows(&unknown.s), after_clear);
    }
    let journal = PrWriteJournal::open(&database(&unknown.s.fixture)).unwrap();
    let blocker = journal.blocking(&unknown.target).unwrap().unwrap();
    assert_eq!(blocker.publication_reference(), unknown.reference);
    assert_eq!(blocker.state, PrWriteState::Uncertain);
    assert!(blocker.confirmed.is_none());
}
#[test]
fn pr_lifecycle_v68_clear_while_actual_opening_core_held_cannot_advance_own_personal_read_reservation(
) {
    held_inspection_clear(0);
}
#[test]
fn pr_lifecycle_v68_clear_while_actual_final_uid_held_cannot_release_exact_state_ack_or_hydrate() {
    held_inspection_clear(9);
}
