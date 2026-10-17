//! SOURCE ONLY / UNRUN. Real SQLite and actual serde/fingerprint paths.
//! These metadata fixtures are NOT provider/source/auth/write capabilities.
use super::*;
use serde_json::json;
use std::{fs, os::unix::fs::PermissionsExt};

fn directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}
fn provider() -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program("/ghtui-fixture/never-a-transport")
}
fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "actual-name-is-not-identity".into(),
        account_id: Some(42),
        gh_version: "fixture-only".into(),
    }
}
fn context() -> ActionsImmutableContext {
    ActionsImmutableContext {
        run_id: Some(123),
        attempt: Some(2),
        head_sha: Some("a".repeat(40)),
        ..Default::default()
    }
}
fn scoped_context(id: u64, original_name: &str) -> ActionsImmutableContext {
    ActionsImmutableContext {
        original_repository_v3: Some(ActionsOriginalRepositoryContextV3 {
            repository_id: id,
            repository_node: format!("REPO_{id}"),
            original_name: original_name.into(),
        }),
        original_run_v3: Some(ActionsOriginalRunContextV3 {
            run_node: "RUN_123".into(),
            workflow_id: 7,
            check_suite_id: Some(777),
        }),
        ..context()
    }
}
fn target(repository: &str, context: &ActionsImmutableContext) -> ActionsWriteTarget {
    ActionsWriteTarget::from_original_context_v3(
        &Scope::github("github.invalid", Some(42)).unwrap(),
        &ActionsTarget::Run {
            repository: repository.into(),
            id: 123,
        },
        context,
    )
    .unwrap()
}
fn intent(context: ActionsImmutableContext) -> ActionsWriteIntent {
    ActionsWriteIntent::new(
        ActionsWriteKind::Cancel,
        context,
        Default::default(),
        b"memory only, never persisted",
    )
    .unwrap()
}
fn sql_state(journal: &ActionsWriteJournal) -> (i64, i64, i64, i64) {
    journal.connection.query_row("SELECT (SELECT COUNT(*) FROM actions_write_attempts),(SELECT COUNT(*) FROM actions_write_resources),(SELECT COUNT(*) FROM actions_write_generations),COALESCE((SELECT SUM(generation) FROM actions_write_generations),0)",[],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap()
}
fn duplicate_rows(journal: &ActionsWriteJournal, original: i64, total: usize) {
    let tx = journal.connection.unchecked_transaction().unwrap();
    for _ in 1..total {
        tx.execute("INSERT INTO actions_write_attempts(host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,created_at) SELECT host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,created_at FROM actions_write_attempts WHERE attempt_id=?1",[original]).unwrap();
        let id = tx.last_insert_rowid();
        tx.execute("INSERT INTO actions_write_resources(host,principal,resource_key,attempt_id) SELECT host,principal,resource_key,?1 FROM actions_write_resources WHERE attempt_id=?2",params![id,original]).unwrap();
    }
    tx.commit().unwrap();
}

#[derive(Serialize)]
struct LegacyContextV1 {
    run_id: Option<u64>,
    attempt: Option<u64>,
    head_sha: Option<String>,
    workflow_commit: Option<String>,
    job_id: Option<u64>,
}
#[test]
fn scoped_actions_publication_v3_legacy_none_actual_serializer_and_fixed_fingerprints_are_exact() {
    for value in [
        Default::default(),
        context(),
        ActionsImmutableContext {
            workflow_commit: Some("b".repeat(40)),
            ..Default::default()
        },
    ] {
        let old = LegacyContextV1 {
            run_id: value.run_id,
            attempt: value.attempt,
            head_sha: value.head_sha.clone(),
            workflow_commit: value.workflow_commit.clone(),
            job_id: value.job_id,
        };
        assert_eq!(
            serde_json::to_vec(&value).unwrap(),
            serde_json::to_vec(&old).unwrap()
        );
        let restored: ActionsImmutableContext =
            serde_json::from_slice(&serde_json::to_vec(&old).unwrap()).unwrap();
        assert_eq!(restored, value);
        assert!(restored.original_repository_v3.is_none());
        assert!(restored.original_run_v3.is_none());
        assert!(restored.original_pr_v3.is_none());
    }
    assert_eq!(
        serde_json::to_string(&ActionsImmutableContext::default()).unwrap(),
        r#"{"run_id":null,"attempt":null,"head_sha":null,"workflow_commit":null,"job_id":null}"#
    );
    let value = context();
    assert_eq!(
        serde_json::to_string(&value).unwrap(),
        r#"{"run_id":123,"attempt":2,"head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","workflow_commit":null,"job_id":null}"#
    );
    assert_eq!(
        intent(value.clone())
            .fingerprint(&target("example/repo", &value))
            .unwrap(),
        "98e427155b4d7e223e433f9150e6bd208ed0a706350536f2e548aab5bde35132"
    );
    let dispatch = ActionsImmutableContext {
        workflow_commit: Some("b".repeat(40)),
        ..Default::default()
    };
    let dispatch_target = ActionsWriteTarget::from_scope(
        &Scope::github("github.invalid", Some(42)).unwrap(),
        &ActionsTarget::Workflow {
            repository: "example/repo".into(),
            id: 7,
        },
    )
    .unwrap();
    assert_eq!(
        ActionsWriteIntent::new(
            ActionsWriteKind::DispatchWorkflow,
            dispatch,
            Default::default(),
            b""
        )
        .unwrap()
        .fingerprint(&dispatch_target)
        .unwrap(),
        "bae33f1a17ec02745ec73de1778df5eb47ccb5969fc1015c2080336539f869a6"
    );
    assert!(intent(Default::default())
        .fingerprint(&target("example/repo", &Default::default()))
        .is_err());
}

#[test]
fn scoped_actions_publication_v3_actual_old_sqlite_literal_survives_reopen_without_rewrite() {
    let dir = directory();
    let path = dir.path().join("cache.sqlite3");
    let journal = ActionsWriteJournal::open(&path).unwrap();
    let context = r#"{"run_id":123,"attempt":2,"head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","workflow_commit":null,"job_id":null}"#;
    let target = r#"{"Run":{"repository":"example/repo","id":123}}"#;
    let resources = r#"["github.invalid/example/repo/run/123"]"#;
    let options = r#"{"debug_logging":false,"enabled":null,"selected_ids":[]}"#;
    let fingerprint = "98e427155b4d7e223e433f9150e6bd208ed0a706350536f2e548aab5bde35132";
    journal.connection.execute("INSERT INTO actions_write_attempts(host,principal,target_json,resources_json,kind_json,context_json,options_json,fingerprint_sha256,state,created_at) VALUES('github.invalid','user-id:42',?1,?2,'\"cancel\"',?3,?4,?5,'uncertain',1)",params![target,resources,context,options,fingerprint]).unwrap();
    let id = journal.connection.last_insert_rowid();
    journal.connection.execute("INSERT INTO actions_write_resources VALUES('github.invalid','user-id:42','github.invalid/example/repo/run/123',?1)",[id]).unwrap();
    drop(journal);
    let journal = ActionsWriteJournal::open(&path).unwrap();
    let marker = journal
        .original_publication(
            &provider(),
            &identity(),
            &ActionsPublicationReference {
                attempt_id: id,
                fingerprint_sha256: fingerprint.into(),
            },
        )
        .unwrap();
    marker.validate_metadata().unwrap();
    assert!(marker.context.original_repository_v3.is_none());
    assert_eq!(marker.state, ActionsWriteState::Uncertain);
    let saved:(String,String,String,String)=journal.connection.query_row("SELECT target_json,resources_json,context_json,options_json FROM actions_write_attempts WHERE attempt_id=?1",[id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?))).unwrap();
    assert_eq!(
        saved,
        (
            target.into(),
            resources.into(),
            context.into(),
            options.into()
        )
    );
    assert_eq!(sql_state(&journal), (1, 1, 0, 0));
}

#[test]
fn scoped_actions_publication_v3_original_current_numeric_aliases_roundtrip_actual_sqlite() {
    let dir = directory();
    let path = dir.path().join("cache.sqlite3");
    let ctx = scoped_context(10000, "Example/OLD");
    let wanted = target("Example/NEW", &ctx);
    assert_eq!(
        wanted
            .resource_keys()
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        vec![
            "github.invalid/example/new/run/123",
            "github.invalid/example/old/run/123",
            "github.invalid/repository-id/10000/actions/run/123"
        ]
    );
    let mut journal = ActionsWriteJournal::open(&path).unwrap();
    let permit = journal.begin(&wanted, &intent(ctx.clone())).unwrap();
    permit.marker().validate_metadata().unwrap();
    let reference = permit.marker().publication_reference();
    drop(journal);
    let journal = ActionsWriteJournal::open(&path).unwrap();
    let marker = journal
        .original_publication(&provider(), &identity(), &reference)
        .unwrap();
    assert_eq!(marker.context, ctx);
    assert_eq!(marker.target.resource_keys(), wanted.resource_keys());
    marker.validate_metadata().unwrap();
    assert_eq!(
        journal
            .blocking(&wanted, ActionsWriteKind::ForceCancel)
            .unwrap()
            .len(),
        1
    );
    // Metadata reconstruction still cannot use the legacy name-only inspector.
    assert!(journal
        .prepare_inspection(
            &provider(),
            &identity(),
            reference.attempt_id,
            &Default::default()
        )
        .unwrap_err()
        .to_string()
        .contains("genuine current numeric source"));
}

#[test]
fn scoped_actions_publication_v3_alias_omission_injection_or_name_change_is_invalid_metadata() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let ctx = scoped_context(10000, "example/old");
    let wanted = target("example/new", &ctx);
    let permit = journal.begin(&wanted, &intent(ctx)).unwrap();
    for mutation in 0..3 {
        let mut marker = permit.marker().clone();
        match mutation {
            0 => {
                marker.target.resources.pop();
            }
            1 => marker
                .target
                .resources
                .push("github.invalid/forged/run/123".into()),
            _ => {
                marker
                    .context
                    .original_repository_v3
                    .as_mut()
                    .unwrap()
                    .original_name = "example/invented".into()
            }
        }
        assert!(marker.validate_metadata().is_err());
    }
    let mut raw = serde_json::to_value(&permit.marker().context).unwrap();
    raw["original_repository_v3"]
        .as_object_mut()
        .unwrap()
        .remove("original_name");
    assert!(serde_json::from_value::<ActionsImmutableContext>(raw).is_err());
    let legacy = ActionsWriteTarget::from_scope(wanted.scope(), wanted.target()).unwrap();
    assert!(intent(permit.marker().context.clone())
        .fingerprint(&legacy)
        .is_err());
}

#[test]
fn scoped_actions_publication_v3_numeric_context_rejects_incomplete_or_fabricated_facts() {
    let good = scoped_context(10000, "example/repo");
    for kind in 0..8 {
        let mut value = good.clone();
        match kind {
            0 => value.original_repository_v3.as_mut().unwrap().repository_id = 0,
            1 => {
                value
                    .original_repository_v3
                    .as_mut()
                    .unwrap()
                    .repository_node = "x".repeat(257)
            }
            2 => value.original_repository_v3.as_mut().unwrap().original_name = "../bad".into(),
            3 => value.original_run_v3.as_mut().unwrap().run_node = "line\nbreak".into(),
            4 => value.original_run_v3.as_mut().unwrap().workflow_id = 0,
            5 => value.original_run_v3.as_mut().unwrap().check_suite_id = Some(0),
            6 => value.original_repository_v3 = None,
            _ => {
                value.original_pr_v3 = Some(ActionsOriginalPrContextV3 {
                    id: 123,
                    node: "PR_123".into(),
                    number: 4,
                    head_sha: "not-a-sha".into(),
                    base_sha: "b".repeat(40),
                })
            }
        }
        assert!(
            ActionsWriteIntent::new(ActionsWriteKind::Cancel, value, Default::default(), b"")
                .is_err()
        );
    }
    let mut unknown = good;
    unknown.original_run_v3 = None;
    assert!(ActionsWriteTarget::from_original_context_v3(
        &Scope::github("github.invalid", Some(42)).unwrap(),
        &ActionsTarget::Run {
            repository: "example/repo".into(),
            id: 123
        },
        &unknown
    )
    .is_err());
}

#[test]
fn scoped_actions_publication_v3_unknown_legacy_blocks_before_attempt_or_generation_writes() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let old = context();
    journal
        .begin(&target("unrelated/unknown-old-name", &old), &intent(old))
        .unwrap();
    let ctx = scoped_context(10000, "example/old");
    let wanted = target("example/new", &ctx);
    let before = sql_state(&journal);
    assert!(journal
        .begin(&wanted, &intent(ctx))
        .unwrap_err()
        .to_string()
        .contains("UNKNOWN legacy"));
    assert_eq!(sql_state(&journal), before);
}

#[test]
fn scoped_actions_publication_v3_known_scoped_unrelated_provenance_does_not_globally_block() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let old = scoped_context(20000, "other/original");
    journal
        .begin(&target("other/current", &old), &intent(old))
        .unwrap();
    let ctx = scoped_context(10000, "example/old");
    let wanted = target("example/new", &ctx);
    let before = sql_state(&journal);
    journal.begin(&wanted, &intent(ctx)).unwrap();
    let after = sql_state(&journal);
    assert_eq!(after.0, before.0 + 1);
    assert_eq!(after.1, before.1 + 3);
}

#[test]
fn scoped_actions_publication_v3_same_numeric_repository_survives_new_names_and_blocks() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let old = scoped_context(10000, "old/name");
    journal
        .begin(&target("changed/name", &old), &intent(old))
        .unwrap();
    let current = scoped_context(10000, "new/original");
    let wanted = target("new/current", &current);
    let before = sql_state(&journal);
    assert!(journal.begin(&wanted, &intent(current)).is_err());
    assert_eq!(sql_state(&journal), before);
    // A different numeric ID which reuses an old confirmed name is still
    // conservatively protected by the retained NAME alias, never discarded.
    let reused = scoped_context(90000, "old/name");
    let wanted = target("replacement/current", &reused);
    assert!(journal.begin(&wanted, &intent(reused)).is_err());
    assert_eq!(sql_state(&journal), before);
}

#[test]
fn scoped_actions_publication_v3_scope_unknown_preflight_is_exactly_256_not_partial_absence() {
    for count in [256, 257] {
        let dir = directory();
        let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
        let old = scoped_context(20000, "other/repo");
        let permit = journal
            .begin(&target("other/repo", &old), &intent(old))
            .unwrap();
        duplicate_rows(&journal, permit.marker().attempt_id, count);
        let ctx = scoped_context(10000, "example/repo");
        let wanted = target("example/repo", &ctx);
        let before = sql_state(&journal);
        let result = journal.begin(&wanted, &intent(ctx));
        if count == 256 {
            assert!(result.is_ok());
            assert_eq!(sql_state(&journal).0, 257);
        } else {
            assert!(result.unwrap_err().to_string().contains("256"));
            assert_eq!(sql_state(&journal), before);
        }
    }
}

#[test]
fn scoped_actions_publication_v3_unknown_scope_metadata_preflight_checks_actual_sql_byte_lengths() {
    for (count, per_row) in [(64, 128 * 1024), (65, 128 * 1024), (1, 128 * 1024 + 1)] {
        let dir = directory();
        let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
        let old = scoped_context(20000, "other/repo");
        let permit = journal
            .begin(&target("other/repo", &old), &intent(old.clone()))
            .unwrap();
        let id = permit.marker().attempt_id;
        // Hostile extra JSON is NOT current authority. Actual SQLite BLOB byte
        // lengths must refuse it before parsing/allocating any candidate body.
        let header = serde_json::to_string(&old).unwrap();
        let suffix = ",\"unrecognized_fixture_padding\":\"\"}";
        let base=journal.connection.query_row("SELECT length(CAST(target_json AS BLOB))+length(CAST(resources_json AS BLOB))+length(CAST(kind_json AS BLOB))+length(CAST(options_json AS BLOB))+length(CAST(fingerprint_sha256 AS BLOB)) FROM actions_write_attempts WHERE attempt_id=?1",[id],|row|row.get::<_,usize>(0)).unwrap();
        let prefix = header.strip_suffix('}').expect("one actual outer object");
        let n = per_row - base - prefix.len() - suffix.len();
        let padded = format!(
            "{prefix},\"unrecognized_fixture_padding\":\"{}\"}}",
            "x".repeat(n)
        );
        assert_eq!(base + padded.len(), per_row);
        journal
            .connection
            .execute(
                "UPDATE actions_write_attempts SET context_json=?1 WHERE attempt_id=?2",
                params![padded, id],
            )
            .unwrap();
        duplicate_rows(&journal, id, count);
        let before = sql_state(&journal);
        let ctx = scoped_context(10000, "example/repo");
        let result = journal.begin(&target("example/repo", &ctx), &intent(ctx));
        if count == 64 {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
            assert_eq!(sql_state(&journal), before);
        }
    }
}

#[test]
fn scoped_actions_publication_v3_scoped_preflight_does_not_change_legacy_none_begin_policy() {
    let dir = directory();
    let mut journal = ActionsWriteJournal::open(&dir.path().join("cache.sqlite3")).unwrap();
    let old = context();
    journal
        .begin(&target("old/unknown", &old), &intent(old))
        .unwrap();
    let value = context();
    let permit = journal
        .begin(&target("other/legacy", &value), &intent(value))
        .unwrap();
    assert!(permit.marker().context.original_repository_v3.is_none());
    assert_eq!(sql_state(&journal).0, 2);
    assert_eq!(
        serde_json::to_value(&permit.marker().context).unwrap(),
        json!({"run_id":123,"attempt":2,"head_sha":"a".repeat(40),"workflow_commit":null,"job_id":null})
    );
}
