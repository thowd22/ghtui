//! Actual held successful POST/readback plus real Background replacement.
//! The pending native Form intentionally exposes Esc only: generic replacement
//! is tested here, NOT invented as a user-reachable PTY action.
use super::*;
use crate::owned_pty_processes_test_support as process;
use crate::{
    app::{Overlay, View},
    runtime::{scoped_inline_test_support_v1 as channel, Background, PreparedView},
    scoped_pr_collaboration_native_v17 as native,
};
use std::sync::mpsc;
#[path = "scoped_pr_collaboration_close_v20_tests.rs"]
mod close_v20;

#[derive(Debug, PartialEq, Eq)]
enum ReapProbeV105 {
    Gone,
    StillOwned,
    #[cfg(target_os = "linux")]
    Unresolved,
}

fn reap_probe_v105(
    observed: std::io::Result<Option<process::ProcessIdentity>>,
    original: &process::ProcessIdentity,
) -> std::io::Result<ReapProbeV105> {
    match observed {
        Ok(None) => Ok(ReapProbeV105::Gone),
        Ok(Some(now))
            if (now.pid, now.uid, now.birth, now.session)
                == (original.pid, original.uid, original.birth, original.session) =>
        {
            // A zombie is still present: only explicit kernel absence is done.
            Ok(ReapProbeV105::StillOwned)
        }
        Ok(Some(_)) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Cleanup observed a foreign process identity",
        )),
        #[cfg(target_os = "linux")]
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
            // /proc can be read while the task's ownership fields retire.
            // This is UNKNOWN, not absence or permission to signal. Keep the
            // strict shared parser and require a subsequent explicit absence.
            Ok(ReapProbeV105::Unresolved)
        }
        Err(error) => Err(error),
    }
}

#[test]
fn conversation_comments_v105_reap_probe_requires_explicit_absence_not_invalid_or_zombie() {
    let original = process::process_identity(std::process::id() as i32)
        .unwrap()
        .unwrap();
    assert_eq!(
        reap_probe_v105(Ok(Some(original.clone())), &original).unwrap(),
        ReapProbeV105::StillOwned
    );
    let mut zombie = original.clone();
    zombie.zombie = true;
    assert_eq!(
        reap_probe_v105(Ok(Some(zombie)), &original).unwrap(),
        ReapProbeV105::StillOwned
    );
    #[cfg(target_os = "linux")]
    assert_eq!(
        reap_probe_v105(
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid stat ownership fields"
            )),
            &original
        )
        .unwrap(),
        ReapProbeV105::Unresolved
    );
    for field in 0..4 {
        let mut foreign = original.clone();
        match field {
            0 => foreign.pid += 1,
            1 => foreign.uid += 1,
            2 => foreign.birth.0 += 1,
            _ => foreign.session += 1,
        }
        assert!(reap_probe_v105(Ok(Some(foreign)), &original).is_err());
    }
    let error = reap_probe_v105(
        Err(std::io::Error::from_raw_os_error(libc::EACCES)),
        &original,
    )
    .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(libc::EACCES));
    assert_eq!(
        reap_probe_v105(Ok(None), &original).unwrap(),
        ReapProbeV105::Gone
    );
}

#[test]
fn conversation_comments_v18_prepared_confirm_resize_refuses_before_ack_or_slot_then_new_confirm_succeeds(
) {
    let command = PrCommand::NewComment;
    let mut tail = phase(command, false, Some(true));
    tail.extend(receipts(command));
    tail.extend(visibility());
    let (mut s, form, _) = prepared(command, tail);
    let source = ui::request(&form).unwrap().intent().source.clone();
    let key = form.resource();
    let scope = form.scope();
    let encoded = ui::encoded_draft(&form).unwrap();
    let mut writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    writer
        .save(scope.clone(), key.clone(), encoded.clone())
        .unwrap();
    // A genuine same-client completed ACK witnesses that rejected Background
    // admission did not even replace the active publication-capture ticket.
    let probe_token = CancellationToken::default();
    let probe_deadline = Instant::now() + Duration::from_secs(30);
    let probe = client
        .capture_publication_v2(
            &database(&s.fixture),
            &scope,
            &key,
            &probe_token,
            probe_deadline,
        )
        .unwrap();
    let ack = client
        .save_publication_and_wait_v2(probe, encoded.clone(), &probe_token)
        .unwrap();
    s.app.form = Some(form);
    s.app.overlay = Some(Overlay::Form);
    s.app
        .dispatch(crate::app::Action::Form(crate::forms::FormAction::Submit));
    native::prepare_requested(&mut s.app);
    let old = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert_eq!(old.phase(), native::ConversationPhaseKindV17::Confirm);
    assert!(old.matches(&s.app));
    s.app.handle_event(crossterm::event::Event::Resize(40, 10));
    assert!(!old.matches(&s.app));
    let before_sql = sql(&s.fixture);
    let before_commands = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    s.app.status = "ORIGINAL_NO_SLOT_EFFECT".into();
    let mut background = Background::new();
    assert!(!background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        old.clone(),
        s.api.clone(),
        database(&s.fixture),
        client.clone()
    ));
    assert_eq!(s.app.status, "ORIGINAL_NO_SLOT_EFFECT");
    assert!(!s.app.loading[View::Inbox.index()]);
    assert!(channel::generation(&background).is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    assert!(s.app.scoped_pr_collaboration_work_v17.is_none());
    assert_eq!(sql(&s.fixture), before_sql);
    assert_eq!(
        fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        before_commands
    );
    ack.check_current(&client, &database(&s.fixture), &scope, &key, &probe_token)
        .unwrap();
    assert_eq!(s.app.form.as_ref().unwrap().publication, None);
    assert!(Arc::ptr_eq(
        &ui::request(s.app.form.as_ref().unwrap())
            .unwrap()
            .intent()
            .source,
        &source
    ));
    s.app.handle_event(crossterm::event::Event::Resize(120, 40));
    rendered(&mut s.app);
    native::prepare_requested(&mut s.app);
    assert!(
        s.app.scoped_pr_collaboration_work_v17.is_none(),
        "Repaint is never another explicit Confirm"
    );
    s.app
        .dispatch(crate::app::Action::Form(crate::forms::FormAction::Submit));
    native::prepare_requested(&mut s.app);
    let current = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(!Arc::ptr_eq(&current, &old));
    assert!(current.matches(&s.app));
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        current,
        s.api.clone(),
        database(&s.fixture),
        client
    ));
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        background.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Fresh separately confirmed original worker did not finish"
        );
        std::thread::yield_now();
    }
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_attempts WHERE state='confirmed'",
            [],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM drafts WHERE resource=?1",
            [&key],
            |r| r.get::<_, usize>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    assert_eq!(
        fs::read(s.fixture.repo().join(".git/index")).unwrap(),
        s.git[1]
    );
    assert_eq!(
        fs::read(s.fixture.repo().join(".git/config")).unwrap(),
        s.git[2]
    );
    assert_eq!(fs::read(s.fixture.repo().join("kept")).unwrap(), s.git[3]);
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let rows = rows
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), s.steps.len());
    assert!(rows.iter().all(|r| r["matched"] == true));
    assert_eq!(
        rows.iter()
            .filter(
                |r| r["args"].as_array().unwrap().iter().any(|a| a == "POST")
                    && r["args"][1] != "graphql"
            )
            .count(),
        1
    );
}

#[test]
fn conversation_comments_v18_held_original_inbox_does_not_drop_independent_repository_completion() {
    let command = PrCommand::NewComment;
    let mut tail = phase(command, false, Some(true));
    tail[3].delay_ms = 30_000;
    tail.truncate(4);
    let (mut s, form, _) = prepared(command, tail);
    let source = ui::request(&form).unwrap().intent().source.clone();
    let key = form.resource();
    let encoded = ui::encoded_draft(&form).unwrap();
    let mut writer = DraftWriter::new(database(&s.fixture));
    writer
        .save(form.scope(), key.clone(), encoded.clone())
        .unwrap();
    s.app.form = Some(form);
    s.app.overlay = Some(Overlay::Form);
    s.app
        .dispatch(crate::app::Action::Form(crate::forms::FormAction::Submit));
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    let mut background = Background::new();
    assert!(background.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work.clone(),
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    let owner = s.app.scoped_pr_collaboration_owner_v17.clone().unwrap();
    let generation = channel::generation(&background).unwrap();
    let token = owner.cancellation().clone();
    let index = s.steps.iter().position(|s| s.delay_ms == 30_000).unwrap();
    let held = held_call(&s, index);
    let sql_before = sql(&s.fixture);
    assert!(
        background.submit(&mut s.app, View::Repository, |_| Ok(PreparedView {
            lines: vec!["ACTUAL_INDEPENDENT_REPOSITORY_RESULT".into()],
            status: "Independent Repository completion".into(),
            ..Default::default()
        }))
    );
    let until = Instant::now() + Duration::from_secs(5);
    while s.app.loading[View::Repository.index()] {
        background.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Independent view completion was dropped by the Inbox lease"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        s.app.content[View::Repository.index()],
        ["ACTUAL_INDEPENDENT_REPOSITORY_RESULT"]
    );
    assert!(
        !background.request_cancel(View::Repository),
        "The completed other-view physical slot must actually be removed"
    );
    assert!(s.app.loading[View::Inbox.index()]);
    assert_eq!(channel::generation(&background), Some(generation));
    assert!(Arc::ptr_eq(
        s.app.scoped_pr_collaboration_owner_v17.as_ref().unwrap(),
        &owner
    ));
    assert!(!token.is_cancelled());
    assert!(work.matches(&s.app));
    assert!(Arc::ptr_eq(
        &ui::request(s.app.form.as_ref().unwrap())
            .unwrap()
            .intent()
            .source,
        &source
    ));
    assert_eq!(
        ui::encoded_draft(s.app.form.as_ref().unwrap()).unwrap(),
        encoded
    );
    assert_eq!(sql(&s.fixture), sql_before);
    assert!(process::process_identity(held.pid)
        .unwrap()
        .is_some_and(|now| now.birth == held.birth
            && now.uid == held.uid
            && now.session == held.session
            && !now.zombie));
    assert!(background.cancel_scoped_pr_collaboration_v17(&mut s.app));
    let until = Instant::now() + Duration::from_secs(5);
    while reap_probe_v105(process::process_identity(held.pid), &held).unwrap()
        != ReapProbeV105::Gone
    {
        assert!(
            Instant::now() < until,
            "Exact original held child was not reaped"
        );
        std::thread::yield_now();
    }
    let rows = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let rows = rows
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), s.steps.len());
    assert!(rows.iter().all(|r| r["matched"] == true));
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    assert_eq!(
        fs::read(s.fixture.repo().join(".git/index")).unwrap(),
        s.git[1]
    );
    assert_eq!(
        fs::read(s.fixture.repo().join(".git/config")).unwrap(),
        s.git[2]
    );
    assert_eq!(fs::read(s.fixture.repo().join("kept")).unwrap(), s.git[3]);
}

fn held_call(s: &Setup, index: usize) -> process::ProcessIdentity {
    let until = Instant::now() + Duration::from_secs(6);
    loop {
        let lines = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap_or_default();
        if let Some(line) = lines.lines().nth(index) {
            let row: Value = serde_json::from_str(line).unwrap();
            assert_eq!(row["matched"], true);
            let pid = i32::try_from(row["pid"].as_u64().unwrap()).unwrap();
            let actual = process::process_identity(pid).unwrap().unwrap();
            assert_eq!(actual.uid, unsafe { libc::geteuid() });
            assert_eq!(actual.session, unsafe { libc::getsid(0) });
            assert!(!actual.zombie);
            return actual;
        }
        assert!(
            Instant::now() < until,
            "Original actual readback was never launched: {lines}"
        );
        std::thread::yield_now();
    }
}
fn rendered(app: &mut App) -> String {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
fn replacement(mode: u8) {
    let command = PrCommand::NewComment;
    let mut tail = phase(command, false, Some(true));
    tail.push(receipts(command).remove(0));
    let mut read = receipts(command).remove(1);
    read.delay_ms = 30_000;
    tail.push(read);
    // Modes 0/1/2 finish a genuine original Summary-history Core after the
    // replacement is released. No source is minted from a DTO or old result.
    if mode <= 2 {
        tail.extend(parent_phase());
    }
    let (mut s, form, _) = prepared(command, tail);
    let mut independent = if mode == 4 {
        Some(fixture(command, vec![]))
    } else {
        None
    };
    s.app.overlay = Some(Overlay::Form);
    s.app.form = Some(form);
    let body_pointer = s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .details
        .summary
        .pr
        .body
        .as_ptr();
    let rows_pointer = s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .rows
        .as_ptr();
    let mut writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    writer
        .save(
            s.app.form.as_ref().unwrap().scope(),
            s.app.form.as_ref().unwrap().resource(),
            ui::encoded_draft(s.app.form.as_ref().unwrap()).unwrap(),
        )
        .unwrap();
    s.app.form.as_mut().unwrap().submit_requested = true;
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    let mut bg = Background::new();
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        client.clone()
    ));
    let old = s.app.scoped_pr_collaboration_owner_v17.clone().unwrap();
    let index = s
        .steps
        .iter()
        .position(|step| step.delay_ms == 30_000)
        .unwrap();
    let held = held_call(&s, index);
    let original_body = Cache::open(&database(&s.fixture))
        .unwrap()
        .draft(
            old.original().scope(),
            &old.work().request().unwrap().intent().key,
        )
        .unwrap()
        .unwrap();
    let key = old.work().request().unwrap().intent().key.clone();
    let (released, release) = mpsc::sync_channel(1);
    let (started, start) = mpsc::sync_channel(1);
    let api = s.api.clone();
    let path = database(&s.fixture);
    let native::CollaborationWorkOriginV78::PrPage(original) = old.original() else {
        panic!("This actual Conversation replacement requires its genuine PR-page origin");
    };
    let original = Arc::clone(original);
    assert!(bg.submit(&mut s.app,View::Inbox,move|cancel|{
        started.send(cancel.clone()).unwrap();release.recv_timeout(Duration::from_secs(8)).unwrap();
        if mode<=2 {
            let deadline=Instant::now()+Duration::from_secs(30);
            let bounded=api.clone().with_original_pr_read_bounds_v13(deadline,1024*1024).unwrap();
            crate::scoped_pr_collaboration_source_v17::with_history_read_v17(original,&bounded,&path,&cancel,deadline,
                |read| {
                    let target=crate::pr_write_journal::PrWriteTarget::new(read.identity,read.parent.summary()).unwrap();
                    let snapshot=PrWriteJournal::open(&path).unwrap().conversation_history_v17(&target,read.parent).unwrap();
                    Ok((target,read.parent.clone(),snapshot))
                },|(target,parent,snapshot),boundary|crate::scoped_pr_observed_tabs_v1::resources::ResourceLocalGuard::validate_conversation_history_v17(&path,boundary,target,parent,snapshot)).unwrap();
        }
        match mode {1=>Err("ACTUAL_REPLACEMENT_ERROR".into()),2=>panic!("actual caught replacement panic"),
            _=>Ok(PreparedView{status:"ACTUAL_REPLACEMENT_SAFE".into(),..Default::default()})}
    }));
    let next_token = start.recv_timeout(Duration::from_secs(5)).unwrap();
    let generation = channel::generation(&bg).unwrap();
    s.app.status = "NEWER_SLOT_STATUS".into();
    if mode == 4 {
        s.app = std::mem::take(&mut independent.as_mut().unwrap().app);
        s.app.status = "NEW_SOURCE_STATUS".into();
        s.app.loading[View::Inbox.index()] = true;
    }
    if mode == 5 {
        let next = s
            .app
            .github
            .take()
            .unwrap()
            .load_tab(PrTab::Conversation, &s.api, &CancellationToken::default())
            .unwrap();
        s.app.github = Some(next);
        s.app.status = "NEW_NAV_STATUS".into();
    }
    if mode == 6 {
        s.app.overlay = Some(Overlay::Notice);
        s.app.notice = "NEW_NOTICE_BODY".into();
        s.app.hits = vec![crate::app::HitRegion {
            rect: ratatui::layout::Rect::new(3, 4, 8, 1),
            action: crate::app::Action::Close,
        }];
    }
    if mode == 7 {
        s.app.github.as_mut().unwrap().identity.account_id = Some(43);
    }
    let before_status = s.app.status.clone();
    let before_page = s
        .app
        .github
        .as_ref()
        .and_then(|v| v.pr.as_ref())
        .map(|p| p.details.summary.pr.body.as_ptr());
    bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
    assert_eq!(channel::generation(&bg), Some(generation));
    assert!(!next_token.is_cancelled());
    assert!(s.app.loading[View::Inbox.index()]);
    assert_eq!(s.app.status, before_status);
    if mode <= 3 || mode == 6 {
        assert!(native::retired_presentation(&s.app));
        assert!(s.app.form.is_none());
        let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
        assert_eq!(page.details.summary.pr.body.as_ptr(), body_pointer);
        assert_eq!(page.rows.as_ptr(), rows_pointer);
        if mode == 6 {
            assert_eq!(s.app.overlay, Some(Overlay::Notice));
            assert_eq!(s.app.hits.len(), 1);
            assert_eq!(s.app.hits[0].rect, ratatui::layout::Rect::new(3, 4, 8, 1));
        }
        let screen = rendered(&mut s.app);
        assert!(!screen.contains(TEXT));
        assert!(!screen.contains(crate::personal_pr_entry_v13::tests::BODY));
        assert!(PrCollaborationOpenIntentV17::capture(&s.app, &database(&s.fixture)).is_err());
    } else if mode == 7 {
        assert!(s.app.github.is_none());
        assert!(s.app.form.is_none());
        assert!(!s
            .app
            .context
            .contains(crate::personal_pr_entry_v13::tests::BODY));
        assert!(!rendered(&mut s.app).contains(crate::personal_pr_entry_v13::tests::BODY));
    } else {
        assert!(!native::retired_presentation(&s.app));
        assert_eq!(
            s.app.github.as_ref().and_then(|v| v.pr.as_ref()).map(|p| p
                .details
                .summary
                .pr
                .body
                .as_ptr()),
            before_page
        );
    }
    if mode == 3 {
        bg.cancel(View::Inbox);
        s.app.status = "AFTER_SUCCESSOR_REMOVED".into();
    }
    let expected_status = s.app.status.clone();
    let until = Instant::now() + Duration::from_secs(5);
    while reap_probe_v105(process::process_identity(held.pid), &held).unwrap()
        != ReapProbeV105::Gone
    {
        assert!(Instant::now() < until);
        bg.poll(&mut s.app);
        std::thread::yield_now();
    }
    // A genuinely late old completion cannot erase replacement controls/status
    // even if its physical slot was removed independently.
    bg.poll(&mut s.app);
    assert_eq!(s.app.status, expected_status);
    assert_eq!(
        channel::generation(&bg),
        if mode == 3 { None } else { Some(generation) }
    );
    assert!(!next_token.is_cancelled() || mode == 3);
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(old.original().scope(), &key)
            .unwrap()
            .as_deref(),
        Some(original_body.as_str())
    );
    released.send(()).unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    while channel::generation(&bg).is_some() {
        bg.poll(&mut s.app);
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    if mode == 3 {
        assert_eq!(s.app.status, "AFTER_SUCCESSOR_REMOVED");
    }
    if mode <= 2 {
        assert!(!rendered(&mut s.app).contains(crate::personal_pr_entry_v13::tests::BODY));
    }
    let connection = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |r| r
                .get::<_, usize>(0))
            .unwrap(),
        1
    );
    assert!(matches!(
        connection
            .query_row("SELECT state FROM pr_write_attempts", [], |r| r
                .get::<_, String>(0))
            .unwrap()
            .as_str(),
        "pending" | "uncertain"
    ));
    exact_protocol(&s);
}
#[test]
fn conversation_comments_v17_runtime_held_post_success_err_actual_panic_replacement() {
    for mode in 0..=2 {
        replacement(mode)
    }
}
#[test]
fn conversation_comments_v17_runtime_held_post_removed_successor_late_completion() {
    replacement(3)
}
#[test]
fn conversation_comments_v17_runtime_held_post_independently_newer_source_preserved() {
    replacement(4)
}
#[test]
fn conversation_comments_v17_runtime_held_post_same_source_genuine_new_navigation_preserved() {
    replacement(5)
}
#[test]
fn conversation_comments_v17_runtime_held_post_newer_notice_controls_preserved() {
    replacement(6)
}
#[test]
fn conversation_comments_v17_runtime_held_post_current_uid_loss_hides_private_even_lost_slot() {
    replacement(7)
}
