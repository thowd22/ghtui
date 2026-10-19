//! Genuine typed PATCH and held actual readback. The old Confirm is orphaned
//! by a real replacement Inbox job; this is not a manufactured completion.
use super::*;
use crate::owned_pty_processes_test_support as process_observation;
use std::sync::mpsc;
const LIFECYCLE_BODY: &str = "Exact actual comment lifecycle replacement";
struct Held(process_observation::ProcessIdentity);
impl Held {
    fn stop(pid: i32) -> Self {
        let parent = process_observation::process_identity(std::process::id() as i32)
            .unwrap()
            .unwrap();
        let actual = process_observation::process_identity(pid)
            .unwrap()
            .expect("actual original held readback child live");
        assert!(pid > 0 && pid != parent.pid && !actual.zombie);
        assert_eq!(actual.parent, parent.pid);
        assert_eq!(actual.uid, parent.uid);
        assert_eq!(actual.session, parent.session);
        assert!(actual.birth >= parent.birth);
        assert_eq!(
            process_observation::process_identity(pid).unwrap().as_ref(),
            Some(&actual)
        );
        // SAFETY: exact own positive direct child, UID/birth/SID rechecked.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        Self(actual)
    }
}
impl Drop for Held {
    fn drop(&mut self) {
        if let Ok(Some(current)) = process_observation::process_identity(self.0.pid) {
            if !current.zombie
                && (current.pid, current.uid, current.birth, current.session)
                    == (self.0.pid, self.0.uid, self.0.birth, self.0.session)
            {
                // SAFETY: only the exact independently revalidated original child.
                unsafe { libc::kill(current.pid, libc::SIGCONT) };
            }
        }
    }
}
fn held_confirm() -> Vec<GhStep> {
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(observation(selected_comment(), current_rest()));
    steps.extend(mutation_guard());
    steps.extend(close_steps());
    let observed = rest_comment(LIFECYCLE_BODY, "2026-10-15T03:00:00Z");
    let mut patch = GhStep::read(
        &[
            "api",
            &format!("repos/{REPO}/pulls/comments/9009"),
            "--hostname",
            HOST,
            "--method",
            "PATCH",
            "--input",
            "-",
        ],
        &observed.to_string(),
    );
    patch.stdin = serde_json::to_vec(&json!({"body":LIFECYCLE_BODY})).unwrap();
    steps.push(patch);
    let mut read = get(&format!("repos/{REPO}/pulls/comments/9009"), observed);
    read.delay_ms = 5000;
    steps.push(read);
    assert_eq!(steps.len(), 22);
    steps
}
fn rows(real: &Real) -> Vec<Value> {
    fs::read_to_string(real.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn wait_calls(real: &Real, count: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    while rows(real).len() < count {
        assert!(
            Instant::now() < until,
            "actual original readback child never started"
        );
        std::thread::yield_now();
    }
}
fn finish(background: &mut Background, app: &mut App) {
    let until = Instant::now() + Duration::from_secs(8);
    loop {
        background.poll(app);
        if app.status == "GENUINE NEWER INBOX OWNER COMPLETED"
            && channel::generation(background).is_none()
        {
            return;
        }
        assert!(
            Instant::now() < until,
            "actual replacement worker completion not admitted"
        );
        std::thread::yield_now();
    }
}
#[test]
fn review_threads_v2_comment_actual_patch_held_readback_orphan_preserves_new_slot_source_navigation_and_form(
) {
    //0 original presentation must hide;1 genuine SAME-source new nav/resource;
    //2 independently produced source;3 same original context NEW Form ticket.
    for successor in 0..4 {
        let mut extra = fresh_comment_steps();
        extra.extend(fresh_comment_steps());
        if successor == 1 {
            extra.extend(operation(REPO, 42));
            extra.extend(resource_steps(PrResourceKindV1::Files, REPO));
        }
        if successor == 3 {
            extra.extend(fresh_comment_steps());
        }
        extra.extend(held_confirm());
        let mut real = browser(extra);
        let chosen = intent(&real, CommentActionV2::Edit);
        let mut form = open(&real, chosen.clone());
        composer::replace_editor_result(&mut form, LIFECYCLE_BODY).unwrap();
        let request = composer::request(&form).unwrap();
        let preview = publish::CommentPreviewV2::prepare(
            chosen.clone(),
            request.body().clone(),
            &provider(&real.fixture),
            &database(&real.fixture),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert!(composer::accept_preview(&mut form, &request, preview));
        real.app.form = Some(form);
        real.app.overlay = Some(Overlay::Form);
        real.app.form_dismissed = false;
        // Cross the same actual presentation boundary as the real binary:
        // freshly prepared worker data is not a useful painted Preview yet.
        let prepared_display = real
            .app
            .form
            .as_ref()
            .unwrap()
            .preview_display_v1
            .document
            .clone()
            .unwrap();
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut real.app))
            .unwrap();
        let painted: String = terminal
            .backend()
            .buffer()
            .content()
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(painted.contains("READ-ONLY PREVIEW") && painted.contains("[Confirm action]"));
        let form = real.app.form.as_ref().unwrap();
        assert!(Arc::ptr_eq(
            form.preview_display_v1.document.as_ref().unwrap(),
            &prepared_display
        ));
        assert!(!form.preview_display_v1.geometry_rejected);
        let replacement_workspace = if successor == 1 {
            let source = real.app.remote_scope_source.clone().unwrap();
            let active = real.app.remote_scope.clone().unwrap();
            let mut workspace = call(
                &real.fixture,
                source.clone(),
                active.clone(),
                previous(&source),
            )
            .unwrap()
            .workspace;
            let result = run_resource(
                &real.fixture,
                source,
                active,
                &mut workspace,
                PrResourceKindV1::Files,
                &Default::default(),
            )
            .unwrap();
            install_resource_if_current(&mut workspace, result).unwrap();
            Some(workspace)
        } else {
            None
        };
        let mut independent = if successor == 2 {
            Some(super::super::super::super::real(vec![]))
        } else {
            None
        };
        // A NEW actual source/draft opener yields another presentation ticket;
        // cloning the old Form would not establish independent ownership.
        let mut newer_form = if successor == 3 {
            let mut form = open(&real, chosen);
            composer::replace_editor_result(&mut form, "NEWER ACTUAL COMMENT FORM BODY").unwrap();
            Some(form)
        } else {
            None
        };
        let mut writer = crate::drafts::DraftWriter::new(database(&real.fixture));
        let drafts = writer.client();
        let scope = real.app.form.as_ref().unwrap().scope();
        let key = real.app.form.as_ref().unwrap().resource();
        writer
            .save(
                scope.clone(),
                key.clone(),
                composer::encoded_draft(real.app.form.as_ref().unwrap()).unwrap(),
            )
            .unwrap();
        real.app
            .form
            .as_mut()
            .unwrap()
            .dispatch(&crate::forms::FormAction::Submit);
        native_v1::capture_form_with_drafts(&mut real.app, &database(&real.fixture), &drafts);
        let owner = real.app.scoped_threads_work_v1.take().unwrap();
        assert_eq!(owner.phase(), native_v1::ScopedThreadPhaseV1::Confirm);
        let mut background = Background::new();
        assert!(background.submit_scoped_threads_with_drafts_v1(
            &mut real.app,
            owner.clone(),
            provider(&real.fixture),
            database(&real.fixture),
            drafts
        ));
        wait_calls(&real, real.steps.len());
        let calls = rows(&real);
        assert_eq!(calls[calls.len() - 2]["args"][5], "PATCH");
        assert_eq!(
            calls[calls.len() - 1]["args"][1],
            format!("repos/{REPO}/pulls/comments/9009")
        );
        let held =
            Held::stop(i32::try_from(calls.last().unwrap()["pid"].as_u64().unwrap()).unwrap());
        let encoded = Cache::open(&database(&real.fixture))
            .unwrap()
            .draft(&scope, &key)
            .unwrap()
            .unwrap();
        let value: Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["values"]["body"], LIFECYCLE_BODY);
        assert!(value["publication"].is_object());
        assert_eq!(
            sql(&real.fixture)
                .query_row("SELECT state FROM pr_write_attempts", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "pending"
        );
        if let Some(workspace) = replacement_workspace {
            real.app.inbox_list.selected = (0..workspace.pr.as_ref().unwrap().rows.len())
                .find(|index| {
                    matches!(
                        workspace.scoped_pr_resource.as_ref().unwrap().row(*index),
                        Some(PrResourceRowV1::FileList(0))
                    )
                })
                .unwrap();
            real.app.github = Some(workspace);
        }
        if let Some(newer) = &mut independent {
            real.app.inbox_list.selected = newer.app.inbox_list.selected;
            real.app.github = newer.app.github.take();
            real.app.remote_scope = newer.app.remote_scope.take();
            real.app.remote_scope_source = newer.app.remote_scope_source.take();
        }
        if let Some(form) = newer_form.take() {
            real.app.form = Some(form);
        }
        let source = real.app.remote_scope_source.clone().unwrap();
        let active = real.app.remote_scope.clone().unwrap();
        let nav = real
            .app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .clone()
            .unwrap();
        let files = real
            .app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .clone()
            .unwrap();
        let (started, observed) = mpsc::sync_channel(1);
        let (release, wait) = mpsc::sync_channel(1);
        assert!(
            background.submit(&mut real.app, View::Inbox, move |cancel| {
                started.send(cancel.clone()).unwrap();
                wait.recv_timeout(Duration::from_secs(8))
                    .map_err(|_| "replacement not released".to_owned())?;
                if cancel.is_cancelled() {
                    return Err("old Confirm canceled a new Inbox slot".into());
                }
                Ok(crate::runtime::PreparedView {
                    status: "GENUINE NEWER INBOX OWNER COMPLETED".into(),
                    ..Default::default()
                })
            })
        );
        let token = observed.recv_timeout(Duration::from_secs(8)).unwrap();
        let generation = channel::generation(&background);
        let loading = real.app.loading;
        real.app.status = "NEWER INBOX STATUS MUST SURVIVE ORPHAN".into();
        background.retire_stale_scoped_threads_v1(&mut real.app);
        assert!(!token.is_cancelled());
        assert!(owner.cancellation().is_cancelled());
        assert_eq!(channel::generation(&background), generation);
        assert_eq!(real.app.loading, loading);
        assert_eq!(real.app.status, "NEWER INBOX STATUS MUST SURVIVE ORPHAN");
        assert!(real.app.scoped_threads_owner_v1.is_none());
        assert!(Arc::ptr_eq(
            real.app.remote_scope_source.as_ref().unwrap(),
            &source
        ));
        assert!(Arc::ptr_eq(
            real.app.remote_scope.as_ref().unwrap(),
            &active
        ));
        assert!(Arc::ptr_eq(
            real.app
                .github
                .as_ref()
                .unwrap()
                .scoped_pr_navigation
                .as_ref()
                .unwrap(),
            &nav
        ));
        if successor == 0 {
            assert!(real.app.github.as_ref().unwrap().pr.is_none());
            assert!(real.app.form.is_none());
        } else {
            assert!(real.app.github.as_ref().unwrap().pr.is_some());
            assert!(Arc::ptr_eq(
                real.app
                    .github
                    .as_ref()
                    .unwrap()
                    .scoped_pr_resource
                    .as_ref()
                    .unwrap(),
                &files
            ));
            if successor == 3 {
                assert_eq!(
                    real.app.form.as_ref().unwrap().fields[0].editor.text,
                    "NEWER ACTUAL COMMENT FORM BODY"
                );
                assert!(real
                    .app
                    .scoped_threads_v1
                    .as_ref()
                    .unwrap()
                    .comments_v2
                    .is_some());
            }
        }
        release.send(()).unwrap();
        finish(&mut background, &mut real.app);
        assert_eq!(real.app.status, "GENUINE NEWER INBOX OWNER COMPLETED");
        assert!(channel::generation(&background).is_none());
        let until = Instant::now() + Duration::from_secs(8);
        while sql(&real.fixture)
            .query_row("SELECT state FROM pr_write_attempts", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap()
            != "uncertain"
        {
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
        let until = Instant::now() + Duration::from_secs(8);
        loop {
            match process_observation::process_identity(held.0.pid) {
                Ok(None) => break,
                Ok(Some(current))
                    if (current.uid, current.birth, current.session)
                        != (held.0.uid, held.0.birth, held.0.session) =>
                {
                    break
                }
                Ok(Some(_)) => {}
                Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {}
                Err(error) => {
                    panic!("Exact original readback identity observation failed: {error}")
                }
            }
            assert!(
                Instant::now() < until,
                "Exact original readback PID/UID/birth/SID not proven gone/reaped"
            );
            std::thread::yield_now();
        }
        assert_eq!(
            Cache::open(&database(&real.fixture))
                .unwrap()
                .draft(&scope, &key)
                .unwrap()
                .as_deref(),
            Some(encoded.as_str())
        );
        assert_eq!(
            rows(&real)
                .iter()
                .filter(|row| row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg == "PATCH"))
                .count(),
            1
        );
        assert!(rows(&real).iter().all(|row| row["matched"] == true));
        assert_eq!(git_state_resource(&real.fixture), real.git);
        drop(held);
        real.sql = sql_state(&real.fixture);
        exact_protocol(&real, 1);
        if let Some(newer) = independent {
            protocol(&newer, 0);
        }
    }
}
