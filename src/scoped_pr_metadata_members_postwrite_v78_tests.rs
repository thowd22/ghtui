//! Real publisher/runner continuation: an owned write advances the epoch, but
//! a later write must retire its paid context, never its durable receipt.
use super::*;
use crate::owned_pty_processes_test_support as process;

fn commands(s: &Setup) -> Vec<Value> {
    let bytes = fs::read(s.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    // Only complete fixture records are observations. A malformed complete
    // record is an error; an unfinished append (including UTF-8) is not one.
    let end = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
    bytes[..end]
        .split(|b| *b == b'\n')
        .filter(|b| !b.is_empty())
        .map(|row| serde_json::from_slice(row).unwrap())
        .collect()
}
struct Held(process::ProcessIdentity);
impl Held {
    fn stop(pid: i32) -> Self {
        let parent = process::process_identity(std::process::id() as i32)
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
        // SAFETY: one exact positive owned child, with UID/session/birth checked
        // immediately above; never a name, foreign PID or process-group sweep.
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
                // SAFETY: resume only the same verified original child.
                assert_eq!(unsafe { libc::kill(now.pid, libc::SIGCONT) }, 0);
            }
        }
    }
}
#[test]
fn pr_metadata_members_postwrite_v78_own_successor_epoch_preserves_both_actual_origin_pages() {
    for catalogue in [false, true] {
        let command = Cmd::AddAssignee;
        let mut tail = selected_phase(command);
        tail.extend(metadata_post(command));
        let (s, form, preview) = prepared_metadata(command, catalogue, tail);
        let runner = s.api.native_external_runner_v4();
        let resource = format!("github.invalid/{REPO}/pull/7");
        let before = runner.resource_generation(&resource);
        let result = execute_metadata(&s, &form, &preview).unwrap();
        assert_eq!(
            runner.resource_generation(&resource),
            before.checked_add(1).unwrap()
        );
        assert_eq!(result.state, State::Confirmed);
        assert!(result.refreshed_page_v19.is_some());
        assert!(!result.draft_retained());
        assert!(result.return_v19.is_none());
        exact(&s, s.steps.len());
    }
}
#[test]
fn pr_metadata_members_postwrite_v78_newer_epoch_during_real_detail_never_admits_or_resends() {
    for catalogue in [false, true] {
        let command = Cmd::AddAssignee;
        let mut paid = metadata_post(command);
        paid[11].delay_ms = 5_000; // the first genuine paid detail process
        let mut tail = selected_phase(command);
        tail.extend(paid);
        let (s, form, preview) = prepared_metadata(command, catalogue, tail);
        let before_calls = commands(&s).len();
        let before_source = allocations(&s.app);
        let resource = format!("github.invalid/{REPO}/pull/7");
        let runner = s.api.native_external_runner_v4().clone();
        let before_generation = runner.resource_generation(&resource);
        let path = database(&s.fixture);
        let mut writer = DraftWriter::new(path.clone());
        save_and_ack(
            &mut writer,
            &form,
            &form.resource(),
            &ui::encoded_draft(&form).unwrap(),
            &s,
        );
        let drafts = writer.client();
        let cancel = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        let fence = drafts
            .capture_publication_v2(&path, &form.scope(), &form.resource(), &cancel, deadline)
            .unwrap();
        let api = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
            .unwrap();
        let worker_preview = Arc::clone(&preview);
        let worker = std::thread::spawn(move || {
            PrPlan::execute_original_metadata_members_v48(
                &worker_preview,
                &api,
                &path,
                &drafts,
                fence,
                &cancel,
                deadline,
            )
        });
        let until = Instant::now() + Duration::from_secs(5);
        let child = loop {
            let rows = commands(&s);
            if rows.len() == before_calls + 23 {
                // actual pre-Core11 + paid12
                let row = rows.last().unwrap();
                assert_eq!(row["matched"], true);
                assert!(row["args"].as_array().unwrap().iter().any(|a| a
                    .as_str()
                    .is_some_and(|s| s.ends_with("issues/7/comments?per_page=100"))));
                break Held::stop(i32::try_from(row["pid"].as_i64().unwrap()).unwrap());
            }
            assert!(
                rows.len() < before_calls + 23 && Instant::now() < until,
                "Actual first detail process not held"
            );
            std::thread::yield_now();
        };
        assert_eq!(
            runner.resource_generation(&resource),
            before_generation.checked_add(1).unwrap()
        );
        let before = crate::personal_pr_entry_v13::tests::sql(&s.fixture);
        let protected = |rows: Vec<String>| {
            rows.into_iter()
                .filter(|r| r.starts_with("pr_write_") || r.starts_with("drafts:"))
                .collect::<Vec<_>>()
        };
        let original = protected(before);
        runner.mark_ambiguous(&resource);
        let pid = child.0.pid;
        drop(child);
        let result = worker.join().unwrap().unwrap();
        assert!(
            process::process_identity(pid).unwrap().is_none(),
            "Exact detail child must be reaped, not left as a zombie"
        );
        assert_eq!(result.state, State::ConfirmedVisibilityUnavailable);
        assert!(result.refreshed_page_v19.is_none());
        assert!(result.draft_retained());
        assert_eq!(allocations(&s.app), before_source);
        assert_eq!(protected(crate::personal_pr_entry_v13::tests::sql(&s.fixture)), original, "Newer runner facts cannot rewrite/drop the durable receipt, origin, lineage or protected body");
        let target = PrWriteTarget::new(
            preview.intent.source.identity(),
            preview.intent.source.parent().summary(),
        )
        .unwrap();
        let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
        let marker = journal
            .original_publication(&target, &result.reference)
            .unwrap()
            .unwrap();
        assert_eq!(marker.state, PrWriteState::Confirmed);
        assert!(marker.confirmed.is_some());
        assert_eq!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .get(&form.scope(), "pr-metadata-context-v48", target.node_id())
                .unwrap(),
            None
        );
        assert_eq!(
            runner.resource_generation(&resource),
            before_generation.checked_add(2).unwrap()
        );
        exact(&s, before_calls + 23);
    }
}

fn held_final_uid_v82(uid: u64) {
    for catalogue in [false, true] {
        let command = Cmd::AddLabel;
        let mut paid = metadata_post(command);
        paid[21] = get("user", json!({"id":uid,"login":"octocat"}));
        paid[21].delay_ms = 5_000;
        let mut tail = selected_phase(command);
        tail.extend(paid);
        let (s, form, preview) = prepared_metadata(command, catalogue, tail);
        let before_calls = commands(&s).len();
        let path = database(&s.fixture);
        let mut writer = DraftWriter::new(path.clone());
        save_and_ack(
            &mut writer,
            &form,
            &form.resource(),
            &ui::encoded_draft(&form).unwrap(),
            &s,
        );
        let drafts = writer.client();
        let cancel = CancellationToken::default();
        let deadline = Instant::now() + Duration::from_secs(30);
        let fence = drafts
            .capture_publication_v2(&path, &form.scope(), &form.resource(), &cancel, deadline)
            .unwrap();
        let api = s
            .api
            .clone()
            .with_original_pr_read_bounds_v13(deadline, 1024 * 1024)
            .unwrap();
        let worker = std::thread::spawn(move || {
            PrPlan::execute_original_metadata_members_v48(
                &preview, &api, &path, &drafts, fence, &cancel, deadline,
            )
        });
        let until = Instant::now() + Duration::from_secs(8);
        let held = loop {
            let rows = commands(&s);
            if rows.len() == before_calls + 33 {
                let final_user = rows.last().unwrap();
                assert_eq!(final_user["matched"], true);
                assert_eq!(final_user["args"][1], "user");
                break Held::stop(i32::try_from(final_user["pid"].as_i64().unwrap()).unwrap());
            }
            assert!(
                rows.len() < before_calls + 33 && Instant::now() < until,
                "Actual last paid UID was not held"
            );
            std::thread::yield_now();
        };
        let body = Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&form.scope(), &form.resource())
            .unwrap();
        assert!(
            body.is_some(),
            "Preparing a page must NOT remove the draft before final UID"
        );
        let protected = |rows: Vec<String>| {
            rows.into_iter()
                .filter(|row| row.starts_with("pr_write_") || row.starts_with("drafts:"))
                .collect::<Vec<_>>()
        };
        let before = protected(crate::personal_pr_entry_v13::tests::sql(&s.fixture));
        let pid = held.0.pid;
        drop(held);
        let result = worker.join().unwrap().unwrap();
        assert!(
            process::process_identity(pid).unwrap().is_none(),
            "Exact final UID child must be reaped"
        );
        let after = Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(&form.scope(), &form.resource())
            .unwrap();
        if uid == 42 {
            assert_eq!(result.state, State::Confirmed);
            assert!(!result.draft_retained());
            assert!(result.refreshed_page_v19.is_some());
            assert_eq!(after, None);
        } else {
            assert_eq!(result.state, State::ConfirmedVisibilityUnavailable);
            assert!(result.draft_retained());
            assert!(result.refreshed_page_v19.is_none());
            assert!(result.return_v19.is_none());
            assert_eq!(after, body);
            assert_eq!(
                protected(crate::personal_pr_entry_v13::tests::sql(&s.fixture)),
                before,
                "Failed final UID preserves exact journal/origin/reference/draft rows"
            );
        }
        exact(&s, s.steps.len());
    }
}

#[test]
fn pr_metadata_members_v82_real_held_last_uid_keeps_draft_until_success_both_origins() {
    held_final_uid_v82(42);
}

#[test]
fn pr_metadata_members_v82_real_held_last_uid_loss_preserves_all_protected_rows_both_origins() {
    held_final_uid_v82(43);
}
