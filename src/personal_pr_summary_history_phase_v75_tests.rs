//! DECLARED / UNRUN. Existing actual search/detail factories and original
//! Core children; no fake Source/Files/Current/permission constructor.
use super::super::tests as f;
use super::*;
fn steps(uid: u64) -> Vec<crate::test_support::GhStep> {
    let mut steps = f::principal();
    steps[1] = f::get(
        "user",
        serde_json::json!({"id":uid,"login":"fresh-history-numeric-principal"}),
    );
    if uid != 42 {
        return steps;
    }
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repository()));
    steps.push(f::get(&format!("repos/{}/pulls/7", f::REPO), f::parent()));
    steps.push(f::get(&format!("repos/{}/pulls/7", f::REPO), f::parent()));
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repository()));
    steps.extend(f::principal());
    assert_eq!(steps.len(), 8);
    steps
}
#[test]
fn file_comments_v75_summary_phase_actual_eight_children_same_parent_and_final_uid_no_source_mint()
{
    let s = f::setup(steps(42));
    let database = f::database(&s.fixture);
    let original =
        summary_v14::PersonalPrSummaryPresentationV14::capture(&s.app, &database).unwrap();
    let provider = s
        .api
        .clone()
        .with_catalogue_read_deadline(original.deadline());
    let phase = PersonalSummaryHistoryPhaseV75::begin(
        &original,
        &provider,
        &database,
        original.cancellation(),
        original.deadline(),
    )
    .unwrap();
    assert_eq!(phase.parent().node_id(), "PERSONAL_PR_1007");
    assert_eq!(phase.parent().repository_id(), 10000);
    assert_eq!(phase.parent().database_id(), 1007);
    assert_eq!(phase.identity().account_id, Some(42));
    assert!(s.app.personal_pr_source_v13.is_none());
    assert!(s
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .is_none());
    assert!(s.app.github.as_ref().unwrap().scoped_pr_resource.is_none());
    let calls = std::cell::Cell::new(0);
    let identity = phase
        .finish(|| {
            calls.set(calls.get() + 1);
            assert_eq!(f::sql(&s.fixture), s.before_sql);
            Ok(())
        })
        .unwrap();
    assert_eq!(calls.get(), 1);
    assert_eq!(identity.account_id, Some(42));
    f::exact(&s, s.steps.len());
}
#[test]
fn file_comments_v75_summary_phase_wrong_numeric_uid_refuses_before_raw_parent_or_sql_payload() {
    let s = f::setup(steps(43));
    let database = f::database(&s.fixture);
    let original =
        summary_v14::PersonalPrSummaryPresentationV14::capture(&s.app, &database).unwrap();
    let provider = s
        .api
        .clone()
        .with_catalogue_read_deadline(original.deadline());
    assert!(PersonalSummaryHistoryPhaseV75::begin(
        &original,
        &provider,
        &database,
        original.cancellation(),
        original.deadline()
    )
    .is_err());
    assert!(original.owns_presentation(&s.app));
    f::exact(&s, s.steps.len());
}
#[test]
fn file_comments_v75_summary_phase_canceled_missing_original_window_or_foreign_database_starts_zero_children(
) {
    let s = f::setup(Vec::new());
    let database = f::database(&s.fixture);
    let original =
        summary_v14::PersonalPrSummaryPresentationV14::capture(&s.app, &database).unwrap();
    assert!(PersonalSummaryHistoryPhaseV75::begin(
        &original,
        &s.api,
        &database,
        original.cancellation(),
        original.deadline()
    )
    .is_err());
    let provider = s
        .api
        .clone()
        .with_catalogue_read_deadline(original.deadline());
    let other = s.fixture.root.join("foreign-history-cache.sqlite3");
    drop(Cache::open(&other).unwrap());
    assert!(PersonalSummaryHistoryPhaseV75::begin(
        &original,
        &provider,
        &other,
        original.cancellation(),
        original.deadline()
    )
    .is_err());
    original.cancellation().cancel();
    assert!(PersonalSummaryHistoryPhaseV75::begin(
        &original,
        &provider,
        &database,
        original.cancellation(),
        original.deadline()
    )
    .is_err());
    f::exact(&s, s.steps.len());
}
#[test]
fn file_comments_v75_summary_phase_changed_closing_actual_author_or_final_uid_never_admits_private_result(
) {
    for uid_loss in [false, true] {
        let mut tail = steps(42);
        if uid_loss {
            tail[7] = f::get(
                "user",
                serde_json::json!({"id":43,"login":"same-visible-login"}),
            );
        } else {
            let mut p = f::parent();
            p["user"]["id"] = serde_json::json!(99);
            tail[4] = f::get(&format!("repos/{}/pulls/7", f::REPO), p);
            tail.truncate(5);
        }
        let s = f::setup(tail);
        let database = f::database(&s.fixture);
        let original =
            summary_v14::PersonalPrSummaryPresentationV14::capture(&s.app, &database).unwrap();
        let provider = s
            .api
            .clone()
            .with_catalogue_read_deadline(original.deadline());
        let phase = PersonalSummaryHistoryPhaseV75::begin(
            &original,
            &provider,
            &database,
            original.cancellation(),
            original.deadline(),
        )
        .unwrap();
        let called = std::cell::Cell::new(0);
        assert!(phase
            .finish(|| {
                called.set(called.get() + 1);
                Ok(())
            })
            .is_err());
        assert_eq!(called.get(), usize::from(uid_loss));
        f::exact(&s, s.steps.len());
    }
}

#[test]
fn personal_review_summary_phase_v15_invalid_limits_expired_canceled_refuse_before_any_child() {
    let s = f::setup(Vec::new());
    let db = f::database(&s.fixture);
    let original = summary_v14::PersonalPrSummaryPresentationV14::capture(&s.app, &db).unwrap();
    let provider = s
        .api
        .clone()
        .with_catalogue_read_deadline(original.deadline());
    let invalid = [
        ReviewReadLimitsV2 {
            max_pages: 14,
            ..Default::default()
        },
        ReviewReadLimitsV2 {
            max_document_bytes: 0,
            ..Default::default()
        },
    ];
    for limits in invalid {
        assert!(PersonalSummaryHistoryPhaseV75::begin_review_inspection_v14(
            &original,
            &provider,
            &db,
            original.cancellation(),
            original.deadline(),
            &limits
        )
        .is_err());
    }
    assert!(PersonalSummaryHistoryPhaseV75::begin_review_inspection_v14(
        &original,
        &provider,
        &db,
        original.cancellation(),
        Instant::now() - Duration::from_millis(1),
        &Default::default()
    )
    .is_err());
    original.cancellation().cancel();
    assert!(PersonalSummaryHistoryPhaseV75::begin_review_inspection_v14(
        &original,
        &provider,
        &db,
        original.cancellation(),
        original.deadline(),
        &Default::default()
    )
    .is_err());
    f::exact(&s, s.steps.len());
}

fn review_inspection_steps_v15() -> Vec<crate::test_support::GhStep> {
    let mut steps = f::principal();
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repository()));
    steps.push(f::get(&format!("repos/{}/pulls/7", f::REPO), f::parent()));
    let review = serde_json::json!({"id":810,"node_id":"ACTUAL_PENDING_REVIEW_810","state":"PENDING","body":"Current observed review", "commit_id":"a".repeat(40),"pull_request_url":format!("https://github.invalid/api/v3/repos/{}/pulls/7",f::REPO),"user":{"id":42,"login":"octocat","type":"User"}});
    steps.push(f::get(
        &format!("repos/{}/pulls/7/reviews?per_page=100&page=1", f::REPO),
        serde_json::json!([review.clone()]),
    ));
    steps.push(f::get(
        &format!("repos/{}/pulls/7/reviews/810", f::REPO),
        review.clone(),
    ));
    steps.push(f::get(
        &format!(
            "repos/{}/pulls/7/reviews/810/comments?per_page=100&page=1",
            f::REPO
        ),
        serde_json::json!([]),
    ));
    steps.push(f::get(
        &format!("repos/{}/pulls/7/reviews/810", f::REPO),
        review,
    ));
    steps.push(f::get(&format!("repos/{}/pulls/7", f::REPO), f::parent()));
    steps.push(f::get(&format!("repos/{}", f::REPO), f::repository()));
    steps.extend(f::principal());
    assert_eq!(steps.len(), 12);
    steps
}

#[test]
fn personal_review_summary_phase_v15_actual_inspector_same_pool_and_no_second_session_or_extra_child(
) {
    let s = f::setup(review_inspection_steps_v15());
    let db = f::database(&s.fixture);
    let original = summary_v14::PersonalPrSummaryPresentationV14::capture(&s.app, &db).unwrap();
    let provider = s
        .api
        .clone()
        .with_catalogue_read_deadline(original.deadline());
    let phase = PersonalSummaryHistoryPhaseV75::begin_review_inspection_v14(
        &original,
        &provider,
        &db,
        original.cancellation(),
        original.deadline(),
        &Default::default(),
    )
    .unwrap();
    assert!(phase
        .measured
        .begin_personal_pr_access_v13(
            original.parent(),
            &Default::default(),
            original.cancellation()
        )
        .is_err());
    let observed = phase
        .inspect_review_outcome_v14(Some((810, None)), false)
        .unwrap();
    assert!(observed
        .lines()
        .iter()
        .any(|line| line.contains("Current review #810")));
    let parent = phase.parent().clone();
    let identity = phase.identity().clone();
    phase
        .finish(|| {
            provider.validate_review_inspection_v11(
                &observed,
                &identity,
                &parent,
                original.cancellation(),
                original.deadline(),
            )
        })
        .unwrap();
    f::exact(&s, s.steps.len());
}

#[test]
fn personal_review_summary_phase_v15_original_narrow_capture_and_uid_database_refuse_without_body_admission(
) {
    let mut tail = f::principal();
    tail.push(f::get(&format!("repos/{}", f::REPO), f::repository()));
    let s = f::setup(tail);
    let db = f::database(&s.fixture);
    let original = summary_v14::PersonalPrSummaryPresentationV14::capture(&s.app, &db).unwrap();
    let provider = s
        .api
        .clone()
        .with_original_pr_read_bounds_v13(original.deadline(), 128)
        .unwrap();
    let foreign = s.fixture.root.join("different-summary-db.sqlite3");
    drop(Cache::open(&foreign).unwrap());
    assert!(PersonalSummaryHistoryPhaseV75::begin_review_inspection_v14(
        &original,
        &provider,
        &foreign,
        original.cancellation(),
        original.deadline(),
        &Default::default()
    )
    .is_err());
    assert!(PersonalSummaryHistoryPhaseV75::begin_review_inspection_v14(
        &original,
        &provider,
        &db,
        original.cancellation(),
        original.deadline(),
        &Default::default()
    )
    .is_err());
    assert!(original.owns_presentation(&s.app));
    f::exact(&s, s.steps.len());
}

#[test]
fn personal_review_summary_phase_v15_whole_pool_exhaustion_never_resets_or_issues_inspection() {
    let mut tail = f::principal();
    tail.push(f::get(&format!("repos/{}", f::REPO), f::repository()));
    tail.push(f::get(&format!("repos/{}/pulls/7", f::REPO), f::parent()));
    for page in 1..=9 {
        let rows=(0..100).map(|offset|{let id=page*100+offset;
            serde_json::json!({"id":id,"node_id":format!("ACTUAL_REVIEW_{id}"),"state":"PENDING","body":"x".repeat(9600),"commit_id":"a".repeat(40),"pull_request_url":format!("https://github.invalid/api/v3/repos/{}/pulls/7",f::REPO),"user":{"id":42,"login":"octocat"}})
        }).collect::<Vec<_>>();
        let step = f::get(
            &format!("repos/{}/pulls/7/reviews?per_page=100&page={page}", f::REPO),
            serde_json::json!(rows),
        );
        assert!(step.stdout.len() < 1024 * 1024);
        tail.push(step);
    }
    let s = f::setup(tail);
    let db = f::database(&s.fixture);
    let original = summary_v14::PersonalPrSummaryPresentationV14::capture(&s.app, &db).unwrap();
    let provider = s
        .api
        .clone()
        .with_catalogue_read_deadline(original.deadline());
    let limits = ReviewReadLimitsV2 {
        max_pages: 9,
        max_reviews: 1000,
        ..Default::default()
    };
    let phase = PersonalSummaryHistoryPhaseV75::begin_review_inspection_v14(
        &original,
        &provider,
        &db,
        original.cancellation(),
        original.deadline(),
        &limits,
    )
    .unwrap();
    assert!(phase.inspect_review_outcome_v14(None, false).is_err());
    drop(phase);
    f::exact(&s, s.steps.len());
}
