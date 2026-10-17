//! Genuine ordinary Personal Summary factories, actual journal/SQL and strict
//! same-Core raw parents. SOURCE ONLY declarations; never synthetic Files.
use super::*;
use crate::{
    pr_write_journal::{PrWriteIntent, PrWriteJournal, PrWriteTarget},
    scoped_pr_observed_tabs_v1::resources::immutable_images::reviews_v2::{
        native_v2 as reviews_native, operations_v3::history_v3 as history,
    },
};

pub(crate) fn summary_phase() -> Vec<GhStep> {
    let mut steps = principal();
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.extend(principal());
    assert_eq!(steps.len(), 8);
    steps
}
pub(crate) fn seed(s: &mut Setup) -> PrWriteTarget {
    let github = s.app.github.as_ref().unwrap();
    let target = PrWriteTarget::new(
        &github.identity,
        &github.pr.as_ref().unwrap().details.summary,
    )
    .unwrap();
    let intent = PrWriteIntent {
        command: "review-submit-v2".into(),
        selected_resource: "review/810".into(),
        observed_head: "a".repeat(40),
        values: std::collections::BTreeMap::from([(
            "body".into(),
            "OLD_PROTECTED_UNKNOWN_ORIGIN_REVIEW_BODY".into(),
        )]),
    };
    let mut journal = PrWriteJournal::open(&database(&s.fixture)).unwrap();
    let permit = journal.begin(&target, &intent).unwrap();
    journal.claim(&permit, &target, &intent).unwrap();
    journal.mark_uncertain(&permit).unwrap();
    Cache::open(&database(&s.fixture))
        .unwrap()
        .put_draft(
            target.scope(),
            "protected-v2-no-origin",
            "OLD_PROTECTED_UNKNOWN_ORIGIN_REVIEW_BODY",
        )
        .unwrap();
    s.before_sql = sql(&s.fixture);
    target
}

#[test]
fn personal_pr_summary_v15_protected_review_list_and_open_use_actual_summary_without_files_source()
{
    let mut tail = summary_phase();
    tail.extend(summary_phase());
    let mut s = setup(tail);
    let target = seed(&mut s);
    let db = database(&s.fixture);
    let body = s
        .app
        .github
        .as_ref()
        .unwrap()
        .pr
        .as_ref()
        .unwrap()
        .details
        .summary
        .pr
        .body
        .as_ptr();
    let original =
        history::ReviewOperationHistoryIntentV3::capture_personal_v15(&s.app, &db).unwrap();
    let list = history::load(
        original.clone(),
        None,
        &s.api,
        &db,
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert_eq!(list.rows(), 1);
    let opened = history::load(
        original.clone(),
        Some((&list, 0)),
        &s.api,
        &db,
        &CancellationToken::default(),
        Instant::now() + Duration::from_secs(30),
    )
    .unwrap();
    assert!(Arc::ptr_eq(opened.intent(), &original));
    assert_eq!(opened.opened, Some(0));
    let mut viewport = Default::default();
    let window = opened.display.window(&mut viewport, 88, 28, 0).unwrap();
    let text = window
        .rows()
        .map(|row| row.text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("Protected legacy record has no original observation"));
    assert!(!text.contains("OLD_PROTECTED_UNKNOWN_ORIGIN_REVIEW_BODY"));
    assert!(s.app.remote_scope_source.is_none() && s.app.personal_pr_source_v13.is_none());
    assert!(s
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .is_none());
    assert!(s.app.github.as_ref().unwrap().scoped_pr_resource.is_none());
    assert_eq!(
        s.app
            .github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .body
            .as_ptr(),
        body
    );
    assert!(PrWriteJournal::open(&db)
        .unwrap()
        .blocking(&target)
        .unwrap()
        .is_some());
    exact(&s, s.steps.len());
}

#[test]
fn personal_pr_summary_v15_actual_f9_capture_adopts_original_token_clock_without_io_or_page_move() {
    let s = setup(Vec::new());
    let mut app = s.app;
    let page = app.github.as_ref().unwrap().pr.as_ref().unwrap();
    let rows = page.rows.as_ptr();
    let body = page.details.summary.pr.body.as_ptr();
    app.dispatch(crate::app::Action::Github(
        crate::github_workspace::GithubAction::ReviewOperationHistoryV3,
    ));
    assert_eq!(
        app.github_action,
        Some(crate::github_workspace::GithubAction::ReviewOperationHistoryV3)
    );
    assert!(reviews_native::capture_requested_history_v15(
        &mut app,
        &database(&s.fixture)
    ));
    let work = app.scoped_reviews_work_v2.as_ref().unwrap();
    let original = work
        .origin()
        .summary()
        .unwrap()
        .personal_summary_v15()
        .unwrap();
    assert!(work
        .cancellation_v15()
        .same_instance(original.cancellation()));
    assert!(work.original_deadline_v15() <= original.deadline());
    assert!(work.matches(&app));
    assert!(app.github_action.is_none());
    assert_eq!(
        app.github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .rows
            .as_ptr(),
        rows
    );
    assert_eq!(
        app.github
            .as_ref()
            .unwrap()
            .pr
            .as_ref()
            .unwrap()
            .details
            .summary
            .pr
            .body
            .as_ptr(),
        body
    );
    assert_eq!(sql(&s.fixture), s.before_sql);
    assert_eq!(
        fs::read_to_string(s.fixture.root.join("gh-fixture/commands.jsonl"))
            .unwrap()
            .lines()
            .count(),
        opening().len()
    );
}

#[test]
fn personal_pr_summary_v15_idle_account_loss_hides_original_header_body_and_preserves_newer_page() {
    for newer in [false, true] {
        let s = setup(Vec::new());
        let original = history::ReviewOperationHistoryIntentV3::capture_personal_v15(
            &s.app,
            &database(&s.fixture),
        )
        .unwrap();
        let mut app = s.app;
        if newer {
            app = setup(Vec::new()).app;
        }
        app.github.as_mut().unwrap().identity.account_id = Some(43);
        app.status = "INDEPENDENT_SUMMARY_STATUS".into();
        app.loading[View::Inbox.index()] = true;
        let context = app.context.clone();
        assert_eq!(original.retire_account_mismatch_v15(&mut app), !newer);
        assert_eq!(app.status, "INDEPENDENT_SUMMARY_STATUS");
        assert!(app.loading[View::Inbox.index()]);
        if newer {
            assert!(app.github.is_some());
            assert_eq!(app.context, context);
        } else {
            assert!(app.github.is_none());
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
            terminal
                .draw(|frame| crate::ui::draw(frame, &mut app))
                .unwrap();
            let text = terminal
                .backend()
                .buffer()
                .content()
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(!text.contains(BODY) && !text.contains(REPO));
        }
    }
}

pub(crate) fn inspection_phase_v15() -> Vec<GhStep> {
    let mut steps = principal();
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    let review = json!({"id":810,"node_id":"ACTUAL_PENDING_REVIEW_810","state":"PENDING","body":"Fresh current server summary, not the old attempt", "commit_id":"a".repeat(40),
        "pull_request_url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/7"),"user":{"id":42,"login":"octocat","type":"User"}});
    steps.push(get(
        &format!("repos/{REPO}/pulls/7/reviews?per_page=100&page=1"),
        json!([review.clone()]),
    ));
    steps.push(get(
        &format!("repos/{REPO}/pulls/7/reviews/810"),
        review.clone(),
    ));
    steps.push(get(
        &format!("repos/{REPO}/pulls/7/reviews/810/comments?per_page=100&page=1"),
        json!([]),
    ));
    steps.push(get(&format!("repos/{REPO}/pulls/7/reviews/810"), review));
    steps.push(get(&format!("repos/{REPO}/pulls/7"), parent()));
    steps.push(get(&format!("repos/{REPO}"), repository()));
    steps.extend(principal());
    assert_eq!(steps.len(), 12);
    steps
}

#[test]
fn personal_pr_summary_v15_fresh_inspect_then_explicit_ack_uses_same_journal_and_never_restores_body_or_permission(
) {
    let mut tail = summary_phase();
    tail.extend(summary_phase());
    tail.extend(inspection_phase_v15());
    tail.extend(summary_phase());
    let mut s = setup(tail);
    let target = seed(&mut s);
    let db = database(&s.fixture);
    let runner = crate::providers::runner::CommandRunner::default();
    runner.mark_ambiguous(&crate::providers::gh::canonical_github_resource(
        "github.invalid",
        &format!("github.invalid/{REPO}/pull/7"),
    ));
    let provider = s.api.clone().with_runner(runner);
    let intent =
        history::ReviewOperationHistoryIntentV3::capture_personal_v15(&s.app, &db).unwrap();
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(30);
    let list = history::load(intent.clone(), None, &provider, &db, &token, deadline).unwrap();
    let detail = history::load(intent, Some((&list, 0)), &provider, &db, &token, deadline).unwrap();
    let request = history::recovery_v11::ReviewRecoveryRequestV11::capture(detail, 0).unwrap();
    let inspected = history::recovery_v11::inspect(
        request,
        &provider,
        &db,
        &Default::default(),
        &token,
        deadline,
    )
    .unwrap();
    assert!(inspected.available());
    assert!(PrWriteJournal::open(&db)
        .unwrap()
        .blocking(&target)
        .unwrap()
        .is_some());
    assert_eq!(sql(&s.fixture), s.before_sql);
    let generation_before = PrWriteJournal::open(&db)
        .unwrap()
        .source_generation(&target)
        .unwrap();
    let expected_sql = s
        .before_sql
        .iter()
        .map(|line| {
            if line.starts_with("pr_write_attempts:") {
                let old = format!(
                    "{:?}",
                    format!("{:?}", rusqlite::types::ValueRef::Text(b"uncertain"))
                );
                let new = format!(
                    "{:?}",
                    format!("{:?}", rusqlite::types::ValueRef::Text(b"acknowledged"))
                );
                assert!(line.contains(&old));
                line.replace(&old, &new)
            } else if line.starts_with("pr_write_scopes:") {
                let old = format!(
                    "{:?}]",
                    format!(
                        "{:?}",
                        rusqlite::types::ValueRef::Integer(generation_before)
                    )
                );
                assert!(line.ends_with(&old));
                format!(
                    "{}{:?}]",
                    &line[..line.len() - old.len()],
                    format!(
                        "{:?}",
                        rusqlite::types::ValueRef::Integer(generation_before + 1)
                    )
                )
            } else {
                line.clone()
            }
        })
        .collect::<Vec<_>>();
    let result =
        history::recovery_v11::acknowledge(&inspected, &provider, &db, &token, deadline).unwrap();
    assert_eq!(result.changed, 1);
    assert!(!inspected.available());
    assert!(PrWriteJournal::open(&db)
        .unwrap()
        .blocking(&target)
        .unwrap()
        .is_none());
    assert!(
        history::recovery_v11::acknowledge(&inspected, &provider, &db, &token, deadline).is_err()
    );
    assert_eq!(
        Cache::open(&db)
            .unwrap()
            .bounded_draft(target.scope(), "protected-v2-no-origin", 128 * 1024)
            .unwrap()
            .as_deref(),
        Some("OLD_PROTECTED_UNKNOWN_ORIGIN_REVIEW_BODY")
    );
    assert!(s.app.personal_pr_source_v13.is_none() && s.app.remote_scope_source.is_none());
    assert!(s.app.github.as_ref().unwrap().scoped_pr_resource.is_none());
    assert!(s
        .app
        .github
        .as_ref()
        .unwrap()
        .scoped_pr_navigation
        .is_none());
    assert_eq!(
        PrWriteJournal::open(&db)
            .unwrap()
            .source_generation(&target)
            .unwrap(),
        generation_before + 1
    );
    s.before_sql = expected_sql;
    exact(&s, s.steps.len());
}
