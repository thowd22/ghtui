//! Factory-backed reaction consumer declarations. No fabricated permission,
//! source/owner constructor or manufactured completion. UNRUN in this lane.
use super::*;
use crate::{
    pr_write_journal::thread_reaction_history_v3::{
        ReactionActionV3 as Action, ThreadReactionOriginSeedV3,
    },
    providers::gh::ThreadReactionContentV3 as Content,
};
use comments::reactions_v3::{self as reactions, form as reaction_form};

fn reaction(id: u64, uid: Option<u64>, content: &str) -> Value {
    json!({"id":id,"node_id":format!("ACTUAL_REACTION_NODE_{id}"),"content":content,
        "user":uid.map(|id|json!({"id":id,"login":"same-login-not-ownership","type":"User","avatar_url":null})),
        "created_at":"2026-10-03T00:00:00Z"})
}
fn actors() -> Value {
    json!([
        reaction(7001, Some(42), "heart"),
        reaction(7002, Some(99), "eyes"),
        reaction(7003, None, "rocket")
    ])
}
fn actor_steps(page: Value) -> Vec<GhStep> {
    let mut steps = write_guard(42, "renamed-numeric-viewer");
    steps.push(get(
        &format!("repos/{REPO}/pulls/comments/9009"),
        current_rest(),
    ));
    steps.push(get(
        &format!("repos/{REPO}/pulls/comments/9009/reactions?per_page=100&page=1"),
        page,
    ));
    steps
}
fn fresh_reactions() -> Vec<GhStep> {
    fresh_reactions_with_page(actors())
}
fn fresh_reactions_with_page(page: Value) -> Vec<GhStep> {
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(observation(selected_comment(), current_rest()));
    steps.extend(actor_steps(page));
    steps.extend(close_steps());
    assert_eq!(steps.len(), 21);
    steps
}
fn reaction_browser(extra: Vec<GhStep>) -> Real {
    reaction_browser_with_page(actors(), extra)
}
fn reaction_browser_with_page(page: Value, extra: Vec<GhStep>) -> Real {
    let mut steps = fresh_reactions_with_page(page);
    steps.extend(extra);
    let mut real = browser(steps);
    let intent = reactions::ThreadReactionsIntentV3::capture(&real.app).unwrap();
    let prepared = reactions::load(
        intent.clone(),
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let mut view = reactions::ui::ThreadReactionsBrowserV3::pending(intent);
    view.prepared = Some(prepared);
    real.app
        .scoped_threads_v1
        .as_mut()
        .unwrap()
        .comments_v2
        .as_mut()
        .unwrap()
        .reactions_v3 = Some(view);
    real
}
fn selected(real: &Real, action: Action) -> Arc<reactions::ThreadReactionIntentV3> {
    reactions::ThreadReactionIntentV3::capture(&real.app, action).unwrap()
}
fn seed(real: &Real, action: Action) -> ThreadReactionOriginSeedV3 {
    let view = real
        .app
        .scoped_threads_v1
        .as_ref()
        .unwrap()
        .comments_v2
        .as_ref()
        .unwrap()
        .reactions_v3
        .as_ref()
        .unwrap();
    let data = view.prepared.as_ref().unwrap();
    ThreadReactionOriginSeedV3::from_producer(
        &database(&real.fixture),
        &data.identity,
        &data.actual,
        action,
    )
    .unwrap()
}
fn form(real: &Real, action: Action) -> crate::forms::Form {
    reaction_form::form_from_draft(
        reactions::ReactionDraftV3::open(
            selected(real, action),
            &provider(&real.fixture),
            &database(&real.fixture),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap(),
    )
    .unwrap()
}
#[test]
fn review_threads_v3_reaction_horizontal_keys_and_clicks_reveal_complete_bounded_unicode_actor() {
    let suffix = "UNIQUE_REACTION_ACTOR_SUFFIX";
    let login = format!("{}界e\u{301}_{suffix}", "long_".repeat(780));
    let mut actor = reaction(7001, Some(42), "heart");
    actor["user"]["login"] = json!(login);
    let mut real = reaction_browser_with_page(json!([actor]), vec![]);
    for (width, height) in [(80, 24), (120, 40)] {
        reactions::ui::dispatch(&mut real.app, reactions::ui::ReactionUiV3::Top);
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| reactions::ui::draw(frame, &mut real.app, frame.area()))
            .unwrap();
        let before = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!before.contains(suffix));
        let data = real
            .app
            .scoped_threads_v1
            .as_ref()
            .unwrap()
            .comments_v2
            .as_ref()
            .unwrap()
            .reactions_v3
            .as_ref()
            .unwrap()
            .prepared
            .as_ref()
            .unwrap();
        let heading = &data.rows[0].heading;
        let prefix = &heading[..heading.find(suffix).unwrap()];
        let columns = ratatui::text::Line::from(prefix).width();
        for _ in 0..columns / 24 {
            assert!(reactions::ui::event(
                &mut real.app,
                &crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
                    crossterm::event::KeyCode::Right,
                    crossterm::event::KeyModifiers::NONE
                ))
            ));
        }
        terminal
            .draw(|frame| reactions::ui::draw(frame, &mut real.app, frame.area()))
            .unwrap();
        let after = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(
            after.contains(suffix),
            "Full actual actor label must be reachable, not only header/detail scrolling"
        );
        let left = reactions::ui::control_hits_for_test(&real.app)
            .into_iter()
            .find(|(_, action)| *action == reactions::ui::ReactionUiV3::Horizontal(-24))
            .unwrap()
            .0;
        assert!(reactions::ui::event(
            &mut real.app,
            &crossterm::event::Event::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: left.x,
                row: left.y,
                modifiers: crossterm::event::KeyModifiers::NONE
            })
        ));
        terminal
            .draw(|frame| reactions::ui::draw(frame, &mut real.app, frame.area()))
            .unwrap();
        let clicked = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert_ne!(after, clicked);
        assert!(real.app.form.is_none());
        assert!(real.app.scoped_threads_work_v1.is_none());
    }
    protocol(&real, 0);
}
#[test]
fn review_threads_v3_reaction_panel_actual_numeric_actors_unknown_avatar_and_disabled_remove_parity(
) {
    let mut real = reaction_browser(vec![]);
    let view = real
        .app
        .scoped_threads_v1
        .as_ref()
        .unwrap()
        .comments_v2
        .as_ref()
        .unwrap()
        .reactions_v3
        .as_ref()
        .unwrap();
    let data = view.prepared.as_ref().unwrap();
    assert_eq!(data.rows.len(), 3);
    assert!(data.rows[0].owned);
    assert!(!data.rows[1].owned);
    assert!(!data.rows[2].owned);
    assert_eq!(data.actual.rows()[1].account_id(), Some(99));
    assert_eq!(data.rows[0].actor.label(), data.rows[1].actor.label());
    assert!(data.rows[2].actor.label().contains("unknown"));
    assert!(data.header.iter().any(|line| line.contains("COMPLETE")));
    assert!(data
        .header
        .iter()
        .any(|line| line.contains(&"c".repeat(40)) && line.contains(&"a".repeat(40))));
    reactions::ui::dispatch(&mut real.app, reactions::ui::ReactionUiV3::Select(1));
    for (width, height) in [(80, 24), (120, 40)] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| reactions::ui::draw(frame, &mut real.app, frame.area()))
            .unwrap();
        let hits = reactions::ui::control_hits_for_test(&real.app);
        assert!(hits
            .iter()
            .any(|(_, action)| matches!(action, reactions::ui::ReactionUiV3::Add(Content::Heart))));
        assert!(!hits
            .iter()
            .any(|(_, action)| *action == reactions::ui::ReactionUiV3::Remove));
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(text.contains("same-login-not-ownership"));
        assert!(text.contains("Selected comment reactions"));
        assert!(reactions::ui::event(
            &mut real.app,
            &crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('r'),
                crossterm::event::KeyModifiers::NONE
            ))
        ));
        assert!(real.app.scoped_threads_work_v1.is_none());
        assert!(real.app.form.is_none());
    }
    protocol(&real, 0);
}
#[test]
fn review_threads_v3_reaction_no_body_form_original_ticket_preview_and_readonly_paste() {
    let mut extra = fresh_reactions();
    extra.extend(fresh_reactions());
    let real = reaction_browser(extra);
    let mut form = form(&real, Action::Add(Content::Heart));
    assert!(form.fields.is_empty());
    assert!(form.dirty);
    let request = reaction_form::request(&form).unwrap();
    let preview = reactions::ReactionPreviewV3::prepare(
        request.intent().clone(),
        Arc::from(""),
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(preview.lines().iter().any(|line| line.contains("heart")));
    assert!(preview.lines().iter().any(|line| line.contains("42")));
    assert!(reaction_form::accept_preview(&mut form, &request, preview));
    let approved = reaction_form::request(&form).unwrap();
    form.paste("POISON");
    assert!(approved.matches_form(&form));
    assert!(form.fields.is_empty());
    let encoded: Value =
        serde_json::from_str(&reaction_form::encoded_draft(&form).unwrap()).unwrap();
    assert_eq!(encoded["version"], 3);
    assert_eq!(encoded["values"], json!({"body":""}));
    assert_eq!(encoded["publication"], Value::Null);
    form.invalidate_plan();
    assert!(!approved.matches_form(&form));
    assert!(reaction_form::confirmed_preview(&form).is_err());
    protocol(&real, 0);
}
#[test]
fn review_threads_v3_reaction_atomic_origin_original_db_bound_and_failure_rolls_back_without_post()
{
    let mut real = reaction_browser(vec![]);
    let seed = seed(&real, Action::Remove(7001));
    let write = seed.write().unwrap();
    let path = database(&real.fixture);
    let target = PrWriteTarget::for_source(&seed.scope, REPO, &seed.pr_node, seed.number).unwrap();
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let foreign = real.fixture.root.join("foreign-reaction.sqlite");
    assert!(PrWriteJournal::open(&foreign)
        .unwrap()
        .begin_thread_reaction_v3(&target, &write, &seed)
        .is_err());
    let generation = journal.source_generation(&target).unwrap();
    let db = rusqlite::Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER refuse_reaction_origin BEFORE INSERT ON pr_write_thread_reaction_origins_v3 BEGIN SELECT RAISE(ABORT,'actual origin write failure'); END;").unwrap();
    assert!(journal
        .begin_thread_reaction_v3(&target, &write, &seed)
        .is_err());
    assert_eq!(journal.source_generation(&target).unwrap(), generation);
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_attempts", [], |row| row
            .get::<_, u64>(0))
            .unwrap(),
        0
    );
    db.execute_batch("DROP TRIGGER refuse_reaction_origin")
        .unwrap();
    let mut oversized = seed.clone();
    oversized.observed_login = "x".repeat(4097);
    assert!(journal
        .begin_thread_reaction_v3(&target, &write, &oversized)
        .is_err());
    let permit = journal
        .begin_thread_reaction_v3(&target, &write, &seed)
        .unwrap();
    let bytes: usize = db
        .query_row(
            "SELECT length(CAST(origin_json AS BLOB)) FROM pr_write_thread_reaction_origins_v3",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!((1..=16384).contains(&bytes));
    assert!(db
        .execute(
            "UPDATE pr_write_thread_reaction_origins_v3 SET origin_json='{}'",
            []
        )
        .is_err());
    let snapshot = journal
        .thread_reaction_history_v3(
            &target,
            seed.repository_id,
            &seed.repository_node,
            seed.pr_id,
        )
        .unwrap();
    assert_eq!(snapshot.rows[0].origin.as_ref().unwrap().seed, seed);
    assert_eq!(
        snapshot.rows[0].marker.publication_reference(),
        permit.marker().publication_reference()
    );
    db.execute(
        "DELETE FROM pr_write_attempts WHERE attempt_id=?1",
        [permit.marker().attempt_id],
    )
    .unwrap();
    assert_eq!(
        db.query_row(
            "SELECT count(*) FROM pr_write_thread_reaction_origins_v3",
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
fn review_threads_v3_reaction_summary_history_protected_exact_origin_legacy_and_account_loss() {
    for legacy in [false, true] {
        let mut extra = history_steps();
        extra.extend(history_steps());
        let mut real = reaction_browser(extra);
        let seed = seed(&real, Action::Add(Content::Heart));
        let write = seed.write().unwrap();
        let path = database(&real.fixture);
        let target =
            PrWriteTarget::for_source(&seed.scope, REPO, &seed.pr_node, seed.number).unwrap();
        let mut journal = PrWriteJournal::open(&path).unwrap();
        let permit = if legacy {
            journal.begin(&target, &write).unwrap()
        } else {
            journal
                .begin_thread_reaction_v3(&target, &write, &seed)
                .unwrap()
        };
        let encoded =
            reactions::encode_draft("", Some(&permit.marker().publication_reference())).unwrap();
        Cache::open(&path)
            .unwrap()
            .put_draft(&seed.scope, &seed.draft_key().unwrap(), &encoded)
            .unwrap();
        summary(&mut real);
        real.sql = sql_state(&real.fixture);
        let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
        let list = history::reactions_v3::load(
            intent.clone(),
            None,
            &provider(&real.fixture),
            &path,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let detail = history::reactions_v3::load(
            intent,
            Some((&list, 0)),
            &provider(&real.fixture),
            &path,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert!(detail
            .detail
            .iter()
            .any(|line| line.contains("INSPECT ONLY")));
        assert_eq!(
            detail
                .detail
                .iter()
                .any(|line| line.contains("Exact protected LOCAL no-body")),
            !legacy
        );
        assert!(detail.detail.iter().any(|line| line.contains(if legacy {
            "legacy"
        } else {
            "Historical reaction action"
        })));
        assert!(detail.detail.iter().all(|line| !line.contains(BODY)));
        exact_protocol(&real, 0);
    }
    let mut real = reaction_browser(principal(43, "octocat"));
    summary(&mut real);
    let intent = history::ThreadHistoryIntentV1::capture(&real.app).unwrap();
    assert!(history::reactions_v3::load(
        intent,
        None,
        &provider(&real.fixture),
        &database(&real.fixture),
        &Default::default(),
        Instant::now() + Duration::from_secs(30)
    )
    .is_err());
    protocol(&real, 0);
}
#[test]
fn review_threads_v3_reaction_real_worker_refusal_caught_panic_and_newer_owner_veto() {
    for panic in [false, true] {
        for newer in [false, true] {
            let mut real = reaction_browser(if panic {
                vec![]
            } else {
                principal(43, "octocat")
            });
            native_v1::open_reaction_publication_v3(&mut real.app, Action::Add(Content::Heart));
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
                real.app.status = "NEWER REACTION NAVIGATION".into();
                let generation = channel::generation(&background);
                assert!(!background.poll(&mut real.app));
                assert_eq!(channel::generation(&background), generation);
                assert_eq!(real.app.status, "NEWER REACTION NAVIGATION");
                assert!(real.app.scoped_threads_v1.is_some());
                real.app.github_action = None;
                assert!(background.cancel_scoped_threads_v1(&mut real.app));
            } else {
                assert!(background.poll(&mut real.app));
                assert!(real.app.scoped_threads_v1.is_none());
                assert!(real.app.form.is_none());
            }
            protocol(&real, 0);
        }
    }
}
#[test]
fn review_threads_v3_reaction_nested_back_cancels_only_its_original_pending_no_io() {
    let mut real = reaction_browser(vec![]);
    native_v1::open_reaction_publication_v3(&mut real.app, Action::Remove(7001));
    let owner = real.app.scoped_threads_work_v1.as_ref().unwrap().clone();
    assert!(owner.presentation_matches(&real.app));
    native_v1::close_reactions_v3(&mut real.app);
    assert!(owner.cancellation().is_cancelled());
    assert!(real.app.scoped_threads_work_v1.is_none());
    assert!(real
        .app
        .scoped_threads_v1
        .as_ref()
        .unwrap()
        .comments_v2
        .as_ref()
        .unwrap()
        .reactions_v3
        .is_none());
    assert!(real
        .app
        .scoped_threads_v1
        .as_ref()
        .unwrap()
        .comments_v2
        .is_some());
    protocol(&real, 0);
}
#[test]
fn review_threads_v3_reaction_stale_private_draw_and_key_preserve_independent_successor() {
    let mut old = reaction_browser(vec![]);
    let old_view = old.app.scoped_threads_v1.take().unwrap();
    let mut newer = super::super::super::super::real(vec![]);
    native_v1::open(&mut newer.app);
    let successor = newer.app.scoped_threads_work_v1.as_ref().unwrap().clone();
    let source = newer.app.remote_scope_source.clone().unwrap();
    let nav = newer
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .clone()
        .unwrap();
    newer.app.scoped_threads_v1 = Some(old_view);
    newer.app.overlay = Some(Overlay::PrThreadsV1);
    newer.app.status = "NEWER SOURCE REACTION OWNER".into();
    let loading = newer.app.loading;
    for (width, height) in [(80, 24), (120, 40)] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| reactions::ui::draw(frame, &mut newer.app, frame.area()))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!text.contains("same-login-not-ownership"));
        assert!(!text.contains("ACTUAL_REACTION_NODE"));
        reactions::ui::event(
            &mut newer.app,
            &crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('5'),
                crossterm::event::KeyModifiers::NONE,
            )),
        );
        assert!(newer.app.form.is_none());
        assert!(Arc::ptr_eq(
            newer.app.scoped_threads_work_v1.as_ref().unwrap(),
            &successor
        ));
        assert!(Arc::ptr_eq(
            newer.app.remote_scope_source.as_ref().unwrap(),
            &source
        ));
        assert!(Arc::ptr_eq(
            newer
                .app
                .github
                .as_ref()
                .unwrap()
                .scoped_pr_navigation
                .as_ref()
                .unwrap(),
            &nav
        ));
        assert_eq!(newer.app.status, "NEWER SOURCE REACTION OWNER");
        assert_eq!(newer.app.loading, loading);
    }
    protocol(&old, 0);
    protocol(&newer, 0);
}
#[test]
fn review_threads_v3_reaction_preparation_real_source_clear_refuses_before_private_return() {
    let mut steps = opening_steps(selected_node(false, true));
    steps.extend(observation(selected_comment(), current_rest()));
    steps.extend(actor_steps(actors()));
    assert_eq!(steps.len(), 17);
    let mut real = browser(steps);
    let intent = reactions::ThreadReactionsIntentV3::capture(&real.app).unwrap();
    let pointer = Arc::as_ptr(&intent);
    let scope = intent.scope().clone();
    let path = database(&real.fixture);
    let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let called = count.clone();
    let intent = reactions::after_preparation_for_test(
        intent,
        Arc::new(move || {
            called.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Cache::open(&path).unwrap().clear_records(&scope).unwrap();
        }),
    );
    assert_eq!(Arc::as_ptr(&intent), pointer);
    assert!(matches!(
        reactions::load(
            intent,
            &provider(&real.fixture),
            &database(&real.fixture),
            &Default::default(),
            Instant::now() + Duration::from_secs(30)
        ),
        Err(threads::ThreadBrowseFailureV1::Source)
    ));
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 1);
    real.sql = sql_state(&real.fixture);
    protocol(&real, 0);
}
#[test]
fn review_threads_v3_reaction_same_source_genuine_new_navigation_never_paints_or_cancels_successor()
{
    let mut extra = operation(REPO, 42);
    extra.extend(resource_steps(PrResourceKindV1::Files, REPO));
    let mut real = reaction_browser(extra);
    let old_view = real.app.scoped_threads_v1.take().unwrap();
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
        source.clone(),
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
    native_v1::open(&mut real.app);
    let successor = real.app.scoped_threads_work_v1.as_ref().unwrap().clone();
    let nav = real
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .as_ref()
        .unwrap()
        .clone();
    real.app.scoped_threads_v1 = Some(old_view);
    real.app.overlay = Some(Overlay::PrThreadsV1);
    real.app.status = "SAME SOURCE GENUINELY NEW NAVIGATION".into();
    let loading = real.app.loading;
    for (width, height) in [(80, 24), (120, 40)] {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| reactions::ui::draw(frame, &mut real.app, frame.area()))
            .unwrap();
        let text = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(!text.contains("same-login-not-ownership"));
        assert!(!text.contains("ACTUAL_REACTION_NODE"));
        reactions::ui::event(
            &mut real.app,
            &crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('5'),
                crossterm::event::KeyModifiers::NONE,
            )),
        );
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
        assert_eq!(real.app.status, "SAME SOURCE GENUINELY NEW NAVIGATION");
        assert_eq!(real.app.loading, loading);
    }
    protocol(&real, 0);
}
