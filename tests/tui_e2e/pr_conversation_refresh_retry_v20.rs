//! Actual binary + strict installed fake gh + real SQL/Git and owned children.
//! No private source, owner, result or durable receipt is constructed here.
use super::*;
use std::collections::BTreeMap;

const RETURN: &str = "Confirmed Conversation • read-only refresh";
const REFRESH: &str = "[r Refresh PR]";
const BACK: &str = "[Esc Back to Inbox]";
const RETURN_CONTROLS: &[&str] = &[REFRESH, BACK];
type RawRow = BTreeMap<String, Value>;
type RawState = BTreeMap<String, Vec<RawRow>>;
struct EndState {
    git: GitState,
    config: Vec<u8>,
    kept: Vec<String>,
    terminal: Vec<String>,
}

fn snapshot(f: &Fixture) -> RawState {
    use rusqlite::types::ValueRef;
    let db = raw(f);
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    names.into_iter().map(|table|{
        let mut query=db.prepare(&format!("SELECT * FROM {table}")).unwrap();
        let names=query.column_names().iter().map(|s|(*s).to_owned()).collect::<Vec<_>>();
        let mut rows=query.query_map([],|row|Ok(names.iter().enumerate().map(|(i,name)|{
            let value=match row.get_ref(i).unwrap() {
                ValueRef::Null=>Value::Null,ValueRef::Integer(n)=>json!(n),ValueRef::Real(n)=>json!(n),
                ValueRef::Text(s)=>json!(std::str::from_utf8(s).unwrap()),
                ValueRef::Blob(b)=>json!({"exact_blob_hex":b.iter().map(|v|format!("{v:02x}")).collect::<String>()}),
            };(name.clone(),value)
        }).collect::<RawRow>())).unwrap().map(Result::unwrap).collect::<Vec<_>>();
        rows.sort_by_key(|r|serde_json::to_string(r).unwrap());(table,rows)
    }).collect()
}
fn expected_reservation(mut before: RawState, source: &str, generation: u64) -> RawState {
    let rows = before.get_mut("sync_reservations").unwrap();
    let mut seen = 0;
    for row in rows.iter_mut() {
        if row["host"] == HOST && row["login"] == "user-id:42" && row["source"] == source {
            assert_eq!(row["generation"], generation);
            row.insert("generation".into(), json!(generation + 1));
            row.insert("active".into(), json!(1));
            seen += 1;
        }
    }
    assert_eq!(seen, 1);
    rows.sort_by_key(|r| serde_json::to_string(r).unwrap());
    before
}
fn reservation(f: &Fixture) -> (String, u64) {
    raw(f).query_row("SELECT source,generation FROM sync_reservations WHERE host=?1 AND login='user-id:42' AND source LIKE 'personal-pr-source-v13/%'",[HOST],|r|Ok((r.get(0)?,r.get(1)?))).unwrap()
}
fn terminal(f: &Fixture) -> Vec<String> {
    let state = snapshot(f);
    state
        .into_iter()
        .filter(|(name, _)| {
            name.starts_with("pr_write_") || name == "drafts" || name == "sync_mutation_fences"
        })
        .map(|(name, rows)| format!("{name}:{}", serde_json::to_string(&rows).unwrap()))
        .collect()
}
fn failed_phase(index: usize) -> Vec<GhStep> {
    let mut phase = read_phase_v19();
    phase[index].status = 1;
    phase[index].stdout.clear();
    phase[index].stderr = b"ACTUAL_V20_NATIVE_READ_FAILED_NOT_A_WRITE".to_vec();
    phase.truncate(index + 1);
    phase
}
fn retained_setup(
    mut phases: Vec<GhStep>,
    mouse: bool,
) -> (Fixture, Session, Vec<GhStep>, GitState, Vec<u8>) {
    let command = PrCommand::NewComment;
    let mut steps = initial_steps(true);
    let initial = steps.len();
    steps.extend(phase(command, true, Some(true)));
    let opened = steps.len();
    steps.extend(phase(command, false, Some(true)));
    let previewed = steps.len();
    steps.extend(phase(command, false, Some(true)));
    steps.extend(receipt(command));
    let mut unavailable = visibility();
    unavailable[3].stdout = b"[]".to_vec();
    unavailable.truncate(4);
    steps.extend(unavailable);
    let retained = steps.len();
    steps.append(&mut phases);
    let f = resource_configured(&steps);
    seed_independent(&f);
    let git = git_state(&f);
    let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
    let mut s = launch(&f, mouse);
    enter(&mut s, &f, true, mouse, initial, false, &steps[..initial]);
    prepare(&mut s, &f, mouse, command, opened, previewed);
    control(&mut s, mouse, &title(command), "[Confirm action]", EDITABLE);
    s.wait_for_command_count(&f, retained);
    form_frame(&mut s, 0, RETURN, &["DURABLY CONFIRMED", REFRESH, BACK]);
    assert_eq!(
        raw(&f)
            .query_row(
                "SELECT count(*) FROM pr_write_attempts WHERE state='confirmed'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        serde_json::from_str::<Value>(
            &raw(&f)
                .query_row("SELECT confirmed_json FROM pr_write_attempts", [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap()
        )
        .unwrap()["resource_id"],
        "901"
    );
    assert!(
        !crate::support::overlay_body(s.completed_cells_screen().unwrap(), RETURN)
            .unwrap()
            .contains(DETAIL_BODY)
    );
    (f, s, steps, git, config)
}
fn refresh(s: &mut Session, mouse: bool) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        s.poll_output(Duration::from_millis(2));
        let enabled = s.completed_cells_screen().and_then(|screen| {
            let (x, y) = crate::support::completed_navigation_controls_v1::owned_action_position(
                screen,
                RETURN,
                REFRESH,
                &[],
            )?;
            screen.cell(y, x).filter(|c| c.underline()).map(|_| (x, y))
        });
        if enabled.is_some() {
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual useful enabled Refresh control required\n{}",
            s.screen()
        );
    }
    control(s, mouse, RETURN, REFRESH, RETURN_CONTROLS);
}
fn failed(s: &mut Session, after: usize) {
    form_frame(s, after, RETURN, &["Read failed", REFRESH, BACK]);
    assert!(
        !crate::support::overlay_body(s.completed_cells_screen().unwrap(), RETURN)
            .unwrap()
            .contains(DETAIL_BODY)
    );
}
fn fresh(s: &mut Session, after: usize) {
    s.wait_for_new_cursorless_condition(
        after,
        |screen| {
            let text = screen.contents();
            crate::support::main_footer_visible(screen)
                && text.contains("Fresh original PR/search observation")
                && crate::support::controls::control_painted(screen, "[b Back]")
                && !text.contains(REFRESH)
        },
        "Actual atomic NEW Personal page after full fresh read and final UID",
        Duration::from_secs(8),
    );
}
fn finish(mut s: Session, f: &Fixture, mouse: bool, steps: &[GhStep], original: EndState) {
    exact(&s, f, steps);
    assert_eq!(protected(f), original.kept);
    assert_eq!(terminal(f), original.terminal);
    assert_eq!(git_state(f), original.git);
    assert_eq!(
        fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
        original.config
    );
    assert_read_only_git(f);
    assert_eq!(
        raw(f)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        raw(f)
            .query_row(
                "SELECT count(*) FROM pr_write_attempts WHERE state='acknowledged'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    s.quit_and_assert_restored_terminal(mouse);
}
fn repeated(mouse: bool) {
    let mut phases = failed_phase(4);
    phases.extend(failed_phase(9));
    phases.extend(read_phase_v19());
    let (f, mut s, steps, git, config) = retained_setup(phases, mouse);
    let kept = protected(&f);
    let original_terminal = terminal(&f);
    let before = stable_sql(&f);
    let start = commands(&s, &f).len();
    for (ordinal, end) in [start + 5, start + 15].into_iter().enumerate() {
        let (source, generation) = reservation(&f);
        let sql = snapshot(&f);
        let frame = s.completed_cells_end();
        refresh(&mut s, mouse);
        s.wait_for_command_count(&f, end);
        failed(&mut s, frame);
        assert_eq!(
            snapshot(&f),
            expected_reservation(sql, &source, generation),
            "ONLY actual normal reservation advances on failed read {ordinal}"
        );
        assert_eq!(stable_sql(&f), before);
        assert_eq!(terminal(&f), original_terminal);
        exact(&s, &f, &steps[..end]);
        // Genuine SIGWINCH and newly completed current owned Form at both sizes.
        let frame = s.completed_cells_end();
        s.resize(
            if ordinal == 0 { 120 } else { 80 },
            if ordinal == 0 { 40 } else { 24 },
        );
        form_frame(&mut s, frame, RETURN, &[REFRESH, BACK]);
    }
    s.capture_completed_terminal_v1("v20-personal-read-retry-return");
    let frame = s.completed_cursorless_end();
    refresh(&mut s, mouse);
    s.wait_for_command_count(&f, steps.len());
    fresh(&mut s, frame);
    assert_raw_transition_v19(&f, &before);
    assert_projection_v19(&f, &[(501, OLD), (901, BODY)], &[], 1);
    s.capture_completed_terminal_v1("v20-personal-read-retry-fresh-page");
    select_current(&mut s, mouse, BODY);
    finish(
        s,
        &f,
        mouse,
        &steps,
        EndState {
            git,
            config,
            kept,
            terminal: original_terminal,
        },
    );
}
fn wait_child_absent(
    s: &mut Session,
    child: &crate::support::owned_pty_processes_v1::ProcessIdentity,
) {
    let until = Instant::now() + Duration::from_secs(8);
    let mut unknown = None;
    loop {
        match process_identity(child.pid) {
            Ok(None) => return,
            Ok(Some(now))
                if (now.uid, now.birth, now.session) != (child.uid, child.birth, child.session) =>
            {
                return
            }
            Ok(Some(_)) => {}
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                unknown = Some(error.to_string())
            }
            Err(error) => panic!("Actual child identity read failed: {error}"),
        }
        assert!(
            Instant::now() < until,
            "Exact child remains or absence unproven: {unknown:?}"
        );
        s.poll_output(Duration::from_millis(2));
    }
}
fn canceled(mouse: bool) {
    let mut held = read_phase_v19();
    held[4].delay_ms = 2000;
    held.truncate(5);
    held.extend(read_phase_v19());
    let (f, mut s, steps, git, config) = retained_setup(held, mouse);
    let kept = protected(&f);
    let original_terminal = terminal(&f);
    let before = stable_sql(&f);
    let sql = snapshot(&f);
    let (source, generation) = reservation(&f);
    let start = commands(&s, &f).len();
    refresh(&mut s, mouse);
    s.wait_for_command_count(&f, start + 5);
    let pid = i32::try_from(commands(&s, &f)[start + 4]["pid"].as_u64().unwrap()).unwrap();
    let child = process_identity(pid).unwrap().unwrap();
    let owned =
        OwnedPtySession::capture(u32::try_from(s.owned_application_pid()).unwrap()).unwrap();
    assert!(owned.owns(&child));
    assert!(!child.zombie);
    assert_eq!(snapshot(&f), expected_reservation(sql, &source, generation));
    // Actual pending control is disabled for Refresh while its real child runs.
    form_frame(&mut s, 0, RETURN, &["[Esc Cancel]"]);
    assert!(!s.screen().contains(REFRESH));
    let frame = s.completed_cells_end();
    control(&mut s, mouse, RETURN, "[Esc Cancel]", &["[Esc Cancel]"]);
    wait_child_absent(&mut s, &child);
    form_frame(&mut s, frame, RETURN, &[REFRESH, BACK]);
    assert_eq!(commands(&s, &f).len(), start + 5);
    assert_eq!(stable_sql(&f), before);
    assert_eq!(terminal(&f), original_terminal);
    // A NEW explicit input starts the next phase; cancellation never resends.
    let frame = s.completed_cursorless_end();
    refresh(&mut s, mouse);
    s.wait_for_command_count(&f, steps.len());
    fresh(&mut s, frame);
    assert_raw_transition_v19(&f, &before);
    assert_projection_v19(&f, &[(501, OLD), (901, BODY)], &[], 1);
    finish(
        s,
        &f,
        mouse,
        &steps,
        EndState {
            git,
            config,
            kept,
            terminal: original_terminal,
        },
    );
}
fn invalidated(mouse: bool) {
    for clear in [false, true] {
        let (f, mut s, steps, git, config) = retained_setup(failed_phase(4), mouse);
        let kept = protected(&f);
        let original_terminal = terminal(&f);
        let frame = s.completed_cells_end();
        refresh(&mut s, mouse);
        s.wait_for_command_count(&f, steps.len());
        failed(&mut s, frame);
        let (source, _) = reservation(&f);
        let mut expected = snapshot(&f);
        if clear {
            // Independently derive EVERY permitted Clear row change, no
            // observed post-Clear state adopted as a new expected baseline.
            for (table, rows) in &mut expected {
                if matches!(
                    table.as_str(),
                    "records" | "checkpoints" | "checkpoint_records"
                ) {
                    rows.retain(|row| !(row["host"] == HOST && row["login"] == "user-id:42"));
                }
                if table == "sync_reservations" {
                    for row in rows.iter_mut() {
                        if row["host"] == HOST && row["login"] == "user-id:42" {
                            row.insert(
                                "generation".into(),
                                json!(row["generation"].as_i64().unwrap().saturating_add(1)),
                            );
                            row.insert("active".into(), json!(0));
                        }
                    }
                }
                if table == "sync_resource_bindings" {
                    for row in rows.iter_mut() {
                        if row["host"] == HOST && row["login"] == "user-id:42" {
                            row.insert("active".into(), json!(0));
                            row.insert("visible_generation".into(), Value::Null);
                        }
                    }
                }
                rows.sort_by_key(|r| serde_json::to_string(r).unwrap());
            }
            Cache::open(&database(&f))
                .unwrap()
                .clear_records(&Scope::github(HOST, Some(42)).unwrap())
                .unwrap();
        } else {
            for row in expected.get_mut("sync_reservations").unwrap().iter_mut() {
                if row["source"] == source {
                    row.insert(
                        "generation".into(),
                        json!(row["generation"].as_u64().unwrap() + 1),
                    );
                }
            }
            expected
                .get_mut("sync_reservations")
                .unwrap()
                .sort_by_key(|r| serde_json::to_string(r).unwrap());
            // raw() remains the independent READ_ONLY observer. Only this
            // deliberate external source-generation perturbation is writable;
            // absence must not create a substitute database.
            let external = rusqlite::Connection::open_with_flags(
                database(&f),
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE,
            )
            .unwrap();
            assert_eq!(external.execute("UPDATE sync_reservations SET generation=generation+1 WHERE host=?1 AND login='user-id:42' AND source=?2",[HOST,source.as_str()]).unwrap(),1);
        }
        assert_eq!(snapshot(&f), expected);
        let frame = s.completed_cells_end();
        refresh(&mut s, mouse);
        form_frame(&mut s, frame, RETURN, &["Back and reopen", BACK]);
        assert_eq!(commands(&s, &f).len(), steps.len());
        assert_eq!(snapshot(&f), expected);
        let frame = s.completed_cursorless_end();
        control(&mut s, mouse, RETURN, BACK, RETURN_CONTROLS);
        s.wait_for_new_cursorless_condition(
            frame,
            |screen| {
                let text = screen.contents();
                crate::support::main_footer_visible(screen)
                    && text.contains("Cached Inbox only")
                    && !text.contains(DETAIL_BODY)
                    && !text.contains(BODY)
                    && !text.contains(REFRESH)
            },
            "Pure Back strips old PR; no rebase or private revival",
            Duration::from_secs(8),
        );
        assert_eq!(snapshot(&f), expected);
        finish(
            s,
            &f,
            mouse,
            &steps,
            EndState {
                git,
                config,
                kept,
                terminal: original_terminal,
            },
        );
    }
}

#[test]
fn conversation_refresh_retry_v20_personal_keyboard_repeated_failed_reads_then_fresh_page() {
    repeated(false)
}
#[test]
fn conversation_refresh_retry_v20_personal_mouse_repeated_failed_reads_then_fresh_page() {
    repeated(true)
}
#[test]
fn conversation_refresh_retry_v20_personal_keyboard_cancel_settles_then_explicit_new_read() {
    canceled(false)
}
#[test]
fn conversation_refresh_retry_v20_personal_mouse_cancel_settles_then_explicit_new_read() {
    canceled(true)
}
#[test]
fn conversation_refresh_retry_v20_personal_keyboard_clear_or_newer_source_never_rebases() {
    invalidated(false)
}
#[test]
fn conversation_refresh_retry_v20_personal_mouse_clear_or_newer_source_never_rebases() {
    invalidated(true)
}
