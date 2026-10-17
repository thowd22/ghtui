//! Genuine original source producers, actual Background queue and channel.
#[path = "scoped_pr_inline_common_v14_tests.rs"]
mod common_v14;
use super::*;
use crate::{
    app::{Overlay, View},
    runtime::{scoped_inline_test_support_v1 as channel, Background},
};
use images::{
    inline_v1::{native_v1 as native, ui_v1 as ui},
    ui::PrImageUiActionV1,
};
fn app(s: &Setup) -> App {
    App {
        view: View::Inbox,
        remote_scope_source: Some(s.source.clone()),
        remote_scope: Some(s.active.clone()),
        github: Some(s.workspace.clone()),
        ..Default::default()
    }
}
fn original_open(s: &mut Setup) -> (App, Arc<native::ScopedInlineWorkV1>) {
    let closed = read(s, &Default::default(), &Default::default()).unwrap();
    let mut app = app(s);
    let (mut viewer, limited) = images::install_images(&mut app, closed).unwrap();
    assert!(!limited);
    let mut viewer = viewer.take().unwrap();
    viewer.apply(PrImageUiActionV1::Old);
    viewer.apply(PrImageUiActionV1::Move {
        delta: 1,
        extend: true,
    });
    viewer.apply(PrImageUiActionV1::Range);
    app.overlay = Some(Overlay::PrFileImages);
    app.pr_file_images = Some(viewer);
    native::open(&mut app);
    let work = app.scoped_inline_work_v1.take().unwrap();
    assert!(work.matches(&app));
    (app, work)
}
fn submit(
    s: &Setup,
    app: &mut App,
    bg: &mut Background,
    work: Arc<native::ScopedInlineWorkV1>,
    writer: &DraftWriter,
) {
    assert!(bg.submit_scoped_inline_v1(
        app,
        work,
        provider(&s.fixture),
        database(&s.fixture),
        writer.client()
    ));
}
fn completed(bg: &mut Background, app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !bg.poll(app) {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ReapProbeV308 {
    Gone,
    StillOwned,
    #[cfg(target_os = "linux")]
    Unresolved,
}

fn reap_probe_v308(
    observed: std::io::Result<Option<original_process_identity::ProcessIdentity>>,
    original: &original_process_identity::ProcessIdentity,
) -> std::io::Result<ReapProbeV308> {
    match observed {
        Ok(None) => Ok(ReapProbeV308::Gone),
        Ok(Some(now))
            if (now.pid, now.uid, now.birth, now.session)
                == (original.pid, original.uid, original.birth, original.session) =>
        {
            // A live or zombie original is still present; neither is cleanup.
            Ok(ReapProbeV308::StillOwned)
        }
        Ok(Some(_)) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Cleanup observed a foreign process identity",
        )),
        #[cfg(target_os = "linux")]
        Err(error) if error.kind() == std::io::ErrorKind::InvalidData => {
            // Linux can retire ownership fields between strict /proc reads.
            // UNKNOWN is not absence or permission to signal/reap anything.
            // The original bounded poll must still reach a genuine None.
            Ok(ReapProbeV308::Unresolved)
        }
        Err(error) => Err(error),
    }
}

#[test]
fn inline_comments_runtime_reap_probe_requires_kernel_absence_not_invalid_zombie_or_foreign() {
    let original = original_process_identity::process_identity(std::process::id() as i32)
        .unwrap()
        .unwrap();
    assert_eq!(
        reap_probe_v308(Ok(Some(original.clone())), &original).unwrap(),
        ReapProbeV308::StillOwned
    );
    let mut zombie = original.clone();
    zombie.zombie = true;
    assert_eq!(
        reap_probe_v308(Ok(Some(zombie)), &original).unwrap(),
        ReapProbeV308::StillOwned
    );
    for field in 0..4 {
        let mut foreign = original.clone();
        match field {
            0 => foreign.pid += 1,
            1 => foreign.uid += 1,
            2 => foreign.birth.0 += 1,
            _ => foreign.session += 1,
        }
        assert!(reap_probe_v308(Ok(Some(foreign)), &original).is_err());
    }
    #[cfg(target_os = "linux")]
    assert_eq!(
        reap_probe_v308(
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "invalid stat ownership fields"
            )),
            &original,
        )
        .unwrap(),
        ReapProbeV308::Unresolved
    );
    let denied = reap_probe_v308(
        Err(std::io::Error::from_raw_os_error(libc::EACCES)),
        &original,
    )
    .unwrap_err();
    assert_eq!(denied.raw_os_error(), Some(libc::EACCES));
    assert_eq!(
        reap_probe_v308(Ok(None), &original).unwrap(),
        ReapProbeV308::Gone
    );
}

#[test]
fn inline_comments_runtime_actual_queue_publishes_only_original_closed_draft_form() {
    let mut s = inline_setup(|h, steps| steps.extend(source_fence_steps(h, true)));
    let (mut app, work) = original_open(&mut s);
    let mut bg = Background::new();
    let writer = DraftWriter::new(database(&s.fixture));
    submit(&s, &mut app, &mut bg, work, &writer);
    let owner = app.scoped_inline_owner_v1.clone().unwrap();
    assert!(!owner.expired());
    assert!(owner.deadline().duration_since(Instant::now()) <= Duration::from_secs(30));
    completed(&mut bg, &mut app);
    assert!(channel::generation(&bg).is_none());
    assert!(app.scoped_inline_owner_v1.is_none());
    assert!(!app.loading[View::Inbox.index()]);
    assert_eq!(app.overlay, Some(Overlay::Form));
    let form = app.form.as_ref().unwrap();
    assert!(matches!(
        &form.purpose,
        crate::forms::FormPurpose::PrInline(_)
    ));
    assert!(!form.plan_ready());
    assert!(form.fields[0].editor.text.is_empty());
    assert!(app.github.is_some());
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_runtime_mixed_result_or_missing_original_owner_cannot_remove_slot_or_publish() {
    for mode in [0, 1, 5, 6] {
        let mut s = inline_setup(|h, steps| steps.extend(source_fence_steps(h, true)));
        let (mut app, work) = original_open(&mut s);
        let mut bg = Background::new();
        let writer = DraftWriter::new(database(&s.fixture));
        submit(&s, &mut app, &mut bg, work, &writer);
        let generation = channel::generation(&bg);
        let owner = app.scoped_inline_owner_v1.clone().unwrap();
        app.status = "EXACT_CURRENT_STATUS".into();
        channel::fault_next(&mut bg, mode);
        assert!(!bg.poll(&mut app));
        assert_eq!(channel::generation(&bg), generation);
        assert!(Arc::ptr_eq(
            app.scoped_inline_owner_v1.as_ref().unwrap(),
            &owner
        ));
        assert!(app.loading[View::Inbox.index()]);
        assert_eq!(app.status, "EXACT_CURRENT_STATUS");
        assert!(app.github.is_some() && app.pr_file_images.is_some());
        assert!(app.form.is_none());
        assert!(bg.cancel_scoped_inline_v1(&mut app));
        assert!(owner.cancellation().is_cancelled());
        assert!(app.github.is_none());
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn inline_comments_runtime_stale_preview_releases_only_exact_original_presentation_busy_flag() {
    for newer in [false, true] {
        let mut s = inline_setup(|h, steps| {
            steps.extend(source_fence_steps(h, true));
            if newer {
                steps.extend(source_fence_steps(h, true));
            }
            let mut held = image_steps(h);
            held[0].delay_ms = 6_000;
            held.truncate(1);
            steps.extend(held)
        });
        let mut form = super::inline_ui::form(&mut s);
        let original = ui::request(&form).unwrap();
        let replacement = if newer {
            Some(
                ui::form_from_draft(
                    ScopedInlineDraftV1::open(
                        original.intent().clone(),
                        &provider(&s.fixture),
                        &database(&s.fixture),
                        &Default::default(),
                        &Default::default(),
                        &Default::default(),
                        Instant::now() + Duration::from_secs(30),
                    )
                    .unwrap(),
                )
                .unwrap(),
            )
        } else {
            None
        };
        form.preview_requested = true;
        let mut app = app(&s);
        app.overlay = Some(Overlay::Form);
        app.form = Some(form);
        native::capture_form(&mut app);
        let work = app.scoped_inline_work_v1.take().unwrap();
        let mut bg = Background::new();
        let writer = DraftWriter::new(database(&s.fixture));
        submit(&s, &mut app, &mut bg, work, &writer);
        let owner = app.scoped_inline_owner_v1.clone().unwrap();
        assert!(app.form.as_ref().unwrap().submitting);
        let pid = wait_for_original_call(&s.fixture, s.steps.len() - 1);
        let held = original_process_identity::process_identity(pid)
            .unwrap()
            .unwrap();
        assert!(!held.zombie);
        if let Some(mut replacement) = replacement {
            replacement.submitting = true;
            assert!(!original.matches_form(&replacement));
            app.form = Some(replacement);
        } else {
            ui::replace_editor_result(app.form.as_mut().unwrap(), "Out-of-band newer bounded body")
                .unwrap();
        }
        bg.retire_stale_scoped_inline_v1(&mut app);
        assert!(owner.cancellation().is_cancelled());
        assert!(app.scoped_inline_owner_v1.is_none());
        assert!(channel::generation(&bg).is_none());
        assert_eq!(
            app.form.as_ref().unwrap().submitting,
            newer,
            "Never clear another genuine open presentation's busy flag"
        );
        assert_eq!(
            app.form.as_ref().unwrap().fields[0].editor.text,
            if newer {
                INLINE_BODY
            } else {
                "Out-of-band newer bounded body"
            }
        );
        assert!(app.github.is_some());
        let deadline = Instant::now() + Duration::from_secs(5);
        while reap_probe_v308(original_process_identity::process_identity(pid), &held).unwrap()
            != ReapProbeV308::Gone
        {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(!bg.poll(&mut app));
        assert_eq!(app.form.as_ref().unwrap().submitting, newer);
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn inline_comments_runtime_queued_new_intent_vetoes_success_error_and_real_worker_panic_before_effects(
) {
    queued_new_intent(0);
}

#[test]
fn inline_comments_runtime_original_stash_intents_veto_success_error_and_real_panic_before_effects()
{
    queued_new_intent(1);
    queued_new_intent(2);
}

#[test]
fn inline_comments_runtime_inflight_navigation_restore_vetoes_success_error_and_real_panic() {
    queued_new_intent(3);
}

fn queued_new_intent(input: u8) {
    for mode in [4, 2, 3] {
        let mut s = inline_setup(|h, steps| {
            if mode != 3 {
                steps.extend(source_fence_steps(h, true))
            }
        });
        let (mut app, mut work) = original_open(&mut s);
        if mode == 3 {
            work = native::actual_worker_panic_for_test(work)
        }
        let mut bg = Background::new();
        let writer = DraftWriter::new(database(&s.fixture));
        submit(&s, &mut app, &mut bg, work, &writer);
        let owner = app.scoped_inline_owner_v1.clone().unwrap();
        let generation = channel::generation(&bg);
        channel::fault_next(&mut bg, mode);
        let mut stash = crate::repository_stash_recovery_v1::tests::Real::new();
        match input {
            0 => app.repository_load_requested = true,
            1 => {
                crate::repository_stash_recovery_v1::dispatch(
                    &mut stash.app,
                    crate::repository_stash_recovery_v1::StashRecoveryActionV1::Inspect,
                );
                app.original_stash_recovery_request_v1 =
                    stash.app.original_stash_recovery_request_v1.take();
                assert!(app.original_stash_recovery_request_v1.is_some());
            }
            2 => app.original_stash_recovery_cancel_requested_v1 = true,
            3 => app.navigation_restore_inflight = true,
            _ => panic!("Unknown original intent"),
        }
        app.status = "NEWER_LOCAL_REQUEST".into();
        assert!(!bg.poll(&mut app));
        assert_eq!(channel::generation(&bg), generation);
        assert!(Arc::ptr_eq(
            app.scoped_inline_owner_v1.as_ref().unwrap(),
            &owner
        ));
        assert!(app.loading[View::Inbox.index()]);
        match input {
            0 => assert!(app.repository_load_requested),
            1 => assert!(app.original_stash_recovery_request_v1.is_some()),
            2 => assert!(app.original_stash_recovery_cancel_requested_v1),
            3 => assert!(app.navigation_restore_inflight),
            _ => unreachable!(),
        }
        assert_eq!(app.status, "NEWER_LOCAL_REQUEST");
        assert!(app.github.is_some() && app.pr_file_images.is_some());
        assert!(app.form.is_none());
        app.repository_load_requested = false;
        app.original_stash_recovery_request_v1 = None;
        app.original_stash_recovery_cancel_requested_v1 = false;
        app.navigation_restore_inflight = false;
        assert!(bg.cancel_scoped_inline_v1(&mut app));
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn inline_comments_runtime_actual_stash_slot_refuses_a_mixed_closed_inline_result() {
    let mut s = inline_setup(|h, steps| steps.extend(source_fence_steps(h, true)));
    let (mut app, work) = original_open(&mut s);
    let mut bg = Background::new();
    let writer = DraftWriter::new(database(&s.fixture));
    submit(&s, &mut app, &mut bg, work, &writer);
    channel::reject_inline_in_original_stash_slot(&mut bg);
    assert!(app.form.is_none());
    assert!(app.github.is_some() && app.pr_file_images.is_some());
    assert!(bg.cancel_scoped_inline_v1(&mut app));
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_runtime_current_error_and_real_worker_panic_hide_original_private_models() {
    for panic in [false, true] {
        let mut s = inline_setup(|h, steps| {
            if !panic {
                steps.extend(source_fence_steps(h, true))
            }
        });
        let (mut app, mut work) = original_open(&mut s);
        if panic {
            work = native::actual_worker_panic_for_test(work)
        }
        let mut bg = Background::new();
        let writer = DraftWriter::new(database(&s.fixture));
        submit(&s, &mut app, &mut bg, work, &writer);
        channel::fault_next(&mut bg, if panic { 3 } else { 2 });
        assert!(bg.poll(&mut app));
        assert!(channel::generation(&bg).is_none());
        assert!(app.github.is_none() && app.pr_file_images.is_none() && app.form.is_none());
        assert!(app.remote_scope_source.is_none());
        assert!(!app.status.contains("PRIVATE_"));
        assert!(app.status.contains("nothing resent"));
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn inline_comments_runtime_old_cancel_cannot_remove_newer_generic_inbox_slot() {
    let mut s = inline_setup(|h, steps| steps.extend(source_fence_steps(h, true)));
    let (mut app, work) = original_open(&mut s);
    let mut bg = Background::new();
    let writer = DraftWriter::new(database(&s.fixture));
    submit(&s, &mut app, &mut bg, work, &writer);
    channel::fault_next(&mut bg, 1);
    let old = app.scoped_inline_owner_v1.clone().unwrap();
    let (send, receive) = std::sync::mpsc::sync_channel(1);
    assert!(bg.submit(&mut app, View::Inbox, move |_| {
        receive.recv().unwrap();
        Ok(crate::runtime::PreparedView {
            status: "FRESH_SLOT".into(),
            ..Default::default()
        })
    }));
    let fresh = channel::generation(&bg);
    assert!(!bg.cancel_scoped_inline_v1(&mut app));
    bg.retire_stale_scoped_inline_v1(&mut app);
    assert_eq!(channel::generation(&bg), fresh);
    assert!(app.loading[View::Inbox.index()]);
    assert!(app.github.is_some() && app.pr_file_images.is_some());
    assert!(old.cancellation().is_cancelled());
    send.send(()).unwrap();
    completed(&mut bg, &mut app);
    assert_eq!(app.status, "FRESH_SLOT");
    unchanged_git_and_exact_script(&s);
}
#[test]
fn inline_comments_runtime_dismissed_confirm_retires_only_same_original_source_after_actual_post() {
    for newer in [false, true] {
        let mut s = inline_setup(|h, steps| {
            steps.extend(source_fence_steps(h, true));
            steps.extend(image_steps(h));
            let mut confirm = confirm_steps(h);
            confirm[12].delay_ms = 6_000;
            confirm.truncate(13);
            steps.extend(confirm)
        });
        let mut form = super::inline_ui::form(&mut s);
        let request = ui::request(&form).unwrap();
        let preview = super::inline_ui::preview(&s, &request, request.body().clone());
        assert!(ui::accept_preview(&mut form, &request, preview));
        form.submit_requested = true;
        let mut app = app(&s);
        app.overlay = Some(Overlay::Form);
        app.form = Some(form);
        native::capture_form(&mut app);
        let work = app.scoped_inline_work_v1.take().unwrap();
        assert_eq!(work.phase(), native::ScopedInlinePhaseV1::Confirm);
        let mut bg = Background::new();
        let writer = DraftWriter::new(database(&s.fixture));
        submit(&s, &mut app, &mut bg, work, &writer);
        let owner = app.scoped_inline_owner_v1.clone().unwrap();
        let pid = wait_for_original_call(&s.fixture, s.steps.len() - 1);
        let held = original_process_identity::process_identity(pid)
            .unwrap()
            .unwrap();
        assert!(!held.zombie);
        assert_eq!(held.uid, unsafe { libc::geteuid() });
        assert_eq!(held.session, unsafe { libc::getsid(0) });
        assert_eq!(
            sql(&s.fixture)
                .query_row("SELECT state FROM pr_write_attempts", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "pending"
        );
        assert_eq!(
            commands(&s.fixture)
                .iter()
                .filter(|row| row["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|arg| arg == "POST"))
                .count(),
            1
        );
        app.form = None;
        app.overlay = None;
        app.form_dismissed = true;
        let other = if newer {
            let other = inline_setup(|_, _| {});
            app.remote_scope_source = Some(other.source.clone());
            app.remote_scope = Some(other.active.clone());
            app.github = Some(other.workspace.clone());
            Some(other)
        } else {
            None
        };
        bg.retire_stale_scoped_inline_v1(&mut app);
        assert!(owner.cancellation().is_cancelled());
        assert!(channel::generation(&bg).is_none());
        assert!(app.scoped_inline_owner_v1.is_none());
        if let Some(other) = &other {
            assert!(Arc::ptr_eq(
                app.remote_scope_source.as_ref().unwrap(),
                &other.source
            ));
            assert!(Arc::ptr_eq(
                app.remote_scope.as_ref().unwrap(),
                &other.active
            ));
            assert!(app.github.is_some());
        } else {
            assert!(
                app.github.is_none()
                    && app.pr_file_images.is_none()
                    && app.remote_scope_source.is_none()
            );
            assert!(app.status.contains("nothing resent"));
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while reap_probe_v308(original_process_identity::process_identity(pid), &held).unwrap()
            != ReapProbeV308::Gone
        {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        while sql(&s.fixture)
            .query_row("SELECT state FROM pr_write_attempts", [], |row| {
                row.get::<_, String>(0)
            })
            .unwrap()
            != "uncertain"
        {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert!(!bg.poll(&mut app));
        if let Some(other) = &other {
            assert!(Arc::ptr_eq(
                app.remote_scope_source.as_ref().unwrap(),
                &other.source
            ));
            assert!(app.github.is_some());
        } else {
            assert!(app.github.is_none());
        }
        unchanged_git_and_exact_script(&s);
    }
}
