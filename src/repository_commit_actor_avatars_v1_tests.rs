//! Genuine closed report display identities + actual isolated image work.
//! Synthetic oversized identities below are DISPLAY inputs, never read proofs.
use super::*;
use crate::{
    identity::{AvatarError, AvatarFetcher, AvatarState},
    providers::{gh::HostIdentity, runner::CancellationToken},
    repository_commit_actor_native_v1::tests::Kit,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::{
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
    fail: AtomicBool,
}
impl AvatarFetcher for Fetcher {
    fn fetch(&self, url: &str, cancel: &CancellationToken) -> Result<Vec<u8>, AvatarError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.urls.lock().unwrap().push(url.to_owned());
        let end = Instant::now() + Duration::from_secs(3);
        while self.wait.load(Ordering::SeqCst) {
            if cancel.is_cancelled() {
                self.cancelled.store(true, Ordering::SeqCst);
                return Err(AvatarError("Original public-image request canceled".into()));
            }
            assert!(Instant::now() < end);
            std::thread::yield_now();
        }
        if self.fail.load(Ordering::SeqCst) {
            return Err(AvatarError("Public image unavailable".into()));
        }
        let color = if url.contains("/201?") {
            [255, 0, 0, 255]
        } else {
            [0, 255, 0, 255]
        };
        let mut bytes = Cursor::new(Vec::new());
        DynamicImage::ImageRgba8(RgbaImage::from_pixel(2, 2, Rgba(color)))
            .write_to(&mut bytes, ImageFormat::Png)
            .unwrap();
        Ok(bytes.into_inner())
    }
}
fn until(mut condition: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < end, "Actual image event must arrive");
        std::thread::yield_now();
    }
}
fn load(
    states: &mut ActorAvatarStatesV1,
    cache: &AvatarCache,
    identity: &HostIdentity,
    roles: &[&ActorIdentity],
) {
    until(|| {
        assert!(states.sync_prepared(identity, cache, roles));
        roles
            .iter()
            .all(|role| matches!(states.get(role), Some(AvatarState::Ready(_))))
    });
}
#[test]
fn git_actor_association_native_avatars_exact_genuine_roles_public_bytes_not_dto_remap_or_actor_uid_partition(
) {
    let mut k = Kit::new("sha1", false, 42);
    let (_, report) = k.verify();
    let roles = prepared_roles(&report);
    assert_eq!(roles.len(), 2);
    assert_eq!(roles[0].git_role, Some(GitRole::Author));
    assert_eq!(roles[1].git_role, Some(GitRole::Committer));
    assert_eq!(roles[0].name, "Raw Writer");
    assert_eq!(roles[1].name, "Raw Recorder");
    let fetch = Arc::new(Fetcher::default());
    let dir = tempfile::tempdir().unwrap();
    let cache = AvatarCache::new(
        dir.path().to_owned(),
        partition(report.source().identity()).unwrap(),
        fetch.clone(),
    );
    let mut states = ActorAvatarStatesV1::default();
    load(&mut states, &cache, report.source().identity(), &roles);
    assert_eq!(fetch.calls.load(Ordering::SeqCst), 2);
    assert_eq!(states.len(), 2);
    assert_eq!(states.ready(roles[0]).unwrap().pixels[0], [255, 0, 0, 255]);
    assert_eq!(states.ready(roles[1]).unwrap().pixels[0], [0, 255, 0, 255]);
    let mut wrong_role = roles[0].clone();
    wrong_role.git_role = Some(GitRole::Committer);
    assert!(states.ready(&wrong_role).is_none());
    let wrong_dto = ActorIdentity::github(
        &report.source().identity().host,
        match report.role(GitRole::Author) {
            LocalCommitRoleReport::Reported(a) => a.actor(),
            _ => panic!(),
        },
    );
    assert!(states.ready(&wrong_dto).is_none());
    cache.shutdown();
    k.assert_reads(14);
}
#[test]
fn git_actor_association_native_avatars_null_role_does_not_request_guess_or_other_role_image() {
    let mut k = Kit::new("sha1", true, 42);
    let (_, report) = k.verify();
    let roles = prepared_roles(&report);
    assert_eq!(roles.len(), 1);
    assert_eq!(roles[0].git_role, Some(GitRole::Committer));
    let fetch = Arc::new(Fetcher::default());
    let dir = tempfile::tempdir().unwrap();
    let cache = AvatarCache::new(
        dir.path().to_owned(),
        partition(report.source().identity()).unwrap(),
        fetch.clone(),
    );
    let mut states = ActorAvatarStatesV1::default();
    load(&mut states, &cache, report.source().identity(), &roles);
    assert_eq!(fetch.calls.load(Ordering::SeqCst), 1);
    assert_eq!(states.len(), 1);
    cache.shutdown();
    k.assert_reads(14);
}
#[test]
fn git_actor_association_native_avatars_request_preflight_positive_requester_and_raw_field_cap_before_fetch(
) {
    let mut k = Kit::new("sha1", false, 42);
    let (_, report) = k.verify();
    let roles = prepared_roles(&report);
    let fetch = Arc::new(Fetcher::default());
    let dir = tempfile::tempdir().unwrap();
    let cache = AvatarCache::new(
        dir.path().to_owned(),
        partition(report.source().identity()).unwrap(),
        fetch.clone(),
    );
    let mut states = ActorAvatarStatesV1::default();
    let mut bad = roles[0].clone();
    bad.name = "x".repeat(4097);
    assert!(!prepared_avatar_preflight(&[&bad]));
    assert!(!states.sync_prepared(report.source().identity(), &cache, &[&bad]));
    let mut no_uid = report.source().identity().clone();
    no_uid.account_id = None;
    assert!(!states.sync_prepared(&no_uid, &cache, &roles));
    let mut actor_uid = report.source().identity().clone();
    actor_uid.account_id = Some(201);
    assert!(!states.sync_prepared(&actor_uid, &cache, &roles));
    assert!(states.is_empty());
    assert_eq!(fetch.calls.load(Ordering::SeqCst), 0);
    cache.shutdown();
    k.assert_reads(14);
}
#[test]
fn git_actor_association_native_avatars_no_color_unavailable_repository_captures_no_hidden_fetch() {
    let mut k = Kit::new("sha1", false, 42);
    let (owner, report) = k.verify();
    assert!(k.state.accept(&owner, report));
    let dir = tempfile::tempdir().unwrap();
    let mut states = ActorAvatarStatesV1::default();
    let mut cache = None;
    assert!(poll_repository(
        &k.state,
        true,
        &mut states,
        &mut cache,
        dir.path(),
        true,
        ColorMode::NoColor
    ));
    assert!(cache.is_none());
    assert!(states.is_empty());
    k.state.retire_private();
    assert!(poll_repository(
        &k.state,
        true,
        &mut states,
        &mut cache,
        dir.path(),
        true,
        ColorMode::TrueColor
    ));
    assert!(cache.is_none());
    assert!(!poll_repository(
        &k.state,
        false,
        &mut states,
        &mut cache,
        dir.path(),
        true,
        ColorMode::TrueColor
    ));
    k.assert_reads(14);
}
#[test]
fn git_actor_association_native_avatars_retirement_reaps_cold_fetch_late_state_and_no_failed_retry()
{
    let mut k = Kit::new("sha1", false, 42);
    let (_, report) = k.verify();
    let roles = prepared_roles(&report);
    let dir = tempfile::tempdir().unwrap();
    let fetch = Arc::new(Fetcher::default());
    fetch.wait.store(true, Ordering::SeqCst);
    let scope = partition(report.source().identity()).unwrap();
    let cache = AvatarCache::new(dir.path().to_owned(), scope.clone(), fetch.clone());
    let mut states = ActorAvatarStatesV1::default();
    assert!(states.sync_prepared(report.source().identity(), &cache, &roles));
    until(|| fetch.calls.load(Ordering::SeqCst) == 2);
    let mut owned = Some((scope.clone(), cache));
    states.retire_owned_cache(&mut owned);
    assert!(owned.is_none());
    assert!(states.is_empty());
    assert!(fetch.cancelled.load(Ordering::SeqCst));
    let failed = Arc::new(Fetcher::default());
    failed.fail.store(true, Ordering::SeqCst);
    let cache = AvatarCache::new(dir.path().join("failed"), scope, failed.clone());
    until(|| {
        states.sync_prepared(report.source().identity(), &cache, &roles);
        roles
            .iter()
            .all(|r| matches!(states.get(r), Some(AvatarState::Failed(_))))
    });
    for _ in 0..5 {
        assert!(states.sync_prepared(report.source().identity(), &cache, &roles));
    }
    assert_eq!(failed.calls.load(Ordering::SeqCst), 2);
    cache.shutdown();
    k.assert_reads(14);
}
