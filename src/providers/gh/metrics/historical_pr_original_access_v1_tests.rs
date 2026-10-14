//! SOURCE ONLY strict provider/SQL declarations, not native/pass evidence.
use super::*;
use crate::{
    forms::PrFormKind,
    legacy_pr_draft_access_fixture_v1 as f,
    pr_write_journal::{readonly_original_publication_v1, PrPublicationReference},
    test_support::GhStep,
};
use rusqlite::Connection;
use serde_json::json;
use std::{collections::BTreeMap, fs, time::Duration};

#[test]
fn historical_original_pending_uncertain_and_acknowledged_remain_actual_history_not_confirmation_or_resend(
) {
    use crate::pr_write_journal::{PrWriteIntent, PrWriteJournal, PrWriteState};
    for state in [
        PrWriteState::Pending,
        PrWriteState::Uncertain,
        PrWriteState::Acknowledged,
    ] {
        let fixture = f::fixture();
        let intent = PrWriteIntent {
            command: "new-comment".into(),
            selected_resource: "pr".into(),
            observed_head: "a".repeat(40),
            values: BTreeMap::from([(
                "body".into(),
                "PRIVATE_NEVER_RETAINED_ORIGINAL_BODY".into(),
            )]),
        };
        let mut journal = PrWriteJournal::open(f::db(&fixture)).unwrap();
        let permit = journal.begin(&target(), &intent).unwrap();
        if state != PrWriteState::Pending {
            journal.claim(&permit, &target(), &intent).unwrap();
            journal.mark_uncertain(&permit).unwrap();
        }
        let reference = permit.marker().publication_reference();
        drop(journal);
        if state == PrWriteState::Acknowledged {
            // Seed an already-recorded historical acknowledgement; this reader
            // neither performs it nor interprets it as confirmed request effect.
            Connection::open(f::db(&fixture))
                .unwrap()
                .execute(
                    "UPDATE pr_write_attempts SET state='acknowledged' WHERE attempt_id=?1",
                    [reference.attempt_id],
                )
                .unwrap();
        }
        let before = fs::read(f::db(&fixture)).unwrap();
        let mut steps = group();
        steps.extend(group());
        fixture.scripted_gh(&steps);
        let provider = f::provider(&fixture);
        let window = window();
        let proof = load(&provider, &window).unwrap();
        let sql = Connection::open_with_flags(
            f::db(&fixture),
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let marker =
            readonly_original_publication_v1(&sql, &target(), &reference, Some(window.deadline()))
                .unwrap()
                .unwrap();
        assert_eq!(marker.state, state);
        assert!(marker.confirmed.is_none());
        assert_eq!(marker.observed_head, "a".repeat(40));
        assert!(!format!("{marker:?}").contains("PRIVATE_NEVER_RETAINED"));
        let closed = provider
            .revalidate_historical_pr_original_access_with_guard_v1(
                &proof,
                &f::identity(),
                &window,
                &CancellationToken::default(),
                |context| {
                    let actual = readonly_original_publication_v1(
                        &sql,
                        context.target(),
                        &reference,
                        Some(window.deadline()),
                    )
                    .unwrap()
                    .unwrap();
                    assert_eq!(actual.state, state);
                    assert!(actual.confirmed.is_none());
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(closed.current_source().head_sha(), "c".repeat(40));
        assert_eq!(fs::read(f::db(&fixture)).unwrap(), before);
        f::assert_effects(&fixture, 10);
    }
}

fn window() -> LegacyPrDraftReadWindow {
    LegacyPrDraftReadWindow::new(Duration::from_secs(30)).unwrap()
}
fn target() -> PrWriteTarget {
    PrWriteTarget::new(&f::identity(), &f::original()).unwrap()
}
fn current() -> Value {
    let mut raw = f::parent();
    raw["head"]["sha"] = json!("c".repeat(40));
    raw["head"]["ref"] = json!("after-push/α");
    raw["base"]["sha"] = json!("d".repeat(40));
    raw["base"]["ref"] = json!("release");
    raw
}
fn group() -> Vec<GhStep> {
    let mut steps = f::group();
    steps[2] = f::get("repos/example/repo/pulls/7", current());
    steps
}
fn load(
    provider: &GithubProvider,
    window: &LegacyPrDraftReadWindow,
) -> GithubResult<HistoricalPrOriginalAccess> {
    provider.historical_pr_original_access_v1(
        &f::identity(),
        &target(),
        &LegacyPrDraftAccessLimits::default(),
        window,
        &CancellationToken::default(),
    )
}
fn commands(fixture: &crate::test_support::Fixture) -> usize {
    let path = fixture.root.join("gh-fixture/commands.jsonl");
    if path.exists() {
        fs::read_to_string(path).unwrap().lines().count()
    } else {
        0
    }
}

#[test]
fn historical_original_actual_old_head_remains_historical_and_current_access_getters_do_not_invent_old_db_ids(
) {
    let fixture = f::fixture();
    fixture.scripted_gh(&group());
    let provider = f::provider(&fixture);
    let window = window();
    let proof = load(&provider, &window).unwrap();
    proof
        .validate_for(
            &provider,
            &f::identity(),
            &target(),
            &window,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(proof.repository().id, 410);
    assert_eq!(proof.repository().node_id, "R_410");
    assert_eq!(proof.current_source().database_id(), 1007);
    assert_eq!(proof.current_source().node_id(), target().node_id());
    assert_eq!(proof.current_source().head_sha(), "c".repeat(40));
    assert_eq!(proof.current_source().base_sha(), "d".repeat(40));
    assert_eq!(proof.current_source().head_ref(), "after-push/α");
    assert_eq!(f::original().head_sha, "a".repeat(40));
    assert_eq!(proof.accounting().requests(), 5);
    assert_eq!(proof.accounting().failed_reserved_bytes(), 0);
    assert!(!format!("{proof:?}").contains("after-push"));
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 5);
}

#[test]
fn historical_original_closing_sql_hook_after_pr8_before_repo9_user10_preserves_genuine_receipt_without_resend(
) {
    let fixture = f::fixture();
    let raw = f::confirmed_raw(
        &fixture,
        PrFormKind::Description,
        "PRIVATE_ORIGINAL_VALUE_CANARY",
    );
    let reference: PrPublicationReference =
        serde_json::from_value(serde_json::from_str::<Value>(&raw).unwrap()["publication"].clone())
            .unwrap();
    let before = fs::read(f::db(&fixture)).unwrap();
    let mut steps = group();
    steps.extend(group());
    fixture.scripted_gh(&steps);
    let provider = f::provider(&fixture);
    let window = window();
    let proof = load(&provider, &window).unwrap();
    let sql =
        Connection::open_with_flags(f::db(&fixture), rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    assert_eq!(commands(&fixture), 5);
    let original =
        readonly_original_publication_v1(&sql, &target(), &reference, Some(window.deadline()))
            .unwrap()
            .unwrap();
    assert_eq!(original.observed_head, "a".repeat(40));
    assert_eq!(original.command, "legacy-description");
    assert_eq!(original.selected_resource, "pr");
    assert_eq!(
        original.state,
        crate::pr_write_journal::PrWriteState::Confirmed
    );
    let mut recaptured = None;
    let closed = provider
        .revalidate_historical_pr_original_access_with_guard_v1(
            &proof,
            &f::identity(),
            &window,
            &CancellationToken::default(),
            |context| {
                assert_eq!(
                    commands(&fixture),
                    8,
                    "all private SQL occurs before repo9/user10"
                );
                assert_eq!(context.identity().account_id, Some(42));
                assert_eq!(context.target(), &target());
                assert_eq!(context.repository().id, 410);
                assert_eq!(context.current_source().head_sha(), "c".repeat(40));
                let actual = readonly_original_publication_v1(
                    &sql,
                    context.target(),
                    &reference,
                    Some(window.deadline()),
                )
                .unwrap()
                .unwrap();
                assert_eq!(actual.publication_reference(), reference);
                assert_eq!(actual.command, original.command);
                assert_eq!(actual.confirmed, original.confirmed);
                recaptured = Some(actual);
                Ok(())
            },
        )
        .unwrap();
    // No SQL after final sealed completion in the presentation path.
    assert_eq!(closed.current_source().head_sha(), "c".repeat(40));
    assert_eq!(recaptured.unwrap().observed_head, "a".repeat(40));
    assert_eq!(fs::read(f::db(&fixture)).unwrap(), before);
    f::assert_effects(&fixture, 10);
}

#[test]
fn historical_original_scope_host_window_or_producer_substitution_preflights_zero_new_calls() {
    let fixture = f::fixture();
    fixture.scripted_gh(&group());
    let provider = f::provider(&fixture);
    let window = window();
    let proof = load(&provider, &window).unwrap();
    let mut other = provider.clone();
    other.host = "other.invalid".into();
    assert!(other
        .revalidate_historical_pr_original_access_v1(
            &proof,
            &f::identity(),
            &window,
            &CancellationToken::default()
        )
        .is_err());
    let mut foreign = f::identity();
    foreign.account_id = Some(43);
    assert!(provider
        .historical_pr_original_access_v1(
            &foreign,
            &target(),
            &LegacyPrDraftAccessLimits::default(),
            &window,
            &CancellationToken::default()
        )
        .is_err());
    assert!(provider
        .revalidate_historical_pr_original_access_v1(
            &proof,
            &f::identity(),
            &self::window(),
            &CancellationToken::default()
        )
        .is_err());
    assert!(f::provider(&fixture)
        .revalidate_historical_pr_original_access_v1(
            &proof,
            &f::identity(),
            &window,
            &CancellationToken::default()
        )
        .is_err());
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 5);
}

#[test]
fn historical_original_first_user_repo403_or_recreated_opaque_parent_refuses_before_private_read() {
    for variant in 0..4 {
        let fixture = f::fixture();
        let mut steps = group();
        let count = match variant {
            0 => {
                steps[0] = f::get("user", f::user(43, "octocat"));
                1
            }
            1 => {
                steps[1].status = 1;
                steps[1].stdout.clear();
                steps[1].stderr = b"gh: Resource not accessible (HTTP 403)".to_vec();
                2
            }
            2 => {
                let mut raw = current();
                raw["node_id"] = json!("PR_recreated_same_number");
                steps[2] = f::get("repos/example/repo/pulls/7", raw);
                3
            }
            _ => {
                let mut raw = f::repo();
                raw["full_name"] = json!("moved/repo");
                steps[1] = f::get("repos/example/repo", raw);
                2
            }
        };
        fixture.scripted_gh(&steps);
        assert!(load(&f::provider(&fixture), &window()).is_err());
        assert!(!f::db(&fixture).exists());
        f::assert_effects(&fixture, count);
    }
}

#[test]
fn historical_original_revalidation_replaced_database_ids_or_current_source_never_retargests_history_or_runs_sql_hook(
) {
    for variant in 0..6 {
        let fixture = f::fixture();
        let mut steps = group();
        let mut closing = group();
        let count = if variant == 0 {
            let mut raw = f::repo();
            raw["id"] = json!(420);
            closing[1] = f::get("repos/example/repo", raw);
            7
        } else {
            let mut raw = current();
            match variant {
                1 => raw["id"] = json!(1008),
                2 => raw["head"]["sha"] = json!("e".repeat(40)),
                3 => raw["base"]["ref"] = json!("changed"),
                4 => raw["state"] = json!("closed"),
                _ => raw["head"]["repo"] = Value::Null,
            };
            closing[2] = f::get("repos/example/repo/pulls/7", raw);
            8
        };
        steps.extend(closing);
        fixture.scripted_gh(&steps);
        let provider = f::provider(&fixture);
        let window = window();
        let proof = load(&provider, &window).unwrap();
        let mut hook_called = false;
        assert!(provider
            .revalidate_historical_pr_original_access_with_guard_v1(
                &proof,
                &f::identity(),
                &window,
                &CancellationToken::default(),
                |_| {
                    hook_called = true;
                    Ok(())
                }
            )
            .is_err());
        assert!(!hook_called);
        assert!(!f::db(&fixture).exists());
        f::assert_effects(&fixture, count);
    }
}

#[test]
fn historical_original_missing_full_current_facts_do_not_default_but_explicit_deleted_head_and_64_sha_are_known(
) {
    for variant in 0..7 {
        let fixture = f::fixture();
        let mut steps = group();
        let mut raw = current();
        match variant {
            0 => raw["head"]["sha"] = json!("c".repeat(7)),
            1 => raw["base"]["sha"] = Value::Null,
            2 => raw["draft"] = Value::Null,
            3 => raw["merged"] = Value::Null,
            4 => {
                raw["head"].as_object_mut().unwrap().remove("repo");
            }
            5 => raw["base"]["repo"]["node_id"] = Value::Null,
            _ => {
                raw["head"]["repo"] = Value::Null;
                raw["head"]["sha"] = json!("c".repeat(64));
            }
        };
        steps[2] = f::get("repos/example/repo/pulls/7", raw);
        fixture.scripted_gh(&steps);
        let result = load(&f::provider(&fixture), &window());
        if variant == 6 {
            let proof = result.unwrap();
            assert!(proof.current_source().head_repository().is_none());
            assert_eq!(proof.current_source().head_sha().len(), 64);
            f::assert_effects(&fixture, 5);
        } else {
            assert!(result.is_err());
            f::assert_effects(&fixture, 3);
        }
        assert!(!f::db(&fixture).exists());
    }
}

#[test]
fn historical_original_post_sql_repo_or_uid_failure_discards_prepared_private_metadata() {
    for uid in [false, true] {
        let fixture = f::fixture();
        let mut steps = group();
        let mut closing = group();
        let count = if uid {
            closing[4] = f::get("user", f::user(43, "octocat"));
            10
        } else {
            closing[3].status = 1;
            closing[3].stdout.clear();
            closing[3].stderr = b"gh: Resource not accessible (HTTP 403)".to_vec();
            9
        };
        steps.extend(closing);
        fixture.scripted_gh(&steps);
        let provider = f::provider(&fixture);
        let window = window();
        let proof = load(&provider, &window).unwrap();
        let mut called = false;
        let result = provider.revalidate_historical_pr_original_access_with_guard_v1(
            &proof,
            &f::identity(),
            &window,
            &CancellationToken::default(),
            |context| {
                assert_eq!(commands(&fixture), 8);
                assert_eq!(context.current_source().head_sha(), "c".repeat(40));
                called = true;
                Ok(())
            },
        );
        assert!(called);
        assert!(result.is_err());
        assert!(!f::db(&fixture).exists());
        f::assert_effects(&fixture, count);
    }
}

#[test]
fn historical_original_callback_private_error_is_redacted_and_suppresses_closing_transport_and_proof(
) {
    let fixture = f::fixture();
    let mut steps = group();
    steps.extend(group());
    fixture.scripted_gh(&steps);
    let provider = f::provider(&fixture);
    let window = window();
    let proof = load(&provider, &window).unwrap();
    let failure = provider
        .revalidate_historical_pr_original_access_with_guard_v1(
            &proof,
            &f::identity(),
            &window,
            &CancellationToken::default(),
            |context| {
                assert_eq!(context.target(), &target());
                Err(error(
                    GithubErrorKind::Transport,
                    "PRIVATE_SQL_ORIGINAL_DIAGNOSTIC_CANARY",
                ))
            },
        )
        .unwrap_err();
    assert!(!failure.message.contains("CANARY"));
    assert!(!format!("{failure:?}").contains("CANARY"));
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 8);
}

#[test]
fn historical_original_callback_time_and_cancellation_use_same_outer_budget_before_closing_repo_get(
) {
    for cancelled in [false, true] {
        let fixture = f::fixture();
        let mut steps = group();
        steps.extend(group());
        fixture.scripted_gh(&steps);
        let provider = f::provider(&fixture);
        let window = LegacyPrDraftReadWindow::new(Duration::from_millis(400)).unwrap();
        let cancel = CancellationToken::default();
        let proof = provider
            .historical_pr_original_access_v1(
                &f::identity(),
                &target(),
                &LegacyPrDraftAccessLimits::default(),
                &window,
                &cancel,
            )
            .unwrap();
        assert!(provider
            .revalidate_historical_pr_original_access_with_guard_v1(
                &proof,
                &f::identity(),
                &window,
                &cancel,
                |_| {
                    if cancelled {
                        cancel.cancel();
                    } else {
                        std::thread::sleep(Duration::from_millis(500));
                    }
                    Ok(())
                }
            )
            .is_err());
        assert!(!f::db(&fixture).exists());
        f::assert_effects(&fixture, 8);
    }
}

#[test]
fn historical_original_raw_control_stderr_capture_counts_are_actual_and_huge_failed_capture_launches_no_next_get(
) {
    let fixture = f::fixture();
    let mut steps = group();
    let diagnostic = b"\x1b[0m\x00\t\n";
    steps[0].stderr = diagnostic.to_vec();
    fixture.scripted_gh(&steps);
    let proof = load(&f::provider(&fixture), &window()).unwrap();
    assert_eq!(
        proof.accounting().successful_diagnostic_bytes(),
        diagnostic.len()
    );
    f::assert_effects(&fixture, 5);
    let fixture = f::fixture();
    let mut steps = group();
    steps[1].stdout = "雪".repeat(12000).into_bytes();
    steps[1].stderr = "🐙".repeat(12000).into_bytes();
    steps[1].status = 1;
    fixture.scripted_gh(&steps);
    let limits = LegacyPrDraftAccessLimits {
        max_document_bytes: 4096,
        max_response_bytes: 5 * 4096,
        max_duration: Duration::from_secs(10),
    };
    assert!(f::provider(&fixture)
        .historical_pr_original_access_v1(
            &f::identity(),
            &target(),
            &limits,
            &window(),
            &CancellationToken::default()
        )
        .is_err());
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 2);
}

#[test]
fn historical_original_invalid_cap_or_precancel_never_launches_transport_or_opens_private_database()
{
    let fixture = f::fixture();
    let provider = f::provider(&fixture);
    let window = window();
    let mut limits = LegacyPrDraftAccessLimits::default();
    limits.max_response_bytes = 1;
    assert!(provider
        .historical_pr_original_access_v1(
            &f::identity(),
            &target(),
            &limits,
            &window,
            &CancellationToken::default()
        )
        .is_err());
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(provider
        .historical_pr_original_access_v1(
            &f::identity(),
            &target(),
            &LegacyPrDraftAccessLimits::default(),
            &window,
            &cancel
        )
        .is_err());
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 0);
}
