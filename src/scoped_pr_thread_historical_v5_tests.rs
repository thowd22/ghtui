//! Genuine Source/comment/worker tests supplement paired real-binary journeys.
//! UNRUN. No source, historical result, actor or placement is fabricated.
use super::super::super::super::preparation_close_v4 as clear_oracle;
use super::super::historical_v5 as provider_contracts;
use super::*;
use crate::immutable_git_v1_test_support as git_history;
use crate::providers::gh::HistoricalCommitRoleV5 as Role;
use crate::scoped_pr_observed_tabs_v1::resources::threads_v1::historical_v5 as historical;

fn replace(value: &mut Value, h: &git_history::GitImagesV1) {
    match value {
        Value::String(raw) if raw == &"c".repeat(40) => *raw = h.h.clone(),
        Value::String(raw) if raw == &"d".repeat(40) => *raw = h.m.clone(),
        Value::String(raw) if raw == "src/old.rs" => *raw = git_history::PATH.into(),
        Value::Array(rows) => {
            for row in rows {
                replace(row, h);
            }
        }
        Value::Object(map) => {
            for row in map.values_mut() {
                replace(row, h);
            }
        }
        _ => {}
    }
}
fn browser_historical(h: &git_history::GitImagesV1, mut extra: Vec<GhStep>) -> Real {
    let mut steps = thread_steps(Some("LINE"));
    steps.extend(observation_steps(selected_node(false, true)));
    steps.append(&mut extra);
    for step in &mut steps {
        if let Ok(mut value) = serde_json::from_slice::<Value>(&step.stdout) {
            replace(&mut value, h);
            step.stdout = serde_json::to_vec(&value).unwrap();
        }
    }
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
fn operation(h: &git_history::GitImagesV1, role: Role, blob: bool) -> Vec<GhStep> {
    let mut rows = provider_contracts::opening(h);
    rows.extend(provider_contracts::objects(h, role, blob));
    rows.extend(close_steps());
    rows
}
fn paint(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal.draw(|frame| crate::ui::draw(frame, app)).unwrap();
    terminal
        .backend()
        .buffer()
        .content
        .chunks(width as usize)
        .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}
fn finish(real: &mut Real, bg: &mut Background) {
    let deadline = Instant::now() + Duration::from_secs(8);
    while real.app.loading[View::Inbox.index()] {
        bg.poll(&mut real.app);
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
}
#[test]
fn review_threads_v5_historical_real_worker_key_click_width_scroll_and_pure_back_no_write_authority(
) {
    for mouse in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let h = git_history::GitImagesV1::create(dir.path(), false);
        let mut real = browser_historical(&h, operation(&h, Role::Associated, true));
        let before = paint(&mut real.app, 120, 40);
        historical::ui::open(&mut real.app);
        assert!(paint(&mut real.app, 120, 40).contains("placement UNKNOWN"));
        let event = if mouse {
            // Native event hits are the actual just-painted owned chooser.
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut real.app))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let row = (0..40)
                .find(|y| {
                    (0..120)
                        .map(|x| buffer[(x, *y)].symbol())
                        .collect::<String>()
                        .contains("[c Associated C]")
                })
                .unwrap();
            let label = "[c Associated C]";
            let col = (0..120u16)
                .find(|x| {
                    x.saturating_add(label.len() as u16) <= 120
                        && label.chars().enumerate().all(|(offset, ch)| {
                            buffer[(*x + offset as u16, row)].symbol() == ch.to_string()
                        })
                })
                .unwrap();
            crossterm::event::Event::Mouse(crossterm::event::MouseEvent {
                kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
                column: col,
                row,
                modifiers: crossterm::event::KeyModifiers::NONE,
            })
        } else {
            crossterm::event::Event::Key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('c'),
                crossterm::event::KeyModifiers::NONE,
            ))
        };
        real.app.handle_event(event);
        let owner = real.app.scoped_threads_work_v1.take().unwrap();
        let mut bg = Background::new();
        assert!(bg.submit_scoped_threads_v1(
            &mut real.app,
            owner,
            provider(&real.fixture),
            database(&real.fixture)
        ));
        finish(&mut real, &mut bg);
        for (width, height) in [(80, 24), (120, 40)] {
            let visible = paint(&mut real.app, width, height);
            assert!(visible.contains("VERIFIED HISTORICAL"));
            assert!(visible.contains("placement UNKNOWN"));
            assert!(!visible.contains("[Confirm action]"));
            assert!(real.app.form.is_none());
        }
        let selected = real
            .app
            .scoped_threads_v1
            .as_ref()
            .unwrap()
            .comments_v2
            .as_ref()
            .unwrap()
            .historical_v5
            .as_ref()
            .unwrap()
            .selection
            .clone();
        native_v1::close_historical_v5(&mut real.app, &selected);
        assert_eq!(
            paint(&mut real.app, 120, 40)
                .lines()
                .take(38)
                .collect::<Vec<_>>(),
            before.lines().take(38).collect::<Vec<_>>()
        );
        protocol(&real, 0);
    }
}
#[test]
fn review_threads_v5_historical_pending_back_cancels_only_original_child_preserving_comments() {
    let dir = tempfile::tempdir().unwrap();
    let h = git_history::GitImagesV1::create(dir.path(), false);
    let mut extra = operation(&h, Role::Associated, true);
    extra[12].delay_ms = 6000;
    extra.truncate(13);
    let mut real = browser_historical(&h, extra);
    historical::ui::open(&mut real.app);
    native_v1::open_historical_v5(&mut real.app, Role::Associated);
    let owner = real.app.scoped_threads_work_v1.take().unwrap();
    let selection = real
        .app
        .scoped_threads_v1
        .as_ref()
        .unwrap()
        .comments_v2
        .as_ref()
        .unwrap()
        .historical_v5
        .as_ref()
        .unwrap()
        .selection
        .clone();
    let mut bg = Background::new();
    assert!(bg.submit_scoped_threads_v1(
        &mut real.app,
        owner,
        provider(&real.fixture),
        database(&real.fixture)
    ));
    let deadline = Instant::now() + Duration::from_secs(8);
    while fs::read_to_string(real.fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap()
        .lines()
        .count()
        < real.steps.len()
    {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    native_v1::close_historical_v5(&mut real.app, &selection);
    bg.retire_stale_scoped_threads_v1(&mut real.app);
    bg.poll(&mut real.app);
    assert!(!real.app.loading[View::Inbox.index()]);
    assert!(real
        .app
        .scoped_threads_v1
        .as_ref()
        .unwrap()
        .comments_v2
        .as_ref()
        .unwrap()
        .historical_v5
        .is_none());
    assert!(paint(&mut real.app, 120, 40).contains("Selected thread comments"));
    protocol(&real, 0);
}
#[test]
fn review_threads_v5_historical_genuine_form_ticket_input_epoch_and_numeric_scope_refuse_old_private_draw(
) {
    let dir = tempfile::tempdir().unwrap();
    let h = git_history::GitImagesV1::create(dir.path(), false);
    let mut real = browser_historical(&h, vec![]);
    let make_form = |app: &App| {
        crate::forms::Form::pr(crate::forms::PrIntent {
            kind: crate::forms::PrFormKind::Comment,
            target: app
                .github
                .as_ref()
                .unwrap()
                .pr
                .as_ref()
                .unwrap()
                .details
                .summary
                .clone(),
            identity: app.github.as_ref().unwrap().identity.clone(),
        })
        .unwrap()
    };
    real.app.form = Some(make_form(&real.app));
    real.app.form_dismissed = true;
    historical::ui::open(&mut real.app);
    let selection = real
        .app
        .scoped_threads_v1
        .as_ref()
        .unwrap()
        .comments_v2
        .as_ref()
        .unwrap()
        .historical_v5
        .as_ref()
        .unwrap()
        .selection
        .clone();
    assert!(selection.owns(&real.app));
    real.app.selected += 1;
    assert!(!selection.owns(&real.app));
    real.app.selected -= 1;
    let original_form = real.app.form.as_ref().unwrap().clone();
    real.app
        .form
        .as_mut()
        .unwrap()
        .observe_presentation_input_v4();
    assert!(!selection.owns(&real.app));
    real.app.form = Some(original_form);
    assert!(
        !selection.owns(&real.app),
        "Same logical clone cannot rewind shared actual input epoch"
    );
    real.app.form = Some(make_form(&real.app));
    assert!(
        !selection.owns(&real.app),
        "A new genuine Form constructor is a distinct presentation"
    );
    real.app.github.as_mut().unwrap().identity.account_id = Some(43);
    assert!(!selection.owns(&real.app));
    let visible = paint(&mut real.app, 120, 40);
    assert!(!visible.contains(BODY));
    assert!(!visible.contains("VERIFIED HISTORICAL"));
    protocol(&real, 0);
}
#[test]
fn review_threads_v5_historical_actual_clear_after_prepared_doc_refuses_before_final_children_with_exact_sql_effects(
) {
    let dir = tempfile::tempdir().unwrap();
    let h = git_history::GitImagesV1::create(dir.path(), false);
    let mut real = browser_historical(&h, operation(&h, Role::Associated, true));
    let scope = real
        .app
        .remote_scope_source
        .as_ref()
        .unwrap()
        .scope()
        .clone();
    clear_oracle::seed_clear_witnesses_v5(&real, &scope);
    let expected = clear_oracle::clear_sql_v5(&real.fixture, &scope, true);
    assert_ne!(
        expected,
        clear_oracle::clear_sql_v5(&real.fixture, &scope, false)
    );
    let config = real.fixture.root.join("config/ghtui/config.toml");
    let config_before = fs::read(&config).ok();
    let git_config = fs::read(real.fixture.repo().join(".git/config")).unwrap();
    historical::ui::open(&mut real.app);
    native_v1::open_historical_v5(&mut real.app, Role::Associated);
    let owner = real.app.scoped_threads_work_v1.take().unwrap();
    let pointer = Arc::as_ptr(&owner);
    let path = database(&real.fixture);
    let original_scope = scope.clone();
    let called = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let observed = called.clone();
    let owner = native_v1::after_historical_preparation_for_test(
        owner,
        Arc::new(move || {
            observed.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Cache::open(&path)
                .unwrap()
                .clear_records(&original_scope)
                .unwrap();
        }),
    );
    assert_eq!(Arc::as_ptr(&owner), pointer);
    let mut bg = Background::new();
    assert!(bg.submit_scoped_threads_v1(
        &mut real.app,
        owner,
        provider(&real.fixture),
        database(&real.fixture)
    ));
    finish(&mut real, &mut bg);
    assert_eq!(called.load(std::sync::atomic::Ordering::SeqCst), 1);
    assert!(real.app.scoped_threads_v1.is_none());
    assert!(!paint(&mut real.app, 120, 40).contains("VERIFIED HISTORICAL"));
    assert_eq!(clear_oracle::clear_sql_v5(&real.fixture,&scope,false),expected,"Only independently predicted scoped Clear effects; exact drafts/attempts/foreign/config/Git preserved");
    assert_eq!(fs::read(config).ok(), config_before);
    assert_eq!(
        fs::read(real.fixture.repo().join(".git/config")).unwrap(),
        git_config
    );
    real.steps.truncate(real.steps.len() - 4);
    clear_oracle::clear_protocol_intact_v5(&real);
}
#[test]
fn review_threads_v5_historical_actual_worker_source_failure_panic_and_mixed_result_before_private_slot_effects(
) {
    for mode in 0..3 {
        let dir = tempfile::tempdir().unwrap();
        let h = git_history::GitImagesV1::create(dir.path(), false);
        let extra = if mode == 0 {
            principal(43, "same-login-but-different-account")
        } else if mode == 1 {
            vec![]
        } else {
            operation(&h, Role::Associated, true)
        };
        let mut real = browser_historical(&h, extra);
        historical::ui::open(&mut real.app);
        native_v1::open_historical_v5(&mut real.app, Role::Associated);
        let mut owner = real.app.scoped_threads_work_v1.take().unwrap();
        if mode == 1 {
            owner = native_v1::actual_worker_panic_for_test(owner);
        }
        let mut bg = Background::new();
        assert!(bg.submit_scoped_threads_v1(
            &mut real.app,
            owner,
            provider(&real.fixture),
            database(&real.fixture)
        ));
        if mode < 2 {
            crate::runtime::scoped_threads_test_support_v1::actual_completion(&mut bg, mode == 1);
            bg.poll(&mut real.app);
            assert!(real
                .app
                .github
                .as_ref()
                .is_none_or(|github| github.pr.is_none()));
            assert!(real.app.scoped_threads_v1.is_none());
            let visible = paint(&mut real.app, 120, 40);
            assert!(!visible.contains(BODY));
            assert!(!visible.contains("VERIFIED HISTORICAL"));
        } else {
            // Manufactured poison over an actual successful completion proves
            // only mixed-facet refusal, unlike the genuine panic above.
            crate::runtime::scoped_threads_test_support_v1::fault_next(&mut bg, 0);
            let generation = crate::runtime::scoped_threads_test_support_v1::generation(&bg);
            let owner = real.app.scoped_threads_owner_v1.as_ref().unwrap().clone();
            let status = real.app.status.clone();
            bg.poll(&mut real.app);
            assert_eq!(
                crate::runtime::scoped_threads_test_support_v1::generation(&bg),
                generation
            );
            assert!(Arc::ptr_eq(
                real.app.scoped_threads_owner_v1.as_ref().unwrap(),
                &owner
            ));
            assert!(real.app.loading[View::Inbox.index()]);
            assert_eq!(real.app.status, status);
            assert!(real
                .app
                .scoped_threads_v1
                .as_ref()
                .unwrap()
                .comments_v2
                .as_ref()
                .unwrap()
                .historical_v5
                .as_ref()
                .unwrap()
                .prepared
                .is_none());
            let selection = real
                .app
                .scoped_threads_v1
                .as_ref()
                .unwrap()
                .comments_v2
                .as_ref()
                .unwrap()
                .historical_v5
                .as_ref()
                .unwrap()
                .selection
                .clone();
            native_v1::close_historical_v5(&mut real.app, &selection);
            bg.retire_stale_scoped_threads_v1(&mut real.app);
            assert!(real
                .app
                .scoped_threads_v1
                .as_ref()
                .unwrap()
                .comments_v2
                .is_some());
        }
        protocol(&real, 0);
    }
}
