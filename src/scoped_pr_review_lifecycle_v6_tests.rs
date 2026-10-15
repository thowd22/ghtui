//! V5-relative lifecycle declarations. Actual queued Background and real typed
//! POST/readback children; no fabricated source/receipt/result or new queue.
//! All declarations remain UNRUN until Root's serialized verification.
use super::*;
use crate::owned_pty_processes_test_support as process_observation;
use images::reviews_v2::{ui_v2, ReviewRequestedActionV2, ScopedReviewPreviewV2};
use std::sync::mpsc;
struct HeldChild(process_observation::ProcessIdentity);
impl HeldChild {
    fn stop(pid: i32) -> Self {
        let parent = process_observation::process_identity(std::process::id() as i32)
            .unwrap()
            .unwrap();
        let child = process_observation::process_identity(pid)
            .unwrap()
            .expect("actual readback child is still live");
        assert!(pid > 0 && pid != parent.pid && !child.zombie);
        assert_eq!(child.parent, parent.pid);
        assert_eq!(child.uid, parent.uid);
        assert_eq!(child.session, parent.session);
        assert!(child.birth >= parent.birth);
        let check = process_observation::process_identity(pid).unwrap().unwrap();
        assert_eq!(check, child);
        // SAFETY: exact positive original child, current UID/birth/SID/parent
        // revalidated immediately before signal, no name or process group.
        assert_eq!(unsafe { libc::kill(pid, libc::SIGSTOP) }, 0);
        Self(child)
    }
}
impl Drop for HeldChild {
    fn drop(&mut self) {
        if let Ok(Some(now)) = process_observation::process_identity(self.0.pid) {
            if !now.zombie
                && (now.pid, now.uid, now.birth, now.session)
                    == (self.0.pid, self.0.uid, self.0.birth, self.0.session)
            {
                // SAFETY: only the same verified original child may be resumed.
                unsafe { libc::kill(now.pid, libc::SIGCONT) };
            }
        }
    }
}

fn paint(app: &mut App) {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::forms::draw(frame, app, frame.area()))
        .unwrap();
}
fn confirmed_form(s: &mut Setup) -> (App, Arc<ScopedReviewDraftV2>) {
    let (mut app, _) = owner(s);
    let draft = open(s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
    let mut form = ui_v2::form_from_draft(
        draft.clone(),
        None,
        ReviewRequestedActionV2::CreatePending,
        None,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    form.paste("pending summary");
    app.form = Some(form);
    app.overlay = Some(Overlay::Form);
    paint(&mut app);
    let request = ui_v2::request(app.form.as_ref().unwrap()).unwrap();
    let preview = ScopedReviewPreviewV2::prepare(
        request.request().clone(),
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(ui_v2::accept_preview(
        app.form.as_mut().unwrap(),
        &request,
        preview
    ));
    paint(&mut app);
    assert!(ui_v2::confirmed_preview(app.form.as_ref().unwrap()).is_ok());
    (app, draft)
}
fn held_confirm_steps(h: &history::GitImagesV1) -> Vec<GhStep> {
    let actual = review(h, "PENDING", "pending summary", 42);
    let mut steps = opening_steps(h);
    steps.extend(guard_steps(h));
    steps.push(catalogue_step(json!([])));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), current_parent(h)));
    steps.extend(closing_steps(h));
    steps.push(write(
        &format!("repos/{REPO}/pulls/7/reviews"),
        "POST",
        Some(json!({"body":"pending summary","commit_id":h.h,"comments":[]})),
        actual.clone(),
    ));
    let mut held = get(&format!("repos/{REPO}/pulls/7/reviews/810"), actual);
    held.delay_ms = 5_000;
    steps.push(held);
    steps
}
fn wait_calls(s: &Setup, count: usize) {
    let until = Instant::now() + Duration::from_secs(8);
    while commands(&s.fixture).len() < count {
        assert!(
            Instant::now() < until,
            "actual original child never started"
        );
        std::thread::yield_now();
    }
}
fn replacement(
    bg: &mut Background,
    app: &mut App,
    s: &Setup,
) -> (mpsc::Receiver<()>, mpsc::SyncSender<()>) {
    let source = app.remote_scope_source.clone().unwrap();
    let active = app.remote_scope.clone().unwrap();
    let nav = app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .clone()
        .unwrap();
    let provider = provider(&s.fixture);
    let (started, observed) = mpsc::sync_channel(1);
    let (release, wait) = mpsc::sync_channel(1);
    assert!(
        bg.submit_scoped_pr_tab(app, active, source, nav, move |cancel| {
            started.send(()).unwrap();
            wait.recv_timeout(Duration::from_secs(8))
                .map_err(|_| "replacement not released".to_owned())?;
            if cancel.is_cancelled() {
                return Err("replacement canceled incorrectly".into());
            }
            let identity = provider
                .identity(&cancel)
                .map_err(|_| "actual replacement identity observation refused".to_owned())?;
            Ok(crate::runtime::PreparedView {
                status: "Fresh original-nav successor observed actual numeric UID42".into(),
                verified_identity: Some(identity),
                ..Default::default()
            })
        })
    );
    (observed, release)
}

#[test]
fn pending_reviews_v2_lifecycle_expired_or_canceled_original_work_cannot_cancel_or_retag_live_inbox(
) {
    for canceled in [false, true] {
        let mut s = inline_setup(|_, _| {});
        let (mut app, _) = owner(&mut s);
        app.overlay = None;
        native::open(&mut app, false);
        let mut original = app.scoped_reviews_work_v2.take().unwrap();
        assert!(original.matches(&app));
        native::expire_original_work_for_test(&mut original, canceled);
        assert!(original.expired());
        let mut bg = Background::new();
        let writer = DraftWriter::new(database(&s.fixture));
        let (arrived, started) = mpsc::sync_channel(1);
        let (release, wait) = mpsc::sync_channel(1);
        assert!(bg.submit(&mut app, View::Inbox, move |token| {
            arrived.send(token.clone()).unwrap();
            wait.recv_timeout(Duration::from_secs(8))
                .map_err(|_| "live slot not released".to_owned())?;
            if token.is_cancelled() {
                return Err("expired original canceled newer live slot".into());
            }
            Ok(Default::default())
        }));
        let live = started.recv_timeout(Duration::from_secs(5)).unwrap();
        let generation = channel::generation(&bg);
        let before = commands(&s.fixture).len();
        let sql_before = sql_state(&s.fixture);
        app.status = "INDEPENDENT LIVE INBOX STATUS".into();
        assert!(app.loading[View::Inbox.index()]);
        assert!(!bg.submit_scoped_reviews_v2(
            &mut app,
            original,
            provider(&s.fixture),
            database(&s.fixture),
            writer.client()
        ));
        assert!(!live.is_cancelled());
        assert_eq!(channel::generation(&bg), generation);
        assert!(app.loading[View::Inbox.index()]);
        assert_eq!(app.status, "INDEPENDENT LIVE INBOX STATUS");
        assert!(app.scoped_reviews_owner_v2.is_none());
        assert_eq!(commands(&s.fixture).len(), before);
        assert_eq!(sql_state(&s.fixture), sql_before);
        release.send(()).unwrap();
        complete(&mut bg, &mut app);
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_lifecycle_actual_post_held_readback_orphan_preserves_replacement_slot_and_newer_presentations(
) {
    //0 exact original,1 SAME Source with genuinely reissued navigation/images,
    //2 independently produced Source,3 SAME Source/Nav but newer Form ticket.
    for successor in 0..4 {
        let mut s = inline_setup(|h, steps| {
            steps.extend(open_steps(h, json!([])));
            steps.extend(open_steps(h, json!([])));
            if successor == 1 {
                steps.extend(image_steps(h));
            }
            steps.extend(held_confirm_steps(h));
            steps.extend(principal(42, "actual-successor-account"));
        });
        let (mut app, draft) = confirmed_form(&mut s);
        let fresh_images = if successor == 1 {
            // Finish this genuine read BEFORE the write claims protection:
            // pending publication correctly blocks a subsequent private read.
            // Its later presentation is not renewed access or write approval.
            let original_files = app
                .github
                .as_ref()
                .unwrap()
                .scoped_pr_resource
                .clone()
                .unwrap();
            assert!(Arc::ptr_eq(
                &original_files,
                s.workspace.scoped_pr_resource.as_ref().unwrap()
            ));
            Some((
                original_files,
                read(&mut s, &Default::default(), &Default::default()).unwrap(),
            ))
        } else {
            None
        };
        let mut independent = if successor == 2 {
            let mut other = inline_setup(|_, _| {});
            let (other_app, _) = owner(&mut other);
            Some((other, other_app))
        } else {
            None
        };
        let mut newer_form = if successor == 3 {
            let mut form = ui_v2::form_from_draft(
                draft.clone(),
                None,
                ReviewRequestedActionV2::CreatePending,
                None,
                &Default::default(),
                Instant::now() + Duration::from_secs(30),
            )
            .unwrap();
            form.paste("NEWER DIFFERENT PRESENTATION BODY");
            Some(form)
        } else {
            None
        };
        app.form
            .as_mut()
            .unwrap()
            .dispatch(&crate::forms::FormAction::Submit);
        native::capture_form(&mut app);
        let work = app.scoped_reviews_work_v2.take().unwrap();
        assert_eq!(work.phase(), native::ScopedReviewsPhaseV2::Confirm);
        let writer = DraftWriter::new(database(&s.fixture));
        let mut bg = Background::new();
        assert!(bg.submit_scoped_reviews_v2(
            &mut app,
            work,
            provider(&s.fixture),
            database(&s.fixture),
            writer.client()
        ));
        let old = app.scoped_reviews_owner_v2.clone().unwrap();
        let held_count = s
            .steps
            .iter()
            .position(|step| {
                step.delay_ms == 5_000
                    && step.args.get(1) == Some(&format!("repos/{REPO}/pulls/7/reviews/810"))
            })
            .unwrap()
            + 1;
        wait_calls(&s, held_count);
        let calls = commands(&s.fixture);
        assert_eq!(calls[held_count - 2]["args"][5], "POST");
        assert_eq!(
            calls[held_count - 1]["args"][1],
            format!("repos/{REPO}/pulls/7/reviews/810")
        );
        assert_eq!(calls[held_count - 1]["matched"], true);
        assert!(!old.cancellation().is_cancelled());
        let child =
            HeldChild::stop(i32::try_from(calls[held_count - 1]["pid"].as_u64().unwrap()).unwrap());
        let body = Cache::open(&database(&s.fixture))
            .unwrap()
            .draft(draft.intent().scope(), draft.key())
            .unwrap()
            .unwrap();
        let encoded: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(encoded["values"]["summary"], "pending summary");
        assert!(encoded["publication"].is_object());
        assert_eq!(
            sql(&s.fixture)
                .query_row("SELECT state FROM pr_write_attempts", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "pending"
        );
        if let Some((original_files, result)) = fresh_images {
            // Enter the actual pending consumer shape for the SAME original
            // Files/Nav allocation before delivering the precompleted result.
            let github = app.github.as_mut().unwrap();
            let view = github.scoped_pr_resource.as_ref().unwrap();
            assert!(Arc::ptr_eq(view, &original_files));
            assert!(Arc::ptr_eq(
                github.scoped_pr_navigation.as_ref().unwrap(),
                &result.requested_navigation
            ));
            let row = (0..github.pr.as_ref().unwrap().rows.len())
                .find(|row| matches!(view.row(*row), Some(PrResourceRowV1::FileList(0))))
                .unwrap();
            drop(images::take_image_input(github, row).unwrap());
            let (viewer, display_limited) = images::install_images(&mut app, result).unwrap();
            assert!(!display_limited);
            app.pr_file_images = viewer;
        }
        if let Some((_, other)) = &mut independent {
            app.github = other.github.take();
            app.remote_scope_source = other.remote_scope_source.take();
            app.remote_scope = other.remote_scope.take();
            app.pr_file_images = other.pr_file_images.take();
        }
        if let Some(form) = newer_form.take() {
            app.form = Some(form);
            app.overlay = Some(Overlay::Form);
            paint(&mut app);
        }
        let current_source = app.remote_scope_source.clone().unwrap();
        let current_scope = app.remote_scope.clone().unwrap();
        let current_nav = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .clone()
            .unwrap();
        let current_files = app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .clone()
            .unwrap();
        let current_request = if successor == 3 {
            Some(ui_v2::request(app.form.as_ref().unwrap()).unwrap())
        } else {
            None
        };
        let (started, release) = replacement(&mut bg, &mut app, &s);
        let generation = channel::generation(&bg);
        app.status = "NEWER ORIGINAL-NAV OWNER STATUS".into();
        let loading = app.loading;
        bg.retire_stale_scoped_reviews_v2(&mut app);
        assert_eq!(channel::generation(&bg), generation);
        assert_eq!(app.loading, loading);
        assert_eq!(app.status, "NEWER ORIGINAL-NAV OWNER STATUS");
        assert!(app.scoped_reviews_owner_v2.is_none());
        assert!(old.cancellation().is_cancelled());
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &current_source
        ));
        assert!(Arc::ptr_eq(
            app.remote_scope.as_ref().unwrap(),
            &current_scope
        ));
        assert!(Arc::ptr_eq(
            app.github
                .as_ref()
                .unwrap()
                .scoped_pr_navigation
                .as_ref()
                .unwrap(),
            &current_nav
        ));
        if successor == 0 {
            assert!(app.github.as_ref().unwrap().pr.is_none());
            assert!(app.github.as_ref().unwrap().scoped_pr_resource.is_none());
            assert!(app.pr_file_images.is_none());
            assert!(app.form.is_none());
        } else {
            assert!(app.github.as_ref().unwrap().pr.is_some());
            assert!(Arc::ptr_eq(
                app.github
                    .as_ref()
                    .unwrap()
                    .scoped_pr_resource
                    .as_ref()
                    .unwrap(),
                &current_files
            ));
            assert!(app.pr_file_images.as_ref().unwrap().owns(&app));
            if let Some(request) = current_request {
                assert!(request.matches_form(app.form.as_ref().unwrap()));
                assert_eq!(
                    app.form.as_ref().unwrap().fields[0].editor.text,
                    "NEWER DIFFERENT PRESENTATION BODY"
                );
            } else {
                assert!(app.form.is_none());
            }
        }
        started.recv_timeout(Duration::from_secs(8)).unwrap();
        release.send(()).unwrap();
        complete(&mut bg, &mut app);
        assert_eq!(
            app.status,
            "Fresh original-nav successor observed actual numeric UID42"
        );
        assert_eq!(
            app.github.as_ref().unwrap().identity.login,
            "actual-successor-account"
        );
        assert!(channel::generation(&bg).is_none());
        let reap_deadline = Instant::now() + Duration::from_secs(8);
        while process_observation::process_identity(child.0.pid)
            .unwrap()
            .is_some()
        {
            assert!(
                Instant::now() < reap_deadline,
                "same original held child was not reaped"
            );
            std::thread::yield_now();
        }
        let until = Instant::now() + Duration::from_secs(8);
        while sql(&s.fixture)
            .query_row("SELECT state FROM pr_write_attempts", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap()
            != "uncertain"
        {
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
        assert_eq!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .draft(draft.intent().scope(), draft.key())
                .unwrap()
                .as_deref(),
            Some(body.as_str())
        );
        assert_eq!(
            commands(&s.fixture)
                .iter()
                .filter(|call| call["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg == "POST"))
                .count(),
            1
        );
        assert_eq!(commands(&s.fixture).len(), s.steps.len());
        assert_eq!(git_state_resource(&s.fixture), s.git_before);
        assert!(commands(&s.fixture)
            .iter()
            .all(|call| call["matched"] == true));
        if let Some((fresh, _)) = independent {
            unchanged_git_and_exact_script(&fresh);
        }
    }
}
