//! Deterministic real-binary proof: actual metadata GET held at a verified PID,
//! original Summary scrolling occurs first, then the actual tab completes.
use super::*;
use crate::support::owned_pty_processes_v1::{process_identity, OwnedPtySession, ProcessIdentity};
struct Held {
    pid: i32,
    stopped: bool,
    identity: ProcessIdentity,
    session: OwnedPtySession,
}
impl Held {
    fn current(&self) -> bool {
        process_identity(self.pid).is_ok_and(|current| {
            current.is_some_and(|current| {
                self.session.owns(&current)
                    && current.pid == self.identity.pid
                    && current.uid == self.identity.uid
                    && current.session == self.identity.session
                    && current.birth == self.identity.birth
                    && !current.zombie
            })
        })
    }
    fn signal(&self, signal: i32) {
        assert!(
            self.current(),
            "Original UID/birth/SID must still identify owned actual child before signal"
        );
        assert_eq!(unsafe { libc::kill(self.pid, signal) }, 0);
    }
    fn stop(session: &mut Session, fixture: &Fixture, steps: &[GhStep]) -> Self {
        session.wait_for_command_count(fixture, 16);
        let journal = records(session, fixture, 16);
        assert_eq!(journal[15]["matched"], true);
        assert_eq!(journal[15]["args"], json!(steps[15].args));
        let pid = i32::try_from(journal[15]["pid"].as_u64().unwrap()).unwrap();
        assert!(pid > 0 && pid != std::process::id() as i32);
        let owner = OwnedPtySession::capture(session.owned_application_pid() as u32).unwrap();
        let identity = process_identity(pid)
            .unwrap()
            .expect("Actual journal child must exist");
        assert!(owner.owns(&identity));
        let mut held = Self {
            pid,
            stopped: false,
            identity,
            session: owner,
        };
        held.signal(libc::SIGSTOP);
        held.stopped = true;
        session
            .try_wait_for_condition(
                |_| {
                    let out = std::process::Command::new("ps")
                        .args(["-p", &pid.to_string(), "-o", "stat="])
                        .output()
                        .unwrap();
                    out.status.success()
                        && String::from_utf8_lossy(&out.stdout)
                            .trim_start()
                            .starts_with('T')
                },
                "Actual original Metadata GET stopped",
                Duration::from_secs(8),
            )
            .unwrap();
        held
    }
    fn release(&mut self) {
        self.signal(libc::SIGCONT);
        self.stopped = false;
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        if self.stopped && self.current() {
            unsafe { libc::kill(self.pid, libc::SIGCONT) };
        }
    }
}
fn sql(fixture: &Fixture) -> Vec<String> {
    let db = rusqlite::Connection::open(cache_path(fixture)).unwrap();
    ["SELECT json_array(host,login,resource,body) FROM drafts ORDER BY host,login,resource","SELECT json_array(host,login,name,payload) FROM saved_views ORDER BY host,login,name","SELECT json_array(host,login,source,generation,complete) FROM checkpoints ORDER BY host,login,source","SELECT json_array(host,login,kind,id,payload) FROM records ORDER BY host,login,kind,id","SELECT json_array(attempt_id,state) FROM pr_write_attempts ORDER BY attempt_id","SELECT json_array(attempt_id,state) FROM actions_write_attempts ORDER BY attempt_id"].iter().flat_map(|query|db.prepare(query).unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().map(Result::unwrap).collect::<Vec<_>>()).collect()
}
fn journey(mouse: bool) {
    let fixture = fixture();
    drop(ghtui::pr_write_journal::PrWriteJournal::open(&cache_path(&fixture)).unwrap());
    drop(ghtui::actions_write_journal::ActionsWriteJournal::open(&cache_path(&fixture)).unwrap());
    let body = (0..60)
        .map(|n| format!("Summary original line {n:02}"))
        .collect::<Vec<_>>()
        .join("\n");
    let original = target("Native collaboration", &body, "main");
    let mut steps = at_target(initial(), &original);
    steps.extend(at_target(details(vec![starter()]), &original));
    steps.extend(metadata(&original));
    assert_eq!(steps.len(), 18);
    assert!(steps[15]
        .args
        .iter()
        .any(|a| a == "repos/example/repo/issues/7"));
    steps[15].delay_ms = 3000;
    fixture.scripted_gh(&steps);
    let head = fixture.git(&["rev-parse", "HEAD"]);
    let index = fs::read(fixture.repo().join(".git/index")).unwrap();
    let status = fixture.git(&["status", "--porcelain=v2", "-z"]);
    let refs = fixture.git(&["show-ref"]);
    let config = fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap();
    let mut session = Session::launch(&fixture, 120, 40);
    session.wait_for_screen("ghtui");
    session.send_key(b"1");
    session.assert_focused_view("Inbox");
    choose(&mut session, mouse, "[r Refresh]", b"r");
    session.wait_for_screen("Native collaboration");
    if mouse {
        session.click_visible_action("Native collaboration");
    } else {
        session.send_key(b"\r");
    }
    session.wait_for_command_count(&fixture, 14);
    session.wait_for_screen("[o Summary]");
    choose(&mut session, mouse, "[o Summary]", b"o");
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains("Summary original line 00")
                && !frame.contents().contains("[Esc Cancel]")
        },
        "Actual original Summary before tab request",
        Duration::from_secs(8),
    );
    let before = sql(&fixture);
    choose(&mut session, mouse, "[u Metadata]", b"u");
    let mut held = Held::stop(&mut session, &fixture, &steps);
    if mouse {
        for _ in 0..7 {
            session.wheel(10, 16, true);
        }
    } else {
        session.send_key(b"\x1b[6~\x1b[6~");
    }
    session.wait_for_cursorless_condition(
        |frame| {
            frame.contents().contains("Summary original line 30")
                && !frame.contents().contains("Locked conversation: false")
        },
        "Original Summary visibly scrolled while real Metadata child held",
        Duration::from_secs(8),
    );
    assert_eq!(records(&mut session, &fixture, 16).len(), 16);
    assert_eq!(sql(&fixture), before);
    let previous = session.completed_cursorless_end();
    held.release();
    session.wait_for_command_count(&fixture, 18);
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        session.poll_output(Duration::from_millis(5));
        if session.completed_cursorless_end() > previous
            && session.completed_cursorless_screen().is_some_and(|frame| {
                frame.contents().contains("Your repository permissions:")
                    && frame.contents().contains("Locked conversation: false")
                    && frame.contents().contains("Milestone: October release")
                    && !frame.contents().contains("Summary original line")
            })
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "New completed Metadata tab must start at its own first rows\n{}",
            session.screen()
        );
    }
    session
        .try_wait_for_condition(
            |_| process_identity(held.pid).is_ok_and(|current| current.is_none()),
            "Held metadata child actually reaped",
            Duration::from_secs(8),
        )
        .unwrap();
    session.resize(80, 24);
    session.wait_for_cursorless_condition(
        |frame| frame.contents().contains("Locked conversation: false"),
        "Narrow Metadata preserves visible section",
        Duration::from_secs(8),
    );
    assert_eq!(sql(&fixture), before);
    assert_eq!(fixture.git(&["show-ref"]), refs);
    assert_eq!(
        fs::read(fixture.root.join("config/ghtui/config.toml")).unwrap(),
        config
    );
    preserve_git(&fixture, &head, &index, &status);
    assert!(receipts(&fixture).is_empty());
    session.quit_and_assert_restored_terminal(mouse);
    let journal = observed_requests(&mut session, &fixture, 18);
    assert_eq!(journal.len(), 18);
    for (row, step) in journal.iter().zip(&steps) {
        assert_eq!(row["args"], json!(step.args));
        assert_eq!(row["matched"], true);
    }
}
#[test]
fn pr_metadata_viewport_v9_keyboard_held_section_read_retires_old_summary_scroll() {
    journey(false);
}
#[test]
fn pr_metadata_viewport_v9_mouse_held_section_read_retires_old_summary_scroll() {
    journey(true);
}
