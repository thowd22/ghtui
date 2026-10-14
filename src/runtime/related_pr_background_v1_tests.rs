use super::*;
use crate::repository_commit_actor_native_v1::tests::Kit;
use crate::repository_related_pr_v1::{self as related, RelatedPrActionV1};
use std::{
    sync::{Barrier, Condvar},
    time::{Duration, Instant},
};
struct ReleaseOnDrop(Arc<(Mutex<bool>, Condvar)>);
impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        *self.0 .0.lock().unwrap_or_else(|p| p.into_inner()) = true;
        self.0 .1.notify_all();
    }
}
/// Re-deliver the exact real worker-produced completion, unchanged. This makes
/// before-poll intent veto deterministic without forging an owner/generation or
/// sleeping until a guessed worker schedule. The production queue is unchanged.
fn poll_actual(background: &mut Background, app: &mut App) -> bool {
    let actual = background
        .completions
        .recv_timeout(Duration::from_secs(3))
        .unwrap();
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    sender.send(actual).unwrap();
    let original = std::mem::replace(&mut background.completions, receiver);
    let changed = background.poll(app);
    background.completions = original;
    changed
}
#[test]
fn related_pr_saturated_existing_queue_captures_original_deadline_and_launches_zero_children() {
    let kit = Kit::new("sha1", false, 42);
    let mut app = App {
        view: View::Repository,
        workspace: Some(kit.workspace),
        local_commit_actors: kit.state,
        ..Default::default()
    };
    let barrier = Arc::new(Barrier::new(3));
    let gate = Arc::new((Mutex::new(false), Condvar::new()));
    let mut background = Background::new();
    let _release_on_failure = ReleaseOnDrop(gate.clone());
    for view in [View::Inbox, View::Search] {
        let barrier = barrier.clone();
        let gate = gate.clone();
        assert!(background.submit(&mut app, view, move |_| {
            barrier.wait();
            let (mut guard, cv) = (gate.0.lock().unwrap(), &gate.1);
            while !*guard {
                guard = cv.wait(guard).unwrap();
            }
            Ok(Default::default())
        }));
    }
    barrier.wait();
    for _ in 0..4 {
        assert!(background.submit(&mut app, View::Actions, |_| Ok(Default::default())));
    }
    related::dispatch(&mut app, RelatedPrActionV1::Discover);
    app.related_pr_requested_v1 = false;
    assert!(!background.submit_related_pr_v1(&mut app, |_| panic!(
        "Full queue must not execute related producer"
    )));
    assert!(!app.loading[View::Repository.index()]);
    assert!(app.status.contains("zero work submitted"));
    assert!(background.current_related_pr_v1.is_none());
    assert!(
        std::fs::read_to_string(kit.fixture.root.join("native-actor-git.jsonl"))
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        std::fs::read_to_string(kit.fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap()
            .lines()
            .count(),
        7
    );
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
}
#[test]
fn related_pr_current_owned_caught_worker_panic_never_reaches_generic_effects() {
    let kit = Kit::new("sha1", false, 42);
    let mut app = App {
        view: View::Repository,
        workspace: Some(kit.workspace),
        local_commit_actors: kit.state,
        ..Default::default()
    };
    app.settings_host = "github.invalid".into();
    // Actual factory-produced private catalogue/source, not a synthetic DTO.
    // An unavailable original boundary must fence every retained consumer,
    // even when a model remains behind the capturing Related modal.
    crate::repository_catalogue_native::dispatch(
        &mut app,
        crate::repository_catalogue_native::CatalogueUiAction::Open,
    );
    let modal = app.catalogue.as_mut().unwrap();
    assert!(kit.catalogue.frame().unwrap().rows[0]
        .label
        .contains("example/original"));
    modal.workspace = Some(kit.catalogue);
    modal.guard = Some(kit.source.clone());
    modal.source = Some(kit.source.source().source().clone());
    let sha = app
        .workspace
        .as_ref()
        .unwrap()
        .selected_commit
        .as_ref()
        .unwrap()
        .id
        .clone();
    related::dispatch(&mut app, RelatedPrActionV1::Discover);
    app.related_pr_requested_v1 = false;
    let mut background = Background::new();
    assert!(background.submit_related_pr_v1(&mut app, |_| panic!(
        "deliberate original-owned worker failure"
    )));
    let until = Instant::now() + Duration::from_secs(3);
    while app.loading[View::Repository.index()] {
        background.poll(&mut app);
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    assert_eq!(
        app.workspace
            .as_ref()
            .unwrap()
            .selected_commit
            .as_ref()
            .unwrap()
            .id,
        sha
    );
    assert!(app.related_pr_v1.is_some());
    assert!(app.hits.is_empty());
    assert!(app.local_commit_actors.profile().is_none());
    assert!(app.local_commit_actors.choice().is_none());
    let modal = app.catalogue.as_ref().unwrap();
    assert!(modal.fenced);
    assert!(modal.workspace.is_none());
    assert!(modal.guard.is_none());
    assert!(modal.request.is_none());
    assert!(modal.hits.is_empty());
    assert!(app.catalogue_cancel_requested);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 24)).unwrap();
    terminal
        .draw(|frame| {
            assert!(related::draw(frame, &mut app));
        })
        .unwrap();
    let painted: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(painted.contains("Original related PR read unavailable"));
    assert!(!painted.contains("example/original"));
    assert!(!painted.contains("[r Discover]"));
    related::dispatch(&mut app, RelatedPrActionV1::Discover);
    related::dispatch(&mut app, RelatedPrActionV1::Open(0));
    assert!(!app.related_pr_requested_v1);
    related::dispatch(&mut app, RelatedPrActionV1::Back);
    assert!(app.related_pr_v1.is_none());
    assert!(app.catalogue.as_ref().unwrap().fenced);
    assert!(
        std::fs::read_to_string(kit.fixture.root.join("native-actor-git.jsonl"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn related_pr_genuine_closed_result_refuses_graph_tree_gh_and_generic_mixed_publishers() {
    let kit = Kit::new("sha1", false, 42);
    let mut steps = crate::repository_related_pr_v1::tests::protocol(&kit.sha);
    steps.extend(
        crate::repository_related_pr_v1::tests::protocol(&kit.sha)
            .into_iter()
            .skip(7),
    );
    kit.fixture.scripted_gh(&steps);
    let mut app = App {
        view: View::Repository,
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
    let mut prepared = PreparedView {
        related_pr_v1: Some(result),
        ..Default::default()
    };
    assert!(only_related(&prepared));
    prepared.workspace = app.workspace.clone();
    assert!(!only_related(&prepared));
    assert!(!crate::runtime::local_saved_navigation_v1::original_local_prepared_only_v1(&prepared));
    assert!(
        !crate::runtime::original_tree_background_v1::original_tree_prepared_only_v1(&prepared)
    );
    assert!(
        !crate::pr_checks_native::scoped::choices::continuations_v2::corrective_v3::prepared_only(
            &prepared
        )
    );
    prepared.workspace = None;
    prepared.clear_private = true;
    assert!(!only_related(&prepared));
    prepared.clear_private = false;
    prepared.status = "private status must never be smuggled".into();
    assert!(!only_related(&prepared));
    prepared.status.clear();
    assert!(only_related(&prepared));
    let old = app.status.clone();
    let mut background = Background::new();
    let provider = kit.provider.clone();
    let database = kit.fixture.root.join("native.sqlite3");
    assert!(
        background.submit_related_pr_v1(&mut app, move |actual_owner| {
            let fresh = related::read(&actual_owner, &provider, &database)?;
            Ok(PreparedView {
                related_pr_v1: Some(fresh),
                clear_private: true,
                ..Default::default()
            })
        })
    );
    assert!(!poll_actual(&mut background, &mut app));
    assert!(background.current.contains_key(&View::Repository.index()));
    assert!(background.cancel_current_related_pr_v1());
    assert_eq!(app.status, old);
    assert!(app.related_pr_v1.is_some());
    assert!(app.workspace.is_some());
    let rows = std::fs::read_to_string(kit.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(rows.lines().count(), 23);
    assert!(rows
        .lines()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
        .all(|r| r["matched"] == true));
}

#[test]
fn related_pr_actual_completed_success_error_and_panic_newer_intent_veto_keeps_logical_close() {
    for outcome in 0..3 {
        for diff_cancel in [false, true] {
            let kit = Kit::new("sha1", false, 42);
            kit.fixture
                .scripted_gh(&crate::repository_related_pr_v1::tests::protocol(&kit.sha));
            let mut app = App {
                view: View::Repository,
                workspace: Some(kit.workspace),
                local_commit_actors: kit.state,
                ..Default::default()
            };
            related::dispatch(&mut app, RelatedPrActionV1::Discover);
            app.related_pr_requested_v1 = false;
            app.status = "newer original user intent must survive".into();
            app.content[View::Inbox.index()] = vec!["unrelated existing private model".into()];
            let mut background = Background::new();
            let provider = kit.provider.clone();
            let database = kit.fixture.root.join("native.sqlite3");
            assert!(
                background.submit_related_pr_v1(&mut app, move |owner| match outcome {
                    0 => related::read(&owner, &provider, &database).map(|result| PreparedView {
                        related_pr_v1: Some(result),
                        ..Default::default()
                    }),
                    1 => Err("must not publish a private error".into()),
                    _ => panic!("deliberate current completion panic before queued veto"),
                })
            );
            let owner = background.current_related_pr_v1.as_ref().unwrap().clone();
            let generation = background.current_related_pr_generation_v1.unwrap();
            if diff_cancel {
                app.original_diff_cancel_v1 = true;
            } else {
                app.repository_load_requested = true;
            }
            assert!(!poll_actual(&mut background, &mut app));
            assert_eq!(
                background.current.get(&View::Repository.index()).unwrap().0,
                generation
            );
            assert!(Arc::ptr_eq(
                background.current_related_pr_v1.as_ref().unwrap(),
                &owner
            ));
            assert!(app.loading[View::Repository.index()]);
            assert_eq!(app.status, "newer original user intent must survive");
            assert_eq!(
                app.content[View::Inbox.index()],
                ["unrelated existing private model"]
            );
            assert!(app.local_commit_actors.choice().is_some());
            related::dispatch(&mut app, RelatedPrActionV1::Cancel);
            assert!(app.related_pr_cancel_v1);
            assert!(background.cancel_current_related_pr_v1());
            assert!(owner.cancellation().is_cancelled());
            assert!(background.current_related_pr_v1.is_none());
            assert!(background.current_related_pr_generation_v1.is_none());
            assert!(!background.current.contains_key(&View::Repository.index()));
        }
    }
}

#[test]
fn related_pr_close_cannot_cancel_genuinely_submitted_other_repository_generation() {
    let kit = Kit::new("sha1", false, 42);
    let mut app = App {
        view: View::Repository,
        workspace: Some(kit.workspace),
        local_commit_actors: kit.state,
        ..Default::default()
    };
    related::dispatch(&mut app, RelatedPrActionV1::Discover);
    app.related_pr_requested_v1 = false;
    let mut background = Background::new();
    assert!(background.submit_related_pr_v1(&mut app, |_| Err("old completed boundary".into())));
    let owner = background.current_related_pr_v1.as_ref().unwrap().clone();
    app.repository_load_requested = true;
    assert!(!poll_actual(&mut background, &mut app));
    assert!(
        background.submit(&mut app, View::Repository, |_| Ok(PreparedView {
            status: "actual newer repository generation".into(),
            ..Default::default()
        }))
    );
    let (new_generation, new_cancel) = background
        .current
        .get(&View::Repository.index())
        .unwrap()
        .clone();
    related::dispatch(&mut app, RelatedPrActionV1::Cancel);
    assert!(!background.cancel_current_related_pr_v1());
    assert!(owner.cancellation().is_cancelled());
    assert!(!new_cancel.is_cancelled());
    assert!(app.loading[View::Repository.index()]);
    assert_eq!(
        background.current.get(&View::Repository.index()).unwrap().0,
        new_generation
    );
    assert!(poll_actual(&mut background, &mut app));
    assert_eq!(app.status, "actual newer repository generation");
}

#[test]
fn related_pr_original_diff_sides_genuine_mixed_result_rejects_before_slot_and_private_effects() {
    use crate::original_diff_sides_v1_test_support as diff_fixture;
    use crate::repository_diff_sides_v1::OriginalDiffPickerV1;
    use crate::repository_diff_sides_worker_v1::{OriginalDiffWorkV1, OriginalDiffWorkerOwnerV1};
    let (real, base, head, _) = diff_fixture::fixture("sha256");
    let workspace =
        diff_fixture::workspace(&real, crate::git::Comparison::BranchChanges { base, head });
    let mut picker = OriginalDiffPickerV1::new(
        crate::git::OriginalComparisonIntentV1::capture(&workspace).unwrap(),
    );
    let work = Arc::new(OriginalDiffWorkV1::Query(picker.request().unwrap()));
    let diff_owner = OriginalDiffWorkerOwnerV1::begin(
        &workspace,
        Some(&picker),
        work,
        CancellationToken::default(),
    )
    .unwrap();
    let original_diff_result = diff_owner.prepare().unwrap();
    assert!(diff_owner.accounting().unwrap().sealed());
    assert_eq!(diff_owner.accounting().unwrap().children_started(), 9);
    let before = real.snapshot();
    real.clear_trace();

    let kit = Kit::new("sha1", false, 42);
    kit.fixture
        .scripted_gh(&crate::repository_related_pr_v1::tests::protocol(&kit.sha));
    let mut app = App {
        view: View::Repository,
        workspace: Some(kit.workspace),
        local_commit_actors: kit.state,
        ..Default::default()
    };
    related::dispatch(&mut app, RelatedPrActionV1::Discover);
    app.related_pr_requested_v1 = false;
    app.status = "Existing status must survive foreign prepared kind".into();
    app.content[View::Inbox.index()] = vec!["Retained unrelated private model".into()];
    let origin = app
        .workspace
        .as_ref()
        .unwrap()
        .actor_origin
        .clone()
        .unwrap();
    let mut background = Background::new();
    let provider = kit.provider.clone();
    let database = kit.fixture.root.join("native.sqlite3");
    assert!(background.submit_related_pr_v1(&mut app, move |owner| {
        Ok(PreparedView {
            related_pr_v1: Some(related::read(&owner, &provider, &database)?),
            original_diff_result_v1: Some(original_diff_result),
            ..Default::default()
        })
    }));
    let owner = background.current_related_pr_v1.as_ref().unwrap().clone();
    let generation = background.current_related_pr_generation_v1.unwrap();
    assert!(!poll_actual(&mut background, &mut app));
    assert_eq!(
        background.current.get(&View::Repository.index()).unwrap().0,
        generation
    );
    assert!(Arc::ptr_eq(
        background.current_related_pr_v1.as_ref().unwrap(),
        &owner
    ));
    assert!(app.loading[View::Repository.index()]);
    assert_eq!(
        app.status,
        "Existing status must survive foreign prepared kind"
    );
    assert_eq!(
        app.content[View::Inbox.index()],
        ["Retained unrelated private model"]
    );
    assert!(Arc::ptr_eq(
        &origin,
        app.workspace
            .as_ref()
            .unwrap()
            .actor_origin
            .as_ref()
            .unwrap()
    ));
    assert!(app.local_commit_actors.choice().is_some());
    assert!(background.cancel_current_related_pr_v1());
    assert!(owner.cancellation().is_cancelled());
    assert!(real.children().is_empty());
    assert_eq!(real.snapshot(), before);
    let journal =
        std::fs::read_to_string(kit.fixture.root.join("gh-fixture/commands.jsonl")).unwrap();
    assert_eq!(journal.lines().count(), 15);
    assert!(journal
        .lines()
        .map(|s| serde_json::from_str::<serde_json::Value>(s).unwrap())
        .all(|r| r["matched"] == true));
}
