//! SOURCE ONLY: declarations, not executed evidence. Register only with the
//! matching worker and exact-key Cache child during a coordinated source lease.
use super::*;
use crate::{
    editor::TextBuffer,
    forms::PrFormKind,
    pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrPublicationReference, PrWriteIntent, PrWriteJournal,
        PrWriteState,
    },
    providers::gh::PrSummary,
    test_support::{Fixture, GhStep},
};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::PathBuf, process::Command};

fn identity(id: u64, login: &str) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: login.into(),
        account_id: Some(id),
        gh_version: "fixture".into(),
    }
}
fn target() -> PrSummary {
    serde_json::from_value(json!({
        "pr":{"host":"github.invalid","id":"PR_legacy_draft_7",
            "repository":"example/repo","number":7,"title":"Original title",
            "body":"Original description","url":"https://github.invalid/example/repo/pull/7",
            "author":{"login":"author","actor_type":"User"},"state":"open",
            "draft":false,"created_at":"2026-10-13T00:00:00Z",
            "updated_at":"2026-10-13T00:01:00Z","queues":[]},
        "head_sha":"a".repeat(40),"base_sha":"b".repeat(40),
        "head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,
        "merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean",
        "additions":1,"deletions":1,"changed_files":1
    }))
    .unwrap()
}
fn intent(kind: PrFormKind) -> PrIntent {
    PrIntent {
        kind,
        target: target(),
        identity: identity(42, "octocat"),
    }
}
fn account(id: u64, login: &str) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        GhStep::read(
            &[
                "api",
                "user",
                "--hostname",
                "github.invalid",
                "--method",
                "GET",
            ],
            &json!({"id":id,"login":login}).to_string(),
        ),
    ]
}
fn both(id: u64, login: &str) -> Vec<GhStep> {
    let mut steps = account(id, login);
    steps.extend(account(id, login));
    steps
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn git(fixture: &Fixture, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(fixture.repo())
        .args(args)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}
fn fixture() -> Fixture {
    let fixture = Fixture::new();
    git(&fixture, &["init", "--initial-branch=main"]);
    fs::write(fixture.repo().join("baseline.txt"), b"committed baseline\n").unwrap();
    git(&fixture, &["add", "--", "baseline.txt"]);
    git(
        &fixture,
        &[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "baseline",
        ],
    );
    fs::write(fixture.repo().join("baseline.txt"), b"staged canary\n").unwrap();
    git(&fixture, &["add", "--", "baseline.txt"]);
    fs::write(fixture.repo().join("baseline.txt"), b"dirty canary\n").unwrap();
    fs::write(fixture.repo().join("untracked.txt"), b"untracked canary\n").unwrap();
    fs::write(
        fixture.root.join("original-index.bin"),
        fs::read(fixture.repo().join(".git/index")).unwrap(),
    )
    .unwrap();
    fs::write(
        fixture.root.join("original-head.txt"),
        git(&fixture, &["rev-parse", "HEAD"]),
    )
    .unwrap();
    fixture
}
fn assert_effects(fixture: &Fixture, count: usize) {
    let journal = fixture.root.join("gh-fixture/commands.jsonl");
    let records = if journal.exists() {
        fs::read_to_string(journal)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    assert_eq!(records.len(), count);
    for record in records {
        assert_eq!(record["matched"], true);
        assert_eq!(record["stdin_hex"], "");
        assert!(
            record["args"] == json!(["--version"])
                || record["args"]
                    == json!([
                        "api",
                        "user",
                        "--hostname",
                        "github.invalid",
                        "--method",
                        "GET"
                    ])
        );
    }
    assert_eq!(git(fixture, &["rev-list", "--count", "HEAD"]), "1\n");
    assert_eq!(
        git(fixture, &["show", "HEAD:baseline.txt"]),
        "committed baseline\n"
    );
    assert_eq!(git(fixture, &["show", ":baseline.txt"]), "staged canary\n");
    assert_eq!(
        fs::read(fixture.repo().join("baseline.txt")).unwrap(),
        b"dirty canary\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join("untracked.txt")).unwrap(),
        b"untracked canary\n"
    );
    assert_eq!(
        git(fixture, &["status", "--porcelain=v1"]),
        "MM baseline.txt\n?? untracked.txt\n"
    );
    assert_eq!(
        fs::read(fixture.repo().join(".git/index")).unwrap(),
        fs::read(fixture.root.join("original-index.bin")).unwrap()
    );
    assert_eq!(
        git(fixture, &["rev-parse", "HEAD"]),
        fs::read_to_string(fixture.root.join("original-head.txt")).unwrap()
    );
}
fn database(fixture: &Fixture) -> PathBuf {
    fixture.root.join("drafts.sqlite3")
}
fn save(fixture: &Fixture, kind: PrFormKind, raw: &str) {
    let form = Form::pr(intent(kind)).unwrap();
    Cache::open(database(fixture))
        .unwrap()
        .put_draft(&form.scope(), &form.resource(), raw)
        .unwrap();
}
fn raw_saved(fixture: &Fixture, kind: PrFormKind) -> String {
    let form = Form::pr(intent(kind)).unwrap();
    Cache::open(database(fixture))
        .unwrap()
        .draft(&form.scope(), &form.resource())
        .unwrap()
        .unwrap()
}
fn confirmed_raw(fixture: &Fixture, kind: PrFormKind, body: &str) -> String {
    let original = intent(kind);
    let target = PrWriteTarget::new(&original.identity, &original.target).unwrap();
    let write = PrWriteIntent {
        command: if kind == PrFormKind::Comment {
            "new-comment".into()
        } else {
            format!("legacy-{}", kind.key())
        },
        selected_resource: "pr".into(),
        observed_head: "c".repeat(40),
        values: BTreeMap::from([("body".into(), "original published value".into())]),
    };
    let mut journal = PrWriteJournal::open(database(fixture)).unwrap();
    let permit = journal.begin(&target, &write).unwrap();
    journal.claim(&permit, &target, &write).unwrap();
    let effect = match kind {
        PrFormKind::Comment => PrConfirmedEffect {
            kind: PrEffectKind::Comment,
            resource_id: "801".into(),
            node_id: Some("IC_801".into()),
        },
        PrFormKind::Approve | PrFormKind::RequestChanges | PrFormKind::ReviewComment => {
            PrConfirmedEffect {
                kind: PrEffectKind::Review,
                resource_id: "802".into(),
                node_id: Some("PRR_802".into()),
            }
        }
        _ => PrConfirmedEffect {
            kind: PrEffectKind::Metadata,
            resource_id: original.target.pr.id.clone(),
            node_id: Some(original.target.pr.id.clone()),
        },
    };
    journal.mark_confirmed(&permit, &effect).unwrap();
    json!({"version":2,"values":{"body":body},"publication":permit.marker().publication_reference()}).to_string()
}
fn open(fixture: &Fixture, kind: PrFormKind) -> Result<LegacyPrDraftReady, LegacyPrDraftFailure> {
    open_legacy_pr_draft_v1(
        intent(kind),
        &provider(fixture),
        &database(fixture),
        &CancellationToken::default(),
    )
}
fn assert_reset(form: &Form) {
    assert!(!form.preview && !form.preview_requested && !form.submit_requested);
    assert!(!form.submitting && !form.submitted && !form.editor_requested);
    assert!(form.metadata_preview.is_none() && form.base_selection.is_none());
    assert!(form.picker.is_none() && form.metadata_picker.is_none());
    // Optional-preview legacy comments may still be plan_ready: that is NOT
    // evidence of a restored approval or permission to publish.
}

#[test]
fn legacy_pr_open_preprincipal_uid_switch_same_login_does_not_open_private_database() {
    let fixture = fixture();
    fixture.scripted_gh(&account(43, "octocat"));
    let failure = open(&fixture, PrFormKind::Description).unwrap_err();
    assert!(matches!(
        failure,
        LegacyPrDraftFailure::Identity {
            phase: LegacyDraftPhase::BeforePrivateRead,
            problem: LegacyDraftIdentityProblem::DifferentPrincipal
        }
    ));
    assert!(!database(&fixture).exists());
    assert_effects(&fixture, 2);
}

#[test]
fn legacy_pr_open_auth_failure_precedes_private_sql_and_hides_transport_diagnostics() {
    let fixture = fixture();
    let mut steps = account(42, "octocat");
    steps[1].status = 1;
    steps[1].stdout.clear();
    steps[1].stderr = b"PRIVATE_TRANSPORT_CANARY gh: Requires authentication (HTTP 401)".to_vec();
    fixture.scripted_gh(&steps);
    let failure = open(&fixture, PrFormKind::Comment).unwrap_err();
    assert!(matches!(
        failure,
        LegacyPrDraftFailure::Identity {
            phase: LegacyDraftPhase::BeforePrivateRead,
            problem: LegacyDraftIdentityProblem::Provider(GithubErrorKind::Authentication)
        }
    ));
    assert!(!failure.to_string().contains("PRIVATE_TRANSPORT_CANARY"));
    assert!(!format!("{failure:?}").contains("PRIVATE_TRANSPORT_CANARY"));
    assert!(!database(&fixture).exists());
    assert_effects(&fixture, 2);
}

#[test]
fn legacy_pr_open_final_drift_suppresses_private_form_and_corruption_diagnostics() {
    for raw in [
        json!({"body":"PRIVATE_DRAFT_CANARY"}).to_string(),
        "{PRIVATE_CORRUPTION_CANARY".into(),
    ] {
        let fixture = fixture();
        save(&fixture, PrFormKind::Description, &raw);
        let mut steps = account(42, "octocat");
        steps.extend(account(43, "octocat"));
        fixture.scripted_gh(&steps);
        let failure = open(&fixture, PrFormKind::Description).unwrap_err();
        assert!(matches!(
            failure,
            LegacyPrDraftFailure::Identity {
                phase: LegacyDraftPhase::BeforePresentation,
                problem: LegacyDraftIdentityProblem::DifferentPrincipal
            }
        ));
        assert!(!failure.to_string().contains("CANARY"));
        assert!(!format!("{failure:?}").contains("CANARY"));
        assert_eq!(raw_saved(&fixture, PrFormKind::Description), raw);
        assert_effects(&fixture, 4);
    }
}

#[test]
fn legacy_pr_open_same_uid_login_rename_restores_stable_scope_with_actual_final_identity() {
    let fixture = fixture();
    let raw = confirmed_raw(&fixture, PrFormKind::Comment, "saved by numeric UID42");
    save(&fixture, PrFormKind::Comment, &raw);
    let mut steps = account(42, "renamed-before");
    steps.extend(account(42, "renamed-final"));
    fixture.scripted_gh(&steps);
    let ready = open(&fixture, PrFormKind::Comment).unwrap();
    assert!(!format!("{ready:?}").contains("saved by"));
    let (form, actual) = ready.into_parts();
    assert_eq!(actual.login, "renamed-final");
    assert_eq!(actual.account_id, Some(42));
    assert_eq!(
        form.scope(),
        Scope::github("github.invalid", Some(42)).unwrap()
    );
    assert_eq!(form.fields[0].editor.text, "saved by numeric UID42");
    let FormPurpose::Pr(original) = &form.purpose else {
        panic!("not legacy PR");
    };
    assert_eq!(original.identity, actual);
    assert_reset(&form);
    assert_eq!(raw_saved(&fixture, PrFormKind::Comment), raw);
    assert_effects(&fixture, 4);
}

#[test]
fn legacy_pr_open_all_seven_kinds_preserve_genuine_original_bound_drafts_without_approval() {
    for (kind, _) in PrFormKind::ALL {
        let fixture = fixture();
        let body = "Unicode α/🐙\t\n\u{001b}\u{0000} raw body";
        let raw = confirmed_raw(&fixture, kind, body);
        let reference: PrPublicationReference = serde_json::from_value(
            serde_json::from_str::<Value>(&raw).unwrap()["publication"].clone(),
        )
        .unwrap();
        save(&fixture, kind, &raw);
        fixture.scripted_gh(&both(42, "octocat"));
        let (form, _) = open(&fixture, kind).unwrap().into_parts();
        assert_eq!(form.fields.len(), LEGACY_PR_DRAFT_FIELDS);
        assert_eq!(form.fields[0].key, "body");
        assert!(!form.fields[0].secret);
        assert_eq!(form.fields[0].editor.text, body);
        assert_eq!(form.publication, Some(reference));
        assert!(form.restored_draft);
        assert_reset(&form);
        assert_eq!(raw_saved(&fixture, kind), raw);
        assert_effects(&fixture, 4);
    }
}

#[test]
fn legacy_pr_draft_budget_covers_actual_seven_constructors_full_editor_and_reference_envelope() {
    assert_eq!(PrFormKind::ALL.len(), 7);
    let cap = std::hint::black_box(LEGACY_PR_DRAFT_SERIALIZED_BYTES);
    assert_eq!(cap, 1_724_722);
    assert!(cap < 2 * 1024 * 1024);
    let body = "\u{001b}".repeat(MAX_TEXT_BYTES);
    for (kind, _) in PrFormKind::ALL {
        let mut original = intent(kind);
        original.target.pr.id = "N".repeat(4096);
        let mut form = Form::pr(original.clone()).unwrap();
        assert_eq!(form.fields.len(), LEGACY_PR_DRAFT_FIELDS);
        assert!(form
            .fields
            .iter()
            .all(|field| !field.secret && field.key == "body"));
        form.fields[0].editor = TextBuffer::new(&body).unwrap();
        let reference = PrPublicationReference {
            attempt_id: i64::MAX,
            pr_node_id: original.target.pr.id.clone(),
            observed_head: "a".repeat(64),
            payload_sha256: "b".repeat(64),
        };
        form.publication = Some(reference.clone());
        let raw = form.draft();
        assert!(raw.len() > MAX_TEXT_BYTES * 6);
        assert!(raw.len() <= LEGACY_PR_DRAFT_SERIALIZED_BYTES);
        let mut restored = Form::pr(original).unwrap();
        restored.restore_draft(&raw).unwrap();
        assert_eq!(restored.fields[0].editor.text, body);
        assert_eq!(restored.publication, Some(reference));
    }
}

#[test]
fn legacy_pr_open_reads_a_legal_full_control_editor_above_metadata_recovery_caps() {
    let fixture = fixture();
    let body = "\u{001b}".repeat(MAX_TEXT_BYTES);
    let raw = confirmed_raw(&fixture, PrFormKind::Description, &body);
    assert!(raw.len() > 128 * 1024);
    assert!(raw.len() <= LEGACY_PR_DRAFT_SERIALIZED_BYTES);
    save(&fixture, PrFormKind::Description, &raw);
    let form = Form::pr(intent(PrFormKind::Description)).unwrap();
    assert!(Cache::open(database(&fixture))
        .unwrap()
        .bounded_draft(&form.scope(), &form.resource(), 128 * 1024)
        .is_err());
    fixture.scripted_gh(&both(42, "octocat"));
    let (form, _) = open(&fixture, PrFormKind::Description)
        .unwrap()
        .into_parts();
    assert_eq!(form.fields[0].editor.text, body);
    assert_reset(&form);
    assert_eq!(raw_saved(&fixture, PrFormKind::Description), raw);
    assert_effects(&fixture, 4);
}

#[test]
fn legacy_pr_open_final_auth_or_missing_positive_uid_hides_text_and_local_errors() {
    for missing_uid in [false, true] {
        let fixture = fixture();
        save(
            &fixture,
            PrFormKind::Description,
            "{PRIVATE_LOCAL_ERROR_CANARY",
        );
        let mut steps = account(42, "octocat");
        let mut final_steps = account(42, "octocat");
        if missing_uid {
            final_steps[1].stdout = br#"{"login":"octocat"}"#.to_vec();
        } else {
            final_steps[1].status = 1;
            final_steps[1].stdout.clear();
            final_steps[1].stderr =
                b"PRIVATE_FINAL_AUTH_CANARY gh: Requires authentication (HTTP 401)".to_vec();
        }
        steps.extend(final_steps);
        fixture.scripted_gh(&steps);
        let failure = open(&fixture, PrFormKind::Description).unwrap_err();
        assert!(matches!(
            failure,
            LegacyPrDraftFailure::Identity {
                phase: LegacyDraftPhase::BeforePresentation,
                ..
            }
        ));
        assert!(!failure.to_string().contains("CANARY"));
        assert!(!format!("{failure:?}").contains("CANARY"));
        assert_eq!(
            raw_saved(&fixture, PrFormKind::Description),
            "{PRIVATE_LOCAL_ERROR_CANARY"
        );
        assert_effects(&fixture, 4);
    }
}

#[test]
fn legacy_pr_open_corrupt_or_oversized_field_never_falls_back_to_original_text() {
    for raw in [
        "{PRIVATE_CORRUPT_CANARY".into(),
        // A genuine original binding is still not approval for an oversized edit.
        "OVERSIZED_BOUND_SENTINEL".into(),
    ] {
        let fixture = fixture();
        let raw = if raw == "OVERSIZED_BOUND_SENTINEL" {
            confirmed_raw(&fixture, PrFormKind::Title, &"x".repeat(MAX_TEXT_BYTES + 1))
        } else {
            raw
        };
        save(&fixture, PrFormKind::Title, &raw);
        fixture.scripted_gh(&both(42, "octocat"));
        let failure = open(&fixture, PrFormKind::Title).unwrap_err();
        assert!(matches!(failure, LegacyPrDraftFailure::InvalidPrivateDraft));
        assert!(!failure.to_string().contains("PRIVATE_CORRUPT_CANARY"));
        assert_eq!(raw_saved(&fixture, PrFormKind::Title), raw);
        assert_effects(&fixture, 4);
    }
}

#[test]
fn legacy_pr_open_sqlite_utf8_byte_preflight_rejects_multibyte_payload_before_body_copy() {
    let fixture = fixture();
    let raw = "🐙".repeat(LEGACY_PR_DRAFT_SERIALIZED_BYTES / 4 + 1);
    save(&fixture, PrFormKind::Description, &raw);
    let connection = Connection::open(database(&fixture)).unwrap();
    let (characters, bytes): (usize, usize) = connection
        .query_row(
            "SELECT length(body),length(CAST(body AS BLOB)) FROM drafts",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(characters < LEGACY_PR_DRAFT_SERIALIZED_BYTES);
    assert!(bytes > LEGACY_PR_DRAFT_SERIALIZED_BYTES);
    drop(connection);
    fixture.scripted_gh(&both(42, "octocat"));
    let failure = open(&fixture, PrFormKind::Description).unwrap_err();
    assert!(matches!(failure, LegacyPrDraftFailure::PrivateStorage));
    assert_eq!(raw_saved(&fixture, PrFormKind::Description), raw);
    assert_effects(&fixture, 4);
}

#[test]
fn legacy_pr_open_sqlite_nontext_and_invalid_utf8_preserve_rows_and_return_static_failure() {
    for expression in ["x'ff00'", "CAST(x'ff00' AS TEXT)"] {
        let fixture = fixture();
        save(
            &fixture,
            PrFormKind::Description,
            "{\"body\":\"original private canary\"}",
        );
        let connection = Connection::open(database(&fixture)).unwrap();
        connection
            .execute(&format!("UPDATE drafts SET body={expression}"), [])
            .unwrap();
        let before: String = connection
            .query_row("SELECT hex(CAST(body AS BLOB)) FROM drafts", [], |row| {
                row.get(0)
            })
            .unwrap();
        fixture.scripted_gh(&both(42, "octocat"));
        let failure = open(&fixture, PrFormKind::Description).unwrap_err();
        assert!(matches!(failure, LegacyPrDraftFailure::PrivateStorage));
        assert!(!failure.to_string().contains("ff00"));
        let after: String = connection
            .query_row("SELECT hex(CAST(body AS BLOB)) FROM drafts", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(before, after);
        assert_effects(&fixture, 4);
    }
}

#[test]
fn legacy_pr_open_preserves_pending_uncertain_and_confirmed_original_records_without_resend() {
    for state in [
        PrWriteState::Pending,
        PrWriteState::Uncertain,
        PrWriteState::Confirmed,
    ] {
        let fixture = fixture();
        let original = intent(PrFormKind::Comment);
        let target = PrWriteTarget::new(&original.identity, &original.target).unwrap();
        let write = PrWriteIntent {
            command: "new-comment".into(),
            selected_resource: "pr".into(),
            observed_head: original.target.head_sha.clone(),
            values: BTreeMap::from([("body".into(), "original published canary".into())]),
        };
        let mut journal = PrWriteJournal::open(database(&fixture)).unwrap();
        let permit = journal.begin(&target, &write).unwrap();
        if state != PrWriteState::Pending {
            journal.claim(&permit, &target, &write).unwrap();
            if state == PrWriteState::Uncertain {
                journal.mark_uncertain(&permit).unwrap();
            } else {
                journal
                    .mark_confirmed(
                        &permit,
                        &PrConfirmedEffect {
                            kind: PrEffectKind::Comment,
                            resource_id: "801".into(),
                            node_id: Some("IC_801".into()),
                        },
                    )
                    .unwrap();
            }
        }
        let reference = permit.marker().publication_reference();
        let raw=json!({"version":2,"values":{"body":"edited after original receipt"},"publication":reference}).to_string();
        save(&fixture, PrFormKind::Comment, &raw);
        let connection = Connection::open(database(&fixture)).unwrap();
        // A malformed stored resource must not make the journal's existing
        // same-scope/opaque-node unresolved barrier disappear from OPEN.
        if state == PrWriteState::Pending {
            connection.execute("UPDATE pr_write_attempts SET repository='other/repo',pr_number=8 WHERE attempt_id=?1",[reference.attempt_id]).unwrap();
        }
        let snapshot = || {
            connection.query_row("SELECT state,payload_sha256,confirmed_json,(SELECT generation FROM pr_write_scopes LIMIT 1) FROM pr_write_attempts WHERE attempt_id=?1",[reference.attempt_id],|row|Ok((row.get::<_,String>(0)?,row.get::<_,String>(1)?,row.get::<_,Option<String>>(2)?,row.get::<_,i64>(3)?))).unwrap()
        };
        let before = snapshot();
        fixture.scripted_gh(&both(42, "octocat"));
        if state == PrWriteState::Confirmed {
            let (form, _) = open(&fixture, PrFormKind::Comment).unwrap().into_parts();
            assert_eq!(form.publication, Some(reference));
            assert_eq!(form.fields[0].editor.text, "edited after original receipt");
            assert!(form.error.is_empty());
            assert_reset(&form);
        } else {
            assert!(matches!(
                open(&fixture, PrFormKind::Comment),
                Err(LegacyPrDraftFailure::UnresolvedOriginal)
            ));
        }
        assert_eq!(snapshot(), before);
        assert_eq!(raw_saved(&fixture, PrFormKind::Comment), raw);
        assert_effects(&fixture, 4);
    }
}

#[test]
fn legacy_pr_open_invalid_original_and_precancellation_do_not_touch_sql_or_launch_gh() {
    let fixture = fixture();
    let mut original = intent(PrFormKind::Comment);
    original.identity.account_id = None;
    assert!(matches!(
        open_legacy_pr_draft_v1(
            original,
            &provider(&fixture),
            &database(&fixture),
            &CancellationToken::default()
        ),
        Err(LegacyPrDraftFailure::InvalidOriginal)
    ));
    let cancel = CancellationToken::default();
    cancel.cancel();
    assert!(matches!(
        open_legacy_pr_draft_v1(
            intent(PrFormKind::Comment),
            &provider(&fixture),
            &database(&fixture),
            &cancel
        ),
        Err(LegacyPrDraftFailure::Cancelled(
            LegacyDraftPhase::BeforePrivateRead
        ))
    ));
    assert!(!database(&fixture).exists());
    assert_effects(&fixture, 0);
}

#[test]
fn legacy_pr_exact_key_accessor_cannot_cross_scope_kind_or_budget_and_does_not_modify_drafts() {
    let fixture = fixture();
    save(
        &fixture,
        PrFormKind::Comment,
        "{\"body\":\"exact UID42 canary\"}",
    );
    let original = intent(PrFormKind::Comment);
    let form = Form::pr(original.clone()).unwrap();
    let target = PrWriteTarget::new(&original.identity, &original.target).unwrap();
    let cache = Cache::open(database(&fixture)).unwrap();
    let foreign = Scope::github("github.invalid", Some(43)).unwrap();
    cache
        .put_draft(
            &foreign,
            &form.resource(),
            "{\"body\":\"foreign UID43 canary\"}",
        )
        .unwrap();
    assert!(cache
        .legacy_pr_draft_snapshot_v1(
            &foreign,
            &form.resource(),
            &target,
            LEGACY_PR_DRAFT_SERIALIZED_BYTES
        )
        .is_err());
    assert!(cache
        .legacy_pr_draft_snapshot_v1(
            &form.scope(),
            "example/repo/pull/7/composer/create",
            &target,
            LEGACY_PR_DRAFT_SERIALIZED_BYTES
        )
        .is_err());
    for cap in [0, 2 * 1024 * 1024 + 1] {
        assert!(cache
            .legacy_pr_draft_snapshot_v1(&form.scope(), &form.resource(), &target, cap)
            .is_err());
    }
    let own = cache
        .legacy_pr_draft_snapshot_v1(
            &form.scope(),
            &form.resource(),
            &target,
            LEGACY_PR_DRAFT_SERIALIZED_BYTES,
        )
        .unwrap();
    assert_eq!(
        own.body.as_deref(),
        Some("{\"body\":\"exact UID42 canary\"}")
    );
    assert!(!own.unresolved);
    assert_eq!(
        cache.draft(&foreign, &form.resource()).unwrap().as_deref(),
        Some("{\"body\":\"foreign UID43 canary\"}")
    );
    let connection = Connection::open(database(&fixture)).unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM drafts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        2
    );
    assert!(!connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name='pr_write_attempts')",
            params![],
            |row| row.get::<_, bool>(0)
        )
        .unwrap());
    assert_effects(&fixture, 0);
}
