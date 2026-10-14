//! SOURCE ONLY binding contracts; zero declarations here have been executed.
use super::*;
use crate::{
    editor::{TextBuffer, MAX_TEXT_BYTES},
    forms::PrIntent,
    legacy_pr_draft_access_fixture_v1 as f,
    legacy_pr_draft_admitted_v1::{open_admitted_legacy_pr_draft_v1, LegacyAdmittedDraftFailure},
    legacy_pr_draft_open_v1::open_legacy_pr_draft_v1,
    providers::gh::LegacyPrDraftAccessLimits,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::time::Duration;

fn open(
    fixture: &crate::test_support::Fixture,
) -> Result<crate::legacy_pr_draft_admitted_v1::LegacyPrDraftAdmitted, LegacyAdmittedDraftFailure> {
    open_admitted_legacy_pr_draft_v1(
        f::intent(),
        None,
        &f::provider(fixture),
        &f::db(fixture),
        &LegacyPrDraftAccessLimits::default(),
        &LegacyPrDraftReadWindow::new(Duration::from_secs(30)).unwrap(),
        &CancellationToken::default(),
    )
}
fn binding_seed(fixture: &crate::test_support::Fixture, kind: PrFormKind, body: &str) -> String {
    fixture.scripted_gh(&f::group());
    let provider = f::provider(fixture);
    let window = LegacyPrDraftReadWindow::new(Duration::from_secs(30)).unwrap();
    let cancel = CancellationToken::default();
    let access = provider
        .legacy_pr_draft_access_v1(
            &f::identity(),
            &f::original(),
            &LegacyPrDraftAccessLimits::default(),
            &window,
            &cancel,
        )
        .unwrap();
    let witness = LegacyPrDraftWitness::issue(
        &access,
        &provider,
        &f::identity(),
        &f::original(),
        kind,
        &window,
        &cancel,
    )
    .unwrap();
    let mut form = Form::pr(PrIntent {
        identity: f::identity(),
        target: f::original(),
        kind,
    })
    .unwrap();
    form.fields[0].editor = TextBuffer::new(body).unwrap();
    witness.encode(&form).unwrap()
}

#[test]
fn legacy_pr_binding_unbound_versions_all_seven_kinds_are_unchanged_quarantined_not_fresh_forms() {
    for (kind, _) in PrFormKind::ALL {
        for raw in [
            json!({"body":"PRIVATE_UNBOUND α\u{001b}"}),
            json!({"version":1,"values":{"body":"PRIVATE_UNBOUND"}}),
            json!({"version":2,"values":{"body":"PRIVATE_UNBOUND"},"publication":null}),
        ] {
            let fixture = f::fixture();
            let mut intent = f::intent();
            intent.kind = kind;
            let form = Form::pr(intent.clone()).unwrap();
            let raw = raw.to_string();
            crate::storage::Cache::open(f::db(&fixture))
                .unwrap()
                .put_draft(&form.scope(), &form.resource(), &raw)
                .unwrap();
            let before = std::fs::read(f::db(&fixture)).unwrap();
            let mut steps = f::identity_pair();
            steps.extend(f::identity_pair());
            fixture.scripted_gh(&steps);
            let failure = open_legacy_pr_draft_v1(
                intent,
                &f::provider(&fixture),
                &f::db(&fixture),
                &CancellationToken::default(),
            )
            .unwrap_err();
            assert!(matches!(failure, LegacyPrDraftFailure::UnboundOriginal));
            assert!(!failure.to_string().contains("PRIVATE_UNBOUND"));
            assert_eq!(std::fs::read(f::db(&fixture)).unwrap(), before);
            f::assert_effects(&fixture, 4);
        }
    }
}

#[test]
fn legacy_pr_binding_actual_access_issued_v4_preserves_raw_text_and_approval_reset_all_seven_kinds()
{
    for (kind, _) in PrFormKind::ALL {
        let fixture = f::fixture();
        let body = "PRIVATE_BOUND α/🐙\t\n\u{001b}\u{0000}";
        let raw = binding_seed(&fixture, kind, body);
        let encoded: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(encoded["version"], 4);
        assert_eq!(encoded["target"]["repository"]["id"], 410);
        assert_eq!(encoded["target"]["pr_database_id"], 1007);
        assert_eq!(encoded["target"]["principal"], "user-id:42");
        for forbidden in [
            "approval",
            "permission",
            "gh_version",
            "verified_at",
            "window",
            "login",
            "token",
            "plan",
            "picker",
        ] {
            assert!(!raw.contains(&format!("\"{forbidden}\"")));
        }
        let mut intent = f::intent();
        intent.kind = kind;
        let form = Form::pr(intent.clone()).unwrap();
        crate::storage::Cache::open(f::db(&fixture))
            .unwrap()
            .put_draft(&form.scope(), &form.resource(), &raw)
            .unwrap();
        let before = std::fs::read(f::db(&fixture)).unwrap();
        let mut steps = f::group();
        steps.extend(f::complete());
        fixture.scripted_gh(&steps);
        // The first five were already consumed by witness issuance.
        let ready = open_admitted_legacy_pr_draft_v1(
            intent,
            None,
            &f::provider(&fixture),
            &f::db(&fixture),
            &LegacyPrDraftAccessLimits::default(),
            &LegacyPrDraftReadWindow::new(Duration::from_secs(30)).unwrap(),
            &CancellationToken::default(),
        )
        .unwrap();
        let (form, _, _) = ready.into_parts();
        assert_eq!(form.fields[0].editor.text, body);
        assert!(form.restored_draft);
        assert!(!form.preview && !form.submit_requested && !form.submitting && !form.submitted);
        assert!(form.publication.is_none());
        assert_eq!(std::fs::read(f::db(&fixture)).unwrap(), before);
        f::assert_effects(&fixture, 19);
    }
}

#[test]
fn legacy_pr_binding_restored_json_cannot_adopt_recreated_repo_pr_changed_source_or_foreign_kind() {
    for variant in 0..8 {
        let fixture = f::fixture();
        let raw = binding_seed(&fixture, PrFormKind::Description, "PRIVATE_BINDING_CANARY");
        let mut value: Value = serde_json::from_str(&raw).unwrap();
        match variant {
            0 => value["target"]["repository"]["id"] = json!(420),
            1 => value["target"]["repository"]["node"] = json!("R_recreated"),
            2 => value["target"]["pr_database_id"] = json!(1008),
            3 => value["target"]["pr_node"] = json!("PR_recreated"),
            4 => value["target"]["head"] = json!("c".repeat(40)),
            5 => value["target"]["head_repository"] = Value::Null,
            6 => value["target"]["principal"] = json!("user-id:43"),
            _ => value["kind"] = json!("title"),
        }
        let raw = value.to_string();
        f::seed(&fixture, &raw);
        let mut steps = f::group();
        steps.extend(f::complete());
        fixture.scripted_gh(&steps);
        let error = open(&fixture).unwrap_err();
        assert!(matches!(
            error,
            LegacyAdmittedDraftFailure::Local(LegacyPrDraftFailure::DifferentOriginalBinding)
        ));
        assert!(!format!("{error:?}").contains("CANARY"));
        assert_eq!(f::saved(&fixture), raw);
        f::assert_effects(&fixture, 19);
    }
}

#[test]
fn legacy_pr_binding_original_reference_lookup_precedes_edited_field_decoding() {
    let fixture = f::fixture();
    let raw=json!({"version":2,"values":{"body":"x".repeat(MAX_TEXT_BYTES+1)},"publication":{"attempt_id":19,"pr_node_id":f::original().pr.id,"observed_head":"a".repeat(40),"payload_sha256":"b".repeat(64)}}).to_string();
    f::seed(&fixture, &raw);
    fixture.scripted_gh(&f::complete());
    let error = open(&fixture).unwrap_err();
    assert!(matches!(
        error,
        LegacyAdmittedDraftFailure::Local(LegacyPrDraftFailure::InvalidOriginalPublication)
    ));
    assert_eq!(f::saved(&fixture), raw);
    f::assert_effects(&fixture, 14);
}

#[test]
fn legacy_pr_binding_original_command_selected_resource_receipt_and_metadata_budget_are_exact() {
    for variant in 0..7 {
        let fixture = f::fixture();
        let raw = f::confirmed_raw(&fixture, PrFormKind::Description, "PRIVATE_ORIGINAL_CANARY");
        f::seed(&fixture, &raw);
        let connection = Connection::open(f::db(&fixture)).unwrap();
        let sql=match variant {
            0=>"UPDATE pr_write_attempts SET command='legacy-title'",
            1=>"UPDATE pr_write_attempts SET selected_resource='different'",
            2=>"UPDATE pr_write_attempts SET repository='different/repo'",
            3=>"UPDATE pr_write_attempts SET confirmed_json='{}'",
            4=>"UPDATE pr_write_attempts SET confirmed_json='{\"kind\":\"review\",\"resource_id\":\"801\",\"node_id\":\"PRR_801\"}'",
            5=>"UPDATE pr_write_attempts SET confirmed_json='{\"kind\":\"metadata\",\"resource_id\":\"PR_other\",\"node_id\":\"PR_other\"}'",
            _=>"UPDATE pr_write_attempts SET command=printf('%09000d',1)",
        };
        connection.execute(sql, []).unwrap();
        let before = std::fs::read(f::db(&fixture)).unwrap();
        fixture.scripted_gh(&f::complete());
        let error = open(&fixture).unwrap_err();
        assert!(matches!(
            error,
            LegacyAdmittedDraftFailure::Local(LegacyPrDraftFailure::InvalidOriginalPublication)
        ));
        assert!(!format!("{error:?}").contains("CANARY"));
        assert_eq!(std::fs::read(f::db(&fixture)).unwrap(), before);
        assert_eq!(f::saved(&fixture), raw);
        f::assert_effects(&fixture, 14);
    }
}

#[test]
fn legacy_pr_binding_quarantine_diagnostic_is_hidden_by_final_access_or_principal_failure() {
    for account in [false, true] {
        let fixture = f::fixture();
        let raw = "{\"body\":\"PRIVATE_UNBOUND_CANARY\"}";
        f::seed(&fixture, raw);
        let mut steps = f::complete();
        let count = if account {
            steps[13] = f::get("user", f::user(43, "octocat"));
            14
        } else {
            steps[10].status = 1;
            steps[10].stdout.clear();
            steps[10].stderr = b"gh: Not Found (HTTP 404)".to_vec();
            11
        };
        fixture.scripted_gh(&steps);
        let error = open(&fixture).unwrap_err();
        assert!(matches!(error, LegacyAdmittedDraftFailure::Access(_)));
        assert!(!error.to_string().contains("quarantined"));
        assert!(!format!("{error:?}").contains("PRIVATE_UNBOUND"));
        assert_eq!(f::saved(&fixture), raw);
        f::assert_effects(&fixture, count);
    }
}

#[test]
fn legacy_pr_binding_v4_budget_covers_actual_max_editor_and_explicit_fact_field_bounds() {
    let fixture = f::fixture();
    let raw = binding_seed(
        &fixture,
        PrFormKind::Description,
        &"\u{001b}".repeat(MAX_TEXT_BYTES),
    );
    assert!(raw.len() > MAX_TEXT_BYTES * 6);
    assert!(raw.len() < LEGACY_PR_DRAFT_SERIALIZED_BYTES);
    let cap = std::hint::black_box(LEGACY_PR_DRAFT_SERIALIZED_BYTES);
    assert!(cap < 2 * 1024 * 1024);
    let repository_bound = HOST_BYTES + REPOSITORY_BYTES + NODE_BYTES;
    let persisted_bound =
        2 * repository_bound + HOST_BYTES + 28 + NODE_BYTES + 2 * REF_BYTES + 2 * 64 + 32;
    let reference_bound = 4096 + 64 + 64;
    assert_eq!(
        cap,
        6 * (MAX_TEXT_BYTES + persisted_bound + reference_bound) + 4096
    );
    assert_eq!(draft_header(&raw).unwrap().version, Some(4));
    f::assert_effects(&fixture, 5);
}

#[test]
fn legacy_pr_binding_identity_only_reader_cannot_use_serialized_v4_as_access_authority() {
    let fixture = f::fixture();
    let raw = binding_seed(&fixture, PrFormKind::Description, "PRIVATE_V4_CANARY");
    f::seed(&fixture, &raw);
    let mut steps = f::group();
    steps.extend(f::identity_pair());
    steps.extend(f::identity_pair());
    fixture.scripted_gh(&steps);
    let error = open_legacy_pr_draft_v1(
        f::intent(),
        &f::provider(&fixture),
        &f::db(&fixture),
        &CancellationToken::default(),
    )
    .unwrap_err();
    assert!(matches!(error, LegacyPrDraftFailure::UnboundOriginal));
    assert_eq!(f::saved(&fixture), raw);
    f::assert_effects(&fixture, 9);
}
