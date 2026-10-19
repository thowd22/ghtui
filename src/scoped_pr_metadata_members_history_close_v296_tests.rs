//! Actual original/history factories; dismissal is not an ACK or saved source.
use super::*;
use crate::scoped_pr_collaboration_ui_v17::history_close_v296 as close;

fn listed(later: Vec<GhStep>) -> Setup {
    let mut tail = section();
    tail.extend(history_calls());
    tail.extend(later);
    let (mut s, _, _, _) = uncertain_metadata(false, tail);
    cached_conversation(&mut s);
    enter_section(&mut s);
    let original = PrCollaborationOpenIntentV17::capture_metadata_members_history_v61(
        &s.app,
        &database(&s.fixture),
    )
    .unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let form = member_ui::history::open(
        original.clone(),
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
        None,
    )
    .unwrap();
    install(&mut s, form);
    s
}
fn install(s: &mut Setup, form: Form) {
    assert!(s.app.form.is_none());
    s.app.form = Some(form);
    s.app.overlay = Some(Overlay::Form);
    s.app.form_dismissed = false;
    paint(&mut s.app, 120, 40);
}
fn commands(s: &Setup) -> Vec<u8> {
    fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap()
}
fn schema(s: &Setup) -> Vec<(String, String, String)> {
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut statement = db
        .prepare("SELECT type,name,COALESCE(sql,'') FROM sqlite_master ORDER BY type,name")
        .unwrap();
    let rows = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    rows
}
fn next_read(mut before: FullSql) -> FullSql {
    use rusqlite::types::Value as Sql;
    let (columns, rows) = before.get_mut("sync_reservations").unwrap();
    assert_eq!(rows.len(), 1);
    let column = |name| columns.iter().position(|value| value == name).unwrap();
    assert_eq!(rows[0][column("host")], Sql::Text("github.invalid".into()));
    assert_eq!(rows[0][column("login")], Sql::Text("user-id:42".into()));
    // Independent canonical fixture scope/repository10000/PR1007 tuple hash.
    assert_eq!(rows[0][column("source")], Sql::Text("personal-pr-source-v13/70014d4dd940fd87b9623122722d345014287e9c5628700049691aa23b64ae01".into()));
    assert_eq!(rows[0][column("active")], Sql::Integer(1));
    let generation = column("generation");
    let Sql::Integer(old) = rows[0][generation] else {
        panic!("Actual integer read reservation")
    };
    rows[0][generation] = Sql::Integer(old.checked_add(1).unwrap());
    before
}
fn close_key(s: &mut Setup) {
    s.app.handle_event(crossterm::event::Event::Key(
        crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Esc,
            crossterm::event::KeyModifiers::NONE,
        ),
    ));
}
fn list_again(s: &mut Setup) {
    let original = PrCollaborationOpenIntentV17::capture_metadata_members_history_v61(
        &s.app,
        &database(&s.fixture),
    )
    .unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 1024 * 1024)
        .unwrap();
    let form = member_ui::history::open(
        original.clone(),
        &provider,
        &database(&s.fixture),
        original.cancellation(),
        original.deadline(),
        None,
    )
    .unwrap();
    install(s, form);
}
#[test]
fn pr_metadata_members_history_v296_actual_close_preserves_protected_rows_and_allows_fresh_list() {
    let mut s = listed(history_calls());
    let before = full_sql(&s);
    let before_schema = schema(&s);
    let traffic = commands(&s);
    let original = Arc::clone(
        &member_ui::history::state(s.app.form.as_ref().unwrap())
            .unwrap()
            .view
            .original,
    );
    assert!(close::capture(&s.app).is_some());
    assert!(!ui::pure_close_v20::can_close(&s.app));
    close_key(&mut s);
    assert!(s.app.form.is_none());
    assert!(s.app.overlay.is_none());
    assert!(s.app.form_dismissed);
    assert!(s.app.hits.is_empty());
    assert!(s.app.closed_conversation_draft_v20.is_none());
    assert!(original.owns_presentation(&s.app));
    assert_eq!(s.app.status, "Composer closed • draft kept locally");
    assert_eq!(commands(&s), traffic);
    assert_eq!(
        full_sql(&s),
        before,
        "Close does not acknowledge, delete or hydrate any original row"
    );
    assert_eq!(schema(&s), before_schema);
    let expected_next = next_read(before);
    list_again(&mut s);
    assert_eq!(
        full_sql(&s),
        expected_next,
        "Fresh List advances only its exact genuine read reservation"
    );
    assert_eq!(schema(&s), before_schema);
    assert!(close::capture(&s.app).is_some());
    assert_eq!(
        member_ui::history::state(s.app.form.as_ref().unwrap())
            .unwrap()
            .view
            .snapshot
            .rows
            .len(),
        1
    );
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_members_history_v296_grafted_fields_busy_pending_and_foreign_presentation_refuse() {
    for case in 0..15 {
        let mut s = listed(Vec::new());
        let before = full_sql(&s);
        let traffic = commands(&s);
        let presentation = Arc::clone(s.app.form.as_ref().unwrap().presentation_v4().0);
        match case {
            0 => {
                s.app.form.as_mut().unwrap().fields.push(
                    crate::forms::Field::text("graft", "Not history", "private", false).unwrap(),
                )
            }
            1 => s.app.form.as_mut().unwrap().dirty = true,
            2 => s.app.form.as_mut().unwrap().submitting = true,
            3 => s.app.form.as_mut().unwrap().submitted = true,
            4 => s.app.form.as_mut().unwrap().preview_requested = true,
            5 => s.app.form.as_mut().unwrap().submit_requested = true,
            6 => s.app.form.as_mut().unwrap().editor_requested = true,
            7 => s.app.form.as_mut().unwrap().picker_requested = Some(0),
            8 => s.app.scoped_inline_cancel_v1 = true,
            9 => s.app.loading[View::Inbox.index()] = true,
            10 => s.app.github.as_mut().unwrap().identity.account_id = Some(43),
            11 => s.app.inbox_list.selected = s.app.inbox_list.selected.checked_add(1).unwrap(),
            12 => {
                let reference = member_ui::history::state(s.app.form.as_ref().unwrap())
                    .unwrap()
                    .view
                    .snapshot
                    .rows[0]
                    .marker
                    .publication_reference();
                s.app.form.as_mut().unwrap().publication = Some(reference);
            }
            13 => s.app.github.as_mut().unwrap().personal_pr_page_ticket_v13 = None,
            14 => s.app.form.as_mut().unwrap().preview_display_v1.document = None,
            _ => unreachable!(),
        }
        assert!(close::capture(&s.app).is_none(), "case {case}");
        assert!(Arc::ptr_eq(
            s.app.form.as_ref().unwrap().presentation_v4().0,
            &presentation
        ));
        assert_eq!(commands(&s), traffic);
        assert_eq!(full_sql(&s), before);
        exact(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_members_history_v296_confirmed_list_close_never_requires_unresolved_inspect_request()
{
    let mut tail = selected_phase(Cmd::AddLabel);
    tail.extend(metadata_post(Cmd::AddLabel));
    tail.extend(section());
    tail.extend(history_calls());
    let (mut s, form, preview) = prepared_metadata(Cmd::AddLabel, false, tail);
    let outcome = execute_metadata(&s, &form, &preview).unwrap();
    assert_eq!(outcome.state, State::Confirmed);
    cached_conversation(&mut s);
    enter_section(&mut s);
    list_again(&mut s);
    let history = member_ui::history::state(s.app.form.as_ref().unwrap()).unwrap();
    assert_eq!(
        history.view.snapshot.rows[0].marker.state,
        PrWriteState::Confirmed
    );
    assert!(member_ui::history::request(s.app.form.as_ref().unwrap()).is_err());
    assert!(close::capture(&s.app).is_some());
    let before = full_sql(&s);
    let traffic = commands(&s);
    close_key(&mut s);
    assert!(s.app.form.is_none());
    assert_eq!(full_sql(&s), before);
    assert_eq!(commands(&s), traffic);
    exact(&s, s.steps.len());
}
#[test]
fn pr_metadata_members_history_v296_stale_close_proof_never_drops_actual_newer_history() {
    let mut s = listed(history_calls());
    let old = close::capture(&s.app).unwrap();
    close_key(&mut s);
    assert!(s.app.form.is_none());
    list_again(&mut s);
    let current = Arc::clone(s.app.form.as_ref().unwrap().presentation_v4().0);
    let before = full_sql(&s);
    let traffic = commands(&s);
    // Adversarially replay the old synchronous Close proof after a NEW genuine
    // history has itself reached the normal dismissed/invalidate state.
    s.app
        .form
        .as_mut()
        .unwrap()
        .dispatch(&crate::forms::FormAction::Close);
    s.app.form.as_mut().unwrap().invalidate_plan();
    s.app.overlay = None;
    s.app.form_dismissed = true;
    assert!(!close::form_closed(&mut s.app, old));
    assert!(Arc::ptr_eq(
        s.app.form.as_ref().unwrap().presentation_v4().0,
        &current
    ));
    assert_eq!(commands(&s), traffic);
    assert_eq!(full_sql(&s), before);
    exact(&s, s.steps.len());
}
