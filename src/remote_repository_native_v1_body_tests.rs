//! Actual provider-produced private file before any pending/refusal assertion.
use super::*;

fn script(objects: &fx::Objects) -> Vec<crate::test_support::GhStep> {
    let mut steps = fx::source_steps();
    steps.extend(fx::prefix());
    steps.extend(fx::default_payload(objects));
    steps.extend(fx::suffix(42));
    steps.extend(fx::prefix());
    steps.push(fx::get(
        &format!("repos/example/remote/git/trees/{}", objects.child),
        objects.child_json.clone(),
    ));
    steps.extend(fx::suffix(42));
    steps.extend(fx::prefix());
    steps.push(fx::blob_step(objects));
    steps.extend(fx::suffix(42));
    assert_eq!(steps.len(), 31);
    steps
}
fn actual_private_file(
    fixture: &Fixture,
    objects: &fx::Objects,
    steps: &[crate::test_support::GhStep],
) -> App {
    fixture.scripted_gh(steps);
    let source = fx::source(fixture);
    let state = initial(
        source,
        0,
        &fx::provider(fixture),
        &fixture.root.join("cache.sqlite3"),
        &Default::default(),
    )
    .unwrap();
    let mut app = App::default();
    install_initial(&mut app, state);
    let nested = app
        .remote_repository
        .as_ref()
        .unwrap()
        .workspace
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .position(|row| row.label.contains("nested"))
        .unwrap();
    dispatch(&mut app, RemoteBrowseActionV1::Row(nested));
    for _ in 0..2 {
        dispatch(&mut app, RemoteBrowseActionV1::Open);
        let work = take_request(&mut app).unwrap();
        assert!(work.owner.owns(&app));
        let mut prepared = execute(
            work,
            &fx::provider(fixture),
            &fixture.root.join("cache.sqlite3"),
            &Default::default(),
        )
        .unwrap();
        assert!(accept(&mut app, prepared.remote_repository.take().unwrap()));
    }
    let state = app.remote_repository.as_ref().unwrap();
    let RemoteBrowsePageV1::File { original, .. } = &state.workspace.as_ref().unwrap().page else {
        panic!("actual private file")
    };
    assert_eq!(original.bytes(), objects.bytes);
    assert!(state
        .workspace
        .as_ref()
        .unwrap()
        .rows
        .iter()
        .any(|row| row.label.contains("old remote saved content")));
    app
}
fn rendered(app: &mut App, width: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn remote_tree_v1_followup_native_pending_refresh_hides_real_old_blob_before_any_worker_read() {
    for mouse in [false, true] {
        let fixture = Fixture::new();
        let objects = fx::objects(&fixture);
        let steps = script(&objects);
        let mut app = actual_private_file(&fixture, &objects, &steps);
        assert!(rendered(&mut app, 80).contains("old remote saved content"));
        dispatch(&mut app, RemoteBrowseActionV1::Refresh);
        let work = take_request(&mut app).unwrap();
        assert!(work.owner.owns(&app));
        assert!(
            app.remote_repository
                .as_ref()
                .unwrap()
                .workspace
                .as_ref()
                .unwrap()
                .rows
                .iter()
                .any(|row| row.label.contains("old remote saved content")),
            "real retained body exists but is not display authority while pending"
        );
        for width in [120, 80, 40] {
            let frame = rendered(&mut app, width);
            assert!(frame.contains("read pending") && frame.contains("[Esc Cancel]"));
            assert!(!frame.contains("old remote saved content"));
            let state = app.remote_repository.as_ref().unwrap();
            assert_eq!(state.hits.len(), 1);
            assert_eq!(state.hits[0].1, RemoteBrowseActionV1::Back);
        }
        if mouse {
            let hit = app.remote_repository.as_ref().unwrap().hits[0].0;
            event(
                &mut app,
                Event::Mouse(MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column: hit.x,
                    row: hit.y,
                    modifiers: KeyModifiers::NONE,
                }),
            );
        } else {
            key(&mut app, KeyCode::Esc, KeyModifiers::NONE);
        }
        assert!(app.remote_repository.is_none());
        assert!(!work.owner.owns(&app));
        assert!(take_cancel(&mut app));
        assert!(take_request(&mut app).is_none());
        fx::exact(&fixture, &steps);
        fx::preserved(&fixture, &objects);
    }
}

#[test]
fn remote_tree_v1_followup_native_closed403_erases_real_old_blob_and_back_keeps_unavailable() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut steps = script(&objects);
    steps.extend(fx::prefix());
    let mut denied = fx::blob_step(&objects);
    denied.status = 1;
    denied.stdout.clear();
    denied.stderr = b"gh: PRIVATE_BLOB_DENIED (HTTP 403)".to_vec();
    steps.push(denied);
    steps.extend(fx::suffix(42));
    assert_eq!(steps.len(), 38);
    let mut app = actual_private_file(&fixture, &objects, &steps);
    assert!(rendered(&mut app, 80).contains("old remote saved content"));
    dispatch(&mut app, RemoteBrowseActionV1::Refresh);
    let work = take_request(&mut app).unwrap();
    let prepared = execute(
        work,
        &fx::provider(&fixture),
        &fixture.root.join("cache.sqlite3"),
        &Default::default(),
    )
    .unwrap();
    assert!(accept(&mut app, prepared.remote_repository.unwrap()));
    assert!(app.remote_repository.as_ref().unwrap().workspace.is_none());
    let frame = rendered(&mut app, 120);
    assert!(frame.contains("private tree/content hidden"));
    assert!(!frame.contains("old remote saved content") && !frame.contains("PRIVATE_BLOB_DENIED"));
    dispatch(&mut app, RemoteBrowseActionV1::Back);
    let frame = rendered(&mut app, 80);
    assert!(
        frame.contains("default snapshot unavailable")
            && !frame.contains("old remote saved content")
    );
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}

#[test]
fn remote_tree_v1_followup_native_changed_source_hides_real_blob_without_adopting_sql_epoch() {
    let fixture = Fixture::new();
    let objects = fx::objects(&fixture);
    let mut steps = script(&objects);
    steps.extend(fx::prefix());
    assert_eq!(steps.len(), 34);
    let mut app = actual_private_file(&fixture, &objects, &steps);
    assert!(rendered(&mut app, 80).contains("old remote saved content"));
    let source = app.remote_repository.as_ref().unwrap().source.clone();
    let database = fixture.root.join("cache.sqlite3");
    let connection = rusqlite::Connection::open(&database).unwrap();
    assert_eq!(connection.execute("UPDATE sync_reservations SET generation=2 WHERE host='github.invalid' AND login='user-id:42' AND source=?1 AND generation=1 AND active=1",[source.sync_source()]).unwrap(),1);
    drop(connection);
    dispatch(&mut app, RemoteBrowseActionV1::Refresh);
    let work = take_request(&mut app).unwrap();
    let prepared = execute(
        work,
        &fx::provider(&fixture),
        &database,
        &Default::default(),
    )
    .unwrap();
    assert!(accept(&mut app, prepared.remote_repository.unwrap()));
    assert!(app.remote_repository.is_none());
    assert!(app.status.contains("private pages hidden"));
    let connection = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let generation:u64=connection.query_row("SELECT generation FROM sync_reservations WHERE host='github.invalid' AND login='user-id:42' AND source=?1",[source.sync_source()],|row|row.get(0)).unwrap();
    assert_eq!(generation, 2);
    fx::exact(&fixture, &steps);
    fx::preserved(&fixture, &objects);
}
