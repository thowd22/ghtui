//! Exact-actor contracts. HostIdentity here is cache/display input,
//! not fabricated API permission proof; real public bytes use the existing cache.
use super::*;
use crate::{
    app::{App, View},
    config::ThemeName,
    git::GitActor,
    identity::{ActorKind, AvatarError, AvatarFetcher, AvatarWidget, CurlAvatarFetcher, GitRole},
    providers::runner::CancellationToken,
    theme::{ColorMode, Theme},
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use ratatui::{buffer::Buffer, layout::Rect, style::Color, widgets::Widget};
use std::{
    io::Cursor,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

const URL_A: &str = "https://avatars.githubusercontent.com/u/1?v=4";
const URL_B: &str = "https://avatars.githubusercontent.com/u/2?v=4";
fn caller(uid: Option<u64>) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "viewer".into(),
        account_id: uid,
        gh_version: "gh version 2.99.0 (2026-10-01)".into(),
    }
}
fn actor(url: &str) -> Actor {
    Actor {
        login: "same-login".into(),
        display_name: Some("Actual Observed User".into()),
        avatar_url: Some(url.into()),
        actor_type: "User".into(),
    }
}
fn png(color: [u8; 4]) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba(color)))
        .write_to(&mut bytes, ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}
#[derive(Default)]
struct Fetcher {
    calls: AtomicUsize,
    urls: Mutex<Vec<String>>,
    wait: AtomicBool,
    released: AtomicBool,
    cancelled: AtomicBool,
    fail: AtomicBool,
}
impl AvatarFetcher for Fetcher {
    fn fetch(&self, url: &str, token: &CancellationToken) -> Result<Vec<u8>, AvatarError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.urls.lock().unwrap().push(url.to_owned());
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.wait.load(Ordering::SeqCst) && !self.released.load(Ordering::SeqCst) {
            if token.is_cancelled() {
                self.cancelled.store(true, Ordering::SeqCst);
                return Err(AvatarError("Actual canceled fixture public fetch".into()));
            }
            assert!(Instant::now() < deadline, "Owned release/cancel condition");
            std::thread::yield_now();
        }
        if self.fail.load(Ordering::SeqCst) {
            Err(AvatarError("Actual public image unavailable".into()))
        } else {
            Ok(png(if url == URL_A {
                [255, 0, 0, 255]
            } else {
                [0, 255, 0, 255]
            }))
        }
    }
}
fn cache(dir: &std::path::Path, uid: u64, fetcher: Arc<Fetcher>) -> AvatarCache {
    AvatarCache::new(
        dir.to_owned(),
        partition(&caller(Some(uid))).unwrap(),
        fetcher,
    )
}
fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "Actual asynchronous asset condition"
        );
        std::thread::yield_now();
    }
}
fn load(states: &mut ActorAvatarStatesV1, cache: &AvatarCache, actors: &[Actor]) {
    wait_until(|| {
        assert!(states.sync_from_cache(&caller(Some(42)), "Inbox genuine source", cache, actors));
        actors.iter().all(|actor| {
            states
                .ready(&ActorIdentity::github("github.invalid", actor))
                .is_some()
        })
    });
}

#[test]
fn actor_avatar_exact_all_observed_identity_fields_are_keyed_not_login() {
    let identity = ActorIdentity::github("github.invalid", &actor(URL_A));
    let image = Arc::new(AvatarImage::decode(&png([255, 0, 0, 255])).unwrap());
    let mut states = ActorAvatarStatesV1::default();
    states
        .states
        .insert(identity.clone(), AvatarState::Ready(image.clone()));
    assert!(std::ptr::eq(
        states.ready(&identity).unwrap(),
        image.as_ref()
    ));
    for field in 0..8 {
        let mut different = identity.clone();
        match field {
            0 => different.host = Some("other.invalid".into()),
            1 => different.login = Some("another".into()),
            2 => different.kind = ActorKind::Organization,
            3 => different.name = "Different actual display name".into(),
            4 => different.avatar_url = Some(URL_B.into()),
            5 => different.email = Some("local@example.invalid".into()),
            6 => different.git_role = Some(GitRole::Author),
            _ => different.host_associated = false,
        }
        assert!(states.ready(&different).is_none(), "Actual field{field}");
    }
    let mut missing_url = identity;
    missing_url.avatar_url = None;
    assert!(states.ready(&missing_url).is_none());
}

#[test]
fn actor_avatar_exact_same_login_distinct_actual_urls_keep_two_real_images() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    let cache = cache(dir.path(), 42, fetcher.clone());
    let actors = [actor(URL_A), actor(URL_B)];
    let mut states = ActorAvatarStatesV1::default();
    load(&mut states, &cache, &actors);
    assert_eq!(states.len(), 2);
    for (actor, color) in actors.iter().zip([[255, 0, 0, 255], [0, 255, 0, 255]]) {
        assert_eq!(
            states
                .ready(&ActorIdentity::github("github.invalid", actor))
                .unwrap()
                .pixels[0],
            color
        );
    }
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 2);
    cache.shutdown();
}

#[test]
fn actor_avatar_exact_duplicate_actor_same_numeric_requester_login_rename_dedups() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    let cache = cache(dir.path(), 42, fetcher.clone());
    let actors = [actor(URL_A), actor(URL_A)];
    let mut states = ActorAvatarStatesV1::default();
    load(&mut states, &cache, &actors);
    let mut renamed = caller(Some(42));
    renamed.login = "actual-new-login".into();
    assert!(states.sync_from_cache(&renamed, "Renamed display context", &cache, &actors));
    assert_eq!(states.len(), 1);
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    assert_eq!(partition(&renamed), partition(&caller(Some(42))));
    cache.shutdown();
}

#[test]
fn actor_avatar_exact_current_observation_replacement_drops_old_same_login_url() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    let cache = cache(dir.path(), 42, fetcher.clone());
    let mut states = ActorAvatarStatesV1::default();
    load(&mut states, &cache, &[actor(URL_A)]);
    load(&mut states, &cache, &[actor(URL_B)]);
    assert!(states
        .ready(&ActorIdentity::github("github.invalid", &actor(URL_A)))
        .is_none());
    assert!(states
        .ready(&ActorIdentity::github("github.invalid", &actor(URL_B)))
        .is_some());
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 2);
    cache.shutdown();
}

#[test]
fn actor_avatar_exact_absent_zero_switched_uid_wrong_cache_partition_refuses() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    let cache = cache(dir.path(), 42, fetcher.clone());
    let mut states = ActorAvatarStatesV1::default();
    for uid in [None, Some(0), Some(43)] {
        assert!(!states.sync_from_cache(&caller(uid), "Source", &cache, &[actor(URL_A)]));
        assert!(states.is_empty());
    }
    let mut different_host = caller(Some(42));
    different_host.host = "other.invalid".into();
    assert!(!states.sync_from_cache(&different_host, "Source", &cache, &[actor(URL_A)]));
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 0);
    cache.shutdown();
}

#[test]
fn actor_avatar_exact_raw_field_and_aggregate_caps_refuse_before_public_requests() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    let cache = cache(dir.path(), 42, fetcher.clone());
    let mut states = ActorAvatarStatesV1::default();
    let mut huge = actor(URL_A);
    huge.display_name = Some("x".repeat(MAX_FIELD_BYTES + 1));
    assert!(!states.sync_from_cache(&caller(Some(42)), "Source", &cache, &[huge]));
    let mut maximum = actor(URL_A);
    maximum.display_name = Some("x".repeat(MAX_FIELD_BYTES));
    assert!(!states.sync_from_cache(&caller(Some(42)), "Source", &cache, &vec![maximum; 17]));
    assert!(!states.sync_from_cache(
        &caller(Some(42)),
        &"x".repeat(MAX_CONTEXT_BYTES + 1),
        &cache,
        &[actor(URL_A)]
    ));
    assert!(!states.sync_from_cache(
        &caller(Some(42)),
        "unsafe\0context",
        &cache,
        &[actor(URL_A)]
    ));
    assert!(states.is_empty());
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 0);
    cache.shutdown();
}

#[test]
fn actor_avatar_exact_only_bounded_first128_rows_are_copied_requested_or_available() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    let cache = cache(dir.path(), 42, fetcher.clone());
    let mut actors = (0..129)
        .map(|i| Actor {
            login: format!("actor{i}"),
            actor_type: "User".into(),
            ..Default::default()
        })
        .collect::<Vec<_>>();
    actors[128].avatar_url = Some("x".repeat(MAX_FIELD_BYTES + 1));
    let mut states = ActorAvatarStatesV1::default();
    assert!(states.sync_from_cache(&caller(Some(42)), "Source", &cache, &actors));
    assert_eq!(states.len(), 128);
    assert!(states
        .get(&ActorIdentity::github("github.invalid", &actors[128]))
        .is_none());
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 0);
    cache.shutdown();
}

#[test]
fn actor_avatar_exact_no_url_actor_cannot_borrow_same_login_ready_or_invent_asset() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    let cache = cache(dir.path(), 42, fetcher.clone());
    let mut no_url = actor(URL_A);
    no_url.avatar_url = None;
    let mut states = ActorAvatarStatesV1::default();
    load(&mut states, &cache, &[actor(URL_A)]);
    assert!(states.sync_from_cache(
        &caller(Some(42)),
        "Source",
        &cache,
        &[actor(URL_A), no_url.clone()]
    ));
    assert!(matches!(
        states.get(&ActorIdentity::github("github.invalid", &no_url)),
        Some(AvatarState::Absent)
    ));
    assert!(states
        .ready(&ActorIdentity::github("github.invalid", &no_url))
        .is_none());
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    cache.shutdown();
}

#[test]
fn actor_avatar_exact_same_actor_pending_survives_filter_tab_context_label_changes() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    fetcher.wait.store(true, Ordering::SeqCst);
    let cache = cache(dir.path(), 42, fetcher.clone());
    let mut states = ActorAvatarStatesV1::default();
    assert!(states.sync_from_cache(&caller(Some(42)), "Inbox/All", &cache, &[actor(URL_A)]));
    wait_until(|| fetcher.calls.load(Ordering::SeqCst) == 1);
    for label in [
        "Inbox/Created",
        "PR same actual actor",
        "Search same actual actor",
    ] {
        assert!(states.sync_from_cache(&caller(Some(42)), label, &cache, &[actor(URL_A)]));
        assert!(!fetcher.cancelled.load(Ordering::SeqCst));
        assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    }
    fetcher.released.store(true, Ordering::SeqCst);
    load(&mut states, &cache, &[actor(URL_A)]);
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    cache.shutdown();
}

#[test]
fn actor_avatar_exact_retirement_drains_old_epoch_and_recovery_not_true_failure_retry() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    fetcher.wait.store(true, Ordering::SeqCst);
    let old = cache(dir.path(), 42, fetcher.clone());
    let mut owned = Some((partition(&caller(Some(42))).unwrap(), old.clone()));
    let mut states = ActorAvatarStatesV1::default();
    assert!(states.sync_from_cache(&caller(Some(42)), "Source", &old, &[actor(URL_A)]));
    wait_until(|| fetcher.calls.load(Ordering::SeqCst) == 1);
    states.retire_owned_cache(&mut owned);
    assert!(owned.is_none());
    assert!(states.is_empty());
    assert!(fetcher.cancelled.load(Ordering::SeqCst));
    assert!(!dir.path().read_dir().unwrap().any(|row| row.is_ok()));
    assert!(matches!(
        old.request(&ActorIdentity::github("github.invalid", &actor(URL_A))),
        AvatarState::Failed(_)
    ));
    fetcher.released.store(true, Ordering::SeqCst);
    let current = cache(dir.path(), 42, fetcher.clone());
    load(&mut states, &current, &[actor(URL_A)]);
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 2);
    current.shutdown();
}

#[test]
fn actor_avatar_exact_true_failed_asset_keeps_exact_username_and_never_poll_retries() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    fetcher.fail.store(true, Ordering::SeqCst);
    let cache = cache(dir.path(), 42, fetcher.clone());
    let mut states = ActorAvatarStatesV1::default();
    wait_until(|| {
        assert!(states.sync_from_cache(&caller(Some(42)), "Source", &cache, &[actor(URL_A)]));
        matches!(
            states.get(&ActorIdentity::github("github.invalid", &actor(URL_A))),
            Some(AvatarState::Failed(_))
        )
    });
    for _ in 0..20 {
        assert!(states.sync_from_cache(
            &caller(Some(42)),
            "Different harmless filter label",
            &cache,
            &[actor(URL_A)]
        ));
    }
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    assert!(states
        .ready(&ActorIdentity::github("github.invalid", &actor(URL_A)))
        .is_none());
    cache.shutdown();
}

#[test]
fn actor_avatar_exact_no_color_ready_pixels_keep_actual_initials_without_rgb() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    let cache = cache(dir.path(), 42, fetcher);
    let mut states = ActorAvatarStatesV1::default();
    load(&mut states, &cache, &[actor(URL_A)]);
    let identity = ActorIdentity::github("github.invalid", &actor(URL_A));
    for mode in [ColorMode::NoColor, ColorMode::LowColor] {
        let mut buffer = Buffer::empty(Rect::new(0, 0, 4, 1));
        AvatarWidget {
            identity: &identity,
            image: states.ready(&identity),
            theme: Theme::new(ThemeName::Dark, mode),
        }
        .render(buffer.area, &mut buffer);
        assert_eq!(
            buffer
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>(),
            "[AU]"
        );
        assert!(buffer
            .content
            .iter()
            .all(|cell| !matches!(cell.fg, Color::Rgb(..)) && !matches!(cell.bg, Color::Rgb(..))));
    }
    cache.shutdown();
}

#[test]
fn actor_avatar_exact_local_author_committer_never_match_host_user_by_display_name() {
    let git = GitActor {
        name: "Actual Observed User".into(),
        email: "same@example.invalid".into(),
        timestamp: 0,
    };
    let author = ActorIdentity::local(&git, GitRole::Author);
    let committer = ActorIdentity::local(&git, GitRole::Committer);
    let mut states = ActorAvatarStatesV1::default();
    states.states.insert(
        ActorIdentity::github("github.invalid", &actor(URL_A)),
        AvatarState::Ready(Arc::new(
            AvatarImage::decode(&png([255, 0, 0, 255])).unwrap(),
        )),
    );
    assert_ne!(author, committer);
    assert!(states.ready(&author).is_none());
    assert!(states.ready(&committer).is_none());
    assert!(author.label().contains("author"));
    assert!(committer.label().contains("committer"));
}

#[test]
fn actor_avatar_exact_only_active_surface_no_hidden_workspace_source_fallback() {
    let identity = caller(Some(42));
    let mut app = App {
        github: Some(crate::github_workspace::GithubWorkspace::empty(
            identity.clone(),
            vec!["example/repo".into()],
            Default::default(),
        )),
        ..Default::default()
    };
    for view in [
        View::Repository,
        View::Actions,
        View::Search,
        View::Dashboard,
    ] {
        app.view = view;
        assert!(
            app.avatar_source().is_none(),
            "No hidden Inbox fallback for {view:?}"
        );
    }
    app.view = View::Inbox;
    assert_eq!(app.avatar_source().unwrap().0, identity);
}

#[test]
fn actor_avatar_exact_actions_projection_keeps_same_login_url_and_nullable_variants() {
    use crate::actions_workspace::{ActionsRow, ActionsWorkspace};
    use crate::theme::SemanticToken;
    let mut workspace = ActionsWorkspace::new(caller(Some(42)), vec!["example/repo".into()]);
    let mut missing = actor(URL_A);
    missing.avatar_url = None;
    workspace.rows = [actor(URL_A), actor(URL_B), actor(URL_A), missing.clone()]
        .into_iter()
        .map(|actor| ActionsRow {
            label: "Actual actor row".into(),
            role: SemanticToken::Actor,
            actor: Some(actor),
            action: None,
        })
        .collect();
    let observations = workspace.actors();
    assert_eq!(observations.len(), 3);
    assert!(observations.contains(&actor(URL_A)));
    assert!(observations.contains(&actor(URL_B)));
    assert!(observations.contains(&missing));
}

#[test]
fn actor_avatar_exact_github_projection_borrows_and_caps_before_actor_cloning() {
    use crate::providers::gh::{PullRequest, Queue};
    let mut workspace = crate::github_workspace::GithubWorkspace::empty(
        caller(Some(42)),
        vec!["example/repo".into()],
        Default::default(),
    );
    workspace.inbox.pull_requests = (0..129)
        .map(|index| PullRequest {
            host: "github.invalid".into(),
            id: format!("ACTUAL_PR_{index}"),
            repository: "example/repo".into(),
            number: index + 1,
            title: "Actual PR".into(),
            body: String::new(),
            url: format!("https://github.invalid/example/repo/pull/{}", index + 1),
            author: if index == 128 {
                Actor {
                    login: "outside-cap".into(),
                    display_name: Some("x".repeat(2 * 1024 * 1024)),
                    ..Default::default()
                }
            } else {
                actor(if index % 2 == 0 { URL_A } else { URL_B })
            },
            state: "OPEN".into(),
            draft: false,
            created_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-01T00:00:00Z".into(),
            queues: [Queue::Authored].into_iter().collect(),
        })
        .collect();
    let observations = workspace.actors();
    assert_eq!(observations.len(), 128);
    assert!(observations
        .iter()
        .any(|found| found.avatar_url.as_deref() == Some(URL_A)));
    assert!(observations
        .iter()
        .any(|found| found.avatar_url.as_deref() == Some(URL_B)));
    assert!(observations
        .iter()
        .all(|found| found.login != "outside-cap"));
}

#[test]
fn actor_avatar_exact_active_hook_account_none_retirement_cancels_without_hidden_source() {
    let dir = tempfile::tempdir().unwrap();
    let fetcher = Arc::new(Fetcher::default());
    fetcher.wait.store(true, Ordering::SeqCst);
    let previous = cache(dir.path(), 42, fetcher.clone());
    let mut owned = Some((partition(&caller(Some(42))).unwrap(), previous.clone()));
    let mut app = App {
        view: View::Inbox,
        github: Some(crate::github_workspace::GithubWorkspace::empty(
            caller(None),
            vec!["example/repo".into()],
            Default::default(),
        )),
        ..Default::default()
    };
    assert!(app.avatar_states.sync_from_cache(
        &caller(Some(42)),
        "Original source",
        &previous,
        &[actor(URL_A)]
    ));
    wait_until(|| fetcher.calls.load(Ordering::SeqCst) == 1);
    sync_active_surface(&mut app, &mut owned, dir.path(), true);
    assert!(owned.is_none());
    assert!(app.avatar_states.is_empty());
    assert!(fetcher.cancelled.load(Ordering::SeqCst));
    assert_eq!(fetcher.calls.load(Ordering::SeqCst), 1);
    app.view = View::Repository;
    sync_active_surface(&mut app, &mut owned, dir.path(), true);
    assert!(owned.is_none());
    app.no_color = true;
    sync_active_surface(&mut app, &mut owned, dir.path(), true);
    assert!(owned.is_none());
}

#[test]
fn actor_avatar_exact_search_projection_preserves_actual_same_login_name_url_variants() {
    use crate::providers::gh::{RemoteSearchItem, RemoteSearchKind, RemoteSearchResult};
    use crate::search::{remote, workspace::SearchWorkspace, Query, SearchMode};
    let identity = caller(Some(42));
    let scope = Scope::github(&identity.host, identity.account_id).unwrap();
    let query = Query::parse("actual type:pr", SearchMode::GitHub).unwrap();
    let items = [actor(URL_A), actor(URL_B)]
        .into_iter()
        .enumerate()
        .map(|(index, author)| RemoteSearchItem {
            host: identity.host.clone(),
            repository: "example/repo".into(),
            kind: RemoteSearchKind::PullRequests,
            id: format!("ACTUAL_NODE_{}", index + 1),
            number: Some(index as u64 + 1),
            title: "Actual result".into(),
            body: String::new(),
            url: format!("https://github.invalid/example/repo/pull/{}", index + 1),
            state: "open".into(),
            author: Some(author),
            repository_owner: None,
            path: None,
            blob_sha: None,
            created_at: "2026-10-01T00:00:00Z".into(),
            updated_at: "2026-10-01T00:00:00Z".into(),
            text_matches: vec![],
        })
        .collect();
    let (page, remote_items) = remote::prepare_page(
        &scope,
        &query,
        RemoteSearchResult {
            identity: identity.clone(),
            query: "actual type:pr".into(),
            items,
            coverage: Default::default(),
            provider_notes: vec![],
        },
        128,
    )
    .unwrap();
    let mut workspace = SearchWorkspace::new(scope, vec![]);
    workspace.remote_identity = Some(identity);
    workspace.page = Some(page);
    workspace.remote_items = remote_items;
    let observations = workspace.actors();
    assert_eq!(observations.len(), 2);
    assert!(observations.contains(&actor(URL_A)));
    assert!(observations.contains(&actor(URL_B)));
}

#[test]
fn actor_avatar_exact_real_public_curl_two_urls_no_auth_env_and_exact_ready_pixels() {
    let dir = tempfile::tempdir().unwrap();
    let red = dir.path().join("red.png");
    let green = dir.path().join("green.png");
    std::fs::write(&red, png([255, 0, 0, 255])).unwrap();
    std::fs::write(&green, png([0, 255, 0, 255])).unwrap();
    let journal = dir.path().join("urls");
    let env = dir.path().join("env-keys");
    let program = dir.path().join("curl");
    let script=format!("#!/bin/sh\nfor arg; do url=$arg; done\nprintf '%s\\n' \"$url\" >> '{}'\n/usr/bin/env | /usr/bin/sed 's/=.*//' > '{}'\ncase \"$url\" in '{}' ) /bin/cat '{}' ;; '{}' ) /bin/cat '{}' ;; *) exit 91 ;; esac\n",journal.display(),env.display(),URL_A,red.display(),URL_B,green.display());
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    let cache = AvatarCache::new(
        dir.path().join("assets"),
        partition(&caller(Some(42))).unwrap(),
        Arc::new(CurlAvatarFetcher::with_program(program, Default::default())),
    );
    let mut states = ActorAvatarStatesV1::default();
    load(&mut states, &cache, &[actor(URL_A), actor(URL_B)]);
    cache.shutdown();
    let mut urls = std::fs::read_to_string(journal)
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    urls.sort();
    assert_eq!(urls, vec![URL_A.to_owned(), URL_B.to_owned()]);
    assert_eq!(
        states
            .ready(&ActorIdentity::github("github.invalid", &actor(URL_A)))
            .unwrap()
            .pixels[0],
        [255, 0, 0, 255]
    );
    assert_eq!(
        states
            .ready(&ActorIdentity::github("github.invalid", &actor(URL_B)))
            .unwrap()
            .pixels[0],
        [0, 255, 0, 255]
    );
    let keys = std::fs::read_to_string(env).unwrap();
    for forbidden in [
        "GH_TOKEN",
        "GITHUB_TOKEN",
        "GH_CONFIG_DIR",
        "CURL_HOME",
        "HTTP_PROXY",
        "HTTPS_PROXY",
    ] {
        assert!(!keys.lines().any(|key| key == forbidden));
    }
}
