//! Genuine closed catalogue fixtures; key/mouse disabled reason is the SAME
//! native action. Actual binary paired journeys are registered separately.
use super::*;
use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use images::reviews_v2::{
    browser_v2::{self, ReviewBrowserActionV2, ReviewBrowserV2},
    native_v2,
};
#[derive(Clone, Copy)]
enum Case {
    OwnPending,
    UnknownPending,
    Partial,
    Empty,
}
fn browser(case: Case) -> (Setup, App) {
    let mut s = inline_setup(|h, steps| {
        let mut row = review(h, "PENDING", "actual server PENDING", 42);
        match case {
            Case::OwnPending => steps.extend(open_steps(h, json!([row]))),
            Case::UnknownPending => {
                row["user"]["id"] = Value::Null;
                steps.extend(open_steps(h, json!([row])));
            }
            Case::Empty => steps.extend(open_steps(h, json!([]))),
            Case::Partial => {
                steps.extend(opening_steps(h));
                for page in 1..=2 {
                    let rows = (0..100)
                        .map(|offset| {
                            let mut actual = review(h, "APPROVED", "actual submitted review", 51);
                            let id = 1000 + (page - 1) * 100 + offset;
                            actual["id"] = json!(id);
                            actual["node_id"] = json!(format!("ACTUAL_BOUNDED_REVIEW_{id}"));
                            actual
                        })
                        .collect::<Vec<_>>();
                    steps.push(get(
                        &format!("repos/{REPO}/pulls/7/reviews?per_page=100&page={page}"),
                        json!(rows),
                    ));
                }
                steps.extend(closing_steps(h));
            }
        }
    });
    let (mut app, _) = owner(&mut s);
    let draft = open(&s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
    app.scoped_reviews_v2 = Some(
        ReviewBrowserV2::prepare(
            draft,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap(),
    );
    app.overlay = Some(crate::app::Overlay::ScopedReviewsV2);
    (s, app)
}
fn paint(app: &mut App) -> ratatui::Terminal<ratatui::backend::TestBackend> {
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|frame| native_v2::draw(frame, app, frame.area()))
        .unwrap();
    terminal
}
fn text(terminal: &ratatui::Terminal<ratatui::backend::TestBackend>) -> String {
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
fn activate(app: &mut App, mouse: bool) {
    if mouse {
        let view = app.scoped_reviews_v2.as_ref().unwrap();
        let event = (0..40)
            .flat_map(|row| {
                (0..120).map(move |column| MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                })
            })
            .find(|event| browser_v2::mouse(view, *event) == Some(ReviewBrowserActionV2::NewBatch))
            .expect("full actual New batch button has a hit");
        native_v2::event(app, Event::Mouse(event));
    } else {
        native_v2::event(
            app,
            Event::Key(KeyEvent::new(KeyCode::Char('B'), KeyModifiers::NONE)),
        );
    }
}
#[test]
fn pending_reviews_v3_focused_enter_applies_same_actual_horizontal_action_as_shortcut_and_mouse() {
    fn paint_at(
        app: &mut App,
        width: u16,
        height: u16,
    ) -> ratatui::Terminal<ratatui::backend::TestBackend> {
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| native_v2::draw(frame, app, frame.area()))
            .unwrap();
        terminal
    }
    let (s, mut app) = browser(Case::OwnPending);
    let before = commands(&s.fixture).len();
    let sql = sql_state(&s.fixture);
    let source = app.remote_scope_source.as_ref().unwrap().clone();
    for (width, height) in [(40, 18), (80, 24), (120, 40)] {
        for input in 0..3 {
            assert!(text(&paint_at(&mut app, width, height)).contains("#810"));
            let click = (0..height)
                .flat_map(|row| {
                    (0..width).map(move |column| MouseEvent {
                        kind: MouseEventKind::Down(MouseButton::Left),
                        column,
                        row,
                        modifiers: KeyModifiers::NONE,
                    })
                })
                .find(|event| {
                    browser_v2::mouse(app.scoped_reviews_v2.as_ref().unwrap(), *event)
                        == Some(ReviewBrowserActionV2::Horizontal(16))
                })
                .expect("current Right control has its exact actual clickable hit");
            match input {
                0 => {
                    let mut focused = false;
                    for _ in 0..=20 {
                        let terminal = paint_at(&mut app, width, height);
                        if terminal.backend().buffer()[(click.column, click.row)]
                            .modifier
                            .contains(ratatui::style::Modifier::REVERSED)
                        {
                            focused = true;
                            break;
                        }
                        native_v2::event(
                            &mut app,
                            Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
                        );
                    }
                    assert!(focused, "actual Tab reaches the painted Right control");
                    native_v2::event(
                        &mut app,
                        Event::Key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
                    );
                }
                1 => native_v2::event(
                    &mut app,
                    Event::Key(KeyEvent::new(KeyCode::Char('>'), KeyModifiers::NONE)),
                ),
                _ => native_v2::event(&mut app, Event::Mouse(click)),
            }
            assert!(
                !text(&paint_at(&mut app, width, height)).contains("#810"),
                "focused Enter, shortcut and click must apply the SAME horizontal transition"
            );
            native_v2::event(
                &mut app,
                Event::Key(KeyEvent::new(KeyCode::Char('<'), KeyModifiers::NONE)),
            );
            assert!(text(&paint_at(&mut app, width, height)).contains("#810"));
            assert!(app.scoped_reviews_work_v2.is_none());
            assert!(app.scoped_reviews_owner_v2.is_none());
            assert!(app.form.is_none());
            assert!(Arc::ptr_eq(
                app.remote_scope_source.as_ref().unwrap(),
                &source
            ));
            assert_eq!(commands(&s.fixture).len(), before);
            assert_eq!(sql_state(&s.fixture), sql);
        }
    }
    unchanged_git_and_exact_script(&s);
}
#[test]
fn pending_reviews_v3_existing_pending_new_batch_keys_and_clicks_show_reason_without_queue_or_effects(
) {
    for mouse in [false, true] {
        let (s, mut app) = browser(Case::OwnPending);
        let terminal = paint(&mut app);
        assert!(text(&terminal).contains("[B New batch]"));
        let source = app.remote_scope_source.as_ref().unwrap().clone();
        let before = commands(&s.fixture).len();
        let sql = sql_state(&s.fixture);
        activate(&mut app, mouse);
        assert!(text(&paint(&mut app))
            .contains("Own PENDING exists; Resume or G Add range. New batch unavailable."));
        assert!(app.scoped_reviews_work_v2.is_none());
        assert!(app.form.is_none());
        assert!(app.scoped_reviews_owner_v2.is_none());
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &source
        ));
        assert_eq!(commands(&s.fixture).len(), before);
        assert_eq!(sql_state(&s.fixture), sql);
        unchanged_git_and_exact_script(&s);
    }
}
#[test]
fn pending_reviews_v3_new_batch_requires_actual_complete_known_own_absence_not_partial_or_unknown_actor(
) {
    for (case, reason) in [
        (
            Case::Partial,
            "Review list partial; own-PENDING absence UNKNOWN",
        ),
        (Case::UnknownPending, "Pending author UID unknown"),
    ] {
        let (s, mut app) = browser(case);
        paint(&mut app);
        let sql = sql_state(&s.fixture);
        let before = commands(&s.fixture).len();
        activate(&mut app, false);
        assert!(text(&paint(&mut app)).contains(reason));
        assert!(app.scoped_reviews_work_v2.is_none());
        assert_eq!(sql_state(&s.fixture), sql);
        assert_eq!(commands(&s.fixture).len(), before);
        unchanged_git_and_exact_script(&s);
    }
    let (s, mut app) = browser(Case::Empty);
    paint(&mut app);
    assert!(app
        .scoped_reviews_v2
        .as_ref()
        .unwrap()
        .new_batch_disabled_v12()
        .is_none());
    let before = commands(&s.fixture).len();
    let sql = sql_state(&s.fixture);
    activate(&mut app, true);
    assert!(
        app.scoped_reviews_work_v2.is_some(),
        "only actual complete empty catalogue can queue new local batch"
    );
    assert!(
        app.form.is_none(),
        "foreground queues intent only, never builds or publishes a form"
    );
    assert_eq!(commands(&s.fixture).len(), before);
    assert_eq!(sql_state(&s.fixture), sql);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v3_tiny_warning_is_complete_and_stale_key_mouse_actions_have_no_effects() {
    for mouse in [false, true] {
        let (s, mut app) = browser(Case::OwnPending);
        paint(&mut app);
        let old_hit = (0..40)
            .flat_map(|row| {
                (0..120).map(move |column| MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                })
            })
            .find(|event| {
                browser_v2::mouse(app.scoped_reviews_v2.as_ref().unwrap(), *event)
                    == Some(ReviewBrowserActionV2::NewBatch)
            })
            .unwrap();
        let before = commands(&s.fixture).len();
        let sql = sql_state(&s.fixture);
        let source = app.remote_scope_source.as_ref().unwrap().clone();
        native_v2::event(&mut app, Event::Resize(40, 14));
        let mut terminal =
            ratatui::Terminal::new(ratatui::backend::TestBackend::new(40, 14)).unwrap();
        terminal
            .draw(|frame| native_v2::draw(frame, &mut app, frame.area()))
            .unwrap();
        let tiny = text(&terminal);
        assert!(tiny.contains("Resize to inspect reviews • disabled"));
        assert!(tiny.contains("[Esc Close]"));
        assert!(!tiny.contains("[B New batch]"));
        assert!(!tiny.contains("[F3 Edit pending]"));
        assert!(crate::editor::display_width("Resize to inspect reviews • disabled") <= 38);
        if mouse {
            native_v2::event(&mut app, Event::Mouse(old_hit));
        } else {
            native_v2::event(
                &mut app,
                Event::Key(KeyEvent::new(KeyCode::Char('B'), KeyModifiers::NONE)),
            );
            native_v2::event(
                &mut app,
                Event::Key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE)),
            );
        }
        assert!(app.scoped_reviews_work_v2.is_none());
        assert!(app.scoped_reviews_owner_v2.is_none());
        assert!(app.form.is_none());
        assert!(Arc::ptr_eq(
            app.remote_scope_source.as_ref().unwrap(),
            &source
        ));
        assert_eq!(commands(&s.fixture).len(), before);
        assert_eq!(sql_state(&s.fixture), sql);
        unchanged_git_and_exact_script(&s);
    }
}
