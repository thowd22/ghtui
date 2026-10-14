//! SOURCE ONLY declarations. Must activate common raw factor/access/local
//! forwards atomically; no normal core/provider/native test has run here.
use super::*;
use crate::{
    forms::PrFormKind, legacy_pr_draft_access_fixture_v1 as f,
    legacy_pr_draft_admitted_v1::open_admitted_legacy_pr_draft_workflow_v1,
};
use serde_json::json;
use std::{fs, time::Duration};

fn ledger(provider: &GithubProvider) -> LegacyPrDraftWorkflowBudget {
    LegacyPrDraftWorkflowBudget::new(
        provider,
        &f::identity(),
        LegacyPrDraftAccessLimits::default(),
        LegacyPrDraftReadWindow::new(Duration::from_secs(30)).unwrap(),
        LEGACY_WORKFLOW_CAPTURE_MAX,
        &CancellationToken::default(),
    )
    .unwrap()
}
fn initial(
    provider: &GithubProvider,
    workflow: &mut LegacyPrDraftWorkflowBudget,
) -> crate::providers::gh::LegacyPrDraftAccess {
    provider
        .legacy_pr_draft_access_budgeted_v1(
            &f::identity(),
            &f::original(),
            workflow,
            &CancellationToken::default(),
        )
        .unwrap()
}
fn pair(
    provider: &GithubProvider,
    workflow: &mut LegacyPrDraftWorkflowBudget,
    phase: LegacyPrIdentityPhase,
) -> GithubResult<HostIdentity> {
    provider.legacy_pr_draft_identity_budgeted_v1(
        &Scope::github("github.invalid", Some(42)).unwrap(),
        workflow,
        phase,
        &CancellationToken::default(),
    )
}

#[test]
fn legacy_workflow_all_fourteen_actual_raw_captures_one_total_and_two_access_deltas_preserve_sql_and_git(
) {
    let fixture = f::fixture();
    let raw = f::confirmed_raw(&fixture, PrFormKind::Description, "PRIVATE_WHOLE_BODY α");
    f::seed(&fixture, &raw);
    let mut steps = f::complete();
    for (index, step) in steps.iter_mut().enumerate() {
        step.stderr = if index % 2 == 0 {
            b"\x1b[0m\0\t\n".to_vec()
        } else {
            "雪 diagnostic\n".as_bytes().to_vec()
        };
    }
    let stdout: usize = steps.iter().map(|step| step.stdout.len()).sum();
    let diagnostics: usize = steps.iter().map(|step| step.stderr.len()).sum();
    fixture.scripted_gh(&steps);
    let before = fs::read(f::db(&fixture)).unwrap();
    let provider = f::provider(&fixture);
    let mut workflow = ledger(&provider);
    let whole = open_admitted_legacy_pr_draft_workflow_v1(
        f::intent(),
        None,
        &provider,
        &f::db(&fixture),
        &mut workflow,
        &CancellationToken::default(),
    )
    .unwrap();
    let accounting = whole.accounting();
    assert_eq!(accounting.requests(), 14);
    assert_eq!(accounting.successful_stdout_bytes(), stdout);
    assert_eq!(accounting.successful_diagnostic_bytes(), diagnostics);
    assert_eq!(accounting.failed_reserved_bytes(), 0);
    let ready = whole.into_admitted();
    assert_eq!(ready.initial_access_accounting().requests(), 5);
    assert_eq!(ready.final_admission().accounting().requests(), 5);
    assert!(
        ready.initial_access_accounting().charged_bytes()
            + ready.final_admission().accounting().charged_bytes()
            < accounting.charged_bytes(),
        "identity4 counts are not lost/group totals are not claimed as whole"
    );
    let (form, identity, _) = ready.into_parts();
    assert_eq!(form.fields[0].editor.text, "PRIVATE_WHOLE_BODY α");
    assert_eq!(identity.account_id, Some(42));
    assert!(!form.preview && !form.submit_requested && !form.submitted);
    assert_eq!(fs::read(f::db(&fixture)).unwrap(), before);
    assert_eq!(f::saved(&fixture), raw);
    f::assert_effects(&fixture, 14);
}

#[test]
fn legacy_workflow_capture_allocation_capacity_overflow_cancel_and_expired_clock_preflight_zero_calls(
) {
    let fixture = f::fixture();
    let provider = f::provider(&fixture);
    assert_eq!(
        10 * 128 * 1024 + 2 * LEGACY_IDENTITY_VERSION_CAPTURE + 2 * LEGACY_IDENTITY_USER_CAPTURE,
        1_581_056
    );
    for cap in [0, 1_581_055, LEGACY_WORKFLOW_CAPTURE_MAX + 1, usize::MAX] {
        assert!(LegacyPrDraftWorkflowBudget::new(
            &provider,
            &f::identity(),
            LegacyPrDraftAccessLimits::default(),
            LegacyPrDraftReadWindow::new(Duration::from_secs(30)).unwrap(),
            cap,
            &CancellationToken::default()
        )
        .is_err());
    }
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(LegacyPrDraftWorkflowBudget::new(
        &provider,
        &f::identity(),
        LegacyPrDraftAccessLimits::default(),
        LegacyPrDraftReadWindow::new(Duration::from_secs(30)).unwrap(),
        LEGACY_WORKFLOW_CAPTURE_MAX,
        &cancel
    )
    .is_err());
    let expired = LegacyPrDraftReadWindow::new(Duration::from_nanos(1)).unwrap();
    assert!(LegacyPrDraftWorkflowBudget::new(
        &provider,
        &f::identity(),
        LegacyPrDraftAccessLimits::default(),
        expired,
        LEGACY_WORKFLOW_CAPTURE_MAX,
        &CancellationToken::default()
    )
    .is_err());
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 0);
}

#[test]
fn legacy_workflow_identity_cannot_bypass_initial_access_repeat_phase_or_change_actual_producer() {
    let fixture = f::fixture();
    fixture.scripted_gh(&f::group());
    let provider = f::provider(&fixture);
    let mut workflow = ledger(&provider);
    assert!(pair(
        &provider,
        &mut workflow,
        LegacyPrIdentityPhase::BeforePrivateRead
    )
    .is_err());
    let proof = initial(&provider, &mut workflow);
    let mut rebased = ledger(&provider);
    assert!(provider
        .revalidate_legacy_pr_draft_access_budgeted_v1(
            &proof,
            &f::identity(),
            &mut rebased,
            &CancellationToken::default()
        )
        .is_err());
    let mut other = provider.clone();
    other.host = "other.invalid".into();
    assert!(pair(
        &other,
        &mut workflow,
        LegacyPrIdentityPhase::BeforePrivateRead
    )
    .is_err());
    let changed = provider
        .clone()
        .with_env("GH_FIXTURE_DIR", fixture.root.join("different-fixture"));
    assert!(pair(
        &changed,
        &mut workflow,
        LegacyPrIdentityPhase::BeforePrivateRead
    )
    .is_err());
    let changed = provider
        .clone()
        .with_program(fixture.root.join("bin/different-gh"));
    assert!(pair(
        &changed,
        &mut workflow,
        LegacyPrIdentityPhase::BeforePrivateRead
    )
    .is_err());
    assert!(pair(
        &f::provider(&fixture),
        &mut workflow,
        LegacyPrIdentityPhase::BeforePrivateRead
    )
    .is_err());
    assert!(provider
        .legacy_pr_draft_access_budgeted_v1(
            &f::identity(),
            &f::original(),
            &mut workflow,
            &CancellationToken::default()
        )
        .is_err());
    assert!(workflow
        .verify_complete(&provider, &f::identity(), &CancellationToken::default())
        .is_err());
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 5);
}

#[test]
fn legacy_workflow_unsupported_version_keeps_existing_prefix_semantics_and_never_requests_user_or_private_sql(
) {
    let fixture = f::fixture();
    let mut steps = f::group();
    let mut identity = f::identity_pair();
    identity[0].stdout = b"gh version 3.0.0\n".to_vec();
    steps.extend(identity);
    fixture.scripted_gh(&steps);
    let provider = f::provider(&fixture);
    let mut workflow = ledger(&provider);
    initial(&provider, &mut workflow);
    let failure = pair(
        &provider,
        &mut workflow,
        LegacyPrIdentityPhase::BeforePrivateRead,
    )
    .unwrap_err();
    assert!(matches!(failure.kind, GithubErrorKind::Unsupported));
    assert_eq!(workflow.accounting().requests(), 6);
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 6);
    // Do not silently change legacy parsing to the separate strict CLI parser.
    let fixture = f::fixture();
    let mut steps = f::group();
    let mut identity = f::identity_pair();
    identity[0].stdout = b"gh version 2.101.0-custom\nignored second line\n".to_vec();
    steps.extend(identity);
    fixture.scripted_gh(&steps);
    let provider = f::provider(&fixture);
    let mut workflow = ledger(&provider);
    initial(&provider, &mut workflow);
    assert_eq!(
        pair(
            &provider,
            &mut workflow,
            LegacyPrIdentityPhase::BeforePrivateRead
        )
        .unwrap()
        .gh_version,
        "gh version 2.101.0-custom"
    );
    f::assert_effects(&fixture, 7);
}

#[test]
fn legacy_workflow_missing_or_same_login_other_uid_before_private_read_never_opens_sql() {
    for case in 0..3 {
        let fixture = f::fixture();
        let mut steps = f::group();
        let mut identity = f::identity_pair();
        identity[1].stdout = match case {
            0 => json!({"login":"octocat"}).to_string().into_bytes(),
            1 => serde_json::to_vec(&f::user(43, "octocat")).unwrap(),
            _ => vec![],
        };
        steps.extend(identity);
        fixture.scripted_gh(&steps);
        let provider = f::provider(&fixture);
        let mut workflow = ledger(&provider);
        initial(&provider, &mut workflow);
        let failure = pair(
            &provider,
            &mut workflow,
            LegacyPrIdentityPhase::BeforePrivateRead,
        )
        .unwrap_err();
        if case == 2 {
            assert!(matches!(failure.kind, GithubErrorKind::Authentication));
        } else {
            assert!(matches!(failure.kind, GithubErrorKind::IdentityChanged));
        }
        assert!(!f::db(&fixture).exists());
        f::assert_effects(&fixture, 7);
    }
}

#[test]
fn legacy_workflow_huge_failed_version_shared_cap_is_reserved_and_no_following_user_or_access_request_launches(
) {
    let fixture = f::fixture();
    let mut steps = f::group();
    let mut identity = f::identity_pair();
    identity[0].stdout = "雪".repeat(12000).into_bytes();
    identity[0].stderr = "🐙".repeat(12000).into_bytes();
    identity[0].status = 1;
    steps.extend(identity);
    fixture.scripted_gh(&steps);
    let provider = f::provider(&fixture);
    let mut workflow = ledger(&provider);
    let proof = initial(&provider, &mut workflow);
    assert!(pair(
        &provider,
        &mut workflow,
        LegacyPrIdentityPhase::BeforePrivateRead
    )
    .is_err());
    assert_eq!(workflow.accounting().failed_reserved_bytes(), 4096);
    assert!(provider
        .revalidate_legacy_pr_draft_access_budgeted_v1(
            &proof,
            &f::identity(),
            &mut workflow,
            &CancellationToken::default()
        )
        .is_err());
    assert_eq!(workflow.accounting().requests(), 6);
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 6);
}

#[test]
fn legacy_workflow_identity_user_exit_reserves_full128k_and_later_closing_group_uses_same_ledger_not_reset(
) {
    let fixture = f::fixture();
    let mut steps = f::group();
    let mut identity = f::identity_pair();
    identity[1].status = 1;
    identity[1].stdout.clear();
    identity[1].stderr = b"gh: API unavailable (HTTP 500)".to_vec();
    steps.extend(identity);
    steps.extend(f::group());
    fixture.scripted_gh(&steps);
    let provider = f::provider(&fixture);
    let mut workflow = ledger(&provider);
    let initial = initial(&provider, &mut workflow);
    assert!(pair(
        &provider,
        &mut workflow,
        LegacyPrIdentityPhase::BeforePrivateRead
    )
    .is_err());
    assert_eq!(workflow.accounting().failed_reserved_bytes(), 128 * 1024);
    provider
        .revalidate_legacy_pr_draft_access_budgeted_v1(
            &initial,
            &f::identity(),
            &mut workflow,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(workflow.accounting().requests(), 12);
    assert_eq!(workflow.accounting().failed_reserved_bytes(), 128 * 1024);
    assert!(workflow
        .verify_complete(&provider, &f::identity(), &CancellationToken::default())
        .is_err());
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 12);
}

#[test]
fn legacy_workflow_final_numeric_drift_hides_private_corruption_and_never_returns_complete_form() {
    let fixture = f::fixture();
    let raw = "{PRIVATE_CORRUPTION_CANARY";
    f::seed(&fixture, raw);
    let mut steps = f::complete();
    steps[8] = f::get("user", f::user(43, "octocat"));
    fixture.scripted_gh(&steps);
    let provider = f::provider(&fixture);
    let mut workflow = ledger(&provider);
    let failure = open_admitted_legacy_pr_draft_workflow_v1(
        f::intent(),
        None,
        &provider,
        &f::db(&fixture),
        &mut workflow,
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(!format!("{failure:?}").contains("CANARY"));
    assert!(!failure.to_string().contains("CANARY"));
    assert_eq!(f::saved(&fixture), raw);
    assert_eq!(workflow.accounting().requests(), 14);
    assert!(workflow
        .verify_complete(&provider, &f::identity(), &CancellationToken::default())
        .is_err());
    f::assert_effects(&fixture, 14);
}

#[test]
fn legacy_workflow_shared_deadline_sql_wait_and_cancel_do_not_restart_clocks_or_publish_local_error(
) {
    let fixture = f::fixture();
    f::seed(&fixture, "{\"body\":\"PRIVATE_LOCK_CANARY\"}");
    let lock = rusqlite::Connection::open(f::db(&fixture)).unwrap();
    lock.execute_batch("BEGIN EXCLUSIVE").unwrap();
    fixture.scripted_gh(&f::complete());
    let provider = f::provider(&fixture);
    let mut workflow = LegacyPrDraftWorkflowBudget::new(
        &provider,
        &f::identity(),
        LegacyPrDraftAccessLimits::default(),
        LegacyPrDraftReadWindow::new(Duration::from_millis(300)).unwrap(),
        LEGACY_WORKFLOW_CAPTURE_MAX,
        &CancellationToken::default(),
    )
    .unwrap();
    let started = Instant::now();
    let failure = open_admitted_legacy_pr_draft_workflow_v1(
        f::intent(),
        None,
        &provider,
        &f::db(&fixture),
        &mut workflow,
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(!failure.to_string().contains("PRIVATE_LOCK_CANARY"));
    lock.execute_batch("ROLLBACK").unwrap();
    assert_eq!(workflow.accounting().requests(), 7);
    f::assert_effects(&fixture, 7);
    let fixture = f::fixture();
    fixture.scripted_gh(&f::group());
    let provider = f::provider(&fixture);
    let mut workflow = ledger(&provider);
    initial(&provider, &mut workflow);
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(provider
        .legacy_pr_draft_identity_budgeted_v1(
            &Scope::github("github.invalid", Some(42)).unwrap(),
            &mut workflow,
            LegacyPrIdentityPhase::BeforePrivateRead,
            &cancel
        )
        .is_err());
    assert!(!f::db(&fixture).exists());
    f::assert_effects(&fixture, 5);
}
