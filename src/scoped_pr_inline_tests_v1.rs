//! Genuine closed image producers, real divergent Git objects, strict gh/SQL.
use super::*;
#[path = "scoped_pr_inline_runtime_tests_v1.rs"]
mod inline_runtime;
#[path = "scoped_pr_inline_ui_tests_v1.rs"]
mod inline_ui;
#[path = "scoped_pr_native_inline_v1_tests.rs"]
mod native_entry;
#[path = "scoped_pr_review_author_v2_tests.rs"]
mod review_author_v2;
#[path = "scoped_pr_review_provider_v2_tests.rs"]
mod review_provider_v2;
use crate::owned_pty_processes_test_support as original_process_identity;
use crate::{drafts::DraftWriter, pr_write_journal::PrWriteJournal, storage::Cache};

struct HeldOriginalChild {
    identity: original_process_identity::ProcessIdentity,
    stopped: bool,
}
impl HeldOriginalChild {
    fn stop(pid: i32) -> Self {
        assert!(pid > 0 && pid != std::process::id() as i32);
        let identity = original_process_identity::process_identity(pid)
            .unwrap()
            .unwrap();
        // SAFETY: read-only current owner/session queries on macOS/Linux.
        assert_eq!(identity.uid, unsafe { libc::geteuid() });
        assert_eq!(identity.parent, std::process::id() as i32);
        assert_eq!(identity.session, unsafe { libc::getsid(0) });
        let mut held = Self {
            identity,
            stopped: false,
        };
        held.signal(libc::SIGSTOP);
        held.stopped = true;
        held
    }
    fn signal(&self, signal: i32) {
        let now = original_process_identity::process_identity(self.identity.pid)
            .unwrap()
            .unwrap();
        assert_eq!(now.uid, self.identity.uid);
        assert_eq!(now.birth, self.identity.birth);
        assert_eq!(now.session, self.identity.session);
        assert_eq!(now.pid, self.identity.pid);
        assert!(!now.zombie);
        // SAFETY: positive exact subprocess was freshly revalidated above; no
        // names, process groups or stale publication fragments are signalled.
        assert_eq!(unsafe { libc::kill(now.pid, signal) }, 0);
    }
    fn resume(mut self) {
        self.signal(libc::SIGCONT);
        self.stopped = false;
    }
}
impl Drop for HeldOriginalChild {
    fn drop(&mut self) {
        if self.stopped {
            if let Ok(Some(now)) = original_process_identity::process_identity(self.identity.pid) {
                if now.pid == self.identity.pid
                    && now.uid == self.identity.uid
                    && now.birth == self.identity.birth
                    && now.session == self.identity.session
                    && !now.zombie
                {
                    // SAFETY: exact current original child identity, never a reused PID.
                    unsafe {
                        libc::kill(now.pid, libc::SIGCONT);
                    }
                }
            }
        }
    }
}
fn wait_for_original_call(fixture: &Fixture, index: usize) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let bytes = fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
        let rows: Vec<Value> = bytes
            .split_terminator('\n')
            .enumerate()
            .filter_map(|(i, row)| {
                if i == bytes.matches('\n').count() && !bytes.ends_with('\n') {
                    return None;
                }
                Some(serde_json::from_str(row).expect("completed journal record is valid JSON"))
            })
            .collect();
        if let Some(row) = rows.get(index) {
            assert_eq!(row["matched"], true);
            let pid = row["pid"]
                .as_i64()
                .filter(|pid| *pid > 0 && *pid <= i64::from(i32::MAX))
                .unwrap();
            return pid as i32;
        }
        assert!(
            Instant::now() < deadline,
            "original child never reached its actual journal barrier"
        );
        std::thread::yield_now();
    }
}
use images::inline_v1::{
    ScopedInlineDraftV1, ScopedInlineIntentV1, ScopedInlinePreviewV1,
    ScopedInlinePublicationStateV1,
};

#[test]
fn inline_comments_intent_retains_original_range_head_and_stable_body_independent_key_without_reads(
) {
    let mut s = setup(false, |_| {});
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let range = result
        .proof
        .canonical_range(PrImageSideV1::Old, 1, 2)
        .unwrap();
    let intent = ScopedInlineIntentV1::capture(&range).unwrap();
    assert_eq!(intent.start(), 1);
    assert_eq!(intent.end(), 2);
    assert_eq!(intent.side(), crate::providers::gh::DiffSide::Left);
    assert_eq!(intent.review_head(), s.h.h);
    assert_eq!(intent.merge_base(), s.h.m);
    assert_ne!(intent.review_head(), intent.merge_base());
    assert_eq!(intent.server_path(), history::PATH);
    assert_eq!(intent.selected_image_path(), history::PATH.as_bytes());
    assert_eq!(intent.original_repository_id(), 410);
    assert_eq!(intent.original_pr_id(), 1007);
    assert_eq!(intent.scope().login, "user-id:42");
    let key = intent.draft_key().unwrap();
    assert!(key.starts_with("inline/") && key.len() == 71);
    assert_eq!(key, intent.draft_key().unwrap());
    let same = ScopedInlineIntentV1::capture(&range).unwrap();
    assert_eq!(same.draft_key().unwrap(), key);
    let right = ScopedInlineIntentV1::capture(
        &result
            .proof
            .canonical_range(PrImageSideV1::New, 1, 2)
            .unwrap(),
    )
    .unwrap();
    assert_ne!(right.draft_key().unwrap(), key);
    assert!(
        !intent.owns(&App::default()),
        "old range cannot adopt an unrelated current App"
    );
    intact(&s);
}

#[test]
fn inline_comments_fresh_preview_sha1_reverifies_actual_source_and_remains_read_only() {
    fresh_preview(false);
}
#[test]
fn inline_comments_fresh_preview_sha256_reverifies_actual_source_and_remains_read_only() {
    fresh_preview(true);
}
fn fresh_preview(sha256: bool) {
    let mut s = setup(sha256, |steps| {
        let extra = steps.clone();
        steps.extend(extra);
    });
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let intent = ScopedInlineIntentV1::capture(
        &result
            .proof
            .canonical_range(PrImageSideV1::Old, 1, 2)
            .unwrap(),
    )
    .unwrap();
    let before = commands(&s.fixture).len();
    let preview = ScopedInlinePreviewV1::prepare(
        intent.clone(),
        Arc::from("Original multiline inline body 界"),
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(commands(&s.fixture).len() - before, 17);
    assert!(Arc::ptr_eq(preview.intent(), &intent));
    assert_eq!(preview.body(), "Original multiline inline body 界");
    assert_eq!(preview.identity().account_id, Some(42));
    assert_eq!(preview.draft_key(), intent.draft_key().unwrap());
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains(&s.h.h) && line.contains("LEFT lines 1–2")));
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains("numeric account 42")));
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains("Explicit Confirm")));
    intact(&s);
}

fn commands(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn inline_comments_old_context_and_invalid_body_or_expired_preview_refuse_without_extra_requests() {
    let mut s = setup(false, |_| {});
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let context = ScopedInlineIntentV1::capture(
        &result
            .proof
            .canonical_range(PrImageSideV1::Old, 1, 1)
            .unwrap(),
    )
    .unwrap();
    let changed = ScopedInlineIntentV1::capture(
        &result
            .proof
            .canonical_range(PrImageSideV1::Old, 2, 2)
            .unwrap(),
    )
    .unwrap();
    for (intent, body, deadline) in [
        (
            context,
            Arc::<str>::from("not a deleted endpoint"),
            Instant::now() + Duration::from_secs(30),
        ),
        (
            changed.clone(),
            Arc::from(" "),
            Instant::now() + Duration::from_secs(30),
        ),
        (
            changed.clone(),
            Arc::from("x".repeat(65_537)),
            Instant::now() + Duration::from_secs(30),
        ),
        (
            changed.clone(),
            Arc::from("\0".repeat(65_536)),
            Instant::now() + Duration::from_secs(30),
        ),
        (changed, Arc::from("not a fresh deadline"), Instant::now()),
    ] {
        assert!(ScopedInlinePreviewV1::prepare(
            intent,
            body,
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            deadline
        )
        .is_err());
    }
    intact(&s);
}

const INLINE_BODY: &str = "Retained original source-bound inline body 界";

#[test]
fn inline_comments_post_transport_failure_keeps_uncertain_original_body_and_never_retries() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        let mut confirm = confirm_steps(h);
        confirm[11].status = 1;
        confirm[11].stdout.clear();
        confirm[11].stderr = b"PRIVATE_POST_FAILURE_DIAGNOSTIC".to_vec();
        confirm.truncate(12);
        steps.extend(confirm);
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let result = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(result.state(), ScopedInlinePublicationStateV1::Uncertain);
    assert!(result.hide_private());
    assert!(result.source_retired());
    assert!(result.confirmed_comment_id().is_none());
    assert!(result.draft_retained());
    assert_eq!(
        publication_marker(&s, &preview, result.reference()).state,
        crate::pr_write_journal::PrWriteState::Uncertain
    );
    retained_reference(&s, &preview, result.reference());
    assert!(!format!("{result:?}").contains("PRIVATE_POST_FAILURE_DIAGNOSTIC"));
    let before = commands(&s.fixture).len();
    assert!(preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            Some(result.reference()),
            &Default::default(),
            Instant::now() + Duration::from_secs(30)
        )
        .is_err());
    assert_eq!(commands(&s.fixture).len(), before);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn inline_comments_full_utf8_body_budget_remains_available_when_encoded_draft_is_bounded() {
    let mut s = inline_setup(|h, steps| steps.extend(image_steps(h)));
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let intent = ScopedInlineIntentV1::capture(
        &result
            .proof
            .canonical_range(PrImageSideV1::Old, 2, 2)
            .unwrap(),
    )
    .unwrap();
    let body = Arc::<str>::from("界".repeat(21_845) + "x");
    assert_eq!(body.len(), 65_536);
    let preview = ScopedInlinePreviewV1::prepare(
        intent,
        body,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(preview.body().len(), 65_536);
    intact(&s);
}

#[test]
fn inline_comments_draft_open_preflights_oversized_private_sql_before_copy_and_closes_no_new_body()
{
    let mut s =
        inline_setup(|h, steps| steps.extend(source_fence_steps(h, true).into_iter().take(4)));
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let intent = ScopedInlineIntentV1::capture(
        &result
            .proof
            .canonical_range(PrImageSideV1::Old, 2, 2)
            .unwrap(),
    )
    .unwrap();
    let key = intent.draft_key().unwrap();
    Cache::open(&database(&s.fixture))
        .unwrap()
        .put_draft(
            intent.scope(),
            &key,
            &"PRIVATE_OVERSIZED_DRAFT_BODY".repeat(64 * 1024),
        )
        .unwrap();
    let error = ScopedInlineDraftV1::open(
        intent,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap_err();
    assert!(error.hide_private);
    assert!(!format!("{error:?}").contains("PRIVATE_OVERSIZED_DRAFT_BODY"));
    assert_eq!(
        sql(&s.fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_restart_pending_reference_never_restores_body_approval_or_bypasses_source_barrier(
) {
    let mut s =
        inline_setup(|h, steps| steps.extend(source_fence_steps(h, true).into_iter().take(4)));
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let intent = ScopedInlineIntentV1::capture(
        &result
            .proof
            .canonical_range(PrImageSideV1::Old, 2, 2)
            .unwrap(),
    )
    .unwrap();
    let key = intent.draft_key().unwrap();
    let target = crate::pr_write_journal::PrWriteTarget::for_source(
        intent.scope(),
        REPO,
        "GENUINE_PR_NODE_1007",
        7,
    )
    .unwrap();
    let original = crate::pr_write_journal::PrWriteIntent {
        command: "inline-comment".into(),
        selected_resource: key.clone(),
        observed_head: s.h.h.clone(),
        values: std::collections::BTreeMap::new(),
    };
    let permit = PrWriteJournal::open(&database(&s.fixture))
        .unwrap()
        .begin(&target, &original)
        .unwrap();
    let reference = permit.marker().publication_reference();
    let encoded =
        json!({"version":2,"values":{"body":INLINE_BODY},"publication":reference}).to_string();
    Cache::open(&database(&s.fixture))
        .unwrap()
        .put_draft(intent.scope(), &key, &encoded)
        .unwrap();
    let error = ScopedInlineDraftV1::open(
        intent,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap_err();
    assert!(error.hide_private);
    assert!(!format!("{error:?}").contains(INLINE_BODY));
    let marker = PrWriteJournal::open(&database(&s.fixture))
        .unwrap()
        .original_publication(&target, &reference)
        .unwrap()
        .unwrap();
    assert_eq!(marker.state, crate::pr_write_journal::PrWriteState::Pending);
    assert_eq!(
        Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(target.scope(), &key)
            .unwrap()
            .as_deref(),
        Some(encoded.as_str())
    );
    unchanged_git_and_exact_script(&s);
}

#[test]
fn inline_comments_held_original_closing_read_observes_sql_clear_before_prepost_guard_and_never_posts(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        let mut confirm = confirm_steps(h);
        confirm[7].delay_ms = 1000;
        confirm.truncate(9);
        steps.extend(confirm);
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let before = commands(&s.fixture).len();
    let client = writer.client();
    std::thread::scope(|scope| {
        let worker = scope.spawn(|| {
            preview.execute(
                &provider(&s.fixture),
                &database(&s.fixture),
                &Default::default(),
                &Default::default(),
                &client,
                true,
                None,
                &Default::default(),
                Instant::now() + Duration::from_secs(30),
            )
        });
        let pid = wait_for_original_call(&s.fixture, before + 7);
        let held = HeldOriginalChild::stop(pid);
        assert_eq!(commands(&s.fixture).len(), before + 8);
        Cache::open(&database(&s.fixture))
            .unwrap()
            .clear_scope(preview.intent().scope(), false)
            .unwrap();
        held.resume();
        let result = worker.join().unwrap().unwrap();
        assert_eq!(result.state(), ScopedInlinePublicationStateV1::NotStarted);
        assert_eq!(
            publication_marker(&s, &preview, result.reference()).state,
            crate::pr_write_journal::PrWriteState::Pending
        );
        retained_reference(&s, &preview, result.reference());
    });
    s.sql_before = sql_state(&s.fixture)
        .into_iter()
        .filter(|row| {
            let value: Value = serde_json::from_str(row).unwrap();
            !value[2]
                .as_str()
                .is_some_and(|resource| resource.starts_with("inline/"))
        })
        .collect();
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_original_confirm_deadline_expires_during_actual_closing_child_without_post_or_renewal(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        let mut confirm = confirm_steps(h);
        confirm[7].delay_ms = 10_000;
        confirm.truncate(8);
        steps.extend(confirm);
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let started = Instant::now();
    let result = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            started + Duration::from_secs(2),
        )
        .unwrap();
    assert!(started.elapsed() < Duration::from_secs(4));
    assert_eq!(result.state(), ScopedInlinePublicationStateV1::NotStarted);
    assert_eq!(
        publication_marker(&s, &preview, result.reference()).state,
        crate::pr_write_journal::PrWriteState::Pending
    );
    retained_reference(&s, &preview, result.reference());
    unchanged_git_and_exact_script(&s);
}

#[test]
fn inline_comments_confirmed_receipt_journal_failure_keeps_pending_draft_real_id_and_terminal_unresolved_fence(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        steps.extend(confirm_steps(h).into_iter().take(13));
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    rusqlite::Connection::open(database(&s.fixture)).unwrap().execute_batch("CREATE TRIGGER refuse_inline_confirmation BEFORE UPDATE OF state ON pr_write_attempts WHEN NEW.state='confirmed' BEGIN SELECT RAISE(ABORT,'PRIVATE_CONFIRM_SQL_FAILURE'); END;").unwrap();
    let result = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(
        result.state(),
        ScopedInlinePublicationStateV1::ConfirmedButJournalFailed
    );
    assert_eq!(result.confirmed_comment_id(), Some(901));
    assert!(result.source_retired());
    assert!(result.hide_private());
    assert!(result.draft_retained());
    assert_eq!(
        publication_marker(&s, &preview, result.reference()).state,
        crate::pr_write_journal::PrWriteState::Pending
    );
    assert!(Cache::open(&database(&s.fixture))
        .unwrap()
        .validate_repository_catalogue_intent_source(&s.source)
        .is_err());
    retained_reference(&s, &preview, result.reference());
    let before = commands(&s.fixture).len();
    assert!(preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30)
        )
        .is_err());
    assert_eq!(commands(&s.fixture).len(), before);
    unchanged_git_and_exact_script(&s);
}

fn inline_setup(append: impl FnOnce(&history::GitImagesV1, &mut Vec<GhStep>)) -> Setup {
    let fixture = Fixture::new();
    let h = history::GitImagesV1::create(&fixture.repo(), false);
    let mut steps = script(&resource_steps(PrResourceKindV1::Files, REPO));
    bind_steps(&mut steps, &h);
    steps.extend(image_steps(&h));
    append(&h, &mut steps);
    fixture.scripted_gh(&steps);
    let (source, active, mut workspace) = opened(&fixture);
    let result = run_resource(
        &fixture,
        source.clone(),
        active.clone(),
        &mut workspace,
        PrResourceKindV1::Files,
        &Default::default(),
    )
    .unwrap();
    install_resource_if_current(&mut workspace, result).unwrap();
    let sql_before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    Setup {
        fixture,
        h,
        steps,
        source,
        active,
        original_files: workspace.scoped_pr_resource.as_ref().unwrap().clone(),
        workspace,
        sql_before,
        git_before,
    }
}
fn source_fence_steps(h: &history::GitImagesV1, both_parents: bool) -> Vec<GhStep> {
    let mut parent = resource_parent(REPO);
    h.bind_parent(&mut parent);
    let mut result = principal(42, "inline-publisher");
    result.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    result.push(get(&format!("repos/{REPO}/pulls/7"), parent.clone()));
    if both_parents {
        result.push(get(&format!("repos/{REPO}/pulls/7"), parent));
    }
    result.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    result.extend(principal(42, "inline-publisher"));
    result
}
fn inline_receipt(h: &history::GitImagesV1) -> Value {
    json!({"id":901,"node_id":"PRRC_901","pull_request_url":format!("https://{HOST}/repos/{REPO}/pulls/7"),"user":{"id":42,"login":"inline-publisher","type":"User"},"path":history::PATH,"side":"LEFT","line":2,"start_line":1,"start_side":"LEFT","commit_id":h.h,"body":INLINE_BODY})
}
fn confirm_steps(h: &history::GitImagesV1) -> Vec<GhStep> {
    let mut parent = resource_parent(REPO);
    h.bind_parent(&mut parent);
    // ONE existing Files producer P+3, not the legacy unmetered guard.
    let mut result = resource_steps(PrResourceKindV1::Files, REPO);
    bind_steps(&mut result, h);
    result.truncate(7); // actual user/version, repo, PR+page+patch+PR
    result.push(get(&format!("repos/{REPO}/pulls/7"), parent.clone()));
    result.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    result.extend(principal(42, "inline-publisher"));
    let payload = json!({"path":history::PATH,"side":"LEFT","line":2,"start_line":1,"start_side":"LEFT","commit_id":h.h,"body":INLINE_BODY});
    let mut post = GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/pulls/7/comments"),
            "--hostname",
            HOST,
            "--method",
            "POST",
            "--input",
            "-",
        ],
        &inline_receipt(h).to_string(),
    );
    post.stdin = serde_json::to_vec(&payload).unwrap();
    result.push(post);
    result.push(get(
        &format!("repos/{REPO}/pulls/comments/901"),
        inline_receipt(h),
    ));
    // SAME four prepaid postwrite slots, no session/version/pool reset.
    result.push(get("user", json!({"id":42,"login":"inline-publisher"})));
    result.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    result.push(get(&format!("repos/{REPO}/pulls/7"), parent));
    result.push(get("user", json!({"id":42,"login":"inline-publisher"})));
    assert_eq!(result.len(), 17);
    result
}
fn prepared(s: &mut Setup) -> Arc<ScopedInlinePreviewV1> {
    let result = read(s, &Default::default(), &Default::default()).unwrap();
    let intent = ScopedInlineIntentV1::capture(
        &result
            .proof
            .canonical_range(PrImageSideV1::Old, 1, 2)
            .unwrap(),
    )
    .unwrap();
    ScopedInlinePreviewV1::prepare(
        intent,
        Arc::from(INLINE_BODY),
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap()
}
fn unchanged_git_and_exact_script(s: &Setup) {
    assert_eq!(git_state_resource(&s.fixture), s.git_before);
    assert_eq!(
        fs::read(s.fixture.repo().join("LOCAL_DIRTY_CANARY")).unwrap(),
        b"LOCAL_WORKTREE_UNTOUCHED\0\xff\n"
    );
    let calls = commands(&s.fixture);
    assert_eq!(calls.len(), s.steps.len());
    for (call, step) in calls.iter().zip(&s.steps) {
        assert_eq!(call["matched"], true);
        assert_eq!(call["args"], json!(step.args));
        let stdin: String = step
            .stdin
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(call["stdin_hex"], stdin);
    }
    let current: Vec<_> = sql_state(&s.fixture)
        .into_iter()
        .filter(|row| {
            let value: Value = serde_json::from_str(row).unwrap();
            !value[2]
                .as_str()
                .is_some_and(|resource| resource.starts_with("inline/"))
        })
        .collect();
    assert_eq!(
        current, s.sql_before,
        "all other private draft/cache canaries remain unchanged"
    );
    let foreign: u64 = sql(&s.fixture).query_row("SELECT (SELECT count(*) FROM records WHERE login='user-id:43')+(SELECT count(*) FROM drafts WHERE login='user-id:43')+(SELECT count(*) FROM pr_write_attempts WHERE principal='user-id:43')", [], |row| row.get(0)).unwrap();
    assert_eq!(foreign, 0);
}

#[test]
fn inline_comments_confirm_persists_exact_attempt_posts_once_then_terminally_retires_original_source(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        steps.extend(confirm_steps(h));
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let before = commands(&s.fixture).len();
    let result = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(commands(&s.fixture).len() - before, 17);
    assert_eq!(
        result.state(),
        ScopedInlinePublicationStateV1::ConfirmedSnapshotRetired
    );
    assert!(result.hide_private());
    assert!(result.source_retired());
    assert!(!result.draft_retained());
    assert_eq!(result.confirmed_comment_id(), Some(901));
    assert!(!format!("{result:?}").contains(INLINE_BODY));
    assert!(Cache::open(&database(&s.fixture))
        .unwrap()
        .validate_repository_catalogue_intent_source(&s.source)
        .is_err());
    let journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let target = crate::pr_write_journal::PrWriteTarget::for_source(
        preview.intent().scope(),
        REPO,
        "GENUINE_PR_NODE_1007",
        7,
    )
    .unwrap();
    let marker = journal
        .original_publication(&target, result.reference())
        .unwrap()
        .unwrap();
    assert_eq!(marker.command, "inline-comment");
    assert_eq!(marker.selected_resource, preview.draft_key());
    assert_eq!(
        marker.state,
        crate::pr_write_journal::PrWriteState::Confirmed
    );
    assert_eq!(marker.confirmed.as_ref().unwrap().resource_id, "901");
    assert!(journal.blocking(&target).unwrap().is_none());
    assert!(Cache::open(&database(&s.fixture))
        .unwrap()
        .draft(preview.intent().scope(), preview.draft_key())
        .unwrap()
        .is_none());
    let count = commands(&s.fixture).len();
    assert!(preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30)
        )
        .is_err());
    assert_eq!(commands(&s.fixture).len(), count);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn inline_comments_draft_open_closes_original_source_and_never_restores_confirmation() {
    let mut s = inline_setup(|h, steps| {
        let mut fence = source_fence_steps(h, true);
        fence.last_mut().unwrap().stdout = json!({"id":42,"login":"actual-final-draft-reader"})
            .to_string()
            .into_bytes();
        steps.extend(fence);
    });
    let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
    let intent = ScopedInlineIntentV1::capture(
        &result
            .proof
            .canonical_range(PrImageSideV1::Old, 2, 2)
            .unwrap(),
    )
    .unwrap();
    let key = intent.draft_key().unwrap();
    Cache::open(&database(&s.fixture))
        .unwrap()
        .put_draft(
            intent.scope(),
            &key,
            &json!({"version":2,"values":{"body":INLINE_BODY},"publication":null}).to_string(),
        )
        .unwrap();
    let before = commands(&s.fixture).len();
    let draft = ScopedInlineDraftV1::open(
        intent.clone(),
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(commands(&s.fixture).len() - before, 8);
    assert_eq!(draft.body(), INLINE_BODY);
    assert!(draft.publication().is_none());
    assert_eq!(draft.identity().account_id, Some(42));
    assert_eq!(draft.identity().login, "actual-final-draft-reader");
    assert_eq!(draft.draft_key(), key);
    assert!(Arc::ptr_eq(draft.intent(), &intent));
    unchanged_git_and_exact_script(&s);
}

fn publication_marker(
    s: &Setup,
    preview: &ScopedInlinePreviewV1,
    reference: &crate::pr_write_journal::PrPublicationReference,
) -> crate::pr_write_journal::PrWriteMarker {
    let target = crate::pr_write_journal::PrWriteTarget::for_source(
        preview.intent().scope(),
        REPO,
        "GENUINE_PR_NODE_1007",
        7,
    )
    .unwrap();
    PrWriteJournal::open(&database(&s.fixture))
        .unwrap()
        .original_publication(&target, reference)
        .unwrap()
        .unwrap()
}
fn retained_reference(
    s: &Setup,
    preview: &ScopedInlinePreviewV1,
    reference: &crate::pr_write_journal::PrPublicationReference,
) {
    let saved = Cache::open(&database(&s.fixture))
        .unwrap()
        .bounded_draft(preview.intent().scope(), preview.draft_key(), 128 * 1024)
        .unwrap()
        .unwrap();
    let value: Value = serde_json::from_str(&saved).unwrap();
    assert_eq!(value["values"]["body"], INLINE_BODY);
    assert_eq!(
        value["publication"],
        serde_json::to_value(reference).unwrap()
    );
    assert!(value.get("approved").is_none());
}
#[test]
fn inline_comments_prepost_changed_parent_retains_pending_reference_and_never_claims_uncertainty() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        let mut confirm = confirm_steps(h);
        let mut changed: Value = serde_json::from_slice(&confirm[7].stdout).unwrap();
        changed["head"]["sha"] = json!(h.m);
        confirm[7].stdout = changed.to_string().into_bytes();
        confirm.truncate(8);
        steps.extend(confirm);
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let before = commands(&s.fixture).len();
    let result = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(commands(&s.fixture).len() - before, 8);
    assert_eq!(result.state(), ScopedInlinePublicationStateV1::NotStarted);
    assert!(result.hide_private());
    assert!(result.confirmed_comment_id().is_none());
    assert_eq!(
        publication_marker(&s, &preview, result.reference()).state,
        crate::pr_write_journal::PrWriteState::Pending
    );
    retained_reference(&s, &preview, result.reference());
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_mismatched_post_receipt_is_uncertain_with_draft_and_no_readback_or_resend() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        let mut confirm = confirm_steps(h);
        let mut changed: Value = serde_json::from_slice(&confirm[11].stdout).unwrap();
        changed["user"]["id"] = json!(43);
        confirm[11].stdout = changed.to_string().into_bytes();
        confirm.truncate(12);
        steps.extend(confirm);
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let result = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(result.state(), ScopedInlinePublicationStateV1::Uncertain);
    assert!(result.confirmed_comment_id().is_none());
    assert!(result.hide_private());
    assert_eq!(
        publication_marker(&s, &preview, result.reference()).state,
        crate::pr_write_journal::PrWriteState::Uncertain
    );
    retained_reference(&s, &preview, result.reference());
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_post_confirmed_then_account_loss_hides_receipt_but_keeps_exact_confirmation() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        let mut confirm = confirm_steps(h);
        confirm[13].stdout = json!({"id":43,"login":"different-account"})
            .to_string()
            .into_bytes();
        confirm.truncate(14);
        steps.extend(confirm);
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let result = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(
        result.state(),
        ScopedInlinePublicationStateV1::ConfirmedButSourceUnavailable
    );
    assert!(result.hide_private());
    assert_eq!(result.confirmed_comment_id(), Some(901));
    assert!(result.source_retired());
    let marker = publication_marker(&s, &preview, result.reference());
    assert_eq!(
        marker.state,
        crate::pr_write_journal::PrWriteState::Confirmed
    );
    assert_eq!(marker.confirmed.unwrap().resource_id, "901");
    retained_reference(&s, &preview, result.reference());
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_unconfirmed_expired_canceled_or_restored_confirmation_never_starts_a_child() {
    let mut s = inline_setup(|h, steps| steps.extend(image_steps(h)));
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let restored = crate::pr_write_journal::PrPublicationReference {
        attempt_id: 1,
        pr_node_id: "GENUINE_PR_NODE_1007".into(),
        observed_head: s.h.h.clone(),
        payload_sha256: "a".repeat(64),
    };
    let canceled = CancellationToken::default();
    canceled.cancel();
    for (confirmed, reference, cancel, deadline) in [
        (
            false,
            None,
            CancellationToken::default(),
            Instant::now() + Duration::from_secs(30),
        ),
        (
            true,
            Some(&restored),
            CancellationToken::default(),
            Instant::now() + Duration::from_secs(30),
        ),
        (
            true,
            None,
            canceled,
            Instant::now() + Duration::from_secs(30),
        ),
        (true, None, CancellationToken::default(), Instant::now()),
    ] {
        assert!(preview
            .execute(
                &provider(&s.fixture),
                &database(&s.fixture),
                &Default::default(),
                &Default::default(),
                &writer.client(),
                confirmed,
                reference,
                &cancel,
                deadline
            )
            .is_err());
    }
    intact(&s);
}

const RENAMED_PATH: &str = "src/renamed-界.rs";
fn actual_git(repo: &std::path::Path, args: &[&str]) -> Vec<u8> {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "actual renamed fixture: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn actual_git_text(repo: &std::path::Path, args: &[&str]) -> String {
    String::from_utf8(actual_git(repo, args))
        .unwrap()
        .trim()
        .into()
}
fn actual_renamed_trees(repo: &std::path::Path, root: &str) -> Vec<(String, Value)> {
    let mut object = root.to_owned();
    let mut result = vec![];
    for part in RENAMED_PATH.split('/') {
        let bytes = actual_git(repo, &["ls-tree", "-z", &object]);
        let mut selected = None;
        let rows: Vec<Value> = bytes
            .split(|byte| *byte == 0)
            .filter(|row| !row.is_empty())
            .map(|row| {
                let tab = row.iter().position(|byte| *byte == b'\t').unwrap();
                let fields: Vec<_> = std::str::from_utf8(&row[..tab])
                    .unwrap()
                    .split(' ')
                    .collect();
                let name = std::str::from_utf8(&row[tab + 1..]).unwrap();
                if name == part {
                    selected = Some((fields[1].to_owned(), fields[2].to_owned()));
                }
                let mut value =
                    json!({"mode":fields[0],"type":fields[1],"sha":fields[2],"path":name});
                if fields[1] == "blob" {
                    value["size"] = json!(actual_git_text(repo, &["cat-file", "-s", fields[2]])
                        .parse::<u64>()
                        .unwrap());
                }
                value
            })
            .collect();
        result.push((
            object.clone(),
            json!({"sha":object,"truncated":false,"tree":rows}),
        ));
        let (kind, next) = selected.unwrap();
        if kind != "tree" {
            break;
        }
        object = next;
    }
    result
}
fn renamed_setup(sha256: bool) -> Setup {
    let fixture = Fixture::new();
    let common: String = (0..30)
        .map(|i| format!("genuine unchanged context {i}\n"))
        .collect();
    let old = format!("{common}old deleted line\n");
    let new = format!("{common}new added line\n");
    let mut h = history::GitImagesV1::create_with_bytes(
        &fixture.repo(),
        sha256,
        old.as_bytes(),
        new.as_bytes(),
    );
    actual_git(&fixture.repo(), &["switch", "feature"]);
    actual_git(&fixture.repo(), &["mv", "--", history::PATH, RENAMED_PATH]);
    actual_git(&fixture.repo(), &["commit", "--amend", "--no-edit"]);
    h.h = actual_git_text(&fixture.repo(), &["rev-parse", "HEAD"]);
    h.th = actual_git_text(&fixture.repo(), &["rev-parse", "HEAD^{tree}"]);
    h.new_trees = actual_renamed_trees(&fixture.repo(), &h.th);
    h.patch = actual_git(
        &fixture.repo(),
        &[
            "diff",
            "--no-ext-diff",
            "--no-textconv",
            "--full-index",
            "--find-renames=50%",
            &h.m,
            &h.h,
        ],
    );
    assert!(String::from_utf8_lossy(&h.patch).contains("rename from src/界.rs"));
    assert!(String::from_utf8_lossy(&h.patch).contains("rename to src/renamed-界.rs"));
    assert_eq!(
        actual_git_text(&fixture.repo(), &["merge-base", &h.b, &h.h]),
        h.m
    );
    assert_eq!(
        actual_git_text(
            &fixture.repo(),
            &["rev-parse", &format!("{}:{RENAMED_PATH}", h.h)]
        ),
        h.new_blob
    );
    actual_git(&fixture.repo(), &["switch", "main"]);
    let mut file = h.file();
    file["filename"] = json!(RENAMED_PATH);
    file["previous_filename"] = json!(history::PATH);
    file["status"] = json!("renamed");
    let mut steps = script(&resource_steps(PrResourceKindV1::Files, REPO));
    bind_steps(&mut steps, &h);
    for step in &mut steps {
        if step
            .args
            .get(1)
            .is_some_and(|endpoint| endpoint.contains("/pulls/7/files"))
        {
            step.stdout = if step.args.iter().any(|arg| arg == "--slurp") {
                json!([[file]])
            } else {
                json!([file])
            }
            .to_string()
            .into_bytes();
        }
    }
    steps.extend(image_steps(&h));
    steps.extend(image_steps(&h));
    fixture.scripted_gh(&steps);
    let (source, active, mut workspace) = opened(&fixture);
    let result = run_resource(
        &fixture,
        source.clone(),
        active.clone(),
        &mut workspace,
        PrResourceKindV1::Files,
        &Default::default(),
    )
    .unwrap();
    install_resource_if_current(&mut workspace, result).unwrap();
    let sql_before = sql_state(&fixture);
    let git_before = git_state_resource(&fixture);
    Setup {
        fixture,
        h,
        steps,
        source,
        active,
        original_files: workspace.scoped_pr_resource.as_ref().unwrap().clone(),
        workspace,
        sql_before,
        git_before,
    }
}
#[test]
fn inline_comments_renamed_left_retains_actual_old_path_but_api_filename_is_new_sha1_and_sha256() {
    for sha256 in [false, true] {
        let mut s = renamed_setup(sha256);
        let result = read(&mut s, &Default::default(), &Default::default()).unwrap();
        let intent = ScopedInlineIntentV1::capture(
            &result
                .proof
                .canonical_range(PrImageSideV1::Old, 30, 31)
                .unwrap(),
        )
        .unwrap();
        assert_eq!(intent.server_path(), RENAMED_PATH);
        assert_eq!(intent.selected_image_path(), history::PATH.as_bytes());
        assert_ne!(
            intent.server_path().as_bytes(),
            intent.selected_image_path()
        );
        assert_eq!(intent.review_head(), s.h.h);
        assert_eq!(intent.merge_base(), s.h.m);
        let preview = ScopedInlinePreviewV1::prepare(
            intent,
            Arc::from(INLINE_BODY),
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert!(preview
            .lines()
            .iter()
            .any(|line| line == "Canonical API filename: src/renamed-界.rs"));
        assert!(preview
            .lines()
            .iter()
            .any(|line| line == "Selected image path: src/界.rs"));
        intact(&s);
    }
}

#[test]
fn inline_comments_changed_uid_before_confirm_has_two_reads_no_attempt_or_post() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        steps.extend(principal(43, "another-numeric-account"));
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let before = commands(&s.fixture).len();
    let error = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap_err();
    assert!(error.hide_private);
    assert_eq!(commands(&s.fixture).len() - before, 2);
    assert_eq!(
        sql(&s.fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_cleared_original_sql_new_inode_or_new_pending_generation_refuses_before_own_begin(
) {
    for kind in 0..3 {
        let mut s = inline_setup(|h, steps| steps.extend(image_steps(h)));
        let preview = prepared(&mut s);
        match kind {
            0 => Cache::open(&database(&s.fixture))
                .unwrap()
                .clear_scope(preview.intent().scope(), false)
                .unwrap(),
            1 => {
                let old = database(&s.fixture).with_file_name("original-private-cache.sqlite3");
                fs::rename(database(&s.fixture), &old).unwrap();
                drop(Cache::open(&database(&s.fixture)).unwrap());
                drop(PrWriteJournal::open(&database(&s.fixture)).unwrap());
                assert!(Cache::open(&old)
                    .unwrap()
                    .draft(preview.intent().scope(), "original-review")
                    .unwrap()
                    .is_some());
            }
            _ => {
                let target = crate::pr_write_journal::PrWriteTarget::for_source(
                    preview.intent().scope(),
                    REPO,
                    "GENUINE_PR_NODE_1007",
                    7,
                )
                .unwrap();
                let intent = crate::pr_write_journal::PrWriteIntent {
                    command: "new-comment".into(),
                    selected_resource: "pr".into(),
                    observed_head: s.h.h.clone(),
                    values: std::collections::BTreeMap::new(),
                };
                PrWriteJournal::open(&database(&s.fixture))
                    .unwrap()
                    .begin(&target, &intent)
                    .unwrap();
            }
        }
        s.sql_before = sql_state(&s.fixture);
        let writer = DraftWriter::new(database(&s.fixture));
        let error = preview
            .execute(
                &provider(&s.fixture),
                &database(&s.fixture),
                &Default::default(),
                &Default::default(),
                &writer.client(),
                true,
                None,
                &Default::default(),
                Instant::now() + Duration::from_secs(30),
            )
            .unwrap_err();
        assert!(error.hide_private);
        let count: u64 = sql(&s.fixture)
            .query_row("SELECT count(*) FROM pr_write_attempts", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, u64::from(kind == 2));
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn inline_comments_failed_ordered_draft_persistence_keeps_pending_and_never_starts_provider_validation(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        steps.extend(confirm_steps(h).into_iter().take(7));
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let client = writer.client();
    let cancel = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let publication = client
        .capture_publication_v2(
            &database(&s.fixture),
            preview.intent().scope(),
            preview.draft_key(),
            &cancel,
            deadline,
        )
        .unwrap();
    drop(writer);
    let result = preview
        .execute_with_publication_v14(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &client,
            publication,
            true,
            None,
            &cancel,
            deadline,
        )
        .unwrap();
    assert_eq!(result.state(), ScopedInlinePublicationStateV1::NotStarted);
    assert!(
        !result.draft_retained(),
        "failed save cannot claim durable body/reference exists"
    );
    assert!(result.confirmed_comment_id().is_none());
    assert_eq!(
        publication_marker(&s, &preview, result.reference()).state,
        crate::pr_write_journal::PrWriteState::Pending
    );
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_final_prepost_principal_change_retains_pending_without_post_or_visibility_fence()
{
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        let mut confirm = confirm_steps(h);
        confirm[10].stdout = json!({"id":43,"login":"wrong-final-principal"})
            .to_string()
            .into_bytes();
        confirm.truncate(11);
        steps.extend(confirm);
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let result = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(result.state(), ScopedInlinePublicationStateV1::NotStarted);
    assert!(!result.source_retired());
    assert_eq!(
        publication_marker(&s, &preview, result.reference()).state,
        crate::pr_write_journal::PrWriteState::Pending
    );
    retained_reference(&s, &preview, result.reference());
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_bad_readback_is_uncertain_and_retains_original_reference_without_resend() {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        let mut confirm = confirm_steps(h);
        let mut value: Value = serde_json::from_slice(&confirm[12].stdout).unwrap();
        value["start_side"] = json!("RIGHT");
        confirm[12].stdout = value.to_string().into_bytes();
        confirm.truncate(13);
        steps.extend(confirm);
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    let result = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(result.state(), ScopedInlinePublicationStateV1::Uncertain);
    assert!(result.confirmed_comment_id().is_none());
    assert_eq!(
        publication_marker(&s, &preview, result.reference()).state,
        crate::pr_write_journal::PrWriteState::Uncertain
    );
    retained_reference(&s, &preview, result.reference());
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_final_fence_failure_keeps_real_confirmed_id_and_hides_body_without_rebasing_source(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(image_steps(h));
        steps.extend(confirm_steps(h));
    });
    let preview = prepared(&mut s);
    let writer = DraftWriter::new(database(&s.fixture));
    rusqlite::Connection::open(database(&s.fixture)).unwrap().execute_batch("CREATE TRIGGER refuse_inline_fence BEFORE INSERT ON sync_mutation_fences BEGIN SELECT RAISE(ABORT,'PRIVATE_SQL_FAILURE'); END;").unwrap();
    let result = preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
    assert_eq!(
        result.state(),
        ScopedInlinePublicationStateV1::ConfirmedButSourceUnavailable
    );
    assert_eq!(result.confirmed_comment_id(), Some(901));
    assert!(!result.source_retired());
    assert!(result.hide_private());
    assert!(Cache::open(&database(&s.fixture))
        .unwrap()
        .validate_repository_catalogue_intent_source(&s.source)
        .is_ok());
    assert_eq!(
        publication_marker(&s, &preview, result.reference()).state,
        crate::pr_write_journal::PrWriteState::Confirmed
    );
    assert!(!format!("{result:?}").contains("PRIVATE_SQL_FAILURE"));
    let before = commands(&s.fixture).len();
    assert!(preview
        .execute(
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &writer.client(),
            true,
            None,
            &Default::default(),
            Instant::now() + Duration::from_secs(30)
        )
        .is_err());
    assert_eq!(commands(&s.fixture).len(), before);
    unchanged_git_and_exact_script(&s);
}
