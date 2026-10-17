//! Genuine first principal/page/repository/final principal plus real private
//! SQLite. No synthetic membership/current source constructor or live network.
use super::*;
use crate::{
    repository_catalogue_read_limits_test_support_v1 as support,
    test_support::{Fixture, GhStep},
};
use serde_json::json;

pub(super) fn expected() -> HostIdentity {
    HostIdentity {
        host: support::HOST.into(),
        login: "cached-login-is-intent".into(),
        account_id: Some(42),
        gh_version: String::new(),
    }
}
pub(super) fn row() -> Value {
    let mut repository = support::row(0);
    repository["url"] = json!("https://github.invalid/api/v3/repos/example/repo0000");
    json!({"id":"9007199254740993","unread":true,"reason":"review_requested","updated_at":"2026-10-15T18:00:00Z","last_read_at":null,
        "url":"https://github.invalid/api/v3/notifications/threads/9007199254740993","subscription_url":"https://github.invalid/api/v3/notifications/threads/9007199254740993/subscription",
        "repository":repository,"subject":{"title":"PRIVATE_REAL_NOTIFICATION_BODY","url":"https://github.invalid/api/v3/repos/example/repo0000/pulls/7","latest_comment_url":"https://github.invalid/api/v3/repos/example/repo0000/issues/comments/99","type":"PullRequest"}})
}
fn page(raw: &str, not_modified: bool, conditional: bool) -> GhStep {
    let mut args = vec![
        "api",
        "notifications?all=false&participating=false&per_page=50&page=1",
        "--hostname",
        support::HOST,
        "--method",
        "GET",
        "--include",
    ];
    if conditional {
        args.extend(["--header", "If-None-Match: \"snap\""]);
    }
    let mut step = GhStep::read(&args, raw);
    step.status = if not_modified { 1 } else { 0 };
    if not_modified {
        step.stderr = b"gh: HTTP 304\n".to_vec();
    }
    step
}
fn fresh_page(rows: Value) -> GhStep {
    page(
        &format!("HTTP/2 200 OK\nETag: \"snap\"\r\nX-Poll-Interval: 60\r\n\r\n{rows}"),
        false,
        false,
    )
}
pub(super) fn collection(empty: bool, final_uid: u64) -> Vec<GhStep> {
    let mut steps = support::principal(42);
    steps.push(fresh_page(if empty { json!([]) } else { json!([row()]) }));
    if !empty {
        for _ in 0..2 {
            steps.push(support::get("repos/example/repo0000", support::row(0)));
        }
    }
    steps.extend(support::principal(final_uid));
    steps
}
pub(super) fn bounded(f: &Fixture, deadline: Instant) -> GithubProvider {
    support::provider(f)
        .with_scoped_read_window(deadline, 1024 * 1024)
        .unwrap()
}
pub(super) fn load(
    f: &Fixture,
    cache: &mut Cache,
    provider: &GithubProvider,
    token: &CancellationToken,
    deadline: Instant,
) -> Arc<ClosedNotificationReadV63> {
    let pending = provider
        .begin_notification_read_v63(
            &NotificationQueryV63::default(),
            &expected(),
            cache,
            None,
            token,
            deadline,
        )
        .unwrap();
    assert_eq!(pending.coverage().pages, 1);
    let source = pending.finish(cache, token).unwrap();
    assert_eq!(
        source.scope(),
        &crate::storage::Scope::github(support::HOST, Some(42)).unwrap()
    );
    assert_eq!(source.identity().account_id, Some(42));
    assert!(source.generation() > 0);
    assert!(source.accounting().charged_bytes() < WHOLE);
    assert!(!format!("{source:?}").contains("PRIVATE_REAL"));
    assert!(f.root.join("cache.sqlite3").exists());
    source
}
#[test]
fn notifications_core_v63_opening_unknown_permission_refuses_before_private_history_or_selected_child(
) {
    let f = Fixture::new();
    let mut steps = collection(false, 42);
    let mut unknown = support::row(0);
    unknown.as_object_mut().unwrap().remove("permissions");
    steps[3] = support::get("repos/example/repo0000", unknown);
    steps.truncate(4);
    f.scripted_gh(&steps);
    let deadline = Instant::now() + Duration::from_secs(30);
    let provider = bounded(&f, deadline);
    let mut cache = Cache::open(&support::database(&f)).unwrap();
    let foreign = crate::storage::Scope::github(support::HOST, Some(99)).unwrap();
    cache
        .put_draft(&foreign, "foreign", "EXACT_FOREIGN_DRAFT")
        .unwrap();
    let result = provider.begin_notification_read_v63(
        &NotificationQueryV63::default(),
        &expected(),
        &mut cache,
        None,
        &CancellationToken::default(),
        deadline,
    );
    assert!(matches!(
        result,
        Err(GithubError {
            kind: GithubErrorKind::Permission,
            ..
        })
    ));
    assert_eq!(
        cache.draft(&foreign, "foreign").unwrap().as_deref(),
        Some("EXACT_FOREIGN_DRAFT")
    );
    assert_eq!(
        rusqlite::Connection::open(support::database(&f))
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM notification_write_attempts_v63",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    support::exact(&f, &steps);
}

#[test]
fn notifications_core_v63_actual_nonempty_and_typed_complete_empty_close_same_started_budget() {
    for empty in [false, true] {
        let f = Fixture::new();
        let steps = collection(empty, 42);
        f.scripted_gh(&steps);
        let mut cache = Cache::open(&support::database(&f)).unwrap();
        let foreign = crate::storage::Scope::github(support::HOST, Some(99)).unwrap();
        cache.put_draft(&foreign, "FOREIGN", "UNCHANGED").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let p = bounded(&f, deadline);
        let token = CancellationToken::default();
        let source = load(&f, &mut cache, &p, &token, deadline);
        assert_eq!(source.rows().len(), usize::from(!empty));
        assert!(source.coverage().complete);
        assert_eq!(source.accounting().requests(), if empty { 5 } else { 7 });
        assert_eq!(
            cache.draft(&foreign, "FOREIGN").unwrap().as_deref(),
            Some("UNCHANGED")
        );
        assert_eq!(steps.len(), if empty { 5 } else { 7 });
        support::exact(&f, &steps);
    }
}
#[test]
fn notifications_core_v63_actual304_reuses_same_body_arc_only_after_fresh_repo_sql_uid() {
    let f = Fixture::new();
    let mut steps = collection(false, 42);
    steps.extend(support::principal(42));
    steps.push(page("HTTP/2 304 Not Modified\nETag: \"snap\"\r\nContent-Length: 17\r\nX-Poll-Interval: 120\r\n\r\n",true,true));
    for _ in 0..2 {
        steps.push(support::get("repos/example/repo0000", support::row(0)));
    }
    steps.extend(support::principal(42));
    f.scripted_gh(&steps);
    let mut cache = Cache::open(&support::database(&f)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let p = bounded(&f, deadline);
    let token = CancellationToken::default();
    let original = load(&f, &mut cache, &p, &token, deadline);
    let pending = p
        .begin_notification_read_v63(
            original.query(),
            &expected(),
            &mut cache,
            Some(&original),
            &token,
            deadline,
        )
        .unwrap();
    let current = pending.finish(&cache, &token).unwrap();
    assert!(Arc::ptr_eq(&original.pages[0].rows, &current.pages[0].rows));
    assert!(!Arc::ptr_eq(&original, &current));
    assert_eq!(current.rows()[0].title(), "PRIVATE_REAL_NOTIFICATION_BODY");
    assert_eq!(current.generation(), original.generation() + 1);
    assert!(original.validate_local(&cache, &token, deadline).is_err());
    assert!(current.validate_local(&cache, &token, deadline).is_ok());
    assert_eq!(current.accounting().requests(), 7);
    assert_eq!(steps.len(), 14);
    support::exact(&f, &steps);
}
#[test]
fn notifications_core_v63_initial304_or_incomplete_json_never_becomes_empty_membership() {
    for response in [
        "HTTP/2 304 Not Modified\n\n",
        "HTTP/2 200 OK\n\n[",
        "HTTP/2 200 OK\n\n[]{}",
    ] {
        let f = Fixture::new();
        let mut steps = support::principal(42);
        steps.push(page(response, response.contains("304"), false));
        f.scripted_gh(&steps);
        let mut cache = Cache::open(&support::database(&f)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let p = bounded(&f, deadline);
        assert!(p
            .begin_notification_read_v63(
                &NotificationQueryV63::default(),
                &expected(),
                &mut cache,
                None,
                &CancellationToken::default(),
                deadline
            )
            .is_err());
        support::exact(&f, &steps);
        assert_eq!(steps.len(), 3);
    }
}
#[test]
fn notifications_core_v63_real_clear_foreign_db_and_changed_final_uid_refuse_private_close() {
    for mode in ["clear", "foreign", "uid"] {
        let f = Fixture::new();
        let mut steps = collection(false, if mode == "uid" { 43 } else { 42 });
        if mode != "uid" {
            steps.truncate(5);
        }
        f.scripted_gh(&steps);
        let mut cache = Cache::open(&support::database(&f)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let p = bounded(&f, deadline);
        let token = CancellationToken::default();
        let pending = p
            .begin_notification_read_v63(
                &NotificationQueryV63::default(),
                &expected(),
                &mut cache,
                None,
                &token,
                deadline,
            )
            .unwrap();
        assert_eq!(pending.rows().len(), 1);
        let foreign = Cache::open(&f.root.join("foreign.sqlite3")).unwrap();
        if mode == "clear" {
            cache
                .clear_records(&crate::storage::Scope::github(support::HOST, Some(42)).unwrap())
                .unwrap();
        }
        let selected = if mode == "foreign" { &foreign } else { &cache };
        assert!(pending.finish(selected, &token).is_err());
        support::exact(&f, &steps);
    }
}
#[test]
fn notifications_core_v63_actual304_recreated_repo_and_current_uid_loss_no_cached_admission() {
    for mode in ["repo", "uid", "row"] {
        let f = Fixture::new();
        let mut steps = collection(false, 42);
        steps.extend(support::principal(if mode == "uid" { 43 } else { 42 }));
        if mode == "repo" {
            steps.push(page("HTTP/2 304 Not Modified\n\n", true, true));
            let mut recreated = support::row(0);
            recreated["id"] = json!(20000);
            recreated["node_id"] = json!("R_RECREATED");
            steps.push(support::get("repos/example/repo0000", recreated));
        }
        if mode == "row" {
            let mut changed = row();
            changed["repository"]["id"] = json!(20000);
            changed["repository"]["node_id"] = json!("R_RECREATED");
            steps.push(page(
                &format!("HTTP/2 200 OK\nETag: \"snap\"\r\n\r\n{}", json!([changed])),
                false,
                true,
            ));
        }
        f.scripted_gh(&steps);
        let mut cache = Cache::open(&support::database(&f)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let p = bounded(&f, deadline);
        let token = CancellationToken::default();
        let old = load(&f, &mut cache, &p, &token, deadline);
        assert!(p
            .begin_notification_read_v63(
                old.query(),
                &expected(),
                &mut cache,
                Some(&old),
                &token,
                deadline
            )
            .is_err());
        support::exact(&f, &steps);
        assert_eq!(
            steps.len(),
            match mode {
                "uid" => 9,
                "row" => 10,
                _ => 11,
            }
        );
    }
}
#[test]
fn notifications_core_v63_foreign_original_runner_query_or_expired_window_zero_next_children() {
    let f = Fixture::new();
    let steps = collection(false, 42);
    f.scripted_gh(&steps);
    let mut cache = Cache::open(&support::database(&f)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let p = bounded(&f, deadline);
    let token = CancellationToken::default();
    let old = load(&f, &mut cache, &p, &token, deadline);
    let foreign = p.clone().with_runner(CommandRunner::default());
    assert!(foreign
        .begin_notification_read_v63(
            old.query(),
            &expected(),
            &mut cache,
            Some(&old),
            &token,
            deadline
        )
        .is_err());
    let query = NotificationQueryV63 {
        include_read: true,
        ..Default::default()
    };
    assert!(p
        .begin_notification_read_v63(
            &query,
            &expected(),
            &mut cache,
            Some(&old),
            &token,
            deadline
        )
        .is_err());
    assert!(p
        .begin_notification_read_v63(
            old.query(),
            &expected(),
            &mut cache,
            Some(&old),
            &token,
            Instant::now()
        )
        .is_err());
    support::exact(&f, &steps);
}

fn journal_rows(f: &Fixture) -> Vec<(i64, String, String, String)> {
    let sql = rusqlite::Connection::open_with_flags(
        support::database(f),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let mut stmt=sql.prepare("SELECT id,state,intent_digest,origin_json FROM notification_write_attempts_v63 ORDER BY id").unwrap();
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))
        .unwrap()
        .map(|row| row.unwrap())
        .collect();
    rows
}
#[test]
fn notifications_journal_v63_atomic_pending_restart_duplicate_and_once_permit_no_remote_effect() {
    let f = Fixture::new();
    let steps = collection(false, 42);
    f.scripted_gh(&steps);
    let mut cache = Cache::open(&support::database(&f)).unwrap();
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    let provider = bounded(&f, deadline);
    let phase = provider
        .begin_notification_read_v63(
            &NotificationQueryV63::default(),
            &expected(),
            &mut cache,
            None,
            &token,
            deadline,
        )
        .unwrap();
    let origin = phase.write_origin_v63(0).unwrap();
    let permit = cache
        .begin_notification_write_v63(
            &origin,
            phase.local_witness_v63(),
            crate::storage::NotificationRequestedV63::MarkRead,
            &token,
            deadline,
        )
        .unwrap();
    assert!(permit.marker().reference().attempt_id() > 0);
    assert_eq!(
        permit.marker().state(),
        crate::storage::NotificationWriteStateV63::Pending
    );
    assert!(cache
        .begin_notification_write_v63(
            &origin,
            phase.local_witness_v63(),
            crate::storage::NotificationRequestedV63::Ignore,
            &token,
            deadline
        )
        .is_err());
    permit.claim_once().unwrap();
    assert!(permit.claim_once().is_err());
    cache
        .validate_notification_permit_v63(
            &permit,
            &origin,
            phase.local_witness_v63(),
            &token,
            deadline,
        )
        .unwrap();
    let listed = cache
        .notification_attempts_v63(&origin, phase.local_witness_v63(), &token, deadline)
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].reference(), permit.marker().reference());
    let before = journal_rows(&f);
    assert_eq!(before.len(), 1);
    assert!(!before[0].3.contains("PRIVATE_REAL_NOTIFICATION_BODY"));
    assert!(before[0].3.contains("9007199254740993"));
    assert!(before[0].3.len() < 16384);
    phase.finish(&cache, &token).unwrap();
    drop(cache);
    drop(permit);
    let reopened = Cache::open(&support::database(&f)).unwrap();
    drop(reopened);
    assert_eq!(journal_rows(&f), before);
    support::exact(&f, &steps);
    assert_eq!(steps.len(), 7);
}
#[test]
fn notifications_journal_v63_true_sql_insert_failure_rolls_back_origin_and_attempt_together() {
    let f = Fixture::new();
    let steps = collection(false, 42);
    f.scripted_gh(&steps);
    let mut cache = Cache::open(&support::database(&f)).unwrap();
    let foreign = crate::storage::Scope::github(support::HOST, Some(99)).unwrap();
    cache
        .put_draft(&foreign, "foreign", "EXACT_FOREIGN_DRAFT")
        .unwrap();
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    let provider = bounded(&f, deadline);
    let phase = provider
        .begin_notification_read_v63(
            &NotificationQueryV63::default(),
            &expected(),
            &mut cache,
            None,
            &token,
            deadline,
        )
        .unwrap();
    let origin = phase.write_origin_v63(0).unwrap();
    let external = rusqlite::Connection::open(support::database(&f)).unwrap();
    external.execute_batch("CREATE TRIGGER reject_notification_origin BEFORE INSERT ON notification_write_attempts_v63 BEGIN SELECT RAISE(ABORT,'actual bounded-origin insertion refused'); END;").unwrap();
    assert!(cache
        .begin_notification_write_v63(
            &origin,
            phase.local_witness_v63(),
            crate::storage::NotificationRequestedV63::MarkRead,
            &token,
            deadline
        )
        .is_err());
    assert!(journal_rows(&f).is_empty());
    external
        .execute_batch("DROP TRIGGER reject_notification_origin")
        .unwrap();
    let permit = cache
        .begin_notification_write_v63(
            &origin,
            phase.local_witness_v63(),
            crate::storage::NotificationRequestedV63::MarkRead,
            &token,
            deadline,
        )
        .unwrap();
    assert_eq!(journal_rows(&f).len(), 1);
    assert_eq!(
        cache.draft(&foreign, "foreign").unwrap().as_deref(),
        Some("EXACT_FOREIGN_DRAFT")
    );
    drop(permit);
    phase.finish(&cache, &token).unwrap();
    support::exact(&f, &steps);
}
#[test]
fn notifications_journal_v63_clear_invalidates_source_but_never_deletes_protected_attempt_or_foreign_draft(
) {
    let f = Fixture::new();
    let mut steps = collection(false, 42);
    steps.truncate(5);
    f.scripted_gh(&steps);
    let mut cache = Cache::open(&support::database(&f)).unwrap();
    let foreign = crate::storage::Scope::github(support::HOST, Some(99)).unwrap();
    cache
        .put_draft(&foreign, "foreign", "UNCHANGED_FOREIGN")
        .unwrap();
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    let provider = bounded(&f, deadline);
    let phase = provider
        .begin_notification_read_v63(
            &NotificationQueryV63::default(),
            &expected(),
            &mut cache,
            None,
            &token,
            deadline,
        )
        .unwrap();
    let origin = phase.write_origin_v63(0).unwrap();
    let permit = cache
        .begin_notification_write_v63(
            &origin,
            phase.local_witness_v63(),
            crate::storage::NotificationRequestedV63::Unignore,
            &token,
            deadline,
        )
        .unwrap();
    let before = journal_rows(&f);
    cache.clear_records(origin.scope()).unwrap();
    assert!(cache
        .validate_notification_permit_v63(
            &permit,
            &origin,
            phase.local_witness_v63(),
            &token,
            deadline
        )
        .is_err());
    assert!(cache
        .begin_notification_write_v63(
            &origin,
            phase.local_witness_v63(),
            crate::storage::NotificationRequestedV63::MarkRead,
            &token,
            deadline
        )
        .is_err());
    assert_eq!(journal_rows(&f), before);
    assert_eq!(
        cache.draft(&foreign, "foreign").unwrap().as_deref(),
        Some("UNCHANGED_FOREIGN")
    );
    assert!(phase.finish(&cache, &token).is_err());
    support::exact(&f, &steps);
}
#[test]
fn notifications_journal_v63_original_token_clock_database_and_changed_attempt_are_fail_closed() {
    let f = Fixture::new();
    let steps = collection(false, 42);
    f.scripted_gh(&steps);
    let mut cache = Cache::open(&support::database(&f)).unwrap();
    let token = CancellationToken::default();
    let deadline = Instant::now() + Duration::from_secs(10);
    let provider = bounded(&f, deadline);
    let phase = provider
        .begin_notification_read_v63(
            &NotificationQueryV63::default(),
            &expected(),
            &mut cache,
            None,
            &token,
            deadline,
        )
        .unwrap();
    let origin = phase.write_origin_v63(0).unwrap();
    assert!(cache
        .begin_notification_write_v63(
            &origin,
            phase.local_witness_v63(),
            crate::storage::NotificationRequestedV63::MarkRead,
            &CancellationToken::default(),
            deadline
        )
        .is_err());
    assert!(cache
        .begin_notification_write_v63(
            &origin,
            phase.local_witness_v63(),
            crate::storage::NotificationRequestedV63::MarkRead,
            &token,
            deadline + Duration::from_secs(1)
        )
        .is_err());
    let mut foreign = Cache::open(&f.root.join("foreign.sqlite3")).unwrap();
    assert!(foreign
        .begin_notification_write_v63(
            &origin,
            phase.local_witness_v63(),
            crate::storage::NotificationRequestedV63::MarkRead,
            &token,
            deadline
        )
        .is_err());
    assert!(journal_rows(&f).is_empty());
    let permit = cache
        .begin_notification_write_v63(
            &origin,
            phase.local_witness_v63(),
            crate::storage::NotificationRequestedV63::Ignore,
            &token,
            deadline,
        )
        .unwrap();
    let sql = rusqlite::Connection::open(support::database(&f)).unwrap();
    sql.execute(
        "UPDATE notification_write_attempts_v63 SET generation=generation+1 WHERE id=?1",
        [permit.marker().reference().attempt_id()],
    )
    .unwrap();
    let changed = journal_rows(&f);
    assert!(cache
        .validate_notification_permit_v63(
            &permit,
            &origin,
            phase.local_witness_v63(),
            &token,
            deadline
        )
        .is_err());
    assert_eq!(journal_rows(&f), changed);
    // Even an external writer bypassing CHECK constraints cannot make the
    // loader materialize an unbounded saved origin before refusing it.
    let oversized = "UNTRUSTED_OVERSIZED_ORIGIN".repeat(2048);
    sql.execute_batch("PRAGMA ignore_check_constraints=ON")
        .unwrap();
    sql.execute(
        "UPDATE notification_write_attempts_v63 SET origin_json=?1 WHERE id=?2",
        rusqlite::params![oversized, permit.marker().reference().attempt_id()],
    )
    .unwrap();
    assert!(cache
        .notification_attempts_v63(&origin, phase.local_witness_v63(), &token, deadline)
        .is_err());
    let bytes:usize=sql.query_row("SELECT length(CAST(origin_json AS BLOB)) FROM notification_write_attempts_v63 WHERE id=?1",
        [permit.marker().reference().attempt_id()],|row|row.get(0)).unwrap();
    assert_eq!(bytes, oversized.len());
    phase.finish(&cache, &token).unwrap();
    support::exact(&f, &steps);
}
