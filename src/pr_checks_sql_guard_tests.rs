//! SOURCE ONLY: meaningful real-SQLite contracts. Undeclared/uncompiled/unrun.
use super::*;
use crate::{
    actions_workspace::{ActionsCommand, ActionsView},
    actions_write_journal::{
        ActionsImmutableContext, ActionsSafeOptions, ActionsWriteIntent, ActionsWriteKind,
        ActionsWriteTarget,
    },
    pr_write_journal::{PrConfirmedEffect, PrEffectKind, PrWriteIntent},
    providers::gh::{ActionCache, ActionsTarget},
    storage::{Cache, Record},
};
use serde_json::json;
use std::{collections::BTreeMap, fs, os::unix::fs::PermissionsExt};

fn directory() -> tempfile::TempDir {
    tempfile::Builder::new()
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()
        .unwrap()
}

fn owned_run_v4(
    path: &Path,
    node: &str,
    uid: u64,
) -> (ActionsWriteJournal, ActionsWriteTarget, ActionsWriteIntent) {
    use crate::actions_write_journal::{
        ActionsOriginalPrContextV3, ActionsOriginalRepositoryContextV3, ActionsOriginalRunContextV3,
    };
    let context = ActionsImmutableContext {
        run_id: Some(123),
        attempt: Some(2),
        head_sha: Some("a".repeat(40)),
        original_repository_v3: Some(ActionsOriginalRepositoryContextV3 {
            repository_id: 10000,
            repository_node: "R_10000".into(),
            original_name: "example/repo".into(),
        }),
        original_run_v3: Some(ActionsOriginalRunContextV3 {
            run_node: "RUN_123".into(),
            workflow_id: 7,
            check_suite_id: Some(900),
        }),
        original_pr_v3: Some(ActionsOriginalPrContextV3 {
            id: 1007,
            node: node.into(),
            number: 7,
            head_sha: "a".repeat(40),
            base_sha: "b".repeat(40),
        }),
        ..Default::default()
    };
    let target = ActionsWriteTarget::from_original_context_v3(
        &Scope::github("github.invalid", Some(uid)).unwrap(),
        &ActionsTarget::Run {
            repository: "example/repo".into(),
            id: 123,
        },
        &context,
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        ActionsWriteKind::Cancel,
        context,
        Default::default(),
        b"exact original run binding",
    )
    .unwrap();
    (ActionsWriteJournal::open(path).unwrap(), target, intent)
}

#[test]
fn scoped_actions_rerun_v4_pr_checks_sql_guard_accepts_only_actual_own_claim_and_typed_result() {
    use crate::actions_write_journal::{ActionsReceipt, ActionsReceiptKind};
    let temp = directory();
    let path = temp.path().join("cache.sqlite3");
    seed_cache(&path);
    let guard = PrChecksSqlGuard::capture(&path, &identity(42), &summary()).unwrap();
    let (mut journal, target, intent) = owned_run_v4(&path, "PR_checks_7", 42);
    let permit = journal.begin(&target, &intent).unwrap();
    assert!(journal.capture_claim_stamp_v3(&permit).is_err());
    journal.claim(&permit, &target, &intent).unwrap();
    let claim = journal.capture_claim_stamp_v3(&permit).unwrap();
    assert!(
        guard.validate(&path).is_err(),
        "Ordinary stale Checks presentation remains fenced"
    );
    guard
        .validate_owned_publication_v4(&path, &journal, &claim, None)
        .unwrap();
    let receipt = ActionsReceipt {
        kind: ActionsReceiptKind::RunTransition,
        resource_id: Some(123),
        observed_attempt: Some(2),
        observed_run_id: None,
        original_dispatch_run_v7: None,
        workflow_remote_v9: None,
    };
    journal.mark_confirmed(&permit, &receipt).unwrap();
    assert!(guard
        .validate_owned_publication_v4(&path, &journal, &claim, None)
        .is_err());
    guard
        .validate_owned_publication_v4(
            &path,
            &journal,
            &claim,
            Some((ActionsWriteState::Confirmed, &receipt)),
        )
        .unwrap();
    let mut wrong = receipt;
    wrong.resource_id = Some(999);
    assert!(guard
        .validate_owned_publication_v4(
            &path,
            &journal,
            &claim,
            Some((ActionsWriteState::Confirmed, &wrong))
        )
        .is_err());
    assert!(
        guard.validate(&path).is_err(),
        "No replacement guard or released protection minted"
    );
}

#[test]
fn scoped_actions_rerun_v4_pr_checks_sql_guard_own_claim_never_waives_cache_pr_or_alias_drift() {
    for sql in [
        "UPDATE checkpoints SET generation=generation+1",
        "UPDATE records SET fetched_at=fetched_at+1",
        "UPDATE actions_write_generations SET generation=generation+1 WHERE resource_key LIKE '%repository-id/%'",
        "UPDATE actions_write_attempts SET claimed=0",
    ] {
        let temp = directory(); let path = temp.path().join("cache.sqlite3"); seed_cache(&path);
        let guard = PrChecksSqlGuard::capture(&path, &identity(42), &summary()).unwrap();
        let (mut journal, target, intent) = owned_run_v4(&path, "PR_checks_7", 42);
        let permit = journal.begin(&target, &intent).unwrap(); journal.claim(&permit, &target, &intent).unwrap();
        let claim = journal.capture_claim_stamp_v3(&permit).unwrap();
        guard.validate_owned_publication_v4(&path, &journal, &claim, None).unwrap();
        let writer = Connection::open(&path).unwrap(); assert_eq!(writer.execute(sql, []).unwrap(), 1, "{sql}");
        assert!(guard.validate_owned_publication_v4(&path, &journal, &claim, None).is_err(), "{sql}");
        let state: (String, i64) = writer.query_row("SELECT state,COUNT(*) FROM actions_write_attempts", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(state, ("pending".into(), 1), "No repair/ACK/resend");
    }
    let temp = directory();
    let path = temp.path().join("cache.sqlite3");
    seed_cache(&path);
    let guard = PrChecksSqlGuard::capture(&path, &identity(42), &summary()).unwrap();
    let (mut actions, target, intent) = owned_run_v4(&path, "PR_checks_7", 42);
    let permit = actions.begin(&target, &intent).unwrap();
    actions.claim(&permit, &target, &intent).unwrap();
    let claim = actions.capture_claim_stamp_v3(&permit).unwrap();
    let mut pr = PrWriteJournal::open(&path).unwrap();
    let target = PrWriteTarget::new(&identity(42), &summary()).unwrap();
    let intent = PrWriteIntent {
        command: "comment".into(),
        selected_resource: "conversation".into(),
        observed_head: summary().head_sha,
        values: BTreeMap::from([("body".into(), "new protected comment".into())]),
    };
    let _protected = pr.begin(&target, &intent).unwrap();
    assert!(guard
        .validate_owned_publication_v4(&path, &actions, &claim, None)
        .is_err());
}

#[test]
fn scoped_actions_rerun_v4_pr_checks_sql_guard_refuses_other_pr_uid_or_database_claim() {
    for (node, uid, other_db) in [
        ("PR_other_7", 42, false),
        ("PR_checks_7", 43, false),
        ("PR_checks_7", 42, true),
    ] {
        let temp = directory();
        let path = temp.path().join("cache.sqlite3");
        seed_cache(&path);
        let guard = PrChecksSqlGuard::capture(&path, &identity(42), &summary()).unwrap();
        let claimed_path = if other_db {
            temp.path().join("other.sqlite3")
        } else {
            path.clone()
        };
        let (mut journal, target, intent) = owned_run_v4(&claimed_path, node, uid);
        let permit = journal.begin(&target, &intent).unwrap();
        journal.claim(&permit, &target, &intent).unwrap();
        let claim = journal.capture_claim_stamp_v3(&permit).unwrap();
        journal.validate_claim_stamp_v3(&claim).unwrap();
        assert!(guard
            .validate_owned_publication_v4(&path, &journal, &claim, None)
            .is_err());
    }
}
fn identity(id: u64) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "same-login".into(),
        account_id: Some(id),
        gh_version: "fixture".into(),
    }
}
fn summary() -> PrSummary {
    serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_checks_7","repository":"example/repo","number":7,
      "title":"Original","body":"PRIVATE_BODY_NOT_A_FENCE","url":"https://github.invalid/example/repo/pull/7",
      "author":{"login":"author","actor_type":"User"},"state":"open","draft":false,
      "created_at":"2026-10-01T00:00:00Z","updated_at":"2026-10-13T00:00:00Z","queues":[]},
      "head_sha":"a".repeat(40),"base_sha":"b".repeat(40),"head_ref":"feature","base_ref":"main",
      "merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":null,"mergeable_state":"unknown",
      "additions":0,"deletions":0,"changed_files":0})).unwrap()
}
fn action_cache(id: u64) -> ActionCache {
    ActionCache {
        id,
        key: format!("cache-{id}"),
        ref_name: "refs/heads/main".into(),
        version: "fixture".into(),
        size_in_bytes: 42,
        created_at: "2026-10-01T00:00:00Z".into(),
        last_accessed_at: "2026-10-13T00:00:00Z".into(),
    }
}
fn workspace(id: u64) -> ActionsWorkspace {
    let mut workspace = ActionsWorkspace::new(identity(id), vec!["example/repo".into()]);
    workspace.view = ActionsView::Caches;
    workspace.caches = vec![action_cache(71), action_cache(72)];
    workspace.prepare_rows();
    workspace
}
fn pending_cache(database: &Path, uid: u64, id: u64) -> i64 {
    let mut journal = ActionsWriteJournal::open(database).unwrap();
    let target = ActionsWriteTarget::from_scope(
        &Scope::github("github.invalid", Some(uid)).unwrap(),
        &ActionsTarget::Cache {
            repository: "example/repo".into(),
            id,
        },
    )
    .unwrap();
    let intent = ActionsWriteIntent::new(
        ActionsWriteKind::DeleteCache,
        ActionsImmutableContext::default(),
        ActionsSafeOptions::default(),
        b"",
    )
    .unwrap();
    journal.begin(&target, &intent).unwrap().marker().attempt_id
}
fn reason(workspace: &ActionsWorkspace) -> Option<&str> {
    workspace
        .controls
        .iter()
        .find(|control| control.action == ActionsAction::Compose(ActionsCommand::DeleteCache))
        .unwrap()
        .disabled_reason
        .as_deref()
}
fn seed_cache(database: &Path) {
    let mut cache = Cache::open(database).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let generation = cache
        .reserve_sync_generation(&scope, "checks-fixture")
        .unwrap();
    assert!(cache
        .commit_reserved_sync(
            &scope,
            "checks-fixture",
            generation,
            &[Record::new(
                "private-fixture",
                "PR_checks_7",
                "example/repo",
                json!({"body":"PRIVATE_CACHE_BODY_CANARY"})
            )],
            true
        )
        .unwrap());
    cache
        .put_draft(&scope, "pr/7", "PRIVATE_DRAFT_CANARY")
        .unwrap();
}

#[test]
fn pr_checks_sql_guard_pure_decoration_retains_pending_original_without_sql_or_release() {
    let directory = directory();
    let database = directory.path().join("cache.sqlite3");
    seed_cache(&database);
    let original = pending_cache(&database, 42, 71);
    let guard = PrChecksSqlGuard::capture(&database, &identity(42), &summary()).unwrap();
    guard.validate(&database).unwrap();
    // Removing the exact pathname proves decorate does not reopen SQLite or FS.
    let retained = directory.path().join("retained.sqlite3");
    fs::rename(&database, &retained).unwrap();
    let mut current = workspace(42);
    guard.decorate(&mut current).unwrap();
    assert_eq!(current.publication_total, Some(1));
    assert!(current.publication_complete);
    assert_eq!(current.publication_rows[0].attempt_id, original);
    assert!(current.publication_rows[0]
        .details
        .iter()
        .any(|line| line.contains("effect not confirmed")));
    assert!(reason(&current).unwrap().contains("Original attempt #"));
    current.select(1);
    assert!(reason(&current).is_none());
    current.select(0);
    assert!(reason(&current).is_some());
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let rows = ActionsWriteJournal::open(&retained)
        .unwrap()
        .list_unresolved(&scope, 256)
        .unwrap();
    assert_eq!(rows.total, 1);
    assert_eq!(rows.markers[0].state, ActionsWriteState::Pending);
    assert!(rows.markers[0].receipt.is_none());
    assert_eq!(
        Cache::open(&retained)
            .unwrap()
            .draft(&scope, "pr/7")
            .unwrap()
            .as_deref(),
        Some("PRIVATE_DRAFT_CANARY")
    );
    assert!(!format!("{guard:?}").contains("PRIVATE_"));
    assert!(!format!("{guard:?}").contains("cache.sqlite3"));
    assert!(guard.validate(&database).is_err());
}

#[test]
fn pr_checks_sql_guard_real_clear_and_revoke_reject_old_cache_without_erasing_originals() {
    for revoke in [false, true] {
        let directory = directory();
        let database = directory.path().join("cache.sqlite3");
        seed_cache(&database);
        let attempt = pending_cache(&database, 42, 71);
        let guard = PrChecksSqlGuard::capture(&database, &identity(42), &summary()).unwrap();
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let mut cache = Cache::open(&database).unwrap();
        if revoke {
            assert_eq!(cache.revoke_repository(&scope, "example/repo").unwrap(), 1);
        } else {
            cache.clear_scope(&scope, false).unwrap();
        }
        assert!(guard
            .validate(&database)
            .unwrap_err()
            .contains("fences changed"));
        assert_eq!(
            cache.draft(&scope, "pr/7").unwrap().as_deref(),
            Some("PRIVATE_DRAFT_CANARY")
        );
        let originals = ActionsWriteJournal::open(&database)
            .unwrap()
            .list_unresolved(&scope, 256)
            .unwrap();
        assert_eq!(originals.total, 1);
        assert_eq!(originals.markers[0].attempt_id, attempt);
        assert_eq!(originals.markers[0].state, ActionsWriteState::Pending);
    }
}

#[test]
fn pr_checks_sql_guard_actual_pr_receipt_advances_original_epoch_and_pending_never_becomes_effect()
{
    let directory = directory();
    let database = directory.path().join("cache.sqlite3");
    let guard = PrChecksSqlGuard::capture(&database, &identity(42), &summary()).unwrap();
    let mut journal = PrWriteJournal::open(&database).unwrap();
    let target = PrWriteTarget::new(&identity(42), &summary()).unwrap();
    let intent = PrWriteIntent {
        command: "legacy-description".into(),
        selected_resource: "pr".into(),
        observed_head: "a".repeat(40),
        values: BTreeMap::from([("body".into(), "not read by checks fence".into())]),
    };
    let permit = journal.begin(&target, &intent).unwrap();
    assert!(guard.validate(&database).is_err());
    assert!(
        PrChecksSqlGuard::capture(&database, &identity(42), &summary())
            .unwrap_err()
            .contains("neither invocation nor success")
    );
    journal.claim(&permit, &target, &intent).unwrap();
    let effect = PrConfirmedEffect {
        kind: PrEffectKind::Metadata,
        resource_id: "7".into(),
        node_id: Some("PR_checks_7".into()),
    };
    journal.mark_confirmed(&permit, &effect).unwrap();
    assert!(guard.validate(&database).is_err());
    let receipt = permit.marker().publication_reference();
    assert_eq!(
        journal
            .confirmed_publication(&target, &receipt)
            .unwrap()
            .unwrap()
            .confirmed,
        Some(effect)
    );
    let fresh = PrChecksSqlGuard::capture(&database, &identity(42), &summary()).unwrap();
    fresh.validate(&database).unwrap();
    assert!(journal
        .confirmed_publication(&target, &receipt)
        .unwrap()
        .is_some());
}

#[test]
fn pr_checks_sql_guard_new_actions_marker_and_other_account_are_distinct_local_epochs() {
    let directory = directory();
    let database = directory.path().join("cache.sqlite3");
    let guard = PrChecksSqlGuard::capture(&database, &identity(42), &summary()).unwrap();
    pending_cache(&database, 43, 71);
    guard.validate(&database).unwrap();
    pending_cache(&database, 42, 72);
    assert!(guard.validate(&database).is_err());
    let current = PrChecksSqlGuard::capture(&database, &identity(42), &summary()).unwrap();
    let mut foreign = workspace(43);
    assert!(current.decorate(&mut foreign).is_err());
    assert!(foreign.publication_rows.is_empty());
    let mut renamed = workspace(42);
    renamed.identity.login = "actual-new-login".into();
    current.decorate(&mut renamed).unwrap();
    assert_eq!(renamed.publication_total, Some(1));
    assert!(reason(&renamed).is_none());
    renamed.select(1);
    assert!(reason(&renamed).is_some());
    let mut unavailable = identity(42);
    unavailable.account_id = None;
    assert!(PrChecksSqlGuard::capture(&database, &unavailable, &summary()).is_err());
    let mut wrong = summary();
    wrong.pr.host = "another.invalid".into();
    assert!(PrChecksSqlGuard::capture(&database, &identity(42), &wrong).is_err());
}

#[test]
fn pr_checks_sql_guard_incomplete_257_actual_originals_disable_all_publication_after_rebuild() {
    let directory = directory();
    let database = directory.path().join("cache.sqlite3");
    for id in 1..=257 {
        pending_cache(&database, 42, id);
    }
    let guard = PrChecksSqlGuard::capture(&database, &identity(42), &summary()).unwrap();
    guard.validate(&database).unwrap();
    let mut current = workspace(42);
    guard.decorate(&mut current).unwrap();
    assert_eq!(current.publication_total, Some(257));
    assert_eq!(current.publication_rows.len(), 256);
    assert!(!current.publication_complete);
    assert!(current
        .publication_leases
        .as_ref()
        .unwrap()
        .is_unavailable());
    assert!(reason(&current).unwrap().contains("partial"));
    // Unknown/new-name targets cannot bypass the conservative captured lease.
    assert!(current
        .disabled_reason(ActionsCommand::SetVariable)
        .unwrap()
        .contains("unavailable"));
    current.caches = vec![action_cache(1000)];
    current.selected = 0;
    current.prepare_rows();
    assert!(reason(&current).unwrap().contains("unavailable"));
    // An older marker outside the shown256 still participates via generation.
    let sql = Connection::open(&database).unwrap();
    // Independently advance the actual omitted target using its stored key(s),
    // never a guessed spelling of its resource alias.
    assert!(sql.execute("UPDATE actions_write_generations SET generation=generation+1 WHERE host='github.invalid' AND principal='user-id:42' AND resource_key IN(SELECT resource_key FROM actions_write_resources WHERE attempt_id=1)",[]).unwrap()>0);
    assert!(guard.validate(&database).is_err());
    assert_eq!(
        ActionsWriteJournal::open(&database)
            .unwrap()
            .list_unresolved(&Scope::github("github.invalid", Some(42)).unwrap(), 256)
            .unwrap()
            .total,
        257
    );
}

#[test]
fn pr_checks_sql_guard_utf8_metadata_preflight_and_malformed_page_refuse_before_copy_or_empty_claim(
) {
    for action in [false, true] {
        let directory = directory();
        let database = directory.path().join("cache.sqlite3");
        PrChecksSqlGuard::capture(&database, &identity(42), &summary()).unwrap();
        if action {
            pending_cache(&database, 42, 71);
        }
        let sql = Connection::open(&database).unwrap();
        if action {
            sql.execute("UPDATE actions_write_attempts SET context_json=?1 WHERE host='github.invalid' AND principal='user-id:42'",params!["雪".repeat(50_000)]).unwrap();
        } else {
            sql.execute("INSERT INTO sync_reservations(host,login,source,generation,active) VALUES('github.invalid','user-id:42',?1,1,1)",params!["雪".repeat(1400)]).unwrap();
        }
        let failure = PrChecksSqlGuard::capture(&database, &identity(42), &summary()).unwrap_err();
        assert!(
            failure.contains("before")
                && (failure.contains("UTF8") || failure.contains("metadata budget"))
        );
        let count:i64=sql.query_row("SELECT COUNT(*) FROM actions_write_attempts WHERE host='github.invalid' AND principal='user-id:42'",[],|row|row.get(0)).unwrap();
        assert_eq!(count, i64::from(action));
    }
}

#[test]
fn pr_checks_sql_guard_original_file_replacement_symlink_and_foreign_path_never_rebase() {
    use std::os::unix::fs::symlink;
    let directory = directory();
    let database = directory.path().join("cache.sqlite3");
    let guard = PrChecksSqlGuard::capture(&database, &identity(42), &summary()).unwrap();
    let replacement = directory.path().join("replacement.sqlite3");
    PrChecksSqlGuard::capture(&replacement, &identity(42), &summary()).unwrap();
    assert!(guard.validate(&replacement).is_err());
    let original = directory.path().join("original-retained.sqlite3");
    fs::rename(&database, &original).unwrap();
    fs::rename(&replacement, &database).unwrap();
    assert!(guard.validate(&database).unwrap_err().contains("replaced"));
    let alias = directory.path().join("alias.sqlite3");
    symlink(&database, &alias).unwrap();
    assert!(guard.validate(&alias).is_err());
    assert!(PrChecksSqlGuard::capture(&alias, &identity(42), &summary()).is_err());
    assert!(original.is_file());
    assert!(database.is_file());
}

#[test]
fn pr_checks_sql_guard_foreign_repo_and_body_only_changes_do_not_claim_payload_authority() {
    let directory = directory();
    let database = directory.path().join("cache.sqlite3");
    seed_cache(&database);
    let guard = PrChecksSqlGuard::capture(&database, &identity(42), &summary()).unwrap();
    let cache = Cache::open(&database).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    cache
        .put(
            &scope,
            &Record::new(
                "foreign",
                "a",
                "another/repo",
                json!({"body":"FOREIGN_PRIVATE_CANARY"}),
            ),
        )
        .unwrap();
    guard.validate(&database).unwrap();
    let sql = Connection::open(&database).unwrap();
    sql.execute("UPDATE records SET payload=?1 WHERE host='github.invalid' AND login='user-id:42' AND kind='private-fixture'",params![json!({"body":"CHANGED_BODY_NOT_CACHE_AUTHORITY"}).to_string()]).unwrap();
    guard.validate(&database).unwrap();
    let mut wrong = ActionsWorkspace::new(identity(42), vec!["another/repo".into()]);
    assert!(guard.decorate(&mut wrong).is_err());
    assert!(wrong.publication_rows.is_empty());
    assert!(!format!("{guard:?}").contains("PRIVATE"));
}
