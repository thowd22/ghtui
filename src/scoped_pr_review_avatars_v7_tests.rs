//! SOURCE declarations. Genuine original producer + public cache/widget/SQL/Git
//! evidence; Root alone executes nonzero gates. These are not PTY substitutes.
use super::*;
use crate::{
    identity::{ActorIdentity, AvatarCache, AvatarError, AvatarFetcher, AvatarState},
    providers::gh::Actor,
};
use images::reviews_v2::{
    avatars_v7 as avatars,
    browser_v2::{ReviewBrowserActionV2, ReviewBrowserV2},
    native_v2,
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Mutex,
};
const URL: &str = "https://avatars.githubusercontent.com/u/201?v=4";
#[derive(Default)]
struct Fetcher {
    calls: AtomicUsize,
    hold: AtomicBool,
    released: AtomicBool,
    cancelled: AtomicBool,
    urls: Mutex<Vec<String>>,
}
impl AvatarFetcher for Fetcher {
    fn fetch(&self, url: &str, cancel: &CancellationToken) -> Result<Vec<u8>, AvatarError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.urls.lock().unwrap().push(url.into());
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.hold.load(Ordering::SeqCst) && !self.released.load(Ordering::SeqCst) {
            if cancel.is_cancelled() {
                self.cancelled.store(true, Ordering::SeqCst);
                return Err(AvatarError("Original asset cancelled".into()));
            }
            assert!(
                Instant::now() < deadline,
                "Actual release/cancel, not an unconditional sleeper"
            );
            std::thread::yield_now();
        }
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([255, 0, 0, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        Ok(png.into_inner())
    }
}
fn observed(login: &str, uid: u64, kind: &str) -> Value {
    json!({"id":uid,"login":login,"type":kind,"avatar_url":URL})
}
fn setup(user: Value) -> (Setup, App) {
    setup_users(vec![user])
}
fn setup_users(users: Vec<Value>) -> (Setup, App) {
    let mut s = inline_setup(|h, steps| {
        let rows = users
            .into_iter()
            .enumerate()
            .map(|(index, user)| {
                let mut row = review(h, "PENDING", "Actual author/private body", 43);
                row["user"] = user;
                row["id"] = json!(810 + index);
                row["node_id"] = json!(format!("ACTUAL_REVIEW_{}", 810 + index));
                row
            })
            .collect::<Vec<_>>();
        steps.extend(opening_steps(h));
        for (index, page) in rows.chunks(100).enumerate() {
            steps.push(get(
                &format!(
                    "repos/{REPO}/pulls/7/reviews?per_page=100&page={}",
                    index + 1
                ),
                json!(page),
            ));
        }
        if rows.is_empty() {
            steps.push(catalogue_step(json!([])));
        }
        steps.extend(closing_steps(h));
    });
    let (mut app, _) = owner(&mut s);
    let draft = open(&s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
    app.scoped_reviews_v2 = Some(
        ReviewBrowserV2::prepare(
            draft,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap(),
    );
    app.overlay = Some(crate::app::Overlay::ScopedReviewsV2);
    (s, app)
}
fn cache(s: &Setup, app: &App, fetcher: Arc<Fetcher>) -> Option<(String, AvatarCache)> {
    let scope =
        crate::actor_avatar_states_v1::partition(&app.github.as_ref().unwrap().identity).unwrap();
    Some((
        scope.clone(),
        AvatarCache::new(s.fixture.root.join("review-avatars"), scope, fetcher),
    ))
}
fn draw(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| native_v2::draw(frame, app, frame.area()))
        .unwrap();
    terminal.backend().buffer().clone()
}
fn text(buffer: &ratatui::buffer::Buffer) -> String {
    buffer.content().iter().map(|cell| cell.symbol()).collect()
}
fn wait(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "Actual bounded worker state");
        std::thread::yield_now();
    }
}
fn poll(
    s: &Setup,
    app: &mut App,
    cache: &mut Option<(String, AvatarCache)>,
    lease: &mut Option<avatars::ReviewAvatarLeaseV7>,
) -> bool {
    avatars::poll_review(
        app,
        cache,
        &s.fixture.root.join("review-avatars"),
        true,
        lease,
    )
}
fn metadata(_s: &Setup, app: &App) -> Actor {
    app.scoped_reviews_v2
        .as_ref()
        .unwrap()
        .draft_v7_test()
        .catalogue()
        .rows()[0]
        .author()
        .clone()
}

#[test]
fn pending_reviews_v2_avatars_v7_actual_bot_deleted_unknown_labels_and_numeric_author_never_inherit_requester(
) {
    for (login, kind, label) in [
        ("reviewer-active", "Bot", "[bot]"),
        ("ghost", "deleted", "[deleted]"),
        ("", "", "[unknown]"),
    ] {
        let (s, mut app) = setup(observed(login, 43, kind));
        let calls = commands(&s.fixture).len();
        let sql_before = sql_state(&s.fixture);
        let buffer = draw(&mut app, 120, 40);
        let painted = text(&buffer);
        assert!(painted.contains(label));
        assert!(painted.contains("UID Some(43)"));
        assert!(painted.contains("requester UID Some(42)"));
        assert!(!buffer.content().iter().any(|cell| cell.symbol() == "▀"));
        assert_eq!(commands(&s.fixture).len(), calls);
        assert_eq!(sql_state(&s.fixture), sql_before);
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_avatars_v7_genuine_visible_actor_pixels_one_numeric_cache_and_no_gh_sql_git_effect(
) {
    let (s, mut app) = setup(observed("actual-author", 43, "User"));
    let actor = metadata(&s, &app);
    let identity = ActorIdentity::github(HOST, &actor);
    let fetcher = Arc::new(Fetcher::default());
    let mut cache = cache(&s, &app, fetcher.clone());
    let mut lease = None;
    let sql_before = sql_state(&s.fixture);
    let calls = commands(&s.fixture).len();
    draw(&mut app, 120, 40);
    assert!(poll(&s, &mut app, &mut cache, &mut lease));
    wait(|| {
        poll(&s, &mut app, &mut cache, &mut lease);
        matches!(
            app.avatar_states.get(&identity),
            Some(AvatarState::Ready(_))
        )
    });
    let buffer = draw(&mut app, 120, 40);
    assert!(buffer.content().iter().any(|cell| cell.symbol() == "▀"
        && cell.fg == ratatui::style::Color::Rgb(255, 0, 0)
        && cell.bg == cell.fg));
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    assert_eq!(*fetcher.urls.lock().unwrap(), vec![URL.to_owned()]);
    assert_eq!(commands(&s.fixture).len(), calls);
    assert_eq!(sql_state(&s.fixture), sql_before);
    unchanged_git_and_exact_script(&s);
    cache.take().unwrap().1.shutdown();
}

#[test]
fn pending_reviews_v2_avatars_v7_newer_same_allocation_same_actor_new_ticket_preserves_request_and_map(
) {
    let (s, mut app) = setup(observed("actual-author", 43, "User"));
    let actor = metadata(&s, &app);
    let identity = ActorIdentity::github(HOST, &actor);
    let fetcher = Arc::new(Fetcher::default());
    fetcher.hold.store(true, Ordering::SeqCst);
    let mut cache = cache(&s, &app, fetcher.clone());
    let mut old = None;
    draw(&mut app, 120, 40);
    poll(&s, &mut app, &mut cache, &mut old);
    wait(|| fetcher.calls.load(Ordering::SeqCst) == 1);
    let original_cache = cache.as_ref().unwrap().1.clone();
    let old_ticket = app
        .scoped_reviews_v2
        .as_ref()
        .unwrap()
        .ticket_v7_test()
        .clone();
    app.scoped_reviews_v2
        .as_mut()
        .unwrap()
        .apply(ReviewBrowserActionV2::Select(0));
    assert!(!Arc::ptr_eq(
        &old_ticket,
        app.scoped_reviews_v2.as_ref().unwrap().ticket_v7_test()
    ));
    draw(&mut app, 120, 40);
    let mut newer = None;
    poll(&s, &mut app, &mut cache, &mut newer);
    assert!(cache.as_ref().unwrap().1.same_instance_v7(&original_cache));
    // Even after that newer view closes, old ticket absence is not authority:
    // its successful projection marker is different on the SAME allocation.
    app.scoped_reviews_v2 = None;
    app.overlay = None;
    avatars::retire_lease_for_test(&mut app, &mut cache, &mut old);
    assert!(old.is_none());
    assert!(cache.as_ref().unwrap().1.same_instance_v7(&original_cache));
    assert!(matches!(
        app.avatar_states.get(&identity),
        Some(AvatarState::Loading)
    ));
    assert!(!fetcher.cancelled.load(Ordering::SeqCst));
    fetcher.released.store(true, Ordering::SeqCst);
    wait(|| matches!(original_cache.request(&identity), AvatarState::Ready(_)));
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    unchanged_git_and_exact_script(&s);
    original_cache.shutdown();
}

#[test]
fn pending_reviews_v2_avatars_v7_newer_same_cache_different_observed_author_projection_is_not_old_cleanup_authority(
) {
    let (s, mut app) = setup_users(vec![
        observed("actual-author", 43, "User"),
        observed("other-observed-author", 44, "Bot"),
    ]);
    let actor = app
        .scoped_reviews_v2
        .as_ref()
        .unwrap()
        .draft_v7_test()
        .catalogue()
        .rows()[1]
        .author()
        .clone();
    let identity = ActorIdentity::github(HOST, &actor);
    let fetcher = Arc::new(Fetcher::default());
    fetcher.hold.store(true, Ordering::SeqCst);
    let mut cache = cache(&s, &app, fetcher.clone());
    let mut old = None;
    draw(&mut app, 120, 40);
    poll(&s, &mut app, &mut cache, &mut old);
    wait(|| fetcher.calls.load(Ordering::SeqCst) == 2);
    let requester = app
        .scoped_reviews_v2
        .as_ref()
        .unwrap()
        .draft_v7_test()
        .identity()
        .clone();
    app.scoped_reviews_v2
        .as_mut()
        .unwrap()
        .apply(ReviewBrowserActionV2::Select(1));
    // SAME shared dispatcher entry point, a new successful actual-observation
    // projection, no actor manufactured from author UID or requester login.
    let current = cache.as_ref().unwrap().1.clone();
    assert!(app.avatar_states.sync_from_cache(
        &requester,
        "Newer actual observed author surface",
        &current,
        &[actor]
    ));
    app.scoped_reviews_v2 = None;
    app.overlay = None;
    avatars::retire_lease_for_test(&mut app, &mut cache, &mut old);
    assert!(cache.as_ref().unwrap().1.same_instance_v7(&current));
    assert_eq!(app.avatar_states.len(), 1);
    assert!(matches!(
        app.avatar_states.get(&identity),
        Some(AvatarState::Loading)
    ));
    assert!(!fetcher.cancelled.load(Ordering::SeqCst));
    fetcher.released.store(true, Ordering::SeqCst);
    wait(|| matches!(current.request(&identity), AvatarState::Ready(_)));
    unchanged_git_and_exact_script(&s);
    current.shutdown();
}

#[test]
fn pending_reviews_v2_avatars_v7_different_genuine_source_and_cache_allocation_preserve_newer_actor_requests(
) {
    let (s, mut app) = setup(observed("old-author", 43, "User"));
    let fetcher = Arc::new(Fetcher::default());
    let mut cache = cache(&s, &app, fetcher);
    let mut old = None;
    draw(&mut app, 120, 40);
    poll(&s, &mut app, &mut cache, &mut old);
    let (new_s, mut newer) = setup(observed("new-author", 44, "User"));
    let new_actor = metadata(&new_s, &newer);
    let new_identity = ActorIdentity::github(HOST, &new_actor);
    let new_fetcher = Arc::new(Fetcher::default());
    new_fetcher.hold.store(true, Ordering::SeqCst);
    let mut new_cache = self::cache(&new_s, &newer, new_fetcher.clone());
    draw(&mut newer, 120, 40);
    let mut newer_lease = None;
    poll(&new_s, &mut newer, &mut new_cache, &mut newer_lease);
    wait(|| new_fetcher.calls.load(Ordering::SeqCst) == 1);
    let new_allocation = new_cache.as_ref().unwrap().1.clone();
    let source = newer.remote_scope_source.clone();
    let nav = newer.github.as_ref().unwrap().scoped_pr_navigation.clone();
    avatars::retire_lease_for_test(&mut newer, &mut new_cache, &mut old);
    assert!(new_cache
        .as_ref()
        .unwrap()
        .1
        .same_instance_v7(&new_allocation));
    assert!(matches!(
        newer.avatar_states.get(&new_identity),
        Some(AvatarState::Loading)
    ));
    assert!(Arc::ptr_eq(
        source.as_ref().unwrap(),
        newer.remote_scope_source.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        nav.as_ref().unwrap(),
        newer
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap()
    ));
    assert!(!new_fetcher.cancelled.load(Ordering::SeqCst));
    new_fetcher.released.store(true, Ordering::SeqCst);
    new_allocation.shutdown();
    cache.take().unwrap().1.shutdown();
    unchanged_git_and_exact_script(&s);
    unchanged_git_and_exact_script(&new_s);
}

#[test]
fn pending_reviews_v2_avatars_v7_geometry_invalidates_marker_without_retry_or_hidden_underlay_fetch(
) {
    let (s, mut app) = setup(observed("actual-author", 43, "User"));
    let fetcher = Arc::new(Fetcher::default());
    fetcher.hold.store(true, Ordering::SeqCst);
    let mut cache = cache(&s, &app, fetcher.clone());
    let mut lease = None;
    draw(&mut app, 120, 40);
    poll(&s, &mut app, &mut cache, &mut lease);
    wait(|| fetcher.calls.load(Ordering::SeqCst) == 1);
    native_v2::event(&mut app, crossterm::event::Event::Resize(80, 24));
    assert!(poll(&s, &mut app, &mut cache, &mut lease));
    assert!(app.avatar_states.is_empty());
    draw(&mut app, 80, 24);
    poll(&s, &mut app, &mut cache, &mut lease);
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    assert!(!fetcher.cancelled.load(Ordering::SeqCst));
    app.scoped_reviews_v2 = None;
    app.overlay = None;
    poll(&s, &mut app, &mut cache, &mut lease);
    assert!(cache.is_none());
    assert!(fetcher.cancelled.load(Ordering::SeqCst));
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_avatars_v7_disabled_nocolor_missing_layout_and_raw_visible_limits_have_http_zero(
) {
    for mode in 0..4 {
        let (s, mut app) = setup(observed("actual-author", 43, "User"));
        let fetcher = Arc::new(Fetcher::default());
        let mut cache = cache(&s, &app, fetcher.clone());
        let mut lease = None;
        if mode != 2 {
            draw(&mut app, 120, 40);
        }
        if mode == 1 {
            app.color_mode = crate::theme::ColorMode::NoColor;
        }
        if mode == 3 {
            app.scoped_reviews_v2.as_mut().unwrap().clear_layout();
        }
        assert!(avatars::poll_review(
            &mut app,
            &mut cache,
            &s.fixture.root.join("review-avatars"),
            mode != 0,
            &mut lease
        ));
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 0);
        assert!(app.avatar_states.is_empty());
        unchanged_git_and_exact_script(&s);
        if let Some((_, cache)) = cache {
            cache.shutdown();
        }
    }
}

#[test]
fn pending_reviews_v2_avatars_v7_actual_positive_uid_loss_retires_only_original_held_asset_before_map_clear(
) {
    let mut s = inline_setup(|h, steps| {
        let mut row = review(h, "PENDING", "Actual private author body", 43);
        row["user"] = observed("actual-author", 43, "User");
        steps.extend(open_steps(h, json!([row])));
        steps.extend(principal(43, "another-observed-account"));
    });
    let (mut app, _) = owner(&mut s);
    let draft = open(&s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
    app.scoped_reviews_v2 = Some(
        ReviewBrowserV2::prepare(
            draft,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap(),
    );
    app.overlay = Some(crate::app::Overlay::ScopedReviewsV2);
    let fetcher = Arc::new(Fetcher::default());
    fetcher.hold.store(true, Ordering::SeqCst);
    let mut cache = cache(&s, &app, fetcher.clone());
    let mut lease = None;
    draw(&mut app, 120, 40);
    poll(&s, &mut app, &mut cache, &mut lease);
    wait(|| fetcher.calls.load(Ordering::SeqCst) == 1);
    let actual = provider(&s.fixture).identity(&Default::default()).unwrap();
    assert_eq!(actual.account_id, Some(43));
    app.github.as_mut().unwrap().identity = actual;
    poll(&s, &mut app, &mut cache, &mut lease);
    assert!(app.scoped_reviews_v2.is_none());
    assert!(app.avatar_states.is_empty());
    assert!(cache.is_none());
    assert!(fetcher.cancelled.load(Ordering::SeqCst));
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_avatars_v7_visible_129_actor_and_64k_raw_batches_refuse_before_any_asset_dispatch(
) {
    for metadata_bound in [false, true] {
        let mut user = observed("actual-author", 43, "User");
        if metadata_bound {
            user["name"] = json!("x".repeat(4096));
        }
        let count = if metadata_bound { 20 } else { 129 };
        let (s, mut app) = setup_users(vec![user; count]);
        let fetcher = Arc::new(Fetcher::default());
        let mut cache = cache(&s, &app, fetcher.clone());
        let mut lease = None;
        draw(&mut app, 120, 180);
        poll(&s, &mut app, &mut cache, &mut lease);
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 0);
        assert!(app.avatar_states.is_empty());
        assert!(cache.is_none());
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_avatars_v7_original_deadline_cancel_and_actual_oversized_actor_refusal_never_dispatch_partial_rows(
) {
    let (s, app) = setup(observed("actual-author", 43, "User"));
    let draft = app
        .scoped_reviews_v2
        .as_ref()
        .unwrap()
        .draft_v7_test()
        .clone();
    let calls = commands(&s.fixture).len();
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(ReviewBrowserV2::prepare(
        draft.clone(),
        &cancel,
        Instant::now() + Duration::from_secs(30)
    )
    .is_err());
    assert!(ReviewBrowserV2::prepare(
        draft,
        &Default::default(),
        Instant::now() - Duration::from_millis(1)
    )
    .is_err());
    assert_eq!(commands(&s.fixture).len(), calls);
    unchanged_git_and_exact_script(&s);
    let mut large = observed("not-an-authorized-shortened-identity", 43, "User");
    large["name"] = json!("x".repeat(4097));
    let mut s = inline_setup(|h, steps| {
        let mut row = review(h, "PENDING", "Private oversized actor", 43);
        row["user"] = large;
        steps.extend(opening_steps(h));
        steps.push(catalogue_step(json!([row])));
    });
    let (mut app, _) = owner(&mut s);
    assert!(open(&s, ScopedReviewIntentV2::capture(&app).unwrap()).is_err());
    let fetcher = Arc::new(Fetcher::default());
    let mut cache = cache(&s, &app, fetcher.clone());
    let mut lease = None;
    assert!(!poll(&s, &mut app, &mut cache, &mut lease));
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 0);
    unchanged_git_and_exact_script(&s);
    if let Some((_, cache)) = cache {
        cache.shutdown();
    }
}

#[test]
fn pending_reviews_v2_avatars_v7_clear_or_failed_projection_has_no_marker_and_cannot_own_old_cache_cleanup(
) {
    for failed in [false, true] {
        let (s, mut app) = setup(observed("actual-author", 43, "User"));
        let fetcher = Arc::new(Fetcher::default());
        fetcher.hold.store(true, Ordering::SeqCst);
        let mut cache = cache(&s, &app, fetcher.clone());
        let mut old = None;
        draw(&mut app, 120, 40);
        poll(&s, &mut app, &mut cache, &mut old);
        wait(|| fetcher.calls.load(Ordering::SeqCst) == 1);
        let allocation = cache.as_ref().unwrap().1.clone();
        if failed {
            let mut bad = app.github.as_ref().unwrap().identity.clone();
            bad.account_id = None;
            assert!(!app.avatar_states.sync_from_cache(
                &bad,
                "Invalid projection",
                &allocation,
                &[]
            ));
        } else {
            app.avatar_states.clear();
        }
        assert!(app.avatar_states.projection_v7().is_none());
        app.scoped_reviews_v2 = None;
        app.overlay = None;
        avatars::retire_lease_for_test(&mut app, &mut cache, &mut old);
        assert!(cache.as_ref().unwrap().1.same_instance_v7(&allocation));
        assert!(app.avatar_states.is_empty());
        // The failed sync itself may cancel its own invalid assets. Old cleanup
        // never reacquires ownership from None or revives a successful marker.
        assert!(app.avatar_states.projection_v7().is_none());
        fetcher.released.store(true, Ordering::SeqCst);
        allocation.shutdown();
        unchanged_git_and_exact_script(&s);
    }
}

#[test]
fn pending_reviews_v2_avatars_v7_actual_selected_form_and_fresh_preview_keep_requester_review_and_comment_actors_distinct(
) {
    use images::reviews_v2::{ui_v2, ReviewRequestedActionV2, ScopedReviewPreviewV2};
    let mut s = inline_setup(|h, steps| {
        let mut actual = review(h, "APPROVED", "Actual submitted author body", 43);
        actual["user"] = observed("actual-review-author", 43, "User");
        steps.extend(open_steps(h, json!([actual.clone()])));
        steps.extend(opening_steps(h));
        steps.extend(snapshot_steps(h, &actual));
        steps.extend(closing_steps(h));
        steps.extend(open_steps(h, json!([actual.clone()])));
        steps.extend(opening_steps(h));
        let mut fresh = snapshot_steps(h, &actual);
        let mut comments: Value = serde_json::from_slice(&fresh[1].stdout).unwrap();
        comments[0]["user"] = observed("observed-comment-bot", 44, "Bot");
        comments[1]["user"] = Value::Null;
        fresh[1].stdout = comments.to_string().into_bytes();
        steps.extend(fresh);
        steps.extend(closing_steps(h));
    });
    let (mut app, _) = owner(&mut s);
    let draft = open(&s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
    let selected = ScopedReviewSelectionV2::read(
        &draft,
        0,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let draft = ScopedReviewDraftV2::open_selected(
        &selected,
        ReviewRequestedActionV2::Dismiss,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let mut form = ui_v2::form_from_draft(
        draft,
        Some(selected),
        ReviewRequestedActionV2::Dismiss,
        None,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    form.paste("Exact dismissal reason, server privilege UNKNOWN");
    app.form = Some(form);
    app.overlay = Some(crate::app::Overlay::Form);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::forms::draw(frame, &mut app, frame.area()))
        .unwrap();
    let initial = text(terminal.backend().buffer());
    assert!(initial.contains("Observed review author @actual-review-author"));
    assert!(initial.contains("UID Some(43)"));
    assert!(initial.contains("numeric UID Some(42)"));
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
    terminal
        .draw(|frame| crate::forms::draw(frame, &mut app, frame.area()))
        .unwrap();
    let fetcher = Arc::new(Fetcher::default());
    let mut cache = cache(&s, &app, fetcher.clone());
    let mut lease = None;
    poll(&s, &mut app, &mut cache, &mut lease);
    wait(|| {
        poll(&s, &mut app, &mut cache, &mut lease);
        fetcher.calls.load(Ordering::SeqCst) >= 1
    });
    // Scroll invalidates the pure old-row marker before another draw. No old
    // comment row becomes an image request just from retained metadata.
    let calls = fetcher.calls.load(Ordering::SeqCst);
    app.form.as_mut().unwrap().preview_scroll += 1;
    poll(&s, &mut app, &mut cache, &mut lease);
    assert!(app.avatar_states.is_empty());
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), calls);
    terminal
        .draw(|frame| crate::forms::draw(frame, &mut app, frame.area()))
        .unwrap();
    poll(&s, &mut app, &mut cache, &mut lease);
    let rendered = text(terminal.backend().buffer());
    assert!(rendered.contains("@observed-comment-bot"));
    assert!(rendered.contains("[bot]"));
    assert!(rendered.contains("unknown actor"));
    assert!(!request.matches_form(app.form.as_ref().unwrap()),"a fresh Preview changes the exact captured request, never restores the old pre-preview request");
    unchanged_git_and_exact_script(&s);
    if let Some((_, cache)) = cache {
        cache.shutdown();
    }
}
