//! SOURCE ONLY access/identity/SQLite declarations. Real-binary parity is a
//! later separately registered journey, not replaced by these worker tests.
use super::*;
use crate::{
    forms::FormPurpose,
    legacy_pr_draft_access_fixture_v1 as f,
    legacy_pr_draft_open_v1::{open_legacy_pr_draft_before_v1, LEGACY_PR_DRAFT_SERIALIZED_BYTES},
    pr_write_journal::PrWriteTarget,
    storage::Cache,
};
use rusqlite::Connection;
use serde_json::json;
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    time::{Duration, Instant},
};

fn window() -> LegacyPrDraftReadWindow {
    LegacyPrDraftReadWindow::new(Duration::from_secs(30)).unwrap()
}
fn open(
    fixture: &crate::test_support::Fixture,
    provider: &GithubProvider,
    window: &LegacyPrDraftReadWindow,
) -> Result<LegacyPrDraftAdmitted, LegacyAdmittedDraftFailure> {
    open_admitted_legacy_pr_draft_v1(
        f::intent(),
        None,
        provider,
        &f::db(fixture),
        &LegacyPrDraftAccessLimits::default(),
        window,
        &CancellationToken::default(),
    )
}

#[test]
fn legacy_pr_admitted_open_actual_fourteen_get_version_calls_preserve_private_text_and_git() {
    let fixture = f::fixture();
    let raw = f::confirmed_raw(
        &fixture,
        crate::forms::PrFormKind::Description,
        "PRIVATE_UID42 α\n\u{001b}",
    );
    f::seed(&fixture, &raw);
    fixture.scripted_gh(&f::complete());
    let provider = f::provider(&fixture);
    let window = window();
    let ready = open(&fixture, &provider, &window).unwrap();
    assert!(!format!("{ready:?}").contains("PRIVATE_UID42"));
    assert_eq!(ready.initial_access_accounting().requests(), 5);
    assert_eq!(ready.final_admission().accounting().requests(), 5);
    let (form, identity, proof) = ready.into_parts();
    assert_eq!(form.fields[0].editor.text, "PRIVATE_UID42 α\n\u{001b}");
    assert!(
        !form.preview
            && !form.preview_requested
            && !form.submit_requested
            && !form.submitted
            && !form.submitting
    );
    let FormPurpose::Pr(intent) = &form.purpose else {
        panic!("wrong purpose")
    };
    assert_eq!(intent.identity, identity);
    proof
        .validate_for(
            &provider,
            &identity,
            &f::original(),
            &window,
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(f::saved(&fixture), raw);
    f::assert_effects(&fixture, 14);
}

#[test]
fn legacy_pr_admitted_open_initial_uid_repo403_or_replacement_never_prepares_private_files() {
    for kind in 0..4 {
        let fixture = f::fixture();
        f::seed(&fixture, "{\"body\":\"PRIVATE_BEFORE_ACCESS_CANARY\"}");
        fs::set_permissions(f::db(&fixture), fs::Permissions::from_mode(0o644)).unwrap();
        let before = fs::read(f::db(&fixture)).unwrap();
        let mut steps = f::complete();
        let count = match kind {
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
                let mut raw = f::repo();
                raw["id"] = json!(420);
                steps[3] = f::get("repos/example/repo", raw);
                4
            }
            _ => {
                let mut raw = f::parent();
                raw["node_id"] = json!("PR_replacement");
                steps[2] = f::get("repos/example/repo/pulls/7", raw);
                3
            }
        };
        fixture.scripted_gh(&steps);
        let error = open(&fixture, &f::provider(&fixture), &window()).unwrap_err();
        assert!(matches!(error, LegacyAdmittedDraftFailure::Access(_)));
        assert!(!error.to_string().contains("CANARY"));
        assert_eq!(fs::metadata(f::db(&fixture)).unwrap().mode() & 0o777, 0o644);
        assert_eq!(fs::read(f::db(&fixture)).unwrap(), before);
        assert_eq!(
            f::saved(&fixture),
            "{\"body\":\"PRIVATE_BEFORE_ACCESS_CANARY\"}"
        );
        f::assert_effects(&fixture, count);
    }
}

#[test]
fn legacy_pr_admitted_open_final_access_loss_hides_private_form_and_local_corruption_error() {
    for corrupt in [false, true] {
        let fixture = f::fixture();
        let raw = if corrupt {
            "{PRIVATE_CORRUPTION_CANARY"
        } else {
            "{\"body\":\"PRIVATE_PRESENTATION_CANARY\"}"
        };
        f::seed(&fixture, raw);
        let mut steps = f::complete();
        steps[10].status = 1;
        steps[10].stdout.clear();
        steps[10].stderr =
            b"PRIVATE_ACCESS_DIAGNOSTIC gh: Resource not accessible (HTTP 403)".to_vec();
        fixture.scripted_gh(&steps);
        let error = open(&fixture, &f::provider(&fixture), &window()).unwrap_err();
        assert!(matches!(
            error,
            LegacyAdmittedDraftFailure::Access(GithubErrorKind::Permission)
        ));
        assert!(!error.to_string().contains("PRIVATE_"));
        assert!(!format!("{error:?}").contains("PRIVATE_"));
        assert_eq!(f::saved(&fixture), raw);
        f::assert_effects(&fixture, 11);
    }
}

#[test]
fn legacy_pr_admitted_open_final_uid_or_parent_head_drift_hides_saved_text() {
    for uid in [true, false] {
        let fixture = f::fixture();
        f::seed(&fixture, "{\"body\":\"PRIVATE_FINAL_DRIFT_CANARY\"}");
        let mut steps = f::complete();
        let count = if uid {
            steps[13] = f::get("user", f::user(43, "octocat"));
            14
        } else {
            let mut parent = f::parent();
            parent["head"]["sha"] = json!("c".repeat(40));
            steps[11] = f::get("repos/example/repo/pulls/7", parent);
            12
        };
        fixture.scripted_gh(&steps);
        let error = open(&fixture, &f::provider(&fixture), &window()).unwrap_err();
        assert!(matches!(error, LegacyAdmittedDraftFailure::Access(_)));
        assert!(!error.to_string().contains("CANARY"));
        assert_eq!(
            f::saved(&fixture),
            "{\"body\":\"PRIVATE_FINAL_DRIFT_CANARY\"}"
        );
        f::assert_effects(&fixture, count);
    }
}

#[test]
fn legacy_pr_admitted_open_known_repo_id_is_optional_genuine_expectation_not_cached_permission() {
    for replace in [false, true] {
        let fixture = f::fixture();
        let raw = f::confirmed_raw(
            &fixture,
            crate::forms::PrFormKind::Description,
            "known original",
        );
        f::seed(&fixture, &raw);
        fixture.scripted_gh(&f::complete());
        let mut old = f::original_repo();
        assert!(!old.permissions.pull && !old.permissions.push);
        if replace {
            old.id = 420;
        }
        let result = open_admitted_legacy_pr_draft_v1(
            f::intent(),
            Some(&old),
            &f::provider(&fixture),
            &f::db(&fixture),
            &LegacyPrDraftAccessLimits::default(),
            &window(),
            &CancellationToken::default(),
        );
        if replace {
            assert!(matches!(
                result,
                Err(LegacyAdmittedDraftFailure::Access(
                    GithubErrorKind::IdentityChanged
                ))
            ));
        } else {
            assert_eq!(result.unwrap().final_admission().repository().id, 410);
        }
        assert_eq!(f::saved(&fixture), raw);
        f::assert_effects(&fixture, if replace { 5 } else { 14 });
    }
}

#[test]
fn legacy_pr_readonly_schema_version_zero_tables_future_missing_or_wrong_columns_fail_closed() {
    for kind in 0..5 {
        let fixture = f::fixture();
        f::seed(&fixture, "{\"body\":\"PRIVATE_SCHEMA_CANARY\"}");
        let connection = Connection::open(f::db(&fixture)).unwrap();
        match kind {0=>connection.pragma_update(None,"user_version",0).unwrap(),1=>connection.pragma_update(None,"user_version",5).unwrap(),2=>connection.execute_batch("DROP TABLE drafts").unwrap(),3=>connection.execute_batch("ALTER TABLE drafts RENAME TO original_drafts; CREATE TABLE drafts(host TEXT NOT NULL,login TEXT NOT NULL,resource TEXT NOT NULL,body BLOB NOT NULL,updated_at INTEGER NOT NULL,PRIMARY KEY(host,login,resource)); INSERT INTO drafts SELECT * FROM original_drafts").unwrap(),_=>connection.execute_batch("CREATE TABLE pr_write_attempts(state TEXT,pr_node TEXT)").unwrap()};
        let version: u32 = connection
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        let tables: u64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        fixture.scripted_gh(&f::complete());
        let error = open(&fixture, &f::provider(&fixture), &window()).unwrap_err();
        assert!(matches!(
            error,
            LegacyAdmittedDraftFailure::Local(LegacyPrDraftFailure::PrivateStorage)
        ));
        assert!(!error.to_string().contains("PRIVATE_SCHEMA_CANARY"));
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            version
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM sqlite_master WHERE type='table'",
                    [],
                    |row| row.get::<_, u64>(0)
                )
                .unwrap(),
            tables
        );
        f::assert_effects(&fixture, 14);
    }
}

#[test]
fn legacy_pr_readonly_supported_versions_preserve_tagged_uid_body_schema_and_file_mode() {
    for version in 1..=4 {
        let fixture = f::fixture();
        let raw = f::confirmed_raw(
            &fixture,
            crate::forms::PrFormKind::Description,
            "PRIVATE_SUPPORTED_SCHEMA",
        );
        f::seed(&fixture, &raw);
        let connection = Connection::open(f::db(&fixture)).unwrap();
        connection
            .pragma_update(None, "user_version", version)
            .unwrap();
        let before = fs::read(f::db(&fixture)).unwrap();
        fs::set_permissions(f::db(&fixture), fs::Permissions::from_mode(0o644)).unwrap();
        fixture.scripted_gh(&f::complete());
        let (form, _, _) = open(&fixture, &f::provider(&fixture), &window())
            .unwrap()
            .into_parts();
        assert_eq!(form.fields[0].editor.text, "PRIVATE_SUPPORTED_SCHEMA");
        assert_eq!(
            connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
                .unwrap(),
            version
        );
        assert_eq!(fs::read(f::db(&fixture)).unwrap(), before);
        assert_eq!(fs::metadata(f::db(&fixture)).unwrap().mode() & 0o777, 0o600);
        assert_eq!(f::saved(&fixture), raw);
        f::assert_effects(&fixture, 14);
    }
}

#[test]
fn legacy_pr_readonly_genuinely_empty_version_zero_is_absent_without_schema_or_corpus_migration() {
    let fixture = f::fixture();
    fixture.scripted_gh(&f::complete());
    let (form, _, _) = open(&fixture, &f::provider(&fixture), &window())
        .unwrap()
        .into_parts();
    assert!(!form.restored_draft);
    assert_eq!(form.fields[0].editor.text, f::original().pr.body);
    assert!(f::db(&fixture).exists());
    assert_eq!(fs::metadata(f::db(&fixture)).unwrap().mode() & 0o777, 0o600);
    let connection = Connection::open(f::db(&fixture)).unwrap();
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |row| row.get::<_, u32>(0))
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
    f::assert_effects(&fixture, 14);
}

#[test]
fn legacy_pr_readonly_locked_database_uses_original_short_deadline_without_private_error_presentation(
) {
    let fixture = f::fixture();
    f::seed(&fixture, "{\"body\":\"PRIVATE_LOCK_CANARY\"}");
    let lock = Connection::open(f::db(&fixture)).unwrap();
    lock.execute_batch("BEGIN EXCLUSIVE").unwrap();
    fixture.scripted_gh(&f::complete());
    let window = LegacyPrDraftReadWindow::new(Duration::from_millis(300)).unwrap();
    let start = Instant::now();
    let error = open(&fixture, &f::provider(&fixture), &window).unwrap_err();
    assert!(matches!(
        error,
        LegacyAdmittedDraftFailure::Access(GithubErrorKind::Transport)
    ));
    assert!(!error.to_string().contains("PRIVATE_LOCK_CANARY"));
    assert!(start.elapsed() < Duration::from_secs(1));
    lock.execute_batch("ROLLBACK").unwrap();
    assert_eq!(f::saved(&fixture), "{\"body\":\"PRIVATE_LOCK_CANARY\"}");
    f::assert_effects(&fixture, 7);
}

#[test]
fn legacy_pr_shared_deadline_and_readonly_alias_refusal_do_not_reinitialize_or_expose_secret() {
    let fixture = f::fixture();
    let expired = Instant::now() - Duration::from_secs(1);
    assert!(matches!(
        open_legacy_pr_draft_before_v1(
            f::intent(),
            &f::provider(&fixture),
            &f::db(&fixture),
            &CancellationToken::default(),
            expired
        ),
        Err(LegacyPrDraftFailure::Deadline(_))
    ));
    assert!(!f::db(&fixture).exists());
    let target = PrWriteTarget::new(&f::identity(), &f::original()).unwrap();
    let form = crate::forms::Form::pr(f::intent()).unwrap();
    let canary = fixture.root.join("alias-private.bin");
    fs::write(&canary, b"PRIVATE_ALIAS_CANARY").unwrap();
    std::os::unix::fs::symlink(&canary, f::db(&fixture)).unwrap();
    assert!(Cache::legacy_pr_draft_snapshot_file_before_v1(
        &f::db(&fixture),
        &form.scope(),
        &form.resource(),
        &target,
        LEGACY_PR_DRAFT_SERIALIZED_BYTES,
        Instant::now() + Duration::from_secs(1)
    )
    .is_err());
    assert_eq!(fs::read(canary).unwrap(), b"PRIVATE_ALIAS_CANARY");
    f::assert_effects(&fixture, 0);
}
