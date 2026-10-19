//! Genuine factory-backed controller/storage declarations. UNRUN until Root's
//! serialized verification. Direct tests supplement, never replace PTYs.
use super::*;
use crate::{
    pr_write_journal::{
        thread_comment_history_v2::{CommentActionV2, ThreadCommentOriginSeedV2},
        PrWriteJournal, PrWriteTarget,
    },
    scoped_pr_observed_tabs_v1::resources::threads_v1::{
        comments_v2 as comments, history_v1 as history,
    },
};
use comments::publication::{self as publish, ui as composer};
#[path = "scoped_pr_thread_historical_v5_tests.rs"]
mod historical_v5;
#[path = "scoped_pr_thread_comment_lifecycle_v2_tests.rs"]
mod lifecycle_v2;
#[path = "scoped_pr_thread_reaction_controller_v3_tests.rs"]
mod reactions_v3;

fn fresh_comment_steps() -> Vec<GhStep> {
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(observation(selected_comment(), current_rest()));
    steps.extend(close_steps());
    assert_eq!(steps.len(), 16);
    steps
}
fn browser(extra: Vec<GhStep>) -> Real {
    let mut steps = thread_steps(Some("LINE"));
    steps.extend(observation_steps(selected_node(false, true)));
    steps.extend(extra);
    let mut real = real(steps);
    let prepared = read(&real).unwrap();
    let mut view = ui_v1::ThreadBrowserV1::pending(prepared.intent.clone());
    view.prepared = Some(prepared);
    real.app.scoped_threads_v1 = Some(view);
    real.app.overlay = Some(Overlay::PrThreadsV1);
    let intent = comments::ThreadCommentsIntentV2::capture(&real.app).unwrap();
    let prepared = comments::load(
        intent.clone(),
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let mut panel = comments::ui::ThreadCommentsBrowserV2::pending(intent);
    panel.prepared = Some(prepared);
    real.app.scoped_threads_v1.as_mut().unwrap().comments_v2 = Some(panel);
    real
}
fn intent(real: &Real, action: CommentActionV2) -> Arc<publish::ThreadCommentIntentV2> {
    publish::ThreadCommentIntentV2::capture(&real.app, action).unwrap()
}
fn open(real: &Real, intent: Arc<publish::ThreadCommentIntentV2>) -> crate::forms::Form {
    composer::form_from_draft(
        publish::CommentDraftV2::open(
            intent,
            &provider(&real.fixture),
            &database(&real.fixture),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap(),
    )
    .unwrap()
}
fn origin(real: &Real, action: CommentActionV2) -> ThreadCommentOriginSeedV2 {
    with_original_limits(real, 20, |a| {
        let actual = observe(&a).unwrap();
        let origin = ThreadCommentOriginSeedV2::from_producer(
            &database(&real.fixture),
            a.identity,
            &actual,
            action,
        )
        .unwrap();
        (a.close)().unwrap();
        origin
    })
}
fn producer_real() -> Real {
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(observation(selected_comment(), current_rest()));
    steps.extend(close_steps());
    real(steps)
}
fn history_steps() -> Vec<GhStep> {
    let mut steps = principal(42, "current-inspector");
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), resource_parent(REPO)));
    steps.push(get(&format!("repos/{REPO}"), repo(410, REPO)));
    steps.extend(principal(42, "renamed-final-inspector"));
    assert_eq!(steps.len(), 8);
    steps
}
fn summary(real: &mut Real) {
    real.app.overlay = None;
    real.app.scoped_threads_v1 = None;
    real.app.inbox_list.selected = 0;
    real.app.inbox_list.top = 0;
    let github = real.app.github.as_mut().unwrap();
    github.scoped_pr_resource = None;
    let page = github.pr.as_mut().unwrap();
    page.tab = crate::github_workspace::PrTab::Summary;
    page.prepare_rows().unwrap();
}

#[test]
fn review_threads_v2_comment_genuine_panel_author_c_h_and_shared_fallback_no_write_permission() {
    let real = browser(vec![]);
    let panel = real
        .app
        .scoped_threads_v1
        .as_ref()
        .unwrap()
        .comments_v2
        .as_ref()
        .unwrap();
    let data = panel.prepared.as_ref().unwrap();
    assert_eq!(data.identity.account_id, Some(42));
    assert_eq!(data.rows[0].actor.label(), "@thread-actor");
    assert!(data.rows[0]
        .lines
        .iter()
        .any(|line| line.contains(&"c".repeat(40))));
    assert!(data
        .header
        .iter()
        .any(|line| line.contains(&"a".repeat(40))));
    assert!(data.rows[0]
        .lines
        .iter()
        .any(|line| line.contains("not a canonical")));
    assert!(data.rows[0]
        .lines
        .iter()
        .any(|line| line.contains("Created") && line.contains("Updated")));
    protocol(&real, 0);
}
#[test]
fn review_threads_v2_comment_edit_form_exact_ticket_caps_editor_and_busy_preview_paste() {
    let mut extra = fresh_comment_steps();
    extra.extend(fresh_comment_steps());
    let real = browser(extra);
    let selected = intent(&real, CommentActionV2::Edit);
    let mut form = open(&real, selected);
    assert_eq!(form.fields[0].editor.text, BODY);
    composer::replace_editor_result(&mut form, "replacement 界").unwrap();
    let request = composer::request(&form).unwrap();
    let preview = publish::CommentPreviewV2::prepare(
        request.intent().clone(),
        request.body().clone(),
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains("octocat") && line.contains("42")));
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains("thread-actor")));
    assert!(composer::accept_preview(&mut form, &request, preview));
    let approved = composer::request(&form).unwrap();
    form.paste("POISON");
    assert!(approved.matches_form(&form));
    assert_eq!(form.fields[0].editor.text, "replacement 界");
    form.preview = false;
    form.submitting = true;
    form.paste("BUSY_POISON");
    assert!(approved.matches_form(&form));
    form.submitting = false;
    assert!(
        composer::replace_editor_result(&mut form, &"x".repeat(publish::BODY_CAP + 1)).is_err()
    );
    assert!(
        composer::replace_editor_result(&mut form, &"\u{1}".repeat(publish::BODY_CAP)).is_err()
    );
    composer::replace_editor_result(&mut form, "replacement 界").unwrap();
    assert!(!approved.matches_form(&form));
    assert!(composer::confirmed_preview(&form).is_err());
    protocol(&real, 0);
}
#[test]
fn review_threads_v2_comment_delete_has_no_body_or_external_editor_and_disabled_input() {
    let real = browser(fresh_comment_steps());
    let mut form = open(&real, intent(&real, CommentActionV2::Delete));
    assert!(form.fields.is_empty());
    assert!(composer::request(&form).unwrap().body().is_empty());
    assert!(composer::replace_editor_result(&mut form, "must not enter deletion").is_err());
    form.paste("ignored");
    assert!(form.fields.is_empty());
    let encoded: Value = serde_json::from_str(&composer::encoded_draft(&form).unwrap()).unwrap();
    assert_eq!(encoded["values"]["body"], "");
    assert_eq!(encoded["publication"], Value::Null);
    protocol(&real, 0);
}
#[test]
fn review_threads_v2_comment_closed_origin_atomic_failure_rolls_back_attempt_generation_and_no_post(
) {
    let mut real = producer_real();
    let seed = origin(&real, CommentActionV2::Edit);
    let write = seed.write("exact requested change").unwrap();
    let path = database(&real.fixture);
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let target = PrWriteTarget::for_source(&seed.scope, REPO, &seed.pr_node, seed.number).unwrap();
    let generation = journal.source_generation(&target).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER refuse_real_comment_origin BEFORE INSERT ON pr_write_thread_comment_origins_v2 BEGIN SELECT RAISE(ABORT,'actual metadata failure'); END;").unwrap();
    assert!(journal
        .begin_thread_comment_v2(&target, &write, &seed)
        .is_err());
    assert_eq!(journal.source_generation(&target).unwrap(), generation);
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_thread_comment_origins_v2",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    real.sql = sql_state(&real.fixture);
    exact_protocol(&real, 0);
}
#[test]
fn review_threads_v2_comment_origin_binds_original_db_typed_ids_and_immutable_record_cleanup() {
    let mut real = producer_real();
    let seed = origin(&real, CommentActionV2::Delete);
    let write = seed.write("").unwrap();
    let path = database(&real.fixture);
    let target = PrWriteTarget::for_source(&seed.scope, REPO, &seed.pr_node, seed.number).unwrap();
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let other = real.fixture.root.join("other-private-comment.sqlite");
    let mut foreign = PrWriteJournal::open(&other).unwrap();
    assert!(foreign
        .begin_thread_comment_v2(&target, &write, &seed)
        .is_err());
    let permit = journal
        .begin_thread_comment_v2(&target, &write, &seed)
        .unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    let bytes: usize = db
        .query_row(
            "SELECT length(CAST(origin_json AS BLOB)) FROM pr_write_thread_comment_origins_v2",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(bytes > 0 && bytes <= 16 * 1024);
    assert!(db
        .execute(
            "UPDATE pr_write_thread_comment_origins_v2 SET origin_json='{}'",
            []
        )
        .is_err());
    let snapshot = journal.thread_comment_history_v2(&target, 410, "GENUINE_REPO_NODE_410", 1007);
    // The exact node is taken from the genuine producer, not the display name.
    assert!(snapshot.is_err());
    let snapshot = journal
        .thread_comment_history_v2(
            &target,
            seed.repository_id,
            &seed.repository_node,
            seed.pr_id,
        )
        .unwrap();
    assert_eq!(snapshot.rows.len(), 1);
    assert_eq!(snapshot.rows[0].origin.as_ref().unwrap().seed, seed);
    db.execute(
        "DELETE FROM pr_write_attempts WHERE attempt_id=?1",
        [permit.marker().attempt_id],
    )
    .unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_thread_comment_origins_v2",
            [],
            |row| row.get::<_, u64>(0)
        )
        .unwrap(),
        0
    );
    real.sql = sql_state(&real.fixture);
    exact_protocol(&real, 0);
}
#[test]
fn review_threads_v2_comment_summary_history_exact_protected_body_legacy_no_body_and_uid_loss() {
    for legacy in [false, true] {
        let mut steps = opening_steps(selected_node(false, true));
        steps.extend(observation(selected_comment(), current_rest()));
        steps.extend(close_steps());
        steps.extend(history_steps());
        steps.extend(history_steps());
        let mut real = real(steps);
        let seed = origin(&real, CommentActionV2::Edit);
        let write = seed.write("PRIVATE_COMMENT_RESTART_BODY 界").unwrap();
        let target =
            PrWriteTarget::for_source(&seed.scope, REPO, &seed.pr_node, seed.number).unwrap();
        let mut journal = PrWriteJournal::open(&database(&real.fixture)).unwrap();
        let permit = if legacy {
            journal.begin(&target, &write).unwrap()
        } else {
            journal
                .begin_thread_comment_v2(&target, &write, &seed)
                .unwrap()
        };
        let encoded = publish::encode_draft(
            "PRIVATE_COMMENT_RESTART_BODY 界",
            Some(&permit.marker().publication_reference()),
        )
        .unwrap();
        Cache::open(&database(&real.fixture))
            .unwrap()
            .put_draft(&seed.scope, &seed.draft_key().unwrap(), &encoded)
            .unwrap();
        summary(&mut real);
        real.sql = sql_state(&real.fixture);
        let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
        let list = history::comments_v2::load(
            intent.clone(),
            None,
            &provider(&real.fixture),
            &database(&real.fixture),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let detail = history::comments_v2::load(
            intent,
            Some((&list, 0)),
            &provider(&real.fixture),
            &database(&real.fixture),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(
            detail
                .detail
                .iter()
                .any(|line| line.contains("PRIVATE_COMMENT_RESTART_BODY")),
            !legacy
        );
        assert!(detail
            .detail
            .iter()
            .any(|line| line.contains("INSPECT ONLY")));
        assert!(detail.detail.iter().any(|line| line.contains(if legacy {
            "legacy"
        } else {
            "Historical base"
        })));
        exact_protocol(&real, 0);
    }
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(observation(selected_comment(), current_rest()));
    steps.extend(close_steps());
    steps.extend(principal(43, "octocat"));
    let mut real = real(steps);
    let _ = origin(&real, CommentActionV2::Edit);
    summary(&mut real);
    let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
    assert!(history::comments_v2::load(
        intent,
        None,
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30)
    )
    .is_err());
    exact_protocol(&real, 0);
}
#[test]
fn review_threads_v2_comment_genuine_worker_refusal_and_panic_current_private_retirement_newer_intent_veto(
) {
    for panic in [false, true] {
        for newer in [false, true] {
            let mut real = browser(if panic {
                vec![]
            } else {
                principal(43, "octocat")
            });
            native_v1::open_comment_publication_v2(&mut real.app, CommentActionV2::Edit);
            let mut owner = real.app.scoped_threads_work_v1.take().unwrap();
            let pointer = Arc::as_ptr(&owner);
            let deadline = owner.deadline();
            let token = owner.cancellation().clone();
            if panic {
                owner = native_v1::actual_worker_panic_for_test(owner);
            }
            assert_eq!(Arc::as_ptr(&owner), pointer);
            assert_eq!(owner.deadline(), deadline);
            assert!(owner.cancellation().same_instance(&token));
            let mut background = Background::new();
            assert!(background.submit_scoped_threads_v1(
                &mut real.app,
                owner,
                provider(&real.fixture),
                database(&real.fixture)
            ));
            channel::actual_completion(&mut background, panic);
            if newer {
                real.app.github_action = Some(GithubAction::Back);
                real.app.status = "NEWER_COMMENT_PRESENTATION_MUST_SURVIVE".into();
                let generation = channel::generation(&background);
                assert!(!background.poll(&mut real.app));
                assert_eq!(channel::generation(&background), generation);
                assert_eq!(real.app.status, "NEWER_COMMENT_PRESENTATION_MUST_SURVIVE");
                assert!(real
                    .app
                    .scoped_threads_v1
                    .as_ref()
                    .unwrap()
                    .comments_v2
                    .is_some());
                real.app.github_action = None;
                assert!(background.cancel_scoped_threads_v1(&mut real.app));
            } else {
                assert!(background.poll(&mut real.app));
                assert!(real.app.scoped_threads_v1.is_none());
                assert!(real.app.form.is_none());
                assert!(channel::generation(&background).is_none());
            }
            protocol(&real, 0);
        }
    }
}
#[test]
fn review_threads_v2_comment_real_selection_and_panel_back_retire_only_exact_queued_owner_no_io() {
    let mut real = browser(vec![]);
    native_v1::open_comment_publication_v2(&mut real.app, CommentActionV2::Delete);
    let owner = real.app.scoped_threads_work_v1.as_ref().unwrap().clone();
    assert!(owner.presentation_matches(&real.app));
    native_v1::close_comments_v2(&mut real.app);
    assert!(owner.cancellation().is_cancelled());
    assert!(real.app.scoped_threads_work_v1.is_none());
    assert!(real
        .app
        .scoped_threads_v1
        .as_ref()
        .unwrap()
        .comments_v2
        .is_none());
    assert_eq!(real.app.overlay, Some(Overlay::PrThreadsV1));
    protocol(&real, 0);
}
#[test]
fn review_threads_v2_comment_preparation_actual_source_clear_is_rechecked_before_final_private_return(
) {
    let mut real = browser(opening_steps(selected_node(false, true)));
    let intent = comments::ThreadCommentsIntentV2::capture(&real.app).unwrap();
    let original_pointer = Arc::as_ptr(&intent);
    let path = database(&real.fixture);
    let scope = intent.original().scope().clone();
    let invoked = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let count = invoked.clone();
    let intent = comments::after_preparation_for_test(
        intent,
        Arc::new(move || {
            count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Cache::open(&path).unwrap().clear_records(&scope).unwrap();
        }),
    );
    assert_eq!(Arc::as_ptr(&intent), original_pointer);
    assert!(matches!(
        comments::load(
            intent,
            &provider(&real.fixture),
            &database(&real.fixture),
            &Default::default(),
            Instant::now() + Duration::from_secs(30)
        ),
        Err(threads::ThreadBrowseFailureV1::Source)
    ));
    assert_eq!(invoked.load(std::sync::atomic::Ordering::SeqCst), 1);
    real.sql = sql_state(&real.fixture);
    protocol(&real, 0);
}
#[test]
fn review_threads_v2_comment_stale_nested_paint_and_event_cannot_expose_body_under_new_source_or_genuine_new_navigation(
) {
    for same_source in [false, true] {
        let mut extra = vec![];
        if same_source {
            extra.extend(operation(REPO, 42));
            extra.extend(resource_steps(PrResourceKindV1::Files, REPO));
        }
        let mut real = browser(extra);
        let old_view = real.app.scoped_threads_v1.take().unwrap();
        let mut independent = None;
        if same_source {
            let source = real.app.remote_scope_source.as_ref().unwrap().clone();
            let active = real.app.remote_scope.as_ref().unwrap().clone();
            let mut workspace = call(
                &real.fixture,
                source.clone(),
                active.clone(),
                previous(&source),
            )
            .unwrap()
            .workspace;
            let result = run_resource(
                &real.fixture,
                source,
                active,
                &mut workspace,
                PrResourceKindV1::Files,
                &Default::default(),
            )
            .unwrap();
            install_resource_if_current(&mut workspace, result).unwrap();
            real.app.inbox_list.selected = (0..workspace.pr.as_ref().unwrap().rows.len())
                .find(|index| {
                    matches!(
                        workspace.scoped_pr_resource.as_ref().unwrap().row(*index),
                        Some(PrResourceRowV1::FileList(0))
                    )
                })
                .unwrap();
            real.app.github = Some(workspace);
            real.app.overlay = None;
            real.sql = sql_state(&real.fixture);
        } else {
            let mut newer = super::super::super::real(vec![]);
            real.app = std::mem::take(&mut newer.app);
            independent = Some(newer);
        }
        native_v1::open(&mut real.app);
        let successor = real.app.scoped_threads_work_v1.as_ref().unwrap().clone();
        let source = real.app.remote_scope_source.clone().unwrap();
        let nav = real
            .app
            .github
            .as_ref()
            .unwrap()
            .scoped_pr_navigation
            .clone()
            .unwrap();
        real.app.scoped_threads_v1 = Some(old_view);
        real.app.overlay = Some(Overlay::PrThreadsV1);
        real.app.status = "NEWER INBOX OWNER MUST SURVIVE STALE COMMENT DRAW".into();
        let loading = real.app.loading;
        for (width, height) in [(80, 24), (120, 40)] {
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal
                .draw(|frame| comments::ui::draw(frame, &mut real.app, frame.area()))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(!text.contains("Original private thread body"));
            assert!(!text.contains("thread-actor"));
            assert!(!text.contains("Comment #9009"));
            assert!(comments::ui::event(
                &mut real.app,
                &crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Char('d'),
                    crossterm::event::KeyModifiers::NONE
                ))
            ));
            assert!(real.app.form.is_none());
            assert!(Arc::ptr_eq(
                real.app.scoped_threads_work_v1.as_ref().unwrap(),
                &successor
            ));
            assert!(Arc::ptr_eq(
                real.app.remote_scope_source.as_ref().unwrap(),
                &source
            ));
            assert!(Arc::ptr_eq(
                real.app
                    .github
                    .as_ref()
                    .unwrap()
                    .scoped_pr_navigation
                    .as_ref()
                    .unwrap(),
                &nav
            ));
            assert_eq!(real.app.loading, loading);
            assert_eq!(
                real.app.status,
                "NEWER INBOX OWNER MUST SURVIVE STALE COMMENT DRAW"
            );
        }
        protocol(&real, 0);
        if let Some(newer) = independent {
            protocol(&newer, 0);
        }
    }
}
