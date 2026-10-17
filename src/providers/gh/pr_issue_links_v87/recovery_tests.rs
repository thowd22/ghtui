//! Actual producer/SQLite/worker inspections, not rows posing as permission.
use super::*;
use crate::pr_write_journal::issue_link_origin_v87::history_v87::IssueLinkRecoveryWitnessV87;
use crate::{
    drafts::DraftWriter,
    pr_write_journal::{PrWriteJournal, PrWriteState, PrWriteTarget},
    scoped_pr_collaboration_source_v17::issue_links_phase_v87::IssueLinkHistoryOpenV87,
    scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard,
};
use rusqlite::{params, Connection};

struct UncertainV87 {
    s: personal::Setup,
    source: Arc<crate::scoped_pr_collaboration_source_v17::PrCollaborationSourceV17>,
    target: PrWriteTarget,
    reference: crate::pr_write_journal::PrPublicationReference,
    body: String,
    writer: DraftWriter,
}
fn uncertain() -> UncertainV87 {
    uncertain_with_tail(selected_steps(pair(pr_node(), issue(9))))
}
fn uncertain_with_tail(tail: Vec<GhStep>) -> UncertainV87 {
    let mut calls = open_steps(
        catalogue(vec![issue(9)], false, None),
        pr_node(),
        personal::parent(),
    );
    calls.extend(selected_steps(pair(pr_node(), issue(9))));
    calls.extend(tail);
    let s = personal::setup(calls);
    let (source, actual, _, seed) = selected_seed_v87(&s);
    let database = personal::database(&s.fixture);
    let mut journal = PrWriteJournal::open(&database).unwrap();
    let target = PrWriteTarget::new(actual.identity(), actual.parent().summary()).unwrap();
    let values = seed.values().unwrap();
    let write = seed.write(&values).unwrap();
    let permit = journal
        .begin_issue_link_v87(&target, &write, &seed)
        .unwrap();
    journal.claim(&permit, &target, &write).unwrap();
    journal.mark_uncertain(&permit).unwrap();
    let reference = permit.marker().publication_reference();
    let body =
        crate::scoped_pr_collaboration_draft_v17::encode_draft_v17(&values, Some(&reference))
            .unwrap();
    let writer = DraftWriter::new(database);
    writer
        .client()
        .save_and_wait(
            target.scope().clone(),
            seed.draft_key().into(),
            body.clone(),
        )
        .unwrap();
    UncertainV87 {
        s,
        source,
        target,
        reference,
        body,
        writer,
    }
}
fn history_list_steps() -> Vec<GhStep> {
    let selected = selected_steps(pair(pr_node(), issue(9)));
    [0usize, 1, 2, 3, 10, 11, 12, 13]
        .into_iter()
        .map(|i| selected[i].clone())
        .collect()
}
fn inspect(
    f: &UncertainV87,
) -> (
    IssueLinkRecoveryWitnessV87,
    IssueLinkRecoveryCurrentV87,
    CancellationToken,
    Instant,
) {
    let database = personal::database(&f.s.fixture);
    let journal = PrWriteJournal::open(&database).unwrap();
    let witness = journal
        .prepare_issue_link_inspection_v87(&f.target, &f.reference)
        .unwrap();
    let token = CancellationToken::default();
    let deadline = Instant::now() + std::time::Duration::from_secs(30);
    let provider =
        f.s.api
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
            .unwrap();
    let ((current, _, _), _) = issue_links_phase_v87::with_issue_link_history_v87(
        IssueLinkHistoryOpenV87 {
            opening: Arc::clone(f.source.opening()),
            inspect: Some(&witness),
            inherited: f.source.parent().retained_heap_bytes_v2(),
        },
        &provider,
        &database,
        &token,
        deadline,
        |parent, _, boundary, current| {
            let journal = PrWriteJournal::open(&database)
                .map_err(|_| refused(GithubErrorKind::IdentityChanged))?;
            let history = journal
                .issue_link_history_v87(&f.target, parent, IssueLinkCommandV87::LinkIssue)
                .map_err(|_| refused(GithubErrorKind::IdentityChanged))?;
            assert!(history.complete);
            assert_eq!(history.rows.len(), 1);
            assert_eq!(history.rows[0].body.as_ref().unwrap().0, f.body);
            ResourceLocalGuard::validate_issue_link_history_v87(
                &database, boundary, &f.target, parent, &history,
            )?;
            let current = current.ok_or_else(|| refused(GithubErrorKind::IdentityChanged))?;
            assert!(
                current.actual().prepare_requested().is_err(),
                "Inspection cannot issue eligible request"
            );
            assert!(current.matches_witness_v87(&witness));
            Ok((current, history, Arc::clone(parent)))
        },
        |(current, history, parent), boundary| {
            ResourceLocalGuard::validate_issue_link_history_v87(
                &database, boundary, &f.target, parent, history,
            )?;
            PrWriteJournal::open(&database)
                .map_err(|_| refused(GithubErrorKind::IdentityChanged))?
                .validate_issue_link_inspection_v87(&witness)
                .map_err(|_| refused(GithubErrorKind::IdentityChanged))?;
            provider.validate_issue_link_inspection_v87(current, &token, deadline)
        },
    )
    .unwrap();
    provider
        .validate_issue_link_inspection_v87(&current, &token, deadline)
        .unwrap();
    (witness, current, token, deadline)
}
type RawV87 = BTreeMap<String, (Vec<String>, Vec<Vec<rusqlite::types::Value>>)>;
fn dump(db: &Connection) -> RawV87 {
    let tables=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap()
        .query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut result = RawV87::new();
    for name in tables {
        let mut q = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = q
            .column_names()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let mut rows = q
            .query_map([], |r| {
                (0..columns.len())
                    .map(|i| r.get(i))
                    .collect::<rusqlite::Result<Vec<rusqlite::types::Value>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort_by_key(|r| format!("{r:?}"));
        result.insert(name, (columns, rows));
    }
    result
}
fn reference_db(db: &Connection) -> Connection {
    let mut q=db.prepare("SELECT type,name,sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type,name").unwrap();
    let schema = q
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    let raw = dump(db);
    let copy = Connection::open_in_memory().unwrap();
    copy.pragma_update(None, "foreign_keys", false).unwrap();
    for (kind, _, ddl) in &schema {
        if kind == "table" {
            copy.execute_batch(ddl).unwrap();
        }
    }
    for (name, (columns, rows)) in &raw {
        let columns = columns
            .iter()
            .map(|c| format!("\"{}\"", c.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(",");
        for row in rows {
            let placeholders = (1..=row.len())
                .map(|i| format!("?{i}"))
                .collect::<Vec<_>>()
                .join(",");
            copy.execute(
                &format!(
                    "INSERT INTO \"{}\"({columns}) VALUES({placeholders})",
                    name.replace('"', "\"\"")
                ),
                rusqlite::params_from_iter(row),
            )
            .unwrap();
        }
    }
    copy.pragma_update(None, "foreign_keys", true).unwrap();
    assert!(copy
        .prepare("PRAGMA foreign_key_check")
        .unwrap()
        .query([])
        .unwrap()
        .next()
        .unwrap()
        .is_none());
    for (kind, _, ddl) in &schema {
        if kind != "table" {
            copy.execute_batch(ddl).unwrap();
        }
    }
    assert_eq!(dump(&copy), raw);
    copy
}
#[test]
fn pr_metadata_issue_link_v87_actual_inspection_exact_two_version_local_ack_keeps_original_body_and_all_other_sql(
) {
    let f = uncertain();
    let (witness, current, token, deadline) = inspect(&f);
    let database = personal::database(&f.s.fixture);
    let db = Connection::open(&database).unwrap();
    let expected = reference_db(&db);
    let before = dump(&db);
    let expected_reservations = reservations(&f.s);
    let before_schema = schema_sql(&f.s);
    // Independent business oracle: the sole known pending attempt becomes
    // acknowledged and its existing PR generation advances once. NO origin,
    // protected draft, source row, secondary index or foreign scope is omitted.
    assert_eq!(expected.execute("UPDATE pr_write_attempts SET state='acknowledged' WHERE attempt_id=?1 AND state='uncertain'",[f.reference.attempt_id]).unwrap(),1);
    assert_eq!(expected.execute("UPDATE pr_write_scopes SET generation=generation+1 WHERE host=?1 AND principal=?2 AND pr_node=?3",params![f.target.scope().host,f.target.scope().login,f.target.node_id()]).unwrap(),1);
    let mut journal = PrWriteJournal::open(&database).unwrap();
    assert_eq!(
        journal
            .acknowledge_issue_link_inspection_v87(&f.s.api, witness, current, &token, deadline)
            .unwrap(),
        1
    );
    assert_eq!(dump(&db), dump(&expected));
    assert_ne!(dump(&db), before);
    assert!(journal.blocking(&f.target).unwrap().is_none());
    assert_eq!(
        journal
            .original_publication(&f.target, &f.reference)
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Acknowledged
    );
    assert_eq!(reservations(&f.s), expected_reservations);
    assert_eq!(schema_sql(&f.s), before_schema);
    assert_eq!(
        crate::storage::Cache::open(&database)
            .unwrap()
            .bounded_draft(
                f.target.scope(),
                &journal
                    .original_publication(&f.target, &f.reference)
                    .unwrap()
                    .unwrap()
                    .selected_resource,
                128 * 1024
            )
            .unwrap()
            .as_deref(),
        Some(f.body.as_str())
    );
    transport_and_git(&f.s);
    drop(f.writer);
}
#[test]
fn pr_metadata_issue_link_v87_actual_inspection_raw_cas_mutation_refuses_both_release_and_ack_without_fallback(
) {
    let f = uncertain();
    let (witness, current, token, deadline) = inspect(&f);
    let database = personal::database(&f.s.fixture);
    let db = Connection::open(&database).unwrap();
    assert_eq!(
        db.execute(
            "UPDATE pr_write_attempts SET created_at=created_at+1 WHERE attempt_id=?1",
            [f.reference.attempt_id]
        )
        .unwrap(),
        1
    );
    let before = dump(&db);
    let mut journal = PrWriteJournal::open(&database).unwrap();
    assert!(journal
        .acknowledge_issue_link_inspection_v87(&f.s.api, witness, current, &token, deadline)
        .is_err());
    assert_eq!(dump(&db), before);
    assert_eq!(
        journal.blocking(&f.target).unwrap().unwrap().state,
        PrWriteState::Uncertain
    );
    transport_and_git(&f.s);
}
#[test]
fn pr_metadata_issue_link_v87_actual_inspection_sibling_token_foreign_runner_expiry_and_newer_issue_refuse_no_resend(
) {
    for fault in 0..4 {
        let f = uncertain();
        let (witness, current, token, deadline) = inspect(&f);
        let database = personal::database(&f.s.fixture);
        let before = personal::sql(&f.s.fixture);
        let sibling = CancellationToken::default();
        let foreign = GithubProvider::new("github.invalid").unwrap();
        let deadline = if fault == 2 { Instant::now() } else { deadline };
        if fault == 3 {
            // A real ordinary writer on the SAME map advances the selected
            // Issue while invoking only installed local git (no Git mutation).
            let mut spec = crate::providers::runner::CommandSpec::new("git");
            spec.args = vec!["--version".into()];
            spec.kind = crate::providers::runner::CommandKind::Write {
                resource: current.actual().resources[1].clone(),
            };
            f.s.api.runner.run(spec, &token).unwrap();
        }
        let mut journal = PrWriteJournal::open(&database).unwrap();
        assert!(journal
            .acknowledge_issue_link_inspection_v87(
                if fault == 1 { &foreign } else { &f.s.api },
                witness,
                current,
                if fault == 0 { &sibling } else { &token },
                deadline
            )
            .is_err());
        assert_eq!(personal::sql(&f.s.fixture), before);
        assert_eq!(
            journal.blocking(&f.target).unwrap().unwrap().state,
            PrWriteState::Uncertain
        );
        transport_and_git(&f.s);
    }
}
#[test]
fn pr_metadata_issue_link_v87_actual_history_form_key_mouse_inspect_paint_resize_local_ack_uses_exact_original_not_saved_permission(
) {
    use crate::{
        app::{Action, Overlay},
        forms::FormAction,
        scoped_pr_collaboration_ui_v17::issue_link_v87::history::{self, IssueHistoryAdmissionV87},
    };
    use crossterm::event::{
        Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
    };
    let paint = |app: &mut crate::app::App, cols, rows| {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(cols, rows)).unwrap();
        terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    };
    let click = |app: &mut crate::app::App, action: FormAction| {
        let hit = app
            .hits
            .iter()
            .find(|h| h.action == Action::Form(action.clone()))
            .unwrap();
        let (column, row) = (
            hit.rect.x + hit.rect.width / 2,
            hit.rect.y + hit.rect.height / 2,
        );
        app.handle_event(Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column,
            row,
            modifiers: KeyModifiers::NONE,
        }));
    };
    for mouse in [false, true] {
        let mut tail = history_list_steps();
        tail.extend(selected_steps(pair(pr_node(), issue(9))));
        let mut f = uncertain_with_tail(tail);
        let database = personal::database(&f.s.fixture);
        let before = personal::sql(&f.s.fixture);
        let token = CancellationToken::default();
        let deadline = Instant::now() + std::time::Duration::from_secs(30);
        let provider =
            f.s.api
                .clone()
                .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
                .unwrap();
        let form = history::open(
            Arc::clone(f.source.opening()),
            IssueLinkCommandV87::LinkIssue,
            &provider,
            &database,
            &token,
            deadline,
            IssueHistoryAdmissionV87::Current,
        )
        .unwrap();
        assert!(form.fields.is_empty());
        assert!(!history::ack_ready(&form));
        f.s.app.form = Some(form);
        f.s.app.overlay = Some(Overlay::Form);
        f.s.app.color_mode = crate::theme::ColorMode::NoColor;
        paint(&mut f.s.app, 120, 40);
        if mouse {
            click(&mut f.s.app, FormAction::Preview);
        } else {
            f.s.app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )));
        }
        assert!(f.s.app.form.as_ref().unwrap().preview_requested);
        let request = history::request(f.s.app.form.as_ref().unwrap()).unwrap();
        let inspection_token = CancellationToken::default();
        let inspection_deadline = Instant::now() + std::time::Duration::from_secs(30);
        let bounded =
            f.s.api
                .clone()
                .with_original_pr_read_bounds_v13(inspection_deadline, 1024 * 1024)
                .unwrap();
        let form = history::open(
            Arc::clone(&request.view.original),
            request.view.command,
            &bounded,
            &database,
            &inspection_token,
            inspection_deadline,
            IssueHistoryAdmissionV87::Inspection(&request),
        )
        .unwrap();
        assert!(
            !history::ack_ready(&form),
            "Actual current observation starts unpainted"
        );
        assert!(form.fields.is_empty());
        f.s.app.form = Some(form);
        paint(&mut f.s.app, 120, 40);
        paint(&mut f.s.app, 120, 40);
        assert!(history::ack_ready(f.s.app.form.as_ref().unwrap()));
        for required in [
            "PR references (all)",
            "PR references (manual)",
            "PR references (nonmanual)",
            "Issue PR references (all)",
            "Issue PR references (manual)",
            "Issue PR references (nonmanual)",
            "ISSUE_20009",
            "PERSONAL_PR_1007",
        ] {
            assert!(
                history::state(f.s.app.form.as_ref().unwrap())
                    .unwrap()
                    .view
                    .lines
                    .iter()
                    .any(|line| line.contains(required)),
                "Missing actual inspection fact {required}"
            );
        }
        f.s.app.handle_event(Event::Resize(40, 10));
        paint(&mut f.s.app, 40, 10);
        assert!(!history::ack_ready(f.s.app.form.as_ref().unwrap()));
        assert_eq!(
            personal::sql(&f.s.fixture),
            before,
            "History/inspection/resize are read-only and retain every protected row"
        );
        f.s.app.handle_event(Event::Resize(120, 40));
        paint(&mut f.s.app, 120, 40);
        paint(&mut f.s.app, 120, 40);
        assert!(history::ack_ready(f.s.app.form.as_ref().unwrap()));
        if mouse {
            click(&mut f.s.app, FormAction::Submit);
        } else {
            f.s.app
                .handle_event(Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));
            f.s.app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Enter,
                KeyModifiers::NONE,
            )));
        }
        assert!(f.s.app.form.as_ref().unwrap().submit_requested);
        let request = history::request(f.s.app.form.as_ref().unwrap()).unwrap();
        let current = request.view.current.as_ref().unwrap();
        let result = history::acknowledge(
            &request,
            &f.s.api,
            &database,
            &current.cancel,
            current.deadline,
        )
        .unwrap();
        assert_eq!(result, (f.reference.clone(), 1));
        assert!(
            !history::ack_ready(f.s.app.form.as_ref().unwrap()),
            "Consumed once, no restoration by display"
        );
        assert!(PrWriteJournal::open(&database)
            .unwrap()
            .blocking(&f.target)
            .unwrap()
            .is_none());
        transport_and_git(&f.s);
    }
}
