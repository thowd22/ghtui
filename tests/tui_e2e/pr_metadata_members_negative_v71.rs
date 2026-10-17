//! Reachable Metadata-member read retirement before Form installation. Each
//! native name uses both genuine Personal and Catalogue origins, never an
//! injected busy-Form dismissal, constructed completion or saved-row source.
use super::*;
use crate::support::owned_pty_processes_v1::ProcessIdentity;

fn dump(db: &rusqlite::Connection) -> RawSql {
    let tables=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|row|row.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut result = RawSql::new();
    for name in tables {
        let mut query = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
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
                    .collect::<rusqlite::Result<Vec<rusqlite::types::Value>>>()
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort_by_key(|row| format!("{row:?}"));
        result.insert(name, (columns, rows));
    }
    result
}
fn schema(f: &Fixture) -> Vec<(String, String, String)> {
    let db = sql(f);
    schema_connection(&db)
}
fn schema_connection(db: &rusqlite::Connection) -> Vec<(String, String, String)> {
    let mut query=db.prepare("SELECT type,name,sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type,name").unwrap();
    let result = query
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect();
    result
}
fn expected_clear(f: &Fixture, before: &RawSql) -> RawSql {
    // Independent disposable reference database. No production Cache clear
    // helper is called and this oracle never writes the application's file.
    // Tables/typed data precede triggers, so importing immutable origin rows
    // itself cannot fire the application's mutation/lookup triggers.
    let definition = schema(f);
    let reference = rusqlite::Connection::open_in_memory().unwrap();
    for (kind, _, ddl) in &definition {
        if kind == "table" {
            reference.execute_batch(ddl).unwrap();
        }
    }
    // Alphabetical table import is not foreign-key dependency order. Disable
    // checks ONLY on this disposable, autocommit reference while copying the
    // complete typed snapshot; prove all references before installing triggers.
    assert!(reference.is_autocommit());
    reference
        .pragma_update(None, "foreign_keys", false)
        .unwrap();
    assert!(!reference
        .pragma_query_value(None, "foreign_keys", |row| row.get::<_, bool>(0))
        .unwrap());
    for (table, (columns, rows)) in before {
        let names = columns
            .iter()
            .map(|name| format!("\"{}\"", name.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(",");
        let placeholders = (1..=columns.len())
            .map(|i| format!("?{i}"))
            .collect::<Vec<_>>()
            .join(",");
        for row in rows {
            reference
                .execute(
                    &format!(
                        "INSERT INTO \"{}\"({names}) VALUES({placeholders})",
                        table.replace('"', "\"\"")
                    ),
                    rusqlite::params_from_iter(row),
                )
                .unwrap();
        }
    }
    reference.pragma_update(None, "foreign_keys", true).unwrap();
    assert!(reference
        .pragma_query_value(None, "foreign_keys", |row| row.get::<_, bool>(0))
        .unwrap());
    {
        let mut check = reference.prepare("PRAGMA foreign_key_check").unwrap();
        assert!(check.query([]).unwrap().next().unwrap().is_none());
    }
    assert_eq!(dump(&reference), *before);
    assert_eq!(
        schema_connection(&reference),
        definition
            .iter()
            .filter(|(kind, _, _)| kind == "table")
            .cloned()
            .collect::<Vec<_>>()
    );
    for (kind, _, ddl) in &definition {
        if kind != "table" {
            reference.execute_batch(ddl).unwrap();
        }
    }
    assert_eq!(schema_connection(&reference), definition);
    assert_eq!(dump(&reference), *before);
    assert_eq!(schema(f), definition);
    // The documented local Clear effects are independently written here. The
    // actual fixed schema's lookup triggers account for every affected row,
    // including typed NULL and epoch increments, without excluding a table.
    reference.execute("UPDATE sync_reservations SET generation=CASE WHEN generation<9223372036854775807 THEN generation+1 ELSE generation END,active=0 WHERE host=?1 AND login='user-id:42'",[HOST]).unwrap();
    reference
        .execute(
            "UPDATE sync_resource_bindings SET active=0 WHERE host=?1 AND login='user-id:42'",
            [HOST],
        )
        .unwrap();
    reference.execute("UPDATE sync_resource_bindings SET visible_generation=NULL WHERE host=?1 AND login='user-id:42'",[HOST]).unwrap();
    reference
        .execute(
            "DELETE FROM checkpoints WHERE host=?1 AND login='user-id:42'",
            [HOST],
        )
        .unwrap();
    reference
        .execute(
            "DELETE FROM records WHERE host=?1 AND login='user-id:42'",
            [HOST],
        )
        .unwrap();
    dump(&reference)
}
fn owned_held(s: &mut Session, f: &Fixture, index: usize) -> ProcessIdentity {
    s.wait_for_command_count(f, index + 1);
    let log = commands(s, f);
    assert_eq!(log.len(), index + 1);
    assert_eq!(log[index]["matched"], true);
    let pid = i32::try_from(log[index]["pid"].as_u64().unwrap()).unwrap();
    let actual = process_identity(pid).unwrap().unwrap();
    assert!(!actual.zombie);
    let owner =
        OwnedPtySession::capture(u32::try_from(s.owned_application_pid()).unwrap()).unwrap();
    assert!(owner.owns(&actual));
    actual
}
fn reap(s: &mut Session, old: &ProcessIdentity) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        s.poll_output(Duration::from_millis(2));
        match process_identity(old.pid) {
            Ok(None) => return,
            Ok(Some(now)) => assert_eq!(
                (now.pid, now.uid, now.birth, now.session),
                (old.pid, old.uid, old.birth, old.session)
            ),
            #[cfg(target_os = "linux")]
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {}
            Err(error) => panic!("Actual Members read child observation: {error}"),
        }
        assert!(
            Instant::now() < until,
            "Exact old child must disappear; a zombie is not cleanup"
        );
    }
}
fn saved_member(f: &Fixture) -> (String, String) {
    let db = sql(f);
    let mut query=db.prepare("SELECT resource,body FROM drafts WHERE host=?1 AND login='user-id:42' AND resource LIKE 'metadata-members-v48/%' ORDER BY resource").unwrap();
    let rows = query
        .query_map([HOST], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    let body: Value = serde_json::from_str(&rows[0].1).unwrap();
    assert_eq!(
        body,
        json!({"version":17,"values":{"body":selected_value(Cmd::AddLabel)},"publication":null})
    );
    rows.into_iter().next().unwrap()
}
fn prepare_held(
    personal: bool,
    mouse: bool,
) -> (
    Fixture,
    Session,
    GitState,
    Vec<u8>,
    Vec<String>,
    Vec<GhStep>,
    ProcessIdentity,
) {
    let command = Cmd::AddLabel;
    let (mut steps, initial, section_end, opened, prepared) = prepared_steps(personal, command);
    let mut next = phase(command, raw(), candidate(command), current());
    next.last_mut().unwrap().delay_ms = 30_000;
    steps.extend(next);
    let (f, mut s, git, config, stable) =
        begin_metadata(&steps, personal, mouse, initial, section_end);
    preview_from_metadata(&mut s, &f, mouse, command, opened, prepared, &steps);
    let saved = saved_member(&f);
    close_members(&mut s, mouse, command, true);
    assert_eq!(saved_member(&f), saved);
    no_attempt(&f);
    // This is a NEW read, not Confirm. Its old Metadata underlay remains while
    // the worker prepares the new Form, so global view/Back are real controls.
    crate::pr_actions::menu(&mut s, mouse, command.label());
    let held = owned_held(&mut s, &f, steps.len() - 1);
    s.wait_for_cursorless_condition(
        |frame| {
            let text = frame.contents();
            crate::support::main_footer_visible(frame)
                && text.contains("[u Metadata]")
                && text.contains("[Esc Cancel]")
                && !text.contains(&members_title(command))
                && !text.contains("[Confirm action]")
        },
        "Actual pre-Form member read leaves genuine Metadata underlay",
        Duration::from_secs(8),
    );
    assert_eq!(saved_member(&f), saved);
    exact(&s, &f, &steps);
    (f, s, git, config, stable, steps, held)
}
fn clear(mouse: bool) {
    for personal in [false, true] {
        let (f, mut s, git, config, stable, steps, held) = prepare_held(personal, mouse);
        let before = all_sql(&f);
        let definitions = schema(&f);
        let saved = saved_member(&f);
        let frame = s.completed_cursorless_end();
        choose(&mut s, mouse, "[b Back]", b"b");
        s.wait_for_new_cursorless_condition(
            frame,
            |screen| {
                let text = screen.contents();
                crate::support::main_footer_visible(screen)
                    && text.contains("[x Clear cache]")
                    && !text.contains("[p PR actions]")
                    && !text.contains("[Esc Cancel]")
                    && !text.contains(DETAIL_BODY)
                    && !text.contains(&members_title(Cmd::AddLabel))
            },
            "NEW actual cached Inbox Back before visible Clear",
            Duration::from_secs(8),
        );
        reap(&mut s, &held);
        assert_eq!(all_sql(&f), before);
        exact(&s, &f, &steps);
        let expected = expected_clear(&f, &before);
        // Clear intentionally removes this account's cached records, not its
        // drafts or any foreign account. The complete typed SQL oracle above
        // covers every row; keep the ancillary observer equally precise.
        let stable_after_clear = stable
            .iter()
            .filter(|encoded| {
                let row: Value = serde_json::from_str(encoded).unwrap();
                !(row.as_array().is_some_and(|row| row.len() == 9)
                    && row[0] == HOST
                    && row[1] == "user-id:42")
            })
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(stable.len() - stable_after_clear.len(), 1);
        let frame = s.completed_cursorless_end();
        choose(&mut s, mouse, "[x Clear cache]", b"x");
        s.wait_for_new_cursorless_condition(
            frame,
            |screen| {
                let text = screen.contents();
                crate::support::main_footer_visible(screen)
                    && text.contains(if personal {
                        "No pull requests"
                    } else {
                        "Original account cache cleared • drafts and write history retained"
                    })
                    && !text.contains("[p PR actions]")
                    && !text.contains(DETAIL_BODY)
                    && !text.contains(&members_title(Cmd::AddLabel))
                    && !text.contains("[Confirm action]")
            },
            "NEW origin-specific actual Clear with no old private producer paint",
            Duration::from_secs(8),
        );
        assert_eq!(
            all_sql(&f),
            expected,
            "Every real typed row equals independently modelled Clear, no table exemption"
        );
        assert_eq!(schema(&f), definitions);
        assert_eq!(saved_member(&f), saved);
        no_attempt(&f);
        assert_eq!(
            commands(&s, &f).len(),
            steps.len(),
            "Local Clear sends no GH read/write or resend"
        );
        s.capture_completed_terminal_v1("members-metadata-v71-real-Clear-retains-saved-draft");
        finish(s, &f, mouse, &steps, &git, &config, &stable_after_clear);
    }
}
fn newer_view(mouse: bool) {
    for personal in [false, true] {
        let (f, mut s, git, config, stable, steps, held) = prepare_held(personal, mouse);
        let before = all_sql(&f);
        let definitions = schema(&f);
        let saved = saved_member(&f);
        let frame = s.completed_cursorless_end();
        choose(&mut s, mouse, "2 Repository", b"2");
        s.wait_for_new_cursorless_condition(
            frame,
            |screen| {
                let text = screen.contents();
                crate::support::main_footer_visible(screen)
                    && text.contains("Repository • focused view")
                    && text.contains("Recent commits")
                    && text.contains("[m Local operations]")
                    && !text.contains(&members_title(Cmd::AddLabel))
                    && !text.contains(DETAIL_BODY)
                    && !text.contains("Verifying original Conversation")
                    && !text.contains("[Esc Cancel]")
            },
            "NEW actual newer Repository owns its content/loading/status",
            Duration::from_secs(8),
        );
        reap(&mut s, &held);
        exact(&s, &f, &steps);
        let frame = s.completed_cells_end();
        crate::local_management::operation(
            &mut s,
            mouse,
            ghtui::local_actions::LocalCommand::CreateBranch,
        );
        // This real editable Form owns a visible cursor. Require a NEW
        // completed cell frame, not a cursorless redraw of its underlay.
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            s.poll_output(Duration::from_millis(1));
            let ready = s.completed_cells_end() > frame
                && s.completed_cells_screen().is_some_and(|screen| {
                    crate::support::main_footer_visible(screen)
                        && crate::support::overlay_body(screen, "Create branch").is_some_and(
                            |body| {
                                body.contains(
                                    "LOCAL INTENT • Preview first, then explicitly confirm",
                                ) && body.contains("New name:")
                                    && body.contains("Saved version:")
                                    && body.contains(git.head.trim())
                            },
                        )
                        && [
                            "[F4 Choose]",
                            "[Preview]",
                            "[External editor]",
                            "[Confirm action]",
                            "[Keep draft / Close]",
                        ]
                        .iter()
                        .all(|label| {
                            crate::support::completed_navigation_controls_v1::owned_action_position(
                                screen,
                                "Create branch",
                                label,
                                &[],
                            )
                            .is_some()
                        })
                        && !screen.contents().contains(&members_title(Cmd::AddLabel))
                        && !screen.contents().contains(DETAIL_BODY)
                });
            if ready {
                break;
            }
            assert!(
                Instant::now() < until,
                "NEW genuine topmost editable local Form with exact fields/controls, never stale Members Form\n{}",
                s.screen()
            );
        }
        assert_eq!(all_sql(&f), before);
        assert_eq!(schema(&f), definitions);
        assert_eq!(saved_member(&f), saved);
        no_attempt(&f);
        exact(&s, &f, &steps);
        s.capture_completed_terminal_v1("members-metadata-v71-newer-Repository-local-Form");
        crate::local::close(&mut s, mouse);
        s.wait_for_cursorless_condition(
            |screen| {
                let text = screen.contents();
                crate::support::main_footer_visible(screen)
                    && text.contains("Repository • focused view")
                    && text.contains("[m Local operations]")
                    && !text.contains("LOCAL INTENT")
                    && !text.contains(&members_title(Cmd::AddLabel))
            },
            "Actual newer local Form closes without old private completion",
            Duration::from_secs(8),
        );
        assert_eq!(all_sql(&f), before);
        assert_eq!(saved_member(&f), saved);
        finish(s, &f, mouse, &steps, &git, &config, &stable);
    }
}
#[test]
fn pr_metadata_members_v71_clear_after_held_open_keyboard_both_origins_exact_sql_and_no_stale_source(
) {
    clear(false)
}
#[test]
fn pr_metadata_members_v71_clear_after_held_open_mouse_both_origins_exact_sql_and_no_stale_source()
{
    clear(true)
}
#[test]
fn pr_metadata_members_v71_newer_repository_form_keyboard_both_origins_not_clobbered() {
    newer_view(false)
}
#[test]
fn pr_metadata_members_v71_newer_repository_form_mouse_both_origins_not_clobbered() {
    newer_view(true)
}
