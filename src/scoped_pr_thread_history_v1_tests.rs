//! DECLARED/UNRUN. Genuine original source/Files/selected-thread producer,
//! actual SQLite draft/reference, then a DIFFERENT Summary history purpose.
use super::*;
use crate::{
    pr_write_journal::{thread_history_v1::ThreadOriginSeedV1, PrWriteJournal, PrWriteTarget},
    scoped_pr_observed_tabs_v1::resources::threads_v1::history_v1 as history,
};
use sha2::{Digest, Sha256};
fn history_steps(uid: u64) -> Vec<GhStep> {
    let mut steps = principal(uid, "octocat");
    if uid != 42 {
        return steps;
    }
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.extend(principal(42, "renamed-same-positive-id"));
    assert_eq!(steps.len(), 8);
    steps
}
fn seed(real: &Real) -> ThreadOriginSeedV1 {
    with_original(real, |attempt| {
        let p = attempt.actual.parent();
        let root = attempt.actual.original_reply_root().unwrap();
        let scope = Scope::github(HOST, Some(42)).unwrap();
        let original = serde_json::to_vec(&(
            &scope,
            p.repository_id(),
            p.repository_node(),
            p.database_id(),
            p.node_id(),
            p.number(),
            p.head_sha(),
            p.base_tip_sha(),
            attempt.actual.thread().node_id.as_str(),
            "thread-reply",
            Some((
                &root.node_id,
                root.database_id,
                &root.commit_sha,
                &root.observed_path,
            )),
        ))
        .unwrap();
        let key = format!("thread-draft/{:x}", Sha256::digest(original));
        let origin = ThreadOriginSeedV1::from_producer(
            &database(&real.fixture),
            attempt.identity,
            attempt.actual,
            "thread-reply",
            &key,
        )
        .unwrap();
        (attempt.close)().unwrap();
        origin
    })
}
fn summary(real: &mut Real) {
    real.app.overlay = None;
    real.app.scoped_threads_v1 = None;
    real.app.selected = 0;
    real.app.scroll = 0;
    let github = real.app.github.as_mut().unwrap();
    github.scoped_pr_resource = None;
    let page = github.pr.as_mut().unwrap();
    page.tab = crate::github_workspace::PrTab::Summary;
    page.prepare_rows().unwrap();
}
fn setup(extra: Vec<GhStep>, legacy: bool) -> (Real, String) {
    let mut steps = observation_steps(selected_node(false, true));
    steps.extend(extra);
    let mut real = real(steps);
    let origin = seed(&real);
    let write = origin
        .write("PRIVATE_PROTECTED_HISTORY_BODY 界\nnext line")
        .unwrap();
    let mut journal = PrWriteJournal::open(&database(&real.fixture)).unwrap();
    let target = PrWriteTarget::for_source(&origin.scope, REPO, "GENUINE_PR_NODE_1007", 7).unwrap();
    let permit = if legacy {
        journal.begin(&target, &write).unwrap()
    } else {
        journal.begin_thread_v1(&target, &write, &origin).unwrap()
    };
    let encoded = threads::publication_v1::encode_draft(
        "PRIVATE_PROTECTED_HISTORY_BODY 界\nnext line",
        Some(&permit.marker().publication_reference()),
    )
    .unwrap();
    Cache::open(&database(&real.fixture))
        .unwrap()
        .put_draft(&origin.scope, &origin.draft_key, &encoded)
        .unwrap();
    summary(&mut real);
    real.sql = sql_state(&real.fixture);
    (real, origin.draft_key)
}
fn load(
    real: &Real,
    intent: Arc<history::ThreadHistoryIntentV1>,
    previous: Option<(&Arc<history::PreparedThreadHistoryV1>, usize)>,
) -> Result<Arc<history::PreparedThreadHistoryV1>, threads::ThreadBrowseFailureV1> {
    history::load_history(
        intent,
        previous,
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
}
#[test]
fn review_threads_v1_history_actual_producer_pending_summary_route_opens_exact_protected_body_without_files_or_permission(
) {
    let mut extra = history_steps(42);
    extra.extend(history_steps(42));
    let (real, _) = setup(extra, false);
    assert!(real
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_resource
        .is_none());
    let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
    let list = load(&real, intent.clone(), None).unwrap();
    assert_eq!(list.labels.len(), 1);
    assert!(list.labels[0].contains("Pending"));
    assert!(list.detail.is_empty());
    let opened = load(&real, intent, Some((&list, 0))).unwrap();
    let detail = opened.detail.join("\n");
    assert!(detail.contains("PRIVATE_PROTECTED_HISTORY_BODY 界"));
    assert!(detail.contains("root REST #9009"));
    assert!(detail.contains(&"c".repeat(40)));
    assert!(
        detail.contains("not current thread locations")
            || opened
                .header
                .join(" ")
                .contains("not current thread locations")
    );
    assert!(detail.contains("no restore, approval, retry, discard or acknowledgement"));
    assert_eq!(opened.identity.account_id, Some(42));
    assert_eq!(opened.actor.label(), "@octocat");
    protocol(&real, 0);
}
#[test]
fn review_threads_v1_history_protected_legacy_metadata_only_never_reads_or_adopts_existing_private_body(
) {
    let mut extra = history_steps(42);
    extra.extend(history_steps(42));
    let (real, key) = setup(extra, true);
    let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
    let list = load(&real, intent.clone(), None).unwrap();
    let opened = load(&real, intent, Some((&list, 0))).unwrap();
    assert!(opened
        .detail
        .iter()
        .any(|line| line.contains("Protected legacy attempt")));
    assert!(!opened
        .detail
        .iter()
        .any(|line| line.contains("PRIVATE_PROTECTED_HISTORY_BODY")));
    assert!(Cache::open(&database(&real.fixture))
        .unwrap()
        .draft(&Scope::github(HOST, Some(42)).unwrap(), &key)
        .unwrap()
        .unwrap()
        .contains("PRIVATE_PROTECTED_HISTORY_BODY"));
    protocol(&real, 0);
}
#[test]
fn review_threads_v1_history_same_login_numeric_uid_loss_refuses_before_body_even_when_record_is_present(
) {
    let (real, _) = setup(history_steps(43), false);
    let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
    assert!(matches!(
        load(&real, intent, None),
        Err(threads::ThreadBrowseFailureV1::Source)
    ));
    protocol(&real, 0);
}
#[test]
fn review_threads_v1_history_exact_reference_body_fingerprint_and_current_generation_mismatch_never_return_private_body(
) {
    for mismatch in 0..3 {
        let mut extra = history_steps(42);
        extra.extend(history_steps(42));
        extra.truncate(12);
        let (mut real, key) = setup(extra, false);
        let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
        let list = load(&real, intent.clone(), None).unwrap();
        let connection = rusqlite::Connection::open(database(&real.fixture)).unwrap();
        if mismatch == 0 {
            connection
                .execute("UPDATE pr_write_scopes SET generation=generation+1", [])
                .unwrap();
        } else if mismatch == 1 {
            let mut value: Value = serde_json::from_str(
                &Cache::open(&database(&real.fixture))
                    .unwrap()
                    .draft(&Scope::github(HOST, Some(42)).unwrap(), &key)
                    .unwrap()
                    .unwrap(),
            )
            .unwrap();
            value["values"]["body"] = json!("PRIVATE_REPLACED_HISTORY_BODY");
            connection
                .execute(
                    "UPDATE drafts SET body=?1 WHERE resource=?2",
                    rusqlite::params![value.to_string(), key],
                )
                .unwrap();
        } else {
            connection
                .execute(
                    "UPDATE pr_write_attempts SET observed_head=?1",
                    ["d".repeat(40)],
                )
                .unwrap();
        }
        // Refusal occurs after only opening principal/repo/raw parent (four
        // children), never the final private return or another source fallback.
        real.sql = sql_state(&real.fixture);
        assert!(matches!(
            load(&real, intent, Some((&list, 0))),
            Err(threads::ThreadBrowseFailureV1::Source)
        ));
        protocol(&real, 0);
    }
}
#[test]
fn review_threads_v1_history_real_background_mixed_foreign_and_newer_back_veto_before_status_slot_or_private_effects(
) {
    for mode in [0, 1, 2, 3] {
        let (mut real, _) = setup(history_steps(42), false);
        native_v1::open_history(&mut real.app);
        let owner = real.app.scoped_threads_work_v1.take().unwrap();
        assert_eq!(owner.phase(), native_v1::ScopedThreadPhaseV1::HistoryList);
        let mut background = Background::new();
        assert!(background.submit_scoped_threads_v1(
            &mut real.app,
            owner.clone(),
            provider(&real.fixture),
            database(&real.fixture)
        ));
        channel::fault_next(&mut background, mode);
        let generation = channel::generation(&background);
        if mode >= 2 {
            real.app.github_action = Some(GithubAction::Back);
        }
        real.app.status = "NEWER_ORIGINAL_HISTORY_STATE".into();
        assert!(!background.poll(&mut real.app));
        assert_eq!(channel::generation(&background), generation);
        assert_eq!(real.app.status, "NEWER_ORIGINAL_HISTORY_STATE");
        assert!(real
            .app
            .scoped_thread_history_v1
            .as_ref()
            .unwrap()
            .listed
            .is_none());
        real.app.github_action = None;
        assert!(background.cancel_scoped_threads_v1(&mut real.app));
        protocol(&real, 0);
    }
}
fn genuine_failure_operation(
    panic: bool,
) -> (Real, Background, Arc<native_v1::ScopedThreadsOwnerV1>) {
    let (mut real, _) = setup(if panic { Vec::new() } else { history_steps(43) }, false);
    native_v1::open_history(&mut real.app);
    let mut owner = real.app.scoped_threads_work_v1.take().unwrap();
    let original_pointer = Arc::as_ptr(&owner);
    let original_deadline = owner.deadline();
    let original_cancel = owner.cancellation().clone();
    if panic {
        owner = native_v1::actual_worker_panic_for_test(owner);
    }
    assert_eq!(Arc::as_ptr(&owner), original_pointer);
    assert_eq!(owner.deadline(), original_deadline);
    assert!(owner.cancellation().same_instance(&original_cancel));
    assert_eq!(owner.phase(), native_v1::ScopedThreadPhaseV1::HistoryList);
    let mut background = Background::new();
    assert!(background.submit_scoped_threads_v1(
        &mut real.app,
        owner.clone(),
        provider(&real.fixture),
        database(&real.fixture),
    ));
    channel::actual_completion(&mut background, panic);
    (real, background, owner)
}
#[test]
fn review_threads_v1_history_actual_provider_refusal_and_background_caught_worker_panic_retire_only_current_original_private_source(
) {
    for panic in [false, true] {
        let (mut real, mut background, _) = genuine_failure_operation(panic);
        assert!(background.poll(&mut real.app));
        assert!(channel::generation(&background).is_none());
        assert!(real.app.scoped_threads_owner_v1.is_none());
        assert!(real.app.scoped_thread_history_v1.is_none());
        assert!(real.app.github.is_none());
        assert!(real.app.remote_scope_source.is_none());
        assert!(real.app.form.is_none());
        assert!(!real.app.loading[View::Inbox.index()]);
        assert!(!real.app.status.contains("PRIVATE_"));
        assert!(real.app.status.contains("refresh explicitly"));
        protocol(&real, 0);
    }
}
#[test]
fn review_threads_v1_history_actual_provider_refusal_and_caught_panic_cannot_touch_newer_intent_or_navigation(
) {
    for panic in [false, true] {
        for newer in 0..3 {
            let (mut real, mut background, owner) = genuine_failure_operation(panic);
            let generation = channel::generation(&background);
            let source = real.app.remote_scope_source.clone().unwrap();
            let navigation = real
                .app
                .github
                .as_ref()
                .unwrap()
                .scoped_pr_navigation
                .clone()
                .unwrap();
            match newer {
                0 => real.app.repository_load_requested = true,
                1 => real.app.github_action = Some(GithubAction::Back),
                2 => {
                    real.app.github.as_mut().unwrap().scoped_pr_navigation =
                        Some(Arc::new(navigation.as_ref().clone()));
                    real.app.overlay = Some(Overlay::Notice);
                    real.app.notice = "NEWER_NAVIGATION_PRIVATE_NOTICE".into();
                }
                _ => unreachable!(),
            }
            real.app.status = "NEWER_HISTORY_STATE_MUST_SURVIVE".into();
            real.app.hits.push(crate::app::HitRegion {
                rect: ratatui::layout::Rect::new(1, 1, 7, 1),
                action: crate::app::Action::Close,
            });
            let hit_count = real.app.hits.len();
            assert!(!background.poll(&mut real.app));
            assert_eq!(channel::generation(&background), generation);
            assert!(Arc::ptr_eq(
                real.app.scoped_threads_owner_v1.as_ref().unwrap(),
                &owner
            ));
            assert!(Arc::ptr_eq(
                real.app.remote_scope_source.as_ref().unwrap(),
                &source
            ));
            assert!(real.app.github.is_some());
            assert!(real
                .app
                .scoped_thread_history_v1
                .as_ref()
                .unwrap()
                .listed
                .is_none());
            assert_eq!(real.app.status, "NEWER_HISTORY_STATE_MUST_SURVIVE");
            assert_eq!(real.app.hits.len(), hit_count);
            assert!(real.app.loading[View::Inbox.index()]);
            if newer == 2 {
                assert_eq!(real.app.overlay, Some(Overlay::Notice));
                assert_eq!(real.app.notice, "NEWER_NAVIGATION_PRIVATE_NOTICE");
                assert!(!Arc::ptr_eq(
                    real.app
                        .github
                        .as_ref()
                        .unwrap()
                        .scoped_pr_navigation
                        .as_ref()
                        .unwrap(),
                    &navigation
                ));
            }
            real.app.repository_load_requested = false;
            real.app.github_action = None;
            assert!(background.cancel_scoped_threads_v1(&mut real.app));
            protocol(&real, 0);
        }
    }
}
#[test]
fn review_threads_v1_history_actual_closed_provider_refusal_mixed_or_foreign_owner_poison_cannot_retire_private_state(
) {
    for mode in [0, 1] {
        let (mut real, mut background, owner) = genuine_failure_operation(false);
        // This is deliberately labelled poison/veto coverage. The actual
        // provider refusal remains genuine; extra facet/owner removal is not.
        channel::fault_next(&mut background, mode);
        let generation = channel::generation(&background);
        real.app.status = "CURRENT_ORIGINAL_HISTORY_STATE".into();
        assert!(!background.poll(&mut real.app));
        assert_eq!(channel::generation(&background), generation);
        assert!(Arc::ptr_eq(
            real.app.scoped_threads_owner_v1.as_ref().unwrap(),
            &owner
        ));
        assert!(real.app.github.is_some() && real.app.remote_scope_source.is_some());
        assert!(real
            .app
            .scoped_thread_history_v1
            .as_ref()
            .unwrap()
            .listed
            .is_none());
        assert_eq!(real.app.status, "CURRENT_ORIGINAL_HISTORY_STATE");
        assert!(background.cancel_scoped_threads_v1(&mut real.app));
        protocol(&real, 0);
    }
}
#[test]
fn review_threads_v1_history_pure_render_80_120_has_shared_actor_fallback_all_key_click_controls_and_no_form(
) {
    for (width, height) in [(80, 24), (120, 40)] {
        let (mut real, _) = setup(history_steps(42), false);
        let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
        let list = load(&real, intent.clone(), None).unwrap();
        let mut view = history::ui::ThreadHistoryBrowserV1::pending(intent);
        view.listed = Some(list);
        real.app.scoped_thread_history_v1 = Some(view);
        real.app.overlay = Some(Overlay::PrThreadHistoryV1);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| crate::ui::draw(frame, &mut real.app))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        for word in [
            "[Enter Open]",
            "[r Refresh]",
            "[Esc Back]",
            "[q Quit]",
            "@renamed-same-positive-id",
            "Pending",
            "[RI]",
        ] {
            assert!(
                text.contains(word),
                "{word} must be in actual current terminal cells"
            );
        }
        assert!(real.app.form.is_none());
        for action in [
            history::ui::HistoryActionV1::Open,
            history::ui::HistoryActionV1::Refresh,
            history::ui::HistoryActionV1::Scroll(-10),
            history::ui::HistoryActionV1::Scroll(10),
            history::ui::HistoryActionV1::Home,
            history::ui::HistoryActionV1::Horizontal(-24),
            history::ui::HistoryActionV1::Horizontal(24),
            history::ui::HistoryActionV1::Back,
            history::ui::HistoryActionV1::Quit,
        ] {
            assert!(real
                .app
                .scoped_thread_history_v1
                .as_ref()
                .unwrap()
                .hits
                .iter()
                .any(|(rect, current)| *current == action && !rect.is_empty()));
        }
        history::ui::dispatch(&mut real.app, history::ui::HistoryActionV1::Back);
        assert!(real.app.scoped_thread_history_v1.is_none());
        assert_eq!(
            real.app.github.as_ref().unwrap().pr.as_ref().unwrap().tab,
            crate::github_workspace::PrTab::Summary
        );
        protocol(&real, 0);
    }
}
#[test]
fn review_threads_v1_history_idle_stale_overlay_drops_only_old_private_history_not_newer_navigation_notice_or_status(
) {
    let (mut real, _) = setup(history_steps(42), false);
    let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
    let list = load(&real, intent.clone(), None).unwrap();
    let mut view = history::ui::ThreadHistoryBrowserV1::pending(intent);
    view.listed = Some(list);
    real.app.scoped_thread_history_v1 = Some(view);
    let nav = real
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .as_ref()
        .unwrap()
        .clone();
    let newer = Arc::new(nav.as_ref().clone());
    real.app.github.as_mut().unwrap().scoped_pr_navigation = Some(newer.clone());
    real.app.overlay = Some(Overlay::Notice);
    real.app.notice = "NEWER_OWNED_NOTICE".into();
    real.app.status = "NEWER_STATUS_MUST_SURVIVE".into();
    real.app.github_action = Some(GithubAction::Back);
    real.app.hits.push(crate::app::HitRegion {
        rect: ratatui::layout::Rect::new(1, 1, 8, 1),
        action: crate::app::Action::Close,
    });
    let hits = real.app.hits.len();
    history::ui::retire_invalid(&mut real.app);
    assert!(real.app.scoped_thread_history_v1.is_none());
    assert_eq!(real.app.overlay, Some(Overlay::Notice));
    assert_eq!(real.app.notice, "NEWER_OWNED_NOTICE");
    assert_eq!(real.app.status, "NEWER_STATUS_MUST_SURVIVE");
    assert_eq!(real.app.hits.len(), hits);
    assert_eq!(real.app.github_action, Some(GithubAction::Back));
    assert!(!real.app.scoped_threads_cancel_v1);
    assert!(Arc::ptr_eq(
        real.app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .as_ref()
            .unwrap(),
        &newer
    ));
    assert!(real.app.github.as_ref().unwrap().pr.is_some());
    protocol(&real, 0);
}
#[test]
fn review_threads_v1_history_actual_visible_body_then_idle_actual_numeric_principal_loss_cannot_reveal_github_underlay(
) {
    let mut extra = history_steps(42);
    extra.extend(history_steps(42));
    extra.extend(principal(43, "octocat"));
    let (mut real, _) = setup(extra, false);
    let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
    let list = load(&real, intent.clone(), None).unwrap();
    let opened = load(&real, intent.clone(), Some((&list, 0))).unwrap();
    let mut view = history::ui::ThreadHistoryBrowserV1::pending(intent);
    view.listed = Some(list);
    view.opened = Some(opened);
    real.app.scoped_thread_history_v1 = Some(view);
    real.app.overlay = Some(Overlay::PrThreadHistoryV1);
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut real.app))
        .unwrap();
    let before = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(before.contains("PRIVATE_PROTECTED_HISTORY_BODY"));
    let current = provider(&real.fixture)
        .identity(&Default::default())
        .unwrap();
    assert_eq!(current.account_id, Some(43));
    assert_eq!(current.login, "octocat");
    let github = real.app.github.as_mut().unwrap();
    github.identity = current.clone();
    github.inbox.identity = current;
    terminal
        .draw(|frame| crate::ui::draw(frame, &mut real.app))
        .unwrap();
    let after = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>();
    assert!(!after.contains("PRIVATE_PROTECTED_HISTORY_BODY"));
    assert!(!after.contains("Recorded intent account"));
    assert!(!after.contains(history::ui::TITLE));
    assert!(real.app.github.is_none());
    assert!(real.app.scoped_thread_history_v1.is_none());
    assert!(real.app.remote_scope_source.is_none());
    assert!(real.app.avatar_states.is_empty());
    protocol(&real, 0);
}
