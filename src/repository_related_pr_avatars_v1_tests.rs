//! Genuine closed sources; synthetic oversized Actor values below are only
//! projection boundary inputs, never constructed access receipts.
use super::*;
use crate::{
    identity::{AvatarError, AvatarFetcher, AvatarState},
    providers::runner::CancellationToken,
    repository_commit_actor_native_v1::tests::Kit,
    repository_related_pr_v1::{self as related, RelatedPrActionV1},
    test_support::{Fixture, GhStep},
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use serde_json::{json, Value};
use std::{
    fs,
    io::Cursor,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Default)]
struct Fetcher {
    calls: AtomicUsize,
    urls: Mutex<Vec<String>>,
    wait: AtomicBool,
    cancelled: AtomicBool,
}
impl AvatarFetcher for Fetcher {
    fn fetch(&self, url: &str, cancel: &CancellationToken) -> Result<Vec<u8>, AvatarError> {
        self.urls.lock().unwrap().push(url.into());
        self.calls.fetch_add(1, Ordering::SeqCst);
        let until = Instant::now() + Duration::from_secs(3);
        while self.wait.load(Ordering::SeqCst) {
            if cancel.is_cancelled() {
                self.cancelled.store(true, Ordering::SeqCst);
                return Err(AvatarError("cancelled original public asset".into()));
            }
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
        let mut png = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba([255, 0, 0, 255])))
            .write_to(&mut png, ImageFormat::Png)
            .unwrap();
        Ok(png.into_inner())
    }
}
fn until(mut ready: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(3);
    while !ready() {
        assert!(Instant::now() < end);
        std::thread::yield_now();
    }
}
fn loaded(rows: usize, long_name: bool) -> (App, Fixture) {
    let kit = Kit::new("sha256", false, 42);
    let mut steps = related::tests::protocol(&kit.sha);
    let repo: Value = serde_json::from_slice(&steps[9].stdout).unwrap();
    let candidates: Vec<Value> = (0..rows).map(|i| json!({"id":9009+i,"node_id":format!("PR_safe_{}",9009+i),"number":9+i,
        "title":format!("Observed candidate {i}"),"body":"original private observation","state":"open","merged":false,
        "user":{"id":301+i,"node_id":format!("USER_{}",301+i),"login":format!("writer-{i}"),"type":"User",
            "name":if long_name {"x".repeat(1000)} else {format!("Writer {i}")},"avatar_url":format!("https://avatars.githubusercontent.com/u/{}?v=4{}",301+i,if long_name {"&padding=".to_owned()+&"x".repeat(1000)} else {String::new()})},
        "base":{"repo":repo},"url":format!("https://github.invalid/api/v3/repos/example/original/pulls/{}",9+i),
        "html_url":format!("https://github.invalid/example/original/pull/{}",9+i)})).collect();
    let pages = candidates.len().div_ceil(50).max(1);
    let reads: Vec<_> = (0..pages)
        .map(|page| {
            GhStep::read(
                &[
                    "api",
                    &format!(
                        "repos/example/original/commits/{}/pulls?per_page=50&page={}",
                        kit.sha,
                        page + 1
                    ),
                    "--hostname",
                    "github.invalid",
                    "--method",
                    "GET",
                ],
                &json!(&candidates[page * 50..((page + 1) * 50).min(candidates.len())]).to_string(),
            )
        })
        .collect();
    steps.splice(11..12, reads);
    kit.fixture.scripted_gh(&steps);
    let mut app = App {
        view: crate::app::View::Repository,
        no_color: false,
        workspace: Some(kit.workspace),
        local_commit_actors: kit.state,
        ..Default::default()
    };
    related::dispatch(&mut app, RelatedPrActionV1::Discover);
    app.related_pr_requested_v1 = false;
    let owner = related::begin(&mut app, Default::default()).unwrap();
    let result = related::read(
        &owner,
        &kit.provider,
        &kit.fixture.root.join("native.sqlite3"),
    )
    .unwrap();
    assert!(related::accept(&mut app, &owner, result));
    assert_eq!(owner.operation.accounting().unwrap().children_started(), 2);
    let journal = fs::read_to_string(kit.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(journal.lines().count(), 14 + pages);
    assert!(journal
        .lines()
        .all(|r| serde_json::from_str::<Value>(r).unwrap()["matched"] == true));
    (app, kit.fixture)
}
fn draw(app: &mut App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| {
            assert!(related::draw(f, app));
        })
        .unwrap();
    terminal.backend().buffer().clone()
}
fn cache(app: &App, fixture: &Fixture, fetcher: Arc<Fetcher>) -> Option<(String, AvatarCache)> {
    let scope = partition(
        app.related_pr_v1
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .identity(),
    )
    .unwrap();
    assert_eq!(scope, "github.invalid/user-id:42");
    Some((
        scope.clone(),
        AvatarCache::new(fixture.root.join("avatars"), scope, fetcher),
    ))
}
#[test]
fn related_pr_avatars_genuine_visible_candidates_only_ready_pixels_and_weak_retirement() {
    let (mut app, fixture) = loaded(20, false);
    let f = Arc::new(Fetcher::default());
    let mut cache = cache(&app, &fixture, f.clone());
    assert!(poll_related(&mut app, &mut cache, &fixture.root, true));
    assert_eq!(
        f.calls.load(Ordering::SeqCst),
        0,
        "No draw geometry is not hidden first128 prefetch"
    );
    draw(&mut app, 80, 10);
    let expected = actors(app.related_pr_v1.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert!(!expected.is_empty() && expected.len() < 20);
    until(|| {
        poll_related(&mut app, &mut cache, &fixture.root, true);
        app.avatar_states.len() == expected.len()
            && expected.iter().all(|a| {
                matches!(
                    app.avatar_states
                        .get(&ActorIdentity::github("github.invalid", a)),
                    Some(AvatarState::Ready(_))
                )
            })
    });
    assert_eq!(f.calls.load(Ordering::SeqCst), expected.len());
    let paint = draw(&mut app, 80, 10);
    assert!(paint
        .content
        .iter()
        .any(|c| c.symbol() == "▀" && c.fg == ratatui::style::Color::Rgb(255, 0, 0)));
    let weak = Arc::downgrade(
        app.related_pr_v1
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap(),
    );
    related::dispatch(&mut app, RelatedPrActionV1::Back);
    assert!(
        weak.upgrade().is_none(),
        "Geometry must not retain retired private PR bodies"
    );
    app.avatar_states.retire_owned_cache(&mut cache);
}
#[test]
fn related_pr_avatars_resize_scroll_replay_and_tiny_geometry_never_fetch_hidden_rows() {
    let (mut app, fixture) = loaded(20, false);
    draw(&mut app, 80, 10);
    let old = app
        .related_pr_v1
        .as_mut()
        .unwrap()
        .avatar_view_v1
        .take()
        .unwrap();
    related::dispatch(&mut app, RelatedPrActionV1::Scroll(12));
    assert!(app.related_pr_v1.as_ref().unwrap().avatar_view_v1.is_none());
    app.related_pr_v1.as_mut().unwrap().avatar_view_v1 = Some(old);
    assert!(
        actors(app.related_pr_v1.as_ref().unwrap()).is_err(),
        "Old viewport is not a new observed row request"
    );
    draw(&mut app, 80, 10);
    let actual = actors(app.related_pr_v1.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(actual[0].login, "writer-12");
    related::event(&mut app, crossterm::event::Event::Resize(2, 10));
    assert!(app.related_pr_v1.as_ref().unwrap().avatar_view_v1.is_none());
    draw(&mut app, 2, 10);
    assert!(actors(app.related_pr_v1.as_ref().unwrap())
        .unwrap()
        .is_none_or(|rows| rows.is_empty()));
    let f = Arc::new(Fetcher::default());
    let mut cache = cache(&app, &fixture, f.clone());
    assert!(poll_related(&mut app, &mut cache, &fixture.root, true));
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    app.avatar_states.retire_owned_cache(&mut cache);
}
#[test]
fn related_pr_avatars_pending_disabled_no_color_and_source_loss_cancel_actual_asset() {
    for boundary in 0..4 {
        let (mut app, fixture) = loaded(1, false);
        draw(&mut app, 80, 24);
        let f = Arc::new(Fetcher::default());
        f.wait.store(true, Ordering::SeqCst);
        let mut cache = cache(&app, &fixture, f.clone());
        assert!(poll_related(&mut app, &mut cache, &fixture.root, true));
        until(|| f.calls.load(Ordering::SeqCst) == 1);
        match boundary {
            0 => related::dispatch(&mut app, RelatedPrActionV1::Discover),
            1 => {}
            2 => app.color_mode = ColorMode::NoColor,
            _ => app.view = crate::app::View::Dashboard,
        }
        let owned = poll_related(&mut app, &mut cache, &fixture.root, boundary != 1);
        assert_eq!(owned, boundary != 3);
        assert!(cache.is_none());
        assert_eq!(app.avatar_states.len(), 0);
        assert!(f.cancelled.load(Ordering::SeqCst));
        assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    }
}
#[test]
fn related_pr_avatars_geometry_only_preserves_original_cache_failure_epoch() {
    let (mut app, fixture) = loaded(1, false);
    draw(&mut app, 80, 24);
    let f = Arc::new(Fetcher::default());
    f.wait.store(true, Ordering::SeqCst);
    let mut cache = cache(&app, &fixture, f.clone());
    poll_related(&mut app, &mut cache, &fixture.root, true);
    until(|| f.calls.load(Ordering::SeqCst) == 1);
    related::event(&mut app, crossterm::event::Event::Resize(120, 40));
    poll_related(&mut app, &mut cache, &fixture.root, true);
    assert!(cache.is_some());
    assert!(!f.cancelled.load(Ordering::SeqCst));
    draw(&mut app, 120, 40);
    f.wait.store(false, Ordering::SeqCst);
    until(|| {
        poll_related(&mut app, &mut cache, &fixture.root, true);
        app.avatar_states
            .ready(
                app.related_pr_v1
                    .as_ref()
                    .unwrap()
                    .snapshot
                    .as_ref()
                    .unwrap()
                    .candidates()[0]
                    .display_actor(),
            )
            .is_some()
    });
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    app.avatar_states.retire_owned_cache(&mut cache);
}
#[test]
fn related_pr_avatars_genuine_aggregate_preflight_no_partial_requests_or_cropped_actor() {
    let (mut app, fixture) = loaded(49, true);
    draw(&mut app, 120, 100);
    assert!(actors(app.related_pr_v1.as_ref().unwrap()).is_err());
    let f = Arc::new(Fetcher::default());
    let mut cache = cache(&app, &fixture, f.clone());
    assert!(poll_related(&mut app, &mut cache, &fixture.root, true));
    assert!(cache.is_none());
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        app.related_pr_v1
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .candidates()[48]
            .pr()
            .author
            .display_name
            .as_ref()
            .unwrap()
            .len(),
        1000,
        "Actual observed source untouched"
    );
}
#[test]
fn related_pr_avatars_genuine_visible_projection_caps128_observed_actors_not_source_rows() {
    let (mut app, _fixture) = loaded(200, false);
    draw(&mut app, 80, 220);
    let observed = actors(app.related_pr_v1.as_ref().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(observed.len(), 128);
    assert_eq!(observed[0].login, "writer-0");
    assert_eq!(observed[127].login, "writer-127");
    assert_eq!(
        app.related_pr_v1
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap()
            .candidates()
            .len(),
        200
    );
}
#[test]
fn related_pr_avatars_equal_numeric_partition_foreign_closed_source_marker_cannot_dispatch() {
    let (mut old, _old_fixture) = loaded(1, false);
    draw(&mut old, 80, 24);
    let (mut app, fixture) = loaded(1, false);
    draw(&mut app, 80, 24);
    app.related_pr_v1.as_mut().unwrap().avatar_view_v1 =
        old.related_pr_v1.as_mut().unwrap().avatar_view_v1.take();
    assert!(actors(app.related_pr_v1.as_ref().unwrap()).is_err());
    let f = Arc::new(Fetcher::default());
    let mut cache = cache(&app, &fixture, f.clone());
    assert!(poll_related(&mut app, &mut cache, &fixture.root, true));
    assert!(cache.is_none());
    assert_eq!(f.calls.load(Ordering::SeqCst), 0);
    assert!(app.related_pr_v1.as_ref().unwrap().avatar_view_v1.is_none());
}

#[test]
fn related_pr_avatars_actual_cohort_diff_cancel_veto_drains_original_held_asset() {
    let (mut app, fixture) = loaded(1, false);
    draw(&mut app, 80, 24);
    let f = Arc::new(Fetcher::default());
    f.wait.store(true, Ordering::SeqCst);
    let mut cache = cache(&app, &fixture, f.clone());
    assert!(poll_related(&mut app, &mut cache, &fixture.root, true));
    until(|| f.calls.load(Ordering::SeqCst) == 1);
    app.original_diff_cancel_v1 = true;
    assert!(poll_related(&mut app, &mut cache, &fixture.root, true));
    assert!(cache.is_none());
    assert!(f.cancelled.load(Ordering::SeqCst));
    assert_eq!(f.calls.load(Ordering::SeqCst), 1);
    assert!(app.related_pr_v1.as_ref().unwrap().avatar_view_v1.is_none());
}
