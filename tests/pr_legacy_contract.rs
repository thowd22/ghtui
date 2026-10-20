#[path = "support/providers.rs"]
mod support;
use ghtui::{
    forms::{PrFormKind, PrIntent},
    github_workspace::{GithubWorkspace, PrPage, PrTab},
    pr_actions::PrExecution,
    pr_legacy::execute_legacy,
    pr_write_journal::{PrPublicationReference, PrWriteJournal, PrWriteTarget},
    providers::{
        gh::{Actor, Coverage, GithubProvider, HostIdentity, PrDetails, PrSummary, PullRequest},
        runner::CancellationToken,
    },
    storage::{Cache, Record, Scope, SyncBinding, SyncBindingKind, SyncNamespace},
};
use rusqlite::Connection;
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fs,
};
use support::{Fixture, GhStep};

#[path = "pr_legacy_contract/metadata.rs"]
mod metadata;

fn identity() -> HostIdentity {
    HostIdentity {
        host: "github.invalid".into(),
        login: "octocat".into(),
        account_id: Some(42),
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
            title: "Original title".into(),
            body: "Original details".into(),
            url: "https://github.invalid/example/repo/pull/7".into(),
            author: Actor {
                login: "alice".into(),
                actor_type: "User".into(),
                ..Actor::default()
            },
            state: "open".into(),
            draft: false,
            created_at: "t0".into(),
            updated_at: "t0".into(),
            queues: BTreeSet::new(),
        },
        head_sha: "a".repeat(40),
        base_sha: "b".repeat(40),
        head_ref: "topic".into(),
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
fn workspace() -> GithubWorkspace {
    let mut workspace =
        GithubWorkspace::empty(identity(), vec!["example/repo".into()], BTreeSet::new());
    workspace.pr = Some(PrPage {
        details: PrDetails {
            summary: target(),
            comments: vec![],
            commits: vec![],
            files: vec![],
            reviews: vec![],
            timeline: vec![],
            coverage: Coverage::default(),
        },
        tab: PrTab::Summary,
        diff: None,
        checks: None,
        required_checks: None,
        rows: vec![],
        context: None,
        metadata_display_v51: None,
        review_progress: None,
    });
    workspace
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
fn pr() -> Value {
    json!({"node_id":"PR_example/repo_7","number":7,"title":"Original title","body":"Original details","repository_url":"https://api.github.invalid/repos/example/repo","html_url":"https://github.invalid/example/repo/pull/7","user":{"login":"alice","id":43,"type":"User"},"state":"open","updated_at":"t0","head":{"sha":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","ref":"topic"},"base":{"sha":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","ref":"main"}})
}
fn identity_steps() -> Vec<GhStep> {
    vec![
        GhStep::read(&["--version"], "gh version 2.101.0\n"),
        api("user", "GET", json!({"id":42,"login":"octocat"}), false),
    ]
}
fn guard() -> Vec<GhStep> {
    let mut steps = identity_steps();
    steps.push(api("repos/example/repo/pulls/7", "GET", pr(), false));
    steps
}
fn write(endpoint: &str, method: &str, payload: Value, result: Value) -> GhStep {
    let mut step = api(endpoint, method, result, false);
    step.args.extend(["--input".into(), "-".into()]);
    step.stdin = serde_json::to_vec(&payload).unwrap();
    step
}
fn body(kind: PrFormKind) -> String {
    match kind {
        PrFormKind::Approve | PrFormKind::Description => String::new(),
        PrFormKind::Title => "Edited title 雪".into(),
        PrFormKind::Base => "release/雪".into(),
        _ => "Legacy submitted body 雪".into(),
    }
}
fn successful_mutation(kind: PrFormKind, body: &str) -> (Vec<GhStep>, Value) {
    let mut steps = guard();
    let mut fresh = pr();
    match kind {
        PrFormKind::Comment => {
            let raw = json!({"id":10,"node_id":"IC_10","body":body,"user":{"id":42,"login":"octocat","type":"User"},"created_at":"t0","updated_at":"t0","issue_url":"https://api.github.invalid/repos/example/repo/issues/7"});
            steps.push(write(
                "repos/example/repo/issues/7/comments",
                "POST",
                json!({"body":body}),
                raw.clone(),
            ));
            steps.push(api(
                "repos/example/repo/issues/comments/10",
                "GET",
                raw,
                false,
            ));
        }
        PrFormKind::Approve | PrFormKind::RequestChanges | PrFormKind::ReviewComment => {
            let (event, state) = match kind {
                PrFormKind::Approve => ("APPROVE", "APPROVED"),
                PrFormKind::RequestChanges => ("REQUEST_CHANGES", "CHANGES_REQUESTED"),
                _ => ("COMMENT", "COMMENTED"),
            };
            let raw = json!({"id":600,"node_id":"PRR_600","body":body,"state":state,"commit_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","user":{"id":42,"login":"octocat","type":"User"},"pull_request_url":"https://api.github.invalid/repos/example/repo/pulls/7"});
            steps.push(write("repos/example/repo/pulls/7/reviews","POST",json!({"event":event,"body":body,"commit_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}),raw.clone()));
            steps.push(api(
                "repos/example/repo/pulls/7/reviews/600",
                "GET",
                raw,
                false,
            ));
        }
        PrFormKind::Title | PrFormKind::Description | PrFormKind::Base => {
            let field = match kind {
                PrFormKind::Title => "title",
                PrFormKind::Description => "body",
                _ => "base",
            };
            let payload = json!({field:body});
            if kind == PrFormKind::Base {
                fresh["base"]["ref"] = json!(body);
            } else {
                fresh[field] = json!(body);
            }
            steps.push(write(
                "repos/example/repo/pulls/7",
                "PATCH",
                payload,
                fresh.clone(),
            ));
            steps.push(api(
                "repos/example/repo/pulls/7",
                "GET",
                fresh.clone(),
                false,
            ));
        }
    }
    (steps, fresh)
}
fn inspection(fresh: Value) -> Vec<GhStep> {
    let mut steps = identity_steps();
    steps.push(api(
        "repos/example/repo/pulls/7",
        "GET",
        fresh.clone(),
        false,
    ));
    for endpoint in [
        "repos/example/repo/issues/7/comments?per_page=100",
        "repos/example/repo/pulls/7/commits?per_page=100",
        "repos/example/repo/pulls/7/files?per_page=100",
        "repos/example/repo/pulls/7/reviews?per_page=100",
        "repos/example/repo/issues/7/timeline?per_page=100",
    ] {
        steps.push(api(endpoint, "GET", json!([[]]), true));
    }
    steps.push(api("repos/example/repo/pulls/7", "GET", fresh, false));
    steps.extend(identity_steps());
    steps
}
fn commands(fixture: &Fixture) -> Vec<Value> {
    fs::read_to_string(fixture.root.join("gh-fixture/commands.jsonl"))
        .unwrap_or_default()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn write_count(fixture: &Fixture) -> usize {
    commands(fixture)
        .iter()
        .filter(|row| {
            row["args"]
                .as_array()
                .unwrap()
                .iter()
                .any(|arg| arg == "POST" || arg == "PATCH" || arg == "DELETE")
        })
        .count()
}

#[test]
fn conversation_comments_legacy_all_seven_forms_commit_exact_receipts_before_refresh() {
    for (kind, _) in PrFormKind::ALL {
        let fixture = Fixture::new();
        let client = provider(&fixture);
        let body = body(kind);
        let intent = PrIntent {
            kind,
            target: target(),
            identity: identity(),
        };
        let mut steps = if kind == PrFormKind::Comment {
            guard()
        } else {
            vec![]
        };
        steps.extend(guard());
        let (mutation, fresh) = successful_mutation(kind, &body);
        steps.extend(mutation);
        steps.extend(inspection(fresh));
        fixture.scripted_gh(&steps);
        let path = fixture.repo().join("cache.sqlite3");
        let mut cache = Cache::open(&path).unwrap();
        let scope = Scope::github("github.invalid", Some(42)).unwrap();
        let mut sources = vec![];
        for (index, category) in [
            SyncBindingKind::PullRequestBody,
            SyncBindingKind::ConversationComment,
            SyncBindingKind::Review,
        ]
        .into_iter()
        .enumerate()
        {
            let source = format!("collaboration-v1:{index:064x}");
            let token = cache
                .reserve_resource_sync(
                    &scope,
                    &source,
                    &SyncBinding {
                        namespace: SyncNamespace::Collaboration,
                        repository_id: 9,
                        repository: "example/repo".into(),
                        resource: "PR_example/repo_7".into(),
                        kind: category,
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
                        &source,
                        "example/repo",
                        json!({"body":"cached category body"})
                    )],
                    true
                )
                .unwrap());
            sources.push((source, token, category));
        }
        let mut journal = PrWriteJournal::open(&path).unwrap();
        let publication = RefCell::new(None);
        let outcome = execute_legacy(
            &intent,
            &workspace(),
            &BTreeMap::from([("body".into(), body.clone())]),
            &client,
            &mut journal,
            true,
            None,
            |marker| {
                let state: String = Connection::open(&path)
                    .unwrap()
                    .query_row("SELECT state FROM pr_write_attempts", [], |row| row.get(0))
                    .unwrap();
                assert_eq!(state, "pending");
                let _ = publication.replace(Some(marker.publication_reference()));
                Ok(())
            },
            &CancellationToken::default(),
        );
        let PrExecution::Confirmed {
            workspace,
            publication: actual,
            ..
        } = outcome
        else {
            panic!("legacy {kind:?} did not confirm: {outcome:?}");
        };
        assert_eq!(publication.borrow().as_ref(), Some(&actual));
        assert_eq!(workspace.pr.as_ref().unwrap().tab, PrTab::Summary);
        let row: (String, String) = Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT state,confirmed_json FROM pr_write_attempts",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(row.0, "confirmed");
        let receipt: Value = serde_json::from_str(&row.1).unwrap();
        assert_eq!(
            receipt["resource_id"],
            match kind {
                PrFormKind::Comment => "10",
                PrFormKind::Approve | PrFormKind::RequestChanges | PrFormKind::ReviewComment =>
                    "600",
                _ => "PR_example/repo_7",
            }
        );
        assert!(journal
            .blocking(&PrWriteTarget::new(&identity(), &intent.target).unwrap())
            .unwrap()
            .is_none());
        assert_eq!(
            commands(&fixture).len(),
            if kind == PrFormKind::Comment { 22 } else { 19 }
        );
        assert_eq!(write_count(&fixture), 1);
        let changed = match kind {
            PrFormKind::Comment => Some(SyncBindingKind::ConversationComment),
            PrFormKind::Approve | PrFormKind::RequestChanges | PrFormKind::ReviewComment => {
                Some(SyncBindingKind::Review)
            }
            PrFormKind::Title | PrFormKind::Description => Some(SyncBindingKind::PullRequestBody),
            PrFormKind::Base => None,
        };
        for (source, token, category) in &sources {
            assert_eq!(
                cache
                    .binding_status(&scope, source)
                    .unwrap()
                    .unwrap()
                    .visible_generation,
                if Some(*category) == changed {
                    None
                } else {
                    Some(*token)
                },
                "{kind:?} fences only genuine changed text category {category:?}"
            );
        }
        assert!(commands(&fixture).iter().all(|row| row["matched"] == true));
        assert!(matches!(
            execute_legacy(
                &intent,
                &workspace,
                &BTreeMap::from([("body".into(), body)]),
                &client,
                &mut journal,
                true,
                Some(&actual),
                |_| panic!("confirmed original must not publish"),
                &CancellationToken::default()
            ),
            PrExecution::RestoredAlreadyConfirmed { .. }
        ));
        assert_eq!(write_count(&fixture), 1);
    }
}

#[test]
fn conversation_comments_legacy_callback_failure_blocks_before_any_provider_write() {
    for kind in [PrFormKind::Comment, PrFormKind::Approve, PrFormKind::Title] {
        let fixture = Fixture::new();
        let client = provider(&fixture);
        let mut steps = if kind == PrFormKind::Comment {
            guard()
        } else {
            vec![]
        };
        steps.extend(guard());
        fixture.scripted_gh(&steps);
        let path = fixture.repo().join("cache.sqlite3");
        let mut journal = PrWriteJournal::open(&path).unwrap();
        let intent = PrIntent {
            kind,
            target: target(),
            identity: identity(),
        };
        assert!(matches!(
            execute_legacy(
                &intent,
                &workspace(),
                &BTreeMap::from([("body".into(), body(kind))]),
                &client,
                &mut journal,
                true,
                None,
                |_| Err("fixture ordered save failure".into()),
                &CancellationToken::default()
            ),
            PrExecution::Refused { .. }
        ));
        assert_eq!(write_count(&fixture), 0);
        let before = commands(&fixture).len();
        drop(journal);
        let mut reopened = PrWriteJournal::open(&path).unwrap();
        let another = PrIntent {
            kind: PrFormKind::Title,
            ..intent.clone()
        };
        assert!(matches!(
            execute_legacy(
                &another,
                &workspace(),
                &BTreeMap::from([("body".into(), "different action after restart".into())]),
                &client,
                &mut reopened,
                true,
                None,
                |_| panic!("restart block cannot issue a permit"),
                &CancellationToken::default()
            ),
            PrExecution::Refused { .. }
        ));
        assert_eq!(commands(&fixture).len(), before);
        assert!(reopened
            .blocking(&PrWriteTarget::new(&identity(), &intent.target).unwrap())
            .unwrap()
            .is_some());
    }
}

#[test]
fn conversation_comments_legacy_confirmed_refresh_failure_restores_original_head_without_resend() {
    let fixture = Fixture::new();
    let client = provider(&fixture);
    let kind = PrFormKind::ReviewComment;
    let intent = PrIntent {
        kind,
        target: target(),
        identity: identity(),
    };
    let submitted = body(kind);
    let mut steps = guard();
    let (mutation, _) = successful_mutation(kind, &submitted);
    steps.extend(mutation);
    let mut failed = identity_steps();
    failed[1].stdout.clear();
    failed[1].status = 1;
    failed[1].stderr = b"fixture refresh unavailable".to_vec();
    steps.extend(failed);
    fixture.scripted_gh(&steps);
    let path = fixture.repo().join("cache.sqlite3");
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let publication: RefCell<Option<PrPublicationReference>> = RefCell::new(None);
    assert!(matches!(
        execute_legacy(
            &intent,
            &workspace(),
            &BTreeMap::from([("body".into(), submitted.clone())]),
            &client,
            &mut journal,
            true,
            None,
            |marker| {
                let _ = publication.replace(Some(marker.publication_reference()));
                Ok(())
            },
            &CancellationToken::default()
        ),
        PrExecution::ConfirmedButRefreshFailed {
            private_visible: false,
            ..
        }
    ));
    let scope = PrWriteTarget::new(&identity(), &intent.target).unwrap();
    assert!(journal.blocking(&scope).unwrap().is_none());
    drop(journal);
    let mut reopened = PrWriteJournal::open(&path).unwrap();
    let mut changed = intent.clone();
    changed.target.head_sha = "c".repeat(40);
    let mut fresh_workspace = workspace();
    fresh_workspace.pr.as_mut().unwrap().details.summary = changed.target.clone();
    assert!(matches!(
        execute_legacy(
            &changed,
            &fresh_workspace,
            &BTreeMap::from([("body".into(), submitted)]),
            &client,
            &mut reopened,
            true,
            publication.borrow().as_ref(),
            |_| panic!("original restored review must not resend"),
            &CancellationToken::default()
        ),
        PrExecution::RestoredAlreadyConfirmed { .. }
    ));
    assert_eq!(commands(&fixture).len(), 10);
    assert_eq!(write_count(&fixture), 1);
    let marker: String = Connection::open(&path)
        .unwrap()
        .query_row("SELECT state FROM pr_write_attempts", [], |row| row.get(0))
        .unwrap();
    assert_eq!(marker, "confirmed");
}

#[test]
fn conversation_comments_legacy_invalid_or_unconfirmed_intents_send_zero_requests() {
    let fixture = Fixture::new();
    let client = provider(&fixture);
    let mut journal = PrWriteJournal::open(&fixture.repo().join("cache.sqlite3")).unwrap();
    let intent = PrIntent {
        kind: PrFormKind::Title,
        target: target(),
        identity: identity(),
    };
    for (confirmed, values) in [
        (false, BTreeMap::from([("body".into(), "title".into())])),
        (true, BTreeMap::from([("shell".into(), "never run".into())])),
        (true, BTreeMap::from([("body".into(), " ".into())])),
    ] {
        assert!(matches!(
            execute_legacy(
                &intent,
                &workspace(),
                &values,
                &client,
                &mut journal,
                confirmed,
                None,
                |_| panic!("invalid intent cannot reserve a publication"),
                &CancellationToken::default()
            ),
            PrExecution::Refused { .. }
        ));
    }
    let mut wrong = workspace();
    wrong.identity.account_id = Some(43);
    assert!(matches!(
        execute_legacy(
            &intent,
            &wrong,
            &BTreeMap::from([("body".into(), "title".into())]),
            &client,
            &mut journal,
            true,
            None,
            |_| panic!("wrong principal cannot reserve a publication"),
            &CancellationToken::default()
        ),
        PrExecution::Refused { .. }
    ));
    assert!(commands(&fixture).is_empty());
    assert!(journal
        .blocking(&PrWriteTarget::new(&identity(), &intent.target).unwrap())
        .unwrap()
        .is_none());
}

#[test]
fn conversation_comments_legacy_deliberate_fresh_identical_review_is_not_deduplicated() {
    let fixture = Fixture::new();
    let client = provider(&fixture);
    let kind = PrFormKind::ReviewComment;
    let intent = PrIntent {
        kind,
        target: target(),
        identity: identity(),
    };
    let submitted = body(kind);
    let mut steps = guard();
    let (first, fresh) = successful_mutation(kind, &submitted);
    steps.extend(first);
    steps.extend(inspection(fresh));
    steps.extend(guard());
    let (mut second, fresh) = successful_mutation(kind, &submitted);
    for index in [3, 4] {
        let mut value: Value = serde_json::from_slice(&second[index].stdout).unwrap();
        value["id"] = json!(601);
        value["node_id"] = json!("PRR_601");
        second[index].stdout = serde_json::to_vec(&value).unwrap();
    }
    second[4].args[1] = "repos/example/repo/pulls/7/reviews/601".into();
    steps.extend(second);
    steps.extend(inspection(fresh));
    fixture.scripted_gh(&steps);
    let path = fixture.repo().join("cache.sqlite3");
    let mut journal = PrWriteJournal::open(&path).unwrap();
    let values = BTreeMap::from([("body".into(), submitted)]);
    let first = execute_legacy(
        &intent,
        &workspace(),
        &values,
        &client,
        &mut journal,
        true,
        None,
        |_| Ok(()),
        &CancellationToken::default(),
    );
    let PrExecution::Confirmed {
        workspace: updated,
        publication: first_reference,
        ..
    } = first
    else {
        panic!("first review failed: {first:?}");
    };
    let second = execute_legacy(
        &intent,
        &updated,
        &values,
        &client,
        &mut journal,
        true,
        None,
        |_| Ok(()),
        &CancellationToken::default(),
    );
    let PrExecution::Confirmed {
        publication: second_reference,
        ..
    } = second
    else {
        panic!("deliberately fresh identical review must be permitted: {second:?}");
    };
    assert_ne!(first_reference.attempt_id, second_reference.attempt_id);
    assert_eq!(
        first_reference.payload_sha256,
        second_reference.payload_sha256
    );
    let count: u64 = Connection::open(&path)
        .unwrap()
        .query_row(
            "SELECT count(*) FROM pr_write_attempts WHERE state='confirmed'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(count, 2);
    assert_eq!(commands(&fixture).len(), 38);
    assert_eq!(write_count(&fixture), 2);
    assert!(commands(&fixture).iter().all(|row| row["matched"] == true));
}
