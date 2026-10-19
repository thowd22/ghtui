use ghtui::{
    config::ThemeName,
    git::GitActor,
    identity::{
        ActorIdentity, ActorKind, ActorWidget, AssetPolicy, AvatarCache, AvatarError,
        AvatarFetcher, AvatarImage, AvatarState, AvatarWidget, CommitAssociation,
        CurlAvatarFetcher, GitRole, MAX_AVATAR_BYTES,
    },
    providers::{gh::Actor, runner::CancellationToken},
    theme::{ColorMode, SemanticToken, Theme},
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use ratatui::{
    buffer::Buffer,
    layout::Rect,
    style::{Color, Modifier},
    widgets::Widget,
};
use serde_json::json;
#[path = "support/fixture_shell_publication_v1.rs"]
mod fixture_shell_publication_v1;
#[path = "git_support/mod.rs"]
mod git_support;
use std::{
    io::Cursor,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

fn actor() -> ActorIdentity {
    ActorIdentity::github(
        "github.com",
        &Actor {
            login: "alice".into(),
            display_name: Some("Alice Example".into()),
            avatar_url: Some("https://avatars.githubusercontent.com/u/1?v=4".into()),
            actor_type: "User".into(),
        },
    )
}
fn png(width: u32, height: u32) -> Vec<u8> {
    let mut image = RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = if y == 0 {
            Rgba([255, 0, 0, 255])
        } else if x == 0 {
            Rgba([0, 0, 255, 255])
        } else {
            Rgba([0, 255, 0, 255])
        };
    }
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
fn buffer_text(buffer: &Buffer) -> String {
    buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn artifacts_bounded_image_preview_native_pixels_does_not_widen_avatar_limits() {
    use ghtui::identity::{ImageDisplay, ImageWidget};
    let larger = png(513, 2);
    assert!(
        AvatarImage::decode(&larger).is_err(),
        "Public avatar limits remain smaller"
    );
    let display = ImageDisplay::decode(&larger).unwrap();
    assert_eq!((display.original_width, display.original_height), (513, 2));
    assert!(display.raster.width <= 64 && display.raster.height <= 64);
    assert_eq!(
        display.raster.pixels.len(),
        display.raster.width as usize * display.raster.height as usize
    );
    assert!(ImageDisplay::decode(&png(4097, 1)).is_err());
    assert!(ImageDisplay::decode(&vec![0; 256 * 1024 + 1]).is_err());
    assert!(ImageDisplay::decode(b"GIF89a\x01\x00\x01\x00").is_err());
    let mut corrupt = png(2, 2);
    corrupt.truncate(corrupt.len() / 2);
    assert!(ImageDisplay::decode(&corrupt).is_err());
    let mut jpeg = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(2, 2, image::Rgb([255, 0, 0])))
        .write_to(&mut jpeg, image::ImageFormat::Jpeg)
        .unwrap();
    assert_eq!(
        ImageDisplay::decode(&jpeg.into_inner()).unwrap().format,
        "JPEG"
    );
    let display = ImageDisplay::decode(&png(2, 2)).unwrap();
    let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 2));
    ImageWidget {
        image: &display,
        theme: Theme::new(ThemeName::CatppuccinMocha, ColorMode::TrueColor),
    }
    .render(buffer.area, &mut buffer);
    assert_eq!(buffer[(0, 0)].symbol(), "▀");
    assert_eq!(buffer[(0, 0)].fg, Color::Rgb(255, 0, 0));
    assert_eq!(buffer[(0, 0)].bg, Color::Rgb(0, 0, 255));
    assert_eq!(
        buffer[(2, 0)].symbol(),
        " ",
        "Raster is not stretched into unrelated cells"
    );
    for mode in [ColorMode::LowColor, ColorMode::NoColor] {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 80, 2));
        ImageWidget {
            image: &display,
            theme: Theme::new(ThemeName::CatppuccinMocha, mode),
        }
        .render(buffer.area, &mut buffer);
        assert!(buffer_text(&buffer).contains("PNG image 2x2"));
        assert!(buffer_text(&buffer).contains("truecolor terminal"));
        assert!(buffer
            .content
            .iter()
            .all(|cell| !matches!(cell.fg, Color::Rgb(..)) && !matches!(cell.bg, Color::Rgb(..))));
    }
}

#[test]
fn actor_identity_gh_bot_deleted_unknown_and_honest_git_roles() {
    let identity = actor();
    assert_eq!(identity.label(), "@alice");
    assert_eq!(identity.initials(), "AE");
    let bot = ActorIdentity::github(
        "github.com",
        &Actor {
            login: "build[bot]".into(),
            actor_type: "Bot".into(),
            ..Actor::default()
        },
    );
    assert_eq!(bot.kind, ActorKind::Bot);
    assert_eq!(bot.label(), "@build[bot] [bot]");
    let deleted = ActorIdentity::github(
        "github.com",
        &Actor {
            login: "ghost".into(),
            actor_type: "User".into(),
            ..Actor::default()
        },
    );
    assert_eq!(deleted.kind, ActorKind::Deleted);
    assert_eq!(deleted.initials(), "×");
    let unknown = ActorIdentity::github("github.com", &Actor::default());
    assert_eq!(unknown.initials(), "?");
    assert!(unknown.label().contains("unknown"));
    let unknown_type = ActorIdentity::github(
        "github.com",
        &Actor {
            login: "known-login".into(),
            actor_type: "Unknown".into(),
            ..Actor::default()
        },
    );
    assert_eq!(unknown_type.kind, ActorKind::Unknown);
    assert_eq!(unknown_type.label(), "@known-login [unknown]");
    let local = GitActor {
        name: "Alice Example".into(),
        email: "local@example.test".into(),
        timestamp: 0,
    };
    let author = ActorIdentity::local(&local, GitRole::Author);
    let committer = ActorIdentity::local(&local, GitRole::Committer);
    assert_eq!(author.login, None);
    assert!(!author.host_associated);
    assert!(author.label().starts_with("author Alice"));
    assert!(committer.label().starts_with("committer Alice"));
}

#[test]
fn semantic_colors_canonical_range_selection_and_hunk_keep_narrow_override_notice_visible() {
    use ghtui::{
        app::{Action, App, DiffSelection},
        workspace::{RepositoryMode, RowAction, Workspace},
    };
    use ratatui::{backend::TestBackend, Terminal};
    let fixture = git_support::Fixture::linear();
    fixture.write("readme.txt", b"first\nthird\nfourth\n");
    let mut workspace = Workspace::load(&fixture.root, RepositoryMode::Working, "HEAD", None)
        .unwrap()
        .unwrap();
    workspace.status.replacement_refs = 2;
    workspace.status.grafted = true;
    let anchors = workspace
        .rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| {
            if let Some(RowAction::DiffLine(anchor)) = &row.action {
                Some((index, anchor.clone(), row.role, row.label.clone()))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert!(anchors.len() >= 2);
    let mut app = App::new();
    app.view = ghtui::app::View::Repository;
    app.workspace = Some(workspace);
    app.repository_selection = Some(DiffSelection {
        start: anchors[0].1.clone(),
        end: anchors.last().unwrap().1.clone(),
    });
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            // The pane above, and the main key bar on the bottom row as the frame paints it.
            let (pane, bar) = (
                ratatui::layout::Rect::new(area.x, area.y, area.width, area.height - 1),
                ratatui::layout::Rect::new(area.x, area.bottom() - 1, area.width, 1),
            );
            ghtui::repository_ui::draw(frame, &mut app, pane);
            let controls = ghtui::footer::main_controls(&app);
            ghtui::footer::draw(frame, &mut app, bar, &controls);
        })
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    let bar = (0..80)
        .map(|column| terminal.backend().buffer()[(column, 23)].symbol())
        .collect::<String>();
    assert!(
        bar.contains("h Hunk"),
        "The Hunk key is on the key bar: {bar}"
    );
    assert!(text.contains("Original snapshots • replacements: 2 • legacy grafts: present"));
    let hunk = app
        .hits
        .iter()
        .find(|hit| hit.action == Action::RepositoryHunk)
        .unwrap();
    assert!(hunk.rect.right() <= 80);
    for (_, anchor, role, label) in anchors {
        let observed = (0..24)
            .find(|row| {
                (0..80)
                    .map(|column| terminal.backend().buffer()[(column, *row)].symbol())
                    .collect::<String>()
                    .trim_end()
                    == label.trim_end()
            })
            .expect("Every canonical selected diff row must remain visible");
        let hit = app
            .hits
            .iter()
            .find(|hit| hit.action == Action::RepositoryRow(RowAction::DiffLine(anchor.clone())))
            .expect("The exact canonical selected row must remain mouse-accessible");
        assert_eq!(
            hit.rect.y, observed,
            "The canonical mouse target must match the independently observed diff row"
        );
        let expected = app.theme().selected(SemanticToken::from(role));
        let cell = &terminal.backend().buffer()[(0, observed)];
        assert!(cell.modifier.contains(Modifier::REVERSED | Modifier::BOLD));
        assert_eq!(cell.fg, expected.fg.unwrap_or(Color::Reset));
    }
}

#[test]
fn actor_identity_exact_commit_role_association_does_not_infer_from_names() {
    let sha = "a".repeat(40);
    let local = GitActor {
        name: "Git Name".into(),
        email: "git@example.test".into(),
        timestamp: 0,
    };
    let response = json!({"sha":sha,"commit":{"author":{"name":"Git Name","email":"git@example.test"},"committer":{"name":"Different Name","email":"committer@example.test"}},"author":{"login":"remote-login","type":"User","avatar_url":"https://avatars.githubusercontent.com/u/2"},"committer":{"login":"other-login","type":"User"}});
    let association = CommitAssociation::from_response(
        "github.com",
        "org/repo",
        &sha,
        GitRole::Author,
        &response,
    )
    .unwrap();
    let author = ActorIdentity::local(&local, GitRole::Author)
        .associate(&sha, &association)
        .unwrap();
    assert_eq!(author.login.as_deref(), Some("remote-login"));
    assert!(author.label().contains("Git Name <git@example.test>"));
    assert!(ActorIdentity::local(&local, GitRole::Committer)
        .associate(&sha, &association)
        .is_err());
    assert!(ActorIdentity::local(&local, GitRole::Author)
        .associate(&"b".repeat(40), &association)
        .is_err());
    assert!(CommitAssociation::from_response(
        "github.com",
        "org/repo",
        &"b".repeat(40),
        GitRole::Author,
        &response
    )
    .is_err());
    let no_association = json!({"sha":sha,"commit":{"author":{"name":"Git Name","email":"git@example.test"}},"author":null});
    assert!(CommitAssociation::from_response(
        "github.com",
        "org/repo",
        &sha,
        GitRole::Author,
        &no_association
    )
    .is_err());
}

#[test]
fn actor_identity_avatar_decoding_bounds_and_actual_raster_cells() {
    let image = AvatarImage::decode(&png(2, 2)).unwrap();
    assert_eq!(image.width, 2);
    assert_eq!(image.height, 2);
    assert_eq!(image.pixels[0], [255, 0, 0, 255]);
    assert!(AvatarImage::decode(b"bad PNG").is_err());
    assert!(AvatarImage::decode(&vec![0; MAX_AVATAR_BYTES + 1]).is_err());
    assert!(AvatarImage::decode(&png(513, 1)).is_err());
    let big = AvatarImage::decode(&png(128, 128)).unwrap();
    assert_eq!((big.width, big.height), (64, 64));
    let identity = actor();
    let mut buffer = Buffer::empty(Rect::new(0, 0, 8, 3));
    AvatarWidget {
        identity: &identity,
        image: Some(&image),
        theme: Theme::new(ThemeName::CatppuccinMocha, ColorMode::TrueColor),
    }
    .render(Rect::new(1, 1, 2, 1), &mut buffer);
    assert_eq!(buffer[(1, 1)].symbol(), "▀");
    assert_eq!(buffer[(1, 1)].fg, Color::Rgb(255, 0, 0));
    assert_eq!(buffer[(1, 1)].bg, Color::Rgb(0, 0, 255));
    assert_eq!(buffer[(0, 1)].symbol(), " ");
    assert_eq!(buffer[(3, 1)].symbol(), " ");
    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 2));
    ActorWidget {
        identity: &identity,
        image: Some(&image),
        theme: Theme::new(ThemeName::NoColor, ColorMode::NoColor),
    }
    .render(buffer.area, &mut buffer);
    assert!(buffer_text(&buffer).contains("[AE]"));
    assert!(buffer_text(&buffer).contains("@alice"));
    assert!(buffer_text(&buffer).contains("Alice Example"));
    assert!(!buffer_text(&buffer).contains('▀'));
    let mut buffer = Buffer::empty(Rect::new(0, 0, 40, 2));
    ActorWidget {
        identity: &identity,
        image: Some(&image),
        theme: Theme::new(ThemeName::CatppuccinMocha, ColorMode::LowColor),
    }
    .render(buffer.area, &mut buffer);
    assert!(buffer_text(&buffer).contains("[AE]"));
    assert!(buffer_text(&buffer).contains("@alice"));
    assert!(buffer
        .content
        .iter()
        .all(|cell| !matches!(cell.fg, Color::Rgb(..)) && !matches!(cell.bg, Color::Rgb(..))));
}

#[test]
fn actor_identity_avatar_urls_reject_private_auth_and_redirect_targets() {
    let policy = AssetPolicy::default();
    assert!(policy
        .validate("https://avatars.githubusercontent.com/u/1?v=4&s=80")
        .is_ok());
    for url in [
        "http://avatars.githubusercontent.com/u/1",
        "https://user:secret@avatars.githubusercontent.com/u/1",
        "https://avatars.githubusercontent.com.evil.test/u/1",
        "https://127.0.0.1/avatar",
        "https://avatars.githubusercontent.com:443/u/1",
        "https://avatars.githubusercontent.com/u/1?access_token=secret",
        "https://avatars.githubusercontent.com/u/1#fragment",
        "https://avatars.githubusercontent.com/\nheader",
    ] {
        assert!(policy.validate(url).is_err(), "Must reject {url}");
    }
}

struct FixtureFetcher {
    bytes: Vec<u8>,
    calls: AtomicUsize,
    blocked: Mutex<bool>,
    started: Condvar,
    released: Condvar,
}
impl FixtureFetcher {
    fn new(blocked: bool) -> Self {
        Self {
            bytes: png(2, 2),
            calls: AtomicUsize::new(0),
            blocked: Mutex::new(blocked),
            started: Condvar::new(),
            released: Condvar::new(),
        }
    }
    fn release(&self) {
        *self.blocked.lock().unwrap() = false;
        self.released.notify_all();
    }
    fn wait_started(&self, count: usize) {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut lock = self.blocked.lock().unwrap();
        while self.calls.load(Ordering::SeqCst) < count {
            assert!(Instant::now() < deadline, "Avatar worker must start");
            lock = self
                .started
                .wait_timeout(lock, Duration::from_millis(10))
                .unwrap()
                .0;
        }
    }
}
impl AvatarFetcher for FixtureFetcher {
    fn fetch(&self, _: &str, cancellation: &CancellationToken) -> Result<Vec<u8>, AvatarError> {
        let mut blocked = self.blocked.lock().unwrap();
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.notify_all();
        while *blocked {
            if cancellation.is_cancelled() {
                return Err(AvatarError("Fixture cancelled".into()));
            }
            blocked = self
                .released
                .wait_timeout(blocked, Duration::from_millis(10))
                .unwrap()
                .0;
        }
        Ok(self.bytes.clone())
    }
}
fn ready(cache: &AvatarCache, identity: &ActorIdentity) -> Arc<AvatarImage> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match cache.request(identity) {
            AvatarState::Ready(image) => return image,
            AvatarState::Failed(error) => panic!("Unexpected avatar failure: {error}"),
            _ => {
                assert!(Instant::now() < deadline, "Avatar ready deadline");
                std::thread::yield_now();
            }
        }
    }
}

#[test]
fn actor_identity_avatar_background_dedup_restart_account_partition_and_fallback() {
    let directory = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(FixtureFetcher::new(true));
    let cache = AvatarCache::new(
        directory.path().into(),
        "github.com/alice".into(),
        fetcher.clone(),
    );
    let identity = actor();
    assert!(matches!(cache.request(&identity), AvatarState::Loading));
    fetcher.wait_started(1);
    for _ in 0..20 {
        assert!(matches!(cache.request(&identity), AvatarState::Loading));
    }
    let mut buffer = Buffer::empty(Rect::new(0, 0, 30, 2));
    ActorWidget {
        identity: &identity,
        image: None,
        theme: Theme::new(ThemeName::CatppuccinMocha, ColorMode::TrueColor),
    }
    .render(buffer.area, &mut buffer);
    assert!(buffer_text(&buffer).contains("[AE]"));
    assert!(buffer_text(&buffer).contains("@alice"));
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    fetcher.release();
    let image = ready(&cache, &identity);
    assert_eq!(image.pixels[0], [255, 0, 0, 255]);
    let restarted_fetcher = Arc::new(FixtureFetcher::new(false));
    let restarted = AvatarCache::new(
        directory.path().into(),
        "github.com/alice".into(),
        restarted_fetcher.clone(),
    );
    ready(&restarted, &identity);
    assert_eq!(
        restarted_fetcher.calls.load(Ordering::SeqCst),
        0,
        "Same account reuses persisted real pixels"
    );
    let other = AvatarCache::new(
        directory.path().into(),
        "github.com/bob".into(),
        restarted_fetcher.clone(),
    );
    ready(&other, &identity);
    assert_eq!(
        restarted_fetcher.calls.load(Ordering::SeqCst),
        1,
        "Another account gets separate provenance"
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 2);
}

#[test]
fn actor_identity_avatar_workers_are_bounded_and_cancelled() {
    let directory = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(FixtureFetcher::new(true));
    let cache = AvatarCache::new(directory.path().into(), "scope".into(), fetcher.clone());
    let mut identities = Vec::new();
    for index in 0..5 {
        let mut identity = actor();
        identity.login = Some(format!("actor{index}"));
        let state = cache.request(&identity);
        assert!(if index < 4 {
            matches!(state, AvatarState::Loading)
        } else {
            matches!(state, AvatarState::Busy)
        });
        identities.push(identity);
    }
    fetcher.wait_started(4);
    cache.cancel_pending();
    let deadline = Instant::now() + Duration::from_secs(5);
    for identity in identities.iter().take(4) {
        loop {
            if let AvatarState::Failed(message) = cache.request(identity) {
                assert!(message.contains("cancelled"));
                break;
            }
            assert!(Instant::now() < deadline, "Cancelled worker must exit");
            std::thread::yield_now();
        }
    }
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 4);
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn actor_identity_avatar_corrupt_cache_refetches_and_disk_eviction_preserves_other_files() {
    let directory = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(FixtureFetcher::new(false));
    let identity = actor();
    let cache = AvatarCache::new(directory.path().into(), "scope".into(), fetcher.clone());
    ready(&cache, &identity);
    let path = std::fs::read_dir(directory.path())
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let mut corrupted: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    corrupted["image"]["width"] = json!(50000);
    std::fs::write(&path, serde_json::to_vec(&corrupted).unwrap()).unwrap();
    let fresh = AvatarCache::new(directory.path().into(), "scope".into(), fetcher.clone());
    assert_eq!(ready(&fresh, &identity).width, 2);
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 2);
    std::fs::write(directory.path().join("user-data.txt"), b"must preserve").unwrap();
    std::fs::write(
        directory.path().join("avatar-0000000000000000.json"),
        b"not an owned valid cache record",
    )
    .unwrap();
    for index in 0..130 {
        let mut identity = actor();
        identity.login = Some(format!("extra{index}"));
        ready(&fresh, &identity);
    }
    assert_eq!(
        std::fs::read(directory.path().join("user-data.txt")).unwrap(),
        b"must preserve"
    );
    assert_eq!(
        std::fs::read(directory.path().join("avatar-0000000000000000.json")).unwrap(),
        b"not an owned valid cache record"
    );
    assert_eq!(
        std::fs::read_dir(directory.path()).unwrap().count(),
        130,
        "128 validated avatar records and two preserved unrelated files"
    );
}

#[test]
fn actor_identity_curl_transport_exact_argv_no_auth_env_or_redirect_follow() {
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("fixture-curl");
    let source = directory.path().join("image.png");
    let journal = directory.path().join("argv");
    let environment = directory.path().join("environment");
    std::fs::write(&source, png(2, 2)).unwrap();
    let script = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nenv > '{}'\ncat '{}'\n",
        journal.display(),
        environment.display(),
        source.display()
    );
    fixture_shell_publication_v1::install(&program, script.as_bytes()).unwrap();
    let fetcher = CurlAvatarFetcher::with_program(program, AssetPolicy::default());
    let bytes = fetcher
        .fetch(
            "https://avatars.githubusercontent.com/u/1?v=4",
            &CancellationToken::default(),
        )
        .unwrap();
    AvatarImage::decode(&bytes).unwrap();
    let args = std::fs::read_to_string(journal).unwrap();
    assert_eq!(
        args.lines().collect::<Vec<_>>(),
        vec![
            "-q",
            "--fail",
            "--silent",
            "--show-error",
            "--proto",
            "=https",
            "--max-redirs",
            "0",
            "--connect-timeout",
            "3",
            "--max-time",
            "6",
            "--max-filesize",
            "1048576",
            "--header",
            "Accept: image/png,image/jpeg",
            "--url",
            "https://avatars.githubusercontent.com/u/1?v=4"
        ]
    );
    assert!(!args.contains("--location"));
    assert!(!args.contains("Authorization"));
    let env = std::fs::read_to_string(environment).unwrap();
    for prohibited in [
        "GH_TOKEN=",
        "GITHUB_TOKEN=",
        "HOME=",
        "CURL_HOME=",
        "HTTP_PROXY=",
        "HTTPS_PROXY=",
    ] {
        assert!(
            !env.contains(prohibited),
            "No auth/config/proxy inheritance"
        );
    }
    assert!(fetcher
        .fetch(
            "https://private.invalid/avatar",
            &CancellationToken::default()
        )
        .is_err());
}

#[test]
fn actor_identity_shutdown_cancels_and_reaps_actual_image_process() {
    let directory = tempfile::tempdir().unwrap();
    let program = directory.path().join("fixture-curl");
    let pid_path = directory.path().join("child-pid");
    let script = format!(
        "#!/bin/sh\nprintf '%s' \"$$\" > '{}'\nexec /bin/sleep 30\n",
        pid_path.display()
    );
    fixture_shell_publication_v1::install(&program, script.as_bytes()).unwrap();
    let cache = AvatarCache::new(
        directory.path().join("cache"),
        "github.com/alice".into(),
        Arc::new(CurlAvatarFetcher::with_program(
            program,
            AssetPolicy::default(),
        )),
    );
    assert!(matches!(cache.request(&actor()), AvatarState::Loading));
    let deadline = Instant::now() + Duration::from_secs(5);
    let pid = loop {
        if let Ok(value) = std::fs::read_to_string(&pid_path) {
            if let Ok(pid) = value.parse::<i32>() {
                break pid;
            }
        }
        if let AvatarState::Failed(reason) = cache.request(&actor()) {
            panic!("Actual image worker failed before its start marker: {reason}");
        }
        assert!(Instant::now() < deadline, "Actual image worker must start");
        std::thread::yield_now();
    };
    assert!(pid > 0 && pid != std::process::id() as i32);
    let started = Instant::now();
    cache.shutdown();
    assert!(started.elapsed() < Duration::from_secs(2));
    // Signal zero only observes process existence; it never mutates another process.
    let exists = unsafe { libc::kill(pid, 0) };
    assert_eq!(
        exists, -1,
        "Image process must be reaped before shutdown returns"
    );
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
    assert!(matches!(cache.request(&actor()), AvatarState::Failed(_)));
    assert!(!directory.path().join("cache").exists());
    cache.shutdown();
}

#[test]
fn actor_identity_panicked_worker_has_honest_failure_and_drains() {
    struct PanickedFetcher;
    impl AvatarFetcher for PanickedFetcher {
        fn fetch(&self, _: &str, _: &CancellationToken) -> Result<Vec<u8>, AvatarError> {
            panic!("fixture avatar decoder transport panic")
        }
    }
    let directory = tempfile::tempdir().unwrap();
    let cache = AvatarCache::new(
        directory.path().into(),
        "scope".into(),
        Arc::new(PanickedFetcher),
    );
    assert!(matches!(cache.request(&actor()), AvatarState::Loading));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let AvatarState::Failed(error) = cache.request(&actor()) {
            assert!(error.contains("worker failed"));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Panic must not leave perpetual Loading"
        );
        std::thread::yield_now();
    }
    cache.shutdown();
}

#[test]
fn semantic_colors_terminal_capabilities_are_explicit_and_respect_monochrome() {
    assert_eq!(
        ColorMode::for_terminal(Some("xterm-256color"), Some("truecolor"), false),
        ColorMode::TrueColor
    );
    assert_eq!(
        ColorMode::for_terminal(Some("screen"), Some("24BIT"), false),
        ColorMode::TrueColor
    );
    assert_eq!(
        ColorMode::for_terminal(Some("xterm-direct"), None, false),
        ColorMode::TrueColor
    );
    assert_eq!(
        ColorMode::for_terminal(Some("screen-256color"), None, false),
        ColorMode::Indexed256
    );
    assert_eq!(
        ColorMode::for_terminal(None, None, false),
        ColorMode::LowColor
    );
    assert_eq!(
        ColorMode::for_terminal(Some("dumb"), Some("truecolor"), false),
        ColorMode::NoColor
    );
    assert_eq!(
        ColorMode::for_terminal(Some("xterm"), Some("truecolor"), true),
        ColorMode::NoColor
    );
    assert_eq!(
        Theme::from_capability(0, ColorMode::LowColor).mode,
        ColorMode::LowColor
    );
    assert_eq!(
        Theme::from_capability(ThemeName::NoColor.index(), ColorMode::TrueColor).mode,
        ColorMode::NoColor
    );
}

#[test]
fn semantic_colors_shared_tokens_selection_no_color_and_graph_legend() {
    for name in ThemeName::ALL {
        for mode in [
            ColorMode::TrueColor,
            ColorMode::LowColor,
            ColorMode::NoColor,
        ] {
            let theme = Theme::new(name, mode);
            for token in [
                SemanticToken::Added,
                SemanticToken::Removed,
                SemanticToken::Changed,
                SemanticToken::Conflict,
                SemanticToken::Head,
                SemanticToken::Reference,
                SemanticToken::Success,
                SemanticToken::Failure,
                SemanticToken::Running,
                SemanticToken::Pending,
                SemanticToken::Cancelled,
            ] {
                assert!(!token.symbol().is_empty());
                assert!(!token.label().is_empty());
                let base = theme.style(token);
                let selected = theme.selected(token);
                assert_eq!(selected.fg, base.fg);
                assert_eq!(selected.bg, base.bg);
                assert!(selected.add_modifier.contains(Modifier::REVERSED));
                if theme.mode == ColorMode::NoColor {
                    assert_eq!(base.fg, None);
                    assert_eq!(base.bg, None);
                }
            }
            assert!(theme
                .legend()
                .iter()
                .any(|row| row.contains("never permanent branch identities")));
            assert!(theme
                .legend()
                .iter()
                .any(|row| row.contains("shallow clone")));
            assert_eq!(theme.graph(2), theme.graph(10));
        }
    }
}

#[test]
fn semantic_colors_true_color_contrast_meets_readable_status_labels() {
    fn luminance(color: Color) -> f64 {
        let (r, g, b) = match color {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Black => (0, 0, 0),
            Color::White => (255, 255, 255),
            _ => panic!("Expected explicit RGB status"),
        };
        let channel = |v: u8| {
            let value = f64::from(v) / 255.0;
            if value <= 0.04045 {
                value / 12.92
            } else {
                ((value + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
    }
    for name in [
        ThemeName::CatppuccinMocha,
        ThemeName::TokyoNight,
        ThemeName::GruvboxDark,
    ] {
        let theme = Theme::new(name, ColorMode::TrueColor);
        for token in [
            SemanticToken::Added,
            SemanticToken::Removed,
            SemanticToken::Changed,
            SemanticToken::Conflict,
            SemanticToken::Head,
            SemanticToken::Reference,
            SemanticToken::Muted,
            SemanticToken::Success,
            SemanticToken::Failure,
            SemanticToken::Running,
            SemanticToken::Pending,
        ] {
            let style = theme.style(token);
            let foreground = luminance(style.fg.unwrap());
            let background = luminance(style.bg.unwrap());
            let ratio = (foreground.max(background) + 0.05) / (foreground.min(background) + 0.05);
            assert!(ratio >= 4.5, "{name:?} {token:?} contrast {ratio}");
        }
    }
}

#[test]
fn semantic_colors_repository_testbackend_preserves_identity_hits_selection_and_monochrome() {
    use ghtui::{
        app::{Action, App, View},
        identity::GitRole,
        repository_ui,
        workspace::{RepositoryMode, RowAction, Workspace},
    };
    use ratatui::{backend::TestBackend, Terminal};
    let fixture = git_support::Fixture::linear();
    let head = fixture.head();
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    let mut app = App::new();
    app.view = View::Repository;
    app.no_color = false;
    app.color_mode = ColorMode::TrueColor;
    app.workspace = Workspace::load(&fixture.root, RepositoryMode::Graph, "HEAD", None).unwrap();
    app.selected = app
        .workspace
        .as_ref()
        .unwrap()
        .graph_focus_row_v1()
        .unwrap();
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| repository_ui::draw(frame, &mut app, frame.area()))
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("author Fixture Author"));
    assert!(text.contains("committer Fixture Author"));
    assert!(text.contains("[FA]"));
    let author = app
        .hits
        .iter()
        .find(|hit| hit.action == Action::RepositoryRow(RowAction::Actor(GitRole::Author)))
        .unwrap();
    assert_eq!(author.rect.y, 1);
    let selected_hit = app
        .hits
        .iter()
        .find(|hit| hit.action == Action::RepositoryRow(RowAction::Commit(head.clone())))
        .unwrap();
    let selected =
        terminal.backend().buffer()[(selected_hit.rect.x + 4, selected_hit.rect.y)].modifier;
    assert!(selected.contains(Modifier::REVERSED));
    // The graph Legend is a key-bar control: its key opens the graph legend, and where the
    // bar paints its token that token is the same action's click target.
    let legend = Action::RepositoryRow(RowAction::Graph(
        ghtui::repository_graph_v1::GraphActionV1::Legend,
    ));
    let controls = ghtui::footer::main_controls(&app);
    assert_eq!(
        ghtui::footer::resolve_key(
            &controls,
            crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('l'),
                crossterm::event::KeyModifiers::NONE
            )
        ),
        Some(legend.clone())
    );
    let painted = ghtui::footer::layout(&controls, 120)
        .into_iter()
        .any(|placed| controls[placed.index].action == legend);
    terminal
        .draw(|frame| {
            let area = frame.area();
            let bar = ratatui::layout::Rect::new(area.x, area.bottom() - 1, area.width, 1);
            ghtui::footer::draw(frame, &mut app, bar, &controls);
        })
        .unwrap();
    assert_eq!(
        app.hits.iter().any(|hit| hit.action == legend),
        painted,
        "A painted Legend token is clickable; an unpainted one has no stale target"
    );
    app.settings_theme = ThemeName::NoColor.index();
    app.no_color = true;
    app.hits.clear();
    terminal
        .draw(|frame| repository_ui::draw(frame, &mut app, frame.area()))
        .unwrap();
    assert!(terminal
        .backend()
        .buffer()
        .content
        .iter()
        .all(|cell| matches!(cell.fg, Color::Reset) && matches!(cell.bg, Color::Reset)));
    assert!(buffer_text(terminal.backend().buffer()).contains("[FA]"));
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
}

#[test]
fn git_provider_repository_full_saved_identity_narrow_layout_retains_override_notice_and_exact_hits(
) {
    use ghtui::{
        app::{Action, App, View},
        workspace::{RepositoryMode, Workspace},
    };
    use ratatui::{backend::TestBackend, Terminal};
    let fixture = git_support::Fixture::linear();
    let original = Workspace::load(&fixture.root, RepositoryMode::Graph, "HEAD", None)
        .unwrap()
        .unwrap();
    let head = fixture.head();
    let index = std::fs::read(fixture.repository.git_dir.join("index")).unwrap();
    for width in [80, 120] {
        for saved in [head.clone(), "a".repeat(64)] {
            for override_count in [0, 2] {
                let mut workspace = original.clone();
                workspace.revision.clone_from(&saved);
                workspace
                    .selected_commit
                    .as_mut()
                    .unwrap()
                    .id
                    .clone_from(&saved);
                workspace.status.replacement_refs = override_count;
                workspace.status.grafted = override_count != 0;
                let genuine = saved == head;
                let rows = workspace.display_rows_v1().to_vec();
                if genuine {
                    assert!(
                        !rows.is_empty(),
                        "Actual immutable Graph rows must be exercised"
                    );
                } else {
                    assert!(
                        !workspace.graph_visible_v1(),
                        "A rendering-only changed SHA is not a genuine Graph member"
                    );
                    assert!(rows.is_empty());
                }
                let mut app = App::new();
                app.view = View::Repository;
                app.context = format!(
                    "Local repository: {} • selected {saved}",
                    "deep/".repeat(40)
                );
                app.workspace = Some(workspace);
                if genuine {
                    app.selected = app
                        .workspace
                        .as_ref()
                        .unwrap()
                        .graph_focus_row_v1()
                        .unwrap();
                    app.scroll = app.selected;
                }
                let mut terminal = Terminal::new(TestBackend::new(width, 24)).unwrap();
                terminal
                    .draw(|frame| ghtui::ui::draw(frame, &mut app))
                    .unwrap();
                let buffer = terminal.backend().buffer();
                let row = |y| {
                    (0..width)
                        .map(|x| buffer[(x, y)].symbol())
                        .collect::<String>()
                };
                // The merged header and rounded content frame place the identity line below the
                // repository controls; it is located by its complete text, never a fixed row.
                let saved_row = (0..24)
                    .find(|y| row(*y).contains(&format!("Saved: {saved}")))
                    .expect(
                        "The complete immutable identity fits independently of the long breadcrumb",
                    );
                assert!(
                    (0..24).all(|y| !row(y).contains(&"deep/".repeat(40))),
                    "The long breadcrumb is bounded rather than painted in full"
                );
                if override_count != 0 {
                    assert!(row(saved_row + 1)
                        .contains("Original snapshots • replacements: 2 • legacy grafts: present"));
                }
                let content_start = rows.get(app.scroll).map(|first| {
                    let prefix = first.label.chars().take(24).collect::<String>();
                    (0..24)
                        .find(|y| row(*y).contains(&prefix))
                        .expect("The actual first borrowed Graph row must be visibly painted")
                });
                if let Some(start) = content_start {
                    assert!(
                        start > saved_row + u16::from(override_count != 0),
                        "Graph rows start below the identity and override notice"
                    );
                }
                let mut verified_actions = 0;
                for (position, prepared) in rows
                    .iter()
                    .enumerate()
                    .skip(app.scroll)
                    .take(app.repository_visible_rows)
                {
                    if let Some(action) = &prepared.action {
                        let hit = app
                            .hits
                            .iter()
                            .find(|hit| {
                                hit.action == Action::RepositoryRow(action.clone())
                                    && hit.rect.y
                                        == content_start.unwrap() + (position - app.scroll) as u16
                            })
                            .unwrap();
                        assert_eq!(
                            hit.rect.y,
                            content_start.unwrap() + (position - app.scroll) as u16,
                            "The exact mouse action follows the visible row after header insertion"
                        );
                        assert!(
                            hit.rect.width > 0
                                && hit.rect.right() <= width
                                && hit.rect.bottom() <= 24
                        );
                        verified_actions += 1;
                    }
                }
                if genuine {
                    assert!(
                        verified_actions > 0,
                        "At least one actual borrowed Graph action must be independently checked"
                    );
                }
            }
        }
    }
    assert_eq!(fixture.head(), head);
    assert_eq!(
        std::fs::read(fixture.repository.git_dir.join("index")).unwrap(),
        index
    );
}

#[test]
fn semantic_colors_repository_clears_loading_shell_glyphs_before_controls_and_content() {
    use ghtui::{
        app::{Action, App},
        workspace::{RepositoryMode, Workspace},
    };
    use ratatui::{backend::TestBackend, widgets::Paragraph, Terminal};
    let fixture = git_support::Fixture::linear();
    let mut app = App::new();
    app.view = ghtui::app::View::Repository;
    app.workspace = Workspace::load(&fixture.root, RepositoryMode::Overview, "HEAD", None).unwrap();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(
                Paragraph::new(
                    std::iter::repeat_n("Z".repeat(80), 24)
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                frame.area(),
            );
        })
        .unwrap();
    assert!(
        buffer_text(terminal.backend().buffer()).contains(&"Z".repeat(80)),
        "Negative control: generic shell really fills the pane"
    );
    terminal
        .draw(|frame| {
            // Reproduce layering in the same frame, not merely old terminal buffer cells.
            frame.render_widget(
                Paragraph::new(
                    std::iter::repeat_n("Z".repeat(80), 24)
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                frame.area(),
            );
            // The pane clears all of its own rows; the key bar then takes the bottom row.
            let area = frame.area();
            ghtui::repository_ui::draw(frame, &mut app, area);
            let bar = ratatui::layout::Rect::new(area.x, area.bottom() - 1, area.width, 1);
            let controls = ghtui::footer::main_controls(&app);
            ghtui::footer::draw(frame, &mut app, bar, &controls);
        })
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(
        !text.contains('Z'),
        "No underlying shell symbols may survive between/after prepared rows"
    );
    let row = |row: u16| {
        (0..80)
            .map(|column| terminal.backend().buffer()[(column, row)].symbol())
            .collect::<String>()
    };
    // The mode tabs lead the pane; Compare is a key-bar token on the bottom row.
    assert!(row(0).contains("o Overview"), "{}", row(0));
    assert!(row(23).contains("C Compare"), "{}", row(23));
    assert!(text.contains("Recent commits"));
    assert!(app.hits.iter().any(|hit| hit.action == Action::Compare));
    assert!(app
        .hits
        .iter()
        .any(|hit| matches!(hit.action, Action::RepositoryMode(RepositoryMode::Graph))));
}
