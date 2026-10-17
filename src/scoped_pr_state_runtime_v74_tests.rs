//! Actual same Background channel, genuine factory Form/source and strict
//! fixed paid roles. No prepared DTO is substituted for original authority.
use super::*;
use crate::{
    app::{Action, View},
    drafts::DraftWriter,
    forms::FormAction,
    runtime::Background,
    scoped_pr_collaboration_native_v17 as native,
    scoped_pr_collaboration_ui_v17::state_v68 as state,
};
#[path = "scoped_pr_state_startup_v80_tests.rs"]
mod startup_v80;
fn visibility() -> Vec<GhStep> {
    let mut closed = state_parent();
    closed["state"] = json!("closed");
    // This paid response is a complete fresh summary, not the smaller target
    // identity fixture used by the opening Core. Keep its actual file counts.
    closed["created_at"] = json!("2026-10-01T00:00:00Z");
    closed["updated_at"] = json!("2026-10-16T00:00:00Z");
    closed["additions"] = json!(1);
    closed["deletions"] = json!(1);
    let mut rows = vec![
        get("user", json!({"id":42,"login":"octocat"})),
        get(&format!("repos/{REPO}"), repository()),
        get(&format!("repos/{REPO}/pulls/7"), closed.clone()),
    ];
    for (endpoint, data) in [
        ("issues/7/comments", json!([])),
        ("pulls/7/commits", json!([])),
        (
            "pulls/7/files",
            json!([{"filename":"src/view.rs","status":"modified","additions":1,"deletions":1,"patch":"@@ -1 +1 @@\n-old\n+new\n"}]),
        ),
        ("pulls/7/reviews", json!([])),
        ("issues/7/timeline", json!([])),
    ] {
        let mut call = get(
            &format!("repos/{REPO}/{endpoint}?per_page=100"),
            json!([data]),
        );
        call.args.extend(["--paginate".into(), "--slurp".into()]);
        rows.push(call);
    }
    rows.push(get(&format!("repos/{REPO}/pulls/7"), closed));
    rows.push(get("user", json!({"id":42,"login":"octocat"})));
    assert_eq!(rows.len(), 10);
    rows
}
fn strict_calls(s: &Setup) -> Vec<Value> {
    let raw = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let end = raw.iter().rposition(|v| *v == b'\n').map_or(0, |n| n + 1);
    raw[..end]
        .split(|v| *v == b'\n')
        .filter(|v| !v.is_empty())
        .map(|v| serde_json::from_slice(v).unwrap())
        .collect()
}
fn assert_calls(s: &Setup, through: usize) {
    let rows = strict_calls(s);
    assert_eq!(rows.len(), through);
    for (ordinal, (row, expected)) in rows.iter().zip(&s.steps).enumerate() {
        assert_eq!(row["matched"], true);
        assert_eq!(row["step"], ordinal);
        assert_eq!(row["args"], json!(expected.args));
        assert_eq!(
            row["stdin_hex"],
            expected
                .stdin
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        );
    }
}
fn submit(
    s: &mut Setup,
    bg: &mut Background,
    writer: &DraftWriter,
    action: FormAction,
) -> Arc<native::ScopedPrCollaborationWorkV17> {
    s.app.dispatch(Action::Form(action));
    native::prepare_requested(&mut s.app);
    let work = s
        .app
        .scoped_pr_collaboration_work_v17
        .take()
        .expect("Actual bodyless Form requested this phase");
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        Arc::clone(&work),
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    work
}
use crate::{
    owned_pty_processes_test_support as process, runtime::scoped_inline_test_support_v1 as channel,
};
struct Held(process::ProcessIdentity);
impl Held {
    fn stop(pid: i32) -> Self {
        let parent = process::process_identity(i32::try_from(std::process::id()).unwrap())
            .unwrap()
            .unwrap();
        let child = process::process_identity(pid).unwrap().unwrap();
        assert!(pid > 0 && pid != parent.pid && !child.zombie);
        assert_eq!(
            (child.parent, child.uid, child.session),
            (parent.pid, parent.uid, parent.session)
        );
        assert!(child.birth >= parent.birth);
        assert_eq!(
            process::process_identity(pid).unwrap().as_ref(),
            Some(&child)
        );
        // SAFETY: SAME exact positive direct child UID/birth/session rechecked;
        // no arbitrary PID, process-name/group sweep, or harness session lease.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        Self(child)
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        if let Ok(Some(now)) = process::process_identity(self.0.pid) {
            if !now.zombie
                && (now.pid, now.uid, now.session, now.birth)
                    == (self.0.pid, self.0.uid, self.0.session, self.0.birth)
            {
                // SAFETY: resume only the SAME owned child, also on unwinding.
                assert_eq!(unsafe { libc::kill(now.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}
struct Running {
    // Resume first, then let Background cancel/join before fixture disposal.
    held: Option<Held>,
    bg: Background,
    writer: DraftWriter,
    owner: Arc<native::ScopedPrCollaborationOwnerV17>,
    work: Arc<native::ScopedPrCollaborationWorkV17>,
    preview: Arc<state::StatePreviewV68>,
    generation: u64,
    held_index: usize,
    sql: Vec<String>,
    s: Setup,
}
fn held_confirm() -> Running {
    let (mut s, mut bg, writer, preview) = background_ready(true);
    let sql = all_sql(&s);
    let work = submit(&mut s, &mut bg, &writer, FormAction::Submit);
    let owner = s.app.scoped_pr_collaboration_owner_v17.clone().unwrap();
    let generation = channel::generation(&bg).unwrap();
    let held_index = s
        .steps
        .iter()
        .position(|step| step.delay_ms == 1000)
        .unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    let held = loop {
        if let Some(row) = strict_calls(&s).get(held_index) {
            assert_eq!(row["matched"], true);
            assert_eq!(row["args"], json!(s.steps[held_index].args));
            break Held::stop(i32::try_from(row["pid"].as_u64().unwrap()).unwrap());
        }
        assert!(
            Instant::now() < until,
            "Actual State consumed Confirm Core never started"
        );
        std::thread::yield_now();
    };
    assert!(preview.used.load(std::sync::atomic::Ordering::Acquire));
    assert!(!work.matches(&s.app));
    assert!(owner.owns(&s.app));
    assert_eq!(all_sql(&s), sql);
    Running {
        held: Some(held),
        bg,
        writer,
        owner,
        work,
        preview,
        generation,
        held_index,
        sql,
        s,
    }
}
fn assert_live(run: &Running) {
    assert!(run.preview.used.load(std::sync::atomic::Ordering::Acquire));
    assert_eq!(channel::generation(&run.bg), Some(run.generation));
    assert!(run.s.app.loading[View::Inbox.index()]);
    assert!(run.s.app.form.as_ref().unwrap().submitting);
    assert!(run
        .s
        .app
        .scoped_pr_collaboration_owner_v17
        .as_ref()
        .is_some_and(|o| Arc::ptr_eq(o, &run.owner)));
    assert!(!run.owner.cancellation().is_cancelled());
    assert!(run.owner.owns(&run.s.app));
    assert_calls(&run.s, run.held_index + 1);
    assert_eq!(all_sql(&run.s), run.sql);
}
fn cancel_without_effect(run: &mut Running) {
    run.bg
        .retire_stale_scoped_pr_collaboration_v17(&mut run.s.app);
    assert!(run.owner.cancellation().is_cancelled());
    assert!(channel::generation(&run.bg).is_none());
    let held = run.held.take().unwrap();
    let identity = held.0.clone();
    drop(held);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        match process::process_identity(identity.pid) {
            Ok(None) => break,
            Ok(Some(now)) if now.birth != identity.birth => break,
            _ => {}
        }
        assert!(
            Instant::now() < until,
            "Exact held State child absence not proved"
        );
        std::thread::yield_now();
    }
    run.bg.poll(&mut run.s.app);
    assert_eq!(all_sql(&run.s), run.sql);
    assert_calls(&run.s, run.held_index + 1);
    assert_eq!(git(&run.s.fixture, &["rev-parse", "HEAD"]), run.s.git[0]);
    for (path, expected) in [
        (".git/index", &run.s.git[1]),
        (".git/config", &run.s.git[2]),
        ("kept", &run.s.git[3]),
    ] {
        assert_eq!(
            &fs::read(run.s.fixture.repo().join(path)).unwrap(),
            expected
        );
    }
}
#[test]
fn pr_lifecycle_v68_consumed_state_preview_keeps_only_actual_busy_owner_through_held_core_and_completion(
) {
    let mut run = held_confirm();
    run.bg
        .retire_stale_scoped_pr_collaboration_v17(&mut run.s.app);
    assert_live(&run);
    paint(&mut run.s.app, 120, 40);
    run.bg
        .retire_stale_scoped_pr_collaboration_v17(&mut run.s.app);
    assert_live(&run);
    drop(run.held.take());
    finish(&mut run.s, &mut run.bg);
    assert!(!run.owner.cancellation().is_cancelled());
    assert!(run.s.app.form.is_none());
    assert_eq!(
        run.s
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
            .state,
        "closed"
    );
    assert_calls(&run.s, run.s.steps.len());
}
#[test]
fn pr_lifecycle_v68_bodyless_raw_field_graft_retires_held_owner_without_attempt_or_overwriting_input(
) {
    let mut run = held_confirm();
    run.s.app.form.as_mut().unwrap().fields.push(
        crate::forms::Field::text("body", "Foreign graft", "kept entered value", false).unwrap(),
    );
    assert!(!run.owner.owns(&run.s.app));
    cancel_without_effect(&mut run);
    assert_eq!(
        run.s.app.form.as_ref().unwrap().fields[0].editor.text,
        "kept entered value"
    );
}
#[test]
fn pr_lifecycle_v68_new_actual_state_form_or_foreign_owner_cannot_borrow_inflight_exception() {
    for foreign_owner in [false, true] {
        let mut run = held_confirm();
        let mut foreign_expected = None;
        let replacement = if foreign_owner {
            let foreign =
                native::ScopedPrCollaborationOwnerV17::new(Arc::clone(&run.work), &run.s.api)
                    .unwrap();
            assert!(!foreign.owns(&run.s.app));
            run.s.app.scoped_pr_collaboration_owner_v17 = Some(Arc::clone(&foreign));
            foreign_expected = Some(foreign);
            None
        } else {
            let data = state::state(run.s.app.form.as_ref().unwrap())
                .unwrap()
                .clone();
            let form = crate::forms::Form::pr_state_v68(data);
            let presentation = Arc::clone(form.presentation_v4().0);
            run.s.app.form = Some(form);
            Some(presentation)
        };
        assert!(!run.owner.owns(&run.s.app));
        cancel_without_effect(&mut run);
        if let Some(expected) = replacement {
            assert!(Arc::ptr_eq(
                run.s.app.form.as_ref().unwrap().presentation_v4().0,
                &expected
            ));
        }
        if let Some(expected) = foreign_expected {
            assert!(run
                .s
                .app
                .scoped_pr_collaboration_owner_v17
                .as_ref()
                .is_some_and(|o| Arc::ptr_eq(o, &expected)));
        }
    }
}
#[test]
fn pr_lifecycle_v68_consumed_state_refuses_duplicate_enqueue_and_nonbusy_admission_without_slot_effects(
) {
    let mut run = held_confirm();
    assert!(!run.bg.submit_scoped_pr_collaboration_v17(
        &mut run.s.app,
        Arc::clone(&run.work),
        run.s.api.clone(),
        database(&run.s.fixture),
        run.writer.client()
    ));
    native::prepare_requested(&mut run.s.app);
    assert!(run.s.app.scoped_pr_collaboration_work_v17.is_none());
    assert_live(&run);
    run.s.app.form.as_mut().unwrap().submitting = false;
    assert!(!run.owner.owns(&run.s.app));
    assert!(!run.work.matches(&run.s.app));
    let same = run.s.app.scoped_pr_collaboration_owner_v17.take().unwrap();
    assert!(!run.owner.owns(&run.s.app));
    run.s.app.scoped_pr_collaboration_owner_v17 = Some(same);
    cancel_without_effect(&mut run);
}
fn finish(s: &mut Setup, bg: &mut Background) {
    let until = Instant::now() + Duration::from_secs(8);
    while s.app.loading[View::Inbox.index()] {
        bg.poll(&mut s.app);
        assert!(
            Instant::now() < until,
            "Actual State Background phase did not finish"
        );
        std::thread::yield_now();
    }
    assert!(s.app.scoped_pr_collaboration_owner_v17.is_none());
}
fn background_ready(held: bool) -> (Setup, Background, DraftWriter, Arc<state::StatePreviewV68>) {
    let mut steps = phase(capability(json!(true)));
    steps.extend(phase(capability(json!(true))));
    let mut confirm = phase(capability(json!(true)));
    if held {
        confirm[0].delay_ms = 1000;
    }
    confirm.extend(super::receipt_v68::receipt_steps());
    confirm.extend(visibility());
    steps.extend(confirm);
    let mut s = setup(steps);
    let form = open_form(&s);
    let key = form.resource();
    let mut writer = DraftWriter::new(database(&s.fixture));
    super::receipt_v68::saved_bodyless(&mut writer, &s, &key);
    install_form(&mut s, form);
    // This exact accepted revision has already produced its real Saved ACK;
    // no ready Preview exists yet, and normal Main save retirement is safe.
    s.app.form.as_mut().unwrap().dirty = false;
    let mut bg = Background::new();
    submit(&mut s, &mut bg, &writer, FormAction::Preview);
    finish(&mut s, &mut bg);
    let preview = Arc::clone(
        state::state(s.app.form.as_ref().unwrap())
            .unwrap()
            .preview
            .as_ref()
            .unwrap(),
    );
    assert!(!s.app.form.as_ref().unwrap().dirty);
    paint(&mut s.app, 120, 40);
    assert!(state::request(s.app.form.as_ref().unwrap())
        .unwrap()
        .matches_confirm(s.app.form.as_ref().unwrap()));
    (s, bg, writer, preview)
}
#[test]
fn pr_lifecycle_v68_actual_accepted_initial_bodyless_save_settles_before_background_preview_core() {
    let mut steps = phase(capability(json!(true)));
    steps.extend(phase(capability(json!(true))));
    let mut s = setup(steps);
    let mut form = open_form(&s);
    let scope = form.scope();
    let key = form.resource();
    // SAME ordinary Main State path: invalidate unapproved intent, accept
    // the real bodyless Save, dirty=false means acceptance NOT durability.
    form.invalidate_plan();
    let encoded = crate::scoped_pr_collaboration_ui_v17::encoded_draft(&form).unwrap();
    let mut writer = DraftWriter::new(database(&s.fixture));
    // Initialize THIS real worker, then remove its harmless warm-up through
    // the same ordered queue before the actual State baseline. No fake ACK.
    let warm = "state-initial-save-worker-boundary-v94".to_owned();
    writer
        .save(scope.clone(), warm.clone(), "warm worker".into())
        .unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(ack) = writer.poll().into_iter().next() {
            assert_eq!(ack.scope, scope);
            assert_eq!(ack.resource, warm);
            ack.result.unwrap();
            break;
        }
        assert!(
            Instant::now() < until,
            "Actual warm Save must produce its Saved ACK"
        );
        std::thread::yield_now();
    }
    writer.client().remove(scope.clone(), warm.clone()).unwrap();
    let observed = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let missing = || {
        observed
            .query_row(
                "SELECT count(*) FROM drafts WHERE host=?1 AND login=?2 AND resource IN(?3,?4)",
                rusqlite::params![scope.host, scope.login, key, warm],
                |r| r.get::<_, usize>(0),
            )
            .unwrap()
    };
    assert_eq!(
        missing(),
        0,
        "Warm-up is actually removed before State Save"
    );
    let before_accepted = all_sql_connection(&observed);
    // Declared after writer: rollback on unwinding releases this exact owned
    // lock BEFORE writer Drop joins. No Cache::open/migration while held.
    let mut locked = rusqlite::Connection::open(database(&s.fixture)).unwrap();
    let lock = locked
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .unwrap();
    writer
        .save(scope.clone(), key.clone(), encoded.clone())
        .unwrap();
    form.dirty = false;
    assert_eq!(
        missing(),
        0,
        "Accepted State Save cannot commit through the owned write lock"
    );
    install_form(&mut s, form);
    let commands = strict_calls(&s).len();
    let mut bg = Background::new();
    submit(&mut s, &mut bg, &writer, FormAction::Preview);
    assert!(s
        .app
        .scoped_pr_collaboration_owner_v17
        .as_ref()
        .unwrap()
        .waiting_open_drafts_v20());
    assert_calls(&s, commands);
    assert!(state::state(s.app.form.as_ref().unwrap())
        .unwrap()
        .preview
        .is_none());
    assert_eq!(missing(), 0);
    assert_eq!(all_sql_connection(&observed), before_accepted);
    drop(lock); // Release promptly within the unchanged worker2s busy bound.
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Some(ack) = writer.poll().into_iter().next() {
            assert_eq!(ack.scope, scope);
            assert_eq!(ack.resource, key);
            ack.result.unwrap();
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Actual accepted initial Save never produced Saved ACK"
        );
        std::thread::yield_now();
    }
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&scope, &key)
            .unwrap()
            .as_deref(),
        Some(encoded.as_str())
    );
    let after_saved = all_sql(&s);
    finish(&mut s, &mut bg);
    assert_eq!(
        all_sql(&s),
        after_saved,
        "Readonly Preview cannot alter the exact Saved envelope or any SQL"
    );
    assert_calls(&s, s.steps.len());
    paint(&mut s.app, 120, 40);
    assert!(state::request(s.app.form.as_ref().unwrap())
        .unwrap()
        .matches_confirm(s.app.form.as_ref().unwrap()));
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (name, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(name)).unwrap(), expected);
    }
}
#[test]
fn pr_lifecycle_v68_failed_absent_or_wrong_actual_initial_save_keeps_same_form_without_any_preview_child(
) {
    for mode in 0..3 {
        let mut s = setup(phase(capability(json!(true))));
        let mut form = open_form(&s);
        let scope = form.scope();
        let key = form.resource();
        form.invalidate_plan();
        let mut writer = DraftWriter::new(database(&s.fixture));
        let db = rusqlite::Connection::open(database(&s.fixture)).unwrap();
        if mode == 0 {
            db.execute_batch("CREATE TRIGGER fail_state_initial_save BEFORE INSERT ON drafts WHEN NEW.resource LIKE 'state-v68/%' BEGIN SELECT RAISE(ABORT,'exact disposable initial Save failure'); END;").unwrap();
        }
        if mode != 1 {
            let body = if mode == 2 {
                "{\"version\":17,\"values\":{\"foreign\":\"not bodyless\"},\"publication\":null}"
                    .into()
            } else {
                crate::scoped_pr_collaboration_ui_v17::encoded_draft(&form).unwrap()
            };
            writer.save(scope.clone(), key.clone(), body).unwrap();
            let deadline = Instant::now() + Duration::from_secs(8);
            loop {
                if let Some(ack) = writer.poll().into_iter().next() {
                    assert_eq!(ack.scope, scope);
                    assert_eq!(ack.resource, key);
                    assert_eq!(ack.result.is_err(), mode == 0);
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "Actual initial Save ACK never arrived"
                );
                std::thread::yield_now();
            }
        }
        form.dirty = false;
        install_form(&mut s, form);
        let request = state::request(s.app.form.as_ref().unwrap()).unwrap();
        let presentation = Arc::clone(s.app.form.as_ref().unwrap().presentation_v4().0);
        let stable = all_sql(&s);
        let mut bg = Background::new();
        submit(&mut s, &mut bg, &writer, FormAction::Preview);
        finish(&mut s, &mut bg);
        let same = s
            .app
            .form
            .as_ref()
            .expect("A local initial Save refusal is NOT account/source loss");
        assert!(Arc::ptr_eq(same.presentation_v4().0, &presentation));
        assert!(request.owns(same));
        assert!(state::state(same).unwrap().preview.is_none());
        assert!(!same.submitting && !same.submit_requested && !same.plan_ready());
        assert!(same
            .error
            .contains("Save the exact bodyless State intent successfully before Preview"));
        assert_eq!(all_sql(&s), stable);
        assert_calls(&s, s.steps.len());
        assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
        for (name, expected) in [
            (".git/index", &s.git[1]),
            (".git/config", &s.git[2]),
            ("kept", &s.git[3]),
        ] {
            assert_eq!(&fs::read(s.fixture.repo().join(name)).unwrap(), expected);
        }
    }
}
#[test]
fn pr_lifecycle_v68_actual_save_refusal_cannot_paint_grafted_successor_or_changed_account_form() {
    for mode in 0..4 {
        let mut steps = phase(capability(json!(true)));
        if mode == 3 {
            // A genuine provider account loss AFTER successful Save must not
            // be confused with the closed local Save refusal disposition.
            steps.extend([
                principal().remove(0),
                get("user", json!({"id":43,"login":"other-account"})),
            ]);
        }
        let mut s = setup(steps);
        let mut form = open_form(&s);
        form.invalidate_plan();
        form.dirty = false;
        let key = form.resource();
        install_form(&mut s, form);
        let request = state::request(s.app.form.as_ref().unwrap()).unwrap();
        let mut writer = DraftWriter::new(database(&s.fixture));
        if mode == 3 {
            super::receipt_v68::saved_bodyless(&mut writer, &s, &key);
        }
        let stable = all_sql(&s);
        let mut bg = Background::new();
        submit(&mut s, &mut bg, &writer, FormAction::Preview);
        let owner = s.app.scoped_pr_collaboration_owner_v17.clone().unwrap();
        assert!(owner.owns(&s.app));
        // Prove the actual old worker has completed before changing its Form;
        // the original message is only observed and put back, never replaced.
        bg.observe_state_preview_completion_v99(&s.app, &owner);
        let replacement = if mode == 0 {
            s.app.form.as_mut().unwrap().fields.push(
                crate::forms::Field::text(
                    "foreign",
                    "Grafted input",
                    "keep this exact input",
                    false,
                )
                .unwrap(),
            );
            None
        } else if mode == 1 {
            let actual = state::state(s.app.form.as_ref().unwrap()).unwrap().clone();
            let mut next = crate::forms::Form::pr_state_v68(actual);
            next.error = "New actual Form error remains exact".into();
            let presentation = Arc::clone(next.presentation_v4().0);
            s.app.form = Some(next);
            Some(presentation)
        } else if mode == 2 {
            s.app.github.as_mut().unwrap().identity.account_id = Some(43);
            None
        } else {
            None
        };
        if mode < 3 {
            assert!(!owner.owns(&s.app));
        }
        // SAME Main order: retire stale physical ownership BEFORE polling,
        // then draw the actual Form. Account privacy may retire the original
        // page on one draw and its no-longer-owned Form on the next real tick.
        for _ in 0..2 {
            bg.retire_stale_scoped_pr_collaboration_v17(&mut s.app);
            if mode < 3 {
                assert!(owner.cancellation().is_cancelled());
                assert!(channel::generation(&bg).is_none());
                assert!(!s.app.loading[View::Inbox.index()]);
            }
            bg.poll(&mut s.app);
            paint(&mut s.app, 120, 40);
        }
        finish(&mut s, &mut bg);
        if mode == 0 {
            let same = s.app.form.as_ref().unwrap();
            assert_eq!(same.fields[0].editor.text, "keep this exact input");
            assert!(!same.error.contains("Save the exact bodyless State intent"));
            assert!(!request.matches(same));
        } else if let Some(expected) = replacement {
            let next = s.app.form.as_ref().unwrap();
            assert!(Arc::ptr_eq(next.presentation_v4().0, &expected));
            assert_eq!(next.error, "New actual Form error remains exact");
            assert!(!request.owns(next));
        } else {
            assert!(s.app.form.is_none() && s.app.github.is_none());
            assert!(!owner.owns(&s.app));
            assert!(!s
                .app
                .status
                .contains("Save the exact bodyless State intent"));
        }
        assert_eq!(all_sql(&s), stable);
        assert_calls(&s, s.steps.len());
        assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
        for (name, expected) in [
            (".git/index", &s.git[1]),
            (".git/config", &s.git[2]),
            ("kept", &s.git[3]),
        ] {
            assert_eq!(&fs::read(s.fixture.repo().join(name)).unwrap(), expected);
        }
    }
}
#[test]
fn pr_lifecycle_v68_actual_background_bodyless_confirm_publishes_closed_page_not_old_queue_truth() {
    let (mut s, mut bg, writer, preview) = background_ready(false);
    let old_ticket = s
        .app
        .github
        .as_ref()
        .unwrap()
        .personal_pr_page_ticket_v13
        .clone()
        .unwrap();
    let scope = s.app.form.as_ref().unwrap().scope();
    let key = s.app.form.as_ref().unwrap().resource();
    submit(&mut s, &mut bg, &writer, FormAction::Submit);
    finish(&mut s, &mut bg);
    assert!(preview.used.load(std::sync::atomic::Ordering::Acquire));
    let workspace = s
        .app
        .github
        .as_ref()
        .expect("Actual full final-UID page admitted");
    assert!(s.app.form.is_none() && s.app.overlay.is_none());
    assert_eq!(
        workspace.pr.as_ref().unwrap().details.summary.pr.state,
        "closed"
    );
    assert_eq!(
        workspace.pr.as_ref().unwrap().details.summary.head_sha,
        "a".repeat(40)
    );
    assert_eq!(
        workspace.pr.as_ref().unwrap().details.summary.base_sha,
        "b".repeat(40)
    );
    assert!(!Arc::ptr_eq(
        &old_ticket,
        workspace.personal_pr_page_ticket_v13.as_ref().unwrap()
    ));
    assert!(workspace.state_membership_unavailable_v68());
    assert!(workspace.inbox.pull_requests.is_empty());
    assert!(workspace.coverage_label().contains("counts unavailable"));
    assert!(!workspace.coverage_label().contains("0 unique"));
    let cache = Cache::open(&database(&s.fixture)).unwrap();
    assert!(cache.draft(&scope, &key).unwrap().is_none());
    let target = crate::pr_write_journal::PrWriteTarget::new(
        &workspace.identity,
        &workspace.pr.as_ref().unwrap().details.summary,
    )
    .unwrap();
    let journal = crate::pr_write_journal::PrWriteJournal::open(&database(&s.fixture)).unwrap();
    assert!(journal.blocking(&target).unwrap().is_none());
    let db = rusqlite::Connection::open(database(&s.fixture)).unwrap();
    let (attempt, raw): (i64, String) = db
        .query_row(
            "SELECT attempt_id,origin_json FROM pr_write_state_origins_v68",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let origin: crate::pr_write_journal::state_origin_v68::StateOriginRecordV68 =
        serde_json::from_str(&raw).unwrap();
    let marker = journal
        .original_publication(&target, &origin.reference)
        .unwrap()
        .unwrap();
    assert_eq!(
        marker.state,
        crate::pr_write_journal::PrWriteState::Confirmed
    );
    assert_eq!(marker.attempt_id, attempt);
    journal.state_origin_v68(&target, &marker).unwrap();
    let source = format!("pr-write-projection:{}", target.node_id());
    let stable = all_sql(&s);
    let frame = cache.sync_frame(&scope, &source).unwrap().unwrap();
    assert!(frame
        .records
        .iter()
        .any(|r| r.kind == "pr-details" && r.payload["summary"]["pr"]["state"] == "closed"));
    assert!(cache
        .sync_frame(&scope, "personal-inbox:unproven-old-alias")
        .is_err());
    assert_eq!(all_sql(&s), stable);
    let generation: i64 = db
        .query_row(
            "SELECT source_generation FROM pr_write_state_source_watermarks_v73 WHERE source=?1",
            [&source],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(frame.generation, generation as u64);
    assert_calls(&s, s.steps.len());
    assert_eq!(
        strict_calls(&s)
            .iter()
            .filter(|r| r["args"][5] == "POST"
                && r["stdin_hex"]
                    .as_str()
                    .unwrap()
                    .starts_with("7b227175657279223a226d75746174696f6e"))
            .count(),
        1
    );
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (path, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(path)).unwrap(), expected);
    }
}
