//! Actual queued worker consumes its Preview before a genuine held Core child.
//! Admission remains one-take; only the SAME accepted busy owner can continue.
use super::*;
use crate::owned_pty_processes_test_support as process;

fn calls(s: &Setup) -> Vec<Value> {
    let bytes = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |n| n + 1);
    bytes[..end]
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|b| serde_json::from_slice(b).unwrap())
        .collect()
}
type FullSql = std::collections::BTreeMap<String, (Vec<String>, Vec<Vec<rusqlite::types::Value>>)>;
fn full_sql(s: &Setup) -> FullSql {
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names = db
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    names
        .into_iter()
        .map(|name| {
            let mut q = db
                .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
                .unwrap();
            let columns = q
                .column_names()
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>();
            let mut rows = q
                .query_map([], |row| {
                    (0..columns.len())
                        .map(|i| row.get(i))
                        .collect::<rusqlite::Result<Vec<_>>>()
                })
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>();
            rows.sort_by_key(|row| format!("{row:?}"));
            (name, (columns, rows))
        })
        .collect()
}
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
        // SAFETY: exact positive test-owned child, actual UID/session/birth
        // rechecked immediately above. Never a process-name/group sweep.
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
                // SAFETY: only the same verified child is resumed, including
                // assertion unwinding; Background then cancels/joins normally.
                assert_eq!(unsafe { libc::kill(now.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}
struct Running {
    // Drop the hold BEFORE Background joins and before fixture data is removed.
    held: Option<Held>,
    background: Background,
    writer: DraftWriter,
    owner: Arc<native::ScopedPrCollaborationOwnerV17>,
    work: Arc<native::ScopedPrCollaborationWorkV17>,
    preview: Arc<member_ui::MetadataMembersPreviewV48>,
    generation: u64,
    held_index: usize,
    sql_before: FullSql,
    s: Setup,
}
fn started(catalogue: bool) -> Running {
    let command = Cmd::AddAssignee;
    let mut tail = selected_phase(command);
    tail[0].delay_ms = 1_000;
    tail.extend(metadata_post(command));
    let (mut s, mut form, preview) = prepared_metadata(command, catalogue, tail);
    let mut writer = DraftWriter::new(database(&s.fixture));
    form.prepare_draft_autosave_v77();
    save_and_ack(
        &mut writer,
        &form,
        &form.resource(),
        &ui::encoded_draft(&form).unwrap(),
        &s,
    );
    form.settle_draft_autosave_v77(true);
    assert!(!form.dirty);
    s.app.form = Some(form);
    s.app.overlay = Some(Overlay::Form);
    s.app.form_dismissed = false;
    paint(&mut s.app, 120, 40);
    paint(&mut s.app, 120, 40);
    let sql_before = full_sql(&s);
    s.app.dispatch(Action::Form(FormAction::Submit));
    native::prepare_requested(&mut s.app);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(work.matches(&s.app));
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
    let held_index = s
        .steps
        .iter()
        .position(|step| step.delay_ms == 1_000)
        .unwrap();
    let until = Instant::now() + Duration::from_secs(8);
    let held = loop {
        if let Some(row) = calls(&s).get(held_index) {
            assert_eq!(row["matched"], true);
            assert_eq!(row["args"], json!(s.steps[held_index].args));
            break Held::stop(i32::try_from(row["pid"].as_u64().unwrap()).unwrap());
        }
        assert!(
            Instant::now() < until,
            "Actual consumed Confirm Core child never started"
        );
        std::thread::yield_now();
    };
    assert!(preview.used.load(std::sync::atomic::Ordering::Acquire));
    assert!(
        !member_ui::request(s.app.form.as_ref().unwrap())
            .unwrap()
            .matches_confirm(s.app.form.as_ref().unwrap()),
        "Duplicate admission stays one-take"
    );
    assert!(!work.matches(&s.app));
    assert!(owner.owns(&s.app));
    assert_eq!(full_sql(&s), sql_before);
    Running {
        held: Some(held),
        background,
        writer,
        owner,
        work,
        preview,
        generation,
        held_index,
        sql_before,
        s,
    }
}
fn assert_live(run: &Running) {
    assert_eq!(channel::generation(&run.background), Some(run.generation));
    assert!(run.s.app.loading[View::Inbox.index()]);
    assert!(run.s.app.form.as_ref().unwrap().submitting);
    assert!(run
        .s
        .app
        .scoped_pr_collaboration_owner_v17
        .as_ref()
        .is_some_and(|owner| Arc::ptr_eq(owner, &run.owner)));
    assert!(!run.owner.cancellation().is_cancelled());
    assert!(run.owner.owns(&run.s.app));
    assert_eq!(calls(&run.s).len(), run.held_index + 1);
    assert_eq!(full_sql(&run.s), run.sql_before);
}
fn child_absent_v253(
    observed: std::io::Result<Option<process::ProcessIdentity>>,
    original: &process::ProcessIdentity,
) -> std::io::Result<bool> {
    match observed {
        Ok(None) => Ok(true),
        Ok(Some(now))
            if (now.pid, now.uid, now.birth, now.session)
                == (original.pid, original.uid, original.birth, original.session) =>
        {
            // A zombie remains present; require explicit kernel absence.
            Ok(false)
        }
        Ok(Some(_)) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Cleanup observed a foreign process identity",
        )),
        #[cfg(target_os = "linux")]
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
            // Linux can retire ownership fields during /proc reads. Unknown
            // is NOT absence, not permission to signal, and not a pass: keep
            // polling within the original bound for actual disappearance.
            Ok(false)
        }
        Err(error) => Err(error),
    }
}
#[test]
fn pr_metadata_members_v253_reaping_requires_actual_absence_not_invalid_zombie_or_foreign() {
    let original = process::process_identity(std::process::id() as i32)
        .unwrap()
        .unwrap();
    assert!(child_absent_v253(Ok(None), &original).unwrap());
    assert!(!child_absent_v253(Ok(Some(original.clone())), &original).unwrap());
    let mut zombie = original.clone();
    zombie.zombie = true;
    assert!(!child_absent_v253(Ok(Some(zombie)), &original).unwrap());
    #[cfg(target_os = "linux")]
    assert!(!child_absent_v253(
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "invalid stat ownership fields",
        )),
        &original,
    )
    .unwrap());
    for field in 0..4 {
        let mut foreign = original.clone();
        match field {
            0 => foreign.pid += 1,
            1 => foreign.uid += 1,
            2 => foreign.birth.0 += 1,
            _ => foreign.session += 1,
        }
        assert!(child_absent_v253(Ok(Some(foreign)), &original).is_err());
    }
    let denied = child_absent_v253(
        Err(std::io::Error::from_raw_os_error(libc::EACCES)),
        &original,
    )
    .unwrap_err();
    assert_eq!(denied.raw_os_error(), Some(libc::EACCES));
}
fn cancel_and_assert_no_effect(run: &mut Running) {
    run.background
        .retire_stale_scoped_pr_collaboration_v17(&mut run.s.app);
    assert!(run.owner.cancellation().is_cancelled());
    assert!(channel::generation(&run.background).is_none());
    assert!(!run.s.app.loading[View::Inbox.index()]);
    let held = run.held.take().unwrap();
    let identity = held.0.clone();
    drop(held);
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        if child_absent_v253(process::process_identity(identity.pid), &identity).unwrap() {
            break;
        }
        assert!(
            Instant::now() < until,
            "Exact original held child not reaped"
        );
        std::thread::yield_now();
    }
    run.background.poll(&mut run.s.app);
    assert_eq!(full_sql(&run.s), run.sql_before);
    assert_eq!(calls(&run.s).len(), run.held_index + 1);
    exact(&run.s, run.held_index + 1);
}

#[test]
fn pr_metadata_members_v81_consumed_preview_keeps_actual_held_owner_and_full_publication_both_origins(
) {
    for catalogue in [false, true] {
        let mut run = started(catalogue);
        run.background
            .retire_stale_scoped_pr_collaboration_v17(&mut run.s.app);
        assert_live(&run);
        paint(&mut run.s.app, 120, 40);
        run.background
            .retire_stale_scoped_pr_collaboration_v17(&mut run.s.app);
        assert_live(&run);
        drop(run.held.take());
        let until = Instant::now() + Duration::from_secs(8);
        while run.s.app.loading[View::Inbox.index()] {
            run.background.poll(&mut run.s.app);
            assert!(
                Instant::now() < until,
                "Actual admitted Members publisher did not finish"
            );
            std::thread::yield_now();
        }
        assert!(!run.owner.cancellation().is_cancelled());
        assert!(run.s.app.scoped_pr_collaboration_owner_v17.is_none());
        assert!(run.s.app.form.is_none());
        let page = run.s.app.github.as_ref().unwrap().pr.as_ref().unwrap();
        assert_eq!(page.tab, PrTab::Metadata);
        assert_eq!(
            page.context.as_ref().unwrap().issue_metadata,
            issue(&after(Cmd::AddAssignee))
        );
        let db = rusqlite::Connection::open_with_flags(
            database(&run.s.fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM pr_write_attempts WHERE command=?1 AND state='confirmed'",
                [Cmd::AddAssignee.key()],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM drafts WHERE resource LIKE 'metadata-members-v48/%'",
                [],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
            0
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM pr_write_metadata_origins_v25",
                [],
                |row| row.get::<_, usize>(0)
            )
            .unwrap(),
            1
        );
        exact(&run.s, run.s.steps.len());
    }
}

#[test]
fn pr_metadata_members_v81_changed_raw_value_retires_actual_held_owner_without_write_both_origins()
{
    for catalogue in [false, true] {
        let mut run = started(catalogue);
        run.s.app.form.as_mut().unwrap().fields[0].editor =
            crate::editor::TextBuffer::new("FOREIGN_RAW_MEMBER_VALUE").unwrap();
        assert!(!run.owner.owns(&run.s.app));
        cancel_and_assert_no_effect(&mut run);
        assert_eq!(
            run.s.app.form.as_ref().unwrap().fields[0].editor.text,
            "FOREIGN_RAW_MEMBER_VALUE"
        );
    }
}

#[test]
fn pr_metadata_members_v81_newer_real_form_and_foreign_owner_never_gain_inflight_exception() {
    for catalogue in [false, true] {
        for foreign_owner in [false, true] {
            let mut run = started(catalogue);
            if foreign_owner {
                // Even the SAME Work is not this actual installed owner Arc.
                let foreign =
                    native::ScopedPrCollaborationOwnerV17::new(run.work.clone(), &run.s.api)
                        .unwrap();
                assert!(!foreign.owns(&run.s.app));
                run.s.app.scoped_pr_collaboration_owner_v17 = Some(foreign);
            } else {
                let old = run.s.app.form.take().unwrap();
                let mut replacement = Form::pr(crate::forms::PrIntent {
                    kind: crate::forms::PrFormKind::Comment,
                    target: run.preview.intent.source.parent().summary().clone(),
                    identity: run.preview.identity.clone(),
                })
                .unwrap();
                replacement.purpose = old.purpose;
                replacement.fields = old.fields;
                replacement.preview = true;
                replacement.submitting = true;
                run.s.app.form = Some(replacement);
            }
            assert!(!run.owner.owns(&run.s.app));
            cancel_and_assert_no_effect(&mut run);
        }
    }
}

#[test]
fn pr_metadata_members_v81_consumed_preview_refuses_duplicate_queue_and_raw_nonbusy_admission() {
    for catalogue in [false, true] {
        let mut run = started(catalogue);
        assert!(!run.background.submit_scoped_pr_collaboration_v17(
            &mut run.s.app,
            run.work.clone(),
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
        let current = run.s.app.scoped_pr_collaboration_owner_v17.take().unwrap();
        assert!(
            !run.owner.owns(&run.s.app),
            "Absent actual App owner cannot use exception"
        );
        run.s.app.scoped_pr_collaboration_owner_v17 = Some(current);
        cancel_and_assert_no_effect(&mut run);
    }
}
