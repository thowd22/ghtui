//! Genuine original-source owner/token contracts. Poison PreparedView fields
//! probe publisher isolation only; they are never API or access evidence.
use super::*;
use crate::{
    github_workspace::GithubWorkspace,
    repository_commit_actor_native_v1::{tests::Kit, verify_local_actor_v1},
    storage::Scope,
};
use std::time::{Duration, Instant};

fn app(k: &mut Kit) -> App {
    App {
        view: View::Repository,
        workspace: Some(k.workspace.clone()),
        local_commit_actors: std::mem::take(&mut k.state),
        github: Some(GithubWorkspace::empty(
            k.source.source().identity().clone(),
            vec!["example/original".into()],
            Default::default(),
        )),
        context: "Original local Git source".into(),
        ..Default::default()
    }
}
fn poll(background: &mut Background, app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while background.current.contains_key(&View::Repository.index()) {
        background.poll(app);
        assert!(Instant::now() < deadline, "Actual actor completion missing");
        thread::yield_now();
    }
}

#[test]
fn git_actor_association_native_runtime_retired_owner_rejects_success_error_and_caught_panic_before_effects(
) {
    for outcome in 0..3 {
        let mut k = Kit::new("sha1", false, 42);
        let mut app = app(&mut k);
        let mut background = Background::new();
        let (started, live) = mpsc::sync_channel(1);
        let (release, waiting) = mpsc::sync_channel(1);
        let mut poison = k.source.source().identity().clone();
        poison.account_id = Some(43);
        assert!(background.submit_local_actor(&mut app, move |owner| {
            started.send(owner.cancellation().clone()).unwrap();
            waiting.recv_timeout(Duration::from_secs(3)).unwrap();
            match outcome {
                0 => Ok(PreparedView {
                    clear_private: true,
                    verified_identity: Some(poison),
                    status: "LATE_ACTOR_PRIVATE_STATUS".into(),
                    ..Default::default()
                }),
                1 => Err("LATE_ACTOR_PRIVATE_ERROR".into()),
                _ => panic!("LATE_ACTOR_PRIVATE_PANIC"),
            }
        }));
        let actual_token = live.recv_timeout(Duration::from_secs(3)).unwrap();
        app.local_commit_actors.retire_private();
        assert!(
            actual_token.is_cancelled(),
            "Profile and actual worker share one token"
        );
        assert!(background
            .current
            .get(&View::Repository.index())
            .unwrap()
            .1
            .is_cancelled());
        app.status = "REPLACEMENT_STATUS".into();
        app.content[View::Inbox.index()] = vec!["CURRENT_PRIVATE_FRAME".into()];
        app.loading[View::Repository.index()] = true;
        release.send(()).unwrap();
        poll(&mut background, &mut app);
        assert_eq!(app.status, "REPLACEMENT_STATUS");
        assert_eq!(app.context, "Original local Git source");
        assert_eq!(app.content[View::Inbox.index()], ["CURRENT_PRIVATE_FRAME"]);
        assert_eq!(app.github.as_ref().unwrap().identity.account_id, Some(42));
        assert!(
            app.loading[View::Repository.index()],
            "Late completion cannot clear replacement loading"
        );
        assert_eq!(
            app.workspace
                .as_ref()
                .unwrap()
                .selected_commit
                .as_ref()
                .unwrap()
                .id,
            k.sha
        );
        assert!(k.children().is_empty());
        k.assert_reads(7);
    }
}

#[test]
fn git_actor_association_native_runtime_missing_typed_result_and_foreign_closed_report_never_use_generic_publisher(
) {
    for foreign_report in [false, true] {
        let mut k = Kit::new("sha1", false, 42);
        let old_report = foreign_report.then(|| {
            let (owner, report) = k.verify();
            assert!(k.state.accept(&owner, report.clone()));
            report
        });
        let mut app = app(&mut k);
        let mut background = Background::new();
        let mut poison = k.source.source().identity().clone();
        poison.account_id = Some(43);
        let (started, live) = mpsc::sync_channel(1);
        let (release, waiting) = mpsc::sync_channel(1);
        assert!(background.submit_local_actor(&mut app, move |_| {
            started.send(()).unwrap();
            waiting.recv_timeout(Duration::from_secs(3)).unwrap();
            Ok(PreparedView {
                local_actor_result: old_report.map(Ok),
                clear_private: true,
                verified_identity: Some(poison),
                status: "FOREIGN_ACTOR_PRIVATE_STATUS".into(),
                ..Default::default()
            })
        }));
        live.recv_timeout(Duration::from_secs(3)).unwrap();
        app.status = "CURRENT_ORIGINAL_STATUS".into();
        app.content[View::Inbox.index()] = vec!["ORIGINAL_PRIVATE_FRAME".into()];
        release.send(()).unwrap();
        poll(&mut background, &mut app);
        assert_eq!(app.status, "CURRENT_ORIGINAL_STATUS");
        assert_eq!(app.content[View::Inbox.index()], ["ORIGINAL_PRIVATE_FRAME"]);
        assert_eq!(app.github.as_ref().unwrap().identity.account_id, Some(42));
        assert!(app.loading[View::Repository.index()]);
        assert!(app.local_commit_actors.report().is_none());
        assert_eq!(k.children().len(), if foreign_report { 2 } else { 0 });
        k.assert_reads(if foreign_report { 14 } else { 7 });
    }
}

#[test]
fn git_actor_association_native_runtime_genuine_request_accepts_exact_report_and_keeps_local_bytes()
{
    let mut k = Kit::new("sha1", false, 42);
    let mut app = app(&mut k);
    let provider = k.provider.clone();
    let database = k.fixture.root.join("native.sqlite3");
    let original_index = std::fs::read(k.fixture.repo().join(".git/index")).unwrap();
    let original_work = std::fs::read(k.fixture.repo().join("tracked")).unwrap();
    let mut background = Background::new();
    assert!(background.submit_local_actor(&mut app, move |owner| {
        owner.guard()?;
        let cache = crate::storage::Cache::open(&database).unwrap();
        Ok(PreparedView {
            local_actor_result: Some(verify_local_actor_v1(&owner, &provider, &cache)),
            clear_private: true, // Other generic fields are ignored even on valid actor success.
            status: "NOT_AN_ACTOR_STATUS".into(),
            ..Default::default()
        })
    }));
    poll(&mut background, &mut app);
    assert!(!app.loading[View::Repository.index()]);
    assert!(app.local_commit_actors.report().is_some());
    assert!(!app.local_commit_actors.is_pending());
    assert_eq!(app.github.as_ref().unwrap().identity.account_id, Some(42));
    assert_eq!(app.context, "Original local Git source");
    assert!(app.status.contains("Original observed host report"));
    assert_eq!(
        std::fs::read(k.fixture.repo().join(".git/index")).unwrap(),
        original_index
    );
    assert_eq!(
        std::fs::read(k.fixture.repo().join("tracked")).unwrap(),
        original_work
    );
    assert_eq!(
        k.cache
            .draft(
                &Scope::github("github.invalid", Some(42)).unwrap(),
                "actor-draft"
            )
            .unwrap()
            .as_deref(),
        Some("Original42 unsent")
    );
    assert_eq!(k.children().len(), 2);
    k.assert_reads(14);
}
