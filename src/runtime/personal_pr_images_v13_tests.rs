//! Actual existing Native Inbox Request/Completion producer for Personal reads.
//! Source-only declarations; no direct prepared/result constructors.
use super::*;
use crate::{
    app::Action,
    github_workspace::GithubAction,
    scoped_pr_native_diff_native_v1 as native_diff,
    scoped_pr_observed_tabs_v1::resources::{
        immutable_images::personal_v13, PrResourceUiActionV1 as Loaded,
    },
};

fn queue(s: &mut f::Setup, background: &mut Background, native: bool) {
    native_diff::open_personal_images(&mut s.app, native);
    let work = s
        .app
        .pr_native_diff_work_v1
        .take()
        .expect("actual input capture");
    assert!(s.app.remote_scope_source.is_none());
    assert_eq!(work.host(), s.app.github.as_ref().unwrap().identity.host);
    assert!(background.submit_pr_native_diff_v1(
        &mut s.app,
        work,
        s.api.clone(),
        f::database(&s.fixture)
    ));
}
fn native_completion(background: &mut Background) -> Completion {
    let completion = actual(background);
    assert!(completion
        .remote_scope_owner
        .as_ref()
        .is_some_and(|r| r.pr_native_diff_v1.is_some()));
    completion
}
fn body(s: &f::Setup) -> *const u8 {
    s.app
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
        .as_ptr()
}
#[test]
fn personal_pr_entry_v13_images_runtime_actual_channel_publishes_same_image_and_native_engine() {
    for native in [false, true] {
        let (mut s, h) = f::images::prepared(false, |_| {});
        let original_body = body(&s);
        let mut background = Background::new();
        queue(&mut s, &mut background, native);
        let completion = native_completion(&mut background);
        put(&background, completion);
        assert!(background.poll(&mut s.app));
        let proof = s.app.pr_file_images.as_ref().unwrap().original_proof_v1();
        assert!(proof.owns(&s.app));
        assert_eq!(proof.observed().merge_base(), h.m);
        if native {
            let state = s.app.pr_native_diff_v1.as_ref().unwrap();
            assert!(state.owns(&s.app));
            assert!(Arc::ptr_eq(state.source().unwrap().proof(), proof));
            assert!(s.app.overlay.is_none());
        } else {
            assert_eq!(s.app.overlay, Some(crate::app::Overlay::PrFileImages));
        }
        assert_eq!(body(&s), original_body);
        assert!(!background.current.contains_key(&View::Inbox.index()));
        assert!(!s.app.loading[View::Inbox.index()]);
        f::exact(&s, s.steps.len());
    }
}
#[test]
fn personal_pr_entry_v13_images_runtime_mixed_and_canceled_actual_completions_never_publish_private_state(
) {
    for mixed in [false, true] {
        let (mut s, _) = f::images::prepared(false, |_| {});
        let mut background = Background::new();
        queue(&mut s, &mut background, true);
        let mut completion = native_completion(&mut background);
        let original_body = body(&s);
        if mixed {
            let prepared = completion.result.as_mut().unwrap();
            prepared.clear_private = true;
            prepared.status = "FOREIGN_MIXED_PRIVATE_STATUS".into();
        } else {
            s.app.dispatch(Action::Cancel);
            assert!(s.app.pr_native_diff_cancel_v1);
            s.app.pr_native_diff_cancel_v1 = false;
            assert!(background.cancel_pr_native_diff_v1(&mut s.app));
        }
        let status = s.app.status.clone();
        let before = background.current.get(&View::Inbox.index()).cloned();
        assert_eq!(
            background.poll_pr_native_diff_v1(&mut s.app, &mut completion),
            Some(false)
        );
        assert_eq!(s.app.status, status);
        assert_eq!(body(&s), original_body);
        assert!(s.app.pr_native_diff_v1.is_none());
        assert!(s.app.pr_file_images.is_none());
        assert_eq!(
            background.current.get(&View::Inbox.index()).map(|v| v.0),
            before.as_ref().map(|v| v.0)
        );
        if let Some((_, token)) = before {
            assert!(background
                .current
                .get(&View::Inbox.index())
                .unwrap()
                .1
                .same_instance(&token));
            assert!(!token.is_cancelled());
        }
        background.cancel_pr_native_diff_v1(&mut s.app);
        f::exact(&s, s.steps.len());
    }
}
#[test]
fn personal_pr_entry_v13_images_runtime_newer_same_source_navigation_and_foreign_slot_are_preserved(
) {
    let (mut s, _) = f::images::prepared(false, |steps| steps.extend(steps.clone()));
    let mut background = Background::new();
    queue(&mut s, &mut background, true);
    let mut old = native_completion(&mut background);
    let input =
        personal_v13::PersonalImageInputV13::capture(&s.app, s.app.inbox_list.selected, None)
            .err()
            .expect("pending owner/loading prevents a second input");
    assert!(input.contains("Wait/cancel"));
    // Capture a genuinely new read only after the exact old slot drains. The
    // retained old Completion must not displace that source's newer Nav.
    assert!(background.cancel_pr_native_diff_v1(&mut s.app));
    let input =
        personal_v13::PersonalImageInputV13::capture(&s.app, s.app.inbox_list.selected, None)
            .unwrap();
    let result = personal_v13::read(
        input.clone(),
        &s.api,
        &f::database(&s.fixture),
        &Default::default(),
        &Default::default(),
        input.cancellation(),
    )
    .unwrap();
    s.app.pr_file_images = personal_v13::install(&mut s.app, result).unwrap();
    let proof = s
        .app
        .pr_file_images
        .as_ref()
        .unwrap()
        .original_proof_v1()
        .clone();
    assert!(proof.owns(&s.app));
    let nav = s
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .clone()
        .unwrap();
    assert!(background.submit(&mut s.app, View::Inbox, |_| Ok(PreparedView::default())));
    let successor = background
        .current
        .get(&View::Inbox.index())
        .unwrap()
        .clone();
    let status = s.app.status.clone();
    let original_body = body(&s);
    assert_eq!(
        background.poll_pr_native_diff_v1(&mut s.app, &mut old),
        Some(false)
    );
    background.retire_stale_pr_native_diff_v1(&mut s.app);
    assert!(Arc::ptr_eq(
        s.app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap(),
        &nav
    ));
    assert!(Arc::ptr_eq(
        s.app.pr_file_images.as_ref().unwrap().original_proof_v1(),
        &proof
    ));
    assert_eq!(body(&s), original_body);
    assert_eq!(s.app.status, status);
    let now = background.current.get(&View::Inbox.index()).unwrap();
    assert_eq!(now.0, successor.0);
    assert!(now.1.same_instance(&successor.1));
    assert!(!successor.1.is_cancelled());
    assert!(s.app.loading[View::Inbox.index()]);
    f::exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_images_runtime_expired_inherited_issuer_refuses_before_foreign_slot_status_or_epoch_effect(
) {
    for foreign in [false, true] {
        let (mut s, _) = f::images::prepared(false, |steps| steps.clear());
        native_diff::open_personal_images(&mut s.app, true);
        let work = s.app.pr_native_diff_work_v1.take().unwrap();
        let mut background = Background::new();
        if foreign {
            assert!(background.submit(&mut s.app, View::Inbox, |_| Ok(PreparedView::default())));
        }
        let before = background.current.get(&View::Inbox.index()).cloned();
        let epoch = background.generation;
        let status = s.app.status.clone();
        let original_body = body(&s);
        let provider = s
            .api
            .clone()
            .with_scoped_read_window(std::time::Instant::now() - Duration::from_millis(1), 4096)
            .unwrap();
        assert!(!background.submit_pr_native_diff_v1(
            &mut s.app,
            work,
            provider,
            f::database(&s.fixture)
        ));
        if let Some(before) = before {
            let after = background.current.get(&View::Inbox.index()).unwrap();
            assert_eq!(after.0, before.0);
            assert!(after.1.same_instance(&before.1));
            assert!(!before.1.is_cancelled());
        } else {
            assert!(!background.current.contains_key(&View::Inbox.index()));
        }
        assert_eq!(background.generation, epoch);
        assert_eq!(s.app.status, status);
        assert_eq!(s.app.loading[View::Inbox.index()], foreign);
        assert!(s.app.pr_native_diff_owner_v1.is_none());
        assert_eq!(body(&s), original_body);
        f::exact(&s, s.steps.len());
    }
}
#[test]
fn personal_pr_entry_v13_images_runtime_full_disconnected_shutdown_queue_preserves_actual_page_and_epoch(
) {
    for unavailable in 0..3 {
        let (mut s, _) = f::images::prepared(false, |steps| steps.clear());
        native_diff::open_personal_images(&mut s.app, true);
        let work = s.app.pr_native_diff_work_v1.take().unwrap();
        let original_body = body(&s);
        let rows = s
            .app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .rows
            .as_ptr();
        let mut background = Background::new();
        let epoch = background.generation;
        let status = s.app.status.clone();
        let (sender, receiver) = mpsc::sync_channel(0);
        background.requests = if unavailable == 2 { None } else { Some(sender) };
        let held = if unavailable == 1 {
            drop(receiver);
            None
        } else {
            Some(receiver)
        };
        assert!(!background.submit_pr_native_diff_v1(
            &mut s.app,
            work,
            s.api.clone(),
            f::database(&s.fixture)
        ));
        assert_eq!(background.generation, epoch);
        assert_eq!(s.app.status, status);
        assert!(background.current.is_empty());
        assert!(background.current_pr_native_diff_v1.is_none());
        assert!(s.app.pr_native_diff_owner_v1.is_none());
        assert!(!s.app.loading[View::Inbox.index()]);
        assert_eq!(body(&s), original_body);
        assert_eq!(
            s.app
                .github
                .as_ref()
                .unwrap()
                .pr
                .as_ref()
                .unwrap()
                .rows
                .as_ptr(),
            rows
        );
        f::exact(&s, s.steps.len());
        drop(held);
    }
}
#[test]
fn personal_pr_entry_v13_images_runtime_caught_actual_worker_panic_retains_original_page() {
    let (mut s, _) = f::images::prepared(false, |steps| steps.clear());
    let original_body = body(&s);
    native_diff::open_personal_images(&mut s.app, true);
    let actual_work = s.app.pr_native_diff_work_v1.take().unwrap();
    let work = native_diff::actual_panic_work(actual_work);
    let mut background = Background::new();
    assert!(background.submit_pr_native_diff_v1(
        &mut s.app,
        work,
        s.api.clone(),
        f::database(&s.fixture)
    ));
    let completion = native_completion(&mut background);
    assert_eq!(
        completion.result.as_ref().err().map(String::as_str),
        Some("Background task failed unexpectedly; refresh to retry")
    );
    put(&background, completion);
    assert!(background.poll(&mut s.app));
    assert_eq!(body(&s), original_body);
    assert!(s.app.personal_pr_source_v13.as_ref().unwrap().owns(&s.app));
    assert!(s.app.pr_file_images.is_none());
    assert!(s.app.pr_native_diff_v1.is_none());
    assert!(!s.app.loading[View::Inbox.index()]);
    f::exact(&s, s.steps.len());
}
#[test]
fn personal_pr_entry_v13_loaded_controls_use_actual_native_channel_without_new_reads_or_body_moves()
{
    let mut s = f::setup(f::phase());
    let owner = f::owner(&mut s);
    let prepared = native::execute(owner.clone(), &s.api).unwrap();
    assert!(native::publish(&mut s.app, &owner, prepared));
    let original_body = body(&s);
    let mut background = Background::new();
    for action in [
        Loaded::NextFile,
        Loaded::NextHunk,
        Loaded::Layout,
        Loaded::PatchContext,
    ] {
        let old_view = s
            .app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .clone()
            .unwrap();
        s.app
            .dispatch(Action::Github(GithubAction::Resource(action)));
        let work = s.app.pr_native_diff_work_v1.take().unwrap();
        assert!(background.submit_pr_native_diff_v1(
            &mut s.app,
            work,
            s.api.clone(),
            f::database(&s.fixture)
        ));
        let completion = native_completion(&mut background);
        assert!(completion.result.is_ok());
        put(&background, completion);
        assert!(background.poll(&mut s.app));
        let new_view = s
            .app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_resource
            .as_ref()
            .unwrap();
        assert!(std::ptr::eq(old_view.observed(), new_view.observed()));
        if action == Loaded::Layout {
            assert!(!Arc::ptr_eq(&old_view, new_view));
            assert_eq!(new_view.layout, crate::repository_view::DiffLayout::Split);
        }
        if action == Loaded::PatchContext {
            assert!(s.app.notice.contains("Loaded patch context only"));
            assert!(s.app.notice.contains("old"));
            s.app.overlay = None;
            s.app.notice.clear();
        }
        assert_eq!(body(&s), original_body);
        f::exact(&s, s.steps.len());
    }
    s.app.dispatch(Action::Github(GithubAction::Back));
    assert!(s.app.personal_pr_source_v13.is_none());
    assert_eq!(body(&s), original_body);
    assert!(s.app.github.as_ref().unwrap().scoped_pr_resource.is_none());
    f::exact(&s, s.steps.len());
}
