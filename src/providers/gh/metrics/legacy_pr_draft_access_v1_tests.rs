//! SOURCE ONLY strict provider declarations, never native/whole evidence.
use super::*;
use crate::legacy_pr_draft_access_fixture_v1 as f;
use serde_json::json;

fn window() -> LegacyPrDraftReadWindow {
    LegacyPrDraftReadWindow::new(Duration::from_secs(30)).unwrap()
}
fn load(p: &GithubProvider, w: &LegacyPrDraftReadWindow) -> GithubResult<LegacyPrDraftAccess> {
    p.legacy_pr_draft_access_v1(
        &f::identity(),
        &f::original(),
        &LegacyPrDraftAccessLimits::default(),
        w,
        &CancellationToken::default(),
    )
}

#[test]
fn legacy_pr_access_actual_five_gets_issue_opaque_positive_original_read_not_write_permission() {
    let fixture = f::fixture();
    let steps = f::group();
    let bytes = steps.iter().map(|s| s.stdout.len()).sum::<usize>();
    fixture.scripted_gh(&steps);
    let provider = f::provider(&fixture);
    let window = window();
    let access = load(&provider, &window).unwrap();
    assert_eq!(access.repository().id, 410);
    assert_eq!(access.repository().node_id, "R_410");
    assert_eq!(access.pr_database_id(), 1007);
    assert_eq!(access.identity().account_id, Some(42));
    assert_eq!(access.accounting().requests(), 5);
    assert_eq!(access.accounting().successful_stdout_bytes(), bytes);
    assert_eq!(access.accounting().successful_diagnostic_bytes(), 0);
    assert_eq!(access.accounting().failed_reserved_bytes(), 0);
    assert!(!format!("{access:?}").contains("example/repo"));
    access
        .validate_for(
            &provider,
            &f::identity(),
            &f::original(),
            &window,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 5);
}

#[test]
fn legacy_pr_access_same_uid_rename_is_actual_numeric_principal_but_uid_switch_refuses() {
    for switch in [false, true] {
        let fixture = f::fixture();
        let mut steps = f::group();
        steps[0] = f::get("user", f::user(42, "renamed-before"));
        steps[4] = f::get(
            "user",
            f::user(if switch { 43 } else { 42 }, "renamed-final"),
        );
        fixture.scripted_gh(&steps);
        let provider = f::provider(&fixture);
        let result = load(&provider, &window());
        if switch {
            assert!(matches!(
                result.unwrap_err().kind,
                GithubErrorKind::IdentityChanged
            ));
        } else {
            assert_eq!(result.unwrap().identity().login, "renamed-final");
        }
        assert!(!f::db(&fixture).exists());
        f::assert_effects(&fixture, 5);
    }
}

#[test]
fn legacy_pr_access_repo_permission_or_403_and_nonpositive_ids_never_admit_private_scope() {
    for kind in 0..5 {
        let fixture = f::fixture();
        let mut steps = f::group();
        let mut raw = f::repo();
        match kind {
            0 => {
                steps[1].stdout.clear();
                steps[1].status = 1;
                steps[1].stderr = b"gh: Resource not accessible (HTTP 403)".to_vec();
            }
            1 => {
                raw["permissions"]["pull"] = json!(false);
                steps[1] = f::get("repos/example/repo", raw);
            }
            2 => {
                raw["permissions"] = json!(null);
                steps[1] = f::get("repos/example/repo", raw);
            }
            3 => {
                raw["id"] = json!(0);
                steps[1] = f::get("repos/example/repo", raw);
            }
            _ => {
                raw["node_id"] = json!(null);
                steps[1] = f::get("repos/example/repo", raw);
            }
        }
        fixture.scripted_gh(&steps);
        let result = load(&f::provider(&fixture), &window());
        assert!(result.is_err());
        assert!(!f::db(&fixture).exists());
        f::assert_effects(&fixture, 2);
    }
}

#[test]
fn legacy_pr_access_closing_repo_database_node_name_or_permission_drift_refuses() {
    for kind in 0..4 {
        let fixture = f::fixture();
        let mut steps = f::group();
        let mut raw = f::repo();
        match kind {
            0 => raw["id"] = json!(420),
            1 => raw["node_id"] = json!("R_recreated"),
            2 => raw["full_name"] = json!("other/repo"),
            _ => raw["permissions"]["pull"] = json!(false),
        }
        steps[3] = f::get("repos/example/repo", raw);
        fixture.scripted_gh(&steps);
        assert!(load(&f::provider(&fixture), &window()).is_err());
        assert!(!f::db(&fixture).exists());
        f::assert_effects(&fixture, 4);
    }
}

#[test]
fn legacy_pr_access_parent_canonical_node_number_full_head_base_ref_and_source_are_exact() {
    for kind in 0..8 {
        let fixture = f::fixture();
        let mut steps = f::group();
        let mut raw = f::parent();
        match kind {
            0 => raw["id"] = json!(0),
            1 => raw["node_id"] = json!("PR_replacement"),
            2 => raw["number"] = json!(8),
            3 => raw["head"]["sha"] = json!("c".repeat(40)),
            4 => raw["base"]["sha"] = json!("d".repeat(40)),
            5 => raw["head"]["ref"] = json!("Feature/α"),
            6 => raw["url"] = json!("https://other.invalid/api/v3/repos/example/repo/pulls/7"),
            _ => raw["base"]["repo"]["id"] = json!(420),
        }
        steps[2] = f::get("repos/example/repo/pulls/7", raw);
        fixture.scripted_gh(&steps);
        assert!(load(&f::provider(&fixture), &window()).is_err());
        assert!(!f::db(&fixture).exists());
        f::assert_effects(&fixture, 3);
    }
}

#[test]
fn legacy_pr_access_null_states_unknown_head_source_and_unsupported_viewer_are_not_fabricated() {
    for kind in 0..5 {
        let fixture = f::fixture();
        let mut steps = f::group();
        let mut raw = f::parent();
        let count = if kind < 3 {
            match kind {
                0 => raw["draft"] = json!(null),
                1 => raw["merged"] = json!(null),
                _ => {
                    raw["head"].as_object_mut().unwrap().remove("repo");
                }
            }
            steps[2] = f::get("repos/example/repo/pulls/7", raw);
            3
        } else {
            let mut user = f::user(42, "octocat");
            if kind == 3 {
                user["type"] = json!("Bot");
            } else {
                user.as_object_mut().unwrap().remove("type");
            }
            steps[0] = f::get("user", user);
            1
        };
        fixture.scripted_gh(&steps);
        assert!(load(&f::provider(&fixture), &window()).is_err());
        assert!(!f::db(&fixture).exists());
        f::assert_effects(&fixture, count);
    }
}

#[test]
fn legacy_pr_access_actual_deleted_head_and_64_digit_git_id_are_distinct_known_source_facts() {
    let fixture = f::fixture();
    let mut original = f::original();
    original.head_sha = "a".repeat(64);
    original.base_sha = "b".repeat(64);
    let mut raw = f::parent();
    raw["head"]["sha"] = json!(original.head_sha);
    raw["base"]["sha"] = json!(original.base_sha);
    raw["head"]["repo"] = json!(null);
    let mut steps = f::group();
    steps[2] = f::get("repos/example/repo/pulls/7", raw);
    fixture.scripted_gh(&steps);
    let provider = f::provider(&fixture);
    let window = window();
    let access = provider
        .legacy_pr_draft_access_v1(
            &f::identity(),
            &original,
            &LegacyPrDraftAccessLimits::default(),
            &window,
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(access.head_repository().is_none());
    access
        .validate_for(
            &provider,
            &f::identity(),
            &original,
            &window,
            &CancellationToken::default(),
        )
        .unwrap();
    f::assert_effects(&fixture, 5);
}

#[test]
fn legacy_pr_access_invalid_original_budget_cancel_or_expired_window_launches_no_child() {
    let fixture = f::fixture();
    let provider = f::provider(&fixture);
    let mut original = f::original();
    original.head_sha = "a".repeat(7);
    assert!(provider
        .legacy_pr_draft_access_v1(
            &f::identity(),
            &original,
            &LegacyPrDraftAccessLimits::default(),
            &window(),
            &CancellationToken::default()
        )
        .is_err());
    let limits = LegacyPrDraftAccessLimits {
        max_document_bytes: 4096,
        max_response_bytes: 4 * 4096,
        ..Default::default()
    };
    assert!(provider
        .legacy_pr_draft_access_v1(
            &f::identity(),
            &f::original(),
            &limits,
            &window(),
            &CancellationToken::default()
        )
        .is_err());
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(provider
        .legacy_pr_draft_access_v1(
            &f::identity(),
            &f::original(),
            &LegacyPrDraftAccessLimits::default(),
            &window(),
            &cancel
        )
        .is_err());
    let mut expired = window();
    expired.started = Instant::now() - Duration::from_secs(31);
    assert!(load(&provider, &expired).is_err());
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 0);
}

#[test]
fn legacy_pr_access_revalidation_cannot_rebase_original_window_or_provider() {
    let fixture = f::fixture();
    fixture.scripted_gh(&f::group());
    let provider = f::provider(&fixture);
    let window = window();
    let access = load(&provider, &window).unwrap();
    let new_window = LegacyPrDraftReadWindow::new(Duration::from_secs(30)).unwrap();
    assert!(provider
        .revalidate_legacy_pr_draft_access_v1(
            &access,
            &f::identity(),
            &new_window,
            &CancellationToken::default()
        )
        .is_err());
    let other = f::provider(&fixture);
    assert!(other
        .revalidate_legacy_pr_draft_access_v1(
            &access,
            &f::identity(),
            &window,
            &CancellationToken::default()
        )
        .is_err());
    // Same runner is not same host: zero wrong-host calls, even when a caller
    // supplies the original identity/window and can clone provider internals.
    let mut cross_host = provider.clone();
    cross_host.host = "other.invalid".into();
    assert!(cross_host
        .revalidate_legacy_pr_draft_access_v1(
            &access,
            &f::identity(),
            &window,
            &CancellationToken::default()
        )
        .is_err());
    f::assert_effects(&fixture, 5);
}

#[test]
fn legacy_pr_access_revalidation_refuses_observed_repo_pr_database_and_head_source_replacement() {
    for kind in 0..3 {
        let fixture = f::fixture();
        let mut steps = f::group();
        let mut again = f::group();
        if kind == 0 {
            let mut repo = f::repo();
            repo["id"] = json!(420);
            again[1] = f::get("repos/example/repo", repo);
        } else {
            let mut parent = f::parent();
            if kind == 1 {
                parent["id"] = json!(2007);
            } else {
                parent["head"]["repo"]["node_id"] = json!("R_new_head");
            }
            again[2] = f::get("repos/example/repo/pulls/7", parent);
        }
        steps.extend(again);
        fixture.scripted_gh(&steps);
        let provider = f::provider(&fixture);
        let window = window();
        let access = load(&provider, &window).unwrap();
        assert!(provider
            .revalidate_legacy_pr_draft_access_v1(
                &access,
                &f::identity(),
                &window,
                &CancellationToken::default()
            )
            .is_err());
        f::assert_effects(&fixture, if kind == 0 { 7 } else { 8 });
    }
}

#[test]
fn legacy_pr_access_successful_raw_control_diagnostics_are_still_charged() {
    let fixture = f::fixture();
    let mut steps = f::group();
    steps[0].stderr = b"\x1b[0m".repeat(100);
    let diagnostic = steps[0].stderr.len();
    let stdout = steps.iter().map(|step| step.stdout.len()).sum::<usize>();
    fixture.scripted_gh(&steps);
    let access = load(&f::provider(&fixture), &window()).unwrap();
    assert_eq!(
        access.accounting().successful_diagnostic_bytes(),
        diagnostic
    );
    assert_eq!(access.accounting().successful_stdout_bytes(), stdout);
    assert_eq!(access.accounting().charged_bytes(), stdout + diagnostic);
    f::assert_effects(&fixture, 5);
}

#[test]
fn legacy_pr_access_failed_huge_multibyte_capture_reserves_cap_and_launches_no_next_call() {
    let fixture = f::fixture();
    let mut step = f::get("user", f::user(42, "octocat"));
    step.stdout = "🐙".repeat(8192).into_bytes();
    step.stderr = "\x1b[0m🐙".repeat(8192).into_bytes();
    step.status = 1;
    fixture.scripted_gh(&[step]);
    let provider = f::provider(&fixture);
    let window = window();
    let limits = LegacyPrDraftAccessLimits {
        max_document_bytes: 4096,
        max_response_bytes: 5 * 4096,
        ..Default::default()
    };
    let selected = limits
        .selected(&window, &CancellationToken::default())
        .unwrap();
    let mut budget = MetricsBudget::new(&selected).unwrap();
    assert!(budget
        .read_document(&provider, "user", &CancellationToken::default())
        .is_err());
    assert_eq!(budget.failed_reserved_bytes, 4096);
    assert!(budget
        .read_document(
            &provider,
            "repos/example/repo",
            &CancellationToken::default()
        )
        .is_err());
    assert_eq!(budget.requests, 1);
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 1);
}

#[test]
fn legacy_pr_access_shared_short_deadline_stops_delayed_child_before_repo_or_sql() {
    let fixture = f::fixture();
    let mut steps = f::group();
    steps[0].delay_ms = 500;
    fixture.scripted_gh(&steps);
    let window = LegacyPrDraftReadWindow::new(Duration::from_millis(80)).unwrap();
    assert!(load(&f::provider(&fixture), &window).is_err());
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 1);
}
