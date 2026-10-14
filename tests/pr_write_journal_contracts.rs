#[path = "support/providers.rs"]
mod support;
use ghtui::{
    pr_write_journal::{
        PrConfirmedEffect, PrEffectKind, PrWriteIntent, PrWriteJournal, PrWriteState, PrWriteTarget,
    },
    providers::{
        gh::{Actor, GithubProvider, HostIdentity, PrSummary, PullRequest},
        runner::{CancellationToken, CommandRunner},
    },
    storage::{Cache, Record},
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs};
use support::{Fixture, GhStep};

fn identity(id: u64) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(id),
        gh_version: "gh version 2.101.0".into(),
    }
}
fn target() -> PrSummary {
    PrSummary {
        pr: PullRequest {
            host: "github.invalid".into(),
            id: "PR_example/repo_7".into(),
            repository: "example/repo".into(),
            number: 7,
            title: "PR 7".into(),
            body: "details".into(),
            url: "https://github.invalid/example/repo/pull/7".into(),
            author: Actor::default(),
            state: "open".into(),
            draft: false,
            created_at: String::new(),
            updated_at: "t0".into(),
            queues: Default::default(),
        },
        head_sha: "a".repeat(40),
        base_sha: "b".repeat(40),
        head_ref: "feature".into(),
        base_ref: "main".into(),
        merged: false,
        auto_merge: false,
        merge_commit_sha: None,
        mergeable: Some(true),
        mergeable_state: "clean".into(),
        additions: 0,
        deletions: 0,
        changed_files: 0,
    }
}
fn intent(body: &str) -> PrWriteIntent {
    PrWriteIntent {
        command: "new-comment".into(),
        selected_resource: "pr".into(),
        observed_head: "a".repeat(40),
        values: BTreeMap::from([("body".into(), body.into())]),
    }
}
fn effect() -> PrConfirmedEffect {
    PrConfirmedEffect {
        kind: PrEffectKind::Comment,
        resource_id: "9".into(),
        node_id: Some("IC_9".into()),
    }
}
fn provider(fixture: &Fixture, runner: CommandRunner) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
        .with_runner(runner)
}
fn api(endpoint: &str, value: Value, paged: bool) -> GhStep {
    let mut args = vec![
        "api".into(),
        endpoint.into(),
        "--hostname".into(),
        "github.invalid".into(),
        "--method".into(),
        "GET".into(),
    ];
    if paged {
        args.extend(["--paginate".into(), "--slurp".into()]);
    }
    GhStep {
        args,
        stdin: vec![],
        stdout: serde_json::to_vec(&value).unwrap(),
        stderr: vec![],
        status: 0,
        delay_ms: 0,
    }
}
fn account(id: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        api("user", json!({"login":"octocat","id":id}), false),
    ]
}
fn inspection_steps() -> Vec<GhStep> {
    let pr = json!({"node_id":"PR_example/repo_7","number":7,"title":"PR 7","repository_url":"https://api.github.invalid/repos/example/repo","html_url":"https://github.invalid/example/repo/pull/7","user":{"login":"octocat","id":42,"type":"User"},"state":"closed","merged":true,"updated_at":"t1","head":{"sha":"cccccccccccccccccccccccccccccccccccccccc","ref":"feature"},"base":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"main"}});
    let mut steps = account(42);
    steps.push(api("repos/example/repo/pulls/7", pr.clone(), false));
    for endpoint in [
        "repos/example/repo/issues/7/comments?per_page=100",
        "repos/example/repo/pulls/7/commits?per_page=100",
        "repos/example/repo/pulls/7/files?per_page=100",
        "repos/example/repo/pulls/7/reviews?per_page=100",
        "repos/example/repo/issues/7/timeline?per_page=100",
    ] {
        steps.push(api(endpoint, json!([[]]), true));
    }
    steps.push(api("repos/example/repo/pulls/7", pr, false));
    steps.extend(account(42));
    steps
}
fn commands(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn conversation_comments_journal_pending_survives_restart_blocks_changed_intent_and_repo_alias() {
    let fixture = Fixture::new();
    let path = fixture.repo().join("cache.sqlite3");
    let scope = PrWriteTarget::new(&identity(42), &target()).unwrap();
    let payload = intent("private Unicode draft 雪 ghp_not_a_real_secret_123");
    let mut store = PrWriteJournal::open(&path).unwrap();
    let permit = store.begin(&scope, &payload).unwrap();
    let attempt = permit.marker().attempt_id;
    drop(permit);
    drop(store);
    let mut reopened = PrWriteJournal::open(&path).unwrap();
    let pending = reopened.blocking(&scope).unwrap().unwrap();
    assert_eq!(pending.attempt_id, attempt);
    assert_eq!(pending.state, PrWriteState::Pending);
    assert_eq!(pending.payload_sha256, payload.digest().unwrap());
    let mut renamed = target();
    renamed.pr.repository = "Example/Renamed".into();
    renamed.pr.number = 88;
    let alias = PrWriteTarget::new(&identity(42), &renamed).unwrap();
    let mut different = intent("a different comment");
    different.command = "subscription".into();
    assert!(reopened
        .begin(&alias, &different)
        .unwrap_err()
        .to_string()
        .contains("unresolved publication"));
    assert_eq!(
        reopened
            .latest_matching(&alias, &payload)
            .unwrap()
            .unwrap()
            .attempt_id,
        attempt
    );
    assert!(commands(&fixture).is_empty());
    assert!(
        !fs::read(&path)
            .unwrap()
            .windows(payload.values["body"].len())
            .any(|bytes| bytes == payload.values["body"].as_bytes()),
        "journal must not duplicate submitted content"
    );
}

#[test]
fn conversation_comments_journal_scope_uses_host_numeric_account_and_canonical_node() {
    let fixture = Fixture::new();
    let mut store = PrWriteJournal::open(&fixture.repo().join("cache.sqlite3")).unwrap();
    let scope = PrWriteTarget::new(&identity(42), &target()).unwrap();
    store.begin(&scope, &intent("first")).unwrap();
    let mut renamed_identity = identity(42);
    renamed_identity.host = "GITHUB.INVALID".into();
    renamed_identity.login = "renamed-login".into();
    let alias = PrWriteTarget::new(&renamed_identity, &target()).unwrap();
    assert!(store.blocking(&alias).unwrap().is_some());
    let reassigned = PrWriteTarget::new(&identity(43), &target()).unwrap();
    assert!(store.blocking(&reassigned).unwrap().is_none());
    store.begin(&reassigned, &intent("other account")).unwrap();
    let mut second = target();
    second.pr.id = "PR_other".into();
    store
        .begin(
            &PrWriteTarget::new(&identity(42), &second).unwrap(),
            &intent("other PR"),
        )
        .unwrap();
    let mut host = identity(42);
    host.host = "enterprise.invalid".into();
    second.pr.host = host.host.clone();
    second.pr.id = target().pr.id;
    store
        .begin(
            &PrWriteTarget::new(&host, &second).unwrap(),
            &intent("other host"),
        )
        .unwrap();
    assert!(PrWriteTarget::new(
        &HostIdentity {
            account_id: None,
            ..identity(42)
        },
        &target()
    )
    .is_err());
    assert!(PrWriteTarget::new(&identity(0), &target()).is_err());
    assert!(commands(&fixture).is_empty());
}

#[test]
fn conversation_comments_journal_permit_is_exact_single_use_and_cross_database_safe() {
    let fixture = Fixture::new();
    let mut store = PrWriteJournal::open(&fixture.repo().join("one.sqlite3")).unwrap();
    let mut other = PrWriteJournal::open(&fixture.repo().join("two.sqlite3")).unwrap();
    let scope = PrWriteTarget::new(&identity(42), &target()).unwrap();
    let payload = intent("comment");
    let permit = store.begin(&scope, &payload).unwrap();
    assert!(other.claim(&permit, &scope, &payload).is_err());
    assert!(store
        .claim(&permit, &scope, &intent("edited text"))
        .is_err());
    assert!(store.mark_confirmed(&permit, &effect()).is_err());
    store.claim(&permit, &scope, &payload).unwrap();
    assert!(store.claim(&permit, &scope, &payload).is_err());
    store.mark_uncertain(&permit).unwrap();
    assert_eq!(
        store.blocking(&scope).unwrap().unwrap().state,
        PrWriteState::Uncertain
    );
    assert!(other.mark_confirmed(&permit, &effect()).is_err());
    assert!(commands(&fixture).is_empty());
}

#[test]
fn conversation_comments_journal_confirmed_restore_detection_does_not_ban_fresh_identical_text() {
    let fixture = Fixture::new();
    let path = fixture.repo().join("cache.sqlite3");
    let mut store = PrWriteJournal::open(&path).unwrap();
    let scope = PrWriteTarget::new(&identity(42), &target()).unwrap();
    let payload = intent("deliberate repeated message");
    let permit = store.begin(&scope, &payload).unwrap();
    store.claim(&permit, &scope, &payload).unwrap();
    let mut invalid = effect();
    invalid.resource_id = "this is a body, not an ID".into();
    assert!(store.mark_confirmed(&permit, &invalid).is_err());
    store.mark_confirmed(&permit, &effect()).unwrap();
    assert!(store.blocking(&scope).unwrap().is_none());
    assert!(store.mark_uncertain(&permit).is_err());
    drop(store);
    let mut reopened = PrWriteJournal::open(&path).unwrap();
    let confirmed = reopened.latest_matching(&scope, &payload).unwrap().unwrap();
    assert_eq!(confirmed.state, PrWriteState::Confirmed);
    assert_eq!(confirmed.confirmed, Some(effect()));
    let original = confirmed.publication_reference();
    let mut refreshed_intent = payload.clone();
    refreshed_intent.observed_head = "c".repeat(40);
    assert!(
        reopened
            .confirmed_original(&scope, &original, &refreshed_intent)
            .unwrap()
            .is_some(),
        "an already-published restored draft must stay detected after its PR head changes"
    );
    refreshed_intent
        .values
        .insert("body".into(), "genuinely edited draft".into());
    assert!(reopened
        .confirmed_original(&scope, &original, &refreshed_intent)
        .unwrap()
        .is_none());
    assert!(
        reopened
            .confirmed_original(
                &PrWriteTarget::new(&identity(43), &target()).unwrap(),
                &original,
                &payload
            )
            .unwrap()
            .is_none(),
        "another account cannot learn a confirmed record"
    );
    let fresh = reopened.begin(&scope, &payload).unwrap();
    assert_ne!(fresh.marker().attempt_id, permit.marker().attempt_id);
    assert_eq!(
        reopened.blocking(&scope).unwrap().unwrap().attempt_id,
        fresh.marker().attempt_id
    );
}

#[test]
fn conversation_comments_journal_storage_failures_never_issue_a_permit_or_clear_pending() {
    let fixture = Fixture::new();
    let path = fixture.repo().join("cache.sqlite3");
    let mut store = PrWriteJournal::open(&path).unwrap();
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TRIGGER reject_publish BEFORE INSERT ON pr_write_attempts BEGIN SELECT RAISE(ABORT,'fixture persistence failure'); END;").unwrap();
    let scope = PrWriteTarget::new(&identity(42), &target()).unwrap();
    let payload = intent("new");
    assert!(store.begin(&scope, &payload).is_err());
    assert!(store.blocking(&scope).unwrap().is_none());
    assert_eq!(
        db.query_row("SELECT count(*) FROM pr_write_scopes", [], |row| row
            .get::<_, u64>(0))
            .unwrap(),
        0,
        "failed begin must roll back its generation too"
    );
    db.execute_batch("DROP TRIGGER reject_publish;").unwrap();
    let permit = store.begin(&scope, &payload).unwrap();
    store.claim(&permit, &scope, &payload).unwrap();
    db.execute_batch("CREATE TRIGGER reject_confirm BEFORE UPDATE ON pr_write_attempts BEGIN SELECT RAISE(ABORT,'fixture confirmation failure'); END;").unwrap();
    assert!(store.mark_confirmed(&permit, &effect()).is_err());
    drop(store);
    let mut reopened = PrWriteJournal::open(&path).unwrap();
    assert_eq!(
        reopened.blocking(&scope).unwrap().unwrap().state,
        PrWriteState::Pending
    );
    assert!(reopened.begin(&scope, &intent("edited")).is_err());
    assert!(commands(&fixture).is_empty());
}

#[test]
fn conversation_comments_journal_only_presented_fresh_observation_can_acknowledge() {
    let fixture = Fixture::new();
    let runner = CommandRunner::default();
    runner.mark_ambiguous("github.invalid/example/repo/pull/7");
    let mut steps = inspection_steps();
    steps.extend(account(42));
    fixture.scripted_gh(&steps);
    let client = provider(&fixture, runner);
    let path = fixture.repo().join("cache.sqlite3");
    let mut store = PrWriteJournal::open(&path).unwrap();
    let scope = PrWriteTarget::new(&identity(42), &target()).unwrap();
    let permit = store.begin(&scope, &intent("unknown")).unwrap();
    store.mark_uncertain(&permit).unwrap();
    let inspection = store
        .prepare_inspection(
            &client,
            &identity(42),
            &target(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert_eq!(inspection.observation().identity().account_id, Some(42));
    assert!(inspection.observation().details().summary.merged);
    assert_eq!(
        inspection.observation().details().summary.head_sha,
        "c".repeat(40)
    );
    assert!(
        store.blocking(&scope).unwrap().is_some(),
        "inspection/presentation alone must not acknowledge"
    );
    assert_eq!(
        store
            .acknowledge(
                &client,
                inspection,
                &identity(42),
                &CancellationToken::default()
            )
            .unwrap(),
        1
    );
    assert!(store.blocking(&scope).unwrap().is_none());
    assert_eq!(
        store
            .latest_matching(&scope, &intent("unknown"))
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Acknowledged
    );
    assert_eq!(commands(&fixture).len(), 13);
    assert!(commands(&fixture)
        .iter()
        .all(|item| item["matched"] == true));
    assert!(commands(&fixture).iter().all(|item| !item["args"]
        .as_array()
        .unwrap()
        .iter()
        .any(|arg| arg == "POST" || arg == "PATCH" || arg == "DELETE")));
    drop(store);
    assert!(PrWriteJournal::open(&path)
        .unwrap()
        .blocking(&scope)
        .unwrap()
        .is_none());
}

#[test]
fn conversation_comments_journal_old_local_generation_and_account_observations_stay_blocked() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&inspection_steps());
    let client = provider(&fixture, CommandRunner::default());
    let mut store = PrWriteJournal::open(&fixture.repo().join("cache.sqlite3")).unwrap();
    let scope = PrWriteTarget::new(&identity(42), &target()).unwrap();
    let payload = intent("new");
    let permit = store.begin(&scope, &payload).unwrap();
    let inspection = store
        .prepare_inspection(
            &client,
            &identity(42),
            &target(),
            &CancellationToken::default(),
        )
        .unwrap();
    store.claim(&permit, &scope, &payload).unwrap();
    assert!(store
        .acknowledge(
            &client,
            inspection,
            &identity(42),
            &CancellationToken::default()
        )
        .unwrap_err()
        .to_string()
        .contains("changed since inspection"));
    assert!(store.blocking(&scope).unwrap().is_some());
    assert_eq!(
        commands(&fixture).len(),
        11,
        "stale local observations must fail before any provider call"
    );
    let fixture = Fixture::new();
    fixture.scripted_gh(&inspection_steps());
    let client = provider(&fixture, CommandRunner::default());
    let mut store = PrWriteJournal::open(&fixture.repo().join("cache.sqlite3")).unwrap();
    store.begin(&scope, &payload).unwrap();
    let inspection = store
        .prepare_inspection(
            &client,
            &identity(42),
            &target(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(store
        .acknowledge(
            &client,
            inspection,
            &identity(43),
            &CancellationToken::default()
        )
        .unwrap_err()
        .to_string()
        .contains("Account changed"));
    assert!(store.blocking(&scope).unwrap().is_some());
    assert_eq!(commands(&fixture).len(), 11);
}

#[test]
fn conversation_comments_journal_reassigned_fresh_account_cannot_acknowledge_old_private_outcome() {
    let fixture = Fixture::new();
    let mut steps = inspection_steps();
    steps.extend(account(43));
    fixture.scripted_gh(&steps);
    let client = provider(&fixture, CommandRunner::default());
    let mut store = PrWriteJournal::open(&fixture.repo().join("cache.sqlite3")).unwrap();
    let scope = PrWriteTarget::new(&identity(42), &target()).unwrap();
    store.begin(&scope, &intent("new")).unwrap();
    let inspection = store
        .prepare_inspection(
            &client,
            &identity(42),
            &target(),
            &CancellationToken::default(),
        )
        .unwrap();
    assert!(store
        .acknowledge(
            &client,
            inspection,
            &identity(42),
            &CancellationToken::default()
        )
        .unwrap_err()
        .to_string()
        .contains("Account changed before acknowledgement"));
    assert!(store.blocking(&scope).unwrap().is_some());
    assert_eq!(commands(&fixture).len(), 13);
}

#[test]
fn conversation_comments_journal_provider_and_sql_ack_failures_remain_durably_blocked() {
    for fail_sql in [false, true] {
        let fixture = Fixture::new();
        let mut steps = inspection_steps();
        steps.extend(account(42));
        fixture.scripted_gh(&steps);
        let runner = CommandRunner::default();
        let client = provider(&fixture, runner.clone());
        let path = fixture.repo().join("cache.sqlite3");
        let mut store = PrWriteJournal::open(&path).unwrap();
        let scope = PrWriteTarget::new(&identity(42), &target()).unwrap();
        store.begin(&scope, &intent("new")).unwrap();
        let inspection = store
            .prepare_inspection(
                &client,
                &identity(42),
                &target(),
                &CancellationToken::default(),
            )
            .unwrap();
        if fail_sql {
            Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER reject_ack BEFORE UPDATE ON pr_write_attempts BEGIN SELECT RAISE(ABORT,'fixture acknowledgement failure'); END;").unwrap();
        } else {
            runner.mark_ambiguous("github.invalid/example/repo/pull/7");
        }
        assert!(store
            .acknowledge(
                &client,
                inspection,
                &identity(42),
                &CancellationToken::default()
            )
            .is_err());
        drop(store);
        let mut reopened = PrWriteJournal::open(&path).unwrap();
        assert!(reopened.blocking(&scope).unwrap().is_some());
        assert!(reopened.begin(&scope, &intent("different action")).is_err());
        assert_eq!(commands(&fixture).len(), 13);
        assert!(commands(&fixture)
            .iter()
            .all(|item| item["matched"] == true));
    }
}

#[test]
fn conversation_comments_journal_clearing_cache_or_drafts_never_acknowledges_publication() {
    let fixture = Fixture::new();
    let path = fixture.repo().join("cache.sqlite3");
    let scope = PrWriteTarget::new(&identity(42), &target()).unwrap();
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let payload = intent("an interrupted private publication");
    journal.begin(&scope, &payload).unwrap();
    let mut cache = Cache::open(&path).unwrap();
    cache
        .put(
            scope.scope(),
            &Record::new(
                "pr",
                "PR_example/repo_7",
                "example/repo",
                json!({"body":"cached view"}),
            ),
        )
        .unwrap();
    cache
        .put_draft(
            scope.scope(),
            "composer",
            "an interrupted private publication",
        )
        .unwrap();
    cache.clear_records(scope.scope()).unwrap();
    assert!(cache.list(scope.scope(), None).unwrap().is_empty());
    assert!(cache.draft(scope.scope(), "composer").unwrap().is_some());
    assert!(journal.blocking(&scope).unwrap().is_some());
    cache.clear_scope(scope.scope(), true).unwrap();
    assert!(cache.draft(scope.scope(), "composer").unwrap().is_none());
    drop(cache);
    drop(journal);
    let mut reopened = PrWriteJournal::open(&path).unwrap();
    assert_eq!(
        reopened.blocking(&scope).unwrap().unwrap().state,
        PrWriteState::Pending
    );
    assert!(reopened
        .begin(&scope, &intent("different after cache clear"))
        .is_err());
    assert!(commands(&fixture).is_empty());
}
