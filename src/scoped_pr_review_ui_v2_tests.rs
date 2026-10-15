//! Actual closed source/range/review fixtures plus geometry/input contracts.
//! Real-binary keyboard/mouse coverage is separately registered; no snapshot
//! acceptance or these direct actions substitute for those journeys.
use super::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use images::reviews_v2::{
    browser_v2::{self, ReviewBrowserActionV2, ReviewBrowserV2},
    ReviewRequestedActionV2, ScopedReviewPreviewV2, ScopedReviewRequestV2,
};
fn draw(view: &mut ReviewBrowserV2, width: u16, height: u16) -> String {
    let mut terminal =
        ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|frame| {
            browser_v2::draw(
                frame,
                frame.area(),
                view,
                crate::theme::Theme::from_settings(0, false),
                false,
                &Default::default(),
            )
        })
        .unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}
fn has_mouse(
    view: &ReviewBrowserV2,
    action: ReviewBrowserActionV2,
    width: u16,
    height: u16,
) -> bool {
    (0..height).any(|row| {
        (0..width).any(|column| {
            browser_v2::mouse(
                view,
                MouseEvent {
                    kind: MouseEventKind::Down(MouseButton::Left),
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                },
            ) == Some(action)
        })
    })
}

#[test]
fn pending_reviews_v2_preview_actual_anchor_rehydrates_same_m_h_raw_blob_producer_without_promoting_saved_coordinates(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(open_steps(h, json!([])));
        steps.extend(opening_steps(h));
        let images = image_steps(h);
        // The inner immutable producer performs its own opening parent GET;
        // the surrounding review session cannot replace that fresh guard.
        steps.extend(images[3..images.len() - 3].iter().cloned());
        steps.push(catalogue_step(json!([])));
        steps.extend(closing_steps(h));
    });
    let (app, proof) = owner(&mut s);
    let draft = open(&s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
    let anchor = ScopedReviewAnchorIntentV2::capture(
        &proof.canonical_range(PrImageSideV1::Old, 1, 2).unwrap(),
        &app,
    )
    .unwrap();
    let comment = ReviewDraftCommentV2::prepare(
        &anchor,
        draft.intent(),
        Arc::from("original selected M-side comment"),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    let request = ScopedReviewRequestV2::create(
        &draft,
        Arc::new(
            ReviewDraftValuesV2::replace(Arc::from("batch summary"), vec![comment].into()).unwrap(),
        ),
    );
    let before = commands(&s.fixture).len();
    let preview = ScopedReviewPreviewV2::prepare(
        request,
        &provider(&s.fixture),
        &database(&s.fixture),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(commands(&s.fixture).len() - before, 20);
    assert_eq!(preview.request().values().comments().len(), 1);
    assert!(preview.lines().iter().any(|line| line.contains(&s.h.m)));
    assert!(preview.lines().iter().any(|line| line.contains(&s.h.h)));
    assert!(preview
        .lines()
        .iter()
        .any(|line| line.contains("original selected M-side comment")));
    let calls = commands(&s.fixture);
    let fresh = &calls[before..];
    for blob in [&s.h.old_blob, &s.h.new_blob] {
        assert!(fresh
            .iter()
            .any(|call| call["args"][1] == format!("repos/{REPO}/git/blobs/{blob}")));
    }
    assert_ne!(
        preview.request().values().comments()[0].anchor().head(),
        s.h.m
    );
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_browser_all_current_controls_share_exact_keyboard_mouse_actions_narrow_and_tiny_never_enable_hidden_effects(
) {
    let mut s = inline_setup(|h, steps| {
        steps.extend(open_steps(
            h,
            json!([review(h, "PENDING", "actual server pending", 42)]),
        ))
    });
    let (app, _) = owner(&mut s);
    let draft = open(&s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
    let before = commands(&s.fixture).len();
    let mut view = ReviewBrowserV2::prepare(
        draft,
        &Default::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(!view.interactive());
    let controls = [
        ('B', ReviewBrowserActionV2::NewBatch),
        ('R', ReviewBrowserActionV2::Resume),
        (
            'U',
            ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::UpdateSummary),
        ),
        (
            'C',
            ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::SubmitComment),
        ),
        (
            'A',
            ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::Approve),
        ),
        (
            'X',
            ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::RequestChanges),
        ),
        (
            'D',
            ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::Discard),
        ),
        (
            'I',
            ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::Dismiss),
        ),
        ('<', ReviewBrowserActionV2::Horizontal(-16)),
        ('>', ReviewBrowserActionV2::Horizontal(16)),
        ('Q', ReviewBrowserActionV2::Quit),
    ];
    for (width, height) in [(120, 40), (80, 24), (40, 18)] {
        let painted = draw(&mut view, width, height);
        assert!(view.interactive());
        assert!(painted.contains("PENDING"));
        for (key, action) in controls {
            assert_eq!(
                browser_v2::key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE)),
                Some(action)
            );
            assert!(has_mouse(&view,action,width,height),"every actual keyboard action needs its OWN clickable region at {width}×{height}: {action:?}");
        }
        assert!(has_mouse(
            &view,
            ReviewBrowserActionV2::Close,
            width,
            height
        ));
        for (code, action) in [
            (KeyCode::F(2), ReviewBrowserActionV2::NextPendingCommentV3),
            (KeyCode::F(3), ReviewBrowserActionV2::EditPendingCommentV3),
            (KeyCode::F(4), ReviewBrowserActionV2::DeletePendingCommentV3),
            (KeyCode::Char('G'), ReviewBrowserActionV2::AddPendingRangeV3),
            (KeyCode::F(5), ReviewBrowserActionV2::RequestReviewerV3),
            (KeyCode::F(6), ReviewBrowserActionV2::TeamsV3),
            (KeyCode::F(7), ReviewBrowserActionV2::RequestTeamV3),
        ] {
            assert_eq!(
                browser_v2::key(KeyEvent::new(code, KeyModifiers::NONE)),
                Some(action)
            );
            assert!(
                has_mouse(&view, action, width, height),
                "new full action lacks actual clickable control: {action:?}"
            );
        }
    }
    let sql = sql_state(&s.fixture);
    assert!(draw(&mut view, 40, 14).contains("Resize"));
    assert!(!view.interactive());
    for (_, action) in controls {
        assert!(
            !has_mouse(&view, action, 40, 14),
            "no hidden action in old now-insufficient40x14 geometry"
        );
    }
    assert!(has_mouse(&view, ReviewBrowserActionV2::Close, 40, 14));
    assert_eq!(commands(&s.fixture).len(), before);
    assert_eq!(sql_state(&s.fixture), sql);
    assert!(draw(&mut view, 20, 6).contains("Resize"));
    assert!(!view.interactive());
    assert!(has_mouse(&view, ReviewBrowserActionV2::Close, 20, 6));
    assert!(!has_mouse(&view, ReviewBrowserActionV2::NewBatch, 20, 6));
    assert!(!has_mouse(
        &view,
        ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::Approve),
        20,
        6
    ));
    assert_eq!(commands(&s.fixture).len(), before);
    unchanged_git_and_exact_script(&s);
}

#[test]
fn pending_reviews_v2_browser_actual_same_login_foreign_uid_and_missing_submission_remain_readable_disabled_without_extra_children(
) {
    for submitted_missing in [false, true] {
        let mut s = inline_setup(|h, steps| {
            let mut row = review(
                h,
                if submitted_missing {
                    "APPROVED"
                } else {
                    "PENDING"
                },
                "Readable actual foreign review",
                43,
            );
            row["user"]["login"] = json!("reviewer-active");
            if submitted_missing {
                row["submitted_at"] = Value::Null;
            }
            steps.extend(open_steps(h, json!([row.clone()])));
            steps.extend(opening_steps(h));
            steps.extend(snapshot_steps(h, &row));
            steps.extend(closing_steps(h));
        });
        let (app, _) = owner(&mut s);
        let draft = open(&s, ScopedReviewIntentV2::capture(&app).unwrap()).unwrap();
        let selection = ScopedReviewSelectionV2::read(
            &draft,
            0,
            &provider(&s.fixture),
            &database(&s.fixture),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let rows = ReviewBrowserV2::selected_rows(
            &selection,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        let mut view = ReviewBrowserV2::prepare(
            draft,
            &Default::default(),
            Instant::now() + Duration::from_secs(30),
        )
        .unwrap();
        assert!(view.install_selection(selection, rows));
        let before = commands(&s.fixture).len();
        let text = draw(&mut view, 120, 40);
        assert!(text.contains("Readable actual foreign review"));
        assert!(text.contains("reviewer-active"));
        assert!(view
            .effect_disabled(if submitted_missing {
                ReviewRequestedActionV2::Dismiss
            } else {
                ReviewRequestedActionV2::Approve
            })
            .is_some());
        assert!(has_mouse(
            &view,
            ReviewBrowserActionV2::Effect(ReviewRequestedActionV2::Dismiss),
            120,
            40
        ));
        assert_eq!(commands(&s.fixture).len(), before);
        unchanged_git_and_exact_script(&s);
    }
}
