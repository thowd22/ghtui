//! Real held section requests and accepted queue successors. No constructed
//! source, completion or publication. Only the read-only Metadata section
//! tolerates viewport movement; write/Form ownership remains exact.
use super::*;
use crate::{
    app::{Action, Overlay, View},
    drafts::DraftWriter,
    owned_pty_processes_test_support as process,
    runtime::{scoped_inline_test_support_v1 as channel, Background, PreparedView},
    scoped_pr_collaboration_native_v17 as native,
};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use std::{sync::mpsc, time::Duration};

type Owner = Arc<native::ScopedPrCollaborationOwnerV17>;
fn calls() -> Vec<GhStep> {
    context_steps(context_parent(), issue(), repository(), context_parent())
}
fn enqueue(s: &mut Setup, bg: &mut Background, writer: &DraftWriter) -> Owner {
    native::capture_metadata_section_v51(&mut s.app, &database(&s.fixture));
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client(),
    ));
    s.app.scoped_pr_collaboration_owner_v17.clone().unwrap()
}
fn held_pid(s: &Setup, index: usize) -> process::ProcessIdentity {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let journal = fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap_or_default();
        let rows = journal
            .split_inclusive('\n')
            .filter(|line| line.ends_with('\n'))
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        if let Some(row) = rows.get(index) {
            assert_eq!(row["step"], index);
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
            "Actual held section child: {journal}"
        );
        std::thread::yield_now();
    }
}
fn held(
    later: Vec<GhStep>,
) -> (
    Setup,
    Background,
    DraftWriter,
    Owner,
    process::ProcessIdentity,
) {
    let mut first = calls();
    first.last_mut().unwrap().delay_ms = 30_000;
    let index = opening().len() + first.len() - 1;
    first.extend(later);
    let mut s = setup(first);
    let writer = DraftWriter::new(database(&s.fixture));
    let mut bg = Background::new();
    let owner = enqueue(&mut s, &mut bg, &writer);
    let pid = held_pid(&s, index);
    assert!(owner.owns(&s.app));
    assert!(s.app.loading[View::Inbox.index()]);
    assert!(channel::generation(&bg).is_some());
    (s, bg, writer, owner, pid)
}
fn drain_child(s: &mut Setup, bg: &mut Background, pid: &process::ProcessIdentity) {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        bg.poll(&mut s.app);
        match process::process_identity(pid.pid) {
            Ok(None) => break,
            Ok(Some(now)) => {
                assert_eq!(
                    (now.pid, now.uid, now.birth, now.session),
                    (pid.pid, pid.uid, pid.birth, pid.session)
                );
            }
            #[cfg(target_os = "linux")]
            Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {}
            Err(error) => panic!("Exact owned child observation: {error}"),
        }
        assert!(
            Instant::now() < until,
            "Exact held child must disappear, not stay zombie"
        );
        std::thread::yield_now();
    }
    bg.poll(&mut s.app);
}
fn finish_read(s: &mut Setup, bg: &mut Background) {
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        bg.poll(&mut s.app);
        assert!(Instant::now() < until, "Actual new section must complete");
        std::thread::yield_now();
    }
    assert!(channel::generation(bg).is_none());
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    let page = s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
    assert_eq!(page.tab, PrTab::Metadata);
    assert!(page.metadata_display_v51.is_some());
}
fn scroll(s: &mut Setup) {
    let before = s.app.scroll;
    s.app
        .handle_event(Event::Key(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)));
    assert_ne!(
        s.app.scroll, before,
        "Actual original Summary has no interactive row"
    );
}
#[test]
fn conversation_comments_metadata_read_retirement_v70_held_scroll_clears_only_exact_slot_and_reaps_late_result(
) {
    let (mut s, mut bg, _writer, old, pid) = held(Vec::new());
    let before_sql = sql(&s.fixture);
    let page = allocations(&s.app);
    scroll(&mut s);
    assert!(
        old.owns(&s.app),
        "Same-page read-only scrolling stays owned"
    );
    let original = match old.original() {
        native::CollaborationWorkOriginV78::PrPage(original) => original,
        native::CollaborationWorkOriginV78::StateInbox(_) => {
            panic!("Actual Metadata section owner must retain its genuine PR page")
        }
    };
    assert!(
        !original.owns_presentation(&s.app),
        "The ordinary write/Form row/scroll predicate is NOT relaxed"
    );
    let generation = channel::generation(&bg);
    bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
    assert_eq!(channel::generation(&bg), generation);
    assert!(!old.cancellation().is_cancelled());
    // Preserve the historical test name and all exact-slot/cleanup oracles,
    // but supersede its old scroll-cancels contract with a genuine new tab
    // input. A pending replacement must invalidate the read before dispatch.
    s.app.dispatch(Action::Github(
        crate::github_workspace::GithubAction::PrTab(PrTab::Commits),
    ));
    assert!(!old.owns(&s.app));
    assert_eq!(
        s.app.status,
        "Verifying original Conversation • Esc Cancel • nothing resent"
    );
    let context = s.app.context.clone();
    bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
    assert!(old.cancellation().is_cancelled());
    assert!(channel::generation(&bg).is_none());
    assert!(!s.app.loading[View::Inbox.index()]);
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
    assert_eq!(
        s.app.github_action,
        Some(crate::github_workspace::GithubAction::PrTab(PrTab::Commits)),
        "Old retirement cannot consume/replay the genuine newer tab input"
    );
    assert_eq!(s.app.context, context);
    drain_child(&mut s, &mut bg, &pid);
    assert_eq!(allocations(&s.app), page);
    assert_eq!(
        s.app.github.as_ref().unwrap().pr.as_ref().unwrap().tab,
        PrTab::Summary
    );
    assert_eq!(sql(&s.fixture), before_sql);
    unchanged(&s, s.steps.len());
}
#[test]
fn conversation_comments_metadata_read_retirement_v70_readonly_scroll_keeps_actual_owner_until_genuine_section_install(
) {
    let mut tail = calls();
    tail.last_mut().unwrap().delay_ms = 1000;
    let mut s = setup(tail);
    let writer = DraftWriter::new(database(&s.fixture));
    let mut bg = Background::new();
    let owner = enqueue(&mut s, &mut bg, &writer);
    let pid = held_pid(&s, s.steps.len() - 1);
    let before_sql = sql(&s.fixture);
    let before_page = allocations(&s.app);
    let generation = channel::generation(&bg);
    scroll(&mut s);
    assert!(owner.owns(&s.app));
    let original = match owner.original() {
        native::CollaborationWorkOriginV78::PrPage(original) => original,
        native::CollaborationWorkOriginV78::StateInbox(_) => {
            panic!("Actual Metadata section owner must retain its genuine PR page")
        }
    };
    assert!(!original.owns_presentation(&s.app));
    bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
    assert!(!owner.cancellation().is_cancelled());
    assert_eq!(channel::generation(&bg), generation);
    assert_eq!(allocations(&s.app), before_page);
    assert_eq!(sql(&s.fixture), before_sql);
    finish_read(&mut s, &mut bg);
    drain_child(&mut s, &mut bg, &pid);
    assert_eq!((s.app.selected, s.app.scroll), (0, 0));
    assert_eq!(sql(&s.fixture), before_sql);
    unchanged(&s, s.steps.len());
}
#[test]
fn conversation_comments_metadata_read_retirement_v70_other_view_preserves_context_form_notice_hits_and_loading(
) {
    for notice in [false, true] {
        let (mut s, mut bg, _writer, old, pid) = held(Vec::new());
        let before_sql = sql(&s.fixture);
        let (release, wait) = mpsc::sync_channel(1);
        let (started, entered) = mpsc::sync_channel(1);
        assert!(bg.submit(&mut s.app, View::Repository, move |cancel| {
            started.send(cancel).unwrap();
            wait.recv_timeout(Duration::from_secs(8)).unwrap();
            Ok(PreparedView {
                status: "ACTUAL_REPOSITORY_SUCCESSOR".into(),
                ..Default::default()
            })
        }));
        let next = entered.recv_timeout(Duration::from_secs(5)).unwrap();
        s.app.dispatch(Action::View(View::Repository));
        let workspace = crate::workspace::Workspace::load(
            &s.fixture.repo(),
            crate::workspace::RepositoryMode::Working,
            "HEAD",
            None,
        )
        .unwrap()
        .unwrap();
        s.app.form = Some(
            crate::forms::Form::local(
                crate::local_actions::LocalCommand::CreateBranch,
                crate::local_actions::LocalContext::from_workspace(&workspace, None, None),
            )
            .unwrap(),
        );
        s.app.overlay = Some(Overlay::Form);
        if notice {
            s.app
                .dispatch(Action::Explain("ACTUAL_NEWER_NOTICE".into()));
        }
        let form = s.app.form.as_ref().unwrap().presentation_v4().0.clone();
        let overlay = s.app.overlay;
        let status = s.app.status.clone();
        let context = s.app.context.clone();
        let message = s.app.notice.clone();
        let hits = s
            .app
            .hits
            .iter()
            .map(|h| (h.rect, h.action.clone()))
            .collect::<Vec<_>>();
        bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
        drain_child(&mut s, &mut bg, &pid);
        assert!(old.cancellation().is_cancelled());
        assert!(!next.is_cancelled());
        assert!(!s.app.loading[View::Inbox.index()]);
        assert!(s.app.loading[View::Repository.index()]);
        assert_eq!(s.app.status, status);
        assert_eq!(s.app.context, context);
        assert_eq!(s.app.notice, message);
        assert_eq!(s.app.overlay, overlay);
        assert!(Arc::ptr_eq(
            s.app.form.as_ref().unwrap().presentation_v4().0,
            &form
        ));
        assert_eq!(
            s.app
                .hits
                .iter()
                .map(|h| (h.rect, h.action.clone()))
                .collect::<Vec<_>>(),
            hits
        );
        assert_eq!(sql(&s.fixture), before_sql);
        release.send(()).unwrap();
        let until = Instant::now() + Duration::from_secs(8);
        while s.app.loading[View::Repository.index()] {
            bg.poll(&mut s.app);
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
        assert!(!next.is_cancelled());
        unchanged(&s, s.steps.len());
    }
}
#[test]
fn conversation_comments_metadata_read_retirement_v70_accepted_replacement_keeps_new_generation_token_and_loading(
) {
    let (mut s, mut bg, _writer, old, pid) = held(Vec::new());
    let before_sql = sql(&s.fixture);
    let old_generation = channel::generation(&bg).unwrap();
    let (release, wait) = mpsc::sync_channel(1);
    let (started, entered) = mpsc::sync_channel(1);
    assert!(bg.submit(&mut s.app, View::Inbox, move |cancel| {
        started.send(cancel).unwrap();
        wait.recv_timeout(Duration::from_secs(8)).unwrap();
        Ok(PreparedView {
            status: "ACTUAL_NEW_INBOX_RESULT".into(),
            ..Default::default()
        })
    }));
    let next = entered.recv_timeout(Duration::from_secs(5)).unwrap();
    let generation = channel::generation(&bg).unwrap();
    assert!(generation > old_generation);
    assert!(!next.same_instance(old.cancellation()));
    let status = s.app.status.clone();
    let page = allocations(&s.app);
    bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
    drain_child(&mut s, &mut bg, &pid);
    assert!(old.cancellation().is_cancelled());
    assert!(!next.is_cancelled());
    assert_eq!(channel::generation(&bg), Some(generation));
    assert!(s.app.loading[View::Inbox.index()]);
    assert_eq!(s.app.status, status);
    assert_eq!(allocations(&s.app), page);
    assert_eq!(sql(&s.fixture), before_sql);
    release.send(()).unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        bg.poll(&mut s.app);
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    assert_eq!(s.app.status, "ACTUAL_NEW_INBOX_RESULT");
    assert!(!next.is_cancelled());
    unchanged(&s, s.steps.len());
}
#[test]
fn conversation_comments_metadata_read_retirement_v70_retired_read_admits_fresh_phase_escape_and_retry(
) {
    let mut second = calls();
    second.last_mut().unwrap().delay_ms = 30_000;
    second.extend(calls());
    let (mut s, mut bg, writer, old, pid) = held(second);
    let before_sql = sql(&s.fixture);
    // Explicit Cancel, not harmless same-page scrolling, ends the first read.
    s.app.dispatch(Action::Cancel);
    assert!(s.app.cancel_requested);
    s.app.cancel_requested = false;
    bg.cancel(s.app.view);
    s.app.loading[View::Inbox.index()] = false;
    bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
    drain_child(&mut s, &mut bg, &pid);
    assert!(!s.app.loading[View::Inbox.index()]);
    let next = enqueue(&mut s, &mut bg, &writer);
    assert!(!next.cancellation().same_instance(old.cancellation()));
    let held = held_pid(&s, opening().len() + 19);
    s.app.dispatch(Action::Cancel);
    assert!(s.app.cancel_requested);
    s.app.cancel_requested = false;
    // The real no-Form Esc route in main calls the ordinary Background cancel
    // for the selected view, then retires its old App-only logical owner.
    bg.cancel(s.app.view);
    s.app.loading[View::Inbox.index()] = false;
    bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
    drain_child(&mut s, &mut bg, &held);
    assert!(!s.app.loading[View::Inbox.index()]);
    let final_owner = enqueue(&mut s, &mut bg, &writer);
    assert!(!final_owner
        .cancellation()
        .same_instance(next.cancellation()));
    finish_read(&mut s, &mut bg);
    assert_eq!(sql(&s.fixture), before_sql);
    unchanged(&s, s.steps.len());
}
