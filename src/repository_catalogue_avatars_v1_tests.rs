//! SOURCE-ONLY actual-producer/asset/widget contracts. No fabricated opaque
//! reads or reconstructed permission receipts; all declarations are UNRUN.
use super::*;
use crate::test_support::{Fixture, GhStep};
use crate::{
    config::ThemeName,
    identity::{ActorKind, AvatarError, AvatarFetcher, AvatarWidget, CurlAvatarFetcher},
    providers::{
        gh::{GithubProvider, RepositoryCatalogueLimits, RepositoryCatalogueSource},
        runner::CancellationToken,
    },
    repository_catalogue_workspace_v1::CatalogueInput,
    theme::{ColorMode, Theme},
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use ratatui::{backend::TestBackend, layout::Rect, widgets::Widget, Terminal};
use serde_json::{json, Value};
use std::{
    io::Cursor,
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{Duration, Instant},
};

const URL: &str = "https://avatars.githubusercontent.com/u/71?v=4";
fn get(endpoint: &str, value: Value) -> GhStep {
    GhStep::read(
        &[
            "api",
            endpoint,
            "--hostname",
            "github.invalid",
            "--method",
            "GET",
        ],
        &value.to_string(),
    )
}
fn actual(
    uid: u64,
    count: usize,
    nullable: bool,
    dual_url: bool,
) -> (Fixture, Arc<RepositoryCatalogueRead>, CatalogueWorkspace) {
    let fixture = Fixture::new();
    let account = || {
        vec![
            GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
            get("user", json!({"id":uid,"login":"octocat"})),
        ]
    };
    let owner = json!({"id":71,"node_id":"OWNER_avatar71","login":"example","name":"Example Org","type":"Organization","avatar_url":URL});
    let mut steps = account();
    steps.push(get("orgs/example", owner.clone()));
    for page in 0..=count / 100 {
        let values=(page*100..((page+1)*100).min(count)).map(|index| {
            let mut actor=owner.clone();
            if dual_url && index==1 {actor["avatar_url"]=json!("https://avatars.githubusercontent.com/u/71?v=5");}
            json!({"id":10000+index,"node_id":format!("R_avatar{index}"),"full_name":format!("example/repo{index:04}"),"owner":if nullable {Value::Null}else{actor},"private":true,"archived":false,"permissions":{"pull":true}})
        }).collect::<Vec<_>>();
        steps.push(get(
            &format!(
                "orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page={}",
                page + 1
            ),
            json!(values),
        ));
    }
    steps.push(get("orgs/example", owner));
    steps.extend(account());
    fixture.scripted_gh(&steps);
    let client = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let source = Arc::new(
        client
            .repository_catalogue_read(
                None,
                &RepositoryCatalogueSource::Organization("example".into()),
                &RepositoryCatalogueLimits::default(),
                &CancellationToken::default(),
            )
            .unwrap(),
    );
    let log = std::fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(log.lines().count(), steps.len());
    assert!(log
        .lines()
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .all(|row| row["matched"] == true && row["stdin_hex"] == ""));
    let (mut workspace, job) = CatalogueWorkspace::new(source.clone()).unwrap();
    workspace
        .accept(job.prepare(&Default::default()).unwrap())
        .unwrap();
    draw(&mut workspace, None, ColorMode::TrueColor);
    (fixture, source, workspace)
}
fn png(red: bool) -> Vec<u8> {
    let rgba = RgbaImage::from_fn(2, 2, |_, y| {
        if red && y == 0 {
            Rgba([255, 0, 0, 255])
        } else {
            Rgba([0, 0, 255, 255])
        }
    });
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(rgba)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
struct Assets {
    calls: Mutex<Vec<String>>,
    fail: bool,
}
impl AvatarFetcher for Assets {
    fn fetch(&self, url: &str, cancel: &CancellationToken) -> Result<Vec<u8>, AvatarError> {
        if cancel.is_cancelled() {
            return Err(AvatarError("cancelled fixture".into()));
        }
        self.calls.lock().unwrap().push(url.into());
        if self.fail {
            Err(AvatarError("fixture unavailable".into()))
        } else {
            Ok(png(url.ends_with("v=4")))
        }
    }
}
fn cache(
    fixture: &Fixture,
    request: &CatalogueAvatarRequest,
    fail: bool,
) -> (AvatarCache, Arc<Assets>) {
    let fetcher = Arc::new(Assets {
        calls: Mutex::new(vec![]),
        fail,
    });
    (
        AvatarCache::new(
            fixture.root.join("assets"),
            request.cache_scope().into(),
            fetcher.clone(),
        ),
        fetcher,
    )
}
fn draw(
    workspace: &mut CatalogueWorkspace,
    assets: Option<&CatalogueAvatarAssets>,
    mode: ColorMode,
) -> Terminal<TestBackend> {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| {
            workspace.draw_with_avatars(
                frame,
                frame.area(),
                Theme::new(ThemeName::CatppuccinMocha, mode),
                assets,
            )
        })
        .unwrap();
    terminal
}
fn ready(
    assets: &mut CatalogueAvatarAssets,
    request: &CatalogueAvatarRequest,
    cache: &AvatarCache,
) {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        assets.prefetch(request, cache);
        let frame = request.frame.as_ref().unwrap();
        if request.indices.iter().all(|offset| {
            assets
                .image(&request.source, &frame.rows[*offset])
                .is_some()
        }) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Actual bounded asset workers must finish"
        );
        std::thread::yield_now();
    }
}

#[test]
fn repo_catalog_avatar_genuine_organization_kind_and_actual_owner_not_guessed_user() {
    let (_fixture, source, workspace) = actual(42, 1, false, false);
    let row = &workspace.frame().unwrap().rows[0];
    let identity = row.actor.as_ref().unwrap();
    assert_eq!(identity.kind, ActorKind::Organization);
    assert_eq!(identity.label(), "@example [organization]");
    assert_eq!(identity.initials(), "EO");
    assert_eq!(
        source.repositories()[0].owner.as_ref().unwrap().database_id,
        Some(71)
    );
}

#[test]
fn repo_catalog_avatar_ready_pixels_borrowed_draw_preserves_genuine_row_hit_no_io() {
    let (fixture, source, mut workspace) = actual(42, 1, false, false);
    let request = CatalogueAvatarRequest::from_workspace(&workspace, false).unwrap();
    let (cache, fetcher) = cache(&fixture, &request, false);
    let mut assets = CatalogueAvatarAssets::default();
    ready(&mut assets, &request, &cache);
    let journal = std::fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let before = fetcher.calls.lock().unwrap().clone();
    let terminal = draw(&mut workspace, Some(&assets), ColorMode::TrueColor);
    let hit = workspace
        .hits
        .iter()
        .find(|hit| matches!(hit.input, CatalogueInput::Row(_)))
        .unwrap();
    let x = workspace.list_area.right() - 4;
    let y = hit.rect.y;
    let cell = &terminal.backend().buffer()[(x, y)];
    assert_eq!(cell.symbol(), "▀");
    assert_eq!(cell.fg, ratatui::style::Color::Rgb(255, 0, 0));
    assert_eq!(cell.bg, ratatui::style::Color::Rgb(0, 0, 255));
    assert!(hit.rect.contains(ratatui::layout::Position::new(x, y)));
    assert!(assets
        .image(&source, &workspace.frame().unwrap().rows[0])
        .is_some());
    assert_eq!(*fetcher.calls.lock().unwrap(), before);
    assert_eq!(
        std::fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        journal
    );
    cache.shutdown();
}

#[test]
fn repo_catalog_avatar_same_login_distinct_actual_urls_never_alias_ready_pixels() {
    let (fixture, source, workspace) = actual(42, 2, false, true);
    let request = CatalogueAvatarRequest::from_workspace(&workspace, false).unwrap();
    let (cache, fetcher) = cache(&fixture, &request, false);
    let mut assets = CatalogueAvatarAssets::default();
    ready(&mut assets, &request, &cache);
    let frame = workspace.frame().unwrap();
    assert_ne!(
        assets.image(&source, &frame.rows[0]).unwrap().pixels,
        assets.image(&source, &frame.rows[1]).unwrap().pixels
    );
    assert_eq!(fetcher.calls.lock().unwrap().len(), 2);
    cache.shutdown();
}

#[test]
fn repo_catalog_avatar_new_source_same_original_slot_refuses_old_ready_no_pointer_alias() {
    let (fixture, source, workspace) = actual(42, 1, false, false);
    let request = CatalogueAvatarRequest::from_workspace(&workspace, false).unwrap();
    let (cache, _) = cache(&fixture, &request, false);
    let mut assets = CatalogueAvatarAssets::default();
    ready(&mut assets, &request, &cache);
    let (_other, other, other_workspace) = actual(42, 1, false, false);
    assert!(!Arc::ptr_eq(&source, &other));
    assert!(assets
        .image(&other, &other_workspace.frame().unwrap().rows[0])
        .is_none());
    assets.clear();
    assert!(assets
        .image(&source, &workspace.frame().unwrap().rows[0])
        .is_none());
    cache.shutdown();
}

#[test]
fn repo_catalog_avatar_uid43_same_login_wrong_cache_scope_refuses_before_asset_request() {
    let (fixture, source, workspace) = actual(42, 1, false, false);
    let first = CatalogueAvatarRequest::from_workspace(&workspace, false).unwrap();
    let (cache, fetcher) = cache(&fixture, &first, false);
    let mut assets = CatalogueAvatarAssets::default();
    ready(&mut assets, &first, &cache);
    let (_other, other, other_workspace) = actual(43, 1, false, false);
    let second = CatalogueAvatarRequest::from_workspace(&other_workspace, false).unwrap();
    assert_ne!(first.cache_scope(), second.cache_scope());
    assert!(second.cache_scope().contains("user-id:43"));
    assets.prefetch(&second, &cache);
    assert!(assets.is_empty());
    assert!(assets
        .image(&source, &workspace.frame().unwrap().rows[0])
        .is_none());
    assert!(assets
        .image(&other, &other_workspace.frame().unwrap().rows[0])
        .is_none());
    assert_eq!(fetcher.calls.lock().unwrap().len(), 1);
    cache.shutdown();
}

#[test]
fn repo_catalog_avatar_only_painted_frame_rows_prefetch1001_same_owner_deduplicated() {
    let (fixture, _source, workspace) = actual(42, 1001, false, false);
    let request = CatalogueAvatarRequest::from_workspace(&workspace, false).unwrap();
    assert!(
        request.requested_rows() > 0
            && request.requested_rows() <= usize::from(workspace.list_area.height)
    );
    assert!(request.requested_rows() < 64);
    let (cache, fetcher) = cache(&fixture, &request, false);
    let mut assets = CatalogueAvatarAssets::default();
    ready(&mut assets, &request, &cache);
    assert_eq!(assets.len(), request.requested_rows());
    assert_eq!(fetcher.calls.lock().unwrap().as_slice(), [URL]);
    cache.shutdown();
}

#[test]
fn repo_catalog_avatar_hidden_dialog_and_unpainted_geometry_request_no_new_assets() {
    let (fixture, _source, mut workspace) = actual(42, 1, false, false);
    let request = CatalogueAvatarRequest::from_workspace(&workspace, true).unwrap();
    let (cache, fetcher) = cache(&fixture, &request, false);
    let mut assets = CatalogueAvatarAssets::default();
    assets.prefetch(&request, &cache);
    assert_eq!(request.requested_rows(), 0);
    assert!(fetcher.calls.lock().unwrap().is_empty());
    workspace.clear_layout();
    let request = CatalogueAvatarRequest::from_workspace(&workspace, false).unwrap();
    assets.prefetch(&request, &cache);
    assert_eq!(request.requested_rows(), 0);
    assert!(fetcher.calls.lock().unwrap().is_empty());
    cache.shutdown();
}

#[test]
fn repo_catalog_avatar_missing_owner_no_invented_source_identity_or_public_url() {
    let (fixture, _source, workspace) = actual(42, 1, true, false);
    let request = CatalogueAvatarRequest::from_workspace(&workspace, false).unwrap();
    let (cache, fetcher) = cache(&fixture, &request, false);
    let mut assets = CatalogueAvatarAssets::default();
    assets.prefetch(&request, &cache);
    assert!(assets.is_empty());
    assert!(fetcher.calls.lock().unwrap().is_empty());
    assert!(workspace.frame().unwrap().rows[0].actor.is_none());
    cache.shutdown();
}

#[test]
fn repo_catalog_avatar_no_color_and_low_color_ready_image_still_honest_initials() {
    let (fixture, _source, workspace) = actual(42, 1, false, false);
    let request = CatalogueAvatarRequest::from_workspace(&workspace, false).unwrap();
    let (cache, _) = cache(&fixture, &request, false);
    let mut assets = CatalogueAvatarAssets::default();
    ready(&mut assets, &request, &cache);
    let row = &workspace.frame().unwrap().rows[0];
    let identity = row.actor.as_ref().unwrap();
    for mode in [ColorMode::NoColor, ColorMode::LowColor] {
        let mut buffer = ratatui::buffer::Buffer::empty(Rect::new(0, 0, 4, 1));
        AvatarWidget {
            identity,
            image: assets.image(&request.source, row),
            theme: Theme::new(ThemeName::CatppuccinMocha, mode),
        }
        .render(buffer.area, &mut buffer);
        assert_eq!(
            (0..4).map(|x| buffer[(x, 0)].symbol()).collect::<String>(),
            "[EO]"
        );
    }
    cache.shutdown();
}

#[test]
fn repo_catalog_avatar_failed_public_asset_preserves_exact_owner_and_no_ready_bytes() {
    let (fixture, source, workspace) = actual(42, 1, false, false);
    let request = CatalogueAvatarRequest::from_workspace(&workspace, false).unwrap();
    let (cache, fetcher) = cache(&fixture, &request, true);
    let mut assets = CatalogueAvatarAssets::default();
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        assets.prefetch(&request, &cache);
        if matches!(assets.entries[&0].state, AvatarState::Failed(_)) {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(assets
        .image(&source, &workspace.frame().unwrap().rows[0])
        .is_none());
    assert_eq!(
        workspace.frame().unwrap().rows[0]
            .actor
            .as_ref()
            .unwrap()
            .label(),
        "@example [organization]"
    );
    assert_eq!(fetcher.calls.lock().unwrap().len(), 1);
    cache.shutdown();
}

#[test]
fn repo_catalog_avatar_retirement_cancel_drains_real_asset_worker_no_late_ready() {
    struct Pending {
        started: AtomicBool,
        cancelled: AtomicBool,
    }
    impl AvatarFetcher for Pending {
        fn fetch(&self, _: &str, cancel: &CancellationToken) -> Result<Vec<u8>, AvatarError> {
            self.started.store(true, Ordering::SeqCst);
            let deadline = Instant::now() + Duration::from_secs(3);
            while !cancel.is_cancelled() {
                if Instant::now() >= deadline {
                    return Err(AvatarError("fixture deadline".into()));
                }
                std::thread::yield_now();
            }
            self.cancelled.store(true, Ordering::SeqCst);
            Err(AvatarError("cancelled".into()))
        }
    }
    let (fixture, source, workspace) = actual(42, 1, false, false);
    let request = CatalogueAvatarRequest::from_workspace(&workspace, false).unwrap();
    let fetcher = Arc::new(Pending {
        started: AtomicBool::new(false),
        cancelled: AtomicBool::new(false),
    });
    let cache = AvatarCache::new(
        fixture.root.join("assets"),
        request.cache_scope().into(),
        fetcher.clone(),
    );
    let mut assets = CatalogueAvatarAssets::default();
    assets.prefetch(&request, &cache);
    let deadline = Instant::now() + Duration::from_secs(3);
    while !fetcher.started.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assets.clear();
    cache.cancel_pending();
    cache.shutdown();
    assert!(fetcher.cancelled.load(Ordering::SeqCst));
    assert!(assets
        .image(&source, &workspace.frame().unwrap().rows[0])
        .is_none());
    assert!(!fixture.root.join("assets").exists());
}

#[test]
fn repo_catalog_avatar_actual_public_curl_isolated_argv_environment_and_original_url() {
    use std::os::unix::fs::PermissionsExt;
    let (fixture, source, workspace) = actual(42, 1, false, false);
    let request = CatalogueAvatarRequest::from_workspace(&workspace, false).unwrap();
    let file = fixture.root.join("asset.png");
    std::fs::write(&file, png(true)).unwrap();
    let program = fixture.root.join("bin/fixture-curl");
    let argv = fixture.root.join("asset-argv");
    let env = fixture.root.join("asset-env");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n/usr/bin/env > '{}'\n/bin/cat '{}'\n",
            argv.display(),
            env.display(),
            file.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cache = AvatarCache::new(
        fixture.root.join("assets"),
        request.cache_scope().into(),
        Arc::new(CurlAvatarFetcher::with_program(program, Default::default())),
    );
    let mut assets = CatalogueAvatarAssets::default();
    ready(&mut assets, &request, &cache);
    let args = std::fs::read_to_string(argv).unwrap();
    assert!(args.starts_with("-q\n--fail\n"));
    assert!(args.contains("--proto\n=https\n--max-redirs\n0"));
    assert!(args.ends_with(&format!("--url\n{URL}\n")));
    let env = std::fs::read_to_string(env).unwrap();
    assert!(!env.lines().any(|line| [
        "GH_TOKEN=",
        "GITHUB_TOKEN=",
        "GH_HOST=",
        "CURL_HOME=",
        "HTTPS_PROXY="
    ]
    .iter()
    .any(|key| line.starts_with(key))));
    assert!(assets
        .image(&source, &workspace.frame().unwrap().rows[0])
        .is_some());
    cache.shutdown();
}

fn admitted_app() -> (Fixture, App) {
    let fixture = Fixture::new();
    let account = || {
        vec![
            GhStep::read(&["--version"], "gh version 2.99.0 (2026-10-01)\n"),
            get("user", json!({"id":42,"login":"octocat"})),
        ]
    };
    let owner = json!({"id":71,"node_id":"OWNER_bound71","login":"example","type":"Organization","name":"Example Org","avatar_url":URL});
    let mut steps = account();
    steps.push(get("orgs/example", owner.clone()));
    steps.push(get("orgs/example/repos?type=all&sort=full_name&direction=asc&per_page=100&page=1",json!([{"id":10000,"node_id":"R_bound10000","full_name":"example/repo0000","owner":owner,"permissions":{"pull":true}}])));
    steps.push(get(
        "orgs/example",
        json!({"id":71,"node_id":"OWNER_bound71","login":"example","type":"Organization"}),
    ));
    steps.extend(account());
    fixture.scripted_gh(&steps);
    let provider = GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"));
    let mut storage = crate::storage::Cache::open(&fixture.root.join("cache.sqlite3")).unwrap();
    let guard = Arc::new(
        crate::repository_catalogue_save_v1::load_catalogue_intent_source(
            &provider,
            &mut storage,
            None,
            &RepositoryCatalogueSource::Organization("example".into()),
            &RepositoryCatalogueLimits::default(),
            &Default::default(),
        )
        .unwrap(),
    );
    let (mut workspace, job) = CatalogueWorkspace::new(guard.source().clone()).unwrap();
    workspace
        .accept(job.prepare(&Default::default()).unwrap())
        .unwrap();
    draw(&mut workspace, None, ColorMode::TrueColor);
    let mut app = App {
        settings_host: "github.invalid".into(),
        ..App::default()
    };
    crate::repository_catalogue_native::dispatch(
        &mut app,
        crate::repository_catalogue_native::CatalogueUiAction::Open,
    );
    let modal = app.catalogue.as_mut().unwrap();
    modal.guard = Some(guard);
    modal.workspace = Some(workspace);
    (fixture, app)
}

#[test]
fn repo_catalog_avatar_real_guard_request_suppresses_hidden_surface_busy_fence_and_no_color() {
    let (fixture, mut app) = admitted_app();
    assert!(request(&app).unwrap().requested_rows() > 0);
    let journal = std::fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    let mut cache = None;
    let mut active = false;
    assert!(poll_catalogue(
        &mut app,
        &mut cache,
        &fixture.root.join("assets"),
        false,
        &mut active
    ));
    assert!(cache.is_none());
    assert!(!active);
    app.catalogue.as_mut().unwrap().busy = true;
    assert!(request(&app).is_none());
    assert!(poll_catalogue(
        &mut app,
        &mut cache,
        &fixture.root.join("assets"),
        true,
        &mut active
    ));
    assert!(cache.is_none());
    let modal = app.catalogue.as_mut().unwrap();
    modal.busy = false;
    modal.fenced = true;
    assert!(request(&app).is_none());
    assert!(poll_catalogue(
        &mut app,
        &mut cache,
        &fixture.root.join("assets"),
        true,
        &mut active
    ));
    assert!(cache.is_none());
    assert_eq!(
        std::fs::read(fixture.root.join("gh-fixture/commands.jsonl")).unwrap(),
        journal
    );
}

#[test]
fn repo_catalog_avatar_actual_guard_rejects_different_read_even_same_ids_and_login() {
    let (_fixture, mut app) = admitted_app();
    let (_other, _source, other_workspace) = actual(42, 1, false, false);
    app.catalogue.as_mut().unwrap().workspace = Some(other_workspace);
    assert!(request(&app).is_none());
}
