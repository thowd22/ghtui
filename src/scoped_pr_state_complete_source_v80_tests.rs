//! Actual State Confirm -> Back -> same Background source read -> exact atomic
//! membership publication. Expected SQL is derived independently from the
//! original rows + the fixed authorized effects, never from poststate blanket
//! exclusions. All transport is the actual strict gh fixture.
use super::*;
use rusqlite::types::Value as SqlValue;
fn text_sql(value: &str) -> SqlValue {
    SqlValue::Text(value.to_owned())
}
type Sql = BTreeMap<String, Vec<Vec<SqlValue>>>;
fn rows(s: &Setup) -> Sql {
    let db = rusqlite::Connection::open_with_flags(
        database(&s.fixture),
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    let names=db.prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").unwrap().query_map([],|r|r.get::<_,String>(0)).unwrap().collect::<Result<Vec<_>,_>>().unwrap();
    let mut out = BTreeMap::new();
    for name in names {
        let mut q = db
            .prepare(&format!("SELECT * FROM \"{}\"", name.replace('"', "\"\"")))
            .unwrap();
        let columns = q.column_count();
        let values = q
            .query_map([], |r| {
                (0..columns)
                    .map(|i| r.get(i))
                    .collect::<Result<Vec<SqlValue>, _>>()
            })
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        out.insert(name, values);
    }
    canonical(&mut out);
    out
}
fn canonical(rows: &mut Sql) {
    for values in rows.values_mut() {
        values.sort_by_cached_key(|v| format!("{v:?}"));
    }
}
fn read_tail() -> Vec<GhStep> {
    let item = json!({"id":2008,"node_id":"INDEPENDENT_SEARCH_ISSUE_2008","number":8,"title":"Actual next open PR café 雪","body":"fresh query body",
        "html_url":format!("https://github.invalid/{REPO}/pull/8"),"url":format!("https://github.invalid/api/v3/repos/{REPO}/issues/8"),
        "repository_url":format!("https://github.invalid/api/v3/repos/{REPO}"),"pull_request":{"url":format!("https://github.invalid/api/v3/repos/{REPO}/pulls/8")},
        "user":{"login":"queue-author","type":"User"},"state":"open","draft":false,"created_at":"2026-10-16T00:00:00Z","updated_at":"2026-10-16T01:00:00Z"});
    let mut result = principal();
    for role in ["author", "assignee", "review-requested"] {
        let mut call = get(
            "search/issues",
            json!({"total_count":1,"incomplete_results":false,"items":[item.clone()]}),
        );
        call.args.extend([
            "-f".into(),
            format!("q=is:pr is:open {role}:octocat"),
            "-f".into(),
            "per_page=100".into(),
            "-f".into(),
            "page=1".into(),
        ]);
        result.push(call);
    }
    result.extend(principal());
    assert_eq!(result.len(), 7);
    result
}
fn expected_pr() -> crate::providers::gh::PullRequest {
    crate::providers::gh::PullRequest {
        host: "github.invalid".into(),
        id: "INDEPENDENT_SEARCH_ISSUE_2008".into(),
        repository: REPO.into(),
        number: 8,
        title: "Actual next open PR café 雪".into(),
        body: "fresh query body".into(),
        url: format!("https://github.invalid/{REPO}/pull/8"),
        author: crate::providers::gh::Actor {
            login: "queue-author".into(),
            actor_type: "User".into(),
            ..Default::default()
        },
        state: "open".into(),
        draft: false,
        created_at: "2026-10-16T00:00:00Z".into(),
        updated_at: "2026-10-16T01:00:00Z".into(),
        queues: [
            crate::providers::gh::Queue::Authored,
            crate::providers::gh::Queue::Assigned,
            crate::providers::gh::Queue::ReviewRequested,
        ]
        .into_iter()
        .collect(),
    }
}
fn request(s: &mut Setup) -> Arc<native::ScopedPrCollaborationWorkV17> {
    s.app.dispatch(Action::Refresh);
    assert!(s.app.refresh_requested);
    native::prepare_state_source_sync_v80(&mut s.app, &database(&s.fixture));
    assert!(!s.app.refresh_requested);
    let work = s.app.scoped_pr_collaboration_work_v17.take().unwrap();
    assert!(matches!(
        work.original(),
        native::CollaborationWorkOriginV78::StateInbox(_)
    ));
    assert_eq!(
        work.phase(),
        native::ConversationPhaseKindV17::StateSourceSyncV80
    );
    assert!(work.matches(&s.app));
    work
}
fn expected_sql(mut before: Sql, source: &str, generation: u64, fetched: u64) -> Sql {
    let own = |r: &Vec<SqlValue>| {
        r[0] == SqlValue::Text("github.invalid".into())
            && r[1] == SqlValue::Text("user-id:42".into())
    };
    // Derive the lookup trigger effects from actual typed PRE bindings and
    // reservation keys. OLD+NEW reservation UPDATE contributes twice; the
    // active and visible binding UPDATEs each contribute once. Group by the
    // actual namespace/repository, never by an observed poststate epoch.
    let mut lookup_deltas = BTreeMap::<(String, String), i64>::new();
    for binding in before["sync_resource_bindings"].iter().filter(|r| own(r)) {
        let (SqlValue::Text(namespace), SqlValue::Text(repository)) = (&binding[3], &binding[5])
        else {
            panic!("Actual PRE binding namespace/repository must be typed text");
        };
        let reserved = before["sync_reservations"]
            .iter()
            .any(|r| own(r) && r[2] == binding[2]);
        *lookup_deltas
            .entry((namespace.clone(), repository.clone()))
            .or_default() += 2 + 2 * i64::from(reserved);
    }
    for ((namespace, repository), delta) in lookup_deltas {
        let epochs = before.get_mut("sync_binding_lookup_epochs").unwrap();
        if let Some(row) = epochs
            .iter_mut()
            .find(|r| own(r) && r[2] == text_sql(&namespace) && r[3] == text_sql(&repository))
        {
            let SqlValue::Integer(epoch) = &mut row[4] else {
                panic!("Actual PRE lookup epoch must be an integer");
            };
            *epoch += delta;
        } else {
            epochs.push(vec![
                text_sql("github.invalid"),
                text_sql("user-id:42"),
                text_sql(&namespace),
                text_sql(&repository),
                SqlValue::Integer(delta),
            ]);
        }
    }
    for row in before.get_mut("sync_reservations").unwrap() {
        if own(row) {
            let SqlValue::Integer(n) = &mut row[3] else {
                panic!("actual integer reservation")
            };
            *n += 1;
            row[4] = SqlValue::Integer(0);
        }
    }
    let rs = before.get_mut("sync_reservations").unwrap();
    if let Some(row) = rs
        .iter_mut()
        .find(|r| own(r) && r[2] == SqlValue::Text(source.into()))
    {
        row[3] = SqlValue::Integer(generation as i64);
        row[4] = SqlValue::Integer(1);
    } else {
        rs.push(vec![
            text_sql("github.invalid"),
            text_sql("user-id:42"),
            text_sql(source),
            SqlValue::Integer(generation as i64),
            SqlValue::Integer(1),
        ]);
    }
    for row in before.get_mut("sync_resource_bindings").unwrap() {
        if own(row) {
            row[9] = SqlValue::Null;
            row[10] = SqlValue::Integer(0);
        }
    }
    let own_members = before["checkpoint_records"]
        .iter()
        .filter(|r| own(r) && r[2] == SqlValue::Text(source.into()))
        .map(|r| (r[3].clone(), r[4].clone()))
        .collect::<Vec<_>>();
    let members = before.get_mut("checkpoint_records").unwrap();
    members.retain(|r| !(own(r) && r[2] == SqlValue::Text(source.into())));
    for (kind, id) in [
        ("inbox-meta", source.to_owned()),
        ("pr", format!("{REPO}#8")),
    ] {
        members.push(vec![
            text_sql("github.invalid"),
            text_sql("user-id:42"),
            text_sql(source),
            text_sql(kind),
            id.into(),
        ]);
    }
    let still = before["checkpoint_records"].clone();
    before.get_mut("records").unwrap().retain(|r| {
        !own(r)
            || !own_members.contains(&(r[2].clone(), r[3].clone()))
            || still.iter().any(|m| own(m) && m[3] == r[2] && m[4] == r[3])
    });
    let pr = expected_pr();
    let coverage = crate::providers::gh::Coverage {
        loaded: 1,
        total_matches: 3,
        complete: true,
        reasons: vec![],
    };
    let insert = [
        (
            "inbox-meta",
            source.to_owned(),
            String::new(),
            format!(
                "{{\"coverage\":{}}}",
                serde_json::to_string(&coverage).unwrap()
            ),
            SqlValue::Null,
        ),
        (
            "pr",
            format!("{REPO}#8"),
            REPO.into(),
            serde_json::to_string(&pr).unwrap(),
            SqlValue::Text(pr.updated_at.clone()),
        ),
    ];
    for (kind, id, repository, payload, updated) in insert {
        let records = before.get_mut("records").unwrap();
        records.retain(|r| {
            !(own(r) && r[2] == SqlValue::Text(kind.into()) && r[3] == SqlValue::Text(id.clone()))
        });
        records.push(vec![
            text_sql("github.invalid"),
            text_sql("user-id:42"),
            text_sql(kind),
            id.into(),
            repository.into(),
            payload.into(),
            SqlValue::Integer(fetched as i64),
            updated,
            SqlValue::Integer(1),
        ]);
    }
    let checkpoints = before.get_mut("checkpoints").unwrap();
    checkpoints.retain(|r| !(own(r) && r[2] == SqlValue::Text(source.into())));
    checkpoints.push(vec![
        text_sql("github.invalid"),
        text_sql("user-id:42"),
        text_sql(source),
        SqlValue::Integer(generation as i64),
        SqlValue::Integer(fetched as i64),
        SqlValue::Integer(1),
    ]);
    let wm = before
        .get_mut("pr_write_state_source_watermarks_v73")
        .unwrap();
    wm.retain(|r| !(own(r) && r[3] == SqlValue::Text(source.into())));
    wm.push(vec![
        text_sql("github.invalid"),
        text_sql("user-id:42"),
        text_sql("PERSONAL_PR_1007"),
        text_sql(source),
        SqlValue::Integer(generation as i64),
        SqlValue::Integer(1),
        SqlValue::Integer(10000),
        SqlValue::Integer(1007),
    ]);
    canonical(&mut before);
    before
}
#[test]
fn pr_lifecycle_v68_real_confirm_back_background_source_sync_publishes_exact_own_membership_without_alias_renewal(
) {
    let (mut s, mut bg, writer) = confirmed();
    back(&mut s);
    let scope = s.app.github.as_ref().unwrap().scope().unwrap();
    let source = s.app.github.as_ref().unwrap().source();
    let old = Cache::open(&database(&s.fixture))
        .unwrap()
        .sync_reservation(&scope, &source)
        .unwrap();
    let db = rusqlite::Connection::open(database(&s.fixture)).unwrap();
    let (r,p):(Option<u64>,Option<u64>)=db.query_row("SELECT (SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3),(SELECT generation FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3)",rusqlite::params![scope.host,scope.login,source],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
    let expected_generation = (r.unwrap_or(0) + u64::from(r.is_some())).max(p.unwrap_or(0)) + 1;
    assert!(
        old.is_none(),
        "Confirmed scope fence already retired the old read reservation"
    );
    let stable = rows(&s);
    let before = s.steps.len();
    s.steps.extend(read_tail());
    s.fixture.scripted_gh(&s.steps);
    let start = crate::storage::now();
    let work = request(&mut s);
    assert!(bg.submit_scoped_pr_collaboration_v17(
        &mut s.app,
        work,
        s.api.clone(),
        database(&s.fixture),
        writer.client()
    ));
    finish(&mut s, &mut bg);
    let g = s.app.github.as_ref().unwrap();
    assert!(!g.state_membership_unavailable_v68() && g.pr.is_none() && g.inbox.coverage.complete);
    assert_eq!(g.inbox.pull_requests, vec![expected_pr()]);
    assert_eq!(g.generation, expected_generation);
    let crate::github_workspace::Freshness::Fresh { fetched_at } = g.freshness else {
        panic!("actual fresh source")
    };
    assert!((start..=crate::storage::now()).contains(&fetched_at));
    assert_eq!(
        rows(&s),
        expected_sql(stable, &source, expected_generation, fetched_at)
    );
    let cache = Cache::open(&database(&s.fixture)).unwrap();
    assert!(cache.sync_frame(&scope, &source).unwrap().is_some());
    let frame = paint(&mut s.app, 120, 40);
    assert!(frame.contains("Actual next open PR") && !frame.contains("counts unavailable"));
    assert_eq!(s.steps.len(), before + 7);
    assert_calls(&s, s.steps.len());
    assert_eq!(git(&s.fixture, &["rev-parse", "HEAD"]), s.git[0]);
    for (name, expected) in [
        (".git/index", &s.git[1]),
        (".git/config", &s.git[2]),
        ("kept", &s.git[3]),
    ] {
        assert_eq!(&fs::read(s.fixture.repo().join(name)).unwrap(), expected);
    }
}
#[test]
fn pr_lifecycle_v68_source_sync_oversized_actual_environment_and_pre_core_clear_refuse_without_child_or_sql(
) {
    for clear in [false, true] {
        let (mut s, _bg, _writer) = confirmed();
        back(&mut s);
        let sync = StateSourceSyncV76::capture(&s.app, &database(&s.fixture)).unwrap();
        if clear {
            let scope = s.app.github.as_ref().unwrap().scope().unwrap();
            Cache::open(&database(&s.fixture))
                .unwrap()
                .clear_records(&scope)
                .unwrap();
        }
        let stable = all_sql(&s);
        let before = s.steps.len();
        let api = if clear {
            s.api.clone()
        } else {
            s.api
                .clone()
                .with_env("STATE_SOURCE_BOUND_PROOF", "x".repeat(2 * 1024 * 1024))
        };
        assert!(crate::providers::gh::StateSourceReadV80::read(sync, &api).is_err());
        unchanged_local(&s, &stable, before);
    }
}
#[test]
fn pr_lifecycle_v68_repeated_complete_source_refresh_and_authenticated_cached_restart_keep_the_closed_read_route(
) {
    let (mut s, mut bg, writer) = confirmed();
    back(&mut s);
    let scope = s.app.github.as_ref().unwrap().scope().unwrap();
    let source = s.app.github.as_ref().unwrap().source();
    for _ in 0..2 {
        let baseline = rows(&s);
        let db = rusqlite::Connection::open(database(&s.fixture)).unwrap();
        let(r,p):(Option<u64>,Option<u64>)=db.query_row("SELECT (SELECT generation FROM sync_reservations WHERE host=?1 AND login=?2 AND source=?3),(SELECT generation FROM checkpoints WHERE host=?1 AND login=?2 AND source=?3)",rusqlite::params![scope.host,scope.login,source],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        let next = (r.unwrap_or(0) + u64::from(r.is_some())).max(p.unwrap_or(0)) + 1;
        s.steps.extend(read_tail());
        s.fixture.scripted_gh(&s.steps);
        let start = crate::storage::now();
        let work = request(&mut s);
        assert!(bg.submit_scoped_pr_collaboration_v17(
            &mut s.app,
            work,
            s.api.clone(),
            database(&s.fixture),
            writer.client()
        ));
        finish(&mut s, &mut bg);
        let g = s.app.github.as_ref().unwrap();
        assert!(!g.state_membership_unavailable_v68() && g.state_source_sync_required_v80());
        let crate::github_workspace::Freshness::Fresh { fetched_at } = g.freshness else {
            panic!("actual fresh state")
        };
        assert!((start..=crate::storage::now()).contains(&fetched_at));
        assert_eq!(g.inbox.pull_requests, vec![expected_pr()]);
        assert_eq!(rows(&s), expected_sql(baseline, &source, next, fetched_at));
        assert_calls(&s, s.steps.len());
        assert!(
            Cache::open(&database(&s.fixture))
                .unwrap()
                .sync_frame(&scope, "pr-write-projection:PERSONAL_PR_1007")
                .is_err(),
            "Only own complete source is renewed, never an old alias"
        );
    }
    let stable = all_sql(&s);
    let before = s.steps.len();
    s.steps.extend(principal());
    s.fixture.scripted_gh(&s.steps);
    let startup = GithubWorkspace::legacy_startup_v80(
        &s.api,
        vec![],
        Default::default(),
        &database(&s.fixture),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(
        !startup.state_membership_unavailable_v68() && startup.state_source_sync_required_v80()
    );
    assert_eq!(startup.inbox.pull_requests, vec![expected_pr()]);
    assert!(matches!(
        startup.freshness,
        crate::github_workspace::Freshness::Cached { .. }
    ));
    assert_eq!(
        s.steps.len(),
        before + 2,
        "Same required startup principal only; cached admission issues no GH child"
    );
    unchanged_local(&s, &stable, s.steps.len());
}
#[test]
fn pr_lifecycle_v68_source_sync_actual_final_account_change_and_atomic_insert_failure_preserve_every_row(
) {
    for account in [false, true] {
        let (mut s, _bg, _writer) = confirmed();
        back(&mut s);
        let mut tail = read_tail();
        if account {
            tail.last_mut().unwrap().stdout =
                serde_json::to_vec(&json!({"id":43,"login":"other"})).unwrap();
        }
        let first = s.steps.len();
        s.steps.extend(tail);
        s.fixture.scripted_gh(&s.steps);
        let sync = StateSourceSyncV76::capture(&s.app, &database(&s.fixture)).unwrap();
        let read = crate::providers::gh::StateSourceReadV80::read(sync, &s.api).unwrap();
        if !account {
            rusqlite::Connection::open(database(&s.fixture)).unwrap().execute_batch("CREATE TRIGGER refuse_state_source_v80 BEFORE INSERT ON records WHEN NEW.kind='inbox-meta' BEGIN SELECT RAISE(ABORT,'actual atomic State source rollback'); END;").unwrap();
        }
        let stable = all_sql(&s);
        let prepared = crate::storage::PreparedStateSourceCommitV80::prepare(&read).unwrap();
        assert_eq!(
            all_sql(&s),
            stable,
            "Preparation/final UID cannot write a generation, record, draft or fence"
        );
        match read.close() {
            Err(error) => {
                assert!(account);
                assert!(matches!(error.kind, GithubErrorKind::IdentityChanged));
            }
            Ok(closed) => {
                assert!(!account);
                assert!(prepared.commit(closed).is_err());
            }
        }
        assert_eq!(s.steps.len(), first + 7);
        unchanged_local(&s, &stable, s.steps.len());
        if !account {
            rusqlite::Connection::open(database(&s.fixture))
                .unwrap()
                .execute_batch("DROP TRIGGER refuse_state_source_v80;")
                .unwrap();
        }
    }
}
