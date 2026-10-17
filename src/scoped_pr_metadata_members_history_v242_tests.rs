//! Actual Metadata original/Core/Inspect/local ACK, never saved-row authority.
use super::*;
#[path = "scoped_pr_metadata_members_history_close_v296_tests.rs"]
mod close_v296;
type FullSql = std::collections::BTreeMap<String, (Vec<String>, Vec<Vec<rusqlite::types::Value>>)>;
fn full_sql(s: &Setup) -> FullSql {
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
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
            let mut query = db
                .prepare(&format!("SELECT * FROM \"{}\"", table.replace('"', "\"\"")))
                .unwrap();
            let columns = query
                .column_names()
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>();
            let mut rows = query
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
fn painted(s: &mut Setup, form: Form) -> Form {
    assert!(s.app.form.is_none());
    s.app.form = Some(form);
    s.app.form_dismissed = false;
    s.app.overlay = Some(Overlay::Form);
    paint(&mut s.app, 120, 40);
    let form = s.app.form.take().unwrap();
    s.app.overlay = None;
    form
}
#[test]
fn pr_metadata_members_v242_actual_metadata_inspection_and_local_ack_preserve_both_origin_rows() {
    use rusqlite::types::Value as Sql;
    for catalogue in [false, true] {
        let mut later = section();
        later.extend(history_calls());
        let mut inspected = principal();
        inspected.push(get(&format!("repos/{REPO}"), repo()));
        inspected.push(get(&format!("repos/{REPO}/pulls/7"), raw()));
        inspected.push(get(&format!("repos/{REPO}/issues/7"), issue(&raw())));
        inspected.push(graphql(Cmd::AddLabel, current(), false));
        inspected.push(get(&format!("repos/{REPO}/pulls/7"), raw()));
        inspected.push(get(&format!("repos/{REPO}"), repo()));
        inspected.extend(principal());
        assert_eq!(inspected.len(), 10);
        later.extend(inspected);
        let (mut s, _, _, reference) = uncertain_metadata(catalogue, later);
        cached_conversation(&mut s);
        enter_section(&mut s);
        let original = PrCollaborationOpenIntentV17::capture_metadata_members_history_v61(
            &s.app,
            &database(&s.fixture),
        )
        .unwrap();
        assert!(original.metadata_members_history_tab_v62());
        let cancel = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        let api = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
            .unwrap();
        let listed = member_ui::history::open(
            original,
            &api,
            &database(&s.fixture),
            &cancel,
            deadline,
            None,
        )
        .unwrap();
        let listed = painted(&mut s, listed);
        assert!(!member_ui::history::ack_ready(&listed));
        let request = member_ui::history::request(&listed).unwrap();
        let mut expected_read = full_sql(&s);
        if !catalogue {
            // The genuine Personal parent read reserves the next generation
            // on its SAME original source. It does not publish query rows or
            // acknowledge the protected attempt. Model the one allowed cell
            // before invoking the reader, preserving every other typed row.
            let (columns, rows) = expected_read.get_mut("sync_reservations").unwrap();
            assert_eq!(rows.len(), 1);
            let host = columns.iter().position(|s| s == "host").unwrap();
            let login = columns.iter().position(|s| s == "login").unwrap();
            let source = columns.iter().position(|s| s == "source").unwrap();
            let generation = columns.iter().position(|s| s == "generation").unwrap();
            assert_eq!(rows[0][host], Sql::Text("github.invalid".into()));
            assert_eq!(rows[0][login], Sql::Text("user-id:42".into()));
            assert!(
                matches!(&rows[0][source], Sql::Text(key) if key.starts_with("personal-pr-source-v13/"))
            );
            let Sql::Integer(old) = rows[0][generation] else {
                panic!("Actual Personal reservation integer generation")
            };
            rows[0][generation] = Sql::Integer(old.checked_add(1).unwrap());
        }
        let before_allocations = allocations(&s.app);
        let observed = member_ui::history::open(
            request.view.original.clone(),
            &api,
            &database(&s.fixture),
            &cancel,
            deadline,
            Some(&request),
        )
        .unwrap();
        assert_eq!(
            full_sql(&s),
            expected_read,
            "Only the original Personal read reservation advances; no local ACK or row hydration"
        );
        assert_eq!(allocations(&s.app), before_allocations);
        let observed = painted(&mut s, observed);
        assert!(member_ui::history::ack_ready(&observed));
        let request = member_ui::history::request(&observed).unwrap();
        let current = request.view.current.as_ref().unwrap();
        let mut expected = full_sql(&s);
        let (columns, rows) = expected.get_mut("pr_write_attempts").unwrap();
        assert_eq!(rows.len(), 1);
        let state = columns.iter().position(|s| s == "state").unwrap();
        assert_eq!(rows[0][state], Sql::Text("uncertain".into()));
        rows[0][state] = Sql::Text("acknowledged".into());
        let (columns, rows) = expected.get_mut("pr_write_scopes").unwrap();
        assert_eq!(rows.len(), 1);
        let generation = columns.iter().position(|s| s == "generation").unwrap();
        let Sql::Integer(old) = rows[0][generation] else {
            panic!("Exact integer journal generation")
        };
        rows[0][generation] = Sql::Integer(old.checked_add(1).unwrap());
        let commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        assert_eq!(
            member_ui::history::acknowledge(
                &request,
                &s.api,
                &database(&s.fixture),
                &current.cancel,
                current.deadline
            )
            .unwrap(),
            (reference, 1)
        );
        assert_eq!(full_sql(&s), expected, "Only exact attempt state and original scope generation change; all drafts/origins/foreign cells retained");
        assert_eq!(
            fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
            commands
        );
        exact(&s, s.steps.len());
    }
}
