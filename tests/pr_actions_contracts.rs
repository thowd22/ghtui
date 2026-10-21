#[path = "support/providers.rs"]
mod support;
use ghtui::{
    github_workspace::{GithubWorkspace, PrPage, PrTab},
    pr_actions::{fields, PrActionContext, PrCommand, PrExecution, PrPlan, SelectedComment},
    pr_write_journal::{PrPublicationReference, PrWriteJournal, PrWriteState, PrWriteTarget},
    providers::{
        gh::{
            Actor, CommentKind, CommentReaction, ConversationComment, Coverage, GithubProvider,
            HostIdentity, PrDetails, PrSummary, PullRequest,
        },
        runner::CancellationToken,
    },
    storage::{Cache, Record, Scope, SyncBinding, SyncBindingKind, SyncNamespace},
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    fs,
};
use support::{Fixture, GhStep};

fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
        gh_version: "gh version 2.101.0".into(),
    }
}
fn target(head: &str) -> PrSummary {
    PrSummary {
        pr: PullRequest {
            host: "github.invalid".into(),
            id: "PR_example/repo_7".into(),
            repository: "example/repo".into(),
            number: 7,
            title: "PR 7".into(),
            body: "details".into(),
            url: "https://github.invalid/example/repo/pull/7".into(),
            author: Actor {
                login: "octocat".into(),
                actor_type: "User".into(),
                ..Actor::default()
            },
            state: "open".into(),
            draft: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
            queues: BTreeSet::new(),
        },
        head_sha: head.into(),
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
fn observed() -> ConversationComment {
    ConversationComment {
        id: 9,
        node_id: "IC_9".into(),
        body: "observed body".into(),
        author: Actor {
            login: "octocat".into(),
            actor_type: "User".into(),
            ..Actor::default()
        },
        created_at: "t0".into(),
        updated_at: "t0".into(),
        url: "https://github.invalid/example/repo/pull/7#issuecomment-9".into(),
        card_v115: None,
    }
}
fn action_context(head: &str, selected: bool) -> PrActionContext {
    let mut workspace =
        GithubWorkspace::empty(identity(), vec!["example/repo".into()], BTreeSet::new());
    workspace.pr = Some(PrPage {
        details: PrDetails {
            summary: target(head),
            comments: vec![observed()],
            commits: vec![],
            files: vec![],
            reviews: vec![],
            timeline: vec![],
            coverage: Coverage::default(),
        },
        tab: PrTab::Conversation,
        diff: None,
        checks: None,
        required_checks: None,
        rows: vec![],
        context: None,
        metadata_display_v51: None,
        review_progress: None,
    });
    PrActionContext::from_workspace(
        &workspace,
        selected.then(|| SelectedComment {
            kind: CommentKind::Conversation,
            observed: observed(),
        }),
    )
    .unwrap()
}
fn provider(fixture: &Fixture) -> GithubProvider {
    GithubProvider::new("github.invalid")
        .unwrap()
        .with_program(fixture.root.join("bin/gh"))
        .with_env("GH_FIXTURE_DIR", fixture.root.join("gh-fixture"))
}
fn api(endpoint: &str, method: &str, value: Value, paged: bool) -> GhStep {
    let mut args = vec![
        "api".into(),
        endpoint.into(),
        "--hostname".into(),
        "github.invalid".into(),
        "--method".into(),
        method.into(),
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
fn pr_value(head: &str) -> Value {
    json!({"node_id":"PR_example/repo_7","number":7,"title":"PR 7","body":"details","repository_url":"https://api.github.invalid/repos/example/repo","html_url":"https://github.invalid/example/repo/pull/7","user":{"id":42,"login":"octocat","type":"User"},"state":"open","updated_at":"t0","head":{"sha":head,"ref":"feature"},"base":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"main"}})
}
fn comment_value(id: u64, body: &str) -> Value {
    json!({"id":id,"node_id":format!("IC_{id}"),"body":body,"user":{"id":42,"login":"octocat","type":"User"},"created_at":"t0","updated_at":"t0","issue_url":"https://api.github.invalid/repos/example/repo/issues/7","html_url":format!("https://github.invalid/example/repo/pull/7#issuecomment-{id}")})
}
fn account() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        api("user", "GET", json!({"id":42,"login":"octocat"}), false),
    ]
}
fn guard(head: &str) -> Vec<GhStep> {
    let mut steps = account();
    steps.push(api(
        "repos/example/repo/pulls/7",
        "GET",
        pr_value(head),
        false,
    ));
    steps
}
fn create_steps(head: &str, id: u64, body: &str) -> Vec<GhStep> {
    let mut steps = guard(head);
    let mut write = api(
        "repos/example/repo/issues/7/comments",
        "POST",
        comment_value(id, body),
        false,
    );
    write.args.extend(["--input".into(), "-".into()]);
    write.stdin = serde_json::to_vec(&json!({"body":body})).unwrap();
    steps.push(write);
    steps.push(api(
        &format!("repos/example/repo/issues/comments/{id}"),
        "GET",
        comment_value(id, body),
        false,
    ));
    steps
}
fn refresh(head: &str, comments: Vec<Value>) -> Vec<GhStep> {
    let mut steps = account();
    steps.push(api(
        "repos/example/repo/pulls/7",
        "GET",
        pr_value(head),
        false,
    ));
    steps.push(api(
        "repos/example/repo/issues/7/comments?per_page=100",
        "GET",
        json!([comments]),
        true,
    ));
    for endpoint in [
        "repos/example/repo/pulls/7/commits?per_page=100",
        "repos/example/repo/pulls/7/files?per_page=100",
        "repos/example/repo/pulls/7/reviews?per_page=100",
        "repos/example/repo/issues/7/timeline?per_page=100",
    ] {
        steps.push(api(endpoint, "GET", json!([[]]), true));
    }
    steps.push(api(
        "repos/example/repo/pulls/7",
        "GET",
        pr_value(head),
        false,
    ));
    steps.extend(account());
    steps
}
fn commands(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn writes(fixture: &Fixture) -> usize {
    commands(fixture)
        .iter()
        .filter(|item| {
            item["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST" || arg == "PATCH" || arg == "DELETE" || arg == "PUT")
        })
        .count()
}
fn values(body: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("body".into(), body.into())])
}

fn cached_discussion(path: &std::path::Path) -> (Cache, Scope, String, u64) {
    let mut cache = Cache::open(path).unwrap();
    let scope = Scope::github("github.invalid", Some(42)).unwrap();
    let source = format!("collaboration-v1:{}", "d".repeat(64));
    let token = cache
        .reserve_resource_sync(
            &scope,
            &source,
            &SyncBinding {
                namespace: SyncNamespace::Collaboration,
                repository_id: 9,
                repository: "example/repo".into(),
                resource: "PR_example/repo_7".into(),
                kind: SyncBindingKind::ConversationComment,
            },
        )
        .unwrap();
    assert!(cache
        .commit_reserved_sync(
            &scope,
            &source,
            token,
            &[Record::new(
                "collaboration-document-v1",
                "old-comment",
                "example/repo",
                json!({"body":"old private discussion text"})
            )],
            true
        )
        .unwrap());
    (cache, scope, source, token)
}

#[test]
fn conversation_comments_controller_confirmation_and_invalid_fields_send_zero_writes() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&guard(&"a".repeat(40)));
    let client = provider(&fixture);
    let context = action_context(&"a".repeat(40), false);
    assert!(PrPlan::prepare(
        &client,
        &context,
        PrCommand::NewComment,
        &BTreeMap::from([("shell".into(), "do not execute".into())]),
        &CancellationToken::default()
    )
    .is_err());
    assert!(PrPlan::prepare(
        &client,
        &context,
        PrCommand::NewComment,
        &values(" "),
        &CancellationToken::default()
    )
    .is_err());
    assert!(commands(&fixture).is_empty());
    let plan = PrPlan::prepare(
        &client,
        &context,
        PrCommand::NewComment,
        &values("Unicode 雪\n\u{1b}]0;unsafe\u{7}actual text"),
        &CancellationToken::default(),
    )
    .unwrap();
    assert!(plan.preview.iter().any(|line| line.contains("Unicode 雪")));
    assert!(plan
        .preview
        .iter()
        .all(|line| !line.contains('\u{1b}') && !line.contains("unsafe")));
    let path = fixture.repo().join("cache.sqlite3");
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let callback = Cell::new(false);
    assert!(matches!(
        plan.execute(
            &client,
            &mut journal,
            false,
            None,
            |_| {
                callback.set(true);
                Ok(())
            },
            &CancellationToken::default()
        ),
        PrExecution::Refused { .. }
    ));
    assert!(!callback.get());
    assert!(journal
        .blocking(&PrWriteTarget::new(&identity(), &context.target).unwrap())
        .unwrap()
        .is_none());
    assert_eq!(commands(&fixture).len(), 3);
    assert_eq!(writes(&fixture), 0);
}

#[test]
fn conversation_comments_controller_callback_failure_commits_pending_before_zero_post() {
    let fixture = Fixture::new();
    let mut steps = guard(&"a".repeat(40));
    steps.extend(guard(&"a".repeat(40)));
    fixture.scripted_gh(&steps);
    let client = provider(&fixture);
    let context = action_context(&"a".repeat(40), false);
    let plan = PrPlan::prepare(
        &client,
        &context,
        PrCommand::NewComment,
        &values("private pending text"),
        &CancellationToken::default(),
    )
    .unwrap();
    let path = fixture.repo().join("cache.sqlite3");
    let (cache, cache_scope, source, token) = cached_discussion(&path);
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let callback = Cell::new(false);
    let outcome = plan.execute(
        &client,
        &mut journal,
        true,
        None,
        |marker| {
            callback.set(true);
            let database = Connection::open(&path).unwrap();
            let saved: (i64, String, String) = database
                .query_row(
                    "SELECT attempt_id,state,payload_sha256 FROM pr_write_attempts",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .unwrap();
            assert_eq!(
                saved,
                (
                    marker.attempt_id,
                    "pending".into(),
                    plan.intent().digest().unwrap()
                )
            );
            Err("fixture ordered draft persistence failure".into())
        },
        &CancellationToken::default(),
    );
    assert!(
        matches!(outcome,PrExecution::Refused{error} if error.contains("No GitHub write was sent"))
    );
    assert!(callback.get());
    assert_eq!(commands(&fixture).len(), 6);
    assert_eq!(writes(&fixture), 0);
    assert_eq!(
        cache
            .binding_status(&cache_scope, &source)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(token),
        "pending-before-invocation alone is not a text-change claim"
    );
    drop(journal);
    let mut reopened = PrWriteJournal::open(&path).unwrap();
    assert!(matches!(
        plan.execute(
            &client,
            &mut reopened,
            true,
            None,
            |_| panic!("blocked retry cannot persist another publication"),
            &CancellationToken::default()
        ),
        PrExecution::Refused { .. }
    ));
    assert_eq!(commands(&fixture).len(), 6);
    assert!(reopened
        .blocking(&PrWriteTarget::new(&identity(), &context.target).unwrap())
        .unwrap()
        .is_some());
}

#[test]
fn conversation_comments_controller_uncertain_provider_write_blocks_restart_and_edited_body() {
    let fixture = Fixture::new();
    let mut steps = guard(&"a".repeat(40));
    steps.extend(guard(&"a".repeat(40)));
    let mut create = create_steps(&"a".repeat(40), 9, "attempted body");
    create.pop();
    let failed = create.last_mut().unwrap();
    failed.status = 1;
    failed.stdout.clear();
    failed.stderr = b"fixture publication transport failed after invocation".to_vec();
    steps.extend(create);
    steps.extend(guard(&"a".repeat(40)));
    fixture.scripted_gh(&steps);
    let client = provider(&fixture);
    let context = action_context(&"a".repeat(40), false);
    let plan = PrPlan::prepare(
        &client,
        &context,
        PrCommand::NewComment,
        &values("attempted body"),
        &CancellationToken::default(),
    )
    .unwrap();
    let path = fixture.repo().join("cache.sqlite3");
    let (cache, cache_scope, source, _) = cached_discussion(&path);
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let publication = RefCell::new(None);
    let outcome = plan.execute(
        &client,
        &mut journal,
        true,
        None,
        |marker| {
            let _ = publication.replace(Some(marker.publication_reference()));
            Ok(())
        },
        &CancellationToken::default(),
    );
    assert!(
        matches!(outcome,PrExecution::UncertainWrite{error,..} if error.contains("ordinary refresh is not an acknowledgement"))
    );
    assert_eq!(writes(&fixture), 1);
    assert_eq!(
        cache
            .binding_status(&cache_scope, &source)
            .unwrap()
            .unwrap()
            .visible_generation,
        None,
        "actually invoked unknown text is hidden without pretending confirmation"
    );
    assert_eq!(
        cache
            .get(&cache_scope, "collaboration-document-v1", "old-comment")
            .unwrap()
            .unwrap()
            .payload["body"],
        "old private discussion text",
        "bytes retained for recovery but not search authority"
    );
    let scope = PrWriteTarget::new(&identity(), &context.target).unwrap();
    assert_eq!(
        journal.blocking(&scope).unwrap().unwrap().state,
        PrWriteState::Uncertain
    );
    drop(journal);
    let edited = PrPlan::prepare(
        &client,
        &context,
        PrCommand::NewComment,
        &values("different body"),
        &CancellationToken::default(),
    )
    .unwrap();
    let before = commands(&fixture).len();
    let mut reopened = PrWriteJournal::open(&path).unwrap();
    assert!(matches!(
        edited.execute(
            &client,
            &mut reopened,
            true,
            publication.borrow().as_ref(),
            |_| panic!("blocked restart cannot issue another permit"),
            &CancellationToken::default()
        ),
        PrExecution::Refused { .. }
    ));
    assert_eq!(commands(&fixture).len(), before);
    assert_eq!(writes(&fixture), 1);
    assert!(commands(&fixture)
        .iter()
        .all(|item| item["matched"] == true));
}

#[test]
fn conversation_comments_controller_confirmed_refresh_failure_never_republishes_restored_draft() {
    let fixture = Fixture::new();
    let head = "a".repeat(40);
    let body = "exact submitted Unicode 雪";
    let mut steps = guard(&head);
    steps.extend(guard(&head));
    steps.extend(create_steps(&head, 9, body));
    let mut failed = account();
    failed[1].status = 1;
    failed[1].stderr = b"fixture identity refresh temporarily unavailable".to_vec();
    failed[1].stdout.clear();
    steps.extend(failed);
    fixture.scripted_gh(&steps);
    let client = provider(&fixture);
    let context = action_context(&head, false);
    let plan = PrPlan::prepare(
        &client,
        &context,
        PrCommand::NewComment,
        &values(body),
        &CancellationToken::default(),
    )
    .unwrap();
    let path = fixture.repo().join("cache.sqlite3");
    let (mut cache, cache_scope, source, _) = cached_discussion(&path);
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let reference: RefCell<Option<PrPublicationReference>> = RefCell::new(None);
    let outcome = plan.execute(
        &client,
        &mut journal,
        true,
        None,
        |marker| {
            let _ = reference.replace(Some(marker.publication_reference()));
            Ok(())
        },
        &CancellationToken::default(),
    );
    assert!(matches!(
        outcome,
        PrExecution::ConfirmedButRefreshFailed {
            private_visible: false,
            ..
        }
    ));
    let scope = PrWriteTarget::new(&identity(), &context.target).unwrap();
    assert!(journal.blocking(&scope).unwrap().is_none());
    let confirmed = journal
        .latest_matching(&scope, plan.intent())
        .unwrap()
        .unwrap();
    assert_eq!(confirmed.state, PrWriteState::Confirmed);
    assert_eq!(confirmed.confirmed.unwrap().resource_id, "9");
    assert_eq!(commands(&fixture).len(), 13);
    assert_eq!(writes(&fixture), 1);
    assert_eq!(
        cache
            .binding_status(&cache_scope, &source)
            .unwrap()
            .unwrap()
            .visible_generation,
        None,
        "genuine receipt hides old text before a failing private refresh"
    );
    let binding = cache
        .binding_status(&cache_scope, &source)
        .unwrap()
        .unwrap()
        .binding;
    let fresh = cache
        .reserve_resource_sync(&cache_scope, &source, &binding)
        .unwrap();
    assert!(cache
        .commit_reserved_sync(
            &cache_scope,
            &source,
            fresh,
            &[Record::new(
                "collaboration-document-v1",
                "old-comment",
                "example/repo",
                json!({"body":body})
            )],
            true
        )
        .unwrap());
    drop(journal);

    let second = Fixture::new();
    let changed = "c".repeat(40);
    let mut next_steps = guard(&changed);
    next_steps.extend(guard(&changed));
    next_steps.extend(create_steps(&changed, 10, body));
    next_steps.extend(refresh(
        &changed,
        vec![comment_value(9, body), comment_value(10, body)],
    ));
    second.scripted_gh(&next_steps);
    let second_client = provider(&second);
    let refreshed_context = action_context(&changed, false);
    let restored_plan = PrPlan::prepare(
        &second_client,
        &refreshed_context,
        PrCommand::NewComment,
        &values(body),
        &CancellationToken::default(),
    )
    .unwrap();
    let mut reopened = PrWriteJournal::open(&path).unwrap();
    assert!(matches!(
        restored_plan.execute(
            &second_client,
            &mut reopened,
            true,
            reference.borrow().as_ref(),
            |_| panic!("confirmed restored draft must not publish"),
            &CancellationToken::default()
        ),
        PrExecution::RestoredAlreadyConfirmed { .. }
    ));
    assert_eq!(commands(&second).len(), 3);
    assert_eq!(writes(&second), 0);
    assert_eq!(
        cache
            .binding_status(&cache_scope, &source)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(fresh),
        "restored original confirmed receipt cannot hide newer explicit hydration"
    );
    let fresh_reference = RefCell::new(None);
    let outcome = restored_plan.execute(
        &second_client,
        &mut reopened,
        true,
        None,
        |marker| {
            let _ = fresh_reference.replace(Some(marker.publication_reference()));
            Ok(())
        },
        &CancellationToken::default(),
    );
    let PrExecution::Confirmed {
        workspace,
        publication,
        ..
    } = outcome
    else {
        panic!("deliberate fresh identical message must be permitted: {outcome:?}");
    };
    assert_ne!(
        publication.attempt_id,
        reference.borrow().as_ref().unwrap().attempt_id
    );
    assert_eq!(fresh_reference.borrow().as_ref(), Some(&publication));
    assert_eq!(workspace.pr.as_ref().unwrap().tab, PrTab::Conversation);
    assert_eq!(workspace.pr.as_ref().unwrap().details.comments.len(), 2);
    assert_eq!(
        workspace.pr.as_ref().unwrap().details.summary.head_sha,
        changed
    );
    assert!(workspace.pr.as_ref().unwrap().rows.iter().any(|row| row
        .comment
        .as_ref()
        .is_some_and(|comment| comment.observed.id == 10)));
    assert_eq!(commands(&second).len(), 22);
    assert_eq!(writes(&second), 1);
    assert!(commands(&second).iter().all(|item| item["matched"] == true));
}

#[test]
fn conversation_comments_controller_confirmed_sql_failure_keeps_pending_receipt_without_retry() {
    let fixture = Fixture::new();
    let head = "a".repeat(40);
    let body = "confirmed remote text";
    let mut steps = guard(&head);
    steps.extend(guard(&head));
    steps.extend(create_steps(&head, 9, body));
    fixture.scripted_gh(&steps);
    let client = provider(&fixture);
    let context = action_context(&head, false);
    let plan = PrPlan::prepare(
        &client,
        &context,
        PrCommand::NewComment,
        &values(body),
        &CancellationToken::default(),
    )
    .unwrap();
    let path = fixture.repo().join("cache.sqlite3");
    let (cache, cache_scope, source, _) = cached_discussion(&path);
    let mut journal = PrWriteJournal::open(&path).unwrap();
    Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER reject_confirm BEFORE UPDATE OF state ON pr_write_attempts WHEN NEW.state='confirmed' BEGIN SELECT RAISE(ABORT,'fixture SQL confirmation failure'); END;").unwrap();
    assert!(
        matches!(plan.execute(&client,&mut journal,true,None,|_|Ok(()),&CancellationToken::default()),PrExecution::ConfirmedButJournalFailed{receipt,..} if receipt.resource_id == "9")
    );
    assert_eq!(writes(&fixture), 1);
    assert_eq!(commands(&fixture).len(), 11);
    assert_eq!(
        cache
            .binding_status(&cache_scope, &source)
            .unwrap()
            .unwrap()
            .visible_generation,
        None,
        "failed durable receipt still conservatively fences possible changed text"
    );
    let scope = PrWriteTarget::new(&identity(), &context.target).unwrap();
    assert_eq!(
        journal.blocking(&scope).unwrap().unwrap().state,
        PrWriteState::Pending
    );
    drop(journal);
    let mut reopened = PrWriteJournal::open(&path).unwrap();
    assert!(matches!(
        plan.execute(
            &client,
            &mut reopened,
            true,
            None,
            |_| panic!("pending confirmation must stay blocked"),
            &CancellationToken::default()
        ),
        PrExecution::Refused { .. }
    ));
    assert_eq!(writes(&fixture), 1);
    assert_eq!(commands(&fixture).len(), 11);
}

#[test]
fn conversation_comments_controller_reactions_have_typed_ids_labels_and_stale_comment_guards() {
    let fixture = Fixture::new();
    let head = "a".repeat(40);
    let raw_reactions = json!([[{"id":91,"node_id":"REACT_91","content":"heart","user":{"id":42,"login":"octocat","type":"User"}},{"id":92,"node_id":"REACT_92","content":"+1","user":{"id":43,"login":"octocat","type":"User"}}]]);
    let mut steps = guard(&head);
    steps.push(api(
        "repos/example/repo/issues/comments/9",
        "GET",
        comment_value(9, "observed body"),
        false,
    ));
    steps.push(api(
        "repos/example/repo/issues/comments/9",
        "GET",
        comment_value(9, "observed body"),
        false,
    ));
    steps.push(api(
        "repos/example/repo/issues/comments/9/reactions?per_page=100",
        "GET",
        raw_reactions,
        true,
    ));
    steps.push(api(
        "repos/example/repo/issues/comments/9",
        "GET",
        comment_value(9, "observed body"),
        false,
    ));
    steps.push(api(
        "repos/example/repo/pulls/7",
        "GET",
        pr_value(&head),
        false,
    ));
    steps.extend(account());
    steps.extend(guard(&head));
    let mut changed = comment_value(9, "server concurrent body");
    changed["updated_at"] = json!("t1");
    steps.push(api(
        "repos/example/repo/issues/comments/9",
        "GET",
        changed,
        false,
    ));
    fixture.scripted_gh(&steps);
    let client = provider(&fixture);
    let mut context = action_context(&head, true);
    context
        .load_reactions(&client, &CancellationToken::default())
        .unwrap();
    let reaction_fields = fields(PrCommand::RemoveReaction, &context);
    assert_eq!(reaction_fields[0].choices.len(), 1);
    assert_eq!(reaction_fields[0].choices[0].value, "91");
    assert!(reaction_fields[0].choices[0]
        .label
        .contains("heart • @octocat"));
    assert!(
        PrPlan::prepare(
            &client,
            &context,
            PrCommand::RemoveReaction,
            &BTreeMap::from([(
                "reaction_id".into(),
                reaction_fields[0].choices[0].label.clone()
            )]),
            &CancellationToken::default()
        )
        .is_err(),
        "display labels must never become target IDs"
    );
    assert!(
        PrPlan::prepare(
            &client,
            &context,
            PrCommand::RemoveReaction,
            &BTreeMap::from([("reaction_id".into(), "92".into())]),
            &CancellationToken::default()
        )
        .is_err(),
        "same username with a different numeric account does not grant ownership"
    );
    assert_eq!(commands(&fixture).len(), 10);
    assert!(PrPlan::prepare(
        &client,
        &context,
        PrCommand::EditComment,
        &values("user pending edit"),
        &CancellationToken::default()
    )
    .unwrap_err()
    .contains("Comment changed"));
    assert_eq!(
        context.comment.as_ref().unwrap().observed.body,
        "observed body"
    );
    assert_eq!(commands(&fixture).len(), 14);
    assert_eq!(writes(&fixture), 0);
    let other = CommentReaction {
        id: 93,
        node_id: "REACT_93".into(),
        content: "eyes".into(),
        actor: Actor::default(),
        account_id: Some(43),
        created_at: String::new(),
    };
    context.reactions = vec![other];
    assert!(PrCommand::RemoveReaction
        .disabled_reason(&context)
        .is_some());
}

#[test]
fn conversation_comments_collaboration_index_controller_visibility_failure_keeps_confirmed_receipt_and_no_resend(
) {
    let fixture = Fixture::new();
    let head = "a".repeat(40);
    let body = "confirmed effect despite failed cache visibility";
    let mut steps = guard(&head);
    steps.extend(guard(&head));
    steps.extend(create_steps(&head, 9, body));
    fixture.scripted_gh(&steps);
    let client = provider(&fixture);
    let context = action_context(&head, false);
    let plan = PrPlan::prepare(
        &client,
        &context,
        PrCommand::NewComment,
        &values(body),
        &CancellationToken::default(),
    )
    .unwrap();
    let path = fixture.repo().join("cache.sqlite3");
    let (cache, cache_scope, source, token) = cached_discussion(&path);
    let mut journal = PrWriteJournal::open(&path).unwrap();
    Connection::open(&path).unwrap().execute_batch("CREATE TRIGGER reject_visibility BEFORE UPDATE OF visible_generation ON sync_resource_bindings BEGIN SELECT RAISE(ABORT,'fixture cache visibility failure'); END;").unwrap();
    let reference = RefCell::new(None);
    let outcome = plan.execute(
        &client,
        &mut journal,
        true,
        None,
        |marker| {
            reference.replace(Some(marker.publication_reference()));
            Ok(())
        },
        &CancellationToken::default(),
    );
    assert!(
        matches!(outcome,PrExecution::ConfirmedButRefreshFailed{private_visible:false,error,..} if error.contains("discussion-cache visibility failed before refresh")&&error.contains("do not resend"))
    );
    assert_eq!(writes(&fixture), 1);
    assert_eq!(
        commands(&fixture).len(),
        11,
        "cache failure is reported before initiating private view refresh"
    );
    let target = PrWriteTarget::new(&identity(), &context.target).unwrap();
    assert_eq!(
        journal
            .latest_matching(&target, plan.intent())
            .unwrap()
            .unwrap()
            .state,
        PrWriteState::Confirmed
    );
    assert!(
        journal.blocking(&target).unwrap().is_none(),
        "a confirmed receipt is never relabelled uncertain"
    );
    assert_eq!(
        cache
            .binding_status(&cache_scope, &source)
            .unwrap()
            .unwrap()
            .visible_generation,
        Some(token),
        "SQL rollback preserves recovery bytes and requires a visibility retry, not resend"
    );
    drop(journal);
    let mut restarted = PrWriteJournal::open(&path).unwrap();
    assert!(matches!(
        plan.execute(
            &client,
            &mut restarted,
            true,
            reference.borrow().as_ref(),
            |_| panic!("restored confirmation never republishes"),
            &CancellationToken::default()
        ),
        PrExecution::ConfirmedButRefreshFailed {
            private_visible: false,
            ..
        }
    ));
    assert_eq!(commands(&fixture).len(), 11);
    Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TRIGGER reject_visibility;")
        .unwrap();
    assert!(matches!(
        plan.execute(
            &client,
            &mut restarted,
            true,
            reference.borrow().as_ref(),
            |_| panic!("visibility recovery is read-only locally"),
            &CancellationToken::default()
        ),
        PrExecution::RestoredAlreadyConfirmed { .. }
    ));
    assert_eq!(
        cache
            .binding_status(&cache_scope, &source)
            .unwrap()
            .unwrap()
            .visible_generation,
        None
    );
    assert_eq!(writes(&fixture), 1);
    assert_eq!(commands(&fixture).len(), 11);
}
