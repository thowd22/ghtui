//! Actual binary, strict original transport and the SAME DraftWriter worker.
//! The owned final UID is stopped only after its exact journal child appears.
use super::*;
use crate::support::owned_pty_processes_v1::ProcessIdentity;

pub(crate) struct HeldUidV85 {
    identity: ProcessIdentity,
    stopped: bool,
}
impl HeldUidV85 {
    pub(crate) fn capture(s: &mut Session, f: &Fixture, expected: &[GhStep]) -> Self {
        s.wait_for_command_count(f, expected.len());
        exact(s, f, expected);
        let row = commands(s, f).pop().unwrap();
        let pid = i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
        let identity = process_identity(pid).unwrap().unwrap();
        let owner =
            OwnedPtySession::capture(u32::try_from(s.owned_application_pid()).unwrap()).unwrap();
        assert!(owner.owns(&identity));
        assert!(!identity.zombie);
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        Self {
            identity,
            stopped: true,
        }
    }
    pub(crate) fn release(&mut self) {
        let now = process_identity(self.identity.pid).unwrap().unwrap();
        assert_eq!(
            (now.pid, now.uid, now.birth, now.session),
            (
                self.identity.pid,
                self.identity.uid,
                self.identity.birth,
                self.identity.session
            )
        );
        assert!(!now.zombie);
        assert!(self.stopped);
        assert_eq!(unsafe { libc::kill(now.pid, libc::SIGCONT) }, 0);
        self.stopped = false;
    }
    pub(crate) fn reaped(&self, s: &mut Session) {
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            match process_identity(self.identity.pid) {
                Ok(None) => return,
                Ok(Some(now))
                    if (now.uid, now.birth, now.session)
                        != (
                            self.identity.uid,
                            self.identity.birth,
                            self.identity.session,
                        ) =>
                {
                    return
                }
                Ok(Some(_)) => {}
                Err(e) if e.kind() == std::io::ErrorKind::InvalidData => {}
                Err(e) => panic!("Exact held final UID identity: {e}"),
            }
            assert!(Instant::now() < until, "Exact final UID must be reaped");
            s.poll_output(Duration::from_millis(2));
        }
    }
}
impl Drop for HeldUidV85 {
    fn drop(&mut self) {
        if self.stopped {
            if let Ok(Some(now)) = process_identity(self.identity.pid) {
                if (now.uid, now.birth, now.session)
                    == (
                        self.identity.uid,
                        self.identity.birth,
                        self.identity.session,
                    )
                    && !now.zombie
                {
                    unsafe { libc::kill(now.pid, libc::SIGCONT) };
                }
            }
        }
    }
}
pub(crate) fn full_sql_v85(f: &Fixture) -> Vec<String> {
    let db = raw(f);
    dump_v85(&db)
}
fn dump_v85(db: &rusqlite::Connection) -> Vec<String> {
    let names = db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
        .unwrap().query_map([], |r| r.get::<_, String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let mut result = db.prepare("SELECT type,name,sql FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY type,name")
        .unwrap().query_map([],|r|Ok(format!("schema:{:?}",(r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?))))
        .unwrap().map(Result::unwrap).collect::<Vec<_>>();
    for name in names {
        let mut q = db.prepare(&format!("SELECT * FROM {name}")).unwrap();
        let count = q.column_count();
        let mut rows = q
            .query_map([], |r| {
                Ok((0..count)
                    .map(|n| format!("{:?}", r.get_ref(n).unwrap()))
                    .collect::<Vec<_>>())
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        rows.sort();
        result.extend(rows.into_iter().map(|r| format!("{name}:{r:?}")));
    }
    result
}
pub(crate) fn privacy_oracle_v85(f: &Fixture, kinds: &[&str]) -> Vec<String> {
    // Independent business oracle, never production Cache/fence code. Copy
    // this known fixture's COMPLETE typed rows+schema into an isolated memory
    // DB, then model only the documented exact privacy fence. No canonical DB
    // changes and no broad allowance for arbitrary derived-table differences.
    let source = raw(f);
    let expected = rusqlite::Connection::open_in_memory().unwrap();
    expected.execute_batch("PRAGMA foreign_keys=OFF;").unwrap();
    let schema = source.prepare("SELECT type,name,sql FROM sqlite_master WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%' ORDER BY type,name")
        .unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?)))
        .unwrap().map(Result::unwrap).collect::<Vec<_>>();
    for (_, name, sql) in schema.iter().filter(|r| r.0 == "table") {
        expected.execute_batch(sql).unwrap();
        let mut q = source.prepare(&format!("SELECT * FROM {name}")).unwrap();
        let columns = q.column_count();
        let insert = format!(
            "INSERT INTO {name} VALUES({})",
            vec!["?"; columns].join(",")
        );
        let rows = q
            .query_map([], |r| {
                (0..columns)
                    .map(|n| r.get::<_, rusqlite::types::Value>(n))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .unwrap();
        for row in rows {
            expected
                .execute(&insert, rusqlite::params_from_iter(row.unwrap()))
                .unwrap();
        }
    }
    for (_, _, sql) in schema.iter().filter(|r| r.0 != "table") {
        expected.execute_batch(sql).unwrap();
    }
    expected.execute_batch("PRAGMA foreign_keys=ON;").unwrap();
    assert_eq!(
        expected
            .prepare("PRAGMA foreign_key_check")
            .unwrap()
            .query_map([], |_| Ok(()))
            .unwrap()
            .count(),
        0
    );
    assert_eq!(dump_v85(&expected), dump_v85(&source));
    let attempt: i64 = source
        .query_row(
            "SELECT attempt_id FROM pr_write_attempts WHERE state='confirmed'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(attempt > 0);
    for kind in kinds {
        let previous:Option<i64> = source.query_row("SELECT mutation_ordinal FROM sync_mutation_fences WHERE host=?1 AND login='user-id:42' AND namespace='collaboration' AND repository=?2 AND resource='GENUINE_CANONICAL_PR_NODE_1007' AND kind=?3",[HOST,FIRST,*kind],|r|r.get(0)).optional().unwrap();
        assert!(previous.is_none_or(|n| n < attempt));
        expected.execute("INSERT INTO sync_mutation_fences VALUES(?1,'user-id:42','collaboration',?2,'GENUINE_CANONICAL_PR_NODE_1007',?3,?4) ON CONFLICT(host,login,namespace,repository,resource,kind) DO UPDATE SET mutation_ordinal=excluded.mutation_ordinal",rusqlite::params![HOST,FIRST,kind,attempt]).unwrap();
        expected.execute("UPDATE sync_resource_bindings SET visible_generation=NULL WHERE host=?1 AND login='user-id:42' AND namespace='collaboration' AND repository=?2 AND resource='GENUINE_CANONICAL_PR_NODE_1007' AND repository_id=10000 AND kind=?3",[HOST,FIRST,*kind]).unwrap();
    }
    expected.execute("UPDATE sync_reservations SET generation=CASE WHEN generation<9223372036854775807 THEN generation+1 ELSE generation END,active=0 WHERE host=?1 AND login='user-id:42'",[HOST]).unwrap();
    expected
        .execute(
            "UPDATE sync_resource_bindings SET active=0 WHERE host=?1 AND login='user-id:42'",
            [HOST],
        )
        .unwrap();
    dump_v85(&expected)
}
pub(crate) fn protected_sql_v85(rows: &[String]) -> Vec<&String> {
    // Privacy-only fallback may invalidate derived source/binding rows. It
    // must never change any draft, terminal, receipt or opposite-origin row.
    rows.iter()
        .filter(|r| r.starts_with("drafts:") || r.starts_with("pr_write_"))
        .collect()
}
fn run(mouse: bool) {
    for personal in [false, true] {
        for uid in [42, 43] {
            let command = PrCommand::NewComment;
            let mut steps = initial_steps(personal);
            let initial = steps.len();
            steps.extend(phase(command, true, Some(true)));
            let opened = steps.len();
            steps.extend(phase(command, false, Some(true)));
            let previewed = steps.len();
            steps.extend(phase(command, false, Some(true)));
            steps.extend(receipt(command));
            let mut paid = visibility();
            let last = paid.last_mut().unwrap();
            *last = get("user", json!({"id":uid,"login":"octocat"}));
            last.delay_ms = 2000;
            steps.extend(paid);
            let f = resource_configured(&steps);
            seed_independent(&f);
            let git = git_state(&f);
            let config = fs::read(f.root.join("config/ghtui/config.toml")).unwrap();
            let mut s = launch(&f, mouse);
            enter(
                &mut s,
                &f,
                personal,
                mouse,
                initial,
                false,
                &steps[..initial],
            );
            let kept = protected(&f);
            let stable = stable_sql(&f);
            prepare(&mut s, &f, mouse, command, opened, previewed);
            control(&mut s, mouse, &title(command), "[Confirm action]", EDITABLE);
            let mut held = HeldUidV85::capture(&mut s, &f, &steps);
            let db = raw(&f);
            let (key, body):(String,String) = db.query_row("SELECT resource,body FROM drafts WHERE resource LIKE 'conversation-draft-v17/%'", [], |r|Ok((r.get(0)?,r.get(1)?))).unwrap();
            let envelope: Value = serde_json::from_str(&body).unwrap();
            assert_eq!(envelope["values"], json!({"body":BODY}));
            assert!(envelope["publication"].is_object());
            assert!(envelope.get("approval").is_none());
            confirm_receipts(&f, &[("conversation-create-v17", "901")]);
            let before = full_sql_v85(&f);
            let privacy = (uid == 43).then(|| privacy_oracle_v85(&f, &["conversation_comment"]));
            assert!(Cache::open(&database(&f))
                .unwrap()
                .sync_frame(
                    &Scope::github(HOST, Some(42)).unwrap(),
                    "pr-write-projection:GENUINE_CANONICAL_PR_NODE_1007"
                )
                .unwrap()
                .is_none());
            held.release();
            if uid == 42 {
                wait_fresh_page_v19(&mut s);
                assert_published_transition_v19(&f, &stable, command, 1);
                assert_eq!(
                    db.query_row(
                        "SELECT count(*) FROM drafts WHERE resource=?1",
                        [&key],
                        |r| r.get::<_, u64>(0)
                    )
                    .unwrap(),
                    0
                );
                assert_ne!(full_sql_v85(&f), before);
            } else {
                s.wait_for_cursorless_condition(
                    |frame| {
                        crate::support::main_footer_visible(frame)
                            && frame
                                .contents()
                                .contains("Confirmed; visibility unavailable; no resend")
                            && !frame.contents().contains(BODY)
                            && !frame.contents().contains(DETAIL_BODY)
                            && !frame.contents().contains("[Confirm action]")
                            && !frame.contents().contains("[r Refresh PR]")
                    },
                    "Actual final UID loss cannot delete or admit the private projection",
                    Duration::from_secs(8),
                );
                assert_eq!(
                    protected_sql_v85(&full_sql_v85(&f)),
                    protected_sql_v85(&before)
                );
                assert_eq!(
                    full_sql_v85(&f),
                    privacy.unwrap(),
                    "Only independently modelled original privacy fencing is permitted"
                );
                assert_eq!(
                    db.query_row("SELECT body FROM drafts WHERE resource=?1", [&key], |r| {
                        r.get::<_, String>(0)
                    })
                    .unwrap(),
                    body
                );
                assert!(Cache::open(&database(&f))
                    .unwrap()
                    .sync_frame(
                        &Scope::github(HOST, Some(42)).unwrap(),
                        "pr-write-projection:GENUINE_CANONICAL_PR_NODE_1007"
                    )
                    .unwrap()
                    .is_none());
            }
            held.reaped(&mut s);
            assert_eq!(protected(&f), kept);
            assert_eq!(git_state(&f), git);
            assert_eq!(
                fs::read(f.root.join("config/ghtui/config.toml")).unwrap(),
                config
            );
            exact(&s, &f, &steps);
            assert_read_only_git(&f);
            s.capture_completed_terminal_v1(
                "conversation-atomic-v85-final-uid-exact-draft-projection",
            );
            s.quit_and_assert_restored_terminal(mouse);
        }
    }
}
#[test]
fn conversation_refresh_v19_atomic_keyboard_both_originals_held_final_uid_success_and_loss_exact_sql(
) {
    run(false)
}
#[test]
fn conversation_refresh_v19_atomic_mouse_both_originals_held_final_uid_success_and_loss_exact_sql()
{
    run(true)
}
