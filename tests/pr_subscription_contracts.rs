#[path = "support/providers.rs"]
mod support;
use ghtui::{
    github_workspace::{GithubWorkspace, PrPage, PrTab},
    pr_actions::{
        fields, PrActionContext, PrCommand, PrExecution, PrPlan, PrReactionPage, SelectedComment,
    },
    pr_write_journal::{PrWriteIntent, PrWriteJournal, PrWriteTarget},
    providers::{
        gh::{
            CommentKind, CurrentPrSubscription, GithubProvider, HostIdentity, PrDetails, PrSummary,
            Subscription, PR_CURRENT_SUBSCRIPTION_QUERY,
        },
        runner::CancellationToken,
    },
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
};
use support::{Fixture, GhStep};

fn identity(id: u64) -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(id),
        gh_version: "gh version 2.101.0".into(),
    }
}
fn summary() -> PrSummary {
    serde_json::from_value(json!({"pr":{"host":"github.invalid","id":"PR_example/repo_7","repository":"example/repo","number":7,"title":"Subscription fixture","body":"Description","url":"https://github.invalid/example/repo/pull/7","author":{"login":"octocat","actor_type":"User"},"state":"open","draft":false,"created_at":"t0","updated_at":"t0","queues":[]},"head_sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","base_sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","head_ref":"feature","base_ref":"main","merged":false,"auto_merge":false,"merge_commit_sha":null,"mergeable":true,"mergeable_state":"clean","additions":0,"deletions":0,"changed_files":0})).unwrap()
}
fn pr() -> Value {
    json!({"node_id":"PR_example/repo_7","number":7,"title":"Subscription fixture","body":"Description","repository_url":"https://api.github.invalid/repos/example/repo","html_url":"https://github.invalid/example/repo/pull/7","user":{"id":42,"login":"octocat","type":"User"},"state":"open","created_at":"t0","updated_at":"t0","head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","ref":"feature"},"base":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"main"},"changed_files":0})
}
fn comment(body: &str, updated: &str) -> Value {
    json!({"id":10,"node_id":"IC_10","body":body,"user":{"id":42,"login":"octocat","type":"User"},"created_at":"t0","updated_at":updated,"issue_url":"https://api.github.invalid/repos/example/repo/issues/7","html_url":"https://github.invalid/example/repo/pull/7#issuecomment-10"})
}
fn context(selected: bool) -> PrActionContext {
    let mut workspace =
        GithubWorkspace::empty(identity(42), vec!["example/repo".into()], BTreeSet::new());
    let observed = serde_json::from_value(json!({"id":10,"node_id":"IC_10","body":"Selected original body","author":{"login":"octocat","actor_type":"User"},"created_at":"t0","updated_at":"t0","url":"https://github.invalid/example/repo/pull/7#issuecomment-10"})).unwrap();
    workspace.pr = Some(PrPage {
        details: PrDetails {
            summary: summary(),
            comments: vec![observed],
            commits: vec![],
            files: vec![],
            reviews: vec![],
            timeline: vec![],
            coverage: Default::default(),
        },
        tab: PrTab::Conversation,
        diff: None,
        checks: None,
        rows: vec![],
        context: None,
        metadata_display_v51: None,
        review_progress: None,
    });
    let selected = selected.then(|| SelectedComment {
        kind: CommentKind::Conversation,
        observed: workspace.pr.as_ref().unwrap().details.comments[0].clone(),
    });
    PrActionContext::from_workspace(&workspace, selected).unwrap()
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
fn account(id: u64) -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        api("user", "GET", json!({"id":id,"login":"octocat"}), false),
    ]
}
fn guard() -> Vec<GhStep> {
    let mut steps = account(42);
    steps.push(api("repos/example/repo/pulls/7", "GET", pr(), false));
    steps
}
fn subscription_result(state: Value, permission: bool) -> Value {
    json!({"data":{"node":{"__typename":"PullRequest","id":"PR_example/repo_7","number":7,"headRefOid":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","baseRefOid":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","repository":{"nameWithOwner":"example/repo"},"viewerSubscription":state,"viewerCanSubscribe":permission}}})
}
fn current(state: Value, permission: bool) -> Vec<GhStep> {
    let mut steps = guard();
    let mut query = api(
        "graphql",
        "POST",
        subscription_result(state, permission),
        false,
    );
    query.args.extend(["--input".into(), "-".into()]);
    query.stdin = serde_json::to_vec(
        &json!({"query":PR_CURRENT_SUBSCRIPTION_QUERY,"variables":{"id":"PR_example/repo_7"}}),
    )
    .unwrap();
    steps.push(query);
    steps.push(api("repos/example/repo/pulls/7", "GET", pr(), false));
    steps.extend(account(42));
    steps
}
fn journal(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn assert_reads(fixture: &Fixture, count: usize) {
    let commands = journal(fixture);
    assert_eq!(commands.len(), count);
    assert!(commands.iter().all(|command| command["matched"] == true));
    for command in commands {
        let args = command["args"].as_array().unwrap();
        if args.get(1).is_some_and(|endpoint| endpoint == "graphql") {
            let hex = command["stdin_hex"].as_str().unwrap();
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16).unwrap())
                .collect::<Vec<_>>();
            let body: Value = serde_json::from_slice(&bytes).unwrap();
            assert!(body["query"]
                .as_str()
                .unwrap()
                .trim_start()
                .starts_with("query"));
        } else {
            assert!(!args
                .iter()
                .any(|arg| arg == "POST" || arg == "PATCH" || arg == "DELETE" || arg == "PUT"));
        }
    }
}

#[test]
fn conversation_comments_current_subscription_known_nullable_and_denied_are_readonly() {
    for (state, label, permission) in [
        (json!("SUBSCRIBED"), "subscribed", true),
        (json!("UNSUBSCRIBED"), "unsubscribed", true),
        (json!("IGNORED"), "ignored", true),
        (Value::Null, "unavailable (null)", true),
        (json!("UNSUBSCRIBED"), "unsubscribed", false),
    ] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&current(state, permission));
        let value = provider(&fixture)
            .current_subscription(&identity(42), &summary(), &CancellationToken::default())
            .unwrap();
        assert_eq!(value.state_label(), label);
        assert_eq!(value.viewer_can_subscribe, permission);
        assert_reads(&fixture, 7);
        let mut context = context(false);
        context.current_subscription = Some(value);
        let input = fields(PrCommand::Subscription, &context);
        assert_eq!(input[0].label, "Requested subscription state");
        assert_eq!(
            input[0].value,
            if label == "unavailable (null)" {
                ""
            } else {
                label
            }
        );
        if !permission {
            let reason = PrCommand::Subscription.disabled_reason(&context).unwrap();
            assert!(reason.contains("Current verified subscription: unsubscribed"));
            assert!(reason.contains("viewerCanSubscribe=false"));
        }
    }
    let fixture = Fixture::new();
    let mut unknown = identity(42);
    unknown.account_id = None;
    assert!(provider(&fixture)
        .current_subscription(&unknown, &summary(), &CancellationToken::default())
        .is_err());
    unknown.account_id = Some(0);
    assert!(provider(&fixture)
        .current_subscription(&unknown, &summary(), &CancellationToken::default())
        .is_err());
    assert_reads(&fixture, 0);
}

#[test]
fn conversation_comments_current_subscription_rejects_malformed_or_redirected_node() {
    let base = subscription_result(json!("SUBSCRIBED"), true);
    for (key, value) in [
        ("__typename", json!("Issue")),
        ("id", json!("PR_elsewhere")),
        ("number", json!(8)),
        ("headRefOid", json!("c".repeat(40))),
        ("baseRefOid", json!("c".repeat(40))),
        ("repository", json!({"nameWithOwner":"other/repo"})),
        ("viewerCanSubscribe", Value::Null),
        ("viewerSubscription", json!("FUTURE")),
        ("viewerSubscription", json!(5)),
    ] {
        let fixture = Fixture::new();
        let mut steps = current(json!("SUBSCRIBED"), true);
        let mut result = base.clone();
        result["data"]["node"][key] = value;
        steps[3].stdout = serde_json::to_vec(&result).unwrap();
        fixture.scripted_gh(&steps);
        assert!(provider(&fixture)
            .current_subscription(&identity(42), &summary(), &CancellationToken::default())
            .is_err());
        assert_reads(&fixture, 4);
    }
    for missing in ["viewerSubscription", "viewerCanSubscribe"] {
        let fixture = Fixture::new();
        let mut steps = current(json!("SUBSCRIBED"), true);
        let mut result = base.clone();
        result["data"]["node"]
            .as_object_mut()
            .unwrap()
            .remove(missing);
        steps[3].stdout = serde_json::to_vec(&result).unwrap();
        fixture.scripted_gh(&steps);
        assert!(provider(&fixture)
            .current_subscription(&identity(42), &summary(), &CancellationToken::default())
            .is_err());
        assert_reads(&fixture, 4);
    }
}

#[test]
fn conversation_comments_current_subscription_rejects_account_and_head_races() {
    for (index, response, count) in [
        (1, json!({"id":43,"login":"octocat"}), 2),
        (6, json!({"id":43,"login":"octocat"}), 7),
        (
            4,
            {
                let mut value = pr();
                value["head"]["sha"] = json!("c".repeat(40));
                value
            },
            5,
        ),
        (
            2,
            {
                let mut value = pr();
                value["node_id"] = json!("PR_other");
                value
            },
            3,
        ),
    ] {
        let fixture = Fixture::new();
        let mut steps = current(json!("SUBSCRIBED"), true);
        steps[index].stdout = serde_json::to_vec(&response).unwrap();
        fixture.scripted_gh(&steps);
        assert!(provider(&fixture)
            .current_subscription(&identity(42), &summary(), &CancellationToken::default())
            .is_err());
        assert_reads(&fixture, count);
    }
}

#[test]
fn conversation_comments_subscription_state_and_permission_races_refuse_before_journal_begin() {
    for (state, permission) in [(json!("UNSUBSCRIBED"), true), (json!("SUBSCRIBED"), false)] {
        let fixture = Fixture::new();
        fixture.scripted_gh(&current(state, permission));
        let mut context = context(false);
        context.current_subscription = Some(CurrentPrSubscription {
            state: Some(Subscription::Subscribed),
            viewer_can_subscribe: true,
        });
        assert!(PrPlan::prepare(
            &provider(&fixture),
            &context,
            PrCommand::Subscription,
            &BTreeMap::from([("subscription".into(), "ignored".into())]),
            &CancellationToken::default()
        )
        .is_err());
        assert_reads(&fixture, 7);
    }
    let fixture = Fixture::new();
    let mut steps = current(json!("SUBSCRIBED"), true);
    steps.extend(current(json!("UNSUBSCRIBED"), true));
    fixture.scripted_gh(&steps);
    let client = provider(&fixture);
    let context = context(false);
    let plan = PrPlan::prepare(
        &client,
        &context,
        PrCommand::Subscription,
        &BTreeMap::from([("subscription".into(), "ignored".into())]),
        &CancellationToken::default(),
    )
    .unwrap();
    let database = fixture.repo().join("cache.sqlite3");
    let mut store = PrWriteJournal::open(&database).unwrap();
    let outcome = plan.execute(
        &client,
        &mut store,
        true,
        None,
        |_| panic!("no permit or callback before stale-current refusal"),
        &CancellationToken::default(),
    );
    assert!(matches!(outcome, PrExecution::Refused { .. }));
    assert_reads(&fixture, 14);
    assert_eq!(
        Connection::open(database)
            .unwrap()
            .query_row("SELECT COUNT(*) FROM pr_write_attempts", [], |row| row
                .get::<_, u64>(0))
            .unwrap(),
        0
    );
}

fn reaction(id: u64, account: Option<u64>, login: Option<&str>, kind: Option<&str>) -> Value {
    json!({"id":id,"node_id":format!("REACT_{id}"),"content":"heart","user":login.map(|login|json!({"id":account,"login":login,"type":kind})),"created_at":if id == 504 {""} else {"t1"}})
}
fn reaction_reads() -> Vec<GhStep> {
    let mut steps = guard();
    for _ in 0..2 {
        steps.push(api(
            "repos/example/repo/issues/comments/10",
            "GET",
            comment("Fresh reaction comment", "t1"),
            false,
        ));
    }
    steps.push(api(
        "repos/example/repo/issues/comments/10/reactions?per_page=100",
        "GET",
        json!([
            [
                reaction(501, Some(42), Some("octocat"), Some("User")),
                reaction(502, Some(43), Some("octocat"), Some("User"))
            ],
            [
                reaction(503, Some(99), Some("ci-bot"), Some("Bot")),
                reaction(504, None, None, None)
            ]
        ]),
        true,
    ));
    steps.push(api(
        "repos/example/repo/issues/comments/10",
        "GET",
        comment("Fresh reaction comment", "t1"),
        false,
    ));
    steps.push(api("repos/example/repo/pulls/7", "GET", pr(), false));
    steps.extend(account(42));
    steps
}

#[test]
fn conversation_comments_reaction_page_browses_all_actor_pages_without_ack_or_write() {
    let fixture = Fixture::new();
    fixture.scripted_gh(&reaction_reads());
    let mut context = context(true);
    let database = fixture.repo().join("cache.sqlite3");
    let mut store = PrWriteJournal::open(&database).unwrap();
    let target = PrWriteTarget::new(&context.identity, &context.target).unwrap();
    let permit = store
        .begin(
            &target,
            &PrWriteIntent {
                command: "new-comment".into(),
                selected_resource: "pr".into(),
                observed_head: "a".repeat(40),
                values: BTreeMap::from([("body".into(), "uncertain publication".into())]),
            },
        )
        .unwrap();
    context
        .load_reactions(&provider(&fixture), &CancellationToken::default())
        .unwrap();
    let page = PrReactionPage::from_context(&context).unwrap();
    assert_reads(&fixture, 10);
    assert!(page.complete);
    assert!(page.changed_comment);
    assert!(page.fetched_at > 0);
    assert_eq!(page.comment.observed.body, "Fresh reaction comment");
    assert_eq!(
        page.reactions
            .iter()
            .map(|reaction| reaction.id)
            .collect::<Vec<_>>(),
        vec![501, 502, 503, 504]
    );
    assert!(page.is_yours(&page.reactions[0]));
    assert!(!page.is_yours(&page.reactions[1]));
    assert_eq!(page.reactions[2].actor.actor_type, "Bot");
    assert_eq!(page.reactions[3].actor.label(), "unknown actor");
    assert_eq!(page.reactions[3].created_at, "");
    assert_eq!(
        store.blocking(&target).unwrap().unwrap().attempt_id,
        permit.marker().attempt_id
    );
    assert_eq!(
        Connection::open(database)
            .unwrap()
            .query_row("SELECT state FROM pr_write_attempts", [], |row| row
                .get::<_, String>(0))
            .unwrap(),
        "pending"
    );
}

#[test]
fn conversation_comments_reaction_page_errors_never_publish_empty_success_or_stale_private_rows() {
    for (index, response, count) in [
        (1, json!({"id":43,"login":"octocat"}), 2),
        (
            2,
            {
                let mut value = pr();
                value["head"]["sha"] = json!("c".repeat(40));
                value
            },
            3,
        ),
        (
            7,
            {
                let mut value = pr();
                value["node_id"] = json!("PR_other");
                value
            },
            8,
        ),
        (9, json!({"id":43,"login":"octocat"}), 10),
    ] {
        let fixture = Fixture::new();
        let mut steps = reaction_reads();
        steps[index].stdout = serde_json::to_vec(&response).unwrap();
        fixture.scripted_gh(&steps);
        let mut context = context(true);
        context.reactions_complete = true;
        context.reactions_fetched_at = 1;
        assert!(context
            .load_reactions(&provider(&fixture), &CancellationToken::default())
            .is_err());
        assert_reads(&fixture, count);
        assert!(!context.reactions_complete);
        assert_eq!(context.reactions_fetched_at, 0);
        assert!(context.reactions.is_empty());
        assert!(PrReactionPage::from_context(&context).is_err());
    }
    let fixture = Fixture::new();
    let mut steps = reaction_reads();
    steps[1].stdout.clear();
    steps[1].stderr = b"HTTP 401: authentication failed".to_vec();
    steps[1].status = 1;
    fixture.scripted_gh(&steps);
    let mut context = context(true);
    let error = context
        .load_reactions(&provider(&fixture), &CancellationToken::default())
        .unwrap_err();
    assert!(error.starts_with("Account identity unavailable:"));
    assert_reads(&fixture, 2);
    assert!(PrReactionPage::from_context(&context).is_err());
}
